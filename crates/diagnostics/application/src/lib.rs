// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt, sync::Arc};

/// Exact confirmation required before an incoming adapter clears the local
/// diagnostic buffer.
pub const CLEAR_LOCAL_DIAGNOSTICS_INTENT: &str = "CLEAR_LOCAL_DIAGNOSTICS";

/// Hard ceiling for one process-local diagnostic ring.
pub const MAX_DIAGNOSTIC_CAPACITY: usize = 1_024;

/// Closed, payload-free event codes admitted by the diagnostics boundary.
///
/// The enum intentionally has no custom string or metadata variant. Adapter
/// errors, endpoints, profile identifiers, credentials, transaction material,
/// and external response bodies therefore cannot cross this boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticCode {
    HeadlessRequestRejected,
    HeadlessMethodNotFound,
    MidnightDustSyncFailed,
    MidnightDustSyncWorkerPanicked,
    MidnightDustSyncWorkerSpawnFailed,
    MidnightShieldedSyncFailed,
    MidnightShieldedSyncWorkerPanicked,
    MidnightShieldedSyncWorkerSpawnFailed,
    MidnightTransferWorkerTerminated,
    MidnightTransferWorkerSpawnFailed,
    MidnightContractCallWorkerPanicked,
}

impl DiagnosticCode {
    /// Returns the stable wire/UI representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HeadlessRequestRejected => "headless.request.rejected",
            Self::HeadlessMethodNotFound => "headless.method.not_found",
            Self::MidnightDustSyncFailed => "midnight.dust.sync.failed",
            Self::MidnightDustSyncWorkerPanicked => "midnight.dust.sync.worker_panicked",
            Self::MidnightDustSyncWorkerSpawnFailed => "midnight.dust.sync.worker_spawn_failed",
            Self::MidnightShieldedSyncFailed => "midnight.shielded.sync.failed",
            Self::MidnightShieldedSyncWorkerPanicked => "midnight.shielded.sync.worker_panicked",
            Self::MidnightShieldedSyncWorkerSpawnFailed => {
                "midnight.shielded.sync.worker_spawn_failed"
            }
            Self::MidnightTransferWorkerTerminated => "midnight.transfer.worker_terminated",
            Self::MidnightTransferWorkerSpawnFailed => "midnight.transfer.worker_spawn_failed",
            Self::MidnightContractCallWorkerPanicked => "midnight.vault_call.worker_panicked",
        }
    }
}

/// Stable severity without a free-form logging level or target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

impl DiagnosticSeverity {
    /// Returns the stable wire/UI representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// One retained payload-free diagnostic event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticEventView {
    sequence: u64,
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
}

impl DiagnosticEventView {
    #[must_use]
    pub const fn new(sequence: u64, code: DiagnosticCode, severity: DiagnosticSeverity) -> Self {
        Self {
            sequence,
            code,
            severity,
        }
    }

    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn code(self) -> DiagnosticCode {
        self.code
    }

    #[must_use]
    pub const fn severity(self) -> DiagnosticSeverity {
        self.severity
    }
}

/// Aggregate count for one closed code/severity pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticCountView {
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
    occurrences: u64,
}

impl DiagnosticCountView {
    #[must_use]
    pub const fn new(code: DiagnosticCode, severity: DiagnosticSeverity, occurrences: u64) -> Self {
        Self {
            code,
            severity,
            occurrences,
        }
    }

    #[must_use]
    pub const fn code(self) -> DiagnosticCode {
        self.code
    }

    #[must_use]
    pub const fn severity(self) -> DiagnosticSeverity {
        self.severity
    }

    #[must_use]
    pub const fn occurrences(self) -> u64 {
        self.occurrences
    }
}

/// Bounded process-local diagnostic snapshot returned to incoming adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticSnapshotView {
    capacity: usize,
    total_events: u64,
    evicted_events: u64,
    counts: Vec<DiagnosticCountView>,
    recent: Vec<DiagnosticEventView>,
}

impl DiagnosticSnapshotView {
    #[must_use]
    pub fn new(
        capacity: usize,
        total_events: u64,
        evicted_events: u64,
        counts: Vec<DiagnosticCountView>,
        recent: Vec<DiagnosticEventView>,
    ) -> Self {
        Self {
            capacity,
            total_events,
            evicted_events,
            counts,
            recent,
        }
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub const fn total_events(&self) -> u64 {
        self.total_events
    }

    #[must_use]
    pub const fn evicted_events(&self) -> u64 {
        self.evicted_events
    }

    #[must_use]
    pub fn counts(&self) -> &[DiagnosticCountView] {
        &self.counts
    }

    #[must_use]
    pub fn recent(&self) -> &[DiagnosticEventView] {
        &self.recent
    }
}

/// Stable store failures. No poisoned-lock or backend details are projected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticsError {
    Unavailable,
    ConfirmationRequired,
}

impl fmt::Display for DiagnosticsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "local diagnostics are unavailable",
            Self::ConfirmationRequired => "clearing local diagnostics requires confirmation",
        })
    }
}

impl Error for DiagnosticsError {}

/// Best-effort outgoing sink used only with closed event codes.
pub trait DiagnosticEventSinkPort: Send + Sync {
    fn record(&self, code: DiagnosticCode, severity: DiagnosticSeverity);
}

/// Process-local repository used by the diagnostic snapshot service.
pub trait DiagnosticRepositoryPort: DiagnosticEventSinkPort {
    fn snapshot(&self) -> Result<DiagnosticSnapshotView, DiagnosticsError>;

    fn clear(&self) -> Result<u64, DiagnosticsError>;
}

/// Incoming diagnostic snapshot use case.
pub trait GetDiagnosticSnapshotUseCase: Send + Sync {
    fn execute(&self) -> Result<DiagnosticSnapshotView, DiagnosticsError>;
}

/// Confirmation input for clearing the process-local diagnostic ring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClearDiagnosticsCommand {
    pub confirmed: bool,
    pub intent: String,
}

/// Public result of clearing the process-local ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClearedDiagnosticsView {
    pub cleared_events: u64,
}

/// Incoming diagnostic reset use case.
pub trait ClearDiagnosticsUseCase: Send + Sync {
    fn execute(
        &self,
        command: ClearDiagnosticsCommand,
    ) -> Result<ClearedDiagnosticsView, DiagnosticsError>;
}

/// Application service that exposes a repository through read/reset use cases.
pub struct DiagnosticsService<R> {
    repository: Arc<R>,
}

impl<R> DiagnosticsService<R> {
    #[must_use]
    pub const fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

impl<R> GetDiagnosticSnapshotUseCase for DiagnosticsService<R>
where
    R: DiagnosticRepositoryPort + 'static,
{
    fn execute(&self) -> Result<DiagnosticSnapshotView, DiagnosticsError> {
        self.repository.snapshot()
    }
}

impl<R> ClearDiagnosticsUseCase for DiagnosticsService<R>
where
    R: DiagnosticRepositoryPort + 'static,
{
    fn execute(
        &self,
        command: ClearDiagnosticsCommand,
    ) -> Result<ClearedDiagnosticsView, DiagnosticsError> {
        if !command.confirmed || command.intent != CLEAR_LOCAL_DIAGNOSTICS_INTENT {
            return Err(DiagnosticsError::ConfirmationRequired);
        }
        self.repository
            .clear()
            .map(|cleared_events| ClearedDiagnosticsView { cleared_events })
    }
}

/// Fail-silent sink used by adapters before composition attaches diagnostics.
pub struct NoopDiagnosticEventSink;

impl DiagnosticEventSinkPort for NoopDiagnosticEventSink {
    fn record(&self, _: DiagnosticCode, _: DiagnosticSeverity) {}
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct TestRepository {
        events: Mutex<Vec<(DiagnosticCode, DiagnosticSeverity)>>,
    }

    impl DiagnosticEventSinkPort for TestRepository {
        fn record(&self, code: DiagnosticCode, severity: DiagnosticSeverity) {
            self.events
                .lock()
                .expect("events lock")
                .push((code, severity));
        }
    }

    impl DiagnosticRepositoryPort for TestRepository {
        fn snapshot(&self) -> Result<DiagnosticSnapshotView, DiagnosticsError> {
            let events = self
                .events
                .lock()
                .map_err(|_| DiagnosticsError::Unavailable)?;
            Ok(DiagnosticSnapshotView::new(
                8,
                u64::try_from(events.len()).unwrap_or(u64::MAX),
                0,
                Vec::new(),
                events
                    .iter()
                    .enumerate()
                    .map(|(index, (code, severity))| {
                        DiagnosticEventView::new(
                            u64::try_from(index + 1).unwrap_or(u64::MAX),
                            *code,
                            *severity,
                        )
                    })
                    .collect(),
            ))
        }

        fn clear(&self) -> Result<u64, DiagnosticsError> {
            let mut events = self
                .events
                .lock()
                .map_err(|_| DiagnosticsError::Unavailable)?;
            let cleared = u64::try_from(events.len()).unwrap_or(u64::MAX);
            events.clear();
            Ok(cleared)
        }
    }

    #[test]
    fn clear_requires_the_exact_confirmation() {
        let repository = Arc::new(TestRepository::default());
        repository.record(
            DiagnosticCode::HeadlessRequestRejected,
            DiagnosticSeverity::Warning,
        );
        let service = DiagnosticsService::new(repository.clone());

        assert_eq!(
            ClearDiagnosticsUseCase::execute(
                &service,
                ClearDiagnosticsCommand {
                    confirmed: true,
                    intent: "clear".to_owned(),
                },
            ),
            Err(DiagnosticsError::ConfirmationRequired)
        );
        assert_eq!(repository.snapshot().expect("snapshot").total_events(), 1);

        assert_eq!(
            ClearDiagnosticsUseCase::execute(
                &service,
                ClearDiagnosticsCommand {
                    confirmed: true,
                    intent: CLEAR_LOCAL_DIAGNOSTICS_INTENT.to_owned(),
                },
            )
            .expect("clear"),
            ClearedDiagnosticsView { cleared_events: 1 }
        );
    }

    #[test]
    fn diagnostic_codes_have_only_static_payload_free_representations() {
        assert_eq!(
            DiagnosticCode::MidnightDustSyncWorkerPanicked.as_str(),
            "midnight.dust.sync.worker_panicked"
        );
        assert_eq!(DiagnosticSeverity::Error.as_str(), "error");
    }
}
