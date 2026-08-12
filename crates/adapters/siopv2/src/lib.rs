// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    net::IpAddr,
    sync::{Arc, Mutex, MutexGuard},
};

use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{Signature as Ed25519Signature, Verifier as _, VerifyingKey as Ed25519Key};
use oxid_identity_application::{
    DidLifecyclePortError, DidOperationConfirmation, DidOperationError, DidRecordQuery,
    DidRecordRepositoryError, GetDidRecordUseCase, SignDidPayloadCommand, SignDidPayloadUseCase,
};
use oxid_platform_ports::ClockPort;
use oxid_protocol_application::{
    AuthenticateSelfIssuedPortFuture, PrepareSelfIssuedAuthenticationPortFuture,
    PrepareSelfIssuedAuthenticationRequest, PreparedSelfIssuedAuthentication,
    ProtocolSelfIssuedAuthenticationRequest, SelfIssuedAuthenticationProtocolPort,
    SelfIssuedIdentityProofPort, SelfIssuedProofError, SelfIssuedProofFuture,
    SelfIssuedProofRequest, SelfIssuedProtocolError,
};
use oxid_protocol_domain::{SelfIssuedAuthenticationId, SelfIssuedAuthenticationPreview};
use p256::ecdsa::{Signature as P256Signature, VerifyingKey as P256Key};
use serde::{Deserialize, Deserializer, de};
use serde_json::{Map, Number, Value, json};
use url::Url;
use zeroize::Zeroizing;

pub const STANDALONE_VERIFIER: &str = "http://127.0.0.1:32192/verifier";
const STANDALONE_REQUEST_URI: &str = "http://127.0.0.1:32192/verifier/request";
const STANDALONE_RESPONSE_URI: &str = "http://127.0.0.1:32192/verifier/response";
const STANDALONE_PURPOSE: &str = "Authenticate with the selected DID.";
const MAX_JSON_DEPTH: usize = 16;
const MAX_PROTOCOL_BYTES: usize = 64 * 1_024;
const MAX_ENDPOINT_CHARACTERS: usize = 2_048;
const MAX_SECRET_CHARACTERS: usize = 4_096;
const TOKEN_LIFETIME_SECONDS: u64 = 300;

/// Public request-by-reference URI for the deterministic in-process verifier.
#[must_use]
pub fn standalone_self_issued_request() -> String {
    let mut url = Url::parse("openid4vp://authorize").expect("constant request URL is valid");
    url.query_pairs_mut()
        .append_pair("client_id", STANDALONE_VERIFIER)
        .append_pair("request_uri", STANDALONE_REQUEST_URI);
    url.into()
}

struct PreparedRequest {
    profile_id: String,
    client_id: String,
    response_uri: String,
    nonce: Zeroizing<String>,
    state: Zeroizing<String>,
    expires_at_seconds: u64,
}

/// Draft-13 SIOPv2 self-issued authentication backed by an in-process,
/// deterministic relying party. It intentionally does not implement a
/// `vp_token` or claim that draft SIOP is a production profile.
pub struct StandaloneSiopV2Verifier {
    proof: Arc<dyn SelfIssuedIdentityProofPort>,
    get_did: Arc<dyn GetDidRecordUseCase>,
    clock: Arc<dyn ClockPort>,
    sessions: Mutex<BTreeMap<String, PreparedRequest>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl StandaloneSiopV2Verifier {
    #[must_use]
    pub fn new(
        proof: Arc<dyn SelfIssuedIdentityProofPort>,
        get_did: Arc<dyn GetDidRecordUseCase>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            proof,
            get_did,
            clock,
            sessions: Mutex::new(BTreeMap::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    fn sessions(
        &self,
    ) -> Result<MutexGuard<'_, BTreeMap<String, PreparedRequest>>, SelfIssuedProtocolError> {
        self.sessions
            .lock()
            .map_err(|_| SelfIssuedProtocolError::Unavailable)
    }

    fn next_id(&self) -> Result<SelfIssuedAuthenticationId, SelfIssuedProtocolError> {
        let value = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        SelfIssuedAuthenticationId::parse(format!("authentication_{value:016x}"))
            .map_err(|_| SelfIssuedProtocolError::Unavailable)
    }
}

impl SelfIssuedAuthenticationProtocolPort for StandaloneSiopV2Verifier {
    fn prepare<'a>(
        &'a self,
        request: PrepareSelfIssuedAuthenticationRequest,
    ) -> PrepareSelfIssuedAuthenticationPortFuture<'a> {
        Box::pin(async move {
            let invocation = parse_invocation(&request.request)?;
            if invocation.client_id != STANDALONE_VERIFIER
                || invocation.request_uri != STANDALONE_REQUEST_URI
            {
                return Err(SelfIssuedProtocolError::InvalidVerifier);
            }
            let id = self.next_id()?;
            let now = self
                .clock
                .now()
                .map_err(|_| SelfIssuedProtocolError::Unavailable)?
                .value()
                / 1_000;
            let nonce = Zeroizing::new(format!("oxid-siop-nonce-{}", id.as_str()));
            let state = Zeroizing::new(format!("oxid-siop-state-{}", id.as_str()));
            let request_object = json!({
                "client_id": STANDALONE_VERIFIER,
                "response_type": "id_token",
                "response_mode": "direct_post",
                "response_uri": STANDALONE_RESPONSE_URI,
                "scope": "openid",
                "nonce": nonce.as_str(),
                "state": state.as_str(),
                "iat": now,
                "exp": now + TOKEN_LIFETIME_SECONDS,
                "purpose": STANDALONE_PURPOSE
            });
            let parsed = parse_request_object(request_object.to_string().as_bytes(), now)?;
            if parsed.client_id != invocation.client_id {
                return Err(SelfIssuedProtocolError::InvalidVerifier);
            }
            let preview =
                SelfIssuedAuthenticationPreview::new(parsed.client_id.clone(), parsed.purpose)
                    .map_err(|_| SelfIssuedProtocolError::InvalidRequest)?;
            let prepared = PreparedRequest {
                profile_id: request.profile_id.as_str().to_owned(),
                client_id: parsed.client_id,
                response_uri: parsed.response_uri,
                nonce,
                state,
                expires_at_seconds: parsed.exp,
            };
            if self
                .sessions()?
                .insert(id.as_str().to_owned(), prepared)
                .is_some()
            {
                return Err(SelfIssuedProtocolError::Unavailable);
            }
            Ok(PreparedSelfIssuedAuthentication { id, preview })
        })
    }

    fn authenticate<'a>(
        &'a self,
        request: ProtocolSelfIssuedAuthenticationRequest,
    ) -> AuthenticateSelfIssuedPortFuture<'a> {
        Box::pin(async move {
            let prepared = self
                .sessions()?
                .remove(request.authentication_id.as_str())
                .ok_or(SelfIssuedProtocolError::InvalidRequest)?;
            if prepared.profile_id != request.profile_id.as_str() {
                return Err(SelfIssuedProtocolError::InvalidRequest);
            }
            let now = self
                .clock
                .now()
                .map_err(|_| SelfIssuedProtocolError::Unavailable)?
                .value()
                / 1_000;
            if now >= prepared.expires_at_seconds {
                return Err(SelfIssuedProtocolError::RequestExpired);
            }
            validate_exact_response_uri(&prepared.response_uri)?;
            let id_token = self
                .proof
                .create(SelfIssuedProofRequest {
                    profile_id: request.profile_id.clone(),
                    holder_did: request.holder_did.clone(),
                    method_id: request.method_id.clone(),
                    audience: prepared.client_id.clone(),
                    nonce: prepared.nonce.to_string(),
                    issued_at_seconds: now,
                    expires_at_seconds: now + TOKEN_LIFETIME_SECONDS,
                })
                .await
                .map_err(map_proof_error)?;
            let response = json!({
                "id_token": id_token,
                "state": prepared.state.as_str()
            });
            validate_response(
                response.to_string().as_bytes(),
                prepared.state.as_str(),
                &prepared.client_id,
                prepared.nonce.as_str(),
                &request.holder_did,
                &request.method_id,
                now,
            )?;
            verify_id_token_signature(
                self.get_did.as_ref(),
                &request.profile_id,
                &request.holder_did,
                &request.method_id,
                response
                    .get("id_token")
                    .and_then(Value::as_str)
                    .ok_or(SelfIssuedProtocolError::InvalidProof)?,
            )
        })
    }

    fn discard(
        &self,
        authentication_id: &SelfIssuedAuthenticationId,
    ) -> Result<(), SelfIssuedProtocolError> {
        self.sessions()?
            .remove(authentication_id.as_str())
            .map(|_| ())
            .ok_or(SelfIssuedProtocolError::InvalidRequest)
    }
}

fn map_proof_error(error: SelfIssuedProofError) -> SelfIssuedProtocolError {
    match error {
        SelfIssuedProofError::Unavailable => SelfIssuedProtocolError::ProtectionUnavailable,
        SelfIssuedProofError::WalletLocked => SelfIssuedProtocolError::WalletLocked,
        SelfIssuedProofError::DidNotFound
        | SelfIssuedProofError::MethodNotFound
        | SelfIssuedProofError::MethodNotAuthorized
        | SelfIssuedProofError::UnsupportedAlgorithm
        | SelfIssuedProofError::Rejected => SelfIssuedProtocolError::InvalidProof,
    }
}

struct ParsedInvocation {
    client_id: String,
    request_uri: String,
}

fn parse_invocation(input: &str) -> Result<ParsedInvocation, SelfIssuedProtocolError> {
    let url = Url::parse(input).map_err(|_| SelfIssuedProtocolError::InvalidRequest)?;
    if url.scheme() != "openid4vp"
        || url.host_str() != Some("authorize")
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(SelfIssuedProtocolError::InvalidRequest);
    }
    let pairs = url.query_pairs().collect::<Vec<_>>();
    if pairs.len() != 2 {
        return Err(SelfIssuedProtocolError::InvalidRequest);
    }
    let mut values = BTreeMap::new();
    for (name, value) in pairs {
        if !matches!(name.as_ref(), "client_id" | "request_uri")
            || values
                .insert(name.into_owned(), value.into_owned())
                .is_some()
        {
            return Err(SelfIssuedProtocolError::InvalidRequest);
        }
    }
    let client_id = values
        .remove("client_id")
        .ok_or(SelfIssuedProtocolError::InvalidRequest)?;
    let request_uri = values
        .remove("request_uri")
        .ok_or(SelfIssuedProtocolError::InvalidRequest)?;
    validate_endpoint(&client_id, EndpointPolicy::StandaloneLoopback)?;
    let request = validate_endpoint(&request_uri, EndpointPolicy::StandaloneLoopback)?;
    if request.path() != "/verifier/request" {
        return Err(SelfIssuedProtocolError::InvalidRequest);
    }
    Ok(ParsedInvocation {
        client_id,
        request_uri,
    })
}

struct ParsedRequestObject {
    client_id: String,
    response_uri: String,
    purpose: String,
    exp: u64,
}

fn parse_request_object(
    bytes: &[u8],
    now: u64,
) -> Result<ParsedRequestObject, SelfIssuedProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(SelfIssuedProtocolError::InvalidRequest)?;
    if object.len() != 10
        || required_string(object, "response_type", 32)? != "id_token"
        || required_string(object, "response_mode", 32)? != "direct_post"
        || required_string(object, "scope", 32)? != "openid"
        || object.contains_key("vp_token")
        || object.contains_key("presentation_definition")
        || object.contains_key("dcql_query")
        || object.contains_key("redirect_uri")
    {
        return Err(SelfIssuedProtocolError::UnsupportedRequest);
    }
    let client_id = required_string(object, "client_id", MAX_ENDPOINT_CHARACTERS)?;
    validate_endpoint(&client_id, EndpointPolicy::StandaloneLoopback)?;
    let response_uri = required_string(object, "response_uri", MAX_ENDPOINT_CHARACTERS)?;
    validate_exact_response_uri(&response_uri)?;
    required_string(object, "nonce", MAX_SECRET_CHARACTERS)?;
    required_string(object, "state", MAX_SECRET_CHARACTERS)?;
    let purpose = required_string(object, "purpose", 512)?;
    let iat = required_u64(object, "iat")?;
    let exp = required_u64(object, "exp")?;
    if iat > now.saturating_add(60)
        || exp <= now
        || exp < iat
        || exp.saturating_sub(iat) > TOKEN_LIFETIME_SECONDS
    {
        return Err(SelfIssuedProtocolError::RequestExpired);
    }
    Ok(ParsedRequestObject {
        client_id,
        response_uri,
        purpose,
        exp,
    })
}

fn validate_response(
    bytes: &[u8],
    expected_state: &str,
    expected_audience: &str,
    expected_nonce: &str,
    expected_subject: &str,
    expected_method: &str,
    now: u64,
) -> Result<(), SelfIssuedProtocolError> {
    let value = parse_strict_json(bytes).map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let object = value
        .as_object()
        .filter(|object| object.len() == 2)
        .ok_or(SelfIssuedProtocolError::InvalidProof)?;
    if required_string(object, "state", MAX_SECRET_CHARACTERS)
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?
        != expected_state
    {
        return Err(SelfIssuedProtocolError::VerifierRejected);
    }
    let token = required_string(object, "id_token", MAX_PROTOCOL_BYTES)
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    validate_id_token(
        &token,
        expected_audience,
        expected_nonce,
        expected_subject,
        expected_method,
        now,
    )
}

fn validate_id_token(
    token: &str,
    expected_audience: &str,
    expected_nonce: &str,
    expected_subject: &str,
    expected_method: &str,
    now: u64,
) -> Result<(), SelfIssuedProtocolError> {
    if token.len() > MAX_PROTOCOL_BYTES {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }
    let parts = token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }
    let header = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let claims = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let signature = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    if signature.len() != 64 {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }
    let header = parse_strict_json(&header).map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let header = header
        .as_object()
        .filter(|object| object.len() == 3)
        .ok_or(SelfIssuedProtocolError::InvalidProof)?;
    let algorithm =
        required_string(header, "alg", 16).map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    if !matches!(algorithm.as_str(), "EdDSA" | "ES256")
        || required_string(header, "typ", 16).map_err(|_| SelfIssuedProtocolError::InvalidProof)?
            != "JWT"
        || required_string(header, "kid", MAX_ENDPOINT_CHARACTERS)
            .map_err(|_| SelfIssuedProtocolError::InvalidProof)?
            != expected_method
    {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }
    let claims = parse_strict_json(&claims).map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let claims = claims
        .as_object()
        .filter(|object| object.len() == 6)
        .ok_or(SelfIssuedProtocolError::InvalidProof)?;
    let issuer = required_string(claims, "iss", MAX_ENDPOINT_CHARACTERS)
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let subject = required_string(claims, "sub", MAX_ENDPOINT_CHARACTERS)
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let issued_at =
        required_u64(claims, "iat").map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let expires_at =
        required_u64(claims, "exp").map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    if issuer != expected_subject
        || subject != expected_subject
        || issuer != subject
        || required_string(claims, "aud", MAX_ENDPOINT_CHARACTERS)
            .map_err(|_| SelfIssuedProtocolError::InvalidProof)?
            != expected_audience
        || required_string(claims, "nonce", MAX_SECRET_CHARACTERS)
            .map_err(|_| SelfIssuedProtocolError::InvalidProof)?
            != expected_nonce
        || issued_at > now.saturating_add(60)
        || expires_at <= now
        || expires_at <= issued_at
        || expires_at.saturating_sub(issued_at) > TOKEN_LIFETIME_SECONDS
    {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }
    Ok(())
}

fn verify_id_token_signature(
    get_did: &dyn GetDidRecordUseCase,
    profile_id: &oxid_protocol_domain::ProtocolProfileId,
    holder_did: &str,
    method_id: &str,
    token: &str,
) -> Result<(), SelfIssuedProtocolError> {
    let record = get_did
        .execute(DidRecordQuery {
            profile_id: profile_id.as_str().to_owned(),
            did: holder_did.to_owned(),
        })
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    if record.document_metadata.deactivated == Some(true) {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }
    let method = record
        .document
        .verification_methods
        .iter()
        .find(|method| method.id == method_id)
        .ok_or(SelfIssuedProtocolError::InvalidProof)?;
    if method.controller != holder_did
        || !record.document.relationships.iter().any(|relationship| {
            relationship.relationship == "authentication"
                && relationship.method_ids.iter().any(|id| id == method_id)
        })
    {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }
    let parts = token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }
    let header = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let header = parse_strict_json(&header).map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let header = header
        .as_object()
        .ok_or(SelfIssuedProtocolError::InvalidProof)?;
    let algorithm =
        required_string(header, "alg", 16).map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let signature = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| SelfIssuedProtocolError::InvalidProof)?;
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let valid = match (
        algorithm.as_str(),
        method.public_key_jwk.key_type.as_str(),
        method.public_key_jwk.curve.as_str(),
        method.public_key_jwk.y.as_deref(),
    ) {
        ("EdDSA", "OKP", "Ed25519", None) => verify_ed25519(
            &method.public_key_jwk.x,
            signing_input.as_bytes(),
            &signature,
        ),
        ("ES256", "EC", "P-256", Some(y)) => verify_p256(
            &method.public_key_jwk.x,
            y,
            signing_input.as_bytes(),
            &signature,
        ),
        _ => false,
    };
    valid
        .then_some(())
        .ok_or(SelfIssuedProtocolError::InvalidProof)
}

fn verify_ed25519(x: &str, message: &[u8], signature: &[u8]) -> bool {
    let bytes = general_purpose::URL_SAFE_NO_PAD.decode(x).ok();
    let key = bytes
        .as_deref()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .and_then(|bytes| Ed25519Key::from_bytes(&bytes).ok());
    let signature = Ed25519Signature::from_slice(signature).ok();
    matches!((key, signature), (Some(key), Some(signature)) if key.verify(message, &signature).is_ok())
}

fn verify_p256(x: &str, y: &str, message: &[u8], signature: &[u8]) -> bool {
    let x = general_purpose::URL_SAFE_NO_PAD.decode(x).ok();
    let y = general_purpose::URL_SAFE_NO_PAD.decode(y).ok();
    let key = match (x, y) {
        (Some(x), Some(y)) if x.len() == 32 && y.len() == 32 => {
            let mut point = Vec::with_capacity(65);
            point.push(4);
            point.extend_from_slice(&x);
            point.extend_from_slice(&y);
            P256Key::from_sec1_bytes(&point).ok()
        }
        _ => None,
    };
    let signature = P256Signature::from_slice(signature).ok();
    matches!((key, signature), (Some(key), Some(signature)) if key.verify(message, &signature).is_ok())
}

fn validate_exact_response_uri(value: &str) -> Result<(), SelfIssuedProtocolError> {
    let endpoint = validate_endpoint(value, EndpointPolicy::StandaloneLoopback)?;
    if endpoint.as_str() != STANDALONE_RESPONSE_URI {
        return Err(SelfIssuedProtocolError::InvalidVerifier);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum EndpointPolicy {
    HttpsOnly,
    StandaloneLoopback,
}

pub fn validate_production_endpoint(value: &str) -> Result<(), SelfIssuedProtocolError> {
    validate_endpoint(value, EndpointPolicy::HttpsOnly).map(|_| ())
}

fn validate_endpoint(value: &str, policy: EndpointPolicy) -> Result<Url, SelfIssuedProtocolError> {
    if value.chars().count() > MAX_ENDPOINT_CHARACTERS {
        return Err(SelfIssuedProtocolError::InvalidVerifier);
    }
    let url = Url::parse(value).map_err(|_| SelfIssuedProtocolError::InvalidVerifier)?;
    if url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
    {
        return Err(SelfIssuedProtocolError::InvalidVerifier);
    }
    match url.scheme() {
        "https" => {}
        "http" if matches!(policy, EndpointPolicy::StandaloneLoopback) => {
            let host = url
                .host_str()
                .ok_or(SelfIssuedProtocolError::InvalidVerifier)?;
            let loopback = host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<IpAddr>()
                    .is_ok_and(|address| address.is_loopback());
            if !loopback {
                return Err(SelfIssuedProtocolError::InvalidVerifier);
            }
        }
        _ => return Err(SelfIssuedProtocolError::InvalidVerifier),
    }
    Ok(url)
}

fn parse_strict_json(bytes: &[u8]) -> Result<Value, SelfIssuedProtocolError> {
    if bytes.is_empty() || bytes.len() > MAX_PROTOCOL_BYTES {
        return Err(SelfIssuedProtocolError::InvalidRequest);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = StrictValue::deserialize(&mut deserializer)
        .map_err(|_| SelfIssuedProtocolError::InvalidRequest)?
        .0;
    deserializer
        .end()
        .map_err(|_| SelfIssuedProtocolError::InvalidRequest)?;
    if json_depth(&value, 1) > MAX_JSON_DEPTH {
        return Err(SelfIssuedProtocolError::InvalidRequest);
    }
    Ok(value)
}

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictVisitor).map(Self)
    }
}

struct StrictVisitor;

impl<'de> de::Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a bounded JSON value without duplicate object members")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_string(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        StrictValue::deserialize(deserializer).map(|value| value.0)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictValue>()? {
            values.push(value.0);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        let mut values = Map::new();
        let mut names = BTreeSet::new();
        while let Some(name) = map.next_key::<String>()? {
            if !names.insert(name.clone()) {
                return Err(de::Error::custom("duplicate JSON object member"));
            }
            let value = map.next_value::<StrictValue>()?;
            values.insert(name, value.0);
        }
        Ok(Value::Object(values))
    }
}

fn json_depth(value: &Value, depth: usize) -> usize {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| json_depth(value, depth + 1))
            .max()
            .unwrap_or(depth),
        Value::Object(values) => values
            .values()
            .map(|value| json_depth(value, depth + 1))
            .max()
            .unwrap_or(depth),
        _ => depth,
    }
}

fn required_string(
    object: &Map<String, Value>,
    name: &str,
    max: usize,
) -> Result<String, SelfIssuedProtocolError> {
    let value = object
        .get(name)
        .and_then(Value::as_str)
        .ok_or(SelfIssuedProtocolError::InvalidRequest)?;
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(SelfIssuedProtocolError::InvalidRequest);
    }
    Ok(value.to_owned())
}

fn required_u64(object: &Map<String, Value>, name: &str) -> Result<u64, SelfIssuedProtocolError> {
    object
        .get(name)
        .and_then(Value::as_u64)
        .ok_or(SelfIssuedProtocolError::InvalidRequest)
}

/// Builds the draft SIOPv2 self-issued ID Token only after application-level
/// consent, using the existing profile-scoped DID lifecycle and opaque key use.
pub struct DidSelfIssuedIdentityProof {
    get_did: Arc<dyn GetDidRecordUseCase>,
    sign: Arc<dyn SignDidPayloadUseCase>,
}

impl DidSelfIssuedIdentityProof {
    #[must_use]
    pub fn new(
        get_did: Arc<dyn GetDidRecordUseCase>,
        sign: Arc<dyn SignDidPayloadUseCase>,
    ) -> Self {
        Self { get_did, sign }
    }
}

impl SelfIssuedIdentityProofPort for DidSelfIssuedIdentityProof {
    fn create<'a>(&'a self, request: SelfIssuedProofRequest) -> SelfIssuedProofFuture<'a> {
        Box::pin(async move {
            let record = self
                .get_did
                .execute(DidRecordQuery {
                    profile_id: request.profile_id.as_str().to_owned(),
                    did: request.holder_did.clone(),
                })
                .map_err(map_get_did_error)?;
            if record.document_metadata.deactivated == Some(true) {
                return Err(SelfIssuedProofError::Rejected);
            }
            let method = record
                .document
                .verification_methods
                .iter()
                .find(|method| method.id == request.method_id)
                .ok_or(SelfIssuedProofError::MethodNotFound)?;
            if method.controller != request.holder_did
                || !record.document.relationships.iter().any(|relationship| {
                    relationship.relationship == "authentication"
                        && relationship.method_ids.contains(&request.method_id)
                })
            {
                return Err(SelfIssuedProofError::MethodNotAuthorized);
            }
            let algorithm = match method.public_key_jwk.curve.as_str() {
                "Ed25519" => "EdDSA",
                "P-256" => "ES256",
                _ => return Err(SelfIssuedProofError::UnsupportedAlgorithm),
            };
            if request.expires_at_seconds <= request.issued_at_seconds
                || request
                    .expires_at_seconds
                    .saturating_sub(request.issued_at_seconds)
                    > TOKEN_LIFETIME_SECONDS
            {
                return Err(SelfIssuedProofError::Rejected);
            }
            let header = json!({
                "alg": algorithm,
                "kid": request.method_id,
                "typ": "JWT"
            });
            let payload = json!({
                "iss": request.holder_did,
                "sub": request.holder_did,
                "aud": request.audience,
                "nonce": request.nonce,
                "iat": request.issued_at_seconds,
                "exp": request.expires_at_seconds
            });
            let protected = general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&header).map_err(|_| SelfIssuedProofError::Rejected)?);
            let claims = general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&payload).map_err(|_| SelfIssuedProofError::Rejected)?);
            let signing_input = format!("{protected}.{claims}");
            let signature = self
                .sign
                .execute(SignDidPayloadCommand {
                    profile_id: request.profile_id.as_str().to_owned(),
                    did: request.holder_did,
                    method_id: request.method_id,
                    payload: signing_input.as_bytes().to_vec(),
                    confirmation: DidOperationConfirmation {
                        title: "Authenticate with DID".to_owned(),
                        summary: "Bind the accepted self-issued authentication to this verifier."
                            .to_owned(),
                        confirmed: true,
                    },
                })
                .map_err(map_sign_error)?;
            if signature.signature_bytes.len() != 64
                || !matches!(
                    (signature.algorithm.as_str(), algorithm),
                    ("ed25519", "EdDSA") | ("p256", "ES256")
                )
            {
                return Err(SelfIssuedProofError::Rejected);
            }
            Ok(format!(
                "{signing_input}.{}",
                general_purpose::URL_SAFE_NO_PAD.encode(signature.signature_bytes)
            ))
        })
    }
}

fn map_get_did_error(error: DidOperationError) -> SelfIssuedProofError {
    match error {
        DidOperationError::Persistence(DidRecordRepositoryError::NotFound) => {
            SelfIssuedProofError::DidNotFound
        }
        DidOperationError::InvalidProfileIdentifier(_) | DidOperationError::InvalidDid(_) => {
            SelfIssuedProofError::Rejected
        }
        _ => SelfIssuedProofError::Unavailable,
    }
}

fn map_sign_error(error: DidOperationError) -> SelfIssuedProofError {
    match error {
        DidOperationError::Lifecycle(DidLifecyclePortError::Locked) => {
            SelfIssuedProofError::WalletLocked
        }
        DidOperationError::Lifecycle(DidLifecyclePortError::NotFound) => {
            SelfIssuedProofError::MethodNotFound
        }
        DidOperationError::Lifecycle(DidLifecyclePortError::UnsupportedAlgorithm) => {
            SelfIssuedProofError::UnsupportedAlgorithm
        }
        DidOperationError::Lifecycle(DidLifecyclePortError::Unavailable)
        | DidOperationError::Lifecycle(DidLifecyclePortError::ProtectionUnavailable) => {
            SelfIssuedProofError::Unavailable
        }
        _ => SelfIssuedProofError::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_adapter_did_midnight::{StandaloneDidLifecycle, StandaloneDidResolver};
    use oxid_adapter_platform_system::{OsRandom, SystemClock};
    use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
    use oxid_adapter_storage_memory::{
        InMemoryDidRecordRepository, InMemoryWalletProfileRepository,
    };
    use oxid_identity_application::{
        CreateDidCommand, CreateDidUseCase, DidOperationConfirmation, DidRecordRepository,
        DidService, DidUpdate, UpdateDidCommand, UpdateDidUseCase,
    };
    use oxid_identity_domain::VerificationRelationship;
    use oxid_wallet_application::{
        CreateWalletProfileCommand, CreateWalletProfileService, CreateWalletProfileUseCase,
        InitializeWalletSecurityUseCase, WalletKeyOperationPort, WalletProtectionService,
    };

    struct ProofFixture {
        proof: Arc<dyn SelfIssuedIdentityProofPort>,
        get_did: Arc<dyn GetDidRecordUseCase>,
        clock: Arc<dyn ClockPort>,
        profile_id: String,
        did: String,
        method: String,
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        std::task::Waker::noop().wake_by_ref();
        let mut future = std::pin::pin!(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
            std::thread::yield_now();
        }
    }

    fn proof_fixture() -> ProofFixture {
        proof_fixture_with_curve("Ed25519")
    }

    fn proof_fixture_with_curve(curve: &str) -> ProofFixture {
        let clock = Arc::new(SystemClock);
        let random = Arc::new(OsRandom);
        let profiles = Arc::new(InMemoryWalletProfileRepository::new());
        let created = CreateWalletProfileService::new(
            Arc::clone(&profiles),
            Arc::clone(&clock),
            Arc::clone(&random),
        )
        .execute(CreateWalletProfileCommand {
            display_name: "SIOP".to_owned(),
        })
        .expect("profile should be created");
        let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
        let protection = WalletProtectionService::new(Arc::clone(&security));
        InitializeWalletSecurityUseCase::execute(
            &protection,
            oxid_wallet_application::WalletProfileSecurityCommand {
                profile_id: created.id.clone(),
            },
        )
        .expect("security should initialize");
        let keys: Arc<dyn WalletKeyOperationPort> = security;
        let repository: Arc<dyn DidRecordRepository> = Arc::new(InMemoryDidRecordRepository::new());
        let identity = Arc::new(DidService::from_ports(
            repository,
            Arc::new(StandaloneDidResolver),
            Arc::new(StandaloneDidLifecycle::new(keys)),
        ));
        let mut did = CreateDidUseCase::execute(
            identity.as_ref(),
            CreateDidCommand {
                profile_id: created.id.clone(),
                network: "undeployed".to_owned(),
            },
        )
        .expect("DID should be created");
        let method = did
            .document
            .verification_methods
            .iter()
            .find(|method| method.public_key_jwk.curve == curve)
            .map(|method| method.id.clone())
            .expect("selected curve should exist");
        if !did
            .document
            .relationships
            .iter()
            .find(|relationship| relationship.relationship == "authentication")
            .is_some_and(|relationship| relationship.method_ids.contains(&method))
        {
            did = UpdateDidUseCase::execute(
                identity.as_ref(),
                UpdateDidCommand {
                    profile_id: created.id.clone(),
                    did: did.document.id.clone(),
                    operation: DidUpdate::AddVerificationRelationship {
                        relationship: VerificationRelationship::Authentication,
                        method_id: method.clone(),
                    },
                    confirmation: DidOperationConfirmation {
                        title: "Authorize authentication method".to_owned(),
                        summary: "Test both supported self-issued proof curves.".to_owned(),
                        confirmed: true,
                    },
                },
            )
            .expect("selected method should become an authentication method");
        }
        let get: Arc<dyn GetDidRecordUseCase> = identity.clone();
        let sign: Arc<dyn SignDidPayloadUseCase> = identity;
        let proof = Arc::new(DidSelfIssuedIdentityProof::new(Arc::clone(&get), sign));
        ProofFixture {
            proof,
            get_did: get,
            clock,
            profile_id: created.id,
            did: did.document.id,
            method,
        }
    }

    #[test]
    fn invocation_is_strict_and_production_endpoints_require_https() {
        let parsed = parse_invocation(&standalone_self_issued_request())
            .expect("standalone invocation should parse");
        assert_eq!(parsed.client_id, STANDALONE_VERIFIER);
        assert_eq!(parsed.request_uri, STANDALONE_REQUEST_URI);
        assert_eq!(
            parse_invocation("openid4vp://authorize?client_id=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier&request_uri=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier%2Frequest&request_uri=https%3A%2F%2Fattacker.example").err(),
            Some(SelfIssuedProtocolError::InvalidRequest)
        );
        assert!(validate_production_endpoint("https://verifier.example/response").is_ok());
        assert!(validate_production_endpoint(STANDALONE_RESPONSE_URI).is_err());
    }

    #[test]
    fn duplicate_request_members_and_vp_mode_fail_closed() {
        let duplicate = br#"{"client_id":"http://127.0.0.1:32192/verifier","client_id":"https://attacker.example","response_type":"id_token","response_mode":"direct_post","response_uri":"http://127.0.0.1:32192/verifier/response","scope":"openid","nonce":"n","state":"s","iat":1,"exp":2,"purpose":"Authenticate"}"#;
        assert_eq!(
            parse_request_object(duplicate, 1).err(),
            Some(SelfIssuedProtocolError::InvalidRequest)
        );
        let vp = json!({
            "client_id": STANDALONE_VERIFIER,
            "response_type": "vp_token",
            "response_mode": "direct_post",
            "response_uri": STANDALONE_RESPONSE_URI,
            "scope": "openid",
            "nonce": "n",
            "state": "s",
            "iat": 1,
            "exp": 2,
            "purpose": "Present a credential"
        });
        assert_eq!(
            parse_request_object(vp.to_string().as_bytes(), 1).err(),
            Some(SelfIssuedProtocolError::UnsupportedRequest)
        );
    }

    #[test]
    fn managed_did_authenticates_once_and_replay_is_rejected() {
        let ProofFixture {
            proof,
            get_did,
            clock,
            profile_id,
            did,
            method,
        } = proof_fixture();
        let adapter = StandaloneSiopV2Verifier::new(proof, get_did, clock);
        let profile = oxid_protocol_domain::ProtocolProfileId::parse(profile_id)
            .expect("fixture profile id is valid");
        let prepared = block_on(adapter.prepare(PrepareSelfIssuedAuthenticationRequest {
            profile_id: profile.clone(),
            request: standalone_self_issued_request(),
        }))
        .expect("request should prepare");
        block_on(
            adapter.authenticate(ProtocolSelfIssuedAuthenticationRequest {
                profile_id: profile.clone(),
                authentication_id: prepared.id.clone(),
                holder_did: did,
                method_id: method,
            }),
        )
        .expect("managed DID should authenticate");
        assert_eq!(
            block_on(
                adapter.authenticate(ProtocolSelfIssuedAuthenticationRequest {
                    profile_id: profile,
                    authentication_id: prepared.id,
                    holder_did: "did:midnight:undeployed:replay".to_owned(),
                    method_id: "did:midnight:undeployed:replay#auth".to_owned(),
                })
            ),
            Err(SelfIssuedProtocolError::InvalidRequest)
        );
    }

    #[test]
    fn managed_p256_did_authenticates_with_es256() {
        let ProofFixture {
            proof,
            get_did,
            clock,
            profile_id,
            did,
            method,
        } = proof_fixture_with_curve("P-256");
        let adapter = StandaloneSiopV2Verifier::new(proof, get_did, clock);
        let profile = oxid_protocol_domain::ProtocolProfileId::parse(profile_id)
            .expect("fixture profile id is valid");
        let prepared = block_on(adapter.prepare(PrepareSelfIssuedAuthenticationRequest {
            profile_id: profile.clone(),
            request: standalone_self_issued_request(),
        }))
        .expect("request should prepare");
        block_on(
            adapter.authenticate(ProtocolSelfIssuedAuthenticationRequest {
                profile_id: profile,
                authentication_id: prepared.id,
                holder_did: did,
                method_id: method,
            }),
        )
        .expect("managed P-256 DID should authenticate");
    }

    #[test]
    fn malformed_and_exactly_expired_tokens_are_invalid_proofs() {
        let now = 100;
        let duplicate_header = br#"{"alg":"EdDSA","alg":"ES256","kid":"did:midnight:undeployed:test#auth","typ":"JWT"}"#;
        let claims = json!({
            "iss": "did:midnight:undeployed:test",
            "sub": "did:midnight:undeployed:test",
            "aud": STANDALONE_VERIFIER,
            "nonce": "nonce",
            "iat": 99,
            "exp": 101
        });
        let malformed = format!(
            "{}.{}.{}",
            general_purpose::URL_SAFE_NO_PAD.encode(duplicate_header),
            general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string()),
            general_purpose::URL_SAFE_NO_PAD.encode([0_u8; 64])
        );
        assert_eq!(
            validate_id_token(
                &malformed,
                STANDALONE_VERIFIER,
                "nonce",
                "did:midnight:undeployed:test",
                "did:midnight:undeployed:test#auth",
                now,
            ),
            Err(SelfIssuedProtocolError::InvalidProof)
        );

        let header = json!({
            "alg": "EdDSA",
            "kid": "did:midnight:undeployed:test#auth",
            "typ": "JWT"
        });
        let expired_claims = json!({
            "iss": "did:midnight:undeployed:test",
            "sub": "did:midnight:undeployed:test",
            "aud": STANDALONE_VERIFIER,
            "nonce": "nonce",
            "iat": 99,
            "exp": now
        });
        let expired = format!(
            "{}.{}.{}",
            general_purpose::URL_SAFE_NO_PAD.encode(header.to_string()),
            general_purpose::URL_SAFE_NO_PAD.encode(expired_claims.to_string()),
            general_purpose::URL_SAFE_NO_PAD.encode([0_u8; 64])
        );
        assert_eq!(
            validate_id_token(
                &expired,
                STANDALONE_VERIFIER,
                "nonce",
                "did:midnight:undeployed:test",
                "did:midnight:undeployed:test#auth",
                now,
            ),
            Err(SelfIssuedProtocolError::InvalidProof)
        );
    }

    #[test]
    fn verifier_rejects_a_tampered_signature() {
        let ProofFixture {
            proof,
            get_did,
            clock,
            profile_id,
            did,
            method,
        } = proof_fixture();
        let profile = oxid_protocol_domain::ProtocolProfileId::parse(profile_id)
            .expect("fixture profile id is valid");
        let now = clock.now().expect("clock").value() / 1_000;
        let token = block_on(proof.create(SelfIssuedProofRequest {
            profile_id: profile.clone(),
            holder_did: did.clone(),
            method_id: method.clone(),
            audience: STANDALONE_VERIFIER.to_owned(),
            nonce: "nonce".to_owned(),
            issued_at_seconds: now,
            expires_at_seconds: now + TOKEN_LIFETIME_SECONDS,
        }))
        .expect("proof should be created");
        validate_id_token(&token, STANDALONE_VERIFIER, "nonce", &did, &method, now)
            .expect("token claims should validate");
        let mut tampered = token.into_bytes();
        let signature_start = tampered
            .iter()
            .rposition(|byte| *byte == b'.')
            .map(|position| position + 1)
            .expect("token has a signature");
        tampered[signature_start] = if tampered[signature_start] == b'A' {
            b'B'
        } else {
            b'A'
        };
        let tampered = String::from_utf8(tampered).expect("ASCII token");
        assert_eq!(
            verify_id_token_signature(get_did.as_ref(), &profile, &did, &method, &tampered,),
            Err(SelfIssuedProtocolError::InvalidProof)
        );
    }
}
