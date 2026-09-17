// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::Cell,
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::{
    ChainNetworkId, WalletAccountSnapshot, WalletAccountSource, WalletDustSyncFailure,
    WalletDustSyncSnapshot, WalletDustSyncState, WalletProfileId, WalletShieldedSyncFailure,
    WalletShieldedSyncSnapshot, WalletShieldedSyncState, WalletSyncState,
};

use crate::{
    GetWalletOperationTimelineUseCase, WalletAccountPortError, WalletAccountReadPort,
    WalletAccountView, WalletDustSyncPort, WalletDustSyncPortError, WalletDustSyncView,
    WalletNetworkPort, WalletNetworkSelectionObserver, WalletOperationAttempt,
    WalletOperationCausationId, WalletOperationCorrelationId, WalletOperationDurationMillis,
    WalletOperationEvent, WalletOperationFailure, WalletOperationId, WalletOperationOutcome,
    WalletOperationResource, WalletOperationResourceIdentity, WalletOperationResourceMeasurement,
    WalletOperationResourceMeasurements, WalletOperationTimeline, WalletOperationTimelineError,
    WalletOperationTimelineSnapshot, WalletOperationTrigger, WalletRealmCoordinatorEffect,
    WalletRealmCoordinatorInput, WalletRealmCoordinatorState, WalletRealmEffectOutcome,
    WalletRealmFacetState, WalletRealmReconciliationCoordinator, WalletRealmReconciliationEffect,
    WalletRealmReconciliationState, WalletRealmReconciliationTrigger, WalletShieldedSyncPort,
    WalletShieldedSyncPortError, WalletShieldedSyncView, timeline_effect_outcome,
};

/// Profile-scoped command for reconciling the currently selected network realm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedWalletRealmSyncCommand {
    pub profile_id: String,
}

/// A typed family outcome. Absence and temporary failure never masquerade as a
/// zero balance, while a ready view retains the adapter's exact sync state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletRealmFamilyView<T> {
    Ready(T),
    Busy,
    NotFound,
    Unsupported,
    ProtectionNotInitialized,
    ProtectionLocked,
    Unavailable,
    InvalidData,
}

impl<T> WalletRealmFamilyView<T> {
    #[must_use]
    pub const fn state_name(&self) -> &'static str {
        match self {
            Self::Ready(_) => "ready",
            Self::Busy => "busy",
            Self::NotFound => "not_found",
            Self::Unsupported => "unsupported",
            Self::ProtectionNotInitialized => "protection_not_initialized",
            Self::ProtectionLocked => "protection_locked",
            Self::Unavailable => "unavailable",
            Self::InvalidData => "invalid_data",
        }
    }
}

/// One coherent public projection for the selected network realm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedWalletRealmSyncView {
    pub account: WalletRealmFamilyView<WalletAccountView>,
    pub dust: WalletRealmFamilyView<WalletDustSyncView>,
    pub shielded: WalletRealmFamilyView<WalletShieldedSyncView>,
}

/// Typed identity and monotonically increasing revision of a selected realm observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedWalletRealmIdentity {
    pub profile: WalletProfileId,
    pub realm: ChainNetworkId,
}

/// Whether an observation can be used for a new user action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedWalletRealmActionReadiness {
    Ready,
    Refreshing,
    Unavailable,
}

/// Presentation-neutral observation policy for a projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedWalletRealmObservation {
    Settled,
    PollAfter(Duration),
}

impl SelectedWalletRealmObservation {
    #[must_use]
    pub const fn poll_after(self) -> Option<Duration> {
        match self {
            Self::Settled => None,
            Self::PollAfter(duration) => Some(duration),
        }
    }
}

/// The application-owned query observation consumed by every selected-realm client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedWalletRealmProjection {
    pub identity: SelectedWalletRealmIdentity,
    pub revision: u64,
    pub fresh: bool,
    pub consistent: bool,
    pub actionable: SelectedWalletRealmActionReadiness,
    pub observation: SelectedWalletRealmObservation,
    pub view: SelectedWalletRealmSyncView,
}

/// Reconciliation output preserving the coordinator's typed facet state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedWalletRealmReconciliation {
    pub projection: SelectedWalletRealmProjection,
    pub facets: WalletRealmReconciliationState,
}

impl SelectedWalletRealmProjection {
    /// Rejects an observation belonging to another selected realm or an older revision.
    #[must_use]
    pub fn supersedes(&self, previous: &Self) -> bool {
        self.identity == previous.identity && self.revision >= previous.revision
    }
}

/// Validation or public-account failure for selected-realm reconciliation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectedWalletRealmSyncError {
    InvalidProfileIdentifier(OpaqueIdError),
    SelectedNetwork(WalletAccountPortError),
    SelectionChanged,
    ObservationSuperseded,
    Unavailable,
}

impl fmt::Display for SelectedWalletRealmSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
            Self::SelectedNetwork(error) => error.fmt(formatter),
            Self::SelectionChanged => {
                formatter.write_str("selected wallet realm changed during synchronization")
            }
            Self::ObservationSuperseded => {
                formatter.write_str("selected wallet realm observation was superseded")
            }
            Self::Unavailable => formatter.write_str("selected realm runtime is unavailable"),
        }
    }
}

impl Error for SelectedWalletRealmSyncError {}

pub type SelectedWalletRealmProjectionFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError>>
            + Send
            + 'a,
    >,
>;
pub type SelectedWalletRealmReconciliationFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<SelectedWalletRealmReconciliation, SelectedWalletRealmSyncError>>
            + Send
            + 'a,
    >,
>;

/// Starts one bounded public/DUST/shielded reconciliation for the selected realm.
pub trait SyncSelectedWalletRealmUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> SelectedWalletRealmProjectionFuture<'_>;
}

/// Executes a lifecycle-selected trigger and retains its authoritative facets.
pub trait ReconcileSelectedWalletRealmUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
        trigger: WalletRealmReconciliationTrigger,
    ) -> SelectedWalletRealmReconciliationFuture<'_>;
}

/// Reads the most recently published aggregate without starting I/O.
pub trait GetSelectedWalletRealmSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError>;
}

/// Cooperatively cancels the private family workers and returns their state.
pub trait CancelSelectedWalletRealmSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError>;
}

/// Application-owned persistent coordinator registry for selected wallet realms.
/// The owner is intentionally synchronous: adapters decide how and when to run
/// effects, while all lease and stale-completion decisions remain deterministic.
#[derive(Default)]
pub struct SelectedWalletRealmRuntime {
    entries: Vec<SelectedWalletRealmRuntimeEntry>,
    selections: Vec<SelectedWalletRealmSelection>,
    observations: Vec<SelectedWalletRealmObservationGeneration>,
    projections: Vec<SelectedWalletRealmPublishedProjection>,
}

struct SelectedWalletRealmRuntimeEntry {
    profile: WalletProfileId,
    realm: ChainNetworkId,
    state: WalletRealmCoordinatorState,
}

struct SelectedWalletRealmSelection {
    profile: WalletProfileId,
    realm: ChainNetworkId,
}

#[derive(Clone)]
struct SelectedWalletRealmPublishedProjection {
    profile: WalletProfileId,
    realm: ChainNetworkId,
    revision: u64,
    authority_generation: u64,
    reconciling: bool,
    view: SelectedWalletRealmSyncView,
}

struct SelectedWalletRealmObservationGeneration {
    profile: WalletProfileId,
    realm: ChainNetworkId,
    generation: u64,
    authority_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectedWalletRealmObservationToken {
    generation: u64,
    authority_generation: u64,
}

impl SelectedWalletRealmRuntime {
    pub fn reconcile(
        &mut self,
        profile: WalletProfileId,
        realm: ChainNetworkId,
        observed: WalletRealmReconciliationState,
        trigger: WalletRealmReconciliationTrigger,
    ) -> Vec<WalletRealmCoordinatorEffect> {
        let entry = if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.profile == profile && entry.realm == realm)
        {
            entry
        } else {
            self.entries.push(SelectedWalletRealmRuntimeEntry {
                profile,
                realm,
                state: WalletRealmCoordinatorState::new(observed),
            });
            self.entries.last_mut().expect("entry inserted")
        };
        let observed = WalletRealmReconciliationCoordinator::reduce(
            entry.state,
            WalletRealmCoordinatorInput::Observe(observed),
        );
        let transition = WalletRealmReconciliationCoordinator::reduce(
            *observed.state(),
            WalletRealmCoordinatorInput::Reconcile(trigger),
        );
        entry.state = *transition.state();
        transition.effects().to_vec()
    }

    pub fn facets(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Option<WalletRealmReconciliationState> {
        self.entries
            .iter()
            .find(|entry| &entry.profile == profile && &entry.realm == realm)
            .map(|entry| entry.state.facets())
    }

    pub fn retire_other_realms(
        &mut self,
        profile: &WalletProfileId,
        selected_realm: &ChainNetworkId,
    ) -> Vec<ChainNetworkId> {
        let Some(previous) = self.observe_selected_realm(profile, selected_realm) else {
            return Vec::new();
        };
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| &entry.profile == profile && entry.realm == previous)
        {
            entry.state = *WalletRealmReconciliationCoordinator::reduce(
                entry.state,
                WalletRealmCoordinatorInput::Cancel,
            )
            .state();
        }
        vec![previous]
    }

    fn observe_selected_realm(
        &mut self,
        profile: &WalletProfileId,
        selected_realm: &ChainNetworkId,
    ) -> Option<ChainNetworkId> {
        let previous = if let Some(selection) = self
            .selections
            .iter_mut()
            .find(|selection| &selection.profile == profile)
        {
            if &selection.realm == selected_realm {
                return None;
            }
            Some(std::mem::replace(
                &mut selection.realm,
                selected_realm.clone(),
            ))
        } else {
            self.selections.push(SelectedWalletRealmSelection {
                profile: profile.clone(),
                realm: selected_realm.clone(),
            });
            None
        };
        let previous = previous?;
        self.invalidate_projection(profile, &previous);
        self.invalidate_projection(profile, selected_realm);
        Some(previous)
    }

    fn invalidate_projection(&mut self, profile: &WalletProfileId, realm: &ChainNetworkId) {
        if let Some(projection) = self
            .projections
            .iter_mut()
            .find(|projection| &projection.profile == profile && &projection.realm == realm)
        {
            projection.revision = projection.revision.saturating_add(1);
        }
        self.advance_authority_generation(profile, realm);
    }

    fn advance_authority_generation(&mut self, profile: &WalletProfileId, realm: &ChainNetworkId) {
        if let Some(observation) = self
            .observations
            .iter_mut()
            .find(|observation| &observation.profile == profile && &observation.realm == realm)
        {
            observation.generation = observation.generation.saturating_add(1);
            observation.authority_generation = observation.authority_generation.saturating_add(1);
            return;
        }
        self.observations
            .push(SelectedWalletRealmObservationGeneration {
                profile: profile.clone(),
                realm: realm.clone(),
                generation: 1,
                authority_generation: 1,
            });
    }

    fn advance_observation_generation(
        &mut self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> SelectedWalletRealmObservationToken {
        if let Some(observation) = self
            .observations
            .iter_mut()
            .find(|observation| &observation.profile == profile && &observation.realm == realm)
        {
            observation.generation = observation.generation.saturating_add(1);
            return SelectedWalletRealmObservationToken {
                generation: observation.generation,
                authority_generation: observation.authority_generation,
            };
        }
        self.observations
            .push(SelectedWalletRealmObservationGeneration {
                profile: profile.clone(),
                realm: realm.clone(),
                generation: 1,
                authority_generation: 0,
            });
        SelectedWalletRealmObservationToken {
            generation: 1,
            authority_generation: 0,
        }
    }

    fn begin_observation(
        &mut self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Option<SelectedWalletRealmObservationToken> {
        match self
            .selections
            .iter()
            .find(|selection| &selection.profile == profile)
        {
            Some(selection) if &selection.realm != realm => None,
            Some(_) => Some(self.advance_observation_generation(profile, realm)),
            None => {
                self.selections.push(SelectedWalletRealmSelection {
                    profile: profile.clone(),
                    realm: realm.clone(),
                });
                Some(self.advance_observation_generation(profile, realm))
            }
        }
    }

    fn selection_matches(&self, profile: &WalletProfileId, realm: &ChainNetworkId) -> bool {
        self.selections
            .iter()
            .find(|selection| &selection.profile == profile)
            .is_some_and(|selection| &selection.realm == realm)
    }

    fn authority_matches(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
        token: SelectedWalletRealmObservationToken,
    ) -> bool {
        self.observations
            .iter()
            .find(|observation| &observation.profile == profile && &observation.realm == realm)
            .is_some_and(|observation| {
                observation.authority_generation == token.authority_generation
            })
    }

    pub fn complete(
        &mut self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
        effect: WalletRealmCoordinatorEffect,
        outcome: WalletRealmEffectOutcome,
    ) -> Vec<WalletRealmCoordinatorEffect> {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| &entry.profile == profile && &entry.realm == realm)
        else {
            return Vec::new();
        };
        let transition = WalletRealmReconciliationCoordinator::reduce(
            entry.state,
            WalletRealmCoordinatorInput::EffectCompleted { effect, outcome },
        );
        entry.state = *transition.state();
        transition.effects().to_vec()
    }

    pub fn cancel_profile(&mut self, profile: &WalletProfileId) {
        let mut invalidated_realms = Vec::new();
        for entry in self
            .entries
            .iter_mut()
            .filter(|entry| &entry.profile == profile)
        {
            entry.state = *WalletRealmReconciliationCoordinator::reduce(
                entry.state,
                WalletRealmCoordinatorInput::Cancel,
            )
            .state();
            invalidated_realms.push(entry.realm.clone());
        }
        for realm in invalidated_realms {
            self.invalidate_projection(profile, &realm);
        }
    }

    #[must_use]
    pub fn effect_active(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
        effect: WalletRealmCoordinatorEffect,
    ) -> bool {
        self.state(profile, realm)
            .is_some_and(|state| state.accepts(effect))
    }

    #[must_use]
    pub fn state(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Option<WalletRealmCoordinatorState> {
        self.entries
            .iter()
            .find(|entry| &entry.profile == profile && &entry.realm == realm)
            .map(|entry| entry.state)
    }

    #[must_use]
    fn publish(
        &mut self,
        profile: WalletProfileId,
        realm: ChainNetworkId,
        observation_generation: SelectedWalletRealmObservationToken,
        view: &SelectedWalletRealmSyncView,
    ) -> Option<u64> {
        if !self.observations.iter().any(|observation| {
            observation.profile == profile
                && observation.realm == realm
                && observation.generation == observation_generation.generation
                && observation.authority_generation == observation_generation.authority_generation
        }) {
            return None;
        }
        let reconciling = self
            .state(&profile, &realm)
            .is_some_and(WalletRealmCoordinatorState::has_active_leases);
        if let Some(projection) = self
            .projections
            .iter_mut()
            .find(|projection| projection.profile == profile && projection.realm == realm)
        {
            if &projection.view != view || projection.reconciling != reconciling {
                projection.revision = projection.revision.saturating_add(1);
                projection.view.clone_from(view);
            }
            projection.authority_generation = observation_generation.authority_generation;
            projection.reconciling = reconciling;
            return Some(projection.revision);
        }
        self.projections
            .push(SelectedWalletRealmPublishedProjection {
                profile,
                realm,
                revision: 1,
                authority_generation: observation_generation.authority_generation,
                reconciling,
                view: view.clone(),
            });
        Some(1)
    }

    fn authoritative_projection(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Option<SelectedWalletRealmPublishedProjection> {
        let authority_generation = self
            .observations
            .iter()
            .find(|observation| &observation.profile == profile && &observation.realm == realm)?
            .authority_generation;
        self.projections
            .iter()
            .find(|projection| {
                &projection.profile == profile
                    && &projection.realm == realm
                    && projection.authority_generation == authority_generation
            })
            .cloned()
    }

    pub fn expire(
        &mut self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
        effect: WalletRealmCoordinatorEffect,
    ) -> Vec<WalletRealmCoordinatorEffect> {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| &entry.profile == profile && &entry.realm == realm)
        else {
            return Vec::new();
        };
        let transition = WalletRealmReconciliationCoordinator::reduce(
            entry.state,
            WalletRealmCoordinatorInput::EffectExpired(effect),
        );
        entry.state = *transition.state();
        transition.effects().to_vec()
    }
}

/// Application orchestration over the three focused wallet ports.
pub struct SelectedWalletRealmSyncService<W> {
    wallet: Arc<W>,
    runtime: Arc<Mutex<SelectedWalletRealmRuntime>>,
    selection_gate: Arc<Mutex<()>>,
    operation_gate: Mutex<()>,
    timeline: WalletOperationTimeline,
}

struct ObservedWalletRealm {
    view: SelectedWalletRealmSyncView,
    state: WalletRealmReconciliationState,
}

struct SelectedWalletRealmReconciliationAdmission {
    profile: WalletProfileId,
    realm: ChainNetworkId,
    authority: SelectedWalletRealmObservationToken,
    view: SelectedWalletRealmSyncView,
    effects: Vec<WalletRealmCoordinatorEffect>,
    projection: SelectedWalletRealmProjection,
}

enum SelectedWalletRealmEffectPublication {
    Published {
        follow_up: Vec<WalletRealmCoordinatorEffect>,
        projection: SelectedWalletRealmProjection,
    },
    Superseded(SelectedWalletRealmProjection),
}

struct SelectedWalletRealmTimelineOperation {
    timeline: WalletOperationTimeline,
    operation_id: WalletOperationId,
    correlation_id: WalletOperationCorrelationId,
    admission_id: WalletOperationCausationId,
    resource: WalletOperationResource,
    trigger: WalletOperationTrigger,
    started: Instant,
    completed_effects: u16,
    unsuccessful_effects: u16,
    in_progress_effects: u16,
    superseded_effects: u16,
    effect_attempts: [u16; 3],
    last_cause: Cell<WalletOperationCausationId>,
    terminal_recorded: Cell<bool>,
}

impl SelectedWalletRealmTimelineOperation {
    fn begin(
        timeline: WalletOperationTimeline,
        profile: WalletProfileId,
        realm: ChainNetworkId,
        revision: u64,
        trigger: WalletRealmReconciliationTrigger,
    ) -> Option<Self> {
        let resource = WalletOperationResource {
            identity: WalletOperationResourceIdentity::SelectedWalletRealm { profile, realm },
            revision,
        };
        let trigger = WalletOperationTrigger::from(trigger);
        let (operation_id, correlation_id, admission_id) =
            timeline.begin_operation(resource.clone(), trigger).ok()?;
        Some(Self {
            timeline,
            operation_id,
            correlation_id,
            admission_id,
            resource,
            trigger,
            started: Instant::now(),
            completed_effects: 0,
            unsuccessful_effects: 0,
            in_progress_effects: 0,
            superseded_effects: 0,
            effect_attempts: [0; 3],
            last_cause: Cell::new(admission_id),
            terminal_recorded: Cell::new(false),
        })
    }

    fn planned(
        &mut self,
        effect: WalletRealmCoordinatorEffect,
        caused_by: WalletOperationCausationId,
    ) -> (WalletOperationCausationId, WalletOperationAttempt) {
        let attempt_slot = match effect.kind() {
            WalletRealmReconciliationEffect::SyncAccount => &mut self.effect_attempts[0],
            WalletRealmReconciliationEffect::SyncDust => &mut self.effect_attempts[1],
            WalletRealmReconciliationEffect::SyncShielded => &mut self.effect_attempts[2],
        };
        *attempt_slot = attempt_slot
            .saturating_add(1)
            .min(crate::MAX_WALLET_OPERATION_ATTEMPT);
        let attempt = WalletOperationAttempt::new(*attempt_slot)
            .expect("bounded effect attempt is always valid");
        self.resource.revision = effect.revision();
        let cause = self
            .timeline
            .record(
                self.operation_id,
                self.correlation_id,
                Some(caused_by),
                self.resource.clone(),
                self.trigger,
                attempt,
                WalletOperationDurationMillis::zero(),
                WalletOperationEvent::EffectPlanned(effect.kind().into()),
            )
            .unwrap_or(caused_by);
        self.last_cause.set(cause);
        (cause, attempt)
    }

    fn completed(
        &mut self,
        effect: WalletRealmCoordinatorEffect,
        attempt: WalletOperationAttempt,
        caused_by: WalletOperationCausationId,
        timeline_outcome: WalletOperationOutcome,
        failure: Option<WalletOperationFailure>,
        started: Instant,
    ) -> WalletOperationCausationId {
        self.completed_with_measurements(
            effect,
            attempt,
            caused_by,
            timeline_outcome,
            failure,
            started,
            WalletOperationResourceMeasurements::default(),
        )
    }

    fn completed_with_measurements(
        &mut self,
        effect: WalletRealmCoordinatorEffect,
        attempt: WalletOperationAttempt,
        caused_by: WalletOperationCausationId,
        timeline_outcome: WalletOperationOutcome,
        failure: Option<WalletOperationFailure>,
        started: Instant,
        measurements: WalletOperationResourceMeasurements,
    ) -> WalletOperationCausationId {
        self.completed_effects = self.completed_effects.saturating_add(1);
        match timeline_outcome {
            WalletOperationOutcome::Succeeded => {}
            WalletOperationOutcome::InProgress => {
                self.in_progress_effects = self.in_progress_effects.saturating_add(1);
            }
            WalletOperationOutcome::Stale
            | WalletOperationOutcome::Missing
            | WalletOperationOutcome::Blocked
            | WalletOperationOutcome::Unsupported
            | WalletOperationOutcome::PartialFailure
            | WalletOperationOutcome::Failed
            | WalletOperationOutcome::Superseded
            | WalletOperationOutcome::SelectionChanged
            | WalletOperationOutcome::Cancelled
            | WalletOperationOutcome::NoChanges => {
                self.unsuccessful_effects = self.unsuccessful_effects.saturating_add(1);
                if timeline_outcome == WalletOperationOutcome::Superseded {
                    self.superseded_effects = self.superseded_effects.saturating_add(1);
                }
            }
        }
        let cause = self
            .timeline
            .record_with_measurements(
                self.operation_id,
                self.correlation_id,
                Some(caused_by),
                self.resource.clone(),
                self.trigger,
                attempt,
                WalletOperationDurationMillis::bounded(started.elapsed()),
                measurements,
                WalletOperationEvent::EffectCompleted {
                    effect: effect.kind().into(),
                    outcome: timeline_outcome,
                    failure,
                },
            )
            .unwrap_or(caused_by);
        self.last_cause.set(cause);
        cause
    }

    fn completed_from_view(
        &mut self,
        effect: WalletRealmCoordinatorEffect,
        attempt: WalletOperationAttempt,
        caused_by: WalletOperationCausationId,
        timeline_outcome: WalletOperationOutcome,
        failure: Option<WalletOperationFailure>,
        started: Instant,
        view: &SelectedWalletRealmSyncView,
    ) -> WalletOperationCausationId {
        self.completed_with_measurements(
            effect,
            attempt,
            caused_by,
            timeline_outcome,
            failure,
            started,
            resource_measurements(effect.kind(), view),
        )
    }

    fn terminal(
        &self,
        caused_by: WalletOperationCausationId,
        outcome: WalletOperationOutcome,
        failure: Option<WalletOperationFailure>,
    ) {
        self.terminal_recorded.set(true);
        let _ = self.timeline.record(
            self.operation_id,
            self.correlation_id,
            Some(caused_by),
            self.resource.clone(),
            self.trigger,
            WalletOperationAttempt::new(1).expect("one is a valid attempt"),
            WalletOperationDurationMillis::bounded(self.started.elapsed()),
            WalletOperationEvent::Terminal { outcome, failure },
        );
    }

    fn completed_outcome(&self, planned_effects: usize) -> WalletOperationOutcome {
        if planned_effects == 0 {
            WalletOperationOutcome::NoChanges
        } else if self.superseded_effects > 0 {
            WalletOperationOutcome::Superseded
        } else if self.completed_effects > 0 && self.unsuccessful_effects == self.completed_effects
        {
            WalletOperationOutcome::Failed
        } else if self.unsuccessful_effects > 0 {
            WalletOperationOutcome::PartialFailure
        } else if self.in_progress_effects > 0 {
            WalletOperationOutcome::InProgress
        } else {
            WalletOperationOutcome::Succeeded
        }
    }
}

impl Drop for SelectedWalletRealmTimelineOperation {
    fn drop(&mut self) {
        if !self.terminal_recorded.get() {
            self.terminal(
                self.last_cause.get(),
                WalletOperationOutcome::Cancelled,
                Some(WalletOperationFailure::OperationCancelled),
            );
        }
    }
}

struct WalletRealmLeaseGuard<'a> {
    runtime: &'a Mutex<SelectedWalletRealmRuntime>,
    profile: WalletProfileId,
    realm: ChainNetworkId,
    active: Vec<WalletRealmCoordinatorEffect>,
}

impl WalletRealmLeaseGuard<'_> {
    fn effect_active(
        &self,
        effect: WalletRealmCoordinatorEffect,
    ) -> Result<bool, SelectedWalletRealmSyncError> {
        self.runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)
            .map(|runtime| runtime.effect_active(&self.profile, &self.realm, effect))
    }

    fn settle(
        &mut self,
        effect: WalletRealmCoordinatorEffect,
        follow_up: &[WalletRealmCoordinatorEffect],
    ) {
        self.active.retain(|candidate| *candidate != effect);
        self.active.extend(follow_up.iter().copied());
    }

    fn discard(&mut self, effect: WalletRealmCoordinatorEffect) {
        self.active.retain(|candidate| *candidate != effect);
    }
}

impl Drop for WalletRealmLeaseGuard<'_> {
    fn drop(&mut self) {
        let Ok(mut runtime) = self.runtime.lock() else {
            return;
        };
        while let Some(effect) = self.active.pop() {
            let follow_up = runtime.expire(&self.profile, &self.realm, effect);
            self.active.extend(follow_up);
        }
    }
}

impl<W> SelectedWalletRealmSyncService<W> {
    #[must_use]
    pub fn new(wallet: Arc<W>) -> Self {
        Self::with_runtime(
            wallet,
            Arc::new(Mutex::new(SelectedWalletRealmRuntime::default())),
        )
    }

    #[must_use]
    pub fn with_runtime(wallet: Arc<W>, runtime: Arc<Mutex<SelectedWalletRealmRuntime>>) -> Self {
        Self::with_runtime_and_selection_gate(wallet, runtime, Arc::new(Mutex::new(())))
    }

    #[must_use]
    pub fn with_runtime_and_selection_gate(
        wallet: Arc<W>,
        runtime: Arc<Mutex<SelectedWalletRealmRuntime>>,
        selection_gate: Arc<Mutex<()>>,
    ) -> Self {
        Self::with_runtime_selection_gate_and_timeline(
            wallet,
            runtime,
            selection_gate,
            WalletOperationTimeline::default(),
        )
    }

    #[must_use]
    pub fn with_runtime_selection_gate_and_timeline(
        wallet: Arc<W>,
        runtime: Arc<Mutex<SelectedWalletRealmRuntime>>,
        selection_gate: Arc<Mutex<()>>,
        timeline: WalletOperationTimeline,
    ) -> Self {
        Self {
            wallet,
            runtime,
            selection_gate,
            operation_gate: Mutex::new(()),
            timeline,
        }
    }

    fn profile(
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<WalletProfileId, SelectedWalletRealmSyncError> {
        WalletProfileId::parse(command.profile_id)
            .map_err(SelectedWalletRealmSyncError::InvalidProfileIdentifier)
    }

    fn begin_observation(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Result<SelectedWalletRealmObservationToken, SelectedWalletRealmSyncError> {
        self.runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)
            .and_then(|mut runtime| {
                runtime
                    .begin_observation(profile, realm)
                    .ok_or(SelectedWalletRealmSyncError::SelectionChanged)
            })
    }

    fn projection(
        &self,
        profile: WalletProfileId,
        realm: ChainNetworkId,
        observation_generation: SelectedWalletRealmObservationToken,
        view: SelectedWalletRealmSyncView,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError>
    where
        W: WalletNetworkPort,
    {
        let _selection = self
            .selection_gate
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        self.publish_projection_while_selected(profile, realm, observation_generation, view)
    }

    fn publish_projection_while_selected(
        &self,
        profile: WalletProfileId,
        realm: ChainNetworkId,
        observation_generation: SelectedWalletRealmObservationToken,
        view: SelectedWalletRealmSyncView,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError>
    where
        W: WalletNetworkPort,
    {
        self.ensure_selected_realm(&profile, &realm)?;
        let published = {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
            if !runtime.selection_matches(&profile, &realm) {
                return Err(SelectedWalletRealmSyncError::SelectionChanged);
            }
            let _ = runtime.publish(
                profile.clone(),
                realm.clone(),
                observation_generation,
                &view,
            );
            runtime.authoritative_projection(&profile, &realm)
        };
        let published = published.ok_or(SelectedWalletRealmSyncError::ObservationSuperseded)?;
        Ok(Self::projection_from_published(profile, realm, published))
    }

    fn projection_from_published(
        profile: WalletProfileId,
        realm: ChainNetworkId,
        published: SelectedWalletRealmPublishedProjection,
    ) -> SelectedWalletRealmProjection {
        let view = published.view;
        let fresh = selected_realm_is_fresh(&view);
        let consistent = selected_realm_is_consistent(&view);
        let refreshing = published.reconciling || selected_realm_is_refreshing(&view);
        SelectedWalletRealmProjection {
            identity: SelectedWalletRealmIdentity { profile, realm },
            revision: published.revision,
            fresh,
            consistent,
            actionable: if fresh && consistent {
                SelectedWalletRealmActionReadiness::Ready
            } else if refreshing {
                SelectedWalletRealmActionReadiness::Refreshing
            } else {
                SelectedWalletRealmActionReadiness::Unavailable
            },
            observation: if refreshing {
                SelectedWalletRealmObservation::PollAfter(Duration::from_millis(150))
            } else {
                SelectedWalletRealmObservation::Settled
            },
            view,
        }
    }

    fn observed(&self, profile: &WalletProfileId, realm: &ChainNetworkId) -> ObservedWalletRealm
    where
        W: WalletAccountReadPort + WalletDustSyncPort + WalletShieldedSyncPort,
    {
        let (account, account_state) =
            observe_account(self.wallet.account_in_realm(profile, realm));
        let (dust, dust_state) = observe_dust(self.wallet.dust_status_in_realm(profile, realm));
        let (shielded, shielded_state) =
            observe_shielded(self.wallet.shielded_status_in_realm(profile, realm));
        ObservedWalletRealm {
            view: SelectedWalletRealmSyncView {
                account,
                dust,
                shielded,
            },
            state: WalletRealmReconciliationState {
                account: account_state,
                dust: dust_state,
                shielded: shielded_state,
            },
        }
    }
}

impl<W> SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort,
{
    fn ensure_selected_realm(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Result<(), SelectedWalletRealmSyncError> {
        let selected = self
            .wallet
            .selected_network(profile)
            .map_err(SelectedWalletRealmSyncError::SelectedNetwork)?;
        if &selected == realm {
            Ok(())
        } else {
            Err(SelectedWalletRealmSyncError::SelectionChanged)
        }
    }
}

impl<W> SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort
        + WalletAccountReadPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + 'static,
{
    fn pin_selected_realm(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Result<(), SelectedWalletRealmSyncError> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        let retired = self
            .runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
            .retire_other_realms(profile, realm);
        for retired_realm in retired {
            let _ = self
                .wallet
                .cancel_dust_sync_in_realm(profile, &retired_realm);
            let _ = self
                .wallet
                .cancel_shielded_sync_in_realm(profile, &retired_realm);
        }
        Ok(())
    }

    fn begin_selected_realm_observation(
        &self,
        profile: &WalletProfileId,
    ) -> Result<(ChainNetworkId, SelectedWalletRealmObservationToken), SelectedWalletRealmSyncError>
    {
        let _selection = self
            .selection_gate
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        let realm = self
            .wallet
            .selected_network(profile)
            .map_err(SelectedWalletRealmSyncError::SelectedNetwork)?;
        self.pin_selected_realm(profile, &realm)?;
        let observation_generation = self.begin_observation(profile, &realm)?;
        Ok((realm, observation_generation))
    }

    fn admit_reconciliation(
        &self,
        profile: WalletProfileId,
        trigger: WalletRealmReconciliationTrigger,
    ) -> Result<SelectedWalletRealmReconciliationAdmission, SelectedWalletRealmSyncError> {
        let _selection = self
            .selection_gate
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        let realm = self
            .wallet
            .selected_network(&profile)
            .map_err(SelectedWalletRealmSyncError::SelectedNetwork)?;
        self.pin_selected_realm(&profile, &realm)?;
        let authority = self.begin_observation(&profile, &realm)?;
        let observed = self.observed(&profile, &realm);
        let view = observed.view;
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        let effects = runtime.reconcile(profile.clone(), realm.clone(), observed.state, trigger);
        if runtime
            .publish(profile.clone(), realm.clone(), authority, &view)
            .is_none()
        {
            return Err(SelectedWalletRealmSyncError::ObservationSuperseded);
        }
        let projection = runtime
            .authoritative_projection(&profile, &realm)
            .map(|published| {
                Self::projection_from_published(profile.clone(), realm.clone(), published)
            })
            .ok_or(SelectedWalletRealmSyncError::ObservationSuperseded)?;
        Ok(SelectedWalletRealmReconciliationAdmission {
            profile,
            realm,
            authority,
            view,
            effects,
            projection,
        })
    }

    fn complete_effect_and_publish(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
        authority: SelectedWalletRealmObservationToken,
        effect: WalletRealmCoordinatorEffect,
        outcome: WalletRealmEffectOutcome,
        completed: &SelectedWalletRealmSyncView,
    ) -> Result<SelectedWalletRealmEffectPublication, SelectedWalletRealmSyncError> {
        let _selection = self
            .selection_gate
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        self.ensure_selected_realm(profile, realm)?;
        let mut completed = completed.clone();
        match effect.kind() {
            WalletRealmReconciliationEffect::SyncAccount => {}
            WalletRealmReconciliationEffect::SyncDust => {
                let latest = observe_dust(self.wallet.dust_status_in_realm(profile, realm)).0;
                if matches!(latest, WalletRealmFamilyView::Ready(_)) {
                    completed.dust = latest;
                }
            }
            WalletRealmReconciliationEffect::SyncShielded => {
                let latest =
                    observe_shielded(self.wallet.shielded_status_in_realm(profile, realm)).0;
                if matches!(latest, WalletRealmFamilyView::Ready(_)) {
                    completed.shielded = latest;
                }
            }
        }
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        if !runtime.authority_matches(profile, realm, authority) {
            return runtime
                .authoritative_projection(profile, realm)
                .map(|published| {
                    SelectedWalletRealmEffectPublication::Superseded(
                        Self::projection_from_published(profile.clone(), realm.clone(), published),
                    )
                })
                .ok_or(SelectedWalletRealmSyncError::ObservationSuperseded);
        }
        let follow_up = runtime.complete(profile, realm, effect, outcome);
        let mut view = runtime
            .authoritative_projection(profile, realm)
            .map_or_else(|| completed.clone(), |published| published.view);
        match effect.kind() {
            WalletRealmReconciliationEffect::SyncAccount => {
                view.account.clone_from(&completed.account);
            }
            WalletRealmReconciliationEffect::SyncDust => {
                view.dust.clone_from(&completed.dust);
            }
            WalletRealmReconciliationEffect::SyncShielded => {
                view.shielded.clone_from(&completed.shielded);
            }
        }
        let observation = runtime.advance_observation_generation(profile, realm);
        if runtime
            .publish(profile.clone(), realm.clone(), observation, &view)
            .is_none()
        {
            return Err(SelectedWalletRealmSyncError::ObservationSuperseded);
        }
        let projection = runtime
            .authoritative_projection(profile, realm)
            .map(|published| {
                Self::projection_from_published(profile.clone(), realm.clone(), published)
            })
            .ok_or(SelectedWalletRealmSyncError::ObservationSuperseded)?;
        Ok(SelectedWalletRealmEffectPublication::Published {
            follow_up,
            projection,
        })
    }

    /// Reconciles the selected realm for one explicit application trigger.
    pub fn reconcile(
        &self,
        command: SelectedWalletRealmSyncCommand,
        trigger: WalletRealmReconciliationTrigger,
    ) -> SelectedWalletRealmProjectionFuture<'_> {
        Box::pin(async move {
            let profile = Self::profile(command)?;
            let admission = self.admit_reconciliation(profile, trigger)?;
            let profile = admission.profile;
            let realm = admission.realm;
            let authority = admission.authority;
            let mut view = admission.view;
            let mut effects = admission.effects;
            let mut projection = admission.projection;
            let revision = effects
                .first()
                .map_or(projection.revision, |effect| effect.revision());
            let mut timeline = SelectedWalletRealmTimelineOperation::begin(
                self.timeline.clone(),
                profile.clone(),
                realm.clone(),
                revision,
                trigger,
            );
            let mut effect_causes = effects
                .iter()
                .map(|effect| {
                    timeline
                        .as_mut()
                        .map(|operation| operation.planned(*effect, operation.admission_id))
                })
                .collect::<Vec<_>>();
            let mut planned_effects = effects.len();
            let mut leases = WalletRealmLeaseGuard {
                runtime: &self.runtime,
                profile: profile.clone(),
                realm: realm.clone(),
                active: effects.clone(),
            };
            while !effects.is_empty() {
                let effect = effects.remove(0);
                let effect_cause = effect_causes.remove(0);
                let effect_started = Instant::now();
                let outcome = match effect.kind() {
                    WalletRealmReconciliationEffect::SyncAccount => {
                        match leases.effect_active(effect) {
                            Ok(true) => {}
                            Ok(false) => {
                                if let (Some(operation), Some((cause, attempt))) =
                                    (timeline.as_mut(), effect_cause)
                                {
                                    operation.completed(
                                        effect,
                                        attempt,
                                        cause,
                                        WalletOperationOutcome::Superseded,
                                        Some(WalletOperationFailure::ObservationSuperseded),
                                        effect_started,
                                    );
                                }
                                leases.discard(effect);
                                continue;
                            }
                            Err(error) => {
                                record_terminal_error(timeline.as_ref(), &error);
                                return Err(error);
                            }
                        }
                        let (family, state) =
                            observe_account(self.wallet.sync_in_realm(&profile, &realm).await);
                        view.account = family;
                        effect_outcome(state)
                    }
                    WalletRealmReconciliationEffect::SyncDust => {
                        let _operation = match self.operation_gate.lock() {
                            Ok(operation) => operation,
                            Err(_) => {
                                let error = SelectedWalletRealmSyncError::Unavailable;
                                record_terminal_error(timeline.as_ref(), &error);
                                return Err(error);
                            }
                        };
                        match leases.effect_active(effect) {
                            Ok(true) => {}
                            Ok(false) => {
                                if let (Some(operation), Some((cause, attempt))) =
                                    (timeline.as_mut(), effect_cause)
                                {
                                    operation.completed(
                                        effect,
                                        attempt,
                                        cause,
                                        WalletOperationOutcome::Superseded,
                                        Some(WalletOperationFailure::ObservationSuperseded),
                                        effect_started,
                                    );
                                }
                                leases.discard(effect);
                                continue;
                            }
                            Err(error) => {
                                record_terminal_error(timeline.as_ref(), &error);
                                return Err(error);
                            }
                        }
                        let (family, state) =
                            observe_dust(self.wallet.start_dust_sync_in_realm(&profile, &realm));
                        view.dust = family;
                        effect_outcome(state)
                    }
                    WalletRealmReconciliationEffect::SyncShielded => {
                        let _operation = match self.operation_gate.lock() {
                            Ok(operation) => operation,
                            Err(_) => {
                                let error = SelectedWalletRealmSyncError::Unavailable;
                                record_terminal_error(timeline.as_ref(), &error);
                                return Err(error);
                            }
                        };
                        match leases.effect_active(effect) {
                            Ok(true) => {}
                            Ok(false) => {
                                if let (Some(operation), Some((cause, attempt))) =
                                    (timeline.as_mut(), effect_cause)
                                {
                                    operation.completed(
                                        effect,
                                        attempt,
                                        cause,
                                        WalletOperationOutcome::Superseded,
                                        Some(WalletOperationFailure::ObservationSuperseded),
                                        effect_started,
                                    );
                                }
                                leases.discard(effect);
                                continue;
                            }
                            Err(error) => {
                                record_terminal_error(timeline.as_ref(), &error);
                                return Err(error);
                            }
                        }
                        let (family, state) = observe_shielded(
                            self.wallet.start_shielded_sync_in_realm(&profile, &realm),
                        );
                        view.shielded = family;
                        effect_outcome(state)
                    }
                };
                let failure = timeline_effect_failure(effect.kind(), outcome, &view);
                let publication = match self.complete_effect_and_publish(
                    &profile, &realm, authority, effect, outcome, &view,
                ) {
                    Ok(publication) => publication,
                    Err(error) => {
                        if let (Some(operation), Some((cause, attempt))) =
                            (timeline.as_mut(), effect_cause)
                        {
                            operation.completed_from_view(
                                effect,
                                attempt,
                                cause,
                                timeline_effect_outcome(outcome),
                                failure,
                                effect_started,
                                &view,
                            );
                        }
                        record_terminal_error(timeline.as_ref(), &error);
                        return Err(error);
                    }
                };
                match publication {
                    SelectedWalletRealmEffectPublication::Published {
                        follow_up,
                        projection: published,
                    } => {
                        if let (Some(operation), Some((cause, attempt))) =
                            (timeline.as_mut(), effect_cause)
                        {
                            operation.completed_from_view(
                                effect,
                                attempt,
                                cause,
                                timeline_effect_outcome(outcome),
                                failure,
                                effect_started,
                                &published.view,
                            );
                        }
                        leases.settle(effect, &follow_up);
                        for follow_up_effect in &follow_up {
                            let cause = timeline.as_mut().map(|operation| {
                                operation.planned(*follow_up_effect, operation.last_cause.get())
                            });
                            effect_causes.push(cause);
                        }
                        planned_effects = planned_effects.saturating_add(follow_up.len());
                        effects.extend(follow_up);
                        projection = published;
                    }
                    SelectedWalletRealmEffectPublication::Superseded(published) => {
                        if let (Some(operation), Some((cause, attempt))) =
                            (timeline.as_mut(), effect_cause)
                        {
                            operation.completed_from_view(
                                effect,
                                attempt,
                                cause,
                                timeline_effect_outcome(outcome),
                                failure,
                                effect_started,
                                &published.view,
                            );
                        }
                        if let Some(operation) = timeline.as_ref() {
                            operation.terminal(
                                operation.last_cause.get(),
                                WalletOperationOutcome::Superseded,
                                Some(WalletOperationFailure::ObservationSuperseded),
                            );
                        }
                        return Ok(published);
                    }
                }
            }
            if let Some(operation) = timeline.as_ref() {
                operation.terminal(
                    operation.last_cause.get(),
                    operation.completed_outcome(planned_effects),
                    None,
                );
            }
            Ok(projection)
        })
    }
}

impl<W> GetWalletOperationTimelineUseCase for SelectedWalletRealmSyncService<W>
where
    W: Send + Sync,
{
    fn execute(&self) -> Result<WalletOperationTimelineSnapshot, WalletOperationTimelineError> {
        self.timeline.query()
    }
}

impl<W> WalletNetworkSelectionObserver for SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort
        + WalletAccountReadPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + 'static,
{
    fn selected(
        &self,
        profile: &WalletProfileId,
        network: &ChainNetworkId,
    ) -> Result<(), WalletAccountPortError> {
        self.pin_selected_realm(profile, network)
            .map_err(|_| WalletAccountPortError::Unavailable)
    }
}

impl<W> ReconcileSelectedWalletRealmUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort
        + WalletAccountReadPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
        trigger: WalletRealmReconciliationTrigger,
    ) -> SelectedWalletRealmReconciliationFuture<'_> {
        Box::pin(async move {
            let projection = self.reconcile(command, trigger).await?;
            let facets = self
                .runtime
                .lock()
                .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
                .facets(&projection.identity.profile, &projection.identity.realm)
                .ok_or(SelectedWalletRealmSyncError::ObservationSuperseded)?;
            Ok(SelectedWalletRealmReconciliation { projection, facets })
        })
    }
}

impl<W> SyncSelectedWalletRealmUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort
        + WalletAccountReadPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> SelectedWalletRealmProjectionFuture<'_> {
        self.reconcile(command, WalletRealmReconciliationTrigger::ManualRefresh)
    }
}

impl<W> GetSelectedWalletRealmSyncUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort
        + WalletAccountReadPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError> {
        let profile = Self::profile(command)?;
        let (realm, observation_generation) = self.begin_selected_realm_observation(&profile)?;
        let view = self.observed(&profile, &realm).view;
        self.projection(profile, realm, observation_generation, view)
    }
}

impl<W> CancelSelectedWalletRealmSyncUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort
        + WalletAccountReadPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError> {
        let profile = Self::profile(command)?;
        let _selection = self
            .selection_gate
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        let realm = self
            .wallet
            .selected_network(&profile)
            .map_err(SelectedWalletRealmSyncError::SelectedNetwork)?;
        self.pin_selected_realm(&profile, &realm)?;
        let operation = self
            .operation_gate
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        self.runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
            .cancel_profile(&profile);
        let dust = self
            .wallet
            .cancel_dust_sync_in_realm(&profile, &realm)
            .map(|snapshot| WalletRealmFamilyView::Ready(WalletDustSyncView::from(&snapshot)))
            .unwrap_or_else(dust_family_error);
        let shielded = self
            .wallet
            .cancel_shielded_sync_in_realm(&profile, &realm)
            .map(|snapshot| WalletRealmFamilyView::Ready(WalletShieldedSyncView::from(&snapshot)))
            .unwrap_or_else(shielded_family_error);
        let account = self
            .wallet
            .account_in_realm(&profile, &realm)
            .map(|snapshot| {
                WalletRealmFamilyView::Ready(WalletAccountView::from_snapshot(&snapshot))
            })
            .unwrap_or_else(account_family_error);
        drop(operation);
        self.ensure_selected_realm(&profile, &realm)?;
        let observation_generation = self.begin_observation(&profile, &realm)?;
        self.publish_projection_while_selected(
            profile,
            realm,
            observation_generation,
            SelectedWalletRealmSyncView {
                account,
                dust,
                shielded,
            },
        )
    }
}

fn resource_measurements(
    effect: WalletRealmReconciliationEffect,
    view: &SelectedWalletRealmSyncView,
) -> WalletOperationResourceMeasurements {
    let values = match effect {
        WalletRealmReconciliationEffect::SyncDust => match &view.dust {
            WalletRealmFamilyView::Ready(dust) => [
                dust.current_cursor
                    .map(WalletOperationResourceMeasurement::CurrentCursor),
                dust.target_cursor
                    .map(WalletOperationResourceMeasurement::TargetCursor),
                Some(WalletOperationResourceMeasurement::EventsProcessed(
                    dust.events_processed,
                )),
            ]
            .into_iter()
            .flatten()
            .collect(),
            _ => Vec::new(),
        },
        WalletRealmReconciliationEffect::SyncShielded => match &view.shielded {
            WalletRealmFamilyView::Ready(shielded) => [
                shielded
                    .current_cursor
                    .map(WalletOperationResourceMeasurement::CurrentCursor),
                shielded
                    .target_cursor
                    .map(WalletOperationResourceMeasurement::TargetCursor),
                Some(WalletOperationResourceMeasurement::EventsProcessed(
                    shielded.events_processed,
                )),
                shielded
                    .owned_note_count
                    .map(WalletOperationResourceMeasurement::OwnedNoteCount),
                shielded
                    .commitment_count
                    .map(WalletOperationResourceMeasurement::CommitmentCount),
            ]
            .into_iter()
            .flatten()
            .collect(),
            _ => Vec::new(),
        },
        WalletRealmReconciliationEffect::SyncAccount => Vec::new(),
    };
    WalletOperationResourceMeasurements::from_values(values)
}

fn record_terminal_error(
    operation: Option<&SelectedWalletRealmTimelineOperation>,
    error: &SelectedWalletRealmSyncError,
) {
    let (outcome, failure) = match error {
        SelectedWalletRealmSyncError::InvalidProfileIdentifier(_) => (
            WalletOperationOutcome::Failed,
            WalletOperationFailure::InvalidAdapterData,
        ),
        SelectedWalletRealmSyncError::SelectedNetwork(error) => (
            WalletOperationOutcome::Failed,
            account_timeline_failure(*error),
        ),
        SelectedWalletRealmSyncError::SelectionChanged => (
            WalletOperationOutcome::SelectionChanged,
            WalletOperationFailure::SelectionChanged,
        ),
        SelectedWalletRealmSyncError::ObservationSuperseded => (
            WalletOperationOutcome::Superseded,
            WalletOperationFailure::ObservationSuperseded,
        ),
        SelectedWalletRealmSyncError::Unavailable => (
            WalletOperationOutcome::Failed,
            WalletOperationFailure::RuntimeUnavailable,
        ),
    };
    if let Some(operation) = operation {
        operation.terminal(operation.last_cause.get(), outcome, Some(failure));
    }
}

fn timeline_effect_failure(
    effect: WalletRealmReconciliationEffect,
    outcome: WalletRealmEffectOutcome,
    view: &SelectedWalletRealmSyncView,
) -> Option<WalletOperationFailure> {
    if matches!(outcome, WalletRealmEffectOutcome::Current) {
        return None;
    }
    match effect {
        WalletRealmReconciliationEffect::SyncAccount => match &view.account {
            WalletRealmFamilyView::NotFound => Some(WalletOperationFailure::AccountNotFound),
            WalletRealmFamilyView::Unsupported => Some(WalletOperationFailure::UnsupportedNetwork),
            WalletRealmFamilyView::ProtectionNotInitialized => {
                Some(WalletOperationFailure::ProtectionNotInitialized)
            }
            WalletRealmFamilyView::ProtectionLocked => {
                Some(WalletOperationFailure::ProtectionLocked)
            }
            WalletRealmFamilyView::InvalidData => Some(WalletOperationFailure::InvalidAdapterData),
            WalletRealmFamilyView::Busy | WalletRealmFamilyView::Unavailable => {
                Some(WalletOperationFailure::AdapterUnavailable)
            }
            WalletRealmFamilyView::Ready(_) => None,
        },
        WalletRealmReconciliationEffect::SyncDust => match &view.dust {
            WalletRealmFamilyView::Ready(value) => sync_timeline_failure(value.failure.as_deref()),
            family => family_timeline_failure(family),
        },
        WalletRealmReconciliationEffect::SyncShielded => match &view.shielded {
            WalletRealmFamilyView::Ready(value) => sync_timeline_failure(value.failure.as_deref()),
            family => family_timeline_failure(family),
        },
    }
}

fn sync_timeline_failure(failure: Option<&str>) -> Option<WalletOperationFailure> {
    match failure {
        Some("protection_not_initialized") => {
            Some(WalletOperationFailure::ProtectionNotInitialized)
        }
        Some("protection_locked") => Some(WalletOperationFailure::ProtectionLocked),
        Some("unsupported_network") => Some(WalletOperationFailure::UnsupportedNetwork),
        Some("invalid_chain_state") => Some(WalletOperationFailure::InvalidAdapterData),
        Some("transport_unavailable" | "timed_out" | "storage_unavailable") => {
            Some(WalletOperationFailure::AdapterUnavailable)
        }
        _ => None,
    }
}

fn family_timeline_failure<T>(family: &WalletRealmFamilyView<T>) -> Option<WalletOperationFailure> {
    match family {
        WalletRealmFamilyView::Busy => Some(WalletOperationFailure::Conflict),
        WalletRealmFamilyView::NotFound => Some(WalletOperationFailure::AccountNotFound),
        WalletRealmFamilyView::Unsupported => Some(WalletOperationFailure::UnsupportedNetwork),
        WalletRealmFamilyView::ProtectionNotInitialized => {
            Some(WalletOperationFailure::ProtectionNotInitialized)
        }
        WalletRealmFamilyView::ProtectionLocked => Some(WalletOperationFailure::ProtectionLocked),
        WalletRealmFamilyView::InvalidData => Some(WalletOperationFailure::InvalidAdapterData),
        WalletRealmFamilyView::Unavailable => Some(WalletOperationFailure::AdapterUnavailable),
        WalletRealmFamilyView::Ready(_) => None,
    }
}

const fn account_timeline_failure(error: WalletAccountPortError) -> WalletOperationFailure {
    match error {
        WalletAccountPortError::NotFound => WalletOperationFailure::AccountNotFound,
        WalletAccountPortError::UnsupportedNetwork => WalletOperationFailure::UnsupportedNetwork,
        WalletAccountPortError::ProtectionNotInitialized => {
            WalletOperationFailure::ProtectionNotInitialized
        }
        WalletAccountPortError::ProtectionLocked => WalletOperationFailure::ProtectionLocked,
        WalletAccountPortError::Unavailable => WalletOperationFailure::AdapterUnavailable,
        WalletAccountPortError::InvalidData => WalletOperationFailure::InvalidAdapterData,
    }
}

fn selected_realm_is_fresh(view: &SelectedWalletRealmSyncView) -> bool {
    matches!(&view.account, WalletRealmFamilyView::Ready(account) if account.sync.state == "synced" && account.source == "live")
        && matches!(&view.dust, WalletRealmFamilyView::Ready(dust) if dust.state == "synced" && dust.failure.is_none())
        && matches!(&view.shielded, WalletRealmFamilyView::Ready(shielded) if shielded.state == "synced" && shielded.failure.is_none())
}

fn selected_realm_is_refreshing(view: &SelectedWalletRealmSyncView) -> bool {
    matches!(&view.account, WalletRealmFamilyView::Ready(account) if account.sync.state == "syncing")
        || matches!(&view.dust, WalletRealmFamilyView::Ready(dust) if dust.state == "syncing")
        || matches!(&view.shielded, WalletRealmFamilyView::Ready(shielded) if shielded.state == "syncing")
        || matches!(&view.dust, WalletRealmFamilyView::Busy)
        || matches!(&view.shielded, WalletRealmFamilyView::Busy)
}

fn selected_realm_is_consistent(view: &SelectedWalletRealmSyncView) -> bool {
    !matches!(
        &view.account,
        WalletRealmFamilyView::InvalidData | WalletRealmFamilyView::Unavailable
    ) && !matches!(
        &view.dust,
        WalletRealmFamilyView::InvalidData | WalletRealmFamilyView::Unavailable
    ) && !matches!(
        &view.shielded,
        WalletRealmFamilyView::InvalidData | WalletRealmFamilyView::Unavailable
    )
}

fn observe_account(
    result: Result<WalletAccountSnapshot, WalletAccountPortError>,
) -> (
    WalletRealmFamilyView<WalletAccountView>,
    WalletRealmFacetState,
) {
    match result {
        Ok(snapshot) => {
            let state = match snapshot.sync().state() {
                WalletSyncState::Synced if snapshot.source() == WalletAccountSource::Live => {
                    WalletRealmFacetState::Current
                }
                WalletSyncState::Syncing => WalletRealmFacetState::Updating,
                WalletSyncState::NeverSynced => WalletRealmFacetState::Missing,
                WalletSyncState::Synced
                | WalletSyncState::Stalled
                | WalletSyncState::Unavailable => WalletRealmFacetState::Stale,
            };
            (
                WalletRealmFamilyView::Ready(WalletAccountView::from_snapshot(&snapshot)),
                state,
            )
        }
        Err(error) => (account_family_error(error), account_error_state(error)),
    }
}

fn observe_dust(
    result: Result<WalletDustSyncSnapshot, WalletDustSyncPortError>,
) -> (
    WalletRealmFamilyView<WalletDustSyncView>,
    WalletRealmFacetState,
) {
    match result {
        Ok(snapshot) => {
            let state = snapshot.failure().map_or_else(
                || match snapshot.state() {
                    WalletDustSyncState::Synced => WalletRealmFacetState::Current,
                    WalletDustSyncState::Syncing => WalletRealmFacetState::Updating,
                    WalletDustSyncState::NeverSynced => WalletRealmFacetState::Missing,
                    WalletDustSyncState::Cached
                    | WalletDustSyncState::Cancelled
                    | WalletDustSyncState::Stalled
                    | WalletDustSyncState::Unavailable => WalletRealmFacetState::Stale,
                },
                dust_failure_state,
            );
            (
                WalletRealmFamilyView::Ready(WalletDustSyncView::from(&snapshot)),
                state,
            )
        }
        Err(error) => (dust_family_error(error), dust_error_state(error)),
    }
}

fn observe_shielded(
    result: Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError>,
) -> (
    WalletRealmFamilyView<WalletShieldedSyncView>,
    WalletRealmFacetState,
) {
    match result {
        Ok(snapshot) => {
            let state = snapshot.failure().map_or_else(
                || match snapshot.state() {
                    WalletShieldedSyncState::Synced => WalletRealmFacetState::Current,
                    WalletShieldedSyncState::Syncing => WalletRealmFacetState::Updating,
                    WalletShieldedSyncState::NeverSynced => WalletRealmFacetState::Missing,
                    WalletShieldedSyncState::Cached
                    | WalletShieldedSyncState::Cancelled
                    | WalletShieldedSyncState::Stalled
                    | WalletShieldedSyncState::Unavailable => WalletRealmFacetState::Stale,
                },
                shielded_failure_state,
            );
            (
                WalletRealmFamilyView::Ready(WalletShieldedSyncView::from(&snapshot)),
                state,
            )
        }
        Err(error) => (shielded_family_error(error), shielded_error_state(error)),
    }
}

const fn dust_failure_state(failure: WalletDustSyncFailure) -> WalletRealmFacetState {
    match failure {
        WalletDustSyncFailure::ProtectionNotInitialized
        | WalletDustSyncFailure::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletDustSyncFailure::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletDustSyncFailure::TransportUnavailable
        | WalletDustSyncFailure::TimedOut
        | WalletDustSyncFailure::InvalidChainState
        | WalletDustSyncFailure::StorageUnavailable => WalletRealmFacetState::Stale,
    }
}

const fn shielded_failure_state(failure: WalletShieldedSyncFailure) -> WalletRealmFacetState {
    match failure {
        WalletShieldedSyncFailure::ProtectionNotInitialized
        | WalletShieldedSyncFailure::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletShieldedSyncFailure::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletShieldedSyncFailure::TransportUnavailable
        | WalletShieldedSyncFailure::TimedOut
        | WalletShieldedSyncFailure::InvalidChainState
        | WalletShieldedSyncFailure::StorageUnavailable => WalletRealmFacetState::Stale,
    }
}

const fn account_error_state(error: WalletAccountPortError) -> WalletRealmFacetState {
    match error {
        WalletAccountPortError::NotFound => WalletRealmFacetState::Missing,
        WalletAccountPortError::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletAccountPortError::ProtectionNotInitialized
        | WalletAccountPortError::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletAccountPortError::Unavailable | WalletAccountPortError::InvalidData => {
            WalletRealmFacetState::Stale
        }
    }
}

const fn dust_error_state(error: WalletDustSyncPortError) -> WalletRealmFacetState {
    match error {
        WalletDustSyncPortError::Conflict => WalletRealmFacetState::Updating,
        WalletDustSyncPortError::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletDustSyncPortError::ProtectionNotInitialized
        | WalletDustSyncPortError::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletDustSyncPortError::Unavailable | WalletDustSyncPortError::InvalidData => {
            WalletRealmFacetState::Stale
        }
    }
}

const fn shielded_error_state(error: WalletShieldedSyncPortError) -> WalletRealmFacetState {
    match error {
        WalletShieldedSyncPortError::Conflict => WalletRealmFacetState::Updating,
        WalletShieldedSyncPortError::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletShieldedSyncPortError::ProtectionNotInitialized
        | WalletShieldedSyncPortError::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletShieldedSyncPortError::Unavailable | WalletShieldedSyncPortError::InvalidData => {
            WalletRealmFacetState::Stale
        }
    }
}

const fn effect_outcome(state: WalletRealmFacetState) -> WalletRealmEffectOutcome {
    match state {
        WalletRealmFacetState::Current => WalletRealmEffectOutcome::Current,
        WalletRealmFacetState::Missing => WalletRealmEffectOutcome::Missing,
        WalletRealmFacetState::Blocked => WalletRealmEffectOutcome::Blocked,
        WalletRealmFacetState::Unsupported => WalletRealmEffectOutcome::Unsupported,
        WalletRealmFacetState::Updating => WalletRealmEffectOutcome::InProgress,
        WalletRealmFacetState::Stale => WalletRealmEffectOutcome::Stale,
    }
}

const fn account_family_error(
    error: WalletAccountPortError,
) -> WalletRealmFamilyView<WalletAccountView> {
    match error {
        WalletAccountPortError::NotFound => WalletRealmFamilyView::NotFound,
        WalletAccountPortError::UnsupportedNetwork => WalletRealmFamilyView::Unsupported,
        WalletAccountPortError::ProtectionNotInitialized => {
            WalletRealmFamilyView::ProtectionNotInitialized
        }
        WalletAccountPortError::ProtectionLocked => WalletRealmFamilyView::ProtectionLocked,
        WalletAccountPortError::Unavailable => WalletRealmFamilyView::Unavailable,
        WalletAccountPortError::InvalidData => WalletRealmFamilyView::InvalidData,
    }
}

const fn dust_family_error(
    error: WalletDustSyncPortError,
) -> WalletRealmFamilyView<WalletDustSyncView> {
    match error {
        WalletDustSyncPortError::Conflict => WalletRealmFamilyView::Busy,
        WalletDustSyncPortError::UnsupportedNetwork => WalletRealmFamilyView::Unsupported,
        WalletDustSyncPortError::ProtectionNotInitialized => {
            WalletRealmFamilyView::ProtectionNotInitialized
        }
        WalletDustSyncPortError::ProtectionLocked => WalletRealmFamilyView::ProtectionLocked,
        WalletDustSyncPortError::Unavailable => WalletRealmFamilyView::Unavailable,
        WalletDustSyncPortError::InvalidData => WalletRealmFamilyView::InvalidData,
    }
}

const fn shielded_family_error(
    error: WalletShieldedSyncPortError,
) -> WalletRealmFamilyView<WalletShieldedSyncView> {
    match error {
        WalletShieldedSyncPortError::Conflict => WalletRealmFamilyView::Busy,
        WalletShieldedSyncPortError::UnsupportedNetwork => WalletRealmFamilyView::Unsupported,
        WalletShieldedSyncPortError::ProtectionNotInitialized => {
            WalletRealmFamilyView::ProtectionNotInitialized
        }
        WalletShieldedSyncPortError::ProtectionLocked => WalletRealmFamilyView::ProtectionLocked,
        WalletShieldedSyncPortError::Unavailable => WalletRealmFamilyView::Unavailable,
        WalletShieldedSyncPortError::InvalidData => WalletRealmFamilyView::InvalidData,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc,
        },
        task::{Context, Poll, Waker},
    };

    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{
        ChainKind, ChainNetwork, ChainNetworkId, NetworkDisplayName, NetworkEnvironment,
        WalletAccountSnapshot, WalletDustSyncFailure, WalletDustSyncSnapshot,
        WalletShieldedSyncFailure, WalletShieldedSyncSnapshot,
    };

    use crate::{
        SelectWalletNetworkCommand, SelectWalletNetworkUseCase, WalletNetworkService,
        WalletOperationEffect, WalletOperationRecord,
    };

    use super::*;

    #[derive(Default)]
    struct PartialWallet {
        account_syncs: AtomicUsize,
        dust_starts: AtomicUsize,
        shielded_starts: AtomicUsize,
    }

    impl WalletNetworkPort for PartialWallet {
        fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
            Ok(vec![network()])
        }

        fn selected_network(
            &self,
            _: &WalletProfileId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            Ok(network_id())
        }

        fn select_network(
            &self,
            _: &WalletProfileId,
            network_id: &ChainNetworkId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            Ok(network_id.clone())
        }
    }

    impl WalletAccountReadPort for PartialWallet {
        fn account(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            Ok(WalletAccountSnapshot::unavailable(network()))
        }

        fn sync<'a>(&'a self, _: &'a WalletProfileId) -> crate::WalletAccountPortFuture<'a> {
            self.account_syncs.fetch_add(1, Ordering::Relaxed);
            Box::pin(async { Ok(WalletAccountSnapshot::unavailable(network())) })
        }

        fn account_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            self.account(profile)
        }

        fn sync_in_realm<'a>(
            &'a self,
            profile: &'a WalletProfileId,
            _: &'a ChainNetworkId,
        ) -> crate::WalletAccountPortFuture<'a> {
            self.sync(profile)
        }
    }

    impl WalletDustSyncPort for PartialWallet {
        fn dust_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::ProtectionLocked)
        }

        fn start_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_starts.fetch_add(1, Ordering::Relaxed);
            Err(WalletDustSyncPortError::ProtectionLocked)
        }

        fn cancel_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::ProtectionLocked)
        }

        fn dust_status_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_status(profile)
        }

        fn start_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.start_dust_sync(profile)
        }

        fn cancel_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.cancel_dust_sync(profile)
        }
    }

    impl WalletShieldedSyncPort for PartialWallet {
        fn shielded_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Ok(WalletShieldedSyncSnapshot::never_synced(network_id()))
        }

        fn start_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_starts.fetch_add(1, Ordering::Relaxed);
            Ok(WalletShieldedSyncSnapshot::never_synced(network_id()))
        }

        fn cancel_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Err(WalletShieldedSyncPortError::Unavailable)
        }

        fn shielded_status_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_status(profile)
        }

        fn start_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.start_shielded_sync(profile)
        }

        fn cancel_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.cancel_shielded_sync(profile)
        }
    }

    #[derive(Default)]
    struct PendingAccountWallet {
        account_syncs: AtomicUsize,
    }

    impl WalletNetworkPort for PendingAccountWallet {
        fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
            Ok(vec![network()])
        }

        fn selected_network(
            &self,
            _: &WalletProfileId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            Ok(network_id())
        }

        fn select_network(
            &self,
            _: &WalletProfileId,
            network_id: &ChainNetworkId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            Ok(network_id.clone())
        }
    }

    impl WalletAccountReadPort for PendingAccountWallet {
        fn account(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            Ok(WalletAccountSnapshot::unavailable(network()))
        }

        fn sync<'a>(&'a self, _: &'a WalletProfileId) -> crate::WalletAccountPortFuture<'a> {
            self.account_syncs.fetch_add(1, Ordering::Relaxed);
            Box::pin(async {
                std::future::pending::<()>().await;
                unreachable!("pending fixture never completes")
            })
        }

        fn account_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            self.account(profile)
        }

        fn sync_in_realm<'a>(
            &'a self,
            profile: &'a WalletProfileId,
            _: &'a ChainNetworkId,
        ) -> crate::WalletAccountPortFuture<'a> {
            self.sync(profile)
        }
    }

    impl WalletDustSyncPort for PendingAccountWallet {
        fn dust_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::UnsupportedNetwork)
        }

        fn start_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::UnsupportedNetwork)
        }

        fn cancel_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::UnsupportedNetwork)
        }

        fn dust_status_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_status(profile)
        }

        fn start_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.start_dust_sync(profile)
        }

        fn cancel_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.cancel_dust_sync(profile)
        }
    }

    impl WalletShieldedSyncPort for PendingAccountWallet {
        fn shielded_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Err(WalletShieldedSyncPortError::UnsupportedNetwork)
        }

        fn start_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Err(WalletShieldedSyncPortError::UnsupportedNetwork)
        }

        fn cancel_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Err(WalletShieldedSyncPortError::UnsupportedNetwork)
        }

        fn shielded_status_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_status(profile)
        }

        fn start_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.start_shielded_sync(profile)
        }

        fn cancel_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.cancel_shielded_sync(profile)
        }
    }

    struct CancelDuringAccountWallet {
        account_ready: AtomicBool,
        dust_observation: AtomicUsize,
        dust_starts: AtomicUsize,
        dust_cancels: AtomicUsize,
        shielded_cancels: AtomicUsize,
        shielded_observation: AtomicUsize,
        selected: Mutex<ChainNetworkId>,
        account_realm: Mutex<Option<ChainNetworkId>>,
        dust_realm: Mutex<Option<ChainNetworkId>>,
    }

    impl Default for CancelDuringAccountWallet {
        fn default() -> Self {
            Self {
                account_ready: AtomicBool::new(false),
                dust_observation: AtomicUsize::new(0),
                dust_starts: AtomicUsize::new(0),
                dust_cancels: AtomicUsize::new(0),
                shielded_cancels: AtomicUsize::new(0),
                shielded_observation: AtomicUsize::new(0),
                selected: Mutex::new(network_id()),
                account_realm: Mutex::new(None),
                dust_realm: Mutex::new(None),
            }
        }
    }

    impl CancelDuringAccountWallet {
        fn observed_dust(
            &self,
            realm: ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            match self.dust_observation.load(Ordering::Relaxed) {
                1 => WalletDustSyncSnapshot::new(
                    realm,
                    WalletDustSyncState::Syncing,
                    Some(4),
                    Some(9),
                    3,
                    None,
                    None,
                    None,
                ),
                2 => WalletDustSyncSnapshot::new(
                    realm,
                    WalletDustSyncState::Synced,
                    Some(9),
                    Some(9),
                    8,
                    Some(777),
                    Some(UnixTimestampMillis::new(42)),
                    None,
                ),
                _ => Ok(WalletDustSyncSnapshot::never_synced(realm)),
            }
            .map_err(|_| WalletDustSyncPortError::InvalidData)
        }
    }

    impl CancelDuringAccountWallet {
        fn observed_shielded(
            &self,
            realm: ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            match self.shielded_observation.load(Ordering::Relaxed) {
                2 => WalletShieldedSyncSnapshot::new(
                    realm,
                    WalletShieldedSyncState::Synced,
                    Some(10),
                    Some(10),
                    6,
                    Some(2),
                    Some(4),
                    Vec::new(),
                    Some(UnixTimestampMillis::new(42)),
                    None,
                ),
                _ => Ok(WalletShieldedSyncSnapshot::never_synced(realm)),
            }
            .map_err(|_| WalletShieldedSyncPortError::InvalidData)
        }
    }

    impl WalletNetworkPort for CancelDuringAccountWallet {
        fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
            Ok(vec![network(), preprod_network()])
        }

        fn selected_network(
            &self,
            _: &WalletProfileId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            self.selected
                .lock()
                .map(|selected| selected.clone())
                .map_err(|_| WalletAccountPortError::Unavailable)
        }

        fn select_network(
            &self,
            _: &WalletProfileId,
            network_id: &ChainNetworkId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            *self
                .selected
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)? = network_id.clone();
            Ok(network_id.clone())
        }
    }

    impl WalletAccountReadPort for CancelDuringAccountWallet {
        fn account(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            Ok(WalletAccountSnapshot::unavailable(network()))
        }

        fn sync<'a>(&'a self, _: &'a WalletProfileId) -> crate::WalletAccountPortFuture<'a> {
            Box::pin(std::future::poll_fn(move |_| {
                if self.account_ready.load(Ordering::Relaxed) {
                    Poll::Ready(Ok(WalletAccountSnapshot::unavailable(completed_network())))
                } else {
                    Poll::Pending
                }
            }))
        }

        fn account_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            self.account(profile)
        }

        fn sync_in_realm<'a>(
            &'a self,
            profile: &'a WalletProfileId,
            realm: &'a ChainNetworkId,
        ) -> crate::WalletAccountPortFuture<'a> {
            if let Ok(mut captured) = self.account_realm.lock() {
                *captured = Some(realm.clone());
            }
            self.sync(profile)
        }
    }

    impl WalletDustSyncPort for CancelDuringAccountWallet {
        fn dust_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.observed_dust(network_id())
        }

        fn start_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_starts.fetch_add(1, Ordering::Relaxed);
            WalletDustSyncSnapshot::new(
                network_id(),
                WalletDustSyncState::Syncing,
                None,
                None,
                0,
                None,
                None,
                None,
            )
            .map_err(|_| WalletDustSyncPortError::InvalidData)
        }

        fn cancel_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_cancels.fetch_add(1, Ordering::Relaxed);
            Err(WalletDustSyncPortError::Conflict)
        }

        fn dust_status_in_realm(
            &self,
            _: &WalletProfileId,
            realm: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.observed_dust(realm.clone())
        }

        fn start_dust_sync_in_realm(
            &self,
            _: &WalletProfileId,
            realm: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_starts.fetch_add(1, Ordering::Relaxed);
            *self
                .dust_realm
                .lock()
                .map_err(|_| WalletDustSyncPortError::Unavailable)? = Some(realm.clone());
            let started = WalletDustSyncSnapshot::new(
                realm.clone(),
                WalletDustSyncState::Syncing,
                None,
                None,
                0,
                None,
                None,
                None,
            )
            .map_err(|_| WalletDustSyncPortError::InvalidData)?;
            let _ =
                self.dust_observation
                    .compare_exchange(3, 2, Ordering::Relaxed, Ordering::Relaxed);
            Ok(started)
        }

        fn cancel_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.cancel_dust_sync(profile)
        }
    }

    impl WalletShieldedSyncPort for CancelDuringAccountWallet {
        fn shielded_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.observed_shielded(network_id())
        }

        fn start_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_observation.store(2, Ordering::Relaxed);
            Ok(WalletShieldedSyncSnapshot::never_synced(network_id()))
        }

        fn cancel_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_cancels.fetch_add(1, Ordering::Relaxed);
            Err(WalletShieldedSyncPortError::UnsupportedNetwork)
        }

        fn shielded_status_in_realm(
            &self,
            _: &WalletProfileId,
            realm: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.observed_shielded(realm.clone())
        }

        fn start_shielded_sync_in_realm(
            &self,
            _: &WalletProfileId,
            realm: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_observation.store(2, Ordering::Relaxed);
            Ok(WalletShieldedSyncSnapshot::never_synced(realm.clone()))
        }

        fn cancel_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.cancel_shielded_sync(profile)
        }
    }

    fn network_id() -> ChainNetworkId {
        ChainNetworkId::parse("undeployed").expect("network id is valid")
    }

    fn network() -> ChainNetwork {
        ChainNetwork::new(
            ChainKind::Midnight,
            network_id(),
            NetworkDisplayName::parse("Standalone").expect("network name is valid"),
            NetworkEnvironment::Development,
        )
    }

    fn completed_network() -> ChainNetwork {
        ChainNetwork::new(
            ChainKind::Midnight,
            network_id(),
            NetworkDisplayName::parse("Completed Standalone").expect("network name is valid"),
            NetworkEnvironment::Development,
        )
    }

    fn preprod_network() -> ChainNetwork {
        ChainNetwork::new(
            ChainKind::Midnight,
            ChainNetworkId::parse("preprod").expect("network id is valid"),
            NetworkDisplayName::parse("PreProd").expect("network name is valid"),
            NetworkEnvironment::PublicTest,
        )
    }

    fn command() -> SelectedWalletRealmSyncCommand {
        SelectedWalletRealmSyncCommand {
            profile_id: "profile_test".to_owned(),
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
    fn runtime_isolates_profiles_and_invalidates_previous_realm_leases() {
        let mut runtime = SelectedWalletRealmRuntime::default();
        let stale = WalletRealmReconciliationState {
            account: WalletRealmFacetState::Stale,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Current,
        };
        let profile_a = WalletProfileId::parse("profile_a").expect("profile id");
        let profile_b = WalletProfileId::parse("profile_b").expect("profile id");
        assert!(
            runtime
                .retire_other_realms(&profile_a, &network_id())
                .is_empty()
        );
        let first = runtime.reconcile(
            profile_a.clone(),
            network_id(),
            stale,
            WalletRealmReconciliationTrigger::Initial,
        );
        assert_eq!(first.len(), 1);
        assert!(
            runtime
                .complete(
                    &profile_a,
                    &network_id(),
                    first[0],
                    WalletRealmEffectOutcome::Current,
                )
                .is_empty()
        );
        let current = WalletRealmReconciliationState {
            account: WalletRealmFacetState::Current,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Current,
        };
        let repeated = runtime.reconcile(
            profile_a.clone(),
            network_id(),
            current,
            WalletRealmReconciliationTrigger::ManualRefresh,
        );
        assert_eq!(repeated.len(), 3);
        assert_eq!(
            runtime
                .state(&profile_a, &network_id())
                .expect("same-key state persists")
                .revision(),
            2
        );
        let other = runtime.reconcile(
            profile_b,
            network_id(),
            stale,
            WalletRealmReconciliationTrigger::Initial,
        );
        assert_eq!(other.len(), 1);
        let preprod = ChainNetworkId::parse("preprod").expect("network id");
        assert_eq!(
            runtime.retire_other_realms(&profile_a, &preprod),
            [network_id()]
        );
        let switched = runtime.reconcile(
            profile_a.clone(),
            preprod,
            stale,
            WalletRealmReconciliationTrigger::Initial,
        );
        assert_eq!(switched.len(), 1);
        let retired_revision = runtime
            .state(&profile_a, &network_id())
            .expect("retired generation remains as a tombstone")
            .revision();
        assert_eq!(retired_revision, 3);
        assert!(
            runtime
                .complete(
                    &profile_a,
                    &network_id(),
                    repeated[0],
                    WalletRealmEffectOutcome::Current
                )
                .is_empty()
        );
        assert_eq!(
            runtime
                .state(&profile_a, &network_id())
                .expect("stale completion cannot replace tombstone")
                .revision(),
            retired_revision
        );

        assert_eq!(
            runtime.retire_other_realms(&profile_a, &network_id()),
            [ChainNetworkId::parse("preprod").expect("network id")]
        );
        let returned = runtime.reconcile(
            profile_a.clone(),
            network_id(),
            stale,
            WalletRealmReconciliationTrigger::Initial,
        );
        assert_eq!(returned[0].revision(), 4);
        assert_ne!(returned[0], repeated[0]);
    }

    #[test]
    fn abandoned_service_future_expires_every_owned_lease() {
        let wallet = Arc::new(PendingAccountWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let mut first = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 1);

        let duplicate = resolve(
            service.reconcile(command(), WalletRealmReconciliationTrigger::ActionPreflight),
        )
        .expect("duplicate reconciliation returns its observation");
        assert!(matches!(
            duplicate.view.account,
            WalletRealmFamilyView::Ready(_)
        ));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 1);

        drop(first);
        let timeline = GetWalletOperationTimelineUseCase::execute(&service).expect("timeline");
        assert!(matches!(
            timeline.records().last().map(|record| record.event),
            Some(WalletOperationEvent::Terminal {
                outcome: WalletOperationOutcome::Cancelled,
                failure: Some(WalletOperationFailure::OperationCancelled),
            })
        ));
        let first_operation = timeline.records()[0].operation_id;
        let first_records = timeline
            .records()
            .iter()
            .filter(|record| record.operation_id == first_operation)
            .collect::<Vec<_>>();
        assert_eq!(first_records.len(), 3);
        assert!(matches!(
            first_records[1].event,
            WalletOperationEvent::EffectPlanned(WalletOperationEffect::SyncAccount)
        ));
        assert_eq!(
            first_records[2].caused_by,
            Some(first_records[1].causation_id),
            "drop cancellation must follow the latest successfully recorded event"
        );
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let expired = service
            .runtime
            .lock()
            .expect("runtime lock")
            .state(&profile, &network_id())
            .expect("runtime state");
        assert_eq!(expired.revision(), 2);
        assert_eq!(expired.status(), crate::WalletRealmCoordinatorStatus::Stale);

        let mut restarted = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        assert!(matches!(
            restarted.as_mut().poll(&mut context),
            Poll::Pending
        ));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn dropped_operation_after_completion_uses_the_completion_as_its_cause() {
        let timeline = WalletOperationTimeline::with_capacity(8).expect("timeline");
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let mut runtime = SelectedWalletRealmRuntime::default();
        let effect = runtime
            .reconcile(
                profile.clone(),
                network_id(),
                WalletRealmReconciliationState {
                    account: WalletRealmFacetState::Stale,
                    dust: WalletRealmFacetState::Current,
                    shielded: WalletRealmFacetState::Current,
                },
                WalletRealmReconciliationTrigger::Initial,
            )
            .into_iter()
            .next()
            .expect("account effect");
        let mut operation = SelectedWalletRealmTimelineOperation::begin(
            timeline.clone(),
            profile,
            network_id(),
            effect.revision(),
            WalletRealmReconciliationTrigger::Initial,
        )
        .expect("timeline operation");
        let (planned, attempt) = operation.planned(effect, operation.admission_id);
        let completed = operation.completed(
            effect,
            attempt,
            planned,
            WalletOperationOutcome::Stale,
            None,
            Instant::now(),
        );
        drop(operation);

        let snapshot = timeline.query().expect("timeline snapshot");
        let terminal = snapshot.records().last().expect("terminal record");
        assert!(matches!(
            terminal.event,
            WalletOperationEvent::Terminal {
                outcome: WalletOperationOutcome::Cancelled,
                failure: Some(WalletOperationFailure::OperationCancelled),
            }
        ));
        assert_eq!(terminal.caused_by, Some(completed));
    }

    #[test]
    fn actual_follow_up_retry_keeps_causality_and_increments_effect_attempt() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        wallet.dust_observation.store(3, Ordering::Relaxed);
        wallet.shielded_observation.store(3, Ordering::Relaxed);
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let mut first = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));

        resolve(service.reconcile(command(), WalletRealmReconciliationTrigger::ActionPreflight))
            .expect("follow-up trigger is queued while the first account effect awaits");
        wallet.account_ready.store(true, Ordering::Relaxed);
        assert!(matches!(
            first.as_mut().poll(&mut context),
            Poll::Ready(Ok(_))
        ));

        let timeline = GetWalletOperationTimelineUseCase::execute(&service).expect("timeline");
        let operation_id = timeline
            .records()
            .iter()
            .find(|record| {
                record.trigger == WalletOperationTrigger::Initial
                    && matches!(record.event, WalletOperationEvent::Admitted)
            })
            .expect("initial operation admission")
            .operation_id;
        let records = timeline
            .records()
            .iter()
            .filter(|record| record.operation_id == operation_id)
            .collect::<Vec<_>>();
        let account_records = records
            .iter()
            .copied()
            .filter(|record| {
                matches!(
                    record.event,
                    WalletOperationEvent::EffectPlanned(WalletOperationEffect::SyncAccount)
                        | WalletOperationEvent::EffectCompleted {
                            effect: WalletOperationEffect::SyncAccount,
                            ..
                        }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(account_records.len(), 4);
        assert_eq!(
            account_records
                .iter()
                .map(|record| record.attempt.value())
                .collect::<Vec<_>>(),
            [1, 1, 2, 2]
        );
        assert_eq!(
            account_records
                .iter()
                .map(|record| record.resource.revision)
                .collect::<Vec<_>>(),
            [1, 1, 2, 2],
            "resource revision remains separate from the per-effect attempt"
        );
        assert_eq!(
            account_records[1].caused_by,
            Some(account_records[0].causation_id)
        );
        assert_eq!(
            account_records[3].caused_by,
            Some(account_records[2].causation_id)
        );
        let dust_completion = records
            .iter()
            .copied()
            .find(|record| {
                matches!(
                    record.event,
                    WalletOperationEvent::EffectCompleted {
                        effect: WalletOperationEffect::SyncDust,
                        ..
                    }
                )
            })
            .expect("DUST completion causes the queued retry plan");
        assert_eq!(
            dust_completion.measurements.as_slice(),
            [
                WalletOperationResourceMeasurement::CurrentCursor(9),
                WalletOperationResourceMeasurement::TargetCursor(9),
                WalletOperationResourceMeasurement::EventsProcessed(8),
            ],
            "completion records the refreshed DUST snapshot, not the start snapshot"
        );
        let shielded_completion = records
            .iter()
            .copied()
            .find(|record| {
                matches!(
                    record.event,
                    WalletOperationEvent::EffectCompleted {
                        effect: WalletOperationEffect::SyncShielded,
                        ..
                    }
                )
            })
            .expect("shielded completion is recorded");
        assert_eq!(
            shielded_completion.measurements.as_slice(),
            [
                WalletOperationResourceMeasurement::CurrentCursor(10),
                WalletOperationResourceMeasurement::TargetCursor(10),
                WalletOperationResourceMeasurement::EventsProcessed(6),
                WalletOperationResourceMeasurement::OwnedNoteCount(2),
                WalletOperationResourceMeasurement::CommitmentCount(4),
            ],
            "completion records the refreshed shielded snapshot, not the start snapshot"
        );
        assert_eq!(
            account_records[2].caused_by,
            Some(shielded_completion.causation_id)
        );
        assert!(matches!(
            records.last().expect("terminal").event,
            WalletOperationEvent::Terminal { .. }
        ));
    }

    #[test]
    fn degraded_typed_effect_outcomes_never_produce_terminal_success() {
        for outcome in [
            WalletOperationOutcome::Stale,
            WalletOperationOutcome::Missing,
            WalletOperationOutcome::Blocked,
            WalletOperationOutcome::Unsupported,
            WalletOperationOutcome::Superseded,
        ] {
            let timeline = WalletOperationTimeline::with_capacity(4).expect("timeline");
            let profile = WalletProfileId::parse("profile_test").expect("profile id");
            let mut runtime = SelectedWalletRealmRuntime::default();
            let effect = runtime
                .reconcile(
                    profile.clone(),
                    network_id(),
                    WalletRealmReconciliationState {
                        account: WalletRealmFacetState::Stale,
                        dust: WalletRealmFacetState::Current,
                        shielded: WalletRealmFacetState::Current,
                    },
                    WalletRealmReconciliationTrigger::Initial,
                )
                .into_iter()
                .next()
                .expect("account effect");
            let mut operation = SelectedWalletRealmTimelineOperation::begin(
                timeline.clone(),
                profile,
                network_id(),
                effect.revision(),
                WalletRealmReconciliationTrigger::Initial,
            )
            .expect("timeline operation");
            let (planned, attempt) = operation.planned(effect, operation.admission_id);
            operation.completed(effect, attempt, planned, outcome, None, Instant::now());
            let terminal_outcome = operation.completed_outcome(1);
            operation.terminal(operation.last_cause.get(), terminal_outcome, None);
            drop(operation);

            let expected = if outcome == WalletOperationOutcome::Superseded {
                WalletOperationOutcome::Superseded
            } else {
                WalletOperationOutcome::Failed
            };
            assert_eq!(terminal_outcome, expected);
            assert!(matches!(
                timeline.query().expect("snapshot").records().last(),
                Some(WalletOperationRecord {
                    event: WalletOperationEvent::Terminal {
                        outcome,
                        ..
                    },
                    ..
                }) if *outcome == expected
            ));
        }
    }

    #[test]
    fn cancellation_prevents_queued_family_effects_from_restarting() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let mut reconcile = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(
            reconcile.as_mut().poll(&mut context),
            Poll::Pending
        ));

        CancelSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("cancel invalidates queued coordinator effects");
        wallet.account_ready.store(true, Ordering::Relaxed);
        let Poll::Ready(Ok(projection)) = reconcile.as_mut().poll(&mut context) else {
            panic!("superseded worker returns the newer cancellation projection");
        };
        assert!(matches!(projection.view.dust, WalletRealmFamilyView::Busy));
        assert_eq!(wallet.dust_starts.load(Ordering::Relaxed), 0);
        let timeline = GetWalletOperationTimelineUseCase::execute(&service).expect("timeline");
        assert!(matches!(
            timeline.records().last().map(|record| record.event),
            Some(WalletOperationEvent::Terminal {
                outcome: WalletOperationOutcome::Superseded,
                failure: Some(WalletOperationFailure::ObservationSuperseded),
            })
        ));
    }

    #[test]
    fn cancellation_supersedes_a_read_admitted_before_its_side_effects() {
        let service =
            SelectedWalletRealmSyncService::new(Arc::new(CancelDuringAccountWallet::default()));
        let first = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("initial projection");
        let stale = service
            .begin_observation(&first.identity.profile, &first.identity.realm)
            .expect("read is admitted before cancellation");
        let canceled = CancelSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("cancellation publishes atomically");
        let recovered = service
            .projection(
                first.identity.profile,
                first.identity.realm,
                stale,
                first.view,
            )
            .expect("stale read recovers the cancellation projection");

        assert_eq!(recovered, canceled);
    }

    #[test]
    fn completed_command_rebases_over_a_status_read_started_during_await() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let mut reconcile = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(
            reconcile.as_mut().poll(&mut context),
            Poll::Pending
        ));
        let intermediate = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("status read publishes while account sync awaits");
        assert_eq!(
            intermediate.observation,
            SelectedWalletRealmObservation::PollAfter(Duration::from_millis(150))
        );
        let WalletRealmFamilyView::Ready(intermediate_account) = &intermediate.view.account else {
            panic!("intermediate account is available");
        };
        assert_eq!(intermediate_account.network_name, "Standalone");

        wallet.account_ready.store(true, Ordering::Relaxed);
        let Poll::Ready(Ok(completed)) = reconcile.as_mut().poll(&mut context) else {
            panic!("account effect completes");
        };
        let WalletRealmFamilyView::Ready(completed_account) = &completed.view.account else {
            panic!("completed account is available");
        };
        assert_eq!(completed_account.network_name, "Completed Standalone");
        assert!(completed.revision > intermediate.revision);
        assert!(completed.supersedes(&intermediate));
    }

    #[test]
    fn completed_command_preserves_newer_untouched_facet_observations() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        wallet.dust_observation.store(1, Ordering::Relaxed);
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let mut reconcile = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(
            reconcile.as_mut().poll(&mut context),
            Poll::Pending
        ));

        wallet.dust_observation.store(2, Ordering::Relaxed);
        let intermediate = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("status read publishes the completed DUST observation");
        let WalletRealmFamilyView::Ready(intermediate_dust) = &intermediate.view.dust else {
            panic!("intermediate DUST observation is available");
        };
        assert_eq!(intermediate_dust.state, "synced");

        wallet.account_ready.store(true, Ordering::Relaxed);
        let Poll::Ready(Ok(completed)) = reconcile.as_mut().poll(&mut context) else {
            panic!("account effect completes");
        };
        assert_eq!(completed.view.dust, intermediate.view.dust);
        assert!(completed.revision > intermediate.revision);
        assert_eq!(wallet.dust_starts.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn effect_publication_reobserves_a_worker_that_completed_after_start() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        wallet.dust_observation.store(3, Ordering::Relaxed);
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let mut reconcile = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(
            reconcile.as_mut().poll(&mut context),
            Poll::Pending
        ));

        wallet.account_ready.store(true, Ordering::Relaxed);
        let Poll::Ready(Ok(completed)) = reconcile.as_mut().poll(&mut context) else {
            panic!("account and DUST effects complete");
        };
        let WalletRealmFamilyView::Ready(dust) = completed.view.dust else {
            panic!("completed DUST observation is available");
        };
        assert_eq!(dust.state, "synced");
        assert_eq!(dust.balance_atomic_units.as_deref(), Some("777"));
        assert_eq!(wallet.dust_starts.load(Ordering::Relaxed), 1);
        assert_eq!(
            completed.observation,
            SelectedWalletRealmObservation::Settled
        );
    }

    #[test]
    fn in_flight_reconciliation_rejects_an_unobserved_a_b_a_switch() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        let runtime = Arc::new(Mutex::new(SelectedWalletRealmRuntime::default()));
        let selection_gate = Arc::new(Mutex::new(()));
        let service = Arc::new(
            SelectedWalletRealmSyncService::with_runtime_and_selection_gate(
                Arc::clone(&wallet),
                Arc::clone(&runtime),
                Arc::clone(&selection_gate),
            ),
        );
        let selection_observer: Arc<dyn WalletNetworkSelectionObserver> = service.clone();
        let network_service = WalletNetworkService::with_selection_observer_and_gate(
            Arc::clone(&wallet),
            selection_observer,
            selection_gate,
        );
        let cached = GetSelectedWalletRealmSyncUseCase::execute(service.as_ref(), command())
            .expect("realm A has a published projection before reconciliation");
        let mut reconcile = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(
            reconcile.as_mut().poll(&mut context),
            Poll::Pending
        ));

        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let preprod = ChainNetworkId::parse("preprod").expect("network id");
        SelectWalletNetworkUseCase::execute(
            &network_service,
            SelectWalletNetworkCommand {
                profile_id: profile.as_str().to_owned(),
                network_id: preprod.as_str().to_owned(),
            },
        )
        .expect("selection changes while the old operation is in flight");
        SelectWalletNetworkUseCase::execute(
            &network_service,
            SelectWalletNetworkCommand {
                profile_id: profile.as_str().to_owned(),
                network_id: network_id().as_str().to_owned(),
            },
        )
        .expect("selection returns before the old operation completes");
        wallet.account_ready.store(true, Ordering::Relaxed);
        assert!(matches!(
            reconcile.as_mut().poll(&mut context),
            Poll::Ready(Err(SelectedWalletRealmSyncError::ObservationSuperseded))
        ));
        let timeline = GetWalletOperationTimelineUseCase::execute(service.as_ref())
            .expect("superseded operation remains inspectable");
        assert!(matches!(
            timeline.records().last().map(|record| record.event),
            Some(WalletOperationEvent::Terminal {
                outcome: WalletOperationOutcome::Superseded,
                failure: Some(WalletOperationFailure::ObservationSuperseded),
            })
        ));
        let completion = timeline
            .records()
            .iter()
            .find(|record| {
                matches!(
                    record.event,
                    WalletOperationEvent::EffectCompleted {
                        effect: WalletOperationEffect::SyncAccount,
                        ..
                    }
                )
            })
            .expect("publication error retains the effect completion");
        assert!(completion.measurements.as_slice().is_empty());
        assert_eq!(completion.attempt.value(), 1);
        assert!(completion.caused_by.is_some());

        assert_eq!(
            *wallet.account_realm.lock().expect("account realm lock"),
            Some(network_id())
        );
        assert_eq!(*wallet.dust_realm.lock().expect("DUST realm lock"), None);
        assert_eq!(wallet.dust_starts.load(Ordering::Relaxed), 0);
        assert_eq!(wallet.dust_cancels.load(Ordering::Relaxed), 2);
        assert_eq!(wallet.shielded_cancels.load(Ordering::Relaxed), 2);
        assert!(
            runtime
                .lock()
                .expect("runtime lock")
                .authoritative_projection(&cached.identity.profile, &cached.identity.realm)
                .is_none(),
            "the pre-switch cached projection must not regain authority"
        );
    }

    #[test]
    fn projection_rejects_a_realm_switch_deterministically() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let first = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("initial projection");
        let unchanged = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("unchanged projection");
        assert_eq!(first.revision, unchanged.revision);
        let mut refreshing_view = first.view.clone();
        refreshing_view.dust = WalletRealmFamilyView::Busy;
        let observation_generation = service
            .begin_observation(&first.identity.profile, &first.identity.realm)
            .expect("observation begins");
        let refreshing = service
            .projection(
                first.identity.profile.clone(),
                first.identity.realm.clone(),
                observation_generation,
                refreshing_view,
            )
            .expect("refreshing projection");
        assert_eq!(
            refreshing.observation,
            SelectedWalletRealmObservation::PollAfter(Duration::from_millis(150))
        );
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let preprod = ChainNetworkId::parse("preprod").expect("network id");
        wallet
            .select_network(&profile, &preprod)
            .expect("select replacement realm");
        WalletNetworkSelectionObserver::selected(&service, &profile, &preprod)
            .expect("record replacement realm");
        let replacement = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("replacement projection");
        assert_ne!(first.identity, replacement.identity);
        assert!(!replacement.supersedes(&first));

        wallet
            .select_network(&profile, &network_id())
            .expect("restore original realm");
        WalletNetworkSelectionObserver::selected(&service, &profile, &network_id())
            .expect("record restored realm");
        let restored = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("restored projection");
        assert_eq!(first.identity, restored.identity);
        assert!(restored.revision > first.revision);
        assert!(restored.supersedes(&first));
        assert!(!first.supersedes(&restored));
    }

    #[test]
    fn older_same_realm_observation_cannot_publish_after_a_newer_read() {
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let realm = network_id();
        let mut runtime = SelectedWalletRealmRuntime::default();
        let older = runtime
            .begin_observation(&profile, &realm)
            .expect("first observation begins");
        let newer = runtime
            .begin_observation(&profile, &realm)
            .expect("second observation begins");
        let view = SelectedWalletRealmSyncView {
            account: WalletRealmFamilyView::Unavailable,
            dust: WalletRealmFamilyView::Unavailable,
            shielded: WalletRealmFamilyView::Unavailable,
        };

        assert_eq!(
            runtime.publish(profile.clone(), realm.clone(), newer, &view),
            Some(1)
        );
        assert_eq!(runtime.publish(profile, realm, older, &view), None);
    }

    #[test]
    fn read_cannot_rewrite_a_selection_owned_by_the_mutation_boundary() {
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let standalone = network_id();
        let preprod = ChainNetworkId::parse("preprod").expect("network id");
        let mut runtime = SelectedWalletRealmRuntime::default();

        assert!(runtime.retire_other_realms(&profile, &preprod).is_empty());
        assert_eq!(runtime.begin_observation(&profile, &standalone), None);
        assert!(runtime.selection_matches(&profile, &preprod));
        assert_eq!(
            runtime.retire_other_realms(&profile, &standalone),
            [preprod]
        );
    }

    #[test]
    fn selection_invalidation_never_recovers_a_cached_previous_realm() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet::default()));
        let first = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("initial projection");
        let stale = service
            .begin_observation(&first.identity.profile, &first.identity.realm)
            .expect("observation begins before selection changes");
        service
            .runtime
            .lock()
            .expect("runtime lock")
            .retire_other_realms(
                &first.identity.profile,
                &ChainNetworkId::parse("preprod").expect("network id"),
            );

        assert_eq!(
            service.projection(
                first.identity.profile,
                first.identity.realm,
                stale,
                first.view,
            ),
            Err(SelectedWalletRealmSyncError::SelectionChanged)
        );
    }

    #[test]
    fn selection_mutation_and_projection_publication_share_one_gate() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        let runtime = Arc::new(Mutex::new(SelectedWalletRealmRuntime::default()));
        let selection_gate = Arc::new(Mutex::new(()));
        let service = Arc::new(
            SelectedWalletRealmSyncService::with_runtime_and_selection_gate(
                Arc::clone(&wallet),
                runtime,
                Arc::clone(&selection_gate),
            ),
        );
        let observer: Arc<dyn WalletNetworkSelectionObserver> = service.clone();
        let networks = Arc::new(WalletNetworkService::with_selection_observer_and_gate(
            Arc::clone(&wallet),
            observer,
            Arc::clone(&selection_gate),
        ));
        let first = GetSelectedWalletRealmSyncUseCase::execute(service.as_ref(), command())
            .expect("initial projection");
        let guard = selection_gate.lock().expect("selection gate");
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        std::thread::scope(|scope| {
            scope.spawn(move || {
                started_tx.send(()).expect("announce selection");
                let selected = SelectWalletNetworkUseCase::execute(
                    networks.as_ref(),
                    SelectWalletNetworkCommand {
                        profile_id: "profile_test".to_owned(),
                        network_id: "preprod".to_owned(),
                    },
                );
                done_tx.send(selected).expect("return selection result");
            });
            started_rx.recv().expect("selection starts");
            assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
            assert_eq!(
                wallet
                    .selected_network(&first.identity.profile)
                    .expect("selected network"),
                first.identity.realm
            );
            drop(guard);
            done_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("selection completes after gate release")
                .expect("selection succeeds");
        });

        let selected = GetSelectedWalletRealmSyncUseCase::execute(service.as_ref(), command())
            .expect("replacement projection");
        assert_eq!(selected.identity.realm.as_str(), "preprod");
        assert!(!selected.supersedes(&first));
    }

    #[test]
    fn superseded_action_returns_the_newer_published_projection() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet::default()));
        let first = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("initial projection");
        let older = service
            .begin_observation(&first.identity.profile, &first.identity.realm)
            .expect("action observation begins");
        let newer = service
            .begin_observation(&first.identity.profile, &first.identity.realm)
            .expect("poll observation begins");
        let mut newer_view = first.view.clone();
        newer_view.dust = WalletRealmFamilyView::Busy;
        let published = service
            .projection(
                first.identity.profile.clone(),
                first.identity.realm.clone(),
                newer,
                newer_view.clone(),
            )
            .expect("newer poll publishes");

        let recovered = service
            .projection(
                first.identity.profile,
                first.identity.realm,
                older,
                first.view,
            )
            .expect("superseded action recovers the latest projection");

        assert_eq!(recovered, published);
        assert_eq!(recovered.view, newer_view);
        assert_eq!(
            recovered.observation,
            SelectedWalletRealmObservation::PollAfter(Duration::from_millis(150))
        );
    }

    #[test]
    fn cached_account_snapshot_never_makes_the_realm_fresh() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet::default()));
        let first = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("initial projection");
        let mut view = first.view;
        {
            let WalletRealmFamilyView::Ready(account) = &mut view.account else {
                panic!("account fixture must be present");
            };
            account.sync.state = "synced".to_owned();
            account.source = "cached".to_owned();
        }
        view.dust = WalletRealmFamilyView::Ready(WalletDustSyncView {
            network_id: "undeployed".to_owned(),
            state: "synced".to_owned(),
            current_cursor: Some(1),
            target_cursor: Some(1),
            events_processed: 1,
            balance_atomic_units: Some("1".to_owned()),
            updated_at_millis: Some(1),
            failure: None,
        });
        view.shielded = WalletRealmFamilyView::Ready(WalletShieldedSyncView {
            network_id: "undeployed".to_owned(),
            state: "synced".to_owned(),
            current_cursor: Some(1),
            target_cursor: Some(1),
            events_processed: 1,
            owned_note_count: Some(1),
            commitment_count: Some(1),
            balances: Vec::new(),
            updated_at_millis: Some(1),
            failure: None,
        });

        assert!(!selected_realm_is_fresh(&view));
        if let WalletRealmFamilyView::Ready(account) = &mut view.account {
            account.source = "live".to_owned();
        }
        assert!(selected_realm_is_fresh(&view));

        if let WalletRealmFamilyView::Ready(dust) = &mut view.dust {
            dust.failure = Some("transport_unavailable".to_owned());
        }
        assert!(!selected_realm_is_fresh(&view));
        if let WalletRealmFamilyView::Ready(dust) = &mut view.dust {
            dust.failure = None;
        }
        if let WalletRealmFamilyView::Ready(shielded) = &mut view.shielded {
            shielded.failure = Some("transport_unavailable".to_owned());
        }
        assert!(!selected_realm_is_fresh(&view));
        if let WalletRealmFamilyView::Ready(shielded) = &mut view.shielded {
            shielded.failure = None;
        }
        assert!(selected_realm_is_fresh(&view));
    }

    #[test]
    fn aggregate_preserves_partial_family_failures_as_typed_state() {
        let wallet = Arc::new(PartialWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let started = resolve(SyncSelectedWalletRealmUseCase::execute(&service, command()))
            .expect("aggregate starts");
        assert!(matches!(
            started.view.account,
            WalletRealmFamilyView::Ready(_)
        ));
        assert_eq!(started.view.dust, WalletRealmFamilyView::ProtectionLocked);
        assert!(matches!(
            started.view.shielded,
            WalletRealmFamilyView::Ready(WalletShieldedSyncView { ref state, .. })
                if state == "never_synced"
        ));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 1);
        assert_eq!(wallet.dust_starts.load(Ordering::Relaxed), 1);
        assert_eq!(wallet.shielded_starts.load(Ordering::Relaxed), 1);
        assert_eq!(started.identity.realm, network_id());
        assert!(!started.fresh);
        assert!(started.consistent);
        assert_eq!(
            started.actionable,
            SelectedWalletRealmActionReadiness::Unavailable
        );
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let state = service
            .runtime
            .lock()
            .expect("runtime lock")
            .state(&profile, &network_id())
            .expect("runtime state");
        assert_eq!(
            state.facets(),
            WalletRealmReconciliationState {
                account: WalletRealmFacetState::Stale,
                dust: WalletRealmFacetState::Blocked,
                shielded: WalletRealmFacetState::Missing,
            }
        );

        let timeline = GetWalletOperationTimelineUseCase::execute(&service)
            .expect("shared operation timeline query");
        assert_eq!(timeline.records().len(), 8);
        assert!(matches!(
            timeline.records()[0].event,
            WalletOperationEvent::Admitted
        ));
        assert!(matches!(
            timeline.records()[1].event,
            WalletOperationEvent::EffectPlanned(WalletOperationEffect::SyncAccount)
        ));
        assert!(matches!(
            timeline.records()[2].event,
            WalletOperationEvent::EffectPlanned(WalletOperationEffect::SyncDust)
        ));
        assert!(matches!(
            timeline.records()[3].event,
            WalletOperationEvent::EffectPlanned(WalletOperationEffect::SyncShielded)
        ));
        assert!(matches!(
            timeline.records()[4].event,
            WalletOperationEvent::EffectCompleted {
                effect: WalletOperationEffect::SyncAccount,
                outcome: WalletOperationOutcome::Stale,
                failure: None,
            }
        ));
        assert!(matches!(
            timeline.records()[5].event,
            WalletOperationEvent::EffectCompleted {
                effect: WalletOperationEffect::SyncDust,
                outcome: WalletOperationOutcome::Blocked,
                failure: Some(WalletOperationFailure::ProtectionLocked),
            }
        ));
        assert!(matches!(
            timeline.records()[6].event,
            WalletOperationEvent::EffectCompleted {
                effect: WalletOperationEffect::SyncShielded,
                outcome: WalletOperationOutcome::Missing,
                failure: None,
            }
        ));
        assert!(matches!(
            timeline.records()[7].event,
            WalletOperationEvent::Terminal {
                outcome: WalletOperationOutcome::Failed,
                failure: None,
            }
        ));
        assert!(
            timeline
                .records()
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
        let aggregates = timeline.aggregates();
        assert_eq!(aggregates.maximum_attempt, 1);
        assert_eq!(aggregates.outcomes.stale, 1);
        assert_eq!(aggregates.outcomes.blocked, 1);
        assert_eq!(aggregates.outcomes.missing, 1);
        assert_eq!(aggregates.outcomes.failed, 1);

        let cancelled = CancelSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("aggregate cancellation reports every family");
        assert_eq!(cancelled.view.dust, WalletRealmFamilyView::ProtectionLocked);
        assert_eq!(cancelled.view.shielded, WalletRealmFamilyView::Unavailable);
    }

    #[test]
    fn aggregate_rejects_an_invalid_profile_before_calling_ports() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet::default()));
        assert!(matches!(
            GetSelectedWalletRealmSyncUseCase::execute(
                &service,
                SelectedWalletRealmSyncCommand {
                    profile_id: " invalid".to_owned(),
                },
            ),
            Err(SelectedWalletRealmSyncError::InvalidProfileIdentifier(_))
        ));
    }

    #[test]
    fn resource_measurements_retain_ready_dust_and_shielded_values() {
        let dust_view = SelectedWalletRealmSyncView {
            account: WalletRealmFamilyView::Unavailable,
            dust: WalletRealmFamilyView::Ready(WalletDustSyncView {
                network_id: "undeployed".to_owned(),
                state: "synced".to_owned(),
                current_cursor: Some(4),
                target_cursor: Some(9),
                events_processed: 3,
                balance_atomic_units: None,
                updated_at_millis: None,
                failure: None,
            }),
            shielded: WalletRealmFamilyView::Unavailable,
        };
        assert_eq!(
            resource_measurements(WalletRealmReconciliationEffect::SyncDust, &dust_view).as_slice(),
            [
                WalletOperationResourceMeasurement::CurrentCursor(4),
                WalletOperationResourceMeasurement::TargetCursor(9),
                WalletOperationResourceMeasurement::EventsProcessed(3),
            ]
        );

        let shielded_view = SelectedWalletRealmSyncView {
            account: WalletRealmFamilyView::Unavailable,
            dust: WalletRealmFamilyView::Unavailable,
            shielded: WalletRealmFamilyView::Ready(WalletShieldedSyncView {
                network_id: "undeployed".to_owned(),
                state: "synced".to_owned(),
                current_cursor: None,
                target_cursor: Some(9),
                events_processed: 3,
                owned_note_count: Some(1),
                commitment_count: Some(2),
                balances: Vec::new(),
                updated_at_millis: None,
                failure: None,
            }),
        };
        assert_eq!(
            resource_measurements(
                WalletRealmReconciliationEffect::SyncShielded,
                &shielded_view
            )
            .as_slice(),
            [
                WalletOperationResourceMeasurement::TargetCursor(9),
                WalletOperationResourceMeasurement::EventsProcessed(3),
                WalletOperationResourceMeasurement::OwnedNoteCount(1),
                WalletOperationResourceMeasurement::CommitmentCount(2),
            ]
        );
        assert!(
            resource_measurements(WalletRealmReconciliationEffect::SyncAccount, &shielded_view)
                .as_slice()
                .is_empty()
        );
    }

    #[test]
    fn published_failures_classify_blocked_or_unsupported_reconciliation() {
        let dust = WalletDustSyncSnapshot::new(
            network_id(),
            WalletDustSyncState::Cached,
            Some(4),
            Some(9),
            3,
            Some(42),
            Some(UnixTimestampMillis::new(42)),
            Some(WalletDustSyncFailure::ProtectionLocked),
        )
        .expect("cached DUST fixture is valid");
        let (_, dust_state) = observe_dust(Ok(dust));
        assert_eq!(dust_state, WalletRealmFacetState::Blocked);
        assert_eq!(
            sync_timeline_failure(Some("protection_locked")),
            Some(WalletOperationFailure::ProtectionLocked)
        );

        let shielded = WalletShieldedSyncSnapshot::new(
            network_id(),
            WalletShieldedSyncState::Stalled,
            Some(4),
            Some(9),
            3,
            Some(1),
            Some(2),
            Vec::new(),
            Some(UnixTimestampMillis::new(42)),
            Some(WalletShieldedSyncFailure::UnsupportedNetwork),
        )
        .expect("stalled shielded fixture is valid");
        let (_, shielded_state) = observe_shielded(Ok(shielded));
        assert_eq!(shielded_state, WalletRealmFacetState::Unsupported);
        assert_eq!(
            sync_timeline_failure(Some("unsupported_network")),
            Some(WalletOperationFailure::UnsupportedNetwork)
        );

        let (family, state) = observe_dust(Err(WalletDustSyncPortError::Conflict));
        assert_eq!(family, WalletRealmFamilyView::Busy);
        assert_eq!(state, WalletRealmFacetState::Updating);
        let view = SelectedWalletRealmSyncView {
            account: WalletRealmFamilyView::Unavailable,
            dust: family,
            shielded: WalletRealmFamilyView::Unavailable,
        };
        assert_eq!(
            timeline_effect_failure(
                WalletRealmReconciliationEffect::SyncDust,
                WalletRealmEffectOutcome::InProgress,
                &view,
            ),
            Some(WalletOperationFailure::Conflict)
        );
    }
}
