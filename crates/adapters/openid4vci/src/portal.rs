// SPDX-License-Identifier: Apache-2.0

//! Strict native-host-only HTTP client for the exact Portal PR #17 profile.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read as _,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose};
use futures::StreamExt as _;
use oxid_identity_application::GetDidRecordUseCase;
use oxid_protocol_application::{
    CredentialHolderProofPort, CredentialIssuanceProtocolPort, HolderProofRequest,
    IssuanceProtocolError, IssueCredentialPortFuture, IssuedCredentialBytes,
    PrepareIssuancePortFuture, PrepareIssuanceRequest, PreparedCredentialOffer,
    ProtocolIssueRequest,
};
use oxid_protocol_domain::{CredentialIssuanceId, CredentialOfferPreview};
use reqwest::{
    Certificate, Client, Response, StatusCode,
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};
use url::Url;
use webpki_root_certs::TLS_SERVER_ROOT_CERTS;
use zeroize::Zeroizing;

use super::{
    EndpointPolicy, ParsedOffer, host_is_loopback, map_get_did_error, map_holder_proof_error,
    parse_issuer_metadata, parse_offer, parse_strict_json, required_object, required_string,
    required_unique_strings, resolve_holder_binding, validate_endpoint,
};

pub const PORTAL_PR_HEAD: &str = "9c82db23eabe8b6d758b2731f2225910ea627c14";
pub const PORTAL_PROFILE_SOURCE: &str = "76e8edf394a4cb37ca822037272d543c68f25f71";
pub const PORTAL_PROVENANCE_SHA256: &str =
    "cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87";
const PORTAL_CONFIGURATION_ID: &str = "digital_passport_v1";
const PORTAL_FORMAT: &str = "midnight_cbor_phase1";
const PORTAL_FAMILY: &str = "digital-passport";
const PORTAL_SCHEMA_ID: &str = "digital-passport:v1";
const PORTAL_SCHEMA_VERSION: &str = "1.0";
const PORTAL_ENCODING: &str = "compact-value-v1.base64url";
const PRE_AUTHORIZED_GRANT: &str = "urn:ietf:params:oauth:grant-type:pre-authorized_code";
const MAX_DEPLOYMENT_MANIFEST_BYTES: usize = 65_536;
const MAX_METADATA_BYTES: usize = 128 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_NONCE_BYTES: usize = 16 * 1024;
const MAX_CREDENTIAL_BYTES: usize = 1024 * 1024 + 32 * 1024;
const MAX_SECRET_BYTES: usize = 4_096;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const BUNDLED_SOURCE_LOCK: &[u8] = include_bytes!(
    "../../../../fixtures/laceid-portal/76e8edf394a4cb37ca822037272d543c68f25f71/source-lock.json"
);
const BUNDLED_PROVENANCE: &[u8] = include_bytes!(
    "../../../../fixtures/laceid-portal/76e8edf394a4cb37ca822037272d543c68f25f71/openid4vci-final/provenance.json"
);

/// Payload-free deployment/source-lock authentication errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalDeploymentManifestError {
    InvalidFile,
    InvalidDigest,
    DigestMismatch,
    InvalidManifest,
    SourceLockMismatch,
    InvalidOrigin,
    InvalidIssuer,
    InvalidJwk,
    ClientUnavailable,
}

impl std::fmt::Display for PortalDeploymentManifestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFile => "invalid Portal deployment manifest file",
            Self::InvalidDigest => "invalid Portal deployment manifest digest",
            Self::DigestMismatch => "Portal deployment manifest digest mismatch",
            Self::InvalidManifest => "invalid Portal deployment manifest",
            Self::SourceLockMismatch => "Portal source lock mismatch",
            Self::InvalidOrigin => "invalid Portal deployment origin",
            Self::InvalidIssuer => "invalid Portal issuer identity",
            Self::InvalidJwk => "invalid Portal issuer public JWK",
            Self::ClientUnavailable => "Portal HTTP client unavailable",
        })
    }
}

impl std::error::Error for PortalDeploymentManifestError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalPublicJwk {
    #[serde(rename = "crv")]
    pub curve: String,
    #[serde(rename = "kty")]
    pub key_type: String,
    pub x: String,
    pub y: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PortalDeploymentManifest {
    issuer_did: String,
    issuer_jubjub_jwk: PortalPublicJwk,
    issuer_jubjub_jwk_sha256: String,
    issuer_method: String,
    issuer_origin: String,
    issuer_resolver_origin: String,
    portal_pr_head: String,
    profile_source_commit: String,
    provenance_sha256: String,
    schema: String,
}

impl PortalDeploymentManifest {
    pub fn from_file(
        path: impl AsRef<Path>,
        expected_sha256: &str,
    ) -> Result<Self, PortalDeploymentManifestError> {
        validate_sha256(expected_sha256)?;
        let path = path.as_ref();
        let link_metadata =
            fs::symlink_metadata(path).map_err(|_| PortalDeploymentManifestError::InvalidFile)?;
        if link_metadata.file_type().is_symlink() || !link_metadata.file_type().is_file() {
            return Err(PortalDeploymentManifestError::InvalidFile);
        }
        let mut file = File::open(path).map_err(|_| PortalDeploymentManifestError::InvalidFile)?;
        let metadata = file
            .metadata()
            .map_err(|_| PortalDeploymentManifestError::InvalidFile)?;
        if !metadata.file_type().is_file()
            || metadata.len() == 0
            || metadata.len() > MAX_DEPLOYMENT_MANIFEST_BYTES as u64
        {
            return Err(PortalDeploymentManifestError::InvalidFile);
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.by_ref()
            .take((MAX_DEPLOYMENT_MANIFEST_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| PortalDeploymentManifestError::InvalidFile)?;
        if bytes.len() > MAX_DEPLOYMENT_MANIFEST_BYTES {
            return Err(PortalDeploymentManifestError::InvalidFile);
        }
        Self::from_bytes(&bytes, expected_sha256)
    }

    pub fn from_bytes(
        bytes: &[u8],
        expected_sha256: &str,
    ) -> Result<Self, PortalDeploymentManifestError> {
        validate_sha256(expected_sha256)?;
        if bytes.is_empty() || bytes.len() > MAX_DEPLOYMENT_MANIFEST_BYTES {
            return Err(PortalDeploymentManifestError::InvalidManifest);
        }
        if sha256_hex(bytes) != expected_sha256 {
            return Err(PortalDeploymentManifestError::DigestMismatch);
        }
        let mut deserializer = serde_json::Deserializer::from_slice(bytes);
        let manifest = Self::deserialize(&mut deserializer)
            .map_err(|_| PortalDeploymentManifestError::InvalidManifest)?;
        deserializer
            .end()
            .map_err(|_| PortalDeploymentManifestError::InvalidManifest)?;
        let canonical = serde_json::to_vec(&manifest)
            .map_err(|_| PortalDeploymentManifestError::InvalidManifest)?;
        if canonical != bytes {
            return Err(PortalDeploymentManifestError::InvalidManifest);
        }
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), PortalDeploymentManifestError> {
        if self.schema != "oxid-portal-deployment-v1"
            || self.portal_pr_head != PORTAL_PR_HEAD
            || self.profile_source_commit != PORTAL_PROFILE_SOURCE
            || self.provenance_sha256 != PORTAL_PROVENANCE_SHA256
        {
            return Err(PortalDeploymentManifestError::SourceLockMismatch);
        }
        validate_origin(&self.issuer_origin)?;
        validate_origin(&self.issuer_resolver_origin)?;
        validate_issuer_did(&self.issuer_did)?;
        if !self
            .issuer_method
            .starts_with(&format!("{}#", self.issuer_did))
            || self.issuer_method.len() > 2_048
            || self.issuer_method.chars().any(char::is_control)
        {
            return Err(PortalDeploymentManifestError::InvalidIssuer);
        }
        validate_sha256(&self.issuer_jubjub_jwk_sha256)
            .map_err(|_| PortalDeploymentManifestError::InvalidJwk)?;
        if self.issuer_jubjub_jwk.key_type != "EC"
            || self.issuer_jubjub_jwk.curve != "Jubjub"
            || decode_canonical_b64::<32>(&self.issuer_jubjub_jwk.x).is_err()
            || decode_canonical_b64::<32>(&self.issuer_jubjub_jwk.y).is_err()
            || sha256_hex(
                &serde_json::to_vec(&self.issuer_jubjub_jwk)
                    .map_err(|_| PortalDeploymentManifestError::InvalidJwk)?,
            ) != self.issuer_jubjub_jwk_sha256
        {
            return Err(PortalDeploymentManifestError::InvalidJwk);
        }
        authenticate_bundled_portal_source()?;
        Ok(())
    }

    #[must_use]
    pub fn issuer_origin(&self) -> &str {
        &self.issuer_origin
    }

    #[must_use]
    pub fn issuer_resolver_origin(&self) -> &str {
        &self.issuer_resolver_origin
    }

    #[must_use]
    pub fn issuer_did(&self) -> &str {
        &self.issuer_did
    }

    #[must_use]
    pub fn issuer_method(&self) -> &str {
        &self.issuer_method
    }

    #[must_use]
    pub fn issuer_jubjub_jwk(&self) -> &PortalPublicJwk {
        &self.issuer_jubjub_jwk
    }
}

/// Authenticates the exact raw upstream provenance and the immutable source
/// pins compiled into this native adapter.
pub fn authenticate_bundled_portal_source() -> Result<(), PortalDeploymentManifestError> {
    if sha256_hex(BUNDLED_PROVENANCE) != PORTAL_PROVENANCE_SHA256 {
        return Err(PortalDeploymentManifestError::SourceLockMismatch);
    }
    let lock = parse_strict_json(BUNDLED_SOURCE_LOCK)
        .map_err(|_| PortalDeploymentManifestError::SourceLockMismatch)?;
    let lock = lock
        .as_object()
        .ok_or(PortalDeploymentManifestError::SourceLockMismatch)?;
    let lock_keys = [
        "portalPrHead",
        "profileSourceCommit",
        "provenancePath",
        "provenanceSha256",
        "schema",
    ];
    if lock.len() != lock_keys.len()
        || !lock_keys.iter().all(|key| lock.contains_key(*key))
        || lock["portalPrHead"] != PORTAL_PR_HEAD
        || lock["profileSourceCommit"] != PORTAL_PROFILE_SOURCE
        || lock["provenancePath"] != "openid4vci-final/provenance.json"
        || lock["provenanceSha256"] != PORTAL_PROVENANCE_SHA256
        || lock["schema"] != "oxid-portal-source-lock-v1"
    {
        return Err(PortalDeploymentManifestError::SourceLockMismatch);
    }
    let value = parse_strict_json(BUNDLED_PROVENANCE)
        .map_err(|_| PortalDeploymentManifestError::SourceLockMismatch)?;
    if value["schema"] != "laceid-openid4vci-profile-provenance-v1"
        || value["portal"]["profileSourceCommit"] != PORTAL_PROFILE_SOURCE
        || value["portal"]["baselineCommit"] != "804de0a9e58cf48ece3cc6c24b2245bb70bc80f1"
        || value["profile"]["credentialConfigurationId"] != PORTAL_CONFIGURATION_ID
        || value["profile"]["name"] != "lace-id-portal-oxid-midnight-phase1"
        || value["profile"]["openid4vciVersion"] != "1.0 Final"
        || value["profile"]["representationFormat"] != PORTAL_FORMAT
    {
        return Err(PortalDeploymentManifestError::SourceLockMismatch);
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), PortalDeploymentManifestError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(PortalDeploymentManifestError::InvalidDigest);
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn validate_origin(value: &str) -> Result<(), PortalDeploymentManifestError> {
    let url = Url::parse(value).map_err(|_| PortalDeploymentManifestError::InvalidOrigin)?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || !matches!(url.scheme(), "https" | "http")
        || (url.scheme() == "http" && !host_is_loopback(&url))
        || url.origin().ascii_serialization() != value
    {
        return Err(PortalDeploymentManifestError::InvalidOrigin);
    }
    Ok(())
}

fn validate_issuer_did(value: &str) -> Result<(), PortalDeploymentManifestError> {
    let Some(identifier) = value.strip_prefix("did:midnight:") else {
        return Err(PortalDeploymentManifestError::InvalidIssuer);
    };
    let mut parts = identifier.split(':');
    let network = parts.next().unwrap_or_default();
    let address = parts.next().unwrap_or_default();
    if network.is_empty()
        || network.len() > 64
        || !network
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || address.len() != 64
        || !address
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || parts.next().is_some()
    {
        return Err(PortalDeploymentManifestError::InvalidIssuer);
    }
    Ok(())
}

fn decode_canonical_b64<const N: usize>(value: &str) -> Result<[u8; N], ()> {
    let bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ())?;
    let bytes = <[u8; N]>::try_from(bytes).map_err(|_| ())?;
    (general_purpose::URL_SAFE_NO_PAD.encode(bytes) == value)
        .then_some(bytes)
        .ok_or(())
}

/// Adapter-owned seam for converting opaque Portal private-material JSON to
/// credential-family bytes. It deliberately exposes no claim/application type.
pub trait PortalCredentialMaterialDecoder: Send + Sync {
    fn decode(
        &self,
        signed_credential: &[u8],
        portal_private_json: &[u8],
    ) -> Result<Vec<u8>, PortalCredentialMaterialError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalCredentialMaterialError {
    Invalid,
    Unavailable,
}

impl std::fmt::Display for PortalCredentialMaterialError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Invalid => "invalid credential private material",
            Self::Unavailable => "credential private material decoder unavailable",
        })
    }
}

impl std::error::Error for PortalCredentialMaterialError {}

struct PortalPreparedSecret {
    profile_id: String,
    issuer: String,
    configuration_id: String,
    token_endpoint: String,
    nonce_endpoint: String,
    credential_endpoint: String,
    pre_authorized_code: Zeroizing<String>,
}

/// Strict one-profile HTTP implementation of the existing issuance port.
pub struct PortalOid4vciClient {
    deployment: PortalDeploymentManifest,
    client: Client,
    proof: Arc<dyn CredentialHolderProofPort>,
    get_did: Arc<dyn GetDidRecordUseCase>,
    decoder: Arc<dyn PortalCredentialMaterialDecoder>,
    sessions: Mutex<BTreeMap<String, PortalPreparedSecret>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl PortalOid4vciClient {
    pub fn new(
        deployment: PortalDeploymentManifest,
        proof: Arc<dyn CredentialHolderProofPort>,
        get_did: Arc<dyn GetDidRecordUseCase>,
        decoder: Arc<dyn PortalCredentialMaterialDecoder>,
    ) -> Result<Self, PortalDeploymentManifestError> {
        deployment.validate()?;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let roots = TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|certificate| Certificate::from_der(certificate.as_ref()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| PortalDeploymentManifestError::ClientUnavailable)?;
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent("oxid-portal-openid4vci/0.1")
            .tls_certs_only(roots)
            .build()
            .map_err(|_| PortalDeploymentManifestError::ClientUnavailable)?;
        Ok(Self {
            deployment,
            client,
            proof,
            get_did,
            decoder,
            sessions: Mutex::new(BTreeMap::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        })
    }

    fn sessions(
        &self,
    ) -> Result<MutexGuard<'_, BTreeMap<String, PortalPreparedSecret>>, IssuanceProtocolError> {
        self.sessions
            .lock()
            .map_err(|_| IssuanceProtocolError::Unavailable)
    }

    fn next_id(&self) -> Result<CredentialIssuanceId, IssuanceProtocolError> {
        let value = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        CredentialIssuanceId::parse(format!("portal_{value:016x}"))
            .map_err(|_| IssuanceProtocolError::Unavailable)
    }

    fn endpoint(&self, path: &str) -> Result<Url, IssuanceProtocolError> {
        let value = format!("{}{path}", self.deployment.issuer_origin);
        validate_portal_endpoint(&value, self.deployment.issuer_origin(), path)
    }
}

impl CredentialIssuanceProtocolPort for PortalOid4vciClient {
    fn prepare<'a>(&'a self, request: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
        Box::pin(async move {
            let offer = parse_portal_offer(&request.offer, self.deployment.issuer_origin())?;
            let issuer_url = self.endpoint("/.well-known/openid-credential-issuer")?;
            let issuer_bytes = get_json(&self.client, issuer_url, MAX_METADATA_BYTES).await?;
            let issuer_metadata =
                parse_issuer_metadata(&issuer_bytes, EndpointPolicy::StandaloneLoopback)?;
            validate_portal_issuer_metadata_shape(&issuer_bytes)?;
            if issuer_metadata.issuer != offer.issuer
                || issuer_metadata.issuer != self.deployment.issuer_origin
                || issuer_metadata.authorization_servers != [self.deployment.issuer_origin.clone()]
                || issuer_metadata.configurations.len() != 1
                || !issuer_metadata
                    .configurations
                    .contains_key(PORTAL_CONFIGURATION_ID)
            {
                return Err(IssuanceProtocolError::InvalidMetadata);
            }
            validate_portal_endpoint(
                &issuer_metadata.credential_endpoint,
                self.deployment.issuer_origin(),
                "/api/issuer/credentials",
            )?;
            validate_portal_endpoint(
                &issuer_metadata.nonce_endpoint,
                self.deployment.issuer_origin(),
                "/api/issuer/nonce",
            )?;

            let authorization_url = self.endpoint("/.well-known/oauth-authorization-server")?;
            let authorization_bytes =
                get_json(&self.client, authorization_url, MAX_METADATA_BYTES).await?;
            let authorization = parse_portal_authorization_metadata(
                &authorization_bytes,
                self.deployment.issuer_origin(),
            )?;
            let display_names = offer
                .configuration_ids
                .iter()
                .map(|id| {
                    issuer_metadata
                        .configurations
                        .get(id)
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
            let id = self.next_id()?;
            let secret = PortalPreparedSecret {
                profile_id: request.profile_id.as_str().to_owned(),
                issuer: offer.issuer,
                configuration_id: PORTAL_CONFIGURATION_ID.to_owned(),
                token_endpoint: authorization,
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
            if secret.profile_id != request.profile_id.as_str()
                || request.method_id == request.holder_binding_method_id
            {
                return Err(IssuanceProtocolError::InvalidProof);
            }
            validate_managed_authentication_method(
                self.get_did.as_ref(),
                &request.profile_id,
                &request.holder_did,
                &request.method_id,
            )?;
            let binding = resolve_holder_binding(
                self.get_did.as_ref(),
                &request.profile_id,
                &request.holder_did,
                &request.holder_binding_method_id,
            )?;
            if binding.holder_binding_method_id != request.holder_binding_method_id {
                return Err(IssuanceProtocolError::InvalidProof);
            }

            validate_portal_endpoint(
                &secret.token_endpoint,
                self.deployment.issuer_origin(),
                "/api/issuer/token",
            )?;
            let token_response = self
                .client
                .post(&secret.token_endpoint)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .form(&[
                    ("grant_type", PRE_AUTHORIZED_GRANT),
                    ("pre-authorized_code", secret.pre_authorized_code.as_str()),
                ])
                .send()
                .await
                .map_err(|_| IssuanceProtocolError::Unavailable)?;
            let token_bytes = read_json_response(token_response, MAX_TOKEN_BYTES).await?;
            let access_token = Zeroizing::new(parse_token_response(&token_bytes)?);

            validate_portal_endpoint(
                &secret.nonce_endpoint,
                self.deployment.issuer_origin(),
                "/api/issuer/nonce",
            )?;
            let nonce_response = self
                .client
                .post(&secret.nonce_endpoint)
                .header(CONTENT_TYPE, "application/json")
                .body("{}")
                .send()
                .await
                .map_err(|_| IssuanceProtocolError::Unavailable)?;
            let nonce_bytes = read_json_response(nonce_response, MAX_NONCE_BYTES).await?;
            let nonce = Zeroizing::new(parse_nonce_response(&nonce_bytes)?);

            let proof = Zeroizing::new(
                self.proof
                    .create(HolderProofRequest {
                        profile_id: request.profile_id,
                        holder_did: request.holder_did.clone(),
                        method_id: request.method_id,
                        audience: secret.issuer.clone(),
                        nonce: nonce.to_string(),
                    })
                    .await
                    .map_err(map_holder_proof_error)?,
            );
            let credential_request = json!({
                "credential_configuration_id": secret.configuration_id,
                "midnight": {"holderBindingMethod": request.holder_binding_method_id},
                "proofs": {"jwt": [proof.as_str()]}
            });
            validate_portal_endpoint(
                &secret.credential_endpoint,
                self.deployment.issuer_origin(),
                "/api/issuer/credentials",
            )?;
            let credential_response = self
                .client
                .post(&secret.credential_endpoint)
                .bearer_auth(access_token.as_str())
                .json(&credential_request)
                .send()
                .await
                .map_err(|_| IssuanceProtocolError::Unavailable)?;
            let response_bytes =
                read_json_response(credential_response, MAX_CREDENTIAL_BYTES).await?;
            parse_portal_credential_response(
                &response_bytes,
                &request.holder_did,
                &binding.holder_binding_method_id,
                nonce.as_str(),
                self.decoder.as_ref(),
            )
        })
    }

    fn discard(&self, issuance_id: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError> {
        self.sessions()?
            .remove(issuance_id.as_str())
            .map(|_| ())
            .ok_or(IssuanceProtocolError::InvalidOffer)
    }
}

fn validate_managed_authentication_method(
    get_did: &dyn GetDidRecordUseCase,
    profile_id: &oxid_protocol_domain::ProtocolProfileId,
    holder_did: &str,
    method_id: &str,
) -> Result<(), IssuanceProtocolError> {
    let record = get_did
        .execute(oxid_identity_application::DidRecordQuery {
            profile_id: profile_id.as_str().to_owned(),
            did: holder_did.to_owned(),
        })
        .map_err(map_get_did_error)
        .map_err(map_holder_proof_error)?;
    if record.document_metadata.deactivated == Some(true)
        || !record
            .managed_method_ids
            .iter()
            .any(|managed| managed == method_id)
        || !record.document.relationships.iter().any(|relationship| {
            relationship.relationship == "authentication"
                && relationship
                    .method_ids
                    .iter()
                    .any(|value| value == method_id)
        })
        || !record.document.verification_methods.iter().any(|method| {
            method.id == method_id
                && method.controller == holder_did
                && matches!(
                    (
                        method.public_key_jwk.key_type.as_str(),
                        method.public_key_jwk.curve.as_str()
                    ),
                    ("OKP", "Ed25519") | ("EC", "P-256")
                )
        })
    {
        return Err(IssuanceProtocolError::InvalidProof);
    }
    Ok(())
}

fn parse_portal_offer(
    input: &str,
    expected_origin: &str,
) -> Result<ParsedOffer, IssuanceProtocolError> {
    let offer = parse_offer(input)?;
    let url = Url::parse(input).map_err(|_| IssuanceProtocolError::InvalidOffer)?;
    let pairs = url.query_pairs().collect::<Vec<_>>();
    let embedded = pairs
        .first()
        .filter(|_| pairs.len() == 1)
        .map(|(_, value)| value.as_bytes())
        .ok_or(IssuanceProtocolError::InvalidOffer)?;
    let value = parse_strict_json(embedded).map_err(|_| IssuanceProtocolError::InvalidOffer)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidOffer)?;
    exact_keys(
        object,
        &[
            "credential_configuration_ids",
            "credential_issuer",
            "grants",
        ],
        IssuanceProtocolError::InvalidOffer,
    )?;
    let grants = object
        .get("grants")
        .and_then(Value::as_object)
        .ok_or(IssuanceProtocolError::InvalidOffer)?;
    exact_keys(
        grants,
        &[PRE_AUTHORIZED_GRANT],
        IssuanceProtocolError::InvalidOffer,
    )?;
    let grant = grants
        .get(PRE_AUTHORIZED_GRANT)
        .and_then(Value::as_object)
        .ok_or(IssuanceProtocolError::InvalidOffer)?;
    exact_keys(
        grant,
        &["pre-authorized_code"],
        IssuanceProtocolError::InvalidOffer,
    )?;
    if offer.issuer != expected_origin
        || offer.configuration_ids != [PORTAL_CONFIGURATION_ID]
        || offer.authorization_server.is_some()
    {
        return Err(IssuanceProtocolError::InvalidOffer);
    }
    Ok(offer)
}

fn validate_portal_issuer_metadata_shape(bytes: &[u8]) -> Result<(), IssuanceProtocolError> {
    let value = parse_strict_json(bytes)?;
    let root = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    exact_keys(
        root,
        &[
            "authorization_servers",
            "credential_configurations_supported",
            "credential_endpoint",
            "credential_issuer",
            "nonce_endpoint",
        ],
        IssuanceProtocolError::InvalidMetadata,
    )?;
    let configurations = root
        .get("credential_configurations_supported")
        .and_then(Value::as_object)
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    exact_keys(
        configurations,
        &[PORTAL_CONFIGURATION_ID],
        IssuanceProtocolError::InvalidMetadata,
    )?;
    let configuration = configurations[PORTAL_CONFIGURATION_ID]
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    exact_keys(
        configuration,
        &[
            "credential_metadata",
            "cryptographic_binding_methods_supported",
            "format",
            "proof_types_supported",
            "scope",
        ],
        IssuanceProtocolError::InvalidMetadata,
    )?;
    if required_unique_strings(
        configuration,
        "cryptographic_binding_methods_supported",
        1,
        32,
    )? != ["did"]
        || required_string(configuration, "scope", 64)? != "digital-passport"
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let proof_types = required_object(configuration, "proof_types_supported")?;
    exact_keys(
        proof_types,
        &["jwt"],
        IssuanceProtocolError::InvalidMetadata,
    )?;
    let jwt = required_object(proof_types, "jwt")?;
    exact_keys(
        jwt,
        &["proof_signing_alg_values_supported"],
        IssuanceProtocolError::InvalidMetadata,
    )?;
    if required_unique_strings(jwt, "proof_signing_alg_values_supported", 2, 16)?
        != ["EdDSA", "ES256"]
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let metadata = required_object(configuration, "credential_metadata")?;
    exact_keys(
        metadata,
        &["display"],
        IssuanceProtocolError::InvalidMetadata,
    )?;
    let displays = metadata
        .get("display")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 1)
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    let display = displays[0]
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    exact_keys(
        display,
        &["locale", "name"],
        IssuanceProtocolError::InvalidMetadata,
    )?;
    if required_string(display, "locale", 16)? != "en"
        || required_string(display, "name", 256)? != "Digital Passport"
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    Ok(())
}

fn parse_portal_authorization_metadata(
    bytes: &[u8],
    expected_origin: &str,
) -> Result<String, IssuanceProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    exact_keys(
        object,
        &[
            "grant_types_supported",
            "issuer",
            "pre-authorized_grant_anonymous_access_supported",
            "token_endpoint",
        ],
        IssuanceProtocolError::InvalidMetadata,
    )?;
    if required_string(object, "issuer", 2_048)? != expected_origin
        || object
            .get("pre-authorized_grant_anonymous_access_supported")
            .and_then(Value::as_bool)
            != Some(true)
        || required_unique_strings(object, "grant_types_supported", 1, 256)?
            != [PRE_AUTHORIZED_GRANT]
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let token = required_string(object, "token_endpoint", 2_048)?;
    validate_portal_endpoint(&token, expected_origin, "/api/issuer/token")?;
    Ok(token)
}

fn parse_token_response(bytes: &[u8]) -> Result<String, IssuanceProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::IssuerRejected)?;
    exact_keys(
        object,
        &["access_token", "expires_in", "token_type"],
        IssuanceProtocolError::IssuerRejected,
    )?;
    let token = object
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_SECRET_BYTES)
        .ok_or(IssuanceProtocolError::IssuerRejected)?;
    if object.get("token_type").and_then(Value::as_str) != Some("Bearer")
        || object.get("expires_in").and_then(Value::as_u64).is_none()
    {
        return Err(IssuanceProtocolError::IssuerRejected);
    }
    Ok(token.to_owned())
}

fn parse_nonce_response(bytes: &[u8]) -> Result<String, IssuanceProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::IssuerRejected)?;
    exact_keys(
        object,
        &["c_nonce", "c_nonce_expires_in"],
        IssuanceProtocolError::IssuerRejected,
    )?;
    if object
        .get("c_nonce_expires_in")
        .and_then(Value::as_u64)
        .is_none()
    {
        return Err(IssuanceProtocolError::IssuerRejected);
    }
    object
        .get("c_nonce")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_SECRET_BYTES)
        .map(str::to_owned)
        .ok_or(IssuanceProtocolError::IssuerRejected)
}

fn parse_portal_credential_response(
    bytes: &[u8],
    expected_holder_did: &str,
    expected_binding_method: &str,
    expected_nonce: &str,
    decoder: &dyn PortalCredentialMaterialDecoder,
) -> Result<IssuedCredentialBytes, IssuanceProtocolError> {
    let value =
        parse_strict_json(bytes).map_err(|_| IssuanceProtocolError::InvalidCredentialResponse)?;
    let root = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    exact_keys(
        root,
        &["credentials"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    let credentials = root
        .get("credentials")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 1)
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    let item = credentials[0]
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    exact_keys(
        item,
        &["credential", "midnight"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    let signed = item
        .get("credential")
        .and_then(Value::as_str)
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)
        .and_then(decode_payload)?;
    let midnight = required_response_object(item, "midnight")?;
    exact_keys(
        midnight,
        &[
            "credentialFamily",
            "credentialPrivateParts",
            "credentialProof",
            "encoding",
            "expiresAt",
            "hasExpiration",
            "holderBinding",
            "schemaId",
            "schemaVersion",
        ],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    if response_string(midnight, "credentialFamily")? != PORTAL_FAMILY
        || response_string(midnight, "encoding")? != PORTAL_ENCODING
        || response_string(midnight, "schemaId")? != PORTAL_SCHEMA_ID
        || response_string(midnight, "schemaVersion")? != PORTAL_SCHEMA_VERSION
        || midnight
            .get("hasExpiration")
            .and_then(Value::as_bool)
            .is_none()
        || response_string(midnight, "expiresAt")?.len() > 64
    {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let proof = required_response_object(midnight, "credentialProof")?;
    exact_keys(
        proof,
        &["encoding", "payload"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    if response_string(proof, "encoding")? != PORTAL_ENCODING {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let detached_proof = decode_payload(response_string(proof, "payload")?)?;
    let holder = required_response_object(midnight, "holderBinding")?;
    exact_keys(
        holder,
        &["challenge", "holderDidMethod", "method"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    if response_string(holder, "challenge")? != expected_nonce
        || response_string(holder, "method")? != "explicit_did_method"
    {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let method = required_response_object(holder, "holderDidMethod")?;
    exact_keys(
        method,
        &["did", "keyType", "methodId"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    if response_string(method, "did")? != expected_holder_did
        || response_string(method, "methodId")? != expected_binding_method
        || response_string(method, "keyType")? != "jubjub"
    {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let private_value = midnight
        .get("credentialPrivateParts")
        .filter(|value| value.is_object())
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    let private_json = Zeroizing::new(
        serde_json::to_vec(private_value)
            .map_err(|_| IssuanceProtocolError::InvalidCredentialResponse)?,
    );
    let private_material = decoder
        .decode(&signed, private_json.as_slice())
        .map_err(|error| match error {
            PortalCredentialMaterialError::Invalid => {
                IssuanceProtocolError::InvalidCredentialResponse
            }
            PortalCredentialMaterialError::Unavailable => {
                IssuanceProtocolError::ProtectionUnavailable
            }
        })?;
    if signed.is_empty() || detached_proof.is_empty() || private_material.is_empty() {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    Ok(IssuedCredentialBytes {
        signed_bytes: signed,
        detached_proof: Some(detached_proof),
        private_material: Some(private_material),
    })
}

fn decode_payload(value: &str) -> Result<Vec<u8>, IssuanceProtocolError> {
    if value.is_empty() || value.len() > MAX_CREDENTIAL_BYTES * 2 {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| IssuanceProtocolError::InvalidCredentialResponse)?;
    if general_purpose::URL_SAFE_NO_PAD.encode(&bytes) != value {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    Ok(bytes)
}

fn required_response_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, IssuanceProtocolError> {
    object
        .get(key)
        .and_then(Value::as_object)
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)
}

fn response_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, IssuanceProtocolError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= 2_048 && !value.chars().any(char::is_control)
        })
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)
}

fn exact_keys(
    object: &Map<String, Value>,
    expected: &[&str],
    error: IssuanceProtocolError,
) -> Result<(), IssuanceProtocolError> {
    if object.len() != expected.len() || !expected.iter().all(|key| object.contains_key(*key)) {
        return Err(error);
    }
    Ok(())
}

fn validate_portal_endpoint(
    value: &str,
    expected_origin: &str,
    expected_path: &str,
) -> Result<Url, IssuanceProtocolError> {
    let endpoint = validate_endpoint(value, EndpointPolicy::StandaloneLoopback)?;
    if endpoint.origin().ascii_serialization() != expected_origin
        || endpoint.path() != expected_path
        || endpoint.query().is_some()
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    Ok(endpoint)
}

async fn get_json(
    client: &Client,
    url: Url,
    limit: usize,
) -> Result<Vec<u8>, IssuanceProtocolError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| IssuanceProtocolError::Unavailable)?;
    read_json_response(response, limit).await
}

async fn read_json_response(
    response: Response,
    limit: usize,
) -> Result<Vec<u8>, IssuanceProtocolError> {
    if response.status() != StatusCode::OK {
        return Err(IssuanceProtocolError::IssuerRejected);
    }
    if response.headers().contains_key(CONTENT_ENCODING) {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    validate_json_content_type(content_type)?;
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > limit as u64)
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| IssuanceProtocolError::Unavailable)?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(IssuanceProtocolError::InvalidMetadata);
        }
        bytes.extend_from_slice(&chunk);
    }
    std::str::from_utf8(&bytes).map_err(|_| IssuanceProtocolError::InvalidMetadata)?;
    Ok(bytes)
}

fn validate_json_content_type(value: &str) -> Result<(), IssuanceProtocolError> {
    let mut parts = value.split(';');
    if parts.next().map(str::trim) != Some("application/json") {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    match parts.next().map(str::trim) {
        None | Some("") => {}
        Some(parameter) if parameter.eq_ignore_ascii_case("charset=utf-8") => {}
        _ => return Err(IssuanceProtocolError::InvalidMetadata),
    }
    if parts.any(|part| !part.trim().is_empty()) {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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

    fn deployment(origin: &str) -> PortalDeploymentManifest {
        let jwk = PortalPublicJwk {
            curve: "Jubjub".to_owned(),
            key_type: "EC".to_owned(),
            x: general_purpose::URL_SAFE_NO_PAD.encode([7_u8; 32]),
            y: general_purpose::URL_SAFE_NO_PAD.encode([9_u8; 32]),
        };
        let jwk_digest = sha256_hex(&serde_json::to_vec(&jwk).expect("jwk"));
        let manifest = PortalDeploymentManifest {
            issuer_did: "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
            issuer_jubjub_jwk: jwk,
            issuer_jubjub_jwk_sha256: jwk_digest,
            issuer_method: "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef#key-assert".to_owned(),
            issuer_origin: origin.to_owned(),
            issuer_resolver_origin: origin.to_owned(),
            portal_pr_head: PORTAL_PR_HEAD.to_owned(),
            profile_source_commit: PORTAL_PROFILE_SOURCE.to_owned(),
            provenance_sha256: PORTAL_PROVENANCE_SHA256.to_owned(),
            schema: "oxid-portal-deployment-v1".to_owned(),
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
                    if let Some(position) = bytes.windows(4).position(|value| value == b"\r\n\r\n")
                    {
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
        let offer =
            std::fs::read_to_string(format!("{POSITIVE_ROOT}/positive/credential-offer.txt"))
                .expect("offer");
        parse_portal_offer(offer.trim(), "https://issuer.example").expect("positive offer");
        let issuer = std::fs::read(format!(
            "{POSITIVE_ROOT}/positive/credential-issuer-metadata.json"
        ))
        .expect("issuer metadata");
        validate_portal_issuer_metadata_shape(&issuer)
            .expect("exact positive issuer metadata shape");
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
    async fn exact_http_flow_uses_form_token_post_nonce_managed_proof_and_distinct_jubjub_binding()
    {
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

    async fn oversized_metadata_response(
        response: Vec<u8>,
    ) -> (String, tokio::task::JoinHandle<()>) {
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
    fn hostile_urls_content_types_sizes_and_legacy_shapes_fail_closed() {
        for url in [
            "http://issuer.example/api/issuer/token",
            "https://user:pass@issuer.example/api/issuer/token",
            "https://issuer.example/api/issuer/token#fragment",
            "https://issuer.example/other",
        ] {
            assert!(
                validate_portal_endpoint(url, "https://issuer.example", "/api/issuer/token")
                    .is_err()
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
}
