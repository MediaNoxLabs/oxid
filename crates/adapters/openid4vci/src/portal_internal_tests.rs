use std::sync::{Arc, Mutex};

use oxid_identity_application::{
    DidDocumentMetadataView, DidDocumentView, DidOperationError, DidRecordQuery, DidRecordView,
    PublicJwkView, VerificationMethodView, VerificationRelationshipView,
};
use oxid_protocol_application::{
    CredentialHolderProofPort, HolderProofError, HolderProofFuture, PrepareIssuanceRequest,
};
use oxid_protocol_domain::ProtocolProfileId;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
};

use super::*;

#[test]
fn deployment_resolver_base_accepts_the_exact_tailnet_prefix() {
    assert_eq!(
        validate_origin("https://oxid-demo.tail1234.ts.net:9443"),
        Ok(())
    );
    assert_eq!(
        validate_resolver_base("https://oxid-demo.tail1234.ts.net:9443/issuer-resolver"),
        Ok(())
    );
}

const HOLDER_DID: &str = "did:example:synthetic-holder";
const AUTH_METHOD: &str = "did:example:synthetic-holder#auth";
const BINDING_METHOD: &str = "did:example:synthetic-holder#assert";
const POSITIVE_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../fixtures/laceid-portal/76e8edf394a4cb37ca822037272d543c68f25f71/openid4vci-final"
);

struct Proof;

impl CredentialHolderProofPort for Proof {
    fn create<'a>(&'a self, request: HolderProofRequest) -> HolderProofFuture<'a> {
        Box::pin(async move {
            if request.holder_did == HOLDER_DID
                && request.method_id == AUTH_METHOD
                && !request.nonce.is_empty()
            {
                Ok("SYNTHETIC.JWT.PROOF".to_owned())
            } else {
                Err(HolderProofError::Rejected)
            }
        })
    }
}

struct Did;

impl GetDidRecordUseCase for Did {
    fn execute(&self, query: DidRecordQuery) -> Result<DidRecordView, DidOperationError> {
        Ok(DidRecordView {
            document: DidDocumentView {
                contexts: vec![],
                id: query.did,
                network: "undeployed".to_owned(),
                also_known_as: vec![],
                verification_methods: vec![
                    VerificationMethodView {
                        id: AUTH_METHOD.to_owned(),
                        controller: HOLDER_DID.to_owned(),
                        public_key_jwk: PublicJwkView {
                            key_type: "OKP".to_owned(),
                            curve: "Ed25519".to_owned(),
                            x: general_purpose::URL_SAFE_NO_PAD.encode([3_u8; 32]),
                            y: None,
                        },
                    },
                    VerificationMethodView {
                        id: BINDING_METHOD.to_owned(),
                        controller: HOLDER_DID.to_owned(),
                        public_key_jwk: PublicJwkView {
                            key_type: "EC".to_owned(),
                            curve: "Jubjub".to_owned(),
                            x: general_purpose::URL_SAFE_NO_PAD.encode([4_u8; 32]),
                            y: Some(general_purpose::URL_SAFE_NO_PAD.encode([5_u8; 32])),
                        },
                    },
                ],
                relationships: vec![
                    VerificationRelationshipView {
                        relationship: "authentication".to_owned(),
                        method_ids: vec![AUTH_METHOD.to_owned()],
                    },
                    VerificationRelationshipView {
                        relationship: "assertionMethod".to_owned(),
                        method_ids: vec![BINDING_METHOD.to_owned()],
                    },
                ],
                services: vec![],
            },
            document_metadata: DidDocumentMetadataView {
                created: None,
                updated: None,
                deactivated: Some(false),
                version_id: None,
                next_update: None,
                next_version_id: None,
                equivalent_ids: vec![],
                canonical_id: None,
            },
            content_type: None,
            source: "managed".to_owned(),
            managed_method_ids: vec![AUTH_METHOD.to_owned(), BINDING_METHOD.to_owned()],
        })
    }
}

struct Decoder;

impl PortalCredentialMaterialDecoder for Decoder {
    fn decode(
        &self,
        signed_credential: &[u8],
        private_json: &[u8],
    ) -> Result<Vec<u8>, PortalCredentialMaterialError> {
        if signed_credential == b"synthetic-signed-bytes"
            && private_json == br#"{"synthetic":"private-parts-contract-value"}"#
        {
            Ok(b"decoded-private".to_vec())
        } else {
            Err(PortalCredentialMaterialError::Invalid)
        }
    }
}

struct UnavailableDecoder;

impl PortalCredentialMaterialDecoder for UnavailableDecoder {
    fn decode(&self, _: &[u8], _: &[u8]) -> Result<Vec<u8>, PortalCredentialMaterialError> {
        Err(PortalCredentialMaterialError::Unavailable)
    }
}

fn deployment(origin: &str) -> PortalDeploymentManifest {
    let jwk = PortalPublicJwk {
        curve: "Jubjub".to_owned(),
        key_type: "EC".to_owned(),
        x: general_purpose::URL_SAFE_NO_PAD.encode([7_u8; 32]),
        y: general_purpose::URL_SAFE_NO_PAD.encode([9_u8; 32]),
    };
    let jwk_digest = sha256_hex(&serde_json::to_vec(&jwk).expect("jwk"));
    let manifest = PortalDeploymentManifest {
            integration_commit: PORTAL_INTEGRATION_COMMIT.to_owned(),
            integration_tree: PORTAL_INTEGRATION_TREE.to_owned(),
            issuer_did: "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
            issuer_jubjub_jwk: jwk,
            issuer_jubjub_jwk_sha256: jwk_digest,
            issuer_method: "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef#key-assert".to_owned(),
            issuer_origin: origin.to_owned(),
            issuer_resolver_origin: origin.to_owned(),
            provenance_sha256: PORTAL_PROVENANCE_SHA256.to_owned(),
            schema: "oxid-portal-deployment-v3".to_owned(),
        };
    let bytes = serde_json::to_vec(&manifest).expect("manifest");
    PortalDeploymentManifest::from_bytes(&bytes, &sha256_hex(&bytes)).expect("deployment")
}

fn offer(origin: &str) -> String {
    let embedded = json!({
        "credential_issuer": origin,
        "credential_configuration_ids": [PORTAL_CONFIGURATION_ID],
        "grants": {PRE_AUTHORIZED_GRANT: {"pre-authorized_code": "SECRET_CODE"}}
    });
    let mut url = Url::parse("openid-credential-offer://").expect("offer URL");
    url.query_pairs_mut()
        .append_pair("credential_offer", &embedded.to_string());
    url.into()
}

fn response_for(path: &str, origin: &str) -> String {
    match path {
            "/.well-known/openid-credential-issuer" => json!({
                "authorization_servers": [origin],
                "credential_configurations_supported": {
                    PORTAL_CONFIGURATION_ID: {
                        "credential_metadata": {"display": [{"locale":"en","name":"Digital Passport"}]},
                        "cryptographic_binding_methods_supported": ["did"],
                        "format": PORTAL_FORMAT,
                        "proof_types_supported": {"jwt":{"proof_signing_alg_values_supported":["EdDSA","ES256"]}},
                        "scope": "digital-passport"
                    }
                },
                "credential_endpoint": format!("{origin}/api/issuer/credentials"),
                "credential_issuer": origin,
                "nonce_endpoint": format!("{origin}/api/issuer/nonce")
            }).to_string(),
            "/.well-known/oauth-authorization-server" => json!({
                "grant_types_supported": [PRE_AUTHORIZED_GRANT],
                "issuer": origin,
                "pre-authorized_grant_anonymous_access_supported": true,
                "token_endpoint": format!("{origin}/api/issuer/token")
            }).to_string(),
            "/api/issuer/token" => json!({
                "access_token":"SECRET_ACCESS_TOKEN","expires_in":300,"token_type":"Bearer"
            }).to_string(),
            "/api/issuer/nonce" => json!({"c_nonce":"SECRET_NONCE","c_nonce_expires_in":300}).to_string(),
            "/api/issuer/credentials" => json!({"credentials":[{
                "credential": general_purpose::URL_SAFE_NO_PAD.encode(b"synthetic-signed-bytes"),
                "midnight": {
                    "credentialFamily": PORTAL_FAMILY,
                    "credentialPrivateParts":{"synthetic":"private-parts-contract-value"},
                    "credentialProof":{"encoding":PORTAL_ENCODING,"payload":general_purpose::URL_SAFE_NO_PAD.encode(b"synthetic-proof")},
                    "encoding":PORTAL_ENCODING,
                    "expiresAt":"2030-12-15T00:00:00Z",
                    "hasExpiration":true,
                    "holderBinding":{"challenge":"SECRET_NONCE","holderDidMethod":{"did":HOLDER_DID,"keyType":"jubjub","methodId":BINDING_METHOD},"method":"explicit_did_method"},
                    "schemaId":PORTAL_SCHEMA_ID,"schemaVersion":PORTAL_SCHEMA_VERSION
                }
            }]}).to_string(),
            _ => panic!("unexpected request path"),
        }
}

async fn spawn_server(
    requests: usize,
    status: StatusCode,
) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let origin = format!("http://{address}");
    let journal = Arc::new(Mutex::new(Vec::new()));
    let task_origin = origin.clone();
    let task_journal = Arc::clone(&journal);
    let task = tokio::spawn(async move {
        for _ in 0..requests {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 4096];
            let header_end = loop {
                let read = socket.read(&mut buffer).await.expect("read");
                assert_ne!(read, 0, "complete request headers");
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(position) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                    break position + 4;
                }
            };
            let headers = std::str::from_utf8(&bytes[..header_end]).expect("headers");
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            while bytes.len() - header_end < content_length {
                let read = socket.read(&mut buffer).await.expect("body");
                assert_ne!(read, 0, "complete request body");
                bytes.extend_from_slice(&buffer[..read]);
            }
            let request = String::from_utf8(bytes).expect("request UTF-8");
            let first = request.lines().next().expect("request line");
            let path = first.split_whitespace().nth(1).expect("path").to_owned();
            task_journal.lock().expect("journal").push(request);
            let body = if status == StatusCode::OK {
                response_for(&path, &task_origin)
            } else {
                "HOSTILE_SECRET_RESPONSE_BODY".to_owned()
            };
            let location = if status.is_redirection() {
                format!("Location: {task_origin}/redirected\r\n")
            } else {
                String::new()
            };
            let response = format!(
                "HTTP/1.1 {} {}\r\n{location}Content-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Error"),
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("response");
        }
    });
    (origin, journal, task)
}

fn protocol(origin: &str) -> PortalOid4vciClient {
    PortalOid4vciClient::new(
        deployment(origin),
        Arc::new(Proof),
        Arc::new(Did),
        Arc::new(Decoder),
    )
    .expect("client")
}

#[test]
fn exact_public_positive_and_negative_profile_fixtures_are_final_only() {
    let offer = std::fs::read_to_string(format!("{POSITIVE_ROOT}/positive/credential-offer.txt"))
        .expect("offer");
    parse_portal_offer(offer.trim(), "https://issuer.example").expect("positive offer");
    let issuer = std::fs::read(format!(
        "{POSITIVE_ROOT}/positive/credential-issuer-metadata.json"
    ))
    .expect("issuer metadata");
    validate_portal_issuer_metadata_shape(&issuer).expect("exact positive issuer metadata shape");
    let issuer = parse_issuer_metadata(&issuer, EndpointPolicy::StandaloneLoopback)
        .expect("positive issuer metadata");
    validate_portal_endpoint(
        &issuer.credential_endpoint,
        "https://issuer.example",
        "/api/issuer/credentials",
    )
    .expect("credential endpoint");
    let authorization = std::fs::read(format!(
        "{POSITIVE_ROOT}/positive/authorization-server-metadata.json"
    ))
    .expect("authorization metadata");
    parse_portal_authorization_metadata(&authorization, "https://issuer.example")
        .expect("positive authorization metadata");
    let response = std::fs::read(format!("{POSITIVE_ROOT}/positive/credential-response.json"))
        .expect("credential response");
    let issued = parse_portal_credential_response(
        &response,
        HOLDER_DID,
        BINDING_METHOD,
        "SYNTHETIC_NONCE",
        &Decoder,
    )
    .expect("positive credential response");
    assert_eq!(issued.signed_bytes, b"synthetic-signed-bytes");
    assert_eq!(issued.detached_proof, Some(b"synthetic-proof".to_vec()));
    assert_eq!(issued.private_material, Some(b"decoded-private".to_vec()));

    for negative in ["legacy-singular-response.json", "malformed-proof.json"] {
        let bytes = std::fs::read(format!("{POSITIVE_ROOT}/negative/{negative}"))
            .expect("negative fixture");
        assert!(
            parse_portal_credential_response(
                &bytes,
                HOLDER_DID,
                BINDING_METHOD,
                "SYNTHETIC_NONCE",
                &Decoder
            )
            .is_err()
        );
    }
}

#[test]
fn credential_response_rejects_duplicate_fields_and_preserves_decoder_errors() {
    let response = std::fs::read(format!("{POSITIVE_ROOT}/positive/credential-response.json"))
        .expect("credential response");
    let duplicate = String::from_utf8(response.clone())
        .expect("fixture UTF-8")
        .replacen(
            "\"credentials\": [",
            "\"credentials\": [],\"credentials\": [",
            1,
        );
    assert!(
        parse_portal_credential_response(
            duplicate.as_bytes(),
            HOLDER_DID,
            BINDING_METHOD,
            "SYNTHETIC_NONCE",
            &Decoder,
        )
        .is_err()
    );
    assert_eq!(
        parse_portal_credential_response(
            &vec![b' '; MAX_CREDENTIAL_BYTES + 1],
            HOLDER_DID,
            BINDING_METHOD,
            "SYNTHETIC_NONCE",
            &Decoder,
        ),
        Err(IssuanceProtocolError::InvalidCredentialResponse),
    );

    let too_deep = String::from_utf8(response.clone())
        .expect("fixture UTF-8")
        .replacen(
            r#""private-parts-contract-value""#,
            &format!(
                "{}0{}",
                "[".repeat(super::super::MAX_JSON_DEPTH),
                "]".repeat(super::super::MAX_JSON_DEPTH)
            ),
            1,
        );
    assert_eq!(
        parse_portal_credential_response(
            too_deep.as_bytes(),
            HOLDER_DID,
            BINDING_METHOD,
            "SYNTHETIC_NONCE",
            &Decoder,
        ),
        Err(IssuanceProtocolError::InvalidCredentialResponse),
    );
    assert_eq!(
        parse_portal_credential_response(
            &response,
            HOLDER_DID,
            BINDING_METHOD,
            "SYNTHETIC_NONCE",
            &UnavailableDecoder,
        ),
        Err(IssuanceProtocolError::ProtectionUnavailable),
    );
}

#[tokio::test]
async fn request_sequence_is_exact_and_refusal_makes_no_secret_posts() {
    let (origin, journal, server) = spawn_server(2, StatusCode::OK).await;
    let client = protocol(&origin);
    let profile = ProtocolProfileId::parse("profile_1").expect("profile");
    let prepared = client
        .prepare(PrepareIssuanceRequest {
            profile_id: profile,
            offer: offer(&origin),
        })
        .await
        .expect("prepare");
    client.discard(&prepared.id).expect("discard");
    server.await.expect("server");
    let journal = journal.lock().expect("journal");
    assert_eq!(journal.len(), 2);
    assert!(journal[0].starts_with("GET /.well-known/openid-credential-issuer "));
    assert!(journal[1].starts_with("GET /.well-known/oauth-authorization-server "));
    assert!(
        journal
            .iter()
            .all(|request| !request.contains("SECRET_CODE"))
    );
}

#[tokio::test]
async fn unmanaged_authentication_is_rejected_before_token_nonce_or_credential_calls() {
    let (origin, journal, server) = spawn_server(2, StatusCode::OK).await;
    let client = protocol(&origin);
    let profile = ProtocolProfileId::parse("profile_1").expect("profile");
    let prepared = client
        .prepare(PrepareIssuanceRequest {
            profile_id: profile.clone(),
            offer: offer(&origin),
        })
        .await
        .expect("prepare");
    let error = client
        .issue(ProtocolIssueRequest {
            profile_id: profile,
            issuance_id: prepared.id,
            holder_did: HOLDER_DID.to_owned(),
            method_id: "did:example:synthetic-holder#unmanaged".to_owned(),
            holder_binding_method_id: BINDING_METHOD.to_owned(),
        })
        .await
        .expect_err("unmanaged authentication method must fail");
    assert_eq!(error, IssuanceProtocolError::InvalidProof);
    server.await.expect("server");
    assert_eq!(journal.lock().expect("journal").len(), 2);
}

#[tokio::test]
async fn exact_http_flow_uses_form_token_post_nonce_managed_proof_and_distinct_jubjub_binding() {
    let (origin, journal, server) = spawn_server(5, StatusCode::OK).await;
    let client = protocol(&origin);
    let profile = ProtocolProfileId::parse("profile_1").expect("profile");
    let prepared = client
        .prepare(PrepareIssuanceRequest {
            profile_id: profile.clone(),
            offer: offer(&origin),
        })
        .await
        .expect("prepare");
    let issued = client
        .issue(ProtocolIssueRequest {
            profile_id: profile,
            issuance_id: prepared.id,
            holder_did: HOLDER_DID.to_owned(),
            method_id: AUTH_METHOD.to_owned(),
            holder_binding_method_id: BINDING_METHOD.to_owned(),
        })
        .await
        .expect("issue");
    server.await.expect("server");
    assert_eq!(issued.signed_bytes, b"synthetic-signed-bytes");
    assert_eq!(issued.detached_proof, Some(b"synthetic-proof".to_vec()));
    assert_eq!(issued.private_material, Some(b"decoded-private".to_vec()));

    let journal = journal.lock().expect("journal");
    assert_eq!(journal.len(), 5);
    assert!(journal[2].starts_with("POST /api/issuer/token "));
    assert!(journal[2].contains("content-type: application/x-www-form-urlencoded"));
    assert!(journal[2].contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Apre-authorized_code&pre-authorized_code=SECRET_CODE"));
    assert!(journal[3].starts_with("POST /api/issuer/nonce "));
    assert_eq!(
        journal[3].split("\r\n\r\n").nth(1),
        Some(""),
        "Portal's nonce handler rejects any request body"
    );
    assert!(journal[4].starts_with("POST /api/issuer/credentials "));
    let split = journal[4].split("\r\n\r\n").nth(1).expect("body");
    let body = parse_strict_json(split.as_bytes()).expect("request JSON");
    assert_eq!(body["proofs"]["jwt"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["proofs"]["jwt"][0], "SYNTHETIC.JWT.PROOF");
    assert_eq!(body["midnight"]["holderBindingMethod"], BINDING_METHOD);
    assert_ne!(AUTH_METHOD, BINDING_METHOD);
    assert!(
        journal[4]
            .to_ascii_lowercase()
            .contains("authorization: bearer secret_access_token")
    );
    assert!(journal[..4].iter().all(|request| {
        !request
            .to_ascii_lowercase()
            .contains("authorization: bearer")
    }));
}

#[tokio::test]
async fn status_and_redirect_errors_are_payload_free_and_never_retried() {
    for status in [StatusCode::INTERNAL_SERVER_ERROR, StatusCode::FOUND] {
        let (origin, journal, server) = spawn_server(1, status).await;
        let client = protocol(&origin);
        let error = client
            .prepare(PrepareIssuanceRequest {
                profile_id: ProtocolProfileId::parse("profile_1").expect("profile"),
                offer: offer(&origin),
            })
            .await
            .expect_err("status must fail");
        server.await.expect("server");
        assert_eq!(journal.lock().expect("journal").len(), 1);
        assert!(!format!("{error:?} {error}").contains("HOSTILE_SECRET_RESPONSE_BODY"));
    }
}

#[tokio::test]
async fn service_unavailable_status_is_payload_free_and_not_retried() {
    let (origin, journal, server) = spawn_server(1, StatusCode::SERVICE_UNAVAILABLE).await;
    let error = protocol(&origin)
        .prepare(PrepareIssuanceRequest {
            profile_id: ProtocolProfileId::parse("profile_1").expect("profile"),
            offer: offer(&origin),
        })
        .await
        .expect_err("service unavailability must fail");
    server.await.expect("server");

    assert_eq!(error, IssuanceProtocolError::Unavailable);
    assert_eq!(journal.lock().expect("journal").len(), 1);
    assert!(!format!("{error:?} {error}").contains("HOSTILE_SECRET_RESPONSE_BODY"));
}

#[tokio::test]
async fn whole_request_timeout_is_payload_free_and_not_retried() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let origin = format!("http://{}", listener.local_addr().expect("address"));
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let task_calls = Arc::clone(&calls);
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        task_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut buffer = [0_u8; 1024];
        let _ = socket.read(&mut buffer).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    let mut client = protocol(&origin);
    client.client = Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_millis(10))
        .timeout(Duration::from_millis(10))
        .build()
        .expect("test client");
    let error = client
        .prepare(PrepareIssuanceRequest {
            profile_id: ProtocolProfileId::parse("profile_1").expect("profile"),
            offer: offer(&origin),
        })
        .await
        .expect_err("timeout must fail");
    server.await.expect("server");
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(error, IssuanceProtocolError::Unavailable);
}

async fn oversized_metadata_response(response: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let origin = format!("http://{}", listener.local_addr().expect("address"));
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buffer = [0_u8; 1024];
        let _ = socket.read(&mut buffer).await;
        socket.write_all(&response).await.expect("response");
    });
    (origin, server)
}

#[tokio::test]
async fn declared_and_streamed_metadata_limits_are_enforced() {
    let declared = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_METADATA_BYTES + 1
        )
        .into_bytes();
    let streamed_body = vec![b' '; MAX_METADATA_BYTES + 1];
    let mut streamed = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n",
            streamed_body.len()
        )
        .into_bytes();
    streamed.extend_from_slice(&streamed_body);
    streamed.extend_from_slice(b"\r\n0\r\n\r\n");

    for response in [declared, streamed] {
        let (origin, server) = oversized_metadata_response(response).await;
        let error = protocol(&origin)
            .prepare(PrepareIssuanceRequest {
                profile_id: ProtocolProfileId::parse("profile_1").expect("profile"),
                offer: offer(&origin),
            })
            .await
            .expect_err("oversized response must fail");
        server.await.expect("server");
        assert_eq!(error, IssuanceProtocolError::InvalidMetadata);
    }
}

#[test]
fn native_transport_source_disables_ambient_routing_and_automatic_replay() {
    let source = include_str!("portal.rs");
    for required in [
        ".no_proxy()",
        ".redirect(Policy::none())",
        ".retry(reqwest::retry::never())",
        ".tls_certs_only(roots)",
    ] {
        assert!(
            source.contains(required),
            "missing transport lock: {required}"
        );
    }
    for forbidden_feature in ["gzip", "brotli", "deflate", "zstd", "cookies"] {
        assert!(!source.contains(&format!(".{forbidden_feature}(")));
    }
}

#[test]
fn token_and_nonce_responses_remain_strictly_typed() {
    assert_eq!(
        parse_token_response(br#"{"access_token":"token","expires_in":300,"token_type":"Bearer"}"#)
            .as_ref()
            .map(|value| value.as_str()),
        Ok("token")
    );
    assert_eq!(
        parse_nonce_response(br#"{"c_nonce":"nonce","c_nonce_expires_in":300}"#)
            .as_ref()
            .map(|value| value.as_str()),
        Ok("nonce")
    );
    assert_eq!(
        parse_token_response(
            br#"{"access_token":"token","expires_in":300,"token_type":"B\u0065arer"}"#
        )
        .as_ref()
        .map(|value| value.as_str()),
        Ok("token")
    );

    for response in [
        br#"{"access_token":"token","expires_in":"300","token_type":"Bearer"}"#.as_slice(),
        br#"{"access_token":"token","expires_in":300,"token_type":"Basic"}"#,
        br#"{"access_token":"","expires_in":300,"token_type":"Bearer"}"#,
        br#"{"access_token":7,"expires_in":300,"token_type":"Bearer"}"#,
        br#"{"access_token":"token","expires_in":300,"token_type":"Bearer","extra":true}"#,
        br#"{"expires_in":300,"token_type":"Bearer"}"#,
        br#"{"access_token":"token","token_type":"Bearer"}"#,
        br#"{"access_token":"token","expires_in":300}"#,
    ] {
        assert_eq!(
            parse_token_response(response),
            Err(IssuanceProtocolError::IssuerRejected)
        );
    }
    for response in [
        br#"{"c_nonce":"nonce","c_nonce_expires_in":"300"}"#.as_slice(),
        br#"{"c_nonce":"","c_nonce_expires_in":300}"#,
        br#"{"c_nonce":7,"c_nonce_expires_in":300}"#,
        br#"{"c_nonce":"nonce","c_nonce_expires_in":300,"extra":true}"#,
        br#"{"c_nonce_expires_in":300}"#,
        br#"{"c_nonce":"nonce"}"#,
    ] {
        assert_eq!(
            parse_nonce_response(response),
            Err(IssuanceProtocolError::IssuerRejected)
        );
    }
}

#[test]
fn secret_response_strings_decode_escaped_ascii_and_unicode_without_serde_ownership() {
    assert_eq!(
        parse_token_response(
            br#"{"access_token":"quote:\" slash:\\ solidus:\/ controls:\b\f\n\r\t","expires_in":300,"token_type":"Bearer"}"#
        )
        .as_ref()
        .map(|value| value.as_str()),
        Ok("quote:\" slash:\\ solidus:/ controls:\u{0008}\u{000c}\n\r\t")
    );
    assert_eq!(
        parse_nonce_response(
            br#"{"c_nonce":"caf\u00e9-\u6c34-\ud83d\ude80","c_nonce_expires_in":300}"#
        )
        .as_ref()
        .map(|value| value.as_str()),
        Ok("café-水-🚀")
    );
    assert_eq!(
        parse_nonce_response(
            "{\"c_nonce\":\"direct-水-🚀\",\"c_nonce_expires_in\":300}".as_bytes()
        )
        .as_ref()
        .map(|value| value.as_str()),
        Ok("direct-水-🚀")
    );
}

#[test]
fn secret_response_strings_reject_malformed_escapes_and_decoded_bounds() {
    for response in [
        br#"{"access_token":"prefix\xsuffix","expires_in":300,"token_type":"Bearer"}"#.as_slice(),
        br#"{"access_token":"prefix\u12","expires_in":300,"token_type":"Bearer"}"#,
        br#"{"access_token":"prefix\ud800suffix","expires_in":300,"token_type":"Bearer"}"#,
        br#"{"access_token":"prefix\ud800\u0041","expires_in":300,"token_type":"Bearer"}"#,
        br#"{"access_token":"prefix\udc00","expires_in":300,"token_type":"Bearer"}"#,
    ] {
        assert_eq!(
            parse_token_response(response),
            Err(IssuanceProtocolError::InvalidMetadata)
        );
    }

    let oversized_ascii = format!(
        r#"{{"access_token":"{}","expires_in":300,"token_type":"Bearer"}}"#,
        "a".repeat(MAX_SECRET_BYTES + 1)
    );
    assert_eq!(
        parse_token_response(oversized_ascii.as_bytes()),
        Err(IssuanceProtocolError::IssuerRejected)
    );

    let oversized_escaped = format!(
        r#"{{"c_nonce":"{}","c_nonce_expires_in":300}}"#,
        "\\u00e9".repeat(MAX_SECRET_BYTES / 2 + 1)
    );
    assert_eq!(
        parse_nonce_response(oversized_escaped.as_bytes()),
        Err(IssuanceProtocolError::IssuerRejected)
    );
}

#[test]
fn secret_response_partial_failures_and_full_document_limits_are_payload_free() {
    for response in [
        br#"{"access_token":"decoded\nsecret","expires_in":300,"token_type":"Basic"}"#.as_slice(),
        br#"{"access_token":"borrowed\u0020secret","expires_in":"invalid","token_type":"Bearer"}"#,
    ] {
        let error = parse_token_response(response).expect_err("response must be rejected");
        assert_eq!(error, IssuanceProtocolError::IssuerRejected);
        assert_eq!(error.code(), "issuer_rejected");
    }
    let nonce_error = parse_nonce_response(
        br#"{"c_nonce":"borrowed\ud83d\ude80secret","c_nonce_expires_in":"invalid"}"#,
    )
    .expect_err("partially parsed nonce response must be rejected");
    assert_eq!(nonce_error, IssuanceProtocolError::IssuerRejected);
    assert_eq!(nonce_error.code(), "issuer_rejected");

    for response in [
        br#"{"access_token":"borrowed-secret","expires_in":300,"token_type":"Bearer"} trailing"#.as_slice(),
        br#"{"access_token":"partially\xsensitive","expires_in":300,"token_type":"Bearer"}"#,
        br#"{"access_token":"first","access_token":"second","expires_in":300,"token_type":"Bearer"}"#,
        br#"{"access_token":"first","\u0061ccess_token":"second","expires_in":300,"token_type":"Bearer"}"#,
        br#"{"access_token":"borrowed-secret","expires_in":300,"token_type":"Bearer","extra":{"key":1,"key":2}}"#,
    ] {
        let error = parse_token_response(response).expect_err("response must be rejected");
        assert_eq!(error, IssuanceProtocolError::InvalidMetadata);
        assert_eq!(error.code(), "invalid_metadata");
    }
    assert_eq!(
        parse_nonce_response(br#"{"c_nonce":"first","c_nonce":"second","c_nonce_expires_in":300}"#),
        Err(IssuanceProtocolError::InvalidMetadata)
    );

    let nesting = super::super::MAX_JSON_DEPTH;
    let too_deep = format!(
        r#"{{"access_token":"borrowed-secret","expires_in":300,"token_type":"Bearer","extra":{}0{}}}"#,
        "[".repeat(nesting),
        "]".repeat(nesting)
    );
    assert_eq!(
        parse_token_response(too_deep.as_bytes()),
        Err(IssuanceProtocolError::InvalidMetadata)
    );
    let stack_hostile = format!(
        r#"{{"access_token":"borrowed-secret","expires_in":300,"token_type":"Bearer","extra":{}0{}}}"#,
        "[".repeat(4_096),
        "]".repeat(4_096)
    );
    assert_eq!(
        parse_token_response(stack_hostile.as_bytes()),
        Err(IssuanceProtocolError::InvalidMetadata),
        "depth must be rejected before recursively traversing a size-bounded hostile value"
    );
    let at_depth_limit = format!(
        r#"{{"access_token":"borrowed-secret","expires_in":300,"token_type":"Bearer","extra":{}0{}}}"#,
        "[".repeat(nesting - 1),
        "]".repeat(nesting - 1)
    );
    assert_eq!(
        parse_token_response(at_depth_limit.as_bytes()),
        Err(IssuanceProtocolError::IssuerRejected),
        "a structurally valid response at the old depth limit reaches semantic validation"
    );

    let oversized_response = format!(
        r#"{{"access_token":"token","expires_in":300,"token_type":"Bearer","extra":"{}"}}"#,
        "x".repeat(super::super::MAX_PROTOCOL_RESPONSE_BYTES)
    );
    assert_eq!(
        parse_token_response(oversized_response.as_bytes()),
        Err(IssuanceProtocolError::InvalidMetadata)
    );
}

#[test]
fn hostile_urls_content_types_sizes_and_legacy_shapes_fail_closed() {
    for url in [
        "http://issuer.example/api/issuer/token",
        "https://user:pass@issuer.example/api/issuer/token",
        "https://issuer.example/api/issuer/token#fragment",
        "https://issuer.example/other",
    ] {
        assert!(
            validate_portal_endpoint(url, "https://issuer.example", "/api/issuer/token").is_err()
        );
    }
    for content_type in [
        "text/json",
        "application/json; charset=iso-8859-1",
        "application/json; charset=utf-8; profile=hostile",
    ] {
        assert!(validate_json_content_type(content_type).is_err());
    }
    assert!(validate_json_content_type("application/json").is_ok());
    assert!(validate_json_content_type("application/json; charset=UTF-8").is_ok());
    assert_eq!(
        parse_token_response(include_bytes!(
            "../../../../fixtures/laceid-portal/76e8edf394a4cb37ca822037272d543c68f25f71/openid4vci-final/negative/legacy-json-token-request.json"
        )),
        Err(IssuanceProtocolError::IssuerRejected)
    );
    assert_eq!(
        parse_strict_json(&vec![b' '; MAX_CREDENTIAL_BYTES + 1]),
        Err(IssuanceProtocolError::InvalidMetadata)
    );
}
