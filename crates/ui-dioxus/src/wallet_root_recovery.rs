// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(feature = "preprod-observation")]
#[component]
pub(super) fn WalletRootRecoveryForm(
    profile: WalletProfileView,
    lifecycle_wake: Signal<u64>,
    on_recovered: EventHandler<WalletProfileView>,
    on_cancel: Option<EventHandler<WalletProfileView>>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let Some(capability) = services.wallet_root_recovery.clone() else {
        return rsx! {};
    };
    let mut root_input = use_signal(WalletRootInput::default);
    let mut confirmed = use_signal(|| false);
    let mut state = use_signal(|| WalletRootRecoveryUiState::Idle);
    let busy = matches!(*state.read(), WalletRootRecoveryUiState::Working);
    let can_recover = !busy && confirmed() && !root_input.read().is_empty();
    let mut lifecycle_root = root_input;
    let mut lifecycle_confirmation = confirmed;
    use_effect(move || {
        let _generation = lifecycle_wake();
        lifecycle_root.write().clear();
        lifecycle_confirmation.set(false);
    });
    let feedback = match state.read().clone() {
        WalletRootRecoveryUiState::Idle => rsx! {},
        WalletRootRecoveryUiState::Working => rsx! {
            div { class: "result", role: "status", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                p { "Waiting for device authorization and deriving the canonical account…" }
            }
        },
        WalletRootRecoveryUiState::Failed(message) => rsx! {
            div { class: "result error", role: "alert",
                strong { "PreProd recovery did not finish" }
                p { "{message}" }
                p { "The root field was cleared. Re-enter it to retry." }
            }
        },
    };
    let network_id = capability.network_id.clone();
    let recover = Arc::clone(&capability.recover);
    let recovery_profile = profile.clone();
    let cancel_profile = profile.clone();

    rsx! {
        section { class: "profile-card surface-card complete-recovery-card",
            p { class: "card-eyebrow", "Existing Midnight wallet" }
            h2 { "Recover on {ui::midnight_network(&network_id)}" }
            p {
                "Enter the existing Midnight wallet root only into this field. It is installed into this empty profile behind native device protection, then account 0/address 0 is derived for balance observation."
            }
            p { class: "backup-warning",
                strong { "Read-only PreProd journey. " }
                "This profile can synchronize NIGHT, shielded token, and DUST balances. Sending, DUST registration, proving, and submission are not offered."
            }
            label { r#for: "wallet-root-seed", "Midnight wallet root (64 lowercase hex characters)"
                input {
                    id: "wallet-root-seed",
                    r#type: "password",
                    minlength: 64,
                    maxlength: 64,
                    autocomplete: "off",
                    autocapitalize: "none",
                    spellcheck: false,
                    disabled: busy,
                    value: root_input.read().as_str(),
                    oninput: move |event| root_input.set(WalletRootInput::new(event.value())),
                }
            }
            label { class: "confirmation-row",
                input {
                    r#type: "checkbox",
                    checked: confirmed(),
                    disabled: busy,
                    onchange: move |event| confirmed.set(event.checked()),
                }
                "I confirm recovery into this empty profile and understand that the existing root will replace no local custody."
            }
            div { class: "transfer-actions",
                if let Some(cancel) = on_cancel {
                    button {
                        class: "secondary-action",
                        r#type: "button",
                        disabled: busy,
                        onclick: move |_| {
                            root_input.write().clear();
                            confirmed.set(false);
                            cancel.call(cancel_profile.clone());
                        },
                        "Cancel"
                    }
                }
                button {
                    class: "primary-action",
                    r#type: "button",
                    disabled: !can_recover,
                    onclick: move |_| {
                        let raw = root_input.write().take();
                        confirmed.set(false);
                        let root = match WalletRootSeed::parse_hex(&raw) {
                            Ok(root) => root,
                            Err(error) => {
                                state.set(WalletRootRecoveryUiState::Failed(error.to_string()));
                                return;
                            }
                        };
                        let recover = Arc::clone(&recover);
                        let profile = recovery_profile.clone();
                        let profile_id = profile.id.clone();
                        state.set(WalletRootRecoveryUiState::Working);
                        spawn(async move {
                            let result = run_ui_blocking(move || {
                                recover.execute(RecoverWalletRootCommand {
                                    profile_id,
                                    root,
                                    confirmation: SensitiveOperationConfirmation {
                                        title: RECOVER_WALLET_ROOT_TITLE.to_owned(),
                                        summary: RECOVER_WALLET_ROOT_SUMMARY.to_owned(),
                                        confirmed: true,
                                    },
                                })
                            })
                            .await;
                            match result {
                                Ok(Ok(_)) => on_recovered.call(profile),
                                Ok(Err(error)) => state.set(
                                    WalletRootRecoveryUiState::Failed(error.to_string()),
                                ),
                                Err(error) => state.set(
                                    WalletRootRecoveryUiState::Failed(error.to_string()),
                                ),
                            }
                        });
                    },
                    if busy { "Recovering…" } else { "Authorize and recover" }
                }
            }
            {feedback}
        }
    }
}
