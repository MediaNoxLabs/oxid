// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    net::IpAddr,
    sync::{Arc, Mutex, MutexGuard},
};

use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{Signature as Ed25519Signature, Verifier as _, VerifyingKey as Ed25519Key};
use oxid_credential_application::{
    CredentialDetachedProofInput, CredentialDisclosurePortError, CredentialOperationError,
    CredentialPrivateMaterialInput, CredentialRepositoryError, CredentialVerificationError,
    ImportVerifiedCredentialCommand, ImportVerifiedCredentialUseCase,
};
use oxid_identity_application::{
    DidLifecyclePortError, DidOperationConfirmation, DidOperationError, DidRecordQuery,
    DidRecordRepositoryError, GetDidRecordUseCase, SignDidPayloadCommand, SignDidPayloadUseCase,
};
use oxid_platform_ports::ClockPort;
use oxid_protocol_application::{
    CredentialHolderProofPort, CredentialIssuanceProtocolPort, HolderProofError, HolderProofFuture,
    HolderProofRequest, IssuanceProtocolError, IssueCredentialPortFuture, IssuedCredentialBytes,
    IssuedCredentialSinkError, IssuedCredentialSinkPort, PrepareIssuancePortFuture,
    PrepareIssuanceRequest, PreparedCredentialOffer, ProtocolIssueRequest,
    StoreIssuedCredentialFuture, StoreIssuedCredentialRequest, StoredCredential,
};
use oxid_protocol_domain::{CredentialIssuanceId, CredentialOfferPreview};
use p256::ecdsa::{Signature as P256Signature, VerifyingKey as P256Key};
use serde::{Deserialize, Deserializer, de};
use serde_json::{Map, Number, Value, json};
use url::Url;
use zeroize::Zeroizing;

pub const STANDALONE_CREDENTIAL_ISSUER: &str = "http://127.0.0.1:32191/issuer";
pub const STANDALONE_AUTHORIZATION_SERVER: &str = "http://127.0.0.1:32191/auth";
pub const STANDALONE_CONFIGURATION_ID: &str = "oxid_digital_passport";
const STANDALONE_PRE_AUTHORIZED_CODE: &str = "oxid-standalone-pre-authorized";
const PRE_AUTHORIZED_GRANT: &str = "urn:ietf:params:oauth:grant-type:pre-authorized_code";
const MAX_JSON_DEPTH: usize = 16;
const MAX_PROTOCOL_RESPONSE_BYTES: usize = 1024 * 1024 + 32 * 1024;
const MAX_SECRET_CHARACTERS: usize = 4_096;
const MAX_ENDPOINT_CHARACTERS: usize = 2_048;

/// Public, non-secret offer used by the deterministic standalone issuer.
#[must_use]
pub fn standalone_credential_offer() -> String {
    let offer = json!({
        "credential_issuer": STANDALONE_CREDENTIAL_ISSUER,
        "credential_configuration_ids": [STANDALONE_CONFIGURATION_ID],
        "grants": {
            PRE_AUTHORIZED_GRANT: {
                "pre-authorized_code": STANDALONE_PRE_AUTHORIZED_CODE
            }
        }
    });
    let mut url = Url::parse("openid-credential-offer://").expect("constant offer URL is valid");
    url.query_pairs_mut()
        .append_pair("credential_offer", &offer.to_string());
    url.into()
}

struct PreparedSecret {
    profile_id: String,
    issuer: String,
    configuration_id: String,
    token_endpoint: String,
    nonce_endpoint: String,
    credential_endpoint: String,
    pre_authorized_code: Zeroizing<String>,
}

/// Exact OpenID4VCI 1.0 Final pre-authorized flow backed by an in-process,
/// deterministic issuer. No HTTP listener, access token, nonce, or offer code
/// crosses the incoming-adapter boundary.
pub struct StandaloneOid4vciIssuer {
    proof: Arc<dyn CredentialHolderProofPort>,
    get_did: Arc<dyn GetDidRecordUseCase>,
    clock: Arc<dyn ClockPort>,
    sessions: Mutex<BTreeMap<String, PreparedSecret>>,
    next_id: std::sync::atomic::AtomicU64,
    signed_credential: Vec<u8>,
    detached_proof: Option<Vec<u8>>,
    private_material: Option<Vec<u8>>,
}

impl StandaloneOid4vciIssuer {
    #[must_use]
    pub fn new(
        proof: Arc<dyn CredentialHolderProofPort>,
        get_did: Arc<dyn GetDidRecordUseCase>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        let signed_credential = general_purpose::STANDARD
            .decode(
                include_str!("../../../../fixtures/credentials/standalone-midnight-phase1.b64")
                    .trim(),
            )
            .expect("checked-in standalone credential fixture must be valid base64");
        Self::with_credential_fixture(proof, get_did, clock, signed_credential, None, None)
    }

    /// Builds the standalone protocol around a composition-provided public
    /// signed fixture and optional protected format material. Keeping fixture
    /// selection in composition avoids an adapter-to-adapter dependency.
    #[must_use]
    pub fn with_credential_fixture(
        proof: Arc<dyn CredentialHolderProofPort>,
        get_did: Arc<dyn GetDidRecordUseCase>,
        clock: Arc<dyn ClockPort>,
        signed_credential: Vec<u8>,
        detached_proof: Option<Vec<u8>>,
        private_material: Option<Vec<u8>>,
    ) -> Self {
        Self {
            proof,
            get_did,
            clock,
            sessions: Mutex::new(BTreeMap::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
            signed_credential,
            detached_proof,
            private_material,
        }
    }

    fn sessions(
        &self,
    ) -> Result<MutexGuard<'_, BTreeMap<String, PreparedSecret>>, IssuanceProtocolError> {
        self.sessions
            .lock()
            .map_err(|_| IssuanceProtocolError::Unavailable)
    }

    fn next_id(&self) -> Result<CredentialIssuanceId, IssuanceProtocolError> {
        let value = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        CredentialIssuanceId::parse(format!("issuance_{value:016x}"))
            .map_err(|_| IssuanceProtocolError::Unavailable)
    }
}

impl CredentialIssuanceProtocolPort for StandaloneOid4vciIssuer {
    fn prepare<'a>(&'a self, request: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
        Box::pin(async move {
            let offer = parse_offer(&request.offer)?;
            let issuer_metadata = standalone_issuer_metadata()?;
            if offer.issuer != issuer_metadata.issuer {
                return Err(IssuanceProtocolError::InvalidMetadata);
            }
            if !offer
                .configuration_ids
                .iter()
                .all(|id| issuer_metadata.configurations.contains_key(id))
            {
                return Err(IssuanceProtocolError::UnsupportedCredential);
            }
            let authorization_server = offer
                .authorization_server
                .as_deref()
                .map_or_else(
                    || {
                        issuer_metadata
                            .authorization_servers
                            .first()
                            .map(String::as_str)
                    },
                    Some,
                )
                .ok_or(IssuanceProtocolError::InvalidMetadata)?;
            if !issuer_metadata
                .authorization_servers
                .iter()
                .any(|value| value == authorization_server)
            {
                return Err(IssuanceProtocolError::InvalidMetadata);
            }
            let authorization_metadata = standalone_authorization_metadata()?;
            if authorization_metadata.issuer != authorization_server
                || !authorization_metadata
                    .grant_types
                    .iter()
                    .any(|grant| grant == PRE_AUTHORIZED_GRANT)
                || !authorization_metadata.anonymous_pre_authorized
            {
                return Err(IssuanceProtocolError::InvalidMetadata);
            }
            let id = self.next_id()?;
            let display_names = offer
                .configuration_ids
                .iter()
                .map(|configuration_id| {
                    issuer_metadata
                        .configurations
                        .get(configuration_id)
                        .map(|configuration| configuration.display_name.clone())
                        .ok_or(IssuanceProtocolError::UnsupportedCredential)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let preview = CredentialOfferPreview::new(
                offer.issuer.clone(),
                offer.configuration_ids.clone(),
                display_names,
            )
            .map_err(|_| IssuanceProtocolError::InvalidOffer)?;
            let configuration_id = offer
                .configuration_ids
                .first()
                .cloned()
                .ok_or(IssuanceProtocolError::InvalidOffer)?;
            let secret = PreparedSecret {
                profile_id: request.profile_id.as_str().to_owned(),
                issuer: offer.issuer,
                configuration_id,
                token_endpoint: authorization_metadata.token_endpoint,
                nonce_endpoint: issuer_metadata.nonce_endpoint,
                credential_endpoint: issuer_metadata.credential_endpoint,
                pre_authorized_code: Zeroizing::new(offer.pre_authorized_code),
            };
            if self
                .sessions()?
                .insert(id.as_str().to_owned(), secret)
                .is_some()
            {
                return Err(IssuanceProtocolError::Unavailable);
            }
            Ok(PreparedCredentialOffer { id, preview })
        })
    }

    fn issue<'a>(&'a self, request: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a> {
        Box::pin(async move {
            let secret = self
                .sessions()?
                .remove(request.issuance_id.as_str())
                .ok_or(IssuanceProtocolError::InvalidOffer)?;
            if secret.profile_id != request.profile_id.as_str() {
                return Err(IssuanceProtocolError::InvalidOffer);
            }
            validate_expected_endpoint(&secret.token_endpoint, "/auth/token")?;
            if secret.pre_authorized_code.as_str() != STANDALONE_PRE_AUTHORIZED_CODE {
                return Err(IssuanceProtocolError::IssuerRejected);
            }
            let access_token =
                Zeroizing::new(format!("oxid-access-{}", request.issuance_id.as_str()));
            validate_expected_endpoint(&secret.nonce_endpoint, "/issuer/nonce")?;
            let nonce = Zeroizing::new(format!("oxid-nonce-{}", request.issuance_id.as_str()));
            let proof_profile = request.profile_id.clone();
            let proof_did = request.holder_did.clone();
            let proof_method = request.method_id.clone();
            let proof = self
                .proof
                .create(HolderProofRequest {
                    profile_id: proof_profile.clone(),
                    holder_did: proof_did.clone(),
                    method_id: proof_method.clone(),
                    audience: secret.issuer,
                    nonce: nonce.to_string(),
                })
                .await
                .map_err(map_holder_proof_error)?;
            let credential_request = json!({
                "credential_configuration_id": secret.configuration_id,
                "proofs": {"jwt": [proof]}
            });
            validate_credential_request(
                credential_request.to_string().as_bytes(),
                STANDALONE_CONFIGURATION_ID,
                &proof_method,
                nonce.as_str(),
                self.clock
                    .now()
                    .map_err(|_| IssuanceProtocolError::Unavailable)?
                    .value()
                    / 1_000,
            )?;
            verify_key_proof_signature(
                self.get_did.as_ref(),
                &proof_profile,
                &proof_did,
                &proof_method,
                &proof,
            )?;
            validate_expected_endpoint(&secret.credential_endpoint, "/issuer/credential")?;
            if access_token.is_empty() || secret.configuration_id != STANDALONE_CONFIGURATION_ID {
                return Err(IssuanceProtocolError::IssuerRejected);
            }
            if self.signed_credential.is_empty() {
                return Err(IssuanceProtocolError::InvalidCredentialResponse);
            }
            let response = json!({
                "credentials": [{
                    "credential": general_purpose::URL_SAFE_NO_PAD.encode(&self.signed_credential)
                }]
            });
            parse_credential_response(response.to_string().as_bytes()).map(|signed_bytes| {
                IssuedCredentialBytes {
                    signed_bytes,
                    detached_proof: self.detached_proof.clone(),
                    private_material: self.private_material.clone(),
                }
            })
        })
    }

    fn discard(&self, issuance_id: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError> {
        self.sessions()?
            .remove(issuance_id.as_str())
            .map(|_| ())
            .ok_or(IssuanceProtocolError::InvalidOffer)
    }
}

fn map_holder_proof_error(error: HolderProofError) -> IssuanceProtocolError {
    match error {
        HolderProofError::Unavailable => IssuanceProtocolError::ProtectionUnavailable,
        HolderProofError::WalletLocked => IssuanceProtocolError::WalletLocked,
        HolderProofError::DidNotFound
        | HolderProofError::MethodNotFound
        | HolderProofError::MethodNotAuthorized
        | HolderProofError::UnsupportedAlgorithm
        | HolderProofError::Rejected => IssuanceProtocolError::InvalidProof,
    }
}

fn validate_expected_endpoint(value: &str, path: &str) -> Result<(), IssuanceProtocolError> {
    let endpoint = validate_endpoint(value, EndpointPolicy::StandaloneLoopback)?;
    if endpoint.path() != path {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    Ok(())
}

struct ParsedOffer {
    issuer: String,
    configuration_ids: Vec<String>,
    authorization_server: Option<String>,
    pre_authorized_code: String,
}

fn parse_offer(input: &str) -> Result<ParsedOffer, IssuanceProtocolError> {
    let url = Url::parse(input).map_err(|_| IssuanceProtocolError::InvalidOffer)?;
    if url.scheme() != "openid-credential-offer"
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(IssuanceProtocolError::InvalidOffer);
    }
    let pairs = url.query_pairs().collect::<Vec<_>>();
    if pairs.len() != 1 || pairs[0].0 != "credential_offer" {
        return Err(
            if pairs.iter().any(|(name, _)| name == "credential_offer_uri") {
                IssuanceProtocolError::UnsupportedOffer
            } else {
                IssuanceProtocolError::InvalidOffer
            },
        );
    }
    let bytes = pairs[0].1.as_bytes();
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidOffer)?;
    let issuer = required_string(object, "credential_issuer", MAX_ENDPOINT_CHARACTERS)?;
    validate_endpoint(&issuer, EndpointPolicy::StandaloneLoopback)?;
    let configuration_ids =
        required_unique_strings(object, "credential_configuration_ids", 16, 256)?;
    let grants = required_object(object, "grants")?;
    let grant = grants
        .get(PRE_AUTHORIZED_GRANT)
        .and_then(Value::as_object)
        .ok_or(IssuanceProtocolError::UnsupportedOffer)?;
    if grant.contains_key("tx_code") {
        return Err(IssuanceProtocolError::TransactionCodeRequired);
    }
    let pre_authorized_code = required_string(grant, "pre-authorized_code", MAX_SECRET_CHARACTERS)?;
    let authorization_server = grant
        .get("authorization_server")
        .map(|value| {
            let value = value.as_str().ok_or(IssuanceProtocolError::InvalidOffer)?;
            validate_endpoint(value, EndpointPolicy::StandaloneLoopback)?;
            Ok(value.to_owned())
        })
        .transpose()?;
    Ok(ParsedOffer {
        issuer,
        configuration_ids,
        authorization_server,
        pre_authorized_code,
    })
}

#[derive(Clone)]
struct CredentialConfiguration {
    display_name: String,
}

struct IssuerMetadata {
    issuer: String,
    authorization_servers: Vec<String>,
    credential_endpoint: String,
    nonce_endpoint: String,
    configurations: BTreeMap<String, CredentialConfiguration>,
}

fn standalone_issuer_metadata() -> Result<IssuerMetadata, IssuanceProtocolError> {
    let bytes = br#"{
        "credential_issuer":"http://127.0.0.1:32191/issuer",
        "authorization_servers":["http://127.0.0.1:32191/auth"],
        "credential_endpoint":"http://127.0.0.1:32191/issuer/credential",
        "nonce_endpoint":"http://127.0.0.1:32191/issuer/nonce",
        "credential_configurations_supported":{
            "oxid_digital_passport":{
                "format":"midnight_cbor_phase1",
                "proof_types_supported":{"jwt":{"proof_signing_alg_values_supported":["EdDSA","ES256"]}},
                "credential_metadata":{"display":[{"name":"Digital Passport"}]}
            }
        }
    }"#;
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    let issuer = required_string(object, "credential_issuer", MAX_ENDPOINT_CHARACTERS)?;
    validate_endpoint(&issuer, EndpointPolicy::StandaloneLoopback)?;
    let authorization_servers =
        required_unique_strings(object, "authorization_servers", 4, MAX_ENDPOINT_CHARACTERS)?;
    for server in &authorization_servers {
        validate_endpoint(server, EndpointPolicy::StandaloneLoopback)?;
    }
    let credential_endpoint =
        required_string(object, "credential_endpoint", MAX_ENDPOINT_CHARACTERS)?;
    validate_endpoint(&credential_endpoint, EndpointPolicy::StandaloneLoopback)?;
    let nonce_endpoint = required_string(object, "nonce_endpoint", MAX_ENDPOINT_CHARACTERS)?;
    validate_endpoint(&nonce_endpoint, EndpointPolicy::StandaloneLoopback)?;
    let raw_configurations = required_object(object, "credential_configurations_supported")?;
    if raw_configurations.is_empty() || raw_configurations.len() > 16 {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let mut configurations = BTreeMap::new();
    for (id, value) in raw_configurations {
        let configuration = value
            .as_object()
            .ok_or(IssuanceProtocolError::InvalidMetadata)?;
        if required_string(configuration, "format", 128)? != "midnight_cbor_phase1" {
            return Err(IssuanceProtocolError::UnsupportedCredential);
        }
        let proof_types = required_object(configuration, "proof_types_supported")?;
        let jwt = proof_types
            .get("jwt")
            .and_then(Value::as_object)
            .ok_or(IssuanceProtocolError::InvalidMetadata)?;
        let algorithms = required_unique_strings(jwt, "proof_signing_alg_values_supported", 8, 32)?;
        if !algorithms.iter().any(|value| value == "EdDSA")
            && !algorithms.iter().any(|value| value == "ES256")
        {
            return Err(IssuanceProtocolError::UnsupportedCredential);
        }
        let metadata = required_object(configuration, "credential_metadata")?;
        let display = metadata
            .get("display")
            .and_then(Value::as_array)
            .filter(|values| !values.is_empty())
            .ok_or(IssuanceProtocolError::InvalidMetadata)?;
        let display_name = display[0]
            .as_object()
            .ok_or(IssuanceProtocolError::InvalidMetadata)
            .and_then(|value| required_string(value, "name", 256))?;
        configurations.insert(id.clone(), CredentialConfiguration { display_name });
    }
    Ok(IssuerMetadata {
        issuer,
        authorization_servers,
        credential_endpoint,
        nonce_endpoint,
        configurations,
    })
}

struct AuthorizationMetadata {
    issuer: String,
    token_endpoint: String,
    grant_types: Vec<String>,
    anonymous_pre_authorized: bool,
}

fn standalone_authorization_metadata() -> Result<AuthorizationMetadata, IssuanceProtocolError> {
    let bytes = br#"{
        "issuer":"http://127.0.0.1:32191/auth",
        "token_endpoint":"http://127.0.0.1:32191/auth/token",
        "grant_types_supported":["urn:ietf:params:oauth:grant-type:pre-authorized_code"],
        "pre-authorized_grant_anonymous_access_supported":true
    }"#;
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    let issuer = required_string(object, "issuer", MAX_ENDPOINT_CHARACTERS)?;
    validate_endpoint(&issuer, EndpointPolicy::StandaloneLoopback)?;
    let token_endpoint = required_string(object, "token_endpoint", MAX_ENDPOINT_CHARACTERS)?;
    validate_endpoint(&token_endpoint, EndpointPolicy::StandaloneLoopback)?;
    let grant_types = required_unique_strings(object, "grant_types_supported", 16, 256)?;
    let anonymous_pre_authorized = object
        .get("pre-authorized_grant_anonymous_access_supported")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(AuthorizationMetadata {
        issuer,
        token_endpoint,
        grant_types,
        anonymous_pre_authorized,
    })
}

fn parse_credential_response(bytes: &[u8]) -> Result<Vec<u8>, IssuanceProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    if object.contains_key("transaction_id") {
        return Err(IssuanceProtocolError::UnsupportedOffer);
    }
    let credentials = object
        .get("credentials")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 1)
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    let encoded = credentials[0]
        .as_object()
        .and_then(|value| value.get("credential"))
        .and_then(Value::as_str)
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| IssuanceProtocolError::InvalidCredentialResponse)
}

fn validate_credential_request(
    bytes: &[u8],
    expected_configuration: &str,
    expected_method: &str,
    expected_nonce: &str,
    current_time_seconds: u64,
) -> Result<(), IssuanceProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidProof)?;
    if required_string(object, "credential_configuration_id", 256)? != expected_configuration
        || object.contains_key("credential_identifier")
    {
        return Err(IssuanceProtocolError::InvalidProof);
    }
    let proofs = object
        .get("proofs")
        .and_then(Value::as_object)
        .ok_or(IssuanceProtocolError::InvalidProof)?;
    let jwt = proofs
        .get("jwt")
        .and_then(Value::as_array)
        .filter(|proofs| proofs.len() == 1)
        .and_then(|proofs| proofs[0].as_str())
        .ok_or(IssuanceProtocolError::InvalidProof)?;
    validate_key_proof(jwt, expected_method, expected_nonce, current_time_seconds)
}

fn validate_key_proof(
    proof: &str,
    expected_method: &str,
    expected_nonce: &str,
    current_time_seconds: u64,
) -> Result<(), IssuanceProtocolError> {
    if proof.len() > 64 * 1024 {
        return Err(IssuanceProtocolError::InvalidProof);
    }
    let parts = proof.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(IssuanceProtocolError::InvalidProof);
    }
    let header = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|_| IssuanceProtocolError::InvalidProof)?;
    let payload = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| IssuanceProtocolError::InvalidProof)?;
    let signature = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| IssuanceProtocolError::InvalidProof)?;
    if signature.len() != 64 {
        return Err(IssuanceProtocolError::InvalidProof);
    }
    let header = parse_strict_json(&header)?;
    let header = header
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidProof)?;
    let algorithm = required_string(header, "alg", 32)?;
    if !matches!(algorithm.as_str(), "EdDSA" | "ES256")
        || required_string(header, "typ", 64)? != "openid4vci-proof+jwt"
        || required_string(header, "kid", MAX_ENDPOINT_CHARACTERS)? != expected_method
        || header.contains_key("jwk")
        || header.contains_key("x5c")
    {
        return Err(IssuanceProtocolError::InvalidProof);
    }
    let payload = parse_strict_json(&payload)?;
    let payload = payload
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidProof)?;
    let issued_at = payload
        .get("iat")
        .and_then(Value::as_u64)
        .ok_or(IssuanceProtocolError::InvalidProof)?;
    if required_string(payload, "aud", MAX_ENDPOINT_CHARACTERS)? != STANDALONE_CREDENTIAL_ISSUER
        || required_string(payload, "nonce", MAX_SECRET_CHARACTERS)? != expected_nonce
        || issued_at > current_time_seconds.saturating_add(60)
        || current_time_seconds.saturating_sub(issued_at) > 300
        || payload.contains_key("iss")
    {
        return Err(IssuanceProtocolError::InvalidProof);
    }
    Ok(())
}

fn verify_key_proof_signature(
    get_did: &dyn GetDidRecordUseCase,
    profile_id: &oxid_protocol_domain::ProtocolProfileId,
    holder_did: &str,
    method_id: &str,
    proof: &str,
) -> Result<(), IssuanceProtocolError> {
    let record = get_did
        .execute(DidRecordQuery {
            profile_id: profile_id.as_str().to_owned(),
            did: holder_did.to_owned(),
        })
        .map_err(map_get_did_error)
        .map_err(map_holder_proof_error)?;
    if record.document_metadata.deactivated == Some(true) {
        return Err(IssuanceProtocolError::InvalidProof);
    }
    let method = record
        .document
        .verification_methods
        .iter()
        .find(|method| method.id == method_id)
        .filter(|method| method.controller == holder_did)
        .ok_or(IssuanceProtocolError::InvalidProof)?;
    if !record.document.relationships.iter().any(|relationship| {
        relationship.relationship == "authentication"
            && relationship
                .method_ids
                .iter()
                .any(|value| value == method_id)
    }) {
        return Err(IssuanceProtocolError::InvalidProof);
    }

    let parts = proof.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(IssuanceProtocolError::InvalidProof);
    }
    let signature = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| IssuanceProtocolError::InvalidProof)?;
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let verified = match (
        method.public_key_jwk.key_type.as_str(),
        method.public_key_jwk.curve.as_str(),
    ) {
        ("OKP", "Ed25519") => verify_ed25519(
            &method.public_key_jwk.x,
            signing_input.as_bytes(),
            &signature,
        ),
        ("EC", "P-256") => method.public_key_jwk.y.as_deref().is_some_and(|y| {
            verify_p256(
                &method.public_key_jwk.x,
                y,
                signing_input.as_bytes(),
                &signature,
            )
        }),
        _ => false,
    };
    if verified {
        Ok(())
    } else {
        Err(IssuanceProtocolError::InvalidProof)
    }
}

fn verify_ed25519(x: &str, message: &[u8], signature: &[u8]) -> bool {
    let key = general_purpose::URL_SAFE_NO_PAD
        .decode(x)
        .ok()
        .and_then(|value| <[u8; 32]>::try_from(value).ok())
        .and_then(|value| Ed25519Key::from_bytes(&value).ok());
    let signature = Ed25519Signature::from_slice(signature).ok();
    matches!((key, signature), (Some(key), Some(signature)) if key.verify(message, &signature).is_ok())
}

fn verify_p256(x: &str, y: &str, message: &[u8], signature: &[u8]) -> bool {
    let (Ok(x), Ok(y)) = (
        general_purpose::URL_SAFE_NO_PAD.decode(x),
        general_purpose::URL_SAFE_NO_PAD.decode(y),
    ) else {
        return false;
    };
    if x.len() != 32 || y.len() != 32 {
        return false;
    }
    let mut point = Vec::with_capacity(65);
    point.push(4);
    point.extend_from_slice(&x);
    point.extend_from_slice(&y);
    let key = P256Key::from_sec1_bytes(&point).ok();
    let signature = P256Signature::from_slice(signature).ok();
    matches!((key, signature), (Some(key), Some(signature)) if key.verify(message, &signature).is_ok())
}

#[derive(Clone, Copy)]
enum EndpointPolicy {
    HttpsOnly,
    StandaloneLoopback,
}

/// Validates an externally discovered production endpoint. Production
/// integrations are HTTPS-only; loopback HTTP is reserved for this adapter's
/// deterministic in-process standalone fixture.
pub fn validate_production_endpoint(value: &str) -> Result<(), IssuanceProtocolError> {
    validate_endpoint(value, EndpointPolicy::HttpsOnly).map(|_| ())
}

fn validate_endpoint(value: &str, policy: EndpointPolicy) -> Result<Url, IssuanceProtocolError> {
    if value.is_empty() || value.len() > MAX_ENDPOINT_CHARACTERS {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let url = Url::parse(value).map_err(|_| IssuanceProtocolError::InvalidMetadata)?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    match (url.scheme(), policy) {
        ("https", _) => {}
        ("http", EndpointPolicy::StandaloneLoopback) if host_is_loopback(&url) => {}
        _ => return Err(IssuanceProtocolError::InvalidMetadata),
    }
    Ok(url)
}

fn host_is_loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}

fn required_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, IssuanceProtocolError> {
    object
        .get(key)
        .and_then(Value::as_object)
        .ok_or(IssuanceProtocolError::InvalidMetadata)
}

fn required_string(
    object: &Map<String, Value>,
    key: &str,
    max: usize,
) -> Result<String, IssuanceProtocolError> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    Ok(value.to_owned())
}

fn required_unique_strings(
    object: &Map<String, Value>,
    key: &str,
    max_count: usize,
    max_length: usize,
) -> Result<Vec<String>, IssuanceProtocolError> {
    let values = object
        .get(key)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty() && values.len() <= max_count)
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    let mut unique = BTreeSet::new();
    values
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .ok_or(IssuanceProtocolError::InvalidMetadata)?;
            if value.is_empty()
                || value.chars().count() > max_length
                || value.chars().any(char::is_control)
                || !unique.insert(value)
            {
                return Err(IssuanceProtocolError::InvalidMetadata);
            }
            Ok(value.to_owned())
        })
        .collect()
}

fn parse_strict_json(bytes: &[u8]) -> Result<Value, IssuanceProtocolError> {
    if bytes.is_empty() || bytes.len() > MAX_PROTOCOL_RESPONSE_BYTES {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = StrictValue::deserialize(&mut deserializer)
        .map_err(|_| IssuanceProtocolError::InvalidMetadata)?
        .0;
    deserializer
        .end()
        .map_err(|_| IssuanceProtocolError::InvalidMetadata)?;
    if json_depth(&value, 0) > MAX_JSON_DEPTH {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    Ok(value)
}

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

struct StrictValueVisitor;

impl<'de> de::Visitor<'de> for StrictValueVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value without duplicate object members")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Number(Number::from(value))))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Number(Number::from(value))))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .map(StrictValue)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_string(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictValue>()? {
            values.push(value.0);
        }
        Ok(StrictValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        let mut values = Map::new();
        while let Some((key, value)) = map.next_entry::<String, StrictValue>()? {
            if values.insert(key, value.0).is_some() {
                return Err(de::Error::custom("duplicate JSON object member"));
            }
        }
        Ok(StrictValue(Value::Object(values)))
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

/// Bridges explicit issuance consent to the existing profile-scoped DID
/// lifecycle without exposing opaque key handles to the protocol adapter.
pub struct DidCredentialHolderProof {
    get_did: Arc<dyn GetDidRecordUseCase>,
    sign: Arc<dyn SignDidPayloadUseCase>,
    clock: Arc<dyn ClockPort>,
}

impl DidCredentialHolderProof {
    #[must_use]
    pub fn new(
        get_did: Arc<dyn GetDidRecordUseCase>,
        sign: Arc<dyn SignDidPayloadUseCase>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            get_did,
            sign,
            clock,
        }
    }
}

impl CredentialHolderProofPort for DidCredentialHolderProof {
    fn create<'a>(&'a self, request: HolderProofRequest) -> HolderProofFuture<'a> {
        Box::pin(async move {
            let record = self
                .get_did
                .execute(DidRecordQuery {
                    profile_id: request.profile_id.as_str().to_owned(),
                    did: request.holder_did.clone(),
                })
                .map_err(map_get_did_error)?;
            if record.document_metadata.deactivated == Some(true) {
                return Err(HolderProofError::Rejected);
            }
            let method = record
                .document
                .verification_methods
                .iter()
                .find(|method| method.id == request.method_id)
                .ok_or(HolderProofError::MethodNotFound)?;
            if method.controller != request.holder_did
                || !record.document.relationships.iter().any(|relationship| {
                    relationship.relationship == "authentication"
                        && relationship.method_ids.contains(&request.method_id)
                })
            {
                return Err(HolderProofError::MethodNotAuthorized);
            }
            let algorithm = match method.public_key_jwk.curve.as_str() {
                "Ed25519" => "EdDSA",
                "P-256" => "ES256",
                _ => return Err(HolderProofError::UnsupportedAlgorithm),
            };
            let issued_at = self
                .clock
                .now()
                .map_err(|_| HolderProofError::Unavailable)?
                .value()
                / 1_000;
            let header = json!({
                "alg": algorithm,
                "kid": request.method_id,
                "typ": "openid4vci-proof+jwt"
            });
            let payload = json!({
                "aud": request.audience,
                "iat": issued_at,
                "nonce": request.nonce
            });
            let protected = general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&header).map_err(|_| HolderProofError::Rejected)?);
            let claims = general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&payload).map_err(|_| HolderProofError::Rejected)?);
            let signing_input = format!("{protected}.{claims}");
            let signature = self
                .sign
                .execute(SignDidPayloadCommand {
                    profile_id: request.profile_id.as_str().to_owned(),
                    did: request.holder_did,
                    method_id: request.method_id,
                    payload: signing_input.as_bytes().to_vec(),
                    confirmation: DidOperationConfirmation {
                        title: "Issue credential".to_owned(),
                        summary: "Bind the accepted credential issuance to this DID method."
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
                return Err(HolderProofError::Rejected);
            }
            Ok(format!(
                "{signing_input}.{}",
                general_purpose::URL_SAFE_NO_PAD.encode(signature.signature_bytes)
            ))
        })
    }
}

fn map_get_did_error(error: DidOperationError) -> HolderProofError {
    match error {
        DidOperationError::Persistence(DidRecordRepositoryError::NotFound) => {
            HolderProofError::DidNotFound
        }
        DidOperationError::InvalidProfileIdentifier(_) | DidOperationError::InvalidDid(_) => {
            HolderProofError::Rejected
        }
        _ => HolderProofError::Unavailable,
    }
}

fn map_sign_error(error: DidOperationError) -> HolderProofError {
    match error {
        DidOperationError::Lifecycle(DidLifecyclePortError::Locked) => {
            HolderProofError::WalletLocked
        }
        DidOperationError::Lifecycle(DidLifecyclePortError::NotFound) => {
            HolderProofError::MethodNotFound
        }
        DidOperationError::Lifecycle(DidLifecyclePortError::UnsupportedAlgorithm) => {
            HolderProofError::UnsupportedAlgorithm
        }
        DidOperationError::Lifecycle(DidLifecyclePortError::Unavailable)
        | DidOperationError::Lifecycle(DidLifecyclePortError::ProtectionUnavailable) => {
            HolderProofError::Unavailable
        }
        _ => HolderProofError::Rejected,
    }
}

/// Bridges successful protocol output to the strict verifier plus protected
/// repository. Non-valid reports never reach persistence.
pub struct VerifiedCredentialSink {
    importer: Arc<dyn ImportVerifiedCredentialUseCase>,
}

impl VerifiedCredentialSink {
    #[must_use]
    pub const fn new(importer: Arc<dyn ImportVerifiedCredentialUseCase>) -> Self {
        Self { importer }
    }
}

impl IssuedCredentialSinkPort for VerifiedCredentialSink {
    fn store_verified<'a>(
        &'a self,
        request: StoreIssuedCredentialRequest,
    ) -> StoreIssuedCredentialFuture<'a> {
        Box::pin(async move {
            let detached_proof = request
                .detached_proof
                .map(CredentialDetachedProofInput::new)
                .transpose()
                .map_err(|_| IssuedCredentialSinkError::InvalidCredential)?;
            let private_material = request
                .private_material
                .map(CredentialPrivateMaterialInput::new)
                .transpose()
                .map_err(|_| IssuedCredentialSinkError::InvalidCredential)?;
            self.importer
                .execute(ImportVerifiedCredentialCommand {
                    profile_id: request.profile_id.as_str().to_owned(),
                    signed_bytes: request.signed_bytes,
                    detached_proof,
                    private_material,
                })
                .await
                .map(|credential| StoredCredential {
                    credential_id: credential.id,
                })
                .map_err(map_import_error)
        })
    }
}

fn map_import_error(error: CredentialOperationError) -> IssuedCredentialSinkError {
    match error {
        CredentialOperationError::VerificationNotValid => {
            IssuedCredentialSinkError::VerificationFailed
        }
        CredentialOperationError::Verification(CredentialVerificationError::InvalidCredential)
        | CredentialOperationError::Verification(CredentialVerificationError::UnsupportedFormat)
        | CredentialOperationError::Disclosure(
            CredentialDisclosurePortError::InvalidPrivateMaterial
            | CredentialDisclosurePortError::UnsupportedCredential,
        )
        | CredentialOperationError::Domain(_) => IssuedCredentialSinkError::InvalidCredential,
        CredentialOperationError::Persistence(CredentialRepositoryError::Unavailable) => {
            IssuedCredentialSinkError::Unavailable
        }
        CredentialOperationError::Persistence(_) => IssuedCredentialSinkError::PersistenceFailed,
        _ => IssuedCredentialSinkError::Unavailable,
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
        CreateDidCommand, CreateDidUseCase, DidRecordRepository, DidService,
    };
    use oxid_protocol_application::{CredentialIssuanceProtocolPort, PrepareIssuanceRequest};
    use oxid_protocol_domain::ProtocolProfileId;
    use oxid_wallet_application::{
        CreateWalletProfileCommand, CreateWalletProfileService, CreateWalletProfileUseCase,
        InitializeWalletSecurityUseCase, WalletKeyOperationPort, WalletProtectionService,
    };

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
            std::thread::yield_now();
        }
    }

    struct ProofFixture {
        proof: Arc<DidCredentialHolderProof>,
        get_did: Arc<dyn GetDidRecordUseCase>,
        clock: Arc<dyn ClockPort>,
        profile_id: String,
        did: String,
        method: String,
    }

    fn proof_fixture() -> ProofFixture {
        let clock = Arc::new(SystemClock);
        let random = Arc::new(OsRandom);
        let profiles = Arc::new(InMemoryWalletProfileRepository::new());
        let created = CreateWalletProfileService::new(
            Arc::clone(&profiles),
            Arc::clone(&clock),
            Arc::clone(&random),
        )
        .execute(CreateWalletProfileCommand {
            display_name: "OID4VCI".to_owned(),
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
        let did = CreateDidUseCase::execute(
            identity.as_ref(),
            CreateDidCommand {
                profile_id: created.id.clone(),
                network: "undeployed".to_owned(),
            },
        )
        .expect("DID should be created");
        let method = did
            .document
            .relationships
            .iter()
            .find(|relationship| relationship.relationship == "authentication")
            .and_then(|relationship| relationship.method_ids.first())
            .cloned()
            .expect("authentication method should exist");
        let get: Arc<dyn GetDidRecordUseCase> = identity.clone();
        let sign: Arc<dyn SignDidPayloadUseCase> = identity;
        let proof_clock: Arc<dyn ClockPort> = clock.clone();
        ProofFixture {
            proof: Arc::new(DidCredentialHolderProof::new(Arc::clone(&get), sign, clock)),
            get_did: get,
            clock: proof_clock,
            profile_id: created.id,
            did: did.document.id,
            method,
        }
    }

    #[test]
    fn standalone_offer_is_final_shape_and_unknown_extensions_are_ignored() {
        let offer = standalone_credential_offer();
        let parsed = parse_offer(&offer).expect("standalone offer should parse");
        assert_eq!(parsed.issuer, STANDALONE_CREDENTIAL_ISSUER);
        assert_eq!(parsed.configuration_ids, [STANDALONE_CONFIGURATION_ID]);

        let extended = json!({
            "credential_issuer": STANDALONE_CREDENTIAL_ISSUER,
            "credential_configuration_ids": [STANDALONE_CONFIGURATION_ID],
            "extension_parameter": {"ignored": true},
            "grants": {PRE_AUTHORIZED_GRANT: {
                "pre-authorized_code": STANDALONE_PRE_AUTHORIZED_CODE,
                "extension_grant_parameter": "ignored"
            }}
        });
        let mut url = Url::parse("openid-credential-offer://").expect("valid URL");
        url.query_pairs_mut()
            .append_pair("credential_offer", &extended.to_string());
        parse_offer(url.as_str()).expect("unknown extension parameters must be ignored");
    }

    #[test]
    fn duplicate_members_by_reference_and_transaction_codes_fail_closed() {
        let duplicate = r#"{"credential_issuer":"http://127.0.0.1:32191/issuer","credential_issuer":"https://attacker.example","credential_configuration_ids":["oxid_digital_passport"],"grants":{"urn:ietf:params:oauth:grant-type:pre-authorized_code":{"pre-authorized_code":"code"}}}"#;
        let mut url = Url::parse("openid-credential-offer://").expect("valid URL");
        url.query_pairs_mut()
            .append_pair("credential_offer", duplicate);
        assert_eq!(
            parse_offer(url.as_str()).err(),
            Some(IssuanceProtocolError::InvalidMetadata)
        );
        assert_eq!(
            parse_offer("openid-credential-offer://?credential_offer_uri=https%3A%2F%2Fissuer.example%2Foffer").err(),
            Some(IssuanceProtocolError::UnsupportedOffer)
        );
        let transaction_code = json!({
            "credential_issuer": STANDALONE_CREDENTIAL_ISSUER,
            "credential_configuration_ids": [STANDALONE_CONFIGURATION_ID],
            "grants": {PRE_AUTHORIZED_GRANT: {
                "pre-authorized_code": STANDALONE_PRE_AUTHORIZED_CODE,
                "tx_code": {"input_mode": "numeric"}
            }}
        });
        let mut with_code = Url::parse("openid-credential-offer://").expect("valid URL");
        with_code
            .query_pairs_mut()
            .append_pair("credential_offer", &transaction_code.to_string());
        assert_eq!(
            parse_offer(with_code.as_str()).err(),
            Some(IssuanceProtocolError::TransactionCodeRequired)
        );
    }

    #[test]
    fn endpoint_policy_rejects_remote_http_and_credentials() {
        assert!(validate_endpoint("https://issuer.example", EndpointPolicy::HttpsOnly).is_ok());
        assert!(
            validate_endpoint("http://127.0.0.1:32191", EndpointPolicy::StandaloneLoopback).is_ok()
        );
        assert!(
            validate_endpoint("http://issuer.example", EndpointPolicy::StandaloneLoopback).is_err()
        );
        assert!(
            validate_endpoint(
                "https://user:pass@issuer.example",
                EndpointPolicy::HttpsOnly
            )
            .is_err()
        );
    }

    #[test]
    fn managed_did_builds_final_typed_nonce_bound_proof() {
        let ProofFixture {
            proof,
            get_did,
            clock,
            profile_id,
            did,
            method,
        } = proof_fixture();
        let profile = ProtocolProfileId::parse(profile_id).expect("fixture profile id is valid");
        let jwt = block_on(proof.create(HolderProofRequest {
            profile_id: profile.clone(),
            holder_did: did.clone(),
            method_id: method.clone(),
            audience: STANDALONE_CREDENTIAL_ISSUER.to_owned(),
            nonce: "nonce-1".to_owned(),
        }));
        let jwt = jwt.expect("active authentication method should produce a proof");
        let now = clock.now().expect("clock").value() / 1_000;
        validate_key_proof(&jwt, &method, "nonce-1", now)
            .expect("generated proof should match the final OID4VCI shape");
        verify_key_proof_signature(get_did.as_ref(), &profile, &did, &method, &jwt)
            .expect("issuer should verify the generated proof signature");

        let mut tampered = jwt.into_bytes();
        let signature_start = tampered
            .iter()
            .rposition(|byte| *byte == b'.')
            .map(|position| position + 1)
            .expect("proof has a signature");
        tampered[signature_start] = if tampered[signature_start] == b'A' {
            b'B'
        } else {
            b'A'
        };
        let tampered = String::from_utf8(tampered).expect("ASCII proof");
        assert_eq!(
            verify_key_proof_signature(get_did.as_ref(), &profile, &did, &method, &tampered),
            Err(IssuanceProtocolError::InvalidProof)
        );
    }

    #[test]
    fn standalone_issuer_verifies_the_proof_before_issuing() {
        let ProofFixture {
            proof,
            get_did,
            clock,
            profile_id,
            did,
            method,
        } = proof_fixture();
        let adapter = StandaloneOid4vciIssuer::new(proof, get_did, clock);
        let profile = ProtocolProfileId::parse(profile_id).expect("fixture profile id is valid");
        let prepared = block_on(adapter.prepare(PrepareIssuanceRequest {
            profile_id: profile.clone(),
            offer: standalone_credential_offer(),
        }))
        .expect("offer should prepare");
        let issued = block_on(adapter.issue(ProtocolIssueRequest {
            profile_id: profile,
            issuance_id: prepared.id,
            holder_did: did,
            method_id: method,
        }))
        .expect("valid managed proof should issue");
        let expected = general_purpose::STANDARD
            .decode(
                include_str!("../../../../fixtures/credentials/standalone-midnight-phase1.b64")
                    .trim(),
            )
            .expect("fixture");
        assert_eq!(issued.signed_bytes, expected);
        assert!(issued.private_material.is_none());
    }

    #[test]
    fn standalone_protocol_prepares_refuses_and_rejects_bad_proofs() {
        struct BadProof;
        impl CredentialHolderProofPort for BadProof {
            fn create<'a>(&'a self, _: HolderProofRequest) -> HolderProofFuture<'a> {
                Box::pin(async { Ok("bad.proof.value".to_owned()) })
            }
        }
        let ProofFixture { get_did, clock, .. } = proof_fixture();
        let adapter = StandaloneOid4vciIssuer::new(Arc::new(BadProof), get_did, clock);
        let profile = ProtocolProfileId::parse("profile_1").expect("profile id is valid");
        let prepared = block_on(adapter.prepare(PrepareIssuanceRequest {
            profile_id: profile.clone(),
            offer: standalone_credential_offer(),
        }))
        .expect("offer should prepare");
        let error = block_on(adapter.issue(ProtocolIssueRequest {
            profile_id: profile,
            issuance_id: prepared.id,
            holder_did: "did:midnight:undeployed:holder".to_owned(),
            method_id: "did:midnight:undeployed:holder#auth-1".to_owned(),
        }))
        .expect_err("malformed proof must fail");
        assert_eq!(error, IssuanceProtocolError::InvalidProof);
    }
}
