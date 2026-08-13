// SPDX-License-Identifier: Apache-2.0

//! Exact public-input boundary for the Oxid Digital Passport presentation
//! circuit.
//!
//! This adapter prepares and independently reconstructs the public statement
//! without producing a proof. Protected values and openings are used only to
//! build the presentation preimage and never enter the portable `MPS1` public
//! input. The proof port remains deliberately unavailable until protected
//! Jubjub custody and the reviewed prover runtime are connected.

use std::{collections::BTreeSet, fmt, sync::Arc};

use oxid_credential_application::{CredentialRepository, CredentialRepositoryError};
use oxid_credential_domain::{
    CredentialDetachedProof, CredentialFormat, CredentialId, CredentialProfileId,
    VerificationOutcome,
};
use oxid_platform_ports::ClockPort;
use oxid_presentation_application::{
    CreatePresentationProofFuture, PresentationProofError, PresentationProofPort,
    PresentationProofRequest,
};
use oxid_presentation_domain::{PresentationClaimIntent, RequestedPresentationClaim};

use crate::compact_digital_passport::{
    CompactCredential, credential_body_root, inspect, parse_credential, persistent_hash,
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
const CONSENTED_CLAIMS_DOMAIN: &[u8] = b"oxid:compact-vp:claims:v1";

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
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PublicDisclosures {
    reveal_first_name: bool,
    first_name: [u8; 64],
    first_name_opening: [u8; 32],
    reveal_last_name: bool,
    last_name: [u8; 64],
    last_name_opening: [u8; 32],
    prove_age: bool,
    age_threshold_years: u8,
    reveal_document_number: bool,
    document_number: [u8; 32],
    document_number_opening: [u8; 32],
    reveal_issuing_state: bool,
    issuing_state: [u8; 32],
    issuing_state_opening: [u8; 32],
}

impl PublicDisclosures {
    fn from_private_parts(
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
    credential_root: [u8; 32],
    presentation_root: [u8; 32],
    verifier_challenge_hash: [u8; 32],
    verifier_domain_hash: [u8; 32],
    consented_claims_hash: [u8; 32],
    current_day: u32,
    disclosures: PublicDisclosures,
    statement: [u8; 32],
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

fn presentation_root(credential: &CompactCredential, disclosures: &PublicDisclosures) -> [u8; 32] {
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

fn consented_claims_hash(disclosures: &PublicDisclosures) -> [u8; 32] {
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

fn presentation_statement(
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

/// Standalone proof port that performs exact proof-preimage validation and
/// then fails closed before proof construction. This makes the headless/mobile
/// consent flow exercise the real credential, opening, statement, and time
/// boundaries without manufacturing a proof or `vp_token`.
pub struct PreflightOnlyCompactPresentationProof {
    repository: Arc<dyn CredentialRepository>,
    clock: Arc<dyn ClockPort>,
}

impl PreflightOnlyCompactPresentationProof {
    #[must_use]
    pub const fn new(repository: Arc<dyn CredentialRepository>, clock: Arc<dyn ClockPort>) -> Self {
        Self { repository, clock }
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
            let proof = record
                .detached_proof()
                .map(CredentialDetachedProof::as_bytes)
                .ok_or(PresentationProofError::InvalidCredential)?;
            let inspection = inspect(record.signed_bytes(), Some(proof))
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
            Err(PresentationProofError::Unavailable)
        })
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
    fn standalone_proof_port_reaches_exact_preflight_then_fails_closed() {
        let (credential_id, record) =
            standalone_record(standalone_compact_proof(), standalone_private_material());
        let adapter = PreflightOnlyCompactPresentationProof::new(
            Arc::new(Repository(record)),
            Arc::new(Clock),
        );
        let result = poll(adapter.create(proof_request(credential_id)));
        assert!(matches!(result, Err(PresentationProofError::Unavailable)));
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
        );
        assert_eq!(
            poll(adapter.create(proof_request(wrong_id.as_str().to_owned()))),
            Err(PresentationProofError::InvalidCredential)
        );
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
