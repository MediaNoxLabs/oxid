// SPDX-License-Identifier: Apache-2.0

use oxid_wallet_application::{
    CreateWalletProfileCommand, CreateWalletProfileError, ReadWalletProfilesError,
    SelectWalletProfileCommand, SelectWalletProfileError, WalletProfileRepositoryError,
    WalletProfileView,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Dispatch, HeadlessWallet, Request, Response, params_are_empty};

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

impl HeadlessWallet {
    pub(super) fn create_profile(&self, request: Request) -> Dispatch {
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

    pub(super) fn list_profiles(&self, request: Request) -> Dispatch {
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

    pub(super) fn select_profile(&self, request: Request) -> Dispatch {
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

    pub(super) fn active_profile(&self, request: Request) -> Dispatch {
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

pub(super) fn read_profiles_error(id: Option<String>, error: ReadWalletProfilesError) -> Response {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PROTOCOL_VERSION;

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
}
