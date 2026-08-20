// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt};

use oxid_foundation::UnixTimestampMillis;

use crate::{
    AssetBalance, ChainAccountId, ChainBlockId, ChainNetworkId, ChainTransactionId,
    WalletTransactionAuthorizationChallenge, WalletTransactionDraftId, WalletTransactionDraftState,
    WalletTransactionFeeState, WalletTransactionSubmissionState, WalletTransferSubmissionMode,
};

/// Maximum number of unshielded NIGHT inputs one registration preview may aggregate.
pub const MAX_WALLET_DUST_REGISTRATION_INPUTS: u16 = 256;

const MIDNIGHT_NIGHT_ASSET_ID: &str = "midnight:night";
const MIDNIGHT_NIGHT_SYMBOL: &str = "NIGHT";
const MIDNIGHT_NIGHT_DECIMALS: u8 = 6;
const MIDNIGHT_DUST_ASSET_ID: &str = "midnight:dust";
const MIDNIGHT_DUST_SYMBOL: &str = "DUST";
const MIDNIGHT_DUST_DECIMALS: u8 = 15;

/// Safe public preview of one retained DUST registration intent.
///
/// The protected DUST key, registration signature, NIGHT input identities,
/// proof material, and serialized transaction stay in the outgoing adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationPreview {
    draft_id: WalletTransactionDraftId,
    authorization_challenge: WalletTransactionAuthorizationChallenge,
    network_id: ChainNetworkId,
    account_id: ChainAccountId,
    registered_night: AssetBalance,
    input_count: u16,
    maximum_fee_allowance: AssetBalance,
    fee_state: WalletTransactionFeeState,
    expires_at: UnixTimestampMillis,
    state: WalletTransactionDraftState,
}

impl WalletDustRegistrationPreview {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        draft_id: WalletTransactionDraftId,
        authorization_challenge: WalletTransactionAuthorizationChallenge,
        network_id: ChainNetworkId,
        account_id: ChainAccountId,
        registered_night: AssetBalance,
        input_count: u16,
        maximum_fee_allowance: AssetBalance,
        fee_state: WalletTransactionFeeState,
        expires_at: UnixTimestampMillis,
        state: WalletTransactionDraftState,
    ) -> Result<Self, WalletDustRegistrationPreviewError> {
        if !is_night(&registered_night) {
            return Err(WalletDustRegistrationPreviewError::InvalidNightAsset);
        }
        if registered_night.atomic_units() == 0 {
            return Err(WalletDustRegistrationPreviewError::ZeroNightValue);
        }
        if input_count == 0 || input_count > MAX_WALLET_DUST_REGISTRATION_INPUTS {
            return Err(WalletDustRegistrationPreviewError::InvalidInputCount);
        }
        if !is_dust(&maximum_fee_allowance) {
            return Err(WalletDustRegistrationPreviewError::InvalidFeeAllowanceAsset);
        }
        if maximum_fee_allowance.atomic_units() == 0 {
            return Err(WalletDustRegistrationPreviewError::ZeroFeeAllowance);
        }

        Ok(Self {
            draft_id,
            authorization_challenge,
            network_id,
            account_id,
            registered_night,
            input_count,
            maximum_fee_allowance,
            fee_state,
            expires_at,
            state,
        })
    }

    #[must_use]
    pub const fn draft_id(&self) -> &WalletTransactionDraftId {
        &self.draft_id
    }

    #[must_use]
    pub const fn authorization_challenge(&self) -> &WalletTransactionAuthorizationChallenge {
        &self.authorization_challenge
    }

    #[must_use]
    pub const fn network_id(&self) -> &ChainNetworkId {
        &self.network_id
    }

    #[must_use]
    pub const fn account_id(&self) -> &ChainAccountId {
        &self.account_id
    }

    #[must_use]
    pub const fn registered_night(&self) -> &AssetBalance {
        &self.registered_night
    }

    #[must_use]
    pub const fn input_count(&self) -> u16 {
        self.input_count
    }

    #[must_use]
    pub const fn maximum_fee_allowance(&self) -> &AssetBalance {
        &self.maximum_fee_allowance
    }

    #[must_use]
    pub const fn fee_state(&self) -> WalletTransactionFeeState {
        self.fee_state
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixTimestampMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn state(&self) -> WalletTransactionDraftState {
        self.state
    }

    #[must_use]
    pub fn with_state(&self, state: WalletTransactionDraftState) -> Self {
        let mut updated = self.clone();
        updated.state = state;
        updated
    }

    #[must_use]
    pub fn with_fee_state(&self, fee_state: WalletTransactionFeeState) -> Self {
        let mut updated = self.clone();
        updated.fee_state = fee_state;
        updated
    }
}

/// Whether finalized chain state has observed the protected registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationObservation {
    NotObserved,
    Included,
}

/// What the registration result establishes about usable DUST.
///
/// This boundary deliberately has no `Spendable` variant. Only the separate
/// DUST synchronization capability may establish a current spendable balance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustReadiness {
    NotEstablished,
    RequiresSynchronization,
}

/// Public finalized inclusion of one DUST registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationSubmission {
    draft_id: WalletTransactionDraftId,
    transaction_id: ChainTransactionId,
    block_id: ChainBlockId,
    fee: AssetBalance,
    mode: WalletTransferSubmissionMode,
}

impl WalletDustRegistrationSubmission {
    pub fn new(
        draft_id: WalletTransactionDraftId,
        transaction_id: ChainTransactionId,
        block_id: ChainBlockId,
        fee: AssetBalance,
        mode: WalletTransferSubmissionMode,
    ) -> Result<Self, WalletDustRegistrationSubmissionError> {
        if !is_dust(&fee) {
            return Err(WalletDustRegistrationSubmissionError::InvalidFeeAsset);
        }
        Ok(Self {
            draft_id,
            transaction_id,
            block_id,
            fee,
            mode,
        })
    }

    #[must_use]
    pub const fn draft_id(&self) -> &WalletTransactionDraftId {
        &self.draft_id
    }

    #[must_use]
    pub const fn transaction_id(&self) -> &ChainTransactionId {
        &self.transaction_id
    }

    #[must_use]
    pub const fn block_id(&self) -> &ChainBlockId {
        &self.block_id
    }

    #[must_use]
    pub const fn fee(&self) -> &AssetBalance {
        &self.fee
    }

    #[must_use]
    pub const fn mode(&self) -> WalletTransferSubmissionMode {
        self.mode
    }
}

/// Registration-specific submission status.
///
/// It intentionally does not implement or alias the transfer-history record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationSubmissionStatus {
    draft_id: WalletTransactionDraftId,
    state: WalletTransactionSubmissionState,
    transaction_id: Option<ChainTransactionId>,
    fee: Option<AssetBalance>,
    mode: Option<WalletTransferSubmissionMode>,
    submission: Option<WalletDustRegistrationSubmission>,
}

impl WalletDustRegistrationSubmissionStatus {
    pub fn pending(
        draft_id: WalletTransactionDraftId,
        state: WalletTransactionSubmissionState,
    ) -> Result<Self, WalletDustRegistrationSubmissionStatusError> {
        if matches!(state, WalletTransactionSubmissionState::Included) {
            return Err(WalletDustRegistrationSubmissionStatusError::MissingInclusion);
        }
        Ok(Self {
            draft_id,
            state,
            transaction_id: None,
            fee: None,
            mode: None,
            submission: None,
        })
    }

    /// Constructs a durable post-broadcast status before inclusion is known.
    pub fn recorded(
        draft_id: WalletTransactionDraftId,
        state: WalletTransactionSubmissionState,
        transaction_id: ChainTransactionId,
        fee: AssetBalance,
        mode: WalletTransferSubmissionMode,
    ) -> Result<Self, WalletDustRegistrationSubmissionStatusError> {
        if matches!(
            state,
            WalletTransactionSubmissionState::NotStarted
                | WalletTransactionSubmissionState::Running
                | WalletTransactionSubmissionState::CancellationRequested
                | WalletTransactionSubmissionState::Cancelled
                | WalletTransactionSubmissionState::Included
        ) {
            return Err(WalletDustRegistrationSubmissionStatusError::InvalidRecordedState);
        }
        if !is_dust(&fee) {
            return Err(WalletDustRegistrationSubmissionStatusError::InvalidFeeAsset);
        }
        Ok(Self {
            draft_id,
            state,
            transaction_id: Some(transaction_id),
            fee: Some(fee),
            mode: Some(mode),
            submission: None,
        })
    }

    pub fn included(
        submission: WalletDustRegistrationSubmission,
    ) -> Result<Self, WalletDustRegistrationSubmissionStatusError> {
        if !is_dust(submission.fee()) {
            return Err(WalletDustRegistrationSubmissionStatusError::InvalidFeeAsset);
        }
        Ok(Self {
            draft_id: submission.draft_id().clone(),
            state: WalletTransactionSubmissionState::Included,
            transaction_id: Some(submission.transaction_id().clone()),
            fee: Some(submission.fee().clone()),
            mode: Some(submission.mode()),
            submission: Some(submission),
        })
    }

    #[must_use]
    pub const fn draft_id(&self) -> &WalletTransactionDraftId {
        &self.draft_id
    }

    #[must_use]
    pub const fn state(&self) -> WalletTransactionSubmissionState {
        self.state
    }

    #[must_use]
    pub const fn submission(&self) -> Option<&WalletDustRegistrationSubmission> {
        self.submission.as_ref()
    }

    #[must_use]
    pub const fn transaction_id(&self) -> Option<&ChainTransactionId> {
        self.transaction_id.as_ref()
    }

    #[must_use]
    pub fn block_id(&self) -> Option<&ChainBlockId> {
        self.submission
            .as_ref()
            .map(WalletDustRegistrationSubmission::block_id)
    }

    #[must_use]
    pub const fn fee(&self) -> Option<&AssetBalance> {
        self.fee.as_ref()
    }

    #[must_use]
    pub const fn mode(&self) -> Option<WalletTransferSubmissionMode> {
        self.mode
    }

    #[must_use]
    pub const fn registration_observation(&self) -> WalletDustRegistrationObservation {
        if matches!(self.state, WalletTransactionSubmissionState::Included) {
            WalletDustRegistrationObservation::Included
        } else {
            WalletDustRegistrationObservation::NotObserved
        }
    }

    #[must_use]
    pub const fn dust_readiness(&self) -> WalletDustReadiness {
        if matches!(self.state, WalletTransactionSubmissionState::Included) {
            WalletDustReadiness::RequiresSynchronization
        } else {
            WalletDustReadiness::NotEstablished
        }
    }

    #[must_use]
    pub const fn cancellation_allowed(&self) -> bool {
        matches!(self.state, WalletTransactionSubmissionState::Running)
    }

    #[must_use]
    pub const fn reconciliation_allowed(&self) -> bool {
        matches!(
            self.state,
            WalletTransactionSubmissionState::Broadcasting
                | WalletTransactionSubmissionState::OutcomeUnknown
        )
    }
}

/// Domain validation failures for a registration preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationPreviewError {
    InvalidNightAsset,
    ZeroNightValue,
    InvalidInputCount,
    InvalidFeeAllowanceAsset,
    ZeroFeeAllowance,
}

impl fmt::Display for WalletDustRegistrationPreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidNightAsset => "registration value must use canonical Midnight NIGHT",
            Self::ZeroNightValue => "registration NIGHT value must be greater than zero",
            Self::InvalidInputCount => "registration input count is outside the supported range",
            Self::InvalidFeeAllowanceAsset => {
                "registration fee allowance must use canonical Midnight DUST"
            }
            Self::ZeroFeeAllowance => "registration fee allowance must be greater than zero",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletDustRegistrationPreviewError {}

/// Domain validation failures for finalized registration metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationSubmissionError {
    InvalidFeeAsset,
}

impl fmt::Display for WalletDustRegistrationSubmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFeeAsset => "registration fee must use canonical Midnight DUST",
        })
    }
}

impl Error for WalletDustRegistrationSubmissionError {}

/// Domain validation failures for registration submission status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationSubmissionStatusError {
    MissingInclusion,
    InvalidRecordedState,
    InvalidFeeAsset,
}

impl fmt::Display for WalletDustRegistrationSubmissionStatusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingInclusion => "included registration status requires inclusion metadata",
            Self::InvalidRecordedState => {
                "registration metadata is invalid for the submission state"
            }
            Self::InvalidFeeAsset => "registration fee must use canonical Midnight DUST",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletDustRegistrationSubmissionStatusError {}

fn is_night(balance: &AssetBalance) -> bool {
    balance.asset().id().as_str() == MIDNIGHT_NIGHT_ASSET_ID
        && balance.asset().symbol().as_str() == MIDNIGHT_NIGHT_SYMBOL
        && balance.asset().decimals() == MIDNIGHT_NIGHT_DECIMALS
}

fn is_dust(balance: &AssetBalance) -> bool {
    balance.asset().id().as_str() == MIDNIGHT_DUST_ASSET_ID
        && balance.asset().symbol().as_str() == MIDNIGHT_DUST_SYMBOL
        && balance.asset().decimals() == MIDNIGHT_DUST_DECIMALS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetSymbol, ChainAsset, ChainAssetId};

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

    fn preview(
        registered_night: AssetBalance,
        input_count: u16,
        maximum_fee_allowance: AssetBalance,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPreviewError> {
        WalletDustRegistrationPreview::new(
            WalletTransactionDraftId::parse("dustreg_test").expect("draft id is valid"),
            WalletTransactionAuthorizationChallenge::parse("dustauth_test")
                .expect("challenge is valid"),
            ChainNetworkId::parse("undeployed").expect("network id is valid"),
            ChainAccountId::parse("midnight_account_0_0").expect("account id is valid"),
            registered_night,
            input_count,
            maximum_fee_allowance,
            WalletTransactionFeeState::RequiresBalancing,
            UnixTimestampMillis::new(1_700_003_600_000),
            WalletTransactionDraftState::Prepared,
        )
    }

    #[test]
    fn preview_requires_exact_assets_nonzero_values_and_bounded_inputs() {
        let value = preview(night(5_000_000), 2, dust(42)).expect("preview is valid");
        assert_eq!(value.registered_night().atomic_units(), 5_000_000);
        assert_eq!(value.maximum_fee_allowance().atomic_units(), 42);
        assert_eq!(value.input_count(), 2);

        assert_eq!(
            preview(night(0), 1, dust(42)),
            Err(WalletDustRegistrationPreviewError::ZeroNightValue)
        );
        assert_eq!(
            preview(night(1), 0, dust(42)),
            Err(WalletDustRegistrationPreviewError::InvalidInputCount)
        );
        assert_eq!(
            preview(night(1), MAX_WALLET_DUST_REGISTRATION_INPUTS + 1, dust(42)),
            Err(WalletDustRegistrationPreviewError::InvalidInputCount)
        );
        assert_eq!(
            preview(night(1), 1, dust(0)),
            Err(WalletDustRegistrationPreviewError::ZeroFeeAllowance)
        );
        assert_eq!(
            preview(asset("midnight:dust", "DUST", 15, 1), 1, dust(42)),
            Err(WalletDustRegistrationPreviewError::InvalidNightAsset)
        );
        assert_eq!(
            preview(night(1), 1, asset("midnight:night", "NIGHT", 6, 42)),
            Err(WalletDustRegistrationPreviewError::InvalidFeeAllowanceAsset)
        );
    }

    #[test]
    fn lifecycle_changes_preserve_the_exact_public_registration_plan() {
        let prepared = preview(night(5_000_000), 1, dust(42)).expect("preview is valid");
        let authorized = prepared.with_state(WalletTransactionDraftState::Authorized);
        assert_eq!(authorized.draft_id(), prepared.draft_id());
        assert_eq!(
            authorized.authorization_challenge(),
            prepared.authorization_challenge()
        );
        assert_eq!(authorized.registered_night(), prepared.registered_night());
        assert_eq!(
            authorized.maximum_fee_allowance(),
            prepared.maximum_fee_allowance()
        );
    }

    #[test]
    fn included_registration_requires_a_separate_dust_synchronization() {
        let submission = WalletDustRegistrationSubmission::new(
            WalletTransactionDraftId::parse("dustreg_test").expect("draft is valid"),
            ChainTransactionId::parse("tx_registration").expect("transaction is valid"),
            ChainBlockId::parse("block_registration").expect("block is valid"),
            dust(17),
            WalletTransferSubmissionMode::Live,
        )
        .expect("submission is valid");
        let status = WalletDustRegistrationSubmissionStatus::included(submission)
            .expect("included status is valid");

        assert_eq!(
            status.registration_observation(),
            WalletDustRegistrationObservation::Included
        );
        assert_eq!(
            status.dust_readiness(),
            WalletDustReadiness::RequiresSynchronization
        );
        assert!(status.submission().is_some());
    }

    #[test]
    fn pending_status_never_claims_registration_or_dust_readiness() {
        let status = WalletDustRegistrationSubmissionStatus::pending(
            WalletTransactionDraftId::parse("dustreg_test").expect("draft is valid"),
            WalletTransactionSubmissionState::Running,
        )
        .expect("pending status is valid");

        assert_eq!(
            status.registration_observation(),
            WalletDustRegistrationObservation::NotObserved
        );
        assert_eq!(status.dust_readiness(), WalletDustReadiness::NotEstablished);
        assert_eq!(
            WalletDustRegistrationSubmissionStatus::pending(
                WalletTransactionDraftId::parse("dustreg_test").expect("draft is valid"),
                WalletTransactionSubmissionState::Included,
            ),
            Err(WalletDustRegistrationSubmissionStatusError::MissingInclusion)
        );
        assert!(status.cancellation_allowed());
        assert!(!status.reconciliation_allowed());

        let unknown = WalletDustRegistrationSubmissionStatus::recorded(
            WalletTransactionDraftId::parse("dustreg_unknown").expect("draft is valid"),
            WalletTransactionSubmissionState::OutcomeUnknown,
            ChainTransactionId::parse("tx_unknown").expect("transaction is valid"),
            dust(42),
            WalletTransferSubmissionMode::Live,
        )
        .expect("recorded status is valid");
        assert!(!unknown.cancellation_allowed());
        assert!(unknown.reconciliation_allowed());
    }
}
