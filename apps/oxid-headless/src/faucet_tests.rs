// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use serde_json::Value;

use super::*;

const RECIPIENT_A: &str = "mn_addr_undeployed1recipient_a";
const RECIPIENT_B: &str = "mn_addr_undeployed1recipient_b";

struct FakeGrant {
    calls: Arc<AtomicUsize>,
    result: Result<GrantOutcome, GrantError>,
}

impl NightGrantPort for FakeGrant {
    fn grant(&self, _: &str) -> Result<GrantOutcome, GrantError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.clone()
    }
}

fn faucet(result: Result<GrantOutcome, GrantError>) -> (StandaloneFaucet, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        StandaloneFaucet {
            grant: Box::new(FakeGrant {
                calls: Arc::clone(&calls),
                result,
            }),
            receipts: VecDeque::new(),
        },
        calls,
    )
}

fn successful_outcome() -> Result<GrantOutcome, GrantError> {
    Ok(GrantOutcome {
        transaction_id: "transaction_public".to_owned(),
        block_id: "block_public".to_owned(),
    })
}

fn execute(faucet: &mut StandaloneFaucet, input: &str) -> Vec<Value> {
    execute_bytes(faucet, input.as_bytes())
}

fn execute_bytes(faucet: &mut StandaloneFaucet, input: &[u8]) -> Vec<Value> {
    let mut output = Vec::new();
    faucet.run(input, &mut output).expect("protocol exchange");
    String::from_utf8(output)
        .expect("UTF-8 output")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON response"))
        .collect()
}

fn fund_request(id: &str, request_id: &str, recipient: &str) -> String {
    serde_json::json!({
        "protocol": PROTOCOL_VERSION,
        "id": id,
        "method": "faucet.fund",
        "params": {
            "requestId": request_id,
            "recipientAddress": recipient
        }
    })
    .to_string()
}

#[test]
fn health_reports_the_fixed_local_development_contract() {
    let (mut faucet, calls) = faucet(successful_outcome());
    let input = format!(
        "{{\"protocol\":\"{PROTOCOL_VERSION}\",\"id\":\"health\",\"method\":\"faucet.health\",\"params\":{{}}}}\n\
         {{\"protocol\":\"{PROTOCOL_VERSION}\",\"id\":\"done\",\"method\":\"faucet.shutdown\",\"params\":{{}}}}\n"
    );

    let responses = execute(&mut faucet, &input);

    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["result"]["ready"], true);
    assert_eq!(responses[0]["result"]["protocol"], PROTOCOL_VERSION);
    assert_eq!(responses[0]["result"]["networkId"], "undeployed");
    assert_eq!(responses[0]["result"]["routeProfile"], "localhost");
    assert_eq!(
        responses[0]["result"]["grant"]["atomicUnits"],
        "50000000000"
    );
    assert_eq!(responses[0]["result"]["maxInFlight"], 1);
    assert_eq!(responses[1]["result"]["shuttingDown"], true);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn retries_and_same_recipient_requests_do_not_fund_twice() {
    let (mut faucet, calls) = faucet(successful_outcome());
    let input = [
        fund_request("first", "request-a", RECIPIENT_A),
        fund_request("retry", "request-a", RECIPIENT_A),
        fund_request("same-recipient", "request-b", RECIPIENT_A),
        fund_request("alias-conflict", "request-b", RECIPIENT_B),
    ]
    .join("\n");

    let responses = execute(&mut faucet, &(input + "\n"));

    assert_eq!(responses.len(), 4);
    assert_eq!(responses[0]["result"]["receipt"]["deduplicated"], false);
    assert_eq!(responses[1]["result"]["receipt"]["deduplicated"], true);
    assert_eq!(responses[2]["result"]["receipt"]["deduplicated"], true);
    assert_eq!(
        responses[0]["result"]["receipt"]["transactionId"],
        responses[2]["result"]["receipt"]["transactionId"]
    );
    assert_eq!(responses[2]["result"]["receipt"]["requestId"], "request-b");
    assert_eq!(responses[3]["error"]["code"], "idempotency_conflict");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn request_identifier_cannot_be_rebound_to_another_recipient() {
    let (mut faucet, calls) = faucet(successful_outcome());
    let input = [
        fund_request("first", "request-a", RECIPIENT_A),
        fund_request("conflict", "request-a", RECIPIENT_B),
    ]
    .join("\n");

    let responses = execute(&mut faucet, &(input + "\n"));

    assert_eq!(responses[0]["ok"], true);
    assert_eq!(responses[1]["ok"], false);
    assert_eq!(responses[1]["error"]["code"], "idempotency_conflict");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn callers_cannot_select_amount_network_or_secret_bearing_input() {
    let (mut faucet, calls) = faucet(successful_outcome());
    let caller_amount = serde_json::json!({
        "protocol": PROTOCOL_VERSION,
        "id": "amount",
        "method": "faucet.fund",
        "params": {
            "requestId": "request-a",
            "recipientAddress": RECIPIENT_A,
            "amount": "1"
        }
    });
    let inputs = [
        caller_amount.to_string(),
        fund_request("preprod", "request-b", "mn_addr_preprod1recipient"),
        fund_request(
            "shielded",
            "request-c",
            "mn_shield-addr_undeployed1recipient",
        ),
        fund_request(
            "credential",
            "request-d",
            "https://user:password@example.invalid/address",
        ),
    ]
    .join("\n");

    let responses = execute(&mut faucet, &(inputs + "\n"));

    assert_eq!(responses.len(), 4);
    assert!(responses.iter().all(|response| response["ok"] == false));
    assert!(
        responses
            .iter()
            .all(|response| response["error"]["code"] == "invalid_params")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn grant_failures_are_closed_codes_without_adapter_payloads() {
    let (mut faucet, calls) = faucet(Err(GrantError::OutcomeUnknown));

    let input = [
        fund_request("fund", "request-a", RECIPIENT_A),
        fund_request("retry", "request-a", RECIPIENT_A),
        fund_request("same-recipient", "request-b", RECIPIENT_A),
        fund_request("alias-conflict", "request-b", RECIPIENT_B),
    ]
    .join("\n");
    let responses = execute(&mut faucet, &(input + "\n"));

    assert_eq!(responses[0]["ok"], false);
    assert_eq!(responses[0]["error"]["code"], "outcome_unknown");
    assert_eq!(
        responses[0]["error"]["message"],
        "funding transaction outcome is not yet known"
    );
    assert!(responses[..3].iter().all(|response| {
        response["error"]["code"] == "outcome_unknown"
            && !response.to_string().contains(RECIPIENT_A)
    }));
    assert_eq!(responses[3]["error"]["code"], "idempotency_conflict");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn protocol_and_line_size_are_bounded() {
    let (mut faucet, calls) = faucet(successful_outcome());
    let oversized = format!(
        "{{\"payload\":\"{}\"}}\n",
        "x".repeat(MAX_REQUEST_BYTES * 4)
    );
    let unsupported =
        "{\"protocol\":\"wrong\",\"id\":\"one\",\"method\":\"faucet.health\",\"params\":{}}\n";

    let responses = execute(&mut faucet, &(oversized + unsupported));

    assert_eq!(responses[0]["error"]["code"], "request_too_large");
    assert_eq!(responses[1]["error"]["code"], "unsupported_protocol");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn invalid_utf8_is_rejected_without_desynchronizing_the_next_frame() {
    let (mut faucet, calls) = faucet(successful_outcome());
    let mut input = vec![0xff, b'\n'];
    input.extend_from_slice(
        format!(
            "{{\"protocol\":\"{PROTOCOL_VERSION}\",\"id\":\"health\",\"method\":\"faucet.health\",\"params\":{{}}}}\n"
        )
        .as_bytes(),
    );

    let responses = execute_bytes(&mut faucet, &input);

    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["error"]["code"], "invalid_request");
    assert_eq!(responses[1]["result"]["ready"], true);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn receipt_cache_evicts_oldest_entry_at_its_fixed_capacity() {
    let (mut faucet, calls) = faucet(successful_outcome());
    for index in 0..=MAX_RECEIPTS {
        let recipient = format!("mn_addr_undeployed1recipient_{index}");
        let input = fund_request("fund", &format!("request-{index}"), &recipient) + "\n";
        let response = execute(&mut faucet, &input);
        assert_eq!(response[0]["ok"], true);
    }

    assert_eq!(faucet.receipts.len(), MAX_RECEIPTS);
    assert_eq!(faucet.receipts.front().unwrap().request_id, "request-1");
    assert_eq!(calls.load(Ordering::SeqCst), MAX_RECEIPTS + 1);
}
