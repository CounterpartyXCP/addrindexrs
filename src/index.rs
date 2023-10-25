use bincode;
use bitcoin::blockdata::block::{Block, BlockHeader};
use bitcoin::blockdata::transaction::{Transaction, TxIn, TxOut};
use bitcoin::consensus::encode::{deserialize, serialize};
use bitcoin::util::hash::BitcoinHash;
use bitcoin_hashes::sha256d::Hash as Sha256dHash;
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use std::collections::HashSet;
use std::sync::RwLock;

use crate::daemon::Daemon;
use crate::errors::*;
use crate::signal::Waiter;
use crate::store::{ReadStore, Row, WriteStore};
use crate::util::{
    full_hash, hash_prefix, spawn_thread, Bytes,
    FullHash, HashPrefix, HeaderEntry, HeaderList,
    HeaderMap, SyncChannel, HASH_PREFIX_LEN,
};

//
// Key of a row storing an input of a transaction
//
#[derive(Serialize, Deserialize)]
pub struct TxInKey {
    pub code: u8,
    pub prev_txid_prefix: HashPrefix,
    pub prev_vout: u16,
}

//
// Row storing an input of a transaction
//
#[derive(Serialize, Deserialize)]
pub struct TxInRow {
    key: TxInKey,
    pub txid_prefix: HashPrefix,
}

impl TxInRow {
    pub fn new(txid: &Sha256dHash, input: &TxIn) -> TxInRow {
        TxInRow {
            key: TxInKey {
                code: b'I',
                prev_txid_prefix: hash_prefix(&input.previous_output.txid[..]),
                prev_vout: input.previous_output.vout as u16,
            },
            txid_prefix: hash_prefix(&txid[..]),
        }
    }

    pub fn filter(txid: &Sha256dHash, vout: usize) -> Bytes {
        bincode::serialize(&TxInKey {
            code: b'I',
            prev_txid_prefix: hash_prefix(&txid[..]),
            prev_vout: vout as u16,
        })
        .unwrap()
    }

    pub fn to_row(&self) -> Row {
        Row {
            key: bincode::serialize(&self).unwrap(),
            value: vec![],
        }
    }

    pub fn from_row(row: &Row) -> TxInRow {
        bincode::deserialize(&row.key).expect("failed to parse TxInRow")
    }
}

//
// Key of a row storing an output of a transaction
//
#[derive(Serialize, Deserialize)]
pub struct TxOutKey {
    code: u8,
    script_hash_prefix: HashPrefix,
}

//
// Row storing an output of a transaction
//
#[derive(Serialize, Deserialize)]
pub struct TxOutRow {
    key: TxOutKey,
    pub txid_prefix: HashPrefix,
    pub vout: u16,
}

impl TxOutRow {
    pub fn new(txid: &Sha256dHash, vout: u32, output: &TxOut) -> TxOutRow {
        TxOutRow {
            key: TxOutKey {
                code: b'O',
                script_hash_prefix: hash_prefix(&compute_script_hash(&output.script_pubkey[..])),
            },
            txid_prefix: hash_prefix(&txid[..]),
            vout: vout as u16,
        }
    }

    pub fn filter(script_hash: &[u8]) -> Bytes {
        bincode::serialize(&TxOutKey {
            code: b'O',
            script_hash_prefix: hash_prefix(&script_hash[..HASH_PREFIX_LEN]),
        })
        .unwrap()
    }

    pub fn to_row(&self) -> Row {
        Row {
            key: bincode::serialize(&self).unwrap(),
            value: vec![],
        }
    }

    pub fn from_row(row: &Row) -> TxOutRow {
        bincode::deserialize(&row.key).expect("failed to parse TxOutRow")
    }
}

//
// Key of a row storing a transaction
//
#[derive(Serialize, Deserialize)]
pub struct TxKey {
    code: u8,
    pub txid: FullHash,
}

//
// Row storing a transaction
//
#[derive(Serialize, Deserialize)]
pub struct TxRow {
    pub key: TxKey,
    pub block_hash: FullHash
}

impl TxRow {
    pub fn new(txid: &Sha256dHash, blockhash: &Sha256dHash) -> TxRow {
        TxRow {
            key: TxKey {
                code: b'T',
                txid: full_hash(&txid[..]),             
            },
            block_hash: full_hash(&blockhash)
        }
    }

    pub fn filter_prefix(txid_prefix: HashPrefix) -> Bytes {
        [b"T", &txid_prefix[..]].concat()
    }

    pub fn filter_full(txid: &Sha256dHash) -> Bytes {
        [b"T", &txid[..]].concat()
    }

    pub fn to_row(&self) -> Row {
        Row {
            key: bincode::serialize(&self.key).unwrap(),
            value: bincode::serialize(&self.block_hash).unwrap(),
        }
    }

    pub fn from_row(row: &Row) -> TxRow {
        TxRow {
            key:bincode::deserialize(&row.key).expect("failed to parse TxRow"),
            block_hash:bincode::deserialize(&row.value).expect("failed to parse TxRow")
        }
    }
}

//
// Key of a row storing a block
//
#[derive(Serialize, Deserialize)]
struct BlockKey {
    code: u8,
    hash: FullHash,
}

//
// Compute the script hash of a scriptpubkey
//
pub fn compute_script_hash(data: &[u8]) -> FullHash {
    let mut hash = FullHash::default();
    let mut sha2 = Sha256::new();
    sha2.input(data);
    sha2.result(&mut hash);
    hash
}

//
// Index a transaction
//
pub fn index_transaction<'a>(txn: &'a Transaction, blockhash: &Sha256dHash) -> impl 'a + Iterator<Item = Row> {
    let null_hash = Sha256dHash::default();
    let txid: Sha256dHash = txn.txid();

    let inputs = txn.input.iter().filter_map(move |input| {
        if input.previous_output.txid == null_hash {
            None
        } else {
            Some(TxInRow::new(&txid, &input).to_row())
        }
    });

    let outputs = txn
        .output
        .iter()
        .enumerate()
        .map(move |(vout, output)| TxOutRow::new(&txid, vout as u32, &output).to_row());

    inputs
        .chain(outputs)
        .chain(std::iter::once(TxRow::new(&txid, &blockhash).to_row()))
}

//
// Index a block
//
pub fn index_block<'a>(block: &'a Block) -> impl 'a + Iterator<Item = Row> {
    let blockhash = block.bitcoin_hash();
    // Persist block hash and header
    let row = Row {
        key: bincode::serialize(&BlockKey {
            code: b'B',
            hash: full_hash(&blockhash[..]),
        })
        .unwrap(),
        value: serialize(&block.header),
    };
    block
        .txdata
        .iter()
        .flat_map(move |txn| index_transaction(&txn, &blockhash))
        .chain(std::iter::once(row))
}

//
// Retrieve the last indexed block
//
pub fn last_indexed_block(blockhash: &Sha256dHash) -> Row {
    // Store last indexed block (i.e. all previous blocks were indexed)
    Row {
        key: b"L".to_vec(),
        value: serialize(blockhash),
    }
}

//
// Retrieve the hashes of all the indexed blocks
//
pub fn read_indexed_blockhashes(store: &dyn ReadStore) -> HashSet<Sha256dHash> {
    let mut result = HashSet::new();
    for row in store.scan(b"B") {
        let key: BlockKey = bincode::deserialize(&row.key).unwrap();
        result.insert(deserialize(&key.hash).unwrap());
    }
    result
}

//
// Retrieve the headers of all the indexed blocks
//
fn read_indexed_headers(store: &dyn ReadStore) -> HeaderList {
    let latest_blockhash: Sha256dHash = match store.get(b"L") {
        // latest blockheader persisted in the DB.
        Some(row) => deserialize(&row).unwrap(),
        None => Sha256dHash::default(),
    };
    trace!("lastest indexed blockhash: {}", latest_blockhash);

    let mut map = HeaderMap::new();
    for row in store.scan(b"B") {
        let key: BlockKey = bincode::deserialize(&row.key).unwrap();
        let header: BlockHeader = deserialize(&row.value).unwrap();
        map.insert(deserialize(&key.hash).unwrap(), header);
    }

    let mut headers = vec![];
    let null_hash = Sha256dHash::default();
    let mut blockhash = latest_blockhash;

    while blockhash != null_hash {
        let header = map
            .remove(&blockhash)
            .unwrap_or_else(|| panic!("missing {} header in DB", blockhash));
        blockhash = header.prev_blockhash;
        headers.push(header);
    }

    headers.reverse();

    assert_eq!(
        headers
            .first()
            .map(|h| h.prev_blockhash)
            .unwrap_or(null_hash),
        null_hash
    );

    assert_eq!(
        headers
            .last()
            .map(BitcoinHash::bitcoin_hash)
            .unwrap_or(null_hash),
        latest_blockhash
    );

    let mut result = HeaderList::empty();
    let entries = result.order(headers);
    result.apply(entries, latest_blockhash);
    result
}

//
// Indexer
//
pub struct Index {
    // TODO: store also latest snapshot.
    headers: RwLock<HeaderList>,
    daemon: Daemon,
    batch_size: usize,
}

impl Index {
    pub fn load(
        store: &dyn ReadStore,
        daemon: &Daemon,
        batch_size: usize,
    ) -> Result<Index> {
        let headers = read_indexed_headers(store);
        Ok(Index {
            headers: RwLock::new(headers),
            daemon: daemon.reconnect()?,
            batch_size,
        })
    }

    pub fn reload(&self, store: &dyn ReadStore) {
        let mut headers = self.headers.write().unwrap();
        *headers = read_indexed_headers(store);
    }

    pub fn best_header(&self) -> Option<HeaderEntry> {
        let headers = self.headers.read().unwrap();
        headers.header_by_blockhash(&headers.tip()).cloned()
    }

    pub fn get_header(&self, height: usize) -> Option<HeaderEntry> {
        self.headers
            .read()
            .unwrap()
            .header_by_height(height)
            .cloned()
    }

    pub fn get_header_by_block_hash(&self, block_hash: Sha256dHash) -> Option<HeaderEntry> {
        self.headers
            .read()
            .unwrap()
            .header_by_blockhash(&block_hash)
            .cloned()
    }

    pub fn update(&self, store: &impl WriteStore, waiter: &Waiter) -> Result<Sha256dHash> {
        let daemon = self.daemon.reconnect()?;
        let tip = daemon.getbestblockhash()?;

        let new_headers: Vec<HeaderEntry> = {
            let indexed_headers = self.headers.read().unwrap();
            indexed_headers.order(daemon.get_new_headers(&indexed_headers, &tip)?)
        };

        if let Some(latest_header) = new_headers.last() {
            info!("{:?} ({} left to index)", latest_header, new_headers.len());
        };

        let chan = SyncChannel::new(1);
        let sender = chan.sender();
        let blockhashes: Vec<Sha256dHash> = new_headers.iter().map(|h| *h.hash()).collect();
        let batch_size = self.batch_size;

        let fetcher = spawn_thread("fetcher", move || {
            for chunk in blockhashes.chunks(batch_size) {
                sender
                    .send(daemon.getblocks(&chunk))
                    .expect("failed sending blocks to be indexed");
            }
            sender
                .send(Ok(vec![]))
                .expect("failed sending explicit end of stream");
        });

        loop {
            waiter.poll()?;

            let batch = chan
                .receiver()
                .recv()
                .expect("block fetch exited prematurely")?;

            if batch.is_empty() {
                break;
            }

            let rows_iter = batch.iter().flat_map(|block| {
                let blockhash = block.bitcoin_hash();
                info!("indexing block {}", blockhash);
                index_block(block).chain(std::iter::once(last_indexed_block(&blockhash)))
            });

            store.write(rows_iter);
        }

        store.flush(); // make sure no row is left behind
        fetcher.join().expect("block fetcher failed");
        self.headers.write().unwrap().apply(new_headers, tip);
        assert_eq!(tip, self.headers.read().unwrap().tip());
        Ok(tip)
    }
}
