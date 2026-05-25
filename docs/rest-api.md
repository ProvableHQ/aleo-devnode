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

- Blocks: `/block/height/latest`, `/block/latest`, `/block/{height_or_hash}`, `/block/create`
- Transactions: `/transaction/broadcast`, `/transaction/{id}`, confirmed and unconfirmed lookups
- Programs and mappings: `/program/{id}`, `/program/{id}/mapping/{name}/{key}`, `/program/{id}/mapping/{name}`
- State and find helpers: `/stateRoot/latest`, `/statePath/{commitment}`, `/find/...`
- Devnode controls: `/snapshot`, `/snapshots`, `/shutdown`

## Behavior Notes

`POST /transaction/broadcast` verifies by default unless `check_transaction=false` is supplied. In automatic mode it creates a block immediately; in manual mode it buffers the transaction until `/block/create`. Verification is bounded by per-type in-flight slots (`VM::MAX_PARALLEL_EXECUTE_VERIFICATIONS` / `MAX_PARALLEL_DEPLOY_VERIFICATIONS`); exceeding them returns 429.

Mapping list reads require `all=true`; metadata responses use `metadata=true`. The request body limit is set in `build_routes` to match the snarkVM binary transaction limit policy.

A `tower_governor` rate-limit layer is wired into `build_routes`, but `start.rs` passes an effectively unlimited RPS, so it does not throttle in practice. Don't rely on it for limiting; the real backpressure is the verification-slot counter above.
