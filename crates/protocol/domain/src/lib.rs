// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{collections::BTreeSet, error::Error, fmt};

use oxid_foundation::opaque_id_type;

const MAX_ISSUER_CHARACTERS: usize = 2_048;
const MAX_CONFIGURATION_CHARACTERS: usize = 256;
const MAX_CONFIGURATION_COUNT: usize = 16;
const MAX_VERIFIER_CHARACTERS: usize = 2_048;
const MAX_PURPOSE_CHARACTERS: usize = 512;

opaque_id_type! {
    pub struct CredentialIssuanceId;
}

opaque_id_type! {
    pub struct ProtocolProfileId;
}

opaque_id_type! {
    pub struct SelfIssuedAuthenticationId;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfIssuedAuthenticationPreview {
    verifier: String,
    purpose: String,
}

impl SelfIssuedAuthenticationPreview {
    pub fn new(
        verifier: impl Into<String>,
        purpose: impl Into<String>,
    ) -> Result<Self, ProtocolDomainError> {
        let verifier = verifier.into();
        let purpose = purpose.into();
        validate_text(&verifier, MAX_VERIFIER_CHARACTERS)?;
        validate_text(&purpose, MAX_PURPOSE_CHARACTERS)?;
        Ok(Self { verifier, purpose })
    }

    #[must_use]
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    #[must_use]
    pub fn purpose(&self) -> &str {
        &self.purpose
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfIssuedAuthenticationState {
    AwaitingConsent,
    Authenticating,
    Succeeded,
    Refused,
    Failed,
}

impl SelfIssuedAuthenticationState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingConsent => "awaiting_consent",
            Self::Authenticating => "authenticating",
            Self::Succeeded => "succeeded",
            Self::Refused => "refused",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialOfferPreview {
    issuer: String,
    configuration_ids: Vec<String>,
    display_names: Vec<String>,
}

impl CredentialOfferPreview {
    pub fn new(
        issuer: impl Into<String>,
        configuration_ids: Vec<String>,
        display_names: Vec<String>,
    ) -> Result<Self, ProtocolDomainError> {
        let issuer = issuer.into();
        validate_text(&issuer, MAX_ISSUER_CHARACTERS)?;
        if configuration_ids.is_empty()
            || configuration_ids.len() > MAX_CONFIGURATION_COUNT
            || display_names.len() != configuration_ids.len()
        {
            return Err(ProtocolDomainError::InvalidConfigurations);
        }
        let mut unique = BTreeSet::new();
        for value in configuration_ids.iter().chain(display_names.iter()) {
            validate_text(value, MAX_CONFIGURATION_CHARACTERS)?;
        }
        if !configuration_ids.iter().all(|value| unique.insert(value)) {
            return Err(ProtocolDomainError::InvalidConfigurations);
        }
        Ok(Self {
            issuer,
            configuration_ids,
            display_names,
        })
    }

    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    #[must_use]
    pub fn configuration_ids(&self) -> &[String] {
        &self.configuration_ids
    }

    #[must_use]
    pub fn display_names(&self) -> &[String] {
        &self.display_names
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialIssuanceState {
    AwaitingConsent,
    Issuing,
    Succeeded,
    Refused,
    Failed,
}

impl CredentialIssuanceState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingConsent => "awaiting_consent",
            Self::Issuing => "issuing",
            Self::Succeeded => "succeeded",
            Self::Refused => "refused",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolDomainError {
    EmptyText,
    TextTooLong,
    ControlCharacter,
    InvalidConfigurations,
}

impl fmt::Display for ProtocolDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyText => "protocol text must not be empty",
            Self::TextTooLong => "protocol text exceeds its bounded length",
            Self::ControlCharacter => "protocol text must not contain control characters",
            Self::InvalidConfigurations => "credential configurations are invalid",
        })
    }
}

impl Error for ProtocolDomainError {}

fn validate_text(value: &str, max: usize) -> Result<(), ProtocolDomainError> {
    if value.trim().is_empty() {
        return Err(ProtocolDomainError::EmptyText);
    }
    if value.chars().count() > max {
        return Err(ProtocolDomainError::TextTooLong);
    }
    if value.chars().any(char::is_control) {
        return Err(ProtocolDomainError::ControlCharacter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_preview_requires_unique_bounded_configurations() {
        let preview = CredentialOfferPreview::new(
            "https://issuer.example",
            vec!["identity".to_owned()],
            vec!["Identity credential".to_owned()],
        )
        .expect("preview should be valid");
        assert_eq!(preview.issuer(), "https://issuer.example");
        assert_eq!(preview.configuration_ids(), ["identity"]);

        assert_eq!(
            CredentialOfferPreview::new(
                "https://issuer.example",
                vec!["identity".to_owned(), "identity".to_owned()],
                vec!["Identity".to_owned(), "Identity".to_owned()],
            ),
            Err(ProtocolDomainError::InvalidConfigurations)
        );
    }

    #[test]
    fn issuance_states_have_stable_names() {
        assert_eq!(
            CredentialIssuanceState::AwaitingConsent.as_str(),
            "awaiting_consent"
        );
        assert_eq!(CredentialIssuanceState::Succeeded.as_str(), "succeeded");
    }

    #[test]
    fn authentication_preview_and_states_are_bounded() {
        let preview = SelfIssuedAuthenticationPreview::new(
            "https://verifier.example",
            "Authenticate with the selected DID.",
        )
        .expect("preview should be valid");
        assert_eq!(preview.verifier(), "https://verifier.example");
        assert_eq!(
            SelfIssuedAuthenticationState::AwaitingConsent.as_str(),
            "awaiting_consent"
        );
        assert_eq!(
            SelfIssuedAuthenticationPreview::new("verifier", "\n"),
            Err(ProtocolDomainError::EmptyText)
        );
    }
}
