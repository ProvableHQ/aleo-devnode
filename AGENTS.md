# Repository Guidelines

## Project Structure & Module Organization

This is a Rust binary crate for the `aleo-devnode` CLI. The entrypoint is `src/main.rs`; command modules live in `src/accounts.rs`, `src/advance.rs`, `src/start.rs`, and `src/restore.rs`. REST routing is under `src/rest/`, with shared helpers in `src/rest/helpers/`. Integration tests are in `tests/integration.rs`, and the bundled genesis block is in `resources/`.

## Build, Test, and Development Commands

- `cargo build --release`: builds the production CLI binary.
- `cargo test --locked`: runs the same locked-dependency test command used by CI.
- `cargo fmt --all -- --check`: verifies formatting against `rustfmt.toml`.
- `cargo run -- accounts`: lists built-in funded development accounts.
- `cargo run -- start --private-key <DEV_PRIVATE_KEY>`: starts a local node on `127.0.0.1:3030`.

Use the standard development private key from `README.md` only for local testing.

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
