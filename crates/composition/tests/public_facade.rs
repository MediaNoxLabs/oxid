// SPDX-License-Identifier: Apache-2.0

use oxid_composition::*;

fn assert_clone<T: Clone>() {}
fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}

#[test]
fn application_service_getters_remain_available_at_the_root_facade() {
    assert_clone::<ApplicationServices>();
    let services = compose_in_memory();
    let _ = services.diagnostic_events();
    let _ = services.get_diagnostic_snapshot();
    let _ = services.clear_diagnostics();
    let _ = services.qr_scanner();
    let _ = services.identity_link_ingress();
    let _ = services.public_text_exporter();
    let _ = services.screen_privacy();
    let _ = services.portable_wallet_backup_documents();
    let _ = services.route_identity_request();
    let _ = services.create_wallet_profile();
    let _ = services.list_wallet_profiles();
    let _ = services.select_wallet_profile();
    let _ = services.get_active_wallet_profile();
    let _ = services.get_wallet_backup_receipt();
    let _ = services.record_wallet_backup_receipt();
    let _ = services.get_wallet_security_status();
    let _ = services.initialize_wallet_security();
    let _ = services.unlock_wallet();
    let _ = services.lock_wallet();
    let _ = services.export_portable_wallet_backup();
    let _ = services.recover_portable_wallet_backup();
    let _ = services.export_complete_wallet_backup();
    let _ = services.recover_complete_wallet_backup();
    let _ = services.generate_wallet_key();
    let _ = services.list_wallet_keys();
    let _ = services.sign_wallet_data();
    let _ = services.delete_wallet_key();
    let _ = services.list_wallet_networks();
    let _ = services.select_wallet_network();
    let _ = services.derive_wallet_account();
    let _ = services.get_wallet_account();
    let _ = services.sync_wallet_account();
    let _ = services.get_wallet_dust_sync_status();
    let _ = services.start_wallet_dust_sync();
    let _ = services.cancel_wallet_dust_sync();
    let _ = services.get_wallet_shielded_sync_status();
    let _ = services.start_wallet_shielded_sync();
    let _ = services.cancel_wallet_shielded_sync();
    let _ = services.prepare_wallet_dust_registration();
    let _ = services.authorize_wallet_dust_registration();
    let _ = services.submit_wallet_dust_registration();
    let _ = services.get_wallet_dust_registration();
    let _ = services.get_wallet_dust_registration_status();
    let _ = services.cancel_wallet_dust_registration_submission();
    let _ = services.reconcile_wallet_dust_registration_submission();
    let _ = services.prepare_wallet_transfer();
    let _ = services.prepare_shielded_wallet_transfer();
    let _ = services.authorize_wallet_transfer();
    let _ = services.submit_wallet_transfer();
    let _ = services.get_wallet_transfer_draft();
    let _ = services.get_wallet_transfer_submission_status();
    let _ = services.cancel_wallet_transfer_submission();
    let _ = services.list_wallet_transfer_submissions();
    let _ = services.reconcile_wallet_transfer_submission();
    let _ = services.resolve_did();
    let _ = services.create_did();
    let _ = services.list_did_records();
    let _ = services.get_did_record();
    let _ = services.update_did();
    let _ = services.deactivate_did();
    let _ = services.sign_did_payload();
    let _ = services.forget_did();
    let _ = services.receive_credential();
    let _ = services.list_credentials();
    let _ = services.get_credential();
    let _ = services.reverify_credential();
    let _ = services.delete_credential();
    let _ = services.get_credential_disclosure();
    let _ = services.preview_credential_disclosure();
    let _ = services.reveal_credential_claim();
    let _ = services.prepare_credential_issuance();
    let _ = services.accept_credential_issuance();
    let _ = services.refuse_credential_issuance();
    let _ = services.get_credential_issuance();
    let _ = services.list_credential_issuances();
    let _ = services.prepare_self_issued_authentication();
    let _ = services.accept_self_issued_authentication();
    let _ = services.refuse_self_issued_authentication();
    let _ = services.get_self_issued_authentication();
    let _ = services.list_self_issued_authentications();
    let _ = services.prepare_credential_presentation();
    let _ = services.accept_credential_presentation();
    let _ = services.cancel_credential_presentation();
    let _ = services.set_credential_presentation_foreground();
    let _ = services.refuse_credential_presentation();
    let _ = services.get_credential_presentation();
    let _ = services.list_credential_presentations();
    let _ = services.list_passport_vault_locks();
    let _ = services.decode_passport_vault_contract_state();
    let _ = services.read_passport_vault_contract_state();
    let _ = services.create_passport_vault_lock();
    let _ = services.deposit_passport_vault_lock();
    let _ = services.claim_passport_vault_lock();
    let _ = services.withdraw_passport_vault_lock();
    let _ = services.prepare_passport_vault_call();
    let _ = services.authorize_passport_vault_call();
    let _ = services.submit_passport_vault_call();
    let _ = services.get_passport_vault_call();
    let _ = services.get_passport_vault_call_submission_status();
    let _ = services.cancel_passport_vault_call_submission();
    let _ = services.list_passport_vault_call_submissions();
    let _ = services.reconcile_passport_vault_call_submission();
    let _ = services.passport_vault_call_mode();
    let _ = services.passport_vault_call_contract_address_hex();
    let _ = services.passport_vault_state_persistence();
    let _ = services.compact_presentation_proof_available();
}

#[test]
fn default_native_root_facade_remains_source_compatible() {
    assert_error::<HeadlessCompositionError>();
    assert_error::<ProductionDeploymentCompositionError>();
    let _ = std::mem::size_of::<AuthenticatedProductionDeployment>();
    let _: for<'a> fn(
        &'a AuthenticatedProductionDeployment,
    ) -> &'a oxid_adapter_deployment_profile::AuthenticatedDeploymentProfile =
        AuthenticatedProductionDeployment::profile;

    let _ = standalone_oid4vci_offer as fn() -> String;
    let _ = standalone_siopv2_request as fn() -> String;
    let _ = standalone_openid4vp_request as fn() -> String;
    let _ = simulated_passport_vault_contract_address_hex as fn() -> &'static str;
    let _ = compose as fn() -> ApplicationServices;
    let _ = compose_headless as fn() -> ApplicationServices;
    let _ = compose_in_memory as fn() -> ApplicationServices;
    let _: fn() -> Result<ApplicationServices, HeadlessCompositionError> =
        compose_headless_from_environment;
    let _ = authenticate_production_deployment;
    let _ = compose_authenticated_production;
    let _ = compose_headless_live_with_checkpoint_options;
    let _ = compose_headless_standalone_with_checkpoint_options;
    let _ = compose_headless_with_submission_journal;
    let _ = compose_headless_live;
    let _ = compose_headless_live_with_checkpoints;
    let _ = compose_headless_standalone;
    let _ = compose_headless_standalone_with_checkpoints;
    let _ = compose_headless_standalone_with_dust_checkpoints;
    let _ = compose_headless_standalone_with_all_checkpoints;
    #[cfg(feature = "standalone-development")]
    let _ = compose_mobile_public_genesis_standalone_from_routes;
    let _ = compose_mobile_development_standalone_from_routes;
    let _ = || compose_in_memory_with_compact_presentation_artifacts(std::path::PathBuf::new());

    for value in [
        MIDNIGHT_NETWORK_ID_ENV,
        MIDNIGHT_INDEXER_WS_URL_ENV,
        MIDNIGHT_UNSHIELDED_ADDRESS_ENV,
        MIDNIGHT_INDEXER_HTTP_URL_ENV,
        MIDNIGHT_NODE_WS_URL_ENV,
        MIDNIGHT_PROOF_SERVER_URL_ENV,
        MIDNIGHT_PROVING_CACHE_DIR_ENV,
        MIDNIGHT_ACCOUNT_CHECKPOINT_PATH_ENV,
        MIDNIGHT_DUST_CHECKPOINT_PATH_ENV,
        MIDNIGHT_SHIELDED_CHECKPOINT_PATH_ENV,
        MIDNIGHT_SUBMISSION_JOURNAL_PATH_ENV,
        MIDNIGHT_DID_RESOLVER_URL_ENV,
        PASSPORT_VAULT_DEPLOYMENT_HEIGHT_ENV,
        PASSPORT_VAULT_COMPOSER_ENV,
        PRESENTATION_COMPACT_ARTIFACTS_DIR_ENV,
        DID_STORE_PATH_ENV,
        CREDENTIAL_STORE_PATH_ENV,
        CREDENTIAL_KEY_PATH_ENV,
        OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH_ENV,
        OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256_ENV,
        PASSPORT_VAULT_STORE_PATH_ENV,
    ] {
        assert!(!value.is_empty());
    }
}

#[cfg(feature = "headless-portal-local")]
#[test]
fn headless_portal_root_entry_point_remains_available() {
    let _: fn() -> Result<ApplicationServices, HeadlessCompositionError> =
        compose_native_headless_process_from_environment;
}

#[cfg(feature = "desktop-portal-test")]
#[test]
fn desktop_portal_root_entry_point_remains_available() {
    let _: fn() -> Result<ApplicationServices, HeadlessCompositionError> =
        compose_native_desktop_test_from_environment;
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[test]
fn native_mobile_root_entry_points_remain_available() {
    let _: fn() -> ApplicationServices = compose_mobile_native_standalone;
}

#[cfg(all(
    feature = "mobile-compact-artifacts",
    any(target_os = "ios", target_os = "android")
))]
#[test]
fn compact_mobile_root_entry_points_remain_available() {
    let _ = compose_mobile_native_standalone_with_compact_presentation;
    let _ = authenticate_embedded_mobile_compact_presentation_artifacts;
}

#[cfg(all(
    feature = "mobile-portal",
    feature = "standalone-development",
    any(target_os = "ios", target_os = "android")
))]
#[test]
fn portal_mobile_root_entry_points_remain_available() {
    let _ = compose_mobile_development_portal_standalone_from_routes;
    let _ = compose_mobile_public_genesis_portal_standalone_from_routes;
}

#[cfg(all(feature = "mobile-portal", target_os = "android"))]
#[test]
fn android_portal_verification_root_entry_point_remains_available() {
    let _: fn() -> Result<(), &'static str> = verify_android_portal_virtual_device_profile;
}

#[cfg(all(
    feature = "mobile-portal-tailnet",
    feature = "standalone-development",
    target_os = "android"
))]
#[test]
fn portal_tailnet_root_entry_point_remains_available() {
    let _ = compose_mobile_development_portal_tailnet_from_routes;
    let _ = compose_mobile_public_genesis_portal_tailnet_from_routes;
}

#[cfg(all(feature = "android-jni-exception-recovery-test", target_os = "android"))]
#[test]
fn android_jni_recovery_root_entry_point_remains_available() {
    let _ = verify_android_jni_exception_recovery;
}
