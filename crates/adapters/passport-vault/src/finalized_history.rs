// SPDX-License-Identifier: Apache-2.0

//! Complete canonical Passport Vault history acquisition from a finalized node.
//!
//! The configured deployment height is only a hint. This adapter validates the
//! target deployment event at that exact finalized block, walks every canonical
//! block through a captured finalized head, and obtains the historical runtime
//! metadata active for each block from its parent state. Indexer state and
//! transaction-result data are deliberately absent from this boundary.

use std::{collections::BTreeMap, error::Error, fmt, future::Future, io::Cursor, time::Duration};

use midnight_coin_structure::contract::ContractAddress;
use midnight_serialize::tagged_deserialize;
use subxt::{
    Metadata, SubstrateConfig,
    backend::{legacy::LegacyRpcMethods, rpc::RpcClient},
    config::{Config, Hasher, Header},
    dynamic,
    ext::{
        codec::{Compact, Decode},
        subxt_core::{
            blocks::Extrinsics,
            events::{Events, Phase},
            metadata,
            storage::get_address_bytes,
        },
    },
};
use tokio::time::timeout;

use super::{
    live_state::{ensure_tls_provider, validate_node_endpoint},
    replay::{
        CanonicalMidnightBlockContext, CanonicalMidnightOperation, CanonicalMidnightTransaction,
        PassportVaultReplayError, transaction_targets_contract,
    },
};

const RPC_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FINALIZED_HISTORY_BLOCKS: u64 = 1_000_000;
const MAX_TARGET_TRANSACTIONS: usize = 16_384;
const MAX_EXTRINSICS_PER_BLOCK: usize = 4_096;
const MAX_EVENTS_PER_BLOCK: u32 = 16_384;
const MAX_BLOCK_BODY_BYTES: usize = 256 * 1024 * 1024;
const MAX_EVENT_STORAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_RUNTIME_METADATA_BYTES: usize = 32 * 1024 * 1024;
const MAX_TRANSACTION_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONTRACT_ADDRESS_BYTES: usize = 4 * 1024;
const BLOCK_TIME_UNCERTAINTY_SECONDS: u32 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalizedMidnightHistoryCollectorConfigError {
    InvalidNodeEndpoint,
    InvalidDeploymentHeight,
    ClientUnavailable,
}

impl fmt::Display for FinalizedMidnightHistoryCollectorConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidNodeEndpoint => "finalized Midnight history node endpoint is invalid",
            Self::InvalidDeploymentHeight => {
                "finalized Midnight history deployment height is invalid"
            }
            Self::ClientUnavailable => "finalized Midnight history client is unavailable",
        })
    }
}

impl Error for FinalizedMidnightHistoryCollectorConfigError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalizedMidnightHistoryError {
    Unavailable,
    TimedOut,
    InvalidChainState,
    CapacityExceeded,
    DeploymentNotFinalized,
    DeploymentHeightMismatch,
    MissingDeployment,
    DuplicateDeployment,
}

impl fmt::Display for FinalizedMidnightHistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "finalized Midnight history is unavailable",
            Self::TimedOut => "finalized Midnight history request timed out",
            Self::InvalidChainState => "finalized Midnight history is inconsistent",
            Self::CapacityExceeded => "finalized Midnight history exceeds a public bound",
            Self::DeploymentNotFinalized => {
                "Passport Vault deployment is not at or below the finalized head"
            }
            Self::DeploymentHeightMismatch => {
                "Passport Vault deployment does not match the configured finalized height"
            }
            Self::MissingDeployment => {
                "Passport Vault deployment is missing at the configured finalized height"
            }
            Self::DuplicateDeployment => "Passport Vault history contains more than one deployment",
        })
    }
}

impl Error for FinalizedMidnightHistoryError {}

/// A complete target-touching transaction sequence collected while observing
/// every canonical block from deployment through the captured finalized head.
#[derive(Clone, Debug)]
pub struct FinalizedMidnightHistory {
    pub transactions: Vec<CanonicalMidnightTransaction>,
    pub deployment_block_height: u64,
    pub finalized_head_hash: [u8; 32],
    pub finalized_head_height: u64,
    pub finalized_head_time_seconds: u64,
}

/// Native finalized-node collector. The deployment height is an untrusted,
/// caller-supplied discovery hint and is authenticated by the deployment event.
#[derive(Clone, Debug)]
pub struct FinalizedMidnightHistoryCollector {
    node_endpoint: String,
    deployment_block_height: u64,
}

impl FinalizedMidnightHistoryCollector {
    pub fn new(
        node_endpoint: impl AsRef<str>,
        deployment_block_height: u64,
    ) -> Result<Self, FinalizedMidnightHistoryCollectorConfigError> {
        ensure_tls_provider()
            .map_err(|()| FinalizedMidnightHistoryCollectorConfigError::ClientUnavailable)?;
        let node_endpoint = validate_node_endpoint(node_endpoint.as_ref())
            .ok_or(FinalizedMidnightHistoryCollectorConfigError::InvalidNodeEndpoint)?;
        if deployment_block_height == 0 {
            return Err(FinalizedMidnightHistoryCollectorConfigError::InvalidDeploymentHeight);
        }
        Ok(Self {
            node_endpoint,
            deployment_block_height,
        })
    }

    /// Captures one finalized head and scans the exact canonical height range.
    /// Historical metadata is resolved at each block's parent state, so runtime
    /// upgrades do not cause current-schema decoding of old blocks.
    pub async fn collect(
        &self,
        contract_address: [u8; 32],
    ) -> Result<FinalizedMidnightHistory, FinalizedMidnightHistoryError> {
        let client = rpc_result(RpcClient::from_insecure_url(&self.node_endpoint)).await?;
        let rpc = LegacyRpcMethods::<SubstrateConfig>::new(client);
        collect_from_node(&rpc, self.deployment_block_height, contract_address).await
    }
}

#[derive(Clone)]
struct RuntimeSchema {
    metadata: Metadata,
    events_key: Vec<u8>,
}

#[derive(Default)]
struct RuntimeSchemaCache {
    by_spec_version: BTreeMap<u32, RuntimeSchema>,
}

impl RuntimeSchemaCache {
    async fn at(
        &mut self,
        rpc: &LegacyRpcMethods<SubstrateConfig>,
        state_hash: subxt::utils::H256,
    ) -> Result<RuntimeSchema, FinalizedMidnightHistoryError> {
        let version = rpc_result(rpc.state_get_runtime_version(Some(state_hash))).await?;
        if let Some(schema) = self.by_spec_version.get(&version.spec_version) {
            return Ok(schema.clone());
        }
        let response = rpc_result(rpc.state_get_metadata(Some(state_hash))).await?;
        let bytes = response.into_raw();
        if bytes.is_empty() || bytes.len() > MAX_RUNTIME_METADATA_BYTES {
            return Err(FinalizedMidnightHistoryError::CapacityExceeded);
        }
        let metadata = metadata::decode_from(&bytes)
            .map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
        let address = dynamic::storage("System", "Events", Vec::<dynamic::Value>::new());
        let events_key = get_address_bytes(&address, &metadata)
            .map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
        let schema = RuntimeSchema {
            metadata,
            events_key,
        };
        self.by_spec_version
            .insert(version.spec_version, schema.clone());
        Ok(schema)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DecodedMidnightTransaction {
    raw_transaction: Vec<u8>,
    transaction_hash: [u8; 32],
    extrinsic_index: u32,
    all_applied: bool,
    applied_operations: Vec<CanonicalMidnightOperation>,
    targets_contract: bool,
}

#[derive(Clone, Debug)]
struct DecodedFinalizedBlock {
    hash: [u8; 32],
    parent_hash: [u8; 32],
    height: u64,
    timestamp_seconds: u64,
    transactions: Vec<DecodedMidnightTransaction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RelevantEvent {
    ExtrinsicSuccess,
    ExtrinsicFailed,
    Outcome {
        transaction_hash: [u8; 32],
        all_applied: bool,
    },
    Operation {
        transaction_hash: [u8; 32],
        operation: CanonicalMidnightOperation,
    },
}

struct CanonicalHistoryBuilder {
    target: [u8; 32],
    deployment_height: u64,
    finalized_head_hash: [u8; 32],
    finalized_head_height: u64,
    previous_height: u64,
    previous_hash: [u8; 32],
    previous_timestamp_seconds: u64,
    deployment_count: usize,
    transactions: Vec<CanonicalMidnightTransaction>,
}

impl CanonicalHistoryBuilder {
    fn new(
        target: [u8; 32],
        deployment_height: u64,
        finalized_head_hash: [u8; 32],
        finalized_head_height: u64,
        prior_block: &DecodedFinalizedBlock,
    ) -> Result<Self, FinalizedMidnightHistoryError> {
        if prior_block.height.checked_add(1) != Some(deployment_height) {
            return Err(FinalizedMidnightHistoryError::InvalidChainState);
        }
        Ok(Self {
            target,
            deployment_height,
            finalized_head_hash,
            finalized_head_height,
            previous_height: prior_block.height,
            previous_hash: prior_block.hash,
            previous_timestamp_seconds: prior_block.timestamp_seconds,
            deployment_count: 0,
            transactions: Vec::new(),
        })
    }

    fn push(&mut self, block: DecodedFinalizedBlock) -> Result<(), FinalizedMidnightHistoryError> {
        if self.previous_height.checked_add(1) != Some(block.height)
            || block.parent_hash != self.previous_hash
            || block.timestamp_seconds < self.previous_timestamp_seconds
        {
            return Err(FinalizedMidnightHistoryError::InvalidChainState);
        }
        for transaction in block.transactions {
            let event_targets_contract = transaction
                .applied_operations
                .iter()
                .any(|operation| operation_targets(*operation, self.target));
            if event_targets_contract && !transaction.targets_contract {
                return Err(FinalizedMidnightHistoryError::InvalidChainState);
            }
            if !transaction.targets_contract {
                continue;
            }
            let deployment_events = transaction
                .applied_operations
                .iter()
                .filter(|operation| {
                    matches!(operation, CanonicalMidnightOperation::Deploy(address) if *address == self.target)
                })
                .count();
            if deployment_events > 0 && block.height != self.deployment_height {
                return Err(FinalizedMidnightHistoryError::DeploymentHeightMismatch);
            }
            self.deployment_count = self
                .deployment_count
                .checked_add(deployment_events)
                .ok_or(FinalizedMidnightHistoryError::CapacityExceeded)?;
            if self.deployment_count > 1 {
                return Err(FinalizedMidnightHistoryError::DuplicateDeployment);
            }
            if self.transactions.len() == MAX_TARGET_TRANSACTIONS {
                return Err(FinalizedMidnightHistoryError::CapacityExceeded);
            }
            self.transactions.push(CanonicalMidnightTransaction {
                raw_transaction: transaction.raw_transaction,
                transaction_hash: transaction.transaction_hash,
                block_hash: block.hash,
                block_height: block.height,
                extrinsic_index: transaction.extrinsic_index,
                block_context: CanonicalMidnightBlockContext {
                    seconds_since_epoch: block.timestamp_seconds,
                    uncertainty_seconds: BLOCK_TIME_UNCERTAINTY_SECONDS,
                    parent_block_hash: block.parent_hash,
                    prior_block_seconds_since_epoch: self.previous_timestamp_seconds,
                },
                all_applied: transaction.all_applied,
                applied_operations: transaction.applied_operations,
            });
        }
        self.previous_height = block.height;
        self.previous_hash = block.hash;
        self.previous_timestamp_seconds = block.timestamp_seconds;
        Ok(())
    }

    fn finish(self) -> Result<FinalizedMidnightHistory, FinalizedMidnightHistoryError> {
        if self.previous_height != self.finalized_head_height
            || self.previous_hash != self.finalized_head_hash
        {
            return Err(FinalizedMidnightHistoryError::InvalidChainState);
        }
        match self.deployment_count {
            0 => Err(FinalizedMidnightHistoryError::MissingDeployment),
            1 => Ok(FinalizedMidnightHistory {
                transactions: self.transactions,
                deployment_block_height: self.deployment_height,
                finalized_head_hash: self.finalized_head_hash,
                finalized_head_height: self.finalized_head_height,
                finalized_head_time_seconds: self.previous_timestamp_seconds,
            }),
            _ => Err(FinalizedMidnightHistoryError::DuplicateDeployment),
        }
    }
}

async fn collect_from_node(
    rpc: &LegacyRpcMethods<SubstrateConfig>,
    deployment_height: u64,
    contract_address: [u8; 32],
) -> Result<FinalizedMidnightHistory, FinalizedMidnightHistoryError> {
    let finalized_head = rpc_result(rpc.chain_get_finalized_head()).await?;
    let finalized_header =
        required_archive_item(rpc_result(rpc.chain_get_header(Some(finalized_head))).await?)?;
    let finalized_head_height: u64 = finalized_header.number().into();
    let span = finalized_head_height
        .checked_sub(deployment_height)
        .and_then(|difference| difference.checked_add(1))
        .ok_or(FinalizedMidnightHistoryError::DeploymentNotFinalized)?;
    if span > MAX_FINALIZED_HISTORY_BLOCKS {
        return Err(FinalizedMidnightHistoryError::CapacityExceeded);
    }

    let mut schemas = RuntimeSchemaCache::default();
    let prior_height = deployment_height - 1;
    let prior_hash = canonical_hash_at(rpc, prior_height).await?;
    let prior_block = fetch_block(rpc, &mut schemas, prior_hash, false, contract_address).await?;
    let mut builder = CanonicalHistoryBuilder::new(
        contract_address,
        deployment_height,
        finalized_head.0,
        finalized_head_height,
        &prior_block,
    )?;

    for height in deployment_height..=finalized_head_height {
        let hash = if height == finalized_head_height {
            finalized_head
        } else {
            canonical_hash_at(rpc, height).await?
        };
        let block = fetch_block(rpc, &mut schemas, hash, true, contract_address).await?;
        builder.push(block)?;
    }
    builder.finish()
}

async fn canonical_hash_at(
    rpc: &LegacyRpcMethods<SubstrateConfig>,
    height: u64,
) -> Result<subxt::utils::H256, FinalizedMidnightHistoryError> {
    rpc_result(rpc.chain_get_block_hash(Some(height.into())))
        .await?
        .ok_or(FinalizedMidnightHistoryError::InvalidChainState)
}

async fn fetch_block(
    rpc: &LegacyRpcMethods<SubstrateConfig>,
    schemas: &mut RuntimeSchemaCache,
    expected_hash: subxt::utils::H256,
    include_events: bool,
    contract_address: [u8; 32],
) -> Result<DecodedFinalizedBlock, FinalizedMidnightHistoryError> {
    let details =
        required_archive_item(rpc_result(rpc.chain_get_block(Some(expected_hash))).await?)?;
    let height: u64 = details.block.header.number().into();
    let schema_state_hash = if height == 0 {
        expected_hash
    } else {
        details.block.header.parent_hash
    };
    let schema = schemas.at(rpc, schema_state_hash).await?;
    let hasher = <SubstrateConfig as Config>::Hasher::new(&schema.metadata);
    if details.block.header.hash_with(hasher) != expected_hash {
        return Err(FinalizedMidnightHistoryError::InvalidChainState);
    }
    if details.block.extrinsics.len() > MAX_EXTRINSICS_PER_BLOCK {
        return Err(FinalizedMidnightHistoryError::CapacityExceeded);
    }
    let mut body_bytes = 0usize;
    let body = details
        .block
        .extrinsics
        .into_iter()
        .map(|bytes| {
            body_bytes = body_bytes
                .checked_add(bytes.0.len())
                .ok_or(FinalizedMidnightHistoryError::CapacityExceeded)?;
            if body_bytes > MAX_BLOCK_BODY_BYTES {
                return Err(FinalizedMidnightHistoryError::CapacityExceeded);
            }
            Ok(bytes.0)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let extrinsics = Extrinsics::<SubstrateConfig>::decode_from(body, schema.metadata.clone())
        .map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
    let mut timestamp_ms = None;
    let mut midnight_payloads = vec![None; extrinsics.len()];
    for extrinsic in extrinsics.iter() {
        let pallet = extrinsic
            .pallet_name()
            .map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
        let variant = extrinsic
            .variant_name()
            .map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
        match (pallet, variant) {
            ("Timestamp", "set") => {
                if timestamp_ms
                    .replace(decode_timestamp(extrinsic.field_bytes())?)
                    .is_some()
                {
                    return Err(FinalizedMidnightHistoryError::InvalidChainState);
                }
            }
            ("Midnight", "send_mn_transaction") => {
                midnight_payloads[extrinsic.index() as usize] = Some(decode_scale_bytes(
                    extrinsic.field_bytes(),
                    MAX_TRANSACTION_BYTES,
                )?);
            }
            _ => {}
        }
    }
    let timestamp_seconds =
        timestamp_ms.ok_or(FinalizedMidnightHistoryError::InvalidChainState)? / 1_000;

    let mut transactions = Vec::new();
    if include_events {
        let event_bytes = required_archive_item(
            rpc_result(rpc.state_get_storage(&schema.events_key, Some(expected_hash))).await?,
        )?;
        if event_bytes.len() > MAX_EVENT_STORAGE_BYTES {
            return Err(FinalizedMidnightHistoryError::CapacityExceeded);
        }
        let events = Events::<SubstrateConfig>::decode_from(event_bytes, schema.metadata);
        if events.len() > MAX_EVENTS_PER_BLOCK {
            return Err(FinalizedMidnightHistoryError::CapacityExceeded);
        }
        let mut by_extrinsic = vec![Vec::new(); extrinsics.len()];
        for event in events.iter() {
            let event = event.map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
            let Phase::ApplyExtrinsic(index) = event.phase() else {
                continue;
            };
            let slot = by_extrinsic
                .get_mut(index as usize)
                .ok_or(FinalizedMidnightHistoryError::InvalidChainState)?;
            if let Some(event) = decode_relevant_event(
                event.pallet_name(),
                event.variant_name(),
                event.field_bytes(),
            )? {
                slot.push(event);
            }
        }
        for (index, events) in by_extrinsic.iter().enumerate() {
            match midnight_payloads[index].take() {
                Some(raw_transaction) => {
                    if let Some(transaction) = decode_midnight_transaction(
                        raw_transaction,
                        u32::try_from(index)
                            .map_err(|_| FinalizedMidnightHistoryError::CapacityExceeded)?,
                        events,
                        contract_address,
                    )? {
                        transactions.push(transaction);
                    }
                }
                None if events.iter().any(|event| {
                    matches!(
                        event,
                        RelevantEvent::Outcome { .. } | RelevantEvent::Operation { .. }
                    )
                }) =>
                {
                    // Wrapped Midnight calls are visible but their raw inner payload
                    // is not a direct call field. Refuse an incomplete replay.
                    return Err(FinalizedMidnightHistoryError::InvalidChainState);
                }
                None => {}
            }
        }
    }

    Ok(DecodedFinalizedBlock {
        hash: expected_hash.0,
        parent_hash: details.block.header.parent_hash.0,
        height,
        timestamp_seconds,
        transactions,
    })
}

fn decode_relevant_event(
    pallet: &str,
    variant: &str,
    fields: &[u8],
) -> Result<Option<RelevantEvent>, FinalizedMidnightHistoryError> {
    match (pallet, variant) {
        ("System", "ExtrinsicSuccess") => Ok(Some(RelevantEvent::ExtrinsicSuccess)),
        ("System", "ExtrinsicFailed") => Ok(Some(RelevantEvent::ExtrinsicFailed)),
        ("Midnight", "TxApplied") => Ok(Some(RelevantEvent::Outcome {
            transaction_hash: decode_outcome_hash(fields)?,
            all_applied: true,
        })),
        ("Midnight", "TxPartialSuccess") => Ok(Some(RelevantEvent::Outcome {
            transaction_hash: decode_outcome_hash(fields)?,
            all_applied: false,
        })),
        ("Midnight", "ContractCall") => {
            let (transaction_hash, address) = decode_contract_event(fields)?;
            Ok(Some(RelevantEvent::Operation {
                transaction_hash,
                operation: CanonicalMidnightOperation::Call(address),
            }))
        }
        ("Midnight", "ContractDeploy") => {
            let (transaction_hash, address) = decode_contract_event(fields)?;
            Ok(Some(RelevantEvent::Operation {
                transaction_hash,
                operation: CanonicalMidnightOperation::Deploy(address),
            }))
        }
        ("Midnight", "ContractMaintain") => {
            let (transaction_hash, address) = decode_contract_event(fields)?;
            Ok(Some(RelevantEvent::Operation {
                transaction_hash,
                operation: CanonicalMidnightOperation::Maintain(address),
            }))
        }
        _ => Ok(None),
    }
}

fn decode_midnight_transaction(
    raw_transaction: Vec<u8>,
    extrinsic_index: u32,
    events: &[RelevantEvent],
    contract_address: [u8; 32],
) -> Result<Option<DecodedMidnightTransaction>, FinalizedMidnightHistoryError> {
    let successes = events
        .iter()
        .filter(|event| matches!(event, RelevantEvent::ExtrinsicSuccess))
        .count();
    let failures = events
        .iter()
        .filter(|event| matches!(event, RelevantEvent::ExtrinsicFailed))
        .count();
    let outcomes = events
        .iter()
        .filter_map(|event| match event {
            RelevantEvent::Outcome {
                transaction_hash,
                all_applied,
            } => Some((*transaction_hash, *all_applied)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let operation_events = events
        .iter()
        .filter_map(|event| match event {
            RelevantEvent::Operation {
                transaction_hash,
                operation,
            } => Some((*transaction_hash, *operation)),
            _ => None,
        })
        .collect::<Vec<_>>();

    if failures == 1 && successes == 0 && outcomes.is_empty() && operation_events.is_empty() {
        return Ok(None);
    }
    if successes != 1 || failures != 0 || outcomes.len() != 1 {
        return Err(FinalizedMidnightHistoryError::InvalidChainState);
    }
    let (transaction_hash, all_applied) = outcomes[0];
    if operation_events
        .iter()
        .any(|(event_hash, _)| *event_hash != transaction_hash)
    {
        return Err(FinalizedMidnightHistoryError::InvalidChainState);
    }
    let targets_contract = transaction_targets_contract(&raw_transaction, contract_address)
        .map_err(map_replay_inspection_error)?;
    Ok(Some(DecodedMidnightTransaction {
        raw_transaction,
        transaction_hash,
        extrinsic_index,
        all_applied,
        applied_operations: operation_events
            .into_iter()
            .map(|(_, operation)| operation)
            .collect(),
        targets_contract,
    }))
}

const fn map_replay_inspection_error(
    error: PassportVaultReplayError,
) -> FinalizedMidnightHistoryError {
    match error {
        PassportVaultReplayError::CapacityExceeded => {
            FinalizedMidnightHistoryError::CapacityExceeded
        }
        _ => FinalizedMidnightHistoryError::InvalidChainState,
    }
}

fn decode_timestamp(fields: &[u8]) -> Result<u64, FinalizedMidnightHistoryError> {
    let mut cursor = fields;
    let timestamp =
        u64::decode(&mut cursor).map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
    if !cursor.is_empty() {
        return Err(FinalizedMidnightHistoryError::InvalidChainState);
    }
    Ok(timestamp)
}

fn decode_scale_bytes(
    fields: &[u8],
    maximum: usize,
) -> Result<Vec<u8>, FinalizedMidnightHistoryError> {
    let mut cursor = fields;
    let declared = Compact::<u32>::decode(&mut cursor)
        .map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?
        .0 as usize;
    if declared == 0 || declared > maximum {
        return Err(FinalizedMidnightHistoryError::CapacityExceeded);
    }
    if cursor.len() != declared {
        return Err(FinalizedMidnightHistoryError::InvalidChainState);
    }
    Ok(cursor.to_vec())
}

fn decode_outcome_hash(fields: &[u8]) -> Result<[u8; 32], FinalizedMidnightHistoryError> {
    let mut cursor = fields;
    let transaction_hash = <[u8; 32]>::decode(&mut cursor)
        .map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
    if !cursor.is_empty() {
        return Err(FinalizedMidnightHistoryError::InvalidChainState);
    }
    Ok(transaction_hash)
}

fn decode_contract_event(
    fields: &[u8],
) -> Result<([u8; 32], [u8; 32]), FinalizedMidnightHistoryError> {
    let mut cursor = fields;
    let transaction_hash = <[u8; 32]>::decode(&mut cursor)
        .map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
    let address_bytes = decode_scale_bytes(cursor, MAX_CONTRACT_ADDRESS_BYTES)?;
    let mut address_cursor = Cursor::new(address_bytes.as_slice());
    let address: ContractAddress = tagged_deserialize(&mut address_cursor)
        .map_err(|_| FinalizedMidnightHistoryError::InvalidChainState)?;
    if address_cursor.position() != address_bytes.len() as u64 {
        return Err(FinalizedMidnightHistoryError::InvalidChainState);
    }
    Ok((transaction_hash, address.0.0))
}

fn operation_targets(operation: CanonicalMidnightOperation, target: [u8; 32]) -> bool {
    match operation {
        CanonicalMidnightOperation::Call(address)
        | CanonicalMidnightOperation::Deploy(address)
        | CanonicalMidnightOperation::Maintain(address) => address == target,
    }
}

fn required_archive_item<T>(value: Option<T>) -> Result<T, FinalizedMidnightHistoryError> {
    value.ok_or(FinalizedMidnightHistoryError::Unavailable)
}

async fn rpc_result<T, E>(
    future: impl Future<Output = Result<T, E>>,
) -> Result<T, FinalizedMidnightHistoryError> {
    timeout(RPC_TIMEOUT, future)
        .await
        .map_err(|_| FinalizedMidnightHistoryError::TimedOut)?
        .map_err(|_| FinalizedMidnightHistoryError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use midnight_base_crypto::{hash::HashOutput, schnorr::Signature, time::Timestamp};
    use midnight_ledger::structure::{
        ContractDeploy, Intent, ProofPreimageMarker, StandardTransaction, Transaction,
    };
    use midnight_serialize::tagged_serialize;
    use midnight_storage::{
        DefaultDB,
        storage::{Array, HashMap as LedgerHashMap},
    };
    use midnight_transient_crypto::commitment::PedersenRandomness;
    use subxt::ext::codec::Encode;

    const FIXTURE_HEX: &str =
        include_str!("../../../../fixtures/passport-vault/contract-state-v1.hex");

    fn encoded_contract_event(hash: [u8; 32], address: [u8; 32]) -> Vec<u8> {
        let mut tagged = Vec::new();
        tagged_serialize(&ContractAddress(HashOutput(address)), &mut tagged)
            .expect("contract address serializes");
        let mut fields = hash.encode();
        fields.extend(tagged.encode());
        fields
    }

    fn serialized_deployment() -> (Vec<u8>, [u8; 32], [u8; 32]) {
        let bytes = hex::decode(FIXTURE_HEX.trim()).expect("fixture is valid hex");
        let mut cursor = Cursor::new(bytes);
        let state = tagged_deserialize(&mut cursor).expect("fixture is an official contract state");
        let deploy = ContractDeploy {
            initial_state: state,
            nonce: HashOutput([7; 32]),
        };
        let address = deploy.address().0.0;
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
            .expect("deployment-only transaction can use the mock prover");
        let hash = proven.transaction_hash().0.0;
        let mut raw = Vec::new();
        tagged_serialize(&proven, &mut raw).expect("transaction serializes");
        (raw, hash, address)
    }

    fn decoded_transaction(
        hash: [u8; 32],
        operation: CanonicalMidnightOperation,
    ) -> DecodedMidnightTransaction {
        DecodedMidnightTransaction {
            raw_transaction: vec![1, 2, 3],
            transaction_hash: hash,
            extrinsic_index: 2,
            all_applied: true,
            applied_operations: vec![operation],
            targets_contract: true,
        }
    }

    fn block(
        height: u64,
        hash: [u8; 32],
        parent_hash: [u8; 32],
        timestamp_seconds: u64,
        transactions: Vec<DecodedMidnightTransaction>,
    ) -> DecodedFinalizedBlock {
        DecodedFinalizedBlock {
            hash,
            parent_hash,
            height,
            timestamp_seconds,
            transactions,
        }
    }

    #[test]
    fn configuration_accepts_only_secure_or_loopback_nodes_and_non_genesis_deployments() {
        assert!(FinalizedMidnightHistoryCollector::new("wss://node.example", 1).is_ok());
        assert!(FinalizedMidnightHistoryCollector::new("ws://127.0.0.1:9944", 1).is_ok());
        assert_eq!(
            FinalizedMidnightHistoryCollector::new("ws://node.example", 1)
                .expect_err("remote plaintext is rejected"),
            FinalizedMidnightHistoryCollectorConfigError::InvalidNodeEndpoint
        );
        assert_eq!(
            FinalizedMidnightHistoryCollector::new("wss://node.example", 0)
                .expect_err("genesis deployment hints are rejected"),
            FinalizedMidnightHistoryCollectorConfigError::InvalidDeploymentHeight
        );
        assert_eq!(
            required_archive_item::<Vec<u8>>(None),
            Err(FinalizedMidnightHistoryError::Unavailable)
        );
    }

    #[test]
    fn decodes_bounded_call_fields_and_the_official_tagged_contract_address() {
        let hash = [7; 32];
        let address = [9; 32];
        assert_eq!(
            decode_relevant_event(
                "Midnight",
                "ContractCall",
                &encoded_contract_event(hash, address)
            )
            .expect("event"),
            Some(RelevantEvent::Operation {
                transaction_hash: hash,
                operation: CanonicalMidnightOperation::Call(address),
            })
        );
        let mut malformed = encoded_contract_event(hash, address);
        malformed.push(0);
        assert_eq!(
            decode_relevant_event("Midnight", "ContractCall", &malformed),
            Err(FinalizedMidnightHistoryError::InvalidChainState)
        );
    }

    #[test]
    fn requires_exact_success_outcome_and_matching_operation_hashes() {
        let (raw_transaction, hash, target) = serialized_deployment();
        let events = vec![
            RelevantEvent::Operation {
                transaction_hash: hash,
                operation: CanonicalMidnightOperation::Deploy(target),
            },
            RelevantEvent::Outcome {
                transaction_hash: hash,
                all_applied: true,
            },
            RelevantEvent::ExtrinsicSuccess,
        ];
        let decoded = decode_midnight_transaction(raw_transaction.clone(), 4, &events, target)
            .expect("valid events")
            .expect("successful transaction");
        assert_eq!(decoded.transaction_hash, hash);
        assert_eq!(decoded.extrinsic_index, 4);
        assert_eq!(
            decoded.applied_operations,
            [CanonicalMidnightOperation::Deploy(target)]
        );

        let mut mismatched = events;
        mismatched[0] = RelevantEvent::Operation {
            transaction_hash: [13; 32],
            operation: CanonicalMidnightOperation::Deploy(target),
        };
        assert_eq!(
            decode_midnight_transaction(raw_transaction, 4, &mismatched, target),
            Err(FinalizedMidnightHistoryError::InvalidChainState)
        );
    }

    #[test]
    fn failed_extrinsics_are_ignored_only_without_midnight_outcomes() {
        assert!(
            decode_midnight_transaction(vec![1], 0, &[RelevantEvent::ExtrinsicFailed], [0; 32])
                .expect("clean failure")
                .is_none()
        );
        assert_eq!(
            decode_midnight_transaction(
                vec![1],
                0,
                &[
                    RelevantEvent::ExtrinsicFailed,
                    RelevantEvent::Outcome {
                        transaction_hash: [1; 32],
                        all_applied: false,
                    },
                ],
                [0; 32],
            ),
            Err(FinalizedMidnightHistoryError::InvalidChainState)
        );
    }

    #[test]
    fn builds_exact_contexts_from_a_contiguous_finalized_range() {
        let target = [21; 32];
        let prior = block(40, [40; 32], [39; 32], 1_000, Vec::new());
        let mut builder =
            CanonicalHistoryBuilder::new(target, 41, [42; 32], 42, &prior).expect("builder");
        builder
            .push(block(
                41,
                [41; 32],
                [40; 32],
                1_006,
                vec![decoded_transaction(
                    [1; 32],
                    CanonicalMidnightOperation::Deploy(target),
                )],
            ))
            .expect("deployment block");
        builder
            .push(block(
                42,
                [42; 32],
                [41; 32],
                1_012,
                vec![decoded_transaction(
                    [2; 32],
                    CanonicalMidnightOperation::Call(target),
                )],
            ))
            .expect("call block");
        let history = builder.finish().expect("complete history");
        assert_eq!(history.transactions.len(), 2);
        assert_eq!(
            history.transactions[0].block_context,
            CanonicalMidnightBlockContext {
                seconds_since_epoch: 1_006,
                uncertainty_seconds: 30,
                parent_block_hash: [40; 32],
                prior_block_seconds_since_epoch: 1_000,
            }
        );
        assert_eq!(
            history.transactions[1]
                .block_context
                .prior_block_seconds_since_epoch,
            1_006
        );
        assert_eq!(history.finalized_head_hash, [42; 32]);
    }

    #[test]
    fn retains_target_transactions_without_target_operation_events() {
        let target = [25; 32];
        let prior = block(50, [50; 32], [49; 32], 2_000, Vec::new());
        let mut builder =
            CanonicalHistoryBuilder::new(target, 51, [52; 32], 52, &prior).expect("builder");
        builder
            .push(block(
                51,
                [51; 32],
                [50; 32],
                2_006,
                vec![decoded_transaction(
                    [1; 32],
                    CanonicalMidnightOperation::Deploy(target),
                )],
            ))
            .expect("deployment");
        builder
            .push(block(
                52,
                [52; 32],
                [51; 32],
                2_012,
                vec![DecodedMidnightTransaction {
                    raw_transaction: vec![4, 5, 6],
                    transaction_hash: [2; 32],
                    extrinsic_index: 3,
                    all_applied: false,
                    applied_operations: Vec::new(),
                    targets_contract: true,
                }],
            ))
            .expect("guaranteed-only target transaction");
        let history = builder.finish().expect("complete history");
        assert_eq!(history.transactions.len(), 2);
        assert!(history.transactions[1].applied_operations.is_empty());
    }

    #[test]
    fn rejects_gaps_wrong_parents_and_unvalidated_deployment_hints() {
        let target = [31; 32];
        let prior = block(9, [9; 32], [8; 32], 100, Vec::new());
        let mut wrong_parent =
            CanonicalHistoryBuilder::new(target, 10, [10; 32], 10, &prior).expect("builder");
        assert_eq!(
            wrong_parent.push(block(10, [10; 32], [7; 32], 106, Vec::new())),
            Err(FinalizedMidnightHistoryError::InvalidChainState)
        );

        let mut missing =
            CanonicalHistoryBuilder::new(target, 10, [10; 32], 10, &prior).expect("builder");
        missing
            .push(block(10, [10; 32], [9; 32], 106, Vec::new()))
            .expect("contiguous block");
        assert_eq!(
            missing
                .finish()
                .expect_err("deployment must be authenticated"),
            FinalizedMidnightHistoryError::MissingDeployment
        );

        let mut late =
            CanonicalHistoryBuilder::new(target, 10, [11; 32], 11, &prior).expect("builder");
        late.push(block(10, [10; 32], [9; 32], 106, Vec::new()))
            .expect("configured deployment block");
        assert_eq!(
            late.push(block(
                11,
                [11; 32],
                [10; 32],
                112,
                vec![decoded_transaction(
                    [3; 32],
                    CanonicalMidnightOperation::Deploy(target),
                )],
            )),
            Err(FinalizedMidnightHistoryError::DeploymentHeightMismatch)
        );
    }
}
