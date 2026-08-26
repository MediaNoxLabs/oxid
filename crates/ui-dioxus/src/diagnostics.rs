// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use dioxus::prelude::*;
use oxid_diagnostics_application::{
    CLEAR_LOCAL_DIAGNOSTICS_INTENT, ClearDiagnosticsCommand, ClearDiagnosticsUseCase,
    DiagnosticSnapshotView, GetDiagnosticSnapshotUseCase,
};
use oxid_wallet_application::WalletProfileView;

use super::labels as ui;
use super::{AccountPageState, WalletUiServices, load_account_page, run_ui_blocking};

/// Process-local, payload-free diagnostic use cases consumed by the
/// Diagnostics page.
pub struct DiagnosticsUiServices {
    pub(super) get: Arc<dyn GetDiagnosticSnapshotUseCase>,
    pub(super) clear: Arc<dyn ClearDiagnosticsUseCase>,
}

impl DiagnosticsUiServices {
    #[must_use]
    pub const fn new(
        get: Arc<dyn GetDiagnosticSnapshotUseCase>,
        clear: Arc<dyn ClearDiagnosticsUseCase>,
    ) -> Self {
        Self { get, clear }
    }
}

#[derive(Clone)]
enum LocalDiagnosticsPageState {
    Loading,
    Ready(DiagnosticSnapshotView),
    Failed,
}

/// The Diagnostics page has only the composed booleans to work with:
/// `ready` is true for both the in-process standalone issuer and the
/// `standalone-portal` HTTP backend, and only the portal build omits the
/// in-process demo offer (see `apps/oxid/src/main.rs`). Route on that
/// distinction so a Portal-composed build is never mislabeled as the
/// generic in-process standalone issuer.
const fn credential_protocol_labels(
    ready: bool,
    standalone_demo_offer_available: bool,
) -> (&'static str, &'static str) {
    if !ready {
        return ("Not connected", "Not connected");
    }
    if standalone_demo_offer_available {
        ("Standalone Midnight DID", "OpenID4VCI 1.0 · standalone")
    } else {
        (
            "Standalone Midnight DID · Portal HTTP",
            "OpenID4VCI 1.0 · standalone-portal",
        )
    }
}

#[component]
pub(super) fn DiagnosticsPage(active_profile: WalletProfileView) -> Element {
    let services = consume_context::<WalletUiServices>();
    let credential_protocol_ready = services.credential_issuance_ready();
    let (did_adapter_state, credential_protocol_state) = credential_protocol_labels(
        credential_protocol_ready,
        services.standalone_credential_offer().is_some(),
    );
    let mut account_state = use_signal(|| AccountPageState::Loading);
    let mut diagnostic_state = use_signal(|| LocalDiagnosticsPageState::Loading);
    let profile_id = active_profile.id.clone();
    let effect_services = services.clone();
    use_effect(move || {
        let services = effect_services.clone();
        let profile_id = profile_id.clone();
        let get_diagnostics = services.get_diagnostic_snapshot();
        spawn(async move {
            account_state.set(
                run_ui_blocking(move || load_account_page(&services, &profile_id))
                    .await
                    .unwrap_or_else(|error| AccountPageState::Failed(error.to_string())),
            );
        });
        spawn(async move {
            diagnostic_state.set(
                match run_ui_blocking(move || get_diagnostics.execute()).await {
                    Ok(Ok(snapshot)) => LocalDiagnosticsPageState::Ready(snapshot),
                    Ok(Err(_)) | Err(_) => LocalDiagnosticsPageState::Failed,
                },
            );
        });
    });

    let (protection_state, protection_ready, midnight_state, midnight_ready, completion_state) =
        match account_state.read().clone() {
            AccountPageState::Loading => (
                "Loading".to_owned(),
                false,
                "Loading".to_owned(),
                false,
                "Loading".to_owned(),
            ),
            AccountPageState::Failed(_) => (
                "Status unavailable".to_owned(),
                false,
                "Status unavailable".to_owned(),
                false,
                "Status unavailable".to_owned(),
            ),
            AccountPageState::Ready {
                account, security, ..
            } => {
                let protection_ready = security.is_available();
                let midnight_ready = account.source != "unavailable";
                (
                    format!("{} · {}", security.state_name(), security.protection_name()),
                    protection_ready,
                    format!(
                        "{} · {}",
                        ui::account_source(&account.source),
                        ui::sync_state(&account.sync.state)
                    ),
                    midnight_ready,
                    if account.source == "simulated" {
                        "Deterministic simulation".to_owned()
                    } else {
                        "Not connected".to_owned()
                    },
                )
            }
        };
    let (diagnostic_summary, diagnostic_rows, diagnostics_ready) = match diagnostic_state
        .read()
        .clone()
    {
        LocalDiagnosticsPageState::Loading => ("Loading".to_owned(), Vec::new(), false),
        LocalDiagnosticsPageState::Failed => ("Status unavailable".to_owned(), Vec::new(), false),
        LocalDiagnosticsPageState::Ready(snapshot) => {
            let rows = snapshot
                .counts()
                .iter()
                .map(|count| {
                    (
                        count.code().as_str().to_owned(),
                        format!(
                            "{} · {} occurrence{}",
                            count.severity().as_str(),
                            count.occurrences(),
                            if count.occurrences() == 1 { "" } else { "s" }
                        ),
                    )
                })
                .collect();
            (
                format!(
                    "{} retained · {} total · {} evicted · capacity {}",
                    snapshot.recent().len(),
                    snapshot.total_events(),
                    snapshot.evicted_events(),
                    snapshot.capacity()
                ),
                rows,
                true,
            )
        }
    };
    let refresh_services = services.clone();
    let clear_services = services.clone();
    let mut refresh_state = diagnostic_state;
    let mut clear_state = diagnostic_state;
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Capability status" }
            h1 { "Diagnostics" }
            p { "This view reports only capabilities that are actually composed into the current application." }
        }
        div { class: "diagnostic-grid",
            CapabilityStatus { name: "Profile lifecycle", state: "Create · list · select · restore".to_owned(), ready: true }
            CapabilityStatus { name: "Profile metadata store", state: "Persistent · public metadata only".to_owned(), ready: true }
            CapabilityStatus { name: "Protected secret store", state: protection_state, ready: protection_ready }
            CapabilityStatus { name: "Midnight account", state: midnight_state, ready: midnight_ready }
            CapabilityStatus { name: "Transaction completion", state: completion_state, ready: midnight_ready }
            CapabilityStatus { name: "Local proof provider", state: "Device-gated".to_owned(), ready: false }
            CapabilityStatus { name: "DID adapter", state: did_adapter_state.to_owned(), ready: credential_protocol_ready }
            CapabilityStatus {
                name: "Credential protocols",
                state: credential_protocol_state.to_owned(),
                ready: credential_protocol_ready,
            }
        }
        section { class: "surface-card",
            p { class: "card-eyebrow", "Secret-safe runtime health" }
            h2 { "Process-local diagnostics" }
            p { "Telemetry is off. Events use fixed codes, retain no payloads, and disappear when this process exits." }
            div { class: "button-row",
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        let get = refresh_services.get_diagnostic_snapshot();
                        refresh_state.set(LocalDiagnosticsPageState::Loading);
                        spawn(async move {
                            refresh_state.set(match run_ui_blocking(move || get.execute()).await {
                                Ok(Ok(snapshot)) => LocalDiagnosticsPageState::Ready(snapshot),
                                Ok(Err(_)) | Err(_) => LocalDiagnosticsPageState::Failed,
                            });
                        });
                    },
                    "Refresh"
                }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        let clear = clear_services.clear_diagnostics();
                        let get = clear_services.get_diagnostic_snapshot();
                        clear_state.set(LocalDiagnosticsPageState::Loading);
                        spawn(async move {
                            clear_state.set(match run_ui_blocking(move || {
                                clear.execute(ClearDiagnosticsCommand {
                                    confirmed: true,
                                    intent: CLEAR_LOCAL_DIAGNOSTICS_INTENT.to_owned(),
                                })?;
                                get.execute()
                            }).await {
                                Ok(Ok(snapshot)) => LocalDiagnosticsPageState::Ready(snapshot),
                                Ok(Err(_)) | Err(_) => LocalDiagnosticsPageState::Failed,
                            });
                        });
                    },
                    "Clear local events"
                }
            }
            div { class: "diagnostic-grid",
                CapabilityStatus { name: "Bounded event ring", state: diagnostic_summary, ready: diagnostics_ready }
                CapabilityStatus { name: "Privacy boundary", state: "No persistence · no upload · no payloads".to_owned(), ready: true }
                if diagnostic_rows.is_empty() && diagnostics_ready {
                    article { class: "capability-row",
                        span { class: "capability-dot ready" }
                        div { strong { "No diagnostic events recorded" } p { "Runtime health is clean for this process." } }
                    }
                }
                for (code, detail) in diagnostic_rows {
                    article { class: "capability-row", key: "{code}",
                        span { class: "capability-dot queued" }
                        div { strong { "{code}" } p { "{detail}" } }
                    }
                }
            }
        }
    }
}

#[component]
fn CapabilityStatus(name: &'static str, state: String, ready: bool) -> Element {
    rsx! {
        article { class: "capability-row",
            span { class: if ready { "capability-dot ready" } else { "capability-dot queued" } }
            div {
                strong { "{name}" }
                p { "{state}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::credential_protocol_labels;

    #[test]
    fn protocol_labels_never_mislabel_the_portal_http_backend_as_generic_standalone() {
        assert_eq!(
            credential_protocol_labels(false, false),
            ("Not connected", "Not connected")
        );
        assert_eq!(
            credential_protocol_labels(true, true),
            ("Standalone Midnight DID", "OpenID4VCI 1.0 · standalone")
        );
        assert_eq!(
            credential_protocol_labels(true, false),
            (
                "Standalone Midnight DID · Portal HTTP",
                "OpenID4VCI 1.0 · standalone-portal",
            )
        );
    }
}
