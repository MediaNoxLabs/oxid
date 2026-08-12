// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt, io, io::BufRead, io::Write};

use oxid_composition::ApplicationServices;
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
    WalletTransferDraftQuery, WalletTransferPreviewView, WalletTransferSubmissionView,
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
            "wallet.transaction.draft" => self.transaction_draft(request),
            "wallet.connect" | "wallet.sync.force" => self.sync_account(request),
            "wallet.dust.sync.status" => self.dust_sync_status(request),
            "wallet.dust.sync.start" => self.start_dust_sync(request),
            "wallet.dust.sync.cancel" => self.cancel_dust_sync(request),
            "wallet.shielded.sync.status" => self.shielded_sync_status(request),
            "wallet.shielded.sync.start" => self.start_shielded_sync(request),
            "wallet.shielded.sync.cancel" => self.cancel_shielded_sync(request),
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

fn transfer_asset_value(asset: &oxid_wallet_application::WalletTransferAssetView) -> Value {
    json!({
        "assetId": asset.asset_id,
        "symbol": asset.symbol,
        "decimals": asset.decimals,
        "atomicUnits": asset.atomic_units,
    })
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
        WalletTransactionPortError::SubmissionCancelled => Response::error(
            id,
            "submission_cancelled",
            "transaction submission was cancelled before broadcast",
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
        { "method": "wallet.sync.force", "status": "ready", "mode": "standalone", "sources": ["simulated", "live"] },
        { "method": "wallet.dust.sync.status", "status": "ready", "mode": "standalone", "sources": ["simulated", "live", "cached", "unavailable"] },
        { "method": "wallet.dust.sync.start", "status": "ready", "mode": "standalone", "execution": "adapter_worker" },
        { "method": "wallet.dust.sync.cancel", "status": "ready", "mode": "standalone", "checkpoint": "resumable" },
        { "method": "wallet.shielded.sync.status", "status": "ready", "mode": "standalone", "sources": ["simulated", "unavailable"] },
        { "method": "wallet.shielded.sync.start", "status": "ready", "mode": "standalone", "execution": "adapter_session" },
        { "method": "wallet.shielded.sync.cancel", "status": "ready", "mode": "standalone", "checkpoint": "session_resumable" },
        { "method": "vault.total_locked", "status": "queued" },
        { "method": "vault.locks.list", "status": "queued" },
        { "method": "vault.credentials.list", "status": "queued" },
        { "method": "vault.lock.create", "status": "queued" },
        { "method": "vault.deposit", "status": "queued" },
        { "method": "vault.claim", "status": "queued" },
        { "method": "identity.login", "status": "queued" },
        { "method": "credential.request", "status": "queued" },
        { "method": "credential.verify", "status": "queued" },
        { "method": "did.create", "status": "queued" },
        { "method": "did.resolve", "status": "queued" },
        { "method": "did.update", "status": "queued" },
        { "method": "did.deactivate", "status": "queued" },
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
