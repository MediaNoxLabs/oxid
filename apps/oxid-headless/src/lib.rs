// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]
#![recursion_limit = "256"]

//! Versioned incoming adapter organized according to
//! [ADR-0104](../../../docs/adr/0104-regrow-incoming-adapters-behind-capability-facades.md).
//! `protocol` owns the envelope and stream errors; `parameters`, `projections`,
//! and `errors` own wire translation; capability modules own application-port
//! invocation; this root owns transport, stable re-exports, and dispatch.

mod accounts;
mod dids;
mod errors;
mod identity_protocols;
mod midnight_wallet;
mod parameters;
mod passport_vault;
mod projections;
mod protocol;
mod security;
mod system;
mod wallet_profiles;

pub use protocol::HeadlessIoError;

use std::{io::BufRead, io::Write};

use oxid_composition::ApplicationServices;
use oxid_diagnostics_application::{DiagnosticCode, DiagnosticSeverity};
use serde_json::{Value, json};

use protocol::{Dispatch, Request, Response, request_id};
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
                self.record_diagnostic(
                    DiagnosticCode::HeadlessRequestRejected,
                    DiagnosticSeverity::Warning,
                );
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
                self.record_diagnostic(
                    DiagnosticCode::HeadlessRequestRejected,
                    DiagnosticSeverity::Warning,
                );
                return Dispatch::continue_with(Response::error(None, "invalid_request", message));
            }
        };

        let request = match serde_json::from_value::<Request>(value) {
            Ok(request) => request,
            Err(_) => {
                self.record_diagnostic(
                    DiagnosticCode::HeadlessRequestRejected,
                    DiagnosticSeverity::Warning,
                );
                return Dispatch::continue_with(Response::error(
                    request_id,
                    "invalid_request",
                    "request must include string protocol and method fields",
                ));
            }
        };

        if request.protocol != PROTOCOL_VERSION {
            self.record_diagnostic(
                DiagnosticCode::HeadlessRequestRejected,
                DiagnosticSeverity::Warning,
            );
            return Dispatch::continue_with(Response::error(
                request.id,
                "unsupported_protocol",
                "request protocol is not supported",
            ));
        }

        if !request.params.is_object() {
            self.record_diagnostic(
                DiagnosticCode::HeadlessRequestRejected,
                DiagnosticSeverity::Warning,
            );
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "params must be a JSON object",
            ));
        }

        // BEGIN HEADLESS METHOD ROUTER — protocol_contract.rs inventories these arms.
        match request.method.as_str() {
            "system.capabilities" => self.capabilities(request),
            "system.diagnostics.snapshot" => self.diagnostics_snapshot(request),
            "system.diagnostics.clear" => self.clear_diagnostics(request),
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
            "wallet.transaction.prepare_shielded" => self.prepare_shielded(request),
            "wallet.transaction.authorize_unshielded" => self.authorize_unshielded(request),
            "wallet.transaction.authorize_shielded" => self.authorize_unshielded(request),
            "wallet.transaction.submit_unshielded" | "wallet.transaction.send_unshielded" => {
                self.submit_unshielded(request)
            }
            "wallet.transaction.submit_shielded" | "wallet.transaction.send_shielded" => {
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
            "wallet.dust.registration.prepare" => self.prepare_dust_registration(request),
            "wallet.dust.registration.authorize" => self.authorize_dust_registration(request),
            "wallet.dust.registration.submit" => self.submit_dust_registration(request),
            "wallet.dust.registration.start_submission" => {
                self.start_dust_registration_submission(request)
            }
            "wallet.dust.registration.draft" => self.dust_registration_draft(request),
            "wallet.dust.registration.status" => self.dust_registration_status(request),
            "wallet.dust.registration.cancel_submission" => {
                self.cancel_dust_registration_submission(request)
            }
            "wallet.dust.registration.reconcile_submission" => {
                self.reconcile_dust_registration_submission(request)
            }
            "wallet.shielded.sync.status" => self.shielded_sync_status(request),
            "wallet.shielded.sync.start" => self.start_shielded_sync(request),
            "wallet.shielded.sync.cancel" => self.cancel_shielded_sync(request),
            "vault.total_locked" | "vault.locks.list" => self.list_vault_locks(request),
            "vault.contract_state.decode" => self.decode_vault_contract_state(request),
            "vault.contract_state.read" => self.read_vault_contract_state(request),
            "vault.contract_call.prepare" => self.prepare_vault_contract_call(request),
            "vault.contract_call.authorize" => self.authorize_vault_contract_call(request),
            "vault.contract_call.draft" => self.vault_contract_call_draft(request),
            "vault.contract_call.submit" => self.submit_vault_contract_call(request),
            "vault.contract_call.start_submission" => {
                self.start_vault_contract_call_submission(request)
            }
            "vault.contract_call.submission_status" => {
                self.vault_contract_call_submission_status(request)
            }
            "vault.contract_call.submission_history" => {
                self.vault_contract_call_submission_history(request)
            }
            "vault.contract_call.cancel_submission" => {
                self.cancel_vault_contract_call_submission(request)
            }
            "vault.contract_call.reconcile_submission" => {
                self.reconcile_vault_contract_call_submission(request)
            }
            "vault.lock.create" => self.create_vault_lock(request),
            "vault.deposit" => self.deposit_to_vault_lock(request),
            "vault.claim" => self.claim_from_vault_lock(request),
            "vault.withdraw" => self.withdraw_from_vault_lock(request),
            "did.create" => self.create_did(request),
            "did.resolve" => self.resolve_did(request),
            "did.list" => self.list_dids(request),
            "did.get" => self.get_did(request),
            "did.update" => self.update_did(request),
            "did.sign" => self.sign_did(request),
            "did.deactivate" => self.deactivate_did(request),
            "did.forget" => self.forget_did(request),
            "credential.receive" | "credential.request" => self.receive_credential(request),
            "credential.list" | "vault.credentials.list" => self.list_credentials(request),
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
            "identity.request.route" => self.route_identity_request(request),
            "identity.login" | "identity.authentication.prepare" => {
                self.prepare_self_issued_authentication(request)
            }
            "identity.authentication.accept" => self.accept_self_issued_authentication(request),
            "identity.authentication.refuse" => self.refuse_self_issued_authentication(request),
            "identity.authentication.get" => self.get_self_issued_authentication(request),
            "identity.authentication.list" => self.list_self_issued_authentications(request),
            _ => {
                self.record_diagnostic(
                    DiagnosticCode::HeadlessMethodNotFound,
                    DiagnosticSeverity::Warning,
                );
                Dispatch::continue_with(Response::error(
                    request.id,
                    "method_not_found",
                    "requested method is not implemented",
                ))
            }
        }
        // END HEADLESS METHOD ROUTER
    }

    fn active_profile_id(&self, id: Option<String>) -> Result<String, Response> {
        match self.application.get_active_wallet_profile().execute() {
            Ok(Some(profile)) => Ok(profile.id),
            Ok(None) => Err(Response::error(
                id,
                "failed_precondition",
                "an active wallet profile is required",
            )),
            Err(error) => Err(wallet_profiles::read_profiles_error(id, error)),
        }
    }
}
