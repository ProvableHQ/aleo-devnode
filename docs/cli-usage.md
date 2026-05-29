# CLI Usage Deep Dive

Read this when changing `src/main.rs`, any command module, README command examples, or user-facing flag behavior.

## Command Map

- `start`: starts the REST devnode and owns server, storage, genesis, logging, and block creation flags.
- `advance`: a thin client that POSTs to `/testnet/block/create`; defaults to advancing one block. It surfaces failures — a connection error or a non-success HTTP status returns an error and exits nonzero.
- `restore`: copies a named snapshot back into a storage directory and can re-exec as `start`.
- `accounts`: prints the 50 built-in development accounts for the bundled genesis block.
- `update`: self-updates the binary from GitHub releases (`ProvableHQ/aleo-devnode`).

## Global flags

These are defined on the top-level CLI (`src/main.rs`):

- `-v` / `-V` / `--version`: print the version and exit. Top-level only (parsed before the subcommand). **Caution:** `-v` at the top level means *version*, but `-v` after `start` or `restore` means *verbosity* — clap disambiguates by position.
- `--private-key <KEY>`: marked `global = true`, so clap accepts it positionally alongside `start` (e.g. `start --private-key ...` or `--private-key ... start`). However, `src/main.rs` only forwards it into `start`. **It is ignored by `restore`** — `restore --restart` reads its own `--private-key` (or `PRIVATE_KEY`), not the top-level one. See Private Key Rules below.

## `start` flags

| Flag | Short | Default | Notes |
| --- | --- | --- | --- |
| `--private-key <KEY>` | | `PRIVATE_KEY` env | Global. Required (flag or env). Used to sign every block. |
| `--verbosity <0-2>` | `-v` | `2` | Logging level; values outside `0..=2` are rejected by clap. |
| `--socket-addr <ADDR>` | `-a` | `127.0.0.1:3030` | REST API bind address. Must parse as a `SocketAddr`. |
| `--genesis-path <PATH>` | `-g` | `blank` | `blank` is a sentinel meaning the bundled 40-validator genesis; any other value is read as a genesis block file. |
| `--manual-block-creation` | `-m` | off | Switches block creation from automatic to manual — see below. |
| `--storage [DIR]` | `-s` | in-memory | Persist the ledger to disk (RocksDB). See storage behavior below. |
| `--clear-storage` | `-c` | off | Wipe the storage dir before starting. **Requires `--storage`** (clap enforces this). |

### Storage flag behavior (`--storage`)

`--storage` takes an optional value (`num_args = 0..=1`):

- **Omitted entirely** → ledger runs in memory (`ConsensusMemory`); nothing persists across restarts.
- **`-s` / `--storage` with no value** → defaults to the directory `devnode` (`default_missing_value`).
- **`--storage <DIR>`** → persists to `<DIR>` (`ConsensusDB` / RocksDB).

Snapshots (`POST /snapshot`) and the `restore` command only work when `--storage` is set — in-memory nodes return `400` for snapshot routes. See `docs/storage-and-snapshots.md`.

## Block creation

This is the core of how clients drive the ledger. There are two modes, selected at `start` time:

### Automatic (default)

Every successful `POST /testnet/transaction/broadcast` **immediately mints one block** containing that transaction — no extra call needed. Additionally, on startup in this mode `run_devnode` auto-advances the ledger to the last height in `TEST_CONSENSUS_VERSION_HEIGHTS` (by calling `/block/create` once internally), so the node boots at the latest consensus version.

You can still mint blocks explicitly in automatic mode — `advance <N>` (or `POST /block/create`) works here too. Since the broadcast buffer is always empty in this mode, those blocks are empty; use it to advance height or trigger height/time-gated logic without any transactions.

### Manual (`--manual-block-creation` / `-m`)

Broadcasts are only **buffered**; they do not land on the ledger until a block is minted explicitly. Use this when a test needs several transactions in one block, deterministic block boundaries, or control over block timing. Two ways to mint:

- `cargo run -- advance <N>` — the `advance` subcommand (a thin client that POSTs to `/block/create`).
- `POST /testnet/block/create` with `{"num_blocks": N}` directly (see `docs/rest-api.md`).

The buffered transactions are drained into the **first** block created; any further blocks in the same call are empty. `num_blocks` is bounded server-side to `1..=1000`.

Note: in manual mode the startup consensus-version auto-advance is **skipped**, so the node remains at the loaded ledger height — genesis height for a fresh ledger, or the persisted height when restarting against existing `--storage`. Advance manually if you need a later consensus version.

## `advance` flags

| Arg | Default | Notes |
| --- | --- | --- |
| `<num_blocks>` (positional) | `1` | Number of blocks to mint. |
| `--socket-addr <ADDR>` | `127.0.0.1:3030` | Target devnode. |

A thin client against a running server — it does not share the ledger, it just calls `/block/create`. Connection failures and non-success HTTP responses (status + body) are returned as errors and exit nonzero.

## `restore` flags

`restore` is **offline** — the server must be stopped first. Flags forwarded to `start` only apply with `--restart`, and **only the ones in the table below are forwarded** (`--storage`, `--private-key`, `--socket-addr`, `--verbosity`, `--manual-block-creation`). Notably `--genesis-path` and `--clear-storage` are **not** forwarded — if you restored a snapshot built from a custom genesis, restart with a manual `start --genesis-path ...` instead of `--restart`.

| Flag | Short | Default | Notes |
| --- | --- | --- | --- |
| `--snapshot <NAME>` | | (required) | Snapshot directory name under `{storage}-snapshots/`. |
| `--storage <DIR>` | | `devnode` | Storage dir to restore into; must match the `--storage` used at `start`. |
| `--restart` | | off | Re-exec as `start` after restoring (via `exec` on Unix, same PID). |
| `--private-key <KEY>` | | `PRIVATE_KEY` env | Forwarded to `start` on `--restart`. |
| `--socket-addr <ADDR>` | `-a` | `127.0.0.1:3030` | Forwarded to `start` on `--restart`. |
| `--verbosity <0-2>` | `-v` | `2` | Forwarded to `start` on `--restart`. |
| `--manual-block-creation` | `-m` | off | Forwarded to `start` on `--restart`. |

## `update` flags

| Flag | Short | Notes |
| --- | --- | --- |
| `--list` | `-l` | List available releases instead of updating. |
| `--name <TAG>` | `-n` | Update to a specific release tag rather than latest. |
| `--quiet` | `-q` | Suppress download/progress output. |

## Private Key Rules

`start` requires a private key. Prefer `--private-key` in examples because it is explicit and overrides the `PRIVATE_KEY` environment fallback. If neither is present, startup exits with an error. Invalid or blank keys should fail before the REST server starts.

`restore --restart` has its own forwarded `--private-key`; without it, restart relies on `PRIVATE_KEY`. The key is passed to the re-exec'd process via the `PRIVATE_KEY` env var to keep it out of the process argument list.

## Examples To Keep Current

```sh
cargo run -- accounts
cargo run -- start --private-key <DEV_PRIVATE_KEY>
cargo run -- start --private-key <DEV_PRIVATE_KEY> --manual-block-creation
cargo run -- start --private-key <DEV_PRIVATE_KEY> --storage devnode --clear-storage
cargo run -- advance 5 --socket-addr 127.0.0.1:3030
cargo run -- restore --snapshot before-deploy --storage devnode
cargo run -- update --list
```

When adding or renaming flags, update the owning command module, `README.md`, and this file together.
