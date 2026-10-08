// SPDX-License-Identifier: Apache-2.0

use super::*;
use oxid_wallet_application::WalletProfileSecurityCommand;

#[test]
fn in_memory_composition_exposes_only_development_protection() {
    let services = compose_in_memory();
    let command = WalletProfileSecurityCommand {
        profile_id: "profile_test".to_owned(),
    };
    let initial = services
        .get_wallet_security_status()
        .execute(command.clone())
        .expect("development status should be available");

    assert_eq!(initial.state_name(), "Uninitialized");
    assert_eq!(initial.protection_name(), "Development only");
    assert_eq!(
        services
            .initialize_wallet_security()
            .execute(command)
            .expect("development setup should succeed")
            .state_name(),
        "Unlocked"
    );
}

#[test]
fn ordinary_memory_headless_and_production_cannot_approve_did_operations() {
    use oxid_identity_application::{
        DeactivateDidCommand, DidApprovalError, DidOperationError, DidUpdate,
        SignDidPayloadCommand, UpdateDidCommand,
    };
    for services in [
        compose_in_memory(),
        crate::compose_headless(),
        crate::compose(),
    ] {
        let did = format!("did:midnight:undeployed:{:064x}", 1);
        let expected = DidOperationError::Approval(DidApprovalError::Unavailable);
        assert_eq!(
            services.update_did().execute(UpdateDidCommand {
                profile_id: "profile_test".into(),
                did: did.clone(),
                operation: DidUpdate::RemoveService {
                    id: "#service".into()
                }
            }),
            Err(expected.clone())
        );
        assert_eq!(
            services.deactivate_did().execute(DeactivateDidCommand {
                profile_id: "profile_test".into(),
                did: did.clone()
            }),
            Err(expected.clone())
        );
        assert_eq!(
            services.sign_did_payload().execute(SignDidPayloadCommand {
                profile_id: "profile_test".into(),
                did,
                method_id: "#auth-1".into(),
                payload: b"challenge"
            }),
            Err(expected)
        );
    }
}
