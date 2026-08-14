// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use oxid_foundation::{OpaqueId, OpaqueIdError, UnixTimestampMillis};
use oxid_passport_vault_domain::PassportVaultPolicy;
use oxid_platform_ports::{ClockPort, PlatformError, RandomPort};

use super::{
    PassportVaultContractStateAuthentication, PassportVaultContractStateSnapshot,
    PassportVaultContractStateSourceError, PassportVaultContractStateSourcePort, normalize_hex_32,
    validate_snapshot,
};

/// Retained vault calls expire before stale state can be authorized indefinitely.
pub const PASSPORT_VAULT_CALL_DRAFT_TTL_MILLIS: u64 = 60 * 60 * 1_000;
pub const MAX_PASSPORT_VAULT_CALL_SUBMISSION_HISTORY: usize = 128;
pub const AUTHORIZE_PASSPORT_VAULT_CALL_INTENT: &str = "AUTHORIZE_PASSPORT_VAULT_CALL";
pub const SUBMIT_PASSPORT_VAULT_CALL_INTENT: &str = "SUBMIT_PASSPORT_VAULT_CALL";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PassportVaultCallDraftId(OpaqueId);

impl PassportVaultCallDraftId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PassportVaultCallAuthorizationChallenge(OpaqueId);

impl PassportVaultCallAuthorizationChallenge {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Closed set of state-changing Passport Vault circuits supported by Oxid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultCallKind {
    CreateLock,
    DepositToLock,
    ClaimFromLock,
    WithdrawFromLock,
}

impl PassportVaultCallKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CreateLock => "create_lock",
            Self::DepositToLock => "deposit_to_lock",
            Self::ClaimFromLock => "claim_from_lock",
            Self::WithdrawFromLock => "withdraw_from_lock",
        }
    }
}

/// Adapter-neutral call intent. Claim material is referenced by opaque
/// profile/credential identifiers; private values, openings, holder keys,
/// signatures, and proof bytes never cross this application boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassportVaultCallOperation {
    CreateLock {
        policy: PassportVaultPolicy,
        initial_amount: u128,
    },
    DepositToLock {
        lock_id: u64,
        amount: u128,
    },
    ClaimFromLock {
        lock_id: u64,
        amount: u128,
        credential_id: OpaqueId,
    },
    WithdrawFromLock {
        lock_id: u64,
        amount: u128,
    },
}

impl PassportVaultCallOperation {
    #[must_use]
    pub const fn kind(&self) -> PassportVaultCallKind {
        match self {
            Self::CreateLock { .. } => PassportVaultCallKind::CreateLock,
            Self::DepositToLock { .. } => PassportVaultCallKind::DepositToLock,
            Self::ClaimFromLock { .. } => PassportVaultCallKind::ClaimFromLock,
            Self::WithdrawFromLock { .. } => PassportVaultCallKind::WithdrawFromLock,
        }
    }

    #[must_use]
    pub const fn lock_id(&self) -> Option<u64> {
        match self {
            Self::CreateLock { .. } => None,
            Self::DepositToLock { lock_id, .. }
            | Self::ClaimFromLock { lock_id, .. }
            | Self::WithdrawFromLock { lock_id, .. } => Some(*lock_id),
        }
    }

    #[must_use]
    pub const fn amount(&self) -> u128 {
        match self {
            Self::CreateLock { initial_amount, .. } => *initial_amount,
            Self::DepositToLock { amount, .. }
            | Self::ClaimFromLock { amount, .. }
            | Self::WithdrawFromLock { amount, .. } => *amount,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultCallDraftState {
    Prepared,
    Authorized,
    Submitting,
    Submitted,
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultCallSubmissionState {
    NotStarted,
    Running,
    CancellationRequested,
    Broadcasting,
    Cancelled,
    Included,
    Rejected,
    Expired,
    OutcomeUnknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultCallPortError {
    Unavailable,
    ProtectionNotInitialized,
    ProtectionLocked,
    AccountNotDerived,
    AccountNotSynchronized,
    UnsupportedNetwork,
    DraftNotFound,
    DraftExpired,
    DraftConflict,
    AuthorizationChallengeMismatch,
    SubmissionInProgress,
    SubmissionNotInProgress,
    SubmissionCancelled,
    SubmissionCancellationUnsafe,
    InsufficientFunds,
    InsufficientDust,
    InvalidChainState,
    ProvingFailed,
    SubmissionRejected,
    SubmissionOutcomeUnknown,
    Timeout,
    InvalidData,
}

impl fmt::Display for PassportVaultCallPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Passport Vault contract calls are unavailable",
            Self::ProtectionNotInitialized => "wallet protection is not initialized",
            Self::ProtectionLocked => "wallet is locked",
            Self::AccountNotDerived => "a protected wallet account must be derived first",
            Self::AccountNotSynchronized => "wallet account must be synchronized first",
            Self::UnsupportedNetwork => "wallet network is not supported",
            Self::DraftNotFound => "Passport Vault call draft was not found",
            Self::DraftExpired => "Passport Vault call draft has expired",
            Self::DraftConflict => "Passport Vault call draft conflicts with current state",
            Self::AuthorizationChallengeMismatch => {
                "Passport Vault authorization does not match the prepared call"
            }
            Self::SubmissionInProgress => "Passport Vault submission is already in progress",
            Self::SubmissionNotInProgress => "Passport Vault submission is not in progress",
            Self::SubmissionCancelled => "Passport Vault submission was cancelled before broadcast",
            Self::SubmissionCancellationUnsafe => {
                "Passport Vault submission can no longer be cancelled safely"
            }
            Self::InsufficientFunds => "wallet has insufficient NIGHT for the vault call",
            Self::InsufficientDust => "wallet has insufficient DUST for the vault call fee",
            Self::InvalidChainState => "Midnight chain state is invalid or unavailable",
            Self::ProvingFailed => "Passport Vault transaction proving failed",
            Self::SubmissionRejected => "Midnight rejected the Passport Vault transaction",
            Self::SubmissionOutcomeUnknown => {
                "Midnight Passport Vault submission outcome is not yet known"
            }
            Self::Timeout => "Passport Vault transaction operation timed out",
            Self::InvalidData => "Passport Vault adapter returned invalid data",
        })
    }
}

impl Error for PassportVaultCallPortError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparePassportVaultCallRequest {
    pub profile_id: OpaqueId,
    pub contract_state: PassportVaultContractStateSnapshot,
    pub operation: PassportVaultCallOperation,
    pub expires_at: UnixTimestampMillis,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizePassportVaultCallRequest {
    pub draft_id: PassportVaultCallDraftId,
    pub authorization_challenge: PassportVaultCallAuthorizationChallenge,
    pub now: UnixTimestampMillis,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitPassportVaultCallRequest {
    pub draft_id: PassportVaultCallDraftId,
    pub now: UnixTimestampMillis,
}

/// Safe public draft metadata. Serialized transactions, signatures, proofs,
/// witnesses, credentials, and key material remain adapter-owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultCallPreview {
    pub draft_id: PassportVaultCallDraftId,
    pub authorization_challenge: PassportVaultCallAuthorizationChallenge,
    pub contract_address_hex: String,
    pub operation: PassportVaultCallOperation,
    pub state_anchor_transaction_hash_hex: String,
    pub state_anchor_block_hash_hex: String,
    pub state_anchor_block_height: u64,
    pub expires_at: UnixTimestampMillis,
    pub state: PassportVaultCallDraftState,
    pub fee_atomic_units: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultCallInclusion {
    pub transaction_hash_hex: String,
    pub block_hash_hex: String,
    pub block_height: u64,
    pub fee_atomic_units: u128,
    pub mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmittedPassportVaultCall {
    pub preview: PassportVaultCallPreview,
    pub inclusion: PassportVaultCallInclusion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultCallSubmissionStatus {
    pub draft_id: PassportVaultCallDraftId,
    pub state: PassportVaultCallSubmissionState,
    pub transaction_hash_hex: Option<String>,
    pub block_hash_hex: Option<String>,
    pub block_height: Option<u64>,
    pub fee_atomic_units: Option<u128>,
    pub mode: Option<String>,
}

impl PassportVaultCallSubmissionStatus {
    #[must_use]
    pub const fn cancellation_allowed(&self) -> bool {
        matches!(self.state, PassportVaultCallSubmissionState::Running)
    }

    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self.state,
            PassportVaultCallSubmissionState::NotStarted
                | PassportVaultCallSubmissionState::Cancelled
        )
    }

    #[must_use]
    pub const fn replacement_allowed(&self) -> bool {
        matches!(
            self.state,
            PassportVaultCallSubmissionState::Rejected | PassportVaultCallSubmissionState::Expired
        )
    }

    #[must_use]
    pub const fn reconciliation_allowed(&self) -> bool {
        matches!(
            self.state,
            PassportVaultCallSubmissionState::Broadcasting
                | PassportVaultCallSubmissionState::OutcomeUnknown
        )
    }
}

pub type PassportVaultCallSubmissionFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<SubmittedPassportVaultCall, PassportVaultCallPortError>>
            + Send
            + 'a,
    >,
>;
pub type PassportVaultCallStatusFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError>>
            + Send
            + 'a,
    >,
>;

/// Capability-specific outgoing port for the four mutating Compact circuits.
pub trait PassportVaultContractCallPort: Send + Sync {
    fn prepare(
        &self,
        request: PreparePassportVaultCallRequest,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError>;

    fn authorize(
        &self,
        profile_id: &OpaqueId,
        request: AuthorizePassportVaultCallRequest,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError>;

    fn submit<'a>(
        &'a self,
        profile_id: &'a OpaqueId,
        request: SubmitPassportVaultCallRequest,
    ) -> PassportVaultCallSubmissionFuture<'a>;

    fn get(
        &self,
        profile_id: &OpaqueId,
        draft_id: &PassportVaultCallDraftId,
        now: UnixTimestampMillis,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError>;

    fn submission_status(
        &self,
        profile_id: &OpaqueId,
        draft_id: &PassportVaultCallDraftId,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError>;

    fn cancel_submission(
        &self,
        profile_id: &OpaqueId,
        draft_id: &PassportVaultCallDraftId,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError>;

    fn submission_history(
        &self,
        profile_id: &OpaqueId,
    ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError>;

    fn reconcile_submission<'a>(
        &'a self,
        profile_id: &'a OpaqueId,
        draft_id: &'a PassportVaultCallDraftId,
    ) -> PassportVaultCallStatusFuture<'a>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreparePassportVaultCallAction {
    CreateLock {
        minimum_age_years: u8,
        required_issuing_state: Option<[u8; 32]>,
        required_document_number: Option<[u8; 32]>,
        maximum_claim_amount: String,
        initial_amount: String,
    },
    DepositToLock {
        lock_id: u64,
        amount: String,
    },
    ClaimFromLock {
        lock_id: u64,
        amount: String,
        credential_id: String,
    },
    WithdrawFromLock {
        lock_id: u64,
        amount: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparePassportVaultCallCommand {
    pub profile_id: String,
    pub contract_address_hex: String,
    pub action: PreparePassportVaultCallAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizePassportVaultCallCommand {
    pub profile_id: String,
    pub draft_id: String,
    pub authorization_challenge: String,
    pub confirmed: bool,
    pub intent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitPassportVaultCallCommand {
    pub profile_id: String,
    pub draft_id: String,
    pub confirmed: bool,
    pub intent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultCallQuery {
    pub profile_id: String,
    pub draft_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultCallPreviewView {
    pub draft_id: String,
    pub authorization_challenge: String,
    pub contract_address_hex: String,
    pub operation: String,
    pub lock_id: Option<u64>,
    pub amount_atomic_units: String,
    pub state_anchor_transaction_hash_hex: String,
    pub state_anchor_block_hash_hex: String,
    pub state_anchor_block_height: u64,
    pub expires_at_millis: u64,
    pub state: String,
    pub fee_atomic_units: Option<String>,
    pub proof_required: bool,
    pub submission_ready: bool,
}

impl From<&PassportVaultCallPreview> for PassportVaultCallPreviewView {
    fn from(value: &PassportVaultCallPreview) -> Self {
        Self {
            draft_id: value.draft_id.as_str().to_owned(),
            authorization_challenge: value.authorization_challenge.as_str().to_owned(),
            contract_address_hex: value.contract_address_hex.clone(),
            operation: value.operation.kind().name().to_owned(),
            lock_id: value.operation.lock_id(),
            amount_atomic_units: value.operation.amount().to_string(),
            state_anchor_transaction_hash_hex: value.state_anchor_transaction_hash_hex.clone(),
            state_anchor_block_hash_hex: value.state_anchor_block_hash_hex.clone(),
            state_anchor_block_height: value.state_anchor_block_height,
            expires_at_millis: value.expires_at.value(),
            state: draft_state_name(value.state).to_owned(),
            fee_atomic_units: value.fee_atomic_units.map(|fee| fee.to_string()),
            proof_required: !matches!(value.state, PassportVaultCallDraftState::Submitted),
            submission_ready: matches!(value.state, PassportVaultCallDraftState::Authorized),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultCallSubmissionView {
    pub call: PassportVaultCallPreviewView,
    pub transaction_hash_hex: String,
    pub block_hash_hex: String,
    pub block_height: u64,
    pub fee_atomic_units: String,
    pub mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultCallSubmissionStatusView {
    pub draft_id: String,
    pub state: String,
    pub cancellation_allowed: bool,
    pub retryable: bool,
    pub replacement_allowed: bool,
    pub reconciliation_allowed: bool,
    pub transaction_hash_hex: Option<String>,
    pub block_hash_hex: Option<String>,
    pub block_height: Option<u64>,
    pub fee_atomic_units: Option<String>,
    pub mode: Option<String>,
}

impl From<&PassportVaultCallSubmissionStatus> for PassportVaultCallSubmissionStatusView {
    fn from(value: &PassportVaultCallSubmissionStatus) -> Self {
        Self {
            draft_id: value.draft_id.as_str().to_owned(),
            state: submission_state_name(value.state).to_owned(),
            cancellation_allowed: value.cancellation_allowed(),
            retryable: value.retryable(),
            replacement_allowed: value.replacement_allowed(),
            reconciliation_allowed: value.reconciliation_allowed(),
            transaction_hash_hex: value.transaction_hash_hex.clone(),
            block_hash_hex: value.block_hash_hex.clone(),
            block_height: value.block_height,
            fee_atomic_units: value.fee_atomic_units.map(|fee| fee.to_string()),
            mode: value.mode.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassportVaultCallError {
    InvalidIdentifier(OpaqueIdError),
    InvalidAddress,
    InvalidAmount,
    ZeroAmount,
    InvalidPolicy,
    ConfirmationRequired,
    InvalidConfirmation,
    UnauthenticatedState,
    Clock(PlatformError),
    Random(PlatformError),
    State(PassportVaultContractStateSourceError),
    Operation(PassportVaultCallPortError),
}

impl fmt::Display for PassportVaultCallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier(error) => error.fmt(formatter),
            Self::InvalidAddress => {
                formatter.write_str("Passport Vault contract address is invalid")
            }
            Self::InvalidAmount => {
                formatter.write_str("Passport Vault amount must be an unsigned integer")
            }
            Self::ZeroAmount => {
                formatter.write_str("Passport Vault amount must be greater than zero")
            }
            Self::InvalidPolicy => formatter.write_str("Passport Vault policy is invalid"),
            Self::ConfirmationRequired => formatter.write_str("explicit confirmation is required"),
            Self::InvalidConfirmation => formatter.write_str("confirmation intent is invalid"),
            Self::UnauthenticatedState => formatter.write_str(
                "Passport Vault state does not satisfy the configured authentication mode",
            ),
            Self::Clock(error) | Self::Random(error) => error.fmt(formatter),
            Self::State(error) => error.fmt(formatter),
            Self::Operation(error) => error.fmt(formatter),
        }
    }
}

impl Error for PassportVaultCallError {}

pub type PreparePassportVaultCallFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PassportVaultCallPreviewView, PassportVaultCallError>>
            + Send
            + 'a,
    >,
>;
pub type SubmitPassportVaultCallFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PassportVaultCallSubmissionView, PassportVaultCallError>>
            + Send
            + 'a,
    >,
>;
pub type ReconcilePassportVaultCallFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PassportVaultCallSubmissionStatusView, PassportVaultCallError>>
            + Send
            + 'a,
    >,
>;

pub trait PreparePassportVaultCallUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: PreparePassportVaultCallCommand,
    ) -> PreparePassportVaultCallFuture<'a>;
}
pub trait AuthorizePassportVaultCallUseCase: Send + Sync {
    fn execute(
        &self,
        command: AuthorizePassportVaultCallCommand,
    ) -> Result<PassportVaultCallPreviewView, PassportVaultCallError>;
}
pub trait SubmitPassportVaultCallUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: SubmitPassportVaultCallCommand,
    ) -> SubmitPassportVaultCallFuture<'a>;
}
pub trait GetPassportVaultCallUseCase: Send + Sync {
    fn execute(
        &self,
        query: PassportVaultCallQuery,
    ) -> Result<PassportVaultCallPreviewView, PassportVaultCallError>;
}
pub trait GetPassportVaultCallSubmissionStatusUseCase: Send + Sync {
    fn execute(
        &self,
        query: PassportVaultCallQuery,
    ) -> Result<PassportVaultCallSubmissionStatusView, PassportVaultCallError>;
}
pub trait CancelPassportVaultCallSubmissionUseCase: Send + Sync {
    fn execute(
        &self,
        command: PassportVaultCallQuery,
    ) -> Result<PassportVaultCallSubmissionStatusView, PassportVaultCallError>;
}
pub trait ListPassportVaultCallSubmissionsUseCase: Send + Sync {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<Vec<PassportVaultCallSubmissionStatusView>, PassportVaultCallError>;
}
pub trait ReconcilePassportVaultCallSubmissionUseCase: Send + Sync {
    fn execute<'a>(&'a self, query: PassportVaultCallQuery)
    -> ReconcilePassportVaultCallFuture<'a>;
}

pub struct PassportVaultContractCallService {
    state: Arc<dyn PassportVaultContractStateSourcePort>,
    calls: Arc<dyn PassportVaultContractCallPort>,
    clock: Arc<dyn ClockPort>,
    random: Arc<dyn RandomPort>,
    required_authentication: PassportVaultContractStateAuthentication,
}

impl PassportVaultContractCallService {
    #[must_use]
    pub fn new(
        state: Arc<dyn PassportVaultContractStateSourcePort>,
        calls: Arc<dyn PassportVaultContractCallPort>,
        clock: Arc<dyn ClockPort>,
        random: Arc<dyn RandomPort>,
    ) -> Self {
        Self {
            state,
            calls,
            clock,
            random,
            required_authentication:
                PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
        }
    }

    /// Constructs the explicitly development-only call harness. Keeping its
    /// authentication label distinct prevents fixture state from being
    /// accepted by the live finalized-node composition.
    #[must_use]
    pub fn new_simulated(
        state: Arc<dyn PassportVaultContractStateSourcePort>,
        calls: Arc<dyn PassportVaultContractCallPort>,
        clock: Arc<dyn ClockPort>,
        random: Arc<dyn RandomPort>,
    ) -> Self {
        Self {
            state,
            calls,
            clock,
            random,
            required_authentication:
                PassportVaultContractStateAuthentication::DeterministicSimulation,
        }
    }

    fn now(&self) -> Result<UnixTimestampMillis, PassportVaultCallError> {
        self.clock.now().map_err(PassportVaultCallError::Clock)
    }
}

impl PreparePassportVaultCallUseCase for PassportVaultContractCallService {
    fn execute<'a>(
        &'a self,
        command: PreparePassportVaultCallCommand,
    ) -> PreparePassportVaultCallFuture<'a> {
        Box::pin(async move {
            let profile_id = OpaqueId::parse(command.profile_id)
                .map_err(PassportVaultCallError::InvalidIdentifier)?;
            let contract_address_hex = normalize_hex_32(&command.contract_address_hex)
                .ok_or(PassportVaultCallError::InvalidAddress)?;
            let operation = operation_from_action(&*self.random, command.action)?;
            let now = self.now()?;
            let expires_at = now
                .value()
                .checked_add(PASSPORT_VAULT_CALL_DRAFT_TTL_MILLIS)
                .map(UnixTimestampMillis::new)
                .ok_or(PassportVaultCallError::Clock(
                    PlatformError::ClockUnavailable,
                ))?;
            let snapshot = self
                .state
                .read(&contract_address_hex)
                .await
                .map_err(PassportVaultCallError::State)?;
            validate_snapshot(&snapshot, &contract_address_hex)
                .map_err(PassportVaultCallError::State)?;
            if snapshot.authentication != self.required_authentication {
                return Err(PassportVaultCallError::UnauthenticatedState);
            }
            let expected_contract_address_hex = snapshot.contract_address_hex.clone();
            let expected_transaction_hash_hex = snapshot.transaction_hash_hex.clone();
            let expected_block_hash_hex = snapshot.action_block_hash_hex.clone();
            let expected_block_height = snapshot.action_block_height;
            let expected_operation = operation.clone();
            let preview = self
                .calls
                .prepare(PreparePassportVaultCallRequest {
                    profile_id,
                    contract_state: snapshot,
                    operation,
                    expires_at,
                })
                .map_err(PassportVaultCallError::Operation)?;
            validate_prepared_preview(
                &preview,
                &expected_contract_address_hex,
                &expected_transaction_hash_hex,
                &expected_block_hash_hex,
                expected_block_height,
                &expected_operation,
                expires_at,
            )?;
            Ok(PassportVaultCallPreviewView::from(&preview))
        })
    }
}

impl AuthorizePassportVaultCallUseCase for PassportVaultContractCallService {
    fn execute(
        &self,
        command: AuthorizePassportVaultCallCommand,
    ) -> Result<PassportVaultCallPreviewView, PassportVaultCallError> {
        require_confirmation(
            command.confirmed,
            &command.intent,
            AUTHORIZE_PASSPORT_VAULT_CALL_INTENT,
        )?;
        let profile_id = parse_identifier(command.profile_id)?;
        let draft_id = PassportVaultCallDraftId::parse(command.draft_id)
            .map_err(PassportVaultCallError::InvalidIdentifier)?;
        let authorization_challenge =
            PassportVaultCallAuthorizationChallenge::parse(command.authorization_challenge)
                .map_err(PassportVaultCallError::InvalidIdentifier)?;
        let now = self.now()?;
        let before = self
            .calls
            .get(&profile_id, &draft_id, now)
            .map_err(PassportVaultCallError::Operation)?;
        if now.value() >= before.expires_at.value() {
            return Err(PassportVaultCallError::Operation(
                PassportVaultCallPortError::DraftExpired,
            ));
        }
        if before.draft_id != draft_id || before.state != PassportVaultCallDraftState::Prepared {
            return Err(PassportVaultCallError::Operation(
                PassportVaultCallPortError::InvalidData,
            ));
        }
        if before.authorization_challenge != authorization_challenge {
            return Err(PassportVaultCallError::Operation(
                PassportVaultCallPortError::AuthorizationChallengeMismatch,
            ));
        }
        let preview = self
            .calls
            .authorize(
                &profile_id,
                AuthorizePassportVaultCallRequest {
                    draft_id,
                    authorization_challenge,
                    now,
                },
            )
            .map_err(PassportVaultCallError::Operation)?;
        validate_transition(&before, &preview, PassportVaultCallDraftState::Authorized)?;
        Ok(PassportVaultCallPreviewView::from(&preview))
    }
}

impl SubmitPassportVaultCallUseCase for PassportVaultContractCallService {
    fn execute<'a>(
        &'a self,
        command: SubmitPassportVaultCallCommand,
    ) -> SubmitPassportVaultCallFuture<'a> {
        Box::pin(async move {
            require_confirmation(
                command.confirmed,
                &command.intent,
                SUBMIT_PASSPORT_VAULT_CALL_INTENT,
            )?;
            let profile_id = parse_identifier(command.profile_id)?;
            let draft_id = PassportVaultCallDraftId::parse(command.draft_id)
                .map_err(PassportVaultCallError::InvalidIdentifier)?;
            let now = self.now()?;
            let before = self
                .calls
                .get(&profile_id, &draft_id, now)
                .map_err(PassportVaultCallError::Operation)?;
            if now.value() >= before.expires_at.value() {
                return Err(PassportVaultCallError::Operation(
                    PassportVaultCallPortError::DraftExpired,
                ));
            }
            if before.draft_id != draft_id
                || before.state != PassportVaultCallDraftState::Authorized
            {
                return Err(invalid_adapter_data());
            }
            let submitted = self
                .calls
                .submit(
                    &profile_id,
                    SubmitPassportVaultCallRequest { draft_id, now },
                )
                .await
                .map_err(PassportVaultCallError::Operation)?;
            validate_transition(
                &before,
                &submitted.preview,
                PassportVaultCallDraftState::Submitted,
            )?;
            validate_submitted(&submitted)?;
            Ok(PassportVaultCallSubmissionView {
                call: PassportVaultCallPreviewView::from(&submitted.preview),
                transaction_hash_hex: submitted.inclusion.transaction_hash_hex,
                block_hash_hex: submitted.inclusion.block_hash_hex,
                block_height: submitted.inclusion.block_height,
                fee_atomic_units: submitted.inclusion.fee_atomic_units.to_string(),
                mode: submitted.inclusion.mode,
            })
        })
    }
}

impl GetPassportVaultCallUseCase for PassportVaultContractCallService {
    fn execute(
        &self,
        query: PassportVaultCallQuery,
    ) -> Result<PassportVaultCallPreviewView, PassportVaultCallError> {
        let (profile_id, draft_id) = parse_query(query)?;
        let preview = self
            .calls
            .get(&profile_id, &draft_id, self.now()?)
            .map_err(PassportVaultCallError::Operation)?;
        if preview.draft_id != draft_id {
            return Err(invalid_adapter_data());
        }
        validate_preview_identifiers(&preview)?;
        Ok(PassportVaultCallPreviewView::from(&preview))
    }
}

impl GetPassportVaultCallSubmissionStatusUseCase for PassportVaultContractCallService {
    fn execute(
        &self,
        query: PassportVaultCallQuery,
    ) -> Result<PassportVaultCallSubmissionStatusView, PassportVaultCallError> {
        let (profile_id, draft_id) = parse_query(query)?;
        let status = self
            .calls
            .submission_status(&profile_id, &draft_id)
            .map_err(PassportVaultCallError::Operation)?;
        validate_status(&status, &draft_id)?;
        Ok(PassportVaultCallSubmissionStatusView::from(&status))
    }
}

impl CancelPassportVaultCallSubmissionUseCase for PassportVaultContractCallService {
    fn execute(
        &self,
        command: PassportVaultCallQuery,
    ) -> Result<PassportVaultCallSubmissionStatusView, PassportVaultCallError> {
        let (profile_id, draft_id) = parse_query(command)?;
        let status = self
            .calls
            .cancel_submission(&profile_id, &draft_id)
            .map_err(PassportVaultCallError::Operation)?;
        validate_status(&status, &draft_id)?;
        Ok(PassportVaultCallSubmissionStatusView::from(&status))
    }
}

impl ListPassportVaultCallSubmissionsUseCase for PassportVaultContractCallService {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<Vec<PassportVaultCallSubmissionStatusView>, PassportVaultCallError> {
        let profile_id = parse_identifier(profile_id)?;
        let history = self
            .calls
            .submission_history(&profile_id)
            .map_err(PassportVaultCallError::Operation)?;
        if history.len() > MAX_PASSPORT_VAULT_CALL_SUBMISSION_HISTORY {
            return Err(invalid_adapter_data());
        }
        let mut draft_ids = std::collections::BTreeSet::new();
        history
            .iter()
            .map(|status| {
                if !draft_ids.insert(status.draft_id.clone()) {
                    return Err(invalid_adapter_data());
                }
                validate_status(status, &status.draft_id)?;
                Ok(PassportVaultCallSubmissionStatusView::from(status))
            })
            .collect()
    }
}

impl ReconcilePassportVaultCallSubmissionUseCase for PassportVaultContractCallService {
    fn execute<'a>(
        &'a self,
        query: PassportVaultCallQuery,
    ) -> ReconcilePassportVaultCallFuture<'a> {
        Box::pin(async move {
            let (profile_id, draft_id) = parse_query(query)?;
            let status = self
                .calls
                .reconcile_submission(&profile_id, &draft_id)
                .await
                .map_err(PassportVaultCallError::Operation)?;
            validate_status(&status, &draft_id)?;
            Ok(PassportVaultCallSubmissionStatusView::from(&status))
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailablePassportVaultContractCall;

impl PassportVaultContractCallPort for UnavailablePassportVaultContractCall {
    fn prepare(
        &self,
        _: PreparePassportVaultCallRequest,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }

    fn authorize(
        &self,
        _: &OpaqueId,
        _: AuthorizePassportVaultCallRequest,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }

    fn submit<'a>(
        &'a self,
        _: &'a OpaqueId,
        _: SubmitPassportVaultCallRequest,
    ) -> PassportVaultCallSubmissionFuture<'a> {
        Box::pin(async { Err(PassportVaultCallPortError::Unavailable) })
    }

    fn get(
        &self,
        _: &OpaqueId,
        _: &PassportVaultCallDraftId,
        _: UnixTimestampMillis,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }

    fn submission_status(
        &self,
        _: &OpaqueId,
        _: &PassportVaultCallDraftId,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }

    fn cancel_submission(
        &self,
        _: &OpaqueId,
        _: &PassportVaultCallDraftId,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }

    fn submission_history(
        &self,
        _: &OpaqueId,
    ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }

    fn reconcile_submission<'a>(
        &'a self,
        _: &'a OpaqueId,
        _: &'a PassportVaultCallDraftId,
    ) -> PassportVaultCallStatusFuture<'a> {
        Box::pin(async { Err(PassportVaultCallPortError::Unavailable) })
    }
}

fn operation_from_action(
    random: &dyn RandomPort,
    action: PreparePassportVaultCallAction,
) -> Result<PassportVaultCallOperation, PassportVaultCallError> {
    Ok(match action {
        PreparePassportVaultCallAction::CreateLock {
            minimum_age_years,
            required_issuing_state,
            required_document_number,
            maximum_claim_amount,
            initial_amount,
        } => {
            let maximum_claim_amount = parse_positive_amount(&maximum_claim_amount)?;
            let initial_amount = parse_amount(&initial_amount)?;
            let mut verifier_challenge_hash = [0_u8; 32];
            random
                .fill_bytes(&mut verifier_challenge_hash)
                .map_err(PassportVaultCallError::Random)?;
            if verifier_challenge_hash == [0; 32] {
                verifier_challenge_hash[0] = 1;
            }
            let policy = PassportVaultPolicy::new(
                minimum_age_years,
                required_issuing_state,
                required_document_number,
                maximum_claim_amount,
                verifier_challenge_hash,
            )
            .map_err(|_| PassportVaultCallError::InvalidPolicy)?;
            PassportVaultCallOperation::CreateLock {
                policy,
                initial_amount,
            }
        }
        PreparePassportVaultCallAction::DepositToLock { lock_id, amount } => {
            PassportVaultCallOperation::DepositToLock {
                lock_id,
                amount: parse_positive_amount(&amount)?,
            }
        }
        PreparePassportVaultCallAction::ClaimFromLock {
            lock_id,
            amount,
            credential_id,
        } => PassportVaultCallOperation::ClaimFromLock {
            lock_id,
            amount: parse_positive_amount(&amount)?,
            credential_id: parse_identifier(credential_id)?,
        },
        PreparePassportVaultCallAction::WithdrawFromLock { lock_id, amount } => {
            PassportVaultCallOperation::WithdrawFromLock {
                lock_id,
                amount: parse_positive_amount(&amount)?,
            }
        }
    })
}

fn parse_amount(value: &str) -> Result<u128, PassportVaultCallError> {
    if value.is_empty()
        || value.len() > 39
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(PassportVaultCallError::InvalidAmount);
    }
    value
        .parse::<u128>()
        .map_err(|_| PassportVaultCallError::InvalidAmount)
}

fn parse_positive_amount(value: &str) -> Result<u128, PassportVaultCallError> {
    let amount = parse_amount(value)?;
    if amount == 0 {
        return Err(PassportVaultCallError::ZeroAmount);
    }
    Ok(amount)
}

fn parse_identifier(value: String) -> Result<OpaqueId, PassportVaultCallError> {
    OpaqueId::parse(value).map_err(PassportVaultCallError::InvalidIdentifier)
}

fn parse_query(
    query: PassportVaultCallQuery,
) -> Result<(OpaqueId, PassportVaultCallDraftId), PassportVaultCallError> {
    Ok((
        parse_identifier(query.profile_id)?,
        PassportVaultCallDraftId::parse(query.draft_id)
            .map_err(PassportVaultCallError::InvalidIdentifier)?,
    ))
}

fn require_confirmation(
    confirmed: bool,
    actual: &str,
    expected: &str,
) -> Result<(), PassportVaultCallError> {
    if !confirmed {
        return Err(PassportVaultCallError::ConfirmationRequired);
    }
    if actual != expected {
        return Err(PassportVaultCallError::InvalidConfirmation);
    }
    Ok(())
}

fn validate_prepared_preview(
    preview: &PassportVaultCallPreview,
    expected_contract_address_hex: &str,
    expected_transaction_hash_hex: &str,
    expected_block_hash_hex: &str,
    expected_block_height: u64,
    operation: &PassportVaultCallOperation,
    expires_at: UnixTimestampMillis,
) -> Result<(), PassportVaultCallError> {
    if preview.contract_address_hex != expected_contract_address_hex
        || preview.operation != *operation
        || preview.state_anchor_transaction_hash_hex != expected_transaction_hash_hex
        || preview.state_anchor_block_hash_hex != expected_block_hash_hex
        || preview.state_anchor_block_height != expected_block_height
        || preview.expires_at != expires_at
        || preview.state != PassportVaultCallDraftState::Prepared
    {
        return Err(invalid_adapter_data());
    }
    validate_preview_identifiers(preview)
}

fn validate_transition(
    before: &PassportVaultCallPreview,
    after: &PassportVaultCallPreview,
    expected_state: PassportVaultCallDraftState,
) -> Result<(), PassportVaultCallError> {
    if before.draft_id != after.draft_id
        || before.authorization_challenge != after.authorization_challenge
        || before.contract_address_hex != after.contract_address_hex
        || before.operation != after.operation
        || before.state_anchor_transaction_hash_hex != after.state_anchor_transaction_hash_hex
        || before.state_anchor_block_hash_hex != after.state_anchor_block_hash_hex
        || before.state_anchor_block_height != after.state_anchor_block_height
        || before.expires_at != after.expires_at
        || after.state != expected_state
    {
        return Err(invalid_adapter_data());
    }
    validate_preview_identifiers(after)
}

fn validate_preview_identifiers(
    preview: &PassportVaultCallPreview,
) -> Result<(), PassportVaultCallError> {
    if normalize_hex_32(&preview.contract_address_hex).as_deref()
        != Some(preview.contract_address_hex.as_str())
        || normalize_hex_32(&preview.state_anchor_transaction_hash_hex).as_deref()
            != Some(preview.state_anchor_transaction_hash_hex.as_str())
        || normalize_hex_32(&preview.state_anchor_block_hash_hex).as_deref()
            != Some(preview.state_anchor_block_hash_hex.as_str())
    {
        return Err(invalid_adapter_data());
    }
    Ok(())
}

fn validate_submitted(
    submitted: &SubmittedPassportVaultCall,
) -> Result<(), PassportVaultCallError> {
    validate_preview_identifiers(&submitted.preview)?;
    if submitted.preview.state != PassportVaultCallDraftState::Submitted
        || normalize_hex_32(&submitted.inclusion.transaction_hash_hex).as_deref()
            != Some(submitted.inclusion.transaction_hash_hex.as_str())
        || normalize_hex_32(&submitted.inclusion.block_hash_hex).as_deref()
            != Some(submitted.inclusion.block_hash_hex.as_str())
        || submitted.inclusion.mode.is_empty()
        || submitted.inclusion.mode.chars().count() > 64
        || submitted.inclusion.mode.chars().any(char::is_control)
    {
        return Err(invalid_adapter_data());
    }
    Ok(())
}

fn validate_status(
    status: &PassportVaultCallSubmissionStatus,
    expected_draft_id: &PassportVaultCallDraftId,
) -> Result<(), PassportVaultCallError> {
    let included = matches!(status.state, PassportVaultCallSubmissionState::Included);
    let block_fields_complete = status.block_hash_hex.is_some() == status.block_height.is_some();
    if &status.draft_id != expected_draft_id
        || status
            .transaction_hash_hex
            .as_deref()
            .is_some_and(|value| normalize_hex_32(value).as_deref() != Some(value))
        || status
            .block_hash_hex
            .as_deref()
            .is_some_and(|value| normalize_hex_32(value).as_deref() != Some(value))
        || status.mode.as_deref().is_some_and(|mode| {
            mode.is_empty()
                || mode.trim() != mode
                || mode.chars().count() > 64
                || mode.chars().any(char::is_control)
        })
        || !block_fields_complete
        || (included
            && (status.transaction_hash_hex.is_none()
                || status.block_hash_hex.is_none()
                || status.fee_atomic_units.is_none()
                || status.mode.is_none()))
    {
        return Err(invalid_adapter_data());
    }
    Ok(())
}

const fn invalid_adapter_data() -> PassportVaultCallError {
    PassportVaultCallError::Operation(PassportVaultCallPortError::InvalidData)
}

const fn draft_state_name(state: PassportVaultCallDraftState) -> &'static str {
    match state {
        PassportVaultCallDraftState::Prepared => "prepared",
        PassportVaultCallDraftState::Authorized => "authorized",
        PassportVaultCallDraftState::Submitting => "submitting",
        PassportVaultCallDraftState::Submitted => "submitted",
        PassportVaultCallDraftState::Expired => "expired",
    }
}

const fn submission_state_name(state: PassportVaultCallSubmissionState) -> &'static str {
    match state {
        PassportVaultCallSubmissionState::NotStarted => "not_started",
        PassportVaultCallSubmissionState::Running => "running",
        PassportVaultCallSubmissionState::CancellationRequested => "cancellation_requested",
        PassportVaultCallSubmissionState::Broadcasting => "broadcasting",
        PassportVaultCallSubmissionState::Cancelled => "cancelled",
        PassportVaultCallSubmissionState::Included => "included",
        PassportVaultCallSubmissionState::Rejected => "rejected",
        PassportVaultCallSubmissionState::Expired => "expired",
        PassportVaultCallSubmissionState::OutcomeUnknown => "outcome_unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll, Waker},
    };

    use super::*;
    use crate::PassportVaultContractStateReadFuture;

    const ADDRESS: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const ANCHOR_TX: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    const ANCHOR_BLOCK: &str = "3333333333333333333333333333333333333333333333333333333333333333";
    const INCLUDED_TX: &str = "4444444444444444444444444444444444444444444444444444444444444444";
    const INCLUDED_BLOCK: &str = "5555555555555555555555555555555555555555555555555555555555555555";

    struct Clock;

    impl ClockPort for Clock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_000))
        }
    }

    struct Random;

    impl RandomPort for Random {
        fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
            destination.fill(7);
            Ok(())
        }
    }

    struct Source {
        authentication: PassportVaultContractStateAuthentication,
    }

    impl PassportVaultContractStateSourcePort for Source {
        fn read<'a>(
            &'a self,
            contract_address_hex: &'a str,
        ) -> PassportVaultContractStateReadFuture<'a> {
            let authentication = self.authentication;
            Box::pin(async move {
                Ok(PassportVaultContractStateSnapshot {
                    serialized_contract_state: vec![1, 2, 3],
                    authentication,
                    contract_address_hex: contract_address_hex.to_owned(),
                    transaction_hash_hex: ANCHOR_TX.to_owned(),
                    action_block_hash_hex: ANCHOR_BLOCK.to_owned(),
                    action_block_height: 40,
                    finalized_head_hash_hex: "66".repeat(32),
                    finalized_head_height: 42,
                })
            })
        }
    }

    #[derive(Clone)]
    struct RetainedCall {
        profile_id: OpaqueId,
        preview: PassportVaultCallPreview,
        status: PassportVaultCallSubmissionStatus,
    }

    #[derive(Default)]
    struct Calls {
        retained: Mutex<Option<RetainedCall>>,
        prepares: AtomicUsize,
    }

    impl Calls {
        fn retained(
            &self,
            profile_id: &OpaqueId,
            draft_id: &PassportVaultCallDraftId,
        ) -> Result<std::sync::MutexGuard<'_, Option<RetainedCall>>, PassportVaultCallPortError>
        {
            let guard = self
                .retained
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            if guard.as_ref().is_none_or(|retained| {
                &retained.profile_id != profile_id || &retained.preview.draft_id != draft_id
            }) {
                return Err(PassportVaultCallPortError::DraftNotFound);
            }
            Ok(guard)
        }
    }

    impl PassportVaultContractCallPort for Calls {
        fn prepare(
            &self,
            request: PreparePassportVaultCallRequest,
        ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
            self.prepares.fetch_add(1, Ordering::SeqCst);
            let draft_id = PassportVaultCallDraftId::parse("vault_draft_1")
                .map_err(|_| PassportVaultCallPortError::InvalidData)?;
            let preview = PassportVaultCallPreview {
                draft_id: draft_id.clone(),
                authorization_challenge: PassportVaultCallAuthorizationChallenge::parse(
                    "vault_authorization_1",
                )
                .map_err(|_| PassportVaultCallPortError::InvalidData)?,
                contract_address_hex: request.contract_state.contract_address_hex,
                operation: request.operation,
                state_anchor_transaction_hash_hex: request.contract_state.transaction_hash_hex,
                state_anchor_block_hash_hex: request.contract_state.action_block_hash_hex,
                state_anchor_block_height: request.contract_state.action_block_height,
                expires_at: request.expires_at,
                state: PassportVaultCallDraftState::Prepared,
                fee_atomic_units: None,
            };
            let retained = RetainedCall {
                profile_id: request.profile_id,
                preview: preview.clone(),
                status: PassportVaultCallSubmissionStatus {
                    draft_id,
                    state: PassportVaultCallSubmissionState::NotStarted,
                    transaction_hash_hex: None,
                    block_hash_hex: None,
                    block_height: None,
                    fee_atomic_units: None,
                    mode: None,
                },
            };
            *self
                .retained
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)? = Some(retained);
            Ok(preview)
        }

        fn authorize(
            &self,
            profile_id: &OpaqueId,
            request: AuthorizePassportVaultCallRequest,
        ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
            let mut guard = self.retained(profile_id, &request.draft_id)?;
            let retained = guard
                .as_mut()
                .ok_or(PassportVaultCallPortError::DraftNotFound)?;
            if retained.preview.authorization_challenge != request.authorization_challenge {
                return Err(PassportVaultCallPortError::AuthorizationChallengeMismatch);
            }
            if request.now > retained.preview.expires_at {
                return Err(PassportVaultCallPortError::DraftExpired);
            }
            retained.preview.state = PassportVaultCallDraftState::Authorized;
            Ok(retained.preview.clone())
        }

        fn submit<'a>(
            &'a self,
            profile_id: &'a OpaqueId,
            request: SubmitPassportVaultCallRequest,
        ) -> PassportVaultCallSubmissionFuture<'a> {
            Box::pin(async move {
                let mut guard = self.retained(profile_id, &request.draft_id)?;
                let retained = guard
                    .as_mut()
                    .ok_or(PassportVaultCallPortError::DraftNotFound)?;
                if request.now > retained.preview.expires_at {
                    return Err(PassportVaultCallPortError::DraftExpired);
                }
                if retained.preview.state != PassportVaultCallDraftState::Authorized {
                    return Err(PassportVaultCallPortError::DraftConflict);
                }
                retained.preview.state = PassportVaultCallDraftState::Submitted;
                retained.preview.fee_atomic_units = Some(9);
                retained.status = PassportVaultCallSubmissionStatus {
                    draft_id: request.draft_id,
                    state: PassportVaultCallSubmissionState::Included,
                    transaction_hash_hex: Some(INCLUDED_TX.to_owned()),
                    block_hash_hex: Some(INCLUDED_BLOCK.to_owned()),
                    block_height: Some(43),
                    fee_atomic_units: Some(9),
                    mode: Some("simulated".to_owned()),
                };
                Ok(SubmittedPassportVaultCall {
                    preview: retained.preview.clone(),
                    inclusion: PassportVaultCallInclusion {
                        transaction_hash_hex: INCLUDED_TX.to_owned(),
                        block_hash_hex: INCLUDED_BLOCK.to_owned(),
                        block_height: 43,
                        fee_atomic_units: 9,
                        mode: "simulated".to_owned(),
                    },
                })
            })
        }

        fn get(
            &self,
            profile_id: &OpaqueId,
            draft_id: &PassportVaultCallDraftId,
            _: UnixTimestampMillis,
        ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
            self.retained(profile_id, draft_id)?
                .as_ref()
                .map(|retained| retained.preview.clone())
                .ok_or(PassportVaultCallPortError::DraftNotFound)
        }

        fn submission_status(
            &self,
            profile_id: &OpaqueId,
            draft_id: &PassportVaultCallDraftId,
        ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
            self.retained(profile_id, draft_id)?
                .as_ref()
                .map(|retained| retained.status.clone())
                .ok_or(PassportVaultCallPortError::DraftNotFound)
        }

        fn cancel_submission(
            &self,
            profile_id: &OpaqueId,
            draft_id: &PassportVaultCallDraftId,
        ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
            let mut guard = self.retained(profile_id, draft_id)?;
            let retained = guard
                .as_mut()
                .ok_or(PassportVaultCallPortError::DraftNotFound)?;
            retained.status.state = PassportVaultCallSubmissionState::Cancelled;
            Ok(retained.status.clone())
        }

        fn submission_history(
            &self,
            profile_id: &OpaqueId,
        ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError> {
            let guard = self
                .retained
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            Ok(guard
                .as_ref()
                .filter(|retained| &retained.profile_id == profile_id)
                .map(|retained| vec![retained.status.clone()])
                .unwrap_or_default())
        }

        fn reconcile_submission<'a>(
            &'a self,
            profile_id: &'a OpaqueId,
            draft_id: &'a PassportVaultCallDraftId,
        ) -> PassportVaultCallStatusFuture<'a> {
            Box::pin(async move { self.submission_status(profile_id, draft_id) })
        }
    }

    fn call_service(
        authentication: PassportVaultContractStateAuthentication,
        calls: Arc<Calls>,
    ) -> PassportVaultContractCallService {
        PassportVaultContractCallService::new(
            Arc::new(Source { authentication }),
            calls,
            Arc::new(Clock),
            Arc::new(Random),
        )
    }

    fn simulated_call_service(
        authentication: PassportVaultContractStateAuthentication,
        calls: Arc<Calls>,
    ) -> PassportVaultContractCallService {
        PassportVaultContractCallService::new_simulated(
            Arc::new(Source { authentication }),
            calls,
            Arc::new(Clock),
            Arc::new(Random),
        )
    }

    fn command(action: PreparePassportVaultCallAction) -> PreparePassportVaultCallCommand {
        PreparePassportVaultCallCommand {
            profile_id: "profile_1".to_owned(),
            contract_address_hex: format!("0x{}", ADDRESS.to_uppercase()),
            action,
        }
    }

    fn ready<T>(mut future: Pin<Box<dyn Future<Output = T> + Send + '_>>) -> T {
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }

    #[test]
    fn prepares_every_supported_circuit_from_authenticated_state() {
        let calls = Arc::new(Calls::default());
        let service = call_service(
            PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
            Arc::clone(&calls),
        );
        let actions = [
            PreparePassportVaultCallAction::CreateLock {
                minimum_age_years: 18,
                required_issuing_state: None,
                required_document_number: None,
                maximum_claim_amount: "50".to_owned(),
                initial_amount: "0".to_owned(),
            },
            PreparePassportVaultCallAction::DepositToLock {
                lock_id: 3,
                amount: "10".to_owned(),
            },
            PreparePassportVaultCallAction::ClaimFromLock {
                lock_id: 3,
                amount: "4".to_owned(),
                credential_id: "credential_1".to_owned(),
            },
            PreparePassportVaultCallAction::WithdrawFromLock {
                lock_id: 3,
                amount: "6".to_owned(),
            },
        ];
        let expected = [
            "create_lock",
            "deposit_to_lock",
            "claim_from_lock",
            "withdraw_from_lock",
        ];

        for (action, expected_operation) in actions.into_iter().zip(expected) {
            let preview = ready(PreparePassportVaultCallUseCase::execute(
                &service,
                command(action),
            ))
            .expect("prepare");
            assert_eq!(preview.operation, expected_operation);
            assert_eq!(preview.contract_address_hex, ADDRESS);
            assert_eq!(preview.state, "prepared");
            assert_eq!(preview.state_anchor_block_height, 40);
            assert_eq!(preview.expires_at_millis, 3_601_000);
            assert!(preview.proof_required);
            assert!(!preview.submission_ready);
        }
        assert_eq!(calls.prepares.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn live_and_simulated_compositions_reject_each_others_state_authentication() {
        let action = || PreparePassportVaultCallAction::DepositToLock {
            lock_id: 3,
            amount: "10".to_owned(),
        };

        let calls = Arc::new(Calls::default());
        let live_service = call_service(
            PassportVaultContractStateAuthentication::DeterministicSimulation,
            Arc::clone(&calls),
        );
        assert_eq!(
            ready(PreparePassportVaultCallUseCase::execute(
                &live_service,
                command(action()),
            )),
            Err(PassportVaultCallError::UnauthenticatedState)
        );
        assert_eq!(calls.prepares.load(Ordering::SeqCst), 0);

        let calls = Arc::new(Calls::default());
        let simulated_service = simulated_call_service(
            PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
            Arc::clone(&calls),
        );
        assert_eq!(
            ready(PreparePassportVaultCallUseCase::execute(
                &simulated_service,
                command(action()),
            )),
            Err(PassportVaultCallError::UnauthenticatedState)
        );
        assert_eq!(calls.prepares.load(Ordering::SeqCst), 0);

        let simulated_service = simulated_call_service(
            PassportVaultContractStateAuthentication::DeterministicSimulation,
            Arc::clone(&calls),
        );
        assert!(
            ready(PreparePassportVaultCallUseCase::execute(
                &simulated_service,
                command(action()),
            ))
            .is_ok()
        );
        assert_eq!(calls.prepares.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn refuses_unproven_state_and_invalid_public_inputs_before_composition() {
        let calls = Arc::new(Calls::default());
        let unproven_service = call_service(
            PassportVaultContractStateAuthentication::IndexerSuppliedNotProven,
            Arc::clone(&calls),
        );
        let error = ready(PreparePassportVaultCallUseCase::execute(
            &unproven_service,
            command(PreparePassportVaultCallAction::DepositToLock {
                lock_id: 0,
                amount: "1".to_owned(),
            }),
        ));
        assert_eq!(error, Err(PassportVaultCallError::UnauthenticatedState));
        assert_eq!(calls.prepares.load(Ordering::SeqCst), 0);

        let calls = Arc::new(Calls::default());
        let service = call_service(
            PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
            Arc::clone(&calls),
        );
        let zero = ready(PreparePassportVaultCallUseCase::execute(
            &service,
            command(PreparePassportVaultCallAction::ClaimFromLock {
                lock_id: 0,
                amount: "0".to_owned(),
                credential_id: "credential_1".to_owned(),
            }),
        ));
        assert_eq!(zero, Err(PassportVaultCallError::ZeroAmount));
        let invalid = ready(PreparePassportVaultCallUseCase::execute(
            &service,
            command(PreparePassportVaultCallAction::DepositToLock {
                lock_id: 0,
                amount: "-1".to_owned(),
            }),
        ));
        assert_eq!(invalid, Err(PassportVaultCallError::InvalidAmount));
        let noncanonical = ready(PreparePassportVaultCallUseCase::execute(
            &service,
            command(PreparePassportVaultCallAction::DepositToLock {
                lock_id: 0,
                amount: "01".to_owned(),
            }),
        ));
        assert_eq!(noncanonical, Err(PassportVaultCallError::InvalidAmount));
        assert_eq!(calls.prepares.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn authorizes_submits_and_reconciles_without_exposing_transaction_material() {
        let calls = Arc::new(Calls::default());
        let service = call_service(
            PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
            calls,
        );
        let prepared = ready(PreparePassportVaultCallUseCase::execute(
            &service,
            command(PreparePassportVaultCallAction::WithdrawFromLock {
                lock_id: 8,
                amount: "12".to_owned(),
            }),
        ))
        .expect("prepare");
        let denied = AuthorizePassportVaultCallUseCase::execute(
            &service,
            AuthorizePassportVaultCallCommand {
                profile_id: "profile_1".to_owned(),
                draft_id: prepared.draft_id.clone(),
                authorization_challenge: prepared.authorization_challenge.clone(),
                confirmed: false,
                intent: AUTHORIZE_PASSPORT_VAULT_CALL_INTENT.to_owned(),
            },
        );
        assert_eq!(denied, Err(PassportVaultCallError::ConfirmationRequired));

        let authorized = AuthorizePassportVaultCallUseCase::execute(
            &service,
            AuthorizePassportVaultCallCommand {
                profile_id: "profile_1".to_owned(),
                draft_id: prepared.draft_id.clone(),
                authorization_challenge: prepared.authorization_challenge,
                confirmed: true,
                intent: AUTHORIZE_PASSPORT_VAULT_CALL_INTENT.to_owned(),
            },
        )
        .expect("authorize");
        assert_eq!(authorized.state, "authorized");
        assert!(authorized.submission_ready);

        let submitted = ready(SubmitPassportVaultCallUseCase::execute(
            &service,
            SubmitPassportVaultCallCommand {
                profile_id: "profile_1".to_owned(),
                draft_id: prepared.draft_id.clone(),
                confirmed: true,
                intent: SUBMIT_PASSPORT_VAULT_CALL_INTENT.to_owned(),
            },
        ))
        .expect("submit");
        assert_eq!(submitted.call.state, "submitted");
        assert_eq!(submitted.transaction_hash_hex, INCLUDED_TX);
        assert_eq!(submitted.block_hash_hex, INCLUDED_BLOCK);
        assert_eq!(submitted.fee_atomic_units, "9");
        assert_eq!(submitted.mode, "simulated");

        let query = PassportVaultCallQuery {
            profile_id: "profile_1".to_owned(),
            draft_id: prepared.draft_id,
        };
        let status = GetPassportVaultCallSubmissionStatusUseCase::execute(&service, query.clone())
            .expect("status");
        assert_eq!(status.state, "included");
        assert!(!status.retryable);
        assert!(!status.reconciliation_allowed);
        let reconciled = ready(ReconcilePassportVaultCallSubmissionUseCase::execute(
            &service, query,
        ))
        .expect("reconcile");
        assert_eq!(reconciled, status);
        let history =
            ListPassportVaultCallSubmissionsUseCase::execute(&service, "profile_1".to_owned())
                .expect("history");
        assert_eq!(history, vec![status]);
    }

    #[test]
    fn unavailable_port_and_wrong_authorization_challenge_fail_closed() {
        let unavailable = PassportVaultContractCallService::new(
            Arc::new(Source {
                authentication: PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
            }),
            Arc::new(UnavailablePassportVaultContractCall),
            Arc::new(Clock),
            Arc::new(Random),
        );
        assert_eq!(
            ready(PreparePassportVaultCallUseCase::execute(
                &unavailable,
                command(PreparePassportVaultCallAction::DepositToLock {
                    lock_id: 0,
                    amount: "1".to_owned(),
                }),
            )),
            Err(PassportVaultCallError::Operation(
                PassportVaultCallPortError::Unavailable
            ))
        );

        let calls = Arc::new(Calls::default());
        let service = call_service(
            PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
            calls,
        );
        let prepared = ready(PreparePassportVaultCallUseCase::execute(
            &service,
            command(PreparePassportVaultCallAction::DepositToLock {
                lock_id: 0,
                amount: "1".to_owned(),
            }),
        ))
        .expect("prepare");
        assert_eq!(
            AuthorizePassportVaultCallUseCase::execute(
                &service,
                AuthorizePassportVaultCallCommand {
                    profile_id: "profile_1".to_owned(),
                    draft_id: prepared.draft_id,
                    authorization_challenge: "wrong_challenge".to_owned(),
                    confirmed: true,
                    intent: AUTHORIZE_PASSPORT_VAULT_CALL_INTENT.to_owned(),
                },
            ),
            Err(PassportVaultCallError::Operation(
                PassportVaultCallPortError::AuthorizationChallengeMismatch
            ))
        );

        let calls = Arc::new(Calls::default());
        let service = call_service(
            PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
            Arc::clone(&calls),
        );
        let prepared = ready(PreparePassportVaultCallUseCase::execute(
            &service,
            command(PreparePassportVaultCallAction::DepositToLock {
                lock_id: 0,
                amount: "1".to_owned(),
            }),
        ))
        .expect("prepare");
        calls
            .retained
            .lock()
            .expect("retained")
            .as_mut()
            .expect("draft")
            .preview
            .expires_at = UnixTimestampMillis::new(1_000);
        assert_eq!(
            AuthorizePassportVaultCallUseCase::execute(
                &service,
                AuthorizePassportVaultCallCommand {
                    profile_id: "profile_1".to_owned(),
                    draft_id: prepared.draft_id.clone(),
                    authorization_challenge: prepared.authorization_challenge.clone(),
                    confirmed: true,
                    intent: AUTHORIZE_PASSPORT_VAULT_CALL_INTENT.to_owned(),
                },
            ),
            Err(PassportVaultCallError::Operation(
                PassportVaultCallPortError::DraftExpired
            ))
        );
        calls
            .retained
            .lock()
            .expect("retained")
            .as_mut()
            .expect("draft")
            .preview
            .expires_at = UnixTimestampMillis::new(3_601_000);
        AuthorizePassportVaultCallUseCase::execute(
            &service,
            AuthorizePassportVaultCallCommand {
                profile_id: "profile_1".to_owned(),
                draft_id: prepared.draft_id.clone(),
                authorization_challenge: prepared.authorization_challenge,
                confirmed: true,
                intent: AUTHORIZE_PASSPORT_VAULT_CALL_INTENT.to_owned(),
            },
        )
        .expect("authorize before expiry");
        calls
            .retained
            .lock()
            .expect("retained")
            .as_mut()
            .expect("draft")
            .preview
            .expires_at = UnixTimestampMillis::new(1_000);
        assert_eq!(
            ready(SubmitPassportVaultCallUseCase::execute(
                &service,
                SubmitPassportVaultCallCommand {
                    profile_id: "profile_1".to_owned(),
                    draft_id: prepared.draft_id,
                    confirmed: true,
                    intent: SUBMIT_PASSPORT_VAULT_CALL_INTENT.to_owned(),
                },
            )),
            Err(PassportVaultCallError::Operation(
                PassportVaultCallPortError::DraftExpired
            ))
        );
    }
}
