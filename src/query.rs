use bitcoin::consensus::encode::deserialize;
use bitcoin_hashes::sha256d::Hash as Sha256dHash;
use std::sync::{Arc, RwLock};

use crate::app::App;
use crate::errors::*;
use crate::index::{TxInRow, TxOutRow, TxRow};
use crate::mempool::Tracker;
use crate::store::ReadStore;
use crate::util::{HashPrefix, HeaderEntry};

//
// Output of a Transaction
//
pub struct Txo {
    pub txid: Sha256dHash,
    pub vout: usize,
    pub blockindex: usize
}

//
// Input of a Transaction
//
pub type OutPoint = (Sha256dHash, usize); // (txid, vout)

pub struct SpendingInput {
    pub txid: Sha256dHash,
    pub outpoint: OutPoint,
    pub blockindex: usize
}


pub struct TxBlockIndex {
    pub txid: Sha256dHash,
    pub blockindex: usize
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
    pub fn funding(&self) -> impl Iterator<Item = &Txo> {
        self.confirmed.0.iter().chain(self.mempool.0.iter())
    }

    pub fn spending(&self) -> impl Iterator<Item = &SpendingInput> {
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
        txns.dedup();
        txns
    }
    
    pub fn oldest(&self) -> Option<TxBlockIndex> {
        let mut min_found = false;
        let mut min_block_index = 0;
        let mut min_tx : Option<TxBlockIndex> = None;
            
        for f in self.funding() {
            if !min_found || (min_block_index == 0 && f.blockindex > 0) || f.blockindex < min_block_index {
                min_block_index = f.blockindex;
                min_tx = Some(TxBlockIndex{
                        txid: f.txid,
                        blockindex: f.blockindex
                    }
                );
                min_found = true;
            }
        }
    
        for f in self.spending() {
            if !min_found || (min_block_index == 0 && f.blockindex > 0) || f.blockindex < min_block_index {
                min_block_index = f.blockindex;
                min_tx = Some(TxBlockIndex{
                        txid: f.txid,
                        blockindex: f.blockindex
                    }
                );
                min_found = true;
            }
        }
        
        min_tx
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

    /*fn get_txids_by_prefix(
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
    }*/

    fn get_txrows_by_prefixes(
        &self,
        store: &dyn ReadStore,
        prefixes: Vec<HashPrefix>,
    ) -> Result<Vec<TxRow>> {
        let mut txns = vec![];
        for prefix in prefixes {
            for tx_row in self.get_txrows_by_prefix(store, prefix) {
                txns.push(tx_row)
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
        //let txids = self.get_txids_by_prefix(store, prefixes)?;
        let txrows = self.get_txrows_by_prefixes(store, prefixes)?;

        for txrow in &txrows {
            let block_index = match self.get_block_index(deserialize(&txrow.block_hash).unwrap()){
                Ok(header) => header.height(),
                Err(_error) => 0
            };
    
            spendings.push(SpendingInput {
                txid: deserialize(&txrow.key.txid).unwrap(),
                outpoint: (txo.txid, txo.vout),
                blockindex: block_index
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
            //let txids = self.get_txids_by_prefix(store, vec![row.txid_prefix])?;
            let txrows = self.get_txrows_by_prefixes(store, vec![row.txid_prefix])?;
            
            for txrow in &txrows {
                let block_index = match self.get_block_index(deserialize(&txrow.block_hash).unwrap()){
                    Ok(header) => header.height(),
                    Err(_error) => 0
                };
                    
                result.push(Txo {
                    txid: deserialize(&txrow.key.txid).unwrap(),
                    vout: row.vout as usize,
                    blockindex: block_index
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
    
    pub fn oldest_tx(&self, script_hash: &[u8]) -> Result<TxBlockIndex> {
        let all_status = self.status(script_hash).unwrap();
        
        all_status.oldest().chain_err(|| "no txs for address")
    }
    
    pub fn get_best_header(&self) -> Result<HeaderEntry> {
        let last_header = self.app.index().best_header();
        Ok(last_header.chain_err(|| "no headers indexed")?)
    }
    
    pub fn get_block_index(&self, block_hash:Sha256dHash) -> Result<HeaderEntry> {
        let block_header = self.app.index().get_header_by_block_hash(block_hash);
        Ok(block_header.chain_err(|| "no headers indexed")?)
    }

    pub fn update_mempool(&self) -> Result<()> {
        self.tracker.write().unwrap().update(self.app.daemon())
    }
}
