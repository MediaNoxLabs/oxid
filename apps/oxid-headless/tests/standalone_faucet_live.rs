// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    fs,
    io::{BufRead as _, BufReader, Read as _, Write as _},
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
            .env("OXID_PROFILE_STORE_PATH", root.join("profiles.json"))
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

fn await_night(wallet: &mut ProcessHarness) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let response = wallet.request("account-sync", "wallet.connect", json!({}));
        if response["ok"] != true {
            assert_eq!(
                response["error"]["code"], "capability_unavailable",
                "{response}"
            );
            assert!(
                Instant::now() < deadline,
                "funded NIGHT could not be observed before the indexer deadline: {response}"
            );
            thread::sleep(Duration::from_secs(2));
            continue;
        }
        let observed = response["result"]["account"]["balances"]
            .as_array()
            .expect("balances")
            .iter()
            .find(|balance| balance["symbol"] == "NIGHT")
            .and_then(|balance| balance["atomicUnits"].as_str())
            .and_then(|amount| amount.parse::<u128>().ok())
            .unwrap_or_default();
        if observed == FIXED_GRANT {
            return;
        }
        assert!(
            observed < FIXED_GRANT,
            "fresh wallet received more than the fixed grant"
        );
        assert!(Instant::now() < deadline, "funded NIGHT was not observed");
        thread::sleep(Duration::from_secs(2));
    }
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
