// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt, fmt::Write as _, sync::Arc};

use oxid_foundation::OpaqueIdError;
use oxid_platform_ports::{ClockPort, PlatformError, RandomPort};
use oxid_wallet_domain::{ProfileName, ProfileNameError, WalletProfile, WalletProfileId};

mod chain;
mod security;
mod transaction;

pub use chain::*;
pub use security::*;
pub use transaction::*;

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
    NotFound,
    Unavailable,
}

impl fmt::Display for WalletProfileRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Conflict => "wallet profile already exists",
            Self::NotFound => "wallet profile was not found",
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

    fn set_active(
        &self,
        id: &WalletProfileId,
    ) -> Result<WalletProfile, WalletProfileRepositoryError>;

    fn active(&self) -> Result<Option<WalletProfile>, WalletProfileRepositoryError>;
}

/// Incoming port consumed by UI, CLI, deep-link, and test adapters.
pub trait CreateWalletProfileUseCase: Send + Sync {
    fn execute(
        &self,
        command: CreateWalletProfileCommand,
    ) -> Result<WalletProfileView, CreateWalletProfileError>;
}

/// Incoming query for public wallet profile metadata.
pub trait ListWalletProfilesUseCase: Send + Sync {
    fn execute(&self) -> Result<Vec<WalletProfileView>, ReadWalletProfilesError>;
}

/// Input owned by the Select Wallet Profile application boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectWalletProfileCommand {
    pub profile_id: String,
}

/// Incoming command for choosing the active wallet profile.
pub trait SelectWalletProfileUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectWalletProfileCommand,
    ) -> Result<WalletProfileView, SelectWalletProfileError>;
}

/// Incoming query for the currently active wallet profile.
pub trait GetActiveWalletProfileUseCase: Send + Sync {
    fn execute(&self) -> Result<Option<WalletProfileView>, ReadWalletProfilesError>;
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

/// Structured failures for profile metadata queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadWalletProfilesError {
    Persistence(WalletProfileRepositoryError),
}

impl fmt::Display for ReadWalletProfilesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persistence(error) => error.fmt(formatter),
        }
    }
}

impl Error for ReadWalletProfilesError {}

/// Structured failures for selecting an active profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectWalletProfileError {
    InvalidIdentifier(OpaqueIdError),
    Persistence(WalletProfileRepositoryError),
}

impl fmt::Display for SelectWalletProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier(error) => error.fmt(formatter),
            Self::Persistence(error) => error.fmt(formatter),
        }
    }
}

impl Error for SelectWalletProfileError {}

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

/// Application service for listing public profile metadata.
pub struct ListWalletProfilesService<R> {
    repository: Arc<R>,
}

impl<R> ListWalletProfilesService<R> {
    #[must_use]
    pub const fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

impl<R> ListWalletProfilesUseCase for ListWalletProfilesService<R>
where
    R: WalletProfileRepository + 'static,
{
    fn execute(&self) -> Result<Vec<WalletProfileView>, ReadWalletProfilesError> {
        let mut profiles = self
            .repository
            .list()
            .map_err(ReadWalletProfilesError::Persistence)?;
        profiles.sort_by(|left, right| {
            left.created_at()
                .cmp(&right.created_at())
                .then_with(|| left.id().cmp(right.id()))
        });

        Ok(profiles.iter().map(WalletProfileView::from).collect())
    }
}

/// Application service for selecting the active wallet profile.
pub struct SelectWalletProfileService<R> {
    repository: Arc<R>,
}

impl<R> SelectWalletProfileService<R> {
    #[must_use]
    pub const fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

impl<R> SelectWalletProfileUseCase for SelectWalletProfileService<R>
where
    R: WalletProfileRepository + 'static,
{
    fn execute(
        &self,
        command: SelectWalletProfileCommand,
    ) -> Result<WalletProfileView, SelectWalletProfileError> {
        let id = WalletProfileId::parse(command.profile_id)
            .map_err(SelectWalletProfileError::InvalidIdentifier)?;
        self.repository
            .set_active(&id)
            .map(|profile| WalletProfileView::from(&profile))
            .map_err(SelectWalletProfileError::Persistence)
    }
}

/// Application service for restoring the selected wallet profile.
pub struct GetActiveWalletProfileService<R> {
    repository: Arc<R>,
}

impl<R> GetActiveWalletProfileService<R> {
    #[must_use]
    pub const fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

impl<R> GetActiveWalletProfileUseCase for GetActiveWalletProfileService<R>
where
    R: WalletProfileRepository + 'static,
{
    fn execute(&self) -> Result<Option<WalletProfileView>, ReadWalletProfilesError> {
        self.repository
            .active()
            .map(|profile| profile.as_ref().map(WalletProfileView::from))
            .map_err(ReadWalletProfilesError::Persistence)
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
        active_profile_id: Mutex<Option<String>>,
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

        fn set_active(
            &self,
            id: &WalletProfileId,
        ) -> Result<WalletProfile, WalletProfileRepositoryError> {
            let profiles = self
                .profiles
                .lock()
                .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
            let profile = profiles
                .iter()
                .find(|profile| profile.id() == id)
                .cloned()
                .ok_or(WalletProfileRepositoryError::NotFound)?;
            *self
                .active_profile_id
                .lock()
                .map_err(|_| WalletProfileRepositoryError::Unavailable)? =
                Some(id.as_str().to_owned());

            Ok(profile)
        }

        fn active(&self) -> Result<Option<WalletProfile>, WalletProfileRepositoryError> {
            let profiles = self
                .profiles
                .lock()
                .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
            let active_profile_id = self
                .active_profile_id
                .lock()
                .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
            let Some(active_profile_id) = active_profile_id.as_deref() else {
                return Ok(None);
            };

            profiles
                .iter()
                .find(|profile| profile.id().as_str() == active_profile_id)
                .cloned()
                .map(Some)
                .ok_or(WalletProfileRepositoryError::NotFound)
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

    #[test]
    fn lists_selects_and_restores_profiles_through_focused_use_cases() {
        let repository = Arc::new(RecordingRepository::default());
        let create = CreateWalletProfileService::new(
            Arc::clone(&repository),
            Arc::new(FixedClock),
            Arc::new(FixedRandom),
        );
        let list = ListWalletProfilesService::new(Arc::clone(&repository));
        let select = SelectWalletProfileService::new(Arc::clone(&repository));
        let active = GetActiveWalletProfileService::new(Arc::clone(&repository));

        let created = create
            .execute(CreateWalletProfileCommand {
                display_name: "Primary".to_owned(),
            })
            .expect("profile should be created");

        assert_eq!(
            list.execute().expect("profiles should load"),
            vec![created.clone()]
        );
        assert_eq!(active.execute().expect("active query should work"), None);
        assert_eq!(
            select
                .execute(SelectWalletProfileCommand {
                    profile_id: created.id.clone(),
                })
                .expect("profile should be selectable"),
            created.clone()
        );
        assert_eq!(
            active.execute().expect("selection should be restored"),
            Some(created)
        );
    }

    #[test]
    fn selecting_an_unknown_profile_returns_not_found() {
        let repository = Arc::new(RecordingRepository::default());
        let select = SelectWalletProfileService::new(repository);

        assert_eq!(
            select.execute(SelectWalletProfileCommand {
                profile_id: "profile_missing".to_owned(),
            }),
            Err(SelectWalletProfileError::Persistence(
                WalletProfileRepositoryError::NotFound
            ))
        );
    }
}
