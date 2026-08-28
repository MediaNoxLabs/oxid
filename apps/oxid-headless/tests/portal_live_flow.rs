// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
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
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};

const ISSUER_METHOD: &str = "did:midnight:undeployed:a4c9483a0c7cdd808056a93334ab97207b38b4363d1da5cbfb78ad256cd689f0#issuer-key-1";
const ISSUER_X: &str = "r3S3KuAV2Y2wviagxqTsKNuUFmqHlVjfWwQvZaV_pQA";
const ISSUER_Y: &str = "b8GewrvMw5hldx4dBHZSAqBhYb_p7bVdcVqC2FU08mM";
const PRE_AUTHORIZED_CODE: &str = "OXID_PHASE1_PRE_AUTHORIZED_CODE";
const ACCESS_TOKEN: &str = "OXID_PHASE1_ACCESS_TOKEN";
const NONCE: &str = "OXID_PHASE1_NONCE";
const INDEXER_WS: &str = "ws://127.0.0.1:8088/api/v4/graphql/ws";
const INDEXER_HTTP: &str = "http://127.0.0.1:8088/api/v4/graphql";
const NODE_WS: &str = "ws://127.0.0.1:9944";
const PROOF_SERVER: &str = "http://127.0.0.1:6300";
const STANDALONE_ADDRESS: &str =
    "mn_addr_undeployed1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9smkp9zh";
const MAX_HEADERS_BYTES: usize = 64 * 1024;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_HEIGHT_DELTA: u64 = 4;
const EXCLUDED_ENVIRONMENT: [&str; 10] = [
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

struct RuntimeCleanup(PathBuf);

impl Drop for RuntimeCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ProcessHarness {
    child: Option<Child>,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    error: BufReader<ChildStderr>,
}

impl ProcessHarness {
    fn spawn(root: &Path, manifest: &Path, manifest_digest: &str) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxid-headless"));
        command
            .env("OXID_PROFILE_STORE_PATH", root.join("profiles.json"))
            .env("OXID_DID_STORE_PATH", root.join("private/did-records.json"))
            .env(
                "OXID_CREDENTIAL_STORE_PATH",
                root.join("private/credentials.enc"),
            )
            .env(
                "OXID_CREDENTIAL_KEY_PATH",
                root.join("private/credentials.key"),
            )
            .env("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH", manifest)
            .env(
                "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256",
                manifest_digest,
            )
            .env("OXID_MIDNIGHT_NETWORK_ID", "undeployed")
            .env("OXID_MIDNIGHT_INDEXER_WS_URL", INDEXER_WS)
            .env("OXID_MIDNIGHT_INDEXER_HTTP_URL", INDEXER_HTTP)
            .env("OXID_MIDNIGHT_NODE_WS_URL", NODE_WS)
            .env("OXID_MIDNIGHT_PROOF_SERVER_URL", PROOF_SERVER)
            .env("OXID_MIDNIGHT_UNSHIELDED_ADDRESS", STANDALONE_ADDRESS)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for key in EXCLUDED_ENVIRONMENT {
            command.env_remove(key);
        }
        let mut child = command.spawn().expect("headless process should start");
        Self {
            input: child.stdin.take().expect("headless stdin"),
            output: BufReader::new(child.stdout.take().expect("headless stdout")),
            error: BufReader::new(child.stderr.take().expect("headless stderr")),
            child: Some(child),
        }
    }

    fn request(&mut self, id: &str, method: &str, params: Value) -> Value {
        serde_json::to_writer(
            &mut self.input,
            &json!({"protocol":"oxid.headless.v1","id":id,"method":method,"params":params}),
        )
        .expect("request JSON");
        self.input.write_all(b"\n").expect("request newline");
        self.input.flush().expect("request flush");
        let mut line = String::new();
        self.output.read_line(&mut line).expect("response read");
        assert!(
            !line.is_empty(),
            "headless process exited before responding"
        );
        serde_json::from_str(&line).expect("response JSON")
    }

    fn quit(mut self) -> String {
        assert_eq!(self.request("quit", "system.quit", json!({}))["ok"], true);
        let mut child = self.child.take().expect("running child");
        assert!(child.wait().expect("headless wait").success());
        let mut stderr = String::new();
        self.error.read_to_string(&mut stderr).expect("stderr read");
        stderr
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MockCounters {
    issuer_metadata: u8,
    authorization_metadata: u8,
    token: u8,
    nonce: u8,
    credential: u8,
    issuer_resolution: u8,
}

impl MockCounters {
    fn increment(value: &mut u8) {
        *value = value.checked_add(1).expect("mock call counter is bounded");
        assert!(*value <= 8, "mock call counter exceeded its bound");
    }
}

struct MockIssuer {
    origin: String,
    counters: Arc<Mutex<MockCounters>>,
    holder_sender: mpsc::SyncSender<BoundCredentialRequest>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockIssuer {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock issuer listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let origin = format!(
            "http://{}",
            listener.local_addr().expect("listener address")
        );
        let counters = Arc::new(Mutex::new(MockCounters::default()));
        let thread_counters = Arc::clone(&counters);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread_origin = origin.clone();
        let (holder_sender, holder_receiver) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("mock issuer accept failed: {error}"),
                };
                if thread_stop.load(Ordering::Relaxed) {
                    break;
                }
                stream
                    .set_nonblocking(false)
                    .expect("blocking mock issuer stream");
                set_stream_timeouts(&stream);
                let request = read_http_request(&mut stream);
                let response =
                    mock_response(request, &thread_origin, &thread_counters, &holder_receiver);
                write_json_response(&mut stream, &response);
            }
        });
        Self {
            origin,
            counters,
            holder_sender,
            stop,
            thread: Some(thread),
        }
    }

    fn offer(&self) -> String {
        let offer = json!({
            "credential_issuer":self.origin,
            "credential_configuration_ids":["digital_passport_v1"],
            "grants":{
                "urn:ietf:params:oauth:grant-type:pre-authorized_code":{
                    "pre-authorized_code":PRE_AUTHORIZED_CODE
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
        let jwk_digest = hex::encode(Sha256::digest(serde_json::to_vec(&jwk).expect("JWK")));
        let manifest = json!({
            "integrationCommit":"22ae5369b6f939e6b20648f4b85dd993527748ef",
            "integrationTree":"74d8d1a5b87c160ea554006e47d5f3edc3cd3e10",
            "issuerDid":STANDALONE_COMPACT_PASSPORT_ISSUER_DID,
            "issuerJubjubJwk":jwk,
            "issuerJubjubJwkSha256":jwk_digest,
            "issuerMethod":ISSUER_METHOD,
            "issuerOrigin":self.origin,
            "issuerResolverOrigin":self.origin,
            "provenanceSha256":"cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87",
            "schema":"oxid-portal-deployment-v3"
        });
        let bytes = serde_json::to_vec(&manifest).expect("manifest JSON");
        fs::write(path, &bytes).expect("manifest file");
        hex::encode(Sha256::digest(bytes))
    }

    fn provide_holder(&self, holder: BoundCredentialRequest) {
        self.holder_sender
            .send(holder)
            .expect("one bounded holder response");
    }

    fn counters(&self) -> MockCounters {
        *self.counters.lock().expect("mock counters")
    }

    fn stop(mut self) -> MockCounters {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.origin.trim_start_matches("http://"));
        if let Some(thread) = self.thread.take() {
            thread.join().expect("mock issuer thread");
        }
        self.counters()
    }
}

impl Drop for MockIssuer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.origin.trim_start_matches("http://"));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct HttpRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn set_stream_timeouts(stream: &TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .expect("write timeout");
}

fn read_http_request(stream: &mut TcpStream) -> HttpRequest {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer).expect("request header read");
        assert_ne!(read, 0, "complete request headers");
        bytes.extend_from_slice(&buffer[..read]);
        assert!(
            bytes.len() <= MAX_HEADERS_BYTES,
            "request headers too large"
        );
        if let Some(position) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers_text = std::str::from_utf8(&bytes[..header_end]).expect("request headers UTF-8");
    let mut lines = headers_text.split("\r\n");
    let mut request_line = lines.next().expect("request line").split_whitespace();
    let method = request_line.next().expect("request method").to_owned();
    let path = request_line.next().expect("request path").to_owned();
    assert_eq!(request_line.next(), Some("HTTP/1.1"));
    assert!(request_line.next().is_none());
    let mut headers = BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').expect("bounded HTTP header");
        let name = name.to_ascii_lowercase();
        assert!(headers.insert(name, value.trim().to_owned()).is_none());
        assert!(headers.len() <= 32, "too many request headers");
    }
    assert!(!headers.contains_key("transfer-encoding"));
    let content_length = headers
        .get("content-length")
        .map_or(0, |value| value.parse::<usize>().expect("Content-Length"));
    assert!(
        content_length <= MAX_REQUEST_BYTES,
        "request body too large"
    );
    let target = header_end
        .checked_add(content_length)
        .expect("bounded request length");
    assert!(bytes.len() <= target, "request exceeds Content-Length");
    while bytes.len() < target {
        let remaining = target - bytes.len();
        let chunk_length = remaining.min(buffer.len());
        let read = stream
            .read(&mut buffer[..chunk_length])
            .expect("request body read");
        assert_ne!(read, 0, "complete request body");
        bytes.extend_from_slice(&buffer[..read]);
    }
    HttpRequest {
        method,
        path,
        headers,
        body: bytes[header_end..target].to_vec(),
    }
}

fn write_json_response(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("mock issuer response");
}

fn exact_keys(object: &Map<String, Value>, expected: &[&str]) {
    let actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

fn increment_counter(
    counters: &Arc<Mutex<MockCounters>>,
    select: impl FnOnce(&mut MockCounters) -> &mut u8,
) {
    let mut counters = counters.lock().expect("mock counters");
    MockCounters::increment(select(&mut counters));
}

fn require_method_and_empty_body(request: &HttpRequest, method: &str) {
    assert_eq!(request.method, method);
    assert!(request.body.is_empty());
}

fn require_json_content_type(request: &HttpRequest) {
    assert_eq!(
        request.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
}

fn mock_response(
    request: HttpRequest,
    origin: &str,
    counters: &Arc<Mutex<MockCounters>>,
    holder_receiver: &mpsc::Receiver<BoundCredentialRequest>,
) -> String {
    match request.path.as_str() {
        "/.well-known/openid-credential-issuer" => {
            require_method_and_empty_body(&request, "GET");
            increment_counter(counters, |value| &mut value.issuer_metadata);
            json!({
                "authorization_servers":[origin],
                "credential_configurations_supported":{
                    "digital_passport_v1":{
                        "credential_metadata":{"display":[{"locale":"en","name":"Digital Passport"}]},
                        "cryptographic_binding_methods_supported":["did"],
                        "format":"midnight_cbor_phase1",
                        "proof_types_supported":{"jwt":{"proof_signing_alg_values_supported":["EdDSA","ES256"]}},
                        "scope":"digital-passport"
                    }
                },
                "credential_endpoint":format!("{origin}/api/issuer/credentials"),
                "credential_issuer":origin,
                "nonce_endpoint":format!("{origin}/api/issuer/nonce")
            })
            .to_string()
        }
        "/.well-known/oauth-authorization-server" => {
            require_method_and_empty_body(&request, "GET");
            increment_counter(counters, |value| &mut value.authorization_metadata);
            json!({
                "grant_types_supported":["urn:ietf:params:oauth:grant-type:pre-authorized_code"],
                "issuer":origin,
                "pre-authorized_grant_anonymous_access_supported":true,
                "token_endpoint":format!("{origin}/api/issuer/token")
            })
            .to_string()
        }
        "/api/issuer/token" => {
            assert_eq!(request.method, "POST");
            assert_eq!(
                request.headers.get("content-type").map(String::as_str),
                Some("application/x-www-form-urlencoded")
            );
            let fields = url::form_urlencoded::parse(&request.body)
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect::<Vec<_>>();
            assert_eq!(fields.len(), 2);
            let fields = fields.into_iter().collect::<BTreeMap<_, _>>();
            assert_eq!(fields.len(), 2);
            assert_eq!(
                fields.get("grant_type").map(String::as_str),
                Some("urn:ietf:params:oauth:grant-type:pre-authorized_code")
            );
            assert_eq!(
                fields.get("pre-authorized_code").map(String::as_str),
                Some(PRE_AUTHORIZED_CODE)
            );
            increment_counter(counters, |value| &mut value.token);
            json!({"access_token":ACCESS_TOKEN,"expires_in":300,"token_type":"Bearer"}).to_string()
        }
        "/api/issuer/nonce" => {
            require_method_and_empty_body(&request, "POST");
            increment_counter(counters, |value| &mut value.nonce);
            json!({"c_nonce":NONCE,"c_nonce_expires_in":300}).to_string()
        }
        "/api/issuer/credentials" => {
            assert_eq!(request.method, "POST");
            require_json_content_type(&request);
            assert_eq!(
                request.headers.get("authorization").map(String::as_str),
                Some("Bearer OXID_PHASE1_ACCESS_TOKEN")
            );
            let body: Value =
                serde_json::from_slice(&request.body).expect("credential request JSON");
            let root = body.as_object().expect("credential request object");
            exact_keys(root, &["credential_configuration_id", "midnight", "proofs"]);
            assert_eq!(body["credential_configuration_id"], "digital_passport_v1");
            let midnight = body["midnight"]
                .as_object()
                .expect("Midnight request object");
            exact_keys(midnight, &["holderBindingMethod"]);
            let proofs = body["proofs"].as_object().expect("proofs object");
            exact_keys(proofs, &["jwt"]);
            let proof = body["proofs"]["jwt"]
                .as_array()
                .filter(|proofs| proofs.len() == 1)
                .and_then(|proofs| proofs[0].as_str())
                .expect("one proof JWT");
            assert_eq!(proof.split('.').count(), 3);
            let holder = holder_receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("one transient holder response");
            assert_eq!(
                body["midnight"]["holderBindingMethod"],
                holder.holder_binding_method_id
            );
            let bundle = block_on(
                StandaloneBoundCompactCredentialIssuer::new(Arc::new(SystemClock))
                    .issue(holder.clone()),
            )
            .expect("valid signed credential bundle");
            increment_counter(counters, |value| &mut value.credential);
            json!({
                "credentials":[{
                    "credential":general_purpose::URL_SAFE_NO_PAD.encode(bundle.signed_bytes),
                    "midnight":{
                        "credentialFamily":"digital-passport",
                        "credentialPrivateParts":portal_private_parts(),
                        "credentialProof":{
                            "encoding":"compact-value-v1.base64url",
                            "payload":general_purpose::URL_SAFE_NO_PAD.encode(bundle.detached_proof.expect("detached proof"))
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
            assert_eq!(request.method, "POST");
            require_json_content_type(&request);
            let body: Value = serde_json::from_slice(&request.body).expect("resolver request JSON");
            let root = body.as_object().expect("resolver request object");
            exact_keys(root, &["did"]);
            assert_eq!(body["did"], STANDALONE_COMPACT_PASSPORT_ISSUER_DID);
            let did =
                MidnightDid::parse(STANDALONE_COMPACT_PASSPORT_ISSUER_DID).expect("issuer DID");
            let resolution =
                block_on(StandaloneDidResolver.resolve(&did)).expect("issuer resolution");
            increment_counter(counters, |value| &mut value.issuer_resolution);
            resolution_to_json_value(&resolution).to_string()
        }
        _ => panic!("unexpected mock issuer route"),
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

fn read_http_response(stream: &mut TcpStream, maximum_body_bytes: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer).expect("response header read");
        assert_ne!(read, 0, "complete response headers");
        bytes.extend_from_slice(&buffer[..read]);
        assert!(
            bytes.len() <= MAX_HEADERS_BYTES,
            "response headers too large"
        );
        if let Some(position) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).expect("response headers UTF-8");
    assert!(headers.starts_with("HTTP/1.1 200"));
    assert!(!headers.to_ascii_lowercase().contains("transfer-encoding:"));
    let lengths = headers
        .lines()
        .filter_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|value| value.parse::<usize>().ok())
        })
        .collect::<Vec<_>>();
    assert_eq!(lengths.len(), 1);
    let length = lengths[0];
    assert!(length <= maximum_body_bytes, "response body too large");
    let target = header_end
        .checked_add(length)
        .expect("bounded response length");
    assert!(bytes.len() <= target, "response exceeds Content-Length");
    while bytes.len() < target {
        let remaining = target - bytes.len();
        let chunk_length = remaining.min(buffer.len());
        let read = stream
            .read(&mut buffer[..chunk_length])
            .expect("response body read");
        assert_ne!(read, 0, "complete response body");
        bytes.extend_from_slice(&buffer[..read]);
    }
    bytes[header_end..target].to_vec()
}

fn independent_indexer_height() -> u64 {
    let mut stream = TcpStream::connect(("127.0.0.1", 8088)).expect("local indexer v4");
    set_stream_timeouts(&stream);
    let body = br#"{"query":"query OxidPhase1Evidence { block { height } }"}"#;
    write!(
        stream,
        "POST /api/v4/graphql HTTP/1.1\r\nHost: 127.0.0.1:8088\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .expect("indexer request headers");
    stream.write_all(body).expect("indexer request body");
    let response = read_http_response(&mut stream, 64 * 1024);
    let value: Value = serde_json::from_slice(&response).expect("indexer response JSON");
    assert!(
        value
            .get("errors")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    );
    value["data"]["block"]["height"]
        .as_u64()
        .expect("numeric indexer block height")
}

fn authentication_and_binding(document: &Value) -> (String, String, BoundCredentialRequest) {
    let holder_did = document["id"].as_str().expect("holder DID").to_owned();
    let authentication = document["relationships"]
        .as_array()
        .expect("DID relationships")
        .iter()
        .find(|value| value["relationship"] == "authentication")
        .and_then(|value| value["methodIds"][0].as_str())
        .expect("managed authentication method")
        .to_owned();
    let binding = document["verificationMethods"]
        .as_array()
        .expect("verification methods")
        .iter()
        .find(|value| value["publicKeyJwk"]["crv"] == "Jubjub")
        .expect("managed Jubjub method");
    let binding_method = binding["id"].as_str().expect("binding method").to_owned();
    assert_ne!(authentication, binding_method);
    let holder = BoundCredentialRequest {
        holder_did,
        holder_binding_method_id: binding_method.clone(),
        public_key_x: binding["publicKeyJwk"]["x"]
            .as_str()
            .expect("binding x")
            .to_owned(),
        public_key_y: binding["publicKeyJwk"]["y"]
            .as_str()
            .expect("binding y")
            .to_owned(),
    };
    (authentication, binding_method, holder)
}

fn assert_valid_verification(value: &Value) {
    assert_eq!(value["verification"]["outcome"], "valid");
    let stages = value["verification"]["stages"]
        .as_array()
        .expect("verification stages");
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
}

fn write_journey(path: &Path, headless_height: u64, independent_height: u64) {
    let journey = json!({
        "acceptance":{
            "encryptedPersistence":true,
            "explicitConsent":true,
            "issuerCallsBlockedBeforeConsent":true,
            "listing":true,
            "managedAuthentication":true,
            "pendingIssuancePreservedAcrossSync":true,
            "restartRestoration":true,
            "reverification":true,
            "sameProcessIssuanceAndSync":true,
            "separateJubjubBinding":true,
            "verifiedImport":true
        },
        "headlessIndexerHeight":headless_height,
        "independentIndexerHeight":independent_height,
        "schema":"oxid-phase1-local-headless-journey-v1"
    });
    let bytes = serde_json::to_vec(&journey).expect("journey JSON");
    fs::create_dir_all(path.parent().expect("journey parent")).expect("journey parent");
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).expect("temporary journey");
    fs::rename(temporary, path).expect("atomic journey publication");
}

#[test]
#[ignore = "requires the existing local oxid-standalone node, indexer, and proof-server containers"]
fn local_mock_issuer_and_same_headless_process_use_standalone_indexer() {
    let candidate_head = std::env::var("OXID_PHASE1_CANDIDATE_HEAD").expect("candidate head");
    assert!(
        candidate_head.len() == 40
            && candidate_head
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );
    let journey_path = PathBuf::from(
        std::env::var_os("OXID_PHASE1_JOURNEY_PATH")
            .expect("journey path")
            .into_string()
            .expect("journey path UTF-8"),
    );
    assert!(journey_path.is_absolute());
    let runtime_root = journey_path
        .parent()
        .expect("journey parent")
        .join("runtime");
    let _ = fs::remove_dir_all(&runtime_root);
    fs::create_dir_all(&runtime_root).expect("runtime root");
    let _runtime_cleanup = RuntimeCleanup(runtime_root.clone());
    let _ = fs::remove_file(&journey_path);

    let mock = MockIssuer::spawn();
    let manifest = runtime_root.join("deployment.json");
    let manifest_digest = mock.write_manifest(&manifest);
    let offer = mock.offer();
    let wallet_root = runtime_root.join("wallet");
    fs::create_dir_all(&wallet_root).expect("wallet root");
    let mut first = ProcessHarness::spawn(&wallet_root, &manifest, &manifest_digest);

    let created = first.request(
        "profile-create",
        "wallet.profile.create",
        json!({"displayName":"Phase 1 local headless"}),
    );
    let profile_id = created["result"]["profile"]["id"]
        .as_str()
        .expect("profile id")
        .to_owned();
    assert_eq!(
        first.request(
            "profile-select",
            "wallet.profile.select",
            json!({"profileId":profile_id})
        )["ok"],
        true
    );
    assert_eq!(
        first.request("security", "wallet.security.initialize", json!({}))["ok"],
        true
    );
    let derived = first.request("account-derive", "wallet.account.derive", json!({}));
    assert_eq!(derived["result"]["account"]["networkId"], "undeployed");
    let did = first.request("did-create", "did.create", json!({}));
    let document = &did["result"]["didRecord"]["document"];
    let (authentication, binding, holder) = authentication_and_binding(document);

    let prepared = first.request(
        "prepare",
        "credential.issuance.prepare",
        json!({"offer":offer}),
    );
    assert_eq!(prepared["result"]["issuance"]["state"], "awaiting_consent");
    let issuance_id = prepared["result"]["issuance"]["id"]
        .as_str()
        .expect("issuance id")
        .to_owned();
    let blocked = first.request(
        "blocked-accept",
        "credential.issuance.accept",
        json!({
            "issuanceId":issuance_id,
            "holderDid":holder.holder_did,
            "methodId":authentication,
            "holderBindingMethodId":binding,
            "confirmed":false,
            "intent":"ACCEPT_CREDENTIAL_ISSUANCE"
        }),
    );
    assert_eq!(blocked["error"]["code"], "confirmation_required");
    let before_consent = mock.counters();
    assert_eq!(before_consent.token, 0);
    assert_eq!(before_consent.nonce, 0);
    assert_eq!(before_consent.credential, 0);

    let connected = first.request("connect", "wallet.connect", json!({}));
    assert_eq!(connected["ok"], true, "unexpected sync response");
    let account = &connected["result"]["account"];
    assert_eq!(account["source"], "live");
    assert_eq!(account["networkId"], "undeployed");
    assert_eq!(account["sync"]["state"], "synced");
    let headless_height = account["sync"]["chainTipHeight"]
        .as_u64()
        .expect("headless reports a numeric indexer height");
    let independent_height = independent_indexer_height();
    assert!(
        headless_height.abs_diff(independent_height) <= MAX_HEIGHT_DELTA,
        "headless and independent indexer heights exceed the advancing-tip bound"
    );
    let still_pending = first.request(
        "pending-after-sync",
        "credential.issuance.get",
        json!({"issuanceId":issuance_id}),
    );
    assert_eq!(
        still_pending["result"]["issuance"]["state"],
        "awaiting_consent"
    );

    mock.provide_holder(holder.clone());
    let accepted = first.request(
        "accept",
        "credential.issuance.accept",
        json!({
            "issuanceId":issuance_id,
            "holderDid":holder.holder_did,
            "methodId":authentication,
            "holderBindingMethodId":binding,
            "confirmed":true,
            "intent":"ACCEPT_CREDENTIAL_ISSUANCE"
        }),
    );
    assert_eq!(accepted["result"]["issuance"]["state"], "succeeded");
    let after_accept = mock.counters();
    assert_eq!(after_accept.token, 1);
    assert_eq!(after_accept.nonce, 1);
    assert_eq!(after_accept.credential, 1);
    let credential_id = accepted["result"]["issuance"]["credentialId"]
        .as_str()
        .expect("credential id")
        .to_owned();
    let reverified = first.request(
        "reverify",
        "credential.reverify",
        json!({"credentialId":credential_id}),
    );
    assert_valid_verification(&reverified["result"]["credential"]);
    let first_stderr = first.quit();
    assert!(first_stderr.is_empty(), "unexpected first-process stderr");

    let encrypted = fs::read(wallet_root.join("private/credentials.enc")).expect("encrypted store");
    for plaintext in [
        b"Alice".as_slice(),
        b"Example".as_slice(),
        b"AB1234567".as_slice(),
        PRE_AUTHORIZED_CODE.as_bytes(),
        ACCESS_TOKEN.as_bytes(),
        NONCE.as_bytes(),
    ] {
        assert!(
            !encrypted
                .windows(plaintext.len())
                .any(|window| window == plaintext)
        );
    }

    let mut second = ProcessHarness::spawn(&wallet_root, &manifest, &manifest_digest);
    let listed = second.request("restored-list", "credential.list", json!({}));
    assert_eq!(
        listed["result"]["credentials"].as_array().map(Vec::len),
        Some(1)
    );
    let restored_id = listed["result"]["credentials"][0]["id"]
        .as_str()
        .expect("restored credential id");
    let restored = second.request(
        "restored-reverify",
        "credential.reverify",
        json!({"credentialId":restored_id}),
    );
    assert_valid_verification(&restored["result"]["credential"]);
    let second_stderr = second.quit();
    assert!(second_stderr.is_empty(), "unexpected second-process stderr");

    let final_counters = mock.stop();
    assert_eq!(final_counters.token, 1);
    assert_eq!(final_counters.nonce, 1);
    assert_eq!(final_counters.credential, 1);
    assert!(final_counters.issuer_metadata >= 1 && final_counters.issuer_metadata <= 2);
    assert!(
        final_counters.authorization_metadata >= 1 && final_counters.authorization_metadata <= 2
    );
    assert!(final_counters.issuer_resolution >= 3 && final_counters.issuer_resolution <= 4);

    write_journey(&journey_path, headless_height, independent_height);
}
