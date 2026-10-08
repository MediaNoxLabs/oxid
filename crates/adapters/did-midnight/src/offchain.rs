// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, error::Error, fmt};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use blake2::{Blake2s256, Digest as _};

use oxid_identity_domain::{
    DID_CONTEXT, DidDocument, DidDocumentMetadata, DidDocumentParts, DidResolution,
    DidResolutionMetadata, DidResolutionSource, JWK_CONTEXT, JwkCurve, JwkKeyType, MidnightDid,
    MidnightNetwork, PublicJwk, Service, ServiceEndpointValue, VerificationMethod,
    VerificationRelationship, VerificationRelationshipEntry,
};

pub const OFFCHAIN_STATE_ENCODING: &str = "midnight-offchain-did-state-v1.base64url";

const MAGIC: &[u8; 4] = b"MOD1";
const MAX_ALIASES: usize = 4;
const MAX_METHODS: usize = 4;
const MAX_SERVICES: usize = 4;
const CHUNK_COUNT: usize = 1 + MAX_ALIASES + MAX_METHODS * 6 + MAX_SERVICES * 4;
const MAX_STATE_BYTES: usize = MidnightDid::MAX_CHARACTERS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OffchainDidState {
    version: u16,
    also_known_as: Vec<String>,
    verification_methods: Vec<OffchainVerificationMethod>,
    services: Vec<OffchainService>,
}

impl OffchainDidState {
    pub fn new(
        also_known_as: Vec<String>,
        verification_methods: Vec<OffchainVerificationMethod>,
        services: Vec<OffchainService>,
    ) -> Result<Self, OffchainDidError> {
        let state = Self {
            version: 1,
            also_known_as,
            verification_methods,
            services,
        };
        state.validate()?;
        Ok(state)
    }

    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    #[must_use]
    pub fn also_known_as(&self) -> &[String] {
        &self.also_known_as
    }

    #[must_use]
    pub fn verification_methods(&self) -> &[OffchainVerificationMethod] {
        &self.verification_methods
    }

    #[must_use]
    pub fn services(&self) -> &[OffchainService] {
        &self.services
    }

    fn validate(&self) -> Result<(), OffchainDidError> {
        if self.version != 1 {
            return Err(OffchainDidError::UnsupportedVersion);
        }
        if self.also_known_as.len() > MAX_ALIASES
            || self.verification_methods.is_empty()
            || self.verification_methods.len() > MAX_METHODS
            || self.services.len() > MAX_SERVICES
        {
            return Err(OffchainDidError::Bounds);
        }
        let mut method_ids = BTreeSet::new();
        for method in &self.verification_methods {
            validate_relative_id(&method.id)?;
            if !method_ids.insert(method.id.as_str()) {
                return Err(OffchainDidError::DuplicateIdentifier);
            }
        }
        let mut service_ids = BTreeSet::new();
        for service in &self.services {
            validate_relative_id(&service.id)?;
            if !service_ids.insert(service.id.as_str()) {
                return Err(OffchainDidError::DuplicateIdentifier);
            }
        }
        if self
            .also_known_as
            .iter()
            .any(|value| value.is_empty() || invalid_text(value))
            || self.verification_methods.iter().any(|method| {
                invalid_text(&method.id)
                    || invalid_text(method.public_key_jwk.x())
                    || method.public_key_jwk.y().is_some_and(invalid_text)
            })
            || self.services.iter().any(|service| {
                invalid_text(&service.id)
                    || invalid_text(&service.service_type)
                    || invalid_text(&service.endpoint)
            })
        {
            return Err(OffchainDidError::Bounds);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OffchainVerificationMethod {
    id: String,
    public_key_jwk: PublicJwk,
    relationships: BTreeSet<VerificationRelationship>,
}

impl OffchainVerificationMethod {
    pub fn new(
        id: impl Into<String>,
        public_key_jwk: PublicJwk,
        relationships: impl IntoIterator<Item = VerificationRelationship>,
    ) -> Result<Self, OffchainDidError> {
        let value = Self {
            id: id.into(),
            public_key_jwk,
            relationships: relationships.into_iter().collect(),
        };
        validate_relative_id(&value.id)?;
        Ok(value)
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn public_key_jwk(&self) -> &PublicJwk {
        &self.public_key_jwk
    }

    #[must_use]
    pub const fn relationships(&self) -> &BTreeSet<VerificationRelationship> {
        &self.relationships
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OffchainService {
    id: String,
    service_type: String,
    endpoint: String,
}

impl OffchainService {
    pub fn new(
        id: impl Into<String>,
        service_type: impl Into<String>,
        endpoint: impl Into<String>,
    ) -> Result<Self, OffchainDidError> {
        let value = Self {
            id: id.into(),
            service_type: service_type.into(),
            endpoint: endpoint.into(),
        };
        validate_relative_id(&value.id)?;
        if value.service_type.is_empty() || value.endpoint.is_empty() {
            return Err(OffchainDidError::InvalidState);
        }
        if invalid_text(&value.service_type) || invalid_text(&value.endpoint) {
            return Err(OffchainDidError::Bounds);
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffchainDidError {
    InvalidDid,
    InvalidEncoding,
    InvalidState,
    HashMismatch,
    UnsupportedVersion,
    DuplicateIdentifier,
    Bounds,
}

impl fmt::Display for OffchainDidError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidDid => "off-chain Midnight DID is invalid",
            Self::InvalidEncoding => "off-chain Midnight DID state encoding is invalid",
            Self::InvalidState => "off-chain Midnight DID state is invalid",
            Self::HashMismatch => "off-chain Midnight DID state hash does not match",
            Self::UnsupportedVersion => "off-chain Midnight DID state version is unsupported",
            Self::DuplicateIdentifier => "off-chain Midnight DID contains duplicate identifiers",
            Self::Bounds => "off-chain Midnight DID exceeds supported bounds",
        })
    }
}

impl Error for OffchainDidError {}

pub fn create_long_form_offchain_did(
    state: &OffchainDidState,
) -> Result<MidnightDid, OffchainDidError> {
    state.validate()?;
    let bytes = encode_state(state)?;
    let hash = state_hash(&bytes);
    let payload = URL_SAFE_NO_PAD.encode(bytes);
    MidnightDid::parse(format!("did:midnight:offchain:{hash}:{payload}"))
        .map_err(|_| OffchainDidError::Bounds)
}

pub fn resolve_long_form_offchain_did(
    did: &MidnightDid,
) -> Result<DidResolution, OffchainDidError> {
    let parts = did.as_str().split(':').collect::<Vec<_>>();
    if did.network() != MidnightNetwork::Offchain || parts.len() != 5 {
        return Err(OffchainDidError::InvalidDid);
    }
    let bytes = decode_payload(parts[4])?;
    if state_hash(&bytes) != parts[3] {
        return Err(OffchainDidError::HashMismatch);
    }
    let state = decode_state(&bytes)?;
    let document = document_from_state(did, &state)?;
    Ok(DidResolution::new(
        document,
        DidDocumentMetadata {
            deactivated: Some(false),
            version_id: Some(OFFCHAIN_STATE_ENCODING.to_owned()),
            ..DidDocumentMetadata::default()
        },
        DidResolutionMetadata {
            content_type: Some("application/did+ld+json".to_owned()),
        },
        DidResolutionSource::Standalone,
    ))
}

fn document_from_state(
    did: &MidnightDid,
    state: &OffchainDidState,
) -> Result<DidDocument, OffchainDidError> {
    let methods = state
        .verification_methods
        .iter()
        .map(|method| {
            VerificationMethod::new(did, &method.id, did.clone(), method.public_key_jwk.clone())
                .map_err(|_| OffchainDidError::InvalidState)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let relationships = [
        VerificationRelationship::Authentication,
        VerificationRelationship::AssertionMethod,
        VerificationRelationship::KeyAgreement,
        VerificationRelationship::CapabilityInvocation,
        VerificationRelationship::CapabilityDelegation,
    ]
    .into_iter()
    .filter_map(|relationship| {
        let method_ids = state
            .verification_methods
            .iter()
            .filter(|method| method.relationships.contains(&relationship))
            .map(|method| format!("{}{id}", did.as_str(), id = method.id))
            .collect::<Vec<_>>();
        (!method_ids.is_empty())
            .then(|| VerificationRelationshipEntry::new(relationship, method_ids))
    })
    .collect();
    let services = state
        .services
        .iter()
        .map(|service| {
            Service::new(
                format!("{}{id}", did.as_str(), id = service.id),
                vec![service.service_type.clone()],
                vec![
                    ServiceEndpointValue::uri(service.endpoint.clone())
                        .map_err(|_| OffchainDidError::InvalidState)?,
                ],
                false,
            )
            .map_err(|_| OffchainDidError::InvalidState)
        })
        .collect::<Result<Vec<_>, _>>()?;
    DidDocument::new(DidDocumentParts {
        contexts: vec![DID_CONTEXT.to_owned(), JWK_CONTEXT.to_owned()],
        id: did.clone(),
        controllers: vec![did.clone()],
        also_known_as: state.also_known_as.clone(),
        verification_methods: methods,
        relationships,
        services,
    })
    .map_err(|_| OffchainDidError::InvalidState)
}

fn encode_state(state: &OffchainDidState) -> Result<Vec<u8>, OffchainDidError> {
    let mut chunks = Vec::with_capacity(CHUNK_COUNT);
    chunks.push(compact_uint(u64::from(state.version)));
    for index in 0..MAX_ALIASES {
        chunks.push(
            state
                .also_known_as
                .get(index)
                .map_or_else(Vec::new, |value| value.as_bytes().to_vec()),
        );
    }
    for index in 0..MAX_METHODS {
        if let Some(method) = state.verification_methods.get(index) {
            let (kind, x, y) = encode_jwk(&method.public_key_jwk)?;
            chunks.extend([
                vec![1],
                method.id.as_bytes().to_vec(),
                compact_uint(u64::from(kind)),
                x.into_bytes(),
                y.into_bytes(),
                compact_uint(u64::from(relationship_mask(&method.relationships))),
            ]);
        } else {
            chunks.extend([
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ]);
        }
    }
    for index in 0..MAX_SERVICES {
        if let Some(service) = state.services.get(index) {
            chunks.extend([
                vec![1],
                service.id.as_bytes().to_vec(),
                service.service_type.as_bytes().to_vec(),
                service.endpoint.as_bytes().to_vec(),
            ]);
        } else {
            chunks.extend([Vec::new(), Vec::new(), Vec::new(), Vec::new()]);
        }
    }
    frame_chunks(&chunks)
}

fn frame_chunks(chunks: &[Vec<u8>]) -> Result<Vec<u8>, OffchainDidError> {
    if chunks.len() != CHUNK_COUNT {
        return Err(OffchainDidError::InvalidState);
    }
    let body_len = chunks.iter().try_fold(0_usize, |total, chunk| {
        total
            .checked_add(4)
            .and_then(|value| value.checked_add(chunk.len()))
            .ok_or(OffchainDidError::Bounds)
    })?;
    let capacity = 8_usize
        .checked_add(body_len)
        .ok_or(OffchainDidError::Bounds)?;
    if capacity > MAX_STATE_BYTES {
        return Err(OffchainDidError::Bounds);
    }
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(chunks.len() as u32).to_be_bytes());
    for chunk in chunks {
        let len = u32::try_from(chunk.len()).map_err(|_| OffchainDidError::Bounds)?;
        bytes.extend_from_slice(&len.to_be_bytes());
        bytes.extend_from_slice(chunk);
    }
    Ok(bytes)
}

fn decode_state(bytes: &[u8]) -> Result<OffchainDidState, OffchainDidError> {
    let mut chunks = unframe_chunks(bytes)?.into_iter();
    let version = u16::try_from(decode_uint(next_chunk(&mut chunks)?)?)
        .map_err(|_| OffchainDidError::UnsupportedVersion)?;
    if version != 1 {
        return Err(OffchainDidError::UnsupportedVersion);
    }
    let mut aliases = Vec::new();
    for _ in 0..MAX_ALIASES {
        let value = decode_string(next_chunk(&mut chunks)?)?;
        if !value.is_empty() {
            aliases.push(value);
        }
    }
    let mut methods = Vec::new();
    for _ in 0..MAX_METHODS {
        let present = decode_bool(next_chunk(&mut chunks)?)?;
        let id = decode_string(next_chunk(&mut chunks)?)?;
        let kind = u8::try_from(decode_uint(next_chunk(&mut chunks)?)?)
            .map_err(|_| OffchainDidError::InvalidState)?;
        let x = decode_string(next_chunk(&mut chunks)?)?;
        let y = decode_string(next_chunk(&mut chunks)?)?;
        let mask = u8::try_from(decode_uint(next_chunk(&mut chunks)?)?)
            .map_err(|_| OffchainDidError::InvalidState)?;
        if present {
            methods.push(OffchainVerificationMethod::new(
                id,
                decode_jwk(kind, &x, &y)?,
                relationships_from_mask(mask)?,
            )?);
        } else if !id.is_empty() || kind != 0 || !x.is_empty() || !y.is_empty() || mask != 0 {
            return Err(OffchainDidError::InvalidState);
        }
    }
    let mut services = Vec::new();
    for _ in 0..MAX_SERVICES {
        let present = decode_bool(next_chunk(&mut chunks)?)?;
        let id = decode_string(next_chunk(&mut chunks)?)?;
        let service_type = decode_string(next_chunk(&mut chunks)?)?;
        let endpoint = decode_string(next_chunk(&mut chunks)?)?;
        if present {
            services.push(OffchainService::new(id, service_type, endpoint)?);
        } else if !id.is_empty() || !service_type.is_empty() || !endpoint.is_empty() {
            return Err(OffchainDidError::InvalidState);
        }
    }
    if chunks.next().is_some() {
        return Err(OffchainDidError::InvalidState);
    }
    let state = OffchainDidState {
        version,
        also_known_as: aliases,
        verification_methods: methods,
        services,
    };
    state.validate()?;
    if encode_state(&state)? != bytes {
        return Err(OffchainDidError::InvalidEncoding);
    }
    Ok(state)
}

fn unframe_chunks(bytes: &[u8]) -> Result<Vec<Vec<u8>>, OffchainDidError> {
    if bytes.len() < 8 || bytes.len() > MAX_STATE_BYTES || &bytes[..4] != MAGIC {
        return Err(OffchainDidError::InvalidEncoding);
    }
    let count = u32::from_be_bytes(
        bytes[4..8]
            .try_into()
            .map_err(|_| OffchainDidError::InvalidEncoding)?,
    ) as usize;
    if count != CHUNK_COUNT {
        return Err(OffchainDidError::InvalidState);
    }
    let mut offset = 8_usize;
    let mut chunks = Vec::with_capacity(count);
    for _ in 0..count {
        let end = offset.checked_add(4).ok_or(OffchainDidError::Bounds)?;
        let length_bytes = bytes
            .get(offset..end)
            .ok_or(OffchainDidError::InvalidEncoding)?;
        let length = u32::from_be_bytes(
            length_bytes
                .try_into()
                .map_err(|_| OffchainDidError::InvalidEncoding)?,
        ) as usize;
        offset = end;
        let end = offset.checked_add(length).ok_or(OffchainDidError::Bounds)?;
        chunks.push(
            bytes
                .get(offset..end)
                .ok_or(OffchainDidError::InvalidEncoding)?
                .to_vec(),
        );
        offset = end;
    }
    if offset != bytes.len() {
        return Err(OffchainDidError::InvalidEncoding);
    }
    Ok(chunks)
}

fn decode_payload(value: &str) -> Result<Vec<u8>, OffchainDidError> {
    if value.is_empty() || value.len() > MAX_STATE_BYTES {
        return Err(OffchainDidError::Bounds);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| OffchainDidError::InvalidEncoding)?;
    if URL_SAFE_NO_PAD.encode(&bytes) != value {
        return Err(OffchainDidError::InvalidEncoding);
    }
    Ok(bytes)
}

fn encode_jwk(jwk: &PublicJwk) -> Result<(u8, String, String), OffchainDidError> {
    let kind = match (jwk.key_type(), jwk.curve()) {
        (JwkKeyType::Ec, JwkCurve::Jubjub) => 1,
        (JwkKeyType::Okp, JwkCurve::Ed25519) => 2,
        (JwkKeyType::Ec, JwkCurve::P256) => 3,
        (JwkKeyType::Okp, JwkCurve::X25519) => 4,
        (JwkKeyType::Ec, JwkCurve::Secp256k1) => 5,
        (JwkKeyType::Okp, JwkCurve::Bls12381G1) => 6,
        (JwkKeyType::Okp, JwkCurve::Bls12381G2) => 7,
        _ => return Err(OffchainDidError::InvalidState),
    };
    let x = if jwk.curve() == JwkCurve::Jubjub {
        reverse_coordinate(jwk.x())?
    } else {
        jwk.x().to_owned()
    };
    let y = match jwk.y() {
        Some(value) if jwk.curve() == JwkCurve::Jubjub => reverse_coordinate(value)?,
        Some(value) => value.to_owned(),
        None => String::new(),
    };
    Ok((kind, x, y))
}

fn decode_jwk(kind: u8, x: &str, y: &str) -> Result<PublicJwk, OffchainDidError> {
    let (key_type, curve, x, y) = match kind {
        1 => (
            JwkKeyType::Ec,
            JwkCurve::Jubjub,
            reverse_coordinate(x)?,
            Some(reverse_coordinate(y)?),
        ),
        2 => (JwkKeyType::Okp, JwkCurve::Ed25519, x.to_owned(), None),
        3 => (
            JwkKeyType::Ec,
            JwkCurve::P256,
            x.to_owned(),
            Some(y.to_owned()),
        ),
        4 => (JwkKeyType::Okp, JwkCurve::X25519, x.to_owned(), None),
        5 => (
            JwkKeyType::Ec,
            JwkCurve::Secp256k1,
            x.to_owned(),
            Some(y.to_owned()),
        ),
        6 => (JwkKeyType::Okp, JwkCurve::Bls12381G1, x.to_owned(), None),
        7 => (JwkKeyType::Okp, JwkCurve::Bls12381G2, x.to_owned(), None),
        _ => return Err(OffchainDidError::InvalidState),
    };
    PublicJwk::new(key_type, curve, x, y).map_err(|_| OffchainDidError::InvalidState)
}

fn reverse_coordinate(value: &str) -> Result<String, OffchainDidError> {
    let mut bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| OffchainDidError::InvalidState)?;
    bytes.reverse();
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn relationship_mask(relationships: &BTreeSet<VerificationRelationship>) -> u8 {
    relationships.iter().fold(0, |mask, relationship| {
        mask | match relationship {
            VerificationRelationship::Authentication => 1,
            VerificationRelationship::AssertionMethod => 2,
            VerificationRelationship::KeyAgreement => 4,
            VerificationRelationship::CapabilityInvocation => 8,
            VerificationRelationship::CapabilityDelegation => 16,
        }
    })
}

fn relationships_from_mask(mask: u8) -> Result<Vec<VerificationRelationship>, OffchainDidError> {
    if mask & !0x1f != 0 {
        return Err(OffchainDidError::InvalidState);
    }
    Ok([
        (1, VerificationRelationship::Authentication),
        (2, VerificationRelationship::AssertionMethod),
        (4, VerificationRelationship::KeyAgreement),
        (8, VerificationRelationship::CapabilityInvocation),
        (16, VerificationRelationship::CapabilityDelegation),
    ]
    .into_iter()
    .filter_map(|(bit, relationship)| (mask & bit != 0).then_some(relationship))
    .collect())
}

fn compact_uint(value: u64) -> Vec<u8> {
    let bytes = value.to_le_bytes();
    let length = bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |index| index + 1);
    bytes[..length].to_vec()
}

fn decode_uint(bytes: Vec<u8>) -> Result<u64, OffchainDidError> {
    if bytes.len() > 8 || bytes.last() == Some(&0) {
        return Err(OffchainDidError::InvalidState);
    }
    let mut value = [0_u8; 8];
    value[..bytes.len()].copy_from_slice(&bytes);
    Ok(u64::from_le_bytes(value))
}

fn decode_bool(bytes: Vec<u8>) -> Result<bool, OffchainDidError> {
    match bytes.as_slice() {
        [] => Ok(false),
        [1] => Ok(true),
        _ => Err(OffchainDidError::InvalidState),
    }
}

fn decode_string(bytes: Vec<u8>) -> Result<String, OffchainDidError> {
    String::from_utf8(bytes).map_err(|_| OffchainDidError::InvalidState)
}

fn next_chunk(chunks: &mut impl Iterator<Item = Vec<u8>>) -> Result<Vec<u8>, OffchainDidError> {
    chunks.next().ok_or(OffchainDidError::InvalidState)
}

fn state_hash(bytes: &[u8]) -> String {
    let digest = Blake2s256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_relative_id(value: &str) -> Result<(), OffchainDidError> {
    let Some(fragment) = value.strip_prefix('#') else {
        return Err(OffchainDidError::InvalidState);
    };
    if fragment.is_empty()
        || !fragment.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'%')
        })
    {
        return Err(OffchainDidError::InvalidState);
    }
    Ok(())
}

fn invalid_text(value: &str) -> bool {
    value.len() > 2_048 || value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTOR_PAYLOAD: &str = "TU9EMQAAAC0AAAABAQAAACFodHRwczovL2V4YW1wbGUub3JnL2hvbGRlcnMvYWxpY2UAAAAAAAAAAAAAAAAAAAABAQAAAA0jaG9sZGVyLWtleS0xAAAAAQEAAAArQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQQAAACtBUUVCQVFFQkFRRUJBUUVCQVFFQkFRRUJBUUVCQVFFQkFRRUJBUUVCQVFFAAAAAQMAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAQAAAAgjcHJvZmlsZQAAAA1MaW5rZWREb21haW5zAAAAIWh0dHBzOi8vZXhhbXBsZS5vcmcvcHJvZmlsZS9hbGljZQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const VECTOR_DID: &str =
        "did:midnight:offchain:3c08b85758d973a6002942c730d077ede51920c184927aaf010562035203fc21";

    fn vector_state() -> OffchainDidState {
        OffchainDidState::new(
            vec!["https://example.org/holders/alice".to_owned()],
            vec![
                OffchainVerificationMethod::new(
                    "#holder-key-1",
                    PublicJwk::new(
                        JwkKeyType::Ec,
                        JwkCurve::Jubjub,
                        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                        Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE".to_owned()),
                    )
                    .expect("Jubjub JWK"),
                    [
                        VerificationRelationship::Authentication,
                        VerificationRelationship::AssertionMethod,
                    ],
                )
                .expect("method"),
            ],
            vec![
                OffchainService::new(
                    "#profile",
                    "LinkedDomains",
                    "https://example.org/profile/alice",
                )
                .expect("service"),
            ],
        )
        .expect("state")
    }

    #[test]
    fn matches_canonical_midnight_did_cross_language_vector() {
        let did = create_long_form_offchain_did(&vector_state()).expect("create DID");
        assert_eq!(did.as_str(), format!("{VECTOR_DID}:{VECTOR_PAYLOAD}"));
        let resolution = resolve_long_form_offchain_did(&did).expect("resolve DID");
        assert_eq!(resolution.document().id(), &did);
        assert_eq!(resolution.document().verification_methods().len(), 1);
    }

    #[test]
    fn rejects_tampered_noncanonical_and_unsupported_state() {
        let did = create_long_form_offchain_did(&vector_state()).expect("create DID");
        let mut parts = did
            .as_str()
            .split(':')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        parts[3].replace_range(..1, "0");
        let tampered = MidnightDid::parse(parts.join(":")).expect("syntactically valid");
        assert_eq!(
            resolve_long_form_offchain_did(&tampered),
            Err(OffchainDidError::HashMismatch)
        );

        let mut bytes = URL_SAFE_NO_PAD.decode(VECTOR_PAYLOAD).expect("payload");
        bytes[12] = 2;
        let payload = URL_SAFE_NO_PAD.encode(&bytes);
        let hash = state_hash(&bytes);
        let unsupported = MidnightDid::parse(format!("did:midnight:offchain:{hash}:{payload}"))
            .expect("syntactically valid");
        assert_eq!(
            resolve_long_form_offchain_did(&unsupported),
            Err(OffchainDidError::UnsupportedVersion)
        );
    }

    #[test]
    fn rejects_duplicate_identifiers_and_unbounded_public_text() {
        let method = vector_state().verification_methods()[0].clone();
        assert_eq!(
            OffchainDidState::new(Vec::new(), vec![method.clone(), method], Vec::new()),
            Err(OffchainDidError::DuplicateIdentifier)
        );

        assert_eq!(
            OffchainService::new("#status", "LinkedDomains", "x".repeat(2_049)),
            Err(OffchainDidError::Bounds)
        );
        assert_eq!(
            OffchainDidState::new(vec![String::new()], vec![method_for_test()], Vec::new()),
            Err(OffchainDidError::Bounds)
        );
    }

    fn method_for_test() -> OffchainVerificationMethod {
        vector_state().verification_methods()[0].clone()
    }
}
