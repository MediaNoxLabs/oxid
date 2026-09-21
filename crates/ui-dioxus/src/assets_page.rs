// SPDX-License-Identifier: Apache-2.0

use super::*;

#[component]
pub(super) fn AssetsPage(
    active_profile: WalletProfileView,
    secret_mode: SecretModeController,
    on_realm_changed: EventHandler<()>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    #[cfg(feature = "preprod-observation")]
    let observation_only = services.wallet_root_recovery.is_some();
    #[cfg(not(feature = "preprod-observation"))]
    let observation_only = false;
    let mut state = use_signal(|| AccountPageState::Loading);
    let profile_id = active_profile.id.clone();
    let services_for_load = services.clone();
    use_effect(move || {
        let services = services_for_load.clone();
        let profile_id = profile_id.clone();
        spawn(async move {
            state.set(
                run_ui_blocking(move || load_account_page(&services, &profile_id))
                    .await
                    .unwrap_or_else(|error| AccountPageState::Failed(error.to_string())),
            );
        });
    });

    match state.read().clone() {
        AccountPageState::Loading => rsx! {
            section { class: "wallet-hero",
                p { class: "eyebrow", "Wallet overview" }
                div { class: "wallet-hero__number-row",
                    h1 { "…" }
                    span { "NIGHT" }
                }
                p { class: "wallet-hero__hint", "Loading the selected Midnight account boundary…" }
            }
        },
        AccountPageState::Failed(error) => rsx! {
            section { class: "wallet-hero",
                p { class: "eyebrow", "Wallet overview" }
                div { class: "wallet-hero__number-row",
                    h1 { "—" }
                    span { "NIGHT" }
                }
                p { class: "wallet-hero__hint", "Account state could not be loaded safely." }
            }
            article { class: "empty-state surface-card", role: "alert",
                h2 { "Midnight account unavailable" }
                p { "{error}" }
                button {
                    class: "secondary-action",
                    r#type: "button",
                    onclick: move |_| {
                        let services = services.clone();
                        let profile_id = active_profile.id.clone();
                        state.set(AccountPageState::Loading);
                        spawn(async move {
                            state.set(
                                run_ui_blocking(move || {
                                    load_account_page(&services, &profile_id)
                                })
                                .await
                                .unwrap_or_else(|error| {
                                    AccountPageState::Failed(error.to_string())
                                }),
                            );
                        });
                    },
                    "Retry"
                }
            }
        },
        AccountPageState::Ready {
            networks,
            account,
            security,
            custody_recovery_required,
            busy,
        } => {
            let night = balance_for(&account, "NIGHT")
                .map(|balance| ui::format_atomic_units(&balance.atomic_units, balance.decimals))
                .unwrap_or_else(|| "—".to_owned());
            let dust = balance_for(&account, "DUST")
                .map(|balance| ui::format_atomic_units(&balance.atomic_units, balance.decimals))
                .unwrap_or_else(|| "—".to_owned());
            let dust_balance_positive = balance_for(&account, "DUST")
                .is_some_and(|balance| balance.atomic_units.bytes().any(|digit| digit != b'0'));
            let unavailable = account.source == "unavailable";
            let is_busy = busy.is_some();
            let account_hint = account_hint(&account, busy);
            let source_label = ui::account_source(&account.source);
            let protected_account = has_protected_account(&account);
            let protection_available = security.is_available();
            let protection_unlocked = security.state_name() == "Unlocked";
            #[cfg(feature = "ui-profile-dev")]
            let lifecycle_label = services
                .reconcile_wallet_realm_lifecycle()
                .status()
                .ok()
                .map(|status| {
                    format!(
                        "Wallet lifecycle generation {}",
                        status.lifecycle_generation
                    )
                });
            #[cfg(not(feature = "ui-profile-dev"))]
            let lifecycle_label = None::<String>;
            let selected_network_id = networks.selected_network_id.clone();
            let select_services = services.clone();
            let select_profile_id = active_profile.id.clone();
            let mut select_state = state;
            let activate_services = services.clone();
            let activate_profile_id = active_profile.id.clone();
            let activate_networks = networks.clone();
            let activate_account = account.clone();
            let mut activate_state = state;

            rsx! {
                section { class: "wallet-hero",
                    div { class: "wallet-hero__heading-row",
                        p { class: "eyebrow", "Wallet overview" }
                        span { class: if account.source == "simulated" { "status-pill warning" } else { "status-pill" },
                            "{source_label}"
                        }
                    }
                    div { class: "wallet-hero__number-row",
                        h1 {
                            class: "privacy-value",
                            aria_hidden: if secret_mode.is_masked() { "true" } else { "false" },
                            "{night}"
                        }
                        span { "NIGHT" }
                    }
                    div { class: "dust-pill",
                        strong {
                            class: "privacy-value",
                            aria_hidden: if secret_mode.is_masked() { "true" } else { "false" },
                            "{dust}"
                        }
                        span { "DUST" }
                    }
                    p { class: "wallet-hero__hint", "{account_hint}" }
                }

                section { class: "trust-line", role: "status",
                    span { class: "trust-line__icon", aria_hidden: "true", if unavailable { "○" } else { "◇" } }
                    div {
                        strong { "{active_profile.display_name} · {account.network_name}" }
                        p {
                            if let Some(height) = account.sync.chain_tip_height {
                                "{ui::sync_state(&account.sync.state)} · block {height} · {source_label} source"
                            } else {
                                "{ui::sync_state(&account.sync.state)} · {source_label} source"
                            }
                        }
                        if let Some(lifecycle_label) = lifecycle_label.as_deref() {
                            small { class: "dev-lifecycle-marker", "{lifecycle_label}" }
                        }
                    }
                }

                label { class: "network-field",
                    span { "Midnight network" }
                    select {
                        value: "{selected_network_id}",
                        disabled: is_busy || observation_only,
                        onchange: move |event| {
                            let network_id = event.value();
                            let services = select_services.clone();
                            let profile_id = select_profile_id.clone();
                            select_state.set(AccountPageState::Loading);
                            spawn(async move {
                                let result = run_ui_blocking(move || {
                                    services
                                        .select_wallet_network()
                                        .execute(SelectWalletNetworkCommand {
                                            profile_id: profile_id.clone(),
                                            network_id,
                                        })
                                        .and_then(|selected| {
                                            services
                                                .get_wallet_account()
                                                .execute(WalletAccountQuery { profile_id })
                                                .map(|account| (selected, account))
                                        })
                                })
                                .await;
                                match result {
                                    Ok(Ok((networks, account))) => {
                                        select_state.set(AccountPageState::Ready {
                                            networks,
                                            account: Box::new(account),
                                            security,
                                            custody_recovery_required: false,
                                            busy: None,
                                        });
                                        on_realm_changed.call(());
                                    }
                                    Ok(Err(error)) => select_state
                                        .set(AccountPageState::Failed(error.to_string())),
                                    Err(error) => select_state
                                        .set(AccountPageState::Failed(error.to_string())),
                                }
                            });
                        },
                        for network in networks.networks.iter() {
                            option {
                                key: "{network.network_id}",
                                value: "{network.network_id}",
                                selected: network.selected,
                                "{network.display_name}"
                            }
                        }
                    }
                    if observation_only {
                        small { "The authenticated deployment fixes this recovered wallet to PreProd." }
                    }
                }

                if observation_only && protection_available && security.state_name() == "Uninitialized" {
                    article { class: "surface-card development-card",
                        p { class: "card-eyebrow", "PreProd recovery" }
                        h2 { "Wallet root not installed" }
                        p { "Finish owner-root recovery from Settings before deriving or synchronizing this profile." }
                    }
                }

                if wallet_account_activation_available(
                    observation_only,
                    protection_available,
                    security.state_name(),
                    protected_account,
                    custody_recovery_required,
                ) {
                    article { class: "surface-card development-card",
                        p { class: "card-eyebrow", if observation_only { "PreProd recovery" } else { "Standalone development" } }
                        h2 {
                            if observation_only && security.state_name() == "Locked" {
                                "Unlock recovered PreProd wallet"
                            } else if observation_only {
                                "Finish recovered PreProd account"
                            } else if security.state_name() == "Uninitialized" {
                                "Activate protected test account"
                            } else if security.state_name() == "Locked" {
                                "Unlock protected test account"
                            } else {
                                "Derive protected NIGHT account"
                            }
                        }
                        p {
                            if observation_only {
                                "Native custody already holds the recovered root. Authorize account 0/address 0 derivation without entering the root again."
                            } else {
                                "This opt-in simulator/emulator mode uses development-only custody. It is not production key protection."
                            }
                        }
                        button {
                            class: "primary-action",
                            r#type: "button",
                            disabled: is_busy,
                            aria_label: if observation_only { "Finish recovered PreProd account" } else { "Activate protected Midnight account" },
                            onclick: move |_| {
                                activate_state.set(AccountPageState::Ready {
                                    networks: activate_networks.clone(),
                                    account: activate_account.clone(),
                                    security,
                                    custody_recovery_required,
                                    busy: Some(account_activation_operation(security)),
                                });
                                let services = activate_services.clone();
                                let profile_id = activate_profile_id.clone();
                                let networks = activate_networks.clone();
                                let account = activate_account.clone();
                                spawn(async move {
                                    match activate_protected_account(
                                        services.clone(),
                                        profile_id.clone(),
                                        security,
                                    )
                                    .await
                                    {
                                        Ok(updated_security) => {
                                            if matches!(
                                                security.state_name(),
                                                "Uninitialized" | "Locked"
                                            ) {
                                                secret_mode.rearm();
                                            }
                                            let service = services.sync_wallet_account();
                                            activate_state.set(AccountPageState::Ready {
                                                networks: networks.clone(),
                                                account: account.clone(),
                                                security: updated_security,
                                                custody_recovery_required: false,
                                                busy: Some(AccountOperation::Syncing),
                                            });
                                            match run_ui_future(async move {
                                                service.execute(WalletAccountQuery { profile_id }).await
                                            })
                                            .await
                                            {
                                                Ok(Ok(account)) => activate_state.set(AccountPageState::Ready {
                                                    networks,
                                                    account: Box::new(account),
                                                    security: updated_security,
                                                    custody_recovery_required: false,
                                                    busy: None,
                                                }),
                                                Ok(Err(error)) => activate_state.set(AccountPageState::Failed(error.to_string())),
                                                Err(error) => activate_state.set(AccountPageState::Failed(error.to_string())),
                                            }
                                        }
                                        Err(error) => activate_state.set(AccountPageState::Failed(error)),
                                    }
                                });
                            },
                            if is_busy {
                                "Activating…"
                            } else if observation_only {
                                "Authorize and finish account"
                            } else {
                                "Activate development wallet"
                            }
                        }
                    }
                }

                AccountSyncCard {
                    profile_id: active_profile.id.clone(),
                    secret_mode,
                    can_sync: protection_unlocked,
                    account_unavailable: unavailable,
                    on_account_updated: move |updated_account| {
                        state.set(AccountPageState::Ready {
                            networks: networks.clone(),
                            account: Box::new(updated_account),
                            security,
                            custody_recovery_required,
                            busy: None,
                        });
                    },
                }

                if custody_recovery_required {
                    article { class: "surface-card account-sync-card", role: "alert",
                        p { class: "card-eyebrow", "Wallet recovery" }
                        h2 { "Restore this wallet to continue" }
                        p { "The saved profile still refers to a protected Midnight account, but this app process cannot access its custody root. Restore the original recovery phrase or complete backup before synchronizing, registering DUST, or sending funds." }
                        p { class: "consent-copy", "Do not initialize a replacement root for this funded profile." }
                    }
                } else if observation_only {
                    article { class: "surface-card account-sync-card", role: "status",
                        p { class: "card-eyebrow", "PreProd observation" }
                        h2 { "Balances only" }
                        p { "This recovery profile exposes synchronization and receive addresses only. Sending, DUST registration, proving, and transaction submission are disabled for this slice." }
                    }
                } else if protection_unlocked {
                    DustRegistrationPanel {
                        profile_id: active_profile.id.clone(),
                        dust_balance_positive,
                    }
                }

                div { class: "dashboard-grid",
                    article { class: "surface-card",
                        p { class: "card-eyebrow", "Receive" }
                        if !protected_account || account.addresses.is_empty() {
                            h2 { "Address unavailable" }
                            p { "Activate and derive this profile's protected Midnight account before sharing a holder-controlled address." }
                        } else {
                            for address in account.addresses.iter() {
                                ReceiveAddress {
                                    key: "{address.kind}",
                                    kind: address.kind.clone(),
                                    value: address.value.clone(),
                                }
                            }
                            p { "Each QR, clipboard copy, and share sheet contains exactly the public receive address shown." }
                        }
                    }
                    AccountActivityCard { account: (*account).clone(), unavailable }
                }

                if wallet_write_actions_available(observation_only) {
                    SubmissionRecoveryPane { profile_id: active_profile.id.clone() }
                }

                if wallet_write_actions_available(observation_only) && protected_account && protection_unlocked && account.sync.state == "synced" {
                    if let (Some(unshielded), Some(shielded)) = (
                        account.addresses.iter().find(|address| address.kind == "unshielded"),
                        account.addresses.iter().find(|address| address.kind == "shielded"),
                    ) {
                        SendTransferPanel {
                            profile_id: active_profile.id.clone(),
                            active_network_id: account.network_id.clone(),
                            unshielded_receive_address: unshielded.value.clone(),
                            shielded_receive_address: shielded.value.clone(),
                            night_balance: balance_for(&account, "NIGHT").cloned(),
                        }
                    }
                }
            }
        }
    }
}

pub(super) async fn activate_protected_account(
    services: WalletUiServices,
    profile_id: String,
    current: WalletSecurityStatusView,
) -> Result<WalletSecurityStatusView, String> {
    match run_ui_blocking(move || {
        let command = || WalletProfileSecurityCommand {
            profile_id: profile_id.clone(),
        };
        let security = match current.state_name() {
            "Uninitialized" => services
                .initialize_wallet_security()
                .execute(command())
                .map_err(|error| error.to_string())?,
            "Locked" => services
                .unlock_wallet()
                .execute(command())
                .map_err(|error| error.to_string())?,
            "Unlocked" => current,
            _ => return Err("wallet protection is unavailable".to_owned()),
        };
        services
            .derive_wallet_account()
            .execute(DeriveWalletAccountCommand {
                profile_id,
                account_index: 0,
                address_index: 0,
            })
            .map_err(|error| error.to_string())?;
        Ok(security)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => Err(error.to_string()),
    }
}

fn account_activation_operation(status: WalletSecurityStatusView) -> AccountOperation {
    match status.state_name() {
        "Uninitialized" => AccountOperation::Initializing,
        "Locked" => AccountOperation::Unlocking,
        _ => AccountOperation::Deriving,
    }
}

pub(super) fn has_protected_account(account: &WalletAccountView) -> bool {
    account
        .account_id
        .as_deref()
        .is_some_and(|account_id| account_id.starts_with("midnight_account_"))
        && account
            .addresses
            .iter()
            .any(|address| address.kind == "unshielded")
        && account
            .addresses
            .iter()
            .any(|address| address.kind == "shielded")
}

pub(super) const fn wallet_write_actions_available(observation_only: bool) -> bool {
    !observation_only
}

pub(super) fn wallet_account_activation_available(
    observation_only: bool,
    protection_available: bool,
    protection_state: &str,
    protected_account: bool,
    custody_recovery_required: bool,
) -> bool {
    protection_available
        && !custody_recovery_required
        && (!observation_only || protection_state != "Uninitialized")
        && (protection_state != "Unlocked" || !protected_account)
}
