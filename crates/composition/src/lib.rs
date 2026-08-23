// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(all(
    feature = "mobile-portal",
    not(any(target_os = "ios", target_os = "android"))
))]
compile_error!("mobile-portal is available only on iOS and Android");

#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        all(not(target_os = "ios"), not(target_os = "android")),
        all(
            feature = "mobile-portal",
            any(target_os = "ios", target_os = "android")
        )
    )
))]
mod portal;

use std::{fmt, sync::Arc};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use oxid_adapter_backup_complete::InMemoryRecoveryJournal;
use oxid_adapter_backup_complete::{CompleteWalletBackupAdapter, RecoveryJournalPort};
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_backup_complete::{FileRecoveryJournal, UnavailableRecoveryJournal};
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_backup_document_mobile::NativePortableWalletBackupDocuments;
use oxid_adapter_backup_portable::PortableCustodyVaultPort;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_deployment_profile::AuthenticatedDeploymentProfile;
use oxid_adapter_diagnostics_memory::InMemoryDiagnosticStore;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_did_midnight::{
    HttpDidResolver, HttpDidResolverConfig, HttpDidResolverConfigError,
};
use oxid_adapter_did_midnight::{StandaloneDidLifecycle, StandaloneDidResolver};
use oxid_adapter_identity_ingress::StrictIdentityRequestRouter;
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_identity_ingress::{NativeIdentityLinkIngress, NativeQrScanner};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{
    MidnightAccountCheckpointConfig, MidnightAccountCheckpointConfigError,
    MidnightDustCheckpointConfig, MidnightDustCheckpointConfigError, MidnightIndexerConfig,
    MidnightIndexerConfigError, MidnightLocalProvingConfig, MidnightLocalProvingConfigError,
    MidnightShieldedCheckpointConfig, MidnightShieldedCheckpointConfigError,
    MidnightStandaloneConfig, MidnightStandaloneConfigError, MidnightSubmissionJournalConfig,
    MidnightSubmissionJournalConfigError, authenticate_midnight_chain_identity,
    configuration_placeholder_address, protected_live_midnight_wallet,
    protected_live_midnight_wallet_with_checkpoint_options,
    protected_live_midnight_wallet_with_checkpoints,
    protected_simulated_midnight_wallet_with_submission_journal,
    protected_standalone_midnight_wallet,
    protected_standalone_midnight_wallet_with_all_checkpoints,
    protected_standalone_midnight_wallet_with_checkpoint_options,
    protected_standalone_midnight_wallet_with_checkpoints,
    protected_standalone_midnight_wallet_with_dust_checkpoints,
};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{
    MidnightContractCallFundingPort, MidnightContractCallFundingRequest,
    MidnightContractCallSubmissionMode, MidnightContractCallSubmissionPort,
    MidnightContractCallSubmissionRequest, MidnightContractCallSubmissionState,
    MidnightContractCallSubmissionStatus,
};
use oxid_adapter_midnight::{MidnightDiagnosticAttachPort, MidnightPublicCallContextSource};
use oxid_adapter_midnight::{protected_simulated_midnight_wallet, unavailable_midnight_wallet};
#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        all(not(target_os = "ios"), not(target_os = "android")),
        all(
            feature = "mobile-portal",
            any(target_os = "ios", target_os = "android")
        )
    )
))]
use oxid_adapter_openid4vci::PortalOid4vciClientFactory;
use oxid_adapter_openid4vci::{
    DidCredentialHolderProof, StandaloneOid4vciIssuer, VerifiedCredentialSink,
};
use oxid_adapter_openid4vp::{CredentialDisclosureCandidateSource, StandaloneOpenId4VpVerifier};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_passport_vault::{
    AuthenticatedPassportVaultStateConfigError, AuthenticatedPassportVaultStateSource,
    FundedPassportVaultCall, JsonPassportVaultRepository, NativePassportVaultContractCall,
    NativePassportVaultContractStateDecoder, NodeAnchoredPassportVaultStateSource,
    PassportVaultCallChainContextSource, PassportVaultCallCompletionPort,
    PassportVaultCallCompletionRequest, PassportVaultCallComposerConfigError,
    PassportVaultCallCompositionContext, PassportVaultCallCompositionContextSource,
    PassportVaultCallFundingPort, PassportVaultCallFundingRequest, PassportVaultStoreConfig,
    PassportVaultStoreConfigError, SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX,
    SimulatedPassportVaultContractCall, SimulatedPassportVaultStateSource,
};
use oxid_adapter_passport_vault::{
    InMemoryPassportVaultRepository, StandalonePassportVaultCredential,
};
use oxid_adapter_siopv2::{DidSelfIssuedIdentityProof, StandaloneSiopV2Verifier};
#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        all(not(target_os = "ios"), not(target_os = "android")),
        all(
            feature = "mobile-portal",
            any(target_os = "ios", target_os = "android")
        )
    )
))]
use portal::{PortalIdentityConfiguration, PortalPrivateMaterialDecoder};

/// Verifies that the Android Portal conformance composition is executing under
/// the repository's QEMU-only runtime boundary. iOS simulator authority is
/// already encoded by its distinct Rust target; non-mobile builds never reach
/// this feature because of the compile-time guard above.
#[cfg(all(feature = "mobile-portal", target_os = "android"))]
pub fn verify_android_portal_virtual_device_profile() -> Result<(), &'static str> {
    oxid_adapter_mobile_native::verify_android_qemu_profile()
        .map_err(|_| "standalone-portal requires Android QEMU at runtime")
}

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

/// Returns the fixed development-only Passport Vault address accepted by the
/// deterministic headless call harness.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub const fn simulated_passport_vault_contract_address_hex() -> &'static str {
    SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX
}
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_platform_system::{NativePublicTextExporter, NativeScreenPrivacy};
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_credential_json::EncryptedJsonCredentialRepository;
use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use oxid_adapter_storage_dev::UnavailableWalletSecurity;
use oxid_adapter_storage_identity_json::JsonDidRecordRepository;
use oxid_adapter_storage_json::JsonWalletProfileRepository;
use oxid_adapter_storage_memory::{
    InMemoryCredentialRepository, InMemoryDidRecordRepository, InMemoryWalletProfileRepository,
};
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_storage_mobile::MobileWalletSecurity;
#[cfg(all(
    feature = "mobile-compact-artifacts",
    any(target_os = "ios", target_os = "android")
))]
use oxid_adapter_vc_midnight::ForegroundCompactPresentationProofWorker;
use oxid_adapter_vc_midnight::{
    CompactHolderProofPort, DigitalPassportDisclosureAdapter, ManagedDidJubjubHolderAuthorization,
    MidnightCredentialVerifier, PreflightOnlyCompactPresentationProof,
    StandaloneBoundCompactCredentialIssuer, StandaloneCredentialInbox,
    standalone_digital_passport_issuer_trust_anchor,
};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_vc_midnight::{
    CompactPresentationArtifactsConfig, CompactPresentationRuntimeError,
    NativeCompactPresentationRuntime, NativeCompactPresentationVerifier,
    ProtectedDigitalPassportPresentationSource,
};
use oxid_credential_application::{
    CredentialDisclosurePort, CredentialInboxPort, CredentialRepository, CredentialService,
    CredentialVerificationPort, DeleteCredentialUseCase, GetCredentialDisclosureUseCase,
    GetCredentialUseCase, ImportVerifiedCredentialUseCase, ListCredentialsUseCase,
    PreviewCredentialDisclosureUseCase, ReceiveCredentialUseCase, RevealCredentialClaimUseCase,
    ReverifyCredentialUseCase, UnavailableCredentialDisclosure, UnavailableCredentialInbox,
    UnavailableCredentialRepository, UnavailableCredentialVerifier,
};
use oxid_diagnostics_application::{
    ClearDiagnosticsUseCase, DiagnosticEventSinkPort, DiagnosticsService,
    GetDiagnosticSnapshotUseCase,
};
use oxid_identity_application::{
    CreateDidUseCase, DeactivateDidUseCase, DidJubjubChallengeSigningPort, DidLifecyclePort,
    DidRecordRepository, DidResolutionPort, DidService, ForgetDidUseCase, GetDidRecordUseCase,
    ListDidRecordsUseCase, ResolveDidUseCase, SignDidPayloadUseCase, UnavailableDidLifecycle,
    UnavailableDidRecordRepository, UnavailableDidResolver, UpdateDidUseCase,
};
use oxid_passport_vault_application::{
    AuthorizePassportVaultCallUseCase, CancelPassportVaultCallSubmissionUseCase,
    ClaimPassportVaultLockUseCase, CreatePassportVaultLockUseCase,
    DecodePassportVaultContractStateUseCase, DepositPassportVaultLockUseCase,
    GetPassportVaultCallSubmissionStatusUseCase, GetPassportVaultCallUseCase,
    ListPassportVaultCallSubmissionsUseCase, ListPassportVaultLocksUseCase,
    PassportVaultContractCallService, PassportVaultContractStateDecoderPort,
    PassportVaultContractStateService, PassportVaultContractStateSourcePort,
    PassportVaultCredentialPort, PassportVaultRepository, PassportVaultService,
    PreparePassportVaultCallUseCase, ReadPassportVaultContractStateUseCase,
    ReconcilePassportVaultCallSubmissionUseCase, SubmitPassportVaultCallUseCase,
    UnavailablePassportVaultContractCall, UnavailablePassportVaultContractStateSource,
    UnavailablePassportVaultCredential, UnavailablePassportVaultRepository,
    WithdrawPassportVaultLockUseCase,
};
#[cfg(not(target_arch = "wasm32"))]
use oxid_passport_vault_application::{
    PassportVaultCallDraftId, PassportVaultCallInclusion, PassportVaultCallPortError,
    PassportVaultCallSubmissionState, PassportVaultCallSubmissionStatus,
    PassportVaultContractStateSnapshot,
};
use oxid_platform_ports::{
    IdentityLinkIngressPort, PublicTextExportPort, QrScannerPort, ScreenPrivacyPort,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use oxid_platform_ports::{
    UnavailableIdentityLinkIngress, UnavailablePublicTextExporter, UnavailableQrScanner,
    UnavailableScreenPrivacy,
};
use oxid_presentation_application::{
    AcceptCredentialPresentationUseCase, CancelCredentialPresentationUseCase,
    CredentialPresentationProtocolPort, CredentialPresentationService,
    GetCredentialPresentationUseCase, ListCredentialPresentationsUseCase,
    PrepareCredentialPresentationUseCase, RefuseCredentialPresentationUseCase,
    SetCredentialPresentationForegroundUseCase, UnavailableCredentialPresentationProtocol,
    UnavailablePresentationVerifier,
};
#[cfg(all(
    feature = "mobile-compact-artifacts",
    any(target_os = "ios", target_os = "android")
))]
use oxid_presentation_application::{PresentationProofControlPort, PresentationProofPort};
use oxid_protocol_application::{
    AcceptCredentialIssuanceUseCase, AcceptSelfIssuedAuthenticationUseCase,
    CredentialIssuanceProtocolPort, CredentialIssuanceService, GetCredentialIssuanceUseCase,
    GetSelfIssuedAuthenticationUseCase, IdentityRequestRouterPort, IdentityRequestRoutingService,
    IssuedCredentialSinkPort, ListCredentialIssuancesUseCase, ListSelfIssuedAuthenticationsUseCase,
    PrepareCredentialIssuanceUseCase, PrepareSelfIssuedAuthenticationUseCase,
    RefuseCredentialIssuanceUseCase, RefuseSelfIssuedAuthenticationUseCase,
    RouteIdentityRequestUseCase, SelfIssuedAuthenticationProtocolPort,
    SelfIssuedAuthenticationService, UnavailableCredentialIssuanceProtocol,
    UnavailableIssuedCredentialSink, UnavailableSelfIssuedAuthenticationProtocol,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use oxid_wallet_application::UnavailablePortableWalletBackupDocuments;
#[cfg(target_arch = "wasm32")]
use oxid_wallet_application::UnavailableWalletDustRegistrationPort;
#[cfg(not(target_arch = "wasm32"))]
use oxid_wallet_application::WalletDustRegistrationPort;
use oxid_wallet_application::{
    AuthorizeWalletDustRegistrationUseCase, AuthorizeWalletTransferUseCase,
    CancelWalletDustRegistrationSubmissionUseCase, CancelWalletDustSyncUseCase,
    CancelWalletShieldedSyncUseCase, CancelWalletTransferSubmissionUseCase,
    CompleteWalletBackupService, CreateWalletProfileService, CreateWalletProfileUseCase,
    DeleteWalletKeyUseCase, DeriveWalletAccountUseCase, ExportCompleteWalletBackupUseCase,
    ExportPortableWalletBackupUseCase, GenerateWalletKeyUseCase, GetActiveWalletProfileService,
    GetActiveWalletProfileUseCase, GetWalletAccountUseCase, GetWalletBackupReceiptUseCase,
    GetWalletDustRegistrationStatusUseCase, GetWalletDustRegistrationUseCase,
    GetWalletDustSyncStatusUseCase, GetWalletSecurityStatusUseCase,
    GetWalletShieldedSyncStatusUseCase, GetWalletTransferDraftUseCase,
    GetWalletTransferSubmissionStatusUseCase, InitializeWalletSecurityUseCase,
    ListWalletKeysUseCase, ListWalletNetworksUseCase, ListWalletProfilesService,
    ListWalletProfilesUseCase, ListWalletTransferSubmissionsUseCase, LockWalletUseCase,
    PortableWalletBackupDocumentPort, PrepareShieldedWalletTransferUseCase,
    PrepareWalletDustRegistrationUseCase, PrepareWalletTransferUseCase,
    ReconcileWalletDustRegistrationSubmissionUseCase, ReconcileWalletTransferSubmissionUseCase,
    RecordWalletBackupReceiptUseCase, RecoverCompleteWalletBackupUseCase,
    RecoverPortableWalletBackupUseCase, SelectWalletNetworkUseCase, SelectWalletProfileService,
    SelectWalletProfileUseCase, SignWalletDataUseCase, StartWalletDustSyncUseCase,
    StartWalletShieldedSyncUseCase, SubmitWalletDustRegistrationUseCase,
    SubmitWalletTransferUseCase, SyncWalletAccountUseCase, UnlockWalletUseCase,
    WalletAccountDerivationPort, WalletAccountDerivationService, WalletAccountReadPort,
    WalletAccountService, WalletBackupReceiptRepository, WalletBackupReceiptService,
    WalletDustRegistrationService, WalletDustSyncPort, WalletDustSyncService,
    WalletJubjubChallengeSigningPort, WalletKeyOperationPort, WalletKeyService, WalletNetworkPort,
    WalletNetworkService, WalletPortableBackupPort, WalletPortableBackupService,
    WalletProfileAssociationRepository, WalletProfileRepository, WalletProtectionPort,
    WalletProtectionService, WalletShieldedSyncPort, WalletShieldedSyncService,
    WalletTransactionPort, WalletTransactionService,
};

#[cfg(not(target_arch = "wasm32"))]
trait NativeMidnightCompositionCapability:
    MidnightContractCallFundingPort + MidnightContractCallSubmissionPort
{
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> NativeMidnightCompositionCapability for T where
    T: MidnightContractCallFundingPort + MidnightContractCallSubmissionPort
{
}

#[cfg(target_arch = "wasm32")]
trait NativeMidnightCompositionCapability {}

#[cfg(target_arch = "wasm32")]
impl<T> NativeMidnightCompositionCapability for T {}

#[cfg(not(target_arch = "wasm32"))]
trait NativeWalletDustRegistrationCapability: WalletDustRegistrationPort {}

#[cfg(not(target_arch = "wasm32"))]
impl<T> NativeWalletDustRegistrationCapability for T where T: WalletDustRegistrationPort {}

#[cfg(target_arch = "wasm32")]
trait NativeWalletDustRegistrationCapability {}

#[cfg(target_arch = "wasm32")]
impl<T> NativeWalletDustRegistrationCapability for T {}

/// Application capabilities shared by every incoming adapter.
#[derive(Clone)]
pub struct ApplicationServices {
    diagnostic_events: Arc<dyn DiagnosticEventSinkPort>,
    get_diagnostic_snapshot: Arc<dyn GetDiagnosticSnapshotUseCase>,
    clear_diagnostics: Arc<dyn ClearDiagnosticsUseCase>,
    qr_scanner: Arc<dyn QrScannerPort>,
    identity_link_ingress: Arc<dyn IdentityLinkIngressPort>,
    public_text_exporter: Arc<dyn PublicTextExportPort>,
    screen_privacy: Arc<dyn ScreenPrivacyPort>,
    portable_wallet_backup_documents: Arc<dyn PortableWalletBackupDocumentPort>,
    route_identity_request: Arc<dyn RouteIdentityRequestUseCase>,
    midnight_public_call_context: Arc<dyn MidnightPublicCallContextSource>,
    #[cfg(not(target_arch = "wasm32"))]
    midnight_contract_call_funding: Arc<dyn MidnightContractCallFundingPort>,
    #[cfg(not(target_arch = "wasm32"))]
    midnight_contract_call_submission: Arc<dyn MidnightContractCallSubmissionPort>,
    #[cfg(not(target_arch = "wasm32"))]
    protected_passport_vault_presentations: Option<Arc<ProtectedDigitalPassportPresentationSource>>,
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
    list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
    select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
    get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
    get_wallet_backup_receipt: Arc<dyn GetWalletBackupReceiptUseCase>,
    record_wallet_backup_receipt: Arc<dyn RecordWalletBackupReceiptUseCase>,
    get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase>,
    initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase>,
    unlock_wallet: Arc<dyn UnlockWalletUseCase>,
    lock_wallet: Arc<dyn LockWalletUseCase>,
    export_portable_wallet_backup: Arc<dyn ExportPortableWalletBackupUseCase>,
    recover_portable_wallet_backup: Arc<dyn RecoverPortableWalletBackupUseCase>,
    export_complete_wallet_backup: Arc<dyn ExportCompleteWalletBackupUseCase>,
    recover_complete_wallet_backup: Arc<dyn RecoverCompleteWalletBackupUseCase>,
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
    prepare_wallet_dust_registration: Arc<dyn PrepareWalletDustRegistrationUseCase>,
    authorize_wallet_dust_registration: Arc<dyn AuthorizeWalletDustRegistrationUseCase>,
    submit_wallet_dust_registration: Arc<dyn SubmitWalletDustRegistrationUseCase>,
    get_wallet_dust_registration: Arc<dyn GetWalletDustRegistrationUseCase>,
    get_wallet_dust_registration_status: Arc<dyn GetWalletDustRegistrationStatusUseCase>,
    cancel_wallet_dust_registration_submission:
        Arc<dyn CancelWalletDustRegistrationSubmissionUseCase>,
    reconcile_wallet_dust_registration_submission:
        Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase>,
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
    cancel_credential_presentation: Arc<dyn CancelCredentialPresentationUseCase>,
    set_credential_presentation_foreground: Arc<dyn SetCredentialPresentationForegroundUseCase>,
    refuse_credential_presentation: Arc<dyn RefuseCredentialPresentationUseCase>,
    get_credential_presentation: Arc<dyn GetCredentialPresentationUseCase>,
    list_credential_presentations: Arc<dyn ListCredentialPresentationsUseCase>,
    list_passport_vault_locks: Arc<dyn ListPassportVaultLocksUseCase>,
    decode_passport_vault_contract_state: Arc<dyn DecodePassportVaultContractStateUseCase>,
    read_passport_vault_contract_state: Arc<dyn ReadPassportVaultContractStateUseCase>,
    create_passport_vault_lock: Arc<dyn CreatePassportVaultLockUseCase>,
    deposit_passport_vault_lock: Arc<dyn DepositPassportVaultLockUseCase>,
    claim_passport_vault_lock: Arc<dyn ClaimPassportVaultLockUseCase>,
    withdraw_passport_vault_lock: Arc<dyn WithdrawPassportVaultLockUseCase>,
    prepare_passport_vault_call: Arc<dyn PreparePassportVaultCallUseCase>,
    authorize_passport_vault_call: Arc<dyn AuthorizePassportVaultCallUseCase>,
    submit_passport_vault_call: Arc<dyn SubmitPassportVaultCallUseCase>,
    get_passport_vault_call: Arc<dyn GetPassportVaultCallUseCase>,
    get_passport_vault_call_submission_status: Arc<dyn GetPassportVaultCallSubmissionStatusUseCase>,
    cancel_passport_vault_call_submission: Arc<dyn CancelPassportVaultCallSubmissionUseCase>,
    list_passport_vault_call_submissions: Arc<dyn ListPassportVaultCallSubmissionsUseCase>,
    reconcile_passport_vault_call_submission: Arc<dyn ReconcilePassportVaultCallSubmissionUseCase>,
    passport_vault_call_mode: &'static str,
    passport_vault_call_contract_address_hex: Option<&'static str>,
    passport_vault_state_persistence: &'static str,
    compact_presentation_proof_available: bool,
}

enum CredentialIssuanceComposition {
    Unavailable,
    Standalone,
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(
            all(not(target_os = "ios"), not(target_os = "android")),
            all(
                feature = "mobile-portal",
                any(target_os = "ios", target_os = "android")
            )
        )
    ))]
    Portal(Box<PortalOid4vciClientFactory>),
}

#[derive(Clone, Copy)]
enum HeadlessCredentialProfile {
    Standalone,
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(
            all(not(target_os = "ios"), not(target_os = "android")),
            all(
                feature = "mobile-portal",
                any(target_os = "ios", target_os = "android")
            )
        )
    ))]
    Portal,
}

#[derive(Clone, Copy)]
enum SelfIssuedAuthenticationComposition {
    Unavailable,
    Standalone,
}

#[derive(Clone)]
enum CredentialPresentationComposition {
    Unavailable,
    Standalone,
    #[cfg(not(target_arch = "wasm32"))]
    StandaloneZk(Arc<NativeCompactPresentationRuntime>),
    #[cfg(all(
        feature = "mobile-compact-artifacts",
        any(target_os = "ios", target_os = "android")
    ))]
    StandaloneMobileZk(Arc<NativeCompactPresentationRuntime>),
}

struct IdentityAdapters {
    did_repository: Arc<dyn DidRecordRepository>,
    did_resolver: Arc<dyn DidResolutionPort>,
    did_lifecycle: Arc<dyn DidLifecyclePort>,
    did_jubjub_challenge_signing: Arc<dyn DidJubjubChallengeSigningPort>,
    credential_repository: Arc<dyn CredentialRepository>,
    credential_inbox: Arc<dyn CredentialInboxPort>,
    credential_verifier: Arc<dyn CredentialVerificationPort>,
    credential_disclosure: Arc<dyn CredentialDisclosurePort>,
    credential_issuance: CredentialIssuanceComposition,
    self_issued_authentication: SelfIssuedAuthenticationComposition,
    credential_presentation: CredentialPresentationComposition,
    portal_test_ingress: bool,
}

struct PassportVaultRepositoryComposition {
    repository: Arc<dyn PassportVaultRepository>,
    persistence: &'static str,
}

impl PassportVaultRepositoryComposition {
    fn unavailable() -> Self {
        Self {
            repository: Arc::new(UnavailablePassportVaultRepository),
            persistence: "unavailable",
        }
    }

    fn process_local() -> Self {
        Self {
            repository: Arc::new(InMemoryPassportVaultRepository::default()),
            persistence: "process_local",
        }
    }
}

impl ApplicationServices {
    #[must_use]
    pub fn diagnostic_events(&self) -> Arc<dyn DiagnosticEventSinkPort> {
        Arc::clone(&self.diagnostic_events)
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
    pub fn screen_privacy(&self) -> Arc<dyn ScreenPrivacyPort> {
        Arc::clone(&self.screen_privacy)
    }

    #[must_use]
    pub fn portable_wallet_backup_documents(&self) -> Arc<dyn PortableWalletBackupDocumentPort> {
        Arc::clone(&self.portable_wallet_backup_documents)
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
    pub fn get_wallet_backup_receipt(&self) -> Arc<dyn GetWalletBackupReceiptUseCase> {
        Arc::clone(&self.get_wallet_backup_receipt)
    }

    #[must_use]
    pub fn record_wallet_backup_receipt(&self) -> Arc<dyn RecordWalletBackupReceiptUseCase> {
        Arc::clone(&self.record_wallet_backup_receipt)
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
    pub fn export_portable_wallet_backup(&self) -> Arc<dyn ExportPortableWalletBackupUseCase> {
        Arc::clone(&self.export_portable_wallet_backup)
    }

    #[must_use]
    pub fn recover_portable_wallet_backup(&self) -> Arc<dyn RecoverPortableWalletBackupUseCase> {
        Arc::clone(&self.recover_portable_wallet_backup)
    }

    #[must_use]
    pub fn export_complete_wallet_backup(&self) -> Arc<dyn ExportCompleteWalletBackupUseCase> {
        Arc::clone(&self.export_complete_wallet_backup)
    }

    #[must_use]
    pub fn recover_complete_wallet_backup(&self) -> Arc<dyn RecoverCompleteWalletBackupUseCase> {
        Arc::clone(&self.recover_complete_wallet_backup)
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
    pub fn prepare_wallet_dust_registration(
        &self,
    ) -> Arc<dyn PrepareWalletDustRegistrationUseCase> {
        Arc::clone(&self.prepare_wallet_dust_registration)
    }

    #[must_use]
    pub fn authorize_wallet_dust_registration(
        &self,
    ) -> Arc<dyn AuthorizeWalletDustRegistrationUseCase> {
        Arc::clone(&self.authorize_wallet_dust_registration)
    }

    #[must_use]
    pub fn submit_wallet_dust_registration(&self) -> Arc<dyn SubmitWalletDustRegistrationUseCase> {
        Arc::clone(&self.submit_wallet_dust_registration)
    }

    #[must_use]
    pub fn get_wallet_dust_registration(&self) -> Arc<dyn GetWalletDustRegistrationUseCase> {
        Arc::clone(&self.get_wallet_dust_registration)
    }

    #[must_use]
    pub fn get_wallet_dust_registration_status(
        &self,
    ) -> Arc<dyn GetWalletDustRegistrationStatusUseCase> {
        Arc::clone(&self.get_wallet_dust_registration_status)
    }

    #[must_use]
    pub fn cancel_wallet_dust_registration_submission(
        &self,
    ) -> Arc<dyn CancelWalletDustRegistrationSubmissionUseCase> {
        Arc::clone(&self.cancel_wallet_dust_registration_submission)
    }

    #[must_use]
    pub fn reconcile_wallet_dust_registration_submission(
        &self,
    ) -> Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase> {
        Arc::clone(&self.reconcile_wallet_dust_registration_submission)
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
    pub fn cancel_credential_presentation(&self) -> Arc<dyn CancelCredentialPresentationUseCase> {
        Arc::clone(&self.cancel_credential_presentation)
    }

    #[must_use]
    pub fn set_credential_presentation_foreground(
        &self,
    ) -> Arc<dyn SetCredentialPresentationForegroundUseCase> {
        Arc::clone(&self.set_credential_presentation_foreground)
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

    #[must_use]
    pub fn list_passport_vault_locks(&self) -> Arc<dyn ListPassportVaultLocksUseCase> {
        Arc::clone(&self.list_passport_vault_locks)
    }

    #[must_use]
    pub fn decode_passport_vault_contract_state(
        &self,
    ) -> Arc<dyn DecodePassportVaultContractStateUseCase> {
        Arc::clone(&self.decode_passport_vault_contract_state)
    }

    #[must_use]
    pub fn read_passport_vault_contract_state(
        &self,
    ) -> Arc<dyn ReadPassportVaultContractStateUseCase> {
        Arc::clone(&self.read_passport_vault_contract_state)
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
    pub fn prepare_passport_vault_call(&self) -> Arc<dyn PreparePassportVaultCallUseCase> {
        Arc::clone(&self.prepare_passport_vault_call)
    }

    #[must_use]
    pub fn authorize_passport_vault_call(&self) -> Arc<dyn AuthorizePassportVaultCallUseCase> {
        Arc::clone(&self.authorize_passport_vault_call)
    }

    #[must_use]
    pub fn submit_passport_vault_call(&self) -> Arc<dyn SubmitPassportVaultCallUseCase> {
        Arc::clone(&self.submit_passport_vault_call)
    }

    #[must_use]
    pub fn get_passport_vault_call(&self) -> Arc<dyn GetPassportVaultCallUseCase> {
        Arc::clone(&self.get_passport_vault_call)
    }

    #[must_use]
    pub fn get_passport_vault_call_submission_status(
        &self,
    ) -> Arc<dyn GetPassportVaultCallSubmissionStatusUseCase> {
        Arc::clone(&self.get_passport_vault_call_submission_status)
    }

    #[must_use]
    pub fn cancel_passport_vault_call_submission(
        &self,
    ) -> Arc<dyn CancelPassportVaultCallSubmissionUseCase> {
        Arc::clone(&self.cancel_passport_vault_call_submission)
    }

    #[must_use]
    pub fn list_passport_vault_call_submissions(
        &self,
    ) -> Arc<dyn ListPassportVaultCallSubmissionsUseCase> {
        Arc::clone(&self.list_passport_vault_call_submissions)
    }

    #[must_use]
    pub fn reconcile_passport_vault_call_submission(
        &self,
    ) -> Arc<dyn ReconcilePassportVaultCallSubmissionUseCase> {
        Arc::clone(&self.reconcile_passport_vault_call_submission)
    }

    /// Returns the explicit adapter mode for incoming capability discovery.
    #[must_use]
    pub const fn passport_vault_call_mode(&self) -> &'static str {
        self.passport_vault_call_mode
    }

    /// Returns the fixed address only for deterministic development simulation.
    #[must_use]
    pub const fn passport_vault_call_contract_address_hex(&self) -> Option<&'static str> {
        self.passport_vault_call_contract_address_hex
    }

    /// Reports only how the standalone conformance ledger is retained. This
    /// never describes native contract state or contract-call history.
    #[must_use]
    pub const fn passport_vault_state_persistence(&self) -> &'static str {
        self.passport_vault_state_persistence
    }

    /// Reports whether an authenticated Compact prover and an independent
    /// verifier are connected to this composition.
    #[must_use]
    pub const fn compact_presentation_proof_available(&self) -> bool {
        self.compact_presentation_proof_available
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn complete_wallet_recovery_journal() -> Arc<dyn RecoveryJournalPort> {
    JsonWalletProfileRepository::at_default_location()
        .configured_path()
        .and_then(std::path::Path::parent)
        .map(|directory| directory.join("private/complete-wallet-recovery.json"))
        .and_then(|path| FileRecoveryJournal::new(path).ok())
        .map_or_else(
            || Arc::new(UnavailableRecoveryJournal) as Arc<dyn RecoveryJournalPort>,
            |journal| Arc::new(journal) as Arc<dyn RecoveryJournalPort>,
        )
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn complete_wallet_recovery_journal() -> Arc<dyn RecoveryJournalPort> {
    Arc::new(InMemoryRecoveryJournal::default())
}

/// A signed deployment profile after the configured node has also proven the
/// exact genesis hash bound by that profile.
#[cfg(not(target_arch = "wasm32"))]
pub struct AuthenticatedProductionDeployment {
    profile: AuthenticatedDeploymentProfile,
    midnight: MidnightStandaloneConfig,
}

#[cfg(not(target_arch = "wasm32"))]
impl fmt::Debug for AuthenticatedProductionDeployment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedProductionDeployment")
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AuthenticatedProductionDeployment {
    #[must_use]
    pub const fn profile(&self) -> &AuthenticatedDeploymentProfile {
        &self.profile
    }
}

/// Payload-free failures from the production deployment composition gate.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductionDeploymentCompositionError {
    InvalidMidnightProfile,
    ChainIdentityUnavailable,
    ChainIdentityMismatch,
    InvalidSsiProfile,
}

#[cfg(not(target_arch = "wasm32"))]
impl fmt::Display for ProductionDeploymentCompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidMidnightProfile => "authenticated Midnight deployment profile is invalid",
            Self::ChainIdentityUnavailable => {
                "authenticated Midnight chain identity is unavailable"
            }
            Self::ChainIdentityMismatch => {
                "authenticated Midnight chain identity does not match the node"
            }
            Self::InvalidSsiProfile => "authenticated SSI deployment profile is invalid",
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for ProductionDeploymentCompositionError {}

/// Binds a signed deployment profile to the genesis hash returned by its
/// reviewed node route. The caller cannot provide alternate endpoints after
/// this asynchronous gate succeeds.
#[cfg(not(target_arch = "wasm32"))]
pub async fn authenticate_production_deployment(
    profile: AuthenticatedDeploymentProfile,
) -> Result<AuthenticatedProductionDeployment, ProductionDeploymentCompositionError> {
    let midnight = profile.midnight();
    let placeholder = configuration_placeholder_address(midnight.network_id())
        .map_err(|_| ProductionDeploymentCompositionError::InvalidMidnightProfile)?;
    let config = MidnightStandaloneConfig::new(
        midnight.network_id(),
        midnight.indexer_websocket_url(),
        midnight.indexer_http_url(),
        midnight.node_websocket_url(),
        midnight.proof_server_url(),
        placeholder.value(),
    )
    .map_err(|_| ProductionDeploymentCompositionError::InvalidMidnightProfile)?;
    authenticate_midnight_chain_identity(midnight.node_websocket_url(), midnight.genesis_hash())
        .await
        .map_err(|error| match error {
            oxid_adapter_midnight::MidnightChainIdentityError::GenesisMismatch => {
                ProductionDeploymentCompositionError::ChainIdentityMismatch
            }
            oxid_adapter_midnight::MidnightChainIdentityError::InvalidNodeEndpoint
            | oxid_adapter_midnight::MidnightChainIdentityError::NodeUnavailable => {
                ProductionDeploymentCompositionError::ChainIdentityUnavailable
            }
        })?;
    Ok(AuthenticatedProductionDeployment {
        profile,
        midnight: config,
    })
}

/// Composes the live Midnight path only after profile-signature and node
/// genesis authentication. The default [`compose`] function remains
/// fail-closed and never calls this opt-in constructor.
///
/// The authenticated DID resolver is enabled from the same signed profile.
/// Issuer and verifier HTTP protocol adapters remain unavailable until their
/// independent metadata/transport implementation is reviewed.
#[cfg(not(target_arch = "wasm32"))]
pub fn compose_authenticated_production(
    deployment: AuthenticatedProductionDeployment,
) -> Result<ApplicationServices, ProductionDeploymentCompositionError> {
    let did_resolver = HttpDidResolverConfig::new(deployment.profile.ssi().did_resolver_url())
        .map(HttpDidResolver::new)
        .map_err(|_| ProductionDeploymentCompositionError::InvalidSsiProfile)?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let security = {
        let clock = Arc::new(SystemClock);
        let random = Arc::new(OsRandom);
        Arc::new(MobileWalletSecurity::native(clock, random))
    };
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let security = Arc::new(UnavailableWalletSecurity);
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let clock = Arc::new(SystemClock);
    let midnight = Arc::new(
        protected_standalone_midnight_wallet(
            deployment.midnight,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    Ok(compose_with_identity_adapters(
        profiles,
        security,
        midnight,
        IdentityAdapters {
            did_repository: Arc::new(UnavailableDidRecordRepository),
            did_resolver: Arc::new(did_resolver),
            did_lifecycle: Arc::new(UnavailableDidLifecycle),
            did_jubjub_challenge_signing: Arc::new(UnavailableDidLifecycle),
            credential_repository: Arc::new(UnavailableCredentialRepository),
            credential_inbox: Arc::new(UnavailableCredentialInbox),
            credential_verifier: Arc::new(UnavailableCredentialVerifier),
            credential_disclosure: Arc::new(UnavailableCredentialDisclosure),
            credential_issuance: CredentialIssuanceComposition::Unavailable,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Unavailable,
            credential_presentation: CredentialPresentationComposition::Unavailable,
            portal_test_ingress: false,
        },
        PassportVaultRepositoryComposition::unavailable(),
    ))
}

/// Wires the application with persistent public-profile metadata storage.
#[must_use]
pub fn compose() -> ApplicationServices {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let security = {
        let clock = Arc::new(SystemClock);
        let random = Arc::new(OsRandom);
        Arc::new(MobileWalletSecurity::native(clock, random))
    };
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let security = Arc::new(UnavailableWalletSecurity);
    compose_with_identity_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        Arc::new(unavailable_midnight_wallet()),
        IdentityAdapters {
            did_repository: Arc::new(UnavailableDidRecordRepository),
            did_resolver: Arc::new(UnavailableDidResolver),
            did_lifecycle: Arc::new(UnavailableDidLifecycle),
            did_jubjub_challenge_signing: Arc::new(UnavailableDidLifecycle),
            credential_repository: Arc::new(UnavailableCredentialRepository),
            credential_inbox: Arc::new(UnavailableCredentialInbox),
            credential_verifier: Arc::new(UnavailableCredentialVerifier),
            credential_disclosure: Arc::new(UnavailableCredentialDisclosure),
            credential_issuance: CredentialIssuanceComposition::Unavailable,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Unavailable,
            credential_presentation: CredentialPresentationComposition::Unavailable,
            portal_test_ingress: false,
        },
        PassportVaultRepositoryComposition::unavailable(),
    )
}

/// Wires the complete standalone simulation through production mobile custody.
///
/// This opt-in harness exists so iOS/Android can exercise every wallet and SSI
/// flow against the same device-bound security adapter selected by normal
/// mobile composition. It never enables development custody and does not turn
/// simulated Midnight settlement into a production claim.
#[cfg(any(target_os = "ios", target_os = "android"))]
#[must_use]
pub fn compose_mobile_native_standalone() -> ApplicationServices {
    compose_mobile_native_standalone_with_presentation(
        CredentialPresentationComposition::Standalone,
    )
}

/// Wires the explicit standalone native-custody mobile harness to the
/// authenticated embedded Compact runtime through the foreground-only worker.
/// Normal production and ordinary standalone mobile composition do not call
/// this constructor.
#[cfg(all(
    feature = "mobile-compact-artifacts",
    any(target_os = "ios", target_os = "android")
))]
pub fn compose_mobile_native_standalone_with_compact_presentation()
-> Result<ApplicationServices, CompactPresentationRuntimeError> {
    let runtime =
        Arc::new(oxid_adapter_vc_midnight::load_embedded_mobile_compact_presentation_runtime()?);
    Ok(compose_mobile_native_standalone_with_presentation(
        CredentialPresentationComposition::StandaloneMobileZk(runtime),
    ))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn compose_mobile_native_standalone_with_presentation(
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(MobileWalletSecurity::native(
        Arc::clone(&clock),
        Arc::clone(&random),
    ));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
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
        )
        .with_profile_association_repository(profiles.clone());
    let services = compose_with_adapters_and_presentation(
        profiles,
        security,
        Arc::new(midnight),
        credential_presentation,
    );
    with_simulated_passport_vault_calls(services)
}

/// Runs the explicit Android smoke probe for JNI exception recovery.
#[cfg(all(target_os = "android", feature = "android-jni-exception-recovery-test"))]
pub fn verify_android_jni_exception_recovery()
-> Result<(), oxid_adapter_mobile_native::NativeBridgeError> {
    oxid_adapter_mobile_native::verify_android_jni_exception_recovery()
}

/// Authenticates the immutable Compact presentation package selected by an
/// explicit mobile conformance build without changing composition by itself.
///
/// Callers that need proof execution must select
/// [`compose_mobile_native_standalone_with_compact_presentation`].
#[cfg(all(
    feature = "mobile-compact-artifacts",
    any(target_os = "ios", target_os = "android")
))]
pub fn authenticate_embedded_mobile_compact_presentation_artifacts()
-> Result<[u8; 32], CompactPresentationRuntimeError> {
    oxid_adapter_vc_midnight::load_embedded_mobile_compact_presentation_runtime()
        .map(|runtime| runtime.identity())
}

/// Wires persistent public profiles with an explicit process-local custody
/// adapter for the standalone development harness.
#[must_use]
pub fn compose_headless() -> ApplicationServices {
    compose_headless_with_presentation(CredentialPresentationComposition::Standalone)
}

fn compose_headless_with_presentation(
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    compose_headless_with_credential_profile(
        credential_presentation,
        HeadlessCredentialProfile::Standalone,
        None,
    )
}

fn compose_headless_with_credential_profile(
    credential_presentation: CredentialPresentationComposition,
    credential_profile: HeadlessCredentialProfile,
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(
            all(not(target_os = "ios"), not(target_os = "android")),
            all(
                feature = "mobile-portal",
                any(target_os = "ios", target_os = "android")
            )
        )
    ))]
    portal: Option<PortalIdentityConfiguration>,
    #[cfg(not(all(
        not(target_arch = "wasm32"),
        any(
            all(not(target_os = "ios"), not(target_os = "android")),
            all(
                feature = "mobile-portal",
                any(target_os = "ios", target_os = "android")
            )
        )
    )))]
    _portal: Option<()>,
) -> ApplicationServices {
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
        )
        .with_profile_association_repository(profiles.clone());
    #[cfg(target_arch = "wasm32")]
    let midnight = Arc::new(
        protected_simulated_midnight_wallet(Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    #[cfg(not(target_arch = "wasm32"))]
    let midnight = Arc::new(midnight);
    let services = compose_with_adapters_and_credential_profile(
        profiles,
        security,
        midnight,
        credential_presentation,
        credential_profile,
        #[cfg(all(
            not(target_arch = "wasm32"),
            any(
                all(not(target_os = "ios"), not(target_os = "android")),
                all(
                    feature = "mobile-portal",
                    any(target_os = "ios", target_os = "android")
                )
            )
        ))]
        portal,
        #[cfg(not(all(
            not(target_arch = "wasm32"),
            any(
                all(not(target_os = "ios"), not(target_os = "android")),
                all(
                    feature = "mobile-portal",
                    any(target_os = "ios", target_os = "android")
                )
            )
        )))]
        _portal,
    );
    #[cfg(not(target_arch = "wasm32"))]
    {
        with_simulated_passport_vault_calls(services)
    }
    #[cfg(target_arch = "wasm32")]
    {
        services
    }
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
/// Environment variable holding the untrusted Passport Vault deployment-height hint.
#[cfg(not(target_arch = "wasm32"))]
pub const PASSPORT_VAULT_DEPLOYMENT_HEIGHT_ENV: &str = "OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT";
/// Environment variable holding the immutable packaged Passport Vault call composer.
#[cfg(not(target_arch = "wasm32"))]
pub const PASSPORT_VAULT_COMPOSER_ENV: &str = "OXID_PASSPORT_VAULT_COMPOSER";
/// Environment variable holding the immutable Compact presentation artifact root.
#[cfg(not(target_arch = "wasm32"))]
pub const PRESENTATION_COMPACT_ARTIFACTS_DIR_ENV: &str = "OXID_PRESENTATION_ARTIFACTS_DIR";
/// Environment variable holding the app-private public DID record file.
pub const DID_STORE_PATH_ENV: &str = "OXID_DID_STORE_PATH";
/// Environment variable holding the app-private encrypted credential file.
pub const CREDENTIAL_STORE_PATH_ENV: &str = "OXID_CREDENTIAL_STORE_PATH";
/// Environment variable holding the development-only credential wrapping key.
pub const CREDENTIAL_KEY_PATH_ENV: &str = "OXID_CREDENTIAL_KEY_PATH";
/// Environment variable holding the absolute authenticated Portal deployment manifest path.
#[cfg(not(target_arch = "wasm32"))]
pub const OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH_ENV: &str =
    "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH";
/// Environment variable holding the expected SHA-256 of the exact Portal deployment manifest.
#[cfg(not(target_arch = "wasm32"))]
pub const OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256_ENV: &str =
    "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256";
/// Environment variable holding the owner-private standalone Passport Vault file.
#[cfg(not(target_arch = "wasm32"))]
pub const PASSPORT_VAULT_STORE_PATH_ENV: &str = "OXID_PASSPORT_VAULT_STORE_PATH";

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
    InvalidPassportVaultDeploymentHeight,
    InvalidPassportVaultHistoryConfiguration(AuthenticatedPassportVaultStateConfigError),
    InvalidPassportVaultComposerConfiguration(PassportVaultCallComposerConfigError),
    InvalidPassportVaultStoreConfiguration(PassportVaultStoreConfigError),
    PassportVaultHistoryRequiresStandalone,
    InvalidCompactPresentationRuntime(CompactPresentationRuntimeError),
    IncompleteCredentialStoreConfiguration,
    IncompletePortalConfiguration,
    PortalConfigurationUnavailable,
    PortalRequiresStandaloneSimulation,
    InvalidPortalConfiguration,
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
            Self::InvalidPassportVaultDeploymentHeight => {
                "Passport Vault deployment height must be a non-zero unsigned integer"
            }
            Self::InvalidPassportVaultHistoryConfiguration(error) => return error.fmt(formatter),
            Self::InvalidPassportVaultComposerConfiguration(error) => return error.fmt(formatter),
            Self::InvalidPassportVaultStoreConfiguration(error) => return error.fmt(formatter),
            Self::PassportVaultHistoryRequiresStandalone => {
                "Passport Vault canonical replay requires the complete standalone Midnight routes"
            }
            Self::InvalidCompactPresentationRuntime(error) => return error.fmt(formatter),
            Self::IncompleteCredentialStoreConfiguration => {
                "credential store and key paths must be configured together"
            }
            Self::IncompletePortalConfiguration => {
                "Portal manifest path and digest must be configured together"
            }
            Self::PortalConfigurationUnavailable => {
                "Portal issuance is available only to native desktop headless development"
            }
            Self::PortalRequiresStandaloneSimulation => {
                "Portal issuance cannot be combined with live Midnight or alternate resolver configuration"
            }
            Self::InvalidPortalConfiguration => "invalid Portal deployment configuration",
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
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if std::env::var_os(OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH_ENV).is_some()
        || std::env::var_os(OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256_ENV).is_some()
    {
        return Err(HeadlessCompositionError::PortalConfigurationUnavailable);
    }
    #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
    let portal = parse_optional_portal_configuration()?;
    let credential_presentation =
        read_optional_environment(PRESENTATION_COMPACT_ARTIFACTS_DIR_ENV)?
            .map(|root| {
                CompactPresentationArtifactsConfig::new(root)
                    .and_then(|config| NativeCompactPresentationRuntime::load(&config))
                    .map(Arc::new)
            })
            .transpose()
            .map_err(HeadlessCompositionError::InvalidCompactPresentationRuntime)?
            .map_or(CredentialPresentationComposition::Standalone, |runtime| {
                CredentialPresentationComposition::StandaloneZk(runtime)
            });
    let credential_paths = (
        read_optional_environment(CREDENTIAL_STORE_PATH_ENV)?,
        read_optional_environment(CREDENTIAL_KEY_PATH_ENV)?,
    );
    if matches!(credential_paths, (Some(_), None) | (None, Some(_))) {
        return Err(HeadlessCompositionError::IncompleteCredentialStoreConfiguration);
    }
    read_optional_environment(PASSPORT_VAULT_STORE_PATH_ENV)?
        .map(PassportVaultStoreConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidPassportVaultStoreConfiguration)?;
    let midnight_did_resolver = read_optional_environment(MIDNIGHT_DID_RESOLVER_URL_ENV)?
        .map(HttpDidResolverConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightDidResolverConfiguration)?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let _ = &midnight_did_resolver;
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
    let passport_vault_deployment_height = parse_optional_passport_vault_deployment_height(
        read_optional_environment(PASSPORT_VAULT_DEPLOYMENT_HEIGHT_ENV)?,
    )?;
    let passport_vault_composer = read_optional_environment(PASSPORT_VAULT_COMPOSER_ENV)?;
    let midnight_config = parse_optional_midnight_config(values)?;
    #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
    if portal.is_some()
        && (midnight_config.is_some()
            || midnight_did_resolver.is_some()
            || checkpoints.is_some()
            || dust_checkpoints.is_some()
            || shielded_checkpoints.is_some()
            || submission_journal.is_some()
            || passport_vault_deployment_height.is_some()
            || passport_vault_composer.is_some())
    {
        return Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation);
    }
    if passport_vault_deployment_height.is_some()
        && !matches!(
            &midnight_config,
            Some(HeadlessMidnightConfig::Standalone(_))
        )
    {
        return Err(HeadlessCompositionError::PassportVaultHistoryRequiresStandalone);
    }
    match midnight_config {
        Some(HeadlessMidnightConfig::Indexer(config))
            if dust_checkpoints.is_none() && submission_journal.is_none() =>
        {
            Ok(
                compose_headless_live_with_checkpoint_options_and_presentation(
                    config,
                    checkpoints,
                    shielded_checkpoints,
                    credential_presentation,
                ),
            )
        }
        Some(HeadlessMidnightConfig::Standalone(config)) => {
            let passport_vault_source = passport_vault_deployment_height
                .map(|height| {
                    AuthenticatedPassportVaultStateSource::new_with_indexer(
                        config.indexer_http_url(),
                        config.node_websocket_url(),
                        height,
                    )
                    .map(Arc::new)
                })
                .transpose()
                .map_err(HeadlessCompositionError::InvalidPassportVaultHistoryConfiguration)?;
            let services = compose_headless_standalone_with_checkpoint_options_and_presentation(
                config,
                checkpoints,
                dust_checkpoints,
                shielded_checkpoints,
                submission_journal,
                credential_presentation,
            );
            let Some(source) = passport_vault_source else {
                return Ok(services);
            };
            let state_source: Arc<dyn PassportVaultContractStateSourcePort> = source.clone();
            let services = with_passport_vault_state_source(services, Some(state_source.clone()));
            let Some(composer) = passport_vault_composer else {
                return Ok(services);
            };
            let chain_source: Arc<dyn PassportVaultCallChainContextSource> = source;
            with_native_passport_vault_calls(services, state_source, chain_source, composer)
                .map_err(HeadlessCompositionError::InvalidPassportVaultComposerConfiguration)
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
        None => {
            #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
            if let Some(portal) = portal {
                return Ok(compose_headless_with_credential_profile(
                    credential_presentation,
                    HeadlessCredentialProfile::Portal,
                    Some(portal),
                ));
            }
            Ok(submission_journal.map_or_else(
                || compose_headless_with_presentation(credential_presentation.clone()),
                |journal| {
                    compose_headless_with_submission_journal_and_presentation(
                        journal,
                        credential_presentation.clone(),
                    )
                },
            ))
        }
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
    compose_headless_live_with_checkpoint_options_and_presentation(
        config,
        account_checkpoints,
        shielded_checkpoints,
        CredentialPresentationComposition::Standalone,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn compose_headless_live_with_checkpoint_options_and_presentation(
    config: MidnightIndexerConfig,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_live_midnight_wallet_with_checkpoint_options(
            config,
            account_checkpoints,
            shielded_checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    compose_with_adapters_and_presentation(profiles, security, midnight, credential_presentation)
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
    compose_headless_standalone_with_checkpoint_options_and_presentation(
        config,
        account_checkpoints,
        dust_checkpoints,
        shielded_checkpoints,
        submission_journal,
        CredentialPresentationComposition::Standalone,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn compose_headless_standalone_with_checkpoint_options_and_presentation(
    config: MidnightStandaloneConfig,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    dust_checkpoints: Option<MidnightDustCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    submission_journal: Option<MidnightSubmissionJournalConfig>,
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_checkpoint_options(
            config,
            account_checkpoints,
            dust_checkpoints,
            shielded_checkpoints,
            submission_journal,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters_and_presentation(
            profiles,
            security,
            midnight,
            credential_presentation,
        ),
        passport_vault_state_source,
    )
}

/// Wires deterministic simulation to an explicit durable public submission journal.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_with_submission_journal(
    journal: MidnightSubmissionJournalConfig,
) -> ApplicationServices {
    compose_headless_with_submission_journal_and_presentation(
        journal,
        CredentialPresentationComposition::Standalone,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn compose_headless_with_submission_journal_and_presentation(
    journal: MidnightSubmissionJournalConfig,
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_simulated_midnight_wallet_with_submission_journal(
            journal,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_simulated_passport_vault_calls(compose_with_adapters_and_presentation(
        profiles,
        security,
        midnight,
        credential_presentation,
    ))
}

/// Wires persistent public profiles and development custody to an explicitly
/// configured live standalone indexer. Normal mobile composition never calls it.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_live(config: MidnightIndexerConfig) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_live_midnight_wallet(config, Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    compose_with_adapters(profiles, security, midnight)
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
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_live_midnight_wallet_with_checkpoints(
            config,
            checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    compose_with_adapters(profiles, security, midnight)
}

/// Wires development custody to the complete, explicitly configured standalone stack.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone(config: MidnightStandaloneConfig) -> ApplicationServices {
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet(config, Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters(profiles, security, midnight),
        passport_vault_state_source,
    )
}

/// Wires the mobile development harness to an explicitly build-selected
/// standalone stack without making routes part of the network catalog.
///
/// The app crate exposes this constructor only behind its opt-in local or
/// tailnet live-stack route profile. Normal and native-custody mobile
/// composition never call it.
#[cfg(not(target_arch = "wasm32"))]
pub fn compose_mobile_development_standalone_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    let config = mobile_standalone_config_from_routes(
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
    )?;
    Ok(compose_headless_standalone(config))
}

/// Wires the exact manifest-authenticated Portal identity profile into the
/// explicit standalone-local mobile development composition.
///
/// Routes and deployment authority are build inputs owned by `oxid-app`'s
/// `standalone-portal` profile. No runtime environment, production, tailnet,
/// native-custody, or WebAssembly composition calls this constructor.
#[cfg(all(
    feature = "mobile-portal",
    any(target_os = "ios", target_os = "android"),
    not(target_arch = "wasm32")
))]
pub fn compose_mobile_development_portal_standalone_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
    deployment_manifest: &[u8],
    deployment_manifest_sha256: &str,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    let config = mobile_standalone_config_from_routes(
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
    )?;
    let portal =
        PortalIdentityConfiguration::from_bytes(deployment_manifest, deployment_manifest_sha256)
            .map_err(|_| HeadlessCompositionError::InvalidPortalConfiguration)?;
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet(config, Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    let services = compose_with_adapters_and_credential_profile(
        profiles,
        security,
        midnight,
        CredentialPresentationComposition::Standalone,
        HeadlessCredentialProfile::Portal,
        Some(portal),
    );
    Ok(with_passport_vault_state_source(
        services,
        passport_vault_state_source,
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn mobile_standalone_config_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
) -> Result<MidnightStandaloneConfig, HeadlessCompositionError> {
    let placeholder = oxid_adapter_midnight::standalone_configuration_placeholder_address()
        .map_err(|_| {
            HeadlessCompositionError::InvalidMidnightStandaloneConfiguration(
                MidnightStandaloneConfigError::Indexer(MidnightIndexerConfigError::InvalidAddress),
            )
        })?;
    MidnightStandaloneConfig::new(
        "undeployed",
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
        placeholder.value(),
    )
    .map_err(HeadlessCompositionError::InvalidMidnightStandaloneConfiguration)
}

/// Wires the complete standalone stack with durable public account checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_checkpoints(
    config: MidnightStandaloneConfig,
    checkpoints: MidnightAccountCheckpointConfig,
) -> ApplicationServices {
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_checkpoints(
            config,
            checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters(profiles, security, midnight),
        passport_vault_state_source,
    )
}

/// Wires the complete standalone stack with private key-scoped DUST checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_dust_checkpoints(
    config: MidnightStandaloneConfig,
    dust_checkpoints: MidnightDustCheckpointConfig,
) -> ApplicationServices {
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_dust_checkpoints(
            config,
            dust_checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters(profiles, security, midnight),
        passport_vault_state_source,
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
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_all_checkpoints(
            config,
            account_checkpoints,
            dust_checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters(profiles, security, midnight),
        passport_vault_state_source,
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
fn parse_optional_portal_configuration()
-> Result<Option<PortalIdentityConfiguration>, HeadlessCompositionError> {
    let values = (
        read_optional_environment(OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH_ENV)?,
        read_optional_environment(OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256_ENV)?,
    );
    match values {
        (None, None) => Ok(None),
        (Some(path), Some(digest)) => PortalIdentityConfiguration::from_file(&path, &digest)
            .map(Some)
            .map_err(|_| HeadlessCompositionError::InvalidPortalConfiguration),
        _ => Err(HeadlessCompositionError::IncompletePortalConfiguration),
    }
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

#[cfg(not(target_arch = "wasm32"))]
fn parse_optional_passport_vault_deployment_height(
    value: Option<String>,
) -> Result<Option<u64>, HeadlessCompositionError> {
    value
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|height| *height > 0)
                .ok_or(HeadlessCompositionError::InvalidPassportVaultDeploymentHeight)
        })
        .transpose()
}

/// Wires deterministic process-local services for tests and development tools.
#[must_use]
pub fn compose_in_memory() -> ApplicationServices {
    compose_in_memory_with_presentation(CredentialPresentationComposition::Standalone)
}

/// Wires deterministic process-local services to one authenticated Compact
/// presentation artifact set. This is the standalone end-to-end proof harness;
/// normal production and mobile composition remain fail-closed.
#[cfg(not(target_arch = "wasm32"))]
pub fn compose_in_memory_with_compact_presentation_artifacts(
    root: impl Into<std::path::PathBuf>,
) -> Result<ApplicationServices, CompactPresentationRuntimeError> {
    let config = CompactPresentationArtifactsConfig::new(root)?;
    let runtime = NativeCompactPresentationRuntime::load(&config)?;
    Ok(compose_in_memory_with_presentation(
        CredentialPresentationComposition::StandaloneZk(Arc::new(runtime)),
    ))
}

fn compose_in_memory_with_presentation(
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let midnight = Arc::new(
        protected_simulated_midnight_wallet(Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    let key_operations: Arc<dyn WalletKeyOperationPort> = security.clone();
    let challenge_signing: Arc<dyn WalletJubjubChallengeSigningPort> = security.clone();
    let did_lifecycle = Arc::new(StandaloneDidLifecycle::with_jubjub_challenge_signing(
        key_operations,
        challenge_signing,
    ));
    let did_lifecycle_port: Arc<dyn DidLifecyclePort> = did_lifecycle.clone();
    let did_jubjub_challenge_signing: Arc<dyn DidJubjubChallengeSigningPort> = did_lifecycle;
    let services = compose_with_identity_adapters(
        profiles,
        security,
        midnight,
        IdentityAdapters {
            did_repository: Arc::new(InMemoryDidRecordRepository::new()),
            did_resolver: Arc::new(StandaloneDidResolver),
            did_lifecycle: did_lifecycle_port,
            did_jubjub_challenge_signing,
            credential_repository: Arc::new(InMemoryCredentialRepository::new()),
            credential_inbox: Arc::new(StandaloneCredentialInbox),
            credential_verifier: Arc::new(MidnightCredentialVerifier::with_compact_policy(
                Arc::new(StandaloneDidResolver),
                Arc::new(StandaloneDidResolver),
                clock.clone(),
                standalone_digital_passport_issuer_trust_anchor(),
            )),
            credential_disclosure: Arc::new(DigitalPassportDisclosureAdapter),
            credential_issuance: CredentialIssuanceComposition::Standalone,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Standalone,
            credential_presentation,
            portal_test_ingress: false,
        },
        PassportVaultRepositoryComposition::process_local(),
    );
    #[cfg(not(target_arch = "wasm32"))]
    {
        with_simulated_passport_vault_calls(services)
    }
    #[cfg(target_arch = "wasm32")]
    {
        services
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn node_anchored_passport_vault_state_source(
    config: &MidnightStandaloneConfig,
) -> Option<Arc<dyn PassportVaultContractStateSourcePort>> {
    NodeAnchoredPassportVaultStateSource::new(
        config.indexer_http_url(),
        config.node_websocket_url(),
    )
    .ok()
    .map(|source| Arc::new(source) as Arc<dyn PassportVaultContractStateSourcePort>)
}

#[cfg(not(target_arch = "wasm32"))]
fn with_passport_vault_state_source(
    mut services: ApplicationServices,
    source: Option<Arc<dyn PassportVaultContractStateSourcePort>>,
) -> ApplicationServices {
    if let Some(source) = source {
        services.read_passport_vault_contract_state =
            Arc::new(PassportVaultContractStateService::with_source(
                Arc::new(NativePassportVaultContractStateDecoder),
                Arc::clone(&source),
            ));
        let calls = Arc::new(PassportVaultContractCallService::new(
            source,
            Arc::new(UnavailablePassportVaultContractCall),
            Arc::new(SystemClock),
            Arc::new(OsRandom),
        ));
        services.prepare_passport_vault_call = calls.clone();
        services.authorize_passport_vault_call = calls.clone();
        services.submit_passport_vault_call = calls.clone();
        services.get_passport_vault_call = calls.clone();
        services.get_passport_vault_call_submission_status = calls.clone();
        services.cancel_passport_vault_call_submission = calls.clone();
        services.list_passport_vault_call_submissions = calls.clone();
        services.reconcile_passport_vault_call_submission = calls;
        services.passport_vault_call_mode = "native_pending";
    }
    services
}

#[cfg(not(target_arch = "wasm32"))]
struct ComposedPassportVaultCallContextSource {
    wallet: Arc<dyn MidnightPublicCallContextSource>,
    chain: Arc<dyn PassportVaultCallChainContextSource>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PassportVaultCallCompositionContextSource for ComposedPassportVaultCallContextSource {
    fn context(
        &self,
        profile_id: &str,
        contract_state: &PassportVaultContractStateSnapshot,
    ) -> Result<PassportVaultCallCompositionContext, PassportVaultCallPortError> {
        let wallet = self
            .wallet
            .public_call_context(profile_id)
            .map_err(map_wallet_context_error)?;
        let chain = self.chain.chain_context(contract_state)?;
        PassportVaultCallCompositionContext::new(
            wallet.network_id().as_str(),
            chain.zswap_chain_state().to_vec(),
            chain.ledger_parameters().to_vec(),
            wallet.coin_public_key(),
            wallet.encryption_public_key(),
            wallet.unshielded_recipient(),
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
const fn map_wallet_context_error(
    error: oxid_wallet_application::WalletAccountPortError,
) -> PassportVaultCallPortError {
    match error {
        oxid_wallet_application::WalletAccountPortError::ProtectionNotInitialized => {
            PassportVaultCallPortError::ProtectionNotInitialized
        }
        oxid_wallet_application::WalletAccountPortError::ProtectionLocked => {
            PassportVaultCallPortError::ProtectionLocked
        }
        oxid_wallet_application::WalletAccountPortError::NotFound => {
            PassportVaultCallPortError::AccountNotDerived
        }
        oxid_wallet_application::WalletAccountPortError::UnsupportedNetwork => {
            PassportVaultCallPortError::UnsupportedNetwork
        }
        oxid_wallet_application::WalletAccountPortError::Unavailable => {
            PassportVaultCallPortError::Unavailable
        }
        oxid_wallet_application::WalletAccountPortError::InvalidData => {
            PassportVaultCallPortError::InvalidData
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct ComposedPassportVaultCallFunding {
    midnight: Arc<dyn MidnightContractCallFundingPort>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PassportVaultCallFundingPort for ComposedPassportVaultCallFunding {
    fn fund(
        &self,
        request: PassportVaultCallFundingRequest,
    ) -> Result<FundedPassportVaultCall, PassportVaultCallPortError> {
        let (profile_id, network_id, expires_at_seconds, requires_night_funding, transaction) =
            request.into_parts();
        let funded = self
            .midnight
            .fund_contract_call(MidnightContractCallFundingRequest::new(
                profile_id,
                network_id,
                expires_at_seconds,
                requires_night_funding,
                transaction,
            ))
            .map_err(map_wallet_transaction_error)?;
        let funded_night_atomic_units = funded.funded_night_atomic_units();
        let funding_input_count = funded.funding_input_count();
        Ok(FundedPassportVaultCall::new(
            funded.into_transaction(),
            funded_night_atomic_units,
            funding_input_count,
        ))
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct ComposedPassportVaultCallCompletion {
    midnight: Arc<dyn MidnightContractCallSubmissionPort>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PassportVaultCallCompletionPort for ComposedPassportVaultCallCompletion {
    fn complete(
        &self,
        request: PassportVaultCallCompletionRequest,
    ) -> Result<PassportVaultCallInclusion, PassportVaultCallPortError> {
        let (
            profile_id,
            network_id,
            draft_id,
            planning_fingerprint,
            expires_at,
            updated_at,
            transaction,
        ) = request.into_parts();
        let outcome = self
            .midnight
            .complete_contract_call(MidnightContractCallSubmissionRequest::new(
                profile_id,
                network_id,
                draft_id,
                planning_fingerprint,
                expires_at,
                updated_at,
                transaction,
            ))
            .map_err(map_wallet_transaction_error)?;
        Ok(PassportVaultCallInclusion {
            transaction_hash_hex: hex::encode(outcome.transaction_hash),
            block_hash_hex: hex::encode(outcome.block_hash),
            block_height: outcome.block_height,
            fee_atomic_units: outcome.fee_specks,
            mode: midnight_submission_mode(outcome.mode).to_owned(),
        })
    }

    fn status(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        self.midnight
            .contract_call_submission_status(profile_id, draft_id)
            .map_err(map_wallet_transaction_error)
            .and_then(map_midnight_contract_call_status)
    }

    fn cancel(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        self.midnight
            .cancel_contract_call_submission(profile_id, draft_id)
            .map_err(map_wallet_transaction_error)
            .and_then(map_midnight_contract_call_status)
    }

    fn history(
        &self,
        profile_id: &str,
    ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError> {
        self.midnight
            .contract_call_submission_history(profile_id)
            .map_err(map_wallet_transaction_error)?
            .into_iter()
            .map(map_midnight_contract_call_status)
            .collect()
    }

    fn reconcile(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        self.midnight
            .reconcile_contract_call_submission(profile_id, draft_id)
            .map_err(map_wallet_transaction_error)
            .and_then(map_midnight_contract_call_status)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn map_midnight_contract_call_status(
    status: MidnightContractCallSubmissionStatus,
) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
    let draft_id = PassportVaultCallDraftId::parse(status.draft_id)
        .map_err(|_| PassportVaultCallPortError::InvalidData)?;
    let state = match status.state {
        MidnightContractCallSubmissionState::Running => PassportVaultCallSubmissionState::Running,
        MidnightContractCallSubmissionState::CancellationRequested => {
            PassportVaultCallSubmissionState::CancellationRequested
        }
        MidnightContractCallSubmissionState::Broadcasting => {
            PassportVaultCallSubmissionState::Broadcasting
        }
        MidnightContractCallSubmissionState::Included => PassportVaultCallSubmissionState::Included,
        MidnightContractCallSubmissionState::Rejected => PassportVaultCallSubmissionState::Rejected,
        MidnightContractCallSubmissionState::Expired => PassportVaultCallSubmissionState::Expired,
        MidnightContractCallSubmissionState::OutcomeUnknown => {
            PassportVaultCallSubmissionState::OutcomeUnknown
        }
    };
    Ok(PassportVaultCallSubmissionStatus {
        draft_id,
        state,
        transaction_hash_hex: status.transaction_hash.map(hex::encode),
        block_hash_hex: status.block_hash.map(hex::encode),
        block_height: status.block_height,
        fee_atomic_units: status.fee_specks,
        mode: status.mode.map(midnight_submission_mode).map(str::to_owned),
    })
}

#[cfg(not(target_arch = "wasm32"))]
const fn midnight_submission_mode(mode: MidnightContractCallSubmissionMode) -> &'static str {
    match mode {
        MidnightContractCallSubmissionMode::Simulated => "simulated",
        MidnightContractCallSubmissionMode::Live => "live",
    }
}

#[cfg(not(target_arch = "wasm32"))]
const fn map_wallet_transaction_error(
    error: oxid_wallet_application::WalletTransactionPortError,
) -> PassportVaultCallPortError {
    use oxid_wallet_application::WalletTransactionPortError as WalletError;

    match error {
        WalletError::Unavailable => PassportVaultCallPortError::Unavailable,
        WalletError::ProtectionNotInitialized => {
            PassportVaultCallPortError::ProtectionNotInitialized
        }
        WalletError::ProtectionLocked => PassportVaultCallPortError::ProtectionLocked,
        WalletError::AccountNotDerived => PassportVaultCallPortError::AccountNotDerived,
        WalletError::AccountNotSynchronized => PassportVaultCallPortError::AccountNotSynchronized,
        WalletError::ShieldedStateNotCurrent => PassportVaultCallPortError::InvalidChainState,
        WalletError::UnsupportedNetwork => PassportVaultCallPortError::UnsupportedNetwork,
        WalletError::InvalidRecipient | WalletError::RecipientNetworkMismatch => {
            PassportVaultCallPortError::InvalidData
        }
        WalletError::InsufficientFunds => PassportVaultCallPortError::InsufficientFunds,
        WalletError::DraftNotFound => PassportVaultCallPortError::DraftNotFound,
        WalletError::DraftExpired => PassportVaultCallPortError::DraftExpired,
        WalletError::DraftConflict => PassportVaultCallPortError::DraftConflict,
        WalletError::SubmissionInProgress => PassportVaultCallPortError::SubmissionInProgress,
        WalletError::SubmissionNotInProgress => PassportVaultCallPortError::SubmissionNotInProgress,
        WalletError::SubmissionCancelled => PassportVaultCallPortError::SubmissionCancelled,
        WalletError::SubmissionCancellationUnsafe => {
            PassportVaultCallPortError::SubmissionCancellationUnsafe
        }
        WalletError::AuthorizationChallengeMismatch => {
            PassportVaultCallPortError::AuthorizationChallengeMismatch
        }
        WalletError::InsufficientDust => PassportVaultCallPortError::InsufficientDust,
        WalletError::InvalidChainState => PassportVaultCallPortError::InvalidChainState,
        WalletError::ProvingFailed => PassportVaultCallPortError::ProvingFailed,
        WalletError::SubmissionRejected => PassportVaultCallPortError::SubmissionRejected,
        WalletError::SubmissionOutcomeUnknown => {
            PassportVaultCallPortError::SubmissionOutcomeUnknown
        }
        WalletError::Timeout => PassportVaultCallPortError::Timeout,
        WalletError::InvalidData => PassportVaultCallPortError::InvalidData,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn with_native_passport_vault_calls(
    mut services: ApplicationServices,
    state_source: Arc<dyn PassportVaultContractStateSourcePort>,
    chain_source: Arc<dyn PassportVaultCallChainContextSource>,
    composer: impl AsRef<std::path::Path>,
) -> Result<ApplicationServices, PassportVaultCallComposerConfigError> {
    let contexts: Arc<dyn PassportVaultCallCompositionContextSource> =
        Arc::new(ComposedPassportVaultCallContextSource {
            wallet: Arc::clone(&services.midnight_public_call_context),
            chain: chain_source,
        });
    let funding: Arc<dyn PassportVaultCallFundingPort> =
        Arc::new(ComposedPassportVaultCallFunding {
            midnight: Arc::clone(&services.midnight_contract_call_funding),
        });
    let completion: Arc<dyn PassportVaultCallCompletionPort> =
        Arc::new(ComposedPassportVaultCallCompletion {
            midnight: Arc::clone(&services.midnight_contract_call_submission),
        });
    let native_calls =
        if let Some(presentations) = services.protected_passport_vault_presentations.clone() {
            NativePassportVaultContractCall::new_with_protected_claims_and_completion(
                composer,
                contexts,
                funding,
                completion,
                presentations,
            )?
        } else {
            NativePassportVaultContractCall::new_with_funding_and_completion(
                composer, contexts, funding, completion,
            )?
        };
    let calls = Arc::new(PassportVaultContractCallService::new(
        state_source,
        Arc::new(native_calls),
        Arc::new(SystemClock),
        Arc::new(OsRandom),
    ));
    services.prepare_passport_vault_call = calls.clone();
    services.authorize_passport_vault_call = calls.clone();
    services.submit_passport_vault_call = calls.clone();
    services.get_passport_vault_call = calls.clone();
    services.get_passport_vault_call_submission_status = calls.clone();
    services.cancel_passport_vault_call_submission = calls.clone();
    services.list_passport_vault_call_submissions = calls.clone();
    services.reconcile_passport_vault_call_submission = calls;
    services.passport_vault_call_mode = "native_settlement";
    Ok(services)
}

#[cfg(not(target_arch = "wasm32"))]
fn with_simulated_passport_vault_calls(mut services: ApplicationServices) -> ApplicationServices {
    let Ok(source) = SimulatedPassportVaultStateSource::new() else {
        return services;
    };
    let source: Arc<dyn PassportVaultContractStateSourcePort> = Arc::new(source);
    services.read_passport_vault_contract_state =
        Arc::new(PassportVaultContractStateService::with_source(
            Arc::new(NativePassportVaultContractStateDecoder),
            Arc::clone(&source),
        ));
    let calls = Arc::new(PassportVaultContractCallService::new_simulated(
        source,
        Arc::new(SimulatedPassportVaultContractCall::new()),
        Arc::new(SystemClock),
        Arc::new(OsRandom),
    ));
    services.prepare_passport_vault_call = calls.clone();
    services.authorize_passport_vault_call = calls.clone();
    services.submit_passport_vault_call = calls.clone();
    services.get_passport_vault_call = calls.clone();
    services.get_passport_vault_call_submission_status = calls.clone();
    services.cancel_passport_vault_call_submission = calls.clone();
    services.list_passport_vault_call_submissions = calls.clone();
    services.reconcile_passport_vault_call_submission = calls;
    services.passport_vault_call_mode = "deterministic_simulation";
    services.passport_vault_call_contract_address_hex =
        Some(SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX);
    services
}

fn compose_with_adapters<R, S, M>(
    repository: Arc<R>,
    security: Arc<S>,
    midnight: Arc<M>,
) -> ApplicationServices
where
    R: WalletProfileRepository
        + WalletProfileAssociationRepository
        + WalletBackupReceiptRepository
        + 'static,
    S: WalletProtectionPort
        + WalletKeyOperationPort
        + WalletJubjubChallengeSigningPort
        + WalletPortableBackupPort
        + PortableCustodyVaultPort
        + 'static,
    M: WalletNetworkPort
        + WalletAccountReadPort
        + WalletAccountDerivationPort
        + WalletDustSyncPort
        + NativeWalletDustRegistrationCapability
        + WalletShieldedSyncPort
        + WalletTransactionPort
        + MidnightPublicCallContextSource
        + MidnightDiagnosticAttachPort
        + NativeMidnightCompositionCapability
        + 'static,
{
    compose_with_adapters_and_presentation(
        repository,
        security,
        midnight,
        CredentialPresentationComposition::Standalone,
    )
}

fn compose_with_adapters_and_presentation<R, S, M>(
    repository: Arc<R>,
    security: Arc<S>,
    midnight: Arc<M>,
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices
where
    R: WalletProfileRepository
        + WalletProfileAssociationRepository
        + WalletBackupReceiptRepository
        + 'static,
    S: WalletProtectionPort
        + WalletKeyOperationPort
        + WalletJubjubChallengeSigningPort
        + WalletPortableBackupPort
        + PortableCustodyVaultPort
        + 'static,
    M: WalletNetworkPort
        + WalletAccountReadPort
        + WalletAccountDerivationPort
        + WalletDustSyncPort
        + NativeWalletDustRegistrationCapability
        + WalletShieldedSyncPort
        + WalletTransactionPort
        + MidnightPublicCallContextSource
        + MidnightDiagnosticAttachPort
        + NativeMidnightCompositionCapability
        + 'static,
{
    compose_with_adapters_and_credential_profile(
        repository,
        security,
        midnight,
        credential_presentation,
        HeadlessCredentialProfile::Standalone,
        None,
    )
}

fn compose_with_adapters_and_credential_profile<R, S, M>(
    repository: Arc<R>,
    security: Arc<S>,
    midnight: Arc<M>,
    credential_presentation: CredentialPresentationComposition,
    credential_profile: HeadlessCredentialProfile,
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(
            all(not(target_os = "ios"), not(target_os = "android")),
            all(
                feature = "mobile-portal",
                any(target_os = "ios", target_os = "android")
            )
        )
    ))]
    portal: Option<PortalIdentityConfiguration>,
    #[cfg(not(all(
        not(target_arch = "wasm32"),
        any(
            all(not(target_os = "ios"), not(target_os = "android")),
            all(
                feature = "mobile-portal",
                any(target_os = "ios", target_os = "android")
            )
        )
    )))]
    _portal: Option<()>,
) -> ApplicationServices
where
    R: WalletProfileRepository
        + WalletProfileAssociationRepository
        + WalletBackupReceiptRepository
        + 'static,
    S: WalletProtectionPort
        + WalletKeyOperationPort
        + WalletJubjubChallengeSigningPort
        + WalletPortableBackupPort
        + PortableCustodyVaultPort
        + 'static,
    M: WalletNetworkPort
        + WalletAccountReadPort
        + WalletAccountDerivationPort
        + WalletDustSyncPort
        + NativeWalletDustRegistrationCapability
        + WalletShieldedSyncPort
        + WalletTransactionPort
        + MidnightPublicCallContextSource
        + MidnightDiagnosticAttachPort
        + NativeMidnightCompositionCapability
        + 'static,
{
    let key_operations: Arc<dyn WalletKeyOperationPort> = security.clone();
    let challenge_signing: Arc<dyn WalletJubjubChallengeSigningPort> = security.clone();
    let did_lifecycle = Arc::new(StandaloneDidLifecycle::with_jubjub_challenge_signing(
        key_operations,
        challenge_signing,
    ));
    let did_lifecycle_port: Arc<dyn DidLifecyclePort> = did_lifecycle.clone();
    let did_jubjub_challenge_signing: Arc<dyn DidJubjubChallengeSigningPort> = did_lifecycle;
    let did_resolver = headless_did_resolver();
    let portal_test_ingress = match &credential_profile {
        HeadlessCredentialProfile::Standalone => false,
        #[cfg(all(
            not(target_arch = "wasm32"),
            any(
                all(not(target_os = "ios"), not(target_os = "android")),
                all(
                    feature = "mobile-portal",
                    any(target_os = "ios", target_os = "android")
                )
            )
        ))]
        HeadlessCredentialProfile::Portal => true,
    };
    let (compact_issuer_resolver, trust_anchor, credential_issuance) = match credential_profile {
        HeadlessCredentialProfile::Standalone => (
            Arc::new(StandaloneDidResolver) as Arc<dyn DidResolutionPort>,
            standalone_digital_passport_issuer_trust_anchor(),
            CredentialIssuanceComposition::Standalone,
        ),
        #[cfg(all(
            not(target_arch = "wasm32"),
            any(
                all(not(target_os = "ios"), not(target_os = "android")),
                all(
                    feature = "mobile-portal",
                    any(target_os = "ios", target_os = "android")
                )
            )
        ))]
        HeadlessCredentialProfile::Portal => {
            let portal = portal.expect("Portal headless profile requires authenticated config");
            (
                portal.issuer_resolver,
                portal.trust_anchor,
                CredentialIssuanceComposition::Portal(Box::new(portal.client_factory)),
            )
        }
    };
    let verifier: Arc<dyn CredentialVerificationPort> =
        Arc::new(MidnightCredentialVerifier::with_compact_policy(
            Arc::clone(&did_resolver),
            compact_issuer_resolver,
            Arc::new(SystemClock),
            trust_anchor,
        ));
    compose_with_identity_adapters(
        repository,
        security,
        midnight,
        IdentityAdapters {
            did_repository: headless_did_repository(),
            did_resolver,
            did_lifecycle: did_lifecycle_port,
            did_jubjub_challenge_signing,
            credential_repository: headless_credential_repository(),
            credential_inbox: Arc::new(StandaloneCredentialInbox),
            credential_verifier: verifier,
            credential_disclosure: Arc::new(DigitalPassportDisclosureAdapter),
            credential_issuance,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Standalone,
            credential_presentation,
            portal_test_ingress,
        },
        headless_passport_vault_repository(),
    )
}

fn compose_with_identity_adapters<R, S, M>(
    repository: Arc<R>,
    security: Arc<S>,
    midnight: Arc<M>,
    identity_adapters: IdentityAdapters,
    passport_vault_repository: PassportVaultRepositoryComposition,
) -> ApplicationServices
where
    R: WalletProfileRepository
        + WalletProfileAssociationRepository
        + WalletBackupReceiptRepository
        + 'static,
    S: WalletProtectionPort
        + WalletKeyOperationPort
        + WalletJubjubChallengeSigningPort
        + WalletPortableBackupPort
        + PortableCustodyVaultPort
        + 'static,
    M: WalletNetworkPort
        + WalletAccountReadPort
        + WalletAccountDerivationPort
        + WalletDustSyncPort
        + NativeWalletDustRegistrationCapability
        + WalletShieldedSyncPort
        + WalletTransactionPort
        + MidnightPublicCallContextSource
        + MidnightDiagnosticAttachPort
        + NativeMidnightCompositionCapability
        + 'static,
{
    let diagnostic_repository = Arc::new(InMemoryDiagnosticStore::default());
    let diagnostic_events: Arc<dyn DiagnosticEventSinkPort> = diagnostic_repository.clone();
    midnight.attach_diagnostic_sink(Arc::clone(&diagnostic_events));
    let diagnostics = Arc::new(DiagnosticsService::new(diagnostic_repository));
    let get_diagnostic_snapshot: Arc<dyn GetDiagnosticSnapshotUseCase> = diagnostics.clone();
    let clear_diagnostics: Arc<dyn ClearDiagnosticsUseCase> = diagnostics;
    let IdentityAdapters {
        did_repository,
        did_resolver,
        did_lifecycle,
        did_jubjub_challenge_signing,
        credential_repository,
        credential_inbox,
        credential_verifier,
        credential_disclosure,
        credential_issuance,
        self_issued_authentication,
        credential_presentation,
        portal_test_ingress,
    } = identity_adapters;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = portal_test_ingress;
    let identity_request_router: Arc<dyn IdentityRequestRouterPort> = if matches!(
        self_issued_authentication,
        SelfIssuedAuthenticationComposition::Standalone
    ) && !matches!(
        &credential_presentation,
        CredentialPresentationComposition::Unavailable
    ) {
        StrictIdentityRequestRouter::with_registered_openid4vp_requests(
            &standalone_siopv2_request(),
            &standalone_openid4vp_request(),
        )
        .map_or_else(
            |_| {
                Arc::new(StrictIdentityRequestRouter::credential_offers_only())
                    as Arc<dyn IdentityRequestRouterPort>
            },
            |router| Arc::new(router) as Arc<dyn IdentityRequestRouterPort>,
        )
    } else {
        Arc::new(StrictIdentityRequestRouter::credential_offers_only())
    };
    let route_identity_request =
        Arc::new(IdentityRequestRoutingService::new(identity_request_router));
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let qr_scanner: Arc<dyn QrScannerPort> = Arc::new(NativeQrScanner);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let qr_scanner: Arc<dyn QrScannerPort> = Arc::new(UnavailableQrScanner);
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let identity_link_ingress: Arc<dyn IdentityLinkIngressPort> = if portal_test_ingress {
        #[cfg(feature = "mobile-portal")]
        {
            Arc::new(NativeIdentityLinkIngress::standalone_portal_test())
        }
        #[cfg(not(feature = "mobile-portal"))]
        unreachable!("Portal ingress requires mobile-portal")
    } else {
        Arc::new(NativeIdentityLinkIngress::default())
    };
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let identity_link_ingress: Arc<dyn IdentityLinkIngressPort> =
        Arc::new(UnavailableIdentityLinkIngress);
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let public_text_exporter: Arc<dyn PublicTextExportPort> = Arc::new(NativePublicTextExporter);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let public_text_exporter: Arc<dyn PublicTextExportPort> =
        Arc::new(UnavailablePublicTextExporter);
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let screen_privacy: Arc<dyn ScreenPrivacyPort> = Arc::new(NativeScreenPrivacy);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let screen_privacy: Arc<dyn ScreenPrivacyPort> = Arc::new(UnavailableScreenPrivacy);
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let portable_wallet_backup_documents: Arc<dyn PortableWalletBackupDocumentPort> =
        Arc::new(NativePortableWalletBackupDocuments);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let portable_wallet_backup_documents: Arc<dyn PortableWalletBackupDocumentPort> =
        Arc::new(UnavailablePortableWalletBackupDocuments);
    let presentation_credential_repository = Arc::clone(&credential_repository);
    let vault_credential_repository = Arc::clone(&credential_repository);
    let standalone_passport_vault = !matches!(
        &credential_issuance,
        CredentialIssuanceComposition::Unavailable
    );
    #[cfg(not(target_arch = "wasm32"))]
    let compact_presentation_proof_available = match &credential_presentation {
        CredentialPresentationComposition::StandaloneZk(_) => true,
        #[cfg(all(
            feature = "mobile-compact-artifacts",
            any(target_os = "ios", target_os = "android")
        ))]
        CredentialPresentationComposition::StandaloneMobileZk(_) => true,
        _ => false,
    };
    #[cfg(target_arch = "wasm32")]
    let compact_presentation_proof_available = false;
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let complete_custody: Arc<dyn PortableCustodyVaultPort> = security.clone();
    let complete_profiles: Arc<dyn WalletProfileRepository> = repository.clone();
    let complete_associations: Arc<dyn WalletProfileAssociationRepository> = repository.clone();
    let complete_random: Arc<dyn oxid_platform_ports::RandomPort> = random.clone();
    let complete_backup_adapter = Arc::new(CompleteWalletBackupAdapter::new(
        complete_custody,
        complete_profiles,
        did_repository.clone(),
        credential_repository.clone(),
        complete_associations,
        complete_random,
        complete_wallet_recovery_journal(),
    ));
    let complete_backup = Arc::new(CompleteWalletBackupService::new(complete_backup_adapter));
    let create_wallet_profile = Arc::new(CreateWalletProfileService::new(
        Arc::clone(&repository),
        Arc::clone(&clock),
        Arc::clone(&random),
    ));
    let list_wallet_profiles = Arc::new(ListWalletProfilesService::new(Arc::clone(&repository)));
    let select_wallet_profile = Arc::new(SelectWalletProfileService::new(Arc::clone(&repository)));
    let get_active_wallet_profile =
        Arc::new(GetActiveWalletProfileService::new(Arc::clone(&repository)));
    let backup_receipts = Arc::new(WalletBackupReceiptService::new(
        repository,
        Arc::clone(&clock),
    ));
    let protection = Arc::new(WalletProtectionService::new(Arc::clone(&security)));
    let portable_backup = Arc::new(WalletPortableBackupService::new(Arc::clone(&security)));
    let keys = Arc::new(WalletKeyService::new(security));
    let midnight_public_call_context: Arc<dyn MidnightPublicCallContextSource> = midnight.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let midnight_contract_call_funding: Arc<dyn MidnightContractCallFundingPort> = midnight.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let midnight_contract_call_submission: Arc<dyn MidnightContractCallSubmissionPort> =
        midnight.clone();
    let networks = Arc::new(WalletNetworkService::new(Arc::clone(&midnight)));
    let account_derivation = Arc::new(WalletAccountDerivationService::new(Arc::clone(&midnight)));
    let accounts = Arc::new(WalletAccountService::new(Arc::clone(&midnight)));
    let dust = Arc::new(WalletDustSyncService::new(Arc::clone(&midnight)));
    let shielded = Arc::new(WalletShieldedSyncService::new(Arc::clone(&midnight)));
    #[cfg(not(target_arch = "wasm32"))]
    let dust_registrations = Arc::new(WalletDustRegistrationService::new(
        Arc::clone(&midnight),
        Arc::clone(&clock),
    ));
    #[cfg(target_arch = "wasm32")]
    let dust_registrations = Arc::new(WalletDustRegistrationService::new(
        Arc::new(UnavailableWalletDustRegistrationPort),
        Arc::clone(&clock),
    ));
    let transactions = Arc::new(WalletTransactionService::new(midnight, Arc::clone(&clock)));
    let identity = Arc::new(DidService::from_ports(
        did_repository,
        did_resolver,
        did_lifecycle,
    ));
    #[cfg(not(target_arch = "wasm32"))]
    let protected_passport_vault_presentations = standalone_passport_vault.then(|| {
        let get_did: Arc<dyn GetDidRecordUseCase> = identity.clone();
        let sign_did: Arc<dyn SignDidPayloadUseCase> = identity.clone();
        let holder_authorization =
            Arc::new(ManagedDidJubjubHolderAuthorization::with_challenge_signing(
                get_did,
                sign_did,
                Arc::clone(&did_jubjub_challenge_signing),
            ));
        let holder_proof: Arc<dyn CompactHolderProofPort> = holder_authorization.clone();
        Arc::new(ProtectedDigitalPassportPresentationSource::new(
            Arc::clone(&vault_credential_repository),
            holder_authorization,
            holder_proof,
        ))
    });
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
                    Arc::new(StandaloneBoundCompactCredentialIssuer::new(clock.clone())),
                )),
                Arc::new(VerifiedCredentialSink::new(importer)),
            )
        }
        #[cfg(all(
            not(target_arch = "wasm32"),
            any(
                all(not(target_os = "ios"), not(target_os = "android")),
                all(
                    feature = "mobile-portal",
                    any(target_os = "ios", target_os = "android")
                )
            )
        ))]
        CredentialIssuanceComposition::Portal(factory) => {
            let get_did: Arc<dyn GetDidRecordUseCase> = identity.clone();
            let sign_did: Arc<dyn SignDidPayloadUseCase> = identity.clone();
            let proof = Arc::new(DidCredentialHolderProof::new(
                Arc::clone(&get_did),
                sign_did,
                clock.clone(),
            ));
            let importer: Arc<dyn ImportVerifiedCredentialUseCase> = credentials.clone();
            (
                Arc::new(factory.build(proof, get_did, Arc::new(PortalPrivateMaterialDecoder))),
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
                let get_did: Arc<dyn GetDidRecordUseCase> = identity.clone();
                let sign_did: Arc<dyn SignDidPayloadUseCase> = identity.clone();
                let holder_authorization =
                    Arc::new(ManagedDidJubjubHolderAuthorization::with_challenge_signing(
                        get_did,
                        sign_did,
                        did_jubjub_challenge_signing,
                    ));
                let holder_proof: Arc<dyn CompactHolderProofPort> = holder_authorization.clone();
                Arc::new(StandaloneOpenId4VpVerifier::new(
                    Arc::new(CredentialDisclosureCandidateSource::new(list, disclosure)),
                    Arc::new(PreflightOnlyCompactPresentationProof::with_holder_proof(
                        presentation_credential_repository,
                        clock.clone(),
                        holder_authorization,
                        holder_proof,
                    )),
                    Arc::new(UnavailablePresentationVerifier),
                    clock.clone(),
                ))
            }
            #[cfg(not(target_arch = "wasm32"))]
            CredentialPresentationComposition::StandaloneZk(runtime) => {
                let list: Arc<dyn ListCredentialsUseCase> = credentials.clone();
                let disclosure: Arc<dyn GetCredentialDisclosureUseCase> = credentials.clone();
                let get_did: Arc<dyn GetDidRecordUseCase> = identity.clone();
                let verifier_get_did = Arc::clone(&get_did);
                let sign_did: Arc<dyn SignDidPayloadUseCase> = identity.clone();
                let holder_authorization =
                    Arc::new(ManagedDidJubjubHolderAuthorization::with_challenge_signing(
                        get_did,
                        sign_did,
                        did_jubjub_challenge_signing,
                    ));
                let holder_proof: Arc<dyn CompactHolderProofPort> = holder_authorization.clone();
                Arc::new(StandaloneOpenId4VpVerifier::new(
                    Arc::new(CredentialDisclosureCandidateSource::new(list, disclosure)),
                    Arc::new(PreflightOnlyCompactPresentationProof::with_runtime(
                        presentation_credential_repository,
                        clock.clone(),
                        holder_authorization,
                        holder_proof,
                        Arc::clone(&runtime),
                    )),
                    Arc::new(NativeCompactPresentationVerifier::new(
                        runtime,
                        clock.clone(),
                        verifier_get_did,
                    )),
                    clock.clone(),
                ))
            }
            #[cfg(all(
                feature = "mobile-compact-artifacts",
                any(target_os = "ios", target_os = "android")
            ))]
            CredentialPresentationComposition::StandaloneMobileZk(runtime) => {
                let list: Arc<dyn ListCredentialsUseCase> = credentials.clone();
                let disclosure: Arc<dyn GetCredentialDisclosureUseCase> = credentials.clone();
                let get_did: Arc<dyn GetDidRecordUseCase> = identity.clone();
                let verifier_get_did = Arc::clone(&get_did);
                let sign_did: Arc<dyn SignDidPayloadUseCase> = identity.clone();
                let holder_authorization =
                    Arc::new(ManagedDidJubjubHolderAuthorization::with_challenge_signing(
                        get_did,
                        sign_did,
                        did_jubjub_challenge_signing,
                    ));
                let holder_proof: Arc<dyn CompactHolderProofPort> = holder_authorization.clone();
                let proof = Arc::new(ForegroundCompactPresentationProofWorker::new(Arc::new(
                    PreflightOnlyCompactPresentationProof::with_runtime(
                        presentation_credential_repository,
                        clock.clone(),
                        holder_authorization,
                        holder_proof,
                        Arc::clone(&runtime),
                    ),
                )));
                let proof_port: Arc<dyn PresentationProofPort> = proof.clone();
                let proof_control: Arc<dyn PresentationProofControlPort> = proof;
                Arc::new(StandaloneOpenId4VpVerifier::with_proof_control(
                    Arc::new(CredentialDisclosureCandidateSource::new(list, disclosure)),
                    proof_port,
                    proof_control,
                    Arc::new(NativeCompactPresentationVerifier::new(
                        runtime,
                        clock.clone(),
                        verifier_get_did,
                    )),
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
    let passport_vault_state_persistence = passport_vault_repository.persistence;
    let passport_vault_credential: Arc<dyn PassportVaultCredentialPort> =
        if standalone_passport_vault {
            Arc::new(StandalonePassportVaultCredential::new(
                vault_credential_repository,
                clock.clone(),
                oxid_adapter_vc_midnight::standalone_digital_passport_issuer_trust_anchor(),
            ))
        } else {
            Arc::new(UnavailablePassportVaultCredential)
        };
    let passport_vault = Arc::new(PassportVaultService::new(
        passport_vault_repository.repository,
        passport_vault_credential,
        random.clone(),
    ));
    #[cfg(not(target_arch = "wasm32"))]
    let passport_vault_contract_state_decoder: Arc<dyn PassportVaultContractStateDecoderPort> =
        Arc::new(NativePassportVaultContractStateDecoder);
    #[cfg(target_arch = "wasm32")]
    let passport_vault_contract_state_decoder: Arc<dyn PassportVaultContractStateDecoderPort> =
        Arc::new(oxid_passport_vault_application::UnavailablePassportVaultContractStateDecoder);
    let passport_vault_contract_state_source: Arc<dyn PassportVaultContractStateSourcePort> =
        Arc::new(UnavailablePassportVaultContractStateSource);
    let passport_vault_contract_state = Arc::new(PassportVaultContractStateService::with_source(
        passport_vault_contract_state_decoder,
        Arc::clone(&passport_vault_contract_state_source),
    ));
    let passport_vault_contract_calls = Arc::new(PassportVaultContractCallService::new(
        passport_vault_contract_state_source,
        Arc::new(UnavailablePassportVaultContractCall),
        clock.clone(),
        random,
    ));

    let get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase> = protection.clone();
    let initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase> = protection.clone();
    let unlock_wallet: Arc<dyn UnlockWalletUseCase> = protection.clone();
    let lock_wallet: Arc<dyn LockWalletUseCase> = protection;
    let export_portable_wallet_backup: Arc<dyn ExportPortableWalletBackupUseCase> =
        portable_backup.clone();
    let recover_portable_wallet_backup: Arc<dyn RecoverPortableWalletBackupUseCase> =
        portable_backup;
    let export_complete_wallet_backup: Arc<dyn ExportCompleteWalletBackupUseCase> =
        complete_backup.clone();
    let recover_complete_wallet_backup: Arc<dyn RecoverCompleteWalletBackupUseCase> =
        complete_backup;
    let get_wallet_backup_receipt: Arc<dyn GetWalletBackupReceiptUseCase> = backup_receipts.clone();
    let record_wallet_backup_receipt: Arc<dyn RecordWalletBackupReceiptUseCase> = backup_receipts;
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
    let prepare_wallet_dust_registration: Arc<dyn PrepareWalletDustRegistrationUseCase> =
        dust_registrations.clone();
    let authorize_wallet_dust_registration: Arc<dyn AuthorizeWalletDustRegistrationUseCase> =
        dust_registrations.clone();
    let submit_wallet_dust_registration: Arc<dyn SubmitWalletDustRegistrationUseCase> =
        dust_registrations.clone();
    let get_wallet_dust_registration: Arc<dyn GetWalletDustRegistrationUseCase> =
        dust_registrations.clone();
    let get_wallet_dust_registration_status: Arc<dyn GetWalletDustRegistrationStatusUseCase> =
        dust_registrations.clone();
    let cancel_wallet_dust_registration_submission: Arc<
        dyn CancelWalletDustRegistrationSubmissionUseCase,
    > = dust_registrations.clone();
    let reconcile_wallet_dust_registration_submission: Arc<
        dyn ReconcileWalletDustRegistrationSubmissionUseCase,
    > = dust_registrations;
    let prepare_shielded_wallet_transfer: Arc<dyn PrepareShieldedWalletTransferUseCase> =
        transactions.clone();
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
    let cancel_credential_presentation: Arc<dyn CancelCredentialPresentationUseCase> =
        credential_presentation.clone();
    let set_credential_presentation_foreground: Arc<
        dyn SetCredentialPresentationForegroundUseCase,
    > = credential_presentation.clone();
    let refuse_credential_presentation: Arc<dyn RefuseCredentialPresentationUseCase> =
        credential_presentation.clone();
    let get_credential_presentation: Arc<dyn GetCredentialPresentationUseCase> =
        credential_presentation.clone();
    let list_credential_presentations: Arc<dyn ListCredentialPresentationsUseCase> =
        credential_presentation;
    let list_passport_vault_locks: Arc<dyn ListPassportVaultLocksUseCase> = passport_vault.clone();
    let decode_passport_vault_contract_state: Arc<dyn DecodePassportVaultContractStateUseCase> =
        passport_vault_contract_state.clone();
    let read_passport_vault_contract_state: Arc<dyn ReadPassportVaultContractStateUseCase> =
        passport_vault_contract_state;
    let create_passport_vault_lock: Arc<dyn CreatePassportVaultLockUseCase> =
        passport_vault.clone();
    let deposit_passport_vault_lock: Arc<dyn DepositPassportVaultLockUseCase> =
        passport_vault.clone();
    let claim_passport_vault_lock: Arc<dyn ClaimPassportVaultLockUseCase> = passport_vault.clone();
    let withdraw_passport_vault_lock: Arc<dyn WithdrawPassportVaultLockUseCase> = passport_vault;
    let prepare_passport_vault_call: Arc<dyn PreparePassportVaultCallUseCase> =
        passport_vault_contract_calls.clone();
    let authorize_passport_vault_call: Arc<dyn AuthorizePassportVaultCallUseCase> =
        passport_vault_contract_calls.clone();
    let submit_passport_vault_call: Arc<dyn SubmitPassportVaultCallUseCase> =
        passport_vault_contract_calls.clone();
    let get_passport_vault_call: Arc<dyn GetPassportVaultCallUseCase> =
        passport_vault_contract_calls.clone();
    let get_passport_vault_call_submission_status: Arc<
        dyn GetPassportVaultCallSubmissionStatusUseCase,
    > = passport_vault_contract_calls.clone();
    let cancel_passport_vault_call_submission: Arc<dyn CancelPassportVaultCallSubmissionUseCase> =
        passport_vault_contract_calls.clone();
    let list_passport_vault_call_submissions: Arc<dyn ListPassportVaultCallSubmissionsUseCase> =
        passport_vault_contract_calls.clone();
    let reconcile_passport_vault_call_submission: Arc<
        dyn ReconcilePassportVaultCallSubmissionUseCase,
    > = passport_vault_contract_calls;

    ApplicationServices {
        diagnostic_events,
        get_diagnostic_snapshot,
        clear_diagnostics,
        qr_scanner,
        identity_link_ingress,
        public_text_exporter,
        screen_privacy,
        portable_wallet_backup_documents,
        route_identity_request,
        midnight_public_call_context,
        #[cfg(not(target_arch = "wasm32"))]
        midnight_contract_call_funding,
        #[cfg(not(target_arch = "wasm32"))]
        midnight_contract_call_submission,
        #[cfg(not(target_arch = "wasm32"))]
        protected_passport_vault_presentations,
        create_wallet_profile,
        list_wallet_profiles,
        select_wallet_profile,
        get_active_wallet_profile,
        get_wallet_backup_receipt,
        record_wallet_backup_receipt,
        get_wallet_security_status,
        initialize_wallet_security,
        unlock_wallet,
        lock_wallet,
        export_portable_wallet_backup,
        recover_portable_wallet_backup,
        export_complete_wallet_backup,
        recover_complete_wallet_backup,
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
        prepare_wallet_dust_registration,
        authorize_wallet_dust_registration,
        submit_wallet_dust_registration,
        get_wallet_dust_registration,
        get_wallet_dust_registration_status,
        cancel_wallet_dust_registration_submission,
        reconcile_wallet_dust_registration_submission,
        prepare_shielded_wallet_transfer,
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
        cancel_credential_presentation,
        set_credential_presentation_foreground,
        refuse_credential_presentation,
        get_credential_presentation,
        list_credential_presentations,
        list_passport_vault_locks,
        decode_passport_vault_contract_state,
        read_passport_vault_contract_state,
        create_passport_vault_lock,
        deposit_passport_vault_lock,
        claim_passport_vault_lock,
        withdraw_passport_vault_lock,
        prepare_passport_vault_call,
        authorize_passport_vault_call,
        submit_passport_vault_call,
        get_passport_vault_call,
        get_passport_vault_call_submission_status,
        cancel_passport_vault_call_submission,
        list_passport_vault_call_submissions,
        reconcile_passport_vault_call_submission,
        passport_vault_call_mode: "unavailable",
        passport_vault_call_contract_address_hex: None,
        passport_vault_state_persistence,
        compact_presentation_proof_available,
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
fn headless_passport_vault_repository() -> PassportVaultRepositoryComposition {
    let path = std::env::var_os(PASSPORT_VAULT_STORE_PATH_ENV)
        .map(std::path::PathBuf::from)
        .or_else(|| {
            JsonWalletProfileRepository::at_default_location()
                .configured_path()
                .and_then(std::path::Path::parent)
                .map(|directory| directory.join("private/passport-vault.json"))
        });
    path.and_then(|path| PassportVaultStoreConfig::new(path).ok())
        .map_or_else(PassportVaultRepositoryComposition::unavailable, |config| {
            PassportVaultRepositoryComposition {
                repository: Arc::new(JsonPassportVaultRepository::new(config)),
                persistence: "owner_private_atomic_file",
            }
        })
}

#[cfg(target_arch = "wasm32")]
fn headless_passport_vault_repository() -> PassportVaultRepositoryComposition {
    PassportVaultRepositoryComposition::process_local()
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "ios", target_os = "android"))
))]
fn headless_did_resolver() -> Arc<dyn DidResolutionPort> {
    std::env::var_os(MIDNIGHT_DID_RESOLVER_URL_ENV)
        .and_then(|value| value.into_string().ok())
        .and_then(|value| HttpDidResolverConfig::new(value).ok())
        .map_or_else(
            || Arc::new(StandaloneDidResolver) as Arc<dyn DidResolutionPort>,
            |config| Arc::new(HttpDidResolver::new(config)) as Arc<dyn DidResolutionPort>,
        )
}

#[cfg(any(target_arch = "wasm32", target_os = "ios", target_os = "android"))]
fn headless_did_resolver() -> Arc<dyn DidResolutionPort> {
    Arc::new(StandaloneDidResolver)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod standalone_funding_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use futures::executor::block_on;
    #[cfg(not(target_arch = "wasm32"))]
    use midnight_base_crypto::fab::AlignedValue;
    #[cfg(not(target_arch = "wasm32"))]
    use midnight_ledger::structure::INITIAL_PARAMETERS;
    #[cfg(not(target_arch = "wasm32"))]
    use midnight_onchain_runtime::state::{ChargedState, ContractState, StateValue};
    #[cfg(not(target_arch = "wasm32"))]
    use midnight_serialize::{tagged_deserialize, tagged_serialize};
    #[cfg(not(target_arch = "wasm32"))]
    use midnight_storage::{DefaultDB, arena::Sp, storage::Array};
    #[cfg(not(target_arch = "wasm32"))]
    use midnight_zswap::ledger::State as ZswapChainState;
    #[cfg(not(target_arch = "wasm32"))]
    use oxid_adapter_vc_midnight::standalone_digital_passport_issuer_trust_anchor;
    use oxid_credential_application::{
        CredentialOperationError, CredentialProfileQuery, CredentialRepositoryError,
    };
    use oxid_identity_application::{
        CreateDidCommand, DidOperationError, DidRecordRepositoryError, ListDidRecordsQuery,
    };
    #[cfg(not(target_arch = "wasm32"))]
    use oxid_passport_vault_application::{
        AUTHORIZE_PASSPORT_VAULT_CALL_INTENT, AuthorizePassportVaultCallCommand,
        PassportVaultContractStateReadFuture, PassportVaultContractStateSourceError,
        PreparePassportVaultCallAction, PreparePassportVaultCallCommand,
        SUBMIT_PASSPORT_VAULT_CALL_INTENT, SubmitPassportVaultCallCommand,
    };
    #[cfg(not(target_arch = "wasm32"))]
    use oxid_protocol_application::{
        AcceptCredentialIssuanceCommand, PrepareCredentialIssuanceCommand,
    };
    use oxid_wallet_application::{
        CreateWalletProfileCommand, DeriveWalletAccountCommand,
        EXPORT_COMPLETE_WALLET_BACKUP_SUMMARY, EXPORT_COMPLETE_WALLET_BACKUP_TITLE,
        ExportCompleteWalletBackupCommand, RECOVER_COMPLETE_WALLET_BACKUP_SUMMARY,
        RECOVER_COMPLETE_WALLET_BACKUP_TITLE, RecoverCompleteWalletBackupCommand,
        SensitiveOperationConfirmation, WalletAccountQuery, WalletBackupReceiptCommand,
        WalletDustSyncCommand, WalletProfileSecurityCommand, WalletRecoverySecret,
        WalletShieldedSyncCommand,
    };

    #[cfg(not(target_arch = "wasm32"))]
    struct FixedVaultChainContext;

    #[cfg(not(target_arch = "wasm32"))]
    impl PassportVaultCallChainContextSource for FixedVaultChainContext {
        fn chain_context(
            &self,
            snapshot: &PassportVaultContractStateSnapshot,
        ) -> Result<
            oxid_adapter_passport_vault::PassportVaultCallChainContext,
            PassportVaultCallPortError,
        > {
            oxid_adapter_passport_vault::PassportVaultCallChainContext::from_snapshot(
                snapshot,
                vec![1],
                vec![2],
            )
            .map_err(|_| PassportVaultCallPortError::InvalidChainState)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    struct ManagedClaimVaultChainContext;

    #[cfg(not(target_arch = "wasm32"))]
    impl PassportVaultCallChainContextSource for ManagedClaimVaultChainContext {
        fn chain_context(
            &self,
            snapshot: &PassportVaultContractStateSnapshot,
        ) -> Result<
            oxid_adapter_passport_vault::PassportVaultCallChainContext,
            PassportVaultCallPortError,
        > {
            let mut zswap_chain_state = Vec::new();
            tagged_serialize(&ZswapChainState::<DefaultDB>::new(), &mut zswap_chain_state)
                .map_err(|_| PassportVaultCallPortError::InvalidData)?;
            let mut ledger_parameters = Vec::new();
            tagged_serialize(&INITIAL_PARAMETERS, &mut ledger_parameters)
                .map_err(|_| PassportVaultCallPortError::InvalidData)?;
            oxid_adapter_passport_vault::PassportVaultCallChainContext::from_snapshot(
                snapshot,
                zswap_chain_state,
                ledger_parameters,
            )
            .map_err(|_| PassportVaultCallPortError::InvalidChainState)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[derive(Clone)]
    struct ManagedClaimVaultStateSource {
        snapshot: PassportVaultContractStateSnapshot,
    }

    #[cfg(not(target_arch = "wasm32"))]
    impl PassportVaultContractStateSourcePort for ManagedClaimVaultStateSource {
        fn read<'a>(
            &'a self,
            contract_address_hex: &'a str,
        ) -> PassportVaultContractStateReadFuture<'a> {
            Box::pin(async move {
                if contract_address_hex != self.snapshot.contract_address_hex {
                    return Err(PassportVaultContractStateSourceError::NotFound);
                }
                Ok(self.snapshot.clone())
            })
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn managed_claim_contract_state() -> Vec<u8> {
        const FIXTURE: &str =
            include_str!("../../../fixtures/passport-vault/contract-state-v1.hex");
        let mut cursor = std::io::Cursor::new(hex::decode(FIXTURE.trim()).expect("fixture bytes"));
        let mut contract: ContractState<DefaultDB> =
            tagged_deserialize(&mut cursor).expect("fixture state");
        let StateValue::Array(fields) = contract.data.get_ref() else {
            panic!("fixture ledger fields");
        };
        let mut fields: Vec<StateValue<DefaultDB>> = fields.iter_deref().cloned().collect();

        let trust = standalone_digital_passport_issuer_trust_anchor();
        let issuer_contract: [u8; 32] = hex::decode(
            trust
                .issuer_did()
                .strip_prefix("did:midnight:undeployed:")
                .expect("standalone issuer DID"),
        )
        .expect("issuer contract hex")
        .try_into()
        .expect("issuer contract bytes");
        fields[2] = StateValue::Cell(Sp::new(AlignedValue::from((
            issuer_contract,
            trust.method_id(),
        ))));
        fields[3] = StateValue::Cell(Sp::new(AlignedValue::from(trust.public_key_hash())));

        let locks = match &fields[4] {
            StateValue::Map(locks) => locks.clone(),
            _ => panic!("fixture locks"),
        };
        let record = (
            [9_u8; 32], 18_u8, false, [0_u8; 32], false, [0_u8; 32], 40_u128, [5_u8; 32], 100_u128,
            0_u128,
        );
        fields[4] = StateValue::Map(locks.insert(
            AlignedValue::from(0_u64),
            StateValue::Cell(Sp::new(AlignedValue::from(record))),
        ));
        fields[7] = StateValue::Cell(Sp::new(AlignedValue::from(100_u128)));
        contract.data = ChargedState::new(StateValue::Array(Array::new_from_slice(&fields)));

        let mut state = Vec::new();
        tagged_serialize(&contract, &mut state).expect("claim-ready state");
        state
    }

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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn standalone_composition_recovers_a_complete_wallet_into_a_fresh_instance() {
        let source = compose_in_memory();
        let profile = source
            .create_wallet_profile()
            .execute(CreateWalletProfileCommand {
                display_name: "Portable standalone wallet".to_owned(),
            })
            .expect("source profile");
        source
            .record_wallet_backup_receipt()
            .execute(WalletBackupReceiptCommand {
                profile_id: profile.id.clone(),
            })
            .expect("prior complete-document export receipt");
        assert!(
            source
                .get_wallet_backup_receipt()
                .execute(WalletBackupReceiptCommand {
                    profile_id: profile.id.clone(),
                })
                .expect("source receipt query")
                .is_some()
        );
        source
            .initialize_wallet_security()
            .execute(WalletProfileSecurityCommand {
                profile_id: profile.id.clone(),
            })
            .expect("source custody");
        let account = source
            .derive_wallet_account()
            .execute(DeriveWalletAccountCommand {
                profile_id: profile.id.clone(),
                account_index: 0,
                address_index: 0,
            })
            .expect("source account");
        let did = source
            .create_did()
            .execute(CreateDidCommand {
                profile_id: profile.id.clone(),
                network: "undeployed".to_owned(),
            })
            .expect("source DID");
        let authentication_method = did
            .document
            .relationships
            .iter()
            .find(|relationship| relationship.relationship == "authentication")
            .and_then(|relationship| relationship.method_ids.first())
            .cloned()
            .expect("authentication method");
        let holder_method = did
            .document
            .verification_methods
            .iter()
            .find(|method| method.public_key_jwk.curve == "Jubjub")
            .map(|method| method.id.clone())
            .expect("holder method");
        let issuance = block_on(source.prepare_credential_issuance().execute(
            PrepareCredentialIssuanceCommand {
                profile_id: profile.id.clone(),
                offer: standalone_oid4vci_offer(),
            },
        ))
        .expect("issuance plan");
        block_on(
            source
                .accept_credential_issuance()
                .execute(AcceptCredentialIssuanceCommand {
                    profile_id: profile.id.clone(),
                    issuance_id: issuance.id,
                    holder_did: did.document.id.clone(),
                    method_id: authentication_method,
                    holder_binding_method_id: holder_method,
                    confirmed: true,
                    intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
                }),
        )
        .expect("source credential");

        let backup = source
            .export_complete_wallet_backup()
            .execute(ExportCompleteWalletBackupCommand {
                profile_id: profile.id.clone(),
                recovery_secret: WalletRecoverySecret::parse("standalone complete recovery secret")
                    .expect("recovery secret"),
                confirmation: SensitiveOperationConfirmation {
                    title: EXPORT_COMPLETE_WALLET_BACKUP_TITLE.to_owned(),
                    summary: EXPORT_COMPLETE_WALLET_BACKUP_SUMMARY.to_owned(),
                    confirmed: true,
                },
            })
            .expect("complete backup");

        let destination = compose_in_memory();
        let summary = destination
            .recover_complete_wallet_backup()
            .execute(RecoverCompleteWalletBackupCommand {
                expected_profile_id: None,
                backup,
                recovery_secret: WalletRecoverySecret::parse("standalone complete recovery secret")
                    .expect("recovery secret"),
                confirmation: SensitiveOperationConfirmation {
                    title: RECOVER_COMPLETE_WALLET_BACKUP_TITLE.to_owned(),
                    summary: RECOVER_COMPLETE_WALLET_BACKUP_SUMMARY.to_owned(),
                    confirmed: true,
                },
            })
            .expect("fresh-instance recovery");

        assert_eq!(summary.profile_id, profile.id);
        assert!(summary.restored_key_count >= 4);
        assert_eq!(summary.restored_did_count, 1);
        assert_eq!(summary.restored_credential_count, 1);
        assert_eq!(
            destination
                .get_wallet_backup_receipt()
                .execute(WalletBackupReceiptCommand {
                    profile_id: summary.profile_id.clone(),
                })
                .expect("recovered receipt query"),
            None
        );
        assert_eq!(
            destination
                .get_active_wallet_profile()
                .execute()
                .expect("active profile")
                .expect("recovered profile"),
            profile
        );
        assert!(
            destination
                .get_wallet_account()
                .execute(WalletAccountQuery {
                    profile_id: summary.profile_id.clone(),
                })
                .expect("recovered account")
                .addresses
                .contains(&account.receive_address)
        );
        assert_eq!(
            destination
                .list_did_records()
                .execute(ListDidRecordsQuery {
                    profile_id: summary.profile_id.clone(),
                })
                .expect("recovered DIDs")
                .len(),
            1
        );
        assert_eq!(
            destination
                .list_credentials()
                .execute(CredentialProfileQuery {
                    profile_id: summary.profile_id.clone(),
                })
                .expect("recovered credentials")
                .len(),
            1
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_vault_context_is_joined_only_inside_composition() {
        let services = compose_in_memory();
        let source = ComposedPassportVaultCallContextSource {
            wallet: Arc::clone(&services.midnight_public_call_context),
            chain: Arc::new(FixedVaultChainContext),
        };
        let snapshot = PassportVaultContractStateSnapshot {
            serialized_contract_state: vec![3],
            authentication:
                oxid_passport_vault_application::PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
            contract_address_hex: "11".repeat(32),
            transaction_hash_hex: "22".repeat(32),
            action_block_hash_hex: "33".repeat(32),
            action_block_height: 4,
            finalized_head_hash_hex: "44".repeat(32),
            finalized_head_height: 5,
            finalized_head_time_seconds: 1_700_000_000,
        };
        let context = source
            .context("profile_test", &snapshot)
            .expect("public contexts join");
        let debug = format!("{context:?}");
        assert!(debug.contains("undeployed"));
        assert!(debug.contains("zswap_chain_state_bytes: 1"));
        assert!(!debug.contains("094a9125"));

        let state =
            Arc::new(SimulatedPassportVaultStateSource::new().expect("simulated state source"));
        let state_port: Arc<dyn PassportVaultContractStateSourcePort> = state;
        let composer = std::fs::canonicalize(std::env::current_exe().expect("test executable"))
            .expect("canonical test executable");
        let services = with_native_passport_vault_calls(
            services,
            state_port,
            Arc::new(FixedVaultChainContext),
            composer,
        )
        .expect("native adapter wiring");
        assert_eq!(services.passport_vault_call_mode(), "native_settlement");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn standalone_managed_claim_composes_and_settles_through_the_native_stack() {
        let Some(composer) = std::env::var_os("OXID_PASSPORT_VAULT_COMPOSER") else {
            return;
        };
        let composer = std::fs::canonicalize(composer).expect("packaged composer");
        let services = compose_in_memory();
        let profile = services
            .create_wallet_profile()
            .execute(CreateWalletProfileCommand {
                display_name: "Managed vault claimant".to_owned(),
            })
            .expect("profile");
        services
            .initialize_wallet_security()
            .execute(WalletProfileSecurityCommand {
                profile_id: profile.id.clone(),
            })
            .expect("protected custody");
        services
            .derive_wallet_account()
            .execute(DeriveWalletAccountCommand {
                profile_id: profile.id.clone(),
                account_index: 0,
                address_index: 0,
            })
            .expect("managed Midnight account");
        block_on(services.sync_wallet_account().execute(WalletAccountQuery {
            profile_id: profile.id.clone(),
        }))
        .expect("synchronized Midnight account");

        let did = services
            .create_did()
            .execute(CreateDidCommand {
                profile_id: profile.id.clone(),
                network: "undeployed".to_owned(),
            })
            .expect("managed DID");
        let authentication_method = did
            .document
            .relationships
            .iter()
            .find(|relationship| relationship.relationship == "authentication")
            .and_then(|relationship| relationship.method_ids.first())
            .cloned()
            .expect("authentication method");
        let holder_method = did
            .document
            .verification_methods
            .iter()
            .find(|method| method.public_key_jwk.curve == "Jubjub")
            .map(|method| method.id.clone())
            .expect("managed Jubjub method");

        let issuance = block_on(services.prepare_credential_issuance().execute(
            PrepareCredentialIssuanceCommand {
                profile_id: profile.id.clone(),
                offer: standalone_oid4vci_offer(),
            },
        ))
        .expect("issuance plan");
        let issued = block_on(services.accept_credential_issuance().execute(
            AcceptCredentialIssuanceCommand {
                profile_id: profile.id.clone(),
                issuance_id: issuance.id,
                holder_did: did.document.id,
                method_id: authentication_method,
                holder_binding_method_id: holder_method,
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
            },
        ))
        .expect("holder-bound credential");
        let credential_id = issued.credential_id.expect("credential identifier");

        let finalized_head_time_seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs();
        let contract_address_hex = "aa".repeat(32);
        let state_source: Arc<dyn PassportVaultContractStateSourcePort> =
            Arc::new(ManagedClaimVaultStateSource {
                snapshot: PassportVaultContractStateSnapshot {
                    serialized_contract_state: managed_claim_contract_state(),
                    authentication: oxid_passport_vault_application::PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
                    contract_address_hex: contract_address_hex.clone(),
                    transaction_hash_hex: "bb".repeat(32),
                    action_block_hash_hex: "cc".repeat(32),
                    action_block_height: 40,
                    finalized_head_hash_hex: "dd".repeat(32),
                    finalized_head_height: 42,
                    finalized_head_time_seconds,
                },
            });
        let services = with_native_passport_vault_calls(
            services,
            state_source,
            Arc::new(ManagedClaimVaultChainContext),
            composer,
        )
        .expect("native protected claim composition");

        let prepared = block_on(services.prepare_passport_vault_call().execute(
            PreparePassportVaultCallCommand {
                profile_id: profile.id.clone(),
                contract_address_hex,
                action: PreparePassportVaultCallAction::ClaimFromLock {
                    lock_id: 0,
                    amount: "1".to_owned(),
                    credential_id,
                },
            },
        ))
        .expect("protected claim plan");
        assert_eq!(prepared.operation, "claim_from_lock");
        assert_eq!(prepared.state, "prepared");
        assert!(!prepared.submission_ready);

        let authorized = services
            .authorize_passport_vault_call()
            .execute(AuthorizePassportVaultCallCommand {
                profile_id: profile.id.clone(),
                draft_id: prepared.draft_id.clone(),
                authorization_challenge: prepared.authorization_challenge,
                confirmed: true,
                intent: AUTHORIZE_PASSPORT_VAULT_CALL_INTENT.to_owned(),
            })
            .expect("managed claim authorization and composition");
        assert_eq!(authorized.state, "authorized");
        assert!(authorized.submission_ready);

        let submitted = block_on(services.submit_passport_vault_call().execute(
            SubmitPassportVaultCallCommand {
                profile_id: profile.id.clone(),
                draft_id: prepared.draft_id.clone(),
                confirmed: true,
                intent: SUBMIT_PASSPORT_VAULT_CALL_INTENT.to_owned(),
            },
        ))
        .expect("native claim settlement");
        assert_eq!(submitted.call.operation, "claim_from_lock");
        assert_eq!(submitted.call.state, "submitted");
        assert_eq!(submitted.mode, "simulated");
        assert_ne!(submitted.transaction_hash_hex, "00".repeat(32));
        assert_ne!(submitted.block_hash_hex, "00".repeat(32));
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
        drop(services.export_portable_wallet_backup());
        drop(services.recover_portable_wallet_backup());
        drop(services.export_complete_wallet_backup());
        drop(services.recover_complete_wallet_backup());
        drop(services.portable_wallet_backup_documents());
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
        drop(services.prepare_passport_vault_call());
        drop(services.authorize_passport_vault_call());
        drop(services.submit_passport_vault_call());
        drop(services.get_passport_vault_call());
        drop(services.get_passport_vault_call_submission_status());
        drop(services.cancel_passport_vault_call_submission());
        drop(services.list_passport_vault_call_submissions());
        drop(services.reconcile_passport_vault_call_submission());
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn mobile_development_routes_require_tls_for_remote_proving() {
        drop(
            compose_mobile_development_standalone_from_routes(
                "wss://laptop.example.invalid:8443/api/v4/graphql/ws",
                "https://laptop.example.invalid:8443/api/v4/graphql",
                "wss://laptop.example.invalid:10000",
                "https://laptop.example.invalid",
            )
            .expect("explicit TLS standalone routes compose without network I/O"),
        );
        assert!(matches!(
            compose_mobile_development_standalone_from_routes(
                "ws://100.64.0.1:8088/api/v4/graphql/ws",
                "http://100.64.0.1:8088/api/v4/graphql",
                "ws://100.64.0.1:9944",
                "http://100.64.0.1:6300",
            ),
            Err(
                HeadlessCompositionError::InvalidMidnightStandaloneConfiguration(
                    MidnightStandaloneConfigError::InvalidProofEndpoint
                )
            )
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn mobile_development_routes_accept_the_reviewed_loopback_stack() {
        drop(
            compose_mobile_development_standalone_from_routes(
                "ws://127.0.0.1:8088/api/v4/graphql/ws",
                "http://127.0.0.1:8088/api/v4/graphql",
                "ws://127.0.0.1:9944",
                "http://127.0.0.1:6300",
            )
            .expect("reviewed localhost standalone routes compose without network I/O"),
        );
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
                did_jubjub_challenge_signing: Arc::new(UnavailableDidLifecycle),
                credential_repository: Arc::new(UnavailableCredentialRepository),
                credential_inbox: Arc::new(UnavailableCredentialInbox),
                credential_verifier: Arc::new(UnavailableCredentialVerifier),
                credential_disclosure: Arc::new(UnavailableCredentialDisclosure),
                credential_issuance: CredentialIssuanceComposition::Unavailable,
                self_issued_authentication: SelfIssuedAuthenticationComposition::Unavailable,
                credential_presentation: CredentialPresentationComposition::Unavailable,
                portal_test_ingress: false,
            },
            PassportVaultRepositoryComposition::unavailable(),
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
        assert_eq!(
            parse_optional_passport_vault_deployment_height(None),
            Ok(None)
        );
        assert_eq!(
            parse_optional_passport_vault_deployment_height(Some("42".to_owned())),
            Ok(Some(42))
        );
        for invalid in ["", "0", "-1", " 42", "18446744073709551616"] {
            assert_eq!(
                parse_optional_passport_vault_deployment_height(Some(invalid.to_owned())),
                Err(HeadlessCompositionError::InvalidPassportVaultDeploymentHeight)
            );
        }
    }
}
