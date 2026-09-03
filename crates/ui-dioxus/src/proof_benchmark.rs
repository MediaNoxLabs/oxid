// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use dioxus::prelude::*;
use oxid_wallet_application::{
    PROOF_BENCHMARK_DEFAULT_MAX_K, PROOF_BENCHMARK_HIGH_RESOURCE_K, PROOF_BENCHMARK_MAX_K,
    PROOF_BENCHMARK_MIN_K, ProofBenchmarkError, ProofBenchmarkReport, ProofBenchmarkSnapshot,
    ProofBenchmarkStage, ProofBenchmarkVerification, RunProofBenchmarkCommand,
    RunProofBenchmarkUseCase,
};

use super::WalletUiServices;

const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(400);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchmarkOutcome {
    Completed(ProofBenchmarkReport),
    Failed(ProofBenchmarkError),
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

fn report_text(report: ProofBenchmarkReport) -> String {
    let row_qualifier = if report.row_count.is_estimated() {
        "estimated rows"
    } else {
        "measured rows"
    };
    let verification = match report.verification_result {
        ProofBenchmarkVerification::Verified => report.verification.map_or_else(
            || "verified".to_owned(),
            |duration| format!("verified in {}", duration_text(duration)),
        ),
        ProofBenchmarkVerification::Failed => "verification failed".to_owned(),
        ProofBenchmarkVerification::Skipped => "verification unavailable above k=14".to_owned(),
    };
    format!(
        "realized k={} · {} {} · {} hashes · keygen {} · prove {} · {} · {} bytes",
        report.realized_k,
        report.row_count.value(),
        row_qualifier,
        report.hash_chain_length,
        duration_text(report.key_generation),
        duration_text(report.proving),
        verification,
        report.proof_bytes,
    )
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
    let mut sweeping = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);

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

    let current = snapshot();
    let worker_busy = benchmark_is_running(current);
    let stage = current.active_k.map_or_else(
        || current.stage.as_str().to_owned(),
        |k| format!("k={k} · {}", current.stage.as_str()),
    );
    let result_snapshot = results.read().clone();
    let benchmark_for_sweep = Arc::clone(&benchmark);

    rsx! {
        section { class: "surface-card", aria_label: "Development proof benchmark",
            p { class: "card-eyebrow", "Development-only proof benchmark" }
            h2 { "Midnight proving envelope" }
            p {
                "Runs one synthetic proof at a time through k=21. Results live only in this process. First runs may download public proving parameters into the app-private cache."
            }
            p { class: "field-hint",
                "k=18–21 can consume substantial memory, time, network, and disk. Oxid intentionally does not run high-k proofs in CI. Leaving this page does not cancel an admitted worker."
            }
            div { class: "button-row",
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
                    class: "secondary-button",
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
            label { class: "confirmation-check",
                input {
                    r#type: "checkbox",
                    checked: high_resource_acknowledged(),
                    disabled: worker_busy || sweeping(),
                    onchange: move |event| high_resource_acknowledged.set(event.checked()),
                }
                span { "I understand that k=18–21 may exhaust this device's resources." }
            }
            p { class: "status-pill", role: "status", "{stage}" }
            if let Some(message) = notice() {
                p { class: "field-error", role: "alert", "{message}" }
            }
            div { class: "developer-capability-list",
                for k in PROOF_BENCHMARK_MIN_K..=PROOF_BENCHMARK_MAX_K {
                    {
                        let outcome = result_snapshot.get(&k).copied();
                        let benchmark = Arc::clone(&benchmark);
                        let high_k_blocked = k >= PROOF_BENCHMARK_HIGH_RESOURCE_K
                            && !high_resource_acknowledged();
                        rsx! {
                            article { class: "developer-capability-row capability-row", key: "proof-k-{k}",
                                span { class: if matches!(outcome, Some(BenchmarkOutcome::Completed(_))) { "capability-dot ready" } else { "capability-dot queued" } }
                                div { class: "developer-capability-row__body",
                                    strong { "Circuit k={k}" }
                                    if let Some(outcome) = outcome {
                                        match outcome {
                                            BenchmarkOutcome::Completed(report) => rsx! {
                                                small { "{report_text(report)}" }
                                            },
                                            BenchmarkOutcome::Failed(error) => rsx! {
                                                small { "{error}" }
                                            },
                                        }
                                    } else {
                                        small { "Not run in this process" }
                                    }
                                }
                                button {
                                    class: "secondary-button",
                                    r#type: "button",
                                    disabled: worker_busy || sweeping() || high_k_blocked,
                                    onclick: move |_| {
                                        notice.set(None);
                                        let benchmark = Arc::clone(&benchmark);
                                        spawn(async move {
                                            let outcome = run_one(benchmark, k).await;
                                            results.write().insert(k, outcome);
                                        });
                                    },
                                    "Run"
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
