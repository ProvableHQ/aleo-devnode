# Storage And Snapshots Deep Dive

Read this when changing `src/start.rs`, `src/restore.rs`, snapshot REST handlers, or tests that persist ledger state.

## Storage Modes

Without `--storage`, the devnode uses in-memory ledger storage and state is lost on shutdown. With `--storage`, it uses RocksDB-backed storage at the provided path. Passing `--storage` without a value uses `devnode/`.

Use `--clear-storage` only with `--storage`. It clears entries inside the storage directory before loading the ledger and leaves the directory itself in place.

## Snapshot Layout

Snapshots require persistent storage. The snapshot directory is a sibling of the storage directory:

```text
devnode/             # active ledger
devnode-snapshots/   # snapshots for that ledger
```

For a custom storage path, `snapshots_sibling_dir()` derives the snapshot root from the storage directory name, for example `tmp/ledger` maps to `tmp/ledger-snapshots`.

## Restore Rules

`restore --snapshot <name> --storage <dir>` clears the target storage directory and copies the named snapshot into it. The source snapshot is not deleted, so the same snapshot can be reused.

The devnode should be stopped before restore. `restore --restart` re-executes the current binary as `start` with the storage path and forwarded runtime flags.
