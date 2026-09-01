// SPDX-License-Identifier: Apache-2.0

use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use serde_json::json;

use super::support::{execute, execute_with_wallet};

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
