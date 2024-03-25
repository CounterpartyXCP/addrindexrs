# addrindexrs


## Releases ##

- [v0.4.4](#0_4_5)
- [v0.4.4](#0_4_4)
- [v0.4.3](#0_4_3)
- [v0.4.0](#0_4_0)
- [v0.3.0](#0_3_0)
- [v0.2.0](#0_2_0)
- [v0.1.0](#0_1_0)

<a name="0_4_5"/>

## addrindexrs v0.4.5 ##

### Change log ###

- Use `txid_limit` only in `get_oldest_tx()`

#### Credits ###

- Warren Puffet
- Adam Krellenstein
- Ouziel Slama


<a name="0_4_4"/>

## addrindexrs v0.4.4 ##

### Change log ###

- `--backend-connect` also accepts hostnames and not just IPs.
- Build Docker image on every push and publish on release.

#### Credits ###

- Warren Puffet
- Adam Krellenstein
- Ouziel Slama

<a name="0_4_3"/>

## addrindexrs v0.4.3 ##

### Change log ###

- Pretty print config on startup.
- Split addr options into host and port.
- Upgrade rocksdb to 0.22.0.
- Remove `txid_limit` parameter.
- Add`current_block_index` argument to `get_oldest_tx` function.
- Upgrade Docker file and documentation.


#### Credits ###

- Warren Puffet
- Adam Krellenstein
- Ouziel Slama


<a name="0_4_0"/>

## addrindexrs v0.4.0 ##

### Change log ###

- [#mr7](https://code.samourai.io/dojo/addrindexrs/-/merge_requests/7) reactivate api endpoints


#### Credits ###

- kenshin-samourai


<a name="0_3_0"/>

## addrindexrs v0.3.0 ##

### Change log ###

- [#9](https://github.com/Samourai-Wallet/addrindexrs/pull/9) rewrite writestore::flush()


#### Credits ###

- kenshin-samourai


<a name="0_2_0"/>

## addrindexrs v0.2.0 ##

### Change log ###

- [#6](https://github.com/Samourai-Wallet/addrindexrs/pull/6) store the compaction marker before the full compaction
- [#7](https://github.com/Samourai-Wallet/addrindexrs/pull/7) add trace for indexed block


#### Credits ###

- kenshin-samourai


<a name="0_1_0"/>

## addrindexrs v0.1.0 ##

Initial release


# Prior releases (electrs)

## 0.8.0 (28 Oct 2019)

* Use `configure_me` instead of `clap` to support config files, environment variables and man pages (@Kixunil)
* Don't accept `--cookie` via CLI arguments (@Kixunil)
* Define cache size in MB instead of number of elements (@dagurval)
* Support Rust >=1.34 (for Debian)
* Bump rust-rocksdb to 0.12.3, using RockDB 6.1.2
* Bump bitcoin crate to 0.21 (@MichelKansou)

## 0.7.1 (27 July 2019)

* Allow stopping bulk indexing via SIGINT/SIGTERM
* Cache list of transaction IDs for blocks (@dagurval)

## 0.7.0 (13 June 2019)

* Support Bitcoin Core 0.18
* Build with LTO
* Allow building with latest Rust (via feature flag)
* Use iterators instead of returning vectors (@Kixunil)
* Use atomics instead of `Mutex<u64>` (@Kixunil)
* Better handling invalid blocks (@azuchi)

## 0.6.2 (17 May 2019)

* Support Rust 1.32 (for Debian)

## 0.6.1 (9 May 2019)

* Fix crash during initial sync
* Switch to `signal-hook` crate

## 0.6.0 (29 Apr 2019)

* Update to Rust 1.34
* Prefix Prometheus metrics with 'electrs_'
* Update RocksDB crate to 0.12.1
* Update Bitcoin crate to 0.18
* Support latest bitcoind mempool entry vsize field name
* Fix "chain-trimming" reorgs
* Serve by default on IPv4 localhost

## 0.5.0 (3 Mar 2019)

* Limit query results, to prevent RPC server to get stuck (see `--txid-limit` flag)
* Update RocksDB crate to 0.11
* Update Bitcoin crate to 0.17

## 0.4.3 (23 Dec 2018)

* Support Rust 2018 edition (1.31)
* Upgrade to Electrum protocol 1.4 (from 1.2)
* Let server banner be configurable via command-line flag
* Improve query.get_merkle_proof() performance

## 0.4.2 (22 Nov 2018)

* Update to rust-bitcoin 0.15.1
* Use bounded LRU cache for transaction retrieval
* Support 'server.ping' and partially 'blockchain.block.header' Electrum RPC

## 0.4.1 (14 Oct 2018)

* Don't run full compaction after initial import is over (when using JSONRPC)

## 0.4.0 (22 Sep 2018)

* Optimize for low-memory systems by using different RocksDB settings
* Rename `--skip_bulk_import` flag to `--jsonrpc-import`

## 0.3.2 (14 Sep 2018)

* Optimize block headers processing during startup
* Handle TCP disconnections during long RPCs
* Use # of CPUs for bulk indexing threads
* Update rust-bitcoin to 0.14
* Optimize block headers processing during startup

## 0.3.1 (20 Aug 2018)

* Reconnect to bitcoind only on transient errors
* Poll mempool after transaction broadcasting

## 0.3.0 (14 Aug 2018)

* Optimize for low-memory systems
* Improve compaction performance
* Handle disconnections from bitcoind by retrying
* Make `blk*.dat` ingestion more robust
* Support regtest network
* Support more Electrum RPC methods
* Export more Prometheus metrics (CPU, RAM, file descriptors)
* Add `scripts/run.sh` for building and running `electrs`
* Add some Python tools (as API usage examples)
* Change default Prometheus monitoring ports

## 0.2.0 (14 Jul 2018)

* Allow specifying custom bitcoind data directory
* Allow specifying JSONRPC cookie from commandline
* Improve initial bulk indexing performance
* Support 32-bit systems

## 0.1.0 (2 Jul 2018)

* Announcement: https://lists.linuxfoundation.org/pipermail/bitcoin-dev/2018-July/016190.html
* Published to https://crates.io/electrs and https://docs.rs/electrs
