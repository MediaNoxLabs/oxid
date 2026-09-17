// SPDX-License-Identifier: Apache-2.0

//! Presentation-neutral, deterministic watches for selected-realm actions.
//!
//! Adapters supply observations and monotonic time. This module neither polls
//! transport nor stores transaction payloads.

use crate::WalletRealmLifecycleIdentity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmittedTransactionWatchId(String);

impl SubmittedTransactionWatchId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingArrivalWatchId(String);

impl IncomingArrivalWatchId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// The two watch kinds deliberately use distinct public identity types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletActionWatch {
    SubmittedTransaction {
        transaction: SubmittedTransactionWatchId,
    },
    IncomingArrival {
        arrival: IncomingArrivalWatchId,
        starting_checkpoint: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletActionWatchObservation {
    SubmittedTransaction {
        transaction: SubmittedTransactionWatchId,
    },
    IncomingArrival {
        arrival: IncomingArrivalWatchId,
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
    pub state: WalletActionWatchState,
    pub realm_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveWatch {
    watch: WalletActionWatch,
    identity: WalletRealmLifecycleIdentity,
    generation: u64,
    deadline_millis: u64,
}

/// One selected-realm-generation watch owner.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalletActionWatchRuntime {
    identity: Option<WalletRealmLifecycleIdentity>,
    generation: u64,
    active: Option<ActiveWatch>,
    projection: Option<WalletActionWatchProjection>,
}

impl WalletActionWatchRuntime {
    /// Changes realm generation and supersedes an active watch from the prior one.
    pub fn select_realm(&mut self, identity: WalletRealmLifecycleIdentity) {
        if self.identity.as_ref() != Some(&identity) {
            self.finish(WalletActionWatchState::Superseded);
            self.generation = self.generation.saturating_add(1);
            self.identity = Some(identity);
        }
    }

    /// Admits exactly one watch for the currently selected realm generation.
    pub fn admit(&mut self, watch: WalletActionWatch, deadline_millis: u64, now_millis: u64) {
        self.finish(WalletActionWatchState::Superseded);
        let Some(identity) = self.identity.clone() else {
            self.projection = Some(WalletActionWatchProjection {
                state: WalletActionWatchState::Degraded,
                realm_generation: self.generation,
            });
            return;
        };
        if deadline_millis <= now_millis {
            self.projection = Some(WalletActionWatchProjection {
                state: WalletActionWatchState::Expired,
                realm_generation: self.generation,
            });
            return;
        }
        self.active = Some(ActiveWatch {
            watch,
            identity,
            generation: self.generation,
            deadline_millis,
        });
        self.projection = Some(WalletActionWatchProjection {
            state: WalletActionWatchState::Waiting,
            realm_generation: self.generation,
        });
    }

    pub fn observe(&mut self, observation: WalletActionWatchObservation, now_millis: u64) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        if now_millis >= active.deadline_millis {
            self.finish(WalletActionWatchState::Expired);
        } else if matches_watch(&active.watch, &observation) {
            self.finish(WalletActionWatchState::Confirmed);
        }
    }

    pub fn timeout(&mut self, now_millis: u64) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| now_millis >= active.deadline_millis)
        {
            self.finish(WalletActionWatchState::Expired);
        }
    }

    pub fn cancel(&mut self) {
        self.finish(WalletActionWatchState::Cancelled);
    }
    pub fn offline(&mut self) {
        self.finish(WalletActionWatchState::Offline);
    }
    pub fn degraded(&mut self) {
        self.finish(WalletActionWatchState::Degraded);
    }

    /// Suspension retains the watch; callers resume by continuing observations.
    pub fn suspend(&mut self) {}
    pub fn resume(&mut self) {}

    #[must_use]
    pub fn projection(&self) -> Option<&WalletActionWatchProjection> {
        self.projection.as_ref()
    }

    fn finish(&mut self, state: WalletActionWatchState) {
        if let Some(active) = self.active.take() {
            debug_assert_eq!(active.generation, self.generation);
            debug_assert_eq!(self.identity.as_ref(), Some(&active.identity));
            self.projection = Some(WalletActionWatchProjection {
                state,
                realm_generation: active.generation,
            });
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
                arrival: expected,
                starting_checkpoint,
            },
            WalletActionWatchObservation::IncomingArrival {
                arrival: observed,
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
            transaction: SubmittedTransactionWatchId::new(id),
        }
    }

    #[test]
    fn outgoing_requires_the_exact_transaction_identity() {
        let mut runtime = WalletActionWatchRuntime::default();
        runtime.select_realm(realm("preprod"));
        runtime.admit(outgoing("tx-a"), 10, 1);
        runtime.observe(
            WalletActionWatchObservation::SubmittedTransaction {
                transaction: SubmittedTransactionWatchId::new("tx-b"),
            },
            2,
        );
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Waiting
        );
        runtime.observe(
            WalletActionWatchObservation::SubmittedTransaction {
                transaction: SubmittedTransactionWatchId::new("tx-a"),
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
        runtime.admit(
            WalletActionWatch::IncomingArrival {
                arrival: IncomingArrivalWatchId::new("account-a"),
                starting_checkpoint: 9,
            },
            20,
            1,
        );
        for checkpoint in [8, 9] {
            runtime.observe(
                WalletActionWatchObservation::IncomingArrival {
                    arrival: IncomingArrivalWatchId::new("account-a"),
                    checkpoint,
                },
                2,
            );
        }
        runtime.observe(
            WalletActionWatchObservation::IncomingArrival {
                arrival: IncomingArrivalWatchId::new("account-b"),
                checkpoint: 10,
            },
            2,
        );
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Waiting
        );
        runtime.observe(
            WalletActionWatchObservation::IncomingArrival {
                arrival: IncomingArrivalWatchId::new("account-a"),
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
        runtime.admit(outgoing("tx"), 2, 1);
        runtime.timeout(2);
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Expired
        );
        runtime.admit(outgoing("tx"), 4, 2);
        runtime.cancel();
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Cancelled
        );
        runtime.admit(outgoing("tx"), 5, 2);
        runtime.offline();
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Offline
        );
        runtime.admit(outgoing("tx"), 5, 2);
        runtime.select_realm(realm("mainnet"));
        assert_eq!(
            runtime.projection().unwrap().state,
            WalletActionWatchState::Superseded
        );
    }
}
