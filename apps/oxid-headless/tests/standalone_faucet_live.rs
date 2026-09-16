// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    fs,
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

const ENABLE_ENV: &str = "OXID_ENABLE_LIVE_STANDALONE_FAUCET_E2E";
const NETWORK: &str = "undeployed";
const FIXED_GRANT: u128 = 50_000_000_000;
const DUST_DEADLINE: Duration = Duration::from_secs(10 * 60);
const TRANSFER_A_TO_B: u128 = 10_000_000_000;
const TRANSFER_B_TO_A: u128 = 4_000_000_000;
const INDEXER_WS: &str = "ws://127.0.0.1:8088/api/v4/graphql/ws";
const INDEXER_HTTP: &str = "http://127.0.0.1:8088/api/v4/graphql";
const NODE_WS: &str = "ws://127.0.0.1:9944";
const PROOF_SERVER: &str = "http://127.0.0.1:6300";
const PLACEHOLDER_ADDRESS: &str =
    "mn_addr_undeployed1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9smkp9zh";

struct StateRoot(PathBuf);

impl StateRoot {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "oxid-standalone-faucet-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(path.join("private")).expect("isolated state directory");
        make_owner_private(&path);
        make_owner_private(&path.join("private"));
        Self(path)
    }

    fn child(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.join("private")).expect("isolated child state directory");
        make_owner_private(&path);
        make_owner_private(&path.join("private"));
        path
    }

    fn cleanup(self) {
        fs::remove_dir_all(&self.0).expect("isolated live-test state cleanup");
        assert!(
            !self.0.exists(),
            "isolated live-test state must be absent after cleanup"
        );
    }
}

#[cfg(unix)]
fn make_owner_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .expect("owner-private state permissions");
}

#[cfg(not(unix))]
fn make_owner_private(_: &Path) {}

impl Drop for StateRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ProcessHarness {
    child: Option<Child>,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    error: BufReader<ChildStderr>,
    protocol: &'static str,
}

impl ProcessHarness {
    fn wallet(root: &Path) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
        command
            .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
            .env("OXID_PROFILE_STORE_PATH", root.join("profiles.json"))
            .env(
                "OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH",
                root.join("private/account-checkpoints.json"),
            )
            .env(
                "OXID_MIDNIGHT_DUST_CHECKPOINT_PATH",
                root.join("private/dust-checkpoints.json"),
            )
            .env(
                "OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH",
                root.join("private/shielded-checkpoints.json"),
            )
            .env(
                "OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH",
                root.join("private/submissions.json"),
            )
            .env("OXID_MIDNIGHT_NETWORK_ID", NETWORK)
            .env("OXID_MIDNIGHT_INDEXER_WS_URL", INDEXER_WS)
            .env("OXID_MIDNIGHT_INDEXER_HTTP_URL", INDEXER_HTTP)
            .env("OXID_MIDNIGHT_NODE_WS_URL", NODE_WS)
            .env("OXID_MIDNIGHT_PROOF_SERVER_URL", PROOF_SERVER)
            .env("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", PLACEHOLDER_ADDRESS);
        Self::spawn(command, "oxid.headless.v1")
    }

    fn faucet(root: &Path) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-standalone-faucet"));
        command
            .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
            .env("OXID_ENABLE_STANDALONE_FAUCET", "1")
            .env("OXID_PROFILE_STORE_PATH", root.join("profiles.json"));
        Self::spawn(command, "oxid.standalone-faucet.v1")
    }

    fn spawn(mut command: Command, protocol: &'static str) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("headless child starts");
        Self {
            input: child.stdin.take().expect("child stdin"),
            output: BufReader::new(child.stdout.take().expect("child stdout")),
            error: BufReader::new(child.stderr.take().expect("child stderr")),
            child: Some(child),
            protocol,
        }
    }

    fn request(&mut self, id: &str, method: &str, params: Value) -> Value {
        serde_json::to_writer(
            &mut self.input,
            &json!({"protocol":self.protocol,"id":id,"method":method,"params":params}),
        )
        .expect("request JSON");
        self.input.write_all(b"\n").expect("request newline");
        self.input.flush().expect("request flush");
        let mut line = String::new();
        self.output.read_line(&mut line).expect("response line");
        assert!(!line.is_empty(), "child exited before responding");
        serde_json::from_str(&line).expect("response JSON")
    }

    fn finish(mut self, method: &str) {
        let response = self.request("shutdown", method, json!({}));
        assert_eq!(response["ok"], true, "{response}");
        let mut child = self.child.take().expect("running child");
        assert!(child.wait().expect("child wait").success());
        let mut stderr = String::new();
        self.error
            .read_to_string(&mut stderr)
            .expect("child stderr");
        assert!(stderr.is_empty(), "unexpected child stderr: {stderr}");
    }
}

impl Drop for ProcessHarness {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

struct HttpFaucetHarness {
    child: Option<Child>,
    address: SocketAddr,
}

impl HttpFaucetHarness {
    fn start(root: &Path) -> Self {
        let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
        let address = reservation.local_addr().expect("reserved address");
        drop(reservation);
        let child = Command::new(env!("CARGO_BIN_EXE_oxid-standalone-faucet-http"))
            .env("OXID_ENABLE_STANDALONE_FAUCET", "1")
            .env("OXID_PROFILE_STORE_PATH", root.join("profiles.json"))
            .env("OXID_STANDALONE_FAUCET_HTTP_ADDRESS", address.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("HTTP faucet starts");
        let mut harness = Self {
            child: Some(child),
            address,
        };
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if harness.health_ready() {
                return harness;
            }
            assert!(
                harness
                    .child
                    .as_mut()
                    .expect("HTTP faucet child")
                    .try_wait()
                    .expect("HTTP faucet status")
                    .is_none(),
                "HTTP faucet exited before readiness"
            );
            assert!(
                Instant::now() < deadline,
                "HTTP faucet was not ready before deadline"
            );
            thread::sleep(Duration::from_millis(250));
        }
    }

    fn health_ready(&self) -> bool {
        self.exchange("GET", "/health", None)
            .is_ok_and(|(status, body)| {
                status == 200 && body["ok"] == true && body["result"]["ready"] == true
            })
    }

    fn fund(&self, request_id: &str, recipient_address: &str) -> Value {
        let body = serde_json::to_vec(&json!({
            "requestId": request_id,
            "recipientAddress": recipient_address
        }))
        .expect("funding request JSON");
        let (status, response) = self
            .exchange("POST", "/fund", Some(&body))
            .expect("HTTP funding response");
        assert_eq!(status, 200, "{response}");
        response
    }

    fn exchange(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
    ) -> std::io::Result<(u16, Value)> {
        let mut stream = TcpStream::connect_timeout(&self.address, Duration::from_secs(2))?;
        // The HTTP framing deadline is ten seconds, but a successful funding
        // response waits for the existing prove, submit, and finality path.
        stream.set_read_timeout(Some(Duration::from_secs(180)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let body = body.unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
            if body.is_empty() {
                ""
            } else {
                "Content-Type: application/json\r\n"
            },
            body.len()
        )?;
        stream.write_all(body)?;
        stream.flush()?;
        stream.shutdown(Shutdown::Write)?;
        let mut response = Vec::new();
        stream.take(16 * 1024).read_to_end(&mut response)?;
        let boundary = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or_else(|| std::io::Error::other("HTTP response headers are incomplete"))?;
        let headers = std::str::from_utf8(&response[..boundary])
            .map_err(|_| std::io::Error::other("HTTP response headers are invalid"))?;
        let status = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| std::io::Error::other("HTTP response status is invalid"))?;
        let body = serde_json::from_slice(&response[boundary + 4..])?;
        Ok((status, body))
    }

    fn finish(mut self) {
        let mut child = self.child.take().expect("HTTP faucet child");
        child.kill().expect("stop HTTP faucet");
        let _ = child.wait().expect("wait for HTTP faucet");
    }
}

impl Drop for HttpFaucetHarness {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn prepare_wallet(wallet: &mut ProcessHarness, name: &str) -> String {
    let created = wallet.request(
        "profile-create",
        "wallet.profile.create",
        json!({"displayName":name}),
    );
    assert_eq!(created["ok"], true, "{created}");
    let profile = created["result"]["profile"]["id"]
        .as_str()
        .expect("profile id")
        .to_owned();
    for (id, method, params) in [
        (
            "profile-select",
            "wallet.profile.select",
            json!({"profileId":profile}),
        ),
        (
            "network-select",
            "wallet.network.select",
            json!({"networkId":NETWORK}),
        ),
        (
            "security-initialize",
            "wallet.security.initialize",
            json!({}),
        ),
    ] {
        let response = wallet.request(id, method, params);
        assert_eq!(response["ok"], true, "{response}");
    }
    let derived = wallet.request("account-derive", "wallet.account.derive", json!({}));
    assert_eq!(derived["ok"], true, "{derived}");
    derived["result"]["account"]["receiveAddress"]["value"]
        .as_str()
        .expect("unshielded receive address")
        .to_owned()
}

fn await_night_amount(wallet: &mut ProcessHarness, expected: u128) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let response = wallet.request("account-sync", "wallet.connect", json!({}));
        if response["ok"] == true {
            let observed = response["result"]["account"]["balances"]
                .as_array()
                .and_then(|balances| balances.iter().find(|balance| balance["symbol"] == "NIGHT"))
                .and_then(|balance| balance["atomicUnits"].as_str())
                .and_then(|amount| amount.parse::<u128>().ok())
                .unwrap_or_default();
            if observed == expected {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "authoritative NIGHT balance was not observed"
        );
        thread::sleep(Duration::from_secs(2));
    }
}

fn await_night(wallet: &mut ProcessHarness) {
    await_night_amount(wallet, FIXED_GRANT);
}

fn import_recipient(wallet: &mut ProcessHarness, receive_request: &str, format: &str) -> String {
    let response = wallet.request(
        "receive-request-import",
        "wallet.receive_request.import",
        json!({"receiveRequest": receive_request}),
    );
    assert_eq!(response["ok"], true, "receive request must validate");
    assert_eq!(response["result"]["recipient"]["format"], format);
    assert_eq!(response["result"]["recipient"]["networkId"], NETWORK);
    assert_eq!(response["result"]["recipient"]["asset"], "NIGHT");
    response["result"]["recipient"]["address"]
        .as_str()
        .expect("validated recipient address")
        .to_owned()
}

fn transfer_and_await_inclusion(
    wallet: &mut ProcessHarness,
    recipient_address: &str,
    amount: u128,
) -> String {
    let prepared = wallet.request(
        "transfer-prepare",
        "wallet.transaction.prepare_unshielded",
        json!({"recipientAddress": recipient_address, "amountAtomicUnits": amount.to_string()}),
    );
    assert_eq!(prepared["ok"], true, "transfer preparation must succeed");
    let transfer = &prepared["result"]["transfer"];
    let draft_id = transfer["draftId"]
        .as_str()
        .expect("transfer draft id")
        .to_owned();
    let challenge = transfer["authorizationChallenge"]
        .as_str()
        .expect("transfer authorization challenge")
        .to_owned();
    let authorized = wallet.request(
        "transfer-authorize",
        "wallet.transaction.authorize_unshielded",
        json!({"draftId": draft_id, "authorizationChallenge": challenge,
            "confirmation":{"title":"Authorize NIGHT transfer","summary":"Authorize the wallet-derived NIGHT transfer","confirmed":true}}),
    );
    assert_eq!(
        authorized["ok"], true,
        "transfer authorization must succeed"
    );
    let submitted = wallet.request(
        "transfer-submit",
        "wallet.transaction.submit_unshielded",
        json!({"draftId": draft_id,
            "confirmation":{"title":"Submit NIGHT transfer","summary":"Submit the authorized NIGHT transfer","confirmed":true}}),
    );
    let submitted_transaction_id = if submitted["ok"] == true {
        Some(
            submitted["result"]["submission"]["transactionId"]
                .as_str()
                .expect("public transaction identifier")
                .to_owned(),
        )
    } else {
        assert_eq!(
            submitted["error"]["code"], "submission_unknown",
            "only an explicitly reconcilable finality timeout may continue: {submitted}"
        );
        None
    };
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let status = wallet.request(
            "transfer-reconcile",
            "wallet.transaction.reconcile_submission",
            json!({"draftId": draft_id}),
        );
        if status["ok"] == true {
            let status = &status["result"]["submissionStatus"];
            if status["state"] == "included" {
                let reconciled_transaction_id = status["transactionId"]
                    .as_str()
                    .expect("included transaction identifier")
                    .to_owned();
                if let Some(submitted_transaction_id) = submitted_transaction_id.as_deref() {
                    assert_eq!(reconciled_transaction_id, submitted_transaction_id);
                }
                return reconciled_transaction_id;
            }
            assert!(
                matches!(
                    status["state"].as_str(),
                    Some("running" | "broadcasting" | "outcome_unknown")
                ),
                "transfer entered an unexpected terminal state: {status}"
            );
        } else {
            assert_eq!(
                status["error"]["code"], "submission_unknown",
                "reconciliation failed irrecoverably: {status}"
            );
        }
        assert!(
            Instant::now() < deadline,
            "transfer finality was not observed: {status}"
        );
        thread::sleep(Duration::from_secs(2));
    }
}

fn night_changes(transaction: &Value) -> Option<(u128, u128)> {
    let mut credits = 0_u128;
    let mut debits = 0_u128;
    for change in transaction["changes"].as_array()? {
        if change["balance"]["symbol"] != "NIGHT" {
            continue;
        }
        let amount = change["balance"]["atomicUnits"]
            .as_str()?
            .parse::<u128>()
            .ok()?;
        match change["direction"].as_str()? {
            "credit" => credits = credits.checked_add(amount)?,
            "debit" => debits = debits.checked_add(amount)?,
            _ => return None,
        }
    }
    Some((credits, debits))
}

fn assert_history(wallet: &mut ProcessHarness, direction: &str, amount: u128) {
    let response = wallet.request(
        "transaction-history",
        "wallet.transaction.history",
        json!({}),
    );
    assert_eq!(
        response["ok"], true,
        "authoritative transaction history must be readable"
    );
    let transactions = response["result"]["transactions"]
        .as_array()
        .expect("transaction history array");
    let observations = transactions
        .iter()
        .map(|transaction| {
            let (credits, debits) = night_changes(transaction).unwrap_or_default();
            json!({
                "direction": transaction["direction"],
                "status": transaction["status"],
                "nightCredits": credits.to_string(),
                "nightDebits": debits.to_string()
            })
        })
        .collect::<Vec<_>>();
    assert!(
        transactions.iter().any(|transaction| {
            let Some((credits, debits)) = night_changes(transaction) else {
                return false;
            };
            let exact_delta = match direction {
                "incoming" => credits.checked_sub(debits),
                "outgoing" => debits.checked_sub(credits),
                _ => None,
            };
            transaction["transactionId"]
                .as_str()
                .is_some_and(|identifier| {
                    identifier.len() == 64
                        && identifier.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                && transaction["direction"] == direction
                && transaction["status"] == "confirmed"
                && exact_delta == Some(amount)
        }),
        "finalized transaction direction must reconcile: {observations:?}"
    );
}

fn assert_submission_history(wallet: &mut ProcessHarness, transaction_id: &str) {
    let response = wallet.request(
        "submission-history",
        "wallet.transaction.submission_history",
        json!({}),
    );
    assert_eq!(response["ok"], true, "submission journal must be readable");
    assert!(
        response["result"]["submissions"]
            .as_array()
            .is_some_and(|submissions| submissions.iter().any(|submission| {
                submission["transactionId"] == transaction_id
                    && submission["state"] == "included"
                    && submission["reconciliationAllowed"] == false
            })),
        "included submission must be recorded in the durable journal"
    );
}

fn register_and_await_dust(wallet: &mut ProcessHarness) -> Duration {
    let prepared = wallet.request(
        "dust-registration-prepare",
        "wallet.dust.registration.prepare",
        json!({}),
    );
    assert_eq!(prepared["ok"], true, "{prepared}");
    let registration = &prepared["result"]["registration"];
    assert_eq!(
        registration["registeredNight"]["atomicUnits"],
        FIXED_GRANT.to_string()
    );
    let draft = registration["draftId"].as_str().expect("draft id");
    let challenge = registration["authorizationChallenge"]
        .as_str()
        .expect("authorization challenge");
    let authorized = wallet.request(
        "dust-registration-authorize",
        "wallet.dust.registration.authorize",
        json!({
            "draftId":draft,
            "authorizationChallenge":challenge,
            "confirmation":{
                "title":"Authorize DUST registration",
                "summary":"Register this wallet's eligible NIGHT with its protected DUST key",
                "confirmed":true
            }
        }),
    );
    assert_eq!(authorized["ok"], true, "{authorized}");
    let submitted = wallet.request(
        "dust-registration-submit",
        "wallet.dust.registration.submit",
        json!({
            "draftId":draft,
            "confirmation":{
                "title":"Submit DUST registration",
                "summary":"Prove and submit the authorized DUST registration",
                "confirmed":true
            }
        }),
    );
    assert_eq!(submitted["ok"], true, "{submitted}");

    let started = Instant::now();
    loop {
        let response = wallet.request("dust-sync-start", "wallet.dust.sync.start", json!({}));
        assert_eq!(response["ok"], true, "{response}");
        loop {
            let response = wallet.request("dust-sync-status", "wallet.dust.sync.status", json!({}));
            assert_eq!(response["ok"], true, "{response}");
            let sync = &response["result"]["dustSync"];
            let amount = sync["balance"]["atomicUnits"]
                .as_str()
                .and_then(|amount| amount.parse::<u128>().ok())
                .unwrap_or_default();
            if matches!(sync["state"].as_str(), Some("synced" | "cached")) {
                if amount > 0 {
                    return started.elapsed();
                }
                break;
            }
            assert_ne!(sync["state"], "stalled", "{response}");
            assert!(
                started.elapsed() < DUST_DEADLINE,
                "DUST was not ready within ten minutes"
            );
            thread::sleep(Duration::from_secs(2));
        }
        assert!(
            started.elapsed() < DUST_DEADLINE,
            "DUST was not ready within ten minutes"
        );
        thread::sleep(Duration::from_secs(2));
    }
}

#[test]
#[ignore = "requires an explicitly authorized local standalone stack"]
fn two_fresh_wallets_receive_fixed_night_and_generate_dust() {
    assert_eq!(std::env::var(ENABLE_ENV).as_deref(), Ok("1"));
    let root = StateRoot::new();
    let mut faucet = ProcessHarness::faucet(&root.child("faucet"));
    let mut wallet_a = ProcessHarness::wallet(&root.child("wallet-a"));
    let mut wallet_b = ProcessHarness::wallet(&root.child("wallet-b"));
    let address_a = prepare_wallet(&mut wallet_a, "Standalone wallet A");
    let address_b = prepare_wallet(&mut wallet_b, "Standalone wallet B");
    assert_ne!(
        address_a, address_b,
        "fresh wallets must derive distinct addresses"
    );
    println!("standalone-faucet-headless-e2e: prepared wallets=2");

    for (index, (request, address)) in [("wallet-a", address_a), ("wallet-b", address_b)]
        .into_iter()
        .enumerate()
    {
        let funded = faucet.request(
            request,
            "faucet.fund",
            json!({"requestId":request,"recipientAddress":address}),
        );
        assert_eq!(funded["ok"], true, "{funded}");
        assert_eq!(
            funded["result"]["receipt"]["amount"]["atomicUnits"],
            FIXED_GRANT.to_string()
        );
        assert_eq!(funded["result"]["receipt"]["state"], "included");
        println!(
            "standalone-faucet-headless-e2e: funded wallet={}",
            index + 1
        );
    }

    await_night(&mut wallet_a);
    println!("standalone-faucet-headless-e2e: observed wallet=1");
    await_night(&mut wallet_b);
    println!("standalone-faucet-headless-e2e: observed wallet=2");
    let dust_a = register_and_await_dust(&mut wallet_a);
    println!(
        "standalone-faucet-headless-e2e: dust-ready wallet=1 seconds={}",
        dust_a.as_secs()
    );
    let dust_b = register_and_await_dust(&mut wallet_b);
    assert!(dust_a <= DUST_DEADLINE && dust_b <= DUST_DEADLINE);
    println!(
        "standalone-faucet-headless-e2e: PASS wallets=2 grantAtomicUnits={FIXED_GRANT} dustASeconds={} dustBSeconds={}",
        dust_a.as_secs(),
        dust_b.as_secs()
    );

    wallet_a.finish("system.quit");
    wallet_b.finish("system.quit");
    faucet.finish("faucet.shutdown");
}

#[test]
#[ignore = "requires an explicitly authorized local standalone stack"]
fn two_fresh_wallets_complete_a_night_round_trip_and_reconcile_history() {
    assert_eq!(std::env::var(ENABLE_ENV).as_deref(), Ok("1"));
    let root = StateRoot::new();
    let mut faucet = ProcessHarness::faucet(&root.child("faucet"));
    let wallet_a_root = root.child("wallet-a");
    let wallet_b_root = root.child("wallet-b");
    let mut wallet_a = ProcessHarness::wallet(&wallet_a_root);
    let mut wallet_b = ProcessHarness::wallet(&wallet_b_root);
    let address_a = prepare_wallet(&mut wallet_a, "Standalone round-trip wallet A");
    let address_b = prepare_wallet(&mut wallet_b, "Standalone round-trip wallet B");
    assert_ne!(address_a, address_b, "wallet roots must remain independent");

    for (request_id, recipient_address) in [
        ("round-trip-wallet-a", &address_a),
        ("round-trip-wallet-b", &address_b),
    ] {
        let funded = faucet.request(
            request_id,
            "faucet.fund",
            json!({"requestId": request_id, "recipientAddress": recipient_address}),
        );
        assert_eq!(
            funded["ok"], true,
            "fixed funding must succeed once: {funded}"
        );
        assert_eq!(
            funded["result"]["receipt"]["amount"]["atomicUnits"],
            FIXED_GRANT.to_string()
        );
        assert_eq!(funded["result"]["receipt"]["state"], "included");
    }
    await_night(&mut wallet_a);
    await_night(&mut wallet_b);

    let dust_a = register_and_await_dust(&mut wallet_a);
    let dust_b = register_and_await_dust(&mut wallet_b);
    assert!(dust_a <= DUST_DEADLINE && dust_b <= DUST_DEADLINE);
    // Registration spends each original NIGHT output and returns the same
    // principal to a new same-owner output. Refresh the public account UTXO
    // snapshot before either transfer is planned; a balance-only equality is
    // not proof that the previously selected input remains unspent.
    await_night(&mut wallet_a);
    await_night(&mut wallet_b);

    let versioned_b =
        format!("midnight-receive:v1|network={NETWORK}|asset=NIGHT|address={address_b}");
    let imported_b = import_recipient(&mut wallet_a, &versioned_b, "versioned");
    let a_to_b = transfer_and_await_inclusion(&mut wallet_a, &imported_b, TRANSFER_A_TO_B);
    await_night_amount(&mut wallet_a, FIXED_GRANT - TRANSFER_A_TO_B);
    await_night_amount(&mut wallet_b, FIXED_GRANT + TRANSFER_A_TO_B);
    assert_history(&mut wallet_a, "outgoing", TRANSFER_A_TO_B);
    assert_history(&mut wallet_b, "incoming", TRANSFER_A_TO_B);

    let imported_a = import_recipient(&mut wallet_b, &address_a, "raw");
    let b_to_a = transfer_and_await_inclusion(&mut wallet_b, &imported_a, TRANSFER_B_TO_A);
    await_night_amount(
        &mut wallet_a,
        FIXED_GRANT - TRANSFER_A_TO_B + TRANSFER_B_TO_A,
    );
    await_night_amount(
        &mut wallet_b,
        FIXED_GRANT + TRANSFER_A_TO_B - TRANSFER_B_TO_A,
    );
    assert_history(&mut wallet_a, "incoming", TRANSFER_B_TO_A);
    assert_history(&mut wallet_b, "outgoing", TRANSFER_B_TO_A);
    assert_submission_history(&mut wallet_a, &a_to_b);
    assert_submission_history(&mut wallet_b, &b_to_a);
    wallet_a.finish("system.quit");
    wallet_b.finish("system.quit");
    faucet.finish("faucet.shutdown");
    root.cleanup();
    println!(
        "standalone-night-round-trip-e2e: PASS wallets=2 grantAtomicUnits={FIXED_GRANT} transfers=2 dustASeconds={} dustBSeconds={} cleanup=complete",
        dust_a.as_secs(),
        dust_b.as_secs()
    );
}

#[test]
#[ignore = "requires an explicitly authorized local standalone stack"]
fn two_fresh_wallets_receive_fixed_night_over_loopback_http() {
    assert_eq!(std::env::var(ENABLE_ENV).as_deref(), Ok("1"));
    let root = StateRoot::new();
    let faucet = HttpFaucetHarness::start(&root.child("http-faucet"));
    let mut wallet_a = ProcessHarness::wallet(&root.child("wallet-a"));
    let mut wallet_b = ProcessHarness::wallet(&root.child("wallet-b"));
    let address_a = prepare_wallet(&mut wallet_a, "HTTP wallet A");
    let address_b = prepare_wallet(&mut wallet_b, "HTTP wallet B");
    assert_ne!(
        address_a, address_b,
        "fresh wallets must derive distinct addresses"
    );

    for (request, address) in [("http-wallet-a", address_a), ("http-wallet-b", address_b)] {
        let funded = faucet.fund(request, &address);
        assert_eq!(funded["ok"], true, "{funded}");
        assert_eq!(
            funded["result"]["receipt"]["amount"]["atomicUnits"],
            FIXED_GRANT.to_string()
        );
        assert_eq!(funded["result"]["receipt"]["state"], "included");
    }

    await_night(&mut wallet_a);
    await_night(&mut wallet_b);
    println!("standalone-faucet-http-headless-e2e: PASS wallets=2 grantAtomicUnits={FIXED_GRANT}");

    wallet_a.finish("system.quit");
    wallet_b.finish("system.quit");
    faucet.finish();
}
