// SPDX-License-Identifier: Apache-2.0

use oxid_adapter_openid4vci::standalone_credential_offer;
use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use serde_json::json;

use super::support::execute_with_wallet;

#[test]
fn receives_reverifies_and_deletes_a_credential_without_exposing_wire_bytes() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"credential-profile","method":"wallet.profile.create","params":{"displayName":"Credential flow"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile");
    let select = json!({"protocol": PROTOCOL_VERSION, "id": "credential-select", "method": "wallet.profile.select", "params": {"profileId": profile_id}}).to_string();
    assert_eq!(execute_with_wallet(&wallet, &select)[0]["ok"], true);
    let received = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"credential-receive","method":"credential.receive","params":{}}"#,
    );
    let credential = &received[0]["result"]["credential"];
    assert_eq!(credential["verification"]["outcome"], "valid");
    assert_eq!(
        credential["verification"]["stages"]
            .as_array()
            .map(Vec::len),
        Some(7)
    );
    assert!(credential.get("signedBytes").is_none());
    let credential_id = credential["id"].as_str().expect("credential id");
    let requests = format!(
        "{}\n{}\n{}\n{}",
        json!({"protocol": PROTOCOL_VERSION, "id": "credential-list", "method": "credential.list", "params": {}}),
        json!({"protocol": PROTOCOL_VERSION, "id": "credential-verify", "method": "credential.reverify", "params": {"credentialId": credential_id}}),
        json!({"protocol": PROTOCOL_VERSION, "id": "credential-delete-denied", "method": "credential.delete", "params": {"credentialId": credential_id, "confirmed": false, "intent": "DELETE_CREDENTIAL"}}),
        json!({"protocol": PROTOCOL_VERSION, "id": "credential-delete", "method": "credential.delete", "params": {"credentialId": credential_id, "confirmed": true, "intent": "DELETE_CREDENTIAL"}}),
    );
    let responses = execute_with_wallet(&wallet, &requests);
    assert_eq!(
        responses[0]["result"]["credentials"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        responses[1]["result"]["credential"]["verification"]["outcome"],
        "valid"
    );
    assert_eq!(responses[2]["error"]["code"], "confirmation_required");
    assert_eq!(responses[3]["result"]["deleted"], true);
}

#[test]
fn issues_and_stores_a_verified_credential_through_the_headless_flow() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"issuance-profile","method":"wallet.profile.create","params":{"displayName":"Issuance flow"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile identifier");
    let initialization = format!(
        "{}\n{}",
        json!({"protocol": PROTOCOL_VERSION, "id": "issuance-select", "method": "wallet.profile.select", "params": {"profileId": profile_id}}),
        json!({"protocol": PROTOCOL_VERSION, "id": "issuance-security", "method": "wallet.security.initialize", "params": {}}),
    );
    let initialized = execute_with_wallet(&wallet, &initialization);
    assert_eq!(initialized[0]["ok"], true);
    assert_eq!(initialized[1]["ok"], true);

    let created_did = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"issuance-did","method":"did.create","params":{}}"#,
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
    let holder_binding_method_id = record["verificationMethods"]
        .as_array()
        .expect("verification methods")
        .iter()
        .find(|method| method["publicKeyJwk"]["crv"] == "Jubjub")
        .and_then(|method| method["id"].as_str())
        .expect("managed Jubjub holder-binding method");

    let prepared = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "issuance-prepare",
            "method": "credential.issuance.prepare",
            "params": {"offer": standalone_credential_offer()},
        })
        .to_string(),
    );
    assert_eq!(
        prepared[0]["result"]["issuance"]["state"],
        "awaiting_consent"
    );
    assert!(prepared[0].to_string().contains("Digital Passport"));
    assert!(!prepared[0].to_string().contains("pre-authorized"));
    let issuance_id = prepared[0]["result"]["issuance"]["id"]
        .as_str()
        .expect("issuance identifier");

    let denied = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "issuance-denied",
                "method": "credential.issuance.accept",
                "params": {"issuanceId": issuance_id, "holderDid": did, "methodId": method_id, "holderBindingMethodId": holder_binding_method_id, "confirmed": false, "intent": "ACCEPT_CREDENTIAL_ISSUANCE"},
            })
            .to_string(),
        );
    assert_eq!(denied[0]["error"]["code"], "confirmation_required");

    let accepted = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "issuance-accept",
                "method": "credential.issuance.accept",
                "params": {"issuanceId": issuance_id, "holderDid": did, "methodId": method_id, "holderBindingMethodId": holder_binding_method_id, "confirmed": true, "intent": "ACCEPT_CREDENTIAL_ISSUANCE"},
            })
            .to_string(),
        );
    assert_eq!(accepted[0]["result"]["issuance"]["state"], "succeeded");
    let credential_id = accepted[0]["result"]["issuance"]["credentialId"]
        .as_str()
        .expect("credential identifier")
        .to_owned();
    assert!(!accepted[0].to_string().contains("credential_offer"));

    let inventories = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}",
            json!({"protocol": PROTOCOL_VERSION, "id": "issuance-list", "method": "credential.issuance.list", "params": {}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "issued-credentials", "method": "credential.list", "params": {}}),
        ),
    );
    assert_eq!(
        inventories[0]["result"]["issuances"][0]["state"],
        "succeeded"
    );
    assert_eq!(
        inventories[1]["result"]["credentials"][0]["verification"]["outcome"],
        "valid"
    );
    assert_eq!(
        inventories[1]["result"]["credentials"][0]["format"],
        "midnight_compact_vc"
    );
    assert_eq!(
        inventories[1]["result"]["credentials"][0]["displayName"],
        "Digital Passport"
    );
    assert_eq!(
        inventories[1]["result"]["credentials"][0]["subjectDid"],
        did
    );
    let stages = inventories[1]["result"]["credentials"][0]["verification"]["stages"]
        .as_array()
        .expect("verification stages");
    let stage_status = |name: &str| {
        stages
            .iter()
            .find(|stage| stage["name"] == name)
            .and_then(|stage| stage["status"].as_str())
    };
    assert_eq!(stage_status("issuer"), Some("passed"));
    assert_eq!(stage_status("temporal"), Some("passed"));
    assert_eq!(stage_status("trust"), Some("passed"));
    assert_eq!(stage_status("status"), Some("not_checked"));
    let issued_inventory = inventories[1].to_string();
    assert!(!issued_inventory.contains("signedBytes"));
    assert!(!issued_inventory.contains("detachedProof"));
    assert!(!issued_inventory.contains("privateMaterial"));

    let mismatched_prepared = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "issuance-mismatch-prepare",
            "method": "credential.issuance.prepare",
            "params": {"offer": standalone_credential_offer()},
        })
        .to_string(),
    );
    let mismatched_issuance_id = mismatched_prepared[0]["result"]["issuance"]["id"]
        .as_str()
        .expect("mismatched issuance identifier");
    let mismatched = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "issuance-mismatch",
            "method": "credential.issuance.accept",
            "params": {
                "issuanceId": mismatched_issuance_id,
                "holderDid": did,
                "methodId": method_id,
                "holderBindingMethodId": method_id,
                "confirmed": true,
                "intent": "ACCEPT_CREDENTIAL_ISSUANCE",
            },
        })
        .to_string(),
    );
    assert_eq!(mismatched[0]["error"]["code"], "invalid_proof");

    let other_profile = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"issuance-other-profile","method":"wallet.profile.create","params":{"displayName":"Other holder"}}"#,
    );
    let other_profile_id = other_profile[0]["result"]["profile"]["id"]
        .as_str()
        .expect("other profile identifier");
    let select_other = json!({
        "protocol": PROTOCOL_VERSION,
        "id": "issuance-select-other",
        "method": "wallet.profile.select",
        "params": {"profileId": other_profile_id},
    })
    .to_string();
    assert_eq!(execute_with_wallet(&wallet, &select_other)[0]["ok"], true);
    let isolated = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "disclosure-other-profile",
            "method": "credential.disclosure.candidates",
            "params": {"credentialId": credential_id.clone()},
        })
        .to_string(),
    );
    assert_eq!(isolated[0]["error"]["code"], "not_found");
    let select_owner = json!({
        "protocol": PROTOCOL_VERSION,
        "id": "issuance-select-owner",
        "method": "wallet.profile.select",
        "params": {"profileId": profile_id},
    })
    .to_string();
    assert_eq!(execute_with_wallet(&wallet, &select_owner)[0]["ok"], true);

    let original_holder_x = record["verificationMethods"]
        .as_array()
        .expect("verification methods")
        .iter()
        .find(|method| method["id"] == holder_binding_method_id)
        .and_then(|method| method["publicKeyJwk"]["x"].as_str())
        .expect("original holder x-coordinate")
        .to_owned();
    let rotated_holder = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "presentation-holder-rotate",
                "method": "did.update",
                "params": {
                    "operation": "updateVerificationMethod",
                    "did": did,
                    "methodId": holder_binding_method_id,
                    "algorithm": "jubjub",
                    "confirmation": {
                        "title": "Rotate presentation key",
                        "summary": "Authorize the current DID method to replace its protected presentation key.",
                        "confirmed": true
                    }
                }
            })
            .to_string(),
        );
    let rotated_holder_x =
        rotated_holder[0]["result"]["didRecord"]["document"]["verificationMethods"]
            .as_array()
            .expect("rotated verification methods")
            .iter()
            .find(|method| method["id"] == holder_binding_method_id)
            .and_then(|method| method["publicKeyJwk"]["x"].as_str())
            .expect("rotated holder x-coordinate");
    assert_ne!(rotated_holder_x, original_holder_x);

    let disclosure = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}",
            json!({"protocol": PROTOCOL_VERSION, "id": "disclosure-candidates", "method": "credential.disclosure.candidates", "params": {"credentialId": credential_id.clone()}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "disclosure-preview", "method": "credential.disclosure.preview", "params": {
                "credentialId": credential_id,
                "revealClaimPaths": ["/credentialSubject/firstName", "/credentialSubject/lastName"],
                "predicates": [{"claimPath": "/credentialSubject/dateOfBirth", "kind": "age_over", "threshold": 18}]
            }}),
        ),
    );
    assert_eq!(
        disclosure[0]["result"]["disclosure"]["schemaId"],
        "digital-passport:v1"
    );
    assert_eq!(
        disclosure[0]["result"]["disclosure"]["candidates"]
            .as_array()
            .map(Vec::len),
        Some(5)
    );
    assert_eq!(
        disclosure[1]["result"]["plan"]["outcome"],
        "local_preview_ready"
    );
    assert_eq!(
        disclosure[1]["result"]["plan"]["presentationGenerated"],
        false
    );
    let disclosure_json = serde_json::to_string(&disclosure).expect("serialize responses");
    assert!(!disclosure_json.contains("Alice"));
    assert!(!disclosure_json.contains("Example"));
    assert!(!disclosure_json.contains("AB1234567"));

    let prepared_presentation = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "presentation-prepare",
            "method": "credential.presentation.prepare",
            "params": {"request": oxid_composition::standalone_openid4vp_request()},
        })
        .to_string(),
    );
    let presentation = &prepared_presentation[0]["result"]["presentation"];
    assert_eq!(presentation["state"], "awaiting_consent");
    assert_eq!(presentation["presentationGenerated"], false);
    assert_eq!(presentation["verifierValidated"], false);
    assert_eq!(
        presentation["requestedClaims"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(presentation["candidates"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        presentation["candidates"][0]["issuer"],
        "did:midnight:undeployed:a4c9483a0c7cdd808056a93334ab97207b38b4363d1da5cbfb78ad256cd689f0"
    );
    let presentation_id = presentation["id"]
        .as_str()
        .expect("presentation identifier");
    let candidate_id = presentation["candidates"][0]["credentialId"]
        .as_str()
        .expect("credential candidate identifier");
    let preview_json = prepared_presentation[0].to_string();
    assert!(!preview_json.contains("Alice"));
    assert!(!preview_json.contains("Example"));
    assert!(!preview_json.contains("AB1234567"));

    let denied_presentation = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "presentation-denied",
            "method": "credential.presentation.accept",
            "params": {
                "presentationId": presentation_id,
                "credentialId": candidate_id,
                "confirmed": false,
                "intent": "ACCEPT_CREDENTIAL_PRESENTATION"
            },
        })
        .to_string(),
    );
    assert_eq!(
        denied_presentation[0]["error"]["code"],
        "confirmation_required"
    );

    let blocked_presentation = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "presentation-accept",
            "method": "credential.presentation.accept",
            "params": {
                "presentationId": presentation_id,
                "credentialId": candidate_id,
                "confirmed": true,
                "intent": "ACCEPT_CREDENTIAL_PRESENTATION"
            },
        })
        .to_string(),
    );
    assert_eq!(
        blocked_presentation[0]["error"]["code"],
        "proof_unavailable"
    );

    let failed_presentation = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "presentation-get",
            "method": "credential.presentation.get",
            "params": {"presentationId": presentation_id},
        })
        .to_string(),
    );
    let failed = &failed_presentation[0]["result"]["presentation"];
    assert_eq!(failed["state"], "failed");
    assert_eq!(failed["failureCode"], "proof_unavailable");
    assert_eq!(failed["presentationGenerated"], false);
    assert_eq!(failed["verifierValidated"], false);
    assert!(!failed_presentation[0].to_string().contains("vp_token"));

    let prepared_while_locked = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "presentation-locked-prepare",
            "method": "credential.presentation.prepare",
            "params": {"request": oxid_composition::standalone_openid4vp_request()},
        })
        .to_string(),
    );
    let locked = &prepared_while_locked[0]["result"]["presentation"];
    let locked_id = locked["id"].as_str().expect("presentation identifier");
    let locked_credential_id = locked["candidates"][0]["credentialId"]
        .as_str()
        .expect("credential candidate");
    assert_eq!(
        execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"presentation-wallet-lock","method":"wallet.security.lock","params":{}}"#,
        )[0]["ok"],
        true
    );
    let rejected_while_locked = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "presentation-locked-accept",
            "method": "credential.presentation.accept",
            "params": {
                "presentationId": locked_id,
                "credentialId": locked_credential_id,
                "confirmed": true,
                "intent": "ACCEPT_CREDENTIAL_PRESENTATION"
            }
        })
        .to_string(),
    );
    assert_eq!(
        rejected_while_locked[0]["error"]["code"],
        "holder_authorization_unavailable"
    );
    assert!(!rejected_while_locked[0].to_string().contains("vp_token"));
    assert_eq!(
        execute_with_wallet(
            &wallet,
            r#"{"protocol":"oxid.headless.v1","id":"presentation-wallet-unlock","method":"wallet.security.unlock","params":{}}"#,
        )[0]["ok"],
        true
    );

    let relationship_removed = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "presentation-holder-unlink",
            "method": "did.update",
            "params": {
                "operation": "removeVerificationRelationship",
                "did": did,
                "relationship": "assertionMethod",
                "methodId": holder_binding_method_id,
                "confirmation": {
                    "title": "Remove presentation authority",
                    "summary": "Remove this DID method from the assertion relationship.",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(relationship_removed[0]["ok"], true);
    let prepared_without_authority = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "presentation-unlinked-prepare",
            "method": "credential.presentation.prepare",
            "params": {"request": oxid_composition::standalone_openid4vp_request()},
        })
        .to_string(),
    );
    let unlinked = &prepared_without_authority[0]["result"]["presentation"];
    let unlinked_id = unlinked["id"].as_str().expect("presentation identifier");
    let unlinked_credential_id = unlinked["candidates"][0]["credentialId"]
        .as_str()
        .expect("credential candidate");
    let rejected_without_authority = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "presentation-unlinked-accept",
            "method": "credential.presentation.accept",
            "params": {
                "presentationId": unlinked_id,
                "credentialId": unlinked_credential_id,
                "confirmed": true,
                "intent": "ACCEPT_CREDENTIAL_PRESENTATION"
            }
        })
        .to_string(),
    );
    assert_eq!(
        rejected_without_authority[0]["error"]["code"],
        "holder_not_authorized"
    );
    assert!(
        !rejected_without_authority[0]
            .to_string()
            .contains("vp_token")
    );
}
