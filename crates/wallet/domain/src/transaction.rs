// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt};

use oxid_foundation::{OpaqueId, OpaqueIdError, UnixTimestampMillis};

use crate::{
    AssetBalance, ChainAccountId, ChainAddress, ChainAddressKind, ChainBlockId, ChainNetworkId,
    ChainTransactionId,
};

/// Maximum number of public inputs that a wallet transfer preview may report.
pub const MAX_WALLET_TRANSFER_INPUTS: u16 = 256;

/// Opaque handle for transaction material retained by an outgoing adapter.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WalletTransactionDraftId(OpaqueId);

impl WalletTransactionDraftId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Public challenge binding authorization to one exact retained draft.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WalletTransactionAuthorizationChallenge(OpaqueId);

impl WalletTransactionAuthorizationChallenge {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
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
        if recipient.kind() != ChainAddressKind::Unshielded {
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
            Self::UnsupportedRecipientKind => "transfer recipient must be an unshielded address",
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
}
