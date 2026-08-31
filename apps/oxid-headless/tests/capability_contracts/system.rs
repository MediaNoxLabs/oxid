// SPDX-License-Identifier: Apache-2.0

use oxid_diagnostics_application::CLEAR_LOCAL_DIAGNOSTICS_INTENT;
use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use oxid_passport_vault_application::{
    AUTHORIZE_PASSPORT_VAULT_CALL_INTENT, CLAIM_INTENT, CREATE_LOCK_INTENT, DEPOSIT_INTENT,
    SUBMIT_PASSPORT_VAULT_CALL_INTENT, WITHDRAW_INTENT,
};
use serde_json::json;

use super::support::{execute, execute_with_wallet};

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
