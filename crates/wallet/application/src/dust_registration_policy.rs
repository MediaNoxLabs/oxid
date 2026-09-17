// SPDX-License-Identifier: Apache-2.0

//! Pure, presentation-neutral DUST registration settlement policy.
//!
//! This projection owns no adapter, scheduler, signing, proof, or transaction
//! payload. An adapter may translate its observations into these events, but it
//! cannot advance a newer selected-realm generation with stale input.

use oxid_wallet_domain::{
    ChainNetworkId, ChainTransactionId, WalletProfileId, WalletTransactionDraftId,
};

/// Identity of one selected-realm generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationSettlementIdentity {
    pub profile: WalletProfileId,
    pub realm: ChainNetworkId,
    pub generation: u64,
}

/// Retained public identities after preparation and submission respectively.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationSettlementRegistration {
    pub draft_id: WalletTransactionDraftId,
    pub transaction_id: Option<ChainTransactionId>,
    /// Maximum retained finality or reconciliation observation sequence.
    pub observation_revision: u64,
    pub finality_revision: u64,
    pub reconciliation_revision: u64,
    /// Monotonic DUST snapshot sequence for this registration.
    pub dust_revision: u64,
    pub dust_observation_revision: u64,
    pub dust_ready: bool,
    pub included: bool,
}

/// Last consistent selected-realm observation retained through recoverable failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationSettlementCheckpoint {
    pub identity: WalletDustRegistrationSettlementIdentity,
    pub revision: u64,
    pub eligible: bool,
}

/// Public reconciliation result for one exact registration transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationSettlementReconciliation {
    Pending,
    Included,
    Dropped,
}

/// Public states that a headless or Dioxus client may render without owning policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationSettlementState {
    Unavailable,
    NotEligible,
    ActionRequired,
    AwaitingAuthorization,
    Submitting,
    Confirming,
    Reconciling,
    Ready,
    Cancelled,
    Offline,
    TimedOut,
    Degraded,
    Suspended,
}

use WalletDustRegistrationSettlementState as State;

/// One complete presentation-neutral settlement projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationSettlementProjection {
    pub state: WalletDustRegistrationSettlementState,
    pub identity: Option<WalletDustRegistrationSettlementIdentity>,
    pub registration: Option<WalletDustRegistrationSettlementRegistration>,
    pub checkpoint: Option<WalletDustRegistrationSettlementCheckpoint>,
    pub preparation_revision: u64,
    /// Monotonic sequence shared by every recoverable-status event.
    pub recovery_revision: u64,
    resume_state: Option<WalletDustRegistrationSettlementState>,
    pending_retry_revision: Option<u64>,
}

impl Default for WalletDustRegistrationSettlementProjection {
    fn default() -> Self {
        Self {
            state: WalletDustRegistrationSettlementState::Unavailable,
            identity: None,
            registration: None,
            checkpoint: None,
            preparation_revision: 0,
            recovery_revision: 0,
            resume_state: None,
            pending_retry_revision: None,
        }
    }
}

/// Typed observations accepted by the pure reducer.
///
/// Events contain neither unrestricted adapter errors nor custody material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationSettlementEvent {
    Eligibility {
        identity: WalletDustRegistrationSettlementIdentity,
        revision: u64,
        eligible: bool,
    },
    AuthorizationRequested {
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
        preparation_revision: u64,
    },
    AuthorizationSucceeded {
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
    },
    AuthorizationRejected {
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
    },
    SubmissionAccepted {
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
        transaction_id: ChainTransactionId,
    },
    FinalityObserved {
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
        revision: u64,
    },
    RegistrationReconciled {
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
        revision: u64,
        reconciliation: WalletDustRegistrationSettlementReconciliation,
    },
    DustRefreshed {
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
        revision: u64,
        after_observation_revision: u64,
        ready: bool,
    },
    DroppedRegistrationAbandoned {
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
        after_observation_revision: u64,
    },
    Cancelled {
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
    },
    Offline {
        identity: WalletDustRegistrationSettlementIdentity,
        revision: u64,
    },
    TimedOut {
        identity: WalletDustRegistrationSettlementIdentity,
        revision: u64,
    },
    Degraded {
        identity: WalletDustRegistrationSettlementIdentity,
        revision: u64,
    },
    Suspended {
        identity: WalletDustRegistrationSettlementIdentity,
        revision: u64,
    },
    Resumed {
        identity: WalletDustRegistrationSettlementIdentity,
        revision: u64,
    },
    Retry {
        identity: WalletDustRegistrationSettlementIdentity,
        revision: u64,
    },
    /// Retire this projection after its selected identity is superseded.
    ///
    /// The outgoing identity remains as an unavailable tombstone so delayed
    /// observations cannot resurrect it. A strictly newer selection starts a
    /// replacement projection with an `Eligibility` event.
    Superseded {
        identity: WalletDustRegistrationSettlementIdentity,
    },
}

/// Reduce one event. Duplicate, out-of-order, and stale events are no-ops.
#[must_use]
pub fn reduce_wallet_dust_registration_settlement(
    projection: &WalletDustRegistrationSettlementProjection,
    event: WalletDustRegistrationSettlementEvent,
) -> WalletDustRegistrationSettlementProjection {
    let identity = event_identity(&event);
    let replaces_generation = match &projection.identity {
        Some(active) if identity == active => false,
        Some(active)
            if matches!(
                event,
                WalletDustRegistrationSettlementEvent::Eligibility { .. }
            ) && identity.generation > active.generation =>
        {
            true
        }
        Some(_) => return projection.clone(),
        None if !matches!(
            event,
            WalletDustRegistrationSettlementEvent::Eligibility { .. }
        ) =>
        {
            return projection.clone();
        }
        None => false,
    };
    if !replaces_generation
        && matches!(
            projection.state,
            WalletDustRegistrationSettlementState::Cancelled
        )
        && !matches!(
            event,
            WalletDustRegistrationSettlementEvent::Superseded { .. }
        )
    {
        return projection.clone();
    }
    if !replaces_generation
        && projection.identity.is_some()
        && matches!(
            projection.state,
            WalletDustRegistrationSettlementState::Unavailable
        )
    {
        return projection.clone();
    }

    let mut next = if replaces_generation {
        WalletDustRegistrationSettlementProjection::default()
    } else {
        projection.clone()
    };
    let settles_pending_retry = recovery_revision(&event).is_some()
        && !matches!(event, WalletDustRegistrationSettlementEvent::Retry { .. });
    if let Some(revision) = recovery_revision(&event) {
        if revision <= projection.recovery_revision {
            return projection.clone();
        }
        if matches!(event, WalletDustRegistrationSettlementEvent::Retry { .. })
            && !can_retry(projection.state)
        {
            if projection
                .pending_retry_revision
                .is_some_and(|pending| revision <= pending)
            {
                return projection.clone();
            }
            next.pending_retry_revision = Some(revision);
            return next;
        }
        next.recovery_revision = revision;
    }
    match event {
        WalletDustRegistrationSettlementEvent::Eligibility {
            identity,
            revision,
            eligible,
        } => {
            if next.identity.is_none() {
                next.identity = Some(identity.clone());
                next.registration = None;
                next.checkpoint = Some(WalletDustRegistrationSettlementCheckpoint {
                    identity,
                    revision,
                    eligible,
                });
                set_state(
                    &mut next,
                    if eligible {
                        WalletDustRegistrationSettlementState::ActionRequired
                    } else {
                        WalletDustRegistrationSettlementState::NotEligible
                    },
                );
            } else {
                let current_revision = projection
                    .checkpoint
                    .as_ref()
                    .map_or(0, |checkpoint| checkpoint.revision);
                if revision <= current_revision {
                    return projection.clone();
                }
                next.checkpoint = Some(WalletDustRegistrationSettlementCheckpoint {
                    identity,
                    revision,
                    eligible,
                });
                apply_eligibility(&mut next, eligible);
            }
        }
        WalletDustRegistrationSettlementEvent::AuthorizationRequested {
            draft_id,
            preparation_revision,
            ..
        } if matches!(
            effective_state(projection),
            WalletDustRegistrationSettlementState::ActionRequired
        ) && !has_submitted_registration(projection)
            && preparation_revision > projection.preparation_revision =>
        {
            next.preparation_revision = preparation_revision;
            next.registration = Some(WalletDustRegistrationSettlementRegistration {
                draft_id,
                transaction_id: None,
                observation_revision: 0,
                finality_revision: 0,
                reconciliation_revision: 0,
                dust_revision: 0,
                dust_observation_revision: 0,
                dust_ready: false,
                included: false,
            });
            set_effective_state(
                &mut next,
                WalletDustRegistrationSettlementState::AwaitingAuthorization,
            );
        }
        WalletDustRegistrationSettlementEvent::AuthorizationRequested {
            draft_id,
            preparation_revision,
            ..
        } if matches!(
            effective_state(projection),
            WalletDustRegistrationSettlementState::AwaitingAuthorization
        ) && !has_draft(projection, &draft_id)
            && matches!(
                latest_eligibility_state(projection),
                WalletDustRegistrationSettlementState::ActionRequired
            )
            && preparation_revision > projection.preparation_revision =>
        {
            next.preparation_revision = preparation_revision;
            next.registration = Some(WalletDustRegistrationSettlementRegistration {
                draft_id,
                transaction_id: None,
                observation_revision: 0,
                finality_revision: 0,
                reconciliation_revision: 0,
                dust_revision: 0,
                dust_observation_revision: 0,
                dust_ready: false,
                included: false,
            });
        }
        WalletDustRegistrationSettlementEvent::AuthorizationSucceeded { draft_id, .. }
            if matches!(
                effective_state(projection),
                WalletDustRegistrationSettlementState::AwaitingAuthorization
            ) && has_draft(projection, &draft_id)
                && matches!(
                    latest_eligibility_state(projection),
                    WalletDustRegistrationSettlementState::NotEligible
                ) =>
        {
            next.registration = None;
            set_effective_state(
                &mut next,
                WalletDustRegistrationSettlementState::NotEligible,
            );
        }
        WalletDustRegistrationSettlementEvent::AuthorizationSucceeded { draft_id, .. }
            if matches!(
                effective_state(projection),
                WalletDustRegistrationSettlementState::AwaitingAuthorization
            ) && has_draft(projection, &draft_id) =>
        {
            set_effective_state(&mut next, WalletDustRegistrationSettlementState::Submitting);
        }
        WalletDustRegistrationSettlementEvent::AuthorizationRejected { draft_id, .. }
            if matches!(
                effective_state(projection),
                WalletDustRegistrationSettlementState::AwaitingAuthorization
            ) && has_draft(projection, &draft_id) =>
        {
            next.registration = None;
            set_effective_state(&mut next, latest_eligibility_state(projection));
        }
        WalletDustRegistrationSettlementEvent::SubmissionAccepted {
            draft_id,
            transaction_id,
            ..
        } if matches!(
            effective_state(projection),
            WalletDustRegistrationSettlementState::Submitting
        ) && has_draft(projection, &draft_id) =>
        {
            if let Some(registration) = &mut next.registration {
                registration.transaction_id = Some(transaction_id);
            }
            set_effective_state(&mut next, WalletDustRegistrationSettlementState::Confirming);
        }
        WalletDustRegistrationSettlementEvent::FinalityObserved {
            transaction_id,
            revision,
            ..
        } if matches!(
            effective_state(projection),
            State::NotEligible
                | State::ActionRequired
                | State::Confirming
                | State::Reconciling
                | State::Ready
        ) && has_transaction(projection, &transaction_id)
            && projection
                .registration
                .as_ref()
                .is_some_and(|registration| {
                    revision > registration.finality_revision
                        && (!matches!(
                            effective_state(projection),
                            State::NotEligible | State::ActionRequired
                        ) || revision > registration.reconciliation_revision)
                }) =>
        {
            let current_state = effective_state(projection);
            if let Some(registration) = &mut next.registration {
                registration.finality_revision = revision;
                registration.observation_revision = registration.observation_revision.max(revision);
            }
            let finality_is_current = next.registration.as_ref().is_some_and(|registration| {
                registration.finality_revision >= registration.reconciliation_revision
            });
            if registration_ready(&next) {
                set_effective_state(&mut next, WalletDustRegistrationSettlementState::Ready);
            } else if matches!(current_state, WalletDustRegistrationSettlementState::Ready)
                || (matches!(
                    current_state,
                    State::NotEligible | State::ActionRequired | State::Confirming
                ) && finality_is_current)
            {
                set_effective_state(
                    &mut next,
                    WalletDustRegistrationSettlementState::Reconciling,
                );
            }
        }
        WalletDustRegistrationSettlementEvent::RegistrationReconciled {
            transaction_id,
            revision,
            reconciliation,
            ..
        } if matches!(
            effective_state(projection),
            State::NotEligible
                | State::ActionRequired
                | State::Confirming
                | State::Reconciling
                | State::Ready
        ) && has_transaction(projection, &transaction_id)
            && projection
                .registration
                .as_ref()
                .is_some_and(|registration| {
                    revision > registration.reconciliation_revision
                        && (reconciliation
                            != WalletDustRegistrationSettlementReconciliation::Dropped
                            || revision >= registration.finality_revision)
                }) =>
        {
            if let Some(registration) = &mut next.registration {
                registration.reconciliation_revision = revision;
                registration.observation_revision = registration.observation_revision.max(revision);
            }
            match reconciliation {
                WalletDustRegistrationSettlementReconciliation::Included => {
                    if let Some(registration) = &mut next.registration {
                        registration.included = true;
                    }
                    let state = if registration_ready(&next) {
                        WalletDustRegistrationSettlementState::Ready
                    } else {
                        WalletDustRegistrationSettlementState::Reconciling
                    };
                    set_effective_state(&mut next, state);
                }
                WalletDustRegistrationSettlementReconciliation::Pending => {
                    if let Some(registration) = &mut next.registration {
                        registration.included = false;
                    }
                    let state = if next.registration.as_ref().is_some_and(|registration| {
                        registration.finality_revision >= registration.reconciliation_revision
                    }) {
                        WalletDustRegistrationSettlementState::Reconciling
                    } else {
                        WalletDustRegistrationSettlementState::Confirming
                    };
                    set_effective_state(&mut next, state);
                }
                WalletDustRegistrationSettlementReconciliation::Dropped => {
                    if let Some(registration) = &mut next.registration {
                        registration.included = false;
                    }
                    set_effective_state(&mut next, latest_eligibility_state(projection));
                }
            }
        }
        WalletDustRegistrationSettlementEvent::DustRefreshed {
            transaction_id,
            revision,
            after_observation_revision,
            ready,
            ..
        } if matches!(
            effective_state(projection),
            State::NotEligible
                | State::ActionRequired
                | State::Confirming
                | State::Reconciling
                | State::Ready
        ) && has_transaction(projection, &transaction_id)
            && projection
                .registration
                .as_ref()
                .is_some_and(|registration| revision > registration.dust_revision) =>
        {
            let current_state = effective_state(projection);
            if let Some(registration) = &mut next.registration {
                registration.dust_revision = revision;
                registration.dust_observation_revision = after_observation_revision;
                registration.dust_ready = ready;
            }
            if registration_ready(&next) {
                set_effective_state(&mut next, WalletDustRegistrationSettlementState::Ready);
            } else if matches!(current_state, WalletDustRegistrationSettlementState::Ready) {
                set_effective_state(
                    &mut next,
                    WalletDustRegistrationSettlementState::Reconciling,
                );
            }
        }
        WalletDustRegistrationSettlementEvent::DroppedRegistrationAbandoned {
            transaction_id,
            after_observation_revision,
            ..
        } if projection.state == State::ActionRequired
            && has_transaction(projection, &transaction_id)
            && projection
                .registration
                .as_ref()
                .is_some_and(|registration| {
                    registration.observation_revision == after_observation_revision
                        && registration.dust_observation_revision <= after_observation_revision
                }) =>
        {
            next.registration = None;
        }
        WalletDustRegistrationSettlementEvent::Cancelled { draft_id, .. }
            if can_cancel(projection) && has_draft(projection, &draft_id) =>
        {
            next.registration = None;
            set_state(&mut next, WalletDustRegistrationSettlementState::Cancelled);
        }
        WalletDustRegistrationSettlementEvent::Offline { .. } => enter_recoverable_state(
            &mut next,
            projection,
            WalletDustRegistrationSettlementState::Offline,
        ),
        WalletDustRegistrationSettlementEvent::TimedOut { .. } => enter_recoverable_state(
            &mut next,
            projection,
            WalletDustRegistrationSettlementState::TimedOut,
        ),
        WalletDustRegistrationSettlementEvent::Degraded { .. } => enter_recoverable_state(
            &mut next,
            projection,
            WalletDustRegistrationSettlementState::Degraded,
        ),
        WalletDustRegistrationSettlementEvent::Suspended { .. } => enter_recoverable_state(
            &mut next,
            projection,
            WalletDustRegistrationSettlementState::Suspended,
        ),
        WalletDustRegistrationSettlementEvent::Resumed { .. }
            if is_recoverable_state(projection.state) =>
        {
            resume_recoverable_state(&mut next);
        }
        WalletDustRegistrationSettlementEvent::Retry { .. } if can_retry(projection.state) => {
            resume_recoverable_state(&mut next);
        }
        WalletDustRegistrationSettlementEvent::Resumed { .. }
        | WalletDustRegistrationSettlementEvent::Retry { .. } => {}
        WalletDustRegistrationSettlementEvent::Superseded { .. } => {
            next.registration = None;
            set_state(
                &mut next,
                WalletDustRegistrationSettlementState::Unavailable,
            );
        }
        _ => return projection.clone(),
    }
    if settles_pending_retry {
        settle_pending_retry(&mut next);
    }
    next
}

fn set_state(
    projection: &mut WalletDustRegistrationSettlementProjection,
    state: WalletDustRegistrationSettlementState,
) {
    projection.state = state;
    projection.resume_state = None;
}

fn effective_state(
    projection: &WalletDustRegistrationSettlementProjection,
) -> WalletDustRegistrationSettlementState {
    if is_recoverable_state(projection.state) {
        projection.resume_state.unwrap_or(projection.state)
    } else {
        projection.state
    }
}

fn set_effective_state(
    projection: &mut WalletDustRegistrationSettlementProjection,
    state: WalletDustRegistrationSettlementState,
) {
    if is_recoverable_state(projection.state) {
        projection.resume_state = Some(state);
    } else {
        set_state(projection, state);
    }
}

fn apply_eligibility(projection: &mut WalletDustRegistrationSettlementProjection, eligible: bool) {
    let state = if is_recoverable_state(projection.state) {
        projection.resume_state.as_mut()
    } else {
        Some(&mut projection.state)
    };
    match state {
        Some(state @ WalletDustRegistrationSettlementState::NotEligible) if eligible => {
            *state = WalletDustRegistrationSettlementState::ActionRequired;
        }
        Some(state @ WalletDustRegistrationSettlementState::ActionRequired) if !eligible => {
            *state = WalletDustRegistrationSettlementState::NotEligible;
        }
        _ => {}
    }
}

fn latest_eligibility_state(
    projection: &WalletDustRegistrationSettlementProjection,
) -> WalletDustRegistrationSettlementState {
    if projection
        .checkpoint
        .as_ref()
        .is_some_and(|checkpoint| !checkpoint.eligible)
    {
        WalletDustRegistrationSettlementState::NotEligible
    } else {
        WalletDustRegistrationSettlementState::ActionRequired
    }
}

fn is_recoverable_state(state: WalletDustRegistrationSettlementState) -> bool {
    matches!(
        state,
        WalletDustRegistrationSettlementState::Offline
            | WalletDustRegistrationSettlementState::TimedOut
            | WalletDustRegistrationSettlementState::Degraded
            | WalletDustRegistrationSettlementState::Suspended
    )
}

fn can_cancel(projection: &WalletDustRegistrationSettlementProjection) -> bool {
    fn is_pre_submission(state: WalletDustRegistrationSettlementState) -> bool {
        matches!(
            state,
            WalletDustRegistrationSettlementState::ActionRequired
                | WalletDustRegistrationSettlementState::AwaitingAuthorization
        )
    }

    (is_pre_submission(projection.state)
        || (is_recoverable_state(projection.state)
            && projection.resume_state.is_some_and(is_pre_submission)))
        && !has_submitted_registration(projection)
}

fn enter_recoverable_state(
    next: &mut WalletDustRegistrationSettlementProjection,
    current: &WalletDustRegistrationSettlementProjection,
    state: WalletDustRegistrationSettlementState,
) {
    if current.resume_state.is_none() {
        next.resume_state = Some(current.state);
    }
    next.state = state;
}

fn resume_recoverable_state(projection: &mut WalletDustRegistrationSettlementProjection) {
    let fallback = latest_eligibility_state(projection);
    let retained = projection.resume_state.take().unwrap_or(fallback);
    projection.state = match retained {
        WalletDustRegistrationSettlementState::Ready => {
            WalletDustRegistrationSettlementState::Ready
        }
        WalletDustRegistrationSettlementState::NotEligible => {
            WalletDustRegistrationSettlementState::NotEligible
        }
        WalletDustRegistrationSettlementState::Submitting
            if projection
                .registration
                .as_ref()
                .is_some_and(|registration| registration.transaction_id.is_none()) =>
        {
            WalletDustRegistrationSettlementState::Submitting
        }
        WalletDustRegistrationSettlementState::AwaitingAuthorization
            if projection
                .registration
                .as_ref()
                .is_some_and(|registration| registration.transaction_id.is_none()) =>
        {
            WalletDustRegistrationSettlementState::AwaitingAuthorization
        }
        WalletDustRegistrationSettlementState::Confirming
        | WalletDustRegistrationSettlementState::Reconciling
            if projection
                .registration
                .as_ref()
                .and_then(|registration| registration.transaction_id.as_ref())
                .is_some() =>
        {
            if projection
                .registration
                .as_ref()
                .is_some_and(|registration| {
                    registration.included
                        || (registration.finality_revision > 0
                            && registration.finality_revision
                                >= registration.reconciliation_revision)
                })
            {
                WalletDustRegistrationSettlementState::Reconciling
            } else {
                WalletDustRegistrationSettlementState::Confirming
            }
        }
        _ => fallback,
    };
    let transaction_retained = has_submitted_registration(projection);
    if matches!(projection.state, State::NotEligible | State::ActionRequired)
        && !transaction_retained
    {
        projection.registration = None;
    }
}

fn event_identity(
    event: &WalletDustRegistrationSettlementEvent,
) -> &WalletDustRegistrationSettlementIdentity {
    match event {
        WalletDustRegistrationSettlementEvent::Eligibility { identity, .. }
        | WalletDustRegistrationSettlementEvent::AuthorizationRequested { identity, .. }
        | WalletDustRegistrationSettlementEvent::AuthorizationSucceeded { identity, .. }
        | WalletDustRegistrationSettlementEvent::AuthorizationRejected { identity, .. }
        | WalletDustRegistrationSettlementEvent::SubmissionAccepted { identity, .. }
        | WalletDustRegistrationSettlementEvent::FinalityObserved { identity, .. }
        | WalletDustRegistrationSettlementEvent::RegistrationReconciled { identity, .. }
        | WalletDustRegistrationSettlementEvent::DustRefreshed { identity, .. }
        | WalletDustRegistrationSettlementEvent::DroppedRegistrationAbandoned {
            identity, ..
        }
        | WalletDustRegistrationSettlementEvent::Cancelled { identity, .. }
        | WalletDustRegistrationSettlementEvent::Offline { identity, .. }
        | WalletDustRegistrationSettlementEvent::TimedOut { identity, .. }
        | WalletDustRegistrationSettlementEvent::Degraded { identity, .. }
        | WalletDustRegistrationSettlementEvent::Suspended { identity, .. }
        | WalletDustRegistrationSettlementEvent::Resumed { identity, .. }
        | WalletDustRegistrationSettlementEvent::Retry { identity, .. }
        | WalletDustRegistrationSettlementEvent::Superseded { identity } => identity,
    }
}

fn recovery_revision(event: &WalletDustRegistrationSettlementEvent) -> Option<u64> {
    match event {
        WalletDustRegistrationSettlementEvent::Offline { revision, .. }
        | WalletDustRegistrationSettlementEvent::TimedOut { revision, .. }
        | WalletDustRegistrationSettlementEvent::Degraded { revision, .. }
        | WalletDustRegistrationSettlementEvent::Suspended { revision, .. }
        | WalletDustRegistrationSettlementEvent::Resumed { revision, .. }
        | WalletDustRegistrationSettlementEvent::Retry { revision, .. } => Some(*revision),
        _ => None,
    }
}

fn can_retry(state: WalletDustRegistrationSettlementState) -> bool {
    matches!(
        state,
        WalletDustRegistrationSettlementState::Offline
            | WalletDustRegistrationSettlementState::TimedOut
            | WalletDustRegistrationSettlementState::Degraded
            | WalletDustRegistrationSettlementState::Suspended
    )
}

fn settle_pending_retry(projection: &mut WalletDustRegistrationSettlementProjection) {
    let Some(revision) = projection.pending_retry_revision else {
        return;
    };
    if revision <= projection.recovery_revision {
        projection.pending_retry_revision = None;
        return;
    }
    if can_retry(projection.state) {
        projection.pending_retry_revision = None;
        projection.recovery_revision = revision;
        resume_recoverable_state(projection);
    }
}

fn has_draft(
    projection: &WalletDustRegistrationSettlementProjection,
    draft: &WalletTransactionDraftId,
) -> bool {
    projection
        .registration
        .as_ref()
        .is_some_and(|registration| registration.draft_id == *draft)
}

fn has_transaction(
    projection: &WalletDustRegistrationSettlementProjection,
    transaction: &ChainTransactionId,
) -> bool {
    projection
        .registration
        .as_ref()
        .and_then(|registration| registration.transaction_id.as_ref())
        .is_some_and(|current| current == transaction)
}

fn has_submitted_registration(projection: &WalletDustRegistrationSettlementProjection) -> bool {
    projection
        .registration
        .as_ref()
        .is_some_and(|registration| registration.transaction_id.is_some())
}

fn registration_ready(projection: &WalletDustRegistrationSettlementProjection) -> bool {
    projection
        .registration
        .as_ref()
        .is_some_and(|registration| {
            registration.included
                && registration.dust_ready
                && registration.dust_observation_revision == registration.observation_revision
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(
        profile: &str,
        realm: &str,
        generation: u64,
    ) -> WalletDustRegistrationSettlementIdentity {
        WalletDustRegistrationSettlementIdentity {
            profile: WalletProfileId::parse(profile).unwrap(),
            realm: ChainNetworkId::parse(realm).unwrap(),
            generation,
        }
    }

    fn selected_identity() -> WalletDustRegistrationSettlementIdentity {
        identity("profile_test", "undeployed", 1)
    }

    fn draft() -> WalletTransactionDraftId {
        WalletTransactionDraftId::parse("dustreg_test").unwrap()
    }

    fn transaction() -> ChainTransactionId {
        ChainTransactionId::parse("tx_test").unwrap()
    }

    fn other_draft() -> WalletTransactionDraftId {
        WalletTransactionDraftId::parse("dustreg_other").unwrap()
    }

    fn other_transaction() -> ChainTransactionId {
        ChainTransactionId::parse("tx_other").unwrap()
    }

    fn finality(revision: u64) -> WalletDustRegistrationSettlementEvent {
        WalletDustRegistrationSettlementEvent::FinalityObserved {
            identity: selected_identity(),
            transaction_id: transaction(),
            revision,
        }
    }

    fn reconciliation(
        revision: u64,
        reconciliation: WalletDustRegistrationSettlementReconciliation,
    ) -> WalletDustRegistrationSettlementEvent {
        WalletDustRegistrationSettlementEvent::RegistrationReconciled {
            identity: selected_identity(),
            transaction_id: transaction(),
            revision,
            reconciliation,
        }
    }

    fn dust_refresh(
        revision: u64,
        after_observation_revision: u64,
        ready: bool,
    ) -> WalletDustRegistrationSettlementEvent {
        WalletDustRegistrationSettlementEvent::DustRefreshed {
            identity: selected_identity(),
            transaction_id: transaction(),
            revision,
            after_observation_revision,
            ready,
        }
    }

    fn abandon_dropped(after_observation_revision: u64) -> WalletDustRegistrationSettlementEvent {
        WalletDustRegistrationSettlementEvent::DroppedRegistrationAbandoned {
            identity: selected_identity(),
            transaction_id: transaction(),
            after_observation_revision,
        }
    }

    fn authorization_request(
        draft_id: WalletTransactionDraftId,
        preparation_revision: u64,
    ) -> WalletDustRegistrationSettlementEvent {
        WalletDustRegistrationSettlementEvent::AuthorizationRequested {
            identity: selected_identity(),
            draft_id,
            preparation_revision,
        }
    }

    fn cancellation(draft_id: WalletTransactionDraftId) -> WalletDustRegistrationSettlementEvent {
        WalletDustRegistrationSettlementEvent::Cancelled {
            identity: selected_identity(),
            draft_id,
        }
    }

    fn eligibility(revision: u64, eligible: bool) -> WalletDustRegistrationSettlementEvent {
        WalletDustRegistrationSettlementEvent::Eligibility {
            identity: selected_identity(),
            revision,
            eligible,
        }
    }

    fn offline_event(revision: u64) -> WalletDustRegistrationSettlementEvent {
        WalletDustRegistrationSettlementEvent::Offline {
            identity: selected_identity(),
            revision,
        }
    }

    fn resumed_event(revision: u64) -> WalletDustRegistrationSettlementEvent {
        WalletDustRegistrationSettlementEvent::Resumed {
            identity: selected_identity(),
            revision,
        }
    }

    fn reduce(
        projection: &WalletDustRegistrationSettlementProjection,
        event: WalletDustRegistrationSettlementEvent,
    ) -> WalletDustRegistrationSettlementProjection {
        reduce_wallet_dust_registration_settlement(projection, event)
    }

    fn retained(
        projection: &WalletDustRegistrationSettlementProjection,
    ) -> &WalletDustRegistrationSettlementRegistration {
        projection.registration.as_ref().unwrap()
    }

    fn eligible() -> WalletDustRegistrationSettlementProjection {
        reduce(&Default::default(), eligibility(1, true))
    }

    fn submitting() -> WalletDustRegistrationSettlementProjection {
        let identity = selected_identity();
        let awaiting = reduce(&eligible(), authorization_request(draft(), 1));
        reduce(
            &awaiting,
            WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                identity,
                draft_id: draft(),
            },
        )
    }

    fn confirming() -> WalletDustRegistrationSettlementProjection {
        reduce(
            &submitting(),
            WalletDustRegistrationSettlementEvent::SubmissionAccepted {
                identity: selected_identity(),
                draft_id: draft(),
                transaction_id: transaction(),
            },
        )
    }

    fn reconciling() -> WalletDustRegistrationSettlementProjection {
        reduce(&confirming(), finality(1))
    }

    fn ready() -> WalletDustRegistrationSettlementProjection {
        let included = reduce(
            &reconciling(),
            reconciliation(2, WalletDustRegistrationSettlementReconciliation::Included),
        );
        reduce(&included, dust_refresh(1, 2, true))
    }

    #[test]
    fn eligible_flow_requires_authorization_finality_reconciliation_and_dust_refresh() {
        let identity = selected_identity();
        let mut projection = eligible();
        assert_eq!(projection.state, State::ActionRequired);
        assert_eq!(
            reduce(
                &projection,
                WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                    identity: identity.clone(),
                    draft_id: draft(),
                },
            ),
            projection
        );
        projection = reduce(&projection, authorization_request(draft(), 1));
        projection = reduce(
            &projection,
            WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                identity: identity.clone(),
                draft_id: draft(),
            },
        );
        assert_eq!(projection.state, State::Submitting);
        projection = reduce(
            &projection,
            WalletDustRegistrationSettlementEvent::SubmissionAccepted {
                identity: identity.clone(),
                draft_id: draft(),
                transaction_id: transaction(),
            },
        );
        projection = reduce(&projection, finality(1));
        assert_eq!(projection.state, State::Reconciling);
        projection = reduce(&projection, dust_refresh(1, 1, true));
        assert_eq!(projection.state, State::Reconciling);
        projection = reduce(
            &projection,
            reconciliation(2, WalletDustRegistrationSettlementReconciliation::Included),
        );
        projection = reduce(&projection, dust_refresh(2, 2, true));
        assert_eq!(projection.state, State::Ready);
    }

    #[test]
    fn eligibility_revisions_are_monotonic_and_do_not_start_effects() {
        let mut projection = reduce(&Default::default(), eligibility(1, false));
        assert_eq!(projection.state, State::NotEligible);
        projection = reduce(&projection, eligibility(2, true));
        assert_eq!(projection.state, State::ActionRequired);
        assert!(projection.registration.is_none());
        assert_eq!(reduce(&projection, eligibility(1, false)), projection);

        let awaiting = reduce(&eligible(), authorization_request(draft(), 1));
        let became_ineligible = reduce(&awaiting, eligibility(2, false));
        assert_eq!(became_ineligible.state, State::AwaitingAuthorization);
        assert!(
            became_ineligible
                .checkpoint
                .as_ref()
                .is_some_and(|checkpoint| !checkpoint.eligible)
        );
        let rejected = reduce(
            &became_ineligible,
            WalletDustRegistrationSettlementEvent::AuthorizationRejected {
                identity: selected_identity(),
                draft_id: draft(),
            },
        );
        assert_eq!(rejected.state, State::NotEligible);
        assert!(rejected.registration.is_none());

        let stale_success = reduce(
            &became_ineligible,
            WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                identity: selected_identity(),
                draft_id: draft(),
            },
        );
        assert_eq!(stale_success.state, State::NotEligible);
        assert!(stale_success.registration.is_none());
    }

    #[test]
    fn recoverable_failures_resume_only_from_durable_public_status() {
        let identity = selected_identity();
        let submitting = submitting();
        let offline = reduce(&submitting, offline_event(1));
        assert_eq!(offline.checkpoint, submitting.checkpoint);
        assert_eq!(
            reduce(
                &offline,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: identity.clone(),
                    revision: 2,
                },
            )
            .state,
            State::Submitting
        );
        assert!(
            reduce(
                &offline,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: identity.clone(),
                    revision: 2,
                },
            )
            .registration
            .is_some()
        );

        let confirming = confirming();
        let timed_out = reduce(
            &confirming,
            WalletDustRegistrationSettlementEvent::TimedOut {
                identity: identity.clone(),
                revision: 1,
            },
        );
        assert_eq!(
            reduce(
                &timed_out,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: identity.clone(),
                    revision: 2,
                },
            )
            .state,
            WalletDustRegistrationSettlementState::Confirming
        );
        assert_eq!(
            reduce(&timed_out, resumed_event(2)).state,
            WalletDustRegistrationSettlementState::Confirming
        );

        let offline = reduce(&confirming, offline_event(1));
        let finality_while_offline = reduce(&offline, finality(2));
        let pending_while_offline = reduce(
            &finality_while_offline,
            reconciliation(1, WalletDustRegistrationSettlementReconciliation::Pending),
        );
        assert_eq!(pending_while_offline.state, State::Offline);
        assert_eq!(
            reduce(
                &pending_while_offline,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: identity.clone(),
                    revision: 2,
                },
            )
            .state,
            State::Reconciling
        );

        let included = reduce(
            &reconciling(),
            reconciliation(2, WalletDustRegistrationSettlementReconciliation::Included),
        );
        let degraded = reduce(
            &included,
            WalletDustRegistrationSettlementEvent::Degraded {
                identity: identity.clone(),
                revision: 1,
            },
        );
        assert_eq!(
            reduce(
                &degraded,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: identity.clone(),
                    revision: 2,
                },
            )
            .state,
            State::Reconciling
        );
        let degraded_confirming = reduce(
            &confirming,
            WalletDustRegistrationSettlementEvent::Degraded {
                identity: identity.clone(),
                revision: 1,
            },
        );
        let included_while_degraded = reduce(
            &degraded_confirming,
            reconciliation(1, WalletDustRegistrationSettlementReconciliation::Included),
        );
        assert_eq!(included_while_degraded.state, State::Degraded);
        assert_eq!(
            reduce(
                &included_while_degraded,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: identity.clone(),
                    revision: 2,
                },
            )
            .state,
            WalletDustRegistrationSettlementState::Reconciling
        );
        let dropped_while_degraded = reduce(
            &degraded_confirming,
            reconciliation(1, WalletDustRegistrationSettlementReconciliation::Dropped),
        );
        assert_eq!(dropped_while_degraded.state, State::Degraded);
        let retried = reduce(
            &dropped_while_degraded,
            WalletDustRegistrationSettlementEvent::Retry {
                identity: identity.clone(),
                revision: 2,
            },
        );
        assert_eq!(retried.state, State::ActionRequired);
        assert!(has_transaction(&retried, &transaction()));
        let ready = reduce(&included, dust_refresh(1, 2, true));
        let suspended = reduce(
            &ready,
            WalletDustRegistrationSettlementEvent::Suspended {
                identity: identity.clone(),
                revision: 1,
            },
        );
        assert_eq!(
            reduce(
                &suspended,
                WalletDustRegistrationSettlementEvent::Resumed {
                    identity: identity.clone(),
                    revision: 2,
                },
            )
            .state,
            State::Ready
        );
        let retried_suspension = reduce(
            &suspended,
            WalletDustRegistrationSettlementEvent::Retry {
                identity: identity.clone(),
                revision: 3,
            },
        );
        assert_eq!(retried_suspension.state, State::Ready);
        assert_eq!(retried_suspension.recovery_revision, 3);
        assert_eq!(
            reduce(
                &retried_suspension,
                WalletDustRegistrationSettlementEvent::Resumed {
                    identity: identity.clone(),
                    revision: 2,
                },
            )
            .state,
            State::Ready
        );
        let early_retry = reduce(
            &ready,
            WalletDustRegistrationSettlementEvent::Retry {
                identity: identity.clone(),
                revision: 5,
            },
        );
        let early_resume = reduce(
            &early_retry,
            WalletDustRegistrationSettlementEvent::Resumed {
                identity: identity.clone(),
                revision: 2,
            },
        );
        let delayed_offline = reduce(
            &early_resume,
            WalletDustRegistrationSettlementEvent::Offline {
                identity: identity.clone(),
                revision: 3,
            },
        );
        let delayed_suspension = reduce(
            &delayed_offline,
            WalletDustRegistrationSettlementEvent::Suspended {
                identity: identity.clone(),
                revision: 4,
            },
        );
        assert_eq!(delayed_suspension.state, State::Ready);
        assert_eq!(delayed_suspension.recovery_revision, 5);
        assert_eq!(
            reduce(
                &ready,
                WalletDustRegistrationSettlementEvent::Cancelled {
                    identity,
                    draft_id: draft(),
                },
            ),
            ready
        );
    }

    #[test]
    fn accepted_submission_is_retained_while_the_visible_state_is_recoverable() {
        let identity = selected_identity();
        let offline = reduce(
            &submitting(),
            WalletDustRegistrationSettlementEvent::Offline {
                identity: identity.clone(),
                revision: 1,
            },
        );

        let accepted = reduce(
            &offline,
            WalletDustRegistrationSettlementEvent::SubmissionAccepted {
                identity: identity.clone(),
                draft_id: draft(),
                transaction_id: transaction(),
            },
        );

        assert_eq!(accepted.state, State::Offline);
        assert_eq!(accepted.resume_state, Some(State::Confirming));
        assert!(has_transaction(&accepted, &transaction()));

        let retried = reduce(
            &accepted,
            WalletDustRegistrationSettlementEvent::Retry {
                identity,
                revision: 2,
            },
        );
        assert_eq!(retried.state, State::Confirming);
        assert!(has_transaction(&retried, &transaction()));

        let retried_before_acceptance = reduce(
            &offline,
            WalletDustRegistrationSettlementEvent::Retry {
                identity: selected_identity(),
                revision: 2,
            },
        );
        assert_eq!(retried_before_acceptance.state, State::Submitting);
        let accepted_after_retry = reduce(
            &retried_before_acceptance,
            WalletDustRegistrationSettlementEvent::SubmissionAccepted {
                identity: selected_identity(),
                draft_id: draft(),
                transaction_id: transaction(),
            },
        );
        assert_eq!(accepted_after_retry.state, State::Confirming);
        assert!(has_transaction(&accepted_after_retry, &transaction()));
    }

    #[test]
    fn cancellation_and_realm_supersession_are_terminal_and_generation_safe() {
        let selected = selected_identity();
        let other = identity("profile_other", "preprod", 2);
        let projection = eligible();
        let awaiting = reduce(&projection, authorization_request(draft(), 1));
        assert_eq!(
            reduce(
                &projection,
                WalletDustRegistrationSettlementEvent::Offline {
                    identity: other.clone(),
                    revision: 1,
                },
            ),
            projection
        );
        let cancelled = reduce(&awaiting, cancellation(draft()));
        assert_eq!(
            reduce(
                &cancelled,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: selected.clone(),
                    revision: 1,
                },
            ),
            cancelled
        );
        let awaiting_cancelled = reduce(&awaiting, cancellation(draft()));
        assert_eq!(awaiting_cancelled.state, State::Cancelled);
        assert!(awaiting_cancelled.registration.is_none());
        let awaiting_offline = reduce(
            &awaiting,
            WalletDustRegistrationSettlementEvent::Offline {
                identity: selected.clone(),
                revision: 1,
            },
        );
        assert_eq!(
            reduce(&awaiting_offline, cancellation(draft())).state,
            State::Cancelled
        );
        let submitting = submitting();
        assert_eq!(reduce(&submitting, cancellation(draft())), submitting);
        assert_eq!(
            reduce(
                &cancelled,
                WalletDustRegistrationSettlementEvent::Offline {
                    identity: selected.clone(),
                    revision: 1,
                },
            ),
            cancelled
        );
        let superseded = reduce(
            &projection,
            WalletDustRegistrationSettlementEvent::Superseded {
                identity: selected.clone(),
            },
        );
        assert_eq!(superseded.state, State::Unavailable);
        assert_eq!(superseded.identity, Some(selected.clone()));
        assert!(superseded.registration.is_none());
        let delayed_offline = reduce(
            &superseded,
            WalletDustRegistrationSettlementEvent::Offline {
                identity: selected.clone(),
                revision: 2,
            },
        );
        assert_eq!(delayed_offline, superseded);
        assert_eq!(
            reduce(
                &delayed_offline,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: selected.clone(),
                    revision: 3,
                },
            ),
            superseded
        );
        assert_eq!(
            reduce(
                &superseded,
                WalletDustRegistrationSettlementEvent::Eligibility {
                    identity: selected,
                    revision: 2,
                    eligible: true,
                },
            )
            .state,
            State::Unavailable
        );
        let replacement = reduce(
            &superseded,
            WalletDustRegistrationSettlementEvent::Eligibility {
                identity: other.clone(),
                revision: 1,
                eligible: true,
            },
        );
        assert_eq!(replacement.identity, Some(other));
        assert_eq!(replacement.state, State::ActionRequired);
    }

    #[test]
    fn recoverable_status_revisions_reject_delayed_state_changes() {
        let identity = selected_identity();
        let projection = eligible();
        let early_resume = reduce(
            &projection,
            WalletDustRegistrationSettlementEvent::Resumed {
                identity: identity.clone(),
                revision: 2,
            },
        );
        assert_eq!(early_resume.state, State::ActionRequired);
        assert_eq!(early_resume.recovery_revision, 2);
        assert_eq!(
            reduce(
                &early_resume,
                WalletDustRegistrationSettlementEvent::Suspended {
                    identity: identity.clone(),
                    revision: 1,
                },
            ),
            early_resume
        );

        let suspended = reduce(
            &early_resume,
            WalletDustRegistrationSettlementEvent::Suspended {
                identity: identity.clone(),
                revision: 3,
            },
        );
        assert_eq!(suspended.state, State::Suspended);
        assert_eq!(
            reduce(
                &suspended,
                WalletDustRegistrationSettlementEvent::Resumed {
                    identity,
                    revision: 4,
                },
            )
            .state,
            State::ActionRequired
        );
    }

    #[test]
    fn authorization_results_are_retained_across_recoverable_states() {
        let identity = selected_identity();
        let offline = reduce(
            &eligible(),
            WalletDustRegistrationSettlementEvent::Offline {
                identity: identity.clone(),
                revision: 1,
            },
        );
        let awaiting = reduce(
            &offline,
            WalletDustRegistrationSettlementEvent::AuthorizationRequested {
                identity: identity.clone(),
                draft_id: draft(),
                preparation_revision: 1,
            },
        );
        assert_eq!(awaiting.state, State::Offline);
        assert_eq!(awaiting.resume_state, Some(State::AwaitingAuthorization));
        let authorized = reduce(
            &awaiting,
            WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                identity: identity.clone(),
                draft_id: draft(),
            },
        );
        assert_eq!(authorized.resume_state, Some(State::Submitting));
        assert_eq!(
            reduce(
                &authorized,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: identity.clone(),
                    revision: 2,
                },
            )
            .state,
            State::Submitting
        );

        let retried = reduce(
            &awaiting,
            WalletDustRegistrationSettlementEvent::Retry {
                identity: identity.clone(),
                revision: 2,
            },
        );
        assert_eq!(retried.state, State::AwaitingAuthorization);
        assert_eq!(
            reduce(
                &retried,
                WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                    identity,
                    draft_id: draft(),
                },
            )
            .state,
            State::Submitting
        );
    }

    #[test]
    fn newer_generation_and_recoverable_eligibility_supersede_stale_state() {
        let selected = selected_identity();
        let not_eligible = reduce(
            &Default::default(),
            WalletDustRegistrationSettlementEvent::Eligibility {
                identity: selected.clone(),
                revision: 1,
                eligible: false,
            },
        );
        let offline = reduce(
            &not_eligible,
            WalletDustRegistrationSettlementEvent::Offline {
                identity: selected.clone(),
                revision: 1,
            },
        );
        let refreshed = reduce(
            &offline,
            WalletDustRegistrationSettlementEvent::Eligibility {
                identity: selected.clone(),
                revision: 2,
                eligible: true,
            },
        );
        assert_eq!(
            reduce(
                &refreshed,
                WalletDustRegistrationSettlementEvent::Retry {
                    identity: selected.clone(),
                    revision: 2,
                },
            )
            .state,
            State::ActionRequired
        );

        let replacement = identity("profile_other", "preprod", 2);
        let replaced = reduce(
            &not_eligible,
            WalletDustRegistrationSettlementEvent::Eligibility {
                identity: replacement.clone(),
                revision: 1,
                eligible: true,
            },
        );
        assert_eq!(replaced.identity, Some(replacement.clone()));
        assert_eq!(replaced.state, State::ActionRequired);
        assert_eq!(
            reduce(
                &replaced,
                WalletDustRegistrationSettlementEvent::Offline {
                    identity: selected,
                    revision: 1,
                },
            ),
            replaced
        );
    }

    #[test]
    fn early_pending_and_dropped_reconciliation_are_deterministic() {
        let confirming = confirming();
        let included_early = reduce(
            &confirming,
            reconciliation(1, WalletDustRegistrationSettlementReconciliation::Included),
        );
        assert_eq!(included_early.state, State::Reconciling);
        assert_eq!(
            reduce(&included_early, dust_refresh(1, 1, true),).state,
            State::Ready
        );
        let duplicate_finality = reduce(&included_early, finality(1));
        assert_eq!(duplicate_finality.state, included_early.state);
        assert!(retained(&duplicate_finality).included);

        let pending = reduce(
            &reconciling(),
            reconciliation(2, WalletDustRegistrationSettlementReconciliation::Pending),
        );
        assert_eq!(pending.state, State::Confirming);
        let dropped = reduce(
            &pending,
            reconciliation(3, WalletDustRegistrationSettlementReconciliation::Dropped),
        );
        assert_eq!(dropped.state, State::ActionRequired);
        assert!(has_transaction(&dropped, &transaction()));
    }

    #[test]
    fn ready_regresses_on_dust_or_reconciliation_rollback() {
        let ready = ready();

        let dust_not_ready = reduce(&ready, dust_refresh(2, 2, false));
        assert_eq!(dust_not_ready.state, State::Reconciling);
        assert!(retained(&dust_not_ready).included);

        let pending = reduce(
            &ready,
            reconciliation(3, WalletDustRegistrationSettlementReconciliation::Pending),
        );
        assert_eq!(pending.state, State::Confirming);
        assert!(!retained(&pending).included);

        let dropped = reduce(
            &ready,
            reconciliation(3, WalletDustRegistrationSettlementReconciliation::Dropped),
        );
        assert_eq!(dropped.state, State::ActionRequired);
        assert!(has_transaction(&dropped, &transaction()));
    }

    #[test]
    fn re_prepared_draft_replaces_expired_authorization_request() {
        let identity = selected_identity();
        let first_draft = draft();
        let second_draft = WalletTransactionDraftId::parse("dustreg_reprepared").unwrap();
        let awaiting = reduce(
            &eligible(),
            WalletDustRegistrationSettlementEvent::AuthorizationRequested {
                identity: identity.clone(),
                draft_id: first_draft.clone(),
                preparation_revision: 1,
            },
        );
        let reprepared = reduce(
            &awaiting,
            WalletDustRegistrationSettlementEvent::AuthorizationRequested {
                identity: identity.clone(),
                draft_id: second_draft.clone(),
                preparation_revision: 2,
            },
        );
        assert_eq!(
            reduce(
                &reprepared,
                WalletDustRegistrationSettlementEvent::Cancelled {
                    identity: identity.clone(),
                    draft_id: first_draft.clone(),
                },
            ),
            reprepared
        );
        assert_eq!(
            reduce(
                &reprepared,
                WalletDustRegistrationSettlementEvent::AuthorizationRequested {
                    identity: identity.clone(),
                    draft_id: first_draft.clone(),
                    preparation_revision: 1,
                },
            ),
            reprepared
        );
        assert_eq!(
            reduce(
                &reprepared,
                WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                    identity: identity.clone(),
                    draft_id: first_draft,
                },
            ),
            reprepared
        );
        assert_eq!(
            reduce(
                &reprepared,
                WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                    identity,
                    draft_id: second_draft,
                },
            )
            .state,
            State::Submitting
        );
    }

    #[test]
    fn rejection_retains_the_latest_preparation_watermark() {
        let identity = selected_identity();
        let first_draft = draft();
        let second_draft = WalletTransactionDraftId::parse("dustreg_reprepared").unwrap();
        let first = reduce(
            &eligible(),
            WalletDustRegistrationSettlementEvent::AuthorizationRequested {
                identity: identity.clone(),
                draft_id: first_draft.clone(),
                preparation_revision: 1,
            },
        );
        let second = reduce(
            &first,
            WalletDustRegistrationSettlementEvent::AuthorizationRequested {
                identity: identity.clone(),
                draft_id: second_draft.clone(),
                preparation_revision: 2,
            },
        );
        let rejected = reduce(
            &second,
            WalletDustRegistrationSettlementEvent::AuthorizationRejected {
                identity: identity.clone(),
                draft_id: second_draft,
            },
        );
        assert_eq!(rejected.preparation_revision, 2);
        assert_eq!(rejected.state, State::ActionRequired);
        assert!(rejected.registration.is_none());

        let stale_request = reduce(
            &rejected,
            WalletDustRegistrationSettlementEvent::AuthorizationRequested {
                identity: identity.clone(),
                draft_id: first_draft.clone(),
                preparation_revision: 1,
            },
        );
        assert_eq!(stale_request, rejected);
        assert_eq!(
            reduce(
                &stale_request,
                WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                    identity,
                    draft_id: first_draft,
                },
            ),
            rejected
        );
    }

    #[test]
    fn reconciliation_revisions_prevent_stale_regressions() {
        let finality_then_pending = reduce(
            &reduce(&confirming(), finality(1)),
            reconciliation(2, WalletDustRegistrationSettlementReconciliation::Pending),
        );
        let pending_then_finality = reduce(
            &reduce(
                &confirming(),
                reconciliation(2, WalletDustRegistrationSettlementReconciliation::Pending),
            ),
            finality(1),
        );
        assert_eq!(finality_then_pending, pending_then_finality);

        let finality_first = reduce(&confirming(), finality(2));
        let included_after_finality = reduce(
            &finality_first,
            reconciliation(1, WalletDustRegistrationSettlementReconciliation::Included),
        );
        assert!(retained(&included_after_finality).included);
        assert_eq!(retained(&included_after_finality).observation_revision, 2);
        let finalized = reduce(&confirming(), finality(3));
        assert_eq!(
            reduce(
                &finalized,
                reconciliation(2, WalletDustRegistrationSettlementReconciliation::Dropped),
            ),
            finalized
        );
        let dropped = reduce(
            &confirming(),
            reconciliation(2, WalletDustRegistrationSettlementReconciliation::Dropped),
        );
        let recovered = reduce(&dropped, finality(3));
        assert_eq!(recovered.state, State::Reconciling);
        assert_eq!(retained(&recovered).reconciliation_revision, 2);
        assert_eq!(
            reduce(
                &recovered,
                reconciliation(1, WalletDustRegistrationSettlementReconciliation::Included),
            ),
            recovered
        );
        assert_eq!(
            reduce(&dropped, authorization_request(other_draft(), 2)),
            dropped
        );
        assert_eq!(reduce(&dropped, cancellation(draft())), dropped);
        let abandoned = reduce(&dropped, abandon_dropped(2));
        assert!(abandoned.registration.is_none());
        assert_eq!(
            reduce(&abandoned, authorization_request(other_draft(), 2)).state,
            State::AwaitingAuthorization
        );
        assert_eq!(reduce(&recovered, abandon_dropped(2)), recovered);
        let refreshed = reduce(&dropped, dust_refresh(1, 3, true));
        assert_eq!(reduce(&refreshed, abandon_dropped(2)), refreshed);
        let included = reduce(
            &refreshed,
            reconciliation(3, WalletDustRegistrationSettlementReconciliation::Included),
        );
        assert_eq!(included.state, State::Ready);
        let newer_drop = reduce(
            &confirming(),
            reconciliation(5, WalletDustRegistrationSettlementReconciliation::Dropped),
        );
        assert_eq!(reduce(&newer_drop, finality(3)), newer_drop);
        let ineligible = reduce(&confirming(), eligibility(2, false));
        let offline = reduce(&ineligible, offline_event(1));
        let dropped_ineligible = reduce(
            &offline,
            reconciliation(2, WalletDustRegistrationSettlementReconciliation::Dropped),
        );
        let resumed = reduce(&dropped_ineligible, resumed_event(2));
        assert!(has_transaction(&resumed, &transaction()));
        assert_eq!(reduce(&resumed, finality(3)).state, State::Reconciling);

        let included = reduce(
            &confirming(),
            reconciliation(2, WalletDustRegistrationSettlementReconciliation::Included),
        );
        assert_eq!(included.state, State::Reconciling);
        assert_eq!(retained(&included).observation_revision, 2);
        let newer_finality = reduce(&included, finality(4));
        assert_eq!(retained(&newer_finality).observation_revision, 4);
        let pending_after_finality = reduce(
            &newer_finality,
            reconciliation(3, WalletDustRegistrationSettlementReconciliation::Pending),
        );
        assert_eq!(pending_after_finality.state, State::Reconciling);
        assert!(!retained(&pending_after_finality).included);

        let stale_pending = reduce(
            &included,
            reconciliation(1, WalletDustRegistrationSettlementReconciliation::Pending),
        );
        assert_eq!(stale_pending, included);

        let current_pending = reduce(
            &stale_pending,
            reconciliation(3, WalletDustRegistrationSettlementReconciliation::Pending),
        );
        assert_eq!(current_pending.state, State::Confirming);
        assert_eq!(retained(&current_pending).observation_revision, 3);
        let delayed_finality = reduce(&current_pending, finality(2));
        assert_eq!(delayed_finality.state, current_pending.state);
        assert!(!retained(&delayed_finality).included);
        assert_eq!(
            reduce(
                &current_pending,
                reconciliation(2, WalletDustRegistrationSettlementReconciliation::Included),
            ),
            current_pending
        );
    }

    #[test]
    fn dust_refreshes_are_ordered_and_bound_to_the_inclusion_observation() {
        let included = reduce(
            &reconciling(),
            reconciliation(2, WalletDustRegistrationSettlementReconciliation::Included),
        );
        let not_ready = reduce(&included, dust_refresh(2, 2, false));
        assert_eq!(not_ready.state, State::Reconciling);
        assert_eq!(reduce(&not_ready, dust_refresh(1, 2, true),), not_ready);
        let stale_binding = reduce(&not_ready, dust_refresh(3, 1, true));
        assert_eq!(stale_binding.state, State::Reconciling);
        let ready = reduce(&stale_binding, dust_refresh(4, 2, true));
        assert_eq!(ready.state, State::Ready);
        let advanced_after_ready = reduce(&ready, finality(3));
        assert_eq!(advanced_after_ready.state, State::Reconciling);
        let advanced_before_ready = reduce(&included, finality(3));
        assert_eq!(
            reduce(&advanced_before_ready, dust_refresh(3, 2, true)).state,
            State::Reconciling
        );
        let refresh_first = reduce(&reduce(&included, dust_refresh(4, 3, true)), finality(3));
        let observation_first = reduce(&advanced_before_ready, dust_refresh(4, 3, true));
        assert_eq!(refresh_first, observation_first);
        assert_eq!(refresh_first.state, State::Ready);

        let mut second_included = included.clone();
        let registration = second_included.registration.as_mut().unwrap();
        registration.draft_id = other_draft();
        registration.transaction_id = Some(other_transaction());
        registration.dust_revision = 0;
        assert_eq!(
            reduce(
                &second_included,
                WalletDustRegistrationSettlementEvent::DustRefreshed {
                    identity: selected_identity(),
                    transaction_id: transaction(),
                    revision: 4,
                    after_observation_revision: 2,
                    ready: true,
                },
            ),
            second_included
        );
    }
}
