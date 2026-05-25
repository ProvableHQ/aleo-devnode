# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Start here

Read **`AGENTS.md`** first — it holds the canonical project structure, build/test commands, coding style, commit/PR conventions, and security notes. This file adds the cross-file architecture and Claude-specific pointers; it does not repeat AGENTS.md.

`docs/` is selective-disclosure reference. Read the matching file only when touching that area:
- `docs/cli-usage.md` — commands, flags, env vars.
- `docs/rest-api.md` — route prefixes, endpoint patterns, response behavior.
- `docs/storage-and-snapshots.md` — persistence, snapshot layout, restore rules.
- `docs/testing.md` — integration harness, CI, ports, child-process cleanup.
- `docs/genesis-and-accounts.md` — bundled genesis data, dev account limits.

## Common commands

Native build prerequisite: `clang`/`libclang` (snarkVM's `rocks`/RocksDB feature links it). CI installs `libclang-dev`; on macOS it comes with the Xcode Command Line Tools (`xcode-select --install`).

```sh
cargo build --release                 # build the CLI binary
cargo test --locked                   # full suite, mirrors CI
cargo test <name>                     # run a single test by substring
cargo test --test integration <name>  # run one integration test
cargo fmt --all -- --check            # CI does NOT enforce fmt; run it yourself
cargo run -- accounts                 # list pre-funded dev accounts
cargo run -- start --private-key APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH
```

CI (`.github/workflows/ci.yml`) installs `libclang-dev`, verifies `Cargo.lock` is present, and runs `cargo test --locked` on stable. The release workflow refuses to build unless the git tag version matches the `Cargo.toml` version — bump `Cargo.toml` before tagging.

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

## Testing model

Integration tests (`tests/integration.rs`) spawn the compiled binary through `env!("CARGO_BIN_EXE_aleo-devnode")` rather than calling internals — this exercises real CLI/process behavior. `DevnodeGuard` owns the child process and kills it on drop; `alloc_port()` hands out unique ports (never hard-code `3030`). Startup polls `/block/height/latest` for readiness. See `docs/testing.md` before changing the harness.
