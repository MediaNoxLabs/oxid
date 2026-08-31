// SPDX-License-Identifier: Apache-2.0

use oxid_adapter_openid4vci::standalone_credential_offer;
use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use serde_json::json;

use super::support::execute_with_wallet;

#[test]
#[ignore = "requires the authenticated p18 Compact proving artifact closure"]
fn proves_and_independently_verifies_a_compact_presentation_end_to_end() {
    let artifact_root = std::env::var_os("OXID_PRESENTATION_ARTIFACTS_DIR")
        .expect("set OXID_PRESENTATION_ARTIFACTS_DIR to the Nix artifact closure");
    let application =
        oxid_composition::compose_in_memory_with_compact_presentation_artifacts(artifact_root)
            .expect("authenticated Compact runtime");
    let wallet = HeadlessWallet::new(application);
    let capabilities = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"zk-capabilities","method":"system.capabilities","params":{}}"#,
    );
    let presentation_capability = capabilities[0]["result"]["methods"]
        .as_array()
        .expect("capabilities")
        .iter()
        .find(|capability| capability["method"] == "credential.presentation.accept")
        .expect("presentation capability");
    assert_eq!(presentation_capability["status"], "ready");
    assert_eq!(presentation_capability["proofAvailable"], true);
    assert_eq!(presentation_capability["generatesPresentation"], true);

    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"zk-profile","method":"wallet.profile.create","params":{"displayName":"ZK presentation flow"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile identifier")
        .to_owned();
    let initialized = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}",
            json!({"protocol": PROTOCOL_VERSION, "id": "zk-select", "method": "wallet.profile.select", "params": {"profileId": profile_id}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "zk-security", "method": "wallet.security.initialize", "params": {}}),
        ),
    );
    assert!(initialized.iter().all(|response| response["ok"] == true));

    let created_did = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"zk-did","method":"did.create","params":{}}"#,
    );
    let document = &created_did[0]["result"]["didRecord"]["document"];
    let did = document["id"].as_str().expect("holder DID").to_owned();
    let method_id = document["relationships"]
        .as_array()
        .expect("relationships")
        .iter()
        .find(|relationship| relationship["relationship"] == "authentication")
        .and_then(|relationship| relationship["methodIds"][0].as_str())
        .expect("authentication method")
        .to_owned();
    let holder_binding_method_id = document["verificationMethods"]
        .as_array()
        .expect("verification methods")
        .iter()
        .find(|method| method["publicKeyJwk"]["crv"] == "Jubjub")
        .and_then(|method| method["id"].as_str())
        .expect("Jubjub holder method")
        .to_owned();

    let prepared_issuance = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "zk-issuance-prepare",
            "method": "credential.issuance.prepare",
            "params": {"offer": standalone_credential_offer()},
        })
        .to_string(),
    );
    let issuance_id = prepared_issuance[0]["result"]["issuance"]["id"]
        .as_str()
        .expect("issuance identifier");
    let issued = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "zk-issuance-accept",
            "method": "credential.issuance.accept",
            "params": {
                "issuanceId": issuance_id,
                "holderDid": did,
                "methodId": method_id,
                "holderBindingMethodId": holder_binding_method_id,
                "confirmed": true,
                "intent": "ACCEPT_CREDENTIAL_ISSUANCE",
            },
        })
        .to_string(),
    );
    assert_eq!(issued[0]["result"]["issuance"]["state"], "succeeded");
    let credential_id = issued[0]["result"]["issuance"]["credentialId"]
        .as_str()
        .expect("credential identifier")
        .to_owned();

    let prepared_presentation = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "zk-presentation-prepare",
            "method": "credential.presentation.prepare",
            "params": {"request": oxid_composition::standalone_openid4vp_request()},
        })
        .to_string(),
    );
    let presentation_id = prepared_presentation[0]["result"]["presentation"]["id"]
        .as_str()
        .expect("presentation identifier");
    let accepted = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "zk-presentation-accept",
            "method": "credential.presentation.accept",
            "params": {
                "presentationId": presentation_id,
                "credentialId": credential_id,
                "confirmed": true,
                "intent": "ACCEPT_CREDENTIAL_PRESENTATION",
            },
        })
        .to_string(),
    );
    let presentation = &accepted[0]["result"]["presentation"];
    assert_eq!(presentation["state"], "succeeded");
    assert_eq!(presentation["presentationGenerated"], true);
    assert_eq!(presentation["verifierValidated"], true);
    assert!(presentation["failureCode"].is_null());

    let public_result = accepted[0].to_string();
    assert!(!public_result.contains("vp_token"));
    assert!(!public_result.contains("Alice"));
    assert!(!public_result.contains("Example"));
    assert!(!public_result.contains("AB1234567"));
    assert!(!public_result.contains("privateMaterial"));

    let replayed = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "zk-presentation-replay",
            "method": "credential.presentation.accept",
            "params": {
                "presentationId": presentation_id,
                "credentialId": credential_id,
                "confirmed": true,
                "intent": "ACCEPT_CREDENTIAL_PRESENTATION",
            },
        })
        .to_string(),
    );
    assert_eq!(replayed[0]["error"]["code"], "failed_precondition");
    assert!(!replayed[0].to_string().contains("vp_token"));
}
