// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use dioxus::prelude::*;
use oxid_platform_ports::ProcessResourceSample;
use oxid_wallet_application::{
    PROOF_BENCHMARK_DEFAULT_MAX_K, PROOF_BENCHMARK_HIGH_RESOURCE_K, PROOF_BENCHMARK_MAX_K,
    PROOF_BENCHMARK_MIN_K, ProofBenchmarkError, ProofBenchmarkReport, ProofBenchmarkSnapshot,
    ProofBenchmarkStage, ProofBenchmarkVerification, RunProofBenchmarkCommand,
    RunProofBenchmarkUseCase,
};

use super::WalletUiServices;

const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(400);
const RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchmarkOutcome {
    Completed(ProofBenchmarkReport),
    Failed(ProofBenchmarkError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BenchmarkRowPresentation {
    state_copy: &'static str,
    total: String,
    realized_k: Option<String>,
    detail: Option<String>,
}

fn stage_total(report: ProofBenchmarkReport) -> Duration {
    report.key_generation + report.proving + report.verification.unwrap_or_default()
}

fn next_expanded_row(current: Option<u8>, selected: u8) -> Option<u8> {
    (current != Some(selected)).then_some(selected)
}

fn row_presentation(
    requested_k: u8,
    outcome: Option<BenchmarkOutcome>,
    snapshot: ProofBenchmarkSnapshot,
    queued_through: Option<u8>,
) -> BenchmarkRowPresentation {
    if benchmark_is_running(snapshot) && snapshot.active_k == Some(requested_k) {
        return BenchmarkRowPresentation {
            state_copy: "Running",
            total: format!("{}…", snapshot.stage.as_str()),
            realized_k: None,
            detail: None,
        };
    }
    if benchmark_is_running(snapshot)
        && snapshot.active_k.is_some_and(|active_k| {
            queued_through.is_some_and(|max_k| requested_k > active_k && requested_k <= max_k)
        })
    {
        return BenchmarkRowPresentation {
            state_copy: "Queued",
            total: "Waiting for the active worker".to_owned(),
            realized_k: None,
            detail: None,
        };
    }
    match outcome {
        Some(BenchmarkOutcome::Completed(report)) => {
            let state_copy = match report.verification_result {
                ProofBenchmarkVerification::Failed => "Verification failed",
                ProofBenchmarkVerification::Verified | ProofBenchmarkVerification::Skipped => {
                    "Completed"
                }
            };
            BenchmarkRowPresentation {
                state_copy,
                total: format!("Stage total {}", duration_text(stage_total(report))),
                realized_k: (report.realized_k != requested_k)
                    .then(|| format!("realized k={}", report.realized_k)),
                detail: Some(verification_text(report)),
            }
        }
        Some(BenchmarkOutcome::Failed(ProofBenchmarkError::Busy)) => BenchmarkRowPresentation {
            state_copy: "Admission refused",
            total: "No result".to_owned(),
            realized_k: None,
            detail: Some("Another proof benchmark is still running".to_owned()),
        },
        Some(BenchmarkOutcome::Failed(
            ProofBenchmarkError::Unavailable | ProofBenchmarkError::ResourceUnavailable,
        )) => BenchmarkRowPresentation {
            state_copy: "Unavailable",
            total: "No result".to_owned(),
            realized_k: None,
            detail: Some("Required benchmark resources are unavailable".to_owned()),
        },
        Some(BenchmarkOutcome::Failed(error)) => BenchmarkRowPresentation {
            state_copy: "Failed",
            total: "No result".to_owned(),
            realized_k: None,
            detail: Some(error.to_string()),
        },
        None => BenchmarkRowPresentation {
            state_copy: "Not run",
            total: "No result".to_owned(),
            realized_k: None,
            detail: None,
        },
    }
}

fn displayed_report(
    presentation: &BenchmarkRowPresentation,
    outcome: Option<BenchmarkOutcome>,
) -> Option<ProofBenchmarkReport> {
    matches!(presentation.state_copy, "Completed" | "Verification failed")
        .then(|| match outcome {
            Some(BenchmarkOutcome::Completed(report)) => Some(report),
            _ => None,
        })
        .flatten()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct HelpDisclosure {
    expanded: bool,
}

impl HelpDisclosure {
    fn is_expanded(self) -> bool {
        self.expanded
    }

    fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }
}

fn benchmark_is_running(snapshot: ProofBenchmarkSnapshot) -> bool {
    matches!(
        snapshot.stage,
        ProofBenchmarkStage::Preparing
            | ProofBenchmarkStage::KeyGeneration
            | ProofBenchmarkStage::Proving
            | ProofBenchmarkStage::Verifying
    )
}

fn sweep_targets(max_k: u8, high_resource_acknowledged: bool) -> Result<Vec<u8>, &'static str> {
    let max_k = max_k.clamp(PROOF_BENCHMARK_MIN_K, PROOF_BENCHMARK_MAX_K);
    if max_k >= PROOF_BENCHMARK_HIGH_RESOURCE_K && !high_resource_acknowledged {
        return Err("Acknowledge the high-resource warning before running k=18 or above.");
    }
    Ok((PROOF_BENCHMARK_MIN_K..=max_k).collect())
}

fn duration_text(duration: Duration) -> String {
    if duration.as_secs() >= 1 {
        format!("{:.2}s", duration.as_secs_f64())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn verification_text(report: ProofBenchmarkReport) -> String {
    match report.verification_result {
        ProofBenchmarkVerification::Verified => report.verification.map_or_else(
            || "verified".to_owned(),
            |duration| format!("verified in {}", duration_text(duration)),
        ),
        ProofBenchmarkVerification::Failed => "verification failed".to_owned(),
        ProofBenchmarkVerification::Skipped => "verification unavailable above k=14".to_owned(),
    }
}

fn verification_duration_text(report: ProofBenchmarkReport) -> String {
    report
        .verification
        .map_or_else(|| "Not measured".to_owned(), duration_text)
}

fn memory_text(bytes: u64) -> String {
    const MEBIBYTE: f64 = 1_048_576.0;
    format!("{:.1} MiB", bytes as f64 / MEBIBYTE)
}

fn cpu_text(basis_points: u32) -> String {
    format!("{:.1}%", f64::from(basis_points) / 100.0)
}

fn proof_size_text(bytes: usize) -> String {
    if bytes >= 1_024 {
        format!("{:.1} KiB", bytes as f64 / 1_024.0)
    } else {
        format!("{bytes} B")
    }
}

fn high_resource_selected(max_k: u8) -> bool {
    max_k.clamp(PROOF_BENCHMARK_MIN_K, PROOF_BENCHMARK_MAX_K) >= PROOF_BENCHMARK_HIGH_RESOURCE_K
}

fn individual_high_resource_guidance_visible(selected_k: Option<u8>, row_k: u8) -> bool {
    selected_k == Some(row_k) && row_k >= PROOF_BENCHMARK_HIGH_RESOURCE_K
}

fn resource_monitor_status(unavailable: bool) -> &'static str {
    if unavailable {
        "Process resource monitor unavailable on this target"
    } else {
        "Warming up process resource monitor…"
    }
}

async fn run_one(benchmark: Arc<dyn RunProofBenchmarkUseCase>, k: u8) -> BenchmarkOutcome {
    match benchmark.execute(RunProofBenchmarkCommand { k }).await {
        Ok(report) => BenchmarkOutcome::Completed(report),
        Err(error) => BenchmarkOutcome::Failed(error),
    }
}

#[component]
pub(super) fn ProofBenchmarkPanel() -> Element {
    let services = consume_context::<WalletUiServices>();
    let Some(benchmark) = services.proof_benchmark() else {
        return rsx! {};
    };
    let mut results = use_signal(BTreeMap::<u8, BenchmarkOutcome>::new);
    let mut snapshot = use_signal(|| benchmark.snapshot());
    let mut sweep_max_k = use_signal(|| PROOF_BENCHMARK_DEFAULT_MAX_K);
    let mut high_resource_acknowledged = use_signal(|| false);
    let mut help_disclosure = use_signal(HelpDisclosure::default);
    let mut expanded_row = use_signal(|| None::<u8>);
    let mut individual_high_k_selected = use_signal(|| None::<u8>);
    let mut sweeping = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);
    let mut resource_sample = use_signal(|| None::<ProcessResourceSample>);
    let mut peak_resident_bytes = use_signal(|| 0_u64);
    let mut resource_sampler_unavailable = use_signal(|| false);

    let polling_benchmark = Arc::clone(&benchmark);
    use_future(move || {
        let polling_benchmark = Arc::clone(&polling_benchmark);
        async move {
            loop {
                snapshot.set(polling_benchmark.snapshot());
                tokio::time::sleep(SNAPSHOT_INTERVAL).await;
            }
        }
    });

    let resource_sampler = services.process_resource_sampler();
    use_future(move || {
        let resource_sampler = Arc::clone(&resource_sampler);
        async move {
            loop {
                match resource_sampler.sample() {
                    Ok(sample) => {
                        peak_resident_bytes
                            .with_mut(|peak| *peak = (*peak).max(sample.resident_bytes()));
                        resource_sample.set(Some(sample));
                        resource_sampler_unavailable.set(false);
                    }
                    Err(_) => {
                        resource_sample.set(None);
                        resource_sampler_unavailable.set(true);
                    }
                }
                tokio::time::sleep(RESOURCE_SAMPLE_INTERVAL).await;
            }
        }
    });

    let current = snapshot();
    let worker_busy = benchmark_is_running(current);
    let stage = current.active_k.map_or_else(
        || current.stage.as_str().to_owned(),
        |k| format!("k={k} · {}", current.stage.as_str()),
    );
    let result_snapshot = results.read().clone();
    let high_resource_selected = high_resource_selected(sweep_max_k());
    let help_expanded = help_disclosure().is_expanded();
    let benchmark_for_sweep = Arc::clone(&benchmark);

    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Development tool" }
            h1 { "Proof benchmark" }
            p { "Run synthetic Midnight proving measurements in this development process." }
        }
        section { class: "surface-card", aria_label: "Development proof benchmark",
            dl { class: "proof-resource-monitor", aria_label: "Current process resource monitor",
                if let Some(sample) = resource_sample() {
                    div {
                        dt { "Memory now" }
                        dd { "{memory_text(sample.resident_bytes())}" }
                    }
                    div {
                        dt { "Page-session peak" }
                        dd { "{memory_text(peak_resident_bytes())}" }
                    }
                    div {
                        dt { "Process CPU" }
                        dd { "{cpu_text(sample.cpu_usage_basis_points())}" }
                    }
                } else {
                    div { class: "proof-resource-monitor__unavailable",
                        dt { "Resource monitor" }
                        dd { "{resource_monitor_status(resource_sampler_unavailable())}" }
                    }
                }
            }
            div { class: "button-row proof-benchmark-controls",
                label { class: "network-field",
                    span { "Run-all maximum k" }
                    input {
                        r#type: "number",
                        min: "1",
                        max: "21",
                        value: "{sweep_max_k}",
                        disabled: worker_busy || sweeping(),
                        oninput: move |event| {
                            if let Ok(value) = event.value().parse::<u8>() {
                                sweep_max_k.set(value.clamp(
                                    PROOF_BENCHMARK_MIN_K,
                                    PROOF_BENCHMARK_MAX_K,
                                ));
                            }
                        },
                    }
                }
                button {
                    class: "proof-benchmark-run-button proof-benchmark-sweep-button",
                    r#type: "button",
                    disabled: worker_busy || sweeping(),
                    onclick: move |_| {
                        let targets = match sweep_targets(
                            sweep_max_k(),
                            high_resource_acknowledged(),
                        ) {
                            Ok(targets) => targets,
                            Err(message) => {
                                notice.set(Some(message.to_owned()));
                                return;
                            }
                        };
                        let benchmark = Arc::clone(&benchmark_for_sweep);
                        notice.set(None);
                        sweeping.set(true);
                        spawn(async move {
                            for k in targets {
                                let outcome = run_one(Arc::clone(&benchmark), k).await;
                                let failed = matches!(outcome, BenchmarkOutcome::Failed(_));
                                results.write().insert(k, outcome);
                                if failed {
                                    notice.set(Some(
                                        "Sweep stopped after the first failed row to bound resource use."
                                            .to_owned(),
                                    ));
                                    break;
                                }
                                tokio::task::yield_now().await;
                            }
                            sweeping.set(false);
                        });
                    },
                    if sweeping() { "Running sequential sweep…" } else { "Run sequential sweep" }
                }
            }
            if high_resource_selected {
                p { class: "field-hint proof-benchmark-high-resource-warning",
                    "k=18–21 can consume substantial memory, time, network, and disk."
                }
                label { class: "confirmation-check",
                    input {
                        r#type: "checkbox",
                        checked: high_resource_acknowledged(),
                        disabled: worker_busy || sweeping(),
                        onchange: move |event| high_resource_acknowledged.set(event.checked()),
                    }
                    span { "I understand that k=18–21 may exhaust this device's resources." }
                }
            }
            p { class: "status-pill", role: "status", "{stage}" }
            button {
                class: "proof-benchmark-help-button",
                r#type: "button",
                aria_expanded: if help_expanded { "true" } else { "false" },
                aria_controls: "proof-benchmark-help",
                onclick: move |_| help_disclosure.with_mut(HelpDisclosure::toggle),
                if help_expanded { "Hide benchmark help" } else { "About this benchmark" }
            }
            if help_expanded {
                div { id: "proof-benchmark-help", class: "proof-benchmark-help", role: "note",
                    p { "Runs one synthetic proof at a time through k=21; results live only in this process." }
                    p { "Stage total is the sum of measured key generation, proving, and verification stages; it is not end-to-end elapsed time." }
                    p { "First runs may download public proving parameters into the app-private cache." }
                    p { "Leaving this page does not cancel an admitted worker. This build does not run high-k proofs in CI." }
                }
            }
            if let Some(message) = notice() {
                p { class: "field-error", role: "alert", "{message}" }
            }
            div { class: "proof-benchmark-list", aria_label: "Circuit benchmark results",
                div { class: "proof-benchmark-list__header", aria_hidden: "true",
                    div { class: "proof-benchmark-list__header-summary",
                        span { "Circuit" }
                        span { "Status" }
                        span { "Stage total" }
                    }
                    span { "Details" }
                    span { "Run" }
                }
                for k in PROOF_BENCHMARK_MIN_K..=PROOF_BENCHMARK_MAX_K {
                    {
                        let outcome = result_snapshot.get(&k).copied();
                        let active = worker_busy && current.active_k == Some(k);
                        let presentation = row_presentation(
                            k,
                            outcome,
                            current,
                            sweeping().then_some(sweep_max_k()),
                        );
                        let displayed_report = displayed_report(&presentation, outcome);
                        let expanded = expanded_row() == Some(k);
                        let benchmark = Arc::clone(&benchmark);
                        let show_individual_high_resource_guidance =
                            individual_high_resource_guidance_visible(individual_high_k_selected(), k);
                        rsx! {
                            article { class: "proof-benchmark-row capability-row", key: "proof-k-{k}",
                                div { class: "proof-benchmark-row__summary",
                                    strong { "Circuit k={k}" }
                                    span { class: "proof-benchmark-row__state", "{presentation.state_copy}" }
                                    span { class: "proof-benchmark-row__total", "{presentation.total}" }
                                    if let Some(realized_k) = &presentation.realized_k {
                                        span { class: "proof-benchmark-row__realized", "{realized_k}" }
                                    }
                                }
                                button {
                                    class: "proof-benchmark-disclosure",
                                    r#type: "button",
                                    aria_expanded: if expanded { "true" } else { "false" },
                                    aria_controls: "proof-benchmark-details-{k}",
                                    aria_label: if expanded { format!("Collapse details for circuit k={k}") } else { format!("Show details for circuit k={k}") },
                                    onclick: move |_| expanded_row.set(next_expanded_row(expanded_row(), k)),
                                    if expanded { "⌃" } else { "⌄" }
                                }
                                button {
                                    class: "proof-benchmark-run-button",
                                    r#type: "button",
                                    disabled: worker_busy || sweeping(),
                                    onclick: move |_| {
                                        notice.set(None);
                                        if k >= PROOF_BENCHMARK_HIGH_RESOURCE_K
                                            && !high_resource_acknowledged()
                                        {
                                            individual_high_k_selected.set(Some(k));
                                            return;
                                        }
                                        let benchmark = Arc::clone(&benchmark);
                                        spawn(async move {
                                            let outcome = run_one(benchmark, k).await;
                                            results.write().insert(k, outcome);
                                        });
                                    },
                                    if active {
                                        "Running…"
                                    } else if outcome.is_some() {
                                        "Retry"
                                    } else {
                                        "Run"
                                    }
                                }
                                if expanded {
                                    div { id: "proof-benchmark-details-{k}", class: "proof-benchmark-row__details",
                                        if let Some(detail) = presentation.detail {
                                            p { class: "proof-benchmark-outcome", "{detail}" }
                                        }
                                        if let Some(report) = displayed_report {
                                            dl { class: "proof-benchmark-timings", aria_label: "Circuit k={k} timing metrics",
                                                div { dt { "Stage total" } dd { "{duration_text(stage_total(report))}" } }
                                                div { dt { "Key generation" } dd { "{duration_text(report.key_generation)}" } }
                                                div { dt { "Proving" } dd { "{duration_text(report.proving)}" } }
                                                div { dt { "Verification" } dd { "{verification_duration_text(report)}" } }
                                            }
                                            dl { class: "proof-benchmark-facts", aria_label: "Circuit k={k} result facts",
                                                div { dt { "Requested circuit" } dd { "k={report.requested_k}" } }
                                                div { dt { "Realized circuit" } dd { "k={report.realized_k}" } }
                                                div { dt { if report.row_count.is_estimated() { "Estimated rows" } else { "Measured rows" } } dd { "{report.row_count.value()}" } }
                                                div { dt { "Hash chain" } dd { "{report.hash_chain_length}" } }
                                                div { dt { "Proof size" } dd { "{proof_size_text(report.proof_bytes)}" } }
                                            }
                                        }
                                    }
                                }
                                if show_individual_high_resource_guidance {
                                    div { class: "proof-benchmark-inline-warning", role: "alert",
                                        p { "k={k} may consume substantial memory, time, network, and disk." }
                                        label { class: "confirmation-check",
                                            input {
                                                r#type: "checkbox",
                                                checked: high_resource_acknowledged(),
                                                disabled: worker_busy || sweeping(),
                                                onchange: move |event| high_resource_acknowledged.set(event.checked()),
                                            }
                                            span { "I understand and want to enable high-resource proof runs." }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_wallet_application::ProofBenchmarkRowCount;

    #[test]
    fn safe_default_sweep_is_bounded_at_k17() {
        assert_eq!(PROOF_BENCHMARK_DEFAULT_MAX_K, 17);
        let targets = sweep_targets(PROOF_BENCHMARK_DEFAULT_MAX_K, false).expect("safe sweep");
        assert_eq!(targets.first(), Some(&1));
        assert_eq!(targets.last(), Some(&17));
    }

    #[test]
    fn high_k_requires_explicit_resource_acknowledgement() {
        assert!(sweep_targets(18, false).is_err());
        assert_eq!(sweep_targets(21, true).expect("acknowledged").len(), 21);
    }

    #[test]
    fn help_is_collapsed_by_default_and_expands_only_on_action() {
        let mut disclosure = HelpDisclosure::default();
        assert!(!disclosure.is_expanded());
        disclosure.toggle();
        assert!(disclosure.is_expanded());
        disclosure.toggle();
        assert!(!disclosure.is_expanded());
    }

    #[test]
    fn high_resource_warning_is_shown_only_for_a_selected_high_k_range() {
        assert!(!high_resource_selected(17));
        assert!(high_resource_selected(18));
        assert!(high_resource_selected(21));
        assert!(!individual_high_resource_guidance_visible(None, 18));
        assert!(!individual_high_resource_guidance_visible(Some(18), 17));
        assert!(individual_high_resource_guidance_visible(Some(18), 18));
    }

    #[test]
    fn unavailable_resource_sampler_has_a_truthful_compact_status() {
        assert_eq!(
            resource_monitor_status(true),
            "Process resource monitor unavailable on this target"
        );
        assert_eq!(
            resource_monitor_status(false),
            "Warming up process resource monitor…"
        );
    }

    #[test]
    fn public_resource_and_result_values_are_human_readable() {
        assert_eq!(memory_text(1_572_864), "1.5 MiB");
        assert_eq!(cpu_text(12_345), "123.5%");
        assert_eq!(proof_size_text(900), "900 B");
        assert_eq!(proof_size_text(2_560), "2.5 KiB");
    }

    fn completed_report(realized_k: u8) -> ProofBenchmarkReport {
        ProofBenchmarkReport {
            requested_k: 7,
            realized_k,
            row_count: ProofBenchmarkRowCount::Measured(42),
            hash_chain_length: 4,
            key_generation: Duration::from_millis(100),
            proving: Duration::from_millis(200),
            verification: Some(Duration::from_millis(50)),
            verification_result: ProofBenchmarkVerification::Verified,
            proof_bytes: 2_560,
        }
    }

    #[test]
    fn compact_presenter_labels_stage_total_and_only_shows_differing_realized_k() {
        let idle = ProofBenchmarkSnapshot {
            stage: ProofBenchmarkStage::Idle,
            active_k: None,
        };
        let matching = row_presentation(
            7,
            Some(BenchmarkOutcome::Completed(completed_report(7))),
            idle,
            None,
        );
        assert_eq!(matching.total, "Stage total 350ms");
        assert_eq!(matching.realized_k, None);

        let differing = row_presentation(
            7,
            Some(BenchmarkOutcome::Completed(completed_report(8))),
            idle,
            None,
        );
        assert_eq!(differing.realized_k.as_deref(), Some("realized k=8"));

        let mut failed_verification = completed_report(7);
        failed_verification.verification_result = ProofBenchmarkVerification::Failed;
        let failed = row_presentation(
            7,
            Some(BenchmarkOutcome::Completed(failed_verification)),
            idle,
            None,
        );
        assert_eq!(failed.state_copy, "Verification failed");
        assert!(
            displayed_report(
                &failed,
                Some(BenchmarkOutcome::Completed(failed_verification))
            )
            .is_some()
        );
    }

    #[test]
    fn row_disclosure_keeps_at_most_one_row_expanded_without_running_a_proof() {
        assert_eq!(next_expanded_row(None, 7), Some(7));
        assert_eq!(next_expanded_row(Some(7), 7), None);
        assert_eq!(next_expanded_row(Some(7), 8), Some(8));
    }

    #[test]
    fn compact_presenter_makes_each_row_state_textual() {
        let idle = ProofBenchmarkSnapshot {
            stage: ProofBenchmarkStage::Idle,
            active_k: None,
        };
        let running = ProofBenchmarkSnapshot {
            stage: ProofBenchmarkStage::Proving,
            active_k: Some(7),
        };
        let queued = ProofBenchmarkSnapshot {
            stage: ProofBenchmarkStage::Proving,
            active_k: Some(8),
        };
        assert_eq!(
            row_presentation(
                7,
                Some(BenchmarkOutcome::Completed(completed_report(7))),
                idle,
                None,
            )
            .state_copy,
            "Completed"
        );
        assert_eq!(
            row_presentation(
                7,
                Some(BenchmarkOutcome::Completed(completed_report(7))),
                running,
                None,
            )
            .state_copy,
            "Running"
        );
        assert_eq!(
            row_presentation(9, None, queued, Some(17)).state_copy,
            "Queued"
        );
        let queued_with_prior_result = row_presentation(
            9,
            Some(BenchmarkOutcome::Completed(completed_report(9))),
            queued,
            Some(17),
        );
        assert_eq!(queued_with_prior_result.state_copy, "Queued");
        assert!(
            displayed_report(
                &queued_with_prior_result,
                Some(BenchmarkOutcome::Completed(completed_report(9))),
            )
            .is_none()
        );
        assert_eq!(
            row_presentation(7, None, queued, Some(17)).state_copy,
            "Not run"
        );
        assert_eq!(
            row_presentation(18, None, queued, Some(17)).state_copy,
            "Not run"
        );
        assert_eq!(
            row_presentation(
                7,
                Some(BenchmarkOutcome::Failed(ProofBenchmarkError::Busy)),
                idle,
                None,
            )
            .state_copy,
            "Admission refused"
        );
        assert_eq!(
            row_presentation(
                7,
                Some(BenchmarkOutcome::Failed(ProofBenchmarkError::ProvingFailed)),
                idle,
                None,
            )
            .state_copy,
            "Failed"
        );
        assert_eq!(
            row_presentation(
                7,
                Some(BenchmarkOutcome::Failed(ProofBenchmarkError::Unavailable)),
                idle,
                None,
            )
            .state_copy,
            "Unavailable"
        );
        assert_eq!(row_presentation(7, None, idle, None).state_copy, "Not run");
    }

    #[test]
    fn only_active_worker_stages_disable_new_runs() {
        for stage in [
            ProofBenchmarkStage::Preparing,
            ProofBenchmarkStage::KeyGeneration,
            ProofBenchmarkStage::Proving,
            ProofBenchmarkStage::Verifying,
        ] {
            assert!(benchmark_is_running(ProofBenchmarkSnapshot {
                stage,
                active_k: Some(7),
            }));
        }
        assert!(!benchmark_is_running(ProofBenchmarkSnapshot {
            stage: ProofBenchmarkStage::Completed,
            active_k: Some(7),
        }));
    }
}
