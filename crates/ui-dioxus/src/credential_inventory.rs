// SPDX-License-Identifier: Apache-2.0

use super::*;

#[component]
pub(super) fn CredentialInventoryCard(
    credential: CredentialView,
    item_index: usize,
    on_open: EventHandler<MouseEvent>,
) -> Element {
    let issuer = truncate_middle(&credential.issuer_did, 20, 12);
    let outcome = credential.verification_outcome.clone();
    let status_class = if outcome == "valid" {
        "status-pill success"
    } else {
        "status-pill warning"
    };
    let issued = credential.issued_at_ms.map_or_else(
        || "Issue date not supplied".to_owned(),
        |timestamp| ui::format_epoch_millis(timestamp),
    );
    rsx! {
        button {
            class: "credential-inventory-card",
            r#type: "button",
            "data-testid": "identity-document-item-{item_index}",
            "data-ui-primitive": "CredentialCard",
            aria_label: "Open {credential.display_name} document details",
            onclick: move |event| on_open.call(event),
            div { class: "credential-inventory-card__heading",
                div {
                    p { class: "card-eyebrow", "{ui::credential_format(&credential.format)}" }
                    h2 { "{credential.display_name}" }
                }
                span { class: status_class, "{ui::verification_outcome(&outcome)}" }
            }
            dl { class: "credential-inventory-card__facts",
                div { dt { "Issuer" } dd { title: "{credential.issuer_did}", "{issuer}" } }
                div { dt { "Issued" } dd { "{issued}" } }
            }
            span { class: "credential-inventory-card__action", "View details" }
        }
    }
}
