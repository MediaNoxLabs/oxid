// SPDX-License-Identifier: Apache-2.0

//! Compact, payload-free presentation for application-owned action watches.

use dioxus::prelude::*;
use oxid_wallet_application::{
    WalletAccountQuery, WalletAccountView, WalletActionWatch, WalletActionWatchKind,
    WalletActionWatchObservation, WalletActionWatchProjection, WalletActionWatchState,
};
use std::{collections::BTreeSet, time::Duration};

use crate::{WalletUiServices, wallet_realm_lifecycle::monotonic_millis};

const ACTION_WATCH_POLL_MILLIS: u64 = 250;
const ACTION_WATCH_DURATION_MILLIS: u64 = 10 * 60 * 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WalletActionWatchContext {
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

pub(crate) fn action_watch_projection_from(
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

pub(crate) fn use_action_watch_projection(
    services: WalletUiServices,
    context: WalletActionWatchContext,
) -> Signal<Option<WalletActionWatchProjection>> {
    let initial = action_watch_projection_from(&services, context);
    let mut projection = use_signal(|| initial);
    use_future(move || {
        let services = services.clone();
        async move {
            loop {
                let next = action_watch_projection_from(&services, context);
                if projection() != next {
                    projection.set(next);
                }
                tokio::time::sleep(Duration::from_millis(ACTION_WATCH_POLL_MILLIS)).await;
            }
        }
    });
    projection
}

pub(crate) fn record_included_transfer(
    manager: &std::sync::Arc<dyn oxid_wallet_application::ManageWalletActionWatchUseCase>,
    transaction_id: &str,
) {
    let Ok(watch) = WalletActionWatch::submitted_transaction(transaction_id) else {
        return;
    };
    let Ok(observation) = WalletActionWatchObservation::submitted_transaction(transaction_id)
    else {
        return;
    };
    let now_millis = monotonic_millis();
    let Ok(Some(handle)) = manager.admit(
        watch,
        now_millis.saturating_add(ACTION_WATCH_DURATION_MILLIS),
        now_millis,
    ) else {
        return;
    };
    let _ = manager.observe(handle, observation, now_millis);
}

pub(crate) async fn observe_receive_arrival(
    services: WalletUiServices,
    profile_id: String,
    baseline: WalletAccountView,
) {
    let Some(account_id) = baseline.account_id.as_deref() else {
        return;
    };
    let account_id = account_id.to_owned();
    let baseline_transactions = incoming_transactions(&baseline);
    let starting_checkpoint = u64::try_from(baseline_transactions.len()).unwrap_or(u64::MAX);
    let manager = services.manage_wallet_action_watch();
    let now_millis = monotonic_millis();
    let deadline_millis = now_millis.saturating_add(ACTION_WATCH_DURATION_MILLIS);
    let Ok(watch) = WalletActionWatch::incoming_arrival(&account_id, starting_checkpoint) else {
        return;
    };
    let Ok(Some(handle)) = manager.admit(watch, deadline_millis, now_millis) else {
        return;
    };
    let mut guard = ActionWatchCancellation::new(manager.clone(), handle);

    loop {
        tokio::time::sleep(Duration::from_millis(ACTION_WATCH_POLL_MILLIS)).await;
        let now_millis = monotonic_millis();
        if now_millis >= deadline_millis {
            let _ = manager.timeout(handle, now_millis);
            guard.disarm();
            return;
        }
        let Ok(observed) = services.get_wallet_account().execute(WalletAccountQuery {
            profile_id: profile_id.clone(),
        }) else {
            continue;
        };
        if observed.account_id.as_deref() != Some(account_id.as_str()) {
            return;
        }
        if incoming_transactions(&observed)
            .difference(&baseline_transactions)
            .next()
            .is_none()
        {
            continue;
        }
        let Ok(observation) = WalletActionWatchObservation::incoming_arrival(
            &account_id,
            starting_checkpoint.saturating_add(1),
        ) else {
            return;
        };
        let _ = manager.observe(handle, observation, now_millis);
        guard.disarm();
        return;
    }
}

fn incoming_transactions(account: &WalletAccountView) -> BTreeSet<String> {
    account
        .transactions
        .iter()
        .filter(|transaction| transaction.direction == "incoming")
        .map(|transaction| transaction.transaction_id.clone())
        .collect()
}

struct ActionWatchCancellation {
    manager: std::sync::Arc<dyn oxid_wallet_application::ManageWalletActionWatchUseCase>,
    handle: Option<oxid_wallet_application::WalletActionWatchHandle>,
}

impl ActionWatchCancellation {
    fn new(
        manager: std::sync::Arc<dyn oxid_wallet_application::ManageWalletActionWatchUseCase>,
        handle: oxid_wallet_application::WalletActionWatchHandle,
    ) -> Self {
        Self {
            manager,
            handle: Some(handle),
        }
    }

    fn disarm(&mut self) {
        self.handle = None;
    }
}

impl Drop for ActionWatchCancellation {
    fn drop(&mut self) {
        if let Some(handle) = self.handle {
            let _ = self.manager.cancel(handle);
        }
    }
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
pub(crate) fn WalletActionWatchStatus(projection: WalletActionWatchProjection) -> Element {
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
        let handle = watches
            .admit(
                WalletActionWatch::submitted_transaction("tx-ui").expect("transaction id"),
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
                WalletActionWatchObservation::submitted_transaction("tx-ui")
                    .expect("transaction id"),
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
    fn included_transfer_helper_settles_the_composed_production_owner() {
        let services = services_with_selected_realm();
        let watches = services.manage_wallet_action_watch();

        record_included_transfer(&watches, "tx-included-ui");

        let projection = watches
            .projection()
            .expect("projection query")
            .expect("included transfer watch");
        assert_eq!(projection.kind, WalletActionWatchKind::SubmittedTransaction);
        assert_eq!(projection.state, WalletActionWatchState::Confirmed);
    }

    #[test]
    fn dropping_a_mounted_receive_observer_cancels_its_watch() {
        let services = services_with_selected_realm();
        let watches = services.manage_wallet_action_watch();
        let handle = watches
            .admit(
                WalletActionWatch::incoming_arrival("account-drop", 0).expect("account id"),
                10,
                0,
            )
            .expect("watch admission")
            .expect("selected realm admits watch");

        drop(ActionWatchCancellation::new(watches.clone(), handle));

        assert_eq!(
            watches
                .projection()
                .expect("projection query")
                .expect("cancelled projection")
                .state,
            WalletActionWatchState::Cancelled
        );
    }

    #[test]
    fn composed_incoming_watch_presents_only_after_a_later_checkpoint() {
        let services = services_with_selected_realm();
        let watches = services.manage_wallet_action_watch();
        let handle = watches
            .admit(
                WalletActionWatch::incoming_arrival("account-ui", 7).expect("account id"),
                10,
                0,
            )
            .expect("watch admission")
            .expect("selected realm admits watch");
        watches
            .observe(
                handle,
                WalletActionWatchObservation::incoming_arrival("account-ui", 7)
                    .expect("account id"),
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
                WalletActionWatchObservation::incoming_arrival("account-ui", 8)
                    .expect("account id"),
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
