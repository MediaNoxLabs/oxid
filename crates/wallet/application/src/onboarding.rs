// SPDX-License-Identifier: Apache-2.0

//! Transient, one-shot creation and restoration of a protected Midnight wallet.
//!
//! The application retains only a typed root between prepare and complete. A
//! recovery phrase is returned once to the incoming adapter, never persisted,
//! and completion consumes the root before entering platform custody.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

use oxid_foundation::OpaqueIdError;
use oxid_platform_ports::{PlatformError, RandomPort};
use oxid_wallet_domain::WalletProfileId;

use crate::{
    RECOVER_WALLET_ROOT_SUMMARY, RECOVER_WALLET_ROOT_TITLE, RecoverWalletRootCommand,
    RecoverWalletRootUseCase, SensitiveOperationConfirmation, SensitiveWalletOperationError,
    WalletRootRecoveryError, WalletRootRecoveryView, WalletRootSeed, validate_confirmation,
};

pub const COMPLETE_WALLET_ONBOARDING_TITLE: &str = "Protect wallet recovery phrase";
pub const COMPLETE_WALLET_ONBOARDING_SUMMARY: &str =
    "I saved the 24-word recovery phrase and authorize this wallet to install its root.";
pub const WALLET_ONBOARDING_ENTROPY_BYTES: usize = 32;
const WALLET_ONBOARDING_CEREMONY_ID_BYTES: usize = 16;

/// Payload-free, one-use authorization for displaying a newly created phrase.
/// Implementations must obtain fresh user presence and never accept caller prose.
pub trait WalletOnboardingAuthorizationPort: Send + Sync {
    fn authorize_recovery_phrase_reveal(&self) -> Result<(), WalletOnboardingAuthorizationError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletOnboardingAuthorizationError {
    Denied,
    Unavailable,
}

impl fmt::Display for WalletOnboardingAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Denied => "device authorization was not completed",
            Self::Unavailable => "device authorization is unavailable",
        })
    }
}

impl Error for WalletOnboardingAuthorizationError {}
const MAX_PENDING_WALLET_ONBOARDINGS: usize = 8;

/// Recovery text crossing only the authenticated onboarding boundary.
/// Debug output is always redacted and dropping the value clears its buffer.
pub struct WalletRecoveryPhrase(Vec<u8>);

impl WalletRecoveryPhrase {
    #[must_use]
    pub fn new(phrase: String) -> Self {
        Self(phrase.into_bytes())
    }

    /// Reveals the phrase only to the authenticated onboarding screen.
    #[must_use]
    pub fn expose_for_onboarding(&self) -> &str {
        // Construction starts from a valid Rust String and mutation is private.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}

impl Drop for WalletRecoveryPhrase {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&mut self.0);
    }
}

impl fmt::Debug for WalletRecoveryPhrase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletRecoveryPhrase([REDACTED])")
    }
}

pub enum WalletOnboardingMode {
    CreateNew,
    RestoreMnemonic {
        phrase: WalletRecoveryPhrase,
    },
    #[cfg(feature = "development-wallet-root")]
    RestoreRawSeed {
        root: WalletRootSeed,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletMnemonicPortError {
    InvalidPhrase,
    PhraseNotNormalized,
    InvalidWordCount,
}

impl fmt::Display for WalletMnemonicPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPhrase => "recovery phrase is invalid",
            Self::PhraseNotNormalized => "recovery phrase is not normalized",
            Self::InvalidWordCount => "recovery phrase has an invalid word count",
        })
    }
}

impl Error for WalletMnemonicPortError {}

pub struct CreatedWalletMnemonic {
    pub phrase: WalletRecoveryPhrase,
    pub root: WalletRootSeed,
}

/// Adapter boundary for BIP-39 vocabulary, checksum, normalization, and seed
/// conversion. Application policy never depends on the external codec.
pub trait WalletMnemonicPort: Send + Sync {
    fn create_from_entropy(
        &self,
        entropy: &[u8; WALLET_ONBOARDING_ENTROPY_BYTES],
    ) -> Result<CreatedWalletMnemonic, WalletMnemonicPortError>;

    fn restore_phrase(
        &self,
        phrase: &WalletRecoveryPhrase,
    ) -> Result<WalletRootSeed, WalletMnemonicPortError>;
}

impl fmt::Debug for WalletOnboardingMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateNew => formatter.write_str("CreateNew"),
            Self::RestoreMnemonic { .. } => formatter.write_str("RestoreMnemonic([REDACTED])"),
            #[cfg(feature = "development-wallet-root")]
            Self::RestoreRawSeed { .. } => formatter.write_str("RestoreRawSeed([REDACTED])"),
        }
    }
}

pub struct PrepareWalletOnboardingCommand {
    pub profile_id: String,
    pub mode: WalletOnboardingMode,
}

impl fmt::Debug for PrepareWalletOnboardingCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrepareWalletOnboardingCommand")
            .field("profile_id", &self.profile_id)
            .field("mode", &self.mode)
            .finish()
    }
}

pub struct PreparedWalletOnboarding {
    pub ceremony_id: String,
    pub created_recovery_phrase: Option<WalletRecoveryPhrase>,
    pub backup_acknowledgement_required: bool,
}

impl fmt::Debug for PreparedWalletOnboarding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedWalletOnboarding")
            .field("ceremony_id", &self.ceremony_id)
            .field(
                "created_recovery_phrase",
                &self.created_recovery_phrase.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "backup_acknowledgement_required",
                &self.backup_acknowledgement_required,
            )
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteWalletOnboardingCommand {
    pub profile_id: String,
    pub ceremony_id: String,
    pub backup_acknowledged: bool,
    pub confirmation: SensitiveOperationConfirmation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancelWalletOnboardingCommand {
    pub profile_id: String,
    pub ceremony_id: String,
}

pub trait PrepareWalletOnboardingUseCase: Send + Sync {
    fn execute(
        &self,
        command: PrepareWalletOnboardingCommand,
    ) -> Result<PreparedWalletOnboarding, WalletOnboardingError>;
}

pub trait CompleteWalletOnboardingUseCase: Send + Sync {
    fn execute(
        &self,
        command: CompleteWalletOnboardingCommand,
    ) -> Result<WalletRootRecoveryView, WalletOnboardingError>;
}

pub trait CancelWalletOnboardingUseCase: Send + Sync {
    fn execute(&self, command: CancelWalletOnboardingCommand) -> Result<(), WalletOnboardingError>;

    /// Clears every process-local secret when the incoming adapter is suspended.
    fn suspend(&self);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletOnboardingError {
    InvalidProfileIdentifier(OpaqueIdError),
    Randomness(PlatformError),
    InvalidMnemonic,
    UserPresence(WalletOnboardingAuthorizationError),
    MnemonicMustBeNormalized,
    MnemonicMustContainTwentyFourWords,
    CeremonyAlreadyPending,
    CeremonyCapacityReached,
    CeremonyNotFound,
    BackupAcknowledgementRequired,
    ConfirmationRequired,
    InvalidConfirmation,
    Recovery(WalletRootRecoveryError),
    StateUnavailable,
}

impl fmt::Display for WalletOnboardingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProfileIdentifier(_) => "wallet profile identifier is invalid",
            Self::Randomness(_) => "secure wallet randomness is unavailable",
            Self::InvalidMnemonic => "recovery phrase is not a valid English BIP-39 phrase",
            Self::UserPresence(error) => return error.fmt(formatter),
            Self::MnemonicMustBeNormalized => {
                "recovery phrase must use normalized lowercase words separated by one space"
            }
            Self::MnemonicMustContainTwentyFourWords => {
                "recovery phrase must contain exactly 24 words"
            }
            Self::CeremonyAlreadyPending => {
                "a wallet onboarding ceremony is already pending for this profile"
            }
            Self::CeremonyCapacityReached => "wallet onboarding ceremony capacity is reached",
            Self::CeremonyNotFound => "wallet onboarding ceremony was not found",
            Self::BackupAcknowledgementRequired => {
                "recovery phrase backup acknowledgement is required"
            }
            Self::ConfirmationRequired => "wallet onboarding confirmation is required",
            Self::InvalidConfirmation => "wallet onboarding confirmation is invalid",
            Self::Recovery(_) => "protected wallet initialization failed",
            Self::StateUnavailable => "wallet onboarding state is unavailable",
        })
    }
}

impl Error for WalletOnboardingError {}

struct PendingWalletOnboarding {
    ceremony_id: String,
    root: WalletRootSeed,
}

/// Process-local coordinator. Pending roots intentionally disappear on restart;
/// callers must restart the ceremony and native custody still refuses a second
/// root if the previous completion reached durable installation.
pub struct WalletOnboardingService<N, M, R, A> {
    random: Arc<N>,
    mnemonics: Arc<M>,
    recovery: Arc<R>,
    authorization: Arc<A>,
    pending: Mutex<BTreeMap<WalletProfileId, PendingWalletOnboarding>>,
}

impl<N, M, R, A> WalletOnboardingService<N, M, R, A> {
    #[must_use]
    pub fn new(random: Arc<N>, mnemonics: Arc<M>, recovery: Arc<R>, authorization: Arc<A>) -> Self {
        Self {
            random,
            mnemonics,
            recovery,
            authorization,
            pending: Mutex::new(BTreeMap::new()),
        }
    }
}

impl<N, M, R, A> PrepareWalletOnboardingUseCase for WalletOnboardingService<N, M, R, A>
where
    N: RandomPort + 'static,
    M: WalletMnemonicPort + 'static,
    R: RecoverWalletRootUseCase + 'static,
    A: WalletOnboardingAuthorizationPort + 'static,
{
    fn execute(
        &self,
        command: PrepareWalletOnboardingCommand,
    ) -> Result<PreparedWalletOnboarding, WalletOnboardingError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletOnboardingError::InvalidProfileIdentifier)?;
        {
            let pending = self
                .pending
                .lock()
                .map_err(|_| WalletOnboardingError::StateUnavailable)?;
            if pending.contains_key(&profile_id) {
                return Err(WalletOnboardingError::CeremonyAlreadyPending);
            }
            if pending.len() >= MAX_PENDING_WALLET_ONBOARDINGS {
                return Err(WalletOnboardingError::CeremonyCapacityReached);
            }
        }

        let (root, created_recovery_phrase) = match command.mode {
            WalletOnboardingMode::CreateNew => {
                self.authorization
                    .authorize_recovery_phrase_reveal()
                    .map_err(WalletOnboardingError::UserPresence)?;
                let mut entropy = SecretEntropy::default();
                self.random
                    .fill_bytes(&mut entropy.0)
                    .map_err(WalletOnboardingError::Randomness)?;
                let created = self
                    .mnemonics
                    .create_from_entropy(&entropy.0)
                    .map_err(map_mnemonic_error)?;
                (created.root, Some(created.phrase))
            }
            WalletOnboardingMode::RestoreMnemonic { phrase } => {
                let root = self
                    .mnemonics
                    .restore_phrase(&phrase)
                    .map_err(map_mnemonic_error)?;
                (root, None)
            }
            #[cfg(feature = "development-wallet-root")]
            WalletOnboardingMode::RestoreRawSeed { root } => (root, None),
        };

        let mut ceremony_random = [0_u8; WALLET_ONBOARDING_CEREMONY_ID_BYTES];
        self.random
            .fill_bytes(&mut ceremony_random)
            .map_err(WalletOnboardingError::Randomness)?;
        let ceremony_id = encode_ceremony_id(ceremony_random);
        ceremony_random.fill(0);

        let mut pending = self
            .pending
            .lock()
            .map_err(|_| WalletOnboardingError::StateUnavailable)?;
        if pending.contains_key(&profile_id) {
            return Err(WalletOnboardingError::CeremonyAlreadyPending);
        }
        if pending.len() >= MAX_PENDING_WALLET_ONBOARDINGS {
            return Err(WalletOnboardingError::CeremonyCapacityReached);
        }
        pending.insert(
            profile_id,
            PendingWalletOnboarding {
                ceremony_id: ceremony_id.clone(),
                root,
            },
        );

        Ok(PreparedWalletOnboarding {
            ceremony_id,
            created_recovery_phrase,
            backup_acknowledgement_required: true,
        })
    }
}

impl<N, M, R, A> CompleteWalletOnboardingUseCase for WalletOnboardingService<N, M, R, A>
where
    N: RandomPort + 'static,
    M: WalletMnemonicPort + 'static,
    R: RecoverWalletRootUseCase + 'static,
    A: WalletOnboardingAuthorizationPort + 'static,
{
    fn execute(
        &self,
        command: CompleteWalletOnboardingCommand,
    ) -> Result<WalletRootRecoveryView, WalletOnboardingError> {
        map_onboarding_confirmation(validate_confirmation(&command.confirmation))?;
        if command.confirmation.title != COMPLETE_WALLET_ONBOARDING_TITLE
            || command.confirmation.summary != COMPLETE_WALLET_ONBOARDING_SUMMARY
        {
            return Err(WalletOnboardingError::InvalidConfirmation);
        }
        if !command.backup_acknowledged {
            return Err(WalletOnboardingError::BackupAcknowledgementRequired);
        }
        let profile_id = WalletProfileId::parse(command.profile_id.clone())
            .map_err(WalletOnboardingError::InvalidProfileIdentifier)?;
        let pending = {
            let mut ceremonies = self
                .pending
                .lock()
                .map_err(|_| WalletOnboardingError::StateUnavailable)?;
            let matches = ceremonies
                .get(&profile_id)
                .is_some_and(|pending| pending.ceremony_id == command.ceremony_id);
            if !matches {
                return Err(WalletOnboardingError::CeremonyNotFound);
            }
            ceremonies
                .remove(&profile_id)
                .ok_or(WalletOnboardingError::CeremonyNotFound)?
        };

        self.recovery
            .execute(RecoverWalletRootCommand {
                profile_id: command.profile_id,
                root: pending.root,
                confirmation: SensitiveOperationConfirmation {
                    title: RECOVER_WALLET_ROOT_TITLE.to_owned(),
                    summary: RECOVER_WALLET_ROOT_SUMMARY.to_owned(),
                    confirmed: true,
                },
            })
            .map_err(WalletOnboardingError::Recovery)
    }
}

impl<N, M, R, A> CancelWalletOnboardingUseCase for WalletOnboardingService<N, M, R, A>
where
    N: RandomPort + 'static,
    M: WalletMnemonicPort + 'static,
    R: RecoverWalletRootUseCase + 'static,
    A: WalletOnboardingAuthorizationPort + 'static,
{
    fn execute(&self, command: CancelWalletOnboardingCommand) -> Result<(), WalletOnboardingError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletOnboardingError::InvalidProfileIdentifier)?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| WalletOnboardingError::StateUnavailable)?;
        let matches = pending
            .get(&profile_id)
            .is_some_and(|value| value.ceremony_id == command.ceremony_id);
        if !matches {
            return Err(WalletOnboardingError::CeremonyNotFound);
        }
        pending.remove(&profile_id);
        Ok(())
    }

    fn suspend(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.clear();
        }
    }
}

#[derive(Default)]
struct SecretEntropy([u8; WALLET_ONBOARDING_ENTROPY_BYTES]);

impl Drop for SecretEntropy {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&mut self.0);
    }
}

fn encode_ceremony_id(bytes: [u8; WALLET_ONBOARDING_CEREMONY_ID_BYTES]) -> String {
    let mut value = String::with_capacity(18 + bytes.len() * 2);
    value.push_str("wallet_onboarding_");
    for byte in bytes {
        use fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    value
}

fn map_onboarding_confirmation(
    result: Result<(), SensitiveWalletOperationError>,
) -> Result<(), WalletOnboardingError> {
    result.map_err(|error| match error {
        SensitiveWalletOperationError::ConfirmationRequired => {
            WalletOnboardingError::ConfirmationRequired
        }
        _ => WalletOnboardingError::InvalidConfirmation,
    })
}

fn map_mnemonic_error(error: WalletMnemonicPortError) -> WalletOnboardingError {
    match error {
        WalletMnemonicPortError::InvalidPhrase => WalletOnboardingError::InvalidMnemonic,
        WalletMnemonicPortError::PhraseNotNormalized => {
            WalletOnboardingError::MnemonicMustBeNormalized
        }
        WalletMnemonicPortError::InvalidWordCount => {
            WalletOnboardingError::MnemonicMustContainTwentyFourWords
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{BIP39_WALLET_SEED_BYTES, WalletRootSeedKind};

    const PUBLIC_ZERO_ENTROPY_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

    struct FixedRandom {
        calls: Mutex<usize>,
    }

    impl RandomPort for FixedRandom {
        fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
            let mut calls = self.calls.lock().expect("calls");
            destination.fill(u8::try_from(*calls).expect("bounded calls"));
            *calls += 1;
            Ok(())
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct TestMnemonic;

    impl WalletMnemonicPort for TestMnemonic {
        fn create_from_entropy(
            &self,
            entropy: &[u8; WALLET_ONBOARDING_ENTROPY_BYTES],
        ) -> Result<CreatedWalletMnemonic, WalletMnemonicPortError> {
            Ok(CreatedWalletMnemonic {
                phrase: WalletRecoveryPhrase::new(PUBLIC_ZERO_ENTROPY_PHRASE.to_owned()),
                root: WalletRootSeed::from_bip39_seed([entropy[0]; BIP39_WALLET_SEED_BYTES]),
            })
        }

        fn restore_phrase(
            &self,
            phrase: &WalletRecoveryPhrase,
        ) -> Result<WalletRootSeed, WalletMnemonicPortError> {
            let value = phrase.expose_for_onboarding();
            if value.split(' ').count() != 24 {
                return Err(WalletMnemonicPortError::InvalidWordCount);
            }
            if value != PUBLIC_ZERO_ENTROPY_PHRASE {
                return Err(WalletMnemonicPortError::InvalidPhrase);
            }
            Ok(WalletRootSeed::from_bip39_seed(
                [0x42; BIP39_WALLET_SEED_BYTES],
            ))
        }
    }

    #[derive(Default)]
    struct RecordingAuthorization {
        denied: Mutex<bool>,
        calls: Mutex<usize>,
    }

    impl WalletOnboardingAuthorizationPort for RecordingAuthorization {
        fn authorize_recovery_phrase_reveal(
            &self,
        ) -> Result<(), WalletOnboardingAuthorizationError> {
            *self.calls.lock().expect("calls") += 1;
            if *self.denied.lock().expect("denied") {
                Err(WalletOnboardingAuthorizationError::Denied)
            } else {
                Ok(())
            }
        }
    }

    #[derive(Default)]
    struct RecordingRecovery {
        roots: Mutex<Vec<(WalletRootSeedKind, Vec<u8>)>>,
        fail: Mutex<bool>,
    }

    impl RecoverWalletRootUseCase for RecordingRecovery {
        fn execute(
            &self,
            command: RecoverWalletRootCommand,
        ) -> Result<WalletRootRecoveryView, WalletRootRecoveryError> {
            if *self.fail.lock().expect("failure") {
                return Err(WalletRootRecoveryError::ProfileNotEmpty);
            }
            assert_eq!(command.confirmation.title, RECOVER_WALLET_ROOT_TITLE);
            assert_eq!(command.confirmation.summary, RECOVER_WALLET_ROOT_SUMMARY);
            assert!(command.confirmation.confirmed);
            self.roots.lock().expect("roots").push((
                command.root.kind(),
                command.root.expose_for_protected_use().to_vec(),
            ));
            Ok(WalletRootRecoveryView {
                network_id: "standalone".to_owned(),
                account_index: 0,
                address_index: 0,
                canonical_account_ready: true,
            })
        }
    }

    fn service() -> (
        WalletOnboardingService<
            FixedRandom,
            TestMnemonic,
            RecordingRecovery,
            RecordingAuthorization,
        >,
        Arc<RecordingRecovery>,
        Arc<RecordingAuthorization>,
    ) {
        let recovery = Arc::new(RecordingRecovery::default());
        let authorization = Arc::new(RecordingAuthorization::default());
        (
            WalletOnboardingService::new(
                Arc::new(FixedRandom {
                    calls: Mutex::new(0),
                }),
                Arc::new(TestMnemonic),
                Arc::clone(&recovery),
                Arc::clone(&authorization),
            ),
            recovery,
            authorization,
        )
    }

    fn confirmation(confirmed: bool) -> SensitiveOperationConfirmation {
        SensitiveOperationConfirmation {
            title: COMPLETE_WALLET_ONBOARDING_TITLE.to_owned(),
            summary: COMPLETE_WALLET_ONBOARDING_SUMMARY.to_owned(),
            confirmed,
        }
    }

    fn prepare_create(service: &impl PrepareWalletOnboardingUseCase) -> PreparedWalletOnboarding {
        service
            .execute(PrepareWalletOnboardingCommand {
                profile_id: "profile_onboarding".to_owned(),
                mode: WalletOnboardingMode::CreateNew,
            })
            .expect("prepare")
    }

    #[test]
    fn create_uses_public_256_bit_vector_and_installs_full_seed_after_acknowledgement() {
        let (service, recovery, _) = service();
        let prepared = prepare_create(&service);
        let phrase = prepared
            .created_recovery_phrase
            .as_ref()
            .expect("created phrase");
        assert_eq!(phrase.expose_for_onboarding(), PUBLIC_ZERO_ENTROPY_PHRASE);
        assert_eq!(format!("{phrase:?}"), "WalletRecoveryPhrase([REDACTED])");
        let view = CompleteWalletOnboardingUseCase::execute(
            &service,
            CompleteWalletOnboardingCommand {
                profile_id: "profile_onboarding".to_owned(),
                ceremony_id: prepared.ceremony_id,
                backup_acknowledged: true,
                confirmation: confirmation(true),
            },
        )
        .expect("complete");

        assert_eq!((view.account_index, view.address_index), (0, 0));
        assert!(view.canonical_account_ready);
        let roots = recovery.roots.lock().expect("roots");
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].0, WalletRootSeedKind::Bip39);
        assert_eq!(roots[0].1, [0; BIP39_WALLET_SEED_BYTES]);
    }

    #[test]
    fn restore_requires_exact_normalized_checksum_valid_twenty_four_words() {
        let (service, _, _) = service();
        let invalid_phrases = [
            (
                "abandon abandon abandon".to_owned(),
                WalletOnboardingError::MnemonicMustContainTwentyFourWords,
            ),
            (
                PUBLIC_ZERO_ENTROPY_PHRASE.replace(" art", " abandon"),
                WalletOnboardingError::InvalidMnemonic,
            ),
            (
                PUBLIC_ZERO_ENTROPY_PHRASE.replace(' ', "  "),
                WalletOnboardingError::MnemonicMustContainTwentyFourWords,
            ),
        ];
        for (phrase, expected) in invalid_phrases {
            let error = PrepareWalletOnboardingUseCase::execute(
                &service,
                PrepareWalletOnboardingCommand {
                    profile_id: "profile_onboarding".to_owned(),
                    mode: WalletOnboardingMode::RestoreMnemonic {
                        phrase: WalletRecoveryPhrase::new(phrase),
                    },
                },
            )
            .expect_err("invalid phrase");
            assert_eq!(error, expected);
            assert!(!error.to_string().contains("abandon"));
        }
    }

    #[test]
    fn restored_public_phrase_uses_the_same_complete_bip39_seed() {
        let (service, recovery, _) = service();
        let prepared = PrepareWalletOnboardingUseCase::execute(
            &service,
            PrepareWalletOnboardingCommand {
                profile_id: "profile_onboarding".to_owned(),
                mode: WalletOnboardingMode::RestoreMnemonic {
                    phrase: WalletRecoveryPhrase::new(PUBLIC_ZERO_ENTROPY_PHRASE.to_owned()),
                },
            },
        )
        .expect("restore prepare");
        assert!(prepared.created_recovery_phrase.is_none());
        CompleteWalletOnboardingUseCase::execute(
            &service,
            CompleteWalletOnboardingCommand {
                profile_id: "profile_onboarding".to_owned(),
                ceremony_id: prepared.ceremony_id,
                backup_acknowledged: true,
                confirmation: confirmation(true),
            },
        )
        .expect("restore complete");

        let roots = recovery.roots.lock().expect("roots");
        assert_eq!(
            roots.as_slice(),
            &[(
                WalletRootSeedKind::Bip39,
                vec![0x42; BIP39_WALLET_SEED_BYTES]
            )]
        );
    }

    #[test]
    fn acknowledgement_and_confirmation_gate_completion_without_exposing_root() {
        let (service, recovery, _) = service();
        let prepared = prepare_create(&service);
        let command = |acknowledged, confirmed| CompleteWalletOnboardingCommand {
            profile_id: "profile_onboarding".to_owned(),
            ceremony_id: prepared.ceremony_id.clone(),
            backup_acknowledged: acknowledged,
            confirmation: confirmation(confirmed),
        };
        assert_eq!(
            CompleteWalletOnboardingUseCase::execute(&service, command(true, false)),
            Err(WalletOnboardingError::ConfirmationRequired)
        );
        assert_eq!(
            CompleteWalletOnboardingUseCase::execute(&service, command(false, true)),
            Err(WalletOnboardingError::BackupAcknowledgementRequired)
        );
        CompleteWalletOnboardingUseCase::execute(&service, command(true, true)).expect("complete");
        assert_eq!(
            CompleteWalletOnboardingUseCase::execute(&service, command(true, true)),
            Err(WalletOnboardingError::CeremonyNotFound)
        );
        assert_eq!(recovery.roots.lock().expect("roots").len(), 1);
    }

    #[test]
    fn duplicate_cancel_suspend_and_failed_recovery_are_fail_closed() {
        let (service, recovery, _) = service();
        let first = prepare_create(&service);
        assert!(matches!(
            PrepareWalletOnboardingUseCase::execute(
                &service,
                PrepareWalletOnboardingCommand {
                    profile_id: "profile_onboarding".to_owned(),
                    mode: WalletOnboardingMode::CreateNew,
                }
            ),
            Err(WalletOnboardingError::CeremonyAlreadyPending)
        ));
        CancelWalletOnboardingUseCase::execute(
            &service,
            CancelWalletOnboardingCommand {
                profile_id: "profile_onboarding".to_owned(),
                ceremony_id: first.ceremony_id,
            },
        )
        .expect("cancel");

        let suspended = prepare_create(&service);
        CancelWalletOnboardingUseCase::suspend(&service);
        assert_eq!(
            CompleteWalletOnboardingUseCase::execute(
                &service,
                CompleteWalletOnboardingCommand {
                    profile_id: "profile_onboarding".to_owned(),
                    ceremony_id: suspended.ceremony_id,
                    backup_acknowledged: true,
                    confirmation: confirmation(true),
                }
            ),
            Err(WalletOnboardingError::CeremonyNotFound)
        );

        let failed = prepare_create(&service);
        *recovery.fail.lock().expect("failure") = true;
        assert!(matches!(
            CompleteWalletOnboardingUseCase::execute(
                &service,
                CompleteWalletOnboardingCommand {
                    profile_id: "profile_onboarding".to_owned(),
                    ceremony_id: failed.ceremony_id.clone(),
                    backup_acknowledged: true,
                    confirmation: confirmation(true),
                }
            ),
            Err(WalletOnboardingError::Recovery(_))
        ));
        assert_eq!(
            CompleteWalletOnboardingUseCase::execute(
                &service,
                CompleteWalletOnboardingCommand {
                    profile_id: "profile_onboarding".to_owned(),
                    ceremony_id: failed.ceremony_id,
                    backup_acknowledged: true,
                    confirmation: confirmation(true),
                }
            ),
            Err(WalletOnboardingError::CeremonyNotFound)
        );
        assert!(recovery.roots.lock().expect("roots").is_empty());
    }

    #[test]
    fn restart_discards_pending_root_and_requires_a_fresh_ceremony() {
        let (service, recovery, authorization) = service();
        let prepared = prepare_create(&service);
        drop(service);
        let restarted = WalletOnboardingService::new(
            Arc::new(FixedRandom {
                calls: Mutex::new(0),
            }),
            Arc::new(TestMnemonic),
            Arc::clone(&recovery),
            Arc::clone(&authorization),
        );

        assert_eq!(
            CompleteWalletOnboardingUseCase::execute(
                &restarted,
                CompleteWalletOnboardingCommand {
                    profile_id: "profile_onboarding".to_owned(),
                    ceremony_id: prepared.ceremony_id,
                    backup_acknowledged: true,
                    confirmation: confirmation(true),
                }
            ),
            Err(WalletOnboardingError::CeremonyNotFound)
        );
        assert!(recovery.roots.lock().expect("roots").is_empty());
    }

    #[test]
    fn denied_presence_never_prepares_or_exposes_a_phrase() {
        let (service, recovery, authorization) = service();
        *authorization.denied.lock().expect("denied") = true;
        assert!(matches!(
            PrepareWalletOnboardingUseCase::execute(
                &service,
                PrepareWalletOnboardingCommand {
                    profile_id: "profile_onboarding".to_owned(),
                    mode: WalletOnboardingMode::CreateNew,
                },
            ),
            Err(WalletOnboardingError::UserPresence(
                WalletOnboardingAuthorizationError::Denied
            ))
        ));
        assert!(recovery.roots.lock().expect("roots").is_empty());
        assert_eq!(*authorization.calls.lock().expect("calls"), 1);
    }

    #[test]
    fn command_and_result_debug_never_reveal_the_phrase() {
        let command = PrepareWalletOnboardingCommand {
            profile_id: "profile_onboarding".to_owned(),
            mode: WalletOnboardingMode::RestoreMnemonic {
                phrase: WalletRecoveryPhrase::new(PUBLIC_ZERO_ENTROPY_PHRASE.to_owned()),
            },
        };
        let debug = format!("{command:?}");
        assert!(!debug.contains("abandon"));
        assert!(debug.contains("REDACTED"));
    }
}
