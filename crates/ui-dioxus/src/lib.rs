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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Destination {
    Assets,
    Dids,
    Credentials,
    Diagnostics,
    Settings,
    Profile,
}

impl Destination {
    const fn label(self) -> &'static str {
        match self {
            Self::Assets => "Assets",
            Self::Dids => "DIDs",
            Self::Credentials => "Credentials",
            Self::Diagnostics => "Diagnostics",
            Self::Settings => "Settings",
            Self::Profile => "Wallet profile",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            Self::Assets => LUCIDE_WALLET,
            Self::Dids => LUCIDE_FINGERPRINT,
            Self::Credentials => LUCIDE_BADGE_CHECK,
            Self::Diagnostics => LUCIDE_ACTIVITY,
            Self::Settings | Self::Profile => LUCIDE_SETTINGS_2,
        }
    }
}

const PRIMARY_DESTINATIONS: [Destination; 5] = [
    Destination::Assets,
    Destination::Dids,
    Destination::Credentials,
    Destination::Diagnostics,
    Destination::Settings,
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum CreationState {
    Idle,
    Created(WalletProfileView),
    Failed(String),
}

/// Oxid's Dioxus incoming adapter and mobile-first application shell.
#[component]
pub fn App() -> Element {
    let mut active_destination = use_signal(|| Destination::Assets);
    let mut menu_open = use_signal(|| false);
    let active = *active_destination.read();

    rsx! {
        style { {STYLES} }
        div { class: "app-shell",
            header { class: "app-header",
                button {
                    class: "brand-button",
                    r#type: "button",
                    aria_label: "Open Assets",
                    onclick: move |_| active_destination.set(Destination::Assets),
                    span { class: "oxid-mark", aria_hidden: "true",
                        span { class: "oxid-mark__dot" }
                        span { class: "oxid-mark__dot" }
                        span { class: "oxid-mark__dot" }
                    }
                    span { class: "wordmark",
                        strong { "oxid" }
                        small { "identity wallet" }
                    }
                }
                div { class: "header-actions",
                    button {
                        class: "profile-shortcut",
                        r#type: "button",
                        aria_label: "Open wallet profile",
                        title: "Wallet profile",
                        onclick: move |_| {
                            active_destination.set(Destination::Profile);
                            menu_open.set(false);
                        },
                        "O"
                    }
                    button {
                        class: if *menu_open.read() { "menu-button active" } else { "menu-button" },
                        r#type: "button",
                        aria_label: "Open navigation menu",
                        aria_expanded: if *menu_open.read() { "true" } else { "false" },
                        onclick: move |_| {
                            let next = !*menu_open.read();
                            menu_open.set(next);
                        },
                        span { aria_hidden: "true", "≡" }
                    }
                }
            }

            div { class: "page-context",
                span { class: "connection-state",
                    span { class: "status-dot" }
                    "Local foundation"
                }
                span { class: "page-context__title", "{active.label()}" }
            }

            if *menu_open.read() {
                nav { class: "menu-dropdown", aria_label: "All wallet destinations",
                    for destination in [
                        Destination::Assets,
                        Destination::Dids,
                        Destination::Credentials,
                        Destination::Diagnostics,
                        Destination::Settings,
                        Destination::Profile,
                    ] {
                        button {
                            key: "{destination.label()}",
                            class: if active == destination { "menu-item active" } else { "menu-item" },
                            r#type: "button",
                            onclick: move |_| {
                                active_destination.set(destination);
                                menu_open.set(false);
                            },
                            "{destination.label()}"
                        }
                    }
                }
            }

            main { class: "page-content",
                match active {
                    Destination::Assets => rsx! { AssetsPage {} },
                    Destination::Dids => rsx! {
                        DeferredPage {
                            eyebrow: "Decentralized identity",
                            title: "Your DIDs",
                            description: "DID inventory and did:midnight lifecycle operations will arrive behind identity-owned ports.",
                            empty_title: "No DID adapter connected",
                            empty_body: "Create, resolve, update, deactivate, and signing flows are tracked in the parity backlog.",
                        }
                    },
                    Destination::Credentials => rsx! {
                        DeferredPage {
                            eyebrow: "Identity centre",
                            title: "Credentials",
                            description: "Credentials will remain local-first, holder-controlled, and independent from chain account state.",
                            empty_title: "Credential wallet is queued",
                            empty_body: "The encrypted store, verification pipeline, OID4VCI, OID4VP, SIOP, and consent flows are not connected yet.",
                        }
                    },
                    Destination::Diagnostics => rsx! { DiagnosticsPage {} },
                    Destination::Settings => rsx! {
                        SettingsPage {
                            on_open_profile: move |_| active_destination.set(Destination::Profile),
                        }
                    },
                    Destination::Profile => rsx! { ProfilePage {} },
                }
            }

            nav { class: "bottom-nav", aria_label: "Primary wallet destinations",
                for destination in PRIMARY_DESTINATIONS {
                    {
                        let is_active = active == destination;
                        rsx! {
                            button {
                                key: "{destination.label()}",
                                class: if is_active { "bottom-nav__item active" } else { "bottom-nav__item" },
                                r#type: "button",
                                aria_label: "{destination.label()}",
                                aria_current: if is_active { "page" } else { "false" },
                                onclick: move |_| {
                                    active_destination.set(destination);
                                    menu_open.set(false);
                                },
                                span {
                                    class: "bottom-nav__icon",
                                    aria_hidden: "true",
                                    dangerous_inner_html: "{destination.icon()}",
                                }
                                span { class: "bottom-nav__label", "{destination.label()}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AssetsPage() -> Element {
    rsx! {
        section { class: "wallet-hero",
            p { class: "eyebrow", "Wallet overview" }
            div { class: "wallet-hero__number-row",
                h1 { "—" }
                span { "NIGHT" }
            }
            div { class: "dust-pill",
                strong { "—" }
                span { "DUST" }
            }
            p { class: "wallet-hero__hint", "Midnight account and balance adapters are not connected in this slice." }
        }

        section { class: "trust-line", role: "status",
            span { class: "trust-line__icon", aria_hidden: "true", "◇" }
            div {
                strong { "Foundation ready" }
                p { "Profile creation works locally. Asset custody, sync, and proving remain disabled until their reviewed adapters land." }
            }
        }

        button { class: "primary-action", r#type: "button", disabled: true,
            "Connect Midnight wallet · queued"
        }

        div { class: "dashboard-grid",
            article { class: "surface-card",
                p { class: "card-eyebrow", "Receive" }
                h2 { "Address unavailable" }
                p { "A network-correct receive address and QR code will appear after account derivation is migrated." }
            }
            article { class: "surface-card",
                p { class: "card-eyebrow", "Activity" }
                h2 { "No synced history" }
                p { "Indexer-backed transaction history is part of the Midnight read-capability slice." }
            }
        }
    }
}

#[component]
fn DeferredPage(
    eyebrow: &'static str,
    title: &'static str,
    description: &'static str,
    empty_title: &'static str,
    empty_body: &'static str,
) -> Element {
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "{eyebrow}" }
            h1 { "{title}" }
            p { "{description}" }
        }
        article { class: "empty-state surface-card",
            span { class: "empty-state__mark", aria_hidden: "true", "◇" }
            h2 { "{empty_title}" }
            p { "{empty_body}" }
            span { class: "status-pill", "Migration queued" }
        }
    }
}

#[component]
fn DiagnosticsPage() -> Element {
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Capability status" }
            h1 { "Diagnostics" }
            p { "This view reports only capabilities that are actually composed into the current application." }
        }
        div { class: "diagnostic-grid",
            CapabilityStatus { name: "Profile use case", state: "Ready", ready: true }
            CapabilityStatus { name: "In-memory profile store", state: "Development only", ready: true }
            CapabilityStatus { name: "Midnight ledger", state: "Not connected", ready: false }
            CapabilityStatus { name: "Proof provider", state: "Not connected", ready: false }
            CapabilityStatus { name: "DID adapter", state: "Not connected", ready: false }
            CapabilityStatus { name: "Credential protocols", state: "Not connected", ready: false }
        }
    }
}

#[component]
fn CapabilityStatus(name: &'static str, state: &'static str, ready: bool) -> Element {
    rsx! {
        article { class: "capability-row",
            span { class: if ready { "capability-dot ready" } else { "capability-dot queued" } }
            div {
                strong { "{name}" }
                p { "{state}" }
            }
        }
    }
}

#[component]
fn SettingsPage(on_open_profile: EventHandler<MouseEvent>) -> Element {
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Local controls" }
            h1 { "Settings" }
            p { "Security-sensitive settings appear only when their application ports and platform adapters are available." }
        }
        article { class: "settings-card surface-card",
            div {
                p { class: "card-eyebrow", "Profile" }
                h2 { "Wallet profile" }
                p { "The M0 profile page is retained during shell migration. Persistent selection and onboarding are tracked separately." }
            }
            button {
                class: "secondary-action",
                r#type: "button",
                onclick: move |event| on_open_profile.call(event),
                "Open profile page"
            }
        }
        article { class: "settings-card surface-card",
            div {
                p { class: "card-eyebrow", "Privacy" }
                h2 { "Local-first · telemetry off" }
                p { "No chain, DID, credential, analytics, or remote-storage adapter is active in this slice." }
            }
            span { class: "status-pill success", "Enforced" }
        }
    }
}

#[component]
fn ProfilePage() -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut display_name = use_signal(|| "My wallet".to_owned());
    let mut state = use_signal(|| CreationState::Idle);
    let can_submit = !display_name.read().trim().is_empty();

    let feedback = match state.read().clone() {
        CreationState::Idle => rsx! {
            p { class: "form-hint", "Profiles contain public labels only. Keys are created through separate protected capabilities." }
        },
        CreationState::Created(profile) => rsx! {
            section { class: "result success", role: "status",
                span { class: "capability-dot ready" }
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
        section { class: "page-heading profile-heading",
            p { class: "eyebrow", "Wallet profile" }
            h1 { "Create your profile" }
            p { "Start with a local public identity for this wallet. Account keys, DIDs, and credentials attach through protected capabilities in later slices." }
        }
        section { class: "profile-card surface-card",
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
                class: "primary-action",
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
    }
}

// Inline Lucide icons retained from the reviewed prototype shell. Lucide's ISC
// notice is reproduced in THIRD_PARTY_NOTICES.md.
const LUCIDE_WALLET: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 7V4a1 1 0 0 0-1-1H5a2 2 0 0 0 0 4h15a1 1 0 0 1 1 1v4h-3a2 2 0 0 0 0 4h3a1 1 0 0 0 1-1v-2a1 1 0 0 0-1-1"/><path d="M3 5v14a2 2 0 0 0 2 2h15a1 1 0 0 0 1-1v-4"/></svg>"#;
const LUCIDE_FINGERPRINT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 10a2 2 0 0 0-2 2c0 1.02-.1 2.51-.26 4"/><path d="M14 13.12c0 2.38 0 6.38-1 8.88"/><path d="M17.29 21.02c.12-.6.43-2.3.5-3.02"/><path d="M2 12a10 10 0 0 1 18-6"/><path d="M2 16h.01"/><path d="M21.8 16c.2-2 .131-5.354 0-6"/><path d="M5 19.5C5.5 18 6 15 6 12a6 6 0 0 1 .34-2"/><path d="M8.65 22c.21-.66.45-1.32.57-2"/><path d="M9 6.8a6 6 0 0 1 9 5.2c0 .47 0 1.17-.02 2"/></svg>"#;
const LUCIDE_BADGE_CHECK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3.85 8.62a4 4 0 0 1 4.78-4.77 4 4 0 0 1 6.74 0 4 4 0 0 1 4.78 4.78 4 4 0 0 1 0 6.74 4 4 0 0 1-4.77 4.78 4 4 0 0 1-6.75 0 4 4 0 0 1 0-6.76Z"/><path d="m9 12 2 2 4-4"/></svg>"#;
const LUCIDE_ACTIVITY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.5.5 0 0 1-.96 0L9.24 2.18a.5.5 0 0 0-.96 0l-2.35 8.36A2 2 0 0 1 4 12H2"/></svg>"#;
const LUCIDE_SETTINGS_2: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 7h-9"/><path d="M14 17H5"/><circle cx="17" cy="17" r="3"/><circle cx="7" cy="7" r="3"/></svg>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_navigation_matches_the_reviewed_wallet_shell() {
        let labels = PRIMARY_DESTINATIONS.map(Destination::label);

        assert_eq!(
            labels,
            ["Assets", "DIDs", "Credentials", "Diagnostics", "Settings"]
        );
    }

    #[test]
    fn profile_remains_an_explicit_non_primary_destination() {
        assert_eq!(Destination::Profile.label(), "Wallet profile");
        assert!(!PRIMARY_DESTINATIONS.contains(&Destination::Profile));
    }
}
