extern crate addrindexrs;

extern crate error_chain;
#[macro_use]
extern crate log;

use error_chain::ChainedError;
use std::process;
use std::sync::Arc;
use std::time::Duration;


use addrindexrs::{
    app::App,
    bulk,
    cache::BlockTxIDsCache,
    config::Config,
    daemon::Daemon,
    errors::*,
    index::Index,
    query::Query,
    rpc::RPC,
    signal::Waiter,
    store::{full_compaction, is_fully_compacted, DBStore},
};

fn run_server(config: &Config) -> Result<()> {
    let signal = Waiter::start();
    let blocktxids_cache = Arc::new(BlockTxIDsCache::new(config.blocktxids_cache_size));

    let daemon = Daemon::new(
        &config.daemon_dir,
        config.daemon_rpc_addr,
        config.cookie_getter(),
        config.network_type,
        signal.clone(),
        blocktxids_cache,
    )?;

    // Perform initial indexing from local blk*.dat block files.
    let store = DBStore::open(&config.db_path, /*low_memory=*/ config.jsonrpc_import);
    let index = Index::load(&store, &daemon, config.index_batch_size)?;

    let store = if is_fully_compacted(&store) {
         // initial import and full compaction are over
        store
    } else if config.jsonrpc_import {
         // slower: uses JSONRPC for fetching blocks
        index.update(&store, &signal)?;
        full_compaction(store)
    } else {
        // faster, but uses more memory
        let store =
            bulk::index_blk_files(&daemon, config.bulk_index_threads, &signal, store)?;
        let store = full_compaction(store);
         // make sure the block header index is up-to-date
        index.reload(&store);
        store
    }
    .enable_compaction(); // enable auto compactions before starting incremental index updates.

    let app = App::new(store, index, daemon)?;
    let query = Query::new(app.clone(), config.txid_limit);

    let mut server = None; // Indexer RPC server
    loop {
        app.update(&signal)?;
        query.update_mempool()?;
        server.get_or_insert_with(|| RPC::start(config.indexer_rpc_addr, query.clone()));
        if let Err(err) = signal.wait(Duration::from_secs(5)) {
            info!("stopping server: {}", err);
            break;
        }
    }
    Ok(())
}

fn main() {
    let config = Config::from_args();
    if let Err(e) = run_server(&config) {
        error!("server failed: {}", e.display_chain());
        process::exit(1);
    }
}
