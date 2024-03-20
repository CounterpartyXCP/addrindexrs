extern crate addrindexrs;

extern crate error_chain;
#[macro_use]
extern crate log;

use error_chain::ChainedError;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
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

    let daemon_rpc = config.daemon_rpc_host.as_str().to_owned() + ":" + &config.daemon_rpc_port.to_string();

    let daemon = Daemon::new(
        &config.daemon_dir,
        daemon_rpc.as_str().to_socket_addrs().unwrap().next().unwrap(),
        //SocketAddr::new(config.daemon_rpc_host, config.daemon_rpc_port),
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
        let store = bulk::index_blk_files(&daemon, config.bulk_index_threads, &signal, store)?;
        let store = full_compaction(store);
        // make sure the block header index is up-to-date
        index.reload(&store);
        store
    }
    .enable_compaction(); // enable auto compactions before starting incremental index updates.

    let app = App::new(store, index, daemon)?;
    let query = Query::new(app.clone(), 100);

    let mut server = None; // Indexer RPC server
    loop {
        app.update(&signal)?;
        query.update_mempool()?;
        server.get_or_insert_with(|| {
            RPC::start(
                SocketAddr::new(IpAddr::V4(config.indexer_rpc_host), config.indexer_rpc_port),
                query.clone(),
            )
        });
        if let Err(err) = signal.wait(Duration::from_secs(5)) {
            info!("stopping servertest: {}", err);
            process::exit(1);
        }
    }
}

fn main() {
    let config = Config::from_args();
    if let Err(e) = run_server(&config) {
        error!("server failed: {}", e.display_chain());
        process::exit(1);
    }
}
