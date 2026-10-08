// SPDX-License-Identifier: Apache-2.0

//! Secret-safe, process-local evidence for wallet operations.
//!
//! This bounded timeline supports diagnostics and derived metrics. It is not an
//! authoritative event store: wallet projections and checkpoints remain the
//! source of truth and records are neither durable nor replayable. Mnemonics,
//! seeds, keys, addresses, transaction or credential payloads, shielded notes,
//! endpoint credentials, and unrestricted adapter errors have no storage type.

use std::{
    collections::VecDeque,
    error::Error,
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};

use oxid_wallet_domain::{ChainNetworkId, WalletProfileId};

use crate::{
    WalletRealmEffectOutcome, WalletRealmReconciliationEffect, WalletRealmReconciliationTrigger,
};

/// Default and hard maximum retention for the process-local operation timeline.
pub const DEFAULT_WALLET_OPERATION_TIMELINE_CAPACITY: usize = 256;
pub const MAX_WALLET_OPERATION_TIMELINE_CAPACITY: usize = 1_024;
pub const MAX_WALLET_OPERATION_DURATION_MILLIS: u64 = 86_400_000;
pub const MAX_WALLET_OPERATION_TIMESTAMP_MILLIS: u64 = 253_402_300_799_999;
pub const MAX_WALLET_OPERATION_ATTEMPT: u16 = 1_024;

macro_rules! fixed_id {
    ($name:ident, $size:expr) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; $size]);

        impl $name {
            #[must_use]
            pub const fn from_bytes(bytes: [u8; $size]) -> Self {
                Self(bytes)
            }

            #[must_use]
            pub const fn to_bytes(self) -> [u8; $size] {
                self.0
            }
        }
    };
}

// Shapes match OpenTelemetry trace/span identifiers without depending on an exporter.
fixed_id!(WalletOperationId, 16);
fixed_id!(WalletOperationCorrelationId, 16);
fixed_id!(WalletOperationCausationId, 8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WalletOperationAttempt(u16);

impl WalletOperationAttempt {
    pub fn new(value: u16) -> Result<Self, WalletOperationValueError> {
        if (1..=MAX_WALLET_OPERATION_ATTEMPT).contains(&value) {
            Ok(Self(value))
        } else {
            Err(WalletOperationValueError::AttemptOutOfRange)
        }
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WalletOperationDurationMillis(u64);

impl WalletOperationDurationMillis {
    pub fn new(value: u64) -> Result<Self, WalletOperationValueError> {
        if value <= MAX_WALLET_OPERATION_DURATION_MILLIS {
            Ok(Self(value))
        } else {
            Err(WalletOperationValueError::DurationOutOfRange)
        }
    }

    #[must_use]
    pub const fn zero() -> Self {
        Self(0)
    }

    #[must_use]
    pub fn bounded(duration: Duration) -> Self {
        let millis = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        Self(millis.min(MAX_WALLET_OPERATION_DURATION_MILLIS))
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Non-authoritative process-relative observation time. Record sequence is the
/// sole ordering authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WalletOperationTimestampMillis(u64);

impl WalletOperationTimestampMillis {
    #[must_use]
    pub fn bounded(duration: Duration) -> Self {
        let millis = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        Self(millis.min(MAX_WALLET_OPERATION_TIMESTAMP_MILLIS))
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletOperationValueError {
    AttemptOutOfRange,
    DurationOutOfRange,
}

impl fmt::Display for WalletOperationValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AttemptOutOfRange => "operation attempt is outside the bounded range",
            Self::DurationOutOfRange => "operation duration is outside the bounded range",
        })
    }
}

impl Error for WalletOperationValueError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletOperationResourceIdentity {
    SelectedWalletRealm {
        profile: WalletProfileId,
        realm: ChainNetworkId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletOperationResource {
    pub identity: WalletOperationResourceIdentity,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletOperationTrigger {
    Initial,
    ManualRefresh,
    ActionPreflight,
}

impl From<WalletRealmReconciliationTrigger> for WalletOperationTrigger {
    fn from(value: WalletRealmReconciliationTrigger) -> Self {
        match value {
            WalletRealmReconciliationTrigger::Initial => Self::Initial,
            WalletRealmReconciliationTrigger::ManualRefresh => Self::ManualRefresh,
            WalletRealmReconciliationTrigger::ActionPreflight => Self::ActionPreflight,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletOperationEffect {
    SyncAccount,
    SyncDust,
    SyncShielded,
    DustRegistrationPrepare,
    DustRegistrationAuthorization,
    DustRegistrationSubmit,
    DustRegistrationObserveTransaction,
    DustRegistrationRefreshDust,
}

impl From<WalletRealmReconciliationEffect> for WalletOperationEffect {
    fn from(value: WalletRealmReconciliationEffect) -> Self {
        match value {
            WalletRealmReconciliationEffect::SyncAccount => Self::SyncAccount,
            WalletRealmReconciliationEffect::SyncDust => Self::SyncDust,
            WalletRealmReconciliationEffect::SyncShielded => Self::SyncShielded,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletOperationOutcome {
    Succeeded,
    InProgress,
    Stale,
    Missing,
    Blocked,
    Unsupported,
    PartialFailure,
    Failed,
    Superseded,
    SelectionChanged,
    Cancelled,
    NoChanges,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletOperationFailure {
    AccountNotFound,
    UnsupportedNetwork,
    ProtectionNotInitialized,
    ProtectionLocked,
    AdapterUnavailable,
    InvalidAdapterData,
    Conflict,
    SelectionChanged,
    ObservationSuperseded,
    OperationCancelled,
    RuntimeUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletOperationResourceMeasurement {
    CurrentCursor(u64),
    TargetCursor(u64),
    EventsProcessed(u64),
    OwnedNoteCount(u64),
    CommitmentCount(u64),
}

/// Closed, bounded resource measurements attached to one effect completion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalletOperationResourceMeasurements {
    values: Vec<WalletOperationResourceMeasurement>,
}

impl WalletOperationResourceMeasurements {
    pub(crate) const MAX_VALUES: usize = 5;

    pub(crate) fn from_values(values: Vec<WalletOperationResourceMeasurement>) -> Self {
        assert!(
            values.len() <= Self::MAX_VALUES,
            "resource measurements are bounded"
        );
        Self { values }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[WalletOperationResourceMeasurement] {
        &self.values
    }
}

/// Closed, payload-free DUST registration recovery facts.
///
/// Each value is a presentation-neutral observation of a reducer event; consumers
/// must use the accompanying [`WalletOperationEvent::Terminal`] record to identify
/// a closed lifecycle outcome. Recoverable codes (`Offline`, `TimedOut`,
/// `AdapterFailed`, and `Suspended`) close their current timeline operation as
/// [`WalletOperationOutcome::InProgress`], never as a failure. A later `Resumed`
/// or `Retry` observation starts a new operation when it changes the reducer
/// projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationTimelineCode {
    EligibilityObserved,
    RegistrationAlreadyCurrent,
    Prepared,
    AuthorizationSucceeded,
    AuthorizationRejected,
    SubmissionAccepted,
    FinalityObserved,
    ReconciliationPending,
    ReconciliationIncluded,
    ReconciliationDropped,
    DustRefreshedReady,
    DustRefreshedPending,
    DroppedRegistrationAbandoned,
    Cancelled,
    Offline,
    TimedOut,
    AdapterFailed,
    Suspended,
    Resumed,
    Retry,
    Superseded,
    Restored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletOperationEvent {
    Admitted,
    EffectPlanned(WalletOperationEffect),
    DustRegistration(WalletDustRegistrationTimelineCode),
    EffectCompleted {
        effect: WalletOperationEffect,
        outcome: WalletOperationOutcome,
        failure: Option<WalletOperationFailure>,
    },
    Terminal {
        outcome: WalletOperationOutcome,
        failure: Option<WalletOperationFailure>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletOperationRecord {
    pub sequence: u64,
    pub operation_id: WalletOperationId,
    pub correlation_id: WalletOperationCorrelationId,
    pub causation_id: WalletOperationCausationId,
    pub caused_by: Option<WalletOperationCausationId>,
    pub resource: WalletOperationResource,
    pub trigger: WalletOperationTrigger,
    pub attempt: WalletOperationAttempt,
    pub timestamp: Option<WalletOperationTimestampMillis>,
    pub duration: WalletOperationDurationMillis,
    pub measurements: WalletOperationResourceMeasurements,
    pub event: WalletOperationEvent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalletOperationOutcomeCounts {
    pub succeeded: u64,
    pub in_progress: u64,
    pub stale: u64,
    pub missing: u64,
    pub blocked: u64,
    pub unsupported: u64,
    pub partial_failure: u64,
    pub failed: u64,
    pub superseded: u64,
    pub selection_changed: u64,
    pub cancelled: u64,
    pub no_changes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalletOperationAggregates {
    pub total_duration_millis: u64,
    pub maximum_attempt: u16,
    pub outcomes: WalletOperationOutcomeCounts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletOperationTimelineSnapshot {
    capacity: usize,
    total_records: u64,
    evicted_records: u64,
    records: Vec<WalletOperationRecord>,
}

impl WalletOperationTimelineSnapshot {
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub const fn total_records(&self) -> u64 {
        self.total_records
    }

    #[must_use]
    pub const fn evicted_records(&self) -> u64 {
        self.evicted_records
    }

    #[must_use]
    pub fn records(&self) -> &[WalletOperationRecord] {
        &self.records
    }

    #[must_use]
    pub fn aggregates(&self) -> WalletOperationAggregates {
        let mut aggregate = WalletOperationAggregates::default();
        for record in &self.records {
            aggregate.total_duration_millis = aggregate
                .total_duration_millis
                .saturating_add(record.duration.value());
            aggregate.maximum_attempt = aggregate.maximum_attempt.max(record.attempt.value());
            let outcome = match record.event {
                WalletOperationEvent::EffectCompleted { outcome, .. }
                | WalletOperationEvent::Terminal { outcome, .. } => Some(outcome),
                WalletOperationEvent::Admitted
                | WalletOperationEvent::EffectPlanned(_)
                | WalletOperationEvent::DustRegistration(_) => None,
            };
            if let Some(outcome) = outcome {
                let count = match outcome {
                    WalletOperationOutcome::Succeeded => &mut aggregate.outcomes.succeeded,
                    WalletOperationOutcome::InProgress => &mut aggregate.outcomes.in_progress,
                    WalletOperationOutcome::Stale => &mut aggregate.outcomes.stale,
                    WalletOperationOutcome::Missing => &mut aggregate.outcomes.missing,
                    WalletOperationOutcome::Blocked => &mut aggregate.outcomes.blocked,
                    WalletOperationOutcome::Unsupported => &mut aggregate.outcomes.unsupported,
                    WalletOperationOutcome::PartialFailure => {
                        &mut aggregate.outcomes.partial_failure
                    }
                    WalletOperationOutcome::Failed => &mut aggregate.outcomes.failed,
                    WalletOperationOutcome::Superseded => &mut aggregate.outcomes.superseded,
                    WalletOperationOutcome::SelectionChanged => {
                        &mut aggregate.outcomes.selection_changed
                    }
                    WalletOperationOutcome::Cancelled => &mut aggregate.outcomes.cancelled,
                    WalletOperationOutcome::NoChanges => &mut aggregate.outcomes.no_changes,
                };
                *count = count.saturating_add(1);
            }
        }
        aggregate
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletOperationTimelineError {
    InvalidCapacity,
    Unavailable,
}

impl fmt::Display for WalletOperationTimelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCapacity => "operation timeline capacity is outside the bounded range",
            Self::Unavailable => "operation timeline is unavailable",
        })
    }
}

impl Error for WalletOperationTimelineError {}

/// Bounded diagnostics and metrics evidence shared by incoming adapters.
///
/// This process-local ring is not authoritative, durable, or replayable;
/// typed wallet projections and checkpoints remain the source of truth.
#[derive(Clone)]
pub struct WalletOperationTimeline {
    state: Arc<Mutex<WalletOperationTimelineState>>,
}

struct WalletOperationTimelineState {
    capacity: usize,
    started: std::time::Instant,
    next_sequence: u64,
    total_records: u64,
    evicted_records: u64,
    records: VecDeque<WalletOperationRecord>,
}

impl fmt::Debug for WalletOperationTimeline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletOperationTimeline(..)")
    }
}

impl Default for WalletOperationTimeline {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_WALLET_OPERATION_TIMELINE_CAPACITY)
            .expect("default operation timeline capacity is valid")
    }
}

impl WalletOperationTimeline {
    pub fn with_capacity(capacity: usize) -> Result<Self, WalletOperationTimelineError> {
        if capacity == 0 || capacity > MAX_WALLET_OPERATION_TIMELINE_CAPACITY {
            return Err(WalletOperationTimelineError::InvalidCapacity);
        }
        Ok(Self {
            state: Arc::new(Mutex::new(WalletOperationTimelineState {
                capacity,
                started: std::time::Instant::now(),
                next_sequence: 1,
                total_records: 0,
                evicted_records: 0,
                records: VecDeque::with_capacity(capacity),
            })),
        })
    }

    pub fn query(&self) -> Result<WalletOperationTimelineSnapshot, WalletOperationTimelineError> {
        let state = self
            .state
            .lock()
            .map_err(|_| WalletOperationTimelineError::Unavailable)?;
        Ok(WalletOperationTimelineSnapshot {
            capacity: state.capacity,
            total_records: state.total_records,
            evicted_records: state.evicted_records,
            records: state.records.iter().cloned().collect(),
        })
    }

    // Keeping every closed envelope field explicit prevents an extensible
    // attribute bag from entering this privacy boundary.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record(
        &self,
        operation_id: WalletOperationId,
        correlation_id: WalletOperationCorrelationId,
        caused_by: Option<WalletOperationCausationId>,
        resource: WalletOperationResource,
        trigger: WalletOperationTrigger,
        attempt: WalletOperationAttempt,
        duration: WalletOperationDurationMillis,
        event: WalletOperationEvent,
    ) -> Result<WalletOperationCausationId, WalletOperationTimelineError> {
        self.record_with_measurements(
            operation_id,
            correlation_id,
            caused_by,
            resource,
            trigger,
            attempt,
            duration,
            WalletOperationResourceMeasurements::default(),
            event,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_with_measurements(
        &self,
        operation_id: WalletOperationId,
        correlation_id: WalletOperationCorrelationId,
        caused_by: Option<WalletOperationCausationId>,
        resource: WalletOperationResource,
        trigger: WalletOperationTrigger,
        attempt: WalletOperationAttempt,
        duration: WalletOperationDurationMillis,
        measurements: WalletOperationResourceMeasurements,
        event: WalletOperationEvent,
    ) -> Result<WalletOperationCausationId, WalletOperationTimelineError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletOperationTimelineError::Unavailable)?;
        let sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.saturating_add(1);
        state.total_records = state.total_records.saturating_add(1);
        let causation_id = causation_id(sequence);
        let timestamp = WalletOperationTimestampMillis::bounded(state.started.elapsed());
        if state.records.len() == state.capacity {
            state.records.pop_front();
            state.evicted_records = state.evicted_records.saturating_add(1);
        }
        state.records.push_back(WalletOperationRecord {
            sequence,
            operation_id,
            correlation_id,
            causation_id,
            caused_by,
            resource,
            trigger,
            attempt,
            timestamp: Some(timestamp),
            duration,
            measurements,
            event,
        });
        Ok(causation_id)
    }

    pub(crate) fn begin_operation(
        &self,
        resource: WalletOperationResource,
        trigger: WalletOperationTrigger,
    ) -> Result<
        (
            WalletOperationId,
            WalletOperationCorrelationId,
            WalletOperationCausationId,
        ),
        WalletOperationTimelineError,
    > {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WalletOperationTimelineError::Unavailable)?;
        let sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.saturating_add(1);
        state.total_records = state.total_records.saturating_add(1);
        let operation_id = operation_id(sequence);
        let correlation_id = correlation_id(sequence);
        let causation_id = causation_id(sequence);
        let timestamp = WalletOperationTimestampMillis::bounded(state.started.elapsed());
        if state.records.len() == state.capacity {
            state.records.pop_front();
            state.evicted_records = state.evicted_records.saturating_add(1);
        }
        state.records.push_back(WalletOperationRecord {
            sequence,
            operation_id,
            correlation_id,
            causation_id,
            caused_by: None,
            resource,
            trigger,
            attempt: WalletOperationAttempt::new(1).expect("one is a valid attempt"),
            timestamp: Some(timestamp),
            duration: WalletOperationDurationMillis::zero(),
            measurements: WalletOperationResourceMeasurements::default(),
            event: WalletOperationEvent::Admitted,
        });
        Ok((operation_id, correlation_id, causation_id))
    }
}

/// Shared presentation-neutral snapshot query for headless and UI adapters.
pub trait GetWalletOperationTimelineUseCase: Send + Sync {
    fn execute(&self) -> Result<WalletOperationTimelineSnapshot, WalletOperationTimelineError>;
}

impl GetWalletOperationTimelineUseCase for WalletOperationTimeline {
    fn execute(&self) -> Result<WalletOperationTimelineSnapshot, WalletOperationTimelineError> {
        self.query()
    }
}

fn operation_id(value: u64) -> WalletOperationId {
    let mut bytes = [0_u8; 16];
    bytes[8..].copy_from_slice(&value.to_be_bytes());
    WalletOperationId::from_bytes(bytes)
}

fn correlation_id(value: u64) -> WalletOperationCorrelationId {
    let mut bytes = [0_u8; 16];
    bytes[8..].copy_from_slice(&value.to_be_bytes());
    WalletOperationCorrelationId::from_bytes(bytes)
}

fn causation_id(value: u64) -> WalletOperationCausationId {
    WalletOperationCausationId::from_bytes(value.to_be_bytes())
}

pub(crate) const fn timeline_effect_outcome(
    outcome: WalletRealmEffectOutcome,
) -> WalletOperationOutcome {
    match outcome {
        WalletRealmEffectOutcome::Current => WalletOperationOutcome::Succeeded,
        WalletRealmEffectOutcome::InProgress => WalletOperationOutcome::InProgress,
        WalletRealmEffectOutcome::Stale => WalletOperationOutcome::Stale,
        WalletRealmEffectOutcome::Missing => WalletOperationOutcome::Missing,
        WalletRealmEffectOutcome::Blocked => WalletOperationOutcome::Blocked,
        WalletRealmEffectOutcome::Unsupported => WalletOperationOutcome::Unsupported,
    }
}

#[cfg(test)]
mod tests;
