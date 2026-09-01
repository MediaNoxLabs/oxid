// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt};

use oxid_foundation::{UnixTimestampMillis, opaque_id_type};

use crate::{
    AssetBalance, ChainAccountId, ChainAddress, ChainAddressKind, ChainBlockId, ChainNetworkId,
    ChainTransactionId,
};

/// Maximum number of public inputs that a wallet transfer preview may report.
pub const MAX_WALLET_TRANSFER_INPUTS: u16 = 256;

opaque_id_type! {
    /// Opaque handle for transaction material retained by an outgoing adapter.
    pub struct WalletTransactionDraftId;
}

opaque_id_type! {
    /// Public challenge binding authorization to one exact retained draft.
    pub struct WalletTransactionAuthorizationChallenge;
}

/// Lifecycle state of retained transaction material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletTransactionDraftState {
    Prepared,
    Authorized,
    Submitting,
    Submitted,
    Expired,
}

/// Public lifecycle of one submission attempt.
///
/// This is deliberately separate from the retained draft state: a cancelled
/// pre-broadcast attempt restores an authorized draft so it can be retried,
/// while the submission status records that the previous attempt ended by
/// explicit cancellation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletTransactionSubmissionState {
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

/// Safe status for an adapter-owned submission attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransactionSubmissionStatus {
    draft_id: WalletTransactionDraftId,
    state: WalletTransactionSubmissionState,
    transaction_id: Option<ChainTransactionId>,
    fee: Option<AssetBalance>,
    mode: Option<WalletTransferSubmissionMode>,
    submission: Option<WalletTransferSubmission>,
}

impl WalletTransactionSubmissionStatus {
    #[must_use]
    pub fn new(
        draft_id: WalletTransactionDraftId,
        state: WalletTransactionSubmissionState,
        submission: Option<WalletTransferSubmission>,
    ) -> Self {
        let transaction_id = submission
            .as_ref()
            .map(|value| value.transaction_id().clone());
        let fee = submission.as_ref().map(|value| value.fee().clone());
        let mode = submission.as_ref().map(WalletTransferSubmission::mode);
        Self {
            draft_id,
            state,
            transaction_id,
            fee,
            mode,
            submission,
        }
    }

    /// Constructs a durable post-broadcast status before an inclusion block is known.
    #[must_use]
    pub fn recorded(
        draft_id: WalletTransactionDraftId,
        state: WalletTransactionSubmissionState,
        transaction_id: ChainTransactionId,
        fee: AssetBalance,
        mode: WalletTransferSubmissionMode,
    ) -> Self {
        Self {
            draft_id,
            state,
            transaction_id: Some(transaction_id),
            fee: Some(fee),
            mode: Some(mode),
            submission: None,
        }
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
    pub const fn submission(&self) -> Option<&WalletTransferSubmission> {
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
            .map(WalletTransferSubmission::block_id)
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
    pub const fn cancellation_allowed(&self) -> bool {
        matches!(self.state, WalletTransactionSubmissionState::Running)
    }

    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self.state,
            WalletTransactionSubmissionState::NotStarted
                | WalletTransactionSubmissionState::Cancelled
        )
    }

    /// Whether a newly built transfer may safely replace this attempt.
    #[must_use]
    pub const fn replacement_allowed(&self) -> bool {
        matches!(
            self.state,
            WalletTransactionSubmissionState::Rejected | WalletTransactionSubmissionState::Expired
        )
    }

    /// Whether finalized chain state can still resolve this attempt.
    #[must_use]
    pub const fn reconciliation_allowed(&self) -> bool {
        matches!(
            self.state,
            WalletTransactionSubmissionState::Broadcasting
                | WalletTransactionSubmissionState::OutcomeUnknown
        )
    }
}

/// Fee state surfaced before a chain-specific balancing adapter is invoked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletTransactionFeeState {
    RequiresBalancing,
    Estimated,
    Final,
}

/// Safe, exact preview of one transfer without signing bytes or serialized transaction data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransferPreview {
    draft_id: WalletTransactionDraftId,
    authorization_challenge: WalletTransactionAuthorizationChallenge,
    network_id: ChainNetworkId,
    account_id: ChainAccountId,
    recipient: ChainAddress,
    amount: AssetBalance,
    change: AssetBalance,
    fee: Option<AssetBalance>,
    fee_state: WalletTransactionFeeState,
    input_count: u16,
    expires_at: UnixTimestampMillis,
    state: WalletTransactionDraftState,
}

impl WalletTransferPreview {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        draft_id: WalletTransactionDraftId,
        authorization_challenge: WalletTransactionAuthorizationChallenge,
        network_id: ChainNetworkId,
        account_id: ChainAccountId,
        recipient: ChainAddress,
        amount: AssetBalance,
        change: AssetBalance,
        fee: Option<AssetBalance>,
        fee_state: WalletTransactionFeeState,
        input_count: u16,
        expires_at: UnixTimestampMillis,
        state: WalletTransactionDraftState,
    ) -> Result<Self, WalletTransferPreviewError> {
        if !matches!(
            recipient.kind(),
            ChainAddressKind::Unshielded | ChainAddressKind::Shielded
        ) {
            return Err(WalletTransferPreviewError::UnsupportedRecipientKind);
        }
        if amount.atomic_units() == 0 {
            return Err(WalletTransferPreviewError::ZeroAmount);
        }
        if amount.asset() != change.asset() {
            return Err(WalletTransferPreviewError::ChangeAssetMismatch);
        }
        if input_count == 0 || input_count > MAX_WALLET_TRANSFER_INPUTS {
            return Err(WalletTransferPreviewError::InvalidInputCount);
        }
        if matches!(fee_state, WalletTransactionFeeState::RequiresBalancing) && fee.is_some() {
            return Err(WalletTransferPreviewError::FeeStateMismatch);
        }
        if matches!(
            fee_state,
            WalletTransactionFeeState::Estimated | WalletTransactionFeeState::Final
        ) && fee.is_none()
        {
            return Err(WalletTransferPreviewError::FeeStateMismatch);
        }

        Ok(Self {
            draft_id,
            authorization_challenge,
            network_id,
            account_id,
            recipient,
            amount,
            change,
            fee,
            fee_state,
            input_count,
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
    pub const fn recipient(&self) -> &ChainAddress {
        &self.recipient
    }

    #[must_use]
    pub const fn amount(&self) -> &AssetBalance {
        &self.amount
    }

    #[must_use]
    pub const fn change(&self) -> &AssetBalance {
        &self.change
    }

    #[must_use]
    pub const fn fee(&self) -> Option<&AssetBalance> {
        self.fee.as_ref()
    }

    #[must_use]
    pub const fn fee_state(&self) -> WalletTransactionFeeState {
        self.fee_state
    }

    #[must_use]
    pub const fn input_count(&self) -> u16 {
        self.input_count
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

    /// Records the exact chain fee once balancing/proving has completed.
    #[must_use]
    pub fn with_final_fee(&self, fee: AssetBalance) -> Self {
        let mut updated = self.clone();
        updated.fee = Some(fee);
        updated.fee_state = WalletTransactionFeeState::Final;
        updated
    }
}

/// Whether a public submission outcome came from conformance simulation or a live chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletTransferSubmissionMode {
    Simulated,
    Live,
}

/// Public inclusion outcome; serialized transaction and proof material stay in the adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransferSubmission {
    draft_id: WalletTransactionDraftId,
    transaction_id: ChainTransactionId,
    block_id: ChainBlockId,
    fee: AssetBalance,
    mode: WalletTransferSubmissionMode,
}

impl WalletTransferSubmission {
    #[must_use]
    pub const fn new(
        draft_id: WalletTransactionDraftId,
        transaction_id: ChainTransactionId,
        block_id: ChainBlockId,
        fee: AssetBalance,
        mode: WalletTransferSubmissionMode,
    ) -> Self {
        Self {
            draft_id,
            transaction_id,
            block_id,
            fee,
            mode,
        }
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

/// Domain validation failures for a transfer preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletTransferPreviewError {
    UnsupportedRecipientKind,
    ZeroAmount,
    ChangeAssetMismatch,
    InvalidInputCount,
    FeeStateMismatch,
}

impl fmt::Display for WalletTransferPreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnsupportedRecipientKind => {
                "transfer recipient must be an unshielded or shielded address"
            }
            Self::ZeroAmount => "transfer amount must be greater than zero",
            Self::ChangeAssetMismatch => "transfer amount and change must use the same asset",
            Self::InvalidInputCount => "transfer input count is outside the supported range",
            Self::FeeStateMismatch => "transfer fee metadata does not match its state",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletTransferPreviewError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetSymbol, ChainAsset, ChainAssetId};

    fn night() -> ChainAsset {
        ChainAsset::new(
            ChainAssetId::parse("midnight:night").expect("asset id is valid"),
            AssetSymbol::parse("NIGHT").expect("symbol is valid"),
            6,
        )
    }

    fn preview(input_count: u16) -> Result<WalletTransferPreview, WalletTransferPreviewError> {
        WalletTransferPreview::new(
            WalletTransactionDraftId::parse("txdraft_test").expect("draft id is valid"),
            WalletTransactionAuthorizationChallenge::parse("txauth_test")
                .expect("challenge is valid"),
            ChainNetworkId::parse("undeployed").expect("network id is valid"),
            ChainAccountId::parse("midnight_account_0_0").expect("account id is valid"),
            ChainAddress::parse(ChainAddressKind::Unshielded, "mn_addr_undeployed1recipient")
                .expect("address is structurally valid"),
            AssetBalance::new(night(), 1_000_000),
            AssetBalance::new(night(), 2_000_000),
            None,
            WalletTransactionFeeState::RequiresBalancing,
            input_count,
            UnixTimestampMillis::new(1_700_003_600_000),
            WalletTransactionDraftState::Prepared,
        )
    }

    #[test]
    fn preview_requires_exact_nonzero_transfer_and_bounded_inputs() {
        let value = preview(2).expect("preview is valid");
        assert_eq!(value.amount().atomic_units(), 1_000_000);
        assert_eq!(value.change().atomic_units(), 2_000_000);
        assert_eq!(value.input_count(), 2);
        assert_eq!(
            preview(0),
            Err(WalletTransferPreviewError::InvalidInputCount)
        );
        assert_eq!(
            preview(MAX_WALLET_TRANSFER_INPUTS + 1),
            Err(WalletTransferPreviewError::InvalidInputCount)
        );
    }

    #[test]
    fn state_transition_preserves_the_bound_preview() {
        let prepared = preview(1).expect("preview is valid");
        let authorized = prepared.with_state(WalletTransactionDraftState::Authorized);
        assert_eq!(authorized.state(), WalletTransactionDraftState::Authorized);
        assert_eq!(authorized.draft_id(), prepared.draft_id());
        assert_eq!(
            authorized.authorization_challenge(),
            prepared.authorization_challenge()
        );
        assert_eq!(authorized.recipient(), prepared.recipient());
    }

    #[test]
    fn preview_accepts_a_shielded_recipient_without_exposing_private_material() {
        let value = WalletTransferPreview::new(
            WalletTransactionDraftId::parse("txdraft_shielded").expect("draft id is valid"),
            WalletTransactionAuthorizationChallenge::parse("txauth_shielded")
                .expect("challenge is valid"),
            ChainNetworkId::parse("undeployed").expect("network id is valid"),
            ChainAccountId::parse("midnight_account_0_0").expect("account id is valid"),
            ChainAddress::parse(
                ChainAddressKind::Shielded,
                "mn_shield-addr_undeployed1recipient",
            )
            .expect("address is structurally valid"),
            AssetBalance::new(night(), 1_000_000),
            AssetBalance::new(night(), 2_000_000),
            None,
            WalletTransactionFeeState::RequiresBalancing,
            1,
            UnixTimestampMillis::new(1_700_003_600_000),
            WalletTransactionDraftState::Prepared,
        )
        .expect("shielded preview is valid");

        assert_eq!(value.recipient().kind(), ChainAddressKind::Shielded);
    }

    #[test]
    fn submission_status_separates_cancellation_from_draft_retryability() {
        let draft_id = WalletTransactionDraftId::parse("txdraft_test").expect("draft is valid");
        let running = WalletTransactionSubmissionStatus::new(
            draft_id.clone(),
            WalletTransactionSubmissionState::Running,
            None,
        );
        assert!(running.cancellation_allowed());
        assert!(!running.retryable());

        let cancelled = WalletTransactionSubmissionStatus::new(
            draft_id.clone(),
            WalletTransactionSubmissionState::Cancelled,
            None,
        );
        assert!(!cancelled.cancellation_allowed());
        assert!(cancelled.retryable());

        let unknown = WalletTransactionSubmissionStatus::new(
            draft_id.clone(),
            WalletTransactionSubmissionState::OutcomeUnknown,
            None,
        );
        assert!(unknown.reconciliation_allowed());
        assert!(!unknown.retryable());
        assert!(!unknown.replacement_allowed());

        let dust = ChainAsset::new(
            crate::ChainAssetId::parse("midnight:dust").expect("asset id is valid"),
            crate::AssetSymbol::parse("DUST").expect("symbol is valid"),
            15,
        );
        let recorded = WalletTransactionSubmissionStatus::recorded(
            draft_id.clone(),
            WalletTransactionSubmissionState::OutcomeUnknown,
            ChainTransactionId::parse("tx_recorded").expect("transaction id is valid"),
            AssetBalance::new(dust, 42),
            WalletTransferSubmissionMode::Live,
        );
        assert_eq!(
            recorded.transaction_id().map(ChainTransactionId::as_str),
            Some("tx_recorded")
        );
        assert_eq!(recorded.fee().map(AssetBalance::atomic_units), Some(42));
        assert_eq!(recorded.mode(), Some(WalletTransferSubmissionMode::Live));
        assert!(recorded.submission().is_none());

        let expired = WalletTransactionSubmissionStatus::new(
            draft_id,
            WalletTransactionSubmissionState::Expired,
            None,
        );
        assert!(expired.replacement_allowed());
        assert!(!expired.retryable());
        assert!(!expired.reconciliation_allowed());
    }
}
