// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

fn main() {
    let application = oxid_composition::compose();
    let ui = oxid_ui_dioxus::WalletUiServices::new(
        application.create_wallet_profile(),
        application.list_wallet_profiles(),
        application.select_wallet_profile(),
        application.get_active_wallet_profile(),
        application.get_wallet_security_status(),
    );

    dioxus::LaunchBuilder::new()
        .with_context(ui)
        .launch(oxid_ui_dioxus::App);
}
