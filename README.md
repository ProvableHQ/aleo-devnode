# aleo-devnode

A standalone Aleo development node for local testing and development.

## Build

```sh
cargo build --release
```

## Usage

### Start the devnode

```sh
aleo-devnode start --private-key APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH
```

The private key above is the standard Aleo testnet development key — safe to use locally, **never use it in production**.

The node starts a REST API on `http://127.0.0.1:3030` by default and automatically advances the ledger to the latest consensus version.

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--private-key` | `$PRIVATE_KEY` env var | Private key for block creation |
| `-a, --socket-addr` | `127.0.0.1:3030` | REST API bind address |
| `-v, --verbosity` | `2` | Log verbosity: `0` = info, `1` = debug, `2` = trace |
| `-g, --genesis-path` | built-in | Path to a custom genesis block file |
| `-m, --manual-block-creation` | off | Disable automatic block creation on broadcast |
| `-s, --storage [DIR]` | in-memory | Persist the ledger to disk at `DIR` (default: `devnode/`) |
| `-c, --clear-storage` | off | Clear the storage directory before starting (requires `-s`) |

### Pre-funded accounts

The built-in genesis block seeds 50 accounts with funds for testing. The first five are:

| # | Address | Private Key |
|---|---------|-------------|
| 0 | `aleo1rhgdu77hgyqd3xjj8ucu3jj9r2krwz6mnzyd80gncr5fxcwlh5rsvzp9px` | `APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH` |
| 1 | `aleo1s3ws5tra87fjycnjrwsjcrnw2qxr8jfqqdugnf0xzqqw29q9m5pqem2u4t` | `APrivateKey1zkp2RWGDcde3efb89rjhME1VYA8QMxcxep5DShNBR6n8Yjh` |
| 2 | `aleo1ashyu96tjwe63u0gtnnv8z5lhapdu4l5pjsl2kha7fv7hvz2eqxs5dz0rg` | `APrivateKey1zkp2GUmKbVsuc1NSj28pa1WTQuZaK5f1DQJAT6vPcHyWokG` |
| 3 | `aleo12ux3gdauck0v60westgcpqj7v8rrcr3v346e4jtq04q7kkt22czsh808v2` | `APrivateKey1zkpBjpEgLo4arVUkQmcLdKQMiAKGaHAQVVwmF8HQby8vdYs` |
| 4 | `aleo1p9sg8gapg22p3j42tang7c8kqzp4lhe6mg77gx32yys2a5y7pq9sxh6wrd` | `APrivateKey1zkp3J6rRrDEDKAMMzSQmkBqd3vPbjp4XTyH7oMKFn7eVFwf` |

> ⚠️ These are development keys. Never use them in production.

To list all 50 accounts:

```sh
aleo-devnode accounts
```

If you are using a custom genesis block, query block 0 to inspect funded accounts:

```sh
curl http://127.0.0.1:3030/testnet/block/0
```

### Advance the ledger manually

When running with `--manual-block-creation`, use `advance` to create blocks explicitly:

```sh
aleo-devnode advance
```

Advance by a specific number of blocks:

```sh
aleo-devnode advance 5
```

By default this targets `127.0.0.1:3030`. Use `--socket-addr` to point at a different instance.

### Persistent storage

```sh
aleo-devnode start --private-key APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH --storage
```

This persists the ledger to a `devnode/` directory. To start fresh:

```sh
aleo-devnode start --private-key APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH --storage --clear-storage
```

## Snapshots

Snapshots capture the ledger state at a specific block height and can be restored later. They require persistent storage (`--storage`).

Snapshots are saved alongside the storage directory — e.g. if storage is `devnode/`, snapshots are written to `devnode-snapshots/`.

### Take a snapshot

```sh
curl -X POST http://127.0.0.1:3030/testnet/snapshot \
     -H "Content-Type: application/json" \
     -d '{"name": "before-deploy"}'
```

If no name is provided, the snapshot is named automatically using the current block height (e.g. `snapshot-42`):

```sh
curl -X POST http://127.0.0.1:3030/testnet/snapshot \
     -H "Content-Type: application/json" \
     -d '{}'
```

Response: `{"name": "before-deploy", "height": 42}`

### List snapshots

```sh
curl http://127.0.0.1:3030/testnet/snapshots
```

Response: `["before-deploy", "snapshot-42"]`

### Restore a snapshot

The devnode must not be running when restoring. Stop it first, then run:

```sh
aleo-devnode restore --snapshot before-deploy --storage devnode
```

The original snapshot directory is left untouched, so the same snapshot can be restored multiple times.

### Restore and restart in one step

```sh
aleo-devnode restore \
    --snapshot before-deploy \
    --storage devnode \
    --restart \
    --private-key APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH
```

If `PRIVATE_KEY` is set in the environment, `--private-key` can be omitted.

### Restore options

| Flag | Default | Description |
|------|---------|-------------|
| `--snapshot` | required | Name of the snapshot to restore |
| `--storage` | `devnode` | Ledger storage directory to restore into |
| `--restart` | off | Restart the devnode after restoring |
| `--private-key` | `$PRIVATE_KEY` env var | Private key (forwarded to `start` when `--restart` is set) |
| `-a, --socket-addr` | `127.0.0.1:3030` | REST API address (forwarded to `start` when `--restart` is set) |
| `-v, --verbosity` | `2` | Log verbosity (forwarded to `start` when `--restart` is set) |
| `-m, --manual-block-creation` | off | Disable auto block creation (forwarded to `start` when `--restart` is set) |

## REST API

The node exposes the standard Aleo REST API under `/<network>/` (e.g. `/testnet/`), with versioned prefixes `/v1/testnet/` and `/v2/testnet/` also available.

Key endpoints:

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/testnet/block/height/latest` | Latest block height |
| `GET` | `/testnet/block/latest` | Latest block |
| `GET` | `/testnet/block/{height_or_hash}` | Block by height or hash |
| `POST` | `/testnet/transaction/broadcast` | Broadcast a transaction |
| `POST` | `/testnet/block/create` | Create blocks (body: `{"num_blocks": N}`, optional) |
| `GET` | `/testnet/program/{id}` | Get a deployed program |
| `GET` | `/testnet/program/{id}/mapping/{name}/{key}` | Get a mapping value |
| `POST` | `/testnet/snapshot` | Take a snapshot (body: `{"name": "optional"}`) |
| `GET` | `/testnet/snapshots` | List available snapshots |
| `POST` | `/testnet/shutdown` | Gracefully shut down the node |

### Shutdown

To stop the node gracefully (draining any in-flight requests before exiting):

```sh
curl -X POST http://127.0.0.1:3030/testnet/shutdown
```

Ctrl+C and SIGTERM also trigger graceful shutdown.
