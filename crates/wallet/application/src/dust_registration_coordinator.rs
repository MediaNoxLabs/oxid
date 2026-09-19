// SPDX-License-Identifier: Apache-2.0

//! Presentation-neutral single-flight coordinator for DUST registration.

use oxid_wallet_domain::{ChainTransactionId, WalletTransactionDraftId};

use crate::{
    WalletDustRegistrationSettlementEvent as Event,
    WalletDustRegistrationSettlementIdentity as Identity,
    WalletDustRegistrationSettlementProjection as Projection,
    WalletDustRegistrationSettlementReconciliation as Reconciliation,
    WalletDustRegistrationSettlementState as State, reduce_wallet_dust_registration_settlement,
};

/// One externally executed operation for the current registration identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationEffect {
    Prepare {
        identity: Identity,
    },
    RequestProtectedAuthorization {
        identity: Identity,
        draft_id: WalletTransactionDraftId,
    },
    Submit {
        identity: Identity,
        draft_id: WalletTransactionDraftId,
    },
    ObserveTransaction {
        identity: Identity,
        transaction_id: ChainTransactionId,
    },
    RefreshDust {
        identity: Identity,
        transaction_id: ChainTransactionId,
    },
}

/// The public coordinator state; adapter execution remains outside this module.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WalletDustRegistrationCoordinator {
    projection: Projection,
    active_effect: Option<WalletDustRegistrationEffect>,
}

impl WalletDustRegistrationCoordinator {
    #[must_use]
    pub fn projection(&self) -> &Projection {
        &self.projection
    }

    #[must_use]
    pub fn active_effect(&self) -> Option<&WalletDustRegistrationEffect> {
        self.active_effect.as_ref()
    }

    /// Applies one policy event and projects its single next external effect.
    #[must_use]
    pub fn reduce(mut self, event: Event) -> Self {
        let previous = self.projection;
        self.projection = reduce_wallet_dust_registration_settlement(&previous, event.clone());
        let accepted = self.projection != previous;
        let eligibility_permits_prepare =
            accepted && matches!(&event, Event::Eligibility { eligible: true, .. });
        let authorization_stopped = accepted
            && matches!(
                &event,
                Event::AuthorizationRejected { .. } | Event::Cancelled { .. }
            );
        self.active_effect = next_effect(
            &self.projection,
            eligibility_permits_prepare,
            authorization_stopped,
        );
        self
    }
}

fn next_effect(
    projection: &Projection,
    eligibility_permits_prepare: bool,
    authorization_stopped: bool,
) -> Option<WalletDustRegistrationEffect> {
    let identity = projection.identity.clone()?;
    let registration = projection.registration.as_ref();
    match projection.state {
        State::ActionRequired
            if authorization_stopped
                && registration
                    .is_none_or(|registration| registration.abandonment_revision > 0) =>
        {
            None
        }
        State::ActionRequired => {
            if let Some(parked) = projection.parked_registration() {
                return registration_effect(identity, parked);
            }
            match registration {
                Some(registration) if registration.abandonment_revision > 0 => {
                    Some(WalletDustRegistrationEffect::Prepare { identity })
                }
                Some(registration) => registration_effect(identity, registration),
                None if projection.preparation_revision == 0 || eligibility_permits_prepare => {
                    Some(WalletDustRegistrationEffect::Prepare { identity })
                }
                None => None,
            }
        }
        State::AwaitingAuthorization => registration.map(|registration| {
            WalletDustRegistrationEffect::RequestProtectedAuthorization {
                identity,
                draft_id: registration.draft_id.clone(),
            }
        }),
        State::Submitting => {
            registration.map(|registration| WalletDustRegistrationEffect::Submit {
                identity,
                draft_id: registration.draft_id.clone(),
            })
        }
        State::Confirming => registration
            .and_then(|registration| registration.transaction_id.clone())
            .map(
                |transaction_id| WalletDustRegistrationEffect::ObserveTransaction {
                    identity,
                    transaction_id,
                },
            ),
        State::Reconciling => registration
            .and_then(|registration| {
                registration
                    .transaction_id
                    .clone()
                    .map(|transaction_id| (registration.included, transaction_id))
            })
            .map(|(included, transaction_id)| {
                if included {
                    WalletDustRegistrationEffect::RefreshDust {
                        identity,
                        transaction_id,
                    }
                } else {
                    WalletDustRegistrationEffect::ObserveTransaction {
                        identity,
                        transaction_id,
                    }
                }
            }),
        _ => None,
    }
}

fn registration_effect(
    identity: Identity,
    registration: &crate::WalletDustRegistrationSettlementRegistration,
) -> Option<WalletDustRegistrationEffect> {
    use crate::WalletDustRegistrationSettlementAuthorizationPhase as AuthorizationPhase;

    match registration.authorization_phase {
        AuthorizationPhase::AwaitingAuthorization => Some(
            WalletDustRegistrationEffect::RequestProtectedAuthorization {
                identity,
                draft_id: registration.draft_id.clone(),
            },
        ),
        AuthorizationPhase::Submitting => Some(WalletDustRegistrationEffect::Submit {
            identity,
            draft_id: registration.draft_id.clone(),
        }),
        AuthorizationPhase::Submitted => {
            registration.transaction_id.clone().map(|transaction_id| {
                if registration.included {
                    WalletDustRegistrationEffect::RefreshDust {
                        identity,
                        transaction_id,
                    }
                } else {
                    WalletDustRegistrationEffect::ObserveTransaction {
                        identity,
                        transaction_id,
                    }
                }
            })
        }
    }
}

/// Typed completion constructors keep adapter inputs bounded to public identities.
pub mod completion {
    use super::*;

    #[must_use]
    pub fn prepared(
        identity: Identity,
        draft_id: WalletTransactionDraftId,
        revision: u64,
    ) -> Event {
        Event::AuthorizationRequested {
            identity,
            draft_id,
            preparation_revision: revision,
        }
    }

    #[must_use]
    pub fn authorized(identity: Identity, draft_id: WalletTransactionDraftId) -> Event {
        Event::AuthorizationSucceeded { identity, draft_id }
    }

    #[must_use]
    pub fn authorization_rejected(identity: Identity, draft_id: WalletTransactionDraftId) -> Event {
        Event::AuthorizationRejected { identity, draft_id }
    }

    #[must_use]
    pub fn submitted(
        identity: Identity,
        draft_id: WalletTransactionDraftId,
        transaction_id: ChainTransactionId,
    ) -> Event {
        Event::SubmissionAccepted {
            identity,
            draft_id,
            transaction_id,
        }
    }

    #[must_use]
    pub fn finality_observed(
        identity: Identity,
        transaction_id: ChainTransactionId,
        revision: u64,
    ) -> Event {
        Event::FinalityObserved {
            identity,
            transaction_id,
            revision,
        }
    }

    #[must_use]
    pub fn reconciled(
        identity: Identity,
        transaction_id: ChainTransactionId,
        revision: u64,
        reconciliation: Reconciliation,
    ) -> Event {
        Event::RegistrationReconciled {
            identity,
            transaction_id,
            revision,
            reconciliation,
        }
    }

    #[must_use]
    pub fn dropped_registration_abandoned(
        identity: Identity,
        transaction_id: ChainTransactionId,
        after_observation_revision: u64,
    ) -> Event {
        Event::DroppedRegistrationAbandoned {
            identity,
            transaction_id,
            after_observation_revision,
        }
    }

    #[must_use]
    pub fn dust_refreshed(
        identity: Identity,
        transaction_id: ChainTransactionId,
        revision: u64,
        after_observation_revision: u64,
        ready: bool,
    ) -> Event {
        Event::DustRefreshed {
            identity,
            transaction_id,
            revision,
            after_observation_revision,
            ready,
        }
    }
}

#[cfg(test)]
#[path = "dust_registration_coordinator_tests.rs"]
mod tests;
