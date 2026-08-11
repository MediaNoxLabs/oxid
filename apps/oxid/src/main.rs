// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

fn main() {
    dioxus::LaunchBuilder::new()
        .with_context(oxid_composition::compose())
        .launch(oxid_ui_dioxus::App);
}
