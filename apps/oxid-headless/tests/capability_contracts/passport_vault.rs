// SPDX-License-Identifier: Apache-2.0

use oxid_adapter_openid4vci::standalone_credential_offer;
use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use oxid_passport_vault_application::{
    AUTHORIZE_PASSPORT_VAULT_CALL_INTENT, CLAIM_INTENT, CREATE_LOCK_INTENT, DEPOSIT_INTENT,
    SUBMIT_PASSPORT_VAULT_CALL_INTENT, WITHDRAW_INTENT,
};
use serde_json::{Value, json};

use super::support::{execute, execute_with_wallet};

#[test]
fn decodes_the_pinned_generated_passport_vault_fixture_headlessly() {
    let contract_state_hex =
        include_str!("../../../../fixtures/passport-vault/contract-state-v1.hex").trim();
    let responses = execute(
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "vault-contract-state",
            "method": "vault.contract_state.decode",
            "params": { "contractStateHex": contract_state_hex },
        })
        .to_string(),
    );

    assert_eq!(responses[0]["ok"], true);
    let vault = &responses[0]["result"]["vault"];
    assert_eq!(vault["source"], "pinned_contract_layout");
    assert_eq!(vault["chainAnchor"], Value::Null);
    assert_eq!(vault["contract"]["version"], 1);
    assert_eq!(
        vault["contract"]["trustedIssuerDidContractHex"],
        "02".repeat(32)
    );
    assert_eq!(vault["locks"].as_array().map(Vec::len), Some(2));
    assert_eq!(vault["locks"][0]["policy"]["minimumAgeYears"], 18);
    assert_eq!(vault["locks"][1]["policy"]["minimumAgeYears"], 21);
    assert_eq!(vault["totalLocked"], "0");

    let malformed = execute(
        r#"{"protocol":"oxid.headless.v1","id":"bad-vault-state","method":"vault.contract_state.decode","params":{"contractStateHex":"00"}}"#,
    );
    assert_eq!(malformed[0]["ok"], false);
    assert_eq!(malformed[0]["error"]["code"], "invalid_contract_state");
}

#[test]
fn simulated_contract_state_read_is_explicit_and_address_scoped() {
    let invalid = execute(
        r#"{"protocol":"oxid.headless.v1","id":"bad-address","method":"vault.contract_state.read","params":{"contractAddressHex":"00"}}"#,
    );
    assert_eq!(invalid[0]["ok"], false);
    assert_eq!(invalid[0]["error"]["code"], "invalid_params");

    let missing = execute(
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "unknown-simulated-contract",
            "method": "vault.contract_state.read",
            "params": { "contractAddressHex": "11".repeat(32) },
        })
        .to_string(),
    );
    assert_eq!(missing[0]["ok"], false);
    assert_eq!(missing[0]["error"]["code"], "not_found");

    let simulated = execute(
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "simulated-state",
                "method": "vault.contract_state.read",
                "params": {
                    "contractAddressHex": oxid_composition::simulated_passport_vault_contract_address_hex()
                },
            })
            .to_string(),
        );
    assert_eq!(simulated[0]["ok"], true);
    assert_eq!(
        simulated[0]["result"]["vault"]["source"],
        "deterministic_simulation"
    );
    assert_eq!(
        simulated[0]["result"]["vault"]["chainAnchor"]["stateAuthentication"],
        "deterministic_simulation"
    );
}

#[test]
fn contract_call_protocol_runs_all_four_simulated_operations_without_secret_views() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "profile",
            "method": "wallet.profile.create",
            "params": { "displayName": "Vault caller" },
        })
        .to_string(),
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile");
    let selected = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id },
        })
        .to_string(),
    );
    assert_eq!(selected[0]["ok"], true);
    let actions = [
        json!({
            "type": "create_lock",
            "minimumAgeYears": 18,
            "maximumClaimAmount": "40",
            "initialAmount": "100"
        }),
        json!({ "type": "deposit_to_lock", "lockId": 0, "amount": "12" }),
        json!({
            "type": "claim_from_lock",
            "lockId": 0,
            "credentialId": "credential_private_reference",
            "amount": "5"
        }),
        json!({ "type": "withdraw_from_lock", "lockId": 0, "amount": "4" }),
    ];
    let mut transcript = Vec::new();
    for (index, action) in actions.into_iter().enumerate() {
        let prepared = execute_with_wallet(
                &wallet,
                &json!({
                    "protocol": PROTOCOL_VERSION,
                    "id": format!("prepare-{index}"),
                    "method": "vault.contract_call.prepare",
                    "params": {
                        "contractAddressHex": oxid_composition::simulated_passport_vault_contract_address_hex(),
                        "action": action
                    },
                })
                .to_string(),
            );
        assert_eq!(prepared[0]["ok"], true);
        assert_eq!(prepared[0]["result"]["call"]["state"], "prepared");
        let draft_id = prepared[0]["result"]["call"]["draftId"]
            .as_str()
            .expect("draft id");
        let challenge = prepared[0]["result"]["call"]["authorizationChallenge"]
            .as_str()
            .expect("authorization challenge");
        let authorized = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": format!("authorize-{index}"),
                "method": "vault.contract_call.authorize",
                "params": {
                    "draftId": draft_id,
                    "authorizationChallenge": challenge,
                    "confirmed": true,
                    "intent": AUTHORIZE_PASSPORT_VAULT_CALL_INTENT
                },
            })
            .to_string(),
        );
        assert_eq!(authorized[0]["result"]["call"]["state"], "authorized");
        let submitted = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": format!("submit-{index}"),
                "method": "vault.contract_call.submit",
                "params": {
                    "draftId": draft_id,
                    "confirmed": true,
                    "intent": SUBMIT_PASSPORT_VAULT_CALL_INTENT
                },
            })
            .to_string(),
        );
        assert_eq!(submitted[0]["ok"], true);
        assert_eq!(
            submitted[0]["result"]["submission"]["mode"],
            "deterministic_simulation_only"
        );
        assert_eq!(
            submitted[0]["result"]["submission"]["call"]["state"],
            "submitted"
        );
        transcript.extend(prepared);
        transcript.extend(authorized);
        transcript.extend(submitted);
    }
    let history = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "vault-call-history",
            "method": "vault.contract_call.submission_history",
            "params": {}
        })
        .to_string(),
    );
    assert_eq!(
        history[0]["result"]["submissions"].as_array().map(Vec::len),
        Some(4)
    );
    assert!(
        !serde_json::to_string(&transcript)
            .expect("transcript JSON")
            .contains("credential_private_reference")
    );

    let malformed = execute(
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "malformed-call",
            "method": "vault.contract_call.prepare",
            "params": {
                "contractAddressHex": "11".repeat(32),
                "action": {
                    "type": "deposit_to_lock",
                    "lockId": 0,
                    "amount": "01"
                }
            },
        })
        .to_string(),
    );
    assert_eq!(malformed[0]["error"]["code"], "invalid_params");
}

#[test]
fn runs_the_complete_standalone_passport_vault_flow_and_rejects_replay() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"vault-profile","method":"wallet.profile.create","params":{"displayName":"Vault holder"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile")
        .to_owned();
    let initialized = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}",
            json!({"protocol": PROTOCOL_VERSION, "id": "vault-select", "method": "wallet.profile.select", "params": {"profileId": profile_id}}),
            json!({"protocol": PROTOCOL_VERSION, "id": "vault-security", "method": "wallet.security.initialize", "params": {}}),
        ),
    );
    assert!(initialized.iter().all(|response| response["ok"] == true));
    let did_response = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"vault-did","method":"did.create","params":{}}"#,
    );
    let document = &did_response[0]["result"]["didRecord"]["document"];
    let did = document["id"].as_str().expect("DID");
    let method_id = document["relationships"]
        .as_array()
        .expect("relationships")
        .iter()
        .find(|relationship| relationship["relationship"] == "authentication")
        .and_then(|relationship| relationship["methodIds"][0].as_str())
        .expect("authentication method");
    let holder_binding_method_id = document["verificationMethods"]
        .as_array()
        .expect("methods")
        .iter()
        .find(|method| method["publicKeyJwk"]["crv"] == "Jubjub")
        .and_then(|method| method["id"].as_str())
        .expect("Jubjub method");
    let prepared = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "vault-issuance-prepare",
            "method": "credential.issuance.prepare",
            "params": {"offer": standalone_credential_offer()},
        })
        .to_string(),
    );
    let issuance_id = prepared[0]["result"]["issuance"]["id"]
        .as_str()
        .expect("issuance");
    let issued = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "vault-issuance-accept",
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
    let credential_id = issued[0]["result"]["issuance"]["credentialId"]
        .as_str()
        .expect("credential")
        .to_owned();

    let created_lock = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "vault-create",
            "method": "vault.lock.create",
            "params": {
                "minimumAgeYears": 18,
                "requiredIssuingState": "US",
                "requiredDocumentNumber": "AB1234567",
                "maximumClaimAmount": "40",
                "initialAmount": "100",
                "confirmed": true,
                "intent": CREATE_LOCK_INTENT,
            },
        })
        .to_string(),
    );
    assert_eq!(created_lock[0]["result"]["lock"]["lockId"], 0);
    assert_eq!(created_lock[0]["result"]["lock"]["remaining"], "100");
    assert_eq!(
        created_lock[0]["result"]["lock"]["policy"]["requiredIssuingState"],
        "US"
    );

    let deposited = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "vault-deposit",
            "method": "vault.deposit",
            "params": {"lockId": 0, "amount": "20", "confirmed": true, "intent": DEPOSIT_INTENT},
        })
        .to_string(),
    );
    assert_eq!(deposited[0]["result"]["lock"]["remaining"], "120");
    let denied = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "vault-claim-denied",
                "method": "vault.claim",
                "params": {"lockId": 0, "credentialId": credential_id, "amount": "40", "confirmed": false, "intent": CLAIM_INTENT},
            }).to_string(),
        );
    assert_eq!(denied[0]["error"]["code"], "confirmation_required");
    let claimed = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "vault-claim",
                "method": "vault.claim",
                "params": {"lockId": 0, "credentialId": credential_id, "amount": "40", "confirmed": true, "intent": CLAIM_INTENT},
            }).to_string(),
        );
    assert_eq!(claimed[0]["ok"], true, "claim response: {}", claimed[0]);
    assert_eq!(claimed[0]["result"]["releasedAmount"], "40");
    assert_eq!(claimed[0]["result"]["lock"]["remaining"], "80");
    let replay = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "vault-replay",
                "method": "vault.claim",
                "params": {"lockId": 0, "credentialId": credential_id, "amount": "1", "confirmed": true, "intent": CLAIM_INTENT},
            }).to_string(),
        );
    assert_eq!(replay[0]["error"]["code"], "conflict");
    let withdrawn = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "vault-withdraw",
            "method": "vault.withdraw",
            "params": {"lockId": 0, "amount": "80", "confirmed": true, "intent": WITHDRAW_INTENT},
        })
        .to_string(),
    );
    assert_eq!(withdrawn[0]["result"]["lock"]["remaining"], "0");
    let final_state = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"vault-list","method":"vault.locks.list","params":{}}"#,
    );
    assert_eq!(final_state[0]["result"]["vault"]["totalDeposited"], "120");
    assert_eq!(final_state[0]["result"]["vault"]["totalReleased"], "120");
    assert_eq!(final_state[0]["result"]["vault"]["claimCount"], 1);
    let serialized = final_state[0].to_string();
    assert!(!serialized.contains("privateMaterial"));
    assert!(!serialized.contains("credentialFingerprint"));
}
