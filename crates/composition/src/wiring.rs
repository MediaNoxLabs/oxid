// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

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
use super::portal::PortalPrivateMaterialDecoder;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use oxid_adapter_backup_complete::InMemoryRecoveryJournal;
use oxid_adapter_backup_complete::{CompleteWalletBackupAdapter, RecoveryJournalPort};
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_backup_complete::{FileRecoveryJournal, UnavailableRecoveryJournal};
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_backup_document_mobile::NativePortableWalletBackupDocuments;
use oxid_adapter_backup_portable::PortableCustodyVaultPort;
use oxid_adapter_diagnostics_memory::InMemoryDiagnosticStore;
use oxid_adapter_did_midnight::{StandaloneDidLifecycle, StandaloneDidResolver};
use oxid_adapter_identity_ingress::StrictIdentityRequestRouter;
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_identity_ingress::{NativeIdentityLinkIngress, NativeQrScanner};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{MidnightContractCallFundingPort, MidnightContractCallSubmissionPort};
use oxid_adapter_midnight::{MidnightDiagnosticAttachPort, MidnightPublicCallContextSource};
use oxid_adapter_openid4vci::{
    DidCredentialHolderProof, StandaloneOid4vciIssuer, VerifiedCredentialSink,
};
use oxid_adapter_openid4vp::{CredentialDisclosureCandidateSource, StandaloneOpenId4VpVerifier};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_passport_vault::NativePassportVaultContractStateDecoder;
use oxid_adapter_passport_vault::StandalonePassportVaultCredential;
use oxid_adapter_siopv2::{DidSelfIssuedIdentityProof, StandaloneSiopV2Verifier};

use super::identity::{
    CredentialIssuanceComposition, CredentialPresentationComposition, HeadlessCredentialProfile,
    IdentityAdapters, SelfIssuedAuthenticationComposition, headless_credential_repository,
    headless_did_repository, headless_did_resolver, standalone_openid4vp_request,
    standalone_siopv2_request,
};
use super::passport_vault::{
    PassportVaultRepositoryComposition, headless_passport_vault_repository,
};
use super::services::ApplicationServices;
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_platform_system::{NativePublicTextExporter, NativeScreenPrivacy};
use oxid_adapter_platform_system::{OsRandom, SystemClock};
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_storage_json::JsonWalletProfileRepository;
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
    NativeCompactPresentationVerifier, ProtectedDigitalPassportPresentationSource,
};
use oxid_credential_application::{
    CredentialService, CredentialVerificationPort, DeleteCredentialUseCase,
    GetCredentialDisclosureUseCase, GetCredentialUseCase, ImportVerifiedCredentialUseCase,
    ListCredentialsUseCase, PreviewCredentialDisclosureUseCase, ReceiveCredentialUseCase,
    RevealCredentialClaimUseCase, ReverifyCredentialUseCase,
};
use oxid_diagnostics_application::{
    ClearDiagnosticsUseCase, DiagnosticEventSinkPort, DiagnosticsService,
    GetDiagnosticSnapshotUseCase,
};
use oxid_identity_application::{
    CreateDidUseCase, DeactivateDidUseCase, DidJubjubChallengeSigningPort, DidLifecyclePort,
    DidResolutionPort, DidService, ForgetDidUseCase, GetDidRecordUseCase, ListDidRecordsUseCase,
    ResolveDidUseCase, SignDidPayloadUseCase, UpdateDidUseCase,
};
use oxid_passport_vault_application::{
    AuthorizePassportVaultCallUseCase, CancelPassportVaultCallSubmissionUseCase,
    ClaimPassportVaultLockUseCase, CreatePassportVaultLockUseCase,
    DecodePassportVaultContractStateUseCase, DepositPassportVaultLockUseCase,
    GetPassportVaultCallSubmissionStatusUseCase, GetPassportVaultCallUseCase,
    ListPassportVaultCallSubmissionsUseCase, ListPassportVaultLocksUseCase,
    PassportVaultContractCallService, PassportVaultContractStateDecoderPort,
    PassportVaultContractStateService, PassportVaultContractStateSourcePort,
    PassportVaultCredentialPort, PassportVaultService, PreparePassportVaultCallUseCase,
    ReadPassportVaultContractStateUseCase, ReconcilePassportVaultCallSubmissionUseCase,
    SubmitPassportVaultCallUseCase, UnavailablePassportVaultContractCall,
    UnavailablePassportVaultContractStateSource, UnavailablePassportVaultCredential,
    WithdrawPassportVaultLockUseCase,
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
    SelfIssuedAuthenticationProtocolPort, SelfIssuedAuthenticationService,
    UnavailableCredentialIssuanceProtocol, UnavailableIssuedCredentialSink,
    UnavailableSelfIssuedAuthenticationProtocol,
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
    CompleteWalletBackupService, CreateWalletProfileService, DeleteWalletKeyUseCase,
    DeriveWalletAccountUseCase, ExportCompleteWalletBackupUseCase,
    ExportPortableWalletBackupUseCase, GenerateWalletKeyUseCase, GetActiveWalletProfileService,
    GetWalletAccountUseCase, GetWalletBackupReceiptUseCase, GetWalletDustRegistrationStatusUseCase,
    GetWalletDustRegistrationUseCase, GetWalletDustSyncStatusUseCase,
    GetWalletSecurityStatusUseCase, GetWalletShieldedSyncStatusUseCase,
    GetWalletTransferDraftUseCase, GetWalletTransferSubmissionStatusUseCase,
    InitializeWalletSecurityUseCase, ListWalletKeysUseCase, ListWalletNetworksUseCase,
    ListWalletProfilesService, ListWalletTransferSubmissionsUseCase, LockWalletUseCase,
    PortableWalletBackupDocumentPort, PrepareShieldedWalletTransferUseCase,
    PrepareWalletDustRegistrationUseCase, PrepareWalletTransferUseCase,
    ReconcileWalletDustRegistrationSubmissionUseCase, ReconcileWalletTransferSubmissionUseCase,
    RecordWalletBackupReceiptUseCase, RecoverCompleteWalletBackupUseCase,
    RecoverPortableWalletBackupUseCase, SelectWalletNetworkUseCase, SelectWalletProfileService,
    SignWalletDataUseCase, StartWalletDustSyncUseCase, StartWalletShieldedSyncUseCase,
    SubmitWalletDustRegistrationUseCase, SubmitWalletTransferUseCase, SyncWalletAccountUseCase,
    UnlockWalletUseCase, WalletAccountDerivationPort, WalletAccountDerivationService,
    WalletAccountReadPort, WalletAccountService, WalletBackupReceiptRepository,
    WalletBackupReceiptService, WalletDustRegistrationService, WalletDustSyncPort,
    WalletDustSyncService, WalletJubjubChallengeSigningPort, WalletKeyOperationPort,
    WalletKeyService, WalletNetworkPort, WalletNetworkService, WalletPortableBackupPort,
    WalletPortableBackupService, WalletProfileAssociationRepository, WalletProfileRepository,
    WalletProtectionPort, WalletProtectionService, WalletShieldedSyncPort,
    WalletShieldedSyncService, WalletTransactionPort, WalletTransactionService,
};

#[cfg(not(target_arch = "wasm32"))]
pub(super) trait NativeMidnightCompositionCapability:
    MidnightContractCallFundingPort + MidnightContractCallSubmissionPort
{
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> NativeMidnightCompositionCapability for T where
    T: MidnightContractCallFundingPort + MidnightContractCallSubmissionPort
{
}

#[cfg(target_arch = "wasm32")]
pub(super) trait NativeMidnightCompositionCapability {}

#[cfg(target_arch = "wasm32")]
impl<T> NativeMidnightCompositionCapability for T {}

#[cfg(not(target_arch = "wasm32"))]
pub(super) trait NativeWalletDustRegistrationCapability: WalletDustRegistrationPort {}

#[cfg(not(target_arch = "wasm32"))]
impl<T> NativeWalletDustRegistrationCapability for T where T: WalletDustRegistrationPort {}

#[cfg(target_arch = "wasm32")]
pub(super) trait NativeWalletDustRegistrationCapability {}

#[cfg(target_arch = "wasm32")]
impl<T> NativeWalletDustRegistrationCapability for T {}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) fn complete_wallet_recovery_journal() -> Arc<dyn RecoveryJournalPort> {
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
pub(super) fn complete_wallet_recovery_journal() -> Arc<dyn RecoveryJournalPort> {
    Arc::new(InMemoryRecoveryJournal::default())
}

pub(super) fn compose_with_adapters<R, S, M>(
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

pub(super) fn compose_with_adapters_and_presentation<R, S, M>(
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
    )
}

pub(super) fn compose_with_adapters_and_credential_profile<R, S, M>(
    repository: Arc<R>,
    security: Arc<S>,
    midnight: Arc<M>,
    credential_presentation: CredentialPresentationComposition,
    credential_profile: HeadlessCredentialProfile,
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
    let (compact_issuer_resolver, trust_anchor, credential_issuance, portal_test_ingress) =
        match credential_profile {
            HeadlessCredentialProfile::Standalone => (
                Arc::new(StandaloneDidResolver) as Arc<dyn DidResolutionPort>,
                standalone_digital_passport_issuer_trust_anchor(),
                CredentialIssuanceComposition::Standalone,
                None,
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
            HeadlessCredentialProfile::Portal(portal) => (
                portal.issuer_resolver,
                portal.trust_anchor,
                CredentialIssuanceComposition::Portal(Box::new(portal.client_factory)),
                portal.test_ingress,
            ),
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

pub(super) fn compose_with_identity_adapters<R, S, M>(
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
    let identity_link_ingress: Arc<dyn IdentityLinkIngressPort> =
        portal_test_ingress.unwrap_or_else(|| Arc::new(NativeIdentityLinkIngress::default()));
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
