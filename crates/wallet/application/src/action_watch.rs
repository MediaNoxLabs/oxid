// SPDX-License-Identifier: Apache-2.0

//! Presentation-neutral, deterministic watches for selected-realm actions.
//!
//! Adapters supply observations and monotonic time. This module neither polls
//! transport nor stores transaction payloads.

use oxid_wallet_domain::{ChainAccountId, ChainTransactionId};

use crate::WalletRealmLifecycleIdentity;

/// Admission identity returned to the adapter that owns one watch worker.
///
/// Requiring this handle on every later transition prevents an observation or
/// cancellation from a superseded realm generation from settling a newer
/// watch that happens to use the same public chain identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletActionWatchHandle {
    pub realm_generation: u64,
    pub sequence: u64,
}

/// The two watch kinds deliberately use distinct public identity types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletActionWatch {
    SubmittedTransaction {
        transaction: ChainTransactionId,
    },
    IncomingArrival {
        account: ChainAccountId,
        starting_checkpoint: u64,
    },
}

/// Payload-free action category retained by projections so incoming adapters
/// can show a watch only in the journey that owns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletActionWatchKind {
    SubmittedTransaction,
    IncomingArrival,
}

impl WalletActionWatch {
    #[must_use]
    pub const fn kind(&self) -> WalletActionWatchKind {
        match self {
            Self::SubmittedTransaction { .. } => WalletActionWatchKind::SubmittedTransaction,
            Self::IncomingArrival { .. } => WalletActionWatchKind::IncomingArrival,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletActionWatchObservation {
    SubmittedTransaction {
        transaction: ChainTransactionId,
    },
    IncomingArrival {
        account: ChainAccountId,
        checkpoint: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletActionWatchState {
    Waiting,
    Confirmed,
    Expired,
    Superseded,
    Offline,
    Degraded,
    Cancelled,
}

impl WalletActionWatchState {
    #[must_use]
    pub const fn terminal(self) -> bool {
        !matches!(self, Self::Waiting)
    }
}

/// Payload-free projection suitable for UI or headless adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletActionWatchProjection {
    pub kind: WalletActionWatchKind,
    pub state: WalletActionWatchState,
    pub realm_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveWatch {
    watch: WalletActionWatch,
    identity: WalletRealmLifecycleIdentity,
    handle: WalletActionWatchHandle,
    deadline_millis: u64,
}

/// One selected-realm-generation watch owner.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalletActionWatchRuntime {
    identity: Option<WalletRealmLifecycleIdentity>,
    generation: u64,
    sequence: u64,
    active: Option<ActiveWatch>,
    projection: Option<WalletActionWatchProjection>,
    suspended: bool,
}

impl WalletActionWatchRuntime {
    /// Changes realm generation and supersedes an active watch from the prior one.
    pub fn select_realm(&mut self, identity: WalletRealmLifecycleIdentity) {
        if self.identity.as_ref() != Some(&identity) {
            self.active = None;
            self.projection = None;
            self.suspended = false;
            self.generation = self.generation.saturating_add(1);
            self.identity = Some(identity);
        }
    }

    /// Admits exactly one watch for the currently selected realm generation.
    pub fn admit(
        &mut self,
        watch: WalletActionWatch,
        deadline_millis: u64,
        now_millis: u64,
    ) -> Option<WalletActionWatchHandle> {
        let kind = watch.kind();
        self.finish(WalletActionWatchState::Superseded);
        self.suspended = false;
        let Some(identity) = self.identity.clone() else {
            self.projection = Some(WalletActionWatchProjection {
                kind,
                state: WalletActionWatchState::Degraded,
                realm_generation: self.generation,
            });
            return None;
        };
        if deadline_millis <= now_millis {
            self.projection = Some(WalletActionWatchProjection {
                kind,
                state: WalletActionWatchState::Expired,
                realm_generation: self.generation,
            });
            return None;
        }
        self.sequence = self.sequence.saturating_add(1);
        let handle = WalletActionWatchHandle {
            realm_generation: self.generation,
            sequence: self.sequence,
        };
        self.active = Some(ActiveWatch {
            watch,
            identity,
            handle,
            deadline_millis,
        });
        self.projection = Some(WalletActionWatchProjection {
            kind,
            state: WalletActionWatchState::Waiting,
            realm_generation: self.generation,
        });
        Some(handle)
    }

    pub fn observe(
        &mut self,
        handle: WalletActionWatchHandle,
        observation: WalletActionWatchObservation,
        now_millis: u64,
    ) {
        if self.suspended {
            return;
        }
        let Some(active) = self.active.as_ref() else {
            return;
        };
        if active.handle != handle {
            return;
        }
        if now_millis >= active.deadline_millis {
            self.finish(WalletActionWatchState::Expired);
        } else if matches_watch(&active.watch, &observation) {
            self.finish(WalletActionWatchState::Confirmed);
        }
    }

    pub fn timeout(&mut self, handle: WalletActionWatchHandle, now_millis: u64) {
        if self.suspended {
            return;
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.handle == handle && now_millis >= active.deadline_millis)
        {
            self.finish(WalletActionWatchState::Expired);
        }
    }

    pub fn cancel(&mut self, handle: WalletActionWatchHandle) {
        self.finish_owned(handle, WalletActionWatchState::Cancelled);
    }
    pub fn offline(&mut self, handle: WalletActionWatchHandle) {
        self.finish_owned(handle, WalletActionWatchState::Offline);
    }
    pub fn degraded(&mut self, handle: WalletActionWatchHandle) {
        self.finish_owned(handle, WalletActionWatchState::Degraded);
    }

    /// Suspension retains the watch but gates every observation and timeout.
    pub fn suspend(&mut self, handle: WalletActionWatchHandle) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.handle == handle)
        {
            self.suspended = true;
        }
    }

    pub fn resume(&mut self, handle: WalletActionWatchHandle) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.handle == handle)
        {
            self.suspended = false;
        }
    }

    #[must_use]
    pub fn projection(&self) -> Option<&WalletActionWatchProjection> {
        self.projection.as_ref()
    }

    fn finish(&mut self, state: WalletActionWatchState) {
        if let Some(active) = self.active.take() {
            debug_assert_eq!(active.handle.realm_generation, self.generation);
            debug_assert_eq!(self.identity.as_ref(), Some(&active.identity));
            self.suspended = false;
            self.projection = Some(WalletActionWatchProjection {
                kind: active.watch.kind(),
                state,
                realm_generation: active.handle.realm_generation,
            });
        }
    }

    fn finish_owned(&mut self, handle: WalletActionWatchHandle, state: WalletActionWatchState) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.handle == handle)
        {
            self.finish(state);
        }
    }
}

fn matches_watch(watch: &WalletActionWatch, observation: &WalletActionWatchObservation) -> bool {
    match (watch, observation) {
        (
            WalletActionWatch::SubmittedTransaction {
                transaction: expected,
            },
            WalletActionWatchObservation::SubmittedTransaction {
                transaction: observed,
            },
        ) => expected == observed,
        (
            WalletActionWatch::IncomingArrival {
                account: expected,
                starting_checkpoint,
            },
            WalletActionWatchObservation::IncomingArrival {
                account: observed,
                checkpoint,
            },
        ) => expected == observed && checkpoint > starting_checkpoint,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn realm(name: &str) -> WalletRealmLifecycleIdentity {
        WalletRealmLifecycleIdentity::parse("profile", name).expect("valid realm")
    }
    fn outgoing(id: &str) -> WalletActionWatch {
        WalletActionWatch::SubmittedTransaction {
            transaction: ChainTransactionId::parse(id).expect("valid transaction id"),
        }
    }

    fn account(id: &str) -> ChainAccountId {
        ChainAccountId::parse(id).expect("valid account id")
    }

    #[test]
    fn outgoing_requires_the_exact_transaction_identity() {
        let mut runtime = WalletActionWatchRuntime::default();
        runtime.select_realm(realm("preprod"));
        let handle = runtime
            .admit(outgoing("tx-a"), 10, 1)
            .expect("watch admitted");
        runtime.observe(
            handle,
            WalletActionWatchObservation::SubmittedTransaction {
                transaction: ChainTransactionId::parse("tx-b").expect("valid transaction id"),
            },
            2,
        );
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Waiting
        );
        assert_eq!(
            runtime.projection().unwrap().kind,
            WalletActionWatchKind::SubmittedTransaction
        );
        runtime.observe(
            handle,
            WalletActionWatchObservation::SubmittedTransaction {
                transaction: ChainTransactionId::parse("tx-a").expect("valid transaction id"),
            },
            2,
        );
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Confirmed
        );
    }

    #[test]
    fn incoming_requires_a_matching_observation_after_its_checkpoint() {
        let mut runtime = WalletActionWatchRuntime::default();
        runtime.select_realm(realm("preprod"));
        let handle = runtime
            .admit(
                WalletActionWatch::IncomingArrival {
                    account: account("account-a"),
                    starting_checkpoint: 9,
                },
                20,
                1,
            )
            .expect("watch admitted");
        for checkpoint in [8, 9] {
            runtime.observe(
                handle,
                WalletActionWatchObservation::IncomingArrival {
                    account: account("account-a"),
                    checkpoint,
                },
                2,
            );
        }
        runtime.observe(
            handle,
            WalletActionWatchObservation::IncomingArrival {
                account: account("account-b"),
                checkpoint: 10,
            },
            2,
        );
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Waiting
        );
        assert_eq!(
            runtime.projection().unwrap().kind,
            WalletActionWatchKind::IncomingArrival
        );
        runtime.observe(
            handle,
            WalletActionWatchObservation::IncomingArrival {
                account: account("account-a"),
                checkpoint: 10,
            },
            2,
        );
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Confirmed
        );
    }

    #[test]
    fn expiry_cancel_connectivity_and_realm_switch_are_deterministic() {
        let mut runtime = WalletActionWatchRuntime::default();
        runtime.select_realm(realm("preprod"));
        let expired = runtime.admit(outgoing("tx"), 2, 1).expect("watch admitted");
        runtime.timeout(expired, 2);
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Expired
        );
        let cancelled = runtime.admit(outgoing("tx"), 4, 2).expect("watch admitted");
        runtime.cancel(cancelled);
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Cancelled
        );
        let offline = runtime.admit(outgoing("tx"), 5, 2).expect("watch admitted");
        runtime.offline(offline);
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Offline
        );
        runtime.admit(outgoing("tx"), 5, 2).expect("watch admitted");
        runtime.select_realm(realm("mainnet"));
        assert_eq!(runtime.projection(), None);
    }

    #[test]
    fn stale_realm_handle_cannot_settle_a_new_watch_with_the_same_identity() {
        let mut runtime = WalletActionWatchRuntime::default();
        runtime.select_realm(realm("realm-a"));
        let old = runtime
            .admit(outgoing("same-tx"), 20, 1)
            .expect("old watch admitted");
        runtime.select_realm(realm("realm-b"));
        let current = runtime
            .admit(outgoing("same-tx"), 20, 2)
            .expect("current watch admitted");
        assert_ne!(old.realm_generation, current.realm_generation);

        runtime.observe(
            old,
            WalletActionWatchObservation::SubmittedTransaction {
                transaction: ChainTransactionId::parse("same-tx").expect("valid transaction id"),
            },
            3,
        );
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Waiting
        );
        runtime.observe(
            current,
            WalletActionWatchObservation::SubmittedTransaction {
                transaction: ChainTransactionId::parse("same-tx").expect("valid transaction id"),
            },
            3,
        );
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Confirmed
        );
    }

    #[test]
    fn suspension_gates_observations_and_timeouts_until_resume() {
        let mut runtime = WalletActionWatchRuntime::default();
        runtime.select_realm(realm("preprod"));
        let handle = runtime
            .admit(outgoing("tx"), 10, 1)
            .expect("watch admitted");
        runtime.suspend(handle);
        runtime.observe(
            handle,
            WalletActionWatchObservation::SubmittedTransaction {
                transaction: ChainTransactionId::parse("tx").expect("valid transaction id"),
            },
            2,
        );
        runtime.timeout(handle, 10);
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Waiting
        );

        runtime.resume(handle);
        runtime.observe(
            handle,
            WalletActionWatchObservation::SubmittedTransaction {
                transaction: ChainTransactionId::parse("tx").expect("valid transaction id"),
            },
            3,
        );
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Confirmed
        );
    }

    #[test]
    fn stale_control_handle_cannot_cancel_or_degrade_a_newer_watch() {
        let mut runtime = WalletActionWatchRuntime::default();
        runtime.select_realm(realm("preprod"));
        let old = runtime
            .admit(outgoing("old"), 10, 1)
            .expect("old watch admitted");
        let current = runtime
            .admit(outgoing("current"), 10, 1)
            .expect("current watch admitted");
        runtime.cancel(old);
        runtime.degraded(old);
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Waiting
        );
        runtime.degraded(current);
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Degraded
        );
    }

    #[test]
    fn missing_realm_and_expired_admission_do_not_create_handles() {
        let mut runtime = WalletActionWatchRuntime::default();
        assert_eq!(runtime.admit(outgoing("tx"), 10, 1), None);
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Degraded
        );
        runtime.select_realm(realm("preprod"));
        assert_eq!(runtime.admit(outgoing("tx"), 5, 5), None);
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Expired
        );
    }
}
