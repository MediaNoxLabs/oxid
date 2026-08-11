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
            .env_remove("OXID_MIDNIGHT_NETWORK_ID")
            .env_remove("OXID_MIDNIGHT_INDEXER_WS_URL")
            .env_remove("OXID_MIDNIGHT_UNSHIELDED_ADDRESS");
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

const LIVE_ADDRESS: &str =
    "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
const NIGHT_TOKEN_TYPE: &str = "0000000000000000000000000000000000000000000000000000000000000000";

// The upstream handshake callback fixes a large HTTP response as its error
// type; this test must use that signature to negotiate the GraphQL subprotocol.
#[allow(clippy::result_large_err)]
fn spawn_indexer_fixture() -> (String, thread::JoinHandle<()>) {
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
            assert_eq!(subscribe["payload"]["variables"]["address"], LIVE_ADDRESS);
            assert!(subscribe["payload"]["query"].as_str().is_some_and(|query| {
                query.contains("highestTransactionId") && query.contains("fee")
            }));

            send_fixture_event(
                &mut socket,
                json!({
                    "unshieldedTransactions": {
                        "__typename": "UnshieldedTransactionsProgress",
                        "highestTransactionId": 2
                    }
                }),
            )
            .await;
            send_fixture_event(
                &mut socket,
                transaction_event(
                    1,
                    "11",
                    41,
                    vec![utxo("aa", 0, "3000000")],
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
                    vec![utxo("bb", 0, "2500000")],
                    vec![utxo("aa", 0, "3000000")],
                    "SUCCESS",
                    "1500",
                ),
            )
            .await;

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

fn utxo(intent_byte: &str, output_index: i64, value: &str) -> Value {
    json!({
        "owner": LIVE_ADDRESS,
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
fn executable_fails_startup_on_partial_live_configuration_without_echoing_values() {
    let store = TestStore::new();
    let output = Command::new(env!("CARGO_BIN_EXE_oxid-headless"))
        .env("OXID_PROFILE_STORE_PATH", &store.path)
        .env("OXID_MIDNIGHT_NETWORK_ID", "undeployed")
        .env_remove("OXID_MIDNIGHT_INDEXER_WS_URL")
        .env_remove("OXID_MIDNIGHT_UNSHIELDED_ADDRESS")
        .output()
        .expect("headless wallet should report startup failure");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("startup error should be UTF-8");
    assert!(stderr.contains("requires network, indexer WebSocket, and unshielded address"));
    assert!(!stderr.contains(LIVE_ADDRESS));
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

    for (algorithm, key_ref) in [("ed25519", &ed25519_ref), ("p256", &p256_ref)] {
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
        assert!(
            signed["result"]["signatureHex"]
                .as_str()
                .is_some_and(|signature| !signature.is_empty())
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
    for key_ref in [&ed25519_ref, &p256_ref] {
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

    let before = process.request(json!({
        "protocol": "oxid.headless.v1",
        "id": "account-before-sync",
        "method": "wallet.account.get",
        "params": {}
    }));
    assert_eq!(before["result"]["account"]["source"], "simulated");
    assert_eq!(before["result"]["account"]["sync"]["state"], "never_synced");
    assert_eq!(before["result"]["account"]["balances"], json!([]));
    assert!(
        before["result"]["account"]["addresses"][0]["value"]
            .as_str()
            .is_some_and(|address| address.starts_with("mn_addr_undeployed1"))
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
fn executable_syncs_a_live_standalone_indexer_without_secret_input() {
    let (endpoint, server) = spawn_indexer_fixture();
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
        LIVE_ADDRESS
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
