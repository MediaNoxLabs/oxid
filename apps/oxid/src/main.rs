// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

fn main() {
    #[cfg(feature = "standalone-development")]
    let application = oxid_composition::compose_headless();
    #[cfg(not(feature = "standalone-development"))]
    let application = oxid_composition::compose();
    #[cfg(feature = "standalone-development")]
    let standalone_credential_offer = Some(oxid_composition::standalone_oid4vci_offer());
    #[cfg(not(feature = "standalone-development"))]
    let standalone_credential_offer = None;
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
                application.receive_credential(),
                application.list_credentials(),
                application.get_credential(),
                application.reverify_credential(),
                application.delete_credential(),
                oxid_ui_dioxus::CredentialIssuanceUiServices::new(
                    application.prepare_credential_issuance(),
                    application.accept_credential_issuance(),
                    application.refuse_credential_issuance(),
                    standalone_credential_offer,
                ),
            ),
        ),
    );

    dioxus::LaunchBuilder::new()
        .with_context(ui)
        .launch(oxid_ui_dioxus::App);
}
