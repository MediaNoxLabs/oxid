// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_adapter_platform_system::SystemClock;
use oxid_wallet_application::{
    TrustedWalletApprovalError, TrustedWalletApprovalPort, WalletApprovalIntent,
    WalletApprovalService,
};

/// Fixture-only authority for the canonical standalone NIGHT round-trip.
///
/// The separately named fixture executable is the only caller compiled with
/// this module. Ordinary headless and product compositions retain the
/// fail-closed approval service.
struct DevelopmentTransferOnlyApproval;

impl TrustedWalletApprovalPort for DevelopmentTransferOnlyApproval {
    fn approve(&self, intent: &WalletApprovalIntent) -> Result<(), TrustedWalletApprovalError> {
        match intent {
            WalletApprovalIntent::AuthorizeTransfer { .. }
            | WalletApprovalIntent::SubmitTransfer { .. } => Ok(()),
            _ => Err(TrustedWalletApprovalError::Unavailable),
        }
    }
}

pub(super) fn development_movement_approval_service() -> Arc<WalletApprovalService> {
    Arc::new(WalletApprovalService::with_trusted_port(
        Arc::new(SystemClock),
        Arc::new(DevelopmentTransferOnlyApproval),
    ))
}

#[cfg(test)]
mod tests {
    use oxid_wallet_application::{
        CanonicalApprovalDigest, TrustedWalletApprovalError, TrustedWalletApprovalPort,
        WalletApprovalIntent, WalletProfileId,
    };

    use super::DevelopmentTransferOnlyApproval;

    #[test]
    fn fixture_rejects_unrelated_protected_operations() {
        let intent = WalletApprovalIntent::SignData {
            profile: WalletProfileId::parse("profile_test").unwrap(),
            digest: CanonicalApprovalDigest::from_sha256([7; 32]),
        };

        assert_eq!(
            DevelopmentTransferOnlyApproval.approve(&intent),
            Err(TrustedWalletApprovalError::Unavailable)
        );
    }
}
