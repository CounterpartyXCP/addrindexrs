## Installation

Install [latest Rust](https://rustup.rs/) (1.34+),
[latest Bitcoin Core](https://bitcoincore.org/en/download/) (0.16+).

Also, install the following packages (on Debian):
```bash
$ sudo apt update
$ sudo apt install clang cmake  # for building 'rust-rocksdb'
```

## Build

First build should take ~20 minutes:
```bash
$ git clone https://github.com/Samourai-Wallet/addrindexrs
$ cd addrindexrs
$ cargo build --release
```


## Bitcoind configuration

Allow Bitcoin daemon to sync before starting the indexer. The indexer requires that bitcoin daemon isn't pruned and maintains a txindex.

```bash
$ bitcoind -server=1 -txindex=1 -prune=0
```

If you are using `-rpcuser=USER` and `-rpcpassword=PASSWORD` for authentication, please use `cookie="USER:PASSWORD"` option in one of the config files.
Otherwise, [`~/.bitcoin/.cookie`](https://github.com/bitcoin/bitcoin/blob/0212187fc624ea4a02fc99bc57ebd413499a9ee1/contrib/debian/examples/bitcoin.conf#L70-L72) will be read, allowing this server to use bitcoind JSONRPC interface.

## Usage

First index sync should take ~1.5 hours (on a dual core Intel CPU @ 3.3 GHz, 8 GB RAM, 1TB WD Blue HDD):
```bash
$ cargo run --release -- -vvv --timestamp --db-dir ./db --indexer-rpc-addr="127.0.0.1:50001"
2018-08-17T18:27:42 - INFO - NetworkInfo { version: 179900, subversion: "/Satoshi:0.17.99/" }
2018-08-17T18:27:42 - INFO - BlockchainInfo { chain: "main", blocks: 537204, headers: 537204, bestblockhash: "0000000000000000002956768ca9421a8ddf4e53b1d81e429bd0125a383e3636", pruned: false, initialblockdownload: false }
2018-08-17T18:27:42 - DEBUG - opening DB at "./db/mainnet"
2018-08-17T18:27:42 - DEBUG - full compaction marker: None
2018-08-17T18:27:42 - INFO - listing block files at "/home/user/.bitcoin/blocks/blk*.dat"
2018-08-17T18:27:42 - INFO - indexing 1348 blk*.dat files
2018-08-17T18:27:42 - DEBUG - found 0 indexed blocks
2018-08-17T18:27:55 - DEBUG - applying 537205 new headers from height 0
2018-08-17T19:31:01 - DEBUG - no more blocks to index
2018-08-17T19:31:03 - DEBUG - no more blocks to index
2018-08-17T19:31:03 - DEBUG - last indexed block: best=0000000000000000002956768ca9421a8ddf4e53b1d81e429bd0125a383e3636 height=537204 @ 2018-08-17T15:24:02Z
2018-08-17T19:31:05 - DEBUG - opening DB at "./db/mainnet"
2018-08-17T19:31:06 - INFO - starting full compaction
2018-08-17T19:58:19 - INFO - finished full compaction
2018-08-17T19:58:19 - INFO - enabling auto-compactions
2018-08-17T19:58:19 - DEBUG - opening DB at "./db/mainnet"
2018-08-17T19:58:26 - DEBUG - applying 537205 new headers from height 0
2018-08-17T19:58:27 - DEBUG - downloading new block headers (537205 already indexed) from 000000000000000000150d26fcc38b8c3b71ae074028d1d50949ef5aa429da00
2018-08-17T19:58:27 - INFO - best=000000000000000000150d26fcc38b8c3b71ae074028d1d50949ef5aa429da00 height=537218 @ 2018-08-17T16:57:50Z (14 left to index)
2018-08-17T19:58:28 - DEBUG - applying 14 new headers from height 537205
2018-08-17T19:58:29 - INFO - RPC server running on 127.0.0.1:50001
```
You can specify options via command-line parameters, environment variables or using config files. See the documentation below.

Note that the final DB size should be ~20% of the `blk*.dat` files, but it may increase to ~35% at the end of the inital sync (just before the [full compaction is invoked](https://github.com/facebook/rocksdb/wiki/Manual-Compaction)).

If initial sync fails due to `memory allocation of xxxxxxxx bytes failedAborted` errors, as may happen on devices with limited RAM, try the following arguments when starting `addrindexrs`. It should take roughly 18 hours to sync and compact the index on an ODROID-HC1 with 8 CPU cores @ 2GHz, 2GB RAM, and an SSD using the following command:

```bash
$ cargo run --release -- -vvvv --index-batch-size=10 --jsonrpc-import --db-dir ./db --indexer-rpc-addr="127.0.0.1:50001"
```

The index database is stored here:
```bash
$ du db/
38G db/mainnet/
```

### Example of use with docker

Assuming `bitcoind` is listening on 127.0.0.1:10312 with "USER:PASS" as rpc credentials:

```
$ docker build -t addrindexrs .
$ docker run --rm -d \
    --name indexer \
    --network="host" \
    addrindexrs \
    -vvv --daemon-rpc-addr="127.0.0.1:10312" \
    --cookie="USER:PASS"
```

## Configuration files and environment variables

The config files must be in the Toml format. These config files are (from lowest priority to highest): `/etc/addrindexrs/config.toml`, `~/.addrindexrs/config.toml`, `./addrindexrs.toml`.

The options in highest-priority config files override options set in lowest-priority config files. Environment variables override options in config files and finally arguments override everythig else.

For each argument an environment variable of the same name with `ADDRINDEXRS_` prefix, upper case letters and underscores instead of hypens exists (e.g. you can use `ADDRINDEXRS_INDEXER_RPC_ADDR` instead of `--indexer-rpc-addr`). Similarly, for each argument an option in config file exists with underscores instead o hypens (e.g. `indexer_rpc_addr`).

Finally, you need to use a number in config file if you want to increase verbosity (e.g. `verbose = 3` is equivalent to `-vvv`) and `true` value in case of flags (e.g. `timestamp = true`)


### SSL connection

In order to use a secure connection, you can also use [NGINX as an SSL endpoint](https://docs.nginx.com/nginx/admin-guide/security-controls/terminating-ssl-tcp/#) by placing the following block in `nginx.conf`.

```nginx
stream {
        upstream addrindexrs {
                server 127.0.0.1:50001;
        }

        server {
                listen 50002 ssl;
                proxy_pass addrindexrs;

                ssl_certificate /path/to/example.crt;
                ssl_certificate_key /path/to/example.key;
                ssl_session_cache shared:SSL:1m;
                ssl_session_timeout 4h;
                ssl_protocols TLSv1 TLSv1.1 TLSv1.2 TLSv1.3;
                ssl_prefer_server_ciphers on;
        }
}
```

```bash
$ sudo systemctl restart nginx
```

Note: You can obtain a free SSL certificate as follows:

1. Follow the instructions at https://certbot.eff.org/ to install the certbot on your system.
2. When certbot obtains the SSL certificates for you, change the SSL paths in the nginx template above as follows:
```
ssl_certificate /etc/letsencrypt/live/<your-domain>/fullchain.pem;
ssl_certificate_key /etc/letsencrypt/live/<your-domain>/privkey.pem;
```

### Tor hidden service

Install Tor on your server and client machines (assuming Ubuntu/Debian):

```
$ sudo apt install tor
```

Add the following config to `/etc/tor/torrc`:
```
HiddenServiceDir /var/lib/tor/hidden_service/
HiddenServiceVersion 3
HiddenServicePort 50001 127.0.0.1:50001
```

Restart the service:
```
$ sudo systemctl restart tor
```

Note: your server's onion address is stored under:
```
$ sudo cat /var/lib/tor/hidden_service/hostname
<your-onion-address>.onion
```

For more details, see http://docs.electrum.org/en/latest/tor.html.

### Sample Systemd Unit File

You may wish to have systemd manage addrindexrs so that it's "always on." Here is a sample unit file (which assumes that the bitcoind unit file is `bitcoind.service`):

```
[Unit]
Description=addrindexrs
After=bitcoind.service

[Service]
WorkingDirectory=/home/bitcoin/addrindexrs
ExecStart=/home/bitcoin/addrindexrs/target/release/addrindexrs --db-dir ./db --indexer-rpc-addr="127.0.0.1:50001"
User=bitcoin
Group=bitcoin
Type=simple
KillMode=process
TimeoutSec=60
Restart=always
RestartSec=60

[Install]
WantedBy=multi-user.target
```
