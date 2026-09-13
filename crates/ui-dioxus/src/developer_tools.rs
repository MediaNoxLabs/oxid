// SPDX-License-Identifier: Apache-2.0

use dioxus::prelude::*;

#[cfg(feature = "proof-benchmark")]
use super::proof_benchmark::ProofBenchmarkPanel;
use super::{Route, WalletUiServices};

pub(super) const DEVELOPER_SECTIONS: [(Route, &str); 3] = [
    (Route::DeveloperManifest, "Capabilities"),
    (Route::DeveloperProofBenchmark, "Benchmark"),
    (Route::DeveloperDiagnostics, "Event log"),
];

pub(super) const fn is_developer_route(route: Route) -> bool {
    matches!(
        route,
        Route::Developer
            | Route::DeveloperManifest
            | Route::DeveloperProofBenchmark
            | Route::DeveloperDiagnostics
    )
}

pub(super) const fn is_developer_section(route: Route) -> bool {
    matches!(
        route,
        Route::DeveloperManifest | Route::DeveloperProofBenchmark | Route::DeveloperDiagnostics
    )
}

const SWIPE_THRESHOLD_PX: f64 = 48.0;

pub(super) fn developer_section_index(route: Route) -> usize {
    DEVELOPER_SECTIONS
        .iter()
        .position(|(section, _)| *section == route)
        .unwrap_or(0)
}

pub(super) fn developer_section_after_swipe(
    current: Route,
    start_x: f64,
    end_x: f64,
) -> Option<Route> {
    let distance = end_x - start_x;
    if distance.abs() < SWIPE_THRESHOLD_PX {
        return None;
    }

    let current_index = DEVELOPER_SECTIONS
        .iter()
        .position(|(route, _)| *route == current)?;
    let next_index = if distance < 0.0 {
        current_index.checked_add(1)
    } else {
        current_index.checked_sub(1)
    }?;
    DEVELOPER_SECTIONS.get(next_index).map(|(route, _)| *route)
}

#[component]
pub(super) fn DeveloperSectionNav(current: Route, on_select: EventHandler<Route>) -> Element {
    rsx! {
        nav { class: "developer-section-nav", aria_label: "Developer tool sections",
            for (route, label) in DEVELOPER_SECTIONS {
                button {
                    class: if current == route { "developer-section-nav__item active" } else { "developer-section-nav__item" },
                    r#type: "button",
                    aria_current: if current == route { "page" } else { "false" },
                    onclick: move |_| on_select.call(route),
                    "{label}"
                }
            }
        }
    }
}

#[component]
pub(super) fn DeveloperSectionPager(
    current: Route,
    on_select: EventHandler<Route>,
    children: Element,
) -> Element {
    let mut swipe_start = use_signal(|| None::<f64>);
    let mut wheel_offset = use_signal(|| 0.0_f64);
    rsx! {
        div {
            class: "developer-section-pager",
            style: "--developer-section-index: {developer_section_index(current)};",
            aria_label: "Developer tool pages",
            role: "region",
            onpointerdown: move |event| swipe_start.set(Some(event.client_coordinates().x)),
            onpointerup: move |event| {
                if let Some(start_x) = swipe_start.take() {
                    if let Some(route) = developer_section_after_swipe(current, start_x, event.client_coordinates().x) {
                        on_select.call(route);
                    }
                }
            },
            onpointercancel: move |_| swipe_start.set(None),
            onwheel: move |event| {
                let offset = *wheel_offset.read() + event.delta().strip_units().x;
                if let Some(route) = developer_section_after_swipe(current, 0.0, -offset) {
                    wheel_offset.set(0.0);
                    on_select.call(route);
                } else if offset.abs() >= SWIPE_THRESHOLD_PX {
                    wheel_offset.set(0.0);
                } else {
                    wheel_offset.set(offset);
                }
            },
            div { class: "developer-section-pager__track", {children} }
        }
    }
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
                purpose: "See what this build can do and which operations are available.",
                help: Some("An inventory of the operations included in this build and whether each one is ready. It helps developers understand how the app was composed; it does not grant permission, reveal wallet data, or confirm that a remote service is healthy right now."),
                availability: format!("{ready} of {} methods ready", capabilities.len()),
                action: "Open manifest",
                on_open: on_open_manifest,
            }
            DeveloperToolLink {
                title: "Proof benchmark",
                purpose: "Run synthetic proofs and inspect process-local results.",
                help: None,
                availability: if cfg!(feature = "proof-benchmark") { "Available in this development build".to_owned() } else { "Not compiled into this build".to_owned() },
                action: "Open benchmark",
                on_open: on_open_benchmark,
            }
            DeveloperToolLink {
                title: "Event log",
                purpose: "Filter and clear bounded, payload-free diagnostic events.",
                help: None,
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
    help: Option<&'static str>,
    availability: String,
    action: &'static str,
    on_open: EventHandler<MouseEvent>,
) -> Element {
    let mut help_open = use_signal(|| false);
    rsx! {
        article { class: "developer-tool surface-card",
            div {
                div { class: "developer-tool__title-row",
                    h2 { "{title}" }
                    if help.is_some() {
                        button {
                            class: "developer-tool__help-button",
                            r#type: "button",
                            aria_label: "About {title}",
                            aria_expanded: if *help_open.read() { "true" } else { "false" },
                            title: "What is this?",
                            onclick: move |_| {
                                let next = !*help_open.read();
                                help_open.set(next);
                            },
                            span { aria_hidden: "true", "?" }
                        }
                    }
                }
                p { "{purpose}" }
                if *help_open.read() {
                    if let Some(help) = help {
                        p { class: "developer-tool__help-copy", role: "note", "{help}" }
                    }
                }
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
                "A read-only inventory of the operations included in this build and whether each one is available. It is rendered from the same app-owned manifest serialized by system.capabilities; it does not grant permission or expose wallet data."
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BASE_STYLES;

    #[test]
    fn pager_swipes_snap_sections_without_affecting_primary_routes() {
        assert_eq!(
            DEVELOPER_SECTIONS,
            [
                (Route::DeveloperManifest, "Capabilities"),
                (Route::DeveloperProofBenchmark, "Benchmark"),
                (Route::DeveloperDiagnostics, "Event log"),
            ]
        );
        assert_eq!(
            developer_section_after_swipe(Route::DeveloperManifest, 200.0, 100.0),
            Some(Route::DeveloperProofBenchmark)
        );
        assert_eq!(
            developer_section_after_swipe(Route::DeveloperProofBenchmark, 100.0, 200.0),
            Some(Route::DeveloperManifest)
        );
        assert_eq!(
            developer_section_after_swipe(Route::DeveloperDiagnostics, 200.0, 100.0),
            None
        );
        assert_eq!(
            developer_section_after_swipe(Route::DeveloperManifest, 100.0, 130.0),
            None
        );
        assert_eq!(developer_section_index(Route::DeveloperProofBenchmark), 1);

        let pager = BASE_STYLES
            .split(".developer-section-pager {")
            .nth(1)
            .and_then(|styles| styles.split('}').next())
            .expect("developer section pager rule");
        assert!(pager.contains("scroll-snap-type: x mandatory;"));
        assert!(pager.contains("overscroll-behavior-x: contain;"));
        assert!(BASE_STYLES.contains(".developer-section-pager__page"));
        assert!(BASE_STYLES.contains("scroll-snap-stop: always;"));
        assert!(BASE_STYLES.contains("@media (prefers-reduced-motion: reduce)"));
        assert!(!BASE_STYLES.contains(".bottom-nav {\n  scroll-snap-type"));

        let phone_rules = BASE_STYLES
            .split("@media (max-width: 30rem) {")
            .nth(1)
            .expect("phone-width rules");
        let benchmark_row = phone_rules
            .split(".proof-benchmark-row.capability-row {")
            .nth(1)
            .and_then(|styles| styles.split('}').next())
            .expect("phone-width benchmark row rule");
        assert!(benchmark_row.contains("grid-template-columns: minmax(0, 1fr) auto auto;"));
        assert!(phone_rules.contains("grid-template-columns: auto minmax(0, 1fr) minmax(0, 1fr);"));
        // 360px and 390px both select the compact (max-width: 30rem) contract.
        assert!(360 < 30 * 16 && 390 < 30 * 16);
    }
}
