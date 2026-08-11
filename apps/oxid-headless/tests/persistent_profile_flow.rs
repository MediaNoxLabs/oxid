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
