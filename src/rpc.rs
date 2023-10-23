use bitcoin::consensus::encode::serialize;
use bitcoin_hashes::hex::{FromHex, ToHex};
use bitcoin_hashes::sha256d::Hash as Sha256dHash;
use error_chain::ChainedError;
use serde_json::{from_str, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::errors::*;
use crate::query::Query;
use crate::util::{spawn_thread, Channel, SyncChannel};

// Indexer version
const ADDRINDEXRS_VERSION: &str = env!("CARGO_PKG_VERSION");
// Version of the simulated electrum protocol
const PROTOCOL_VERSION: &str = "1.4";

//
// Get a script hash from a given value
//
fn hash_from_value(val: Option<&Value>) -> Result<Sha256dHash> {
    // TODO: Sha256dHash should be a generic hash-container (since script hash is single SHA256)
    let script_hash = val.chain_err(|| "missing hash")?;
    let script_hash = script_hash.as_str().chain_err(|| "non-string hash")?;
    let script_hash = Sha256dHash::from_hex(script_hash).chain_err(|| "non-hex hash")?;
    Ok(script_hash)
}

//
// Connection with a RPC client
//
struct Connection {
    query: Arc<Query>,
    stream: TcpStream,
    addr: SocketAddr,
    chan: SyncChannel<Message>,
}

impl Connection {
    pub fn new(
        query: Arc<Query>,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Connection {
        Connection {
            query,
            stream,
            addr,
            chan: SyncChannel::new(10),
        }
    }

    fn server_version(&self) -> Result<Value> {
        Ok(json!([
            format!("addrindexrs {}", ADDRINDEXRS_VERSION),
            PROTOCOL_VERSION
        ]))
    }

    fn blockchain_headers_subscribe(&mut self) -> Result<Value> {
        let entry = self.query.get_best_header()?;
        let hex_header = hex::encode(serialize(entry.header()));
        let result = json!({"hex": hex_header, "height": entry.height()});
        Ok(result)
    }

    fn blockchain_scripthash_get_balance(&self, _params: &[Value]) -> Result<Value> {
        Ok(
            json!({ "confirmed": null, "unconfirmed": null }),
        )
    }

    fn blockchain_scripthash_get_history(&self, params: &[Value]) -> Result<Value> {
        let script_hash = hash_from_value(params.get(0)).chain_err(|| "bad script_hash")?;
        let status = self.query.status(&script_hash[..])?;
        Ok(json!(Value::Array(
            status
                .history()
                .into_iter()
                .map(|item| json!({"tx_hash": item.to_hex()}))
                .collect()
        )))
    }

    fn blockchain_scripthash_get_oldest_tx(&self, params: &[Value]) -> Result<Value> {
        let script_hash = hash_from_value(params.get(0)).chain_err(|| "bad script_hash")?;
        let oldest_tx = self.query.oldest_tx(&script_hash[..])?;
        Ok(json!({"tx_hash":oldest_tx.txid.to_hex(),"block_index":oldest_tx.blockindex}))
    }

    fn blockchain_scripthash_get_utxos(&self, params: &[Value]) -> Result<Value> {
        let script_hash = hash_from_value(params.get(0)).chain_err(|| "bad script_hash")?;
        let status = self.query.status(&script_hash[..])?;

        let mut dict = HashMap::new();
        for item in status.funding().into_iter() {
            dict.insert(item.txid.to_hex() + ":" + &item.vout.to_string(), "");
        }

        for item in status.spending().into_iter() {
            dict.remove( &(item.outpoint.0.to_hex() + ":" + &item.outpoint.1.to_string()) );
        }

        let mut utxos = vec![];
        for (outpoint, _drop) in &dict {
            utxos.push(outpoint)
        }

        Ok(json!(utxos))
    }

    fn handle_command(&mut self, method: &str, params: &[Value], id: &Value) -> Result<Value> {
        let result = match method {
            "blockchain.headers.subscribe" => self.blockchain_headers_subscribe(),
            "blockchain.scripthash.get_balance" => self.blockchain_scripthash_get_balance(&params),
            "blockchain.scripthash.get_history" => self.blockchain_scripthash_get_history(&params),
            "blockchain.scripthash.get_oldest_tx" => self.blockchain_scripthash_get_oldest_tx(&params),
            "blockchain.scripthash.get_utxos" => self.blockchain_scripthash_get_utxos(&params),
            "server.ping" => Ok(Value::Null),
            "server.version" => self.server_version(),
            &_ => bail!("unknown method {} {:?}", method, params),
        };
        // TODO: return application errors should be sent to the client
        Ok(match result {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(e) => {
                warn!(
                    "rpc #{} {} {:?} failed: {}",
                    id,
                    method,
                    params,
                    e.display_chain()
                );
                json!({"jsonrpc": "2.0", "id": id, "error": format!("{}", e)})
            }
        })
    }

    fn send_values(&mut self, values: &[Value]) -> Result<()> {
        for value in values {
            let line = value.to_string() + "\n";
            self.stream
                .write_all(line.as_bytes())
                .chain_err(|| format!("failed to send {}", value))?;
        }
        Ok(())
    }

    fn handle_replies(&mut self) -> Result<()> {
        let empty_params = json!([]);
        loop {
            let msg = self.chan.receiver().recv().chain_err(|| "channel closed")?;
            trace!("RPC {:?}", msg);
            match msg {
                Message::Request(line) => {
                    let cmd: Value = from_str(&line).chain_err(|| "invalid JSON format")?;
                    let reply = match (
                        cmd.get("method"),
                        cmd.get("params").unwrap_or_else(|| &empty_params),
                        cmd.get("id"),
                    ) {
                        (
                            Some(&Value::String(ref method)),
                            &Value::Array(ref params),
                            Some(ref id),
                        ) => self.handle_command(method, params, id)?,
                        _ => bail!("invalid command: {}", cmd),
                    };
                    self.send_values(&[reply])?
                }
                Message::Done => return Ok(()),
            }
        }
    }

    fn handle_requests(mut reader: BufReader<TcpStream>, tx: SyncSender<Message>) -> Result<()> {
        loop {
            let mut line = Vec::<u8>::new();
            reader
                .read_until(b'\n', &mut line)
                .chain_err(|| "failed to read a request")?;
            if line.is_empty() {
                tx.send(Message::Done).chain_err(|| "channel closed")?;
                return Ok(());
            } else {
                if line.starts_with(&[22, 3, 1]) {
                    // (very) naive SSL handshake detection
                    let _ = tx.send(Message::Done);
                    bail!("invalid request - maybe SSL-encrypted data?: {:?}", line)
                }
                match String::from_utf8(line) {
                    Ok(req) => tx
                        .send(Message::Request(req))
                        .chain_err(|| "channel closed")?,
                    Err(err) => {
                        let _ = tx.send(Message::Done);
                        bail!("invalid UTF8: {}", err)
                    }
                }
            }
        }
    }

    pub fn run(mut self) {
        let reader = BufReader::new(self.stream.try_clone().expect("failed to clone TcpStream"));
        let tx = self.chan.sender();
        let child = spawn_thread("reader", || Connection::handle_requests(reader, tx));
        if let Err(e) = self.handle_replies() {
            error!(
                "[{}] connection handling failed: {}",
                self.addr,
                e.display_chain().to_string()
            );
        }
        debug!("[{}] shutting down connection", self.addr);
        let _ = self.stream.shutdown(Shutdown::Both);
        if let Err(err) = child.join().expect("receiver panicked") {
            error!("[{}] receiver failed: {}", self.addr, err);
        }
    }
}

//
// Messages supported by the RPC API
//
#[derive(Debug)]
pub enum Message {
    Request(String),
    Done,
}

//
// RPC server
//
pub struct RPC {
    server: Option<thread::JoinHandle<()>>, // so we can join the server while dropping this ojbect
}

impl RPC {
    fn start_acceptor(addr: SocketAddr) -> Channel<Option<(TcpStream, SocketAddr)>> {
        let chan = Channel::unbounded();
        let acceptor = chan.sender();
        spawn_thread("acceptor", move || {
            let listener =
                TcpListener::bind(addr).unwrap_or_else(|e| panic!("bind({}) failed: {}", addr, e));
            info!(
                "Indexer RPC server running on {} (protocol {})",
                addr, PROTOCOL_VERSION
            );
            loop {
                let (stream, addr) = listener.accept().expect("accept failed");
                stream
                    .set_nonblocking(false)
                    .expect("failed to set connection as blocking");
                acceptor.send(Some((stream, addr))).expect("send failed");
            }
        });
        chan
    }

    pub fn start(addr: SocketAddr, query: Arc<Query>) -> RPC {
        RPC {
            server: Some(spawn_thread("rpc", move || {
                let senders = Arc::new(Mutex::new(HashMap::<i32, SyncSender<Message>>::new()));
                let handles = Arc::new(Mutex::new(
                    HashMap::<i32, std::thread::JoinHandle<()>>::new(),
                ));

                let acceptor = RPC::start_acceptor(addr);
                let mut handle_count = 0;

                while let Some((stream, addr)) = acceptor.receiver().recv().unwrap() {
                    let handle_id = handle_count;
                    handle_count += 1;
                    // explicitely scope the shadowed variables for the new thread
                    let handle: thread::JoinHandle<()> = {
                        let query = Arc::clone(&query);
                        let senders = Arc::clone(&senders);
                        let handles = Arc::clone(&handles);

                        spawn_thread("peer", move || {
                            info!("[{}] connected peer #{}", addr, handle_id);
                            let conn = Connection::new(query, stream, addr);
                            senders
                                .lock()
                                .unwrap()
                                .insert(handle_id, conn.chan.sender());
                            conn.run();
                            info!("[{}] disconnected peer #{}", addr, handle_id);
                            senders.lock().unwrap().remove(&handle_id);
                            handles.lock().unwrap().remove(&handle_id);
                        })
                    };

                    handles.lock().unwrap().insert(handle_id, handle);
                }

                trace!("closing {} RPC connections", senders.lock().unwrap().len());
                for sender in senders.lock().unwrap().values() {
                    let _ = sender.send(Message::Done);
                }

                trace!("waiting for {} RPC handling threads", handles.lock().unwrap().len());
                for (_, handle) in handles.lock().unwrap().drain() {
                    if let Err(e) = handle.join() {
                        warn!("failed to join thread: {:?}", e);
                    }
                }

                trace!("RPC connections are closed");
            })),
        }
    }
}

impl Drop for RPC {
    fn drop(&mut self) {
        trace!("stop accepting new RPCs");
        if let Some(handle) = self.server.take() {
            handle.join().unwrap();
        }
        trace!("RPC server is stopped");
    }
}
