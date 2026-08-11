// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use dioxus::prelude::*;
use oxid_wallet_application::{
    CreateWalletProfileCommand, CreateWalletProfileUseCase, WalletProfileView,
};

const STYLES: &str = include_str!("../assets/styles.css");

/// Incoming capabilities made available to Dioxus by the composition root.
#[derive(Clone)]
pub struct WalletUiServices {
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
}

impl WalletUiServices {
    #[must_use]
    pub const fn new(create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>) -> Self {
        Self {
            create_wallet_profile,
        }
    }

    #[must_use]
    pub fn create_wallet_profile(&self) -> Arc<dyn CreateWalletProfileUseCase> {
        Arc::clone(&self.create_wallet_profile)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CreationState {
    Idle,
    Created(WalletProfileView),
    Failed(String),
}

/// Minimal M0 Dioxus screen for the Create Wallet Profile use case.
#[component]
pub fn App() -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut display_name = use_signal(|| "My wallet".to_owned());
    let mut state = use_signal(|| CreationState::Idle);
    let can_submit = !display_name.read().trim().is_empty();

    let feedback = match state.read().clone() {
        CreationState::Idle => rsx! {
            p { class: "hint", "Profiles contain public labels only. Keys are created through separate protected capabilities." }
        },
        CreationState::Created(profile) => rsx! {
            section { class: "result success", role: "status",
                span { class: "status-dot" }
                div {
                    strong { "Profile created" }
                    p { "{profile.display_name}" }
                    code { "{profile.id}" }
                }
            }
        },
        CreationState::Failed(message) => rsx! {
            section { class: "result error", role: "alert",
                strong { "Could not create profile" }
                p { "{message}" }
            }
        },
    };

    rsx! {
        style { {STYLES} }
        main { class: "shell",
            header { class: "brand",
                div { class: "mark", aria_label: "Oxid", "O" }
                div {
                    p { class: "eyebrow", "IDENTITY WALLET" }
                    h1 { "Create your wallet profile" }
                }
            }

            section { class: "card",
                p { class: "lede", "Start with a local profile. Assets, DIDs, and credentials will attach through explicit capabilities in later slices." }
                label { r#for: "profile-name", "Profile name" }
                input {
                    id: "profile-name",
                    r#type: "text",
                    maxlength: 64,
                    autocomplete: "off",
                    value: "{display_name}",
                    oninput: move |event| display_name.set(event.value()),
                }
                button {
                    r#type: "button",
                    disabled: !can_submit,
                    onclick: move |_| {
                        let command = CreateWalletProfileCommand {
                            display_name: display_name.read().clone(),
                        };
                        let next_state = services
                            .create_wallet_profile()
                            .execute(command)
                            .map_or_else(
                                |error| CreationState::Failed(error.to_string()),
                                CreationState::Created,
                            );
                        state.set(next_state);
                    },
                    "Create profile"
                }
                {feedback}
            }

            footer { "Local-first · telemetry off · Apache-2.0" }
        }
    }
}
