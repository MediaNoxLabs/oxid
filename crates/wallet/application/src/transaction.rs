// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use oxid_foundation::{OpaqueIdError, UnixTimestampMillis};
use oxid_platform_ports::{ClockPort, PlatformError};
use oxid_wallet_domain::{
    AssetBalance, ChainAddress, ChainAddressError, ChainAddressKind, ChainAssetId, WalletProfileId,
    WalletTransactionAuthorizationChallenge, WalletTransactionDraftId, WalletTransactionDraftState,
    WalletTransactionFeeState, WalletTransactionSubmissionState, WalletTransactionSubmissionStatus,
    WalletTransferPreview, WalletTransferSubmission, WalletTransferSubmissionMode,
};

use crate::{SensitiveOperationConfirmation, SensitiveWalletOperationError, validate_confirmation};

/// Lifetime of a prepared transfer before its retained signing material expires.
pub const WALLET_TRANSFER_DRAFT_TTL_MILLIS: u64 = 60 * 60 * 1_000;

/// Safe failures returned by transaction planning/authorization adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletTransactionPortError {
    Unavailable,
    ProtectionNotInitialized,
    ProtectionLocked,
    AccountNotDerived,
    AccountNotSynchronized,
    ShieldedStateNotCurrent,
    UnsupportedNetwork,
    InvalidRecipient,
    RecipientNetworkMismatch,
    InsufficientFunds,
    DraftNotFound,
    DraftExpired,
    DraftConflict,
    SubmissionInProgress,
    SubmissionNotInProgress,
    SubmissionCancelled,
    SubmissionCancellationUnsafe,
    AuthorizationChallengeMismatch,
    InsufficientDust,
    InvalidChainState,
    ProvingFailed,
    SubmissionRejected,
    SubmissionOutcomeUnknown,
    Timeout,
    InvalidData,
}

impl fmt::Display for WalletTransactionPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unavailable => "wallet transaction capability is unavailable",
            Self::ProtectionNotInitialized => "wallet protection is not initialized",
            Self::ProtectionLocked => "wallet is locked",
            Self::AccountNotDerived => "a protected wallet account must be derived first",
            Self::AccountNotSynchronized => "wallet account must be synchronized first",
            Self::ShieldedStateNotCurrent => {
                "shielded wallet state must complete a live synchronization first"
            }
            Self::UnsupportedNetwork => "wallet network is not supported",
            Self::InvalidRecipient => "transaction recipient is invalid",
            Self::RecipientNetworkMismatch => "transaction recipient belongs to another network",
            Self::InsufficientFunds => "wallet has insufficient funds",
            Self::DraftNotFound => "transaction draft was not found",
            Self::DraftExpired => "transaction draft has expired",
            Self::DraftConflict => "transaction draft conflicts with current wallet state",
            Self::SubmissionInProgress => "transaction submission is already in progress",
            Self::SubmissionNotInProgress => "transaction submission is not in progress",
            Self::SubmissionCancelled => "transaction submission was cancelled before broadcast",
            Self::SubmissionCancellationUnsafe => {
                "transaction submission can no longer be cancelled safely"
            }
            Self::AuthorizationChallengeMismatch => {
                "transaction authorization does not match the prepared preview"
            }
            Self::InsufficientDust => "wallet has insufficient DUST for the transaction fee",
            Self::InvalidChainState => "Midnight chain state is invalid or unavailable",
            Self::ProvingFailed => "transaction proving failed",
            Self::SubmissionRejected => "Midnight rejected the transaction submission",
            Self::SubmissionOutcomeUnknown => {
                "Midnight transaction submission outcome is not yet known"
            }
            Self::Timeout => "transaction operation timed out",
            Self::InvalidData => "transaction adapter returned invalid data",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletTransactionPortError {}

/// Adapter-neutral request for constructing one exact transfer intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareWalletTransferRequest {
    pub recipient: ChainAddress,
    pub amount_atomic_units: u128,
    pub expires_at: UnixTimestampMillis,
}

/// Adapter-neutral request for constructing one exact shielded transfer intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareShieldedWalletTransferRequest {
    pub recipient: ChainAddress,
    pub asset_id: ChainAssetId,
    pub amount_atomic_units: u128,
    pub expires_at: UnixTimestampMillis,
}

/// Adapter-neutral request for authorizing one retained transfer draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizeWalletTransferRequest {
    pub draft_id: WalletTransactionDraftId,
    pub authorization_challenge: WalletTransactionAuthorizationChallenge,
    pub now: UnixTimestampMillis,
}

/// Adapter-neutral request for completing an authorized retained transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitWalletTransferRequest {
    pub draft_id: WalletTransactionDraftId,
    pub now: UnixTimestampMillis,
}

/// Adapter result combining safe final preview metadata and public inclusion identifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmittedWalletTransfer {
    pub preview: WalletTransferPreview,
    pub submission: WalletTransferSubmission,
}

/// Asynchronous result returned by a transaction adapter.
pub type WalletTransactionPortFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<SubmittedWalletTransfer, WalletTransactionPortError>>
            + Send
            + 'a,
    >,
>;

/// Asynchronous safe status returned by transaction reconciliation.
pub type WalletTransactionStatusPortFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<WalletTransactionSubmissionStatus, WalletTransactionPortError>>
            + Send
            + 'a,
    >,
>;

/// Focused outgoing port retaining chain-specific draft/signing material.
pub trait WalletTransactionPort: Send + Sync {
    fn prepare(
        &self,
        profile_id: &WalletProfileId,
        request: PrepareWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError>;

    fn prepare_shielded(
        &self,
        _: &WalletProfileId,
        _: PrepareShieldedWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }

    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        request: AuthorizeWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError>;

    fn submit<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        request: SubmitWalletTransferRequest,
    ) -> WalletTransactionPortFuture<'a>;

    /// Read one retained in-memory draft snapshot. Persistent recovery belongs
    /// to `submission_history`, not this latency-sensitive control query.
    fn get(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
        now: UnixTimestampMillis,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError>;

    /// Read already-published worker state without transport or filesystem I/O.
    fn submission_status(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<WalletTransactionSubmissionStatus, WalletTransactionPortError>;

    /// Signal the adapter-owned worker and return its bounded current state;
    /// this method must not wait for cancellation acknowledgement.
    fn cancel_submission(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<WalletTransactionSubmissionStatus, WalletTransactionPortError>;

    fn submission_history(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Vec<WalletTransactionSubmissionStatus>, WalletTransactionPortError>;

    fn reconcile_submission<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        draft_id: &'a WalletTransactionDraftId,
    ) -> WalletTransactionStatusPortFuture<'a>;
}

/// Incoming request for preparing an unshielded NIGHT transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareWalletTransferCommand {
    pub profile_id: String,
    pub recipient_address: String,
    pub amount_atomic_units: String,
}

/// Incoming request for preparing a shielded token transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareShieldedWalletTransferCommand {
    pub profile_id: String,
    pub recipient_address: String,
    pub token_type: String,
    pub amount_atomic_units: String,
}

/// Incoming request for authorizing an exact prepared transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizeWalletTransferCommand {
    pub profile_id: String,
    pub draft_id: String,
    pub authorization_challenge: String,
    pub confirmation: SensitiveOperationConfirmation,
}

/// Incoming request for proving and submitting one authorized transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitWalletTransferCommand {
    pub profile_id: String,
    pub draft_id: String,
    pub confirmation: SensitiveOperationConfirmation,
}

/// Incoming query for safe retained-draft metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransferDraftQuery {
    pub profile_id: String,
    pub draft_id: String,
}

/// Incoming query/command identifying one retained submission attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransferSubmissionQuery {
    pub profile_id: String,
    pub draft_id: String,
}

/// Exact asset value exposed to incoming adapters without floating point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransferAssetView {
    pub asset_id: String,
    pub symbol: String,
    pub decimals: u8,
    pub atomic_units: String,
}

/// Safe transfer preview with no signing bytes or serialized transaction material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransferPreviewView {
    pub draft_id: String,
    pub authorization_challenge: String,
    pub network_id: String,
    pub account_id: String,
    pub recipient_address: String,
    pub recipient_kind: String,
    pub amount: WalletTransferAssetView,
    pub change: WalletTransferAssetView,
    pub fee: Option<WalletTransferAssetView>,
    pub fee_state: String,
    pub input_count: u16,
    pub expires_at_millis: u64,
    pub state: String,
    pub proof_required: bool,
    pub submission_ready: bool,
}

impl From<&WalletTransferPreview> for WalletTransferPreviewView {
    fn from(preview: &WalletTransferPreview) -> Self {
        Self {
            draft_id: preview.draft_id().as_str().to_owned(),
            authorization_challenge: preview.authorization_challenge().as_str().to_owned(),
            network_id: preview.network_id().as_str().to_owned(),
            account_id: preview.account_id().as_str().to_owned(),
            recipient_address: preview.recipient().value().to_owned(),
            recipient_kind: address_kind_name(preview.recipient().kind()).to_owned(),
            amount: asset_view(preview.amount()),
            change: asset_view(preview.change()),
            fee: preview.fee().map(asset_view),
            fee_state: fee_state_name(preview.fee_state()).to_owned(),
            input_count: preview.input_count(),
            expires_at_millis: preview.expires_at().value(),
            state: draft_state_name(preview.state()).to_owned(),
            proof_required: !matches!(preview.state(), WalletTransactionDraftState::Submitted),
            submission_ready: matches!(preview.state(), WalletTransactionDraftState::Authorized),
        }
    }
}

/// Incoming use case for creating a retained transfer draft.
pub trait PrepareWalletTransferUseCase: Send + Sync {
    fn execute(
        &self,
        command: PrepareWalletTransferCommand,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError>;
}

/// Incoming use case for creating a retained shielded transfer draft.
pub trait PrepareShieldedWalletTransferUseCase: Send + Sync {
    fn execute(
        &self,
        command: PrepareShieldedWalletTransferCommand,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError>;
}

/// Incoming use case for explicitly authorizing a retained transfer draft.
pub trait AuthorizeWalletTransferUseCase: Send + Sync {
    fn execute(
        &self,
        command: AuthorizeWalletTransferCommand,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError>;
}

/// Public result of an included transfer without proof or serialized transaction bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransferSubmissionView {
    pub transfer: WalletTransferPreviewView,
    pub transaction_id: String,
    pub block_id: String,
    pub fee: WalletTransferAssetView,
    pub mode: String,
}

/// Safe submission lifecycle view with optional public inclusion metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransferSubmissionStatusView {
    pub draft_id: String,
    pub state: String,
    pub cancellation_allowed: bool,
    pub retryable: bool,
    pub replacement_allowed: bool,
    pub reconciliation_allowed: bool,
    pub transaction_id: Option<String>,
    pub block_id: Option<String>,
    pub fee: Option<WalletTransferAssetView>,
    pub mode: Option<String>,
}

impl From<&WalletTransactionSubmissionStatus> for WalletTransferSubmissionStatusView {
    fn from(status: &WalletTransactionSubmissionStatus) -> Self {
        Self {
            draft_id: status.draft_id().as_str().to_owned(),
            state: submission_state_name(status.state()).to_owned(),
            cancellation_allowed: status.cancellation_allowed(),
            retryable: status.retryable(),
            replacement_allowed: status.replacement_allowed(),
            reconciliation_allowed: status.reconciliation_allowed(),
            transaction_id: status
                .transaction_id()
                .map(|value| value.as_str().to_owned()),
            block_id: status.block_id().map(|value| value.as_str().to_owned()),
            fee: status.fee().map(asset_view),
            mode: status
                .mode()
                .map(|value| submission_mode_name(value).to_owned()),
        }
    }
}

impl From<&SubmittedWalletTransfer> for WalletTransferSubmissionView {
    fn from(value: &SubmittedWalletTransfer) -> Self {
        Self {
            transfer: WalletTransferPreviewView::from(&value.preview),
            transaction_id: value.submission.transaction_id().as_str().to_owned(),
            block_id: value.submission.block_id().as_str().to_owned(),
            fee: asset_view(value.submission.fee()),
            mode: submission_mode_name(value.submission.mode()).to_owned(),
        }
    }
}

/// Incoming use case for completing an authorized transfer off the UI thread.
pub trait SubmitWalletTransferUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: SubmitWalletTransferCommand,
    ) -> WalletTransferSubmissionViewFuture<'a>;
}

/// Asynchronous submission view returned to incoming adapters.
pub type WalletTransferSubmissionViewFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<WalletTransferSubmissionView, WalletTransactionError>>
            + Send
            + 'a,
    >,
>;

/// Incoming use case for reading safe retained-draft state.
pub trait GetWalletTransferDraftUseCase: Send + Sync {
    fn execute(
        &self,
        query: WalletTransferDraftQuery,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError>;
}

/// Incoming use case for reading an adapter-owned submission attempt.
pub trait GetWalletTransferSubmissionStatusUseCase: Send + Sync {
    fn execute(
        &self,
        query: WalletTransferSubmissionQuery,
    ) -> Result<WalletTransferSubmissionStatusView, WalletTransactionError>;
}

/// Incoming use case for requesting cooperative pre-broadcast cancellation.
pub trait CancelWalletTransferSubmissionUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletTransferSubmissionQuery,
    ) -> Result<WalletTransferSubmissionStatusView, WalletTransactionError>;
}

/// Incoming use case for listing durable public submission attempts.
pub trait ListWalletTransferSubmissionsUseCase: Send + Sync {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<Vec<WalletTransferSubmissionStatusView>, WalletTransactionError>;
}

/// Incoming use case for reconciling one broadcast against finalized chain state.
pub trait ReconcileWalletTransferSubmissionUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        query: WalletTransferSubmissionQuery,
    ) -> WalletTransferSubmissionStatusViewFuture<'a>;
}

/// Asynchronous safe lifecycle view returned by reconciliation.
pub type WalletTransferSubmissionStatusViewFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<WalletTransferSubmissionStatusView, WalletTransactionError>>
            + Send
            + 'a,
    >,
>;

/// Stable transaction failures exposed by the application boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletTransactionError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidDraftIdentifier(OpaqueIdError),
    InvalidAuthorizationChallenge(OpaqueIdError),
    InvalidRecipient(ChainAddressError),
    InvalidAmount,
    InvalidTokenType,
    ZeroAmount,
    ConfirmationRequired,
    InvalidConfirmation,
    Clock(PlatformError),
    Operation(WalletTransactionPortError),
}

impl fmt::Display for WalletTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error)
            | Self::InvalidDraftIdentifier(error)
            | Self::InvalidAuthorizationChallenge(error) => error.fmt(formatter),
            Self::InvalidRecipient(error) => error.fmt(formatter),
            Self::InvalidAmount => {
                formatter.write_str("transaction amount must be an unsigned integer")
            }
            Self::InvalidTokenType => formatter
                .write_str("shielded token type must be exactly 32 lowercase hexadecimal bytes"),
            Self::ZeroAmount => formatter.write_str("transaction amount must be greater than zero"),
            Self::ConfirmationRequired => formatter.write_str("explicit confirmation is required"),
            Self::InvalidConfirmation => formatter.write_str("confirmation intent is invalid"),
            Self::Clock(error) => error.fmt(formatter),
            Self::Operation(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletTransactionError {}

/// Transaction application service; adapter state owns every chain-specific artifact.
pub struct WalletTransactionService<T, C> {
    transactions: Arc<T>,
    clock: Arc<C>,
}

impl<T, C> WalletTransactionService<T, C> {
    #[must_use]
    pub const fn new(transactions: Arc<T>, clock: Arc<C>) -> Self {
        Self {
            transactions,
            clock,
        }
    }

    fn now(&self) -> Result<UnixTimestampMillis, WalletTransactionError>
    where
        C: ClockPort,
    {
        self.clock.now().map_err(WalletTransactionError::Clock)
    }
}

impl<T, C> PrepareWalletTransferUseCase for WalletTransactionService<T, C>
where
    T: WalletTransactionPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        command: PrepareWalletTransferCommand,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletTransactionError::InvalidProfileIdentifier)?;
        let recipient =
            ChainAddress::parse(ChainAddressKind::Unshielded, command.recipient_address)
                .map_err(WalletTransactionError::InvalidRecipient)?;
        let amount_atomic_units = command
            .amount_atomic_units
            .parse::<u128>()
            .map_err(|_| WalletTransactionError::InvalidAmount)?;
        if amount_atomic_units == 0 {
            return Err(WalletTransactionError::ZeroAmount);
        }
        let expires_at = self
            .now()?
            .value()
            .checked_add(WALLET_TRANSFER_DRAFT_TTL_MILLIS)
            .map(UnixTimestampMillis::new)
            .ok_or(WalletTransactionError::Clock(
                PlatformError::ClockUnavailable,
            ))?;
        let preview = self
            .transactions
            .prepare(
                &profile_id,
                PrepareWalletTransferRequest {
                    recipient,
                    amount_atomic_units,
                    expires_at,
                },
            )
            .map_err(WalletTransactionError::Operation)?;
        Ok(WalletTransferPreviewView::from(&preview))
    }
}

impl<T, C> PrepareShieldedWalletTransferUseCase for WalletTransactionService<T, C>
where
    T: WalletTransactionPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        command: PrepareShieldedWalletTransferCommand,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletTransactionError::InvalidProfileIdentifier)?;
        let recipient = ChainAddress::parse(ChainAddressKind::Shielded, command.recipient_address)
            .map_err(WalletTransactionError::InvalidRecipient)?;
        if command.token_type.len() != 64
            || !command
                .token_type
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(WalletTransactionError::InvalidTokenType);
        }
        let asset_id = ChainAssetId::parse(format!("midnight:shielded:{}", command.token_type))
            .map_err(|_| WalletTransactionError::InvalidTokenType)?;
        let amount_atomic_units = command
            .amount_atomic_units
            .parse::<u128>()
            .map_err(|_| WalletTransactionError::InvalidAmount)?;
        if amount_atomic_units == 0 {
            return Err(WalletTransactionError::ZeroAmount);
        }
        let expires_at = self
            .now()?
            .value()
            .checked_add(WALLET_TRANSFER_DRAFT_TTL_MILLIS)
            .map(UnixTimestampMillis::new)
            .ok_or(WalletTransactionError::Clock(
                PlatformError::ClockUnavailable,
            ))?;
        let preview = self
            .transactions
            .prepare_shielded(
                &profile_id,
                PrepareShieldedWalletTransferRequest {
                    recipient,
                    asset_id,
                    amount_atomic_units,
                    expires_at,
                },
            )
            .map_err(WalletTransactionError::Operation)?;
        Ok(WalletTransferPreviewView::from(&preview))
    }
}

impl<T, C> AuthorizeWalletTransferUseCase for WalletTransactionService<T, C>
where
    T: WalletTransactionPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        command: AuthorizeWalletTransferCommand,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError> {
        validate_confirmation(&command.confirmation).map_err(map_confirmation_error)?;
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletTransactionError::InvalidProfileIdentifier)?;
        let draft_id = WalletTransactionDraftId::parse(command.draft_id)
            .map_err(WalletTransactionError::InvalidDraftIdentifier)?;
        let authorization_challenge =
            WalletTransactionAuthorizationChallenge::parse(command.authorization_challenge)
                .map_err(WalletTransactionError::InvalidAuthorizationChallenge)?;
        let preview = self
            .transactions
            .authorize(
                &profile_id,
                AuthorizeWalletTransferRequest {
                    draft_id,
                    authorization_challenge,
                    now: self.now()?,
                },
            )
            .map_err(WalletTransactionError::Operation)?;
        Ok(WalletTransferPreviewView::from(&preview))
    }
}

impl<T, C> SubmitWalletTransferUseCase for WalletTransactionService<T, C>
where
    T: WalletTransactionPort + 'static,
    C: ClockPort + 'static,
{
    fn execute<'a>(
        &'a self,
        command: SubmitWalletTransferCommand,
    ) -> WalletTransferSubmissionViewFuture<'a> {
        Box::pin(async move {
            validate_confirmation(&command.confirmation).map_err(map_confirmation_error)?;
            let profile_id = WalletProfileId::parse(command.profile_id)
                .map_err(WalletTransactionError::InvalidProfileIdentifier)?;
            let draft_id = WalletTransactionDraftId::parse(command.draft_id)
                .map_err(WalletTransactionError::InvalidDraftIdentifier)?;
            let submitted = self
                .transactions
                .submit(
                    &profile_id,
                    SubmitWalletTransferRequest {
                        draft_id,
                        now: self.now()?,
                    },
                )
                .await
                .map_err(WalletTransactionError::Operation)?;
            Ok(WalletTransferSubmissionView::from(&submitted))
        })
    }
}

impl<T, C> GetWalletTransferDraftUseCase for WalletTransactionService<T, C>
where
    T: WalletTransactionPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        query: WalletTransferDraftQuery,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError> {
        let profile_id = WalletProfileId::parse(query.profile_id)
            .map_err(WalletTransactionError::InvalidProfileIdentifier)?;
        let draft_id = WalletTransactionDraftId::parse(query.draft_id)
            .map_err(WalletTransactionError::InvalidDraftIdentifier)?;
        let preview = self
            .transactions
            .get(&profile_id, &draft_id, self.now()?)
            .map_err(WalletTransactionError::Operation)?;
        Ok(WalletTransferPreviewView::from(&preview))
    }
}

impl<T, C> GetWalletTransferSubmissionStatusUseCase for WalletTransactionService<T, C>
where
    T: WalletTransactionPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        query: WalletTransferSubmissionQuery,
    ) -> Result<WalletTransferSubmissionStatusView, WalletTransactionError> {
        let profile_id = WalletProfileId::parse(query.profile_id)
            .map_err(WalletTransactionError::InvalidProfileIdentifier)?;
        let draft_id = WalletTransactionDraftId::parse(query.draft_id)
            .map_err(WalletTransactionError::InvalidDraftIdentifier)?;
        let status = self
            .transactions
            .submission_status(&profile_id, &draft_id)
            .map_err(WalletTransactionError::Operation)?;
        Ok(WalletTransferSubmissionStatusView::from(&status))
    }
}

impl<T, C> CancelWalletTransferSubmissionUseCase for WalletTransactionService<T, C>
where
    T: WalletTransactionPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        command: WalletTransferSubmissionQuery,
    ) -> Result<WalletTransferSubmissionStatusView, WalletTransactionError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletTransactionError::InvalidProfileIdentifier)?;
        let draft_id = WalletTransactionDraftId::parse(command.draft_id)
            .map_err(WalletTransactionError::InvalidDraftIdentifier)?;
        let status = self
            .transactions
            .cancel_submission(&profile_id, &draft_id)
            .map_err(WalletTransactionError::Operation)?;
        Ok(WalletTransferSubmissionStatusView::from(&status))
    }
}

impl<T, C> ListWalletTransferSubmissionsUseCase for WalletTransactionService<T, C>
where
    T: WalletTransactionPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<Vec<WalletTransferSubmissionStatusView>, WalletTransactionError> {
        let profile_id = WalletProfileId::parse(profile_id)
            .map_err(WalletTransactionError::InvalidProfileIdentifier)?;
        self.transactions
            .submission_history(&profile_id)
            .map_err(WalletTransactionError::Operation)
            .map(|statuses| {
                statuses
                    .iter()
                    .map(WalletTransferSubmissionStatusView::from)
                    .collect()
            })
    }
}

impl<T, C> ReconcileWalletTransferSubmissionUseCase for WalletTransactionService<T, C>
where
    T: WalletTransactionPort + 'static,
    C: ClockPort + 'static,
{
    fn execute<'a>(
        &'a self,
        query: WalletTransferSubmissionQuery,
    ) -> WalletTransferSubmissionStatusViewFuture<'a> {
        Box::pin(async move {
            let profile_id = WalletProfileId::parse(query.profile_id)
                .map_err(WalletTransactionError::InvalidProfileIdentifier)?;
            let draft_id = WalletTransactionDraftId::parse(query.draft_id)
                .map_err(WalletTransactionError::InvalidDraftIdentifier)?;
            let status = self
                .transactions
                .reconcile_submission(&profile_id, &draft_id)
                .await
                .map_err(WalletTransactionError::Operation)?;
            Ok(WalletTransferSubmissionStatusView::from(&status))
        })
    }
}

fn asset_view(balance: &AssetBalance) -> WalletTransferAssetView {
    WalletTransferAssetView {
        asset_id: balance.asset().id().as_str().to_owned(),
        symbol: balance.asset().symbol().as_str().to_owned(),
        decimals: balance.asset().decimals(),
        atomic_units: balance.atomic_units().to_string(),
    }
}

const fn fee_state_name(state: WalletTransactionFeeState) -> &'static str {
    match state {
        WalletTransactionFeeState::RequiresBalancing => "requires_balancing",
        WalletTransactionFeeState::Estimated => "estimated",
        WalletTransactionFeeState::Final => "final",
    }
}

const fn address_kind_name(kind: ChainAddressKind) -> &'static str {
    match kind {
        ChainAddressKind::Unshielded => "unshielded",
        ChainAddressKind::Shielded => "shielded",
        ChainAddressKind::Dust => "dust",
        ChainAddressKind::Reward => "reward",
    }
}

const fn draft_state_name(state: WalletTransactionDraftState) -> &'static str {
    match state {
        WalletTransactionDraftState::Prepared => "prepared",
        WalletTransactionDraftState::Authorized => "authorized",
        WalletTransactionDraftState::Submitting => "submitting",
        WalletTransactionDraftState::Submitted => "submitted",
        WalletTransactionDraftState::Expired => "expired",
    }
}

const fn submission_mode_name(mode: WalletTransferSubmissionMode) -> &'static str {
    match mode {
        WalletTransferSubmissionMode::Simulated => "simulated",
        WalletTransferSubmissionMode::Live => "live",
    }
}

const fn submission_state_name(state: WalletTransactionSubmissionState) -> &'static str {
    match state {
        WalletTransactionSubmissionState::NotStarted => "not_started",
        WalletTransactionSubmissionState::Running => "running",
        WalletTransactionSubmissionState::CancellationRequested => "cancellation_requested",
        WalletTransactionSubmissionState::Broadcasting => "broadcasting",
        WalletTransactionSubmissionState::Cancelled => "cancelled",
        WalletTransactionSubmissionState::Included => "included",
        WalletTransactionSubmissionState::Rejected => "rejected",
        WalletTransactionSubmissionState::Expired => "expired",
        WalletTransactionSubmissionState::OutcomeUnknown => "outcome_unknown",
    }
}

const fn map_confirmation_error(error: SensitiveWalletOperationError) -> WalletTransactionError {
    match error {
        SensitiveWalletOperationError::ConfirmationRequired => {
            WalletTransactionError::ConfirmationRequired
        }
        SensitiveWalletOperationError::InvalidConfirmation => {
            WalletTransactionError::InvalidConfirmation
        }
        SensitiveWalletOperationError::InvalidProfileIdentifier(_)
        | SensitiveWalletOperationError::InvalidKeyReference(_)
        | SensitiveWalletOperationError::EmptyPayload
        | SensitiveWalletOperationError::PayloadTooLarge
        | SensitiveWalletOperationError::Operation(_) => {
            WalletTransactionError::InvalidConfirmation
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::pin,
        sync::Mutex,
        task::{Context, Poll, Waker},
    };

    use oxid_wallet_domain::{
        AssetSymbol, ChainAccountId, ChainAsset, ChainAssetId, ChainBlockId, ChainNetworkId,
        ChainTransactionId, WalletTransactionDraftState, WalletTransactionFeeState,
        WalletTransferSubmission, WalletTransferSubmissionMode,
    };

    use super::*;

    fn ready<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let waker = Waker::noop();
        match future.as_mut().poll(&mut Context::from_waker(waker)) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("recording adapter unexpectedly returned a pending future"),
        }
    }

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    #[derive(Default)]
    struct RecordingTransactions {
        prepare_calls: Mutex<usize>,
        prepare_shielded_calls: Mutex<usize>,
        authorize_calls: Mutex<usize>,
        submit_calls: Mutex<usize>,
    }

    impl RecordingTransactions {
        fn preview(state: WalletTransactionDraftState) -> WalletTransferPreview {
            let night = ChainAsset::new(
                ChainAssetId::parse("midnight:night").expect("asset id is valid"),
                AssetSymbol::parse("NIGHT").expect("symbol is valid"),
                6,
            );
            WalletTransferPreview::new(
                WalletTransactionDraftId::parse("txdraft_test").expect("draft is valid"),
                WalletTransactionAuthorizationChallenge::parse("txauth_test")
                    .expect("challenge is valid"),
                ChainNetworkId::parse("undeployed").expect("network is valid"),
                ChainAccountId::parse("midnight_account_0_0").expect("account is valid"),
                ChainAddress::parse(ChainAddressKind::Unshielded, "mn_addr_undeployed1recipient")
                    .expect("recipient is structurally valid"),
                AssetBalance::new(night.clone(), 1_000_000),
                AssetBalance::new(night, 4_000_000),
                None,
                WalletTransactionFeeState::RequiresBalancing,
                1,
                UnixTimestampMillis::new(1_700_003_600_000),
                state,
            )
            .expect("preview is valid")
        }
    }

    impl WalletTransactionPort for RecordingTransactions {
        fn prepare(
            &self,
            _: &WalletProfileId,
            request: PrepareWalletTransferRequest,
        ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
            assert_eq!(request.amount_atomic_units, 1_000_000);
            assert_eq!(request.expires_at.value(), 1_700_003_600_000);
            *self.prepare_calls.lock().expect("counter is available") += 1;
            Ok(Self::preview(WalletTransactionDraftState::Prepared))
        }

        fn prepare_shielded(
            &self,
            _: &WalletProfileId,
            request: PrepareShieldedWalletTransferRequest,
        ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
            assert_eq!(
                request.asset_id.as_str(),
                "midnight:shielded:0000000000000000000000000000000000000000000000000000000000000000"
            );
            assert_eq!(request.amount_atomic_units, 1_000_000);
            assert_eq!(request.expires_at.value(), 1_700_003_600_000);
            *self
                .prepare_shielded_calls
                .lock()
                .expect("counter is available") += 1;
            let night = ChainAsset::new(
                request.asset_id,
                AssetSymbol::parse("NIGHT").expect("symbol is valid"),
                6,
            );
            WalletTransferPreview::new(
                WalletTransactionDraftId::parse("txdraft_shielded_test").expect("draft is valid"),
                WalletTransactionAuthorizationChallenge::parse("txauth_shielded_test")
                    .expect("challenge is valid"),
                ChainNetworkId::parse("undeployed").expect("network is valid"),
                ChainAccountId::parse("midnight_account_0_0").expect("account is valid"),
                request.recipient,
                AssetBalance::new(night.clone(), 1_000_000),
                AssetBalance::new(night, 4_000_000),
                None,
                WalletTransactionFeeState::RequiresBalancing,
                1,
                request.expires_at,
                WalletTransactionDraftState::Prepared,
            )
            .map_err(|_| WalletTransactionPortError::InvalidData)
        }

        fn authorize(
            &self,
            _: &WalletProfileId,
            request: AuthorizeWalletTransferRequest,
        ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
            assert_eq!(request.draft_id.as_str(), "txdraft_test");
            assert_eq!(request.authorization_challenge.as_str(), "txauth_test");
            *self.authorize_calls.lock().expect("counter is available") += 1;
            Ok(Self::preview(WalletTransactionDraftState::Authorized))
        }

        fn get(
            &self,
            _: &WalletProfileId,
            _: &WalletTransactionDraftId,
            _: UnixTimestampMillis,
        ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
            Ok(Self::preview(WalletTransactionDraftState::Prepared))
        }

        fn submit<'a>(
            &'a self,
            _: &'a WalletProfileId,
            request: SubmitWalletTransferRequest,
        ) -> WalletTransactionPortFuture<'a> {
            *self.submit_calls.lock().expect("counter is available") += 1;
            Box::pin(async move {
                let fee = AssetBalance::new(
                    ChainAsset::new(
                        ChainAssetId::parse("midnight:dust").expect("asset id is valid"),
                        AssetSymbol::parse("DUST").expect("symbol is valid"),
                        15,
                    ),
                    42,
                );
                let preview = Self::preview(WalletTransactionDraftState::Submitted)
                    .with_final_fee(fee.clone());
                Ok(SubmittedWalletTransfer {
                    preview,
                    submission: WalletTransferSubmission::new(
                        request.draft_id,
                        ChainTransactionId::parse("tx_submitted").expect("transaction id is valid"),
                        ChainBlockId::parse("block_submitted").expect("block id is valid"),
                        fee,
                        WalletTransferSubmissionMode::Simulated,
                    ),
                })
            })
        }

        fn submission_status(
            &self,
            _: &WalletProfileId,
            draft_id: &WalletTransactionDraftId,
        ) -> Result<WalletTransactionSubmissionStatus, WalletTransactionPortError> {
            Ok(WalletTransactionSubmissionStatus::new(
                draft_id.clone(),
                WalletTransactionSubmissionState::NotStarted,
                None,
            ))
        }

        fn cancel_submission(
            &self,
            _: &WalletProfileId,
            _: &WalletTransactionDraftId,
        ) -> Result<WalletTransactionSubmissionStatus, WalletTransactionPortError> {
            Err(WalletTransactionPortError::SubmissionNotInProgress)
        }

        fn submission_history(
            &self,
            _: &WalletProfileId,
        ) -> Result<Vec<WalletTransactionSubmissionStatus>, WalletTransactionPortError> {
            Ok(Vec::new())
        }

        fn reconcile_submission<'a>(
            &'a self,
            _: &'a WalletProfileId,
            _: &'a WalletTransactionDraftId,
        ) -> WalletTransactionStatusPortFuture<'a> {
            Box::pin(async { Err(WalletTransactionPortError::Unavailable) })
        }
    }

    fn service() -> WalletTransactionService<RecordingTransactions, FixedClock> {
        WalletTransactionService::new(
            Arc::new(RecordingTransactions::default()),
            Arc::new(FixedClock),
        )
    }

    fn confirmation(confirmed: bool) -> SensitiveOperationConfirmation {
        SensitiveOperationConfirmation {
            title: "Authorize NIGHT transfer".to_owned(),
            summary: "Send 1 NIGHT on Standalone; DUST fee balancing remains pending".to_owned(),
            confirmed,
        }
    }

    #[test]
    fn transaction_port_failures_have_stable_safe_messages() {
        let cases = [
            (
                WalletTransactionPortError::Unavailable,
                "wallet transaction capability is unavailable",
            ),
            (
                WalletTransactionPortError::ProtectionNotInitialized,
                "wallet protection is not initialized",
            ),
            (
                WalletTransactionPortError::ProtectionLocked,
                "wallet is locked",
            ),
            (
                WalletTransactionPortError::AccountNotDerived,
                "a protected wallet account must be derived first",
            ),
            (
                WalletTransactionPortError::AccountNotSynchronized,
                "wallet account must be synchronized first",
            ),
            (
                WalletTransactionPortError::ShieldedStateNotCurrent,
                "shielded wallet state must complete a live synchronization first",
            ),
            (
                WalletTransactionPortError::UnsupportedNetwork,
                "wallet network is not supported",
            ),
            (
                WalletTransactionPortError::InvalidRecipient,
                "transaction recipient is invalid",
            ),
            (
                WalletTransactionPortError::RecipientNetworkMismatch,
                "transaction recipient belongs to another network",
            ),
            (
                WalletTransactionPortError::InsufficientFunds,
                "wallet has insufficient funds",
            ),
            (
                WalletTransactionPortError::DraftNotFound,
                "transaction draft was not found",
            ),
            (
                WalletTransactionPortError::DraftExpired,
                "transaction draft has expired",
            ),
            (
                WalletTransactionPortError::DraftConflict,
                "transaction draft conflicts with current wallet state",
            ),
            (
                WalletTransactionPortError::SubmissionInProgress,
                "transaction submission is already in progress",
            ),
            (
                WalletTransactionPortError::SubmissionNotInProgress,
                "transaction submission is not in progress",
            ),
            (
                WalletTransactionPortError::SubmissionCancelled,
                "transaction submission was cancelled before broadcast",
            ),
            (
                WalletTransactionPortError::SubmissionCancellationUnsafe,
                "transaction submission can no longer be cancelled safely",
            ),
            (
                WalletTransactionPortError::AuthorizationChallengeMismatch,
                "transaction authorization does not match the prepared preview",
            ),
            (
                WalletTransactionPortError::InsufficientDust,
                "wallet has insufficient DUST for the transaction fee",
            ),
            (
                WalletTransactionPortError::InvalidChainState,
                "Midnight chain state is invalid or unavailable",
            ),
            (
                WalletTransactionPortError::ProvingFailed,
                "transaction proving failed",
            ),
            (
                WalletTransactionPortError::SubmissionRejected,
                "Midnight rejected the transaction submission",
            ),
            (
                WalletTransactionPortError::SubmissionOutcomeUnknown,
                "Midnight transaction submission outcome is not yet known",
            ),
            (
                WalletTransactionPortError::Timeout,
                "transaction operation timed out",
            ),
            (
                WalletTransactionPortError::InvalidData,
                "transaction adapter returned invalid data",
            ),
        ];

        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn prepare_maps_exact_amount_and_truthful_pending_work() {
        let result = PrepareWalletTransferUseCase::execute(
            &service(),
            PrepareWalletTransferCommand {
                profile_id: "profile_test".to_owned(),
                recipient_address: "mn_addr_undeployed1recipient".to_owned(),
                amount_atomic_units: "1000000".to_owned(),
            },
        )
        .expect("prepare succeeds");

        assert_eq!(result.amount.atomic_units, "1000000");
        assert_eq!(result.change.atomic_units, "4000000");
        assert_eq!(result.fee_state, "requires_balancing");
        assert!(result.proof_required);
        assert!(!result.submission_ready);
    }

    #[test]
    fn invalid_amount_is_rejected_before_the_adapter() {
        let service = service();
        assert_eq!(
            PrepareWalletTransferUseCase::execute(
                &service,
                PrepareWalletTransferCommand {
                    profile_id: "profile_test".to_owned(),
                    recipient_address: "mn_addr_undeployed1recipient".to_owned(),
                    amount_atomic_units: "1.5".to_owned(),
                },
            ),
            Err(WalletTransactionError::InvalidAmount)
        );
        assert_eq!(
            PrepareWalletTransferUseCase::execute(
                &service,
                PrepareWalletTransferCommand {
                    profile_id: "profile_test".to_owned(),
                    recipient_address: "mn_addr_undeployed1recipient".to_owned(),
                    amount_atomic_units: "0".to_owned(),
                },
            ),
            Err(WalletTransactionError::ZeroAmount)
        );
    }

    #[test]
    fn shielded_prepare_requires_canonical_token_type_and_forwards_exact_amount() {
        let transactions = Arc::new(RecordingTransactions::default());
        let service =
            WalletTransactionService::new(Arc::clone(&transactions), Arc::new(FixedClock));
        let recipient = "mn_shield-addr_undeployed1recipient".to_owned();
        assert_eq!(
            PrepareShieldedWalletTransferUseCase::execute(
                &service,
                PrepareShieldedWalletTransferCommand {
                    profile_id: "profile_test".to_owned(),
                    recipient_address: recipient.clone(),
                    token_type: "000000000000000000000000000000000000000000000000000000000000000A"
                        .to_owned(),
                    amount_atomic_units: "1000000".to_owned(),
                },
            ),
            Err(WalletTransactionError::InvalidTokenType)
        );
        assert_eq!(
            *transactions
                .prepare_shielded_calls
                .lock()
                .expect("counter is available"),
            0
        );

        let result = PrepareShieldedWalletTransferUseCase::execute(
            &service,
            PrepareShieldedWalletTransferCommand {
                profile_id: "profile_test".to_owned(),
                recipient_address: recipient,
                token_type: "0000000000000000000000000000000000000000000000000000000000000000"
                    .to_owned(),
                amount_atomic_units: "1000000".to_owned(),
            },
        )
        .expect("canonical shielded transfer prepares");
        assert_eq!(result.recipient_kind, "shielded");
        assert_eq!(result.amount.atomic_units, "1000000");
        assert_eq!(result.change.atomic_units, "4000000");
        assert_eq!(
            *transactions
                .prepare_shielded_calls
                .lock()
                .expect("counter is available"),
            1
        );
    }

    #[test]
    fn authorization_requires_confirmation_before_adapter_use() {
        let transactions = Arc::new(RecordingTransactions::default());
        let service =
            WalletTransactionService::new(Arc::clone(&transactions), Arc::new(FixedClock));
        let command = AuthorizeWalletTransferCommand {
            profile_id: "profile_test".to_owned(),
            draft_id: "txdraft_test".to_owned(),
            authorization_challenge: "txauth_test".to_owned(),
            confirmation: confirmation(false),
        };
        assert_eq!(
            AuthorizeWalletTransferUseCase::execute(&service, command),
            Err(WalletTransactionError::ConfirmationRequired)
        );
        assert_eq!(
            *transactions
                .authorize_calls
                .lock()
                .expect("counter is available"),
            0
        );
    }

    #[test]
    fn confirmed_authorization_returns_only_safe_status() {
        let service = service();
        let result = AuthorizeWalletTransferUseCase::execute(
            &service,
            AuthorizeWalletTransferCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "txdraft_test".to_owned(),
                authorization_challenge: "txauth_test".to_owned(),
                confirmation: confirmation(true),
            },
        )
        .expect("authorization succeeds");
        assert_eq!(result.state, "authorized");
        assert!(result.submission_ready);
    }

    #[test]
    fn submission_requires_confirmation_before_adapter_use() {
        let transactions = Arc::new(RecordingTransactions::default());
        let service =
            WalletTransactionService::new(Arc::clone(&transactions), Arc::new(FixedClock));
        let result = ready(SubmitWalletTransferUseCase::execute(
            &service,
            SubmitWalletTransferCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "txdraft_test".to_owned(),
                confirmation: confirmation(false),
            },
        ));

        assert_eq!(result, Err(WalletTransactionError::ConfirmationRequired));
        assert_eq!(
            *transactions
                .submit_calls
                .lock()
                .expect("counter is available"),
            0
        );
    }

    #[test]
    fn confirmed_submission_returns_only_public_inclusion_metadata() {
        let result = ready(SubmitWalletTransferUseCase::execute(
            &service(),
            SubmitWalletTransferCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "txdraft_test".to_owned(),
                confirmation: confirmation(true),
            },
        ))
        .expect("submission succeeds");

        assert_eq!(result.transfer.state, "submitted");
        assert!(!result.transfer.proof_required);
        assert!(!result.transfer.submission_ready);
        assert_eq!(result.transaction_id, "tx_submitted");
        assert_eq!(result.block_id, "block_submitted");
        assert_eq!(result.fee.asset_id, "midnight:dust");
        assert_eq!(result.fee.atomic_units, "42");
        assert_eq!(result.mode, "simulated");
    }

    #[test]
    fn submission_status_maps_safe_lifecycle_without_chain_material() {
        let status = GetWalletTransferSubmissionStatusUseCase::execute(
            &service(),
            WalletTransferSubmissionQuery {
                profile_id: "profile_test".to_owned(),
                draft_id: "txdraft_test".to_owned(),
            },
        )
        .expect("submission status is readable");

        assert_eq!(status.draft_id, "txdraft_test");
        assert_eq!(status.state, "not_started");
        assert!(status.retryable);
        assert!(!status.cancellation_allowed);
        assert!(status.transaction_id.is_none());
        assert!(status.fee.is_none());
    }

    #[test]
    fn recorded_submission_maps_public_broadcast_metadata_without_an_inclusion_block() {
        let fee = AssetBalance::new(
            ChainAsset::new(
                ChainAssetId::parse("midnight:dust").expect("asset id is valid"),
                AssetSymbol::parse("DUST").expect("symbol is valid"),
                15,
            ),
            42,
        );
        let status = WalletTransactionSubmissionStatus::recorded(
            WalletTransactionDraftId::parse("txdraft_test").expect("draft id is valid"),
            WalletTransactionSubmissionState::OutcomeUnknown,
            ChainTransactionId::parse("tx_recorded").expect("transaction id is valid"),
            fee,
            WalletTransferSubmissionMode::Live,
        );
        let view = WalletTransferSubmissionStatusView::from(&status);

        assert_eq!(view.transaction_id.as_deref(), Some("tx_recorded"));
        assert!(view.block_id.is_none());
        assert_eq!(
            view.fee.as_ref().map(|value| value.atomic_units.as_str()),
            Some("42")
        );
        assert_eq!(view.mode.as_deref(), Some("live"));
        assert!(view.reconciliation_allowed);
        assert!(!view.replacement_allowed);
    }
}
