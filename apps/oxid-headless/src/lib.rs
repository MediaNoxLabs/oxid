// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt, io, io::BufRead, io::Write};

use oxid_composition::ApplicationServices;
use oxid_wallet_application::{
    CreateWalletProfileCommand, CreateWalletProfileError, ReadWalletProfilesError,
    SelectWalletProfileCommand, SelectWalletProfileError, WalletProfileRepositoryError,
    WalletProfileView,
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

fn capability_manifest() -> Value {
    json!([
        { "method": "system.capabilities", "status": "ready" },
        { "method": "system.quit", "status": "ready" },
        { "method": "wallet.profile.create", "status": "ready" },
        { "method": "wallet.profile.list", "status": "ready" },
        { "method": "wallet.profile.select", "status": "ready" },
        { "method": "wallet.profile.active", "status": "ready" },
        { "method": "wallet.connect", "status": "queued" },
        { "method": "wallet.bootstrap", "status": "queued" },
        { "method": "wallet.address.unshielded", "status": "queued" },
        { "method": "wallet.balance.snapshot", "status": "queued" },
        { "method": "wallet.transaction.send_unshielded", "status": "queued" },
        { "method": "wallet.sync.force", "status": "queued" },
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
}
