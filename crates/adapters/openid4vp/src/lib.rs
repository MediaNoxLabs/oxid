// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    net::IpAddr,
    sync::{Arc, Mutex, MutexGuard},
};

use base64::{Engine as _, engine::general_purpose};
use oxid_credential_application::{
    CredentialDisclosureQuery, CredentialOperationError, CredentialProfileQuery,
    GetCredentialDisclosureUseCase, ListCredentialsUseCase,
};
use oxid_platform_ports::ClockPort;
use oxid_presentation_application::{
    CancelPresentationProofRequest, CredentialPresentationProtocolPort,
    FindPresentationCandidatesFuture, PrepareCredentialPresentationRequest,
    PreparePresentationPortFuture, PreparedCredentialPresentation, PresentCredentialPortFuture,
    PresentationCandidateError, PresentationCandidateQuery, PresentationCandidateSourcePort,
    PresentationProofControlPort, PresentationProofError, PresentationProofPort,
    PresentationProofRequest, PresentationProtocolError, PresentationProtocolOutcome,
    PresentationVerificationError, PresentationVerificationRequest, PresentationVerifierPort,
    ProtocolPresentCredentialRequest,
};
use oxid_presentation_domain::{
    CredentialPresentationId, CredentialPresentationPreview, PresentationClaimIntent,
    PresentationCredentialCandidate, RequestedPresentationClaim,
};
use serde::{Deserialize, Deserializer, de};
use serde_json::{Map, Number, Value, json};
use sha2::{Digest as _, Sha256};
use url::Url;
use zeroize::Zeroizing;

pub const STANDALONE_OPENID4VP_VERIFIER: &str = "http://127.0.0.1:32193/verifier";
const STANDALONE_CLIENT_ID: &str = "redirect_uri:http://127.0.0.1:32193/verifier/response";
const STANDALONE_REQUEST_URI: &str = "http://127.0.0.1:32193/verifier/request";
const STANDALONE_RESPONSE_URI: &str = "http://127.0.0.1:32193/verifier/response";
const STANDALONE_PURPOSE: &str = "Prove your first and last name and that you are at least 18.";
const DIGITAL_PASSPORT_SCHEMA: &str = "digital-passport:v1";
const DIGITAL_PASSPORT_QUERY_ID: &str = "digital_passport";
const MIDNIGHT_FORMAT: &str = "midnight_compact_vp";
const FIRST_NAME_PATH: &str = "/credentialSubject/firstName";
const LAST_NAME_PATH: &str = "/credentialSubject/lastName";
const DATE_OF_BIRTH_PATH: &str = "/credentialSubject/dateOfBirth";
const MAX_JSON_DEPTH: usize = 16;
const MAX_PROTOCOL_BYTES: usize = 64 * 1_024;
const MAX_ENDPOINT_CHARACTERS: usize = 2_048;
const MAX_TEXT_CHARACTERS: usize = 512;
const REQUEST_LIFETIME_SECONDS: u64 = 300;

#[must_use]
pub fn standalone_openid4vp_request() -> String {
    let mut url = Url::parse("openid4vp://authorize").expect("constant request URL is valid");
    url.query_pairs_mut()
        .append_pair("client_id", STANDALONE_CLIENT_ID)
        .append_pair("request_uri", STANDALONE_REQUEST_URI);
    url.into()
}

/// Credential-owned candidate lookup exposed to the OpenID4VP adapter through
/// schema-neutral application use cases only.
pub struct CredentialDisclosureCandidateSource {
    list: Arc<dyn ListCredentialsUseCase>,
    disclosure: Arc<dyn GetCredentialDisclosureUseCase>,
}

impl CredentialDisclosureCandidateSource {
    #[must_use]
    pub fn new(
        list: Arc<dyn ListCredentialsUseCase>,
        disclosure: Arc<dyn GetCredentialDisclosureUseCase>,
    ) -> Self {
        Self { list, disclosure }
    }
}

impl PresentationCandidateSourcePort for CredentialDisclosureCandidateSource {
    fn find<'a>(
        &'a self,
        query: PresentationCandidateQuery,
    ) -> FindPresentationCandidatesFuture<'a> {
        Box::pin(async move {
            let credentials = self
                .list
                .execute(CredentialProfileQuery {
                    profile_id: query.profile_id.as_str().to_owned(),
                })
                .map_err(map_candidate_error)?;
            let mut candidates = Vec::new();
            for credential in credentials {
                if credential.verification_outcome != "valid" {
                    continue;
                }
                let disclosure = match self.disclosure.execute(CredentialDisclosureQuery {
                    profile_id: query.profile_id.as_str().to_owned(),
                    credential_id: credential.id.clone(),
                }) {
                    Ok(disclosure) => disclosure,
                    Err(
                        CredentialOperationError::Persistence(_)
                        | CredentialOperationError::Disclosure(_)
                        | CredentialOperationError::VerificationNotValid,
                    ) => continue,
                    Err(_) => return Err(PresentationCandidateError::Unavailable),
                };
                if disclosure.schema_id != query.schema_id {
                    continue;
                }
                let matches = query.requested_claims.iter().all(|requested| {
                    disclosure.candidates.iter().any(|candidate| {
                        candidate.claim_path == requested.path()
                            && matches!(
                                (requested.intent(), candidate.privacy_tier.as_str()),
                                (PresentationClaimIntent::Reveal, "selective_disclosure")
                                    | (PresentationClaimIntent::Predicate, "predicate_only")
                            )
                    })
                });
                if matches {
                    candidates.push(
                        PresentationCredentialCandidate::new(
                            credential.id,
                            credential.display_name,
                            credential.issuer_did,
                        )
                        .map_err(|_| PresentationCandidateError::InvalidQuery)?,
                    );
                }
            }
            Ok(candidates)
        })
    }
}

fn map_candidate_error(_: CredentialOperationError) -> PresentationCandidateError {
    PresentationCandidateError::Unavailable
}

struct PreparedRequest {
    profile_id: String,
    client_id: String,
    response_uri: String,
    state: Zeroizing<String>,
    challenge_hash: [u8; 32],
    verifier_domain_hash: [u8; 32],
    expires_at_seconds: u64,
    requested_claims: Vec<RequestedPresentationClaim>,
    candidate_ids: BTreeSet<String>,
}

/// Deterministic Final OpenID4VP/DCQL request and consent boundary.
///
/// Acceptance consumes the verifier session before calling the configured
/// proof port. Composition decides whether that port is unavailable or backed
/// by an authenticated Compact runtime.
pub struct StandaloneOpenId4VpVerifier {
    candidates: Arc<dyn PresentationCandidateSourcePort>,
    proof: Arc<dyn PresentationProofPort>,
    proof_control: Option<Arc<dyn PresentationProofControlPort>>,
    verifier: Arc<dyn PresentationVerifierPort>,
    clock: Arc<dyn ClockPort>,
    sessions: Mutex<BTreeMap<String, PreparedRequest>>,
    next_id: std::sync::atomic::AtomicU64,
}

struct ProofCompletionGuard<'a> {
    control: Option<&'a dyn PresentationProofControlPort>,
    request: Option<CancelPresentationProofRequest>,
}

impl<'a> ProofCompletionGuard<'a> {
    fn new(
        control: Option<&'a dyn PresentationProofControlPort>,
        request: CancelPresentationProofRequest,
    ) -> Self {
        Self {
            control,
            request: control.map(|_| request),
        }
    }

    fn finish(mut self) -> Result<(), PresentationProofError> {
        let Some(control) = self.control.take() else {
            return Ok(());
        };
        let request = self
            .request
            .take()
            .ok_or(PresentationProofError::Rejected)?;
        control.finish(request)
    }
}

impl Drop for ProofCompletionGuard<'_> {
    fn drop(&mut self) {
        if let (Some(control), Some(request)) = (self.control.take(), self.request.take()) {
            let _ = control.finish(request);
        }
    }
}

impl StandaloneOpenId4VpVerifier {
    #[must_use]
    pub fn new(
        candidates: Arc<dyn PresentationCandidateSourcePort>,
        proof: Arc<dyn PresentationProofPort>,
        verifier: Arc<dyn PresentationVerifierPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            candidates,
            proof,
            proof_control: None,
            verifier,
            clock,
            sessions: Mutex::new(BTreeMap::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    #[must_use]
    pub fn with_proof_control(
        candidates: Arc<dyn PresentationCandidateSourcePort>,
        proof: Arc<dyn PresentationProofPort>,
        proof_control: Arc<dyn PresentationProofControlPort>,
        verifier: Arc<dyn PresentationVerifierPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            candidates,
            proof,
            proof_control: Some(proof_control),
            verifier,
            clock,
            sessions: Mutex::new(BTreeMap::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    fn sessions(
        &self,
    ) -> Result<MutexGuard<'_, BTreeMap<String, PreparedRequest>>, PresentationProtocolError> {
        self.sessions
            .lock()
            .map_err(|_| PresentationProtocolError::Unavailable)
    }

    fn next_id(&self) -> Result<CredentialPresentationId, PresentationProtocolError> {
        let value = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        CredentialPresentationId::parse(format!("presentation_{value:016x}"))
            .map_err(|_| PresentationProtocolError::Unavailable)
    }
}

impl CredentialPresentationProtocolPort for StandaloneOpenId4VpVerifier {
    fn prepare<'a>(
        &'a self,
        request: PrepareCredentialPresentationRequest,
    ) -> PreparePresentationPortFuture<'a> {
        Box::pin(async move {
            let invocation = parse_invocation(&request.request)?;
            if invocation.client_id != STANDALONE_CLIENT_ID
                || invocation.request_uri != STANDALONE_REQUEST_URI
            {
                return Err(PresentationProtocolError::InvalidVerifier);
            }
            let id = self.next_id()?;
            let now = self
                .clock
                .now()
                .map_err(|_| PresentationProtocolError::Unavailable)?
                .value()
                / 1_000;
            let nonce = Zeroizing::new(format!("oxid-vp-nonce-{}", id.as_str()));
            let state = Zeroizing::new(format!("oxid-vp-state-{}", id.as_str()));
            let request_object = standalone_request_object(&nonce, &state, now);
            let parsed = parse_request_object(request_object.to_string().as_bytes(), now)?;
            if parsed.client_id != invocation.client_id {
                return Err(PresentationProtocolError::InvalidVerifier);
            }
            let candidates = self
                .candidates
                .find(PresentationCandidateQuery {
                    profile_id: request.profile_id.clone(),
                    schema_id: parsed.schema_id.clone(),
                    requested_claims: parsed.requested_claims.clone(),
                })
                .await
                .map_err(|error| match error {
                    PresentationCandidateError::Unavailable => {
                        PresentationProtocolError::Unavailable
                    }
                    PresentationCandidateError::InvalidQuery => {
                        PresentationProtocolError::InvalidRequest
                    }
                })?;
            if candidates.is_empty() {
                return Err(PresentationProtocolError::NoCandidate);
            }
            let preview = CredentialPresentationPreview::new(
                STANDALONE_OPENID4VP_VERIFIER,
                parsed.purpose,
                parsed.query_id,
                candidates.clone(),
                parsed.requested_claims.clone(),
            )
            .map_err(|_| PresentationProtocolError::InvalidRequest)?;
            let prepared = PreparedRequest {
                profile_id: request.profile_id.as_str().to_owned(),
                client_id: parsed.client_id,
                response_uri: parsed.response_uri,
                state,
                challenge_hash: parsed.challenge_hash,
                verifier_domain_hash: parsed.verifier_domain_hash,
                expires_at_seconds: parsed.expires_at_seconds,
                requested_claims: parsed.requested_claims,
                candidate_ids: candidates
                    .into_iter()
                    .map(|candidate| candidate.credential_id().to_owned())
                    .collect(),
            };
            if self
                .sessions()?
                .insert(id.as_str().to_owned(), prepared)
                .is_some()
            {
                return Err(PresentationProtocolError::Unavailable);
            }
            Ok(PreparedCredentialPresentation { id, preview })
        })
    }

    fn present<'a>(
        &'a self,
        request: ProtocolPresentCredentialRequest,
    ) -> PresentCredentialPortFuture<'a> {
        Box::pin(async move {
            let prepared = self
                .sessions()?
                .remove(request.presentation_id.as_str())
                .ok_or(PresentationProtocolError::InvalidRequest)?;
            if prepared.profile_id != request.profile_id.as_str()
                || !prepared.candidate_ids.contains(&request.credential_id)
            {
                return Err(PresentationProtocolError::InvalidRequest);
            }
            let now = self
                .clock
                .now()
                .map_err(|_| PresentationProtocolError::Unavailable)?
                .value()
                / 1_000;
            if now >= prepared.expires_at_seconds {
                return Err(PresentationProtocolError::RequestExpired);
            }
            let proof_request = PresentationProofRequest {
                profile_id: request.profile_id.clone(),
                presentation_id: request.presentation_id.clone(),
                credential_id: request.credential_id.clone(),
                verifier: prepared.client_id.clone(),
                challenge_hash: prepared.challenge_hash,
                verifier_domain_hash: prepared.verifier_domain_hash,
                requested_claims: prepared.requested_claims.clone(),
            };
            let proof = self
                .proof
                .create(proof_request)
                .await
                .map_err(map_proof_error)?;
            let proof_control_request = CancelPresentationProofRequest {
                profile_id: request.profile_id.clone(),
                presentation_id: request.presentation_id.clone(),
            };
            let proof_completion =
                ProofCompletionGuard::new(self.proof_control.as_deref(), proof_control_request);
            let verification = self
                .verifier
                .verify(PresentationVerificationRequest {
                    profile_id: request.profile_id,
                    credential_id: request.credential_id,
                    verifier: prepared.client_id,
                    challenge_hash: prepared.challenge_hash,
                    verifier_domain_hash: prepared.verifier_domain_hash,
                    requested_claims: prepared.requested_claims,
                    proof: proof.clone(),
                })
                .await;
            let proof_completion = proof_completion.finish().map_err(map_proof_error);
            verification.map_err(map_verification_error)?;
            proof_completion?;
            let response = json!({
                "state": prepared.state.as_str(),
                "vp_token": {
                    DIGITAL_PASSPORT_QUERY_ID: [{
                        "format": MIDNIGHT_FORMAT,
                        "proof": general_purpose::URL_SAFE_NO_PAD.encode(proof.as_bytes())
                    }]
                }
            });
            validate_response_container(
                response.to_string().as_bytes(),
                prepared.state.as_str(),
                &prepared.response_uri,
            )?;
            Ok(PresentationProtocolOutcome {
                verifier_validated: true,
            })
        })
    }

    fn discard(
        &self,
        presentation_id: &CredentialPresentationId,
    ) -> Result<(), PresentationProtocolError> {
        self.sessions()?
            .remove(presentation_id.as_str())
            .map(|_| ())
            .ok_or(PresentationProtocolError::InvalidRequest)
    }

    fn cancel(
        &self,
        request: CancelPresentationProofRequest,
    ) -> Result<(), PresentationProtocolError> {
        self.proof_control
            .as_ref()
            .ok_or(PresentationProtocolError::ProofUnavailable)?
            .cancel(request)
            .map_err(map_proof_error)
    }

    fn set_foreground(&self, foreground: bool) -> Result<(), PresentationProtocolError> {
        self.proof_control
            .as_ref()
            .ok_or(PresentationProtocolError::ProofUnavailable)?
            .set_foreground(foreground)
            .map_err(map_proof_error)
    }
}

fn map_proof_error(error: PresentationProofError) -> PresentationProtocolError {
    match error {
        PresentationProofError::Unavailable => PresentationProtocolError::ProofUnavailable,
        PresentationProofError::Busy => PresentationProtocolError::ProofBusy,
        PresentationProofError::Cancelled => PresentationProtocolError::ProofCancelled,
        PresentationProofError::Backgrounded => PresentationProtocolError::ProofBackgrounded,
        PresentationProofError::TimedOut => PresentationProtocolError::ProofTimedOut,
        PresentationProofError::HolderAuthorizationUnavailable => {
            PresentationProtocolError::HolderAuthorizationUnavailable
        }
        PresentationProofError::HolderNotAuthorized => {
            PresentationProtocolError::HolderNotAuthorized
        }
        PresentationProofError::InvalidCredential | PresentationProofError::InvalidSelection => {
            PresentationProtocolError::InvalidRequest
        }
        PresentationProofError::Rejected => PresentationProtocolError::InvalidProof,
    }
}

fn map_verification_error(error: PresentationVerificationError) -> PresentationProtocolError {
    match error {
        PresentationVerificationError::Unavailable => PresentationProtocolError::ProofUnavailable,
        PresentationVerificationError::InvalidProof => PresentationProtocolError::InvalidProof,
        PresentationVerificationError::Rejected => PresentationProtocolError::VerifierRejected,
    }
}

struct ParsedInvocation {
    client_id: String,
    request_uri: String,
}

fn parse_invocation(input: &str) -> Result<ParsedInvocation, PresentationProtocolError> {
    if input.len() > MAX_PROTOCOL_BYTES {
        return Err(PresentationProtocolError::InvalidRequest);
    }
    let url = Url::parse(input).map_err(|_| PresentationProtocolError::InvalidRequest)?;
    if url.scheme() != "openid4vp"
        || url.host_str() != Some("authorize")
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(PresentationProtocolError::InvalidRequest);
    }
    let pairs = url.query_pairs().collect::<Vec<_>>();
    if pairs.len() != 2 {
        return Err(PresentationProtocolError::InvalidRequest);
    }
    let mut values = BTreeMap::new();
    for (name, value) in pairs {
        if !matches!(name.as_ref(), "client_id" | "request_uri")
            || values
                .insert(name.into_owned(), value.into_owned())
                .is_some()
        {
            return Err(PresentationProtocolError::InvalidRequest);
        }
    }
    let client_id = values
        .remove("client_id")
        .ok_or(PresentationProtocolError::InvalidRequest)?;
    validate_redirect_client_id(&client_id)?;
    let request_uri = values
        .remove("request_uri")
        .ok_or(PresentationProtocolError::InvalidRequest)?;
    let endpoint = validate_endpoint(&request_uri)?;
    if endpoint.as_str() != STANDALONE_REQUEST_URI {
        return Err(PresentationProtocolError::InvalidVerifier);
    }
    Ok(ParsedInvocation {
        client_id,
        request_uri,
    })
}

fn standalone_request_object(nonce: &str, state: &str, now: u64) -> Value {
    json!({
        "client_id": STANDALONE_CLIENT_ID,
        "response_type": "vp_token",
        "response_mode": "direct_post",
        "response_uri": STANDALONE_RESPONSE_URI,
        "nonce": nonce,
        "state": state,
        "iat": now,
        "exp": now + REQUEST_LIFETIME_SECONDS,
        "client_metadata": {
            "client_name": "Oxid standalone verifier",
            "purpose": STANDALONE_PURPOSE,
            "vp_formats_supported": { MIDNIGHT_FORMAT: {} }
        },
        "dcql_query": {
            "credentials": [{
                "id": DIGITAL_PASSPORT_QUERY_ID,
                "format": MIDNIGHT_FORMAT,
                "meta": { "schema_id": DIGITAL_PASSPORT_SCHEMA },
                "claims": [
                    { "id": "first_name", "path": ["credentialSubject", "firstName"] },
                    { "id": "last_name", "path": ["credentialSubject", "lastName"] },
                    { "id": "age_over_18", "path": ["credentialSubject", "dateOfBirth"] }
                ],
                "require_cryptographic_holder_binding": true
            }]
        },
        "midnight": {
            "profile": "org.midnight.credentials.openid.v1",
            "verifier_domain": "127.0.0.1:32193",
            "accepted_credential_families": ["midnight:vc:digital-passport"],
            "predicate_hints": [{"claim_id": "age_over_18", "kind": "age_over", "threshold": 18}]
        }
    })
}

struct ParsedRequestObject {
    client_id: String,
    response_uri: String,
    purpose: String,
    query_id: String,
    schema_id: String,
    requested_claims: Vec<RequestedPresentationClaim>,
    challenge_hash: [u8; 32],
    verifier_domain_hash: [u8; 32],
    expires_at_seconds: u64,
}

fn parse_request_object(
    bytes: &[u8],
    now: u64,
) -> Result<ParsedRequestObject, PresentationProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = exact_object(
        &value,
        &[
            "client_id",
            "response_type",
            "response_mode",
            "response_uri",
            "nonce",
            "state",
            "iat",
            "exp",
            "client_metadata",
            "dcql_query",
            "midnight",
        ],
    )?;
    if required_string(object, "response_type", 32)? != "vp_token"
        || required_string(object, "response_mode", 32)? != "direct_post"
        || object.contains_key("redirect_uri")
        || object.contains_key("presentation_definition")
        || object.contains_key("scope")
    {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }
    let client_id = required_string(object, "client_id", MAX_ENDPOINT_CHARACTERS)?;
    validate_redirect_client_id(&client_id)?;
    let response_uri = required_string(object, "response_uri", MAX_ENDPOINT_CHARACTERS)?;
    let response_endpoint = validate_endpoint(&response_uri)?;
    if response_endpoint.as_str() != STANDALONE_RESPONSE_URI
        || client_id != format!("redirect_uri:{response_uri}")
    {
        return Err(PresentationProtocolError::InvalidVerifier);
    }
    let nonce = required_string(object, "nonce", MAX_TEXT_CHARACTERS)?;
    required_string(object, "state", MAX_TEXT_CHARACTERS)?;
    let issued_at = required_u64(object, "iat")?;
    let expires_at = required_u64(object, "exp")?;
    if issued_at > now.saturating_add(60)
        || expires_at <= now
        || expires_at <= issued_at
        || expires_at.saturating_sub(issued_at) > REQUEST_LIFETIME_SECONDS
    {
        return Err(PresentationProtocolError::RequestExpired);
    }
    let metadata = required_exact_object(
        object,
        "client_metadata",
        &["client_name", "purpose", "vp_formats_supported"],
    )?;
    required_string(metadata, "client_name", MAX_TEXT_CHARACTERS)?;
    let purpose = required_string(metadata, "purpose", MAX_TEXT_CHARACTERS)?;
    let formats = required_exact_object(metadata, "vp_formats_supported", &[MIDNIGHT_FORMAT])?;
    exact_object(
        formats
            .get(MIDNIGHT_FORMAT)
            .ok_or(PresentationProtocolError::InvalidRequest)?,
        &[],
    )?;

    let dcql = required_exact_object(object, "dcql_query", &["credentials"])?;
    let credentials = required_array(dcql, "credentials")?;
    if credentials.len() != 1 {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }
    let query = exact_object(
        &credentials[0],
        &[
            "id",
            "format",
            "meta",
            "claims",
            "require_cryptographic_holder_binding",
        ],
    )?;
    let query_id = required_string(query, "id", MAX_TEXT_CHARACTERS)?;
    if query_id != DIGITAL_PASSPORT_QUERY_ID
        || required_string(query, "format", 64)? != MIDNIGHT_FORMAT
        || !required_bool(query, "require_cryptographic_holder_binding")?
    {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }
    let meta = required_exact_object(query, "meta", &["schema_id"])?;
    let schema_id = required_string(meta, "schema_id", MAX_TEXT_CHARACTERS)?;
    if schema_id != DIGITAL_PASSPORT_SCHEMA {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }
    let claim_values = required_array(query, "claims")?;
    if claim_values.len() != 3 {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }
    let mut claim_ids = BTreeSet::new();
    let mut claim_paths = BTreeMap::new();
    for claim in claim_values {
        let claim = exact_object(claim, &["id", "path"])?;
        let id = required_string(claim, "id", MAX_TEXT_CHARACTERS)?;
        let segments = required_array(claim, "path")?;
        if segments.len() != 2
            || !segments.iter().all(Value::is_string)
            || !claim_ids.insert(id.clone())
        {
            return Err(PresentationProtocolError::InvalidRequest);
        }
        claim_paths.insert(
            id,
            format!(
                "/{}/{}",
                segments[0]
                    .as_str()
                    .ok_or(PresentationProtocolError::InvalidRequest)?,
                segments[1]
                    .as_str()
                    .ok_or(PresentationProtocolError::InvalidRequest)?
            ),
        );
    }
    if claim_paths.get("first_name").map(String::as_str) != Some(FIRST_NAME_PATH)
        || claim_paths.get("last_name").map(String::as_str) != Some(LAST_NAME_PATH)
        || claim_paths.get("age_over_18").map(String::as_str) != Some(DATE_OF_BIRTH_PATH)
    {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }

    let midnight = required_exact_object(
        object,
        "midnight",
        &[
            "profile",
            "verifier_domain",
            "accepted_credential_families",
            "predicate_hints",
        ],
    )?;
    let verifier_domain = required_string(midnight, "verifier_domain", MAX_TEXT_CHARACTERS)?;
    if required_string(midnight, "profile", MAX_TEXT_CHARACTERS)?
        != "org.midnight.credentials.openid.v1"
        || verifier_domain != "127.0.0.1:32193"
    {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }
    let families = required_array(midnight, "accepted_credential_families")?;
    if families.len() != 1 || families[0].as_str() != Some("midnight:vc:digital-passport") {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }
    let predicates = required_array(midnight, "predicate_hints")?;
    if predicates.len() != 1 {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }
    let predicate = exact_object(&predicates[0], &["claim_id", "kind", "threshold"])?;
    if required_string(predicate, "claim_id", MAX_TEXT_CHARACTERS)? != "age_over_18"
        || required_string(predicate, "kind", MAX_TEXT_CHARACTERS)? != "age_over"
        || required_u64(predicate, "threshold")? != 18
    {
        return Err(PresentationProtocolError::UnsupportedRequest);
    }
    let requested_claims = vec![
        RequestedPresentationClaim::reveal(FIRST_NAME_PATH, "First name")
            .map_err(|_| PresentationProtocolError::InvalidRequest)?,
        RequestedPresentationClaim::reveal(LAST_NAME_PATH, "Last name")
            .map_err(|_| PresentationProtocolError::InvalidRequest)?,
        RequestedPresentationClaim::predicate(DATE_OF_BIRTH_PATH, "Age over 18", "age_over", 18)
            .map_err(|_| PresentationProtocolError::InvalidRequest)?,
    ];
    Ok(ParsedRequestObject {
        client_id,
        response_uri,
        purpose,
        query_id,
        schema_id,
        requested_claims,
        challenge_hash: Sha256::digest(nonce.as_bytes()).into(),
        verifier_domain_hash: verifier_domain_hash(&verifier_domain),
        expires_at_seconds: expires_at,
    })
}

fn verifier_domain_hash(domain: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:openid4vp:verifier-domain:v1\0");
    digest.update(domain.as_bytes());
    digest.finalize().into()
}

fn validate_response_container(
    bytes: &[u8],
    expected_state: &str,
    response_uri: &str,
) -> Result<(), PresentationProtocolError> {
    if response_uri != STANDALONE_RESPONSE_URI {
        return Err(PresentationProtocolError::InvalidVerifier);
    }
    let value = parse_strict_json(bytes).map_err(|_| PresentationProtocolError::InvalidProof)?;
    let object = exact_object(&value, &["state", "vp_token"])
        .map_err(|_| PresentationProtocolError::InvalidProof)?;
    if required_string(object, "state", MAX_TEXT_CHARACTERS)
        .map_err(|_| PresentationProtocolError::InvalidProof)?
        != expected_state
    {
        return Err(PresentationProtocolError::VerifierRejected);
    }
    let vp_token = required_exact_object(object, "vp_token", &[DIGITAL_PASSPORT_QUERY_ID])
        .map_err(|_| PresentationProtocolError::InvalidProof)?;
    let presentations = required_array(vp_token, DIGITAL_PASSPORT_QUERY_ID)
        .map_err(|_| PresentationProtocolError::InvalidProof)?;
    if presentations.len() != 1 {
        return Err(PresentationProtocolError::InvalidProof);
    }
    let presentation = exact_object(&presentations[0], &["format", "proof"])
        .map_err(|_| PresentationProtocolError::InvalidProof)?;
    if required_string(presentation, "format", 64)
        .map_err(|_| PresentationProtocolError::InvalidProof)?
        != MIDNIGHT_FORMAT
    {
        return Err(PresentationProtocolError::InvalidProof);
    }
    let proof = required_string(presentation, "proof", 6 * 1_024 * 1_024)
        .map_err(|_| PresentationProtocolError::InvalidProof)?;
    let decoded = general_purpose::URL_SAFE_NO_PAD
        .decode(proof)
        .map_err(|_| PresentationProtocolError::InvalidProof)?;
    (!decoded.is_empty() && decoded.len() <= 4 * 1_024 * 1_024)
        .then_some(())
        .ok_or(PresentationProtocolError::InvalidProof)
}

fn validate_redirect_client_id(value: &str) -> Result<(), PresentationProtocolError> {
    let response_uri = value
        .strip_prefix("redirect_uri:")
        .ok_or(PresentationProtocolError::InvalidVerifier)?;
    let endpoint = validate_endpoint(response_uri)?;
    if endpoint.as_str() != STANDALONE_RESPONSE_URI {
        return Err(PresentationProtocolError::InvalidVerifier);
    }
    Ok(())
}

fn validate_endpoint(value: &str) -> Result<Url, PresentationProtocolError> {
    if value.chars().count() > MAX_ENDPOINT_CHARACTERS {
        return Err(PresentationProtocolError::InvalidVerifier);
    }
    let url = Url::parse(value).map_err(|_| PresentationProtocolError::InvalidVerifier)?;
    if url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
    {
        return Err(PresentationProtocolError::InvalidVerifier);
    }
    match url.scheme() {
        "https" => {}
        "http" => {
            let host = url
                .host_str()
                .ok_or(PresentationProtocolError::InvalidVerifier)?;
            let loopback = host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<IpAddr>()
                    .is_ok_and(|address| address.is_loopback());
            if !loopback {
                return Err(PresentationProtocolError::InvalidVerifier);
            }
        }
        _ => return Err(PresentationProtocolError::InvalidVerifier),
    }
    Ok(url)
}

fn parse_strict_json(bytes: &[u8]) -> Result<Value, PresentationProtocolError> {
    if bytes.is_empty() || bytes.len() > MAX_PROTOCOL_BYTES {
        return Err(PresentationProtocolError::InvalidRequest);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = StrictValue::deserialize(&mut deserializer)
        .map_err(|_| PresentationProtocolError::InvalidRequest)?
        .0;
    deserializer
        .end()
        .map_err(|_| PresentationProtocolError::InvalidRequest)?;
    if json_depth(&value, 1) > MAX_JSON_DEPTH {
        return Err(PresentationProtocolError::InvalidRequest);
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

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
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
            if values.len() >= 128 {
                return Err(de::Error::custom("JSON array exceeds item limit"));
            }
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
            if values.len() >= 128 || !names.insert(name.clone()) {
                return Err(de::Error::custom(
                    "duplicate or excessive JSON object member",
                ));
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

fn exact_object<'a>(
    value: &'a Value,
    keys: &[&str],
) -> Result<&'a Map<String, Value>, PresentationProtocolError> {
    let object = value
        .as_object()
        .ok_or(PresentationProtocolError::InvalidRequest)?;
    let expected = keys.iter().copied().collect::<BTreeSet<_>>();
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if object.len() != keys.len() || actual != expected {
        return Err(PresentationProtocolError::InvalidRequest);
    }
    Ok(object)
}

fn required_exact_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    keys: &[&str],
) -> Result<&'a Map<String, Value>, PresentationProtocolError> {
    exact_object(
        object
            .get(key)
            .ok_or(PresentationProtocolError::InvalidRequest)?,
        keys,
    )
}

fn required_array<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a [Value], PresentationProtocolError> {
    object
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(PresentationProtocolError::InvalidRequest)
}

fn required_string(
    object: &Map<String, Value>,
    key: &str,
    maximum: usize,
) -> Result<String, PresentationProtocolError> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(PresentationProtocolError::InvalidRequest)?;
    if value.is_empty()
        || value.chars().count() > maximum
        || value.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '\u{202a}'..='\u{202e}')
        })
    {
        return Err(PresentationProtocolError::InvalidRequest);
    }
    Ok(value.to_owned())
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, PresentationProtocolError> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(PresentationProtocolError::InvalidRequest)
}

fn required_bool(
    object: &Map<String, Value>,
    key: &str,
) -> Result<bool, PresentationProtocolError> {
    object
        .get(key)
        .and_then(Value::as_bool)
        .ok_or(PresentationProtocolError::InvalidRequest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_credential_application::{
        CredentialDisclosureCandidateView, CredentialDisclosureView, CredentialOperationError,
        CredentialView, GetCredentialDisclosureUseCase, ListCredentialsUseCase,
        VerificationStageView,
    };
    use oxid_foundation::UnixTimestampMillis;
    use oxid_platform_ports::PlatformError;
    use oxid_presentation_application::{
        PresentationProofArtifact, UnavailablePresentationProof, UnavailablePresentationVerifier,
    };
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Clock;
    impl ClockPort for Clock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    struct Credentials;
    impl ListCredentialsUseCase for Credentials {
        fn execute(
            &self,
            _: CredentialProfileQuery,
        ) -> Result<Vec<CredentialView>, CredentialOperationError> {
            Ok(vec![
                CredentialView {
                    id: "vc_one".to_owned(),
                    display_name: "Digital Passport".to_owned(),
                    issuer_did: "did:midnight:undeployed:issuer".to_owned(),
                    subject_did: None,
                    format: "midnight_cbor_phase1".to_owned(),
                    issued_at_ms: None,
                    verification_outcome: "valid".to_owned(),
                    verification_stages: Vec::<VerificationStageView>::new(),
                },
                CredentialView {
                    id: "vc_two".to_owned(),
                    display_name: "Digital Passport".to_owned(),
                    issuer_did: "did:midnight:undeployed:second-issuer".to_owned(),
                    subject_did: None,
                    format: "midnight_compact_vc".to_owned(),
                    issued_at_ms: None,
                    verification_outcome: "valid".to_owned(),
                    verification_stages: Vec::<VerificationStageView>::new(),
                },
            ])
        }
    }

    impl GetCredentialDisclosureUseCase for Credentials {
        fn execute(
            &self,
            _: CredentialDisclosureQuery,
        ) -> Result<CredentialDisclosureView, CredentialOperationError> {
            Ok(CredentialDisclosureView {
                credential_id: "vc_one".to_owned(),
                schema_id: DIGITAL_PASSPORT_SCHEMA.to_owned(),
                candidates: vec![
                    candidate(FIRST_NAME_PATH, "First name", "selective_disclosure"),
                    candidate(LAST_NAME_PATH, "Last name", "selective_disclosure"),
                    candidate(DATE_OF_BIRTH_PATH, "Age", "predicate_only"),
                ],
            })
        }
    }

    fn candidate(path: &str, label: &str, privacy: &str) -> CredentialDisclosureCandidateView {
        CredentialDisclosureCandidateView {
            claim_path: path.to_owned(),
            label: label.to_owned(),
            privacy_tier: privacy.to_owned(),
        }
    }

    fn adapter() -> StandaloneOpenId4VpVerifier {
        let credentials = Arc::new(Credentials);
        StandaloneOpenId4VpVerifier::new(
            Arc::new(CredentialDisclosureCandidateSource::new(
                credentials.clone(),
                credentials,
            )),
            Arc::new(UnavailablePresentationProof),
            Arc::new(UnavailablePresentationVerifier),
            Arc::new(Clock),
        )
    }

    struct Proof;

    impl PresentationProofPort for Proof {
        fn create<'a>(
            &'a self,
            _: PresentationProofRequest,
        ) -> oxid_presentation_application::CreatePresentationProofFuture<'a> {
            Box::pin(async { PresentationProofArtifact::new(vec![0x42]) })
        }
    }

    struct Verifier;

    impl PresentationVerifierPort for Verifier {
        fn verify<'a>(
            &'a self,
            _: PresentationVerificationRequest,
        ) -> oxid_presentation_application::VerifyPresentationProofFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Default)]
    struct ProofControl {
        finished: AtomicBool,
    }

    impl PresentationProofControlPort for ProofControl {
        fn cancel(&self, _: CancelPresentationProofRequest) -> Result<(), PresentationProofError> {
            Ok(())
        }

        fn set_foreground(&self, _: bool) -> Result<(), PresentationProofError> {
            Ok(())
        }

        fn finish(&self, _: CancelPresentationProofRequest) -> Result<(), PresentationProofError> {
            self.finished.store(true, Ordering::Release);
            Ok(())
        }
    }

    #[test]
    fn proof_completion_guard_releases_admission_when_dropped() {
        let control = ProofControl::default();
        {
            let _completion = ProofCompletionGuard::new(
                Some(&control),
                CancelPresentationProofRequest {
                    profile_id: oxid_presentation_domain::PresentationProfileId::parse(
                        "profile_one",
                    )
                    .expect("profile"),
                    presentation_id: oxid_presentation_domain::CredentialPresentationId::parse(
                        "presentation_one",
                    )
                    .expect("presentation"),
                },
            );
        }
        assert!(control.finished.load(Ordering::Acquire));
    }

    #[test]
    fn strict_final_dcql_preview_matches_candidates_without_values() {
        let adapter = adapter();
        let prepared = futures::executor::block_on(
            adapter.prepare(PrepareCredentialPresentationRequest {
                profile_id: oxid_presentation_domain::PresentationProfileId::parse("profile_one")
                    .expect("profile"),
                request: standalone_openid4vp_request(),
            }),
        )
        .expect("prepare");
        assert_eq!(prepared.preview.query_id(), DIGITAL_PASSPORT_QUERY_ID);
        assert_eq!(prepared.preview.candidates().len(), 2);
        assert_eq!(
            prepared.preview.candidates()[1].issuer(),
            "did:midnight:undeployed:second-issuer"
        );
        assert_eq!(prepared.preview.requested_claims().len(), 3);
        assert_eq!(prepared.preview.requested_claims()[2].threshold(), Some(18));
        let debug = format!("{prepared:?}");
        assert!(!debug.contains("Alice"));
        assert!(!debug.contains("Example"));
    }

    #[test]
    fn request_parser_derives_the_domain_separated_verifier_hash() {
        let parsed = parse_request_object(
            standalone_request_object("nonce", "state", 1)
                .to_string()
                .as_bytes(),
            1,
        )
        .expect("request");
        assert_eq!(
            hex::encode(parsed.verifier_domain_hash),
            "786028d6c0189dbbc11bf4c3853af8bac2e840070f61494d94de2ca693801d36"
        );
    }

    #[test]
    fn accept_consumes_session_and_fails_before_vp_token_without_real_proof() {
        let adapter = adapter();
        let profile =
            oxid_presentation_domain::PresentationProfileId::parse("profile_one").expect("profile");
        let prepared =
            futures::executor::block_on(adapter.prepare(PrepareCredentialPresentationRequest {
                profile_id: profile.clone(),
                request: standalone_openid4vp_request(),
            }))
            .expect("prepare");
        assert_eq!(
            futures::executor::block_on(adapter.present(ProtocolPresentCredentialRequest {
                profile_id: profile.clone(),
                presentation_id: prepared.id.clone(),
                credential_id: "vc_one".to_owned(),
            })),
            Err(PresentationProtocolError::ProofUnavailable)
        );
        assert_eq!(
            futures::executor::block_on(adapter.present(ProtocolPresentCredentialRequest {
                profile_id: profile,
                presentation_id: prepared.id,
                credential_id: "vc_one".to_owned(),
            })),
            Err(PresentationProtocolError::InvalidRequest)
        );
    }

    #[test]
    fn controlled_proof_releases_admission_only_after_independent_verification() {
        let credentials = Arc::new(Credentials);
        let control = Arc::new(ProofControl::default());
        let adapter = StandaloneOpenId4VpVerifier::with_proof_control(
            Arc::new(CredentialDisclosureCandidateSource::new(
                credentials.clone(),
                credentials,
            )),
            Arc::new(Proof),
            control.clone(),
            Arc::new(Verifier),
            Arc::new(Clock),
        );
        let profile =
            oxid_presentation_domain::PresentationProfileId::parse("profile_one").expect("profile");
        let prepared =
            futures::executor::block_on(adapter.prepare(PrepareCredentialPresentationRequest {
                profile_id: profile.clone(),
                request: standalone_openid4vp_request(),
            }))
            .expect("prepare");
        let outcome =
            futures::executor::block_on(adapter.present(ProtocolPresentCredentialRequest {
                profile_id: profile,
                presentation_id: prepared.id,
                credential_id: "vc_one".to_owned(),
            }))
            .expect("present");
        assert!(outcome.verifier_validated);
        assert!(control.finished.load(Ordering::Acquire));
    }

    #[test]
    fn duplicate_unknown_and_oversized_inputs_fail_closed() {
        let duplicate = br#"{"client_id":"redirect_uri:http://127.0.0.1:32193/verifier/response","client_id":"attacker","response_type":"vp_token"}"#;
        assert_eq!(
            parse_request_object(duplicate, 1).err(),
            Some(PresentationProtocolError::InvalidRequest)
        );
        assert_eq!(
            parse_request_object(&vec![b' '; MAX_PROTOCOL_BYTES + 1], 1).err(),
            Some(PresentationProtocolError::InvalidRequest)
        );
        let mut request = standalone_request_object("nonce", "state", 1);
        request
            .as_object_mut()
            .expect("object")
            .insert("unknown".to_owned(), Value::Bool(true));
        assert_eq!(
            parse_request_object(request.to_string().as_bytes(), 1).err(),
            Some(PresentationProtocolError::InvalidRequest)
        );
    }

    #[test]
    fn proof_artifact_debug_output_is_redacted() {
        let artifact = PresentationProofArtifact::new(b"secret-proof".to_vec()).expect("proof");
        let debug = format!("{artifact:?}");
        assert!(debug.contains("length"));
        assert!(!debug.contains("secret-proof"));
    }
}
