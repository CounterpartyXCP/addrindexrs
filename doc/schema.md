# Index Schema

The index is stored at a single RocksDB database using the following schema:

## Transaction outputs' index

Allows efficiently finding all funding transactions for a specific address:

|  Code  | Script Hash Prefix   | Funding TxID Prefix   | Funding Output Index  |   |
| ------ | -------------------- | --------------------- | --------------------- | - |
| `b'O'` | `SHA256(script)[:8]` | `txid[:8]`            | `uint16`              |   |

## Transaction inputs' index

Allows efficiently finding spending transaction of a specific output:

|  Code  | Funding TxID Prefix  | Funding Output Index  | Spending TxID Prefix  |   |
| ------ | -------------------- | --------------------- | --------------------- | - |
| `b'I'` | `txid[:8]`           | `uint16`              | `txid[:8]`            |   |


## Full Transaction IDs

In order to save storage space, we store the full transaction IDs once, and use their 8-byte prefixes for the indexes above.

|  Code  | Transaction ID    |   |
| ------ | ----------------- | - |
| `b'T'` | `txid` (32 bytes) |   |


## Blocks

Stores the hashes and headers of blocks.

|  Code  | Block hash        |   | Block header          |
| ------ | ----------------- | - | --------------------- |
| `b'B'` | `hash` (32 bytes) |   | 80 bytes              |
