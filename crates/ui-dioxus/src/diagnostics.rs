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

#[derive(Debug, PartialEq, Eq)]
struct DiagnosticsProjection {
    summary: String,
    rows: Vec<(String, String)>,
    ready: bool,
    empty: bool,
}

fn project_diagnostics(state: &LocalDiagnosticsPageState) -> DiagnosticsProjection {
    match state {
        LocalDiagnosticsPageState::Loading => DiagnosticsProjection {
            summary: "Loading".to_owned(),
            rows: Vec::new(),
            ready: false,
            empty: false,
        },
        LocalDiagnosticsPageState::Failed => DiagnosticsProjection {
            summary: "Status unavailable".to_owned(),
            rows: Vec::new(),
            ready: false,
            empty: false,
        },
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
                .collect::<Vec<_>>();
            DiagnosticsProjection {
                summary: format!(
                    "{} retained · {} total · {} evicted · capacity {}",
                    snapshot.recent().len(),
                    snapshot.total_events(),
                    snapshot.evicted_events(),
                    snapshot.capacity()
                ),
                empty: rows.is_empty(),
                rows,
                ready: true,
            }
        }
    }
}

fn map_diagnostic_snapshot(
    result: Result<
        Result<DiagnosticSnapshotView, oxid_diagnostics_application::DiagnosticsError>,
        super::UiBlockingTaskError,
    >,
) -> LocalDiagnosticsPageState {
    match result {
        Ok(Ok(snapshot)) => LocalDiagnosticsPageState::Ready(snapshot),
        Ok(Err(_)) | Err(_) => LocalDiagnosticsPageState::Failed,
    }
}

async fn load_diagnostic_snapshot(
    get: Arc<dyn GetDiagnosticSnapshotUseCase>,
) -> LocalDiagnosticsPageState {
    map_diagnostic_snapshot(run_ui_blocking(move || get.execute()).await)
}

async fn clear_diagnostics_and_reload(
    clear: Arc<dyn ClearDiagnosticsUseCase>,
    get: Arc<dyn GetDiagnosticSnapshotUseCase>,
) -> LocalDiagnosticsPageState {
    map_diagnostic_snapshot(
        run_ui_blocking(move || {
            clear.execute(ClearDiagnosticsCommand {
                confirmed: true,
                intent: CLEAR_LOCAL_DIAGNOSTICS_INTENT.to_owned(),
            })?;
            get.execute()
        })
        .await,
    )
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
            diagnostic_state.set(load_diagnostic_snapshot(get_diagnostics).await);
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
    let diagnostic_projection = project_diagnostics(&diagnostic_state.read());
    let diagnostic_summary = diagnostic_projection.summary;
    let diagnostic_rows = diagnostic_projection.rows;
    let diagnostics_ready = diagnostic_projection.ready;
    let diagnostics_empty = diagnostic_projection.empty;
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
                            refresh_state.set(load_diagnostic_snapshot(get).await);
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
                            clear_state.set(clear_diagnostics_and_reload(clear, get).await);
                        });
                    },
                    "Clear local events"
                }
            }
            div { class: "diagnostic-grid",
                CapabilityStatus { name: "Bounded event ring", state: diagnostic_summary, ready: diagnostics_ready }
                CapabilityStatus { name: "Privacy boundary", state: "No persistence · no upload · no payloads".to_owned(), ready: true }
                if diagnostics_empty && diagnostics_ready {
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
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use oxid_diagnostics_application::{
        ClearedDiagnosticsView, DiagnosticCode, DiagnosticCountView, DiagnosticEventView,
        DiagnosticSeverity, DiagnosticsError,
    };

    use super::*;

    const SECRET_SENTINEL: &str = "secret://credential-bearing-fake-error";

    struct FakeGetDiagnostics {
        outcomes: Mutex<VecDeque<Result<DiagnosticSnapshotView, DiagnosticsError>>>,
        calls: Arc<Mutex<Vec<&'static str>>>,
        private_error_detail: &'static str,
    }

    impl FakeGetDiagnostics {
        fn new(
            outcomes: Vec<Result<DiagnosticSnapshotView, DiagnosticsError>>,
            calls: Arc<Mutex<Vec<&'static str>>>,
        ) -> Self {
            Self {
                outcomes: Mutex::new(outcomes.into()),
                calls,
                private_error_detail: SECRET_SENTINEL,
            }
        }
    }

    impl GetDiagnosticSnapshotUseCase for FakeGetDiagnostics {
        fn execute(&self) -> Result<DiagnosticSnapshotView, DiagnosticsError> {
            self.calls.lock().expect("call log").push("get");
            let _private_error_detail = self.private_error_detail;
            self.outcomes
                .lock()
                .expect("snapshot outcomes")
                .pop_front()
                .expect("configured snapshot outcome")
        }
    }

    struct FakeClearDiagnostics {
        outcome: Result<ClearedDiagnosticsView, DiagnosticsError>,
        calls: Arc<Mutex<Vec<&'static str>>>,
        commands: Arc<Mutex<Vec<ClearDiagnosticsCommand>>>,
        private_error_detail: &'static str,
    }

    impl FakeClearDiagnostics {
        fn new(
            outcome: Result<ClearedDiagnosticsView, DiagnosticsError>,
            calls: Arc<Mutex<Vec<&'static str>>>,
            commands: Arc<Mutex<Vec<ClearDiagnosticsCommand>>>,
        ) -> Self {
            Self {
                outcome,
                calls,
                commands,
                private_error_detail: SECRET_SENTINEL,
            }
        }
    }

    impl ClearDiagnosticsUseCase for FakeClearDiagnostics {
        fn execute(
            &self,
            command: ClearDiagnosticsCommand,
        ) -> Result<ClearedDiagnosticsView, DiagnosticsError> {
            self.calls.lock().expect("call log").push("clear");
            self.commands.lock().expect("commands").push(command);
            let _private_error_detail = self.private_error_detail;
            self.outcome
        }
    }

    fn populated_snapshot() -> DiagnosticSnapshotView {
        DiagnosticSnapshotView::new(
            8,
            4,
            1,
            vec![
                DiagnosticCountView::new(
                    DiagnosticCode::HeadlessRequestRejected,
                    DiagnosticSeverity::Warning,
                    1,
                ),
                DiagnosticCountView::new(
                    DiagnosticCode::MidnightDustSyncFailed,
                    DiagnosticSeverity::Error,
                    3,
                ),
            ],
            vec![
                DiagnosticEventView::new(
                    2,
                    DiagnosticCode::HeadlessRequestRejected,
                    DiagnosticSeverity::Warning,
                ),
                DiagnosticEventView::new(
                    4,
                    DiagnosticCode::MidnightDustSyncFailed,
                    DiagnosticSeverity::Error,
                ),
            ],
        )
    }

    fn projection_text(projection: &DiagnosticsProjection) -> String {
        let rows = projection
            .rows
            .iter()
            .map(|(code, detail)| format!("{code} {detail}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!("{} {rows}", projection.summary)
    }

    #[test]
    fn projects_loading_failed_and_ready_diagnostics() {
        assert_eq!(
            project_diagnostics(&LocalDiagnosticsPageState::Loading),
            DiagnosticsProjection {
                summary: "Loading".to_owned(),
                rows: Vec::new(),
                ready: false,
                empty: false,
            }
        );
        assert_eq!(
            project_diagnostics(&LocalDiagnosticsPageState::Failed),
            DiagnosticsProjection {
                summary: "Status unavailable".to_owned(),
                rows: Vec::new(),
                ready: false,
                empty: false,
            }
        );
        assert_eq!(
            project_diagnostics(&LocalDiagnosticsPageState::Ready(populated_snapshot())),
            DiagnosticsProjection {
                summary: "2 retained · 4 total · 1 evicted · capacity 8".to_owned(),
                rows: vec![
                    (
                        "headless.request.rejected".to_owned(),
                        "warning · 1 occurrence".to_owned(),
                    ),
                    (
                        "midnight.dust.sync.failed".to_owned(),
                        "error · 3 occurrences".to_owned(),
                    ),
                ],
                ready: true,
                empty: false,
            }
        );
    }

    #[test]
    fn projects_ready_empty_diagnostics() {
        let projection = project_diagnostics(&LocalDiagnosticsPageState::Ready(
            DiagnosticSnapshotView::new(1_024, 0, 0, Vec::new(), Vec::new()),
        ));

        assert_eq!(
            projection.summary,
            "0 retained · 0 total · 0 evicted · capacity 1024"
        );
        assert!(projection.rows.is_empty());
        assert!(projection.ready);
        assert!(projection.empty);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn initial_and_refresh_snapshot_outcomes_use_payload_free_states() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let get = Arc::new(FakeGetDiagnostics::new(
            vec![Ok(populated_snapshot()), Err(DiagnosticsError::Unavailable)],
            calls.clone(),
        ));

        let initial = futures::executor::block_on(load_diagnostic_snapshot(get.clone()));
        let refresh = futures::executor::block_on(load_diagnostic_snapshot(get));

        assert_eq!(
            project_diagnostics(&initial).summary,
            "2 retained · 4 total · 1 evicted · capacity 8"
        );
        let refresh_projection = project_diagnostics(&refresh);
        assert_eq!(refresh_projection.summary, "Status unavailable");
        assert!(!projection_text(&refresh_projection).contains(SECRET_SENTINEL));
        assert_eq!(*calls.lock().expect("call log"), ["get", "get"]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn clear_uses_exact_command_before_reloading() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let clear = Arc::new(FakeClearDiagnostics::new(
            Ok(ClearedDiagnosticsView { cleared_events: 4 }),
            calls.clone(),
            commands.clone(),
        ));
        let get = Arc::new(FakeGetDiagnostics::new(
            vec![Ok(DiagnosticSnapshotView::new(
                8,
                0,
                0,
                Vec::new(),
                Vec::new(),
            ))],
            calls.clone(),
        ));

        let state = futures::executor::block_on(clear_diagnostics_and_reload(clear, get));

        assert_eq!(*calls.lock().expect("call log"), ["clear", "get"]);
        assert_eq!(
            *commands.lock().expect("commands"),
            [ClearDiagnosticsCommand {
                confirmed: true,
                intent: CLEAR_LOCAL_DIAGNOSTICS_INTENT.to_owned(),
            }]
        );
        let projection = project_diagnostics(&state);
        assert!(projection.ready);
        assert!(projection.empty);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn clear_and_reload_failures_are_redacted_and_stop_in_order() {
        let clear_failure_calls = Arc::new(Mutex::new(Vec::new()));
        let clear_failure = Arc::new(FakeClearDiagnostics::new(
            Err(DiagnosticsError::Unavailable),
            clear_failure_calls.clone(),
            Arc::new(Mutex::new(Vec::new())),
        ));
        let skipped_get = Arc::new(FakeGetDiagnostics::new(
            Vec::new(),
            clear_failure_calls.clone(),
        ));

        let clear_failure =
            futures::executor::block_on(clear_diagnostics_and_reload(clear_failure, skipped_get));
        let clear_failure_projection = project_diagnostics(&clear_failure);
        assert_eq!(*clear_failure_calls.lock().expect("call log"), ["clear"]);
        assert_eq!(clear_failure_projection.summary, "Status unavailable");
        assert!(!projection_text(&clear_failure_projection).contains(SECRET_SENTINEL));

        let reload_failure_calls = Arc::new(Mutex::new(Vec::new()));
        let reload_failure = futures::executor::block_on(clear_diagnostics_and_reload(
            Arc::new(FakeClearDiagnostics::new(
                Ok(ClearedDiagnosticsView { cleared_events: 4 }),
                reload_failure_calls.clone(),
                Arc::new(Mutex::new(Vec::new())),
            )),
            Arc::new(FakeGetDiagnostics::new(
                vec![Err(DiagnosticsError::Unavailable)],
                reload_failure_calls.clone(),
            )),
        ));
        let reload_failure_projection = project_diagnostics(&reload_failure);
        assert_eq!(
            *reload_failure_calls.lock().expect("call log"),
            ["clear", "get"]
        );
        assert_eq!(reload_failure_projection.summary, "Status unavailable");
        assert!(!projection_text(&reload_failure_projection).contains(SECRET_SENTINEL));
    }
}
