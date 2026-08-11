// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{collections::BTreeMap, sync::RwLock};

use oxid_wallet_application::{WalletProfileRepository, WalletProfileRepositoryError};
use oxid_wallet_domain::{WalletProfile, WalletProfileId};

/// Process-local profile storage for development, demos, and tests.
///
/// This adapter is neither durable nor a secure secret store.
#[derive(Default)]
pub struct InMemoryWalletProfileRepository {
    profiles: RwLock<BTreeMap<String, WalletProfile>>,
    active_profile_id: RwLock<Option<String>>,
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

    fn set_active(
        &self,
        id: &WalletProfileId,
    ) -> Result<WalletProfile, WalletProfileRepositoryError> {
        let profiles = self
            .profiles
            .read()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        let profile = profiles
            .get(id.as_str())
            .cloned()
            .ok_or(WalletProfileRepositoryError::NotFound)?;
        *self
            .active_profile_id
            .write()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)? = Some(id.as_str().to_owned());

        Ok(profile)
    }

    fn active(&self) -> Result<Option<WalletProfile>, WalletProfileRepositoryError> {
        let profiles = self
            .profiles
            .read()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        let active_profile_id = self
            .active_profile_id
            .read()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        let Some(active_profile_id) = active_profile_id.as_deref() else {
            return Ok(None);
        };

        profiles
            .get(active_profile_id)
            .cloned()
            .map(Some)
            .ok_or(WalletProfileRepositoryError::NotFound)
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

    #[test]
    fn persists_active_selection_for_the_process_lifetime() {
        let repository = InMemoryWalletProfileRepository::new();
        let profile = profile();
        let profile_id = profile.id().clone();
        repository
            .save(profile.clone())
            .expect("save should succeed");

        assert_eq!(repository.active().expect("active read should work"), None);
        assert_eq!(
            repository
                .set_active(&profile_id)
                .expect("selection should succeed"),
            profile
        );
        assert_eq!(
            repository.active().expect("selection should persist"),
            Some(profile)
        );
    }

    #[test]
    fn rejects_selecting_an_unknown_profile() {
        let repository = InMemoryWalletProfileRepository::new();
        let missing = WalletProfileId::parse("profile_missing").expect("identifier should parse");

        assert_eq!(
            repository.set_active(&missing),
            Err(WalletProfileRepositoryError::NotFound)
        );
    }
}
