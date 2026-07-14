# aleo-devnode

Prebuilt binary distribution of [`aleo-devnode`](https://github.com/ProvableHQ/aleo-devnode) — a local Aleo development node that skips proof verification so clients can broadcast transactions built with placeholder proofs.

## Install

```sh
npm install -g @provablehq/aleo-devnode
```

This installs a tiny launcher plus one platform-specific binary package (selected automatically by npm via `os`/`cpu`). Supported platforms:

| OS      | Arch  |
| ------- | ----- |
| macOS   | arm64 |
| macOS   | x64   |
| Linux   | x64   |
| Linux   | arm64 |
| Windows | x64   |

## Usage

```sh
aleo-devnode accounts
aleo-devnode start --private-key <DEV_PRIVATE_KEY>
```

All arguments are forwarded directly to the underlying binary. See the [project README](https://github.com/ProvableHQ/aleo-devnode) for full CLI documentation.

## How it works

The `@provablehq/aleo-devnode` package declares each platform binary as an `optionalDependency`. npm installs only the one whose `os`/`cpu` match the host. The `aleo-devnode` command is a thin Node shim that resolves the matching package and execs the real binary.

If you install with `--no-optional` (or `--omit=optional`), the binary won't be present; reinstall without that flag.
