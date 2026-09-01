// SPDX-License-Identifier: Apache-2.0

//! One-shot recovery of an owner-supplied Midnight root into empty custody.
//!
//! The application owns confirmation, empty-profile, and authenticated-network
//! binding policy. Concrete custody adapters alone receive the typed root and
//! must install it atomically behind their user-presence boundary.

use std::{error::Error, fmt, sync::Arc};

use crate::{
    SensitiveOperationConfirmation, SensitiveWalletOperationError, WalletAccountDerivationPort,
    WalletAccountPortError, WalletNetworkPort, WalletProfileAssociationRepository,
    WalletProfileAssociationRepositoryError, WalletProfileRepository, WalletProfileRepositoryError,
    WalletProtectionPort, WalletSecurityPortError, validate_confirmation,
};
use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::{ChainNetworkId, WalletProfileId, WalletProtectionState};

pub const RECOVER_WALLET_ROOT_TITLE: &str = "Recover PreProd wallet";
pub const RECOVER_WALLET_ROOT_SUMMARY: &str = "Install the owner root into this empty protected profile and derive its canonical PreProd account.";

/// Canonical owner input: exactly 32 bytes encoded as 64 lowercase hexadecimal
/// characters. Formatting and debugging never expose the value.
pub struct WalletRootSeed([u8; 32]);

impl WalletRootSeed {
    pub fn parse_hex(value: &str) -> Result<Self, WalletRootSeedError> {
        let bytes = value.as_bytes();
        if bytes.len() != 64 || bytes.iter().any(|byte| !byte.is_ascii_hexdigit()) {
            return Err(WalletRootSeedError::InvalidEncoding);
        }
        if bytes.iter().any(u8::is_ascii_uppercase) {
            return Err(WalletRootSeedError::NonCanonicalEncoding);
        }

        let mut decoded = [0_u8; 32];
        for (index, pair) in bytes.chunks_exact(2).enumerate() {
            decoded[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
        }
        Ok(Self(decoded))
    }

    /// Copies the root only for the selected protected adapter. The adapter
    /// must immediately place the copy in its own zeroizing container.
    #[must_use]
    pub fn copy_for_protected_import(&self) -> [u8; 32] {
        self.0
    }
}

impl Drop for WalletRootSeed {
    fn drop(&mut self) {
        self.0.fill(0);
        // Keep the cleared value observable without introducing a low-level
        // core-layer memory exception. Concrete adapters additionally use the
        // audited `zeroize` crate for every copied value.
        std::hint::black_box(&mut self.0);
    }
}

impl fmt::Debug for WalletRootSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletRootSeed([REDACTED])")
    }
}

fn hex_nibble(value: u8) -> Result<u8, WalletRootSeedError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(WalletRootSeedError::InvalidEncoding),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRootSeedError {
    InvalidEncoding,
    NonCanonicalEncoding,
}

impl fmt::Display for WalletRootSeedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEncoding => "wallet root must contain exactly 64 hexadecimal characters",
            Self::NonCanonicalEncoding => "wallet root must use lowercase hexadecimal characters",
        })
    }
}

impl Error for WalletRootSeedError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletRootRecoveryConfigurationError {
    InvalidNetworkIdentifier,
}

impl fmt::Display for WalletRootRecoveryConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("wallet-root recovery requires a valid authenticated network")
    }
}

impl Error for WalletRootRecoveryConfigurationError {}

/// Outgoing one-shot custody boundary. Implementations must require a fresh
/// native authorization and refuse every initialized destination.
pub trait WalletRootRecoveryPort: Send + Sync {
    fn recover_root(
        &self,
        profile_id: &WalletProfileId,
        root: WalletRootSeed,
    ) -> Result<(), WalletSecurityPortError>;
}

pub struct RecoverWalletRootCommand {
    pub profile_id: String,
    pub root: WalletRootSeed,
    pub confirmation: SensitiveOperationConfirmation,
}

impl fmt::Debug for RecoverWalletRootCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoverWalletRootCommand")
            .field("profile_id", &self.profile_id)
            .field("root", &"[REDACTED]")
            .field("confirmation", &self.confirmation)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRootRecoveryView {
    pub network_id: String,
    pub account_index: u32,
    pub address_index: u32,
}

pub trait RecoverWalletRootUseCase: Send + Sync {
    fn execute(
        &self,
        command: RecoverWalletRootCommand,
    ) -> Result<WalletRootRecoveryView, WalletRootRecoveryError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletRootRecoveryError {
    InvalidProfileIdentifier(OpaqueIdError),
    ConfirmationRequired,
    InvalidConfirmation,
    ProfileNotFound,
    ProfileNotEmpty,
    ProfileStorage(WalletProfileRepositoryError),
    AssociationStorage(WalletProfileAssociationRepositoryError),
    Network(WalletAccountPortError),
    Custody(WalletSecurityPortError),
}

impl fmt::Display for WalletRootRecoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
            Self::ConfirmationRequired => {
                formatter.write_str("explicit wallet-root recovery confirmation is required")
            }
            Self::InvalidConfirmation => {
                formatter.write_str("wallet-root recovery confirmation is invalid")
            }
            Self::ProfileNotFound => formatter.write_str("wallet profile was not found"),
            Self::ProfileNotEmpty => {
                formatter.write_str("wallet-root recovery requires an empty wallet profile")
            }
            Self::ProfileStorage(error) => error.fmt(formatter),
            Self::AssociationStorage(error) => error.fmt(formatter),
            Self::Network(error) => error.fmt(formatter),
            Self::Custody(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletRootRecoveryError {}

/// Application policy for a composition-authenticated network. The network is
/// constructor state, not owner input, so a seed cannot be rebound by changing
/// a UI field or runtime environment variable.
pub struct WalletRootRecoveryService<R, P, N> {
    profiles: Arc<R>,
    protection: Arc<P>,
    networks: Arc<N>,
    network_id: ChainNetworkId,
}

impl<R, P, N> WalletRootRecoveryService<R, P, N> {
    pub fn new(
        profiles: Arc<R>,
        protection: Arc<P>,
        networks: Arc<N>,
        network_id: String,
    ) -> Result<Self, WalletRootRecoveryConfigurationError> {
        let network_id = ChainNetworkId::parse(network_id)
            .map_err(|_| WalletRootRecoveryConfigurationError::InvalidNetworkIdentifier)?;
        Ok(Self {
            profiles,
            protection,
            networks,
            network_id,
        })
    }
}

impl<R, P, N> RecoverWalletRootUseCase for WalletRootRecoveryService<R, P, N>
where
    R: WalletProfileRepository + WalletProfileAssociationRepository + 'static,
    P: WalletProtectionPort + WalletRootRecoveryPort + 'static,
    N: WalletNetworkPort + WalletAccountDerivationPort + 'static,
{
    fn execute(
        &self,
        command: RecoverWalletRootCommand,
    ) -> Result<WalletRootRecoveryView, WalletRootRecoveryError> {
        map_confirmation(validate_confirmation(&command.confirmation))?;
        if command.confirmation.title != RECOVER_WALLET_ROOT_TITLE
            || command.confirmation.summary != RECOVER_WALLET_ROOT_SUMMARY
        {
            return Err(WalletRootRecoveryError::InvalidConfirmation);
        }
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletRootRecoveryError::InvalidProfileIdentifier)?;
        let exists = self
            .profiles
            .list()
            .map_err(WalletRootRecoveryError::ProfileStorage)?
            .iter()
            .any(|profile| profile.id() == &profile_id);
        if !exists {
            return Err(WalletRootRecoveryError::ProfileNotFound);
        }

        let status = self
            .protection
            .status(&profile_id)
            .map_err(WalletRootRecoveryError::Custody)?;
        if status.state() != WalletProtectionState::Uninitialized {
            return Err(WalletRootRecoveryError::ProfileNotEmpty);
        }

        if let Some(associations) = self
            .profiles
            .load_associations(&profile_id)
            .map_err(WalletRootRecoveryError::AssociationStorage)?
        {
            // A denied native authorization may leave only this public,
            // authenticated network selection staged for a safe retry.
            if associations.selected_network_id() != &self.network_id
                || !associations.accounts().is_empty()
            {
                return Err(WalletRootRecoveryError::ProfileNotEmpty);
            }
        }

        let supported = self
            .networks
            .available_networks()
            .map_err(WalletRootRecoveryError::Network)?
            .iter()
            .any(|network| network.id() == &self.network_id);
        if !supported {
            return Err(WalletRootRecoveryError::Network(
                WalletAccountPortError::UnsupportedNetwork,
            ));
        }
        self.networks
            .select_network(&profile_id, &self.network_id)
            .map_err(WalletRootRecoveryError::Network)?;
        self.protection
            .recover_root(&profile_id, command.root)
            .map_err(WalletRootRecoveryError::Custody)?;
        self.networks
            .derive_account(&profile_id, 0, 0)
            .map_err(WalletRootRecoveryError::Network)?;

        Ok(WalletRootRecoveryView {
            network_id: self.network_id.as_str().to_owned(),
            account_index: 0,
            address_index: 0,
        })
    }
}

fn map_confirmation(
    result: Result<(), SensitiveWalletOperationError>,
) -> Result<(), WalletRootRecoveryError> {
    result.map_err(|error| match error {
        SensitiveWalletOperationError::ConfirmationRequired => {
            WalletRootRecoveryError::ConfirmationRequired
        }
        _ => WalletRootRecoveryError::InvalidConfirmation,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{
        ChainAccountId, ChainAddress, ChainAddressKind, ChainKind, ChainNetwork,
        DerivedChainAccount, NetworkDisplayName, NetworkEnvironment, ProfileName,
        PublicKeyEncoding, WalletKeyReference, WalletProfile, WalletProtectionClass,
        WalletPublicKey, WalletSecurityStatus,
    };

    use super::*;

    #[test]
    fn seed_format_is_canonical_and_debug_is_redacted() {
        let secret = "01".repeat(32);
        let root = WalletRootSeed::parse_hex(&secret).expect("root");
        assert_eq!(format!("{root:?}"), "WalletRootSeed([REDACTED])");
        for invalid in [
            String::new(),
            "01".to_owned(),
            "GG".repeat(32),
            "AB".repeat(32),
            format!("{secret}\n"),
        ] {
            let error = WalletRootSeed::parse_hex(&invalid).expect_err("invalid root");
            if !invalid.is_empty() {
                assert!(!error.to_string().contains(&invalid));
            }
        }
    }

    struct TestState {
        profile: WalletProfile,
        associations: Mutex<Option<crate::WalletProfileAssociations>>,
        status: Mutex<WalletSecurityStatus>,
        recovered: Mutex<Option<[u8; 32]>>,
        deny_recovery: Mutex<bool>,
        derive_count: Mutex<u8>,
        network: ChainNetwork,
    }

    impl TestState {
        fn new() -> Self {
            let profile_id = WalletProfileId::parse("profile_recover").expect("profile id");
            Self {
                profile: WalletProfile::new(
                    profile_id,
                    ProfileName::parse("Recovered wallet").expect("name"),
                    UnixTimestampMillis::new(1),
                ),
                associations: Mutex::new(None),
                status: Mutex::new(WalletSecurityStatus::new(
                    WalletProtectionState::Uninitialized,
                    WalletProtectionClass::HardwareBacked,
                    true,
                    true,
                )),
                recovered: Mutex::new(None),
                deny_recovery: Mutex::new(false),
                derive_count: Mutex::new(0),
                network: ChainNetwork::new(
                    ChainKind::Midnight,
                    ChainNetworkId::parse("preprod").expect("network id"),
                    NetworkDisplayName::parse("Midnight PreProd").expect("network name"),
                    NetworkEnvironment::PublicTest,
                ),
            }
        }
    }

    impl WalletProfileRepository for TestState {
        fn save(&self, _: WalletProfile) -> Result<(), WalletProfileRepositoryError> {
            Err(WalletProfileRepositoryError::Unavailable)
        }

        fn list(&self) -> Result<Vec<WalletProfile>, WalletProfileRepositoryError> {
            Ok(vec![self.profile.clone()])
        }

        fn remove(&self, _: &WalletProfileId) -> Result<(), WalletProfileRepositoryError> {
            Err(WalletProfileRepositoryError::Unavailable)
        }

        fn set_active(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletProfile, WalletProfileRepositoryError> {
            Ok(self.profile.clone())
        }

        fn active(&self) -> Result<Option<WalletProfile>, WalletProfileRepositoryError> {
            Ok(Some(self.profile.clone()))
        }
    }

    impl WalletProfileAssociationRepository for TestState {
        fn load_associations(
            &self,
            _: &WalletProfileId,
        ) -> Result<Option<crate::WalletProfileAssociations>, WalletProfileAssociationRepositoryError>
        {
            Ok(self.associations.lock().expect("associations").clone())
        }

        fn save_associations(
            &self,
            _: &WalletProfileId,
            associations: crate::WalletProfileAssociations,
        ) -> Result<(), WalletProfileAssociationRepositoryError> {
            *self.associations.lock().expect("associations") = Some(associations);
            Ok(())
        }

        fn remove_associations(
            &self,
            _: &WalletProfileId,
        ) -> Result<(), WalletProfileAssociationRepositoryError> {
            *self.associations.lock().expect("associations") = None;
            Ok(())
        }
    }

    impl WalletProtectionPort for TestState {
        fn status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
            Ok(*self.status.lock().expect("status"))
        }

        fn initialize(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
            Err(WalletSecurityPortError::InvalidOperation)
        }

        fn unlock(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
            Err(WalletSecurityPortError::InvalidOperation)
        }

        fn lock(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
            Err(WalletSecurityPortError::InvalidOperation)
        }
    }

    impl WalletRootRecoveryPort for TestState {
        fn recover_root(
            &self,
            _: &WalletProfileId,
            root: WalletRootSeed,
        ) -> Result<(), WalletSecurityPortError> {
            if *self.deny_recovery.lock().expect("denial") {
                return Err(WalletSecurityPortError::AuthorizationDenied);
            }
            *self.recovered.lock().expect("root") = Some(root.copy_for_protected_import());
            *self.status.lock().expect("status") = WalletSecurityStatus::new(
                WalletProtectionState::Unlocked,
                WalletProtectionClass::HardwareBacked,
                true,
                true,
            );
            Ok(())
        }
    }

    impl WalletNetworkPort for TestState {
        fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
            Ok(vec![self.network.clone()])
        }

        fn selected_network(
            &self,
            profile_id: &WalletProfileId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            self.load_associations(profile_id)
                .map_err(|_| WalletAccountPortError::Unavailable)?
                .map(|value| value.selected_network_id().clone())
                .ok_or(WalletAccountPortError::NotFound)
        }

        fn select_network(
            &self,
            profile_id: &WalletProfileId,
            network_id: &ChainNetworkId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            self.save_associations(
                profile_id,
                crate::WalletProfileAssociations::new(network_id.clone(), Vec::new())
                    .map_err(|_| WalletAccountPortError::InvalidData)?,
            )
            .map_err(|_| WalletAccountPortError::Unavailable)?;
            Ok(network_id.clone())
        }
    }

    impl WalletAccountDerivationPort for TestState {
        fn derive_account(
            &self,
            _: &WalletProfileId,
            account_index: u32,
            address_index: u32,
        ) -> Result<DerivedChainAccount, WalletAccountPortError> {
            *self.derive_count.lock().expect("derive count") += 1;
            DerivedChainAccount::new(
                self.network.id().clone(),
                ChainAccountId::parse("account_preprod_0").expect("account id"),
                account_index,
                address_index,
                ChainAddress::parse(ChainAddressKind::Unshielded, "mn_addr_preprod1owner")
                    .expect("address"),
                WalletPublicKey::new(PublicKeyEncoding::Secp256k1XOnly, vec![7; 32]),
                WalletKeyReference::parse("key_preprod_owner").expect("key reference"),
            )
            .map_err(|_| WalletAccountPortError::InvalidData)
        }
    }

    fn confirmation(confirmed: bool) -> SensitiveOperationConfirmation {
        SensitiveOperationConfirmation {
            title: RECOVER_WALLET_ROOT_TITLE.to_owned(),
            summary: RECOVER_WALLET_ROOT_SUMMARY.to_owned(),
            confirmed,
        }
    }

    fn command(confirmed: bool) -> RecoverWalletRootCommand {
        RecoverWalletRootCommand {
            profile_id: "profile_recover".to_owned(),
            root: WalletRootSeed::parse_hex(&"11".repeat(32)).expect("root"),
            confirmation: confirmation(confirmed),
        }
    }

    #[test]
    fn exact_confirmation_precedes_network_or_custody_changes() {
        let state = Arc::new(TestState::new());
        let service = WalletRootRecoveryService::new(
            Arc::clone(&state),
            Arc::clone(&state),
            Arc::clone(&state),
            "preprod".to_owned(),
        )
        .expect("service");
        assert_eq!(
            service.execute(command(false)),
            Err(WalletRootRecoveryError::ConfirmationRequired)
        );
        let mut wrong_intent = command(true);
        wrong_intent.confirmation.title = "Recover wallet".to_owned();
        assert_eq!(
            service.execute(wrong_intent),
            Err(WalletRootRecoveryError::InvalidConfirmation)
        );
        assert!(state.associations.lock().expect("associations").is_none());
        assert!(state.recovered.lock().expect("root").is_none());
    }

    #[test]
    fn recovery_command_debug_never_contains_the_root() {
        let command = command(true);
        let rendered = format!("{command:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains(&"11".repeat(32)));
    }

    #[test]
    fn recovery_configuration_rejects_an_invalid_authenticated_network() {
        let state = Arc::new(TestState::new());
        let result = WalletRootRecoveryService::new(
            Arc::clone(&state),
            Arc::clone(&state),
            Arc::clone(&state),
            "preprod network".to_owned(),
        );
        assert!(matches!(
            result,
            Err(WalletRootRecoveryConfigurationError::InvalidNetworkIdentifier)
        ));
    }

    #[test]
    fn authorization_denial_is_retryable_but_duplicate_import_is_rejected() {
        let state = Arc::new(TestState::new());
        let service = WalletRootRecoveryService::new(
            Arc::clone(&state),
            Arc::clone(&state),
            Arc::clone(&state),
            "preprod".to_owned(),
        )
        .expect("service");
        *state.deny_recovery.lock().expect("denial") = true;
        assert_eq!(
            service.execute(command(true)),
            Err(WalletRootRecoveryError::Custody(
                WalletSecurityPortError::AuthorizationDenied
            ))
        );
        let staged = state
            .associations
            .lock()
            .expect("associations")
            .clone()
            .expect("staged network");
        assert_eq!(staged.selected_network_id().as_str(), "preprod");
        assert!(staged.accounts().is_empty());

        *state.deny_recovery.lock().expect("denial") = false;
        let recovered = service.execute(command(true)).expect("authorized retry");
        assert_eq!(recovered.network_id, "preprod");
        assert_eq!(recovered.account_index, 0);
        assert_eq!(recovered.address_index, 0);
        assert!(state.recovered.lock().expect("root").is_some());
        assert_eq!(*state.derive_count.lock().expect("derive count"), 1);
        assert_eq!(
            service.execute(command(true)),
            Err(WalletRootRecoveryError::ProfileNotEmpty)
        );
    }

    #[test]
    fn initialized_custody_and_existing_accounts_fail_before_root_use() {
        let state = Arc::new(TestState::new());
        let service = WalletRootRecoveryService::new(
            Arc::clone(&state),
            Arc::clone(&state),
            Arc::clone(&state),
            "preprod".to_owned(),
        )
        .expect("service");
        *state.status.lock().expect("status") = WalletSecurityStatus::new(
            WalletProtectionState::Locked,
            WalletProtectionClass::HardwareBacked,
            true,
            true,
        );
        assert_eq!(
            service.execute(command(true)),
            Err(WalletRootRecoveryError::ProfileNotEmpty)
        );
        assert!(state.recovered.lock().expect("root").is_none());

        *state.status.lock().expect("status") = WalletSecurityStatus::new(
            WalletProtectionState::Uninitialized,
            WalletProtectionClass::HardwareBacked,
            true,
            true,
        );
        *state.associations.lock().expect("associations") = Some(
            crate::WalletProfileAssociations::new(
                ChainNetworkId::parse("preprod").expect("network"),
                vec![
                    crate::WalletAccountAssociation::new(
                        ChainNetworkId::parse("preprod").expect("network"),
                        0,
                        0,
                    )
                    .expect("association"),
                ],
            )
            .expect("associations"),
        );
        assert_eq!(
            service.execute(command(true)),
            Err(WalletRootRecoveryError::ProfileNotEmpty)
        );
        assert!(state.recovered.lock().expect("root").is_none());
    }
}
