# Testing Deep Dive

Read this when changing `tests/integration.rs`, process lifecycle behavior, REST readiness, storage persistence, or CI expectations.

## Commands

```sh
cargo test --locked
cargo fmt --all -- --check
```

CI currently runs `cargo test --locked` on stable Rust after installing `libclang-dev` and verifying `Cargo.lock` exists. Run formatting locally even though CI does not currently enforce it.

## Integration Harness

Integration tests spawn the compiled binary using `env!("CARGO_BIN_EXE_aleo-devnode")`. This mirrors real CLI behavior better than calling internal functions directly.

`DevnodeGuard` owns the child process, a blocking `reqwest` client, and the base REST URL. It waits for REST readiness by polling `/testnet/block/height/latest`, then kills and waits for the child on drop. Prefer this guard for new tests.

## Test Patterns

Use `alloc_port()` instead of hard-coding `3030`. Use `tempfile::tempdir()` for persistent storage and snapshot cases. Keep test names in the `test_*` pattern. For manual block creation tests, pass `--manual-block-creation` and explicitly call `/block/create` through the helper methods.

Long startup paths can take time because the ledger and consensus heights are initialized; keep polling timeouts realistic.
