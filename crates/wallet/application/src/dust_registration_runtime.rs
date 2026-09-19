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

/// Current format accepted by [`WalletDustRegistrationRuntime::restore`].
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationRuntime {
    coordinator: WalletDustRegistrationCoordinator,
    admitted_effect: Option<(
        WalletDustRegistrationEffect,
        WalletDustRegistrationRuntimeAdmissionToken,
    )>,
    runtime_instance: u64,
    next_admission_sequence: u64,
}

impl Default for WalletDustRegistrationRuntime {
    fn default() -> Self {
        Self {
            coordinator: WalletDustRegistrationCoordinator::default(),
            admitted_effect: None,
            runtime_instance: next_runtime_instance(),
            next_admission_sequence: 0,
        }
    }
}

impl WalletDustRegistrationRuntime {
    #[must_use]
    pub fn coordinator(&self) -> &WalletDustRegistrationCoordinator {
        &self.coordinator
    }

    /// Captures quiescent application state without any in-flight admission.
    #[must_use]
    pub fn quiescent_checkpoint(&self) -> WalletDustRegistrationRuntimeCheckpoint {
        WalletDustRegistrationRuntimeCheckpoint {
            version: WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION,
            coordinator: self.coordinator.clone(),
        }
    }

    /// Restores a supported quiescent checkpoint and requires executors to re-admit work.
    pub fn restore_quiescent_checkpoint(
        checkpoint: WalletDustRegistrationRuntimeCheckpoint,
    ) -> Result<Self, WalletDustRegistrationRuntimeRestoreError> {
        if checkpoint.version != WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION {
            return Err(
                WalletDustRegistrationRuntimeRestoreError::UnsupportedCheckpointVersion {
                    found: checkpoint.version,
                },
            );
        }
        Ok(Self {
            coordinator: checkpoint.coordinator,
            admitted_effect: None,
            runtime_instance: next_runtime_instance(),
            next_admission_sequence: 0,
        })
    }

    /// Applies an external policy observation without starting work.
    pub fn observe(&mut self, event: WalletDustRegistrationSettlementEvent) {
        let next = self.coordinator.clone().reduce(event);
        if next.projection() != self.coordinator.projection() {
            self.coordinator = next;
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
        let next = self.coordinator.clone().reduce(event);
        if next.projection() == self.coordinator.projection() {
            return false;
        }
        self.coordinator = next;
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
}

fn next_runtime_instance() -> u64 {
    static NEXT_RUNTIME_INSTANCE: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(1);
    NEXT_RUNTIME_INSTANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
        let stale_token = admitted_token(runtime.admit_current(&prepare));

        let mut restored = WalletDustRegistrationRuntime::restore_quiescent_checkpoint(
            runtime.quiescent_checkpoint(),
        )
        .unwrap();
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
        let first_token = admitted_token(runtime.admit_current(&prepare));
        let checkpoint = runtime.quiescent_checkpoint();
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
            runtime.quiescent_checkpoint(),
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
            runtime.quiescent_checkpoint(),
        )
        .unwrap();
        assert!(matches!(
            included.coordinator().active_effect(),
            Some(WalletDustRegistrationEffect::RefreshDust { transaction_id, .. })
                if transaction_id == &transaction
        ));
    }

    #[test]
    fn rejects_unknown_checkpoint_versions() {
        let mut checkpoint = WalletDustRegistrationRuntime::default().quiescent_checkpoint();
        checkpoint.version = WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION + 1;
        assert_eq!(
            WalletDustRegistrationRuntime::restore_quiescent_checkpoint(checkpoint),
            Err(
                WalletDustRegistrationRuntimeRestoreError::UnsupportedCheckpointVersion {
                    found: WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION + 1,
                }
            )
        );
    }
}
