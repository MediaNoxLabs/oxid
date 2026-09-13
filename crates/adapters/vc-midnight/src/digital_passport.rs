// SPDX-License-Identifier: Apache-2.0

//! Digital Passport protected-claim adapter.
//!
//! This module mirrors the reference Compact schema's five claim commitments
//! using the pinned Midnight persistent hash primitives. The private-material
//! envelope is adapter-owned and intentionally opaque to credential core.

use std::collections::BTreeSet;

use ciborium::Value;
use midnight_base_crypto::{
    fab::AlignedValue,
    hash::{HashOutput, PersistentHashWriter, persistent_commit},
    repr::BinaryHashRepr,
};
use midnight_transient_crypto::fab::ValueReprAlignedValue;
use oxid_credential_application::{
    CredentialDisclosurePort, CredentialDisclosurePortError, CredentialLocalClaim,
};
use oxid_credential_domain::{
    CredentialClaimPrivacy, CredentialDisclosureCandidate, CredentialDisclosureManifest,
    MAX_CREDENTIAL_PRIVATE_MATERIAL_BYTES, MAX_SIGNED_CREDENTIAL_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest as _, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const PACKAGE_ID: &str = "midnight:vc:digital-passport";
pub const SCHEMA_ID: &str = "digital-passport:v1";
pub const CLAIM_FIRST_NAME: &str = "/credentialSubject/firstName";
pub const CLAIM_LAST_NAME: &str = "/credentialSubject/lastName";
pub const CLAIM_DATE_OF_BIRTH: &str = "/credentialSubject/dateOfBirth";
pub const CLAIM_DOCUMENT_NUMBER: &str = "/credentialSubject/documentNumber";
pub const CLAIM_ISSUING_STATE: &str = "/credentialSubject/issuingState";
pub const STANDALONE_DIGITAL_PASSPORT_CREDENTIAL_B64: &str =
    include_str!("../../../../fixtures/credentials/standalone-digital-passport-phase1.b64");

const PRIVATE_MATERIAL_VERSION: u64 = 1;
const CLAIM_ROOT_DOMAIN: &[u8] = b"midnight:vc:digital-passport:v1";
const NULL_COMMITMENT_DOMAIN: &[u8] = b"midnight:vc:digital-passport:nil";
const NULL_DOCUMENT_LABEL: &[u8] = b"document-number";

#[derive(Clone, Copy, Debug, Default)]
pub struct DigitalPassportDisclosureAdapter;

#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub(crate) struct ClaimValues {
    pub(crate) first_name: [u8; 64],
    pub(crate) last_name: [u8; 64],
    pub(crate) date_of_birth_days: u32,
    pub(crate) document_number: [u8; 32],
    pub(crate) issuing_state: [u8; 32],
}

#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub(crate) struct ClaimOpenings {
    pub(crate) first_name: [u8; 32],
    pub(crate) last_name: [u8; 32],
    pub(crate) date_of_birth: [u8; 32],
    pub(crate) document_number: [u8; 32],
    pub(crate) issuing_state: [u8; 32],
}

#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub(crate) struct PrivateParts {
    pub(crate) values: ClaimValues,
    pub(crate) openings: ClaimOpenings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DigitalPassportCommitments {
    pub first_name: [u8; 32],
    pub last_name: [u8; 32],
    pub date_of_birth: [u8; 32],
    pub document_number: [u8; 32],
    pub issuing_state: [u8; 32],
    pub claim_root: [u8; 32],
}

impl CredentialDisclosurePort for DigitalPassportDisclosureAdapter {
    fn inspect(
        &self,
        signed_bytes: &[u8],
        private_material: &[u8],
    ) -> Result<CredentialDisclosureManifest, CredentialDisclosurePortError> {
        let (commitments, _) = validated_private_parts(signed_bytes, private_material)?;
        manifest(&commitments)
    }

    fn reveal_local(
        &self,
        signed_bytes: &[u8],
        private_material: &[u8],
        claim_path: &str,
    ) -> Result<CredentialLocalClaim, CredentialDisclosurePortError> {
        let (commitments, private_parts) = validated_private_parts(signed_bytes, private_material)?;
        let value = match claim_path {
            CLAIM_FIRST_NAME => decode_padded_text(&private_parts.values.first_name)?,
            CLAIM_LAST_NAME => decode_padded_text(&private_parts.values.last_name)?,
            CLAIM_DOCUMENT_NUMBER
                if commitments.document_number != document_number_null_commitment() =>
            {
                decode_padded_text(&private_parts.values.document_number)?
            }
            CLAIM_ISSUING_STATE => decode_padded_text(&private_parts.values.issuing_state)?,
            CLAIM_DATE_OF_BIRTH => {
                return Err(CredentialDisclosurePortError::ClaimNotRevealable);
            }
            _ => return Err(CredentialDisclosurePortError::ClaimNotFound),
        };
        CredentialLocalClaim::new(claim_path, value)
    }
}

fn manifest(
    commitments: &DigitalPassportCommitments,
) -> Result<CredentialDisclosureManifest, CredentialDisclosurePortError> {
    let mut candidates = vec![
        candidate(
            CLAIM_FIRST_NAME,
            "First name",
            CredentialClaimPrivacy::SelectiveDisclosure,
        )?,
        candidate(
            CLAIM_LAST_NAME,
            "Last name",
            CredentialClaimPrivacy::SelectiveDisclosure,
        )?,
        candidate(
            CLAIM_DATE_OF_BIRTH,
            "Age over threshold",
            CredentialClaimPrivacy::PredicateOnly,
        )?,
    ];
    if commitments.document_number != document_number_null_commitment() {
        candidates.push(candidate(
            CLAIM_DOCUMENT_NUMBER,
            "Document number",
            CredentialClaimPrivacy::SelectiveDisclosure,
        )?);
    }
    candidates.push(candidate(
        CLAIM_ISSUING_STATE,
        "Issuing state",
        CredentialClaimPrivacy::SelectiveDisclosure,
    )?);
    CredentialDisclosureManifest::new(SCHEMA_ID, candidates)
        .map_err(|_| CredentialDisclosurePortError::InvalidPrivateMaterial)
}

fn candidate(
    path: &str,
    label: &str,
    privacy: CredentialClaimPrivacy,
) -> Result<CredentialDisclosureCandidate, CredentialDisclosurePortError> {
    CredentialDisclosureCandidate::new(path, label, privacy)
        .map_err(|_| CredentialDisclosurePortError::InvalidPrivateMaterial)
}

/// Returns the deterministic public test private-parts envelope. The values
/// match the reference package's cross-language fixture at commit
/// `39b1354212620b396e914b29603e6a38f2656546`.
pub fn standalone_private_material() -> Vec<u8> {
    encode_private_parts(&standalone_private_parts())
        .expect("constant Digital Passport fixture must encode")
}

/// Returns the detached public signed fixture used by standalone issuance.
pub fn standalone_credential() -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(STANDALONE_DIGITAL_PASSPORT_CREDENTIAL_B64.trim())
        .expect("checked-in Digital Passport fixture must be valid base64")
}

#[must_use]
pub fn standalone_commitments() -> DigitalPassportCommitments {
    commitments(&standalone_private_parts())
}

fn standalone_private_parts() -> PrivateParts {
    PrivateParts {
        values: ClaimValues {
            first_name: pad_text::<64>(b"Alice"),
            last_name: pad_text::<64>(b"Example"),
            date_of_birth_days: 3_650,
            document_number: pad_text::<32>(b"AB1234567"),
            issuing_state: pad_text::<32>(b"US"),
        },
        openings: ClaimOpenings {
            first_name: sha256(b"opening:first-name"),
            last_name: sha256(b"opening:last-name"),
            date_of_birth: sha256(b"opening:date-of-birth"),
            document_number: sha256(b"opening:document-number"),
            issuing_state: sha256(b"opening:issuing-state"),
        },
    }
}

fn pad_text<const N: usize>(text: &[u8]) -> [u8; N] {
    let mut padded = [0_u8; N];
    padded[..text.len()].copy_from_slice(text);
    padded
}

fn sha256(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

fn commitments(private_parts: &PrivateParts) -> DigitalPassportCommitments {
    let document_number = commit(
        private_parts.values.document_number,
        private_parts.openings.document_number,
    );
    let mut result = DigitalPassportCommitments {
        first_name: commit(
            private_parts.values.first_name,
            private_parts.openings.first_name,
        ),
        last_name: commit(
            private_parts.values.last_name,
            private_parts.openings.last_name,
        ),
        date_of_birth: commit(
            private_parts.values.date_of_birth_days,
            private_parts.openings.date_of_birth,
        ),
        document_number,
        issuing_state: commit(
            private_parts.values.issuing_state,
            private_parts.openings.issuing_state,
        ),
        claim_root: [0; 32],
    };
    result.claim_root = claim_root(&result);
    result
}

fn validate_private_parts(
    expected: &DigitalPassportCommitments,
    private_parts: &PrivateParts,
) -> Result<(), CredentialDisclosurePortError> {
    let actual = commitments(private_parts);
    let document_matches = if expected.document_number == document_number_null_commitment() {
        private_parts.values.document_number == [0; 32]
            && private_parts.openings.document_number == [0; 32]
    } else {
        expected.document_number == actual.document_number
    };
    if expected.first_name != actual.first_name
        || expected.last_name != actual.last_name
        || expected.date_of_birth != actual.date_of_birth
        || !document_matches
        || expected.issuing_state != actual.issuing_state
        || expected.claim_root != claim_root(expected)
    {
        return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
    }
    decode_padded_text(&private_parts.values.first_name)?;
    decode_padded_text(&private_parts.values.last_name)?;
    decode_padded_text(&private_parts.values.issuing_state)?;
    if expected.document_number != document_number_null_commitment() {
        decode_padded_text(&private_parts.values.document_number)?;
    }
    Ok(())
}

pub(crate) fn commit<T>(value: T, opening: [u8; 32]) -> [u8; 32]
where
    T: midnight_base_crypto::fab::DynAligned,
    midnight_base_crypto::fab::Value: From<T>,
{
    persistent_commit(
        &ValueReprAlignedValue(AlignedValue::from(value)),
        HashOutput(opening),
    )
    .0
}

fn claim_root(commitments: &DigitalPassportCommitments) -> [u8; 32] {
    let value = (
        pad_text::<32>(CLAIM_ROOT_DOMAIN),
        commitments.first_name,
        commitments.last_name,
        commitments.date_of_birth,
        commitments.document_number,
        commitments.issuing_state,
    );
    let mut writer = PersistentHashWriter::new();
    ValueReprAlignedValue(AlignedValue::from(value)).binary_repr(&mut writer);
    writer.finalize().0
}

pub(crate) fn document_number_null_commitment() -> [u8; 32] {
    let value = (
        pad_text::<32>(NULL_COMMITMENT_DOMAIN),
        pad_text::<32>(NULL_DOCUMENT_LABEL),
    );
    let mut writer = PersistentHashWriter::new();
    ValueReprAlignedValue(AlignedValue::from(value)).binary_repr(&mut writer);
    writer.finalize().0
}

fn parse_public_commitments(
    signed_bytes: &[u8],
) -> Result<DigitalPassportCommitments, CredentialDisclosurePortError> {
    if signed_bytes.is_empty() || signed_bytes.len() > MAX_SIGNED_CREDENTIAL_BYTES {
        return Err(CredentialDisclosurePortError::UnsupportedCredential);
    }
    if signed_bytes.starts_with(b"MCV1") {
        return super::compact_digital_passport::parse_credential(signed_bytes)
            .map(|credential| credential.commitments)
            .map_err(|_| CredentialDisclosurePortError::UnsupportedCredential);
    }
    let mut input = signed_bytes;
    let value: Value = ciborium::de::from_reader_with_recursion_limit(&mut input, 32)
        .map_err(|_| CredentialDisclosurePortError::UnsupportedCredential)?;
    if !input.is_empty() {
        return Err(CredentialDisclosurePortError::UnsupportedCredential);
    }
    let root = value
        .as_map()
        .ok_or(CredentialDisclosurePortError::UnsupportedCredential)?;
    let types = optional_unique(root, "type")?
        .and_then(Value::as_array)
        .ok_or(CredentialDisclosurePortError::UnsupportedCredential)?;
    if !types
        .iter()
        .any(|value| value.as_text() == Some("DigitalPassportCredential"))
    {
        return Err(CredentialDisclosurePortError::UnsupportedCredential);
    }
    let subject = required_map(root, "credentialSubject")?;
    let passport = required_map(subject, "digitalPassport")?;
    strict_keys(
        passport,
        &["packageId", "schemaId", "claimCommitments", "claimRoot"],
    )?;
    if required_text(passport, "packageId")? != PACKAGE_ID
        || required_text(passport, "schemaId")? != SCHEMA_ID
    {
        return Err(CredentialDisclosurePortError::UnsupportedCredential);
    }
    let values = required_map(passport, "claimCommitments")?;
    strict_keys(
        values,
        &[
            "firstNameCommitment",
            "lastNameCommitment",
            "dateOfBirthCommitment",
            "documentNumberCommitment",
            "issuingStateCommitment",
        ],
    )?;
    Ok(DigitalPassportCommitments {
        first_name: required_bytes(values, "firstNameCommitment")?,
        last_name: required_bytes(values, "lastNameCommitment")?,
        date_of_birth: required_bytes(values, "dateOfBirthCommitment")?,
        document_number: required_bytes(values, "documentNumberCommitment")?,
        issuing_state: required_bytes(values, "issuingStateCommitment")?,
        claim_root: required_bytes(passport, "claimRoot")?,
    })
}

/// Payload-free failure returned when Portal private claim/opening JSON cannot
/// be converted into Oxid's adapter-owned private-material envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalPrivateMaterialError {
    Invalid,
}

impl std::fmt::Display for PortalPrivateMaterialError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid Portal credential private material")
    }
}

impl std::error::Error for PortalPrivateMaterialError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PortalClaimValues<'a> {
    #[serde(borrow)]
    first_name_value_padded: &'a RawValue,
    #[serde(borrow)]
    last_name_value_padded: &'a RawValue,
    date_of_birth_days: u32,
    #[serde(borrow)]
    document_number_value: &'a RawValue,
    #[serde(borrow)]
    issuing_state_value: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PortalOpenings<'a> {
    #[serde(borrow)]
    first_name_opening: &'a RawValue,
    #[serde(borrow)]
    last_name_opening: &'a RawValue,
    #[serde(borrow)]
    date_of_birth_opening: &'a RawValue,
    #[serde(borrow)]
    document_number_opening: &'a RawValue,
    #[serde(borrow)]
    issuing_state_opening: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PortalPrivateParts<'a> {
    #[serde(borrow)]
    claim_values: PortalClaimValues<'a>,
    #[serde(borrow)]
    openings: PortalOpenings<'a>,
}

/// Strictly converts Portal's `credentialPrivateParts` JSON object to the
/// canonical existing five-field CBOR private envelope and verifies that every
/// value/opening pair matches the commitments in `signed_credential`.
///
/// This boundary deliberately accepts only bytes. It exposes neither claims
/// nor application-layer types to the OpenID adapter.
pub fn convert_portal_private_parts(
    signed_credential: &[u8],
    portal_json: &[u8],
) -> Result<Vec<u8>, PortalPrivateMaterialError> {
    if portal_json.is_empty() || portal_json.len() > MAX_CREDENTIAL_PRIVATE_MATERIAL_BYTES {
        return Err(PortalPrivateMaterialError::Invalid);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(portal_json);
    let value = PortalPrivateParts::deserialize(&mut deserializer)
        .map_err(|_| PortalPrivateMaterialError::Invalid)?;
    deserializer
        .end()
        .map_err(|_| PortalPrivateMaterialError::Invalid)?;

    fn decode_json_string(
        raw: &RawValue,
    ) -> Result<Zeroizing<Vec<u8>>, PortalPrivateMaterialError> {
        let source = raw.get().as_bytes();
        if source.len() < 2 || source[0] != b'"' || source[source.len() - 1] != b'"' {
            return Err(PortalPrivateMaterialError::Invalid);
        }
        let mut decoded = Zeroizing::new(Vec::with_capacity(source.len() - 2));
        let mut index = 1;
        while index < source.len() - 1 {
            let byte = source[index];
            index += 1;
            if byte != b'\\' {
                if byte < 0x20 || byte == b'"' {
                    return Err(PortalPrivateMaterialError::Invalid);
                }
                decoded.push(byte);
                continue;
            }
            if index >= source.len() - 1 {
                return Err(PortalPrivateMaterialError::Invalid);
            }
            let escaped = source[index];
            index += 1;
            match escaped {
                b'"' | b'\\' | b'/' => decoded.push(escaped),
                b'b' => decoded.push(0x08),
                b'f' => decoded.push(0x0c),
                b'n' => decoded.push(b'\n'),
                b'r' => decoded.push(b'\r'),
                b't' => decoded.push(b'\t'),
                b'u' => {
                    let end = index
                        .checked_add(4)
                        .filter(|end| *end < source.len())
                        .ok_or(PortalPrivateMaterialError::Invalid)?;
                    let code = std::str::from_utf8(&source[index..end])
                        .ok()
                        .and_then(|digits| u16::from_str_radix(digits, 16).ok())
                        .filter(|code| *code <= 0x7f)
                        .ok_or(PortalPrivateMaterialError::Invalid)?;
                    decoded.push(code as u8);
                    index = end;
                }
                _ => return Err(PortalPrivateMaterialError::Invalid),
            }
        }
        Ok(decoded)
    }

    fn decode<const N: usize>(
        input: &RawValue,
    ) -> Result<Zeroizing<[u8; N]>, PortalPrivateMaterialError> {
        use base64::{Engine as _, engine::general_purpose};

        let input = decode_json_string(input)?;
        let mut decoded = Zeroizing::new(Vec::new());
        general_purpose::URL_SAFE_NO_PAD
            .decode_vec(&input, &mut decoded)
            .map_err(|_| PortalPrivateMaterialError::Invalid)?;
        if decoded.len() != N {
            return Err(PortalPrivateMaterialError::Invalid);
        }
        let mut bytes = Zeroizing::new([0_u8; N]);
        bytes.copy_from_slice(&decoded);
        let canonical = Zeroizing::new(general_purpose::URL_SAFE_NO_PAD.encode(*bytes));
        if canonical.as_bytes() != input.as_slice() {
            return Err(PortalPrivateMaterialError::Invalid);
        }
        Ok(bytes)
    }

    // Keep every independently decoded value in a zeroizing owner so that a
    // later-field failure also wipes values decoded earlier in this sequence.
    let first_name = decode(value.claim_values.first_name_value_padded)?;
    let last_name = decode(value.claim_values.last_name_value_padded)?;
    let document_number = decode(value.claim_values.document_number_value)?;
    let issuing_state = decode(value.claim_values.issuing_state_value)?;
    let first_name_opening = decode(value.openings.first_name_opening)?;
    let last_name_opening = decode(value.openings.last_name_opening)?;
    let date_of_birth_opening = decode(value.openings.date_of_birth_opening)?;
    let document_number_opening = decode(value.openings.document_number_opening)?;
    let issuing_state_opening = decode(value.openings.issuing_state_opening)?;
    let parts = PrivateParts {
        values: ClaimValues {
            first_name: *first_name,
            last_name: *last_name,
            date_of_birth_days: value.claim_values.date_of_birth_days,
            document_number: *document_number,
            issuing_state: *issuing_state,
        },
        openings: ClaimOpenings {
            first_name: *first_name_opening,
            last_name: *last_name_opening,
            date_of_birth: *date_of_birth_opening,
            document_number: *document_number_opening,
            issuing_state: *issuing_state_opening,
        },
    };
    let commitments = parse_public_commitments(signed_credential)
        .map_err(|_| PortalPrivateMaterialError::Invalid)?;
    validate_private_parts(&commitments, &parts)
        .map_err(|_| PortalPrivateMaterialError::Invalid)?;
    encode_private_parts(&parts).map_err(|_| PortalPrivateMaterialError::Invalid)
}

fn parse_private_parts(bytes: &[u8]) -> Result<PrivateParts, CredentialDisclosurePortError> {
    if bytes.is_empty() || bytes.len() > MAX_CREDENTIAL_PRIVATE_MATERIAL_BYTES {
        return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
    }
    let mut input = bytes;
    let value: Value = ciborium::de::from_reader_with_recursion_limit(&mut input, 8)
        .map_err(|_| CredentialDisclosurePortError::InvalidPrivateMaterial)?;
    if !input.is_empty() {
        return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
    }
    let root = value
        .as_map()
        .ok_or(CredentialDisclosurePortError::InvalidPrivateMaterial)?;
    strict_keys(root, &["version", "claimValues", "openings"])?;
    if required_u64(root, "version")? != PRIVATE_MATERIAL_VERSION {
        return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
    }
    let values = required_map(root, "claimValues")?;
    strict_keys(
        values,
        &[
            "firstNameValuePadded",
            "lastNameValuePadded",
            "dateOfBirthDays",
            "documentNumberValue",
            "issuingStateValue",
        ],
    )?;
    let openings = required_map(root, "openings")?;
    strict_keys(
        openings,
        &[
            "firstNameOpening",
            "lastNameOpening",
            "dateOfBirthOpening",
            "documentNumberOpening",
            "issuingStateOpening",
        ],
    )?;
    Ok(PrivateParts {
        values: ClaimValues {
            first_name: required_bytes(values, "firstNameValuePadded")?,
            last_name: required_bytes(values, "lastNameValuePadded")?,
            date_of_birth_days: u32::try_from(required_u64(values, "dateOfBirthDays")?)
                .map_err(|_| CredentialDisclosurePortError::InvalidPrivateMaterial)?,
            document_number: required_bytes(values, "documentNumberValue")?,
            issuing_state: required_bytes(values, "issuingStateValue")?,
        },
        openings: ClaimOpenings {
            first_name: required_bytes(openings, "firstNameOpening")?,
            last_name: required_bytes(openings, "lastNameOpening")?,
            date_of_birth: required_bytes(openings, "dateOfBirthOpening")?,
            document_number: required_bytes(openings, "documentNumberOpening")?,
            issuing_state: required_bytes(openings, "issuingStateOpening")?,
        },
    })
}

pub(crate) fn validated_private_parts(
    signed_bytes: &[u8],
    private_material: &[u8],
) -> Result<(DigitalPassportCommitments, PrivateParts), CredentialDisclosurePortError> {
    let commitments = parse_public_commitments(signed_bytes)?;
    let private_parts = parse_private_parts(private_material)?;
    validate_private_parts(&commitments, &private_parts)?;
    Ok((commitments, private_parts))
}

fn encode_private_parts(
    private_parts: &PrivateParts,
) -> Result<Vec<u8>, CredentialDisclosurePortError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EncodedClaimValues<'a> {
        #[serde(with = "serde_bytes")]
        first_name_value_padded: &'a [u8],
        #[serde(with = "serde_bytes")]
        last_name_value_padded: &'a [u8],
        date_of_birth_days: u32,
        #[serde(with = "serde_bytes")]
        document_number_value: &'a [u8],
        #[serde(with = "serde_bytes")]
        issuing_state_value: &'a [u8],
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EncodedOpenings<'a> {
        #[serde(with = "serde_bytes")]
        first_name_opening: &'a [u8],
        #[serde(with = "serde_bytes")]
        last_name_opening: &'a [u8],
        #[serde(with = "serde_bytes")]
        date_of_birth_opening: &'a [u8],
        #[serde(with = "serde_bytes")]
        document_number_opening: &'a [u8],
        #[serde(with = "serde_bytes")]
        issuing_state_opening: &'a [u8],
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EncodedPrivateParts<'a> {
        version: u64,
        claim_values: EncodedClaimValues<'a>,
        openings: EncodedOpenings<'a>,
    }

    let value = EncodedPrivateParts {
        version: PRIVATE_MATERIAL_VERSION,
        claim_values: EncodedClaimValues {
            first_name_value_padded: &private_parts.values.first_name,
            last_name_value_padded: &private_parts.values.last_name,
            date_of_birth_days: private_parts.values.date_of_birth_days,
            document_number_value: &private_parts.values.document_number,
            issuing_state_value: &private_parts.values.issuing_state,
        },
        openings: EncodedOpenings {
            first_name_opening: &private_parts.openings.first_name,
            last_name_opening: &private_parts.openings.last_name,
            date_of_birth_opening: &private_parts.openings.date_of_birth,
            document_number_opening: &private_parts.openings.document_number,
            issuing_state_opening: &private_parts.openings.issuing_state,
        },
    };
    let mut output = Zeroizing::new(Vec::new());
    ciborium::into_writer(&value, &mut *output)
        .map_err(|_| CredentialDisclosurePortError::InvalidPrivateMaterial)?;
    Ok(std::mem::take(&mut output))
}

fn strict_keys(
    map: &[(Value, Value)],
    expected: &[&str],
) -> Result<(), CredentialDisclosurePortError> {
    if map.len() != expected.len() {
        return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
    }
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    let actual = map
        .iter()
        .map(|(key, _)| {
            key.as_text()
                .ok_or(CredentialDisclosurePortError::InvalidPrivateMaterial)
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if actual.len() != map.len() || actual != expected {
        return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
    }
    Ok(())
}

fn optional_unique<'a>(
    map: &'a [(Value, Value)],
    key: &str,
) -> Result<Option<&'a Value>, CredentialDisclosurePortError> {
    let mut values = map
        .iter()
        .filter_map(|(candidate, value)| (candidate.as_text() == Some(key)).then_some(value));
    let value = values.next();
    if values.next().is_some() {
        return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
    }
    Ok(value)
}

fn required<'a>(
    map: &'a [(Value, Value)],
    key: &str,
) -> Result<&'a Value, CredentialDisclosurePortError> {
    optional_unique(map, key)?.ok_or(CredentialDisclosurePortError::InvalidPrivateMaterial)
}

fn required_map<'a>(
    map: &'a [(Value, Value)],
    key: &str,
) -> Result<&'a [(Value, Value)], CredentialDisclosurePortError> {
    required(map, key)?
        .as_map()
        .map(Vec::as_slice)
        .ok_or(CredentialDisclosurePortError::InvalidPrivateMaterial)
}

fn required_text<'a>(
    map: &'a [(Value, Value)],
    key: &str,
) -> Result<&'a str, CredentialDisclosurePortError> {
    required(map, key)?
        .as_text()
        .ok_or(CredentialDisclosurePortError::InvalidPrivateMaterial)
}

fn required_bytes<const N: usize>(
    map: &[(Value, Value)],
    key: &str,
) -> Result<[u8; N], CredentialDisclosurePortError> {
    required(map, key)?
        .as_bytes()
        .and_then(|value| <[u8; N]>::try_from(value.as_slice()).ok())
        .ok_or(CredentialDisclosurePortError::InvalidPrivateMaterial)
}

fn required_u64(map: &[(Value, Value)], key: &str) -> Result<u64, CredentialDisclosurePortError> {
    let value = required(map, key)?
        .as_integer()
        .ok_or(CredentialDisclosurePortError::InvalidPrivateMaterial)?;
    u64::try_from(value).map_err(|_| CredentialDisclosurePortError::InvalidPrivateMaterial)
}

fn decode_padded_text<const N: usize>(
    bytes: &[u8; N],
) -> Result<String, CredentialDisclosurePortError> {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(N);
    if end == 0 || bytes[end..].iter().any(|byte| *byte != 0) {
        return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
    }
    let value = std::str::from_utf8(&bytes[..end])
        .map_err(|_| CredentialDisclosurePortError::InvalidPrivateMaterial)?;
    if value.chars().any(|character| {
        character.is_control() || matches!(character, '<' | '>' | '\u{202a}'..='\u{202e}')
    }) {
        return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_compact_cross_language_vectors() {
        let commitments = standalone_commitments();
        assert_eq!(
            hex::encode(commitments.first_name),
            "1e223ae182208a05f8ece3c3c70582d183fc08eb6c4e1e10abaad251a7c6c2a0"
        );
        assert_eq!(
            hex::encode(commitments.last_name),
            "09f2976ab79d7882b4796ec172aa8b724e7c432d3efca70cdd176a52b16e4ad3"
        );
        assert_eq!(
            hex::encode(commitments.date_of_birth),
            "2dfba8882ef4b998264625ca38b4eccbfa5887ae572d8fae8ec80c939f551d07"
        );
        assert_eq!(
            hex::encode(commitments.document_number),
            "6cb043794216f1c6e2484d6444cdbe72ea2e0d2d2d80387f044fedfad9379399"
        );
        assert_eq!(
            hex::encode(commitments.issuing_state),
            "794506dccd0f50879f59b839a20e11fe662ba259b49f17f852fb170a71b6c258"
        );
        assert_eq!(
            hex::encode(commitments.claim_root),
            "a0be50c4abdd41e7eaeff74f5c8d2856b0ecedd01786913f19fc92345a955fc3"
        );
    }

    #[test]
    fn private_codec_rejects_malformed_oversized_tampered_and_duplicate_fields() {
        let material = standalone_private_material();
        let parts = parse_private_parts(&material).expect("private parts");
        let expected = standalone_commitments();
        validate_private_parts(&expected, &parts).expect("valid fixture");

        assert_eq!(
            parse_private_parts(&material[..material.len() - 1]).err(),
            Some(CredentialDisclosurePortError::InvalidPrivateMaterial)
        );
        let mut trailing = material.clone();
        trailing.push(0);
        assert_eq!(
            parse_private_parts(&trailing).err(),
            Some(CredentialDisclosurePortError::InvalidPrivateMaterial)
        );
        assert_eq!(
            parse_private_parts(&vec![0; MAX_CREDENTIAL_PRIVATE_MATERIAL_BYTES + 1]).err(),
            Some(CredentialDisclosurePortError::InvalidPrivateMaterial)
        );

        let mut tampered = parts;
        tampered.openings.first_name[0] ^= 1;
        assert_eq!(
            validate_private_parts(&expected, &tampered),
            Err(CredentialDisclosurePortError::InvalidPrivateMaterial)
        );

        let duplicate = Value::Map(vec![
            (Value::Text("version".to_owned()), Value::Integer(1.into())),
            (Value::Text("version".to_owned()), Value::Integer(1.into())),
            (
                Value::Text("claimValues".to_owned()),
                Value::Map(Vec::new()),
            ),
        ]);
        let mut bytes = Vec::new();
        ciborium::into_writer(&duplicate, &mut bytes).expect("encode malformed fixture");
        assert_eq!(
            parse_private_parts(&bytes).err(),
            Some(CredentialDisclosurePortError::InvalidPrivateMaterial)
        );
    }

    fn portal_private_parts_fixture() -> Vec<u8> {
        use base64::{Engine as _, engine::general_purpose};

        let parts = standalone_private_parts();
        serde_json::to_vec(&serde_json::json!({
            "claimValues": {
                "firstNameValuePadded": general_purpose::URL_SAFE_NO_PAD.encode(parts.values.first_name),
                "lastNameValuePadded": general_purpose::URL_SAFE_NO_PAD.encode(parts.values.last_name),
                "dateOfBirthDays": parts.values.date_of_birth_days,
                "documentNumberValue": general_purpose::URL_SAFE_NO_PAD.encode(parts.values.document_number),
                "issuingStateValue": general_purpose::URL_SAFE_NO_PAD.encode(parts.values.issuing_state)
            },
            "openings": {
                "firstNameOpening": general_purpose::URL_SAFE_NO_PAD.encode(parts.openings.first_name),
                "lastNameOpening": general_purpose::URL_SAFE_NO_PAD.encode(parts.openings.last_name),
                "dateOfBirthOpening": general_purpose::URL_SAFE_NO_PAD.encode(parts.openings.date_of_birth),
                "documentNumberOpening": general_purpose::URL_SAFE_NO_PAD.encode(parts.openings.document_number),
                "issuingStateOpening": general_purpose::URL_SAFE_NO_PAD.encode(parts.openings.issuing_state)
            }
        }))
        .expect("fixture")
    }

    #[test]
    fn portal_private_parts_convert_to_the_exact_validated_cbor_envelope() {
        let converted =
            convert_portal_private_parts(&standalone_credential(), &portal_private_parts_fixture())
                .expect("exact Portal private parts should convert");
        assert_eq!(converted, standalone_private_material());
        validated_private_parts(&standalone_credential(), &converted)
            .expect("converted material must satisfy signed commitments");
    }

    #[test]
    fn portal_private_parts_accept_equivalent_escaped_base64url_text() {
        let fixture = String::from_utf8(portal_private_parts_fixture()).expect("fixture UTF-8");
        let marker = r#""firstNameValuePadded":""#;
        let value_index = fixture.find(marker).expect("first-name field") + marker.len();
        let escaped = format!(
            "{}\\u{:04x}{}",
            &fixture[..value_index],
            fixture.as_bytes()[value_index],
            &fixture[value_index + 1..],
        );
        let converted = convert_portal_private_parts(&standalone_credential(), escaped.as_bytes())
            .expect("equivalent JSON escape should preserve protocol behavior");
        assert_eq!(converted, standalone_private_material());
    }

    #[test]
    fn portal_private_parts_reject_missing_extra_duplicate_malformed_and_tampered_fields() {
        let signed = standalone_credential();
        let fixture = portal_private_parts_fixture();
        let value: serde_json::Value = serde_json::from_slice(&fixture).expect("fixture");

        let mut missing = value.clone();
        missing["openings"]
            .as_object_mut()
            .expect("object")
            .remove("firstNameOpening");
        let mut extra = value.clone();
        extra
            .as_object_mut()
            .expect("object")
            .insert("claims".to_owned(), serde_json::Value::Null);
        let duplicate = format!(
            "{{\"claimValues\":{},\"claimValues\":{},\"openings\":{}}}",
            value["claimValues"], value["claimValues"], value["openings"]
        );
        let mut malformed = value.clone();
        malformed["claimValues"]["firstNameValuePadded"] =
            serde_json::Value::String("not+base64".to_owned());
        let mut tampered = value;
        let opening = tampered["openings"]["firstNameOpening"]
            .as_str()
            .expect("opening");
        let replacement = if opening.starts_with('A') { 'B' } else { 'A' };
        tampered["openings"]["firstNameOpening"] =
            serde_json::Value::String(format!("{replacement}{}", &opening[1..]));

        for rejected in [
            serde_json::to_vec(&missing).expect("json"),
            serde_json::to_vec(&extra).expect("json"),
            duplicate.into_bytes(),
            serde_json::to_vec(&malformed).expect("json"),
            serde_json::to_vec(&tampered).expect("json"),
            b"{} trailing".to_vec(),
        ] {
            assert_eq!(
                convert_portal_private_parts(&signed, &rejected),
                Err(PortalPrivateMaterialError::Invalid),
            );
        }
    }

    #[test]
    fn local_claim_debug_output_is_redacted() {
        let claim = CredentialLocalClaim::new(CLAIM_FIRST_NAME, "Alice").expect("claim");
        let debug = format!("{claim:?}");
        assert!(debug.contains(CLAIM_FIRST_NAME));
        assert!(!debug.contains("Alice"));
    }

    #[test]
    fn signed_fixture_binds_candidates_and_targeted_local_reveal() {
        let adapter = DigitalPassportDisclosureAdapter;
        let signed = standalone_credential();
        let private_material = standalone_private_material();
        let manifest = adapter
            .inspect(&signed, &private_material)
            .expect("fixture must bind");
        assert_eq!(manifest.schema_id(), SCHEMA_ID);
        assert_eq!(manifest.candidates().len(), 5);
        let first_name = adapter
            .reveal_local(&signed, &private_material, CLAIM_FIRST_NAME)
            .expect("local reveal");
        assert_eq!(first_name.value(), "Alice");
        assert_eq!(
            adapter.reveal_local(&signed, &private_material, CLAIM_DATE_OF_BIRTH),
            Err(CredentialDisclosurePortError::ClaimNotRevealable)
        );

        let mut tampered = private_material;
        let last = tampered.last_mut().expect("material is non-empty");
        *last ^= 1;
        assert_eq!(
            adapter.inspect(&signed, &tampered),
            Err(CredentialDisclosurePortError::InvalidPrivateMaterial)
        );
    }

    #[test]
    fn exact_compact_body_binds_the_same_private_parts() {
        let adapter = DigitalPassportDisclosureAdapter;
        let signed = super::super::standalone_compact_credential();
        let private_material = standalone_private_material();
        let manifest = adapter
            .inspect(&signed, &private_material)
            .expect("Compact fixture must bind");
        assert_eq!(manifest.schema_id(), SCHEMA_ID);
        assert_eq!(manifest.candidates().len(), 5);
        assert_eq!(
            adapter
                .reveal_local(&signed, &private_material, CLAIM_LAST_NAME)
                .expect("local reveal")
                .value(),
            "Example"
        );
    }
}
