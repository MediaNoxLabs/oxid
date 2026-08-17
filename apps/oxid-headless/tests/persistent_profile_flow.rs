// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    fs,
    io::{BufRead as _, BufReader, Write as _},
    net::TcpListener,
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
};

use futures::{SinkExt as _, StreamExt as _};
use oxid_adapter_openid4vci::standalone_credential_offer;
use serde_json::{Value, json};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{Request, Response},
        http::HeaderValue,
    },
};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct ProcessHarness {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl ProcessHarness {
    fn spawn(store_path: &PathBuf) -> Self {
        Self::spawn_with_environment(store_path, &[])
    }

    fn spawn_with_environment(store_path: &PathBuf, environment: &[(&str, &str)]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
        command
            .env("OXID_PROFILE_STORE_PATH", store_path)
            .env(
                "OXID_DID_STORE_PATH",
                store_path
                    .parent()
                    .expect("profile store should have a parent")
                    .join("private/did-records.json"),
            )
            .env_remove("OXID_MIDNIGHT_NETWORK_ID")
            .env_remove("OXID_MIDNIGHT_INDEXER_WS_URL")
            .env_remove("OXID_MIDNIGHT_UNSHIELDED_ADDRESS")
            .env_remove("OXID_MIDNIGHT_INDEXER_HTTP_URL")
            .env_remove("OXID_MIDNIGHT_NODE_WS_URL")
            .env_remove("OXID_MIDNIGHT_PROOF_SERVER_URL")
            .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
            .env_remove("OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_DUST_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH")
            .env_remove("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT")
            .env_remove("OXID_PASSPORT_VAULT_STORE_PATH")
            .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
            .env_remove("OXID_CREDENTIAL_STORE_PATH")
            .env_remove("OXID_CREDENTIAL_KEY_PATH");
        command.env_remove("OXID_MIDNIGHT_DID_RESOLVER_URL");
        for (key, value) in environment {
            command.env(key, value);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("headless wallet should start");
        let input = child.stdin.take().expect("stdin should be piped");
        let output = BufReader::new(child.stdout.take().expect("stdout should be piped"));

        Self {
            child,
            input,
            output,
        }
    }

    fn request(&mut self, request: Value) -> Value {
        serde_json::to_writer(&mut self.input, &request).expect("request should serialize");
        self.input
            .write_all(b"\n")
            .and_then(|()| self.input.flush())
            .expect("request should be written");

        let mut line = String::new();
        self.output
            .read_line(&mut line)
            .expect("response should be readable");
        assert!(!line.is_empty(), "headless wallet ended before responding");
        serde_json::from_str(&line).expect("response should be JSON")
    }

    fn quit(mut self) {
        let response = self.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "quit",
            "method": "system.quit",
            "params": {}
        }));
        assert_eq!(response["ok"], true);
        assert!(
            self.child
                .wait()
                .expect("headless wallet should exit")
                .success()
        );
    }
}

fn wait_for_shielded_sync(process: &mut ProcessHarness, prefix: &str) -> Value {
    for attempt in 0..200 {
        let response = process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": format!("{prefix}-{attempt}"),
            "method": "wallet.shielded.sync.status",
            "params": {}
        }));
        let state = response["result"]["shieldedSync"]["state"]
            .as_str()
            .expect("shielded status should have a state");
        if !matches!(state, "syncing" | "cached") {
            return response;
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("shielded worker did not reach a terminal state");
}

const LIVE_ADDRESS: &str =
    "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
const NIGHT_TOKEN_TYPE: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const FOREIGN_ZSWAP_OUTPUT: &str = "6d69646e696768743a6576656e745b76395d3a0400a90200000000000000000000000000000000000000000000000000000000000000000000000001c4ef4c0723d6e09b1cac903d1a717274bd2c0633cb9c3cf69047ce5655dc2be9017fe874ddd951049b65bb24127764920e85d04bd1ff724d390d4022b83a6157ed0000000000000000000000000000000000000000000000000000000000000000140019d316b8bc931a9fb308370cc43c6bf7fed9e484a5a7e961ec4b68fd9524e6020100";

// The upstream handshake callback fixes a large HTTP response as its error
// type; this test must use that signature to negotiate the GraphQL subprotocol.
#[allow(clippy::result_large_err)]
fn spawn_indexer_fixture(
    expected_transaction_id: i64,
    incremental: bool,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("fixture listener should bind");
    listener
        .set_nonblocking(true)
        .expect("fixture listener should become nonblocking");
    let port = listener
        .local_addr()
        .expect("fixture address should be available")
        .port();
    let endpoint = format!("ws://127.0.0.1:{port}/api/v4/graphql/ws");
    let handle = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("fixture runtime should build");
        runtime.block_on(async move {
            let listener =
                tokio::net::TcpListener::from_std(listener).expect("listener should convert");
            let (stream, _) = listener
                .accept()
                .await
                .expect("fixture should accept client");
            let callback = |request: &Request, mut response: Response| {
                assert_eq!(
                    request
                        .headers()
                        .get("Sec-WebSocket-Protocol")
                        .and_then(|value| value.to_str().ok()),
                    Some("graphql-transport-ws")
                );
                response.headers_mut().insert(
                    "Sec-WebSocket-Protocol",
                    HeaderValue::from_static("graphql-transport-ws"),
                );
                Ok(response)
            };
            let mut socket = accept_hdr_async(stream, callback)
                .await
                .expect("fixture handshake should succeed");
            let init = socket
                .next()
                .await
                .expect("connection init should arrive")
                .expect("connection init should be readable");
            let init: Value = serde_json::from_str(
                init.into_text()
                    .expect("connection init should be text")
                    .as_str(),
            )
            .expect("connection init should be JSON");
            assert_eq!(init["type"], "connection_init");
            socket
                .send(Message::Text(
                    json!({ "type": "connection_ack" }).to_string().into(),
                ))
                .await
                .expect("ack should send");

            let subscribe = socket
                .next()
                .await
                .expect("subscribe should arrive")
                .expect("subscribe should be readable");
            let subscribe: Value = serde_json::from_str(
                subscribe
                    .into_text()
                    .expect("subscribe should be text")
                    .as_str(),
            )
            .expect("subscribe should be JSON");
            assert_eq!(subscribe["type"], "subscribe");
            let subscribed_address = subscribe["payload"]["variables"]["address"]
                .as_str()
                .expect("subscription address should be a string")
                .to_owned();
            assert!(subscribed_address.starts_with("mn_addr_devnet1"));
            assert_eq!(
                subscribe["payload"]["variables"]["transactionId"],
                expected_transaction_id
            );
            assert!(subscribe["payload"]["query"].as_str().is_some_and(|query| {
                query.contains("highestTransactionId") && query.contains("fee")
            }));

            let target = if incremental { 3 } else { 2 };
            send_fixture_event(
                &mut socket,
                json!({
                    "unshieldedTransactions": {
                        "__typename": "UnshieldedTransactionsProgress",
                        "highestTransactionId": target
                    }
                }),
            )
            .await;
            if incremental {
                send_fixture_event(
                    &mut socket,
                    transaction_event(
                        3,
                        "33",
                        43,
                        vec![utxo(&subscribed_address, "cc", 0, "1000000")],
                        vec![utxo(&subscribed_address, "bb", 0, "2500000")],
                        "SUCCESS",
                        "900",
                    ),
                )
                .await;
            } else {
                send_fixture_event(
                    &mut socket,
                    transaction_event(
                        1,
                        "11",
                        41,
                        vec![utxo(&subscribed_address, "aa", 0, "3000000")],
                        vec![],
                        "SUCCESS",
                        "100",
                    ),
                )
                .await;
                send_fixture_event(
                    &mut socket,
                    transaction_event(
                        2,
                        "22",
                        42,
                        vec![utxo(&subscribed_address, "bb", 0, "2500000")],
                        vec![utxo(&subscribed_address, "aa", 0, "3000000")],
                        "SUCCESS",
                        "1500",
                    ),
                )
                .await;
            }

            let complete = socket
                .next()
                .await
                .expect("complete should arrive")
                .expect("complete should be readable");
            let complete: Value = serde_json::from_str(
                complete
                    .into_text()
                    .expect("complete should be text")
                    .as_str(),
            )
            .expect("complete should be JSON");
            assert_eq!(complete["type"], "complete");
        });
    });
    (endpoint, handle)
}

// The upstream handshake callback fixes a large HTTP response as its error
// type; this test must use that signature to negotiate the GraphQL subprotocol.
#[allow(clippy::result_large_err)]
fn spawn_shielded_indexer_fixture() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("shielded fixture listener should bind");
    listener
        .set_nonblocking(true)
        .expect("shielded fixture listener should become nonblocking");
    let port = listener
        .local_addr()
        .expect("shielded fixture address should be available")
        .port();
    let endpoint = format!("ws://127.0.0.1:{port}/api/v4/graphql/ws");
    let handle = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("shielded fixture runtime should build");
        runtime.block_on(async move {
            let listener =
                tokio::net::TcpListener::from_std(listener).expect("listener should convert");
            for expected_start in [0, 1] {
                let (stream, _) = listener
                    .accept()
                    .await
                    .expect("shielded fixture should accept client");
                let callback = |request: &Request, mut response: Response| {
                    assert_eq!(
                        request
                            .headers()
                            .get("Sec-WebSocket-Protocol")
                            .and_then(|value| value.to_str().ok()),
                        Some("graphql-transport-ws")
                    );
                    response.headers_mut().insert(
                        "Sec-WebSocket-Protocol",
                        HeaderValue::from_static("graphql-transport-ws"),
                    );
                    Ok(response)
                };
                let mut socket = accept_hdr_async(stream, callback)
                    .await
                    .expect("shielded fixture handshake should succeed");
                let _ = socket
                    .next()
                    .await
                    .expect("connection init should arrive")
                    .expect("connection init should be readable");
                socket
                    .send(Message::Text(
                        json!({ "type": "connection_ack" }).to_string().into(),
                    ))
                    .await
                    .expect("ack should send");
                let subscribe = socket
                    .next()
                    .await
                    .expect("subscribe should arrive")
                    .expect("subscribe should be readable");
                let subscribe: Value = serde_json::from_str(
                    subscribe
                        .into_text()
                        .expect("subscribe should be text")
                        .as_str(),
                )
                .expect("subscribe should be JSON");
                assert_eq!(subscribe["payload"]["variables"]["id"], expected_start);
                assert!(
                    subscribe["payload"]["query"]
                        .as_str()
                        .is_some_and(|query| query.contains("zswapLedgerEvents"))
                );
                if expected_start == 0 {
                    socket
                        .send(Message::Text(
                            json!({
                                "type": "next",
                                "id": "oxid-shielded",
                                "payload": {
                                    "data": {
                                        "zswapLedgerEvents": {
                                            "__typename": "ZswapOutput",
                                            "id": 0,
                                            "maxId": 0,
                                            "raw": FOREIGN_ZSWAP_OUTPUT
                                        }
                                    }
                                }
                            })
                            .to_string()
                            .into(),
                        ))
                        .await
                        .expect("shielded event should send");
                } else {
                    socket
                        .send(Message::Text(
                            json!({ "type": "complete", "id": "oxid-shielded" })
                                .to_string()
                                .into(),
                        ))
                        .await
                        .expect("shielded completion should send");
                }
                let _ = socket.next().await;
            }
        });
    });
    (endpoint, handle)
}

async fn send_fixture_event<S>(socket: &mut tokio_tungstenite::WebSocketStream<S>, data: Value)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    socket
        .send(Message::Text(
            json!({
                "type": "next",
                "id": "oxid-account",
                "payload": { "data": data }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("fixture event should send");
}

fn transaction_event(
    id: i64,
    hash_byte: &str,
    height: i64,
    created: Vec<Value>,
    spent: Vec<Value>,
    status: &str,
    fee: &str,
) -> Value {
    json!({
        "unshieldedTransactions": {
            "__typename": "UnshieldedTransaction",
            "transaction": {
                "id": id,
                "hash": hash_byte.repeat(32),
                "block": {
                    "height": height,
                    "timestamp": 1_700_000_000_000_i64 + height
                },
                "__typename": "RegularTransaction",
                "transactionResult": { "status": status },
                "fee": fee
            },
            "createdUtxos": created,
            "spentUtxos": spent
        }
    })
}

fn utxo(owner: &str, intent_byte: &str, output_index: i64, value: &str) -> Value {
    json!({
        "owner": owner,
        "tokenType": NIGHT_TOKEN_TYPE,
        "value": value,
        "intentHash": intent_byte.repeat(32),
        "outputIndex": output_index
    })
}

struct TestStore {
    root: PathBuf,
    path: PathBuf,
}

impl TestStore {
    fn new() -> Self {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "oxid-headless-process-test-{}-{sequence}",
            std::process::id()
        ));
        Self {
            path: root.join("wallet-profiles.json"),
            root,
        }
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn executable_restores_profile_selection_in_a_new_process() {
    let store = TestStore::new();
    let mut first_process = ProcessHarness::spawn(&store.path);
    let created = first_process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "create",
        "method": "wallet.profile.create",
        "params": { "displayName": "Persistent flow" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("created profile should have an identifier")
        .to_owned();
    let selected = first_process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "select",
        "method": "wallet.profile.select",
        "params": { "profileId": profile_id }
    }));
    assert_eq!(
        selected["result"]["profile"]["displayName"],
        "Persistent flow"
    );
    first_process.quit();

    let mut second_process = ProcessHarness::spawn(&store.path);
    let restored = second_process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "active",
        "method": "wallet.profile.active",
        "params": {}
    }));
    assert_eq!(restored["result"]["profile"]["id"], profile_id);
    assert_eq!(
        restored["result"]["profile"]["displayName"],
        "Persistent flow"
    );
    second_process.quit();
}

#[test]
fn executable_restores_encrypted_credentials_in_a_new_process() {
    let store = TestStore::new();
    let mut first_process = ProcessHarness::spawn(&store.path);
    let created = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-create-profile",
        "method": "wallet.profile.create", "params": { "displayName": "Credential owner" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("profile id")
        .to_owned();
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "credential-select-profile",
            "method": "wallet.profile.select", "params": { "profileId": profile_id }
        }))["ok"],
        true
    );
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "credential-security",
            "method": "wallet.security.initialize", "params": {}
        }))["ok"],
        true
    );
    let did_record = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-holder-did",
        "method": "did.create", "params": {}
    }));
    let document = &did_record["result"]["didRecord"]["document"];
    let holder_did = document["id"].as_str().expect("holder DID");
    let method_id = document["relationships"]
        .as_array()
        .expect("DID relationships")
        .iter()
        .find(|relationship| relationship["relationship"] == "authentication")
        .and_then(|relationship| relationship["methodIds"][0].as_str())
        .expect("authentication method");
    let holder_binding_method_id = document["verificationMethods"]
        .as_array()
        .expect("verification methods")
        .iter()
        .find(|method| method["publicKeyJwk"]["crv"] == "Jubjub")
        .and_then(|method| method["id"].as_str())
        .expect("managed Jubjub holder-binding method");
    let prepared = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-offer",
        "method": "credential.issuance.prepare",
        "params": { "offer": standalone_credential_offer() }
    }));
    assert_eq!(prepared["result"]["issuance"]["state"], "awaiting_consent");
    let issuance_id = prepared["result"]["issuance"]["id"]
        .as_str()
        .expect("issuance id");
    let issued = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-accept",
        "method": "credential.issuance.accept",
        "params": {
            "issuanceId": issuance_id,
            "holderDid": holder_did,
            "methodId": method_id,
            "holderBindingMethodId": holder_binding_method_id,
            "confirmed": true,
            "intent": "ACCEPT_CREDENTIAL_ISSUANCE"
        }
    }));
    assert_eq!(issued["result"]["issuance"]["state"], "succeeded");
    let credential_id = issued["result"]["issuance"]["credentialId"]
        .as_str()
        .expect("credential id")
        .to_owned();
    assert!(!issued.to_string().contains("pre-authorized"));
    assert!(!issued.to_string().contains("signedBytes"));
    assert!(!issued.to_string().contains("detachedProof"));
    let disclosure = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-disclosure",
        "method": "credential.disclosure.candidates",
        "params": { "credentialId": credential_id.clone() }
    }));
    assert_eq!(
        disclosure["result"]["disclosure"]["schemaId"],
        "digital-passport:v1"
    );
    assert_eq!(
        disclosure["result"]["disclosure"]["candidates"]
            .as_array()
            .map(Vec::len),
        Some(5)
    );
    assert!(!disclosure.to_string().contains("Alice"));
    first_process.quit();

    let encrypted_path = store.root.join("private/credentials.enc");
    let key_path = store.root.join("private/credentials.key");
    let encrypted = fs::read(&encrypted_path).expect("encrypted credential store");
    assert!(encrypted.starts_with(b"OXIDVC01"));
    assert!(
        !encrypted
            .windows(b"Digital Passport".len())
            .any(|window| window == b"Digital Passport")
    );
    for secret in [b"Alice".as_slice(), b"Example", b"AB1234567"] {
        assert!(
            !encrypted
                .windows(secret.len())
                .any(|window| window == secret)
        );
    }
    for protected in [
        include_str!("../../../fixtures/credentials/standalone-digital-passport-compact-body.b64")
            .trim()
            .as_bytes(),
        include_str!("../../../fixtures/credentials/standalone-digital-passport-compact-proof.b64")
            .trim()
            .as_bytes(),
    ] {
        assert!(
            !encrypted
                .windows(protected.len())
                .any(|window| window == protected),
            "encrypted store must not expose Compact body or proof bytes"
        );
    }
    assert_eq!(
        fs::read(&key_path).expect("development wrapping key").len(),
        32
    );

    let mut second_process = ProcessHarness::spawn(&store.path);
    let listed = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-list-restored",
        "method": "credential.list", "params": {}
    }));
    assert_eq!(listed["result"]["credentials"][0]["id"], credential_id);
    assert_eq!(
        listed["result"]["credentials"][0]["format"],
        "midnight_compact_vc"
    );
    assert_eq!(
        listed["result"]["credentials"][0]["verification"]["outcome"],
        "valid"
    );
    let restored_disclosure = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-disclosure-restored",
        "method": "credential.disclosure.preview",
        "params": {
            "credentialId": credential_id,
            "revealClaimPaths": ["/credentialSubject/firstName"],
            "predicates": [{
                "claimPath": "/credentialSubject/dateOfBirth",
                "kind": "age_over",
                "threshold": 21
            }]
        }
    }));
    assert_eq!(
        restored_disclosure["result"]["plan"]["outcome"],
        "local_preview_ready"
    );
    assert_eq!(
        restored_disclosure["result"]["plan"]["presentationGenerated"],
        false
    );
    let reverified = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-reverify-restored",
        "method": "credential.reverify", "params": { "credentialId": credential_id }
    }));
    assert_eq!(
        reverified["result"]["credential"]["verification"]["outcome"],
        "valid"
    );
    assert_eq!(
        reverified["result"]["credential"]["format"],
        "midnight_compact_vc"
    );
    let stages = reverified["result"]["credential"]["verification"]["stages"]
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
    assert!(!reverified.to_string().contains("detachedProof"));
    let restored_presentation = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-presentation-restored",
        "method": "credential.presentation.prepare",
        "params": { "request": oxid_composition::standalone_openid4vp_request() }
    }));
    let restored_presentation_id = restored_presentation["result"]["presentation"]["id"]
        .as_str()
        .expect("restored presentation id");
    let restored_candidate_id =
        restored_presentation["result"]["presentation"]["candidates"][0]["credentialId"]
            .as_str()
            .expect("restored presentation candidate");
    let rejected_presentation = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-presentation-restored-accept",
        "method": "credential.presentation.accept",
        "params": {
            "presentationId": restored_presentation_id,
            "credentialId": restored_candidate_id,
            "confirmed": true,
            "intent": "ACCEPT_CREDENTIAL_PRESENTATION"
        }
    }));
    assert_eq!(
        rejected_presentation["error"]["code"],
        "holder_not_authorized"
    );
    assert!(!rejected_presentation.to_string().contains("vp_token"));
    let deleted = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-delete-restored",
        "method": "credential.delete",
        "params": {
            "credentialId": credential_id.clone(),
            "confirmed": true,
            "intent": "DELETE_CREDENTIAL"
        }
    }));
    assert_eq!(deleted["result"]["deleted"], true);
    let removed_disclosure = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-disclosure-deleted",
        "method": "credential.disclosure.candidates",
        "params": { "credentialId": credential_id }
    }));
    assert_eq!(removed_disclosure["error"]["code"], "not_found");
    second_process.quit();

    let mut third_process = ProcessHarness::spawn(&store.path);
    let removed_after_restart = third_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "credential-list-after-delete",
        "method": "credential.list", "params": {}
    }));
    assert!(
        removed_after_restart["result"]["credentials"]
            .as_array()
            .expect("credentials")
            .is_empty()
    );
    third_process.quit();
}

#[test]
fn executable_restores_standalone_vault_accounting_and_claim_replay_in_a_new_process() {
    let store = TestStore::new();
    let mut first_process = ProcessHarness::spawn(&store.path);
    let created = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-profile",
        "method": "wallet.profile.create", "params": { "displayName": "Vault owner" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("profile id")
        .to_owned();
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "vault-persist-select",
            "method": "wallet.profile.select", "params": { "profileId": profile_id }
        }))["ok"],
        true
    );
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "vault-persist-security",
            "method": "wallet.security.initialize", "params": {}
        }))["ok"],
        true
    );
    let did_record = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-did",
        "method": "did.create", "params": {}
    }));
    let document = &did_record["result"]["didRecord"]["document"];
    let holder_did = document["id"].as_str().expect("holder DID");
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
    let prepared = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-offer",
        "method": "credential.issuance.prepare",
        "params": { "offer": standalone_credential_offer() }
    }));
    let issuance_id = prepared["result"]["issuance"]["id"]
        .as_str()
        .expect("issuance id");
    let issued = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-accept",
        "method": "credential.issuance.accept",
        "params": {
            "issuanceId": issuance_id,
            "holderDid": holder_did,
            "methodId": method_id,
            "holderBindingMethodId": holder_binding_method_id,
            "confirmed": true,
            "intent": "ACCEPT_CREDENTIAL_ISSUANCE"
        }
    }));
    let credential_id = issued["result"]["issuance"]["credentialId"]
        .as_str()
        .expect("credential id")
        .to_owned();

    let capabilities = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-capabilities",
        "method": "system.capabilities", "params": {}
    }));
    assert_eq!(
        capabilities["result"]["passportVaultState"]["persistence"],
        "owner_private_atomic_file"
    );
    assert_eq!(
        capabilities["result"]["passportVaultState"]["settlesOnMidnight"],
        false
    );

    let lock = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-create",
        "method": "vault.lock.create",
        "params": {
            "minimumAgeYears": 18,
            "requiredIssuingState": "US",
            "requiredDocumentNumber": "AB1234567",
            "maximumClaimAmount": "40",
            "initialAmount": "100",
            "confirmed": true,
            "intent": "CREATE_PASSPORT_VAULT_LOCK"
        }
    }));
    assert_eq!(lock["result"]["lock"]["lockId"], 0);
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "vault-persist-deposit",
            "method": "vault.deposit",
            "params": {"lockId": 0, "amount": "20", "confirmed": true, "intent": "DEPOSIT_TO_PASSPORT_VAULT"}
        }))["result"]["lock"]["remaining"],
        "120"
    );
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "vault-persist-claim",
            "method": "vault.claim",
            "params": {"lockId": 0, "credentialId": credential_id.clone(), "amount": "40", "confirmed": true, "intent": "CLAIM_FROM_PASSPORT_VAULT"}
        }))["result"]["lock"]["remaining"],
        "80"
    );
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "vault-persist-withdraw",
            "method": "vault.withdraw",
            "params": {"lockId": 0, "amount": "10", "confirmed": true, "intent": "WITHDRAW_FROM_PASSPORT_VAULT"}
        }))["result"]["lock"]["remaining"],
        "70"
    );
    first_process.quit();

    let vault_path = store.root.join("private/passport-vault.json");
    let stored = fs::read_to_string(&vault_path).expect("standalone vault store");
    assert!(stored.contains("\"schemaVersion\": 1"));
    assert!(!stored.contains(&credential_id));
    for forbidden in [
        "signedBytes",
        "detachedProof",
        "privateMaterial",
        "holderDid",
    ] {
        assert!(!stored.contains(forbidden));
    }

    let mut second_process = ProcessHarness::spawn(&store.path);
    let restored = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-restored",
        "method": "vault.locks.list", "params": {}
    }));
    assert_eq!(restored["result"]["vault"]["source"], "standalone");
    assert_eq!(restored["result"]["vault"]["totalDeposited"], "120");
    assert_eq!(restored["result"]["vault"]["totalReleased"], "50");
    assert_eq!(restored["result"]["vault"]["totalLocked"], "70");
    assert_eq!(restored["result"]["vault"]["claimCount"], 1);
    assert_eq!(restored["result"]["vault"]["locks"][0]["remaining"], "70");

    let replay = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-replay",
        "method": "vault.claim",
        "params": {"lockId": 0, "credentialId": credential_id.clone(), "amount": "1", "confirmed": true, "intent": "CLAIM_FROM_PASSPORT_VAULT"}
    }));
    assert_eq!(replay["error"]["code"], "conflict");
    let next_lock = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-next-lock",
        "method": "vault.lock.create",
        "params": {
            "minimumAgeYears": 21,
            "maximumClaimAmount": "5",
            "initialAmount": "5",
            "confirmed": true,
            "intent": "CREATE_PASSPORT_VAULT_LOCK"
        }
    }));
    assert_eq!(next_lock["result"]["lock"]["lockId"], 1);
    second_process.quit();

    let mut third_process = ProcessHarness::spawn(&store.path);
    let final_state = third_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "vault-persist-final",
        "method": "vault.locks.list", "params": {}
    }));
    assert_eq!(
        final_state["result"]["vault"]["locks"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(final_state["result"]["vault"]["claimCount"], 1);
    third_process.quit();
}

#[test]
fn executable_restores_profile_scoped_did_inventory_in_a_new_process() {
    const FIXTURE_DID: &str =
        "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let store = TestStore::new();
    let mut first_process = ProcessHarness::spawn(&store.path);
    let created = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "did-create-profile",
        "method": "wallet.profile.create", "params": { "displayName": "DID owner" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("profile id")
        .to_owned();
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "did-select-profile",
            "method": "wallet.profile.select", "params": { "profileId": profile_id }
        }))["ok"],
        true
    );
    let resolved = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "did-resolve",
        "method": "did.resolve", "params": { "did": FIXTURE_DID }
    }));
    assert_eq!(resolved["ok"], true);
    assert_eq!(
        resolved["result"]["didRecord"]["document"]["id"],
        FIXTURE_DID
    );
    assert_eq!(resolved["result"]["didRecord"]["source"], "standalone");
    assert_eq!(
        resolved["result"]["didRecord"]["document"]["verificationMethods"]
            .as_array()
            .expect("methods")
            .len(),
        2
    );
    let unknown = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "did-unknown", "method": "did.resolve",
        "params": { "did": format!("did:midnight:undeployed:{}", "f".repeat(64)) }
    }));
    assert_eq!(unknown["error"]["code"], "not_found");
    first_process.quit();

    let mut second_process = ProcessHarness::spawn(&store.path);
    let inventory = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "did-list", "method": "did.list", "params": {}
    }));
    assert_eq!(
        inventory["result"]["didRecords"][0]["document"]["id"],
        FIXTURE_DID
    );
    assert_eq!(inventory["result"]["didRecords"][0]["source"], "stored");
    assert_eq!(second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "did-get", "method": "did.get", "params": { "did": FIXTURE_DID }
    }))["ok"], true);
    assert_eq!(second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "did-forget", "method": "did.forget", "params": { "did": FIXTURE_DID }
    }))["result"]["forgotten"], true);
    assert!(second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "did-list-empty", "method": "did.list", "params": {}
    }))["result"]["didRecords"].as_array().expect("records").is_empty());
    second_process.quit();
}

#[test]
fn executable_restores_managed_did_as_public_but_not_owned_after_restart() {
    let store = TestStore::new();
    let mut first_process = ProcessHarness::spawn(&store.path);
    let created = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "managed-create-profile",
        "method": "wallet.profile.create", "params": { "displayName": "Managed DID" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("profile id")
        .to_owned();
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "managed-select",
            "method": "wallet.profile.select", "params": { "profileId": profile_id }
        }))["ok"],
        true
    );
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1", "id": "managed-security",
            "method": "wallet.security.initialize", "params": {}
        }))["ok"],
        true
    );
    let created_did = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "managed-did-create",
        "method": "did.create", "params": {}
    }));
    assert_eq!(created_did["ok"], true);
    let did = created_did["result"]["didRecord"]["document"]["id"]
        .as_str()
        .expect("created did")
        .to_owned();
    let updated = first_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "managed-did-update",
        "method": "did.update", "params": {
            "operation": "addAlsoKnownAs",
            "did": did,
            "value": "https://example.test/managed",
            "confirmation": {
                "title": "Update DID document",
                "summary": "Authorize the visible alias change",
                "confirmed": true
            }
        }
    }));
    assert_eq!(updated["ok"], true);
    assert_eq!(
        updated["result"]["didRecord"]["documentMetadata"]["versionId"],
        "standalone-2"
    );
    assert!(!updated.to_string().contains("key_"));
    first_process.quit();

    let mut second_process = ProcessHarness::spawn(&store.path);
    let restored = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "managed-did-get",
        "method": "did.get", "params": { "did": did }
    }));
    assert_eq!(restored["result"]["didRecord"]["source"], "stored");
    assert_eq!(
        restored["result"]["didRecord"]["document"]["alsoKnownAs"][0],
        "https://example.test/managed"
    );
    let unmanaged = second_process.request(json!({
        "protocol": "oxid.headless.v1", "id": "managed-did-update-after-restart",
        "method": "did.update", "params": {
            "operation": "removeAlsoKnownAs",
            "did": did,
            "value": "https://example.test/managed",
            "confirmation": {
                "title": "Update DID document",
                "summary": "Authorize the visible alias change",
                "confirmed": true
            }
        }
    }));
    assert_eq!(unmanaged["error"]["code"], "failed_precondition");
    assert_eq!(
        unmanaged["error"]["message"],
        "DID is not managed by the current protected session"
    );
    second_process.quit();
}

#[test]
fn executable_restores_public_submission_status_in_a_new_process() {
    let store = TestStore::new();
    let journal_path = store.root.join("private/submission-journal.json");
    let journal_path = journal_path
        .to_str()
        .expect("fixture journal path should be Unicode");
    let environment = [("OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH", journal_path)];
    let mut first_process = ProcessHarness::spawn_with_environment(&store.path, &environment);
    let created = first_process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "submission-create",
        "method": "wallet.profile.create",
        "params": { "displayName": "Submission recovery" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("created profile should have an identifier")
        .to_owned();
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "submission-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }))["ok"],
        true
    );
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "submission-initialize",
            "method": "wallet.security.initialize",
            "params": {}
        }))["ok"],
        true
    );
    let derived = first_process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "submission-derive",
        "method": "wallet.account.derive",
        "params": {}
    }));
    let recipient = derived["result"]["account"]["receiveAddress"]["value"]
        .as_str()
        .expect("derived receive address should be public")
        .to_owned();
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "submission-sync",
            "method": "wallet.connect",
            "params": {}
        }))["ok"],
        true
    );
    let prepared = first_process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "submission-prepare",
        "method": "wallet.transaction.prepare_unshielded",
        "params": {
            "recipientAddress": recipient,
            "amountAtomicUnits": "1500000"
        }
    }));
    let transfer = &prepared["result"]["transfer"];
    let draft_id = transfer["draftId"]
        .as_str()
        .expect("draft identifier should be public")
        .to_owned();
    let challenge = transfer["authorizationChallenge"]
        .as_str()
        .expect("authorization challenge should be public")
        .to_owned();
    assert_eq!(
        first_process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "submission-authorize",
            "method": "wallet.transaction.authorize_unshielded",
            "params": {
                "draftId": draft_id,
                "authorizationChallenge": challenge,
                "confirmation": {
                    "title": "Authorize NIGHT transfer",
                    "summary": "Authorize the persistent submission fixture",
                    "confirmed": true
                }
            }
        }))["ok"],
        true
    );
    let submitted = first_process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "submission-submit",
        "method": "wallet.transaction.submit_unshielded",
        "params": {
            "draftId": draft_id,
            "confirmation": {
                "title": "Submit NIGHT transfer",
                "summary": "Submit the persistent submission fixture",
                "confirmed": true
            }
        }
    }));
    assert_eq!(submitted["ok"], true);
    let transaction_id = submitted["result"]["submission"]["transactionId"]
        .as_str()
        .expect("transaction identifier should be public")
        .to_owned();
    first_process.quit();

    let mut second_process = ProcessHarness::spawn_with_environment(&store.path, &environment);
    let history = second_process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "submission-history",
        "method": "wallet.transaction.submission_history",
        "params": {}
    }));
    assert_eq!(history["result"]["submissions"][0]["draftId"], draft_id);
    assert_eq!(history["result"]["submissions"][0]["state"], "included");
    assert_eq!(
        history["result"]["submissions"][0]["transactionId"],
        transaction_id
    );
    assert_eq!(
        history["result"]["submissions"][0]["reconciliationAllowed"],
        false
    );
    let restored = second_process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "submission-status",
        "method": "wallet.transaction.submission_status",
        "params": { "draftId": draft_id }
    }));
    assert_eq!(restored["result"]["submissionStatus"]["state"], "included");
    assert_eq!(
        restored["result"]["submissionStatus"]["transactionId"],
        transaction_id
    );
    second_process.quit();
}

#[test]
fn executable_fails_startup_on_partial_live_configuration_without_echoing_values() {
    let store = TestStore::new();
    let output = Command::new(env!("CARGO_BIN_EXE_oxid-headless"))
        .env("OXID_PROFILE_STORE_PATH", &store.path)
        .env("OXID_MIDNIGHT_NETWORK_ID", "undeployed")
        .env_remove("OXID_MIDNIGHT_INDEXER_WS_URL")
        .env_remove("OXID_MIDNIGHT_UNSHIELDED_ADDRESS")
        .env_remove("OXID_MIDNIGHT_INDEXER_HTTP_URL")
        .env_remove("OXID_MIDNIGHT_NODE_WS_URL")
        .env_remove("OXID_MIDNIGHT_PROOF_SERVER_URL")
        .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
        .env_remove("OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH")
        .env_remove("OXID_MIDNIGHT_DUST_CHECKPOINT_PATH")
        .env_remove("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH")
        .env_remove("OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH")
        .env_remove("OXID_MIDNIGHT_DID_RESOLVER_URL")
        .env_remove("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT")
        .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
        .output()
        .expect("headless wallet should report startup failure");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("startup error should be UTF-8");
    assert!(stderr.contains("requires the read-only indexer values"));
    assert!(!stderr.contains(LIVE_ADDRESS));
}

#[test]
fn executable_requires_standalone_routes_for_authenticated_vault_replay() {
    let store = TestStore::new();
    let output = Command::new(env!("CARGO_BIN_EXE_oxid-headless"))
        .env("OXID_PROFILE_STORE_PATH", &store.path)
        .env("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT", "42")
        .env_remove("OXID_MIDNIGHT_NETWORK_ID")
        .env_remove("OXID_MIDNIGHT_INDEXER_WS_URL")
        .env_remove("OXID_MIDNIGHT_UNSHIELDED_ADDRESS")
        .env_remove("OXID_MIDNIGHT_INDEXER_HTTP_URL")
        .env_remove("OXID_MIDNIGHT_NODE_WS_URL")
        .env_remove("OXID_MIDNIGHT_PROOF_SERVER_URL")
        .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
        .env_remove("OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH")
        .env_remove("OXID_MIDNIGHT_DUST_CHECKPOINT_PATH")
        .env_remove("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH")
        .env_remove("OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH")
        .env_remove("OXID_MIDNIGHT_DID_RESOLVER_URL")
        .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
        .output()
        .expect("headless wallet should report startup failure");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("startup error should be UTF-8");
    assert!(stderr.contains("canonical replay requires the complete standalone Midnight routes"));
    assert!(!stderr.contains("42"));
}

#[test]
fn executable_fails_startup_on_partial_credential_configuration_without_echoing_values() {
    let store = TestStore::new();
    let private_route = store.root.join("private/do-not-echo-credentials.enc");
    let output = Command::new(env!("CARGO_BIN_EXE_oxid-headless"))
        .env("OXID_PROFILE_STORE_PATH", &store.path)
        .env("OXID_CREDENTIAL_STORE_PATH", &private_route)
        .env_remove("OXID_CREDENTIAL_KEY_PATH")
        .env_remove("OXID_MIDNIGHT_NETWORK_ID")
        .env_remove("OXID_MIDNIGHT_INDEXER_WS_URL")
        .env_remove("OXID_MIDNIGHT_UNSHIELDED_ADDRESS")
        .env_remove("OXID_MIDNIGHT_INDEXER_HTTP_URL")
        .env_remove("OXID_MIDNIGHT_NODE_WS_URL")
        .env_remove("OXID_MIDNIGHT_PROOF_SERVER_URL")
        .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
        .env_remove("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT")
        .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
        .output()
        .expect("headless wallet should report startup failure");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("startup error should be UTF-8");
    assert!(stderr.contains("credential store and key paths must be configured together"));
    assert!(!stderr.contains("do-not-echo-credentials"));
}

#[test]
fn executable_rejects_a_relative_vault_store_without_echoing_the_path() {
    let store = TestStore::new();
    let output = Command::new(env!("CARGO_BIN_EXE_oxid-headless"))
        .env("OXID_PROFILE_STORE_PATH", &store.path)
        .env(
            "OXID_PASSPORT_VAULT_STORE_PATH",
            "relative-do-not-echo-passport-vault.json",
        )
        .env_remove("OXID_MIDNIGHT_NETWORK_ID")
        .env_remove("OXID_MIDNIGHT_INDEXER_WS_URL")
        .env_remove("OXID_MIDNIGHT_UNSHIELDED_ADDRESS")
        .env_remove("OXID_MIDNIGHT_INDEXER_HTTP_URL")
        .env_remove("OXID_MIDNIGHT_NODE_WS_URL")
        .env_remove("OXID_MIDNIGHT_PROOF_SERVER_URL")
        .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
        .env_remove("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT")
        .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
        .output()
        .expect("headless wallet should report startup failure");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("startup error should be UTF-8");
    assert!(stderr.contains("must be a normalized absolute file path"));
    assert!(!stderr.contains("relative-do-not-echo"));
}

#[test]
fn executable_rejects_insecure_did_resolver_without_echoing_the_route() {
    let store = TestStore::new();
    let output = Command::new(env!("CARGO_BIN_EXE_oxid-headless"))
        .env("OXID_PROFILE_STORE_PATH", &store.path)
        .env(
            "OXID_MIDNIGHT_DID_RESOLVER_URL",
            "http://resolver.example/sensitive-route-name",
        )
        .env_remove("OXID_MIDNIGHT_NETWORK_ID")
        .env_remove("OXID_MIDNIGHT_INDEXER_WS_URL")
        .env_remove("OXID_MIDNIGHT_UNSHIELDED_ADDRESS")
        .env_remove("OXID_MIDNIGHT_INDEXER_HTTP_URL")
        .env_remove("OXID_MIDNIGHT_NODE_WS_URL")
        .env_remove("OXID_MIDNIGHT_PROOF_SERVER_URL")
        .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
        .env_remove("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT")
        .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
        .output()
        .expect("headless wallet should report startup failure");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("startup error should be UTF-8");
    assert!(stderr.contains("non-loopback DID resolver URLs must use HTTPS"));
    assert!(!stderr.contains("sensitive-route-name"));
    assert!(!stderr.contains("resolver.example"));
}

#[test]
fn executable_accepts_private_checkpoints_only_for_supported_live_stacks() {
    let store = TestStore::new();
    let dust_path = store.root.join("private/dust-checkpoints.bin");
    let dust_path = dust_path
        .to_str()
        .expect("fixture checkpoint path is Unicode");
    let shielded_path = store.root.join("private/shielded-checkpoints.bin");
    let shielded_path = shielded_path
        .to_str()
        .expect("fixture checkpoint path is Unicode");
    for (variable, path) in [
        ("OXID_MIDNIGHT_DUST_CHECKPOINT_PATH", dust_path),
        ("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH", shielded_path),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
        command
            .env("OXID_PROFILE_STORE_PATH", &store.path)
            .env_remove("OXID_MIDNIGHT_NETWORK_ID")
            .env_remove("OXID_MIDNIGHT_INDEXER_WS_URL")
            .env_remove("OXID_MIDNIGHT_UNSHIELDED_ADDRESS")
            .env_remove("OXID_MIDNIGHT_INDEXER_HTTP_URL")
            .env_remove("OXID_MIDNIGHT_NODE_WS_URL")
            .env_remove("OXID_MIDNIGHT_PROOF_SERVER_URL")
            .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
            .env_remove("OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_DUST_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH")
            .env_remove("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT")
            .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
            .env(variable, path);
        let output = command
            .output()
            .expect("headless wallet reports an incomplete live boundary");
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .expect("startup error is UTF-8")
                .contains("requires the read-only indexer values")
        );
    }

    let process = ProcessHarness::spawn_with_environment(
        &store.path,
        &[
            ("OXID_MIDNIGHT_NETWORK_ID", "devnet"),
            (
                "OXID_MIDNIGHT_INDEXER_WS_URL",
                "ws://127.0.0.1:18088/api/v1/graphql/ws",
            ),
            (
                "OXID_MIDNIGHT_INDEXER_HTTP_URL",
                "http://127.0.0.1:18088/api/v1/graphql",
            ),
            ("OXID_MIDNIGHT_NODE_WS_URL", "ws://127.0.0.1:19944"),
            ("OXID_MIDNIGHT_PROOF_SERVER_URL", "http://127.0.0.1:16300"),
            ("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", LIVE_ADDRESS),
            ("OXID_MIDNIGHT_DUST_CHECKPOINT_PATH", dust_path),
            ("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH", shielded_path),
        ],
    );
    process.quit();

    let process = ProcessHarness::spawn_with_environment(
        &store.path,
        &[
            ("OXID_MIDNIGHT_NETWORK_ID", "devnet"),
            (
                "OXID_MIDNIGHT_INDEXER_WS_URL",
                "ws://127.0.0.1:18088/api/v1/graphql/ws",
            ),
            ("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", LIVE_ADDRESS),
            ("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH", shielded_path),
        ],
    );
    process.quit();
}

#[test]
fn executable_rebuilds_resumes_and_refreshes_a_live_shielded_checkpoint() {
    let store = TestStore::new();
    let private_directory = store.root.join("private");
    fs::create_dir_all(&private_directory).expect("private fixture directory is created");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        fs::set_permissions(&private_directory, fs::Permissions::from_mode(0o700))
            .expect("private fixture directory permissions are restricted");
    }
    let shielded_path = private_directory.join("shielded-checkpoints.bin");
    fs::write(&shielded_path, b"corrupt-shielded-checkpoint")
        .expect("corrupt fixture checkpoint is written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        fs::set_permissions(&shielded_path, fs::Permissions::from_mode(0o600))
            .expect("private fixture checkpoint permissions are restricted");
    }
    let shielded_path_text = shielded_path
        .to_str()
        .expect("fixture checkpoint path is Unicode");
    let (endpoint, server) = spawn_shielded_indexer_fixture();
    let mut process = ProcessHarness::spawn_with_environment(
        &store.path,
        &[
            ("OXID_MIDNIGHT_NETWORK_ID", "devnet"),
            ("OXID_MIDNIGHT_INDEXER_WS_URL", &endpoint),
            ("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", LIVE_ADDRESS),
            ("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH", shielded_path_text),
        ],
    );
    let created = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "shielded-live-create",
        "method": "wallet.profile.create",
        "params": { "displayName": "Live shielded flow" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("profile has an identifier");
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "shielded-live-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }))["ok"],
        true
    );
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "shielded-live-initialize",
            "method": "wallet.security.initialize",
            "params": {}
        }))["result"]["security"]["state"],
        "unlocked"
    );

    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "shielded-live-start",
            "method": "wallet.shielded.sync.start",
            "params": {}
        }))["result"]["shieldedSync"]["state"],
        "syncing"
    );
    let rebuilt = wait_for_shielded_sync(&mut process, "shielded-live-rebuild");
    let rebuilt = &rebuilt["result"]["shieldedSync"];
    assert_eq!(rebuilt["state"], "synced");
    assert_eq!(rebuilt["currentCursor"], 0);
    assert_eq!(rebuilt["targetCursor"], 0);
    assert_eq!(rebuilt["eventsProcessed"], 1);
    assert_eq!(rebuilt["ownedNoteCount"], 0);
    assert_eq!(rebuilt["commitmentCount"], 1);
    assert_eq!(rebuilt["balances"], json!([]));
    assert!(
        fs::metadata(&shielded_path)
            .expect("replacement checkpoint exists")
            .len()
            > b"corrupt-shielded-checkpoint".len() as u64
    );

    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "shielded-live-resume",
            "method": "wallet.shielded.sync.start",
            "params": {}
        }))["result"]["shieldedSync"]["state"],
        "syncing"
    );
    let refreshed = wait_for_shielded_sync(&mut process, "shielded-live-current");
    let refreshed = &refreshed["result"]["shieldedSync"];
    assert_eq!(refreshed["state"], "synced");
    assert_eq!(refreshed["currentCursor"], 0);
    assert_eq!(refreshed["targetCursor"], 0);
    assert_eq!(refreshed["eventsProcessed"], 0);
    assert_eq!(refreshed["ownedNoteCount"], 0);
    assert_eq!(refreshed["commitmentCount"], 1);
    assert_eq!(refreshed["failure"], Value::Null);
    let encoded = serde_json::to_string(refreshed).expect("shielded response serializes");
    assert!(!encoded.contains(shielded_path_text));
    assert!(!encoded.contains(FOREIGN_ZSWAP_OUTPUT));
    assert!(!encoded.contains("seed"));

    server.join().expect("shielded fixture exits");
    process.quit();
}

#[test]
fn executable_exercises_the_standalone_protected_key_flow() {
    let store = TestStore::new();
    let mut process = ProcessHarness::spawn(&store.path);
    let created = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "create",
        "method": "wallet.profile.create",
        "params": { "displayName": "Standalone secure flow" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("created profile should have an identifier");
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }))["ok"],
        true
    );

    let initial_status = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "status",
        "method": "wallet.security.status",
        "params": {}
    }));
    assert_eq!(
        initial_status["result"]["security"]["state"],
        "uninitialized"
    );
    let initialized = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "initialize",
        "method": "wallet.security.initialize",
        "params": {}
    }));
    assert_eq!(initialized["result"]["security"]["state"], "unlocked");
    assert_eq!(
        initialized["result"]["security"]["protection"],
        "development_only"
    );

    let ed25519 = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "generate-ed25519",
        "method": "wallet.key.generate",
        "params": {
            "label": "Authentication key",
            "algorithm": "ed25519",
            "purpose": "authentication"
        }
    }));
    let ed25519_ref = ed25519["result"]["key"]["keyRef"]
        .as_str()
        .expect("opaque Ed25519 reference should be returned")
        .to_owned();
    assert_eq!(
        ed25519["result"]["key"]["publicKey"]["encoding"],
        "ed25519-compressed"
    );
    let p256 = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "generate-p256",
        "method": "wallet.key.generate",
        "params": {
            "label": "Assertion key",
            "algorithm": "p256",
            "purpose": "assertion"
        }
    }));
    let p256_ref = p256["result"]["key"]["keyRef"]
        .as_str()
        .expect("opaque P-256 reference should be returned")
        .to_owned();
    assert_eq!(
        p256["result"]["key"]["publicKey"]["encoding"],
        "sec1-compressed"
    );
    let jubjub = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "generate-jubjub",
        "method": "wallet.key.generate",
        "params": {
            "label": "Compact holder presentation",
            "algorithm": "jubjub",
            "purpose": "assertion"
        }
    }));
    let jubjub_ref = jubjub["result"]["key"]["keyRef"]
        .as_str()
        .expect("opaque Jubjub reference should be returned")
        .to_owned();
    assert_eq!(
        jubjub["result"]["key"]["publicKey"]["encoding"],
        "jubjub-compressed"
    );
    assert_eq!(
        jubjub["result"]["key"]["publicKey"]["bytesHex"]
            .as_str()
            .map(str::len),
        Some(64)
    );

    for (algorithm, key_ref, signature_hex_length) in [
        ("ed25519", &ed25519_ref, 128),
        ("p256", &p256_ref, 128),
        ("jubjub", &jubjub_ref, 192),
    ] {
        let signed = process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": format!("sign-{algorithm}"),
            "method": "wallet.key.sign",
            "params": {
                "keyRef": key_ref,
                "payloadHex": "7374616e64616c6f6e652d6368616c6c656e6765",
                "confirmation": {
                    "title": "Sign conformance challenge",
                    "summary": "Authorize the public standalone test payload",
                    "confirmed": true
                }
            }
        }));
        assert_eq!(signed["ok"], true, "unexpected response: {signed}");
        assert_eq!(signed["result"]["algorithm"], algorithm);
        assert_eq!(
            signed["result"]["signatureHex"].as_str().map(str::len),
            Some(signature_hex_length)
        );
    }

    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "lock",
            "method": "wallet.security.lock",
            "params": {}
        }))["result"]["security"]["state"],
        "locked"
    );
    let locked_sign = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "locked-sign",
        "method": "wallet.key.sign",
        "params": {
            "keyRef": ed25519_ref,
            "payloadHex": "00",
            "confirmation": {
                "title": "Sign while locked",
                "summary": "This operation must fail closed",
                "confirmed": true
            }
        }
    }));
    assert_eq!(locked_sign["error"]["code"], "wallet_locked");
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "unlock",
            "method": "wallet.security.unlock",
            "params": {}
        }))["result"]["security"]["state"],
        "unlocked"
    );

    let denied_delete = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "delete-denied",
        "method": "wallet.key.delete",
        "params": {
            "keyRef": ed25519_ref,
            "confirmation": {
                "title": "Delete test key",
                "summary": "Remove the ephemeral test key",
                "confirmed": false
            }
        }
    }));
    assert_eq!(denied_delete["error"]["code"], "confirmation_required");
    for key_ref in [&ed25519_ref, &p256_ref, &jubjub_ref] {
        let deleted = process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "delete",
            "method": "wallet.key.delete",
            "params": {
                "keyRef": key_ref,
                "confirmation": {
                    "title": "Delete test key",
                    "summary": "Remove the ephemeral test key",
                    "confirmed": true
                }
            }
        }));
        assert_eq!(deleted["result"]["deleted"], true);
    }
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "list",
            "method": "wallet.key.list",
            "params": {}
        }))["result"]["keys"],
        json!([])
    );
    process.quit();
}

#[test]
fn executable_exercises_midnight_account_parity_without_secret_input() {
    let store = TestStore::new();
    let mut process = ProcessHarness::spawn(&store.path);
    let created = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "account-create",
        "method": "wallet.profile.create",
        "params": { "displayName": "Standalone account flow" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("created profile should have an identifier");
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "account-select-profile",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }))["ok"],
        true
    );

    let networks = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "networks",
        "method": "wallet.network.list",
        "params": {}
    }));
    assert_eq!(networks["result"]["selectedNetworkId"], "undeployed");
    assert!(
        networks["result"]["networks"]
            .as_array()
            .is_some_and(|items| items.len() == 7)
    );

    let before_initialize = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "derive-before-initialize",
        "method": "wallet.account.derive",
        "params": {}
    }));
    assert_eq!(before_initialize["error"]["code"], "failed_precondition");
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "account-security-initialize",
            "method": "wallet.security.initialize",
            "params": {}
        }))["result"]["security"]["state"],
        "unlocked"
    );
    let derived = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "derive-account",
        "method": "wallet.account.derive",
        "params": { "accountIndex": 0, "addressIndex": 0 }
    }));
    assert_eq!(derived["ok"], true, "unexpected response: {derived}");
    assert_eq!(
        derived["result"]["account"]["accountId"],
        "midnight_account_0_0"
    );
    assert_eq!(
        derived["result"]["account"]["custodyMode"],
        "development_only"
    );
    let derived_address = derived["result"]["account"]["receiveAddress"]["value"]
        .as_str()
        .expect("derived public address should be returned")
        .to_owned();
    let transaction_key_ref = derived["result"]["account"]["transactionKeyRef"]
        .as_str()
        .expect("opaque transaction key reference should be returned")
        .to_owned();
    assert!(derived_address.starts_with("mn_addr_undeployed1"));
    assert!(!derived.to_string().contains("seed"));
    assert!(!derived.to_string().contains("private"));

    let repeated = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "derive-account-again",
        "method": "wallet.account.derive",
        "params": { "accountIndex": 0, "addressIndex": 0 }
    }));
    assert_eq!(
        repeated["result"]["account"]["transactionKeyRef"],
        transaction_key_ref
    );
    assert_eq!(
        repeated["result"]["account"]["receiveAddress"]["value"],
        derived_address
    );
    let signed = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "sign-derived-account",
        "method": "wallet.key.sign",
        "params": {
            "keyRef": transaction_key_ref,
            "payloadHex": "4d69646e69676874207472616e73616374696f6e20696e74656e74",
            "confirmation": {
                "title": "Sign Midnight transaction intent",
                "summary": "Authorize the bounded public headless conformance payload",
                "confirmed": true
            }
        }
    }));
    assert_eq!(signed["result"]["algorithm"], "secp256k1-schnorr");
    assert!(
        signed["result"]["signatureHex"]
            .as_str()
            .is_some_and(|signature| signature.len() == 128)
    );
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "derive-account-out-of-bounds",
            "method": "wallet.account.derive",
            "params": { "addressIndex": 2147483648_u64 }
        }))["error"]["code"],
        "invalid_argument"
    );

    let before = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "account-before-sync",
        "method": "wallet.account.get",
        "params": {}
    }));
    assert_eq!(before["result"]["account"]["source"], "simulated");
    assert_eq!(before["result"]["account"]["sync"]["state"], "never_synced");
    assert_eq!(before["result"]["account"]["balances"], json!([]));
    assert_eq!(
        before["result"]["account"]["addresses"][0]["value"],
        derived_address
    );
    let balances_before = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "balances-before-sync",
        "method": "wallet.balance.snapshot",
        "params": {}
    }));
    assert_eq!(balances_before["result"]["balances"], json!([]));
    assert_eq!(balances_before["result"]["sync"]["state"], "never_synced");

    let connected = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "connect",
        "method": "wallet.connect",
        "params": {}
    }));
    assert_eq!(connected["result"]["account"]["sync"]["state"], "synced");
    assert_eq!(connected["result"]["account"]["sync"]["chainTipHeight"], 42);
    assert_eq!(
        connected["result"]["account"]["balances"][0]["atomicUnits"],
        "12000000000000000"
    );
    assert_eq!(
        connected["result"]["account"]["balances"][1]["atomicUnits"],
        "5000000"
    );
    let repeated_after_sync = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "derive-account-after-sync",
        "method": "wallet.account.derive",
        "params": { "accountIndex": 0, "addressIndex": 0 }
    }));
    assert_eq!(
        repeated_after_sync["result"]["account"]["transactionKeyRef"],
        transaction_key_ref
    );
    let balances_after = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "balances-after-sync",
        "method": "wallet.balance.snapshot",
        "params": {}
    }));
    assert_eq!(
        balances_after["result"]["balances"][0]["atomicUnits"],
        "12000000000000000"
    );
    assert_eq!(balances_after["result"]["sync"]["state"], "synced");

    let history = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "history",
        "method": "wallet.transaction.history",
        "params": {}
    }));
    assert_eq!(history["result"]["source"], "simulated");
    assert_eq!(
        history["result"]["transactions"][0]["transactionId"],
        "simulated_outgoing"
    );
    assert_eq!(
        history["result"]["transactions"][0]["direction"],
        "outgoing"
    );

    let prepared = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "transfer-prepare",
        "method": "wallet.transaction.prepare_unshielded",
        "params": {
            "recipientAddress": derived_address,
            "amountAtomicUnits": "1500000"
        }
    }));
    assert_eq!(prepared["ok"], true, "unexpected response: {prepared}");
    assert_eq!(prepared["result"]["transfer"]["state"], "prepared");
    assert_eq!(prepared["result"]["transfer"]["inputCount"], 1);
    assert_eq!(
        prepared["result"]["transfer"]["change"]["atomicUnits"],
        "500000"
    );
    assert_eq!(prepared["result"]["transfer"]["submissionReady"], false);
    let draft_id = prepared["result"]["transfer"]["draftId"]
        .as_str()
        .expect("draft id should be returned")
        .to_owned();
    let challenge = prepared["result"]["transfer"]["authorizationChallenge"]
        .as_str()
        .expect("authorization challenge should be returned")
        .to_owned();
    let authorized = process.request(json!({
        "protocol": "oxid.headless.v1",
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
    }));
    assert_eq!(authorized["ok"], true, "unexpected response: {authorized}");
    assert_eq!(authorized["result"]["transfer"]["state"], "authorized");
    assert_eq!(authorized["result"]["transfer"]["proofRequired"], true);
    assert_eq!(authorized["result"]["transfer"]["submissionReady"], true);
    assert!(!authorized.to_string().contains("signatureHex"));
    assert!(!authorized.to_string().contains("transactionHex"));

    let preprod = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "select-preprod",
        "method": "wallet.network.select",
        "params": { "networkId": "preprod" }
    }));
    assert_eq!(preprod["result"]["selectedNetworkId"], "preprod");
    let preprod_address = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "preprod-address",
        "method": "wallet.address.unshielded",
        "params": {}
    }));
    assert!(
        preprod_address["result"]["address"]["value"]
            .as_str()
            .is_some_and(|address| address.starts_with("mn_addr_preprod1"))
    );
    assert_eq!(preprod_address["result"]["source"], "simulated");

    let rejected = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "unknown-network",
        "method": "wallet.network.select",
        "params": { "networkId": "unknown" }
    }));
    assert_eq!(rejected["error"]["code"], "unsupported_network");
    process.quit();
}

#[test]
fn executable_derives_and_syncs_a_live_account_without_secret_input() {
    let (endpoint, server) = spawn_indexer_fixture(0, false);
    let store = TestStore::new();
    let mut process = ProcessHarness::spawn_with_environment(
        &store.path,
        &[
            ("OXID_MIDNIGHT_NETWORK_ID", "devnet"),
            ("OXID_MIDNIGHT_INDEXER_WS_URL", endpoint.as_str()),
            ("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", LIVE_ADDRESS),
        ],
    );
    let created = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "live-create",
        "method": "wallet.profile.create",
        "params": { "displayName": "Live indexer flow" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("created profile should have an identifier");
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "live-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }))["ok"],
        true
    );

    let watch_only = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "live-watch-only",
        "method": "wallet.account.get",
        "params": {}
    }));
    assert_eq!(
        watch_only["result"]["account"]["addresses"][0]["value"],
        LIVE_ADDRESS
    );
    assert_eq!(
        process.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "live-security-initialize",
            "method": "wallet.security.initialize",
            "params": {}
        }))["result"]["security"]["state"],
        "unlocked"
    );
    let derived = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "live-account-derive",
        "method": "wallet.account.derive",
        "params": {}
    }));
    let derived_address = derived["result"]["account"]["receiveAddress"]["value"]
        .as_str()
        .expect("derived live address should be returned")
        .to_owned();
    assert!(derived_address.starts_with("mn_addr_devnet1"));

    let before = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "live-before",
        "method": "wallet.account.get",
        "params": {}
    }));
    assert_eq!(before["result"]["account"]["networkId"], "devnet");
    assert_eq!(before["result"]["account"]["source"], "live");
    assert_eq!(before["result"]["account"]["sync"]["state"], "never_synced");
    assert_eq!(
        before["result"]["account"]["addresses"][0]["value"],
        derived_address
    );

    let connected = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "live-connect",
        "method": "wallet.connect",
        "params": {}
    }));
    assert_eq!(connected["ok"], true, "unexpected response: {connected}");
    assert_eq!(connected["result"]["account"]["source"], "live");
    assert_eq!(connected["result"]["account"]["sync"]["state"], "synced");
    assert_eq!(connected["result"]["account"]["sync"]["currentCursor"], 2);
    assert_eq!(connected["result"]["account"]["sync"]["targetCursor"], 2);
    assert_eq!(connected["result"]["account"]["sync"]["chainTipHeight"], 42);
    assert_eq!(
        connected["result"]["account"]["balances"][0]["atomicUnits"],
        "2500000"
    );

    let cached = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "live-cached",
        "method": "wallet.balance.snapshot",
        "params": {}
    }));
    assert_eq!(cached["result"]["source"], "cached");
    assert_eq!(cached["result"]["balances"][0]["symbol"], "NIGHT");
    assert_eq!(cached["result"]["balances"][0]["atomicUnits"], "2500000");

    let history = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "live-history",
        "method": "wallet.transaction.history",
        "params": {}
    }));
    assert_eq!(history["result"]["source"], "cached");
    assert_eq!(
        history["result"]["transactions"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(history["result"]["transactions"][0]["blockHeight"], 42);
    assert_eq!(
        history["result"]["transactions"][0]["direction"],
        "outgoing"
    );
    assert_eq!(
        history["result"]["transactions"][0]["fee"]["atomicUnits"],
        "1500"
    );

    process.quit();
    server
        .join()
        .expect("indexer fixture should finish cleanly");
}

#[test]
fn executable_restores_resumes_and_stalls_a_public_account_checkpoint() {
    let store = TestStore::new();
    let checkpoint_path = store.root.join("midnight-account-checkpoints.json");
    let checkpoint = checkpoint_path
        .to_str()
        .expect("checkpoint fixture path should be Unicode");

    let (first_endpoint, first_server) = spawn_indexer_fixture(0, false);
    let mut first = ProcessHarness::spawn_with_environment(
        &store.path,
        &[
            ("OXID_MIDNIGHT_NETWORK_ID", "devnet"),
            ("OXID_MIDNIGHT_INDEXER_WS_URL", first_endpoint.as_str()),
            ("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", LIVE_ADDRESS),
            ("OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH", checkpoint),
        ],
    );
    let created = first.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "checkpoint-create",
        "method": "wallet.profile.create",
        "params": { "displayName": "Checkpoint flow" }
    }));
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("created profile should have an identifier")
        .to_owned();
    assert_eq!(
        first.request(json!({
            "protocol": "oxid.headless.v1",
            "id": "checkpoint-select",
            "method": "wallet.profile.select",
            "params": { "profileId": profile_id }
        }))["ok"],
        true
    );
    let synchronized = first.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "checkpoint-sync",
        "method": "wallet.connect",
        "params": {}
    }));
    assert_eq!(
        synchronized["result"]["account"]["sync"]["currentCursor"],
        2
    );
    assert_eq!(
        synchronized["result"]["account"]["balances"][0]["atomicUnits"],
        "2500000"
    );
    first.quit();
    first_server
        .join()
        .expect("initial indexer fixture should finish cleanly");
    assert!(checkpoint_path.is_file());

    let (second_endpoint, second_server) = spawn_indexer_fixture(3, true);
    let mut second = ProcessHarness::spawn_with_environment(
        &store.path,
        &[
            ("OXID_MIDNIGHT_NETWORK_ID", "devnet"),
            ("OXID_MIDNIGHT_INDEXER_WS_URL", second_endpoint.as_str()),
            ("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", LIVE_ADDRESS),
            ("OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH", checkpoint),
        ],
    );
    let restored = second.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "checkpoint-restored",
        "method": "wallet.account.get",
        "params": {}
    }));
    assert_eq!(restored["result"]["account"]["source"], "cached");
    assert_eq!(restored["result"]["account"]["sync"]["state"], "synced");
    assert_eq!(restored["result"]["account"]["sync"]["currentCursor"], 2);
    assert_eq!(
        restored["result"]["account"]["balances"][0]["atomicUnits"],
        "2500000"
    );

    let resumed = second.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "checkpoint-resume",
        "method": "wallet.connect",
        "params": {}
    }));
    assert_eq!(resumed["result"]["account"]["source"], "live");
    assert_eq!(resumed["result"]["account"]["sync"]["currentCursor"], 3);
    assert_eq!(resumed["result"]["account"]["sync"]["chainTipHeight"], 43);
    assert_eq!(
        resumed["result"]["account"]["balances"][0]["atomicUnits"],
        "1000000"
    );
    assert_eq!(
        resumed["result"]["account"]["transactions"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
    second.quit();
    second_server
        .join()
        .expect("incremental indexer fixture should finish cleanly");

    let mut offline = ProcessHarness::spawn_with_environment(
        &store.path,
        &[
            ("OXID_MIDNIGHT_NETWORK_ID", "devnet"),
            ("OXID_MIDNIGHT_INDEXER_WS_URL", second_endpoint.as_str()),
            ("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", LIVE_ADDRESS),
            ("OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH", checkpoint),
        ],
    );
    let offline_cached = offline.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "checkpoint-offline-read",
        "method": "wallet.balance.snapshot",
        "params": {}
    }));
    assert_eq!(offline_cached["result"]["source"], "cached");
    assert_eq!(offline_cached["result"]["sync"]["currentCursor"], 3);
    assert_eq!(
        offline_cached["result"]["balances"][0]["atomicUnits"],
        "1000000"
    );

    let failed = offline.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "checkpoint-offline-sync",
        "method": "wallet.connect",
        "params": {}
    }));
    assert_eq!(failed["error"]["code"], "capability_unavailable");
    let stalled = offline.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "checkpoint-stalled",
        "method": "wallet.account.get",
        "params": {}
    }));
    assert_eq!(stalled["result"]["account"]["source"], "cached");
    assert_eq!(stalled["result"]["account"]["sync"]["state"], "stalled");
    assert_eq!(stalled["result"]["account"]["sync"]["currentCursor"], 3);
    assert_eq!(
        stalled["result"]["account"]["balances"][0]["atomicUnits"],
        "1000000"
    );
    offline.quit();
}
