// SPDX-License-Identifier: Apache-2.0

//! Application-owned boundary for explicit portable custody backup and recovery.
//!
//! The encrypted package may cross incoming adapters. Recovery secrets and
//! decrypted custody material may not.

use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::WalletProfileId;

use crate::{SensitiveOperationConfirmation, SensitiveWalletOperationError, validate_confirmation};

/// Maximum encrypted package accepted at the application boundary.
///
/// Complete-wallet archives can contain the bounded credential repository in
/// addition to public profile/DID state and custody. Native transports retain
/// the same bound and must measure peak-memory behavior on physical devices.
pub const MAX_PORTABLE_WALLET_BACKUP_BYTES: usize = 80 * 1024 * 1024;
/// Minimum recovery-secret length. This is an application guard, not an entropy estimate.
pub const MIN_WALLET_RECOVERY_SECRET_CHARACTERS: usize = 12;
/// Maximum recovery-secret length accepted from an incoming adapter.
pub const MAX_WALLET_RECOVERY_SECRET_CHARACTERS: usize = 128;
/// Maximum UTF-8 width accepted before invoking a password KDF.
pub const MAX_WALLET_RECOVERY_SECRET_BYTES: usize = 256;

pub const EXPORT_PORTABLE_WALLET_BACKUP_TITLE: &str = "Export portable wallet backup";
pub const EXPORT_PORTABLE_WALLET_BACKUP_SUMMARY: &str =
    "Create one encrypted, profile-bound backup containing protected wallet custody.";
pub const RECOVER_PORTABLE_WALLET_BACKUP_TITLE: &str = "Recover portable wallet backup";
pub const RECOVER_PORTABLE_WALLET_BACKUP_SUMMARY: &str =
    "Initialize this empty profile from one encrypted, profile-bound wallet backup.";
/// Fixed, capability-specific filename suggested to the operating-system document exporter.
pub const PORTABLE_WALLET_BACKUP_FILE_NAME: &str = "oxid-wallet-custody.oxidbak";
pub const EXPORT_COMPLETE_WALLET_BACKUP_TITLE: &str = "Export complete wallet backup";
pub const EXPORT_COMPLETE_WALLET_BACKUP_SUMMARY: &str =
    "Create one encrypted, profile-bound backup containing the complete wallet.";
pub const RECOVER_COMPLETE_WALLET_BACKUP_TITLE: &str = "Recover complete wallet backup";
pub const RECOVER_COMPLETE_WALLET_BACKUP_SUMMARY: &str =
    "Recover one complete wallet into an empty destination.";
/// Fixed filename for the complete-wallet successor to custody-only backups.
pub const COMPLETE_WALLET_BACKUP_FILE_NAME: &str = "oxid-wallet.oxidbak";

/// Validated recovery secret. Formatting and logging never expose its contents.
pub struct WalletRecoverySecret(Vec<u8>);

impl WalletRecoverySecret {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, WalletRecoverySecretError> {
        let value = value.as_ref();
        let characters = value.chars().count();
        if characters < MIN_WALLET_RECOVERY_SECRET_CHARACTERS {
            return Err(WalletRecoverySecretError::TooShort);
        }
        if characters > MAX_WALLET_RECOVERY_SECRET_CHARACTERS
            || value.len() > MAX_WALLET_RECOVERY_SECRET_BYTES
        {
            return Err(WalletRecoverySecretError::TooLong);
        }
        if value.trim() != value {
            return Err(WalletRecoverySecretError::SurroundingWhitespace);
        }
        if value.chars().any(char::is_control) {
            return Err(WalletRecoverySecretError::ContainsControlCharacter);
        }
        Ok(Self(value.as_bytes().to_vec()))
    }

    #[must_use]
    pub fn expose_to_backup_adapter(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for WalletRecoverySecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

impl fmt::Debug for WalletRecoverySecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletRecoverySecret([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRecoverySecretError {
    TooShort,
    TooLong,
    SurroundingWhitespace,
    ContainsControlCharacter,
}

impl fmt::Display for WalletRecoverySecretError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TooShort => "wallet recovery secret is too short",
            Self::TooLong => "wallet recovery secret is too long",
            Self::SurroundingWhitespace => {
                "wallet recovery secret must not contain surrounding whitespace"
            }
            Self::ContainsControlCharacter => {
                "wallet recovery secret must not contain control characters"
            }
        };
        formatter.write_str(message)
    }
}

impl Error for WalletRecoverySecretError {}

/// Opaque encrypted backup bytes safe to hand to a user-selected file adapter.
#[derive(PartialEq, Eq)]
pub struct PortableWalletBackup(Vec<u8>);

impl PortableWalletBackup {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, PortableWalletBackupError> {
        if bytes.is_empty() {
            return Err(PortableWalletBackupError::Empty);
        }
        if bytes.len() > MAX_PORTABLE_WALLET_BACKUP_BYTES {
            return Err(PortableWalletBackupError::TooLarge);
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn into_bytes(mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

impl Drop for PortableWalletBackup {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

impl fmt::Debug for PortableWalletBackup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortableWalletBackup")
            .field("encrypted_bytes", &self.0.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortableWalletBackupError {
    Empty,
    TooLarge,
}

impl fmt::Display for PortableWalletBackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "portable wallet backup must not be empty",
            Self::TooLarge => "portable wallet backup exceeds the application limit",
        })
    }
}

impl Error for PortableWalletBackupError {}

/// A bounded future returned by the operating-system backup document adapter.
pub type PortableWalletBackupDocumentFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, PortableWalletBackupDocumentError>> + Send + 'a>>;

/// Closed set of encrypted backup documents that the native picker can export.
///
/// Callers choose a capability, never a filesystem path or arbitrary filename.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortableWalletBackupDocumentKind {
    CustodyOnly,
    CompleteWallet,
}

impl PortableWalletBackupDocumentKind {
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::CustodyOnly => PORTABLE_WALLET_BACKUP_FILE_NAME,
            Self::CompleteWallet => COMPLETE_WALLET_BACKUP_FILE_NAME,
        }
    }
}

/// Stable, payload-free failures from a user-selected backup document flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortableWalletBackupDocumentError {
    Cancelled,
    Unavailable,
    TimedOut,
    InvalidDocument,
    Failed,
}

impl fmt::Display for PortableWalletBackupDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "wallet backup document selection was cancelled",
            Self::Unavailable => "wallet backup documents are unavailable on this device",
            Self::TimedOut => "wallet backup document selection timed out",
            Self::InvalidDocument => "wallet backup document is invalid",
            Self::Failed => "wallet backup document operation failed",
        })
    }
}

impl Error for PortableWalletBackupDocumentError {}

/// User-selected document transport for encrypted backup packages only.
///
/// Implementations choose files through operating-system UI. No arbitrary
/// caller-supplied path crosses this boundary.
pub trait PortableWalletBackupDocumentPort: Send + Sync {
    fn export<'a>(
        &'a self,
        kind: PortableWalletBackupDocumentKind,
        backup: &'a PortableWalletBackup,
    ) -> PortableWalletBackupDocumentFuture<'a, ()>;

    fn import<'a>(&'a self) -> PortableWalletBackupDocumentFuture<'a, PortableWalletBackup>;
}

/// Fail-closed document transport used by non-mobile compositions.
pub struct UnavailablePortableWalletBackupDocuments;

impl PortableWalletBackupDocumentPort for UnavailablePortableWalletBackupDocuments {
    fn export<'a>(
        &'a self,
        _kind: PortableWalletBackupDocumentKind,
        _backup: &'a PortableWalletBackup,
    ) -> PortableWalletBackupDocumentFuture<'a, ()> {
        Box::pin(async { Err(PortableWalletBackupDocumentError::Unavailable) })
    }

    fn import<'a>(&'a self) -> PortableWalletBackupDocumentFuture<'a, PortableWalletBackup> {
        Box::pin(async { Err(PortableWalletBackupDocumentError::Unavailable) })
    }
}

/// Stable, non-secret recovery outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletPortableRecoverySummary {
    pub restored_key_count: usize,
}

/// Safe failures returned by portable custody adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletPortableBackupPortError {
    Unavailable,
    NotInitialized,
    AlreadyInitialized,
    Locked,
    AuthorizationDenied,
    InvalidPackage,
    AuthenticationFailed,
    WrongProfile,
    Conflict,
    InvalidOperation,
}

impl fmt::Display for WalletPortableBackupPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "portable wallet backup is unavailable",
            Self::NotInitialized => "wallet protection is not initialized",
            Self::AlreadyInitialized => "wallet protection is already initialized",
            Self::Locked => "wallet is locked",
            Self::AuthorizationDenied => "wallet backup authorization was denied",
            Self::InvalidPackage => "portable wallet backup is invalid",
            Self::AuthenticationFailed => "portable wallet backup authentication failed",
            Self::WrongProfile => "portable wallet backup belongs to another profile",
            Self::Conflict => "portable wallet recovery conflicts with existing state",
            Self::InvalidOperation => "portable wallet backup could not be completed",
        })
    }
}

impl Error for WalletPortableBackupPortError {}

/// Outgoing custody port. Plaintext protected material never crosses this boundary.
pub trait WalletPortableBackupPort: Send + Sync {
    fn export_portable_backup(
        &self,
        profile_id: &WalletProfileId,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<PortableWalletBackup, WalletPortableBackupPortError>;

    fn recover_portable_backup(
        &self,
        profile_id: &WalletProfileId,
        backup: &PortableWalletBackup,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError>;
}

/// Stable, non-secret outcome from complete-wallet recovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteWalletRecoverySummary {
    pub profile_id: String,
    pub restored_key_count: usize,
    pub restored_did_count: usize,
    pub restored_credential_count: usize,
}

/// Outgoing boundary for one authenticated all-store archive. Decrypted state
/// remains inside its adapter and never crosses an incoming application port.
pub trait CompleteWalletBackupPort: Send + Sync {
    fn export_complete_wallet_backup(
        &self,
        profile_id: &WalletProfileId,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<PortableWalletBackup, WalletPortableBackupPortError>;

    fn recover_complete_wallet_backup(
        &self,
        expected_profile_id: Option<&WalletProfileId>,
        backup: &PortableWalletBackup,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<CompleteWalletRecoverySummary, WalletPortableBackupPortError>;
}

pub struct ExportPortableWalletBackupCommand {
    pub profile_id: String,
    pub recovery_secret: WalletRecoverySecret,
    pub confirmation: SensitiveOperationConfirmation,
}

pub struct RecoverPortableWalletBackupCommand {
    pub profile_id: String,
    pub backup: PortableWalletBackup,
    pub recovery_secret: WalletRecoverySecret,
    pub confirmation: SensitiveOperationConfirmation,
}

pub struct ExportCompleteWalletBackupCommand {
    pub profile_id: String,
    pub recovery_secret: WalletRecoverySecret,
    pub confirmation: SensitiveOperationConfirmation,
}

/// Fresh-install recovery leaves `expected_profile_id` empty. An existing-
/// profile entry point supplies an exact identifier to prevent cross-profile
/// import after archive authentication.
pub struct RecoverCompleteWalletBackupCommand {
    pub expected_profile_id: Option<String>,
    pub backup: PortableWalletBackup,
    pub recovery_secret: WalletRecoverySecret,
    pub confirmation: SensitiveOperationConfirmation,
}

pub trait ExportPortableWalletBackupUseCase: Send + Sync {
    fn execute(
        &self,
        command: ExportPortableWalletBackupCommand,
    ) -> Result<PortableWalletBackup, WalletPortableBackupUseCaseError>;
}

pub trait RecoverPortableWalletBackupUseCase: Send + Sync {
    fn execute(
        &self,
        command: RecoverPortableWalletBackupCommand,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupUseCaseError>;
}

pub trait ExportCompleteWalletBackupUseCase: Send + Sync {
    fn execute(
        &self,
        command: ExportCompleteWalletBackupCommand,
    ) -> Result<PortableWalletBackup, WalletPortableBackupUseCaseError>;
}

pub trait RecoverCompleteWalletBackupUseCase: Send + Sync {
    fn execute(
        &self,
        command: RecoverCompleteWalletBackupCommand,
    ) -> Result<CompleteWalletRecoverySummary, WalletPortableBackupUseCaseError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletPortableBackupUseCaseError {
    InvalidProfileIdentifier(OpaqueIdError),
    ConfirmationRequired,
    InvalidConfirmation,
    IncorrectIntent,
    Operation(WalletPortableBackupPortError),
}

impl fmt::Display for WalletPortableBackupUseCaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
            Self::ConfirmationRequired => formatter.write_str("explicit confirmation is required"),
            Self::InvalidConfirmation => formatter.write_str("confirmation intent is invalid"),
            Self::IncorrectIntent => {
                formatter.write_str("confirmation does not match this operation")
            }
            Self::Operation(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletPortableBackupUseCaseError {}

pub struct WalletPortableBackupService<P> {
    port: Arc<P>,
}

pub struct CompleteWalletBackupService<P> {
    port: Arc<P>,
}

impl<P> CompleteWalletBackupService<P> {
    #[must_use]
    pub const fn new(port: Arc<P>) -> Self {
        Self { port }
    }
}

impl<P> WalletPortableBackupService<P> {
    #[must_use]
    pub const fn new(port: Arc<P>) -> Self {
        Self { port }
    }
}

impl<P> ExportPortableWalletBackupUseCase for WalletPortableBackupService<P>
where
    P: WalletPortableBackupPort + 'static,
{
    fn execute(
        &self,
        command: ExportPortableWalletBackupCommand,
    ) -> Result<PortableWalletBackup, WalletPortableBackupUseCaseError> {
        validate_exact_confirmation(
            &command.confirmation,
            EXPORT_PORTABLE_WALLET_BACKUP_TITLE,
            EXPORT_PORTABLE_WALLET_BACKUP_SUMMARY,
        )?;
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletPortableBackupUseCaseError::InvalidProfileIdentifier)?;
        self.port
            .export_portable_backup(&profile_id, &command.recovery_secret)
            .map_err(WalletPortableBackupUseCaseError::Operation)
    }
}

impl<P> RecoverPortableWalletBackupUseCase for WalletPortableBackupService<P>
where
    P: WalletPortableBackupPort + 'static,
{
    fn execute(
        &self,
        command: RecoverPortableWalletBackupCommand,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupUseCaseError> {
        validate_exact_confirmation(
            &command.confirmation,
            RECOVER_PORTABLE_WALLET_BACKUP_TITLE,
            RECOVER_PORTABLE_WALLET_BACKUP_SUMMARY,
        )?;
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletPortableBackupUseCaseError::InvalidProfileIdentifier)?;
        self.port
            .recover_portable_backup(&profile_id, &command.backup, &command.recovery_secret)
            .map_err(WalletPortableBackupUseCaseError::Operation)
    }
}

impl<P> ExportCompleteWalletBackupUseCase for CompleteWalletBackupService<P>
where
    P: CompleteWalletBackupPort + 'static,
{
    fn execute(
        &self,
        command: ExportCompleteWalletBackupCommand,
    ) -> Result<PortableWalletBackup, WalletPortableBackupUseCaseError> {
        validate_exact_confirmation(
            &command.confirmation,
            EXPORT_COMPLETE_WALLET_BACKUP_TITLE,
            EXPORT_COMPLETE_WALLET_BACKUP_SUMMARY,
        )?;
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletPortableBackupUseCaseError::InvalidProfileIdentifier)?;
        self.port
            .export_complete_wallet_backup(&profile_id, &command.recovery_secret)
            .map_err(WalletPortableBackupUseCaseError::Operation)
    }
}

impl<P> RecoverCompleteWalletBackupUseCase for CompleteWalletBackupService<P>
where
    P: CompleteWalletBackupPort + 'static,
{
    fn execute(
        &self,
        command: RecoverCompleteWalletBackupCommand,
    ) -> Result<CompleteWalletRecoverySummary, WalletPortableBackupUseCaseError> {
        validate_exact_confirmation(
            &command.confirmation,
            RECOVER_COMPLETE_WALLET_BACKUP_TITLE,
            RECOVER_COMPLETE_WALLET_BACKUP_SUMMARY,
        )?;
        let expected_profile_id = command
            .expected_profile_id
            .map(WalletProfileId::parse)
            .transpose()
            .map_err(WalletPortableBackupUseCaseError::InvalidProfileIdentifier)?;
        self.port
            .recover_complete_wallet_backup(
                expected_profile_id.as_ref(),
                &command.backup,
                &command.recovery_secret,
            )
            .map_err(WalletPortableBackupUseCaseError::Operation)
    }
}

fn validate_exact_confirmation(
    confirmation: &SensitiveOperationConfirmation,
    expected_title: &str,
    expected_summary: &str,
) -> Result<(), WalletPortableBackupUseCaseError> {
    validate_confirmation(confirmation).map_err(|error| match error {
        SensitiveWalletOperationError::ConfirmationRequired => {
            WalletPortableBackupUseCaseError::ConfirmationRequired
        }
        _ => WalletPortableBackupUseCaseError::InvalidConfirmation,
    })?;
    if confirmation.title != expected_title || confirmation.summary != expected_summary {
        return Err(WalletPortableBackupUseCaseError::IncorrectIntent);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort(Mutex<usize>);

    impl WalletPortableBackupPort for RecordingPort {
        fn export_portable_backup(
            &self,
            _: &WalletProfileId,
            _: &WalletRecoverySecret,
        ) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
            *self.0.lock().expect("recording mutex should be available") += 1;
            PortableWalletBackup::parse(vec![1])
                .map_err(|_| WalletPortableBackupPortError::InvalidOperation)
        }

        fn recover_portable_backup(
            &self,
            _: &WalletProfileId,
            _: &PortableWalletBackup,
            _: &WalletRecoverySecret,
        ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
            *self.0.lock().expect("recording mutex should be available") += 1;
            Ok(WalletPortableRecoverySummary {
                restored_key_count: 3,
            })
        }
    }

    impl CompleteWalletBackupPort for RecordingPort {
        fn export_complete_wallet_backup(
            &self,
            _: &WalletProfileId,
            _: &WalletRecoverySecret,
        ) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
            *self.0.lock().expect("recording mutex should be available") += 1;
            PortableWalletBackup::parse(vec![2])
                .map_err(|_| WalletPortableBackupPortError::InvalidOperation)
        }

        fn recover_complete_wallet_backup(
            &self,
            _: Option<&WalletProfileId>,
            _: &PortableWalletBackup,
            _: &WalletRecoverySecret,
        ) -> Result<CompleteWalletRecoverySummary, WalletPortableBackupPortError> {
            *self.0.lock().expect("recording mutex should be available") += 1;
            Ok(CompleteWalletRecoverySummary {
                profile_id: "profile_test".to_owned(),
                restored_key_count: 3,
                restored_did_count: 2,
                restored_credential_count: 1,
            })
        }
    }

    fn secret() -> WalletRecoverySecret {
        WalletRecoverySecret::parse("correct horse battery staple")
            .expect("recovery secret should be valid")
    }

    #[test]
    fn recovery_secret_and_backup_debug_are_redacted() {
        assert_eq!(
            format!("{:?}", secret()),
            "WalletRecoverySecret([REDACTED])"
        );
        let backup = PortableWalletBackup::parse(vec![7; 20]).expect("backup should be bounded");
        assert_eq!(
            format!("{backup:?}"),
            "PortableWalletBackup { encrypted_bytes: 20 }"
        );
    }

    #[test]
    fn recovery_secret_is_strictly_bounded() {
        assert_eq!(
            WalletRecoverySecret::parse("too short").expect_err("short secret must fail"),
            WalletRecoverySecretError::TooShort
        );
        assert_eq!(
            WalletRecoverySecret::parse(" correct horse battery staple")
                .expect_err("surrounding whitespace must fail"),
            WalletRecoverySecretError::SurroundingWhitespace
        );
    }

    #[test]
    fn export_requires_the_exact_confirmed_intent() {
        let port = Arc::new(RecordingPort::default());
        let service = WalletPortableBackupService::new(Arc::clone(&port));
        let error = ExportPortableWalletBackupUseCase::execute(
            &service,
            ExportPortableWalletBackupCommand {
                profile_id: "profile_test".to_owned(),
                recovery_secret: secret(),
                confirmation: SensitiveOperationConfirmation {
                    title: "Export something".to_owned(),
                    summary: EXPORT_PORTABLE_WALLET_BACKUP_SUMMARY.to_owned(),
                    confirmed: true,
                },
            },
        )
        .expect_err("mismatched intent must fail");
        assert_eq!(error, WalletPortableBackupUseCaseError::IncorrectIntent);
        assert_eq!(
            *port.0.lock().expect("recording mutex should be available"),
            0
        );
    }

    #[test]
    fn recovery_dispatches_only_after_exact_confirmation() {
        let port = Arc::new(RecordingPort::default());
        let service = WalletPortableBackupService::new(Arc::clone(&port));
        let summary = RecoverPortableWalletBackupUseCase::execute(
            &service,
            RecoverPortableWalletBackupCommand {
                profile_id: "profile_test".to_owned(),
                backup: PortableWalletBackup::parse(vec![1]).expect("backup should be valid"),
                recovery_secret: secret(),
                confirmation: SensitiveOperationConfirmation {
                    title: RECOVER_PORTABLE_WALLET_BACKUP_TITLE.to_owned(),
                    summary: RECOVER_PORTABLE_WALLET_BACKUP_SUMMARY.to_owned(),
                    confirmed: true,
                },
            },
        )
        .expect("exact recovery intent should dispatch");
        assert_eq!(summary.restored_key_count, 3);
        assert_eq!(
            *port.0.lock().expect("recording mutex should be available"),
            1
        );
    }

    #[test]
    fn fresh_install_complete_recovery_authenticates_before_profile_selection() {
        let port = Arc::new(RecordingPort::default());
        let service = CompleteWalletBackupService::new(Arc::clone(&port));
        let summary = RecoverCompleteWalletBackupUseCase::execute(
            &service,
            RecoverCompleteWalletBackupCommand {
                expected_profile_id: None,
                backup: PortableWalletBackup::parse(vec![2]).expect("backup should be valid"),
                recovery_secret: secret(),
                confirmation: SensitiveOperationConfirmation {
                    title: RECOVER_COMPLETE_WALLET_BACKUP_TITLE.to_owned(),
                    summary: RECOVER_COMPLETE_WALLET_BACKUP_SUMMARY.to_owned(),
                    confirmed: true,
                },
            },
        )
        .expect("fresh install recovery should dispatch");
        assert_eq!(summary.profile_id, "profile_test");
        assert_eq!(summary.restored_did_count, 2);
        assert_eq!(
            *port.0.lock().expect("recording mutex should be available"),
            1
        );
    }

    #[test]
    fn native_document_kinds_have_fixed_distinct_filenames() {
        assert_eq!(
            PortableWalletBackupDocumentKind::CustodyOnly.file_name(),
            "oxid-wallet-custody.oxidbak"
        );
        assert_eq!(
            PortableWalletBackupDocumentKind::CompleteWallet.file_name(),
            "oxid-wallet.oxidbak"
        );
    }
}
