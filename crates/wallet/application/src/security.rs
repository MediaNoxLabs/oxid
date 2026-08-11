// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, sync::Arc};

use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::{
    WalletKeyAlgorithm, WalletKeyDescriptor, WalletKeyLabel, WalletKeyLabelError, WalletKeyPurpose,
    WalletKeyReference, WalletProfileId, WalletProtectionClass, WalletProtectionState,
    WalletSecurityStatus, WalletSignature,
};

/// Maximum raw payload accepted by the generic signing application boundary.
pub const MAX_SIGNING_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_INTENT_TITLE_CHARACTERS: usize = 96;
const MAX_INTENT_SUMMARY_CHARACTERS: usize = 512;

/// Safe failures returned by protected adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletSecurityPortError {
    Unavailable,
    NotInitialized,
    AlreadyInitialized,
    Locked,
    NotFound,
    Conflict,
    UnsupportedAlgorithm,
    AuthorizationDenied,
    InvalidOperation,
}

impl fmt::Display for WalletSecurityPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unavailable => "wallet protection is unavailable",
            Self::NotInitialized => "wallet protection is not initialized",
            Self::AlreadyInitialized => "wallet protection is already initialized",
            Self::Locked => "wallet is locked",
            Self::NotFound => "protected key was not found",
            Self::Conflict => "protected key metadata conflicts with an existing key",
            Self::UnsupportedAlgorithm => "key algorithm is not supported by this adapter",
            Self::AuthorizationDenied => "wallet authorization was denied",
            Self::InvalidOperation => "protected operation could not be completed",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletSecurityPortError {}

/// Focused outgoing port for wallet protection and authorization state.
pub trait WalletProtectionPort: Send + Sync {
    fn status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError>;

    fn initialize(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError>;

    fn unlock(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError>;

    fn lock(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError>;
}

/// Adapter-neutral request for generating a protected key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerateProtectedKeyRequest {
    pub label: WalletKeyLabel,
    pub algorithm: WalletKeyAlgorithm,
    pub purpose: WalletKeyPurpose,
}

/// Focused outgoing port for operations that must keep private bytes protected.
pub trait WalletKeyOperationPort: Send + Sync {
    fn generate(
        &self,
        profile_id: &WalletProfileId,
        request: GenerateProtectedKeyRequest,
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError>;

    fn list(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Vec<WalletKeyDescriptor>, WalletSecurityPortError>;

    fn sign(
        &self,
        profile_id: &WalletProfileId,
        key_reference: &WalletKeyReference,
        payload: &[u8],
    ) -> Result<WalletSignature, WalletSecurityPortError>;

    fn delete(
        &self,
        profile_id: &WalletProfileId,
        key_reference: &WalletKeyReference,
    ) -> Result<(), WalletSecurityPortError>;
}

/// Public status returned to incoming adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletSecurityStatusView {
    pub state: WalletProtectionState,
    pub protection: WalletProtectionClass,
    pub user_presence_required: bool,
    pub portable_backup_supported: bool,
}

impl From<WalletSecurityStatus> for WalletSecurityStatusView {
    fn from(status: WalletSecurityStatus) -> Self {
        Self {
            state: status.state(),
            protection: status.protection(),
            user_presence_required: status.user_presence_required(),
            portable_backup_supported: status.portable_backup_supported(),
        }
    }
}

impl WalletSecurityStatusView {
    #[must_use]
    pub const fn state_name(self) -> &'static str {
        match self.state {
            WalletProtectionState::Uninitialized => "Uninitialized",
            WalletProtectionState::Locked => "Locked",
            WalletProtectionState::Unlocked => "Unlocked",
            WalletProtectionState::Unavailable => "Unavailable",
        }
    }

    #[must_use]
    pub const fn protection_name(self) -> &'static str {
        match self.protection {
            WalletProtectionClass::DevelopmentOnly => "Development only",
            WalletProtectionClass::OperatingSystem => "Operating system",
            WalletProtectionClass::HardwareBacked => "Hardware backed",
            WalletProtectionClass::Unavailable => "Not connected",
        }
    }

    #[must_use]
    pub const fn is_available(self) -> bool {
        !matches!(self.state, WalletProtectionState::Unavailable)
    }
}

/// Public protected-key metadata returned to incoming adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletKeyView {
    pub key_reference: String,
    pub label: String,
    pub algorithm: WalletKeyAlgorithm,
    pub purpose: WalletKeyPurpose,
    pub public_key_encoding: oxid_wallet_domain::PublicKeyEncoding,
    pub public_key_bytes: Vec<u8>,
    pub created_at_millis: u64,
}

impl From<&WalletKeyDescriptor> for WalletKeyView {
    fn from(descriptor: &WalletKeyDescriptor) -> Self {
        Self {
            key_reference: descriptor.reference().as_str().to_owned(),
            label: descriptor.label().as_str().to_owned(),
            algorithm: descriptor.algorithm(),
            purpose: descriptor.purpose(),
            public_key_encoding: descriptor.public_key().encoding(),
            public_key_bytes: descriptor.public_key().bytes().to_vec(),
            created_at_millis: descriptor.created_at().value(),
        }
    }
}

/// Safe signature result returned to incoming adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletSignatureView {
    pub algorithm: WalletKeyAlgorithm,
    pub signature_bytes: Vec<u8>,
}

impl From<WalletSignature> for WalletSignatureView {
    fn from(signature: WalletSignature) -> Self {
        Self {
            algorithm: signature.algorithm(),
            signature_bytes: signature.bytes().to_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletProfileSecurityCommand {
    pub profile_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerateWalletKeyCommand {
    pub profile_id: String,
    pub label: String,
    pub algorithm: WalletKeyAlgorithm,
    pub purpose: WalletKeyPurpose,
}

/// Explicit human-readable consent supplied by an incoming adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SensitiveOperationConfirmation {
    pub title: String,
    pub summary: String,
    pub confirmed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignWalletDataCommand {
    pub profile_id: String,
    pub key_reference: String,
    pub payload: Vec<u8>,
    pub confirmation: SensitiveOperationConfirmation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteWalletKeyCommand {
    pub profile_id: String,
    pub key_reference: String,
    pub confirmation: SensitiveOperationConfirmation,
}

pub trait GetWalletSecurityStatusUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<WalletSecurityStatusView, WalletSecurityError>;
}

pub trait InitializeWalletSecurityUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<WalletSecurityStatusView, WalletSecurityError>;
}

pub trait UnlockWalletUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<WalletSecurityStatusView, WalletSecurityError>;
}

pub trait LockWalletUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<WalletSecurityStatusView, WalletSecurityError>;
}

pub trait GenerateWalletKeyUseCase: Send + Sync {
    fn execute(&self, command: GenerateWalletKeyCommand) -> Result<WalletKeyView, WalletKeyError>;
}

pub trait ListWalletKeysUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<Vec<WalletKeyView>, WalletKeyError>;
}

pub trait SignWalletDataUseCase: Send + Sync {
    fn execute(
        &self,
        command: SignWalletDataCommand,
    ) -> Result<WalletSignatureView, SensitiveWalletOperationError>;
}

pub trait DeleteWalletKeyUseCase: Send + Sync {
    fn execute(&self, command: DeleteWalletKeyCommand)
    -> Result<(), SensitiveWalletOperationError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletSecurityError {
    InvalidProfileIdentifier(OpaqueIdError),
    Operation(WalletSecurityPortError),
}

impl fmt::Display for WalletSecurityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
            Self::Operation(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletSecurityError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletKeyError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidKeyReference(OpaqueIdError),
    InvalidLabel(WalletKeyLabelError),
    Operation(WalletSecurityPortError),
}

impl fmt::Display for WalletKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) | Self::InvalidKeyReference(error) => {
                error.fmt(formatter)
            }
            Self::InvalidLabel(error) => error.fmt(formatter),
            Self::Operation(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletKeyError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensitiveWalletOperationError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidKeyReference(OpaqueIdError),
    EmptyPayload,
    PayloadTooLarge,
    ConfirmationRequired,
    InvalidConfirmation,
    Operation(WalletSecurityPortError),
}

impl fmt::Display for SensitiveWalletOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidProfileIdentifier(error) | Self::InvalidKeyReference(error) => {
                return error.fmt(formatter);
            }
            Self::EmptyPayload => "signing payload must not be empty",
            Self::PayloadTooLarge => "signing payload exceeds the application limit",
            Self::ConfirmationRequired => "explicit confirmation is required",
            Self::InvalidConfirmation => "confirmation intent is invalid",
            Self::Operation(error) => return error.fmt(formatter),
        };
        formatter.write_str(message)
    }
}

impl Error for SensitiveWalletOperationError {}

pub struct WalletProtectionService<P> {
    protection: Arc<P>,
}

impl<P> WalletProtectionService<P> {
    #[must_use]
    pub const fn new(protection: Arc<P>) -> Self {
        Self { protection }
    }
}

impl<P> GetWalletSecurityStatusUseCase for WalletProtectionService<P>
where
    P: WalletProtectionPort + 'static,
{
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<WalletSecurityStatusView, WalletSecurityError> {
        let profile_id = parse_profile_id(command.profile_id)?;
        self.protection
            .status(&profile_id)
            .map(Into::into)
            .map_err(WalletSecurityError::Operation)
    }
}

impl<P> InitializeWalletSecurityUseCase for WalletProtectionService<P>
where
    P: WalletProtectionPort + 'static,
{
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<WalletSecurityStatusView, WalletSecurityError> {
        let profile_id = parse_profile_id(command.profile_id)?;
        self.protection
            .initialize(&profile_id)
            .map(Into::into)
            .map_err(WalletSecurityError::Operation)
    }
}

impl<P> UnlockWalletUseCase for WalletProtectionService<P>
where
    P: WalletProtectionPort + 'static,
{
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<WalletSecurityStatusView, WalletSecurityError> {
        let profile_id = parse_profile_id(command.profile_id)?;
        self.protection
            .unlock(&profile_id)
            .map(Into::into)
            .map_err(WalletSecurityError::Operation)
    }
}

impl<P> LockWalletUseCase for WalletProtectionService<P>
where
    P: WalletProtectionPort + 'static,
{
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<WalletSecurityStatusView, WalletSecurityError> {
        let profile_id = parse_profile_id(command.profile_id)?;
        self.protection
            .lock(&profile_id)
            .map(Into::into)
            .map_err(WalletSecurityError::Operation)
    }
}

pub struct WalletKeyService<K> {
    key_operations: Arc<K>,
}

impl<K> WalletKeyService<K> {
    #[must_use]
    pub const fn new(key_operations: Arc<K>) -> Self {
        Self { key_operations }
    }
}

impl<K> GenerateWalletKeyUseCase for WalletKeyService<K>
where
    K: WalletKeyOperationPort + 'static,
{
    fn execute(&self, command: GenerateWalletKeyCommand) -> Result<WalletKeyView, WalletKeyError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletKeyError::InvalidProfileIdentifier)?;
        let label = WalletKeyLabel::parse(command.label).map_err(WalletKeyError::InvalidLabel)?;
        let descriptor = self
            .key_operations
            .generate(
                &profile_id,
                GenerateProtectedKeyRequest {
                    label,
                    algorithm: command.algorithm,
                    purpose: command.purpose,
                },
            )
            .map_err(WalletKeyError::Operation)?;
        Ok(WalletKeyView::from(&descriptor))
    }
}

impl<K> ListWalletKeysUseCase for WalletKeyService<K>
where
    K: WalletKeyOperationPort + 'static,
{
    fn execute(
        &self,
        command: WalletProfileSecurityCommand,
    ) -> Result<Vec<WalletKeyView>, WalletKeyError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletKeyError::InvalidProfileIdentifier)?;
        let mut descriptors = self
            .key_operations
            .list(&profile_id)
            .map_err(WalletKeyError::Operation)?;
        descriptors.sort_by(|left, right| {
            left.created_at()
                .cmp(&right.created_at())
                .then_with(|| left.reference().cmp(right.reference()))
        });
        Ok(descriptors.iter().map(WalletKeyView::from).collect())
    }
}

impl<K> SignWalletDataUseCase for WalletKeyService<K>
where
    K: WalletKeyOperationPort + 'static,
{
    fn execute(
        &self,
        command: SignWalletDataCommand,
    ) -> Result<WalletSignatureView, SensitiveWalletOperationError> {
        validate_confirmation(&command.confirmation)?;
        if command.payload.is_empty() {
            return Err(SensitiveWalletOperationError::EmptyPayload);
        }
        if command.payload.len() > MAX_SIGNING_PAYLOAD_BYTES {
            return Err(SensitiveWalletOperationError::PayloadTooLarge);
        }
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(SensitiveWalletOperationError::InvalidProfileIdentifier)?;
        let key_reference = WalletKeyReference::parse(command.key_reference)
            .map_err(SensitiveWalletOperationError::InvalidKeyReference)?;
        self.key_operations
            .sign(&profile_id, &key_reference, &command.payload)
            .map(Into::into)
            .map_err(SensitiveWalletOperationError::Operation)
    }
}

impl<K> DeleteWalletKeyUseCase for WalletKeyService<K>
where
    K: WalletKeyOperationPort + 'static,
{
    fn execute(
        &self,
        command: DeleteWalletKeyCommand,
    ) -> Result<(), SensitiveWalletOperationError> {
        validate_confirmation(&command.confirmation)?;
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(SensitiveWalletOperationError::InvalidProfileIdentifier)?;
        let key_reference = WalletKeyReference::parse(command.key_reference)
            .map_err(SensitiveWalletOperationError::InvalidKeyReference)?;
        self.key_operations
            .delete(&profile_id, &key_reference)
            .map_err(SensitiveWalletOperationError::Operation)
    }
}

fn parse_profile_id(profile_id: String) -> Result<WalletProfileId, WalletSecurityError> {
    WalletProfileId::parse(profile_id).map_err(WalletSecurityError::InvalidProfileIdentifier)
}

fn validate_confirmation(
    confirmation: &SensitiveOperationConfirmation,
) -> Result<(), SensitiveWalletOperationError> {
    if !confirmation.confirmed {
        return Err(SensitiveWalletOperationError::ConfirmationRequired);
    }
    let title = confirmation.title.trim();
    let summary = confirmation.summary.trim();
    let invalid = title.is_empty()
        || summary.is_empty()
        || title.chars().count() > MAX_INTENT_TITLE_CHARACTERS
        || summary.chars().count() > MAX_INTENT_SUMMARY_CHARACTERS
        || title.chars().any(char::is_control)
        || summary.chars().any(char::is_control);
    if invalid {
        return Err(SensitiveWalletOperationError::InvalidConfirmation);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{PublicKeyEncoding, WalletPublicKey};

    use super::*;

    struct RecordingAdapter {
        status: Mutex<WalletSecurityStatus>,
        sign_calls: Mutex<usize>,
    }

    impl Default for RecordingAdapter {
        fn default() -> Self {
            Self {
                status: Mutex::new(WalletSecurityStatus::new(
                    WalletProtectionState::Uninitialized,
                    WalletProtectionClass::DevelopmentOnly,
                    false,
                    false,
                )),
                sign_calls: Mutex::new(0),
            }
        }
    }

    impl WalletProtectionPort for RecordingAdapter {
        fn status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
            self.status
                .lock()
                .map(|status| *status)
                .map_err(|_| WalletSecurityPortError::Unavailable)
        }

        fn initialize(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
            let status = WalletSecurityStatus::new(
                WalletProtectionState::Unlocked,
                WalletProtectionClass::DevelopmentOnly,
                false,
                false,
            );
            *self
                .status
                .lock()
                .map_err(|_| WalletSecurityPortError::Unavailable)? = status;
            Ok(status)
        }

        fn unlock(
            &self,
            profile_id: &WalletProfileId,
        ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
            self.initialize(profile_id)
        }

        fn lock(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
            let status = WalletSecurityStatus::new(
                WalletProtectionState::Locked,
                WalletProtectionClass::DevelopmentOnly,
                false,
                false,
            );
            *self
                .status
                .lock()
                .map_err(|_| WalletSecurityPortError::Unavailable)? = status;
            Ok(status)
        }
    }

    impl WalletKeyOperationPort for RecordingAdapter {
        fn generate(
            &self,
            _: &WalletProfileId,
            request: GenerateProtectedKeyRequest,
        ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
            Ok(WalletKeyDescriptor::new(
                WalletKeyReference::parse("key_test").expect("valid reference"),
                request.label,
                request.algorithm,
                request.purpose,
                WalletPublicKey::new(PublicKeyEncoding::Ed25519Compressed, vec![7; 32]),
                UnixTimestampMillis::new(42),
            ))
        }

        fn list(
            &self,
            _: &WalletProfileId,
        ) -> Result<Vec<WalletKeyDescriptor>, WalletSecurityPortError> {
            Ok(Vec::new())
        }

        fn sign(
            &self,
            _: &WalletProfileId,
            _: &WalletKeyReference,
            _: &[u8],
        ) -> Result<WalletSignature, WalletSecurityPortError> {
            *self
                .sign_calls
                .lock()
                .map_err(|_| WalletSecurityPortError::Unavailable)? += 1;
            Ok(WalletSignature::new(
                WalletKeyAlgorithm::Ed25519,
                vec![9; 64],
            ))
        }

        fn delete(
            &self,
            _: &WalletProfileId,
            _: &WalletKeyReference,
        ) -> Result<(), WalletSecurityPortError> {
            Ok(())
        }
    }

    fn confirmation(confirmed: bool) -> SensitiveOperationConfirmation {
        SensitiveOperationConfirmation {
            title: "Sign test challenge".to_owned(),
            summary: "Authorize the standalone conformance challenge".to_owned(),
            confirmed,
        }
    }

    #[test]
    fn protection_service_maps_typed_status_without_secret_input() {
        let adapter = Arc::new(RecordingAdapter::default());
        let service = WalletProtectionService::new(adapter);
        let command = WalletProfileSecurityCommand {
            profile_id: "profile_test".to_owned(),
        };

        assert_eq!(
            GetWalletSecurityStatusUseCase::execute(&service, command.clone())
                .expect("status should load")
                .state,
            WalletProtectionState::Uninitialized
        );
        assert_eq!(
            InitializeWalletSecurityUseCase::execute(&service, command)
                .expect("setup should succeed")
                .state,
            WalletProtectionState::Unlocked
        );
    }

    #[test]
    fn key_generation_normalizes_public_metadata() {
        let service = WalletKeyService::new(Arc::new(RecordingAdapter::default()));
        let view = GenerateWalletKeyUseCase::execute(
            &service,
            GenerateWalletKeyCommand {
                profile_id: "profile_test".to_owned(),
                label: "  Login key  ".to_owned(),
                algorithm: WalletKeyAlgorithm::Ed25519,
                purpose: WalletKeyPurpose::Authentication,
            },
        )
        .expect("generation should succeed");

        assert_eq!(view.key_reference, "key_test");
        assert_eq!(view.label, "Login key");
        assert_eq!(view.public_key_bytes, vec![7; 32]);
    }

    #[test]
    fn signing_requires_valid_confirmation_before_calling_adapter() {
        let adapter = Arc::new(RecordingAdapter::default());
        let service = WalletKeyService::new(Arc::clone(&adapter));
        let error = SignWalletDataUseCase::execute(
            &service,
            SignWalletDataCommand {
                profile_id: "profile_test".to_owned(),
                key_reference: "key_test".to_owned(),
                payload: b"challenge".to_vec(),
                confirmation: confirmation(false),
            },
        )
        .expect_err("unconfirmed signing must fail");

        assert_eq!(error, SensitiveWalletOperationError::ConfirmationRequired);
        assert_eq!(*adapter.sign_calls.lock().expect("counter is available"), 0);
    }

    #[test]
    fn signing_accepts_a_bounded_confirmed_intent() {
        let adapter = Arc::new(RecordingAdapter::default());
        let service = WalletKeyService::new(Arc::clone(&adapter));
        let signature = SignWalletDataUseCase::execute(
            &service,
            SignWalletDataCommand {
                profile_id: "profile_test".to_owned(),
                key_reference: "key_test".to_owned(),
                payload: b"challenge".to_vec(),
                confirmation: confirmation(true),
            },
        )
        .expect("confirmed signing should succeed");

        assert_eq!(signature.algorithm, WalletKeyAlgorithm::Ed25519);
        assert_eq!(signature.signature_bytes, vec![9; 64]);
        assert_eq!(*adapter.sign_calls.lock().expect("counter is available"), 1);
    }

    #[test]
    fn oversized_payload_is_rejected_before_adapter_use() {
        let adapter = Arc::new(RecordingAdapter::default());
        let service = WalletKeyService::new(Arc::clone(&adapter));
        let error = SignWalletDataUseCase::execute(
            &service,
            SignWalletDataCommand {
                profile_id: "profile_test".to_owned(),
                key_reference: "key_test".to_owned(),
                payload: vec![0; MAX_SIGNING_PAYLOAD_BYTES + 1],
                confirmation: confirmation(true),
            },
        )
        .expect_err("oversized signing must fail");

        assert_eq!(error, SensitiveWalletOperationError::PayloadTooLarge);
        assert_eq!(*adapter.sign_calls.lock().expect("counter is available"), 0);
    }
}
