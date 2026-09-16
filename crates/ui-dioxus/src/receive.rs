// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "standalone-deployment-profile")]
use dioxus::prelude::*;
use oxid_wallet_application::{WalletAccountView, WalletAddressView};

#[cfg(feature = "standalone-deployment-profile")]
pub(crate) fn standalone_funding_action(
    deployment: Option<oxid_capabilities_application::DeploymentProfileView>,
) -> Element {
    match deployment.map(|profile| profile.route_class()) {
        Some(oxid_capabilities_application::DeploymentRouteClass::Local) => rsx! {
            div { class: "receive-sheet__funding",
                strong { "Development funding" }
                p { "Request the fixed local development grant, then sync your balance." }
                a {
                    class: "secondary-action",
                    href: "http://127.0.0.1:36301",
                    target: "_blank",
                    rel: "noreferrer",
                    "Open faucet"
                }
            }
        },
        Some(oxid_capabilities_application::DeploymentRouteClass::Tailnet) => rsx! {
            div { class: "receive-sheet__funding",
                strong { "Development funding" }
                p { "Ask the operator to use the private Tailnet faucet to fund this displayed address, then sync your balance." }
            }
        },
        None => rsx! {},
    }
}

pub(crate) fn protected_receive_addresses(
    account: &WalletAccountView,
) -> Option<&[WalletAddressView]> {
    super::has_protected_account(account).then_some(account.addresses.as_slice())
}

pub(crate) fn default_receive_kind(account: &WalletAccountView) -> Option<String> {
    protected_receive_addresses(account)
        .and_then(|addresses| addresses.first())
        .map(|address| address.kind.clone())
}

pub(crate) fn grouped_address_preview(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let visible = if characters.len() > 32 {
        let mut shortened = characters[..20].to_vec();
        shortened.extend(['…', '…', '…']);
        shortened.extend_from_slice(&characters[characters.len() - 8..]);
        shortened
    } else {
        characters
    };

    visible
        .chunks(4)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn render_qr_svg(value: &str) -> Option<String> {
    use qrcode::{QrCode, render::svg};

    QrCode::new(value.as_bytes()).ok().map(|code| {
        code.render::<svg::Color<'_>>()
            .min_dimensions(220, 220)
            .max_dimensions(280, 280)
            .quiet_zone(true)
            .dark_color(svg::Color("#07111f"))
            .light_color(svg::Color("#ffffff"))
            .build()
    })
}
