// SPDX-License-Identifier: Apache-2.0

//! Stateful execution boundary for selected-realm lifecycle reconciliation.

use crate::{
    ReconcileSelectedWalletRealmUseCase, SelectedWalletRealmProjection,
    SelectedWalletRealmSyncCommand, SelectedWalletRealmSyncError, WalletRealmFacetState,
    WalletRealmLifecycleDecision, WalletRealmLifecycleIdentity, WalletRealmLifecycleInput,
    WalletRealmLifecyclePolicy, WalletRealmLifecyclePolicyConfig, WalletRealmLifecycleRequest,
    WalletRealmReconciliationState,
};
use oxid_platform_ports::{ClockPort, PlatformError};
use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

const MAX_DRAINED_REQUESTS: usize = 32;

pub type WalletRealmLifecycleFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<WalletRealmLifecycleResult, WalletRealmLifecycleError>>
            + Send
            + 'a,
    >,
>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletRealmLifecycleError {
    Clock(PlatformError),
    Sync(SelectedWalletRealmSyncError),
    DrainLimit,
    Poisoned,
}

impl fmt::Display for WalletRealmLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock(error) => error.fmt(formatter),
            Self::Sync(error) => error.fmt(formatter),
            Self::DrainLimit => formatter.write_str("wallet realm lifecycle drain limit reached"),
            Self::Poisoned => formatter.write_str("wallet realm lifecycle state is unavailable"),
        }
    }
}

impl Error for WalletRealmLifecycleError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRealmLifecycleResult {
    pub decision: WalletRealmLifecycleDecision,
    pub projection: Option<SelectedWalletRealmProjection>,
}

pub trait ReconcileWalletRealmLifecycleUseCase: Send + Sync {
    fn execute(&self, input: WalletRealmLifecycleInput) -> WalletRealmLifecycleFuture<'_>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WalletRealmLifecycleCheckpoint {
    identity: Option<WalletRealmLifecycleIdentity>,
    facets: WalletRealmReconciliationState,
    now_millis: u64,
}

impl WalletRealmLifecycleCheckpoint {
    fn new(facets: WalletRealmReconciliationState) -> Self {
        Self {
            identity: None,
            facets,
            now_millis: 0,
        }
    }

    fn observe(&mut self, input: &WalletRealmLifecycleInput) {
        match input {
            WalletRealmLifecycleInput::Initialized {
                identity,
                now_millis,
                facets,
            }
            | WalletRealmLifecycleInput::RealmSelected {
                identity,
                now_millis,
                facets,
            } => {
                self.identity = Some(identity.clone());
                self.facets = *facets;
                self.now_millis = *now_millis;
            }
            WalletRealmLifecycleInput::Foreground { now_millis, facets }
            | WalletRealmLifecycleInput::ConnectivityRestored { now_millis, facets }
            | WalletRealmLifecycleInput::PeriodicTick { now_millis, facets }
            | WalletRealmLifecycleInput::ActionPreflight { now_millis, facets } => {
                self.facets = *facets;
                self.now_millis = *now_millis;
            }
            WalletRealmLifecycleInput::Backgrounded { now_millis } => {
                self.now_millis = *now_millis;
            }
            WalletRealmLifecycleInput::ReconciliationFinished {
                identity,
                now_millis,
                facets,
                ..
            } if self.identity.as_ref() == Some(identity) => {
                self.facets = *facets;
                self.now_millis = *now_millis;
            }
            WalletRealmLifecycleInput::ReconciliationFinished { .. } => {}
        }
    }

    fn facets_for(
        &self,
        identity: &WalletRealmLifecycleIdentity,
    ) -> WalletRealmReconciliationState {
        if self.identity.as_ref() == Some(identity) {
            self.facets
        } else {
            missing_facets()
        }
    }

    fn now_for(&self, identity: &WalletRealmLifecycleIdentity) -> u64 {
        if self.identity.as_ref() == Some(identity) {
            self.now_millis
        } else {
            0
        }
    }
}

pub struct WalletRealmLifecycleService {
    policy: Mutex<WalletRealmLifecyclePolicy>,
    config: WalletRealmLifecyclePolicyConfig,
    clock: Arc<dyn ClockPort>,
    sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
    checkpoint: Mutex<WalletRealmLifecycleCheckpoint>,
}

impl WalletRealmLifecycleService {
    #[must_use]
    pub fn new(
        clock: Arc<dyn ClockPort>,
        sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
        facets: WalletRealmReconciliationState,
    ) -> Self {
        Self::with_config(
            clock,
            sync,
            facets,
            WalletRealmLifecyclePolicyConfig::default(),
        )
    }

    #[must_use]
    pub fn with_config(
        clock: Arc<dyn ClockPort>,
        sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
        facets: WalletRealmReconciliationState,
        config: WalletRealmLifecyclePolicyConfig,
    ) -> Self {
        Self {
            policy: Mutex::new(WalletRealmLifecyclePolicy::default()),
            config,
            clock,
            sync,
            checkpoint: Mutex::new(WalletRealmLifecycleCheckpoint::new(facets)),
        }
    }

    fn admit(
        &self,
        input: WalletRealmLifecycleInput,
    ) -> Result<WalletRealmLifecycleDecision, WalletRealmLifecycleError> {
        self.checkpoint
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?
            .observe(&input);
        Ok(self
            .policy
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?
            .reduce(self.config, input))
    }

    fn settle(
        &self,
        request: &WalletRealmLifecycleRequest,
        facets: WalletRealmReconciliationState,
        succeeded: bool,
    ) -> Result<
        (
            WalletRealmLifecycleDecision,
            Option<WalletRealmLifecycleError>,
            bool,
        ),
        WalletRealmLifecycleError,
    > {
        let (fallback_now, current) = {
            let checkpoint = self
                .checkpoint
                .lock()
                .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
            (
                checkpoint.now_for(&request.identity),
                checkpoint.identity.as_ref() == Some(&request.identity),
            )
        };
        let (now_millis, clock_error) = match self.clock.now() {
            Ok(now) => (now.value(), None),
            Err(error) => (fallback_now, Some(WalletRealmLifecycleError::Clock(error))),
        };

        if current {
            let mut checkpoint = self
                .checkpoint
                .lock()
                .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
            checkpoint.facets = facets;
            checkpoint.now_millis = now_millis;
        }

        let decision = self
            .policy
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?
            .reduce(
                self.config,
                WalletRealmLifecycleInput::ReconciliationFinished {
                    identity: request.identity.clone(),
                    sequence: request.sequence,
                    now_millis,
                    facets,
                    succeeded,
                },
            );
        Ok((decision, clock_error, current))
    }

    fn fallback_facets(
        &self,
        identity: &WalletRealmLifecycleIdentity,
    ) -> Result<WalletRealmReconciliationState, WalletRealmLifecycleError> {
        Ok(self
            .checkpoint
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?
            .facets_for(identity))
    }
}

impl ReconcileWalletRealmLifecycleUseCase for WalletRealmLifecycleService {
    fn execute(&self, input: WalletRealmLifecycleInput) -> WalletRealmLifecycleFuture<'_> {
        Box::pin(async move {
            let first_decision = self.admit(input)?;
            let Some(mut request) = admitted_request(&first_decision) else {
                return Ok(WalletRealmLifecycleResult {
                    decision: first_decision,
                    projection: None,
                });
            };

            let mut projection = None;
            let mut first_error = None;
            for drain_index in 0..MAX_DRAINED_REQUESTS {
                let outcome = self
                    .sync
                    .execute(
                        SelectedWalletRealmSyncCommand {
                            profile_id: request.identity.profile.as_str().to_owned(),
                        },
                        request.trigger,
                    )
                    .await;
                let (facets, succeeded, candidate_projection, sync_error) = match outcome {
                    Ok(result)
                        if result.projection.identity.profile == request.identity.profile
                            && result.projection.identity.realm == request.identity.realm =>
                    {
                        (result.facets, true, Some(result.projection), None)
                    }
                    Ok(_) => (
                        self.fallback_facets(&request.identity)?,
                        false,
                        None,
                        Some(WalletRealmLifecycleError::Sync(
                            SelectedWalletRealmSyncError::ObservationSuperseded,
                        )),
                    ),
                    Err(error) => (
                        self.fallback_facets(&request.identity)?,
                        false,
                        None,
                        Some(WalletRealmLifecycleError::Sync(error)),
                    ),
                };
                let (completion, clock_error, current) =
                    self.settle(&request, facets, succeeded)?;

                if current {
                    if let Some(candidate) = candidate_projection {
                        projection = Some(candidate);
                    }
                    if first_error.is_none() {
                        first_error = sync_error.or(clock_error);
                    }
                }

                let Some(next) = admitted_request(&completion) else {
                    break;
                };
                if drain_index + 1 == MAX_DRAINED_REQUESTS {
                    let facets = self.fallback_facets(&next.identity)?;
                    let _ = self.settle(&next, facets, false)?;
                    return Err(WalletRealmLifecycleError::DrainLimit);
                }
                request = next;
            }

            if let Some(error) = first_error {
                return Err(error);
            }
            Ok(WalletRealmLifecycleResult {
                decision: first_decision,
                projection,
            })
        })
    }
}

fn admitted_request(
    decision: &WalletRealmLifecycleDecision,
) -> Option<WalletRealmLifecycleRequest> {
    match decision {
        WalletRealmLifecycleDecision::Request(request)
        | WalletRealmLifecycleDecision::Superseded { request, .. } => Some(request.clone()),
        WalletRealmLifecycleDecision::Retained(_) | WalletRealmLifecycleDecision::Ignored => None,
    }
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
    use crate::{
        SelectedWalletRealmActionReadiness, SelectedWalletRealmIdentity,
        SelectedWalletRealmObservation, SelectedWalletRealmReconciliation,
        SelectedWalletRealmReconciliationFuture, SelectedWalletRealmSyncView,
        WalletRealmFamilyView, WalletRealmReconciliationTrigger,
    };
    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{ChainNetworkId, WalletProfileId};
    use std::{
        collections::VecDeque,
        future::{Future, poll_fn},
        sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        },
        task::{Context, Poll, Waker},
    };

    #[derive(Clone)]
    struct FakeClock {
        values: Arc<Mutex<VecDeque<Result<UnixTimestampMillis, PlatformError>>>>,
    }

    impl FakeClock {
        fn new(values: impl IntoIterator<Item = Result<u64, PlatformError>>) -> Self {
            Self {
                values: Arc::new(Mutex::new(
                    values
                        .into_iter()
                        .map(|value| value.map(UnixTimestampMillis::new))
                        .collect(),
                )),
            }
        }
    }

    impl ClockPort for FakeClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            self.values
                .lock()
                .expect("clock queue")
                .pop_front()
                .unwrap_or(Ok(UnixTimestampMillis::new(0)))
        }
    }

    enum FakeOutcome {
        Immediate(Result<SelectedWalletRealmReconciliation, SelectedWalletRealmSyncError>),
        Gated {
            released: Arc<AtomicBool>,
            result: Result<SelectedWalletRealmReconciliation, SelectedWalletRealmSyncError>,
        },
    }

    struct FakeReconciler {
        outcomes: Mutex<VecDeque<FakeOutcome>>,
        calls: Mutex<Vec<(String, WalletRealmReconciliationTrigger)>>,
    }

    impl FakeReconciler {
        fn new(outcomes: impl IntoIterator<Item = FakeOutcome>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes.into_iter().collect()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, WalletRealmReconciliationTrigger)> {
            self.calls.lock().expect("call log").clone()
        }
    }

    impl ReconcileSelectedWalletRealmUseCase for FakeReconciler {
        fn execute(
            &self,
            command: SelectedWalletRealmSyncCommand,
            trigger: WalletRealmReconciliationTrigger,
        ) -> SelectedWalletRealmReconciliationFuture<'_> {
            self.calls
                .lock()
                .expect("call log")
                .push((command.profile_id, trigger));
            let outcome = self
                .outcomes
                .lock()
                .expect("outcomes")
                .pop_front()
                .expect("configured outcome");
            Box::pin(async move {
                match outcome {
                    FakeOutcome::Immediate(result) => result,
                    FakeOutcome::Gated { released, result } => {
                        poll_fn(move |_| {
                            if released.load(Ordering::SeqCst) {
                                Poll::Ready(result.clone())
                            } else {
                                Poll::Pending
                            }
                        })
                        .await
                    }
                }
            })
        }
    }

    fn identity(profile: &str, realm: &str) -> WalletRealmLifecycleIdentity {
        WalletRealmLifecycleIdentity {
            profile: WalletProfileId::parse(profile).expect("profile"),
            realm: ChainNetworkId::parse(realm).expect("realm"),
        }
    }

    fn facets(state: WalletRealmFacetState) -> WalletRealmReconciliationState {
        WalletRealmReconciliationState {
            account: state,
            dust: state,
            shielded: state,
        }
    }

    fn reconciliation(
        identity: &WalletRealmLifecycleIdentity,
        revision: u64,
        state: WalletRealmFacetState,
    ) -> SelectedWalletRealmReconciliation {
        SelectedWalletRealmReconciliation {
            projection: SelectedWalletRealmProjection {
                identity: SelectedWalletRealmIdentity {
                    profile: identity.profile.clone(),
                    realm: identity.realm.clone(),
                },
                revision,
                fresh: state == WalletRealmFacetState::Current,
                consistent: true,
                actionable: SelectedWalletRealmActionReadiness::Ready,
                observation: SelectedWalletRealmObservation::Settled,
                view: SelectedWalletRealmSyncView {
                    account: WalletRealmFamilyView::NotFound,
                    dust: WalletRealmFamilyView::NotFound,
                    shielded: WalletRealmFamilyView::NotFound,
                },
            },
            facets: facets(state),
        }
    }

    fn initialized(
        identity: WalletRealmLifecycleIdentity,
        now_millis: u64,
        state: WalletRealmFacetState,
    ) -> WalletRealmLifecycleInput {
        WalletRealmLifecycleInput::Initialized {
            identity,
            now_millis,
            facets: facets(state),
        }
    }

    fn resolve<T>(future: impl Future<Output = T>) -> T {
        let mut future = Box::pin(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("fixture future must resolve immediately"),
        }
    }

    #[test]
    fn initial_trigger_is_preserved_and_ignored_signal_performs_no_io() {
        let realm = identity("profile_one", "standalone");
        let reconciler = Arc::new(FakeReconciler::new([FakeOutcome::Immediate(Ok(
            reconciliation(&realm, 1, WalletRealmFacetState::Current),
        ))]));
        let service = WalletRealmLifecycleService::new(
            Arc::new(FakeClock::new([Ok(10)])),
            reconciler.clone(),
            missing_facets(),
        );

        let result =
            resolve(service.execute(initialized(realm, 1, WalletRealmFacetState::Missing)))
                .expect("initial reconciliation");
        assert!(matches!(
            result.decision,
            WalletRealmLifecycleDecision::Request(_)
        ));
        let ignored =
            resolve(service.execute(WalletRealmLifecycleInput::Backgrounded { now_millis: 11 }))
                .expect("background signal");
        assert_eq!(ignored.decision, WalletRealmLifecycleDecision::Ignored);
        assert_eq!(
            reconciler.calls(),
            vec![(
                "profile_one".to_owned(),
                WalletRealmReconciliationTrigger::Initial,
            )]
        );
    }

    #[test]
    fn retained_request_is_drained_once_in_order() {
        let realm = identity("profile_one", "standalone");
        let released = Arc::new(AtomicBool::new(false));
        let reconciler = Arc::new(FakeReconciler::new([
            FakeOutcome::Gated {
                released: released.clone(),
                result: Ok(reconciliation(&realm, 1, WalletRealmFacetState::Stale)),
            },
            FakeOutcome::Immediate(Ok(reconciliation(
                &realm,
                2,
                WalletRealmFacetState::Current,
            ))),
        ]));
        let service = WalletRealmLifecycleService::new(
            Arc::new(FakeClock::new([Ok(10), Ok(20)])),
            reconciler.clone(),
            missing_facets(),
        );
        let mut first = service.execute(initialized(realm, 1, WalletRealmFacetState::Missing));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));

        let retained = resolve(service.execute(WalletRealmLifecycleInput::ActionPreflight {
            now_millis: 2,
            facets: facets(WalletRealmFacetState::Stale),
        }))
        .expect("retained signal");
        assert!(matches!(
            retained.decision,
            WalletRealmLifecycleDecision::Retained(_)
        ));
        released.store(true, Ordering::SeqCst);
        let Poll::Ready(Ok(completed)) = first.as_mut().poll(&mut context) else {
            panic!("drain must complete");
        };
        assert_eq!(completed.projection.expect("latest projection").revision, 2);
        assert_eq!(
            reconciler.calls(),
            vec![
                (
                    "profile_one".to_owned(),
                    WalletRealmReconciliationTrigger::Initial,
                ),
                (
                    "profile_one".to_owned(),
                    WalletRealmReconciliationTrigger::ActionPreflight,
                ),
            ]
        );
    }

    #[test]
    fn clock_failure_settles_request_and_allows_another_action() {
        let realm = identity("profile_one", "standalone");
        let reconciler = Arc::new(FakeReconciler::new([
            FakeOutcome::Immediate(Ok(reconciliation(
                &realm,
                1,
                WalletRealmFacetState::Current,
            ))),
            FakeOutcome::Immediate(Ok(reconciliation(
                &realm,
                2,
                WalletRealmFacetState::Current,
            ))),
        ]));
        let service = WalletRealmLifecycleService::new(
            Arc::new(FakeClock::new([
                Err(PlatformError::ClockUnavailable),
                Ok(20),
            ])),
            reconciler.clone(),
            missing_facets(),
        );
        assert_eq!(
            resolve(service.execute(initialized(realm, 7, WalletRealmFacetState::Missing,))),
            Err(WalletRealmLifecycleError::Clock(
                PlatformError::ClockUnavailable
            ))
        );
        resolve(service.execute(WalletRealmLifecycleInput::ActionPreflight {
            now_millis: 8,
            facets: facets(WalletRealmFacetState::Current),
        }))
        .expect("request was settled despite clock failure");
        assert_eq!(reconciler.calls().len(), 2);
    }

    #[test]
    fn sync_failure_clears_in_flight_and_respects_retry_backoff() {
        let realm = identity("profile_one", "standalone");
        let reconciler = Arc::new(FakeReconciler::new([
            FakeOutcome::Immediate(Err(SelectedWalletRealmSyncError::Unavailable)),
            FakeOutcome::Immediate(Ok(reconciliation(
                &realm,
                2,
                WalletRealmFacetState::Current,
            ))),
        ]));
        let config =
            WalletRealmLifecyclePolicyConfig::new(1, 1, 100, 1_000, 5, 0).expect("policy config");
        let service = WalletRealmLifecycleService::with_config(
            Arc::new(FakeClock::new([Ok(10), Ok(200)])),
            reconciler.clone(),
            missing_facets(),
            config,
        );
        assert_eq!(
            resolve(service.execute(initialized(realm, 0, WalletRealmFacetState::Missing,))),
            Err(WalletRealmLifecycleError::Sync(
                SelectedWalletRealmSyncError::Unavailable
            ))
        );
        let early = resolve(service.execute(WalletRealmLifecycleInput::PeriodicTick {
            now_millis: 50,
            facets: facets(WalletRealmFacetState::Stale),
        }))
        .expect("early tick");
        assert_eq!(early.decision, WalletRealmLifecycleDecision::Ignored);
        resolve(service.execute(WalletRealmLifecycleInput::PeriodicTick {
            now_millis: 120,
            facets: facets(WalletRealmFacetState::Stale),
        }))
        .expect("retry tick");
        assert_eq!(reconciler.calls().len(), 2);
    }

    #[test]
    fn every_authoritative_facet_state_is_retained_exactly() {
        let realm = identity("profile_one", "standalone");
        for state in [
            WalletRealmFacetState::Current,
            WalletRealmFacetState::Stale,
            WalletRealmFacetState::Missing,
            WalletRealmFacetState::Updating,
            WalletRealmFacetState::Blocked,
            WalletRealmFacetState::Unsupported,
        ] {
            let reconciler = Arc::new(FakeReconciler::new([FakeOutcome::Immediate(Ok(
                reconciliation(&realm, 1, state),
            ))]));
            let service = WalletRealmLifecycleService::new(
                Arc::new(FakeClock::new([Ok(10)])),
                reconciler,
                missing_facets(),
            );
            resolve(service.execute(initialized(
                realm.clone(),
                1,
                WalletRealmFacetState::Missing,
            )))
            .expect("reconciliation");
            assert_eq!(
                service.checkpoint.lock().expect("checkpoint").facets,
                facets(state)
            );
        }
    }

    #[test]
    fn mismatched_projection_is_rejected_as_superseded() {
        let requested = identity("profile_one", "standalone");
        let other = identity("profile_one", "preprod");
        let reconciler = Arc::new(FakeReconciler::new([FakeOutcome::Immediate(Ok(
            reconciliation(&other, 1, WalletRealmFacetState::Current),
        ))]));
        let service = WalletRealmLifecycleService::new(
            Arc::new(FakeClock::new([Ok(10)])),
            reconciler,
            missing_facets(),
        );
        assert_eq!(
            resolve(service.execute(initialized(requested, 1, WalletRealmFacetState::Missing,))),
            Err(WalletRealmLifecycleError::Sync(
                SelectedWalletRealmSyncError::ObservationSuperseded
            ))
        );
    }

    #[test]
    fn authoritative_facets_are_checkpointed_without_cross_realm_leakage() {
        let first = identity("profile_one", "standalone");
        let second = identity("profile_two", "preprod");
        let released = Arc::new(AtomicBool::new(false));
        let reconciler = Arc::new(FakeReconciler::new([
            FakeOutcome::Gated {
                released: released.clone(),
                result: Ok(reconciliation(&first, 1, WalletRealmFacetState::Blocked)),
            },
            FakeOutcome::Immediate(Ok(reconciliation(
                &second,
                1,
                WalletRealmFacetState::Current,
            ))),
        ]));
        let service = WalletRealmLifecycleService::new(
            Arc::new(FakeClock::new([Ok(20), Ok(30)])),
            reconciler,
            missing_facets(),
        );
        let mut stale = service.execute(initialized(first, 1, WalletRealmFacetState::Missing));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(stale.as_mut().poll(&mut context), Poll::Pending));

        let current = resolve(service.execute(WalletRealmLifecycleInput::RealmSelected {
            identity: second.clone(),
            now_millis: 2,
            facets: facets(WalletRealmFacetState::Missing),
        }))
        .expect("new realm");
        assert_eq!(
            current.projection.expect("new projection").identity.realm,
            second.realm
        );
        released.store(true, Ordering::SeqCst);
        let Poll::Ready(Ok(stale_result)) = stale.as_mut().poll(&mut context) else {
            panic!("stale work settles");
        };
        assert!(stale_result.projection.is_none());
        let checkpoint = service.checkpoint.lock().expect("checkpoint").clone();
        assert_eq!(checkpoint.identity, Some(second));
        assert_eq!(checkpoint.facets, facets(WalletRealmFacetState::Current));
    }
}
