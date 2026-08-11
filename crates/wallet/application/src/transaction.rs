// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, sync::Arc};

use oxid_foundation::{OpaqueIdError, UnixTimestampMillis};
use oxid_platform_ports::{ClockPort, PlatformError};
use oxid_wallet_domain::{
    AssetBalance, ChainAddress, ChainAddressError, ChainAddressKind, WalletProfileId,
    WalletTransactionAuthorizationChallenge, WalletTransactionDraftId, WalletTransactionDraftState,
    WalletTransactionFeeState, WalletTransferPreview,
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
    UnsupportedNetwork,
    InvalidRecipient,
    RecipientNetworkMismatch,
    InsufficientFunds,
    DraftNotFound,
    DraftExpired,
    DraftConflict,
    AuthorizationChallengeMismatch,
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
            Self::UnsupportedNetwork => "wallet network is not supported",
            Self::InvalidRecipient => "transaction recipient is invalid",
            Self::RecipientNetworkMismatch => "transaction recipient belongs to another network",
            Self::InsufficientFunds => "wallet has insufficient funds",
            Self::DraftNotFound => "transaction draft was not found",
            Self::DraftExpired => "transaction draft has expired",
            Self::DraftConflict => "transaction draft conflicts with current wallet state",
            Self::AuthorizationChallengeMismatch => {
                "transaction authorization does not match the prepared preview"
            }
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

/// Adapter-neutral request for authorizing one retained transfer draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizeWalletTransferRequest {
    pub draft_id: WalletTransactionDraftId,
    pub authorization_challenge: WalletTransactionAuthorizationChallenge,
    pub now: UnixTimestampMillis,
}

/// Focused outgoing port retaining chain-specific draft/signing material.
pub trait WalletTransactionPort: Send + Sync {
    fn prepare(
        &self,
        profile_id: &WalletProfileId,
        request: PrepareWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError>;

    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        request: AuthorizeWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError>;

    fn get(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
        now: UnixTimestampMillis,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError>;
}

/// Incoming request for preparing an unshielded NIGHT transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareWalletTransferCommand {
    pub profile_id: String,
    pub recipient_address: String,
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

/// Incoming query for safe retained-draft metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransferDraftQuery {
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
            amount: asset_view(preview.amount()),
            change: asset_view(preview.change()),
            fee: preview.fee().map(asset_view),
            fee_state: fee_state_name(preview.fee_state()).to_owned(),
            input_count: preview.input_count(),
            expires_at_millis: preview.expires_at().value(),
            state: draft_state_name(preview.state()).to_owned(),
            proof_required: true,
            submission_ready: false,
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

/// Incoming use case for explicitly authorizing a retained transfer draft.
pub trait AuthorizeWalletTransferUseCase: Send + Sync {
    fn execute(
        &self,
        command: AuthorizeWalletTransferCommand,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError>;
}

/// Incoming use case for reading safe retained-draft state.
pub trait GetWalletTransferDraftUseCase: Send + Sync {
    fn execute(
        &self,
        query: WalletTransferDraftQuery,
    ) -> Result<WalletTransferPreviewView, WalletTransactionError>;
}

/// Stable transaction failures exposed by the application boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletTransactionError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidDraftIdentifier(OpaqueIdError),
    InvalidAuthorizationChallenge(OpaqueIdError),
    InvalidRecipient(ChainAddressError),
    InvalidAmount,
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

const fn draft_state_name(state: WalletTransactionDraftState) -> &'static str {
    match state {
        WalletTransactionDraftState::Prepared => "prepared",
        WalletTransactionDraftState::Authorized => "authorized",
        WalletTransactionDraftState::Expired => "expired",
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
    use std::sync::Mutex;

    use oxid_wallet_domain::{
        AssetSymbol, ChainAccountId, ChainAsset, ChainAssetId, ChainNetworkId,
        WalletTransactionDraftState, WalletTransactionFeeState,
    };

    use super::*;

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    #[derive(Default)]
    struct RecordingTransactions {
        prepare_calls: Mutex<usize>,
        authorize_calls: Mutex<usize>,
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
        assert!(!result.submission_ready);
    }
}
