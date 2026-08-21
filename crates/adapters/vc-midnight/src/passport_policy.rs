// SPDX-License-Identifier: Apache-2.0

//! Exact Digital Passport policy validation used by product-specific local
//! verifier adapters. This module owns schema interpretation; vault state and
//! money movement remain outside the reusable credential adapter.

use base64::{Engine as _, engine::general_purpose};
use midnight_base_crypto::fab::AlignedValue;
use midnight_transient_crypto::{
    curve::{EmbeddedGroupAffine, Fr},
    fab::ValueReprAlignedValue,
};
use oxid_credential_domain::VerificationOutcome;
use oxid_identity_domain::MidnightDid;
use sha2::{Digest as _, Sha256};

use crate::{
    compact_digital_passport::{
        credential_body_root, inspect, parse_credential, parse_proof, persistent_hash,
    },
    digital_passport::validated_private_parts,
    standalone_compact_credential, standalone_compact_proof,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigitalPassportIssuerTrustAnchor {
    issuer_did: String,
    method_id: [u8; 32],
    public_key: [u8; 64],
    public_key_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigitalPassportIssuerTrustAnchorError {
    InvalidIssuer,
    InvalidMethod,
    InvalidPublicKey,
    PublicKeyDigestMismatch,
}

impl std::fmt::Display for DigitalPassportIssuerTrustAnchorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidIssuer => "invalid Digital Passport issuer DID",
            Self::InvalidMethod => "invalid Digital Passport issuer method",
            Self::InvalidPublicKey => "invalid Digital Passport issuer public key",
            Self::PublicKeyDigestMismatch => "Digital Passport issuer public-key digest mismatch",
        })
    }
}

impl std::error::Error for DigitalPassportIssuerTrustAnchorError {}

impl DigitalPassportIssuerTrustAnchor {
    /// Builds the explicit standalone Portal trust anchor from the exact
    /// deployment-manifest issuer and canonical Jubjub JWK. The supplied digest
    /// authenticates the canonical public JWK bytes; the verifier's native
    /// point hash is derived only after the coordinates decode to a valid,
    /// non-identity Jubjub point.
    pub fn from_portal_jubjub(
        issuer_did: &str,
        issuer_method: &str,
        x: &str,
        y: &str,
        expected_jwk_sha256: &str,
    ) -> Result<Self, DigitalPassportIssuerTrustAnchorError> {
        MidnightDid::parse(issuer_did.to_owned())
            .map_err(|_| DigitalPassportIssuerTrustAnchorError::InvalidIssuer)?;
        let fragment = issuer_method
            .strip_prefix(issuer_did)
            .filter(|value| value.starts_with('#'))
            .ok_or(DigitalPassportIssuerTrustAnchorError::InvalidMethod)?;
        if fragment.len() > 32
            || fragment.len() < 2
            || !fragment.bytes().skip(1).all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'%')
            })
        {
            return Err(DigitalPassportIssuerTrustAnchorError::InvalidMethod);
        }
        let mut method_id = [0_u8; 32];
        method_id[..fragment.len()].copy_from_slice(fragment.as_bytes());
        let decode = |value: &str| {
            general_purpose::URL_SAFE_NO_PAD
                .decode(value)
                .ok()
                .filter(|bytes| general_purpose::URL_SAFE_NO_PAD.encode(bytes) == value)
                .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                .and_then(|bytes| Fr::from_le_bytes(&bytes))
                .ok_or(DigitalPassportIssuerTrustAnchorError::InvalidPublicKey)
        };
        let x_coordinate = decode(x)?;
        let y_coordinate = decode(y)?;
        let point =
            std::panic::catch_unwind(|| EmbeddedGroupAffine::new(x_coordinate, y_coordinate))
                .ok()
                .flatten()
                .filter(|point| !point.is_identity())
                .ok_or(DigitalPassportIssuerTrustAnchorError::InvalidPublicKey)?;
        if expected_jwk_sha256.len() != 64
            || !expected_jwk_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(DigitalPassportIssuerTrustAnchorError::PublicKeyDigestMismatch);
        }
        let canonical_jwk = serde_json::json!({"crv":"Jubjub","kty":"EC","x":x,"y":y});
        let canonical_jwk = serde_json::to_vec(&canonical_jwk)
            .map_err(|_| DigitalPassportIssuerTrustAnchorError::InvalidPublicKey)?;
        if hex::encode(Sha256::digest(&canonical_jwk)) != expected_jwk_sha256 {
            return Err(DigitalPassportIssuerTrustAnchorError::PublicKeyDigestMismatch);
        }
        Ok(Self {
            issuer_did: issuer_did.to_owned(),
            method_id,
            public_key: point_bytes(point)
                .ok_or(DigitalPassportIssuerTrustAnchorError::InvalidPublicKey)?,
            public_key_hash: persistent_hash(&ValueReprAlignedValue(AlignedValue::from(point))),
        })
    }

    #[must_use]
    pub fn issuer_did(&self) -> &str {
        &self.issuer_did
    }

    #[must_use]
    pub const fn method_id(&self) -> [u8; 32] {
        self.method_id
    }

    #[must_use]
    pub const fn public_key_hash(&self) -> [u8; 32] {
        self.public_key_hash
    }

    pub(crate) fn matches(
        &self,
        issuer_did: &str,
        method_id: &[u8; 32],
        public_key: &[u8; 64],
    ) -> bool {
        self.issuer_did == issuer_did
            && &self.method_id == method_id
            && &self.public_key == public_key
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigitalPassportPolicyRequest {
    pub minimum_age_years: u8,
    pub required_issuing_state: Option<[u8; 32]>,
    pub required_document_number: Option<[u8; 32]>,
    pub current_time_seconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DigitalPassportPolicyEvidence {
    pub credential_root: [u8; 32],
    pub current_day: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigitalPassportPolicyError {
    InvalidCredential,
    InvalidPrivateMaterial,
    IssuerNotTrusted,
    Expired,
    InvalidTime,
    AgeRequirementNotMet,
    IssuingStateMismatch,
    DocumentNumberMismatch,
}

#[must_use]
pub fn standalone_digital_passport_issuer_trust_anchor() -> DigitalPassportIssuerTrustAnchor {
    let credential = parse_credential(&standalone_compact_credential())
        .expect("checked-in standalone credential must remain valid");
    let proof = parse_proof(&standalone_compact_proof())
        .expect("checked-in standalone proof must remain valid");
    DigitalPassportIssuerTrustAnchor {
        issuer_did: format!(
            "did:midnight:undeployed:{}",
            hex::encode(credential.issuer.did_contract_address)
        ),
        method_id: credential.issuer.method_id,
        public_key: point_bytes(proof.public_key)
            .expect("checked-in standalone issuer key must be affine"),
        public_key_hash: persistent_hash(&ValueReprAlignedValue(AlignedValue::from(
            proof.public_key,
        ))),
    }
}

pub fn verify_digital_passport_policy(
    signed_credential: &[u8],
    detached_proof: &[u8],
    private_material: &[u8],
    trust_anchor: &DigitalPassportIssuerTrustAnchor,
    request: &DigitalPassportPolicyRequest,
) -> Result<DigitalPassportPolicyEvidence, DigitalPassportPolicyError> {
    if request.minimum_age_years > 120 || request.current_time_seconds == 0 {
        return Err(DigitalPassportPolicyError::InvalidTime);
    }
    let inspection = inspect(signed_credential, Some(detached_proof))
        .map_err(|_| DigitalPassportPolicyError::InvalidCredential)?;
    if inspection.verification.outcome() != VerificationOutcome::Valid {
        return Err(DigitalPassportPolicyError::InvalidCredential);
    }
    let credential = parse_credential(signed_credential)
        .map_err(|_| DigitalPassportPolicyError::InvalidCredential)?;
    let proof =
        parse_proof(detached_proof).map_err(|_| DigitalPassportPolicyError::InvalidCredential)?;
    let issuer_did = format!(
        "did:midnight:undeployed:{}",
        hex::encode(credential.issuer.did_contract_address)
    );
    if issuer_did != trust_anchor.issuer_did
        || credential.issuer.method_id != trust_anchor.method_id
        || proof.signer != credential.issuer
        || point_bytes(proof.public_key).as_ref() != Some(&trust_anchor.public_key)
    {
        return Err(DigitalPassportPolicyError::IssuerNotTrusted);
    }
    if credential.has_expiration && request.current_time_seconds >= credential.expires_at {
        return Err(DigitalPassportPolicyError::Expired);
    }
    let (_, private_parts) = validated_private_parts(signed_credential, private_material)
        .map_err(|_| DigitalPassportPolicyError::InvalidPrivateMaterial)?;
    let current_day = u32::try_from(request.current_time_seconds / 86_400)
        .map_err(|_| DigitalPassportPolicyError::InvalidTime)?;
    if request.minimum_age_years > 0 {
        let required_days = u32::from(request.minimum_age_years)
            .checked_mul(365)
            .ok_or(DigitalPassportPolicyError::InvalidTime)?;
        let age_days = current_day
            .checked_sub(private_parts.values.date_of_birth_days)
            .ok_or(DigitalPassportPolicyError::AgeRequirementNotMet)?;
        if age_days < required_days {
            return Err(DigitalPassportPolicyError::AgeRequirementNotMet);
        }
    }
    if request
        .required_issuing_state
        .is_some_and(|required| required != private_parts.values.issuing_state)
    {
        return Err(DigitalPassportPolicyError::IssuingStateMismatch);
    }
    if request
        .required_document_number
        .is_some_and(|required| required != private_parts.values.document_number)
    {
        return Err(DigitalPassportPolicyError::DocumentNumberMismatch);
    }
    Ok(DigitalPassportPolicyEvidence {
        credential_root: credential_body_root(&credential),
        current_day,
    })
}

fn point_bytes(point: EmbeddedGroupAffine) -> Option<[u8; 64]> {
    let mut bytes = [0_u8; 64];
    bytes[..32].copy_from_slice(&point.x()?.as_le_bytes());
    bytes[32..].copy_from_slice(&point.y()?.as_le_bytes());
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DigitalPassportDisclosureAdapter, standalone_private_material};
    use oxid_credential_application::{
        CredentialDisclosurePort as _, CredentialDisclosurePortError,
    };

    fn request() -> DigitalPassportPolicyRequest {
        let credential = parse_credential(&standalone_compact_credential()).expect("credential");
        DigitalPassportPolicyRequest {
            // The immutable cross-language fixture uses deliberately tiny
            // epoch timestamps. Age policy for freshly issued credentials is
            // covered by the standalone end-to-end flow.
            minimum_age_years: 0,
            required_issuing_state: None,
            required_document_number: None,
            current_time_seconds: credential.issued_at + 1,
        }
    }

    #[test]
    fn portal_trust_anchor_requires_exact_method_point_and_canonical_jwk_digest() {
        let expected = standalone_digital_passport_issuer_trust_anchor();
        let proof = parse_proof(&standalone_compact_proof()).expect("proof");
        let coordinates = point_bytes(proof.public_key).expect("coordinates");
        let x = general_purpose::URL_SAFE_NO_PAD.encode(&coordinates[..32]);
        let y = general_purpose::URL_SAFE_NO_PAD.encode(&coordinates[32..]);
        let jwk = serde_json::json!({"crv":"Jubjub","kty":"EC","x":x,"y":y});
        let digest = hex::encode(Sha256::digest(serde_json::to_vec(&jwk).expect("jwk")));
        let fragment_end = expected
            .method_id
            .iter()
            .rposition(|byte| *byte != 0)
            .map_or(0, |index| index + 1);
        let fragment = std::str::from_utf8(&expected.method_id[..fragment_end]).expect("method");
        let method = format!("{}{fragment}", expected.issuer_did());
        let actual = DigitalPassportIssuerTrustAnchor::from_portal_jubjub(
            expected.issuer_did(),
            &method,
            jwk["x"].as_str().expect("x"),
            jwk["y"].as_str().expect("y"),
            &digest,
        )
        .expect("valid Portal anchor");
        assert_eq!(actual, expected);

        for result in [
            DigitalPassportIssuerTrustAnchor::from_portal_jubjub(
                expected.issuer_did(),
                expected.issuer_did(),
                jwk["x"].as_str().expect("x"),
                jwk["y"].as_str().expect("y"),
                &digest,
            ),
            DigitalPassportIssuerTrustAnchor::from_portal_jubjub(
                expected.issuer_did(),
                &method,
                &format!("{}=", jwk["x"].as_str().expect("x")),
                jwk["y"].as_str().expect("y"),
                &digest,
            ),
            DigitalPassportIssuerTrustAnchor::from_portal_jubjub(
                expected.issuer_did(),
                &method,
                jwk["x"].as_str().expect("x"),
                jwk["y"].as_str().expect("y"),
                &"0".repeat(64),
            ),
        ] {
            assert!(result.is_err());
        }
    }

    #[test]
    fn validates_the_exact_fixture_against_the_pinned_issuer_and_private_claims() {
        let anchor = standalone_digital_passport_issuer_trust_anchor();
        assert_ne!(anchor.method_id(), [0; 32]);
        assert_ne!(anchor.public_key_hash(), [0; 32]);
        let evidence = verify_digital_passport_policy(
            &standalone_compact_credential(),
            &standalone_compact_proof(),
            &standalone_private_material(),
            &anchor,
            &request(),
        )
        .expect("policy");
        assert_ne!(evidence.credential_root, [0; 32]);
        assert_eq!(
            u64::from(evidence.current_day),
            request().current_time_seconds / 86_400
        );
    }

    #[test]
    fn rejects_age_value_expiry_and_private_material_failures() {
        let anchor = standalone_digital_passport_issuer_trust_anchor();
        let mut too_old = request();
        too_old.minimum_age_years = 120;
        assert_eq!(
            verify_digital_passport_policy(
                &standalone_compact_credential(),
                &standalone_compact_proof(),
                &standalone_private_material(),
                &anchor,
                &too_old,
            ),
            Err(DigitalPassportPolicyError::AgeRequirementNotMet)
        );
        let mut wrong_state = request();
        wrong_state.required_issuing_state = Some([0x55; 32]);
        assert_eq!(
            verify_digital_passport_policy(
                &standalone_compact_credential(),
                &standalone_compact_proof(),
                &standalone_private_material(),
                &anchor,
                &wrong_state,
            ),
            Err(DigitalPassportPolicyError::IssuingStateMismatch)
        );
        let credential = parse_credential(&standalone_compact_credential()).expect("credential");
        let mut expired = request();
        expired.current_time_seconds = credential.expires_at;
        assert_eq!(
            verify_digital_passport_policy(
                &standalone_compact_credential(),
                &standalone_compact_proof(),
                &standalone_private_material(),
                &anchor,
                &expired,
            ),
            Err(DigitalPassportPolicyError::Expired)
        );
        assert_eq!(
            verify_digital_passport_policy(
                &standalone_compact_credential(),
                &standalone_compact_proof(),
                b"invalid",
                &anchor,
                &request(),
            ),
            Err(DigitalPassportPolicyError::InvalidPrivateMaterial)
        );
        let disclosure = DigitalPassportDisclosureAdapter;
        assert_eq!(
            disclosure.reveal_local(
                &standalone_compact_credential(),
                &standalone_private_material(),
                crate::CLAIM_DATE_OF_BIRTH,
            ),
            Err(CredentialDisclosurePortError::ClaimNotRevealable)
        );
    }
}
