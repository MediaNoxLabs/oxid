// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use dioxus::prelude::*;
use oxid_credential_application::{CredentialProfileQuery, CredentialView};
use oxid_passport_vault_application::{
    AUTHORIZE_PASSPORT_VAULT_CALL_INTENT, AuthorizePassportVaultCallCommand,
    AuthorizePassportVaultCallUseCase, CLAIM_INTENT, CREATE_LOCK_INTENT,
    CancelPassportVaultCallSubmissionUseCase, ClaimPassportVaultLockCommand,
    ClaimPassportVaultLockUseCase, CreatePassportVaultLockCommand, CreatePassportVaultLockUseCase,
    DEPOSIT_INTENT, DepositPassportVaultLockUseCase, GetPassportVaultCallSubmissionStatusUseCase,
    GetPassportVaultCallUseCase, ListPassportVaultCallSubmissionsUseCase,
    ListPassportVaultLocksUseCase, PassportVaultAmountCommand, PassportVaultCallPreviewView,
    PassportVaultCallQuery, PassportVaultCallSubmissionStatusView, PassportVaultCallSubmissionView,
    PassportVaultLockView, PassportVaultView, PreparePassportVaultCallAction,
    PreparePassportVaultCallCommand, PreparePassportVaultCallUseCase,
    ReadPassportVaultContractStateCommand, ReadPassportVaultContractStateUseCase,
    ReconcilePassportVaultCallSubmissionUseCase, SUBMIT_PASSPORT_VAULT_CALL_INTENT,
    SubmitPassportVaultCallCommand, SubmitPassportVaultCallUseCase, WITHDRAW_INTENT,
    WithdrawPassportVaultLockUseCase,
};
use oxid_wallet_application::WalletProfileView;

use super::labels as ui;
use super::{BrandProfile, WalletUiServices, run_ui_blocking, run_ui_future, truncate_middle};

/// Product-specific Passport Vault capabilities consumed only by the Vault page.
pub struct PassportVaultUiServices {
    pub(super) list: Arc<dyn ListPassportVaultLocksUseCase>,
    pub(super) create: Arc<dyn CreatePassportVaultLockUseCase>,
    pub(super) deposit: Arc<dyn DepositPassportVaultLockUseCase>,
    pub(super) claim: Arc<dyn ClaimPassportVaultLockUseCase>,
    pub(super) withdraw: Arc<dyn WithdrawPassportVaultLockUseCase>,
    pub(super) state_persistence: String,
    pub(super) contract_calls: PassportVaultContractCallUiServices,
}

impl PassportVaultUiServices {
    #[must_use]
    pub fn new(
        list: Arc<dyn ListPassportVaultLocksUseCase>,
        create: Arc<dyn CreatePassportVaultLockUseCase>,
        deposit: Arc<dyn DepositPassportVaultLockUseCase>,
        claim: Arc<dyn ClaimPassportVaultLockUseCase>,
        withdraw: Arc<dyn WithdrawPassportVaultLockUseCase>,
        state_persistence: impl Into<String>,
        contract_calls: PassportVaultContractCallUiServices,
    ) -> Self {
        Self {
            list,
            create,
            deposit,
            claim,
            withdraw,
            state_persistence: state_persistence.into(),
            contract_calls,
        }
    }
}

/// Public recovery operations for a retained or ambiguously submitted vault call.
pub struct PassportVaultContractCallRecoveryUiServices {
    get_draft: Arc<dyn GetPassportVaultCallUseCase>,
    get_status: Arc<dyn GetPassportVaultCallSubmissionStatusUseCase>,
    cancel: Arc<dyn CancelPassportVaultCallSubmissionUseCase>,
    list: Arc<dyn ListPassportVaultCallSubmissionsUseCase>,
    reconcile: Arc<dyn ReconcilePassportVaultCallSubmissionUseCase>,
}

impl PassportVaultContractCallRecoveryUiServices {
    #[must_use]
    pub fn new(
        get_draft: Arc<dyn GetPassportVaultCallUseCase>,
        get_status: Arc<dyn GetPassportVaultCallSubmissionStatusUseCase>,
        cancel: Arc<dyn CancelPassportVaultCallSubmissionUseCase>,
        list: Arc<dyn ListPassportVaultCallSubmissionsUseCase>,
        reconcile: Arc<dyn ReconcilePassportVaultCallSubmissionUseCase>,
    ) -> Self {
        Self {
            get_draft,
            get_status,
            cancel,
            list,
            reconcile,
        }
    }
}

/// Production-shaped Passport Vault call lifecycle exposed to the mobile page.
#[derive(Clone)]
pub struct PassportVaultContractCallUiServices {
    read_state: Arc<dyn ReadPassportVaultContractStateUseCase>,
    prepare: Arc<dyn PreparePassportVaultCallUseCase>,
    authorize: Arc<dyn AuthorizePassportVaultCallUseCase>,
    submit: Arc<dyn SubmitPassportVaultCallUseCase>,
    get_draft: Arc<dyn GetPassportVaultCallUseCase>,
    get_status: Arc<dyn GetPassportVaultCallSubmissionStatusUseCase>,
    cancel: Arc<dyn CancelPassportVaultCallSubmissionUseCase>,
    list: Arc<dyn ListPassportVaultCallSubmissionsUseCase>,
    reconcile: Arc<dyn ReconcilePassportVaultCallSubmissionUseCase>,
    mode: String,
    configured_contract_address_hex: Option<String>,
}

impl PassportVaultContractCallUiServices {
    #[must_use]
    pub fn new(
        read_state: Arc<dyn ReadPassportVaultContractStateUseCase>,
        prepare: Arc<dyn PreparePassportVaultCallUseCase>,
        authorize: Arc<dyn AuthorizePassportVaultCallUseCase>,
        submit: Arc<dyn SubmitPassportVaultCallUseCase>,
        recovery: PassportVaultContractCallRecoveryUiServices,
        mode: impl Into<String>,
        configured_contract_address_hex: Option<String>,
    ) -> Self {
        Self {
            read_state,
            prepare,
            authorize,
            submit,
            get_draft: recovery.get_draft,
            get_status: recovery.get_status,
            cancel: recovery.cancel,
            list: recovery.list,
            reconcile: recovery.reconcile,
            mode: mode.into(),
            configured_contract_address_hex,
        }
    }
}

impl WalletUiServices {
    #[must_use]
    pub fn list_passport_vault_locks(&self) -> Arc<dyn ListPassportVaultLocksUseCase> {
        Arc::clone(&self.list_passport_vault_locks)
    }

    #[must_use]
    pub fn create_passport_vault_lock(&self) -> Arc<dyn CreatePassportVaultLockUseCase> {
        Arc::clone(&self.create_passport_vault_lock)
    }

    #[must_use]
    pub fn deposit_passport_vault_lock(&self) -> Arc<dyn DepositPassportVaultLockUseCase> {
        Arc::clone(&self.deposit_passport_vault_lock)
    }

    #[must_use]
    pub fn claim_passport_vault_lock(&self) -> Arc<dyn ClaimPassportVaultLockUseCase> {
        Arc::clone(&self.claim_passport_vault_lock)
    }

    #[must_use]
    pub fn withdraw_passport_vault_lock(&self) -> Arc<dyn WithdrawPassportVaultLockUseCase> {
        Arc::clone(&self.withdraw_passport_vault_lock)
    }

    #[must_use]
    pub fn passport_vault_contract_calls(&self) -> PassportVaultContractCallUiServices {
        self.passport_vault_contract_calls.clone()
    }

    #[must_use]
    pub fn passport_vault_state_persistence(&self) -> String {
        self.passport_vault_state_persistence.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PassportVaultPageState {
    Loading,
    Ready {
        vault: Box<PassportVaultView>,
        credentials: Vec<CredentialView>,
        busy: bool,
        operation_error: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PassportVaultLocalOperation {
    Invalid(String),
    Deposit {
        lock_id: u64,
        amount: u128,
    },
    Claim {
        lock_id: u64,
        credential_id: String,
        amount: u128,
    },
    Withdraw {
        lock_id: u64,
        amount: u128,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PassportVaultContractPanelState {
    Editing,
    Preparing,
    Prepared(Box<PassportVaultCallPreviewView>),
    Authorizing(Box<PassportVaultCallPreviewView>),
    Authorized(Box<PassportVaultCallPreviewView>),
    Submitting(Box<PassportVaultCallPreviewView>),
    Cancelling(Box<PassportVaultCallPreviewView>),
    Submitted(Box<PassportVaultCallSubmissionView>),
    Resolved(Box<PassportVaultCallSubmissionStatusView>),
    Failed {
        message: String,
        retained: Option<Box<PassportVaultCallPreviewView>>,
        recovery: PassportVaultCallRecovery,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassportVaultCallRecovery {
    Edit,
    RetryAuthorized,
    ReconcileUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PassportVaultContractStatePaneState {
    Idle,
    Loading,
    Ready(Box<PassportVaultView>),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PassportVaultCallRecoveryPaneState {
    Loading,
    Ready {
        latest: Option<Box<PassportVaultCallSubmissionStatusView>>,
        reconciling: bool,
        operation_error: Option<String>,
    },
    Failed(String),
}

fn load_passport_vault_page(
    services: &WalletUiServices,
    profile_id: &str,
    operation_error: Option<String>,
) -> PassportVaultPageState {
    let vault = match services.list_passport_vault_locks().execute() {
        Ok(vault) => vault,
        Err(error) => return PassportVaultPageState::Failed(error.to_string()),
    };
    let credentials = match services.list_credentials().execute(CredentialProfileQuery {
        profile_id: profile_id.to_owned(),
    }) {
        Ok(credentials) => credentials
            .into_iter()
            .filter(|credential| {
                credential.format == "midnight_compact_vc"
                    && credential.verification_outcome == "valid"
            })
            .collect(),
        Err(error) => return PassportVaultPageState::Failed(error.to_string()),
    };
    PassportVaultPageState::Ready {
        vault: Box::new(vault),
        credentials,
        busy: false,
        operation_error,
    }
}

fn parse_vault_amount(value: &str) -> Result<u128, String> {
    ui::parse_night_amount(value, true)
        .map_err(str::to_owned)?
        .parse()
        .map_err(|_| "The NIGHT amount is outside the supported range.".to_owned())
}

fn vault_policy_value(value: &str) -> Result<Option<[u8; 32]>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    if value.trim() != value
        || value.len() > 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
    {
        return Err("Policy values must be 1–32 printable ASCII bytes.".to_owned());
    }
    let mut padded = [0_u8; 32];
    padded[..value.len()].copy_from_slice(value.as_bytes());
    Ok(Some(padded))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PassportVaultContractInputs {
    operation: String,
    lock_id: String,
    amount: String,
    minimum_age: String,
    maximum_claim: String,
    initial_amount: String,
    required_state: String,
    required_document: String,
    credential_id: String,
}

impl PassportVaultContractInputs {
    fn action(&self) -> Result<PreparePassportVaultCallAction, String> {
        let amount = || {
            let amount = parse_vault_amount(&self.amount)?;
            if amount == 0 {
                return Err("The vault operation amount must be greater than zero.".to_owned());
            }
            Ok(amount.to_string())
        };
        let lock_id = || parse_vault_lock_id(&self.lock_id);
        match self.operation.as_str() {
            "create_lock" => {
                let minimum_age_years = self
                    .minimum_age
                    .parse::<u8>()
                    .map_err(|_| "Minimum age must be 0–120.".to_owned())?;
                if minimum_age_years > 120 {
                    return Err("Minimum age must be 0–120.".to_owned());
                }
                Ok(PreparePassportVaultCallAction::CreateLock {
                    minimum_age_years,
                    required_issuing_state: vault_policy_value(&self.required_state)?,
                    required_document_number: vault_policy_value(&self.required_document)?,
                    maximum_claim_amount: parse_vault_amount(&self.maximum_claim)?.to_string(),
                    initial_amount: parse_vault_amount(&self.initial_amount)?.to_string(),
                })
            }
            "deposit_to_lock" => Ok(PreparePassportVaultCallAction::DepositToLock {
                lock_id: lock_id()?,
                amount: amount()?,
            }),
            "claim_from_lock" => {
                if self.credential_id.is_empty() {
                    return Err("Select a verified Digital Passport before claiming.".to_owned());
                }
                Ok(PreparePassportVaultCallAction::ClaimFromLock {
                    lock_id: lock_id()?,
                    amount: amount()?,
                    credential_id: self.credential_id.clone(),
                })
            }
            "withdraw_from_lock" => Ok(PreparePassportVaultCallAction::WithdrawFromLock {
                lock_id: lock_id()?,
                amount: amount()?,
            }),
            _ => Err("Select a supported Passport Vault operation.".to_owned()),
        }
    }
}

fn parse_vault_lock_id(value: &str) -> Result<u64, String> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err("Enter a canonical non-negative lock identifier.".to_owned());
    }
    value
        .parse()
        .map_err(|_| "The lock identifier is outside the supported range.".to_owned())
}

fn passport_vault_call_recovery(retained_state: Option<&str>) -> PassportVaultCallRecovery {
    match retained_state {
        Some("authorized") => PassportVaultCallRecovery::RetryAuthorized,
        _ => PassportVaultCallRecovery::ReconcileUnknown,
    }
}

#[component]
fn PassportVaultContractCallPanel(profile_id: String, credentials: Vec<CredentialView>) -> Element {
    let services = consume_context::<WalletUiServices>();
    let brand = consume_context::<BrandProfile>();
    let security_copy = brand.security_copy();
    let calls = services.passport_vault_contract_calls();
    let configured_address = calls.configured_contract_address_hex.clone();
    let mut contract_address = use_signal(|| configured_address.clone().unwrap_or_default());
    let mut operation = use_signal(|| "create_lock".to_owned());
    let mut lock_id = use_signal(|| "0".to_owned());
    let mut amount = use_signal(|| "10".to_owned());
    let mut minimum_age = use_signal(|| "18".to_owned());
    let mut maximum_claim = use_signal(|| "40".to_owned());
    let mut initial_amount = use_signal(|| "100".to_owned());
    let mut required_state = use_signal(String::new);
    let mut required_document = use_signal(String::new);
    let mut selected_credential = use_signal(|| {
        credentials
            .first()
            .map_or_else(String::new, |credential| credential.id.clone())
    });
    let mut panel = use_signal(|| PassportVaultContractPanelState::Editing);
    let mut chain_state = use_signal(|| PassportVaultContractStatePaneState::Idle);
    let available = matches!(
        calls.mode.as_str(),
        "native_settlement" | "deterministic_simulation"
    );
    let mode_label = ui::vault_call_mode(&calls.mode);
    let mode_note = ui::vault_call_mode_note(&calls.mode);
    let read_state_button_label = match chain_state.read().clone() {
        PassportVaultContractStatePaneState::Loading => "Reading contract state…".to_owned(),
        PassportVaultContractStatePaneState::Ready(_) => "Refresh contract state".to_owned(),
        PassportVaultContractStatePaneState::Idle
        | PassportVaultContractStatePaneState::Failed(_) => "Read contract state".to_owned(),
    };

    rsx! {
        article { class: "info-card",
            div { class: "card-heading",
                div {
                    p { class: "card-eyebrow", "Midnight contract lifecycle" }
                    h2 { "Prepare, authorize, prove, and submit" }
                }
                span {
                    class: if available { "status-pill" } else { "status-pill warning" },
                    "{mode_label}"
                }
            }
            p { "{mode_note}" }
            label { "Contract address (hex)"
                input {
                    r#type: "text",
                    aria_label: "Passport Vault contract address",
                    maxlength: 64,
                    autocomplete: "off",
                    disabled: configured_address.is_some(),
                    value: "{contract_address}",
                    oninput: move |event| contract_address.set(event.value()),
                }
            }
            if configured_address.is_some() {
                p { class: "form-hint", "This deterministic fixture address is fixed by the development composition." }
            } else if calls.mode == "native_settlement" {
                p { class: "form-hint", "Enter the reviewed deployment address. {brand.product_name()} will authenticate state from configured finalized history." }
            }
            div { class: "button-row",
                button {
                    class: "secondary-button",
                    r#type: "button",
                    disabled: !available || contract_address.read().len() != 64,
                    onclick: {
                        let reader = calls.read_state.clone();
                        let address = contract_address.read().clone();
                        move |_| {
                            chain_state.set(PassportVaultContractStatePaneState::Loading);
                            let reader = reader.clone();
                            let address = address.clone();
                            spawn(async move {
                                match run_ui_future(async move {
                                    reader.execute(ReadPassportVaultContractStateCommand {
                                        contract_address_hex: address,
                                    }).await
                                })
                                .await
                                {
                                    Ok(Ok(view)) => chain_state.set(PassportVaultContractStatePaneState::Ready(Box::new(view))),
                                    Ok(Err(error)) => chain_state.set(PassportVaultContractStatePaneState::Failed(error.to_string())),
                                    Err(error) => chain_state.set(PassportVaultContractStatePaneState::Failed(error.to_string())),
                                }
                            });
                        }
                    },
                    "{read_state_button_label}"
                }
            }
            match chain_state.read().clone() {
                PassportVaultContractStatePaneState::Idle => rsx! {},
                PassportVaultContractStatePaneState::Loading => rsx! {
                    p { class: "form-hint", role: "status", "Reading Passport Vault state…" }
                },
                PassportVaultContractStatePaneState::Failed(message) => rsx! {
                    p { class: "field-error", role: "alert", "State unavailable: {message}" }
                },
                PassportVaultContractStatePaneState::Ready(vault) => {
                    let authentication = vault.chain_anchor.as_ref().map_or(
                        ui::vault_state_authentication("simulated_or_unanchored"),
                        |anchor| ui::vault_state_authentication(&anchor.state_authentication),
                    );
                    let source = ui::vault_contract_source(&vault.source);
                    rsx! {
                        p { class: "form-hint", aria_live: "polite",
                            "Contract state loaded from {source}."
                        }
                        div { class: "surface-card",
                            p { class: "card-eyebrow", "Contract state" }
                            dl { class: "preview-list",
                                div { dt { "Source" } dd { "{source}" } }
                                div { dt { "Authentication" } dd { "{authentication}" } }
                                div { dt { "Total locked" } dd { class: "privacy-value", "{ui::format_night_amount(&vault.total_locked)}" } }
                                div { dt { "Locks" } dd { "{vault.locks.len()}" } }
                                if let Some(anchor) = vault.chain_anchor.as_ref() {
                                    div { dt { "Finalized height" } dd { "{anchor.finalized_head_height}" } }
                                }
                            }
                        }
                    }
                },
            }
        }

        if available {
            PassportVaultCallRecoveryPane { profile_id: profile_id.clone() }
        }

        match panel.read().clone() {
            PassportVaultContractPanelState::Editing => {
                let inputs = PassportVaultContractInputs {
                    operation: operation.read().clone(),
                    lock_id: lock_id.read().clone(),
                    amount: amount.read().clone(),
                    minimum_age: minimum_age.read().clone(),
                    maximum_claim: maximum_claim.read().clone(),
                    initial_amount: initial_amount.read().clone(),
                    required_state: required_state.read().clone(),
                    required_document: required_document.read().clone(),
                    credential_id: selected_credential.read().clone(),
                };
                let selected_operation = operation.read().clone();
                rsx! {
                    article { class: "info-card",
                        p { class: "card-eyebrow", "New contract call" }
                        h2 { "Choose an operation" }
                        label { "Operation"
                            select {
                                aria_label: "Passport Vault contract operation",
                                disabled: !available,
                                value: "{operation}",
                                onchange: move |event| operation.set(event.value()),
                                option { value: "create_lock", "Create lock" }
                                option { value: "deposit_to_lock", "Deposit to lock" }
                                option { value: "claim_from_lock", "Claim from lock" }
                                option { value: "withdraw_from_lock", "Withdraw from lock" }
                            }
                        }
                        if selected_operation == "create_lock" {
                            div { class: "field-grid",
                                label { "Minimum age"
                                    input { r#type: "number", min: "0", max: "120", value: "{minimum_age}", oninput: move |event| minimum_age.set(event.value()) }
                                }
                                label { "Maximum claim (NIGHT)"
                                    input { inputmode: "decimal", value: "{maximum_claim}", oninput: move |event| maximum_claim.set(event.value()) }
                                }
                                label { "Initial deposit (NIGHT)"
                                    input { inputmode: "decimal", value: "{initial_amount}", oninput: move |event| initial_amount.set(event.value()) }
                                }
                                label { "Required issuing state (optional)"
                                    input { maxlength: "32", value: "{required_state}", oninput: move |event| required_state.set(event.value()) }
                                }
                                label { "Required document number (optional)"
                                    input { maxlength: "32", value: "{required_document}", oninput: move |event| required_document.set(event.value()) }
                                }
                            }
                        } else {
                            div { class: "field-grid",
                                label { "Lock ID"
                                    input { inputmode: "numeric", value: "{lock_id}", oninput: move |event| lock_id.set(event.value()) }
                                }
                                label { "Amount (NIGHT)"
                                    input { inputmode: "decimal", value: "{amount}", oninput: move |event| amount.set(event.value()) }
                                }
                            }
                            if selected_operation == "claim_from_lock" {
                                label { "Verified Digital Passport"
                                    select {
                                        aria_label: "Passport Vault claim credential",
                                        value: "{selected_credential}",
                                        onchange: move |event| selected_credential.set(event.value()),
                                        option { value: "", "Select a credential" }
                                        for credential in &credentials {
                                            option { value: "{credential.id}", "{credential.display_name} · {credential.id}" }
                                        }
                                    }
                                }
                            }
                        }
                        p { class: "consent-copy", "Preparation reads authenticated public state but does not sign, prove, or submit." }
                        button {
                            class: "primary-button",
                            r#type: "button",
                            disabled: !available || contract_address.read().len() != 64,
                            onclick: {
                                let prepare = calls.prepare.clone();
                                let profile_id = profile_id.clone();
                                let address = contract_address.read().clone();
                                move |_| match inputs.action() {
                                    Err(message) => panel.set(PassportVaultContractPanelState::Failed {
                                        message,
                                        retained: None,
                                        recovery: PassportVaultCallRecovery::Edit,
                                    }),
                                    Ok(action) => {
                                        panel.set(PassportVaultContractPanelState::Preparing);
                                        let prepare = prepare.clone();
                                        let profile_id = profile_id.clone();
                                        let address = address.clone();
                                        spawn(async move {
                                            match run_ui_future(async move {
                                                prepare.execute(PreparePassportVaultCallCommand {
                                                    profile_id,
                                                    contract_address_hex: address,
                                                    action,
                                                }).await
                                            })
                                            .await
                                            {
                                                Ok(Ok(preview)) => panel.set(PassportVaultContractPanelState::Prepared(Box::new(preview))),
                                                Ok(Err(error)) => panel.set(PassportVaultContractPanelState::Failed {
                                                    message: error.to_string(),
                                                    retained: None,
                                                    recovery: PassportVaultCallRecovery::Edit,
                                                }),
                                                Err(error) => panel.set(PassportVaultContractPanelState::Failed {
                                                    message: error.to_string(),
                                                    retained: None,
                                                    recovery: PassportVaultCallRecovery::Edit,
                                                }),
                                            }
                                        });
                                    }
                                }
                            },
                            "Review contract call"
                        }
                    }
                }
            },
            PassportVaultContractPanelState::Preparing => rsx! {
                article { class: "info-card", role: "status", aria_busy: "true",
                    p { class: "card-eyebrow", "Preparing" }
                    h2 { "Reading authenticated vault state" }
                    p { "No protected claim material or transaction signature is produced before review." }
                }
            },
            PassportVaultContractPanelState::Prepared(preview) => {
                let draft_id = preview.draft_id.clone();
                let challenge = preview.authorization_challenge.clone();
                rsx! {
                    PassportVaultCallPreviewCard { preview: preview.clone() }
                    article { class: "info-card review-card",
                        p { class: "consent-copy", "Authorization is bound to this exact operation, amount, contract, state anchor, account context, and expiry. Claim presentations are assembled only after this consent." }
                        div { class: "button-row",
                            button { class: "secondary-button", r#type: "button", onclick: move |_| panel.set(PassportVaultContractPanelState::Editing), "Edit" }
                            button {
                                class: "primary-button",
                                r#type: "button",
                                onclick: {
                                    let authorize = calls.authorize.clone();
                                    let profile_id = profile_id.clone();
                                    move |_| {
                                        let authorize = authorize.clone();
                                        let command = AuthorizePassportVaultCallCommand {
                                            profile_id: profile_id.clone(),
                                            draft_id: draft_id.clone(),
                                            authorization_challenge: challenge.clone(),
                                            confirmed: true,
                                            intent: AUTHORIZE_PASSPORT_VAULT_CALL_INTENT.to_owned(),
                                        };
                                        let retained = preview.clone();
                                        panel.set(PassportVaultContractPanelState::Authorizing(
                                            preview.clone(),
                                        ));
                                        spawn(async move {
                                            match run_ui_blocking(move || authorize.execute(command)).await {
                                                Ok(Ok(authorized)) => panel.set(
                                                    PassportVaultContractPanelState::Authorized(Box::new(authorized)),
                                                ),
                                                Ok(Err(error)) => panel.set(PassportVaultContractPanelState::Failed {
                                                    message: error.to_string(),
                                                    retained: Some(retained.clone()),
                                                    recovery: PassportVaultCallRecovery::Edit,
                                                }),
                                                Err(error) => panel.set(PassportVaultContractPanelState::Failed {
                                                    message: error.to_string(),
                                                    retained: Some(retained),
                                                    recovery: PassportVaultCallRecovery::Edit,
                                                }),
                                            }
                                        });
                                    }
                                },
                                "Authorize exact call"
                            }
                        }
                    }
                }
            },
            PassportVaultContractPanelState::Authorizing(preview) => rsx! {
                PassportVaultCallPreviewCard { preview: preview.clone() }
                article { class: "info-card", role: "status", aria_busy: "true",
                    p { class: "card-eyebrow", "Authorizing" }
                    h2 { "Confirming the exact call with protected custody" }
                    p { "Native NIGHT funding, holder authorization, and device protection can complete without blocking the wallet interface." }
                }
            },
            PassportVaultContractPanelState::Authorized(preview) => {
                let draft_id = preview.draft_id.clone();
                let submitting_preview = preview.clone();
                rsx! {
                    PassportVaultCallPreviewCard { preview: preview.clone() }
                    article { class: "info-card review-card",
                        h2 { "Authorized call is retained safely" }
                        p { "Continue to balance NIGHT/DUST, prove, persist the public attempt, and submit. A failure before broadcast remains retryable." }
                        button {
                            class: "primary-button",
                            r#type: "button",
                            onclick: {
                                let submit = calls.submit.clone();
                                let drafts = calls.get_draft.clone();
                                let profile_id = profile_id.clone();
                                move |_| {
                                    panel.set(PassportVaultContractPanelState::Submitting(submitting_preview.clone()));
                                    let submit = submit.clone();
                                    let drafts = drafts.clone();
                                    let profile_id = profile_id.clone();
                                    let draft_id = draft_id.clone();
                                    spawn(async move {
                                        let execute_profile = profile_id.clone();
                                        let execute_draft = draft_id.clone();
                                        match run_ui_future(async move {
                                            submit.execute(SubmitPassportVaultCallCommand {
                                                profile_id: execute_profile,
                                                draft_id: execute_draft,
                                                confirmed: true,
                                                intent: SUBMIT_PASSPORT_VAULT_CALL_INTENT.to_owned(),
                                            }).await
                                        })
                                        .await
                                        {
                                            Ok(Ok(submission)) => panel.set(PassportVaultContractPanelState::Submitted(Box::new(submission))),
                                            Ok(Err(error)) => {
                                                let retained = drafts.execute(PassportVaultCallQuery {
                                                    profile_id,
                                                    draft_id,
                                                }).ok().map(Box::new);
                                                let recovery = passport_vault_call_recovery(
                                                    retained.as_deref().map(|value| value.state.as_str()),
                                                );
                                                panel.set(PassportVaultContractPanelState::Failed {
                                                    message: error.to_string(),
                                                    retained,
                                                    recovery,
                                                });
                                            }
                                            Err(error) => panel.set(PassportVaultContractPanelState::Failed {
                                                message: error.to_string(),
                                                retained: None,
                                                recovery: PassportVaultCallRecovery::ReconcileUnknown,
                                            }),
                                        }
                                    });
                                }
                            },
                            "Prove and submit"
                        }
                    }
                }
            },
            PassportVaultContractPanelState::Submitting(preview) => {
                let profile = profile_id.clone();
                let draft = preview.draft_id.clone();
                let cancelling = preview.clone();
                rsx! {
                    article { class: "info-card submitting-card", role: "status", aria_live: "polite", aria_busy: "true",
                        p { class: "card-eyebrow", "Submitting" }
                        h2 { "Proving {ui::vault_operation(&preview.operation)}" }
                        p { "{security_copy.vault_broadcast_warning}" }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: {
                                let calls = calls.clone();
                                move |_| match calls.cancel.execute(PassportVaultCallQuery {
                                    profile_id: profile.clone(),
                                    draft_id: draft.clone(),
                                }) {
                                    Ok(status) => {
                                        panel.set(PassportVaultContractPanelState::Cancelling(cancelling.clone()));
                                        poll_passport_vault_cancellation(
                                            calls.clone(),
                                            profile.clone(),
                                            draft.clone(),
                                            panel,
                                            status,
                                        );
                                    }
                                    Err(error) => panel.set(PassportVaultContractPanelState::Failed {
                                        message: error.to_string(),
                                        retained: Some(preview.clone()),
                                        recovery: PassportVaultCallRecovery::ReconcileUnknown,
                                    }),
                                }
                            },
                            "Cancel before broadcast"
                        }
                    }
                }
            },
            PassportVaultContractPanelState::Cancelling(preview) => rsx! {
                article { class: "info-card submitting-card", role: "status", aria_live: "polite", aria_busy: "true",
                    p { class: "card-eyebrow", "Cancelling" }
                    h2 { "Stopping {ui::vault_operation(&preview.operation)} safely" }
                    p { "Waiting for the submission worker to acknowledge a pre-broadcast boundary." }
                }
            },
            PassportVaultContractPanelState::Submitted(submission) => rsx! {
                article { class: "info-card submitted-card", role: "status", aria_live: "polite",
                    p { class: "card-eyebrow", "Included" }
                    h2 { "Passport Vault call completed" }
                    p { "Mode: {ui::vault_submission_mode(&submission.mode)}. Final DUST fee: {ui::format_dust_amount(&submission.fee_atomic_units)}." }
                    dl { class: "preview-list",
                        div { dt { "Operation" } dd { "{ui::vault_operation(&submission.call.operation)}" } }
                        div { dt { "Transaction" } dd { title: "{submission.transaction_hash_hex}", "{truncate_middle(&submission.transaction_hash_hex, 16, 8)}" } }
                        div { dt { "Block" } dd { title: "{submission.block_hash_hex}", "{truncate_middle(&submission.block_hash_hex, 16, 8)}" } }
                        div { dt { "Height" } dd { "{submission.block_height}" } }
                    }
                    button { class: "secondary-button", r#type: "button", onclick: move |_| panel.set(PassportVaultContractPanelState::Editing), "Prepare another call" }
                }
            },
            PassportVaultContractPanelState::Resolved(submission) => rsx! {
                article { class: "info-card", role: "status", aria_live: "polite",
                    p { class: "card-eyebrow", "Cancellation resolved" }
                    h2 { "{ui::vault_submission_heading(&submission.state)}" }
                    p { "{ui::vault_submission_note(&submission.state, brand.product_name())}" }
                    dl { class: "preview-list",
                        div { dt { "State" } dd { "{ui::submission_state(&submission.state)}" } }
                        if let Some(mode) = submission.mode.as_deref() {
                            div { dt { "Mode" } dd { "{ui::vault_submission_mode(mode)}" } }
                        }
                        if let Some(transaction) = submission.transaction_hash_hex.as_deref() {
                            div { dt { "Transaction" } dd { title: "{transaction}", "{truncate_middle(transaction, 16, 8)}" } }
                        }
                        if let Some(block) = submission.block_hash_hex.as_deref() {
                            div { dt { "Block" } dd { title: "{block}", "{truncate_middle(block, 16, 8)}" } }
                        }
                    }
                    button { class: "secondary-button", r#type: "button", onclick: move |_| panel.set(PassportVaultContractPanelState::Editing), "Prepare another call" }
                }
            },
            PassportVaultContractPanelState::Failed { message, retained, recovery } => {
                let retry = retained.clone();
                rsx! {
                    article { class: "info-card warning-card", role: "alert",
                        p { class: "card-eyebrow", "Call not completed" }
                        h2 {
                            if recovery == PassportVaultCallRecovery::ReconcileUnknown {
                                "Submission outcome needs reconciliation"
                            } else if recovery == PassportVaultCallRecovery::RetryAuthorized {
                                "Authorized call can be retried safely"
                            } else {
                                "Review the call configuration"
                            }
                        }
                        p { "{message}" }
                        if recovery == PassportVaultCallRecovery::ReconcileUnknown {
                            p { "{brand.product_name()} will not prepare or submit a replacement while broadcast may have occurred. Use the recovery card above." }
                        } else if recovery == PassportVaultCallRecovery::RetryAuthorized {
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                onclick: move |_| {
                                    if let Some(preview) = retry.clone() {
                                        panel.set(PassportVaultContractPanelState::Authorized(preview));
                                    }
                                },
                                "Retry safe submission"
                            }
                        } else {
                            button { class: "secondary-button", r#type: "button", onclick: move |_| panel.set(PassportVaultContractPanelState::Editing), "Back to call" }
                        }
                    }
                }
            },
        }
    }
}

#[component]
fn PassportVaultCallPreviewCard(preview: Box<PassportVaultCallPreviewView>) -> Element {
    rsx! {
        article { class: "info-card review-card", aria_label: "Reviewed Passport Vault call",
            p { class: "card-eyebrow", "Exact call preview" }
            p { class: "privacy-consent-exemption", "Details shown for authorization." }
            h2 { "{ui::vault_operation(&preview.operation)}" }
            dl { class: "preview-list",
                div { dt { "Amount" } dd { "{ui::format_night_amount(&preview.amount_atomic_units)}" } }
                if let Some(lock_id) = preview.lock_id {
                    div { dt { "Lock" } dd { "#{lock_id}" } }
                }
                div { dt { "State height" } dd { "{preview.state_anchor_block_height}" } }
                div { dt { "State block" } dd { title: "{preview.state_anchor_block_hash_hex}", "{truncate_middle(&preview.state_anchor_block_hash_hex, 16, 8)}" } }
                div { dt { "Draft state" } dd { "{ui::vault_draft_state(&preview.state)}" } }
                div { dt { "DUST fee" } dd { if let Some(fee) = preview.fee_atomic_units.as_deref() { "{ui::format_dust_amount(fee)}" } else { "Calculated during proving" } } }
            }
        }
    }
}

#[component]
fn PassportVaultCallRecoveryPane(profile_id: String) -> Element {
    let services = consume_context::<WalletUiServices>();
    let brand = consume_context::<BrandProfile>();
    let calls = services.passport_vault_contract_calls();
    let mut state = use_signal(|| PassportVaultCallRecoveryPaneState::Loading);
    let load_calls = calls.clone();
    let load_profile = profile_id.clone();
    use_effect(move || {
        let calls = load_calls.clone();
        let profile_id = load_profile.clone();
        spawn(async move {
            let result = run_ui_blocking(move || calls.list.execute(profile_id)).await;
            state.set(match result {
                Ok(Ok(submissions)) => PassportVaultCallRecoveryPaneState::Ready {
                    latest: submissions.into_iter().next().map(Box::new),
                    reconciling: false,
                    operation_error: None,
                },
                Ok(Err(error)) => PassportVaultCallRecoveryPaneState::Failed(error.to_string()),
                Err(error) => PassportVaultCallRecoveryPaneState::Failed(error.to_string()),
            });
        });
    });

    match state.read().clone() {
        PassportVaultCallRecoveryPaneState::Loading
        | PassportVaultCallRecoveryPaneState::Ready { latest: None, .. } => rsx! {},
        PassportVaultCallRecoveryPaneState::Failed(message) => rsx! {
            article { class: "info-card warning-card", role: "alert",
                p { class: "card-eyebrow", "Vault-call recovery" }
                h2 { "Submission history unavailable" }
                p { "{message}" }
            }
        },
        PassportVaultCallRecoveryPaneState::Ready {
            latest: Some(submission),
            reconciling,
            operation_error,
        } => {
            let current = submission.clone();
            let draft_id = submission.draft_id.clone();
            rsx! {
                article { class: "info-card", role: "status", aria_live: "polite", aria_busy: if reconciling { "true" } else { "false" },
                    p { class: "card-eyebrow", "Latest vault call" }
                    h2 { "{ui::vault_submission_heading(&submission.state)}" }
                    p { "{ui::vault_submission_note(&submission.state, brand.product_name())}" }
                    dl { class: "preview-list",
                        div { dt { "State" } dd { "{ui::submission_state(&submission.state)}" } }
                        if let Some(mode) = submission.mode.as_deref() {
                            div { dt { "Mode" } dd { "{ui::vault_submission_mode(mode)}" } }
                        }
                        if let Some(transaction) = submission.transaction_hash_hex.as_deref() {
                            div { dt { "Transaction" } dd { title: "{transaction}", "{truncate_middle(transaction, 16, 8)}" } }
                        }
                        if let Some(block) = submission.block_hash_hex.as_deref() {
                            div { dt { "Block" } dd { title: "{block}", "{truncate_middle(block, 16, 8)}" } }
                        }
                    }
                    if let Some(message) = operation_error {
                        p { class: "field-error", role: "alert", "{message}" }
                    }
                    if submission.reconciliation_allowed {
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: reconciling,
                            onclick: {
                                let calls = calls.clone();
                                let profile_id = profile_id.clone();
                                move |_| {
                                    state.set(PassportVaultCallRecoveryPaneState::Ready {
                                        latest: Some(current.clone()),
                                        reconciling: true,
                                        operation_error: None,
                                    });
                                    let calls = calls.clone();
                                    let profile_id = profile_id.clone();
                                    let draft_id = draft_id.clone();
                                    let fallback = current.clone();
                                    spawn(async move {
                                        match run_ui_future(async move {
                                            calls.reconcile.execute(PassportVaultCallQuery {
                                                profile_id,
                                                draft_id,
                                            }).await
                                        })
                                        .await
                                        {
                                            Ok(Ok(updated)) => state.set(PassportVaultCallRecoveryPaneState::Ready {
                                                latest: Some(Box::new(updated)),
                                                reconciling: false,
                                                operation_error: None,
                                            }),
                                            Ok(Err(error)) => state.set(PassportVaultCallRecoveryPaneState::Ready {
                                                latest: Some(fallback.clone()),
                                                reconciling: false,
                                                operation_error: Some(error.to_string()),
                                            }),
                                            Err(error) => state.set(PassportVaultCallRecoveryPaneState::Ready {
                                                latest: Some(fallback),
                                                reconciling: false,
                                                operation_error: Some(error.to_string()),
                                            }),
                                        }
                                    });
                                }
                            },
                            if reconciling { "Reconciling…" } else { "Reconcile with Midnight" }
                        }
                    }
                }
            }
        }
    }
}

fn poll_passport_vault_cancellation(
    calls: PassportVaultContractCallUiServices,
    profile_id: String,
    draft_id: String,
    mut panel: Signal<PassportVaultContractPanelState>,
    initial: PassportVaultCallSubmissionStatusView,
) {
    spawn(async move {
        let mut status = initial;
        loop {
            match status.state.as_str() {
                "running" | "cancellation_requested" => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    match calls.get_status.execute(PassportVaultCallQuery {
                        profile_id: profile_id.clone(),
                        draft_id: draft_id.clone(),
                    }) {
                        Ok(updated) => status = updated,
                        Err(error) => {
                            panel.set(PassportVaultContractPanelState::Failed {
                                message: format!(
                                    "Cancellation status is unavailable and may require reconciliation: {error}"
                                ),
                                retained: None,
                                recovery: PassportVaultCallRecovery::ReconcileUnknown,
                            });
                            break;
                        }
                    }
                }
                "cancelled" => {
                    let retained = calls
                        .get_draft
                        .execute(PassportVaultCallQuery {
                            profile_id,
                            draft_id,
                        })
                        .ok()
                        .map(Box::new);
                    let recovery = if retained
                        .as_deref()
                        .is_some_and(|preview| preview.state == "authorized")
                    {
                        PassportVaultCallRecovery::RetryAuthorized
                    } else {
                        PassportVaultCallRecovery::Edit
                    };
                    panel.set(PassportVaultContractPanelState::Failed {
                        message: "Vault-call submission was cancelled before broadcast.".to_owned(),
                        retained,
                        recovery,
                    });
                    break;
                }
                "broadcasting" | "outcome_unknown" => {
                    panel.set(PassportVaultContractPanelState::Failed {
                        message:
                            "The vault call may have reached Midnight and requires reconciliation."
                                .to_owned(),
                        retained: None,
                        recovery: PassportVaultCallRecovery::ReconcileUnknown,
                    });
                    break;
                }
                "included" | "rejected" | "expired" => {
                    panel.set(PassportVaultContractPanelState::Resolved(Box::new(status)));
                    break;
                }
                _ => {
                    panel.set(PassportVaultContractPanelState::Failed {
                        message: format!(
                            "Cancellation returned an unsupported status `{}`; reconcile before replacing the call.",
                            status.state
                        ),
                        retained: None,
                        recovery: PassportVaultCallRecovery::ReconcileUnknown,
                    });
                    break;
                }
            }
        }
    });
}

#[component]
pub(super) fn PassportVaultPage(active_profile: WalletProfileView) -> Element {
    let services = consume_context::<WalletUiServices>();
    let state_persistence = services.passport_vault_state_persistence();
    let mut page = use_signal(|| PassportVaultPageState::Loading);
    let mut minimum_age = use_signal(|| "18".to_owned());
    let mut maximum_claim = use_signal(|| "40".to_owned());
    let mut initial_amount = use_signal(|| "100".to_owned());
    let mut required_state = use_signal(String::new);
    let mut required_document = use_signal(String::new);
    let mut operation_amount = use_signal(|| "10".to_owned());
    let mut selected_credential = use_signal(String::new);
    let services_for_load = services.clone();
    let profile_for_load = active_profile.id.clone();
    use_effect(move || {
        let services = services_for_load.clone();
        let profile_id = profile_for_load.clone();
        spawn(async move {
            let loaded =
                run_ui_blocking(move || load_passport_vault_page(&services, &profile_id, None))
                    .await
                    .unwrap_or_else(|error| PassportVaultPageState::Failed(error.to_string()));
            if selected_credential.read().is_empty()
                && let PassportVaultPageState::Ready { credentials, .. } = &loaded
                && let Some(credential) = credentials.first()
            {
                selected_credential.set(credential.id.clone());
            }
            page.set(loaded);
        });
    });

    match page.read().clone() {
        PassportVaultPageState::Loading => rsx! {
            section { class: "page-stack", aria_busy: "true",
                h1 { "Passport Vault" }
                p { "Loading standalone and Midnight vault capabilities…" }
            }
        },
        PassportVaultPageState::Failed(message) => rsx! {
            section { class: "page-stack",
                div { class: "page-heading",
                    div { h1 { "Passport Vault" } p { "Credential-gated NIGHT locks." } }
                    span { class: "status-pill warning", "Unavailable" }
                }
                article { class: "info-card warning-card",
                    h2 { "Vault capability unavailable" }
                    p { "{message}" }
                    p { "Enable the standalone development composition to exercise local and Midnight-shaped vault flows." }
                }
            }
        },
        PassportVaultPageState::Ready {
            vault,
            credentials,
            busy,
            operation_error,
        } => {
            let persistence_note = ui::vault_persistence_note(&state_persistence);
            let profile_id = active_profile.id.clone();
            let create_services = services.clone();
            let create_profile = profile_id.clone();
            let create_state = required_state.read().clone();
            let create_document = required_document.read().clone();
            let create_age = minimum_age.read().clone();
            let create_maximum = maximum_claim.read().clone();
            let create_initial = initial_amount.read().clone();
            let create_vault = vault.clone();
            let create_credentials = credentials.clone();
            rsx! {
                section { class: "page-stack",
                    div { class: "page-heading",
                        div {
                            p { class: "eyebrow", "Product adapter" }
                            h1 { "Passport Vault" }
                            p { "Create, fund, claim, and withdraw credential-gated NIGHT locks." }
                        }
                        span { class: "status-pill", "Standalone + Midnight" }
                    }

                    PassportVaultContractCallPanel {
                        profile_id: profile_id.clone(),
                        credentials: credentials.clone(),
                    }

                    article { class: "balance-card",
                        p { class: "card-eyebrow", "Standalone conformance ledger · total locked" }
                        h2 { class: "privacy-value", "{ui::format_night_amount(&vault.total_locked)}" }
                        div { class: "balance-breakdown",
                            span { class: "privacy-value", "Deposited {ui::format_night_amount(&vault.total_deposited)}" }
                            span { class: "privacy-value", "Released {ui::format_night_amount(&vault.total_released)}" }
                            span { "Claims {vault.claim_count}" }
                        }
                        p { class: "trust-line", "{persistence_note}" }
                    }

                    if let Some(message) = operation_error {
                        p { class: "field-error", role: "alert", "{message}" }
                    }

                    article { class: "info-card",
                        div { class: "card-heading",
                            div { p { class: "card-eyebrow", "Locker flow" } h2 { "Create a lock" } }
                            span { class: "status-pill", "Explicit consent" }
                        }
                        div { class: "field-grid",
                            label { "Minimum age"
                                input { r#type: "number", min: "0", max: "120", aria_label: "Vault minimum age", value: "{minimum_age}", oninput: move |event| minimum_age.set(event.value()) }
                            }
                            label { "Maximum claim (NIGHT)"
                                input { inputmode: "decimal", aria_label: "Vault maximum claim", value: "{maximum_claim}", oninput: move |event| maximum_claim.set(event.value()) }
                            }
                            label { "Initial deposit (NIGHT)"
                                input { inputmode: "decimal", aria_label: "Vault initial deposit", value: "{initial_amount}", oninput: move |event| initial_amount.set(event.value()) }
                            }
                            label { "Required issuing state (optional)"
                                input { maxlength: "32", aria_label: "Vault required issuing state", value: "{required_state}", placeholder: "US", oninput: move |event| required_state.set(event.value()) }
                            }
                            label { "Required document number (optional)"
                                input { maxlength: "32", aria_label: "Vault required document number", value: "{required_document}", placeholder: "AB1234567", oninput: move |event| required_document.set(event.value()) }
                            }
                        }
                        button {
                            class: "primary-button",
                            r#type: "button",
                            disabled: busy,
                            onclick: move |_| {
                                let parsed = (|| {
                                    let age = create_age.parse::<u8>().map_err(|_| "Minimum age must be 0–120.".to_owned())?;
                                    let maximum = parse_vault_amount(&create_maximum)?;
                                    let initial = if create_initial == "0" { 0 } else { parse_vault_amount(&create_initial)? };
                                    let state = vault_policy_value(&create_state)?;
                                    let document = vault_policy_value(&create_document)?;
                                    Ok::<_, String>((age, maximum, initial, state, document))
                                })();
                                match parsed {
                                    Err(message) => page.set(PassportVaultPageState::Ready {
                                        vault: create_vault.clone(),
                                        credentials: create_credentials.clone(),
                                        busy: false,
                                        operation_error: Some(message),
                                    }),
                                    Ok((age, maximum, initial, state, document)) => {
                                        let services = create_services.clone();
                                        let profile_id = create_profile.clone();
                                        page.set(PassportVaultPageState::Ready {
                                            vault: create_vault.clone(),
                                            credentials: create_credentials.clone(),
                                            busy: true,
                                            operation_error: None,
                                        });
                                        spawn(async move {
                                            let result = run_ui_blocking(move || {
                                                let operation_error = services
                                                    .create_passport_vault_lock()
                                                    .execute(CreatePassportVaultLockCommand {
                                                        profile_id: profile_id.clone(),
                                                        minimum_age_years: age,
                                                        required_issuing_state: state,
                                                        required_document_number: document,
                                                        maximum_claim_amount: maximum,
                                                        initial_amount: initial,
                                                        confirmed: true,
                                                        intent: CREATE_LOCK_INTENT.to_owned(),
                                                    })
                                                    .err()
                                                    .map(|error| error.to_string());
                                                load_passport_vault_page(
                                                    &services,
                                                    &profile_id,
                                                    operation_error,
                                                )
                                            })
                                            .await;
                                            page.set(result.unwrap_or_else(|error| {
                                                PassportVaultPageState::Failed(error.to_string())
                                            }));
                                        });
                                    }
                                }
                            },
                            "Create confirmed lock"
                        }
                    }

                    article { class: "info-card",
                        div { class: "card-heading",
                            div { p { class: "card-eyebrow", "Redeemer flow" } h2 { "Claim controls" } }
                            span { class: "status-pill", "Digital Passport" }
                        }
                        label { "Credential"
                            select {
                                aria_label: "Vault credential",
                                value: "{selected_credential}",
                                onchange: move |event| selected_credential.set(event.value()),
                                option { value: "", "Select a verified Digital Passport" }
                                for credential in &credentials {
                                    option { value: "{credential.id}", "{credential.display_name} · {credential.id}" }
                                }
                            }
                        }
                        label { "Operation amount (NIGHT)"
                            input { inputmode: "decimal", aria_label: "Vault operation amount", value: "{operation_amount}", oninput: move |event| operation_amount.set(event.value()) }
                        }
                        if credentials.is_empty() {
                            p { class: "field-hint", "Issue or import a verified compact Digital Passport on the Credentials page before claiming." }
                        }
                    }

                    if vault.locks.is_empty() {
                        article { class: "empty-card", h2 { "No vault locks" } p { "Create the first policy-bound lock above." } }
                    } else {
                        div { class: "credential-list",
                            for lock in vault.locks.clone() {
                                {
                                    let complete_services = services.clone();
                                    let complete_profile = profile_id.clone();
                                    let complete_vault = vault.clone();
                                    let complete_credentials = credentials.clone();
                                    rsx! {
                                        PassportVaultLockCard {
                                            key: "{lock.lock_id}",
                                            lock,
                                            profile_id: profile_id.clone(),
                                            amount: operation_amount.read().clone(),
                                            credential_id: selected_credential.read().clone(),
                                            busy,
                                            on_operation: move |operation: PassportVaultLocalOperation| {
                                                if let PassportVaultLocalOperation::Invalid(message) = &operation {
                                                    page.set(PassportVaultPageState::Ready {
                                                        vault: complete_vault.clone(),
                                                        credentials: complete_credentials.clone(),
                                                        busy: false,
                                                        operation_error: Some(message.clone()),
                                                    });
                                                    return;
                                                }
                                                let services = complete_services.clone();
                                                let profile_id = complete_profile.clone();
                                                page.set(PassportVaultPageState::Ready {
                                                    vault: complete_vault.clone(),
                                                    credentials: complete_credentials.clone(),
                                                    busy: true,
                                                    operation_error: None,
                                                });
                                                spawn(async move {
                                                    let result = run_ui_blocking(move || {
                                                        let operation_error = match operation {
                                                            PassportVaultLocalOperation::Invalid(_) => unreachable!("validated before dispatch"),
                                                            PassportVaultLocalOperation::Deposit { lock_id, amount } => services
                                                                .deposit_passport_vault_lock()
                                                                .execute(PassportVaultAmountCommand {
                                                                    profile_id: profile_id.clone(),
                                                                    lock_id,
                                                                    amount,
                                                                    confirmed: true,
                                                                    intent: DEPOSIT_INTENT.to_owned(),
                                                                })
                                                                .map(|_| ()),
                                                            PassportVaultLocalOperation::Claim { lock_id, credential_id, amount } => futures::executor::block_on(
                                                                services.claim_passport_vault_lock().execute(ClaimPassportVaultLockCommand {
                                                                    profile_id: profile_id.clone(),
                                                                    lock_id,
                                                                    credential_id,
                                                                    amount,
                                                                    confirmed: true,
                                                                    intent: CLAIM_INTENT.to_owned(),
                                                                }),
                                                            )
                                                            .map(|_| ()),
                                                            PassportVaultLocalOperation::Withdraw { lock_id, amount } => services
                                                                .withdraw_passport_vault_lock()
                                                                .execute(PassportVaultAmountCommand {
                                                                    profile_id: profile_id.clone(),
                                                                    lock_id,
                                                                    amount,
                                                                    confirmed: true,
                                                                    intent: WITHDRAW_INTENT.to_owned(),
                                                                })
                                                                .map(|_| ()),
                                                        }
                                                        .err()
                                                        .map(|error| error.to_string());
                                                        load_passport_vault_page(&services, &profile_id, operation_error)
                                                    })
                                                    .await;
                                                    page.set(result.unwrap_or_else(|error| {
                                                        PassportVaultPageState::Failed(error.to_string())
                                                    }));
                                                });
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PassportVaultLockCard(
    lock: PassportVaultLockView,
    profile_id: String,
    amount: String,
    credential_id: String,
    busy: bool,
    on_operation: EventHandler<PassportVaultLocalOperation>,
) -> Element {
    let creator = lock.creator_profile_id == profile_id;
    let policy_detail = format!(
        "Age {}+ · max {}{}{}",
        lock.minimum_age_years,
        ui::format_night_amount(&lock.maximum_claim_amount),
        lock.required_issuing_state
            .as_ref()
            .map_or(String::new(), |value| format!(" · state {value}")),
        lock.required_document_number
            .as_ref()
            .map_or(String::new(), |value| format!(" · document {value}")),
    );
    rsx! {
        article { class: "credential-card",
            div { class: "credential-card__heading",
                div { p { class: "card-eyebrow", "Lock #{lock.lock_id}" } h2 { class: "privacy-value", "{ui::format_night_amount(&lock.remaining)} remaining" } }
                span { class: "status-pill", if creator { "Your lock" } else { "Claimable" } }
            }
            p { "{policy_detail}" }
            p { class: "field-hint privacy-value", "Deposited {ui::format_night_amount(&lock.total_deposited)} · released {ui::format_night_amount(&lock.total_released)}" }
            div { class: "button-row",
                button {
                    class: "secondary-button", r#type: "button", disabled: busy || !creator,
                    onclick: {
                        let amount = amount.clone();
                        move |_| {
                            on_operation.call(match parse_vault_amount(&amount) {
                                Ok(amount) => PassportVaultLocalOperation::Deposit {
                                    lock_id: lock.lock_id,
                                    amount,
                                },
                                Err(message) => PassportVaultLocalOperation::Invalid(message),
                            });
                        }
                    },
                    "Deposit"
                }
                button {
                    class: "primary-button", r#type: "button", disabled: busy || credential_id.is_empty(),
                    onclick: {
                        let amount = amount.clone();
                        let credential_id = credential_id.clone();
                        move |_| {
                            let Ok(amount) = parse_vault_amount(&amount) else {
                                on_operation.call(PassportVaultLocalOperation::Invalid(
                                    "Enter a valid claim amount.".to_owned(),
                                ));
                                return;
                            };
                            on_operation.call(PassportVaultLocalOperation::Claim {
                                lock_id: lock.lock_id,
                                credential_id: credential_id.clone(),
                                amount,
                            });
                        }
                    },
                    "Claim with credential"
                }
                button {
                    class: "secondary-button", r#type: "button", disabled: busy || !creator,
                    onclick: {
                        let amount = amount.clone();
                        move |_| {
                            on_operation.call(match parse_vault_amount(&amount) {
                                Ok(amount) => PassportVaultLocalOperation::Withdraw {
                                    lock_id: lock.lock_id,
                                    amount,
                                },
                                Err(message) => PassportVaultLocalOperation::Invalid(message),
                            });
                        }
                    },
                    "Withdraw"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_contract_inputs(operation: &str) -> PassportVaultContractInputs {
        PassportVaultContractInputs {
            operation: operation.to_owned(),
            lock_id: "7".to_owned(),
            amount: "10".to_owned(),
            minimum_age: "18".to_owned(),
            maximum_claim: "40".to_owned(),
            initial_amount: "100".to_owned(),
            required_state: "US".to_owned(),
            required_document: "AB1234567".to_owned(),
            credential_id: "credential_test".to_owned(),
        }
    }

    #[test]
    fn mobile_vault_inputs_map_only_the_closed_native_operation_set() {
        assert!(matches!(
            vault_contract_inputs("create_lock").action(),
            Ok(PreparePassportVaultCallAction::CreateLock {
                minimum_age_years: 18,
                maximum_claim_amount,
                initial_amount,
                ..
            }) if maximum_claim_amount == "40000000" && initial_amount == "100000000"
        ));
        assert!(matches!(
            vault_contract_inputs("deposit_to_lock").action(),
            Ok(PreparePassportVaultCallAction::DepositToLock {
                lock_id: 7,
                amount,
            }) if amount == "10000000"
        ));
        assert!(matches!(
            vault_contract_inputs("claim_from_lock").action(),
            Ok(PreparePassportVaultCallAction::ClaimFromLock {
                lock_id: 7,
                amount,
                credential_id,
            }) if amount == "10000000" && credential_id == "credential_test"
        ));
        assert!(matches!(
            vault_contract_inputs("withdraw_from_lock").action(),
            Ok(PreparePassportVaultCallAction::WithdrawFromLock {
                lock_id: 7,
                amount,
            }) if amount == "10000000"
        ));
        assert!(
            vault_contract_inputs("set_trusted_issuer")
                .action()
                .is_err()
        );
    }

    #[test]
    fn mobile_vault_claims_require_opaque_credentials_and_nonzero_canonical_amounts() {
        let mut missing_credential = vault_contract_inputs("claim_from_lock");
        missing_credential.credential_id.clear();
        assert!(missing_credential.action().is_err());

        let mut zero = vault_contract_inputs("deposit_to_lock");
        zero.amount = "0".to_owned();
        assert!(zero.action().is_err());

        let mut ambiguous_lock = vault_contract_inputs("withdraw_from_lock");
        ambiguous_lock.lock_id = "07".to_owned();
        assert!(ambiguous_lock.action().is_err());
    }

    #[test]
    fn mobile_vault_modes_and_recovery_copy_never_overstate_settlement() {
        assert_eq!(
            ui::vault_call_mode("deterministic_simulation"),
            "Simulated — runs locally, nothing on Midnight"
        );
        assert!(ui::vault_call_mode_note("deterministic_simulation").contains("no node broadcast"));
        assert_eq!(ui::vault_call_mode("native_settlement"), "Midnight live");
        assert_eq!(
            ui::vault_contract_source("deterministic_simulation"),
            "Simulated — runs locally, nothing on Midnight"
        );
        assert_eq!(
            ui::vault_contract_source("authenticated_node"),
            "Midnight node"
        );
        assert_eq!(
            ui::vault_submission_mode("deterministic_simulation_only"),
            "Simulated — runs locally, nothing on Midnight"
        );
        assert_eq!(ui::vault_submission_mode("midnight"), "Mode unavailable");
        assert!(
            ui::vault_call_mode_note("native_settlement").contains("authenticated finalized state")
        );
        assert_eq!(
            passport_vault_call_recovery(Some("authorized")),
            PassportVaultCallRecovery::RetryAuthorized
        );
        assert_eq!(
            passport_vault_call_recovery(Some("submitting")),
            PassportVaultCallRecovery::ReconcileUnknown
        );
        assert!(
            ui::vault_submission_note("outcome_unknown", "Oxid").contains("not submit a duplicate")
        );
    }
}
