// SPDX-License-Identifier: Apache-2.0

//! Explicit deterministic test/development authority. Never selected by request
//! data or environment. Enabling the feature alone does not install this fixture.
use super::*;

struct DevelopmentApproval;
impl TrustedDidApprovalPort for DevelopmentApproval {
    fn approve(&self, _: &DidApprovalIntent) -> Result<(), TrustedDidApprovalError> {
        Ok(())
    }
}

/// The sole approving fixture. Callers must provide the application's clock;
/// even development capabilities retain expiry and single-use enforcement.
#[must_use]
pub fn development_did_approvals(clock: Arc<dyn DidApprovalClockPort>) -> Arc<DidApprovalService> {
    Arc::new(DidApprovalService::with_trusted_did_port(
        clock,
        Arc::new(DevelopmentApproval),
    ))
}
