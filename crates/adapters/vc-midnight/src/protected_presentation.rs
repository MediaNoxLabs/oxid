// SPDX-License-Identifier: Apache-2.0

//! Protected Digital Passport presentation assembly for composition-local
//! contract calls. Credential bytes, selective disclosures, openings, holder
//! signatures, and witnesses remain inside adapters and are serialized only
//! into a bounded, zeroizing child-process request.

use std::{fmt, sync::Arc};

use midnight_base_crypto::fab::AlignedValue;
use midnight_transient_crypto::{curve::EmbeddedGroupAffine, fab::ValueReprAlignedValue};
use oxid_credential_application::{CredentialRepository, CredentialRepositoryError};
use oxid_credential_domain::{
    CredentialDetachedProof, CredentialFormat, CredentialId, CredentialProfileId,
    VerificationOutcome,
};
use oxid_presentation_application::{
    PresentationHolderAuthorizationError, PresentationHolderAuthorizationPort,
    PresentationHolderAuthorizationRequest,
};
use oxid_presentation_domain::PresentationProfileId;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use zeroize::Zeroize;

use crate::{
    CompactHolderProofError, CompactHolderProofPort, CompactHolderProofRequest,
    compact_digital_passport::{
        CompactCredential, CompactProof, VerificationMethodRef, credential_body_root, inspect,
        parse_credential, parse_proof, persistent_hash,
    },
    compact_presentation::{
        DigitalPassportPresentationSelection, PublicDisclosures, consented_claims_hash,
        holder_reference, presentation_root, presentation_statement, verify_presentation_proof,
    },
    digital_passport::{PrivateParts, validated_private_parts},
};

const SECONDS_PER_DAY: u64 = 86_400;
const VERIFIER_DOMAIN: &[u8] = b"oxid:passport-vault:verifier:v1\0";
const MAX_VERIFIER_CHARACTERS: usize = 2_048;

#[derive(Clone, PartialEq, Eq)]
pub struct ProtectedDigitalPassportPresentationRequest {
    pub profile_id: String,
    pub credential_id: String,
    pub verifier: String,
    pub verifier_challenge_hash: [u8; 32],
    pub trusted_issuer_did_contract: [u8; 32],
    pub trusted_issuer_method: [u8; 32],
    pub trusted_issuer_public_key_hash: [u8; 32],
    pub minimum_age_years: u8,
    pub required_issuing_state: Option<[u8; 32]>,
    pub required_document_number: Option<[u8; 32]>,
    pub finalized_time_seconds: u64,
}

impl fmt::Debug for ProtectedDigitalPassportPresentationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProtectedDigitalPassportPresentationRequest")
            .field("minimum_age_years", &self.minimum_age_years)
            .field(
                "requires_issuing_state",
                &self.required_issuing_state.is_some(),
            )
            .field(
                "requires_document_number",
                &self.required_document_number.is_some(),
            )
            .field("finalized_time_seconds", &self.finalized_time_seconds)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtectedDigitalPassportPresentationError {
    InvalidRequest,
    NotFound,
    InvalidCredential,
    IssuerNotTrusted,
    Expired,
    PolicyNotSatisfied,
    HolderNotManaged,
    ProtectionLocked,
    Rejected,
    Unavailable,
}

pub struct ProtectedDigitalPassportPresentationSource {
    repository: Arc<dyn CredentialRepository>,
    holder_authorization: Arc<dyn PresentationHolderAuthorizationPort>,
    holder_proof: Arc<dyn CompactHolderProofPort>,
}

impl ProtectedDigitalPassportPresentationSource {
    #[must_use]
    pub fn new(
        repository: Arc<dyn CredentialRepository>,
        holder_authorization: Arc<dyn PresentationHolderAuthorizationPort>,
        holder_proof: Arc<dyn CompactHolderProofPort>,
    ) -> Self {
        Self {
            repository,
            holder_authorization,
            holder_proof,
        }
    }

    pub async fn prepare(
        &self,
        request: ProtectedDigitalPassportPresentationRequest,
    ) -> Result<PreparedDigitalPassportPresentation, ProtectedDigitalPassportPresentationError>
    {
        validate_request(&request)?;
        let credential_profile = CredentialProfileId::parse(request.profile_id.clone())
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidRequest)?;
        let presentation_profile = PresentationProfileId::parse(request.profile_id.clone())
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidRequest)?;
        let credential_id = CredentialId::parse(request.credential_id.clone())
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidRequest)?;
        let record = self
            .repository
            .get(&credential_profile, &credential_id)
            .map_err(map_repository_error)?;
        if record.profile_id() != &credential_profile
            || record.id() != &credential_id
            || record.metadata().format() != CredentialFormat::MidnightCompactVc
            || record.verification().outcome() != VerificationOutcome::Valid
        {
            return Err(ProtectedDigitalPassportPresentationError::InvalidCredential);
        }
        let issuer_proof_bytes = record
            .detached_proof()
            .map(CredentialDetachedProof::as_bytes)
            .ok_or(ProtectedDigitalPassportPresentationError::InvalidCredential)?;
        let inspection = inspect(record.signed_bytes(), Some(issuer_proof_bytes))
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidCredential)?;
        if inspection.id != credential_id
            || inspection.verification.outcome() != VerificationOutcome::Valid
        {
            return Err(ProtectedDigitalPassportPresentationError::InvalidCredential);
        }
        let credential = parse_credential(record.signed_bytes())
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidCredential)?;
        let issuer_proof = parse_proof(issuer_proof_bytes)
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidCredential)?;
        validate_trusted_issuer(&request, &credential, &issuer_proof)?;
        if credential.has_expiration && request.finalized_time_seconds >= credential.expires_at {
            return Err(ProtectedDigitalPassportPresentationError::Expired);
        }
        let private_material = record
            .private_material()
            .ok_or(ProtectedDigitalPassportPresentationError::InvalidCredential)?;
        let (_, private_parts) =
            validated_private_parts(record.signed_bytes(), private_material.as_bytes())
                .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidCredential)?;
        let current_day = u32::try_from(request.finalized_time_seconds / SECONDS_PER_DAY)
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidRequest)?;
        validate_policy(&request, current_day, &private_parts)?;
        let selection = DigitalPassportPresentationSelection::for_passport_vault(
            request.minimum_age_years,
            request.required_document_number.is_some(),
            request.required_issuing_state.is_some(),
        )
        .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidRequest)?;
        let disclosures = PublicDisclosures::from_private_parts(selection, &private_parts);
        let credential_root = credential_body_root(&credential);
        let presentation_root = presentation_root(&credential, &disclosures);
        let consented_claims_hash = consented_claims_hash(&disclosures);
        let verifier_domain_hash = verifier_domain_hash(&request.verifier);
        let statement = presentation_statement(
            credential_root,
            presentation_root,
            request.verifier_challenge_hash,
            verifier_domain_hash,
            consented_claims_hash,
            current_day,
            request.minimum_age_years,
        );
        let (holder_did, holder_method_id) = holder_reference(&credential)
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidCredential)?;
        self.holder_authorization
            .authorize(PresentationHolderAuthorizationRequest {
                profile_id: presentation_profile.clone(),
                holder_did: holder_did.clone(),
                holder_method_id: holder_method_id.clone(),
                verifier: request.verifier,
                presentation_statement: statement,
            })
            .await
            .map_err(map_holder_authorization_error)?;
        let holder_proof_bytes = self
            .holder_proof
            .create_holder_proof(CompactHolderProofRequest {
                profile_id: presentation_profile,
                holder_did,
                holder_method_id,
                presentation_root,
                verifier_challenge_hash: request.verifier_challenge_hash,
                created_at_seconds: request.finalized_time_seconds,
            })
            .map_err(map_holder_proof_error)?;
        let holder_proof = parse_proof(&holder_proof_bytes)
            .map_err(|_| ProtectedDigitalPassportPresentationError::Rejected)?;
        if holder_proof.signer != credential.holder
            || holder_proof.created_at != request.finalized_time_seconds
            || holder_proof.challenge_hash != request.verifier_challenge_hash
            || !verify_presentation_proof(presentation_root, &holder_proof)
        {
            return Err(ProtectedDigitalPassportPresentationError::Rejected);
        }
        prepared_presentation(
            credential,
            issuer_proof,
            disclosures,
            holder_proof,
            private_parts,
            current_day,
        )
    }
}

fn validate_request(
    request: &ProtectedDigitalPassportPresentationRequest,
) -> Result<(), ProtectedDigitalPassportPresentationError> {
    if request.profile_id.is_empty()
        || request.credential_id.is_empty()
        || request.verifier.is_empty()
        || request.verifier.chars().count() > MAX_VERIFIER_CHARACTERS
        || request.verifier.chars().any(char::is_control)
        || request.verifier_challenge_hash == [0; 32]
        || request.trusted_issuer_did_contract == [0; 32]
        || request.trusted_issuer_method == [0; 32]
        || request.trusted_issuer_public_key_hash == [0; 32]
        || request.minimum_age_years > 120
        || request.finalized_time_seconds == 0
    {
        return Err(ProtectedDigitalPassportPresentationError::InvalidRequest);
    }
    Ok(())
}

fn validate_trusted_issuer(
    request: &ProtectedDigitalPassportPresentationRequest,
    credential: &CompactCredential,
    issuer_proof: &CompactProof,
) -> Result<(), ProtectedDigitalPassportPresentationError> {
    if credential.issuer.did_contract_address != request.trusted_issuer_did_contract
        || credential.issuer.method_id != request.trusted_issuer_method
        || issuer_proof.signer != credential.issuer
        || persistent_point_hash(issuer_proof.public_key) != request.trusted_issuer_public_key_hash
    {
        return Err(ProtectedDigitalPassportPresentationError::IssuerNotTrusted);
    }
    Ok(())
}

fn persistent_point_hash(point: EmbeddedGroupAffine) -> [u8; 32] {
    persistent_hash(&ValueReprAlignedValue(AlignedValue::from(point)))
}

fn validate_policy(
    request: &ProtectedDigitalPassportPresentationRequest,
    current_day: u32,
    private_parts: &PrivateParts,
) -> Result<(), ProtectedDigitalPassportPresentationError> {
    if request.minimum_age_years > 0 {
        let required_days = u32::from(request.minimum_age_years)
            .checked_mul(365)
            .ok_or(ProtectedDigitalPassportPresentationError::InvalidRequest)?;
        let age_days = current_day
            .checked_sub(private_parts.values.date_of_birth_days)
            .ok_or(ProtectedDigitalPassportPresentationError::PolicyNotSatisfied)?;
        if age_days < required_days {
            return Err(ProtectedDigitalPassportPresentationError::PolicyNotSatisfied);
        }
    }
    if request
        .required_issuing_state
        .is_some_and(|required| required != private_parts.values.issuing_state)
        || request
            .required_document_number
            .is_some_and(|required| required != private_parts.values.document_number)
    {
        return Err(ProtectedDigitalPassportPresentationError::PolicyNotSatisfied);
    }
    Ok(())
}

fn verifier_domain_hash(verifier: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(VERIFIER_DOMAIN);
    digest.update(verifier.as_bytes());
    digest.finalize().into()
}

fn map_repository_error(
    error: CredentialRepositoryError,
) -> ProtectedDigitalPassportPresentationError {
    match error {
        CredentialRepositoryError::NotFound => ProtectedDigitalPassportPresentationError::NotFound,
        CredentialRepositoryError::CapacityExceeded | CredentialRepositoryError::Integrity => {
            ProtectedDigitalPassportPresentationError::InvalidCredential
        }
        CredentialRepositoryError::Unavailable => {
            ProtectedDigitalPassportPresentationError::Unavailable
        }
    }
}

const fn map_holder_authorization_error(
    error: PresentationHolderAuthorizationError,
) -> ProtectedDigitalPassportPresentationError {
    match error {
        PresentationHolderAuthorizationError::Unavailable => {
            ProtectedDigitalPassportPresentationError::Unavailable
        }
        PresentationHolderAuthorizationError::InvalidBinding
        | PresentationHolderAuthorizationError::Rejected => {
            ProtectedDigitalPassportPresentationError::Rejected
        }
        PresentationHolderAuthorizationError::NotManaged => {
            ProtectedDigitalPassportPresentationError::HolderNotManaged
        }
        PresentationHolderAuthorizationError::Locked => {
            ProtectedDigitalPassportPresentationError::ProtectionLocked
        }
    }
}

const fn map_holder_proof_error(
    error: CompactHolderProofError,
) -> ProtectedDigitalPassportPresentationError {
    match error {
        CompactHolderProofError::InvalidBinding | CompactHolderProofError::Rejected => {
            ProtectedDigitalPassportPresentationError::Rejected
        }
        CompactHolderProofError::NotManaged => {
            ProtectedDigitalPassportPresentationError::HolderNotManaged
        }
        CompactHolderProofError::Locked => {
            ProtectedDigitalPassportPresentationError::ProtectionLocked
        }
        CompactHolderProofError::Unavailable => {
            ProtectedDigitalPassportPresentationError::Unavailable
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedDigitalPassportPresentation {
    credential: ComposerCredential,
    credential_proof: ComposerProof,
    presentation: ComposerPresentation,
    presentation_proof: ComposerProof,
    current_day: u32,
    witness: ComposerWitness,
}

impl PreparedDigitalPassportPresentation {
    #[must_use]
    pub const fn claim_root(&self) -> [u8; 32] {
        self.credential.claim_root
    }

    #[must_use]
    pub const fn current_day(&self) -> u32 {
        self.current_day
    }
}

impl fmt::Debug for PreparedDigitalPassportPresentation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedDigitalPassportPresentation")
            .field("current_day", &self.current_day)
            .finish_non_exhaustive()
    }
}

impl Drop for PreparedDigitalPassportPresentation {
    fn drop(&mut self) {
        self.credential.zeroize();
        self.credential_proof.zeroize();
        self.presentation.zeroize();
        self.presentation_proof.zeroize();
        self.current_day = 0;
        self.witness.zeroize();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerCredential {
    version: u16,
    package_id: [u8; 32],
    schema_id: [u8; 32],
    major_version: u16,
    minor_version: u16,
    issuer: ComposerMethod,
    holder: ComposerMethod,
    issued_at: String,
    has_expiration: bool,
    expires_at: String,
    first_name_commitment: [u8; 32],
    last_name_commitment: [u8; 32],
    date_of_birth_commitment: [u8; 32],
    document_number_commitment: [u8; 32],
    issuing_state_commitment: [u8; 32],
    claim_root: [u8; 32],
}

impl ComposerCredential {
    fn zeroize(&mut self) {
        self.version = 0;
        self.package_id.zeroize();
        self.schema_id.zeroize();
        self.major_version = 0;
        self.minor_version = 0;
        self.issuer.zeroize();
        self.holder.zeroize();
        self.issued_at.zeroize();
        self.has_expiration = false;
        self.expires_at.zeroize();
        self.first_name_commitment.zeroize();
        self.last_name_commitment.zeroize();
        self.date_of_birth_commitment.zeroize();
        self.document_number_commitment.zeroize();
        self.issuing_state_commitment.zeroize();
        self.claim_root.zeroize();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerMethod {
    did_contract_address: [u8; 32],
    method_id: [u8; 32],
}

impl ComposerMethod {
    fn zeroize(&mut self) {
        self.did_contract_address.zeroize();
        self.method_id.zeroize();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerProof {
    signer: ComposerMethod,
    created_at: String,
    challenge_hash: [u8; 32],
    public_key: ComposerPoint,
    announcement: ComposerPoint,
    response_le: [u8; 32],
}

impl ComposerProof {
    fn zeroize(&mut self) {
        self.signer.zeroize();
        self.created_at.zeroize();
        self.challenge_hash.zeroize();
        self.public_key.zeroize();
        self.announcement.zeroize();
        self.response_le.zeroize();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerPoint {
    x_le: [u8; 32],
    y_le: [u8; 32],
}

impl ComposerPoint {
    fn zeroize(&mut self) {
        self.x_le.zeroize();
        self.y_le.zeroize();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerPresentation {
    version: u16,
    package_id: [u8; 32],
    schema_id: [u8; 32],
    major_version: u16,
    minor_version: u16,
    credential_claim_root: [u8; 32],
    issuer: ComposerMethod,
    holder: ComposerMethod,
    disclosures: ComposerDisclosures,
}

impl ComposerPresentation {
    fn zeroize(&mut self) {
        self.version = 0;
        self.package_id.zeroize();
        self.schema_id.zeroize();
        self.major_version = 0;
        self.minor_version = 0;
        self.credential_claim_root.zeroize();
        self.issuer.zeroize();
        self.holder.zeroize();
        self.disclosures.zeroize();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerDisclosures {
    reveal_first_name: bool,
    #[serde(with = "serde_arrays")]
    first_name_value_padded: [u8; 64],
    first_name_opening: [u8; 32],
    reveal_last_name: bool,
    #[serde(with = "serde_arrays")]
    last_name_value_padded: [u8; 64],
    last_name_opening: [u8; 32],
    prove_age_over_threshold: bool,
    age_threshold_years: u8,
    reveal_document_number: bool,
    document_number_value: [u8; 32],
    document_number_opening: [u8; 32],
    reveal_issuing_state: bool,
    issuing_state_value: [u8; 32],
    issuing_state_opening: [u8; 32],
}

impl ComposerDisclosures {
    fn zeroize(&mut self) {
        self.reveal_first_name = false;
        self.first_name_value_padded.zeroize();
        self.first_name_opening.zeroize();
        self.reveal_last_name = false;
        self.last_name_value_padded.zeroize();
        self.last_name_opening.zeroize();
        self.prove_age_over_threshold = false;
        self.age_threshold_years = 0;
        self.reveal_document_number = false;
        self.document_number_value.zeroize();
        self.document_number_opening.zeroize();
        self.reveal_issuing_state = false;
        self.issuing_state_value.zeroize();
        self.issuing_state_opening.zeroize();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerWitness {
    holder_date_of_birth_days: u32,
    holder_date_of_birth_opening: [u8; 32],
}

impl ComposerWitness {
    fn zeroize(&mut self) {
        self.holder_date_of_birth_days = 0;
        self.holder_date_of_birth_opening.zeroize();
    }
}

fn prepared_presentation(
    credential: CompactCredential,
    issuer_proof: CompactProof,
    disclosures: PublicDisclosures,
    holder_proof: CompactProof,
    mut private_parts: PrivateParts,
    current_day: u32,
) -> Result<PreparedDigitalPassportPresentation, ProtectedDigitalPassportPresentationError> {
    let prepared = PreparedDigitalPassportPresentation {
        credential: ComposerCredential {
            version: credential.version,
            package_id: credential.package_id,
            schema_id: credential.schema_id,
            major_version: credential.major_version,
            minor_version: credential.minor_version,
            issuer: composer_method(credential.issuer),
            holder: composer_method(credential.holder),
            issued_at: credential.issued_at.to_string(),
            has_expiration: credential.has_expiration,
            expires_at: credential.expires_at.to_string(),
            first_name_commitment: credential.commitments.first_name,
            last_name_commitment: credential.commitments.last_name,
            date_of_birth_commitment: credential.commitments.date_of_birth,
            document_number_commitment: credential.commitments.document_number,
            issuing_state_commitment: credential.commitments.issuing_state,
            claim_root: credential.commitments.claim_root,
        },
        credential_proof: composer_proof(issuer_proof)?,
        presentation: ComposerPresentation {
            version: credential.version,
            package_id: credential.package_id,
            schema_id: credential.schema_id,
            major_version: credential.major_version,
            minor_version: credential.minor_version,
            credential_claim_root: credential.commitments.claim_root,
            issuer: composer_method(credential.issuer),
            holder: composer_method(credential.holder),
            disclosures: ComposerDisclosures {
                reveal_first_name: disclosures.reveal_first_name,
                first_name_value_padded: disclosures.first_name,
                first_name_opening: disclosures.first_name_opening,
                reveal_last_name: disclosures.reveal_last_name,
                last_name_value_padded: disclosures.last_name,
                last_name_opening: disclosures.last_name_opening,
                prove_age_over_threshold: disclosures.prove_age,
                age_threshold_years: disclosures.age_threshold_years,
                reveal_document_number: disclosures.reveal_document_number,
                document_number_value: disclosures.document_number,
                document_number_opening: disclosures.document_number_opening,
                reveal_issuing_state: disclosures.reveal_issuing_state,
                issuing_state_value: disclosures.issuing_state,
                issuing_state_opening: disclosures.issuing_state_opening,
            },
        },
        presentation_proof: composer_proof(holder_proof)?,
        current_day,
        witness: ComposerWitness {
            holder_date_of_birth_days: private_parts.values.date_of_birth_days,
            holder_date_of_birth_opening: private_parts.openings.date_of_birth,
        },
    };
    private_parts.zeroize();
    Ok(prepared)
}

const fn composer_method(method: VerificationMethodRef) -> ComposerMethod {
    ComposerMethod {
        did_contract_address: method.did_contract_address,
        method_id: method.method_id,
    }
}

fn composer_proof(
    proof: CompactProof,
) -> Result<ComposerProof, ProtectedDigitalPassportPresentationError> {
    Ok(ComposerProof {
        signer: composer_method(proof.signer),
        created_at: proof.created_at.to_string(),
        challenge_hash: proof.challenge_hash,
        public_key: composer_point(proof.public_key)?,
        announcement: composer_point(proof.announcement)?,
        response_le: proof
            .response
            .as_le_bytes()
            .try_into()
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidCredential)?,
    })
}

fn composer_point(
    point: EmbeddedGroupAffine,
) -> Result<ComposerPoint, ProtectedDigitalPassportPresentationError> {
    if point.is_identity() {
        return Err(ProtectedDigitalPassportPresentationError::InvalidCredential);
    }
    Ok(ComposerPoint {
        x_le: point
            .x()
            .ok_or(ProtectedDigitalPassportPresentationError::InvalidCredential)?
            .as_le_bytes()
            .try_into()
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidCredential)?,
        y_le: point
            .y()
            .ok_or(ProtectedDigitalPassportPresentationError::InvalidCredential)?
            .as_le_bytes()
            .try_into()
            .map_err(|_| ProtectedDigitalPassportPresentationError::InvalidCredential)?,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Poll, Waker},
    };

    use midnight_transient_crypto::curve::{EmbeddedFr, Fr};
    use oxid_credential_domain::{CredentialPrivateMaterial, CredentialRecord};
    use oxid_presentation_application::AuthorizePresentationHolderFuture;

    use super::*;
    use crate::{
        compact_digital_passport::encode_proof, compact_presentation::presentation_proof_challenge,
        standalone_compact_credential, standalone_compact_proof, standalone_private_material,
    };

    struct Repository(CredentialRecord);

    impl CredentialRepository for Repository {
        fn upsert(&self, _: CredentialRecord) -> Result<(), CredentialRepositoryError> {
            unreachable!("test repository is read-only")
        }

        fn list(
            &self,
            _: &CredentialProfileId,
        ) -> Result<Vec<CredentialRecord>, CredentialRepositoryError> {
            Ok(vec![self.0.clone()])
        }

        fn get(
            &self,
            profile_id: &CredentialProfileId,
            credential_id: &CredentialId,
        ) -> Result<CredentialRecord, CredentialRepositoryError> {
            if self.0.profile_id() == profile_id && self.0.id() == credential_id {
                Ok(self.0.clone())
            } else {
                Err(CredentialRepositoryError::NotFound)
            }
        }

        fn remove(
            &self,
            _: &CredentialProfileId,
            _: &CredentialId,
        ) -> Result<(), CredentialRepositoryError> {
            unreachable!("test repository is read-only")
        }
    }

    struct Authorization {
        calls: AtomicUsize,
        error: Option<PresentationHolderAuthorizationError>,
    }

    impl Authorization {
        fn ready() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                error: None,
            }
        }

        fn failing(error: PresentationHolderAuthorizationError) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                error: Some(error),
            }
        }
    }

    impl PresentationHolderAuthorizationPort for Authorization {
        fn authorize<'a>(
            &'a self,
            _: PresentationHolderAuthorizationRequest,
        ) -> AuthorizePresentationHolderFuture<'a> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let result = self.error.map_or(Ok(()), Err);
            Box::pin(async move { result })
        }
    }

    #[derive(Clone, Copy)]
    enum HolderProofBehavior {
        Valid,
        Tampered,
        Error(CompactHolderProofError),
    }

    struct HolderProof {
        calls: AtomicUsize,
        behavior: HolderProofBehavior,
    }

    impl HolderProof {
        fn new(behavior: HolderProofBehavior) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                behavior,
            }
        }
    }

    impl CompactHolderProofPort for HolderProof {
        fn create_holder_proof(
            &self,
            request: CompactHolderProofRequest,
        ) -> Result<Vec<u8>, CompactHolderProofError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if let HolderProofBehavior::Error(error) = self.behavior {
                return Err(error);
            }
            let credential = parse_credential(&standalone_compact_credential())
                .map_err(|_| CompactHolderProofError::Rejected)?;
            let secret = EmbeddedFr::from(987_654_321_u64);
            // A test-only scalar that deliberately differs from the prototype's
            // forbidden fixed nonce 17. Production obtains this nonce through
            // the managed holder custody port.
            let nonce = EmbeddedFr::from(23_u64);
            let mut proof = CompactProof {
                signer: credential.holder,
                created_at: request.created_at_seconds,
                challenge_hash: request.verifier_challenge_hash,
                public_key: EmbeddedGroupAffine::generator() * secret,
                announcement: EmbeddedGroupAffine::generator() * nonce,
                response: Fr::from(0_u64),
            };
            let challenge = EmbeddedFr::try_from(presentation_proof_challenge(
                request.presentation_root,
                &proof,
            ))
            .map_err(|_| CompactHolderProofError::Rejected)?;
            proof.response = Fr::from_le_bytes(&(nonce + challenge * secret).as_le_bytes())
                .ok_or(CompactHolderProofError::Rejected)?;
            let mut encoded =
                encode_proof(&proof).map_err(|_| CompactHolderProofError::Rejected)?;
            if matches!(self.behavior, HolderProofBehavior::Tampered) {
                *encoded.last_mut().expect("proof encoding") ^= 1;
            }
            Ok(encoded)
        }
    }

    fn fixture_record() -> (String, CredentialRecord) {
        let credential = standalone_compact_credential();
        let proof = standalone_compact_proof();
        let inspected = inspect(&credential, Some(&proof)).expect("fixture inspection");
        let credential_id = inspected.id.as_str().to_owned();
        let record = CredentialRecord::new_with_proof_and_private_material(
            CredentialProfileId::parse("profile_one").expect("profile"),
            inspected.id,
            credential,
            Some(CredentialDetachedProof::new(proof).expect("proof")),
            Some(
                CredentialPrivateMaterial::new(standalone_private_material())
                    .expect("private material"),
            ),
            inspected.metadata,
            inspected.verification,
        )
        .expect("record");
        (credential_id, record)
    }

    fn request(credential_id: String) -> ProtectedDigitalPassportPresentationRequest {
        let credential =
            parse_credential(&standalone_compact_credential()).expect("fixture credential");
        let proof = parse_proof(&standalone_compact_proof()).expect("fixture proof");
        ProtectedDigitalPassportPresentationRequest {
            profile_id: "profile_one".to_owned(),
            credential_id,
            verifier: "passport-vault:standalone".to_owned(),
            verifier_challenge_hash: [0x11; 32],
            trusted_issuer_did_contract: credential.issuer.did_contract_address,
            trusted_issuer_method: credential.issuer.method_id,
            trusted_issuer_public_key_hash: persistent_point_hash(proof.public_key),
            minimum_age_years: 0,
            required_issuing_state: None,
            required_document_number: None,
            finalized_time_seconds: credential.issued_at + 1,
        }
    }

    fn source(
        record: CredentialRecord,
        authorization: Arc<Authorization>,
        holder_proof: Arc<HolderProof>,
    ) -> ProtectedDigitalPassportPresentationSource {
        ProtectedDigitalPassportPresentationSource::new(
            Arc::new(Repository(record)),
            authorization,
            holder_proof,
        )
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                return output;
            }
        }
    }

    fn assert_error(
        result: Result<
            PreparedDigitalPassportPresentation,
            ProtectedDigitalPassportPresentationError,
        >,
        expected: ProtectedDigitalPassportPresentationError,
    ) {
        match result {
            Err(actual) => assert_eq!(actual, expected),
            Ok(_) => panic!("expected protected presentation preparation to fail"),
        }
    }

    #[test]
    fn prepares_exact_zeroizing_composer_material_after_custody_checks() {
        let (credential_id, record) = fixture_record();
        let authorization = Arc::new(Authorization::ready());
        let holder_proof = Arc::new(HolderProof::new(HolderProofBehavior::Valid));
        let request = request(credential_id.clone());
        let request_debug = format!("{request:?}");
        assert!(!request_debug.contains(&credential_id));
        assert!(!request_debug.contains("profile_one"));
        assert!(!request_debug.contains("passport-vault:standalone"));

        let expected_day =
            u32::try_from(request.finalized_time_seconds / SECONDS_PER_DAY).expect("fixture day");
        let prepared =
            block_on(source(record, authorization.clone(), holder_proof.clone()).prepare(request))
                .expect("protected presentation");
        let credential =
            parse_credential(&standalone_compact_credential()).expect("fixture credential");
        assert_eq!(
            hex::encode(persistent_point_hash(
                parse_proof(&standalone_compact_proof())
                    .expect("fixture proof")
                    .public_key
            )),
            "15a29c1d6912bf5128edf647eeca75a687f8465bfea009d6e7618a8520c03ed9"
        );
        assert_eq!(prepared.claim_root(), credential.commitments.claim_root);
        assert_eq!(prepared.current_day(), expected_day);
        assert_eq!(authorization.calls.load(Ordering::Relaxed), 1);
        assert_eq!(holder_proof.calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            format!("{prepared:?}"),
            format!("PreparedDigitalPassportPresentation {{ current_day: {expected_day}, .. }}")
        );

        let json = serde_json::to_value(&prepared).expect("composer JSON");
        assert_eq!(
            json["credential"]["issuedAt"],
            credential.issued_at.to_string()
        );
        assert_eq!(json["currentDay"], expected_day);
        assert_eq!(
            json["presentation"]["disclosures"]["firstNameValuePadded"]
                .as_array()
                .expect("first-name array"),
            &vec![serde_json::Value::from(0); 64]
        );
        assert_eq!(
            json["presentation"]["disclosures"]["lastNameValuePadded"]
                .as_array()
                .expect("last-name array"),
            &vec![serde_json::Value::from(0); 64]
        );
        assert!(!json.to_string().contains("Alice"));
        assert!(!json.to_string().contains("Example"));
    }

    #[test]
    fn validates_exact_age_state_and_document_policy() {
        let (_, private_parts) = validated_private_parts(
            &standalone_compact_credential(),
            &standalone_private_material(),
        )
        .expect("fixture private parts");
        let mut request = request("vc_fixture".to_owned());
        request.minimum_age_years = 18;
        request.required_issuing_state = Some(private_parts.values.issuing_state);
        request.required_document_number = Some(private_parts.values.document_number);
        assert_eq!(validate_policy(&request, 20_000, &private_parts), Ok(()));

        request.minimum_age_years = 120;
        assert_eq!(
            validate_policy(&request, 20_000, &private_parts),
            Err(ProtectedDigitalPassportPresentationError::PolicyNotSatisfied)
        );
    }

    #[test]
    fn rejects_trust_expiry_and_policy_before_custody_is_used() {
        let authorization = Arc::new(Authorization::ready());
        let holder_proof = Arc::new(HolderProof::new(HolderProofBehavior::Valid));

        let (credential_id, record) = fixture_record();
        let mut wrong_trust = request(credential_id);
        wrong_trust.trusted_issuer_public_key_hash[0] ^= 1;
        assert_error(
            block_on(
                source(record, authorization.clone(), holder_proof.clone()).prepare(wrong_trust),
            ),
            ProtectedDigitalPassportPresentationError::IssuerNotTrusted,
        );

        let (credential_id, record) = fixture_record();
        let credential =
            parse_credential(&standalone_compact_credential()).expect("fixture credential");
        assert!(credential.has_expiration);
        let mut expired = request(credential_id);
        expired.finalized_time_seconds = credential.expires_at;
        assert_error(
            block_on(source(record, authorization.clone(), holder_proof.clone()).prepare(expired)),
            ProtectedDigitalPassportPresentationError::Expired,
        );

        let (credential_id, record) = fixture_record();
        let mut wrong_state = request(credential_id);
        wrong_state.required_issuing_state = Some([0xff; 32]);
        assert_error(
            block_on(
                source(record, authorization.clone(), holder_proof.clone()).prepare(wrong_state),
            ),
            ProtectedDigitalPassportPresentationError::PolicyNotSatisfied,
        );
        assert_eq!(authorization.calls.load(Ordering::Relaxed), 0);
        assert_eq!(holder_proof.calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn fails_closed_for_locked_custody_and_tampered_holder_proofs() {
        let (credential_id, record) = fixture_record();
        let locked = Arc::new(Authorization::failing(
            PresentationHolderAuthorizationError::Locked,
        ));
        let unused_proof = Arc::new(HolderProof::new(HolderProofBehavior::Valid));
        assert_error(
            block_on(
                source(record, locked.clone(), unused_proof.clone())
                    .prepare(request(credential_id)),
            ),
            ProtectedDigitalPassportPresentationError::ProtectionLocked,
        );
        assert_eq!(locked.calls.load(Ordering::Relaxed), 1);
        assert_eq!(unused_proof.calls.load(Ordering::Relaxed), 0);

        let (credential_id, record) = fixture_record();
        let authorization = Arc::new(Authorization::ready());
        let tampered = Arc::new(HolderProof::new(HolderProofBehavior::Tampered));
        assert_error(
            block_on(
                source(record, authorization.clone(), tampered.clone())
                    .prepare(request(credential_id)),
            ),
            ProtectedDigitalPassportPresentationError::Rejected,
        );
        assert_eq!(authorization.calls.load(Ordering::Relaxed), 1);
        assert_eq!(tampered.calls.load(Ordering::Relaxed), 1);

        let (credential_id, record) = fixture_record();
        let unavailable = Arc::new(HolderProof::new(HolderProofBehavior::Error(
            CompactHolderProofError::Unavailable,
        )));
        assert_error(
            block_on(
                source(record, Arc::new(Authorization::ready()), unavailable)
                    .prepare(request(credential_id)),
            ),
            ProtectedDigitalPassportPresentationError::Unavailable,
        );
    }
}
