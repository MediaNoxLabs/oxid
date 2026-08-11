// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    fs,
    io::{BufRead as _, BufReader, Write as _},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct ProcessHarness {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl ProcessHarness {
    fn spawn(store_path: &PathBuf) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_oxid-headless"))
            .env("OXID_PROFILE_STORE_PATH", store_path)
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
