// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_adapter_midnight::MidnightPublicCallContextSource;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{MidnightContractCallFundingPort, MidnightContractCallSubmissionPort};

#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_vc_midnight::ProtectedDigitalPassportPresentationSource;
use oxid_credential_application::{
    DeleteCredentialUseCase, GetCredentialDisclosureUseCase, GetCredentialUseCase,
    ListCredentialsUseCase, PreviewCredentialDisclosureUseCase, ReceiveCredentialUseCase,
    RevealCredentialClaimUseCase, ReverifyCredentialUseCase,
};
use oxid_diagnostics_application::{
    ClearDiagnosticsUseCase, DiagnosticEventSinkPort, GetDiagnosticSnapshotUseCase,
};
use oxid_identity_application::{
    CreateDidUseCase, DeactivateDidUseCase, ForgetDidUseCase, GetDidRecordUseCase,
    ListDidRecordsUseCase, PublishDidUseCase, ResolveDidUseCase, SignDidPayloadUseCase,
    UpdateDidUseCase,
};
use oxid_passport_vault_application::{
    AuthorizePassportVaultCallUseCase, CancelPassportVaultCallSubmissionUseCase,
    ClaimPassportVaultLockUseCase, CreatePassportVaultLockUseCase,
    DecodePassportVaultContractStateUseCase, DepositPassportVaultLockUseCase,
    GetPassportVaultCallSubmissionStatusUseCase, GetPassportVaultCallUseCase,
    ListPassportVaultCallSubmissionsUseCase, ListPassportVaultLocksUseCase,
    PreparePassportVaultCallUseCase, ReadPassportVaultContractStateUseCase,
    ReconcilePassportVaultCallSubmissionUseCase, SubmitPassportVaultCallUseCase,
    WithdrawPassportVaultLockUseCase,
};
use oxid_platform_ports::{
    IdentityLinkIngressPort, PublicTextExportPort, QrScannerPort, ScreenPrivacyPort,
};
use oxid_presentation_application::{
    AcceptCredentialPresentationUseCase, CancelCredentialPresentationUseCase,
    GetCredentialPresentationUseCase, ListCredentialPresentationsUseCase,
    PrepareCredentialPresentationUseCase, RefuseCredentialPresentationUseCase,
    SetCredentialPresentationForegroundUseCase,
};
use oxid_protocol_application::{
    AcceptCredentialIssuanceUseCase, AcceptSelfIssuedAuthenticationUseCase,
    GetCredentialIssuanceUseCase, GetSelfIssuedAuthenticationUseCase,
    ListCredentialIssuancesUseCase, ListSelfIssuedAuthenticationsUseCase,
    PrepareCredentialIssuanceUseCase, PrepareSelfIssuedAuthenticationUseCase,
    RefuseCredentialIssuanceUseCase, RefuseSelfIssuedAuthenticationUseCase,
    RouteIdentityRequestUseCase,
};
use oxid_wallet_application::{
    AuthorizeWalletDustRegistrationUseCase, AuthorizeWalletTransferUseCase,
    CancelWalletDustRegistrationSubmissionUseCase, CancelWalletDustSyncUseCase,
    CancelWalletShieldedSyncUseCase, CancelWalletTransferSubmissionUseCase,
    CreateWalletProfileUseCase, DeleteWalletKeyUseCase, DeriveWalletAccountUseCase,
    ExportCompleteWalletBackupUseCase, ExportPortableWalletBackupUseCase, GenerateWalletKeyUseCase,
    GetActiveWalletProfileUseCase, GetWalletAccountUseCase, GetWalletBackupReceiptUseCase,
    GetWalletDustRegistrationStatusUseCase, GetWalletDustRegistrationUseCase,
    GetWalletDustSyncStatusUseCase, GetWalletSecurityStatusUseCase,
    GetWalletShieldedSyncStatusUseCase, GetWalletTransferDraftUseCase,
    GetWalletTransferSubmissionStatusUseCase, InitializeWalletSecurityUseCase,
    ListWalletKeysUseCase, ListWalletNetworksUseCase, ListWalletProfilesUseCase,
    ListWalletTransferSubmissionsUseCase, LockWalletUseCase, PortableWalletBackupDocumentPort,
    PrepareShieldedWalletTransferUseCase, PrepareWalletDustRegistrationUseCase,
    PrepareWalletTransferUseCase, ReconcileWalletDustRegistrationSubmissionUseCase,
    ReconcileWalletTransferSubmissionUseCase, RecordWalletBackupReceiptUseCase,
    RecoverCompleteWalletBackupUseCase, RecoverPortableWalletBackupUseCase,
    SelectWalletNetworkUseCase, SelectWalletProfileUseCase, SignWalletDataUseCase,
    StartWalletDustSyncUseCase, StartWalletShieldedSyncUseCase,
    SubmitWalletDustRegistrationUseCase, SubmitWalletTransferUseCase, SyncWalletAccountUseCase,
    UnlockWalletUseCase,
};

/// Application capabilities shared by every incoming adapter.
#[derive(Clone)]
pub struct ApplicationServices {
    pub(super) diagnostic_events: Arc<dyn DiagnosticEventSinkPort>,
    pub(super) get_diagnostic_snapshot: Arc<dyn GetDiagnosticSnapshotUseCase>,
    pub(super) clear_diagnostics: Arc<dyn ClearDiagnosticsUseCase>,
    pub(super) qr_scanner: Arc<dyn QrScannerPort>,
    pub(super) identity_link_ingress: Arc<dyn IdentityLinkIngressPort>,
    pub(super) public_text_exporter: Arc<dyn PublicTextExportPort>,
    pub(super) screen_privacy: Arc<dyn ScreenPrivacyPort>,
    pub(super) portable_wallet_backup_documents: Arc<dyn PortableWalletBackupDocumentPort>,
    pub(super) route_identity_request: Arc<dyn RouteIdentityRequestUseCase>,
    pub(super) midnight_public_call_context: Arc<dyn MidnightPublicCallContextSource>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) midnight_contract_call_funding: Arc<dyn MidnightContractCallFundingPort>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) midnight_contract_call_submission: Arc<dyn MidnightContractCallSubmissionPort>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) protected_passport_vault_presentations:
        Option<Arc<ProtectedDigitalPassportPresentationSource>>,
    pub(super) create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
    pub(super) list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
    pub(super) select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
    pub(super) get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
    pub(super) get_wallet_backup_receipt: Arc<dyn GetWalletBackupReceiptUseCase>,
    pub(super) record_wallet_backup_receipt: Arc<dyn RecordWalletBackupReceiptUseCase>,
    pub(super) get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase>,
    pub(super) initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase>,
    pub(super) unlock_wallet: Arc<dyn UnlockWalletUseCase>,
    pub(super) lock_wallet: Arc<dyn LockWalletUseCase>,
    pub(super) export_portable_wallet_backup: Arc<dyn ExportPortableWalletBackupUseCase>,
    pub(super) recover_portable_wallet_backup: Arc<dyn RecoverPortableWalletBackupUseCase>,
    pub(super) export_complete_wallet_backup: Arc<dyn ExportCompleteWalletBackupUseCase>,
    pub(super) recover_complete_wallet_backup: Arc<dyn RecoverCompleteWalletBackupUseCase>,
    pub(super) generate_wallet_key: Arc<dyn GenerateWalletKeyUseCase>,
    pub(super) list_wallet_keys: Arc<dyn ListWalletKeysUseCase>,
    pub(super) sign_wallet_data: Arc<dyn SignWalletDataUseCase>,
    pub(super) delete_wallet_key: Arc<dyn DeleteWalletKeyUseCase>,
    pub(super) list_wallet_networks: Arc<dyn ListWalletNetworksUseCase>,
    pub(super) select_wallet_network: Arc<dyn SelectWalletNetworkUseCase>,
    pub(super) derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase>,
    pub(super) get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
    pub(super) sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
    pub(super) get_wallet_dust_sync_status: Arc<dyn GetWalletDustSyncStatusUseCase>,
    pub(super) start_wallet_dust_sync: Arc<dyn StartWalletDustSyncUseCase>,
    pub(super) cancel_wallet_dust_sync: Arc<dyn CancelWalletDustSyncUseCase>,
    pub(super) get_wallet_shielded_sync_status: Arc<dyn GetWalletShieldedSyncStatusUseCase>,
    pub(super) start_wallet_shielded_sync: Arc<dyn StartWalletShieldedSyncUseCase>,
    pub(super) cancel_wallet_shielded_sync: Arc<dyn CancelWalletShieldedSyncUseCase>,
    pub(super) prepare_wallet_dust_registration: Arc<dyn PrepareWalletDustRegistrationUseCase>,
    pub(super) authorize_wallet_dust_registration: Arc<dyn AuthorizeWalletDustRegistrationUseCase>,
    pub(super) submit_wallet_dust_registration: Arc<dyn SubmitWalletDustRegistrationUseCase>,
    pub(super) get_wallet_dust_registration: Arc<dyn GetWalletDustRegistrationUseCase>,
    pub(super) get_wallet_dust_registration_status: Arc<dyn GetWalletDustRegistrationStatusUseCase>,
    pub(super) cancel_wallet_dust_registration_submission:
        Arc<dyn CancelWalletDustRegistrationSubmissionUseCase>,
    pub(super) reconcile_wallet_dust_registration_submission:
        Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase>,
    pub(super) prepare_shielded_wallet_transfer: Arc<dyn PrepareShieldedWalletTransferUseCase>,
    pub(super) prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
    pub(super) authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
    pub(super) submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
    pub(super) get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
    pub(super) get_wallet_transfer_submission_status:
        Arc<dyn GetWalletTransferSubmissionStatusUseCase>,
    pub(super) cancel_wallet_transfer_submission: Arc<dyn CancelWalletTransferSubmissionUseCase>,
    pub(super) list_wallet_transfer_submissions: Arc<dyn ListWalletTransferSubmissionsUseCase>,
    pub(super) reconcile_wallet_transfer_submission:
        Arc<dyn ReconcileWalletTransferSubmissionUseCase>,
    pub(super) create_did: Arc<dyn CreateDidUseCase>,
    pub(super) resolve_did: Arc<dyn ResolveDidUseCase>,
    pub(super) list_did_records: Arc<dyn ListDidRecordsUseCase>,
    pub(super) publish_did: Option<Arc<dyn PublishDidUseCase>>,
    pub(super) get_did_record: Arc<dyn GetDidRecordUseCase>,
    pub(super) update_did: Arc<dyn UpdateDidUseCase>,
    pub(super) deactivate_did: Arc<dyn DeactivateDidUseCase>,
    pub(super) sign_did_payload: Arc<dyn SignDidPayloadUseCase>,
    pub(super) forget_did: Arc<dyn ForgetDidUseCase>,
    pub(super) receive_credential: Arc<dyn ReceiveCredentialUseCase>,
    pub(super) list_credentials: Arc<dyn ListCredentialsUseCase>,
    pub(super) get_credential: Arc<dyn GetCredentialUseCase>,
    pub(super) reverify_credential: Arc<dyn ReverifyCredentialUseCase>,
    pub(super) delete_credential: Arc<dyn DeleteCredentialUseCase>,
    pub(super) get_credential_disclosure: Arc<dyn GetCredentialDisclosureUseCase>,
    pub(super) preview_credential_disclosure: Arc<dyn PreviewCredentialDisclosureUseCase>,
    pub(super) reveal_credential_claim: Arc<dyn RevealCredentialClaimUseCase>,
    pub(super) prepare_credential_issuance: Arc<dyn PrepareCredentialIssuanceUseCase>,
    pub(super) accept_credential_issuance: Arc<dyn AcceptCredentialIssuanceUseCase>,
    pub(super) refuse_credential_issuance: Arc<dyn RefuseCredentialIssuanceUseCase>,
    pub(super) get_credential_issuance: Arc<dyn GetCredentialIssuanceUseCase>,
    pub(super) list_credential_issuances: Arc<dyn ListCredentialIssuancesUseCase>,
    pub(super) prepare_self_issued_authentication: Arc<dyn PrepareSelfIssuedAuthenticationUseCase>,
    pub(super) accept_self_issued_authentication: Arc<dyn AcceptSelfIssuedAuthenticationUseCase>,
    pub(super) refuse_self_issued_authentication: Arc<dyn RefuseSelfIssuedAuthenticationUseCase>,
    pub(super) get_self_issued_authentication: Arc<dyn GetSelfIssuedAuthenticationUseCase>,
    pub(super) list_self_issued_authentications: Arc<dyn ListSelfIssuedAuthenticationsUseCase>,
    pub(super) prepare_credential_presentation: Arc<dyn PrepareCredentialPresentationUseCase>,
    pub(super) accept_credential_presentation: Arc<dyn AcceptCredentialPresentationUseCase>,
    pub(super) cancel_credential_presentation: Arc<dyn CancelCredentialPresentationUseCase>,
    pub(super) set_credential_presentation_foreground:
        Arc<dyn SetCredentialPresentationForegroundUseCase>,
    pub(super) refuse_credential_presentation: Arc<dyn RefuseCredentialPresentationUseCase>,
    pub(super) get_credential_presentation: Arc<dyn GetCredentialPresentationUseCase>,
    pub(super) list_credential_presentations: Arc<dyn ListCredentialPresentationsUseCase>,
    pub(super) list_passport_vault_locks: Arc<dyn ListPassportVaultLocksUseCase>,
    pub(super) decode_passport_vault_contract_state:
        Arc<dyn DecodePassportVaultContractStateUseCase>,
    pub(super) read_passport_vault_contract_state: Arc<dyn ReadPassportVaultContractStateUseCase>,
    pub(super) create_passport_vault_lock: Arc<dyn CreatePassportVaultLockUseCase>,
    pub(super) deposit_passport_vault_lock: Arc<dyn DepositPassportVaultLockUseCase>,
    pub(super) claim_passport_vault_lock: Arc<dyn ClaimPassportVaultLockUseCase>,
    pub(super) withdraw_passport_vault_lock: Arc<dyn WithdrawPassportVaultLockUseCase>,
    pub(super) prepare_passport_vault_call: Arc<dyn PreparePassportVaultCallUseCase>,
    pub(super) authorize_passport_vault_call: Arc<dyn AuthorizePassportVaultCallUseCase>,
    pub(super) submit_passport_vault_call: Arc<dyn SubmitPassportVaultCallUseCase>,
    pub(super) get_passport_vault_call: Arc<dyn GetPassportVaultCallUseCase>,
    pub(super) get_passport_vault_call_submission_status:
        Arc<dyn GetPassportVaultCallSubmissionStatusUseCase>,
    pub(super) cancel_passport_vault_call_submission:
        Arc<dyn CancelPassportVaultCallSubmissionUseCase>,
    pub(super) list_passport_vault_call_submissions:
        Arc<dyn ListPassportVaultCallSubmissionsUseCase>,
    pub(super) reconcile_passport_vault_call_submission:
        Arc<dyn ReconcilePassportVaultCallSubmissionUseCase>,
    pub(super) passport_vault_call_mode: &'static str,
    pub(super) passport_vault_call_contract_address_hex: Option<&'static str>,
    pub(super) passport_vault_state_persistence: &'static str,
    pub(super) compact_presentation_proof_available: bool,
}

#[cfg(test)]
#[path = "services/tests.rs"]
mod tests;

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
    pub fn publish_did(&self) -> Option<Arc<dyn PublishDidUseCase>> {
        self.publish_did.clone()
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
