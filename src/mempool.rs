use bitcoin::blockdata::transaction::Transaction;
use bitcoin_hashes::sha256d::Hash as Sha256dHash;
use hex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::iter::FromIterator;
use std::ops::Bound;

use crate::daemon::Daemon;
use crate::errors::*;
use crate::index::index_transaction;
use crate::store::{ReadStore, Row};
use crate::util::Bytes;

//
// BTree emulating a db store
// for mempool transactions
//
struct MempoolStore {
    map: BTreeMap<Bytes, Vec<Bytes>>,
}

impl MempoolStore {
    fn new() -> MempoolStore {
        MempoolStore {
            map: BTreeMap::new(),
        }
    }

    fn add(&mut self, tx: &Transaction) {
        let rows = index_transaction(tx, &Sha256dHash::default());
        for row in rows {
            let (key, value) = row.into_pair();
            self.map.entry(key).or_insert_with(|| vec![]).push(value);
        }
    }

    fn remove(&mut self, tx: &Transaction) {
        let rows = index_transaction(tx, &Sha256dHash::default());
        for row in rows {
            let (key, value) = row.into_pair();
            let no_values_left = {
                let values = self
                    .map
                    .get_mut(&key)
                    .unwrap_or_else(|| panic!("missing key {} in mempool", hex::encode(&key)));
                let last_value = values
                    .pop()
                    .unwrap_or_else(|| panic!("no values found for key {}", hex::encode(&key)));
                // TxInRow and TxOutRow have an empty value, TxRow has height=0 as value.
                assert_eq!(
                    value,
                    last_value,
                    "wrong value for key {}: {}",
                    hex::encode(&key),
                    hex::encode(&last_value)
                );
                values.is_empty()
            };
            if no_values_left {
                self.map.remove(&key).unwrap();
            }
        }
    }
}

impl ReadStore for MempoolStore {
    fn get(&self, key: &[u8]) -> Option<Bytes> {
        Some(self.map.get(key)?.last()?.to_vec())
    }

    fn scan(&self, prefix: &[u8]) -> Vec<Row> {
        let range = self
            .map
            .range((Bound::Included(prefix.to_vec()), Bound::Unbounded));
        let mut rows = vec![];
        for (key, values) in range {
            if !key.starts_with(prefix) {
                break;
            }
            if let Some(value) = values.last() {
                rows.push(Row {
                    key: key.to_vec(),
                    value: value.to_vec(),
                });
            }
        }
        rows
    }
}

//
// Tracker managing mempool transactions
//
pub struct Tracker {
    items: HashMap<Sha256dHash, Transaction>,
    index: MempoolStore,
}

impl Tracker {
    pub fn new() -> Tracker {
        Tracker {
            items: HashMap::new(),
            index: MempoolStore::new(),
        }
    }

    pub fn index(&self) -> &dyn ReadStore {
        &self.index
    }

    pub fn update(&mut self, daemon: &Daemon) -> Result<()> {
        let new_txids = daemon
            .getmempooltxids()
            .chain_err(|| "failed to update mempool from daemon")?;

        let old_txids = HashSet::from_iter(self.items.keys().cloned());

        let txids_iter = new_txids.difference(&old_txids);

        let txids: Vec<&Sha256dHash> = txids_iter.collect();

        let txs = match daemon.gettransactions(&txids) {
            Ok(txs) => txs,
            Err(err) => {
                warn!("failed to get transactions {:?}: {}", txids, err); // e.g. new block or RBF
                return Ok(()); // keep the mempool until next update()
            }
        };

        trace!("updated mempool with {} transactions from daemon", txs.len());

        for (txid, tx) in txids.into_iter().zip(txs.into_iter()) {
            assert_eq!(tx.txid(), *txid);
            self.add(txid, tx);
        }

        for txid in old_txids.difference(&new_txids) {
            self.remove(txid);
        }

        Ok(())
    }

    fn add(&mut self, txid: &Sha256dHash, tx: Transaction) {
        self.index.add(&tx);
        self.items.insert(*txid, tx);
    }

    fn remove(&mut self, txid: &Sha256dHash) {
        let tx = self
            .items
            .remove(txid)
            .unwrap_or_else(|| panic!("missing mempool tx {}", txid));
        self.index.remove(&tx);
    }
}
