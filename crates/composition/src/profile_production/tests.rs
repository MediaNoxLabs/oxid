// SPDX-License-Identifier: Apache-2.0

use super::*;
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_credential_application::{
    CredentialOperationError, CredentialProfileQuery, CredentialRepositoryError,
};
use oxid_identity_application::{DidOperationError, DidRecordRepositoryError, ListDidRecordsQuery};
use oxid_wallet_application::{
    WalletAccountQuery, WalletDustSyncCommand, WalletProfileSecurityCommand,
    WalletShieldedSyncCommand,
};

#[test]
fn production_facing_composition_fails_closed_without_native_custody() {
    let services = compose_with_identity_adapters(
        Arc::new(InMemoryWalletProfileRepository::new()),
        Arc::new(UnavailableWalletSecurity),
        Arc::new(unavailable_midnight_wallet()),
        IdentityAdapters {
            did_repository: Arc::new(UnavailableDidRecordRepository),
            did_resolver: Arc::new(UnavailableDidResolver),
            did_lifecycle: Arc::new(UnavailableDidLifecycle),
            did_jubjub_challenge_signing: Arc::new(UnavailableDidLifecycle),
            did_publisher: None,
            credential_repository: Arc::new(UnavailableCredentialRepository),
            credential_inbox: Arc::new(UnavailableCredentialInbox),
            credential_verifier: Arc::new(UnavailableCredentialVerifier),
            credential_disclosure: Arc::new(UnavailableCredentialDisclosure),
            credential_issuance: CredentialIssuanceComposition::Unavailable,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Unavailable,
            credential_presentation: CredentialPresentationComposition::Unavailable,
            portal_test_ingress: None,
        },
        PassportVaultRepositoryComposition::unavailable(),
        |security| security,
    );
    let status = services
        .get_wallet_security_status()
        .execute(WalletProfileSecurityCommand {
            profile_id: "profile_test".to_owned(),
        })
        .expect("unavailable status should be safely reportable");

    assert_eq!(status.state_name(), "Unavailable");
    assert_eq!(status.protection_name(), "Not connected");
    assert_eq!(
        services
            .get_wallet_account()
            .execute(WalletAccountQuery {
                profile_id: "profile_test".to_owned(),
            })
            .expect("unavailable account state is safe")
            .source,
        "unavailable"
    );
    assert_eq!(
        services
            .get_wallet_dust_sync_status()
            .execute(WalletDustSyncCommand {
                profile_id: "profile_test".to_owned(),
            })
            .expect("unavailable DUST status is safe")
            .state,
        "unavailable"
    );
    assert!(
        services
            .start_wallet_dust_sync()
            .execute(WalletDustSyncCommand {
                profile_id: "profile_test".to_owned(),
            })
            .is_err()
    );
    assert_eq!(
        services.list_did_records().execute(ListDidRecordsQuery {
            profile_id: "profile_test".to_owned(),
        }),
        Err(DidOperationError::Persistence(
            DidRecordRepositoryError::Unavailable
        ))
    );
    assert_eq!(
        services.list_credentials().execute(CredentialProfileQuery {
            profile_id: "profile_test".to_owned(),
        }),
        Err(CredentialOperationError::Persistence(
            CredentialRepositoryError::Unavailable
        ))
    );
    assert_eq!(
        services
            .get_wallet_shielded_sync_status()
            .execute(WalletShieldedSyncCommand {
                profile_id: "profile_test".to_owned(),
            })
            .expect("unavailable shielded status is safe")
            .state,
        "unavailable"
    );
    assert!(
        services
            .start_wallet_shielded_sync()
            .execute(WalletShieldedSyncCommand {
                profile_id: "profile_test".to_owned(),
            })
            .is_err()
    );
}
