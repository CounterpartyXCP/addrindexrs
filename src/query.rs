use bitcoin::consensus::encode::deserialize;
use bitcoin_hashes::sha256d::Hash as Sha256dHash;
use std::sync::{Arc, RwLock};

use crate::app::App;
use crate::errors::*;
use crate::index::{TxInRow, TxOutRow, TxRow};
use crate::mempool::Tracker;
use crate::store::ReadStore;
use crate::util::HashPrefix;

//
// Output of a Transaction
//
pub struct Txo {
    pub txid: Sha256dHash,
    pub vout: usize,
}

//
// Input of a Transaction
//
type OutPoint = (Sha256dHash, usize); // (txid, vout)

struct SpendingInput {
    txid: Sha256dHash,
    outpoint: OutPoint,
}

//
// Status of an Address
// (vectors of confirmed and unconfirmed outputs and inputs)
//
pub struct Status {
    confirmed: (Vec<Txo>, Vec<SpendingInput>),
    mempool: (Vec<Txo>, Vec<SpendingInput>),
}

impl Status {
    fn funding(&self) -> impl Iterator<Item = &Txo> {
        self.confirmed.0.iter().chain(self.mempool.0.iter())
    }

    fn spending(&self) -> impl Iterator<Item = &SpendingInput> {
        self.confirmed.1.iter().chain(self.mempool.1.iter())
    }

    pub fn history(&self) -> Vec<Sha256dHash> {
        let mut txns = vec![];
        for f in self.funding() {
            txns.push(f.txid);
        }
        for s in self.spending() {
            txns.push(s.txid);
        }
        txns.sort_unstable();
        txns
    }
}

//
// QUery tool for the indexer
//
pub struct Query {
    app: Arc<App>,
    tracker: RwLock<Tracker>,
    txid_limit: usize,
}

impl Query {
    pub fn new(
        app: Arc<App>,
        txid_limit: usize,
    ) -> Arc<Query> {
        Arc::new(Query {
            app,
            tracker: RwLock::new(Tracker::new()),
            txid_limit,
        })
    }

    fn get_txrows_by_prefix(
        &self,
        store: &dyn ReadStore,
        prefix: HashPrefix
    ) -> Vec<TxRow> {
        store
            .scan(&TxRow::filter_prefix(prefix))
            .iter()
            .map(|row| TxRow::from_row(row))
            .collect()
    }

    fn get_txoutrows_by_script_hash(
        &self,
        store: &dyn ReadStore,
        script_hash: &[u8]
    ) -> Vec<TxOutRow> {
        store
            .scan(&TxOutRow::filter(script_hash))
            .iter()
            .map(|row| TxOutRow::from_row(row))
            .collect()
    }

    fn get_prefixes_by_funding_txo(
        &self,
        store: &dyn ReadStore,
        txid: &Sha256dHash,
        vout: usize,
    ) -> Vec<HashPrefix> {
        store
            .scan(&TxInRow::filter(&txid, vout))
            .iter()
            .map(|row| TxInRow::from_row(row).txid_prefix)
            .collect()
    }

    fn get_txids_by_prefix(
        &self,
        store: &dyn ReadStore,
        prefixes: Vec<HashPrefix>,
    ) -> Result<Vec<Sha256dHash>> {
        let mut txns = vec![];
        for prefix in prefixes {
            for tx_row in self.get_txrows_by_prefix(store, prefix) {
                let txid: Sha256dHash = deserialize(&tx_row.key.txid).unwrap();
                txns.push(txid)
            }
        }
        Ok(txns)
    }

    fn find_spending_input(
        &self,
        store: &dyn ReadStore,
        txo: &Txo,
    ) -> Result<Option<SpendingInput>> {

        let mut spendings = vec![];
        let prefixes = self.get_prefixes_by_funding_txo(store, &txo.txid, txo.vout);
        let txids = self.get_txids_by_prefix(store, prefixes)?;

        for txid in &txids {
            spendings.push(SpendingInput {
                txid: *txid,
                outpoint: (txo.txid, txo.vout),
            })
        }

        assert!(spendings.len() <= 1);

        Ok(if spendings.len() == 1 {
            Some(spendings.remove(0))
        } else {
            None
        })
    }

    fn find_funding_outputs(
        &self,
        store: &dyn ReadStore,
        script_hash: &[u8]
    ) -> Result<Vec<Txo>> {
        let txout_rows = self.get_txoutrows_by_script_hash(store, script_hash);

        let mut result = vec![];

        for row in &txout_rows {
            let txids = self.get_txids_by_prefix(store, vec![row.txid_prefix])?;
            for txid in &txids {
                result.push(Txo {
                    txid: *txid,
                    vout: row.vout as usize,
                })
            }
        }

        Ok(result)
    }

    fn confirmed_status(
        &self,
        script_hash: &[u8],
    ) -> Result<(Vec<Txo>, Vec<SpendingInput>)> {
        let mut funding = vec![];
        let mut spending = vec![];
        let read_store = self.app.read_store();

        let txos = self.find_funding_outputs(read_store, script_hash)?;
        if self.txid_limit > 0 && txos.len() > self.txid_limit {
            bail!(
                "{}+ transactions found, query may take a long time",
                txos.len()
            );
        }
        funding.extend(txos);

        for txo in &funding {
            if let Some(spent) = self.find_spending_input(read_store, &txo)? {
                spending.push(spent);
            }
        }

        Ok((funding, spending))
    }

    fn mempool_status(
        &self,
        script_hash: &[u8],
        confirmed_funding: &[Txo],
    ) -> Result<(Vec<Txo>, Vec<SpendingInput>)> {
        let mut funding = vec![];
        let mut spending = vec![];

        let tracker = self.tracker.read().unwrap();

        let txos = self.find_funding_outputs(tracker.index(), script_hash)?;
        if self.txid_limit > 0 && txos.len() > self.txid_limit {
            bail!(
                "{}+ transactions found, query may take a long time",
                txos.len()
            );
        }
        funding.extend(txos);

        for txo in funding.iter().chain(confirmed_funding.iter()) {
            if let Some(spent) = self.find_spending_input(tracker.index(), &txo)? {
                spending.push(spent);
            }
        }

        Ok((funding, spending))
    }

    pub fn status(&self, script_hash: &[u8]) -> Result<Status> {
        let confirmed = self
            .confirmed_status(script_hash)
            .chain_err(|| "failed to get confirmed status")?;

        let mempool = self
            .mempool_status(script_hash, &confirmed.0)
            .chain_err(|| "failed to get mempool status")?;

        Ok(Status { confirmed, mempool })
    }

    pub fn update_mempool(&self) -> Result<()> {
        self.tracker.write().unwrap().update(self.app.daemon())
    }
}
