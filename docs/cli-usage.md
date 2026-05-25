# CLI Usage Deep Dive

Read this when changing `src/main.rs`, any command module, README command examples, or user-facing flag behavior.

## Command Map

- `start`: starts the REST devnode and owns server, storage, genesis, logging, and block creation flags.
- `advance`: posts to `/testnet/block/create`; defaults to advancing one block. Fire-and-forget — it discards the HTTP response and errors, so a wrong `--socket-addr` or a down server fails silently.
- `restore`: copies a named snapshot back into a storage directory and can re-exec as `start`.
- `accounts`: prints the 50 built-in development accounts for the bundled genesis block.

## Private Key Rules

`start` requires a private key. Prefer `--private-key` in examples because it is explicit and overrides the `PRIVATE_KEY` environment fallback. If neither is present, startup exits with an error. Invalid or blank keys should fail before the REST server starts.

`restore --restart` has its own forwarded `--private-key`; without it, restart relies on `PRIVATE_KEY`.

## Examples To Keep Current

```sh
cargo run -- accounts
cargo run -- start --private-key <DEV_PRIVATE_KEY>
cargo run -- start --private-key <DEV_PRIVATE_KEY> --manual-block-creation
cargo run -- advance 5 --socket-addr 127.0.0.1:3030
cargo run -- restore --snapshot before-deploy --storage devnode
```

When adding or renaming flags, update the owning command module, `README.md`, and this file together.
