// SPDX-License-Identifier: Apache-2.0

//! Strict native reader and issuer-proof verifier for the upstream Digital
//! Passport `compact-value-v1.base64url` credential family.
//!
//! The MCV1 container and circuit arithmetic mirror the exact source pinned by
//! the repository's Nix inputs. No generated or SDK type crosses this adapter.

use base64::{Engine as _, engine::general_purpose};
use midnight_base_crypto::{hash::PersistentHashWriter, repr::BinaryHashRepr};
use midnight_transient_crypto::{
    curve::{EmbeddedFr, EmbeddedGroupAffine, Fr},
    fab::ValueReprAlignedValue,
    hash::{degrade_to_transient, transient_hash, upgrade_from_transient},
    repr::FieldRepr,
};
use oxid_credential_application::{
    BoundCredentialBundle, BoundCredentialError, BoundCredentialFuture, BoundCredentialIssuerPort,
    BoundCredentialRequest, CredentialInspection, CredentialInspectionFuture,
    CredentialVerificationError, CredentialVerificationPort,
};
use oxid_credential_domain::{
    CredentialFormat, CredentialMetadata, MAX_SIGNED_CREDENTIAL_BYTES, VerificationOutcome,
    VerificationReport, VerificationStage, VerificationStageName, VerificationStageStatus,
};
use oxid_foundation::UnixTimestampMillis;
use oxid_platform_ports::ClockPort;
use std::sync::Arc;

use crate::credential_id;
use crate::digital_passport::{DigitalPassportCommitments, PACKAGE_ID, SCHEMA_ID};

const MCV1_MAGIC: &[u8; 4] = b"MCV1";
const CREDENTIAL_CHUNKS: usize = 18;
const PROOF_CHUNKS: usize = 9;
const ISSUANCE_CONTEXT: &[u8] = b"midnight:vc:issuance";
// Public reference-fixture scalars. Standalone composition is conformance
// infrastructure, not issuer custody; production composition never selects it.
const STANDALONE_PUBLIC_ISSUER_SCALAR: u64 = 123_456_789;
const STANDALONE_PUBLIC_NONCE_SCALAR: u64 = 11;

#[derive(Clone, Copy, Debug, Default)]
pub struct MidnightCompactCredentialVerifier;

#[derive(Clone)]
pub struct StandaloneBoundCompactCredentialIssuer {
    clock: Arc<dyn ClockPort>,
}

impl StandaloneBoundCompactCredentialIssuer {
    #[must_use]
    pub fn new(clock: Arc<dyn ClockPort>) -> Self {
        Self { clock }
    }
}

impl BoundCredentialIssuerPort for StandaloneBoundCompactCredentialIssuer {
    fn issue<'a>(&'a self, request: BoundCredentialRequest) -> BoundCredentialFuture<'a> {
        Box::pin(async move {
            let issued_at = self
                .clock
                .now()
                .map_err(|_| BoundCredentialError::Unavailable)?
                .value()
                / 1_000;
            issue_bound_standalone_credential(&request, issued_at)
        })
    }
}

impl CredentialVerificationPort for MidnightCompactCredentialVerifier {
    fn inspect<'a>(
        &'a self,
        signed_bytes: &'a [u8],
        detached_proof: Option<&'a [u8]>,
    ) -> CredentialInspectionFuture<'a> {
        Box::pin(async move { inspect(signed_bytes, detached_proof) })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VerificationMethodRef {
    pub(crate) did_contract_address: [u8; 32],
    pub(crate) method_id: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CompactCredential {
    pub(crate) version: u16,
    pub(crate) package_id: [u8; 32],
    pub(crate) schema_id: [u8; 32],
    pub(crate) major_version: u16,
    pub(crate) minor_version: u16,
    pub(crate) issuer: VerificationMethodRef,
    pub(crate) holder: VerificationMethodRef,
    pub(crate) issued_at: u64,
    pub(crate) has_expiration: bool,
    pub(crate) expires_at: u64,
    pub(crate) commitments: DigitalPassportCommitments,
}

#[derive(Clone, Copy)]
pub(crate) struct CompactProof {
    pub(crate) signer: VerificationMethodRef,
    pub(crate) created_at: u64,
    pub(crate) challenge_hash: [u8; 32],
    pub(crate) public_key: EmbeddedGroupAffine,
    pub(crate) announcement: EmbeddedGroupAffine,
    pub(crate) response: Fr,
}

pub(crate) fn inspect(
    credential_bytes: &[u8],
    detached_proof: Option<&[u8]>,
) -> Result<CredentialInspection, CredentialVerificationError> {
    let credential = parse_credential(credential_bytes)?;
    let id = credential_id(credential_bytes)?;
    let issuer_did = did_from_contract_address(credential.issuer.did_contract_address);
    let holder_did = did_from_contract_address(credential.holder.did_contract_address);
    let issued_at_ms = credential
        .issued_at
        .checked_mul(1_000)
        .ok_or(CredentialVerificationError::InvalidCredential)?;
    let metadata = CredentialMetadata::new(
        "Digital Passport",
        issuer_did,
        Some(holder_did),
        CredentialFormat::MidnightCompactVc,
        Some(UnixTimestampMillis::new(issued_at_ms)),
    )
    .map_err(|_| CredentialVerificationError::InvalidCredential)?;

    if credential.version != 1 {
        return invalid(
            id,
            metadata,
            VerificationStageName::Structural,
            "version_mismatch",
        );
    }
    if credential.package_id != padded::<32>(PACKAGE_ID.as_bytes())
        || credential.schema_id != padded::<32>(SCHEMA_ID.as_bytes())
        || credential.major_version != 1
    {
        return invalid(
            id,
            metadata,
            VerificationStageName::Schema,
            "schema_mismatch",
        );
    }
    if credential.holder.did_contract_address == [0; 32] || credential.holder.method_id == [0; 32] {
        return invalid(
            id,
            metadata,
            VerificationStageName::Structural,
            "holder_binding_missing",
        );
    }
    if credential.has_expiration && credential.expires_at < credential.issued_at {
        return invalid(
            id,
            metadata,
            VerificationStageName::Temporal,
            "expiration_precedes_issuance",
        );
    }
    if credential.commitments.claim_root != claim_root(&credential.commitments) {
        return invalid(
            id,
            metadata,
            VerificationStageName::Schema,
            "claim_root_mismatch",
        );
    }

    let Some(proof_bytes) = detached_proof else {
        return invalid(
            id,
            metadata,
            VerificationStageName::Proof,
            "detached_proof_missing",
        );
    };
    let proof = match parse_proof(proof_bytes) {
        Ok(proof) => proof,
        Err(_) => {
            return invalid(
                id,
                metadata,
                VerificationStageName::Proof,
                "detached_proof_malformed",
            );
        }
    };
    if proof.signer != credential.issuer {
        return invalid(
            id,
            metadata,
            VerificationStageName::Proof,
            "issuer_method_mismatch",
        );
    }
    if !verify_issuance_proof(&credential, &proof) {
        return invalid(
            id,
            metadata,
            VerificationStageName::Proof,
            "invalid_issuance_proof",
        );
    }

    Ok(CredentialInspection {
        id,
        metadata,
        verification: compact_report(VerificationOutcome::Valid, None)?,
    })
}

fn invalid(
    id: oxid_credential_domain::CredentialId,
    metadata: CredentialMetadata,
    stage: VerificationStageName,
    reason: &'static str,
) -> Result<CredentialInspection, CredentialVerificationError> {
    Ok(CredentialInspection {
        id,
        metadata,
        verification: compact_report(VerificationOutcome::Invalid, Some((stage, reason)))?,
    })
}

fn compact_report(
    outcome: VerificationOutcome,
    failure: Option<(VerificationStageName, &'static str)>,
) -> Result<VerificationReport, CredentialVerificationError> {
    let stages = VerificationStageName::ALL
        .into_iter()
        .map(|name| {
            let (status, reason) = match failure {
                Some((failed, reason)) if name == failed => {
                    (VerificationStageStatus::Failed, Some(reason.to_owned()))
                }
                Some(_) => (VerificationStageStatus::NotChecked, None),
                None if matches!(
                    name,
                    VerificationStageName::Structural
                        | VerificationStageName::Proof
                        | VerificationStageName::Schema
                ) =>
                {
                    (VerificationStageStatus::Passed, None)
                }
                None => (VerificationStageStatus::NotChecked, None),
            };
            VerificationStage::new(name, status, reason)
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CredentialVerificationError::InvalidCredential)?;
    VerificationReport::new(outcome, stages)
        .map_err(|_| CredentialVerificationError::InvalidCredential)
}

pub(crate) fn parse_credential(
    bytes: &[u8],
) -> Result<CompactCredential, CredentialVerificationError> {
    let chunks = parse_mcv1(bytes, CREDENTIAL_CHUNKS)?;
    let commitments = DigitalPassportCommitments {
        first_name: fixed(&chunks[12])?,
        last_name: fixed(&chunks[13])?,
        date_of_birth: fixed(&chunks[14])?,
        document_number: fixed(&chunks[15])?,
        issuing_state: fixed(&chunks[16])?,
        claim_root: fixed(&chunks[17])?,
    };
    Ok(CompactCredential {
        version: integer(&chunks[0])?,
        package_id: fixed(&chunks[1])?,
        schema_id: fixed(&chunks[2])?,
        major_version: integer(&chunks[3])?,
        minor_version: integer(&chunks[4])?,
        issuer: VerificationMethodRef {
            did_contract_address: fixed(&chunks[5])?,
            method_id: fixed(&chunks[6])?,
        },
        holder: VerificationMethodRef {
            did_contract_address: fixed(&chunks[7])?,
            method_id: fixed(&chunks[8])?,
        },
        issued_at: integer(&chunks[9])?,
        has_expiration: boolean(&chunks[10])?,
        expires_at: integer(&chunks[11])?,
        commitments,
    })
}

pub(crate) fn parse_proof(bytes: &[u8]) -> Result<CompactProof, CredentialVerificationError> {
    let chunks = parse_mcv1(bytes, PROOF_CHUNKS)?;
    Ok(CompactProof {
        signer: VerificationMethodRef {
            did_contract_address: fixed(&chunks[0])?,
            method_id: fixed(&chunks[1])?,
        },
        created_at: integer(&chunks[2])?,
        challenge_hash: fixed(&chunks[3])?,
        public_key: point(&chunks[4], &chunks[5])?,
        announcement: point(&chunks[6], &chunks[7])?,
        response: field(&chunks[8])?,
    })
}

fn parse_mcv1(
    bytes: &[u8],
    expected_chunks: usize,
) -> Result<Vec<Vec<u8>>, CredentialVerificationError> {
    if bytes.len() < 8 || bytes.len() > MAX_SIGNED_CREDENTIAL_BYTES || &bytes[..4] != MCV1_MAGIC {
        return Err(CredentialVerificationError::UnsupportedFormat);
    }
    let count = u32::from_be_bytes(
        bytes[4..8]
            .try_into()
            .map_err(|_| CredentialVerificationError::InvalidCredential)?,
    ) as usize;
    if count != expected_chunks {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    let mut offset: usize = 8;
    let mut chunks = Vec::with_capacity(count);
    for _ in 0..count {
        let length_end = offset
            .checked_add(4)
            .filter(|end| *end <= bytes.len())
            .ok_or(CredentialVerificationError::InvalidCredential)?;
        let length = u32::from_be_bytes(
            bytes[offset..length_end]
                .try_into()
                .map_err(|_| CredentialVerificationError::InvalidCredential)?,
        ) as usize;
        offset = length_end;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or(CredentialVerificationError::InvalidCredential)?;
        let chunk = bytes[offset..end].to_vec();
        if chunk.last() == Some(&0) {
            return Err(CredentialVerificationError::InvalidCredential);
        }
        chunks.push(chunk);
        offset = end;
    }
    if offset != bytes.len() {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    Ok(chunks)
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], CredentialVerificationError> {
    if bytes.len() > N {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    let mut value = [0; N];
    value[..bytes.len()].copy_from_slice(bytes);
    Ok(value)
}

fn integer<T>(bytes: &[u8]) -> Result<T, CredentialVerificationError>
where
    T: TryFrom<u64>,
{
    if bytes.len() > 8 {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    let mut value = [0; 8];
    value[..bytes.len()].copy_from_slice(bytes);
    T::try_from(u64::from_le_bytes(value))
        .map_err(|_| CredentialVerificationError::InvalidCredential)
}

fn boolean(bytes: &[u8]) -> Result<bool, CredentialVerificationError> {
    match bytes {
        [] => Ok(false),
        [1] => Ok(true),
        _ => Err(CredentialVerificationError::InvalidCredential),
    }
}

fn field(bytes: &[u8]) -> Result<Fr, CredentialVerificationError> {
    if bytes.len() > 32 {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    Fr::from_le_bytes(bytes).ok_or(CredentialVerificationError::InvalidCredential)
}

fn point(x: &[u8], y: &[u8]) -> Result<EmbeddedGroupAffine, CredentialVerificationError> {
    let x = field(x)?;
    let y = field(y)?;
    if x == Fr::from(0_u64) && y == Fr::from(0_u64) {
        // The MCV1 codec uses the all-zero pair for the identity point. A
        // Schnorr issuer key or announcement at identity would admit the
        // trivial all-zero proof, so credential proofs must reject it.
        return Err(CredentialVerificationError::InvalidCredential);
    }
    EmbeddedGroupAffine::new(x, y).ok_or(CredentialVerificationError::InvalidCredential)
}

fn verify_issuance_proof(credential: &CompactCredential, proof: &CompactProof) -> bool {
    let challenge = issuance_challenge(credential, proof);
    EmbeddedGroupAffine::generator() * proof.response
        == proof.announcement + proof.public_key * challenge
}

fn issuance_challenge(credential: &CompactCredential, proof: &CompactProof) -> Fr {
    let body_root = credential_body_root(credential);
    let proof_payload_root = persistent_hash(&(
        body_root,
        padded::<32>(ISSUANCE_CONTEXT),
        persistent_hash(&(proof.signer.did_contract_address, proof.signer.method_id)),
        upgrade_from_transient(transient_hash_value(proof.created_at)).0,
        proof.challenge_hash,
    ));
    degrade_to_transient(midnight_base_crypto::hash::HashOutput(persistent_hash(&(
        proof_payload_root,
        upgrade_from_transient(transient_hash_value(proof.public_key)).0,
        upgrade_from_transient(transient_hash_value(proof.announcement)).0,
    ))))
}

fn issue_bound_standalone_credential(
    request: &BoundCredentialRequest,
    issued_at: u64,
) -> Result<BoundCredentialBundle, BoundCredentialError> {
    let holder = holder_binding(request)?;
    validate_holder_public_key(request)?;

    let mut credential = parse_credential(&super::standalone_compact_credential())
        .map_err(|_| BoundCredentialError::Unavailable)?;
    credential.holder = holder;
    // The checked-in reference body is historical. Standalone issuance signs
    // a fresh holder-bound body with a bounded ten-year development lifetime.
    credential.issued_at = issued_at;
    credential.has_expiration = true;
    credential.expires_at = credential
        .issued_at
        .checked_add(10 * 365 * 24 * 60 * 60)
        .ok_or(BoundCredentialError::Unavailable)?;

    let template = parse_proof(&super::standalone_compact_proof())
        .map_err(|_| BoundCredentialError::Unavailable)?;
    let secret = EmbeddedFr::from(STANDALONE_PUBLIC_ISSUER_SCALAR);
    let public_key = EmbeddedGroupAffine::generator() * secret;
    if template.signer != credential.issuer || template.public_key != public_key {
        return Err(BoundCredentialError::Unavailable);
    }
    let nonce = EmbeddedFr::from(STANDALONE_PUBLIC_NONCE_SCALAR);
    let mut proof = CompactProof {
        signer: credential.issuer,
        created_at: issued_at,
        challenge_hash: template.challenge_hash,
        public_key,
        announcement: EmbeddedGroupAffine::generator() * nonce,
        response: Fr::from(0_u64),
    };
    let challenge = issuance_challenge(&credential, &proof);
    let response = nonce
        + EmbeddedFr::try_from(challenge).map_err(|_| BoundCredentialError::Unavailable)? * secret;
    proof.response =
        Fr::from_le_bytes(&response.as_le_bytes()).ok_or(BoundCredentialError::Unavailable)?;
    if !verify_issuance_proof(&credential, &proof) {
        return Err(BoundCredentialError::Unavailable);
    }

    Ok(BoundCredentialBundle {
        signed_bytes: encode_credential(&credential)?,
        detached_proof: Some(encode_proof(&proof)?),
        private_material: Some(super::standalone_private_material()),
    })
}

fn holder_binding(
    request: &BoundCredentialRequest,
) -> Result<VerificationMethodRef, BoundCredentialError> {
    let identifier = request
        .holder_did
        .strip_prefix("did:midnight:undeployed:")
        .ok_or(BoundCredentialError::InvalidHolderBinding)?;
    let did_contract_address = hex::decode(identifier)
        .ok()
        .and_then(|value| <[u8; 32]>::try_from(value).ok())
        .ok_or(BoundCredentialError::InvalidHolderBinding)?;
    if hex::encode(did_contract_address) != identifier {
        return Err(BoundCredentialError::InvalidHolderBinding);
    }
    let fragment = request
        .holder_binding_method_id
        .strip_prefix(&request.holder_did)
        .filter(|value| value.starts_with('#') && value.len() <= 32)
        .ok_or(BoundCredentialError::InvalidHolderBinding)?;
    if fragment.len() < 2
        || !fragment.bytes().all(|byte| {
            byte == b'#'
                || byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'-' | b'_' | b':' | b'%')
        })
    {
        return Err(BoundCredentialError::InvalidHolderBinding);
    }
    let mut method_id = [0_u8; 32];
    method_id[..fragment.len()].copy_from_slice(fragment.as_bytes());
    Ok(VerificationMethodRef {
        did_contract_address,
        method_id,
    })
}

fn validate_holder_public_key(
    request: &BoundCredentialRequest,
) -> Result<(), BoundCredentialError> {
    let decode = |value: &str| {
        general_purpose::URL_SAFE_NO_PAD
            .decode(value)
            .ok()
            .filter(|bytes| general_purpose::URL_SAFE_NO_PAD.encode(bytes) == value)
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            .and_then(|bytes| Fr::from_le_bytes(&bytes))
            .ok_or(BoundCredentialError::InvalidHolderBinding)
    };
    let point = EmbeddedGroupAffine::new(
        decode(&request.public_key_x)?,
        decode(&request.public_key_y)?,
    )
    .ok_or(BoundCredentialError::InvalidHolderBinding)?;
    if point.is_identity() {
        Err(BoundCredentialError::InvalidHolderBinding)
    } else {
        Ok(())
    }
}

fn encode_credential(credential: &CompactCredential) -> Result<Vec<u8>, BoundCredentialError> {
    Ok(encode_mcv1(&[
        integer_chunk(credential.version),
        bytes_chunk(&credential.package_id),
        bytes_chunk(&credential.schema_id),
        integer_chunk(credential.major_version),
        integer_chunk(credential.minor_version),
        bytes_chunk(&credential.issuer.did_contract_address),
        bytes_chunk(&credential.issuer.method_id),
        bytes_chunk(&credential.holder.did_contract_address),
        bytes_chunk(&credential.holder.method_id),
        integer_chunk(credential.issued_at),
        if credential.has_expiration {
            vec![1]
        } else {
            Vec::new()
        },
        integer_chunk(credential.expires_at),
        bytes_chunk(&credential.commitments.first_name),
        bytes_chunk(&credential.commitments.last_name),
        bytes_chunk(&credential.commitments.date_of_birth),
        bytes_chunk(&credential.commitments.document_number),
        bytes_chunk(&credential.commitments.issuing_state),
        bytes_chunk(&credential.commitments.claim_root),
    ]))
}

pub(crate) fn encode_proof(proof: &CompactProof) -> Result<Vec<u8>, BoundCredentialError> {
    let public_x = proof
        .public_key
        .x()
        .ok_or(BoundCredentialError::Unavailable)?;
    let public_y = proof
        .public_key
        .y()
        .ok_or(BoundCredentialError::Unavailable)?;
    let announcement_x = proof
        .announcement
        .x()
        .ok_or(BoundCredentialError::Unavailable)?;
    let announcement_y = proof
        .announcement
        .y()
        .ok_or(BoundCredentialError::Unavailable)?;
    Ok(encode_mcv1(&[
        bytes_chunk(&proof.signer.did_contract_address),
        bytes_chunk(&proof.signer.method_id),
        integer_chunk(proof.created_at),
        bytes_chunk(&proof.challenge_hash),
        bytes_chunk(&public_x.as_le_bytes()),
        bytes_chunk(&public_y.as_le_bytes()),
        bytes_chunk(&announcement_x.as_le_bytes()),
        bytes_chunk(&announcement_y.as_le_bytes()),
        bytes_chunk(&proof.response.as_le_bytes()),
    ]))
}

fn encode_mcv1(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(MCV1_MAGIC);
    output.extend_from_slice(&(chunks.len() as u32).to_be_bytes());
    for chunk in chunks {
        output.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
        output.extend_from_slice(chunk);
    }
    output
}

fn bytes_chunk(bytes: &[u8]) -> Vec<u8> {
    let length = bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |index| index + 1);
    bytes[..length].to_vec()
}

fn integer_chunk<T: Into<u64>>(value: T) -> Vec<u8> {
    bytes_chunk(&value.into().to_le_bytes())
}

pub(crate) fn credential_body_root(credential: &CompactCredential) -> [u8; 32] {
    persistent_hash(&(
        credential.version,
        (
            credential.package_id,
            credential.schema_id,
            credential.major_version,
            credential.minor_version,
        ),
        (
            credential.issuer.did_contract_address,
            credential.issuer.method_id,
        ),
        (
            credential.holder.did_contract_address,
            credential.holder.method_id,
        ),
        (),
        credential.issued_at,
        credential.has_expiration,
        credential.expires_at,
        (),
        (
            credential.commitments.first_name,
            credential.commitments.last_name,
            credential.commitments.date_of_birth,
            credential.commitments.document_number,
            credential.commitments.issuing_state,
        ),
        credential.commitments.claim_root,
    ))
}

fn claim_root(commitments: &DigitalPassportCommitments) -> [u8; 32] {
    persistent_hash(&(
        padded::<32>(b"midnight:vc:digital-passport:v1"),
        commitments.first_name,
        commitments.last_name,
        commitments.date_of_birth,
        commitments.document_number,
        commitments.issuing_state,
    ))
}

pub(crate) fn transient_hash_value<T>(value: T) -> Fr
where
    midnight_base_crypto::fab::AlignedValue: From<T>,
{
    let aligned = ValueReprAlignedValue(midnight_base_crypto::fab::AlignedValue::from(value));
    transient_hash(&aligned.field_vec())
}

pub(crate) fn persistent_hash<T: BinaryHashRepr + ?Sized>(value: &T) -> [u8; 32] {
    let mut writer = PersistentHashWriter::new();
    value.binary_repr(&mut writer);
    writer.finalize().0
}

fn did_from_contract_address(address: [u8; 32]) -> String {
    format!("did:midnight:undeployed:{}", hex::encode(address))
}

const fn padded<const N: usize>(value: &[u8]) -> [u8; N] {
    let mut padded = [0; N];
    let mut index = 0;
    while index < value.len() && index < N {
        padded[index] = value[index];
        index += 1;
    }
    padded
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose};

    use super::*;

    const BODY_ROOT: [u8; 32] = [
        0xb4, 0x2f, 0x11, 0x15, 0x04, 0x2c, 0xef, 0xec, 0xbd, 0x53, 0x80, 0xa0, 0xa6, 0x30, 0xc0,
        0xef, 0x5f, 0x18, 0xbb, 0x13, 0xe7, 0x61, 0x5c, 0xb1, 0xde, 0x9d, 0x36, 0x25, 0x6f, 0x10,
        0x04, 0x32,
    ];

    fn fixture(value: &str) -> Vec<u8> {
        general_purpose::STANDARD
            .decode(value.trim())
            .expect("fixture base64")
    }

    fn bound_request() -> BoundCredentialRequest {
        let holder_key = EmbeddedGroupAffine::generator() * EmbeddedFr::from(987_654_321_u64);
        BoundCredentialRequest {
            holder_did: format!("did:midnight:undeployed:{}", "ab".repeat(32)),
            holder_binding_method_id: format!(
                "did:midnight:undeployed:{}#holder-jubjub-1",
                "ab".repeat(32)
            ),
            public_key_x: general_purpose::URL_SAFE_NO_PAD
                .encode(holder_key.x().expect("holder x").as_le_bytes()),
            public_key_y: general_purpose::URL_SAFE_NO_PAD
                .encode(holder_key.y().expect("holder y").as_le_bytes()),
        }
    }

    #[test]
    fn matches_upstream_body_root_and_issuance_proof() {
        let body = fixture(super::super::STANDALONE_COMPACT_CREDENTIAL_B64);
        let proof = fixture(super::super::STANDALONE_COMPACT_PROOF_B64);
        let credential = parse_credential(&body).expect("credential");
        let proof_value = parse_proof(&proof).expect("proof");
        assert_eq!(
            encode_credential(&credential).expect("credential codec"),
            body
        );
        assert_eq!(encode_proof(&proof_value).expect("proof codec"), proof);
        assert_eq!(credential_body_root(&credential), BODY_ROOT);
        assert!(verify_issuance_proof(&credential, &proof_value));
        let inspected = super::inspect(&body, Some(&proof)).expect("inspection");
        assert_eq!(inspected.verification.outcome(), VerificationOutcome::Valid);
        assert_eq!(
            inspected.metadata.format(),
            CredentialFormat::MidnightCompactVc
        );
        for stage in inspected.verification.stages() {
            let expected = if matches!(
                stage.name(),
                VerificationStageName::Structural
                    | VerificationStageName::Proof
                    | VerificationStageName::Schema
            ) {
                VerificationStageStatus::Passed
            } else {
                VerificationStageStatus::NotChecked
            };
            assert_eq!(stage.status(), expected, "{:?}", stage.name());
        }
    }

    #[test]
    fn reissues_the_exact_bundle_for_the_selected_holder_method() {
        let request = bound_request();
        let issued =
            issue_bound_standalone_credential(&request, 1_700_000_000).expect("bound issuance");
        let proof = issued.detached_proof.as_deref().expect("detached proof");
        let credential = parse_credential(&issued.signed_bytes).expect("credential");
        let parsed_proof = parse_proof(proof).expect("proof");

        assert_eq!(credential.holder.did_contract_address, [0xab; 32]);
        assert_eq!(
            bytes_chunk(&credential.holder.method_id),
            b"#holder-jubjub-1"
        );
        assert!(verify_issuance_proof(&credential, &parsed_proof));
        assert_ne!(
            issued.signed_bytes,
            super::super::standalone_compact_credential()
        );
        assert_eq!(
            issued.private_material,
            Some(super::super::standalone_private_material())
        );
        let inspected = super::inspect(&issued.signed_bytes, Some(proof)).expect("inspection");
        assert_eq!(inspected.verification.outcome(), VerificationOutcome::Valid);
        assert_eq!(
            inspected.metadata.subject_did(),
            Some(request.holder_did.as_str())
        );
    }

    #[test]
    fn rejects_noncanonical_holder_references_and_public_keys() {
        let mut request = bound_request();
        request.holder_binding_method_id = format!("{}#{}", request.holder_did, "x".repeat(32));
        assert_eq!(
            issue_bound_standalone_credential(&request, 1_700_000_000),
            Err(BoundCredentialError::InvalidHolderBinding)
        );

        let mut request = bound_request();
        request.public_key_x.push('=');
        assert_eq!(
            issue_bound_standalone_credential(&request, 1_700_000_000),
            Err(BoundCredentialError::InvalidHolderBinding)
        );

        let mut request = bound_request();
        request.holder_did = format!("did:midnight:undeployed:{}", "AB".repeat(32));
        request.holder_binding_method_id = format!("{}#holder-jubjub-1", request.holder_did);
        assert_eq!(
            issue_bound_standalone_credential(&request, 1_700_000_000),
            Err(BoundCredentialError::InvalidHolderBinding)
        );
    }

    #[test]
    fn rejects_body_proof_and_container_tampering() {
        let body = fixture(super::super::STANDALONE_COMPACT_CREDENTIAL_B64);
        let proof = fixture(super::super::STANDALONE_COMPACT_PROOF_B64);

        let mut tampered_body = body.clone();
        *tampered_body.last_mut().expect("body byte") ^= 1;
        let inspected = super::inspect(&tampered_body, Some(&proof)).expect("inspection");
        assert_eq!(
            inspected.verification.outcome(),
            VerificationOutcome::Invalid
        );

        let mut tampered_proof = proof.clone();
        *tampered_proof.last_mut().expect("proof byte") ^= 1;
        let inspected = super::inspect(&body, Some(&tampered_proof)).expect("inspection");
        assert_eq!(
            inspected.verification.outcome(),
            VerificationOutcome::Invalid
        );

        let mut trailing = body.clone();
        trailing.push(0);
        assert_eq!(
            super::inspect(&trailing, Some(&proof)),
            Err(CredentialVerificationError::InvalidCredential)
        );

        let inspected = super::inspect(&body, None).expect("inspection");
        assert_eq!(
            inspected.verification.outcome(),
            VerificationOutcome::Invalid
        );

        assert_eq!(
            point(&[], &[]),
            Err(CredentialVerificationError::InvalidCredential)
        );
    }
}
