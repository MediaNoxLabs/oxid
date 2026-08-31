// SPDX-License-Identifier: Apache-2.0

use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use serde_json::json;

use super::support::execute_with_wallet;

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
                "confirmation": {
                    "title": "Sign holder challenge",
                    "summary": "Authorize the holder challenge with the DID-bound Jubjub method",
                    "confirmed": true,
                },
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
