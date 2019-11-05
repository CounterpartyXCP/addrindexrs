use bitcoin::consensus::encode::deserialize;
use bitcoin_hashes::sha256d::Hash as Sha256dHash;
use std::sync::{Arc, RwLock};

use crate::app::App;
use crate::errors::*;
use crate::index::{TxInRow, TxOutRow, TxRow};
use crate::mempool::Tracker;
use crate::store::ReadStore;
use crate::util::HashPrefix;


pub struct FundingOutput {
    pub txn_id: Sha256dHash,
    pub output_index: usize,
}

type OutPoint = (Sha256dHash, usize); // (txid, output_index)

struct SpendingInput {
    txn_id: Sha256dHash,
    funding_output: OutPoint,
}

pub struct Status {
    confirmed: (Vec<FundingOutput>, Vec<SpendingInput>),
    mempool: (Vec<FundingOutput>, Vec<SpendingInput>),
}

impl Status {
    fn funding(&self) -> impl Iterator<Item = &FundingOutput> {
        self.confirmed.0.iter().chain(self.mempool.0.iter())
    }

    fn spending(&self) -> impl Iterator<Item = &SpendingInput> {
        self.confirmed.1.iter().chain(self.mempool.1.iter())
    }

    pub fn history(&self) -> Vec<Sha256dHash> {
        let mut txns = vec![];
        for f in self.funding() {
            txns.push(f.txn_id);
        }
        for s in self.spending() {
            txns.push(s.txn_id);
        }
        txns.sort_unstable();
        txns
    }
}

fn txrows_by_prefix(
    store: &dyn ReadStore,
    txid_prefix: HashPrefix
) -> Vec<TxRow> {
    store
        .scan(&TxRow::filter_prefix(txid_prefix))
        .iter()
        .map(|row| TxRow::from_row(row))
        .collect()
}

fn txoutrows_by_script_hash(
    store: &dyn ReadStore,
    script_hash: &[u8]
) -> Vec<TxOutRow> {
    store
        .scan(&TxOutRow::filter(script_hash))
        .iter()
        .map(|row| TxOutRow::from_row(row))
        .collect()
}

fn txids_by_funding_output(
    store: &dyn ReadStore,
    txn_id: &Sha256dHash,
    output_index: usize,
) -> Vec<HashPrefix> {
    store
        .scan(&TxInRow::filter(&txn_id, output_index))
        .iter()
        .map(|row| TxInRow::from_row(row).txid_prefix)
        .collect()
}

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

    fn load_txns_by_prefix(
        &self,
        store: &dyn ReadStore,
        prefixes: Vec<HashPrefix>,
    ) -> Result<Vec<Sha256dHash>> {
        let mut txns = vec![];
        for txid_prefix in prefixes {
            for tx_row in txrows_by_prefix(store, txid_prefix) {
                let txid: Sha256dHash = deserialize(&tx_row.key.txid).unwrap();
                txns.push(txid)
            }
        }
        Ok(txns)
    }

    fn find_spending_input(
        &self,
        store: &dyn ReadStore,
        funding: &FundingOutput,
    ) -> Result<Option<SpendingInput>> {

        let mut spending_inputs = vec![];
        let prefixes = txids_by_funding_output(store, &funding.txn_id, funding.output_index);
        let spending_txns = self.load_txns_by_prefix(store, prefixes)?;

        for t in &spending_txns {
            if *t == funding.txn_id {
                spending_inputs.push(SpendingInput {
                    txn_id: *t,
                    funding_output: (funding.txn_id, funding.output_index),
                })
            }
        }

        assert!(spending_inputs.len() <= 1);

        Ok(if spending_inputs.len() == 1 {
            Some(spending_inputs.remove(0))
        } else {
            None
        })
    }

    fn find_funding_outputs(
        &self,
        store: &dyn ReadStore,
        script_hash: &[u8]
    ) -> Result<Vec<FundingOutput>> {
        let txout_rows = txoutrows_by_script_hash(store, script_hash);

        let mut result = vec![];

        for row in &txout_rows {
            let funding_txns = self.load_txns_by_prefix(store, vec![row.txid_prefix])?;
            for t in &funding_txns {
                result.push(FundingOutput {
                    txn_id: *t,
                    output_index: row.vout as usize,
                })
            }
        }

        Ok(result)
    }

    fn confirmed_status(
        &self,
        script_hash: &[u8],
    ) -> Result<(Vec<FundingOutput>, Vec<SpendingInput>)> {
        let mut funding = vec![];
        let mut spending = vec![];
        let read_store = self.app.read_store();

        let funding_outputs = self.find_funding_outputs(read_store, script_hash)?;
        funding.extend(funding_outputs);

        for funding_output in &funding {
            if let Some(spent) = self.find_spending_input(read_store, &funding_output)? {
                spending.push(spent);
            }
        }

        Ok((funding, spending))
    }

    fn mempool_status(
        &self,
        script_hash: &[u8],
        confirmed_funding: &[FundingOutput],
    ) -> Result<(Vec<FundingOutput>, Vec<SpendingInput>)> {
        let mut funding = vec![];
        let mut spending = vec![];
        let tracker = self.tracker.read().unwrap();

        let funding_outputs = self.find_funding_outputs(tracker.index(), script_hash)?;
        funding.extend(funding_outputs);

        for funding_output in funding.iter().chain(confirmed_funding.iter()) {
            if let Some(spent) = self.find_spending_input(tracker.index(), &funding_output)? {
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
