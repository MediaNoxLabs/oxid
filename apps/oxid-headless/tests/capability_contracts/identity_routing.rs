// SPDX-License-Identifier: Apache-2.0

use oxid_adapter_openid4vci::standalone_credential_offer;
use oxid_adapter_siopv2::standalone_self_issued_request;
use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use serde_json::json;

use super::support::{execute, execute_with_wallet};

#[test]
fn routes_scanned_identity_links_without_echoing_protocol_secrets() {
    let unknown = "openid4vp://authorize?client_id=https%3A%2F%2Funknown.example&request_uri=https%3A%2F%2Funknown.example%2Frequest";
    let input = [
            json!({"protocol": PROTOCOL_VERSION, "id": "route-offer", "method": "identity.request.route", "params": {"requestUri": standalone_credential_offer()}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "route-login", "method": "identity.request.route", "params": {"requestUri": standalone_self_issued_request()}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "route-presentation", "method": "identity.request.route", "params": {"requestUri": oxid_composition::standalone_openid4vp_request()}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "route-unknown", "method": "identity.request.route", "params": {"requestUri": unknown}}),
        ]
        .map(|request| request.to_string())
        .join("\n");

    let responses = execute(&input);
    assert_eq!(
        responses[0]["result"]["route"]["kind"],
        "credential_issuance"
    );
    assert_eq!(
        responses[1]["result"]["route"]["kind"],
        "self_issued_authentication"
    );
    assert_eq!(
        responses[2]["result"]["route"]["kind"],
        "credential_presentation"
    );
    assert_eq!(responses[3]["error"]["code"], "ambiguous_identity_request");

    let serialized = serde_json::to_string(&responses).expect("responses");
    assert!(!serialized.contains("credential_offer"));
    assert!(!serialized.contains("127.0.0.1"));
    assert!(!serialized.contains("unknown.example"));
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
