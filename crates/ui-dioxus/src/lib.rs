// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{sync::Arc, time::Duration};

use dioxus::prelude::*;
use oxid_credential_application::{
    CredentialDisclosureQuery, CredentialDisclosureView, CredentialOperationError,
    CredentialPredicateInput, CredentialProfileQuery, CredentialQuery, CredentialView,
    DeleteCredentialCommand, DeleteCredentialUseCase, GetCredentialDisclosureUseCase,
    GetCredentialUseCase, ListCredentialsUseCase, PreviewCredentialDisclosureCommand,
    PreviewCredentialDisclosureUseCase, ReceiveCredentialUseCase, RevealCredentialClaimCommand,
    RevealCredentialClaimUseCase, ReverifyCredentialUseCase,
};
use oxid_identity_application::{
    CreateDidCommand, CreateDidUseCase, DeactivateDidCommand, DeactivateDidUseCase,
    DidKeyAlgorithm, DidOperationConfirmation, DidOperationError, DidRecordQuery, DidRecordView,
    DidUpdate, ForgetDidUseCase, ListDidRecordsQuery, ListDidRecordsUseCase, ResolveDidCommand,
    ResolveDidUseCase, SignDidPayloadCommand, SignDidPayloadUseCase, UpdateDidCommand,
    UpdateDidUseCase,
};
use oxid_identity_domain::VerificationRelationship;
use oxid_protocol_application::{
    AcceptCredentialIssuanceCommand, AcceptCredentialIssuanceUseCase,
    AcceptSelfIssuedAuthenticationCommand, AcceptSelfIssuedAuthenticationUseCase,
    CredentialIssuanceError, CredentialIssuanceView, PrepareCredentialIssuanceCommand,
    PrepareCredentialIssuanceUseCase, PrepareSelfIssuedAuthenticationCommand,
    PrepareSelfIssuedAuthenticationUseCase, RefuseCredentialIssuanceCommand,
    RefuseCredentialIssuanceUseCase, RefuseSelfIssuedAuthenticationCommand,
    RefuseSelfIssuedAuthenticationUseCase, SelfIssuedAuthenticationError,
    SelfIssuedAuthenticationView,
};
use oxid_wallet_application::{
    AuthorizeWalletTransferCommand, AuthorizeWalletTransferUseCase, CancelWalletDustSyncUseCase,
    CancelWalletShieldedSyncUseCase, CancelWalletTransferSubmissionUseCase,
    CreateWalletProfileCommand, CreateWalletProfileUseCase, DeriveWalletAccountCommand,
    DeriveWalletAccountUseCase, GetActiveWalletProfileUseCase, GetWalletAccountUseCase,
    GetWalletDustSyncStatusUseCase, GetWalletSecurityStatusUseCase,
    GetWalletShieldedSyncStatusUseCase, GetWalletTransferDraftUseCase,
    GetWalletTransferSubmissionStatusUseCase, InitializeWalletSecurityUseCase,
    ListWalletNetworksUseCase, ListWalletProfilesUseCase, ListWalletTransferSubmissionsUseCase,
    LockWalletUseCase, PrepareWalletTransferCommand, PrepareWalletTransferUseCase,
    ReconcileWalletTransferSubmissionUseCase, SelectWalletNetworkCommand,
    SelectWalletNetworkUseCase, SelectWalletProfileCommand, SelectWalletProfileUseCase,
    SensitiveOperationConfirmation, StartWalletDustSyncUseCase, StartWalletShieldedSyncUseCase,
    SubmitWalletTransferCommand, SubmitWalletTransferUseCase, SyncWalletAccountUseCase,
    UnlockWalletUseCase, WalletAccountQuery, WalletAccountView, WalletDustSyncCommand,
    WalletDustSyncView, WalletNetworkListView, WalletProfileSecurityCommand, WalletProfileView,
    WalletSecurityStatusView, WalletShieldedSyncCommand, WalletShieldedSyncView,
    WalletTransferDraftQuery, WalletTransferPreviewView, WalletTransferSubmissionQuery,
    WalletTransferSubmissionStatusView, WalletTransferSubmissionView,
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
    get_wallet_dust_sync_status: Arc<dyn GetWalletDustSyncStatusUseCase>,
    start_wallet_dust_sync: Arc<dyn StartWalletDustSyncUseCase>,
    cancel_wallet_dust_sync: Arc<dyn CancelWalletDustSyncUseCase>,
    get_wallet_shielded_sync_status: Arc<dyn GetWalletShieldedSyncStatusUseCase>,
    start_wallet_shielded_sync: Arc<dyn StartWalletShieldedSyncUseCase>,
    cancel_wallet_shielded_sync: Arc<dyn CancelWalletShieldedSyncUseCase>,
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
    prepare_self_issued_authentication: Arc<dyn PrepareSelfIssuedAuthenticationUseCase>,
    accept_self_issued_authentication: Arc<dyn AcceptSelfIssuedAuthenticationUseCase>,
    refuse_self_issued_authentication: Arc<dyn RefuseSelfIssuedAuthenticationUseCase>,
    standalone_self_issued_request: Option<String>,
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
}

/// Consent-driven credential issuance capabilities consumed by the Credentials page.
pub struct CredentialIssuanceUiServices {
    prepare_credential_issuance: Arc<dyn PrepareCredentialIssuanceUseCase>,
    accept_credential_issuance: Arc<dyn AcceptCredentialIssuanceUseCase>,
    refuse_credential_issuance: Arc<dyn RefuseCredentialIssuanceUseCase>,
    standalone_credential_offer: Option<String>,
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
        receive_credential: Arc<dyn ReceiveCredentialUseCase>,
        list_credentials: Arc<dyn ListCredentialsUseCase>,
        get_credential: Arc<dyn GetCredentialUseCase>,
        reverify_credential: Arc<dyn ReverifyCredentialUseCase>,
        delete_credential: Arc<dyn DeleteCredentialUseCase>,
        issuance: CredentialIssuanceUiServices,
        disclosure: CredentialDisclosureUiServices,
    ) -> Self {
        Self {
            receive_credential,
            list_credentials,
            get_credential,
            reverify_credential,
            delete_credential,
            get_credential_disclosure: disclosure.get,
            preview_credential_disclosure: disclosure.preview,
            reveal_credential_claim: disclosure.reveal_local,
            prepare_credential_issuance: issuance.prepare_credential_issuance,
            accept_credential_issuance: issuance.accept_credential_issuance,
            refuse_credential_issuance: issuance.refuse_credential_issuance,
            standalone_credential_offer: issuance.standalone_credential_offer,
        }
    }
}

/// Identity-facing UI capabilities kept separate from wallet account services.
pub struct IdentityUiServices {
    dids: DidUiServices,
    credentials: CredentialUiServices,
    authentication: SelfIssuedAuthenticationUiServices,
}

impl IdentityUiServices {
    #[must_use]
    pub const fn new(
        dids: DidUiServices,
        credentials: CredentialUiServices,
        authentication: SelfIssuedAuthenticationUiServices,
    ) -> Self {
        Self {
            dids,
            credentials,
            authentication,
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
    authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
    submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
    get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
    get_wallet_transfer_submission_status: Arc<dyn GetWalletTransferSubmissionStatusUseCase>,
    cancel_wallet_transfer_submission: Arc<dyn CancelWalletTransferSubmissionUseCase>,
    list_wallet_transfer_submissions: Arc<dyn ListWalletTransferSubmissionsUseCase>,
    reconcile_wallet_transfer_submission: Arc<dyn ReconcileWalletTransferSubmissionUseCase>,
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
        prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
        authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
        submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
        get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
        get_wallet_transfer_submission_status: Arc<dyn GetWalletTransferSubmissionStatusUseCase>,
        cancel_wallet_transfer_submission: Arc<dyn CancelWalletTransferSubmissionUseCase>,
        recovery: WalletTransactionRecoveryUiServices,
    ) -> Self {
        Self {
            prepare_wallet_transfer,
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
        dust: WalletDustSyncUiServices,
        shielded: WalletShieldedSyncUiServices,
        transactions: WalletTransactionUiServices,
        identity: IdentityUiServices,
    ) -> Self {
        let dids = identity.dids;
        let credentials = identity.credentials;
        let authentication = identity.authentication;
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
            get_wallet_dust_sync_status: dust.get_wallet_dust_sync_status,
            start_wallet_dust_sync: dust.start_wallet_dust_sync,
            cancel_wallet_dust_sync: dust.cancel_wallet_dust_sync,
            get_wallet_shielded_sync_status: shielded.get_wallet_shielded_sync_status,
            start_wallet_shielded_sync: shielded.start_wallet_shielded_sync,
            cancel_wallet_shielded_sync: shielded.cancel_wallet_shielded_sync,
            prepare_wallet_transfer: transactions.prepare_wallet_transfer,
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
            prepare_self_issued_authentication: authentication.prepare,
            accept_self_issued_authentication: authentication.accept,
            refuse_self_issued_authentication: authentication.refuse,
            standalone_self_issued_request: authentication.standalone_request,
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
enum DustSyncPaneState {
    Loading,
    Ready {
        status: WalletDustSyncView,
        operation_error: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ShieldedSyncPaneState {
    Loading,
    Ready {
        status: WalletShieldedSyncView,
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
    Prepared(Box<WalletTransferPreviewView>),
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
                    Destination::Dids => rsx! { DidsPage { active_profile: active_profile.clone() } },
                    Destination::Credentials => rsx! { CredentialsPage { active_profile: active_profile.clone() } },
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

                SubmissionRecoveryPane { profile_id: active_profile.id.clone() }

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

#[component]
fn SubmissionRecoveryPane(profile_id: String) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| SubmissionRecoveryPaneState::Loading);
    let load_services = services.clone();
    let load_profile = profile_id.clone();
    use_effect(move || {
        state.set(
            load_services
                .list_wallet_transfer_submissions()
                .execute(load_profile.clone())
                .map_or_else(
                    |error| SubmissionRecoveryPaneState::Failed(error.to_string()),
                    |submissions| SubmissionRecoveryPaneState::Ready {
                        latest: submissions.into_iter().next().map(Box::new),
                        reconciling: false,
                        operation_error: None,
                    },
                ),
        );
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
                        state.set(
                            services
                                .list_wallet_transfer_submissions()
                                .execute(profile_id.clone())
                                .map_or_else(
                                    |error| SubmissionRecoveryPaneState::Failed(error.to_string()),
                                    |submissions| SubmissionRecoveryPaneState::Ready {
                                        latest: submissions.into_iter().next().map(Box::new),
                                        reconciling: false,
                                        operation_error: None,
                                    },
                                ),
                        );
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
                                    match service.execute(WalletTransferSubmissionQuery {
                                        profile_id,
                                        draft_id,
                                    }).await {
                                        Ok(updated) => state.set(SubmissionRecoveryPaneState::Ready {
                                            latest: Some(Box::new(updated)),
                                            reconciling: false,
                                            operation_error: None,
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
        state.set(
            load_services
                .get_wallet_dust_sync_status()
                .execute(WalletDustSyncCommand {
                    profile_id: load_profile.clone(),
                })
                .map_or_else(
                    |error| DustSyncPaneState::Failed(error.to_string()),
                    |status| DustSyncPaneState::Ready {
                        status,
                        operation_error: None,
                    },
                ),
        );
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
                            state.set(
                                retry_services
                                    .get_wallet_dust_sync_status()
                                    .execute(WalletDustSyncCommand {
                                        profile_id: retry_profile.clone(),
                                    })
                                    .map_or_else(
                                        |error| DustSyncPaneState::Failed(error.to_string()),
                                        |status| DustSyncPaneState::Ready {
                                            status,
                                            operation_error: None,
                                        },
                                    ),
                            );
                        },
                        "Retry"
                    }
                }
            }
        }
        DustSyncPaneState::Ready {
            status,
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
                        disabled: unavailable || (!can_sync && !syncing),
                        onclick: move |_| {
                            let command = WalletDustSyncCommand {
                                profile_id: action_profile.clone(),
                            };
                            let result = if syncing {
                                action_services.cancel_wallet_dust_sync().execute(command)
                            } else {
                                action_services.start_wallet_dust_sync().execute(command)
                            };
                            match result {
                                Ok(updated) => {
                                    let should_poll = updated.state == "syncing";
                                    action_state.set(DustSyncPaneState::Ready {
                                        status: updated,
                                        operation_error: None,
                                    });
                                    if should_poll {
                                        poll_dust_sync(
                                            action_services.clone(),
                                            action_profile.clone(),
                                            action_state,
                                        );
                                    }
                                }
                                Err(error) => action_state.set(DustSyncPaneState::Ready {
                                    status: status.clone(),
                                    operation_error: Some(error.to_string()),
                                }),
                            }
                        },
                        if syncing {
                            "Cancel DUST sync"
                        } else if !can_sync {
                            "Unlock wallet to sync DUST"
                        } else if status.state == "never_synced" {
                            "Sync DUST"
                        } else {
                            "Resync DUST"
                        }
                    }
                }
            }
        }
    }
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
        state.set(load_shielded_sync(&load_services, &load_profile));
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
                            state.set(load_shielded_sync(&retry_services, &retry_profile));
                        },
                        "Retry"
                    }
                }
            }
        }
        ShieldedSyncPaneState::Ready {
            status,
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
                        disabled: unavailable || (!can_sync && !syncing),
                        onclick: move |_| {
                            let command = WalletShieldedSyncCommand {
                                profile_id: action_profile.clone(),
                            };
                            let result = if syncing {
                                action_services.cancel_wallet_shielded_sync().execute(command)
                            } else {
                                action_services.start_wallet_shielded_sync().execute(command)
                            };
                            match result {
                                Ok(updated) => {
                                    let should_poll = updated.state == "syncing";
                                    action_state.set(ShieldedSyncPaneState::Ready {
                                        status: updated,
                                        operation_error: None,
                                    });
                                    if should_poll {
                                        poll_shielded_sync(
                                            action_services.clone(),
                                            action_profile.clone(),
                                            action_state,
                                        );
                                    }
                                }
                                Err(error) => action_state.set(ShieldedSyncPaneState::Ready {
                                    status: status.clone(),
                                    operation_error: Some(error.to_string()),
                                }),
                            }
                        },
                        if syncing {
                            "Cancel shielded sync"
                        } else if !can_sync {
                            "Unlock wallet to sync shielded assets"
                        } else if status.state == "never_synced" {
                            "Sync shielded assets"
                        } else {
                            "Resync shielded assets"
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
                    let key_algorithm = if algorithm.read().as_str() == "p256" {
                        DidKeyAlgorithm::P256
                    } else {
                        DidKeyAlgorithm::Ed25519
                    };
                    let relationship = VerificationRelationship::parse(relationship.read().as_str())
                        .unwrap_or(VerificationRelationship::AssertionMethod);
                    let result = match operation_name.as_str() {
                        "sign" => services.sign_did_payload().execute(SignDidPayloadCommand {
                            profile_id: profile_id.clone(),
                            did: did.clone(),
                            method_id: method_or_service,
                            payload: input_value.into_bytes(),
                            confirmation: did_confirmation(
                                "Sign identity challenge",
                                "Authorize the visible payload with this DID verification method",
                                confirmed(),
                            ),
                        }).map(|signature| {
                            outcome.set(Some(format!(
                                "Signed {} bytes with {} using {}.",
                                signature.signature_bytes.len(), signature.method_id, signature.algorithm
                            )));
                            None
                        }),
                        "deactivate" => services.deactivate_did().execute(DeactivateDidCommand {
                            profile_id: profile_id.clone(),
                            did: did.clone(),
                            confirmation: did_confirmation(
                                "Deactivate DID",
                                "Permanently disable further operations for this DID",
                                confirmed(),
                            ),
                        }).map(Some),
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
                            services.update_did().execute(UpdateDidCommand {
                                profile_id: profile_id.clone(),
                                did: did.clone(),
                                operation: update,
                                confirmation: did_confirmation(
                                    "Update DID document",
                                    "Authorize the selected visible change to this managed DID",
                                    confirmed(),
                                ),
                            }).map(Some)
                        }
                    };
                    working.set(false);
                    match result {
                        Ok(Some(updated)) => {
                            outcome.set(Some("DID document updated.".to_owned()));
                            on_record.call(Ok(updated));
                        }
                        Ok(None) => {}
                        Err(error) => {
                            let message = did_operation_message(error);
                            outcome.set(Some(message.clone()));
                            on_record.call(Err(message));
                        }
                    }
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

#[component]
fn DidsPage(active_profile: WalletProfileView) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| DidPageState::Loading);
    let mut did_input = use_signal(|| STANDALONE_DID_FIXTURE.to_owned());
    let mut authentication_input = use_signal(String::new);
    let mut prepared_authentication = use_signal(|| None::<SelfIssuedAuthenticationView>);
    let mut authentication_consent = use_signal(|| false);
    let mut authentication_busy = use_signal(|| false);
    let mut authentication_notice = use_signal(|| None::<String>);
    let profile_id = active_profile.id.clone();
    let load_services = services.clone();
    let load_profile = profile_id.clone();
    use_effect(move || state.set(load_did_page(&load_services, &load_profile)));

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
                    onclick: move |_| state.set(load_did_page(&services, &profile_id)),
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
                            match create_services.create_did().execute(CreateDidCommand {
                                profile_id: create_profile.clone(),
                                network: "undeployed".to_owned(),
                            }) {
                                Ok(record) => {
                                    let mut updated = create_records.clone();
                                    updated.retain(|existing| existing.document.id != record.document.id);
                                    updated.push(record);
                                    updated.sort_by(|left, right| left.document.id.cmp(&right.document.id));
                                    state.set(DidPageState::Ready { records: updated, resolving: false, operation_error: None });
                                }
                                Err(error) => state.set(DidPageState::Ready {
                                    records: create_records.clone(), resolving: false,
                                    operation_error: Some(did_operation_message(error)),
                                }),
                            }
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
                                    match service.execute(PrepareSelfIssuedAuthenticationCommand { profile_id, request }).await {
                                        Ok(preview) => {
                                            prepared_authentication.set(Some(preview));
                                            authentication_consent.set(false);
                                            authentication_notice.set(Some("Login preview ready. Review the verifier and purpose before consenting.".to_owned()));
                                        }
                                        Err(error) => {
                                            prepared_authentication.set(None);
                                            authentication_notice.set(Some(self_issued_authentication_message(error)));
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
                                                    match service.execute(AcceptSelfIssuedAuthenticationCommand {
                                                        profile_id,
                                                        authentication_id,
                                                        holder_did,
                                                        method_id,
                                                        confirmed: true,
                                                        intent: "ACCEPT_SELF_ISSUED_AUTHENTICATION".to_owned(),
                                                    }).await {
                                                        Ok(result) => {
                                                            prepared_authentication.set(Some(result));
                                                            authentication_notice.set(Some("DID authentication succeeded and the standalone verifier independently validated the proof.".to_owned()));
                                                        }
                                                        Err(error) => authentication_notice.set(Some(self_issued_authentication_message(error))),
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
                                            move |_| match service.execute(RefuseSelfIssuedAuthenticationCommand {
                                                profile_id: profile_id.clone(),
                                                authentication_id: authentication_id.clone(),
                                            }) {
                                                Ok(result) => {
                                                    prepared_authentication.set(Some(result));
                                                    authentication_consent.set(false);
                                                    authentication_notice.set(Some("Login request refused; ephemeral protocol secrets were discarded.".to_owned()));
                                                }
                                                Err(error) => authentication_notice.set(Some(self_issued_authentication_message(error))),
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
                                match service.execute(ResolveDidCommand { profile_id, did }).await {
                                    Ok(record) => {
                                        records.retain(|existing| existing.document.id != record.document.id);
                                        records.push(record);
                                        records.sort_by(|left, right| left.document.id.cmp(&right.document.id));
                                        state.set(DidPageState::Ready { records, resolving: false, operation_error: None });
                                    }
                                    Err(error) => state.set(DidPageState::Ready { records, resolving: false, operation_error: Some(did_operation_message(error)) }),
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
                                                match forget_services.forget_did().execute(DidRecordQuery { profile_id: forget_profile.clone(), did: forget_did.clone() }) {
                                                    Ok(()) => state.set(DidPageState::Ready { records: retained.iter().filter(|record| record.document.id != forget_did).cloned().collect(), resolving: false, operation_error: None }),
                                                    Err(error) => state.set(DidPageState::Ready { records: retained.clone(), resolving: false, operation_error: Some(did_operation_message(error)) }),
                                                }
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
    let load_service = services.get_credential_disclosure();
    let load_profile = profile_id.clone();
    let load_credential = credential_id.clone();
    use_effect(move || {
        disclosure_state.set(Some(
            load_service
                .execute(CredentialDisclosureQuery {
                    profile_id: load_profile.clone(),
                    credential_id: load_credential.clone(),
                })
                .map_err(credential_operation_message),
        ));
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
                                aria_label: if revealed_first.read().is_some() { "Hide First name" } else { "Reveal First name locally" },
                                onclick: move |_| {
                                    if revealed_first.read().is_some() {
                                        revealed_first.set(None);
                                    } else {
                                        match first_service.execute(RevealCredentialClaimCommand {
                                            profile_id: first_profile.clone(),
                                            credential_id: first_credential.clone(),
                                            claim_path: PASSPORT_FIRST_NAME.to_owned(),
                                        }) {
                                            Ok(claim) => {
                                                revealed_first.set(Some(claim.value().to_owned()));
                                                plan_notice.set(Some("First name revealed only on this device screen.".to_owned()));
                                            }
                                            Err(error) => plan_notice.set(Some(credential_operation_message(error))),
                                        }
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
                                aria_label: if revealed_last.read().is_some() { "Hide Last name" } else { "Reveal Last name locally" },
                                onclick: move |_| {
                                    if revealed_last.read().is_some() {
                                        revealed_last.set(None);
                                    } else {
                                        match last_service.execute(RevealCredentialClaimCommand {
                                            profile_id: last_profile.clone(),
                                            credential_id: last_credential.clone(),
                                            claim_path: PASSPORT_LAST_NAME.to_owned(),
                                        }) {
                                            Ok(claim) => {
                                                revealed_last.set(Some(claim.value().to_owned()));
                                                plan_notice.set(Some("Last name revealed only on this device screen.".to_owned()));
                                            }
                                            Err(error) => plan_notice.set(Some(credential_operation_message(error))),
                                        }
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
                        onclick: move |_| {
                            let mut reveal_claim_paths = Vec::new();
                            if revealed_first.read().is_some() {
                                reveal_claim_paths.push(PASSPORT_FIRST_NAME.to_owned());
                            }
                            if revealed_last.read().is_some() {
                                reveal_claim_paths.push(PASSPORT_LAST_NAME.to_owned());
                            }
                            let result = preview_service.execute(PreviewCredentialDisclosureCommand {
                                profile_id: preview_profile.clone(),
                                credential_id: preview_credential.clone(),
                                reveal_claim_paths,
                                predicates: vec![CredentialPredicateInput {
                                    claim_path: PASSPORT_DATE_OF_BIRTH.to_owned(),
                                    kind: "age_over".to_owned(),
                                    threshold: age_threshold(),
                                }],
                            });
                            plan_notice.set(Some(match result {
                                Ok(plan) => format!(
                                    "{} · local preview only · no presentation generated",
                                    plan.outcome.replace('_', " ")
                                ),
                                Err(error) => credential_operation_message(error),
                            }));
                        },
                        "Preview disclosure plan"
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
                            let result = service.execute(CredentialQuery { profile_id, credential_id }).await;
                            working.set(false);
                            on_change.call(match result {
                                Ok(credential) => CredentialChange::Updated(credential),
                                Err(error) => CredentialChange::Failed(credential_operation_message(error)),
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
                        let result = delete_services.delete_credential().execute(DeleteCredentialCommand {
                            profile_id: delete_profile.clone(),
                            credential_id: delete_id.clone(),
                            confirmed: delete_confirmed(),
                            intent: "DELETE_CREDENTIAL".to_owned(),
                        });
                        on_change.call(match result {
                            Ok(()) => CredentialChange::Deleted(delete_id.clone()),
                            Err(error) => CredentialChange::Failed(credential_operation_message(error)),
                        });
                    },
                    "Delete credential"
                }
            }
        }
    }
}

#[component]
fn CredentialsPage(active_profile: WalletProfileView) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| CredentialPageState::Loading);
    let mut offer_input = use_signal(String::new);
    let mut prepared_issuance = use_signal(|| None::<CredentialIssuanceView>);
    let mut issuance_consent = use_signal(|| false);
    let mut issuance_busy = use_signal(|| false);
    let mut issuance_notice = use_signal(|| None::<String>);
    let profile_id = active_profile.id.clone();
    let load_services = services.clone();
    let load_profile = profile_id.clone();
    use_effect(move || state.set(load_credential_page(&load_services, &load_profile)));

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
                    onclick: move |_| state.set(load_credential_page(&services, &profile_id)),
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
                                    match service.execute(PrepareCredentialIssuanceCommand { profile_id, offer }).await {
                                        Ok(preview) => {
                                            prepared_issuance.set(Some(preview));
                                            issuance_consent.set(false);
                                            issuance_notice.set(Some("Offer preview ready. Review the issuer and requested credential before consenting.".to_owned()));
                                        }
                                        Err(error) => {
                                            prepared_issuance.set(None);
                                            issuance_notice.set(Some(credential_issuance_message(error)));
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
                                                let records = match services.list_did_records().execute(ListDidRecordsQuery { profile_id: profile_id.clone() }) {
                                                    Ok(records) => records,
                                                    Err(error) => {
                                                        issuance_notice.set(Some(did_operation_message(error)));
                                                        return;
                                                    }
                                                };
                                                let selection = records.iter()
                                                    .filter(|record| record.document_metadata.deactivated != Some(true))
                                                    .find_map(|record| {
                                                        record.document.relationships.iter()
                                                            .find(|relationship| relationship.relationship == "authentication")
                                                            .and_then(|relationship| relationship.method_ids.iter()
                                                                .find(|method_id| record.managed_method_ids.contains(method_id)))
                                                            .map(|method| (record.document.id.clone(), method.clone()))
                                                    });
                                                let Some((holder_did, method_id)) = selection else {
                                                    issuance_notice.set(Some("Create an active managed DID before accepting this credential offer.".to_owned()));
                                                    return;
                                                };
                                                let service = services.accept_credential_issuance();
                                                let refresh_services = services.clone();
                                                let refresh_profile = profile_id.clone();
                                                let execute_profile = profile_id.clone();
                                                let execute_issuance_id = issuance_id.clone();
                                                issuance_busy.set(true);
                                                issuance_notice.set(None);
                                                spawn(async move {
                                                    match service.execute(AcceptCredentialIssuanceCommand {
                                                        profile_id: execute_profile,
                                                        issuance_id: execute_issuance_id,
                                                        holder_did,
                                                        method_id,
                                                        confirmed: true,
                                                        intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
                                                    }).await {
                                                        Ok(result) => {
                                                            prepared_issuance.set(Some(result));
                                                            issuance_notice.set(Some("Credential issued, verified, and stored in the protected inventory.".to_owned()));
                                                            state.set(load_credential_page(&refresh_services, &refresh_profile));
                                                        }
                                                        Err(error) => issuance_notice.set(Some(credential_issuance_message(error))),
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
                                            move |_| match service.execute(RefuseCredentialIssuanceCommand {
                                                profile_id: profile_id.clone(),
                                                issuance_id: issuance_id.clone(),
                                            }) {
                                                Ok(result) => {
                                                    prepared_issuance.set(Some(result));
                                                    issuance_consent.set(false);
                                                    issuance_notice.set(Some("Credential offer refused; ephemeral protocol secrets were discarded.".to_owned()));
                                                }
                                                Err(error) => issuance_notice.set(Some(credential_issuance_message(error))),
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
                                match service.execute(CredentialProfileQuery { profile_id }).await {
                                    Ok(credential) => {
                                        next.retain(|existing| existing.id != credential.id);
                                        next.push(credential);
                                        next.sort_by(|left, right| left.id.cmp(&right.id));
                                        state.set(CredentialPageState::Ready { credentials: next, receiving: false, operation_error: None });
                                    }
                                    Err(error) => state.set(CredentialPageState::Ready { credentials: next, receiving: false, operation_error: Some(credential_operation_message(error)) }),
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

#[component]
fn DiagnosticsPage(active_profile: WalletProfileView) -> Element {
    let services = consume_context::<WalletUiServices>();
    let credential_protocol_ready = services.standalone_credential_offer().is_some();
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
            CapabilityStatus {
                name: "Credential protocols",
                state: if credential_protocol_ready { "OpenID4VCI 1.0 · standalone".to_owned() } else { "Not connected".to_owned() },
                ready: credential_protocol_ready,
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

    #[test]
    fn long_public_identifiers_are_shortened_for_mobile_display() {
        assert_eq!(truncate_middle("1234567890", 4, 3), "1234…890");
        assert_eq!(truncate_middle("short", 4, 3), "short");
    }
}
