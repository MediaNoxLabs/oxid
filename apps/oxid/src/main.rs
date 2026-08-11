// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

fn main() {
    let application = oxid_composition::compose();
    let ui = oxid_ui_dioxus::WalletUiServices::new(application.create_wallet_profile());

    dioxus::LaunchBuilder::new()
        .with_context(ui)
        .launch(oxid_ui_dioxus::App);
}
