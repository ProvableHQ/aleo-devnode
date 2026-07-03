# REST API Deep Dive

Read this when changing `src/rest/`, route paths, request or response shapes, error mapping, or README endpoint examples.

## Route Prefixes

Routes are mounted under three prefixes:

- `/testnet/...`
- `/v1/testnet/...`
- `/v2/testnet/...`

The default and `v1` routers wrap responses in `v1_error_middleware` (`src/rest/mod.rs`): any non-success response is flattened to **HTTP 500** with a plain-text body — the error message and its cause chain joined by ` — `. The `v2` router skips that middleware and returns the structured `RestError` form (`src/rest/helpers/error.rs`): the real status code (400/404/422/429/500) and a JSON body `{message, error_type, chain}`.

So the same handler error surfaces as a 500 string under `/testnet` and `/v1/testnet`, but as a typed JSON error under `/v2/testnet`.

## Route Groups

`src/rest/mod.rs` defines the route table. Handler implementations live in `src/rest/routes.rs`. Keep new endpoints grouped with nearby ledger, transaction, program, snapshot, or shutdown routes.

Important endpoint families include:

- Node info: `/consensus_version` (returns the active consensus version at the latest height; the devnode boots at the latest version — see the Architecture section in AGENTS.md)
- Blocks: `/block/height/latest`, `/block/latest`, `/block/{height_or_hash}`, `/block/create`
- Transactions: `/transaction/broadcast`, `/transaction/{id}`, confirmed and unconfirmed lookups
- Programs and mappings: `/program/{id}`, `/program/{id}/mapping/{name}/{key}`, `/program/{id}/mapping/{name}`
- State and find helpers: `/stateRoot/latest`, `/statePath/{commitment}`, `/find/...`
- Devnode controls: `/transaction/broadcast`, `/block/create`, `/snapshot`, `/snapshots`, `/shutdown`

## Devnode Control Endpoints

These are the endpoints that *drive* the devnode rather than just read from it — they are what distinguishes this tool from a read-only snarkVM node. All examples use the bare `/testnet` prefix; the same paths exist under `/v1/testnet` and `/v2/testnet` (only the error shape differs — see Route Prefixes). Handlers are in `src/rest/routes.rs`.

### `POST /transaction/broadcast` — submit a transaction (the main entry point)

The primary way to put state on the ledger. Body is a JSON-serialized snarkVM `Transaction`. Optional query `?check_transaction=false` skips structural validation (`check_transaction_basic`); it defaults to `true`. Note this is *structural* validation only — proof verification is always skipped by design, so placeholder-proof transactions are accepted either way.

```sh
# auto mode (default): this single call also mints a block containing the tx
curl -X POST http://127.0.0.1:3030/testnet/transaction/broadcast \
  -H 'Content-Type: application/json' --data @tx.json
```

**When to use:** every time you want to commit a transaction. In **auto mode** (default) a successful broadcast *immediately mints one block* containing the transaction — nothing else is needed. In **`--manual-block-creation` mode** the transaction is only buffered; it does not land on the ledger until you call `POST /block/create`.

Returns `200` with the transaction ID on success. Failure modes: `400` (transaction exceeds the byte limit), `422` (malformed JSON body or failed validation), `429` (too many in-flight verifications — bounded per type by `VM::MAX_PARALLEL_EXECUTE_VERIFICATIONS` / `MAX_PARALLEL_DEPLOY_VERIFICATIONS`; retry with backoff).

### `POST /block/create` — mint blocks on demand

A JSON body is **required** (the handler uses axum's `Json` extractor, which rejects requests with no body or missing `Content-Type: application/json`). Only the `num_blocks` field is optional: send `{}` to default to one block, or `{"num_blocks": N}` where `N` must be `1..=1000`. Drains *all* currently buffered transactions into the **first** block created; any further blocks in the same call are empty.

```sh
# mint one block, sweeping up everything broadcast since the last block
curl -X POST http://127.0.0.1:3030/testnet/block/create \
  -H 'Content-Type: application/json' -d '{}'

# mint three blocks (txs land in the first, the rest are empty)
curl -X POST http://127.0.0.1:3030/testnet/block/create \
  -H 'Content-Type: application/json' -d '{"num_blocks": 3}'
```

**When to use:**
- In **`--manual-block-creation` mode**: this is how buffered broadcasts actually get committed. Broadcast N transactions, then call this once to seal them into a block. Use this when a test needs several transactions in a *single* block, or precise control over block boundaries/timing.
- In **auto mode**: the buffer is always empty (broadcasts self-seal), so calling this just mints *empty* blocks — handy to advance height or trigger time/height-gated logic without any transactions.

Returns the last created block as JSON. `400` if `num_blocks` is `0` or exceeds `1000`.

### `POST /snapshot` — checkpoint the ledger (online)

A JSON body is **required** (the handler uses axum's `Json` extractor — send `Content-Type: application/json` and a body every time, even for the default name). The `name` field is optional: send `{}` to default to `snapshot-{height}`, or `{"name": "my-snapshot"}` to choose one. **Requires `--storage`** (in-memory nodes return `400`). The name must not contain path separators or `..`. Writes a RocksDB backup to the sibling dir `{storage}-snapshots/{name}/`.

```sh
# default name (snapshot-{height})
curl -X POST http://127.0.0.1:3030/testnet/snapshot \
  -H 'Content-Type: application/json' -d '{}'

# explicit name
curl -X POST http://127.0.0.1:3030/testnet/snapshot \
  -H 'Content-Type: application/json' -d '{"name": "before-upgrade"}'
# -> {"name":"before-upgrade","height":1234}
```

**When to use:** to capture a known-good ledger state mid-run that you can return to later. Snapshots are created *online* here, but **restoring is offline** — stop the node and use the `restore` CLI subcommand (see `docs/storage-and-snapshots.md`); there is no restore REST endpoint.

### `GET /snapshots` — list snapshots

Requires `--storage` (`400` otherwise). Returns a sorted JSON array of snapshot directory names (`[]` if none exist yet). Use it to discover names to pass to the `restore` CLI command.

```sh
curl http://127.0.0.1:3030/testnet/snapshots
# -> ["before-upgrade","snapshot-1234"]
```

### `POST /shutdown` — graceful shutdown

No body. **Loopback-only**: requests from non-local IPs get `403`. Triggers graceful shutdown via the server's one-shot channel and returns `200`.

```sh
curl -X POST http://127.0.0.1:3030/testnet/shutdown
```

**When to use:** to cleanly stop a node you started — e.g. tear-down in a test harness, or before running an offline `restore`. The integration test `DevnodeGuard` kills the child process directly instead, so this is mainly for external scripts.

## Behavior Notes

Mapping list reads require `all=true`; metadata responses use `metadata=true`. The request body limit is set in `build_routes` to match the snarkVM binary transaction limit policy.

A `tower_governor` rate-limit layer is wired into `build_routes`, but `start.rs` passes an effectively unlimited RPS, so it does not throttle in practice. Don't rely on it for limiting; the real backpressure is the verification-slot counter above.
