// SPDX-License-Identifier: Apache-2.0

//! Presentation- and executor-neutral admission boundary for DUST registration.

use crate::{
    WalletDustRegistrationCoordinator, WalletDustRegistrationEffect,
    WalletDustRegistrationSettlementEvent,
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

/// Result of attempting to admit an externally scheduled operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationRuntimeAdmission {
    Admitted(WalletDustRegistrationRuntimeOperation),
    Busy,
    Stale,
    Idle,
}

/// Single-flight composition around the pure coordinator.
///
/// This type owns neither I/O nor scheduling. Callers admit the current operation,
/// execute it through the matching application or protected-custody boundary, then
/// return its bounded coordinator event through [`Self::complete`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalletDustRegistrationRuntime {
    coordinator: WalletDustRegistrationCoordinator,
    admitted_effect: Option<WalletDustRegistrationEffect>,
}

impl WalletDustRegistrationRuntime {
    #[must_use]
    pub fn coordinator(&self) -> &WalletDustRegistrationCoordinator {
        &self.coordinator
    }

    /// Applies an external policy observation without starting work.
    pub fn observe(&mut self, event: WalletDustRegistrationSettlementEvent) {
        self.coordinator = self.coordinator.clone().reduce(event);
        if self.admitted_effect.as_ref() != self.coordinator.active_effect() {
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
        if self.admitted_effect.as_ref() == Some(effect) {
            return WalletDustRegistrationRuntimeAdmission::Busy;
        }
        self.admitted_effect = Some(effect.clone());
        WalletDustRegistrationRuntimeAdmission::Admitted(effect.clone().into())
    }

    /// Accepts a completion only from the operation that this runtime admitted.
    #[must_use]
    pub fn complete(
        &mut self,
        effect: &WalletDustRegistrationEffect,
        event: WalletDustRegistrationSettlementEvent,
    ) -> bool {
        if self.admitted_effect.as_ref() != Some(effect)
            || self.coordinator.active_effect() != Some(effect)
        {
            return false;
        }
        let next = self.coordinator.clone().reduce(event);
        if next == self.coordinator {
            return false;
        }
        self.coordinator = next;
        self.admitted_effect = None;
        true
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

    #[test]
    fn admits_each_current_effect_once_and_rejects_stale_completion_after_supersession() {
        let mut runtime = WalletDustRegistrationRuntime::default();
        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(1),
            revision: 1,
            eligible: true,
        });
        let prepare = runtime.coordinator().active_effect().unwrap().clone();
        assert!(matches!(
            runtime.admit_current(&prepare),
            WalletDustRegistrationRuntimeAdmission::Admitted(
                WalletDustRegistrationRuntimeOperation::Prepare(_)
            )
        ));
        assert_eq!(
            runtime.admit_current(&prepare),
            WalletDustRegistrationRuntimeAdmission::Busy
        );

        runtime.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity(2),
            revision: 1,
            eligible: true,
        });
        assert!(!runtime.complete(
            &prepare,
            completion::prepared(
                identity(1),
                oxid_wallet_domain::WalletTransactionDraftId::parse("dustreg_test").unwrap(),
                1
            ),
        ));
        let replacement = runtime.coordinator().active_effect().unwrap().clone();
        assert!(matches!(
            runtime.admit_current(&replacement),
            WalletDustRegistrationRuntimeAdmission::Admitted(
                WalletDustRegistrationRuntimeOperation::Prepare(_)
            )
        ));
    }
}
