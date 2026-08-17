// SPDX-License-Identifier: Apache-2.0

//! Exact public-input boundary for the Oxid Digital Passport presentation
//! circuit.
//!
//! This adapter prepares and independently reconstructs the public statement,
//! then constructs the credential family's exact protected-holder Schnorr
//! `Proof`. Protected values and openings are used only to build the
//! presentation preimage and never enter the portable `MPS1` public input. The
//! native standalone composition can connect the reviewed prover runtime and
//! independent verifier explicitly; other compositions remain fail-closed.

use std::{collections::BTreeSet, fmt, sync::Arc};

use base64::{Engine as _, engine::general_purpose};
use midnight_serialize::{Deserializable as _, Serializable as _};
use midnight_transient_crypto::{
    curve::{EmbeddedFr, EmbeddedGroupAffine, Fr},
    hash::transient_hash,
};
use oxid_credential_application::{CredentialRepository, CredentialRepositoryError};
use oxid_credential_domain::{
    CredentialDetachedProof, CredentialFormat, CredentialId, CredentialProfileId,
    VerificationOutcome,
};
use oxid_identity_application::{
    DidJubjubChallengeSigningPort, DidLifecyclePortError, DidOperationConfirmation,
    DidOperationError, DidRecordQuery, DidRecordRepositoryError, GetDidRecordUseCase,
    SignDidPayloadCommand, SignDidPayloadUseCase,
};
use oxid_identity_domain::{IdentityProfileId, MidnightDid};
use oxid_platform_ports::ClockPort;
use oxid_presentation_application::{
    AuthorizePresentationHolderFuture, CreatePresentationProofFuture,
    PresentationHolderAuthorizationError, PresentationHolderAuthorizationPort,
    PresentationHolderAuthorizationRequest, PresentationProofArtifact, PresentationProofError,
    PresentationProofPort, PresentationProofRequest, PresentationVerificationError,
    PresentationVerificationRequest, PresentationVerifierPort, VerifyPresentationProofFuture,
};
use oxid_presentation_domain::{PresentationClaimIntent, RequestedPresentationClaim};
use sha2::{Digest as _, Sha256};

use crate::compact_digital_passport::{
    CompactCredential, CompactProof, VerificationMethodRef, credential_body_root, encode_proof,
    inspect, parse_credential, parse_proof, persistent_hash, transient_hash_value,
};
use crate::compact_proving::{presentation_preimage, presentation_public_transcript};
#[cfg(not(target_arch = "wasm32"))]
use crate::compact_runtime::{
    NativeCompactPresentationRuntime, PortableCompactPresentation, decode_portable_presentation,
    encode_portable_presentation, public_binding,
};
use crate::digital_passport::{
    CLAIM_DATE_OF_BIRTH, CLAIM_DOCUMENT_NUMBER, CLAIM_FIRST_NAME, CLAIM_ISSUING_STATE,
    CLAIM_LAST_NAME, PrivateParts, commit, document_number_null_commitment,
    validated_private_parts,
};

const PUBLIC_INPUT_MAGIC: &[u8; 4] = b"MPS1";
const PUBLIC_INPUT_VERSION: u16 = 1;
const PUBLIC_INPUT_BYTES: usize = 524;
const MILLISECONDS_PER_DAY: u64 = 86_400_000;
const PRESENTATION_STATEMENT_DOMAIN: &[u8] = b"oxid:midnight-compact-vp:v1";
const PRESENTATION_PROOF_CONTEXT: &[u8] = b"midnight:vc:presentation";
const CONSENTED_CLAIMS_DOMAIN: &[u8] = b"oxid:compact-vp:claims:v1";
const HOLDER_AUTHORIZATION_DOMAIN: &[u8] = b"oxid:midnight-compact-holder-authorization:v1\0";
const MAX_HOLDER_REFERENCE_CHARACTERS: usize = 512;
const MAX_VERIFIER_CHARACTERS: usize = 2_048;
const PRESENTATION_FRESHNESS_SECONDS: u64 = 300;

const FLAG_FIRST_NAME: u8 = 1 << 0;
const FLAG_LAST_NAME: u8 = 1 << 1;
const FLAG_AGE: u8 = 1 << 2;
const FLAG_DOCUMENT_NUMBER: u8 = 1 << 3;
const FLAG_ISSUING_STATE: u8 = 1 << 4;
const KNOWN_FLAGS: u8 =
    FLAG_FIRST_NAME | FLAG_LAST_NAME | FLAG_AGE | FLAG_DOCUMENT_NUMBER | FLAG_ISSUING_STATE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactPresentationError {
    InvalidCredential,
    InvalidSelection,
    InvalidTime,
    InvalidEncoding,
    StatementMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactHolderProofError {
    InvalidBinding,
    NotManaged,
    Locked,
    Rejected,
    Unavailable,
}

/// Exact public transcript needed to construct the credential-family holder
/// proof. The body root and verifier challenge are public protocol values; no
/// claim value, opening, key reference, scalar, or nonce is included.
#[derive(Clone, PartialEq, Eq)]
pub struct CompactHolderProofRequest {
    pub profile_id: oxid_presentation_domain::PresentationProfileId,
    pub holder_did: String,
    pub holder_method_id: String,
    pub presentation_root: [u8; 32],
    pub verifier_challenge_hash: [u8; 32],
    pub created_at_seconds: u64,
}

impl fmt::Debug for CompactHolderProofRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompactHolderProofRequest")
            .field("holder_did", &self.holder_did)
            .field("holder_method_id", &self.holder_method_id)
            .field("created_at_seconds", &self.created_at_seconds)
            .finish_non_exhaustive()
    }
}

/// Exact Compact holder-proof operation used only inside the Midnight VC
/// adapter/composition boundary.
pub trait CompactHolderProofPort: Send + Sync {
    fn create_holder_proof(
        &self,
        request: CompactHolderProofRequest,
    ) -> Result<Vec<u8>, CompactHolderProofError>;
}

#[derive(Clone, Copy, Debug, Default)]
struct UnavailableCompactHolderProof;

impl CompactHolderProofPort for UnavailableCompactHolderProof {
    fn create_holder_proof(
        &self,
        _: CompactHolderProofRequest,
    ) -> Result<Vec<u8>, CompactHolderProofError> {
        Err(CompactHolderProofError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DigitalPassportPresentationSelection {
    reveal_first_name: bool,
    reveal_last_name: bool,
    prove_age: bool,
    age_threshold_years: u8,
    reveal_document_number: bool,
    reveal_issuing_state: bool,
}

impl DigitalPassportPresentationSelection {
    pub fn from_requested_claims(
        claims: &[RequestedPresentationClaim],
    ) -> Result<Self, CompactPresentationError> {
        let mut paths = BTreeSet::new();
        let mut result = Self {
            reveal_first_name: false,
            reveal_last_name: false,
            prove_age: false,
            age_threshold_years: 0,
            reveal_document_number: false,
            reveal_issuing_state: false,
        };
        for claim in claims {
            if !paths.insert(claim.path()) {
                return Err(CompactPresentationError::InvalidSelection);
            }
            match (claim.path(), claim.intent()) {
                (CLAIM_FIRST_NAME, PresentationClaimIntent::Reveal) => {
                    result.reveal_first_name = true;
                }
                (CLAIM_LAST_NAME, PresentationClaimIntent::Reveal) => {
                    result.reveal_last_name = true;
                }
                (CLAIM_DOCUMENT_NUMBER, PresentationClaimIntent::Reveal) => {
                    result.reveal_document_number = true;
                }
                (CLAIM_ISSUING_STATE, PresentationClaimIntent::Reveal) => {
                    result.reveal_issuing_state = true;
                }
                (CLAIM_DATE_OF_BIRTH, PresentationClaimIntent::Predicate)
                    if claim.predicate_kind() == Some("age_over")
                        && matches!(claim.threshold(), Some(1..=120)) =>
                {
                    result.prove_age = true;
                    result.age_threshold_years = claim.threshold().unwrap_or_default();
                }
                _ => return Err(CompactPresentationError::InvalidSelection),
            }
        }
        // The current reviewed final circuit always checks the private age
        // witness. A request without that predicate cannot use this circuit.
        if !result.prove_age {
            return Err(CompactPresentationError::InvalidSelection);
        }
        Ok(result)
    }

    fn flags(self) -> u8 {
        let mut flags = 0;
        if self.reveal_first_name {
            flags |= FLAG_FIRST_NAME;
        }
        if self.reveal_last_name {
            flags |= FLAG_LAST_NAME;
        }
        if self.prove_age {
            flags |= FLAG_AGE;
        }
        if self.reveal_document_number {
            flags |= FLAG_DOCUMENT_NUMBER;
        }
        if self.reveal_issuing_state {
            flags |= FLAG_ISSUING_STATE;
        }
        flags
    }

    fn from_flags(flags: u8, age_threshold_years: u8) -> Result<Self, CompactPresentationError> {
        if flags & !KNOWN_FLAGS != 0
            || flags & FLAG_AGE == 0
            || !(1..=120).contains(&age_threshold_years)
        {
            return Err(CompactPresentationError::InvalidEncoding);
        }
        Ok(Self {
            reveal_first_name: flags & FLAG_FIRST_NAME != 0,
            reveal_last_name: flags & FLAG_LAST_NAME != 0,
            prove_age: true,
            age_threshold_years,
            reveal_document_number: flags & FLAG_DOCUMENT_NUMBER != 0,
            reveal_issuing_state: flags & FLAG_ISSUING_STATE != 0,
        })
    }

    pub(crate) const fn for_passport_vault(
        minimum_age_years: u8,
        reveal_document_number: bool,
        reveal_issuing_state: bool,
    ) -> Result<Self, CompactPresentationError> {
        if minimum_age_years > 120 {
            return Err(CompactPresentationError::InvalidSelection);
        }
        Ok(Self {
            reveal_first_name: false,
            reveal_last_name: false,
            prove_age: minimum_age_years > 0,
            age_threshold_years: minimum_age_years,
            reveal_document_number,
            reveal_issuing_state,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct PublicDisclosures {
    pub(crate) reveal_first_name: bool,
    pub(crate) first_name: [u8; 64],
    pub(crate) first_name_opening: [u8; 32],
    pub(crate) reveal_last_name: bool,
    pub(crate) last_name: [u8; 64],
    pub(crate) last_name_opening: [u8; 32],
    pub(crate) prove_age: bool,
    pub(crate) age_threshold_years: u8,
    pub(crate) reveal_document_number: bool,
    pub(crate) document_number: [u8; 32],
    pub(crate) document_number_opening: [u8; 32],
    pub(crate) reveal_issuing_state: bool,
    pub(crate) issuing_state: [u8; 32],
    pub(crate) issuing_state_opening: [u8; 32],
}

impl PublicDisclosures {
    pub(crate) fn from_private_parts(
        selection: DigitalPassportPresentationSelection,
        private_parts: &PrivateParts,
    ) -> Self {
        Self {
            reveal_first_name: selection.reveal_first_name,
            first_name: if selection.reveal_first_name {
                private_parts.values.first_name
            } else {
                [0; 64]
            },
            first_name_opening: if selection.reveal_first_name {
                private_parts.openings.first_name
            } else {
                [0; 32]
            },
            reveal_last_name: selection.reveal_last_name,
            last_name: if selection.reveal_last_name {
                private_parts.values.last_name
            } else {
                [0; 64]
            },
            last_name_opening: if selection.reveal_last_name {
                private_parts.openings.last_name
            } else {
                [0; 32]
            },
            prove_age: selection.prove_age,
            age_threshold_years: selection.age_threshold_years,
            reveal_document_number: selection.reveal_document_number,
            document_number: if selection.reveal_document_number {
                private_parts.values.document_number
            } else {
                [0; 32]
            },
            document_number_opening: if selection.reveal_document_number {
                private_parts.openings.document_number
            } else {
                [0; 32]
            },
            reveal_issuing_state: selection.reveal_issuing_state,
            issuing_state: if selection.reveal_issuing_state {
                private_parts.values.issuing_state
            } else {
                [0; 32]
            },
            issuing_state_opening: if selection.reveal_issuing_state {
                private_parts.openings.issuing_state
            } else {
                [0; 32]
            },
        }
    }

    fn selection(self) -> DigitalPassportPresentationSelection {
        DigitalPassportPresentationSelection {
            reveal_first_name: self.reveal_first_name,
            reveal_last_name: self.reveal_last_name,
            prove_age: self.prove_age,
            age_threshold_years: self.age_threshold_years,
            reveal_document_number: self.reveal_document_number,
            reveal_issuing_state: self.reveal_issuing_state,
        }
    }

    fn canonical(self) -> bool {
        (!self.reveal_first_name
            && self.first_name == [0; 64]
            && self.first_name_opening == [0; 32]
            || self.reveal_first_name)
            && (!self.reveal_last_name
                && self.last_name == [0; 64]
                && self.last_name_opening == [0; 32]
                || self.reveal_last_name)
            && (!self.reveal_document_number
                && self.document_number == [0; 32]
                && self.document_number_opening == [0; 32]
                || self.reveal_document_number)
            && (!self.reveal_issuing_state
                && self.issuing_state == [0; 32]
                && self.issuing_state_opening == [0; 32]
                || self.reveal_issuing_state)
            && self.prove_age
            && (1..=120).contains(&self.age_threshold_years)
    }
}

/// Portable public inputs for the reviewed `proveDigitalPassportPresentation`
/// circuit. Revealed values/openings are public by selection; the private
/// date-of-birth value/opening is intentionally absent.
#[derive(Clone, PartialEq, Eq)]
pub struct CompactPresentationPublicInput {
    pub(crate) credential_root: [u8; 32],
    pub(crate) presentation_root: [u8; 32],
    pub(crate) verifier_challenge_hash: [u8; 32],
    pub(crate) verifier_domain_hash: [u8; 32],
    pub(crate) consented_claims_hash: [u8; 32],
    pub(crate) current_day: u32,
    pub(crate) disclosures: PublicDisclosures,
    pub(crate) statement: [u8; 32],
}

impl fmt::Debug for CompactPresentationPublicInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompactPresentationPublicInput")
            .field("current_day", &self.current_day)
            .field("selection", &self.disclosures.selection())
            .finish_non_exhaustive()
    }
}

impl CompactPresentationPublicInput {
    #[must_use]
    pub const fn statement(&self) -> [u8; 32] {
        self.statement
    }

    #[must_use]
    pub const fn credential_root(&self) -> [u8; 32] {
        self.credential_root
    }

    #[must_use]
    pub const fn presentation_root(&self) -> [u8; 32] {
        self.presentation_root
    }

    #[must_use]
    pub const fn consented_claims_hash(&self) -> [u8; 32] {
        self.consented_claims_hash
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(PUBLIC_INPUT_BYTES);
        output.extend_from_slice(PUBLIC_INPUT_MAGIC);
        output.extend_from_slice(&PUBLIC_INPUT_VERSION.to_be_bytes());
        output.extend_from_slice(&self.credential_root);
        output.extend_from_slice(&self.presentation_root);
        output.extend_from_slice(&self.verifier_challenge_hash);
        output.extend_from_slice(&self.verifier_domain_hash);
        output.extend_from_slice(&self.consented_claims_hash);
        output.extend_from_slice(&self.current_day.to_be_bytes());
        output.push(self.disclosures.age_threshold_years);
        output.push(self.disclosures.selection().flags());
        output.extend_from_slice(&self.disclosures.first_name);
        output.extend_from_slice(&self.disclosures.first_name_opening);
        output.extend_from_slice(&self.disclosures.last_name);
        output.extend_from_slice(&self.disclosures.last_name_opening);
        output.extend_from_slice(&self.disclosures.document_number);
        output.extend_from_slice(&self.disclosures.document_number_opening);
        output.extend_from_slice(&self.disclosures.issuing_state);
        output.extend_from_slice(&self.disclosures.issuing_state_opening);
        output.extend_from_slice(&self.statement);
        debug_assert_eq!(output.len(), PUBLIC_INPUT_BYTES);
        output
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CompactPresentationError> {
        if bytes.len() != PUBLIC_INPUT_BYTES || &bytes[..4] != PUBLIC_INPUT_MAGIC {
            return Err(CompactPresentationError::InvalidEncoding);
        }
        let version = u16::from_be_bytes(
            bytes[4..6]
                .try_into()
                .map_err(|_| CompactPresentationError::InvalidEncoding)?,
        );
        if version != PUBLIC_INPUT_VERSION {
            return Err(CompactPresentationError::InvalidEncoding);
        }
        let mut offset = 6;
        let credential_root = take::<32>(bytes, &mut offset)?;
        let presentation_root = take::<32>(bytes, &mut offset)?;
        let verifier_challenge_hash = take::<32>(bytes, &mut offset)?;
        let verifier_domain_hash = take::<32>(bytes, &mut offset)?;
        let consented_claims_hash = take::<32>(bytes, &mut offset)?;
        let current_day = u32::from_be_bytes(take::<4>(bytes, &mut offset)?);
        let age_threshold_years = take::<1>(bytes, &mut offset)?[0];
        let flags = take::<1>(bytes, &mut offset)?[0];
        let selection =
            DigitalPassportPresentationSelection::from_flags(flags, age_threshold_years)?;
        let disclosures = PublicDisclosures {
            reveal_first_name: selection.reveal_first_name,
            first_name: take::<64>(bytes, &mut offset)?,
            first_name_opening: take::<32>(bytes, &mut offset)?,
            reveal_last_name: selection.reveal_last_name,
            last_name: take::<64>(bytes, &mut offset)?,
            last_name_opening: take::<32>(bytes, &mut offset)?,
            prove_age: selection.prove_age,
            age_threshold_years,
            reveal_document_number: selection.reveal_document_number,
            document_number: take::<32>(bytes, &mut offset)?,
            document_number_opening: take::<32>(bytes, &mut offset)?,
            reveal_issuing_state: selection.reveal_issuing_state,
            issuing_state: take::<32>(bytes, &mut offset)?,
            issuing_state_opening: take::<32>(bytes, &mut offset)?,
        };
        let statement = take::<32>(bytes, &mut offset)?;
        if offset != bytes.len()
            || current_day == 0
            || verifier_challenge_hash == [0; 32]
            || verifier_domain_hash == [0; 32]
            || !disclosures.canonical()
        {
            return Err(CompactPresentationError::InvalidEncoding);
        }
        Ok(Self {
            credential_root,
            presentation_root,
            verifier_challenge_hash,
            verifier_domain_hash,
            consented_claims_hash,
            current_day,
            disclosures,
            statement,
        })
    }

    pub fn verify_against(
        &self,
        credential_bytes: &[u8],
        verifier_challenge_hash: [u8; 32],
        verifier_domain_hash: [u8; 32],
        current_day: u32,
        selection: DigitalPassportPresentationSelection,
    ) -> Result<(), CompactPresentationError> {
        let credential = parse_credential(credential_bytes)
            .map_err(|_| CompactPresentationError::InvalidCredential)?;
        if self.credential_root != credential_body_root(&credential)
            || self.verifier_challenge_hash != verifier_challenge_hash
            || self.verifier_domain_hash != verifier_domain_hash
            || self.current_day != current_day
            || self.disclosures.selection() != selection
            || !self.disclosures.canonical()
        {
            return Err(CompactPresentationError::StatementMismatch);
        }
        validate_revealed_commitments(&credential, &self.disclosures)?;
        let presentation_root = presentation_root(&credential, &self.disclosures);
        let consented_claims_hash = consented_claims_hash(&self.disclosures);
        let statement = presentation_statement(
            self.credential_root,
            presentation_root,
            verifier_challenge_hash,
            verifier_domain_hash,
            consented_claims_hash,
            current_day,
            selection.age_threshold_years,
        );
        if self.presentation_root != presentation_root
            || self.consented_claims_hash != consented_claims_hash
            || self.statement != statement
        {
            return Err(CompactPresentationError::StatementMismatch);
        }
        Ok(())
    }
}

pub fn prepare_public_input(
    credential_bytes: &[u8],
    private_material: &[u8],
    verifier_challenge_hash: [u8; 32],
    verifier_domain_hash: [u8; 32],
    current_day: u32,
    requested_claims: &[RequestedPresentationClaim],
) -> Result<CompactPresentationPublicInput, CompactPresentationError> {
    if verifier_challenge_hash == [0; 32] || verifier_domain_hash == [0; 32] || current_day == 0 {
        return Err(CompactPresentationError::InvalidSelection);
    }
    let credential = parse_credential(credential_bytes)
        .map_err(|_| CompactPresentationError::InvalidCredential)?;
    let (_, private_parts) = validated_private_parts(credential_bytes, private_material)
        .map_err(|_| CompactPresentationError::InvalidCredential)?;
    let selection = DigitalPassportPresentationSelection::from_requested_claims(requested_claims)?;
    let minimum_days = u32::from(selection.age_threshold_years)
        .checked_mul(365)
        .ok_or(CompactPresentationError::InvalidTime)?;
    if current_day < private_parts.values.date_of_birth_days
        || current_day - private_parts.values.date_of_birth_days < minimum_days
    {
        return Err(CompactPresentationError::InvalidTime);
    }
    if selection.reveal_document_number
        && credential.commitments.document_number == document_number_null_commitment()
    {
        return Err(CompactPresentationError::InvalidSelection);
    }
    let disclosures = PublicDisclosures::from_private_parts(selection, &private_parts);
    let credential_root = credential_body_root(&credential);
    let presentation_root = presentation_root(&credential, &disclosures);
    let consented_claims_hash = consented_claims_hash(&disclosures);
    let statement = presentation_statement(
        credential_root,
        presentation_root,
        verifier_challenge_hash,
        verifier_domain_hash,
        consented_claims_hash,
        current_day,
        selection.age_threshold_years,
    );
    Ok(CompactPresentationPublicInput {
        credential_root,
        presentation_root,
        verifier_challenge_hash,
        verifier_domain_hash,
        consented_claims_hash,
        current_day,
        disclosures,
        statement,
    })
}

fn validate_revealed_commitments(
    credential: &CompactCredential,
    disclosures: &PublicDisclosures,
) -> Result<(), CompactPresentationError> {
    if disclosures.reveal_first_name
        && commit(disclosures.first_name, disclosures.first_name_opening)
            != credential.commitments.first_name
        || disclosures.reveal_last_name
            && commit(disclosures.last_name, disclosures.last_name_opening)
                != credential.commitments.last_name
        || disclosures.reveal_document_number
            && (credential.commitments.document_number == document_number_null_commitment()
                || commit(
                    disclosures.document_number,
                    disclosures.document_number_opening,
                ) != credential.commitments.document_number)
        || disclosures.reveal_issuing_state
            && commit(disclosures.issuing_state, disclosures.issuing_state_opening)
                != credential.commitments.issuing_state
    {
        return Err(CompactPresentationError::StatementMismatch);
    }
    Ok(())
}

pub(crate) fn presentation_root(
    credential: &CompactCredential,
    disclosures: &PublicDisclosures,
) -> [u8; 32] {
    persistent_hash(&(
        1_u16,
        (
            credential.package_id,
            credential.schema_id,
            credential.major_version,
            credential.minor_version,
        ),
        credential.commitments.claim_root,
        (
            credential.issuer.did_contract_address,
            credential.issuer.method_id,
        ),
        (
            credential.holder.did_contract_address,
            credential.holder.method_id,
        ),
        (
            disclosures.reveal_first_name,
            disclosures.first_name,
            disclosures.first_name_opening,
            disclosures.reveal_last_name,
            disclosures.last_name,
            disclosures.last_name_opening,
            disclosures.prove_age,
            disclosures.age_threshold_years,
            disclosures.reveal_document_number,
            disclosures.document_number,
            disclosures.document_number_opening,
            (
                disclosures.reveal_issuing_state,
                disclosures.issuing_state,
                disclosures.issuing_state_opening,
            ),
        ),
    ))
}

pub(crate) fn consented_claims_hash(disclosures: &PublicDisclosures) -> [u8; 32] {
    persistent_hash(&(
        padded::<32>(CONSENTED_CLAIMS_DOMAIN),
        field_bytes(u64::from(disclosures.reveal_first_name)),
        field_bytes(u64::from(disclosures.reveal_last_name)),
        field_bytes(u64::from(disclosures.prove_age)),
        field_bytes(u64::from(disclosures.age_threshold_years)),
        field_bytes(u64::from(disclosures.reveal_document_number)),
        field_bytes(u64::from(disclosures.reveal_issuing_state)),
    ))
}

pub(crate) fn presentation_statement(
    credential_root: [u8; 32],
    presentation_root: [u8; 32],
    verifier_challenge_hash: [u8; 32],
    verifier_domain_hash: [u8; 32],
    consented_claims_hash: [u8; 32],
    current_day: u32,
    age_threshold_years: u8,
) -> [u8; 32] {
    persistent_hash(&(
        padded::<32>(PRESENTATION_STATEMENT_DOMAIN),
        credential_root,
        presentation_root,
        verifier_challenge_hash,
        verifier_domain_hash,
        consented_claims_hash,
        field_bytes(u64::from(current_day)),
        field_bytes(u64::from(age_threshold_years)),
    ))
}

fn field_bytes(value: u64) -> [u8; 32] {
    let mut output = [0; 32];
    output[..8].copy_from_slice(&value.to_le_bytes());
    output
}

fn take<const N: usize>(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<[u8; N], CompactPresentationError> {
    let end = offset
        .checked_add(N)
        .filter(|end| *end <= bytes.len())
        .ok_or(CompactPresentationError::InvalidEncoding)?;
    let value = bytes[*offset..end]
        .try_into()
        .map_err(|_| CompactPresentationError::InvalidEncoding)?;
    *offset = end;
    Ok(value)
}

const fn padded<const N: usize>(value: &[u8]) -> [u8; N] {
    let mut output = [0; N];
    let mut index = 0;
    while index < value.len() && index < N {
        output[index] = value[index];
        index += 1;
    }
    output
}

/// Presentation-time bridge between an exact Compact holder reference and the
/// currently managed protected Jubjub DID method.
///
/// The returned success is an authorization precondition only. The temporary
/// generic DID signature is independently checked and discarded; it is never
/// returned as a credential-family `Proof` or included in a `vp_token`.
pub struct ManagedDidJubjubHolderAuthorization {
    get_did: Arc<dyn GetDidRecordUseCase>,
    sign_did: Arc<dyn SignDidPayloadUseCase>,
    challenge_signing: Option<Arc<dyn DidJubjubChallengeSigningPort>>,
}

impl ManagedDidJubjubHolderAuthorization {
    #[must_use]
    pub fn new(
        get_did: Arc<dyn GetDidRecordUseCase>,
        sign_did: Arc<dyn SignDidPayloadUseCase>,
    ) -> Self {
        Self {
            get_did,
            sign_did,
            challenge_signing: None,
        }
    }

    #[must_use]
    pub fn with_challenge_signing(
        get_did: Arc<dyn GetDidRecordUseCase>,
        sign_did: Arc<dyn SignDidPayloadUseCase>,
        challenge_signing: Arc<dyn DidJubjubChallengeSigningPort>,
    ) -> Self {
        Self {
            get_did,
            sign_did,
            challenge_signing: Some(challenge_signing),
        }
    }
}

impl PresentationHolderAuthorizationPort for ManagedDidJubjubHolderAuthorization {
    fn authorize<'a>(
        &'a self,
        request: PresentationHolderAuthorizationRequest,
    ) -> AuthorizePresentationHolderFuture<'a> {
        Box::pin(async move {
            validate_holder_authorization_request(&request)?;
            let record = self
                .get_did
                .execute(DidRecordQuery {
                    profile_id: request.profile_id.as_str().to_owned(),
                    did: request.holder_did.clone(),
                })
                .map_err(map_did_lookup_error)?;
            if record.document.id != request.holder_did
                || record.document_metadata.deactivated == Some(true)
                || !record
                    .managed_method_ids
                    .iter()
                    .any(|method| method == &request.holder_method_id)
            {
                return Err(PresentationHolderAuthorizationError::NotManaged);
            }
            let method = record
                .document
                .verification_methods
                .iter()
                .find(|method| method.id == request.holder_method_id)
                .ok_or(PresentationHolderAuthorizationError::InvalidBinding)?;
            if method.controller != request.holder_did
                || method.public_key_jwk.key_type != "EC"
                || method.public_key_jwk.curve != "Jubjub"
                || !record.document.relationships.iter().any(|relationship| {
                    relationship.relationship == "assertionMethod"
                        && relationship
                            .method_ids
                            .iter()
                            .any(|method| method == &request.holder_method_id)
                })
            {
                return Err(PresentationHolderAuthorizationError::InvalidBinding);
            }
            let public_key = jubjub_public_key(
                &method.public_key_jwk.x,
                method
                    .public_key_jwk
                    .y
                    .as_deref()
                    .ok_or(PresentationHolderAuthorizationError::InvalidBinding)?,
            )?;
            let payload = holder_authorization_payload(&request);
            let signature = self
                .sign_did
                .execute(SignDidPayloadCommand {
                    profile_id: request.profile_id.as_str().to_owned(),
                    did: request.holder_did,
                    method_id: request.holder_method_id.clone(),
                    payload: payload.to_vec(),
                    confirmation: DidOperationConfirmation {
                        title: "Authorize credential presentation".to_owned(),
                        summary: "Authorize the current protected holder method for the consented credential presentation.".to_owned(),
                        confirmed: true,
                    },
                })
                .map_err(map_did_signing_error)?;
            if signature.method_id != request.holder_method_id
                || signature.algorithm != "jubjub"
                || verify_did_jubjub_signature(&public_key, &payload, &signature.signature_bytes)
                    .is_err()
            {
                return Err(PresentationHolderAuthorizationError::Rejected);
            }
            Ok(())
        })
    }
}

impl CompactHolderProofPort for ManagedDidJubjubHolderAuthorization {
    fn create_holder_proof(
        &self,
        request: CompactHolderProofRequest,
    ) -> Result<Vec<u8>, CompactHolderProofError> {
        if request.presentation_root == [0; 32]
            || request.verifier_challenge_hash == [0; 32]
            || request.created_at_seconds == 0
        {
            return Err(CompactHolderProofError::InvalidBinding);
        }
        let profile_id = IdentityProfileId::parse(request.profile_id.as_str().to_owned())
            .map_err(|_| CompactHolderProofError::InvalidBinding)?;
        let did = MidnightDid::parse(request.holder_did.clone())
            .map_err(|_| CompactHolderProofError::InvalidBinding)?;
        let record = self
            .get_did
            .execute(DidRecordQuery {
                profile_id: profile_id.as_str().to_owned(),
                did: did.as_str().to_owned(),
            })
            .map_err(map_holder_proof_lookup_error)?;
        if record.document.id != request.holder_did
            || record.document_metadata.deactivated == Some(true)
            || !record
                .managed_method_ids
                .iter()
                .any(|method| method == &request.holder_method_id)
        {
            return Err(CompactHolderProofError::NotManaged);
        }
        let method = record
            .document
            .verification_methods
            .iter()
            .find(|method| method.id == request.holder_method_id)
            .ok_or(CompactHolderProofError::InvalidBinding)?;
        if method.controller != request.holder_did
            || method.public_key_jwk.key_type != "EC"
            || method.public_key_jwk.curve != "Jubjub"
            || !record.document.relationships.iter().any(|relationship| {
                relationship.relationship == "assertionMethod"
                    && relationship
                        .method_ids
                        .iter()
                        .any(|candidate| candidate == &request.holder_method_id)
            })
        {
            return Err(CompactHolderProofError::InvalidBinding);
        }
        let expected_public_key = jubjub_public_key(
            &method.public_key_jwk.x,
            method
                .public_key_jwk
                .y
                .as_deref()
                .ok_or(CompactHolderProofError::InvalidBinding)?,
        )
        .map_err(|_| CompactHolderProofError::InvalidBinding)?;
        let expected_public_key_bytes = {
            let mut bytes = Vec::with_capacity(32);
            expected_public_key
                .serialize(&mut bytes)
                .map_err(|_| CompactHolderProofError::InvalidBinding)?;
            bytes
                .try_into()
                .map_err(|_| CompactHolderProofError::InvalidBinding)?
        };
        let signer = compact_holder_reference(&request.holder_did, &request.holder_method_id)?;
        let challenge_signing = self
            .challenge_signing
            .as_ref()
            .ok_or(CompactHolderProofError::Unavailable)?;
        let mut unsigned = None;
        let mut derive = |public_key: &[u8; 32], announcement: &[u8; 32]| {
            let public_key = compressed_jubjub_point(public_key)
                .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
            let announcement = compressed_jubjub_point(announcement)
                .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
            if public_key != expected_public_key || announcement.is_identity() {
                return Err(DidLifecyclePortError::InvalidOperation);
            }
            let proof = CompactProof {
                signer,
                created_at: request.created_at_seconds,
                challenge_hash: request.verifier_challenge_hash,
                public_key,
                announcement,
                response: Fr::from(0_u64),
            };
            let challenge = presentation_proof_challenge(request.presentation_root, &proof);
            let challenge_bytes = challenge
                .as_le_bytes()
                .try_into()
                .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
            unsigned = Some(proof);
            Ok(challenge_bytes)
        };
        let signature = challenge_signing
            .sign_jubjub_challenge(
                &profile_id,
                &did,
                &request.holder_method_id,
                &expected_public_key_bytes,
                &mut derive,
            )
            .map_err(map_holder_challenge_error)?;
        let mut proof = unsigned.ok_or(CompactHolderProofError::Rejected)?;
        if signature.method_id != request.holder_method_id
            || compressed_jubjub_point(&signature.public_key)
                .map_err(|_| CompactHolderProofError::Rejected)?
                != proof.public_key
            || compressed_jubjub_point(&signature.announcement)
                .map_err(|_| CompactHolderProofError::Rejected)?
                != proof.announcement
        {
            return Err(CompactHolderProofError::Rejected);
        }
        proof.response =
            Fr::from_le_bytes(&signature.response).ok_or(CompactHolderProofError::Rejected)?;
        if !verify_presentation_proof(request.presentation_root, &proof) {
            return Err(CompactHolderProofError::Rejected);
        }
        encode_proof(&proof).map_err(|_| CompactHolderProofError::Rejected)
    }
}

fn compact_holder_reference(
    did: &str,
    method_id: &str,
) -> Result<VerificationMethodRef, CompactHolderProofError> {
    let address = did
        .strip_prefix("did:midnight:undeployed:")
        .filter(|value| value.len() == 64)
        .and_then(|value| hex::decode(value).ok())
        .and_then(|value| value.try_into().ok())
        .ok_or(CompactHolderProofError::InvalidBinding)?;
    let fragment = method_id
        .strip_prefix(did)
        .filter(|value| value.starts_with('#') && value.len() <= 32)
        .ok_or(CompactHolderProofError::InvalidBinding)?;
    if !fragment.bytes().all(|byte| {
        byte == b'#'
            || byte.is_ascii_alphanumeric()
            || matches!(byte, b'.' | b'-' | b'_' | b':' | b'%')
    }) {
        return Err(CompactHolderProofError::InvalidBinding);
    }
    Ok(VerificationMethodRef {
        did_contract_address: address,
        method_id: padded::<32>(fragment.as_bytes()),
    })
}

fn compressed_jubjub_point(
    bytes: &[u8; 32],
) -> Result<EmbeddedGroupAffine, CompactHolderProofError> {
    let mut input = bytes.as_slice();
    let point = EmbeddedGroupAffine::deserialize(&mut input, 0)
        .map_err(|_| CompactHolderProofError::Rejected)?;
    if !input.is_empty() || point.is_identity() {
        return Err(CompactHolderProofError::Rejected);
    }
    Ok(point)
}

pub(crate) fn presentation_proof_challenge(body_root: [u8; 32], proof: &CompactProof) -> Fr {
    let payload_root = persistent_hash(&(
        body_root,
        padded::<32>(PRESENTATION_PROOF_CONTEXT),
        persistent_hash(&(proof.signer.did_contract_address, proof.signer.method_id)),
        midnight_transient_crypto::hash::upgrade_from_transient(transient_hash_value(
            proof.created_at,
        ))
        .0,
        proof.challenge_hash,
    ));
    midnight_transient_crypto::hash::degrade_to_transient(midnight_base_crypto::hash::HashOutput(
        persistent_hash(&(
            payload_root,
            midnight_transient_crypto::hash::upgrade_from_transient(transient_hash_value(
                proof.public_key,
            ))
            .0,
            midnight_transient_crypto::hash::upgrade_from_transient(transient_hash_value(
                proof.announcement,
            ))
            .0,
        )),
    ))
}

pub(crate) fn verify_presentation_proof(body_root: [u8; 32], proof: &CompactProof) -> bool {
    EmbeddedGroupAffine::generator() * proof.response
        == proof.announcement + proof.public_key * presentation_proof_challenge(body_root, proof)
}

fn map_holder_proof_lookup_error(error: DidOperationError) -> CompactHolderProofError {
    match error {
        DidOperationError::InvalidProfileIdentifier(_)
        | DidOperationError::InvalidDid(_)
        | DidOperationError::SubjectMismatch => CompactHolderProofError::InvalidBinding,
        DidOperationError::Persistence(DidRecordRepositoryError::NotFound) => {
            CompactHolderProofError::NotManaged
        }
        _ => CompactHolderProofError::Unavailable,
    }
}

fn map_holder_challenge_error(error: DidLifecyclePortError) -> CompactHolderProofError {
    match error {
        DidLifecyclePortError::NotManaged
        | DidLifecyclePortError::NotFound
        | DidLifecyclePortError::Deactivated => CompactHolderProofError::NotManaged,
        DidLifecyclePortError::Locked => CompactHolderProofError::Locked,
        DidLifecyclePortError::UnsupportedAlgorithm | DidLifecyclePortError::InvalidOperation => {
            CompactHolderProofError::Rejected
        }
        DidLifecyclePortError::Unavailable | DidLifecyclePortError::ProtectionUnavailable => {
            CompactHolderProofError::Unavailable
        }
        DidLifecyclePortError::UnsupportedNetwork | DidLifecyclePortError::Conflict => {
            CompactHolderProofError::Rejected
        }
    }
}

fn validate_holder_authorization_request(
    request: &PresentationHolderAuthorizationRequest,
) -> Result<(), PresentationHolderAuthorizationError> {
    if request.holder_did.is_empty()
        || request.holder_method_id.is_empty()
        || request.holder_did.chars().count() > MAX_HOLDER_REFERENCE_CHARACTERS
        || request.holder_method_id.chars().count() > MAX_HOLDER_REFERENCE_CHARACTERS
        || request.verifier.is_empty()
        || request.verifier.chars().count() > MAX_VERIFIER_CHARACTERS
        || request.presentation_statement == [0; 32]
        || !request
            .holder_method_id
            .starts_with(&format!("{}#", request.holder_did))
    {
        return Err(PresentationHolderAuthorizationError::InvalidBinding);
    }
    Ok(())
}

fn holder_authorization_payload(request: &PresentationHolderAuthorizationRequest) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(HOLDER_AUTHORIZATION_DOMAIN);
    digest.update(request.holder_did.as_bytes());
    digest.update([0]);
    digest.update(request.holder_method_id.as_bytes());
    digest.update([0]);
    digest.update(request.verifier.as_bytes());
    digest.update([0]);
    digest.update(request.presentation_statement);
    digest.finalize().into()
}

fn map_did_lookup_error(error: DidOperationError) -> PresentationHolderAuthorizationError {
    match error {
        DidOperationError::InvalidProfileIdentifier(_)
        | DidOperationError::InvalidDid(_)
        | DidOperationError::SubjectMismatch => {
            PresentationHolderAuthorizationError::InvalidBinding
        }
        DidOperationError::Persistence(DidRecordRepositoryError::NotFound) => {
            PresentationHolderAuthorizationError::NotManaged
        }
        _ => PresentationHolderAuthorizationError::Unavailable,
    }
}

fn map_did_signing_error(error: DidOperationError) -> PresentationHolderAuthorizationError {
    match error {
        DidOperationError::Lifecycle(DidLifecyclePortError::NotManaged)
        | DidOperationError::Lifecycle(DidLifecyclePortError::NotFound)
        | DidOperationError::Lifecycle(DidLifecyclePortError::Deactivated)
        | DidOperationError::Persistence(DidRecordRepositoryError::NotFound) => {
            PresentationHolderAuthorizationError::NotManaged
        }
        DidOperationError::Lifecycle(DidLifecyclePortError::Locked) => {
            PresentationHolderAuthorizationError::Locked
        }
        DidOperationError::InvalidProfileIdentifier(_)
        | DidOperationError::InvalidDid(_)
        | DidOperationError::EmptyPayload
        | DidOperationError::PayloadTooLarge
        | DidOperationError::ConfirmationRequired
        | DidOperationError::InvalidConfirmation
        | DidOperationError::SubjectMismatch => PresentationHolderAuthorizationError::Rejected,
        _ => PresentationHolderAuthorizationError::Unavailable,
    }
}

fn jubjub_public_key(
    x: &str,
    y: &str,
) -> Result<EmbeddedGroupAffine, PresentationHolderAuthorizationError> {
    let decode = |value: &str| {
        general_purpose::URL_SAFE_NO_PAD
            .decode(value)
            .ok()
            .filter(|bytes| general_purpose::URL_SAFE_NO_PAD.encode(bytes) == value)
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            .and_then(|bytes| Fr::from_le_bytes(&bytes))
            .ok_or(PresentationHolderAuthorizationError::InvalidBinding)
    };
    let point = EmbeddedGroupAffine::new(decode(x)?, decode(y)?)
        .ok_or(PresentationHolderAuthorizationError::InvalidBinding)?;
    (!point.is_identity())
        .then_some(point)
        .ok_or(PresentationHolderAuthorizationError::InvalidBinding)
}

fn verify_did_jubjub_signature(
    public_key: &EmbeddedGroupAffine,
    payload: &[u8],
    signature: &[u8],
) -> Result<(), PresentationHolderAuthorizationError> {
    if signature.len() != 96 {
        return Err(PresentationHolderAuthorizationError::Rejected);
    }
    let announcement = EmbeddedGroupAffine::new(
        outer_field_from_be(&signature[..32])?,
        outer_field_from_be(&signature[32..64])?,
    )
    .filter(|point| !point.is_identity())
    .ok_or(PresentationHolderAuthorizationError::Rejected)?;
    let response = embedded_field_from_be(&signature[64..])?;
    let hash = Sha256::digest(payload);
    let payload_fields: [Fr; 4] = std::array::from_fn(|index| {
        let start = index * 8;
        Fr::from(u64::from_be_bytes(
            hash[start..start + 8]
                .try_into()
                .expect("SHA-256 has four complete u64 limbs"),
        ))
    });
    let challenge_fields = [
        announcement
            .x()
            .ok_or(PresentationHolderAuthorizationError::Rejected)?,
        announcement
            .y()
            .ok_or(PresentationHolderAuthorizationError::Rejected)?,
        public_key
            .x()
            .ok_or(PresentationHolderAuthorizationError::Rejected)?,
        public_key
            .y()
            .ok_or(PresentationHolderAuthorizationError::Rejected)?,
        payload_fields[0],
        payload_fields[1],
        payload_fields[2],
        payload_fields[3],
    ];
    let challenge_bytes = transient_hash(&challenge_fields).as_le_bytes();
    let mut reduced = [0_u8; 32];
    reduced[..31].copy_from_slice(&challenge_bytes[..31]);
    let challenge = EmbeddedFr::from_le_bytes(&reduced)
        .ok_or(PresentationHolderAuthorizationError::Rejected)?;
    (EmbeddedGroupAffine::generator() * response == announcement + *public_key * challenge)
        .then_some(())
        .ok_or(PresentationHolderAuthorizationError::Rejected)
}

fn outer_field_from_be(bytes: &[u8]) -> Result<Fr, PresentationHolderAuthorizationError> {
    let little_endian: [u8; 32] = std::array::from_fn(|index| bytes[31 - index]);
    Fr::from_le_bytes(&little_endian).ok_or(PresentationHolderAuthorizationError::Rejected)
}

fn embedded_field_from_be(
    bytes: &[u8],
) -> Result<EmbeddedFr, PresentationHolderAuthorizationError> {
    let little_endian: [u8; 32] = std::array::from_fn(|index| bytes[31 - index]);
    EmbeddedFr::from_le_bytes(&little_endian).ok_or(PresentationHolderAuthorizationError::Rejected)
}

/// Standalone proof port that performs exact proof-preimage validation and
/// protected credential-family holder-proof construction, then fails closed
/// before ZK proof construction. This makes the headless/mobile consent flow
/// exercise the real credential, opening, statement, time, and holder custody
/// boundaries without manufacturing a ZK proof or `vp_token`.
pub struct PreflightOnlyCompactPresentationProof {
    repository: Arc<dyn CredentialRepository>,
    clock: Arc<dyn ClockPort>,
    holder_authorization: Arc<dyn PresentationHolderAuthorizationPort>,
    holder_proof: Arc<dyn CompactHolderProofPort>,
    #[cfg(not(target_arch = "wasm32"))]
    runtime: Option<Arc<NativeCompactPresentationRuntime>>,
}

impl PreflightOnlyCompactPresentationProof {
    #[must_use]
    pub fn new(
        repository: Arc<dyn CredentialRepository>,
        clock: Arc<dyn ClockPort>,
        holder_authorization: Arc<dyn PresentationHolderAuthorizationPort>,
    ) -> Self {
        Self {
            repository,
            clock,
            holder_authorization,
            holder_proof: Arc::new(UnavailableCompactHolderProof),
            #[cfg(not(target_arch = "wasm32"))]
            runtime: None,
        }
    }

    #[must_use]
    pub fn with_holder_proof(
        repository: Arc<dyn CredentialRepository>,
        clock: Arc<dyn ClockPort>,
        holder_authorization: Arc<dyn PresentationHolderAuthorizationPort>,
        holder_proof: Arc<dyn CompactHolderProofPort>,
    ) -> Self {
        Self {
            repository,
            clock,
            holder_authorization,
            holder_proof,
            #[cfg(not(target_arch = "wasm32"))]
            runtime: None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn with_runtime(
        repository: Arc<dyn CredentialRepository>,
        clock: Arc<dyn ClockPort>,
        holder_authorization: Arc<dyn PresentationHolderAuthorizationPort>,
        holder_proof: Arc<dyn CompactHolderProofPort>,
        runtime: Arc<NativeCompactPresentationRuntime>,
    ) -> Self {
        Self {
            repository,
            clock,
            holder_authorization,
            holder_proof,
            runtime: Some(runtime),
        }
    }
}

impl PresentationProofPort for PreflightOnlyCompactPresentationProof {
    fn create<'a>(
        &'a self,
        request: PresentationProofRequest,
    ) -> CreatePresentationProofFuture<'a> {
        Box::pin(async move {
            let profile_id = CredentialProfileId::parse(request.profile_id.as_str().to_owned())
                .map_err(|_| PresentationProofError::InvalidCredential)?;
            let credential_id = CredentialId::parse(request.credential_id)
                .map_err(|_| PresentationProofError::InvalidCredential)?;
            let record = self
                .repository
                .get(&profile_id, &credential_id)
                .map_err(map_repository_error)?;
            if record.metadata().format() != CredentialFormat::MidnightCompactVc
                || record.verification().outcome() != VerificationOutcome::Valid
            {
                return Err(PresentationProofError::InvalidCredential);
            }
            let issuer_proof_bytes = record
                .detached_proof()
                .map(CredentialDetachedProof::as_bytes)
                .ok_or(PresentationProofError::InvalidCredential)?;
            let inspection = inspect(record.signed_bytes(), Some(issuer_proof_bytes))
                .map_err(|_| PresentationProofError::InvalidCredential)?;
            if record.profile_id() != &profile_id
                || inspection.id != *record.id()
                || inspection.verification.outcome() != VerificationOutcome::Valid
            {
                return Err(PresentationProofError::InvalidCredential);
            }
            let private_material = record
                .private_material()
                .ok_or(PresentationProofError::InvalidCredential)?;
            let now = self
                .clock
                .now()
                .map_err(|_| PresentationProofError::Unavailable)?
                .value();
            let current_day = u32::try_from(now / MILLISECONDS_PER_DAY)
                .map_err(|_| PresentationProofError::Rejected)?;
            let public_input = prepare_public_input(
                record.signed_bytes(),
                private_material.as_bytes(),
                request.challenge_hash,
                request.verifier_domain_hash,
                current_day,
                &request.requested_claims,
            )
            .map_err(map_preflight_error)?;
            let decoded = CompactPresentationPublicInput::decode(&public_input.encode())
                .map_err(map_preflight_error)?;
            decoded
                .verify_against(
                    record.signed_bytes(),
                    request.challenge_hash,
                    request.verifier_domain_hash,
                    current_day,
                    DigitalPassportPresentationSelection::from_requested_claims(
                        &request.requested_claims,
                    )
                    .map_err(map_preflight_error)?,
                )
                .map_err(map_preflight_error)?;
            let credential = parse_credential(record.signed_bytes())
                .map_err(|_| PresentationProofError::InvalidCredential)?;
            let (holder_did, holder_method_id) = holder_reference(&credential)
                .map_err(|_| PresentationProofError::InvalidCredential)?;
            self.holder_authorization
                .authorize(PresentationHolderAuthorizationRequest {
                    profile_id: request.profile_id.clone(),
                    holder_did: holder_did.clone(),
                    holder_method_id: holder_method_id.clone(),
                    verifier: request.verifier.clone(),
                    presentation_statement: decoded.statement(),
                })
                .await
                .map_err(map_holder_authorization_error)?;
            let holder_proof_bytes = self
                .holder_proof
                .create_holder_proof(CompactHolderProofRequest {
                    profile_id: request.profile_id.clone(),
                    holder_did,
                    holder_method_id,
                    presentation_root: decoded.presentation_root(),
                    verifier_challenge_hash: request.challenge_hash,
                    created_at_seconds: now / 1_000,
                })
                .map_err(map_compact_holder_proof_error)?;
            let holder_proof =
                parse_proof(&holder_proof_bytes).map_err(|_| PresentationProofError::Rejected)?;
            if holder_proof.signer != credential.holder
                || holder_proof.created_at != now / 1_000
                || holder_proof.challenge_hash != request.challenge_hash
                || !verify_presentation_proof(decoded.presentation_root(), &holder_proof)
            {
                return Err(PresentationProofError::Rejected);
            }
            let credential_proof = parse_proof(issuer_proof_bytes)
                .map_err(|_| PresentationProofError::InvalidCredential)?;
            let (_, private_parts) =
                validated_private_parts(record.signed_bytes(), private_material.as_bytes())
                    .map_err(|_| PresentationProofError::InvalidCredential)?;
            let preimage = presentation_preimage(
                &credential,
                &credential_proof,
                &decoded,
                &holder_proof,
                &private_parts,
            );
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(runtime) = self.runtime.as_ref() {
                let proof = runtime
                    .prove(&preimage)
                    .await
                    .map_err(map_runtime_proving_error)?;
                let (_, communications_commitment) =
                    public_binding(&preimage).map_err(map_runtime_proving_error)?;
                let portable = PortableCompactPresentation {
                    artifact_identity: runtime.identity(),
                    credential: record.signed_bytes().to_vec(),
                    issuer_proof: issuer_proof_bytes.to_vec(),
                    public_input: decoded.encode(),
                    holder_proof: holder_proof_bytes,
                    communications_commitment,
                    proof,
                };
                let encoded =
                    encode_portable_presentation(&portable).map_err(map_runtime_proving_error)?;
                return PresentationProofArtifact::new(encoded);
            }
            // Preflight-only composition drops the exact generated preimage
            // and fails closed before creating a token.
            drop(preimage);
            Err(PresentationProofError::Unavailable)
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct NativeCompactPresentationVerifier {
    runtime: Arc<NativeCompactPresentationRuntime>,
    clock: Arc<dyn ClockPort>,
    get_did: Arc<dyn GetDidRecordUseCase>,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeCompactPresentationVerifier {
    #[must_use]
    pub fn new(
        runtime: Arc<NativeCompactPresentationRuntime>,
        clock: Arc<dyn ClockPort>,
        get_did: Arc<dyn GetDidRecordUseCase>,
    ) -> Self {
        Self {
            runtime,
            clock,
            get_did,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PresentationVerifierPort for NativeCompactPresentationVerifier {
    fn verify<'a>(
        &'a self,
        request: PresentationVerificationRequest,
    ) -> VerifyPresentationProofFuture<'a> {
        Box::pin(async move {
            let portable = decode_portable_presentation(request.proof.as_bytes())
                .map_err(|_| PresentationVerificationError::InvalidProof)?;
            if portable.artifact_identity != self.runtime.identity() {
                return Err(PresentationVerificationError::InvalidProof);
            }
            let inspection = inspect(&portable.credential, Some(&portable.issuer_proof))
                .map_err(|_| PresentationVerificationError::InvalidProof)?;
            if inspection.id.as_str() != request.credential_id
                || inspection.verification.outcome() != VerificationOutcome::Valid
            {
                return Err(PresentationVerificationError::InvalidProof);
            }
            let public_input = CompactPresentationPublicInput::decode(&portable.public_input)
                .map_err(|_| PresentationVerificationError::InvalidProof)?;
            let now = self
                .clock
                .now()
                .map_err(|_| PresentationVerificationError::Unavailable)?
                .value();
            let current_day = u32::try_from(now / MILLISECONDS_PER_DAY)
                .map_err(|_| PresentationVerificationError::Rejected)?;
            let selection = DigitalPassportPresentationSelection::from_requested_claims(
                &request.requested_claims,
            )
            .map_err(|_| PresentationVerificationError::InvalidProof)?;
            public_input
                .verify_against(
                    &portable.credential,
                    request.challenge_hash,
                    request.verifier_domain_hash,
                    current_day,
                    selection,
                )
                .map_err(|_| PresentationVerificationError::InvalidProof)?;

            let credential = parse_credential(&portable.credential)
                .map_err(|_| PresentationVerificationError::InvalidProof)?;
            let holder_proof = parse_proof(&portable.holder_proof)
                .map_err(|_| PresentationVerificationError::InvalidProof)?;
            let now_seconds = now / 1_000;
            if holder_proof.signer != credential.holder
                || holder_proof.challenge_hash != request.challenge_hash
                || holder_proof.created_at > now_seconds
                || now_seconds - holder_proof.created_at > PRESENTATION_FRESHNESS_SECONDS
                || !verify_presentation_proof(public_input.presentation_root(), &holder_proof)
            {
                return Err(PresentationVerificationError::InvalidProof);
            }

            let (holder_did, holder_method_id) = holder_reference(&credential)
                .map_err(|_| PresentationVerificationError::InvalidProof)?;
            let identity_profile = IdentityProfileId::parse(request.profile_id.as_str().to_owned())
                .map_err(|_| PresentationVerificationError::InvalidProof)?;
            let did = MidnightDid::parse(holder_did.clone())
                .map_err(|_| PresentationVerificationError::InvalidProof)?;
            let record = self
                .get_did
                .execute(DidRecordQuery {
                    profile_id: identity_profile.as_str().to_owned(),
                    did: did.as_str().to_owned(),
                })
                .map_err(map_independent_did_error)?;
            let method = record
                .document
                .verification_methods
                .iter()
                .find(|method| method.id == holder_method_id)
                .ok_or(PresentationVerificationError::InvalidProof)?;
            if record.document.id != holder_did
                || record.document_metadata.deactivated == Some(true)
                || method.controller != holder_did
                || method.public_key_jwk.key_type != "EC"
                || method.public_key_jwk.curve != "Jubjub"
                || !record
                    .managed_method_ids
                    .iter()
                    .any(|candidate| candidate == &holder_method_id)
                || !record.document.relationships.iter().any(|relationship| {
                    relationship.relationship == "assertionMethod"
                        && relationship
                            .method_ids
                            .iter()
                            .any(|candidate| candidate == &holder_method_id)
                })
            {
                return Err(PresentationVerificationError::InvalidProof);
            }
            let expected_public_key = jubjub_public_key(
                &method.public_key_jwk.x,
                method
                    .public_key_jwk
                    .y
                    .as_deref()
                    .ok_or(PresentationVerificationError::InvalidProof)?,
            )
            .map_err(|_| PresentationVerificationError::InvalidProof)?;
            if expected_public_key != holder_proof.public_key {
                return Err(PresentationVerificationError::InvalidProof);
            }

            self.runtime
                .verify_public(
                    Fr::from(0_u64),
                    portable.communications_commitment,
                    &presentation_public_transcript(public_input.statement()),
                    &portable.proof,
                )
                .map_err(|_| PresentationVerificationError::InvalidProof)
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn map_runtime_proving_error(
    error: crate::CompactPresentationRuntimeError,
) -> PresentationProofError {
    match error {
        crate::CompactPresentationRuntimeError::InvalidPreimage
        | crate::CompactPresentationRuntimeError::InvalidProof
        | crate::CompactPresentationRuntimeError::CircuitMismatch
        | crate::CompactPresentationRuntimeError::ArtifactMismatch => {
            PresentationProofError::Rejected
        }
        crate::CompactPresentationRuntimeError::InvalidConfiguration
        | crate::CompactPresentationRuntimeError::ArtifactUnavailable
        | crate::CompactPresentationRuntimeError::ProvingFailed => {
            PresentationProofError::Unavailable
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn map_independent_did_error(error: DidOperationError) -> PresentationVerificationError {
    match error {
        DidOperationError::InvalidProfileIdentifier(_)
        | DidOperationError::InvalidDid(_)
        | DidOperationError::SubjectMismatch
        | DidOperationError::Persistence(DidRecordRepositoryError::NotFound) => {
            PresentationVerificationError::InvalidProof
        }
        _ => PresentationVerificationError::Unavailable,
    }
}

fn map_compact_holder_proof_error(error: CompactHolderProofError) -> PresentationProofError {
    match error {
        CompactHolderProofError::InvalidBinding
        | CompactHolderProofError::NotManaged
        | CompactHolderProofError::Rejected => PresentationProofError::HolderNotAuthorized,
        CompactHolderProofError::Locked | CompactHolderProofError::Unavailable => {
            PresentationProofError::HolderAuthorizationUnavailable
        }
    }
}

pub(crate) fn holder_reference(
    credential: &CompactCredential,
) -> Result<(String, String), CompactPresentationError> {
    if credential.holder.did_contract_address == [0; 32] {
        return Err(CompactPresentationError::InvalidCredential);
    }
    let length = credential
        .holder
        .method_id
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |index| index + 1);
    let fragment = std::str::from_utf8(&credential.holder.method_id[..length])
        .map_err(|_| CompactPresentationError::InvalidCredential)?;
    if fragment.len() < 2
        || !fragment.starts_with('#')
        || !fragment.bytes().all(|byte| {
            byte == b'#'
                || byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'-' | b'_' | b':' | b'%')
        })
    {
        return Err(CompactPresentationError::InvalidCredential);
    }
    let holder_did = format!(
        "did:midnight:undeployed:{}",
        hex::encode(credential.holder.did_contract_address)
    );
    let holder_method_id = format!("{holder_did}{fragment}");
    Ok((holder_did, holder_method_id))
}

fn map_holder_authorization_error(
    error: PresentationHolderAuthorizationError,
) -> PresentationProofError {
    match error {
        PresentationHolderAuthorizationError::Unavailable
        | PresentationHolderAuthorizationError::Locked => {
            PresentationProofError::HolderAuthorizationUnavailable
        }
        PresentationHolderAuthorizationError::InvalidBinding
        | PresentationHolderAuthorizationError::NotManaged
        | PresentationHolderAuthorizationError::Rejected => {
            PresentationProofError::HolderNotAuthorized
        }
    }
}

fn map_repository_error(_: CredentialRepositoryError) -> PresentationProofError {
    PresentationProofError::InvalidCredential
}

fn map_preflight_error(error: CompactPresentationError) -> PresentationProofError {
    match error {
        CompactPresentationError::InvalidSelection => PresentationProofError::InvalidSelection,
        CompactPresentationError::InvalidCredential => PresentationProofError::InvalidCredential,
        CompactPresentationError::InvalidTime
        | CompactPresentationError::InvalidEncoding
        | CompactPresentationError::StatementMismatch => PresentationProofError::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_credential_application::CredentialRepository;
    use oxid_credential_domain::{CredentialPrivateMaterial, CredentialRecord};
    use oxid_foundation::UnixTimestampMillis;
    #[cfg(not(target_arch = "wasm32"))]
    use oxid_identity_application::{
        DidDocumentMetadataView, DidDocumentView, DidRecordView, PublicJwkView,
        VerificationMethodView, VerificationRelationshipView,
    };
    use oxid_platform_ports::PlatformError;

    use crate::{
        standalone_compact_credential, standalone_compact_proof, standalone_private_material,
    };

    fn requested_claims() -> Vec<RequestedPresentationClaim> {
        vec![
            RequestedPresentationClaim::reveal(CLAIM_FIRST_NAME, "First name").expect("claim"),
            RequestedPresentationClaim::reveal(CLAIM_LAST_NAME, "Last name").expect("claim"),
            RequestedPresentationClaim::predicate(
                CLAIM_DATE_OF_BIRTH,
                "Age over 18",
                "age_over",
                18,
            )
            .expect("claim"),
        ]
    }

    #[test]
    fn matches_generated_compact_statement_oracle() {
        let input = prepare_public_input(
            &standalone_compact_credential(),
            &standalone_private_material(),
            [0x11; 32],
            [0x22; 32],
            20_000,
            &requested_claims(),
        )
        .expect("preimage");
        assert_eq!(
            hex::encode(input.credential_root()),
            "b42f1115042cefecbd5380a0a630c0ef5f18bb13e7615cb1de9d36256f100432"
        );
        assert_eq!(
            hex::encode(input.presentation_root()),
            "cf7570efcabe17ba6aa6920aed951f2794a7d609a03a49920694c5c4e09d2876"
        );
        assert_eq!(
            hex::encode(input.consented_claims_hash()),
            "5a442aeb83cd3e589bfc27bd029c5e561ed0aca7109ca4e5642780c2f0bd20a3"
        );
        assert_eq!(
            hex::encode(input.statement()),
            "475caef55fc4b454931beb6b4435688ed36cc1740d33ade45741dcd31214011c"
        );
        let debug = format!("{input:?}");
        assert!(!debug.contains("Alice"));
        assert!(!debug.contains("Example"));
        let decoded = CompactPresentationPublicInput::decode(&input.encode()).expect("decode");
        assert_eq!(decoded, input);
        decoded
            .verify_against(
                &standalone_compact_credential(),
                [0x11; 32],
                [0x22; 32],
                20_000,
                DigitalPassportPresentationSelection::from_requested_claims(&requested_claims())
                    .expect("selection"),
            )
            .expect("independent reconstruction");
    }

    #[test]
    fn native_holder_proof_round_trips_and_binds_the_exact_transcript() {
        let credential = parse_credential(&standalone_compact_credential()).expect("credential");
        let input = prepare_public_input(
            &standalone_compact_credential(),
            &standalone_private_material(),
            [0x11; 32],
            [0x22; 32],
            20_000,
            &requested_claims(),
        )
        .expect("preimage");
        let secret = EmbeddedFr::from(987_654_321_u64);
        let nonce = EmbeddedFr::from(17_u64);
        let mut proof = CompactProof {
            signer: credential.holder,
            created_at: 10_100,
            challenge_hash: [0x11; 32],
            public_key: EmbeddedGroupAffine::generator() * secret,
            announcement: EmbeddedGroupAffine::generator() * nonce,
            response: Fr::from(0_u64),
        };
        let challenge = presentation_proof_challenge(input.presentation_root(), &proof);
        let challenge = EmbeddedFr::try_from(challenge).expect("embedded challenge");
        proof.response =
            Fr::from_le_bytes(&(nonce + challenge * secret).as_le_bytes()).expect("response field");
        assert!(verify_presentation_proof(input.presentation_root(), &proof));

        let encoded = encode_proof(&proof).expect("proof encoding");
        let decoded = parse_proof(&encoded).expect("proof decoding");
        assert!(verify_presentation_proof(
            input.presentation_root(),
            &decoded
        ));
        assert!(!verify_presentation_proof([0x44; 32], &decoded));
        let mut wrong_challenge = decoded;
        wrong_challenge.challenge_hash[0] ^= 1;
        assert!(!verify_presentation_proof(
            input.presentation_root(),
            &wrong_challenge
        ));
        let mut wrong_signer = decoded;
        wrong_signer.signer.method_id[1] ^= 1;
        assert!(!verify_presentation_proof(
            input.presentation_root(),
            &wrong_signer
        ));
    }

    #[test]
    fn rejects_public_input_and_context_tampering() {
        let input = prepare_public_input(
            &standalone_compact_credential(),
            &standalone_private_material(),
            [0x11; 32],
            [0x22; 32],
            20_000,
            &requested_claims(),
        )
        .expect("preimage");
        let mut encoded = input.encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 1;
        let decoded = CompactPresentationPublicInput::decode(&encoded).expect("structural decode");
        assert_eq!(
            decoded.verify_against(
                &standalone_compact_credential(),
                [0x11; 32],
                [0x22; 32],
                20_000,
                DigitalPassportPresentationSelection::from_requested_claims(&requested_claims())
                    .expect("selection"),
            ),
            Err(CompactPresentationError::StatementMismatch)
        );
        assert_eq!(
            input.verify_against(
                &standalone_compact_credential(),
                [0x12; 32],
                [0x22; 32],
                20_000,
                DigitalPassportPresentationSelection::from_requested_claims(&requested_claims())
                    .expect("selection"),
            ),
            Err(CompactPresentationError::StatementMismatch)
        );
        assert_eq!(
            CompactPresentationPublicInput::decode(&input.encode()[..PUBLIC_INPUT_BYTES - 1]),
            Err(CompactPresentationError::InvalidEncoding)
        );
        let mut non_canonical = input.encode();
        // Document number is not selected in this vector, so its public slot
        // and opening must remain canonical zero padding.
        non_canonical[364] = 1;
        assert_eq!(
            CompactPresentationPublicInput::decode(&non_canonical),
            Err(CompactPresentationError::InvalidEncoding)
        );
    }

    struct Clock;

    impl ClockPort for Clock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(20_000 * MILLISECONDS_PER_DAY))
        }
    }

    struct Authorization;

    impl PresentationHolderAuthorizationPort for Authorization {
        fn authorize<'a>(
            &'a self,
            _: PresentationHolderAuthorizationRequest,
        ) -> AuthorizePresentationHolderFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

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

    fn standalone_record(
        detached_proof: Vec<u8>,
        private_material: Vec<u8>,
    ) -> (String, CredentialRecord) {
        let credential = standalone_compact_credential();
        let inspected =
            inspect(&credential, Some(&standalone_compact_proof())).expect("inspection");
        let credential_id = inspected.id.as_str().to_owned();
        let record = CredentialRecord::new_with_proof_and_private_material(
            CredentialProfileId::parse("profile_one").expect("profile"),
            inspected.id,
            credential,
            Some(CredentialDetachedProof::new(detached_proof).expect("proof")),
            Some(CredentialPrivateMaterial::new(private_material).expect("private material")),
            inspected.metadata,
            inspected.verification,
        )
        .expect("record");
        (credential_id, record)
    }

    fn proof_request(credential_id: String) -> PresentationProofRequest {
        PresentationProofRequest {
            profile_id: oxid_presentation_domain::PresentationProfileId::parse("profile_one")
                .expect("profile"),
            credential_id,
            verifier: "standalone verifier".to_owned(),
            challenge_hash: [0x11; 32],
            verifier_domain_hash: [0x22; 32],
            requested_claims: requested_claims(),
        }
    }

    #[test]
    fn standalone_proof_port_requires_the_holder_proof_capability_after_preflight() {
        let (credential_id, record) =
            standalone_record(standalone_compact_proof(), standalone_private_material());
        let adapter = PreflightOnlyCompactPresentationProof::new(
            Arc::new(Repository(record)),
            Arc::new(Clock),
            Arc::new(Authorization),
        );
        let result = poll(adapter.create(proof_request(credential_id)));
        assert_eq!(
            result,
            Err(PresentationProofError::HolderAuthorizationUnavailable)
        );
    }

    #[test]
    fn standalone_preflight_rejects_tampered_proof_and_private_opening() {
        let mut detached_proof = standalone_compact_proof();
        *detached_proof.last_mut().expect("proof byte") ^= 1;
        let (credential_id, record) =
            standalone_record(detached_proof, standalone_private_material());
        let adapter = PreflightOnlyCompactPresentationProof::new(
            Arc::new(Repository(record)),
            Arc::new(Clock),
            Arc::new(Authorization),
        );
        assert_eq!(
            poll(adapter.create(proof_request(credential_id))),
            Err(PresentationProofError::InvalidCredential)
        );

        let mut private_material = standalone_private_material();
        *private_material.last_mut().expect("private byte") ^= 1;
        let (credential_id, record) =
            standalone_record(standalone_compact_proof(), private_material);
        let adapter = PreflightOnlyCompactPresentationProof::new(
            Arc::new(Repository(record)),
            Arc::new(Clock),
            Arc::new(Authorization),
        );
        assert_eq!(
            poll(adapter.create(proof_request(credential_id))),
            Err(PresentationProofError::InvalidCredential)
        );
    }

    #[test]
    fn standalone_preflight_rejects_a_record_id_not_bound_to_the_body() {
        let credential = standalone_compact_credential();
        let detached_proof = standalone_compact_proof();
        let inspected = inspect(&credential, Some(&detached_proof)).expect("inspection");
        let wrong_id = CredentialId::parse("vc_wrong").expect("credential id");
        let record = CredentialRecord::new_with_proof_and_private_material(
            CredentialProfileId::parse("profile_one").expect("profile"),
            wrong_id.clone(),
            credential,
            Some(CredentialDetachedProof::new(detached_proof).expect("proof")),
            Some(
                CredentialPrivateMaterial::new(standalone_private_material())
                    .expect("private material"),
            ),
            inspected.metadata,
            inspected.verification,
        )
        .expect("record");
        let adapter = PreflightOnlyCompactPresentationProof::new(
            Arc::new(Repository(record)),
            Arc::new(Clock),
            Arc::new(Authorization),
        );
        assert_eq!(
            poll(adapter.create(proof_request(wrong_id.as_str().to_owned()))),
            Err(PresentationProofError::InvalidCredential)
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    struct HolderProof;

    #[cfg(not(target_arch = "wasm32"))]
    impl CompactHolderProofPort for HolderProof {
        fn create_holder_proof(
            &self,
            request: CompactHolderProofRequest,
        ) -> Result<Vec<u8>, CompactHolderProofError> {
            let signer = compact_holder_reference(&request.holder_did, &request.holder_method_id)?;
            let secret = EmbeddedFr::from(987_654_321_u64);
            let nonce = EmbeddedFr::from(17_u64);
            let mut proof = CompactProof {
                signer,
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
            encode_proof(&proof).map_err(|_| CompactHolderProofError::Rejected)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[derive(Clone)]
    struct DidLookup(DidRecordView);

    #[cfg(not(target_arch = "wasm32"))]
    impl GetDidRecordUseCase for DidLookup {
        fn execute(&self, query: DidRecordQuery) -> Result<DidRecordView, DidOperationError> {
            if query.profile_id == "profile_one" && query.did == self.0.document.id {
                Ok(self.0.clone())
            } else {
                Err(DidOperationError::Persistence(
                    DidRecordRepositoryError::NotFound,
                ))
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    struct ClockAt(u64);

    #[cfg(not(target_arch = "wasm32"))]
    impl ClockPort for ClockAt {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(self.0))
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn holder_did_record() -> DidRecordView {
        let credential = parse_credential(&standalone_compact_credential()).expect("credential");
        let (did, method_id) = holder_reference(&credential).expect("holder reference");
        let public_key = EmbeddedGroupAffine::generator() * EmbeddedFr::from(987_654_321_u64);
        DidRecordView {
            document: DidDocumentView {
                contexts: vec!["https://www.w3.org/ns/did/v1".to_owned()],
                id: did.clone(),
                network: "undeployed".to_owned(),
                also_known_as: Vec::new(),
                verification_methods: vec![VerificationMethodView {
                    id: method_id.clone(),
                    controller: did,
                    public_key_jwk: PublicJwkView {
                        key_type: "EC".to_owned(),
                        curve: "Jubjub".to_owned(),
                        x: general_purpose::URL_SAFE_NO_PAD
                            .encode(public_key.x().expect("holder x-coordinate").as_le_bytes()),
                        y: Some(
                            general_purpose::URL_SAFE_NO_PAD
                                .encode(public_key.y().expect("holder y-coordinate").as_le_bytes()),
                        ),
                    },
                }],
                relationships: vec![VerificationRelationshipView {
                    relationship: "assertionMethod".to_owned(),
                    method_ids: vec![method_id.clone()],
                }],
                services: Vec::new(),
            },
            document_metadata: DidDocumentMetadataView {
                created: None,
                updated: None,
                deactivated: Some(false),
                version_id: None,
                next_update: None,
                next_version_id: None,
                equivalent_ids: Vec::new(),
                canonical_id: None,
            },
            content_type: Some("application/did+ld+json".to_owned()),
            source: "standalone".to_owned(),
            managed_method_ids: vec![method_id],
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn verification_request(
        credential_id: String,
        proof: PresentationProofArtifact,
    ) -> PresentationVerificationRequest {
        PresentationVerificationRequest {
            profile_id: oxid_presentation_domain::PresentationProfileId::parse("profile_one")
                .expect("profile"),
            credential_id,
            verifier: "standalone verifier".to_owned(),
            challenge_hash: [0x11; 32],
            verifier_domain_hash: [0x22; 32],
            requested_claims: requested_claims(),
            proof,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn tampered_artifact(
        artifact: &PresentationProofArtifact,
        mutate: impl FnOnce(&mut PortableCompactPresentation),
    ) -> PresentationProofArtifact {
        let mut portable =
            decode_portable_presentation(artifact.as_bytes()).expect("portable presentation");
        mutate(&mut portable);
        PresentationProofArtifact::new(
            encode_portable_presentation(&portable).expect("tampered envelope remains structural"),
        )
        .expect("presentation artifact")
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "requires the authenticated p18 Compact proving artifact closure"]
    fn native_runtime_proves_restarts_and_rejects_public_tampering() {
        const NOW: u64 = 20_000 * MILLISECONDS_PER_DAY;
        let root = std::env::var_os("OXID_PRESENTATION_ARTIFACTS_DIR")
            .expect("set OXID_PRESENTATION_ARTIFACTS_DIR to the Nix artifact closure");
        let config =
            crate::CompactPresentationArtifactsConfig::new(root).expect("configured artifact root");
        let runtime = Arc::new(
            crate::NativeCompactPresentationRuntime::load(&config).expect("artifact runtime"),
        );
        let (credential_id, record) =
            standalone_record(standalone_compact_proof(), standalone_private_material());
        let adapter = PreflightOnlyCompactPresentationProof::with_runtime(
            Arc::new(Repository(record)),
            Arc::new(ClockAt(NOW)),
            Arc::new(Authorization),
            Arc::new(HolderProof),
            Arc::clone(&runtime),
        );
        let artifact = poll(adapter.create(proof_request(credential_id.clone())))
            .expect("checked proof creation");
        let did = Arc::new(DidLookup(holder_did_record()));
        let verifier = NativeCompactPresentationVerifier::new(
            Arc::clone(&runtime),
            Arc::new(ClockAt(NOW)),
            did.clone(),
        );
        let request = verification_request(credential_id.clone(), artifact.clone());
        assert_eq!(poll(verifier.verify(request.clone())), Ok(()));

        let mut checksum_tamper = artifact.as_bytes().to_vec();
        checksum_tamper[8] ^= 1;
        let checksum_tamper = PresentationProofArtifact::new(checksum_tamper).expect("artifact");
        assert_eq!(
            poll(verifier.verify(verification_request(credential_id.clone(), checksum_tamper,))),
            Err(PresentationVerificationError::InvalidProof)
        );

        let semantic_tampers = [
            tampered_artifact(&artifact, |portable| portable.artifact_identity[0] ^= 1),
            tampered_artifact(&artifact, |portable| portable.credential[0] ^= 1),
            tampered_artifact(&artifact, |portable| portable.issuer_proof[0] ^= 1),
            tampered_artifact(&artifact, |portable| portable.public_input[8] ^= 1),
            tampered_artifact(&artifact, |portable| portable.holder_proof[0] ^= 1),
            tampered_artifact(&artifact, |portable| {
                portable.communications_commitment = Fr::from(123_u64);
            }),
            tampered_artifact(&artifact, |portable| portable.proof.0[0] ^= 1),
        ];
        for tampered in semantic_tampers {
            assert_eq!(
                poll(verifier.verify(verification_request(credential_id.clone(), tampered,))),
                Err(PresentationVerificationError::InvalidProof)
            );
        }

        let mut wrong_request = request.clone();
        wrong_request.challenge_hash[0] ^= 1;
        assert_eq!(
            poll(verifier.verify(wrong_request)),
            Err(PresentationVerificationError::InvalidProof)
        );
        let stale_verifier = NativeCompactPresentationVerifier::new(
            Arc::clone(&runtime),
            Arc::new(ClockAt(NOW + (PRESENTATION_FRESHNESS_SECONDS + 1) * 1_000)),
            did.clone(),
        );
        assert_eq!(
            poll(stale_verifier.verify(request.clone())),
            Err(PresentationVerificationError::InvalidProof)
        );

        drop(verifier);
        drop(runtime);
        let restarted_runtime = Arc::new(
            crate::NativeCompactPresentationRuntime::load(&config).expect("restarted runtime"),
        );
        let restarted_verifier =
            NativeCompactPresentationVerifier::new(restarted_runtime, Arc::new(ClockAt(NOW)), did);
        assert_eq!(poll(restarted_verifier.verify(request)), Ok(()));
    }

    fn poll<T>(
        mut future: std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + '_>>,
    ) -> T {
        use std::task::{Context, Poll, Waker};
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("fixture future must be ready"),
        }
    }
}
