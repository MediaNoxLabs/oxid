// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt, fmt::Write as _, sync::Arc};

use oxid_platform_ports::{ClockPort, PlatformError, RandomPort};
use oxid_wallet_domain::{ProfileName, ProfileNameError, WalletProfile, WalletProfileId};

/// Input owned by the Create Wallet Profile application boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateWalletProfileCommand {
    pub display_name: String,
}

/// Public metadata returned to incoming adapters after profile creation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletProfileView {
    pub id: String,
    pub display_name: String,
    pub created_at_millis: u64,
}

impl From<&WalletProfile> for WalletProfileView {
    fn from(profile: &WalletProfile) -> Self {
        Self {
            id: profile.id().as_str().to_owned(),
            display_name: profile.display_name().as_str().to_owned(),
            created_at_millis: profile.created_at().value(),
        }
    }
}

/// Stable persistence failures exposed by the wallet application boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletProfileRepositoryError {
    Conflict,
    Unavailable,
}

impl fmt::Display for WalletProfileRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Conflict => "wallet profile already exists",
            Self::Unavailable => "wallet profile storage is unavailable",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletProfileRepositoryError {}

/// Outgoing port owned by the wallet application module.
pub trait WalletProfileRepository: Send + Sync {
    fn save(&self, profile: WalletProfile) -> Result<(), WalletProfileRepositoryError>;

    fn list(&self) -> Result<Vec<WalletProfile>, WalletProfileRepositoryError>;
}

/// Incoming port consumed by UI, CLI, deep-link, and test adapters.
pub trait CreateWalletProfileUseCase: Send + Sync {
    fn execute(
        &self,
        command: CreateWalletProfileCommand,
    ) -> Result<WalletProfileView, CreateWalletProfileError>;
}

/// Structured failures for Create Wallet Profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateWalletProfileError {
    InvalidName(ProfileNameError),
    Platform(PlatformError),
    Persistence(WalletProfileRepositoryError),
    InvalidGeneratedIdentifier,
}

impl fmt::Display for CreateWalletProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(error) => error.fmt(formatter),
            Self::Platform(error) => error.fmt(formatter),
            Self::Persistence(error) => error.fmt(formatter),
            Self::InvalidGeneratedIdentifier => {
                formatter.write_str("generated wallet profile identifier is invalid")
            }
        }
    }
}

impl Error for CreateWalletProfileError {}

/// Application service for the first Oxid vertical slice.
pub struct CreateWalletProfileService<R, C, N> {
    repository: Arc<R>,
    clock: Arc<C>,
    random: Arc<N>,
}

impl<R, C, N> CreateWalletProfileService<R, C, N> {
    #[must_use]
    pub const fn new(repository: Arc<R>, clock: Arc<C>, random: Arc<N>) -> Self {
        Self {
            repository,
            clock,
            random,
        }
    }
}

impl<R, C, N> CreateWalletProfileUseCase for CreateWalletProfileService<R, C, N>
where
    R: WalletProfileRepository + 'static,
    C: ClockPort + 'static,
    N: RandomPort + 'static,
{
    fn execute(
        &self,
        command: CreateWalletProfileCommand,
    ) -> Result<WalletProfileView, CreateWalletProfileError> {
        let display_name = ProfileName::parse(command.display_name)
            .map_err(CreateWalletProfileError::InvalidName)?;

        let mut random_bytes = [0_u8; 16];
        self.random
            .fill_bytes(&mut random_bytes)
            .map_err(CreateWalletProfileError::Platform)?;
        let id = profile_id(random_bytes)?;
        let created_at = self
            .clock
            .now()
            .map_err(CreateWalletProfileError::Platform)?;

        let profile = WalletProfile::new(id, display_name, created_at);
        self.repository
            .save(profile.clone())
            .map_err(CreateWalletProfileError::Persistence)?;

        Ok(WalletProfileView::from(&profile))
    }
}

fn profile_id(mut bytes: [u8; 16]) -> Result<WalletProfileId, CreateWalletProfileError> {
    // RFC 9562 UUIDv4 variant/version bits make the random identifier familiar
    // without leaking a UUID crate type into Oxid's domain.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let mut value = String::with_capacity(44);
    value.push_str("profile_");
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            value.push('-');
        }
        write!(&mut value, "{byte:02x}").map_err(|_| {
            // Writing into a String is infallible, but retain a typed boundary
            // instead of panicking if that standard-library contract changes.
            CreateWalletProfileError::InvalidGeneratedIdentifier
        })?;
    }

    WalletProfileId::parse(value).map_err(|_| CreateWalletProfileError::InvalidGeneratedIdentifier)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use oxid_foundation::UnixTimestampMillis;

    use super::*;

    #[derive(Default)]
    struct RecordingRepository {
        profiles: Mutex<Vec<WalletProfile>>,
    }

    impl WalletProfileRepository for RecordingRepository {
        fn save(&self, profile: WalletProfile) -> Result<(), WalletProfileRepositoryError> {
            self.profiles
                .lock()
                .map_err(|_| WalletProfileRepositoryError::Unavailable)?
                .push(profile);
            Ok(())
        }

        fn list(&self) -> Result<Vec<WalletProfile>, WalletProfileRepositoryError> {
            self.profiles
                .lock()
                .map(|profiles| profiles.clone())
                .map_err(|_| WalletProfileRepositoryError::Unavailable)
        }
    }

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    struct FixedRandom;

    impl RandomPort for FixedRandom {
        fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
            destination.fill(0x11);
            Ok(())
        }
    }

    #[test]
    fn creates_and_persists_a_normalized_profile() {
        let repository = Arc::new(RecordingRepository::default());
        let service = CreateWalletProfileService::new(
            Arc::clone(&repository),
            Arc::new(FixedClock),
            Arc::new(FixedRandom),
        );

        let created = service
            .execute(CreateWalletProfileCommand {
                display_name: "  Primary wallet  ".to_owned(),
            })
            .expect("profile creation should succeed");

        assert_eq!(created.id, "profile_11111111-1111-4111-9111-111111111111");
        assert_eq!(created.display_name, "Primary wallet");
        assert_eq!(created.created_at_millis, 1_700_000_000_000);
        assert_eq!(
            repository
                .list()
                .expect("repository should be readable")
                .len(),
            1
        );
    }

    #[test]
    fn rejects_an_invalid_name_before_touching_adapters() {
        let service = CreateWalletProfileService::new(
            Arc::new(RecordingRepository::default()),
            Arc::new(FixedClock),
            Arc::new(FixedRandom),
        );

        assert_eq!(
            service.execute(CreateWalletProfileCommand {
                display_name: "  ".to_owned(),
            }),
            Err(CreateWalletProfileError::InvalidName(
                ProfileNameError::Empty
            ))
        );
    }
}
