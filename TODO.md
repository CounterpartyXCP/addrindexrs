# Indexer

* Snapshot DB after successful indexing - and run queries on the latest snapshot
* Improve management of reorgs (remove rows related to txs of reorg'd blocks from db)

# Rust

* Use [bytes](https://carllerche.github.io/bytes/bytes/index.html) instead of `Vec<u8>` when possible
* Use generators instead of vectors
* Use proper HTTP parser for JSONRPC replies over persistent connection

# Performance

* Consider https://github.com/facebook/rocksdb/wiki/RocksDB-Tuning-Guide#difference-of-spinning-disk
