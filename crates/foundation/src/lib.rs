// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt};

/// A validated identifier whose representation is owned by Oxid.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpaqueId(String);

impl OpaqueId {
    /// Parses a non-empty, whitespace-free identifier up to 128 characters.
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(OpaqueIdError::Empty);
        }
        if value.chars().count() > 128 {
            return Err(OpaqueIdError::TooLong);
        }
        if value.chars().any(char::is_whitespace) {
            return Err(OpaqueIdError::ContainsWhitespace);
        }
        if value.chars().any(char::is_control) {
            return Err(OpaqueIdError::ContainsControlCharacter);
        }

        Ok(Self(value))
    }

    /// Returns the stable string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OpaqueId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Validation failures for [`OpaqueId`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpaqueIdError {
    Empty,
    TooLong,
    ContainsWhitespace,
    ContainsControlCharacter,
}

impl fmt::Display for OpaqueIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "identifier must not be empty",
            Self::TooLong => "identifier must not exceed 128 characters",
            Self::ContainsWhitespace => "identifier must not contain whitespace",
            Self::ContainsControlCharacter => "identifier must not contain control characters",
        };
        formatter.write_str(message)
    }
}

impl Error for OpaqueIdError {}

/// Milliseconds since the Unix epoch, represented without a date-time SDK type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnixTimestampMillis(u64);

impl UnixTimestampMillis {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_id_accepts_a_stable_application_identifier() {
        let identifier = OpaqueId::parse("profile_42").expect("identifier should be valid");

        assert_eq!(identifier.as_str(), "profile_42");
    }

    #[test]
    fn opaque_id_rejects_whitespace() {
        assert_eq!(
            OpaqueId::parse("profile 42"),
            Err(OpaqueIdError::ContainsWhitespace)
        );
    }
}
