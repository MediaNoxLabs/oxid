// SPDX-License-Identifier: Apache-2.0

//! Closed, payload-free application boundary for the optional support journal.
//!
//! This module defines no persistence, encryption, transport, UI, or incoming
//! use case. Compositions may omit the capability by using the no-op sink.

/// Current durable support-journal record schema.
pub const SUPPORT_JOURNAL_SCHEMA_VERSION: u16 = 1;
/// Maximum queued records before best-effort capture drops an event.
pub const MAX_SUPPORT_JOURNAL_PENDING_RECORDS: usize = 128;
/// Maximum records retained by one durable journal epoch.
pub const MAX_SUPPORT_JOURNAL_DURABLE_RECORDS: usize = 4_096;
/// Maximum encoded archive size, including framing and indexes.
pub const MAX_SUPPORT_JOURNAL_DURABLE_BYTES: usize = 2 * 1_024 * 1_024;
/// Maximum accepted records in one second.
pub const MAX_SUPPORT_JOURNAL_RECORDS_PER_SECOND: u16 = 20;
/// Maximum accepted records in one minute.
pub const MAX_SUPPORT_JOURNAL_RECORDS_PER_MINUTE: u16 = 200;
/// Maximum records written in one flush batch.
pub const MAX_SUPPORT_JOURNAL_FLUSH_BATCH: usize = 32;
/// Maximum delay before a non-empty batch is flushed.
pub const MAX_SUPPORT_JOURNAL_FLUSH_LATENCY_SECONDS: u64 = 2;
/// Maximum encrypted export-bundle size after framing.
pub const MAX_SUPPORT_JOURNAL_EXPORT_BYTES: usize = 1_024 * 1_024;
/// Maximum records returned by one read page.
pub const MAX_SUPPORT_JOURNAL_PAGE_RECORDS: usize = 100;
/// Maximum explicitly enabled capture window.
pub const MAX_SUPPORT_JOURNAL_CAPTURE_HOURS: u64 = 24;
/// Maximum archive retention after the last recorded event.
pub const MAX_SUPPORT_JOURNAL_RETENTION_DAYS: u64 = 7;

macro_rules! closed_labels {
    ($name:ident { $($variant:ident => $label:literal),+ $(,)? }) => {
        impl $name {
            /// Every admitted value in stable declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Stable payload-free wire and presentation label.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $label),+ }
            }
        }
    };
}

/// Closed subsystem owning an operational journal event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SupportJournalSubsystem {
    Wallet,
    Midnight,
    Identity,
    Credential,
    Presentation,
    Platform,
    Runtime,
}

closed_labels!(SupportJournalSubsystem {
    Wallet => "wallet",
    Midnight => "midnight",
    Identity => "identity",
    Credential => "credential",
    Presentation => "presentation",
    Platform => "platform",
    Runtime => "runtime",
});

/// Closed lifecycle marker; it never contains the underlying action or intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SupportJournalCode {
    OperationStarted,
    ConsentRequested,
    ConsentResolved,
    CustodyAuthorizationRequested,
    CustodyAuthorizationResolved,
    ProofStageChanged,
    SubmissionStageChanged,
    OperationTerminal,
    CaptureDropped,
    ClockDiscontinuity,
    ArchiveCorruptionDetected,
}

closed_labels!(SupportJournalCode {
    OperationStarted => "operation.started",
    ConsentRequested => "consent.requested",
    ConsentResolved => "consent.resolved",
    CustodyAuthorizationRequested => "custody.authorization.requested",
    CustodyAuthorizationResolved => "custody.authorization.resolved",
    ProofStageChanged => "proof.stage.changed",
    SubmissionStageChanged => "submission.stage.changed",
    OperationTerminal => "operation.terminal",
    CaptureDropped => "capture.dropped",
    ClockDiscontinuity => "clock.discontinuity",
    ArchiveCorruptionDetected => "archive.corruption.detected",
});

/// Closed event severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SupportJournalSeverity {
    Info,
    Warning,
    Error,
}

closed_labels!(SupportJournalSeverity {
    Info => "info",
    Warning => "warning",
    Error => "error",
});

/// Closed lifecycle stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SupportJournalStage {
    Started,
    AwaitingConsent,
    AwaitingAuthorization,
    Proving,
    Submitting,
    Terminal,
}

closed_labels!(SupportJournalStage {
    Started => "started",
    AwaitingConsent => "awaiting_consent",
    AwaitingAuthorization => "awaiting_authorization",
    Proving => "proving",
    Submitting => "submitting",
    Terminal => "terminal",
});

/// Closed operational outcome. This is never transaction or protocol authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SupportJournalOutcome {
    Pending,
    Succeeded,
    Refused,
    Cancelled,
    Failed,
    OutcomeUnknown,
    Dropped,
}

closed_labels!(SupportJournalOutcome {
    Pending => "pending",
    Succeeded => "succeeded",
    Refused => "refused",
    Cancelled => "cancelled",
    Failed => "failed",
    OutcomeUnknown => "outcome_unknown",
    Dropped => "dropped",
});

/// Random journal-session correlation token, local to one archive epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SupportJournalSessionEpoch([u8; 16]);

impl SupportJournalSessionEpoch {
    /// Wraps 128 bits produced by a reviewed random source at composition time.
    #[must_use]
    pub const fn from_random_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the fixed-size token for an encrypted adapter.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Random correlation token scoped to one action in one journal epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SupportJournalActionToken([u8; 16]);

impl SupportJournalActionToken {
    /// Wraps 128 bits produced by a reviewed random source at action start.
    #[must_use]
    pub const fn from_random_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the fixed-size token for an encrypted adapter.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// One versioned, closed, payload-free support-journal event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupportJournalEvent {
    schema_version: u16,
    sequence: u64,
    subsystem: SupportJournalSubsystem,
    code: SupportJournalCode,
    severity: SupportJournalSeverity,
    stage: SupportJournalStage,
    outcome: SupportJournalOutcome,
    session_epoch: SupportJournalSessionEpoch,
    action_token: Option<SupportJournalActionToken>,
    coarse_unix_minute: Option<u64>,
}

impl SupportJournalEvent {
    /// Creates one event. Callers cannot attach strings, identifiers, or payloads.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        sequence: u64,
        subsystem: SupportJournalSubsystem,
        code: SupportJournalCode,
        severity: SupportJournalSeverity,
        stage: SupportJournalStage,
        outcome: SupportJournalOutcome,
        session_epoch: SupportJournalSessionEpoch,
        action_token: Option<SupportJournalActionToken>,
        coarse_unix_minute: Option<u64>,
    ) -> Self {
        Self {
            schema_version: SUPPORT_JOURNAL_SCHEMA_VERSION,
            sequence,
            subsystem,
            code,
            severity,
            stage,
            outcome,
            session_epoch,
            action_token,
            coarse_unix_minute,
        }
    }

    /// Current record schema version.
    #[must_use]
    pub const fn schema_version(self) -> u16 {
        self.schema_version
    }
    /// Device-local monotonic sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Closed owning subsystem.
    #[must_use]
    pub const fn subsystem(self) -> SupportJournalSubsystem {
        self.subsystem
    }
    /// Closed lifecycle code.
    #[must_use]
    pub const fn code(self) -> SupportJournalCode {
        self.code
    }
    /// Closed severity.
    #[must_use]
    pub const fn severity(self) -> SupportJournalSeverity {
        self.severity
    }
    /// Closed lifecycle stage.
    #[must_use]
    pub const fn stage(self) -> SupportJournalStage {
        self.stage
    }
    /// Closed operational outcome.
    #[must_use]
    pub const fn outcome(self) -> SupportJournalOutcome {
        self.outcome
    }
    /// Random local journal epoch.
    #[must_use]
    pub const fn session_epoch(self) -> SupportJournalSessionEpoch {
        self.session_epoch
    }
    /// Optional random action correlation token.
    #[must_use]
    pub const fn action_token(self) -> Option<SupportJournalActionToken> {
        self.action_token
    }
    /// Optional display/filter minute, never authoritative ordering.
    #[must_use]
    pub const fn coarse_unix_minute(self) -> Option<u64> {
        self.coarse_unix_minute
    }
}

/// Closed best-effort result. Backend errors never cross the application port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportJournalRecordResult {
    Recorded,
    Dropped,
    Disabled,
}

/// Best-effort outgoing sink for closed journal events.
pub trait SupportJournalEventSinkPort: Send + Sync {
    fn try_record(&self, event: SupportJournalEvent) -> SupportJournalRecordResult;
}

/// Sink used when the optional durable capability is not composed.
pub struct NoopSupportJournalSink;

impl SupportJournalEventSinkPort for NoopSupportJournalSink {
    fn try_record(&self, _: SupportJournalEvent) -> SupportJournalRecordResult {
        SupportJournalRecordResult::Disabled
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn sample_event() -> SupportJournalEvent {
        SupportJournalEvent::new(
            7,
            SupportJournalSubsystem::Identity,
            SupportJournalCode::ConsentResolved,
            SupportJournalSeverity::Info,
            SupportJournalStage::Terminal,
            SupportJournalOutcome::Refused,
            SupportJournalSessionEpoch::from_random_bytes([1; 16]),
            Some(SupportJournalActionToken::from_random_bytes([2; 16])),
            Some(42),
        )
    }

    #[test]
    fn closed_labels_are_unique_and_payload_free() {
        for labels in [
            SupportJournalSubsystem::ALL
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>(),
            SupportJournalCode::ALL
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>(),
            SupportJournalSeverity::ALL
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>(),
            SupportJournalStage::ALL
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>(),
            SupportJournalOutcome::ALL
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>(),
        ] {
            assert_eq!(
                labels.len(),
                labels.iter().copied().collect::<BTreeSet<_>>().len()
            );
            assert!(labels.iter().all(|label| {
                !label.is_empty()
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte == b'.' || byte == b'_')
            }));
        }
    }

    #[test]
    fn event_round_trips_only_the_reviewed_fields() {
        let event = sample_event();
        assert_eq!(event.schema_version(), 1);
        assert_eq!(event.sequence(), 7);
        assert_eq!(event.subsystem(), SupportJournalSubsystem::Identity);
        assert_eq!(event.code(), SupportJournalCode::ConsentResolved);
        assert_eq!(event.severity(), SupportJournalSeverity::Info);
        assert_eq!(event.stage(), SupportJournalStage::Terminal);
        assert_eq!(event.outcome(), SupportJournalOutcome::Refused);
        assert_eq!(event.session_epoch().into_bytes(), [1; 16]);
        assert_eq!(
            event
                .action_token()
                .map(SupportJournalActionToken::into_bytes),
            Some([2; 16])
        );
        assert_eq!(event.coarse_unix_minute(), Some(42));
    }

    #[test]
    fn optional_fields_can_be_absent() {
        let event = SupportJournalEvent::new(
            1,
            SupportJournalSubsystem::Runtime,
            SupportJournalCode::OperationStarted,
            SupportJournalSeverity::Info,
            SupportJournalStage::Started,
            SupportJournalOutcome::Pending,
            SupportJournalSessionEpoch::from_random_bytes([3; 16]),
            None,
            None,
        );
        assert_eq!(event.action_token(), None);
        assert_eq!(event.coarse_unix_minute(), None);
    }

    #[test]
    fn constants_match_the_accepted_resource_envelope() {
        assert_eq!(MAX_SUPPORT_JOURNAL_PENDING_RECORDS, 128);
        assert_eq!(MAX_SUPPORT_JOURNAL_DURABLE_RECORDS, 4_096);
        assert_eq!(MAX_SUPPORT_JOURNAL_DURABLE_BYTES, 2_097_152);
        assert_eq!(MAX_SUPPORT_JOURNAL_RECORDS_PER_SECOND, 20);
        assert_eq!(MAX_SUPPORT_JOURNAL_RECORDS_PER_MINUTE, 200);
        assert_eq!(MAX_SUPPORT_JOURNAL_FLUSH_BATCH, 32);
        assert_eq!(MAX_SUPPORT_JOURNAL_FLUSH_LATENCY_SECONDS, 2);
        assert_eq!(MAX_SUPPORT_JOURNAL_EXPORT_BYTES, 1_048_576);
        assert_eq!(MAX_SUPPORT_JOURNAL_PAGE_RECORDS, 100);
        assert_eq!(MAX_SUPPORT_JOURNAL_CAPTURE_HOURS, 24);
        assert_eq!(MAX_SUPPORT_JOURNAL_RETENTION_DAYS, 7);
    }

    #[test]
    fn omitted_capability_is_explicitly_disabled() {
        assert_eq!(
            NoopSupportJournalSink.try_record(sample_event()),
            SupportJournalRecordResult::Disabled,
        );
    }
}
