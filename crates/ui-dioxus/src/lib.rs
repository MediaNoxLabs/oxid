// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use dioxus::prelude::*;
use oxid_wallet_application::{
    AuthorizeWalletTransferCommand, AuthorizeWalletTransferUseCase, CreateWalletProfileCommand,
    CreateWalletProfileUseCase, DeriveWalletAccountCommand, DeriveWalletAccountUseCase,
    GetActiveWalletProfileUseCase, GetWalletAccountUseCase, GetWalletSecurityStatusUseCase,
    GetWalletTransferDraftUseCase, InitializeWalletSecurityUseCase, ListWalletNetworksUseCase,
    ListWalletProfilesUseCase, LockWalletUseCase, PrepareWalletTransferCommand,
    PrepareWalletTransferUseCase, SelectWalletNetworkCommand, SelectWalletNetworkUseCase,
    SelectWalletProfileCommand, SelectWalletProfileUseCase, SensitiveOperationConfirmation,
    SubmitWalletTransferCommand, SubmitWalletTransferUseCase, SyncWalletAccountUseCase,
    UnlockWalletUseCase, WalletAccountQuery, WalletAccountView, WalletNetworkListView,
    WalletProfileSecurityCommand, WalletProfileView, WalletSecurityStatusView,
    WalletTransferDraftQuery, WalletTransferPreviewView, WalletTransferSubmissionView,
};

const STYLES: &str = include_str!("../assets/styles.css");

/// Incoming capabilities made available to Dioxus by the composition root.
#[derive(Clone)]
pub struct WalletUiServices {
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
    list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
    select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
    get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
    get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase>,
    initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase>,
    unlock_wallet: Arc<dyn UnlockWalletUseCase>,
    lock_wallet: Arc<dyn LockWalletUseCase>,
    list_wallet_networks: Arc<dyn ListWalletNetworksUseCase>,
    select_wallet_network: Arc<dyn SelectWalletNetworkUseCase>,
    derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase>,
    get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
    sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
    prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
    authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
    submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
    get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
}

/// Public profile lifecycle use cases consumed by the wallet shell.
pub struct WalletProfileUiServices {
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
    list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
    select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
    get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
}

impl WalletProfileUiServices {
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
}

/// Wallet protection use cases consumed by account and settings views.
pub struct WalletSecurityUiServices {
    get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase>,
    initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase>,
    unlock_wallet: Arc<dyn UnlockWalletUseCase>,
    lock_wallet: Arc<dyn LockWalletUseCase>,
}

impl WalletSecurityUiServices {
    #[must_use]
    pub const fn new(
        get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase>,
        initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase>,
        unlock_wallet: Arc<dyn UnlockWalletUseCase>,
        lock_wallet: Arc<dyn LockWalletUseCase>,
    ) -> Self {
        Self {
            get_wallet_security_status,
            initialize_wallet_security,
            unlock_wallet,
            lock_wallet,
        }
    }
}

/// Midnight account use cases consumed by the Assets page.
pub struct WalletAccountUiServices {
    list_wallet_networks: Arc<dyn ListWalletNetworksUseCase>,
    select_wallet_network: Arc<dyn SelectWalletNetworkUseCase>,
    derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase>,
    get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
    sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
}

impl WalletAccountUiServices {
    #[must_use]
    pub const fn new(
        list_wallet_networks: Arc<dyn ListWalletNetworksUseCase>,
        select_wallet_network: Arc<dyn SelectWalletNetworkUseCase>,
        derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase>,
        get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
        sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
    ) -> Self {
        Self {
            list_wallet_networks,
            select_wallet_network,
            derive_wallet_account,
            get_wallet_account,
            sync_wallet_account,
        }
    }
}

/// Transaction use cases consumed by the Assets page.
pub struct WalletTransactionUiServices {
    prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
    authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
    submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
    get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
}

impl WalletTransactionUiServices {
    #[must_use]
    pub const fn new(
        prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
        authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
        submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
        get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
    ) -> Self {
        Self {
            prepare_wallet_transfer,
            authorize_wallet_transfer,
            submit_wallet_transfer,
            get_wallet_transfer_draft,
        }
    }
}

impl WalletUiServices {
    #[must_use]
    pub fn new(
        profiles: WalletProfileUiServices,
        security: WalletSecurityUiServices,
        account: WalletAccountUiServices,
        transactions: WalletTransactionUiServices,
    ) -> Self {
        Self {
            create_wallet_profile: profiles.create_wallet_profile,
            list_wallet_profiles: profiles.list_wallet_profiles,
            select_wallet_profile: profiles.select_wallet_profile,
            get_active_wallet_profile: profiles.get_active_wallet_profile,
            get_wallet_security_status: security.get_wallet_security_status,
            initialize_wallet_security: security.initialize_wallet_security,
            unlock_wallet: security.unlock_wallet,
            lock_wallet: security.lock_wallet,
            list_wallet_networks: account.list_wallet_networks,
            select_wallet_network: account.select_wallet_network,
            derive_wallet_account: account.derive_wallet_account,
            get_wallet_account: account.get_wallet_account,
            sync_wallet_account: account.sync_wallet_account,
            prepare_wallet_transfer: transactions.prepare_wallet_transfer,
            authorize_wallet_transfer: transactions.authorize_wallet_transfer,
            submit_wallet_transfer: transactions.submit_wallet_transfer,
            get_wallet_transfer_draft: transactions.get_wallet_transfer_draft,
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

    #[must_use]
    pub fn get_wallet_security_status(&self) -> Arc<dyn GetWalletSecurityStatusUseCase> {
        Arc::clone(&self.get_wallet_security_status)
    }

    #[must_use]
    pub fn initialize_wallet_security(&self) -> Arc<dyn InitializeWalletSecurityUseCase> {
        Arc::clone(&self.initialize_wallet_security)
    }

    #[must_use]
    pub fn unlock_wallet(&self) -> Arc<dyn UnlockWalletUseCase> {
        Arc::clone(&self.unlock_wallet)
    }

    #[must_use]
    pub fn lock_wallet(&self) -> Arc<dyn LockWalletUseCase> {
        Arc::clone(&self.lock_wallet)
    }

    #[must_use]
    pub fn list_wallet_networks(&self) -> Arc<dyn ListWalletNetworksUseCase> {
        Arc::clone(&self.list_wallet_networks)
    }

    #[must_use]
    pub fn select_wallet_network(&self) -> Arc<dyn SelectWalletNetworkUseCase> {
        Arc::clone(&self.select_wallet_network)
    }

    #[must_use]
    pub fn derive_wallet_account(&self) -> Arc<dyn DeriveWalletAccountUseCase> {
        Arc::clone(&self.derive_wallet_account)
    }

    #[must_use]
    pub fn get_wallet_account(&self) -> Arc<dyn GetWalletAccountUseCase> {
        Arc::clone(&self.get_wallet_account)
    }

    #[must_use]
    pub fn sync_wallet_account(&self) -> Arc<dyn SyncWalletAccountUseCase> {
        Arc::clone(&self.sync_wallet_account)
    }

    #[must_use]
    pub fn prepare_wallet_transfer(&self) -> Arc<dyn PrepareWalletTransferUseCase> {
        Arc::clone(&self.prepare_wallet_transfer)
    }

    #[must_use]
    pub fn authorize_wallet_transfer(&self) -> Arc<dyn AuthorizeWalletTransferUseCase> {
        Arc::clone(&self.authorize_wallet_transfer)
    }

    #[must_use]
    pub fn submit_wallet_transfer(&self) -> Arc<dyn SubmitWalletTransferUseCase> {
        Arc::clone(&self.submit_wallet_transfer)
    }

    #[must_use]
    pub fn get_wallet_transfer_draft(&self) -> Arc<dyn GetWalletTransferDraftUseCase> {
        Arc::clone(&self.get_wallet_transfer_draft)
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum SecurityCapabilityState {
    Loading,
    Ready(WalletSecurityStatusView),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AccountPageState {
    Loading,
    Ready {
        networks: WalletNetworkListView,
        account: Box<WalletAccountView>,
        security: WalletSecurityStatusView,
        busy: Option<AccountOperation>,
    },
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountOperation {
    Initializing,
    Unlocking,
    Deriving,
    Syncing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TransferPanelState {
    Editing,
    Prepared(Box<WalletTransferPreviewView>),
    Authorized(Box<WalletTransferPreviewView>),
    Submitting(Box<WalletTransferPreviewView>),
    Submitted(Box<WalletTransferSubmissionView>),
    Failed {
        message: String,
        retained: Option<Box<WalletTransferPreviewView>>,
        recovery: TransferRecovery,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferRecovery {
    Edit,
    RetryAuthorized,
    ReconcileUnknown,
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
                    Destination::Diagnostics => rsx! { DiagnosticsPage { active_profile: active_profile.clone() } },
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
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| AccountPageState::Loading);
    let profile_id = active_profile.id.clone();
    let services_for_load = services.clone();
    use_effect(move || {
        state.set(load_account_page(&services_for_load, &profile_id));
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
                    onclick: move |_| state.set(load_account_page(&services, &active_profile.id)),
                    "Retry"
                }
            }
        },
        AccountPageState::Ready {
            networks,
            account,
            security,
            busy,
        } => {
            let night = balance_for(&account, "NIGHT")
                .map(|balance| format_atomic_units(&balance.atomic_units, balance.decimals))
                .unwrap_or_else(|| "—".to_owned());
            let dust = balance_for(&account, "DUST")
                .map(|balance| format_atomic_units(&balance.atomic_units, balance.decimals))
                .unwrap_or_else(|| "—".to_owned());
            let unavailable = account.source == "unavailable";
            let is_busy = busy.is_some();
            let account_hint = account_hint(&account, busy);
            let source_label = account_source_label(&account.source);
            let protected_account = has_protected_account(&account);
            let protection_available = security.is_available();
            let protection_unlocked = security.state_name() == "Unlocked";
            let sync_label = if busy == Some(AccountOperation::Syncing) {
                "Syncing Midnight account…"
            } else if unavailable {
                "Midnight account unavailable"
            } else if account.sync.state == "synced" {
                "Resync Midnight account"
            } else {
                "Connect Midnight account"
            };
            let selected_network_id = networks.selected_network_id.clone();
            let select_services = services.clone();
            let select_profile_id = active_profile.id.clone();
            let mut select_state = state;
            let sync_services = services.clone();
            let sync_profile_id = active_profile.id.clone();
            let sync_networks = networks.clone();
            let sync_account = account.clone();
            let sync_security = security;
            let mut sync_state = state;
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
                        h1 { "{night}" }
                        span { "NIGHT" }
                    }
                    div { class: "dust-pill",
                        strong { "{dust}" }
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
                                "{sync_status_label(&account.sync.state)} · block {height} · {source_label} source"
                            } else {
                                "{sync_status_label(&account.sync.state)} · {source_label} source"
                            }
                        }
                    }
                }

                label { class: "network-field",
                    span { "Midnight network" }
                    select {
                        value: "{selected_network_id}",
                        disabled: is_busy,
                        onchange: move |event| {
                            let network_id = event.value();
                            let result = select_services
                                .select_wallet_network()
                                .execute(SelectWalletNetworkCommand {
                                    profile_id: select_profile_id.clone(),
                                    network_id,
                                })
                                .and_then(|selected| {
                                    select_services
                                        .get_wallet_account()
                                        .execute(WalletAccountQuery {
                                            profile_id: select_profile_id.clone(),
                                        })
                                        .map(|account| (selected, account))
                                });
                            match result {
                                Ok((networks, account)) => select_state.set(AccountPageState::Ready {
                                    networks,
                                    account: Box::new(account),
                                    security,
                                    busy: None,
                                }),
                                Err(error) => select_state.set(AccountPageState::Failed(error.to_string())),
                            }
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
                }

                if protection_available && (!protection_unlocked || !protected_account) {
                    article { class: "surface-card development-card",
                        p { class: "card-eyebrow", "Standalone development" }
                        h2 {
                            if security.state_name() == "Uninitialized" {
                                "Activate protected test account"
                            } else if security.state_name() == "Locked" {
                                "Unlock protected test account"
                            } else {
                                "Derive protected NIGHT account"
                            }
                        }
                        p { "This opt-in simulator/emulator mode uses process-local development custody. It is not durable production key protection." }
                        button {
                            class: "primary-action",
                            r#type: "button",
                            disabled: is_busy,
                            aria_label: "Activate protected Midnight account",
                            onclick: move |_| {
                                activate_state.set(AccountPageState::Ready {
                                    networks: activate_networks.clone(),
                                    account: activate_account.clone(),
                                    security,
                                    busy: Some(account_activation_operation(security)),
                                });
                                match activate_protected_account(
                                    &activate_services,
                                    &activate_profile_id,
                                    security,
                                ) {
                                    Ok(updated_security) => {
                                        let service = activate_services.sync_wallet_account();
                                        let profile_id = activate_profile_id.clone();
                                        let networks = activate_networks.clone();
                                        activate_state.set(AccountPageState::Ready {
                                            networks: networks.clone(),
                                            account: activate_account.clone(),
                                            security: updated_security,
                                            busy: Some(AccountOperation::Syncing),
                                        });
                                        spawn(async move {
                                            match service.execute(WalletAccountQuery { profile_id }).await {
                                                Ok(account) => activate_state.set(AccountPageState::Ready {
                                                    networks,
                                                    account: Box::new(account),
                                                    security: updated_security,
                                                    busy: None,
                                                }),
                                                Err(error) => activate_state.set(AccountPageState::Failed(error.to_string())),
                                            }
                                        });
                                    }
                                    Err(error) => activate_state.set(AccountPageState::Failed(error)),
                                }
                            },
                            if is_busy { "Activating…" } else { "Activate development wallet" }
                        }
                    }
                }

                button {
                    class: if protected_account { "secondary-action account-sync-action" } else { "primary-action" },
                    r#type: "button",
                    disabled: is_busy || unavailable,
                    onclick: move |_| {
                        sync_state.set(AccountPageState::Ready {
                            networks: sync_networks.clone(),
                            account: sync_account.clone(),
                            security: sync_security,
                            busy: Some(AccountOperation::Syncing),
                        });
                        let service = sync_services.sync_wallet_account();
                        let profile_id = sync_profile_id.clone();
                        let networks = sync_networks.clone();
                        spawn(async move {
                            match service.execute(WalletAccountQuery { profile_id }).await {
                                Ok(account) => sync_state.set(AccountPageState::Ready {
                                    networks,
                                    account: Box::new(account),
                                    security: sync_security,
                                    busy: None,
                                }),
                                Err(error) => sync_state.set(AccountPageState::Failed(error.to_string())),
                            }
                        });
                    },
                    "{sync_label}"
                }

                div { class: "dashboard-grid",
                    article { class: "surface-card",
                        p { class: "card-eyebrow", "Receive" }
                        if account.addresses.is_empty() {
                            h2 { "Address unavailable" }
                            p { "Protected Midnight account derivation is not connected in this composition." }
                        } else {
                            for address in account.addresses.iter() {
                                ReceiveAddress {
                                    key: "{address.kind}",
                                    kind: address.kind.clone(),
                                    value: address.value.clone(),
                                }
                            }
                            p { "Each QR encodes exactly the public address shown. Native copy/share remains a platform-adapter follow-up." }
                        }
                    }
                    article { class: "surface-card",
                        p { class: "card-eyebrow", "Activity" }
                        if account.transactions.is_empty() {
                            h2 { "No synced history" }
                            p { if unavailable { "A live Midnight account source is not connected." } else { "Connect the account to synchronize transaction history." } }
                        } else {
                            div { class: "activity-list",
                                for transaction in account.transactions.iter() {
                                    div { class: "activity-row", key: "{transaction.transaction_id}",
                                        span { class: "activity-row__mark", aria_hidden: "true", "{transaction_mark(&transaction.direction)}" }
                                        div {
                                            strong { "{transaction_direction_label(&transaction.direction)}" }
                                            small { "{transaction_status_line(transaction)}" }
                                        }
                                        code { "{truncate_middle(&transaction.transaction_id, 12, 6)}" }
                                    }
                                }
                            }
                        }
                    }
                }

                if protected_account && protection_unlocked && account.sync.state == "synced" {
                    SendTransferPanel {
                        profile_id: active_profile.id.clone(),
                        receive_address: account.addresses[0].value.clone(),
                    }
                }
            }
        }
    }
}

fn load_account_page(services: &WalletUiServices, profile_id: &str) -> AccountPageState {
    let query = WalletAccountQuery {
        profile_id: profile_id.to_owned(),
    };
    let networks = match services.list_wallet_networks().execute(query.clone()) {
        Ok(networks) => networks,
        Err(error) => return AccountPageState::Failed(error.to_string()),
    };
    let account = match services.get_wallet_account().execute(query) {
        Ok(account) => account,
        Err(error) => return AccountPageState::Failed(error.to_string()),
    };
    let security =
        match services
            .get_wallet_security_status()
            .execute(WalletProfileSecurityCommand {
                profile_id: profile_id.to_owned(),
            }) {
            Ok(security) => security,
            Err(error) => return AccountPageState::Failed(error.to_string()),
        };
    AccountPageState::Ready {
        networks,
        account: Box::new(account),
        security,
        busy: None,
    }
}

fn activate_protected_account(
    services: &WalletUiServices,
    profile_id: &str,
    current: WalletSecurityStatusView,
) -> Result<WalletSecurityStatusView, String> {
    let command = || WalletProfileSecurityCommand {
        profile_id: profile_id.to_owned(),
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
            profile_id: profile_id.to_owned(),
            account_index: 0,
            address_index: 0,
        })
        .map_err(|error| error.to_string())?;
    Ok(security)
}

fn account_activation_operation(status: WalletSecurityStatusView) -> AccountOperation {
    match status.state_name() {
        "Uninitialized" => AccountOperation::Initializing,
        "Locked" => AccountOperation::Unlocking,
        _ => AccountOperation::Deriving,
    }
}

fn has_protected_account(account: &WalletAccountView) -> bool {
    account
        .account_id
        .as_deref()
        .is_some_and(|account_id| account_id.starts_with("midnight_account_"))
        && account.addresses.len() == 1
        && account.addresses[0].kind == "unshielded"
}

#[component]
fn ReceiveAddress(kind: String, value: String) -> Element {
    let mut qr_open = use_signal(|| false);
    let qr = render_qr_svg(&value);
    rsx! {
        div { class: "address-row",
            div {
                strong { "{address_kind_label(&kind)}" }
                small { "{address_purpose(&kind)}" }
            }
            code { title: "{value}", "{truncate_middle(&value, 18, 8)}" }
            button {
                class: "address-qr-toggle",
                r#type: "button",
                aria_label: if *qr_open.read() { "Hide receive QR" } else { "Show receive QR" },
                aria_expanded: if *qr_open.read() { "true" } else { "false" },
                onclick: move |_| {
                    let next = !*qr_open.read();
                    qr_open.set(next);
                },
                if *qr_open.read() { "Hide QR" } else { "Show QR" }
            }
        }
        if *qr_open.read() {
            div { class: "address-qr", role: "img", aria_label: "QR code for {address_kind_label(&kind)} receive address",
                if let Some(svg) = qr {
                    div { class: "address-qr__frame", dangerous_inner_html: "{svg}" }
                    p { "Scan to receive at the public address shown above." }
                } else {
                    p { role: "alert", "This address could not be encoded as a QR code." }
                }
            }
        }
    }
}

#[component]
fn SendTransferPanel(profile_id: String, receive_address: String) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut panel = use_signal(|| TransferPanelState::Editing);
    let mut recipient = use_signal(String::new);
    let mut amount = use_signal(String::new);

    match panel.read().clone() {
        TransferPanelState::Editing => {
            let can_review =
                !recipient.read().trim().is_empty() && !amount.read().trim().is_empty();
            rsx! {
                article { class: "surface-card transfer-card",
                    p { class: "card-eyebrow", "Send" }
                    h2 { "Send unshielded NIGHT" }
                    p { "The recipient and exact amount are validated before an explicit review and authorization step." }
                    label { r#for: "transfer-recipient", "Recipient address" }
                    input {
                        id: "transfer-recipient",
                        r#type: "text",
                        aria_label: "Recipient address",
                        maxlength: 512,
                        autocomplete: "off",
                        value: "{recipient}",
                        oninput: move |event| recipient.set(event.value()),
                    }
                    button {
                        class: "inline-action",
                        r#type: "button",
                        onclick: move |_| recipient.set(receive_address.clone()),
                        "Use my receive address"
                    }
                    label { r#for: "transfer-amount", "Amount (NIGHT)" }
                    input {
                        id: "transfer-amount",
                        r#type: "text",
                        aria_label: "Amount in NIGHT",
                        inputmode: "decimal",
                        maxlength: 48,
                        autocomplete: "off",
                        placeholder: "1.5",
                        value: "{amount}",
                        oninput: move |event| amount.set(event.value()),
                    }
                    button {
                        class: "primary-action",
                        r#type: "button",
                        disabled: !can_review,
                        onclick: move |_| {
                            match night_display_to_atomic_units(&amount.read()) {
                                Ok(amount_atomic_units) => {
                                    match services.prepare_wallet_transfer().execute(
                                        PrepareWalletTransferCommand {
                                            profile_id: profile_id.clone(),
                                            recipient_address: recipient.read().trim().to_owned(),
                                            amount_atomic_units,
                                        },
                                    ) {
                                        Ok(preview) => panel.set(TransferPanelState::Prepared(Box::new(preview))),
                                        Err(error) => panel.set(TransferPanelState::Failed {
                                            message: error.to_string(),
                                            retained: None,
                                            recovery: TransferRecovery::Edit,
                                        }),
                                    }
                                }
                                Err(error) => panel.set(TransferPanelState::Failed {
                                    message: error.to_owned(),
                                    retained: None,
                                    recovery: TransferRecovery::Edit,
                                }),
                            }
                        },
                        "Review transfer"
                    }
                }
            }
        }
        TransferPanelState::Prepared(preview) => {
            let amount_label = format_transfer_asset(&preview.amount);
            let change_label = format_transfer_asset(&preview.change);
            let recipient_label = truncate_middle(&preview.recipient_address, 18, 8);
            let confirmation = authorize_transfer_confirmation(&preview);
            let draft_id = preview.draft_id.clone();
            let challenge = preview.authorization_challenge.clone();
            rsx! {
                article { class: "surface-card transfer-card review-card", aria_label: "Review NIGHT transfer" ,
                    p { class: "card-eyebrow", "Review" }
                    h2 { "Confirm transfer details" }
                    dl { class: "preview-list",
                        div { dt { "Send" } dd { "{amount_label}" } }
                        div { dt { "Recipient" } dd { title: "{preview.recipient_address}", "{recipient_label}" } }
                        div { dt { "Network" } dd { "{preview.network_id}" } }
                        div { dt { "Change" } dd { "{change_label}" } }
                        div { dt { "Inputs" } dd { "{preview.input_count}" } }
                        div { dt { "DUST fee" } dd { "Calculated during proving" } }
                    }
                    p { class: "consent-copy", "Authorizing signs only this reviewed transfer. Proving and submission remain a separate action." }
                    div { class: "transfer-actions",
                        button {
                            class: "secondary-action",
                            r#type: "button",
                            onclick: move |_| panel.set(TransferPanelState::Editing),
                            "Edit"
                        }
                        button {
                            class: "primary-action",
                            r#type: "button",
                            aria_label: "Authorize reviewed NIGHT transfer",
                            onclick: move |_| {
                                match services.authorize_wallet_transfer().execute(
                                    AuthorizeWalletTransferCommand {
                                        profile_id: profile_id.clone(),
                                        draft_id: draft_id.clone(),
                                        authorization_challenge: challenge.clone(),
                                        confirmation: confirmation.clone(),
                                    },
                                ) {
                                    Ok(authorized) => panel.set(TransferPanelState::Authorized(Box::new(authorized))),
                                    Err(error) => panel.set(TransferPanelState::Failed {
                                        message: error.to_string(),
                                        retained: Some(preview.clone()),
                                        recovery: TransferRecovery::Edit,
                                    }),
                                }
                            },
                            "Authorize transfer"
                        }
                    }
                }
            }
        }
        TransferPanelState::Authorized(preview) => {
            let amount_label = format_transfer_asset(&preview.amount);
            let confirmation = submit_transfer_confirmation(&preview);
            let draft_id = preview.draft_id.clone();
            let submitting_preview = preview.clone();
            rsx! {
                article { class: "surface-card transfer-card review-card", aria_label: "Authorized NIGHT transfer",
                    p { class: "card-eyebrow", "Authorized" }
                    h2 { "{amount_label} is ready" }
                    p { "The protected signature is retained inside the Midnight adapter. Continue to prove, balance the DUST fee, and submit." }
                    button {
                        class: "primary-action",
                        r#type: "button",
                        aria_label: "Prove and submit NIGHT transfer",
                        onclick: move |_| {
                            panel.set(TransferPanelState::Submitting(submitting_preview.clone()));
                            let service = services.submit_wallet_transfer();
                            let drafts = services.get_wallet_transfer_draft();
                            let profile_id = profile_id.clone();
                            let draft_id = draft_id.clone();
                            let confirmation = confirmation.clone();
                            spawn(async move {
                                match service.execute(SubmitWalletTransferCommand {
                                    profile_id: profile_id.clone(),
                                    draft_id: draft_id.clone(),
                                    confirmation,
                                }).await {
                                    Ok(submitted) => panel.set(TransferPanelState::Submitted(Box::new(submitted))),
                                    Err(error) => {
                                        let retained = drafts.execute(WalletTransferDraftQuery {
                                            profile_id,
                                            draft_id,
                                        }).ok().map(Box::new);
                                        let recovery = post_submission_recovery(
                                            retained.as_deref().map(|preview| preview.state.as_str()),
                                        );
                                        panel.set(TransferPanelState::Failed {
                                            message: error.to_string(),
                                            retained,
                                            recovery,
                                        });
                                    }
                                }
                            });
                        },
                        "Prove and submit"
                    }
                }
            }
        }
        TransferPanelState::Submitting(preview) => rsx! {
            article { class: "surface-card transfer-card submitting-card", role: "status", aria_live: "polite", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                div {
                    p { class: "card-eyebrow", "Submitting" }
                    h2 { "Proving {format_transfer_asset(&preview.amount)}" }
                    p { "The worker is balancing the DUST fee, proving locally, and waiting for a public inclusion result." }
                }
            }
        },
        TransferPanelState::Submitted(submission) => rsx! {
            article { class: "surface-card transfer-card submitted-card", role: "status", aria_live: "polite",
                p { class: "card-eyebrow", "Included" }
                h2 { "Transfer submitted" }
                p { "Mode: {submission.mode}. Final DUST fee: {format_transfer_asset(&submission.fee)}." }
                dl { class: "preview-list",
                    div { dt { "Transaction" } dd { title: "{submission.transaction_id}", "{truncate_middle(&submission.transaction_id, 16, 8)}" } }
                    div { dt { "Block" } dd { title: "{submission.block_id}", "{truncate_middle(&submission.block_id, 16, 8)}" } }
                }
                button {
                    class: "secondary-action",
                    r#type: "button",
                    onclick: move |_| {
                        recipient.set(String::new());
                        amount.set(String::new());
                        panel.set(TransferPanelState::Editing);
                    },
                    "Send another"
                }
            }
        },
        TransferPanelState::Failed {
            message,
            retained,
            recovery,
        } => {
            let retryable = recovery == TransferRecovery::RetryAuthorized;
            let outcome_unknown = recovery == TransferRecovery::ReconcileUnknown;
            let retry_preview = retained.clone();
            rsx! {
            article { class: "surface-card transfer-card", role: "alert",
                p { class: "card-eyebrow", "Transfer not completed" }
                h2 {
                    if outcome_unknown {
                        "Submission outcome needs reconciliation"
                    } else if retryable {
                        "Authorized transfer can be retried safely"
                    } else {
                        "Check the transfer and try again"
                    }
                }
                p { "{message}" }
                if outcome_unknown {
                    p { "Oxid will not create or submit a replacement while broadcast may have occurred." }
                } else if retryable {
                    button {
                        class: "secondary-action",
                        r#type: "button",
                        onclick: move |_| {
                            if let Some(preview) = retry_preview.clone() {
                                panel.set(TransferPanelState::Authorized(preview));
                            }
                        },
                        "Retry safe submission"
                    }
                } else {
                    button {
                        class: "secondary-action",
                        r#type: "button",
                        onclick: move |_| panel.set(TransferPanelState::Editing),
                        "Back to transfer"
                    }
                }
            }
            }
        }
    }
}

fn post_submission_recovery(retained_state: Option<&str>) -> TransferRecovery {
    match retained_state {
        Some("authorized") => TransferRecovery::RetryAuthorized,
        _ => TransferRecovery::ReconcileUnknown,
    }
}

fn render_qr_svg(value: &str) -> Option<String> {
    use qrcode::{QrCode, render::svg};

    QrCode::new(value.as_bytes()).ok().map(|code| {
        code.render::<svg::Color<'_>>()
            .min_dimensions(220, 220)
            .max_dimensions(280, 280)
            .quiet_zone(true)
            .dark_color(svg::Color("#07111f"))
            .light_color(svg::Color("#ffffff"))
            .build()
    })
}

fn night_display_to_atomic_units(value: &str) -> Result<String, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Err("enter a NIGHT amount");
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|part| !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("NIGHT amount must be a positive decimal number");
    }
    let fraction = fraction.unwrap_or_default();
    if fraction.len() > 6 {
        return Err("NIGHT supports at most 6 decimal places");
    }
    let padded_fraction = format!("{fraction:0<6}");
    let atomic = format!("{whole}{padded_fraction}")
        .parse::<u128>()
        .map_err(|_| "NIGHT amount is too large")?;
    if atomic == 0 {
        return Err("NIGHT amount must be greater than zero");
    }
    Ok(atomic.to_string())
}

fn format_transfer_asset(asset: &oxid_wallet_application::WalletTransferAssetView) -> String {
    format!(
        "{} {}",
        format_atomic_units(&asset.atomic_units, asset.decimals),
        asset.symbol
    )
}

fn authorize_transfer_confirmation(
    preview: &WalletTransferPreviewView,
) -> SensitiveOperationConfirmation {
    SensitiveOperationConfirmation {
        title: "Authorize NIGHT transfer".to_owned(),
        summary: format!(
            "Send {} to {} on {}; DUST fee balancing and proving remain pending",
            format_transfer_asset(&preview.amount),
            truncate_middle(&preview.recipient_address, 18, 8),
            preview.network_id,
        ),
        confirmed: true,
    }
}

fn submit_transfer_confirmation(
    preview: &WalletTransferPreviewView,
) -> SensitiveOperationConfirmation {
    SensitiveOperationConfirmation {
        title: "Prove and submit NIGHT transfer".to_owned(),
        summary: format!(
            "Prove and submit {} to {} on {}",
            format_transfer_asset(&preview.amount),
            truncate_middle(&preview.recipient_address, 18, 8),
            preview.network_id,
        ),
        confirmed: true,
    }
}

fn balance_for<'a>(
    account: &'a WalletAccountView,
    symbol: &str,
) -> Option<&'a oxid_wallet_application::WalletAssetBalanceView> {
    account
        .balances
        .iter()
        .find(|balance| balance.symbol == symbol)
}

fn format_atomic_units(atomic_units: &str, decimals: u8) -> String {
    if atomic_units.is_empty() || !atomic_units.bytes().all(|byte| byte.is_ascii_digit()) {
        return "—".to_owned();
    }
    let atomic_units = atomic_units.trim_start_matches('0');
    let atomic_units = if atomic_units.is_empty() {
        "0"
    } else {
        atomic_units
    };
    if decimals == 0 {
        return atomic_units.to_owned();
    }
    let decimals = usize::from(decimals);
    let padded = if atomic_units.len() <= decimals {
        format!(
            "{}{}",
            "0".repeat(decimals + 1 - atomic_units.len()),
            atomic_units
        )
    } else {
        atomic_units.to_owned()
    };
    let split = padded.len() - decimals;
    let whole = &padded[..split];
    let fraction = padded[split..].trim_end_matches('0');
    if fraction.is_empty() {
        whole.to_owned()
    } else {
        format!("{whole}.{fraction}")
    }
}

fn account_hint(account: &WalletAccountView, busy: Option<AccountOperation>) -> &'static str {
    if let Some(operation) = busy {
        match operation {
            AccountOperation::Initializing => "Initializing development wallet protection…",
            AccountOperation::Unlocking => "Unlocking the protected wallet session…",
            AccountOperation::Deriving => "Deriving the public Midnight account…",
            AccountOperation::Syncing => "Synchronizing account state from the configured source…",
        }
    } else {
        match account.source.as_str() {
            "unavailable" => {
                "Native custody and a live Midnight account source are not connected yet."
            }
            "simulated" => "Development-only public fixture state; no chain was contacted.",
            "cached" => "Showing local state from the most recent successful synchronization.",
            _ => "Live account state reported by the configured Midnight adapter.",
        }
    }
}

fn account_source_label(source: &str) -> &'static str {
    match source {
        "live" => "Live",
        "cached" => "Cached",
        "simulated" => "Simulated",
        _ => "Not connected",
    }
}

fn sync_status_label(state: &str) -> &'static str {
    match state {
        "never_synced" => "Not synced",
        "syncing" => "Syncing",
        "synced" => "Synced",
        "stalled" => "Stalled",
        _ => "Unavailable",
    }
}

fn address_kind_label(kind: &str) -> &'static str {
    match kind {
        "unshielded" => "Unshielded",
        "shielded" => "Shielded",
        "dust" => "DUST",
        _ => "Reward",
    }
}

fn address_purpose(kind: &str) -> &'static str {
    match kind {
        "unshielded" => "Send public NIGHT here",
        "shielded" => "Private NIGHT receive",
        "dust" => "Fee-token account",
        _ => "Reward address",
    }
}

fn truncate_middle(value: &str, head: usize, tail: usize) -> String {
    let length = value.chars().count();
    if length <= head + tail + 1 {
        return value.to_owned();
    }
    let prefix = value.chars().take(head).collect::<String>();
    let suffix = value.chars().skip(length - tail).collect::<String>();
    format!("{prefix}…{suffix}")
}

fn transaction_mark(direction: &str) -> &'static str {
    match direction {
        "incoming" => "↓",
        "outgoing" => "↑",
        "self_transfer" => "↔",
        _ => "◇",
    }
}

fn transaction_direction_label(direction: &str) -> &'static str {
    match direction {
        "incoming" => "Received",
        "outgoing" => "Sent",
        "self_transfer" => "Self transfer",
        _ => "Transaction",
    }
}

fn transaction_status_line(transaction: &oxid_wallet_application::WalletTransactionView) -> String {
    let block = transaction
        .block_height
        .map_or_else(|| "—".to_owned(), |height| height.to_string());
    format!("{} · block {block}", transaction.status)
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
fn DiagnosticsPage(active_profile: WalletProfileView) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut account_state = use_signal(|| AccountPageState::Loading);
    let profile_id = active_profile.id.clone();
    use_effect(move || account_state.set(load_account_page(&services, &profile_id)));

    let (protection_state, protection_ready, midnight_state, midnight_ready, completion_state) =
        match account_state.read().clone() {
            AccountPageState::Loading => (
                "Loading".to_owned(),
                false,
                "Loading".to_owned(),
                false,
                "Loading".to_owned(),
            ),
            AccountPageState::Failed(_) => (
                "Status unavailable".to_owned(),
                false,
                "Status unavailable".to_owned(),
                false,
                "Status unavailable".to_owned(),
            ),
            AccountPageState::Ready {
                account, security, ..
            } => {
                let protection_ready = security.is_available();
                let midnight_ready = account.source != "unavailable";
                (
                    format!("{} · {}", security.state_name(), security.protection_name()),
                    protection_ready,
                    format!(
                        "{} · {}",
                        account_source_label(&account.source),
                        sync_status_label(&account.sync.state)
                    ),
                    midnight_ready,
                    if account.source == "simulated" {
                        "Deterministic simulation".to_owned()
                    } else {
                        "Not connected".to_owned()
                    },
                )
            }
        };
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Capability status" }
            h1 { "Diagnostics" }
            p { "This view reports only capabilities that are actually composed into the current application." }
        }
        div { class: "diagnostic-grid",
            CapabilityStatus { name: "Profile lifecycle", state: "Create · list · select · restore".to_owned(), ready: true }
            CapabilityStatus { name: "Profile metadata store", state: "Persistent · public metadata only".to_owned(), ready: true }
            CapabilityStatus { name: "Protected secret store", state: protection_state, ready: protection_ready }
            CapabilityStatus { name: "Midnight account", state: midnight_state, ready: midnight_ready }
            CapabilityStatus { name: "Transaction completion", state: completion_state, ready: midnight_ready }
            CapabilityStatus { name: "Local proof provider", state: "Not connected".to_owned(), ready: false }
            CapabilityStatus { name: "DID adapter", state: "Not connected".to_owned(), ready: false }
            CapabilityStatus { name: "Credential protocols", state: "Not connected".to_owned(), ready: false }
        }
    }
}

#[component]
fn CapabilityStatus(name: &'static str, state: String, ready: bool) -> Element {
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
    let services = consume_context::<WalletUiServices>();
    let mut security = use_signal(|| SecurityCapabilityState::Loading);
    let profile_id = active_profile.id.clone();
    let services_for_load = services.clone();
    use_effect(move || {
        security.set(
            services_for_load
                .get_wallet_security_status()
                .execute(WalletProfileSecurityCommand {
                    profile_id: profile_id.clone(),
                })
                .map_or_else(
                    |error| SecurityCapabilityState::Failed(error.to_string()),
                    SecurityCapabilityState::Ready,
                ),
        );
    });
    let security_card = match security.read().clone() {
        SecurityCapabilityState::Loading => rsx! {
            article { class: "settings-card surface-card", role: "status", aria_busy: "true",
                div {
                    p { class: "card-eyebrow", "Wallet protection" }
                    h2 { "Checking custody capability" }
                    p { "Reading the effective protection class from the composed adapter." }
                }
                span { class: "status-pill", "Loading" }
            }
        },
        SecurityCapabilityState::Ready(status) => {
            let available = status.is_available();
            let state = status.state_name();
            let protection = status.protection_name();
            let profile_id = active_profile.id.clone();
            let security_services = services.clone();
            let mut security_state = security;
            rsx! {
                article { class: "settings-card surface-card",
                    div {
                        p { class: "card-eyebrow", "Wallet protection" }
                        h2 { "{state} · {protection}" }
                        p {
                            if available {
                                "This reports the effective adapter capability. Development-only protection is never a production custody claim."
                            } else {
                                "Production composition fails closed until a native Keychain or Keystore adapter is connected. Public profile metadata remains available."
                            }
                        }
                    }
                    span {
                        class: if available { "status-pill success" } else { "status-pill" },
                        if available { "Available" } else { "Fail closed" }
                    }
                    if available {
                        button {
                            class: "secondary-action",
                            r#type: "button",
                            aria_label: "{security_action_label(status)}",
                            onclick: move |_| {
                                let command = WalletProfileSecurityCommand {
                                    profile_id: profile_id.clone(),
                                };
                                let result = match status.state_name() {
                                    "Uninitialized" => security_services
                                        .initialize_wallet_security()
                                        .execute(command),
                                    "Locked" => security_services.unlock_wallet().execute(command),
                                    "Unlocked" => security_services.lock_wallet().execute(command),
                                    _ => return,
                                };
                                security_state.set(result.map_or_else(
                                    |error| SecurityCapabilityState::Failed(error.to_string()),
                                    SecurityCapabilityState::Ready,
                                ));
                            },
                            "{security_action_label(status)}"
                        }
                    }
                }
            }
        }
        SecurityCapabilityState::Failed(message) => rsx! {
            article { class: "settings-card surface-card", role: "alert",
                div {
                    p { class: "card-eyebrow", "Wallet protection" }
                    h2 { "Status unavailable" }
                    p { "{message}" }
                }
                span { class: "status-pill", "Error" }
            }
        },
    };

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
        {security_card}
        article { class: "settings-card surface-card",
            div {
                p { class: "card-eyebrow", "Privacy" }
                h2 { "Local-first · telemetry off" }
                p { "No analytics or remote-storage adapter is active. Development simulation is local and production chain/identity adapters remain explicit capabilities." }
            }
            span { class: "status-pill success", "Enforced" }
        }
    }
}

fn security_action_label(status: WalletSecurityStatusView) -> &'static str {
    match status.state_name() {
        "Uninitialized" => "Initialize wallet",
        "Locked" => "Unlock wallet",
        "Unlocked" => "Lock wallet",
        _ => "Unavailable",
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

    #[test]
    fn atomic_units_are_rendered_without_floating_point_loss() {
        assert_eq!(format_atomic_units("5000000", 6), "5");
        assert_eq!(format_atomic_units("12000000000000000", 15), "12");
        assert_eq!(format_atomic_units("1", 6), "0.000001");
        assert_eq!(format_atomic_units("000000", 6), "0");
        assert_eq!(format_atomic_units("not-a-number", 6), "—");
    }

    #[test]
    fn night_input_is_converted_to_exact_atomic_units() {
        assert_eq!(night_display_to_atomic_units("1"), Ok("1000000".to_owned()));
        assert_eq!(
            night_display_to_atomic_units("1.5"),
            Ok("1500000".to_owned())
        );
        assert_eq!(
            night_display_to_atomic_units("0.000001"),
            Ok("1".to_owned())
        );
        assert_eq!(
            night_display_to_atomic_units("0"),
            Err("NIGHT amount must be greater than zero")
        );
        assert_eq!(
            night_display_to_atomic_units("1.0000001"),
            Err("NIGHT supports at most 6 decimal places")
        );
        assert!(night_display_to_atomic_units("-1").is_err());
        assert!(night_display_to_atomic_units("1.2.3").is_err());
    }

    #[test]
    fn receive_qr_is_deterministic_and_address_specific() {
        let first = render_qr_svg("mn_addr_undeployed1first").expect("address fits a QR code");
        let repeated = render_qr_svg("mn_addr_undeployed1first").expect("address fits a QR code");
        let second = render_qr_svg("mn_addr_undeployed1second").expect("address fits a QR code");

        assert_eq!(first, repeated);
        assert_ne!(first, second);
        assert!(first.starts_with("<?xml"));
        assert!(first.contains("<svg"));
    }

    #[test]
    fn post_submission_recovery_never_blindly_retries_an_unknown_submission() {
        assert_eq!(
            post_submission_recovery(Some("authorized")),
            TransferRecovery::RetryAuthorized
        );
        assert_eq!(
            post_submission_recovery(Some("submitting")),
            TransferRecovery::ReconcileUnknown
        );
        assert_eq!(
            post_submission_recovery(Some("expired")),
            TransferRecovery::ReconcileUnknown
        );
        assert_eq!(
            post_submission_recovery(None),
            TransferRecovery::ReconcileUnknown
        );
    }

    #[test]
    fn long_public_identifiers_are_shortened_for_mobile_display() {
        assert_eq!(truncate_middle("1234567890", 4, 3), "1234…890");
        assert_eq!(truncate_middle("short", 4, 3), "short");
    }
}
