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
            &|| self.monotonic_millis(),
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
                                    &|| self.monotonic_millis(),
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
                                if execute_bounded_until_stopped(
                                    lifecycle,
                                    WalletRealmLifecycleInput::PeriodicTick {
                                        now_millis,
                                        facets: status.facets,
                                    },
                                    u64::MAX,
                                    stop,
                                    &|| self.monotonic_millis(),
                                )
                                .is_none()
                                {
                                    return 1;
                                }
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
            &|| self.monotonic_millis(),
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
                    &|| self.monotonic_millis(),
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
    mut input: WalletRealmLifecycleInput,
    maximum_timeout_millis: u64,
    now_millis: &dyn Fn() -> u64,
) -> Result<WalletRealmLifecycleResult, ()> {
    let timeout_millis = lifecycle
        .status()
        .map_err(|_| ())?
        .request_timeout_millis
        .min(maximum_timeout_millis);
    for _ in 0..3 {
        let execution = lifecycle.begin(input).map_err(|_| ())?;
        let handle = execution.handle;
        match block_on_with_timeout(execution.future, timeout_millis) {
            Some(Ok(result)) => {
                observe_successful_completion(&lifecycle, &result, now_millis())?;
                return Ok(result);
            }
            Some(Err(_)) => return Err(()),
            None => {
                let request = handle.request().map_err(|_| ())?.ok_or(())?;
                let status = lifecycle.status().map_err(|_| ())?;
                input = WalletRealmLifecycleInput::ReconciliationTimedOut {
                    identity: request.identity,
                    sequence: request.sequence,
                    now_millis: now_millis(),
                    facets: status.facets,
                };
            }
        }
    }
    Err(())
}

fn execute_bounded_until_stopped(
    lifecycle: Arc<dyn ReconcileWalletRealmLifecycleUseCase>,
    input: WalletRealmLifecycleInput,
    maximum_timeout_millis: u64,
    stop: &Receiver<()>,
    now_millis: &dyn Fn() -> u64,
) -> Option<Result<WalletRealmLifecycleResult, ()>> {
    let timeout_millis = lifecycle
        .status()
        .ok()?
        .request_timeout_millis
        .min(maximum_timeout_millis);
    let execution = lifecycle.begin(input).ok()?;
    let handle = execution.handle;
    match block_on_with_timeout_or_stop(execution.future, timeout_millis, stop) {
        BoundedWait::Completed(Ok(result)) => {
            Some(observe_successful_completion(&lifecycle, &result, now_millis()).map(|()| result))
        }
        BoundedWait::Completed(Err(_)) => Some(Err(())),
        BoundedWait::TimedOut => {
            let request = handle.request().ok()??;
            let status = lifecycle.status().ok()?;
            Some(execute_bounded(
                Arc::clone(&lifecycle),
                WalletRealmLifecycleInput::ReconciliationTimedOut {
                    identity: request.identity,
                    sequence: request.sequence,
                    now_millis: now_millis(),
                    facets: status.facets,
                },
                maximum_timeout_millis,
                now_millis,
            ))
        }
        BoundedWait::Stopped => {
            let _ = block_on(lifecycle.execute(WalletRealmLifecycleInput::Backgrounded {
                now_millis: now_millis(),
            }));
            None
        }
    }
}

fn observe_successful_completion(
    lifecycle: &Arc<dyn ReconcileWalletRealmLifecycleUseCase>,
    result: &WalletRealmLifecycleResult,
    now_millis: u64,
) -> Result<(), ()> {
    let Some(projection) = result.projection.as_ref() else {
        return Ok(());
    };
    let identity = WalletRealmLifecycleIdentity::parse(
        projection.identity.profile.as_str().to_owned(),
        projection.identity.realm.as_str().to_owned(),
    )
    .map_err(|_| ())?;
    let facets = lifecycle.status().map_err(|_| ())?.facets;
    block_on(
        lifecycle.execute(WalletRealmLifecycleInput::ReconciliationObserved {
            identity,
            now_millis,
            facets,
        }),
    )
    .map(|_| ())
    .map_err(|_| ())
}

enum BoundedWait<T> {
    Completed(T),
    TimedOut,
    Stopped,
}

fn block_on_with_timeout_or_stop<F>(
    future: F,
    timeout_millis: u64,
    stop: &Receiver<()>,
) -> BoundedWait<F::Output>
where
    F: Future,
{
    block_on(async move {
        let timeout = Delay::new(Duration::from_millis(timeout_millis.max(1)));
        let timed = async move {
            pin_mut!(future, timeout);
            match select(future, timeout).await {
                Either::Left((output, _)) => BoundedWait::Completed(output),
                Either::Right(((), _)) => BoundedWait::TimedOut,
            }
        };
        let stopped = async {
            loop {
                match stop.try_recv() {
                    Ok(()) | Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        Delay::new(Duration::from_millis(25)).await;
                    }
                }
            }
        };
        pin_mut!(timed, stopped);
        match select(timed, stopped).await {
            Either::Left((outcome, _)) => outcome,
            Either::Right(((), _)) => BoundedWait::Stopped,
        }
    })
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
    use oxid_wallet_application::{
        CreateWalletProfileCommand, SelectWalletProfileCommand, WalletActionWatch,
        WalletActionWatchObservation, WalletActionWatchState,
    };
    use oxid_wallet_domain::{ChainAccountId, ChainTransactionId};
    use std::future::pending;

    fn initialized_wallet(name: &str) -> HeadlessWallet {
        let application = oxid_composition::compose_in_memory();
        let created = application
            .create_wallet_profile()
            .execute(CreateWalletProfileCommand {
                display_name: name.to_owned(),
            })
            .expect("profile creation");
        application
            .select_wallet_profile()
            .execute(SelectWalletProfileCommand {
                profile_id: created.id,
            })
            .expect("profile selection");
        HeadlessWallet::new(application)
    }

    #[test]
    fn startup_initializes_the_active_selected_realm_without_a_sync_request() {
        let wallet = initialized_wallet("Headless lifecycle");
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
    fn headless_adapter_settles_only_the_exact_submitted_transaction() {
        let wallet = initialized_wallet("Headless send watch");
        let watches = wallet.application.manage_wallet_action_watch();
        let transaction = ChainTransactionId::parse("tx_exact").expect("transaction id");
        let handle = watches
            .admit(
                WalletActionWatch::SubmittedTransaction {
                    transaction: transaction.clone(),
                },
                100,
                0,
            )
            .expect("watch admission")
            .expect("selected realm admits the watch");

        watches
            .observe(
                handle,
                WalletActionWatchObservation::SubmittedTransaction {
                    transaction: ChainTransactionId::parse("tx_unrelated")
                        .expect("unrelated transaction id"),
                },
                1,
            )
            .expect("unrelated observation");
        assert_eq!(
            watches
                .projection()
                .expect("projection query")
                .expect("watch projection")
                .state,
            WalletActionWatchState::Waiting
        );

        watches
            .observe(
                handle,
                WalletActionWatchObservation::SubmittedTransaction { transaction },
                2,
            )
            .expect("matching observation");
        assert_eq!(
            watches
                .projection()
                .expect("projection query")
                .expect("watch projection")
                .state,
            WalletActionWatchState::Confirmed
        );
    }

    #[test]
    fn headless_arrival_watch_respects_checkpoint_and_suspension() {
        let wallet = initialized_wallet("Headless receive watch");
        let watches = wallet.application.manage_wallet_action_watch();
        let account = ChainAccountId::parse("account_exact").expect("account id");
        let handle = watches
            .admit(
                WalletActionWatch::IncomingArrival {
                    account: account.clone(),
                    starting_checkpoint: 7,
                },
                100,
                0,
            )
            .expect("watch admission")
            .expect("selected realm admits the watch");

        watches.suspend(handle).expect("suspend watch");
        watches
            .observe(
                handle,
                WalletActionWatchObservation::IncomingArrival {
                    account: account.clone(),
                    checkpoint: 8,
                },
                1,
            )
            .expect("suspended observation");
        assert_eq!(
            watches
                .projection()
                .expect("projection query")
                .expect("watch projection")
                .state,
            WalletActionWatchState::Waiting
        );

        watches.resume(handle).expect("resume watch");
        watches
            .observe(
                handle,
                WalletActionWatchObservation::IncomingArrival {
                    account,
                    checkpoint: 7,
                },
                2,
            )
            .expect("same-checkpoint observation");
        assert_eq!(
            watches
                .projection()
                .expect("projection query")
                .expect("watch projection")
                .state,
            WalletActionWatchState::Waiting
        );

        watches
            .observe(
                handle,
                WalletActionWatchObservation::IncomingArrival {
                    account: ChainAccountId::parse("account_exact").expect("account id"),
                    checkpoint: 8,
                },
                3,
            )
            .expect("later observation");
        assert_eq!(
            watches
                .projection()
                .expect("projection query")
                .expect("watch projection")
                .state,
            WalletActionWatchState::Confirmed
        );
    }

    #[test]
    fn stalled_startup_work_is_bounded_before_the_request_loop() {
        assert_eq!(block_on_with_timeout(pending::<()>(), 1), None);
    }

    #[test]
    fn scheduler_wait_is_interrupted_when_the_protocol_loop_stops() {
        let (sender, receiver) = std::sync::mpsc::channel();
        drop(sender);
        assert!(matches!(
            block_on_with_timeout_or_stop(pending::<()>(), 1_000, &receiver),
            BoundedWait::Stopped
        ));
    }
}
