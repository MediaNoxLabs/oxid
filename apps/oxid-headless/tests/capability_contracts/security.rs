// SPDX-License-Identifier: Apache-2.0

use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use serde_json::{Value, json};

use super::support::{execute, execute_with_wallet};

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
fn exercises_jubjub_custody_through_opaque_headless_references() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"jubjub-profile","method":"wallet.profile.create","params":{"displayName":"Jubjub flow"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile identifier");
    let setup = format!(
        "{}\n{}\n{}",
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "jubjub-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }),
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "jubjub-init",
            "method": "wallet.security.initialize",
            "params": {}
        }),
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "jubjub-generate",
            "method": "wallet.key.generate",
            "params": {
                "label": "Compact holder presentation",
                "algorithm": "jubjub",
                "purpose": "assertion"
            }
        })
    );
    let setup = execute_with_wallet(&wallet, &setup);
    assert_eq!(setup[2]["ok"], true, "unexpected response: {setup:?}");
    let key = &setup[2]["result"]["key"];
    assert_eq!(key["algorithm"], "jubjub");
    assert_eq!(key["purpose"], "assertion");
    assert_eq!(key["publicKey"]["encoding"], "jubjub-compressed");
    assert_eq!(
        key["publicKey"]["bytesHex"]
            .as_str()
            .expect("public key bytes")
            .len(),
        64
    );
    let key_ref = key["keyRef"].as_str().expect("opaque key reference");
    assert!(key_ref.starts_with("key_"));
    assert!(key.get("privateKey").is_none());
    assert!(key.get("seed").is_none());

    let flow = format!(
        "{}\n{}",
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "jubjub-sign",
            "method": "wallet.key.sign",
            "params": {
                "keyRef": key_ref,
                "payloadHex": "4f78696420686f6c6465722073746174656d656e74",
                "confirmation": {
                    "title": "Present credential",
                    "summary": "Bind the consented public statement to holder custody.",
                    "confirmed": true
                }
            }
        }),
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "jubjub-list",
            "method": "wallet.key.list",
            "params": {}
        })
    );
    let flowed = execute_with_wallet(&wallet, &flow);
    assert_eq!(flowed[0]["result"]["algorithm"], "jubjub");
    assert_eq!(
        flowed[0]["result"]["signatureHex"]
            .as_str()
            .expect("signature bytes")
            .len(),
        192
    );
    assert_eq!(
        flowed[1]["result"]["keys"].as_array().map(Vec::len),
        Some(1)
    );
    assert!(!flowed[1].to_string().contains("private"));
    assert!(!flowed[1].to_string().contains("seed"));
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
