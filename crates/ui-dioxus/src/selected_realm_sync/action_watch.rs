// SPDX-License-Identifier: Apache-2.0

//! Compact, payload-free presentation for application-owned action watches.

use dioxus::prelude::*;
use oxid_wallet_application::{
    SelectedWalletRealmSyncCommand, WalletAccountError, WalletAccountPortError, WalletAccountQuery,
    WalletAccountView, WalletActionWatch, WalletActionWatchHandle, WalletActionWatchKind,
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

pub(crate) fn reset_receive_watch(
    (mut session_active, mut boundary_ready, mut generation): ReceiveWatchSignals,
) -> u64 {
    let next_generation = {
        let mut current = generation.write();
        let next = (*current).wrapping_add(1);
        *current = next;
        next
    };
    session_active.set(false);
    boundary_ready.set(false);
    next_generation
}

pub(crate) fn start_receive_watch(
    services: WalletUiServices,
    profile_id: String,
    initial: WalletAccountView,
    selected_kind: Signal<Option<String>>,
    signals: ReceiveWatchSignals,
) {
    let (session_active, boundary_ready, generation) = signals;
    let expected_generation = reset_receive_watch(signals);
    spawn(async move {
        observe_receive_arrival(
            services,
            profile_id,
            initial,
            selected_kind,
            session_active,
            boundary_ready,
            generation,
            expected_generation,
        )
        .await;
    });
}

pub(crate) type ReceiveWatchSignals = (Signal<bool>, Signal<bool>, Signal<u64>);

async fn observe_receive_arrival(
    services: WalletUiServices,
    profile_id: String,
    initial: WalletAccountView,
    selected_kind: Signal<Option<String>>,
    mut session_active: Signal<bool>,
    mut boundary_ready: Signal<bool>,
    generation: Signal<u64>,
    expected_generation: u64,
) {
    if !receive_watch_is_current(
        *generation.peek(),
        expected_generation,
        selected_kind().as_deref(),
    ) {
        return;
    }
    let Some(account_id) = initial.account_id.as_deref() else {
        return;
    };
    let account_id = account_id.to_owned();
    let manager = services.manage_wallet_action_watch();
    let fallback_checkpoint = initial.sync.current_cursor.unwrap_or_default();

    let preflight = crate::run_ui_future(crate::wallet_realm_lifecycle::explicit_retry(
        services.clone(),
        SelectedWalletRealmSyncCommand {
            profile_id: profile_id.clone(),
        },
    ))
    .await;
    if !receive_watch_is_current(
        *generation.peek(),
        expected_generation,
        selected_kind().as_deref(),
    ) {
        return;
    }
    if !matches!(preflight, Ok(Ok(_))) {
        if publish_receive_terminal(
            &manager,
            &account_id,
            fallback_checkpoint,
            WalletActionWatchState::Degraded,
        ) {
            session_active.set(true);
        }
        return;
    }

    let baseline_services = services.clone();
    let baseline_profile = profile_id.clone();
    let baseline = crate::run_ui_blocking(move || {
        baseline_services
            .get_wallet_account()
            .execute(WalletAccountQuery {
                profile_id: baseline_profile,
            })
    })
    .await;
    if !receive_watch_is_current(
        *generation.peek(),
        expected_generation,
        selected_kind().as_deref(),
    ) {
        return;
    }
    let baseline = match baseline {
        Ok(Ok(account)) => account,
        Ok(Err(error)) => {
            if publish_receive_terminal(
                &manager,
                &account_id,
                fallback_checkpoint,
                account_error_state(&error),
            ) {
                session_active.set(true);
            }
            return;
        }
        Err(_) => {
            if publish_receive_terminal(
                &manager,
                &account_id,
                fallback_checkpoint,
                WalletActionWatchState::Degraded,
            ) {
                session_active.set(true);
            }
            return;
        }
    };
    if baseline.account_id.as_deref() != Some(account_id.as_str()) {
        return;
    }
    let Some(starting_checkpoint) = synchronized_account_checkpoint(&baseline) else {
        if publish_receive_terminal(
            &manager,
            &account_id,
            baseline.sync.current_cursor.unwrap_or(fallback_checkpoint),
            account_state(&baseline).unwrap_or(WalletActionWatchState::Degraded),
        ) {
            session_active.set(true);
        }
        return;
    };
    let baseline_transactions = confirmed_incoming_transactions(&baseline);
    if !receive_watch_is_current(
        *generation.peek(),
        expected_generation,
        selected_kind().as_deref(),
    ) {
        return;
    }
    let now_millis = monotonic_millis();
    let deadline_millis = now_millis.saturating_add(ACTION_WATCH_DURATION_MILLIS);
    let Ok(watch) = WalletActionWatch::incoming_arrival(&account_id, starting_checkpoint) else {
        return;
    };
    let Ok(Some(handle)) = manager.admit(watch, deadline_millis, now_millis) else {
        return;
    };
    boundary_ready.set(true);
    session_active.set(true);
    let mut guard = ActionWatchCancellation::new(manager.clone(), handle);

    loop {
        tokio::time::sleep(Duration::from_millis(ACTION_WATCH_POLL_MILLIS)).await;
        if !receive_watch_is_current(
            *generation.peek(),
            expected_generation,
            selected_kind().as_deref(),
        ) {
            return;
        }
        let now_millis = monotonic_millis();
        if now_millis >= deadline_millis {
            let _ = manager.timeout(handle, now_millis);
            guard.disarm();
            return;
        }
        let query_services = services.clone();
        let query_profile = profile_id.clone();
        let observed = crate::run_ui_blocking(move || {
            query_services
                .get_wallet_account()
                .execute(WalletAccountQuery {
                    profile_id: query_profile,
                })
        })
        .await;
        if !receive_watch_is_current(
            *generation.peek(),
            expected_generation,
            selected_kind().as_deref(),
        ) {
            return;
        }
        let observed = match observed {
            Ok(Ok(account)) => account,
            Ok(Err(error)) => {
                settle_receive_handle(&manager, handle, account_error_state(&error));
                guard.disarm();
                return;
            }
            Err(_) => {
                settle_receive_handle(&manager, handle, WalletActionWatchState::Degraded);
                guard.disarm();
                return;
            }
        };
        if observed.account_id.as_deref() != Some(account_id.as_str()) {
            return;
        }
        if let Some(state) = account_state(&observed) {
            settle_receive_handle(&manager, handle, state);
            guard.disarm();
            return;
        }
        if observed.sync.state == "syncing" {
            continue;
        }
        let Some(observed_checkpoint) = observed.sync.current_cursor else {
            settle_receive_handle(&manager, handle, WalletActionWatchState::Degraded);
            guard.disarm();
            return;
        };
        if observed_checkpoint <= starting_checkpoint
            || confirmed_incoming_transactions(&observed)
                .difference(&baseline_transactions)
                .next()
                .is_none()
        {
            continue;
        }
        let Ok(observation) =
            WalletActionWatchObservation::incoming_arrival(&account_id, observed_checkpoint)
        else {
            return;
        };
        let _ = manager.observe(handle, observation, now_millis);
        guard.disarm();
        return;
    }
}

fn receive_watch_is_current(
    current_generation: u64,
    expected_generation: u64,
    kind: Option<&str>,
) -> bool {
    current_generation == expected_generation && receive_watch_supported(kind)
}

fn receive_watch_supported(kind: Option<&str>) -> bool {
    kind == Some("unshielded")
}

pub(crate) fn receive_address_ready(kind: &str, boundary_ready: bool) -> bool {
    kind != "unshielded" || boundary_ready
}

#[component]
pub(crate) fn ReceiveBoundaryStatus(failed: bool) -> Element {
    rsx! {
        div { class: "receive-sheet__state", role: "status",
            if !failed {
                span { class: "loading-mark", aria_hidden: "true" }
            }
            strong {
                if failed {
                    "Public receive is unavailable"
                } else {
                    "Preparing public receive…"
                }
            }
            p { "The address stays hidden until synchronized arrival tracking is ready." }
        }
    }
}

fn synchronized_account_checkpoint(account: &WalletAccountView) -> Option<u64> {
    if account_state(account).is_none() && account.sync.state == "synced" {
        account.sync.current_cursor
    } else {
        None
    }
}

fn account_state(account: &WalletAccountView) -> Option<WalletActionWatchState> {
    if account.source == "unavailable" || account.sync.state == "unavailable" {
        Some(WalletActionWatchState::Offline)
    } else if account.source != "live"
        || !matches!(account.sync.state.as_str(), "synced" | "syncing")
    {
        Some(WalletActionWatchState::Degraded)
    } else {
        None
    }
}

fn account_error_state(error: &WalletAccountError) -> WalletActionWatchState {
    if matches!(
        error,
        WalletAccountError::Port(WalletAccountPortError::Unavailable)
    ) {
        WalletActionWatchState::Offline
    } else {
        WalletActionWatchState::Degraded
    }
}

fn publish_receive_terminal(
    manager: &std::sync::Arc<dyn oxid_wallet_application::ManageWalletActionWatchUseCase>,
    account_id: &str,
    starting_checkpoint: u64,
    state: WalletActionWatchState,
) -> bool {
    let Ok(watch) = WalletActionWatch::incoming_arrival(account_id, starting_checkpoint) else {
        return false;
    };
    let now_millis = monotonic_millis();
    let Ok(Some(handle)) = manager.admit(
        watch,
        now_millis.saturating_add(ACTION_WATCH_DURATION_MILLIS),
        now_millis,
    ) else {
        return false;
    };
    settle_receive_handle(manager, handle, state);
    true
}

fn settle_receive_handle(
    manager: &std::sync::Arc<dyn oxid_wallet_application::ManageWalletActionWatchUseCase>,
    handle: WalletActionWatchHandle,
    state: WalletActionWatchState,
) {
    match state {
        WalletActionWatchState::Offline => {
            let _ = manager.offline(handle);
        }
        WalletActionWatchState::Degraded => {
            let _ = manager.degraded(handle);
        }
        _ => debug_assert!(false, "receive failure must be offline or degraded"),
    }
}

fn confirmed_incoming_transactions(account: &WalletAccountView) -> BTreeSet<String> {
    account
        .transactions
        .iter()
        .filter(|transaction| {
            transaction.direction == "incoming" && transaction.status == "confirmed"
        })
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
        WalletSyncStatusView, WalletTransactionView,
    };

    fn account(
        source: &str,
        sync_state: &str,
        cursor: Option<u64>,
        transactions: Vec<WalletTransactionView>,
    ) -> WalletAccountView {
        WalletAccountView {
            chain: "midnight".to_owned(),
            network_id: "undeployed".to_owned(),
            network_name: "Standalone".to_owned(),
            network_environment: "undeployed".to_owned(),
            account_id: Some("account-ui".to_owned()),
            source: source.to_owned(),
            addresses: Vec::new(),
            balances: Vec::new(),
            sync: WalletSyncStatusView {
                state: sync_state.to_owned(),
                current_cursor: cursor,
                target_cursor: cursor,
                chain_tip_height: cursor,
                updated_at_millis: None,
            },
            transactions,
        }
    }

    fn transaction(id: &str, direction: &str, status: &str) -> WalletTransactionView {
        WalletTransactionView {
            transaction_id: id.to_owned(),
            direction: direction.to_owned(),
            status: status.to_owned(),
            block_height: None,
            observed_at_millis: None,
            changes: Vec::new(),
            fee: None,
        }
    }

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
    fn superseded_receive_observers_cannot_publish_or_admit_work() {
        assert!(receive_watch_is_current(7, 7, Some("unshielded")));
        assert!(!receive_watch_is_current(8, 7, Some("unshielded")));
        assert!(!receive_watch_is_current(7, 7, Some("shielded")));
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
    fn receive_baseline_requires_a_live_synchronized_chain_cursor() {
        assert_eq!(
            synchronized_account_checkpoint(&account("live", "synced", Some(7), Vec::new())),
            Some(7)
        );
        assert_eq!(
            synchronized_account_checkpoint(&account("cached", "synced", Some(7), Vec::new())),
            None
        );
        assert_eq!(
            account_state(&account("unavailable", "unavailable", None, Vec::new())),
            Some(WalletActionWatchState::Offline)
        );
        assert_eq!(
            account_state(&account("live", "stalled", Some(7), Vec::new())),
            Some(WalletActionWatchState::Degraded)
        );
        assert_eq!(
            account_state(&account("live", "syncing", Some(7), Vec::new())),
            None
        );
        assert_eq!(
            synchronized_account_checkpoint(&account("live", "syncing", Some(7), Vec::new())),
            None
        );
    }

    #[test]
    fn receive_observation_uses_only_confirmed_incoming_rows() {
        let observed = account(
            "live",
            "synced",
            Some(9),
            vec![
                transaction("confirmed-incoming", "incoming", "confirmed"),
                transaction("pending-incoming", "incoming", "pending"),
                transaction("failed-incoming", "incoming", "failed"),
                transaction("confirmed-outgoing", "outgoing", "confirmed"),
            ],
        );

        assert_eq!(
            confirmed_incoming_transactions(&observed),
            BTreeSet::from(["confirmed-incoming".to_owned()])
        );
    }

    #[test]
    fn receive_watch_is_explicitly_scoped_to_the_public_rail() {
        assert!(receive_watch_supported(Some("unshielded")));
        assert!(!receive_watch_supported(Some("shielded")));
        assert!(!receive_watch_supported(None));
        assert!(!receive_address_ready("unshielded", false));
        assert!(receive_address_ready("unshielded", true));
        assert!(receive_address_ready("shielded", false));
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
