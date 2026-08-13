// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]
#![recursion_limit = "256"]

use std::{error::Error, fmt, io, io::BufRead, io::Write, thread, time::Duration};

use oxid_composition::ApplicationServices;
use oxid_credential_application::{
    CredentialDisclosurePlanView, CredentialDisclosurePortError, CredentialDisclosureQuery,
    CredentialDisclosureView, CredentialOperationError, CredentialPredicateInput,
    CredentialProfileQuery, CredentialQuery, CredentialRepositoryError,
    CredentialVerificationError, CredentialView, DeleteCredentialCommand,
    PreviewCredentialDisclosureCommand,
};
use oxid_identity_application::{
    CreateDidCommand, DeactivateDidCommand, DidKeyAlgorithm, DidLifecyclePortError,
    DidOperationConfirmation, DidOperationError, DidRecordQuery, DidRecordRepositoryError,
    DidRecordView, DidResolutionPortError, DidUpdate, ListDidRecordsQuery, ResolveDidCommand,
    SignDidPayloadCommand, UpdateDidCommand,
};
use oxid_identity_domain::VerificationRelationship;
use oxid_presentation_application::{
    AcceptCredentialPresentationCommand, CredentialPresentationError,
    CredentialPresentationProfileQuery, CredentialPresentationQuery, CredentialPresentationView,
    PrepareCredentialPresentationCommand, RefuseCredentialPresentationCommand,
};
use oxid_protocol_application::{
    AcceptCredentialIssuanceCommand, AcceptSelfIssuedAuthenticationCommand,
    CredentialIssuanceError, CredentialIssuanceProfileQuery, CredentialIssuanceQuery,
    CredentialIssuanceView, PrepareCredentialIssuanceCommand,
    PrepareSelfIssuedAuthenticationCommand, RefuseCredentialIssuanceCommand,
    RefuseSelfIssuedAuthenticationCommand, SelfIssuedAuthenticationError,
    SelfIssuedAuthenticationProfileQuery, SelfIssuedAuthenticationQuery,
    SelfIssuedAuthenticationView,
};
use oxid_wallet_application::{
    AuthorizeWalletTransferCommand, CreateWalletProfileCommand, CreateWalletProfileError,
    DeleteWalletKeyCommand, DeriveWalletAccountCommand, DerivedWalletAccountView,
    GenerateWalletKeyCommand, PrepareWalletTransferCommand, ReadWalletProfilesError,
    SelectWalletNetworkCommand, SelectWalletProfileCommand, SelectWalletProfileError,
    SensitiveOperationConfirmation, SensitiveWalletOperationError, SignWalletDataCommand,
    SubmitWalletTransferCommand, WalletAccountError, WalletAccountPortError, WalletAccountQuery,
    WalletAccountView, WalletDustSyncCommand, WalletDustSyncError, WalletDustSyncPortError,
    WalletDustSyncView, WalletKeyError, WalletKeyView, WalletNetworkListView,
    WalletProfileRepositoryError, WalletProfileSecurityCommand, WalletProfileView,
    WalletSecurityError, WalletSecurityPortError, WalletSecurityStatusView,
    WalletShieldedSyncCommand, WalletShieldedSyncError, WalletShieldedSyncPortError,
    WalletShieldedSyncView, WalletTransactionError, WalletTransactionPortError,
    WalletTransferDraftQuery, WalletTransferPreviewView, WalletTransferSubmissionQuery,
    WalletTransferSubmissionStatusView, WalletTransferSubmissionView, validate_confirmation,
};
use oxid_wallet_domain::{
    PublicKeyEncoding, WalletKeyAlgorithm, WalletKeyPurpose, WalletProtectionClass,
    WalletProtectionState,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The protocol identifier required on every structured request and response.
pub const PROTOCOL_VERSION: &str = "oxid.headless.v1";

const MAX_REQUEST_ID_CHARACTERS: usize = 128;

/// Drives Oxid application use cases through line-delimited JSON.
pub struct HeadlessWallet {
    application: ApplicationServices,
}

impl HeadlessWallet {
    #[must_use]
    pub const fn new(application: ApplicationServices) -> Self {
        Self { application }
    }

    /// Processes requests until EOF or a successful shutdown request.
    ///
    /// Protocol responses are the only bytes written to `writer`. Callers must
    /// direct operational diagnostics to stderr.
    pub fn run<R: BufRead, W: Write>(
        &self,
        reader: R,
        mut writer: W,
    ) -> Result<(), HeadlessIoError> {
        for line in reader.lines() {
            let line = line.map_err(HeadlessIoError::Read)?;
            if line.trim().is_empty() {
                continue;
            }

            let dispatch = self.dispatch(&line);
            serde_json::to_writer(&mut writer, &dispatch.response)
                .map_err(HeadlessIoError::Serialize)?;
            writer.write_all(b"\n").map_err(HeadlessIoError::Write)?;
            writer.flush().map_err(HeadlessIoError::Write)?;

            if dispatch.should_exit {
                break;
            }
        }

        Ok(())
    }

    fn dispatch(&self, line: &str) -> Dispatch {
        if matches!(line.trim(), "quit" | "exit") {
            return Dispatch::exit(Response::success(
                None,
                json!({ "shuttingDown": true, "alias": line.trim() }),
            ));
        }

        let value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    None,
                    "parse_error",
                    "request is not valid JSON",
                ));
            }
        };

        let request_id = match request_id(&value) {
            Ok(request_id) => request_id,
            Err(message) => {
                return Dispatch::continue_with(Response::error(None, "invalid_request", message));
            }
        };

        let request = match serde_json::from_value::<Request>(value) {
            Ok(request) => request,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request_id,
                    "invalid_request",
                    "request must include string protocol and method fields",
                ));
            }
        };

        if request.protocol != PROTOCOL_VERSION {
            return Dispatch::continue_with(Response::error(
                request.id,
                "unsupported_protocol",
                "request protocol is not supported",
            ));
        }

        if !request.params.is_object() {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "params must be a JSON object",
            ));
        }

        match request.method.as_str() {
            "system.capabilities" => self.capabilities(request),
            "system.quit" => self.quit(request),
            "wallet.profile.create" => self.create_profile(request),
            "wallet.profile.list" => self.list_profiles(request),
            "wallet.profile.select" => self.select_profile(request),
            "wallet.profile.active" => self.active_profile(request),
            "wallet.security.status" => self.security_status(request),
            "wallet.security.initialize" => self.initialize_security(request),
            "wallet.security.unlock" => self.unlock_wallet(request),
            "wallet.security.lock" => self.lock_wallet(request),
            "wallet.key.generate" => self.generate_key(request),
            "wallet.key.list" => self.list_keys(request),
            "wallet.key.sign" => self.sign(request),
            "wallet.key.delete" => self.delete_key(request),
            "wallet.network.list" => self.list_networks(request),
            "wallet.network.select" => self.select_network(request),
            "wallet.account.derive" => self.derive_account(request),
            "wallet.account.get" => self.get_account(request),
            "wallet.address.list" => self.list_addresses(request),
            "wallet.address.unshielded" => self.unshielded_address(request),
            "wallet.address.shielded" => self.shielded_address(request),
            "wallet.balance.snapshot" => self.balance_snapshot(request),
            "wallet.transaction.history" => self.transaction_history(request),
            "wallet.transaction.prepare_unshielded" => self.prepare_unshielded(request),
            "wallet.transaction.authorize_unshielded" => self.authorize_unshielded(request),
            "wallet.transaction.submit_unshielded" | "wallet.transaction.send_unshielded" => {
                self.submit_unshielded(request)
            }
            "wallet.transaction.start_submission" => self.start_submission(request),
            "wallet.transaction.submission_status" => self.submission_status(request),
            "wallet.transaction.submission_history" => self.submission_history(request),
            "wallet.transaction.reconcile_submission" => self.reconcile_submission(request),
            "wallet.transaction.cancel_submission" => self.cancel_submission(request),
            "wallet.transaction.draft" => self.transaction_draft(request),
            "wallet.connect" | "wallet.sync.force" => self.sync_account(request),
            "wallet.dust.sync.status" => self.dust_sync_status(request),
            "wallet.dust.sync.start" => self.start_dust_sync(request),
            "wallet.dust.sync.cancel" => self.cancel_dust_sync(request),
            "wallet.shielded.sync.status" => self.shielded_sync_status(request),
            "wallet.shielded.sync.start" => self.start_shielded_sync(request),
            "wallet.shielded.sync.cancel" => self.cancel_shielded_sync(request),
            "did.create" => self.create_did(request),
            "did.resolve" => self.resolve_did(request),
            "did.list" => self.list_dids(request),
            "did.get" => self.get_did(request),
            "did.update" => self.update_did(request),
            "did.sign" => self.sign_did(request),
            "did.deactivate" => self.deactivate_did(request),
            "did.forget" => self.forget_did(request),
            "credential.receive" | "credential.request" => self.receive_credential(request),
            "credential.list" => self.list_credentials(request),
            "credential.get" => self.get_credential(request),
            "credential.reverify" | "credential.verify" => self.reverify_credential(request),
            "credential.delete" => self.delete_credential(request),
            "credential.disclosure.candidates" => self.credential_disclosure_candidates(request),
            "credential.disclosure.preview" => self.preview_credential_disclosure(request),
            "credential.issuance.prepare" => self.prepare_credential_issuance(request),
            "credential.issuance.accept" => self.accept_credential_issuance(request),
            "credential.issuance.refuse" => self.refuse_credential_issuance(request),
            "credential.issuance.get" => self.get_credential_issuance(request),
            "credential.issuance.list" => self.list_credential_issuances(request),
            "credential.presentation.prepare" => self.prepare_credential_presentation(request),
            "credential.presentation.accept" => self.accept_credential_presentation(request),
            "credential.presentation.refuse" => self.refuse_credential_presentation(request),
            "credential.presentation.get" => self.get_credential_presentation(request),
            "credential.presentation.list" => self.list_credential_presentations(request),
            "identity.login" | "identity.authentication.prepare" => {
                self.prepare_self_issued_authentication(request)
            }
            "identity.authentication.accept" => self.accept_self_issued_authentication(request),
            "identity.authentication.refuse" => self.refuse_self_issued_authentication(request),
            "identity.authentication.get" => self.get_self_issued_authentication(request),
            "identity.authentication.list" => self.list_self_issued_authentications(request),
            _ => Dispatch::continue_with(Response::error(
                request.id,
                "method_not_found",
                "requested method is not implemented",
            )),
        }
    }

    fn capabilities(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "system.capabilities does not accept parameters",
            ));
        }

        Dispatch::continue_with(Response::success(
            request.id,
            json!({
                "implementation": {
                    "name": "oxid-headless",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "methods": capability_manifest(),
                "custodyMode": "development_only",
                "compatibilityAliases": ["quit", "exit"]
            }),
        ))
    }

    fn quit(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "system.quit does not accept parameters",
            ));
        }

        Dispatch::exit(Response::success(
            request.id,
            json!({ "shuttingDown": true }),
        ))
    }

    fn resolve_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.resolve requires only a string did field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(self.application.resolve_did().execute(
            ResolveDidCommand {
                profile_id,
                did: params.did,
            },
        )) {
            Ok(record) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecord": did_record_value(&record) }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    fn create_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CreateDidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.create accepts only an optional network string",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.create_did().execute(CreateDidCommand {
            profile_id,
            network: params.network,
        }) {
            Ok(record) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecord": did_record_value(&record) }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    fn update_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DidUpdateParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.update requires a supported operation and its exact fields",
                ));
            }
        };
        let (did, operation, confirmation) = match did_update(params) {
            Some(value) => value,
            None => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "DID update algorithm or relationship is unsupported",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.update_did().execute(UpdateDidCommand {
            profile_id,
            did,
            operation,
            confirmation,
        }) {
            Ok(record) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecord": did_record_value(&record) }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    fn sign_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SignDidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.sign requires did, methodId, payloadHex, and confirmation",
                ));
            }
        };
        let payload = match decode_hex(&params.payload_hex) {
            Some(payload) => payload,
            None => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "payloadHex must be bounded even-length hexadecimal",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .sign_did_payload()
            .execute(SignDidPayloadCommand {
                profile_id,
                did: params.did,
                method_id: params.method_id,
                payload,
                confirmation: params.confirmation.into(),
            }) {
            Ok(signature) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "methodId": signature.method_id,
                    "algorithm": signature.algorithm,
                    "signatureHex": encode_hex(&signature.signature_bytes),
                }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    fn deactivate_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DeactivateDidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.deactivate requires did and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .deactivate_did()
            .execute(DeactivateDidCommand {
                profile_id,
                did: params.did,
                confirmation: params.confirmation.into(),
            }) {
            Ok(record) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecord": did_record_value(&record) }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    fn list_dids(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "did.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_did_records()
            .execute(ListDidRecordsQuery { profile_id })
        {
            Ok(records) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecords": records.iter().map(did_record_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    fn get_did(&self, request: Request) -> Dispatch {
        self.did_record_operation(request, false)
    }

    fn forget_did(&self, request: Request) -> Dispatch {
        self.did_record_operation(request, true)
    }

    fn did_record_operation(&self, request: Request, remove: bool) -> Dispatch {
        let params = match serde_json::from_value::<DidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    if remove {
                        "did.forget requires only a string did field"
                    } else {
                        "did.get requires only a string did field"
                    },
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let query = DidRecordQuery {
            profile_id,
            did: params.did,
        };
        if remove {
            match self.application.forget_did().execute(query) {
                Ok(()) => Dispatch::continue_with(Response::success(
                    request.id,
                    json!({ "forgotten": true }),
                )),
                Err(error) => Dispatch::continue_with(did_error(request.id, error)),
            }
        } else {
            match self.application.get_did_record().execute(query) {
                Ok(record) => Dispatch::continue_with(Response::success(
                    request.id,
                    json!({ "didRecord": did_record_value(&record) }),
                )),
                Err(error) => Dispatch::continue_with(did_error(request.id, error)),
            }
        }
    }

    fn create_profile(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CreateProfileParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.profile.create requires only a string displayName",
                ));
            }
        };

        let created =
            self.application
                .create_wallet_profile()
                .execute(CreateWalletProfileCommand {
                    display_name: params.display_name,
                });

        match created {
            Ok(profile) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "profile": {
                        "id": profile.id,
                        "displayName": profile.display_name,
                        "createdAtMillis": profile.created_at_millis
                    }
                }),
            )),
            Err(error) => Dispatch::continue_with(profile_error(request.id, error)),
        }
    }

    fn list_profiles(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "wallet.profile.list does not accept parameters",
            ));
        }

        match self.application.list_wallet_profiles().execute() {
            Ok(profiles) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "profiles": profiles.iter().map(profile_value).collect::<Vec<_>>()
                }),
            )),
            Err(error) => Dispatch::continue_with(read_profiles_error(request.id, error)),
        }
    }

    fn select_profile(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SelectProfileParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.profile.select requires only a string profileId",
                ));
            }
        };

        match self
            .application
            .select_wallet_profile()
            .execute(SelectWalletProfileCommand {
                profile_id: params.profile_id,
            }) {
            Ok(profile) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "profile": profile_value(&profile) }),
            )),
            Err(error) => Dispatch::continue_with(select_profile_error(request.id, error)),
        }
    }

    fn active_profile(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "wallet.profile.active does not accept parameters",
            ));
        }

        match self.application.get_active_wallet_profile().execute() {
            Ok(profile) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "profile": profile.as_ref().map(profile_value) }),
            )),
            Err(error) => Dispatch::continue_with(read_profiles_error(request.id, error)),
        }
    }

    fn security_status(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.security.status");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_wallet_security_status()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "security": security_status_value(status) }),
            )),
            Err(error) => Dispatch::continue_with(security_error(request.id, error)),
        }
    }

    fn initialize_security(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.security.initialize");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .initialize_wallet_security()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "security": security_status_value(status) }),
            )),
            Err(error) => Dispatch::continue_with(security_error(request.id, error)),
        }
    }

    fn unlock_wallet(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.security.unlock");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .unlock_wallet()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "security": security_status_value(status) }),
            )),
            Err(error) => Dispatch::continue_with(security_error(request.id, error)),
        }
    }

    fn lock_wallet(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.security.lock");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .lock_wallet()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "security": security_status_value(status) }),
            )),
            Err(error) => Dispatch::continue_with(security_error(request.id, error)),
        }
    }

    fn generate_key(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<GenerateKeyParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.key.generate requires only label, algorithm, and purpose strings",
                ));
            }
        };
        let algorithm = match key_algorithm(&params.algorithm) {
            Some(algorithm) => algorithm,
            None => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "algorithm must be ed25519, p256, secp256k1-schnorr, or jubjub",
                ));
            }
        };
        let purpose = match key_purpose(&params.purpose) {
            Some(purpose) => purpose,
            None => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "purpose is not supported",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .generate_wallet_key()
            .execute(GenerateWalletKeyCommand {
                profile_id,
                label: params.label,
                algorithm,
                purpose,
            }) {
            Ok(key) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "key": key_value(&key) }),
            )),
            Err(error) => Dispatch::continue_with(key_error(request.id, error)),
        }
    }

    fn list_keys(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.key.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_wallet_keys()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(keys) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "keys": keys.iter().map(key_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(key_error(request.id, error)),
        }
    }

    fn sign(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SignParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.key.sign requires keyRef, payloadHex, and confirmation",
                ));
            }
        };
        let payload = match decode_hex(&params.payload_hex) {
            Some(payload) => payload,
            None => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "payloadHex must be bounded even-length hexadecimal",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .sign_wallet_data()
            .execute(SignWalletDataCommand {
                profile_id,
                key_reference: params.key_reference,
                payload,
                confirmation: params.confirmation.into(),
            }) {
            Ok(signature) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "algorithm": algorithm_name(signature.algorithm),
                    "signatureHex": encode_hex(&signature.signature_bytes)
                }),
            )),
            Err(error) => Dispatch::continue_with(sensitive_error(request.id, error)),
        }
    }

    fn delete_key(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DeleteKeyParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.key.delete requires keyRef and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .delete_wallet_key()
            .execute(DeleteWalletKeyCommand {
                profile_id,
                key_reference: params.key_reference,
                confirmation: params.confirmation.into(),
            }) {
            Ok(()) => {
                Dispatch::continue_with(Response::success(request.id, json!({ "deleted": true })))
            }
            Err(error) => Dispatch::continue_with(sensitive_error(request.id, error)),
        }
    }

    fn list_networks(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.network.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_wallet_networks()
            .execute(WalletAccountQuery { profile_id })
        {
            Ok(networks) => Dispatch::continue_with(Response::success(
                request.id,
                network_list_value(&networks),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    fn select_network(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SelectNetworkParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.network.select requires only a string networkId",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .select_wallet_network()
            .execute(SelectWalletNetworkCommand {
                profile_id,
                network_id: params.network_id,
            }) {
            Ok(networks) => Dispatch::continue_with(Response::success(
                request.id,
                network_list_value(&networks),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    fn get_account(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.account.get");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_wallet_account()
            .execute(WalletAccountQuery { profile_id })
        {
            Ok(account) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "account": account_value(&account) }),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    fn derive_account(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DeriveAccountParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.account.derive accepts only optional accountIndex and addressIndex integers",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .derive_wallet_account()
            .execute(DeriveWalletAccountCommand {
                profile_id,
                account_index: params.account_index,
                address_index: params.address_index,
            }) {
            Ok(account) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "account": derived_account_value(&account) }),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    fn sync_account(&self, request: Request) -> Dispatch {
        let method = match request.method.as_str() {
            "wallet.connect" => "wallet.connect",
            _ => "wallet.sync.force",
        };
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, method);
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application
                .sync_wallet_account()
                .execute(WalletAccountQuery { profile_id }),
        ) {
            Ok(account) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "account": account_value(&account) }),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    fn dust_sync_status(&self, request: Request) -> Dispatch {
        self.dust_sync_operation(
            request,
            "wallet.dust.sync.status",
            |application, command| application.get_wallet_dust_sync_status().execute(command),
        )
    }

    fn start_dust_sync(&self, request: Request) -> Dispatch {
        self.dust_sync_operation(request, "wallet.dust.sync.start", |application, command| {
            application.start_wallet_dust_sync().execute(command)
        })
    }

    fn cancel_dust_sync(&self, request: Request) -> Dispatch {
        self.dust_sync_operation(
            request,
            "wallet.dust.sync.cancel",
            |application, command| application.cancel_wallet_dust_sync().execute(command),
        )
    }

    fn dust_sync_operation(
        &self,
        request: Request,
        method: &'static str,
        operation: impl FnOnce(
            &ApplicationServices,
            WalletDustSyncCommand,
        ) -> Result<WalletDustSyncView, WalletDustSyncError>,
    ) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, method);
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match operation(&self.application, WalletDustSyncCommand { profile_id }) {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "dustSync": dust_sync_value(&status) }),
            )),
            Err(error) => Dispatch::continue_with(dust_sync_error(request.id, error)),
        }
    }

    fn shielded_sync_status(&self, request: Request) -> Dispatch {
        self.shielded_sync_operation(
            request,
            "wallet.shielded.sync.status",
            |application, command| {
                application
                    .get_wallet_shielded_sync_status()
                    .execute(command)
            },
        )
    }

    fn start_shielded_sync(&self, request: Request) -> Dispatch {
        self.shielded_sync_operation(
            request,
            "wallet.shielded.sync.start",
            |application, command| application.start_wallet_shielded_sync().execute(command),
        )
    }

    fn cancel_shielded_sync(&self, request: Request) -> Dispatch {
        self.shielded_sync_operation(
            request,
            "wallet.shielded.sync.cancel",
            |application, command| application.cancel_wallet_shielded_sync().execute(command),
        )
    }

    fn shielded_sync_operation(
        &self,
        request: Request,
        method: &'static str,
        operation: impl FnOnce(
            &ApplicationServices,
            WalletShieldedSyncCommand,
        ) -> Result<WalletShieldedSyncView, WalletShieldedSyncError>,
    ) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, method);
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match operation(&self.application, WalletShieldedSyncCommand { profile_id }) {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "shieldedSync": shielded_sync_value(&status) }),
            )),
            Err(error) => Dispatch::continue_with(shielded_sync_error(request.id, error)),
        }
    }

    fn list_addresses(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.address.list", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "addresses": account.addresses.iter().map(address_value).collect::<Vec<_>>()
            })
        })
    }

    fn unshielded_address(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.address.unshielded", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "address": account.addresses.iter().find(|address| address.kind == "unshielded").map(address_value)
            })
        })
    }

    fn shielded_address(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.address.shielded", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "address": account.addresses.iter().find(|address| address.kind == "shielded").map(address_value)
            })
        })
    }

    fn balance_snapshot(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.balance.snapshot", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "balances": account.balances.iter().map(balance_value).collect::<Vec<_>>(),
                "sync": sync_value(account)
            })
        })
    }

    fn transaction_history(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.transaction.history", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "transactions": account.transactions.iter().map(transaction_value).collect::<Vec<_>>()
            })
        })
    }

    fn prepare_unshielded(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<PrepareTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.transaction.prepare_unshielded requires only string recipientAddress and amountAtomicUnits fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .prepare_wallet_transfer()
            .execute(PrepareWalletTransferCommand {
                profile_id,
                recipient_address: params.recipient_address,
                amount_atomic_units: params.amount_atomic_units,
            }) {
            Ok(preview) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "transfer": transfer_preview_value(&preview) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    fn authorize_unshielded(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AuthorizeTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.transaction.authorize_unshielded requires only string draftId and authorizationChallenge fields plus confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .authorize_wallet_transfer()
            .execute(AuthorizeWalletTransferCommand {
                profile_id,
                draft_id: params.draft_id,
                authorization_challenge: params.authorization_challenge,
                confirmation: params.confirmation.into(),
            }) {
            Ok(preview) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "transfer": transfer_preview_value(&preview) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    fn transaction_draft(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<TransactionDraftParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.transaction.draft requires only a string draftId",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_wallet_transfer_draft()
            .execute(WalletTransferDraftQuery {
                profile_id,
                draft_id: params.draft_id,
            }) {
            Ok(preview) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "transfer": transfer_preview_value(&preview) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    fn submit_unshielded(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SubmitTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.transaction.submit_unshielded requires only a string draftId and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(self.application.submit_wallet_transfer().execute(
            SubmitWalletTransferCommand {
                profile_id,
                draft_id: params.draft_id,
                confirmation: params.confirmation.into(),
            },
        )) {
            Ok(submission) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "submission": transfer_submission_value(&submission) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    fn start_submission(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SubmitTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.transaction.start_submission requires only a string draftId and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let preview =
            match self
                .application
                .get_wallet_transfer_draft()
                .execute(WalletTransferDraftQuery {
                    profile_id: profile_id.clone(),
                    draft_id: params.draft_id.clone(),
                }) {
                Ok(preview) => preview,
                Err(error) => return Dispatch::continue_with(transaction_error(request.id, error)),
            };
        match preview.state.as_str() {
            "authorized" | "submitting" | "submitted" => {}
            "expired" => {
                return Dispatch::continue_with(transaction_port_error(
                    request.id,
                    WalletTransactionPortError::DraftExpired,
                ));
            }
            _ => {
                return Dispatch::continue_with(transaction_port_error(
                    request.id,
                    WalletTransactionPortError::DraftConflict,
                ));
            }
        }

        let confirmation: SensitiveOperationConfirmation = params.confirmation.into();
        if let Err(error) = validate_confirmation(&confirmation) {
            return Dispatch::continue_with(sensitive_error(request.id, error));
        }
        let service = self.application.submit_wallet_transfer();
        let command = SubmitWalletTransferCommand {
            profile_id: profile_id.clone(),
            draft_id: params.draft_id.clone(),
            confirmation,
        };
        if thread::Builder::new()
            .name("oxid-headless-submit".to_owned())
            .spawn(move || {
                let _ = futures::executor::block_on(service.execute(command));
            })
            .is_err()
        {
            return Dispatch::continue_with(Response::error(
                request.id,
                "unavailable",
                "transaction submission worker could not be started",
            ));
        }

        let status_service = self.application.get_wallet_transfer_submission_status();
        let query = WalletTransferSubmissionQuery {
            profile_id,
            draft_id: params.draft_id,
        };
        for _ in 0..100 {
            match status_service.execute(query.clone()) {
                Ok(status) if status.state != "not_started" => {
                    return Dispatch::continue_with(Response::success(
                        request.id,
                        json!({ "submissionStatus": transfer_submission_status_value(&status) }),
                    ));
                }
                Ok(_) => thread::sleep(Duration::from_millis(1)),
                Err(error) => {
                    return Dispatch::continue_with(transaction_error(request.id, error));
                }
            }
        }
        Dispatch::continue_with(Response::error(
            request.id,
            "unavailable",
            "transaction submission worker did not start",
        ))
    }

    fn submission_status(&self, request: Request) -> Dispatch {
        self.submission_operation(
            request,
            "wallet.transaction.submission_status",
            |application, query| {
                application
                    .get_wallet_transfer_submission_status()
                    .execute(query)
            },
        )
    }

    fn submission_history(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.transaction.submission_history");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_wallet_transfer_submissions()
            .execute(profile_id)
        {
            Ok(statuses) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "submissions": statuses
                        .iter()
                        .map(transfer_submission_status_value)
                        .collect::<Vec<_>>()
                }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    fn reconcile_submission(&self, request: Request) -> Dispatch {
        self.submission_operation(
            request,
            "wallet.transaction.reconcile_submission",
            |application, query| {
                futures::executor::block_on(
                    application
                        .reconcile_wallet_transfer_submission()
                        .execute(query),
                )
            },
        )
    }

    fn cancel_submission(&self, request: Request) -> Dispatch {
        self.submission_operation(
            request,
            "wallet.transaction.cancel_submission",
            |application, query| {
                application
                    .cancel_wallet_transfer_submission()
                    .execute(query)
            },
        )
    }

    fn submission_operation(
        &self,
        request: Request,
        method: &'static str,
        operation: impl FnOnce(
            &ApplicationServices,
            WalletTransferSubmissionQuery,
        )
            -> Result<WalletTransferSubmissionStatusView, WalletTransactionError>,
    ) -> Dispatch {
        let params = match serde_json::from_value::<TransactionDraftParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                let message = match method {
                    "wallet.transaction.submission_status" => {
                        "wallet.transaction.submission_status requires only a string draftId"
                    }
                    "wallet.transaction.cancel_submission" => {
                        "wallet.transaction.cancel_submission requires only a string draftId"
                    }
                    "wallet.transaction.reconcile_submission" => {
                        "wallet.transaction.reconcile_submission requires only a string draftId"
                    }
                    _ => "transaction submission method requires only a string draftId",
                };
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    message,
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match operation(
            &self.application,
            WalletTransferSubmissionQuery {
                profile_id,
                draft_id: params.draft_id,
            },
        ) {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "submissionStatus": transfer_submission_status_value(&status) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    fn account_projection(
        &self,
        request: Request,
        method: &'static str,
        projection: impl FnOnce(&WalletAccountView) -> Value,
    ) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, method);
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_wallet_account()
            .execute(WalletAccountQuery { profile_id })
        {
            Ok(account) => {
                Dispatch::continue_with(Response::success(request.id, projection(&account)))
            }
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    fn receive_credential(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "credential.receive does not accept parameters",
            ));
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application
                .receive_credential()
                .execute(CredentialProfileQuery { profile_id }),
        ) {
            Ok(credential) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "credential": credential_value(&credential) }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    fn list_credentials(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "credential.list does not accept parameters",
            ));
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_credentials()
            .execute(CredentialProfileQuery { profile_id })
        {
            Ok(credentials) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "credentials": credentials.iter().map(credential_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    fn get_credential(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.get requires only a string credentialId field",
                ));
            }
        };
        self.credential_query(request.id, params.credential_id, false)
    }

    fn reverify_credential(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.reverify requires only a string credentialId field",
                ));
            }
        };
        self.credential_query(request.id, params.credential_id, true)
    }

    fn credential_query(
        &self,
        id: Option<String>,
        credential_id: String,
        reverify: bool,
    ) -> Dispatch {
        let profile_id = match self.active_profile_id(id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let query = CredentialQuery {
            profile_id,
            credential_id,
        };
        let result = if reverify {
            futures::executor::block_on(self.application.reverify_credential().execute(query))
        } else {
            self.application.get_credential().execute(query)
        };
        match result {
            Ok(credential) => Dispatch::continue_with(Response::success(
                id,
                json!({ "credential": credential_value(&credential) }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(id, error)),
        }
    }

    fn delete_credential(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DeleteCredentialParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.delete requires credentialId, confirmed, and intent fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .delete_credential()
            .execute(DeleteCredentialCommand {
                profile_id,
                credential_id: params.credential_id,
                confirmed: params.confirmed,
                intent: params.intent,
            }) {
            Ok(()) => {
                Dispatch::continue_with(Response::success(request.id, json!({ "deleted": true })))
            }
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    fn credential_disclosure_candidates(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.disclosure.candidates requires only a string credentialId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_credential_disclosure()
            .execute(CredentialDisclosureQuery {
                profile_id,
                credential_id: params.credential_id,
            }) {
            Ok(disclosure) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "disclosure": credential_disclosure_value(&disclosure) }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    fn preview_credential_disclosure(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DisclosurePreviewParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.disclosure.preview requires credentialId, revealClaimPaths, and predicates fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.preview_credential_disclosure().execute(
            PreviewCredentialDisclosureCommand {
                profile_id,
                credential_id: params.credential_id,
                reveal_claim_paths: params.reveal_claim_paths,
                predicates: params
                    .predicates
                    .into_iter()
                    .map(|predicate| CredentialPredicateInput {
                        claim_path: predicate.claim_path,
                        kind: predicate.kind,
                        threshold: predicate.threshold,
                    })
                    .collect(),
            },
        ) {
            Ok(plan) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "plan": credential_disclosure_plan_value(&plan) }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    fn prepare_credential_issuance(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<PrepareCredentialIssuanceParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.issuance.prepare requires only a string offer field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(self.application.prepare_credential_issuance().execute(
            PrepareCredentialIssuanceCommand {
                profile_id,
                offer: params.offer,
            },
        )) {
            Ok(issuance) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuance": credential_issuance_value(&issuance) }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    fn accept_credential_issuance(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AcceptCredentialIssuanceParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.issuance.accept requires issuanceId, holderDid, methodId, confirmed, and intent fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(self.application.accept_credential_issuance().execute(
            AcceptCredentialIssuanceCommand {
                profile_id,
                issuance_id: params.issuance_id,
                holder_did: params.holder_did,
                method_id: params.method_id,
                confirmed: params.confirmed,
                intent: params.intent,
            },
        )) {
            Ok(issuance) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuance": credential_issuance_value(&issuance) }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    fn refuse_credential_issuance(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialIssuanceParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.issuance.refuse requires only a string issuanceId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.refuse_credential_issuance().execute(
            RefuseCredentialIssuanceCommand {
                profile_id,
                issuance_id: params.issuance_id,
            },
        ) {
            Ok(issuance) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuance": credential_issuance_value(&issuance) }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    fn get_credential_issuance(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialIssuanceParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.issuance.get requires only a string issuanceId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_credential_issuance()
            .execute(CredentialIssuanceQuery {
                profile_id,
                issuance_id: params.issuance_id,
            }) {
            Ok(issuance) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuance": credential_issuance_value(&issuance) }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    fn list_credential_issuances(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "credential.issuance.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_credential_issuances()
            .execute(CredentialIssuanceProfileQuery { profile_id })
        {
            Ok(issuances) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuances": issuances.iter().map(credential_issuance_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    fn prepare_credential_presentation(&self, request: Request) -> Dispatch {
        let params =
            match serde_json::from_value::<PrepareCredentialPresentationParams>(request.params) {
                Ok(params) => params,
                Err(_) => {
                    return Dispatch::continue_with(Response::error(
                        request.id,
                        "invalid_params",
                        "credential.presentation.prepare requires only a string request field",
                    ));
                }
            };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application.prepare_credential_presentation().execute(
                PrepareCredentialPresentationCommand {
                    profile_id,
                    request: params.request,
                },
            ),
        ) {
            Ok(presentation) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentation": credential_presentation_value(&presentation) }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    fn accept_credential_presentation(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AcceptCredentialPresentationParams>(
            request.params,
        ) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.presentation.accept requires presentationId, credentialId, confirmed, and intent fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application.accept_credential_presentation().execute(
                AcceptCredentialPresentationCommand {
                    profile_id,
                    presentation_id: params.presentation_id,
                    credential_id: params.credential_id,
                    confirmed: params.confirmed,
                    intent: params.intent,
                },
            ),
        ) {
            Ok(presentation) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentation": credential_presentation_value(&presentation) }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    fn refuse_credential_presentation(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialPresentationParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.presentation.refuse requires only a string presentationId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.refuse_credential_presentation().execute(
            RefuseCredentialPresentationCommand {
                profile_id,
                presentation_id: params.presentation_id,
            },
        ) {
            Ok(presentation) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentation": credential_presentation_value(&presentation) }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    fn get_credential_presentation(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialPresentationParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.presentation.get requires only a string presentationId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_credential_presentation()
            .execute(CredentialPresentationQuery {
                profile_id,
                presentation_id: params.presentation_id,
            }) {
            Ok(presentation) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentation": credential_presentation_value(&presentation) }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    fn list_credential_presentations(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "credential.presentation.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_credential_presentations()
            .execute(CredentialPresentationProfileQuery { profile_id })
        {
            Ok(presentations) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentations": presentations.iter().map(credential_presentation_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    fn prepare_self_issued_authentication(&self, request: Request) -> Dispatch {
        let params =
            match serde_json::from_value::<PrepareSelfIssuedAuthenticationParams>(request.params) {
                Ok(params) => params,
                Err(_) => {
                    return Dispatch::continue_with(Response::error(
                        request.id,
                        "invalid_params",
                        "identity.authentication.prepare requires only a string request field",
                    ));
                }
            };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application
                .prepare_self_issued_authentication()
                .execute(PrepareSelfIssuedAuthenticationCommand {
                    profile_id,
                    request: params.request,
                }),
        ) {
            Ok(authentication) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentication": self_issued_authentication_value(&authentication) }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }

    fn accept_self_issued_authentication(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AcceptSelfIssuedAuthenticationParams>(
            request.params,
        ) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "identity.authentication.accept requires authenticationId, holderDid, methodId, confirmed, and intent fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application
                .accept_self_issued_authentication()
                .execute(AcceptSelfIssuedAuthenticationCommand {
                    profile_id,
                    authentication_id: params.authentication_id,
                    holder_did: params.holder_did,
                    method_id: params.method_id,
                    confirmed: params.confirmed,
                    intent: params.intent,
                }),
        ) {
            Ok(authentication) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentication": self_issued_authentication_value(&authentication) }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }

    fn refuse_self_issued_authentication(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SelfIssuedAuthenticationParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "identity.authentication.refuse requires only a string authenticationId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .refuse_self_issued_authentication()
            .execute(RefuseSelfIssuedAuthenticationCommand {
                profile_id,
                authentication_id: params.authentication_id,
            }) {
            Ok(authentication) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentication": self_issued_authentication_value(&authentication) }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }

    fn get_self_issued_authentication(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SelfIssuedAuthenticationParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "identity.authentication.get requires only a string authenticationId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.get_self_issued_authentication().execute(
            SelfIssuedAuthenticationQuery {
                profile_id,
                authentication_id: params.authentication_id,
            },
        ) {
            Ok(authentication) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentication": self_issued_authentication_value(&authentication) }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }

    fn list_self_issued_authentications(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "identity.authentication.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_self_issued_authentications()
            .execute(SelfIssuedAuthenticationProfileQuery { profile_id })
        {
            Ok(authentications) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentications": authentications.iter().map(self_issued_authentication_value).collect::<Vec<_>>() }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }

    fn active_profile_id(&self, id: Option<String>) -> Result<String, Response> {
        match self.application.get_active_wallet_profile().execute() {
            Ok(Some(profile)) => Ok(profile.id),
            Ok(None) => Err(Response::error(
                id,
                "failed_precondition",
                "an active wallet profile is required",
            )),
            Err(error) => Err(read_profiles_error(id, error)),
        }
    }
}

#[derive(Deserialize)]
struct Request {
    protocol: String,
    #[serde(default)]
    id: Option<String>,
    method: String,
    #[serde(default = "empty_params")]
    params: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateProfileParams {
    display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectProfileParams {
    profile_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectNetworkParams {
    network_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeriveAccountParams {
    #[serde(default)]
    account_index: u32,
    #[serde(default)]
    address_index: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PrepareTransferParams {
    recipient_address: String,
    amount_atomic_units: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthorizeTransferParams {
    draft_id: String,
    authorization_challenge: String,
    confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransactionDraftParams {
    draft_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmitTransferParams {
    draft_id: String,
    confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerateKeyParams {
    label: String,
    algorithm: String,
    purpose: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfirmationParams {
    title: String,
    summary: String,
    confirmed: bool,
}

impl From<ConfirmationParams> for SensitiveOperationConfirmation {
    fn from(value: ConfirmationParams) -> Self {
        Self {
            title: value.title,
            summary: value.summary,
            confirmed: value.confirmed,
        }
    }
}

impl From<ConfirmationParams> for DidOperationConfirmation {
    fn from(value: ConfirmationParams) -> Self {
        Self {
            title: value.title,
            summary: value.summary,
            confirmed: value.confirmed,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignParams {
    #[serde(rename = "keyRef")]
    key_reference: String,
    payload_hex: String,
    confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteKeyParams {
    #[serde(rename = "keyRef")]
    key_reference: String,
    confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DidParams {
    did: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CredentialParams {
    credential_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteCredentialParams {
    credential_id: String,
    confirmed: bool,
    intent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DisclosurePreviewParams {
    credential_id: String,
    reveal_claim_paths: Vec<String>,
    predicates: Vec<DisclosurePredicateParams>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DisclosurePredicateParams {
    claim_path: String,
    kind: String,
    threshold: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareCredentialIssuanceParams {
    offer: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CredentialIssuanceParams {
    issuance_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcceptCredentialIssuanceParams {
    issuance_id: String,
    holder_did: String,
    method_id: String,
    confirmed: bool,
    intent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareCredentialPresentationParams {
    request: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CredentialPresentationParams {
    presentation_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcceptCredentialPresentationParams {
    presentation_id: String,
    credential_id: String,
    confirmed: bool,
    intent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareSelfIssuedAuthenticationParams {
    request: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelfIssuedAuthenticationParams {
    authentication_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcceptSelfIssuedAuthenticationParams {
    authentication_id: String,
    holder_did: String,
    method_id: String,
    confirmed: bool,
    intent: String,
}

fn undeployed_network() -> String {
    "undeployed".to_owned()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateDidParams {
    #[serde(default = "undeployed_network")]
    network: String,
}

#[derive(Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum DidUpdateParams {
    AddAlsoKnownAs {
        did: String,
        value: String,
        confirmation: ConfirmationParams,
    },
    RemoveAlsoKnownAs {
        did: String,
        value: String,
        confirmation: ConfirmationParams,
    },
    AddVerificationMethod {
        did: String,
        fragment: String,
        algorithm: String,
        confirmation: ConfirmationParams,
    },
    UpdateVerificationMethod {
        did: String,
        method_id: String,
        algorithm: String,
        confirmation: ConfirmationParams,
    },
    RemoveVerificationMethod {
        did: String,
        method_id: String,
        confirmation: ConfirmationParams,
    },
    AddVerificationRelationship {
        did: String,
        relationship: String,
        method_id: String,
        confirmation: ConfirmationParams,
    },
    RemoveVerificationRelationship {
        did: String,
        relationship: String,
        method_id: String,
        confirmation: ConfirmationParams,
    },
    AddService {
        did: String,
        id: String,
        service_type: String,
        endpoint: String,
        confirmation: ConfirmationParams,
    },
    UpdateService {
        did: String,
        id: String,
        service_type: String,
        endpoint: String,
        confirmation: ConfirmationParams,
    },
    RemoveService {
        did: String,
        id: String,
        confirmation: ConfirmationParams,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignDidParams {
    did: String,
    method_id: String,
    payload_hex: String,
    confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeactivateDidParams {
    did: String,
    confirmation: ConfirmationParams,
}

fn did_update(params: DidUpdateParams) -> Option<(String, DidUpdate, DidOperationConfirmation)> {
    let value = match params {
        DidUpdateParams::AddAlsoKnownAs {
            did,
            value,
            confirmation,
        } => (
            did,
            DidUpdate::AddAlsoKnownAs { value },
            confirmation.into(),
        ),
        DidUpdateParams::RemoveAlsoKnownAs {
            did,
            value,
            confirmation,
        } => (
            did,
            DidUpdate::RemoveAlsoKnownAs { value },
            confirmation.into(),
        ),
        DidUpdateParams::AddVerificationMethod {
            did,
            fragment,
            algorithm,
            confirmation,
        } => (
            did,
            DidUpdate::AddVerificationMethod {
                fragment,
                algorithm: did_key_algorithm(&algorithm)?,
            },
            confirmation.into(),
        ),
        DidUpdateParams::UpdateVerificationMethod {
            did,
            method_id,
            algorithm,
            confirmation,
        } => (
            did,
            DidUpdate::UpdateVerificationMethod {
                method_id,
                algorithm: did_key_algorithm(&algorithm)?,
            },
            confirmation.into(),
        ),
        DidUpdateParams::RemoveVerificationMethod {
            did,
            method_id,
            confirmation,
        } => (
            did,
            DidUpdate::RemoveVerificationMethod { method_id },
            confirmation.into(),
        ),
        DidUpdateParams::AddVerificationRelationship {
            did,
            relationship,
            method_id,
            confirmation,
        } => (
            did,
            DidUpdate::AddVerificationRelationship {
                relationship: VerificationRelationship::parse(&relationship)?,
                method_id,
            },
            confirmation.into(),
        ),
        DidUpdateParams::RemoveVerificationRelationship {
            did,
            relationship,
            method_id,
            confirmation,
        } => (
            did,
            DidUpdate::RemoveVerificationRelationship {
                relationship: VerificationRelationship::parse(&relationship)?,
                method_id,
            },
            confirmation.into(),
        ),
        DidUpdateParams::AddService {
            did,
            id,
            service_type,
            endpoint,
            confirmation,
        } => (
            did,
            DidUpdate::AddService {
                id,
                service_type,
                endpoint,
            },
            confirmation.into(),
        ),
        DidUpdateParams::UpdateService {
            did,
            id,
            service_type,
            endpoint,
            confirmation,
        } => (
            did,
            DidUpdate::UpdateService {
                id,
                service_type,
                endpoint,
            },
            confirmation.into(),
        ),
        DidUpdateParams::RemoveService {
            did,
            id,
            confirmation,
        } => (did, DidUpdate::RemoveService { id }, confirmation.into()),
    };
    Some(value)
}

fn did_key_algorithm(value: &str) -> Option<DidKeyAlgorithm> {
    match value {
        "ed25519" => Some(DidKeyAlgorithm::Ed25519),
        "p256" => Some(DidKeyAlgorithm::P256),
        _ => None,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    protocol: &'static str,
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorBody>,
}

impl Response {
    fn success(id: Option<String>, result: Value) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<String>, code: &'static str, message: &'static str) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id,
            ok: false,
            result: None,
            error: Some(ErrorBody { code, message }),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
}

struct Dispatch {
    response: Response,
    should_exit: bool,
}

impl Dispatch {
    const fn continue_with(response: Response) -> Self {
        Self {
            response,
            should_exit: false,
        }
    }

    const fn exit(response: Response) -> Self {
        Self {
            response,
            should_exit: true,
        }
    }
}

fn request_id(value: &Value) -> Result<Option<String>, &'static str> {
    let Some(id) = value.get("id") else {
        return Ok(None);
    };
    let Some(id) = id.as_str() else {
        return Err("id must be a string when present");
    };
    let character_count = id.chars().count();
    if character_count == 0 || character_count > MAX_REQUEST_ID_CHARACTERS {
        return Err("id must contain between 1 and 128 characters");
    }

    Ok(Some(id.to_owned()))
}

fn empty_params() -> Value {
    json!({})
}

fn params_are_empty(params: &Value) -> bool {
    params.as_object().is_some_and(serde_json::Map::is_empty)
}

fn profile_error(id: Option<String>, error: CreateWalletProfileError) -> Response {
    match error {
        CreateWalletProfileError::InvalidName(_) => Response::error(
            id,
            "invalid_argument",
            "displayName must be non-empty, contain no control characters, and be at most 64 characters",
        ),
        CreateWalletProfileError::Persistence(WalletProfileRepositoryError::Conflict) => {
            Response::error(id, "conflict", "wallet profile already exists")
        }
        CreateWalletProfileError::Persistence(WalletProfileRepositoryError::NotFound) => {
            Response::error(id, "internal_error", "wallet profile could not be created")
        }
        CreateWalletProfileError::Persistence(WalletProfileRepositoryError::Unavailable) => {
            Response::error(
                id,
                "storage_unavailable",
                "wallet profile storage is unavailable",
            )
        }
        CreateWalletProfileError::Platform(_) => Response::error(
            id,
            "platform_unavailable",
            "required platform service is unavailable",
        ),
        CreateWalletProfileError::InvalidGeneratedIdentifier => {
            Response::error(id, "internal_error", "wallet profile could not be created")
        }
    }
}

fn read_profiles_error(id: Option<String>, error: ReadWalletProfilesError) -> Response {
    match error {
        ReadWalletProfilesError::Persistence(WalletProfileRepositoryError::Unavailable) => {
            Response::error(
                id,
                "storage_unavailable",
                "wallet profile storage is unavailable",
            )
        }
        ReadWalletProfilesError::Persistence(
            WalletProfileRepositoryError::Conflict | WalletProfileRepositoryError::NotFound,
        ) => Response::error(id, "internal_error", "wallet profiles could not be loaded"),
    }
}

fn select_profile_error(id: Option<String>, error: SelectWalletProfileError) -> Response {
    match error {
        SelectWalletProfileError::InvalidIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "profileId must be a valid Oxid profile identifier",
        ),
        SelectWalletProfileError::Persistence(WalletProfileRepositoryError::NotFound) => {
            Response::error(id, "not_found", "wallet profile was not found")
        }
        SelectWalletProfileError::Persistence(WalletProfileRepositoryError::Unavailable) => {
            Response::error(
                id,
                "storage_unavailable",
                "wallet profile storage is unavailable",
            )
        }
        SelectWalletProfileError::Persistence(WalletProfileRepositoryError::Conflict) => {
            Response::error(id, "internal_error", "wallet profile could not be selected")
        }
    }
}

fn profile_value(profile: &WalletProfileView) -> Value {
    json!({
        "id": profile.id,
        "displayName": profile.display_name,
        "createdAtMillis": profile.created_at_millis
    })
}

fn network_list_value(networks: &WalletNetworkListView) -> Value {
    json!({
        "selectedNetworkId": networks.selected_network_id,
        "networks": networks.networks.iter().map(|network| json!({
            "chain": network.chain,
            "networkId": network.network_id,
            "displayName": network.display_name,
            "environment": network.environment,
            "selected": network.selected
        })).collect::<Vec<_>>()
    })
}

fn account_value(account: &WalletAccountView) -> Value {
    json!({
        "chain": account.chain,
        "networkId": account.network_id,
        "networkName": account.network_name,
        "networkEnvironment": account.network_environment,
        "accountId": account.account_id,
        "source": account.source,
        "addresses": account.addresses.iter().map(address_value).collect::<Vec<_>>(),
        "balances": account.balances.iter().map(balance_value).collect::<Vec<_>>(),
        "sync": sync_value(account),
        "transactions": account.transactions.iter().map(transaction_value).collect::<Vec<_>>()
    })
}

fn derived_account_value(account: &DerivedWalletAccountView) -> Value {
    json!({
        "networkId": account.network_id,
        "accountId": account.account_id,
        "accountIndex": account.account_index,
        "addressIndex": account.address_index,
        "receiveAddress": address_value(&account.receive_address),
        "addresses": account.addresses.iter().map(address_value).collect::<Vec<_>>(),
        "transactionKeyRef": account.transaction_key_reference,
        "custodyMode": "development_only"
    })
}

fn address_value(address: &oxid_wallet_application::WalletAddressView) -> Value {
    json!({ "kind": address.kind, "value": address.value })
}

fn balance_value(balance: &oxid_wallet_application::WalletAssetBalanceView) -> Value {
    json!({
        "assetId": balance.asset_id,
        "symbol": balance.symbol,
        "decimals": balance.decimals,
        "atomicUnits": balance.atomic_units
    })
}

fn sync_value(account: &WalletAccountView) -> Value {
    json!({
        "state": account.sync.state,
        "currentCursor": account.sync.current_cursor,
        "targetCursor": account.sync.target_cursor,
        "chainTipHeight": account.sync.chain_tip_height,
        "updatedAtMillis": account.sync.updated_at_millis
    })
}

fn dust_sync_value(status: &WalletDustSyncView) -> Value {
    json!({
        "networkId": status.network_id,
        "state": status.state,
        "currentCursor": status.current_cursor,
        "targetCursor": status.target_cursor,
        "eventsProcessed": status.events_processed,
        "balance": {
            "assetId": "midnight:dust",
            "symbol": "DUST",
            "decimals": 15,
            "atomicUnits": status.balance_atomic_units
        },
        "updatedAtMillis": status.updated_at_millis,
        "failure": status.failure
    })
}

fn shielded_sync_value(status: &WalletShieldedSyncView) -> Value {
    json!({
        "networkId": status.network_id,
        "state": status.state,
        "currentCursor": status.current_cursor,
        "targetCursor": status.target_cursor,
        "eventsProcessed": status.events_processed,
        "ownedNoteCount": status.owned_note_count,
        "commitmentCount": status.commitment_count,
        "balances": status.balances.iter().map(|balance| json!({
            "tokenType": balance.token_type_hex,
            "atomicUnits": balance.atomic_units
        })).collect::<Vec<_>>(),
        "updatedAtMillis": status.updated_at_millis,
        "failure": status.failure
    })
}

fn transaction_value(transaction: &oxid_wallet_application::WalletTransactionView) -> Value {
    json!({
        "transactionId": transaction.transaction_id,
        "direction": transaction.direction,
        "status": transaction.status,
        "blockHeight": transaction.block_height,
        "observedAtMillis": transaction.observed_at_millis,
        "changes": transaction.changes.iter().map(|change| json!({
            "direction": change.direction,
            "balance": balance_value(&change.balance)
        })).collect::<Vec<_>>(),
        "fee": transaction.fee.as_ref().map(balance_value)
    })
}

fn transfer_preview_value(preview: &WalletTransferPreviewView) -> Value {
    json!({
        "draftId": preview.draft_id,
        "authorizationChallenge": preview.authorization_challenge,
        "networkId": preview.network_id,
        "accountId": preview.account_id,
        "recipientAddress": preview.recipient_address,
        "amount": transfer_asset_value(&preview.amount),
        "change": transfer_asset_value(&preview.change),
        "fee": preview.fee.as_ref().map(transfer_asset_value),
        "feeState": preview.fee_state,
        "inputCount": preview.input_count,
        "expiresAtMillis": preview.expires_at_millis,
        "state": preview.state,
        "proofRequired": preview.proof_required,
        "submissionReady": preview.submission_ready,
        "custodyMode": "development_only"
    })
}

fn transfer_submission_value(submission: &WalletTransferSubmissionView) -> Value {
    json!({
        "transfer": transfer_preview_value(&submission.transfer),
        "transactionId": submission.transaction_id,
        "blockId": submission.block_id,
        "fee": transfer_asset_value(&submission.fee),
        "mode": submission.mode,
        "custodyMode": "development_only"
    })
}

fn transfer_submission_status_value(status: &WalletTransferSubmissionStatusView) -> Value {
    json!({
        "draftId": status.draft_id,
        "state": status.state,
        "cancellationAllowed": status.cancellation_allowed,
        "retryable": status.retryable,
        "replacementAllowed": status.replacement_allowed,
        "reconciliationAllowed": status.reconciliation_allowed,
        "transactionId": status.transaction_id,
        "blockId": status.block_id,
        "fee": status.fee.as_ref().map(transfer_asset_value),
        "mode": status.mode,
        "custodyMode": "development_only"
    })
}

fn transfer_asset_value(asset: &oxid_wallet_application::WalletTransferAssetView) -> Value {
    json!({
        "assetId": asset.asset_id,
        "symbol": asset.symbol,
        "decimals": asset.decimals,
        "atomicUnits": asset.atomic_units,
    })
}

fn did_record_value(record: &DidRecordView) -> Value {
    let document = &record.document;
    json!({
        "document": {
            "contexts": document.contexts,
            "id": document.id,
            "network": document.network,
            "alsoKnownAs": document.also_known_as,
            "verificationMethods": document.verification_methods.iter().map(|method| json!({
                "id": method.id,
                "controller": method.controller,
                "publicKeyJwk": {
                    "kty": method.public_key_jwk.key_type,
                    "crv": method.public_key_jwk.curve,
                    "x": method.public_key_jwk.x,
                    "y": method.public_key_jwk.y,
                }
            })).collect::<Vec<_>>(),
            "relationships": document.relationships.iter().map(|relationship| json!({
                "relationship": relationship.relationship,
                "methodIds": relationship.method_ids,
            })).collect::<Vec<_>>(),
            "services": document.services.iter().map(|service| json!({
                "id": service.id,
                "types": service.types,
                "endpoints": service.endpoints.iter().map(|endpoint| json!({
                    "value": endpoint.value,
                    "jsonObject": endpoint.is_json_object,
                })).collect::<Vec<_>>(),
                "endpointWasArray": service.endpoint_was_array,
            })).collect::<Vec<_>>(),
        },
        "documentMetadata": {
            "created": record.document_metadata.created,
            "updated": record.document_metadata.updated,
            "deactivated": record.document_metadata.deactivated,
            "versionId": record.document_metadata.version_id,
            "nextUpdate": record.document_metadata.next_update,
            "nextVersionId": record.document_metadata.next_version_id,
            "equivalentIds": record.document_metadata.equivalent_ids,
            "canonicalId": record.document_metadata.canonical_id,
        },
        "contentType": record.content_type,
        "source": record.source,
    })
}

fn credential_value(credential: &CredentialView) -> Value {
    json!({
        "id": credential.id,
        "displayName": credential.display_name,
        "issuerDid": credential.issuer_did,
        "subjectDid": credential.subject_did,
        "format": credential.format,
        "issuedAtMs": credential.issued_at_ms,
        "verification": {
            "outcome": credential.verification_outcome,
            "stages": credential.verification_stages.iter().map(|stage| json!({
                "name": stage.name,
                "status": stage.status,
                "reasonCode": stage.reason_code,
            })).collect::<Vec<_>>(),
        },
    })
}

fn credential_disclosure_value(disclosure: &CredentialDisclosureView) -> Value {
    json!({
        "credentialId": disclosure.credential_id,
        "schemaId": disclosure.schema_id,
        "candidates": disclosure.candidates.iter().map(|candidate| json!({
            "claimPath": candidate.claim_path,
            "label": candidate.label,
            "privacyTier": candidate.privacy_tier,
        })).collect::<Vec<_>>(),
    })
}

fn credential_disclosure_plan_value(plan: &CredentialDisclosurePlanView) -> Value {
    json!({
        "credentialId": plan.credential_id,
        "schemaId": plan.schema_id,
        "reveals": plan.reveals.iter().map(|candidate| json!({
            "claimPath": candidate.claim_path,
            "label": candidate.label,
            "privacyTier": candidate.privacy_tier,
        })).collect::<Vec<_>>(),
        "predicates": plan.predicates.iter().map(|predicate| json!({
            "claimPath": predicate.claim_path,
            "label": predicate.label,
            "kind": predicate.kind,
            "threshold": predicate.threshold,
        })).collect::<Vec<_>>(),
        "outcome": plan.outcome,
        "presentationGenerated": plan.presentation_generated,
    })
}

fn credential_issuance_value(issuance: &CredentialIssuanceView) -> Value {
    json!({
        "id": issuance.id,
        "issuer": issuance.issuer,
        "configurationIds": issuance.configuration_ids,
        "displayNames": issuance.display_names,
        "state": issuance.state,
        "credentialId": issuance.credential_id,
        "failureCode": issuance.failure_code,
    })
}

fn credential_issuance_error(id: Option<String>, error: CredentialIssuanceError) -> Response {
    let (code, message) = match error {
        CredentialIssuanceError::InvalidProfileIdentifier(_)
        | CredentialIssuanceError::InvalidIssuanceIdentifier(_)
        | CredentialIssuanceError::InvalidOffer
        | CredentialIssuanceError::InvalidHolder => (
            "invalid_argument",
            "credential issuance request contains invalid input",
        ),
        CredentialIssuanceError::ConfirmationRequired
        | CredentialIssuanceError::InvalidConfirmation => (
            "confirmation_required",
            "valid explicit credential issuance consent is required",
        ),
        CredentialIssuanceError::NotFound => (
            "not_found",
            "credential issuance session was not found for the active profile",
        ),
        CredentialIssuanceError::InvalidState => (
            "failed_precondition",
            "credential issuance session is not awaiting this operation",
        ),
        CredentialIssuanceError::Protocol(protocol) => (
            protocol.code(),
            "credential issuer protocol rejected or could not complete the request",
        ),
        CredentialIssuanceError::Sink(_) => (
            "credential_store_failed",
            "issued credential could not be verified and stored",
        ),
        CredentialIssuanceError::Unavailable => (
            "capability_unavailable",
            "credential issuance capability is unavailable",
        ),
    };
    Response::error(id, code, message)
}

fn credential_presentation_value(presentation: &CredentialPresentationView) -> Value {
    json!({
        "id": presentation.id,
        "verifier": presentation.verifier,
        "purpose": presentation.purpose,
        "queryId": presentation.query_id,
        "candidates": presentation.candidates.iter().map(|candidate| json!({
            "credentialId": candidate.credential_id,
            "displayName": candidate.display_name,
        })).collect::<Vec<_>>(),
        "requestedClaims": presentation.requested_claims.iter().map(|claim| json!({
            "claimPath": claim.claim_path,
            "label": claim.label,
            "intent": claim.intent,
            "predicateKind": claim.predicate_kind,
            "threshold": claim.threshold,
        })).collect::<Vec<_>>(),
        "state": presentation.state,
        "presentationGenerated": presentation.presentation_generated,
        "verifierValidated": presentation.verifier_validated,
        "failureCode": presentation.failure_code,
    })
}

fn credential_presentation_error(
    id: Option<String>,
    error: CredentialPresentationError,
) -> Response {
    let (code, message) = match error {
        CredentialPresentationError::InvalidProfileIdentifier(_)
        | CredentialPresentationError::InvalidPresentationIdentifier(_)
        | CredentialPresentationError::InvalidRequest
        | CredentialPresentationError::InvalidCredential => (
            "invalid_argument",
            "credential presentation request contains invalid input",
        ),
        CredentialPresentationError::ConfirmationRequired
        | CredentialPresentationError::InvalidConfirmation => (
            "confirmation_required",
            "valid explicit credential presentation consent is required",
        ),
        CredentialPresentationError::NotFound => (
            "not_found",
            "credential presentation session was not found for the active profile",
        ),
        CredentialPresentationError::InvalidState => (
            "failed_precondition",
            "credential presentation session is not awaiting this operation",
        ),
        CredentialPresentationError::Protocol(protocol) => (
            protocol.code(),
            "credential presentation protocol rejected or could not complete the request",
        ),
        CredentialPresentationError::Unavailable => (
            "capability_unavailable",
            "credential presentation capability is unavailable",
        ),
    };
    Response::error(id, code, message)
}

fn self_issued_authentication_value(authentication: &SelfIssuedAuthenticationView) -> Value {
    json!({
        "id": authentication.id,
        "verifier": authentication.verifier,
        "purpose": authentication.purpose,
        "state": authentication.state,
        "failureCode": authentication.failure_code,
    })
}

fn self_issued_authentication_error(
    id: Option<String>,
    error: SelfIssuedAuthenticationError,
) -> Response {
    let (code, message) = match error {
        SelfIssuedAuthenticationError::InvalidProfileIdentifier(_)
        | SelfIssuedAuthenticationError::InvalidAuthenticationIdentifier(_)
        | SelfIssuedAuthenticationError::InvalidRequest
        | SelfIssuedAuthenticationError::InvalidHolder => (
            "invalid_argument",
            "self-issued authentication request contains invalid input",
        ),
        SelfIssuedAuthenticationError::ConfirmationRequired
        | SelfIssuedAuthenticationError::InvalidConfirmation => (
            "confirmation_required",
            "valid explicit DID authentication consent is required",
        ),
        SelfIssuedAuthenticationError::NotFound => (
            "not_found",
            "self-issued authentication session was not found for the active profile",
        ),
        SelfIssuedAuthenticationError::InvalidState => (
            "failed_precondition",
            "self-issued authentication session is not awaiting this operation",
        ),
        SelfIssuedAuthenticationError::Protocol(protocol) => (
            protocol.code(),
            "self-issued authentication protocol rejected or could not complete the request",
        ),
        SelfIssuedAuthenticationError::Unavailable => (
            "capability_unavailable",
            "self-issued authentication capability is unavailable",
        ),
    };
    Response::error(id, code, message)
}

fn credential_error(id: Option<String>, error: CredentialOperationError) -> Response {
    match error {
        CredentialOperationError::InvalidProfileIdentifier(_)
        | CredentialOperationError::InvalidCredentialIdentifier(_)
        | CredentialOperationError::Domain(_) => Response::error(
            id,
            "invalid_argument",
            "credential request contains invalid identifiers or metadata",
        ),
        CredentialOperationError::ConfirmationRequired
        | CredentialOperationError::InvalidConfirmation => Response::error(
            id,
            "confirmation_required",
            "valid explicit credential deletion confirmation is required",
        ),
        CredentialOperationError::VerificationNotValid => Response::error(
            id,
            "credential_verification_failed",
            "credential verification did not produce a valid outcome",
        ),
        CredentialOperationError::Ingress(_) => Response::error(
            id,
            "capability_unavailable",
            "credential ingress capability is unavailable",
        ),
        CredentialOperationError::Verification(error) => match error {
            CredentialVerificationError::Unavailable => Response::error(
                id,
                "capability_unavailable",
                "credential verification capability is unavailable",
            ),
            CredentialVerificationError::UnsupportedFormat => {
                Response::error(id, "unsupported_format", "credential format is unsupported")
            }
            CredentialVerificationError::InvalidCredential => Response::error(
                id,
                "invalid_credential",
                "credential structure or proof encoding is invalid",
            ),
        },
        CredentialOperationError::Disclosure(error) => match error {
            CredentialDisclosurePortError::Unavailable => Response::error(
                id,
                "capability_unavailable",
                "credential disclosure capability is unavailable",
            ),
            CredentialDisclosurePortError::UnsupportedCredential => Response::error(
                id,
                "unsupported_format",
                "credential schema does not support disclosure preview",
            ),
            CredentialDisclosurePortError::MissingPrivateMaterial => Response::error(
                id,
                "failed_precondition",
                "credential has no protected claim material",
            ),
            CredentialDisclosurePortError::InvalidPrivateMaterial => Response::error(
                id,
                "invalid_credential",
                "credential protected claim material is invalid",
            ),
            CredentialDisclosurePortError::ClaimNotFound
            | CredentialDisclosurePortError::ClaimNotRevealable => Response::error(
                id,
                "invalid_argument",
                "credential disclosure selection is invalid",
            ),
        },
        CredentialOperationError::Persistence(error) => match error {
            CredentialRepositoryError::NotFound => {
                Response::error(id, "not_found", "credential was not found")
            }
            CredentialRepositoryError::CapacityExceeded => {
                Response::error(id, "capacity_exceeded", "credential capacity was exceeded")
            }
            CredentialRepositoryError::Integrity => Response::error(
                id,
                "integrity_error",
                "credential storage failed integrity validation",
            ),
            CredentialRepositoryError::Unavailable => Response::error(
                id,
                "capability_unavailable",
                "credential storage is unavailable",
            ),
        },
    }
}

fn did_error(id: Option<String>, error: DidOperationError) -> Response {
    match error {
        DidOperationError::InvalidProfileIdentifier(_) | DidOperationError::InvalidDid(_) => {
            Response::error(
                id,
                "invalid_argument",
                "active profile or Midnight DID is invalid",
            )
        }
        DidOperationError::SubjectMismatch => Response::error(
            id,
            "invalid_response",
            "resolved DID document does not match the requested subject",
        ),
        DidOperationError::InvalidNetwork => Response::error(
            id,
            "unsupported_network",
            "Midnight DID network is unsupported",
        ),
        DidOperationError::EmptyPayload | DidOperationError::PayloadTooLarge => {
            Response::error(id, "invalid_argument", "DID signing payload is invalid")
        }
        DidOperationError::ConfirmationRequired | DidOperationError::InvalidConfirmation => {
            Response::error(
                id,
                "confirmation_required",
                "valid explicit confirmation is required",
            )
        }
        DidOperationError::Lifecycle(error) => match error {
            DidLifecyclePortError::Unavailable | DidLifecyclePortError::ProtectionUnavailable => {
                Response::error(
                    id,
                    "capability_unavailable",
                    "DID lifecycle capability is unavailable",
                )
            }
            DidLifecyclePortError::UnsupportedNetwork => Response::error(
                id,
                "unsupported_network",
                "DID network does not support standalone lifecycle operations",
            ),
            DidLifecyclePortError::UnsupportedAlgorithm => Response::error(
                id,
                "unsupported_algorithm",
                "DID key algorithm is unsupported",
            ),
            DidLifecyclePortError::NotManaged => Response::error(
                id,
                "failed_precondition",
                "DID is not managed by the current protected session",
            ),
            DidLifecyclePortError::NotFound => {
                Response::error(id, "not_found", "DID document entry was not found")
            }
            DidLifecyclePortError::Conflict => Response::error(
                id,
                "conflict",
                "DID document update conflicts with current state",
            ),
            DidLifecyclePortError::Deactivated => {
                Response::error(id, "failed_precondition", "DID is deactivated")
            }
            DidLifecyclePortError::Locked => {
                Response::error(id, "wallet_locked", "wallet is locked")
            }
            DidLifecyclePortError::InvalidOperation => {
                Response::error(id, "invalid_argument", "DID lifecycle operation is invalid")
            }
        },
        DidOperationError::Resolution(error) => match error {
            DidResolutionPortError::Unavailable => Response::error(
                id,
                "capability_unavailable",
                "DID resolution capability is unavailable",
            ),
            DidResolutionPortError::NotFound => {
                Response::error(id, "not_found", "DID was not found")
            }
            DidResolutionPortError::InvalidDid => Response::error(
                id,
                "invalid_argument",
                "DID resolver rejected the identifier",
            ),
            DidResolutionPortError::MethodNotSupported => Response::error(
                id,
                "unsupported_method",
                "DID method is not supported by the resolver",
            ),
            DidResolutionPortError::InvalidResponse => Response::error(
                id,
                "invalid_response",
                "DID resolver returned an invalid response",
            ),
            DidResolutionPortError::Rejected => {
                Response::error(id, "resolution_rejected", "DID resolution was rejected")
            }
        },
        DidOperationError::Persistence(error) => match error {
            DidRecordRepositoryError::NotFound => {
                Response::error(id, "not_found", "DID record was not found")
            }
            DidRecordRepositoryError::CapacityExceeded => {
                Response::error(id, "resource_exhausted", "DID record capacity was exceeded")
            }
            DidRecordRepositoryError::Integrity => Response::error(
                id,
                "integrity_error",
                "DID record storage failed integrity validation",
            ),
            DidRecordRepositoryError::Unavailable => Response::error(
                id,
                "storage_unavailable",
                "DID record storage is unavailable",
            ),
        },
    }
}

fn transaction_error(id: Option<String>, error: WalletTransactionError) -> Response {
    match error {
        WalletTransactionError::InvalidProfileIdentifier(_)
        | WalletTransactionError::InvalidDraftIdentifier(_)
        | WalletTransactionError::InvalidAuthorizationChallenge(_)
        | WalletTransactionError::InvalidRecipient(_)
        | WalletTransactionError::InvalidAmount
        | WalletTransactionError::ZeroAmount => Response::error(
            id,
            "invalid_argument",
            "transfer recipient, amount, draft, or authorization challenge is invalid",
        ),
        WalletTransactionError::ConfirmationRequired => Response::error(
            id,
            "confirmation_required",
            "explicit human-readable confirmation is required",
        ),
        WalletTransactionError::InvalidConfirmation => Response::error(
            id,
            "invalid_argument",
            "confirmation title and summary must be non-empty and bounded",
        ),
        WalletTransactionError::Clock(_) => Response::error(
            id,
            "platform_unavailable",
            "required platform clock is unavailable",
        ),
        WalletTransactionError::Operation(error) => transaction_port_error(id, error),
    }
}

fn transaction_port_error(id: Option<String>, error: WalletTransactionPortError) -> Response {
    match error {
        WalletTransactionPortError::Unavailable => Response::error(
            id,
            "capability_unavailable",
            "wallet transaction capability is unavailable",
        ),
        WalletTransactionPortError::ProtectionNotInitialized => Response::error(
            id,
            "failed_precondition",
            "wallet protection is not initialized",
        ),
        WalletTransactionPortError::ProtectionLocked => {
            Response::error(id, "wallet_locked", "wallet is locked")
        }
        WalletTransactionPortError::AccountNotDerived => Response::error(
            id,
            "failed_precondition",
            "a protected wallet account must be derived first",
        ),
        WalletTransactionPortError::AccountNotSynchronized => Response::error(
            id,
            "failed_precondition",
            "wallet account must be synchronized first",
        ),
        WalletTransactionPortError::UnsupportedNetwork => Response::error(
            id,
            "unsupported_network",
            "selected wallet network is not supported",
        ),
        WalletTransactionPortError::InvalidRecipient => {
            Response::error(id, "invalid_argument", "recipient address is invalid")
        }
        WalletTransactionPortError::RecipientNetworkMismatch => Response::error(
            id,
            "invalid_argument",
            "recipient address belongs to another network",
        ),
        WalletTransactionPortError::InsufficientFunds => Response::error(
            id,
            "insufficient_funds",
            "wallet has insufficient unshielded NIGHT",
        ),
        WalletTransactionPortError::DraftNotFound => {
            Response::error(id, "not_found", "transaction draft was not found")
        }
        WalletTransactionPortError::DraftExpired => {
            Response::error(id, "failed_precondition", "transaction draft has expired")
        }
        WalletTransactionPortError::DraftConflict => Response::error(
            id,
            "conflict",
            "transaction draft conflicts with current wallet state",
        ),
        WalletTransactionPortError::SubmissionInProgress => Response::error(
            id,
            "conflict",
            "transaction submission is already in progress",
        ),
        WalletTransactionPortError::SubmissionNotInProgress => Response::error(
            id,
            "failed_precondition",
            "transaction submission is not in progress",
        ),
        WalletTransactionPortError::SubmissionCancelled => Response::error(
            id,
            "submission_cancelled",
            "transaction submission was cancelled before broadcast",
        ),
        WalletTransactionPortError::SubmissionCancellationUnsafe => Response::error(
            id,
            "failed_precondition",
            "transaction submission can no longer be cancelled safely",
        ),
        WalletTransactionPortError::AuthorizationChallengeMismatch => Response::error(
            id,
            "authorization_mismatch",
            "authorization does not match the prepared transfer preview",
        ),
        WalletTransactionPortError::InsufficientDust => Response::error(
            id,
            "insufficient_funds",
            "wallet has insufficient DUST for the transaction fee",
        ),
        WalletTransactionPortError::InvalidChainState => Response::error(
            id,
            "chain_state_unavailable",
            "current Midnight chain state could not be used safely",
        ),
        WalletTransactionPortError::ProvingFailed => {
            Response::error(id, "proving_failed", "transaction proof generation failed")
        }
        WalletTransactionPortError::SubmissionRejected => Response::error(
            id,
            "submission_rejected",
            "Midnight rejected the transaction submission",
        ),
        WalletTransactionPortError::SubmissionOutcomeUnknown => Response::error(
            id,
            "submission_unknown",
            "Midnight transaction submission is still awaiting reconciliation",
        ),
        WalletTransactionPortError::Timeout => {
            Response::error(id, "timeout", "transaction operation timed out")
        }
        WalletTransactionPortError::InvalidData => Response::error(
            id,
            "internal_error",
            "transaction material could not be constructed safely",
        ),
    }
}

fn account_error(id: Option<String>, error: WalletAccountError) -> Response {
    match error {
        WalletAccountError::InvalidProfileIdentifier(_)
        | WalletAccountError::InvalidNetworkIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "profile or network identifier is invalid",
        ),
        WalletAccountError::AccountIndexOutOfBounds
        | WalletAccountError::AddressIndexOutOfBounds => Response::error(
            id,
            "invalid_argument",
            "accountIndex and addressIndex must be less than 2^31",
        ),
        WalletAccountError::Port(WalletAccountPortError::NotFound) => {
            Response::error(id, "not_found", "wallet account was not found")
        }
        WalletAccountError::Port(WalletAccountPortError::UnsupportedNetwork) => Response::error(
            id,
            "unsupported_network",
            "selected wallet network is not supported",
        ),
        WalletAccountError::Port(WalletAccountPortError::ProtectionNotInitialized) => {
            Response::error(
                id,
                "failed_precondition",
                "wallet protection is not initialized",
            )
        }
        WalletAccountError::Port(WalletAccountPortError::ProtectionLocked) => {
            Response::error(id, "wallet_locked", "wallet is locked")
        }
        WalletAccountError::Port(WalletAccountPortError::Unavailable) => Response::error(
            id,
            "capability_unavailable",
            "wallet account capability is unavailable",
        ),
        WalletAccountError::Port(WalletAccountPortError::InvalidData) => Response::error(
            id,
            "internal_error",
            "wallet account state could not be decoded safely",
        ),
    }
}

fn dust_sync_error(id: Option<String>, error: WalletDustSyncError) -> Response {
    match error {
        WalletDustSyncError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        WalletDustSyncError::Port(WalletDustSyncPortError::Conflict) => Response::error(
            id,
            "conflict",
            "DUST synchronization is already running or cannot be cancelled",
        ),
        WalletDustSyncError::Port(WalletDustSyncPortError::UnsupportedNetwork) => Response::error(
            id,
            "unsupported_network",
            "selected wallet network does not support DUST synchronization",
        ),
        WalletDustSyncError::Port(WalletDustSyncPortError::ProtectionNotInitialized) => {
            Response::error(
                id,
                "failed_precondition",
                "wallet protection is not initialized",
            )
        }
        WalletDustSyncError::Port(WalletDustSyncPortError::ProtectionLocked) => {
            Response::error(id, "wallet_locked", "wallet is locked")
        }
        WalletDustSyncError::Port(WalletDustSyncPortError::Unavailable) => Response::error(
            id,
            "capability_unavailable",
            "DUST synchronization is unavailable",
        ),
        WalletDustSyncError::Port(WalletDustSyncPortError::InvalidData) => Response::error(
            id,
            "chain_state_unavailable",
            "DUST synchronization state could not be used safely",
        ),
    }
}

fn shielded_sync_error(id: Option<String>, error: WalletShieldedSyncError) -> Response {
    match error {
        WalletShieldedSyncError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::Conflict) => Response::error(
            id,
            "conflict",
            "shielded synchronization is already running or cannot be cancelled",
        ),
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::UnsupportedNetwork) => {
            Response::error(
                id,
                "unsupported_network",
                "selected wallet network does not support shielded synchronization",
            )
        }
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::ProtectionNotInitialized) => {
            Response::error(
                id,
                "failed_precondition",
                "wallet protection is not initialized",
            )
        }
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::ProtectionLocked) => {
            Response::error(id, "wallet_locked", "wallet is locked")
        }
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::Unavailable) => Response::error(
            id,
            "capability_unavailable",
            "shielded synchronization is unavailable",
        ),
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::InvalidData) => Response::error(
            id,
            "chain_state_unavailable",
            "shielded synchronization state could not be used safely",
        ),
    }
}

fn security_status_value(status: WalletSecurityStatusView) -> Value {
    json!({
        "state": match status.state {
            WalletProtectionState::Uninitialized => "uninitialized",
            WalletProtectionState::Locked => "locked",
            WalletProtectionState::Unlocked => "unlocked",
            WalletProtectionState::Unavailable => "unavailable",
        },
        "protection": match status.protection {
            WalletProtectionClass::DevelopmentOnly => "development_only",
            WalletProtectionClass::OperatingSystem => "operating_system",
            WalletProtectionClass::HardwareBacked => "hardware_backed",
            WalletProtectionClass::Unavailable => "unavailable",
        },
        "userPresenceRequired": status.user_presence_required,
        "portableBackupSupported": status.portable_backup_supported,
    })
}

fn key_value(key: &WalletKeyView) -> Value {
    json!({
        "keyRef": key.key_reference,
        "label": key.label,
        "algorithm": algorithm_name(key.algorithm),
        "purpose": purpose_name(key.purpose),
        "publicKey": {
            "encoding": match key.public_key_encoding {
                PublicKeyEncoding::Ed25519Compressed => "ed25519-compressed",
                PublicKeyEncoding::Sec1Compressed => "sec1-compressed",
                PublicKeyEncoding::Secp256k1XOnly => "secp256k1-x-only",
                PublicKeyEncoding::JubjubCompressed => "jubjub-compressed",
            },
            "bytesHex": encode_hex(&key.public_key_bytes),
        },
        "createdAtMillis": key.created_at_millis,
    })
}

const fn algorithm_name(algorithm: WalletKeyAlgorithm) -> &'static str {
    match algorithm {
        WalletKeyAlgorithm::Ed25519 => "ed25519",
        WalletKeyAlgorithm::P256 => "p256",
        WalletKeyAlgorithm::Secp256k1Schnorr => "secp256k1-schnorr",
        WalletKeyAlgorithm::Jubjub => "jubjub",
    }
}

fn key_algorithm(value: &str) -> Option<WalletKeyAlgorithm> {
    match value {
        "ed25519" => Some(WalletKeyAlgorithm::Ed25519),
        "p256" => Some(WalletKeyAlgorithm::P256),
        "secp256k1-schnorr" => Some(WalletKeyAlgorithm::Secp256k1Schnorr),
        "jubjub" => Some(WalletKeyAlgorithm::Jubjub),
        _ => None,
    }
}

const fn purpose_name(purpose: WalletKeyPurpose) -> &'static str {
    match purpose {
        WalletKeyPurpose::Transaction => "transaction",
        WalletKeyPurpose::Authentication => "authentication",
        WalletKeyPurpose::Assertion => "assertion",
        WalletKeyPurpose::KeyAgreement => "key_agreement",
        WalletKeyPurpose::Recovery => "recovery",
    }
}

fn key_purpose(value: &str) -> Option<WalletKeyPurpose> {
    match value {
        "transaction" => Some(WalletKeyPurpose::Transaction),
        "authentication" => Some(WalletKeyPurpose::Authentication),
        "assertion" => Some(WalletKeyPurpose::Assertion),
        "key_agreement" => Some(WalletKeyPurpose::KeyAgreement),
        "recovery" => Some(WalletKeyPurpose::Recovery),
        _ => None,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.is_empty()
        || value.len() > oxid_wallet_application::MAX_SIGNING_PAYLOAD_BYTES * 2
        || !value.len().is_multiple_of(2)
        || !value.is_ascii()
    {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn invalid_empty_params(id: Option<String>, method: &'static str) -> Dispatch {
    let message = match method {
        "wallet.security.status" => "wallet.security.status does not accept parameters",
        "wallet.security.initialize" => "wallet.security.initialize does not accept parameters",
        "wallet.security.unlock" => "wallet.security.unlock does not accept parameters",
        "wallet.security.lock" => "wallet.security.lock does not accept parameters",
        "wallet.key.list" => "wallet.key.list does not accept parameters",
        "wallet.network.list" => "wallet.network.list does not accept parameters",
        "wallet.account.get" => "wallet.account.get does not accept parameters",
        "wallet.address.list" => "wallet.address.list does not accept parameters",
        "wallet.address.unshielded" => "wallet.address.unshielded does not accept parameters",
        "wallet.address.shielded" => "wallet.address.shielded does not accept parameters",
        "wallet.balance.snapshot" => "wallet.balance.snapshot does not accept parameters",
        "wallet.transaction.history" => "wallet.transaction.history does not accept parameters",
        "wallet.transaction.submission_history" => {
            "wallet.transaction.submission_history does not accept parameters"
        }
        "wallet.connect" => "wallet.connect does not accept parameters",
        "wallet.sync.force" => "wallet.sync.force does not accept parameters",
        "wallet.dust.sync.status" => "wallet.dust.sync.status does not accept parameters",
        "wallet.dust.sync.start" => "wallet.dust.sync.start does not accept parameters",
        "wallet.dust.sync.cancel" => "wallet.dust.sync.cancel does not accept parameters",
        "wallet.shielded.sync.status" => "wallet.shielded.sync.status does not accept parameters",
        "wallet.shielded.sync.start" => "wallet.shielded.sync.start does not accept parameters",
        "wallet.shielded.sync.cancel" => "wallet.shielded.sync.cancel does not accept parameters",
        _ => "method does not accept parameters",
    };
    Dispatch::continue_with(Response::error(id, "invalid_params", message))
}

fn security_error(id: Option<String>, error: WalletSecurityError) -> Response {
    match error {
        WalletSecurityError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        WalletSecurityError::Operation(error) => security_port_error(id, error),
    }
}

fn key_error(id: Option<String>, error: WalletKeyError) -> Response {
    match error {
        WalletKeyError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        WalletKeyError::InvalidKeyReference(_) => {
            Response::error(id, "invalid_argument", "keyRef is invalid")
        }
        WalletKeyError::InvalidLabel(_) => Response::error(
            id,
            "invalid_argument",
            "key label must be non-empty, bounded, and contain no control characters",
        ),
        WalletKeyError::Operation(error) => security_port_error(id, error),
    }
}

fn sensitive_error(id: Option<String>, error: SensitiveWalletOperationError) -> Response {
    match error {
        SensitiveWalletOperationError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        SensitiveWalletOperationError::InvalidKeyReference(_) => {
            Response::error(id, "invalid_argument", "keyRef is invalid")
        }
        SensitiveWalletOperationError::EmptyPayload => {
            Response::error(id, "invalid_argument", "signing payload must not be empty")
        }
        SensitiveWalletOperationError::PayloadTooLarge => Response::error(
            id,
            "invalid_argument",
            "signing payload exceeds the application limit",
        ),
        SensitiveWalletOperationError::ConfirmationRequired => Response::error(
            id,
            "confirmation_required",
            "explicit human-readable confirmation is required",
        ),
        SensitiveWalletOperationError::InvalidConfirmation => Response::error(
            id,
            "invalid_argument",
            "confirmation title and summary must be non-empty and bounded",
        ),
        SensitiveWalletOperationError::Operation(error) => security_port_error(id, error),
    }
}

fn security_port_error(id: Option<String>, error: WalletSecurityPortError) -> Response {
    match error {
        WalletSecurityPortError::Unavailable => Response::error(
            id,
            "capability_unavailable",
            "wallet protection is unavailable",
        ),
        WalletSecurityPortError::NotInitialized => Response::error(
            id,
            "failed_precondition",
            "wallet protection is not initialized",
        ),
        WalletSecurityPortError::AlreadyInitialized => {
            Response::error(id, "conflict", "wallet protection is already initialized")
        }
        WalletSecurityPortError::Locked => Response::error(id, "wallet_locked", "wallet is locked"),
        WalletSecurityPortError::NotFound => {
            Response::error(id, "not_found", "protected key was not found")
        }
        WalletSecurityPortError::Conflict => {
            Response::error(id, "conflict", "protected key metadata conflicts")
        }
        WalletSecurityPortError::UnsupportedAlgorithm => Response::error(
            id,
            "unsupported_algorithm",
            "key algorithm is not supported by this adapter",
        ),
        WalletSecurityPortError::AuthorizationDenied => Response::error(
            id,
            "authorization_denied",
            "wallet authorization was denied",
        ),
        WalletSecurityPortError::InvalidOperation => Response::error(
            id,
            "internal_error",
            "protected operation could not be completed",
        ),
    }
}

fn capability_manifest() -> Value {
    json!([
        { "method": "system.capabilities", "status": "ready" },
        { "method": "system.quit", "status": "ready" },
        { "method": "wallet.profile.create", "status": "ready" },
        { "method": "wallet.profile.list", "status": "ready" },
        { "method": "wallet.profile.select", "status": "ready" },
        { "method": "wallet.profile.active", "status": "ready" },
        { "method": "wallet.security.status", "status": "ready", "mode": "development_only" },
        { "method": "wallet.security.initialize", "status": "ready", "mode": "development_only" },
        { "method": "wallet.security.unlock", "status": "ready", "mode": "development_only" },
        { "method": "wallet.security.lock", "status": "ready", "mode": "development_only" },
        { "method": "wallet.key.generate", "status": "ready", "mode": "development_only", "algorithms": ["ed25519", "p256", "secp256k1-schnorr"] },
        { "method": "wallet.key.list", "status": "ready", "mode": "development_only" },
        { "method": "wallet.key.sign", "status": "ready", "mode": "development_only" },
        { "method": "wallet.key.delete", "status": "ready", "mode": "development_only" },
        { "method": "wallet.network.list", "status": "ready", "mode": "standalone" },
        { "method": "wallet.network.select", "status": "ready", "mode": "standalone" },
        { "method": "wallet.account.derive", "status": "ready", "mode": "development_only", "paths": ["midnight-night-external", "midnight-zswap"] },
        { "method": "wallet.account.get", "status": "ready", "mode": "standalone", "sources": ["simulated", "live", "cached"] },
        { "method": "wallet.connect", "status": "ready", "mode": "standalone", "sources": ["simulated", "live"] },
        { "method": "wallet.bootstrap", "status": "queued" },
        { "method": "wallet.address.list", "status": "ready", "mode": "standalone", "sources": ["protected_derivation", "official_public_vectors", "configured_public_address"] },
        { "method": "wallet.address.unshielded", "status": "ready", "mode": "standalone", "sources": ["protected_derivation", "official_public_vectors", "configured_public_address"] },
        { "method": "wallet.address.shielded", "status": "ready", "mode": "standalone", "sources": ["protected_derivation", "official_public_vectors"] },
        { "method": "wallet.balance.snapshot", "status": "ready", "mode": "standalone", "sources": ["simulated", "live", "cached"] },
        { "method": "wallet.transaction.history", "status": "ready", "mode": "standalone", "sources": ["simulated", "live", "cached"] },
        { "method": "wallet.transaction.prepare_unshielded", "status": "ready", "mode": "development_only", "submissionReady": false },
        { "method": "wallet.transaction.authorize_unshielded", "status": "ready", "mode": "development_only", "submissionReady": true },
        { "method": "wallet.transaction.draft", "status": "ready", "mode": "development_only", "submissionReady": "state_dependent" },
        { "method": "wallet.transaction.submit_unshielded", "status": "ready", "mode": "development_only", "sources": ["simulated", "live"] },
        { "method": "wallet.transaction.send_unshielded", "status": "ready", "mode": "development_only", "aliasFor": "wallet.transaction.submit_unshielded" },
        { "method": "wallet.transaction.start_submission", "status": "ready", "mode": "development_only", "execution": "adapter_worker" },
        { "method": "wallet.transaction.submission_status", "status": "ready", "mode": "development_only" },
        { "method": "wallet.transaction.submission_history", "status": "ready", "mode": "standalone", "persistence": "public_metadata_only" },
        { "method": "wallet.transaction.reconcile_submission", "status": "ready", "mode": "standalone", "scope": "finalized_chain" },
        { "method": "wallet.transaction.cancel_submission", "status": "ready", "mode": "development_only", "boundary": "pre_broadcast_only" },
        { "method": "wallet.sync.force", "status": "ready", "mode": "standalone", "sources": ["simulated", "live"] },
        { "method": "wallet.dust.sync.status", "status": "ready", "mode": "standalone", "sources": ["simulated", "live", "cached", "unavailable"] },
        { "method": "wallet.dust.sync.start", "status": "ready", "mode": "standalone", "execution": "adapter_worker" },
        { "method": "wallet.dust.sync.cancel", "status": "ready", "mode": "standalone", "checkpoint": "resumable" },
        { "method": "wallet.shielded.sync.status", "status": "ready", "mode": "standalone", "sources": ["simulated", "live", "cached", "unavailable"] },
        { "method": "wallet.shielded.sync.start", "status": "ready", "mode": "standalone", "execution": "adapter_worker" },
        { "method": "wallet.shielded.sync.cancel", "status": "ready", "mode": "standalone", "checkpoint": "resumable" },
        { "method": "vault.total_locked", "status": "queued" },
        { "method": "vault.locks.list", "status": "queued" },
        { "method": "vault.credentials.list", "status": "queued" },
        { "method": "vault.lock.create", "status": "queued" },
        { "method": "vault.deposit", "status": "queued" },
        { "method": "vault.claim", "status": "queued" },
        { "method": "identity.login", "status": "ready", "mode": "standalone", "aliasFor": "identity.authentication.prepare" },
        { "method": "identity.authentication.prepare", "status": "ready", "mode": "standalone", "standard": "SIOPv2 draft 13", "requestMode": "by_reference", "responseMode": "direct_post", "responseType": "id_token", "secretsExposed": false },
        { "method": "identity.authentication.accept", "status": "ready", "mode": "standalone", "confirmationRequired": true, "algorithms": ["EdDSA", "ES256"], "secretsExposed": false },
        { "method": "identity.authentication.refuse", "status": "ready", "mode": "standalone" },
        { "method": "identity.authentication.get", "status": "ready", "mode": "standalone", "secretsExposed": false },
        { "method": "identity.authentication.list", "status": "ready", "mode": "standalone", "scope": "active_profile", "secretsExposed": false },
        { "method": "credential.receive", "status": "ready", "mode": "standalone", "source": "public_fixture" },
        { "method": "credential.request", "status": "ready", "mode": "standalone", "aliasFor": "credential.receive" },
        { "method": "credential.list", "status": "ready", "mode": "standalone", "scope": "active_profile" },
        { "method": "credential.get", "status": "ready", "mode": "standalone", "scope": "active_profile", "rawCredentialExposed": false },
        { "method": "credential.reverify", "status": "ready", "mode": "standalone", "stages": ["structural", "issuer", "proof", "temporal", "status", "schema", "trust"] },
        { "method": "credential.verify", "status": "ready", "mode": "standalone", "aliasFor": "credential.reverify" },
        { "method": "credential.delete", "status": "ready", "mode": "standalone", "confirmationRequired": true },
        { "method": "credential.disclosure.candidates", "status": "ready", "mode": "standalone", "claimValuesExposed": false },
        { "method": "credential.disclosure.preview", "status": "ready", "mode": "standalone", "generatesPresentation": false, "claimValuesExposed": false },
        { "method": "credential.issuance.prepare", "status": "ready", "mode": "standalone", "standard": "OpenID4VCI 1.0 Final", "offerMode": "embedded" },
        { "method": "credential.issuance.accept", "status": "ready", "mode": "standalone", "grant": "pre-authorized_code", "confirmationRequired": true, "proof": "jwt" },
        { "method": "credential.issuance.refuse", "status": "ready", "mode": "standalone" },
        { "method": "credential.issuance.get", "status": "ready", "mode": "standalone", "secretsExposed": false },
        { "method": "credential.issuance.list", "status": "ready", "mode": "standalone", "scope": "active_profile", "secretsExposed": false },
        { "method": "credential.presentation.prepare", "status": "ready", "mode": "standalone", "standard": "OpenID4VP 1.0 Final", "query": "DCQL", "requestMode": "by_reference", "claimValuesExposed": false },
        { "method": "credential.presentation.accept", "status": "blocked", "mode": "standalone", "confirmationRequired": true, "proofAvailable": false, "generatesPresentation": false, "blocker": "https://github.com/MediaNoxLabs/oxid/issues/28" },
        { "method": "credential.presentation.refuse", "status": "ready", "mode": "standalone" },
        { "method": "credential.presentation.get", "status": "ready", "mode": "standalone", "secretsExposed": false },
        { "method": "credential.presentation.list", "status": "ready", "mode": "standalone", "scope": "active_profile", "secretsExposed": false },
        { "method": "did.create", "status": "ready", "mode": "development_only", "networks": ["undeployed"], "initialMethods": ["ed25519", "p256"] },
        { "method": "did.resolve", "status": "ready", "mode": "standalone", "sources": ["standalone", "live"] },
        { "method": "did.list", "status": "ready", "mode": "standalone", "scope": "active_profile" },
        { "method": "did.get", "status": "ready", "mode": "standalone", "scope": "active_profile" },
        { "method": "did.forget", "status": "ready", "mode": "standalone", "scope": "active_profile" },
        { "method": "did.update", "status": "ready", "mode": "development_only", "operations": ["addAlsoKnownAs", "removeAlsoKnownAs", "addVerificationMethod", "updateVerificationMethod", "removeVerificationMethod", "addVerificationRelationship", "removeVerificationRelationship", "addService", "updateService", "removeService"], "confirmationRequired": true },
        { "method": "did.sign", "status": "ready", "mode": "development_only", "algorithms": ["ed25519", "p256"], "confirmationRequired": true },
        { "method": "did.deactivate", "status": "ready", "mode": "development_only", "confirmationRequired": true },
        { "method": "diagnostics.snapshot", "status": "queued" }
    ])
}

/// Failures while reading or writing the headless protocol stream.
#[derive(Debug)]
pub enum HeadlessIoError {
    Read(io::Error),
    Write(io::Error),
    Serialize(serde_json::Error),
}

impl fmt::Display for HeadlessIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(_) => formatter.write_str("failed to read a headless wallet request"),
            Self::Write(_) => formatter.write_str("failed to write a headless wallet response"),
            Self::Serialize(_) => {
                formatter.write_str("failed to serialize a headless wallet response")
            }
        }
    }
}

impl Error for HeadlessIoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) | Self::Write(error) => Some(error),
            Self::Serialize(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_adapter_openid4vci::standalone_credential_offer;
    use oxid_adapter_siopv2::standalone_self_issued_request;

    fn execute(input: &str) -> Vec<Value> {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        execute_with_wallet(&wallet, input)
    }

    fn execute_with_wallet(wallet: &HeadlessWallet, input: &str) -> Vec<Value> {
        let mut output = Vec::new();
        wallet
            .run(input.as_bytes(), &mut output)
            .expect("protocol exchange should succeed");

        String::from_utf8(output)
            .expect("protocol output should be UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("each response should be JSON"))
            .collect()
    }

    #[test]
    fn reports_ready_and_queued_capabilities() {
        let responses = execute(
            r#"{"protocol":"oxid.headless.v1","id":"cap-1","method":"system.capabilities","params":{}}"#,
        );

        assert_eq!(responses[0]["id"], "cap-1");
        assert_eq!(responses[0]["ok"], true);
        let methods = responses[0]["result"]["methods"]
            .as_array()
            .expect("methods should be an array");
        assert!(methods.iter().any(|capability| {
            capability["method"] == "wallet.profile.create" && capability["status"] == "ready"
        }));
        assert!(methods.iter().any(|capability| {
            capability["method"] == "wallet.transaction.send_unshielded"
                && capability["status"] == "ready"
                && capability["aliasFor"] == "wallet.transaction.submit_unshielded"
        }));
        assert_eq!(responses[0]["result"]["custodyMode"], "development_only");
        assert!(methods.iter().any(|capability| {
            capability["method"] == "wallet.key.sign"
                && capability["status"] == "ready"
                && capability["mode"] == "development_only"
        }));
        assert!(methods.iter().any(|capability| {
            capability["method"] == "wallet.balance.snapshot"
                && capability["status"] == "ready"
                && capability["sources"] == json!(["simulated", "live", "cached"])
        }));
        assert!(methods.iter().any(|capability| {
            capability["method"] == "wallet.account.derive"
                && capability["status"] == "ready"
                && capability["mode"] == "development_only"
        }));
        assert!(methods.iter().any(|capability| {
            capability["method"] == "wallet.transaction.prepare_unshielded"
                && capability["status"] == "ready"
                && capability["submissionReady"] == false
        }));
        assert!(methods.iter().any(|capability| {
            capability["method"] == "did.resolve"
                && capability["status"] == "ready"
                && capability["sources"] == json!(["standalone", "live"])
        }));
        assert!(methods.iter().any(|capability| {
            capability["method"] == "credential.reverify" && capability["status"] == "ready"
        }));
        assert!(methods.iter().any(|capability| {
            capability["method"] == "credential.disclosure.preview"
                && capability["status"] == "ready"
                && capability["generatesPresentation"] == false
                && capability["claimValuesExposed"] == false
        }));
        assert!(methods.iter().any(|capability| {
            capability["method"] == "credential.presentation.accept"
                && capability["status"] == "blocked"
                && capability["proofAvailable"] == false
                && capability["generatesPresentation"] == false
        }));
        assert!(methods.iter().any(|capability| {
            capability["method"] == "identity.login"
                && capability["status"] == "ready"
                && capability["aliasFor"] == "identity.authentication.prepare"
        }));
    }

    #[test]
    fn receives_reverifies_and_deletes_a_credential_without_exposing_wire_bytes() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"credential-profile","method":"wallet.profile.create","params":{"displayName":"Credential flow"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile");
        let select = json!({"protocol": PROTOCOL_VERSION, "id": "credential-select", "method": "wallet.profile.select", "params": {"profileId": profile_id}}).to_string();
        assert_eq!(execute_with_wallet(&wallet, &select)[0]["ok"], true);
        let received = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"credential-receive","method":"credential.receive","params":{}}"#,
        );
        let credential = &received[0]["result"]["credential"];
        assert_eq!(credential["verification"]["outcome"], "valid");
        assert_eq!(
            credential["verification"]["stages"]
                .as_array()
                .map(Vec::len),
            Some(7)
        );
        assert!(credential.get("signedBytes").is_none());
        let credential_id = credential["id"].as_str().expect("credential id");
        let requests = format!(
            "{}\n{}\n{}\n{}",
            json!({"protocol": PROTOCOL_VERSION, "id": "credential-list", "method": "credential.list", "params": {}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "credential-verify", "method": "credential.reverify", "params": {"credentialId": credential_id}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "credential-delete-denied", "method": "credential.delete", "params": {"credentialId": credential_id, "confirmed": false, "intent": "DELETE_CREDENTIAL"}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "credential-delete", "method": "credential.delete", "params": {"credentialId": credential_id, "confirmed": true, "intent": "DELETE_CREDENTIAL"}}),
        );
        let responses = execute_with_wallet(&wallet, &requests);
        assert_eq!(
            responses[0]["result"]["credentials"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            responses[1]["result"]["credential"]["verification"]["outcome"],
            "valid"
        );
        assert_eq!(responses[2]["error"]["code"], "confirmation_required");
        assert_eq!(responses[3]["result"]["deleted"], true);
    }

    #[test]
    fn issues_and_stores_a_verified_credential_through_the_headless_flow() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"issuance-profile","method":"wallet.profile.create","params":{"displayName":"Issuance flow"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile identifier");
        let initialization = format!(
            "{}\n{}",
            json!({"protocol": PROTOCOL_VERSION, "id": "issuance-select", "method": "wallet.profile.select", "params": {"profileId": profile_id}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "issuance-security", "method": "wallet.security.initialize", "params": {}}),
        );
        let initialized = execute_with_wallet(&wallet, &initialization);
        assert_eq!(initialized[0]["ok"], true);
        assert_eq!(initialized[1]["ok"], true);

        let created_did = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"issuance-did","method":"did.create","params":{}}"#,
        );
        let record = &created_did[0]["result"]["didRecord"]["document"];
        let did = record["id"].as_str().expect("holder DID");
        let method_id = record["relationships"]
            .as_array()
            .expect("relationships")
            .iter()
            .find(|relationship| relationship["relationship"] == "authentication")
            .and_then(|relationship| relationship["methodIds"][0].as_str())
            .expect("authentication method");

        let prepared = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "issuance-prepare",
                "method": "credential.issuance.prepare",
                "params": {"offer": standalone_credential_offer()},
            })
            .to_string(),
        );
        assert_eq!(
            prepared[0]["result"]["issuance"]["state"],
            "awaiting_consent"
        );
        assert!(prepared[0].to_string().contains("Digital Passport"));
        assert!(!prepared[0].to_string().contains("pre-authorized"));
        let issuance_id = prepared[0]["result"]["issuance"]["id"]
            .as_str()
            .expect("issuance identifier");

        let denied = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "issuance-denied",
                "method": "credential.issuance.accept",
                "params": {"issuanceId": issuance_id, "holderDid": did, "methodId": method_id, "confirmed": false, "intent": "ACCEPT_CREDENTIAL_ISSUANCE"},
            })
            .to_string(),
        );
        assert_eq!(denied[0]["error"]["code"], "confirmation_required");

        let accepted = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "issuance-accept",
                "method": "credential.issuance.accept",
                "params": {"issuanceId": issuance_id, "holderDid": did, "methodId": method_id, "confirmed": true, "intent": "ACCEPT_CREDENTIAL_ISSUANCE"},
            })
            .to_string(),
        );
        assert_eq!(accepted[0]["result"]["issuance"]["state"], "succeeded");
        let credential_id = accepted[0]["result"]["issuance"]["credentialId"]
            .as_str()
            .expect("credential identifier")
            .to_owned();
        assert!(!accepted[0].to_string().contains("credential_offer"));

        let inventories = execute_with_wallet(
            &wallet,
            &format!(
                "{}\n{}",
                json!({"protocol": PROTOCOL_VERSION, "id": "issuance-list", "method": "credential.issuance.list", "params": {}}),
                json!({"protocol": PROTOCOL_VERSION, "id": "issued-credentials", "method": "credential.list", "params": {}}),
            ),
        );
        assert_eq!(
            inventories[0]["result"]["issuances"][0]["state"],
            "succeeded"
        );
        assert_eq!(
            inventories[1]["result"]["credentials"][0]["verification"]["outcome"],
            "valid"
        );
        assert_eq!(
            inventories[1]["result"]["credentials"][0]["format"],
            "midnight_compact_vc"
        );
        assert_eq!(
            inventories[1]["result"]["credentials"][0]["displayName"],
            "Digital Passport"
        );
        let issued_inventory = inventories[1].to_string();
        assert!(!issued_inventory.contains("signedBytes"));
        assert!(!issued_inventory.contains("detachedProof"));
        assert!(!issued_inventory.contains("privateMaterial"));

        let other_profile = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"issuance-other-profile","method":"wallet.profile.create","params":{"displayName":"Other holder"}}"#,
        );
        let other_profile_id = other_profile[0]["result"]["profile"]["id"]
            .as_str()
            .expect("other profile identifier");
        let select_other = json!({
            "protocol": PROTOCOL_VERSION,
            "id": "issuance-select-other",
            "method": "wallet.profile.select",
            "params": {"profileId": other_profile_id},
        })
        .to_string();
        assert_eq!(execute_with_wallet(&wallet, &select_other)[0]["ok"], true);
        let isolated = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "disclosure-other-profile",
                "method": "credential.disclosure.candidates",
                "params": {"credentialId": credential_id.clone()},
            })
            .to_string(),
        );
        assert_eq!(isolated[0]["error"]["code"], "not_found");
        let select_owner = json!({
            "protocol": PROTOCOL_VERSION,
            "id": "issuance-select-owner",
            "method": "wallet.profile.select",
            "params": {"profileId": profile_id},
        })
        .to_string();
        assert_eq!(execute_with_wallet(&wallet, &select_owner)[0]["ok"], true);

        let disclosure = execute_with_wallet(
            &wallet,
            &format!(
                "{}\n{}",
                json!({"protocol": PROTOCOL_VERSION, "id": "disclosure-candidates", "method": "credential.disclosure.candidates", "params": {"credentialId": credential_id.clone()}}),
                json!({"protocol": PROTOCOL_VERSION, "id": "disclosure-preview", "method": "credential.disclosure.preview", "params": {
                    "credentialId": credential_id,
                    "revealClaimPaths": ["/credentialSubject/firstName", "/credentialSubject/lastName"],
                    "predicates": [{"claimPath": "/credentialSubject/dateOfBirth", "kind": "age_over", "threshold": 18}]
                }}),
            ),
        );
        assert_eq!(
            disclosure[0]["result"]["disclosure"]["schemaId"],
            "digital-passport:v1"
        );
        assert_eq!(
            disclosure[0]["result"]["disclosure"]["candidates"]
                .as_array()
                .map(Vec::len),
            Some(5)
        );
        assert_eq!(
            disclosure[1]["result"]["plan"]["outcome"],
            "local_preview_ready"
        );
        assert_eq!(
            disclosure[1]["result"]["plan"]["presentationGenerated"],
            false
        );
        let disclosure_json = serde_json::to_string(&disclosure).expect("serialize responses");
        assert!(!disclosure_json.contains("Alice"));
        assert!(!disclosure_json.contains("Example"));
        assert!(!disclosure_json.contains("AB1234567"));

        let prepared_presentation = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "presentation-prepare",
                "method": "credential.presentation.prepare",
                "params": {"request": oxid_composition::standalone_openid4vp_request()},
            })
            .to_string(),
        );
        let presentation = &prepared_presentation[0]["result"]["presentation"];
        assert_eq!(presentation["state"], "awaiting_consent");
        assert_eq!(presentation["presentationGenerated"], false);
        assert_eq!(presentation["verifierValidated"], false);
        assert_eq!(
            presentation["requestedClaims"].as_array().map(Vec::len),
            Some(3)
        );
        assert_eq!(presentation["candidates"].as_array().map(Vec::len), Some(1));
        let presentation_id = presentation["id"]
            .as_str()
            .expect("presentation identifier");
        let candidate_id = presentation["candidates"][0]["credentialId"]
            .as_str()
            .expect("credential candidate identifier");
        let preview_json = prepared_presentation[0].to_string();
        assert!(!preview_json.contains("Alice"));
        assert!(!preview_json.contains("Example"));
        assert!(!preview_json.contains("AB1234567"));

        let denied_presentation = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "presentation-denied",
                "method": "credential.presentation.accept",
                "params": {
                    "presentationId": presentation_id,
                    "credentialId": candidate_id,
                    "confirmed": false,
                    "intent": "ACCEPT_CREDENTIAL_PRESENTATION"
                },
            })
            .to_string(),
        );
        assert_eq!(
            denied_presentation[0]["error"]["code"],
            "confirmation_required"
        );

        let blocked_presentation = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "presentation-accept",
                "method": "credential.presentation.accept",
                "params": {
                    "presentationId": presentation_id,
                    "credentialId": candidate_id,
                    "confirmed": true,
                    "intent": "ACCEPT_CREDENTIAL_PRESENTATION"
                },
            })
            .to_string(),
        );
        assert_eq!(
            blocked_presentation[0]["error"]["code"],
            "proof_unavailable"
        );

        let failed_presentation = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "presentation-get",
                "method": "credential.presentation.get",
                "params": {"presentationId": presentation_id},
            })
            .to_string(),
        );
        let failed = &failed_presentation[0]["result"]["presentation"];
        assert_eq!(failed["state"], "failed");
        assert_eq!(failed["failureCode"], "proof_unavailable");
        assert_eq!(failed["presentationGenerated"], false);
        assert_eq!(failed["verifierValidated"], false);
        assert!(!failed_presentation[0].to_string().contains("vp_token"));
    }

    #[test]
    fn authenticates_a_managed_did_once_without_exposing_protocol_secrets() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"authentication-profile","method":"wallet.profile.create","params":{"displayName":"Authentication flow"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile identifier");
        let initialized = execute_with_wallet(
            &wallet,
            &format!(
                "{}\n{}",
                json!({"protocol": PROTOCOL_VERSION, "id": "authentication-select", "method": "wallet.profile.select", "params": {"profileId": profile_id}}),
                json!({"protocol": PROTOCOL_VERSION, "id": "authentication-security", "method": "wallet.security.initialize", "params": {}}),
            ),
        );
        assert_eq!(initialized[0]["ok"], true);
        assert_eq!(initialized[1]["ok"], true);

        let created_did = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"authentication-did","method":"did.create","params":{}}"#,
        );
        let record = &created_did[0]["result"]["didRecord"]["document"];
        let did = record["id"].as_str().expect("holder DID");
        let method_id = record["relationships"]
            .as_array()
            .expect("relationships")
            .iter()
            .find(|relationship| relationship["relationship"] == "authentication")
            .and_then(|relationship| relationship["methodIds"][0].as_str())
            .expect("authentication method");

        let prepared = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "authentication-prepare",
                "method": "identity.authentication.prepare",
                "params": {"request": standalone_self_issued_request()},
            })
            .to_string(),
        );
        let preview = &prepared[0]["result"]["authentication"];
        assert_eq!(preview["state"], "awaiting_consent");
        assert!(preview["verifier"].as_str().is_some());
        assert!(preview["purpose"].as_str().is_some());
        assert!(!prepared[0].to_string().contains("nonce"));
        assert!(!prepared[0].to_string().contains("id_token"));
        let authentication_id = preview["id"].as_str().expect("authentication identifier");

        let denied = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "authentication-denied",
                "method": "identity.authentication.accept",
                "params": {"authenticationId": authentication_id, "holderDid": did, "methodId": method_id, "confirmed": false, "intent": "ACCEPT_SELF_ISSUED_AUTHENTICATION"},
            })
            .to_string(),
        );
        assert_eq!(denied[0]["error"]["code"], "confirmation_required");

        let accepted = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "authentication-accept",
                "method": "identity.authentication.accept",
                "params": {"authenticationId": authentication_id, "holderDid": did, "methodId": method_id, "confirmed": true, "intent": "ACCEPT_SELF_ISSUED_AUTHENTICATION"},
            })
            .to_string(),
        );
        assert_eq!(
            accepted[0]["result"]["authentication"]["state"],
            "succeeded"
        );
        assert!(!accepted[0].to_string().contains("nonce"));
        assert!(!accepted[0].to_string().contains("id_token"));

        let replay = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "authentication-replay",
                "method": "identity.authentication.accept",
                "params": {"authenticationId": authentication_id, "holderDid": did, "methodId": method_id, "confirmed": true, "intent": "ACCEPT_SELF_ISSUED_AUTHENTICATION"},
            })
            .to_string(),
        );
        assert_eq!(replay[0]["error"]["code"], "failed_precondition");

        let inventory = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"authentication-list","method":"identity.authentication.list","params":{}}"#,
        );
        assert_eq!(
            inventory[0]["result"]["authentications"][0]["state"],
            "succeeded"
        );
        assert!(!inventory[0].to_string().contains("nonce"));
        assert!(!inventory[0].to_string().contains("id_token"));
    }

    #[test]
    fn creates_a_profile_through_shared_application_services() {
        let responses = execute(
            r#"{"protocol":"oxid.headless.v1","id":"profile-1","method":"wallet.profile.create","params":{"displayName":"  Headless primary  "}}"#,
        );

        assert_eq!(responses[0]["ok"], true);
        assert_eq!(
            responses[0]["result"]["profile"]["displayName"],
            "Headless primary"
        );
        assert!(
            responses[0]["result"]["profile"]["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("profile_"))
        );
    }

    #[test]
    fn maps_domain_validation_to_a_stable_safe_error() {
        let responses = execute(
            r#"{"protocol":"oxid.headless.v1","id":"profile-2","method":"wallet.profile.create","params":{"displayName":" "}}"#,
        );

        assert_eq!(responses[0]["ok"], false);
        assert_eq!(responses[0]["error"]["code"], "invalid_argument");
        assert_eq!(responses[0]["id"], "profile-2");
    }

    #[test]
    fn lists_selects_and_restores_a_profile_in_one_headless_flow() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"create-flow","method":"wallet.profile.create","params":{"displayName":"Flow profile"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("created profile should have an identifier");
        let follow_up = format!(
            "{}\n{}\n{}",
            r#"{"protocol":"oxid.headless.v1","id":"list-flow","method":"wallet.profile.list","params":{}}"#,
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "select-flow",
                "method": "wallet.profile.select",
                "params": { "profileId": profile_id }
            }),
            r#"{"protocol":"oxid.headless.v1","id":"active-flow","method":"wallet.profile.active","params":{}}"#,
        );
        let responses = execute_with_wallet(&wallet, &follow_up);

        assert_eq!(responses.len(), 3);
        assert_eq!(responses[0]["result"]["profiles"][0]["id"], profile_id);
        assert_eq!(responses[1]["result"]["profile"]["id"], profile_id);
        assert_eq!(responses[2]["result"]["profile"]["id"], profile_id);
    }

    #[test]
    fn selecting_an_unknown_profile_returns_not_found() {
        let responses = execute(
            r#"{"protocol":"oxid.headless.v1","id":"select-missing","method":"wallet.profile.select","params":{"profileId":"profile_missing"}}"#,
        );

        assert_eq!(responses[0]["error"]["code"], "not_found");
        assert_eq!(responses[0]["id"], "select-missing");
    }

    #[test]
    fn recovers_after_malformed_and_unknown_requests() {
        let responses = execute(concat!(
            "not-json\n",
            r#"{"protocol":"oxid.headless.v1","id":"unknown-1","method":"secret.export","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"cap-2","method":"system.capabilities","params":{}}"#,
        ));

        assert_eq!(responses.len(), 3);
        assert_eq!(responses[0]["error"]["code"], "parse_error");
        assert_eq!(responses[1]["error"]["code"], "method_not_found");
        assert_eq!(responses[2]["ok"], true);
    }

    #[test]
    fn rejects_invalid_ids_without_echoing_them() {
        let responses = execute(
            r#"{"protocol":"oxid.headless.v1","id":{"secret":"do-not-echo"},"method":"system.capabilities","params":{}}"#,
        );

        assert_eq!(responses[0]["error"]["code"], "invalid_request");
        assert!(responses[0]["id"].is_null());
        assert!(!responses[0].to_string().contains("do-not-echo"));
    }

    #[test]
    fn rejects_unsupported_protocols_and_invalid_parameters() {
        let responses = execute(concat!(
            r#"{"protocol":"oxid.headless.v2","id":"future-1","method":"system.capabilities","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"params-1","method":"wallet.profile.create","params":{"displayName":"Primary","seedHex":"do-not-accept"}}"#,
        ));

        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["error"]["code"], "unsupported_protocol");
        assert_eq!(responses[0]["id"], "future-1");
        assert_eq!(responses[1]["error"]["code"], "invalid_params");
        assert_eq!(responses[1]["id"], "params-1");
        assert!(!responses[1].to_string().contains("do-not-accept"));
    }

    #[test]
    fn shutdown_stops_processing_subsequent_lines() {
        let responses = execute(concat!(
            r#"{"protocol":"oxid.headless.v1","id":"quit-1","method":"system.quit","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"ignored","method":"system.capabilities","params":{}}"#,
        ));

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["id"], "quit-1");
        assert_eq!(responses[0]["result"]["shuttingDown"], true);
    }

    #[test]
    fn supports_prototype_shutdown_alias_without_a_seed() {
        let responses = execute("quit\n");

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["ok"], true);
        assert_eq!(responses[0]["result"]["alias"], "quit");
    }

    #[test]
    fn exercises_the_protected_key_lifecycle_without_secret_parameters() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"secure-create","method":"wallet.profile.create","params":{"displayName":"Secure flow"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile identifier is returned");
        let select = json!({
            "protocol": PROTOCOL_VERSION,
            "id": "secure-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        });
        let setup = format!(
            "{select}\n{}\n{}\n{}",
            r#"{"protocol":"oxid.headless.v1","id":"secure-status","method":"wallet.security.status","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"secure-init","method":"wallet.security.initialize","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"secure-generate","method":"wallet.key.generate","params":{"label":"Authentication key","algorithm":"ed25519","purpose":"authentication"}}"#,
        );
        let responses = execute_with_wallet(&wallet, &setup);

        assert_eq!(responses[1]["result"]["security"]["state"], "uninitialized");
        assert_eq!(responses[2]["result"]["security"]["state"], "unlocked");
        assert_eq!(
            responses[2]["result"]["security"]["protection"],
            "development_only"
        );
        let key_ref = responses[3]["result"]["key"]["keyRef"]
            .as_str()
            .expect("opaque key reference is returned");
        let confirmation = json!({
            "title": "Sign conformance challenge",
            "summary": "Authorize a non-secret test payload",
            "confirmed": true
        });
        let sign = json!({
            "protocol": PROTOCOL_VERSION,
            "id": "secure-sign",
            "method": "wallet.key.sign",
            "params": {
                "keyRef": key_ref,
                "payloadHex": "6368616c6c656e6765",
                "confirmation": confirmation
            }
        });
        let signed = execute_with_wallet(&wallet, &sign.to_string());
        assert_eq!(signed[0]["ok"], true, "unexpected response: {signed:?}");
        assert_eq!(signed[0]["result"]["algorithm"], "ed25519");
        assert_eq!(
            signed[0]["result"]["signatureHex"]
                .as_str()
                .expect("signature is encoded")
                .len(),
            128
        );

        let locked_sign = format!(
            "{}\n{sign}",
            r#"{"protocol":"oxid.headless.v1","id":"secure-lock","method":"wallet.security.lock","params":{}}"#,
        );
        let locked = execute_with_wallet(&wallet, &locked_sign);
        assert_eq!(locked[0]["result"]["security"]["state"], "locked");
        assert_eq!(locked[1]["error"]["code"], "wallet_locked");

        let delete_without_confirmation = json!({
            "protocol": PROTOCOL_VERSION,
            "id": "secure-delete-denied",
            "method": "wallet.key.delete",
            "params": {
                "keyRef": key_ref,
                "confirmation": {
                    "title": "Delete test key",
                    "summary": "Remove the ephemeral test key",
                    "confirmed": false
                }
            }
        });
        let delete = json!({
            "protocol": PROTOCOL_VERSION,
            "id": "secure-delete",
            "method": "wallet.key.delete",
            "params": {
                "keyRef": key_ref,
                "confirmation": {
                    "title": "Delete test key",
                    "summary": "Remove the ephemeral test key",
                    "confirmed": true
                }
            }
        });
        let cleanup = format!(
            "{}\n{delete_without_confirmation}\n{delete}\n{}",
            r#"{"protocol":"oxid.headless.v1","id":"secure-unlock","method":"wallet.security.unlock","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"secure-list","method":"wallet.key.list","params":{}}"#,
        );
        let cleaned = execute_with_wallet(&wallet, &cleanup);
        assert_eq!(cleaned[0]["result"]["security"]["state"], "unlocked");
        assert_eq!(cleaned[1]["error"]["code"], "confirmation_required");
        assert_eq!(cleaned[2]["result"]["deleted"], true);
        assert_eq!(cleaned[3]["result"]["keys"], json!([]));
    }

    #[test]
    fn exercises_the_complete_standalone_did_lifecycle_without_key_handles() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let setup = execute_with_wallet(
            &wallet,
            concat!(
                r#"{"protocol":"oxid.headless.v1","id":"did-profile","method":"wallet.profile.create","params":{"displayName":"DID flow"}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"did-select","method":"wallet.profile.select","params":{"profileId":"profile_missing"}}"#,
            ),
        );
        let profile_id = setup[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile id");
        let initialize = format!(
            "{}\n{}",
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "did-select-real",
                "method": "wallet.profile.select",
                "params": { "profileId": profile_id },
            }),
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "did-security",
                "method": "wallet.security.initialize",
                "params": {},
            }),
        );
        let initialized = execute_with_wallet(&wallet, &initialize);
        assert_eq!(initialized[0]["ok"], true);
        assert_eq!(initialized[1]["result"]["security"]["state"], "unlocked");

        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"did-create","method":"did.create","params":{}}"#,
        );
        assert_eq!(created[0]["ok"], true, "unexpected response: {created:?}");
        let did = created[0]["result"]["didRecord"]["document"]["id"]
            .as_str()
            .expect("created DID")
            .to_owned();
        assert_eq!(
            created[0]["result"]["didRecord"]["document"]["verificationMethods"]
                .as_array()
                .expect("methods")
                .len(),
            2
        );
        assert!(!created[0].to_string().contains("key_"));

        let unconfirmed = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "did-update-unconfirmed",
                "method": "did.update",
                "params": {
                    "operation": "addAlsoKnownAs",
                    "did": did,
                    "value": "https://example.test/denied",
                    "confirmation": {
                        "title": "Update DID document",
                        "summary": "This update was not authorized",
                        "confirmed": false,
                    },
                },
            })
            .to_string(),
        );
        assert_eq!(unconfirmed[0]["error"]["code"], "confirmation_required");

        let locked = execute_with_wallet(
            &wallet,
            &format!(
                "{}\n{}",
                json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": "did-lock",
                    "method": "wallet.security.lock",
                    "params": {},
                }),
                json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": "did-sign-locked",
                    "method": "did.sign",
                    "params": {
                        "did": did,
                        "methodId": "#auth-1",
                        "payloadHex": "01",
                        "confirmation": {
                            "title": "Sign identity challenge",
                            "summary": "This operation must fail while custody is locked",
                            "confirmed": true,
                        },
                    },
                }),
            ),
        );
        assert_eq!(locked[1]["error"]["code"], "wallet_locked");
        let unlocked = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"did-unlock","method":"wallet.security.unlock","params":{}}"#,
        );
        assert_eq!(unlocked[0]["result"]["security"]["state"], "unlocked");

        let operations = [
            json!({ "operation": "addAlsoKnownAs", "did": did, "value": "https://example.test/alice" }),
            json!({ "operation": "addVerificationMethod", "did": did, "fragment": "recovery-1", "algorithm": "ed25519" }),
            json!({ "operation": "updateVerificationMethod", "did": did, "methodId": "#recovery-1", "algorithm": "p256" }),
            json!({ "operation": "addVerificationRelationship", "did": did, "relationship": "assertionMethod", "methodId": "#recovery-1" }),
            json!({ "operation": "addService", "did": did, "id": "#messages", "serviceType": "MessagingService", "endpoint": "https://example.test/messages" }),
            json!({ "operation": "updateService", "did": did, "id": "#messages", "serviceType": "DIDCommMessaging", "endpoint": "https://example.test/didcomm" }),
        ];
        for (index, mut params) in operations.into_iter().enumerate() {
            params["confirmation"] = json!({
                "title": "Update DID document",
                "summary": "Authorize this visible standalone DID change",
                "confirmed": true,
            });
            let response = execute_with_wallet(
                &wallet,
                &json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": format!("did-update-{index}"),
                    "method": "did.update",
                    "params": params,
                })
                .to_string(),
            );
            assert_eq!(response[0]["ok"], true, "unexpected response: {response:?}");
            assert!(!response[0].to_string().contains("key_"));
        }

        let signed = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "did-sign",
                "method": "did.sign",
                "params": {
                    "did": did,
                    "methodId": "#auth-1",
                    "payloadHex": "6368616c6c656e6765",
                    "confirmation": {
                        "title": "Sign identity challenge",
                        "summary": "Authorize the verifier challenge for this DID",
                        "confirmed": true,
                    },
                },
            })
            .to_string(),
        );
        assert_eq!(signed[0]["result"]["algorithm"], "ed25519");
        assert_eq!(
            signed[0]["result"]["signatureHex"]
                .as_str()
                .expect("signature")
                .len(),
            128
        );
        assert!(!signed[0].to_string().contains("key_"));

        let removals = [
            json!({ "operation": "removeVerificationRelationship", "did": did, "relationship": "assertionMethod", "methodId": "#recovery-1" }),
            json!({ "operation": "removeVerificationMethod", "did": did, "methodId": "#recovery-1" }),
            json!({ "operation": "removeService", "did": did, "id": "#messages" }),
            json!({ "operation": "removeAlsoKnownAs", "did": did, "value": "https://example.test/alice" }),
        ];
        for (index, mut params) in removals.into_iter().enumerate() {
            params["confirmation"] = json!({
                "title": "Update DID document",
                "summary": "Authorize this visible standalone DID change",
                "confirmed": true,
            });
            let response = execute_with_wallet(
                &wallet,
                &json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": format!("did-remove-{index}"),
                    "method": "did.update",
                    "params": params,
                })
                .to_string(),
            );
            assert_eq!(response[0]["ok"], true, "unexpected response: {response:?}");
        }

        let deactivated = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "did-deactivate",
                "method": "did.deactivate",
                "params": {
                    "did": did,
                    "confirmation": {
                        "title": "Deactivate DID",
                        "summary": "Permanently disable standalone DID operations",
                        "confirmed": true,
                    },
                },
            })
            .to_string(),
        );
        assert_eq!(
            deactivated[0]["result"]["didRecord"]["documentMetadata"]["deactivated"],
            true
        );

        let denied = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "did-sign-deactivated",
                "method": "did.sign",
                "params": {
                    "did": did,
                    "methodId": "#auth-1",
                    "payloadHex": "01",
                    "confirmation": {
                        "title": "Sign after deactivation",
                        "summary": "This operation must fail closed",
                        "confirmed": true,
                    },
                },
            })
            .to_string(),
        );
        assert_eq!(denied[0]["error"]["code"], "failed_precondition");
    }

    #[test]
    fn derives_and_binds_a_midnight_account_without_secret_protocol_fields() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"derive-create","method":"wallet.profile.create","params":{"displayName":"Derived account"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile identifier is returned");
        let setup = format!(
            "{}\n{}\n{}\n{}",
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "derive-select",
                "method": "wallet.profile.select",
                "params": { "profileId": profile_id }
            }),
            r#"{"protocol":"oxid.headless.v1","id":"derive-before-init","method":"wallet.account.derive","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"derive-init","method":"wallet.security.initialize","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"derive-account","method":"wallet.account.derive","params":{"accountIndex":0,"addressIndex":0}}"#,
        );
        let responses = execute_with_wallet(&wallet, &setup);
        assert_eq!(responses[1]["error"]["code"], "failed_precondition");
        assert_eq!(
            responses[3]["ok"], true,
            "unexpected response: {responses:?}"
        );
        let derived = &responses[3]["result"]["account"];
        assert_eq!(derived["networkId"], "undeployed");
        assert_eq!(derived["accountId"], "midnight_account_0_0");
        assert_eq!(derived["receiveAddress"]["kind"], "unshielded");
        assert!(
            derived["receiveAddress"]["value"]
                .as_str()
                .is_some_and(|address| address.starts_with("mn_addr_undeployed1"))
        );
        assert_eq!(derived["addresses"].as_array().map(Vec::len), Some(2));
        assert!(derived["addresses"].as_array().is_some_and(|addresses| {
            addresses.iter().any(|address| {
                address["kind"] == "shielded"
                    && address["value"]
                        .as_str()
                        .is_some_and(|value| value.starts_with("mn_shield-addr_undeployed1"))
            })
        }));
        let shielded = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"derive-shielded","method":"wallet.address.shielded","params":{}}"#,
        );
        assert_eq!(shielded[0]["result"]["address"]["kind"], "shielded");
        assert!(
            shielded[0]["result"]["address"]["value"]
                .as_str()
                .is_some_and(|value| value.starts_with("mn_shield-addr_undeployed1"))
        );
        let key_ref = derived["transactionKeyRef"]
            .as_str()
            .expect("opaque key reference is returned");

        let flow = format!(
            "{}\n{}\n{}\n{}",
            r#"{"protocol":"oxid.headless.v1","id":"derive-get","method":"wallet.account.get","params":{}}"#,
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "derive-sign",
                "method": "wallet.key.sign",
                "params": {
                    "keyRef": key_ref,
                    "payloadHex": "7472616e73616374696f6e2d696e74656e74",
                    "confirmation": {
                        "title": "Sign Midnight transaction intent",
                        "summary": "Authorize the bounded headless conformance payload",
                        "confirmed": true
                    }
                }
            }),
            r#"{"protocol":"oxid.headless.v1","id":"derive-lock","method":"wallet.security.lock","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"derive-locked","method":"wallet.account.derive","params":{"addressIndex":1}}"#,
        );
        let flowed = execute_with_wallet(&wallet, &flow);
        assert_eq!(
            flowed[0]["result"]["account"]["accountId"],
            "midnight_account_0_0"
        );
        assert_eq!(flowed[1]["result"]["algorithm"], "secp256k1-schnorr");
        assert_eq!(
            flowed[1]["result"]["signatureHex"]
                .as_str()
                .expect("signature is encoded")
                .len(),
            128
        );
        assert_eq!(flowed[3]["error"]["code"], "wallet_locked");

        let out_of_bounds = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"derive-bounds","method":"wallet.account.derive","params":{"accountIndex":2147483648}}"#,
        );
        assert_eq!(out_of_bounds[0]["error"]["code"], "invalid_argument");

        let rejected = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"derive-secret","method":"wallet.account.derive","params":{"seedHex":"do-not-accept"}}"#,
        );
        assert_eq!(rejected[0]["error"]["code"], "invalid_params");
        assert!(!rejected[0].to_string().contains("do-not-accept"));
    }

    #[test]
    fn completes_an_exact_unshielded_transfer_without_exposing_material() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"transfer-create","method":"wallet.profile.create","params":{"displayName":"Transfer flow"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile identifier is returned");
        let setup = format!(
            "{}\n{}\n{}",
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-select",
                "method": "wallet.profile.select",
                "params": { "profileId": profile_id }
            }),
            r#"{"protocol":"oxid.headless.v1","id":"transfer-init","method":"wallet.security.initialize","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"transfer-derive","method":"wallet.account.derive","params":{}}"#,
        );
        let setup = execute_with_wallet(&wallet, &setup);
        let recipient = setup[2]["result"]["account"]["receiveAddress"]["value"]
            .as_str()
            .expect("derived address is returned");
        let before_sync = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-before-sync",
                "method": "wallet.transaction.prepare_unshielded",
                "params": {
                    "recipientAddress": recipient,
                    "amountAtomicUnits": "1500000"
                }
            })
            .to_string(),
        );
        assert_eq!(before_sync[0]["error"]["code"], "failed_precondition");
        let synchronized = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"transfer-sync","method":"wallet.connect","params":{}}"#,
        );
        assert_eq!(synchronized[0]["ok"], true);
        let prepared = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-prepare",
                "method": "wallet.transaction.prepare_unshielded",
                "params": {
                    "recipientAddress": recipient,
                    "amountAtomicUnits": "1500000"
                }
            })
            .to_string(),
        );
        let transfer = &prepared[0]["result"]["transfer"];
        assert_eq!(transfer["state"], "prepared");
        assert_eq!(transfer["amount"]["atomicUnits"], "1500000");
        assert_eq!(transfer["change"]["atomicUnits"], "500000");
        assert_eq!(transfer["inputCount"], 1);
        assert_eq!(transfer["feeState"], "requires_balancing");
        assert_eq!(transfer["proofRequired"], true);
        assert_eq!(transfer["submissionReady"], false);
        let draft_id = transfer["draftId"].as_str().expect("draft id is returned");
        let challenge = transfer["authorizationChallenge"]
            .as_str()
            .expect("challenge is returned");

        let denied = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-denied",
                "method": "wallet.transaction.authorize_unshielded",
                "params": {
                    "draftId": draft_id,
                    "authorizationChallenge": challenge,
                    "confirmation": {
                        "title": "Authorize NIGHT transfer",
                        "summary": "Send 1.5 NIGHT; proving and submission remain pending",
                        "confirmed": false
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(denied[0]["error"]["code"], "confirmation_required");

        let mismatch = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-mismatch",
                "method": "wallet.transaction.authorize_unshielded",
                "params": {
                    "draftId": draft_id,
                    "authorizationChallenge": "txauth_wrong",
                    "confirmation": {
                        "title": "Authorize NIGHT transfer",
                        "summary": "Send 1.5 NIGHT; proving and submission remain pending",
                        "confirmed": true
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(mismatch[0]["error"]["code"], "authorization_mismatch");

        let locked = execute_with_wallet(
            &wallet,
            &format!(
                "{}\n{}",
                r#"{"protocol":"oxid.headless.v1","id":"transfer-lock","method":"wallet.security.lock","params":{}}"#,
                json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": "transfer-locked-authorize",
                    "method": "wallet.transaction.authorize_unshielded",
                    "params": {
                        "draftId": draft_id,
                        "authorizationChallenge": challenge,
                        "confirmation": {
                            "title": "Authorize NIGHT transfer",
                            "summary": "Send 1.5 NIGHT; proving and submission remain pending",
                            "confirmed": true
                        }
                    }
                })
            ),
        );
        assert_eq!(locked[1]["error"]["code"], "wallet_locked");
        let unlocked = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"transfer-unlock","method":"wallet.security.unlock","params":{}}"#,
        );
        assert_eq!(unlocked[0]["result"]["security"]["state"], "unlocked");

        let authorized = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-authorize",
                "method": "wallet.transaction.authorize_unshielded",
                "params": {
                    "draftId": draft_id,
                    "authorizationChallenge": challenge,
                    "confirmation": {
                        "title": "Authorize NIGHT transfer",
                        "summary": "Send 1.5 NIGHT; proving and submission remain pending",
                        "confirmed": true
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(authorized[0]["result"]["transfer"]["state"], "authorized");
        assert_eq!(authorized[0]["result"]["transfer"]["submissionReady"], true);
        let encoded = authorized[0].to_string();
        assert!(!encoded.contains("signatureHex"));
        assert!(!encoded.contains("transactionHex"));
        assert!(!encoded.contains("private"));

        let retained = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-get",
                "method": "wallet.transaction.draft",
                "params": { "draftId": draft_id }
            })
            .to_string(),
        );
        assert_eq!(retained[0]["result"]["transfer"]["state"], "authorized");

        let submit_denied = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-submit-denied",
                "method": "wallet.transaction.submit_unshielded",
                "params": {
                    "draftId": draft_id,
                    "confirmation": {
                        "title": "Submit NIGHT transfer",
                        "summary": "Prove, balance DUST fees, and submit the authorized transfer",
                        "confirmed": false
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(submit_denied[0]["error"]["code"], "confirmation_required");

        let locked_submit = execute_with_wallet(
            &wallet,
            &format!(
                "{}\n{}\n{}",
                r#"{"protocol":"oxid.headless.v1","id":"transfer-submit-lock","method":"wallet.security.lock","params":{}}"#,
                json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": "transfer-submit-locked",
                    "method": "wallet.transaction.submit_unshielded",
                    "params": {
                        "draftId": draft_id,
                        "confirmation": {
                            "title": "Submit NIGHT transfer",
                            "summary": "Prove, balance DUST fees, and submit the authorized transfer",
                            "confirmed": true
                        }
                    }
                }),
                json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": "transfer-submit-restored",
                    "method": "wallet.transaction.draft",
                    "params": { "draftId": draft_id }
                })
            ),
        );
        assert_eq!(locked_submit[1]["error"]["code"], "wallet_locked");
        assert_eq!(
            locked_submit[2]["result"]["transfer"]["state"],
            "authorized"
        );
        let unlocked = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"transfer-submit-unlock","method":"wallet.security.unlock","params":{}}"#,
        );
        assert_eq!(unlocked[0]["result"]["security"]["state"], "unlocked");

        let submitted = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-submit",
                "method": "wallet.transaction.submit_unshielded",
                "params": {
                    "draftId": draft_id,
                    "confirmation": {
                        "title": "Submit NIGHT transfer",
                        "summary": "Prove, balance DUST fees, and submit the authorized transfer",
                        "confirmed": true
                    }
                }
            })
            .to_string(),
        );
        let submission = &submitted[0]["result"]["submission"];
        assert_eq!(submission["mode"], "simulated");
        assert_eq!(submission["transfer"]["state"], "submitted");
        assert_eq!(submission["transfer"]["feeState"], "final");
        assert_eq!(submission["transfer"]["proofRequired"], false);
        assert_eq!(submission["fee"]["assetId"], "midnight:dust");
        assert_eq!(submission["fee"]["atomicUnits"], "1000000");
        assert_eq!(
            submission["transactionId"]
                .as_str()
                .expect("transaction id is public")
                .len(),
            64
        );
        assert_eq!(
            submission["blockId"]
                .as_str()
                .expect("block id is public")
                .len(),
            64
        );
        let submitted_wire = submitted[0].to_string();
        assert!(!submitted_wire.contains("signatureHex"));
        assert!(!submitted_wire.contains("transactionHex"));
        assert!(!submitted_wire.contains("dustSeed"));

        let submission_history = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"transfer-submission-history","method":"wallet.transaction.submission_history","params":{}}"#,
        );
        let recovered = &submission_history[0]["result"]["submissions"][0];
        assert_eq!(recovered["draftId"], draft_id);
        assert_eq!(recovered["state"], "included");
        assert_eq!(recovered["transactionId"], submission["transactionId"]);
        assert_eq!(recovered["replacementAllowed"], false);
        assert_eq!(recovered["reconciliationAllowed"], false);
        let recovered_wire = submission_history[0].to_string();
        assert!(!recovered_wire.contains("signatureHex"));
        assert!(!recovered_wire.contains("transactionHex"));
        assert!(!recovered_wire.contains("dustSeed"));

        let repeated = execute_with_wallet(
            &wallet,
            &format!(
                "{}\n{}",
                r#"{"protocol":"oxid.headless.v1","id":"transfer-submit-relock","method":"wallet.security.lock","params":{}}"#,
                json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": "transfer-submit-repeat",
                    "method": "wallet.transaction.send_unshielded",
                    "params": {
                        "draftId": draft_id,
                        "confirmation": {
                            "title": "Read submitted NIGHT transfer",
                            "summary": "Return the already included public submission metadata",
                            "confirmed": true
                        }
                    }
                })
            ),
        );
        assert_eq!(repeated[1]["ok"], true);
        assert_eq!(
            repeated[1]["result"]["submission"]["transactionId"],
            submission["transactionId"]
        );
        assert_eq!(
            repeated[1]["result"]["submission"]["blockId"],
            submission["blockId"]
        );

        let insufficient = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-insufficient",
                "method": "wallet.transaction.prepare_unshielded",
                "params": {
                    "recipientAddress": recipient,
                    "amountAtomicUnits": "6000000"
                }
            })
            .to_string(),
        );
        assert_eq!(insufficient[0]["error"]["code"], "insufficient_funds");

        let foreign_network = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-foreign-network",
                "method": "wallet.transaction.prepare_unshielded",
                "params": {
                    "recipientAddress": "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y",
                    "amountAtomicUnits": "1"
                }
            })
            .to_string(),
        );
        assert_eq!(foreign_network[0]["error"]["code"], "invalid_argument");
    }

    #[test]
    fn starts_cancels_and_retries_a_submission_through_the_headless_protocol() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"cancel-create","method":"wallet.profile.create","params":{"displayName":"Cancellation flow"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile identifier is returned");
        let setup = execute_with_wallet(
            &wallet,
            &format!(
                "{}\n{}\n{}\n{}",
                json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": "cancel-select",
                    "method": "wallet.profile.select",
                    "params": { "profileId": profile_id }
                }),
                r#"{"protocol":"oxid.headless.v1","id":"cancel-init","method":"wallet.security.initialize","params":{}}"#,
                r#"{"protocol":"oxid.headless.v1","id":"cancel-derive","method":"wallet.account.derive","params":{}}"#,
                r#"{"protocol":"oxid.headless.v1","id":"cancel-sync","method":"wallet.connect","params":{}}"#,
            ),
        );
        let recipient = setup[2]["result"]["account"]["receiveAddress"]["value"]
            .as_str()
            .expect("receive address is returned");
        let prepared = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "cancel-prepare",
                "method": "wallet.transaction.prepare_unshielded",
                "params": {
                    "recipientAddress": recipient,
                    "amountAtomicUnits": "1500000"
                }
            })
            .to_string(),
        );
        let transfer = &prepared[0]["result"]["transfer"];
        let draft_id = transfer["draftId"]
            .as_str()
            .expect("draft id is returned")
            .to_owned();
        let challenge = transfer["authorizationChallenge"]
            .as_str()
            .expect("authorization challenge is returned");
        let authorized = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "cancel-authorize",
                "method": "wallet.transaction.authorize_unshielded",
                "params": {
                    "draftId": draft_id,
                    "authorizationChallenge": challenge,
                    "confirmation": {
                        "title": "Authorize NIGHT transfer",
                        "summary": "Authorize the cancellable headless transfer",
                        "confirmed": true
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(authorized[0]["result"]["transfer"]["state"], "authorized");

        let started = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "cancel-start",
                "method": "wallet.transaction.start_submission",
                "params": {
                    "draftId": draft_id,
                    "confirmation": {
                        "title": "Submit NIGHT transfer",
                        "summary": "Start the cancellable headless transfer",
                        "confirmed": true
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(started[0]["result"]["submissionStatus"]["state"], "running");
        assert_eq!(
            started[0]["result"]["submissionStatus"]["cancellationAllowed"],
            true
        );
        let cancelled = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "cancel-request",
                "method": "wallet.transaction.cancel_submission",
                "params": { "draftId": draft_id }
            })
            .to_string(),
        );
        assert_eq!(
            cancelled[0]["result"]["submissionStatus"]["state"],
            "cancellation_requested"
        );

        let status_request = json!({
            "protocol": PROTOCOL_VERSION,
            "id": "cancel-status",
            "method": "wallet.transaction.submission_status",
            "params": { "draftId": draft_id }
        })
        .to_string();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        let final_status = loop {
            let response = execute_with_wallet(&wallet, &status_request);
            if response[0]["result"]["submissionStatus"]["state"] == "cancelled" {
                break response;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "submission cancellation was not acknowledged"
            );
            thread::yield_now();
        };
        assert_eq!(
            final_status[0]["result"]["submissionStatus"]["retryable"],
            true
        );
        assert!(!final_status[0].to_string().contains("transactionHex"));

        let retried = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "cancel-retry",
                "method": "wallet.transaction.submit_unshielded",
                "params": {
                    "draftId": draft_id,
                    "confirmation": {
                        "title": "Retry NIGHT transfer",
                        "summary": "Retry only after pre-broadcast cancellation was acknowledged",
                        "confirmed": true
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(
            retried[0]["result"]["submission"]["transfer"]["state"],
            "submitted"
        );
    }

    #[test]
    fn rejects_secret_bearing_security_parameters_without_echoing_them() {
        let responses = execute(concat!(
            r#"{"protocol":"oxid.headless.v1","id":"secret-passphrase","method":"wallet.security.initialize","params":{"passphrase":"never-echo-this"}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"secret-seed","method":"wallet.key.generate","params":{"label":"Key","algorithm":"ed25519","purpose":"authentication","seedHex":"deadbeef-private"}}"#,
        ));

        assert_eq!(responses[0]["error"]["code"], "invalid_params");
        assert_eq!(responses[1]["error"]["code"], "invalid_params");
        let output = Value::Array(responses).to_string();
        assert!(!output.contains("never-echo-this"));
        assert!(!output.contains("deadbeef-private"));
    }

    #[test]
    fn exposes_initial_resumed_current_and_cancelled_dust_flows() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"dust-create","method":"wallet.profile.create","params":{"displayName":"DUST flow"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile id is returned");
        let setup = format!(
            "{}\n{}",
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "dust-select",
                "method": "wallet.profile.select",
                "params": { "profileId": profile_id }
            }),
            r#"{"protocol":"oxid.headless.v1","id":"dust-init","method":"wallet.security.initialize","params":{}}"#,
        );
        assert!(
            execute_with_wallet(&wallet, &setup)
                .iter()
                .all(|response| response["ok"] == true)
        );

        let initial_and_cancelled = execute_with_wallet(
            &wallet,
            concat!(
                r#"{"protocol":"oxid.headless.v1","id":"dust-initial","method":"wallet.dust.sync.status","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"dust-start","method":"wallet.dust.sync.start","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"dust-progress","method":"wallet.dust.sync.status","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"dust-cancel","method":"wallet.dust.sync.cancel","params":{}}"#,
            ),
        );
        assert_eq!(
            initial_and_cancelled[0]["result"]["dustSync"]["state"],
            "never_synced"
        );
        assert_eq!(
            initial_and_cancelled[1]["result"]["dustSync"]["state"],
            "syncing"
        );
        assert_eq!(
            initial_and_cancelled[2]["result"]["dustSync"]["currentCursor"],
            0
        );
        assert_eq!(
            initial_and_cancelled[2]["result"]["dustSync"]["targetCursor"],
            2
        );
        assert_eq!(
            initial_and_cancelled[3]["result"]["dustSync"]["state"],
            "cancelled"
        );

        let resumed_and_current = execute_with_wallet(
            &wallet,
            concat!(
                r#"{"protocol":"oxid.headless.v1","id":"dust-resume","method":"wallet.dust.sync.start","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"dust-resumed-progress","method":"wallet.dust.sync.status","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"dust-complete","method":"wallet.dust.sync.status","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"dust-current-start","method":"wallet.dust.sync.start","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"dust-current","method":"wallet.dust.sync.status","params":{}}"#,
            ),
        );
        assert_eq!(
            resumed_and_current[0]["result"]["dustSync"]["currentCursor"],
            0
        );
        assert_eq!(
            resumed_and_current[1]["result"]["dustSync"]["currentCursor"],
            1
        );
        let completed = &resumed_and_current[2]["result"]["dustSync"];
        assert_eq!(completed["state"], "synced");
        assert_eq!(completed["currentCursor"], completed["targetCursor"]);
        assert_eq!(completed["balance"]["atomicUnits"], "12000000000000000");
        assert_eq!(
            resumed_and_current[4]["result"]["dustSync"]["state"],
            "synced"
        );
        assert_eq!(
            resumed_and_current[4]["result"]["dustSync"]["eventsProcessed"],
            0
        );
    }

    #[test]
    fn exposes_exact_resumable_shielded_flow_without_secret_material() {
        let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
        let created = execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"shielded-create","method":"wallet.profile.create","params":{"displayName":"Shielded flow"}}"#,
        );
        let profile_id = created[0]["result"]["profile"]["id"]
            .as_str()
            .expect("profile id is returned");
        let setup = format!(
            "{}\n{}\n{}",
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "shielded-select",
                "method": "wallet.profile.select",
                "params": { "profileId": profile_id }
            }),
            r#"{"protocol":"oxid.headless.v1","id":"shielded-init","method":"wallet.security.initialize","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"shielded-derive","method":"wallet.account.derive","params":{"accountIndex":0,"addressIndex":0}}"#,
        );
        assert!(
            execute_with_wallet(&wallet, &setup)
                .iter()
                .all(|response| response["ok"] == true)
        );

        let initial_and_cancelled = execute_with_wallet(
            &wallet,
            concat!(
                r#"{"protocol":"oxid.headless.v1","id":"shielded-initial","method":"wallet.shielded.sync.status","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"shielded-start","method":"wallet.shielded.sync.start","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"shielded-progress","method":"wallet.shielded.sync.status","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"shielded-cancel","method":"wallet.shielded.sync.cancel","params":{}}"#,
            ),
        );
        assert_eq!(
            initial_and_cancelled[0]["result"]["shieldedSync"]["state"],
            "never_synced"
        );
        assert_eq!(
            initial_and_cancelled[2]["result"]["shieldedSync"]["commitmentCount"],
            1
        );
        assert_eq!(
            initial_and_cancelled[3]["result"]["shieldedSync"]["state"],
            "cancelled"
        );

        let completed = execute_with_wallet(
            &wallet,
            concat!(
                r#"{"protocol":"oxid.headless.v1","id":"shielded-resume","method":"wallet.shielded.sync.start","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"shielded-progress-2","method":"wallet.shielded.sync.status","params":{}}"#,
                "\n",
                r#"{"protocol":"oxid.headless.v1","id":"shielded-complete","method":"wallet.shielded.sync.status","params":{}}"#,
            ),
        );
        let synced = &completed[2]["result"]["shieldedSync"];
        assert_eq!(synced["state"], "synced");
        assert_eq!(synced["ownedNoteCount"], 1);
        assert_eq!(synced["commitmentCount"], 3);
        assert_eq!(synced["balances"][0]["atomicUnits"], "5000000");
        assert_eq!(
            synced["balances"][0]["tokenType"],
            "0000000000000000000000000000000000000000000000000000000000000000"
        );
        let encoded = serde_json::to_string(&completed).expect("responses serialize");
        assert!(!encoded.contains("seed"));
        assert!(!encoded.contains("private"));
        assert!(!encoded.contains("mnemonic"));
    }
}
