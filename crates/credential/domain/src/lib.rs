// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{collections::BTreeSet, error::Error, fmt};

use oxid_foundation::{OpaqueId, OpaqueIdError, UnixTimestampMillis};

pub const MAX_SIGNED_CREDENTIAL_BYTES: usize = 1_048_576;
pub const MAX_CREDENTIAL_PRIVATE_MATERIAL_BYTES: usize = 262_144;
const MAX_LABEL_CHARACTERS: usize = 128;
const MAX_DID_CHARACTERS: usize = 8_192;
const MAX_SCHEMA_IDENTIFIER_CHARACTERS: usize = 256;
const MAX_CLAIM_PATH_CHARACTERS: usize = 512;
const MAX_DISCLOSURE_CANDIDATES: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialId(OpaqueId);

impl CredentialId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialProfileId(OpaqueId);

impl CredentialProfileId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Opaque, format-owned material delivered alongside a signed credential.
///
/// Examples include commitment openings used by a selectively disclosable
/// credential. Core code bounds and protects these bytes but never interprets
/// them. The custom `Debug` implementation deliberately reveals only length.
#[derive(Clone, PartialEq, Eq)]
pub struct CredentialPrivateMaterial(Vec<u8>);

impl CredentialPrivateMaterial {
    pub fn new(bytes: Vec<u8>) -> Result<Self, CredentialDomainError> {
        if bytes.is_empty() {
            return Err(CredentialDomainError::EmptyPrivateMaterial);
        }
        if bytes.len() > MAX_CREDENTIAL_PRIVATE_MATERIAL_BYTES {
            return Err(CredentialDomainError::PrivateMaterialTooLarge);
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for CredentialPrivateMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialPrivateMaterial")
            .field("length", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Privacy behavior supported by one credential claim.
///
/// These are schema-neutral wallet concepts. Format adapters translate their
/// own claim declarations into this deliberately small vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialClaimPrivacy {
    SelectiveDisclosure,
    PredicateOnly,
}

impl CredentialClaimPrivacy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SelectiveDisclosure => "selective_disclosure",
            Self::PredicateOnly => "predicate_only",
        }
    }
}

/// Public metadata for one claim that can participate in a local disclosure
/// preview. It intentionally carries neither the claim value nor its opening.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialDisclosureCandidate {
    path: String,
    label: String,
    privacy: CredentialClaimPrivacy,
}

impl CredentialDisclosureCandidate {
    pub fn new(
        path: impl Into<String>,
        label: impl Into<String>,
        privacy: CredentialClaimPrivacy,
    ) -> Result<Self, CredentialDomainError> {
        let path = path.into();
        validate_claim_path(&path)?;
        let label = label.into();
        validate_text(&label, MAX_LABEL_CHARACTERS)?;
        Ok(Self {
            path,
            label,
            privacy,
        })
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub const fn privacy(&self) -> CredentialClaimPrivacy {
        self.privacy
    }
}

/// Format-independent, claim-value-free description of the local disclosure
/// controls available for a credential.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialDisclosureManifest {
    schema_id: String,
    candidates: Vec<CredentialDisclosureCandidate>,
}

impl CredentialDisclosureManifest {
    pub fn new(
        schema_id: impl Into<String>,
        candidates: Vec<CredentialDisclosureCandidate>,
    ) -> Result<Self, CredentialDomainError> {
        let schema_id = schema_id.into();
        validate_text(&schema_id, MAX_SCHEMA_IDENTIFIER_CHARACTERS)?;
        if candidates.is_empty() || candidates.len() > MAX_DISCLOSURE_CANDIDATES {
            return Err(CredentialDomainError::InvalidDisclosureManifest);
        }
        let paths = candidates
            .iter()
            .map(CredentialDisclosureCandidate::path)
            .collect::<BTreeSet<_>>();
        if paths.len() != candidates.len() {
            return Err(CredentialDomainError::InvalidDisclosureManifest);
        }
        Ok(Self {
            schema_id,
            candidates,
        })
    }

    #[must_use]
    pub fn schema_id(&self) -> &str {
        &self.schema_id
    }

    #[must_use]
    pub fn candidates(&self) -> &[CredentialDisclosureCandidate] {
        &self.candidates
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialFormat {
    MidnightCborPhase1,
}

impl CredentialFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MidnightCborPhase1 => "midnight_cbor_phase1",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "midnight_cbor_phase1" => Some(Self::MidnightCborPhase1),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialMetadata {
    display_name: String,
    issuer_did: String,
    subject_did: Option<String>,
    format: CredentialFormat,
    issued_at: Option<UnixTimestampMillis>,
}

impl CredentialMetadata {
    pub fn new(
        display_name: impl Into<String>,
        issuer_did: impl Into<String>,
        subject_did: Option<String>,
        format: CredentialFormat,
        issued_at: Option<UnixTimestampMillis>,
    ) -> Result<Self, CredentialDomainError> {
        let display_name = display_name.into();
        validate_text(&display_name, MAX_LABEL_CHARACTERS)?;
        let issuer_did = issuer_did.into();
        validate_text(&issuer_did, MAX_DID_CHARACTERS)?;
        if let Some(subject) = subject_did.as_deref() {
            validate_text(subject, MAX_DID_CHARACTERS)?;
        }
        Ok(Self {
            display_name,
            issuer_did,
            subject_did,
            format,
            issued_at,
        })
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    #[must_use]
    pub fn issuer_did(&self) -> &str {
        &self.issuer_did
    }
    #[must_use]
    pub fn subject_did(&self) -> Option<&str> {
        self.subject_did.as_deref()
    }
    #[must_use]
    pub const fn format(&self) -> CredentialFormat {
        self.format
    }
    #[must_use]
    pub const fn issued_at(&self) -> Option<UnixTimestampMillis> {
        self.issued_at
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerificationStageName {
    Structural,
    Issuer,
    Proof,
    Temporal,
    Status,
    Schema,
    Trust,
}

impl VerificationStageName {
    pub const ALL: [Self; 7] = [
        Self::Structural,
        Self::Issuer,
        Self::Proof,
        Self::Temporal,
        Self::Status,
        Self::Schema,
        Self::Trust,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Structural => "structural",
            Self::Issuer => "issuer",
            Self::Proof => "proof",
            Self::Temporal => "temporal",
            Self::Status => "status",
            Self::Schema => "schema",
            Self::Trust => "trust",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "structural" => Some(Self::Structural),
            "issuer" => Some(Self::Issuer),
            "proof" => Some(Self::Proof),
            "temporal" => Some(Self::Temporal),
            "status" => Some(Self::Status),
            "schema" => Some(Self::Schema),
            "trust" => Some(Self::Trust),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationStageStatus {
    Passed,
    Failed,
    NotChecked,
}

impl VerificationStageStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::NotChecked => "not_checked",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "passed" => Some(Self::Passed),
            "failed" => Some(Self::Failed),
            "not_checked" => Some(Self::NotChecked),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationStage {
    name: VerificationStageName,
    status: VerificationStageStatus,
    reason_code: Option<String>,
}

impl VerificationStage {
    pub fn new(
        name: VerificationStageName,
        status: VerificationStageStatus,
        reason_code: Option<String>,
    ) -> Result<Self, CredentialDomainError> {
        if let Some(reason) = reason_code.as_deref() {
            validate_reason(reason)?;
        }
        if status == VerificationStageStatus::Failed && reason_code.is_none() {
            return Err(CredentialDomainError::MissingFailureReason);
        }
        if status != VerificationStageStatus::Failed && reason_code.is_some() {
            return Err(CredentialDomainError::UnexpectedFailureReason);
        }
        Ok(Self {
            name,
            status,
            reason_code,
        })
    }

    #[must_use]
    pub const fn name(&self) -> VerificationStageName {
        self.name
    }
    #[must_use]
    pub const fn status(&self) -> VerificationStageStatus {
        self.status
    }
    #[must_use]
    pub fn reason_code(&self) -> Option<&str> {
        self.reason_code.as_deref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationOutcome {
    Valid,
    Invalid,
    Error,
}

impl VerificationOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Error => "error",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "valid" => Some(Self::Valid),
            "invalid" => Some(Self::Invalid),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationReport {
    outcome: VerificationOutcome,
    stages: Vec<VerificationStage>,
}

impl VerificationReport {
    pub fn new(
        outcome: VerificationOutcome,
        stages: Vec<VerificationStage>,
    ) -> Result<Self, CredentialDomainError> {
        let names = stages
            .iter()
            .map(VerificationStage::name)
            .collect::<BTreeSet<_>>();
        if stages.len() != VerificationStageName::ALL.len()
            || names.len() != VerificationStageName::ALL.len()
            || !VerificationStageName::ALL
                .iter()
                .all(|name| names.contains(name))
        {
            return Err(CredentialDomainError::IncompleteVerificationReport);
        }
        let failed = stages
            .iter()
            .any(|stage| stage.status() == VerificationStageStatus::Failed);
        if (outcome == VerificationOutcome::Valid && failed)
            || (outcome == VerificationOutcome::Invalid && !failed)
        {
            return Err(CredentialDomainError::InconsistentVerificationOutcome);
        }
        Ok(Self { outcome, stages })
    }

    #[must_use]
    pub const fn outcome(&self) -> VerificationOutcome {
        self.outcome
    }
    #[must_use]
    pub fn stages(&self) -> &[VerificationStage] {
        &self.stages
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct CredentialRecord {
    profile_id: CredentialProfileId,
    id: CredentialId,
    signed_bytes: Vec<u8>,
    private_material: Option<CredentialPrivateMaterial>,
    metadata: CredentialMetadata,
    verification: VerificationReport,
}

impl fmt::Debug for CredentialRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialRecord")
            .field("profile_id", &self.profile_id)
            .field("id", &self.id)
            .field("signed_bytes_length", &self.signed_bytes.len())
            .field(
                "private_material_length",
                &self
                    .private_material
                    .as_ref()
                    .map(|material| material.as_bytes().len()),
            )
            .field("metadata", &self.metadata)
            .field("verification", &self.verification)
            .finish_non_exhaustive()
    }
}

impl CredentialRecord {
    pub fn new(
        profile_id: CredentialProfileId,
        id: CredentialId,
        signed_bytes: Vec<u8>,
        metadata: CredentialMetadata,
        verification: VerificationReport,
    ) -> Result<Self, CredentialDomainError> {
        Self::new_with_private_material(profile_id, id, signed_bytes, None, metadata, verification)
    }

    pub fn new_with_private_material(
        profile_id: CredentialProfileId,
        id: CredentialId,
        signed_bytes: Vec<u8>,
        private_material: Option<CredentialPrivateMaterial>,
        metadata: CredentialMetadata,
        verification: VerificationReport,
    ) -> Result<Self, CredentialDomainError> {
        if signed_bytes.is_empty() {
            return Err(CredentialDomainError::EmptySignedCredential);
        }
        if signed_bytes.len() > MAX_SIGNED_CREDENTIAL_BYTES {
            return Err(CredentialDomainError::SignedCredentialTooLarge);
        }
        Ok(Self {
            profile_id,
            id,
            signed_bytes,
            private_material,
            metadata,
            verification,
        })
    }

    #[must_use]
    pub fn profile_id(&self) -> &CredentialProfileId {
        &self.profile_id
    }
    #[must_use]
    pub fn id(&self) -> &CredentialId {
        &self.id
    }
    #[must_use]
    pub fn signed_bytes(&self) -> &[u8] {
        &self.signed_bytes
    }
    #[must_use]
    pub fn private_material(&self) -> Option<&CredentialPrivateMaterial> {
        self.private_material.as_ref()
    }
    #[must_use]
    pub fn metadata(&self) -> &CredentialMetadata {
        &self.metadata
    }
    #[must_use]
    pub fn verification(&self) -> &VerificationReport {
        &self.verification
    }
    pub fn replace_inspection(
        &mut self,
        id: CredentialId,
        metadata: CredentialMetadata,
        verification: VerificationReport,
    ) -> Result<(), CredentialDomainError> {
        if id != self.id {
            return Err(CredentialDomainError::CredentialIdentifierChanged);
        }
        self.metadata = metadata;
        self.verification = verification;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialDomainError {
    EmptyText,
    TextTooLong,
    InvalidText,
    InvalidReasonCode,
    MissingFailureReason,
    UnexpectedFailureReason,
    IncompleteVerificationReport,
    InconsistentVerificationOutcome,
    EmptySignedCredential,
    SignedCredentialTooLarge,
    EmptyPrivateMaterial,
    PrivateMaterialTooLarge,
    InvalidClaimPath,
    InvalidDisclosureManifest,
    CredentialIdentifierChanged,
}

impl fmt::Display for CredentialDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyText => "credential metadata text must not be empty",
            Self::TextTooLong => "credential metadata text exceeds its limit",
            Self::InvalidText => "credential metadata text contains disallowed characters",
            Self::InvalidReasonCode => "verification reason code is invalid",
            Self::MissingFailureReason => "failed verification stage requires a reason code",
            Self::UnexpectedFailureReason => {
                "non-failed verification stage cannot have a reason code"
            }
            Self::IncompleteVerificationReport => {
                "verification report must contain every stage exactly once"
            }
            Self::InconsistentVerificationOutcome => {
                "verification outcome conflicts with its stages"
            }
            Self::EmptySignedCredential => "signed credential must not be empty",
            Self::SignedCredentialTooLarge => "signed credential exceeds the size limit",
            Self::EmptyPrivateMaterial => "credential private material must not be empty",
            Self::PrivateMaterialTooLarge => "credential private material exceeds the size limit",
            Self::InvalidClaimPath => "credential disclosure claim path is invalid",
            Self::InvalidDisclosureManifest => "credential disclosure manifest is invalid",
            Self::CredentialIdentifierChanged => {
                "credential identifier changed during verification"
            }
        })
    }
}

fn validate_claim_path(value: &str) -> Result<(), CredentialDomainError> {
    if value.len() < 2
        || value.len() > MAX_CLAIM_PATH_CHARACTERS
        || !value.starts_with('/')
        || value.ends_with('/')
        || value.contains("//")
        || value.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '\u{202a}'..='\u{202e}')
        })
    {
        return Err(CredentialDomainError::InvalidClaimPath);
    }
    Ok(())
}

impl Error for CredentialDomainError {}

fn validate_text(value: &str, maximum: usize) -> Result<(), CredentialDomainError> {
    if value.is_empty() {
        return Err(CredentialDomainError::EmptyText);
    }
    if value.chars().count() > maximum {
        return Err(CredentialDomainError::TextTooLong);
    }
    if value.trim() != value
        || value.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '\u{202a}'..='\u{202e}')
        })
    {
        return Err(CredentialDomainError::InvalidText);
    }
    Ok(())
}

fn validate_reason(value: &str) -> Result<(), CredentialDomainError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(CredentialDomainError::InvalidReasonCode);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stages(status: VerificationStageStatus) -> Vec<VerificationStage> {
        VerificationStageName::ALL
            .into_iter()
            .map(|name| {
                VerificationStage::new(
                    name,
                    status,
                    (status == VerificationStageStatus::Failed).then(|| "invalid".to_owned()),
                )
                .expect("stage")
            })
            .collect()
    }

    #[test]
    fn requires_a_complete_structured_report() {
        assert_eq!(
            VerificationReport::new(VerificationOutcome::Valid, Vec::new()),
            Err(CredentialDomainError::IncompleteVerificationReport)
        );
        assert!(
            VerificationReport::new(
                VerificationOutcome::Valid,
                stages(VerificationStageStatus::Passed)
            )
            .is_ok()
        );
    }

    #[test]
    fn keeps_original_signed_bytes_outside_normalized_metadata() {
        let report = VerificationReport::new(
            VerificationOutcome::Valid,
            stages(VerificationStageStatus::Passed),
        )
        .expect("report");
        let record = CredentialRecord::new(
            CredentialProfileId::parse("profile_one").expect("profile"),
            CredentialId::parse("vc_one").expect("id"),
            vec![0xa1, 0x61, b'a', 0x01],
            CredentialMetadata::new(
                "Identity credential",
                "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                None,
                CredentialFormat::MidnightCborPhase1,
                None,
            )
            .expect("metadata"),
            report,
        )
        .expect("record");
        assert_eq!(record.signed_bytes(), &[0xa1, 0x61, b'a', 0x01]);
        assert_eq!(record.metadata().display_name(), "Identity credential");
    }

    #[test]
    fn bounds_and_redacts_format_private_material() {
        assert_eq!(
            CredentialPrivateMaterial::new(Vec::new()),
            Err(CredentialDomainError::EmptyPrivateMaterial)
        );
        assert_eq!(
            CredentialPrivateMaterial::new(vec![0; MAX_CREDENTIAL_PRIVATE_MATERIAL_BYTES + 1]),
            Err(CredentialDomainError::PrivateMaterialTooLarge)
        );
        let material = CredentialPrivateMaterial::new(b"claim-secret".to_vec()).expect("material");
        let debug = format!("{material:?}");
        assert!(debug.contains("length"));
        assert!(!debug.contains("claim-secret"));

        let report = VerificationReport::new(
            VerificationOutcome::Valid,
            stages(VerificationStageStatus::Passed),
        )
        .expect("report");
        let record = CredentialRecord::new_with_private_material(
            CredentialProfileId::parse("profile_one").expect("profile"),
            CredentialId::parse("vc_one").expect("id"),
            b"signed-secret".to_vec(),
            Some(material),
            CredentialMetadata::new(
                "Identity credential",
                "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                None,
                CredentialFormat::MidnightCborPhase1,
                None,
            )
            .expect("metadata"),
            report,
        )
        .expect("record");
        let debug = format!("{record:?}");
        assert!(!debug.contains("signed-secret"));
        assert!(!debug.contains("claim-secret"));
    }

    #[test]
    fn disclosure_manifest_is_public_metadata_with_unique_paths() {
        let first = CredentialDisclosureCandidate::new(
            "/credentialSubject/firstName",
            "First name",
            CredentialClaimPrivacy::SelectiveDisclosure,
        )
        .expect("candidate");
        let date_of_birth = CredentialDisclosureCandidate::new(
            "/credentialSubject/dateOfBirth",
            "Age over threshold",
            CredentialClaimPrivacy::PredicateOnly,
        )
        .expect("candidate");
        let manifest = CredentialDisclosureManifest::new(
            "digital-passport:v1",
            vec![first.clone(), date_of_birth],
        )
        .expect("manifest");
        assert_eq!(manifest.schema_id(), "digital-passport:v1");
        assert_eq!(
            manifest.candidates()[0].privacy().as_str(),
            "selective_disclosure"
        );
        assert_eq!(
            CredentialDisclosureManifest::new("digital-passport:v1", vec![first.clone(), first]),
            Err(CredentialDomainError::InvalidDisclosureManifest)
        );
        assert_eq!(
            CredentialDisclosureCandidate::new(
                "credentialSubject/firstName",
                "First name",
                CredentialClaimPrivacy::SelectiveDisclosure
            ),
            Err(CredentialDomainError::InvalidClaimPath)
        );
    }
}
