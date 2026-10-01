// SPDX-License-Identifier: Apache-2.0

use super::*;

/// Approval for the exact prepared transfer, never its submission.
pub enum AuthorizeTransferApproval {}
/// Separate approval for an already-authorized retained transfer.
pub enum SubmitTransferApproval {}
/// Approval for the exact prepared protected DUST registration.
pub enum AuthorizeDustRegistrationApproval {}
/// Separate approval for an already-authorized protected DUST registration.
pub enum SubmitDustRegistrationApproval {}

impl sealed::Operation for AuthorizeTransferApproval {}
impl sealed::Operation for SubmitTransferApproval {}
impl sealed::Operation for AuthorizeDustRegistrationApproval {}
impl sealed::Operation for SubmitDustRegistrationApproval {}
impl WalletApprovalOperation for AuthorizeTransferApproval {}
impl WalletApprovalOperation for SubmitTransferApproval {}
impl WalletApprovalOperation for AuthorizeDustRegistrationApproval {}
impl WalletApprovalOperation for SubmitDustRegistrationApproval {}

impl WalletApprovalRequest<AuthorizeTransferApproval> {
    #[must_use]
    pub fn authorize_transfer(profile: WalletProfileId, preview: WalletTransferPreview) -> Self {
        Self {
            intent: WalletApprovalIntent::AuthorizeTransfer {
                profile,
                preview: Box::new(preview),
            },
            operation: PhantomData,
        }
    }
}
impl WalletApprovalRequest<SubmitTransferApproval> {
    #[must_use]
    pub fn submit_transfer(profile: WalletProfileId, preview: WalletTransferPreview) -> Self {
        Self {
            intent: WalletApprovalIntent::SubmitTransfer {
                profile,
                preview: Box::new(preview),
            },
            operation: PhantomData,
        }
    }
}
impl WalletApprovalRequest<AuthorizeDustRegistrationApproval> {
    #[must_use]
    pub fn authorize_dust_registration(
        profile: WalletProfileId,
        preview: WalletDustRegistrationPreview,
    ) -> Self {
        Self {
            intent: WalletApprovalIntent::AuthorizeDustRegistration {
                profile,
                preview: Box::new(preview),
            },
            operation: PhantomData,
        }
    }
}
impl WalletApprovalRequest<SubmitDustRegistrationApproval> {
    #[must_use]
    pub fn submit_dust_registration(
        profile: WalletProfileId,
        preview: WalletDustRegistrationPreview,
    ) -> Self {
        Self {
            intent: WalletApprovalIntent::SubmitDustRegistration {
                profile,
                preview: Box::new(preview),
            },
            operation: PhantomData,
        }
    }
}
