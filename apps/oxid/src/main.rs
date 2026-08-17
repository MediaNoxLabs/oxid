// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

fn main() {
    #[cfg(all(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    ))]
    compile_error!("select exactly one standalone custody feature");

    #[cfg(all(
        feature = "standalone-native-proving-artifacts",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    compile_error!("standalone-native-proving-artifacts is available only on iOS and Android");

    #[cfg(feature = "standalone-native-proving-artifacts")]
    let _presentation_artifact_identity =
        oxid_composition::authenticate_embedded_mobile_compact_presentation_artifacts()
            .unwrap_or_else(|error| {
                panic!("embedded Compact presentation artifacts are invalid: {error}")
            });

    #[cfg(all(
        feature = "standalone-native-custody",
        any(target_os = "ios", target_os = "android")
    ))]
    let application = oxid_composition::compose_mobile_native_standalone();
    #[cfg(all(
        feature = "standalone-native-custody",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    compile_error!("standalone-native-custody is available only on iOS and Android");
    #[cfg(all(
        feature = "standalone-development",
        not(feature = "standalone-native-custody"),
        not(target_arch = "wasm32")
    ))]
    let application = oxid_composition::compose_headless_from_environment()
        .unwrap_or_else(|error| panic!("standalone wallet configuration is invalid: {error}"));
    #[cfg(all(
        feature = "standalone-development",
        not(feature = "standalone-native-custody"),
        target_arch = "wasm32"
    ))]
    let application = oxid_composition::compose_headless();
    #[cfg(not(any(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    )))]
    let application = oxid_composition::compose();
    #[cfg(any(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    ))]
    let standalone_credential_offer = Some(oxid_composition::standalone_oid4vci_offer());
    #[cfg(not(any(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    )))]
    let standalone_credential_offer = None;
    #[cfg(any(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    ))]
    let standalone_self_issued_request = Some(oxid_composition::standalone_siopv2_request());
    #[cfg(not(any(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    )))]
    let standalone_self_issued_request = None;
    #[cfg(any(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    ))]
    let standalone_openid4vp_request = Some(oxid_composition::standalone_openid4vp_request());
    #[cfg(not(any(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    )))]
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
            application.public_text_exporter(),
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
            oxid_ui_dioxus::IdentityIngressUiServices::new(
                application.qr_scanner(),
                application.identity_link_ingress(),
                application.route_identity_request(),
            ),
        ),
    );

    let launcher = dioxus::LaunchBuilder::new().with_context(ui);
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let app_links = application.identity_link_ingress();
        let config =
            dioxus::mobile::Config::new().with_custom_event_handler(move |event, _target| {
                if let dioxus::mobile::tao::event::Event::Opened { urls } = event {
                    for url in urls {
                        // Fail closed without reproducing secret-bearing URLs in
                        // logs. The Dioxus UI drains and classifies accepted links.
                        let _ = app_links.capture(url.as_str().to_owned());
                    }
                }
            });
        launcher.with_cfg(config).launch(oxid_ui_dioxus::App);
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    launcher.launch(oxid_ui_dioxus::App);
}
