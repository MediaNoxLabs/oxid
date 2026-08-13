// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_did_midnight::{
    HttpDidResolver, HttpDidResolverConfig, HttpDidResolverConfigError,
};
use oxid_adapter_did_midnight::{StandaloneDidLifecycle, StandaloneDidResolver};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{
    MidnightAccountCheckpointConfig, MidnightAccountCheckpointConfigError,
    MidnightDustCheckpointConfig, MidnightDustCheckpointConfigError, MidnightIndexerConfig,
    MidnightIndexerConfigError, MidnightLocalProvingConfig, MidnightLocalProvingConfigError,
    MidnightShieldedCheckpointConfig, MidnightShieldedCheckpointConfigError,
    MidnightStandaloneConfig, MidnightStandaloneConfigError, MidnightSubmissionJournalConfig,
    MidnightSubmissionJournalConfigError, protected_live_midnight_wallet,
    protected_live_midnight_wallet_with_checkpoint_options,
    protected_live_midnight_wallet_with_checkpoints,
    protected_simulated_midnight_wallet_with_submission_journal,
    protected_standalone_midnight_wallet,
    protected_standalone_midnight_wallet_with_all_checkpoints,
    protected_standalone_midnight_wallet_with_checkpoint_options,
    protected_standalone_midnight_wallet_with_checkpoints,
    protected_standalone_midnight_wallet_with_dust_checkpoints,
};
use oxid_adapter_midnight::{protected_simulated_midnight_wallet, unavailable_midnight_wallet};
use oxid_adapter_openid4vci::{
    DidCredentialHolderProof, StandaloneOid4vciIssuer, VerifiedCredentialSink,
};
use oxid_adapter_openid4vp::{CredentialDisclosureCandidateSource, StandaloneOpenId4VpVerifier};
use oxid_adapter_siopv2::{DidSelfIssuedIdentityProof, StandaloneSiopV2Verifier};

/// Returns the public embedded offer for the deterministic standalone issuer.
/// Production composition keeps the issuer port unavailable.
#[must_use]
pub fn standalone_oid4vci_offer() -> String {
    oxid_adapter_openid4vci::standalone_credential_offer()
}

/// Returns the public request-by-reference URI for the deterministic
/// standalone self-issued verifier. Production composition keeps it unavailable.
#[must_use]
pub fn standalone_siopv2_request() -> String {
    oxid_adapter_siopv2::standalone_self_issued_request()
}

/// Returns the public request-by-reference URI for the deterministic
/// standalone OpenID4VP verifier. Production composition keeps it unavailable.
#[must_use]
pub fn standalone_openid4vp_request() -> String {
    oxid_adapter_openid4vp::standalone_openid4vp_request()
}
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_credential_json::EncryptedJsonCredentialRepository;
use oxid_adapter_storage_dev::{DevelopmentWalletSecurity, UnavailableWalletSecurity};
use oxid_adapter_storage_identity_json::JsonDidRecordRepository;
use oxid_adapter_storage_json::JsonWalletProfileRepository;
use oxid_adapter_storage_memory::{
    InMemoryCredentialRepository, InMemoryDidRecordRepository, InMemoryWalletProfileRepository,
};
use oxid_adapter_vc_midnight::{
    DigitalPassportDisclosureAdapter, MidnightCredentialVerifier,
    PreflightOnlyCompactPresentationProof, StandaloneBoundCompactCredentialIssuer,
    StandaloneCredentialInbox,
};
use oxid_credential_application::{
    CredentialDisclosurePort, CredentialInboxPort, CredentialRepository, CredentialService,
    CredentialVerificationPort, DeleteCredentialUseCase, GetCredentialDisclosureUseCase,
    GetCredentialUseCase, ImportVerifiedCredentialUseCase, ListCredentialsUseCase,
    PreviewCredentialDisclosureUseCase, ReceiveCredentialUseCase, RevealCredentialClaimUseCase,
    ReverifyCredentialUseCase, UnavailableCredentialDisclosure, UnavailableCredentialInbox,
    UnavailableCredentialRepository, UnavailableCredentialVerifier,
};
use oxid_identity_application::{
    CreateDidUseCase, DeactivateDidUseCase, DidLifecyclePort, DidRecordRepository,
    DidResolutionPort, DidService, ForgetDidUseCase, GetDidRecordUseCase, ListDidRecordsUseCase,
    ResolveDidUseCase, SignDidPayloadUseCase, UnavailableDidLifecycle,
    UnavailableDidRecordRepository, UnavailableDidResolver, UpdateDidUseCase,
};
use oxid_presentation_application::{
    AcceptCredentialPresentationUseCase, CredentialPresentationProtocolPort,
    CredentialPresentationService, GetCredentialPresentationUseCase,
    ListCredentialPresentationsUseCase, PrepareCredentialPresentationUseCase,
    RefuseCredentialPresentationUseCase, UnavailableCredentialPresentationProtocol,
    UnavailablePresentationVerifier,
};
use oxid_protocol_application::{
    AcceptCredentialIssuanceUseCase, AcceptSelfIssuedAuthenticationUseCase,
    CredentialIssuanceProtocolPort, CredentialIssuanceService, GetCredentialIssuanceUseCase,
    GetSelfIssuedAuthenticationUseCase, IssuedCredentialSinkPort, ListCredentialIssuancesUseCase,
    ListSelfIssuedAuthenticationsUseCase, PrepareCredentialIssuanceUseCase,
    PrepareSelfIssuedAuthenticationUseCase, RefuseCredentialIssuanceUseCase,
    RefuseSelfIssuedAuthenticationUseCase, SelfIssuedAuthenticationProtocolPort,
    SelfIssuedAuthenticationService, UnavailableCredentialIssuanceProtocol,
    UnavailableIssuedCredentialSink, UnavailableSelfIssuedAuthenticationProtocol,
};
use oxid_wallet_application::{
    AuthorizeWalletTransferUseCase, CancelWalletDustSyncUseCase, CancelWalletShieldedSyncUseCase,
    CancelWalletTransferSubmissionUseCase, CreateWalletProfileService, CreateWalletProfileUseCase,
    DeleteWalletKeyUseCase, DeriveWalletAccountUseCase, GenerateWalletKeyUseCase,
    GetActiveWalletProfileService, GetActiveWalletProfileUseCase, GetWalletAccountUseCase,
    GetWalletDustSyncStatusUseCase, GetWalletSecurityStatusUseCase,
    GetWalletShieldedSyncStatusUseCase, GetWalletTransferDraftUseCase,
    GetWalletTransferSubmissionStatusUseCase, InitializeWalletSecurityUseCase,
    ListWalletKeysUseCase, ListWalletNetworksUseCase, ListWalletProfilesService,
    ListWalletProfilesUseCase, ListWalletTransferSubmissionsUseCase, LockWalletUseCase,
    PrepareWalletTransferUseCase, ReconcileWalletTransferSubmissionUseCase,
    SelectWalletNetworkUseCase, SelectWalletProfileService, SelectWalletProfileUseCase,
    SignWalletDataUseCase, StartWalletDustSyncUseCase, StartWalletShieldedSyncUseCase,
    SubmitWalletTransferUseCase, SyncWalletAccountUseCase, UnlockWalletUseCase,
    WalletAccountDerivationPort, WalletAccountDerivationService, WalletAccountReadPort,
    WalletAccountService, WalletDustSyncPort, WalletDustSyncService, WalletKeyOperationPort,
    WalletKeyService, WalletNetworkPort, WalletNetworkService, WalletProfileRepository,
    WalletProtectionPort, WalletProtectionService, WalletShieldedSyncPort,
    WalletShieldedSyncService, WalletTransactionPort, WalletTransactionService,
};

/// Application capabilities shared by every incoming adapter.
#[derive(Clone)]
pub struct ApplicationServices {
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
    list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
    select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
    get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
    get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase>,
    initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase>,
    unlock_wallet: Arc<dyn UnlockWalletUseCase>,
    lock_wallet: Arc<dyn LockWalletUseCase>,
    generate_wallet_key: Arc<dyn GenerateWalletKeyUseCase>,
    list_wallet_keys: Arc<dyn ListWalletKeysUseCase>,
    sign_wallet_data: Arc<dyn SignWalletDataUseCase>,
    delete_wallet_key: Arc<dyn DeleteWalletKeyUseCase>,
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
    get_did_record: Arc<dyn GetDidRecordUseCase>,
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
    get_credential_issuance: Arc<dyn GetCredentialIssuanceUseCase>,
    list_credential_issuances: Arc<dyn ListCredentialIssuancesUseCase>,
    prepare_self_issued_authentication: Arc<dyn PrepareSelfIssuedAuthenticationUseCase>,
    accept_self_issued_authentication: Arc<dyn AcceptSelfIssuedAuthenticationUseCase>,
    refuse_self_issued_authentication: Arc<dyn RefuseSelfIssuedAuthenticationUseCase>,
    get_self_issued_authentication: Arc<dyn GetSelfIssuedAuthenticationUseCase>,
    list_self_issued_authentications: Arc<dyn ListSelfIssuedAuthenticationsUseCase>,
    prepare_credential_presentation: Arc<dyn PrepareCredentialPresentationUseCase>,
    accept_credential_presentation: Arc<dyn AcceptCredentialPresentationUseCase>,
    refuse_credential_presentation: Arc<dyn RefuseCredentialPresentationUseCase>,
    get_credential_presentation: Arc<dyn GetCredentialPresentationUseCase>,
    list_credential_presentations: Arc<dyn ListCredentialPresentationsUseCase>,
}

#[derive(Clone, Copy)]
enum CredentialIssuanceComposition {
    Unavailable,
    Standalone,
}

#[derive(Clone, Copy)]
enum SelfIssuedAuthenticationComposition {
    Unavailable,
    Standalone,
}

#[derive(Clone, Copy)]
enum CredentialPresentationComposition {
    Unavailable,
    Standalone,
}

struct IdentityAdapters {
    did_repository: Arc<dyn DidRecordRepository>,
    did_resolver: Arc<dyn DidResolutionPort>,
    did_lifecycle: Arc<dyn DidLifecyclePort>,
    credential_repository: Arc<dyn CredentialRepository>,
    credential_inbox: Arc<dyn CredentialInboxPort>,
    credential_verifier: Arc<dyn CredentialVerificationPort>,
    credential_disclosure: Arc<dyn CredentialDisclosurePort>,
    credential_issuance: CredentialIssuanceComposition,
    self_issued_authentication: SelfIssuedAuthenticationComposition,
    credential_presentation: CredentialPresentationComposition,
}

impl ApplicationServices {
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
    pub fn generate_wallet_key(&self) -> Arc<dyn GenerateWalletKeyUseCase> {
        Arc::clone(&self.generate_wallet_key)
    }

    #[must_use]
    pub fn list_wallet_keys(&self) -> Arc<dyn ListWalletKeysUseCase> {
        Arc::clone(&self.list_wallet_keys)
    }

    #[must_use]
    pub fn sign_wallet_data(&self) -> Arc<dyn SignWalletDataUseCase> {
        Arc::clone(&self.sign_wallet_data)
    }

    #[must_use]
    pub fn delete_wallet_key(&self) -> Arc<dyn DeleteWalletKeyUseCase> {
        Arc::clone(&self.delete_wallet_key)
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
    pub fn get_did_record(&self) -> Arc<dyn GetDidRecordUseCase> {
        Arc::clone(&self.get_did_record)
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
    pub fn get_credential_issuance(&self) -> Arc<dyn GetCredentialIssuanceUseCase> {
        Arc::clone(&self.get_credential_issuance)
    }

    #[must_use]
    pub fn list_credential_issuances(&self) -> Arc<dyn ListCredentialIssuancesUseCase> {
        Arc::clone(&self.list_credential_issuances)
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
    pub fn get_self_issued_authentication(&self) -> Arc<dyn GetSelfIssuedAuthenticationUseCase> {
        Arc::clone(&self.get_self_issued_authentication)
    }

    #[must_use]
    pub fn list_self_issued_authentications(
        &self,
    ) -> Arc<dyn ListSelfIssuedAuthenticationsUseCase> {
        Arc::clone(&self.list_self_issued_authentications)
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
    pub fn get_credential_presentation(&self) -> Arc<dyn GetCredentialPresentationUseCase> {
        Arc::clone(&self.get_credential_presentation)
    }

    #[must_use]
    pub fn list_credential_presentations(&self) -> Arc<dyn ListCredentialPresentationsUseCase> {
        Arc::clone(&self.list_credential_presentations)
    }
}

/// Wires the application with persistent public-profile metadata storage.
#[must_use]
pub fn compose() -> ApplicationServices {
    compose_with_identity_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        Arc::new(UnavailableWalletSecurity),
        Arc::new(unavailable_midnight_wallet()),
        IdentityAdapters {
            did_repository: Arc::new(UnavailableDidRecordRepository),
            did_resolver: Arc::new(UnavailableDidResolver),
            did_lifecycle: Arc::new(UnavailableDidLifecycle),
            credential_repository: Arc::new(UnavailableCredentialRepository),
            credential_inbox: Arc::new(UnavailableCredentialInbox),
            credential_verifier: Arc::new(UnavailableCredentialVerifier),
            credential_disclosure: Arc::new(UnavailableCredentialDisclosure),
            credential_issuance: CredentialIssuanceComposition::Unavailable,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Unavailable,
            credential_presentation: CredentialPresentationComposition::Unavailable,
        },
    )
}

/// Wires persistent public profiles with an explicit process-local custody
/// adapter for the standalone development harness.
#[must_use]
pub fn compose_headless() -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    #[cfg(not(target_arch = "wasm32"))]
    let midnight = profiles
        .configured_path()
        .and_then(|path| path.parent())
        .map(|directory| directory.join("private/midnight-submissions.json"))
        .and_then(|path| MidnightSubmissionJournalConfig::new(path).ok())
        .map_or_else(
            || protected_simulated_midnight_wallet(Arc::clone(&clock), Arc::clone(&security)),
            |journal| {
                protected_simulated_midnight_wallet_with_submission_journal(
                    journal,
                    Arc::clone(&clock),
                    Arc::clone(&security),
                )
            },
        );
    #[cfg(target_arch = "wasm32")]
    let midnight = Arc::new(protected_simulated_midnight_wallet(
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    #[cfg(not(target_arch = "wasm32"))]
    let midnight = Arc::new(midnight);
    compose_with_adapters(profiles, security, midnight)
}

/// Environment variable holding the selected Midnight network identity.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_NETWORK_ID_ENV: &str = "OXID_MIDNIGHT_NETWORK_ID";
/// Environment variable holding the standalone indexer GraphQL WebSocket route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_INDEXER_WS_URL_ENV: &str = "OXID_MIDNIGHT_INDEXER_WS_URL";
/// Environment variable holding the public unshielded address to observe.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_UNSHIELDED_ADDRESS_ENV: &str = "OXID_MIDNIGHT_UNSHIELDED_ADDRESS";
/// Environment variable holding the standalone indexer GraphQL HTTP route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_INDEXER_HTTP_URL_ENV: &str = "OXID_MIDNIGHT_INDEXER_HTTP_URL";
/// Environment variable holding the standalone Midnight node WebSocket route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_NODE_WS_URL_ENV: &str = "OXID_MIDNIGHT_NODE_WS_URL";
/// Environment variable holding the standalone Midnight proof-server base route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_PROOF_SERVER_URL_ENV: &str = "OXID_MIDNIGHT_PROOF_SERVER_URL";
/// Environment variable holding the app-private authenticated proving cache.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_PROVING_CACHE_DIR_ENV: &str = "OXID_MIDNIGHT_PROVING_CACHE_DIR";
/// Environment variable holding the app-private public-account checkpoint file.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_ACCOUNT_CHECKPOINT_PATH_ENV: &str = "OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH";
/// Environment variable holding the app-private key-scoped DUST checkpoint file.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_DUST_CHECKPOINT_PATH_ENV: &str = "OXID_MIDNIGHT_DUST_CHECKPOINT_PATH";
/// Environment variable holding the app-private key-scoped shielded checkpoint file.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_SHIELDED_CHECKPOINT_PATH_ENV: &str = "OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH";
/// Environment variable holding the app-private public submission journal.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_SUBMISSION_JOURNAL_PATH_ENV: &str = "OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH";
/// Environment variable holding the explicitly trusted Midnight DID resolver base route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_DID_RESOLVER_URL_ENV: &str = "OXID_MIDNIGHT_DID_RESOLVER_URL";
/// Environment variable holding the app-private public DID record file.
pub const DID_STORE_PATH_ENV: &str = "OXID_DID_STORE_PATH";
/// Environment variable holding the app-private encrypted credential file.
pub const CREDENTIAL_STORE_PATH_ENV: &str = "OXID_CREDENTIAL_STORE_PATH";
/// Environment variable holding the development-only credential wrapping key.
pub const CREDENTIAL_KEY_PATH_ENV: &str = "OXID_CREDENTIAL_KEY_PATH";

/// Safe startup failures for optional standalone-indexer composition.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadlessCompositionError {
    IncompleteMidnightIndexerConfiguration,
    NonUnicodeMidnightIndexerConfiguration,
    InvalidMidnightIndexerConfiguration(MidnightIndexerConfigError),
    InvalidMidnightLocalProvingConfiguration(MidnightLocalProvingConfigError),
    InvalidMidnightStandaloneConfiguration(MidnightStandaloneConfigError),
    InvalidMidnightAccountCheckpointConfiguration(MidnightAccountCheckpointConfigError),
    InvalidMidnightDustCheckpointConfiguration(MidnightDustCheckpointConfigError),
    InvalidMidnightShieldedCheckpointConfiguration(MidnightShieldedCheckpointConfigError),
    InvalidMidnightSubmissionJournalConfiguration(MidnightSubmissionJournalConfigError),
    InvalidMidnightDidResolverConfiguration(HttpDidResolverConfigError),
    IncompleteCredentialStoreConfiguration,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for HeadlessCompositionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::IncompleteMidnightIndexerConfiguration => {
                "Midnight live mode requires the read-only indexer values or every submission route plus exactly one local-cache or remote-prover setting"
            }
            Self::NonUnicodeMidnightIndexerConfiguration => {
                "Midnight live-mode configuration must be valid Unicode"
            }
            Self::InvalidMidnightIndexerConfiguration(error) => return error.fmt(formatter),
            Self::InvalidMidnightLocalProvingConfiguration(error) => return error.fmt(formatter),
            Self::InvalidMidnightStandaloneConfiguration(error) => return error.fmt(formatter),
            Self::InvalidMidnightAccountCheckpointConfiguration(error) => {
                return error.fmt(formatter);
            }
            Self::InvalidMidnightDustCheckpointConfiguration(error) => return error.fmt(formatter),
            Self::InvalidMidnightShieldedCheckpointConfiguration(error) => {
                return error.fmt(formatter);
            }
            Self::InvalidMidnightSubmissionJournalConfiguration(error) => {
                return error.fmt(formatter);
            }
            Self::InvalidMidnightDidResolverConfiguration(error) => return error.fmt(formatter),
            Self::IncompleteCredentialStoreConfiguration => {
                "credential store and key paths must be configured together"
            }
        };
        formatter.write_str(message)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for HeadlessCompositionError {}

/// Selects deterministic simulation when no live variables are present, a
/// read-only indexer when the three read values are present, or complete
/// standalone submission when every route and exactly one proving mode are valid.
#[cfg(not(target_arch = "wasm32"))]
pub fn compose_headless_from_environment() -> Result<ApplicationServices, HeadlessCompositionError>
{
    let credential_paths = (
        read_optional_environment(CREDENTIAL_STORE_PATH_ENV)?,
        read_optional_environment(CREDENTIAL_KEY_PATH_ENV)?,
    );
    if matches!(credential_paths, (Some(_), None) | (None, Some(_))) {
        return Err(HeadlessCompositionError::IncompleteCredentialStoreConfiguration);
    }
    read_optional_environment(MIDNIGHT_DID_RESOLVER_URL_ENV)?
        .map(HttpDidResolverConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightDidResolverConfiguration)?;
    let values = [
        read_optional_environment(MIDNIGHT_NETWORK_ID_ENV)?,
        read_optional_environment(MIDNIGHT_INDEXER_WS_URL_ENV)?,
        read_optional_environment(MIDNIGHT_INDEXER_HTTP_URL_ENV)?,
        read_optional_environment(MIDNIGHT_NODE_WS_URL_ENV)?,
        read_optional_environment(MIDNIGHT_PROOF_SERVER_URL_ENV)?,
        read_optional_environment(MIDNIGHT_UNSHIELDED_ADDRESS_ENV)?,
        read_optional_environment(MIDNIGHT_PROVING_CACHE_DIR_ENV)?,
    ];
    let checkpoints = read_optional_environment(MIDNIGHT_ACCOUNT_CHECKPOINT_PATH_ENV)?
        .map(MidnightAccountCheckpointConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightAccountCheckpointConfiguration)?;
    let dust_checkpoints = read_optional_environment(MIDNIGHT_DUST_CHECKPOINT_PATH_ENV)?
        .map(MidnightDustCheckpointConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightDustCheckpointConfiguration)?;
    let shielded_checkpoints = read_optional_environment(MIDNIGHT_SHIELDED_CHECKPOINT_PATH_ENV)?
        .map(MidnightShieldedCheckpointConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightShieldedCheckpointConfiguration)?;
    let submission_journal = read_optional_environment(MIDNIGHT_SUBMISSION_JOURNAL_PATH_ENV)?
        .map(MidnightSubmissionJournalConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightSubmissionJournalConfiguration)?;
    match parse_optional_midnight_config(values)? {
        Some(HeadlessMidnightConfig::Indexer(config))
            if dust_checkpoints.is_none() && submission_journal.is_none() =>
        {
            Ok(compose_headless_live_with_checkpoint_options(
                config,
                checkpoints,
                shielded_checkpoints,
            ))
        }
        Some(HeadlessMidnightConfig::Standalone(config)) => {
            Ok(compose_headless_standalone_with_checkpoint_options(
                config,
                checkpoints,
                dust_checkpoints,
                shielded_checkpoints,
                submission_journal,
            ))
        }
        Some(HeadlessMidnightConfig::Indexer(_))
            if checkpoints.is_some()
                || dust_checkpoints.is_some()
                || shielded_checkpoints.is_some()
                || submission_journal.is_some() =>
        {
            Err(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        }
        None if checkpoints.is_some()
            || dust_checkpoints.is_some()
            || shielded_checkpoints.is_some() =>
        {
            Err(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        }
        None => submission_journal.map_or_else(
            || Ok(compose_headless()),
            |journal| Ok(compose_headless_with_submission_journal(journal)),
        ),
        Some(HeadlessMidnightConfig::Indexer(_)) => {
            Err(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        }
    }
}

/// Wires optional public-account and private shielded checkpoints to a live indexer.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_live_with_checkpoint_options(
    config: MidnightIndexerConfig,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_live_midnight_wallet_with_checkpoint_options(
        config,
        account_checkpoints,
        shielded_checkpoints,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires any reviewed combination of standalone checkpoint stores.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_checkpoint_options(
    config: MidnightStandaloneConfig,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    dust_checkpoints: Option<MidnightDustCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    submission_journal: Option<MidnightSubmissionJournalConfig>,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_checkpoint_options(
            config,
            account_checkpoints,
            dust_checkpoints,
            shielded_checkpoints,
            submission_journal,
            Arc::clone(&clock),
            Arc::clone(&security),
        ),
    );
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires deterministic simulation to an explicit durable public submission journal.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_with_submission_journal(
    journal: MidnightSubmissionJournalConfig,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_simulated_midnight_wallet_with_submission_journal(
        journal,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires persistent public profiles and development custody to an explicitly
/// configured live standalone indexer. Normal mobile composition never calls it.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_live(config: MidnightIndexerConfig) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_live_midnight_wallet(
        config,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires development custody and a public checkpoint store to a live indexer.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_live_with_checkpoints(
    config: MidnightIndexerConfig,
    checkpoints: MidnightAccountCheckpointConfig,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_live_midnight_wallet_with_checkpoints(
        config,
        checkpoints,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires development custody to the complete, explicitly configured standalone stack.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone(config: MidnightStandaloneConfig) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_standalone_midnight_wallet(
        config,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires the complete standalone stack with durable public account checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_checkpoints(
    config: MidnightStandaloneConfig,
    checkpoints: MidnightAccountCheckpointConfig,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_standalone_midnight_wallet_with_checkpoints(
        config,
        checkpoints,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires the complete standalone stack with private key-scoped DUST checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_dust_checkpoints(
    config: MidnightStandaloneConfig,
    dust_checkpoints: MidnightDustCheckpointConfig,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_standalone_midnight_wallet_with_dust_checkpoints(
        config,
        dust_checkpoints,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires the complete standalone stack with public account and private DUST checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_all_checkpoints(
    config: MidnightStandaloneConfig,
    account_checkpoints: MidnightAccountCheckpointConfig,
    dust_checkpoints: MidnightDustCheckpointConfig,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_standalone_midnight_wallet_with_all_checkpoints(
        config,
        account_checkpoints,
        dust_checkpoints,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn read_optional_environment(key: &str) -> Result<Option<String>, HeadlessCompositionError> {
    std::env::var_os(key)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| HeadlessCompositionError::NonUnicodeMidnightIndexerConfiguration)
        })
        .transpose()
}

#[cfg(not(target_arch = "wasm32"))]
enum HeadlessMidnightConfig {
    Indexer(MidnightIndexerConfig),
    Standalone(MidnightStandaloneConfig),
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_optional_midnight_config(
    values: [Option<String>; 7],
) -> Result<Option<HeadlessMidnightConfig>, HeadlessCompositionError> {
    let [
        network_id,
        indexer_ws,
        indexer_http,
        node_ws,
        proof_server,
        address,
        proving_cache,
    ] = values;
    match (
        network_id,
        indexer_ws,
        indexer_http,
        node_ws,
        proof_server,
        address,
        proving_cache,
    ) {
        (None, None, None, None, None, None, None) => Ok(None),
        (Some(network), Some(indexer_ws), None, None, None, Some(address), None) => {
            MidnightIndexerConfig::new(network, indexer_ws, address)
                .map(HeadlessMidnightConfig::Indexer)
                .map(Some)
                .map_err(HeadlessCompositionError::InvalidMidnightIndexerConfiguration)
        }
        (
            Some(network),
            Some(indexer_ws),
            Some(indexer_http),
            Some(node_ws),
            Some(proof_server),
            Some(address),
            None,
        ) => MidnightStandaloneConfig::new(
            network,
            indexer_ws,
            indexer_http,
            node_ws,
            proof_server,
            address,
        )
        .map(HeadlessMidnightConfig::Standalone)
        .map(Some)
        .map_err(HeadlessCompositionError::InvalidMidnightStandaloneConfiguration),
        (
            Some(network),
            Some(indexer_ws),
            Some(indexer_http),
            Some(node_ws),
            None,
            Some(address),
            Some(proving_cache),
        ) => {
            let local_proving = MidnightLocalProvingConfig::new(proving_cache)
                .map_err(HeadlessCompositionError::InvalidMidnightLocalProvingConfiguration)?;
            MidnightStandaloneConfig::new_private(
                network,
                indexer_ws,
                indexer_http,
                node_ws,
                local_proving,
                address,
            )
            .map(HeadlessMidnightConfig::Standalone)
            .map(Some)
            .map_err(HeadlessCompositionError::InvalidMidnightStandaloneConfiguration)
        }
        _ => Err(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration),
    }
}

/// Wires deterministic process-local services for tests and development tools.
#[must_use]
pub fn compose_in_memory() -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_simulated_midnight_wallet(
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    let key_operations: Arc<dyn WalletKeyOperationPort> = security.clone();
    compose_with_identity_adapters(
        Arc::new(InMemoryWalletProfileRepository::new()),
        security,
        midnight,
        IdentityAdapters {
            did_repository: Arc::new(InMemoryDidRecordRepository::new()),
            did_resolver: Arc::new(StandaloneDidResolver),
            did_lifecycle: Arc::new(StandaloneDidLifecycle::new(key_operations)),
            credential_repository: Arc::new(InMemoryCredentialRepository::new()),
            credential_inbox: Arc::new(StandaloneCredentialInbox),
            credential_verifier: Arc::new(MidnightCredentialVerifier::new(Arc::new(
                StandaloneDidResolver,
            ))),
            credential_disclosure: Arc::new(DigitalPassportDisclosureAdapter),
            credential_issuance: CredentialIssuanceComposition::Standalone,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Standalone,
            credential_presentation: CredentialPresentationComposition::Standalone,
        },
    )
}

fn compose_with_adapters<R, S, M>(
    repository: Arc<R>,
    security: Arc<S>,
    midnight: Arc<M>,
) -> ApplicationServices
where
    R: WalletProfileRepository + 'static,
    S: WalletProtectionPort + WalletKeyOperationPort + 'static,
    M: WalletNetworkPort
        + WalletAccountReadPort
        + WalletAccountDerivationPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + WalletTransactionPort
        + 'static,
{
    let key_operations: Arc<dyn WalletKeyOperationPort> = security.clone();
    let did_lifecycle: Arc<dyn DidLifecyclePort> =
        Arc::new(StandaloneDidLifecycle::new(key_operations));
    let did_resolver = headless_did_resolver();
    let verifier: Arc<dyn CredentialVerificationPort> =
        Arc::new(MidnightCredentialVerifier::new(Arc::clone(&did_resolver)));
    compose_with_identity_adapters(
        repository,
        security,
        midnight,
        IdentityAdapters {
            did_repository: headless_did_repository(),
            did_resolver,
            did_lifecycle,
            credential_repository: headless_credential_repository(),
            credential_inbox: Arc::new(StandaloneCredentialInbox),
            credential_verifier: verifier,
            credential_disclosure: Arc::new(DigitalPassportDisclosureAdapter),
            credential_issuance: CredentialIssuanceComposition::Standalone,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Standalone,
            credential_presentation: CredentialPresentationComposition::Standalone,
        },
    )
}

fn compose_with_identity_adapters<R, S, M>(
    repository: Arc<R>,
    security: Arc<S>,
    midnight: Arc<M>,
    identity_adapters: IdentityAdapters,
) -> ApplicationServices
where
    R: WalletProfileRepository + 'static,
    S: WalletProtectionPort + WalletKeyOperationPort + 'static,
    M: WalletNetworkPort
        + WalletAccountReadPort
        + WalletAccountDerivationPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + WalletTransactionPort
        + 'static,
{
    let IdentityAdapters {
        did_repository,
        did_resolver,
        did_lifecycle,
        credential_repository,
        credential_inbox,
        credential_verifier,
        credential_disclosure,
        credential_issuance,
        self_issued_authentication,
        credential_presentation,
    } = identity_adapters;
    let presentation_credential_repository = Arc::clone(&credential_repository);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let create_wallet_profile = Arc::new(CreateWalletProfileService::new(
        Arc::clone(&repository),
        Arc::clone(&clock),
        random,
    ));
    let list_wallet_profiles = Arc::new(ListWalletProfilesService::new(Arc::clone(&repository)));
    let select_wallet_profile = Arc::new(SelectWalletProfileService::new(Arc::clone(&repository)));
    let get_active_wallet_profile = Arc::new(GetActiveWalletProfileService::new(repository));
    let protection = Arc::new(WalletProtectionService::new(Arc::clone(&security)));
    let keys = Arc::new(WalletKeyService::new(security));
    let networks = Arc::new(WalletNetworkService::new(Arc::clone(&midnight)));
    let account_derivation = Arc::new(WalletAccountDerivationService::new(Arc::clone(&midnight)));
    let accounts = Arc::new(WalletAccountService::new(Arc::clone(&midnight)));
    let dust = Arc::new(WalletDustSyncService::new(Arc::clone(&midnight)));
    let shielded = Arc::new(WalletShieldedSyncService::new(Arc::clone(&midnight)));
    let transactions = Arc::new(WalletTransactionService::new(midnight, Arc::clone(&clock)));
    let identity = Arc::new(DidService::from_ports(
        did_repository,
        did_resolver,
        did_lifecycle,
    ));
    let credentials = Arc::new(CredentialService::from_ports(
        credential_repository,
        credential_inbox,
        credential_verifier,
        credential_disclosure,
    ));
    let (issuance_protocol, issuance_sink): (
        Arc<dyn CredentialIssuanceProtocolPort>,
        Arc<dyn IssuedCredentialSinkPort>,
    ) = match credential_issuance {
        CredentialIssuanceComposition::Unavailable => (
            Arc::new(UnavailableCredentialIssuanceProtocol),
            Arc::new(UnavailableIssuedCredentialSink),
        ),
        CredentialIssuanceComposition::Standalone => {
            let get_did: Arc<dyn GetDidRecordUseCase> = identity.clone();
            let sign_did: Arc<dyn SignDidPayloadUseCase> = identity.clone();
            let proof = Arc::new(DidCredentialHolderProof::new(
                Arc::clone(&get_did),
                sign_did,
                clock.clone(),
            ));
            let importer: Arc<dyn ImportVerifiedCredentialUseCase> = credentials.clone();
            (
                Arc::new(StandaloneOid4vciIssuer::with_bound_credential_issuer(
                    proof,
                    get_did,
                    clock.clone(),
                    Arc::new(StandaloneBoundCompactCredentialIssuer),
                )),
                Arc::new(VerifiedCredentialSink::new(importer)),
            )
        }
    };
    let issuance = Arc::new(CredentialIssuanceService::new(
        issuance_protocol,
        issuance_sink,
    ));
    let presentation_protocol: Arc<dyn CredentialPresentationProtocolPort> =
        match credential_presentation {
            CredentialPresentationComposition::Unavailable => {
                Arc::new(UnavailableCredentialPresentationProtocol)
            }
            CredentialPresentationComposition::Standalone => {
                let list: Arc<dyn ListCredentialsUseCase> = credentials.clone();
                let disclosure: Arc<dyn GetCredentialDisclosureUseCase> = credentials.clone();
                Arc::new(StandaloneOpenId4VpVerifier::new(
                    Arc::new(CredentialDisclosureCandidateSource::new(list, disclosure)),
                    Arc::new(PreflightOnlyCompactPresentationProof::new(
                        presentation_credential_repository,
                        clock.clone(),
                    )),
                    Arc::new(UnavailablePresentationVerifier),
                    clock.clone(),
                ))
            }
        };
    let credential_presentation =
        Arc::new(CredentialPresentationService::new(presentation_protocol));
    let self_issued_protocol: Arc<dyn SelfIssuedAuthenticationProtocolPort> =
        match self_issued_authentication {
            SelfIssuedAuthenticationComposition::Unavailable => {
                Arc::new(UnavailableSelfIssuedAuthenticationProtocol)
            }
            SelfIssuedAuthenticationComposition::Standalone => {
                let get_did: Arc<dyn GetDidRecordUseCase> = identity.clone();
                let sign_did: Arc<dyn SignDidPayloadUseCase> = identity.clone();
                let proof = Arc::new(DidSelfIssuedIdentityProof::new(
                    Arc::clone(&get_did),
                    sign_did,
                ));
                Arc::new(StandaloneSiopV2Verifier::new(proof, get_did, clock.clone()))
            }
        };
    let self_issued_authentication =
        Arc::new(SelfIssuedAuthenticationService::new(self_issued_protocol));

    let get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase> = protection.clone();
    let initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase> = protection.clone();
    let unlock_wallet: Arc<dyn UnlockWalletUseCase> = protection.clone();
    let lock_wallet: Arc<dyn LockWalletUseCase> = protection;
    let generate_wallet_key: Arc<dyn GenerateWalletKeyUseCase> = keys.clone();
    let list_wallet_keys: Arc<dyn ListWalletKeysUseCase> = keys.clone();
    let sign_wallet_data: Arc<dyn SignWalletDataUseCase> = keys.clone();
    let delete_wallet_key: Arc<dyn DeleteWalletKeyUseCase> = keys;
    let list_wallet_networks: Arc<dyn ListWalletNetworksUseCase> = networks.clone();
    let select_wallet_network: Arc<dyn SelectWalletNetworkUseCase> = networks;
    let derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase> = account_derivation;
    let get_wallet_account: Arc<dyn GetWalletAccountUseCase> = accounts.clone();
    let sync_wallet_account: Arc<dyn SyncWalletAccountUseCase> = accounts;
    let get_wallet_dust_sync_status: Arc<dyn GetWalletDustSyncStatusUseCase> = dust.clone();
    let start_wallet_dust_sync: Arc<dyn StartWalletDustSyncUseCase> = dust.clone();
    let cancel_wallet_dust_sync: Arc<dyn CancelWalletDustSyncUseCase> = dust;
    let get_wallet_shielded_sync_status: Arc<dyn GetWalletShieldedSyncStatusUseCase> =
        shielded.clone();
    let start_wallet_shielded_sync: Arc<dyn StartWalletShieldedSyncUseCase> = shielded.clone();
    let cancel_wallet_shielded_sync: Arc<dyn CancelWalletShieldedSyncUseCase> = shielded;
    let prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase> = transactions.clone();
    let authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase> = transactions.clone();
    let submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase> = transactions.clone();
    let get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase> = transactions.clone();
    let get_wallet_transfer_submission_status: Arc<dyn GetWalletTransferSubmissionStatusUseCase> =
        transactions.clone();
    let cancel_wallet_transfer_submission: Arc<dyn CancelWalletTransferSubmissionUseCase> =
        transactions.clone();
    let list_wallet_transfer_submissions: Arc<dyn ListWalletTransferSubmissionsUseCase> =
        transactions.clone();
    let reconcile_wallet_transfer_submission: Arc<dyn ReconcileWalletTransferSubmissionUseCase> =
        transactions;
    let create_did: Arc<dyn CreateDidUseCase> = identity.clone();
    let resolve_did: Arc<dyn ResolveDidUseCase> = identity.clone();
    let list_did_records: Arc<dyn ListDidRecordsUseCase> = identity.clone();
    let get_did_record: Arc<dyn GetDidRecordUseCase> = identity.clone();
    let update_did: Arc<dyn UpdateDidUseCase> = identity.clone();
    let deactivate_did: Arc<dyn DeactivateDidUseCase> = identity.clone();
    let sign_did_payload: Arc<dyn SignDidPayloadUseCase> = identity.clone();
    let forget_did: Arc<dyn ForgetDidUseCase> = identity;
    let receive_credential: Arc<dyn ReceiveCredentialUseCase> = credentials.clone();
    let list_credentials: Arc<dyn ListCredentialsUseCase> = credentials.clone();
    let get_credential: Arc<dyn GetCredentialUseCase> = credentials.clone();
    let reverify_credential: Arc<dyn ReverifyCredentialUseCase> = credentials.clone();
    let delete_credential: Arc<dyn DeleteCredentialUseCase> = credentials.clone();
    let get_credential_disclosure: Arc<dyn GetCredentialDisclosureUseCase> = credentials.clone();
    let preview_credential_disclosure: Arc<dyn PreviewCredentialDisclosureUseCase> =
        credentials.clone();
    let reveal_credential_claim: Arc<dyn RevealCredentialClaimUseCase> = credentials;
    let prepare_credential_issuance: Arc<dyn PrepareCredentialIssuanceUseCase> = issuance.clone();
    let accept_credential_issuance: Arc<dyn AcceptCredentialIssuanceUseCase> = issuance.clone();
    let refuse_credential_issuance: Arc<dyn RefuseCredentialIssuanceUseCase> = issuance.clone();
    let get_credential_issuance: Arc<dyn GetCredentialIssuanceUseCase> = issuance.clone();
    let list_credential_issuances: Arc<dyn ListCredentialIssuancesUseCase> = issuance;
    let prepare_self_issued_authentication: Arc<dyn PrepareSelfIssuedAuthenticationUseCase> =
        self_issued_authentication.clone();
    let accept_self_issued_authentication: Arc<dyn AcceptSelfIssuedAuthenticationUseCase> =
        self_issued_authentication.clone();
    let refuse_self_issued_authentication: Arc<dyn RefuseSelfIssuedAuthenticationUseCase> =
        self_issued_authentication.clone();
    let get_self_issued_authentication: Arc<dyn GetSelfIssuedAuthenticationUseCase> =
        self_issued_authentication.clone();
    let list_self_issued_authentications: Arc<dyn ListSelfIssuedAuthenticationsUseCase> =
        self_issued_authentication;
    let prepare_credential_presentation: Arc<dyn PrepareCredentialPresentationUseCase> =
        credential_presentation.clone();
    let accept_credential_presentation: Arc<dyn AcceptCredentialPresentationUseCase> =
        credential_presentation.clone();
    let refuse_credential_presentation: Arc<dyn RefuseCredentialPresentationUseCase> =
        credential_presentation.clone();
    let get_credential_presentation: Arc<dyn GetCredentialPresentationUseCase> =
        credential_presentation.clone();
    let list_credential_presentations: Arc<dyn ListCredentialPresentationsUseCase> =
        credential_presentation;

    ApplicationServices {
        create_wallet_profile,
        list_wallet_profiles,
        select_wallet_profile,
        get_active_wallet_profile,
        get_wallet_security_status,
        initialize_wallet_security,
        unlock_wallet,
        lock_wallet,
        generate_wallet_key,
        list_wallet_keys,
        sign_wallet_data,
        delete_wallet_key,
        list_wallet_networks,
        select_wallet_network,
        derive_wallet_account,
        get_wallet_account,
        sync_wallet_account,
        get_wallet_dust_sync_status,
        start_wallet_dust_sync,
        cancel_wallet_dust_sync,
        get_wallet_shielded_sync_status,
        start_wallet_shielded_sync,
        cancel_wallet_shielded_sync,
        prepare_wallet_transfer,
        authorize_wallet_transfer,
        submit_wallet_transfer,
        get_wallet_transfer_draft,
        get_wallet_transfer_submission_status,
        cancel_wallet_transfer_submission,
        list_wallet_transfer_submissions,
        reconcile_wallet_transfer_submission,
        create_did,
        resolve_did,
        list_did_records,
        get_did_record,
        update_did,
        deactivate_did,
        sign_did_payload,
        forget_did,
        receive_credential,
        list_credentials,
        get_credential,
        reverify_credential,
        delete_credential,
        get_credential_disclosure,
        preview_credential_disclosure,
        reveal_credential_claim,
        prepare_credential_issuance,
        accept_credential_issuance,
        refuse_credential_issuance,
        get_credential_issuance,
        list_credential_issuances,
        prepare_self_issued_authentication,
        accept_self_issued_authentication,
        refuse_self_issued_authentication,
        get_self_issued_authentication,
        list_self_issued_authentications,
        prepare_credential_presentation,
        accept_credential_presentation,
        refuse_credential_presentation,
        get_credential_presentation,
        list_credential_presentations,
    }
}

fn headless_credential_repository() -> Arc<dyn CredentialRepository> {
    let configured = (
        std::env::var_os(CREDENTIAL_STORE_PATH_ENV),
        std::env::var_os(CREDENTIAL_KEY_PATH_ENV),
    );
    let paths = match configured {
        (Some(path), Some(key)) => Some((
            std::path::PathBuf::from(path),
            std::path::PathBuf::from(key),
        )),
        (None, None) => JsonWalletProfileRepository::at_default_location()
            .configured_path()
            .and_then(std::path::Path::parent)
            .map(|directory| {
                (
                    directory.join("private/credentials.enc"),
                    directory.join("private/credentials.key"),
                )
            }),
        _ => None,
    };
    paths.map_or_else(
        || Arc::new(UnavailableCredentialRepository) as Arc<dyn CredentialRepository>,
        |(path, key)| {
            Arc::new(EncryptedJsonCredentialRepository::new(path, key))
                as Arc<dyn CredentialRepository>
        },
    )
}

fn headless_did_repository() -> Arc<dyn DidRecordRepository> {
    let path = std::env::var_os(DID_STORE_PATH_ENV)
        .map(std::path::PathBuf::from)
        .or_else(|| {
            JsonWalletProfileRepository::at_default_location()
                .configured_path()
                .and_then(std::path::Path::parent)
                .map(|directory| directory.join("private/did-records.json"))
        });
    path.map_or_else(
        || Arc::new(UnavailableDidRecordRepository) as Arc<dyn DidRecordRepository>,
        |path| Arc::new(JsonDidRecordRepository::new(path)) as Arc<dyn DidRecordRepository>,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn headless_did_resolver() -> Arc<dyn DidResolutionPort> {
    std::env::var_os(MIDNIGHT_DID_RESOLVER_URL_ENV)
        .and_then(|value| value.into_string().ok())
        .and_then(|value| HttpDidResolverConfig::new(value).ok())
        .map_or_else(
            || Arc::new(StandaloneDidResolver) as Arc<dyn DidResolutionPort>,
            |config| Arc::new(HttpDidResolver::new(config)) as Arc<dyn DidResolutionPort>,
        )
}

#[cfg(target_arch = "wasm32")]
fn headless_did_resolver() -> Arc<dyn DidResolutionPort> {
    Arc::new(StandaloneDidResolver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_credential_application::{
        CredentialOperationError, CredentialProfileQuery, CredentialRepositoryError,
    };
    use oxid_identity_application::{
        DidOperationError, DidRecordRepositoryError, ListDidRecordsQuery,
    };
    use oxid_wallet_application::{
        CreateWalletProfileCommand, WalletAccountQuery, WalletDustSyncCommand,
        WalletProfileSecurityCommand, WalletShieldedSyncCommand,
    };

    #[test]
    fn composed_application_executes_the_vertical_slice() {
        let services = compose_in_memory();
        let result = services
            .create_wallet_profile()
            .execute(CreateWalletProfileCommand {
                display_name: "Composition smoke".to_owned(),
            })
            .expect("composed use case should succeed");

        assert_eq!(result.display_name, "Composition smoke");
        assert!(result.id.starts_with("profile_"));
        assert_eq!(
            services
                .list_wallet_profiles()
                .execute()
                .expect("composed query should succeed"),
            vec![result]
        );
    }

    #[test]
    fn composition_exposes_every_application_capability() {
        let services = compose_in_memory();

        drop(services.create_wallet_profile());
        drop(services.list_wallet_profiles());
        drop(services.select_wallet_profile());
        drop(services.get_active_wallet_profile());
        drop(services.get_wallet_security_status());
        drop(services.initialize_wallet_security());
        drop(services.unlock_wallet());
        drop(services.lock_wallet());
        drop(services.generate_wallet_key());
        drop(services.list_wallet_keys());
        drop(services.sign_wallet_data());
        drop(services.delete_wallet_key());
        drop(services.list_wallet_networks());
        drop(services.select_wallet_network());
        drop(services.derive_wallet_account());
        drop(services.get_wallet_account());
        drop(services.sync_wallet_account());
        drop(services.get_wallet_dust_sync_status());
        drop(services.start_wallet_dust_sync());
        drop(services.cancel_wallet_dust_sync());
        drop(services.get_wallet_shielded_sync_status());
        drop(services.start_wallet_shielded_sync());
        drop(services.cancel_wallet_shielded_sync());
        drop(services.prepare_wallet_transfer());
        drop(services.authorize_wallet_transfer());
        drop(services.submit_wallet_transfer());
        drop(services.get_wallet_transfer_draft());
        drop(services.get_wallet_transfer_submission_status());
        drop(services.cancel_wallet_transfer_submission());
        drop(services.list_wallet_transfer_submissions());
        drop(services.reconcile_wallet_transfer_submission());
        drop(services.resolve_did());
        drop(services.list_did_records());
        drop(services.get_did_record());
        drop(services.forget_did());
        drop(services.receive_credential());
        drop(services.list_credentials());
        drop(services.get_credential());
        drop(services.reverify_credential());
        drop(services.delete_credential());
        drop(services.prepare_credential_issuance());
        drop(services.accept_credential_issuance());
        drop(services.refuse_credential_issuance());
        drop(services.get_credential_issuance());
        drop(services.list_credential_issuances());
        drop(services.prepare_self_issued_authentication());
        drop(services.accept_self_issued_authentication());
        drop(services.refuse_self_issued_authentication());
        drop(services.get_self_issued_authentication());
        drop(services.list_self_issued_authentications());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn explicit_live_compositions_are_constructible_without_network_io() {
        const ADDRESS: &str =
            "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
        let indexer =
            MidnightIndexerConfig::new("devnet", "ws://127.0.0.1:8088/api/v1/graphql/ws", ADDRESS)
                .expect("indexer fixture is valid");
        drop(compose_headless_live(indexer.clone()));
        let checkpoint = MidnightAccountCheckpointConfig::new(
            std::env::temp_dir().join("oxid-composition-account-checkpoints.json"),
        )
        .expect("checkpoint fixture is valid");
        drop(compose_headless_live_with_checkpoints(
            indexer,
            checkpoint.clone(),
        ));

        let remote = MidnightStandaloneConfig::new(
            "devnet",
            "ws://127.0.0.1:8088/api/v1/graphql/ws",
            "http://127.0.0.1:8088/api/v1/graphql",
            "ws://127.0.0.1:9944",
            "http://127.0.0.1:6300",
            ADDRESS,
        )
        .expect("remote standalone fixture is valid");
        drop(compose_headless_standalone(remote.clone()));
        drop(compose_headless_standalone_with_checkpoints(
            remote.clone(),
            checkpoint.clone(),
        ));
        let dust_checkpoint = MidnightDustCheckpointConfig::new(
            std::env::temp_dir().join("oxid-composition-dust-checkpoints.bin"),
        )
        .expect("DUST checkpoint fixture is valid");
        let shielded_checkpoint = MidnightShieldedCheckpointConfig::new(
            std::env::temp_dir().join("oxid-composition-shielded-checkpoints.bin"),
        )
        .expect("shielded checkpoint fixture is valid");
        let submission_journal = MidnightSubmissionJournalConfig::new(
            std::env::temp_dir().join("oxid-composition-submission-journal.json"),
        )
        .expect("submission journal fixture is valid");
        drop(compose_headless_live_with_checkpoint_options(
            remote.indexer().clone(),
            Some(checkpoint.clone()),
            Some(shielded_checkpoint.clone()),
        ));
        drop(compose_headless_standalone_with_dust_checkpoints(
            remote.clone(),
            dust_checkpoint.clone(),
        ));
        drop(compose_headless_standalone_with_all_checkpoints(
            remote.clone(),
            checkpoint.clone(),
            dust_checkpoint.clone(),
        ));
        drop(compose_headless_standalone_with_checkpoint_options(
            remote,
            Some(checkpoint),
            Some(dust_checkpoint),
            Some(shielded_checkpoint),
            Some(submission_journal),
        ));

        let local_proving = MidnightLocalProvingConfig::new(
            std::env::temp_dir().join("oxid-composition-local-proving"),
        )
        .expect("local proving fixture is valid");
        let private = MidnightStandaloneConfig::new_private(
            "devnet",
            "ws://127.0.0.1:8088/api/v1/graphql/ws",
            "http://127.0.0.1:8088/api/v1/graphql",
            "ws://127.0.0.1:9944",
            local_proving,
            ADDRESS,
        )
        .expect("private standalone fixture is valid");
        drop(compose_headless_standalone(private));

        drop(compose());
        drop(compose_headless());
    }

    #[test]
    fn in_memory_composition_exposes_only_development_protection() {
        let services = compose_in_memory();
        let command = WalletProfileSecurityCommand {
            profile_id: "profile_test".to_owned(),
        };
        let initial = services
            .get_wallet_security_status()
            .execute(command.clone())
            .expect("development status should be available");

        assert_eq!(initial.state_name(), "Uninitialized");
        assert_eq!(initial.protection_name(), "Development only");
        assert_eq!(
            services
                .initialize_wallet_security()
                .execute(command)
                .expect("development setup should succeed")
                .state_name(),
            "Unlocked"
        );
    }

    #[test]
    fn production_facing_composition_fails_closed_without_native_custody() {
        let services = compose_with_identity_adapters(
            Arc::new(InMemoryWalletProfileRepository::new()),
            Arc::new(UnavailableWalletSecurity),
            Arc::new(unavailable_midnight_wallet()),
            IdentityAdapters {
                did_repository: Arc::new(UnavailableDidRecordRepository),
                did_resolver: Arc::new(UnavailableDidResolver),
                did_lifecycle: Arc::new(UnavailableDidLifecycle),
                credential_repository: Arc::new(UnavailableCredentialRepository),
                credential_inbox: Arc::new(UnavailableCredentialInbox),
                credential_verifier: Arc::new(UnavailableCredentialVerifier),
                credential_disclosure: Arc::new(UnavailableCredentialDisclosure),
                credential_issuance: CredentialIssuanceComposition::Unavailable,
                self_issued_authentication: SelfIssuedAuthenticationComposition::Unavailable,
                credential_presentation: CredentialPresentationComposition::Unavailable,
            },
        );
        let status = services
            .get_wallet_security_status()
            .execute(WalletProfileSecurityCommand {
                profile_id: "profile_test".to_owned(),
            })
            .expect("unavailable status should be safely reportable");

        assert_eq!(status.state_name(), "Unavailable");
        assert_eq!(status.protection_name(), "Not connected");
        assert_eq!(
            services
                .get_wallet_account()
                .execute(WalletAccountQuery {
                    profile_id: "profile_test".to_owned(),
                })
                .expect("unavailable account state is safe")
                .source,
            "unavailable"
        );
        assert_eq!(
            services
                .get_wallet_dust_sync_status()
                .execute(WalletDustSyncCommand {
                    profile_id: "profile_test".to_owned(),
                })
                .expect("unavailable DUST status is safe")
                .state,
            "unavailable"
        );
        assert!(
            services
                .start_wallet_dust_sync()
                .execute(WalletDustSyncCommand {
                    profile_id: "profile_test".to_owned(),
                })
                .is_err()
        );
        assert_eq!(
            services.list_did_records().execute(ListDidRecordsQuery {
                profile_id: "profile_test".to_owned(),
            }),
            Err(DidOperationError::Persistence(
                DidRecordRepositoryError::Unavailable
            ))
        );
        assert_eq!(
            services.list_credentials().execute(CredentialProfileQuery {
                profile_id: "profile_test".to_owned(),
            }),
            Err(CredentialOperationError::Persistence(
                CredentialRepositoryError::Unavailable
            ))
        );
        assert_eq!(
            services
                .get_wallet_shielded_sync_status()
                .execute(WalletShieldedSyncCommand {
                    profile_id: "profile_test".to_owned(),
                })
                .expect("unavailable shielded status is safe")
                .state,
            "unavailable"
        );
        assert!(
            services
                .start_wallet_shielded_sync()
                .execute(WalletShieldedSyncCommand {
                    profile_id: "profile_test".to_owned(),
                })
                .is_err()
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn standalone_live_configuration_is_all_or_nothing() {
        const ADDRESS: &str =
            "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
        assert!(matches!(
            parse_optional_midnight_config([None, None, None, None, None, None, None]),
            Ok(None)
        ));
        assert!(matches!(
            parse_optional_midnight_config([
                Some("devnet".to_owned()),
                Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
                None,
                None,
                None,
                Some(ADDRESS.to_owned()),
                None,
            ]),
            Ok(Some(HeadlessMidnightConfig::Indexer(_)))
        ));
        assert!(matches!(
            parse_optional_midnight_config([
                Some("devnet".to_owned()),
                Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
                Some("http://127.0.0.1:8088/api/v1/graphql".to_owned()),
                Some("ws://127.0.0.1:9944".to_owned()),
                Some("http://127.0.0.1:6300".to_owned()),
                Some(ADDRESS.to_owned()),
                None,
            ]),
            Ok(Some(HeadlessMidnightConfig::Standalone(_)))
        ));
        let local_cache = std::env::temp_dir().join("oxid-composition-proving-cache");
        assert!(matches!(
            parse_optional_midnight_config([
                Some("devnet".to_owned()),
                Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
                Some("http://127.0.0.1:8088/api/v1/graphql".to_owned()),
                Some("ws://127.0.0.1:9944".to_owned()),
                None,
                Some(ADDRESS.to_owned()),
                Some(local_cache.to_string_lossy().into_owned()),
            ]),
            Ok(Some(HeadlessMidnightConfig::Standalone(_)))
        ));
        assert_eq!(
            parse_optional_midnight_config([
                Some("undeployed".to_owned()),
                None,
                None,
                None,
                None,
                None,
                None,
            ])
            .err(),
            Some(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        );
        assert_eq!(
            parse_optional_midnight_config([
                Some("devnet".to_owned()),
                Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
                Some("http://127.0.0.1:8088/api/v1/graphql".to_owned()),
                Some("ws://127.0.0.1:9944".to_owned()),
                Some("http://127.0.0.1:6300".to_owned()),
                Some(ADDRESS.to_owned()),
                Some(local_cache.to_string_lossy().into_owned()),
            ])
            .err(),
            Some(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        );
    }
}
