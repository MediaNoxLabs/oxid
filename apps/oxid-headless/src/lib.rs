// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt, io, io::BufRead, io::Write};

use oxid_composition::ApplicationServices;
use oxid_wallet_application::{
    CreateWalletProfileCommand, CreateWalletProfileError, DeleteWalletKeyCommand,
    GenerateWalletKeyCommand, ReadWalletProfilesError, SelectWalletNetworkCommand,
    SelectWalletProfileCommand, SelectWalletProfileError, SensitiveOperationConfirmation,
    SensitiveWalletOperationError, SignWalletDataCommand, WalletAccountError,
    WalletAccountPortError, WalletAccountQuery, WalletAccountView, WalletKeyError, WalletKeyView,
    WalletNetworkListView, WalletProfileRepositoryError, WalletProfileSecurityCommand,
    WalletProfileView, WalletSecurityError, WalletSecurityPortError, WalletSecurityStatusView,
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
            "wallet.account.get" => self.get_account(request),
            "wallet.address.list" => self.list_addresses(request),
            "wallet.address.unshielded" => self.unshielded_address(request),
            "wallet.balance.snapshot" => self.balance_snapshot(request),
            "wallet.transaction.history" => self.transaction_history(request),
            "wallet.connect" | "wallet.sync.force" => self.sync_account(request),
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
                    "algorithm must be ed25519, p256, or jubjub",
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

fn account_error(id: Option<String>, error: WalletAccountError) -> Response {
    match error {
        WalletAccountError::InvalidProfileIdentifier(_)
        | WalletAccountError::InvalidNetworkIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "profile or network identifier is invalid",
        ),
        WalletAccountError::Port(WalletAccountPortError::NotFound) => {
            Response::error(id, "not_found", "wallet account was not found")
        }
        WalletAccountError::Port(WalletAccountPortError::UnsupportedNetwork) => Response::error(
            id,
            "unsupported_network",
            "selected wallet network is not supported",
        ),
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
        WalletKeyAlgorithm::Jubjub => "jubjub",
    }
}

fn key_algorithm(value: &str) -> Option<WalletKeyAlgorithm> {
    match value {
        "ed25519" => Some(WalletKeyAlgorithm::Ed25519),
        "p256" => Some(WalletKeyAlgorithm::P256),
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
        "wallet.balance.snapshot" => "wallet.balance.snapshot does not accept parameters",
        "wallet.transaction.history" => "wallet.transaction.history does not accept parameters",
        "wallet.connect" => "wallet.connect does not accept parameters",
        "wallet.sync.force" => "wallet.sync.force does not accept parameters",
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
        { "method": "wallet.key.generate", "status": "ready", "mode": "development_only", "algorithms": ["ed25519", "p256"] },
        { "method": "wallet.key.list", "status": "ready", "mode": "development_only" },
        { "method": "wallet.key.sign", "status": "ready", "mode": "development_only" },
        { "method": "wallet.key.delete", "status": "ready", "mode": "development_only" },
        { "method": "wallet.network.list", "status": "ready", "mode": "standalone" },
        { "method": "wallet.network.select", "status": "ready", "mode": "standalone" },
        { "method": "wallet.account.get", "status": "ready", "mode": "standalone", "sources": ["simulated", "live", "cached"] },
        { "method": "wallet.connect", "status": "ready", "mode": "standalone", "sources": ["simulated", "live"] },
        { "method": "wallet.bootstrap", "status": "queued" },
        { "method": "wallet.address.list", "status": "ready", "mode": "standalone", "sources": ["official_public_vectors", "configured_public_address"] },
        { "method": "wallet.address.unshielded", "status": "ready", "mode": "standalone", "sources": ["official_public_vectors", "configured_public_address"] },
        { "method": "wallet.balance.snapshot", "status": "ready", "mode": "standalone", "sources": ["simulated", "live", "cached"] },
        { "method": "wallet.transaction.history", "status": "ready", "mode": "standalone", "sources": ["simulated", "live", "cached"] },
        { "method": "wallet.transaction.send_unshielded", "status": "queued" },
        { "method": "wallet.sync.force", "status": "ready", "mode": "standalone", "sources": ["simulated", "live"] },
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
                && capability["status"] == "queued"
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
}
