// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{collections::BTreeSet, error::Error, fmt};

use oxid_foundation::{OpaqueId, OpaqueIdError, UnixTimestampMillis};

pub const MAX_SIGNED_CREDENTIAL_BYTES: usize = 1_048_576;
const MAX_LABEL_CHARACTERS: usize = 128;
const MAX_DID_CHARACTERS: usize = 8_192;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialRecord {
    profile_id: CredentialProfileId,
    id: CredentialId,
    signed_bytes: Vec<u8>,
    metadata: CredentialMetadata,
    verification: VerificationReport,
}

impl CredentialRecord {
    pub fn new(
        profile_id: CredentialProfileId,
        id: CredentialId,
        signed_bytes: Vec<u8>,
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
            Self::CredentialIdentifierChanged => {
                "credential identifier changed during verification"
            }
        })
    }
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
}
