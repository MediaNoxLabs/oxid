// SPDX-License-Identifier: Apache-2.0

//! Stateful execution boundary for selected-realm lifecycle reconciliation.

use crate::{
    ReconcileSelectedWalletRealmUseCase, SelectedWalletRealmProjection,
    SelectedWalletRealmSyncCommand, SelectedWalletRealmSyncError, WalletRealmFacetState,
    WalletRealmLifecycleDecision, WalletRealmLifecycleIdentity, WalletRealmLifecycleInput,
    WalletRealmLifecyclePolicy, WalletRealmLifecyclePolicyConfig, WalletRealmLifecycleRequest,
    WalletRealmReconciliationState,
};
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
    Sync(SelectedWalletRealmSyncError),
    DrainLimit,
    Poisoned,
}

impl fmt::Display for WalletRealmLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
    state: Mutex<WalletRealmLifecycleState>,
    config: WalletRealmLifecyclePolicyConfig,
    sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WalletRealmLifecycleState {
    policy: WalletRealmLifecyclePolicy,
    checkpoint: WalletRealmLifecycleCheckpoint,
}

impl WalletRealmLifecycleService {
    #[must_use]
    pub fn new(
        sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
        facets: WalletRealmReconciliationState,
    ) -> Self {
        Self::with_config(sync, facets, WalletRealmLifecyclePolicyConfig::default())
    }

    #[must_use]
    pub fn with_config(
        sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
        facets: WalletRealmReconciliationState,
        config: WalletRealmLifecyclePolicyConfig,
    ) -> Self {
        Self {
            state: Mutex::new(WalletRealmLifecycleState {
                policy: WalletRealmLifecyclePolicy::default(),
                checkpoint: WalletRealmLifecycleCheckpoint::new(facets),
            }),
            config,
            sync,
        }
    }

    fn admit(
        &self,
        input: WalletRealmLifecycleInput,
    ) -> Result<WalletRealmLifecycleDecision, WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        state.checkpoint.observe(&input);
        Ok(state.policy.reduce(self.config, input))
    }

    fn settle(
        &self,
        request: &WalletRealmLifecycleRequest,
        facets: WalletRealmReconciliationState,
        succeeded: bool,
    ) -> Result<(WalletRealmLifecycleDecision, bool), WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        let current = state.policy.owns(request)
            && state.checkpoint.identity.as_ref() == Some(&request.identity);
        let now_millis = state.checkpoint.now_for(&request.identity);
        if current {
            state.checkpoint.facets = facets;
        }
        let decision = state.policy.reduce(
            self.config,
            WalletRealmLifecycleInput::ReconciliationFinished {
                identity: request.identity.clone(),
                sequence: request.sequence,
                now_millis,
                facets,
                succeeded,
            },
        );
        Ok((decision, current))
    }

    fn fallback_facets(
        &self,
        identity: &WalletRealmLifecycleIdentity,
    ) -> Result<WalletRealmReconciliationState, WalletRealmLifecycleError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?
            .checkpoint
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
                let (completion, current) = self.settle(&request, facets, succeeded)?;

                if current {
                    if let Some(candidate) = candidate_projection {
                        projection = Some(candidate);
                    }
                    if first_error.is_none() {
                        first_error = sync_error;
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
        let service = WalletRealmLifecycleService::new(reconciler.clone(), missing_facets());

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
        let service = WalletRealmLifecycleService::new(reconciler.clone(), missing_facets());
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
    fn completion_stays_in_the_input_monotonic_time_domain() {
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
        let config =
            WalletRealmLifecyclePolicyConfig::new(1, 100, 1, 1_000, 5, 0).expect("policy config");
        let service =
            WalletRealmLifecycleService::with_config(reconciler.clone(), missing_facets(), config);
        resolve(service.execute(initialized(realm, 10, WalletRealmFacetState::Missing)))
            .expect("initial request");
        let fresh = resolve(service.execute(WalletRealmLifecycleInput::PeriodicTick {
            now_millis: 109,
            facets: facets(WalletRealmFacetState::Current),
        }))
        .expect("fresh tick");
        assert_eq!(fresh.decision, WalletRealmLifecycleDecision::Ignored);
        resolve(service.execute(WalletRealmLifecycleInput::PeriodicTick {
            now_millis: 110,
            facets: facets(WalletRealmFacetState::Current),
        }))
        .expect("stale-age tick");
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
        let service =
            WalletRealmLifecycleService::with_config(reconciler.clone(), missing_facets(), config);
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
            let service = WalletRealmLifecycleService::new(reconciler, missing_facets());
            resolve(service.execute(initialized(
                realm.clone(),
                1,
                WalletRealmFacetState::Missing,
            )))
            .expect("reconciliation");
            assert_eq!(
                service
                    .state
                    .lock()
                    .expect("lifecycle state")
                    .checkpoint
                    .facets,
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
        let service = WalletRealmLifecycleService::new(reconciler, missing_facets());
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
        let service = WalletRealmLifecycleService::new(reconciler, missing_facets());
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
        let checkpoint = service
            .state
            .lock()
            .expect("lifecycle state")
            .checkpoint
            .clone();
        assert_eq!(checkpoint.identity, Some(second));
        assert_eq!(checkpoint.facets, facets(WalletRealmFacetState::Current));
    }
}
