// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::VecDeque,
    future::{Future, poll_fn, ready},
    pin::pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};

use oxid_wallet_domain::{
    ChainNetworkId, ChainTransactionId, WalletProfileId, WalletTransactionDraftId,
};

use super::*;
use crate::{
    WalletDustRegistrationSettlementIdentity as Identity,
    WalletDustRegistrationSettlementReconciliation as Reconciliation,
    WalletDustRegistrationSettlementState as State, completion,
};

fn identity(generation: u64) -> Identity {
    Identity {
        profile: WalletProfileId::parse("profile_test").unwrap(),
        realm: ChainNetworkId::parse("undeployed").unwrap(),
        generation,
    }
}

fn draft() -> WalletTransactionDraftId {
    WalletTransactionDraftId::parse("dustreg_test").unwrap()
}

fn transaction() -> ChainTransactionId {
    ChainTransactionId::parse("tx_test").unwrap()
}

fn eligibility(generation: u64, revision: u64) -> WalletDustRegistrationSettlementEvent {
    WalletDustRegistrationSettlementEvent::Eligibility {
        identity: identity(generation),
        revision,
        eligible: true,
    }
}

fn resolve<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture future must resolve immediately"),
    }
}

#[derive(Default)]
struct ScriptedExecutor {
    completions: Mutex<
        VecDeque<
            Result<WalletDustRegistrationSettlementEvent, WalletDustRegistrationExecutorFailure>,
        >,
    >,
    operations: Mutex<Vec<WalletDustRegistrationRuntimeOperation>>,
}

impl ScriptedExecutor {
    fn new(
        completions: impl IntoIterator<
            Item = Result<
                WalletDustRegistrationSettlementEvent,
                WalletDustRegistrationExecutorFailure,
            >,
        >,
    ) -> Self {
        Self {
            completions: Mutex::new(completions.into_iter().collect()),
            operations: Mutex::new(Vec::new()),
        }
    }

    fn operation_count(&self) -> usize {
        self.operations.lock().unwrap().len()
    }
}

impl ExecuteWalletDustRegistrationOperation for ScriptedExecutor {
    fn execute(
        &self,
        operation: WalletDustRegistrationRuntimeOperation,
    ) -> WalletDustRegistrationOperationFuture<'_> {
        self.operations.lock().unwrap().push(operation);
        let completion = self
            .completions
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted completion");
        Box::pin(ready(
            completion.map(WalletDustRegistrationOperationCompletion),
        ))
    }
}

fn successful_script() -> Arc<ScriptedExecutor> {
    Arc::new(ScriptedExecutor::new([
        Ok(completion::prepared(identity(1), draft(), 1)),
        Ok(completion::authorized(identity(1), draft())),
        Ok(completion::submitted(identity(1), draft(), transaction())),
        Ok(completion::reconciled(
            identity(1),
            transaction(),
            1,
            Reconciliation::Included,
        )),
        Ok(completion::dust_refreshed(
            identity(1),
            transaction(),
            1,
            1,
            true,
        )),
    ]))
}

#[test]
fn preparation_stops_at_authorization_then_one_authorization_reaches_ready() {
    let executor = successful_script();
    let driver = WalletDustRegistrationDriver::new(executor.clone());

    let prepared = resolve(driver.advance(eligibility(1, 1))).unwrap();
    assert_eq!(prepared.state, State::AwaitingAuthorization);
    assert_eq!(executor.operation_count(), 1);

    let ready = resolve(driver.authorize()).unwrap();
    assert_eq!(ready.state, State::Ready);
    assert_eq!(executor.operation_count(), 5);
    assert_eq!(driver.projection().unwrap(), ready);
}

#[test]
fn rejected_authorization_never_submits() {
    let executor = Arc::new(ScriptedExecutor::new([
        Ok(completion::prepared(identity(1), draft(), 1)),
        Ok(completion::authorization_rejected(identity(1), draft())),
    ]));
    let driver = WalletDustRegistrationDriver::new(executor.clone());

    assert_eq!(
        resolve(driver.advance(eligibility(1, 1))).unwrap().state,
        State::AwaitingAuthorization
    );
    assert_eq!(
        resolve(driver.authorize()).unwrap().state,
        State::ActionRequired
    );
    assert_eq!(executor.operation_count(), 2);
}

#[test]
fn executor_failure_releases_admission_for_a_bounded_retry() {
    let executor = Arc::new(ScriptedExecutor::new([
        Err(WalletDustRegistrationExecutorFailure::Offline),
        Ok(completion::prepared(identity(1), draft(), 1)),
    ]));
    let driver = WalletDustRegistrationDriver::new(executor.clone());

    assert_eq!(
        resolve(driver.advance(eligibility(1, 1))),
        Err(WalletDustRegistrationDriverError::Executor(
            WalletDustRegistrationExecutorFailure::Offline
        ))
    );
    assert_eq!(driver.projection().unwrap().state, State::ActionRequired);
    assert_eq!(
        resolve(driver.advance(eligibility(1, 2))).unwrap().state,
        State::AwaitingAuthorization
    );
    assert_eq!(executor.operation_count(), 2);
}

#[test]
fn invalid_completion_is_rejected_and_does_not_pin_admission() {
    let executor = Arc::new(ScriptedExecutor::new([
        Ok(completion::prepared(identity(2), draft(), 1)),
        Ok(completion::prepared(identity(1), draft(), 1)),
    ]));
    let driver = WalletDustRegistrationDriver::new(executor);

    assert_eq!(
        resolve(driver.advance(eligibility(1, 1))),
        Err(WalletDustRegistrationDriverError::InvalidCompletion)
    );
    assert_eq!(
        resolve(driver.advance(eligibility(1, 2))).unwrap().state,
        State::AwaitingAuthorization
    );
}

struct GatedExecutor {
    released: Arc<AtomicBool>,
    calls: Mutex<usize>,
}

impl ExecuteWalletDustRegistrationOperation for GatedExecutor {
    fn execute(
        &self,
        _operation: WalletDustRegistrationRuntimeOperation,
    ) -> WalletDustRegistrationOperationFuture<'_> {
        *self.calls.lock().unwrap() += 1;
        let released = self.released.clone();
        Box::pin(poll_fn(move |_| {
            if released.load(Ordering::SeqCst) {
                Poll::Ready(Ok(WalletDustRegistrationOperationCompletion(
                    completion::prepared(identity(1), draft(), 1),
                )))
            } else {
                Poll::Pending
            }
        }))
    }
}

#[test]
fn concurrent_drive_observes_busy_without_duplicate_io_or_held_mutex() {
    let released = Arc::new(AtomicBool::new(false));
    let executor = Arc::new(GatedExecutor {
        released: released.clone(),
        calls: Mutex::new(0),
    });
    let driver = WalletDustRegistrationDriver::new(executor.clone());
    let mut first = Box::pin(driver.advance(eligibility(1, 1)));
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));

    assert_eq!(driver.projection().unwrap().state, State::ActionRequired);
    assert_eq!(
        resolve(driver.advance(eligibility(1, 1))).unwrap().state,
        State::ActionRequired
    );
    assert_eq!(*executor.calls.lock().unwrap(), 1);

    released.store(true, Ordering::SeqCst);
    let Poll::Ready(Ok(prepared)) = first.as_mut().poll(&mut context) else {
        panic!("released preparation must complete");
    };
    assert_eq!(prepared.state, State::AwaitingAuthorization);
}

#[test]
fn supersession_does_not_start_a_second_worker_before_the_first_settles() {
    let released = Arc::new(AtomicBool::new(false));
    let executor = Arc::new(GatedExecutor {
        released: released.clone(),
        calls: Mutex::new(0),
    });
    let driver = WalletDustRegistrationDriver::new(executor.clone());
    let mut first = pin!(driver.advance(eligibility(1, 1)));
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));

    let replacement = resolve(driver.advance(eligibility(2, 1))).unwrap();
    assert_eq!(replacement.state, State::ActionRequired);
    assert_eq!(replacement.identity, Some(identity(2)));
    assert_eq!(*executor.calls.lock().unwrap(), 1);

    released.store(true, Ordering::SeqCst);
    assert_eq!(
        first.as_mut().poll(&mut context),
        Poll::Ready(Err(WalletDustRegistrationDriverError::InvalidCompletion))
    );
    assert_eq!(*executor.calls.lock().unwrap(), 1);
}

#[test]
fn authorization_is_bound_to_the_checked_identity_and_draft() {
    let executor = Arc::new(ScriptedExecutor::new([
        Ok(completion::prepared(identity(1), draft(), 1)),
        Ok(completion::prepared(identity(2), draft(), 1)),
    ]));
    let driver = WalletDustRegistrationDriver::new(executor.clone());
    assert_eq!(
        resolve(driver.advance(eligibility(1, 1))).unwrap().state,
        State::AwaitingAuthorization
    );
    let checked_target = driver
        .runtime
        .lock()
        .unwrap()
        .coordinator()
        .active_effect()
        .cloned()
        .unwrap();

    assert_eq!(
        resolve(driver.advance(eligibility(2, 1))).unwrap().state,
        State::AwaitingAuthorization
    );
    assert_eq!(
        resolve(driver.drain(Some(checked_target))),
        Err(WalletDustRegistrationDriverError::AuthorizationNotPending)
    );
    assert_eq!(executor.operation_count(), 2);
}

#[test]
fn dropping_an_executor_future_releases_runtime_and_driver_admission() {
    let released = Arc::new(AtomicBool::new(false));
    let executor = Arc::new(GatedExecutor {
        released: released.clone(),
        calls: Mutex::new(0),
    });
    let driver = WalletDustRegistrationDriver::new(executor.clone());
    let mut first = Box::pin(driver.advance(eligibility(1, 1)));
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));
    drop(first);

    released.store(true, Ordering::SeqCst);
    assert_eq!(
        resolve(driver.drain(None)).unwrap().state,
        State::AwaitingAuthorization
    );
    assert_eq!(*executor.calls.lock().unwrap(), 2);
}

#[test]
fn lifecycle_recovery_and_supersession_are_policy_owned() {
    let executor = Arc::new(ScriptedExecutor::new([Ok(completion::prepared(
        identity(1),
        draft(),
        1,
    ))]));
    let driver = WalletDustRegistrationDriver::new(executor);
    assert_eq!(
        resolve(driver.advance(eligibility(1, 1))).unwrap().state,
        State::AwaitingAuthorization
    );

    assert_eq!(
        resolve(
            driver.advance(WalletDustRegistrationSettlementEvent::Suspended {
                identity: identity(1),
                revision: 1,
            })
        )
        .unwrap()
        .state,
        State::Suspended
    );
    assert_eq!(
        resolve(
            driver.advance(WalletDustRegistrationSettlementEvent::Resumed {
                identity: identity(1),
                revision: 2,
            })
        )
        .unwrap()
        .state,
        State::AwaitingAuthorization
    );
    assert_eq!(
        resolve(
            driver.advance(WalletDustRegistrationSettlementEvent::Superseded {
                identity: identity(1),
            })
        )
        .unwrap()
        .state,
        State::Unavailable
    );
}

#[test]
fn executor_completion_cannot_enter_through_external_observation() {
    let driver = WalletDustRegistrationDriver::new(Arc::new(ScriptedExecutor::default()));
    assert_eq!(
        resolve(driver.advance(completion::authorized(identity(1), draft()))),
        Err(WalletDustRegistrationDriverError::InvalidObservation)
    );
}

#[test]
fn authorization_is_rejected_until_the_exact_boundary_is_pending() {
    let driver = WalletDustRegistrationDriver::new(Arc::new(ScriptedExecutor::default()));
    assert_eq!(
        resolve(driver.authorize()),
        Err(WalletDustRegistrationDriverError::AuthorizationNotPending)
    );
}

#[test]
fn cancellation_offline_and_timeout_keep_deterministic_public_states() {
    for (event, expected) in [
        (
            WalletDustRegistrationSettlementEvent::Cancelled {
                identity: identity(1),
                draft_id: draft(),
            },
            State::Cancelled,
        ),
        (
            WalletDustRegistrationSettlementEvent::Offline {
                identity: identity(1),
                revision: 1,
            },
            State::Offline,
        ),
        (
            WalletDustRegistrationSettlementEvent::TimedOut {
                identity: identity(1),
                revision: 1,
            },
            State::TimedOut,
        ),
    ] {
        let executor = Arc::new(ScriptedExecutor::new([Ok(completion::prepared(
            identity(1),
            draft(),
            1,
        ))]));
        let driver = WalletDustRegistrationDriver::new(executor);
        assert_eq!(
            resolve(driver.advance(eligibility(1, 1))).unwrap().state,
            State::AwaitingAuthorization
        );
        assert_eq!(resolve(driver.advance(event)).unwrap().state, expected);
    }
}
