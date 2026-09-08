// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AccountSyncCardState {
    Loading,
    Ready {
        realm: Box<SelectedWalletRealmSyncView>,
        action_busy: bool,
        operation_error: Option<String>,
    },
    Failed(String),
}

pub(super) fn load_account_sync_card(
    services: &WalletUiServices,
    profile_id: &str,
) -> AccountSyncCardState {
    services
        .get_selected_wallet_realm_sync()
        .execute(SelectedWalletRealmSyncCommand {
            profile_id: profile_id.to_owned(),
        })
        .map(|realm| AccountSyncCardState::Ready {
            realm: Box::new(realm),
            action_busy: false,
            operation_error: None,
        })
        .unwrap_or_else(|error| AccountSyncCardState::Failed(error.to_string()))
}

pub(super) fn poll_account_sync(
    services: WalletUiServices,
    profile_id: String,
    mut state: Signal<AccountSyncCardState>,
    on_account_updated: EventHandler<WalletAccountView>,
) {
    spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let worker_services = services.clone();
            let worker_profile = profile_id.clone();
            let result =
                run_ui_blocking(move || load_account_sync_card(&worker_services, &worker_profile))
                    .await;
            match result {
                Ok(AccountSyncCardState::Ready { realm, .. }) => {
                    let complete = !selected_realm_is_syncing(&realm);
                    if let WalletRealmFamilyView::Ready(account) = &realm.account {
                        on_account_updated.call(account.clone());
                    }
                    state.set(AccountSyncCardState::Ready {
                        realm,
                        action_busy: false,
                        operation_error: None,
                    });
                    if complete {
                        break;
                    }
                }
                Ok(AccountSyncCardState::Failed(error)) => {
                    state.set(AccountSyncCardState::Failed(error));
                    break;
                }
                Ok(AccountSyncCardState::Loading) => {}
                Err(error) => {
                    state.set(AccountSyncCardState::Failed(error.to_string()));
                    break;
                }
            }
        }
    });
}

pub(super) fn selected_realm_is_syncing(realm: &SelectedWalletRealmSyncView) -> bool {
    selected_realm_dust_state(&realm.dust) == "syncing"
        || selected_realm_shielded_state(&realm.shielded) == "syncing"
}

pub(super) fn selected_realm_sync_state(realm: &SelectedWalletRealmSyncView) -> &str {
    let account = selected_realm_account_state(&realm.account);
    let dust = selected_realm_dust_state(&realm.dust);
    let shielded = selected_realm_shielded_state(&realm.shielded);
    if account == "syncing" || dust == "syncing" || shielded == "syncing" {
        "syncing"
    } else if account == "synced" && dust == "synced" && shielded == "synced" {
        "synced"
    } else if account == "stalled" || dust == "stalled" || shielded == "stalled" {
        "stalled"
    } else if dust == "cancelled" || shielded == "cancelled" {
        "cancelled"
    } else if account == "cached" || dust == "cached" || shielded == "cached" {
        "cached"
    } else if matches!(account, "unsupported" | "unavailable" | "invalid_data")
        || matches!(dust, "unsupported" | "unavailable" | "invalid_data")
        || matches!(shielded, "unsupported" | "unavailable" | "invalid_data")
    {
        "unavailable"
    } else {
        "never_synced"
    }
}

pub(super) fn selected_realm_provenance(realm: &SelectedWalletRealmSyncView) -> String {
    match &realm.account {
        WalletRealmFamilyView::Ready(account) => format!(
            "{} · {} · {} source",
            account.network_name,
            account.chain,
            ui::account_source(&account.source).to_lowercase(),
        ),
        family => format!(
            "Selected realm · {} source",
            family.state_name().replace('_', " ")
        ),
    }
}

pub(super) fn selected_realm_chain_tip(realm: &SelectedWalletRealmSyncView) -> String {
    match &realm.account {
        WalletRealmFamilyView::Ready(account) => account.sync.chain_tip_height.map_or_else(
            || "Indexer tip unavailable".to_owned(),
            |height| format!("Indexer tip · block {height}"),
        ),
        _ => "Indexer tip unavailable".to_owned(),
    }
}

fn selected_realm_account_state(family: &WalletRealmFamilyView<WalletAccountView>) -> &str {
    match family {
        WalletRealmFamilyView::Ready(account) => &account.sync.state,
        WalletRealmFamilyView::Busy => "syncing",
        WalletRealmFamilyView::NotFound => "never_synced",
        _ => "unavailable",
    }
}

pub(super) fn selected_realm_sync_progress(realm: &SelectedWalletRealmSyncView) -> Option<u64> {
    let values = [
        match &realm.dust {
            WalletRealmFamilyView::Ready(status) => dust_progress_percent(status),
            _ => None,
        },
        match &realm.shielded {
            WalletRealmFamilyView::Ready(status) => shielded_progress_percent(status),
            _ => None,
        },
    ];
    let values = values.into_iter().flatten().collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<u64>() / u64::try_from(values.len()).ok()?)
    }
}

pub(super) fn selected_realm_dust_state(
    family: &WalletRealmFamilyView<WalletDustSyncView>,
) -> &str {
    match family {
        WalletRealmFamilyView::Ready(status) => &status.state,
        WalletRealmFamilyView::Busy => "syncing",
        WalletRealmFamilyView::NotFound => "never_synced",
        _ => "unavailable",
    }
}

pub(super) fn selected_realm_shielded_state(
    family: &WalletRealmFamilyView<WalletShieldedSyncView>,
) -> &str {
    match family {
        WalletRealmFamilyView::Ready(status) => &status.state,
        WalletRealmFamilyView::Busy => "syncing",
        WalletRealmFamilyView::NotFound => "never_synced",
        _ => "unavailable",
    }
}

pub(super) fn selected_realm_dust_balance(
    family: &WalletRealmFamilyView<WalletDustSyncView>,
) -> String {
    match family {
        WalletRealmFamilyView::Ready(status) => status.balance_atomic_units.as_deref().map_or_else(
            || {
                if status.state == "synced" {
                    "Not registered".to_owned()
                } else {
                    "Pending".to_owned()
                }
            },
            |value| format!("{} DUST", ui::format_atomic_units(value, ui::DUST_DECIMALS)),
        ),
        _ => "Unavailable".to_owned(),
    }
}

pub(super) fn selected_realm_dust_note(
    family: &WalletRealmFamilyView<WalletDustSyncView>,
) -> String {
    match family {
        WalletRealmFamilyView::Ready(status)
            if status.state == "synced" && status.balance_atomic_units.is_none() =>
        {
            "No registered DUST state was found for this account.".to_owned()
        }
        WalletRealmFamilyView::Ready(status) => dust_sync_note(status),
        WalletRealmFamilyView::Busy => "DUST synchronization is already running.".to_owned(),
        WalletRealmFamilyView::Unsupported => {
            "DUST is not supported by the selected network realm.".to_owned()
        }
        WalletRealmFamilyView::ProtectionNotInitialized => {
            "Set up wallet protection before synchronizing DUST.".to_owned()
        }
        WalletRealmFamilyView::ProtectionLocked => {
            "Unlock this wallet profile before synchronizing DUST.".to_owned()
        }
        WalletRealmFamilyView::NotFound => "No DUST account is active in this realm.".to_owned(),
        WalletRealmFamilyView::Unavailable => {
            "DUST synchronization is unavailable in this composition.".to_owned()
        }
        WalletRealmFamilyView::InvalidData => {
            "The DUST source returned inconsistent state.".to_owned()
        }
    }
}

pub(super) fn selected_realm_shielded_balance(
    family: &WalletRealmFamilyView<WalletShieldedSyncView>,
) -> String {
    match family {
        WalletRealmFamilyView::Ready(status) => home_shielded_value(status),
        _ => "Unavailable".to_owned(),
    }
}

pub(super) fn selected_realm_shielded_note(
    family: &WalletRealmFamilyView<WalletShieldedSyncView>,
) -> String {
    match family {
        WalletRealmFamilyView::Ready(status) => shielded_sync_note(status),
        WalletRealmFamilyView::Busy => "Shielded synchronization is already running.".to_owned(),
        WalletRealmFamilyView::Unsupported => {
            "Shielded assets are not supported by the selected network realm.".to_owned()
        }
        WalletRealmFamilyView::ProtectionNotInitialized => {
            "Set up wallet protection before synchronizing shielded assets.".to_owned()
        }
        WalletRealmFamilyView::ProtectionLocked => {
            "Unlock this wallet profile before synchronizing shielded assets.".to_owned()
        }
        WalletRealmFamilyView::NotFound => {
            "No shielded account is active in this realm.".to_owned()
        }
        WalletRealmFamilyView::Unavailable => {
            "Shielded synchronization is unavailable in this composition.".to_owned()
        }
        WalletRealmFamilyView::InvalidData => {
            "The shielded source returned inconsistent state.".to_owned()
        }
    }
}

pub(super) fn dust_progress_percent(status: &WalletDustSyncView) -> Option<u64> {
    let (current, target) = status.current_cursor.zip(status.target_cursor)?;
    let completed = u128::from(current).checked_add(1)?;
    let total = u128::from(target).checked_add(1)?;
    let percent = completed.checked_mul(100)?.checked_div(total)?.min(100);
    u64::try_from(percent).ok()
}

pub(super) fn dust_sync_note(status: &WalletDustSyncView) -> String {
    let detail = match status.state.as_str() {
        "never_synced" => "DUST has not been indexed for this protected account.".to_owned(),
        "syncing" => "Refreshing the protected DUST balance…".to_owned(),
        "synced" => "DUST is synchronized.".to_owned(),
        "cached" => "Showing a resumable cached DUST checkpoint; spending remains disabled until live catch-up.".to_owned(),
        "cancelled" => "DUST synchronization was cancelled at a consistent checkpoint and can resume.".to_owned(),
        "stalled" => "DUST synchronization stalled; the last consistent checkpoint is retained.".to_owned(),
        _ => "DUST synchronization is not available in this composition.".to_owned(),
    };
    status.failure.as_ref().map_or(detail.clone(), |failure| {
        format!("{detail} ({})", ui::sync_failure(failure))
    })
}

pub(super) fn dust_status_pill_class(state: &str) -> &'static str {
    match state {
        "synced" => "status-pill success",
        "syncing" | "cached" => "status-pill warning",
        _ => "status-pill",
    }
}

pub(super) fn shielded_progress_percent(status: &WalletShieldedSyncView) -> Option<u64> {
    let (current, target) = status.current_cursor.zip(status.target_cursor)?;
    let completed = u128::from(current).checked_add(1)?;
    let total = u128::from(target).checked_add(1)?;
    let percent = completed.checked_mul(100)?.checked_div(total)?.min(100);
    u64::try_from(percent).ok()
}

pub(super) fn shielded_sync_note(status: &WalletShieldedSyncView) -> String {
    let detail = match status.state.as_str() {
        "never_synced" => {
            "Shielded notes have not been indexed for this protected account.".to_owned()
        }
        "syncing" => "Refreshing protected shielded notes…".to_owned(),
        "synced" => "Shielded notes are synchronized.".to_owned(),
        "cached" => {
            "Showing a key-scoped cached shielded checkpoint; live catch-up is still required."
                .to_owned()
        }
        "cancelled" => {
            "Shielded synchronization was cancelled at a consistent checkpoint and can resume."
                .to_owned()
        }
        "stalled" => {
            "Shielded synchronization stalled; the last consistent checkpoint is retained."
                .to_owned()
        }
        _ => "Shielded synchronization is not available in this composition.".to_owned(),
    };
    status.failure.as_ref().map_or(detail.clone(), |failure| {
        format!("{detail} ({})", ui::sync_failure(failure))
    })
}

pub(super) fn non_native_shielded_balances(
    status: &WalletShieldedSyncView,
) -> impl Iterator<Item = &oxid_wallet_application::WalletShieldedTokenBalanceView> {
    status
        .balances
        .iter()
        .filter(|balance| balance.token_type_hex != NATIVE_SHIELDED_NIGHT_TOKEN_TYPE)
}
