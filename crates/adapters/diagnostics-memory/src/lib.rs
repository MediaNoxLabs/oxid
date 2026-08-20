// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{collections::BTreeMap, collections::VecDeque, sync::Mutex};

use oxid_diagnostics_application::{
    DiagnosticCode, DiagnosticCountView, DiagnosticEventSinkPort, DiagnosticEventView,
    DiagnosticRepositoryPort, DiagnosticSeverity, DiagnosticSnapshotView, DiagnosticsError,
    MAX_DIAGNOSTIC_CAPACITY,
};

/// Default number of payload-free events retained by one application process.
pub const DEFAULT_DIAGNOSTIC_CAPACITY: usize = 256;

#[derive(Default)]
struct DiagnosticState {
    next_sequence: u64,
    total_events: u64,
    evicted_events: u64,
    counts: BTreeMap<(DiagnosticCode, DiagnosticSeverity), u64>,
    recent: VecDeque<DiagnosticEventView>,
}

/// Bounded, non-persistent store for closed diagnostic codes.
pub struct InMemoryDiagnosticStore {
    capacity: usize,
    state: Mutex<DiagnosticState>,
}

impl Default for InMemoryDiagnosticStore {
    fn default() -> Self {
        Self::new(DEFAULT_DIAGNOSTIC_CAPACITY).expect("default diagnostic capacity is valid")
    }
}

impl InMemoryDiagnosticStore {
    pub fn new(capacity: usize) -> Result<Self, DiagnosticsError> {
        if capacity == 0 || capacity > MAX_DIAGNOSTIC_CAPACITY {
            return Err(DiagnosticsError::Unavailable);
        }
        Ok(Self {
            capacity,
            state: Mutex::new(DiagnosticState::default()),
        })
    }
}

impl DiagnosticEventSinkPort for InMemoryDiagnosticStore {
    fn record(&self, code: DiagnosticCode, severity: DiagnosticSeverity) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.next_sequence = state.next_sequence.saturating_add(1);
        state.total_events = state.total_events.saturating_add(1);
        let sequence = state.next_sequence;
        let occurrences = state.counts.entry((code, severity)).or_default();
        *occurrences = occurrences.saturating_add(1);
        if state.recent.len() == self.capacity {
            state.recent.pop_front();
            state.evicted_events = state.evicted_events.saturating_add(1);
        }
        state
            .recent
            .push_back(DiagnosticEventView::new(sequence, code, severity));
    }
}

impl DiagnosticRepositoryPort for InMemoryDiagnosticStore {
    fn snapshot(&self) -> Result<DiagnosticSnapshotView, DiagnosticsError> {
        let state = self
            .state
            .lock()
            .map_err(|_| DiagnosticsError::Unavailable)?;
        let counts = state
            .counts
            .iter()
            .map(|((code, severity), occurrences)| {
                DiagnosticCountView::new(*code, *severity, *occurrences)
            })
            .collect();
        Ok(DiagnosticSnapshotView::new(
            self.capacity,
            state.total_events,
            state.evicted_events,
            counts,
            state.recent.iter().copied().collect(),
        ))
    }

    fn clear(&self) -> Result<u64, DiagnosticsError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DiagnosticsError::Unavailable)?;
        let cleared = u64::try_from(state.recent.len()).unwrap_or(u64::MAX);
        *state = DiagnosticState::default();
        Ok(cleared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_ring_retains_newest_events_and_aggregate_counts() {
        let store = InMemoryDiagnosticStore::new(2).expect("store");
        store.record(
            DiagnosticCode::HeadlessRequestRejected,
            DiagnosticSeverity::Warning,
        );
        store.record(
            DiagnosticCode::MidnightDustSyncFailed,
            DiagnosticSeverity::Error,
        );
        store.record(
            DiagnosticCode::HeadlessRequestRejected,
            DiagnosticSeverity::Warning,
        );

        let snapshot = store.snapshot().expect("snapshot");
        assert_eq!(snapshot.capacity(), 2);
        assert_eq!(snapshot.total_events(), 3);
        assert_eq!(snapshot.evicted_events(), 1);
        assert_eq!(
            snapshot
                .recent()
                .iter()
                .map(|event| event.sequence())
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(snapshot.counts().iter().any(|count| {
            count.code() == DiagnosticCode::HeadlessRequestRejected
                && count.severity() == DiagnosticSeverity::Warning
                && count.occurrences() == 2
        }));
    }

    #[test]
    fn clear_removes_events_and_aggregate_history() {
        let store = InMemoryDiagnosticStore::new(1).expect("store");
        store.record(
            DiagnosticCode::MidnightTransferWorkerTerminated,
            DiagnosticSeverity::Error,
        );
        assert_eq!(store.clear().expect("clear"), 1);
        let snapshot = store.snapshot().expect("snapshot");
        assert_eq!(snapshot.total_events(), 0);
        assert!(snapshot.counts().is_empty());
        assert!(snapshot.recent().is_empty());
    }

    #[test]
    fn capacity_is_strictly_bounded() {
        assert!(InMemoryDiagnosticStore::new(0).is_err());
        assert!(InMemoryDiagnosticStore::new(MAX_DIAGNOSTIC_CAPACITY + 1).is_err());
    }
}
