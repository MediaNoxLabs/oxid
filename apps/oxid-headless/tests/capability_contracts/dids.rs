// SPDX-License-Identifier: Apache-2.0

use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use serde_json::json;

use super::support::execute_with_wallet;

#[test]
fn exercises_the_complete_standalone_did_lifecycle_without_key_handles() {
    let wallet =
        HeadlessWallet::new(oxid_composition::compose_in_memory_with_development_did_approval());
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
        3
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
    assert_eq!(unconfirmed[0]["error"]["code"], "invalid_params");

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
    for (index, params) in operations.into_iter().enumerate() {
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

    let holder_signed = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "did-sign-holder-jubjub",
            "method": "did.sign",
            "params": {
                "did": did,
                "methodId": "#holder-jubjub-1",
                "payloadHex": "686f6c6465722d6368616c6c656e6765",
            },
        })
        .to_string(),
    );
    assert_eq!(holder_signed[0]["result"]["algorithm"], "jubjub");
    assert_eq!(
        holder_signed[0]["result"]["signatureHex"]
            .as_str()
            .expect("Jubjub signature")
            .len(),
        192
    );
    assert!(!holder_signed[0].to_string().contains("key_"));

    let removals = [
        json!({ "operation": "removeVerificationRelationship", "did": did, "relationship": "assertionMethod", "methodId": "#recovery-1" }),
        json!({ "operation": "removeVerificationMethod", "did": did, "methodId": "#recovery-1" }),
        json!({ "operation": "removeService", "did": did, "id": "#messages" }),
        json!({ "operation": "removeAlsoKnownAs", "did": did, "value": "https://example.test/alice" }),
    ];
    for (index, params) in removals.into_iter().enumerate() {
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
            },
        })
        .to_string(),
    );
    assert_eq!(denied[0]["error"]["code"], "failed_precondition");
}

#[test]
fn creates_and_resolves_an_offchain_demo_identity_without_network_configuration() {
    let wallet =
        HeadlessWallet::new(oxid_composition::compose_in_memory_with_development_did_approval());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"profile","method":"wallet.profile.create","params":{"displayName":"Offline identity"}}"#,
    );
    let profile = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile id");
    let setup = format!(
        "{}\n{}\n{}",
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile },
        }),
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "security",
            "method": "wallet.security.initialize",
            "params": {},
        }),
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "offchain-create",
            "method": "did.create",
            "params": { "network": "offchain" },
        }),
    );
    let responses = execute_with_wallet(&wallet, &setup);
    assert_eq!(
        responses[2]["ok"], true,
        "unexpected response: {responses:?}"
    );
    let did = responses[2]["result"]["didRecord"]["document"]["id"]
        .as_str()
        .expect("off-chain DID");
    assert!(did.starts_with("did:midnight:offchain:"));
    assert_eq!(did.split(':').count(), 5);
    assert!(!responses[2].to_string().contains("private"));

    let resolved = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "offchain-resolve",
            "method": "did.resolve",
            "params": { "did": did },
        })
        .to_string(),
    );
    assert_eq!(resolved[0]["ok"], true, "unexpected response: {resolved:?}");
    assert_eq!(resolved[0]["result"]["didRecord"]["document"]["id"], did);

    let rejected = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "offchain-resolve-rejected",
            "method": "did.resolve",
            "params": { "did": "did:midnight:offchain:invalid:document" },
        })
        .to_string(),
    );
    assert_eq!(rejected[0]["ok"], false);

    let diagnostics = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"offchain-diagnostics","method":"system.diagnostics.snapshot","params":{}}"#,
    );
    let recent = diagnostics[0]["result"]["diagnostics"]["recent"]
        .as_array()
        .expect("diagnostic events");
    assert_eq!(recent.len(), 3);
    assert_eq!(
        recent[0]["code"],
        "identity.did.offchain.creation.succeeded"
    );
    assert_eq!(
        recent[1]["code"],
        "identity.did.offchain.resolution.succeeded"
    );
    assert_eq!(recent[2]["code"], "identity.did.offchain.resolution.failed");
    assert_eq!(
        diagnostics[0]["result"]["diagnostics"]["payloadsRetained"],
        false
    );
    assert!(!diagnostics[0].to_string().contains(did));

    let locked_wallet =
        HeadlessWallet::new(oxid_composition::compose_in_memory_with_development_did_approval());
    let locked_profile = execute_with_wallet(
        &locked_wallet,
        r#"{"protocol":"oxid.headless.v1","id":"locked-profile","method":"wallet.profile.create","params":{"displayName":"Locked identity"}}"#,
    );
    let locked_profile_id = locked_profile[0]["result"]["profile"]["id"]
        .as_str()
        .expect("locked profile id");
    let locked = execute_with_wallet(
        &locked_wallet,
        &format!(
            "{}\n{}\n{}",
            json!({"protocol": PROTOCOL_VERSION, "id": "locked-select", "method": "wallet.profile.select", "params": {"profileId": locked_profile_id}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "locked-create", "method": "did.create", "params": {"network": "offchain"}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "locked-diagnostics", "method": "system.diagnostics.snapshot", "params": {}}),
        ),
    );
    assert_eq!(locked[1]["ok"], false);
    assert_eq!(
        locked[2]["result"]["diagnostics"]["recent"][0]["code"],
        "identity.did.offchain.creation.failed"
    );
}

#[test]
fn ledger_deployment_is_secret_free_and_fails_closed_when_not_composed() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"deploy-profile","method":"wallet.profile.create","params":{"displayName":"Deployment flow"}}"#,
    );
    let profile = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile id");
    let setup = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}",
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "deploy-select",
                "method": "wallet.profile.select",
                "params": { "profileId": profile },
            }),
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "deploy-did",
                "method": "did.deploy",
                "params": { "network": "undeployed", "accountIndex": 0 },
            }),
        ),
    );

    assert_eq!(setup[0]["ok"], true);
    assert_eq!(setup[1]["error"]["code"], "capability_unavailable");
    assert!(!setup[1].to_string().contains("seed"));
    assert!(!setup[1].to_string().contains("mnemonic"));

    let invalid = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"deploy-offchain","method":"did.deploy","params":{"network":"offchain"}}"#,
    );
    assert_eq!(invalid[0]["error"]["code"], "invalid_params");
}

#[test]
fn default_headless_did_commands_report_approval_unavailable_and_reject_json_authority() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"profile","method":"wallet.profile.create","params":{"displayName":"Closed DID"}}"#,
    );
    let profile = created[0]["result"]["profile"]["id"].as_str().unwrap();
    execute_with_wallet(&wallet, &json!({"protocol": PROTOCOL_VERSION, "id": "select", "method": "wallet.profile.select", "params": {"profileId": profile}}).to_string());
    let did = format!("did:midnight:undeployed:{:064x}", 1);
    for (method, params) in [
        (
            "did.update",
            json!({"did": did, "operation": "removeService", "id": "#service"}),
        ),
        (
            "did.sign",
            json!({"did": did, "methodId": "#auth-1", "payloadHex": "01"}),
        ),
        ("did.deactivate", json!({"did": did})),
    ] {
        let response = execute_with_wallet(&wallet, &json!({"protocol": PROTOCOL_VERSION, "id": "closed", "method": method, "params": params}).to_string());
        assert_eq!(
            response[0]["error"]["code"], "approval_unavailable",
            "{response:?}"
        );
        for authority in [
            "confirmation",
            "capability",
            "approval",
            "developmentFixture",
        ] {
            let mut forged = params.clone();
            forged[authority] = json!({"confirmed": true, "title": "Approve", "summary": "public input is not authority"});
            let response = execute_with_wallet(&wallet, &json!({"protocol": PROTOCOL_VERSION, "id": "forged", "method": method, "params": forged}).to_string());
            assert_eq!(response[0]["error"]["code"], "invalid_params");
        }
    }
}
