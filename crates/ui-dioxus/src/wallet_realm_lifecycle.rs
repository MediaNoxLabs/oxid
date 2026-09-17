// SPDX-License-Identifier: Apache-2.0

//! One foreground driver for the application-owned selected-realm lifecycle.

use super::*;
use oxid_wallet_application::{
    ReconcileWalletRealmLifecycleUseCase, SelectedWalletRealmProjection,
    SelectedWalletRealmSyncCommand, WalletRealmFacetState, WalletRealmLifecycleIdentity,
    WalletRealmLifecycleInput, WalletRealmLifecycleResult, WalletRealmLifecycleSettlement,
    WalletRealmLifecycleStatus, WalletRealmReconciliationState,
};
use std::{sync::OnceLock, time::Instant};

static WALLET_REALM_LIFECYCLE_EPOCH: OnceLock<Instant> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WalletRealmLifecycleWake {
    pub generation: u64,
    pub resumed: bool,
}

impl WalletRealmLifecycleWake {
    pub const INITIAL: Self = Self {
        generation: 0,
        resumed: false,
    };

    pub const fn realm_changed(self) -> Self {
        Self {
            generation: self.generation.wrapping_add(1),
            resumed: false,
        }
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub const fn resumed(self) -> Self {
        Self {
            generation: self.generation.wrapping_add(1),
            resumed: true,
        }
    }
}

pub(super) fn use_wallet_realm_lifecycle_driver(
    services: WalletUiServices,
    profile_session: Signal<ProfileSessionState>,
    lifecycle_wake: Signal<WalletRealmLifecycleWake>,
) {
    use_future(move || {
        let session = profile_session();
        let wake = lifecycle_wake();
        let services = services.clone();
        async move {
            let ProfileSessionState::Active(profile) = session else {
                return;
            };
            drive_selected_realm(services, profile.id, wake.resumed).await;
        }
    });
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) fn background(lifecycle: &Arc<dyn ReconcileWalletRealmLifecycleUseCase>) {
    // Wry defers Dioxus signal writes until resume. Admit this payload-free
    // transition synchronously so late reconciliation cannot be published.
    let _ =
        futures::executor::block_on(lifecycle.execute(WalletRealmLifecycleInput::Backgrounded {
            now_millis: monotonic_millis(),
        }));
}

pub(super) async fn explicit_retry(
    services: WalletUiServices,
    command: SelectedWalletRealmSyncCommand,
) -> Result<SelectedWalletRealmProjection, String> {
    let lifecycle = services.reconcile_wallet_realm_lifecycle();
    let status = lifecycle.status().map_err(|error| error.to_string())?;
    let result = execute_with_timeout(
        lifecycle,
        WalletRealmLifecycleInput::ActionPreflight {
            now_millis: monotonic_millis(),
            facets: status.facets,
        },
    )
    .await
    .map_err(|()| "selected-realm reconciliation did not settle before its deadline".to_owned())?;
    result.projection.map_or_else(
        || {
            services
                .get_selected_wallet_realm_sync()
                .execute(command)
                .map_err(|error| error.to_string())
        },
        Ok,
    )
}

async fn drive_selected_realm(services: WalletUiServices, profile_id: String, resumed: bool) {
    let lifecycle = services.reconcile_wallet_realm_lifecycle();
    let Ok((identity, status)) = selected_identity_and_status(&services, &profile_id).await else {
        return;
    };
    let now_millis = monotonic_millis();
    let input = initial_input(identity, &status, resumed, now_millis);
    // A settled transient error updates the policy's retry/backoff checkpoint.
    // Keep this sole driver alive so it can observe that next wakeup.
    let _ = execute_with_timeout(Arc::clone(&lifecycle), input).await;

    loop {
        let Ok(status) = lifecycle.status() else {
            return;
        };
        let now_millis = monotonic_millis();
        let Some(wait_millis) = next_driver_wait_millis(&status, now_millis) else {
            std::future::pending::<()>().await;
            return;
        };
        tokio::time::sleep(Duration::from_millis(wait_millis)).await;

        // Another caller can own the single in-flight request. Recheck until
        // it settles rather than treating the temporary lack of a policy
        // wakeup as the end of automatic reconciliation.
        if let Ok(latest) = lifecycle.status() {
            if let Some(request) = latest.in_flight {
                let now_millis = monotonic_millis();
                if latest
                    .in_flight_deadline_millis
                    .is_some_and(|deadline| now_millis >= deadline)
                {
                    let _ = execute_with_timeout(
                        Arc::clone(&lifecycle),
                        WalletRealmLifecycleInput::ReconciliationTimedOut {
                            identity: request.identity,
                            sequence: request.sequence,
                            now_millis,
                            facets: latest.facets,
                        },
                    )
                    .await;
                }
                continue;
            }
        }

        let Ok((identity, latest)) = selected_identity_and_status(&services, &profile_id).await
        else {
            return;
        };
        let now_millis = monotonic_millis();
        let input = if latest.identity.as_ref() == Some(&identity) {
            WalletRealmLifecycleInput::PeriodicTick {
                now_millis,
                facets: latest.facets,
            }
        } else {
            WalletRealmLifecycleInput::RealmSelected {
                identity,
                now_millis,
                facets: missing_facets(),
            }
        };
        // Failure is a settled lifecycle outcome with a policy-owned backoff.
        // Only an unreadable status ends the driver on the next iteration.
        let _ = execute_with_timeout(Arc::clone(&lifecycle), input).await;
    }
}

const DRIVER_IN_FLIGHT_POLL_MILLIS: u64 = 250;

fn next_driver_wait_millis(status: &WalletRealmLifecycleStatus, now_millis: u64) -> Option<u64> {
    status
        .next_wakeup_millis
        .map(|wakeup| wakeup.saturating_sub(now_millis).max(1))
        .or_else(|| {
            status.in_flight.as_ref().map(|_| {
                status
                    .in_flight_deadline_millis
                    .map(|deadline| deadline.saturating_sub(now_millis).max(1))
                    .unwrap_or(DRIVER_IN_FLIGHT_POLL_MILLIS)
                    .min(DRIVER_IN_FLIGHT_POLL_MILLIS)
            })
        })
}

async fn selected_identity_and_status(
    services: &WalletUiServices,
    profile_id: &str,
) -> Result<(WalletRealmLifecycleIdentity, WalletRealmLifecycleStatus), UiBlockingTaskError> {
    let list = services.list_wallet_networks();
    let query_profile = profile_id.to_owned();
    let networks = run_ui_blocking(move || {
        list.execute(WalletAccountQuery {
            profile_id: query_profile,
        })
    })
    .await?
    .map_err(|_| UiBlockingTaskError::WorkerFailed)?;
    let identity =
        WalletRealmLifecycleIdentity::parse(profile_id.to_owned(), networks.selected_network_id)
            .map_err(|_| UiBlockingTaskError::WorkerFailed)?;
    let status = services
        .reconcile_wallet_realm_lifecycle()
        .status()
        .map_err(|_| UiBlockingTaskError::WorkerFailed)?;
    Ok((identity, status))
}

fn initial_input(
    identity: WalletRealmLifecycleIdentity,
    status: &WalletRealmLifecycleStatus,
    resumed: bool,
    now_millis: u64,
) -> WalletRealmLifecycleInput {
    if status.identity.as_ref() != Some(&identity) {
        let facets = missing_facets();
        if status.identity.is_some() {
            WalletRealmLifecycleInput::RealmSelected {
                identity,
                now_millis,
                facets,
            }
        } else {
            WalletRealmLifecycleInput::Initialized {
                identity,
                now_millis,
                facets,
            }
        }
    } else if resumed {
        WalletRealmLifecycleInput::Foreground {
            now_millis,
            facets: status.facets,
        }
    } else {
        WalletRealmLifecycleInput::PeriodicTick {
            now_millis,
            facets: status.facets,
        }
    }
}

async fn execute_with_timeout(
    lifecycle: Arc<dyn ReconcileWalletRealmLifecycleUseCase>,
    mut input: WalletRealmLifecycleInput,
) -> Result<WalletRealmLifecycleResult, ()> {
    let timeout_millis = lifecycle.status().map_err(|_| ())?.request_timeout_millis;
    let mut expected_settlement = None;
    // The policy retains at most one action preflight behind an admitted
    // reconciliation. A timeout can therefore expose at most one follow-up
    // drain. Keep one final bounded attempt for settling that follow-up.
    for _ in 0..3 {
        match tokio::time::timeout(
            Duration::from_millis(timeout_millis),
            lifecycle.execute(input),
        )
        .await
        {
            Ok(Ok(result)) => {
                return match expected_settlement {
                    Some(expected)
                        if !matches!(
                            result.settlement,
                            Some(WalletRealmLifecycleSettlement::TimedOut(ref settled))
                                if settled == &expected
                        ) =>
                    {
                        Err(())
                    }
                    _ => Ok(result),
                };
            }
            Ok(Err(_)) => return Err(()),
            Err(_) => {
                let status = lifecycle.status().map_err(|_| ())?;
                let request = status.in_flight.ok_or(())?;
                input = WalletRealmLifecycleInput::ReconciliationTimedOut {
                    identity: request.identity.clone(),
                    sequence: request.sequence,
                    now_millis: monotonic_millis(),
                    facets: status.facets,
                };
                expected_settlement = Some(request);
            }
        }
    }
    Err(())
}

pub(super) fn monotonic_millis() -> u64 {
    u64::try_from(
        WALLET_REALM_LIFECYCLE_EPOCH
            .get_or_init(Instant::now)
            .elapsed()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

const fn missing_facets() -> WalletRealmReconciliationState {
    WalletRealmReconciliationState {
        account: WalletRealmFacetState::Missing,
        dust: WalletRealmFacetState::Missing,
        shielded: WalletRealmFacetState::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(profile: &str, realm: &str) -> WalletRealmLifecycleIdentity {
        WalletRealmLifecycleIdentity::parse(profile.to_owned(), realm.to_owned())
            .expect("valid identity")
    }

    fn status(active: Option<WalletRealmLifecycleIdentity>) -> WalletRealmLifecycleStatus {
        WalletRealmLifecycleStatus {
            identity: active,
            facets: missing_facets(),
            observed_at_millis: 0,
            next_wakeup_millis: None,
            in_flight: None,
            in_flight_deadline_millis: None,
            request_timeout_millis: 10,
        }
    }

    #[test]
    fn initial_signal_distinguishes_start_resume_and_realm_switch() {
        let selected = identity("profile_one", "undeployed");
        assert!(matches!(
            initial_input(selected.clone(), &status(None), false, 1),
            WalletRealmLifecycleInput::Initialized { .. }
        ));
        assert!(matches!(
            initial_input(selected.clone(), &status(Some(selected.clone())), true, 2),
            WalletRealmLifecycleInput::Foreground { .. }
        ));
        assert!(matches!(
            initial_input(
                selected,
                &status(Some(identity("profile_two", "preprod"))),
                false,
                3
            ),
            WalletRealmLifecycleInput::RealmSelected { .. }
        ));
    }

    #[test]
    fn driver_waits_for_policy_wakeup_and_rechecks_concurrent_work() {
        let mut scheduled = status(Some(identity("profile_one", "undeployed")));
        scheduled.next_wakeup_millis = Some(1_250);
        assert_eq!(next_driver_wait_millis(&scheduled, 1_000), Some(250));

        scheduled.next_wakeup_millis = None;
        scheduled.in_flight = Some(oxid_wallet_application::WalletRealmLifecycleRequest {
            identity: identity("profile_one", "undeployed"),
            trigger: oxid_wallet_application::WalletRealmReconciliationTrigger::ManualRefresh,
            sequence: 1,
        });
        scheduled.in_flight_deadline_millis = Some(2_000);
        assert_eq!(next_driver_wait_millis(&scheduled, 1_000), Some(250));
        assert_eq!(next_driver_wait_millis(&scheduled, 1_999), Some(1));

        scheduled.in_flight = None;
        scheduled.in_flight_deadline_millis = None;
        assert_eq!(next_driver_wait_millis(&scheduled, 1_000), None);
    }
}
