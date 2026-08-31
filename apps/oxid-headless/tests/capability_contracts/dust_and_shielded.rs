// SPDX-License-Identifier: Apache-2.0

use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use serde_json::json;

use super::support::execute_with_wallet;

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
