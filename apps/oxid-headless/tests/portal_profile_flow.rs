// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    fs,
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose};
use futures::executor::block_on;
use oxid_adapter_did_midnight::{
    STANDALONE_COMPACT_PASSPORT_ISSUER_DID, StandaloneDidResolver, resolution_to_json_value,
};
use oxid_adapter_platform_system::SystemClock;
use oxid_adapter_vc_midnight::StandaloneBoundCompactCredentialIssuer;
use oxid_credential_application::{BoundCredentialIssuerPort as _, BoundCredentialRequest};
use oxid_identity_application::DidResolutionPort as _;
use oxid_identity_domain::MidnightDid;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const ISSUER_METHOD: &str = "did:midnight:undeployed:a4c9483a0c7cdd808056a93334ab97207b38b4363d1da5cbfb78ad256cd689f0#issuer-key-1";
const ISSUER_X: &str = "r3S3KuAV2Y2wviagxqTsKNuUFmqHlVjfWwQvZaV_pQA";
const ISSUER_Y: &str = "b8GewrvMw5hldx4dBHZSAqBhYb_p7bVdcVqC2FU08mM";
const SECRET_CODE: &str = "PORTAL_TEST_PRE_AUTHORIZED_CODE";
const ACCESS_TOKEN: &str = "PORTAL_TEST_ACCESS_TOKEN";
const NONCE: &str = "PORTAL_TEST_NONCE";
const INDEXER_WS: &str = "ws://127.0.0.1:8088/api/v4/graphql/ws";
const INDEXER_HTTP: &str = "http://127.0.0.1:8088/api/v4/graphql";
const NODE_WS: &str = "ws://127.0.0.1:9944";
const PROOF_SERVER: &str = "http://127.0.0.1:6300";
const STANDALONE_ADDRESS: &str =
    "mn_addr_undeployed1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9smkp9zh";
const PORTAL_STANDALONE_EXCLUDED_ENV: [&str; 10] = [
    "OXID_MIDNIGHT_PROVING_CACHE_DIR",
    "OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH",
    "OXID_MIDNIGHT_DUST_CHECKPOINT_PATH",
    "OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH",
    "OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH",
    "OXID_MIDNIGHT_DID_RESOLVER_URL",
    "OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT",
    "OXID_PASSPORT_VAULT_COMPOSER",
    "OXID_PASSPORT_VAULT_STORE_PATH",
    "OXID_PRESENTATION_ARTIFACTS_DIR",
];

fn configure_canonical_standalone(command: &mut Command) {
    for key in PORTAL_STANDALONE_EXCLUDED_ENV {
        command.env_remove(key);
    }
    command
        .env("OXID_MIDNIGHT_NETWORK_ID", "undeployed")
        .env("OXID_MIDNIGHT_INDEXER_WS_URL", INDEXER_WS)
        .env("OXID_MIDNIGHT_INDEXER_HTTP_URL", INDEXER_HTTP)
        .env("OXID_MIDNIGHT_NODE_WS_URL", NODE_WS)
        .env("OXID_MIDNIGHT_PROOF_SERVER_URL", PROOF_SERVER)
        .env("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", STANDALONE_ADDRESS);
}

struct TestStore {
    root: PathBuf,
}

impl TestStore {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "oxid-portal-profile-flow-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("test root");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .expect("private test root");
        }
        Self { root }
    }

    fn profiles(&self) -> PathBuf {
        self.root.join("profiles.json")
    }

    fn manifest(&self) -> PathBuf {
        self.root.join("portal-deployment.json")
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct ProcessHarness {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    error: BufReader<ChildStderr>,
}

impl ProcessHarness {
    fn spawn(store: &TestStore, manifest_digest: &str) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
        command
            .env("OXID_PROFILE_STORE_PATH", store.profiles())
            .env(
                "OXID_DID_STORE_PATH",
                store.root.join("private/did-records.json"),
            )
            .env(
                "OXID_CREDENTIAL_STORE_PATH",
                store.root.join("private/credentials.enc"),
            )
            .env(
                "OXID_CREDENTIAL_KEY_PATH",
                store.root.join("private/credentials.key"),
            )
            .env(
                "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH",
                store.manifest(),
            )
            .env(
                "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256",
                manifest_digest,
            )
            .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
            .env_remove("OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_DUST_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH")
            .env_remove("OXID_MIDNIGHT_DID_RESOLVER_URL")
            .env_remove("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT")
            .env_remove("OXID_PASSPORT_VAULT_COMPOSER")
            .env_remove("OXID_PASSPORT_VAULT_STORE_PATH")
            .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_canonical_standalone(&mut command);
        let mut child = command.spawn().expect("headless wallet should start");
        Self {
            input: child.stdin.take().expect("stdin"),
            output: BufReader::new(child.stdout.take().expect("stdout")),
            error: BufReader::new(child.stderr.take().expect("stderr")),
            child,
        }
    }

    fn request(&mut self, request: Value) -> Value {
        serde_json::to_writer(&mut self.input, &request).expect("request JSON");
        self.input.write_all(b"\n").expect("request newline");
        self.input.flush().expect("request flush");
        let mut line = String::new();
        self.output.read_line(&mut line).expect("response read");
        if line.is_empty() {
            let mut stderr = String::new();
            self.error
                .read_to_string(&mut stderr)
                .expect("failed stderr");
            panic!("headless wallet exited before response: {stderr}");
        }
        serde_json::from_str(&line).expect("response JSON")
    }

    fn quit(mut self) -> String {
        let quit = self.request(json!({
            "protocol":"oxid.headless.v1","id":"quit","method":"system.quit","params":{}
        }));
        assert_eq!(quit["ok"], true);
        assert!(self.child.wait().expect("wait").success());
        let mut stderr = String::new();
        self.error.read_to_string(&mut stderr).expect("stderr");
        stderr
    }
}

#[derive(Default)]
struct ServerState {
    holder: Option<BoundCredentialRequest>,
    journal: Vec<(String, String)>,
}

struct PortalServer {
    origin: String,
    state: Arc<Mutex<ServerState>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl PortalServer {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Portal fixture listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let origin = format!("http://{}", listener.local_addr().expect("address"));
        let state = Arc::new(Mutex::new(ServerState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let thread_origin = origin.clone();
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("Portal fixture accept failed: {error}"),
                };
                if thread_stop.load(Ordering::Relaxed) {
                    break;
                }
                stream
                    .set_nonblocking(false)
                    .expect("Portal fixture stream must be blocking");
                let (path, body) = read_http_request(&mut stream);
                thread_state
                    .lock()
                    .expect("state")
                    .journal
                    .push((path.clone(), body.clone()));
                let response = response_for(&path, &body, &thread_origin, &thread_state);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                )
                .expect("response");
            }
        });
        Self {
            origin,
            state,
            stop,
            thread: Some(handle),
        }
    }

    fn set_holder(&self, holder: BoundCredentialRequest) {
        self.state.lock().expect("state").holder = Some(holder);
    }

    fn offer(&self) -> String {
        let offer = json!({
            "credential_issuer": self.origin,
            "credential_configuration_ids": ["digital_passport_v1"],
            "grants": {
                "urn:ietf:params:oauth:grant-type:pre-authorized_code": {
                    "pre-authorized_code": SECRET_CODE
                }
            }
        });
        let mut url = url::Url::parse("openid-credential-offer://").expect("offer URL");
        url.query_pairs_mut()
            .append_pair("credential_offer", &offer.to_string());
        url.into()
    }

    fn write_manifest(&self, path: &Path) -> String {
        let jwk = json!({"crv":"Jubjub","kty":"EC","x":ISSUER_X,"y":ISSUER_Y});
        let jwk_digest = hex::encode(Sha256::digest(serde_json::to_vec(&jwk).expect("jwk")));
        let manifest = json!({
            "integrationCommit":"25499870f84d77173c46e4af3021311decfb840b",
            "integrationTree":"2d845d2293603dfd8adce5362c8a9941e6ba78a9",
            "issuerDid": STANDALONE_COMPACT_PASSPORT_ISSUER_DID,
            "issuerJubjubJwk": jwk,
            "issuerJubjubJwkSha256": jwk_digest,
            "issuerMethod": ISSUER_METHOD,
            "issuerOrigin": self.origin,
            "issuerResolverOrigin": self.origin,
            "provenanceSha256": "63d2dd182f1a315d8fe7677ae6481aecebd2fd9cff709cc438b6c0261a3cf4c7",
            "schema": "oxid-portal-deployment-v3"
        });
        let bytes = serde_json::to_vec(&manifest).expect("manifest");
        fs::write(path, &bytes).expect("manifest file");
        hex::encode(Sha256::digest(bytes))
    }
}

impl Drop for PortalServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake the nonblocking loop quickly without depending on a sleep timeout.
        let _ = std::net::TcpStream::connect(self.origin.trim_start_matches("http://"));
        if let Some(handle) = self.thread.take() {
            let joined = handle.join();
            if !thread::panicking() {
                joined.expect("Portal fixture thread");
            }
        }
    }
}

fn read_http_request(stream: &mut std::net::TcpStream) -> (String, String) {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer).expect("request read");
        assert_ne!(read, 0, "complete request headers");
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(position) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end])
        .expect("headers")
        .to_owned();
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|value| value.parse::<usize>().ok())
        })
        .unwrap_or(0);
    while bytes.len() - header_end < content_length {
        let read = stream.read(&mut buffer).expect("request body read");
        assert_ne!(read, 0, "complete request body");
        bytes.extend_from_slice(&buffer[..read]);
    }
    let first = headers.lines().next().expect("request line");
    let path = first.split_whitespace().nth(1).expect("path").to_owned();
    let body = String::from_utf8(bytes[header_end..header_end + content_length].to_vec())
        .expect("body UTF-8");
    (path, body)
}

fn response_for(path: &str, body: &str, origin: &str, state: &Arc<Mutex<ServerState>>) -> String {
    match path {
        "/.well-known/openid-credential-issuer" => json!({
            "authorization_servers": [origin],
            "credential_configurations_supported": {
                "digital_passport_v1": {
                    "credential_metadata": {"display":[{"locale":"en","name":"Digital Passport"}]},
                    "cryptographic_binding_methods_supported":["did"],
                    "format":"midnight_cbor_phase1",
                    "proof_types_supported":{"jwt":{"proof_signing_alg_values_supported":["EdDSA","ES256"]}},
                    "scope":"digital-passport"
                }
            },
            "credential_endpoint": format!("{origin}/api/issuer/credentials"),
            "credential_issuer": origin,
            "nonce_endpoint": format!("{origin}/api/issuer/nonce")
        })
        .to_string(),
        "/.well-known/oauth-authorization-server" => json!({
            "grant_types_supported":["urn:ietf:params:oauth:grant-type:pre-authorized_code"],
            "issuer":origin,
            "pre-authorized_grant_anonymous_access_supported":true,
            "token_endpoint":format!("{origin}/api/issuer/token")
        })
        .to_string(),
        "/api/issuer/token" => {
            assert!(body.contains("pre-authorized_code=PORTAL_TEST_PRE_AUTHORIZED_CODE"));
            json!({"access_token":ACCESS_TOKEN,"expires_in":300,"token_type":"Bearer"}).to_string()
        }
        "/api/issuer/nonce" => {
            assert!(body.is_empty(), "Portal nonce request body must be empty");
            json!({"c_nonce":NONCE,"c_nonce_expires_in":300}).to_string()
        }
        "/api/issuer/credentials" => {
            let request: Value = serde_json::from_str(body).expect("credential request");
            assert_eq!(request["credential_configuration_id"], "digital_passport_v1");
            assert_eq!(request["proofs"]["jwt"].as_array().map(Vec::len), Some(1));
            let holder_method = request["midnight"]["holderBindingMethod"]
                .as_str()
                .expect("holder binding method");
            let holder = state
                .lock()
                .expect("state")
                .holder
                .clone()
                .expect("holder public facts configured");
            assert_eq!(holder.holder_binding_method_id, holder_method);
            let bundle = block_on(
                StandaloneBoundCompactCredentialIssuer::new(Arc::new(SystemClock)).issue(
                    holder.clone(),
                ),
            )
            .expect("fresh bound credential");
            let private = portal_private_parts();
            json!({
                "credentials":[{
                    "credential": general_purpose::URL_SAFE_NO_PAD.encode(bundle.signed_bytes),
                    "midnight":{
                        "credentialFamily":"digital-passport",
                        "credentialPrivateParts":private,
                        "credentialProof":{
                            "encoding":"compact-value-v1.base64url",
                            "payload":general_purpose::URL_SAFE_NO_PAD.encode(bundle.detached_proof.expect("proof"))
                        },
                        "encoding":"compact-value-v1.base64url",
                        "expiresAt":"2099-01-01T00:00:00Z",
                        "hasExpiration":true,
                        "holderBinding":{
                            "challenge":NONCE,
                            "holderDidMethod":{
                                "did":holder.holder_did,
                                "keyType":"jubjub",
                                "methodId":holder.holder_binding_method_id
                            },
                            "method":"explicit_did_method"
                        },
                        "schemaId":"digital-passport:v1",
                        "schemaVersion":"1.0"
                    }
                }]
            })
            .to_string()
        }
        "/resolve" => {
            let requested: Value = serde_json::from_str(body).expect("resolver request");
            assert_eq!(requested["did"], STANDALONE_COMPACT_PASSPORT_ISSUER_DID);
            let did = MidnightDid::parse(STANDALONE_COMPACT_PASSPORT_ISSUER_DID).expect("DID");
            let resolution = block_on(StandaloneDidResolver.resolve(&did)).expect("resolution");
            resolution_to_json_value(&resolution).to_string()
        }
        _ => panic!("unexpected Portal fixture path: {path}"),
    }
}

fn portal_private_parts() -> Value {
    fn padded<const N: usize>(value: &[u8]) -> [u8; N] {
        let mut output = [0_u8; N];
        output[..value.len()].copy_from_slice(value);
        output
    }
    let opening = |label: &[u8]| general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(label));
    json!({
        "claimValues":{
            "firstNameValuePadded":general_purpose::URL_SAFE_NO_PAD.encode(padded::<64>(b"Alice")),
            "lastNameValuePadded":general_purpose::URL_SAFE_NO_PAD.encode(padded::<64>(b"Example")),
            "dateOfBirthDays":3650,
            "documentNumberValue":general_purpose::URL_SAFE_NO_PAD.encode(padded::<32>(b"AB1234567")),
            "issuingStateValue":general_purpose::URL_SAFE_NO_PAD.encode(padded::<32>(b"US"))
        },
        "openings":{
            "firstNameOpening":opening(b"opening:first-name"),
            "lastNameOpening":opening(b"opening:last-name"),
            "dateOfBirthOpening":opening(b"opening:date-of-birth"),
            "documentNumberOpening":opening(b"opening:document-number"),
            "issuingStateOpening":opening(b"opening:issuing-state")
        }
    })
}

fn request(process: &mut ProcessHarness, id: &str, method: &str, params: Value) -> Value {
    process.request(json!({
        "protocol":"oxid.headless.v1","id":id,"method":method,"params":params
    }))
}

#[test]
fn portal_standalone_profile_issues_encrypts_restores_and_reverifies_in_a_new_process() {
    let store = TestStore::new();
    let server = PortalServer::spawn();
    let manifest_digest = server.write_manifest(&store.manifest());
    let offer = server.offer();
    let mut first = ProcessHarness::spawn(&store, &manifest_digest);

    let created = request(
        &mut first,
        "profile-create",
        "wallet.profile.create",
        json!({"displayName":"Portal interoperability"}),
    );
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("profile id")
        .to_owned();
    assert_eq!(
        request(
            &mut first,
            "profile-select",
            "wallet.profile.select",
            json!({"profileId":profile_id})
        )["ok"],
        true
    );
    assert_eq!(
        request(
            &mut first,
            "security",
            "wallet.security.initialize",
            json!({})
        )["ok"],
        true
    );
    let did_response = request(&mut first, "did", "did.create", json!({}));
    let document = &did_response["result"]["didRecord"]["document"];
    let holder_did = document["id"].as_str().expect("holder DID").to_owned();
    let authentication_method = document["relationships"]
        .as_array()
        .expect("relationships")
        .iter()
        .find(|value| value["relationship"] == "authentication")
        .and_then(|value| value["methodIds"][0].as_str())
        .expect("authentication method")
        .to_owned();
    let binding = document["verificationMethods"]
        .as_array()
        .expect("verification methods")
        .iter()
        .find(|value| value["publicKeyJwk"]["crv"] == "Jubjub")
        .expect("Jubjub method");
    let binding_method = binding["id"].as_str().expect("binding method").to_owned();
    assert_ne!(authentication_method, binding_method);
    server.set_holder(BoundCredentialRequest {
        holder_did: holder_did.clone(),
        holder_binding_method_id: binding_method.clone(),
        public_key_x: binding["publicKeyJwk"]["x"]
            .as_str()
            .expect("binding x")
            .to_owned(),
        public_key_y: binding["publicKeyJwk"]["y"]
            .as_str()
            .expect("binding y")
            .to_owned(),
    });

    let routed = request(
        &mut first,
        "route",
        "identity.request.route",
        json!({"requestUri":offer}),
    );
    assert_eq!(routed["result"]["route"]["kind"], "credential_issuance");
    assert!(!routed.to_string().contains(SECRET_CODE));
    let prepared = request(
        &mut first,
        "prepare",
        "credential.issuance.prepare",
        json!({"offer":offer}),
    );
    assert_eq!(prepared["result"]["issuance"]["state"], "awaiting_consent");
    assert!(!prepared.to_string().contains(SECRET_CODE));
    let issuance_id = prepared["result"]["issuance"]["id"]
        .as_str()
        .expect("issuance id")
        .to_owned();
    let denied = request(
        &mut first,
        "deny",
        "credential.issuance.accept",
        json!({
            "issuanceId":issuance_id,
            "holderDid":holder_did,
            "methodId":authentication_method,
            "holderBindingMethodId":binding_method,
            "confirmed":false,
            "intent":"ACCEPT_CREDENTIAL_ISSUANCE"
        }),
    );
    assert_eq!(denied["error"]["code"], "confirmation_required");
    assert_eq!(
        server
            .state
            .lock()
            .expect("state")
            .journal
            .iter()
            .filter(|(path, _)| matches!(
                path.as_str(),
                "/api/issuer/token" | "/api/issuer/nonce" | "/api/issuer/credentials"
            ))
            .count(),
        0
    );

    let accepted = request(
        &mut first,
        "accept",
        "credential.issuance.accept",
        json!({
            "issuanceId":issuance_id,
            "holderDid":holder_did,
            "methodId":authentication_method,
            "holderBindingMethodId":binding_method,
            "confirmed":true,
            "intent":"ACCEPT_CREDENTIAL_ISSUANCE"
        }),
    );
    assert_eq!(accepted["result"]["issuance"]["state"], "succeeded");
    let credential_id = accepted["result"]["issuance"]["credentialId"]
        .as_str()
        .expect("credential id")
        .to_owned();
    let listed = request(&mut first, "list", "credential.list", json!({}));
    assert_eq!(
        listed["result"]["credentials"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        listed["result"]["credentials"][0]["verification"]["outcome"],
        "valid"
    );
    let reverified = request(
        &mut first,
        "reverify",
        "credential.reverify",
        json!({"credentialId":credential_id}),
    );
    assert_eq!(
        reverified["result"]["credential"]["verification"]["outcome"],
        "valid"
    );
    let serialized = format!("{accepted}{listed}{reverified}");
    for secret in [
        SECRET_CODE,
        ACCESS_TOKEN,
        NONCE,
        "signedBytes",
        "detachedProof",
        "privateMaterial",
    ] {
        assert!(!serialized.contains(secret));
    }
    let first_stderr = first.quit();
    assert!(first_stderr.is_empty(), "unexpected first-process stderr");

    assert!(
        store.profiles().is_file(),
        "configured profile store must be used"
    );
    assert!(
        store.root.join("private/did-records.json").is_file(),
        "configured DID store must be used"
    );
    assert!(
        store.root.join("private/credentials.key").is_file(),
        "configured credential wrapping-key path must be used"
    );
    let encrypted = fs::read(store.root.join("private/credentials.enc")).expect("encrypted store");
    for plaintext in [
        b"Alice".as_slice(),
        SECRET_CODE.as_bytes(),
        ACCESS_TOKEN.as_bytes(),
        NONCE.as_bytes(),
    ] {
        assert!(
            !encrypted
                .windows(plaintext.len())
                .any(|window| window == plaintext)
        );
    }

    let mut second = ProcessHarness::spawn(&store, &manifest_digest);
    let restored = request(&mut second, "restored-list", "credential.list", json!({}));
    assert_eq!(
        restored["result"]["credentials"].as_array().map(Vec::len),
        Some(1)
    );
    let restored_id = restored["result"]["credentials"][0]["id"]
        .as_str()
        .expect("restored credential id");
    let restored_verification = request(
        &mut second,
        "restored-reverify",
        "credential.reverify",
        json!({"credentialId":restored_id}),
    );
    assert_eq!(
        restored_verification["result"]["credential"]["verification"]["outcome"],
        "valid"
    );
    let stages = restored_verification["result"]["credential"]["verification"]["stages"]
        .as_array()
        .expect("stages");
    for name in [
        "structural",
        "issuer",
        "proof",
        "temporal",
        "schema",
        "trust",
    ] {
        assert!(
            stages
                .iter()
                .any(|stage| stage["name"] == name && stage["status"] == "passed")
        );
    }
    assert!(
        stages
            .iter()
            .any(|stage| stage["name"] == "status" && stage["status"] == "not_checked")
    );
    let second_stderr = second.quit();
    assert!(second_stderr.is_empty(), "unexpected second-process stderr");
}

#[test]
fn portal_startup_accepts_only_the_exact_local_standalone_bundle() {
    let store = TestStore::new();
    let server = PortalServer::spawn();
    let digest = server.write_manifest(&store.manifest());
    let cases = [
        ("OXID_MIDNIGHT_NETWORK_ID", "devnet"),
        (
            "OXID_MIDNIGHT_INDEXER_WS_URL",
            "ws://localhost:8088/api/v4/graphql/ws",
        ),
        (
            "OXID_MIDNIGHT_INDEXER_HTTP_URL",
            "http://127.0.0.1:8089/api/v4/graphql",
        ),
        ("OXID_MIDNIGHT_NODE_WS_URL", "ws://127.0.0.1:9945"),
        ("OXID_MIDNIGHT_PROOF_SERVER_URL", "http://127.0.0.1:6301"),
        (
            "OXID_MIDNIGHT_UNSHIELDED_ADDRESS",
            "mn_addr_undeployed1noncanonical",
        ),
    ];
    let mut accepted = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
    accepted
        .env("OXID_PROFILE_STORE_PATH", store.profiles())
        .env(
            "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH",
            store.manifest(),
        )
        .env("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256", &digest)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_canonical_standalone(&mut accepted);
    let output = accepted
        .output()
        .expect("canonical headless startup result");
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());

    for (key, value) in cases {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
        command
            .env("OXID_PROFILE_STORE_PATH", store.profiles())
            .env(
                "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH",
                store.manifest(),
            )
            .env("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256", &digest)
            .env_remove("OXID_MIDNIGHT_PROVING_CACHE_DIR")
            .env_remove("OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_DUST_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH")
            .env_remove("OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH")
            .env_remove("OXID_MIDNIGHT_DID_RESOLVER_URL")
            .env_remove("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT")
            .env_remove("OXID_PASSPORT_VAULT_COMPOSER")
            .env_remove("OXID_PASSPORT_VAULT_STORE_PATH")
            .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_canonical_standalone(&mut command);
        command.env(key, value);
        let output = command.output().expect("headless startup result");
        assert!(!output.status.success(), "noncanonical {key} must fail");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("stderr UTF-8");
        assert!(!stderr.contains(value), "startup must not echo {key}");
    }

    for removed in [
        "OXID_MIDNIGHT_NETWORK_ID",
        "OXID_MIDNIGHT_INDEXER_WS_URL",
        "OXID_MIDNIGHT_INDEXER_HTTP_URL",
        "OXID_MIDNIGHT_NODE_WS_URL",
        "OXID_MIDNIGHT_PROOF_SERVER_URL",
        "OXID_MIDNIGHT_UNSHIELDED_ADDRESS",
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
        command
            .env("OXID_PROFILE_STORE_PATH", store.profiles())
            .env(
                "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH",
                store.manifest(),
            )
            .env("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256", &digest)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_canonical_standalone(&mut command);
        command.env_remove(removed);
        let output = command.output().expect("headless startup result");
        assert!(!output.status.success(), "partial bundle missing {removed}");
        assert!(output.stdout.is_empty());
    }

    for extra in [
        "OXID_MIDNIGHT_PROVING_CACHE_DIR",
        "OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH",
        "OXID_MIDNIGHT_DUST_CHECKPOINT_PATH",
        "OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH",
        "OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH",
        "OXID_MIDNIGHT_DID_RESOLVER_URL",
        "OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT",
        "OXID_PASSPORT_VAULT_COMPOSER",
        "OXID_PASSPORT_VAULT_STORE_PATH",
        "OXID_PRESENTATION_ARTIFACTS_DIR",
    ] {
        let marker = if extra == "OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT" {
            "42"
        } else {
            "/tmp/oxid-do-not-echo-local-setting"
        };
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
        command
            .env("OXID_PROFILE_STORE_PATH", store.profiles())
            .env(
                "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH",
                store.manifest(),
            )
            .env("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256", &digest)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_canonical_standalone(&mut command);
        command.env(extra, marker);
        let output = command.output().expect("headless startup result");
        assert!(!output.status.success(), "extra setting {extra} must fail");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("stderr UTF-8");
        assert!(!stderr.contains(marker), "startup must not echo {extra}");
    }
}

#[test]
fn portal_startup_configuration_fails_closed_when_partial_or_relative() {
    let store = TestStore::new();
    let cases = [
        (Some(store.manifest().to_string_lossy().into_owned()), None),
        (
            Some("relative/deployment.json".to_owned()),
            Some("0".repeat(64)),
        ),
        (None, Some("0".repeat(64))),
    ];
    for (path, digest) in cases {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
        command
            .env("OXID_PROFILE_STORE_PATH", store.profiles())
            .env_remove("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH")
            .env_remove("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = path {
            command.env("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH", path);
        }
        if let Some(digest) = digest {
            command.env("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256", digest);
        }
        let output = command.output().expect("headless startup result");
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("stderr UTF-8");
        assert!(
            stderr.contains("Portal") || stderr.contains("portal"),
            "bounded Portal startup error: {stderr}"
        );
        assert!(!stderr.contains("relative/deployment.json"));
    }
}
