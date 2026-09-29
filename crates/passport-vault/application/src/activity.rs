// SPDX-License-Identifier: Apache-2.0

//! Privacy-safe, process-local Passport Vault activity.
//!
//! This projection is intentionally not a protocol event store. It retains a
//! bounded set of public operation facts for the current process and is erased
//! on restart. Credential claims, proof material, keys, addresses, transaction
//! payloads, and unrestricted adapter errors have no representation here.

use std::{
    collections::{BTreeMap, VecDeque},
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

use oxid_platform_ports::ClockPort;

use crate::PassportVaultCallKind;

/// Maximum number of retained Vault operations across all profiles.
pub const MAX_PASSPORT_VAULT_ACTIVITY_RECORDS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PassportVaultActivityId(u64);

impl PassportVaultActivityId {
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultActivitySource {
    StandaloneVault,
    MidnightContractCall,
}

impl PassportVaultActivitySource {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::StandaloneVault => "standalone_vault",
            Self::MidnightContractCall => "midnight_contract_call",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultActivityStatus {
    Pending,
    Confirmed,
    Failed,
    Refused,
    Cancelled,
    TimedOut,
    OutcomeUnknown,
}

impl PassportVaultActivityStatus {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Refused => "refused",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Confirmed | Self::Failed | Self::Refused | Self::Cancelled
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultActivityFinality {
    Pending,
    Final,
    Unknown,
}

impl PassportVaultActivityFinality {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Final => "final",
            Self::Unknown => "unknown",
        }
    }
}

impl From<PassportVaultActivityStatus> for PassportVaultActivityFinality {
    fn from(status: PassportVaultActivityStatus) -> Self {
        match status {
            PassportVaultActivityStatus::Pending => Self::Pending,
            PassportVaultActivityStatus::TimedOut | PassportVaultActivityStatus::OutcomeUnknown => {
                Self::Unknown
            }
            PassportVaultActivityStatus::Confirmed
            | PassportVaultActivityStatus::Failed
            | PassportVaultActivityStatus::Refused
            | PassportVaultActivityStatus::Cancelled => Self::Final,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultActivityRecord {
    pub id: PassportVaultActivityId,
    pub profile_id: String,
    pub source: PassportVaultActivitySource,
    pub operation: PassportVaultCallKind,
    pub status: PassportVaultActivityStatus,
    pub finality: PassportVaultActivityFinality,
    pub observed_at_millis: Option<u64>,
    pub lock_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultActivityView {
    pub source: String,
    pub retention: String,
    pub records: Vec<PassportVaultActivityRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultActivityError {
    Unavailable,
}

impl fmt::Display for PassportVaultActivityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Passport Vault activity is unavailable")
    }
}

impl Error for PassportVaultActivityError {}

pub trait ListPassportVaultActivityUseCase: Send + Sync {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<PassportVaultActivityView, PassportVaultActivityError>;
}

#[derive(Default)]
struct PassportVaultActivityState {
    next_id: u64,
    records: VecDeque<PassportVaultActivityRecord>,
    contract_drafts: BTreeMap<String, PassportVaultActivityId>,
}

/// Shared producer/read-model boundary used by every Vault application path.
pub struct PassportVaultActivityStore {
    clock: Arc<dyn ClockPort>,
    state: Mutex<PassportVaultActivityState>,
}

impl PassportVaultActivityStore {
    #[must_use]
    pub fn new(clock: Arc<dyn ClockPort>) -> Self {
        Self {
            clock,
            state: Mutex::new(PassportVaultActivityState::default()),
        }
    }

    pub(crate) fn begin(
        &self,
        profile_id: String,
        source: PassportVaultActivitySource,
        operation: PassportVaultCallKind,
        lock_id: Option<u64>,
    ) -> Option<PassportVaultActivityId> {
        self.begin_inner(None, profile_id, source, operation, lock_id)
    }

    pub(crate) fn begin_contract(
        &self,
        draft_id: String,
        profile_id: String,
        operation: PassportVaultCallKind,
        lock_id: Option<u64>,
    ) -> Option<PassportVaultActivityId> {
        self.begin_inner(
            Some(draft_id),
            profile_id,
            PassportVaultActivitySource::MidnightContractCall,
            operation,
            lock_id,
        )
    }

    fn begin_inner(
        &self,
        contract_draft: Option<String>,
        profile_id: String,
        source: PassportVaultActivitySource,
        operation: PassportVaultCallKind,
        lock_id: Option<u64>,
    ) -> Option<PassportVaultActivityId> {
        let mut state = self.state.lock().ok()?;
        if let Some(existing) = contract_draft
            .as_ref()
            .and_then(|draft| state.contract_drafts.get(draft))
        {
            return Some(*existing);
        }
        state.next_id = state.next_id.checked_add(1)?;
        let id = PassportVaultActivityId(state.next_id);
        let observed_at_millis = self.clock.now().ok().map(|value| value.value());
        state.records.push_back(PassportVaultActivityRecord {
            id,
            profile_id,
            source,
            operation,
            status: PassportVaultActivityStatus::Pending,
            finality: PassportVaultActivityFinality::Pending,
            observed_at_millis,
            lock_id,
        });
        if let Some(draft) = contract_draft {
            state.contract_drafts.insert(draft, id);
        }
        while state.records.len() > MAX_PASSPORT_VAULT_ACTIVITY_RECORDS {
            let Some(evicted) = state.records.pop_front() else {
                break;
            };
            state
                .contract_drafts
                .retain(|_, value| *value != evicted.id);
        }
        Some(id)
    }

    pub(crate) fn update(&self, id: PassportVaultActivityId, status: PassportVaultActivityStatus) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let Some(record) = state.records.iter_mut().find(|record| record.id == id) else {
            return;
        };
        if record.status == status || record.status.is_terminal() {
            return;
        }
        record.status = status;
        record.finality = status.into();
        if let Ok(now) = self.clock.now() {
            record.observed_at_millis = Some(now.value());
        }
    }

    pub(crate) fn update_contract(&self, draft_id: &str, status: PassportVaultActivityStatus) {
        let id = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.contract_drafts.get(draft_id).copied());
        if let Some(id) = id {
            self.update(id, status);
        }
    }
}

impl ListPassportVaultActivityUseCase for PassportVaultActivityStore {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<PassportVaultActivityView, PassportVaultActivityError> {
        let state = self
            .state
            .lock()
            .map_err(|_| PassportVaultActivityError::Unavailable)?;
        Ok(PassportVaultActivityView {
            source: "application_event_projection".to_owned(),
            retention: "process_local_bounded_not_backed_up".to_owned(),
            records: state
                .records
                .iter()
                .rev()
                .filter(|record| record.profile_id == profile_id)
                .cloned()
                .collect(),
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailablePassportVaultActivity;

impl ListPassportVaultActivityUseCase for UnavailablePassportVaultActivity {
    fn execute(&self, _: String) -> Result<PassportVaultActivityView, PassportVaultActivityError> {
        Err(PassportVaultActivityError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_foundation::UnixTimestampMillis;
    use oxid_platform_ports::PlatformError;

    struct Clock;

    impl ClockPort for Clock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(42))
        }
    }

    fn store() -> PassportVaultActivityStore {
        PassportVaultActivityStore::new(Arc::new(Clock))
    }

    #[test]
    fn duplicate_contract_admission_is_idempotent_and_terminal_state_is_monotonic() {
        let store = store();
        let first = store
            .begin_contract(
                "draft-1".to_owned(),
                "profile-1".to_owned(),
                PassportVaultCallKind::DepositToLock,
                Some(7),
            )
            .expect("activity id");
        let duplicate = store
            .begin_contract(
                "draft-1".to_owned(),
                "profile-1".to_owned(),
                PassportVaultCallKind::DepositToLock,
                Some(7),
            )
            .expect("same activity id");
        assert_eq!(first, duplicate);

        store.update(first, PassportVaultActivityStatus::Confirmed);
        store.update(first, PassportVaultActivityStatus::Pending);
        store.update(first, PassportVaultActivityStatus::Failed);

        let view = store.execute("profile-1".to_owned()).expect("projection");
        assert_eq!(view.records.len(), 1);
        assert_eq!(
            view.records[0].status,
            PassportVaultActivityStatus::Confirmed
        );
        assert_eq!(
            view.records[0].finality,
            PassportVaultActivityFinality::Final
        );
    }

    #[test]
    fn unknown_outcome_can_reconcile_to_confirmed_without_exposing_payloads() {
        let store = store();
        let unknown = store
            .begin(
                "profile-1".to_owned(),
                PassportVaultActivitySource::StandaloneVault,
                PassportVaultCallKind::ClaimFromLock,
                Some(9),
            )
            .expect("activity id");
        store.update(unknown, PassportVaultActivityStatus::OutcomeUnknown);
        store.update(unknown, PassportVaultActivityStatus::Confirmed);
        let timed_out = store
            .begin(
                "profile-1".to_owned(),
                PassportVaultActivitySource::StandaloneVault,
                PassportVaultCallKind::WithdrawFromLock,
                Some(10),
            )
            .expect("activity id");
        store.update(timed_out, PassportVaultActivityStatus::TimedOut);
        store.update(timed_out, PassportVaultActivityStatus::Confirmed);

        let view = store.execute("profile-1".to_owned()).expect("projection");
        assert_eq!(view.records.len(), 2);
        assert_eq!(
            view.records[0].status,
            PassportVaultActivityStatus::Confirmed
        );
        assert_eq!(
            view.records[1].status,
            PassportVaultActivityStatus::Confirmed
        );
        assert_eq!(view.retention, "process_local_bounded_not_backed_up");
    }

    #[test]
    fn evidence_backed_terminal_outcomes_are_distinct_and_monotonic() {
        let store = store();
        for (index, status) in [
            PassportVaultActivityStatus::Failed,
            PassportVaultActivityStatus::Refused,
            PassportVaultActivityStatus::Cancelled,
        ]
        .into_iter()
        .enumerate()
        {
            let id = store
                .begin(
                    "profile-1".to_owned(),
                    PassportVaultActivitySource::StandaloneVault,
                    PassportVaultCallKind::DepositToLock,
                    Some(index as u64),
                )
                .expect("activity id");
            store.update(id, status);
            store.update(id, PassportVaultActivityStatus::Pending);
        }

        let view = store.execute("profile-1".to_owned()).expect("projection");
        assert_eq!(view.records.len(), 3);
        assert_eq!(
            view.records
                .iter()
                .map(|record| (record.status, record.finality))
                .collect::<Vec<_>>(),
            vec![
                (
                    PassportVaultActivityStatus::Cancelled,
                    PassportVaultActivityFinality::Final,
                ),
                (
                    PassportVaultActivityStatus::Refused,
                    PassportVaultActivityFinality::Final,
                ),
                (
                    PassportVaultActivityStatus::Failed,
                    PassportVaultActivityFinality::Final,
                ),
            ]
        );
    }
}
