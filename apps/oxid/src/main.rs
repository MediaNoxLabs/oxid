// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

fn main() {
    #[cfg(feature = "standalone-development")]
    let application = oxid_composition::compose_headless();
    #[cfg(not(feature = "standalone-development"))]
    let application = oxid_composition::compose();
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
        oxid_ui_dioxus::WalletTransactionUiServices::new(
            application.prepare_wallet_transfer(),
            application.authorize_wallet_transfer(),
            application.submit_wallet_transfer(),
            application.get_wallet_transfer_draft(),
        ),
    );

    dioxus::LaunchBuilder::new()
        .with_context(ui)
        .launch(oxid_ui_dioxus::App);
}
