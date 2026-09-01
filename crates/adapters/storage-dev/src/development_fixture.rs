// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_platform_ports::{ClockPort, RandomPort};
use oxid_wallet_application::{
    WalletProfileRepository, WalletProtectionPort, WalletSecurityPortError,
};
use oxid_wallet_domain::{WalletProfileId, WalletSecurityStatus};
use zeroize::Zeroizing;

use super::DevelopmentWalletSecurity;

/// Selects one uniquely named profile for an explicit development root fixture.
///
/// The root remains inside development custody. Profiles with any other name
/// continue to initialize from the adapter's random source, and an ambiguous
/// fixture name fails closed before custody changes. Because display names are
/// user-editable, assigning the fixture name is an explicit opt-in to the
/// shared root when that profile is initialized.
pub struct DevelopmentWalletFixtureProtection<R, C, N> {
    profiles: Arc<R>,
    security: Arc<DevelopmentWalletSecurity<C, N>>,
    profile_name: String,
    root_seed: Zeroizing<[u8; 32]>,
}

impl<R, C, N> DevelopmentWalletFixtureProtection<R, C, N> {
    #[must_use]
    pub fn new(
        profiles: Arc<R>,
        security: Arc<DevelopmentWalletSecurity<C, N>>,
        profile_name: impl Into<String>,
        root_seed: [u8; 32],
    ) -> Self {
        Self {
            profiles,
            security,
            profile_name: profile_name.into(),
            root_seed: Zeroizing::new(root_seed),
        }
    }
}

impl<R, C, N> WalletProtectionPort for DevelopmentWalletFixtureProtection<R, C, N>
where
    R: WalletProfileRepository + 'static,
    C: ClockPort + 'static,
    N: RandomPort + 'static,
{
    fn status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        self.security.status(profile_id)
    }

    fn initialize(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        let profiles = self
            .profiles
            .list()
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        let matching_profiles = profiles
            .iter()
            .filter(|profile| profile.display_name().as_str() == self.profile_name)
            .collect::<Vec<_>>();
        let requested_fixture_profile = matching_profiles
            .iter()
            .any(|profile| profile.id() == profile_id);
        if requested_fixture_profile && matching_profiles.len() > 1 {
            return Err(WalletSecurityPortError::Conflict);
        }
        if requested_fixture_profile {
            self.security
                .initialize_with_root_seed(profile_id, *self.root_seed)
        } else {
            self.security.initialize(profile_id)
        }
    }

    fn unlock(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        self.security.unlock(profile_id)
    }

    fn lock(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        self.security.lock(profile_id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
    use oxid_foundation::UnixTimestampMillis;
    use oxid_platform_ports::PlatformError;
    use oxid_wallet_application::{
        WalletDerivedSecretUsePort, WalletHdPath, WalletHdPathComponent,
    };
    use oxid_wallet_domain::{ProfileName, WalletProfile};

    use super::*;

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    struct IncrementingRandom(Mutex<u8>);

    impl RandomPort for IncrementingRandom {
        fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
            let mut value = self
                .0
                .lock()
                .map_err(|_| PlatformError::RandomnessUnavailable)?;
            destination.fill(*value);
            *value = value.wrapping_add(1).max(1);
            Ok(())
        }
    }

    fn adapter() -> DevelopmentWalletSecurity<FixedClock, IncrementingRandom> {
        DevelopmentWalletSecurity::new(
            Arc::new(FixedClock),
            Arc::new(IncrementingRandom(Mutex::new(17))),
        )
    }

    fn wallet_profile(identifier: &str, name: &str) -> WalletProfile {
        WalletProfile::new(
            WalletProfileId::parse(identifier).expect("profile identifier"),
            ProfileName::parse(name).expect("profile name"),
            UnixTimestampMillis::new(1_700_000_000_000),
        )
    }

    fn first_child(
        security: &DevelopmentWalletSecurity<FixedClock, IncrementingRandom>,
        profile_id: &WalletProfileId,
    ) -> [u8; 32] {
        let path = WalletHdPath::new(vec![
            WalletHdPathComponent::new(0, true).expect("child path"),
        ])
        .expect("child path");
        let mut child = [0_u8; 32];
        security
            .use_derived_secret(profile_id, &path, &mut |secret| {
                child.copy_from_slice(secret);
                Ok(())
            })
            .expect("derive protected test child");
        child
    }

    #[test]
    fn binds_the_root_to_one_unique_named_profile() {
        let profiles = Arc::new(InMemoryWalletProfileRepository::new());
        let ordinary = wallet_profile("profile_ordinary", "Ordinary wallet");
        let fixture = wallet_profile("profile_fixture", "Public fixture");
        profiles.save(ordinary.clone()).expect("save ordinary");
        profiles.save(fixture.clone()).expect("save fixture");
        let security = Arc::new(adapter());
        let protection = DevelopmentWalletFixtureProtection::new(
            Arc::clone(&profiles),
            Arc::clone(&security),
            "Public fixture",
            [1_u8; 32],
        );

        protection
            .initialize(ordinary.id())
            .expect("ordinary initialization");
        protection
            .initialize(fixture.id())
            .expect("fixture initialization");

        let expected = adapter();
        expected
            .initialize_with_root_seed(fixture.id(), [1_u8; 32])
            .expect("expected fixture root");
        assert_eq!(
            first_child(security.as_ref(), fixture.id()),
            first_child(&expected, fixture.id())
        );
        assert_ne!(
            first_child(security.as_ref(), ordinary.id()),
            first_child(security.as_ref(), fixture.id())
        );
    }

    #[test]
    fn rejects_duplicate_profile_names() {
        let profiles = Arc::new(InMemoryWalletProfileRepository::new());
        let first = wallet_profile("profile_first", "Public fixture");
        let ordinary = wallet_profile("profile_ordinary", "Ordinary wallet");
        profiles.save(first.clone()).expect("save first");
        profiles
            .save(wallet_profile("profile_second", "Public fixture"))
            .expect("save second");
        profiles.save(ordinary.clone()).expect("save ordinary");
        let protection = DevelopmentWalletFixtureProtection::new(
            profiles,
            Arc::new(adapter()),
            "Public fixture",
            [1_u8; 32],
        );

        assert_eq!(
            protection.initialize(first.id()),
            Err(WalletSecurityPortError::Conflict)
        );
        protection
            .initialize(ordinary.id())
            .expect("duplicate fixture names do not block ordinary profiles");
    }
}
