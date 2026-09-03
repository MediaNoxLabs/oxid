// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt};

/// Declares a domain-specific newtype backed by [`OpaqueId`].
///
/// The generated type preserves the deliberately small `OpaqueId` surface: it
/// derives the standard value traits and exposes only `parse` and `as_str`.
/// `Display` remains opt-in so adding a new identifier does not make it
/// printable accidentally.
///
/// ```
/// use oxid_foundation::opaque_id_type;
///
/// opaque_id_type! {
///     /// Identifier used by this example boundary.
///     pub struct ExampleId;
///     display;
/// }
///
/// let identifier = ExampleId::parse("example_42").expect("valid identifier");
/// assert_eq!(identifier.as_str(), "example_42");
/// assert_eq!(identifier.to_string(), "example_42");
/// ```
#[macro_export]
macro_rules! opaque_id_type {
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident;
        display;
    ) => {
        $crate::opaque_id_type! {
            @define
            $(#[$attribute])*
            $visibility struct $name;
        }
        $crate::opaque_id_type!(@display $name);
    };
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident;
    ) => {
        $crate::opaque_id_type! {
            @define
            $(#[$attribute])*
            $visibility struct $name;
        }
    };
    (
        @define
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident;
    ) => {
        $(#[$attribute])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $visibility struct $name($crate::OpaqueId);

        impl $name {
            pub fn parse(
                value: impl ::core::convert::Into<::std::string::String>,
            ) -> ::core::result::Result<Self, $crate::OpaqueIdError> {
                $crate::OpaqueId::parse(value).map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }
    };
    (@display $name:ident) => {
        impl ::core::fmt::Display for $name {
            fn fmt(
                &self,
                formatter: &mut ::core::fmt::Formatter<'_>,
            ) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(&self.0, formatter)
            }
        }
    };
}

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

    crate::opaque_id_type! {
        #[allow(dead_code)]
        pub(crate) struct MacroOpaqueId;
    }

    crate::opaque_id_type! {
        pub(crate) struct DisplayMacroOpaqueId;
        display;
    }

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

    #[test]
    fn opaque_id_macro_preserves_validation_and_value_traits() {
        let identifier = MacroOpaqueId::parse("macro_42").expect("identifier should be valid");
        let clone = identifier.clone();

        assert_eq!(identifier, clone);
        assert_eq!(identifier.as_str(), "macro_42");
        assert_eq!(MacroOpaqueId::parse(""), Err(OpaqueIdError::Empty));
        assert_eq!(
            MacroOpaqueId::parse("x".repeat(129)),
            Err(OpaqueIdError::TooLong)
        );
        assert_eq!(
            MacroOpaqueId::parse("macro 42"),
            Err(OpaqueIdError::ContainsWhitespace)
        );
        assert_eq!(
            MacroOpaqueId::parse("macro\0"),
            Err(OpaqueIdError::ContainsControlCharacter)
        );

        let mut ordered = std::collections::BTreeSet::new();
        ordered.insert(identifier.clone());
        assert!(ordered.contains(&identifier));

        let mut hashed = std::collections::HashSet::new();
        hashed.insert(identifier.clone());
        assert!(hashed.contains(&identifier));
        assert!(format!("{identifier:?}").contains("macro_42"));
    }

    #[test]
    fn opaque_id_macro_adds_display_only_when_requested() {
        let identifier =
            DisplayMacroOpaqueId::parse("display_42").expect("identifier should be valid");

        assert_eq!(identifier.to_string(), "display_42");
        assert_eq!(identifier.as_str(), "display_42");
    }
}
