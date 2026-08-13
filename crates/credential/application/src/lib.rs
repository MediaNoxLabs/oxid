// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use oxid_credential_domain::{
    CredentialClaimPrivacy, CredentialDetachedProof, CredentialDisclosureManifest,
    CredentialDomainError, CredentialId, CredentialMetadata, CredentialPrivateMaterial,
    CredentialProfileId, CredentialRecord, VerificationOutcome, VerificationReport,
};
use oxid_foundation::OpaqueIdError;

pub type CredentialBytesFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<u8>, CredentialIngressError>> + Send + 'a>>;
pub type CredentialInspectionFuture<'a> = Pin<
    Box<dyn Future<Output = Result<CredentialInspection, CredentialVerificationError>> + Send + 'a>,
>;
pub type CredentialViewFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CredentialView, CredentialOperationError>> + Send + 'a>>;

pub trait CredentialInboxPort: Send + Sync {
    fn receive<'a>(&'a self) -> CredentialBytesFuture<'a>;
}

pub trait CredentialVerificationPort: Send + Sync {
    fn inspect<'a>(
        &'a self,
        signed_bytes: &'a [u8],
        detached_proof: Option<&'a [u8]>,
    ) -> CredentialInspectionFuture<'a>;
}

/// Schema adapter for protected claims. Implementations must validate that
/// private material opens commitments covered by the signed credential before
/// returning public candidates or a targeted local value.
pub trait CredentialDisclosurePort: Send + Sync {
    fn inspect(
        &self,
        signed_bytes: &[u8],
        private_material: &[u8],
    ) -> Result<CredentialDisclosureManifest, CredentialDisclosurePortError>;

    fn reveal_local(
        &self,
        signed_bytes: &[u8],
        private_material: &[u8],
        claim_path: &str,
    ) -> Result<CredentialLocalClaim, CredentialDisclosurePortError>;
}

pub trait CredentialRepository: Send + Sync {
    fn upsert(&self, record: CredentialRecord) -> Result<(), CredentialRepositoryError>;
    fn list(
        &self,
        profile_id: &CredentialProfileId,
    ) -> Result<Vec<CredentialRecord>, CredentialRepositoryError>;
    fn get(
        &self,
        profile_id: &CredentialProfileId,
        credential_id: &CredentialId,
    ) -> Result<CredentialRecord, CredentialRepositoryError>;
    fn remove(
        &self,
        profile_id: &CredentialProfileId,
        credential_id: &CredentialId,
    ) -> Result<(), CredentialRepositoryError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialInspection {
    pub id: CredentialId,
    pub metadata: CredentialMetadata,
    pub verification: VerificationReport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialIngressError {
    Unavailable,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialVerificationError {
    Unavailable,
    UnsupportedFormat,
    InvalidCredential,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialRepositoryError {
    NotFound,
    CapacityExceeded,
    Integrity,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialDisclosurePortError {
    Unavailable,
    UnsupportedCredential,
    MissingPrivateMaterial,
    InvalidPrivateMaterial,
    ClaimNotFound,
    ClaimNotRevealable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialOperationError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidCredentialIdentifier(OpaqueIdError),
    Domain(CredentialDomainError),
    Ingress(CredentialIngressError),
    Verification(CredentialVerificationError),
    Disclosure(CredentialDisclosurePortError),
    Persistence(CredentialRepositoryError),
    ConfirmationRequired,
    InvalidConfirmation,
    VerificationNotValid,
}

impl fmt::Display for CredentialOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) | Self::InvalidCredentialIdentifier(error) => {
                error.fmt(formatter)
            }
            Self::Domain(error) => error.fmt(formatter),
            Self::Ingress(error) => error.fmt(formatter),
            Self::Verification(error) => error.fmt(formatter),
            Self::Disclosure(error) => error.fmt(formatter),
            Self::Persistence(error) => error.fmt(formatter),
            Self::ConfirmationRequired => formatter.write_str("explicit confirmation is required"),
            Self::InvalidConfirmation => formatter.write_str("confirmation intent is invalid"),
            Self::VerificationNotValid => {
                formatter.write_str("credential verification did not produce a valid outcome")
            }
        }
    }
}

impl Error for CredentialOperationError {}

macro_rules! display_error {
    ($type:ty, $($variant:ident => $message:literal),+ $(,)?) => {
        impl fmt::Display for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(match self { $(Self::$variant => $message),+ })
            }
        }
        impl Error for $type {}
    };
}

display_error!(CredentialIngressError,
    Unavailable => "credential ingress capability is unavailable",
    Rejected => "credential ingress was rejected",
);
display_error!(CredentialVerificationError,
    Unavailable => "credential verification capability is unavailable",
    UnsupportedFormat => "credential format is unsupported",
    InvalidCredential => "credential could not be inspected",
);
display_error!(CredentialRepositoryError,
    NotFound => "credential was not found",
    CapacityExceeded => "credential capacity was exceeded",
    Integrity => "credential storage failed integrity validation",
    Unavailable => "credential storage is unavailable",
);
display_error!(CredentialDisclosurePortError,
    Unavailable => "credential disclosure capability is unavailable",
    UnsupportedCredential => "credential schema does not support disclosure preview",
    MissingPrivateMaterial => "credential has no protected claim material",
    InvalidPrivateMaterial => "credential protected claim material is invalid",
    ClaimNotFound => "credential claim was not found",
    ClaimNotRevealable => "credential claim cannot be revealed locally",
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialQuery {
    pub profile_id: String,
    pub credential_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialProfileQuery {
    pub profile_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteCredentialCommand {
    pub profile_id: String,
    pub credential_id: String,
    pub confirmed: bool,
    pub intent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialDisclosureQuery {
    pub profile_id: String,
    pub credential_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialPredicateInput {
    pub claim_path: String,
    pub kind: String,
    pub threshold: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewCredentialDisclosureCommand {
    pub profile_id: String,
    pub credential_id: String,
    pub reveal_claim_paths: Vec<String>,
    pub predicates: Vec<CredentialPredicateInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevealCredentialClaimCommand {
    pub profile_id: String,
    pub credential_id: String,
    pub claim_path: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CredentialPrivateMaterialInput(CredentialPrivateMaterial);

impl CredentialPrivateMaterialInput {
    pub fn new(bytes: Vec<u8>) -> Result<Self, CredentialDomainError> {
        CredentialPrivateMaterial::new(bytes).map(Self)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    fn into_domain(self) -> CredentialPrivateMaterial {
        self.0
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct CredentialDetachedProofInput(CredentialDetachedProof);

impl CredentialDetachedProofInput {
    pub fn new(bytes: Vec<u8>) -> Result<Self, CredentialDomainError> {
        CredentialDetachedProof::new(bytes).map(Self)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    fn into_domain(self) -> CredentialDetachedProof {
        self.0
    }
}

impl fmt::Debug for CredentialDetachedProofInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialDetachedProofInput")
            .field("length", &self.as_bytes().len())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for CredentialPrivateMaterialInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialPrivateMaterialInput")
            .field("length", &self.as_bytes().len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ImportVerifiedCredentialCommand {
    pub profile_id: String,
    pub signed_bytes: Vec<u8>,
    pub detached_proof: Option<CredentialDetachedProofInput>,
    pub private_material: Option<CredentialPrivateMaterialInput>,
}

impl fmt::Debug for ImportVerifiedCredentialCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImportVerifiedCredentialCommand")
            .field("profile_id", &self.profile_id)
            .field("signed_bytes_length", &self.signed_bytes.len())
            .field(
                "detached_proof_length",
                &self
                    .detached_proof
                    .as_ref()
                    .map(|proof| proof.as_bytes().len()),
            )
            .field(
                "private_material_length",
                &self
                    .private_material
                    .as_ref()
                    .map(|material| material.as_bytes().len()),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationStageView {
    pub name: String,
    pub status: String,
    pub reason_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialView {
    pub id: String,
    pub display_name: String,
    pub issuer_did: String,
    pub subject_did: Option<String>,
    pub format: String,
    pub issued_at_ms: Option<u64>,
    pub verification_outcome: String,
    pub verification_stages: Vec<VerificationStageView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialDisclosureCandidateView {
    pub claim_path: String,
    pub label: String,
    pub privacy_tier: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialDisclosureView {
    pub credential_id: String,
    pub schema_id: String,
    pub candidates: Vec<CredentialDisclosureCandidateView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialPredicateView {
    pub claim_path: String,
    pub label: String,
    pub kind: String,
    pub threshold: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialDisclosurePlanView {
    pub credential_id: String,
    pub schema_id: String,
    pub reveals: Vec<CredentialDisclosureCandidateView>,
    pub predicates: Vec<CredentialPredicateView>,
    pub outcome: String,
    pub presentation_generated: bool,
}

/// A value exposed only through the explicit local-reveal use case. Custom
/// diagnostics prevent UI state and error reports from printing the value.
#[derive(Clone, PartialEq, Eq)]
pub struct CredentialLocalClaim {
    claim_path: String,
    value: String,
}

impl CredentialLocalClaim {
    pub fn new(
        claim_path: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, CredentialDisclosurePortError> {
        let claim_path = claim_path.into();
        let value = value.into();
        if claim_path.is_empty()
            || value.is_empty()
            || value.chars().count() > 256
            || value.chars().any(|character| {
                character.is_control() || matches!(character, '<' | '>' | '\u{202a}'..='\u{202e}')
            })
        {
            return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
        }
        Ok(Self { claim_path, value })
    }

    #[must_use]
    pub fn claim_path(&self) -> &str {
        &self.claim_path
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for CredentialLocalClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialLocalClaim")
            .field("claim_path", &self.claim_path)
            .field("value_length", &self.value.chars().count())
            .finish_non_exhaustive()
    }
}

impl From<&CredentialRecord> for CredentialView {
    fn from(record: &CredentialRecord) -> Self {
        let metadata = record.metadata();
        Self {
            id: record.id().as_str().to_owned(),
            display_name: metadata.display_name().to_owned(),
            issuer_did: metadata.issuer_did().to_owned(),
            subject_did: metadata.subject_did().map(str::to_owned),
            format: metadata.format().as_str().to_owned(),
            issued_at_ms: metadata.issued_at().map(|value| value.value()),
            verification_outcome: record.verification().outcome().as_str().to_owned(),
            verification_stages: record
                .verification()
                .stages()
                .iter()
                .map(|stage| VerificationStageView {
                    name: stage.name().as_str().to_owned(),
                    status: stage.status().as_str().to_owned(),
                    reason_code: stage.reason_code().map(str::to_owned),
                })
                .collect(),
        }
    }
}

pub trait ReceiveCredentialUseCase: Send + Sync {
    fn execute<'a>(&'a self, query: CredentialProfileQuery) -> CredentialViewFuture<'a>;
}
pub trait ImportVerifiedCredentialUseCase: Send + Sync {
    fn execute<'a>(&'a self, command: ImportVerifiedCredentialCommand) -> CredentialViewFuture<'a>;
}
pub trait ListCredentialsUseCase: Send + Sync {
    fn execute(
        &self,
        query: CredentialProfileQuery,
    ) -> Result<Vec<CredentialView>, CredentialOperationError>;
}
pub trait GetCredentialUseCase: Send + Sync {
    fn execute(&self, query: CredentialQuery) -> Result<CredentialView, CredentialOperationError>;
}
pub trait ReverifyCredentialUseCase: Send + Sync {
    fn execute<'a>(&'a self, query: CredentialQuery) -> CredentialViewFuture<'a>;
}
pub trait DeleteCredentialUseCase: Send + Sync {
    fn execute(&self, command: DeleteCredentialCommand) -> Result<(), CredentialOperationError>;
}
pub trait GetCredentialDisclosureUseCase: Send + Sync {
    fn execute(
        &self,
        query: CredentialDisclosureQuery,
    ) -> Result<CredentialDisclosureView, CredentialOperationError>;
}
pub trait PreviewCredentialDisclosureUseCase: Send + Sync {
    fn execute(
        &self,
        command: PreviewCredentialDisclosureCommand,
    ) -> Result<CredentialDisclosurePlanView, CredentialOperationError>;
}
pub trait RevealCredentialClaimUseCase: Send + Sync {
    fn execute(
        &self,
        command: RevealCredentialClaimCommand,
    ) -> Result<CredentialLocalClaim, CredentialOperationError>;
}

pub struct CredentialService {
    repository: Arc<dyn CredentialRepository>,
    inbox: Arc<dyn CredentialInboxPort>,
    verifier: Arc<dyn CredentialVerificationPort>,
    disclosure: Arc<dyn CredentialDisclosurePort>,
}

impl CredentialService {
    #[must_use]
    pub const fn from_ports(
        repository: Arc<dyn CredentialRepository>,
        inbox: Arc<dyn CredentialInboxPort>,
        verifier: Arc<dyn CredentialVerificationPort>,
        disclosure: Arc<dyn CredentialDisclosurePort>,
    ) -> Self {
        Self {
            repository,
            inbox,
            verifier,
            disclosure,
        }
    }

    async fn import(
        &self,
        profile_id: CredentialProfileId,
        bytes: Vec<u8>,
        detached_proof: Option<CredentialDetachedProof>,
        private_material: Option<CredentialPrivateMaterial>,
        require_valid: bool,
    ) -> Result<CredentialView, CredentialOperationError> {
        let inspection = self
            .verifier
            .inspect(
                &bytes,
                detached_proof
                    .as_ref()
                    .map(CredentialDetachedProof::as_bytes),
            )
            .await
            .map_err(CredentialOperationError::Verification)?;
        if require_valid && inspection.verification.outcome() != VerificationOutcome::Valid {
            return Err(CredentialOperationError::VerificationNotValid);
        }
        if let Some(material) = private_material.as_ref() {
            self.disclosure
                .inspect(&bytes, material.as_bytes())
                .map_err(CredentialOperationError::Disclosure)?;
        }
        let record = CredentialRecord::new_with_proof_and_private_material(
            profile_id,
            inspection.id,
            bytes,
            detached_proof,
            private_material,
            inspection.metadata,
            inspection.verification,
        )
        .map_err(CredentialOperationError::Domain)?;
        self.repository
            .upsert(record.clone())
            .map_err(CredentialOperationError::Persistence)?;
        Ok(CredentialView::from(&record))
    }
}

fn disclosure_candidate_view(
    candidate: &oxid_credential_domain::CredentialDisclosureCandidate,
) -> CredentialDisclosureCandidateView {
    CredentialDisclosureCandidateView {
        claim_path: candidate.path().to_owned(),
        label: candidate.label().to_owned(),
        privacy_tier: candidate.privacy().as_str().to_owned(),
    }
}

fn disclosure_record(
    repository: &dyn CredentialRepository,
    query: CredentialDisclosureQuery,
) -> Result<CredentialRecord, CredentialOperationError> {
    let profile_id = profile(query.profile_id)?;
    let credential_id = credential_id(query.credential_id)?;
    let record = repository
        .get(&profile_id, &credential_id)
        .map_err(CredentialOperationError::Persistence)?;
    if record.verification().outcome() != VerificationOutcome::Valid {
        return Err(CredentialOperationError::VerificationNotValid);
    }
    Ok(record)
}

fn inspect_disclosure(
    disclosure: &dyn CredentialDisclosurePort,
    record: &CredentialRecord,
) -> Result<CredentialDisclosureManifest, CredentialOperationError> {
    let private_material =
        record
            .private_material()
            .ok_or(CredentialOperationError::Disclosure(
                CredentialDisclosurePortError::MissingPrivateMaterial,
            ))?;
    disclosure
        .inspect(record.signed_bytes(), private_material.as_bytes())
        .map_err(CredentialOperationError::Disclosure)
}

fn profile(value: String) -> Result<CredentialProfileId, CredentialOperationError> {
    CredentialProfileId::parse(value).map_err(CredentialOperationError::InvalidProfileIdentifier)
}

fn credential_id(value: String) -> Result<CredentialId, CredentialOperationError> {
    CredentialId::parse(value).map_err(CredentialOperationError::InvalidCredentialIdentifier)
}

impl ReceiveCredentialUseCase for CredentialService {
    fn execute<'a>(&'a self, query: CredentialProfileQuery) -> CredentialViewFuture<'a> {
        Box::pin(async move {
            let profile_id = profile(query.profile_id)?;
            let bytes = self
                .inbox
                .receive()
                .await
                .map_err(CredentialOperationError::Ingress)?;
            self.import(profile_id, bytes, None, None, false).await
        })
    }
}

impl ImportVerifiedCredentialUseCase for CredentialService {
    fn execute<'a>(&'a self, command: ImportVerifiedCredentialCommand) -> CredentialViewFuture<'a> {
        Box::pin(async move {
            let profile_id = profile(command.profile_id)?;
            self.import(
                profile_id,
                command.signed_bytes,
                command
                    .detached_proof
                    .map(CredentialDetachedProofInput::into_domain),
                command
                    .private_material
                    .map(CredentialPrivateMaterialInput::into_domain),
                true,
            )
            .await
        })
    }
}

impl ListCredentialsUseCase for CredentialService {
    fn execute(
        &self,
        query: CredentialProfileQuery,
    ) -> Result<Vec<CredentialView>, CredentialOperationError> {
        let profile_id = profile(query.profile_id)?;
        let mut records = self
            .repository
            .list(&profile_id)
            .map_err(CredentialOperationError::Persistence)?;
        records.sort_by(|left, right| left.id().cmp(right.id()));
        Ok(records.iter().map(CredentialView::from).collect())
    }
}

impl GetCredentialUseCase for CredentialService {
    fn execute(&self, query: CredentialQuery) -> Result<CredentialView, CredentialOperationError> {
        let profile_id = profile(query.profile_id)?;
        let credential_id = credential_id(query.credential_id)?;
        let record = self
            .repository
            .get(&profile_id, &credential_id)
            .map_err(CredentialOperationError::Persistence)?;
        Ok(CredentialView::from(&record))
    }
}

impl ReverifyCredentialUseCase for CredentialService {
    fn execute<'a>(&'a self, query: CredentialQuery) -> CredentialViewFuture<'a> {
        Box::pin(async move {
            let profile_id = profile(query.profile_id)?;
            let credential_id = credential_id(query.credential_id)?;
            let mut record = self
                .repository
                .get(&profile_id, &credential_id)
                .map_err(CredentialOperationError::Persistence)?;
            let inspection = self
                .verifier
                .inspect(
                    record.signed_bytes(),
                    record
                        .detached_proof()
                        .map(CredentialDetachedProof::as_bytes),
                )
                .await
                .map_err(CredentialOperationError::Verification)?;
            if let Some(material) = record.private_material() {
                self.disclosure
                    .inspect(record.signed_bytes(), material.as_bytes())
                    .map_err(CredentialOperationError::Disclosure)?;
            }
            record
                .replace_inspection(inspection.id, inspection.metadata, inspection.verification)
                .map_err(CredentialOperationError::Domain)?;
            self.repository
                .upsert(record.clone())
                .map_err(CredentialOperationError::Persistence)?;
            Ok(CredentialView::from(&record))
        })
    }
}

impl DeleteCredentialUseCase for CredentialService {
    fn execute(&self, command: DeleteCredentialCommand) -> Result<(), CredentialOperationError> {
        if !command.confirmed {
            return Err(CredentialOperationError::ConfirmationRequired);
        }
        if command.intent != "DELETE_CREDENTIAL" {
            return Err(CredentialOperationError::InvalidConfirmation);
        }
        let profile_id = profile(command.profile_id)?;
        let credential_id = credential_id(command.credential_id)?;
        self.repository
            .remove(&profile_id, &credential_id)
            .map_err(CredentialOperationError::Persistence)
    }
}

impl GetCredentialDisclosureUseCase for CredentialService {
    fn execute(
        &self,
        query: CredentialDisclosureQuery,
    ) -> Result<CredentialDisclosureView, CredentialOperationError> {
        let record = disclosure_record(self.repository.as_ref(), query)?;
        let manifest = inspect_disclosure(self.disclosure.as_ref(), &record)?;
        Ok(CredentialDisclosureView {
            credential_id: record.id().as_str().to_owned(),
            schema_id: manifest.schema_id().to_owned(),
            candidates: manifest
                .candidates()
                .iter()
                .map(disclosure_candidate_view)
                .collect(),
        })
    }
}

impl PreviewCredentialDisclosureUseCase for CredentialService {
    fn execute(
        &self,
        command: PreviewCredentialDisclosureCommand,
    ) -> Result<CredentialDisclosurePlanView, CredentialOperationError> {
        if command.reveal_claim_paths.len() > 64 || command.predicates.len() > 64 {
            return Err(CredentialOperationError::Disclosure(
                CredentialDisclosurePortError::ClaimNotFound,
            ));
        }
        let record = disclosure_record(
            self.repository.as_ref(),
            CredentialDisclosureQuery {
                profile_id: command.profile_id,
                credential_id: command.credential_id,
            },
        )?;
        let manifest = inspect_disclosure(self.disclosure.as_ref(), &record)?;
        let mut unique = std::collections::BTreeSet::new();
        let reveals = command
            .reveal_claim_paths
            .iter()
            .map(|path| {
                if !unique.insert(path.as_str()) {
                    return Err(CredentialDisclosurePortError::ClaimNotFound);
                }
                let candidate = manifest
                    .candidates()
                    .iter()
                    .find(|candidate| candidate.path() == path)
                    .ok_or(CredentialDisclosurePortError::ClaimNotFound)?;
                if candidate.privacy() != CredentialClaimPrivacy::SelectiveDisclosure {
                    return Err(CredentialDisclosurePortError::ClaimNotRevealable);
                }
                Ok(disclosure_candidate_view(candidate))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(CredentialOperationError::Disclosure)?;
        let predicates = command
            .predicates
            .iter()
            .map(|predicate| {
                if predicate.kind != "age_over"
                    || !(1..=120).contains(&predicate.threshold)
                    || !unique.insert(predicate.claim_path.as_str())
                {
                    return Err(CredentialDisclosurePortError::ClaimNotFound);
                }
                let candidate = manifest
                    .candidates()
                    .iter()
                    .find(|candidate| candidate.path() == predicate.claim_path)
                    .ok_or(CredentialDisclosurePortError::ClaimNotFound)?;
                if candidate.privacy() != CredentialClaimPrivacy::PredicateOnly {
                    return Err(CredentialDisclosurePortError::ClaimNotRevealable);
                }
                Ok(CredentialPredicateView {
                    claim_path: candidate.path().to_owned(),
                    label: candidate.label().to_owned(),
                    kind: predicate.kind.clone(),
                    threshold: predicate.threshold,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(CredentialOperationError::Disclosure)?;
        if reveals.is_empty() && predicates.is_empty() {
            return Err(CredentialOperationError::Disclosure(
                CredentialDisclosurePortError::ClaimNotFound,
            ));
        }
        Ok(CredentialDisclosurePlanView {
            credential_id: record.id().as_str().to_owned(),
            schema_id: manifest.schema_id().to_owned(),
            reveals,
            predicates,
            outcome: "local_preview_ready".to_owned(),
            presentation_generated: false,
        })
    }
}

impl RevealCredentialClaimUseCase for CredentialService {
    fn execute(
        &self,
        command: RevealCredentialClaimCommand,
    ) -> Result<CredentialLocalClaim, CredentialOperationError> {
        let claim_path = command.claim_path;
        let record = disclosure_record(
            self.repository.as_ref(),
            CredentialDisclosureQuery {
                profile_id: command.profile_id,
                credential_id: command.credential_id,
            },
        )?;
        let private_material =
            record
                .private_material()
                .ok_or(CredentialOperationError::Disclosure(
                    CredentialDisclosurePortError::MissingPrivateMaterial,
                ))?;
        self.disclosure
            .reveal_local(
                record.signed_bytes(),
                private_material.as_bytes(),
                &claim_path,
            )
            .map_err(CredentialOperationError::Disclosure)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialInbox;
impl CredentialInboxPort for UnavailableCredentialInbox {
    fn receive<'a>(&'a self) -> CredentialBytesFuture<'a> {
        Box::pin(async { Err(CredentialIngressError::Unavailable) })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialVerifier;
impl CredentialVerificationPort for UnavailableCredentialVerifier {
    fn inspect<'a>(&'a self, _: &'a [u8], _: Option<&'a [u8]>) -> CredentialInspectionFuture<'a> {
        Box::pin(async { Err(CredentialVerificationError::Unavailable) })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialDisclosure;
impl CredentialDisclosurePort for UnavailableCredentialDisclosure {
    fn inspect(
        &self,
        _: &[u8],
        _: &[u8],
    ) -> Result<CredentialDisclosureManifest, CredentialDisclosurePortError> {
        Err(CredentialDisclosurePortError::Unavailable)
    }

    fn reveal_local(
        &self,
        _: &[u8],
        _: &[u8],
        _: &str,
    ) -> Result<CredentialLocalClaim, CredentialDisclosurePortError> {
        Err(CredentialDisclosurePortError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialRepository;
impl CredentialRepository for UnavailableCredentialRepository {
    fn upsert(&self, _: CredentialRecord) -> Result<(), CredentialRepositoryError> {
        Err(CredentialRepositoryError::Unavailable)
    }
    fn list(
        &self,
        _: &CredentialProfileId,
    ) -> Result<Vec<CredentialRecord>, CredentialRepositoryError> {
        Err(CredentialRepositoryError::Unavailable)
    }
    fn get(
        &self,
        _: &CredentialProfileId,
        _: &CredentialId,
    ) -> Result<CredentialRecord, CredentialRepositoryError> {
        Err(CredentialRepositoryError::Unavailable)
    }
    fn remove(
        &self,
        _: &CredentialProfileId,
        _: &CredentialId,
    ) -> Result<(), CredentialRepositoryError> {
        Err(CredentialRepositoryError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_credential_domain::{
        CredentialFormat, VerificationOutcome, VerificationStage, VerificationStageName,
        VerificationStageStatus,
    };
    use oxid_foundation::UnixTimestampMillis;
    use std::sync::RwLock;

    #[derive(Default)]
    struct Memory(RwLock<Vec<CredentialRecord>>);
    impl CredentialRepository for Memory {
        fn upsert(&self, record: CredentialRecord) -> Result<(), CredentialRepositoryError> {
            let mut records = self
                .0
                .write()
                .map_err(|_| CredentialRepositoryError::Unavailable)?;
            records
                .retain(|old| old.profile_id() != record.profile_id() || old.id() != record.id());
            records.push(record);
            Ok(())
        }
        fn list(
            &self,
            profile: &CredentialProfileId,
        ) -> Result<Vec<CredentialRecord>, CredentialRepositoryError> {
            Ok(self
                .0
                .read()
                .map_err(|_| CredentialRepositoryError::Unavailable)?
                .iter()
                .filter(|record| record.profile_id() == profile)
                .cloned()
                .collect())
        }
        fn get(
            &self,
            profile: &CredentialProfileId,
            id: &CredentialId,
        ) -> Result<CredentialRecord, CredentialRepositoryError> {
            self.list(profile)?
                .into_iter()
                .find(|record| record.id() == id)
                .ok_or(CredentialRepositoryError::NotFound)
        }
        fn remove(
            &self,
            profile: &CredentialProfileId,
            id: &CredentialId,
        ) -> Result<(), CredentialRepositoryError> {
            let mut records = self
                .0
                .write()
                .map_err(|_| CredentialRepositoryError::Unavailable)?;
            let before = records.len();
            records.retain(|record| record.profile_id() != profile || record.id() != id);
            (records.len() != before)
                .then_some(())
                .ok_or(CredentialRepositoryError::NotFound)
        }
    }
    struct Inbox;
    impl CredentialInboxPort for Inbox {
        fn receive<'a>(&'a self) -> CredentialBytesFuture<'a> {
            Box::pin(async { Ok(vec![1, 2, 3]) })
        }
    }
    struct Verifier;
    impl CredentialVerificationPort for Verifier {
        fn inspect<'a>(
            &'a self,
            _: &'a [u8],
            _: Option<&'a [u8]>,
        ) -> CredentialInspectionFuture<'a> {
            Box::pin(async {
                let stages = VerificationStageName::ALL
                    .into_iter()
                    .map(|name| {
                        VerificationStage::new(name, VerificationStageStatus::Passed, None)
                            .expect("stage")
                    })
                    .collect();
                Ok(CredentialInspection {
                id: CredentialId::parse("vc_fixture").expect("id"),
                metadata: CredentialMetadata::new("Fixture credential", "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", None, CredentialFormat::MidnightCborPhase1, Some(UnixTimestampMillis::new(7))).expect("metadata"),
                verification: VerificationReport::new(VerificationOutcome::Valid, stages).expect("report"),
            })
            })
        }
    }

    struct Disclosure;
    impl CredentialDisclosurePort for Disclosure {
        fn inspect(
            &self,
            signed_bytes: &[u8],
            private_material: &[u8],
        ) -> Result<CredentialDisclosureManifest, CredentialDisclosurePortError> {
            if signed_bytes != [1, 2, 3] || private_material != [9] {
                return Err(CredentialDisclosurePortError::InvalidPrivateMaterial);
            }
            CredentialDisclosureManifest::new(
                "fixture:v1",
                vec![
                    oxid_credential_domain::CredentialDisclosureCandidate::new(
                        "/credentialSubject/firstName",
                        "First name",
                        CredentialClaimPrivacy::SelectiveDisclosure,
                    )
                    .expect("first-name candidate"),
                    oxid_credential_domain::CredentialDisclosureCandidate::new(
                        "/credentialSubject/dateOfBirth",
                        "Age over threshold",
                        CredentialClaimPrivacy::PredicateOnly,
                    )
                    .expect("date-of-birth candidate"),
                ],
            )
            .map_err(|_| CredentialDisclosurePortError::InvalidPrivateMaterial)
        }

        fn reveal_local(
            &self,
            signed_bytes: &[u8],
            private_material: &[u8],
            claim_path: &str,
        ) -> Result<CredentialLocalClaim, CredentialDisclosurePortError> {
            self.inspect(signed_bytes, private_material)?;
            match claim_path {
                "/credentialSubject/firstName" => CredentialLocalClaim::new(claim_path, "Alice"),
                "/credentialSubject/dateOfBirth" => {
                    Err(CredentialDisclosurePortError::ClaimNotRevealable)
                }
                _ => Err(CredentialDisclosurePortError::ClaimNotFound),
            }
        }
    }

    #[test]
    fn receives_lists_and_requires_delete_confirmation() {
        let service = CredentialService::from_ports(
            Arc::new(Memory::default()),
            Arc::new(Inbox),
            Arc::new(Verifier),
            Arc::new(UnavailableCredentialDisclosure),
        );
        let profile = CredentialProfileQuery {
            profile_id: "profile_one".to_owned(),
        };
        let received =
            poll(ReceiveCredentialUseCase::execute(&service, profile.clone())).expect("receive");
        assert_eq!(received.verification_outcome, "valid");
        assert_eq!(
            ListCredentialsUseCase::execute(&service, profile)
                .expect("list")
                .len(),
            1
        );
        assert_eq!(
            DeleteCredentialUseCase::execute(
                &service,
                DeleteCredentialCommand {
                    profile_id: "profile_one".to_owned(),
                    credential_id: received.id,
                    confirmed: false,
                    intent: String::new()
                }
            ),
            Err(CredentialOperationError::ConfirmationRequired)
        );
    }

    #[test]
    fn validates_profile_scoped_disclosure_plans_and_targeted_local_reveal() {
        let repository = Arc::new(Memory::default());
        let service = CredentialService::from_ports(
            repository.clone(),
            Arc::new(Inbox),
            Arc::new(Verifier),
            Arc::new(Disclosure),
        );
        let imported = poll(ImportVerifiedCredentialUseCase::execute(
            &service,
            ImportVerifiedCredentialCommand {
                profile_id: "profile_one".to_owned(),
                signed_bytes: vec![1, 2, 3],
                detached_proof: Some(
                    CredentialDetachedProofInput::new(vec![7, 8]).expect("detached proof"),
                ),
                private_material: Some(
                    CredentialPrivateMaterialInput::new(vec![9]).expect("private material"),
                ),
            },
        ))
        .expect("verified import");
        let stored = repository
            .get(
                &CredentialProfileId::parse("profile_one").expect("profile"),
                &CredentialId::parse(imported.id.clone()).expect("credential"),
            )
            .expect("stored record");
        assert_eq!(
            stored
                .detached_proof()
                .map(CredentialDetachedProof::as_bytes),
            Some([7, 8].as_slice())
        );

        let disclosure = GetCredentialDisclosureUseCase::execute(
            &service,
            CredentialDisclosureQuery {
                profile_id: "profile_one".to_owned(),
                credential_id: imported.id.clone(),
            },
        )
        .expect("candidate inventory");
        assert_eq!(disclosure.schema_id, "fixture:v1");
        assert_eq!(disclosure.candidates.len(), 2);

        let plan = PreviewCredentialDisclosureUseCase::execute(
            &service,
            PreviewCredentialDisclosureCommand {
                profile_id: "profile_one".to_owned(),
                credential_id: imported.id.clone(),
                reveal_claim_paths: vec!["/credentialSubject/firstName".to_owned()],
                predicates: vec![CredentialPredicateInput {
                    claim_path: "/credentialSubject/dateOfBirth".to_owned(),
                    kind: "age_over".to_owned(),
                    threshold: 21,
                }],
            },
        )
        .expect("local preview");
        assert_eq!(plan.outcome, "local_preview_ready");
        assert!(!plan.presentation_generated);
        assert_eq!(plan.reveals.len(), 1);
        assert_eq!(plan.predicates.len(), 1);

        let local = RevealCredentialClaimUseCase::execute(
            &service,
            RevealCredentialClaimCommand {
                profile_id: "profile_one".to_owned(),
                credential_id: imported.id.clone(),
                claim_path: "/credentialSubject/firstName".to_owned(),
            },
        )
        .expect("explicit local reveal");
        assert_eq!(local.value(), "Alice");

        assert_eq!(
            GetCredentialDisclosureUseCase::execute(
                &service,
                CredentialDisclosureQuery {
                    profile_id: "profile_two".to_owned(),
                    credential_id: imported.id.clone(),
                },
            ),
            Err(CredentialOperationError::Persistence(
                CredentialRepositoryError::NotFound
            ))
        );
        assert_eq!(
            PreviewCredentialDisclosureUseCase::execute(
                &service,
                PreviewCredentialDisclosureCommand {
                    profile_id: "profile_one".to_owned(),
                    credential_id: imported.id.clone(),
                    reveal_claim_paths: vec![
                        "/credentialSubject/firstName".to_owned(),
                        "/credentialSubject/firstName".to_owned(),
                    ],
                    predicates: Vec::new(),
                },
            ),
            Err(CredentialOperationError::Disclosure(
                CredentialDisclosurePortError::ClaimNotFound
            ))
        );
        assert_eq!(
            RevealCredentialClaimUseCase::execute(
                &service,
                RevealCredentialClaimCommand {
                    profile_id: "profile_one".to_owned(),
                    credential_id: imported.id.clone(),
                    claim_path: "/credentialSubject/dateOfBirth".to_owned(),
                },
            ),
            Err(CredentialOperationError::Disclosure(
                CredentialDisclosurePortError::ClaimNotRevealable
            ))
        );

        poll(ReverifyCredentialUseCase::execute(
            &service,
            CredentialQuery {
                profile_id: "profile_one".to_owned(),
                credential_id: imported.id.clone(),
            },
        ))
        .expect("reverify protected record");
        DeleteCredentialUseCase::execute(
            &service,
            DeleteCredentialCommand {
                profile_id: "profile_one".to_owned(),
                credential_id: imported.id.clone(),
                confirmed: true,
                intent: "DELETE_CREDENTIAL".to_owned(),
            },
        )
        .expect("delete protected record");
        assert_eq!(
            GetCredentialDisclosureUseCase::execute(
                &service,
                CredentialDisclosureQuery {
                    profile_id: "profile_one".to_owned(),
                    credential_id: imported.id,
                },
            ),
            Err(CredentialOperationError::Persistence(
                CredentialRepositoryError::NotFound
            ))
        );
    }

    fn poll<T>(mut future: Pin<Box<dyn Future<Output = T> + Send + '_>>) -> T {
        use std::task::{Context, Poll, Waker};
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("fixture future must be ready"),
        }
    }
}
