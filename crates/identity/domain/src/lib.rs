// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{collections::BTreeSet, error::Error, fmt};

use oxid_foundation::{OpaqueId, OpaqueIdError};

pub const DID_CONTEXT: &str = "https://www.w3.org/ns/did/v1";
pub const JWK_CONTEXT: &str = "https://w3id.org/security/jwk/v1";

/// Wallet-profile scope used by identity persistence without coupling identity
/// domain entities to the wallet bounded context.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdentityProfileId(OpaqueId);

impl IdentityProfileId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MidnightNetwork {
    Undeployed,
    Devnet,
    Testnet,
    Mainnet,
    Preview,
    Preprod,
    Offchain,
}

impl MidnightNetwork {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Undeployed => "undeployed",
            Self::Devnet => "devnet",
            Self::Testnet => "testnet",
            Self::Mainnet => "mainnet",
            Self::Preview => "preview",
            Self::Preprod => "preprod",
            Self::Offchain => "offchain",
        }
    }

    /// Parses the network component used by a `did:midnight` identifier.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "undeployed" => Some(Self::Undeployed),
            "devnet" => Some(Self::Devnet),
            "testnet" => Some(Self::Testnet),
            "mainnet" => Some(Self::Mainnet),
            "preview" => Some(Self::Preview),
            "preprod" => Some(Self::Preprod),
            "offchain" => Some(Self::Offchain),
            _ => None,
        }
    }
}

/// A current Midnight DID, including syntactically valid long-form offchain DIDs.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MidnightDid {
    value: String,
    network: MidnightNetwork,
}

impl MidnightDid {
    pub const MAX_CHARACTERS: usize = 8_192;

    pub fn parse(value: impl Into<String>) -> Result<Self, MidnightDidError> {
        let value = value.into();
        if value.len() > Self::MAX_CHARACTERS {
            return Err(MidnightDidError::TooLong);
        }
        if value.trim() != value || value.chars().any(char::is_control) {
            return Err(MidnightDidError::InvalidSyntax);
        }
        let parts = value.split(':').collect::<Vec<_>>();
        if parts.len() < 4 || parts[0] != "did" || parts[1] != "midnight" {
            return Err(MidnightDidError::InvalidSyntax);
        }
        let network = MidnightNetwork::parse(parts[2]).ok_or(MidnightDidError::UnknownNetwork)?;
        let identifier = parts[3];
        if identifier.len() != 64 || !identifier.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(MidnightDidError::InvalidIdentifier);
        }
        if network == MidnightNetwork::Offchain {
            if identifier.bytes().any(|byte| byte.is_ascii_uppercase()) {
                return Err(MidnightDidError::InvalidIdentifier);
            }
            if parts.len() == 5 {
                let state = parts[4];
                if state.is_empty()
                    || state.len() % 4 == 1
                    || !state
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                {
                    return Err(MidnightDidError::InvalidOffchainState);
                }
            } else if parts.len() != 4 {
                return Err(MidnightDidError::InvalidSyntax);
            }
        } else if parts.len() != 4 {
            return Err(MidnightDidError::InvalidSyntax);
        }

        Ok(Self { value, network })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub const fn network(&self) -> MidnightNetwork {
        self.network
    }
}

impl fmt::Display for MidnightDid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightDidError {
    TooLong,
    InvalidSyntax,
    UnknownNetwork,
    InvalidIdentifier,
    InvalidOffchainState,
}

impl fmt::Display for MidnightDidError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooLong => "Midnight DID is too long",
            Self::InvalidSyntax => "Midnight DID syntax is invalid",
            Self::UnknownNetwork => "Midnight DID network is unsupported",
            Self::InvalidIdentifier => "Midnight DID identifier is invalid",
            Self::InvalidOffchainState => "Midnight offchain DID state encoding is invalid",
        })
    }
}

impl Error for MidnightDidError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JwkKeyType {
    Okp,
    Ec,
}

impl JwkKeyType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Okp => "OKP",
            Self::Ec => "EC",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JwkCurve {
    Ed25519,
    X25519,
    Jubjub,
    P256,
    Secp256k1,
    Bls12381G1,
    Bls12381G2,
}

impl JwkCurve {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "Ed25519",
            Self::X25519 => "X25519",
            Self::Jubjub => "Jubjub",
            Self::P256 => "P-256",
            Self::Secp256k1 => "secp256k1",
            Self::Bls12381G1 => "BLS12381G1",
            Self::Bls12381G2 => "BLS12381G2",
        }
    }

    #[must_use]
    pub const fn x_byte_length(self) -> usize {
        match self {
            Self::Bls12381G1 => 48,
            Self::Bls12381G2 => 96,
            _ => 32,
        }
    }

    #[must_use]
    pub const fn key_type(self) -> JwkKeyType {
        match self {
            Self::Ed25519 | Self::X25519 | Self::Bls12381G1 | Self::Bls12381G2 => JwkKeyType::Okp,
            Self::Jubjub | Self::P256 | Self::Secp256k1 => JwkKeyType::Ec,
        }
    }
}

/// Public-only JWK material. There is intentionally no private `d` member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicJwk {
    key_type: JwkKeyType,
    curve: JwkCurve,
    x: String,
    y: Option<String>,
}

impl PublicJwk {
    pub fn new(
        key_type: JwkKeyType,
        curve: JwkCurve,
        x: impl Into<String>,
        y: Option<String>,
    ) -> Result<Self, PublicJwkError> {
        if key_type != curve.key_type() {
            return Err(PublicJwkError::IncompatibleProfile);
        }
        let x = x.into();
        if !is_canonical_base64url_coordinate(&x, curve.x_byte_length()) {
            return Err(PublicJwkError::InvalidXCoordinate);
        }
        match (key_type, y.as_deref()) {
            (JwkKeyType::Okp, Some(_)) => return Err(PublicJwkError::UnexpectedYCoordinate),
            (JwkKeyType::Ec, None) => return Err(PublicJwkError::MissingYCoordinate),
            (JwkKeyType::Ec, Some(value)) if !is_canonical_base64url_coordinate(value, 32) => {
                return Err(PublicJwkError::InvalidYCoordinate);
            }
            _ => {}
        }
        Ok(Self {
            key_type,
            curve,
            x,
            y,
        })
    }

    #[must_use]
    pub const fn key_type(&self) -> JwkKeyType {
        self.key_type
    }

    #[must_use]
    pub const fn curve(&self) -> JwkCurve {
        self.curve
    }

    #[must_use]
    pub fn x(&self) -> &str {
        &self.x
    }

    #[must_use]
    pub fn y(&self) -> Option<&str> {
        self.y.as_deref()
    }
}

fn base64url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn is_canonical_base64url_coordinate(value: &str, byte_length: usize) -> bool {
    let encoded_length = byte_length.saturating_mul(8).div_ceil(6);
    if value.len() != encoded_length || !value.bytes().all(|byte| base64url_value(byte).is_some()) {
        return false;
    }
    let unused_bits = encoded_length.saturating_mul(6) - byte_length.saturating_mul(8);
    unused_bits == 0
        || value
            .bytes()
            .last()
            .and_then(base64url_value)
            .is_some_and(|last| last & ((1_u8 << unused_bits) - 1) == 0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicJwkError {
    IncompatibleProfile,
    InvalidXCoordinate,
    MissingYCoordinate,
    UnexpectedYCoordinate,
    InvalidYCoordinate,
}

impl fmt::Display for PublicJwkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::IncompatibleProfile => "JWK key type and curve are incompatible",
            Self::InvalidXCoordinate => "JWK x coordinate is invalid",
            Self::MissingYCoordinate => "EC JWK is missing its y coordinate",
            Self::UnexpectedYCoordinate => "OKP JWK must not contain a y coordinate",
            Self::InvalidYCoordinate => "JWK y coordinate is invalid",
        })
    }
}

impl Error for PublicJwkError {}

fn canonical_method_id(subject: &MidnightDid, value: &str) -> Result<String, DidDocumentError> {
    let fragment = value
        .strip_prefix(subject.as_str())
        .unwrap_or(value)
        .strip_prefix('#')
        .ok_or(DidDocumentError::InvalidMethodReference)?;
    if fragment.is_empty()
        || !fragment.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'%')
        })
    {
        return Err(DidDocumentError::InvalidMethodReference);
    }
    Ok(format!("{}#{fragment}", subject.as_str()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationMethod {
    id: String,
    controller: MidnightDid,
    public_key_jwk: PublicJwk,
}

impl VerificationMethod {
    pub fn new(
        subject: &MidnightDid,
        id: impl AsRef<str>,
        controller: MidnightDid,
        public_key_jwk: PublicJwk,
    ) -> Result<Self, DidDocumentError> {
        if &controller != subject {
            return Err(DidDocumentError::InvalidController);
        }
        Ok(Self {
            id: canonical_method_id(subject, id.as_ref())?,
            controller,
            public_key_jwk,
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn controller(&self) -> &MidnightDid {
        &self.controller
    }

    #[must_use]
    pub const fn public_key_jwk(&self) -> &PublicJwk {
        &self.public_key_jwk
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerificationRelationship {
    Authentication,
    AssertionMethod,
    KeyAgreement,
    CapabilityInvocation,
    CapabilityDelegation,
}

impl VerificationRelationship {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authentication => "authentication",
            Self::AssertionMethod => "assertionMethod",
            Self::KeyAgreement => "keyAgreement",
            Self::CapabilityInvocation => "capabilityInvocation",
            Self::CapabilityDelegation => "capabilityDelegation",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "authentication" => Some(Self::Authentication),
            "assertionMethod" => Some(Self::AssertionMethod),
            "keyAgreement" => Some(Self::KeyAgreement),
            "capabilityInvocation" => Some(Self::CapabilityInvocation),
            "capabilityDelegation" => Some(Self::CapabilityDelegation),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationRelationshipEntry {
    relationship: VerificationRelationship,
    method_ids: Vec<String>,
}

impl VerificationRelationshipEntry {
    #[must_use]
    pub const fn new(relationship: VerificationRelationship, method_ids: Vec<String>) -> Self {
        Self {
            relationship,
            method_ids,
        }
    }

    #[must_use]
    pub const fn relationship(&self) -> VerificationRelationship {
        self.relationship
    }

    #[must_use]
    pub fn method_ids(&self) -> &[String] {
        &self.method_ids
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceEndpointValue {
    Uri(String),
    JsonObject(String),
}

impl ServiceEndpointValue {
    pub fn uri(value: impl Into<String>) -> Result<Self, DidDocumentError> {
        let value = value.into();
        validate_bounded_text(&value, 2_048)?;
        let Some((scheme, _)) = value.split_once(':') else {
            return Err(DidDocumentError::InvalidService);
        };
        if scheme.is_empty()
            || !scheme.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_alphabetic()
                    || (index > 0 && (byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')))
            })
        {
            return Err(DidDocumentError::InvalidService);
        }
        Ok(Self::Uri(value))
    }

    pub fn json_object(value: impl Into<String>) -> Result<Self, DidDocumentError> {
        let value = value.into();
        validate_bounded_text(&value, 8_192)?;
        Ok(Self::JsonObject(value))
    }

    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            Self::Uri(value) | Self::JsonObject(value) => value,
        }
    }

    #[must_use]
    pub const fn is_json_object(&self) -> bool {
        matches!(self, Self::JsonObject(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Service {
    id: String,
    types: Vec<String>,
    endpoints: Vec<ServiceEndpointValue>,
    endpoint_was_array: bool,
}

impl Service {
    pub fn new(
        id: impl Into<String>,
        types: Vec<String>,
        endpoints: Vec<ServiceEndpointValue>,
        endpoint_was_array: bool,
    ) -> Result<Self, DidDocumentError> {
        let id = id.into();
        validate_bounded_text(&id, 2_048)?;
        if types.is_empty() || endpoints.is_empty() {
            return Err(DidDocumentError::InvalidService);
        }
        for value in &types {
            validate_bounded_text(value, 128)?;
        }
        if types.iter().collect::<BTreeSet<_>>().len() != types.len()
            || endpoints
                .iter()
                .map(ServiceEndpointValue::value)
                .collect::<BTreeSet<_>>()
                .len()
                != endpoints.len()
        {
            return Err(DidDocumentError::DuplicateEntry);
        }
        Ok(Self {
            id,
            types,
            endpoints,
            endpoint_was_array,
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn types(&self) -> &[String] {
        &self.types
    }

    #[must_use]
    pub fn endpoints(&self) -> &[ServiceEndpointValue] {
        &self.endpoints
    }

    #[must_use]
    pub const fn endpoint_was_array(&self) -> bool {
        self.endpoint_was_array
    }
}

fn validate_bounded_text(value: &str, maximum: usize) -> Result<(), DidDocumentError> {
    if value.is_empty()
        || value.len() > maximum
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(DidDocumentError::InvalidText);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidDocument {
    contexts: Vec<String>,
    id: MidnightDid,
    also_known_as: Vec<String>,
    verification_methods: Vec<VerificationMethod>,
    relationships: Vec<VerificationRelationshipEntry>,
    services: Vec<Service>,
}

pub struct DidDocumentParts {
    pub contexts: Vec<String>,
    pub id: MidnightDid,
    pub controllers: Vec<MidnightDid>,
    pub also_known_as: Vec<String>,
    pub verification_methods: Vec<VerificationMethod>,
    pub relationships: Vec<VerificationRelationshipEntry>,
    pub services: Vec<Service>,
}

impl DidDocument {
    pub const MAX_METHODS: usize = 128;
    pub const MAX_SERVICES: usize = 128;

    pub fn new(parts: DidDocumentParts) -> Result<Self, DidDocumentError> {
        if parts.contexts.len() < 2
            || parts.contexts.first().map(String::as_str) != Some(DID_CONTEXT)
            || parts.contexts.get(1).map(String::as_str) != Some(JWK_CONTEXT)
            || parts.contexts.len() > 32
        {
            return Err(DidDocumentError::InvalidContext);
        }
        if !parts.controllers.is_empty()
            && (parts.controllers.len() != 1 || parts.controllers[0] != parts.id)
        {
            return Err(DidDocumentError::InvalidController);
        }
        if parts.verification_methods.len() > Self::MAX_METHODS
            || parts.services.len() > Self::MAX_SERVICES
            || parts.also_known_as.len() > 128
        {
            return Err(DidDocumentError::TooManyEntries);
        }
        for context in &parts.contexts {
            validate_bounded_text(context, 2_048)?;
        }
        for alias in &parts.also_known_as {
            validate_bounded_text(alias, 2_048)?;
        }
        let method_ids = parts
            .verification_methods
            .iter()
            .map(|method| method.id().to_owned())
            .collect::<BTreeSet<_>>();
        if method_ids.len() != parts.verification_methods.len() {
            return Err(DidDocumentError::DuplicateEntry);
        }
        if parts
            .relationships
            .iter()
            .map(|entry| entry.relationship)
            .collect::<BTreeSet<_>>()
            .len()
            != parts.relationships.len()
        {
            return Err(DidDocumentError::DuplicateEntry);
        }
        for relation in &parts.relationships {
            if relation.method_ids.is_empty() || relation.method_ids.len() > Self::MAX_METHODS {
                return Err(DidDocumentError::InvalidMethodReference);
            }
            let mut seen = BTreeSet::new();
            for value in &relation.method_ids {
                let canonical = canonical_method_id(&parts.id, value)?;
                if !seen.insert(canonical.clone()) || !method_ids.contains(&canonical) {
                    return Err(DidDocumentError::InvalidMethodReference);
                }
                let method = parts
                    .verification_methods
                    .iter()
                    .find(|method| method.id() == canonical)
                    .ok_or(DidDocumentError::InvalidMethodReference)?;
                let is_x25519 = method.public_key_jwk().curve() == JwkCurve::X25519;
                if (relation.relationship == VerificationRelationship::KeyAgreement) != is_x25519 {
                    return Err(DidDocumentError::IncompatibleRelationship);
                }
            }
        }
        if parts
            .services
            .iter()
            .map(Service::id)
            .collect::<BTreeSet<_>>()
            .len()
            != parts.services.len()
        {
            return Err(DidDocumentError::DuplicateEntry);
        }
        Ok(Self {
            contexts: parts.contexts,
            id: parts.id,
            also_known_as: parts.also_known_as,
            verification_methods: parts.verification_methods,
            relationships: parts.relationships,
            services: parts.services,
        })
    }

    #[must_use]
    pub fn contexts(&self) -> &[String] {
        &self.contexts
    }
    #[must_use]
    pub const fn id(&self) -> &MidnightDid {
        &self.id
    }
    #[must_use]
    pub fn also_known_as(&self) -> &[String] {
        &self.also_known_as
    }
    #[must_use]
    pub fn verification_methods(&self) -> &[VerificationMethod] {
        &self.verification_methods
    }
    #[must_use]
    pub fn relationships(&self) -> &[VerificationRelationshipEntry] {
        &self.relationships
    }
    #[must_use]
    pub fn services(&self) -> &[Service] {
        &self.services
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidDocumentError {
    InvalidContext,
    InvalidController,
    InvalidMethodReference,
    IncompatibleRelationship,
    InvalidService,
    InvalidText,
    DuplicateEntry,
    TooManyEntries,
}

impl fmt::Display for DidDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidContext => "DID document contexts are invalid",
            Self::InvalidController => "DID document controller must equal its subject",
            Self::InvalidMethodReference => "DID document verification-method reference is invalid",
            Self::IncompatibleRelationship => "DID relationship is incompatible with its key curve",
            Self::InvalidService => "DID service is invalid",
            Self::InvalidText => "DID document text value is invalid",
            Self::DuplicateEntry => "DID document contains a duplicate entry",
            Self::TooManyEntries => "DID document contains too many entries",
        })
    }
}

impl Error for DidDocumentError {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DidDocumentMetadata {
    pub created: Option<String>,
    pub updated: Option<String>,
    pub deactivated: Option<bool>,
    pub version_id: Option<String>,
    pub next_update: Option<String>,
    pub next_version_id: Option<String>,
    pub equivalent_ids: Vec<String>,
    pub canonical_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DidResolutionMetadata {
    pub content_type: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidResolutionSource {
    Standalone,
    Live,
    Stored,
}

impl DidResolutionSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standalone => "standalone",
            Self::Live => "live",
            Self::Stored => "stored",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidResolution {
    document: DidDocument,
    document_metadata: DidDocumentMetadata,
    resolution_metadata: DidResolutionMetadata,
    source: DidResolutionSource,
}

impl DidResolution {
    #[must_use]
    pub const fn new(
        document: DidDocument,
        document_metadata: DidDocumentMetadata,
        resolution_metadata: DidResolutionMetadata,
        source: DidResolutionSource,
    ) -> Self {
        Self {
            document,
            document_metadata,
            resolution_metadata,
            source,
        }
    }
    #[must_use]
    pub const fn document(&self) -> &DidDocument {
        &self.document
    }
    #[must_use]
    pub const fn document_metadata(&self) -> &DidDocumentMetadata {
        &self.document_metadata
    }
    #[must_use]
    pub const fn resolution_metadata(&self) -> &DidResolutionMetadata {
        &self.resolution_metadata
    }
    #[must_use]
    pub const fn source(&self) -> DidResolutionSource {
        self.source
    }

    #[must_use]
    pub fn into_stored(self) -> Self {
        Self {
            source: DidResolutionSource::Stored,
            ..self
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidRecord {
    profile_id: IdentityProfileId,
    resolution: DidResolution,
}

impl DidRecord {
    #[must_use]
    pub const fn new(profile_id: IdentityProfileId, resolution: DidResolution) -> Self {
        Self {
            profile_id,
            resolution,
        }
    }
    #[must_use]
    pub const fn profile_id(&self) -> &IdentityProfileId {
        &self.profile_id
    }
    #[must_use]
    pub const fn resolution(&self) -> &DidResolution {
        &self.resolution
    }
    #[must_use]
    pub fn into_resolution(self) -> DidResolution {
        self.resolution
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DID: &str =
        "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_all_networks_and_offchain_long_form() {
        for network in [
            "undeployed",
            "devnet",
            "testnet",
            "mainnet",
            "preview",
            "preprod",
        ] {
            let value = format!("did:midnight:{network}:{}", "a".repeat(64));
            assert_eq!(
                MidnightDid::parse(value)
                    .expect("valid DID")
                    .network()
                    .as_str(),
                network
            );
        }
        assert!(
            MidnightDid::parse(format!("did:midnight:offchain:{}:AQIDBA", "a".repeat(64))).is_ok()
        );
        assert!(MidnightDid::parse(format!("did:midnight:offchain:{}:A", "a".repeat(64))).is_err());
        assert!(MidnightDid::parse(format!("did:midnight:offchain:{}", "A".repeat(64))).is_err());
    }

    #[test]
    fn validates_every_current_public_jwk_profile() {
        for curve in [
            JwkCurve::Ed25519,
            JwkCurve::X25519,
            JwkCurve::Bls12381G1,
            JwkCurve::Bls12381G2,
        ] {
            let x = "A".repeat(curve.x_byte_length().saturating_mul(8).div_ceil(6));
            assert!(PublicJwk::new(JwkKeyType::Okp, curve, x, None).is_ok());
        }
        for curve in [JwkCurve::Jubjub, JwkCurve::P256, JwkCurve::Secp256k1] {
            assert!(
                PublicJwk::new(JwkKeyType::Ec, curve, "A".repeat(43), Some("A".repeat(43))).is_ok()
            );
        }
        assert_eq!(
            PublicJwk::new(JwkKeyType::Okp, JwkCurve::Ed25519, "_".repeat(43), None),
            Err(PublicJwkError::InvalidXCoordinate)
        );
    }

    #[test]
    fn enforces_references_and_relationship_curve_compatibility() {
        let did = MidnightDid::parse(DID).expect("valid DID");
        let signing = VerificationMethod::new(
            &did,
            "#sign",
            did.clone(),
            PublicJwk::new(JwkKeyType::Okp, JwkCurve::Ed25519, "A".repeat(43), None)
                .expect("valid key"),
        )
        .expect("valid method");
        let agreement = VerificationMethod::new(
            &did,
            "#agree",
            did.clone(),
            PublicJwk::new(JwkKeyType::Okp, JwkCurve::X25519, "A".repeat(43), None)
                .expect("valid key"),
        )
        .expect("valid method");
        let document = DidDocument::new(DidDocumentParts {
            contexts: vec![DID_CONTEXT.to_owned(), JWK_CONTEXT.to_owned()],
            id: did.clone(),
            controllers: vec![did],
            also_known_as: Vec::new(),
            verification_methods: vec![signing, agreement],
            relationships: vec![
                VerificationRelationshipEntry::new(
                    VerificationRelationship::Authentication,
                    vec!["#sign".to_owned()],
                ),
                VerificationRelationshipEntry::new(
                    VerificationRelationship::KeyAgreement,
                    vec!["#agree".to_owned()],
                ),
            ],
            services: Vec::new(),
        });
        assert!(document.is_ok());
    }
}
