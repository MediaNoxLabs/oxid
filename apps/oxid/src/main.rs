// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(test)]
mod portal_profile_authority;

mod generated_brand {
    include!(concat!(env!("OUT_DIR"), "/brand.rs"));
}

#[cfg(feature = "developer-proof-benchmark")]
fn development_proof_cache_directory() -> std::path::PathBuf {
    #[cfg(target_os = "android")]
    {
        return std::path::PathBuf::from("/data/data/io.medianox.oxid/cache")
            .join("midnight-proof-benchmark");
    }
    #[cfg(not(target_os = "android"))]
    std::env::temp_dir().join("oxid-midnight-proof-benchmark")
}

#[cfg(any(
    feature = "standalone-portal",
    feature = "standalone-portal-tailnet",
    feature = "preprod-observation"
))]
fn startup_failure(error: impl std::fmt::Display) -> ! {
    eprintln!("Oxid startup failed: {error}");
    std::process::exit(2)
}

fn main() {
    #[cfg(all(feature = "developer-proof-benchmark", target_arch = "wasm32"))]
    compile_error!("developer-proof-benchmark is available only on native targets");

    #[cfg(all(
        feature = "desktop-portal-test",
        not(all(target_os = "macos", target_arch = "aarch64"))
    ))]
    compile_error!("desktop-portal-test is available only on ARM64 macOS");

    #[cfg(all(
        feature = "desktop-portal-test",
        any(
            feature = "mobile",
            feature = "web",
            feature = "standalone-local",
            feature = "standalone-tailnet",
            feature = "standalone-portal",
            feature = "standalone-portal-tailnet",
            feature = "standalone-native-custody",
            feature = "preprod-observation",
            feature = "ui-profile-dev",
            feature = "ui-profile-demo"
        )
    ))]
    compile_error!("desktop-portal-test is an isolated test-only desktop profile");

    #[cfg(all(
        feature = "preprod-observation",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    compile_error!("preprod-observation is available only on iOS and Android");

    #[cfg(all(
        feature = "preprod-observation",
        any(
            feature = "standalone-development",
            feature = "standalone-native-custody",
            feature = "standalone-local",
            feature = "standalone-tailnet",
            feature = "standalone-portal",
            feature = "standalone-portal-tailnet",
            feature = "standalone-native-proving-artifacts",
            feature = "ui-profile-dev",
            feature = "ui-profile-demo"
        )
    ))]
    compile_error!("preprod-observation is an isolated owner profile");

    #[cfg(all(
        feature = "android-jni-exception-recovery-test",
        not(target_os = "android")
    ))]
    compile_error!("android-jni-exception-recovery-test is available only on Android");

    #[cfg(all(
        feature = "standalone-portal",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    compile_error!("standalone-portal is available only on iOS and Android");

    #[cfg(all(
        feature = "standalone-portal",
        not(oxid_portal_virtual_device_profile_authorized)
    ))]
    compile_error!(
        "standalone-portal requires the repository-authorized iOS Simulator/Android QEMU build profile"
    );

    #[cfg(all(feature = "standalone-portal", target_arch = "wasm32"))]
    compile_error!("standalone-portal is unavailable on WASM");

    #[cfg(all(
        feature = "standalone-portal",
        any(feature = "standalone-tailnet", feature = "standalone-native-custody")
    ))]
    compile_error!("standalone-portal is incompatible with tailnet and native custody");

    #[cfg(all(feature = "standalone-portal-tailnet", not(target_os = "android")))]
    compile_error!("standalone-portal-tailnet is available only on Android");

    #[cfg(all(
        feature = "standalone-portal-tailnet",
        not(oxid_portal_android_physical_profile_authorized)
    ))]
    compile_error!(
        "standalone-portal-tailnet requires repository physical-device profile authority"
    );

    #[cfg(all(
        feature = "standalone-portal-tailnet",
        any(
            feature = "standalone-portal",
            feature = "standalone-local",
            feature = "standalone-native-custody"
        )
    ))]
    compile_error!(
        "standalone-portal-tailnet is incompatible with local Portal, local routes, and native custody"
    );

    #[cfg(all(feature = "android-jni-exception-recovery-test", target_os = "android"))]
    oxid_composition::verify_android_jni_exception_recovery()
        .unwrap_or_else(|_| panic!("Android JNI exception recovery smoke probe failed"));

    #[cfg(all(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    ))]
    compile_error!("select exactly one standalone custody feature");

    #[cfg(all(
        feature = "ui-profile-dev",
        not(any(
            feature = "standalone-development",
            feature = "standalone-native-custody"
        ))
    ))]
    compile_error!("ui-profile-dev requires an explicit standalone composition");

    #[cfg(all(feature = "ui-profile-demo", not(feature = "standalone-development")))]
    compile_error!("ui-profile-demo requires standalone-development");

    #[cfg(all(
        feature = "ui-profile-demo",
        any(feature = "standalone-local", feature = "standalone-tailnet")
    ))]
    compile_error!("ui-profile-demo requires deterministic standalone-development routes");

    #[cfg(all(feature = "ui-profile-dev", feature = "ui-profile-demo"))]
    compile_error!("select at most one non-user UI profile");

    #[cfg(all(feature = "standalone-local", not(feature = "standalone-development")))]
    compile_error!("standalone-local requires standalone-development");

    #[cfg(all(feature = "standalone-local", feature = "standalone-native-custody"))]
    compile_error!("standalone-local is incompatible with native custody");

    #[cfg(all(feature = "standalone-local", feature = "standalone-tailnet"))]
    compile_error!("select at most one live standalone route profile");

    #[cfg(all(feature = "standalone-local", target_arch = "wasm32"))]
    compile_error!("standalone-local is available only on native targets");

    #[cfg(all(
        feature = "standalone-tailnet",
        not(all(
            feature = "standalone-development",
            any(target_os = "ios", target_os = "android")
        ))
    ))]
    compile_error!("standalone-tailnet requires standalone-development on iOS or Android");

    #[cfg(all(
        feature = "standalone-native-proving-artifacts",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    compile_error!("standalone-native-proving-artifacts is available only on iOS and Android");

    #[cfg(all(feature = "standalone-portal", target_os = "android"))]
    oxid_composition::verify_android_portal_virtual_device_profile()
        .unwrap_or_else(|error| startup_failure(error));

    #[cfg(feature = "desktop-portal-test")]
    let application = {
        const OXID_DESKTOP_PORTAL_TEST_PROFILE: &str = "OXID_DESKTOP_PORTAL_TEST_PROFILE";
        let _ = OXID_DESKTOP_PORTAL_TEST_PROFILE;
        oxid_composition::compose_native_desktop_test_from_environment()
            .unwrap_or_else(|error| panic!("desktop Portal test configuration is invalid: {error}"))
    };

    #[cfg(all(
        feature = "standalone-portal",
        feature = "standalone-development",
        feature = "standalone-local",
        not(feature = "standalone-tailnet"),
        not(feature = "standalone-native-custody"),
        not(target_arch = "wasm32"),
        any(target_os = "ios", target_os = "android")
    ))]
    let application = {
        const OXID_STANDALONE_PORTAL_PROFILE: &str = "OXID_STANDALONE_PORTAL_PROFILE";
        let _ = OXID_STANDALONE_PORTAL_PROFILE;
        oxid_composition::compose_mobile_public_genesis_portal_standalone_from_routes(
            "ws://127.0.0.1:8088/api/v4/graphql/ws",
            "http://127.0.0.1:8088/api/v4/graphql",
            "ws://127.0.0.1:9944",
            "http://127.0.0.1:6300",
            include_bytes!(concat!(env!("OUT_DIR"), "/portal-deployment.json")),
            env!("OXID_EMBEDDED_PORTAL_DEPLOYMENT_SHA256"),
        )
        .unwrap_or_else(|error| startup_failure(error))
    };

    #[cfg(all(
        feature = "standalone-portal-tailnet",
        feature = "standalone-development",
        not(feature = "standalone-portal"),
        not(feature = "standalone-local"),
        feature = "standalone-tailnet",
        not(feature = "standalone-native-custody"),
        target_os = "android",
        not(target_arch = "wasm32")
    ))]
    let application = oxid_composition::compose_mobile_public_genesis_portal_tailnet_from_routes(
        env!("OXID_BUILD_MIDNIGHT_INDEXER_WS_URL"),
        env!("OXID_BUILD_MIDNIGHT_INDEXER_HTTP_URL"),
        env!("OXID_BUILD_MIDNIGHT_NODE_WS_URL"),
        env!("OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL"),
        include_bytes!(concat!(env!("OUT_DIR"), "/portal-deployment.json")),
        env!("OXID_EMBEDDED_PORTAL_DEPLOYMENT_SHA256"),
        env!("OXID_EMBEDDED_PORTAL_PUBLIC_ORIGIN"),
    )
    .unwrap_or_else(|error| startup_failure(error));

    #[cfg(feature = "standalone-native-proving-artifacts")]
    let application =
        oxid_composition::compose_mobile_native_standalone_with_compact_presentation()
            .unwrap_or_else(|error| {
                panic!("embedded Compact presentation runtime is invalid: {error}")
            });
    #[cfg(all(
        feature = "preprod-observation",
        any(target_os = "ios", target_os = "android")
    ))]
    let application = oxid_composition::compose_preprod_observation()
        .unwrap_or_else(|error| startup_failure(error));
    #[cfg(all(
        feature = "standalone-native-custody",
        not(feature = "standalone-native-proving-artifacts"),
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
        feature = "standalone-tailnet",
        not(feature = "standalone-local"),
        not(feature = "standalone-portal-tailnet"),
        not(target_arch = "wasm32")
    ))]
    let application = {
        const OXID_STANDALONE_TAILNET_PROFILE: &str = "OXID_STANDALONE_TAILNET_PROFILE";
        let _ = OXID_STANDALONE_TAILNET_PROFILE;
        oxid_composition::compose_mobile_public_genesis_tailnet_standalone_from_routes(
            env!("OXID_BUILD_MIDNIGHT_INDEXER_WS_URL"),
            env!("OXID_BUILD_MIDNIGHT_INDEXER_HTTP_URL"),
            env!("OXID_BUILD_MIDNIGHT_NODE_WS_URL"),
            env!("OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL"),
        )
        .unwrap_or_else(|error| panic!("standalone wallet configuration is invalid: {error}"))
    };
    #[cfg(all(
        feature = "standalone-development",
        not(feature = "standalone-native-custody"),
        feature = "standalone-local",
        not(feature = "standalone-tailnet"),
        not(feature = "standalone-portal"),
        not(target_arch = "wasm32")
    ))]
    let application = {
        const OXID_STANDALONE_LOCAL_PROFILE: &str = "OXID_STANDALONE_LOCAL_PROFILE";
        let _ = OXID_STANDALONE_LOCAL_PROFILE;
        oxid_composition::compose_mobile_public_genesis_local_standalone_from_routes(
            "ws://127.0.0.1:8088/api/v4/graphql/ws",
            "http://127.0.0.1:8088/api/v4/graphql",
            "ws://127.0.0.1:9944",
            "http://127.0.0.1:6300",
        )
        .unwrap_or_else(|error| panic!("standalone wallet configuration is invalid: {error}"))
    };
    #[cfg(all(
        feature = "standalone-development",
        not(feature = "standalone-native-custody"),
        not(feature = "standalone-tailnet"),
        not(feature = "standalone-local"),
        not(feature = "standalone-portal-tailnet"),
        not(feature = "desktop-portal-test"),
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
        feature = "standalone-native-custody",
        feature = "preprod-observation"
    )))]
    let application = oxid_composition::compose();
    #[cfg(all(
        any(
            feature = "standalone-development",
            feature = "standalone-native-custody"
        ),
        not(feature = "standalone-portal"),
        not(feature = "standalone-portal-tailnet"),
        not(feature = "desktop-portal-test")
    ))]
    let standalone_credential_offer = Some(oxid_composition::standalone_oid4vci_offer());
    #[cfg(any(
        feature = "desktop-portal-test",
        feature = "standalone-portal",
        feature = "standalone-portal-tailnet",
        not(any(
            feature = "standalone-development",
            feature = "standalone-native-custody"
        ))
    ))]
    let standalone_credential_offer = None;
    #[cfg(any(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    ))]
    let credential_issuance_ready = true;
    #[cfg(not(any(
        feature = "standalone-development",
        feature = "standalone-native-custody"
    )))]
    let credential_issuance_ready = false;
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
    let wallet_security = oxid_ui_dioxus::WalletSecurityUiServices::new(
        application.get_wallet_security_status(),
        application.initialize_wallet_security(),
        application.unlock_wallet(),
        application.lock_wallet(),
        oxid_ui_dioxus::WalletBackupUiServices::new(
            application.recover_portable_wallet_backup(),
            application.export_complete_wallet_backup(),
            application.recover_complete_wallet_backup(),
            application.get_wallet_backup_receipt(),
            application.record_wallet_backup_receipt(),
            application.portable_wallet_backup_documents(),
        ),
    );
    #[cfg(feature = "preprod-observation")]
    let wallet_security = {
        let capability = application
            .wallet_root_recovery()
            .unwrap_or_else(|| panic!("authenticated PreProd recovery capability is unavailable"));
        wallet_security.with_root_recovery(oxid_ui_dioxus::WalletRootRecoveryUiServices::new(
            capability.network_id().to_owned(),
            capability.recover(),
        ))
    };
    let ui = oxid_ui_dioxus::WalletUiServices::new(
        oxid_ui_dioxus::WalletProfileUiServices::new(
            application.create_wallet_profile(),
            application.list_wallet_profiles(),
            application.select_wallet_profile(),
            application.get_active_wallet_profile(),
        ),
        wallet_security,
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
            oxid_ui_dioxus::WalletDustRegistrationUiServices::new(
                application.prepare_wallet_dust_registration(),
                application.authorize_wallet_dust_registration(),
                application.submit_wallet_dust_registration(),
                oxid_ui_dioxus::WalletDustRegistrationRecoveryUiServices::new(
                    application.get_wallet_dust_registration(),
                    application.get_wallet_dust_registration_status(),
                    application.cancel_wallet_dust_registration_submission(),
                    application.reconcile_wallet_dust_registration_submission(),
                ),
            ),
            oxid_ui_dioxus::WalletShieldedSyncUiServices::new(
                application.get_wallet_shielded_sync_status(),
                application.start_wallet_shielded_sync(),
                application.cancel_wallet_shielded_sync(),
            ),
            oxid_ui_dioxus::WalletTransactionUiServices::new(
                oxid_ui_dioxus::WalletTransactionPreparationUiServices::new(
                    application.prepare_wallet_transfer(),
                    application.prepare_shielded_wallet_transfer(),
                ),
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
            )
            .with_publisher(application.publish_did()),
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
                    application.list_credential_issuances(),
                    standalone_credential_offer,
                    credential_issuance_ready,
                ),
                oxid_ui_dioxus::CredentialPresentationUiServices::new(
                    application.prepare_credential_presentation(),
                    application.accept_credential_presentation(),
                    application.cancel_credential_presentation(),
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
        oxid_ui_dioxus::DiagnosticsUiServices::new(
            application.get_diagnostic_snapshot(),
            application.clear_diagnostics(),
        ),
        application.screen_privacy(),
    );
    #[cfg(any(feature = "standalone-local", feature = "standalone-tailnet"))]
    let ui = ui.with_deployment_profile(
        application
            .deployment_profile()
            .unwrap_or_else(|| panic!("standalone deployment profile is unavailable")),
    );
    #[cfg(feature = "ui-profile-dev")]
    let ui = ui.with_developer_capabilities(oxid_ui_dioxus::CapabilityManifestContext::new(
        application.compact_presentation_proof_available(),
        application.passport_vault_call_mode(),
        application.passport_vault_state_persistence(),
    ));
    #[cfg(feature = "developer-proof-benchmark")]
    let ui = ui.with_proof_benchmark(
        oxid_composition::compose_development_proof_benchmark(development_proof_cache_directory())
            .unwrap_or_else(|error| panic!("development proof benchmark is unavailable: {error}")),
    );
    #[cfg(target_os = "android")]
    let ui = ui.with_android_platform_initializer(std::sync::Arc::new(|| {
        match oxid_composition::initialize_android_tls() {
            oxid_composition::AndroidTlsInitialization::Ready => {
                oxid_ui_dioxus::AndroidPlatformInitialization::Ready
            }
            oxid_composition::AndroidTlsInitialization::Retry => {
                oxid_ui_dioxus::AndroidPlatformInitialization::Retry
            }
            oxid_composition::AndroidTlsInitialization::Failed => {
                oxid_ui_dioxus::AndroidPlatformInitialization::Failed
            }
        }
    }));

    let launcher = dioxus::LaunchBuilder::new()
        .with_context(ui)
        .with_context(generated_brand::BRAND_PROFILE);
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let app_links = application.identity_link_ingress();
        let presentation_lifecycle = application.set_credential_presentation_foreground();
        let config =
            dioxus::mobile::Config::new().with_custom_event_handler(move |event, _target| {
                match event {
                    dioxus::mobile::tao::event::Event::Opened { urls } => {
                        for url in urls {
                            // Fail closed without reproducing secret-bearing URLs in
                            // logs. The Dioxus UI drains and classifies accepted links.
                            let _ = app_links.capture(url.as_str().to_owned());
                        }
                    }
                    dioxus::mobile::tao::event::Event::Suspended => {
                        let _ = presentation_lifecycle.execute(false);
                    }
                    dioxus::mobile::tao::event::Event::Resumed => {
                        let _ = presentation_lifecycle.execute(true);
                    }
                    _ => {}
                }
            });
        launcher.with_cfg(config).launch(oxid_ui_dioxus::App);
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    launcher.launch(oxid_ui_dioxus::App);
}

#[cfg(test)]
mod tests {
    use super::generated_brand::BRAND_PROFILE;

    #[test]
    fn default_brand_pins_identity_and_security_copy() {
        assert_eq!(BRAND_PROFILE.product_name(), "Oxid");
        assert_eq!(BRAND_PROFILE.bundle_identifier(), "io.medianox.oxid");
        assert_eq!(BRAND_PROFILE.publisher(), "MediaNoxLabs");

        let copy = BRAND_PROFILE.security_copy();
        assert_eq!(
            copy.presentation_consent,
            "I consent to use the selected credential and disclose exactly these claims to this verifier."
        );
        assert_eq!(
            copy.vault_broadcast_warning,
            "Cancellation is safe only before the broadcast boundary. The wallet never blind-retries an ambiguous outcome."
        );
        assert_eq!(
            copy.complete_recovery_warning,
            "Oxid never merges this archive into existing local wallet state. Chain-derived caches and transaction history rebuild from their authoritative sources."
        );
        assert_eq!(
            copy.complete_recovery_confirmation,
            "I confirm complete recovery into this empty Oxid installation."
        );
        assert_eq!(
            copy.submission_ambiguity_warning,
            "This may have reached the network. Oxid will check before anything is sent again."
        );
        assert_eq!(
            copy.backup_receipt_failure,
            "Backup document was saved, but Oxid could not record its completion status."
        );
    }
}
