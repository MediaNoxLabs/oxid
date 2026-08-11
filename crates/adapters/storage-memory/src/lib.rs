// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{collections::BTreeMap, sync::RwLock};

use oxid_wallet_application::{WalletProfileRepository, WalletProfileRepositoryError};
use oxid_wallet_domain::WalletProfile;

/// Process-local profile storage for development, demos, and tests.
///
/// This adapter is neither durable nor a secure secret store.
#[derive(Default)]
pub struct InMemoryWalletProfileRepository {
    profiles: RwLock<BTreeMap<String, WalletProfile>>,
}

impl InMemoryWalletProfileRepository {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl WalletProfileRepository for InMemoryWalletProfileRepository {
    fn save(&self, profile: WalletProfile) -> Result<(), WalletProfileRepositoryError> {
        let mut profiles = self
            .profiles
            .write()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        let identifier = profile.id().as_str().to_owned();
        if profiles.contains_key(&identifier) {
            return Err(WalletProfileRepositoryError::Conflict);
        }
        profiles.insert(identifier, profile);
        Ok(())
    }

    fn list(&self) -> Result<Vec<WalletProfile>, WalletProfileRepositoryError> {
        self.profiles
            .read()
            .map(|profiles| profiles.values().cloned().collect())
            .map_err(|_| WalletProfileRepositoryError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{ProfileName, WalletProfileId};

    use super::*;

    fn profile() -> WalletProfile {
        WalletProfile::new(
            WalletProfileId::parse("profile_test").expect("identifier should be valid"),
            ProfileName::parse("Test profile").expect("name should be valid"),
            UnixTimestampMillis::new(7),
        )
    }

    #[test]
    fn saves_and_lists_profiles() {
        let repository = InMemoryWalletProfileRepository::new();
        repository.save(profile()).expect("save should succeed");

        let profiles = repository.list().expect("list should succeed");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].display_name().as_str(), "Test profile");
    }

    #[test]
    fn rejects_duplicate_identifiers() {
        let repository = InMemoryWalletProfileRepository::new();
        repository
            .save(profile())
            .expect("first save should succeed");

        assert_eq!(
            repository.save(profile()),
            Err(WalletProfileRepositoryError::Conflict)
        );
    }
}
