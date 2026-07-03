# Genesis And Accounts Deep Dive

Read this when changing `resources/`, `src/accounts.rs`, genesis loading, or documentation that lists funded development accounts.

## Bundled Genesis

The default genesis block is compiled into the binary from:

```text
resources/genesis_8d710d7e2_40val_snarkos_dev_network.bin
```

`start --genesis-path <file>` replaces the built-in block with a custom genesis file. Do not assume the built-in funded account list applies when a custom genesis path is used.

## Funded Accounts

`src/accounts.rs` defines `FUNDED_ACCOUNTS`, the 50 development address/private-key pairs seeded by the bundled genesis block. The `accounts` command prints this list for local setup convenience.

The first development key is also used by README examples and integration tests:

```text
APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH
```

These keys are public development fixtures. Never present them as safe for production, private deployments, or funded real assets.

## Documentation Rules

If the bundled genesis block or funded account list changes, update `src/accounts.rs`, `README.md`, and tests that assume the standard development private key. For custom genesis workflows, direct users to inspect block 0 through the REST API instead of relying on `accounts`.
