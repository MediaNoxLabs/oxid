// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap as StdHashMap,
    io::Cursor,
    ops::Deref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use bech32::{Bech32m, primitives::decode::CheckedHrpstring};
use midnight_base_crypto::{
    hash::HashOutput,
    schnorr::{Signature, VerifyingKey},
    time::Timestamp,
};
use midnight_coin_structure::coin::{NIGHT, TokenType, UserAddress};
use midnight_ledger::structure::{
    Intent, IntentHash, ProofPreimageMarker, StandardTransaction, Transaction, UnshieldedOffer,
    UtxoOutput, UtxoSpend,
};
use midnight_serialize::Deserializable;
use midnight_storage::{DefaultDB, arena::Sp, storage::HashMap as LedgerHashMap};
use midnight_transient_crypto::commitment::PedersenRandomness;
use oxid_diagnostics_application::{DiagnosticCode, DiagnosticSeverity};
use oxid_wallet_application::{
    AuthorizeWalletTransferRequest, PrepareShieldedWalletTransferRequest,
    PrepareWalletTransferRequest, SubmitWalletTransferRequest, SubmittedWalletTransfer,
    WalletDerivedSecretUsePort, WalletHdPath, WalletHdPathComponent, WalletKeyOperationPort,
    WalletSecurityPortError, WalletTransactionPort, WalletTransactionPortError,
    WalletTransactionPortFuture, WalletTransactionStatusPortFuture,
};
use oxid_wallet_domain::{
    AssetBalance, AssetSymbol, ChainAddress, ChainAddressKind, ChainAsset, ChainBlockId,
    ChainNetwork, ChainTransactionId, DerivedChainAccount, MAX_WALLET_TRANSFER_INPUTS,
    PublicKeyEncoding, WalletKeyAlgorithm, WalletProfileId, WalletSignature,
    WalletTransactionAuthorizationChallenge, WalletTransactionDraftId, WalletTransactionDraftState,
    WalletTransactionFeeState, WalletTransactionSubmissionState, WalletTransactionSubmissionStatus,
    WalletTransferPreview, WalletTransferSubmission, WalletTransferSubmissionMode,
};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    BIP44_PURPOSE, DUST_INDEX, DUST_ROLE, MIDNIGHT_COIN_TYPE, MidnightWalletAdapter,
    ProtectedMidnightAccountDeriver, SPECKS_PER_DUST, STARS_PER_NIGHT,
    SimulatedMidnightAccountSource, UnavailableMidnightAccountDeriver,
    UnavailableMidnightAccountSource, midnight_asset, network_by_id,
    shielded_sync::MidnightShieldedTransferRequest,
    submission_journal::{
        MidnightSubmissionJournalStore, StoredSubmissionJournalEntry, StoredSubmissionState,
        SubmissionJournalStoreError,
    },
};

const SEND_UNSHIELDED_SEGMENT: u16 = 0xCAFE;
const CONTRACT_UNSHIELDED_FUNDING_SEGMENT: u16 = 0xBEEF;
const MAX_CONTRACT_CALL_TRANSACTION_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONTRACT_CALL_SUBMISSION_HISTORY: usize = 128;

type LedgerIntent = Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;
type LedgerTransaction = Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;

/// Adapter-private contract transaction transferred only across the static
/// composition root. The byte payload is zeroized and has no debug projection.
pub struct MidnightContractCallFundingRequest {
    profile_id: String,
    network_id: String,
    expires_at_seconds: u64,
    requires_night_funding: bool,
    transaction: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for MidnightContractCallFundingRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MidnightContractCallFundingRequest")
            .field("profile_id", &self.profile_id)
            .field("network_id", &self.network_id)
            .field("expires_at_seconds", &self.expires_at_seconds)
            .field("requires_night_funding", &self.requires_night_funding)
            .field("transaction_bytes", &self.transaction.len())
            .finish_non_exhaustive()
    }
}

impl MidnightContractCallFundingRequest {
    #[must_use]
    pub fn new(
        profile_id: impl Into<String>,
        network_id: impl Into<String>,
        expires_at_seconds: u64,
        requires_night_funding: bool,
        transaction: Zeroizing<Vec<u8>>,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            network_id: network_id.into(),
            expires_at_seconds,
            requires_night_funding,
            transaction,
        }
    }
}

/// A funded unproven transaction retained by the Passport Vault adapter. Only
/// safe aggregate funding metadata is debug-visible.
pub struct FundedMidnightContractCall {
    transaction: Zeroizing<Vec<u8>>,
    funded_night_atomic_units: u128,
    funding_input_count: u16,
}

impl std::fmt::Debug for FundedMidnightContractCall {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FundedMidnightContractCall")
            .field("transaction_bytes", &self.transaction.len())
            .field("funded_night_atomic_units", &self.funded_night_atomic_units)
            .field("funding_input_count", &self.funding_input_count)
            .finish_non_exhaustive()
    }
}

impl FundedMidnightContractCall {
    #[must_use]
    pub fn into_transaction(self) -> Zeroizing<Vec<u8>> {
        self.transaction
    }

    #[must_use]
    pub const fn funded_night_atomic_units(&self) -> u128 {
        self.funded_night_atomic_units
    }

    #[must_use]
    pub const fn funding_input_count(&self) -> u16 {
        self.funding_input_count
    }
}

/// Adds exact protected unshielded NIGHT funding to a generated contract call.
/// The serialized transaction never crosses an incoming or application port.
pub trait MidnightContractCallFundingPort: Send + Sync {
    fn fund_contract_call(
        &self,
        request: MidnightContractCallFundingRequest,
    ) -> Result<FundedMidnightContractCall, WalletTransactionPortError>;
}

/// Sensitive funded call plus public submission metadata passed only across
/// the composition root. The serialized transaction is zeroized on drop and
/// is never exposed through an incoming adapter.
pub struct MidnightContractCallSubmissionRequest {
    profile_id: String,
    network_id: String,
    draft_id: String,
    planning_fingerprint: [u8; 32],
    expires_at: oxid_foundation::UnixTimestampMillis,
    updated_at: oxid_foundation::UnixTimestampMillis,
    transaction: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for MidnightContractCallSubmissionRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MidnightContractCallSubmissionRequest")
            .field("profile_id", &self.profile_id)
            .field("network_id", &self.network_id)
            .field("draft_id", &self.draft_id)
            .field("expires_at", &self.expires_at)
            .field("updated_at", &self.updated_at)
            .field("transaction_bytes", &self.transaction.len())
            .finish_non_exhaustive()
    }
}

impl MidnightContractCallSubmissionRequest {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        profile_id: impl Into<String>,
        network_id: impl Into<String>,
        draft_id: impl Into<String>,
        planning_fingerprint: [u8; 32],
        expires_at: oxid_foundation::UnixTimestampMillis,
        updated_at: oxid_foundation::UnixTimestampMillis,
        transaction: Zeroizing<Vec<u8>>,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            network_id: network_id.into(),
            draft_id: draft_id.into(),
            planning_fingerprint,
            expires_at,
            updated_at,
            transaction,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightContractCallSubmissionState {
    Running,
    CancellationRequested,
    Broadcasting,
    Included,
    Rejected,
    Expired,
    OutcomeUnknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightContractCallSubmissionMode {
    Simulated,
    Live,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightContractCallSubmissionStatus {
    pub draft_id: String,
    pub state: MidnightContractCallSubmissionState,
    pub transaction_hash: Option<[u8; 32]>,
    pub block_hash: Option<[u8; 32]>,
    pub block_height: Option<u64>,
    pub fee_specks: Option<u128>,
    pub mode: Option<MidnightContractCallSubmissionMode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightContractCallSubmissionOutcome {
    pub transaction_hash: [u8; 32],
    pub block_hash: [u8; 32],
    pub block_height: u64,
    pub fee_specks: u128,
    pub mode: MidnightContractCallSubmissionMode,
}

/// Protected DUST balancing, proving, persistence, broadcast, and recovery
/// boundary for a funded Midnight contract call.
pub trait MidnightContractCallSubmissionPort: Send + Sync {
    fn complete_contract_call(
        &self,
        request: MidnightContractCallSubmissionRequest,
    ) -> Result<MidnightContractCallSubmissionOutcome, WalletTransactionPortError>;

    fn contract_call_submission_status(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError>;

    fn cancel_contract_call_submission(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError>;

    fn contract_call_submission_history(
        &self,
        profile_id: &str,
    ) -> Result<Vec<MidnightContractCallSubmissionStatus>, WalletTransactionPortError>;

    fn reconcile_contract_call_submission(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError>;
}

#[derive(Clone)]
pub(crate) struct MidnightCompletionRequest {
    pub(crate) transaction: LedgerTransaction,
    pub(crate) expires_at_seconds: u64,
    control: Arc<MidnightSubmissionControl>,
}

impl MidnightCompletionRequest {
    pub(crate) fn cancellation_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.control.cancellation)
    }

    pub(crate) fn begin_broadcast(
        &self,
        fee_specks: u128,
        transaction_hash: [u8; 32],
        anchor_block_hash: [u8; 32],
        mode: WalletTransferSubmissionMode,
    ) -> Result<(), WalletTransactionPortError> {
        self.control
            .begin_broadcast(fee_specks, transaction_hash, anchor_block_hash, mode)
    }
}

#[derive(Clone, Debug)]
struct MidnightSubmissionAttempt {
    profile_id: WalletProfileId,
    network_id: oxid_wallet_domain::ChainNetworkId,
    draft_id: WalletTransactionDraftId,
    planning_fingerprint: [u8; 32],
    expires_at: oxid_foundation::UnixTimestampMillis,
    updated_at: oxid_foundation::UnixTimestampMillis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MidnightSubmissionPhase {
    Working,
    CancellationRequested,
    Broadcasting,
}

pub(crate) struct MidnightSubmissionControl {
    cancellation: Arc<AtomicBool>,
    phase: Mutex<MidnightSubmissionPhase>,
    attempt: MidnightSubmissionAttempt,
    journal: Arc<dyn MidnightSubmissionJournalStore>,
}

impl MidnightSubmissionControl {
    fn new(
        attempt: MidnightSubmissionAttempt,
        journal: Arc<dyn MidnightSubmissionJournalStore>,
    ) -> Self {
        Self {
            cancellation: Arc::new(AtomicBool::new(false)),
            phase: Mutex::new(MidnightSubmissionPhase::Working),
            attempt,
            journal,
        }
    }

    fn request_cancellation(&self) -> Result<(), WalletTransactionPortError> {
        let mut phase = self
            .phase
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        match *phase {
            MidnightSubmissionPhase::Working => {
                self.cancellation.store(true, Ordering::Release);
                *phase = MidnightSubmissionPhase::CancellationRequested;
                Ok(())
            }
            MidnightSubmissionPhase::CancellationRequested => Ok(()),
            MidnightSubmissionPhase::Broadcasting => {
                Err(WalletTransactionPortError::SubmissionCancellationUnsafe)
            }
        }
    }

    fn begin_broadcast(
        &self,
        fee_specks: u128,
        transaction_hash: [u8; 32],
        anchor_block_hash: [u8; 32],
        mode: WalletTransferSubmissionMode,
    ) -> Result<(), WalletTransactionPortError> {
        let mut phase = self
            .phase
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        match *phase {
            MidnightSubmissionPhase::Working => {
                self.journal
                    .save(&StoredSubmissionJournalEntry {
                        profile_id: self.attempt.profile_id.clone(),
                        network_id: self.attempt.network_id.clone(),
                        draft_id: self.attempt.draft_id.clone(),
                        planning_fingerprint: self.attempt.planning_fingerprint,
                        expires_at: self.attempt.expires_at,
                        updated_at: self.attempt.updated_at,
                        fee_specks,
                        transaction_hash,
                        anchor_block_hash,
                        block_hash: None,
                        block_height: None,
                        state: StoredSubmissionState::Broadcasting,
                        mode,
                    })
                    .map_err(map_submission_store_error)?;
                *phase = MidnightSubmissionPhase::Broadcasting;
                Ok(())
            }
            MidnightSubmissionPhase::CancellationRequested => {
                Err(WalletTransactionPortError::SubmissionCancelled)
            }
            MidnightSubmissionPhase::Broadcasting => {
                Err(WalletTransactionPortError::SubmissionInProgress)
            }
        }
    }

    fn public_state(&self) -> Result<WalletTransactionSubmissionState, WalletTransactionPortError> {
        let phase = self
            .phase
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        Ok(match *phase {
            MidnightSubmissionPhase::Working => WalletTransactionSubmissionState::Running,
            MidnightSubmissionPhase::CancellationRequested => {
                WalletTransactionSubmissionState::CancellationRequested
            }
            MidnightSubmissionPhase::Broadcasting => WalletTransactionSubmissionState::Broadcasting,
        })
    }

    fn contract_call_state(
        &self,
    ) -> Result<MidnightContractCallSubmissionState, WalletTransactionPortError> {
        let phase = self
            .phase
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        Ok(match *phase {
            MidnightSubmissionPhase::Working => MidnightContractCallSubmissionState::Running,
            MidnightSubmissionPhase::CancellationRequested => {
                MidnightContractCallSubmissionState::CancellationRequested
            }
            MidnightSubmissionPhase::Broadcasting => {
                MidnightContractCallSubmissionState::Broadcasting
            }
        })
    }

    fn mark_terminal(
        &self,
        state: StoredSubmissionState,
        block_hash: Option<[u8; 32]>,
        block_height: Option<u64>,
    ) -> Result<(), WalletTransactionPortError> {
        let mut entry = self
            .journal
            .load(&self.attempt.profile_id, &self.attempt.draft_id)
            .map_err(map_submission_store_error)?
            .ok_or(WalletTransactionPortError::InvalidData)?;
        entry.state = state;
        entry.block_hash = block_hash;
        entry.block_height = block_height;
        self.journal
            .save(&entry)
            .map_err(map_submission_store_error)
    }

    fn broadcast_started(&self) -> Result<bool, WalletTransactionPortError> {
        self.phase
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)
            .map(|phase| *phase == MidnightSubmissionPhase::Broadcasting)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MidnightSubmissionReconciliation {
    Included {
        block_hash: [u8; 32],
        block_height: u64,
    },
    Rejected,
    Expired,
    Unresolved,
}

pub(crate) trait MidnightSubmissionReconciler: Send + Sync {
    fn reconcile(
        &self,
        entry: &StoredSubmissionJournalEntry,
    ) -> Result<MidnightSubmissionReconciliation, WalletTransactionPortError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UnavailableMidnightSubmissionReconciler;

impl MidnightSubmissionReconciler for UnavailableMidnightSubmissionReconciler {
    fn reconcile(
        &self,
        _: &StoredSubmissionJournalEntry,
    ) -> Result<MidnightSubmissionReconciliation, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MidnightCompletionOutcome {
    pub(crate) fee_specks: u128,
    pub(crate) transaction_hash: [u8; 32],
    pub(crate) block_hash: [u8; 32],
    pub(crate) block_height: u64,
    pub(crate) mode: WalletTransferSubmissionMode,
}

pub(crate) trait MidnightTransactionCompleter: Send + Sync {
    fn complete(
        &self,
        request: MidnightCompletionRequest,
        dust_seed: &[u8; 32],
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UnavailableMidnightTransactionCompleter;

impl MidnightTransactionCompleter for UnavailableMidnightTransactionCompleter {
    fn complete(
        &self,
        _: MidnightCompletionRequest,
        _: &[u8; 32],
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SimulatedMidnightTransactionCompleter;

impl MidnightTransactionCompleter for SimulatedMidnightTransactionCompleter {
    fn complete(
        &self,
        request: MidnightCompletionRequest,
        _: &[u8; 32],
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
        // The deterministic development adapter keeps a visible pre-broadcast
        // window so headless and mobile conformance can exercise cancellation.
        // XCTest observes the WebView accessibility tree at a coarser cadence.
        let steps = if cfg!(target_os = "ios") { 240 } else { 40 };
        for _ in 0..steps {
            if request.cancellation_token().load(Ordering::Acquire) {
                return Err(WalletTransactionPortError::SubmissionCancelled);
            }
            thread::sleep(std::time::Duration::from_millis(25));
        }
        let mut encoded = Vec::new();
        midnight_serialize::tagged_serialize(&request.transaction, &mut encoded)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let transaction_hash: [u8; 32] = Sha256::digest(&encoded).into();
        request.begin_broadcast(
            1_000_000,
            transaction_hash,
            [0; 32],
            WalletTransferSubmissionMode::Simulated,
        )?;
        let mut block_digest = Sha256::new();
        block_digest.update(b"oxid:simulated-midnight-block:v1\0");
        block_digest.update(transaction_hash);
        Ok(MidnightCompletionOutcome {
            fee_specks: 1_000_000,
            transaction_hash,
            block_hash: block_digest.finalize().into(),
            block_height: 1,
            mode: WalletTransferSubmissionMode::Simulated,
        })
    }
}

/// Exact native UTXO material retained behind the Midnight adapter boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MidnightSpendableUtxo {
    pub(crate) value: u128,
    pub(crate) intent_hash: [u8; 32],
    pub(crate) output_index: u32,
}

/// A derived account and its latest synchronized native UTXO set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MidnightSpendableAccount {
    pub(crate) account: DerivedChainAccount,
    pub(crate) utxos: Vec<MidnightSpendableUtxo>,
}

/// Internal source capability required for canonical transfer planning.
pub(crate) trait MidnightTransactionSource: Send + Sync {
    fn spendable_account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<MidnightSpendableAccount, WalletTransactionPortError>;
}

trait MidnightTransactionAuthorizer: Send + Sync {
    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        account: &DerivedChainAccount,
        payload: &[u8],
    ) -> Result<WalletSignature, WalletTransactionPortError>;

    fn use_dust_seed(
        &self,
        profile_id: &WalletProfileId,
        account_index: u32,
        operation: &mut dyn FnMut(
            &[u8; 32],
        )
            -> Result<MidnightCompletionOutcome, WalletTransactionPortError>,
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError>;
}

/// Chain-specific draft state. Neither its signing payload nor transaction is
/// available through application or incoming-adapter views.
pub(crate) struct RetainedMidnightDraft {
    planning_fingerprint: [u8; 32],
    preview: WalletTransferPreview,
    account: DerivedChainAccount,
    signing_payload: Zeroizing<Vec<u8>>,
    unsigned_intent: LedgerIntent,
    prepared_shielded_transaction: Option<LedgerTransaction>,
    signed_transaction: Option<LedgerTransaction>,
    submission: Option<WalletTransferSubmission>,
    submission_state: WalletTransactionSubmissionState,
    submission_control: Option<Arc<MidnightSubmissionControl>>,
}

pub(crate) type RetainedMidnightDrafts =
    Mutex<StdHashMap<(WalletProfileId, WalletTransactionDraftId), RetainedMidnightDraft>>;
pub(crate) type RetainedContractCallSubmissions = Arc<
    Mutex<StdHashMap<(WalletProfileId, WalletTransactionDraftId), Arc<MidnightSubmissionControl>>>,
>;

impl MidnightTransactionSource for UnavailableMidnightAccountSource {
    fn spendable_account(
        &self,
        _: &WalletProfileId,
        _: &ChainNetwork,
    ) -> Result<MidnightSpendableAccount, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }
}

impl<C> MidnightTransactionSource for SimulatedMidnightAccountSource<C>
where
    C: oxid_platform_ports::ClockPort + 'static,
{
    fn spendable_account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<MidnightSpendableAccount, WalletTransactionPortError> {
        let key = (profile_id.clone(), network.id().clone());
        let synchronized = self
            .synchronized
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?
            .contains(&key);
        if !synchronized {
            return Err(WalletTransactionPortError::AccountNotSynchronized);
        }
        let account = self
            .derived_accounts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?
            .get(&key)
            .cloned()
            .ok_or(WalletTransactionPortError::AccountNotDerived)?;
        Ok(MidnightSpendableAccount {
            account,
            utxos: vec![
                simulated_utxo(STARS_PER_NIGHT, 1, 0),
                simulated_utxo(2 * STARS_PER_NIGHT, 2, 0),
                simulated_utxo(2 * STARS_PER_NIGHT, 3, 0),
            ],
        })
    }
}

fn simulated_utxo(value: u128, hash_byte: u8, output_index: u32) -> MidnightSpendableUtxo {
    MidnightSpendableUtxo {
        value,
        intent_hash: [hash_byte; 32],
        output_index,
    }
}

impl MidnightTransactionAuthorizer for UnavailableMidnightAccountDeriver {
    fn authorize(
        &self,
        _: &WalletProfileId,
        _: &DerivedChainAccount,
        _: &[u8],
    ) -> Result<WalletSignature, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }

    fn use_dust_seed(
        &self,
        _: &WalletProfileId,
        _: u32,
        _: &mut dyn FnMut(
            &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError>,
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }
}

impl<K> MidnightTransactionAuthorizer for ProtectedMidnightAccountDeriver<K>
where
    K: WalletDerivedSecretUsePort + WalletKeyOperationPort + 'static,
{
    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        account: &DerivedChainAccount,
        payload: &[u8],
    ) -> Result<WalletSignature, WalletTransactionPortError> {
        self.keys
            .sign(profile_id, account.transaction_key(), payload)
            .map_err(map_security_error)
    }

    fn use_dust_seed(
        &self,
        profile_id: &WalletProfileId,
        account_index: u32,
        operation: &mut dyn FnMut(
            &[u8; 32],
        )
            -> Result<MidnightCompletionOutcome, WalletTransactionPortError>,
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
        let path = dust_path(account_index)?;
        let mut outcome = None;
        self.keys
            .use_derived_secret(profile_id, &path, &mut |secret| {
                outcome = Some(operation(secret));
                Ok(())
            })
            .map_err(map_security_error)?;
        outcome.ok_or(WalletTransactionPortError::InvalidData)?
    }
}

fn dust_path(account_index: u32) -> Result<WalletHdPath, WalletTransactionPortError> {
    let component = |index, hardened| {
        WalletHdPathComponent::new(index, hardened)
            .map_err(|_| WalletTransactionPortError::InvalidData)
    };
    WalletHdPath::new(vec![
        component(BIP44_PURPOSE, true)?,
        component(MIDNIGHT_COIN_TYPE, true)?,
        component(account_index, true)?,
        component(DUST_ROLE, false)?,
        component(DUST_INDEX, false)?,
    ])
    .map_err(|_| WalletTransactionPortError::InvalidData)
}

impl<S, D> MidnightContractCallFundingPort for MidnightWalletAdapter<S, D>
where
    S: MidnightTransactionSource,
    D: MidnightTransactionAuthorizer + Send + Sync,
{
    fn fund_contract_call(
        &self,
        request: MidnightContractCallFundingRequest,
    ) -> Result<FundedMidnightContractCall, WalletTransactionPortError> {
        let profile_id = WalletProfileId::parse(request.profile_id)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let selected = self.selected(&profile_id).map_err(map_account_error)?;
        if selected.as_str() != request.network_id {
            return Err(WalletTransactionPortError::UnsupportedNetwork);
        }
        if request.expires_at_seconds == 0
            || request.transaction.is_empty()
            || request.transaction.len() > MAX_CONTRACT_CALL_TRANSACTION_BYTES
        {
            return Err(WalletTransactionPortError::InvalidData);
        }
        let mut cursor = Cursor::new(request.transaction.as_slice());
        let transaction: LedgerTransaction = midnight_serialize::tagged_deserialize(&mut cursor)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        if usize::try_from(cursor.position()).ok() != Some(request.transaction.len()) {
            return Err(WalletTransactionPortError::InvalidData);
        }
        let Transaction::Standard(standard) = &transaction else {
            return Err(WalletTransactionPortError::InvalidData);
        };
        if standard.network_id != request.network_id || standard.intents.iter().count() != 1 {
            return Err(WalletTransactionPortError::InvalidData);
        }
        let shortfall = unshielded_night_shortfall(&transaction)?;
        let (funded, funded_night_atomic_units, funding_input_count) =
            match (request.requires_night_funding, shortfall) {
                (false, None) => (transaction, 0, 0),
                (false, Some(_)) | (true, None) => {
                    return Err(WalletTransactionPortError::InvalidChainState);
                }
                (true, Some((segment, amount))) => {
                    let network = network_by_id(&selected)
                        .map_err(map_account_error)?
                        .ok_or(WalletTransactionPortError::UnsupportedNetwork)?;
                    let spendable = self.source.spendable_account(&profile_id, &network)?;
                    validate_account(&spendable.account, &selected)?;
                    let (selected_utxos, total) = select_utxos(spendable.utxos, amount)?;
                    let input_count = u16::try_from(selected_utxos.len())
                        .map_err(|_| WalletTransactionPortError::InvalidData)?;
                    let funded = fund_unshielded_night(
                        transaction,
                        segment,
                        amount,
                        total,
                        &selected_utxos,
                        &profile_id,
                        &spendable.account,
                        &self.deriver,
                        request.expires_at_seconds,
                        &request.network_id,
                    )?;
                    (funded, amount, input_count)
                }
            };
        if unshielded_night_shortfall(&funded)?.is_some() {
            return Err(WalletTransactionPortError::InvalidChainState);
        }
        let mut encoded = Zeroizing::new(Vec::new());
        midnight_serialize::tagged_serialize(&funded, &mut *encoded)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        if encoded.len() > MAX_CONTRACT_CALL_TRANSACTION_BYTES {
            return Err(WalletTransactionPortError::InvalidData);
        }
        Ok(FundedMidnightContractCall {
            transaction: encoded,
            funded_night_atomic_units,
            funding_input_count,
        })
    }
}

impl<S, D> MidnightContractCallSubmissionPort for MidnightWalletAdapter<S, D>
where
    S: MidnightTransactionSource,
    D: MidnightTransactionAuthorizer + Send + Sync,
{
    fn complete_contract_call(
        &self,
        request: MidnightContractCallSubmissionRequest,
    ) -> Result<MidnightContractCallSubmissionOutcome, WalletTransactionPortError> {
        let profile_id = WalletProfileId::parse(request.profile_id)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let selected = self.selected(&profile_id).map_err(map_account_error)?;
        if selected.as_str() != request.network_id
            || request.expires_at.value() <= request.updated_at.value()
            || request.transaction.is_empty()
            || request.transaction.len() > MAX_CONTRACT_CALL_TRANSACTION_BYTES
        {
            return Err(WalletTransactionPortError::InvalidData);
        }
        let network = network_by_id(&selected)
            .map_err(map_account_error)?
            .ok_or(WalletTransactionPortError::UnsupportedNetwork)?;
        let spendable = self.source.spendable_account(&profile_id, &network)?;
        validate_account(&spendable.account, &selected)?;
        let mut cursor = Cursor::new(request.transaction.as_slice());
        let transaction: LedgerTransaction = midnight_serialize::tagged_deserialize(&mut cursor)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        if usize::try_from(cursor.position()).ok() != Some(request.transaction.len())
            || unshielded_night_shortfall(&transaction)?.is_some()
        {
            return Err(WalletTransactionPortError::InvalidData);
        }
        let Transaction::Standard(standard) = &transaction else {
            return Err(WalletTransactionPortError::InvalidData);
        };
        if standard.network_id != request.network_id
            || !(1..=2).contains(&standard.intents.iter().count())
        {
            return Err(WalletTransactionPortError::InvalidData);
        }

        let (journal_profile, journal_draft) =
            contract_call_journal_ids(&profile_id, &request.draft_id)?;
        if let Some(stored) = self
            .submission_journal
            .load(&journal_profile, &journal_draft)
            .map_err(map_submission_store_error)?
        {
            match stored.state {
                StoredSubmissionState::Included => return contract_call_outcome(&stored),
                StoredSubmissionState::Broadcasting | StoredSubmissionState::OutcomeUnknown => {
                    return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
                }
                StoredSubmissionState::Rejected | StoredSubmissionState::Expired => {}
            }
        }
        let key = (journal_profile.clone(), journal_draft.clone());
        let control = Arc::new(MidnightSubmissionControl::new(
            MidnightSubmissionAttempt {
                profile_id: journal_profile,
                network_id: selected,
                draft_id: journal_draft,
                planning_fingerprint: request.planning_fingerprint,
                expires_at: request.expires_at,
                updated_at: request.updated_at,
            },
            Arc::clone(&self.submission_journal),
        ));
        {
            let mut active = self
                .contract_call_submissions
                .lock()
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            if active.contains_key(&key) {
                return Err(WalletTransactionPortError::SubmissionInProgress);
            }
            active.insert(key.clone(), Arc::clone(&control));
        }

        let expires_at_seconds = request.expires_at.value() / 1_000;
        let mut operation = |dust_seed: &[u8; 32]| {
            self.completer.complete(
                MidnightCompletionRequest {
                    transaction: transaction.clone(),
                    expires_at_seconds,
                    control: Arc::clone(&control),
                },
                dust_seed,
            )
        };
        let completion = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.deriver.use_dust_seed(
                &profile_id,
                spendable.account.account_index(),
                &mut operation,
            )
        }));
        let result = match completion {
            Ok(completion) => finish_contract_call_submission(control.as_ref(), completion),
            Err(_) => {
                self.record_diagnostic(
                    DiagnosticCode::MidnightContractCallWorkerPanicked,
                    DiagnosticSeverity::Error,
                );
                if control.broadcast_started().unwrap_or(true) {
                    let _ =
                        control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
                    Err(WalletTransactionPortError::SubmissionOutcomeUnknown)
                } else {
                    Err(WalletTransactionPortError::Unavailable)
                }
            }
        };
        if let Ok(mut active) = self.contract_call_submissions.lock() {
            active.remove(&key);
        }
        result
    }

    fn contract_call_submission_status(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
        let profile_id = WalletProfileId::parse(profile_id.to_owned())
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let (journal_profile, journal_draft) = contract_call_journal_ids(&profile_id, draft_id)?;
        if let Some(stored) = self
            .submission_journal
            .load(&journal_profile, &journal_draft)
            .map_err(map_submission_store_error)?
        {
            return contract_call_status(&stored);
        }
        self.contract_call_submissions
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?
            .get(&(journal_profile, journal_draft))
            .map(|control| {
                Ok(MidnightContractCallSubmissionStatus {
                    draft_id: draft_id.to_owned(),
                    state: control.contract_call_state()?,
                    transaction_hash: None,
                    block_hash: None,
                    block_height: None,
                    fee_specks: None,
                    mode: None,
                })
            })
            .transpose()?
            .ok_or(WalletTransactionPortError::DraftNotFound)
    }

    fn cancel_contract_call_submission(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
        let profile_id = WalletProfileId::parse(profile_id.to_owned())
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let (journal_profile, journal_draft) = contract_call_journal_ids(&profile_id, draft_id)?;
        if let Some(stored) = self
            .submission_journal
            .load(&journal_profile, &journal_draft)
            .map_err(map_submission_store_error)?
        {
            return match stored.state {
                StoredSubmissionState::Broadcasting
                | StoredSubmissionState::OutcomeUnknown
                | StoredSubmissionState::Included => {
                    Err(WalletTransactionPortError::SubmissionCancellationUnsafe)
                }
                StoredSubmissionState::Rejected | StoredSubmissionState::Expired => {
                    Err(WalletTransactionPortError::SubmissionNotInProgress)
                }
            };
        }
        let control = self
            .contract_call_submissions
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?
            .get(&(journal_profile, journal_draft))
            .cloned()
            .ok_or(WalletTransactionPortError::SubmissionNotInProgress)?;
        control.request_cancellation()?;
        Ok(MidnightContractCallSubmissionStatus {
            draft_id: draft_id.to_owned(),
            state: MidnightContractCallSubmissionState::CancellationRequested,
            transaction_hash: None,
            block_hash: None,
            block_height: None,
            fee_specks: None,
            mode: None,
        })
    }

    fn contract_call_submission_history(
        &self,
        profile_id: &str,
    ) -> Result<Vec<MidnightContractCallSubmissionStatus>, WalletTransactionPortError> {
        let profile_id = WalletProfileId::parse(profile_id.to_owned())
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let journal_profile = contract_call_journal_profile(&profile_id)?;
        let stored = self
            .submission_journal
            .list(&journal_profile)
            .map_err(map_submission_store_error)?;
        let mut statuses = stored
            .iter()
            .map(contract_call_status)
            .collect::<Result<Vec<_>, _>>()?;
        let active = self
            .contract_call_submissions
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        for ((stored_profile, stored_draft), control) in active.iter() {
            if stored_profile == &journal_profile {
                let draft_id = contract_call_draft_id(stored_draft)?;
                if !statuses.iter().any(|status| status.draft_id == draft_id) {
                    if statuses.len() == MAX_CONTRACT_CALL_SUBMISSION_HISTORY {
                        statuses.pop();
                    }
                    statuses.insert(
                        0,
                        MidnightContractCallSubmissionStatus {
                            draft_id,
                            state: control.contract_call_state()?,
                            transaction_hash: None,
                            block_hash: None,
                            block_height: None,
                            fee_specks: None,
                            mode: None,
                        },
                    );
                }
            }
        }
        statuses.truncate(MAX_CONTRACT_CALL_SUBMISSION_HISTORY);
        Ok(statuses)
    }

    fn reconcile_contract_call_submission(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
        let profile_id = WalletProfileId::parse(profile_id.to_owned())
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let (journal_profile, journal_draft) = contract_call_journal_ids(&profile_id, draft_id)?;
        let mut entry = self
            .submission_journal
            .load(&journal_profile, &journal_draft)
            .map_err(map_submission_store_error)?
            .ok_or(WalletTransactionPortError::DraftNotFound)?;
        if !matches!(
            entry.state,
            StoredSubmissionState::Broadcasting | StoredSubmissionState::OutcomeUnknown
        ) {
            return contract_call_status(&entry);
        }
        match self.submission_reconciler.reconcile(&entry)? {
            MidnightSubmissionReconciliation::Included {
                block_hash,
                block_height,
            } => {
                entry.state = StoredSubmissionState::Included;
                entry.block_hash = Some(block_hash);
                entry.block_height = Some(block_height);
            }
            MidnightSubmissionReconciliation::Rejected => {
                entry.state = StoredSubmissionState::Rejected;
                entry.block_hash = None;
                entry.block_height = None;
            }
            MidnightSubmissionReconciliation::Expired => {
                entry.state = StoredSubmissionState::Expired;
                entry.block_hash = None;
                entry.block_height = None;
            }
            MidnightSubmissionReconciliation::Unresolved => {
                entry.state = StoredSubmissionState::OutcomeUnknown;
                entry.block_hash = None;
                entry.block_height = None;
            }
        }
        self.submission_journal
            .save(&entry)
            .map_err(map_submission_store_error)?;
        contract_call_status(&entry)
    }
}

fn contract_call_journal_profile(
    profile_id: &WalletProfileId,
) -> Result<WalletProfileId, WalletTransactionPortError> {
    let mut digest = Sha256::new();
    digest.update(b"oxid:passport-vault-submission-profile:v1\0");
    digest.update(profile_id.as_str().as_bytes());
    WalletProfileId::parse(format!("vault-profile-{}", hex::encode(digest.finalize())))
        .map_err(|_| WalletTransactionPortError::InvalidData)
}

fn contract_call_journal_ids(
    profile_id: &WalletProfileId,
    draft_id: &str,
) -> Result<(WalletProfileId, WalletTransactionDraftId), WalletTransactionPortError> {
    let profile = contract_call_journal_profile(profile_id)?;
    let draft = WalletTransactionDraftId::parse(format!("vault-{draft_id}"))
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
    Ok((profile, draft))
}

fn contract_call_draft_id(
    stored: &WalletTransactionDraftId,
) -> Result<String, WalletTransactionPortError> {
    stored
        .as_str()
        .strip_prefix("vault-")
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(WalletTransactionPortError::InvalidData)
}

fn contract_call_status(
    entry: &StoredSubmissionJournalEntry,
) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
    let state = match entry.state {
        StoredSubmissionState::Broadcasting => MidnightContractCallSubmissionState::Broadcasting,
        StoredSubmissionState::OutcomeUnknown => {
            MidnightContractCallSubmissionState::OutcomeUnknown
        }
        StoredSubmissionState::Included => MidnightContractCallSubmissionState::Included,
        StoredSubmissionState::Rejected => MidnightContractCallSubmissionState::Rejected,
        StoredSubmissionState::Expired => MidnightContractCallSubmissionState::Expired,
    };
    if (entry.state == StoredSubmissionState::Included)
        != (entry.block_hash.is_some() && entry.block_height.is_some())
    {
        return Err(WalletTransactionPortError::InvalidData);
    }
    Ok(MidnightContractCallSubmissionStatus {
        draft_id: contract_call_draft_id(&entry.draft_id)?,
        state,
        transaction_hash: Some(entry.transaction_hash),
        block_hash: entry.block_hash,
        block_height: entry.block_height,
        fee_specks: Some(entry.fee_specks),
        mode: Some(contract_call_submission_mode(entry.mode)),
    })
}

fn contract_call_outcome(
    entry: &StoredSubmissionJournalEntry,
) -> Result<MidnightContractCallSubmissionOutcome, WalletTransactionPortError> {
    if entry.state != StoredSubmissionState::Included {
        return Err(WalletTransactionPortError::InvalidData);
    }
    Ok(MidnightContractCallSubmissionOutcome {
        transaction_hash: entry.transaction_hash,
        block_hash: entry
            .block_hash
            .ok_or(WalletTransactionPortError::InvalidData)?,
        block_height: entry
            .block_height
            .ok_or(WalletTransactionPortError::InvalidData)?,
        fee_specks: entry.fee_specks,
        mode: contract_call_submission_mode(entry.mode),
    })
}

fn finish_contract_call_submission(
    control: &MidnightSubmissionControl,
    completion: Result<MidnightCompletionOutcome, WalletTransactionPortError>,
) -> Result<MidnightContractCallSubmissionOutcome, WalletTransactionPortError> {
    let outcome = match completion {
        Ok(outcome) => outcome,
        Err(WalletTransactionPortError::SubmissionOutcomeUnknown) => {
            let _ = control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
            return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
        }
        Err(WalletTransactionPortError::SubmissionRejected) if control.broadcast_started()? => {
            control.mark_terminal(StoredSubmissionState::Rejected, None, None)?;
            return Err(WalletTransactionPortError::SubmissionRejected);
        }
        Err(_) if control.broadcast_started()? => {
            let _ = control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
            return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
        }
        Err(error) => return Err(error),
    };
    if control
        .mark_terminal(
            StoredSubmissionState::Included,
            Some(outcome.block_hash),
            Some(outcome.block_height),
        )
        .is_err()
    {
        let _ = control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
        return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
    }
    Ok(MidnightContractCallSubmissionOutcome {
        transaction_hash: outcome.transaction_hash,
        block_hash: outcome.block_hash,
        block_height: outcome.block_height,
        fee_specks: outcome.fee_specks,
        mode: contract_call_submission_mode(outcome.mode),
    })
}

const fn contract_call_submission_mode(
    mode: WalletTransferSubmissionMode,
) -> MidnightContractCallSubmissionMode {
    match mode {
        WalletTransferSubmissionMode::Simulated => MidnightContractCallSubmissionMode::Simulated,
        WalletTransferSubmissionMode::Live => MidnightContractCallSubmissionMode::Live,
    }
}

fn unshielded_night_shortfall(
    transaction: &LedgerTransaction,
) -> Result<Option<(u16, u128)>, WalletTransactionPortError> {
    let balance = transaction
        .balance(None)
        .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    let mut shortfall = None;
    for ((token, segment), value) in balance.iter() {
        let TokenType::Unshielded(unshielded) = token else {
            continue;
        };
        if *value >= 0 {
            continue;
        }
        if *unshielded != NIGHT || shortfall.is_some() {
            return Err(WalletTransactionPortError::InvalidChainState);
        }
        let amount = value
            .checked_neg()
            .and_then(|value| u128::try_from(value).ok())
            .ok_or(WalletTransactionPortError::InvalidChainState)?;
        shortfall = Some((*segment, amount));
    }
    Ok(shortfall)
}

#[allow(clippy::too_many_arguments)]
fn fund_unshielded_night<D>(
    transaction: LedgerTransaction,
    shortfall_segment: u16,
    shortfall: u128,
    selected_total: u128,
    selected_utxos: &[MidnightSpendableUtxo],
    profile_id: &WalletProfileId,
    account: &DerivedChainAccount,
    authorizer: &D,
    expires_at_seconds: u64,
    network_id: &str,
) -> Result<LedgerTransaction, WalletTransactionPortError>
where
    D: MidnightTransactionAuthorizer,
{
    let owner = decode_verifying_key(account)?;
    let mut inputs = selected_utxos
        .iter()
        .map(|utxo| UtxoSpend {
            value: utxo.value,
            owner: owner.clone(),
            type_: NIGHT,
            intent_hash: IntentHash(HashOutput(utxo.intent_hash)),
            output_no: utxo.output_index,
        })
        .collect::<Vec<_>>();
    let mut outputs = Vec::new();
    let change = selected_total
        .checked_sub(shortfall)
        .ok_or(WalletTransactionPortError::InvalidChainState)?;
    if change > 0 {
        outputs.push(UtxoOutput {
            value: change,
            owner: UserAddress::from(owner),
            type_: NIGHT,
        });
    }
    inputs.sort();
    outputs.sort();
    let offer = UnshieldedOffer {
        inputs: inputs.clone().into(),
        outputs: outputs.into(),
        signatures: Vec::new().into(),
    };
    let ttl = Timestamp::from_secs(expires_at_seconds);
    if shortfall_segment == 0 {
        let mut intent = LedgerIntent::empty(&mut OsRng, ttl);
        intent.guaranteed_unshielded_offer = Some(Sp::new(offer));
        let signed = authorize_contract_funding_intent(
            intent,
            CONTRACT_UNSHIELDED_FUNDING_SEGMENT,
            true,
            inputs.len(),
            profile_id,
            account,
            authorizer,
        )?;
        let mut intents = LedgerHashMap::new();
        intents = intents.insert(CONTRACT_UNSHIELDED_FUNDING_SEGMENT, signed);
        return transaction
            .merge(&Transaction::Standard(StandardTransaction::new(
                network_id,
                intents,
                None,
                LedgerHashMap::new(),
            )))
            .map_err(|_| WalletTransactionPortError::InvalidChainState);
    }

    let Transaction::Standard(standard) = transaction else {
        return Err(WalletTransactionPortError::InvalidData);
    };
    let mut intents = LedgerHashMap::new();
    let mut grafted = false;
    for (segment, mut intent) in standard.intents.into_iter() {
        if segment == shortfall_segment {
            intent.fallible_unshielded_offer = Some(Sp::new(offer.clone()));
            intent = authorize_contract_funding_intent(
                intent,
                segment,
                false,
                inputs.len(),
                profile_id,
                account,
                authorizer,
            )?;
            grafted = true;
        }
        intents = intents.insert(segment, intent);
    }
    if !grafted {
        return Err(WalletTransactionPortError::InvalidChainState);
    }
    Ok(Transaction::Standard(StandardTransaction {
        network_id: standard.network_id,
        intents,
        guaranteed_coins: standard.guaranteed_coins,
        fallible_coins: standard.fallible_coins,
        binding_randomness: standard.binding_randomness,
    }))
}

#[allow(clippy::too_many_arguments)]
fn authorize_contract_funding_intent<D>(
    mut intent: LedgerIntent,
    segment: u16,
    guaranteed: bool,
    input_count: usize,
    profile_id: &WalletProfileId,
    account: &DerivedChainAccount,
    authorizer: &D,
) -> Result<LedgerIntent, WalletTransactionPortError>
where
    D: MidnightTransactionAuthorizer,
{
    let signing_payload = intent
        .erase_proofs()
        .erase_signatures()
        .data_to_sign(segment);
    let signature = authorizer.authorize(profile_id, account, &signing_payload)?;
    if signature.algorithm() != WalletKeyAlgorithm::Secp256k1Schnorr {
        return Err(WalletTransactionPortError::InvalidData);
    }
    let signature = decode_signature(&signature)?;
    let verifying_key = decode_verifying_key(account)?;
    if !verifying_key.verify(&signing_payload, &signature) {
        return Err(WalletTransactionPortError::InvalidData);
    }
    let offer = if guaranteed {
        intent.guaranteed_unshielded_offer.as_ref()
    } else {
        intent.fallible_unshielded_offer.as_ref()
    }
    .ok_or(WalletTransactionPortError::InvalidData)?;
    if offer.inputs.len() != input_count || input_count == 0 {
        return Err(WalletTransactionPortError::InvalidData);
    }
    let mut signed_offer = offer.deref().clone();
    signed_offer.add_signatures(vec![signature; input_count]);
    if guaranteed {
        intent.guaranteed_unshielded_offer = Some(Sp::new(signed_offer));
    } else {
        intent.fallible_unshielded_offer = Some(Sp::new(signed_offer));
    }
    Ok(intent)
}

impl<S, D> WalletTransactionPort for MidnightWalletAdapter<S, D>
where
    S: MidnightTransactionSource,
    D: MidnightTransactionAuthorizer + Clone + 'static,
{
    fn prepare(
        &self,
        profile_id: &WalletProfileId,
        request: PrepareWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
        let selected = self.selected(profile_id).map_err(map_account_error)?;
        let network = network_by_id(&selected)
            .map_err(map_account_error)?
            .ok_or(WalletTransactionPortError::UnsupportedNetwork)?;
        let recipient = decode_recipient(&request.recipient, &selected)?;
        let spendable = self.source.spendable_account(profile_id, &network)?;
        validate_account(&spendable.account, &selected)?;

        let (selected_utxos, total) = select_utxos(spendable.utxos, request.amount_atomic_units)?;
        let change = total
            .checked_sub(request.amount_atomic_units)
            .ok_or(WalletTransactionPortError::InvalidData)?;
        let owner = decode_verifying_key(&spendable.account)?;
        let mut inputs = selected_utxos
            .iter()
            .map(|utxo| UtxoSpend {
                value: utxo.value,
                owner: owner.clone(),
                type_: NIGHT,
                intent_hash: IntentHash(HashOutput(utxo.intent_hash)),
                output_no: utxo.output_index,
            })
            .collect::<Vec<_>>();
        let mut outputs = vec![UtxoOutput {
            value: request.amount_atomic_units,
            owner: recipient,
            type_: NIGHT,
        }];
        if change > 0 {
            outputs.push(UtxoOutput {
                value: change,
                owner: UserAddress::from(owner),
                type_: NIGHT,
            });
        }
        inputs.sort();
        outputs.sort();

        let planning_fingerprint = planning_fingerprint(
            profile_id,
            &selected,
            &request,
            &spendable.account,
            &selected_utxos,
        );
        if let Some(stored) = self
            .submission_journal
            .find_planning_fingerprint(profile_id, &planning_fingerprint)
            .map_err(map_submission_store_error)?
        {
            match stored.state {
                StoredSubmissionState::Broadcasting | StoredSubmissionState::OutcomeUnknown => {
                    return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
                }
                StoredSubmissionState::Included => {
                    return Err(WalletTransactionPortError::DraftConflict);
                }
                StoredSubmissionState::Rejected | StoredSubmissionState::Expired => {}
            }
        }
        {
            let drafts = self
                .drafts
                .lock()
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            if let Some(existing) = drafts.iter().find_map(|((stored_profile, _), retained)| {
                (stored_profile == profile_id
                    && retained.planning_fingerprint == planning_fingerprint)
                    .then(|| retained.preview.clone())
            }) {
                return Ok(existing);
            }
        }

        let mut rng = OsRng;
        let offer: UnshieldedOffer<Signature, DefaultDB> = UnshieldedOffer {
            inputs: inputs.into(),
            outputs: outputs.into(),
            signatures: Vec::new().into(),
        };
        let mut intent = LedgerIntent::empty(
            &mut rng,
            Timestamp::from_secs(request.expires_at.value() / 1_000),
        );
        intent.guaranteed_unshielded_offer = Some(Sp::new(offer));
        let signing_payload = intent
            .erase_proofs()
            .erase_signatures()
            .data_to_sign(SEND_UNSHIELDED_SEGMENT);
        let draft_id = digest_id("txdraft", &signing_payload)?;
        let challenge = authorization_challenge(&draft_id, &signing_payload)?;
        let night = midnight_asset("midnight:night", "NIGHT", STARS_PER_NIGHT)
            .map_err(map_account_error)?;
        let preview = WalletTransferPreview::new(
            draft_id.clone(),
            challenge,
            selected,
            spendable.account.account_id().clone(),
            request.recipient,
            AssetBalance::new(night.clone(), request.amount_atomic_units),
            AssetBalance::new(night, change),
            None,
            WalletTransactionFeeState::RequiresBalancing,
            u16::try_from(selected_utxos.len())
                .map_err(|_| WalletTransactionPortError::InvalidData)?,
            request.expires_at,
            WalletTransactionDraftState::Prepared,
        )
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let retained = RetainedMidnightDraft {
            planning_fingerprint,
            preview: preview.clone(),
            account: spendable.account,
            signing_payload: Zeroizing::new(signing_payload),
            unsigned_intent: intent,
            prepared_shielded_transaction: None,
            signed_transaction: None,
            submission: None,
            submission_state: WalletTransactionSubmissionState::NotStarted,
            submission_control: None,
        };
        let key = (profile_id.clone(), draft_id);
        let mut drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        if let Some(existing) = drafts.iter().find_map(|((stored_profile, _), retained)| {
            (stored_profile == profile_id && retained.planning_fingerprint == planning_fingerprint)
                .then(|| retained.preview.clone())
        }) {
            return Ok(existing);
        }
        if let Some(existing) = drafts.get(&key) {
            return if existing.preview == preview {
                Ok(existing.preview.clone())
            } else {
                Err(WalletTransactionPortError::DraftConflict)
            };
        }
        drafts.insert(key, retained);
        Ok(preview)
    }

    fn prepare_shielded(
        &self,
        profile_id: &WalletProfileId,
        request: PrepareShieldedWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
        let selected = self.selected(profile_id).map_err(map_account_error)?;
        let network = network_by_id(&selected)
            .map_err(map_account_error)?
            .ok_or(WalletTransactionPortError::UnsupportedNetwork)?;
        let (recipient_coin_public_key, recipient_encryption_public_key) =
            decode_shielded_recipient(&request.recipient, &selected)?;
        let token_type = decode_shielded_asset_id(request.asset_id.as_str())?;
        let spendable = self.source.spendable_account(profile_id, &network)?;
        validate_account(&spendable.account, &selected)?;

        {
            let drafts = self
                .drafts
                .lock()
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            for ((stored_profile, _), retained) in drafts.iter() {
                let active_shielded_draft = stored_profile == profile_id
                    && retained.preview.recipient().kind() == ChainAddressKind::Shielded
                    && matches!(
                        retained.preview.state(),
                        WalletTransactionDraftState::Prepared
                            | WalletTransactionDraftState::Authorized
                            | WalletTransactionDraftState::Submitting
                    );
                if !active_shielded_draft {
                    continue;
                }
                if retained.preview.recipient() == &request.recipient
                    && retained.preview.amount().asset().id() == &request.asset_id
                    && retained.preview.amount().atomic_units() == request.amount_atomic_units
                {
                    return Ok(retained.preview.clone());
                }
                // The adapter deliberately retains canonical inputs privately,
                // so it also serializes protected planning per profile. This
                // prevents two active drafts from selecting the same note.
                return Err(WalletTransactionPortError::DraftConflict);
            }
        }

        let plan = self.shielded_sync.prepare_transfer(
            profile_id,
            &selected,
            MidnightShieldedTransferRequest {
                account_index: spendable.account.account_index(),
                recipient_coin_public_key,
                recipient_encryption_public_key,
                token_type,
                amount_atomic_units: request.amount_atomic_units,
                expires_at_seconds: request.expires_at.value() / 1_000,
            },
        )?;
        let mut transaction_bytes = Zeroizing::new(Vec::new());
        midnight_serialize::tagged_serialize(&plan.transaction, &mut *transaction_bytes)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let planning_fingerprint = shielded_planning_fingerprint(
            profile_id,
            &selected,
            &spendable.account,
            &plan.reservation_fingerprint,
        );
        if let Some(stored) = self
            .submission_journal
            .find_planning_fingerprint(profile_id, &planning_fingerprint)
            .map_err(map_submission_store_error)?
        {
            match stored.state {
                StoredSubmissionState::Broadcasting | StoredSubmissionState::OutcomeUnknown => {
                    return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
                }
                StoredSubmissionState::Included => {
                    return Err(WalletTransactionPortError::DraftConflict);
                }
                StoredSubmissionState::Rejected | StoredSubmissionState::Expired => {}
            }
        }
        let draft_id = digest_id("txdraft", transaction_bytes.as_slice())?;
        let challenge = authorization_challenge(&draft_id, transaction_bytes.as_slice())?;
        let native_night = token_type == [0; 32];
        let asset = ChainAsset::new(
            request.asset_id,
            AssetSymbol::parse(if native_night { "NIGHT" } else { "SHIELDED" })
                .map_err(|_| WalletTransactionPortError::InvalidData)?,
            if native_night { 6 } else { 0 },
        );
        let preview = WalletTransferPreview::new(
            draft_id.clone(),
            challenge,
            selected,
            spendable.account.account_id().clone(),
            request.recipient,
            AssetBalance::new(asset.clone(), request.amount_atomic_units),
            AssetBalance::new(asset, plan.change_atomic_units),
            None,
            WalletTransactionFeeState::RequiresBalancing,
            plan.input_count,
            request.expires_at,
            WalletTransactionDraftState::Prepared,
        )
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let retained = RetainedMidnightDraft {
            planning_fingerprint,
            preview: preview.clone(),
            account: spendable.account,
            signing_payload: transaction_bytes,
            unsigned_intent: LedgerIntent::empty(
                &mut OsRng,
                Timestamp::from_secs(request.expires_at.value() / 1_000),
            ),
            prepared_shielded_transaction: Some(plan.transaction),
            signed_transaction: None,
            submission: None,
            submission_state: WalletTransactionSubmissionState::NotStarted,
            submission_control: None,
        };
        let key = (profile_id.clone(), draft_id);
        let mut drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        for ((stored_profile, _), existing) in drafts.iter() {
            if stored_profile != profile_id
                || existing.preview.recipient().kind() != ChainAddressKind::Shielded
                || !matches!(
                    existing.preview.state(),
                    WalletTransactionDraftState::Prepared
                        | WalletTransactionDraftState::Authorized
                        | WalletTransactionDraftState::Submitting
                )
            {
                continue;
            }
            return if existing.preview == preview {
                Ok(existing.preview.clone())
            } else {
                Err(WalletTransactionPortError::DraftConflict)
            };
        }
        drafts.insert(key, retained);
        Ok(preview)
    }

    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        request: AuthorizeWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
        let key = (profile_id.clone(), request.draft_id.clone());
        let mut drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        let retained = drafts
            .get_mut(&key)
            .ok_or(WalletTransactionPortError::DraftNotFound)?;
        if retained.preview.authorization_challenge() != &request.authorization_challenge {
            return Err(WalletTransactionPortError::AuthorizationChallengeMismatch);
        }
        if request.now.value() >= retained.preview.expires_at().value() {
            retained.preview = retained
                .preview
                .with_state(WalletTransactionDraftState::Expired);
            retained.signing_payload = Zeroizing::new(Vec::new());
            retained.prepared_shielded_transaction = None;
            retained.signed_transaction = None;
            return Err(WalletTransactionPortError::DraftExpired);
        }
        if matches!(
            retained.preview.state(),
            WalletTransactionDraftState::Authorized
                | WalletTransactionDraftState::Submitting
                | WalletTransactionDraftState::Submitted
        ) {
            return Ok(retained.preview.clone());
        }

        if retained.preview.recipient().kind() == ChainAddressKind::Shielded {
            retained.signed_transaction = Some(
                retained
                    .prepared_shielded_transaction
                    .take()
                    .ok_or(WalletTransactionPortError::InvalidData)?,
            );
            retained.signing_payload = Zeroizing::new(Vec::new());
            retained.preview = retained
                .preview
                .with_state(WalletTransactionDraftState::Authorized);
            return Ok(retained.preview.clone());
        }

        let signature = self.deriver.authorize(
            profile_id,
            &retained.account,
            retained.signing_payload.as_slice(),
        )?;
        if signature.algorithm() != WalletKeyAlgorithm::Secp256k1Schnorr {
            return Err(WalletTransactionPortError::InvalidData);
        }
        let ledger_signature = decode_signature(&signature)?;
        let verifying_key = decode_verifying_key(&retained.account)?;
        if !verifying_key.verify(retained.signing_payload.as_slice(), &ledger_signature) {
            return Err(WalletTransactionPortError::InvalidData);
        }

        let mut signed = retained.unsigned_intent.clone();
        let offer = signed
            .guaranteed_unshielded_offer
            .as_ref()
            .ok_or(WalletTransactionPortError::InvalidData)?;
        let input_count = offer.inputs.len();
        let mut signed_offer = offer.deref().clone();
        signed_offer.add_signatures(vec![ledger_signature; input_count]);
        signed.guaranteed_unshielded_offer = Some(Sp::new(signed_offer));
        let mut intents = LedgerHashMap::new();
        intents = intents.insert(SEND_UNSHIELDED_SEGMENT, signed);
        let transaction = StandardTransaction::new(
            retained.preview.network_id().as_str(),
            intents,
            None,
            LedgerHashMap::new(),
        );
        retained.signed_transaction = Some(Transaction::Standard(transaction));
        retained.signing_payload = Zeroizing::new(Vec::new());
        retained.preview = retained
            .preview
            .with_state(WalletTransactionDraftState::Authorized);
        Ok(retained.preview.clone())
    }

    fn submit<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        request: SubmitWalletTransferRequest,
    ) -> WalletTransactionPortFuture<'a> {
        Box::pin(async move {
            let key = (profile_id.clone(), request.draft_id.clone());
            let (transaction, account_index, expires_at_seconds, control) = {
                let mut drafts = self
                    .drafts
                    .lock()
                    .map_err(|_| WalletTransactionPortError::Unavailable)?;
                let retained = drafts
                    .get_mut(&key)
                    .ok_or(WalletTransactionPortError::DraftNotFound)?;
                match retained.preview.state() {
                    WalletTransactionDraftState::Submitted => {
                        let submission = retained
                            .submission
                            .clone()
                            .ok_or(WalletTransactionPortError::InvalidData)?;
                        return Ok(SubmittedWalletTransfer {
                            preview: retained.preview.clone(),
                            submission,
                        });
                    }
                    WalletTransactionDraftState::Submitting => {
                        return Err(WalletTransactionPortError::SubmissionInProgress);
                    }
                    WalletTransactionDraftState::Prepared
                    | WalletTransactionDraftState::Authorized => {}
                    WalletTransactionDraftState::Expired => {
                        return Err(WalletTransactionPortError::DraftExpired);
                    }
                }
                if request.now.value() >= retained.preview.expires_at().value() {
                    retained.preview = retained
                        .preview
                        .with_state(WalletTransactionDraftState::Expired);
                    retained.signing_payload = Zeroizing::new(Vec::new());
                    retained.prepared_shielded_transaction = None;
                    retained.signed_transaction = None;
                    return Err(WalletTransactionPortError::DraftExpired);
                }
                if retained.preview.state() == WalletTransactionDraftState::Prepared {
                    return Err(WalletTransactionPortError::DraftConflict);
                }
                let transaction = retained
                    .signed_transaction
                    .clone()
                    .ok_or(WalletTransactionPortError::InvalidData)?;
                let control = Arc::new(MidnightSubmissionControl::new(
                    MidnightSubmissionAttempt {
                        profile_id: profile_id.clone(),
                        network_id: retained.preview.network_id().clone(),
                        draft_id: request.draft_id.clone(),
                        planning_fingerprint: retained.planning_fingerprint,
                        expires_at: retained.preview.expires_at(),
                        updated_at: request.now,
                    },
                    Arc::clone(&self.submission_journal),
                ));
                retained.preview = retained
                    .preview
                    .with_state(WalletTransactionDraftState::Submitting);
                retained.submission_state = WalletTransactionSubmissionState::Running;
                retained.submission_control = Some(Arc::clone(&control));
                (
                    transaction,
                    retained.account.account_index(),
                    retained.preview.expires_at().value() / 1_000,
                    control,
                )
            };

            let profile = profile_id.clone();
            let deriver = self.deriver.clone();
            let completer = Arc::clone(&self.completer);
            let drafts = Arc::clone(&self.drafts);
            let worker_key = key.clone();
            let draft_id = request.draft_id;
            let mut cancel_on_drop = CancelSubmissionOnDrop::new(Arc::clone(&control));
            let worker_control = Arc::clone(&control);
            let (sender, receiver) = futures::channel::oneshot::channel();
            let diagnostics = self.diagnostic_sink();
            let spawn = thread::Builder::new()
                .name("oxid-midnight-submit".to_owned())
                .spawn(move || {
                    let mut operation = |dust_seed: &[u8; 32]| {
                        completer.complete(
                            MidnightCompletionRequest {
                                transaction: transaction.clone(),
                                expires_at_seconds,
                                control: Arc::clone(&worker_control),
                            },
                            dust_seed,
                        )
                    };
                    let completion = deriver.use_dust_seed(&profile, account_index, &mut operation);
                    let result = finish_submission(
                        drafts.as_ref(),
                        &worker_key,
                        draft_id,
                        worker_control.as_ref(),
                        completion,
                    );
                    let _ = sender.send(result);
                });
            if spawn.is_err() {
                diagnostics.record(
                    DiagnosticCode::MidnightTransferWorkerSpawnFailed,
                    DiagnosticSeverity::Error,
                );
                cancel_on_drop.disarm();
                restore_authorized(
                    self.drafts.as_ref(),
                    &key,
                    WalletTransactionSubmissionState::NotStarted,
                )?;
                return Err(WalletTransactionPortError::Unavailable);
            }

            let result = match receiver.await {
                Ok(result) => result,
                Err(_) => {
                    diagnostics.record(
                        DiagnosticCode::MidnightTransferWorkerTerminated,
                        DiagnosticSeverity::Error,
                    );
                    if control.broadcast_started().unwrap_or(true) {
                        let _ = control.mark_terminal(
                            StoredSubmissionState::OutcomeUnknown,
                            None,
                            None,
                        );
                    }
                    mark_submission_outcome_unknown(self.drafts.as_ref(), &key)?;
                    Err(WalletTransactionPortError::SubmissionOutcomeUnknown)
                }
            };
            cancel_on_drop.disarm();
            result
        })
    }

    fn get(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
        now: oxid_foundation::UnixTimestampMillis,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
        let key = (profile_id.clone(), draft_id.clone());
        let mut drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        let retained = drafts
            .get_mut(&key)
            .ok_or(WalletTransactionPortError::DraftNotFound)?;
        if now.value() >= retained.preview.expires_at().value()
            && matches!(
                retained.preview.state(),
                WalletTransactionDraftState::Prepared | WalletTransactionDraftState::Authorized
            )
        {
            retained.preview = retained
                .preview
                .with_state(WalletTransactionDraftState::Expired);
            retained.signing_payload = Zeroizing::new(Vec::new());
            retained.prepared_shielded_transaction = None;
            retained.signed_transaction = None;
        }
        Ok(retained.preview.clone())
    }

    fn submission_status(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<WalletTransactionSubmissionStatus, WalletTransactionPortError> {
        let drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        let retained_status = drafts
            .get(&(profile_id.clone(), draft_id.clone()))
            .map(submission_status)
            .transpose()?;
        drop(drafts);
        let stored = self
            .submission_journal
            .load(profile_id, draft_id)
            .map_err(map_submission_store_error)?;
        match (stored.as_ref(), retained_status) {
            (Some(entry), _) => status_from_stored(entry),
            (None, Some(status)) => Ok(status),
            (None, None) => Err(WalletTransactionPortError::DraftNotFound),
        }
    }

    fn cancel_submission(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<WalletTransactionSubmissionStatus, WalletTransactionPortError> {
        let mut drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        let retained = drafts
            .get_mut(&(profile_id.clone(), draft_id.clone()))
            .ok_or(WalletTransactionPortError::DraftNotFound)?;
        match retained.submission_state {
            WalletTransactionSubmissionState::Running => {
                let control = retained
                    .submission_control
                    .as_ref()
                    .ok_or(WalletTransactionPortError::InvalidData)?;
                control.request_cancellation()?;
                retained.submission_state = WalletTransactionSubmissionState::CancellationRequested;
            }
            WalletTransactionSubmissionState::CancellationRequested
            | WalletTransactionSubmissionState::Cancelled => {}
            WalletTransactionSubmissionState::NotStarted
            | WalletTransactionSubmissionState::Rejected
            | WalletTransactionSubmissionState::Expired => {
                return Err(WalletTransactionPortError::SubmissionNotInProgress);
            }
            WalletTransactionSubmissionState::Included
            | WalletTransactionSubmissionState::Broadcasting
            | WalletTransactionSubmissionState::OutcomeUnknown => {
                return Err(WalletTransactionPortError::SubmissionCancellationUnsafe);
            }
        }
        submission_status(retained)
    }

    fn submission_history(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Vec<WalletTransactionSubmissionStatus>, WalletTransactionPortError> {
        let stored = self
            .submission_journal
            .list(profile_id)
            .map_err(map_submission_store_error)?;
        let mut statuses = stored
            .iter()
            .map(status_from_stored)
            .collect::<Result<Vec<_>, _>>()?;
        let drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        for ((stored_profile, draft_id), retained) in drafts.iter() {
            if stored_profile == profile_id
                && !statuses.iter().any(|status| status.draft_id() == draft_id)
            {
                statuses.push(submission_status(retained)?);
            }
        }
        Ok(statuses)
    }

    fn reconcile_submission<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        draft_id: &'a WalletTransactionDraftId,
    ) -> WalletTransactionStatusPortFuture<'a> {
        Box::pin(async move {
            let entry = self
                .submission_journal
                .load(profile_id, draft_id)
                .map_err(map_submission_store_error)?
                .ok_or(WalletTransactionPortError::DraftNotFound)?;
            let status = status_from_stored(&entry)?;
            if !status.reconciliation_allowed() {
                return Ok(status);
            }

            let reconciler = Arc::clone(&self.submission_reconciler);
            let journal = Arc::clone(&self.submission_journal);
            let drafts = Arc::clone(&self.drafts);
            let (sender, receiver) = futures::channel::oneshot::channel();
            thread::Builder::new()
                .name("oxid-midnight-reconcile".to_owned())
                .spawn(move || {
                    let result = reconciler.reconcile(&entry).and_then(|outcome| {
                        persist_reconciliation(journal.as_ref(), drafts.as_ref(), entry, outcome)
                    });
                    let _ = sender.send(result);
                })
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            receiver
                .await
                .unwrap_or(Err(WalletTransactionPortError::Unavailable))
        })
    }
}

struct CancelSubmissionOnDrop {
    control: Arc<MidnightSubmissionControl>,
    armed: bool,
}

impl CancelSubmissionOnDrop {
    fn new(control: Arc<MidnightSubmissionControl>) -> Self {
        Self {
            control,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CancelSubmissionOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.control.request_cancellation();
        }
    }
}

fn submission_status(
    retained: &RetainedMidnightDraft,
) -> Result<WalletTransactionSubmissionStatus, WalletTransactionPortError> {
    let state = if retained.submission_state == WalletTransactionSubmissionState::Running {
        retained
            .submission_control
            .as_ref()
            .ok_or(WalletTransactionPortError::InvalidData)?
            .public_state()?
    } else {
        retained.submission_state
    };
    Ok(WalletTransactionSubmissionStatus::new(
        retained.preview.draft_id().clone(),
        state,
        retained.submission.clone(),
    ))
}

fn status_from_stored(
    entry: &StoredSubmissionJournalEntry,
) -> Result<WalletTransactionSubmissionStatus, WalletTransactionPortError> {
    let state = match entry.state {
        StoredSubmissionState::Broadcasting => WalletTransactionSubmissionState::Broadcasting,
        StoredSubmissionState::OutcomeUnknown => WalletTransactionSubmissionState::OutcomeUnknown,
        StoredSubmissionState::Included => WalletTransactionSubmissionState::Included,
        StoredSubmissionState::Rejected => WalletTransactionSubmissionState::Rejected,
        StoredSubmissionState::Expired => WalletTransactionSubmissionState::Expired,
    };
    let transaction_id = ChainTransactionId::parse(hex::encode(entry.transaction_hash))
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
    let fee_asset =
        midnight_asset("midnight:dust", "DUST", SPECKS_PER_DUST).map_err(map_account_error)?;
    let fee = AssetBalance::new(fee_asset, entry.fee_specks);
    let submission = match entry.block_hash {
        Some(block_hash) if entry.state == StoredSubmissionState::Included => {
            Some(WalletTransferSubmission::new(
                entry.draft_id.clone(),
                transaction_id.clone(),
                ChainBlockId::parse(hex::encode(block_hash))
                    .map_err(|_| WalletTransactionPortError::InvalidData)?,
                fee.clone(),
                entry.mode,
            ))
        }
        None if entry.state != StoredSubmissionState::Included => None,
        _ => return Err(WalletTransactionPortError::InvalidData),
    };
    Ok(submission.map_or_else(
        || {
            WalletTransactionSubmissionStatus::recorded(
                entry.draft_id.clone(),
                state,
                transaction_id,
                fee,
                entry.mode,
            )
        },
        |submission| {
            WalletTransactionSubmissionStatus::new(entry.draft_id.clone(), state, Some(submission))
        },
    ))
}

fn persist_reconciliation(
    journal: &dyn MidnightSubmissionJournalStore,
    drafts: &RetainedMidnightDrafts,
    mut entry: StoredSubmissionJournalEntry,
    outcome: MidnightSubmissionReconciliation,
) -> Result<WalletTransactionSubmissionStatus, WalletTransactionPortError> {
    match outcome {
        MidnightSubmissionReconciliation::Included {
            block_hash,
            block_height,
        } => {
            entry.state = StoredSubmissionState::Included;
            entry.block_hash = Some(block_hash);
            entry.block_height = Some(block_height);
        }
        MidnightSubmissionReconciliation::Rejected => {
            entry.state = StoredSubmissionState::Rejected;
            entry.block_hash = None;
            entry.block_height = None;
        }
        MidnightSubmissionReconciliation::Expired => {
            entry.state = StoredSubmissionState::Expired;
            entry.block_hash = None;
            entry.block_height = None;
        }
        MidnightSubmissionReconciliation::Unresolved => {
            entry.state = StoredSubmissionState::OutcomeUnknown;
            entry.block_hash = None;
            entry.block_height = None;
        }
    }
    journal.save(&entry).map_err(map_submission_store_error)?;
    let status = status_from_stored(&entry)?;
    if let Ok(mut retained) = drafts.lock() {
        let key = (entry.profile_id.clone(), entry.draft_id.clone());
        match status.state() {
            WalletTransactionSubmissionState::Included => {
                if let (Some(draft), Some(submission)) =
                    (retained.get_mut(&key), status.submission())
                {
                    draft.submission_state = status.state();
                    draft.submission = Some(submission.clone());
                    draft.submission_control = None;
                    draft.preview = draft
                        .preview
                        .with_final_fee(submission.fee().clone())
                        .with_state(WalletTransactionDraftState::Submitted);
                }
            }
            WalletTransactionSubmissionState::Rejected
            | WalletTransactionSubmissionState::Expired => {
                retained.remove(&key);
            }
            _ => {
                if let Some(draft) = retained.get_mut(&key) {
                    draft.submission_state = status.state();
                    draft.submission = status.submission().cloned();
                    draft.submission_control = None;
                }
            }
        }
    }
    Ok(status)
}

const fn map_submission_store_error(
    error: SubmissionJournalStoreError,
) -> WalletTransactionPortError {
    match error {
        SubmissionJournalStoreError::Unavailable => WalletTransactionPortError::Unavailable,
        SubmissionJournalStoreError::InvalidData => WalletTransactionPortError::InvalidData,
    }
}

fn finish_submission(
    drafts: &RetainedMidnightDrafts,
    key: &(WalletProfileId, WalletTransactionDraftId),
    draft_id: WalletTransactionDraftId,
    control: &MidnightSubmissionControl,
    completion: Result<MidnightCompletionOutcome, WalletTransactionPortError>,
) -> Result<SubmittedWalletTransfer, WalletTransactionPortError> {
    let outcome = match completion {
        Ok(outcome) => outcome,
        Err(WalletTransactionPortError::DraftExpired) => {
            expire_submission(drafts, key)?;
            return Err(WalletTransactionPortError::DraftExpired);
        }
        Err(WalletTransactionPortError::SubmissionOutcomeUnknown) => {
            let _ = control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
            mark_submission_outcome_unknown(drafts, key)?;
            return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
        }
        Err(WalletTransactionPortError::SubmissionCancelled) => {
            restore_authorized(drafts, key, WalletTransactionSubmissionState::Cancelled)?;
            return Err(WalletTransactionPortError::SubmissionCancelled);
        }
        Err(WalletTransactionPortError::SubmissionRejected) => {
            if control.broadcast_started()? {
                control.mark_terminal(StoredSubmissionState::Rejected, None, None)?;
                remove_retained_draft(drafts, key)?;
            } else {
                restore_authorized(drafts, key, WalletTransactionSubmissionState::NotStarted)?;
            }
            return Err(WalletTransactionPortError::SubmissionRejected);
        }
        Err(error) => {
            if control.broadcast_started()? {
                let _ = control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
                mark_submission_outcome_unknown(drafts, key)?;
                return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
            }
            restore_authorized(drafts, key, WalletTransactionSubmissionState::NotStarted)?;
            return Err(error);
        }
    };
    if control
        .mark_terminal(
            StoredSubmissionState::Included,
            Some(outcome.block_hash),
            Some(outcome.block_height),
        )
        .is_err()
    {
        let _ = control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
        mark_submission_outcome_unknown(drafts, key)?;
        return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
    }
    let fee_asset =
        midnight_asset("midnight:dust", "DUST", SPECKS_PER_DUST).map_err(map_account_error)?;
    let fee = AssetBalance::new(fee_asset, outcome.fee_specks);
    let submission = WalletTransferSubmission::new(
        draft_id,
        ChainTransactionId::parse(hex::encode(outcome.transaction_hash))
            .map_err(|_| WalletTransactionPortError::InvalidData)?,
        ChainBlockId::parse(hex::encode(outcome.block_hash))
            .map_err(|_| WalletTransactionPortError::InvalidData)?,
        fee.clone(),
        outcome.mode,
    );
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletTransactionPortError::DraftNotFound)?;
    if retained.preview.state() != WalletTransactionDraftState::Submitting {
        return Err(WalletTransactionPortError::DraftConflict);
    }
    retained.preview = retained
        .preview
        .with_final_fee(fee)
        .with_state(WalletTransactionDraftState::Submitted);
    retained.submission = Some(submission.clone());
    retained.submission_state = WalletTransactionSubmissionState::Included;
    retained.submission_control = None;
    retained.signed_transaction = None;
    Ok(SubmittedWalletTransfer {
        preview: retained.preview.clone(),
        submission,
    })
}

fn remove_retained_draft(
    drafts: &RetainedMidnightDrafts,
    key: &(WalletProfileId, WalletTransactionDraftId),
) -> Result<(), WalletTransactionPortError> {
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    drafts
        .remove(key)
        .map(|_| ())
        .ok_or(WalletTransactionPortError::DraftNotFound)
}

fn restore_authorized(
    drafts: &RetainedMidnightDrafts,
    key: &(WalletProfileId, WalletTransactionDraftId),
    submission_state: WalletTransactionSubmissionState,
) -> Result<(), WalletTransactionPortError> {
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletTransactionPortError::DraftNotFound)?;
    if retained.preview.state() == WalletTransactionDraftState::Submitting {
        retained.preview = retained
            .preview
            .with_state(WalletTransactionDraftState::Authorized);
        retained.submission_state = submission_state;
        retained.submission_control = None;
    }
    Ok(())
}

fn mark_submission_outcome_unknown(
    drafts: &RetainedMidnightDrafts,
    key: &(WalletProfileId, WalletTransactionDraftId),
) -> Result<(), WalletTransactionPortError> {
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletTransactionPortError::DraftNotFound)?;
    if retained.preview.state() == WalletTransactionDraftState::Submitting {
        retained.submission_state = WalletTransactionSubmissionState::OutcomeUnknown;
        retained.submission_control = None;
    }
    Ok(())
}

fn expire_submission(
    drafts: &RetainedMidnightDrafts,
    key: &(WalletProfileId, WalletTransactionDraftId),
) -> Result<(), WalletTransactionPortError> {
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletTransactionPortError::DraftNotFound)?;
    if retained.preview.state() == WalletTransactionDraftState::Submitting {
        retained.preview = retained
            .preview
            .with_state(WalletTransactionDraftState::Expired);
        retained.signing_payload = Zeroizing::new(Vec::new());
        retained.prepared_shielded_transaction = None;
        retained.signed_transaction = None;
        retained.submission_state = WalletTransactionSubmissionState::NotStarted;
        retained.submission_control = None;
    }
    Ok(())
}

fn validate_account(
    account: &DerivedChainAccount,
    network_id: &oxid_wallet_domain::ChainNetworkId,
) -> Result<(), WalletTransactionPortError> {
    if account.network_id() != network_id {
        return Err(WalletTransactionPortError::DraftConflict);
    }
    if account.transaction_public_key().encoding() != PublicKeyEncoding::Secp256k1XOnly
        || account.transaction_public_key().bytes().len() != 32
    {
        return Err(WalletTransactionPortError::InvalidData);
    }
    Ok(())
}

fn select_utxos(
    mut utxos: Vec<MidnightSpendableUtxo>,
    amount: u128,
) -> Result<(Vec<MidnightSpendableUtxo>, u128), WalletTransactionPortError> {
    // Match the prototype's greedy picker: largest native UTXOs first, with
    // stable identity tie-breakers so the retained intent is reproducible.
    utxos.sort_by(|left, right| {
        right
            .value
            .cmp(&left.value)
            .then_with(|| left.intent_hash.cmp(&right.intent_hash))
            .then_with(|| left.output_index.cmp(&right.output_index))
    });
    let mut selected = Vec::new();
    let mut total = 0_u128;
    for utxo in utxos {
        total = total
            .checked_add(utxo.value)
            .ok_or(WalletTransactionPortError::InvalidData)?;
        selected.push(utxo);
        if selected.len() > usize::from(MAX_WALLET_TRANSFER_INPUTS) {
            return Err(WalletTransactionPortError::InvalidData);
        }
        if total >= amount {
            return Ok((selected, total));
        }
    }
    Err(WalletTransactionPortError::InsufficientFunds)
}

fn decode_verifying_key(
    account: &DerivedChainAccount,
) -> Result<VerifyingKey, WalletTransactionPortError> {
    validate_account(account, account.network_id())?;
    VerifyingKey::deserialize(
        &mut Cursor::new(account.transaction_public_key().bytes()),
        0,
    )
    .map_err(|_| WalletTransactionPortError::InvalidData)
}

fn decode_signature(signature: &WalletSignature) -> Result<Signature, WalletTransactionPortError> {
    if signature.bytes().len() != 64 {
        return Err(WalletTransactionPortError::InvalidData);
    }
    Signature::deserialize(&mut Cursor::new(signature.bytes()), 0)
        .map_err(|_| WalletTransactionPortError::InvalidData)
}

fn decode_recipient(
    address: &ChainAddress,
    network_id: &oxid_wallet_domain::ChainNetworkId,
) -> Result<UserAddress, WalletTransactionPortError> {
    let decoded = CheckedHrpstring::new::<Bech32m>(address.value())
        .map_err(|_| WalletTransactionPortError::InvalidRecipient)?;
    let expected = if network_id.as_str() == "mainnet" {
        "mn_addr".to_owned()
    } else {
        format!("mn_addr_{}", network_id.as_str())
    };
    if decoded.hrp().as_str() != expected {
        return Err(WalletTransactionPortError::RecipientNetworkMismatch);
    }
    let payload = decoded.byte_iter().collect::<Vec<_>>();
    let bytes: [u8; 32] = payload
        .try_into()
        .map_err(|_| WalletTransactionPortError::InvalidRecipient)?;
    Ok(UserAddress(HashOutput(bytes)))
}

fn decode_shielded_recipient(
    address: &ChainAddress,
    network_id: &oxid_wallet_domain::ChainNetworkId,
) -> Result<
    (
        midnight_coin_structure::coin::PublicKey,
        midnight_transient_crypto::encryption::PublicKey,
    ),
    WalletTransactionPortError,
> {
    if address.kind() != ChainAddressKind::Shielded {
        return Err(WalletTransactionPortError::InvalidRecipient);
    }
    let decoded = CheckedHrpstring::new::<Bech32m>(address.value())
        .map_err(|_| WalletTransactionPortError::InvalidRecipient)?;
    let expected = if network_id.as_str() == "mainnet" {
        "mn_shield-addr".to_owned()
    } else {
        format!("mn_shield-addr_{}", network_id.as_str())
    };
    if decoded.hrp().as_str() != expected {
        return Err(WalletTransactionPortError::RecipientNetworkMismatch);
    }
    let payload = decoded.byte_iter().collect::<Vec<_>>();
    if payload.len() != 64 {
        return Err(WalletTransactionPortError::InvalidRecipient);
    }
    let coin_bytes: [u8; 32] = payload[..32]
        .try_into()
        .map_err(|_| WalletTransactionPortError::InvalidRecipient)?;
    let encryption = midnight_transient_crypto::encryption::PublicKey::deserialize(
        &mut Cursor::new(&payload[32..]),
        0,
    )
    .map_err(|_| WalletTransactionPortError::InvalidRecipient)?;
    Ok((
        midnight_coin_structure::coin::PublicKey(HashOutput(coin_bytes)),
        encryption,
    ))
}

fn decode_shielded_asset_id(value: &str) -> Result<[u8; 32], WalletTransactionPortError> {
    let encoded = value
        .strip_prefix("midnight:shielded:")
        .ok_or(WalletTransactionPortError::InvalidData)?;
    if encoded.len() != 64
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(WalletTransactionPortError::InvalidData);
    }
    hex::decode(encoded)
        .map_err(|_| WalletTransactionPortError::InvalidData)?
        .try_into()
        .map_err(|_| WalletTransactionPortError::InvalidData)
}

fn planning_fingerprint(
    profile_id: &WalletProfileId,
    network_id: &oxid_wallet_domain::ChainNetworkId,
    request: &PrepareWalletTransferRequest,
    account: &DerivedChainAccount,
    utxos: &[MidnightSpendableUtxo],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:midnight:transfer-plan:v1\0");
    digest.update(profile_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(network_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(account.account_id().as_str().as_bytes());
    digest.update([0]);
    digest.update(request.recipient.value().as_bytes());
    digest.update(request.amount_atomic_units.to_be_bytes());
    digest.update(request.expires_at.value().to_be_bytes());
    for utxo in utxos {
        digest.update(utxo.value.to_be_bytes());
        digest.update(utxo.intent_hash);
        digest.update(utxo.output_index.to_be_bytes());
    }
    digest.finalize().into()
}

fn shielded_planning_fingerprint(
    profile_id: &WalletProfileId,
    network_id: &oxid_wallet_domain::ChainNetworkId,
    account: &DerivedChainAccount,
    reservation_fingerprint: &[u8; 32],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:midnight:shielded-transfer-plan:v1\0");
    digest.update(profile_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(network_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(account.account_id().as_str().as_bytes());
    digest.update([0]);
    digest.update(reservation_fingerprint);
    digest.finalize().into()
}

fn digest_id(
    prefix: &str,
    payload: &[u8],
) -> Result<WalletTransactionDraftId, WalletTransactionPortError> {
    let digest = Sha256::digest(payload);
    WalletTransactionDraftId::parse(format!("{prefix}_{}", hex::encode(digest)))
        .map_err(|_| WalletTransactionPortError::InvalidData)
}

fn authorization_challenge(
    draft_id: &WalletTransactionDraftId,
    payload: &[u8],
) -> Result<WalletTransactionAuthorizationChallenge, WalletTransactionPortError> {
    let mut digest = Sha256::new();
    digest.update(b"oxid:midnight:transfer-authorization:v1\0");
    digest.update(draft_id.as_str().as_bytes());
    digest.update(payload);
    WalletTransactionAuthorizationChallenge::parse(format!(
        "txauth_{}",
        hex::encode(digest.finalize())
    ))
    .map_err(|_| WalletTransactionPortError::InvalidData)
}

const fn map_security_error(error: WalletSecurityPortError) -> WalletTransactionPortError {
    match error {
        WalletSecurityPortError::NotInitialized => {
            WalletTransactionPortError::ProtectionNotInitialized
        }
        WalletSecurityPortError::Locked => WalletTransactionPortError::ProtectionLocked,
        WalletSecurityPortError::Unavailable => WalletTransactionPortError::Unavailable,
        WalletSecurityPortError::NotFound => WalletTransactionPortError::DraftConflict,
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::Conflict
        | WalletSecurityPortError::UnsupportedAlgorithm
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => WalletTransactionPortError::InvalidData,
    }
}

const fn map_account_error(
    error: oxid_wallet_application::WalletAccountPortError,
) -> WalletTransactionPortError {
    match error {
        oxid_wallet_application::WalletAccountPortError::Unavailable => {
            WalletTransactionPortError::Unavailable
        }
        oxid_wallet_application::WalletAccountPortError::ProtectionNotInitialized => {
            WalletTransactionPortError::ProtectionNotInitialized
        }
        oxid_wallet_application::WalletAccountPortError::ProtectionLocked => {
            WalletTransactionPortError::ProtectionLocked
        }
        oxid_wallet_application::WalletAccountPortError::UnsupportedNetwork => {
            WalletTransactionPortError::UnsupportedNetwork
        }
        oxid_wallet_application::WalletAccountPortError::NotFound => {
            WalletTransactionPortError::AccountNotDerived
        }
        oxid_wallet_application::WalletAccountPortError::InvalidData => {
            WalletTransactionPortError::InvalidData
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Condvar, Mutex, mpsc},
        task::{Context, Poll, Waker},
        time::{Duration, Instant},
    };

    use midnight_base_crypto::schnorr::SigningKey;
    use midnight_serialize::Serializable;
    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_application::WalletTransactionPort;
    use oxid_wallet_domain::{
        ChainAccountId, ChainAddressKind, PublicKeyEncoding, WalletKeyReference, WalletPublicKey,
    };

    use super::*;
    use crate::{UnavailableMidnightAccountDeriver, fixture_addresses, network_id};

    struct FixedSpendableSource {
        account: DerivedChainAccount,
    }

    #[derive(Clone)]
    struct FixedAuthorizer {
        signing_key: SigningKey,
    }

    impl MidnightTransactionAuthorizer for FixedAuthorizer {
        fn authorize(
            &self,
            _: &WalletProfileId,
            _: &DerivedChainAccount,
            payload: &[u8],
        ) -> Result<WalletSignature, WalletTransactionPortError> {
            let signature = self.signing_key.sign(&mut OsRng, payload);
            let mut bytes = Vec::new();
            signature
                .serialize(&mut bytes)
                .map_err(|_| WalletTransactionPortError::InvalidData)?;
            Ok(WalletSignature::new(
                WalletKeyAlgorithm::Secp256k1Schnorr,
                bytes,
            ))
        }

        fn use_dust_seed(
            &self,
            _: &WalletProfileId,
            _: u32,
            operation: &mut dyn FnMut(
                &[u8; 32],
            ) -> Result<
                MidnightCompletionOutcome,
                WalletTransactionPortError,
            >,
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            operation(&[0x5a; 32])
        }
    }

    struct FailingCompleter;

    impl MidnightTransactionCompleter for FailingCompleter {
        fn complete(
            &self,
            _: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            Err(WalletTransactionPortError::ProvingFailed)
        }
    }

    struct UnknownOutcomeCompleter;

    impl MidnightTransactionCompleter for UnknownOutcomeCompleter {
        fn complete(
            &self,
            _: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            Err(WalletTransactionPortError::SubmissionOutcomeUnknown)
        }
    }

    struct BroadcastUnknownOutcomeCompleter;

    impl MidnightTransactionCompleter for BroadcastUnknownOutcomeCompleter {
        fn complete(
            &self,
            request: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            request.begin_broadcast(42, [7; 32], [8; 32], WalletTransferSubmissionMode::Live)?;
            Err(WalletTransactionPortError::SubmissionOutcomeUnknown)
        }
    }

    struct IncludedReconciler;

    impl MidnightSubmissionReconciler for IncludedReconciler {
        fn reconcile(
            &self,
            _: &StoredSubmissionJournalEntry,
        ) -> Result<MidnightSubmissionReconciliation, WalletTransactionPortError> {
            Ok(MidnightSubmissionReconciliation::Included {
                block_hash: [9; 32],
                block_height: 9,
            })
        }
    }

    struct RejectedReconciler;

    impl MidnightSubmissionReconciler for RejectedReconciler {
        fn reconcile(
            &self,
            _: &StoredSubmissionJournalEntry,
        ) -> Result<MidnightSubmissionReconciliation, WalletTransactionPortError> {
            Ok(MidnightSubmissionReconciliation::Rejected)
        }
    }

    struct PanickingCompleter;

    impl MidnightTransactionCompleter for PanickingCompleter {
        fn complete(
            &self,
            _: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            panic!("test-only unexpected worker termination")
        }
    }

    #[derive(Default)]
    struct RecordingDiagnosticSink {
        events: Mutex<Vec<(DiagnosticCode, DiagnosticSeverity)>>,
    }

    impl oxid_diagnostics_application::DiagnosticEventSinkPort for RecordingDiagnosticSink {
        fn record(&self, code: DiagnosticCode, severity: DiagnosticSeverity) {
            self.events
                .lock()
                .expect("diagnostic event lock")
                .push((code, severity));
        }
    }

    struct BlockingCompleter {
        started: mpsc::SyncSender<()>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    impl MidnightTransactionCompleter for BlockingCompleter {
        fn complete(
            &self,
            request: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            request.begin_broadcast(
                1,
                [1; 32],
                [0; 32],
                WalletTransferSubmissionMode::Simulated,
            )?;
            self.started
                .send(())
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            let (lock, condition) = self.release.as_ref();
            let mut released = lock
                .lock()
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            while !*released {
                released = condition
                    .wait(released)
                    .map_err(|_| WalletTransactionPortError::Unavailable)?;
            }
            Ok(MidnightCompletionOutcome {
                fee_specks: 1,
                transaction_hash: [1; 32],
                block_hash: [2; 32],
                block_height: 2,
                mode: WalletTransferSubmissionMode::Simulated,
            })
        }
    }

    struct CancellationAwareCompleter {
        started: mpsc::SyncSender<()>,
    }

    impl MidnightTransactionCompleter for CancellationAwareCompleter {
        fn complete(
            &self,
            request: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            self.started
                .send(())
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            let cancellation = request.cancellation_token();
            let deadline = Instant::now() + Duration::from_secs(1);
            while !cancellation.load(Ordering::Acquire) {
                if Instant::now() >= deadline {
                    return Err(WalletTransactionPortError::Unavailable);
                }
                std::thread::yield_now();
            }
            Err(WalletTransactionPortError::SubmissionCancelled)
        }
    }

    struct BroadcastBlockingCompleter {
        started: mpsc::SyncSender<()>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    impl MidnightTransactionCompleter for BroadcastBlockingCompleter {
        fn complete(
            &self,
            request: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            request.begin_broadcast(
                1,
                [1; 32],
                [0; 32],
                WalletTransferSubmissionMode::Simulated,
            )?;
            self.started
                .send(())
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            let (lock, condition) = self.release.as_ref();
            let mut released = lock
                .lock()
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            while !*released {
                released = condition
                    .wait(released)
                    .map_err(|_| WalletTransactionPortError::Unavailable)?;
            }
            Ok(MidnightCompletionOutcome {
                fee_specks: 1,
                transaction_hash: [1; 32],
                block_hash: [2; 32],
                block_height: 2,
                mode: WalletTransferSubmissionMode::Simulated,
            })
        }
    }

    impl MidnightTransactionSource for FixedSpendableSource {
        fn spendable_account(
            &self,
            _: &WalletProfileId,
            _: &ChainNetwork,
        ) -> Result<MidnightSpendableAccount, WalletTransactionPortError> {
            Ok(MidnightSpendableAccount {
                account: self.account.clone(),
                utxos: vec![
                    simulated_utxo(STARS_PER_NIGHT, 1, 0),
                    simulated_utxo(2 * STARS_PER_NIGHT, 2, 0),
                    simulated_utxo(2 * STARS_PER_NIGHT, 3, 0),
                ],
            })
        }
    }

    fn adapter() -> MidnightWalletAdapter<FixedSpendableSource, UnavailableMidnightAccountDeriver> {
        let network = network_id("undeployed").expect("network is valid");
        let address = fixture_addresses(&network)
            .expect("fixture addresses encode")
            .remove(0);
        let public_key =
            hex::decode("b193e54524dc796402870a883fbdcd83869c9c307dda8c0d99c5f769169fc883")
                .expect("public key vector is valid");
        let account = DerivedChainAccount::new(
            network,
            ChainAccountId::parse("midnight_account_0_0").expect("account id is valid"),
            0,
            0,
            address,
            WalletPublicKey::new(PublicKeyEncoding::Secp256k1XOnly, public_key),
            WalletKeyReference::parse("key_test").expect("key reference is valid"),
        )
        .expect("derived account is valid");
        MidnightWalletAdapter::with_deriver(
            FixedSpendableSource { account },
            UnavailableMidnightAccountDeriver,
        )
    }

    fn submittable_adapter(
        completer: Arc<dyn MidnightTransactionCompleter>,
    ) -> MidnightWalletAdapter<FixedSpendableSource, FixedAuthorizer> {
        let network = network_id("undeployed").expect("network is valid");
        let address = fixture_addresses(&network)
            .expect("fixture addresses encode")
            .remove(0);
        let signing_key = SigningKey::from_bytes(&[3; 32]).expect("test scalar is valid");
        let mut public_key = Vec::new();
        signing_key
            .verifying_key()
            .serialize(&mut public_key)
            .expect("verifying key serializes");
        let account = DerivedChainAccount::new(
            network,
            ChainAccountId::parse("midnight_account_0_0").expect("account id is valid"),
            0,
            0,
            address,
            WalletPublicKey::new(PublicKeyEncoding::Secp256k1XOnly, public_key),
            WalletKeyReference::parse("key_test").expect("key reference is valid"),
        )
        .expect("derived account is valid");
        MidnightWalletAdapter::with_deriver_and_completer(
            FixedSpendableSource { account },
            FixedAuthorizer { signing_key },
            completer,
        )
    }

    fn authorize_transfer(
        adapter: &MidnightWalletAdapter<FixedSpendableSource, FixedAuthorizer>,
    ) -> WalletTransferPreview {
        let prepared = adapter
            .prepare(&profile(), request(2_000))
            .expect("transfer prepares");
        adapter
            .authorize(
                &profile(),
                AuthorizeWalletTransferRequest {
                    draft_id: prepared.draft_id().clone(),
                    authorization_challenge: prepared.authorization_challenge().clone(),
                    now: UnixTimestampMillis::new(1_000),
                },
            )
            .expect("transfer authorizes")
    }

    fn serialized_contract_call_with_night_shortfall(amount: u128) -> Zeroizing<Vec<u8>> {
        let recipient = SigningKey::from_bytes(&[9; 32])
            .expect("test scalar")
            .verifying_key();
        let offer = UnshieldedOffer {
            inputs: Vec::new().into(),
            outputs: vec![UtxoOutput {
                value: amount,
                owner: UserAddress::from(recipient),
                type_: NIGHT,
            }]
            .into(),
            signatures: Vec::new().into(),
        };
        let mut intent = LedgerIntent::empty(&mut OsRng, Timestamp::from_secs(1_800_000_000));
        intent.guaranteed_unshielded_offer = Some(Sp::new(offer));
        let mut intents = LedgerHashMap::new();
        intents = intents.insert(7, intent);
        let transaction = Transaction::Standard(StandardTransaction::new(
            "undeployed",
            intents,
            None,
            LedgerHashMap::new(),
        ));
        let mut encoded = Zeroizing::new(Vec::new());
        midnight_serialize::tagged_serialize(&transaction, &mut *encoded)
            .expect("transaction serializes");
        encoded
    }

    #[test]
    fn protected_contract_funding_covers_exact_night_shortfall_and_signs_inputs() {
        let adapter = submittable_adapter(Arc::new(SimulatedMidnightTransactionCompleter));
        let funded = adapter
            .fund_contract_call(MidnightContractCallFundingRequest::new(
                profile().as_str(),
                "undeployed",
                1_800_000_000,
                true,
                serialized_contract_call_with_night_shortfall(2_500_000),
            ))
            .expect("contract call funding succeeds");
        assert_eq!(funded.funded_night_atomic_units(), 2_500_000);
        assert_eq!(funded.funding_input_count(), 2);
        let debug = format!("{funded:?}");
        assert!(debug.contains("funded_night_atomic_units: 2500000"));
        assert!(!debug.contains("03030303"));

        let encoded = funded.into_transaction();
        let mut cursor = Cursor::new(encoded.as_slice());
        let transaction: LedgerTransaction =
            midnight_serialize::tagged_deserialize(&mut cursor).expect("funded transaction");
        assert_eq!(unshielded_night_shortfall(&transaction), Ok(None));
        let Transaction::Standard(standard) = transaction else {
            panic!("standard transaction expected");
        };
        let funding = standard
            .intents
            .clone()
            .into_iter()
            .find_map(|(segment, intent)| {
                (segment == CONTRACT_UNSHIELDED_FUNDING_SEGMENT).then_some(intent)
            })
            .expect("funding intent");
        let offer = funding
            .guaranteed_unshielded_offer
            .as_ref()
            .expect("funding offer");
        assert_eq!(offer.inputs.len(), 2);
        assert_eq!(offer.signatures.len(), 2);
        assert_eq!(
            offer
                .outputs
                .iter()
                .map(|output| output.value)
                .sum::<u128>(),
            1_500_000
        );
    }

    #[test]
    fn contract_call_completion_is_journaled_separately_and_idempotently() {
        let directory =
            std::env::temp_dir().join(format!("oxid-contract-call-journal-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("journal directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                .expect("private journal directory");
        }
        let config = crate::MidnightSubmissionJournalConfig::new(directory.join("journal.json"))
            .expect("journal config");
        let adapter = submittable_adapter(Arc::new(SimulatedMidnightTransactionCompleter))
            .with_submission_recovery(
                Arc::new(
                    crate::submission_journal::JsonMidnightSubmissionJournalStore::new(
                        config.clone(),
                    ),
                ),
                Arc::new(UnavailableMidnightSubmissionReconciler),
            );
        let funded = adapter
            .fund_contract_call(MidnightContractCallFundingRequest::new(
                profile().as_str(),
                "undeployed",
                1_800_000_000,
                true,
                serialized_contract_call_with_night_shortfall(2_500_000),
            ))
            .expect("funding succeeds")
            .into_transaction();
        let complete = || {
            adapter.complete_contract_call(MidnightContractCallSubmissionRequest::new(
                profile().as_str(),
                "undeployed",
                "vault-draft-test",
                [9; 32],
                UnixTimestampMillis::new(1_800_000_000_000),
                UnixTimestampMillis::new(2_000),
                Zeroizing::new(funded.to_vec()),
            ))
        };

        let first = complete().expect("completion succeeds");
        let repeated = complete().expect("included completion is idempotent");
        assert_eq!(repeated, first);
        assert_eq!(first.block_height, 1);
        assert_eq!(first.mode, MidnightContractCallSubmissionMode::Simulated);
        let status = adapter
            .contract_call_submission_status(profile().as_str(), "vault-draft-test")
            .expect("status is restored from the journal");
        assert_eq!(status.state, MidnightContractCallSubmissionState::Included);
        assert_eq!(status.block_height, Some(1));
        assert_eq!(
            adapter
                .contract_call_submission_history(profile().as_str())
                .expect("contract history")
                .len(),
            1
        );
        assert!(
            adapter
                .submission_history(&profile())
                .expect("ordinary transfer history")
                .is_empty()
        );
        drop(adapter);
        let restored = submittable_adapter(Arc::new(SimulatedMidnightTransactionCompleter))
            .with_submission_recovery(
                Arc::new(
                    crate::submission_journal::JsonMidnightSubmissionJournalStore::new(
                        config.clone(),
                    ),
                ),
                Arc::new(UnavailableMidnightSubmissionReconciler),
            );
        assert_eq!(
            restored
                .contract_call_submission_status(profile().as_str(), "vault-draft-test")
                .expect("included status survives adapter reconstruction")
                .block_height,
            Some(1)
        );
        std::fs::remove_file(config.path()).expect("journal cleanup");
        std::fs::remove_dir(directory).expect("directory cleanup");
    }

    #[test]
    fn panicking_contract_call_releases_the_active_reservation_and_records_only_a_code() {
        let adapter = submittable_adapter(Arc::new(PanickingCompleter));
        let diagnostics = Arc::new(RecordingDiagnosticSink::default());
        crate::MidnightDiagnosticAttachPort::attach_diagnostic_sink(&adapter, diagnostics.clone());
        let funded = adapter
            .fund_contract_call(MidnightContractCallFundingRequest::new(
                profile().as_str(),
                "undeployed",
                1_800_000_000,
                true,
                serialized_contract_call_with_night_shortfall(1),
            ))
            .expect("funding succeeds")
            .into_transaction();
        let complete = || {
            adapter.complete_contract_call(MidnightContractCallSubmissionRequest::new(
                profile().as_str(),
                "undeployed",
                "vault-panic-test",
                [5; 32],
                UnixTimestampMillis::new(1_800_000_000_000),
                UnixTimestampMillis::new(2_000),
                Zeroizing::new(funded.to_vec()),
            ))
        };

        assert_eq!(complete(), Err(WalletTransactionPortError::Unavailable));
        assert_eq!(complete(), Err(WalletTransactionPortError::Unavailable));
        assert_eq!(
            adapter.cancel_contract_call_submission(profile().as_str(), "vault-panic-test"),
            Err(WalletTransactionPortError::SubmissionNotInProgress)
        );
        assert_eq!(
            diagnostics.events.lock().expect("events").as_slice(),
            &[
                (
                    DiagnosticCode::MidnightContractCallWorkerPanicked,
                    DiagnosticSeverity::Error,
                ),
                (
                    DiagnosticCode::MidnightContractCallWorkerPanicked,
                    DiagnosticSeverity::Error,
                ),
            ]
        );
    }

    #[test]
    fn contract_call_cancellation_is_allowed_only_before_broadcast() {
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let adapter = Arc::new(submittable_adapter(Arc::new(CancellationAwareCompleter {
            started: started_sender,
        })));
        let funded = adapter
            .fund_contract_call(MidnightContractCallFundingRequest::new(
                profile().as_str(),
                "undeployed",
                1_800_000_000,
                true,
                serialized_contract_call_with_night_shortfall(1),
            ))
            .expect("funding succeeds")
            .into_transaction();
        let worker = Arc::clone(&adapter);
        let submission = std::thread::spawn(move || {
            worker.complete_contract_call(MidnightContractCallSubmissionRequest::new(
                profile().as_str(),
                "undeployed",
                "vault-cancel-test",
                [7; 32],
                UnixTimestampMillis::new(1_800_000_000_000),
                UnixTimestampMillis::new(2_000),
                funded,
            ))
        });
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("completion reaches cancellable work");
        assert_eq!(
            adapter
                .cancel_contract_call_submission(profile().as_str(), "vault-cancel-test")
                .expect("pre-broadcast cancellation is accepted")
                .state,
            MidnightContractCallSubmissionState::CancellationRequested
        );
        assert_eq!(
            submission.join().expect("completion worker"),
            Err(WalletTransactionPortError::SubmissionCancelled)
        );

        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let adapter = Arc::new(submittable_adapter(Arc::new(BroadcastBlockingCompleter {
            started: started_sender,
            release: Arc::clone(&release),
        })));
        let funded = adapter
            .fund_contract_call(MidnightContractCallFundingRequest::new(
                profile().as_str(),
                "undeployed",
                1_800_000_000,
                true,
                serialized_contract_call_with_night_shortfall(1),
            ))
            .expect("funding succeeds")
            .into_transaction();
        let worker = Arc::clone(&adapter);
        let submission = std::thread::spawn(move || {
            worker.complete_contract_call(MidnightContractCallSubmissionRequest::new(
                profile().as_str(),
                "undeployed",
                "vault-broadcast-test",
                [8; 32],
                UnixTimestampMillis::new(1_800_000_000_000),
                UnixTimestampMillis::new(2_000),
                funded,
            ))
        });
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("completion crosses broadcast boundary");
        assert_eq!(
            adapter.cancel_contract_call_submission(profile().as_str(), "vault-broadcast-test"),
            Err(WalletTransactionPortError::SubmissionCancellationUnsafe)
        );
        let (lock, condition) = release.as_ref();
        *lock.lock().expect("release lock") = true;
        condition.notify_all();
        assert!(submission.join().expect("completion worker").is_ok());
    }

    #[test]
    fn ambiguous_contract_call_is_not_rebroadcast_and_reconciles_from_finalized_history() {
        let adapter = submittable_adapter(Arc::new(BroadcastUnknownOutcomeCompleter))
            .with_submission_recovery(
                Arc::new(
                    crate::submission_journal::MemoryMidnightSubmissionJournalStore::default(),
                ),
                Arc::new(IncludedReconciler),
            );
        let funded = adapter
            .fund_contract_call(MidnightContractCallFundingRequest::new(
                profile().as_str(),
                "undeployed",
                1_800_000_000,
                true,
                serialized_contract_call_with_night_shortfall(1),
            ))
            .expect("funding succeeds")
            .into_transaction();
        let complete = || {
            adapter.complete_contract_call(MidnightContractCallSubmissionRequest::new(
                profile().as_str(),
                "undeployed",
                "vault-unknown-test",
                [6; 32],
                UnixTimestampMillis::new(1_800_000_000_000),
                UnixTimestampMillis::new(2_000),
                Zeroizing::new(funded.to_vec()),
            ))
        };
        assert_eq!(
            complete(),
            Err(WalletTransactionPortError::SubmissionOutcomeUnknown)
        );
        assert_eq!(
            complete(),
            Err(WalletTransactionPortError::SubmissionOutcomeUnknown)
        );
        let status = adapter
            .contract_call_submission_status(profile().as_str(), "vault-unknown-test")
            .expect("unknown status is journaled");
        assert_eq!(
            status.state,
            MidnightContractCallSubmissionState::OutcomeUnknown
        );
        assert_eq!(status.transaction_hash, Some([7; 32]));

        let reconciled = adapter
            .reconcile_contract_call_submission(profile().as_str(), "vault-unknown-test")
            .expect("finalized inclusion reconciles");
        assert_eq!(
            reconciled.state,
            MidnightContractCallSubmissionState::Included
        );
        assert_eq!(reconciled.block_hash, Some([9; 32]));
        assert_eq!(reconciled.block_height, Some(9));
        let idempotent = complete().expect("included result is idempotent");
        assert_eq!(idempotent.transaction_hash, [7; 32]);
        assert_eq!(idempotent.block_hash, [9; 32]);
        assert_eq!(idempotent.block_height, 9);
    }

    #[test]
    fn contract_funding_rejects_wrong_mode_network_and_trailing_bytes() {
        let adapter = submittable_adapter(Arc::new(SimulatedMidnightTransactionCompleter));
        assert_eq!(
            adapter
                .fund_contract_call(MidnightContractCallFundingRequest::new(
                    profile().as_str(),
                    "undeployed",
                    1_800_000_000,
                    false,
                    serialized_contract_call_with_night_shortfall(1),
                ))
                .err(),
            Some(WalletTransactionPortError::InvalidChainState)
        );
        assert_eq!(
            adapter
                .fund_contract_call(MidnightContractCallFundingRequest::new(
                    profile().as_str(),
                    "devnet",
                    1_800_000_000,
                    true,
                    serialized_contract_call_with_night_shortfall(1),
                ))
                .err(),
            Some(WalletTransactionPortError::UnsupportedNetwork)
        );
        let mut trailing = serialized_contract_call_with_night_shortfall(1);
        trailing.push(0);
        assert_eq!(
            adapter
                .fund_contract_call(MidnightContractCallFundingRequest::new(
                    profile().as_str(),
                    "undeployed",
                    1_800_000_000,
                    true,
                    trailing,
                ))
                .err(),
            Some(WalletTransactionPortError::InvalidData)
        );
    }

    fn profile() -> WalletProfileId {
        WalletProfileId::parse("profile_test").expect("profile is valid")
    }

    fn request(expires_at: u64) -> PrepareWalletTransferRequest {
        let recipient = fixture_addresses(&network_id("undeployed").expect("network is valid"))
            .expect("fixture addresses encode")
            .remove(0);
        assert_eq!(recipient.kind(), ChainAddressKind::Unshielded);
        PrepareWalletTransferRequest {
            recipient,
            amount_atomic_units: 1_500_000,
            expires_at: UnixTimestampMillis::new(expires_at),
        }
    }

    #[test]
    fn planning_matches_prototype_greedy_selection_and_is_idempotent() {
        let adapter = adapter();
        let first = adapter
            .prepare(&profile(), request(2_000))
            .expect("transfer prepares");
        let repeated = adapter
            .prepare(&profile(), request(2_000))
            .expect("same transfer is idempotent");

        assert_eq!(first.input_count(), 1);
        assert_eq!(first.change().atomic_units(), 500_000);
        assert_eq!(first.draft_id(), repeated.draft_id());
        assert_eq!(
            first.authorization_challenge(),
            repeated.authorization_challenge()
        );
    }

    #[test]
    fn retained_material_expires_without_becoming_submission_ready() {
        let adapter = adapter();
        let prepared = adapter
            .prepare(&profile(), request(2_000))
            .expect("transfer prepares");
        let expired = adapter
            .get(
                &profile(),
                prepared.draft_id(),
                UnixTimestampMillis::new(2_000),
            )
            .expect("safe expired state is readable");

        assert_eq!(expired.state(), WalletTransactionDraftState::Expired);
        assert_eq!(
            expired.fee_state(),
            WalletTransactionFeeState::RequiresBalancing
        );
    }

    #[test]
    fn selection_rejects_insufficient_oversized_and_overflowing_inputs() {
        assert_eq!(
            select_utxos(vec![simulated_utxo(1, 1, 0)], 2),
            Err(WalletTransactionPortError::InsufficientFunds)
        );

        let oversized = (0..=MAX_WALLET_TRANSFER_INPUTS)
            .map(|index| simulated_utxo(1, 1, u32::from(index)))
            .collect();
        assert_eq!(
            select_utxos(oversized, u128::from(MAX_WALLET_TRANSFER_INPUTS) + 2),
            Err(WalletTransactionPortError::InvalidData)
        );

        assert_eq!(
            select_utxos(
                vec![simulated_utxo(u128::MAX - 1, 1, 0), simulated_utxo(2, 2, 0),],
                u128::MAX,
            ),
            Err(WalletTransactionPortError::InvalidData)
        );
    }

    #[test]
    fn dust_witness_uses_the_wallet_sdk_role_two_child() {
        let path = dust_path(7).expect("DUST path is valid");
        let components = path
            .components()
            .iter()
            .map(|component| (component.index(), component.hardened()))
            .collect::<Vec<_>>();

        assert_eq!(
            components,
            vec![(44, true), (2400, true), (7, true), (2, false), (0, false)]
        );
    }

    #[test]
    fn simulated_submission_is_final_and_idempotent() {
        let adapter = submittable_adapter(Arc::new(SimulatedMidnightTransactionCompleter));
        let authorized = authorize_transfer(&adapter);
        let request = SubmitWalletTransferRequest {
            draft_id: authorized.draft_id().clone(),
            now: UnixTimestampMillis::new(1_000),
        };
        let first = futures::executor::block_on(adapter.submit(&profile(), request.clone()))
            .expect("transfer submits");
        let repeated = futures::executor::block_on(adapter.submit(&profile(), request))
            .expect("submitted transfer is idempotent");
        let repeated_after_draft_ttl = futures::executor::block_on(adapter.submit(
            &profile(),
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(3_000),
            },
        ))
        .expect("completed outcome remains idempotent after the draft TTL");

        assert_eq!(
            first.preview.state(),
            WalletTransactionDraftState::Submitted
        );
        assert_eq!(first.preview.fee_state(), WalletTransactionFeeState::Final);
        assert_eq!(
            first.submission.mode(),
            WalletTransferSubmissionMode::Simulated
        );
        assert_eq!(first.submission, repeated.submission);
        assert_eq!(first.submission, repeated_after_draft_ttl.submission);
    }

    #[test]
    fn completion_failure_restores_authorized_state_for_retry() {
        let adapter = submittable_adapter(Arc::new(FailingCompleter));
        let authorized = authorize_transfer(&adapter);
        let error = futures::executor::block_on(adapter.submit(
            &profile(),
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        ))
        .expect_err("proving failure is returned");

        assert_eq!(error, WalletTransactionPortError::ProvingFailed);
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000)
                )
                .expect("draft remains readable")
                .state(),
            WalletTransactionDraftState::Authorized
        );
    }

    #[test]
    fn unknown_node_outcome_cannot_be_retried_as_a_second_send() {
        let adapter = submittable_adapter(Arc::new(UnknownOutcomeCompleter));
        let authorized = authorize_transfer(&adapter);
        let request = SubmitWalletTransferRequest {
            draft_id: authorized.draft_id().clone(),
            now: UnixTimestampMillis::new(1_000),
        };
        let error = futures::executor::block_on(adapter.submit(&profile(), request.clone()))
            .expect_err("unknown node outcome is returned");
        assert_eq!(error, WalletTransactionPortError::SubmissionOutcomeUnknown);
        assert_eq!(
            adapter
                .submission_status(&profile(), authorized.draft_id())
                .expect("unknown submission status is readable")
                .state(),
            WalletTransactionSubmissionState::OutcomeUnknown
        );
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000),
                )
                .expect("ambiguous draft remains readable")
                .state(),
            WalletTransactionDraftState::Submitting
        );
        let repeated = futures::executor::block_on(adapter.submit(&profile(), request))
            .expect_err("ambiguous submission cannot be sent again");
        assert_eq!(repeated, WalletTransactionPortError::SubmissionInProgress);
    }

    #[test]
    fn durable_unknown_submission_is_restored_reconciled_and_never_duplicated() {
        let journal: Arc<dyn MidnightSubmissionJournalStore> =
            Arc::new(crate::submission_journal::MemoryMidnightSubmissionJournalStore::default());
        let adapter = submittable_adapter(Arc::new(BroadcastUnknownOutcomeCompleter))
            .with_submission_recovery(Arc::clone(&journal), Arc::new(IncludedReconciler));
        let authorized = authorize_transfer(&adapter);
        let draft_id = authorized.draft_id().clone();
        let error = futures::executor::block_on(adapter.submit(
            &profile(),
            SubmitWalletTransferRequest {
                draft_id: draft_id.clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        ))
        .expect_err("uncertain broadcast remains unresolved");
        assert_eq!(error, WalletTransactionPortError::SubmissionOutcomeUnknown);
        let history = adapter
            .submission_history(&profile())
            .expect("journal history is readable");
        assert_eq!(history.len(), 1);
        assert_eq!(
            history[0].state(),
            WalletTransactionSubmissionState::OutcomeUnknown
        );
        assert_eq!(
            history[0]
                .transaction_id()
                .expect("broadcast hash is public")
                .as_str(),
            hex::encode([7; 32])
        );
        assert_eq!(
            history[0]
                .fee()
                .expect("broadcast fee is public")
                .atomic_units(),
            42
        );
        assert!(history[0].reconciliation_allowed());

        let restarted = submittable_adapter(Arc::new(SimulatedMidnightTransactionCompleter))
            .with_submission_recovery(Arc::clone(&journal), Arc::new(IncludedReconciler));
        assert_eq!(
            restarted
                .submission_status(&profile(), &draft_id)
                .expect("restart restores public status")
                .state(),
            WalletTransactionSubmissionState::OutcomeUnknown
        );
        assert_eq!(
            restarted.prepare(&profile(), request(2_000)),
            Err(WalletTransactionPortError::SubmissionOutcomeUnknown)
        );

        let reconciled =
            futures::executor::block_on(restarted.reconcile_submission(&profile(), &draft_id))
                .expect("finalized inclusion reconciles");
        assert_eq!(
            reconciled.state(),
            WalletTransactionSubmissionState::Included
        );
        assert_eq!(
            reconciled
                .submission()
                .expect("inclusion metadata is public")
                .transaction_id()
                .as_str(),
            hex::encode([7; 32])
        );
        assert_eq!(
            reconciled
                .submission()
                .expect("inclusion metadata is public")
                .block_id()
                .as_str(),
            hex::encode([9; 32])
        );
        assert_eq!(
            restarted.prepare(&profile(), request(2_000)),
            Err(WalletTransactionPortError::DraftConflict)
        );
    }

    #[test]
    fn finalized_rejection_retires_signed_material_and_allows_fresh_planning() {
        let journal: Arc<dyn MidnightSubmissionJournalStore> =
            Arc::new(crate::submission_journal::MemoryMidnightSubmissionJournalStore::default());
        let adapter = submittable_adapter(Arc::new(BroadcastUnknownOutcomeCompleter))
            .with_submission_recovery(Arc::clone(&journal), Arc::new(RejectedReconciler));
        let authorized = authorize_transfer(&adapter);
        let draft_id = authorized.draft_id().clone();
        let _ = futures::executor::block_on(adapter.submit(
            &profile(),
            SubmitWalletTransferRequest {
                draft_id: draft_id.clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        ));

        let reconciled =
            futures::executor::block_on(adapter.reconcile_submission(&profile(), &draft_id))
                .expect("finalized rejection reconciles");
        assert_eq!(
            reconciled.state(),
            WalletTransactionSubmissionState::Rejected
        );
        assert!(reconciled.replacement_allowed());
        assert_eq!(
            adapter.get(&profile(), &draft_id, UnixTimestampMillis::new(1_000)),
            Err(WalletTransactionPortError::DraftNotFound)
        );
        let replacement = adapter
            .prepare(&profile(), request(2_000))
            .expect("fresh planning is allowed after finalized rejection");
        assert_eq!(replacement.state(), WalletTransactionDraftState::Prepared);
    }

    #[test]
    fn unexpected_worker_termination_is_an_unknown_non_retryable_outcome() {
        let adapter = submittable_adapter(Arc::new(PanickingCompleter));
        let authorized = authorize_transfer(&adapter);
        let request = SubmitWalletTransferRequest {
            draft_id: authorized.draft_id().clone(),
            now: UnixTimestampMillis::new(1_000),
        };
        let error = futures::executor::block_on(adapter.submit(&profile(), request.clone()))
            .expect_err("worker termination is returned as an unknown outcome");
        assert_eq!(error, WalletTransactionPortError::SubmissionOutcomeUnknown);
        assert_eq!(
            adapter
                .submission_status(&profile(), authorized.draft_id())
                .expect("terminated worker status is readable")
                .state(),
            WalletTransactionSubmissionState::OutcomeUnknown
        );
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000),
                )
                .expect("worker-owned draft remains readable")
                .state(),
            WalletTransactionDraftState::Submitting
        );
        let repeated = futures::executor::block_on(adapter.submit(&profile(), request))
            .expect_err("unknown worker outcome cannot be sent again");
        assert_eq!(repeated, WalletTransactionPortError::SubmissionInProgress);
    }

    #[test]
    fn cancelling_submission_future_leaves_the_worker_owning_the_final_transition() {
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let adapter = submittable_adapter(Arc::new(BlockingCompleter {
            started: started_sender,
            release: Arc::clone(&release),
        }));
        let authorized = authorize_transfer(&adapter);
        let profile = profile();
        let mut future = adapter.submit(
            &profile,
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        );
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("completion worker starts");
        drop(future);

        assert_eq!(
            adapter
                .get(
                    &profile,
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000)
                )
                .expect("cancelled draft remains readable")
                .state(),
            WalletTransactionDraftState::Submitting
        );
        let (lock, condition) = release.as_ref();
        *lock.lock().expect("release lock is available") = true;
        condition.notify_one();

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let state = adapter
                .get(
                    &profile,
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000),
                )
                .expect("worker-owned draft remains readable")
                .state();
            if state == WalletTransactionDraftState::Submitted {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "submission worker did not publish its final state"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn cancellation_aware_completion_restores_the_authorized_draft() {
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let adapter = submittable_adapter(Arc::new(CancellationAwareCompleter {
            started: started_sender,
        }));
        let authorized = authorize_transfer(&adapter);
        let profile = profile();
        let mut future = adapter.submit(
            &profile,
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        );
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("completion worker starts");
        drop(future);

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let state = adapter
                .get(
                    &profile,
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000),
                )
                .expect("cancelled draft remains readable")
                .state();
            if state == WalletTransactionDraftState::Authorized {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cancelled worker did not restore the authorized state"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn explicit_cancellation_is_reported_and_leaves_the_draft_retryable() {
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let adapter = submittable_adapter(Arc::new(CancellationAwareCompleter {
            started: started_sender,
        }));
        let authorized = authorize_transfer(&adapter);
        let profile = profile();
        assert_eq!(
            adapter
                .submission_status(&profile, authorized.draft_id())
                .expect("initial submission status is readable")
                .state(),
            WalletTransactionSubmissionState::NotStarted
        );
        let mut future = adapter.submit(
            &profile,
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        );
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("completion worker starts");

        let requested = adapter
            .cancel_submission(&profile, authorized.draft_id())
            .expect("pre-broadcast cancellation is accepted");
        assert_eq!(
            requested.state(),
            WalletTransactionSubmissionState::CancellationRequested
        );
        assert!(!requested.cancellation_allowed());
        assert_eq!(
            futures::executor::block_on(future),
            Err(WalletTransactionPortError::SubmissionCancelled)
        );
        let final_status = adapter
            .submission_status(&profile, authorized.draft_id())
            .expect("cancelled status remains readable");
        assert_eq!(
            final_status.state(),
            WalletTransactionSubmissionState::Cancelled
        );
        assert!(final_status.retryable());
        assert_eq!(
            adapter
                .get(
                    &profile,
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000),
                )
                .expect("cancelled draft remains readable")
                .state(),
            WalletTransactionDraftState::Authorized
        );
    }

    #[test]
    fn cancellation_is_refused_once_the_broadcast_boundary_is_crossed() {
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let adapter = submittable_adapter(Arc::new(BroadcastBlockingCompleter {
            started: started_sender,
            release: Arc::clone(&release),
        }));
        let authorized = authorize_transfer(&adapter);
        let profile = profile();
        let mut future = adapter.submit(
            &profile,
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        );
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("completion crosses the broadcast boundary");
        let status = adapter
            .submission_status(&profile, authorized.draft_id())
            .expect("broadcasting status is readable");
        assert_eq!(
            status.state(),
            WalletTransactionSubmissionState::Broadcasting
        );
        assert!(!status.cancellation_allowed());
        assert_eq!(
            adapter.cancel_submission(&profile, authorized.draft_id()),
            Err(WalletTransactionPortError::SubmissionCancellationUnsafe)
        );

        let (lock, condition) = release.as_ref();
        *lock.lock().expect("release lock is available") = true;
        condition.notify_one();
        let submitted = futures::executor::block_on(future).expect("submission completes");
        assert_eq!(
            adapter
                .submission_status(&profile, authorized.draft_id())
                .expect("included status remains readable")
                .state(),
            WalletTransactionSubmissionState::Included
        );
        assert_eq!(
            submitted.submission.transaction_id().as_str(),
            hex::encode([1; 32])
        );
    }

    #[test]
    fn custody_errors_preserve_actionable_transaction_state() {
        assert_eq!(
            map_security_error(WalletSecurityPortError::NotInitialized),
            WalletTransactionPortError::ProtectionNotInitialized
        );
        assert_eq!(
            map_security_error(WalletSecurityPortError::Locked),
            WalletTransactionPortError::ProtectionLocked
        );
        assert_eq!(
            map_security_error(WalletSecurityPortError::AuthorizationDenied),
            WalletTransactionPortError::InvalidData
        );
    }
}
