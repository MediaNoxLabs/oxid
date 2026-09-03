// SPDX-License-Identifier: Apache-2.0

use dioxus::prelude::*;
#[cfg(test)]
use oxid_capabilities_application::DeploymentServiceSnapshot;
use oxid_capabilities_application::{DeploymentProfileView, DeploymentServiceReadiness};

use super::{WalletUiServices, run_ui_blocking};

const STANDALONE_DEPLOYMENT_PROFILE_MARKER: &str = "OXID_STANDALONE_DEPLOYMENT_PROFILE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeploymentProfileCardState {
    Loading,
    Ready(DeploymentProfileView),
    Unavailable,
}

#[component]
pub(super) fn DeploymentProfileCard() -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| DeploymentProfileCardState::Loading);
    let mut refresh = use_signal(|| 0_u64);
    let profile = services.deployment_profile();
    use_effect(move || {
        let _generation = refresh();
        state.set(DeploymentProfileCardState::Loading);
        let Some(profile) = profile.clone() else {
            state.set(DeploymentProfileCardState::Unavailable);
            return;
        };
        spawn(async move {
            let result = run_ui_blocking(move || profile.execute()).await;
            state.set(result.map_or(
                DeploymentProfileCardState::Unavailable,
                DeploymentProfileCardState::Ready,
            ));
        });
    });

    match state() {
        DeploymentProfileCardState::Loading => rsx! {
            article {
                class: "settings-card surface-card",
                role: "status",
                aria_busy: "true",
                "data-profile-boundary": STANDALONE_DEPLOYMENT_PROFILE_MARKER,
                div {
                    p { class: "card-eyebrow", "Standalone deployment" }
                    h2 { "Checking selected services" }
                    p { "Resolving only the routes authenticated into this build." }
                }
                span { class: "status-pill", "Checking" }
            }
        },
        DeploymentProfileCardState::Unavailable => rsx! {
            article {
                class: "settings-card surface-card",
                role: "alert",
                "data-profile-boundary": STANDALONE_DEPLOYMENT_PROFILE_MARKER,
                div {
                    p { class: "card-eyebrow", "Standalone deployment" }
                    h2 { "Profile check unavailable" }
                    p { "The app did not change routes. Verify the selected launcher profile and retry." }
                }
                button {
                    class: "secondary-action",
                    r#type: "button",
                    onclick: move |_| refresh += 1,
                    "Retry"
                }
            }
        },
        DeploymentProfileCardState::Ready(view) => {
            let services = view.services();
            rsx! {
                article {
                    class: "settings-card surface-card",
                    "data-profile-boundary": STANDALONE_DEPLOYMENT_PROFILE_MARKER,
                    div {
                        p { class: "card-eyebrow", "Standalone deployment" }
                        h2 { "{view.label()}" }
                        p {
                            "Profile {view.profile_id()} · network {view.network_id()} · route class {view.route_class().as_str()}"
                        }
                        div { class: "developer-capability-list", aria_label: "Standalone service readiness",
                            {readiness_row("Indexer", services.indexer())}
                            {readiness_row("Node", services.node())}
                            {readiness_row("Prover", services.prover())}
                            {readiness_row("SSI", services.ssi())}
                        }
                        small {
                            "Checks use the profile already selected at build time. No endpoint or Tailnet peer can be selected here."
                        }
                    }
                    button {
                        class: "secondary-action",
                        r#type: "button",
                        onclick: move |_| refresh += 1,
                        "Check again"
                    }
                }
            }
        }
    }
}

fn readiness_row(label: &'static str, readiness: DeploymentServiceReadiness) -> Element {
    rsx! {
        div { class: "developer-capability-row capability-row",
            span { class: readiness_dot_class(readiness) }
            div { class: "developer-capability-row__body",
                strong { "{label}" }
                code { "status={readiness.as_str()}" }
            }
        }
    }
}

const fn readiness_dot_class(readiness: DeploymentServiceReadiness) -> &'static str {
    match readiness {
        DeploymentServiceReadiness::Ready => "capability-dot ready",
        DeploymentServiceReadiness::Unavailable | DeploymentServiceReadiness::NotConfigured => {
            "capability-dot queued"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_service_state_has_a_closed_public_label() {
        let snapshot = DeploymentServiceSnapshot::new(
            DeploymentServiceReadiness::Ready,
            DeploymentServiceReadiness::Unavailable,
            DeploymentServiceReadiness::NotConfigured,
            DeploymentServiceReadiness::Ready,
        );
        assert_eq!(snapshot.indexer().as_str(), "ready");
        assert_eq!(snapshot.node().as_str(), "unavailable");
        assert_eq!(snapshot.prover().as_str(), "not_configured");
        assert_eq!(readiness_dot_class(snapshot.ssi()), "capability-dot ready");
    }
}
