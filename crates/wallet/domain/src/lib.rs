// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt};

use oxid_foundation::{OpaqueId, OpaqueIdError, UnixTimestampMillis};

mod chain;

pub use chain::*;

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

/// Opaque handle for a protected key. The value never contains key material.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WalletKeyReference(OpaqueId);

impl WalletKeyReference {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for WalletKeyReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// User-facing public label for a protected key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletKeyLabel(String);

impl WalletKeyLabel {
    pub const MAX_CHARACTERS: usize = 96;

    pub fn parse(value: impl AsRef<str>) -> Result<Self, WalletKeyLabelError> {
        let normalized = value.as_ref().trim();
        if normalized.is_empty() {
            return Err(WalletKeyLabelError::Empty);
        }
        if normalized.chars().count() > Self::MAX_CHARACTERS {
            return Err(WalletKeyLabelError::TooLong);
        }
        if normalized.chars().any(char::is_control) {
            return Err(WalletKeyLabelError::ContainsControlCharacter);
        }

        Ok(Self(normalized.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validation failures for a key label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletKeyLabelError {
    Empty,
    TooLong,
    ContainsControlCharacter,
}

impl fmt::Display for WalletKeyLabelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "key label must not be empty",
            Self::TooLong => "key label must not exceed 96 characters",
            Self::ContainsControlCharacter => "key label must not contain control characters",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletKeyLabelError {}

/// Algorithms understood by the wallet boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletKeyAlgorithm {
    Ed25519,
    P256,
    Jubjub,
}

/// Intended use recorded alongside a protected key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletKeyPurpose {
    Transaction,
    Authentication,
    Assertion,
    KeyAgreement,
    Recovery,
}

/// Encoding of safe public-key bytes returned by a key adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicKeyEncoding {
    Ed25519Compressed,
    Sec1Compressed,
    JubjubCompressed,
}

/// Public portion of a protected key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletPublicKey {
    encoding: PublicKeyEncoding,
    bytes: Vec<u8>,
}

impl WalletPublicKey {
    #[must_use]
    pub const fn new(encoding: PublicKeyEncoding, bytes: Vec<u8>) -> Self {
        Self { encoding, bytes }
    }

    #[must_use]
    pub const fn encoding(&self) -> PublicKeyEncoding {
        self.encoding
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Public metadata for a protected key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletKeyDescriptor {
    reference: WalletKeyReference,
    label: WalletKeyLabel,
    algorithm: WalletKeyAlgorithm,
    purpose: WalletKeyPurpose,
    public_key: WalletPublicKey,
    created_at: UnixTimestampMillis,
}

impl WalletKeyDescriptor {
    #[must_use]
    pub const fn new(
        reference: WalletKeyReference,
        label: WalletKeyLabel,
        algorithm: WalletKeyAlgorithm,
        purpose: WalletKeyPurpose,
        public_key: WalletPublicKey,
        created_at: UnixTimestampMillis,
    ) -> Self {
        Self {
            reference,
            label,
            algorithm,
            purpose,
            public_key,
            created_at,
        }
    }

    #[must_use]
    pub const fn reference(&self) -> &WalletKeyReference {
        &self.reference
    }

    #[must_use]
    pub const fn label(&self) -> &WalletKeyLabel {
        &self.label
    }

    #[must_use]
    pub const fn algorithm(&self) -> WalletKeyAlgorithm {
        self.algorithm
    }

    #[must_use]
    pub const fn purpose(&self) -> WalletKeyPurpose {
        self.purpose
    }

    #[must_use]
    pub const fn public_key(&self) -> &WalletPublicKey {
        &self.public_key
    }

    #[must_use]
    pub const fn created_at(&self) -> UnixTimestampMillis {
        self.created_at
    }
}

/// Whether protected state for a profile can currently be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletProtectionState {
    Uninitialized,
    Locked,
    Unlocked,
    Unavailable,
}

/// Effective—not requested—protection supplied by an adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletProtectionClass {
    DevelopmentOnly,
    OperatingSystem,
    HardwareBacked,
    Unavailable,
}

/// Safe capability/status result for the wallet protection boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletSecurityStatus {
    state: WalletProtectionState,
    protection: WalletProtectionClass,
    user_presence_required: bool,
    portable_backup_supported: bool,
}

impl WalletSecurityStatus {
    #[must_use]
    pub const fn new(
        state: WalletProtectionState,
        protection: WalletProtectionClass,
        user_presence_required: bool,
        portable_backup_supported: bool,
    ) -> Self {
        Self {
            state,
            protection,
            user_presence_required,
            portable_backup_supported,
        }
    }

    #[must_use]
    pub const fn unavailable() -> Self {
        Self::new(
            WalletProtectionState::Unavailable,
            WalletProtectionClass::Unavailable,
            false,
            false,
        )
    }

    #[must_use]
    pub const fn state(self) -> WalletProtectionState {
        self.state
    }

    #[must_use]
    pub const fn protection(self) -> WalletProtectionClass {
        self.protection
    }

    #[must_use]
    pub const fn user_presence_required(self) -> bool {
        self.user_presence_required
    }

    #[must_use]
    pub const fn portable_backup_supported(self) -> bool {
        self.portable_backup_supported
    }
}

/// Safe signature result; private key material remains inside the adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletSignature {
    algorithm: WalletKeyAlgorithm,
    bytes: Vec<u8>,
}

impl WalletSignature {
    #[must_use]
    pub const fn new(algorithm: WalletKeyAlgorithm, bytes: Vec<u8>) -> Self {
        Self { algorithm, bytes }
    }

    #[must_use]
    pub const fn algorithm(&self) -> WalletKeyAlgorithm {
        self.algorithm
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
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

    #[test]
    fn key_reference_and_label_are_public_opaque_metadata() {
        let reference = WalletKeyReference::parse("key_opaque_1").expect("reference is valid");
        let label = WalletKeyLabel::parse("  DID authentication  ").expect("label is valid");

        assert_eq!(reference.as_str(), "key_opaque_1");
        assert_eq!(label.as_str(), "DID authentication");
        assert_eq!(
            WalletKeyLabel::parse("bad\nlabel"),
            Err(WalletKeyLabelError::ContainsControlCharacter)
        );
    }

    #[test]
    fn unavailable_security_status_never_claims_protection() {
        let status = WalletSecurityStatus::unavailable();

        assert_eq!(status.state(), WalletProtectionState::Unavailable);
        assert_eq!(status.protection(), WalletProtectionClass::Unavailable);
        assert!(!status.user_presence_required());
        assert!(!status.portable_backup_supported());
    }
}
