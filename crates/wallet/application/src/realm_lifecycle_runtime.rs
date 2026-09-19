// SPDX-License-Identifier: Apache-2.0

//! Stateful execution boundary for selected-realm lifecycle reconciliation.

use crate::{
    DEFAULT_WALLET_REALM_LIFECYCLE_REQUEST_TIMEOUT_MILLIS, ReconcileSelectedWalletRealmUseCase,
    SelectedWalletRealmProjection, SelectedWalletRealmSyncCommand, SelectedWalletRealmSyncError,
    WalletActionWatch, WalletActionWatchHandle, WalletActionWatchObservation,
    WalletActionWatchProjection, WalletActionWatchRecovery, WalletActionWatchRuntime,
    WalletRealmFacetState, WalletRealmLifecycleDecision, WalletRealmLifecycleIdentity,
    WalletRealmLifecycleInput, WalletRealmLifecyclePolicy, WalletRealmLifecyclePolicyConfig,
    WalletRealmLifecycleRequest, WalletRealmReconciliationState,
};
use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

const MAX_DRAINED_REQUESTS: usize = 32;
pub const WALLET_REALM_LIFECYCLE_SNAPSHOT_VERSION: u8 = 1;

pub type WalletRealmLifecycleFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<WalletRealmLifecycleResult, WalletRealmLifecycleError>>
            + Send
            + 'a,
    >,
>;

pub struct WalletRealmLifecycleExecution<'a> {
    pub handle: WalletRealmLifecycleExecutionHandle,
    pub future: WalletRealmLifecycleFuture<'a>,
}

#[derive(Clone)]
pub struct WalletRealmLifecycleExecutionHandle {
    request: Arc<Mutex<Option<WalletRealmLifecycleRequest>>>,
}

impl WalletRealmLifecycleExecutionHandle {
    pub fn request(
        &self,
    ) -> Result<Option<WalletRealmLifecycleRequest>, WalletRealmLifecycleError> {
        self.request
            .lock()
            .map(|request| request.clone())
            .map_err(|_| WalletRealmLifecycleError::Poisoned)
    }

    fn replace(
        &self,
        request: Option<WalletRealmLifecycleRequest>,
    ) -> Result<(), WalletRealmLifecycleError> {
        *self
            .request
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)? = request;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletRealmLifecycleError {
    Sync(SelectedWalletRealmSyncError),
    DrainLimit,
    InvalidSnapshot,
    Poisoned,
}

impl fmt::Display for WalletRealmLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sync(error) => error.fmt(formatter),
            Self::DrainLimit => formatter.write_str("wallet realm lifecycle drain limit reached"),
            Self::InvalidSnapshot => {
                formatter.write_str("wallet realm lifecycle checkpoint is invalid")
            }
            Self::Poisoned => formatter.write_str("wallet realm lifecycle state is unavailable"),
        }
    }
}

impl Error for WalletRealmLifecycleError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRealmLifecycleResult {
    pub decision: WalletRealmLifecycleDecision,
    pub projection: Option<SelectedWalletRealmProjection>,
    pub settlement: Option<WalletRealmLifecycleSettlement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletRealmLifecycleSettlement {
    TimedOut(WalletRealmLifecycleRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRealmLifecycleStatus {
    pub identity: Option<WalletRealmLifecycleIdentity>,
    pub facets: WalletRealmReconciliationState,
    pub observed_at_millis: u64,
    pub next_wakeup_millis: Option<u64>,
    pub in_flight: Option<WalletRealmLifecycleRequest>,
    pub in_flight_deadline_millis: Option<u64>,
    pub request_timeout_millis: u64,
    /// Increments at every accepted deactivation boundary. Adapters use it to
    /// reject callbacks from a previous process/transport generation.
    pub lifecycle_generation: u64,
    /// Safe identities to reconcile after a lifecycle boundary. Submitted
    /// operations are never replayed from this record.
    pub action_recovery: Option<WalletActionWatchRecovery>,
}

/// Atomic, payload-free durable boundary for foreground/cold-start recovery.
///
/// Family-specific checkpoint stores remain authoritative for balances and
/// cursors. This record binds their coherent public state to the selected realm
/// and to any unresolved public action identity without retaining secrets,
/// transport handles, authorization, or transaction bodies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRealmLifecycleSnapshot {
    pub version: u8,
    pub identity: Option<WalletRealmLifecycleIdentity>,
    pub facets: WalletRealmReconciliationState,
    pub lifecycle_generation: u64,
    pub action_recovery: Option<WalletActionWatchRecovery>,
}

pub trait ReconcileWalletRealmLifecycleUseCase: Send + Sync {
    fn begin(
        &self,
        input: WalletRealmLifecycleInput,
    ) -> Result<WalletRealmLifecycleExecution<'_>, WalletRealmLifecycleError>;

    fn execute(&self, input: WalletRealmLifecycleInput) -> WalletRealmLifecycleFuture<'_>;

    fn status(&self) -> Result<WalletRealmLifecycleStatus, WalletRealmLifecycleError>;

    fn checkpoint(&self) -> Result<WalletRealmLifecycleSnapshot, WalletRealmLifecycleError>;

    fn restore(
        &self,
        snapshot: WalletRealmLifecycleSnapshot,
    ) -> Result<(), WalletRealmLifecycleError>;
}

/// Presentation-neutral control surface for the action watch owned by the
/// selected-realm lifecycle service.
///
/// Adapters retain the returned handle and must present it with every later
/// transition. This keeps delayed work from an older realm or admission from
/// settling the current watch.
pub trait ManageWalletActionWatchUseCase: Send + Sync {
    fn admit(
        &self,
        watch: WalletActionWatch,
        deadline_millis: u64,
        now_millis: u64,
    ) -> Result<Option<WalletActionWatchHandle>, WalletRealmLifecycleError>;

    fn observe(
        &self,
        handle: WalletActionWatchHandle,
        observation: WalletActionWatchObservation,
        now_millis: u64,
    ) -> Result<(), WalletRealmLifecycleError>;

    fn timeout(
        &self,
        handle: WalletActionWatchHandle,
        now_millis: u64,
    ) -> Result<(), WalletRealmLifecycleError>;

    fn cancel(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError>;

    fn offline(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError>;

    fn degraded(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError>;

    fn suspend(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError>;

    fn resume(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError>;

    fn projection(&self) -> Result<Option<WalletActionWatchProjection>, WalletRealmLifecycleError>;

    /// Settles a retained recovery record only when the exact public identity
    /// is observed. A mismatch is a no-op and never triggers resubmission.
    fn reconcile_recovery(
        &self,
        observation: WalletActionWatchObservation,
    ) -> Result<bool, WalletRealmLifecycleError>;

    /// Ends a bounded recovery attempt without asserting success. A later
    /// action may be admitted only after the caller has surfaced this result.
    fn expire_recovery(&self) -> Result<bool, WalletRealmLifecycleError>;
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
            WalletRealmLifecycleInput::ReconciliationObserved {
                identity,
                now_millis,
                facets,
            } if self.identity.as_ref() == Some(identity) => {
                self.facets = *facets;
                self.now_millis = *now_millis;
            }
            WalletRealmLifecycleInput::ReconciliationObserved { .. } => {}
            WalletRealmLifecycleInput::ReconciliationTimedOut {
                identity,
                now_millis,
                ..
            } if self.identity.as_ref() == Some(identity) => {
                self.now_millis = *now_millis;
            }
            WalletRealmLifecycleInput::ReconciliationTimedOut { .. } => {}
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
    request_timeout_millis: u64,
    sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WalletRealmLifecycleState {
    policy: WalletRealmLifecyclePolicy,
    checkpoint: WalletRealmLifecycleCheckpoint,
    action_watch: WalletActionWatchRuntime,
    action_recovery: Option<WalletActionWatchRecovery>,
    lifecycle_generation: u64,
    backgrounded: bool,
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
        Self::with_config_and_timeout(
            sync,
            facets,
            config,
            DEFAULT_WALLET_REALM_LIFECYCLE_REQUEST_TIMEOUT_MILLIS,
        )
    }

    #[must_use]
    pub fn with_config_and_timeout(
        sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
        facets: WalletRealmReconciliationState,
        config: WalletRealmLifecyclePolicyConfig,
        request_timeout_millis: u64,
    ) -> Self {
        Self {
            state: Mutex::new(WalletRealmLifecycleState {
                policy: WalletRealmLifecyclePolicy::default(),
                checkpoint: WalletRealmLifecycleCheckpoint::new(facets),
                action_watch: WalletActionWatchRuntime::default(),
                action_recovery: None,
                lifecycle_generation: 0,
                backgrounded: false,
            }),
            config,
            request_timeout_millis: request_timeout_millis.max(1),
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
        let previous_identity = state.checkpoint.identity.clone();
        state.checkpoint.observe(&input);
        if matches!(&input, WalletRealmLifecycleInput::Backgrounded { .. }) {
            if !state.backgrounded {
                state.backgrounded = true;
                if let Some(recovery) = state.action_watch.lifecycle_boundary() {
                    state.action_recovery = Some(recovery);
                }
                state.lifecycle_generation = state.action_watch.generation();
            }
        } else if matches!(
            &input,
            WalletRealmLifecycleInput::Initialized { .. }
                | WalletRealmLifecycleInput::RealmSelected { .. }
                | WalletRealmLifecycleInput::Foreground { .. }
        ) {
            state.backgrounded = false;
        }
        if let WalletRealmLifecycleInput::Initialized { identity, .. }
        | WalletRealmLifecycleInput::RealmSelected { identity, .. } = &input
        {
            if previous_identity
                .as_ref()
                .is_some_and(|previous| previous != identity)
            {
                state.action_recovery = None;
            }
            state.action_watch.select_realm(identity.clone());
            state.lifecycle_generation = state.action_watch.generation();
        }
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

    pub fn admit_action_watch(
        &self,
        watch: WalletActionWatch,
        deadline_millis: u64,
        now_millis: u64,
    ) -> Result<Option<WalletActionWatchHandle>, WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        if state.action_recovery.is_some() {
            return Ok(None);
        }
        Ok(state.action_watch.admit(watch, deadline_millis, now_millis))
    }

    pub fn observe_action_watch(
        &self,
        handle: WalletActionWatchHandle,
        observation: WalletActionWatchObservation,
        now_millis: u64,
    ) -> Result<(), WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        state.action_watch.observe(handle, observation, now_millis);
        Ok(())
    }

    pub fn timeout_action_watch(
        &self,
        handle: WalletActionWatchHandle,
        now_millis: u64,
    ) -> Result<(), WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        state.action_watch.timeout(handle, now_millis);
        Ok(())
    }

    pub fn cancel_action_watch(
        &self,
        handle: WalletActionWatchHandle,
    ) -> Result<(), WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        state.action_watch.cancel(handle);
        Ok(())
    }

    pub fn offline_action_watch(
        &self,
        handle: WalletActionWatchHandle,
    ) -> Result<(), WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        state.action_watch.offline(handle);
        Ok(())
    }

    pub fn degrade_action_watch(
        &self,
        handle: WalletActionWatchHandle,
    ) -> Result<(), WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        state.action_watch.degraded(handle);
        Ok(())
    }

    pub fn suspend_action_watch(
        &self,
        handle: WalletActionWatchHandle,
    ) -> Result<(), WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        state.action_watch.suspend(handle);
        Ok(())
    }

    pub fn resume_action_watch(
        &self,
        handle: WalletActionWatchHandle,
    ) -> Result<(), WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        state.action_watch.resume(handle);
        Ok(())
    }

    pub fn action_watch(
        &self,
    ) -> Result<Option<WalletActionWatchProjection>, WalletRealmLifecycleError> {
        let state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        Ok(state.action_watch.projection().cloned())
    }

    pub fn reconcile_action_recovery(
        &self,
        observation: WalletActionWatchObservation,
    ) -> Result<bool, WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        let Some(recovery) = state.action_recovery.as_ref() else {
            return Ok(false);
        };
        if !recovery.matches(&observation) {
            return Ok(false);
        }
        let kind = recovery.kind();
        state.action_recovery = None;
        state.action_watch.settle_recovery(kind);
        Ok(true)
    }

    pub fn expire_action_recovery(&self) -> Result<bool, WalletRealmLifecycleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        let Some(recovery) = state.action_recovery.take() else {
            return Ok(false);
        };
        state.action_watch.expire_recovery(recovery.kind());
        Ok(true)
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

    fn execute_admitted(
        &self,
        first_decision: WalletRealmLifecycleDecision,
        settlement: Option<WalletRealmLifecycleSettlement>,
        handle: WalletRealmLifecycleExecutionHandle,
    ) -> WalletRealmLifecycleFuture<'_> {
        Box::pin(async move {
            let Some(mut request) = admitted_request(&first_decision) else {
                return Ok(WalletRealmLifecycleResult {
                    decision: first_decision,
                    projection: None,
                    settlement,
                });
            };

            let mut projection = None;
            let mut latest_error = None;
            for drain_index in 0..MAX_DRAINED_REQUESTS {
                handle.replace(Some(request.clone()))?;
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
                    // A retained request represents newer intent than the
                    // request that exposed it. Surface the latest settlement
                    // so callers can observe completion-time freshness after
                    // a retained retry recovers from an earlier failure.
                    latest_error = sync_error;
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
            handle.replace(None)?;

            if let Some(error) = latest_error {
                return Err(error);
            }
            Ok(WalletRealmLifecycleResult {
                decision: first_decision,
                projection,
                settlement,
            })
        })
    }
}

impl ManageWalletActionWatchUseCase for WalletRealmLifecycleService {
    fn admit(
        &self,
        watch: WalletActionWatch,
        deadline_millis: u64,
        now_millis: u64,
    ) -> Result<Option<WalletActionWatchHandle>, WalletRealmLifecycleError> {
        self.admit_action_watch(watch, deadline_millis, now_millis)
    }

    fn observe(
        &self,
        handle: WalletActionWatchHandle,
        observation: WalletActionWatchObservation,
        now_millis: u64,
    ) -> Result<(), WalletRealmLifecycleError> {
        self.observe_action_watch(handle, observation, now_millis)
    }

    fn timeout(
        &self,
        handle: WalletActionWatchHandle,
        now_millis: u64,
    ) -> Result<(), WalletRealmLifecycleError> {
        self.timeout_action_watch(handle, now_millis)
    }

    fn cancel(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError> {
        self.cancel_action_watch(handle)
    }

    fn offline(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError> {
        self.offline_action_watch(handle)
    }

    fn degraded(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError> {
        self.degrade_action_watch(handle)
    }

    fn suspend(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError> {
        self.suspend_action_watch(handle)
    }

    fn resume(&self, handle: WalletActionWatchHandle) -> Result<(), WalletRealmLifecycleError> {
        self.resume_action_watch(handle)
    }

    fn projection(&self) -> Result<Option<WalletActionWatchProjection>, WalletRealmLifecycleError> {
        self.action_watch()
    }

    fn reconcile_recovery(
        &self,
        observation: WalletActionWatchObservation,
    ) -> Result<bool, WalletRealmLifecycleError> {
        self.reconcile_action_recovery(observation)
    }

    fn expire_recovery(&self) -> Result<bool, WalletRealmLifecycleError> {
        self.expire_action_recovery()
    }
}

impl ReconcileWalletRealmLifecycleUseCase for WalletRealmLifecycleService {
    fn begin(
        &self,
        input: WalletRealmLifecycleInput,
    ) -> Result<WalletRealmLifecycleExecution<'_>, WalletRealmLifecycleError> {
        let settlement = match &input {
            WalletRealmLifecycleInput::ReconciliationTimedOut {
                identity, sequence, ..
            } => self
                .state
                .lock()
                .map_err(|_| WalletRealmLifecycleError::Poisoned)?
                .policy
                .in_flight()
                .filter(|request| request.identity == *identity && request.sequence == *sequence)
                .cloned()
                .map(WalletRealmLifecycleSettlement::TimedOut),
            _ => None,
        };
        let first_decision = self.admit(input)?;
        let handle = WalletRealmLifecycleExecutionHandle {
            request: Arc::new(Mutex::new(admitted_request(&first_decision))),
        };
        Ok(WalletRealmLifecycleExecution {
            handle: handle.clone(),
            future: self.execute_admitted(first_decision, settlement, handle),
        })
    }

    fn execute(&self, input: WalletRealmLifecycleInput) -> WalletRealmLifecycleFuture<'_> {
        match self.begin(input) {
            Ok(execution) => execution.future,
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }

    fn status(&self) -> Result<WalletRealmLifecycleStatus, WalletRealmLifecycleError> {
        let state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        Ok(WalletRealmLifecycleStatus {
            identity: state.checkpoint.identity.clone(),
            facets: state.checkpoint.facets,
            observed_at_millis: state.checkpoint.now_millis,
            next_wakeup_millis: state.policy.next_wakeup_millis(self.config),
            in_flight: state.policy.in_flight().cloned(),
            in_flight_deadline_millis: state
                .policy
                .in_flight_deadline_millis(self.request_timeout_millis),
            request_timeout_millis: self.request_timeout_millis,
            lifecycle_generation: state.lifecycle_generation,
            action_recovery: state.action_recovery.clone(),
        })
    }

    fn checkpoint(&self) -> Result<WalletRealmLifecycleSnapshot, WalletRealmLifecycleError> {
        let state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        Ok(WalletRealmLifecycleSnapshot {
            version: WALLET_REALM_LIFECYCLE_SNAPSHOT_VERSION,
            identity: state.checkpoint.identity.clone(),
            facets: durable_facets(state.checkpoint.facets),
            lifecycle_generation: state.lifecycle_generation,
            action_recovery: state.action_recovery.clone(),
        })
    }

    fn restore(
        &self,
        snapshot: WalletRealmLifecycleSnapshot,
    ) -> Result<(), WalletRealmLifecycleError> {
        if snapshot.version != WALLET_REALM_LIFECYCLE_SNAPSHOT_VERSION
            || (snapshot.identity.is_none() && snapshot.action_recovery.is_some())
        {
            return Err(WalletRealmLifecycleError::InvalidSnapshot);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
        state.policy = WalletRealmLifecyclePolicy::default();
        state.checkpoint = WalletRealmLifecycleCheckpoint {
            identity: snapshot.identity.clone(),
            facets: durable_facets(snapshot.facets),
            // Monotonic time belongs to one process and is never restored.
            now_millis: 0,
        };
        state.action_watch = WalletActionWatchRuntime::default();
        state.action_recovery = snapshot.action_recovery;
        state.lifecycle_generation = snapshot.lifecycle_generation.saturating_add(1);
        state.backgrounded = false;
        let lifecycle_generation = state.lifecycle_generation;
        if let Some(identity) = snapshot.identity {
            state.policy.restore_active(identity.clone());
            if let Some(recovery) = state.action_recovery.clone() {
                state
                    .action_watch
                    .restore_recovery(identity, &recovery, lifecycle_generation);
            } else {
                state
                    .action_watch
                    .restore_realm(identity, lifecycle_generation);
            }
        }
        Ok(())
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

const fn durable_facets(
    mut facets: WalletRealmReconciliationState,
) -> WalletRealmReconciliationState {
    if matches!(facets.account, WalletRealmFacetState::Updating) {
        facets.account = WalletRealmFacetState::Stale;
    }
    if matches!(facets.dust, WalletRealmFacetState::Updating) {
        facets.dust = WalletRealmFacetState::Stale;
    }
    if matches!(facets.shielded, WalletRealmFacetState::Updating) {
        facets.shielded = WalletRealmFacetState::Stale;
    }
    facets
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
        WalletActionWatchRecovery, WalletActionWatchState, WalletRealmFamilyView,
        WalletRealmReconciliationTrigger,
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
    fn background_invalidates_action_workers_and_retains_only_exact_recovery_identity() {
        let realm = identity("profile_one", "standalone");
        let reconciler = Arc::new(FakeReconciler::new([FakeOutcome::Immediate(Ok(
            reconciliation(&realm, 1, WalletRealmFacetState::Current),
        ))]));
        let service = WalletRealmLifecycleService::new(reconciler, missing_facets());
        resolve(service.execute(initialized(realm, 1, WalletRealmFacetState::Missing)))
            .expect("initial reconciliation");
        let handle = service
            .admit_action_watch(
                WalletActionWatch::submitted_transaction("tx-a").expect("transaction"),
                10,
                1,
            )
            .expect("watch admission")
            .expect("watch handle");

        resolve(service.execute(WalletRealmLifecycleInput::Backgrounded { now_millis: 2 }))
            .expect("background");
        assert_eq!(
            service.status().expect("status").action_recovery,
            Some(WalletActionWatchRecovery::SubmittedTransaction {
                transaction: oxid_wallet_domain::ChainTransactionId::parse("tx-a")
                    .expect("transaction"),
            })
        );
        assert_eq!(
            service
                .action_watch()
                .expect("watch projection")
                .expect("projection")
                .state,
            WalletActionWatchState::OutcomeUnknown
        );
        service
            .observe_action_watch(
                handle,
                WalletActionWatchObservation::submitted_transaction("tx-a").expect("observation"),
                3,
            )
            .expect("stale observation is ignored");
        assert_eq!(
            service
                .action_watch()
                .expect("watch projection")
                .expect("projection")
                .state,
            WalletActionWatchState::OutcomeUnknown
        );

        let generation = service.status().expect("status").lifecycle_generation;
        resolve(service.execute(WalletRealmLifecycleInput::Backgrounded { now_millis: 3 }))
            .expect("duplicate background");
        let duplicate = service.status().expect("duplicate status");
        assert_eq!(duplicate.lifecycle_generation, generation);
        assert!(matches!(
            duplicate.action_recovery,
            Some(WalletActionWatchRecovery::SubmittedTransaction { .. })
        ));
        assert_eq!(
            service
                .admit_action_watch(
                    WalletActionWatch::submitted_transaction("tx-b").expect("transaction"),
                    20,
                    4,
                )
                .expect("recovery admission is deterministic"),
            None,
            "an unresolved submission must reconcile before another watch is admitted"
        );
        assert!(
            !service
                .reconcile_action_recovery(
                    WalletActionWatchObservation::submitted_transaction("tx-b")
                        .expect("observation")
                )
                .expect("mismatched recovery")
        );
        assert!(
            service
                .reconcile_action_recovery(
                    WalletActionWatchObservation::submitted_transaction("tx-a")
                        .expect("observation")
                )
                .expect("exact recovery")
        );
        assert_eq!(
            service.status().expect("settled status").action_recovery,
            None
        );
        assert_eq!(
            service
                .action_watch()
                .expect("watch projection")
                .expect("projection")
                .state,
            WalletActionWatchState::Confirmed
        );
    }

    #[test]
    fn cold_start_restores_one_complete_checkpoint_and_reconciles_in_a_new_generation() {
        let realm = identity("profile_one", "standalone");
        let initial = Arc::new(FakeReconciler::new([FakeOutcome::Immediate(Ok(
            reconciliation(&realm, 1, WalletRealmFacetState::Current),
        ))]));
        let service = WalletRealmLifecycleService::new(initial, missing_facets());
        resolve(service.execute(initialized(
            realm.clone(),
            1,
            WalletRealmFacetState::Missing,
        )))
        .expect("initial reconciliation");
        service
            .admit_action_watch(
                WalletActionWatch::submitted_transaction("tx-a").expect("transaction"),
                10,
                1,
            )
            .expect("watch admission")
            .expect("watch handle");
        resolve(service.execute(WalletRealmLifecycleInput::Backgrounded { now_millis: 2 }))
            .expect("background");
        let committed = service.checkpoint().expect("atomic checkpoint");

        // A candidate exists only in memory until the adapter atomically
        // commits it. Simulated process death therefore restores the complete
        // previous record, never a mixture of candidate and committed fields.
        let mut uncommitted_candidate = committed.clone();
        uncommitted_candidate.facets.dust = WalletRealmFacetState::Stale;
        uncommitted_candidate.lifecycle_generation = 99;

        let resumed = Arc::new(FakeReconciler::new([FakeOutcome::Immediate(Ok(
            reconciliation(&realm, 2, WalletRealmFacetState::Current),
        ))]));
        let restored = WalletRealmLifecycleService::new(resumed, missing_facets());
        restored
            .restore(committed.clone())
            .expect("restore checkpoint");
        let status = restored.status().expect("restored status");
        assert_eq!(status.identity, committed.identity);
        assert_eq!(status.facets, committed.facets);
        assert_eq!(status.action_recovery, committed.action_recovery);
        assert_eq!(
            status.lifecycle_generation,
            committed.lifecycle_generation.saturating_add(1)
        );
        assert_ne!(status.facets, uncommitted_candidate.facets);
        assert_ne!(
            status.lifecycle_generation,
            uncommitted_candidate.lifecycle_generation
        );
        assert_eq!(
            restored
                .action_watch()
                .expect("watch projection")
                .expect("recovery projection")
                .state,
            WalletActionWatchState::OutcomeUnknown
        );
        assert_eq!(
            restored
                .action_watch()
                .expect("watch projection")
                .expect("recovery projection")
                .realm_generation,
            status.lifecycle_generation
        );

        let result = resolve(restored.execute(WalletRealmLifecycleInput::Foreground {
            now_millis: 1,
            facets: status.facets,
        }))
        .expect("foreground reconciliation");
        assert!(matches!(
            result.decision,
            WalletRealmLifecycleDecision::Request(_)
        ));
    }

    #[test]
    fn durable_snapshot_rejects_unknown_versions_and_never_restores_updating_work() {
        let reconciler = Arc::new(FakeReconciler::new([]));
        let service = WalletRealmLifecycleService::new(
            reconciler,
            WalletRealmReconciliationState {
                account: WalletRealmFacetState::Updating,
                dust: WalletRealmFacetState::Updating,
                shielded: WalletRealmFacetState::Updating,
            },
        );
        let snapshot = service.checkpoint().expect("checkpoint");
        assert_eq!(
            snapshot.facets,
            WalletRealmReconciliationState {
                account: WalletRealmFacetState::Stale,
                dust: WalletRealmFacetState::Stale,
                shielded: WalletRealmFacetState::Stale,
            }
        );

        let mut unsupported = snapshot;
        unsupported.version = 2;
        assert_eq!(
            service.restore(unsupported),
            Err(WalletRealmLifecycleError::InvalidSnapshot)
        );

        let updating = WalletRealmLifecycleSnapshot {
            version: WALLET_REALM_LIFECYCLE_SNAPSHOT_VERSION,
            identity: None,
            facets: WalletRealmReconciliationState {
                account: WalletRealmFacetState::Updating,
                dust: WalletRealmFacetState::Updating,
                shielded: WalletRealmFacetState::Updating,
            },
            lifecycle_generation: 4,
            action_recovery: None,
        };
        service.restore(updating).expect("defensive restore");
        assert_eq!(
            service.status().expect("restored status").facets,
            WalletRealmReconciliationState {
                account: WalletRealmFacetState::Stale,
                dust: WalletRealmFacetState::Stale,
                shielded: WalletRealmFacetState::Stale,
            }
        );
    }

    #[test]
    fn bounded_recovery_expiry_releases_admission_without_claiming_success() {
        let realm = identity("profile_one", "standalone");
        let reconciler = Arc::new(FakeReconciler::new([FakeOutcome::Immediate(Ok(
            reconciliation(&realm, 1, WalletRealmFacetState::Current),
        ))]));
        let service = WalletRealmLifecycleService::new(reconciler, missing_facets());
        resolve(service.execute(initialized(realm, 1, WalletRealmFacetState::Missing)))
            .expect("initial reconciliation");
        service
            .admit_action_watch(
                WalletActionWatch::submitted_transaction("tx-a").expect("transaction"),
                10,
                1,
            )
            .expect("watch admission")
            .expect("watch handle");
        resolve(service.execute(WalletRealmLifecycleInput::Backgrounded { now_millis: 2 }))
            .expect("background");

        assert!(service.expire_action_recovery().expect("expire recovery"));
        assert!(!service.expire_action_recovery().expect("idempotent expiry"));
        assert_eq!(
            service
                .action_watch()
                .expect("watch projection")
                .expect("projection")
                .state,
            WalletActionWatchState::Expired
        );
        assert!(
            service
                .admit_action_watch(
                    WalletActionWatch::submitted_transaction("tx-b").expect("transaction"),
                    20,
                    3,
                )
                .expect("post-recovery admission")
                .is_some()
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
    fn retained_success_supersedes_an_earlier_sync_failure() {
        let realm = identity("profile_one", "standalone");
        let released = Arc::new(AtomicBool::new(false));
        let reconciler = Arc::new(FakeReconciler::new([
            FakeOutcome::Gated {
                released: released.clone(),
                result: Err(SelectedWalletRealmSyncError::Unavailable),
            },
            FakeOutcome::Immediate(Ok(reconciliation(
                &realm,
                2,
                WalletRealmFacetState::Current,
            ))),
        ]));
        let service = WalletRealmLifecycleService::new(reconciler, missing_facets());
        let mut first = service.execute(initialized(realm, 1, WalletRealmFacetState::Missing));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));

        let retained = resolve(service.execute(WalletRealmLifecycleInput::ActionPreflight {
            now_millis: 2,
            facets: facets(WalletRealmFacetState::Missing),
        }))
        .expect("retained signal");
        assert!(matches!(
            retained.decision,
            WalletRealmLifecycleDecision::Retained(_)
        ));

        released.store(true, Ordering::SeqCst);
        let Poll::Ready(Ok(completed)) = first.as_mut().poll(&mut context) else {
            panic!("retained success must become the surfaced outcome");
        };
        assert_eq!(completed.projection.expect("latest projection").revision, 2);
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
        let execution = service
            .begin(initialized(
                first.clone(),
                1,
                WalletRealmFacetState::Missing,
            ))
            .expect("tracked execution");
        let handle = execution.handle;
        let mut stale = execution.future;
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
        assert_eq!(
            handle
                .request()
                .expect("tracked request")
                .expect("admitted request")
                .identity,
            first
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

    #[test]
    fn status_exposes_schedule_and_timeout_retains_the_last_consistent_checkpoint() {
        let realm = identity("profile_one", "standalone");
        let released = Arc::new(AtomicBool::new(false));
        let reconciler = Arc::new(FakeReconciler::new([FakeOutcome::Gated {
            released: released.clone(),
            result: Ok(reconciliation(&realm, 1, WalletRealmFacetState::Current)),
        }]));
        let config =
            WalletRealmLifecyclePolicyConfig::new(1, 100, 10, 1_000, 5, 0).expect("config");
        let service = WalletRealmLifecycleService::with_config_and_timeout(
            reconciler,
            missing_facets(),
            config,
            5,
        );
        let mut admitted = service.execute(initialized(
            realm.clone(),
            10,
            WalletRealmFacetState::Missing,
        ));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(
            admitted.as_mut().poll(&mut context),
            Poll::Pending
        ));

        let active = service.status().expect("active status");
        let request = active.in_flight.clone().expect("admitted request");
        assert_eq!(active.identity, Some(realm.clone()));
        assert_eq!(active.facets, facets(WalletRealmFacetState::Missing));
        assert_eq!(active.in_flight_deadline_millis, Some(15));
        assert_eq!(active.next_wakeup_millis, None);

        let timeout = resolve(
            service.execute(WalletRealmLifecycleInput::ReconciliationTimedOut {
                identity: realm,
                sequence: request.sequence,
                now_millis: 15,
                facets: facets(WalletRealmFacetState::Current),
            }),
        )
        .expect("timeout settlement");
        assert_eq!(
            timeout.settlement,
            Some(WalletRealmLifecycleSettlement::TimedOut(request))
        );
        let settled = service.status().expect("settled status");
        assert_eq!(settled.facets, facets(WalletRealmFacetState::Missing));
        assert!(settled.in_flight.is_none());
        assert!(settled.in_flight_deadline_millis.is_none());
        assert_eq!(settled.next_wakeup_millis, Some(25));

        released.store(true, Ordering::SeqCst);
        let Poll::Ready(Ok(stale)) = admitted.as_mut().poll(&mut context) else {
            panic!("cancelled work must settle without publication");
        };
        assert!(stale.projection.is_none());
        assert_eq!(
            service.status().expect("final status").facets,
            facets(WalletRealmFacetState::Missing)
        );
    }

    #[test]
    fn successful_reconciliation_exposes_the_next_runtime_wakeup() {
        let realm = identity("profile_one", "standalone");
        let reconciler = Arc::new(FakeReconciler::new([FakeOutcome::Immediate(Ok(
            reconciliation(&realm, 1, WalletRealmFacetState::Current),
        ))]));
        let config =
            WalletRealmLifecyclePolicyConfig::new(1, 100, 10, 1_000, 5, 0).expect("config");
        let service =
            WalletRealmLifecycleService::with_config(reconciler, missing_facets(), config);
        resolve(service.execute(initialized(
            realm.clone(),
            10,
            WalletRealmFacetState::Missing,
        )))
        .expect("initial reconciliation");

        let status = service.status().expect("status");
        assert_eq!(status.next_wakeup_millis, Some(110));
        assert!(status.in_flight.is_none());
        assert!(status.in_flight_deadline_millis.is_none());

        resolve(
            service.execute(WalletRealmLifecycleInput::ReconciliationObserved {
                identity: realm,
                now_millis: 50,
                facets: facets(WalletRealmFacetState::Current),
            }),
        )
        .expect("completion observation");
        let completed = service.status().expect("completion status");
        assert_eq!(completed.observed_at_millis, 50);
        assert_eq!(completed.next_wakeup_millis, Some(150));
    }
}
