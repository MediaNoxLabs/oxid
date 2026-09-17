// SPDX-License-Identifier: Apache-2.0

use futures::{
    executor::block_on,
    future::{Either, select},
    pin_mut,
};
use futures_timer::Delay;
use oxid_wallet_application::{
    ReconcileWalletRealmLifecycleUseCase, WalletAccountQuery, WalletRealmFacetState,
    WalletRealmLifecycleIdentity, WalletRealmLifecycleInput, WalletRealmLifecycleResult,
    WalletRealmReconciliationState,
};
use std::{
    future::Future,
    sync::{Arc, mpsc::Receiver},
    time::Duration,
};

use crate::HeadlessWallet;

const HEADLESS_STARTUP_RECONCILIATION_TIMEOUT_MILLIS: u64 = 1_000;
const HEADLESS_SCHEDULER_MAX_SLEEP_MILLIS: u64 = 1_000;

impl HeadlessWallet {
    pub(super) fn initialize_active_realm(&self) {
        let Ok(Some(profile)) = self.application.get_active_wallet_profile().execute() else {
            return;
        };
        let _ = self.drive_selected_realm(profile.id);
    }

    pub(super) fn drive_selected_realm(&self, profile_id: String) -> Result<(), ()> {
        let networks = self
            .application
            .list_wallet_networks()
            .execute(WalletAccountQuery {
                profile_id: profile_id.clone(),
            })
            .map_err(|_| ())?;
        let identity =
            WalletRealmLifecycleIdentity::parse(profile_id, networks.selected_network_id)
                .map_err(|_| ())?;
        let lifecycle = self.application.reconcile_wallet_realm_lifecycle();
        let status = lifecycle.status().map_err(|_| ())?;
        let facets = if status.identity.as_ref() == Some(&identity) {
            status.facets
        } else {
            missing_facets()
        };
        let input = match status.identity {
            None => WalletRealmLifecycleInput::Initialized {
                identity,
                now_millis: self.monotonic_millis(),
                facets,
            },
            Some(active) if active != identity => WalletRealmLifecycleInput::RealmSelected {
                identity,
                now_millis: self.monotonic_millis(),
                facets,
            },
            Some(_) => WalletRealmLifecycleInput::Foreground {
                now_millis: self.monotonic_millis(),
                facets,
            },
        };
        execute_bounded(
            lifecycle,
            input,
            HEADLESS_STARTUP_RECONCILIATION_TIMEOUT_MILLIS,
        )
        .map(|_| ())
    }

    pub(super) fn run_lifecycle_scheduler(&self, stop: &Receiver<()>) {
        loop {
            let lifecycle = self.application.reconcile_wallet_realm_lifecycle();
            let wait_millis =
                lifecycle
                    .status()
                    .map_or(HEADLESS_SCHEDULER_MAX_SLEEP_MILLIS, |status| {
                        let now_millis = self.monotonic_millis();
                        if let Some(request) = status.in_flight {
                            if status
                                .in_flight_deadline_millis
                                .is_some_and(|deadline| now_millis >= deadline)
                            {
                                let _ = execute_bounded(
                                    Arc::clone(&lifecycle),
                                    WalletRealmLifecycleInput::ReconciliationTimedOut {
                                        identity: request.identity,
                                        sequence: request.sequence,
                                        now_millis,
                                        facets: status.facets,
                                    },
                                    HEADLESS_STARTUP_RECONCILIATION_TIMEOUT_MILLIS,
                                );
                                return 1;
                            }
                            return status
                                .in_flight_deadline_millis
                                .map(|deadline| deadline.saturating_sub(now_millis).max(1))
                                .unwrap_or(HEADLESS_SCHEDULER_MAX_SLEEP_MILLIS)
                                .min(HEADLESS_SCHEDULER_MAX_SLEEP_MILLIS);
                        }
                        if let Some(wakeup) = status.next_wakeup_millis {
                            if wakeup <= now_millis {
                                let _ = execute_bounded(
                                    lifecycle,
                                    WalletRealmLifecycleInput::PeriodicTick {
                                        now_millis,
                                        facets: status.facets,
                                    },
                                    u64::MAX,
                                );
                                return 1;
                            }
                            return wakeup
                                .saturating_sub(now_millis)
                                .max(1)
                                .min(HEADLESS_SCHEDULER_MAX_SLEEP_MILLIS);
                        }
                        HEADLESS_SCHEDULER_MAX_SLEEP_MILLIS
                    });
            if stop
                .recv_timeout(Duration::from_millis(wait_millis))
                .is_ok()
                || matches!(
                    stop.try_recv(),
                    Err(std::sync::mpsc::TryRecvError::Disconnected)
                )
            {
                return;
            }
        }
    }

    pub(super) fn execute_realm_lifecycle(
        &self,
        input: WalletRealmLifecycleInput,
    ) -> Result<WalletRealmLifecycleResult, ()> {
        execute_bounded(
            self.application.reconcile_wallet_realm_lifecycle(),
            input,
            u64::MAX,
        )
    }

    pub(super) fn await_realm_lifecycle_idle(&self) -> Result<(), ()> {
        let lifecycle = self.application.reconcile_wallet_realm_lifecycle();
        let timeout_millis = lifecycle
            .status()
            .map_err(|_| ())?
            .request_timeout_millis
            .saturating_mul(2);
        let started = std::time::Instant::now();
        loop {
            let status = lifecycle.status().map_err(|_| ())?;
            let Some(request) = status.in_flight else {
                return Ok(());
            };
            let now_millis = self.monotonic_millis();
            if status
                .in_flight_deadline_millis
                .is_some_and(|deadline| now_millis >= deadline)
            {
                let _ = execute_bounded(
                    Arc::clone(&lifecycle),
                    WalletRealmLifecycleInput::ReconciliationTimedOut {
                        identity: request.identity,
                        sequence: request.sequence,
                        now_millis,
                        facets: status.facets,
                    },
                    HEADLESS_STARTUP_RECONCILIATION_TIMEOUT_MILLIS,
                )?;
            } else {
                std::thread::sleep(Duration::from_millis(25));
            }
            if u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX) >= timeout_millis {
                return Err(());
            }
        }
    }

    pub(super) fn monotonic_millis(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

fn execute_bounded(
    lifecycle: Arc<dyn ReconcileWalletRealmLifecycleUseCase>,
    input: WalletRealmLifecycleInput,
    maximum_timeout_millis: u64,
) -> Result<WalletRealmLifecycleResult, ()> {
    let timeout_millis = lifecycle
        .status()
        .map_err(|_| ())?
        .request_timeout_millis
        .min(maximum_timeout_millis);
    match block_on_with_timeout(lifecycle.execute(input), timeout_millis) {
        Some(Ok(result)) => Ok(result),
        Some(Err(_)) => Err(()),
        None => {
            let status = lifecycle.status().map_err(|_| ())?;
            let request = status.in_flight.ok_or(())?;
            block_on(
                lifecycle.execute(WalletRealmLifecycleInput::ReconciliationTimedOut {
                    identity: request.identity,
                    sequence: request.sequence,
                    now_millis: status.observed_at_millis.saturating_add(timeout_millis),
                    facets: status.facets,
                }),
            )
            .map_err(|_| ())
        }
    }
}

fn block_on_with_timeout<F>(future: F, timeout_millis: u64) -> Option<F::Output>
where
    F: Future,
{
    block_on(async move {
        let timeout = Delay::new(Duration::from_millis(timeout_millis.max(1)));
        pin_mut!(future, timeout);
        match select(future, timeout).await {
            Either::Left((output, _)) => Some(output),
            Either::Right(((), _)) => None,
        }
    })
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
    use oxid_wallet_application::{CreateWalletProfileCommand, SelectWalletProfileCommand};
    use std::future::pending;

    #[test]
    fn startup_initializes_the_active_selected_realm_without_a_sync_request() {
        let application = oxid_composition::compose_in_memory();
        let created = application
            .create_wallet_profile()
            .execute(CreateWalletProfileCommand {
                display_name: "Headless lifecycle".to_owned(),
            })
            .expect("profile creation");
        application
            .select_wallet_profile()
            .execute(SelectWalletProfileCommand {
                profile_id: created.id,
            })
            .expect("profile selection");
        let wallet = HeadlessWallet::new(application);
        let active = wallet
            .application
            .get_active_wallet_profile()
            .execute()
            .expect("active profile read")
            .expect("active profile");
        let status = wallet
            .application
            .reconcile_wallet_realm_lifecycle()
            .status()
            .expect("lifecycle status");

        assert_eq!(
            status
                .identity
                .as_ref()
                .map(|identity| identity.profile.as_str()),
            Some(active.id.as_str())
        );
        assert!(status.in_flight.is_none());
        assert!(status.next_wakeup_millis.is_some());
    }

    #[test]
    fn stalled_startup_work_is_bounded_before_the_request_loop() {
        assert_eq!(block_on_with_timeout(pending::<()>(), 1), None);
    }
}
