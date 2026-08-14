// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

fn main() {
    #[cfg(all(feature = "standalone-development", not(target_arch = "wasm32")))]
    let application = oxid_composition::compose_headless_from_environment()
        .unwrap_or_else(|error| panic!("standalone wallet configuration is invalid: {error}"));
    #[cfg(all(feature = "standalone-development", target_arch = "wasm32"))]
    let application = oxid_composition::compose_headless();
    #[cfg(not(feature = "standalone-development"))]
    let application = oxid_composition::compose();
    #[cfg(feature = "standalone-development")]
    let standalone_credential_offer = Some(oxid_composition::standalone_oid4vci_offer());
    #[cfg(not(feature = "standalone-development"))]
    let standalone_credential_offer = None;
    #[cfg(feature = "standalone-development")]
    let standalone_self_issued_request = Some(oxid_composition::standalone_siopv2_request());
    #[cfg(not(feature = "standalone-development"))]
    let standalone_self_issued_request = None;
    #[cfg(feature = "standalone-development")]
    let standalone_openid4vp_request = Some(oxid_composition::standalone_openid4vp_request());
    #[cfg(not(feature = "standalone-development"))]
    let standalone_openid4vp_request = None;
    let ui = oxid_ui_dioxus::WalletUiServices::new(
        oxid_ui_dioxus::WalletProfileUiServices::new(
            application.create_wallet_profile(),
            application.list_wallet_profiles(),
            application.select_wallet_profile(),
            application.get_active_wallet_profile(),
        ),
        oxid_ui_dioxus::WalletSecurityUiServices::new(
            application.get_wallet_security_status(),
            application.initialize_wallet_security(),
            application.unlock_wallet(),
            application.lock_wallet(),
        ),
        oxid_ui_dioxus::WalletAccountUiServices::new(
            application.list_wallet_networks(),
            application.select_wallet_network(),
            application.derive_wallet_account(),
            application.get_wallet_account(),
            application.sync_wallet_account(),
        ),
        oxid_ui_dioxus::WalletOperationalUiServices::new(
            oxid_ui_dioxus::WalletDustSyncUiServices::new(
                application.get_wallet_dust_sync_status(),
                application.start_wallet_dust_sync(),
                application.cancel_wallet_dust_sync(),
            ),
            oxid_ui_dioxus::WalletShieldedSyncUiServices::new(
                application.get_wallet_shielded_sync_status(),
                application.start_wallet_shielded_sync(),
                application.cancel_wallet_shielded_sync(),
            ),
            oxid_ui_dioxus::WalletTransactionUiServices::new(
                application.prepare_wallet_transfer(),
                application.authorize_wallet_transfer(),
                application.submit_wallet_transfer(),
                application.get_wallet_transfer_draft(),
                application.get_wallet_transfer_submission_status(),
                application.cancel_wallet_transfer_submission(),
                oxid_ui_dioxus::WalletTransactionRecoveryUiServices::new(
                    application.list_wallet_transfer_submissions(),
                    application.reconcile_wallet_transfer_submission(),
                ),
            ),
            oxid_ui_dioxus::PassportVaultUiServices::new(
                application.list_passport_vault_locks(),
                application.create_passport_vault_lock(),
                application.deposit_passport_vault_lock(),
                application.claim_passport_vault_lock(),
                application.withdraw_passport_vault_lock(),
                application.passport_vault_state_persistence(),
                oxid_ui_dioxus::PassportVaultContractCallUiServices::new(
                    application.read_passport_vault_contract_state(),
                    application.prepare_passport_vault_call(),
                    application.authorize_passport_vault_call(),
                    application.submit_passport_vault_call(),
                    oxid_ui_dioxus::PassportVaultContractCallRecoveryUiServices::new(
                        application.get_passport_vault_call(),
                        application.get_passport_vault_call_submission_status(),
                        application.cancel_passport_vault_call_submission(),
                        application.list_passport_vault_call_submissions(),
                        application.reconcile_passport_vault_call_submission(),
                    ),
                    application.passport_vault_call_mode(),
                    application
                        .passport_vault_call_contract_address_hex()
                        .map(str::to_owned),
                ),
            ),
        ),
        oxid_ui_dioxus::IdentityUiServices::new(
            oxid_ui_dioxus::DidUiServices::new(
                application.create_did(),
                application.resolve_did(),
                application.list_did_records(),
                application.update_did(),
                application.deactivate_did(),
                application.sign_did_payload(),
                application.forget_did(),
            ),
            oxid_ui_dioxus::CredentialUiServices::new(
                oxid_ui_dioxus::CredentialInventoryUiServices::new(
                    application.receive_credential(),
                    application.list_credentials(),
                    application.get_credential(),
                    application.reverify_credential(),
                    application.delete_credential(),
                ),
                oxid_ui_dioxus::CredentialIssuanceUiServices::new(
                    application.prepare_credential_issuance(),
                    application.accept_credential_issuance(),
                    application.refuse_credential_issuance(),
                    standalone_credential_offer,
                ),
                oxid_ui_dioxus::CredentialPresentationUiServices::new(
                    application.prepare_credential_presentation(),
                    application.accept_credential_presentation(),
                    application.refuse_credential_presentation(),
                    standalone_openid4vp_request,
                ),
                oxid_ui_dioxus::CredentialDisclosureUiServices::new(
                    application.get_credential_disclosure(),
                    application.preview_credential_disclosure(),
                    application.reveal_credential_claim(),
                ),
            ),
            oxid_ui_dioxus::SelfIssuedAuthenticationUiServices::new(
                application.prepare_self_issued_authentication(),
                application.accept_self_issued_authentication(),
                application.refuse_self_issued_authentication(),
                standalone_self_issued_request,
            ),
        ),
    );

    dioxus::LaunchBuilder::new()
        .with_context(ui)
        .launch(oxid_ui_dioxus::App);
}
