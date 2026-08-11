// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt};

use oxid_foundation::{OpaqueId, OpaqueIdError, UnixTimestampMillis};

/// Stable identifier for one wallet profile.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WalletProfileId(OpaqueId);

impl WalletProfileId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for WalletProfileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// User-facing profile label after domain normalization and validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileName(String);

impl ProfileName {
    pub const MAX_CHARACTERS: usize = 64;

    pub fn parse(value: impl AsRef<str>) -> Result<Self, ProfileNameError> {
        let normalized = value.as_ref().trim();
        if normalized.is_empty() {
            return Err(ProfileNameError::Empty);
        }
        if normalized.chars().count() > Self::MAX_CHARACTERS {
            return Err(ProfileNameError::TooLong);
        }
        if normalized.chars().any(char::is_control) {
            return Err(ProfileNameError::ContainsControlCharacter);
        }

        Ok(Self(normalized.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validation failures for a wallet profile label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileNameError {
    Empty,
    TooLong,
    ContainsControlCharacter,
}

impl fmt::Display for ProfileNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "profile name must not be empty",
            Self::TooLong => "profile name must not exceed 64 characters",
            Self::ContainsControlCharacter => "profile name must not contain control characters",
        };
        formatter.write_str(message)
    }
}

impl Error for ProfileNameError {}

/// A user-controlled wallet profile. It intentionally contains no key material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletProfile {
    id: WalletProfileId,
    display_name: ProfileName,
    created_at: UnixTimestampMillis,
}

impl WalletProfile {
    #[must_use]
    pub const fn new(
        id: WalletProfileId,
        display_name: ProfileName,
        created_at: UnixTimestampMillis,
    ) -> Self {
        Self {
            id,
            display_name,
            created_at,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &WalletProfileId {
        &self.id
    }

    #[must_use]
    pub const fn display_name(&self) -> &ProfileName {
        &self.display_name
    }

    #[must_use]
    pub const fn created_at(&self) -> UnixTimestampMillis {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_name_is_trimmed_at_the_domain_boundary() {
        let name = ProfileName::parse("  Primary wallet  ").expect("name should be valid");

        assert_eq!(name.as_str(), "Primary wallet");
    }

    #[test]
    fn profile_name_rejects_control_characters() {
        assert_eq!(
            ProfileName::parse("Primary\nwallet"),
            Err(ProfileNameError::ContainsControlCharacter)
        );
    }

    #[test]
    fn profile_contains_only_public_profile_metadata() {
        let profile = WalletProfile::new(
            WalletProfileId::parse("profile_1").expect("identifier should be valid"),
            ProfileName::parse("Primary").expect("name should be valid"),
            UnixTimestampMillis::new(42),
        );

        assert_eq!(profile.id().as_str(), "profile_1");
        assert_eq!(profile.display_name().as_str(), "Primary");
        assert_eq!(profile.created_at().value(), 42);
    }
}
