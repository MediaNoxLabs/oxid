// SPDX-License-Identifier: Apache-2.0

use std::{thread, time::Duration};

use oxid_passport_vault_application::{PassportVaultView, SUBMIT_PASSPORT_VAULT_CALL_INTENT};
use serde_json::{Value, json};

use crate::{
    HeadlessWallet, PROTOCOL_VERSION,
    projections::{capability_manifest, passport_vault_value},
};
use oxid_adapter_openid4vci::standalone_credential_offer;
use oxid_adapter_siopv2::standalone_self_issued_request;
use oxid_diagnostics_application::CLEAR_LOCAL_DIAGNOSTICS_INTENT;
use oxid_passport_vault_application::{
    AUTHORIZE_PASSPORT_VAULT_CALL_INTENT, CLAIM_INTENT, CREATE_LOCK_INTENT, DEPOSIT_INTENT,
    WITHDRAW_INTENT,
};

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
    assert_eq!(
        responses[0]["result"]["passportVaultContractCalls"]["mode"],
        "deterministic_simulation"
    );
    assert_eq!(
        responses[0]["result"]["passportVaultContractCalls"]["contractAddressHex"],
        oxid_composition::simulated_passport_vault_contract_address_hex()
    );
    assert_eq!(
        responses[0]["result"]["passportVaultContractCalls"]["settlesOnMidnight"],
        false
    );
    assert_eq!(
        responses[0]["result"]["passportVaultState"]["persistence"],
        "process_local"
    );
    assert_eq!(
        responses[0]["result"]["passportVaultState"]["settlesOnMidnight"],
        false
    );
    let methods = responses[0]["result"]["methods"]
        .as_array()
        .expect("methods should be an array");
    assert!(methods.iter().any(|capability| {
        capability["method"] == "wallet.profile.create" && capability["status"] == "ready"
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "wallet.transaction.send_unshielded"
            && capability["status"] == "ready"
            && capability["aliasFor"] == "wallet.transaction.submit_unshielded"
    }));
    assert_eq!(responses[0]["result"]["custodyMode"], "development_only");
    assert!(methods.iter().any(|capability| {
        capability["method"] == "wallet.key.sign"
            && capability["status"] == "ready"
            && capability["mode"] == "development_only"
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "wallet.key.generate"
            && capability["algorithms"]
                .as_array()
                .is_some_and(|algorithms| algorithms.iter().any(|algorithm| algorithm == "jubjub"))
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "wallet.balance.snapshot"
            && capability["status"] == "ready"
            && capability["sources"] == json!(["simulated", "live", "cached"])
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "wallet.account.derive"
            && capability["status"] == "ready"
            && capability["mode"] == "development_only"
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "wallet.transaction.prepare_unshielded"
            && capability["status"] == "ready"
            && capability["submissionReady"] == false
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "did.resolve"
            && capability["status"] == "ready"
            && capability["sources"] == json!(["standalone", "live"])
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "credential.reverify"
            && capability["status"] == "ready"
            && capability["compactPolicy"]["issuer"] == "did_assertion_method_and_jubjub_key"
            && capability["compactPolicy"]["temporal"] == "current_time_and_expiry"
            && capability["compactPolicy"]["trust"] == "pinned_standalone_anchor"
            && capability["compactPolicy"]["status"] == "not_checked"
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "credential.disclosure.preview"
            && capability["status"] == "ready"
            && capability["generatesPresentation"] == false
            && capability["claimValuesExposed"] == false
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "credential.presentation.accept"
            && capability["status"] == "blocked"
            && capability["holderAuthorization"] == "current_managed_jubjub_method"
            && capability["proofAvailable"] == false
            && capability["artifactRootEnvironment"] == "OXID_PRESENTATION_ARTIFACTS_DIR"
            && capability["generatesPresentation"] == false
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "identity.login"
            && capability["status"] == "ready"
            && capability["aliasFor"] == "identity.authentication.prepare"
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "vault.claim"
            && capability["status"] == "ready"
            && capability["mode"] == "standalone"
            && capability["replayProtection"] == "per_lock_credential_root"
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "vault.contract_state.decode"
            && capability["status"] == "ready"
            && capability["mode"] == "native"
            && capability["mutates"] == false
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "vault.contract_state.read"
            && capability["status"] == "composition_dependent"
            && capability["sources"]
                == json!([
                    "deterministic_simulation",
                    "node_anchored_indexer",
                    "finalized_node_replay"
                ])
            && capability["stateAuthentication"]
                == json!([
                    "deterministic_simulation",
                    "indexer_supplied_not_proven",
                    "canonical_finalized_replay"
                ])
            && capability["mutates"] == false
    }));
    assert!(methods.iter().any(|capability| {
        capability["method"] == "vault.contract_call.prepare"
            && capability["status"] == "ready"
            && capability["mode"] == "deterministic_simulation"
            && capability["requiresStateAuthentication"] == "deterministic_simulation"
            && capability["privateMaterialExposed"] == false
            && capability["operations"]
                == json!([
                    "create_lock",
                    "deposit_to_lock",
                    "claim_from_lock",
                    "withdraw_from_lock"
                ])
    }));
    for (method, intent) in [
        ("system.diagnostics.clear", CLEAR_LOCAL_DIAGNOSTICS_INTENT),
        (
            "vault.contract_call.authorize",
            AUTHORIZE_PASSPORT_VAULT_CALL_INTENT,
        ),
        (
            "vault.contract_call.submit",
            SUBMIT_PASSPORT_VAULT_CALL_INTENT,
        ),
        ("vault.lock.create", CREATE_LOCK_INTENT),
        ("vault.deposit", DEPOSIT_INTENT),
        ("vault.claim", CLAIM_INTENT),
        ("vault.withdraw", WITHDRAW_INTENT),
    ] {
        assert!(methods.iter().any(|capability| {
            capability["method"] == method && capability["intent"] == intent
        }));
    }
}

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
fn native_settlement_manifest_includes_conformant_claim_and_reports_recovery() {
    let methods = capability_manifest(false, "native_settlement", "owner_private_atomic_file");
    let methods = methods.as_array().expect("capability array");
    let prepare = methods
        .iter()
        .find(|capability| capability["method"] == "vault.contract_call.prepare")
        .expect("prepare capability");
    assert_eq!(prepare["status"], "ready");
    assert_eq!(
        prepare["operations"],
        json!([
            "create_lock",
            "deposit_to_lock",
            "claim_from_lock",
            "withdraw_from_lock"
        ])
    );
    let history = methods
        .iter()
        .find(|capability| capability["method"] == "vault.contract_call.submission_history")
        .expect("history capability");
    assert_eq!(history["persistence"], "public_metadata_only");
    let reconcile = methods
        .iter()
        .find(|capability| capability["method"] == "vault.contract_call.reconcile_submission")
        .expect("reconciliation capability");
    assert_eq!(reconcile["scope"], "finalized_chain");
}

#[test]
fn decodes_the_pinned_generated_passport_vault_fixture_headlessly() {
    let contract_state_hex =
        include_str!("../../../fixtures/passport-vault/contract-state-v1.hex").trim();
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
fn contract_state_projection_discloses_the_unproven_indexer_boundary() {
    let view = PassportVaultView {
        source: "node_anchored_indexer".to_owned(),
        chain_anchor: Some(
            oxid_passport_vault_application::PassportVaultChainAnchorView {
                contract_address_hex: "11".repeat(32),
                transaction_hash_hex: "22".repeat(32),
                action_block_hash_hex: "33".repeat(32),
                action_block_height: 40,
                finalized_head_hash_hex: "44".repeat(32),
                finalized_head_height: 42,
                finalized_head_time_seconds: 1_700_000_000,
                state_authentication: "indexer_supplied_not_proven".to_owned(),
            },
        ),
        contract: None,
        locks: Vec::new(),
        total_deposited: "0".to_owned(),
        total_released: "0".to_owned(),
        total_locked: "0".to_owned(),
        claim_count: 0,
    };
    let value = passport_vault_value(&view);
    assert_eq!(
        value["chainAnchor"]["stateAuthentication"],
        "indexer_supplied_not_proven"
    );
    assert_eq!(value["chainAnchor"]["actionBlockHeight"], 40);
    assert_eq!(value["chainAnchor"]["finalizedHeadHeight"], 42);
    assert_eq!(
        value["chainAnchor"]["finalizedHeadTimeSeconds"],
        1_700_000_000_u64
    );
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
fn diagnostics_retain_only_closed_codes_and_clear_with_exact_confirmation() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let first = execute_with_wallet(
        &wallet,
        concat!(
            "not-json-containing-super-secret\n",
            r#"{"protocol":"oxid.headless.v1","id":"diag-1","method":"system.diagnostics.snapshot","params":{}}"#,
        ),
    );

    assert_eq!(first[1]["result"]["diagnostics"]["totalEvents"], 1);
    assert_eq!(
        first[1]["result"]["diagnostics"]["recent"][0]["code"],
        "headless.request.rejected"
    );
    assert_eq!(first[1]["result"]["diagnostics"]["payloadsRetained"], false);
    assert!(!first[1].to_string().contains("super-secret"));

    let clear = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}\n{}",
            r#"{"protocol":"oxid.headless.v1","id":"diag-bad-clear","method":"system.diagnostics.clear","params":{"confirmed":true,"intent":"clear"}}"#,
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "diag-clear",
                "method": "system.diagnostics.clear",
                "params": {
                    "confirmed": true,
                    "intent": CLEAR_LOCAL_DIAGNOSTICS_INTENT,
                }
            }),
            r#"{"protocol":"oxid.headless.v1","id":"diag-after","method":"system.diagnostics.snapshot","params":{}}"#,
        ),
    );
    assert_eq!(clear[0]["error"]["code"], "confirmation_required");
    assert_eq!(clear[1]["result"]["clearedEvents"], 1);
    assert_eq!(clear[2]["result"]["diagnostics"]["totalEvents"], 0);
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

#[test]
fn derives_and_binds_a_midnight_account_without_secret_protocol_fields() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"derive-create","method":"wallet.profile.create","params":{"displayName":"Derived account"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile identifier is returned");
    let setup = format!(
        "{}\n{}\n{}\n{}",
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "derive-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }),
        r#"{"protocol":"oxid.headless.v1","id":"derive-before-init","method":"wallet.account.derive","params":{}}"#,
        r#"{"protocol":"oxid.headless.v1","id":"derive-init","method":"wallet.security.initialize","params":{}}"#,
        r#"{"protocol":"oxid.headless.v1","id":"derive-account","method":"wallet.account.derive","params":{"accountIndex":0,"addressIndex":0}}"#,
    );
    let responses = execute_with_wallet(&wallet, &setup);
    assert_eq!(responses[1]["error"]["code"], "failed_precondition");
    assert_eq!(
        responses[3]["ok"], true,
        "unexpected response: {responses:?}"
    );
    let derived = &responses[3]["result"]["account"];
    assert_eq!(derived["networkId"], "undeployed");
    assert_eq!(derived["accountId"], "midnight_account_0_0");
    assert_eq!(derived["receiveAddress"]["kind"], "unshielded");
    assert!(
        derived["receiveAddress"]["value"]
            .as_str()
            .is_some_and(|address| address.starts_with("mn_addr_undeployed1"))
    );
    assert_eq!(derived["addresses"].as_array().map(Vec::len), Some(2));
    assert!(derived["addresses"].as_array().is_some_and(|addresses| {
        addresses.iter().any(|address| {
            address["kind"] == "shielded"
                && address["value"]
                    .as_str()
                    .is_some_and(|value| value.starts_with("mn_shield-addr_undeployed1"))
        })
    }));
    let listed = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"derive-list","method":"wallet.address.list","params":{}}"#,
    );
    assert_eq!(listed[0]["result"]["addresses"], derived["addresses"]);
    let unshielded = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"derive-unshielded","method":"wallet.address.unshielded","params":{}}"#,
    );
    assert_eq!(unshielded[0]["result"]["address"]["kind"], "unshielded");
    assert_eq!(
        unshielded[0]["result"]["address"]["value"],
        derived["receiveAddress"]["value"]
    );
    let shielded = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"derive-shielded","method":"wallet.address.shielded","params":{}}"#,
    );
    assert_eq!(shielded[0]["result"]["address"]["kind"], "shielded");
    assert!(
        shielded[0]["result"]["address"]["value"]
            .as_str()
            .is_some_and(|value| value.starts_with("mn_shield-addr_undeployed1"))
    );
    let key_ref = derived["transactionKeyRef"]
        .as_str()
        .expect("opaque key reference is returned");

    let flow = format!(
        "{}\n{}\n{}\n{}",
        r#"{"protocol":"oxid.headless.v1","id":"derive-get","method":"wallet.account.get","params":{}}"#,
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "derive-sign",
            "method": "wallet.key.sign",
            "params": {
                "keyRef": key_ref,
                "payloadHex": "7472616e73616374696f6e2d696e74656e74",
                "confirmation": {
                    "title": "Sign Midnight transaction intent",
                    "summary": "Authorize the bounded headless conformance payload",
                    "confirmed": true
                }
            }
        }),
        r#"{"protocol":"oxid.headless.v1","id":"derive-lock","method":"wallet.security.lock","params":{}}"#,
        r#"{"protocol":"oxid.headless.v1","id":"derive-locked","method":"wallet.account.derive","params":{"addressIndex":1}}"#,
    );
    let flowed = execute_with_wallet(&wallet, &flow);
    assert_eq!(
        flowed[0]["result"]["account"]["accountId"],
        "midnight_account_0_0"
    );
    assert_eq!(flowed[1]["result"]["algorithm"], "secp256k1-schnorr");
    assert_eq!(
        flowed[1]["result"]["signatureHex"]
            .as_str()
            .expect("signature is encoded")
            .len(),
        128
    );
    assert_eq!(flowed[3]["error"]["code"], "wallet_locked");

    let out_of_bounds = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"derive-bounds","method":"wallet.account.derive","params":{"accountIndex":2147483648}}"#,
    );
    assert_eq!(out_of_bounds[0]["error"]["code"], "invalid_argument");

    let rejected = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"derive-secret","method":"wallet.account.derive","params":{"seedHex":"do-not-accept"}}"#,
    );
    assert_eq!(rejected[0]["error"]["code"], "invalid_params");
    assert!(!rejected[0].to_string().contains("do-not-accept"));
}

#[test]
fn completes_an_exact_unshielded_transfer_without_exposing_material() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"transfer-create","method":"wallet.profile.create","params":{"displayName":"Transfer flow"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile identifier is returned");
    let setup = format!(
        "{}\n{}\n{}",
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }),
        r#"{"protocol":"oxid.headless.v1","id":"transfer-init","method":"wallet.security.initialize","params":{}}"#,
        r#"{"protocol":"oxid.headless.v1","id":"transfer-derive","method":"wallet.account.derive","params":{}}"#,
    );
    let setup = execute_with_wallet(&wallet, &setup);
    let recipient = setup[2]["result"]["account"]["receiveAddress"]["value"]
        .as_str()
        .expect("derived address is returned");
    let before_sync = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-before-sync",
            "method": "wallet.transaction.prepare_unshielded",
            "params": {
                "recipientAddress": recipient,
                "amountAtomicUnits": "1500000"
            }
        })
        .to_string(),
    );
    assert_eq!(before_sync[0]["error"]["code"], "failed_precondition");
    let synchronized = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"transfer-sync","method":"wallet.connect","params":{}}"#,
    );
    assert_eq!(synchronized[0]["ok"], true);
    let prepared = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-prepare",
            "method": "wallet.transaction.prepare_unshielded",
            "params": {
                "recipientAddress": recipient,
                "amountAtomicUnits": "1500000"
            }
        })
        .to_string(),
    );
    let transfer = &prepared[0]["result"]["transfer"];
    assert_eq!(transfer["state"], "prepared");
    assert_eq!(transfer["amount"]["atomicUnits"], "1500000");
    assert_eq!(transfer["change"]["atomicUnits"], "500000");
    assert_eq!(transfer["inputCount"], 1);
    assert_eq!(transfer["feeState"], "requires_balancing");
    assert_eq!(transfer["proofRequired"], true);
    assert_eq!(transfer["submissionReady"], false);
    let draft_id = transfer["draftId"].as_str().expect("draft id is returned");
    let challenge = transfer["authorizationChallenge"]
        .as_str()
        .expect("challenge is returned");

    let denied = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-denied",
            "method": "wallet.transaction.authorize_unshielded",
            "params": {
                "draftId": draft_id,
                "authorizationChallenge": challenge,
                "confirmation": {
                    "title": "Authorize NIGHT transfer",
                    "summary": "Send 1.5 NIGHT; proving and submission remain pending",
                    "confirmed": false
                }
            }
        })
        .to_string(),
    );
    assert_eq!(denied[0]["error"]["code"], "confirmation_required");

    let mismatch = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-mismatch",
            "method": "wallet.transaction.authorize_unshielded",
            "params": {
                "draftId": draft_id,
                "authorizationChallenge": "txauth_wrong",
                "confirmation": {
                    "title": "Authorize NIGHT transfer",
                    "summary": "Send 1.5 NIGHT; proving and submission remain pending",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(mismatch[0]["error"]["code"], "authorization_mismatch");

    let locked = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}",
            r#"{"protocol":"oxid.headless.v1","id":"transfer-lock","method":"wallet.security.lock","params":{}}"#,
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-locked-authorize",
                "method": "wallet.transaction.authorize_unshielded",
                "params": {
                    "draftId": draft_id,
                    "authorizationChallenge": challenge,
                    "confirmation": {
                        "title": "Authorize NIGHT transfer",
                        "summary": "Send 1.5 NIGHT; proving and submission remain pending",
                        "confirmed": true
                    }
                }
            })
        ),
    );
    assert_eq!(locked[1]["error"]["code"], "wallet_locked");
    let unlocked = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"transfer-unlock","method":"wallet.security.unlock","params":{}}"#,
    );
    assert_eq!(unlocked[0]["result"]["security"]["state"], "unlocked");

    let authorized = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-authorize",
            "method": "wallet.transaction.authorize_unshielded",
            "params": {
                "draftId": draft_id,
                "authorizationChallenge": challenge,
                "confirmation": {
                    "title": "Authorize NIGHT transfer",
                    "summary": "Send 1.5 NIGHT; proving and submission remain pending",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(authorized[0]["result"]["transfer"]["state"], "authorized");
    assert_eq!(authorized[0]["result"]["transfer"]["submissionReady"], true);
    let encoded = authorized[0].to_string();
    assert!(!encoded.contains("signatureHex"));
    assert!(!encoded.contains("transactionHex"));
    assert!(!encoded.contains("private"));

    let retained = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-get",
            "method": "wallet.transaction.draft",
            "params": { "draftId": draft_id }
        })
        .to_string(),
    );
    assert_eq!(retained[0]["result"]["transfer"]["state"], "authorized");

    let submit_denied = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-submit-denied",
            "method": "wallet.transaction.submit_unshielded",
            "params": {
                "draftId": draft_id,
                "confirmation": {
                    "title": "Submit NIGHT transfer",
                    "summary": "Prove, balance DUST fees, and submit the authorized transfer",
                    "confirmed": false
                }
            }
        })
        .to_string(),
    );
    assert_eq!(submit_denied[0]["error"]["code"], "confirmation_required");

    let locked_submit = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}\n{}",
            r#"{"protocol":"oxid.headless.v1","id":"transfer-submit-lock","method":"wallet.security.lock","params":{}}"#,
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-submit-locked",
                "method": "wallet.transaction.submit_unshielded",
                "params": {
                    "draftId": draft_id,
                    "confirmation": {
                        "title": "Submit NIGHT transfer",
                        "summary": "Prove, balance DUST fees, and submit the authorized transfer",
                        "confirmed": true
                    }
                }
            }),
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-submit-restored",
                "method": "wallet.transaction.draft",
                "params": { "draftId": draft_id }
            })
        ),
    );
    assert_eq!(locked_submit[1]["error"]["code"], "wallet_locked");
    assert_eq!(
        locked_submit[2]["result"]["transfer"]["state"],
        "authorized"
    );
    let unlocked = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"transfer-submit-unlock","method":"wallet.security.unlock","params":{}}"#,
    );
    assert_eq!(unlocked[0]["result"]["security"]["state"], "unlocked");

    let submitted = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-submit",
            "method": "wallet.transaction.submit_unshielded",
            "params": {
                "draftId": draft_id,
                "confirmation": {
                    "title": "Submit NIGHT transfer",
                    "summary": "Prove, balance DUST fees, and submit the authorized transfer",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    let submission = &submitted[0]["result"]["submission"];
    assert_eq!(submission["mode"], "simulated");
    assert_eq!(submission["transfer"]["state"], "submitted");
    assert_eq!(submission["transfer"]["feeState"], "final");
    assert_eq!(submission["transfer"]["proofRequired"], false);
    assert_eq!(submission["fee"]["assetId"], "midnight:dust");
    assert_eq!(submission["fee"]["atomicUnits"], "1000000");
    assert_eq!(
        submission["transactionId"]
            .as_str()
            .expect("transaction id is public")
            .len(),
        64
    );
    assert_eq!(
        submission["blockId"]
            .as_str()
            .expect("block id is public")
            .len(),
        64
    );
    let submitted_wire = submitted[0].to_string();
    assert!(!submitted_wire.contains("signatureHex"));
    assert!(!submitted_wire.contains("transactionHex"));
    assert!(!submitted_wire.contains("dustSeed"));

    let submission_history = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"transfer-submission-history","method":"wallet.transaction.submission_history","params":{}}"#,
    );
    let recovered = &submission_history[0]["result"]["submissions"][0];
    assert_eq!(recovered["draftId"], draft_id);
    assert_eq!(recovered["state"], "included");
    assert_eq!(recovered["transactionId"], submission["transactionId"]);
    assert_eq!(recovered["replacementAllowed"], false);
    assert_eq!(recovered["reconciliationAllowed"], false);
    let recovered_wire = submission_history[0].to_string();
    assert!(!recovered_wire.contains("signatureHex"));
    assert!(!recovered_wire.contains("transactionHex"));
    assert!(!recovered_wire.contains("dustSeed"));

    let repeated = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}",
            r#"{"protocol":"oxid.headless.v1","id":"transfer-submit-relock","method":"wallet.security.lock","params":{}}"#,
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-submit-repeat",
                "method": "wallet.transaction.send_unshielded",
                "params": {
                    "draftId": draft_id,
                    "confirmation": {
                        "title": "Read submitted NIGHT transfer",
                        "summary": "Return the already included public submission metadata",
                        "confirmed": true
                    }
                }
            })
        ),
    );
    assert_eq!(repeated[1]["ok"], true);
    assert_eq!(
        repeated[1]["result"]["submission"]["transactionId"],
        submission["transactionId"]
    );
    assert_eq!(
        repeated[1]["result"]["submission"]["blockId"],
        submission["blockId"]
    );

    let insufficient = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "transfer-insufficient",
            "method": "wallet.transaction.prepare_unshielded",
            "params": {
                "recipientAddress": recipient,
                "amountAtomicUnits": "6000000"
            }
        })
        .to_string(),
    );
    assert_eq!(insufficient[0]["error"]["code"], "insufficient_funds");

    let foreign_network = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "transfer-foreign-network",
                "method": "wallet.transaction.prepare_unshielded",
                "params": {
                    "recipientAddress": "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y",
                    "amountAtomicUnits": "1"
                }
            })
            .to_string(),
        );
    assert_eq!(foreign_network[0]["error"]["code"], "invalid_argument");
}

#[test]
fn starts_cancels_and_retries_a_submission_through_the_headless_protocol() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"cancel-create","method":"wallet.profile.create","params":{"displayName":"Cancellation flow"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile identifier is returned");
    let setup = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}\n{}\n{}",
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "cancel-select",
                "method": "wallet.profile.select",
                "params": { "profileId": profile_id }
            }),
            r#"{"protocol":"oxid.headless.v1","id":"cancel-init","method":"wallet.security.initialize","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"cancel-derive","method":"wallet.account.derive","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"cancel-sync","method":"wallet.connect","params":{}}"#,
        ),
    );
    let recipient = setup[2]["result"]["account"]["receiveAddress"]["value"]
        .as_str()
        .expect("receive address is returned");
    let prepared = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "cancel-prepare",
            "method": "wallet.transaction.prepare_unshielded",
            "params": {
                "recipientAddress": recipient,
                "amountAtomicUnits": "1500000"
            }
        })
        .to_string(),
    );
    let transfer = &prepared[0]["result"]["transfer"];
    let draft_id = transfer["draftId"]
        .as_str()
        .expect("draft id is returned")
        .to_owned();
    let challenge = transfer["authorizationChallenge"]
        .as_str()
        .expect("authorization challenge is returned");
    let authorized = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "cancel-authorize",
            "method": "wallet.transaction.authorize_unshielded",
            "params": {
                "draftId": draft_id,
                "authorizationChallenge": challenge,
                "confirmation": {
                    "title": "Authorize NIGHT transfer",
                    "summary": "Authorize the cancellable headless transfer",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(authorized[0]["result"]["transfer"]["state"], "authorized");

    let started = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "cancel-start",
            "method": "wallet.transaction.start_submission",
            "params": {
                "draftId": draft_id,
                "confirmation": {
                    "title": "Submit NIGHT transfer",
                    "summary": "Start the cancellable headless transfer",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(started[0]["result"]["submissionStatus"]["state"], "running");
    assert_eq!(
        started[0]["result"]["submissionStatus"]["cancellationAllowed"],
        true
    );
    let cancelled = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "cancel-request",
            "method": "wallet.transaction.cancel_submission",
            "params": { "draftId": draft_id }
        })
        .to_string(),
    );
    assert_eq!(
        cancelled[0]["result"]["submissionStatus"]["state"],
        "cancellation_requested"
    );

    let status_request = json!({
        "protocol": PROTOCOL_VERSION,
        "id": "cancel-status",
        "method": "wallet.transaction.submission_status",
        "params": { "draftId": draft_id }
    })
    .to_string();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    let final_status = loop {
        let response = execute_with_wallet(&wallet, &status_request);
        if response[0]["result"]["submissionStatus"]["state"] == "cancelled" {
            break response;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "submission cancellation was not acknowledged"
        );
        thread::yield_now();
    };
    assert_eq!(
        final_status[0]["result"]["submissionStatus"]["retryable"],
        true
    );
    assert!(!final_status[0].to_string().contains("transactionHex"));

    let retried = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "cancel-retry",
            "method": "wallet.transaction.submit_unshielded",
            "params": {
                "draftId": draft_id,
                "confirmation": {
                    "title": "Retry NIGHT transfer",
                    "summary": "Retry only after pre-broadcast cancellation was acknowledged",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(
        retried[0]["result"]["submission"]["transfer"]["state"],
        "submitted"
    );
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

#[test]
fn exposes_initial_resumed_current_and_cancelled_dust_flows() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"dust-create","method":"wallet.profile.create","params":{"displayName":"DUST flow"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile id is returned");
    let setup = format!(
        "{}\n{}",
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "dust-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }),
        r#"{"protocol":"oxid.headless.v1","id":"dust-init","method":"wallet.security.initialize","params":{}}"#,
    );
    assert!(
        execute_with_wallet(&wallet, &setup)
            .iter()
            .all(|response| response["ok"] == true)
    );
    let initial_and_cancelled = execute_with_wallet(
        &wallet,
        concat!(
            r#"{"protocol":"oxid.headless.v1","id":"dust-initial","method":"wallet.dust.sync.status","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"dust-start","method":"wallet.dust.sync.start","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"dust-progress","method":"wallet.dust.sync.status","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"dust-cancel","method":"wallet.dust.sync.cancel","params":{}}"#,
        ),
    );
    assert_eq!(
        initial_and_cancelled[0]["result"]["dustSync"]["state"],
        "never_synced"
    );
    assert_eq!(
        initial_and_cancelled[1]["result"]["dustSync"]["state"],
        "syncing"
    );
    assert_eq!(
        initial_and_cancelled[2]["result"]["dustSync"]["currentCursor"],
        0
    );
    assert_eq!(
        initial_and_cancelled[2]["result"]["dustSync"]["targetCursor"],
        2
    );
    assert_eq!(
        initial_and_cancelled[3]["result"]["dustSync"]["state"],
        "cancelled"
    );

    let resumed_and_current = execute_with_wallet(
        &wallet,
        concat!(
            r#"{"protocol":"oxid.headless.v1","id":"dust-resume","method":"wallet.dust.sync.start","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"dust-resumed-progress","method":"wallet.dust.sync.status","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"dust-complete","method":"wallet.dust.sync.status","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"dust-current-start","method":"wallet.dust.sync.start","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"dust-current","method":"wallet.dust.sync.status","params":{}}"#,
        ),
    );
    assert_eq!(
        resumed_and_current[0]["result"]["dustSync"]["currentCursor"],
        0
    );
    assert_eq!(
        resumed_and_current[1]["result"]["dustSync"]["currentCursor"],
        1
    );
    let completed = &resumed_and_current[2]["result"]["dustSync"];
    assert_eq!(completed["state"], "synced");
    assert_eq!(completed["currentCursor"], completed["targetCursor"]);
    assert_eq!(completed["balance"]["atomicUnits"], "12000000000000000");
    assert_eq!(
        resumed_and_current[4]["result"]["dustSync"]["state"],
        "synced"
    );
    assert_eq!(
        resumed_and_current[4]["result"]["dustSync"]["eventsProcessed"],
        0
    );
}

#[test]
fn registers_protected_dust_through_explicit_secret_free_headless_stages() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"register-create","method":"wallet.profile.create","params":{"displayName":"Registration flow"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile id is returned");
    let setup = execute_with_wallet(
        &wallet,
        &format!(
            "{}\n{}\n{}\n{}",
            json!({
                "protocol": PROTOCOL_VERSION,
                "id": "register-select",
                "method": "wallet.profile.select",
                "params": { "profileId": profile_id }
            }),
            r#"{"protocol":"oxid.headless.v1","id":"register-init","method":"wallet.security.initialize","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"register-derive","method":"wallet.account.derive","params":{}}"#,
            r#"{"protocol":"oxid.headless.v1","id":"register-sync","method":"wallet.connect","params":{}}"#,
        ),
    );
    assert!(setup.iter().all(|response| response["ok"] == true));

    let rejected = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"register-secret","method":"wallet.dust.registration.prepare","params":{"seedHex":"never-accept-registration-secret"}}"#,
    );
    assert_eq!(rejected[0]["error"]["code"], "invalid_params");
    assert!(
        !rejected[0]
            .to_string()
            .contains("never-accept-registration-secret")
    );

    let prepared = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"register-prepare","method":"wallet.dust.registration.prepare","params":{}}"#,
    );
    let registration = &prepared[0]["result"]["registration"];
    assert_eq!(registration["state"], "prepared", "{prepared:?}");
    assert_eq!(registration["registeredNight"]["atomicUnits"], "5000000");
    assert_eq!(registration["inputCount"], 3);
    assert_eq!(registration["authorizationReady"], true);
    assert_eq!(registration["submissionReady"], false);
    let draft_id = registration["draftId"]
        .as_str()
        .expect("registration draft id is returned");
    let challenge = registration["authorizationChallenge"]
        .as_str()
        .expect("authorization challenge is returned");

    let denied = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "register-denied",
            "method": "wallet.dust.registration.authorize",
            "params": {
                "draftId": draft_id,
                "authorizationChallenge": challenge,
                "confirmation": {
                    "title": "Authorize DUST registration",
                    "summary": "Register this wallet's eligible NIGHT with its protected DUST key",
                    "confirmed": false
                }
            }
        })
        .to_string(),
    );
    assert_eq!(denied[0]["error"]["code"], "confirmation_required");

    let authorized = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "register-authorize",
            "method": "wallet.dust.registration.authorize",
            "params": {
                "draftId": draft_id,
                "authorizationChallenge": challenge,
                "confirmation": {
                    "title": "Authorize DUST registration",
                    "summary": "Register this wallet's eligible NIGHT with its protected DUST key",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(
        authorized[0]["result"]["registration"]["state"],
        "authorized"
    );
    assert_eq!(
        authorized[0]["result"]["registration"]["submissionReady"],
        true
    );

    let submitted = execute_with_wallet(
            &wallet,
            &json!({
                "protocol": PROTOCOL_VERSION,
                "id": "register-submit",
                "method": "wallet.dust.registration.submit",
                "params": {
                    "draftId": draft_id,
                    "confirmation": {
                        "title": "Submit DUST registration",
                        "summary": "Prove and submit the authorized registration using only this wallet's generated DUST allowance",
                        "confirmed": true
                    }
                }
            })
            .to_string(),
        );
    let submission = &submitted[0]["result"]["submission"];
    assert_eq!(submission["mode"], "simulated");
    assert_eq!(submission["registration"]["state"], "submitted");
    assert_eq!(submission["registrationObservation"], "included");
    assert_eq!(submission["dustReadiness"], "requires_synchronization");
    assert_eq!(submission["fee"]["assetId"], "midnight:dust");
    let wire = submitted[0].to_string();
    for forbidden in [
        "dustSeed",
        "dustSecret",
        "signatureHex",
        "transactionHex",
        "intentHash",
        "outputIndex",
    ] {
        assert!(
            !wire.contains(forbidden),
            "{forbidden} must stay adapter-private"
        );
    }

    let status = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "register-status",
            "method": "wallet.dust.registration.status",
            "params": { "draftId": draft_id }
        })
        .to_string(),
    );
    assert_eq!(
        status[0]["result"]["registrationStatus"]["state"],
        "included"
    );
    assert_eq!(
        status[0]["result"]["registrationStatus"]["dustReadiness"],
        "requires_synchronization"
    );

    let transfer_history = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"register-transfer-history","method":"wallet.transaction.submission_history","params":{}}"#,
    );
    assert_eq!(
        transfer_history[0]["result"]["submissions"],
        serde_json::json!([])
    );
    let repeated = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"register-repeat","method":"wallet.dust.registration.prepare","params":{}}"#,
    );
    assert_eq!(repeated[0]["error"]["code"], "already_registered");
}

#[test]
fn exposes_exact_resumable_shielded_flow_without_secret_material() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let created = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"shielded-create","method":"wallet.profile.create","params":{"displayName":"Shielded flow"}}"#,
    );
    let profile_id = created[0]["result"]["profile"]["id"]
        .as_str()
        .expect("profile id is returned");
    let setup = format!(
        "{}\n{}\n{}",
        json!({
            "protocol": PROTOCOL_VERSION,
            "id": "shielded-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }),
        r#"{"protocol":"oxid.headless.v1","id":"shielded-init","method":"wallet.security.initialize","params":{}}"#,
        r#"{"protocol":"oxid.headless.v1","id":"shielded-derive","method":"wallet.account.derive","params":{"accountIndex":0,"addressIndex":0}}"#,
    );
    assert!(
        execute_with_wallet(&wallet, &setup)
            .iter()
            .all(|response| response["ok"] == true)
    );
    let account = execute_with_wallet(
        &wallet,
        r#"{"protocol":"oxid.headless.v1","id":"shielded-account-sync","method":"wallet.connect","params":{}}"#,
    );
    let recipient = account[0]["result"]["account"]["addresses"]
        .as_array()
        .and_then(|addresses| {
            addresses
                .iter()
                .find(|address| address["kind"] == "shielded")
        })
        .and_then(|address| address["value"].as_str())
        .expect("shielded recipient is returned")
        .to_owned();
    let before_sync = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "shielded-transfer-before-sync",
            "method": "wallet.transaction.prepare_shielded",
            "params": {
                "recipientAddress": recipient,
                "tokenType": "0000000000000000000000000000000000000000000000000000000000000000",
                "amountAtomicUnits": "1500000"
            }
        })
        .to_string(),
    );
    assert_eq!(before_sync[0]["error"]["code"], "failed_precondition");

    let initial_and_cancelled = execute_with_wallet(
        &wallet,
        concat!(
            r#"{"protocol":"oxid.headless.v1","id":"shielded-initial","method":"wallet.shielded.sync.status","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"shielded-start","method":"wallet.shielded.sync.start","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"shielded-progress","method":"wallet.shielded.sync.status","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"shielded-cancel","method":"wallet.shielded.sync.cancel","params":{}}"#,
        ),
    );
    assert_eq!(
        initial_and_cancelled[0]["result"]["shieldedSync"]["state"],
        "never_synced"
    );
    assert_eq!(
        initial_and_cancelled[2]["result"]["shieldedSync"]["commitmentCount"],
        1
    );
    assert_eq!(
        initial_and_cancelled[3]["result"]["shieldedSync"]["state"],
        "cancelled"
    );

    let completed = execute_with_wallet(
        &wallet,
        concat!(
            r#"{"protocol":"oxid.headless.v1","id":"shielded-resume","method":"wallet.shielded.sync.start","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"shielded-progress-2","method":"wallet.shielded.sync.status","params":{}}"#,
            "\n",
            r#"{"protocol":"oxid.headless.v1","id":"shielded-complete","method":"wallet.shielded.sync.status","params":{}}"#,
        ),
    );
    let synced = &completed[2]["result"]["shieldedSync"];
    assert_eq!(synced["state"], "synced");
    assert_eq!(synced["ownedNoteCount"], 1);
    assert_eq!(synced["commitmentCount"], 3);
    assert_eq!(synced["balances"][0]["atomicUnits"], "5000000");
    assert_eq!(
        synced["balances"][0]["tokenType"],
        "0000000000000000000000000000000000000000000000000000000000000000"
    );
    let encoded = serde_json::to_string(&completed).expect("responses serialize");
    assert!(!encoded.contains("seed"));
    assert!(!encoded.contains("private"));
    assert!(!encoded.contains("mnemonic"));

    let prepared = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "shielded-transfer-prepare",
            "method": "wallet.transaction.prepare_shielded",
            "params": {
                "recipientAddress": recipient,
                "tokenType": "0000000000000000000000000000000000000000000000000000000000000000",
                "amountAtomicUnits": "1500000"
            }
        })
        .to_string(),
    );
    let transfer = &prepared[0]["result"]["transfer"];
    assert_eq!(transfer["recipientKind"], "shielded");
    assert_eq!(transfer["amount"]["atomicUnits"], "1500000");
    assert_eq!(transfer["change"]["atomicUnits"], "3500000");
    assert_eq!(transfer["inputCount"], 1);
    let draft_id = transfer["draftId"]
        .as_str()
        .expect("shielded draft is returned");
    let challenge = transfer["authorizationChallenge"]
        .as_str()
        .expect("shielded challenge is returned");
    let competing = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "shielded-transfer-competing-draft",
            "method": "wallet.transaction.prepare_shielded",
            "params": {
                "recipientAddress": recipient,
                "tokenType": "0000000000000000000000000000000000000000000000000000000000000000",
                "amountAtomicUnits": "1000000"
            }
        })
        .to_string(),
    );
    assert_eq!(competing[0]["error"]["code"], "conflict");
    let authorized = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "shielded-transfer-authorize",
            "method": "wallet.transaction.authorize_shielded",
            "params": {
                "draftId": draft_id,
                "authorizationChallenge": challenge,
                "confirmation": {
                    "title": "Authorize shielded NIGHT transfer",
                    "summary": "Send 1.5 shielded NIGHT after exact review",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(authorized[0]["result"]["transfer"]["state"], "authorized");
    let submitted = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "shielded-transfer-submit",
            "method": "wallet.transaction.send_shielded",
            "params": {
                "draftId": draft_id,
                "confirmation": {
                    "title": "Prove and submit shielded NIGHT transfer",
                    "summary": "Prove, balance DUST, and submit the exact shielded transfer",
                    "confirmed": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(submitted[0]["result"]["submission"]["mode"], "simulated");
    assert_eq!(
        submitted[0]["result"]["submission"]["transfer"]["state"],
        "submitted"
    );
    let submitted_json = submitted[0].to_string();
    for forbidden in [
        "nullifier",
        "merkle",
        "witness",
        "proofPreimage",
        "transactionHex",
        "seed",
    ] {
        assert!(!submitted_json.contains(forbidden));
    }
    let replay_before_state_advances = execute_with_wallet(
        &wallet,
        &json!({
            "protocol": PROTOCOL_VERSION,
            "id": "shielded-transfer-replay-before-sync-advances",
            "method": "wallet.transaction.prepare_shielded",
            "params": {
                "recipientAddress": recipient,
                "tokenType": "0000000000000000000000000000000000000000000000000000000000000000",
                "amountAtomicUnits": "1000000"
            }
        })
        .to_string(),
    );
    assert_eq!(replay_before_state_advances[0]["error"]["code"], "conflict");
}
