// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use dioxus::prelude::*;
use oxid_wallet_application::{
    CreateWalletProfileCommand, CreateWalletProfileUseCase, GetActiveWalletProfileUseCase,
    ListWalletProfilesUseCase, SelectWalletProfileCommand, SelectWalletProfileUseCase,
    WalletProfileView,
};

const STYLES: &str = include_str!("../assets/styles.css");

/// Incoming capabilities made available to Dioxus by the composition root.
#[derive(Clone)]
pub struct WalletUiServices {
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
    list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
    select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
    get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
}

impl WalletUiServices {
    #[must_use]
    pub const fn new(
        create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
        list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
        select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
        get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
    ) -> Self {
        Self {
            create_wallet_profile,
            list_wallet_profiles,
            select_wallet_profile,
            get_active_wallet_profile,
        }
    }

    #[must_use]
    pub fn create_wallet_profile(&self) -> Arc<dyn CreateWalletProfileUseCase> {
        Arc::clone(&self.create_wallet_profile)
    }

    #[must_use]
    pub fn list_wallet_profiles(&self) -> Arc<dyn ListWalletProfilesUseCase> {
        Arc::clone(&self.list_wallet_profiles)
    }

    #[must_use]
    pub fn select_wallet_profile(&self) -> Arc<dyn SelectWalletProfileUseCase> {
        Arc::clone(&self.select_wallet_profile)
    }

    #[must_use]
    pub fn get_active_wallet_profile(&self) -> Arc<dyn GetActiveWalletProfileUseCase> {
        Arc::clone(&self.get_active_wallet_profile)
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProfileSessionState {
    Loading,
    Onboarding,
    Choosing(Vec<WalletProfileView>),
    Active(WalletProfileView),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProfileListState {
    Loading,
    Ready(Vec<WalletProfileView>),
    Failed(String),
}

/// Oxid's Dioxus incoming adapter and mobile-first application shell.
#[component]
pub fn App() -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut profile_session = use_signal(|| ProfileSessionState::Loading);
    let mut active_destination = use_signal(|| Destination::Assets);
    let mut menu_open = use_signal(|| false);
    let services_for_load = services.clone();
    use_effect(move || {
        profile_session.set(load_profile_session(&services_for_load));
    });

    let session = profile_session.read().clone();
    let ProfileSessionState::Active(active_profile) = session else {
        return rsx! {
            style { {STYLES} }
            ProfileGateway {
                state: session,
                on_selected: move |profile| {
                    profile_session.set(ProfileSessionState::Active(profile));
                    active_destination.set(Destination::Assets);
                },
                on_retry: move |_| {
                    profile_session.set(load_profile_session(&services));
                },
            }
        };
    };

    let active = *active_destination.read();
    let profile_monogram = profile_monogram(&active_profile.display_name);

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
                        "{profile_monogram}"
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
                    "{active_profile.display_name}"
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
                    Destination::Assets => rsx! { AssetsPage { active_profile: active_profile.clone() } },
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
                            active_profile: active_profile.clone(),
                            on_open_profile: move |_| active_destination.set(Destination::Profile),
                        }
                    },
                    Destination::Profile => rsx! {
                        ProfilePage {
                            active_profile: active_profile.clone(),
                            on_selected: move |profile| {
                                profile_session.set(ProfileSessionState::Active(profile));
                                active_destination.set(Destination::Assets);
                            },
                        }
                    },
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

fn load_profile_session(services: &WalletUiServices) -> ProfileSessionState {
    match services.get_active_wallet_profile().execute() {
        Ok(Some(profile)) => ProfileSessionState::Active(profile),
        Ok(None) => match services.list_wallet_profiles().execute() {
            Ok(profiles) => profile_session_route(None, profiles),
            Err(error) => ProfileSessionState::Failed(error.to_string()),
        },
        Err(error) => ProfileSessionState::Failed(error.to_string()),
    }
}

fn profile_session_route(
    active_profile: Option<WalletProfileView>,
    profiles: Vec<WalletProfileView>,
) -> ProfileSessionState {
    match active_profile {
        Some(profile) => ProfileSessionState::Active(profile),
        None if profiles.is_empty() => ProfileSessionState::Onboarding,
        None => ProfileSessionState::Choosing(profiles),
    }
}

fn profile_monogram(display_name: &str) -> String {
    display_name
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().collect())
        .unwrap_or_else(|| "O".to_owned())
}

#[component]
fn ProfileGateway(
    state: ProfileSessionState,
    on_selected: EventHandler<WalletProfileView>,
    on_retry: EventHandler<MouseEvent>,
) -> Element {
    let content = match state {
        ProfileSessionState::Loading => rsx! {
            section {
                class: "gateway-state surface-card",
                role: "status",
                aria_live: "polite",
                aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                h1 { "Loading wallet profiles" }
                p { "Restoring public profile metadata and the last active selection." }
            }
        },
        ProfileSessionState::Onboarding => rsx! {
            section { class: "page-heading onboarding-heading",
                p { class: "eyebrow", "Welcome to Oxid" }
                h1 { "Create your wallet profile" }
                p { "A profile is a public local label for wallet state. It never contains a seed, private key, credential, or recovery phrase." }
            }
            ProfileManager {
                profiles: Vec::new(),
                active_profile_id: None,
                onboarding: true,
                on_selected,
            }
        },
        ProfileSessionState::Choosing(profiles) => rsx! {
            section { class: "page-heading onboarding-heading",
                p { class: "eyebrow", "Choose a profile" }
                h1 { "Continue to your wallet" }
                p { "Select a previously created profile or add another public wallet label." }
            }
            ProfileManager {
                profiles,
                active_profile_id: None,
                onboarding: true,
                on_selected,
            }
        },
        ProfileSessionState::Failed(message) => rsx! {
            section { class: "gateway-state surface-card", role: "alert",
                span { class: "empty-state__mark", aria_hidden: "true", "!" }
                h1 { "Profiles could not be loaded" }
                p { "{message}" }
                button {
                    class: "secondary-action",
                    r#type: "button",
                    onclick: move |event| on_retry.call(event),
                    "Try again"
                }
            }
        },
        ProfileSessionState::Active(_) => return rsx! {},
    };

    rsx! {
        div { class: "app-shell onboarding-shell",
            header { class: "app-header onboarding-header",
                div { class: "brand-button",
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
            }
            main { class: "page-content", {content} }
        }
    }
}

#[component]
fn ProfileManager(
    profiles: Vec<WalletProfileView>,
    active_profile_id: Option<String>,
    onboarding: bool,
    on_selected: EventHandler<WalletProfileView>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let create_wallet_profile = services.create_wallet_profile();
    let select_wallet_profile = services.select_wallet_profile();
    let mut profile_list = use_signal(|| profiles);
    let mut display_name = use_signal(|| "My wallet".to_owned());
    let mut state = use_signal(|| CreationState::Idle);
    let can_submit = !display_name.read().trim().is_empty();

    let feedback = match state.read().clone() {
        CreationState::Idle => rsx! {
            p { class: "form-hint", "Only public profile metadata is stored here. Protected key operations remain a separate capability." }
        },
        CreationState::Created(profile) => rsx! {
            section { class: "result success", role: "status", aria_live: "polite",
                span { class: "capability-dot ready" }
                div {
                    strong { "Profile ready" }
                    p { "{profile.display_name}" }
                    code { "{profile.id}" }
                }
            }
        },
        CreationState::Failed(message) => rsx! {
            section { class: "result error", role: "alert",
                strong { "Profile action failed" }
                p { "{message}" }
            }
        },
    };

    let create_for_button = Arc::clone(&create_wallet_profile);
    let select_for_button = Arc::clone(&select_wallet_profile);
    rsx! {
        if !profile_list.read().is_empty() {
            section { class: "profile-list", aria_label: "Wallet profiles",
                for profile in profile_list.read().clone() {
                    {
                        let profile_id = profile.id.clone();
                        let is_active = active_profile_id.as_deref() == Some(profile.id.as_str());
                        let select = Arc::clone(&select_wallet_profile);
                        rsx! {
                            article { class: if is_active { "profile-row active" } else { "profile-row" },
                                div { class: "profile-row__identity",
                                    span { class: "profile-avatar", aria_hidden: "true", "{profile_monogram(&profile.display_name)}" }
                                    div {
                                        strong { "{profile.display_name}" }
                                        code { "{profile.id}" }
                                    }
                                }
                                if is_active {
                                    span { class: "status-pill success", "Active" }
                                } else {
                                    button {
                                        class: "secondary-action",
                                        r#type: "button",
                                        aria_label: "Use {profile.display_name}",
                                        onclick: move |_| {
                                            match select.execute(SelectWalletProfileCommand {
                                                profile_id: profile_id.clone(),
                                            }) {
                                                Ok(selected) => on_selected.call(selected),
                                                Err(error) => state.set(CreationState::Failed(error.to_string())),
                                            }
                                        },
                                        "Use profile"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        section { class: "profile-card surface-card",
            p { class: "card-eyebrow", if onboarding && profile_list.read().is_empty() { "First profile" } else { "Add profile" } }
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
                    match create_for_button.execute(command) {
                        Ok(created) => {
                            profile_list.write().push(created.clone());
                            match select_for_button.execute(SelectWalletProfileCommand {
                                profile_id: created.id,
                            }) {
                                Ok(selected) => {
                                    state.set(CreationState::Created(selected.clone()));
                                    on_selected.call(selected);
                                }
                                Err(error) => state.set(CreationState::Failed(error.to_string())),
                            }
                        }
                        Err(error) => state.set(CreationState::Failed(error.to_string())),
                    }
                },
                if onboarding && profile_list.read().is_empty() { "Create and continue" } else { "Create and use profile" }
            }
            {feedback}
        }
    }
}

#[component]
fn AssetsPage(active_profile: WalletProfileView) -> Element {
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
                strong { "{active_profile.display_name} is active" }
                p { "Profile selection is persisted locally. Asset custody, sync, and proving remain disabled until their reviewed adapters land." }
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
            CapabilityStatus { name: "Profile lifecycle", state: "Create · list · select · restore", ready: true }
            CapabilityStatus { name: "Profile metadata store", state: "Persistent · public metadata only", ready: true }
            CapabilityStatus { name: "Protected secret store", state: "Not connected", ready: false }
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
fn SettingsPage(
    active_profile: WalletProfileView,
    on_open_profile: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Local controls" }
            h1 { "Settings" }
            p { "Security-sensitive settings appear only when their application ports and platform adapters are available." }
        }
        article { class: "settings-card surface-card",
            div {
                p { class: "card-eyebrow", "Profile" }
                h2 { "{active_profile.display_name}" }
                p { "Public profile metadata and active selection are persisted. Seeds and keys are never part of this record." }
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
fn ProfilePage(
    active_profile: WalletProfileView,
    on_selected: EventHandler<WalletProfileView>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut profiles = use_signal(|| ProfileListState::Loading);
    use_effect(move || {
        profiles.set(services.list_wallet_profiles().execute().map_or_else(
            |error| ProfileListState::Failed(error.to_string()),
            ProfileListState::Ready,
        ));
    });

    let content = match profiles.read().clone() {
        ProfileListState::Loading => rsx! {
            section { class: "gateway-state surface-card", role: "status", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                strong { "Loading profiles" }
            }
        },
        ProfileListState::Ready(loaded) => rsx! {
            ProfileManager {
                profiles: loaded,
                active_profile_id: Some(active_profile.id),
                onboarding: false,
                on_selected,
            }
        },
        ProfileListState::Failed(message) => rsx! {
            section { class: "result error", role: "alert",
                strong { "Profiles could not be loaded" }
                p { "{message}" }
            }
        },
    };

    rsx! {
        section { class: "page-heading profile-heading",
            p { class: "eyebrow", "Wallet profile" }
            h1 { "Manage profiles" }
            p { "Choose the active public wallet context or add another. Account keys, DIDs, and credentials remain behind separate protected capabilities." }
        }
        {content}
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

    #[test]
    fn profile_route_gates_first_launch_and_restores_active_selection() {
        let profile = WalletProfileView {
            id: "profile_test".to_owned(),
            display_name: "Primary".to_owned(),
            created_at_millis: 42,
        };

        assert_eq!(
            profile_session_route(None, Vec::new()),
            ProfileSessionState::Onboarding
        );
        assert_eq!(
            profile_session_route(None, vec![profile.clone()]),
            ProfileSessionState::Choosing(vec![profile.clone()])
        );
        assert_eq!(
            profile_session_route(Some(profile.clone()), vec![profile.clone()]),
            ProfileSessionState::Active(profile)
        );
    }

    #[test]
    fn profile_monogram_uses_the_first_visible_character() {
        assert_eq!(profile_monogram("  primary"), "P");
        assert_eq!(profile_monogram("---"), "O");
    }
}
