// SPDX-License-Identifier: Apache-2.0

/// Why selected-realm reconciliation is being considered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRealmReconciliationTrigger {
    Initial,
    ManualRefresh,
    ActionPreflight,
}

/// Application-owned observation of one independently synchronized family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRealmFacetState {
    Current,
    Stale,
    Missing,
    Updating,
    Blocked,
    Unsupported,
}

/// Closed input to the pure selected-realm planner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletRealmReconciliationState {
    pub account: WalletRealmFacetState,
    pub dust: WalletRealmFacetState,
    pub shielded: WalletRealmFacetState,
}

/// Explicit I/O work selected by the pure planner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRealmReconciliationEffect {
    SyncAccount,
    SyncDust,
    SyncShielded,
}

/// Ordered, duplicate-free effects for one reconciliation decision.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalletRealmReconciliationPlan {
    effects: Vec<WalletRealmReconciliationEffect>,
}

impl WalletRealmReconciliationPlan {
    #[must_use]
    pub fn effects(&self) -> &[WalletRealmReconciliationEffect] {
        &self.effects
    }
}

/// Pure policy for selected-realm reconciliation. It owns no clock, task,
/// adapter, UI signal, or retry loop.
#[derive(Clone, Copy, Debug, Default)]
pub struct WalletRealmReconciliationPlanner;

impl WalletRealmReconciliationPlanner {
    #[must_use]
    pub fn plan(
        trigger: WalletRealmReconciliationTrigger,
        state: WalletRealmReconciliationState,
    ) -> WalletRealmReconciliationPlan {
        let mut effects = Vec::with_capacity(3);
        plan_family(
            trigger,
            state.account,
            WalletRealmReconciliationEffect::SyncAccount,
            &mut effects,
        );
        plan_family(
            trigger,
            state.dust,
            WalletRealmReconciliationEffect::SyncDust,
            &mut effects,
        );
        plan_family(
            trigger,
            state.shielded,
            WalletRealmReconciliationEffect::SyncShielded,
            &mut effects,
        );
        WalletRealmReconciliationPlan { effects }
    }
}

/// Product-facing summary derived from the independently observed wallet
/// facets. Connectivity is intentionally not inferred here; the lifecycle
/// shell will add an explicit connectivity observation in a later slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRealmCoordinatorStatus {
    UpToDate,
    Updating,
    Stale,
    ActionRequired,
    Degraded,
}

/// Pure state owned by one selected-realm reconciliation coordinator.
///
/// `revision` is a lease generation. An effect completion is accepted only
/// while its generation is current, so cancellation and later runs cannot be
/// overwritten by stale adapter work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletRealmCoordinatorState {
    revision: u64,
    facets: WalletRealmReconciliationState,
    leases: WalletRealmLeaseSet,
    leased_from: Option<WalletRealmReconciliationState>,
    pending_trigger: Option<WalletRealmReconciliationTrigger>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WalletRealmLeaseSet {
    account: bool,
    dust: bool,
    shielded: bool,
}

impl WalletRealmCoordinatorState {
    #[must_use]
    pub const fn new(facets: WalletRealmReconciliationState) -> Self {
        Self {
            revision: 0,
            facets,
            leases: WalletRealmLeaseSet {
                account: false,
                dust: false,
                shielded: false,
            },
            leased_from: None,
            pending_trigger: None,
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn facets(&self) -> WalletRealmReconciliationState {
        self.facets
    }

    #[must_use]
    pub fn status(&self) -> WalletRealmCoordinatorStatus {
        let states = [self.facets.account, self.facets.dust, self.facets.shielded];
        if states.contains(&WalletRealmFacetState::Updating) {
            WalletRealmCoordinatorStatus::Updating
        } else if states.contains(&WalletRealmFacetState::Blocked) {
            WalletRealmCoordinatorStatus::ActionRequired
        } else if states.iter().any(|state| {
            matches!(
                state,
                WalletRealmFacetState::Stale | WalletRealmFacetState::Missing
            )
        }) {
            WalletRealmCoordinatorStatus::Stale
        } else if states.contains(&WalletRealmFacetState::Unsupported) {
            WalletRealmCoordinatorStatus::Degraded
        } else {
            WalletRealmCoordinatorStatus::UpToDate
        }
    }

    #[must_use]
    pub(crate) const fn accepts(self, effect: WalletRealmCoordinatorEffect) -> bool {
        effect.revision == self.revision && lease_active(self.leases, effect.kind)
    }

    pub(crate) const fn has_active_leases(self) -> bool {
        self.leases.account || self.leases.dust || self.leases.shielded
    }
}

/// One leased effect selected by the pure coordinator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletRealmCoordinatorEffect {
    revision: u64,
    kind: WalletRealmReconciliationEffect,
}

impl WalletRealmCoordinatorEffect {
    const fn new(revision: u64, kind: WalletRealmReconciliationEffect) -> Self {
        Self { revision, kind }
    }

    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn kind(self) -> WalletRealmReconciliationEffect {
        self.kind
    }
}

/// Typed result of one synchronization effect. `InProgress` transfers an
/// admitted lease to an adapter-owned worker without pretending that the
/// facet is already current.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRealmEffectOutcome {
    Current,
    InProgress,
    Stale,
    Missing,
    Blocked,
    Unsupported,
}

impl WalletRealmEffectOutcome {
    const fn facet_state(self) -> WalletRealmFacetState {
        match self {
            Self::Current => WalletRealmFacetState::Current,
            Self::InProgress => WalletRealmFacetState::Updating,
            Self::Stale => WalletRealmFacetState::Stale,
            Self::Missing => WalletRealmFacetState::Missing,
            Self::Blocked => WalletRealmFacetState::Blocked,
            Self::Unsupported => WalletRealmFacetState::Unsupported,
        }
    }
}

/// Closed messages accepted by the functional coordinator core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRealmCoordinatorInput {
    Reconcile(WalletRealmReconciliationTrigger),
    EffectCompleted {
        effect: WalletRealmCoordinatorEffect,
        outcome: WalletRealmEffectOutcome,
    },
    EffectExpired(WalletRealmCoordinatorEffect),
    Observe(WalletRealmReconciliationState),
    Cancel,
}

/// Deterministic result of reducing one coordinator input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRealmCoordinatorTransition {
    state: WalletRealmCoordinatorState,
    effects: Vec<WalletRealmCoordinatorEffect>,
}

impl WalletRealmCoordinatorTransition {
    #[must_use]
    pub const fn state(&self) -> &WalletRealmCoordinatorState {
        &self.state
    }

    #[must_use]
    pub fn effects(&self) -> &[WalletRealmCoordinatorEffect] {
        &self.effects
    }
}

/// Pure, single-flight selected-realm coordinator. The application shell is
/// responsible for storing one state per `(profile, realm)` and executing the
/// returned effects behind ports.
#[derive(Clone, Copy, Debug, Default)]
pub struct WalletRealmReconciliationCoordinator;

impl WalletRealmReconciliationCoordinator {
    #[must_use]
    pub fn reduce(
        state: WalletRealmCoordinatorState,
        input: WalletRealmCoordinatorInput,
    ) -> WalletRealmCoordinatorTransition {
        match input {
            WalletRealmCoordinatorInput::Reconcile(trigger) => reconcile(state, trigger),
            WalletRealmCoordinatorInput::EffectCompleted { effect, outcome } => {
                complete_effect(state, effect, outcome)
            }
            WalletRealmCoordinatorInput::EffectExpired(effect) => expire_effect(state, effect),
            WalletRealmCoordinatorInput::Observe(observed) => observe(state, observed),
            WalletRealmCoordinatorInput::Cancel => cancel(state),
        }
    }
}

fn reconcile(
    state: WalletRealmCoordinatorState,
    trigger: WalletRealmReconciliationTrigger,
) -> WalletRealmCoordinatorTransition {
    if state.has_active_leases() {
        let pending_trigger = match state.pending_trigger {
            Some(pending) if trigger_priority(pending) >= trigger_priority(trigger) => pending,
            _ => trigger,
        };
        return unchanged(WalletRealmCoordinatorState {
            pending_trigger: Some(pending_trigger),
            ..state
        });
    }
    let plan = WalletRealmReconciliationPlanner::plan(trigger, state.facets);
    if plan.effects().is_empty() {
        return unchanged(state);
    }

    let revision = next_revision(state.revision);
    let mut next = WalletRealmCoordinatorState {
        revision,
        facets: state.facets,
        leases: WalletRealmLeaseSet::default(),
        leased_from: Some(state.facets),
        pending_trigger: None,
    };
    let effects = plan
        .effects()
        .iter()
        .copied()
        .map(|kind| {
            *facet_mut(&mut next.facets, kind) = WalletRealmFacetState::Updating;
            *lease_mut(&mut next.leases, kind) = true;
            WalletRealmCoordinatorEffect::new(revision, kind)
        })
        .collect();
    WalletRealmCoordinatorTransition {
        state: next,
        effects,
    }
}

fn complete_effect(
    mut state: WalletRealmCoordinatorState,
    effect: WalletRealmCoordinatorEffect,
    outcome: WalletRealmEffectOutcome,
) -> WalletRealmCoordinatorTransition {
    if effect.revision != state.revision || !lease_active(state.leases, effect.kind) {
        return unchanged(state);
    }
    *facet_mut(&mut state.facets, effect.kind) = outcome.facet_state();
    *lease_mut(&mut state.leases, effect.kind) = false;
    if !state.has_active_leases() {
        state.leased_from = None;
        if let Some(trigger) = state.pending_trigger {
            state.pending_trigger = None;
            return reconcile(state, trigger);
        }
    }
    unchanged(state)
}

fn expire_effect(
    mut state: WalletRealmCoordinatorState,
    effect: WalletRealmCoordinatorEffect,
) -> WalletRealmCoordinatorTransition {
    if effect.revision != state.revision || !lease_active(state.leases, effect.kind) {
        return unchanged(state);
    }
    *facet_mut(&mut state.facets, effect.kind) = WalletRealmFacetState::Stale;
    *lease_mut(&mut state.leases, effect.kind) = false;
    if !state.has_active_leases() {
        state.leased_from = None;
        if let Some(trigger) = state.pending_trigger {
            state.pending_trigger = None;
            return reconcile(state, trigger);
        }
    }
    unchanged(state)
}

fn observe(
    mut state: WalletRealmCoordinatorState,
    observed: WalletRealmReconciliationState,
) -> WalletRealmCoordinatorTransition {
    if !state.leases.account {
        state.facets.account = observed.account;
    }
    if !state.leases.dust {
        state.facets.dust = observed.dust;
    }
    if !state.leases.shielded {
        state.facets.shielded = observed.shielded;
    }
    unchanged(state)
}

fn cancel(mut state: WalletRealmCoordinatorState) -> WalletRealmCoordinatorTransition {
    if !state.has_active_leases() {
        return unchanged(state);
    }
    state.revision = next_revision(state.revision);
    if let Some(previous) = state.leased_from {
        restore_leased_facet(
            &mut state.facets.account,
            state.leases.account,
            previous.account,
        );
        restore_leased_facet(&mut state.facets.dust, state.leases.dust, previous.dust);
        restore_leased_facet(
            &mut state.facets.shielded,
            state.leases.shielded,
            previous.shielded,
        );
    }
    state.leases = WalletRealmLeaseSet::default();
    state.leased_from = None;
    state.pending_trigger = None;
    unchanged(state)
}

fn restore_leased_facet(
    facet: &mut WalletRealmFacetState,
    leased: bool,
    previous: WalletRealmFacetState,
) {
    if leased {
        *facet = previous;
    }
}

const fn next_revision(revision: u64) -> u64 {
    let candidate = revision.wrapping_add(1);
    if candidate == 0 { 1 } else { candidate }
}

fn facet_mut(
    facets: &mut WalletRealmReconciliationState,
    effect: WalletRealmReconciliationEffect,
) -> &mut WalletRealmFacetState {
    match effect {
        WalletRealmReconciliationEffect::SyncAccount => &mut facets.account,
        WalletRealmReconciliationEffect::SyncDust => &mut facets.dust,
        WalletRealmReconciliationEffect::SyncShielded => &mut facets.shielded,
    }
}

const fn lease_active(
    leases: WalletRealmLeaseSet,
    effect: WalletRealmReconciliationEffect,
) -> bool {
    match effect {
        WalletRealmReconciliationEffect::SyncAccount => leases.account,
        WalletRealmReconciliationEffect::SyncDust => leases.dust,
        WalletRealmReconciliationEffect::SyncShielded => leases.shielded,
    }
}

fn lease_mut(
    leases: &mut WalletRealmLeaseSet,
    effect: WalletRealmReconciliationEffect,
) -> &mut bool {
    match effect {
        WalletRealmReconciliationEffect::SyncAccount => &mut leases.account,
        WalletRealmReconciliationEffect::SyncDust => &mut leases.dust,
        WalletRealmReconciliationEffect::SyncShielded => &mut leases.shielded,
    }
}

fn unchanged(state: WalletRealmCoordinatorState) -> WalletRealmCoordinatorTransition {
    WalletRealmCoordinatorTransition {
        state,
        effects: Vec::new(),
    }
}

const fn trigger_priority(trigger: WalletRealmReconciliationTrigger) -> u8 {
    match trigger {
        WalletRealmReconciliationTrigger::Initial => 0,
        WalletRealmReconciliationTrigger::ManualRefresh => 1,
        WalletRealmReconciliationTrigger::ActionPreflight => 2,
    }
}

fn plan_family(
    trigger: WalletRealmReconciliationTrigger,
    state: WalletRealmFacetState,
    effect: WalletRealmReconciliationEffect,
    effects: &mut Vec<WalletRealmReconciliationEffect>,
) {
    let required = match (trigger, state) {
        (
            WalletRealmReconciliationTrigger::ManualRefresh,
            WalletRealmFacetState::Current
            | WalletRealmFacetState::Stale
            | WalletRealmFacetState::Missing
            | WalletRealmFacetState::Blocked,
        )
        | (WalletRealmReconciliationTrigger::ActionPreflight, WalletRealmFacetState::Current)
        | (
            WalletRealmReconciliationTrigger::Initial
            | WalletRealmReconciliationTrigger::ActionPreflight,
            WalletRealmFacetState::Stale | WalletRealmFacetState::Missing,
        ) => true,
        (
            WalletRealmReconciliationTrigger::Initial
            | WalletRealmReconciliationTrigger::ActionPreflight,
            WalletRealmFacetState::Current | WalletRealmFacetState::Blocked,
        )
        | (
            WalletRealmReconciliationTrigger::Initial
            | WalletRealmReconciliationTrigger::ManualRefresh
            | WalletRealmReconciliationTrigger::ActionPreflight,
            WalletRealmFacetState::Updating | WalletRealmFacetState::Unsupported,
        ) => false,
    };
    if required {
        effects.push(effect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_is_deterministic_ordered_and_trigger_aware() {
        let state = WalletRealmReconciliationState {
            account: WalletRealmFacetState::Stale,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Missing,
        };
        let expected = [
            WalletRealmReconciliationEffect::SyncAccount,
            WalletRealmReconciliationEffect::SyncShielded,
        ];
        let first = WalletRealmReconciliationPlanner::plan(
            WalletRealmReconciliationTrigger::Initial,
            state,
        );
        let second = WalletRealmReconciliationPlanner::plan(
            WalletRealmReconciliationTrigger::Initial,
            state,
        );
        assert_eq!(first, second);
        assert_eq!(first.effects(), expected);

        let manual = WalletRealmReconciliationPlanner::plan(
            WalletRealmReconciliationTrigger::ManualRefresh,
            state,
        );
        assert_eq!(
            manual.effects(),
            [
                WalletRealmReconciliationEffect::SyncAccount,
                WalletRealmReconciliationEffect::SyncDust,
                WalletRealmReconciliationEffect::SyncShielded,
            ]
        );

        let preflight = WalletRealmReconciliationPlanner::plan(
            WalletRealmReconciliationTrigger::ActionPreflight,
            state,
        );
        assert_eq!(
            preflight.effects(),
            [
                WalletRealmReconciliationEffect::SyncAccount,
                WalletRealmReconciliationEffect::SyncDust,
                WalletRealmReconciliationEffect::SyncShielded,
            ]
        );
    }

    #[test]
    fn planner_never_duplicates_active_or_unsupported_work() {
        for trigger in [
            WalletRealmReconciliationTrigger::Initial,
            WalletRealmReconciliationTrigger::ManualRefresh,
            WalletRealmReconciliationTrigger::ActionPreflight,
        ] {
            let plan = WalletRealmReconciliationPlanner::plan(
                trigger,
                WalletRealmReconciliationState {
                    account: WalletRealmFacetState::Updating,
                    dust: WalletRealmFacetState::Updating,
                    shielded: WalletRealmFacetState::Unsupported,
                },
            );
            assert!(plan.effects().is_empty());
        }
    }

    #[test]
    fn planner_retries_blocked_work_only_after_explicit_manual_refresh() {
        let state = WalletRealmReconciliationState {
            account: WalletRealmFacetState::Blocked,
            dust: WalletRealmFacetState::Blocked,
            shielded: WalletRealmFacetState::Blocked,
        };
        for trigger in [
            WalletRealmReconciliationTrigger::Initial,
            WalletRealmReconciliationTrigger::ActionPreflight,
        ] {
            assert!(
                WalletRealmReconciliationPlanner::plan(trigger, state)
                    .effects()
                    .is_empty()
            );
        }
        assert_eq!(
            WalletRealmReconciliationPlanner::plan(
                WalletRealmReconciliationTrigger::ManualRefresh,
                state,
            )
            .effects(),
            [
                WalletRealmReconciliationEffect::SyncAccount,
                WalletRealmReconciliationEffect::SyncDust,
                WalletRealmReconciliationEffect::SyncShielded,
            ]
        );
    }

    #[test]
    fn coordinator_owns_one_revisioned_pipeline_and_ignores_stale_completions() {
        let state = WalletRealmCoordinatorState::new(WalletRealmReconciliationState {
            account: WalletRealmFacetState::Stale,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Missing,
        });

        let started = WalletRealmReconciliationCoordinator::reduce(
            state,
            WalletRealmCoordinatorInput::Reconcile(WalletRealmReconciliationTrigger::Initial),
        );
        assert_eq!(started.state().revision(), 1);
        assert_eq!(
            started.state().status(),
            WalletRealmCoordinatorStatus::Updating
        );
        assert_eq!(
            started.effects(),
            [
                WalletRealmCoordinatorEffect::new(1, WalletRealmReconciliationEffect::SyncAccount,),
                WalletRealmCoordinatorEffect::new(1, WalletRealmReconciliationEffect::SyncShielded,),
            ]
        );

        let duplicate = WalletRealmReconciliationCoordinator::reduce(
            *started.state(),
            WalletRealmCoordinatorInput::Reconcile(
                WalletRealmReconciliationTrigger::ActionPreflight,
            ),
        );
        assert_eq!(duplicate.state().revision(), started.state().revision());
        assert_eq!(duplicate.state().facets(), started.state().facets());
        assert_eq!(duplicate.state().leases, started.state().leases);
        assert_eq!(
            duplicate.state().pending_trigger,
            Some(WalletRealmReconciliationTrigger::ActionPreflight)
        );
        assert!(duplicate.effects().is_empty());

        let stale_completion = WalletRealmReconciliationCoordinator::reduce(
            *started.state(),
            WalletRealmCoordinatorInput::EffectCompleted {
                effect: WalletRealmCoordinatorEffect::new(
                    0,
                    WalletRealmReconciliationEffect::SyncAccount,
                ),
                outcome: WalletRealmEffectOutcome::Current,
            },
        );
        assert_eq!(stale_completion.state(), started.state());

        let account_complete = WalletRealmReconciliationCoordinator::reduce(
            *started.state(),
            WalletRealmCoordinatorInput::EffectCompleted {
                effect: started.effects()[0],
                outcome: WalletRealmEffectOutcome::Current,
            },
        );
        let complete = WalletRealmReconciliationCoordinator::reduce(
            *account_complete.state(),
            WalletRealmCoordinatorInput::EffectCompleted {
                effect: started.effects()[1],
                outcome: WalletRealmEffectOutcome::Current,
            },
        );
        assert_eq!(
            complete.state().status(),
            WalletRealmCoordinatorStatus::UpToDate
        );
        assert!(complete.effects().is_empty());
    }

    #[test]
    fn coordinator_merges_observations_and_runs_the_highest_priority_pending_trigger() {
        let original = WalletRealmReconciliationState {
            account: WalletRealmFacetState::Stale,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Current,
        };
        let started = WalletRealmReconciliationCoordinator::reduce(
            WalletRealmCoordinatorState::new(original),
            WalletRealmCoordinatorInput::Reconcile(WalletRealmReconciliationTrigger::Initial),
        );
        assert_eq!(started.effects().len(), 1);

        let observed = WalletRealmReconciliationCoordinator::reduce(
            *started.state(),
            WalletRealmCoordinatorInput::Observe(WalletRealmReconciliationState {
                account: WalletRealmFacetState::Current,
                dust: WalletRealmFacetState::Stale,
                shielded: WalletRealmFacetState::Current,
            }),
        );
        assert_eq!(
            observed.state().facets().account,
            WalletRealmFacetState::Updating
        );
        assert_eq!(observed.state().facets().dust, WalletRealmFacetState::Stale);

        let pending_manual = WalletRealmReconciliationCoordinator::reduce(
            *observed.state(),
            WalletRealmCoordinatorInput::Reconcile(WalletRealmReconciliationTrigger::ManualRefresh),
        );
        let pending_preflight = WalletRealmReconciliationCoordinator::reduce(
            *pending_manual.state(),
            WalletRealmCoordinatorInput::Reconcile(
                WalletRealmReconciliationTrigger::ActionPreflight,
            ),
        );
        let recovered = WalletRealmReconciliationCoordinator::reduce(
            *pending_preflight.state(),
            WalletRealmCoordinatorInput::EffectExpired(started.effects()[0]),
        );

        assert_eq!(recovered.state().revision(), 2);
        assert_eq!(
            recovered.effects(),
            [
                WalletRealmCoordinatorEffect::new(2, WalletRealmReconciliationEffect::SyncAccount,),
                WalletRealmCoordinatorEffect::new(2, WalletRealmReconciliationEffect::SyncDust,),
            ]
        );
    }

    #[test]
    fn coordinator_requires_explicit_recovery_and_cancel_invalidates_leases() {
        let blocked = WalletRealmCoordinatorState::new(WalletRealmReconciliationState {
            account: WalletRealmFacetState::Blocked,
            dust: WalletRealmFacetState::Blocked,
            shielded: WalletRealmFacetState::Unsupported,
        });
        assert_eq!(
            blocked.status(),
            WalletRealmCoordinatorStatus::ActionRequired
        );

        let automatic = WalletRealmReconciliationCoordinator::reduce(
            blocked,
            WalletRealmCoordinatorInput::Reconcile(WalletRealmReconciliationTrigger::Initial),
        );
        assert!(automatic.effects().is_empty());

        let manual = WalletRealmReconciliationCoordinator::reduce(
            blocked,
            WalletRealmCoordinatorInput::Reconcile(WalletRealmReconciliationTrigger::ManualRefresh),
        );
        assert_eq!(manual.effects().len(), 2);
        let canceled = WalletRealmReconciliationCoordinator::reduce(
            *manual.state(),
            WalletRealmCoordinatorInput::Cancel,
        );
        assert_eq!(canceled.state().revision(), 2);
        assert_eq!(
            canceled.state().status(),
            WalletRealmCoordinatorStatus::ActionRequired
        );

        let late = WalletRealmReconciliationCoordinator::reduce(
            *canceled.state(),
            WalletRealmCoordinatorInput::EffectCompleted {
                effect: manual.effects()[0],
                outcome: WalletRealmEffectOutcome::Current,
            },
        );
        assert_eq!(late.state(), canceled.state());
    }

    #[test]
    fn coordinator_reports_partial_capability_and_restores_pre_lease_state() {
        let partial = WalletRealmCoordinatorState::new(WalletRealmReconciliationState {
            account: WalletRealmFacetState::Current,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Unsupported,
        });
        assert_eq!(partial.status(), WalletRealmCoordinatorStatus::Degraded);

        let original = WalletRealmReconciliationState {
            account: WalletRealmFacetState::Stale,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Missing,
        };
        let started = WalletRealmReconciliationCoordinator::reduce(
            WalletRealmCoordinatorState::new(original),
            WalletRealmCoordinatorInput::Reconcile(WalletRealmReconciliationTrigger::Initial),
        );
        let canceled = WalletRealmReconciliationCoordinator::reduce(
            *started.state(),
            WalletRealmCoordinatorInput::Cancel,
        );
        assert_eq!(canceled.state().facets(), original);
    }

    #[test]
    fn coordinator_transfers_a_lease_to_an_adapter_owned_worker() {
        let started = WalletRealmReconciliationCoordinator::reduce(
            WalletRealmCoordinatorState::new(WalletRealmReconciliationState {
                account: WalletRealmFacetState::Current,
                dust: WalletRealmFacetState::Missing,
                shielded: WalletRealmFacetState::Unsupported,
            }),
            WalletRealmCoordinatorInput::Reconcile(WalletRealmReconciliationTrigger::Initial),
        );
        let transferred = WalletRealmReconciliationCoordinator::reduce(
            *started.state(),
            WalletRealmCoordinatorInput::EffectCompleted {
                effect: started.effects()[0],
                outcome: WalletRealmEffectOutcome::InProgress,
            },
        );

        assert!(transferred.effects().is_empty());
        assert_eq!(
            transferred.state().facets().dust,
            WalletRealmFacetState::Updating
        );
        assert!(!transferred.state().accepts(started.effects()[0]));
    }

    #[test]
    fn coordinator_does_not_own_or_starve_externally_active_facets() {
        let observed = WalletRealmReconciliationState {
            account: WalletRealmFacetState::Missing,
            dust: WalletRealmFacetState::Updating,
            shielded: WalletRealmFacetState::Missing,
        };
        let state = WalletRealmCoordinatorState::new(observed);
        assert_eq!(state.status(), WalletRealmCoordinatorStatus::Updating);

        let canceled = WalletRealmReconciliationCoordinator::reduce(
            state,
            WalletRealmCoordinatorInput::Cancel,
        );
        assert_eq!(canceled.state().facets(), observed);

        let started = WalletRealmReconciliationCoordinator::reduce(
            state,
            WalletRealmCoordinatorInput::Reconcile(WalletRealmReconciliationTrigger::Initial),
        );
        assert_eq!(
            started.effects(),
            [
                WalletRealmCoordinatorEffect::new(1, WalletRealmReconciliationEffect::SyncAccount,),
                WalletRealmCoordinatorEffect::new(1, WalletRealmReconciliationEffect::SyncShielded,),
            ]
        );
        let canceled = WalletRealmReconciliationCoordinator::reduce(
            *started.state(),
            WalletRealmCoordinatorInput::Cancel,
        );
        assert_eq!(canceled.state().facets(), observed);
    }

    #[test]
    fn updating_status_precedes_action_required_until_leased_work_finishes() {
        let state = WalletRealmCoordinatorState::new(WalletRealmReconciliationState {
            account: WalletRealmFacetState::Updating,
            dust: WalletRealmFacetState::Blocked,
            shielded: WalletRealmFacetState::Current,
        });
        assert_eq!(state.status(), WalletRealmCoordinatorStatus::Updating);
    }
}
