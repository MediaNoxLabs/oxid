// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{collections::BTreeMap, sync::RwLock};

use oxid_credential_application::{CredentialRepository, CredentialRepositoryError};
use oxid_credential_domain::{CredentialId, CredentialProfileId, CredentialRecord};
use oxid_identity_application::{DidRecordRepository, DidRecordRepositoryError};
use oxid_identity_domain::{DidRecord, IdentityProfileId, MidnightDid};
use oxid_wallet_application::{
    WalletProfileAssociationRepository, WalletProfileAssociationRepositoryError,
    WalletProfileAssociations, WalletProfileRepository, WalletProfileRepositoryError,
};
use oxid_wallet_domain::{WalletProfile, WalletProfileId};

/// Process-local profile storage for development, demos, and tests.
///
/// This adapter is neither durable nor a secure secret store.
#[derive(Default)]
pub struct InMemoryWalletProfileRepository {
    profiles: RwLock<BTreeMap<String, WalletProfile>>,
    active_profile_id: RwLock<Option<String>>,
    associations: RwLock<BTreeMap<String, WalletProfileAssociations>>,
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

    fn remove(&self, id: &WalletProfileId) -> Result<(), WalletProfileRepositoryError> {
        if self
            .profiles
            .write()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?
            .remove(id.as_str())
            .is_none()
        {
            return Err(WalletProfileRepositoryError::NotFound);
        }
        let mut active = self
            .active_profile_id
            .write()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        if active.as_deref() == Some(id.as_str()) {
            *active = None;
        }
        self.associations
            .write()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?
            .remove(id.as_str());
        Ok(())
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

impl WalletProfileAssociationRepository for InMemoryWalletProfileRepository {
    fn load_associations(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Option<WalletProfileAssociations>, WalletProfileAssociationRepositoryError> {
        self.associations
            .read()
            .map(|records| records.get(profile_id.as_str()).cloned())
            .map_err(|_| WalletProfileAssociationRepositoryError::Unavailable)
    }

    fn save_associations(
        &self,
        profile_id: &WalletProfileId,
        associations: WalletProfileAssociations,
    ) -> Result<(), WalletProfileAssociationRepositoryError> {
        if !self
            .profiles
            .read()
            .map_err(|_| WalletProfileAssociationRepositoryError::Unavailable)?
            .contains_key(profile_id.as_str())
        {
            return Err(WalletProfileAssociationRepositoryError::Integrity);
        }
        self.associations
            .write()
            .map_err(|_| WalletProfileAssociationRepositoryError::Unavailable)?
            .insert(profile_id.as_str().to_owned(), associations);
        Ok(())
    }

    fn remove_associations(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<(), WalletProfileAssociationRepositoryError> {
        self.associations
            .write()
            .map_err(|_| WalletProfileAssociationRepositoryError::Unavailable)?
            .remove(profile_id.as_str());
        Ok(())
    }
}

/// Process-local public DID record storage for application and protocol tests.
#[derive(Default)]
pub struct InMemoryDidRecordRepository {
    records: RwLock<BTreeMap<(String, String), DidRecord>>,
}

impl InMemoryDidRecordRepository {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl DidRecordRepository for InMemoryDidRecordRepository {
    fn upsert(&self, record: DidRecord) -> Result<(), DidRecordRepositoryError> {
        let key = (
            record.profile_id().as_str().to_owned(),
            record.resolution().document().id().as_str().to_owned(),
        );
        self.records
            .write()
            .map_err(|_| DidRecordRepositoryError::Unavailable)?
            .insert(key, record);
        Ok(())
    }

    fn list(
        &self,
        profile_id: &IdentityProfileId,
    ) -> Result<Vec<DidRecord>, DidRecordRepositoryError> {
        self.records
            .read()
            .map(|records| {
                records
                    .iter()
                    .filter(|((profile, _), _)| profile == profile_id.as_str())
                    .map(|(_, record)| record.clone())
                    .collect()
            })
            .map_err(|_| DidRecordRepositoryError::Unavailable)
    }

    fn get(
        &self,
        profile_id: &IdentityProfileId,
        did: &MidnightDid,
    ) -> Result<DidRecord, DidRecordRepositoryError> {
        self.records
            .read()
            .map_err(|_| DidRecordRepositoryError::Unavailable)?
            .get(&(profile_id.as_str().to_owned(), did.as_str().to_owned()))
            .cloned()
            .ok_or(DidRecordRepositoryError::NotFound)
    }

    fn remove(
        &self,
        profile_id: &IdentityProfileId,
        did: &MidnightDid,
    ) -> Result<(), DidRecordRepositoryError> {
        self.records
            .write()
            .map_err(|_| DidRecordRepositoryError::Unavailable)?
            .remove(&(profile_id.as_str().to_owned(), did.as_str().to_owned()))
            .map(|_| ())
            .ok_or(DidRecordRepositoryError::NotFound)
    }
}

/// Process-local credential storage for application and incoming-adapter tests.
/// Original signed bytes remain private to the repository boundary.
#[derive(Default)]
pub struct InMemoryCredentialRepository {
    records: RwLock<BTreeMap<(String, String), CredentialRecord>>,
}

impl InMemoryCredentialRepository {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialRepository for InMemoryCredentialRepository {
    fn upsert(&self, record: CredentialRecord) -> Result<(), CredentialRepositoryError> {
        let key = (
            record.profile_id().as_str().to_owned(),
            record.id().as_str().to_owned(),
        );
        self.records
            .write()
            .map_err(|_| CredentialRepositoryError::Unavailable)?
            .insert(key, record);
        Ok(())
    }

    fn list(
        &self,
        profile_id: &CredentialProfileId,
    ) -> Result<Vec<CredentialRecord>, CredentialRepositoryError> {
        self.records
            .read()
            .map(|records| {
                records
                    .iter()
                    .filter(|((profile, _), _)| profile == profile_id.as_str())
                    .map(|(_, record)| record.clone())
                    .collect()
            })
            .map_err(|_| CredentialRepositoryError::Unavailable)
    }

    fn get(
        &self,
        profile_id: &CredentialProfileId,
        credential_id: &CredentialId,
    ) -> Result<CredentialRecord, CredentialRepositoryError> {
        self.records
            .read()
            .map_err(|_| CredentialRepositoryError::Unavailable)?
            .get(&(
                profile_id.as_str().to_owned(),
                credential_id.as_str().to_owned(),
            ))
            .cloned()
            .ok_or(CredentialRepositoryError::NotFound)
    }

    fn remove(
        &self,
        profile_id: &CredentialProfileId,
        credential_id: &CredentialId,
    ) -> Result<(), CredentialRepositoryError> {
        self.records
            .write()
            .map_err(|_| CredentialRepositoryError::Unavailable)?
            .remove(&(
                profile_id.as_str().to_owned(),
                credential_id.as_str().to_owned(),
            ))
            .map(|_| ())
            .ok_or(CredentialRepositoryError::NotFound)
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
