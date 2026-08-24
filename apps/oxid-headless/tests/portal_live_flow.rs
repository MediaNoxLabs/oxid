// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    fs,
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose};
use futures::executor::block_on;
use oxid_adapter_did_midnight::{HttpDidResolver, HttpDidResolverConfig};
use oxid_adapter_platform_system::SystemClock;
use oxid_adapter_vc_midnight::{
    DigitalPassportIssuerTrustAnchor, MidnightCredentialVerifier, convert_portal_private_parts,
};
use oxid_credential_application::CredentialVerificationPort as _;
use oxid_identity_application::DidResolutionPort as _;
use oxid_identity_domain::MidnightDid;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

const PORTAL_HELPER_COMMIT: &str = "00d3d6c6b9ebe37e1a4bffc4dd7a3f27cf6e4b24";
const PORTAL_HELPER_TREE: &str = "3cecc6e17d56b2c0d646150df3861005df831ed8";
const PORTAL_INTEGRATION_COMMIT: &str = "925ec8d04882eabd4ac7b784c70fc2f0c152faae";
const PORTAL_INTEGRATION_TREE: &str = "58b4597524f88a0ae2253439a44dab0dc60cbb6f";
const PORTAL_PR_HEAD: &str = "9c82db23eabe8b6d758b2731f2225910ea627c14";
const PORTAL_PROFILE_SOURCE: &str = "76e8edf394a4cb37ca822037272d543c68f25f71";
const PORTAL_PROVENANCE_SHA256: &str =
    "cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87";
const PORTAL_ISSUER_RESOLVER: &str = "http://127.0.0.1:9092";

struct RuntimeCleanup(PathBuf);

impl Drop for RuntimeCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ProcessHarness {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl ProcessHarness {
    fn spawn(root: &Path, manifest: &Path, digest: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_oxid-headless"))
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
            .env("OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256", digest)
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
            .env_remove("OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT")
            .env_remove("OXID_PASSPORT_VAULT_COMPOSER")
            .env_remove("OXID_PASSPORT_VAULT_STORE_PATH")
            .env_remove("OXID_PRESENTATION_ARTIFACTS_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("headless process");
        Self {
            input: child.stdin.take().expect("stdin"),
            output: BufReader::new(child.stdout.take().expect("stdout")),
            child,
        }
    }

    fn request(&mut self, id: &str, method: &str, params: Value) -> Value {
        serde_json::to_writer(
            &mut self.input,
            &json!({"protocol":"oxid.headless.v1","id":id,"method":method,"params":params}),
        )
        .expect("request JSON");
        self.input.write_all(b"\n").expect("newline");
        self.input.flush().expect("flush");
        let mut line = String::new();
        self.output.read_line(&mut line).expect("response");
        assert!(!line.is_empty(), "headless process exited");
        serde_json::from_str(&line).expect("response JSON")
    }

    fn quit(mut self) {
        assert_eq!(self.request("quit", "system.quit", json!({}))["ok"], true);
        assert!(self.child.wait().expect("wait").success());
    }
}

struct PortalProxy {
    origin: String,
    captured_credential_response: Arc<Mutex<Option<Value>>>,
    secret_request_count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    completion: Option<mpsc::Receiver<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl PortalProxy {
    fn spawn() -> Self {
        Self::spawn_bound(
            SocketAddr::from(([127, 0, 0, 1], 8090)),
            SocketAddr::from(([127, 0, 0, 1], 18090)),
        )
    }

    fn spawn_with_upstream(upstream_address: SocketAddr) -> Self {
        Self::spawn_bound(upstream_address, SocketAddr::from(([127, 0, 0, 1], 0)))
    }

    fn spawn_bound(upstream_address: SocketAddr, listen_address: SocketAddr) -> Self {
        let listener = TcpListener::bind(listen_address).expect("Portal observation proxy");
        listener.set_nonblocking(true).expect("nonblocking proxy");
        let port = listener.local_addr().expect("proxy address").port();
        let captured_credential_response: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let thread_capture = Arc::clone(&captured_credential_response);
        let secret_request_count = Arc::new(AtomicUsize::new(0));
        let thread_secret_request_count = Arc::clone(&secret_request_count);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let (completion_sender, completion_receiver) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                let (mut incoming, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("Portal proxy accept: {error}"),
                };
                if thread_stop.load(Ordering::Relaxed) {
                    break;
                }
                incoming
                    .set_nonblocking(false)
                    .expect("blocking proxy stream");
                set_stream_timeouts(&incoming);
                let (path, _, raw_request) = read_raw_request(&mut incoming);
                if matches!(
                    path.as_str(),
                    "/api/issuer/token" | "/api/issuer/nonce" | "/api/issuer/credentials"
                ) {
                    thread_secret_request_count.fetch_add(1, Ordering::Relaxed);
                }
                let mut upstream = TcpStream::connect(upstream_address).expect("Portal upstream");
                set_stream_timeouts(&upstream);
                upstream
                    .write_all(&raw_request)
                    .expect("Portal proxy request");
                let raw_response = read_http_response(&mut upstream, 4 * 1024 * 1024);
                if path == "/api/issuer/credentials"
                    && let Some(split) = raw_response
                        .windows(4)
                        .position(|value| value == b"\r\n\r\n")
                        .map(|index| index + 4)
                    && std::str::from_utf8(&raw_response[..split])
                        .is_ok_and(|headers| headers.starts_with("HTTP/1.1 200"))
                    && let Ok(value) = serde_json::from_slice(&raw_response[split..])
                {
                    *thread_capture.lock().expect("capture") = Some(value);
                }
                incoming
                    .write_all(&raw_response)
                    .expect("Portal proxy downstream");
            }
            let _ = completion_sender.send(());
        });
        Self {
            origin: format!("http://127.0.0.1:{port}"),
            secret_request_count,
            captured_credential_response,
            stop,
            completion: Some(completion_receiver),
            thread: Some(handle),
        }
    }
}

impl PortalProxy {
    fn secret_request_count(&self) -> usize {
        self.secret_request_count.load(Ordering::Relaxed)
    }
}

impl Drop for PortalProxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let port = self
            .origin
            .rsplit(':')
            .next()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(0);
        let _ = TcpStream::connect(("127.0.0.1", port));
        let completed = self
            .completion
            .take()
            .is_some_and(|completion| completion.recv_timeout(Duration::from_secs(2)).is_ok());
        if completed && let Some(handle) = self.thread.take() {
            let joined = handle.join();
            if !thread::panicking() {
                joined.expect("Portal proxy thread");
            }
        }
        // A timed-out worker is detached rather than blocking unwinding. Its
        // bounded socket read/write timeout will release it independently.
        let _ = self.thread.take();
    }
}

struct HolderResolver {
    origin_for_container: String,
    document: Arc<Mutex<Option<Value>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl HolderResolver {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:18092").expect("holder resolver");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("address").port();
        let document: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_document = Arc::clone(&document);
        let thread_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("holder resolver accept: {error}"),
                };
                if thread_stop.load(Ordering::Relaxed) {
                    break;
                }
                stream.set_nonblocking(false).expect("blocking stream");
                set_stream_timeouts(&stream);
                let (path, body) = read_request(&mut stream);
                let (status, response) = if path == "/health" {
                    (200, json!({"ok":true}).to_string())
                } else if path == "/resolve" {
                    let requested: Value = serde_json::from_str(&body).expect("resolver body");
                    let document = thread_document.lock().expect("document").clone();
                    match document {
                        Some(document) if requested["did"] == document["id"] => (
                            200,
                            json!({
                                "didDocument":document,
                                "didDocumentMetadata":{"deactivated":false},
                                "didResolutionMetadata":{"contentType":"application/did+ld+json"}
                            })
                            .to_string(),
                        ),
                        _ => (
                            404,
                            json!({
                                "didDocument":null,
                                "didDocumentMetadata":{"deactivated":false},
                                "didResolutionMetadata":{"error":"notFound"}
                            })
                            .to_string(),
                        ),
                    }
                } else {
                    (404, json!({"error":"not_found"}).to_string())
                };
                write!(
                    stream,
                    "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    if status == 200 { "OK" } else { "Not Found" },
                    response.len(),
                    response
                )
                .expect("holder resolver response");
            }
        });
        Self {
            origin_for_container: format!("http://host.docker.internal:{port}"),
            document,
            stop,
            thread: Some(handle),
        }
    }

    fn install(&self, headless_document: &Value) {
        let methods = headless_document["verificationMethods"]
            .as_array()
            .expect("verification methods")
            .iter()
            .map(|method| {
                json!({
                    "id":method["id"],
                    "type":"JsonWebKey",
                    "controller":method["controller"],
                    "publicKeyJwk":method["publicKeyJwk"]
                })
            })
            .collect::<Vec<_>>();
        let mut relationships = serde_json::Map::new();
        for relationship in headless_document["relationships"]
            .as_array()
            .expect("relationships")
        {
            relationships.insert(
                relationship["relationship"]
                    .as_str()
                    .expect("relationship name")
                    .to_owned(),
                relationship["methodIds"].clone(),
            );
        }
        let mut document = serde_json::Map::from_iter([
            ("id".to_owned(), headless_document["id"].clone()),
            ("verificationMethod".to_owned(), Value::Array(methods)),
        ]);
        document.extend(relationships);
        *self.document.lock().expect("document") = Some(Value::Object(document));
    }
}

impl Drop for HolderResolver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let port = self
            .origin_for_container
            .rsplit(':')
            .next()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(0);
        let _ = TcpStream::connect(("127.0.0.1", port));
        if let Some(handle) = self.thread.take() {
            let joined = handle.join();
            if !thread::panicking() {
                joined.expect("holder resolver thread");
            }
        }
    }
}

fn set_stream_timeouts(stream: &TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .expect("write timeout");
}

fn read_http_response(stream: &mut TcpStream, maximum_body_bytes: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer).expect("response header read");
        assert_ne!(read, 0, "complete response headers");
        bytes.extend_from_slice(&buffer[..read]);
        assert!(bytes.len() <= 64 * 1024, "response headers too large");
        if let Some(position) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).expect("response headers UTF-8");
    let content_lengths = headers
        .lines()
        .filter_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|value| value.parse::<usize>().ok())
        })
        .collect::<Vec<_>>();
    assert_eq!(content_lengths.len(), 1, "one Content-Length is required");
    assert!(
        !headers.to_ascii_lowercase().contains("transfer-encoding:"),
        "ambiguous transfer framing is rejected"
    );
    let content_length = content_lengths[0];
    assert!(
        content_length <= maximum_body_bytes,
        "response body too large"
    );
    let target = header_end
        .checked_add(content_length)
        .expect("bounded response length");
    assert!(bytes.len() <= target, "response exceeds Content-Length");
    while bytes.len() < target {
        let remaining = target - bytes.len();
        let read = stream
            .read({
                let chunk_length = remaining.min(buffer.len());
                &mut buffer[..chunk_length]
            })
            .expect("response body read");
        assert_ne!(read, 0, "complete response body");
        bytes.extend_from_slice(&buffer[..read]);
    }
    bytes
}

fn read_raw_request(stream: &mut TcpStream) -> (String, String, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer).expect("request read");
        assert_ne!(read, 0, "complete headers");
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(position) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end])
        .expect("headers")
        .to_owned();
    let length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|value| value.parse::<usize>().ok())
        })
        .unwrap_or(0);
    while bytes.len() - header_end < length {
        let read = stream.read(&mut buffer).expect("body read");
        assert_ne!(read, 0, "complete body");
        bytes.extend_from_slice(&buffer[..read]);
    }
    bytes.truncate(header_end + length);
    let path = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .expect("path")
        .to_owned();
    let body = String::from_utf8(bytes[header_end..].to_vec()).expect("body");
    (path, body, bytes)
}

fn read_request(stream: &mut TcpStream) -> (String, String) {
    let (path, body, _) = read_raw_request(stream);
    (path, body)
}

fn portal_request(origin: &str, method: &str, path: &str, body: Option<&str>) -> Value {
    let port = origin
        .rsplit(':')
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .expect("Portal proxy port");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("Portal issuer connection");
    set_stream_timeouts(&stream);
    let body = body.unwrap_or("");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("Portal request");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("Portal response");
    let split = response
        .windows(4)
        .position(|value| value == b"\r\n\r\n")
        .map(|index| index + 4)
        .expect("Portal response headers");
    assert!(
        std::str::from_utf8(&response[..split])
            .expect("headers")
            .starts_with("HTTP/1.1 200"),
        "Portal request failed"
    );
    serde_json::from_slice(&response[split..]).expect("Portal JSON")
}

fn issuer_public_facts(compose_project: &str) -> (String, String, Value) {
    let containers = Command::new("docker")
        .args([
            "container",
            "ls",
            "--quiet",
            "--filter",
            &format!("label=com.docker.compose.project={compose_project}"),
            "--filter",
            "label=com.docker.compose.service=issuer",
        ])
        .output()
        .expect("issuer container query");
    assert!(containers.status.success());
    let ids = String::from_utf8(containers.stdout).expect("container ids UTF-8");
    let mut ids = ids.lines().filter(|value| !value.is_empty());
    let issuer_container = ids.next().expect("one issuer container");
    assert!(ids.next().is_none(), "exactly one issuer container");
    let output = Command::new("docker")
        .args([
            "exec",
            issuer_container,
            "sh",
            "-c",
            r#"IFS= read -r value < /bootstrap/issuer-key-id; printf %s "$value""#,
        ])
        .output()
        .expect("issuer key id");
    assert!(output.status.success());
    let method = String::from_utf8(output.stdout)
        .expect("method UTF-8")
        .trim()
        .to_owned();
    let did = method.split_once('#').expect("full method").0.to_owned();
    let resolver = resolver_request(&did);
    let methods = resolver["didDocument"]["verificationMethod"]
        .as_array()
        .expect("issuer methods");
    let jwk = methods
        .iter()
        .find(|value| value["id"] == method)
        .and_then(|value| value.get("publicKeyJwk"))
        .cloned()
        .expect("issuer JWK");
    (did, method, jwk)
}

fn resolver_request(did: &str) -> Value {
    let mut stream = TcpStream::connect(("127.0.0.1", 9092)).expect("Portal resolver");
    set_stream_timeouts(&stream);
    let body = json!({"did":did}).to_string();
    write!(
        stream,
        "POST /resolve HTTP/1.1\r\nHost: 127.0.0.1:9092\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("resolver request");
    let response = read_http_response(&mut stream, 512 * 1024);
    let split = response
        .windows(4)
        .position(|value| value == b"\r\n\r\n")
        .map(|index| index + 4)
        .expect("resolver headers");
    serde_json::from_slice(&response[split..]).expect("resolver JSON")
}

fn write_manifest(
    path: &Path,
    issuer_origin: &str,
    issuer_did: &str,
    issuer_method: &str,
    jwk: &Value,
) -> String {
    let jwk_digest = hex::encode(Sha256::digest(serde_json::to_vec(jwk).expect("JWK")));
    let manifest = json!({
        "integrationCommit":PORTAL_INTEGRATION_COMMIT,
        "integrationTree":PORTAL_INTEGRATION_TREE,
        "issuerDid":issuer_did,
        "issuerJubjubJwk":jwk,
        "issuerJubjubJwkSha256":jwk_digest,
        "issuerMethod":issuer_method,
        "issuerOrigin":issuer_origin,
        "issuerResolverOrigin":PORTAL_ISSUER_RESOLVER,
        "portalPrHead":PORTAL_PR_HEAD,
        "profileSourceCommit":PORTAL_PROFILE_SOURCE,
        "provenanceSha256":PORTAL_PROVENANCE_SHA256,
        "schema":"oxid-portal-deployment-v2"
    });
    let bytes = serde_json::to_vec(&manifest).expect("manifest");
    fs::write(path, &bytes).expect("manifest file");
    hex::encode(Sha256::digest(bytes))
}

fn resolver_shape(value: &Value) -> String {
    let keys = |value: &Value| {
        value
            .as_object()
            .map(|object| object.keys().cloned().collect::<Vec<_>>().join("+"))
            .unwrap_or_else(|| "non_object".to_owned())
    };
    let document = &value["didDocument"];
    let first_method = document["verificationMethod"]
        .as_array()
        .and_then(|methods| methods.first());
    let method_shape = first_method.map_or_else(|| "none".to_owned(), keys);
    let method_type = first_method
        .and_then(|method| method["type"].as_str())
        .unwrap_or("none");
    let key_type = first_method
        .and_then(|method| method["publicKeyJwk"]["kty"].as_str())
        .unwrap_or("none");
    let curve = first_method
        .and_then(|method| method["publicKeyJwk"]["crv"].as_str())
        .unwrap_or("none");
    let controller_kind = if document["controller"].is_string() {
        "string"
    } else if document["controller"].is_array() {
        "array"
    } else {
        "other"
    };
    let kind = |value: &Value| match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(value) if value.is_empty() => "empty_string",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    format!(
        "root={};document={};resolution_meta={};document_meta={};context={};method={};method_type={method_type};kty={key_type};crv={curve};controller_kind={controller_kind};assertion_count={};assertion_first={};service_count={};created={};updated={};version={};resolution_error={};content_type={}",
        keys(value),
        keys(document),
        keys(&value["didResolutionMetadata"]),
        keys(&value["didDocumentMetadata"]),
        document["@context"].as_array().map_or(0, Vec::len),
        method_shape,
        document["assertionMethod"].as_array().map_or(0, Vec::len),
        document["assertionMethod"]
            .as_array()
            .and_then(|values| values.first())
            .map_or("missing", &kind),
        document["service"].as_array().map_or(0, Vec::len),
        kind(&value["didDocumentMetadata"]["created"]),
        kind(&value["didDocumentMetadata"]["updated"]),
        kind(&value["didDocumentMetadata"]["versionId"]),
        kind(&value["didResolutionMetadata"]["error"]),
        kind(&value["didResolutionMetadata"]["contentType"])
    )
}

fn diagnose_captured_response(
    response: &Value,
    issuer_did: &str,
    issuer_method: &str,
    issuer_jwk: &Value,
) -> String {
    let item = &response["credentials"][0];
    let decode = |value: &Value| {
        value
            .as_str()
            .and_then(|value| general_purpose::URL_SAFE_NO_PAD.decode(value).ok())
    };
    let Some(signed) = decode(&item["credential"]) else {
        return "response:invalid_signed_encoding".to_owned();
    };
    let Some(proof) = decode(&item["midnight"]["credentialProof"]["payload"]) else {
        return "response:invalid_proof_encoding".to_owned();
    };
    let private_json = match serde_json::to_vec(&item["midnight"]["credentialPrivateParts"]) {
        Ok(value) => value,
        Err(_) => return "response:invalid_private_json".to_owned(),
    };
    let private = match convert_portal_private_parts(&signed, &private_json) {
        Ok(value) => value,
        Err(_) => return "response:private_conversion_failed".to_owned(),
    };
    if private.is_empty() {
        return "response:private_conversion_empty".to_owned();
    }
    let jwk_digest = hex::encode(Sha256::digest(
        serde_json::to_vec(issuer_jwk).expect("diagnostic JWK"),
    ));
    let anchor = match DigitalPassportIssuerTrustAnchor::from_portal_jubjub(
        issuer_did,
        issuer_method,
        issuer_jwk["x"].as_str().unwrap_or_default(),
        issuer_jwk["y"].as_str().unwrap_or_default(),
        &jwk_digest,
    ) {
        Ok(value) => value,
        Err(_) => return "response:trust_anchor_failed".to_owned(),
    };
    let resolver = match HttpDidResolverConfig::new(PORTAL_ISSUER_RESOLVER) {
        Ok(value) => Arc::new(HttpDidResolver::new(value)),
        Err(_) => return "response:resolver_config_failed".to_owned(),
    };
    let issuer = match MidnightDid::parse(issuer_did.to_owned()) {
        Ok(value) => value,
        Err(_) => return "response:issuer_did_parse_failed".to_owned(),
    };
    if let Err(error) = block_on(resolver.resolve(&issuer)) {
        let shape = resolver_shape(&resolver_request(issuer_did));
        return format!("response:resolver_{error:?}:{shape}");
    }
    let verifier = MidnightCredentialVerifier::with_compact_policy(
        resolver.clone(),
        resolver,
        Arc::new(SystemClock),
        anchor,
    );
    match block_on(verifier.inspect(&signed, Some(&proof))) {
        Ok(inspection) => inspection
            .verification
            .stages()
            .iter()
            .map(|stage| {
                format!(
                    "{}:{}:{}",
                    stage.name().as_str(),
                    stage.status().as_str(),
                    stage.reason_code().unwrap_or("none")
                )
            })
            .collect::<Vec<_>>()
            .join(","),
        Err(_) => "response:verification_error".to_owned(),
    }
}

fn wait_for_portal() {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", 8090)).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("Portal issuer did not become ready");
}

fn approved_portal_offer(origin: &str) -> String {
    let kyc = portal_request(origin, "POST", "/api/issuer/kyc-sessions", Some("{}"));
    let session_id = kyc["sessionId"].as_str().expect("KYC session id");
    let status = portal_request(
        origin,
        "GET",
        &format!("/api/issuer/kyc-sessions/{session_id}/status"),
        None,
    );
    assert_eq!(
        status["status"].as_str().map(str::to_ascii_lowercase),
        Some("approved".to_owned())
    );
    kyc["credentialOfferUri"]
        .as_str()
        .expect("real Portal offer")
        .to_owned()
}

#[test]
#[ignore = "requires authenticated Portal integration checkout plus Docker/Nix compose stack"]
fn landed_portal_service_issues_to_headless_and_restores_in_new_process() {
    let portal_tree = PathBuf::from(
        std::env::var_os("PORTAL_INTEGRATION_TREE")
            .expect("PORTAL_INTEGRATION_TREE")
            .into_string()
            .expect("Portal path UTF-8"),
    );
    let oxid_head = std::env::var("OXID_PORTAL_EVIDENCE_HEAD").expect("OXID_PORTAL_EVIDENCE_HEAD");
    let compose_project =
        std::env::var("OXID_PORTAL_COMPOSE_PROJECT").expect("OXID_PORTAL_COMPOSE_PROJECT");
    assert!(compose_project.starts_with("oxidportal"), "Portal project");
    assert_eq!(
        std::env::var("OXID_PORTAL_HELPER_COMMIT").expect("OXID_PORTAL_HELPER_COMMIT"),
        PORTAL_HELPER_COMMIT
    );
    assert_eq!(
        std::env::var("OXID_PORTAL_HELPER_TREE").expect("OXID_PORTAL_HELPER_TREE"),
        PORTAL_HELPER_TREE
    );
    assert!(
        oxid_head.len() == 40 && oxid_head.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Oxid evidence head must be a commit SHA"
    );
    let evidence_path = PathBuf::from(
        std::env::var_os("OXID_PORTAL_EVIDENCE_PATH")
            .expect("OXID_PORTAL_EVIDENCE_PATH")
            .into_string()
            .expect("evidence path UTF-8"),
    );
    assert!(portal_tree.is_absolute());
    let run_root = evidence_path
        .parent()
        .expect("evidence parent")
        .join("runtime");
    let _ = fs::remove_dir_all(&run_root);
    fs::create_dir_all(&run_root).expect("runtime root");
    let _runtime_cleanup = RuntimeCleanup(run_root.clone());

    let portal_proxy = PortalProxy::spawn();
    let holder_resolver = HolderResolver::spawn();
    wait_for_portal();

    let (issuer_did, issuer_method, issuer_jwk) = issuer_public_facts(&compose_project);
    let manifest_path = run_root.join("deployment.json");
    let manifest_digest = write_manifest(
        &manifest_path,
        &portal_proxy.origin,
        &issuer_did,
        &issuer_method,
        &issuer_jwk,
    );
    let wallet_root = run_root.join("wallet");
    fs::create_dir_all(&wallet_root).expect("wallet root");
    let mut first = ProcessHarness::spawn(&wallet_root, &manifest_path, &manifest_digest);
    let created = first.request(
        "profile-create",
        "wallet.profile.create",
        json!({"displayName":"Portal integration"}),
    );
    let profile = created["result"]["profile"]["id"]
        .as_str()
        .expect("profile")
        .to_owned();
    assert_eq!(
        first.request(
            "profile-select",
            "wallet.profile.select",
            json!({"profileId":profile})
        )["ok"],
        true
    );
    assert_eq!(
        first.request("security", "wallet.security.initialize", json!({}))["ok"],
        true
    );
    let did = first.request("did", "did.create", json!({}));
    let document = &did["result"]["didRecord"]["document"];
    holder_resolver.install(document);
    let holder_did = document["id"].as_str().expect("holder DID").to_owned();
    let authentication = document["relationships"]
        .as_array()
        .expect("relationships")
        .iter()
        .find(|value| value["relationship"] == "authentication")
        .and_then(|value| value["methodIds"][0].as_str())
        .expect("authentication")
        .to_owned();
    let binding = document["verificationMethods"]
        .as_array()
        .expect("methods")
        .iter()
        .find(|value| value["publicKeyJwk"]["crv"] == "Jubjub")
        .and_then(|value| value["id"].as_str())
        .expect("binding")
        .to_owned();
    assert_ne!(authentication, binding);

    let refusal_offer = approved_portal_offer(&portal_proxy.origin);
    let refusal_prepared = first.request(
        "refusal-prepare",
        "credential.issuance.prepare",
        json!({"offer":refusal_offer}),
    );
    assert_eq!(
        refusal_prepared["result"]["issuance"]["state"],
        "awaiting_consent"
    );
    let refusal_issuance = refusal_prepared["result"]["issuance"]["id"]
        .as_str()
        .expect("refusal issuance");
    let refused = first.request(
        "refuse",
        "credential.issuance.refuse",
        json!({"issuanceId":refusal_issuance}),
    );
    assert_eq!(refused["result"]["issuance"]["state"], "refused");
    assert_eq!(portal_proxy.secret_request_count(), 0);

    let offer = approved_portal_offer(&portal_proxy.origin);
    let routed = first.request(
        "route",
        "identity.request.route",
        json!({"requestUri":offer}),
    );
    assert_eq!(routed["result"]["route"]["kind"], "credential_issuance");
    let prepared = first.request(
        "prepare",
        "credential.issuance.prepare",
        json!({"offer":offer}),
    );
    assert_eq!(prepared["result"]["issuance"]["state"], "awaiting_consent");
    let issuance = prepared["result"]["issuance"]["id"]
        .as_str()
        .expect("issuance")
        .to_owned();
    let unconfirmed = first.request(
        "unconfirmed",
        "credential.issuance.accept",
        json!({
            "issuanceId":issuance,
            "holderDid":holder_did,
            "methodId":authentication,
            "holderBindingMethodId":binding,
            "confirmed":false,
            "intent":"ACCEPT_CREDENTIAL_ISSUANCE"
        }),
    );
    assert_eq!(unconfirmed["error"]["code"], "confirmation_required");
    assert_eq!(portal_proxy.secret_request_count(), 0);

    let accepted = first.request(
        "accept",
        "credential.issuance.accept",
        json!({
            "issuanceId":issuance,
            "holderDid":holder_did,
            "methodId":authentication,
            "holderBindingMethodId":binding,
            "confirmed":true,
            "intent":"ACCEPT_CREDENTIAL_ISSUANCE"
        }),
    );
    if accepted["result"]["issuance"]["state"] != "succeeded" {
        let captured = portal_proxy
            .captured_credential_response
            .lock()
            .expect("captured response")
            .clone();
        let diagnosis = captured.as_ref().map_or_else(
            || "response:not_captured".to_owned(),
            |response| {
                diagnose_captured_response(response, &issuer_did, &issuer_method, &issuer_jwk)
            },
        );
        panic!("payload-free acceptance result: {accepted}; diagnosis={diagnosis}");
    }
    let secret_requests_before_replay = portal_proxy.secret_request_count();
    let replay = first.request(
        "replay",
        "credential.issuance.accept",
        json!({
            "issuanceId":issuance,
            "holderDid":holder_did,
            "methodId":authentication,
            "holderBindingMethodId":binding,
            "confirmed":true,
            "intent":"ACCEPT_CREDENTIAL_ISSUANCE"
        }),
    );
    assert_eq!(replay["ok"], false);
    assert_eq!(
        portal_proxy.secret_request_count(),
        secret_requests_before_replay,
        "terminal issuance replay must not contact secret endpoints"
    );
    let credential_id = accepted["result"]["issuance"]["credentialId"]
        .as_str()
        .expect("credential")
        .to_owned();
    let verification = first.request(
        "reverify",
        "credential.reverify",
        json!({"credentialId":credential_id}),
    );
    assert_eq!(
        verification["result"]["credential"]["verification"]["outcome"],
        "valid"
    );
    first.quit();

    let mut second = ProcessHarness::spawn(&wallet_root, &manifest_path, &manifest_digest);
    let restored = second.request("list", "credential.list", json!({}));
    assert_eq!(
        restored["result"]["credentials"].as_array().map(Vec::len),
        Some(1)
    );
    let restored_id = restored["result"]["credentials"][0]["id"]
        .as_str()
        .expect("restored credential");
    let reverified = second.request(
        "restore-reverify",
        "credential.reverify",
        json!({"credentialId":restored_id}),
    );
    assert_eq!(
        reverified["result"]["credential"]["verification"]["outcome"],
        "valid"
    );
    second.quit();

    let encrypted = fs::read(wallet_root.join("private/credentials.enc")).expect("encrypted store");
    for plaintext in [
        b"John".as_slice(),
        b"Doe".as_slice(),
        b"AB1234567".as_slice(),
    ] {
        assert!(
            !encrypted
                .windows(plaintext.len())
                .any(|value| value == plaintext)
        );
    }
    let evidence = json!({
        "acceptance":{
            "encryptedPersistence":true,
            "confirmationRequired":true,
            "exactBundleImported":true,
            "managedAuthenticationProof":true,
            "mockKycApproved":true,
            "refusalWithoutSecretCalls":true,
            "newProcessRestore":true,
            "reverified":true,
            "replayRejected":true,
            "separateJubjubAssertionBinding":true
        },
        "oxid":{"head":oxid_head},
        "portal":{
            "helperCommit":PORTAL_HELPER_COMMIT,
            "helperTree":PORTAL_HELPER_TREE,
            "integrationCommit":PORTAL_INTEGRATION_COMMIT,
            "integrationTree":PORTAL_INTEGRATION_TREE,
            "prHead":PORTAL_PR_HEAD,
            "profileSourceCommit":PORTAL_PROFILE_SOURCE,
            "provenanceSha256":PORTAL_PROVENANCE_SHA256
        },
        "schema":"oxid-portal-headless-evidence-v1"
    });
    let bytes = serde_json::to_vec(&evidence).expect("evidence");
    fs::create_dir_all(evidence_path.parent().expect("evidence parent")).expect("evidence parent");
    let temporary = evidence_path.with_extension("tmp");
    fs::write(&temporary, &bytes).expect("temporary evidence");
    fs::rename(&temporary, &evidence_path).expect("atomic evidence");
}

#[test]
fn observation_proxy_reads_content_length_without_waiting_for_eof_and_drops_bounded() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("persistent upstream");
    let upstream_address = listener.local_addr().expect("upstream address");
    let (served_sender, served_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let upstream = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("upstream accept");
        set_stream_timeouts(&stream);
        let _ = read_raw_request(&mut stream);
        let body = br#"{"ok":true}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
            body.len()
        )
        .expect("upstream headers");
        stream.write_all(body).expect("upstream body");
        served_sender.send(()).expect("served signal");
        // Deliberately keep the HTTP/1.1 connection open. A proxy that waits
        // for EOF deadlocks here instead of honoring Content-Length.
        let _ = release_receiver.recv_timeout(Duration::from_secs(5));
    });
    let proxy = PortalProxy::spawn_with_upstream(upstream_address);
    let port = proxy
        .origin
        .rsplit(':')
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .expect("proxy port");
    let mut client = TcpStream::connect(("127.0.0.1", port)).expect("proxy client");
    set_stream_timeouts(&client);
    client
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .expect("proxy request");
    let response = read_http_response(&mut client, 1024);
    assert!(response.ends_with(br#"{"ok":true}"#));
    served_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("upstream served");
    let started = Instant::now();
    drop(proxy);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "proxy drop must not wait for persistent upstream EOF"
    );
    release_sender.send(()).expect("release upstream");
    upstream.join().expect("upstream thread");
}

#[test]
fn runtime_cleanup_removes_encrypted_store_and_wrapping_key_during_unwind() {
    let root = std::env::temp_dir().join(format!(
        "oxid-portal-runtime-cleanup-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let result = std::panic::catch_unwind({
        let root = root.clone();
        move || {
            fs::create_dir_all(root.join("wallet/private")).expect("private root");
            let _cleanup = RuntimeCleanup(root.clone());
            fs::write(root.join("wallet/private/credentials.enc"), b"ciphertext")
                .expect("encrypted store");
            fs::write(root.join("wallet/private/credentials.key"), b"wrapping key")
                .expect("wrapping key");
            panic!("synthetic live-flow failure");
        }
    });
    assert!(result.is_err());
    assert!(!root.exists(), "sensitive runtime root must be removed");
}
