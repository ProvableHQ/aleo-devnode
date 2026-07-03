# Repository Guidelines

## Project Structure & Module Organization

This is a Rust binary crate for the `aleo-devnode` CLI. The entrypoint is `src/main.rs`; command modules live in `src/accounts.rs`, `src/advance.rs`, `src/start.rs`, and `src/restore.rs`. REST routing is under `src/rest/`, with shared helpers in `src/rest/helpers/`. Integration tests are in `tests/integration.rs`, and the bundled genesis block is in `resources/`.

## Architecture (the big picture)

Single Rust binary crate built on **snarkVM `TestnetV0`**. The defining trait: **the devnode skips proof verification** (snarkVM `dev_skip_checks`/`test_*` features in `Cargo.toml`), so clients can broadcast transactions built with placeholder proofs/verifying keys. Any code touching transaction validation must preserve this "no real proofs" assumption.

**CLI dispatch** (`src/main.rs`): a clap enum with four subcommands — `start`, `advance`, `restore`, `accounts` — each owning its module. `--private-key` is a *global* arg resolved from the flag or the `PRIVATE_KEY` env var (`start::resolve_private_key`). Keep each command's behavior inside its own module.

**Ledger storage is generic over `ConsensusStorage<N>`.** `src/start.rs` picks the concrete backend at runtime: `ConsensusMemory` (in-memory, default) or `ConsensusDB` (RocksDB, when `--storage <DIR>` is given). Everything downstream — including the whole REST layer — is written generically as `<N: Network, C: ConsensusStorage<N>>` so both backends share one code path.

**REST layer** (`src/rest/`): `Rest<N, C>` (`mod.rs`) holds the ledger plus a `Mutex<Vec<Transaction>>` buffer, shutdown channel, and in-flight verification counters. `build_routes()` defines the route table **once** and is mounted under three prefixes in `spawn_server`:
- `/<network>` and `/v1/<network>` — wrapped in `v1_error_middleware`, which flattens any error into a plain-string HTTP 500 (legacy behavior).
- `/v2/<network>` — returns structured JSON errors (`helpers/error.rs`, `RestError`).

So a single handler change automatically affects all three prefixes; error *shape* differs only via the middleware. Route handlers live in `src/rest/routes.rs`; error types/helpers in `src/rest/helpers/`.

**Block creation has two modes** (`routes.rs`):
- Default (auto): `transaction/broadcast` immediately calls `prepare_advance_to_next_beacon_block` + `advance_to_next_block`, producing one block per transaction.
- `--manual-block-creation`: broadcasts only push into the buffer; blocks are minted on demand via `POST /block/create` (`{"num_blocks": N}`), draining the buffer into the first block.

On startup (auto mode only), `run_devnode` advances the ledger to the last height in `TEST_CONSENSUS_VERSION_HEIGHTS` so the node boots at the latest consensus version. Blocking ledger work is wrapped in `tokio::task::spawn_blocking`.

**Snapshots & restore** (`routes.rs` + `src/restore.rs`): snapshots require `--storage` and are written to a sibling dir `{storage}-snapshots/{name}` (`snapshots_sibling_dir` is the shared source of that layout — reuse it, don't recompute the path). Restore is **offline**: the server must be stopped; `restore` clears the storage dir, copies the snapshot in, and with `--restart` re-execs the binary as `start` (via `exec` on Unix, same PID).

**Genesis**: a 40-validator genesis block is embedded at compile time from `resources/` via `include_bytes!`; `--genesis-path` overrides it.

## Build, Test, and Development Commands

Native build prerequisite: `clang`/`libclang` (snarkVM's `rocks`/RocksDB feature links it). CI installs `libclang-dev`; on macOS it comes with the Xcode Command Line Tools (`xcode-select --install`).

- `cargo build --release`: builds the production CLI binary.
- `cargo test --locked`: runs the same locked-dependency test command used by CI.
- `cargo test <name>`: runs a single test by substring; `cargo test --test integration <name>` runs one integration test.
- `cargo fmt --all -- --check`: verifies formatting against `rustfmt.toml`. CI does **not** enforce fmt — run it yourself.
- `cargo run -- accounts`: lists built-in funded development accounts.
- `cargo run -- start --private-key <DEV_PRIVATE_KEY>`: starts a local node on `127.0.0.1:3030`.

Use the standard development private key from `README.md` only for local testing.

CI (`.github/workflows/ci.yml`) installs `libclang-dev`, verifies `Cargo.lock` is present, and runs `cargo test --locked` on stable. The release workflow refuses to build unless the git tag version matches the `Cargo.toml` version — bump `Cargo.toml` before tagging.

## Coding Style & Naming Conventions

Follow Rust 2021 crate semantics and `rustfmt.toml`: 4-space indentation, 120-column width, shorthand field/try syntax, and crate-granular imports. Use `snake_case` for modules, functions, and tests; `PascalCase` for types; and `SCREAMING_SNAKE_CASE` for constants. Keep command behavior in its owning command module.

## Testing Guidelines

Tests use Rust's built-in test harness plus `reqwest` and `tempfile`. Integration tests spawn the compiled `aleo-devnode` binary through `CARGO_BIN_EXE_aleo-devnode`, allocate local ports, and clean up child processes through `DevnodeGuard`. Name new integration tests `test_*` and avoid depending on port `3030`.

## Domain Deep Dives

Use these docs as selective disclosure. Read them only when touching the related area:

- `docs/cli-usage.md`: commands, flags, env vars, and examples.
- `docs/rest-api.md`: route prefixes, endpoint patterns, and response behavior.
- `docs/storage-and-snapshots.md`: persistent storage, snapshot layout, and restore rules.
- `docs/testing.md`: integration harness, CI checks, ports, and child process cleanup.
- `docs/genesis-and-accounts.md`: bundled genesis data and development account limits.

## Commit & Pull Request Guidelines

Recent commits use short, imperative summaries, usually lowercase, such as `bump version to 0.1.1`. Keep commits focused and include `Cargo.lock` when dependency resolution changes. Pull requests should follow `.github/PULL_REQUEST_TEMPLATE.md`: motivation, test plan, and related PRs or issues.

## Security & Configuration Tips

The devnode skips proof verification by design. Treat all pre-funded keys and placeholder proof workflows as development-only. Never document or commit production keys, private ledgers, or generated snapshot data.
