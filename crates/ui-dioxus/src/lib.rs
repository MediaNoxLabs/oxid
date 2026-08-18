// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{fmt, future::Future, sync::Arc, time::Duration};

use dioxus::prelude::*;
use oxid_credential_application::{
    CredentialDisclosureQuery, CredentialDisclosureView, CredentialOperationError,
    CredentialPredicateInput, CredentialProfileQuery, CredentialQuery, CredentialView,
    DeleteCredentialCommand, DeleteCredentialUseCase, GetCredentialDisclosureUseCase,
    GetCredentialUseCase, ListCredentialsUseCase, PreviewCredentialDisclosureCommand,
    PreviewCredentialDisclosureUseCase, ReceiveCredentialUseCase, RevealCredentialClaimCommand,
    RevealCredentialClaimUseCase, ReverifyCredentialUseCase,
};
use oxid_diagnostics_application::{
    CLEAR_LOCAL_DIAGNOSTICS_INTENT, ClearDiagnosticsCommand, ClearDiagnosticsUseCase,
    DiagnosticSnapshotView, GetDiagnosticSnapshotUseCase,
};
use oxid_identity_application::{
    CreateDidCommand, CreateDidUseCase, DeactivateDidCommand, DeactivateDidUseCase,
    DidKeyAlgorithm, DidOperationConfirmation, DidOperationError, DidRecordQuery, DidRecordView,
    DidUpdate, ForgetDidUseCase, ListDidRecordsQuery, ListDidRecordsUseCase, ResolveDidCommand,
    ResolveDidUseCase, SignDidPayloadCommand, SignDidPayloadUseCase, UpdateDidCommand,
    UpdateDidUseCase,
};
use oxid_identity_domain::VerificationRelationship;
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
use oxid_platform_ports::{
    IdentityLinkIngressError, IdentityLinkIngressPort, PublicReceiveAddress, PublicTextExportError,
    PublicTextExportPort, QrScanError, QrScannerPort,
};
use oxid_presentation_application::{
    AcceptCredentialPresentationCommand, AcceptCredentialPresentationUseCase,
    CredentialPresentationError, CredentialPresentationView, PrepareCredentialPresentationCommand,
    PrepareCredentialPresentationUseCase, PresentationProtocolError,
    RefuseCredentialPresentationCommand, RefuseCredentialPresentationUseCase,
};
use oxid_protocol_application::{
    AcceptCredentialIssuanceCommand, AcceptCredentialIssuanceUseCase,
    AcceptSelfIssuedAuthenticationCommand, AcceptSelfIssuedAuthenticationUseCase,
    CredentialIssuanceError, CredentialIssuanceView, IdentityRequestKind,
    IdentityRequestRoutingError, PrepareCredentialIssuanceCommand,
    PrepareCredentialIssuanceUseCase, PrepareSelfIssuedAuthenticationCommand,
    PrepareSelfIssuedAuthenticationUseCase, RefuseCredentialIssuanceCommand,
    RefuseCredentialIssuanceUseCase, RefuseSelfIssuedAuthenticationCommand,
    RefuseSelfIssuedAuthenticationUseCase, RouteIdentityRequestCommand,
    RouteIdentityRequestUseCase, SelfIssuedAuthenticationError, SelfIssuedAuthenticationView,
};
use oxid_wallet_application::{
    AuthorizeWalletTransferCommand, AuthorizeWalletTransferUseCase, CancelWalletDustSyncUseCase,
    CancelWalletShieldedSyncUseCase, CancelWalletTransferSubmissionUseCase,
    CompleteWalletRecoverySummary, CreateWalletProfileCommand, CreateWalletProfileUseCase,
    DeriveWalletAccountCommand, DeriveWalletAccountUseCase, EXPORT_COMPLETE_WALLET_BACKUP_SUMMARY,
    EXPORT_COMPLETE_WALLET_BACKUP_TITLE, ExportCompleteWalletBackupCommand,
    ExportCompleteWalletBackupUseCase, GetActiveWalletProfileUseCase, GetWalletAccountUseCase,
    GetWalletDustSyncStatusUseCase, GetWalletSecurityStatusUseCase,
    GetWalletShieldedSyncStatusUseCase, GetWalletTransferDraftUseCase,
    GetWalletTransferSubmissionStatusUseCase, InitializeWalletSecurityUseCase,
    ListWalletNetworksUseCase, ListWalletProfilesUseCase, ListWalletTransferSubmissionsUseCase,
    LockWalletUseCase, MAX_WALLET_RECOVERY_SECRET_CHARACTERS, PortableWalletBackupDocumentError,
    PortableWalletBackupDocumentKind, PortableWalletBackupDocumentPort,
    PrepareShieldedWalletTransferCommand, PrepareShieldedWalletTransferUseCase,
    PrepareWalletTransferCommand, PrepareWalletTransferUseCase,
    RECOVER_COMPLETE_WALLET_BACKUP_SUMMARY, RECOVER_COMPLETE_WALLET_BACKUP_TITLE,
    RECOVER_PORTABLE_WALLET_BACKUP_SUMMARY, RECOVER_PORTABLE_WALLET_BACKUP_TITLE,
    ReconcileWalletTransferSubmissionUseCase, RecoverCompleteWalletBackupCommand,
    RecoverCompleteWalletBackupUseCase, RecoverPortableWalletBackupCommand,
    RecoverPortableWalletBackupUseCase, SelectWalletNetworkCommand, SelectWalletNetworkUseCase,
    SelectWalletProfileCommand, SelectWalletProfileUseCase, SensitiveOperationConfirmation,
    StartWalletDustSyncUseCase, StartWalletShieldedSyncUseCase, SubmitWalletTransferCommand,
    SubmitWalletTransferUseCase, SyncWalletAccountUseCase, UnlockWalletUseCase, WalletAccountError,
    WalletAccountPortError, WalletAccountQuery, WalletAccountView, WalletDustSyncCommand,
    WalletDustSyncView, WalletNetworkListView, WalletProfileSecurityCommand, WalletProfileView,
    WalletRecoverySecret, WalletSecurityStatusView, WalletShieldedSyncCommand,
    WalletShieldedSyncView, WalletSyncStatusView, WalletTransferDraftQuery,
    WalletTransferPreviewView, WalletTransferSubmissionQuery, WalletTransferSubmissionStatusView,
    WalletTransferSubmissionView,
};
use zeroize::Zeroizing;

const STYLES: &str = include_str!("../assets/styles.css");
#[cfg(not(target_arch = "wasm32"))]
const UI_BLOCKING_TASK_STACK_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiBlockingTaskError {
    WorkerUnavailable,
    WorkerFailed,
}

impl fmt::Display for UiBlockingTaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("background wallet operation failed")
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_ui_blocking<Output, Operation>(
    operation: Operation,
) -> Result<Output, UiBlockingTaskError>
where
    Output: Send + 'static,
    Operation: FnOnce() -> Output + Send + 'static,
{
    let (sender, receiver) = futures::channel::oneshot::channel();
    std::thread::Builder::new()
        .name("oxid-ui-blocking".to_owned())
        .stack_size(UI_BLOCKING_TASK_STACK_BYTES)
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
            let _ = sender.send(outcome);
        })
        .map_err(|_| UiBlockingTaskError::WorkerUnavailable)?;

    match receiver.await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(_)) | Err(_) => Err(UiBlockingTaskError::WorkerFailed),
    }
}

#[cfg(target_arch = "wasm32")]
async fn run_ui_blocking<Output, Operation>(
    operation: Operation,
) -> Result<Output, UiBlockingTaskError>
where
    Operation: FnOnce() -> Output,
{
    // Browser composition has no native authorization bridge or filesystem
    // worker. Its in-memory adapters remain synchronous until Tier-2 gains a
    // reviewed Web Worker boundary.
    Ok(operation())
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_ui_future<Output, Operation>(
    operation: Operation,
) -> Result<Output, UiBlockingTaskError>
where
    Output: Send + 'static,
    Operation: Future<Output = Output> + Send + 'static,
{
    run_ui_blocking(move || futures::executor::block_on(operation)).await
}

#[cfg(target_arch = "wasm32")]
async fn run_ui_future<Output, Operation>(
    operation: Operation,
) -> Result<Output, UiBlockingTaskError>
where
    Operation: Future<Output = Output>,
{
    // See `run_ui_blocking`: Tier-2 currently composes only in-memory
    // adapters. A production browser must provide a Web Worker boundary.
    Ok(operation.await)
}

/// Incoming capabilities made available to Dioxus by the composition root.
#[derive(Clone)]
pub struct WalletUiServices {
    get_diagnostic_snapshot: Arc<dyn GetDiagnosticSnapshotUseCase>,
    clear_diagnostics: Arc<dyn ClearDiagnosticsUseCase>,
    qr_scanner: Arc<dyn QrScannerPort>,
    identity_link_ingress: Arc<dyn IdentityLinkIngressPort>,
    public_text_exporter: Arc<dyn PublicTextExportPort>,
    portable_wallet_backup_documents: Arc<dyn PortableWalletBackupDocumentPort>,
    route_identity_request: Arc<dyn RouteIdentityRequestUseCase>,
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
    list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
    select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
    get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
    get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase>,
    initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase>,
    unlock_wallet: Arc<dyn UnlockWalletUseCase>,
    lock_wallet: Arc<dyn LockWalletUseCase>,
    recover_portable_wallet_backup: Arc<dyn RecoverPortableWalletBackupUseCase>,
    export_complete_wallet_backup: Arc<dyn ExportCompleteWalletBackupUseCase>,
    recover_complete_wallet_backup: Arc<dyn RecoverCompleteWalletBackupUseCase>,
    list_wallet_networks: Arc<dyn ListWalletNetworksUseCase>,
    select_wallet_network: Arc<dyn SelectWalletNetworkUseCase>,
    derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase>,
    get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
    sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
    get_wallet_dust_sync_status: Arc<dyn GetWalletDustSyncStatusUseCase>,
    start_wallet_dust_sync: Arc<dyn StartWalletDustSyncUseCase>,
    cancel_wallet_dust_sync: Arc<dyn CancelWalletDustSyncUseCase>,
    get_wallet_shielded_sync_status: Arc<dyn GetWalletShieldedSyncStatusUseCase>,
    start_wallet_shielded_sync: Arc<dyn StartWalletShieldedSyncUseCase>,
    cancel_wallet_shielded_sync: Arc<dyn CancelWalletShieldedSyncUseCase>,
    prepare_shielded_wallet_transfer: Arc<dyn PrepareShieldedWalletTransferUseCase>,
    prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
    authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
    submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
    get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
    get_wallet_transfer_submission_status: Arc<dyn GetWalletTransferSubmissionStatusUseCase>,
    cancel_wallet_transfer_submission: Arc<dyn CancelWalletTransferSubmissionUseCase>,
    list_wallet_transfer_submissions: Arc<dyn ListWalletTransferSubmissionsUseCase>,
    reconcile_wallet_transfer_submission: Arc<dyn ReconcileWalletTransferSubmissionUseCase>,
    create_did: Arc<dyn CreateDidUseCase>,
    resolve_did: Arc<dyn ResolveDidUseCase>,
    list_did_records: Arc<dyn ListDidRecordsUseCase>,
    update_did: Arc<dyn UpdateDidUseCase>,
    deactivate_did: Arc<dyn DeactivateDidUseCase>,
    sign_did_payload: Arc<dyn SignDidPayloadUseCase>,
    forget_did: Arc<dyn ForgetDidUseCase>,
    receive_credential: Arc<dyn ReceiveCredentialUseCase>,
    list_credentials: Arc<dyn ListCredentialsUseCase>,
    get_credential: Arc<dyn GetCredentialUseCase>,
    reverify_credential: Arc<dyn ReverifyCredentialUseCase>,
    delete_credential: Arc<dyn DeleteCredentialUseCase>,
    get_credential_disclosure: Arc<dyn GetCredentialDisclosureUseCase>,
    preview_credential_disclosure: Arc<dyn PreviewCredentialDisclosureUseCase>,
    reveal_credential_claim: Arc<dyn RevealCredentialClaimUseCase>,
    prepare_credential_issuance: Arc<dyn PrepareCredentialIssuanceUseCase>,
    accept_credential_issuance: Arc<dyn AcceptCredentialIssuanceUseCase>,
    refuse_credential_issuance: Arc<dyn RefuseCredentialIssuanceUseCase>,
    standalone_credential_offer: Option<String>,
    prepare_credential_presentation: Arc<dyn PrepareCredentialPresentationUseCase>,
    accept_credential_presentation: Arc<dyn AcceptCredentialPresentationUseCase>,
    refuse_credential_presentation: Arc<dyn RefuseCredentialPresentationUseCase>,
    standalone_openid4vp_request: Option<String>,
    prepare_self_issued_authentication: Arc<dyn PrepareSelfIssuedAuthenticationUseCase>,
    accept_self_issued_authentication: Arc<dyn AcceptSelfIssuedAuthenticationUseCase>,
    refuse_self_issued_authentication: Arc<dyn RefuseSelfIssuedAuthenticationUseCase>,
    standalone_self_issued_request: Option<String>,
    list_passport_vault_locks: Arc<dyn ListPassportVaultLocksUseCase>,
    create_passport_vault_lock: Arc<dyn CreatePassportVaultLockUseCase>,
    deposit_passport_vault_lock: Arc<dyn DepositPassportVaultLockUseCase>,
    claim_passport_vault_lock: Arc<dyn ClaimPassportVaultLockUseCase>,
    withdraw_passport_vault_lock: Arc<dyn WithdrawPassportVaultLockUseCase>,
    passport_vault_state_persistence: String,
    passport_vault_contract_calls: PassportVaultContractCallUiServices,
}

/// Process-local, payload-free diagnostic use cases consumed by the
/// Diagnostics page.
pub struct DiagnosticsUiServices {
    get: Arc<dyn GetDiagnosticSnapshotUseCase>,
    clear: Arc<dyn ClearDiagnosticsUseCase>,
}

impl DiagnosticsUiServices {
    #[must_use]
    pub const fn new(
        get: Arc<dyn GetDiagnosticSnapshotUseCase>,
        clear: Arc<dyn ClearDiagnosticsUseCase>,
    ) -> Self {
        Self { get, clear }
    }
}

/// Product-specific Passport Vault capabilities consumed only by the Vault page.
pub struct PassportVaultUiServices {
    list: Arc<dyn ListPassportVaultLocksUseCase>,
    create: Arc<dyn CreatePassportVaultLockUseCase>,
    deposit: Arc<dyn DepositPassportVaultLockUseCase>,
    claim: Arc<dyn ClaimPassportVaultLockUseCase>,
    withdraw: Arc<dyn WithdrawPassportVaultLockUseCase>,
    state_persistence: String,
    contract_calls: PassportVaultContractCallUiServices,
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

/// Runtime wallet flows kept separate from profile, security, and identity
/// service bundles at the incoming composition boundary.
pub struct WalletOperationalUiServices {
    dust: WalletDustSyncUiServices,
    shielded: WalletShieldedSyncUiServices,
    transactions: WalletTransactionUiServices,
    vault: PassportVaultUiServices,
}

impl WalletOperationalUiServices {
    #[must_use]
    pub const fn new(
        dust: WalletDustSyncUiServices,
        shielded: WalletShieldedSyncUiServices,
        transactions: WalletTransactionUiServices,
        vault: PassportVaultUiServices,
    ) -> Self {
        Self {
            dust,
            shielded,
            transactions,
            vault,
        }
    }
}

/// DID inventory and resolution use cases consumed by the DIDs page.
pub struct DidUiServices {
    create_did: Arc<dyn CreateDidUseCase>,
    resolve_did: Arc<dyn ResolveDidUseCase>,
    list_did_records: Arc<dyn ListDidRecordsUseCase>,
    update_did: Arc<dyn UpdateDidUseCase>,
    deactivate_did: Arc<dyn DeactivateDidUseCase>,
    sign_did_payload: Arc<dyn SignDidPayloadUseCase>,
    forget_did: Arc<dyn ForgetDidUseCase>,
}

/// Credential inventory use cases consumed by the Credentials page.
pub struct CredentialUiServices {
    receive_credential: Arc<dyn ReceiveCredentialUseCase>,
    list_credentials: Arc<dyn ListCredentialsUseCase>,
    get_credential: Arc<dyn GetCredentialUseCase>,
    reverify_credential: Arc<dyn ReverifyCredentialUseCase>,
    delete_credential: Arc<dyn DeleteCredentialUseCase>,
    get_credential_disclosure: Arc<dyn GetCredentialDisclosureUseCase>,
    preview_credential_disclosure: Arc<dyn PreviewCredentialDisclosureUseCase>,
    reveal_credential_claim: Arc<dyn RevealCredentialClaimUseCase>,
    prepare_credential_issuance: Arc<dyn PrepareCredentialIssuanceUseCase>,
    accept_credential_issuance: Arc<dyn AcceptCredentialIssuanceUseCase>,
    refuse_credential_issuance: Arc<dyn RefuseCredentialIssuanceUseCase>,
    standalone_credential_offer: Option<String>,
    prepare_credential_presentation: Arc<dyn PrepareCredentialPresentationUseCase>,
    accept_credential_presentation: Arc<dyn AcceptCredentialPresentationUseCase>,
    refuse_credential_presentation: Arc<dyn RefuseCredentialPresentationUseCase>,
    standalone_openid4vp_request: Option<String>,
}

/// Protected credential inventory capabilities consumed by the Credentials page.
pub struct CredentialInventoryUiServices {
    receive: Arc<dyn ReceiveCredentialUseCase>,
    list: Arc<dyn ListCredentialsUseCase>,
    get: Arc<dyn GetCredentialUseCase>,
    reverify: Arc<dyn ReverifyCredentialUseCase>,
    delete: Arc<dyn DeleteCredentialUseCase>,
}

impl CredentialInventoryUiServices {
    #[must_use]
    pub fn new(
        receive: Arc<dyn ReceiveCredentialUseCase>,
        list: Arc<dyn ListCredentialsUseCase>,
        get: Arc<dyn GetCredentialUseCase>,
        reverify: Arc<dyn ReverifyCredentialUseCase>,
        delete: Arc<dyn DeleteCredentialUseCase>,
    ) -> Self {
        Self {
            receive,
            list,
            get,
            reverify,
            delete,
        }
    }
}

/// Consent-driven credential issuance capabilities consumed by the Credentials page.
pub struct CredentialIssuanceUiServices {
    prepare_credential_issuance: Arc<dyn PrepareCredentialIssuanceUseCase>,
    accept_credential_issuance: Arc<dyn AcceptCredentialIssuanceUseCase>,
    refuse_credential_issuance: Arc<dyn RefuseCredentialIssuanceUseCase>,
    standalone_credential_offer: Option<String>,
}

/// Consent-driven OpenID4VP capabilities consumed by the Credentials page.
pub struct CredentialPresentationUiServices {
    prepare: Arc<dyn PrepareCredentialPresentationUseCase>,
    accept: Arc<dyn AcceptCredentialPresentationUseCase>,
    refuse: Arc<dyn RefuseCredentialPresentationUseCase>,
    standalone_request: Option<String>,
}

impl CredentialPresentationUiServices {
    #[must_use]
    pub fn new(
        prepare: Arc<dyn PrepareCredentialPresentationUseCase>,
        accept: Arc<dyn AcceptCredentialPresentationUseCase>,
        refuse: Arc<dyn RefuseCredentialPresentationUseCase>,
        standalone_request: Option<String>,
    ) -> Self {
        Self {
            prepare,
            accept,
            refuse,
            standalone_request,
        }
    }
}

/// Targeted protected-claim controls consumed only by schema-aware cards.
pub struct CredentialDisclosureUiServices {
    get: Arc<dyn GetCredentialDisclosureUseCase>,
    preview: Arc<dyn PreviewCredentialDisclosureUseCase>,
    reveal_local: Arc<dyn RevealCredentialClaimUseCase>,
}

impl CredentialDisclosureUiServices {
    #[must_use]
    pub fn new(
        get: Arc<dyn GetCredentialDisclosureUseCase>,
        preview: Arc<dyn PreviewCredentialDisclosureUseCase>,
        reveal_local: Arc<dyn RevealCredentialClaimUseCase>,
    ) -> Self {
        Self {
            get,
            preview,
            reveal_local,
        }
    }
}

/// Consent-driven self-issued DID authentication capabilities consumed by the DIDs page.
pub struct SelfIssuedAuthenticationUiServices {
    prepare: Arc<dyn PrepareSelfIssuedAuthenticationUseCase>,
    accept: Arc<dyn AcceptSelfIssuedAuthenticationUseCase>,
    refuse: Arc<dyn RefuseSelfIssuedAuthenticationUseCase>,
    standalone_request: Option<String>,
}

impl SelfIssuedAuthenticationUiServices {
    #[must_use]
    pub fn new(
        prepare: Arc<dyn PrepareSelfIssuedAuthenticationUseCase>,
        accept: Arc<dyn AcceptSelfIssuedAuthenticationUseCase>,
        refuse: Arc<dyn RefuseSelfIssuedAuthenticationUseCase>,
        standalone_request: Option<String>,
    ) -> Self {
        Self {
            prepare,
            accept,
            refuse,
            standalone_request,
        }
    }
}

impl CredentialIssuanceUiServices {
    #[must_use]
    pub fn new(
        prepare_credential_issuance: Arc<dyn PrepareCredentialIssuanceUseCase>,
        accept_credential_issuance: Arc<dyn AcceptCredentialIssuanceUseCase>,
        refuse_credential_issuance: Arc<dyn RefuseCredentialIssuanceUseCase>,
        standalone_credential_offer: Option<String>,
    ) -> Self {
        Self {
            prepare_credential_issuance,
            accept_credential_issuance,
            refuse_credential_issuance,
            standalone_credential_offer,
        }
    }
}

impl CredentialUiServices {
    #[must_use]
    pub fn new(
        inventory: CredentialInventoryUiServices,
        issuance: CredentialIssuanceUiServices,
        presentation: CredentialPresentationUiServices,
        disclosure: CredentialDisclosureUiServices,
    ) -> Self {
        Self {
            receive_credential: inventory.receive,
            list_credentials: inventory.list,
            get_credential: inventory.get,
            reverify_credential: inventory.reverify,
            delete_credential: inventory.delete,
            get_credential_disclosure: disclosure.get,
            preview_credential_disclosure: disclosure.preview,
            reveal_credential_claim: disclosure.reveal_local,
            prepare_credential_issuance: issuance.prepare_credential_issuance,
            accept_credential_issuance: issuance.accept_credential_issuance,
            refuse_credential_issuance: issuance.refuse_credential_issuance,
            standalone_credential_offer: issuance.standalone_credential_offer,
            prepare_credential_presentation: presentation.prepare,
            accept_credential_presentation: presentation.accept,
            refuse_credential_presentation: presentation.refuse,
            standalone_openid4vp_request: presentation.standalone_request,
        }
    }
}

/// Identity-facing UI capabilities kept separate from wallet account services.
pub struct IdentityUiServices {
    dids: DidUiServices,
    credentials: CredentialUiServices,
    authentication: SelfIssuedAuthenticationUiServices,
    ingress: IdentityIngressUiServices,
}

/// Scan and protocol-link routing capabilities shared by the identity pages.
pub struct IdentityIngressUiServices {
    qr_scanner: Arc<dyn QrScannerPort>,
    app_links: Arc<dyn IdentityLinkIngressPort>,
    route: Arc<dyn RouteIdentityRequestUseCase>,
}

impl IdentityIngressUiServices {
    #[must_use]
    pub fn new(
        qr_scanner: Arc<dyn QrScannerPort>,
        app_links: Arc<dyn IdentityLinkIngressPort>,
        route: Arc<dyn RouteIdentityRequestUseCase>,
    ) -> Self {
        Self {
            qr_scanner,
            app_links,
            route,
        }
    }
}

impl IdentityUiServices {
    #[must_use]
    pub const fn new(
        dids: DidUiServices,
        credentials: CredentialUiServices,
        authentication: SelfIssuedAuthenticationUiServices,
        ingress: IdentityIngressUiServices,
    ) -> Self {
        Self {
            dids,
            credentials,
            authentication,
            ingress,
        }
    }
}

impl DidUiServices {
    #[must_use]
    pub const fn new(
        create_did: Arc<dyn CreateDidUseCase>,
        resolve_did: Arc<dyn ResolveDidUseCase>,
        list_did_records: Arc<dyn ListDidRecordsUseCase>,
        update_did: Arc<dyn UpdateDidUseCase>,
        deactivate_did: Arc<dyn DeactivateDidUseCase>,
        sign_did_payload: Arc<dyn SignDidPayloadUseCase>,
        forget_did: Arc<dyn ForgetDidUseCase>,
    ) -> Self {
        Self {
            create_did,
            resolve_did,
            list_did_records,
            update_did,
            deactivate_did,
            sign_did_payload,
            forget_did,
        }
    }
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
    backup: WalletBackupUiServices,
}

/// Complete and legacy custody-only backup use cases plus native document transport.
pub struct WalletBackupUiServices {
    recover_custody: Arc<dyn RecoverPortableWalletBackupUseCase>,
    export_complete: Arc<dyn ExportCompleteWalletBackupUseCase>,
    recover_complete: Arc<dyn RecoverCompleteWalletBackupUseCase>,
    documents: Arc<dyn PortableWalletBackupDocumentPort>,
}

impl WalletBackupUiServices {
    #[must_use]
    pub const fn new(
        recover_custody: Arc<dyn RecoverPortableWalletBackupUseCase>,
        export_complete: Arc<dyn ExportCompleteWalletBackupUseCase>,
        recover_complete: Arc<dyn RecoverCompleteWalletBackupUseCase>,
        documents: Arc<dyn PortableWalletBackupDocumentPort>,
    ) -> Self {
        Self {
            recover_custody,
            export_complete,
            recover_complete,
            documents,
        }
    }
}

impl WalletSecurityUiServices {
    #[must_use]
    pub const fn new(
        get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase>,
        initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase>,
        unlock_wallet: Arc<dyn UnlockWalletUseCase>,
        lock_wallet: Arc<dyn LockWalletUseCase>,
        backup: WalletBackupUiServices,
    ) -> Self {
        Self {
            get_wallet_security_status,
            initialize_wallet_security,
            unlock_wallet,
            lock_wallet,
            backup,
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
    public_text_exporter: Arc<dyn PublicTextExportPort>,
}

impl WalletAccountUiServices {
    #[must_use]
    pub const fn new(
        list_wallet_networks: Arc<dyn ListWalletNetworksUseCase>,
        select_wallet_network: Arc<dyn SelectWalletNetworkUseCase>,
        derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase>,
        get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
        sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
        public_text_exporter: Arc<dyn PublicTextExportPort>,
    ) -> Self {
        Self {
            list_wallet_networks,
            select_wallet_network,
            derive_wallet_account,
            get_wallet_account,
            sync_wallet_account,
            public_text_exporter,
        }
    }
}

/// Key-scoped DUST synchronization use cases consumed by the Assets page.
pub struct WalletDustSyncUiServices {
    get_wallet_dust_sync_status: Arc<dyn GetWalletDustSyncStatusUseCase>,
    start_wallet_dust_sync: Arc<dyn StartWalletDustSyncUseCase>,
    cancel_wallet_dust_sync: Arc<dyn CancelWalletDustSyncUseCase>,
}

impl WalletDustSyncUiServices {
    #[must_use]
    pub const fn new(
        get_wallet_dust_sync_status: Arc<dyn GetWalletDustSyncStatusUseCase>,
        start_wallet_dust_sync: Arc<dyn StartWalletDustSyncUseCase>,
        cancel_wallet_dust_sync: Arc<dyn CancelWalletDustSyncUseCase>,
    ) -> Self {
        Self {
            get_wallet_dust_sync_status,
            start_wallet_dust_sync,
            cancel_wallet_dust_sync,
        }
    }
}

/// Shielded synchronization use cases consumed by the Assets page.
pub struct WalletShieldedSyncUiServices {
    get_wallet_shielded_sync_status: Arc<dyn GetWalletShieldedSyncStatusUseCase>,
    start_wallet_shielded_sync: Arc<dyn StartWalletShieldedSyncUseCase>,
    cancel_wallet_shielded_sync: Arc<dyn CancelWalletShieldedSyncUseCase>,
}

impl WalletShieldedSyncUiServices {
    #[must_use]
    pub const fn new(
        get_wallet_shielded_sync_status: Arc<dyn GetWalletShieldedSyncStatusUseCase>,
        start_wallet_shielded_sync: Arc<dyn StartWalletShieldedSyncUseCase>,
        cancel_wallet_shielded_sync: Arc<dyn CancelWalletShieldedSyncUseCase>,
    ) -> Self {
        Self {
            get_wallet_shielded_sync_status,
            start_wallet_shielded_sync,
            cancel_wallet_shielded_sync,
        }
    }
}

/// Transaction use cases consumed by the Assets page.
pub struct WalletTransactionUiServices {
    prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
    prepare_shielded_wallet_transfer: Arc<dyn PrepareShieldedWalletTransferUseCase>,
    authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
    submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
    get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
    get_wallet_transfer_submission_status: Arc<dyn GetWalletTransferSubmissionStatusUseCase>,
    cancel_wallet_transfer_submission: Arc<dyn CancelWalletTransferSubmissionUseCase>,
    list_wallet_transfer_submissions: Arc<dyn ListWalletTransferSubmissionsUseCase>,
    reconcile_wallet_transfer_submission: Arc<dyn ReconcileWalletTransferSubmissionUseCase>,
}

/// Public and protected transfer preparation use cases consumed by the Assets page.
pub struct WalletTransactionPreparationUiServices {
    prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
    prepare_shielded_wallet_transfer: Arc<dyn PrepareShieldedWalletTransferUseCase>,
}

impl WalletTransactionPreparationUiServices {
    #[must_use]
    pub const fn new(
        prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
        prepare_shielded_wallet_transfer: Arc<dyn PrepareShieldedWalletTransferUseCase>,
    ) -> Self {
        Self {
            prepare_wallet_transfer,
            prepare_shielded_wallet_transfer,
        }
    }
}

/// Public submission recovery use cases consumed by the Assets page.
pub struct WalletTransactionRecoveryUiServices {
    list_wallet_transfer_submissions: Arc<dyn ListWalletTransferSubmissionsUseCase>,
    reconcile_wallet_transfer_submission: Arc<dyn ReconcileWalletTransferSubmissionUseCase>,
}

impl WalletTransactionRecoveryUiServices {
    #[must_use]
    pub const fn new(
        list_wallet_transfer_submissions: Arc<dyn ListWalletTransferSubmissionsUseCase>,
        reconcile_wallet_transfer_submission: Arc<dyn ReconcileWalletTransferSubmissionUseCase>,
    ) -> Self {
        Self {
            list_wallet_transfer_submissions,
            reconcile_wallet_transfer_submission,
        }
    }
}

impl WalletTransactionUiServices {
    #[must_use]
    pub fn new(
        preparation: WalletTransactionPreparationUiServices,
        authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
        submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
        get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
        get_wallet_transfer_submission_status: Arc<dyn GetWalletTransferSubmissionStatusUseCase>,
        cancel_wallet_transfer_submission: Arc<dyn CancelWalletTransferSubmissionUseCase>,
        recovery: WalletTransactionRecoveryUiServices,
    ) -> Self {
        Self {
            prepare_wallet_transfer: preparation.prepare_wallet_transfer,
            prepare_shielded_wallet_transfer: preparation.prepare_shielded_wallet_transfer,
            authorize_wallet_transfer,
            submit_wallet_transfer,
            get_wallet_transfer_draft,
            get_wallet_transfer_submission_status,
            cancel_wallet_transfer_submission,
            list_wallet_transfer_submissions: recovery.list_wallet_transfer_submissions,
            reconcile_wallet_transfer_submission: recovery.reconcile_wallet_transfer_submission,
        }
    }
}

impl WalletUiServices {
    #[must_use]
    pub fn new(
        profiles: WalletProfileUiServices,
        security: WalletSecurityUiServices,
        account: WalletAccountUiServices,
        operations: WalletOperationalUiServices,
        identity: IdentityUiServices,
        diagnostics: DiagnosticsUiServices,
    ) -> Self {
        let dust = operations.dust;
        let shielded = operations.shielded;
        let transactions = operations.transactions;
        let vault = operations.vault;
        let dids = identity.dids;
        let credentials = identity.credentials;
        let authentication = identity.authentication;
        let ingress = identity.ingress;
        Self {
            get_diagnostic_snapshot: diagnostics.get,
            clear_diagnostics: diagnostics.clear,
            qr_scanner: ingress.qr_scanner,
            identity_link_ingress: ingress.app_links,
            public_text_exporter: account.public_text_exporter,
            portable_wallet_backup_documents: security.backup.documents,
            route_identity_request: ingress.route,
            create_wallet_profile: profiles.create_wallet_profile,
            list_wallet_profiles: profiles.list_wallet_profiles,
            select_wallet_profile: profiles.select_wallet_profile,
            get_active_wallet_profile: profiles.get_active_wallet_profile,
            get_wallet_security_status: security.get_wallet_security_status,
            initialize_wallet_security: security.initialize_wallet_security,
            unlock_wallet: security.unlock_wallet,
            lock_wallet: security.lock_wallet,
            recover_portable_wallet_backup: security.backup.recover_custody,
            export_complete_wallet_backup: security.backup.export_complete,
            recover_complete_wallet_backup: security.backup.recover_complete,
            list_wallet_networks: account.list_wallet_networks,
            select_wallet_network: account.select_wallet_network,
            derive_wallet_account: account.derive_wallet_account,
            get_wallet_account: account.get_wallet_account,
            sync_wallet_account: account.sync_wallet_account,
            get_wallet_dust_sync_status: dust.get_wallet_dust_sync_status,
            start_wallet_dust_sync: dust.start_wallet_dust_sync,
            cancel_wallet_dust_sync: dust.cancel_wallet_dust_sync,
            get_wallet_shielded_sync_status: shielded.get_wallet_shielded_sync_status,
            start_wallet_shielded_sync: shielded.start_wallet_shielded_sync,
            cancel_wallet_shielded_sync: shielded.cancel_wallet_shielded_sync,
            prepare_wallet_transfer: transactions.prepare_wallet_transfer,
            prepare_shielded_wallet_transfer: transactions.prepare_shielded_wallet_transfer,
            authorize_wallet_transfer: transactions.authorize_wallet_transfer,
            submit_wallet_transfer: transactions.submit_wallet_transfer,
            get_wallet_transfer_draft: transactions.get_wallet_transfer_draft,
            get_wallet_transfer_submission_status: transactions
                .get_wallet_transfer_submission_status,
            cancel_wallet_transfer_submission: transactions.cancel_wallet_transfer_submission,
            list_wallet_transfer_submissions: transactions.list_wallet_transfer_submissions,
            reconcile_wallet_transfer_submission: transactions.reconcile_wallet_transfer_submission,
            create_did: dids.create_did,
            resolve_did: dids.resolve_did,
            list_did_records: dids.list_did_records,
            update_did: dids.update_did,
            deactivate_did: dids.deactivate_did,
            sign_did_payload: dids.sign_did_payload,
            forget_did: dids.forget_did,
            receive_credential: credentials.receive_credential,
            list_credentials: credentials.list_credentials,
            get_credential: credentials.get_credential,
            reverify_credential: credentials.reverify_credential,
            delete_credential: credentials.delete_credential,
            get_credential_disclosure: credentials.get_credential_disclosure,
            preview_credential_disclosure: credentials.preview_credential_disclosure,
            reveal_credential_claim: credentials.reveal_credential_claim,
            prepare_credential_issuance: credentials.prepare_credential_issuance,
            accept_credential_issuance: credentials.accept_credential_issuance,
            refuse_credential_issuance: credentials.refuse_credential_issuance,
            standalone_credential_offer: credentials.standalone_credential_offer,
            prepare_credential_presentation: credentials.prepare_credential_presentation,
            accept_credential_presentation: credentials.accept_credential_presentation,
            refuse_credential_presentation: credentials.refuse_credential_presentation,
            standalone_openid4vp_request: credentials.standalone_openid4vp_request,
            prepare_self_issued_authentication: authentication.prepare,
            accept_self_issued_authentication: authentication.accept,
            refuse_self_issued_authentication: authentication.refuse,
            standalone_self_issued_request: authentication.standalone_request,
            list_passport_vault_locks: vault.list,
            create_passport_vault_lock: vault.create,
            deposit_passport_vault_lock: vault.deposit,
            claim_passport_vault_lock: vault.claim,
            withdraw_passport_vault_lock: vault.withdraw,
            passport_vault_state_persistence: vault.state_persistence,
            passport_vault_contract_calls: vault.contract_calls,
        }
    }

    #[must_use]
    pub fn get_diagnostic_snapshot(&self) -> Arc<dyn GetDiagnosticSnapshotUseCase> {
        Arc::clone(&self.get_diagnostic_snapshot)
    }

    #[must_use]
    pub fn clear_diagnostics(&self) -> Arc<dyn ClearDiagnosticsUseCase> {
        Arc::clone(&self.clear_diagnostics)
    }

    #[must_use]
    pub fn qr_scanner(&self) -> Arc<dyn QrScannerPort> {
        Arc::clone(&self.qr_scanner)
    }

    #[must_use]
    pub fn identity_link_ingress(&self) -> Arc<dyn IdentityLinkIngressPort> {
        Arc::clone(&self.identity_link_ingress)
    }

    #[must_use]
    pub fn public_text_exporter(&self) -> Arc<dyn PublicTextExportPort> {
        Arc::clone(&self.public_text_exporter)
    }

    #[must_use]
    pub fn route_identity_request(&self) -> Arc<dyn RouteIdentityRequestUseCase> {
        Arc::clone(&self.route_identity_request)
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
    pub fn get_wallet_dust_sync_status(&self) -> Arc<dyn GetWalletDustSyncStatusUseCase> {
        Arc::clone(&self.get_wallet_dust_sync_status)
    }

    #[must_use]
    pub fn start_wallet_dust_sync(&self) -> Arc<dyn StartWalletDustSyncUseCase> {
        Arc::clone(&self.start_wallet_dust_sync)
    }

    #[must_use]
    pub fn cancel_wallet_dust_sync(&self) -> Arc<dyn CancelWalletDustSyncUseCase> {
        Arc::clone(&self.cancel_wallet_dust_sync)
    }

    #[must_use]
    pub fn get_wallet_shielded_sync_status(&self) -> Arc<dyn GetWalletShieldedSyncStatusUseCase> {
        Arc::clone(&self.get_wallet_shielded_sync_status)
    }

    #[must_use]
    pub fn start_wallet_shielded_sync(&self) -> Arc<dyn StartWalletShieldedSyncUseCase> {
        Arc::clone(&self.start_wallet_shielded_sync)
    }

    #[must_use]
    pub fn cancel_wallet_shielded_sync(&self) -> Arc<dyn CancelWalletShieldedSyncUseCase> {
        Arc::clone(&self.cancel_wallet_shielded_sync)
    }

    #[must_use]
    pub fn prepare_wallet_transfer(&self) -> Arc<dyn PrepareWalletTransferUseCase> {
        Arc::clone(&self.prepare_wallet_transfer)
    }

    #[must_use]
    pub fn prepare_shielded_wallet_transfer(
        &self,
    ) -> Arc<dyn PrepareShieldedWalletTransferUseCase> {
        Arc::clone(&self.prepare_shielded_wallet_transfer)
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

    #[must_use]
    pub fn get_wallet_transfer_submission_status(
        &self,
    ) -> Arc<dyn GetWalletTransferSubmissionStatusUseCase> {
        Arc::clone(&self.get_wallet_transfer_submission_status)
    }

    #[must_use]
    pub fn cancel_wallet_transfer_submission(
        &self,
    ) -> Arc<dyn CancelWalletTransferSubmissionUseCase> {
        Arc::clone(&self.cancel_wallet_transfer_submission)
    }

    #[must_use]
    pub fn list_wallet_transfer_submissions(
        &self,
    ) -> Arc<dyn ListWalletTransferSubmissionsUseCase> {
        Arc::clone(&self.list_wallet_transfer_submissions)
    }

    #[must_use]
    pub fn reconcile_wallet_transfer_submission(
        &self,
    ) -> Arc<dyn ReconcileWalletTransferSubmissionUseCase> {
        Arc::clone(&self.reconcile_wallet_transfer_submission)
    }

    #[must_use]
    pub fn resolve_did(&self) -> Arc<dyn ResolveDidUseCase> {
        Arc::clone(&self.resolve_did)
    }

    #[must_use]
    pub fn create_did(&self) -> Arc<dyn CreateDidUseCase> {
        Arc::clone(&self.create_did)
    }

    #[must_use]
    pub fn list_did_records(&self) -> Arc<dyn ListDidRecordsUseCase> {
        Arc::clone(&self.list_did_records)
    }

    #[must_use]
    pub fn update_did(&self) -> Arc<dyn UpdateDidUseCase> {
        Arc::clone(&self.update_did)
    }

    #[must_use]
    pub fn deactivate_did(&self) -> Arc<dyn DeactivateDidUseCase> {
        Arc::clone(&self.deactivate_did)
    }

    #[must_use]
    pub fn sign_did_payload(&self) -> Arc<dyn SignDidPayloadUseCase> {
        Arc::clone(&self.sign_did_payload)
    }

    #[must_use]
    pub fn forget_did(&self) -> Arc<dyn ForgetDidUseCase> {
        Arc::clone(&self.forget_did)
    }

    #[must_use]
    pub fn receive_credential(&self) -> Arc<dyn ReceiveCredentialUseCase> {
        Arc::clone(&self.receive_credential)
    }

    #[must_use]
    pub fn list_credentials(&self) -> Arc<dyn ListCredentialsUseCase> {
        Arc::clone(&self.list_credentials)
    }

    #[must_use]
    pub fn get_credential(&self) -> Arc<dyn GetCredentialUseCase> {
        Arc::clone(&self.get_credential)
    }

    #[must_use]
    pub fn reverify_credential(&self) -> Arc<dyn ReverifyCredentialUseCase> {
        Arc::clone(&self.reverify_credential)
    }

    #[must_use]
    pub fn delete_credential(&self) -> Arc<dyn DeleteCredentialUseCase> {
        Arc::clone(&self.delete_credential)
    }

    #[must_use]
    pub fn get_credential_disclosure(&self) -> Arc<dyn GetCredentialDisclosureUseCase> {
        Arc::clone(&self.get_credential_disclosure)
    }

    #[must_use]
    pub fn preview_credential_disclosure(&self) -> Arc<dyn PreviewCredentialDisclosureUseCase> {
        Arc::clone(&self.preview_credential_disclosure)
    }

    #[must_use]
    pub fn reveal_credential_claim(&self) -> Arc<dyn RevealCredentialClaimUseCase> {
        Arc::clone(&self.reveal_credential_claim)
    }

    #[must_use]
    pub fn prepare_credential_issuance(&self) -> Arc<dyn PrepareCredentialIssuanceUseCase> {
        Arc::clone(&self.prepare_credential_issuance)
    }

    #[must_use]
    pub fn accept_credential_issuance(&self) -> Arc<dyn AcceptCredentialIssuanceUseCase> {
        Arc::clone(&self.accept_credential_issuance)
    }

    #[must_use]
    pub fn refuse_credential_issuance(&self) -> Arc<dyn RefuseCredentialIssuanceUseCase> {
        Arc::clone(&self.refuse_credential_issuance)
    }

    #[must_use]
    pub fn standalone_credential_offer(&self) -> Option<String> {
        self.standalone_credential_offer.clone()
    }

    #[must_use]
    pub fn prepare_credential_presentation(&self) -> Arc<dyn PrepareCredentialPresentationUseCase> {
        Arc::clone(&self.prepare_credential_presentation)
    }

    #[must_use]
    pub fn accept_credential_presentation(&self) -> Arc<dyn AcceptCredentialPresentationUseCase> {
        Arc::clone(&self.accept_credential_presentation)
    }

    #[must_use]
    pub fn refuse_credential_presentation(&self) -> Arc<dyn RefuseCredentialPresentationUseCase> {
        Arc::clone(&self.refuse_credential_presentation)
    }

    #[must_use]
    pub fn standalone_openid4vp_request(&self) -> Option<String> {
        self.standalone_openid4vp_request.clone()
    }

    #[must_use]
    pub fn prepare_self_issued_authentication(
        &self,
    ) -> Arc<dyn PrepareSelfIssuedAuthenticationUseCase> {
        Arc::clone(&self.prepare_self_issued_authentication)
    }

    #[must_use]
    pub fn accept_self_issued_authentication(
        &self,
    ) -> Arc<dyn AcceptSelfIssuedAuthenticationUseCase> {
        Arc::clone(&self.accept_self_issued_authentication)
    }

    #[must_use]
    pub fn refuse_self_issued_authentication(
        &self,
    ) -> Arc<dyn RefuseSelfIssuedAuthenticationUseCase> {
        Arc::clone(&self.refuse_self_issued_authentication)
    }

    #[must_use]
    pub fn standalone_self_issued_request(&self) -> Option<String> {
        self.standalone_self_issued_request.clone()
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Destination {
    Assets,
    Vault,
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
            Self::Vault => "Vault",
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
            Self::Vault => LUCIDE_LANDMARK,
            Self::Dids => LUCIDE_FINGERPRINT,
            Self::Credentials => LUCIDE_BADGE_CHECK,
            Self::Diagnostics => LUCIDE_ACTIVITY,
            Self::Settings | Self::Profile => LUCIDE_SETTINGS_2,
        }
    }
}

const PRIMARY_DESTINATIONS: [Destination; 6] = [
    Destination::Assets,
    Destination::Vault,
    Destination::Dids,
    Destination::Credentials,
    Destination::Diagnostics,
    Destination::Settings,
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum CreationState {
    Idle,
    Working,
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
enum PortableBackupUiState {
    Idle,
    Working(&'static str),
    Succeeded(String),
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DidPageState {
    Loading,
    Ready {
        records: Vec<DidRecordView>,
        resolving: bool,
        operation_error: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CredentialPageState {
    Loading,
    Ready {
        credentials: Vec<CredentialView>,
        receiving: bool,
        operation_error: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, PartialEq, Eq)]
struct PendingIdentityRequest {
    kind: IdentityRequestKind,
    request_uri: String,
}

fn identity_request_destination(kind: IdentityRequestKind) -> Destination {
    match kind {
        IdentityRequestKind::SelfIssuedAuthentication => Destination::Dids,
        IdentityRequestKind::CredentialIssuance | IdentityRequestKind::CredentialPresentation => {
            Destination::Credentials
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountOperation {
    Initializing,
    Unlocking,
    Deriving,
    Syncing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DustSyncPaneState {
    Loading,
    Ready {
        status: WalletDustSyncView,
        action_busy: bool,
        operation_error: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ShieldedSyncPaneState {
    Loading,
    Ready {
        status: WalletShieldedSyncView,
        action_busy: bool,
        operation_error: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SubmissionRecoveryPaneState {
    Loading,
    Ready {
        latest: Option<Box<WalletTransferSubmissionStatusView>>,
        reconciling: bool,
        operation_error: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TransferPanelState {
    Editing,
    Preparing,
    Prepared(Box<WalletTransferPreviewView>),
    Authorizing(Box<WalletTransferPreviewView>),
    Authorized(Box<WalletTransferPreviewView>),
    Submitting(Box<WalletTransferPreviewView>),
    Cancelling(Box<WalletTransferPreviewView>),
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
    let mut pending_identity_request = use_signal(|| None::<PendingIdentityRequest>);
    let mut identity_ingress_notice = use_signal(|| None::<String>);
    let mut identity_scan_busy = use_signal(|| false);
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let mut identity_link_wake = use_signal(|| 0_u64);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let identity_link_wake = use_signal(|| 0_u64);
    let services_for_load = services.clone();
    use_effect(move || {
        let services = services_for_load.clone();
        spawn(async move {
            profile_session.set(
                run_ui_blocking(move || load_profile_session(&services))
                    .await
                    .unwrap_or_else(|error| ProfileSessionState::Failed(error.to_string())),
            );
        });
    });

    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        dioxus::mobile::use_wry_event_handler(move |event, _target| match event {
            dioxus::mobile::tao::event::Event::Opened { .. } => {
                identity_link_wake.set(identity_link_wake().wrapping_add(1));
            }
            dioxus::mobile::tao::event::Event::Resumed => {
                identity_link_wake.set(identity_link_wake().wrapping_add(1));
            }
            _ => {}
        });
    }

    let services_for_links = services.clone();
    use_effect(move || {
        let _wake = identity_link_wake();
        if matches!(*profile_session.read(), ProfileSessionState::Active(_)) {
            route_pending_identity_link(
                &services_for_links,
                pending_identity_request,
                active_destination,
                menu_open,
                identity_ingress_notice,
            );
        }
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
                    let services = services.clone();
                    profile_session.set(ProfileSessionState::Loading);
                    spawn(async move {
                        profile_session.set(
                            run_ui_blocking(move || load_profile_session(&services))
                                .await
                                .unwrap_or_else(|error| {
                                    ProfileSessionState::Failed(error.to_string())
                                }),
                        );
                    });
                },
            }
        };
    };

    let active = *active_destination.read();
    let profile_monogram = profile_monogram(&active_profile.display_name);
    let identity_request_waiting = pending_identity_request.read().is_some();

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
                        class: "scan-shortcut",
                        r#type: "button",
                        aria_label: "Scan identity QR code",
                        title: "Scan identity QR code",
                        disabled: identity_scan_busy(),
                        onclick: {
                            let scanner = services.qr_scanner();
                            let router = services.route_identity_request();
                            move |_| {
                                let scanner = scanner.clone();
                                let router = router.clone();
                                identity_scan_busy.set(true);
                                identity_ingress_notice.set(None);
                                spawn(async move {
                                    match scanner.scan().await {
                                        Ok(payload) => {
                                            let request_uri = payload.into_inner();
                                            match router.execute(RouteIdentityRequestCommand {
                                                request_uri: request_uri.clone(),
                                            }) {
                                                Ok(kind) => {
                                                    pending_identity_request.set(Some(PendingIdentityRequest {
                                                        kind,
                                                        request_uri,
                                                    }));
                                                    active_destination.set(identity_request_destination(kind));
                                                    menu_open.set(false);
                                                    identity_ingress_notice.set(Some(format!(
                                                        "QR recognized as {}. Review the request before consent.",
                                                        identity_request_kind_label(kind)
                                                    )));
                                                }
                                                Err(error) => {
                                                    identity_ingress_notice.set(Some(identity_request_routing_message(error)));
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            identity_ingress_notice.set(Some(qr_scan_message(error)));
                                        }
                                    }
                                    identity_scan_busy.set(false);
                                });
                            }
                        },
                        if identity_scan_busy() { "Scanning…" } else { "Scan QR" }
                    }
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

            if let Some(message) = identity_ingress_notice.read().as_deref() {
                div { class: "identity-ingress-notice", role: "status",
                    "{message}"
                    if identity_request_waiting {
                        button {
                            class: "identity-ingress-dismiss",
                            r#type: "button",
                            onclick: move |_| {
                                pending_identity_request.set(None);
                                identity_ingress_notice.set(Some(
                                    "Identity request dismissed without consent.".to_owned(),
                                ));
                            },
                            "Dismiss identity request"
                        }
                    }
                }
            }

            if *menu_open.read() {
                nav { class: "menu-dropdown", aria_label: "All wallet destinations",
                    for destination in [
                        Destination::Assets,
                        Destination::Vault,
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
                    Destination::Vault => rsx! { PassportVaultPage { active_profile: active_profile.clone() } },
                    Destination::Dids => rsx! {
                        DidsPage {
                            active_profile: active_profile.clone(),
                            pending_identity_request,
                        }
                    },
                    Destination::Credentials => rsx! {
                        CredentialsPage {
                            active_profile: active_profile.clone(),
                            pending_identity_request,
                        }
                    },
                    Destination::Diagnostics => rsx! { DiagnosticsPage { active_profile: active_profile.clone() } },
                    Destination::Settings => rsx! {
                        SettingsPage {
                            active_profile: active_profile.clone(),
                            lifecycle_wake: identity_link_wake,
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
                p { "Create a new wallet or recover one complete encrypted Oxid backup." }
            }
            ProfileManager {
                profiles: Vec::new(),
                active_profile_id: None,
                onboarding: true,
                on_selected,
            }
            FreshInstallRecovery {
                on_recovered: move |profile| on_selected.call(profile),
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
fn FreshInstallRecovery(on_recovered: EventHandler<WalletProfileView>) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut recovery_secret = use_signal(|| Zeroizing::new(String::new()));
    let mut recovery_confirmed = use_signal(|| false);
    let mut recovery_state = use_signal(|| PortableBackupUiState::Idle);
    let busy = matches!(*recovery_state.read(), PortableBackupUiState::Working(_));
    let can_recover = !busy && recovery_confirmed() && !recovery_secret.read().is_empty();
    let feedback = match recovery_state.read().clone() {
        PortableBackupUiState::Idle => rsx! {},
        PortableBackupUiState::Working(message) => rsx! {
            div { class: "result", role: "status", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                p { "{message}" }
            }
        },
        PortableBackupUiState::Succeeded(message) => rsx! {
            div { class: "result success", role: "status", p { "{message}" } }
        },
        PortableBackupUiState::Cancelled => rsx! {
            div { class: "result", role: "status",
                p { "Document selection cancelled. No recovery was started." }
            }
        },
        PortableBackupUiState::Failed(message) => rsx! {
            div { class: "result error", role: "alert", p { "{message}" } }
        },
    };

    rsx! {
        section { class: "profile-card surface-card complete-recovery-card",
            p { class: "card-eyebrow", "Existing wallet" }
            h2 { "Restore your complete wallet" }
            p {
                "Choose an encrypted Oxid complete-wallet backup. The profile, Midnight account associations, DID records, credentials, and protected keys are authenticated before this empty installation becomes active."
            }
            p { class: "backup-warning",
                strong { "Empty-install recovery only. " }
                "Oxid never merges this archive into existing local wallet state. Chain-derived caches and transaction history rebuild from their authoritative sources."
            }
            label { r#for: "onboarding-recovery-secret", "Recovery secret"
                input {
                    id: "onboarding-recovery-secret",
                    r#type: "password",
                    minlength: 12,
                    maxlength: MAX_WALLET_RECOVERY_SECRET_CHARACTERS,
                    autocomplete: "current-password",
                    spellcheck: false,
                    disabled: busy,
                    value: recovery_secret.read().as_str(),
                    oninput: move |event| recovery_secret.set(Zeroizing::new(event.value())),
                }
            }
            label { class: "confirmation-row",
                input {
                    r#type: "checkbox",
                    checked: recovery_confirmed(),
                    disabled: busy,
                    onchange: move |event| recovery_confirmed.set(event.checked()),
                }
                "I confirm complete recovery into this empty Oxid installation."
            }
            button {
                class: "secondary-action",
                r#type: "button",
                aria_label: "Choose complete wallet backup and recover",
                disabled: !can_recover,
                onclick: move |_| {
                    let raw = recovery_secret();
                    recovery_secret.set(Zeroizing::new(String::new()));
                    recovery_confirmed.set(false);
                    let secret = match WalletRecoverySecret::parse(&*raw) {
                        Ok(secret) => secret,
                        Err(error) => {
                            recovery_state.set(PortableBackupUiState::Failed(error.to_string()));
                            return;
                        }
                    };
                    let services = services.clone();
                    recovery_state.set(PortableBackupUiState::Working(
                        "Waiting for a complete wallet backup",
                    ));
                    spawn(async move {
                        let imported = services.portable_wallet_backup_documents.import().await;
                        let recovered = match imported {
                            Ok(backup) => {
                                let services = services.clone();
                                match run_ui_blocking(move || {
                                    let summary = services
                                        .recover_complete_wallet_backup
                                        .execute(RecoverCompleteWalletBackupCommand {
                                            expected_profile_id: None,
                                            backup,
                                            recovery_secret: secret,
                                            confirmation: SensitiveOperationConfirmation {
                                                title: RECOVER_COMPLETE_WALLET_BACKUP_TITLE
                                                    .to_owned(),
                                                summary: RECOVER_COMPLETE_WALLET_BACKUP_SUMMARY
                                                    .to_owned(),
                                                confirmed: true,
                                            },
                                        })
                                        .map_err(|error| error.to_string())?;
                                    let active_profile = services
                                        .get_active_wallet_profile
                                        .execute()
                                        .map_err(|error| error.to_string())?;
                                    Ok::<_, String>((summary, active_profile))
                                })
                                .await
                                {
                                    Ok(result) => result,
                                    Err(error) => Err(error.to_string()),
                                }
                            }
                            Err(PortableWalletBackupDocumentError::Cancelled) => {
                                recovery_state.set(PortableBackupUiState::Cancelled);
                                return;
                            }
                            Err(error) => Err(error.to_string()),
                        };
                        match recovered {
                            Ok((summary, active_profile)) => match active_profile {
                                Some(profile) if profile.id == summary.profile_id => {
                                    recovery_state.set(PortableBackupUiState::Succeeded(
                                        complete_recovery_message(&summary),
                                    ));
                                    on_recovered.call(profile);
                                }
                                _ => recovery_state.set(PortableBackupUiState::Failed(
                                    "Recovered wallet did not become the active profile.".to_owned(),
                                )),
                            },
                            Err(error) => recovery_state.set(PortableBackupUiState::Failed(error)),
                        }
                    });
                },
                "Choose backup and recover"
            }
            {feedback}
        }
    }
}

fn complete_recovery_message(summary: &CompleteWalletRecoverySummary) -> String {
    format!(
        "Recovered {} protected key(s), {} DID record(s), and {} credential(s).",
        summary.restored_key_count, summary.restored_did_count, summary.restored_credential_count,
    )
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
    let busy = matches!(*state.read(), CreationState::Working);
    let can_submit = !busy && !display_name.read().trim().is_empty();

    let feedback = match state.read().clone() {
        CreationState::Idle => rsx! {
            p { class: "form-hint", "Only public profile metadata is stored here. Protected key operations remain a separate capability." }
        },
        CreationState::Working => rsx! {
            section { class: "result", role: "status", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                p { "Updating the private profile store…" }
            }
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
                                        disabled: busy,
                                        onclick: move |_| {
                                            let select = Arc::clone(&select);
                                            let profile_id = profile_id.clone();
                                            state.set(CreationState::Working);
                                            spawn(async move {
                                                let selected = run_ui_blocking(move || {
                                                    select.execute(SelectWalletProfileCommand {
                                                        profile_id,
                                                    })
                                                })
                                                .await;
                                                match selected {
                                                    Ok(Ok(selected)) => on_selected.call(selected),
                                                    Ok(Err(error)) => state.set(
                                                        CreationState::Failed(error.to_string()),
                                                    ),
                                                    Err(error) => state.set(
                                                        CreationState::Failed(error.to_string()),
                                                    ),
                                                }
                                            });
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
                    let create = Arc::clone(&create_for_button);
                    let select = Arc::clone(&select_for_button);
                    state.set(CreationState::Working);
                    spawn(async move {
                        let result = run_ui_blocking(move || {
                            let created = create
                                .execute(command)
                                .map_err(|error| error.to_string())?;
                            let selected = select
                                .execute(SelectWalletProfileCommand {
                                    profile_id: created.id.clone(),
                                })
                                .map_err(|error| error.to_string())?;
                            Ok::<_, String>((created, selected))
                        })
                        .await;
                        match result {
                            Ok(Ok((created, selected))) => {
                                profile_list.write().push(created);
                                state.set(CreationState::Created(selected.clone()));
                                on_selected.call(selected);
                            }
                            Ok(Err(error)) => state.set(CreationState::Failed(error)),
                            Err(error) => {
                                state.set(CreationState::Failed(error.to_string()));
                            }
                        }
                    });
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
                                            busy: None,
                                        });
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
                                            let service = services.sync_wallet_account();
                                            activate_state.set(AccountPageState::Ready {
                                                networks: networks.clone(),
                                                account: account.clone(),
                                                security: updated_security,
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
                            match run_ui_future(async move {
                                service.execute(WalletAccountQuery { profile_id }).await
                            })
                            .await
                            {
                                Ok(Ok(account)) => sync_state.set(AccountPageState::Ready {
                                    networks,
                                    account: Box::new(account),
                                    security: sync_security,
                                    busy: None,
                                }),
                                Ok(Err(error)) => sync_state.set(AccountPageState::Failed(error.to_string())),
                                Err(error) => sync_state.set(AccountPageState::Failed(error.to_string())),
                            }
                        });
                    },
                    "{sync_label}"
                }

                DustSyncPane {
                    profile_id: active_profile.id.clone(),
                    can_sync: protection_unlocked,
                }

                ShieldedSyncPane {
                    profile_id: active_profile.id.clone(),
                    can_sync: protection_unlocked,
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
                            p { "Each QR, clipboard copy, and share sheet contains exactly the public receive address shown." }
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

                SubmissionRecoveryPane { profile_id: active_profile.id.clone() }

                if protected_account && protection_unlocked && account.sync.state == "synced" {
                    if let (Some(unshielded), Some(shielded)) = (
                        account.addresses.iter().find(|address| address.kind == "unshielded"),
                        account.addresses.iter().find(|address| address.kind == "shielded"),
                    ) {
                        SendTransferPanel {
                            profile_id: active_profile.id.clone(),
                            unshielded_receive_address: unshielded.value.clone(),
                            shielded_receive_address: shielded.value.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SubmissionRecoveryPane(profile_id: String) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| SubmissionRecoveryPaneState::Loading);
    let load_services = services.clone();
    let load_profile = profile_id.clone();
    use_effect(move || {
        let service = load_services.list_wallet_transfer_submissions();
        let profile_id = load_profile.clone();
        spawn(async move {
            let result = run_ui_blocking(move || service.execute(profile_id)).await;
            state.set(match result {
                Ok(Ok(submissions)) => SubmissionRecoveryPaneState::Ready {
                    latest: submissions.into_iter().next().map(Box::new),
                    reconciling: false,
                    operation_error: None,
                },
                Ok(Err(error)) => SubmissionRecoveryPaneState::Failed(error.to_string()),
                Err(error) => SubmissionRecoveryPaneState::Failed(error.to_string()),
            });
        });
    });

    match state.read().clone() {
        SubmissionRecoveryPaneState::Loading => rsx! {},
        SubmissionRecoveryPaneState::Failed(error) => rsx! {
            article { class: "surface-card submission-recovery-card", role: "alert",
                p { class: "card-eyebrow", "Transaction recovery" }
                h2 { "Submission history unavailable" }
                p { "{error}" }
                button {
                    class: "secondary-action",
                    r#type: "button",
                    onclick: move |_| {
                        let service = services.list_wallet_transfer_submissions();
                        let profile_id = profile_id.clone();
                        state.set(SubmissionRecoveryPaneState::Loading);
                        spawn(async move {
                            let result = run_ui_blocking(move || service.execute(profile_id)).await;
                            state.set(match result {
                                Ok(Ok(submissions)) => SubmissionRecoveryPaneState::Ready {
                                    latest: submissions.into_iter().next().map(Box::new),
                                    reconciling: false,
                                    operation_error: None,
                                },
                                Ok(Err(error)) => SubmissionRecoveryPaneState::Failed(error.to_string()),
                                Err(error) => SubmissionRecoveryPaneState::Failed(error.to_string()),
                            });
                        });
                    },
                    "Retry"
                }
            }
        },
        SubmissionRecoveryPaneState::Ready { latest: None, .. } => rsx! {},
        SubmissionRecoveryPaneState::Ready {
            latest: Some(submission),
            reconciling,
            operation_error,
        } => {
            let draft_id = submission.draft_id.clone();
            let current = submission.clone();
            let reconcile_services = services.clone();
            let reconcile_profile = profile_id.clone();
            rsx! {
                article {
                    class: "surface-card submission-recovery-card",
                    role: "status",
                    aria_live: "polite",
                    aria_busy: if reconciling { "true" } else { "false" },
                    p { class: "card-eyebrow", "Latest transaction" }
                    h2 { "{submission_status_heading(&submission.state)}" }
                    p { "{submission_status_note(&submission.state)}" }
                    dl { class: "preview-list",
                        div { dt { "State" } dd { "{submission_status_label(&submission.state)}" } }
                        if let Some(mode) = submission.mode.as_deref() {
                            div { dt { "Mode" } dd { "{mode}" } }
                        }
                        if let Some(transaction_id) = submission.transaction_id.as_deref() {
                            div { dt { "Transaction" } dd { title: "{transaction_id}", "{truncate_middle(transaction_id, 16, 8)}" } }
                        }
                        if let Some(block_id) = submission.block_id.as_deref() {
                            div { dt { "Block" } dd { title: "{block_id}", "{truncate_middle(block_id, 16, 8)}" } }
                        }
                        if let Some(fee) = submission.fee.as_ref() {
                            div { dt { "DUST fee" } dd { "{format_transfer_asset(fee)}" } }
                        }
                    }
                    if let Some(error) = operation_error {
                        p { class: "field-error", role: "alert", "{error}" }
                    }
                    if submission.reconciliation_allowed {
                        button {
                            class: "secondary-action",
                            r#type: "button",
                            disabled: reconciling,
                            aria_label: "Reconcile transaction submission with Midnight",
                            onclick: move |_| {
                                let recovery_status = current.clone();
                                state.set(SubmissionRecoveryPaneState::Ready {
                                    latest: Some(current.clone()),
                                    reconciling: true,
                                    operation_error: None,
                                });
                                let service = reconcile_services.reconcile_wallet_transfer_submission();
                                let profile_id = reconcile_profile.clone();
                                let draft_id = draft_id.clone();
                                spawn(async move {
                                    match run_ui_future(async move {
                                        service.execute(WalletTransferSubmissionQuery {
                                            profile_id,
                                            draft_id,
                                        }).await
                                    })
                                    .await
                                    {
                                        Ok(Ok(updated)) => state.set(SubmissionRecoveryPaneState::Ready {
                                            latest: Some(Box::new(updated)),
                                            reconciling: false,
                                            operation_error: None,
                                        }),
                                        Ok(Err(error)) => state.set(SubmissionRecoveryPaneState::Ready {
                                            latest: Some(recovery_status),
                                            reconciling: false,
                                            operation_error: Some(error.to_string()),
                                        }),
                                        Err(error) => state.set(SubmissionRecoveryPaneState::Ready {
                                            latest: Some(recovery_status),
                                            reconciling: false,
                                            operation_error: Some(error.to_string()),
                                        }),
                                    }
                                });
                            },
                            if reconciling { "Reconciling…" } else { "Reconcile with Midnight" }
                        }
                    } else if submission.replacement_allowed {
                        p { class: "consent-copy", "Midnight finalized no inclusion for this attempt. A newly prepared transfer may replace it." }
                    }
                }
            }
        }
    }
}

#[component]
fn DustSyncPane(profile_id: String, can_sync: bool) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| DustSyncPaneState::Loading);
    let load_services = services.clone();
    let load_profile = profile_id.clone();
    use_effect(move || {
        let services = load_services.clone();
        let profile_id = load_profile.clone();
        spawn(async move {
            state.set(
                run_ui_blocking(move || load_dust_sync(&services, &profile_id))
                    .await
                    .unwrap_or_else(|error| DustSyncPaneState::Failed(error.to_string())),
            );
        });
    });

    match state.read().clone() {
        DustSyncPaneState::Loading => rsx! {
            article { class: "surface-card wallet-sync-pane", role: "status", aria_busy: "true",
                p { class: "card-eyebrow", "DUST index" }
                h2 { "Loading DUST status…" }
            }
        },
        DustSyncPaneState::Failed(message) => {
            let retry_services = services.clone();
            let retry_profile = profile_id.clone();
            rsx! {
                article { class: "surface-card wallet-sync-pane", role: "alert",
                    div { class: "wallet-sync-row__heading",
                        div {
                            p { class: "card-eyebrow", "DUST index" }
                            h2 { "Status unavailable" }
                        }
                        span { class: "status-pill", "Error" }
                    }
                    p { "{message}" }
                    button {
                        class: "secondary-action",
                        r#type: "button",
                        onclick: move |_| {
                            let services = retry_services.clone();
                            let profile_id = retry_profile.clone();
                            state.set(DustSyncPaneState::Loading);
                            spawn(async move {
                                state.set(
                                    run_ui_blocking(move || load_dust_sync(&services, &profile_id))
                                        .await
                                        .unwrap_or_else(|error| DustSyncPaneState::Failed(error.to_string())),
                                );
                            });
                        },
                        "Retry"
                    }
                }
            }
        }
        DustSyncPaneState::Ready {
            status,
            action_busy,
            operation_error,
        } => {
            let syncing = status.state == "syncing";
            let unavailable = status.state == "unavailable";
            let progress = dust_progress_percent(&status);
            let balance = status
                .balance_atomic_units
                .as_deref()
                .map(|value| format_atomic_units(value, 15))
                .unwrap_or_else(|| "—".to_owned());
            let note = dust_sync_note(&status);
            let pill_class = dust_status_pill_class(&status.state);
            let action_services = services.clone();
            let action_profile = profile_id.clone();
            let mut action_state = state;
            rsx! {
                article { class: "surface-card wallet-sync-pane",
                    div { class: "wallet-sync-row__heading",
                        div {
                            p { class: "card-eyebrow", "DUST index" }
                            h2 { "{balance} DUST" }
                        }
                        span { class: "{pill_class}", "{dust_sync_state_label(&status.state)}" }
                    }
                    p { "{note}" }
                    if let Some(message) = operation_error {
                        p { class: "wallet-sync-error", role: "alert", "{message}" }
                    }
                    if let Some(percent) = progress {
                        div { class: "wallet-sync-progress", aria_label: "DUST synchronization progress",
                            div { class: "wallet-sync-progress__bar", style: "width: {percent}%" }
                        }
                    }
                    button {
                        class: "secondary-action wallet-sync-action",
                        r#type: "button",
                        disabled: action_busy || unavailable || (!can_sync && !syncing),
                        onclick: move |_| {
                            let command = WalletDustSyncCommand {
                                profile_id: action_profile.clone(),
                            };
                            let services = action_services.clone();
                            let profile_id = action_profile.clone();
                            let retained = status.clone();
                            action_state.set(DustSyncPaneState::Ready {
                                status: status.clone(),
                                action_busy: true,
                                operation_error: None,
                            });
                            spawn(async move {
                                let worker_services = services.clone();
                                let result = run_ui_blocking(move || {
                                    if syncing {
                                        worker_services.cancel_wallet_dust_sync().execute(command)
                                    } else {
                                        worker_services.start_wallet_dust_sync().execute(command)
                                    }
                                })
                                .await;
                                match result {
                                    Ok(Ok(updated)) => {
                                        let should_poll = updated.state == "syncing";
                                        action_state.set(DustSyncPaneState::Ready {
                                            status: updated,
                                            action_busy: false,
                                            operation_error: None,
                                        });
                                        if should_poll {
                                            poll_dust_sync(services, profile_id, action_state);
                                        }
                                    }
                                    Ok(Err(error)) => action_state.set(DustSyncPaneState::Ready {
                                        status: retained.clone(),
                                        action_busy: false,
                                        operation_error: Some(error.to_string()),
                                    }),
                                    Err(error) => action_state.set(DustSyncPaneState::Ready {
                                        status: retained,
                                        action_busy: false,
                                        operation_error: Some(error.to_string()),
                                    }),
                                }
                            });
                        },
                        if syncing {
                            if action_busy { "Cancelling DUST sync…" } else { "Cancel DUST sync" }
                        } else if !can_sync {
                            "Unlock wallet to sync DUST"
                        } else if status.state == "never_synced" {
                            if action_busy { "Starting DUST sync…" } else { "Sync DUST" }
                        } else {
                            if action_busy { "Starting DUST sync…" } else { "Resync DUST" }
                        }
                    }
                }
            }
        }
    }
}

fn load_dust_sync(services: &WalletUiServices, profile_id: &str) -> DustSyncPaneState {
    services
        .get_wallet_dust_sync_status()
        .execute(WalletDustSyncCommand {
            profile_id: profile_id.to_owned(),
        })
        .map_or_else(
            |error| DustSyncPaneState::Failed(error.to_string()),
            |status| DustSyncPaneState::Ready {
                status,
                action_busy: false,
                operation_error: None,
            },
        )
}

fn poll_dust_sync(
    services: WalletUiServices,
    profile_id: String,
    mut state: Signal<DustSyncPaneState>,
) {
    spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            match services
                .get_wallet_dust_sync_status()
                .execute(WalletDustSyncCommand {
                    profile_id: profile_id.clone(),
                }) {
                Ok(status) => {
                    let complete = status.state != "syncing";
                    state.set(DustSyncPaneState::Ready {
                        status,
                        action_busy: false,
                        operation_error: None,
                    });
                    if complete {
                        break;
                    }
                }
                Err(error) => {
                    state.set(DustSyncPaneState::Failed(error.to_string()));
                    break;
                }
            }
        }
    });
}

fn dust_progress_percent(status: &WalletDustSyncView) -> Option<u64> {
    let (current, target) = status.current_cursor.zip(status.target_cursor)?;
    let completed = u128::from(current).checked_add(1)?;
    let total = u128::from(target).checked_add(1)?;
    let percent = completed.checked_mul(100)?.checked_div(total)?.min(100);
    u64::try_from(percent).ok()
}

fn dust_sync_note(status: &WalletDustSyncView) -> String {
    let progress = status
        .current_cursor
        .zip(status.target_cursor)
        .map(|(current, target)| format!("event {current} of {target}"));
    let detail = match status.state.as_str() {
        "never_synced" => "DUST has not been indexed for this protected account.".to_owned(),
        "syncing" => progress.map_or_else(
            || "Connecting to the DUST event index…".to_owned(),
            |progress| format!("Indexing {progress} · {} processed this run.", status.events_processed),
        ),
        "synced" => progress.map_or_else(
            || "DUST is synchronized.".to_owned(),
            |progress| format!("DUST is current at {progress}."),
        ),
        "cached" => "Showing a resumable cached DUST checkpoint; spending remains disabled until live catch-up.".to_owned(),
        "cancelled" => "DUST synchronization was cancelled at a consistent checkpoint and can resume.".to_owned(),
        "stalled" => "DUST synchronization stalled; the last consistent checkpoint is retained.".to_owned(),
        _ => "DUST synchronization is not available in this composition.".to_owned(),
    };
    status.failure.as_ref().map_or(detail.clone(), |failure| {
        format!("{detail} ({})", failure.replace('_', " "))
    })
}

fn dust_sync_state_label(state: &str) -> &'static str {
    match state {
        "never_synced" => "Not synced",
        "syncing" => "Syncing",
        "synced" => "Synced",
        "cached" => "Cached",
        "cancelled" => "Cancelled",
        "stalled" => "Stalled",
        _ => "Unavailable",
    }
}

fn dust_status_pill_class(state: &str) -> &'static str {
    match state {
        "synced" => "status-pill success",
        "syncing" | "cached" => "status-pill warning",
        _ => "status-pill",
    }
}

#[component]
fn ShieldedSyncPane(profile_id: String, can_sync: bool) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| ShieldedSyncPaneState::Loading);
    let load_services = services.clone();
    let load_profile = profile_id.clone();
    use_effect(move || {
        let services = load_services.clone();
        let profile_id = load_profile.clone();
        spawn(async move {
            state.set(
                run_ui_blocking(move || load_shielded_sync(&services, &profile_id))
                    .await
                    .unwrap_or_else(|error| ShieldedSyncPaneState::Failed(error.to_string())),
            );
        });
    });

    match state.read().clone() {
        ShieldedSyncPaneState::Loading => rsx! {
            article { class: "surface-card wallet-sync-pane", role: "status", aria_busy: "true",
                p { class: "card-eyebrow", "Shielded index" }
                h2 { "Loading shielded status…" }
            }
        },
        ShieldedSyncPaneState::Failed(message) => {
            let retry_services = services.clone();
            let retry_profile = profile_id.clone();
            rsx! {
                article { class: "surface-card wallet-sync-pane", role: "alert",
                    div { class: "wallet-sync-row__heading",
                        div {
                            p { class: "card-eyebrow", "Shielded index" }
                            h2 { "Status unavailable" }
                        }
                        span { class: "status-pill", "Error" }
                    }
                    p { "{message}" }
                    button {
                        class: "secondary-action",
                        r#type: "button",
                        onclick: move |_| {
                            let services = retry_services.clone();
                            let profile_id = retry_profile.clone();
                            state.set(ShieldedSyncPaneState::Loading);
                            spawn(async move {
                                state.set(
                                    run_ui_blocking(move || load_shielded_sync(&services, &profile_id))
                                        .await
                                        .unwrap_or_else(|error| ShieldedSyncPaneState::Failed(error.to_string())),
                                );
                            });
                        },
                        "Retry"
                    }
                }
            }
        }
        ShieldedSyncPaneState::Ready {
            status,
            action_busy,
            operation_error,
        } => {
            let syncing = status.state == "syncing";
            let unavailable = status.state == "unavailable";
            let progress = shielded_progress_percent(&status);
            let note = shielded_sync_note(&status);
            let pill_class = dust_status_pill_class(&status.state);
            let owned_notes = status
                .owned_note_count
                .map_or_else(|| "—".to_owned(), |count| count.to_string());
            let action_services = services.clone();
            let action_profile = profile_id.clone();
            let mut action_state = state;
            rsx! {
                article { class: "surface-card wallet-sync-pane",
                    div { class: "wallet-sync-row__heading",
                        div {
                            p { class: "card-eyebrow", "Shielded index" }
                            h2 { "{owned_notes} shielded notes" }
                        }
                        span { class: "{pill_class}", "{dust_sync_state_label(&status.state)}" }
                    }
                    p { "{note}" }
                    if !status.balances.is_empty() {
                        div { class: "activity-list", aria_label: "Shielded token balances",
                            for balance in status.balances.iter() {
                                div { class: "activity-row", key: "{balance.token_type_hex}",
                                    span { class: "activity-row__mark", aria_hidden: "true", "◈" }
                                    div {
                                        strong { "{balance.atomic_units} atomic units" }
                                        small { title: "{balance.token_type_hex}", "Token {truncate_middle(&balance.token_type_hex, 8, 6)}" }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(message) = operation_error {
                        p { class: "wallet-sync-error", role: "alert", "{message}" }
                    }
                    if let Some(percent) = progress {
                        div { class: "wallet-sync-progress", aria_label: "Shielded synchronization progress",
                            div { class: "wallet-sync-progress__bar", style: "width: {percent}%" }
                        }
                    }
                    button {
                        class: "secondary-action wallet-sync-action",
                        r#type: "button",
                        disabled: action_busy || unavailable || (!can_sync && !syncing),
                        onclick: move |_| {
                            let command = WalletShieldedSyncCommand {
                                profile_id: action_profile.clone(),
                            };
                            let services = action_services.clone();
                            let profile_id = action_profile.clone();
                            let retained = status.clone();
                            action_state.set(ShieldedSyncPaneState::Ready {
                                status: status.clone(),
                                action_busy: true,
                                operation_error: None,
                            });
                            spawn(async move {
                                let worker_services = services.clone();
                                let result = run_ui_blocking(move || {
                                    if syncing {
                                        worker_services.cancel_wallet_shielded_sync().execute(command)
                                    } else {
                                        worker_services.start_wallet_shielded_sync().execute(command)
                                    }
                                })
                                .await;
                                match result {
                                    Ok(Ok(updated)) => {
                                        let should_poll = updated.state == "syncing";
                                        action_state.set(ShieldedSyncPaneState::Ready {
                                            status: updated,
                                            action_busy: false,
                                            operation_error: None,
                                        });
                                        if should_poll {
                                            poll_shielded_sync(services, profile_id, action_state);
                                        }
                                    }
                                    Ok(Err(error)) => action_state.set(ShieldedSyncPaneState::Ready {
                                        status: retained.clone(),
                                        action_busy: false,
                                        operation_error: Some(error.to_string()),
                                    }),
                                    Err(error) => action_state.set(ShieldedSyncPaneState::Ready {
                                        status: retained,
                                        action_busy: false,
                                        operation_error: Some(error.to_string()),
                                    }),
                                }
                            });
                        },
                        if syncing {
                            if action_busy { "Cancelling shielded sync…" } else { "Cancel shielded sync" }
                        } else if !can_sync {
                            "Unlock wallet to sync shielded assets"
                        } else if status.state == "never_synced" {
                            if action_busy { "Starting shielded sync…" } else { "Sync shielded assets" }
                        } else {
                            if action_busy { "Starting shielded sync…" } else { "Resync shielded assets" }
                        }
                    }
                }
            }
        }
    }
}

fn load_shielded_sync(services: &WalletUiServices, profile_id: &str) -> ShieldedSyncPaneState {
    services
        .get_wallet_shielded_sync_status()
        .execute(WalletShieldedSyncCommand {
            profile_id: profile_id.to_owned(),
        })
        .map_or_else(
            |error| ShieldedSyncPaneState::Failed(error.to_string()),
            |status| ShieldedSyncPaneState::Ready {
                status,
                action_busy: false,
                operation_error: None,
            },
        )
}

fn poll_shielded_sync(
    services: WalletUiServices,
    profile_id: String,
    mut state: Signal<ShieldedSyncPaneState>,
) {
    spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            match services
                .get_wallet_shielded_sync_status()
                .execute(WalletShieldedSyncCommand {
                    profile_id: profile_id.clone(),
                }) {
                Ok(status) => {
                    let complete = status.state != "syncing";
                    state.set(ShieldedSyncPaneState::Ready {
                        status,
                        action_busy: false,
                        operation_error: None,
                    });
                    if complete {
                        break;
                    }
                }
                Err(error) => {
                    state.set(ShieldedSyncPaneState::Failed(error.to_string()));
                    break;
                }
            }
        }
    });
}

fn shielded_progress_percent(status: &WalletShieldedSyncView) -> Option<u64> {
    let (current, target) = status.current_cursor.zip(status.target_cursor)?;
    let completed = u128::from(current).checked_add(1)?;
    let total = u128::from(target).checked_add(1)?;
    let percent = completed.checked_mul(100)?.checked_div(total)?.min(100);
    u64::try_from(percent).ok()
}

fn shielded_sync_note(status: &WalletShieldedSyncView) -> String {
    let progress = status
        .current_cursor
        .zip(status.target_cursor)
        .map(|(current, target)| format!("event {current} of {target}"));
    let detail = match status.state.as_str() {
        "never_synced" => {
            "Shielded notes have not been indexed for this protected account.".to_owned()
        }
        "syncing" => progress.map_or_else(
            || "Connecting to the shielded event index…".to_owned(),
            |progress| {
                format!(
                    "Indexing {progress} · {} processed this run.",
                    status.events_processed
                )
            },
        ),
        "synced" => progress.map_or_else(
            || "Shielded notes are synchronized.".to_owned(),
            |progress| format!("Shielded notes are current at {progress}."),
        ),
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
        format!("{detail} ({})", failure.replace('_', " "))
    })
}

fn load_account_page(services: &WalletUiServices, profile_id: &str) -> AccountPageState {
    let query = WalletAccountQuery {
        profile_id: profile_id.to_owned(),
    };
    let networks = match services.list_wallet_networks().execute(query.clone()) {
        Ok(networks) => networks,
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
    if !account_read_is_noninteractive(security.state_name()) {
        let Some(account) = protected_account_placeholder(&networks) else {
            return AccountPageState::Failed("selected Midnight network is unavailable".to_owned());
        };
        return AccountPageState::Ready {
            networks,
            account: Box::new(account),
            security,
            busy: None,
        };
    }
    let account = match services.get_wallet_account().execute(query) {
        Ok(account) => account,
        Err(WalletAccountError::Port(
            WalletAccountPortError::ProtectionNotInitialized
            | WalletAccountPortError::ProtectionLocked,
        )) if matches!(security.state_name(), "Uninitialized" | "Locked") => {
            let Some(account) = protected_account_placeholder(&networks) else {
                return AccountPageState::Failed(
                    "selected Midnight network is unavailable".to_owned(),
                );
            };
            account
        }
        Err(error) => return AccountPageState::Failed(error.to_string()),
    };
    AccountPageState::Ready {
        networks,
        account: Box::new(account),
        security,
        busy: None,
    }
}

fn account_read_is_noninteractive(security_state: &str) -> bool {
    !matches!(security_state, "Uninitialized" | "Locked")
}

fn protected_account_placeholder(networks: &WalletNetworkListView) -> Option<WalletAccountView> {
    let network = networks
        .networks
        .iter()
        .find(|network| network.network_id == networks.selected_network_id)?;
    Some(WalletAccountView {
        chain: network.chain.clone(),
        network_id: network.network_id.clone(),
        network_name: network.display_name.clone(),
        network_environment: network.environment.clone(),
        account_id: None,
        source: "unavailable".to_owned(),
        addresses: Vec::new(),
        balances: Vec::new(),
        sync: WalletSyncStatusView {
            state: "unavailable".to_owned(),
            current_cursor: None,
            target_cursor: None,
            chain_tip_height: None,
            updated_at_millis: None,
        },
        transactions: Vec::new(),
    })
}

async fn activate_protected_account(
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

fn has_protected_account(account: &WalletAccountView) -> bool {
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

#[component]
fn ReceiveAddress(kind: String, value: String) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut qr_open = use_signal(|| false);
    let mut export_notice = use_signal(|| None::<String>);
    let qr = render_qr_svg(&value);
    let copy_exporter = services.public_text_exporter();
    let copy_value = value.clone();
    let share_exporter = services.public_text_exporter();
    let share_value = value.clone();
    rsx! {
        div { class: "address-row",
            div {
                strong { "{address_kind_label(&kind)}" }
                small { "{address_purpose(&kind)}" }
            }
            code { title: "{value}", "{truncate_middle(&value, 18, 8)}" }
            span { class: "address-actions",
                button {
                    class: "address-action",
                    r#type: "button",
                    aria_label: "Copy {address_kind_label(&kind)} receive address",
                    onclick: move |_| {
                        let result = PublicReceiveAddress::new(copy_value.clone())
                            .and_then(|address| copy_exporter.copy_receive_address(address));
                        export_notice.set(Some(public_export_message(result, false)));
                    },
                    "Copy"
                }
                button {
                    class: "address-action",
                    r#type: "button",
                    aria_label: "Share {address_kind_label(&kind)} receive address",
                    onclick: move |_| {
                        let result = PublicReceiveAddress::new(share_value.clone())
                            .and_then(|address| share_exporter.share_receive_address(address));
                        export_notice.set(Some(public_export_message(result, true)));
                    },
                    "Share"
                }
                button {
                    class: "address-action",
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
        }
        if let Some(message) = export_notice.read().as_deref() {
            p { class: "address-export-notice", role: "status", "{message}" }
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

fn public_export_message(result: Result<(), PublicTextExportError>, share: bool) -> String {
    match result {
        Ok(()) if share => "Native share sheet opened for this public receive address.".to_owned(),
        Ok(()) => "Public receive address copied to the native clipboard.".to_owned(),
        Err(PublicTextExportError::Unavailable) => {
            "Native copy/share is unavailable on this device.".to_owned()
        }
        Err(PublicTextExportError::InvalidPublicText) => {
            "This receive address is not safe to export.".to_owned()
        }
        Err(PublicTextExportError::Failed) => {
            "The public receive address could not be exported.".to_owned()
        }
    }
}

#[component]
fn SendTransferPanel(
    profile_id: String,
    unshielded_receive_address: String,
    shielded_receive_address: String,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut panel = use_signal(|| TransferPanelState::Editing);
    let mut recipient = use_signal(String::new);
    let mut amount = use_signal(String::new);
    let mut shielded = use_signal(|| false);

    match panel.read().clone() {
        TransferPanelState::Editing => {
            let can_review =
                !recipient.read().trim().is_empty() && !amount.read().trim().is_empty();
            rsx! {
                article { class: "surface-card transfer-card",
                    p { class: "card-eyebrow", "Send" }
                    h2 { if shielded() { "Send shielded NIGHT" } else { "Send unshielded NIGHT" } }
                    p {
                        if shielded() {
                            "A freshly synchronized private note set is required before exact review and authorization."
                        } else {
                            "The recipient and exact amount are validated before an explicit review and authorization step."
                        }
                    }
                    span { class: "transfer-field-label", "Transfer privacy" }
                    div { class: "privacy-choice", role: "group", aria_label: "Transfer privacy",
                        button {
                            class: if shielded() { "privacy-choice__option" } else { "privacy-choice__option selected" },
                            r#type: "button",
                            aria_label: "Use public NIGHT transfer",
                            aria_pressed: if shielded() { "false" } else { "true" },
                            onclick: move |_| {
                                shielded.set(false);
                                recipient.set(String::new());
                            },
                            "Public NIGHT"
                        }
                        button {
                            class: if shielded() { "privacy-choice__option selected" } else { "privacy-choice__option" },
                            r#type: "button",
                            aria_label: "Use shielded NIGHT transfer",
                            aria_pressed: if shielded() { "true" } else { "false" },
                            onclick: move |_| {
                                shielded.set(true);
                                recipient.set(String::new());
                            },
                            "Shielded NIGHT"
                        }
                    }
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
                        onclick: move |_| {
                            recipient.set(if shielded() {
                                shielded_receive_address.clone()
                            } else {
                                unshielded_receive_address.clone()
                            });
                        },
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
                                    let shielded_transfer = shielded();
                                    let profile_id = profile_id.clone();
                                    let recipient_address = recipient.read().trim().to_owned();
                                    panel.set(TransferPanelState::Preparing);
                                    if shielded_transfer {
                                        let service = services.prepare_shielded_wallet_transfer();
                                        spawn(async move {
                                            let command = PrepareShieldedWalletTransferCommand {
                                                profile_id,
                                                recipient_address,
                                                token_type: "0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
                                                amount_atomic_units,
                                            };
                                            match run_ui_blocking(move || service.execute(command)).await {
                                                Ok(Ok(preview)) => panel.set(
                                                    TransferPanelState::Prepared(Box::new(preview)),
                                                ),
                                                Ok(Err(error)) => panel.set(TransferPanelState::Failed {
                                                    message: error.to_string(),
                                                    retained: None,
                                                    recovery: TransferRecovery::Edit,
                                                }),
                                                Err(error) => panel.set(TransferPanelState::Failed {
                                                    message: error.to_string(),
                                                    retained: None,
                                                    recovery: TransferRecovery::Edit,
                                                }),
                                            }
                                        });
                                    } else {
                                        let service = services.prepare_wallet_transfer();
                                        spawn(async move {
                                            let command = PrepareWalletTransferCommand {
                                                profile_id,
                                                recipient_address,
                                                amount_atomic_units,
                                            };
                                            match run_ui_blocking(move || service.execute(command)).await {
                                            Ok(Ok(preview)) => panel.set(
                                                TransferPanelState::Prepared(Box::new(preview)),
                                            ),
                                            Ok(Err(error)) => panel.set(TransferPanelState::Failed {
                                                message: error.to_string(),
                                                retained: None,
                                                recovery: TransferRecovery::Edit,
                                            }),
                                            Err(error) => panel.set(TransferPanelState::Failed {
                                                message: error.to_string(),
                                                retained: None,
                                                recovery: TransferRecovery::Edit,
                                            }),
                                        }
                                        });
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
        TransferPanelState::Preparing => rsx! {
            article { class: "surface-card transfer-card submitting-card", role: "status", aria_live: "polite", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                div {
                    p { class: "card-eyebrow", "Preparing" }
                    h2 { "Building the transfer preview" }
                    p { "Oxid is validating the recipient, synchronized balance, and canonical Midnight transaction inputs." }
                }
            }
        },
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
                        div { dt { "Privacy" } dd { "{transfer_privacy_label(&preview.recipient_kind)}" } }
                        div { dt { "Network" } dd { "{preview.network_id}" } }
                        div { dt { "Change" } dd { "{change_label}" } }
                        div { dt { "Inputs" } dd { "{preview.input_count}" } }
                        div { dt { "DUST fee" } dd { "Calculated during proving" } }
                    }
                    p { class: "consent-copy", "Authorization binds only this reviewed transfer. Proof generation and submission remain a separate action." }
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
                                let service = services.authorize_wallet_transfer();
                                let command = AuthorizeWalletTransferCommand {
                                    profile_id: profile_id.clone(),
                                    draft_id: draft_id.clone(),
                                    authorization_challenge: challenge.clone(),
                                    confirmation: confirmation.clone(),
                                };
                                let retained_preview = preview.clone();
                                panel.set(TransferPanelState::Authorizing(preview.clone()));
                                spawn(async move {
                                    match run_ui_blocking(move || service.execute(command)).await {
                                        Ok(Ok(authorized)) => panel.set(
                                            TransferPanelState::Authorized(Box::new(authorized)),
                                        ),
                                        Ok(Err(error)) => panel.set(TransferPanelState::Failed {
                                            message: error.to_string(),
                                            retained: Some(retained_preview.clone()),
                                            recovery: TransferRecovery::Edit,
                                        }),
                                        Err(error) => panel.set(TransferPanelState::Failed {
                                            message: error.to_string(),
                                            retained: Some(retained_preview),
                                            recovery: TransferRecovery::Edit,
                                        }),
                                    }
                                });
                            },
                            "Authorize transfer"
                        }
                    }
                }
            }
        }
        TransferPanelState::Authorizing(preview) => rsx! {
            article { class: "surface-card transfer-card submitting-card", role: "status", aria_live: "polite", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                div {
                    p { class: "card-eyebrow", "Authorizing" }
                    h2 { "Confirm {format_transfer_asset(&preview.amount)} with device protection" }
                    p { "The operating-system authorization prompt can complete without freezing the wallet interface." }
                }
            }
        },
        TransferPanelState::Authorized(preview) => {
            let amount_label = format_transfer_asset(&preview.amount);
            let confirmation = submit_transfer_confirmation(&preview);
            let draft_id = preview.draft_id.clone();
            let submitting_preview = preview.clone();
            rsx! {
                article { class: "surface-card transfer-card review-card", aria_label: "Authorized NIGHT transfer",
                    p { class: "card-eyebrow", "Authorized" }
                    h2 { "{amount_label} is ready" }
                    p { "The authorized transaction is retained inside the Midnight adapter. Continue to prove, balance the DUST fee, and submit." }
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
                                let execute_profile = profile_id.clone();
                                let execute_draft = draft_id.clone();
                                match run_ui_future(async move {
                                    service.execute(SubmitWalletTransferCommand {
                                        profile_id: execute_profile,
                                        draft_id: execute_draft,
                                        confirmation,
                                    }).await
                                })
                                .await
                                {
                                    Ok(Ok(submitted)) => panel.set(TransferPanelState::Submitted(Box::new(submitted))),
                                    Ok(Err(error)) => {
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
                                    Err(error) => panel.set(TransferPanelState::Failed {
                                        message: error.to_string(),
                                        retained: None,
                                        recovery: TransferRecovery::ReconcileUnknown,
                                    }),
                                }
                            });
                        },
                        "Prove and submit"
                    }
                }
            }
        }
        TransferPanelState::Submitting(preview) => {
            let cancel_services = services.clone();
            let cancel_profile = profile_id.clone();
            let cancel_draft = preview.draft_id.clone();
            let cancelling_preview = preview.clone();
            rsx! {
                article { class: "surface-card transfer-card submitting-card", role: "status", aria_live: "polite", aria_busy: "true",
                    span { class: "loading-mark", aria_hidden: "true" }
                    div {
                        p { class: "card-eyebrow", "Submitting" }
                        h2 { "Proving {format_transfer_asset(&preview.amount)}" }
                        p { "The worker is balancing the DUST fee and proving locally. Cancellation is available only before broadcast." }
                        button {
                            class: "secondary-action",
                            r#type: "button",
                            aria_label: "Cancel NIGHT transfer submission",
                            onclick: move |_| {
                                let query = WalletTransferSubmissionQuery {
                                    profile_id: cancel_profile.clone(),
                                    draft_id: cancel_draft.clone(),
                                };
                                match cancel_services
                                    .cancel_wallet_transfer_submission()
                                    .execute(query)
                                {
                                    Ok(status) => {
                                        panel.set(TransferPanelState::Cancelling(cancelling_preview.clone()));
                                        poll_transfer_cancellation(
                                            cancel_services.clone(),
                                            cancel_profile.clone(),
                                            cancel_draft.clone(),
                                            panel,
                                            status,
                                        );
                                    }
                                    Err(error) => panel.set(TransferPanelState::Failed {
                                        message: error.to_string(),
                                        retained: Some(preview.clone()),
                                        recovery: TransferRecovery::ReconcileUnknown,
                                    }),
                                }
                            },
                            "Cancel before broadcast"
                        }
                    }
                }
            }
        }
        TransferPanelState::Cancelling(preview) => rsx! {
            article { class: "surface-card transfer-card submitting-card", role: "status", aria_live: "polite", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                div {
                    p { class: "card-eyebrow", "Cancelling" }
                    h2 { "Stopping {format_transfer_asset(&preview.amount)} safely" }
                    p { "Oxid is waiting for the worker to acknowledge cancellation at a pre-broadcast boundary." }
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

fn poll_transfer_cancellation(
    services: WalletUiServices,
    profile_id: String,
    draft_id: String,
    mut panel: Signal<TransferPanelState>,
    initial: WalletTransferSubmissionStatusView,
) {
    spawn(async move {
        let mut status = initial;
        loop {
            match status.state.as_str() {
                "running" | "cancellation_requested" => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    match services.get_wallet_transfer_submission_status().execute(
                        WalletTransferSubmissionQuery {
                            profile_id: profile_id.clone(),
                            draft_id: draft_id.clone(),
                        },
                    ) {
                        Ok(updated) => status = updated,
                        Err(_) => break,
                    }
                }
                "cancelled" => {
                    let retained = services
                        .get_wallet_transfer_draft()
                        .execute(WalletTransferDraftQuery {
                            profile_id,
                            draft_id,
                        })
                        .ok()
                        .map(Box::new);
                    panel.set(TransferPanelState::Failed {
                        message: "Transaction submission was cancelled before broadcast."
                            .to_owned(),
                        retained,
                        recovery: TransferRecovery::RetryAuthorized,
                    });
                    break;
                }
                "outcome_unknown" => {
                    panel.set(TransferPanelState::Failed {
                        message: "Transaction submission may have reached Midnight and requires reconciliation."
                            .to_owned(),
                        retained: None,
                        recovery: TransferRecovery::ReconcileUnknown,
                    });
                    break;
                }
                _ => break,
            }
        }
    });
}

fn post_submission_recovery(retained_state: Option<&str>) -> TransferRecovery {
    match retained_state {
        Some("authorized") => TransferRecovery::RetryAuthorized,
        _ => TransferRecovery::ReconcileUnknown,
    }
}

fn submission_status_heading(state: &str) -> &'static str {
    match state {
        "included" => "Transfer included",
        "broadcasting" => "Transfer broadcast",
        "outcome_unknown" => "Submission outcome unknown",
        "rejected" => "Submission rejected",
        "expired" => "Submission expired",
        _ => "Submission in progress",
    }
}

fn submission_status_label(state: &str) -> &'static str {
    match state {
        "included" => "Included",
        "broadcasting" => "Broadcasting",
        "outcome_unknown" => "Outcome unknown",
        "rejected" => "Rejected",
        "expired" => "Expired",
        "running" => "Preparing",
        "cancellation_requested" => "Cancelling",
        "cancelled" => "Cancelled",
        _ => "Not started",
    }
}

fn submission_status_note(state: &str) -> &'static str {
    match state {
        "included" => {
            "The durable journal confirms this transfer was included in a finalized Midnight block."
        }
        "broadcasting" => {
            "This transaction was durably recorded before broadcast. Reconcile it before preparing a replacement."
        }
        "outcome_unknown" => {
            "The transaction may have reached Midnight. Oxid will not submit a duplicate while its outcome is unknown."
        }
        "rejected" => {
            "Midnight finalized this submission as rejected. Its public record is retained for recovery history."
        }
        "expired" => "The submission was not included before its bounded validity window expired.",
        _ => "Oxid is still preparing this submission and has not crossed the broadcast boundary.",
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

fn transfer_privacy_label(kind: &str) -> &'static str {
    if kind == "shielded" {
        "Shielded"
    } else {
        "Unshielded"
    }
}

fn authorize_transfer_confirmation(
    preview: &WalletTransferPreviewView,
) -> SensitiveOperationConfirmation {
    SensitiveOperationConfirmation {
        title: "Authorize NIGHT transfer".to_owned(),
        summary: format!(
            "Send {} as a {} transfer to {} on {}; DUST fee balancing and proving remain pending",
            format_transfer_asset(&preview.amount),
            transfer_privacy_label(&preview.recipient_kind).to_lowercase(),
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
            "Prove and submit {} as a {} transfer to {} on {}",
            format_transfer_asset(&preview.amount),
            transfer_privacy_label(&preview.recipient_kind).to_lowercase(),
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

const STANDALONE_DID_FIXTURE: &str =
    "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn load_did_page(services: &WalletUiServices, profile_id: &str) -> DidPageState {
    services
        .list_did_records()
        .execute(ListDidRecordsQuery {
            profile_id: profile_id.to_owned(),
        })
        .map_or_else(
            |error| DidPageState::Failed(did_operation_message(error)),
            |records| DidPageState::Ready {
                records,
                resolving: false,
                operation_error: None,
            },
        )
}

fn did_operation_message(error: DidOperationError) -> String {
    error.to_string()
}

fn self_issued_authentication_message(error: SelfIssuedAuthenticationError) -> String {
    error.to_string()
}

fn active_managed_authentication_method(records: &[DidRecordView]) -> Option<(String, String)> {
    records
        .iter()
        .filter(|record| record.document_metadata.deactivated != Some(true))
        .find_map(|record| {
            record
                .document
                .relationships
                .iter()
                .find(|relationship| relationship.relationship == "authentication")
                .and_then(|relationship| {
                    relationship
                        .method_ids
                        .iter()
                        .find(|method_id| record.managed_method_ids.contains(method_id))
                })
                .map(|method_id| (record.document.id.clone(), method_id.clone()))
        })
}

fn active_managed_issuance_methods(records: &[DidRecordView]) -> Option<(String, String, String)> {
    records
        .iter()
        .filter(|record| record.document_metadata.deactivated != Some(true))
        .find_map(|record| {
            let authentication = record
                .document
                .relationships
                .iter()
                .find(|relationship| relationship.relationship == "authentication")?
                .method_ids
                .iter()
                .find(|method_id| record.managed_method_ids.contains(method_id))?;
            let assertion = record
                .document
                .relationships
                .iter()
                .find(|relationship| relationship.relationship == "assertionMethod")?;
            let holder_binding = record.document.verification_methods.iter().find(|method| {
                method.controller == record.document.id
                    && method.public_key_jwk.key_type == "EC"
                    && method.public_key_jwk.curve == "Jubjub"
                    && record.managed_method_ids.contains(&method.id)
                    && assertion.method_ids.contains(&method.id)
            })?;
            Some((
                record.document.id.clone(),
                authentication.clone(),
                holder_binding.id.clone(),
            ))
        })
}

fn did_confirmation(title: &str, summary: &str, confirmed: bool) -> DidOperationConfirmation {
    DidOperationConfirmation {
        title: title.to_owned(),
        summary: summary.to_owned(),
        confirmed,
    }
}

#[component]
fn ManagedDidControls(
    profile_id: String,
    record: DidRecordView,
    on_record: EventHandler<Result<DidRecordView, String>>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut operation = use_signal(|| "add_alias".to_owned());
    let mut identifier = use_signal(String::new);
    let mut value = use_signal(String::new);
    let mut endpoint = use_signal(String::new);
    let mut algorithm = use_signal(|| "ed25519".to_owned());
    let mut relationship = use_signal(|| "assertionMethod".to_owned());
    let mut confirmed = use_signal(|| false);
    let mut working = use_signal(|| false);
    let mut outcome = use_signal(|| None::<String>);
    let did = record.document.id.clone();
    let is_deactivated = record.document_metadata.deactivated == Some(true);
    let operation_name = operation.read().clone();
    let is_service = matches!(operation_name.as_str(), "add_service" | "update_service");
    let needs_identifier = matches!(
        operation_name.as_str(),
        "add_method"
            | "update_method"
            | "remove_method"
            | "add_relationship"
            | "remove_relationship"
            | "add_service"
            | "update_service"
            | "remove_service"
            | "sign"
    );
    let needs_value = matches!(
        operation_name.as_str(),
        "add_alias" | "remove_alias" | "sign" | "add_service" | "update_service"
    );
    let needs_algorithm = matches!(operation_name.as_str(), "add_method" | "update_method");
    let needs_relationship = matches!(
        operation_name.as_str(),
        "add_relationship" | "remove_relationship"
    );
    let needs_confirmation = true;

    rsx! {
        details { class: "did-manager",
            summary { "Manage this DID" }
            p { class: "form-hint", "Standalone operations use protected, process-local keys. Public DID records persist; development key custody does not survive an app restart." }
            label { r#for: "did-operation-{did}", "Operation" }
            select {
                id: "did-operation-{did}",
                value: "{operation}",
                disabled: working() || is_deactivated,
                onchange: move |event| {
                    operation.set(event.value());
                    outcome.set(None);
                    confirmed.set(false);
                },
                option { value: "add_alias", "Add also-known-as" }
                option { value: "remove_alias", "Remove also-known-as" }
                option { value: "add_method", "Add verification method" }
                option { value: "update_method", "Rotate verification method" }
                option { value: "remove_method", "Remove verification method" }
                option { value: "add_relationship", "Add verification relationship" }
                option { value: "remove_relationship", "Remove verification relationship" }
                option { value: "add_service", "Add service" }
                option { value: "update_service", "Update service" }
                option { value: "remove_service", "Remove service" }
                option { value: "sign", "Sign payload" }
                option { value: "deactivate", "Deactivate DID" }
            }
            if needs_identifier {
                label { r#for: "did-entry-{did}",
                    if is_service { "Service fragment" } else if operation_name == "sign" { "Verification method" } else { "Method fragment" }
                }
                input {
                    id: "did-entry-{did}", r#type: "text", maxlength: 2048,
                    autocomplete: "off", spellcheck: false,
                    placeholder: if is_service { "#messages" } else { "#auth-1" },
                    value: "{identifier}",
                    oninput: move |event| identifier.set(event.value()),
                }
            }
            if needs_algorithm {
                label { r#for: "did-algorithm-{did}", "Protected key algorithm" }
                select {
                    id: "did-algorithm-{did}", value: "{algorithm}",
                    onchange: move |event| algorithm.set(event.value()),
                    option { value: "ed25519", "Ed25519" }
                    option { value: "jubjub", "Jubjub" }
                    option { value: "p256", "P-256" }
                }
            }
            if needs_relationship {
                label { r#for: "did-relationship-{did}", "Relationship" }
                select {
                    id: "did-relationship-{did}", value: "{relationship}",
                    onchange: move |event| relationship.set(event.value()),
                    option { value: "authentication", "Authentication" }
                    option { value: "assertionMethod", "Assertion method" }
                    option { value: "capabilityInvocation", "Capability invocation" }
                    option { value: "capabilityDelegation", "Capability delegation" }
                }
            }
            if is_service {
                label { r#for: "did-service-type-{did}", "Service type" }
                input {
                    id: "did-service-type-{did}", r#type: "text", maxlength: 128,
                    placeholder: "DIDCommMessaging", value: "{value}",
                    oninput: move |event| value.set(event.value()),
                }
                label { r#for: "did-endpoint-{did}", "Service endpoint" }
                input {
                    id: "did-endpoint-{did}", r#type: "url", maxlength: 2048,
                    placeholder: "https://example.test/messages", value: "{endpoint}",
                    oninput: move |event| endpoint.set(event.value()),
                }
            } else if needs_value {
                label { r#for: "did-value-{did}",
                    if operation_name == "sign" { "Payload" } else { "URI" }
                }
                input {
                    id: "did-value-{did}", r#type: "text", maxlength: 8192,
                    autocomplete: "off", spellcheck: false,
                    placeholder: if operation_name == "sign" { "Verifier challenge" } else { "https://example.test/alice" },
                    value: "{value}",
                    oninput: move |event| value.set(event.value()),
                }
            }
            if needs_confirmation {
                label { class: "confirmation-row",
                    input {
                        r#type: "checkbox", checked: confirmed(),
                        onchange: move |event| confirmed.set(event.checked()),
                    }
                    if operation_name == "deactivate" {
                        "I understand this DID cannot be used after deactivation"
                    } else if operation_name == "sign" {
                        "Authorize signing this visible payload with the selected DID method"
                    } else {
                        "Authorize this visible change to the managed DID document"
                    }
                }
            }
            button {
                class: if operation_name == "deactivate" { "danger-action" } else { "secondary-action" },
                r#type: "button",
                disabled: working() || is_deactivated || (needs_confirmation && !confirmed()),
                onclick: move |_| {
                    working.set(true);
                    outcome.set(None);
                    let operation_name = operation.read().clone();
                    let method_or_service = identifier.read().trim().to_owned();
                    let input_value = value.read().trim().to_owned();
                    let endpoint_value = endpoint.read().trim().to_owned();
                    let key_algorithm = match algorithm.read().as_str() {
                        "jubjub" => DidKeyAlgorithm::Jubjub,
                        "p256" => DidKeyAlgorithm::P256,
                        _ => DidKeyAlgorithm::Ed25519,
                    };
                    let relationship = VerificationRelationship::parse(relationship.read().as_str())
                        .unwrap_or(VerificationRelationship::AssertionMethod);
                    let confirmed = confirmed();
                    let services = services.clone();
                    let profile_id = profile_id.clone();
                    let did = did.clone();
                    spawn(async move {
                        let result = run_ui_blocking(move || match operation_name.as_str() {
                            "sign" => services
                                .sign_did_payload()
                                .execute(SignDidPayloadCommand {
                                    profile_id,
                                    did,
                                    method_id: method_or_service,
                                    payload: input_value.into_bytes(),
                                    confirmation: did_confirmation(
                                        "Sign identity challenge",
                                        "Authorize the visible payload with this DID verification method",
                                        confirmed,
                                    ),
                                })
                                .map(|signature| {
                                    (
                                        None,
                                        format!(
                                            "Signed {} bytes with {} using {}.",
                                            signature.signature_bytes.len(),
                                            signature.method_id,
                                            signature.algorithm
                                        ),
                                    )
                                }),
                            "deactivate" => services
                                .deactivate_did()
                                .execute(DeactivateDidCommand {
                                    profile_id,
                                    did,
                                    confirmation: did_confirmation(
                                        "Deactivate DID",
                                        "Permanently disable further operations for this DID",
                                        confirmed,
                                    ),
                                })
                                .map(|record| {
                                    (Some(record), "DID document deactivated.".to_owned())
                                }),
                            _ => {
                                let update = match operation_name.as_str() {
                                    "add_alias" => DidUpdate::AddAlsoKnownAs { value: input_value },
                                    "remove_alias" => DidUpdate::RemoveAlsoKnownAs { value: input_value },
                                    "add_method" => DidUpdate::AddVerificationMethod { fragment: method_or_service, algorithm: key_algorithm },
                                    "update_method" => DidUpdate::UpdateVerificationMethod { method_id: method_or_service, algorithm: key_algorithm },
                                    "remove_method" => DidUpdate::RemoveVerificationMethod { method_id: method_or_service },
                                    "add_relationship" => DidUpdate::AddVerificationRelationship { relationship, method_id: method_or_service },
                                    "remove_relationship" => DidUpdate::RemoveVerificationRelationship { relationship, method_id: method_or_service },
                                    "add_service" => DidUpdate::AddService { id: method_or_service, service_type: input_value, endpoint: endpoint_value },
                                    "update_service" => DidUpdate::UpdateService { id: method_or_service, service_type: input_value, endpoint: endpoint_value },
                                    "remove_service" => DidUpdate::RemoveService { id: method_or_service },
                                    _ => DidUpdate::AddAlsoKnownAs { value: input_value },
                                };
                                services
                                    .update_did()
                                    .execute(UpdateDidCommand {
                                        profile_id,
                                        did,
                                        operation: update,
                                        confirmation: did_confirmation(
                                            "Update DID document",
                                            "Authorize the selected visible change to this managed DID",
                                            confirmed,
                                        ),
                                    })
                                    .map(|record| {
                                        (Some(record), "DID document updated.".to_owned())
                                    })
                            }
                        })
                        .await;
                        working.set(false);
                        match result {
                            Ok(Ok((updated, message))) => {
                                outcome.set(Some(message));
                                if let Some(updated) = updated {
                                    on_record.call(Ok(updated));
                                }
                            }
                            Ok(Err(error)) => {
                                let message = did_operation_message(error);
                                outcome.set(Some(message.clone()));
                                on_record.call(Err(message));
                            }
                            Err(error) => {
                                let message = error.to_string();
                                outcome.set(Some(message.clone()));
                                on_record.call(Err(message));
                            }
                        }
                    });
                },
                if working() { "Working…" } else if operation_name == "sign" { "Sign payload" } else if operation_name == "deactivate" { "Deactivate DID" } else { "Apply DID update" }
            }
            if is_deactivated {
                p { class: "form-hint", "This DID is deactivated. Mutable and signing operations are disabled." }
            }
            if let Some(message) = outcome() {
                p { class: "form-hint", role: "status", "{message}" }
            }
        }
    }
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
    if value.is_empty()
        || value.len() > 39
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err("Enter a canonical whole-number NIGHT amount in base units.".to_owned());
    }
    value
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

fn passport_vault_call_mode_label(mode: &str) -> &'static str {
    match mode {
        "native_settlement" => "Midnight live",
        "deterministic_simulation" => "Deterministic simulation",
        _ => "Unavailable",
    }
}

fn passport_vault_call_mode_note(mode: &str) -> &'static str {
    match mode {
        "native_settlement" => {
            "Calls use authenticated finalized state and the protected Midnight proving, submission, and reconciliation boundary."
        }
        "deterministic_simulation" => {
            "Calls exercise the complete retained lifecycle locally and are always labelled simulated; no node broadcast occurs."
        }
        _ => {
            "Configure the complete standalone Midnight stack and authenticated Passport Vault artifacts to enable contract calls."
        }
    }
}

fn passport_vault_contract_source_label(source: &str) -> &str {
    match source {
        "deterministic_simulation" => "simulated",
        value => value,
    }
}

fn passport_vault_submission_mode_label(mode: &str) -> &str {
    match mode {
        "deterministic_simulation_only" => "simulated · deterministic simulation only",
        value => value,
    }
}

fn passport_vault_call_recovery(retained_state: Option<&str>) -> PassportVaultCallRecovery {
    match retained_state {
        Some("authorized") => PassportVaultCallRecovery::RetryAuthorized,
        _ => PassportVaultCallRecovery::ReconcileUnknown,
    }
}

fn passport_vault_submission_heading(state: &str) -> &'static str {
    match state {
        "included" => "Vault call included",
        "broadcasting" => "Vault call broadcast",
        "outcome_unknown" => "Vault call outcome unknown",
        "rejected" => "Vault call rejected",
        "expired" => "Vault call expired",
        "cancelled" => "Vault call cancelled",
        _ => "Vault call in progress",
    }
}

fn passport_vault_submission_note(state: &str) -> &'static str {
    match state {
        "included" => "Midnight reported finalized public inclusion metadata for this call.",
        "broadcasting" => {
            "The broadcast boundary was crossed; cancellation and replacement are disabled."
        }
        "outcome_unknown" => {
            "Oxid will not submit a duplicate. Reconcile this attempt with finalized history."
        }
        "rejected" => "Finalized history rejected this attempt; prepare a fresh call if allowed.",
        "expired" => "The retained authorization expired before safe completion.",
        "cancelled" => {
            "The worker stopped before broadcast; the authorized draft may be retryable."
        }
        _ => "Proving or submission is still running.",
    }
}

#[component]
fn PassportVaultContractCallPanel(profile_id: String, credentials: Vec<CredentialView>) -> Element {
    let services = consume_context::<WalletUiServices>();
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
    let mode_label = passport_vault_call_mode_label(&calls.mode);
    let mode_note = passport_vault_call_mode_note(&calls.mode);
    let read_state_button_label = match chain_state.read().clone() {
        PassportVaultContractStatePaneState::Loading => "Reading contract state…".to_owned(),
        PassportVaultContractStatePaneState::Ready(vault) => {
            format!(
                "Refresh {} contract state",
                passport_vault_contract_source_label(&vault.source)
            )
        }
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
                p { class: "form-hint", "Enter the reviewed deployment address. Oxid will authenticate state from configured finalized history." }
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
                        "simulated_or_unanchored",
                        |anchor| anchor.state_authentication.as_str(),
                    );
                    rsx! {
                        p { class: "form-hint", aria_live: "polite",
                            "Contract state loaded from {vault.source}."
                        }
                        div { class: "surface-card",
                            p { class: "card-eyebrow", "Contract state" }
                            dl { class: "preview-list",
                                div { dt { "Source" } dd { "{vault.source}" } }
                                div { dt { "Authentication" } dd { "{authentication}" } }
                                div { dt { "Total locked" } dd { "{vault.total_locked} base units" } }
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
                                label { "Maximum claim (base units)"
                                    input { inputmode: "numeric", value: "{maximum_claim}", oninput: move |event| maximum_claim.set(event.value()) }
                                }
                                label { "Initial deposit (base units)"
                                    input { inputmode: "numeric", value: "{initial_amount}", oninput: move |event| initial_amount.set(event.value()) }
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
                                label { "Amount (base units)"
                                    input { inputmode: "numeric", value: "{amount}", oninput: move |event| amount.set(event.value()) }
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
                        h2 { "Proving {preview.operation}" }
                        p { "Cancellation is safe only before the broadcast boundary. Oxid never blind-retries an ambiguous outcome." }
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
                    h2 { "Stopping {preview.operation} safely" }
                    p { "Waiting for the submission worker to acknowledge a pre-broadcast boundary." }
                }
            },
            PassportVaultContractPanelState::Submitted(submission) => rsx! {
                article { class: "info-card submitted-card", role: "status", aria_live: "polite",
                    p { class: "card-eyebrow", "Included" }
                    h2 { "Passport Vault call completed" }
                    p { "Mode: {passport_vault_submission_mode_label(&submission.mode)}. Final DUST fee: {submission.fee_atomic_units} base units." }
                    dl { class: "preview-list",
                        div { dt { "Operation" } dd { "{submission.call.operation}" } }
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
                    h2 { "{passport_vault_submission_heading(&submission.state)}" }
                    p { "{passport_vault_submission_note(&submission.state)}" }
                    dl { class: "preview-list",
                        div { dt { "State" } dd { "{submission.state}" } }
                        if let Some(mode) = submission.mode.as_deref() {
                            div { dt { "Mode" } dd { "{passport_vault_submission_mode_label(mode)}" } }
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
                            p { "Oxid will not prepare or submit a replacement while broadcast may have occurred. Use the recovery card above." }
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
            h2 { "{preview.operation}" }
            dl { class: "preview-list",
                div { dt { "Amount" } dd { "{preview.amount_atomic_units} base units" } }
                if let Some(lock_id) = preview.lock_id {
                    div { dt { "Lock" } dd { "#{lock_id}" } }
                }
                div { dt { "State height" } dd { "{preview.state_anchor_block_height}" } }
                div { dt { "State block" } dd { title: "{preview.state_anchor_block_hash_hex}", "{truncate_middle(&preview.state_anchor_block_hash_hex, 16, 8)}" } }
                div { dt { "Draft state" } dd { "{preview.state}" } }
                div { dt { "DUST fee" } dd { if let Some(fee) = preview.fee_atomic_units.as_deref() { "{fee} base units" } else { "Calculated during proving" } } }
            }
        }
    }
}

#[component]
fn PassportVaultCallRecoveryPane(profile_id: String) -> Element {
    let services = consume_context::<WalletUiServices>();
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
                    h2 { "{passport_vault_submission_heading(&submission.state)}" }
                    p { "{passport_vault_submission_note(&submission.state)}" }
                    dl { class: "preview-list",
                        div { dt { "State" } dd { "{submission.state}" } }
                        if let Some(mode) = submission.mode.as_deref() {
                            div { dt { "Mode" } dd { "{passport_vault_submission_mode_label(mode)}" } }
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
fn PassportVaultPage(active_profile: WalletProfileView) -> Element {
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
            let persistence_note = match state_persistence.as_str() {
                "owner_private_atomic_file" => {
                    "Owner-private durable conformance ledger · survives app restart · no on-chain transaction submitted"
                }
                "process_local" => {
                    "Process-local conformance ledger · no on-chain transaction submitted"
                }
                _ => "Standalone conformance ledger · no on-chain transaction submitted",
            };
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
                        h2 { "{vault.total_locked} base units" }
                        div { class: "balance-breakdown",
                            span { "Deposited {vault.total_deposited}" }
                            span { "Released {vault.total_released}" }
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
                            label { "Maximum claim (base units)"
                                input { inputmode: "numeric", aria_label: "Vault maximum claim", value: "{maximum_claim}", oninput: move |event| maximum_claim.set(event.value()) }
                            }
                            label { "Initial deposit (base units)"
                                input { inputmode: "numeric", aria_label: "Vault initial deposit", value: "{initial_amount}", oninput: move |event| initial_amount.set(event.value()) }
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
                        label { "Operation amount (base units)"
                            input { inputmode: "numeric", aria_label: "Vault operation amount", value: "{operation_amount}", oninput: move |event| operation_amount.set(event.value()) }
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
        lock.maximum_claim_amount,
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
                div { p { class: "card-eyebrow", "Lock #{lock.lock_id}" } h2 { "{lock.remaining} base units remaining" } }
                span { class: "status-pill", if creator { "Your lock" } else { "Claimable" } }
            }
            p { "{policy_detail}" }
            p { class: "field-hint", "Deposited {lock.total_deposited} · released {lock.total_released}" }
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

#[component]
fn DidsPage(
    active_profile: WalletProfileView,
    pending_identity_request: Signal<Option<PendingIdentityRequest>>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| DidPageState::Loading);
    let mut did_input = use_signal(|| STANDALONE_DID_FIXTURE.to_owned());
    let mut authentication_input = use_signal(String::new);
    let mut prepared_authentication = use_signal(|| None::<SelfIssuedAuthenticationView>);
    let mut authentication_consent = use_signal(|| false);
    let mut authentication_busy = use_signal(|| false);
    let mut authentication_notice = use_signal(|| None::<String>);
    use_effect(move || {
        let pending = pending_identity_request.read().clone();
        if let Some(request) = pending
            && request.kind == IdentityRequestKind::SelfIssuedAuthentication
        {
            authentication_input.set(request.request_uri);
            prepared_authentication.set(None);
            authentication_consent.set(false);
            authentication_notice.set(Some(
                "Imported login request loaded. Preview it before authenticating.".to_owned(),
            ));
        }
    });
    let profile_id = active_profile.id.clone();
    let load_services = services.clone();
    let load_profile = profile_id.clone();
    use_effect(move || {
        let services = load_services.clone();
        let profile_id = load_profile.clone();
        spawn(async move {
            state.set(
                run_ui_blocking(move || load_did_page(&services, &profile_id))
                    .await
                    .unwrap_or_else(|error| DidPageState::Failed(error.to_string())),
            );
        });
    });

    let state_snapshot = state.read().clone();
    match state_snapshot {
        DidPageState::Loading => rsx! {
            section { class: "page-heading",
                p { class: "eyebrow", "Decentralized identity" }
                h1 { "Your DIDs" }
                p { "Loading public DID records for this wallet profile…" }
            }
        },
        DidPageState::Failed(message) => rsx! {
            section { class: "page-heading",
                p { class: "eyebrow", "Decentralized identity" }
                h1 { "Your DIDs" }
                p { "DID inventory is an independently composed identity capability." }
            }
            article { class: "empty-state surface-card", role: "alert",
                span { class: "empty-state__mark", aria_hidden: "true", "◇" }
                h2 { "DID capability unavailable" }
                p { "{message}" }
                button {
                    class: "secondary-action", r#type: "button",
                    onclick: move |_| {
                        let services = services.clone();
                        let profile_id = profile_id.clone();
                        state.set(DidPageState::Loading);
                        spawn(async move {
                            state.set(
                                run_ui_blocking(move || {
                                    load_did_page(&services, &profile_id)
                                })
                                .await
                                .unwrap_or_else(|error| {
                                    DidPageState::Failed(error.to_string())
                                }),
                            );
                        });
                    },
                    "Retry"
                }
            }
        },
        DidPageState::Ready {
            records,
            resolving,
            operation_error,
        } => {
            let can_resolve = !resolving
                && !did_input.read().trim().is_empty()
                && did_input.read().len() <= 8_192;
            let resolve_services = services.clone();
            let resolve_profile = profile_id.clone();
            let retained_records = records.clone();
            let create_services = services.clone();
            let create_profile = profile_id.clone();
            let create_records = records.clone();
            let standalone_authentication_request = services.standalone_self_issued_request();
            rsx! {
                section { class: "page-heading",
                    p { class: "eyebrow", "Decentralized identity" }
                    h1 { "Your DIDs" }
                    p { "Create, resolve, update, sign with, and deactivate standards-shaped did:midnight documents under the active profile." }
                }
                article { class: "surface-card did-resolver-card",
                    p { class: "card-eyebrow", "Managed identity" }
                    h2 { "Create a standalone DID" }
                    p { class: "form-hint", "Creates protected Ed25519 authentication and P-256 assertion keys. Only the public DID document is persisted." }
                    button {
                        class: "primary-action", r#type: "button", disabled: resolving,
                        onclick: move |_| {
                            state.set(DidPageState::Ready { records: create_records.clone(), resolving: true, operation_error: None });
                            let service = create_services.create_did();
                            let profile_id = create_profile.clone();
                            let records = create_records.clone();
                            spawn(async move {
                                let result = run_ui_blocking(move || {
                                    service.execute(CreateDidCommand {
                                        profile_id,
                                        network: "undeployed".to_owned(),
                                    })
                                })
                                .await;
                                match result {
                                    Ok(Ok(record)) => {
                                        let mut updated = records;
                                        updated.retain(|existing| existing.document.id != record.document.id);
                                        updated.push(record);
                                        updated.sort_by(|left, right| left.document.id.cmp(&right.document.id));
                                        state.set(DidPageState::Ready { records: updated, resolving: false, operation_error: None });
                                    }
                                    Ok(Err(error)) => state.set(DidPageState::Ready {
                                        records, resolving: false,
                                        operation_error: Some(did_operation_message(error)),
                                    }),
                                    Err(error) => state.set(DidPageState::Ready {
                                        records, resolving: false,
                                        operation_error: Some(error.to_string()),
                                    }),
                                }
                            });
                        },
                        if resolving { "Working…" } else { "Create standalone DID" }
                    }
                }
                article { class: "surface-card did-resolver-card",
                    p { class: "card-eyebrow", "SIOPv2 draft 13 · standalone" }
                    h2 { "Authenticate with a DID" }
                    p { class: "form-hint", "Preview the verifier and purpose before consent. This flow proves control of a managed DID; it does not disclose a credential. Nonce, state, and the signed ID token remain inside the protocol adapter." }
                    label { r#for: "self-issued-authentication-request", "Authentication request URI" }
                    textarea {
                        id: "self-issued-authentication-request",
                        maxlength: 32768,
                        rows: 4,
                        autocomplete: "off",
                        spellcheck: false,
                        value: "{authentication_input}",
                        oninput: move |event| authentication_input.set(event.value()),
                    }
                    if let Some(request) = standalone_authentication_request {
                        button {
                            class: "secondary-action",
                            r#type: "button",
                            disabled: authentication_busy(),
                            onclick: move |_| {
                                authentication_input.set(request.clone());
                                prepared_authentication.set(None);
                                authentication_consent.set(false);
                                authentication_notice.set(Some("Standalone login request loaded. Preview it before authenticating.".to_owned()));
                            },
                            "Use standalone login request"
                        }
                    }
                    button {
                        class: "primary-action",
                        r#type: "button",
                        disabled: authentication_busy() || authentication_input.read().trim().is_empty(),
                        onclick: {
                            let service = services.prepare_self_issued_authentication();
                            let profile_id = profile_id.clone();
                            move |_| {
                                let service = service.clone();
                                let profile_id = profile_id.clone();
                                let request = authentication_input.read().trim().to_owned();
                                authentication_busy.set(true);
                                authentication_notice.set(None);
                                spawn(async move {
                                    match run_ui_future(async move {
                                        service.execute(PrepareSelfIssuedAuthenticationCommand { profile_id, request }).await
                                    })
                                    .await
                                    {
                                        Ok(Ok(preview)) => {
                                            prepared_authentication.set(Some(preview));
                                            authentication_consent.set(false);
                                            authentication_notice.set(Some("Login preview ready. Review the verifier and purpose before consenting.".to_owned()));
                                        }
                                        Ok(Err(error)) => {
                                            prepared_authentication.set(None);
                                            authentication_notice.set(Some(self_issued_authentication_message(error)));
                                        }
                                        Err(error) => {
                                            prepared_authentication.set(None);
                                            authentication_notice.set(Some(error.to_string()));
                                        }
                                    }
                                    authentication_busy.set(false);
                                });
                            }
                        },
                        if authentication_busy() { "Checking request…" } else { "Preview login request" }
                    }
                    if let Some(preview) = prepared_authentication.read().clone() {
                        div { class: "credential-offer-preview",
                            h3 { "DID authentication preview" }
                            dl { class: "did-record__facts",
                                div { dt { "Verifier" } dd { title: "{preview.verifier}", "{preview.verifier}" } }
                                div { dt { "Purpose" } dd { "{preview.purpose}" } }
                                div { dt { "State" } dd { {preview.state.replace('_', " ")} } }
                            }
                            if preview.state == "awaiting_consent" {
                                label { class: "confirmation-check",
                                    input {
                                        id: "self-issued-authentication-consent",
                                        r#type: "checkbox",
                                        aria_label: "Consent to DID authentication",
                                        checked: authentication_consent(),
                                        onchange: move |event| authentication_consent.set(event.checked()),
                                    }
                                    span { "I reviewed this verifier and consent to authenticate with my active managed DID." }
                                }
                                div { class: "action-row",
                                    button {
                                        class: "primary-action",
                                        r#type: "button",
                                        disabled: authentication_busy() || !authentication_consent(),
                                        onclick: {
                                            let service = services.accept_self_issued_authentication();
                                            let profile_id = profile_id.clone();
                                            let authentication_id = preview.id.clone();
                                            let records = records.clone();
                                            move |_| {
                                                let Some((holder_did, method_id)) = active_managed_authentication_method(&records) else {
                                                    authentication_notice.set(Some("Create an active managed DID before authenticating.".to_owned()));
                                                    return;
                                                };
                                                let service = service.clone();
                                                let profile_id = profile_id.clone();
                                                let authentication_id = authentication_id.clone();
                                                authentication_busy.set(true);
                                                authentication_notice.set(None);
                                                spawn(async move {
                                                    match run_ui_future(async move {
                                                        service.execute(AcceptSelfIssuedAuthenticationCommand {
                                                            profile_id,
                                                            authentication_id,
                                                            holder_did,
                                                            method_id,
                                                            confirmed: true,
                                                            intent: "ACCEPT_SELF_ISSUED_AUTHENTICATION".to_owned(),
                                                        }).await
                                                    })
                                                    .await
                                                    {
                                                        Ok(Ok(result)) => {
                                                            prepared_authentication.set(Some(result));
                                                            authentication_notice.set(Some("DID authentication succeeded and the standalone verifier independently validated the proof.".to_owned()));
                                                        }
                                                        Ok(Err(error)) => authentication_notice.set(Some(self_issued_authentication_message(error))),
                                                        Err(error) => authentication_notice.set(Some(error.to_string())),
                                                    }
                                                    authentication_busy.set(false);
                                                });
                                            }
                                        },
                                        if authentication_busy() { "Authenticating…" } else { "Authenticate with DID" }
                                    }
                                    button {
                                        class: "secondary-action",
                                        r#type: "button",
                                        disabled: authentication_busy(),
                                        onclick: {
                                            let service = services.refuse_self_issued_authentication();
                                            let profile_id = profile_id.clone();
                                            let authentication_id = preview.id.clone();
                                            move |_| {
                                                let service = service.clone();
                                                let profile_id = profile_id.clone();
                                                let authentication_id = authentication_id.clone();
                                                authentication_busy.set(true);
                                                authentication_notice.set(None);
                                                spawn(async move {
                                                    let result = run_ui_blocking(move || {
                                                        service.execute(RefuseSelfIssuedAuthenticationCommand {
                                                            profile_id,
                                                            authentication_id,
                                                        })
                                                    })
                                                    .await;
                                                    match result {
                                                        Ok(Ok(result)) => {
                                                            prepared_authentication.set(Some(result));
                                                            authentication_consent.set(false);
                                                            authentication_notice.set(Some("Login request refused; ephemeral protocol secrets were discarded.".to_owned()));
                                                        }
                                                        Ok(Err(error)) => authentication_notice.set(Some(self_issued_authentication_message(error))),
                                                        Err(error) => authentication_notice.set(Some(error.to_string())),
                                                    }
                                                    authentication_busy.set(false);
                                                });
                                            }
                                        },
                                        "Refuse login"
                                    }
                                }
                            }
                        }
                    }
                    if let Some(message) = authentication_notice.read().as_deref() {
                        p { class: "form-hint", role: "status", "{message}" }
                    }
                }
                article { class: "surface-card did-resolver-card",
                    p { class: "card-eyebrow", "Resolve a DID" }
                    label { r#for: "did-identifier", "Midnight DID" }
                    input {
                        id: "did-identifier", r#type: "text", maxlength: 8192,
                        autocomplete: "off", spellcheck: false,
                        value: "{did_input}",
                        oninput: move |event| did_input.set(event.value()),
                    }
                    p { class: "form-hint", "Standalone mode recognizes the documented fixture shown by default. A live resolver is used only when its base URL is explicitly configured." }
                    button {
                        class: "primary-action", r#type: "button", disabled: !can_resolve,
                        onclick: move |_| {
                            state.set(DidPageState::Ready { records: retained_records.clone(), resolving: true, operation_error: None });
                            let service = resolve_services.resolve_did();
                            let profile_id = resolve_profile.clone();
                            let did = did_input.read().trim().to_owned();
                            let mut records = retained_records.clone();
                            spawn(async move {
                                match run_ui_future(async move {
                                    service.execute(ResolveDidCommand { profile_id, did }).await
                                })
                                .await
                                {
                                    Ok(Ok(record)) => {
                                        records.retain(|existing| existing.document.id != record.document.id);
                                        records.push(record);
                                        records.sort_by(|left, right| left.document.id.cmp(&right.document.id));
                                        state.set(DidPageState::Ready { records, resolving: false, operation_error: None });
                                    }
                                    Ok(Err(error)) => state.set(DidPageState::Ready { records, resolving: false, operation_error: Some(did_operation_message(error)) }),
                                    Err(error) => state.set(DidPageState::Ready { records, resolving: false, operation_error: Some(error.to_string()) }),
                                }
                            });
                        },
                        if resolving { "Resolving…" } else { "Resolve and save" }
                    }
                    if let Some(error) = operation_error {
                        p { class: "field-error", role: "alert", "{error}" }
                    }
                }
                if records.is_empty() {
                    article { class: "empty-state surface-card",
                        span { class: "empty-state__mark", aria_hidden: "true", "◇" }
                        h2 { "No saved DIDs" }
                        p { "Resolve a did:midnight identifier to add its public document to this profile." }
                        span { class: "status-pill", "Profile scoped" }
                    }
                } else {
                    section { class: "did-inventory", aria_label: "Saved decentralized identifiers",
                        for record in records.clone() {
                            {
                                let did = record.document.id.clone();
                                let forget_did = did.clone();
                                let forget_profile = profile_id.clone();
                                let forget_services = services.clone();
                                let retained = records.clone();
                                let source = record.source.clone();
                                let version = record.document_metadata.version_id.clone().unwrap_or_else(|| "Unversioned".to_owned());
                                rsx! {
                                    article { class: "surface-card did-record", key: "{did}",
                                        div { class: "did-record__heading",
                                            div {
                                                p { class: "card-eyebrow", "{record.document.network} · {source}" }
                                                h2 { title: "{did}", "{truncate_middle(&did, 22, 12)}" }
                                            }
                                            span { class: if record.document_metadata.deactivated == Some(true) { "status-pill" } else { "status-pill success" },
                                                if record.document_metadata.deactivated == Some(true) { "Deactivated" } else { "Resolved" }
                                            }
                                        }
                                        dl { class: "did-record__facts",
                                            div { dt { "Version" } dd { "{version}" } }
                                            div { dt { "Public methods" } dd { "{record.document.verification_methods.len()}" } }
                                            div { dt { "Services" } dd { "{record.document.services.len()}" } }
                                        }
                                        if !record.document.verification_methods.is_empty() {
                                            ul { class: "did-method-list",
                                                for method in record.document.verification_methods.clone() {
                                                    li { key: "{method.id}",
                                                        strong { "{method.public_key_jwk.curve}" }
                                                        code { title: "{method.id}", "{truncate_middle(&method.id, 16, 8)}" }
                                                    }
                                                }
                                            }
                                        }
                                        {
                                            let managed_did = did.clone();
                                            let retained = records.clone();
                                            rsx! {
                                                ManagedDidControls {
                                                    profile_id: profile_id.clone(),
                                                    record: record.clone(),
                                                    on_record: move |result: Result<DidRecordView, String>| {
                                                        match result {
                                                            Ok(updated) => {
                                                                let mut next = retained.clone();
                                                                next.retain(|entry| entry.document.id != managed_did);
                                                                next.push(updated);
                                                                next.sort_by(|left, right| left.document.id.cmp(&right.document.id));
                                                                state.set(DidPageState::Ready { records: next, resolving: false, operation_error: None });
                                                            }
                                                            Err(message) => state.set(DidPageState::Ready { records: retained.clone(), resolving: false, operation_error: Some(message) }),
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        button {
                                            class: "secondary-action", r#type: "button",
                                            aria_label: "Forget saved DID {did}",
                                            onclick: move |_| {
                                                let service = forget_services.forget_did();
                                                let profile_id = forget_profile.clone();
                                                let did = forget_did.clone();
                                                let target = did.clone();
                                                let records = retained.clone();
                                                state.set(DidPageState::Ready { records: records.clone(), resolving: true, operation_error: None });
                                                spawn(async move {
                                                    let result = run_ui_blocking(move || {
                                                        service.execute(DidRecordQuery {
                                                            profile_id,
                                                            did,
                                                        })
                                                    })
                                                    .await;
                                                    match result {
                                                        Ok(Ok(())) => state.set(DidPageState::Ready {
                                                            records: records.iter().filter(|record| record.document.id != target).cloned().collect(),
                                                            resolving: false,
                                                            operation_error: None,
                                                        }),
                                                        Ok(Err(error)) => state.set(DidPageState::Ready {
                                                            records,
                                                            resolving: false,
                                                            operation_error: Some(did_operation_message(error)),
                                                        }),
                                                        Err(error) => state.set(DidPageState::Ready {
                                                            records,
                                                            resolving: false,
                                                            operation_error: Some(error.to_string()),
                                                        }),
                                                    }
                                                });
                                            },
                                            "Forget from profile"
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

fn load_credential_page(services: &WalletUiServices, profile_id: &str) -> CredentialPageState {
    services
        .list_credentials()
        .execute(CredentialProfileQuery {
            profile_id: profile_id.to_owned(),
        })
        .map_or_else(
            |error| CredentialPageState::Failed(credential_operation_message(error)),
            |credentials| CredentialPageState::Ready {
                credentials,
                receiving: false,
                operation_error: None,
            },
        )
}

fn credential_operation_message(error: CredentialOperationError) -> String {
    error.to_string()
}

fn credential_issuance_message(error: CredentialIssuanceError) -> String {
    error.to_string()
}

fn identity_request_kind_label(kind: IdentityRequestKind) -> &'static str {
    match kind {
        IdentityRequestKind::CredentialIssuance => "a credential offer",
        IdentityRequestKind::SelfIssuedAuthentication => "a DID login",
        IdentityRequestKind::CredentialPresentation => "a credential presentation",
    }
}

fn identity_request_routing_message(error: IdentityRequestRoutingError) -> String {
    match error {
        IdentityRequestRoutingError::InvalidRequest => {
            "The QR code is not a valid bounded identity request.".to_owned()
        }
        IdentityRequestRoutingError::UnsupportedRequest => {
            "This QR code does not contain a supported identity protocol link.".to_owned()
        }
        IdentityRequestRoutingError::AmbiguousRequest => {
            "This OpenID4VP endpoint is not registered, so the wallet will not guess whether it is a login or presentation request.".to_owned()
        }
        IdentityRequestRoutingError::Unavailable => {
            "Identity request routing is unavailable in this wallet composition.".to_owned()
        }
    }
}

fn route_pending_identity_link(
    services: &WalletUiServices,
    mut pending_identity_request: Signal<Option<PendingIdentityRequest>>,
    mut active_destination: Signal<Destination>,
    mut menu_open: Signal<bool>,
    mut notice: Signal<Option<String>>,
) {
    if pending_identity_request.read().is_some() {
        return;
    }
    let ingress = services.identity_link_ingress();
    let link = match ingress.take_pending() {
        Ok(Some(link)) => link,
        Ok(None) => return,
        Err(error) => {
            notice.set(Some(identity_link_ingress_message(error)));
            return;
        }
    };
    let request_uri = link.into_inner();
    match services
        .route_identity_request()
        .execute(RouteIdentityRequestCommand {
            request_uri: request_uri.clone(),
        }) {
        Ok(kind) => {
            pending_identity_request.set(Some(PendingIdentityRequest { kind, request_uri }));
            active_destination.set(identity_request_destination(kind));
            menu_open.set(false);
            notice.set(Some(format!(
                "App link recognized as {}. Review the request before consent.",
                identity_request_kind_label(kind)
            )));
        }
        Err(error) => notice.set(Some(identity_request_routing_message(error))),
    }
}

fn identity_link_ingress_message(error: IdentityLinkIngressError) -> String {
    match error {
        IdentityLinkIngressError::Unavailable => {
            "Identity app links are unavailable on this device. Paste or scan the request instead."
                .to_owned()
        }
        IdentityLinkIngressError::InvalidLink => {
            "The operating system delivered an invalid or oversized identity app link."
                .to_owned()
        }
        IdentityLinkIngressError::QueueFull => {
            "Another identity app link is already waiting for review; finish it before opening a new one."
                .to_owned()
        }
        IdentityLinkIngressError::Failed => {
            "Identity app-link ingress failed; no request was imported.".to_owned()
        }
    }
}

fn qr_scan_message(error: QrScanError) -> String {
    match error {
        QrScanError::Cancelled => "QR scan cancelled.".to_owned(),
        QrScanError::Unavailable => {
            "Camera scanning is unavailable here. Paste or load the request in the identity page instead.".to_owned()
        }
        QrScanError::TimedOut => "QR scan timed out; no request was imported.".to_owned(),
        QrScanError::InvalidPayload => {
            "The QR payload is empty or exceeds the identity request limit.".to_owned()
        }
        QrScanError::Failed => "QR scanning failed; no request was imported.".to_owned(),
    }
}

fn credential_presentation_message(error: CredentialPresentationError) -> String {
    match error {
        CredentialPresentationError::Protocol(PresentationProtocolError::HolderNotAuthorized) =>
            "The credential's bound DID method is no longer active and controlled by this wallet. Nothing was presented and no vp_token was generated.".to_owned(),
        CredentialPresentationError::Protocol(PresentationProtocolError::HolderAuthorizationUnavailable) =>
            "Unlock the wallet and make the bound DID holder method available before presenting. Nothing was presented and no vp_token was generated.".to_owned(),
        CredentialPresentationError::Protocol(PresentationProtocolError::ProofUnavailable) =>
            "The holder authorized this exact presentation, but Compact proving is unavailable. No presentation or vp_token was generated.".to_owned(),
        other => other.to_string(),
    }
}

#[component]
fn CredentialPresentationPanel(
    profile_id: String,
    pending_identity_request: Signal<Option<PendingIdentityRequest>>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut request_input = use_signal(String::new);
    let mut preview = use_signal(|| None::<CredentialPresentationView>);
    let mut consent = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);
    use_effect(move || {
        let pending = pending_identity_request.read().clone();
        if let Some(request) = pending
            && request.kind == IdentityRequestKind::CredentialPresentation
        {
            request_input.set(request.request_uri);
            preview.set(None);
            consent.set(false);
            notice.set(Some(
                "Imported presentation request loaded. Preview it before consenting.".to_owned(),
            ));
        }
    });
    let demo_request = services.standalone_openid4vp_request();

    rsx! {
        article { class: "surface-card credential-receive-card",
            p { class: "card-eyebrow", "OpenID4VP 1.0 Final · DCQL" }
            h2 { "Present a Digital Passport" }
            p { class: "form-hint",
                "Inspect the verifier, purpose, and exact requested claims before consent. Claim values stay protected while previewing."
            }
            label { r#for: "openid4vp-request", "OpenID4VP request URI" }
            textarea {
                id: "openid4vp-request",
                maxlength: 65536,
                rows: 4,
                autocomplete: "off",
                spellcheck: false,
                value: "{request_input}",
                oninput: move |event| request_input.set(event.value()),
            }
            if let Some(request) = demo_request {
                button {
                    class: "secondary-action",
                    r#type: "button",
                    disabled: busy(),
                    onclick: move |_| {
                        request_input.set(request.clone());
                        preview.set(None);
                        consent.set(false);
                        notice.set(Some("Standalone verifier request loaded. Preview it before consenting.".to_owned()));
                    },
                    "Use standalone verifier request"
                }
            }
            button {
                class: "primary-action",
                r#type: "button",
                disabled: busy() || request_input.read().trim().is_empty(),
                onclick: {
                    let service = services.prepare_credential_presentation();
                    let profile_id = profile_id.clone();
                    move |_| {
                        let service = service.clone();
                        let profile_id = profile_id.clone();
                        let request = request_input.read().trim().to_owned();
                        busy.set(true);
                        notice.set(None);
                        spawn(async move {
                            match run_ui_future(async move {
                                service.execute(PrepareCredentialPresentationCommand { profile_id, request }).await
                            })
                            .await
                            {
                                Ok(Ok(result)) => {
                                    preview.set(Some(result));
                                    consent.set(false);
                                    notice.set(Some("Request preview ready. Nothing has been presented.".to_owned()));
                                }
                                Ok(Err(error)) => {
                                    preview.set(None);
                                    notice.set(Some(credential_presentation_message(error)));
                                }
                                Err(error) => {
                                    preview.set(None);
                                    notice.set(Some(error.to_string()));
                                }
                            }
                            busy.set(false);
                        });
                    }
                },
                if busy() { "Checking request…" } else { "Preview presentation request" }
            }
            if let Some(presentation) = preview.read().clone() {
                div { class: "credential-offer-preview",
                    h3 { "Presentation preview" }
                    dl { class: "credential-record__facts",
                        div { dt { "Verifier" } dd { title: "{presentation.verifier}", "{presentation.verifier}" } }
                        div { dt { "Purpose" } dd { "{presentation.purpose}" } }
                        div { dt { "State" } dd { {presentation.state.replace('_', " ")} } }
                    }
                    h4 { "Requested claims" }
                    ul { class: "credential-stage-list", aria_label: "Requested presentation claims",
                        for claim in presentation.requested_claims.clone() {
                            li { key: "{claim.claim_path}",
                                span { "{claim.label}" }
                                strong { "{claim.intent}" }
                                if let Some(kind) = claim.predicate_kind {
                                    small { "{kind} {claim.threshold.unwrap_or_default()}" }
                                }
                            }
                        }
                    }
                    if presentation.candidates.is_empty() {
                        p { class: "field-error", role: "alert", "No matching Digital Passport is available in this profile." }
                    } else if presentation.state == "awaiting_consent" {
                        label { class: "confirmation-check",
                            input {
                                id: "credential-presentation-consent",
                                r#type: "checkbox",
                                aria_label: "Consent to credential presentation",
                                checked: consent(),
                                onchange: move |event| consent.set(event.checked()),
                            }
                            span { "I consent to disclose exactly these claims to this verifier." }
                        }
                        div { class: "action-row",
                            button {
                                class: "primary-action",
                                r#type: "button",
                                disabled: busy() || !consent(),
                                onclick: {
                                    let service = services.accept_credential_presentation();
                                    let profile_id = profile_id.clone();
                                    let presentation_id = presentation.id.clone();
                                    let credential_id = presentation.candidates[0].credential_id.clone();
                                    move |_| {
                                        let service = service.clone();
                                        let profile_id = profile_id.clone();
                                        let presentation_id = presentation_id.clone();
                                        let credential_id = credential_id.clone();
                                        busy.set(true);
                                        notice.set(None);
                                        spawn(async move {
                                            match run_ui_future(async move {
                                                service.execute(AcceptCredentialPresentationCommand {
                                                    profile_id,
                                                    presentation_id,
                                                    credential_id,
                                                    confirmed: true,
                                                    intent: "ACCEPT_CREDENTIAL_PRESENTATION".to_owned(),
                                                }).await
                                            })
                                            .await
                                            {
                                                Ok(Ok(result)) => {
                                                    preview.set(Some(result));
                                                    notice.set(Some("Presentation generated and independently verified.".to_owned()));
                                                }
                                                Ok(Err(error)) => {
                                                    let failed_view = preview.read().clone();
                                                    if let CredentialPresentationError::Protocol(protocol) = &error
                                                        && let Some(mut failed) = failed_view {
                                                        failed.state = "failed".to_owned();
                                                        failed.presentation_generated = false;
                                                        failed.verifier_validated = false;
                                                        failed.failure_code = Some(protocol.code().to_owned());
                                                        preview.set(Some(failed));
                                                        consent.set(false);
                                                    }
                                                    notice.set(Some(credential_presentation_message(error)));
                                                }
                                                Err(error) => notice.set(Some(error.to_string())),
                                            }
                                            busy.set(false);
                                        });
                                    }
                                },
                                if busy() { "Generating proof…" } else { "Consent and present" }
                            }
                            button {
                                class: "secondary-action",
                                r#type: "button",
                                disabled: busy(),
                                onclick: {
                                    let service = services.refuse_credential_presentation();
                                    let profile_id = profile_id.clone();
                                    let presentation_id = presentation.id.clone();
                                    move |_| {
                                        let service = service.clone();
                                        let profile_id = profile_id.clone();
                                        let presentation_id = presentation_id.clone();
                                        busy.set(true);
                                        notice.set(None);
                                        spawn(async move {
                                            let result = run_ui_blocking(move || {
                                                service.execute(RefuseCredentialPresentationCommand {
                                                    profile_id,
                                                    presentation_id,
                                                })
                                            })
                                            .await;
                                            match result {
                                                Ok(Ok(result)) => {
                                                    preview.set(Some(result));
                                                    consent.set(false);
                                                    notice.set(Some("Presentation refused; the one-time verifier session was discarded.".to_owned()));
                                                }
                                                Ok(Err(error)) => notice.set(Some(credential_presentation_message(error))),
                                                Err(error) => notice.set(Some(error.to_string())),
                                            }
                                            busy.set(false);
                                        });
                                    }
                                },
                                "Refuse request"
                            }
                        }
                    }
                    if !presentation.presentation_generated {
                        p { class: "form-hint", "No presentation or vp_token has been generated." }
                    }
                }
            }
            if let Some(message) = notice.read().as_deref() {
                p { class: "form-hint", role: "status", "{message}" }
            }
        }
    }
}

enum CredentialChange {
    Updated(CredentialView),
    Deleted(String),
    Failed(String),
}

const PASSPORT_FIRST_NAME: &str = "/credentialSubject/firstName";
const PASSPORT_LAST_NAME: &str = "/credentialSubject/lastName";
const PASSPORT_DATE_OF_BIRTH: &str = "/credentialSubject/dateOfBirth";

#[component]
fn DigitalPassportClaims(profile_id: String, credential_id: String) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut disclosure_state = use_signal(|| None::<Result<CredentialDisclosureView, String>>);
    let mut revealed_first = use_signal(|| None::<String>);
    let mut revealed_last = use_signal(|| None::<String>);
    let mut age_threshold = use_signal(|| 18_u8);
    let mut plan_notice = use_signal(|| None::<String>);
    let mut operation_busy = use_signal(|| false);
    let load_service = services.get_credential_disclosure();
    let load_profile = profile_id.clone();
    let load_credential = credential_id.clone();
    use_effect(move || {
        let service = load_service.clone();
        let profile_id = load_profile.clone();
        let credential_id = load_credential.clone();
        spawn(async move {
            let result = run_ui_blocking(move || {
                service
                    .execute(CredentialDisclosureQuery {
                        profile_id,
                        credential_id,
                    })
                    .map_err(credential_operation_message)
            })
            .await;
            disclosure_state.set(Some(match result {
                Ok(result) => result,
                Err(error) => Err(error.to_string()),
            }));
        });
    });

    match disclosure_state.read().clone() {
        None => rsx! {
            section { class: "passport-claims", aria_label: "Digital Passport protected claims",
                p { class: "form-hint", "Validating protected claim commitments…" }
            }
        },
        Some(Err(message)) => rsx! {
            section { class: "passport-claims", aria_label: "Digital Passport protected claims",
                p { class: "field-error", role: "alert", "Protected claims unavailable: {message}" }
            }
        },
        Some(Ok(disclosure)) => {
            let first = disclosure
                .candidates
                .iter()
                .find(|candidate| candidate.claim_path == PASSPORT_FIRST_NAME)
                .cloned();
            let last = disclosure
                .candidates
                .iter()
                .find(|candidate| candidate.claim_path == PASSPORT_LAST_NAME)
                .cloned();
            let date_of_birth = disclosure
                .candidates
                .iter()
                .find(|candidate| candidate.claim_path == PASSPORT_DATE_OF_BIRTH)
                .cloned();
            let first_service = services.reveal_credential_claim();
            let last_service = services.reveal_credential_claim();
            let preview_service = services.preview_credential_disclosure();
            let first_profile = profile_id.clone();
            let first_credential = credential_id.clone();
            let last_profile = profile_id.clone();
            let last_credential = credential_id.clone();
            let preview_profile = profile_id;
            let preview_credential = credential_id;
            rsx! {
                section { class: "passport-claims", aria_label: "Digital Passport protected claims",
                    div { class: "passport-claims__heading",
                        div {
                            p { class: "card-eyebrow", "{disclosure.schema_id}" }
                            h3 { "Available proofs" }
                        }
                        span { class: "status-pill", "Holder controlled" }
                    }
                    p { class: "form-hint",
                        "Reveal is local to this screen. Preview builds no verifier presentation and sends nothing."
                    }
                    if let Some(candidate) = first {
                        article { class: "passport-claim",
                            div {
                                span { class: "passport-claim__tier", "{candidate.privacy_tier}" }
                                h4 { "{candidate.label}" }
                                if let Some(value) = revealed_first.read().as_deref() {
                                    p { class: "passport-claim__value", "{value}" }
                                } else {
                                    p { "Encrypted until locally revealed." }
                                }
                            }
                            button {
                                class: "secondary-action", r#type: "button",
                                disabled: operation_busy(),
                                aria_label: if revealed_first.read().is_some() { "Hide First name" } else { "Reveal First name locally" },
                                onclick: move |_| {
                                    if revealed_first.read().is_some() {
                                        revealed_first.set(None);
                                    } else {
                                        let service = first_service.clone();
                                        let profile_id = first_profile.clone();
                                        let credential_id = first_credential.clone();
                                        operation_busy.set(true);
                                        spawn(async move {
                                            let result = run_ui_blocking(move || {
                                                service.execute(RevealCredentialClaimCommand {
                                                    profile_id,
                                                    credential_id,
                                                    claim_path: PASSPORT_FIRST_NAME.to_owned(),
                                                })
                                            })
                                            .await;
                                            match result {
                                                Ok(Ok(claim)) => {
                                                    revealed_first.set(Some(claim.value().to_owned()));
                                                    plan_notice.set(Some("First name revealed only on this device screen.".to_owned()));
                                                }
                                                Ok(Err(error)) => plan_notice.set(Some(credential_operation_message(error))),
                                                Err(error) => plan_notice.set(Some(error.to_string())),
                                            }
                                            operation_busy.set(false);
                                        });
                                    }
                                },
                                if revealed_first.read().is_some() { "Hide" } else { "Reveal locally" }
                            }
                        }
                    }
                    if let Some(candidate) = last {
                        article { class: "passport-claim",
                            div {
                                span { class: "passport-claim__tier", "{candidate.privacy_tier}" }
                                h4 { "{candidate.label}" }
                                if let Some(value) = revealed_last.read().as_deref() {
                                    p { class: "passport-claim__value", "{value}" }
                                } else {
                                    p { "Encrypted until locally revealed." }
                                }
                            }
                            button {
                                class: "secondary-action", r#type: "button",
                                disabled: operation_busy(),
                                aria_label: if revealed_last.read().is_some() { "Hide Last name" } else { "Reveal Last name locally" },
                                onclick: move |_| {
                                    if revealed_last.read().is_some() {
                                        revealed_last.set(None);
                                    } else {
                                        let service = last_service.clone();
                                        let profile_id = last_profile.clone();
                                        let credential_id = last_credential.clone();
                                        operation_busy.set(true);
                                        spawn(async move {
                                            let result = run_ui_blocking(move || {
                                                service.execute(RevealCredentialClaimCommand {
                                                    profile_id,
                                                    credential_id,
                                                    claim_path: PASSPORT_LAST_NAME.to_owned(),
                                                })
                                            })
                                            .await;
                                            match result {
                                                Ok(Ok(claim)) => {
                                                    revealed_last.set(Some(claim.value().to_owned()));
                                                    plan_notice.set(Some("Last name revealed only on this device screen.".to_owned()));
                                                }
                                                Ok(Err(error)) => plan_notice.set(Some(credential_operation_message(error))),
                                                Err(error) => plan_notice.set(Some(error.to_string())),
                                            }
                                            operation_busy.set(false);
                                        });
                                    }
                                },
                                if revealed_last.read().is_some() { "Hide" } else { "Reveal locally" }
                            }
                        }
                    }
                    if let Some(candidate) = date_of_birth {
                        article { class: "passport-claim predicate",
                            div {
                                span { class: "passport-claim__tier predicate", "{candidate.privacy_tier}" }
                                h4 { "Date of birth" }
                                p { "Never reveals the date. Plans only an age-over-threshold predicate." }
                            }
                            label { class: "passport-threshold",
                                span { "Age over" }
                                input {
                                    r#type: "number", min: "1", max: "120", inputmode: "numeric",
                                    aria_label: "Age threshold",
                                    value: "{age_threshold}",
                                    oninput: move |event| {
                                        if let Ok(value) = event.value().parse::<u8>()
                                            && (1..=120).contains(&value)
                                        {
                                            age_threshold.set(value);
                                            plan_notice.set(None);
                                        }
                                    },
                                }
                            }
                        }
                    }
                    button {
                        class: "primary-action", r#type: "button",
                        disabled: operation_busy(),
                        onclick: move |_| {
                            let mut reveal_claim_paths = Vec::new();
                            if revealed_first.read().is_some() {
                                reveal_claim_paths.push(PASSPORT_FIRST_NAME.to_owned());
                            }
                            if revealed_last.read().is_some() {
                                reveal_claim_paths.push(PASSPORT_LAST_NAME.to_owned());
                            }
                            let service = preview_service.clone();
                            let profile_id = preview_profile.clone();
                            let credential_id = preview_credential.clone();
                            let threshold = age_threshold();
                            operation_busy.set(true);
                            spawn(async move {
                                let result = run_ui_blocking(move || {
                                    service.execute(PreviewCredentialDisclosureCommand {
                                        profile_id,
                                        credential_id,
                                        reveal_claim_paths,
                                        predicates: vec![CredentialPredicateInput {
                                            claim_path: PASSPORT_DATE_OF_BIRTH.to_owned(),
                                            kind: "age_over".to_owned(),
                                            threshold,
                                        }],
                                    })
                                })
                                .await;
                                plan_notice.set(Some(match result {
                                    Ok(Ok(plan)) => format!(
                                        "{} · local preview only · no presentation generated",
                                        plan.outcome.replace('_', " ")
                                    ),
                                    Ok(Err(error)) => credential_operation_message(error),
                                    Err(error) => error.to_string(),
                                }));
                                operation_busy.set(false);
                            });
                        },
                        if operation_busy() { "Working…" } else { "Preview disclosure plan" }
                    }
                    if let Some(message) = plan_notice.read().as_deref() {
                        p { class: "form-hint", role: "status", "{message}" }
                    }
                }
            }
        }
    }
}

#[component]
fn CredentialRecordCard(
    profile_id: String,
    credential: CredentialView,
    on_change: EventHandler<CredentialChange>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut working = use_signal(|| false);
    let mut delete_confirmed = use_signal(|| false);
    let identifier = credential.id.clone();
    let verify_id = identifier.clone();
    let delete_id = identifier.clone();
    let verify_services = services.clone();
    let delete_services = services;
    let verify_profile = profile_id.clone();
    let delete_profile = profile_id.clone();
    let issuer = truncate_middle(&credential.issuer_did, 20, 12);
    let policy_summary = compact_credential_policy_summary(&credential);
    let outcome = credential.verification_outcome.clone();
    let status_class = if outcome == "valid" {
        "status-pill success"
    } else {
        "status-pill warning"
    };
    rsx! {
        article { class: "surface-card credential-record", key: "{identifier}",
            div { class: "credential-record__heading",
                div {
                    p { class: "card-eyebrow", "{credential.format}" }
                    h2 { "{credential.display_name}" }
                }
                span { class: status_class, "{outcome}" }
            }
            dl { class: "credential-record__facts",
                div { dt { "Issuer" } dd { title: "{credential.issuer_did}", "{issuer}" } }
                div { dt { "Subject" } dd {
                    if let Some(subject) = credential.subject_did.as_deref() {
                        "{truncate_middle(subject, 16, 10)}"
                    } else {
                        "Not disclosed"
                    }
                } }
                div { dt { "Issued" } dd {
                    if let Some(timestamp) = credential.issued_at_ms {
                        "{timestamp} ms"
                    } else {
                        "Not supplied"
                    }
                } }
            }
            if let Some(summary) = policy_summary {
                p { class: "form-hint credential-policy-summary", role: "status", "{summary}" }
            }
            if credential.display_name == "Digital Passport" {
                DigitalPassportClaims {
                    profile_id: profile_id.clone(),
                    credential_id: identifier.clone(),
                }
            }
            ul { class: "credential-stage-list", aria_label: "Verification stages",
                for stage in credential.verification_stages.clone() {
                    {
                        let status_label = stage.status.replace('_', " ");
                        let reason_label = stage.reason_code.as_deref().map(|reason| reason.replace('_', " "));
                        rsx! {
                            li { key: "{stage.name}",
                                span { "{stage.name}" }
                                strong { class: if stage.status == "passed" { "stage-passed" } else if stage.status == "failed" { "stage-failed" } else { "stage-pending" },
                                    "{status_label}"
                                }
                                if let Some(reason) = reason_label {
                                    small { "{reason}" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "credential-actions",
                button {
                    class: "secondary-action", r#type: "button", disabled: working(),
                    onclick: move |_| {
                        working.set(true);
                        let service = verify_services.reverify_credential();
                        let profile_id = verify_profile.clone();
                        let credential_id = verify_id.clone();
                        spawn(async move {
                            let result = run_ui_future(async move {
                                service.execute(CredentialQuery { profile_id, credential_id }).await
                            })
                            .await;
                            working.set(false);
                            on_change.call(match result {
                                Ok(Ok(credential)) => CredentialChange::Updated(credential),
                                Ok(Err(error)) => CredentialChange::Failed(credential_operation_message(error)),
                                Err(error) => CredentialChange::Failed(error.to_string()),
                            });
                        });
                    },
                    if working() { "Verifying…" } else { "Reverify" }
                }
                label { class: "confirmation-row credential-delete-confirmation",
                    input {
                        r#type: "checkbox", checked: delete_confirmed(),
                        onchange: move |event| delete_confirmed.set(event.checked()),
                    }
                    "Confirm removal from this profile"
                }
                button {
                    class: "danger-action", r#type: "button",
                    disabled: working() || !delete_confirmed(),
                    onclick: move |_| {
                        let service = delete_services.delete_credential();
                        let profile_id = delete_profile.clone();
                        let credential_id = delete_id.clone();
                        let deleted_id = delete_id.clone();
                        let confirmed = delete_confirmed();
                        working.set(true);
                        spawn(async move {
                            let result = run_ui_blocking(move || {
                                service.execute(DeleteCredentialCommand {
                                    profile_id,
                                    credential_id,
                                    confirmed,
                                    intent: "DELETE_CREDENTIAL".to_owned(),
                                })
                            })
                            .await;
                            working.set(false);
                            on_change.call(match result {
                                Ok(Ok(())) => CredentialChange::Deleted(deleted_id),
                                Ok(Err(error)) => CredentialChange::Failed(credential_operation_message(error)),
                                Err(error) => CredentialChange::Failed(error.to_string()),
                            });
                        });
                    },
                    "Delete credential"
                }
            }
        }
    }
}

fn compact_credential_policy_summary(credential: &CredentialView) -> Option<String> {
    if credential.format != "midnight_compact_vc" {
        return None;
    }
    let status = |name: &str| {
        credential
            .verification_stages
            .iter()
            .find(|stage| stage.name == name)
            .map_or("not checked", |stage| match stage.status.as_str() {
                "passed" => "passed",
                "failed" => "failed",
                _ => "not checked",
            })
    };
    Some(format!(
        "Credential policy · issuer {} · time {} · trust {} · revocation {}",
        status("issuer"),
        status("temporal"),
        status("trust"),
        status("status")
    ))
}

#[component]
fn CredentialsPage(
    active_profile: WalletProfileView,
    pending_identity_request: Signal<Option<PendingIdentityRequest>>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| CredentialPageState::Loading);
    let mut offer_input = use_signal(String::new);
    let mut prepared_issuance = use_signal(|| None::<CredentialIssuanceView>);
    let mut issuance_consent = use_signal(|| false);
    let mut issuance_busy = use_signal(|| false);
    let mut issuance_notice = use_signal(|| None::<String>);
    use_effect(move || {
        let pending = pending_identity_request.read().clone();
        if let Some(request) = pending
            && request.kind == IdentityRequestKind::CredentialIssuance
        {
            offer_input.set(request.request_uri);
            prepared_issuance.set(None);
            issuance_consent.set(false);
            issuance_notice.set(Some(
                "Imported credential offer loaded. Preview it before accepting.".to_owned(),
            ));
        }
    });
    let profile_id = active_profile.id.clone();
    let load_services = services.clone();
    let load_profile = profile_id.clone();
    use_effect(move || {
        let services = load_services.clone();
        let profile_id = load_profile.clone();
        spawn(async move {
            state.set(
                run_ui_blocking(move || load_credential_page(&services, &profile_id))
                    .await
                    .unwrap_or_else(|error| CredentialPageState::Failed(error.to_string())),
            );
        });
    });

    match state.read().clone() {
        CredentialPageState::Loading => rsx! {
            section { class: "page-heading",
                p { class: "eyebrow", "Identity centre" }
                h1 { "Credentials" }
                p { "Loading the protected credential inventory for this wallet profile…" }
            }
        },
        CredentialPageState::Failed(message) => rsx! {
            section { class: "page-heading",
                p { class: "eyebrow", "Identity centre" }
                h1 { "Credentials" }
                p { "Credentials stay local-first, holder-controlled, and separate from chain account state." }
            }
            article { class: "empty-state surface-card", role: "alert",
                span { class: "empty-state__mark", aria_hidden: "true", "◇" }
                h2 { "Credential capability unavailable" }
                p { "{message}" }
                button {
                    class: "secondary-action", r#type: "button",
                    onclick: move |_| {
                        let services = services.clone();
                        let profile_id = profile_id.clone();
                        state.set(CredentialPageState::Loading);
                        spawn(async move {
                            state.set(
                                run_ui_blocking(move || load_credential_page(&services, &profile_id))
                                    .await
                                    .unwrap_or_else(|error| CredentialPageState::Failed(error.to_string())),
                            );
                        });
                    },
                    "Retry"
                }
            }
        },
        CredentialPageState::Ready {
            credentials,
            receiving,
            operation_error,
        } => {
            let receive_service = services.receive_credential();
            let receive_profile = profile_id.clone();
            let retained = credentials.clone();
            let demo_offer = services.standalone_credential_offer();
            rsx! {
                section { class: "page-heading",
                    p { class: "eyebrow", "Identity centre" }
                    h1 { "Credentials" }
                    p { "Protected original bytes, searchable metadata, and explicit verification stages under the active profile." }
                }
                article { class: "surface-card credential-receive-card",
                    p { class: "card-eyebrow", "OpenID4VCI 1.0 Final" }
                    h2 { "Accept a credential offer" }
                    p { class: "form-hint", "Preview an embedded offer before consent. The pre-authorized code, access token, nonce, and signed proof remain inside the protocol adapter." }
                    label { r#for: "credential-offer", "Credential offer URI" }
                    textarea {
                        id: "credential-offer",
                        maxlength: 32768,
                        rows: 4,
                        autocomplete: "off",
                        spellcheck: false,
                        value: "{offer_input}",
                        oninput: move |event| offer_input.set(event.value()),
                    }
                    if let Some(offer) = demo_offer {
                        button {
                            class: "secondary-action",
                            r#type: "button",
                            disabled: issuance_busy(),
                            onclick: move |_| {
                                offer_input.set(offer.clone());
                                prepared_issuance.set(None);
                                issuance_consent.set(false);
                                issuance_notice.set(Some("Standalone credential offer loaded. Preview it before accepting.".to_owned()));
                            },
                            "Use standalone demo offer"
                        }
                    }
                    button {
                        class: "primary-action",
                        r#type: "button",
                        disabled: issuance_busy() || offer_input.read().trim().is_empty(),
                        onclick: {
                            let service = services.prepare_credential_issuance();
                            let profile_id = profile_id.clone();
                            move |_| {
                                let service = service.clone();
                                let profile_id = profile_id.clone();
                                let offer = offer_input.read().trim().to_owned();
                                issuance_busy.set(true);
                                issuance_notice.set(None);
                                spawn(async move {
                                    match run_ui_future(async move {
                                        service.execute(PrepareCredentialIssuanceCommand { profile_id, offer }).await
                                    })
                                    .await
                                    {
                                        Ok(Ok(preview)) => {
                                            prepared_issuance.set(Some(preview));
                                            issuance_consent.set(false);
                                            issuance_notice.set(Some("Offer preview ready. Review the issuer and requested credential before consenting.".to_owned()));
                                        }
                                        Ok(Err(error)) => {
                                            prepared_issuance.set(None);
                                            issuance_notice.set(Some(credential_issuance_message(error)));
                                        }
                                        Err(error) => {
                                            prepared_issuance.set(None);
                                            issuance_notice.set(Some(error.to_string()));
                                        }
                                    }
                                    issuance_busy.set(false);
                                });
                            }
                        },
                        if issuance_busy() { "Checking offer…" } else { "Preview credential offer" }
                    }
                    if let Some(preview) = prepared_issuance.read().clone() {
                        div { class: "credential-offer-preview",
                            h3 { "Credential offer preview" }
                            dl { class: "credential-record__facts",
                                div { dt { "Issuer" } dd { title: "{preview.issuer}", "{preview.issuer}" } }
                                div { dt { "Credential" } dd { {preview.display_names.join(", ")} } }
                                div { dt { "State" } dd { {preview.state.replace('_', " ")} } }
                            }
                            if preview.state == "awaiting_consent" {
                                label { class: "confirmation-check",
                                    input {
                                        id: "credential-issuance-consent",
                                        r#type: "checkbox",
                                        aria_label: "Consent to credential issuance",
                                        checked: issuance_consent(),
                                        onchange: move |event| issuance_consent.set(event.checked()),
                                    }
                                    span { "I reviewed this issuer and consent to receive the credential using my active DID." }
                                }
                                div { class: "action-row",
                                    button {
                                        class: "primary-action",
                                        r#type: "button",
                                        disabled: issuance_busy() || !issuance_consent(),
                                        onclick: {
                                            let services = services.clone();
                                            let profile_id = profile_id.clone();
                                            let issuance_id = preview.id.clone();
                                            move |_| {
                                                let services = services.clone();
                                                let refresh_services = services.clone();
                                                let refresh_profile = profile_id.clone();
                                                let execute_profile = profile_id.clone();
                                                let execute_issuance_id = issuance_id.clone();
                                                issuance_busy.set(true);
                                                issuance_notice.set(None);
                                                spawn(async move {
                                                    let list_service = services.list_did_records();
                                                    let list_profile = execute_profile.clone();
                                                    let records = match run_ui_blocking(move || {
                                                        list_service.execute(ListDidRecordsQuery {
                                                            profile_id: list_profile,
                                                        })
                                                    })
                                                    .await
                                                    {
                                                        Ok(Ok(records)) => records,
                                                        Ok(Err(error)) => {
                                                            issuance_notice.set(Some(did_operation_message(error)));
                                                            issuance_busy.set(false);
                                                            return;
                                                        }
                                                        Err(error) => {
                                                            issuance_notice.set(Some(error.to_string()));
                                                            issuance_busy.set(false);
                                                            return;
                                                        }
                                                    };
                                                    let Some((holder_did, method_id, holder_binding_method_id)) = active_managed_issuance_methods(&records) else {
                                                        issuance_notice.set(Some("Create an active managed DID with protected authentication and Jubjub assertion methods before accepting this credential offer.".to_owned()));
                                                        issuance_busy.set(false);
                                                        return;
                                                    };
                                                    let service = services.accept_credential_issuance();
                                                    match run_ui_future(async move {
                                                        service.execute(AcceptCredentialIssuanceCommand {
                                                            profile_id: execute_profile,
                                                            issuance_id: execute_issuance_id,
                                                            holder_did,
                                                            method_id,
                                                            holder_binding_method_id,
                                                            confirmed: true,
                                                            intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
                                                        }).await
                                                    })
                                                    .await
                                                    {
                                                        Ok(Ok(result)) => {
                                                            prepared_issuance.set(Some(result));
                                                            issuance_notice.set(Some("Credential issued, verified, and stored in the protected inventory.".to_owned()));
                                                            state.set(
                                                                run_ui_blocking(move || {
                                                                    load_credential_page(&refresh_services, &refresh_profile)
                                                                })
                                                                .await
                                                                .unwrap_or_else(|error| CredentialPageState::Failed(error.to_string())),
                                                            );
                                                        }
                                                        Ok(Err(error)) => issuance_notice.set(Some(credential_issuance_message(error))),
                                                        Err(error) => issuance_notice.set(Some(error.to_string())),
                                                    }
                                                    issuance_busy.set(false);
                                                });
                                            }
                                        },
                                        if issuance_busy() { "Issuing credential…" } else { "Accept and issue credential" }
                                    }
                                    button {
                                        class: "secondary-action",
                                        r#type: "button",
                                        disabled: issuance_busy(),
                                        onclick: {
                                            let service = services.refuse_credential_issuance();
                                            let profile_id = profile_id.clone();
                                            let issuance_id = preview.id.clone();
                                            move |_| {
                                                let service = service.clone();
                                                let profile_id = profile_id.clone();
                                                let issuance_id = issuance_id.clone();
                                                issuance_busy.set(true);
                                                issuance_notice.set(None);
                                                spawn(async move {
                                                    let result = run_ui_blocking(move || {
                                                        service.execute(RefuseCredentialIssuanceCommand {
                                                            profile_id,
                                                            issuance_id,
                                                        })
                                                    })
                                                    .await;
                                                    match result {
                                                        Ok(Ok(result)) => {
                                                            prepared_issuance.set(Some(result));
                                                            issuance_consent.set(false);
                                                            issuance_notice.set(Some("Credential offer refused; ephemeral protocol secrets were discarded.".to_owned()));
                                                        }
                                                        Ok(Err(error)) => issuance_notice.set(Some(credential_issuance_message(error))),
                                                        Err(error) => issuance_notice.set(Some(error.to_string())),
                                                    }
                                                    issuance_busy.set(false);
                                                });
                                            }
                                        },
                                        "Refuse offer"
                                    }
                                }
                            }
                        }
                    }
                    if let Some(message) = issuance_notice.read().as_deref() {
                        p { class: "form-hint", role: "status", "{message}" }
                    }
                }
                CredentialPresentationPanel {
                    profile_id: profile_id.clone(),
                    pending_identity_request,
                }
                article { class: "surface-card credential-receive-card",
                    p { class: "card-eyebrow", "Standalone credential inbox" }
                    h2 { "Receive the public identity fixture" }
                    p { class: "form-hint", "Exercises the same storage and cryptographic verification ports as future protocol ingress. It is clearly marked as a standalone development fixture." }
                    button {
                        class: "primary-action", r#type: "button", disabled: receiving,
                        onclick: move |_| {
                            state.set(CredentialPageState::Ready { credentials: retained.clone(), receiving: true, operation_error: None });
                            let service = receive_service.clone();
                            let profile_id = receive_profile.clone();
                            let mut next = retained.clone();
                            spawn(async move {
                                match run_ui_future(async move {
                                    service.execute(CredentialProfileQuery { profile_id }).await
                                })
                                .await
                                {
                                    Ok(Ok(credential)) => {
                                        next.retain(|existing| existing.id != credential.id);
                                        next.push(credential);
                                        next.sort_by(|left, right| left.id.cmp(&right.id));
                                        state.set(CredentialPageState::Ready { credentials: next, receiving: false, operation_error: None });
                                    }
                                    Ok(Err(error)) => state.set(CredentialPageState::Ready { credentials: next, receiving: false, operation_error: Some(credential_operation_message(error)) }),
                                    Err(error) => state.set(CredentialPageState::Ready { credentials: next, receiving: false, operation_error: Some(error.to_string()) }),
                                }
                            });
                        },
                        if receiving { "Receiving and verifying…" } else { "Receive standalone credential" }
                    }
                    if let Some(error) = operation_error.as_deref() {
                        p { class: "field-error", role: "alert", "{error}" }
                    }
                }
                if credentials.is_empty() {
                    article { class: "empty-state surface-card",
                        span { class: "empty-state__mark", aria_hidden: "true", "◇" }
                        h2 { "No credentials yet" }
                        p { "Receive the standalone fixture to prove protected storage and issuer-proof verification." }
                        span { class: "status-pill", "Profile scoped" }
                    }
                } else {
                    section { class: "credential-inventory", aria_label: "Saved credentials",
                        for credential in credentials.clone() {
                            {
                                let retained = credentials.clone();
                                let current_id = credential.id.clone();
                                rsx! {
                                    CredentialRecordCard {
                                        key: "{current_id}",
                                        profile_id: profile_id.clone(),
                                        credential,
                                        on_change: move |change| {
                                            let mut next = retained.clone();
                                            match change {
                                                CredentialChange::Updated(updated) => {
                                                    next.retain(|entry| entry.id != updated.id);
                                                    next.push(updated);
                                                    next.sort_by(|left, right| left.id.cmp(&right.id));
                                                    state.set(CredentialPageState::Ready { credentials: next, receiving: false, operation_error: None });
                                                }
                                                CredentialChange::Deleted(identifier) => {
                                                    next.retain(|entry| entry.id != identifier);
                                                    state.set(CredentialPageState::Ready { credentials: next, receiving: false, operation_error: None });
                                                }
                                                CredentialChange::Failed(message) => state.set(CredentialPageState::Ready { credentials: next, receiving: false, operation_error: Some(message) }),
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
}

#[derive(Clone)]
enum LocalDiagnosticsPageState {
    Loading,
    Ready(DiagnosticSnapshotView),
    Failed,
}

#[component]
fn DiagnosticsPage(active_profile: WalletProfileView) -> Element {
    let services = consume_context::<WalletUiServices>();
    let credential_protocol_ready = services.standalone_credential_offer().is_some();
    let mut account_state = use_signal(|| AccountPageState::Loading);
    let mut diagnostic_state = use_signal(|| LocalDiagnosticsPageState::Loading);
    let profile_id = active_profile.id.clone();
    let effect_services = services.clone();
    use_effect(move || {
        let services = effect_services.clone();
        let profile_id = profile_id.clone();
        let get_diagnostics = services.get_diagnostic_snapshot();
        spawn(async move {
            account_state.set(
                run_ui_blocking(move || load_account_page(&services, &profile_id))
                    .await
                    .unwrap_or_else(|error| AccountPageState::Failed(error.to_string())),
            );
        });
        spawn(async move {
            diagnostic_state.set(
                match run_ui_blocking(move || get_diagnostics.execute()).await {
                    Ok(Ok(snapshot)) => LocalDiagnosticsPageState::Ready(snapshot),
                    Ok(Err(_)) | Err(_) => LocalDiagnosticsPageState::Failed,
                },
            );
        });
    });

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
    let (diagnostic_summary, diagnostic_rows, diagnostics_ready) = match diagnostic_state
        .read()
        .clone()
    {
        LocalDiagnosticsPageState::Loading => ("Loading".to_owned(), Vec::new(), false),
        LocalDiagnosticsPageState::Failed => ("Status unavailable".to_owned(), Vec::new(), false),
        LocalDiagnosticsPageState::Ready(snapshot) => {
            let rows = snapshot
                .counts()
                .iter()
                .map(|count| {
                    (
                        count.code().as_str().to_owned(),
                        format!(
                            "{} · {} occurrence{}",
                            count.severity().as_str(),
                            count.occurrences(),
                            if count.occurrences() == 1 { "" } else { "s" }
                        ),
                    )
                })
                .collect();
            (
                format!(
                    "{} retained · {} total · {} evicted · capacity {}",
                    snapshot.recent().len(),
                    snapshot.total_events(),
                    snapshot.evicted_events(),
                    snapshot.capacity()
                ),
                rows,
                true,
            )
        }
    };
    let refresh_services = services.clone();
    let clear_services = services.clone();
    let mut refresh_state = diagnostic_state;
    let mut clear_state = diagnostic_state;
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
            CapabilityStatus { name: "Local proof provider", state: "Device-gated".to_owned(), ready: false }
            CapabilityStatus { name: "DID adapter", state: if credential_protocol_ready { "Standalone Midnight DID".to_owned() } else { "Not connected".to_owned() }, ready: credential_protocol_ready }
            CapabilityStatus {
                name: "Credential protocols",
                state: if credential_protocol_ready { "OpenID4VCI 1.0 · standalone".to_owned() } else { "Not connected".to_owned() },
                ready: credential_protocol_ready,
            }
        }
        section { class: "surface-card",
            p { class: "card-eyebrow", "Secret-safe runtime health" }
            h2 { "Process-local diagnostics" }
            p { "Telemetry is off. Events use fixed codes, retain no payloads, and disappear when this process exits." }
            div { class: "button-row",
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        let get = refresh_services.get_diagnostic_snapshot();
                        refresh_state.set(LocalDiagnosticsPageState::Loading);
                        spawn(async move {
                            refresh_state.set(match run_ui_blocking(move || get.execute()).await {
                                Ok(Ok(snapshot)) => LocalDiagnosticsPageState::Ready(snapshot),
                                Ok(Err(_)) | Err(_) => LocalDiagnosticsPageState::Failed,
                            });
                        });
                    },
                    "Refresh"
                }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        let clear = clear_services.clear_diagnostics();
                        let get = clear_services.get_diagnostic_snapshot();
                        clear_state.set(LocalDiagnosticsPageState::Loading);
                        spawn(async move {
                            clear_state.set(match run_ui_blocking(move || {
                                clear.execute(ClearDiagnosticsCommand {
                                    confirmed: true,
                                    intent: CLEAR_LOCAL_DIAGNOSTICS_INTENT.to_owned(),
                                })?;
                                get.execute()
                            }).await {
                                Ok(Ok(snapshot)) => LocalDiagnosticsPageState::Ready(snapshot),
                                Ok(Err(_)) | Err(_) => LocalDiagnosticsPageState::Failed,
                            });
                        });
                    },
                    "Clear local events"
                }
            }
            div { class: "diagnostic-grid",
                CapabilityStatus { name: "Bounded event ring", state: diagnostic_summary, ready: diagnostics_ready }
                CapabilityStatus { name: "Privacy boundary", state: "No persistence · no upload · no payloads".to_owned(), ready: true }
                if diagnostic_rows.is_empty() && diagnostics_ready {
                    article { class: "capability-row",
                        span { class: "capability-dot ready" }
                        div { strong { "No diagnostic events recorded" } p { "Runtime health is clean for this process." } }
                    }
                }
                for (code, detail) in diagnostic_rows {
                    article { class: "capability-row", key: "{code}",
                        span { class: "capability-dot queued" }
                        div { strong { "{code}" } p { "{detail}" } }
                    }
                }
            }
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
    lifecycle_wake: Signal<u64>,
    on_open_profile: EventHandler<MouseEvent>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut security = use_signal(|| SecurityCapabilityState::Loading);
    let mut export_secret = use_signal(|| Zeroizing::new(String::new()));
    let mut export_secret_confirmation = use_signal(|| Zeroizing::new(String::new()));
    let mut export_confirmed = use_signal(|| false);
    let mut recovery_secret = use_signal(|| Zeroizing::new(String::new()));
    let mut recovery_confirmed = use_signal(|| false);
    let mut backup_state = use_signal(|| PortableBackupUiState::Idle);
    let profile_id = active_profile.id.clone();
    let services_for_load = services.clone();
    use_effect(move || {
        let _lifecycle_generation = lifecycle_wake();
        let services = services_for_load.clone();
        let profile_id = profile_id.clone();
        spawn(async move {
            let result = run_ui_blocking(move || {
                services
                    .get_wallet_security_status()
                    .execute(WalletProfileSecurityCommand { profile_id })
            })
            .await;
            security.set(match result {
                Ok(result) => result.map_or_else(
                    |error| SecurityCapabilityState::Failed(error.to_string()),
                    SecurityCapabilityState::Ready,
                ),
                Err(error) => SecurityCapabilityState::Failed(error.to_string()),
            });
        });
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
                                let services = security_services.clone();
                                security_state.set(SecurityCapabilityState::Loading);
                                spawn(async move {
                                    let state = status.state_name();
                                    let result = run_ui_blocking(move || {
                                        match state {
                                            "Uninitialized" => Some(
                                                services
                                                    .initialize_wallet_security()
                                                    .execute(command),
                                            ),
                                            "Locked" => {
                                                Some(services.unlock_wallet().execute(command))
                                            }
                                            "Unlocked" => {
                                                Some(services.lock_wallet().execute(command))
                                            }
                                            _ => None,
                                        }
                                    })
                                    .await;
                                    security_state.set(match result {
                                        Ok(Some(result)) => result.map_or_else(
                                            |error| {
                                                SecurityCapabilityState::Failed(error.to_string())
                                            },
                                            SecurityCapabilityState::Ready,
                                        ),
                                        Ok(None) => SecurityCapabilityState::Failed(
                                            "wallet protection is unavailable".to_owned(),
                                        ),
                                        Err(error) => {
                                            SecurityCapabilityState::Failed(error.to_string())
                                        }
                                    });
                                });
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

    let backup_card = match security.read().clone() {
        SecurityCapabilityState::Ready(status) => {
            let supported = status.portable_backup_supported;
            let busy = matches!(*backup_state.read(), PortableBackupUiState::Working(_));
            let can_export = supported
                && status.state_name() != "Uninitialized"
                && !busy
                && export_confirmed()
                && !export_secret.read().is_empty()
                && !export_secret_confirmation.read().is_empty();
            let can_recover = supported
                && status.state_name() == "Uninitialized"
                && !busy
                && recovery_confirmed()
                && !recovery_secret.read().is_empty();
            let export_services = services.clone();
            let export_profile_id = active_profile.id.clone();
            let recover_services = services.clone();
            let recover_profile_id = active_profile.id.clone();
            rsx! {
                article { class: "backup-card surface-card",
                    div { class: "card-heading",
                        div {
                            p { class: "card-eyebrow", "Portable complete backup" }
                            h2 { "One encrypted wallet document" }
                        }
                        span {
                            class: if supported { "status-pill success" } else { "status-pill" },
                            if supported { "Available" } else { "Fail closed" }
                        }
                    }
                    p { class: "backup-warning",
                        strong { "Store the recovery secret separately. " }
                        "This document contains the profile, Midnight account associations, DID records, complete credentials, and protected custody state. Chain-derived caches and transaction history are intentionally rebuilt instead of copied."
                    }
                    if supported {
                        div { class: "backup-actions",
                            section { class: "backup-action",
                                h3 { "{EXPORT_COMPLETE_WALLET_BACKUP_TITLE}" }
                                p { "Choose a new recovery secret. Native custody requires fresh device authorization before the complete encrypted document can be saved." }
                                label { r#for: "wallet-backup-secret", "Recovery secret"
                                    input {
                                        id: "wallet-backup-secret",
                                        r#type: "password",
                                        minlength: 12,
                                        maxlength: MAX_WALLET_RECOVERY_SECRET_CHARACTERS,
                                        autocomplete: "new-password",
                                        spellcheck: false,
                                        disabled: busy,
                                        value: export_secret.read().as_str(),
                                        oninput: move |event| export_secret.set(Zeroizing::new(event.value())),
                                    }
                                }
                                label { r#for: "wallet-backup-secret-confirmation", "Repeat recovery secret"
                                    input {
                                        id: "wallet-backup-secret-confirmation",
                                        r#type: "password",
                                        minlength: 12,
                                        maxlength: MAX_WALLET_RECOVERY_SECRET_CHARACTERS,
                                        autocomplete: "new-password",
                                        spellcheck: false,
                                        disabled: busy,
                                        value: export_secret_confirmation.read().as_str(),
                                        oninput: move |event| export_secret_confirmation.set(Zeroizing::new(event.value())),
                                    }
                                }
                                label { class: "confirmation-row",
                                    input {
                                        r#type: "checkbox",
                                        checked: export_confirmed(),
                                        disabled: busy,
                                        onchange: move |event| export_confirmed.set(event.checked()),
                                    }
                                    "I confirm this complete wallet export and will store its recovery secret separately."
                                }
                                button {
                                    class: "primary-action",
                                    r#type: "button",
                                    disabled: !can_export,
                                    onclick: move |_| {
                                        let first = export_secret();
                                        let second = export_secret_confirmation();
                                        export_secret.set(Zeroizing::new(String::new()));
                                        export_secret_confirmation.set(Zeroizing::new(String::new()));
                                        export_confirmed.set(false);
                                        if *first != *second {
                                            backup_state.set(PortableBackupUiState::Failed(
                                                "Recovery secrets do not match.".to_owned(),
                                            ));
                                            return;
                                        }
                                        let secret = match WalletRecoverySecret::parse(&*first) {
                                            Ok(secret) => secret,
                                            Err(error) => {
                                                backup_state.set(PortableBackupUiState::Failed(
                                                    error.to_string(),
                                                ));
                                                return;
                                            }
                                        };
                                        let services = export_services.clone();
                                        let profile_id = export_profile_id.clone();
                                        backup_state.set(PortableBackupUiState::Working(
                                            "Authorizing and encrypting the complete wallet",
                                        ));
                                        spawn(async move {
                                            let worker_services = services.clone();
                                            let package = run_ui_blocking(move || {
                                                worker_services
                                                    .export_complete_wallet_backup
                                                    .execute(ExportCompleteWalletBackupCommand {
                                                        profile_id,
                                                        recovery_secret: secret,
                                                        confirmation: SensitiveOperationConfirmation {
                                                            title: EXPORT_COMPLETE_WALLET_BACKUP_TITLE.to_owned(),
                                                            summary: EXPORT_COMPLETE_WALLET_BACKUP_SUMMARY.to_owned(),
                                                            confirmed: true,
                                                        },
                                                    })
                                            })
                                            .await;
                                            let next = match package {
                                                Ok(Ok(package)) => match services
                                                    .portable_wallet_backup_documents
                                                    .export(
                                                        PortableWalletBackupDocumentKind::CompleteWallet,
                                                        &package,
                                                    )
                                                    .await
                                                {
                                                    Ok(()) => PortableBackupUiState::Succeeded(
                                                        "Encrypted complete wallet backup saved to the selected document.".to_owned(),
                                                    ),
                                                    Err(PortableWalletBackupDocumentError::Cancelled) => {
                                                        PortableBackupUiState::Cancelled
                                                    }
                                                    Err(error) => PortableBackupUiState::Failed(
                                                        error.to_string(),
                                                    ),
                                                },
                                                Ok(Err(error)) => PortableBackupUiState::Failed(
                                                    error.to_string(),
                                                ),
                                                Err(error) => PortableBackupUiState::Failed(
                                                    error.to_string(),
                                                ),
                                            };
                                            backup_state.set(next);
                                        });
                                    },
                                    "Choose file and export"
                                }
                            }
                            section { class: "backup-action",
                                h3 { "Legacy · {RECOVER_PORTABLE_WALLET_BACKUP_TITLE}" }
                                p {
                                    if status.state_name() == "Uninitialized" {
                                        "Choose an older custody-only Oxid backup. This compatibility path restores protected keys into this exact empty profile; complete-wallet recovery is available on the first-run screen."
                                    } else {
                                        "Legacy recovery is disabled because this profile is already initialized. Oxid never overwrites or merges existing custody."
                                    }
                                }
                                label { r#for: "wallet-recovery-secret", "Recovery secret"
                                    input {
                                        id: "wallet-recovery-secret",
                                        r#type: "password",
                                        minlength: 12,
                                        maxlength: MAX_WALLET_RECOVERY_SECRET_CHARACTERS,
                                        autocomplete: "current-password",
                                        spellcheck: false,
                                        disabled: busy || status.state_name() != "Uninitialized",
                                        value: recovery_secret.read().as_str(),
                                        oninput: move |event| recovery_secret.set(Zeroizing::new(event.value())),
                                    }
                                }
                                label { class: "confirmation-row",
                                    input {
                                        r#type: "checkbox",
                                        checked: recovery_confirmed(),
                                        disabled: busy || status.state_name() != "Uninitialized",
                                        onchange: move |event| recovery_confirmed.set(event.checked()),
                                    }
                                    "I confirm legacy custody-only recovery into this exact empty profile."
                                }
                                button {
                                    class: "secondary-action",
                                    r#type: "button",
                                    disabled: !can_recover,
                                    onclick: move |_| {
                                        let raw = recovery_secret();
                                        recovery_secret.set(Zeroizing::new(String::new()));
                                        recovery_confirmed.set(false);
                                        let secret = match WalletRecoverySecret::parse(&*raw) {
                                            Ok(secret) => secret,
                                            Err(error) => {
                                                backup_state.set(PortableBackupUiState::Failed(
                                                    error.to_string(),
                                                ));
                                                return;
                                            }
                                        };
                                        let services = recover_services.clone();
                                        let profile_id = recover_profile_id.clone();
                                        let mut security_state = security;
                                        backup_state.set(PortableBackupUiState::Working(
                                            "Waiting for a backup document",
                                        ));
                                        spawn(async move {
                                            let imported = services
                                                .portable_wallet_backup_documents
                                                .import()
                                                .await;
                                            let recovered = match imported {
                                                Ok(backup) => {
                                                    let services = services.clone();
                                                    match run_ui_blocking(move || {
                                                        let summary = services
                                                            .recover_portable_wallet_backup
                                                            .execute(RecoverPortableWalletBackupCommand {
                                                                profile_id: profile_id.clone(),
                                                                backup,
                                                                recovery_secret: secret,
                                                                confirmation: SensitiveOperationConfirmation {
                                                                    title: RECOVER_PORTABLE_WALLET_BACKUP_TITLE.to_owned(),
                                                                    summary: RECOVER_PORTABLE_WALLET_BACKUP_SUMMARY.to_owned(),
                                                                    confirmed: true,
                                                                },
                                                            })
                                                            .map_err(|error| error.to_string())?;
                                                        let status = services
                                                            .get_wallet_security_status
                                                            .execute(WalletProfileSecurityCommand {
                                                                profile_id,
                                                            })
                                                            .map_err(|error| error.to_string())?;
                                                        Ok::<_, String>((summary, status))
                                                    })
                                                    .await
                                                    {
                                                        Ok(result) => result,
                                                        Err(error) => Err(error.to_string()),
                                                    }
                                                }
                                                Err(PortableWalletBackupDocumentError::Cancelled) => {
                                                    backup_state.set(PortableBackupUiState::Cancelled);
                                                    return;
                                                }
                                                Err(error) => Err(error.to_string()),
                                            };
                                            match recovered {
                                                Ok((summary, status)) => {
                                                    backup_state.set(PortableBackupUiState::Succeeded(
                                                        format!(
                                                            "Recovered custody with {} protected key(s).",
                                                            summary.restored_key_count,
                                                        ),
                                                    ));
                                                    security_state.set(
                                                        SecurityCapabilityState::Ready(status),
                                                    );
                                                }
                                                Err(error) => backup_state.set(
                                                    PortableBackupUiState::Failed(error),
                                                ),
                                            }
                                        });
                                    },
                                    "Choose backup and recover"
                                }
                            }
                        }
                    }
                    match backup_state.read().clone() {
                        PortableBackupUiState::Idle => rsx! {},
                        PortableBackupUiState::Working(message) => rsx! {
                            div { class: "result", role: "status", aria_busy: "true",
                                span { class: "loading-mark", aria_hidden: "true" }
                                p { "{message}" }
                            }
                        },
                        PortableBackupUiState::Succeeded(message) => rsx! {
                            div { class: "result", role: "status", p { "{message}" } }
                        },
                        PortableBackupUiState::Cancelled => rsx! {
                            div { class: "result", role: "status", p { "Document selection cancelled. No custody state was changed." } }
                        },
                        PortableBackupUiState::Failed(message) => rsx! {
                            div { class: "result error", role: "alert", p { "{message}" } }
                        },
                    }
                }
            }
        }
        SecurityCapabilityState::Loading | SecurityCapabilityState::Failed(_) => rsx! {},
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
        {backup_card}
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
        let service = services.list_wallet_profiles();
        spawn(async move {
            let result = run_ui_blocking(move || service.execute()).await;
            profiles.set(match result {
                Ok(Ok(profiles)) => ProfileListState::Ready(profiles),
                Ok(Err(error)) => ProfileListState::Failed(error.to_string()),
                Err(error) => ProfileListState::Failed(error.to_string()),
            });
        });
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
const LUCIDE_LANDMARK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 10 9-7 9 7"/><path d="M5 10v9"/><path d="M9 10v9"/><path d="M15 10v9"/><path d="M19 10v9"/><path d="M3 19h18"/><path d="M2 22h20"/></svg>"#;
const LUCIDE_FINGERPRINT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 10a2 2 0 0 0-2 2c0 1.02-.1 2.51-.26 4"/><path d="M14 13.12c0 2.38 0 6.38-1 8.88"/><path d="M17.29 21.02c.12-.6.43-2.3.5-3.02"/><path d="M2 12a10 10 0 0 1 18-6"/><path d="M2 16h.01"/><path d="M21.8 16c.2-2 .131-5.354 0-6"/><path d="M5 19.5C5.5 18 6 15 6 12a6 6 0 0 1 .34-2"/><path d="M8.65 22c.21-.66.45-1.32.57-2"/><path d="M9 6.8a6 6 0 0 1 9 5.2c0 .47 0 1.17-.02 2"/></svg>"#;
const LUCIDE_BADGE_CHECK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3.85 8.62a4 4 0 0 1 4.78-4.77 4 4 0 0 1 6.74 0 4 4 0 0 1 4.78 4.78 4 4 0 0 1 0 6.74 4 4 0 0 1-4.77 4.78 4 4 0 0 1-6.75 0 4 4 0 0 1 0-6.76Z"/><path d="m9 12 2 2 4-4"/></svg>"#;
const LUCIDE_ACTIVITY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.5.5 0 0 1-.96 0L9.24 2.18a.5.5 0 0 0-.96 0l-2.35 8.36A2 2 0 0 1 4 12H2"/></svg>"#;
const LUCIDE_SETTINGS_2: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 7h-9"/><path d="M14 17H5"/><circle cx="17" cy="17" r="3"/><circle cx="7" cy="7" r="3"/></svg>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn blocking_wallet_operations_execute_off_the_caller_thread() {
        let caller = std::thread::current().id();

        let (worker, worker_name) = futures::executor::block_on(run_ui_blocking(|| {
            (
                std::thread::current().id(),
                std::thread::current().name().map(str::to_owned),
            )
        }))
        .expect("worker operation");

        assert_ne!(worker, caller);
        assert_eq!(worker_name.as_deref(), Some("oxid-ui-blocking"));
        assert_eq!(UI_BLOCKING_TASK_STACK_BYTES, 8 * 1024 * 1024);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn asynchronous_wallet_operations_are_polled_off_the_caller_thread() {
        let caller = std::thread::current().id();

        let worker =
            futures::executor::block_on(run_ui_future(async move { std::thread::current().id() }))
                .expect("worker future");

        assert_ne!(worker, caller);
    }

    #[test]
    fn blocking_worker_failures_have_one_payload_free_ui_message() {
        assert_eq!(
            UiBlockingTaskError::WorkerUnavailable.to_string(),
            "background wallet operation failed"
        );
        assert_eq!(
            UiBlockingTaskError::WorkerFailed.to_string(),
            "background wallet operation failed"
        );
    }

    #[test]
    fn primary_navigation_matches_the_reviewed_wallet_shell() {
        let labels = PRIMARY_DESTINATIONS.map(Destination::label);

        assert_eq!(
            labels,
            [
                "Assets",
                "Vault",
                "DIDs",
                "Credentials",
                "Diagnostics",
                "Settings"
            ]
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
    fn locked_account_placeholder_keeps_reactivation_reachable() {
        let networks = WalletNetworkListView {
            selected_network_id: "undeployed".to_owned(),
            networks: vec![oxid_wallet_application::WalletNetworkView {
                chain: "midnight".to_owned(),
                network_id: "undeployed".to_owned(),
                display_name: "Midnight undeployed".to_owned(),
                environment: "development".to_owned(),
                selected: true,
            }],
        };

        let account = protected_account_placeholder(&networks).expect("selected network");

        assert_eq!(account.network_id, "undeployed");
        assert_eq!(account.source, "unavailable");
        assert!(account.account_id.is_none());
        assert!(account.addresses.is_empty());
        assert_eq!(account.sync.state, "unavailable");
        assert!(!has_protected_account(&account));
    }

    #[test]
    fn initial_account_read_never_enters_locked_custody() {
        assert!(!account_read_is_noninteractive("Uninitialized"));
        assert!(!account_read_is_noninteractive("Locked"));
        assert!(account_read_is_noninteractive("Unlocked"));
    }

    #[test]
    fn complete_recovery_feedback_reports_only_bounded_counts() {
        let summary = CompleteWalletRecoverySummary {
            profile_id: "profile_test".to_owned(),
            restored_key_count: 3,
            restored_did_count: 2,
            restored_credential_count: 1,
        };

        assert_eq!(
            complete_recovery_message(&summary),
            "Recovered 3 protected key(s), 2 DID record(s), and 1 credential(s)."
        );
        assert!(!complete_recovery_message(&summary).contains("profile_test"));
    }

    #[test]
    fn profile_monogram_uses_the_first_visible_character() {
        assert_eq!(profile_monogram("  primary"), "P");
        assert_eq!(profile_monogram("---"), "O");
    }

    #[test]
    fn compact_policy_summary_keeps_revocation_truthful() {
        let credential = CredentialView {
            id: "credential_test".to_owned(),
            display_name: "Digital Passport".to_owned(),
            issuer_did: "did:midnight:undeployed:issuer".to_owned(),
            subject_did: None,
            format: "midnight_compact_vc".to_owned(),
            issued_at_ms: Some(42),
            verification_outcome: "valid".to_owned(),
            verification_stages: [
                ("issuer", "passed"),
                ("temporal", "passed"),
                ("trust", "passed"),
                ("status", "not_checked"),
            ]
            .into_iter()
            .map(
                |(name, status)| oxid_credential_application::VerificationStageView {
                    name: name.to_owned(),
                    status: status.to_owned(),
                    reason_code: None,
                },
            )
            .collect(),
        };

        assert_eq!(
            compact_credential_policy_summary(&credential).as_deref(),
            Some(
                "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
            )
        );

        let mut cbor = credential;
        cbor.format = "midnight_cbor_v1".to_owned();
        assert_eq!(compact_credential_policy_summary(&cbor), None);
    }

    #[test]
    fn atomic_units_are_rendered_without_floating_point_loss() {
        assert_eq!(format_atomic_units("5000000", 6), "5");
        assert_eq!(format_atomic_units("12000000000000000", 15), "12");
        assert_eq!(format_atomic_units("1", 6), "0.000001");
        assert_eq!(format_atomic_units("000000", 6), "0");
        assert_eq!(format_atomic_units("not-a-number", 6), "—");
    }

    fn dust_status(state: &str, current: Option<u64>, target: Option<u64>) -> WalletDustSyncView {
        WalletDustSyncView {
            network_id: "undeployed".to_owned(),
            state: state.to_owned(),
            current_cursor: current,
            target_cursor: target,
            events_processed: 2,
            balance_atomic_units: Some("12000000000000000".to_owned()),
            updated_at_millis: Some(42),
            failure: None,
        }
    }

    #[test]
    fn dust_progress_uses_event_indices_without_floating_point() {
        assert_eq!(
            dust_progress_percent(&dust_status("syncing", Some(0), Some(2))),
            Some(33)
        );
        assert_eq!(
            dust_progress_percent(&dust_status("synced", Some(2), Some(2))),
            Some(100)
        );
        assert_eq!(
            dust_progress_percent(&dust_status("never_synced", None, None)),
            None
        );
    }

    #[test]
    fn dust_failure_copy_distinguishes_cached_state_from_live_readiness() {
        let mut cached = dust_status("cached", Some(2), Some(2));
        cached.failure = Some("transport_unavailable".to_owned());

        let note = dust_sync_note(&cached);
        assert!(note.contains("cached DUST checkpoint"));
        assert!(note.contains("spending remains disabled"));
        assert!(note.contains("transport unavailable"));
        assert_eq!(dust_sync_state_label("stalled"), "Stalled");
    }

    fn shielded_status(
        state: &str,
        current: Option<u64>,
        target: Option<u64>,
    ) -> WalletShieldedSyncView {
        WalletShieldedSyncView {
            network_id: "undeployed".to_owned(),
            state: state.to_owned(),
            current_cursor: current,
            target_cursor: target,
            events_processed: 2,
            owned_note_count: Some(1),
            commitment_count: Some(3),
            balances: vec![],
            updated_at_millis: Some(42),
            failure: None,
        }
    }

    #[test]
    fn shielded_progress_and_cached_copy_preserve_live_readiness() {
        assert_eq!(
            shielded_progress_percent(&shielded_status("syncing", Some(0), Some(2))),
            Some(33)
        );
        assert_eq!(
            shielded_progress_percent(&shielded_status("synced", Some(2), Some(2))),
            Some(100)
        );
        let mut cached = shielded_status("cached", Some(2), Some(2));
        cached.failure = Some("transport_unavailable".to_owned());
        let note = shielded_sync_note(&cached);
        assert!(note.contains("cached shielded checkpoint"));
        assert!(note.contains("live catch-up"));
        assert!(note.contains("transport unavailable"));
        assert!(
            shielded_sync_note(&shielded_status("cancelled", Some(1), Some(2)))
                .contains("consistent checkpoint")
        );
        assert!(
            shielded_sync_note(&shielded_status("stalled", Some(1), Some(2)))
                .contains("last consistent checkpoint")
        );
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
    fn durable_submission_states_have_truthful_mobile_copy() {
        assert_eq!(submission_status_heading("included"), "Transfer included");
        assert_eq!(
            submission_status_label("outcome_unknown"),
            "Outcome unknown"
        );
        assert!(submission_status_note("broadcasting").contains("before broadcast"));
        assert!(submission_status_note("outcome_unknown").contains("not submit a duplicate"));
        assert!(submission_status_note("expired").contains("expired"));
    }

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
            }) if maximum_claim_amount == "40" && initial_amount == "100"
        ));
        assert!(matches!(
            vault_contract_inputs("deposit_to_lock").action(),
            Ok(PreparePassportVaultCallAction::DepositToLock {
                lock_id: 7,
                amount,
            }) if amount == "10"
        ));
        assert!(matches!(
            vault_contract_inputs("claim_from_lock").action(),
            Ok(PreparePassportVaultCallAction::ClaimFromLock {
                lock_id: 7,
                amount,
                credential_id,
            }) if amount == "10" && credential_id == "credential_test"
        ));
        assert!(matches!(
            vault_contract_inputs("withdraw_from_lock").action(),
            Ok(PreparePassportVaultCallAction::WithdrawFromLock {
                lock_id: 7,
                amount,
            }) if amount == "10"
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
            passport_vault_call_mode_label("deterministic_simulation"),
            "Deterministic simulation"
        );
        assert!(
            passport_vault_call_mode_note("deterministic_simulation").contains("no node broadcast")
        );
        assert_eq!(
            passport_vault_call_mode_label("native_settlement"),
            "Midnight live"
        );
        assert_eq!(
            passport_vault_contract_source_label("deterministic_simulation"),
            "simulated"
        );
        assert_eq!(
            passport_vault_contract_source_label("authenticated_node"),
            "authenticated_node"
        );
        assert_eq!(
            passport_vault_submission_mode_label("deterministic_simulation_only"),
            "simulated · deterministic simulation only"
        );
        assert_eq!(passport_vault_submission_mode_label("midnight"), "midnight");
        assert!(
            passport_vault_call_mode_note("native_settlement")
                .contains("authenticated finalized state")
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
            passport_vault_submission_note("outcome_unknown").contains("not submit a duplicate")
        );
    }

    #[test]
    fn long_public_identifiers_are_shortened_for_mobile_display() {
        assert_eq!(truncate_middle("1234567890", 4, 3), "1234…890");
        assert_eq!(truncate_middle("short", 4, 3), "short");
    }
}
