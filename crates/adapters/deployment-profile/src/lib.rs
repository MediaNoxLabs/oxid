// SPDX-License-Identifier: Apache-2.0

//! Authentication boundary for immutable production deployment profiles.
//!
//! The signed payload binds every network and SSI route to one Midnight chain
//! identity. Runtime environment variables never enter this adapter.

#![forbid(unsafe_code)]

use std::{collections::BTreeMap, error::Error, fmt};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use url::Url;

/// Signed-envelope and payload format understood by this adapter.
pub const DEPLOYMENT_PROFILE_FORMAT: &str = "oxid.deployment-profile.v1";

const SIGNATURE_ALGORITHM: &str = "Ed25519";
const MAX_ENVELOPE_BYTES: usize = 64 * 1024;
const MAX_PAYLOAD_BYTES: usize = 48 * 1024;
const MAX_IDENTIFIER_CHARACTERS: usize = 128;
const MAX_ENDPOINT_CHARACTERS: usize = 2_048;

/// One reviewed public key and its bounded rotation/revocation policy.
#[derive(Clone)]
pub struct DeploymentTrustRoot {
    key_id: String,
    verifying_key: VerifyingKey,
    valid_from_seconds: u64,
    valid_until_seconds: u64,
    revoked_at_seconds: Option<u64>,
    minimum_profile_sequence: u64,
}

impl fmt::Debug for DeploymentTrustRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeploymentTrustRoot")
            .field("key_id", &self.key_id)
            .field("valid_from_seconds", &self.valid_from_seconds)
            .field("valid_until_seconds", &self.valid_until_seconds)
            .field("revoked", &self.revoked_at_seconds.is_some())
            .field("minimum_profile_sequence", &self.minimum_profile_sequence)
            .finish_non_exhaustive()
    }
}

impl DeploymentTrustRoot {
    /// Constructs one trust root from reviewed public material.
    pub fn new(
        key_id: impl Into<String>,
        verifying_key: [u8; 32],
        valid_from_seconds: u64,
        valid_until_seconds: u64,
        revoked_at_seconds: Option<u64>,
        minimum_profile_sequence: u64,
    ) -> Result<Self, DeploymentProfileError> {
        let key_id = key_id.into();
        validate_identifier(&key_id)?;
        if valid_from_seconds >= valid_until_seconds
            || revoked_at_seconds.is_some_and(|revoked| revoked < valid_from_seconds)
        {
            return Err(DeploymentProfileError::InvalidTrustRoot);
        }
        let verifying_key = VerifyingKey::from_bytes(&verifying_key)
            .map_err(|_| DeploymentProfileError::InvalidTrustRoot)?;
        Ok(Self {
            key_id,
            verifying_key,
            valid_from_seconds,
            valid_until_seconds,
            revoked_at_seconds,
            minimum_profile_sequence,
        })
    }
}

/// Verifies signed profile envelopes against a reviewed set of trust roots.
pub struct DeploymentProfileVerifier {
    expected_audience: String,
    roots: BTreeMap<String, DeploymentTrustRoot>,
    minimum_profile_sequence: u64,
}

impl DeploymentProfileVerifier {
    /// Builds a verifier. Duplicate key identifiers are rejected instead of
    /// silently choosing one rotation epoch.
    pub fn new(
        expected_audience: impl Into<String>,
        roots: impl IntoIterator<Item = DeploymentTrustRoot>,
        minimum_profile_sequence: u64,
    ) -> Result<Self, DeploymentProfileError> {
        let expected_audience = expected_audience.into();
        validate_identifier(&expected_audience)?;
        let mut indexed = BTreeMap::new();
        for root in roots {
            let key_id = root.key_id.clone();
            if indexed.insert(key_id, root).is_some() {
                return Err(DeploymentProfileError::DuplicateTrustRoot);
            }
        }
        if indexed.is_empty() {
            return Err(DeploymentProfileError::MissingTrustRoot);
        }
        Ok(Self {
            expected_audience,
            roots: indexed,
            minimum_profile_sequence,
        })
    }

    /// Authenticates one complete profile at a caller-supplied trusted time.
    ///
    /// The signature covers the exact canonical payload bytes. No endpoint,
    /// network, or SSI field can be supplied separately after verification.
    pub fn verify(
        &self,
        envelope_bytes: &[u8],
        now_seconds: u64,
    ) -> Result<AuthenticatedDeploymentProfile, DeploymentProfileError> {
        if envelope_bytes.is_empty() || envelope_bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(DeploymentProfileError::InvalidEnvelope);
        }
        let envelope: SignedEnvelope = serde_json::from_slice(envelope_bytes)
            .map_err(|_| DeploymentProfileError::InvalidEnvelope)?;
        if envelope.format != DEPLOYMENT_PROFILE_FORMAT || envelope.algorithm != SIGNATURE_ALGORITHM
        {
            return Err(DeploymentProfileError::UnsupportedEnvelope);
        }
        validate_identifier(&envelope.key_id)?;
        let root = self
            .roots
            .get(&envelope.key_id)
            .ok_or(DeploymentProfileError::UntrustedSigner)?;
        if now_seconds < root.valid_from_seconds || now_seconds >= root.valid_until_seconds {
            return Err(DeploymentProfileError::InactiveTrustRoot);
        }
        if root
            .revoked_at_seconds
            .is_some_and(|revoked| now_seconds >= revoked)
        {
            return Err(DeploymentProfileError::RevokedTrustRoot);
        }

        let payload_bytes = URL_SAFE_NO_PAD
            .decode(envelope.payload.as_bytes())
            .map_err(|_| DeploymentProfileError::InvalidEnvelope)?;
        if payload_bytes.is_empty() || payload_bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DeploymentProfileError::InvalidPayload);
        }
        let signature_bytes = URL_SAFE_NO_PAD
            .decode(envelope.signature.as_bytes())
            .map_err(|_| DeploymentProfileError::InvalidEnvelope)?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|_| DeploymentProfileError::InvalidSignature)?;
        root.verifying_key
            .verify_strict(&payload_bytes, &signature)
            .map_err(|_| DeploymentProfileError::InvalidSignature)?;

        let payload: DeploymentPayload = serde_json::from_slice(&payload_bytes)
            .map_err(|_| DeploymentProfileError::InvalidPayload)?;
        let canonical =
            serde_json::to_vec(&payload).map_err(|_| DeploymentProfileError::InvalidPayload)?;
        if canonical != payload_bytes {
            return Err(DeploymentProfileError::NonCanonicalPayload);
        }
        if payload.format != DEPLOYMENT_PROFILE_FORMAT {
            return Err(DeploymentProfileError::UnsupportedPayload);
        }
        if payload.audience != self.expected_audience {
            return Err(DeploymentProfileError::AudienceMismatch);
        }
        validate_identifier(&payload.profile_id)?;
        let minimum_sequence = self
            .minimum_profile_sequence
            .max(root.minimum_profile_sequence);
        if payload.sequence < minimum_sequence {
            return Err(DeploymentProfileError::ProfileRollback);
        }
        if payload.valid_from_seconds >= payload.valid_until_seconds {
            return Err(DeploymentProfileError::InvalidValidityWindow);
        }
        if now_seconds < payload.valid_from_seconds {
            return Err(DeploymentProfileError::ProfileNotYetValid);
        }
        if now_seconds >= payload.valid_until_seconds {
            return Err(DeploymentProfileError::StaleProfile);
        }

        validate_midnight(&payload.midnight)?;
        validate_ssi(&payload.ssi)?;
        let genesis_hash: [u8; 32] = hex::decode(&payload.midnight.genesis_hash_hex)
            .map_err(|_| DeploymentProfileError::InvalidChainIdentity)?
            .try_into()
            .map_err(|_| DeploymentProfileError::InvalidChainIdentity)?;

        Ok(AuthenticatedDeploymentProfile {
            profile_id: payload.profile_id,
            sequence: payload.sequence,
            valid_until_seconds: payload.valid_until_seconds,
            signing_key_id: envelope.key_id,
            midnight: AuthenticatedMidnightDeployment {
                network_id: payload.midnight.network_id,
                genesis_hash,
                indexer_http_url: payload.midnight.indexer_http_url,
                indexer_websocket_url: payload.midnight.indexer_websocket_url,
                node_websocket_url: payload.midnight.node_websocket_url,
                proof_server_url: payload.midnight.proof_server_url,
            },
            ssi: AuthenticatedSsiDeployment {
                did_resolver_url: payload.ssi.did_resolver_url,
                issuer_metadata_url: payload.ssi.issuer_metadata_url,
                verifier_metadata_url: payload.ssi.verifier_metadata_url,
            },
        })
    }
}

/// An immutable profile that can only be constructed after authentication.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthenticatedDeploymentProfile {
    profile_id: String,
    sequence: u64,
    valid_until_seconds: u64,
    signing_key_id: String,
    midnight: AuthenticatedMidnightDeployment,
    ssi: AuthenticatedSsiDeployment,
}

impl fmt::Debug for AuthenticatedDeploymentProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedDeploymentProfile")
            .field("profile_id", &self.profile_id)
            .field("sequence", &self.sequence)
            .field("valid_until_seconds", &self.valid_until_seconds)
            .field("signing_key_id", &self.signing_key_id)
            .field("midnight_network_id", &self.midnight.network_id)
            .finish_non_exhaustive()
    }
}

impl AuthenticatedDeploymentProfile {
    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn valid_until_seconds(&self) -> u64 {
        self.valid_until_seconds
    }

    #[must_use]
    pub fn signing_key_id(&self) -> &str {
        &self.signing_key_id
    }

    #[must_use]
    pub const fn midnight(&self) -> &AuthenticatedMidnightDeployment {
        &self.midnight
    }

    #[must_use]
    pub const fn ssi(&self) -> &AuthenticatedSsiDeployment {
        &self.ssi
    }
}

/// Authenticated Midnight routes and the chain identity they must expose.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthenticatedMidnightDeployment {
    network_id: String,
    genesis_hash: [u8; 32],
    indexer_http_url: String,
    indexer_websocket_url: String,
    node_websocket_url: String,
    proof_server_url: String,
}

impl fmt::Debug for AuthenticatedMidnightDeployment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedMidnightDeployment")
            .field("network_id", &self.network_id)
            .finish_non_exhaustive()
    }
}

impl AuthenticatedMidnightDeployment {
    #[must_use]
    pub fn network_id(&self) -> &str {
        &self.network_id
    }

    #[must_use]
    pub const fn genesis_hash(&self) -> &[u8; 32] {
        &self.genesis_hash
    }

    #[must_use]
    pub fn indexer_http_url(&self) -> &str {
        &self.indexer_http_url
    }

    #[must_use]
    pub fn indexer_websocket_url(&self) -> &str {
        &self.indexer_websocket_url
    }

    #[must_use]
    pub fn node_websocket_url(&self) -> &str {
        &self.node_websocket_url
    }

    #[must_use]
    pub fn proof_server_url(&self) -> &str {
        &self.proof_server_url
    }
}

/// Authenticated SSI discovery locations from the same signed payload.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthenticatedSsiDeployment {
    did_resolver_url: String,
    issuer_metadata_url: String,
    verifier_metadata_url: String,
}

impl fmt::Debug for AuthenticatedSsiDeployment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedSsiDeployment")
            .finish_non_exhaustive()
    }
}

impl AuthenticatedSsiDeployment {
    #[must_use]
    pub fn did_resolver_url(&self) -> &str {
        &self.did_resolver_url
    }

    #[must_use]
    pub fn issuer_metadata_url(&self) -> &str {
        &self.issuer_metadata_url
    }

    #[must_use]
    pub fn verifier_metadata_url(&self) -> &str {
        &self.verifier_metadata_url
    }
}

/// Stable, payload-free authentication failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeploymentProfileError {
    InvalidEnvelope,
    UnsupportedEnvelope,
    MissingTrustRoot,
    DuplicateTrustRoot,
    InvalidTrustRoot,
    UntrustedSigner,
    InactiveTrustRoot,
    RevokedTrustRoot,
    InvalidSignature,
    InvalidPayload,
    NonCanonicalPayload,
    UnsupportedPayload,
    AudienceMismatch,
    InvalidIdentifier,
    InvalidValidityWindow,
    ProfileNotYetValid,
    StaleProfile,
    ProfileRollback,
    InvalidNetwork,
    InvalidChainIdentity,
    InvalidEndpoint,
}

impl fmt::Display for DeploymentProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidEnvelope => "deployment profile envelope is invalid",
            Self::UnsupportedEnvelope => "deployment profile envelope is unsupported",
            Self::MissingTrustRoot => "deployment trust roots are unavailable",
            Self::DuplicateTrustRoot => "deployment trust roots conflict",
            Self::InvalidTrustRoot => "deployment trust root is invalid",
            Self::UntrustedSigner => "deployment profile signer is not trusted",
            Self::InactiveTrustRoot => "deployment trust root is not active",
            Self::RevokedTrustRoot => "deployment trust root is revoked",
            Self::InvalidSignature => "deployment profile signature is invalid",
            Self::InvalidPayload => "deployment profile payload is invalid",
            Self::NonCanonicalPayload => "deployment profile payload is not canonical",
            Self::UnsupportedPayload => "deployment profile payload is unsupported",
            Self::AudienceMismatch => "deployment profile audience does not match this application",
            Self::InvalidIdentifier => "deployment profile identifier is invalid",
            Self::InvalidValidityWindow => "deployment profile validity window is invalid",
            Self::ProfileNotYetValid => "deployment profile is not yet valid",
            Self::StaleProfile => "deployment profile is stale",
            Self::ProfileRollback => "deployment profile sequence is below the accepted floor",
            Self::InvalidNetwork => "deployment profile Midnight network is invalid",
            Self::InvalidChainIdentity => "deployment profile Midnight chain identity is invalid",
            Self::InvalidEndpoint => "deployment profile endpoint is invalid",
        };
        formatter.write_str(message)
    }
}

impl Error for DeploymentProfileError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignedEnvelope {
    format: String,
    algorithm: String,
    key_id: String,
    payload: String,
    signature: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeploymentPayload {
    format: String,
    audience: String,
    profile_id: String,
    sequence: u64,
    valid_from_seconds: u64,
    valid_until_seconds: u64,
    midnight: MidnightPayload,
    ssi: SsiPayload,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MidnightPayload {
    network_id: String,
    genesis_hash_hex: String,
    indexer_http_url: String,
    indexer_websocket_url: String,
    node_websocket_url: String,
    proof_server_url: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SsiPayload {
    did_resolver_url: String,
    issuer_metadata_url: String,
    verifier_metadata_url: String,
}

fn validate_identifier(value: &str) -> Result<(), DeploymentProfileError> {
    let valid = !value.is_empty()
        && value.chars().count() <= MAX_IDENTIFIER_CHARACTERS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'));
    if valid {
        Ok(())
    } else {
        Err(DeploymentProfileError::InvalidIdentifier)
    }
}

fn validate_midnight(value: &MidnightPayload) -> Result<(), DeploymentProfileError> {
    if !matches!(
        value.network_id.as_str(),
        "mainnet" | "preprod" | "preview" | "testnet" | "qanet" | "devnet"
    ) {
        return Err(DeploymentProfileError::InvalidNetwork);
    }
    if value.genesis_hash_hex.len() != 64
        || !value
            .genesis_hash_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DeploymentProfileError::InvalidChainIdentity);
    }
    validate_https_endpoint(&value.indexer_http_url)?;
    validate_wss_endpoint(&value.indexer_websocket_url)?;
    validate_wss_endpoint(&value.node_websocket_url)?;
    validate_https_endpoint(&value.proof_server_url)
}

fn validate_ssi(value: &SsiPayload) -> Result<(), DeploymentProfileError> {
    validate_https_endpoint(&value.did_resolver_url)?;
    validate_https_endpoint(&value.issuer_metadata_url)?;
    validate_https_endpoint(&value.verifier_metadata_url)
}

fn validate_https_endpoint(value: &str) -> Result<(), DeploymentProfileError> {
    validate_endpoint(value, "https")
}

fn validate_wss_endpoint(value: &str) -> Result<(), DeploymentProfileError> {
    validate_endpoint(value, "wss")
}

fn validate_endpoint(value: &str, expected_scheme: &str) -> Result<(), DeploymentProfileError> {
    if value.is_empty() || value.len() > MAX_ENDPOINT_CHARACTERS {
        return Err(DeploymentProfileError::InvalidEndpoint);
    }
    let url = Url::parse(value).map_err(|_| DeploymentProfileError::InvalidEndpoint)?;
    let valid = url.scheme() == expected_scheme
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    if valid {
        Ok(())
    } else {
        Err(DeploymentProfileError::InvalidEndpoint)
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use ed25519_dalek::{Signer as _, SigningKey};
    use serde_json::json;

    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn root(revoked_at_seconds: Option<u64>, minimum_profile_sequence: u64) -> DeploymentTrustRoot {
        DeploymentTrustRoot::new(
            "profile-key-2026",
            signing_key().verifying_key().to_bytes(),
            NOW - 100,
            NOW + 100,
            revoked_at_seconds,
            minimum_profile_sequence,
        )
        .expect("trust root")
    }

    fn payload(sequence: u64) -> DeploymentPayload {
        DeploymentPayload {
            format: DEPLOYMENT_PROFILE_FORMAT.to_owned(),
            audience: "io.medianox.oxid".to_owned(),
            profile_id: "midnight-preprod-2026-08".to_owned(),
            sequence,
            valid_from_seconds: NOW - 10,
            valid_until_seconds: NOW + 10,
            midnight: MidnightPayload {
                network_id: "preprod".to_owned(),
                genesis_hash_hex: "ab".repeat(32),
                indexer_http_url: "https://indexer.example.test/api/v4/graphql".to_owned(),
                indexer_websocket_url: "wss://indexer.example.test/api/v4/graphql/ws".to_owned(),
                node_websocket_url: "wss://node.example.test".to_owned(),
                proof_server_url: "https://prover.example.test".to_owned(),
            },
            ssi: SsiPayload {
                did_resolver_url: "https://identity.example.test/did".to_owned(),
                issuer_metadata_url:
                    "https://issuer.example.test/.well-known/openid-credential-issuer".to_owned(),
                verifier_metadata_url:
                    "https://verifier.example.test/.well-known/openid-configuration".to_owned(),
            },
        }
    }

    fn envelope(payload: &DeploymentPayload, key: &SigningKey) -> Vec<u8> {
        let payload = serde_json::to_vec(payload).expect("canonical payload");
        serde_json::to_vec(&json!({
            "format": DEPLOYMENT_PROFILE_FORMAT,
            "algorithm": SIGNATURE_ALGORITHM,
            "keyId": "profile-key-2026",
            "payload": URL_SAFE_NO_PAD.encode(&payload),
            "signature": URL_SAFE_NO_PAD.encode(key.sign(&payload).to_bytes()),
        }))
        .expect("envelope")
    }

    fn verifier(
        root: DeploymentTrustRoot,
        minimum_profile_sequence: u64,
    ) -> DeploymentProfileVerifier {
        DeploymentProfileVerifier::new("io.medianox.oxid", [root], minimum_profile_sequence)
            .expect("verifier")
    }

    #[test]
    fn authenticates_one_atomic_tls_profile() {
        let profile = verifier(root(None, 3), 2)
            .verify(&envelope(&payload(3), &signing_key()), NOW)
            .expect("authenticated profile");

        assert_eq!(profile.profile_id(), "midnight-preprod-2026-08");
        assert_eq!(profile.sequence(), 3);
        assert_eq!(profile.midnight().network_id(), "preprod");
        assert_eq!(profile.midnight().genesis_hash(), &[0xab; 32]);
        assert_eq!(
            profile.ssi().issuer_metadata_url(),
            "https://issuer.example.test/.well-known/openid-credential-issuer"
        );
        let debug = format!("{profile:?} {:?}", profile.midnight());
        assert!(!debug.contains("indexer.example.test"));
        assert!(!debug.contains("issuer.example.test"));
    }

    #[test]
    fn rejects_tampering_unknown_signers_and_mixed_profile_splices() {
        let verifier = verifier(root(None, 1), 1);
        let mut unsigned: serde_json::Value =
            serde_json::from_slice(&envelope(&payload(1), &signing_key())).expect("envelope");
        unsigned["signature"] = "".into();
        assert_eq!(
            verifier.verify(
                &serde_json::to_vec(&unsigned).expect("unsigned envelope"),
                NOW
            ),
            Err(DeploymentProfileError::InvalidSignature)
        );

        let mut signed = envelope(&payload(1), &signing_key());
        let needle = b"profile-key-2026";
        let offset = signed
            .windows(needle.len())
            .position(|window| window == needle)
            .expect("key id in envelope");
        signed[offset] = b'x';
        assert_eq!(
            verifier.verify(&signed, NOW),
            Err(DeploymentProfileError::UntrustedSigner)
        );

        let mut first = payload(1);
        let second = payload(2);
        let original = envelope(&first, &signing_key());
        first.midnight.node_websocket_url = second.midnight.node_websocket_url + "/other";
        let replacement = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&first).expect("payload"));
        let mut value: serde_json::Value = serde_json::from_slice(&original).expect("envelope");
        value["payload"] = replacement.into();
        assert_eq!(
            verifier.verify(&serde_json::to_vec(&value).expect("spliced envelope"), NOW),
            Err(DeploymentProfileError::InvalidSignature)
        );
    }

    #[test]
    fn enforces_rotation_revocation_and_rollback_floors() {
        assert_eq!(
            verifier(root(Some(NOW), 1), 1).verify(&envelope(&payload(1), &signing_key()), NOW),
            Err(DeploymentProfileError::RevokedTrustRoot)
        );
        assert_eq!(
            verifier(root(None, 5), 4).verify(&envelope(&payload(4), &signing_key()), NOW),
            Err(DeploymentProfileError::ProfileRollback)
        );
    }

    #[test]
    fn rejects_stale_future_insecure_and_noncanonical_profiles() {
        let verifier = verifier(root(None, 1), 1);
        let mut stale = payload(1);
        stale.valid_until_seconds = NOW;
        assert_eq!(
            verifier.verify(&envelope(&stale, &signing_key()), NOW),
            Err(DeploymentProfileError::StaleProfile)
        );
        let mut future = payload(1);
        future.valid_from_seconds = NOW + 1;
        assert_eq!(
            verifier.verify(&envelope(&future, &signing_key()), NOW),
            Err(DeploymentProfileError::ProfileNotYetValid)
        );
        let mut insecure = payload(1);
        insecure.midnight.proof_server_url = "http://prover.example.test".to_owned();
        assert_eq!(
            verifier.verify(&envelope(&insecure, &signing_key()), NOW),
            Err(DeploymentProfileError::InvalidEndpoint)
        );

        let canonical = serde_json::to_vec(&payload(1)).expect("payload");
        let pretty: serde_json::Value = serde_json::from_slice(&canonical).expect("payload value");
        let noncanonical = serde_json::to_vec_pretty(&pretty).expect("pretty payload");
        let key = signing_key();
        let value = json!({
            "format": DEPLOYMENT_PROFILE_FORMAT,
            "algorithm": SIGNATURE_ALGORITHM,
            "keyId": "profile-key-2026",
            "payload": URL_SAFE_NO_PAD.encode(&noncanonical),
            "signature": URL_SAFE_NO_PAD.encode(key.sign(&noncanonical).to_bytes()),
        });
        assert_eq!(
            verifier.verify(&serde_json::to_vec(&value).expect("envelope"), NOW),
            Err(DeploymentProfileError::NonCanonicalPayload)
        );
    }
}
