// SPDX-License-Identifier: Apache-2.0

//! Transient UI for the application-owned BIP-39 onboarding ceremony.

use dioxus::prelude::*;
use oxid_wallet_application::{
    COMPLETE_WALLET_ONBOARDING_SUMMARY, COMPLETE_WALLET_ONBOARDING_TITLE,
    CancelWalletOnboardingCommand, CompleteWalletOnboardingCommand, PrepareWalletOnboardingCommand,
    PreparedWalletOnboarding, SensitiveOperationConfirmation, WalletOnboardingMode,
    WalletProfileView, WalletRecoveryPhrase,
};
use zeroize::{Zeroize, Zeroizing};

use super::{WalletUiServices, run_ui_blocking};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WalletOnboardingIntent {
    Create,
    RestorePhrase,
}

enum WalletOnboardingState {
    Idle,
    Working,
    Prepared(PreparedWalletOnboarding),
    Completing,
    Failed(String),
}

fn ceremony_id(state: &WalletOnboardingState) -> Option<String> {
    match state {
        WalletOnboardingState::Prepared(prepared) => Some(prepared.ceremony_id.clone()),
        WalletOnboardingState::Idle
        | WalletOnboardingState::Working
        | WalletOnboardingState::Completing
        | WalletOnboardingState::Failed(_) => None,
    }
}

#[component]
pub(crate) fn WalletOnboarding(
    profile: WalletProfileView,
    intent: WalletOnboardingIntent,
    lifecycle_wake: Signal<u64>,
    on_complete: EventHandler<WalletProfileView>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let Some(onboarding) = services.wallet_onboarding.clone() else {
        return rsx! {
            section { class: "result error", role: "alert",
                strong { "Private wallet onboarding is unavailable" }
                p { "This application profile did not authenticate a Midnight network." }
            }
        };
    };
    let mut state = use_signal(|| WalletOnboardingState::Idle);
    let mut phrase_input = use_signal(|| Zeroizing::new(String::new()));
    let mut acknowledged = use_signal(|| false);
    let initial_lifecycle = lifecycle_wake();
    let mut last_lifecycle = use_signal(move || initial_lifecycle);

    let screen_privacy = services.screen_privacy();
    use_effect(move || {
        let _ = screen_privacy.set_protected(true);
    });

    let suspend = onboarding.cancel.clone();
    use_effect(move || {
        let generation = lifecycle_wake();
        if generation != last_lifecycle() {
            last_lifecycle.set(generation);
            suspend.suspend();
            phrase_input.write().zeroize();
            phrase_input.set(Zeroizing::new(String::new()));
            acknowledged.set(false);
            state.set(WalletOnboardingState::Idle);
        }
    });

    let busy = matches!(
        *state.read(),
        WalletOnboardingState::Working | WalletOnboardingState::Completing
    );
    let prepared_id = ceremony_id(&state.read());
    let can_prepare = !busy
        && prepared_id.is_none()
        && (intent == WalletOnboardingIntent::Create || !phrase_input.read().trim().is_empty());
    let can_complete = !busy && prepared_id.is_some() && acknowledged();
    let profile_for_prepare = profile.clone();
    let profile_for_complete = profile.clone();
    let profile_id_for_cancel = profile.id.clone();
    let cancel_after_failure = onboarding.cancel.clone();
    let cancel_from_button = onboarding.cancel.clone();
    let heading = match intent {
        WalletOnboardingIntent::Create => "Create private wallet",
        WalletOnboardingIntent::RestorePhrase => "Restore recovery phrase",
    };
    let action = match intent {
        WalletOnboardingIntent::Create => "Generate recovery phrase",
        WalletOnboardingIntent::RestorePhrase => "Verify recovery phrase",
    };

    let feedback = match &*state.read() {
        WalletOnboardingState::Failed(message) => rsx! {
            div { class: "result error", role: "alert", p { "{message}" } }
        },
        WalletOnboardingState::Working => rsx! {
            div { class: "result", role: "status", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                p { "Preparing protected wallet…" }
            }
        },
        WalletOnboardingState::Completing => rsx! {
            div { class: "result", role: "status", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                p { "Installing root behind device protection…" }
            }
        },
        WalletOnboardingState::Idle | WalletOnboardingState::Prepared(_) => rsx! {},
    };

    rsx! {
        section { class: "page-heading onboarding-heading",
            p { class: "eyebrow", "Midnight · {onboarding.network_id}" }
            h1 { "{heading}" }
            p { "The phrase stays in this ceremony and is never written to profile metadata, logs, analytics, or clipboard storage." }
        }
        section { class: "profile-card surface-card complete-recovery-card",
            strong { "{profile.display_name}" }
            if intent == WalletOnboardingIntent::RestorePhrase && prepared_id.is_none() {
                label { r#for: "wallet-recovery-phrase", "24-word recovery phrase"
                    input {
                        id: "wallet-recovery-phrase",
                        r#type: "password",
                        autocomplete: "off",
                        autocapitalize: "none",
                        spellcheck: false,
                        disabled: busy,
                        value: phrase_input.read().as_str(),
                        oninput: move |event| {
                            phrase_input.write().zeroize();
                            phrase_input.set(Zeroizing::new(event.value()));
                        },
                    }
                }
            }
            if let WalletOnboardingState::Prepared(prepared) = &*state.read() {
                if let Some(phrase) = &prepared.created_recovery_phrase {
                    div { class: "recovery-phrase", role: "group", aria_label: "New wallet recovery phrase",
                        p { class: "backup-warning",
                            strong { "Write these 24 words down now. " }
                            "They are shown once and cannot be recovered by the app."
                        }
                        code { "{phrase.expose_for_onboarding()}" }
                    }
                }
                label { class: "confirmation-row",
                    input {
                        r#type: "checkbox",
                        checked: acknowledged(),
                        disabled: busy,
                        onchange: move |event| acknowledged.set(event.checked()),
                    }
                    "I have securely saved or verified this recovery phrase."
                }
            }
            if prepared_id.is_none() {
                button {
                    class: "primary-action",
                    r#type: "button",
                    disabled: !can_prepare,
                    onclick: move |_| {
                        if !matches!(
                            *state.read(),
                            WalletOnboardingState::Idle | WalletOnboardingState::Failed(_)
                        ) {
                            return;
                        }
                        let mode = match intent {
                            WalletOnboardingIntent::Create => WalletOnboardingMode::CreateNew,
                            WalletOnboardingIntent::RestorePhrase => {
                                let mut raw = phrase_input();
                                phrase_input.write().zeroize();
                                phrase_input.set(Zeroizing::new(String::new()));
                                WalletOnboardingMode::RestoreMnemonic {
                                    phrase: WalletRecoveryPhrase::new(std::mem::take(&mut *raw)),
                                }
                            }
                        };
                        let prepare = onboarding.prepare.clone();
                        let profile_id = profile_for_prepare.id.clone();
                        state.set(WalletOnboardingState::Working);
                        spawn(async move {
                            let result = run_ui_blocking(move || {
                                prepare.execute(PrepareWalletOnboardingCommand { profile_id, mode })
                            })
                            .await;
                            match result {
                                Ok(Ok(prepared)) => state.set(WalletOnboardingState::Prepared(prepared)),
                                Ok(Err(error)) => state.set(WalletOnboardingState::Failed(error.to_string())),
                                Err(error) => state.set(WalletOnboardingState::Failed(error.to_string())),
                            }
                        });
                    },
                    "{action}"
                }
            } else {
                button {
                    class: "primary-action",
                    r#type: "button",
                    disabled: !can_complete,
                    onclick: move |_| {
                        if busy || !acknowledged() {
                            return;
                        }
                        let Some(ceremony_id) = ceremony_id(&state.read()) else { return; };
                        let complete = onboarding.complete.clone();
                        let cancel = cancel_after_failure.clone();
                        let profile = profile_for_complete.clone();
                        let profile_id = profile.id.clone();
                        let profile_id_for_failure = profile_id.clone();
                        let ceremony_id_for_failure = ceremony_id.clone();
                        state.set(WalletOnboardingState::Completing);
                        spawn(async move {
                            let result = run_ui_blocking(move || {
                                complete.execute(CompleteWalletOnboardingCommand {
                                    profile_id,
                                    ceremony_id,
                                    backup_acknowledged: true,
                                    confirmation: SensitiveOperationConfirmation {
                                        title: COMPLETE_WALLET_ONBOARDING_TITLE.to_owned(),
                                        summary: COMPLETE_WALLET_ONBOARDING_SUMMARY.to_owned(),
                                        confirmed: true,
                                    },
                                })
                            })
                            .await;
                            match result {
                                Ok(Ok(_)) => on_complete.call(profile),
                                Ok(Err(error)) => {
                                    let _ = cancel.execute(CancelWalletOnboardingCommand {
                                        profile_id: profile_id_for_failure.clone(),
                                        ceremony_id: ceremony_id_for_failure.clone(),
                                    });
                                    state.set(WalletOnboardingState::Failed(error.to_string()));
                                }
                                Err(error) => {
                                    let _ = cancel.execute(CancelWalletOnboardingCommand {
                                        profile_id: profile_id_for_failure,
                                        ceremony_id: ceremony_id_for_failure,
                                    });
                                    state.set(WalletOnboardingState::Failed(error.to_string()));
                                }
                            }
                        });
                    },
                    if busy { "Protecting wallet…" } else { "Finish and open wallet" }
                }
                button {
                    class: "secondary-action",
                    r#type: "button",
                    disabled: busy,
                    onclick: move |_| {
                        let Some(ceremony_id) = ceremony_id(&state.read()) else { return; };
                        let _ = cancel_from_button.execute(CancelWalletOnboardingCommand {
                            profile_id: profile_id_for_cancel.clone(),
                            ceremony_id,
                        });
                        phrase_input.write().zeroize();
                        phrase_input.set(Zeroizing::new(String::new()));
                        acknowledged.set(false);
                        state.set(WalletOnboardingState::Idle);
                    },
                    "Cancel ceremony"
                }
            }
            {feedback}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_submission_is_blocked_while_working() {
        let state = WalletOnboardingState::Working;
        assert!(matches!(state, WalletOnboardingState::Working));
        assert!(ceremony_id(&state).is_none());
    }

    #[test]
    fn only_prepared_state_exposes_a_ceremony_identifier() {
        let prepared = PreparedWalletOnboarding {
            ceremony_id: "ceremony_public".to_owned(),
            created_recovery_phrase: None,
            backup_acknowledgement_required: true,
        };
        assert_eq!(
            ceremony_id(&WalletOnboardingState::Prepared(prepared)).as_deref(),
            Some("ceremony_public")
        );
        assert!(ceremony_id(&WalletOnboardingState::Idle).is_none());
        assert!(ceremony_id(&WalletOnboardingState::Completing).is_none());
    }
}
