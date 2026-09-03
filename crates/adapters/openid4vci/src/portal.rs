// SPDX-License-Identifier: Apache-2.0

//! Strict native HTTP client for the authenticated Portal Final profile.

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

#[path = "portal_response.rs"]
mod response;
use response::*;

pub const PORTAL_INTEGRATION_COMMIT: &str = "25499870f84d77173c46e4af3021311decfb840b";
pub const PORTAL_INTEGRATION_TREE: &str = "2d845d2293603dfd8adce5362c8a9941e6ba78a9";
pub const PORTAL_PROFILE_SOURCE_COMMIT: &str = "76e8edf394a4cb37ca822037272d543c68f25f71";
pub const PORTAL_PROVENANCE_SHA256: &str =
    "63d2dd182f1a315d8fe7677ae6481aecebd2fd9cff709cc438b6c0261a3cf4c7";
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
    "../../../../fixtures/laceid-portal/25499870f84d77173c46e4af3021311decfb840b/source-lock.json"
);
const BUNDLED_PROVENANCE: &[u8] = include_bytes!(
    "../../../../fixtures/laceid-portal/25499870f84d77173c46e4af3021311decfb840b/openid4vci-final/provenance.json"
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
    integration_commit: String,
    integration_tree: String,
    issuer_did: String,
    issuer_jubjub_jwk: PortalPublicJwk,
    issuer_jubjub_jwk_sha256: String,
    issuer_method: String,
    issuer_origin: String,
    issuer_resolver_origin: String,
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            if link_metadata.dev() != metadata.dev() || link_metadata.ino() != metadata.ino() {
                return Err(PortalDeploymentManifestError::InvalidFile);
            }
        }
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
        if self.schema != "oxid-portal-deployment-v3"
            || self.integration_commit != PORTAL_INTEGRATION_COMMIT
            || self.integration_tree != PORTAL_INTEGRATION_TREE
            || self.provenance_sha256 != PORTAL_PROVENANCE_SHA256
        {
            return Err(PortalDeploymentManifestError::SourceLockMismatch);
        }
        validate_origin(&self.issuer_origin)?;
        validate_resolver_base(&self.issuer_resolver_origin)?;
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

    #[must_use]
    pub fn issuer_jubjub_jwk_sha256(&self) -> &str {
        &self.issuer_jubjub_jwk_sha256
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
        "integrationCommit",
        "integrationTree",
        "profileSourceCommit",
        "provenancePath",
        "provenanceSha256",
        "schema",
    ];
    if lock.len() != lock_keys.len()
        || !lock_keys.iter().all(|key| lock.contains_key(*key))
        || lock["integrationCommit"] != PORTAL_INTEGRATION_COMMIT
        || lock["integrationTree"] != PORTAL_INTEGRATION_TREE
        || lock["profileSourceCommit"] != PORTAL_PROFILE_SOURCE_COMMIT
        || lock["provenancePath"] != "openid4vci-final/provenance.json"
        || lock["provenanceSha256"] != PORTAL_PROVENANCE_SHA256
        || lock["schema"] != "oxid-portal-source-lock-v3"
    {
        return Err(PortalDeploymentManifestError::SourceLockMismatch);
    }
    let value = parse_strict_json(BUNDLED_PROVENANCE)
        .map_err(|_| PortalDeploymentManifestError::SourceLockMismatch)?;
    if value["schema"] != "laceid-openid4vci-profile-provenance-v1"
        || value["portal"]["profileSourceCommit"] != PORTAL_PROFILE_SOURCE_COMMIT
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
    let url = validate_transport_base(value)?;
    if url.path() != "/" || url.origin().ascii_serialization() != value {
        return Err(PortalDeploymentManifestError::InvalidOrigin);
    }
    Ok(())
}

fn validate_resolver_base(value: &str) -> Result<(), PortalDeploymentManifestError> {
    let url = validate_transport_base(value)?;
    let canonical = if url.path() == "/" {
        url.origin().ascii_serialization() == value
    } else {
        url.as_str() == value
    };
    if !canonical
        || (url.path() != "/"
            && (url.path().ends_with('/')
                || url.path().contains("//")
                || url
                    .path()
                    .split('/')
                    .any(|segment| matches!(segment, "." | ".."))))
    {
        return Err(PortalDeploymentManifestError::InvalidOrigin);
    }
    Ok(())
}

fn validate_transport_base(value: &str) -> Result<Url, PortalDeploymentManifestError> {
    let url = Url::parse(value).map_err(|_| PortalDeploymentManifestError::InvalidOrigin)?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.scheme(), "https" | "http")
        || (url.scheme() == "http" && !host_is_loopback(&url))
    {
        return Err(PortalDeploymentManifestError::InvalidOrigin);
    }
    Ok(url)
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

struct PortalRuntime {
    handle: tokio::runtime::Handle,
    shutdown: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl PortalRuntime {
    fn new() -> Result<Self, PortalDeploymentManifestError> {
        let (handle_sender, handle_receiver) = std::sync::mpsc::sync_channel(1);
        let (shutdown_sender, shutdown_receiver) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("oxid-portal-http".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build();
                let Ok(runtime) = runtime else {
                    return;
                };
                if handle_sender.send(runtime.handle().clone()).is_err() {
                    return;
                }
                let _ = shutdown_receiver.recv();
            })
            .map_err(|_| PortalDeploymentManifestError::ClientUnavailable)?;
        let handle = handle_receiver
            .recv()
            .map_err(|_| PortalDeploymentManifestError::ClientUnavailable)?;
        Ok(Self {
            handle,
            shutdown: Mutex::new(Some(shutdown_sender)),
            thread: Mutex::new(Some(thread)),
        })
    }
}

impl Drop for PortalRuntime {
    fn drop(&mut self) {
        if let Ok(shutdown) = self.shutdown.get_mut()
            && let Some(shutdown) = shutdown.take()
        {
            let _ = shutdown.send(());
        }
        if let Ok(thread) = self.thread.get_mut()
            && let Some(thread) = thread.take()
        {
            let _ = thread.join();
        }
    }
}

/// Authenticated, preflighted Portal client factory owned by native headless
/// composition. Building the request client is fallible only at startup; the
/// later application-port wiring is infallible and cannot silently fall back.
#[derive(Clone)]
pub struct PortalOid4vciClientFactory {
    deployment: PortalDeploymentManifest,
    client: Client,
    runtime: Arc<PortalRuntime>,
}

impl PortalOid4vciClientFactory {
    pub fn new(
        deployment: PortalDeploymentManifest,
    ) -> Result<Self, PortalDeploymentManifestError> {
        deployment.validate()?;
        let runtime = Arc::new(PortalRuntime::new()?);
        Ok(Self {
            deployment,
            client: build_portal_http_client()?,
            runtime,
        })
    }

    #[must_use]
    pub fn deployment(&self) -> &PortalDeploymentManifest {
        &self.deployment
    }

    #[must_use]
    pub fn build(
        self,
        proof: Arc<dyn CredentialHolderProofPort>,
        get_did: Arc<dyn GetDidRecordUseCase>,
        decoder: Arc<dyn PortalCredentialMaterialDecoder>,
    ) -> PortalOid4vciClient {
        PortalOid4vciClient {
            deployment: self.deployment,
            client: self.client,
            runtime: self.runtime,
            proof,
            get_did,
            decoder,
            sessions: Mutex::new(BTreeMap::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }
}

fn build_portal_http_client() -> Result<Client, PortalDeploymentManifestError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let roots = TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|certificate| Certificate::from_der(certificate.as_ref()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PortalDeploymentManifestError::ClientUnavailable)?;
    Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent("oxid-portal-openid4vci/0.1")
        .tls_certs_only(roots)
        .build()
        .map_err(|_| PortalDeploymentManifestError::ClientUnavailable)
}

/// Strict one-profile HTTP implementation of the existing issuance port.
pub struct PortalOid4vciClient {
    deployment: PortalDeploymentManifest,
    client: Client,
    runtime: Arc<PortalRuntime>,
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
        Ok(PortalOid4vciClientFactory::new(deployment)?.build(proof, get_did, decoder))
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

impl PortalOid4vciClient {
    async fn prepare_inner(
        &self,
        request: PrepareIssuanceRequest,
    ) -> Result<PreparedCredentialOffer, IssuanceProtocolError> {
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
    }

    async fn issue_inner(
        &self,
        request: ProtocolIssueRequest,
    ) -> Result<IssuedCredentialBytes, IssuanceProtocolError> {
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
            Zeroizing::new(read_json_response(credential_response, MAX_CREDENTIAL_BYTES).await?);
        parse_portal_credential_response(
            &response_bytes,
            &request.holder_did,
            &binding.holder_binding_method_id,
            nonce.as_str(),
            self.decoder.as_ref(),
        )
    }
}

impl CredentialIssuanceProtocolPort for PortalOid4vciClient {
    fn prepare<'a>(&'a self, request: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
        Box::pin(async move {
            if tokio::runtime::Handle::try_current().is_ok() {
                return self.prepare_inner(request).await;
            }
            std::thread::scope(|scope| {
                scope
                    .spawn(move || self.runtime.handle.block_on(self.prepare_inner(request)))
                    .join()
            })
            .map_err(|_| IssuanceProtocolError::Unavailable)?
        })
    }

    fn issue<'a>(&'a self, request: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a> {
        Box::pin(async move {
            if tokio::runtime::Handle::try_current().is_ok() {
                return self.issue_inner(request).await;
            }
            std::thread::scope(|scope| {
                scope
                    .spawn(move || self.runtime.handle.block_on(self.issue_inner(request)))
                    .join()
            })
            .map_err(|_| IssuanceProtocolError::Unavailable)?
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

#[cfg(test)]
#[path = "portal_internal_tests.rs"]
mod tests;
