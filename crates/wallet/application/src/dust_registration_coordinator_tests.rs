// SPDX-License-Identifier: Apache-2.0

use oxid_wallet_domain::{ChainNetworkId, WalletProfileId};

use super::*;

fn identity(generation: u64) -> Identity {
    Identity {
        profile: WalletProfileId::parse("profile_test").unwrap(),
        realm: ChainNetworkId::parse("undeployed").unwrap(),
        generation,
    }
}

fn other_identity() -> Identity {
    Identity {
        profile: WalletProfileId::parse("profile_other").unwrap(),
        realm: ChainNetworkId::parse("undeployed").unwrap(),
        generation: 1,
    }
}

fn draft() -> WalletTransactionDraftId {
    WalletTransactionDraftId::parse("dustreg_test").unwrap()
}

fn other_draft() -> WalletTransactionDraftId {
    WalletTransactionDraftId::parse("dustreg_other").unwrap()
}

fn transaction() -> ChainTransactionId {
    ChainTransactionId::parse("tx_test").unwrap()
}

fn eligible(coordinator: WalletDustRegistrationCoordinator) -> WalletDustRegistrationCoordinator {
    coordinator.reduce(Event::Eligibility {
        identity: identity(1),
        revision: 1,
        eligible: true,
    })
}

#[test]
fn success_requires_explicit_authorization_and_dust_refresh() {
    let eligible = eligible(WalletDustRegistrationCoordinator::default());
    assert_eq!(
        eligible.active_effect(),
        Some(&WalletDustRegistrationEffect::Prepare {
            identity: identity(1),
        })
    );

    let prepared = eligible.reduce(completion::prepared(identity(1), draft(), 1));
    assert_eq!(
        prepared.active_effect(),
        Some(
            &WalletDustRegistrationEffect::RequestProtectedAuthorization {
                identity: identity(1),
                draft_id: draft(),
            }
        )
    );

    let authorized = prepared.reduce(completion::authorized(identity(1), draft()));
    assert_eq!(
        authorized.active_effect(),
        Some(&WalletDustRegistrationEffect::Submit {
            identity: identity(1),
            draft_id: draft(),
        })
    );

    let submitted = authorized.reduce(completion::submitted(identity(1), draft(), transaction()));
    assert_eq!(
        submitted.active_effect(),
        Some(&WalletDustRegistrationEffect::ObserveTransaction {
            identity: identity(1),
            transaction_id: transaction(),
        })
    );

    let included = submitted.reduce(completion::reconciled(
        identity(1),
        transaction(),
        1,
        Reconciliation::Included,
    ));
    assert_eq!(included.projection().state, State::Reconciling);
    assert_eq!(
        included.active_effect(),
        Some(&WalletDustRegistrationEffect::RefreshDust {
            identity: identity(1),
            transaction_id: transaction(),
        })
    );

    let ready = included.reduce(completion::dust_refreshed(
        identity(1),
        transaction(),
        1,
        1,
        true,
    ));
    assert_eq!(ready.projection().state, State::Ready);
    assert_eq!(ready.active_effect(), None);
}

#[test]
fn rejection_and_duplicate_completions_do_not_advance_or_duplicate_effects() {
    let eligible = eligible(WalletDustRegistrationCoordinator::default());
    let duplicate_eligibility = eligible.clone().reduce(Event::Eligibility {
        identity: identity(1),
        revision: 1,
        eligible: true,
    });
    assert_eq!(duplicate_eligibility, eligible);

    let prepared = eligible.reduce(completion::prepared(identity(1), draft(), 1));
    let wrong_draft = prepared
        .clone()
        .reduce(completion::authorized(identity(1), other_draft()));
    assert_eq!(wrong_draft, prepared);

    let rejected = prepared.reduce(completion::authorization_rejected(identity(1), draft()));
    assert_eq!(rejected.projection().state, State::ActionRequired);
    assert_eq!(rejected.active_effect(), None);
}

#[test]
fn stale_identity_completions_are_noops_after_supersession() {
    let prepared = eligible(WalletDustRegistrationCoordinator::default())
        .reduce(completion::prepared(identity(1), draft(), 1));
    let unrelated = prepared
        .clone()
        .reduce(completion::authorized(other_identity(), draft()));
    assert_eq!(unrelated, prepared);

    let superseded = prepared.reduce(Event::Superseded {
        identity: identity(1),
    });
    let replacement = superseded.reduce(Event::Eligibility {
        identity: identity(2),
        revision: 1,
        eligible: true,
    });
    let stale = replacement
        .clone()
        .reduce(completion::authorized(identity(1), draft()));
    assert_eq!(stale, replacement);
    assert_eq!(
        stale.active_effect(),
        Some(&WalletDustRegistrationEffect::Prepare {
            identity: identity(2),
        })
    );
}

#[test]
fn dropped_registration_is_observed_until_abandoned_before_reprepare() {
    let submitted = eligible(WalletDustRegistrationCoordinator::default())
        .reduce(completion::prepared(identity(1), draft(), 1))
        .reduce(completion::authorized(identity(1), draft()))
        .reduce(completion::submitted(identity(1), draft(), transaction()));
    let dropped = submitted.reduce(completion::reconciled(
        identity(1),
        transaction(),
        1,
        Reconciliation::Dropped,
    ));
    assert_eq!(dropped.projection().state, State::ActionRequired);
    assert_eq!(
        dropped.active_effect(),
        Some(&WalletDustRegistrationEffect::ObserveTransaction {
            identity: identity(1),
            transaction_id: transaction(),
        })
    );

    let abandoned = dropped.reduce(completion::dropped_registration_abandoned(
        identity(1),
        transaction(),
        2,
    ));
    assert_eq!(
        abandoned.active_effect(),
        Some(&WalletDustRegistrationEffect::Prepare {
            identity: identity(1),
        })
    );
}

#[test]
fn eligibility_returning_after_rejection_permits_a_fresh_prepare() {
    let prepared = eligible(WalletDustRegistrationCoordinator::default())
        .reduce(completion::prepared(identity(1), draft(), 1));
    let ineligible = prepared.reduce(Event::Eligibility {
        identity: identity(1),
        revision: 2,
        eligible: false,
    });
    let rejected = ineligible.reduce(completion::authorization_rejected(identity(1), draft()));
    assert_eq!(rejected.projection().state, State::NotEligible);
    assert_eq!(rejected.active_effect(), None);

    let eligible_again = rejected.reduce(Event::Eligibility {
        identity: identity(1),
        revision: 3,
        eligible: true,
    });
    assert_eq!(
        eligible_again.active_effect(),
        Some(&WalletDustRegistrationEffect::Prepare {
            identity: identity(1),
        })
    );

    let rejected_again = eligible_again
        .reduce(completion::prepared(identity(1), draft(), 2))
        .reduce(completion::authorization_rejected(identity(1), draft()));
    let stale_eligibility = rejected_again.clone().reduce(Event::Eligibility {
        identity: identity(1),
        revision: 3,
        eligible: true,
    });
    assert_eq!(stale_eligibility, rejected_again);
}

#[test]
fn stale_terminal_completion_preserves_the_current_effect() {
    let eligible = eligible(WalletDustRegistrationCoordinator::default());
    let stale = eligible.clone().reduce(completion::authorization_rejected(
        identity(1),
        other_draft(),
    ));
    assert_eq!(stale, eligible);
}

#[test]
fn resurfaced_primary_resumes_the_parked_replacement_after_abandonment() {
    let primary = eligible(WalletDustRegistrationCoordinator::default())
        .reduce(completion::prepared(identity(1), draft(), 1))
        .reduce(completion::authorized(identity(1), draft()))
        .reduce(completion::submitted(identity(1), draft(), transaction()))
        .reduce(completion::reconciled(
            identity(1),
            transaction(),
            2,
            Reconciliation::Dropped,
        ))
        .reduce(completion::dropped_registration_abandoned(
            identity(1),
            transaction(),
            3,
        ));
    let replacement = primary.reduce(completion::prepared(identity(1), other_draft(), 2));
    let resurfaced =
        replacement.reduce(completion::finality_observed(identity(1), transaction(), 4));
    let dropped_again = resurfaced.reduce(completion::reconciled(
        identity(1),
        transaction(),
        5,
        Reconciliation::Dropped,
    ));
    let rejected_replacement = dropped_again
        .clone()
        .reduce(completion::authorization_rejected(
            identity(1),
            other_draft(),
        ));
    assert_eq!(
        rejected_replacement.active_effect(),
        Some(&WalletDustRegistrationEffect::ObserveTransaction {
            identity: identity(1),
            transaction_id: transaction(),
        })
    );
    let abandoned_again = dropped_again.reduce(completion::dropped_registration_abandoned(
        identity(1),
        transaction(),
        6,
    ));

    assert_eq!(
        abandoned_again.active_effect(),
        Some(
            &WalletDustRegistrationEffect::RequestProtectedAuthorization {
                identity: identity(1),
                draft_id: other_draft(),
            }
        )
    );
}
