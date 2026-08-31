// SPDX-License-Identifier: Apache-2.0

use std::{thread, time::Duration};

use oxid_headless::{HeadlessWallet, PROTOCOL_VERSION};
use serde_json::json;

use super::support::execute_with_wallet;

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
