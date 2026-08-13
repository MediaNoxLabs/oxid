// SPDX-License-Identifier: Apache-2.0

//! Exact Digital Passport policy validation used by product-specific local
//! verifier adapters. This module owns schema interpretation; vault state and
//! money movement remain outside the reusable credential adapter.

use midnight_transient_crypto::curve::EmbeddedGroupAffine;
use oxid_credential_domain::VerificationOutcome;

use crate::{
    compact_digital_passport::{credential_body_root, inspect, parse_credential, parse_proof},
    digital_passport::validated_private_parts,
    standalone_compact_credential, standalone_compact_proof,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigitalPassportIssuerTrustAnchor {
    issuer_did: String,
    method_id: [u8; 32],
    public_key: [u8; 64],
}

impl DigitalPassportIssuerTrustAnchor {
    #[must_use]
    pub fn issuer_did(&self) -> &str {
        &self.issuer_did
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
    fn validates_the_exact_fixture_against_the_pinned_issuer_and_private_claims() {
        let evidence = verify_digital_passport_policy(
            &standalone_compact_credential(),
            &standalone_compact_proof(),
            &standalone_private_material(),
            &standalone_digital_passport_issuer_trust_anchor(),
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
