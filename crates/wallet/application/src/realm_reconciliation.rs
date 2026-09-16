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
}
