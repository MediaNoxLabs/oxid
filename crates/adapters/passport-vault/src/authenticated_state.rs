// SPDX-License-Identifier: Apache-2.0

//! Application state source backed by complete finalized-node collection and
//! deterministic native Midnight replay.

use std::{
    error::Error,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use futures::channel::oneshot;
use oxid_passport_vault_application::{
    PassportVaultCallPortError, PassportVaultContractStateAuthentication,
    PassportVaultContractStateReadFuture, PassportVaultContractStateSnapshot,
    PassportVaultContractStateSourceError, PassportVaultContractStateSourcePort,
};

use super::{
    FinalizedMidnightHistory, FinalizedMidnightHistoryCollector,
    FinalizedMidnightHistoryCollectorConfigError, FinalizedMidnightHistoryError,
    NodeAnchoredPassportVaultStateConfigError, NodeAnchoredPassportVaultStateSource,
    PassportVaultCallChainContext, PassportVaultCallChainContextSource, PassportVaultReplayError,
    replay_canonical_passport_vault_history,
};

struct AuthenticatedPassportVaultStateConfig {
    collector: FinalizedMidnightHistoryCollector,
    node_anchored: Option<NodeAnchoredPassportVaultStateSource>,
    in_flight: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthenticatedPassportVaultStateConfigError {
    History(FinalizedMidnightHistoryCollectorConfigError),
    NodeAnchored(NodeAnchoredPassportVaultStateConfigError),
}

impl fmt::Display for AuthenticatedPassportVaultStateConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::History(error) => error.fmt(formatter),
            Self::NodeAnchored(error) => error.fmt(formatter),
        }
    }
}

impl Error for AuthenticatedPassportVaultStateConfigError {}

/// Reads only state reconstructed from a complete canonical range through one
/// captured finalized head. A process permits one expensive replay at a time;
/// concurrent callers fail unavailable instead of creating unbounded scans.
#[derive(Clone)]
pub struct AuthenticatedPassportVaultStateSource(Arc<AuthenticatedPassportVaultStateConfig>);

impl AuthenticatedPassportVaultStateSource {
    pub fn new(
        node_endpoint: impl AsRef<str>,
        deployment_block_height: u64,
    ) -> Result<Self, FinalizedMidnightHistoryCollectorConfigError> {
        Ok(Self(Arc::new(AuthenticatedPassportVaultStateConfig {
            collector: FinalizedMidnightHistoryCollector::new(
                node_endpoint,
                deployment_block_height,
            )?,
            node_anchored: None,
            in_flight: AtomicBool::new(false),
        })))
    }

    pub fn new_with_indexer(
        indexer_endpoint: impl AsRef<str>,
        node_endpoint: impl AsRef<str>,
        deployment_block_height: u64,
    ) -> Result<Self, AuthenticatedPassportVaultStateConfigError> {
        let collector =
            FinalizedMidnightHistoryCollector::new(node_endpoint.as_ref(), deployment_block_height)
                .map_err(AuthenticatedPassportVaultStateConfigError::History)?;
        let node_anchored =
            NodeAnchoredPassportVaultStateSource::new(indexer_endpoint, node_endpoint)
                .map_err(AuthenticatedPassportVaultStateConfigError::NodeAnchored)?;
        Ok(Self(Arc::new(AuthenticatedPassportVaultStateConfig {
            collector,
            node_anchored: Some(node_anchored),
            in_flight: AtomicBool::new(false),
        })))
    }
}

impl PassportVaultCallChainContextSource for AuthenticatedPassportVaultStateSource {
    fn chain_context(
        &self,
        snapshot: &PassportVaultContractStateSnapshot,
    ) -> Result<PassportVaultCallChainContext, PassportVaultCallPortError> {
        self.0
            .node_anchored
            .as_ref()
            .ok_or(PassportVaultCallPortError::Unavailable)?
            .chain_context(snapshot)
    }
}

struct ReadPermit(Arc<AuthenticatedPassportVaultStateConfig>);

impl Drop for ReadPermit {
    fn drop(&mut self) {
        self.0.in_flight.store(false, Ordering::Release);
    }
}

impl PassportVaultContractStateSourcePort for AuthenticatedPassportVaultStateSource {
    fn read<'a>(
        &'a self,
        contract_address_hex: &'a str,
    ) -> PassportVaultContractStateReadFuture<'a> {
        let Some(contract_address) = decode_contract_address(contract_address_hex) else {
            return Box::pin(async { Err(PassportVaultContractStateSourceError::InvalidAddress) });
        };
        if self
            .0
            .in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Box::pin(async { Err(PassportVaultContractStateSourceError::Unavailable) });
        }

        let config = Arc::clone(&self.0);
        let failure_config = Arc::clone(&config);
        let (sender, receiver) = oneshot::channel();
        let spawned = thread::Builder::new()
            .name("oxid-vault-replay".to_owned())
            .spawn(move || {
                let _permit = ReadPermit(Arc::clone(&config));
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| PassportVaultContractStateSourceError::Unavailable)
                    .and_then(|runtime| {
                        runtime.block_on(read_on_runtime(&config, contract_address))
                    });
                let _ = sender.send(result);
            });
        if spawned.is_err() {
            failure_config.in_flight.store(false, Ordering::Release);
            return Box::pin(async { Err(PassportVaultContractStateSourceError::Unavailable) });
        }
        Box::pin(async move {
            receiver
                .await
                .unwrap_or(Err(PassportVaultContractStateSourceError::Unavailable))
        })
    }
}

async fn read_on_runtime(
    config: &AuthenticatedPassportVaultStateConfig,
    contract_address: [u8; 32],
) -> Result<PassportVaultContractStateSnapshot, PassportVaultContractStateSourceError> {
    let history = config
        .collector
        .collect(contract_address)
        .await
        .map_err(map_history_error)?;
    let snapshot = snapshot_from_history(contract_address, history)?;
    if let Some(node_anchored) = &config.node_anchored {
        let indexed = node_anchored.read(&snapshot.contract_address_hex).await?;
        validate_indexed_anchor(&snapshot, &indexed)?;
    }
    Ok(snapshot)
}

fn validate_indexed_anchor(
    canonical: &PassportVaultContractStateSnapshot,
    indexed: &PassportVaultContractStateSnapshot,
) -> Result<(), PassportVaultContractStateSourceError> {
    if indexed.contract_address_hex != canonical.contract_address_hex
        || indexed.serialized_contract_state != canonical.serialized_contract_state
        || indexed.transaction_hash_hex != canonical.transaction_hash_hex
        || indexed.action_block_hash_hex != canonical.action_block_hash_hex
        || indexed.action_block_height != canonical.action_block_height
        || indexed.finalized_head_time_seconds != canonical.finalized_head_time_seconds
        || indexed.finalized_head_height < canonical.finalized_head_height
        || (indexed.finalized_head_height == canonical.finalized_head_height
            && indexed.finalized_head_hash_hex != canonical.finalized_head_hash_hex)
    {
        return Err(PassportVaultContractStateSourceError::FinalityMismatch);
    }
    Ok(())
}

fn snapshot_from_history(
    contract_address: [u8; 32],
    history: FinalizedMidnightHistory,
) -> Result<PassportVaultContractStateSnapshot, PassportVaultContractStateSourceError> {
    let replayed =
        replay_canonical_passport_vault_history(contract_address, history.transactions.as_slice())
            .map_err(map_replay_error)?;
    if replayed.deployment_block_height != history.deployment_block_height
        || replayed.latest_block_height > history.finalized_head_height
    {
        return Err(PassportVaultContractStateSourceError::FinalityMismatch);
    }
    Ok(PassportVaultContractStateSnapshot {
        serialized_contract_state: replayed.serialized_contract_state,
        authentication: PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
        contract_address_hex: hex::encode(contract_address),
        transaction_hash_hex: hex::encode(replayed.latest_transaction_hash),
        action_block_hash_hex: hex::encode(replayed.latest_block_hash),
        action_block_height: replayed.latest_block_height,
        finalized_head_hash_hex: hex::encode(history.finalized_head_hash),
        finalized_head_height: history.finalized_head_height,
        finalized_head_time_seconds: history.finalized_head_time_seconds,
    })
}

fn decode_contract_address(value: &str) -> Option<[u8; 32]> {
    let bytes = hex::decode(value).ok()?;
    bytes.try_into().ok()
}

const fn map_history_error(
    error: FinalizedMidnightHistoryError,
) -> PassportVaultContractStateSourceError {
    match error {
        FinalizedMidnightHistoryError::Unavailable | FinalizedMidnightHistoryError::TimedOut => {
            PassportVaultContractStateSourceError::Unavailable
        }
        FinalizedMidnightHistoryError::CapacityExceeded => {
            PassportVaultContractStateSourceError::CapacityExceeded
        }
        FinalizedMidnightHistoryError::InvalidChainState
        | FinalizedMidnightHistoryError::DeploymentNotFinalized
        | FinalizedMidnightHistoryError::DeploymentHeightMismatch
        | FinalizedMidnightHistoryError::MissingDeployment
        | FinalizedMidnightHistoryError::DuplicateDeployment => {
            PassportVaultContractStateSourceError::FinalityMismatch
        }
    }
}

const fn map_replay_error(
    error: PassportVaultReplayError,
) -> PassportVaultContractStateSourceError {
    match error {
        PassportVaultReplayError::CapacityExceeded => {
            PassportVaultContractStateSourceError::CapacityExceeded
        }
        _ => PassportVaultContractStateSourceError::InvalidResponse,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use midnight_base_crypto::{hash::HashOutput, schnorr::Signature, time::Timestamp};
    use midnight_ledger::structure::{
        ContractDeploy, Intent, ProofPreimageMarker, StandardTransaction, Transaction,
    };
    use midnight_onchain_runtime::state::ContractState;
    use midnight_serialize::{tagged_deserialize, tagged_serialize};
    use midnight_storage::{
        DefaultDB,
        storage::{Array, HashMap as LedgerHashMap},
    };
    use midnight_transient_crypto::commitment::PedersenRandomness;

    use super::*;
    use crate::{
        CanonicalMidnightBlockContext, CanonicalMidnightOperation, CanonicalMidnightTransaction,
    };

    const FIXTURE_HEX: &str =
        include_str!("../../../../fixtures/passport-vault/contract-state-v1.hex");

    fn deployment_history() -> ([u8; 32], FinalizedMidnightHistory) {
        let bytes = hex::decode(FIXTURE_HEX.trim()).expect("fixture hex");
        let state: ContractState<DefaultDB> =
            tagged_deserialize(&mut Cursor::new(bytes)).expect("official state");
        let deployment = ContractDeploy {
            initial_state: state,
            nonce: HashOutput([7; 32]),
        };
        let address = deployment.address().0.0;
        let intent: Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB> =
            Intent {
                guaranteed_unshielded_offer: None,
                fallible_unshielded_offer: None,
                actions: Array::from(vec![deployment.into()]),
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
        })
        .mock_prove()
        .expect("mock proof");
        let transaction_hash = transaction.transaction_hash().0.0;
        let mut raw_transaction = Vec::new();
        tagged_serialize(&transaction, &mut raw_transaction).expect("transaction serialization");
        (
            address,
            FinalizedMidnightHistory {
                transactions: vec![CanonicalMidnightTransaction {
                    raw_transaction,
                    transaction_hash,
                    block_hash: [3; 32],
                    block_height: 3,
                    extrinsic_index: 1,
                    block_context: CanonicalMidnightBlockContext {
                        seconds_since_epoch: 1_700_000_000,
                        uncertainty_seconds: 30,
                        parent_block_hash: [2; 32],
                        prior_block_seconds_since_epoch: 1_699_999_994,
                    },
                    all_applied: true,
                    applied_operations: vec![CanonicalMidnightOperation::Deploy(address)],
                }],
                deployment_block_height: 3,
                finalized_head_hash: [5; 32],
                finalized_head_height: 5,
                finalized_head_time_seconds: 1_700_000_012,
            },
        )
    }

    #[test]
    fn composes_complete_history_and_replay_into_an_authenticated_snapshot() {
        let (address, history) = deployment_history();
        let snapshot = snapshot_from_history(address, history).expect("authenticated snapshot");
        assert_eq!(
            snapshot.authentication,
            PassportVaultContractStateAuthentication::CanonicalFinalizedReplay
        );
        assert_eq!(snapshot.contract_address_hex, hex::encode(address));
        assert_eq!(snapshot.action_block_height, 3);
        assert_eq!(snapshot.finalized_head_height, 5);
        assert_eq!(snapshot.finalized_head_time_seconds, 1_700_000_012);
        assert_eq!(snapshot.finalized_head_hash_hex, hex::encode([5; 32]));
    }

    #[test]
    fn source_rejects_noncanonical_addresses_and_maps_failures_without_details() {
        assert!(decode_contract_address(&"11".repeat(32)).is_some());
        assert!(decode_contract_address("0x11").is_none());
        assert_eq!(
            map_history_error(FinalizedMidnightHistoryError::CapacityExceeded),
            PassportVaultContractStateSourceError::CapacityExceeded
        );
        assert_eq!(
            map_replay_error(PassportVaultReplayError::EffectsMismatch),
            PassportVaultContractStateSourceError::InvalidResponse
        );
    }

    #[test]
    fn indexed_context_must_match_the_replayed_action_and_state() {
        let (address, history) = deployment_history();
        let canonical = snapshot_from_history(address, history).expect("canonical snapshot");
        let mut indexed = canonical.clone();
        indexed.authentication = PassportVaultContractStateAuthentication::IndexerSuppliedNotProven;
        indexed.finalized_head_height += 1;
        indexed.finalized_head_hash_hex = hex::encode([6; 32]);
        assert_eq!(validate_indexed_anchor(&canonical, &indexed), Ok(()));

        indexed.transaction_hash_hex = hex::encode([9; 32]);
        assert_eq!(
            validate_indexed_anchor(&canonical, &indexed),
            Err(PassportVaultContractStateSourceError::FinalityMismatch)
        );
    }

    #[test]
    fn source_admits_only_one_expensive_scan_at_a_time() {
        let source = AuthenticatedPassportVaultStateSource::new("ws://127.0.0.1:9944", 3)
            .expect("loopback source");
        source.0.in_flight.store(true, Ordering::Release);
        assert_eq!(
            futures::executor::block_on(source.read(&"11".repeat(32))),
            Err(PassportVaultContractStateSourceError::Unavailable)
        );
        source.0.in_flight.store(false, Ordering::Release);
        assert_eq!(
            futures::executor::block_on(source.read("0x11")),
            Err(PassportVaultContractStateSourceError::InvalidAddress)
        );
        assert!(!source.0.in_flight.load(Ordering::Acquire));
    }
}
