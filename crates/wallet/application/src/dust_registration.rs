// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use oxid_foundation::{OpaqueIdError, UnixTimestampMillis};
use oxid_platform_ports::{ClockPort, PlatformError};
use oxid_wallet_domain::{
    AssetBalance, WalletDustReadiness, WalletDustRegistrationObservation,
    WalletDustRegistrationPreview, WalletDustRegistrationSubmission,
    WalletDustRegistrationSubmissionStatus, WalletProfileId,
    WalletTransactionAuthorizationChallenge, WalletTransactionDraftId, WalletTransactionDraftState,
    WalletTransactionFeeState, WalletTransactionSubmissionState, WalletTransferSubmissionMode,
};

use crate::{SensitiveOperationConfirmation, SensitiveWalletOperationError, validate_confirmation};

/// Lifetime of a prepared DUST registration before its retained material expires.
pub const WALLET_DUST_REGISTRATION_DRAFT_TTL_MILLIS: u64 = 60 * 60 * 1_000;

/// Safe failures returned by a DUST-registration adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationPortError {
    Unavailable,
    ProtectionNotInitialized,
    ProtectionLocked,
    AccountNotDerived,
    AccountNotSynchronized,
    NoEligibleNight,
    RegistrationAlreadyCurrent,
    InsufficientRegistrationAllowance,
    DraftNotFound,
    DraftExpired,
    DraftConflict,
    SubmissionInProgress,
    SubmissionNotInProgress,
    SubmissionCancellationUnsafe,
    AuthorizationChallengeMismatch,
    InvalidChainState,
    ProvingFailed,
    SubmissionRejected,
    SubmissionOutcomeUnknown,
    Timeout,
    InvalidData,
}

impl fmt::Display for WalletDustRegistrationPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unavailable => "wallet DUST registration capability is unavailable",
            Self::ProtectionNotInitialized => "wallet protection is not initialized",
            Self::ProtectionLocked => "wallet is locked",
            Self::AccountNotDerived => "a protected wallet account must be derived first",
            Self::AccountNotSynchronized => "wallet account must be synchronized first",
            Self::NoEligibleNight => "wallet has no unregistered NIGHT available",
            Self::RegistrationAlreadyCurrent => {
                "wallet NIGHT is already registered for DUST generation"
            }
            Self::InsufficientRegistrationAllowance => {
                "generated DUST cannot yet cover the registration fee"
            }
            Self::DraftNotFound => "DUST registration draft was not found",
            Self::DraftExpired => "DUST registration draft has expired",
            Self::DraftConflict => "DUST registration draft conflicts with current wallet state",
            Self::SubmissionInProgress => "DUST registration submission is already in progress",
            Self::SubmissionNotInProgress => "DUST registration submission is not in progress",
            Self::SubmissionCancellationUnsafe => {
                "DUST registration submission can no longer be cancelled safely"
            }
            Self::AuthorizationChallengeMismatch => {
                "DUST registration authorization does not match the prepared preview"
            }
            Self::InvalidChainState => "Midnight chain state is invalid or unavailable",
            Self::ProvingFailed => "DUST registration proving failed",
            Self::SubmissionRejected => "Midnight rejected the DUST registration",
            Self::SubmissionOutcomeUnknown => "Midnight DUST registration outcome is not yet known",
            Self::Timeout => "DUST registration operation timed out",
            Self::InvalidData => "DUST registration adapter returned invalid data",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletDustRegistrationPortError {}

/// Adapter-neutral request for preparing the current account's eligible NIGHT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareWalletDustRegistrationRequest {
    pub expires_at: UnixTimestampMillis,
}

/// Adapter-neutral request for authorizing one retained registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizeWalletDustRegistrationRequest {
    pub draft_id: WalletTransactionDraftId,
    pub authorization_challenge: WalletTransactionAuthorizationChallenge,
    pub now: UnixTimestampMillis,
}

/// Adapter-neutral request for proving and submitting one authorized registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitWalletDustRegistrationRequest {
    pub draft_id: WalletTransactionDraftId,
    pub now: UnixTimestampMillis,
}

/// Adapter result combining the safe final preview and registration inclusion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmittedWalletDustRegistration {
    pub preview: WalletDustRegistrationPreview,
    pub submission: WalletDustRegistrationSubmission,
}

/// Asynchronous result returned by the registration adapter.
pub type WalletDustRegistrationPortFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<SubmittedWalletDustRegistration, WalletDustRegistrationPortError>,
            > + Send
            + 'a,
    >,
>;

/// Asynchronous result returned by registration reconciliation.
pub type WalletDustRegistrationStatusPortFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    WalletDustRegistrationSubmissionStatus,
                    WalletDustRegistrationPortError,
                >,
            > + Send
            + 'a,
    >,
>;

/// Focused outgoing port retaining every chain-specific registration artifact.
pub trait WalletDustRegistrationPort: Send + Sync {
    fn prepare(
        &self,
        profile_id: &WalletProfileId,
        request: PrepareWalletDustRegistrationRequest,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError>;

    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        request: AuthorizeWalletDustRegistrationRequest,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError>;

    fn submit<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        request: SubmitWalletDustRegistrationRequest,
    ) -> WalletDustRegistrationPortFuture<'a>;

    fn get(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
        now: UnixTimestampMillis,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError>;

    /// Reads the adapter's durable public registration state without asserting
    /// DUST spendability.
    fn status(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError>;

    /// Signals safe pre-broadcast cancellation and returns the bounded state.
    fn cancel_submission(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError>;

    /// Reconciles one durable post-broadcast registration against finality.
    fn reconcile_submission<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        draft_id: &'a WalletTransactionDraftId,
    ) -> WalletDustRegistrationStatusPortFuture<'a>;
}

/// Fail-closed registration port for compositions without a native Midnight
/// transaction/proving stack (for example the browser-only WASM shell).
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableWalletDustRegistrationPort;

impl WalletDustRegistrationPort for UnavailableWalletDustRegistrationPort {
    fn prepare(
        &self,
        _: &WalletProfileId,
        _: PrepareWalletDustRegistrationRequest,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
        Err(WalletDustRegistrationPortError::Unavailable)
    }

    fn authorize(
        &self,
        _: &WalletProfileId,
        _: AuthorizeWalletDustRegistrationRequest,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
        Err(WalletDustRegistrationPortError::Unavailable)
    }

    fn submit<'a>(
        &'a self,
        _: &'a WalletProfileId,
        _: SubmitWalletDustRegistrationRequest,
    ) -> WalletDustRegistrationPortFuture<'a> {
        Box::pin(async { Err(WalletDustRegistrationPortError::Unavailable) })
    }

    fn get(
        &self,
        _: &WalletProfileId,
        _: &WalletTransactionDraftId,
        _: UnixTimestampMillis,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
        Err(WalletDustRegistrationPortError::Unavailable)
    }

    fn status(
        &self,
        _: &WalletProfileId,
        _: &WalletTransactionDraftId,
    ) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError> {
        Err(WalletDustRegistrationPortError::Unavailable)
    }

    fn cancel_submission(
        &self,
        _: &WalletProfileId,
        _: &WalletTransactionDraftId,
    ) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError> {
        Err(WalletDustRegistrationPortError::Unavailable)
    }

    fn reconcile_submission<'a>(
        &'a self,
        _: &'a WalletProfileId,
        _: &'a WalletTransactionDraftId,
    ) -> WalletDustRegistrationStatusPortFuture<'a> {
        Box::pin(async { Err(WalletDustRegistrationPortError::Unavailable) })
    }
}

/// Incoming request to prepare registration of the active account's eligible NIGHT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareWalletDustRegistrationCommand {
    pub profile_id: String,
}

/// Incoming request to authorize one exact registration preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizeWalletDustRegistrationCommand {
    pub profile_id: String,
    pub draft_id: String,
    pub authorization_challenge: String,
    pub confirmation: SensitiveOperationConfirmation,
}

/// Incoming request to prove and submit an authorized registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitWalletDustRegistrationCommand {
    pub profile_id: String,
    pub draft_id: String,
    pub confirmation: SensitiveOperationConfirmation,
}

/// Incoming query for one safe retained registration preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetWalletDustRegistrationCommand {
    pub profile_id: String,
    pub draft_id: String,
}

/// Incoming query for one registration submission status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetWalletDustRegistrationStatusCommand {
    pub profile_id: String,
    pub draft_id: String,
}

/// Incoming request for cooperative pre-broadcast cancellation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancelWalletDustRegistrationSubmissionCommand {
    pub profile_id: String,
    pub draft_id: String,
}

/// Incoming request to reconcile a durable post-broadcast registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileWalletDustRegistrationSubmissionCommand {
    pub profile_id: String,
    pub draft_id: String,
}

/// Exact public asset value without floating-point conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationAssetView {
    pub asset_id: String,
    pub symbol: String,
    pub decimals: u8,
    pub atomic_units: String,
}

/// Safe registration preview; all protected and chain-native material is absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationPreviewView {
    pub draft_id: String,
    pub authorization_challenge: String,
    pub network_id: String,
    pub account_id: String,
    pub registered_night: WalletDustRegistrationAssetView,
    pub input_count: u16,
    pub maximum_fee_allowance: WalletDustRegistrationAssetView,
    pub fee_state: String,
    pub expires_at_millis: u64,
    pub state: String,
    pub authorization_ready: bool,
    pub submission_ready: bool,
}

impl From<&WalletDustRegistrationPreview> for WalletDustRegistrationPreviewView {
    fn from(preview: &WalletDustRegistrationPreview) -> Self {
        Self {
            draft_id: preview.draft_id().as_str().to_owned(),
            authorization_challenge: preview.authorization_challenge().as_str().to_owned(),
            network_id: preview.network_id().as_str().to_owned(),
            account_id: preview.account_id().as_str().to_owned(),
            registered_night: asset_view(preview.registered_night()),
            input_count: preview.input_count(),
            maximum_fee_allowance: asset_view(preview.maximum_fee_allowance()),
            fee_state: fee_state_name(preview.fee_state()).to_owned(),
            expires_at_millis: preview.expires_at().value(),
            state: draft_state_name(preview.state()).to_owned(),
            authorization_ready: matches!(preview.state(), WalletTransactionDraftState::Prepared),
            submission_ready: matches!(preview.state(), WalletTransactionDraftState::Authorized),
        }
    }
}

/// Public included registration result.
///
/// Inclusion requires a later DUST synchronization and is never presented as
/// proof that generated DUST is already spendable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationSubmissionView {
    pub registration: WalletDustRegistrationPreviewView,
    pub transaction_id: String,
    pub block_id: String,
    pub fee: WalletDustRegistrationAssetView,
    pub mode: String,
    pub registration_observation: String,
    pub dust_readiness: String,
}

impl From<&SubmittedWalletDustRegistration> for WalletDustRegistrationSubmissionView {
    fn from(value: &SubmittedWalletDustRegistration) -> Self {
        Self {
            registration: WalletDustRegistrationPreviewView::from(&value.preview),
            transaction_id: value.submission.transaction_id().as_str().to_owned(),
            block_id: value.submission.block_id().as_str().to_owned(),
            fee: asset_view(value.submission.fee()),
            mode: submission_mode_name(value.submission.mode()).to_owned(),
            registration_observation: registration_observation_name(
                WalletDustRegistrationObservation::Included,
            )
            .to_owned(),
            dust_readiness: dust_readiness_name(WalletDustReadiness::RequiresSynchronization)
                .to_owned(),
        }
    }
}

/// Safe registration-specific submission status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationSubmissionStatusView {
    pub draft_id: String,
    pub state: String,
    pub transaction_id: Option<String>,
    pub block_id: Option<String>,
    pub fee: Option<WalletDustRegistrationAssetView>,
    pub mode: Option<String>,
    pub registration_observation: String,
    pub dust_readiness: String,
    pub cancellation_allowed: bool,
    pub reconciliation_allowed: bool,
}

impl From<&WalletDustRegistrationSubmissionStatus> for WalletDustRegistrationSubmissionStatusView {
    fn from(status: &WalletDustRegistrationSubmissionStatus) -> Self {
        Self {
            draft_id: status.draft_id().as_str().to_owned(),
            state: submission_state_name(status.state()).to_owned(),
            transaction_id: status
                .transaction_id()
                .map(|value| value.as_str().to_owned()),
            block_id: status.block_id().map(|value| value.as_str().to_owned()),
            fee: status.fee().map(asset_view),
            mode: status
                .mode()
                .map(|value| submission_mode_name(value).to_owned()),
            registration_observation: registration_observation_name(
                status.registration_observation(),
            )
            .to_owned(),
            dust_readiness: dust_readiness_name(status.dust_readiness()).to_owned(),
            cancellation_allowed: status.cancellation_allowed(),
            reconciliation_allowed: status.reconciliation_allowed(),
        }
    }
}

/// Incoming use case for preparing one retained DUST registration.
pub trait PrepareWalletDustRegistrationUseCase: Send + Sync {
    fn execute(
        &self,
        command: PrepareWalletDustRegistrationCommand,
    ) -> Result<WalletDustRegistrationPreviewView, WalletDustRegistrationError>;
}

/// Incoming use case for authorizing the exact registration preview.
pub trait AuthorizeWalletDustRegistrationUseCase: Send + Sync {
    fn execute(
        &self,
        command: AuthorizeWalletDustRegistrationCommand,
    ) -> Result<WalletDustRegistrationPreviewView, WalletDustRegistrationError>;
}

/// Incoming use case for proving and submitting an authorized registration.
pub trait SubmitWalletDustRegistrationUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: SubmitWalletDustRegistrationCommand,
    ) -> WalletDustRegistrationSubmissionViewFuture<'a>;
}

/// Incoming use case for reading a retained registration preview.
pub trait GetWalletDustRegistrationUseCase: Send + Sync {
    fn execute(
        &self,
        command: GetWalletDustRegistrationCommand,
    ) -> Result<WalletDustRegistrationPreviewView, WalletDustRegistrationError>;
}

/// Incoming use case for reading registration-specific submission status.
pub trait GetWalletDustRegistrationStatusUseCase: Send + Sync {
    fn execute(
        &self,
        command: GetWalletDustRegistrationStatusCommand,
    ) -> Result<WalletDustRegistrationSubmissionStatusView, WalletDustRegistrationError>;
}

/// Incoming use case for requesting cooperative pre-broadcast cancellation.
pub trait CancelWalletDustRegistrationSubmissionUseCase: Send + Sync {
    fn execute(
        &self,
        command: CancelWalletDustRegistrationSubmissionCommand,
    ) -> Result<WalletDustRegistrationSubmissionStatusView, WalletDustRegistrationError>;
}

/// Incoming use case for reconciling a durable registration submission.
pub trait ReconcileWalletDustRegistrationSubmissionUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: ReconcileWalletDustRegistrationSubmissionCommand,
    ) -> WalletDustRegistrationStatusViewFuture<'a>;
}

/// Asynchronous public registration result returned to incoming adapters.
pub type WalletDustRegistrationSubmissionViewFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<WalletDustRegistrationSubmissionView, WalletDustRegistrationError>,
            > + Send
            + 'a,
    >,
>;

/// Asynchronous public registration status returned by reconciliation.
pub type WalletDustRegistrationStatusViewFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    WalletDustRegistrationSubmissionStatusView,
                    WalletDustRegistrationError,
                >,
            > + Send
            + 'a,
    >,
>;

/// Stable registration failures exposed by the application boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidDraftIdentifier(OpaqueIdError),
    InvalidAuthorizationChallenge(OpaqueIdError),
    ConfirmationRequired,
    InvalidConfirmation,
    Clock(PlatformError),
    Operation(WalletDustRegistrationPortError),
}

impl fmt::Display for WalletDustRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error)
            | Self::InvalidDraftIdentifier(error)
            | Self::InvalidAuthorizationChallenge(error) => error.fmt(formatter),
            Self::ConfirmationRequired => formatter.write_str("explicit confirmation is required"),
            Self::InvalidConfirmation => formatter.write_str("confirmation intent is invalid"),
            Self::Clock(error) => error.fmt(formatter),
            Self::Operation(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletDustRegistrationError {}

/// Application service for the distinct DUST-registration lifecycle.
pub struct WalletDustRegistrationService<T, C> {
    registrations: Arc<T>,
    clock: Arc<C>,
}

impl<T, C> WalletDustRegistrationService<T, C> {
    #[must_use]
    pub const fn new(registrations: Arc<T>, clock: Arc<C>) -> Self {
        Self {
            registrations,
            clock,
        }
    }

    fn now(&self) -> Result<UnixTimestampMillis, WalletDustRegistrationError>
    where
        C: ClockPort,
    {
        self.clock.now().map_err(WalletDustRegistrationError::Clock)
    }
}

impl<T, C> PrepareWalletDustRegistrationUseCase for WalletDustRegistrationService<T, C>
where
    T: WalletDustRegistrationPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        command: PrepareWalletDustRegistrationCommand,
    ) -> Result<WalletDustRegistrationPreviewView, WalletDustRegistrationError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletDustRegistrationError::InvalidProfileIdentifier)?;
        let expires_at = self
            .now()?
            .value()
            .checked_add(WALLET_DUST_REGISTRATION_DRAFT_TTL_MILLIS)
            .map(UnixTimestampMillis::new)
            .ok_or(WalletDustRegistrationError::Clock(
                PlatformError::ClockUnavailable,
            ))?;
        let preview = self
            .registrations
            .prepare(
                &profile_id,
                PrepareWalletDustRegistrationRequest { expires_at },
            )
            .map_err(WalletDustRegistrationError::Operation)?;
        Ok(WalletDustRegistrationPreviewView::from(&preview))
    }
}

impl<T, C> AuthorizeWalletDustRegistrationUseCase for WalletDustRegistrationService<T, C>
where
    T: WalletDustRegistrationPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        command: AuthorizeWalletDustRegistrationCommand,
    ) -> Result<WalletDustRegistrationPreviewView, WalletDustRegistrationError> {
        validate_confirmation(&command.confirmation).map_err(map_confirmation_error)?;
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletDustRegistrationError::InvalidProfileIdentifier)?;
        let draft_id = WalletTransactionDraftId::parse(command.draft_id)
            .map_err(WalletDustRegistrationError::InvalidDraftIdentifier)?;
        let authorization_challenge =
            WalletTransactionAuthorizationChallenge::parse(command.authorization_challenge)
                .map_err(WalletDustRegistrationError::InvalidAuthorizationChallenge)?;
        let preview = self
            .registrations
            .authorize(
                &profile_id,
                AuthorizeWalletDustRegistrationRequest {
                    draft_id,
                    authorization_challenge,
                    now: self.now()?,
                },
            )
            .map_err(WalletDustRegistrationError::Operation)?;
        Ok(WalletDustRegistrationPreviewView::from(&preview))
    }
}

impl<T, C> SubmitWalletDustRegistrationUseCase for WalletDustRegistrationService<T, C>
where
    T: WalletDustRegistrationPort + 'static,
    C: ClockPort + 'static,
{
    fn execute<'a>(
        &'a self,
        command: SubmitWalletDustRegistrationCommand,
    ) -> WalletDustRegistrationSubmissionViewFuture<'a> {
        Box::pin(async move {
            validate_confirmation(&command.confirmation).map_err(map_confirmation_error)?;
            let profile_id = WalletProfileId::parse(command.profile_id)
                .map_err(WalletDustRegistrationError::InvalidProfileIdentifier)?;
            let draft_id = WalletTransactionDraftId::parse(command.draft_id)
                .map_err(WalletDustRegistrationError::InvalidDraftIdentifier)?;
            let submitted = self
                .registrations
                .submit(
                    &profile_id,
                    SubmitWalletDustRegistrationRequest {
                        draft_id,
                        now: self.now()?,
                    },
                )
                .await
                .map_err(WalletDustRegistrationError::Operation)?;
            Ok(WalletDustRegistrationSubmissionView::from(&submitted))
        })
    }
}

impl<T, C> GetWalletDustRegistrationUseCase for WalletDustRegistrationService<T, C>
where
    T: WalletDustRegistrationPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        command: GetWalletDustRegistrationCommand,
    ) -> Result<WalletDustRegistrationPreviewView, WalletDustRegistrationError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletDustRegistrationError::InvalidProfileIdentifier)?;
        let draft_id = WalletTransactionDraftId::parse(command.draft_id)
            .map_err(WalletDustRegistrationError::InvalidDraftIdentifier)?;
        let preview = self
            .registrations
            .get(&profile_id, &draft_id, self.now()?)
            .map_err(WalletDustRegistrationError::Operation)?;
        Ok(WalletDustRegistrationPreviewView::from(&preview))
    }
}

impl<T, C> GetWalletDustRegistrationStatusUseCase for WalletDustRegistrationService<T, C>
where
    T: WalletDustRegistrationPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        command: GetWalletDustRegistrationStatusCommand,
    ) -> Result<WalletDustRegistrationSubmissionStatusView, WalletDustRegistrationError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletDustRegistrationError::InvalidProfileIdentifier)?;
        let draft_id = WalletTransactionDraftId::parse(command.draft_id)
            .map_err(WalletDustRegistrationError::InvalidDraftIdentifier)?;
        let status = self
            .registrations
            .status(&profile_id, &draft_id)
            .map_err(WalletDustRegistrationError::Operation)?;
        Ok(WalletDustRegistrationSubmissionStatusView::from(&status))
    }
}

impl<T, C> CancelWalletDustRegistrationSubmissionUseCase for WalletDustRegistrationService<T, C>
where
    T: WalletDustRegistrationPort + 'static,
    C: ClockPort + 'static,
{
    fn execute(
        &self,
        command: CancelWalletDustRegistrationSubmissionCommand,
    ) -> Result<WalletDustRegistrationSubmissionStatusView, WalletDustRegistrationError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletDustRegistrationError::InvalidProfileIdentifier)?;
        let draft_id = WalletTransactionDraftId::parse(command.draft_id)
            .map_err(WalletDustRegistrationError::InvalidDraftIdentifier)?;
        let status = self
            .registrations
            .cancel_submission(&profile_id, &draft_id)
            .map_err(WalletDustRegistrationError::Operation)?;
        Ok(WalletDustRegistrationSubmissionStatusView::from(&status))
    }
}

impl<T, C> ReconcileWalletDustRegistrationSubmissionUseCase for WalletDustRegistrationService<T, C>
where
    T: WalletDustRegistrationPort + 'static,
    C: ClockPort + 'static,
{
    fn execute<'a>(
        &'a self,
        command: ReconcileWalletDustRegistrationSubmissionCommand,
    ) -> WalletDustRegistrationStatusViewFuture<'a> {
        Box::pin(async move {
            let profile_id = WalletProfileId::parse(command.profile_id)
                .map_err(WalletDustRegistrationError::InvalidProfileIdentifier)?;
            let draft_id = WalletTransactionDraftId::parse(command.draft_id)
                .map_err(WalletDustRegistrationError::InvalidDraftIdentifier)?;
            let status = self
                .registrations
                .reconcile_submission(&profile_id, &draft_id)
                .await
                .map_err(WalletDustRegistrationError::Operation)?;
            Ok(WalletDustRegistrationSubmissionStatusView::from(&status))
        })
    }
}

fn asset_view(balance: &AssetBalance) -> WalletDustRegistrationAssetView {
    WalletDustRegistrationAssetView {
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

const fn draft_state_name(state: WalletTransactionDraftState) -> &'static str {
    match state {
        WalletTransactionDraftState::Prepared => "prepared",
        WalletTransactionDraftState::Authorized => "authorized",
        WalletTransactionDraftState::Submitting => "submitting",
        WalletTransactionDraftState::Submitted => "submitted",
        WalletTransactionDraftState::Expired => "expired",
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

const fn submission_mode_name(mode: WalletTransferSubmissionMode) -> &'static str {
    match mode {
        WalletTransferSubmissionMode::Simulated => "simulated",
        WalletTransferSubmissionMode::Live => "live",
    }
}

const fn registration_observation_name(
    observation: WalletDustRegistrationObservation,
) -> &'static str {
    match observation {
        WalletDustRegistrationObservation::NotObserved => "not_observed",
        WalletDustRegistrationObservation::Included => "included",
    }
}

const fn dust_readiness_name(readiness: WalletDustReadiness) -> &'static str {
    match readiness {
        WalletDustReadiness::NotEstablished => "not_established",
        WalletDustReadiness::RequiresSynchronization => "requires_synchronization",
    }
}

const fn map_confirmation_error(
    error: SensitiveWalletOperationError,
) -> WalletDustRegistrationError {
    match error {
        SensitiveWalletOperationError::ConfirmationRequired => {
            WalletDustRegistrationError::ConfirmationRequired
        }
        SensitiveWalletOperationError::InvalidConfirmation => {
            WalletDustRegistrationError::InvalidConfirmation
        }
        SensitiveWalletOperationError::InvalidProfileIdentifier(_)
        | SensitiveWalletOperationError::InvalidKeyReference(_)
        | SensitiveWalletOperationError::EmptyPayload
        | SensitiveWalletOperationError::PayloadTooLarge
        | SensitiveWalletOperationError::Operation(_) => {
            WalletDustRegistrationError::InvalidConfirmation
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
        ChainTransactionId,
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

    fn asset(id: &str, symbol: &str, decimals: u8, atomic_units: u128) -> AssetBalance {
        AssetBalance::new(
            ChainAsset::new(
                ChainAssetId::parse(id).expect("asset id is valid"),
                AssetSymbol::parse(symbol).expect("asset symbol is valid"),
                decimals,
            ),
            atomic_units,
        )
    }

    fn night(atomic_units: u128) -> AssetBalance {
        asset("midnight:night", "NIGHT", 6, atomic_units)
    }

    fn dust(atomic_units: u128) -> AssetBalance {
        asset("midnight:dust", "DUST", 15, atomic_units)
    }

    #[derive(Default)]
    struct RecordingRegistrations {
        prepare_calls: Mutex<usize>,
        authorize_calls: Mutex<usize>,
        submit_calls: Mutex<usize>,
    }

    impl RecordingRegistrations {
        fn preview(state: WalletTransactionDraftState) -> WalletDustRegistrationPreview {
            WalletDustRegistrationPreview::new(
                WalletTransactionDraftId::parse("dustreg_test").expect("draft is valid"),
                WalletTransactionAuthorizationChallenge::parse("dustauth_test")
                    .expect("challenge is valid"),
                ChainNetworkId::parse("undeployed").expect("network is valid"),
                ChainAccountId::parse("midnight_account_0_0").expect("account is valid"),
                night(5_000_000),
                2,
                dust(100),
                WalletTransactionFeeState::RequiresBalancing,
                UnixTimestampMillis::new(1_700_003_600_000),
                state,
            )
            .expect("preview is valid")
        }

        fn submission() -> WalletDustRegistrationSubmission {
            WalletDustRegistrationSubmission::new(
                WalletTransactionDraftId::parse("dustreg_test").expect("draft is valid"),
                ChainTransactionId::parse("tx_registration").expect("transaction is valid"),
                ChainBlockId::parse("block_registration").expect("block is valid"),
                dust(42),
                WalletTransferSubmissionMode::Live,
            )
            .expect("submission is valid")
        }
    }

    impl WalletDustRegistrationPort for RecordingRegistrations {
        fn prepare(
            &self,
            _: &WalletProfileId,
            request: PrepareWalletDustRegistrationRequest,
        ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
            assert_eq!(request.expires_at.value(), 1_700_003_600_000);
            *self.prepare_calls.lock().expect("counter is available") += 1;
            Ok(Self::preview(WalletTransactionDraftState::Prepared))
        }

        fn authorize(
            &self,
            _: &WalletProfileId,
            request: AuthorizeWalletDustRegistrationRequest,
        ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
            assert_eq!(request.draft_id.as_str(), "dustreg_test");
            assert_eq!(request.authorization_challenge.as_str(), "dustauth_test");
            assert_eq!(request.now.value(), 1_700_000_000_000);
            *self.authorize_calls.lock().expect("counter is available") += 1;
            Ok(Self::preview(WalletTransactionDraftState::Authorized))
        }

        fn submit<'a>(
            &'a self,
            _: &'a WalletProfileId,
            request: SubmitWalletDustRegistrationRequest,
        ) -> WalletDustRegistrationPortFuture<'a> {
            assert_eq!(request.draft_id.as_str(), "dustreg_test");
            assert_eq!(request.now.value(), 1_700_000_000_000);
            *self.submit_calls.lock().expect("counter is available") += 1;
            Box::pin(async {
                Ok(SubmittedWalletDustRegistration {
                    preview: Self::preview(WalletTransactionDraftState::Submitted)
                        .with_fee_state(WalletTransactionFeeState::Final),
                    submission: Self::submission(),
                })
            })
        }

        fn get(
            &self,
            _: &WalletProfileId,
            _: &WalletTransactionDraftId,
            _: UnixTimestampMillis,
        ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
            Ok(Self::preview(WalletTransactionDraftState::Prepared))
        }

        fn status(
            &self,
            _: &WalletProfileId,
            _: &WalletTransactionDraftId,
        ) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError>
        {
            WalletDustRegistrationSubmissionStatus::included(Self::submission())
                .map_err(|_| WalletDustRegistrationPortError::InvalidData)
        }

        fn cancel_submission(
            &self,
            _: &WalletProfileId,
            draft_id: &WalletTransactionDraftId,
        ) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError>
        {
            WalletDustRegistrationSubmissionStatus::pending(
                draft_id.clone(),
                WalletTransactionSubmissionState::CancellationRequested,
            )
            .map_err(|_| WalletDustRegistrationPortError::InvalidData)
        }

        fn reconcile_submission<'a>(
            &'a self,
            _: &'a WalletProfileId,
            _: &'a WalletTransactionDraftId,
        ) -> WalletDustRegistrationStatusPortFuture<'a> {
            Box::pin(async {
                WalletDustRegistrationSubmissionStatus::included(Self::submission())
                    .map_err(|_| WalletDustRegistrationPortError::InvalidData)
            })
        }
    }

    fn service() -> WalletDustRegistrationService<RecordingRegistrations, FixedClock> {
        WalletDustRegistrationService::new(
            Arc::new(RecordingRegistrations::default()),
            Arc::new(FixedClock),
        )
    }

    fn confirmation(confirmed: bool) -> SensitiveOperationConfirmation {
        SensitiveOperationConfirmation {
            title: "Authorize DUST registration".to_owned(),
            summary: "Register 5 NIGHT and permit up to 100 atomic DUST for fees".to_owned(),
            confirmed,
        }
    }

    #[test]
    fn prepare_exposes_only_the_exact_aggregate_registration_plan() {
        let result = PrepareWalletDustRegistrationUseCase::execute(
            &service(),
            PrepareWalletDustRegistrationCommand {
                profile_id: "profile_test".to_owned(),
            },
        )
        .expect("prepare succeeds");

        assert_eq!(result.registered_night.atomic_units, "5000000");
        assert_eq!(result.maximum_fee_allowance.atomic_units, "100");
        assert_eq!(result.input_count, 2);
        assert_eq!(result.fee_state, "requires_balancing");
        assert!(result.authorization_ready);
        assert!(!result.submission_ready);
    }

    #[test]
    fn authorization_requires_explicit_confirmation_before_adapter_use() {
        let registrations = Arc::new(RecordingRegistrations::default());
        let service =
            WalletDustRegistrationService::new(Arc::clone(&registrations), Arc::new(FixedClock));
        let command = AuthorizeWalletDustRegistrationCommand {
            profile_id: "profile_test".to_owned(),
            draft_id: "dustreg_test".to_owned(),
            authorization_challenge: "dustauth_test".to_owned(),
            confirmation: confirmation(false),
        };

        assert_eq!(
            AuthorizeWalletDustRegistrationUseCase::execute(&service, command),
            Err(WalletDustRegistrationError::ConfirmationRequired)
        );
        assert_eq!(
            *registrations
                .authorize_calls
                .lock()
                .expect("counter is available"),
            0
        );
    }

    #[test]
    fn submission_requires_separate_confirmation_and_never_claims_spendability() {
        let registrations = Arc::new(RecordingRegistrations::default());
        let service =
            WalletDustRegistrationService::new(Arc::clone(&registrations), Arc::new(FixedClock));
        let rejected = ready(SubmitWalletDustRegistrationUseCase::execute(
            &service,
            SubmitWalletDustRegistrationCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "dustreg_test".to_owned(),
                confirmation: confirmation(false),
            },
        ));
        assert_eq!(
            rejected,
            Err(WalletDustRegistrationError::ConfirmationRequired)
        );
        assert_eq!(
            *registrations
                .submit_calls
                .lock()
                .expect("counter is available"),
            0
        );

        let submitted = ready(SubmitWalletDustRegistrationUseCase::execute(
            &service,
            SubmitWalletDustRegistrationCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "dustreg_test".to_owned(),
                confirmation: confirmation(true),
            },
        ))
        .expect("submission succeeds");
        assert_eq!(submitted.registration_observation, "included");
        assert_eq!(submitted.dust_readiness, "requires_synchronization");
        assert_eq!(submitted.fee.atomic_units, "42");
    }

    #[test]
    fn status_keeps_registration_observation_separate_from_dust_readiness() {
        let status = GetWalletDustRegistrationStatusUseCase::execute(
            &service(),
            GetWalletDustRegistrationStatusCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "dustreg_test".to_owned(),
            },
        )
        .expect("status succeeds");

        assert_eq!(status.state, "included");
        assert_eq!(status.registration_observation, "included");
        assert_eq!(status.dust_readiness, "requires_synchronization");
        assert!(!status.cancellation_allowed);
        assert!(!status.reconciliation_allowed);
    }

    #[test]
    fn cancellation_and_reconciliation_use_registration_specific_status() {
        let cancelled = CancelWalletDustRegistrationSubmissionUseCase::execute(
            &service(),
            CancelWalletDustRegistrationSubmissionCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "dustreg_test".to_owned(),
            },
        )
        .expect("cancellation signal succeeds");
        assert_eq!(cancelled.state, "cancellation_requested");
        assert_eq!(cancelled.registration_observation, "not_observed");
        assert_eq!(cancelled.dust_readiness, "not_established");

        let reconciled = ready(ReconcileWalletDustRegistrationSubmissionUseCase::execute(
            &service(),
            ReconcileWalletDustRegistrationSubmissionCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "dustreg_test".to_owned(),
            },
        ))
        .expect("reconciliation succeeds");
        assert_eq!(reconciled.state, "included");
        assert_eq!(reconciled.registration_observation, "included");
        assert_eq!(reconciled.dust_readiness, "requires_synchronization");
    }

    #[test]
    fn public_views_do_not_carry_adapter_private_registration_material() {
        let sentinel = "private-dust-key-signature-proof-transaction-bytes";
        let preview = PrepareWalletDustRegistrationUseCase::execute(
            &service(),
            PrepareWalletDustRegistrationCommand {
                profile_id: "profile_test".to_owned(),
            },
        )
        .expect("prepare succeeds");
        let submitted = ready(SubmitWalletDustRegistrationUseCase::execute(
            &service(),
            SubmitWalletDustRegistrationCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "dustreg_test".to_owned(),
                confirmation: confirmation(true),
            },
        ))
        .expect("submit succeeds");

        assert!(!format!("{preview:?}").contains(sentinel));
        assert!(!format!("{submitted:?}").contains(sentinel));
        assert!(!format!("{preview:?}").contains("key_reference"));
        assert!(!format!("{submitted:?}").contains("signature"));
        assert!(!format!("{submitted:?}").contains("proof"));
    }
}
