// SPDX-License-Identifier: Apache-2.0

use futures::executor::block_on;
use oxid_credential_application::CredentialProfileQuery;
use oxid_identity_application::{CreateDidCommand, ListDidRecordsQuery};
use oxid_protocol_application::{
    AcceptCredentialIssuanceCommand, PrepareCredentialIssuanceCommand,
};
use oxid_wallet_application::{
    CreateWalletProfileCommand, DeriveWalletAccountCommand, EXPORT_COMPLETE_WALLET_BACKUP_SUMMARY,
    EXPORT_COMPLETE_WALLET_BACKUP_TITLE, ExportCompleteWalletBackupCommand,
    RECOVER_COMPLETE_WALLET_BACKUP_SUMMARY, RECOVER_COMPLETE_WALLET_BACKUP_TITLE,
    RecoverCompleteWalletBackupCommand, SensitiveOperationConfirmation, WalletAccountQuery,
    WalletBackupReceiptCommand, WalletProfileSecurityCommand, WalletRecoverySecret,
};

use crate::{compose_in_memory, standalone_oid4vci_offer};

#[test]
fn composed_application_executes_the_vertical_slice() {
    let services = compose_in_memory();
    let result = services
        .create_wallet_profile()
        .execute(CreateWalletProfileCommand {
            display_name: "Composition smoke".to_owned(),
        })
        .expect("composed use case should succeed");

    assert_eq!(result.display_name, "Composition smoke");
    assert!(result.id.starts_with("profile_"));
    assert_eq!(
        services
            .list_wallet_profiles()
            .execute()
            .expect("composed query should succeed"),
        vec![result]
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn standalone_composition_recovers_a_complete_wallet_into_a_fresh_instance() {
    let source = compose_in_memory();
    let profile = source
        .create_wallet_profile()
        .execute(CreateWalletProfileCommand {
            display_name: "Portable standalone wallet".to_owned(),
        })
        .expect("source profile");
    source
        .record_wallet_backup_receipt()
        .execute(WalletBackupReceiptCommand {
            profile_id: profile.id.clone(),
        })
        .expect("prior complete-document export receipt");
    assert!(
        source
            .get_wallet_backup_receipt()
            .execute(WalletBackupReceiptCommand {
                profile_id: profile.id.clone(),
            })
            .expect("source receipt query")
            .is_some()
    );
    source
        .initialize_wallet_security()
        .execute(WalletProfileSecurityCommand {
            profile_id: profile.id.clone(),
        })
        .expect("source custody");
    let account = source
        .derive_wallet_account()
        .execute(DeriveWalletAccountCommand {
            profile_id: profile.id.clone(),
            account_index: 0,
            address_index: 0,
        })
        .expect("source account");
    let did = source
        .create_did()
        .execute(CreateDidCommand {
            profile_id: profile.id.clone(),
            network: "undeployed".to_owned(),
        })
        .expect("source DID");
    let authentication_method = did
        .document
        .relationships
        .iter()
        .find(|relationship| relationship.relationship == "authentication")
        .and_then(|relationship| relationship.method_ids.first())
        .cloned()
        .expect("authentication method");
    let holder_method = did
        .document
        .verification_methods
        .iter()
        .find(|method| method.public_key_jwk.curve == "Jubjub")
        .map(|method| method.id.clone())
        .expect("holder method");
    let issuance = block_on(source.prepare_credential_issuance().execute(
        PrepareCredentialIssuanceCommand {
            profile_id: profile.id.clone(),
            offer: standalone_oid4vci_offer(),
        },
    ))
    .expect("issuance plan");
    block_on(
        source
            .accept_credential_issuance()
            .execute(AcceptCredentialIssuanceCommand {
                profile_id: profile.id.clone(),
                issuance_id: issuance.id,
                holder_did: did.document.id.clone(),
                method_id: authentication_method,
                holder_binding_method_id: holder_method,
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
            }),
    )
    .expect("source credential");

    let backup = source
        .export_complete_wallet_backup()
        .execute(ExportCompleteWalletBackupCommand {
            profile_id: profile.id.clone(),
            recovery_secret: WalletRecoverySecret::parse("standalone complete recovery secret")
                .expect("recovery secret"),
            confirmation: SensitiveOperationConfirmation {
                title: EXPORT_COMPLETE_WALLET_BACKUP_TITLE.to_owned(),
                summary: EXPORT_COMPLETE_WALLET_BACKUP_SUMMARY.to_owned(),
                confirmed: true,
            },
        })
        .expect("complete backup");

    let destination = compose_in_memory();
    let summary = destination
        .recover_complete_wallet_backup()
        .execute(RecoverCompleteWalletBackupCommand {
            expected_profile_id: None,
            backup,
            recovery_secret: WalletRecoverySecret::parse("standalone complete recovery secret")
                .expect("recovery secret"),
            confirmation: SensitiveOperationConfirmation {
                title: RECOVER_COMPLETE_WALLET_BACKUP_TITLE.to_owned(),
                summary: RECOVER_COMPLETE_WALLET_BACKUP_SUMMARY.to_owned(),
                confirmed: true,
            },
        })
        .expect("fresh-instance recovery");

    assert_eq!(summary.profile_id, profile.id);
    assert!(summary.restored_key_count >= 4);
    assert_eq!(summary.restored_did_count, 1);
    assert_eq!(summary.restored_credential_count, 1);
    assert_eq!(
        destination
            .get_wallet_backup_receipt()
            .execute(WalletBackupReceiptCommand {
                profile_id: summary.profile_id.clone(),
            })
            .expect("recovered receipt query"),
        None
    );
    assert_eq!(
        destination
            .get_active_wallet_profile()
            .execute()
            .expect("active profile")
            .expect("recovered profile"),
        profile
    );
    assert!(
        destination
            .get_wallet_account()
            .execute(WalletAccountQuery {
                profile_id: summary.profile_id.clone(),
            })
            .expect("recovered account")
            .addresses
            .contains(&account.receive_address)
    );
    assert_eq!(
        destination
            .list_did_records()
            .execute(ListDidRecordsQuery {
                profile_id: summary.profile_id.clone(),
            })
            .expect("recovered DIDs")
            .len(),
        1
    );
    assert_eq!(
        destination
            .list_credentials()
            .execute(CredentialProfileQuery {
                profile_id: summary.profile_id.clone(),
            })
            .expect("recovered credentials")
            .len(),
        1
    );
}
