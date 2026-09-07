// SPDX-License-Identifier: Apache-2.0

use dioxus::prelude::*;

#[cfg(feature = "proof-benchmark")]
use super::proof_benchmark::ProofBenchmarkPanel;
use super::{Route, WalletUiServices};

pub(super) const fn is_developer_route(route: Route) -> bool {
    matches!(
        route,
        Route::Developer
            | Route::DeveloperManifest
            | Route::DeveloperProofBenchmark
            | Route::DeveloperDiagnostics
    )
}

#[component]
pub(super) fn DeveloperToolsHub(
    on_open_manifest: EventHandler<MouseEvent>,
    on_open_benchmark: EventHandler<MouseEvent>,
    on_open_diagnostics: EventHandler<MouseEvent>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let capabilities = services.developer_capabilities();
    let ready = capabilities
        .iter()
        .filter(|capability| capability.status() == "ready")
        .count();
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Standalone developer profile" }
            h1 { "Developer tools" }
            p { "Focused, process-local tools for inspecting this development composition." }
        }
        section { class: "developer-tool-list", aria_label: "Developer tools",
            DeveloperToolLink {
                title: "Capability manifest",
                purpose: "Review public composition facts and service availability.",
                availability: format!("{ready} of {} methods ready", capabilities.len()),
                action: "Open manifest",
                on_open: on_open_manifest,
            }
            DeveloperToolLink {
                title: "Proof benchmark",
                purpose: "Run synthetic proofs and inspect process-local results.",
                availability: if cfg!(feature = "proof-benchmark") { "Available in this development build".to_owned() } else { "Not compiled into this build".to_owned() },
                action: "Open benchmark",
                on_open: on_open_benchmark,
            }
            DeveloperToolLink {
                title: "Event log",
                purpose: "Filter and clear bounded, payload-free diagnostic events.",
                availability: "Process-local · telemetry off".to_owned(),
                action: "Open event log",
                on_open: on_open_diagnostics,
            }
        }
    }
}

#[component]
fn DeveloperToolLink(
    title: &'static str,
    purpose: &'static str,
    availability: String,
    action: &'static str,
    on_open: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        article { class: "developer-tool surface-card",
            div {
                h2 { "{title}" }
                p { "{purpose}" }
                span { class: "status-pill", "{availability}" }
            }
            button { class: "secondary-button", r#type: "button", onclick: move |event| on_open.call(event), "{action}" }
        }
    }
}

#[cfg(feature = "proof-benchmark")]
#[component]
pub(super) fn DeveloperProofBenchmarkPage() -> Element {
    rsx! { ProofBenchmarkPanel {} }
}

#[cfg(not(feature = "proof-benchmark"))]
#[component]
pub(super) fn DeveloperProofBenchmarkPage() -> Element {
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Development tool" }
            h1 { "Proof benchmark" }
            p { "This development build does not include the proof benchmark capability." }
        }
    }
}

#[component]
pub(super) fn DeveloperCapabilitiesPage() -> Element {
    let services = consume_context::<WalletUiServices>();
    let capabilities = services.developer_capabilities();
    let ready = capabilities
        .iter()
        .filter(|capability| capability.status() == "ready")
        .count();
    let attention = capabilities.len().saturating_sub(ready);
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Standalone developer profile" }
            h1 { "Capability manifest" }
            p {
                "Rendered from the same Oxid-owned manifest serialized by system.capabilities. Values are public composition facts; request payloads, identifiers, claims, endpoints, logs, and process telemetry are excluded."
            }
        }
        section { class: "developer-capability-summary surface-card",
            div {
                p { class: "card-eyebrow", "Manifest snapshot" }
                h2 { "{capabilities.len()} declared methods" }
                p { "{ready} ready · {attention} queued, blocked, superseded, or composition-dependent" }
            }
            code { "source=oxid_capabilities_application freshness=composition_time cursor=not_applicable timing=not_collected" }
        }
        div { class: "developer-capability-list",
            for capability in capabilities {
                article {
                    class: "developer-capability-row capability-row",
                    key: "{capability.method()}",
                    span {
                        class: if capability.status() == "ready" { "capability-dot ready" } else { "capability-dot queued" }
                    }
                    div { class: "developer-capability-row__body",
                        strong { "{capability.method()}" }
                        code { "status={capability.status()}" }
                        if capability.facts().is_empty() {
                            small { "No additional public composition facts" }
                        } else {
                            dl { class: "developer-capability-facts",
                                for fact in capability.facts() {
                                    div { key: "{fact.key()}",
                                        dt { "{fact.key()}" }
                                        dd { code { "{fact.value().display_text()}" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
