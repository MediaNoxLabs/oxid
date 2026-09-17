// SPDX-License-Identifier: Apache-2.0

//! Compact, payload-free presentation for application-owned action watches.

use dioxus::prelude::*;
use oxid_wallet_application::{
    WalletActionWatchKind, WalletActionWatchProjection, WalletActionWatchState,
};

use crate::WalletUiServices;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WalletActionWatchContext {
    Send,
    Receive,
}

impl WalletActionWatchContext {
    const fn accepts(self, kind: WalletActionWatchKind) -> bool {
        matches!(
            (self, kind),
            (Self::Send, WalletActionWatchKind::SubmittedTransaction)
                | (Self::Receive, WalletActionWatchKind::IncomingArrival)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WalletActionWatchPresentation {
    class: &'static str,
    eyebrow: &'static str,
    title: &'static str,
    note: &'static str,
    busy: bool,
    alert: bool,
}

pub(super) fn action_watch_projection_for(
    projection: Option<WalletActionWatchProjection>,
    context: WalletActionWatchContext,
) -> Option<WalletActionWatchProjection> {
    projection.filter(|projection| context.accepts(projection.kind))
}

pub(super) fn action_watch_projection_from(
    services: &WalletUiServices,
    context: WalletActionWatchContext,
) -> Option<WalletActionWatchProjection> {
    action_watch_projection_for(
        services
            .manage_wallet_action_watch()
            .projection()
            .ok()
            .flatten(),
        context,
    )
}

const fn present(state: WalletActionWatchState) -> WalletActionWatchPresentation {
    match state {
        WalletActionWatchState::Waiting => WalletActionWatchPresentation {
            class: "is-waiting",
            eyebrow: "In progress",
            title: "Waiting for confirmation",
            note: "This updates automatically while the selected network is reachable.",
            busy: true,
            alert: false,
        },
        WalletActionWatchState::Confirmed => WalletActionWatchPresentation {
            class: "is-confirmed",
            eyebrow: "Confirmed",
            title: "Transfer confirmed",
            note: "Balances and activity are reconciling with the selected network.",
            busy: false,
            alert: false,
        },
        WalletActionWatchState::Expired => WalletActionWatchPresentation {
            class: "needs-attention",
            eyebrow: "Needs attention",
            title: "Confirmation timed out",
            note: "Check recent activity before trying again; no duplicate was started.",
            busy: false,
            alert: true,
        },
        WalletActionWatchState::Superseded => WalletActionWatchPresentation {
            class: "is-muted",
            eyebrow: "Tracking ended",
            title: "A newer action replaced this one",
            note: "Only the latest action for this wallet and network remains active.",
            busy: false,
            alert: false,
        },
        WalletActionWatchState::Offline => WalletActionWatchPresentation {
            class: "needs-attention",
            eyebrow: "Offline",
            title: "Confirmation is unavailable",
            note: "The last consistent wallet state is retained until connectivity returns.",
            busy: false,
            alert: true,
        },
        WalletActionWatchState::Degraded => WalletActionWatchPresentation {
            class: "needs-attention",
            eyebrow: "Needs attention",
            title: "Confirmation could not be established",
            note: "The last consistent wallet state is retained; review activity before retrying.",
            busy: false,
            alert: true,
        },
        WalletActionWatchState::Cancelled => WalletActionWatchPresentation {
            class: "is-muted",
            eyebrow: "Stopped",
            title: "Confirmation tracking stopped",
            note: "This action is no longer being watched.",
            busy: false,
            alert: false,
        },
    }
}

#[component]
pub(super) fn WalletActionWatchStatus(projection: WalletActionWatchProjection) -> Element {
    let presentation = present(projection.state);
    let class = format!("action-watch-status {}", presentation.class);
    rsx! {
        aside {
            class,
            role: if presentation.alert { "alert" } else { "status" },
            aria_live: "polite",
            aria_busy: if presentation.busy { "true" } else { "false" },
            span { class: "action-watch-status__mark", aria_hidden: "true" }
            div {
                p { class: "card-eyebrow", "{presentation.eyebrow}" }
                strong { "{presentation.title}" }
                p { "{presentation.note}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use oxid_wallet_application::{
        CreateWalletProfileCommand, SelectWalletProfileCommand, WalletAccountQuery,
        WalletActionWatch, WalletActionWatchObservation, WalletRealmFacetState,
        WalletRealmLifecycleIdentity, WalletRealmLifecycleInput, WalletRealmReconciliationState,
    };
    use oxid_wallet_domain::{ChainAccountId, ChainTransactionId};

    fn services_with_selected_realm() -> oxid_composition::ApplicationServices {
        let services = oxid_composition::compose_in_memory();
        let profile = services
            .create_wallet_profile()
            .execute(CreateWalletProfileCommand {
                display_name: "Action watch UI".to_owned(),
            })
            .expect("profile creation");
        services
            .select_wallet_profile()
            .execute(SelectWalletProfileCommand {
                profile_id: profile.id.clone(),
            })
            .expect("profile selection");
        let networks = services
            .list_wallet_networks()
            .execute(WalletAccountQuery {
                profile_id: profile.id.clone(),
            })
            .expect("network list");
        let identity =
            WalletRealmLifecycleIdentity::parse(profile.id, networks.selected_network_id)
                .expect("selected realm identity");
        block_on(services.reconcile_wallet_realm_lifecycle().execute(
            WalletRealmLifecycleInput::RealmSelected {
                identity,
                now_millis: 0,
                facets: WalletRealmReconciliationState {
                    account: WalletRealmFacetState::Missing,
                    dust: WalletRealmFacetState::Missing,
                    shielded: WalletRealmFacetState::Missing,
                },
            },
        ))
        .expect("realm selection");
        services
    }

    #[test]
    fn composed_outgoing_watch_presents_waiting_then_confirmed_only_in_send() {
        let services = services_with_selected_realm();
        let watches = services.manage_wallet_action_watch();
        let transaction = ChainTransactionId::parse("tx-ui").expect("transaction id");
        let handle = watches
            .admit(
                WalletActionWatch::SubmittedTransaction {
                    transaction: transaction.clone(),
                },
                10,
                0,
            )
            .expect("watch admission")
            .expect("selected realm admits watch");

        let waiting = watches.projection().expect("projection query");
        assert_eq!(
            action_watch_projection_for(waiting.clone(), WalletActionWatchContext::Send)
                .map(|projection| projection.state),
            Some(WalletActionWatchState::Waiting)
        );
        assert_eq!(
            action_watch_projection_for(waiting, WalletActionWatchContext::Receive),
            None
        );

        watches
            .observe(
                handle,
                WalletActionWatchObservation::SubmittedTransaction { transaction },
                1,
            )
            .expect("matching observation");
        assert_eq!(
            action_watch_projection_for(
                watches.projection().expect("projection query"),
                WalletActionWatchContext::Send,
            )
            .map(|projection| projection.state),
            Some(WalletActionWatchState::Confirmed)
        );
    }

    #[test]
    fn composed_incoming_watch_presents_only_after_a_later_checkpoint() {
        let services = services_with_selected_realm();
        let watches = services.manage_wallet_action_watch();
        let account = ChainAccountId::parse("account-ui").expect("account id");
        let handle = watches
            .admit(
                WalletActionWatch::IncomingArrival {
                    account: account.clone(),
                    starting_checkpoint: 7,
                },
                10,
                0,
            )
            .expect("watch admission")
            .expect("selected realm admits watch");
        watches
            .observe(
                handle,
                WalletActionWatchObservation::IncomingArrival {
                    account: account.clone(),
                    checkpoint: 7,
                },
                1,
            )
            .expect("same-checkpoint observation");
        assert_eq!(
            action_watch_projection_for(
                watches.projection().expect("projection query"),
                WalletActionWatchContext::Receive,
            )
            .map(|projection| projection.state),
            Some(WalletActionWatchState::Waiting)
        );

        watches
            .observe(
                handle,
                WalletActionWatchObservation::IncomingArrival {
                    account,
                    checkpoint: 8,
                },
                2,
            )
            .expect("later observation");
        assert_eq!(
            action_watch_projection_for(
                watches.projection().expect("projection query"),
                WalletActionWatchContext::Receive,
            )
            .map(|projection| projection.state),
            Some(WalletActionWatchState::Confirmed)
        );
    }

    #[test]
    fn terminal_states_have_concise_distinct_presentations() {
        let states = [
            WalletActionWatchState::Expired,
            WalletActionWatchState::Superseded,
            WalletActionWatchState::Offline,
            WalletActionWatchState::Degraded,
            WalletActionWatchState::Cancelled,
        ];
        let titles = states.map(|state| present(state).title);
        for (index, title) in titles.iter().enumerate() {
            assert!(
                !titles[..index].contains(title),
                "terminal action states need distinct titles"
            );
        }
    }
}
