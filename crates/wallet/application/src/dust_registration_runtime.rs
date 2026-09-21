// SPDX-License-Identifier: Apache-2.0

//! Presentation- and executor-neutral admission boundary for DUST registration.

use std::time::Instant;

use crate::{
    MAX_WALLET_OPERATION_ATTEMPT, WalletDustRegistrationCoordinator, WalletDustRegistrationEffect,
    WalletDustRegistrationSettlementEvent, WalletDustRegistrationSettlementReconciliation,
    WalletDustRegistrationSettlementState, WalletDustRegistrationTimelineCode,
    WalletOperationAttempt, WalletOperationCausationId, WalletOperationCorrelationId,
    WalletOperationDurationMillis, WalletOperationEvent, WalletOperationId, WalletOperationOutcome,
    WalletOperationResource, WalletOperationResourceIdentity, WalletOperationTimeline,
    WalletOperationTrigger,
};

/// An exact operation handed to an application or protected-custody boundary.
///
/// The authorization operation deliberately carries no challenge or confirmation.
/// The protected-custody boundary obtains those from the retained registration
/// preview and returns an existing typed coordinator completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationRuntimeOperation {
    Prepare(WalletDustRegistrationEffect),
    RequestProtectedAuthorization(WalletDustRegistrationEffect),
    Submit(WalletDustRegistrationEffect),
    ObserveTransaction(WalletDustRegistrationEffect),
    RefreshDust(WalletDustRegistrationEffect),
}

impl From<WalletDustRegistrationEffect> for WalletDustRegistrationRuntimeOperation {
    fn from(effect: WalletDustRegistrationEffect) -> Self {
        match effect {
            effect @ WalletDustRegistrationEffect::Prepare { .. } => Self::Prepare(effect),
            effect @ WalletDustRegistrationEffect::RequestProtectedAuthorization { .. } => {
                Self::RequestProtectedAuthorization(effect)
            }
            effect @ WalletDustRegistrationEffect::Submit { .. } => Self::Submit(effect),
            effect @ WalletDustRegistrationEffect::ObserveTransaction { .. } => {
                Self::ObserveTransaction(effect)
            }
            effect @ WalletDustRegistrationEffect::RefreshDust { .. } => Self::RefreshDust(effect),
        }
    }
}

/// Opaque capability proving ownership of one admitted operation.
///
/// The runtime instance component prevents a token issued before a restore from
/// completing work in any restored instance, including a second restore of the
/// same quiescent checkpoint.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct WalletDustRegistrationRuntimeAdmissionToken {
    runtime_instance: u64,
    sequence: u64,
}

impl std::fmt::Debug for WalletDustRegistrationRuntimeAdmissionToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("WalletDustRegistrationRuntimeAdmissionToken(..)")
    }
}

/// Current format accepted by
/// [`WalletDustRegistrationRuntime::restore_quiescent_checkpoint`].
pub const WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION: u16 = 1;

/// Versioned, quiescent application-boundary state.
///
/// This is an in-memory application checkpoint, not a durable adapter encoding.
/// It deliberately contains only coordinator state; admission ownership is a
/// runtime-local invariant and is never checkpointed. Durable serialization of
/// coordinator state is outside this API's contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationRuntimeCheckpoint {
    pub version: u16,
    pub coordinator: WalletDustRegistrationCoordinator,
}

/// A checkpoint is available only when no external effect owns admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationRuntimeCheckpointError {
    AdmissionInProgress,
}

impl std::fmt::Display for WalletDustRegistrationRuntimeCheckpointError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdmissionInProgress => formatter
                .write_str("DUST registration runtime cannot checkpoint an admitted operation"),
        }
    }
}

impl std::error::Error for WalletDustRegistrationRuntimeCheckpointError {}

/// A checkpoint cannot be restored when its public format is unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationRuntimeRestoreError {
    UnsupportedCheckpointVersion { found: u16 },
}

impl std::fmt::Display for WalletDustRegistrationRuntimeRestoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedCheckpointVersion { found } => {
                write!(
                    formatter,
                    "unsupported DUST registration checkpoint version {found}"
                )
            }
        }
    }
}

impl std::error::Error for WalletDustRegistrationRuntimeRestoreError {}

/// Result of attempting to admit an externally scheduled operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationRuntimeAdmission {
    Admitted {
        operation: WalletDustRegistrationRuntimeOperation,
        token: WalletDustRegistrationRuntimeAdmissionToken,
    },
    Busy,
    Stale,
    Idle,
}

/// Single-flight composition around the pure coordinator.
///
/// This type owns neither I/O nor scheduling. Callers admit the current operation,
/// execute it through the matching application or protected-custody boundary, then
/// return its bounded coordinator event through [`Self::complete`].
#[derive(Debug)]
pub struct WalletDustRegistrationRuntime {
    coordinator: WalletDustRegistrationCoordinator,
    admitted_effect: Option<(
        WalletDustRegistrationEffect,
        WalletDustRegistrationRuntimeAdmissionToken,
    )>,
    timeline: WalletOperationTimeline,
    timeline_operation: Option<DustRegistrationTimelineOperation>,
    runtime_instance: u64,
    next_admission_sequence: u64,
}

#[derive(Debug)]
struct DustRegistrationTimelineOperation {
    operation_id: WalletOperationId,
    correlation_id: WalletOperationCorrelationId,
    causation_id: WalletOperationCausationId,
    attempt: u16,
    started: Instant,
}

impl Default for WalletDustRegistrationRuntime {
    fn default() -> Self {
        Self::with_operation_timeline(WalletOperationTimeline::default())
    }
}

impl WalletDustRegistrationRuntime {
    /// Installs validated recovered coordinator state with a fresh instance identity.
    pub(crate) fn from_recovered_coordinator(
        coordinator: WalletDustRegistrationCoordinator,
    ) -> Self {
        let mut runtime = Self::with_operation_timeline(WalletOperationTimeline::default());
        runtime.coordinator = coordinator;
        runtime.record_restore();
        runtime
    }

    /// Uses the shared operation timeline without making evidence part of policy.
    #[must_use]
    pub fn with_operation_timeline(timeline: WalletOperationTimeline) -> Self {
        Self {
            coordinator: WalletDustRegistrationCoordinator::default(),
            admitted_effect: None,
            timeline,
            timeline_operation: None,
            runtime_instance: next_runtime_instance(),
            next_admission_sequence: 0,
        }
    }

    #[must_use]
    pub fn operation_timeline(&self) -> WalletOperationTimeline {
        self.timeline.clone()
    }

    #[must_use]
    pub fn coordinator(&self) -> &WalletDustRegistrationCoordinator {
        &self.coordinator
    }

    /// Captures quiescent application state without any in-flight admission.
    pub fn quiescent_checkpoint(
        &self,
    ) -> Result<WalletDustRegistrationRuntimeCheckpoint, WalletDustRegistrationRuntimeCheckpointError>
    {
        if self.admitted_effect.is_some() {
            return Err(WalletDustRegistrationRuntimeCheckpointError::AdmissionInProgress);
        }
        Ok(WalletDustRegistrationRuntimeCheckpoint {
            version: WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION,
            coordinator: self.coordinator.clone(),
        })
    }

    /// Restores a supported quiescent checkpoint and requires executors to re-admit work.
    pub fn restore_quiescent_checkpoint(
        checkpoint: WalletDustRegistrationRuntimeCheckpoint,
    ) -> Result<Self, WalletDustRegistrationRuntimeRestoreError> {
        Self::restore_quiescent_checkpoint_with_operation_timeline(
            checkpoint,
            WalletOperationTimeline::default(),
        )
    }

    /// Restores policy state while writing only observational evidence to the supplied seam.
    pub fn restore_quiescent_checkpoint_with_operation_timeline(
        checkpoint: WalletDustRegistrationRuntimeCheckpoint,
        timeline: WalletOperationTimeline,
    ) -> Result<Self, WalletDustRegistrationRuntimeRestoreError> {
        if checkpoint.version != WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION {
            return Err(
                WalletDustRegistrationRuntimeRestoreError::UnsupportedCheckpointVersion {
                    found: checkpoint.version,
                },
            );
        }
        let mut runtime = Self::with_operation_timeline(timeline);
        runtime.coordinator = checkpoint.coordinator;
        runtime.record_restore();
        Ok(runtime)
    }

    /// Applies an external policy observation without starting work.
    pub fn observe(&mut self, event: WalletDustRegistrationSettlementEvent) {
        let next = self.coordinator.clone().reduce(event.clone());
        if next.projection() != self.coordinator.projection() {
            let replaces_identity = matches!(
                &event,
                WalletDustRegistrationSettlementEvent::Eligibility { identity, .. }
                    if self.coordinator.projection().identity.as_ref().is_some_and(|current| current != identity)
            );
            if replaces_identity {
                self.record_code(
                    WalletDustRegistrationTimelineCode::Superseded,
                    Some(WalletOperationOutcome::Superseded),
                );
            }
            let terminal =
                terminal_outcome(&event, self.coordinator.projection(), next.projection());
            self.coordinator = next;
            self.record_observation(&event, terminal);
            self.admitted_effect = None;
        }
    }

    /// Admits only the exact current effect once.
    #[must_use]
    pub fn admit_current(
        &mut self,
        effect: &WalletDustRegistrationEffect,
    ) -> WalletDustRegistrationRuntimeAdmission {
        let Some(current) = self.coordinator.active_effect() else {
            return WalletDustRegistrationRuntimeAdmission::Idle;
        };
        if current != effect {
            return WalletDustRegistrationRuntimeAdmission::Stale;
        }
        if self
            .admitted_effect
            .as_ref()
            .is_some_and(|(admitted, _)| admitted == effect)
        {
            return WalletDustRegistrationRuntimeAdmission::Busy;
        }
        self.next_admission_sequence = self.next_admission_sequence.wrapping_add(1);
        let token = WalletDustRegistrationRuntimeAdmissionToken {
            runtime_instance: self.runtime_instance,
            sequence: self.next_admission_sequence,
        };
        self.admitted_effect = Some((effect.clone(), token));
        self.record_effect_plan(effect);
        WalletDustRegistrationRuntimeAdmission::Admitted {
            operation: effect.clone().into(),
            token,
        }
    }

    /// Accepts a completion only when its opaque admission token is current.
    #[must_use]
    pub fn complete(
        &mut self,
        token: WalletDustRegistrationRuntimeAdmissionToken,
        event: WalletDustRegistrationSettlementEvent,
    ) -> bool {
        let Some((effect, admitted_token)) = &self.admitted_effect else {
            return false;
        };
        if *admitted_token != token || self.coordinator.active_effect() != Some(effect) {
            return false;
        }
        let next = self.coordinator.clone().reduce(event.clone());
        if next.projection() == self.coordinator.projection() {
            return false;
        }
        let terminal = terminal_outcome(&event, self.coordinator.projection(), next.projection());
        self.coordinator = next;
        self.record_observation(&event, terminal);
        self.admitted_effect = None;
        true
    }

    /// Releases an admitted operation without applying a completion.
    #[must_use]
    pub fn release(&mut self, token: WalletDustRegistrationRuntimeAdmissionToken) -> bool {
        if self
            .admitted_effect
            .as_ref()
            .is_none_or(|(_, admitted_token)| *admitted_token != token)
        {
            return false;
        }
        self.admitted_effect = None;
        true
    }

    fn record_restore(&mut self) {
        let Some(identity) = self.coordinator.projection().identity.clone() else {
            return;
        };
        self.ensure_timeline_operation(&identity, self.coordinator.projection().recovery_revision);
        self.record_code(WalletDustRegistrationTimelineCode::Restored, None);
    }

    fn record_observation(
        &mut self,
        event: &WalletDustRegistrationSettlementEvent,
        terminal: Option<WalletOperationOutcome>,
    ) {
        if self.timeline_operation.is_none()
            && !matches!(
                event,
                WalletDustRegistrationSettlementEvent::Eligibility { .. }
            )
        {
            return;
        }
        let identity = event_identity(event).clone();
        let revision = self.coordinator.projection().recovery_revision;
        self.ensure_timeline_operation(&identity, revision);
        self.record_code(event_code(event), terminal);
    }

    fn ensure_timeline_operation(
        &mut self,
        identity: &crate::WalletDustRegistrationSettlementIdentity,
        revision: u64,
    ) {
        if self.timeline_operation.is_some() {
            return;
        }
        let resource = timeline_resource(identity, revision);
        if let Ok((operation_id, correlation_id, causation_id)) = self
            .timeline
            .begin_operation(resource, WalletOperationTrigger::Initial)
        {
            self.timeline_operation = Some(DustRegistrationTimelineOperation {
                operation_id,
                correlation_id,
                causation_id,
                attempt: 0,
                started: Instant::now(),
            });
        }
    }

    fn record_effect_plan(&mut self, effect: &WalletDustRegistrationEffect) {
        let identity = effect_identity(effect);
        self.ensure_timeline_operation(identity, self.coordinator.projection().recovery_revision);
        let Some(context) = self.timeline_operation.as_mut() else {
            return;
        };
        context.attempt = context
            .attempt
            .saturating_add(1)
            .min(MAX_WALLET_OPERATION_ATTEMPT);
        let Some(attempt) = WalletOperationAttempt::new(context.attempt).ok() else {
            return;
        };
        if let Ok(causation) = self.timeline.record(
            context.operation_id,
            context.correlation_id,
            Some(context.causation_id),
            timeline_resource(identity, self.coordinator.projection().recovery_revision),
            WalletOperationTrigger::Initial,
            attempt,
            WalletOperationDurationMillis::zero(),
            WalletOperationEvent::EffectPlanned(effect_code(effect)),
        ) {
            context.causation_id = causation;
            context.started = Instant::now();
        }
    }

    fn record_code(
        &mut self,
        code: WalletDustRegistrationTimelineCode,
        terminal: Option<WalletOperationOutcome>,
    ) {
        let Some(context) = self.timeline_operation.as_mut() else {
            return;
        };
        let Some(identity) = self.coordinator.projection().identity.as_ref() else {
            return;
        };
        let Some(attempt) = WalletOperationAttempt::new(context.attempt.max(1)).ok() else {
            return;
        };
        let duration = WalletOperationDurationMillis::bounded(context.started.elapsed());
        let resource = timeline_resource(identity, self.coordinator.projection().recovery_revision);
        if let Ok(causation) = self.timeline.record(
            context.operation_id,
            context.correlation_id,
            Some(context.causation_id),
            resource.clone(),
            WalletOperationTrigger::Initial,
            attempt,
            duration,
            WalletOperationEvent::DustRegistration(code),
        ) {
            context.causation_id = causation;
        }
        if let Some(outcome) = terminal {
            let _ = self.timeline.record(
                context.operation_id,
                context.correlation_id,
                Some(context.causation_id),
                resource,
                WalletOperationTrigger::Initial,
                attempt,
                duration,
                WalletOperationEvent::Terminal {
                    outcome,
                    failure: None,
                },
            );
            self.timeline_operation = None;
        }
    }
}

fn next_runtime_instance() -> u64 {
    static NEXT_RUNTIME_INSTANCE: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(1);
    NEXT_RUNTIME_INSTANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn timeline_resource(
    identity: &crate::WalletDustRegistrationSettlementIdentity,
    revision: u64,
) -> WalletOperationResource {
    WalletOperationResource {
        identity: WalletOperationResourceIdentity::SelectedWalletRealm {
            profile: identity.profile.clone(),
            realm: identity.realm.clone(),
        },
        revision,
    }
}

fn effect_identity(
    effect: &WalletDustRegistrationEffect,
) -> &crate::WalletDustRegistrationSettlementIdentity {
    match effect {
        WalletDustRegistrationEffect::Prepare { identity }
        | WalletDustRegistrationEffect::RequestProtectedAuthorization { identity, .. }
        | WalletDustRegistrationEffect::Submit { identity, .. }
        | WalletDustRegistrationEffect::ObserveTransaction { identity, .. }
        | WalletDustRegistrationEffect::RefreshDust { identity, .. } => identity,
    }
}

const fn effect_code(effect: &WalletDustRegistrationEffect) -> crate::WalletOperationEffect {
    match effect {
        WalletDustRegistrationEffect::Prepare { .. } => {
            crate::WalletOperationEffect::DustRegistrationPrepare
        }
        WalletDustRegistrationEffect::RequestProtectedAuthorization { .. } => {
            crate::WalletOperationEffect::DustRegistrationAuthorization
        }
        WalletDustRegistrationEffect::Submit { .. } => {
            crate::WalletOperationEffect::DustRegistrationSubmit
        }
        WalletDustRegistrationEffect::ObserveTransaction { .. } => {
            crate::WalletOperationEffect::DustRegistrationObserveTransaction
        }
        WalletDustRegistrationEffect::RefreshDust { .. } => {
            crate::WalletOperationEffect::DustRegistrationRefreshDust
        }
    }
}

fn event_identity(
    event: &WalletDustRegistrationSettlementEvent,
) -> &crate::WalletDustRegistrationSettlementIdentity {
    match event {
        WalletDustRegistrationSettlementEvent::Eligibility { identity, .. }
        | WalletDustRegistrationSettlementEvent::RegistrationAlreadyCurrent { identity, .. }
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

const fn event_code(
    event: &WalletDustRegistrationSettlementEvent,
) -> WalletDustRegistrationTimelineCode {
    match event {
        WalletDustRegistrationSettlementEvent::Eligibility { .. } => {
            WalletDustRegistrationTimelineCode::EligibilityObserved
        }
        WalletDustRegistrationSettlementEvent::RegistrationAlreadyCurrent { .. } => {
            WalletDustRegistrationTimelineCode::RegistrationAlreadyCurrent
        }
        WalletDustRegistrationSettlementEvent::AuthorizationRequested { .. } => {
            WalletDustRegistrationTimelineCode::Prepared
        }
        WalletDustRegistrationSettlementEvent::AuthorizationSucceeded { .. } => {
            WalletDustRegistrationTimelineCode::AuthorizationSucceeded
        }
        WalletDustRegistrationSettlementEvent::AuthorizationRejected { .. } => {
            WalletDustRegistrationTimelineCode::AuthorizationRejected
        }
        WalletDustRegistrationSettlementEvent::SubmissionAccepted { .. } => {
            WalletDustRegistrationTimelineCode::SubmissionAccepted
        }
        WalletDustRegistrationSettlementEvent::FinalityObserved { .. } => {
            WalletDustRegistrationTimelineCode::FinalityObserved
        }
        WalletDustRegistrationSettlementEvent::RegistrationReconciled {
            reconciliation, ..
        } => match reconciliation {
            WalletDustRegistrationSettlementReconciliation::Pending => {
                WalletDustRegistrationTimelineCode::ReconciliationPending
            }
            WalletDustRegistrationSettlementReconciliation::Included => {
                WalletDustRegistrationTimelineCode::ReconciliationIncluded
            }
            WalletDustRegistrationSettlementReconciliation::Dropped => {
                WalletDustRegistrationTimelineCode::ReconciliationDropped
            }
        },
        WalletDustRegistrationSettlementEvent::DustRefreshed { ready, .. } => {
            if *ready {
                WalletDustRegistrationTimelineCode::DustRefreshedReady
            } else {
                WalletDustRegistrationTimelineCode::DustRefreshedPending
            }
        }
        WalletDustRegistrationSettlementEvent::DroppedRegistrationAbandoned { .. } => {
            WalletDustRegistrationTimelineCode::DroppedRegistrationAbandoned
        }
        WalletDustRegistrationSettlementEvent::Cancelled { .. } => {
            WalletDustRegistrationTimelineCode::Cancelled
        }
        WalletDustRegistrationSettlementEvent::Offline { .. } => {
            WalletDustRegistrationTimelineCode::Offline
        }
        WalletDustRegistrationSettlementEvent::TimedOut { .. } => {
            WalletDustRegistrationTimelineCode::TimedOut
        }
        WalletDustRegistrationSettlementEvent::Degraded { .. } => {
            WalletDustRegistrationTimelineCode::AdapterFailed
        }
        WalletDustRegistrationSettlementEvent::Suspended { .. } => {
            WalletDustRegistrationTimelineCode::Suspended
        }
        WalletDustRegistrationSettlementEvent::Resumed { .. } => {
            WalletDustRegistrationTimelineCode::Resumed
        }
        WalletDustRegistrationSettlementEvent::Retry { .. } => {
            WalletDustRegistrationTimelineCode::Retry
        }
        WalletDustRegistrationSettlementEvent::Superseded { .. } => {
            WalletDustRegistrationTimelineCode::Superseded
        }
    }
}

const fn terminal_outcome(
    event: &WalletDustRegistrationSettlementEvent,
    previous: &crate::WalletDustRegistrationSettlementProjection,
    next: &crate::WalletDustRegistrationSettlementProjection,
) -> Option<WalletOperationOutcome> {
    match event {
        WalletDustRegistrationSettlementEvent::Cancelled { .. } => {
            Some(WalletOperationOutcome::Cancelled)
        }
        WalletDustRegistrationSettlementEvent::Superseded { .. } => {
            Some(WalletOperationOutcome::Superseded)
        }
        _ if !matches!(previous.state, WalletDustRegistrationSettlementState::Ready)
            && matches!(next.state, WalletDustRegistrationSettlementState::Ready) =>
        {
            Some(WalletOperationOutcome::Succeeded)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use oxid_wallet_domain::{ChainNetworkId, WalletProfileId};

    use super::*;
    use crate::completion;

    fn identity(generation: u64) -> crate::WalletDustRegistrationSettlementIdentity {
        crate::WalletDustRegistrationSettlementIdentity {
            profile: WalletProfileId::parse("profile_test").unwrap(),
            realm: ChainNetworkId::parse("undeployed").unwrap(),
            generation,
        }
    }

    #[test]
    fn maps_every_coordinator_effect_to_one_runtime_operation() {
        let draft = oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap();
        let transaction = oxid_wallet_domain::ChainTransactionId::parse("tx_test").unwrap();
        let effects = [
            WalletDustRegistrationEffect::Prepare {
                identity: identity(1),
            },
            WalletDustRegistrationEffect::RequestProtectedAuthorization {
                identity: identity(1),
                draft_id: draft.clone(),
            },
            WalletDustRegistrationEffect::Submit {
                identity: identity(1),
                draft_id: draft,
            },
            WalletDustRegistrationEffect::ObserveTransaction {
                identity: identity(1),
                transaction_id: transaction.clone(),
            },
            WalletDustRegistrationEffect::RefreshDust {
                identity: identity(1),
                transaction_id: transaction,
            },
        ];
        let operations: Vec<_> = effects
            .into_iter()
            .map(WalletDustRegistrationRuntimeOperation::from)
            .collect();
        assert!(matches!(
            operations[0],
            WalletDustRegistrationRuntimeOperation::Prepare(_)
        ));
        assert!(matches!(
            operations[1],
            WalletDustRegistrationRuntimeOperation::RequestProtectedAuthorization(_)
        ));
        assert!(matches!(
            operations[2],
            WalletDustRegistrationRuntimeOperation::Submit(_)
        ));
        assert!(matches!(
            operations[3],
            WalletDustRegistrationRuntimeOperation::ObserveTransaction(_)
        ));
        assert!(matches!(
            operations[4],
            WalletDustRegistrationRuntimeOperation::RefreshDust(_)
        ));
    }

    fn admitted_token(
        admission: WalletDustRegistrationRuntimeAdmission,
    ) -> WalletDustRegistrationRuntimeAdmissionToken {
        match admission {
            WalletDustRegistrationRuntimeAdmission::Admitted { token, .. } => token,
            _ => panic!("expected an admitted operation"),
        }
    }

    #[test]
    fn completion_matrix_accepts_current_token_and_rejects_stale_and_idle_tokens() {
        let mut runtime = WalletDustRegistrationRuntime::default();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        let prepare = runtime.coordinator().active_effect().unwrap().clone();
        let token = admitted_token(runtime.admit_current(&prepare));
        assert!(runtime.complete(
            token,
            completion::prepared(
                identity(1),
                oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap(),
                1,
            ),
        ));
        assert!(matches!(
            runtime.coordinator().active_effect(),
            Some(WalletDustRegistrationEffect::RequestProtectedAuthorization { .. })
        ));
        assert!(!runtime.complete(
            token,
            completion::prepared(
                identity(1),
                oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap(),
                1,
            ),
        ));

        let authorization = runtime.coordinator().active_effect().unwrap().clone();
        let stale_token = admitted_token(runtime.admit_current(&authorization));
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(2),
            revision: 1,
            eligible: true,
        });
        assert!(!runtime.release(stale_token));
        assert_eq!(
            runtime.admit_current(&prepare),
            WalletDustRegistrationRuntimeAdmission::Stale
        );
        assert!(!WalletDustRegistrationRuntime::default().release(stale_token));
        assert_eq!(
            WalletDustRegistrationRuntime::default().admit_current(&prepare),
            WalletDustRegistrationRuntimeAdmission::Idle
        );
    }

    #[test]
    fn ignored_observation_preserves_the_in_flight_admission() {
        let mut runtime = WalletDustRegistrationRuntime::default();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        let prepare = runtime.coordinator().active_effect().unwrap().clone();
        let token = admitted_token(runtime.admit_current(&prepare));
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        assert_eq!(
            runtime.admit_current(&prepare),
            WalletDustRegistrationRuntimeAdmission::Busy
        );
        assert!(runtime.release(token));
    }

    #[test]
    fn restores_a_versioned_checkpoint_without_reviving_an_in_flight_admission() {
        let mut runtime = WalletDustRegistrationRuntime::default();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        let prepare = runtime.coordinator().active_effect().unwrap().clone();
        let checkpoint = runtime.quiescent_checkpoint().unwrap();
        let stale_token = admitted_token(runtime.admit_current(&prepare));

        let timeline = WalletOperationTimeline::with_capacity(16).unwrap();
        let mut restored =
            WalletDustRegistrationRuntime::restore_quiescent_checkpoint_with_operation_timeline(
                checkpoint,
                timeline.clone(),
            )
            .unwrap();
        assert!(timeline_codes(&restored).contains(&WalletDustRegistrationTimelineCode::Restored));
        assert_eq!(restored.coordinator().active_effect(), Some(&prepare));
        assert!(!restored.complete(
            stale_token,
            completion::prepared(
                identity(1),
                oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap(),
                1,
            ),
        ));

        let restored_token = admitted_token(restored.admit_current(&prepare));
        assert_ne!(restored_token, stale_token);
        assert!(restored.complete(
            restored_token,
            completion::prepared(
                identity(1),
                oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap(),
                1,
            ),
        ));
    }

    #[test]
    fn quiescent_checkpoints_invalidate_tokens_from_every_prior_runtime_instance() {
        let mut runtime = WalletDustRegistrationRuntime::default();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        let prepare = runtime.coordinator().active_effect().unwrap().clone();
        let checkpoint = runtime.quiescent_checkpoint().unwrap();
        let first_token = admitted_token(runtime.admit_current(&prepare));
        let mut restored =
            WalletDustRegistrationRuntime::restore_quiescent_checkpoint(checkpoint.clone())
                .unwrap();
        let second_token = admitted_token(restored.admit_current(&prepare));
        let mut restored_again =
            WalletDustRegistrationRuntime::restore_quiescent_checkpoint(checkpoint).unwrap();
        let third_token = admitted_token(restored_again.admit_current(&prepare));

        assert_ne!(first_token, second_token);
        assert_ne!(second_token, third_token);
        assert!(!restored_again.complete(
            first_token,
            completion::prepared(
                identity(1),
                oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap(),
                1,
            ),
        ));
        assert!(!restored_again.complete(
            second_token,
            completion::prepared(
                identity(1),
                oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap(),
                1,
            ),
        ));
        assert_eq!(
            restored_again.admit_current(&prepare),
            WalletDustRegistrationRuntimeAdmission::Busy
        );
    }

    #[test]
    fn rejects_a_checkpoint_while_an_operation_owns_admission() {
        let mut runtime = WalletDustRegistrationRuntime::default();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        let prepare = runtime.coordinator().active_effect().unwrap().clone();
        let _ = admitted_token(runtime.admit_current(&prepare));

        assert_eq!(
            runtime.quiescent_checkpoint(),
            Err(WalletDustRegistrationRuntimeCheckpointError::AdmissionInProgress)
        );
    }

    #[test]
    fn restores_submitted_and_included_registrations_to_their_required_work() {
        let mut runtime = WalletDustRegistrationRuntime::default();
        let draft = oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap();
        let transaction = oxid_wallet_domain::ChainTransactionId::parse("tx_test").unwrap();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        runtime.observe(completion::prepared(identity(1), draft.clone(), 1));
        runtime.observe(
            WalletDustRegistrationSettlementEvent::AuthorizationSucceeded {
                identity: identity(1),
                draft_id: draft.clone(),
            },
        );
        runtime.observe(WalletDustRegistrationSettlementEvent::SubmissionAccepted {
            identity: identity(1),
            draft_id: draft,
            transaction_id: transaction.clone(),
        });
        let submitted = WalletDustRegistrationRuntime::restore_quiescent_checkpoint(
            runtime.quiescent_checkpoint().unwrap(),
        )
        .unwrap();
        assert!(matches!(
            submitted.coordinator().active_effect(),
            Some(WalletDustRegistrationEffect::ObserveTransaction { transaction_id, .. })
                if transaction_id == &transaction
        ));

        runtime.observe(
            WalletDustRegistrationSettlementEvent::RegistrationReconciled {
                identity: identity(1),
                transaction_id: transaction.clone(),
                revision: 2,
                reconciliation: crate::WalletDustRegistrationSettlementReconciliation::Included,
            },
        );
        let included = WalletDustRegistrationRuntime::restore_quiescent_checkpoint(
            runtime.quiescent_checkpoint().unwrap(),
        )
        .unwrap();
        assert!(matches!(
            included.coordinator().active_effect(),
            Some(WalletDustRegistrationEffect::RefreshDust { transaction_id, .. })
                if transaction_id == &transaction
        ));
    }

    type RecoveryScenario = (
        fn(
            crate::WalletDustRegistrationSettlementIdentity,
        ) -> WalletDustRegistrationSettlementEvent,
        WalletDustRegistrationTimelineCode,
    );

    fn timeline_codes(
        runtime: &WalletDustRegistrationRuntime,
    ) -> Vec<WalletDustRegistrationTimelineCode> {
        runtime
            .operation_timeline()
            .query()
            .unwrap()
            .records()
            .iter()
            .filter_map(|record| match record.event {
                WalletOperationEvent::DustRegistration(code) => Some(code),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn recovery_timeline_command_event_matrix_is_closed_payload_free_and_observational() {
        let timeline = WalletOperationTimeline::with_capacity(128).unwrap();
        let mut runtime = WalletDustRegistrationRuntime::with_operation_timeline(timeline.clone());
        let draft = oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap();
        let transaction = oxid_wallet_domain::ChainTransactionId::parse("tx_test").unwrap();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        let prepare = runtime.coordinator().active_effect().unwrap().clone();
        let token = admitted_token(runtime.admit_current(&prepare));
        assert!(runtime.complete(token, completion::prepared(identity(1), draft.clone(), 1)));
        runtime.observe(completion::authorized(identity(1), draft.clone()));
        runtime.observe(completion::submitted(
            identity(1),
            draft.clone(),
            transaction.clone(),
        ));
        runtime.observe(completion::finality_observed(
            identity(1),
            transaction.clone(),
            1,
        ));
        runtime.observe(completion::reconciled(
            identity(1),
            transaction.clone(),
            2,
            crate::WalletDustRegistrationSettlementReconciliation::Included,
        ));
        runtime.observe(completion::dust_refreshed(
            identity(1),
            transaction,
            1,
            2,
            true,
        ));
        let codes = timeline_codes(&runtime);
        assert!(codes.contains(&WalletDustRegistrationTimelineCode::EligibilityObserved));
        assert!(codes.contains(&WalletDustRegistrationTimelineCode::Prepared));
        assert!(codes.contains(&WalletDustRegistrationTimelineCode::AuthorizationSucceeded));
        assert!(codes.contains(&WalletDustRegistrationTimelineCode::SubmissionAccepted));
        assert!(codes.contains(&WalletDustRegistrationTimelineCode::FinalityObserved));
        assert!(codes.contains(&WalletDustRegistrationTimelineCode::ReconciliationIncluded));
        assert!(codes.contains(&WalletDustRegistrationTimelineCode::DustRefreshedReady));
        assert!(timeline.query().unwrap().records().iter().all(|record| {
            record.attempt.value() <= MAX_WALLET_OPERATION_ATTEMPT
                && record.duration.value() <= crate::MAX_WALLET_OPERATION_DURATION_MILLIS
                && record.measurements.as_slice().is_empty()
        }));
        assert!(
            timeline
                .query()
                .unwrap()
                .records()
                .iter()
                .any(|record| matches!(
                    record.event,
                    WalletOperationEvent::Terminal {
                        outcome: WalletOperationOutcome::Succeeded,
                        ..
                    }
                ))
        );
    }

    #[test]
    fn terminal_success_is_emitted_once_on_the_transition_into_ready() {
        let timeline = WalletOperationTimeline::with_capacity(32).unwrap();
        let mut runtime = WalletDustRegistrationRuntime::with_operation_timeline(timeline.clone());
        let draft = oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap();
        let transaction = oxid_wallet_domain::ChainTransactionId::parse("tx_test").unwrap();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        let prepare = runtime.coordinator().active_effect().unwrap().clone();
        let token = admitted_token(runtime.admit_current(&prepare));
        assert!(runtime.complete(token, completion::prepared(identity(1), draft.clone(), 1)));
        runtime.observe(completion::authorized(identity(1), draft.clone()));
        runtime.observe(completion::submitted(
            identity(1),
            draft,
            transaction.clone(),
        ));
        runtime.observe(completion::finality_observed(
            identity(1),
            transaction.clone(),
            1,
        ));
        runtime.observe(completion::reconciled(
            identity(1),
            transaction.clone(),
            2,
            WalletDustRegistrationSettlementReconciliation::Included,
        ));
        runtime.observe(completion::dust_refreshed(
            identity(1),
            transaction.clone(),
            1,
            2,
            true,
        ));

        let expected = vec![
            WalletOperationEvent::Admitted,
            WalletOperationEvent::DustRegistration(
                WalletDustRegistrationTimelineCode::EligibilityObserved,
            ),
            WalletOperationEvent::EffectPlanned(
                crate::WalletOperationEffect::DustRegistrationPrepare,
            ),
            WalletOperationEvent::DustRegistration(WalletDustRegistrationTimelineCode::Prepared),
            WalletOperationEvent::DustRegistration(
                WalletDustRegistrationTimelineCode::AuthorizationSucceeded,
            ),
            WalletOperationEvent::DustRegistration(
                WalletDustRegistrationTimelineCode::SubmissionAccepted,
            ),
            WalletOperationEvent::DustRegistration(
                WalletDustRegistrationTimelineCode::FinalityObserved,
            ),
            WalletOperationEvent::DustRegistration(
                WalletDustRegistrationTimelineCode::ReconciliationIncluded,
            ),
            WalletOperationEvent::DustRegistration(
                WalletDustRegistrationTimelineCode::DustRefreshedReady,
            ),
            WalletOperationEvent::Terminal {
                outcome: WalletOperationOutcome::Succeeded,
                failure: None,
            },
        ];
        assert_eq!(
            timeline
                .query()
                .unwrap()
                .records()
                .iter()
                .map(|record| record.event.clone())
                .collect::<Vec<_>>(),
            expected
        );

        // This later refresh changes the projection but leaves it Ready.
        runtime.observe(completion::dust_refreshed(
            identity(1),
            transaction,
            2,
            2,
            true,
        ));
        assert_eq!(
            runtime.coordinator().projection().state,
            WalletDustRegistrationSettlementState::Ready
        );
        assert_eq!(
            timeline
                .query()
                .unwrap()
                .records()
                .iter()
                .map(|record| record.event.clone())
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn recovery_timeline_failure_retry_and_duplicate_matrix_preserves_admission_and_policy() {
        let scenarios: &[RecoveryScenario] = &[
            (
                |identity| WalletDustRegistrationSettlementEvent::Offline {
                    identity,
                    revision: 2,
                },
                WalletDustRegistrationTimelineCode::Offline,
            ),
            (
                |identity| WalletDustRegistrationSettlementEvent::TimedOut {
                    identity,
                    revision: 2,
                },
                WalletDustRegistrationTimelineCode::TimedOut,
            ),
            (
                |identity| WalletDustRegistrationSettlementEvent::Degraded {
                    identity,
                    revision: 2,
                },
                WalletDustRegistrationTimelineCode::AdapterFailed,
            ),
            (
                |identity| WalletDustRegistrationSettlementEvent::Suspended {
                    identity,
                    revision: 2,
                },
                WalletDustRegistrationTimelineCode::Suspended,
            ),
        ];
        for (event, expected) in scenarios {
            let mut runtime = WalletDustRegistrationRuntime::default();
            runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
                identity: identity(1),
                revision: 1,
                eligible: true,
            });
            runtime.observe(event(identity(1)));
            assert!(timeline_codes(&runtime).contains(expected));
        }

        let mut runtime = WalletDustRegistrationRuntime::default();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        let effect = runtime.coordinator().active_effect().unwrap().clone();
        let token = admitted_token(runtime.admit_current(&effect));
        let before_duplicate = runtime
            .operation_timeline()
            .query()
            .unwrap()
            .total_records();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        assert_eq!(
            runtime.admit_current(&effect),
            WalletDustRegistrationRuntimeAdmission::Busy
        );
        assert_eq!(
            runtime
                .operation_timeline()
                .query()
                .unwrap()
                .total_records(),
            before_duplicate
        );
        assert!(runtime.release(token));

        runtime.observe(WalletDustRegistrationSettlementEvent::Offline {
            identity: identity(1),
            revision: 2,
        });
        runtime.observe(WalletDustRegistrationSettlementEvent::Resumed {
            identity: identity(1),
            revision: 3,
        });
        runtime.observe(WalletDustRegistrationSettlementEvent::Offline {
            identity: identity(1),
            revision: 4,
        });
        runtime.observe(WalletDustRegistrationSettlementEvent::Retry {
            identity: identity(1),
            revision: 5,
        });
        assert!(timeline_codes(&runtime).contains(&WalletDustRegistrationTimelineCode::Resumed));
        assert!(timeline_codes(&runtime).contains(&WalletDustRegistrationTimelineCode::Retry));

        let draft = oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap();
        let mut cancelled = WalletDustRegistrationRuntime::default();
        cancelled.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        cancelled.observe(completion::prepared(identity(1), draft.clone(), 1));
        cancelled.observe(WalletDustRegistrationSettlementEvent::Cancelled {
            identity: identity(1),
            draft_id: draft,
        });
        assert!(
            timeline_codes(&cancelled).contains(&WalletDustRegistrationTimelineCode::Cancelled)
        );
        assert!(
            cancelled
                .operation_timeline()
                .query()
                .unwrap()
                .records()
                .iter()
                .any(|record| matches!(
                    record.event,
                    WalletOperationEvent::Terminal {
                        outcome: WalletOperationOutcome::Cancelled,
                        ..
                    }
                ))
        );

        let mut superseded = WalletDustRegistrationRuntime::default();
        superseded.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        superseded.observe(WalletDustRegistrationSettlementEvent::Superseded {
            identity: identity(1),
        });
        assert!(
            timeline_codes(&superseded).contains(&WalletDustRegistrationTimelineCode::Superseded)
        );
    }

    #[test]
    fn rejects_unknown_checkpoint_versions() {
        let mut checkpoint = WalletDustRegistrationRuntime::default()
            .quiescent_checkpoint()
            .unwrap();
        checkpoint.version = WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION + 1;
        assert!(matches!(
            WalletDustRegistrationRuntime::restore_quiescent_checkpoint(checkpoint),
            Err(
                WalletDustRegistrationRuntimeRestoreError::UnsupportedCheckpointVersion {
                    found
                }
            ) if found == WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION + 1
        ));
    }
}
