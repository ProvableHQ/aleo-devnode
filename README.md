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

## REST API

The node exposes the standard Aleo REST API under `/<network>/` (e.g. `/testnet/`), with versioned prefixes `/v1/testnet/` and `/v2/testnet/` also available.

Key endpoints:

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/testnet/block/height/latest` | Latest block height |
| `GET` | `/testnet/block/latest` | Latest block |
| `GET` | `/testnet/block/{height_or_hash}` | Block by height or hash |
| `POST` | `/testnet/transaction/broadcast` | Broadcast a transaction |
| `POST` | `/testnet/block/create` | Create blocks (body: `{"num_blocks": N}`) |
| `GET` | `/testnet/program/{id}` | Get a deployed program |
| `GET` | `/testnet/program/{id}/mapping/{name}/{key}` | Get a mapping value |
| `POST` | `/testnet/shutdown` | Gracefully shut down the node |

### Shutdown

To stop the node gracefully (draining any in-flight requests before exiting):

```sh
curl -X POST http://127.0.0.1:3030/testnet/shutdown
```
