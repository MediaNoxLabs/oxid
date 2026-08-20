// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{collections::BTreeSet, error::Error, fmt};

use oxid_foundation::{OpaqueId, OpaqueIdError};

const MAX_TEXT_CHARACTERS: usize = 2_048;
const MAX_CLAIM_PATH_CHARACTERS: usize = 512;
const MAX_REQUESTED_CLAIMS: usize = 64;
const MAX_CANDIDATES: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialPresentationId(OpaqueId);

impl CredentialPresentationId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresentationProfileId(OpaqueId);

impl PresentationProfileId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationClaimIntent {
    Reveal,
    Predicate,
}

impl PresentationClaimIntent {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reveal => "reveal",
            Self::Predicate => "predicate",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestedPresentationClaim {
    path: String,
    label: String,
    intent: PresentationClaimIntent,
    predicate_kind: Option<String>,
    threshold: Option<u8>,
}

impl RequestedPresentationClaim {
    pub fn reveal(
        path: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<Self, PresentationDomainError> {
        Self::new(path, label, PresentationClaimIntent::Reveal, None, None)
    }

    pub fn predicate(
        path: impl Into<String>,
        label: impl Into<String>,
        kind: impl Into<String>,
        threshold: u8,
    ) -> Result<Self, PresentationDomainError> {
        Self::new(
            path,
            label,
            PresentationClaimIntent::Predicate,
            Some(kind.into()),
            Some(threshold),
        )
    }

    fn new(
        path: impl Into<String>,
        label: impl Into<String>,
        intent: PresentationClaimIntent,
        predicate_kind: Option<String>,
        threshold: Option<u8>,
    ) -> Result<Self, PresentationDomainError> {
        let path = path.into();
        validate_path(&path)?;
        let label = label.into();
        validate_text(&label)?;
        match (intent, predicate_kind.as_deref(), threshold) {
            (PresentationClaimIntent::Reveal, None, None) => {}
            (PresentationClaimIntent::Predicate, Some(kind), Some(1..=120)) => {
                validate_text(kind)?;
            }
            _ => return Err(PresentationDomainError::InvalidClaim),
        }
        Ok(Self {
            path,
            label,
            intent,
            predicate_kind,
            threshold,
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
    pub const fn intent(&self) -> PresentationClaimIntent {
        self.intent
    }

    #[must_use]
    pub fn predicate_kind(&self) -> Option<&str> {
        self.predicate_kind.as_deref()
    }

    #[must_use]
    pub const fn threshold(&self) -> Option<u8> {
        self.threshold
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationCredentialCandidate {
    credential_id: String,
    display_name: String,
    issuer: String,
}

impl PresentationCredentialCandidate {
    pub fn new(
        credential_id: impl Into<String>,
        display_name: impl Into<String>,
        issuer: impl Into<String>,
    ) -> Result<Self, PresentationDomainError> {
        let credential_id = credential_id.into();
        OpaqueId::parse(credential_id.clone())
            .map_err(|_| PresentationDomainError::InvalidCandidate)?;
        let display_name = display_name.into();
        validate_text(&display_name)?;
        let issuer = issuer.into();
        validate_text(&issuer)?;
        Ok(Self {
            credential_id,
            display_name,
            issuer,
        })
    }

    #[must_use]
    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialPresentationPreview {
    verifier: String,
    purpose: String,
    query_id: String,
    candidates: Vec<PresentationCredentialCandidate>,
    requested_claims: Vec<RequestedPresentationClaim>,
}

impl CredentialPresentationPreview {
    pub fn new(
        verifier: impl Into<String>,
        purpose: impl Into<String>,
        query_id: impl Into<String>,
        candidates: Vec<PresentationCredentialCandidate>,
        requested_claims: Vec<RequestedPresentationClaim>,
    ) -> Result<Self, PresentationDomainError> {
        let verifier = verifier.into();
        let purpose = purpose.into();
        let query_id = query_id.into();
        validate_text(&verifier)?;
        validate_text(&purpose)?;
        validate_text(&query_id)?;
        if candidates.is_empty()
            || candidates.len() > MAX_CANDIDATES
            || requested_claims.is_empty()
            || requested_claims.len() > MAX_REQUESTED_CLAIMS
        {
            return Err(PresentationDomainError::InvalidPreview);
        }
        let candidate_ids = candidates
            .iter()
            .map(PresentationCredentialCandidate::credential_id)
            .collect::<BTreeSet<_>>();
        let paths = requested_claims
            .iter()
            .map(RequestedPresentationClaim::path)
            .collect::<BTreeSet<_>>();
        if candidate_ids.len() != candidates.len() || paths.len() != requested_claims.len() {
            return Err(PresentationDomainError::InvalidPreview);
        }
        Ok(Self {
            verifier,
            purpose,
            query_id,
            candidates,
            requested_claims,
        })
    }

    #[must_use]
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    #[must_use]
    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    #[must_use]
    pub fn query_id(&self) -> &str {
        &self.query_id
    }

    #[must_use]
    pub fn candidates(&self) -> &[PresentationCredentialCandidate] {
        &self.candidates
    }

    #[must_use]
    pub fn requested_claims(&self) -> &[RequestedPresentationClaim] {
        &self.requested_claims
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialPresentationState {
    AwaitingConsent,
    Presenting,
    CancellationRequested,
    Cancelled,
    TimedOut,
    Succeeded,
    Refused,
    Failed,
}

impl CredentialPresentationState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingConsent => "awaiting_consent",
            Self::Presenting => "presenting",
            Self::CancellationRequested => "cancellation_requested",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Succeeded => "succeeded",
            Self::Refused => "refused",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationDomainError {
    EmptyText,
    TextTooLong,
    ControlCharacter,
    InvalidClaim,
    InvalidCandidate,
    InvalidPreview,
}

impl fmt::Display for PresentationDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyText => "presentation text must not be empty",
            Self::TextTooLong => "presentation text exceeds its bounded length",
            Self::ControlCharacter => "presentation text contains a forbidden character",
            Self::InvalidClaim => "presentation claim request is invalid",
            Self::InvalidCandidate => "presentation candidate is invalid",
            Self::InvalidPreview => "presentation preview is invalid",
        })
    }
}

impl Error for PresentationDomainError {}

fn validate_text(value: &str) -> Result<(), PresentationDomainError> {
    if value.trim().is_empty() {
        return Err(PresentationDomainError::EmptyText);
    }
    if value.chars().count() > MAX_TEXT_CHARACTERS {
        return Err(PresentationDomainError::TextTooLong);
    }
    if value.chars().any(|character| {
        character.is_control() || matches!(character, '<' | '>' | '\u{202a}'..='\u{202e}')
    }) {
        return Err(PresentationDomainError::ControlCharacter);
    }
    Ok(())
}

fn validate_path(value: &str) -> Result<(), PresentationDomainError> {
    if value.len() < 2
        || value.len() > MAX_CLAIM_PATH_CHARACTERS
        || !value.starts_with('/')
        || value.ends_with('/')
        || value.contains("//")
    {
        return Err(PresentationDomainError::InvalidClaim);
    }
    validate_text(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_is_bounded_unique_and_claim_value_free() {
        let first =
            RequestedPresentationClaim::reveal("/credentialSubject/firstName", "First name")
                .expect("claim");
        let age = RequestedPresentationClaim::predicate(
            "/credentialSubject/dateOfBirth",
            "Age over 18",
            "age_over",
            18,
        )
        .expect("predicate");
        let candidate = PresentationCredentialCandidate::new(
            "vc_one",
            "Digital Passport",
            "did:midnight:undeployed:issuer",
        )
        .expect("candidate");
        let preview = CredentialPresentationPreview::new(
            "https://verifier.example",
            "Prove identity and age.",
            "digital_passport",
            vec![candidate],
            vec![first.clone(), age],
        )
        .expect("preview");
        assert_eq!(preview.requested_claims()[1].threshold(), Some(18));
        assert_eq!(preview.candidates()[0].credential_id(), "vc_one");
        assert_eq!(
            preview.candidates()[0].issuer(),
            "did:midnight:undeployed:issuer"
        );
        assert_eq!(
            CredentialPresentationPreview::new(
                "https://verifier.example",
                "Purpose",
                "query",
                vec![
                    PresentationCredentialCandidate::new(
                        "vc_one",
                        "Passport",
                        "did:midnight:undeployed:issuer",
                    )
                    .expect("candidate")
                ],
                vec![first.clone(), first],
            ),
            Err(PresentationDomainError::InvalidPreview)
        );
    }
}
