// SPDX-License-Identifier: Apache-2.0

//! Contract-local replay of canonically included Midnight transactions.
//!
//! This module deliberately contains no transport. A node adapter must provide
//! every successful Midnight extrinsic from the deployment block onward, in
//! canonical block/extrinsic order, together with the Midnight pallet's applied
//! operation events. The replay engine verifies the official inner transaction
//! hash, matches the pallet's canonical typed event batches, derives the
//! uniquely applied fallible action set, and executes the
//! official public transcripts against the prior official `ContractState`.

use std::{collections::BTreeSet, error::Error, fmt, io::Cursor};

use midnight_base_crypto::{hash::HashOutput, schnorr::Signature, time::Timestamp};
use midnight_coin_structure::contract::ContractAddress;
use midnight_ledger::structure::{ContractAction, ContractCall, Intent, ProofMarker, Transaction};
use midnight_onchain_runtime::{
    context::{BlockContext, QueryContext},
    cost_model::INITIAL_COST_MODEL,
    state::ContractState,
};
use midnight_serialize::{tagged_deserialize, tagged_serialize};
use midnight_storage::{DefaultDB, storage::Map};
use midnight_transient_crypto::commitment::{Pedersen, PureGeneratorPedersen};
use oxid_passport_vault_application::MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES;

const MAX_REPLAY_TRANSACTIONS: usize = 16_384;
const MAX_REPLAY_ACTIONS_PER_TRANSACTION: usize = 256;
const MAX_REPLAY_TRANSACTION_BYTES: usize = 16 * 1024 * 1024;
const MAX_REPLAY_TOTAL_TRANSACTION_BYTES: usize = 512 * 1024 * 1024;
const MAX_OUTCOME_CANDIDATES: usize = 1_024;

type ProvenTransaction = Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;
type ErasedTransaction = Transaction<(), (), Pedersen, DefaultDB>;
type ErasedIntent = Intent<(), (), Pedersen, DefaultDB>;

/// An operation emitted by the Midnight pallet for an action that actually
/// applied. Addresses are the canonical raw 32-byte contract addresses after
/// decoding the pallet event's tagged address payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanonicalMidnightOperation {
    Call([u8; 32]),
    Deploy([u8; 32]),
    Maintain([u8; 32]),
}

/// Consensus block inputs needed by Midnight public transcripts, expressed as
/// Oxid-owned primitives so the adapter does not expose ledger types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalMidnightBlockContext {
    pub seconds_since_epoch: u64,
    pub uncertainty_seconds: u32,
    pub parent_block_hash: [u8; 32],
    pub prior_block_seconds_since_epoch: u64,
}

impl CanonicalMidnightBlockContext {
    fn ledger_context(self) -> BlockContext {
        BlockContext {
            tblock: Timestamp::from_secs(self.seconds_since_epoch),
            tblock_err: self.uncertainty_seconds,
            parent_block_hash: HashOutput(self.parent_block_hash),
            last_block_time: Timestamp::from_secs(self.prior_block_seconds_since_epoch),
        }
    }
}

/// One successful Midnight extrinsic observed in a canonical finalized node
/// block. Transport code is responsible for verifying `ExtrinsicSuccess`, the
/// pallet outcome event, the block hash, and the event transaction hashes.
#[derive(Clone, Debug)]
pub struct CanonicalMidnightTransaction {
    pub raw_transaction: Vec<u8>,
    pub transaction_hash: [u8; 32],
    pub block_hash: [u8; 32],
    pub block_height: u64,
    pub extrinsic_index: u32,
    pub block_context: CanonicalMidnightBlockContext,
    pub all_applied: bool,
    pub applied_operations: Vec<CanonicalMidnightOperation>,
}

/// Exact official state reconstructed from deployment through the last
/// canonically observed transaction that touched the target contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayedPassportVaultState {
    pub serialized_contract_state: Vec<u8>,
    pub deployment_transaction_hash: [u8; 32],
    pub deployment_block_hash: [u8; 32],
    pub deployment_block_height: u64,
    pub latest_transaction_hash: [u8; 32],
    pub latest_block_hash: [u8; 32],
    pub latest_block_height: u64,
    pub replayed_transaction_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultReplayError {
    CapacityExceeded,
    InvalidOrder,
    InvalidTransaction,
    TransactionHashMismatch,
    OutcomeMismatch,
    AmbiguousOutcome,
    MissingDeployment,
    DuplicateDeployment,
    UnsupportedMaintenance,
    MissingContractState,
    TranscriptRejected,
    EffectsMismatch,
    BalanceOverflow,
    InvalidStateEncoding,
}

impl fmt::Display for PassportVaultReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CapacityExceeded => "Passport Vault replay exceeds a public bound",
            Self::InvalidOrder => "Passport Vault replay history is not in canonical order",
            Self::InvalidTransaction => "Passport Vault replay transaction is invalid",
            Self::TransactionHashMismatch => {
                "Passport Vault replay transaction hash does not match the node event"
            }
            Self::OutcomeMismatch => {
                "Passport Vault replay operations do not match the node outcome"
            }
            Self::AmbiguousOutcome => {
                "Passport Vault replay cannot uniquely identify applied contract actions"
            }
            Self::MissingDeployment => "Passport Vault deployment is missing from replay history",
            Self::DuplicateDeployment => {
                "Passport Vault replay history contains a duplicate deployment"
            }
            Self::UnsupportedMaintenance => {
                "Passport Vault replay encountered an unsupported maintenance action"
            }
            Self::MissingContractState => {
                "Passport Vault call occurred before the authenticated deployment"
            }
            Self::TranscriptRejected => "Passport Vault public transcript replay was rejected",
            Self::EffectsMismatch => {
                "Passport Vault replayed effects do not match the proven transcript"
            }
            Self::BalanceOverflow => "Passport Vault replayed balance is invalid",
            Self::InvalidStateEncoding => {
                "Passport Vault replayed state could not be serialized exactly"
            }
        })
    }
}

impl Error for PassportVaultReplayError {}

#[derive(Clone)]
struct ActionRecord {
    index: usize,
    segment: u16,
    action: ContractAction<(), DefaultDB>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OutcomeCandidate {
    included_actions: Vec<usize>,
}

/// Reconstructs one contract's exact official state from a complete,
/// canonically ordered transaction history. This function does not establish
/// history completeness itself; that is the responsibility of the finalized
/// node block scanner that supplies `history`.
pub fn replay_canonical_passport_vault_history(
    contract_address: [u8; 32],
    history: &[CanonicalMidnightTransaction],
) -> Result<ReplayedPassportVaultState, PassportVaultReplayError> {
    if history.is_empty() || history.len() > MAX_REPLAY_TRANSACTIONS {
        return Err(PassportVaultReplayError::CapacityExceeded);
    }

    let mut total_bytes = 0usize;
    let mut previous_position = None;
    let mut previous_block = None;
    let mut state: Option<ContractState<DefaultDB>> = None;
    let mut deployment_anchor = None;
    let mut latest_anchor = None;
    let mut replayed_transaction_count = 0usize;

    for observation in history {
        let position = (observation.block_height, observation.extrinsic_index);
        if previous_position.is_some_and(|previous| previous >= position) {
            return Err(PassportVaultReplayError::InvalidOrder);
        }
        previous_position = Some(position);
        if let Some((height, hash, context)) = previous_block.as_ref()
            && *height == observation.block_height
            && (*hash != observation.block_hash || context != &observation.block_context)
        {
            return Err(PassportVaultReplayError::InvalidOrder);
        }
        previous_block = Some((
            observation.block_height,
            observation.block_hash,
            observation.block_context,
        ));

        if observation.raw_transaction.is_empty()
            || observation.raw_transaction.len() > MAX_REPLAY_TRANSACTION_BYTES
            || observation.applied_operations.len() > MAX_REPLAY_ACTIONS_PER_TRANSACTION
        {
            return Err(PassportVaultReplayError::CapacityExceeded);
        }
        total_bytes = total_bytes
            .checked_add(observation.raw_transaction.len())
            .ok_or(PassportVaultReplayError::CapacityExceeded)?;
        if total_bytes > MAX_REPLAY_TOTAL_TRANSACTION_BYTES {
            return Err(PassportVaultReplayError::CapacityExceeded);
        }

        let transaction = decode_transaction(&observation.raw_transaction)?;
        if transaction.transaction_hash().0.0 != observation.transaction_hash {
            return Err(PassportVaultReplayError::TransactionHashMismatch);
        }
        let erased = transaction.erase_signatures().erase_proofs();
        let records = action_records(&erased)?;
        let block_context = observation.block_context.ledger_context();
        let included_target_actions = infer_included_target_actions(
            &records,
            contract_address,
            observation.all_applied,
            &observation.applied_operations,
        )?;

        let intents = erased.intents().collect::<Vec<_>>();
        let mut touched = false;

        // Midnight applies every guaranteed transcript before attempting any
        // fallible segment. A later partial failure therefore cannot roll
        // these target-state transitions back.
        for record in &records {
            let ContractAction::Call(call) = &record.action else {
                continue;
            };
            if address_bytes(&call.address) != contract_address {
                continue;
            }
            let Some(transcript) = call.guaranteed_transcript.as_ref() else {
                continue;
            };
            let parent = find_intent(&intents, record.segment)?;
            state = Some(apply_transcript(
                state
                    .take()
                    .ok_or(PassportVaultReplayError::MissingContractState)?,
                call,
                transcript,
                parent,
                &block_context,
            )?);
            touched = true;
        }

        // Apply only fallible actions whose segment is uniquely authenticated
        // by the node's canonical typed operation-event batches.
        for record in &records {
            if !included_target_actions.contains(&record.index) {
                continue;
            }
            match &record.action {
                ContractAction::Call(call) => {
                    if let Some(transcript) = call.fallible_transcript.as_ref() {
                        let parent = find_intent(&intents, record.segment)?;
                        state = Some(apply_transcript(
                            state
                                .take()
                                .ok_or(PassportVaultReplayError::MissingContractState)?,
                            call,
                            transcript,
                            parent,
                            &block_context,
                        )?);
                        touched = true;
                    }
                }
                ContractAction::Deploy(deploy) => {
                    if state.is_some() || deployment_anchor.is_some() {
                        return Err(PassportVaultReplayError::DuplicateDeployment);
                    }
                    state = Some(deploy.initial_state.clone());
                    deployment_anchor = Some((
                        observation.transaction_hash,
                        observation.block_hash,
                        observation.block_height,
                    ));
                    touched = true;
                }
                ContractAction::Maintain(_) => {
                    return Err(PassportVaultReplayError::UnsupportedMaintenance);
                }
            }
        }

        if touched {
            latest_anchor = Some((
                observation.transaction_hash,
                observation.block_hash,
                observation.block_height,
            ));
            replayed_transaction_count = replayed_transaction_count
                .checked_add(1)
                .ok_or(PassportVaultReplayError::CapacityExceeded)?;
        }
    }

    let state = state.ok_or(PassportVaultReplayError::MissingDeployment)?;
    let (deployment_transaction_hash, deployment_block_hash, deployment_block_height) =
        deployment_anchor.ok_or(PassportVaultReplayError::MissingDeployment)?;
    let (latest_transaction_hash, latest_block_hash, latest_block_height) =
        latest_anchor.ok_or(PassportVaultReplayError::MissingDeployment)?;
    let mut serialized_contract_state = Vec::new();
    tagged_serialize(&state, &mut serialized_contract_state)
        .map_err(|_| PassportVaultReplayError::InvalidStateEncoding)?;
    if serialized_contract_state.is_empty()
        || serialized_contract_state.len() > MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES
    {
        return Err(PassportVaultReplayError::CapacityExceeded);
    }

    Ok(ReplayedPassportVaultState {
        serialized_contract_state,
        deployment_transaction_hash,
        deployment_block_hash,
        deployment_block_height,
        latest_transaction_hash,
        latest_block_hash,
        latest_block_height,
        replayed_transaction_count,
    })
}

fn decode_transaction(raw: &[u8]) -> Result<ProvenTransaction, PassportVaultReplayError> {
    let mut cursor = Cursor::new(raw);
    let transaction = tagged_deserialize(&mut cursor)
        .map_err(|_| PassportVaultReplayError::InvalidTransaction)?;
    if cursor.position() != raw.len() as u64 {
        return Err(PassportVaultReplayError::InvalidTransaction);
    }
    Ok(transaction)
}

pub(super) fn transaction_targets_contract(
    raw: &[u8],
    contract_address: [u8; 32],
) -> Result<bool, PassportVaultReplayError> {
    if raw.is_empty() || raw.len() > MAX_REPLAY_TRANSACTION_BYTES {
        return Err(PassportVaultReplayError::CapacityExceeded);
    }
    let transaction = decode_transaction(raw)?;
    let erased = transaction.erase_signatures().erase_proofs();
    Ok(action_records(&erased)?
        .iter()
        .any(|record| action_targets(&record.action, contract_address)))
}

fn action_records(
    transaction: &ErasedTransaction,
) -> Result<Vec<ActionRecord>, PassportVaultReplayError> {
    let actions = transaction.actions().collect::<Vec<_>>();
    if actions.len() > MAX_REPLAY_ACTIONS_PER_TRANSACTION {
        return Err(PassportVaultReplayError::CapacityExceeded);
    }
    Ok(actions
        .into_iter()
        .enumerate()
        .map(|(index, (segment, action))| ActionRecord {
            index,
            segment,
            action,
        })
        .collect())
}

fn infer_included_target_actions(
    records: &[ActionRecord],
    contract_address: [u8; 32],
    all_applied: bool,
    observed: &[CanonicalMidnightOperation],
) -> Result<BTreeSet<usize>, PassportVaultReplayError> {
    let groups = action_groups(records)?;
    let observed_batches = observed_operation_batches(observed)?;
    if all_applied {
        if record_operation_batches(records.iter()) != observed_batches {
            return Err(PassportVaultReplayError::OutcomeMismatch);
        }
        return Ok(records
            .iter()
            .filter(|record| action_targets(&record.action, contract_address))
            .map(|record| record.index)
            .collect());
    }

    let mut candidates = BTreeSet::from([OutcomeCandidate {
        included_actions: Vec::new(),
    }]);
    for group in groups {
        let mut next = BTreeSet::new();
        for candidate in &candidates {
            if group[0].segment != 0
                && candidate_matches_observed_prefix(candidate, records, &observed_batches)
            {
                next.insert(candidate.clone());
            }
            let mut included = candidate.clone();
            included
                .included_actions
                .extend(group.iter().map(|record| record.index));
            if candidate_matches_observed_prefix(&included, records, &observed_batches) {
                next.insert(included);
            }
        }
        if next.is_empty() {
            return Err(PassportVaultReplayError::OutcomeMismatch);
        }
        if next.len() > MAX_OUTCOME_CANDIDATES {
            return Err(PassportVaultReplayError::AmbiguousOutcome);
        }
        candidates = next;
    }

    let target_sets = candidates
        .into_iter()
        .filter(|candidate| {
            record_operation_batches(
                candidate
                    .included_actions
                    .iter()
                    .map(|index| &records[*index]),
            ) == observed_batches
        })
        .map(|candidate| {
            candidate
                .included_actions
                .into_iter()
                .filter(|index| action_targets(&records[*index].action, contract_address))
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>();
    if target_sets.is_empty() {
        return Err(PassportVaultReplayError::OutcomeMismatch);
    }
    if target_sets.len() != 1 {
        return Err(PassportVaultReplayError::AmbiguousOutcome);
    }
    Ok(target_sets
        .into_iter()
        .next()
        .expect("one target action set was established")
        .into_iter()
        .collect())
}

fn candidate_matches_observed_prefix(
    candidate: &OutcomeCandidate,
    records: &[ActionRecord],
    observed: &[Vec<CanonicalMidnightOperation>; 3],
) -> bool {
    let candidate = record_operation_batches(
        candidate
            .included_actions
            .iter()
            .map(|index| &records[*index]),
    );
    candidate
        .iter()
        .zip(observed)
        .all(|(candidate, observed)| observed.starts_with(candidate))
}

fn observed_operation_batches(
    observed: &[CanonicalMidnightOperation],
) -> Result<[Vec<CanonicalMidnightOperation>; 3], PassportVaultReplayError> {
    let mut batches: [Vec<CanonicalMidnightOperation>; 3] = std::array::from_fn(|_| Vec::new());
    let mut previous_batch = 0usize;
    for operation in observed {
        let batch = operation_batch(*operation);
        if batch < previous_batch {
            return Err(PassportVaultReplayError::OutcomeMismatch);
        }
        previous_batch = batch;
        batches[batch].push(*operation);
    }
    Ok(batches)
}

fn record_operation_batches<'a>(
    records: impl IntoIterator<Item = &'a ActionRecord>,
) -> [Vec<CanonicalMidnightOperation>; 3] {
    let mut batches: [Vec<CanonicalMidnightOperation>; 3] = std::array::from_fn(|_| Vec::new());
    for record in records {
        let operation = operation(&record.action);
        batches[operation_batch(operation)].push(operation);
    }
    batches
}

const fn operation_batch(operation: CanonicalMidnightOperation) -> usize {
    match operation {
        CanonicalMidnightOperation::Call(_) => 0,
        CanonicalMidnightOperation::Deploy(_) => 1,
        CanonicalMidnightOperation::Maintain(_) => 2,
    }
}

fn action_groups(
    records: &[ActionRecord],
) -> Result<Vec<Vec<&ActionRecord>>, PassportVaultReplayError> {
    let mut groups: Vec<Vec<&ActionRecord>> = Vec::new();
    let mut seen_segments = BTreeSet::new();
    for record in records {
        if groups
            .last()
            .and_then(|group| group.first())
            .is_some_and(|previous| previous.segment == record.segment)
        {
            groups
                .last_mut()
                .expect("the previous group exists")
                .push(record);
            continue;
        }
        if !seen_segments.insert(record.segment) {
            return Err(PassportVaultReplayError::InvalidTransaction);
        }
        groups.push(vec![record]);
    }
    Ok(groups)
}

fn operation(action: &ContractAction<(), DefaultDB>) -> CanonicalMidnightOperation {
    match action {
        ContractAction::Call(call) => {
            CanonicalMidnightOperation::Call(address_bytes(&call.address))
        }
        ContractAction::Deploy(deploy) => {
            CanonicalMidnightOperation::Deploy(address_bytes(&deploy.address()))
        }
        ContractAction::Maintain(update) => {
            CanonicalMidnightOperation::Maintain(address_bytes(&update.address))
        }
    }
}

fn action_targets(action: &ContractAction<(), DefaultDB>, target: [u8; 32]) -> bool {
    match action {
        ContractAction::Call(call) => address_bytes(&call.address) == target,
        ContractAction::Deploy(deploy) => address_bytes(&deploy.address()) == target,
        ContractAction::Maintain(update) => address_bytes(&update.address) == target,
    }
}

fn find_intent(
    intents: &[(u16, ErasedIntent)],
    segment: u16,
) -> Result<&ErasedIntent, PassportVaultReplayError> {
    intents
        .iter()
        .find_map(|(candidate, intent)| (*candidate == segment).then_some(intent))
        .ok_or(PassportVaultReplayError::InvalidTransaction)
}

fn apply_transcript(
    contract: ContractState<DefaultDB>,
    call: &ContractCall<(), DefaultDB>,
    transcript: &midnight_onchain_runtime::transcript::Transcript<DefaultDB>,
    parent: &ErasedIntent,
    block_context: &BlockContext,
) -> Result<ContractState<DefaultDB>, PassportVaultReplayError> {
    let mut query = QueryContext::new(contract.data.clone(), call.address);
    query.call_context = call
        .clone()
        .context(block_context, parent, contract.clone(), &Map::new());
    // Consensus already validated gas and proofs before emitting the node
    // events. Replay needs the deterministic state transition, so it executes
    // without an independent gas limit and then requires the exact proven
    // effects. This avoids trusting indexer-supplied ledger parameters.
    let results = query
        .query(&Vec::from(&transcript.program), None, &INITIAL_COST_MODEL)
        .map_err(|_| PassportVaultReplayError::TranscriptRejected)?;
    if results.context.effects != transcript.effects {
        return Err(PassportVaultReplayError::EffectsMismatch);
    }

    let mut balance = contract.balance.clone();
    for (token_type, value) in transcript.effects.unshielded_inputs.clone() {
        let current = balance.get(&token_type).map(|value| *value).unwrap_or(0);
        balance = balance.insert(
            token_type,
            current
                .checked_add(value)
                .ok_or(PassportVaultReplayError::BalanceOverflow)?,
        );
    }
    for (token_type, value) in transcript.effects.unshielded_outputs.clone() {
        let current = balance.get(&token_type).map(|value| *value).unwrap_or(0);
        balance = balance.insert(
            token_type,
            current
                .checked_sub(value)
                .ok_or(PassportVaultReplayError::BalanceOverflow)?,
        );
    }

    Ok(ContractState {
        data: results.context.state,
        operations: contract.operations,
        maintenance_authority: contract.maintenance_authority,
        balance,
    })
}

fn address_bytes(address: &ContractAddress) -> [u8; 32] {
    address.0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use midnight_ledger::structure::{
        ContractDeploy, Intent, ProofPreimageMarker, ProofVersioned, StandardTransaction,
    };
    use midnight_onchain_runtime::{
        ops::{Key, Op},
        result_mode::ResultModeVerify,
        state::StateValue,
        transcript::Transcript,
    };
    use midnight_storage::{
        arena::Sp,
        storage::{Array, HashMap as LedgerHashMap},
    };
    use midnight_transient_crypto::commitment::PedersenRandomness;
    use midnight_transient_crypto::proofs::Proof;
    use oxid_passport_vault_application::PassportVaultContractStateDecoderPort;
    use rand::{SeedableRng, rngs::StdRng};

    const FIXTURE_HEX: &str =
        include_str!("../../../../fixtures/passport-vault/contract-state-v1.hex");

    fn fixture_state() -> ContractState<DefaultDB> {
        let bytes = hex::decode(FIXTURE_HEX.trim()).expect("fixture is valid hex");
        let mut cursor = Cursor::new(bytes);
        tagged_deserialize(&mut cursor).expect("fixture is an official contract state")
    }

    fn block_context(height: u64) -> CanonicalMidnightBlockContext {
        CanonicalMidnightBlockContext {
            seconds_since_epoch: 1_700_000_000 + height,
            uncertainty_seconds: 30,
            parent_block_hash: [height as u8; 32],
            prior_block_seconds_since_epoch: 1_699_999_994 + height,
        }
    }

    fn serialized_deployment(state: ContractState<DefaultDB>) -> (Vec<u8>, [u8; 32], [u8; 32]) {
        let deploy = ContractDeploy {
            initial_state: state,
            nonce: HashOutput([7; 32]),
        };
        let address = address_bytes(&deploy.address());
        let intent: Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB> =
            Intent {
                guaranteed_unshielded_offer: None,
                fallible_unshielded_offer: None,
                actions: Array::from(vec![deploy.into()]),
                dust_actions: None,
                ttl: Timestamp::from_secs(1_800_000_000),
                binding_commitment: PedersenRandomness::default(),
            };
        let transaction = Transaction::Standard(StandardTransaction {
            network_id: "undeployed".to_owned(),
            intents: LedgerHashMap::from_iter([(1, intent)]),
            guaranteed_coins: None,
            fallible_coins: LedgerHashMap::new(),
            binding_randomness: PedersenRandomness::default(),
        });
        let proven = transaction
            .mock_prove()
            .expect("a deployment-only transaction can use the mock prover");
        let hash = proven.transaction_hash().0.0;
        let mut raw = Vec::new();
        tagged_serialize(&proven, &mut raw).expect("transaction serializes");
        (raw, hash, address)
    }

    fn deployment_observation() -> (CanonicalMidnightTransaction, Vec<u8>, [u8; 32]) {
        let state = fixture_state();
        let mut expected = Vec::new();
        tagged_serialize(&state, &mut expected).expect("state serializes");
        let (raw_transaction, transaction_hash, address) = serialized_deployment(state);
        (
            CanonicalMidnightTransaction {
                raw_transaction,
                transaction_hash,
                block_hash: [9; 32],
                block_height: 42,
                extrinsic_index: 3,
                block_context: block_context(42),
                all_applied: true,
                applied_operations: vec![CanonicalMidnightOperation::Deploy(address)],
            },
            expected,
            address,
        )
    }

    fn serialized_audit_field_call(
        address: [u8; 32],
        guaranteed: bool,
    ) -> (Vec<u8>, [u8; 32], CanonicalMidnightOperation) {
        let program: Vec<Op<ResultModeVerify, DefaultDB>> = midnight_onchain_runtime::Cell_write!(
            [midnight_onchain_runtime::ops::key!(11u8)],
            false,
            u32,
            1u32
        )
        .into();
        let transcript = Transcript {
            gas: Default::default(),
            effects: Default::default(),
            program: Array::from(program),
            version: None,
        };
        let transcript = midnight_storage::arena::Sp::new(transcript);
        let call = ContractCall {
            address: ContractAddress(HashOutput(address)),
            entry_point: b"testIncrementCurrentDay"[..].into(),
            guaranteed_transcript: guaranteed.then(|| transcript.clone()),
            fallible_transcript: (!guaranteed).then_some(transcript),
            communication_commitment: Default::default(),
            proof: ProofVersioned::V2(Proof(Vec::new())),
        };
        let intent: Intent<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB> = Intent {
            guaranteed_unshielded_offer: None,
            fallible_unshielded_offer: None,
            actions: Array::from(vec![call.into()]),
            dust_actions: None,
            ttl: Timestamp::from_secs(1_800_000_000),
            binding_commitment: PureGeneratorPedersen::largest_representable(),
        };
        let transaction = Transaction::Standard(StandardTransaction {
            network_id: "undeployed".to_owned(),
            intents: LedgerHashMap::from_iter([(2, intent)]),
            guaranteed_coins: None,
            fallible_coins: LedgerHashMap::new(),
            binding_randomness: PedersenRandomness::default(),
        });
        let transaction_hash = transaction.transaction_hash().0.0;
        let mut raw = Vec::new();
        tagged_serialize(&transaction, &mut raw).expect("call transaction serializes");
        (
            raw,
            transaction_hash,
            CanonicalMidnightOperation::Call(address),
        )
    }

    #[test]
    fn replays_an_exact_official_deployment_state() {
        let (observation, expected, address) = deployment_observation();
        let result = replay_canonical_passport_vault_history(address, &[observation])
            .expect("canonical deployment replays");
        assert_eq!(result.serialized_contract_state, expected);
        assert_eq!(result.deployment_block_height, 42);
        assert_eq!(result.latest_block_height, 42);
        assert_eq!(result.replayed_transaction_count, 1);
    }

    #[test]
    fn executes_an_official_public_transcript_against_the_replayed_state() {
        let (deployment, _, address) = deployment_observation();
        let (raw_transaction, transaction_hash, operation) =
            serialized_audit_field_call(address, false);
        let call = CanonicalMidnightTransaction {
            raw_transaction,
            transaction_hash,
            block_hash: [10; 32],
            block_height: 43,
            extrinsic_index: 2,
            block_context: block_context(43),
            all_applied: true,
            applied_operations: vec![operation],
        };
        let result = replay_canonical_passport_vault_history(address, &[deployment, call])
            .expect("canonical transcript replays");
        let view = crate::NativePassportVaultContractStateDecoder
            .decode(&result.serialized_contract_state)
            .expect("replayed state retains the Passport Vault layout");
        assert_eq!(view.claim_count, 0);
        assert_eq!(
            view.contract
                .expect("fixture exposes contract audit fields")
                .last_verified_current_day,
            1
        );
        assert_eq!(result.latest_block_height, 43);
        assert_eq!(result.replayed_transaction_count, 2);
    }

    #[test]
    fn replays_guaranteed_effects_when_the_fallible_segment_failed() {
        let (deployment, _, address) = deployment_observation();
        let (raw_transaction, transaction_hash, _) = serialized_audit_field_call(address, true);
        let call = CanonicalMidnightTransaction {
            raw_transaction,
            transaction_hash,
            block_hash: [10; 32],
            block_height: 43,
            extrinsic_index: 2,
            block_context: block_context(43),
            all_applied: false,
            applied_operations: Vec::new(),
        };
        let result = replay_canonical_passport_vault_history(address, &[deployment, call])
            .expect("guaranteed effects survive a failed fallible segment");
        let view = crate::NativePassportVaultContractStateDecoder
            .decode(&result.serialized_contract_state)
            .expect("replayed state retains the Passport Vault layout");
        assert_eq!(
            view.contract
                .expect("fixture exposes contract audit fields")
                .last_verified_current_day,
            1
        );
        assert_eq!(result.replayed_transaction_count, 2);
    }

    #[test]
    fn rejects_non_canonical_observation_order() {
        let (deployment, _, address) = deployment_observation();
        let mut earlier = deployment.clone();
        earlier.block_height -= 1;
        earlier.block_context = block_context(earlier.block_height);
        assert_eq!(
            replay_canonical_passport_vault_history(address, &[deployment, earlier]),
            Err(PassportVaultReplayError::InvalidOrder)
        );
    }

    #[test]
    fn rejects_inconsistent_observations_from_one_block() {
        let (deployment, _, address) = deployment_observation();
        let (raw_transaction, transaction_hash, operation) =
            serialized_audit_field_call(address, false);
        let call = CanonicalMidnightTransaction {
            raw_transaction,
            transaction_hash,
            block_hash: [10; 32],
            block_height: deployment.block_height,
            extrinsic_index: deployment.extrinsic_index + 1,
            block_context: deployment.block_context,
            all_applied: true,
            applied_operations: vec![operation],
        };
        assert_eq!(
            replay_canonical_passport_vault_history(address, &[deployment, call]),
            Err(PassportVaultReplayError::InvalidOrder)
        );
    }

    #[test]
    fn rejects_a_second_target_deployment() {
        let (deployment, _, address) = deployment_observation();
        let mut duplicate = deployment.clone();
        duplicate.block_height += 1;
        duplicate.block_hash = [10; 32];
        duplicate.block_context = block_context(duplicate.block_height);
        assert_eq!(
            replay_canonical_passport_vault_history(address, &[deployment, duplicate]),
            Err(PassportVaultReplayError::DuplicateDeployment)
        );
    }

    #[test]
    fn rejects_a_node_hash_that_does_not_match_the_inner_transaction() {
        let (mut observation, _, address) = deployment_observation();
        observation.transaction_hash = [0; 32];
        assert_eq!(
            replay_canonical_passport_vault_history(address, &[observation]),
            Err(PassportVaultReplayError::TransactionHashMismatch)
        );
    }

    #[test]
    fn rejects_a_node_operation_sequence_that_does_not_match_the_transaction() {
        let (mut observation, _, address) = deployment_observation();
        observation.applied_operations = vec![CanonicalMidnightOperation::Call(address)];
        assert_eq!(
            replay_canonical_passport_vault_history(address, &[observation]),
            Err(PassportVaultReplayError::OutcomeMismatch)
        );
    }

    #[test]
    fn rejects_trailing_transaction_bytes() {
        let (mut observation, _, address) = deployment_observation();
        observation.raw_transaction.push(0);
        assert_eq!(
            replay_canonical_passport_vault_history(address, &[observation]),
            Err(PassportVaultReplayError::InvalidTransaction)
        );
    }

    #[test]
    fn identifies_target_actions_even_without_consulting_applied_events() {
        let (deployment, _, address) = deployment_observation();
        assert!(
            transaction_targets_contract(&deployment.raw_transaction, address)
                .expect("deployment contains its target")
        );
        assert!(
            !transaction_targets_contract(&deployment.raw_transaction, [99; 32])
                .expect("foreign address is absent")
        );
    }

    #[test]
    fn partial_outcome_requires_a_unique_target_action_set() {
        let target = [1; 32];
        let other = [2; 32];
        let records = vec![
            ActionRecord {
                index: 0,
                segment: 1,
                action: synthetic_call(target),
            },
            ActionRecord {
                index: 1,
                segment: 2,
                action: synthetic_call(target),
            },
            ActionRecord {
                index: 2,
                segment: 3,
                action: synthetic_call(other),
            },
        ];
        assert_eq!(
            infer_included_target_actions(
                &records,
                target,
                false,
                &[CanonicalMidnightOperation::Call(target)]
            ),
            Err(PassportVaultReplayError::AmbiguousOutcome)
        );
    }

    #[test]
    fn partial_outcome_may_be_ambiguous_elsewhere_when_target_actions_are_identical() {
        let target = [1; 32];
        let other = [2; 32];
        let records = vec![
            ActionRecord {
                index: 0,
                segment: 1,
                action: synthetic_call(target),
            },
            ActionRecord {
                index: 1,
                segment: 2,
                action: synthetic_call(other),
            },
            ActionRecord {
                index: 2,
                segment: 3,
                action: synthetic_call(other),
            },
        ];
        let included = infer_included_target_actions(
            &records,
            target,
            false,
            &[
                CanonicalMidnightOperation::Call(target),
                CanonicalMidnightOperation::Call(other),
            ],
        )
        .expect("target state is still uniquely determined");
        assert_eq!(included, BTreeSet::from([0]));
    }

    #[test]
    fn matches_the_pallets_typed_event_batches_instead_of_raw_action_order() {
        let deploy = ContractDeploy {
            initial_state: fixture_state(),
            nonce: HashOutput([33; 32]),
        };
        let target = address_bytes(&deploy.address());
        let other = [44; 32];
        let records = vec![
            ActionRecord {
                index: 0,
                segment: 1,
                action: deploy.into(),
            },
            ActionRecord {
                index: 1,
                segment: 2,
                action: synthetic_call(other),
            },
        ];
        let pallet_order = [
            CanonicalMidnightOperation::Call(other),
            CanonicalMidnightOperation::Deploy(target),
        ];
        assert_eq!(
            infer_included_target_actions(&records, target, true, &pallet_order)
                .expect("pallet batch order authenticates all actions"),
            BTreeSet::from([0])
        );
        assert_eq!(
            infer_included_target_actions(
                &records,
                target,
                true,
                &[
                    CanonicalMidnightOperation::Deploy(target),
                    CanonicalMidnightOperation::Call(other),
                ],
            ),
            Err(PassportVaultReplayError::OutcomeMismatch)
        );
    }

    #[test]
    fn infers_partial_segments_across_separate_pallet_event_batches() {
        let deploy = ContractDeploy {
            initial_state: fixture_state(),
            nonce: HashOutput([55; 32]),
        };
        let target = address_bytes(&deploy.address());
        let other = [66; 32];
        let records = vec![
            ActionRecord {
                index: 0,
                segment: 1,
                action: deploy.into(),
            },
            ActionRecord {
                index: 1,
                segment: 2,
                action: synthetic_call(other),
            },
        ];
        assert!(
            infer_included_target_actions(
                &records,
                target,
                false,
                &[CanonicalMidnightOperation::Call(other)],
            )
            .expect("only the call segment applied")
            .is_empty()
        );
    }

    fn synthetic_call(address: [u8; 32]) -> ContractAction<(), DefaultDB> {
        let mut rng = StdRng::seed_from_u64(u64::from(address[0]));
        ContractCall {
            address: ContractAddress(HashOutput(address)),
            entry_point: b"test"[..].into(),
            guaranteed_transcript: None,
            fallible_transcript: None,
            communication_commitment: rand::Rng::r#gen(&mut rng),
            proof: (),
        }
        .into()
    }
}
