// Copyright (C) 2019-2026 Provable Inc.
// This file is part of the aleo-devnode tool.
//
// Licensed under the GNU General Public License v3.0.

use super::*;

use crate::restore::snapshots_sibling_dir;
use snarkvm::{
    ledger::narwhal::BatchHeader,
    prelude::{
        Block, ConsensusVersion, Deployment, Identifier, LimitedWriter, Plaintext, Program, ToBytes, Transaction, VM,
        Value,
    },
    synthesizer::{
        process::transaction_compute_spend_in_microcredits,
        program::{FinalizeGlobalState, StackTrait},
    },
};

use axum::{Json, extract::rejection::JsonRejection};

use anyhow::{Context, anyhow, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::Ordering;

use rayon::prelude::*;

/// Maps a snarkvm ledger error to a REST error, returning 404 for missing items.
/// snarkvm returns `anyhow::Error` without typed variants, so the message text is the only signal.
fn ledger_err(err: anyhow::Error) -> RestError {
    if err.to_string().contains("Missing") { RestError::not_found(err) } else { RestError::from(err) }
}

/// Returns the spend and synthesis limits for a minimally dense beacon block.
fn beacon_block_limits<N: Network>(block_height: u32) -> anyhow::Result<(Option<u64>, Option<u64>)> {
    let consensus_version = N::CONSENSUS_VERSION(block_height)?;
    let max_certificates = active_consensus_value(&N::MAX_CERTIFICATES, consensus_version)
        .map(u64::from)
        .ok_or_else(|| anyhow!("Missing MAX_CERTIFICATES for consensus version {consensus_version}"))?;
    // Match snarkVM PR #3350. A minimal subdag has two rounds. Each round has an availability threshold of
    // ceil(MAX_CERTIFICATES / 3), so this is not ceil(2 * MAX_CERTIFICATES / 3).
    let min_certificates = max_certificates.saturating_add(2).saturating_div(3).saturating_mul(2);

    let spend_limit = if consensus_version >= ConsensusVersion::V16 {
        Some(min_certificates.saturating_mul(BatchHeader::<N>::batch_spend_limit(block_height)))
    } else {
        None
    };
    let v19_height = N::CONSENSUS_HEIGHT(ConsensusVersion::V19)?;
    let synthesis_limit = if consensus_version >= ConsensusVersion::V18 && block_height <= v19_height {
        let synthesis_per_round = 5_f64 * N::SYNTHESIS_PER_SECOND_OF_RUNTIME as f64;
        let synthesis_per_certificate = synthesis_per_round / max_certificates as f64;
        Some((synthesis_per_certificate * min_certificates as f64) as u64)
    } else {
        None
    };

    Ok((spend_limit, synthesis_limit))
}

/// Returns the value that applies at the given consensus version.
fn active_consensus_value<T: Copy>(values: &[(ConsensusVersion, T)], consensus_version: ConsensusVersion) -> Option<T> {
    values.iter().rfind(|(version, _)| *version <= consensus_version).map(|(_, value)| *value)
}

/// Checks the deployment limits that snarkVM skips when `dev_skip_checks` is enabled.
fn check_deployment_limits<N: Network>(
    deployment: &Deployment<N>,
    consensus_version: ConsensusVersion,
) -> anyhow::Result<()> {
    let limits = if consensus_version >= ConsensusVersion::V19 {
        Some((N::MAX_DEPLOYMENT_VARIABLES_V2, N::MAX_DEPLOYMENT_CONSTRAINTS_V2))
    } else if consensus_version >= ConsensusVersion::V18 {
        None
    } else {
        Some((N::MAX_DEPLOYMENT_VARIABLES, N::MAX_DEPLOYMENT_CONSTRAINTS))
    };

    if let Some((variable_limit, constraint_limit)) = limits {
        ensure!(
            deployment.num_combined_variables()? <= variable_limit,
            "The number of combined variables exceeds the deployment limit"
        );
        ensure!(
            deployment.num_combined_constraints()? <= constraint_limit,
            "The number of combined constraints exceeds the deployment limit"
        );
    }

    Ok(())
}

/// Removes transactions that exceed an active deployment, transaction spend, or block synthesis limit.
fn filter_preparation_limits<N: Network, C: ConsensusStorage<N>>(
    ledger: &Ledger<N, C>,
    block_height: u32,
    synthesis_limit: Option<u64>,
    transactions: &[Transaction<N>],
) -> Result<(Vec<(Transaction<N>, u64)>, Vec<N::TransactionID>), RestError> {
    let consensus_version = N::CONSENSUS_VERSION(block_height)
        .map_err(|error| RestError::internal_server_error(error.context("Failed to determine consensus version")))?;
    let current_consensus_version = N::CONSENSUS_VERSION(ledger.latest_height())
        .map_err(|error| RestError::internal_server_error(error.context("Failed to determine consensus version")))?;
    let transaction_spend_limit = if consensus_version >= ConsensusVersion::V16 {
        Some(active_consensus_value(&N::TRANSACTION_SPEND_LIMIT, consensus_version).ok_or_else(|| {
            RestError::internal_server_error(anyhow!("Missing transaction spend limit for {consensus_version}"))
        })?)
    } else {
        None
    };
    let mut block_combined_density = 0u64;
    let mut candidates = Vec::with_capacity(transactions.len());
    let mut aborted_transaction_ids = Vec::new();

    for transaction in transactions {
        if transaction
            .deployment()
            .is_some_and(|deployment| check_deployment_limits(deployment, current_consensus_version).is_err())
        {
            aborted_transaction_ids.push(transaction.id());
            continue;
        }
        let compute_spend = if let Some(transaction_spend_limit) = transaction_spend_limit {
            let Ok(compute_spend) =
                transaction_compute_spend_in_microcredits(ledger.vm().process(), transaction, consensus_version)
            else {
                aborted_transaction_ids.push(transaction.id());
                continue;
            };
            if compute_spend > transaction_spend_limit {
                aborted_transaction_ids.push(transaction.id());
                continue;
            }
            compute_spend
        } else {
            0
        };
        if let (Some(synthesis_limit), Some(deployment)) = (synthesis_limit, transaction.deployment()) {
            let density = deployment.combined_density();
            if block_combined_density.saturating_add(density) > synthesis_limit {
                aborted_transaction_ids.push(transaction.id());
                continue;
            }
            block_combined_density = block_combined_density.saturating_add(density);
        }
        candidates.push((transaction.clone(), compute_spend));
    }

    Ok((candidates, aborted_transaction_ids))
}

/// Removes transactions that exceed the cumulative block spend limit.
fn filter_block_spend_limit<N: Network>(
    transactions: &[(Transaction<N>, u64)],
    spend_limit: Option<u64>,
) -> (Vec<Transaction<N>>, Vec<N::TransactionID>) {
    let mut block_spend = 0u64;
    let mut candidates = Vec::with_capacity(transactions.len());
    let mut aborted_transaction_ids = Vec::new();

    for (transaction, compute_spend) in transactions {
        if spend_limit.is_some_and(|limit| block_spend.saturating_add(*compute_spend) > limit) {
            aborted_transaction_ids.push(transaction.id());
            continue;
        }
        block_spend = block_spend.saturating_add(*compute_spend);
        candidates.push(transaction.clone());
    }

    (candidates, aborted_transaction_ids)
}

/// Adds aborted IDs that belong to the candidate transaction list.
fn extend_snarkvm_aborted_transaction_ids<N: Network>(
    transactions: &[Transaction<N>],
    new_transaction_ids: &[N::TransactionID],
    aborted_transaction_ids: &mut Vec<N::TransactionID>,
) -> Result<(), RestError> {
    let previous_len = aborted_transaction_ids.len();
    for transaction_id in new_transaction_ids {
        if transactions.iter().any(|transaction| transaction.id() == *transaction_id)
            && !aborted_transaction_ids.contains(transaction_id)
        {
            aborted_transaction_ids.push(*transaction_id);
        }
    }
    if aborted_transaction_ids.len() == previous_len {
        return Err(RestError::internal_server_error(anyhow!(
            "Block preparation returned an unknown aborted transaction"
        )));
    }
    Ok(())
}

/// Prepares a beacon block and enforces the limits that snarkVM skips for beacon blocks.
fn prepare_beacon_block_with_limits<N: Network, C: ConsensusStorage<N>, R: rand::Rng + rand::CryptoRng>(
    ledger: &Ledger<N, C>,
    private_key: &PrivateKey<N>,
    transactions: Vec<Transaction<N>>,
    rng: &mut R,
) -> Result<(Block<N>, Vec<N::TransactionID>), RestError> {
    let block_height = ledger.latest_height().saturating_add(1);
    let (spend_limit, synthesis_limit) = beacon_block_limits::<N>(block_height)
        .map_err(|error| RestError::internal_server_error(error.context("Failed to calculate beacon block limits")))?;

    prepare_beacon_block_with_limit_values(ledger, private_key, transactions, spend_limit, synthesis_limit, rng)
}

/// Prepares a beacon block with explicit block limits.
fn prepare_beacon_block_with_limit_values<N: Network, C: ConsensusStorage<N>, R: rand::Rng + rand::CryptoRng>(
    ledger: &Ledger<N, C>,
    private_key: &PrivateKey<N>,
    transactions: Vec<Transaction<N>>,
    spend_limit: Option<u64>,
    synthesis_limit: Option<u64>,
    rng: &mut R,
) -> Result<(Block<N>, Vec<N::TransactionID>), RestError> {
    let block_height = ledger.latest_height().saturating_add(1);
    let mut snarkvm_aborted_transaction_ids = Vec::new();

    loop {
        let active_transactions = transactions
            .iter()
            .filter(|transaction| !snarkvm_aborted_transaction_ids.contains(&transaction.id()))
            .cloned()
            .collect::<Vec<_>>();
        let (preparation_candidates, preparation_aborted_transaction_ids) =
            filter_preparation_limits(ledger, block_height, synthesis_limit, &active_transactions)?;
        let (candidate_transactions, spend_aborted_transaction_ids) =
            filter_block_spend_limit(&preparation_candidates, spend_limit);
        let prepared_block = ledger
            .prepare_advance_to_next_beacon_block(private_key, vec![], vec![], candidate_transactions, rng)
            .map_err(|error| RestError::internal_server_error(anyhow!("Failed to prepare block: {error}")))?;

        if !prepared_block.aborted_transaction_ids().is_empty() {
            extend_snarkvm_aborted_transaction_ids(
                &transactions,
                prepared_block.aborted_transaction_ids(),
                &mut snarkvm_aborted_transaction_ids,
            )?;
            continue;
        }

        let aborted_transaction_ids = transactions
            .iter()
            .filter(|transaction| {
                snarkvm_aborted_transaction_ids.contains(&transaction.id())
                    || preparation_aborted_transaction_ids.contains(&transaction.id())
                    || spend_aborted_transaction_ids.contains(&transaction.id())
            })
            .map(Transaction::id)
            .collect();
        return Ok((prepared_block, aborted_transaction_ids));
    }
}

/// Returns an error if block preparation aborted one or more transactions.
fn ensure_no_aborted_transactions<T: ToString>(aborted_transaction_ids: &[T]) -> Result<(), RestError> {
    if aborted_transaction_ids.is_empty() {
        return Ok(());
    }

    let transaction_ids = aborted_transaction_ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
    Err(RestError::unprocessable_entity(anyhow!(
        "Block preparation aborted the following transactions: {transaction_ids}"
    )))
}

/// Deserialize a CSV string into a vector of strings.
fn de_csv<'de, D>(de: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(de)?;
    Ok(if s.trim().is_empty() { Vec::new() } else { s.split(',').map(|x| x.trim().to_string()).collect() })
}

type ViewFunctionRoute<N> = (ProgramID<N>, Identifier<N>, u32);

/// Parses a list of strings into a `Vec<Value<N>>` for use as view function inputs.
fn parse_view_inputs<N: Network>(inputs: &[String]) -> Result<Vec<Value<N>>, RestError> {
    inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            input.parse::<Value<N>>().map_err(|err| {
                RestError::unprocessable_entity(err.context(format!("Invalid input at index {index}: {input}")))
            })
        })
        .collect::<Result<Vec<_>, _>>()
}

/// The `get_blocks` query object.
#[derive(Deserialize, Serialize)]
pub(crate) struct BlockRange {
    /// The starting block height (inclusive).
    start: u32,
    /// The ending block height (exclusive).
    end: u32,
}

/// The query object for `get_mapping_value` and `get_mapping_values`.
#[derive(Copy, Clone, Deserialize, Serialize)]
pub(crate) struct Metadata {
    metadata: Option<bool>,
    all: Option<bool>,
}

/// The query object for `transaction_broadcast`.
#[derive(Copy, Clone, Deserialize, Serialize)]
pub(crate) struct CheckTransaction {
    check_transaction: Option<bool>,
}

/// The query object for `get_state_paths_for_commitments`.
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Commitments {
    #[serde(deserialize_with = "de_csv")]
    commitments: Vec<String>,
}

/// The request object for creating a new block.
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct CreateBlockRequest {
    /// number of blocks to create.
    pub num_blocks: Option<u32>,
}

impl<N: Network, C: ConsensusStorage<N>> Rest<N, C> {
    /// Get /<network>/consensus_version
    pub(crate) async fn get_consensus_version(State(rest): State<Self>) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(N::CONSENSUS_VERSION(rest.ledger.latest_height())? as u16))
    }

    /// GET /<network>/block/height/latest
    pub(crate) async fn get_block_height_latest(State(rest): State<Self>) -> ErasedJson {
        ErasedJson::new(rest.ledger.latest_height())
    }

    /// GET /<network>/block/hash/latest
    pub(crate) async fn get_block_hash_latest(State(rest): State<Self>) -> ErasedJson {
        ErasedJson::new(rest.ledger.latest_hash())
    }

    /// GET /<network>/block/latest
    pub(crate) async fn get_block_latest(State(rest): State<Self>) -> ErasedJson {
        ErasedJson::new(rest.ledger.latest_block())
    }

    /// GET /<network>/block/{height}
    /// GET /<network>/block/{blockHash}
    pub(crate) async fn get_block(
        State(rest): State<Self>,
        Path(height_or_hash): Path<String>,
    ) -> Result<ErasedJson, RestError> {
        // Manually parse the height or the height of the hash, axum doesn't support different types
        // for the same path param.
        let block = if let Ok(height) = height_or_hash.parse::<u32>() {
            rest.ledger.get_block(height).with_context(|| "Failed to get block by height")?
        } else if let Ok(hash) = height_or_hash.parse::<N::BlockHash>() {
            rest.ledger.get_block_by_hash(&hash).with_context(|| "Failed to get block by hash")?
        } else {
            return Err(RestError::bad_request(anyhow!(
                "invalid input, it is neither a block height nor a block hash"
            )));
        };

        Ok(ErasedJson::new(block))
    }

    /// GET /<network>/blocks?start={start_height}&end={end_height}
    pub(crate) async fn get_blocks(
        State(rest): State<Self>,
        Query(block_range): Query<BlockRange>,
    ) -> Result<ErasedJson, RestError> {
        let start_height = block_range.start;
        let end_height = block_range.end;

        const MAX_BLOCK_RANGE: u32 = 50;

        // Ensure the end height is greater than the start height.
        if start_height > end_height {
            return Err(RestError::bad_request(anyhow!("Invalid block range")));
        }

        // Ensure the block range is bounded.
        if end_height - start_height > MAX_BLOCK_RANGE {
            return Err(RestError::bad_request(anyhow!(
                "Cannot request more than {MAX_BLOCK_RANGE} blocks per call (requested {})",
                end_height - start_height
            )));
        }

        // Prepare a closure for the blocking work.
        let get_json_blocks = move || -> Result<ErasedJson, RestError> {
            let blocks = (start_height..end_height)
                .into_par_iter()
                .map(|height| rest.ledger.get_block(height))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(ErasedJson::new(blocks))
        };

        // Fetch the blocks from ledger and serialize to json.
        match tokio::task::spawn_blocking(get_json_blocks).await {
            Ok(json) => json,
            Err(err) => {
                let err: anyhow::Error = err.into();

                Err(RestError::internal_server_error(
                    err.context(format!("Failed to get blocks '{start_height}..{end_height}'")),
                ))
            }
        }
    }

    /// GET /<network>/height/{blockHash}
    pub(crate) async fn get_height(
        State(rest): State<Self>,
        Path(hash): Path<N::BlockHash>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.get_height(&hash)?))
    }

    /// GET /<network>/block/{height}/header
    pub(crate) async fn get_block_header(
        State(rest): State<Self>,
        Path(height): Path<u32>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.get_header(height)?))
    }

    /// GET /<network>/block/{height}/transactions
    pub(crate) async fn get_block_transactions(
        State(rest): State<Self>,
        Path(height): Path<u32>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.get_transactions(height)?))
    }

    /// GET /<network>/transaction/{transactionID}
    pub(crate) async fn get_transaction(
        State(rest): State<Self>,
        Path(tx_id): Path<N::TransactionID>,
    ) -> Result<ErasedJson, RestError> {
        // Ledger returns a generic anyhow::Error, so checking the message is the only way to parse it.
        Ok(ErasedJson::new(rest.ledger.get_transaction(tx_id).map_err(|err| ledger_err(err))?))
    }

    /// GET /<network>/transaction/confirmed/{transactionID}
    pub(crate) async fn get_confirmed_transaction(
        State(rest): State<Self>,
        Path(tx_id): Path<N::TransactionID>,
    ) -> Result<ErasedJson, RestError> {
        // Ledger returns a generic anyhow::Error, so checking the message is the only way to parse it.
        Ok(ErasedJson::new(rest.ledger.get_confirmed_transaction(tx_id).map_err(|err| ledger_err(err))?))
    }

    /// GET /<network>/transaction/unconfirmed/{transactionID}
    pub(crate) async fn get_unconfirmed_transaction(
        State(rest): State<Self>,
        Path(tx_id): Path<N::TransactionID>,
    ) -> Result<ErasedJson, RestError> {
        // Ledger returns a generic anyhow::Error, so checking the message is the only way to parse it.
        Ok(ErasedJson::new(rest.ledger.get_unconfirmed_transaction(&tx_id).map_err(|err| ledger_err(err))?))
    }

    /// GET /<network>/program/{programID}
    /// GET /<network>/program/{programID}?metadata={true}
    pub(crate) async fn get_program(
        State(rest): State<Self>,
        Path(id): Path<ProgramID<N>>,
        metadata: Query<Metadata>,
    ) -> Result<ErasedJson, RestError> {
        // Get the program from the ledger.
        let program = rest.ledger.get_program(id).with_context(|| format!("Failed to find program `{id}`"))?;
        // Check if metadata is requested and return the program with metadata if so.
        if metadata.metadata.unwrap_or(false) {
            // Get the edition of the program.
            let edition = rest.ledger.get_latest_edition_for_program(&id)?;
            return rest.return_program_with_metadata(program, edition);
        }
        // Return the program without metadata.
        Ok(ErasedJson::new(program))
    }

    /// GET /<network>/program/{programID}/{edition}
    /// GET /<network>/program/{programID}/{edition}?metadata={true}
    pub(crate) async fn get_program_for_edition(
        State(rest): State<Self>,
        Path((id, edition)): Path<(ProgramID<N>, u16)>,
        metadata: Query<Metadata>,
    ) -> Result<ErasedJson, RestError> {
        // Get the program from the ledger.
        match rest
            .ledger
            .try_get_program_for_edition(&id, edition)
            .with_context(|| format!("Failed get program `{id}` for edition {edition}"))?
        {
            Some(program) => {
                // Check if metadata is requested and return the program with metadata if so.
                if metadata.metadata.unwrap_or(false) {
                    rest.return_program_with_metadata(program, edition)
                } else {
                    Ok(ErasedJson::new(program))
                }
            }
            None => Err(RestError::not_found(anyhow!("No program `{id}` exists for edition {edition}"))),
        }
    }

    /// A helper function to return the program and its metadata.
    /// This function is used in the `get_program` and `get_program_for_edition` functions.
    fn return_program_with_metadata(&self, program: Program<N>, edition: u16) -> Result<ErasedJson, RestError> {
        let id = program.id();
        // Get the transaction ID associated with the program and edition.
        let tx_id = self.ledger.find_latest_transaction_id_from_program_id_and_edition(id, edition)?;
        // Get the optional program owner associated with the program.
        // Note: The owner is only available after `ConsensusVersion::V9`.
        let program_owner = match &tx_id {
            Some(tid) => self
                .ledger
                .vm()
                .block_store()
                .transaction_store()
                .deployment_store()
                .get_deployment(tid)?
                .and_then(|deployment| deployment.program_owner()),
            None => None,
        };
        Ok(ErasedJson::new(json!({
            "program": program,
            "edition": edition,
            "transaction_id": tx_id,
            "program_owner": program_owner,
        })))
    }

    /// GET /<network>/program/{programID}/latest_edition
    pub(crate) async fn get_latest_program_edition(
        State(rest): State<Self>,
        Path(id): Path<ProgramID<N>>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.get_latest_edition_for_program(&id)?))
    }

    /// GET /<network>/program/{programID}/mappings
    pub(crate) async fn get_mapping_names(
        State(rest): State<Self>,
        Path(id): Path<ProgramID<N>>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.vm().finalize_store().get_mapping_names_confirmed(&id)?))
    }

    /// GET /<network>/program/{programID}/mapping/{mappingName}/{mappingKey}
    /// GET /<network>/program/{programID}/mapping/{mappingName}/{mappingKey}?metadata={true}
    pub(crate) async fn get_mapping_value(
        State(rest): State<Self>,
        Path((id, name, key)): Path<(ProgramID<N>, Identifier<N>, Plaintext<N>)>,
        metadata: Query<Metadata>,
    ) -> Result<ErasedJson, RestError> {
        // Retrieve the mapping value.
        let mapping_value = rest.ledger.vm().finalize_store().get_value_confirmed(id, name, &key)?;

        // Check if metadata is requested and return the value with metadata if so.
        if metadata.metadata.unwrap_or(false) {
            return Ok(ErasedJson::new(json!({
                "data": mapping_value,
                "height": rest.ledger.latest_height(),
            })));
        }

        // Return the value without metadata.
        Ok(ErasedJson::new(mapping_value))
    }

    /// GET /<network>/program/{programID}/mapping/{mappingName}?all={true}&metadata={true}
    pub(crate) async fn get_mapping_values(
        State(rest): State<Self>,
        Path((id, name)): Path<(ProgramID<N>, Identifier<N>)>,
        metadata: Query<Metadata>,
    ) -> Result<ErasedJson, RestError> {
        // Return an error if the `all` query parameter is not set to `true`.
        if metadata.all != Some(true) {
            return Err(RestError::bad_request(anyhow!(
                "Invalid query parameter. At this time, 'all=true' must be included"
            )));
        }

        // Retrieve the latest height.
        let height = rest.ledger.latest_height();

        // Retrieve all the mapping values from the mapping.
        match tokio::task::spawn_blocking(move || rest.ledger.vm().finalize_store().get_mapping_confirmed(id, name))
            .await
        {
            Ok(Ok(mapping_values)) => {
                // Check if metadata is requested and return the mapping with metadata if so.
                if metadata.metadata.unwrap_or(false) {
                    return Ok(ErasedJson::new(json!({
                        "data": mapping_values,
                        "height": height,
                    })));
                }

                // Return the full mapping without metadata.
                Ok(ErasedJson::new(mapping_values))
            }
            Ok(Err(err)) => Err(RestError::internal_server_error(err.context("Unable to read mapping"))),
            Err(err) => Err(RestError::internal_server_error(anyhow!("Tokio error: {err}"))),
        }
    }

    /// GET /<network>/statePath/{commitment}
    pub(crate) async fn get_state_path_for_commitment(
        State(rest): State<Self>,
        Path(commitment): Path<Field<N>>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.get_state_path_for_commitment(&commitment)?))
    }

    /// GET /<network>/statePaths?commitments=cm1,cm2,...
    pub(crate) async fn get_state_paths_for_commitments(
        State(rest): State<Self>,
        Query(commitments): Query<Commitments>,
    ) -> Result<ErasedJson, RestError> {
        // Retrieve the number of commitments.
        let num_commitments = commitments.commitments.len();
        // Return an error if no commitments are provided.
        if num_commitments == 0 {
            return Err(RestError::unprocessable_entity(anyhow!("No commitments provided")));
        }
        // Return an error if the number of commitments exceeds the maximum allowed.
        if num_commitments > N::MAX_INPUTS {
            return Err(RestError::unprocessable_entity(anyhow!(
                "Too many commitments provided (max: {}, got: {})",
                N::MAX_INPUTS,
                num_commitments
            )));
        }

        // Deserialize the commitments from the query.
        let commitments = commitments
            .commitments
            .iter()
            .map(|s| {
                s.parse::<Field<N>>()
                    .map_err(|err| RestError::unprocessable_entity(err.context(format!("Invalid commitment: {s}"))))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ErasedJson::new(rest.ledger.get_state_paths_for_commitments(&commitments)?))
    }

    /// GET /<network>/stateRoot/latest
    pub(crate) async fn get_state_root_latest(State(rest): State<Self>) -> ErasedJson {
        ErasedJson::new(rest.ledger.latest_state_root())
    }

    /// GET /<network>/stateRoot/{height}
    pub(crate) async fn get_state_root(
        State(rest): State<Self>,
        Path(height): Path<u32>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.get_state_root(height)?))
    }

    /// GET /<network>/find/blockHash/{transactionID}
    pub(crate) async fn find_block_hash(
        State(rest): State<Self>,
        Path(tx_id): Path<N::TransactionID>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.find_block_hash(&tx_id)?))
    }

    /// GET /<network>/find/blockHeight/{stateRoot}
    pub(crate) async fn find_block_height_from_state_root(
        State(rest): State<Self>,
        Path(state_root): Path<N::StateRoot>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.find_block_height_from_state_root(state_root)?))
    }

    /// GET /<network>/find/transactionID/deployment/{programID}
    pub(crate) async fn find_latest_transaction_id_from_program_id(
        State(rest): State<Self>,
        Path(program_id): Path<ProgramID<N>>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.find_latest_transaction_id_from_program_id(&program_id)?))
    }

    /// GET /<network>/find/transactionID/deployment/{programID}/{edition}
    pub(crate) async fn find_transaction_id_from_program_id_and_edition(
        State(rest): State<Self>,
        Path((program_id, edition)): Path<(ProgramID<N>, u16)>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.find_latest_transaction_id_from_program_id_and_edition(&program_id, edition)?))
    }

    /// GET /<network>/find/transactionID/{transitionID}
    pub(crate) async fn find_transaction_id_from_transition_id(
        State(rest): State<Self>,
        Path(transition_id): Path<N::TransitionID>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.find_transaction_id_from_transition_id(&transition_id)?))
    }

    /// GET /<network>/find/transitionID/{inputOrOutputID}
    pub(crate) async fn find_transition_id(
        State(rest): State<Self>,
        Path(input_or_output_id): Path<Field<N>>,
    ) -> Result<ErasedJson, RestError> {
        Ok(ErasedJson::new(rest.ledger.find_transition_id(&input_or_output_id)?))
    }

    // /// POST /<network>/transaction/broadcast
    // /// POST /<network>/transaction/broadcast?check_transaction={true}
    pub(crate) async fn transaction_broadcast(
        State(rest): State<Self>,
        check_transaction: Query<CheckTransaction>,
        json_result: Result<Json<Transaction<N>>, JsonRejection>,
    ) -> Result<impl axum::response::IntoResponse, RestError> {
        let Json(tx) = match json_result {
            Ok(json) => json,
            Err(JsonRejection::JsonDataError(err)) => {
                // For JsonDataError, return 422 to let transaction validation handle it.
                return Err(RestError::unprocessable_entity(anyhow!("Invalid transaction data: {err}")));
            }
            Err(other_rejection) => return Err(other_rejection.into()),
        };
        let tx_id = tx.id();

        // If the transaction exceeds the transaction size limit, return an error.
        // The buffer is initially roughly sized to hold a `transfer_public`.
        // Most transactions will be smaller and this reduces unnecessary allocations.
        let buffer = Vec::with_capacity(3000);
        if tx.write_le(LimitedWriter::new(buffer, N::LATEST_MAX_TRANSACTION_SIZE())).is_err() {
            return Err(RestError::bad_request(anyhow!("Transaction size exceeds the byte limit")));
        }

        // Determine if we need to check the transaction.
        let check_transaction = check_transaction.check_transaction.unwrap_or(true);

        if check_transaction {
            // Select counter and limit based on transaction type.
            let (counter, limit, err_msg) = if tx.is_execute() {
                (
                    &rest.num_verifying_executions,
                    VM::<N, C>::MAX_PARALLEL_EXECUTE_VERIFICATIONS,
                    "Too many execution verifications in progress",
                )
            } else {
                (
                    &rest.num_verifying_deploys,
                    VM::<N, C>::MAX_PARALLEL_DEPLOY_VERIFICATIONS,
                    "Too many deploy verifications in progress",
                )
            };

            // Try to acquire a slot.
            if counter
                .fetch_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |val| {
                        if val < limit { Some(val + 1) } else { None }
                    },
                )
                .is_err()
            {
                return Err(RestError::too_many_requests(anyhow!("{err_msg}")));
            }
            // Perform the check.
            let res = rest
                .ledger
                .check_transaction_basic(&tx, None, &mut rand::rng())
                .map_err(|err| RestError::unprocessable_entity(err.context("Invalid transaction")));

            // Release the slot.
            counter.fetch_sub(1, Ordering::Relaxed);
            // Propagate error if any.
            res?;
        }
        // Create a block with the transaction if the manual block creation feature is not enabled.
        if !rest.manual_block_creation {
            // Prepare and advance in a single blocking task to prevent concurrent broadcasts
            // from both preparing a block at the same height and racing to advance the ledger.
            tokio::task::spawn_blocking(move || -> Result<(), RestError> {
                let _guard = rest.block_creation_lock.lock();
                let (new_block, aborted_transaction_ids) =
                    prepare_beacon_block_with_limits(&rest.ledger, &rest.private_key, vec![tx], &mut rand::rng())?;
                ensure_no_aborted_transactions(&aborted_transaction_ids)?;
                rest.ledger
                    .advance_to_next_block(&new_block)
                    .map_err(|e| RestError::internal_server_error(anyhow!("Failed to advance block: {e}")))
            })
            .await
            .map_err(|e| RestError::internal_server_error(anyhow!("Task panicked: {}", e)))??;
            return Ok((StatusCode::OK, ErasedJson::new(tx_id)));
        }

        // Add the transaction to the Rest buffer.
        {
            let mut buffer = rest.buffer.lock();
            buffer.push(tx);
        }

        Ok((StatusCode::OK, ErasedJson::new(tx_id)))
    }

    /// POST /<network>/snapshot
    /// Body (optional): `{"name": "my-snapshot"}`
    /// Saves a snapshot of the current ledger to `{storage}-snapshots/{name}/`.
    /// Returns 400 if the devnode is running in-memory (no storage path).
    pub(crate) async fn create_snapshot(
        State(rest): State<Self>,
        Json(req): Json<serde_json::Value>,
    ) -> Result<ErasedJson, RestError> {
        let storage_path = rest.storage_path.clone().ok_or_else(|| {
            RestError::bad_request(anyhow!("Snapshots require persistent storage (start with --storage)"))
        })?;

        let height = rest.ledger.latest_height();

        let name = req.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
        if let Some(ref n) = name {
            if n.contains('/') || n.contains('\\') || n.contains("..") {
                return Err(RestError::bad_request(anyhow!(
                    "Invalid snapshot name: must not contain path separators or '..'"
                )));
            }
        }
        let name = name.unwrap_or_else(|| format!("snapshot-{height}"));

        let snapshots_dir = snapshots_sibling_dir(&storage_path);
        let snapshot_path = snapshots_dir.join(&name);

        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&snapshots_dir)
                .map_err(|e| RestError::internal_server_error(anyhow!("Failed to create snapshots directory: {e}")))?;
            rest.ledger
                .backup_database(&snapshot_path)
                .map_err(|e| RestError::internal_server_error(anyhow!("Failed to create snapshot: {e}")))?;
            Ok::<_, RestError>(())
        })
        .await
        .map_err(|e| RestError::internal_server_error(anyhow!("Task panicked: {e}")))??;

        Ok(ErasedJson::new(json!({ "name": name, "height": height })))
    }

    /// GET /<network>/snapshots
    /// Lists available snapshots alongside the block height recorded in their name.
    /// Returns 400 if the devnode is running in-memory (no storage path).
    pub(crate) async fn list_snapshots(State(rest): State<Self>) -> Result<ErasedJson, RestError> {
        let storage_path = rest.storage_path.clone().ok_or_else(|| {
            RestError::bad_request(anyhow!("Snapshots require persistent storage (start with --storage)"))
        })?;

        let snapshots_dir = snapshots_sibling_dir(&storage_path);

        if !snapshots_dir.exists() {
            return Ok(ErasedJson::new(json!([])));
        }

        let snapshots = tokio::task::spawn_blocking(move || {
            let mut entries = vec![];
            for entry in std::fs::read_dir(&snapshots_dir)
                .map_err(|e| RestError::internal_server_error(anyhow!("Failed to read snapshots directory: {e}")))?
            {
                let entry = entry
                    .map_err(|e| RestError::internal_server_error(anyhow!("Failed to read snapshot entry: {e}")))?;
                if entry.path().is_dir() {
                    entries.push(entry.file_name().to_string_lossy().to_string());
                }
            }
            entries.sort();
            Ok::<_, RestError>(entries)
        })
        .await
        .map_err(|e| RestError::internal_server_error(anyhow!("Task panicked: {e}")))??;

        Ok(ErasedJson::new(json!(snapshots)))
    }

    /// POST /<network>/shutdown
    pub(crate) async fn shutdown(ConnectInfo(addr): ConnectInfo<SocketAddr>, State(rest): State<Self>) -> StatusCode {
        if !addr.ip().is_loopback() {
            return StatusCode::FORBIDDEN;
        }
        tracing::info!("Shutdown requested via REST API");
        if let Some(tx) = rest.shutdown_tx.lock().take() {
            let _ = tx.send(());
        }
        StatusCode::OK
    }

    /// POST /{network}/create_block
    pub(crate) async fn create_block(
        State(rest): State<Self>,
        Json(req): Json<CreateBlockRequest>,
    ) -> Result<ErasedJson, RestError> {
        // Determine the number of blocks to create.
        let num_blocks = req.num_blocks.unwrap_or(1);
        if num_blocks == 0 {
            return Err(RestError::bad_request(anyhow!("num_blocks must be at least 1")));
        }
        const MAX_BLOCKS_PER_REQUEST: u32 = 1000;
        if num_blocks > MAX_BLOCKS_PER_REQUEST {
            return Err(RestError::bad_request(anyhow!("num_blocks exceeds maximum ({MAX_BLOCKS_PER_REQUEST})")));
        }

        // Iterate and create the specified number of blocks.
        // Return the last created block.
        let last_block = tokio::task::spawn_blocking(move || -> Result<ErasedJson, RestError> {
            let _guard = rest.block_creation_lock.lock();
            let mut last_block = None;

            // Copy all unconfirmed transactions from the buffer. Remove them only after the block advances.
            let mut unconfirmed_txs = Some({
                let buffer = rest.buffer.lock();
                buffer.clone()
            });

            for _ in 0..num_blocks {
                let txs = unconfirmed_txs.take().unwrap_or_default();
                let num_txs = txs.len();

                // Prepare the new block.  Note that transactions in the buffer are added to the first block.
                // If there are no transactions left in the buffer, create an empty block.
                let (new_block, aborted_transaction_ids) =
                    prepare_beacon_block_with_limits(&rest.ledger, &rest.private_key, txs, &mut rand::rng())?;
                if let Err(error) = ensure_no_aborted_transactions(&aborted_transaction_ids) {
                    rest.buffer.lock().retain(|tx| !aborted_transaction_ids.contains(&tx.id()));
                    return Err(error);
                }

                // Update the ledger to the new block.
                rest.ledger
                    .advance_to_next_block(&new_block)
                    .map_err(|e| RestError::internal_server_error(anyhow!("Failed to advance block: {}", e)))?;

                // Remove the transactions that were committed. Transactions received during block creation remain buffered.
                if num_txs > 0 {
                    rest.buffer.lock().drain(..num_txs);
                }

                last_block = Some(new_block);
            }

            Ok(ErasedJson::new(last_block.unwrap()))
        })
        .await
        .map_err(|e| RestError::internal_server_error(anyhow!("Task panicked: {}", e)))??;

        Ok(last_block)
    }

    /// POST /{network}/program/{id}/view/{functionName}/{height}
    ///
    /// Evaluates a view function against the ledger state at the given block `height`.
    pub(crate) async fn evaluate_view(
        State(rest): State<Self>,
        Path((program_id, view_name, height)): Path<ViewFunctionRoute<N>>,
        json_result: Result<Json<Vec<String>>, JsonRejection>,
    ) -> Result<ErasedJson, RestError> {
        let Json(raw_inputs) = match json_result {
            Ok(json) => json,
            Err(err) => return Err(RestError::unprocessable_entity(anyhow!("Invalid request body: {err}"))),
        };

        let inputs = parse_view_inputs(&raw_inputs)?;

        let outputs = match tokio::task::spawn_blocking(move || {
            rest.ledger.vm().evaluate_view_at_height(program_id, view_name, inputs, height)
        })
        .await
        {
            Ok(Ok(outputs)) => outputs,
            Ok(Err(err)) => {
                return Err(RestError::bad_request(
                    err.context(format!("Failed to evaluate view '{view_name}' for '{program_id}' at height {height}")),
                ));
            }
            Err(err) => return Err(RestError::internal_server_error(anyhow!("Tokio error: {err}"))),
        };

        let output_strings: Vec<String> = outputs.iter().map(|v| v.to_string()).collect();

        Ok(ErasedJson::new(output_strings))
    }

    /// POST /{network}/program/{id}/view/{functionName}
    ///
    /// Evaluates a view function against the ledger state at the latest block height.
    pub(crate) async fn evaluate_view_latest(
        State(rest): State<Self>,
        Path((program_id, view_name)): Path<(ProgramID<N>, Identifier<N>)>,
        metadata: Query<Metadata>,
        json_result: Result<Json<Vec<String>>, JsonRejection>,
    ) -> Result<ErasedJson, RestError> {
        let Json(raw_inputs) = match json_result {
            Ok(json) => json,
            Err(err) => return Err(RestError::unprocessable_entity(anyhow!("Invalid request body: {err}"))),
        };

        let inputs = parse_view_inputs(&raw_inputs)?;

        let (outputs, height) = match tokio::task::spawn_blocking(move || {
            let block = rest.ledger.latest_block();
            let height = block.height();

            let block_timestamp =
                (height >= N::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap_or_default()).then_some(block.timestamp());
            let state = FinalizeGlobalState::new::<N>(
                block.round(),
                height,
                block_timestamp,
                block.cumulative_weight(),
                block.cumulative_proof_target(),
                block.previous_hash(),
                None,
                None,
            )?;

            let stack = rest.ledger.vm().process().get_stack(program_id)?;
            let outputs = stack.evaluate_view(state, rest.ledger.vm().finalize_store(), &view_name, inputs)?;

            Ok::<_, anyhow::Error>((outputs, height))
        })
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(err)) => {
                return Err(RestError::bad_request(err.context(format!(
                    "Failed to evaluate view '{view_name}' for '{program_id}' at the latest height"
                ))));
            }
            Err(err) => return Err(RestError::internal_server_error(anyhow!("Tokio error: {err}"))),
        };

        let output_strings: Vec<String> = outputs.iter().map(|v| v.to_string()).collect();

        if metadata.metadata.unwrap_or(false) {
            return Ok(ErasedJson::new(json!({
                "data": output_strings,
                "height": height,
            })));
        }

        Ok(ErasedJson::new(output_strings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::FUNDED_ACCOUNTS;
    use aleo_std_storage::StorageMode;
    use axum::response::IntoResponse;
    use snarkvm::{
        ledger::store::helpers::memory::ConsensusMemory,
        prelude::{
            Address, Certificate, Deployment, Fee, FromBytes, Ledger, PrivateKey, ProgramOwner, TestnetV0,
            VerifyingKey, deployment_cost,
        },
    };
    use std::{str::FromStr, sync::Arc};

    fn placeholder_deployment(
        ledger: &Ledger<TestnetV0, ConsensusMemory<TestnetV0>>,
        private_key: &PrivateKey<TestnetV0>,
        program_name: &str,
        density: u64,
        rng: &mut (impl rand::Rng + rand::CryptoRng),
    ) -> Transaction<TestnetV0> {
        placeholder_deployment_with_constructor(
            ledger,
            private_key,
            program_name,
            density,
            "    assert.eq true true;\n",
            rng,
        )
    }

    fn placeholder_deployment_with_constructor(
        ledger: &Ledger<TestnetV0, ConsensusMemory<TestnetV0>>,
        private_key: &PrivateKey<TestnetV0>,
        program_name: &str,
        density: u64,
        constructor: &str,
        rng: &mut (impl rand::Rng + rand::CryptoRng),
    ) -> Transaction<TestnetV0> {
        placeholder_deployment_with_key_counts(ledger, private_key, program_name, density, 1, None, constructor, rng)
    }

    fn placeholder_deployment_with_key_counts(
        ledger: &Ledger<TestnetV0, ConsensusMemory<TestnetV0>>,
        private_key: &PrivateKey<TestnetV0>,
        program_name: &str,
        density: u64,
        num_variables: u64,
        num_constraints: Option<usize>,
        constructor: &str,
        rng: &mut (impl rand::Rng + rand::CryptoRng),
    ) -> Transaction<TestnetV0> {
        const PLACEHOLDER_CERTIFICATE: &str = "certificate1qyqsqqqqqqqqqqxvwszp09v860w62s2l4g6eqf0kzppyax5we36957ywqm2dplzwvvlqg0kwlnmhzfatnax7uaqt7yqqqw0sc4u";

        let program = Program::from_str(&format!(
            "program {program_name}.aleo;\n\nfunction run:\n    assert.eq true true;\n\nconstructor:\n{constructor}"
        ))
        .unwrap();
        let function_name = *program.functions().keys().next().unwrap();
        let program_checksum = program.to_checksum();
        let mut circuit_key = TestnetV0::get_credits_verifying_key("fee_public".to_string()).unwrap().as_ref().clone();
        circuit_key.circuit_info.num_non_zero_a = usize::try_from(density).unwrap();
        circuit_key.circuit_info.num_non_zero_b = 0;
        circuit_key.circuit_info.num_non_zero_c = 0;
        if let Some(num_constraints) = num_constraints {
            circuit_key.circuit_info.num_constraints = num_constraints;
        }
        let verifying_key = VerifyingKey::new(Arc::new(circuit_key), num_variables);
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
        let current_consensus_version = TestnetV0::CONSENSUS_VERSION(ledger.latest_height()).unwrap();
        let next_consensus_version = TestnetV0::CONSENSUS_VERSION(ledger.latest_height() + 1).unwrap();
        let (current_base_fee, _) =
            deployment_cost(ledger.vm().process(), &deployment, current_consensus_version).unwrap();
        let (next_base_fee, _) = deployment_cost(ledger.vm().process(), &deployment, next_consensus_version).unwrap();
        let base_fee = current_base_fee.max(next_base_fee);
        let authorization = ledger.vm().authorize_fee_public(private_key, base_fee, 0, deployment_id, rng).unwrap();
        let fee_transition = authorization.transitions().into_values().next().unwrap();
        let fee = Fee::from(fee_transition, ledger.latest_state_root(), None).unwrap();

        Transaction::from_deployment(owner, deployment, fee).unwrap()
    }

    fn test_rest(
        ledger: &Ledger<TestnetV0, ConsensusMemory<TestnetV0>>,
        private_key: PrivateKey<TestnetV0>,
        manual_block_creation: bool,
        buffer: Vec<Transaction<TestnetV0>>,
    ) -> Rest<TestnetV0, ConsensusMemory<TestnetV0>> {
        Rest {
            ledger: ledger.clone(),
            buffer: Arc::new(Mutex::new(buffer)),
            handles: Default::default(),
            num_verifying_deploys: Default::default(),
            num_verifying_executions: Default::default(),
            manual_block_creation,
            private_key,
            block_creation_lock: Default::default(),
            shutdown_tx: Default::default(),
            storage_path: None,
        }
    }

    #[test]
    fn test_beacon_block_limits_match_consensus_versions() {
        let v15_height = TestnetV0::CONSENSUS_HEIGHT(ConsensusVersion::V15).unwrap();
        let v16_height = TestnetV0::CONSENSUS_HEIGHT(ConsensusVersion::V16).unwrap();
        let v18_height = TestnetV0::CONSENSUS_HEIGHT(ConsensusVersion::V18).unwrap();
        let v19_height = TestnetV0::CONSENSUS_HEIGHT(ConsensusVersion::V19).unwrap();
        assert_eq!(beacon_block_limits::<TestnetV0>(v15_height).unwrap(), (None, None));

        let (spend_limit, synthesis_limit) = beacon_block_limits::<TestnetV0>(v16_height).unwrap();
        assert!(spend_limit.is_some());
        assert_eq!(synthesis_limit, None);

        let max_certificates = TestnetV0::MAX_CERTIFICATES.last().unwrap().1 as u64;
        // A minimal subdag contains two availability thresholds, one for each round.
        let min_certificates = max_certificates.saturating_add(2).saturating_div(3).saturating_mul(2);
        let expected_spend_limit =
            min_certificates.saturating_mul(BatchHeader::<TestnetV0>::batch_spend_limit(v18_height));
        let expected_synthesis_limit = ((5_f64 * TestnetV0::SYNTHESIS_PER_SECOND_OF_RUNTIME as f64
            / max_certificates as f64)
            * min_certificates as f64) as u64;

        assert_eq!(
            beacon_block_limits::<TestnetV0>(v18_height).unwrap(),
            (Some(expected_spend_limit), Some(expected_synthesis_limit))
        );
        assert!(beacon_block_limits::<TestnetV0>(v19_height).unwrap().1.is_some());
        assert_eq!(beacon_block_limits::<TestnetV0>(v19_height + 1).unwrap().1, None);
    }

    #[test]
    fn test_deployment_limits_fail_block_requests() {
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

        let synthesis_limit = beacon_block_limits::<TestnetV0>(v18_height).unwrap().1.unwrap();
        let deployment_density = synthesis_limit.saturating_mul(3).saturating_div(5);
        let remaining_density = synthesis_limit.saturating_sub(deployment_density);
        let first_key = PrivateKey::from_str(FUNDED_ACCOUNTS[1].1).unwrap();
        let second_key = PrivateKey::from_str(FUNDED_ACCOUNTS[2].1).unwrap();
        let third_key = PrivateKey::from_str(FUNDED_ACCOUNTS[3].1).unwrap();
        let first = placeholder_deployment(&ledger, &first_key, "limit_first", deployment_density, &mut rng);
        let second = placeholder_deployment(&ledger, &second_key, "limit_second", deployment_density, &mut rng);
        let third = placeholder_deployment(&ledger, &third_key, "limit_third", remaining_density, &mut rng);
        let first_id = first.id();
        let second_id = second.id();
        let third_id = third.id();
        let block = ledger
            .prepare_advance_to_next_beacon_block(
                &beacon_key,
                vec![],
                vec![],
                vec![first.clone(), second.clone(), third.clone()],
                &mut rng,
            )
            .unwrap();

        assert_eq!(block.height(), v18_height);
        assert_eq!(block.transactions().num_accepted(), 3);
        assert_eq!(block.transactions().num_rejected(), 0);
        assert!(block.aborted_transaction_ids().is_empty());
        assert!(block.transactions().get(&first_id).is_some());
        assert!(block.transactions().get(&second_id).is_some());
        assert!(block.transactions().get(&third_id).is_some());

        let consensus_version = TestnetV0::CONSENSUS_VERSION(v18_height).unwrap();
        let deployment_spend =
            transaction_compute_spend_in_microcredits(ledger.vm().process(), &first, consensus_version).unwrap();
        let spend_limit = beacon_block_limits::<TestnetV0>(v18_height).unwrap().0.unwrap();
        assert!(deployment_spend > 0 && deployment_spend <= spend_limit);
        let deployments_within_spend_limit = spend_limit / deployment_spend;
        let spend_candidates =
            (0..=deployments_within_spend_limit).map(|_| (first.clone(), deployment_spend)).collect::<Vec<_>>();
        let (spend_candidates, spend_aborted_transaction_ids) =
            filter_block_spend_limit(&spend_candidates, Some(spend_limit));
        assert_eq!(spend_candidates.len(), usize::try_from(deployments_within_spend_limit).unwrap());
        assert_eq!(spend_aborted_transaction_ids, vec![first_id]);

        let (limited_block, aborted_transaction_ids) = prepare_beacon_block_with_limits(
            &ledger,
            &beacon_key,
            vec![first.clone(), second.clone(), third.clone()],
            &mut rng,
        )
        .unwrap();
        assert_eq!(aborted_transaction_ids, vec![second_id]);
        assert!(limited_block.aborted_transaction_ids().is_empty());
        assert!(limited_block.transactions().get(&first_id).is_some());
        assert!(limited_block.transactions().get(&second_id).is_none());
        assert!(limited_block.transactions().get(&third_id).is_some());

        let shared_key = PrivateKey::from_str(FUNDED_ACCOUNTS[4].1).unwrap();
        let same_payer_overflow = placeholder_deployment_with_constructor(
            &ledger,
            &shared_key,
            "limit_shared_overflow",
            1,
            "    inv 2field into r0;\n",
            &mut rng,
        );
        let same_payer_valid = placeholder_deployment(&ledger, &shared_key, "limit_shared_valid", 1, &mut rng);
        let same_payer_overflow_id = same_payer_overflow.id();
        let same_payer_valid_id = same_payer_valid.id();
        let same_payer_overflow_spend =
            transaction_compute_spend_in_microcredits(ledger.vm().process(), &same_payer_overflow, consensus_version)
                .unwrap();
        let same_payer_valid_spend =
            transaction_compute_spend_in_microcredits(ledger.vm().process(), &same_payer_valid, consensus_version)
                .unwrap();
        assert!(same_payer_overflow_spend > same_payer_valid_spend);

        // Without spend filtering, snarkVM accepts the first deployment and aborts the second deployment because both
        // use the same public fee payer.
        let raw_same_payer_block = ledger
            .prepare_advance_to_next_beacon_block(
                &beacon_key,
                vec![],
                vec![],
                vec![same_payer_overflow.clone(), same_payer_valid.clone()],
                &mut rng,
            )
            .unwrap();
        assert_eq!(raw_same_payer_block.aborted_transaction_ids(), &[same_payer_valid_id]);
        assert!(raw_same_payer_block.transactions().get(&same_payer_overflow_id).is_some());

        let (limited_same_payer_block, aborted_transaction_ids) = prepare_beacon_block_with_limit_values(
            &ledger,
            &beacon_key,
            vec![same_payer_overflow, same_payer_valid],
            Some(same_payer_valid_spend),
            Some(synthesis_limit),
            &mut rng,
        )
        .unwrap();
        assert_eq!(aborted_transaction_ids, vec![same_payer_overflow_id]);
        assert!(limited_same_payer_block.aborted_transaction_ids().is_empty());
        assert!(limited_same_payer_block.transactions().get(&same_payer_overflow_id).is_none());
        assert!(limited_same_payer_block.transactions().get(&same_payer_valid_id).is_some());

        let oversized =
            placeholder_deployment(&ledger, &first_key, "limit_oversized", synthesis_limit.saturating_add(1), &mut rng);
        let oversized_id = oversized.id();
        let initial_height = ledger.latest_height();
        let rest = test_rest(&ledger, beacon_key, false, vec![]);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let response = runtime.block_on(async {
            match Rest::transaction_broadcast(
                State(rest.clone()),
                Query(CheckTransaction { check_transaction: Some(false) }),
                Ok(Json(oversized)),
            )
            .await
            {
                Ok(_) => panic!("oversized deployment broadcast succeeded"),
                Err(error) => error.into_response(),
            }
        });

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(ledger.latest_height(), initial_height);
        assert!(rest.buffer.lock().is_empty());
        let response_body = runtime.block_on(axum::body::to_bytes(response.into_body(), usize::MAX)).unwrap();
        assert!(String::from_utf8(response_body.to_vec()).unwrap().contains(&oversized_id.to_string()));

        let rest = test_rest(&ledger, beacon_key, true, vec![first, second, third]);
        let response = runtime.block_on(async {
            match Rest::create_block(State(rest.clone()), Json(CreateBlockRequest { num_blocks: Some(1) })).await {
                Ok(_) => panic!("block creation accepted an aborted deployment"),
                Err(error) => error.into_response(),
            }
        });

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(ledger.latest_height(), initial_height);
        assert_eq!(rest.buffer.lock().iter().map(Transaction::id).collect::<Vec<_>>(), vec![first_id, third_id]);

        let _ = runtime
            .block_on(Rest::create_block(State(rest.clone()), Json(CreateBlockRequest { num_blocks: Some(1) })))
            .expect("valid buffered deployment should create a block");
        assert_eq!(ledger.latest_height(), v18_height);
        assert!(rest.buffer.lock().is_empty());
        assert!(ledger.latest_block().transactions().get(&first_id).is_some());
        assert!(ledger.latest_block().transactions().get(&third_id).is_some());

        let v19_height = TestnetV0::CONSENSUS_HEIGHT(ConsensusVersion::V19).unwrap();
        let block = ledger.prepare_advance_to_next_beacon_block(&beacon_key, vec![], vec![], vec![], &mut rng).unwrap();
        ledger.advance_to_next_block(&block).unwrap();
        assert_eq!(ledger.latest_height(), v19_height);

        let over_variables = placeholder_deployment_with_key_counts(
            &ledger,
            &first_key,
            "over_variables",
            1,
            TestnetV0::MAX_DEPLOYMENT_VARIABLES_V2 + 1,
            None,
            "    assert.eq true true;\n",
            &mut rng,
        );
        let over_constraints = placeholder_deployment_with_key_counts(
            &ledger,
            &second_key,
            "over_constraints",
            1,
            1,
            Some(usize::try_from(TestnetV0::MAX_DEPLOYMENT_CONSTRAINTS_V2 + 1).unwrap()),
            "    assert.eq true true;\n",
            &mut rng,
        );
        assert!(
            check_deployment_limits(over_variables.deployment().unwrap(), ConsensusVersion::V19)
                .unwrap_err()
                .to_string()
                .contains("combined variables exceeds the deployment limit")
        );
        assert!(
            check_deployment_limits(over_constraints.deployment().unwrap(), ConsensusVersion::V19)
                .unwrap_err()
                .to_string()
                .contains("combined constraints exceeds the deployment limit")
        );

        let rest = test_rest(&ledger, beacon_key, false, vec![]);
        for transaction in [over_variables, over_constraints] {
            let transaction_id = transaction.id();
            let response = runtime.block_on(async {
                match Rest::transaction_broadcast(
                    State(rest.clone()),
                    Query(CheckTransaction { check_transaction: Some(false) }),
                    Ok(Json(transaction)),
                )
                .await
                {
                    Ok(_) => panic!("over-limit deployment broadcast succeeded"),
                    Err(error) => error.into_response(),
                }
            });

            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(ledger.latest_height(), v19_height);
            let response_body = runtime.block_on(axum::body::to_bytes(response.into_body(), usize::MAX)).unwrap();
            assert!(String::from_utf8(response_body.to_vec()).unwrap().contains(&transaction_id.to_string()));
        }
    }
}
