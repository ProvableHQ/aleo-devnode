// Copyright (C) 2019-2026 Provable Inc.
// This file is part of the aleo-devnode tool.
//
// Licensed under the GNU General Public License v3.0.

use crate::{logger::initialize_terminal_logger, rest::Rest};

use anyhow::Result;
use clap::Parser;
use serde_json::json;
use std::{net::SocketAddr, path::PathBuf, str::FromStr};

use aleo_std_storage::StorageMode;
use snarkvm::{
    ledger::store::helpers::{memory::ConsensusMemory, rocksdb::ConsensusDB},
    prelude::{
        Block, FromBytes, Ledger, PrivateKey, TEST_CONSENSUS_VERSION_HEIGHTS, TestnetV0, store::ConsensusStorage,
    },
};

// Command for starting the Devnode server.
#[derive(Parser, Debug)]
#[group(id = "start_args")]
pub struct Start {
    /// Verbosity level for logging (0-2).
    #[clap(short = 'v', long, help = "devnode verbosity (0-2)", default_value = "2", value_parser = clap::value_parser!(u8).range(0..=2))]
    pub(crate) verbosity: u8,
    /// Address to bind the Devnode REST API server to.
    #[clap(short = 'a', long, help = "devnode REST API server address", default_value = "127.0.0.1:3030")]
    pub(crate) socket_addr: String,
    /// Path to the genesis block file.
    #[clap(short = 'g', long, help = "path to genesis block file", default_value = "blank")]
    pub(crate) genesis_path: String,
    /// Enable manual block creation mode.
    #[clap(short = 'm', long, help = "disables automatic block creation after broadcast")]
    pub(crate) manual_block_creation: bool,
    /// Optional flag for persisting the ledger to disk. If not set, the ledger will be stored in memory and will not persist across restarts.
    #[clap(short = 's', long, help = "directory for ledger persistence", num_args = 0..=1, default_missing_value = "devnode")]
    pub(crate) storage: Option<PathBuf>,
    /// If set alongside --storage, clears the ledger directory before starting.
    #[clap(short = 'c', long, help = "Remove existing devnode storage before starting", requires = "storage")]
    pub(crate) clear_storage: bool,
}

impl Start {
    pub fn execute(self, private_key: Option<String>) -> Result<()> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| anyhow::anyhow!("Failed to create async runtime: {e}"))?;
        rt.block_on(async { start_devnode(self, private_key).await })
    }
}

// This command initializes a local development node that is pre-populated with test accounts.
async fn start_devnode(command: Start, private_key: Option<String>) -> Result<()> {
    // Initialize the logger.
    println!("Starting the Devnode server...");
    // Load the private key from the command line or environment variable, and start the server.
    let private_key = resolve_private_key(&private_key)?;
    initialize_terminal_logger(command.verbosity).expect("Failed to initialize logger");

    // Parse the listener address.
    let socket_addr: SocketAddr = command
        .socket_addr
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse listener address '{}': {}", command.socket_addr, e))?;
    // Load the genesis block.
    let genesis_block: Block<TestnetV0> = if command.genesis_path != "blank" {
        Block::from_bytes_le(
            &std::fs::read(&command.genesis_path)
                .map_err(|e| anyhow::anyhow!("Failed to read genesis block file '{}': {}", command.genesis_path, e))?,
        )?
    } else {
        // This genesis block is stored in $TMPDIR when running snarkos start --dev 0 --dev-num-validators N
        Block::from_bytes_le(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/genesis_8d710d7e2_40val_snarkos_dev_network.bin"
        )))?
    };
    let manual_block_creation = command.manual_block_creation;
    match command.storage {
        Some(path) => {
            if command.clear_storage && path.exists() {
                for entry in
                    std::fs::read_dir(&path).map_err(|e| anyhow::anyhow!("Failed to read ledger directory: {e}"))?
                {
                    let entry = entry.map_err(|e| anyhow::anyhow!("Failed to read entry: {e}"))?;
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        std::fs::remove_dir_all(&entry_path)
                            .map_err(|e| anyhow::anyhow!("Failed to remove '{}': {e}", entry_path.display()))?;
                    } else {
                        std::fs::remove_file(&entry_path)
                            .map_err(|e| anyhow::anyhow!("Failed to remove '{}': {e}", entry_path.display()))?;
                    }
                }
                println!("Cleaned ledger directory: {}", path.display());
            }
            println!("Using persistent ledger at: {}", path.display());
            let storage_mode = StorageMode::Custom(path.clone());
            let ledger: Ledger<TestnetV0, ConsensusDB<TestnetV0>> =
                tokio::task::spawn_blocking(move || Ledger::load(genesis_block, storage_mode))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to load ledger: {e}"))??;
            run_devnode(socket_addr, ledger, manual_block_creation, private_key, Some(path)).await?
        }
        None => {
            let storage_mode = StorageMode::new_test(None);
            let ledger: Ledger<TestnetV0, ConsensusMemory<TestnetV0>> =
                tokio::task::spawn_blocking(move || Ledger::load(genesis_block, storage_mode))
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to load ledger: {e}"))??;
            run_devnode(socket_addr, ledger, manual_block_creation, private_key, None).await?
        }
    }

    Ok(())
}

async fn run_devnode<C: 'static + ConsensusStorage<TestnetV0>>(
    socket_addr: SocketAddr,
    ledger: Ledger<TestnetV0, C>,
    manual_block_creation: bool,
    private_key: PrivateKey<TestnetV0>,
    storage_path: Option<PathBuf>,
) -> Result<()> {
    let rps = 999999999;

    // Record the height before handing the ledger off, so we know how far to advance.
    let current_height = ledger.latest_height();

    let rest = Rest::start(socket_addr, rps, ledger, manual_block_creation, private_key, storage_path)
        .await
        .expect("Failed to start the REST API server");
    println!("Server running on http://{socket_addr}");

    if !manual_block_creation {
        let target_height = TEST_CONSENSUS_VERSION_HEIGHTS.last().map(|&(_, h)| h).unwrap_or(0);
        let blocks_to_advance = target_height.saturating_sub(current_height);
        if blocks_to_advance > 0 {
            println!("Advancing the Devnode to the latest consensus version");
            let client = reqwest::Client::new();
            let payload = json!({ "num_blocks": blocks_to_advance });
            match client
                .post(format!("http://{}/testnet/block/create", socket_addr))
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
            {
                Ok(r) if !r.status().is_success() => {
                    tracing::warn!("Auto-advance to consensus version failed with status {}", r.status());
                }
                Err(e) => tracing::warn!("Auto-advance to consensus version failed: {e}"),
                Ok(_) => {}
            }
        }
    }

    // Wait until the server shuts down (via the /shutdown endpoint or a signal).
    rest.wait_for_shutdown().await;
    Ok(())
}

fn resolve_private_key(private_key: &Option<String>) -> Result<PrivateKey<TestnetV0>> {
    match private_key {
        Some(pk) => Ok(PrivateKey::<TestnetV0>::from_str(pk).map_err(|e| anyhow::anyhow!("Invalid private key: {e}"))?),
        None => {
            let pk = std::env::var("PRIVATE_KEY").map_err(|e| {
                anyhow::anyhow!(
                    "
Failed to load `PRIVATE_KEY` from the environment: {e}
Please either:
1. Use the --private-key flag: `aleo-devnode start --private-key <PRIVATE_KEY>`
2. Set the PRIVATE_KEY environment variable"
                )
            })?;
            Ok(PrivateKey::<TestnetV0>::from_str(&pk).map_err(|e| anyhow::anyhow!("Invalid private key: {e}"))?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::FUNDED_ACCOUNTS;
    use snarkvm::{
        ledger::{authority::Authority, narwhal::Subdag},
        prelude::{
            Address, Certificate, ConsensusVersion, Deployment, Fee, Field, Network, Program, ProgramOwner,
            Transaction, VerifyingKey, deployment_cost,
        },
    };
    use std::sync::Arc;

    const VALID_PRIVATE_KEY: &str = "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH";
    const INVALID_PRIVATE_KEY: &str = "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWa";

    #[test]
    fn test_blank_private_key_from_flag() {
        let err = resolve_private_key(&Some(String::new())).unwrap_err();
        assert!(err.to_string().contains("Invalid private key"));
    }

    #[test]
    fn test_invalid_private_key_from_flag() {
        let err = resolve_private_key(&Some(INVALID_PRIVATE_KEY.to_string())).unwrap_err();
        assert!(err.to_string().contains("Invalid private key"));
    }

    #[test]
    fn test_valid_private_key_from_flag() {
        assert!(resolve_private_key(&Some(VALID_PRIVATE_KEY.to_string())).is_ok());
    }

    #[test]
    fn test_beacon_authority_enforces_block_limits() {
        let private_key = resolve_private_key(&Some(VALID_PRIVATE_KEY.to_string())).unwrap();
        let mut rng = rand::rng();
        let authority = Authority::<TestnetV0>::new_beacon(&private_key, Field::from_u64(0), &mut rng).unwrap();
        let v16_height = TestnetV0::CONSENSUS_HEIGHT(ConsensusVersion::V16).unwrap();
        let v18_height = TestnetV0::CONSENSUS_HEIGHT(ConsensusVersion::V18).unwrap();

        assert_eq!(authority.spend_limit(v16_height), Subdag::<TestnetV0>::min_spend_limit(v16_height));
        assert!(authority.spend_limit(v16_height).is_some());
        assert_eq!(authority.synthesis_limit(v16_height), None);
        assert_eq!(authority.synthesis_limit(v18_height), Subdag::<TestnetV0>::min_synthesis_limit(v18_height));
        assert!(authority.synthesis_limit(v18_height).is_some());
    }

    fn placeholder_deployment(
        ledger: &Ledger<TestnetV0, ConsensusMemory<TestnetV0>>,
        private_key: &PrivateKey<TestnetV0>,
        program_name: &str,
        density: u64,
        rng: &mut (impl rand::Rng + rand::CryptoRng),
    ) -> Transaction<TestnetV0> {
        const PLACEHOLDER_CERTIFICATE: &str = "certificate1qyqsqqqqqqqqqqxvwszp09v860w62s2l4g6eqf0kzppyax5we36957ywqm2dplzwvvlqg0kwlnmhzfatnax7uaqt7yqqqw0sc4u";

        let program = Program::from_str(&format!(
            "program {program_name}.aleo;\n\nfunction run:\n    assert.eq true true;\n\nconstructor:\n    assert.eq true true;\n"
        ))
        .unwrap();
        let function_name = *program.functions().keys().next().unwrap();
        let program_checksum = program.to_checksum();
        let mut circuit_key = TestnetV0::get_credits_verifying_key("fee_public".to_string()).unwrap().as_ref().clone();
        circuit_key.circuit_info.num_non_zero_a = usize::try_from(density).unwrap();
        circuit_key.circuit_info.num_non_zero_b = 0;
        circuit_key.circuit_info.num_non_zero_c = 0;
        let verifying_key = VerifyingKey::new(Arc::new(circuit_key), 1);
        let certificate = Certificate::from_str(PLACEHOLDER_CERTIFICATE).unwrap();
        let owner_address = Address::try_from(private_key).unwrap();
        let deployment = Deployment::new(
            0,
            program,
            vec![(function_name, (verifying_key, certificate))],
            Some(program_checksum),
            Some(owner_address),
        )
        .unwrap();

        let deployment_id = deployment.to_deployment_id().unwrap();
        let owner = ProgramOwner::new(private_key, deployment_id, rng).unwrap();
        let consensus_version = TestnetV0::CONSENSUS_VERSION(ledger.latest_height() + 1).unwrap();
        let (base_fee, _) = deployment_cost(ledger.vm().process(), &deployment, consensus_version).unwrap();
        let authorization = ledger.vm().authorize_fee_public(private_key, base_fee, 0, deployment_id, rng).unwrap();
        let fee_transition = authorization.transitions().into_values().next().unwrap();
        let fee = Fee::from(fee_transition, ledger.latest_state_root(), None).unwrap();

        Transaction::from_deployment(owner, deployment, fee).unwrap()
    }

    #[test]
    fn test_beacon_block_aborts_deployment_over_synthesis_limit() {
        let genesis = Block::from_bytes_le(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/genesis_8d710d7e2_40val_snarkos_dev_network.bin"
        )))
        .unwrap();
        let ledger: Ledger<TestnetV0, ConsensusMemory<TestnetV0>> =
            Ledger::load(genesis, StorageMode::new_test(None)).unwrap();
        let beacon_key = PrivateKey::from_str(FUNDED_ACCOUNTS[0].1).unwrap();
        let mut rng = rand::rng();
        let v18_height = TestnetV0::CONSENSUS_HEIGHT(ConsensusVersion::V18).unwrap();

        while ledger.latest_height() + 1 < v18_height {
            let block =
                ledger.prepare_advance_to_next_beacon_block(&beacon_key, vec![], vec![], vec![], &mut rng).unwrap();
            ledger.advance_to_next_block(&block).unwrap();
        }

        let synthesis_limit = Subdag::<TestnetV0>::min_synthesis_limit(v18_height).unwrap();
        let deployment_density = synthesis_limit / 2 + 1;
        let first_key = PrivateKey::from_str(FUNDED_ACCOUNTS[1].1).unwrap();
        let second_key = PrivateKey::from_str(FUNDED_ACCOUNTS[2].1).unwrap();
        let first = placeholder_deployment(&ledger, &first_key, "limit_first", deployment_density, &mut rng);
        let second = placeholder_deployment(&ledger, &second_key, "limit_second", deployment_density, &mut rng);
        let first_id = first.id();
        let second_id = second.id();
        let block = ledger
            .prepare_advance_to_next_beacon_block(&beacon_key, vec![], vec![], vec![first, second], &mut rng)
            .unwrap();

        assert_eq!(block.height(), v18_height);
        assert_eq!(block.transactions().num_accepted(), 1);
        assert_eq!(block.transactions().num_rejected(), 0);
        assert_eq!(block.aborted_transaction_ids().as_slice(), &[second_id]);
        assert!(block.transactions().get(&first_id).is_some());
    }
}
