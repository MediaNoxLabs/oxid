// SPDX-License-Identifier: Apache-2.0

//! Complete-wallet archive coordination across public repositories and custody.
//!
//! Recovery journals contain identifiers and counts only. Decrypted credential
//! and custody material stays below the application incoming boundary.

#![forbid(unsafe_code)]

use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

use oxid_adapter_backup_portable::{
    CompleteWalletArchive, PortableCustodyVault, PortableCustodyVaultPort,
    open_complete_wallet_archive, seal_complete_wallet_archive,
};
use oxid_adapter_storage_credential_json::{
    decode_portable_credential_snapshot, encode_portable_credential_snapshot,
};
use oxid_adapter_storage_identity_json::{
    decode_portable_did_snapshot, encode_portable_did_snapshot,
};
use oxid_adapter_storage_json::{
    decode_portable_profile_snapshot, encode_portable_profile_snapshot,
};
use oxid_credential_application::{CredentialRepository, CredentialRepositoryError};
use oxid_credential_domain::{CredentialId, CredentialProfileId, CredentialRecord};
use oxid_identity_application::{DidRecordRepository, DidRecordRepositoryError};
use oxid_identity_domain::{DidRecord, IdentityProfileId, MidnightDid};
use oxid_platform_ports::RandomPort;
use oxid_wallet_application::{
    CompleteWalletBackupPort, CompleteWalletRecoverySummary, PortableWalletBackup,
    WalletPortableBackupPortError, WalletProfileAssociationRepository,
    WalletProfileAssociationRepositoryError, WalletProfileAssociations, WalletProfileRepository,
    WalletProfileRepositoryError, WalletRecoverySecret,
};
use oxid_wallet_domain::{WalletKeyReference, WalletProfile, WalletProfileId};
use serde::{Deserialize, Serialize};

const JOURNAL_SCHEMA_VERSION: u32 = 1;
const MAX_JOURNAL_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DID_RECORDS: usize = 128;
const MAX_CREDENTIAL_RECORDS: usize = 64;
const MAX_CUSTODY_KEYS: usize = 256;

/// Safe recovery-journal failures. No path or stored value is exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryJournalError {
    Integrity,
    Unavailable,
}

/// Owner-private journal boundary used to reconcile a cross-store commit.
pub trait RecoveryJournalPort: Send + Sync {
    fn load(&self) -> Result<Option<RecoveryJournal>, RecoveryJournalError>;
    fn save(&self, journal: &RecoveryJournal) -> Result<(), RecoveryJournalError>;
    fn clear(&self) -> Result<(), RecoveryJournalError>;
}

/// Process-local journal for deterministic tests and non-mobile harnesses.
#[derive(Default)]
pub struct InMemoryRecoveryJournal(Mutex<Option<RecoveryJournal>>);

impl RecoveryJournalPort for InMemoryRecoveryJournal {
    fn load(&self) -> Result<Option<RecoveryJournal>, RecoveryJournalError> {
        self.0
            .lock()
            .map(|journal| journal.clone())
            .map_err(|_| RecoveryJournalError::Unavailable)
    }

    fn save(&self, journal: &RecoveryJournal) -> Result<(), RecoveryJournalError> {
        journal.validate()?;
        *self
            .0
            .lock()
            .map_err(|_| RecoveryJournalError::Unavailable)? = Some(journal.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), RecoveryJournalError> {
        *self
            .0
            .lock()
            .map_err(|_| RecoveryJournalError::Unavailable)? = None;
        Ok(())
    }
}

/// Fail-closed journal used when durable native storage cannot be configured.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableRecoveryJournal;

impl RecoveryJournalPort for UnavailableRecoveryJournal {
    fn load(&self) -> Result<Option<RecoveryJournal>, RecoveryJournalError> {
        Err(RecoveryJournalError::Unavailable)
    }

    fn save(&self, _: &RecoveryJournal) -> Result<(), RecoveryJournalError> {
        Err(RecoveryJournalError::Unavailable)
    }

    fn clear(&self) -> Result<(), RecoveryJournalError> {
        Err(RecoveryJournalError::Unavailable)
    }
}

/// Filesystem journal for native compositions.
pub struct FileRecoveryJournal {
    path: PathBuf,
    access: Mutex<()>,
}

impl FileRecoveryJournal {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, RecoveryJournalError> {
        let path = path.into();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(RecoveryJournalError::Integrity);
        }
        Ok(Self {
            path,
            access: Mutex::new(()),
        })
    }

    fn load_unlocked(&self) -> Result<Option<RecoveryJournal>, RecoveryJournalError> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(RecoveryJournalError::Unavailable),
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() == 0 {
            return Err(RecoveryJournalError::Integrity);
        }
        if metadata.len() > MAX_JOURNAL_BYTES {
            return Err(RecoveryJournalError::Integrity);
        }
        #[cfg(unix)]
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(RecoveryJournalError::Integrity);
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        File::open(&self.path)
            .map_err(|_| RecoveryJournalError::Unavailable)?
            .take(MAX_JOURNAL_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| RecoveryJournalError::Unavailable)?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(RecoveryJournalError::Integrity);
        }
        let journal: RecoveryJournal =
            serde_json::from_slice(&bytes).map_err(|_| RecoveryJournalError::Integrity)?;
        journal.validate()?;
        Ok(Some(journal))
    }

    fn save_unlocked(&self, journal: &RecoveryJournal) -> Result<(), RecoveryJournalError> {
        journal.validate()?;
        let bytes = serde_json::to_vec(journal).map_err(|_| RecoveryJournalError::Integrity)?;
        if bytes.is_empty() || bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(RecoveryJournalError::Integrity);
        }
        let parent = self.path.parent().ok_or(RecoveryJournalError::Integrity)?;
        fs::create_dir_all(parent).map_err(|_| RecoveryJournalError::Unavailable)?;
        reject_symlink(parent)?;
        reject_symlink_if_present(&self.path)?;
        let temporary = temporary_path(&self.path);
        remove_stale_temporary(&temporary)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(&temporary)
            .map_err(|_| RecoveryJournalError::Unavailable)?;
        let result = (|| {
            file.write_all(&bytes)
                .map_err(|_| RecoveryJournalError::Unavailable)?;
            file.sync_all()
                .map_err(|_| RecoveryJournalError::Unavailable)?;
            fs::rename(&temporary, &self.path).map_err(|_| RecoveryJournalError::Unavailable)?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| RecoveryJournalError::Unavailable)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

impl RecoveryJournalPort for FileRecoveryJournal {
    fn load(&self) -> Result<Option<RecoveryJournal>, RecoveryJournalError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| RecoveryJournalError::Unavailable)?;
        self.load_unlocked()
    }

    fn save(&self, journal: &RecoveryJournal) -> Result<(), RecoveryJournalError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| RecoveryJournalError::Unavailable)?;
        self.save_unlocked(journal)
    }

    fn clear(&self) -> Result<(), RecoveryJournalError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| RecoveryJournalError::Unavailable)?;
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                fs::remove_file(&self.path).map_err(|_| RecoveryJournalError::Unavailable)?;
                let parent = self.path.parent().ok_or(RecoveryJournalError::Integrity)?;
                File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|_| RecoveryJournalError::Unavailable)
            }
            Ok(_) => Err(RecoveryJournalError::Integrity),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(RecoveryJournalError::Unavailable),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryPhase {
    Prepared,
    PublicStaged,
    CustodyCommitted,
}

/// Safe journal contents. Fields are private so only validated coordinator
/// state can be persisted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryJournal {
    schema_version: u32,
    profile_id: String,
    prior_active_profile_id: Option<String>,
    has_associations: bool,
    did_count: usize,
    did_ids: Vec<String>,
    credential_count: usize,
    credential_ids: Vec<String>,
    key_count: usize,
    key_references: Vec<String>,
    phase: RecoveryPhase,
}

impl RecoveryJournal {
    fn prepared(
        state: &PreparedWalletState,
        custody: &PortableCustodyVault,
        prior_active_profile_id: Option<String>,
    ) -> Self {
        let mut did_ids = state
            .dids
            .iter()
            .map(|record| record.resolution().document().id().as_str().to_owned())
            .collect::<Vec<_>>();
        did_ids.sort();
        let mut credential_ids = state
            .credentials
            .iter()
            .map(|record| record.id().as_str().to_owned())
            .collect::<Vec<_>>();
        credential_ids.sort();
        let mut key_references = custody
            .keys()
            .iter()
            .map(|key| key.descriptor().reference().as_str().to_owned())
            .collect::<Vec<_>>();
        key_references.sort();
        Self {
            schema_version: JOURNAL_SCHEMA_VERSION,
            profile_id: state.profile.id().as_str().to_owned(),
            prior_active_profile_id,
            has_associations: state.associations.is_some(),
            did_count: did_ids.len(),
            did_ids,
            credential_count: credential_ids.len(),
            credential_ids,
            key_count: key_references.len(),
            key_references,
            phase: RecoveryPhase::Prepared,
        }
    }

    fn validate(&self) -> Result<(), RecoveryJournalError> {
        if self.schema_version != JOURNAL_SCHEMA_VERSION
            || WalletProfileId::parse(self.profile_id.clone()).is_err()
            || self
                .prior_active_profile_id
                .as_ref()
                .is_some_and(|value| WalletProfileId::parse(value.clone()).is_err())
            || self.did_count != self.did_ids.len()
            || self.credential_count != self.credential_ids.len()
            || self.key_count != self.key_references.len()
            || self.did_count > MAX_DID_RECORDS
            || self.credential_count > MAX_CREDENTIAL_RECORDS
            || self.key_count > MAX_CUSTODY_KEYS
            || !strictly_sorted(&self.did_ids)
            || !strictly_sorted(&self.credential_ids)
            || !strictly_sorted(&self.key_references)
            || self
                .did_ids
                .iter()
                .any(|value| MidnightDid::parse(value.clone()).is_err())
            || self
                .credential_ids
                .iter()
                .any(|value| CredentialId::parse(value.clone()).is_err())
            || self
                .key_references
                .iter()
                .any(|value| WalletKeyReference::parse(value.clone()).is_err())
        {
            return Err(RecoveryJournalError::Integrity);
        }
        Ok(())
    }

    fn matches(&self, expected: &Self) -> bool {
        self.schema_version == expected.schema_version
            && self.profile_id == expected.profile_id
            && self.has_associations == expected.has_associations
            && self.did_count == expected.did_count
            && self.did_ids == expected.did_ids
            && self.credential_count == expected.credential_count
            && self.credential_ids == expected.credential_ids
            && self.key_count == expected.key_count
            && self.key_references == expected.key_references
    }
}

/// One operation-serialized complete-wallet backup and recovery adapter.
pub struct CompleteWalletBackupAdapter {
    custody: Arc<dyn PortableCustodyVaultPort>,
    profiles: Arc<dyn WalletProfileRepository>,
    dids: Arc<dyn DidRecordRepository>,
    credentials: Arc<dyn CredentialRepository>,
    associations: Arc<dyn WalletProfileAssociationRepository>,
    random: Arc<dyn RandomPort>,
    journal: Arc<dyn RecoveryJournalPort>,
    operation: Mutex<()>,
}

impl CompleteWalletBackupAdapter {
    #[must_use]
    pub fn new(
        custody: Arc<dyn PortableCustodyVaultPort>,
        profiles: Arc<dyn WalletProfileRepository>,
        dids: Arc<dyn DidRecordRepository>,
        credentials: Arc<dyn CredentialRepository>,
        associations: Arc<dyn WalletProfileAssociationRepository>,
        random: Arc<dyn RandomPort>,
        journal: Arc<dyn RecoveryJournalPort>,
    ) -> Self {
        Self {
            custody,
            profiles,
            dids,
            credentials,
            associations,
            random,
            journal,
            operation: Mutex::new(()),
        }
    }
}

impl CompleteWalletBackupPort for CompleteWalletBackupAdapter {
    fn export_complete_wallet_backup(
        &self,
        profile_id: &WalletProfileId,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| WalletPortableBackupPortError::Unavailable)?;
        if self.journal.load().map_err(map_journal_error)?.is_some() {
            return Err(WalletPortableBackupPortError::Conflict);
        }
        let profile = self
            .profiles
            .list()
            .map_err(map_profile_error)?
            .into_iter()
            .find(|candidate| candidate.id() == profile_id)
            .ok_or(WalletPortableBackupPortError::NotInitialized)?;
        let associations = self
            .associations
            .load_associations(profile_id)
            .map_err(map_association_error)?;
        let identity_profile = IdentityProfileId::parse(profile_id.as_str())
            .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;
        let dids = self.dids.list(&identity_profile).map_err(map_did_error)?;
        let credential_profile = CredentialProfileId::parse(profile_id.as_str())
            .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;
        let credentials = self
            .credentials
            .list(&credential_profile)
            .map_err(map_credential_error)?;
        let profile_snapshot = encode_portable_profile_snapshot(&profile, associations.as_ref())
            .map_err(map_snapshot_profile_error)?;
        let did_snapshot =
            encode_portable_did_snapshot(&dids).map_err(map_snapshot_did_export_error)?;
        let credential_snapshot = encode_portable_credential_snapshot(&credentials)
            .map_err(map_snapshot_credential_export_error)?;
        let custody = self.custody.export_custody_vault(profile_id)?;
        let archive = CompleteWalletArchive::new(
            profile_id.clone(),
            profile_snapshot,
            did_snapshot,
            credential_snapshot,
            custody,
        )?;
        seal_complete_wallet_archive(&archive, recovery_secret, self.random.as_ref())
    }

    fn recover_complete_wallet_backup(
        &self,
        expected_profile_id: Option<&WalletProfileId>,
        backup: &PortableWalletBackup,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<CompleteWalletRecoverySummary, WalletPortableBackupPortError> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| WalletPortableBackupPortError::Unavailable)?;
        let archive = open_complete_wallet_archive(backup, recovery_secret, expected_profile_id)?;
        let state = PreparedWalletState::decode(&archive)?;
        let prior_active = self
            .profiles
            .active()
            .map_err(map_profile_error)?
            .map(|profile| profile.id().as_str().to_owned());
        let expected_journal = RecoveryJournal::prepared(&state, archive.custody(), prior_active);
        if let Some(existing) = self.journal.load().map_err(map_journal_error)? {
            if !existing.matches(&expected_journal) {
                return Err(WalletPortableBackupPortError::Conflict);
            }
            if let Some(summary) = self.reconcile(&existing, &state, archive.custody())? {
                return Ok(summary);
            }
        }
        self.preflight(&state, archive.custody())?;
        let prior_active = self
            .profiles
            .active()
            .map_err(map_profile_error)?
            .map(|profile| profile.id().as_str().to_owned());
        let mut journal = RecoveryJournal::prepared(&state, archive.custody(), prior_active);
        self.journal.save(&journal).map_err(map_journal_error)?;
        if let Err(error) = self.stage_public(&state) {
            return self.fail_before_custody(error, &journal, &state);
        }
        journal.phase = RecoveryPhase::PublicStaged;
        if let Err(error) = self.journal.save(&journal).map_err(map_journal_error) {
            return self.fail_before_custody(error, &journal, &state);
        }
        let custody_summary = match self.custody.recover_custody_vault(archive.custody()) {
            Ok(summary) => summary,
            Err(error) => return self.fail_before_custody(error, &journal, &state),
        };
        journal.phase = RecoveryPhase::CustodyCommitted;
        self.journal.save(&journal).map_err(map_journal_error)?;
        self.verify_public(&state)?;
        self.custody.verify_recovered_custody(archive.custody())?;
        self.profiles
            .set_active(state.profile.id())
            .map_err(map_profile_error)?;
        self.journal.clear().map_err(map_journal_error)?;
        Ok(state.summary(custody_summary.restored_key_count))
    }
}

impl CompleteWalletBackupAdapter {
    fn preflight(
        &self,
        state: &PreparedWalletState,
        custody: &PortableCustodyVault,
    ) -> Result<(), WalletPortableBackupPortError> {
        if self
            .profiles
            .list()
            .map_err(map_profile_error)?
            .iter()
            .any(|profile| profile.id() == state.profile.id())
            || self
                .associations
                .load_associations(state.profile.id())
                .map_err(map_association_error)?
                .is_some()
            || !self
                .dids
                .list(&state.identity_profile_id()?)
                .map_err(map_did_error)?
                .is_empty()
            || !self
                .credentials
                .list(&state.credential_profile_id()?)
                .map_err(map_credential_error)?
                .is_empty()
        {
            return Err(WalletPortableBackupPortError::Conflict);
        }
        self.custody.preflight_custody_recovery(custody)?;
        Ok(())
    }

    fn stage_public(
        &self,
        state: &PreparedWalletState,
    ) -> Result<(), WalletPortableBackupPortError> {
        self.profiles
            .save(state.profile.clone())
            .map_err(map_profile_error)?;
        if let Some(associations) = &state.associations {
            self.associations
                .save_associations(state.profile.id(), associations.clone())
                .map_err(map_association_error)?;
        }
        for record in &state.dids {
            self.dids.upsert(record.clone()).map_err(map_did_error)?;
        }
        for record in &state.credentials {
            self.credentials
                .upsert(record.clone())
                .map_err(map_credential_error)?;
        }
        Ok(())
    }

    fn verify_public(
        &self,
        state: &PreparedWalletState,
    ) -> Result<(), WalletPortableBackupPortError> {
        let profile_matches = self
            .profiles
            .list()
            .map_err(map_profile_error)?
            .into_iter()
            .find(|profile| profile.id() == state.profile.id())
            .is_some_and(|profile| profile == state.profile);
        let associations_match = self
            .associations
            .load_associations(state.profile.id())
            .map_err(map_association_error)?
            == state.associations;
        let dids_match = sorted_dids(
            self.dids
                .list(&state.identity_profile_id()?)
                .map_err(map_did_error)?,
        ) == sorted_dids(state.dids.clone());
        let credentials_match = sorted_credentials(
            self.credentials
                .list(&state.credential_profile_id()?)
                .map_err(map_credential_error)?,
        ) == sorted_credentials(state.credentials.clone());
        if !profile_matches || !associations_match || !dids_match || !credentials_match {
            return Err(WalletPortableBackupPortError::Conflict);
        }
        Ok(())
    }

    fn reconcile(
        &self,
        journal: &RecoveryJournal,
        state: &PreparedWalletState,
        custody: &PortableCustodyVault,
    ) -> Result<Option<CompleteWalletRecoverySummary>, WalletPortableBackupPortError> {
        match self.custody.verify_recovered_custody(custody) {
            Ok(summary) => {
                self.verify_public(state)?;
                self.profiles
                    .set_active(state.profile.id())
                    .map_err(map_profile_error)?;
                self.journal.clear().map_err(map_journal_error)?;
                Ok(Some(state.summary(summary.restored_key_count)))
            }
            Err(WalletPortableBackupPortError::NotInitialized)
                if journal.phase != RecoveryPhase::CustodyCommitted =>
            {
                self.rollback_public(journal, state)?;
                self.journal.clear().map_err(map_journal_error)?;
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn fail_before_custody<T>(
        &self,
        original: WalletPortableBackupPortError,
        journal: &RecoveryJournal,
        state: &PreparedWalletState,
    ) -> Result<T, WalletPortableBackupPortError> {
        self.rollback_public(journal, state)?;
        self.journal.clear().map_err(map_journal_error)?;
        Err(original)
    }

    fn rollback_public(
        &self,
        journal: &RecoveryJournal,
        state: &PreparedWalletState,
    ) -> Result<(), WalletPortableBackupPortError> {
        self.ensure_only_staged_state(state)?;
        let identity_profile = state.identity_profile_id()?;
        for record in &state.dids {
            match self
                .dids
                .remove(&identity_profile, record.resolution().document().id())
            {
                Ok(()) | Err(DidRecordRepositoryError::NotFound) => {}
                Err(error) => return Err(map_did_error(error)),
            }
        }
        let credential_profile = state.credential_profile_id()?;
        for record in &state.credentials {
            match self.credentials.remove(&credential_profile, record.id()) {
                Ok(()) | Err(CredentialRepositoryError::NotFound) => {}
                Err(error) => return Err(map_credential_error(error)),
            }
        }
        match self.profiles.remove(state.profile.id()) {
            Ok(()) | Err(WalletProfileRepositoryError::NotFound) => {}
            Err(error) => return Err(map_profile_error(error)),
        }
        if let Some(prior) = &journal.prior_active_profile_id {
            let prior = WalletProfileId::parse(prior.clone())
                .map_err(|_| WalletPortableBackupPortError::Conflict)?;
            self.profiles
                .set_active(&prior)
                .map_err(map_profile_error)?;
        }
        Ok(())
    }

    fn ensure_only_staged_state(
        &self,
        state: &PreparedWalletState,
    ) -> Result<(), WalletPortableBackupPortError> {
        let profiles = self.profiles.list().map_err(map_profile_error)?;
        if let Some(current) = profiles
            .into_iter()
            .find(|profile| profile.id() == state.profile.id())
            && current != state.profile
        {
            return Err(WalletPortableBackupPortError::Conflict);
        }
        if let Some(current) = self
            .associations
            .load_associations(state.profile.id())
            .map_err(map_association_error)?
            && Some(current) != state.associations
        {
            return Err(WalletPortableBackupPortError::Conflict);
        }
        let identity_profile = state.identity_profile_id()?;
        for current in self.dids.list(&identity_profile).map_err(map_did_error)? {
            if !state.dids.contains(&current) {
                return Err(WalletPortableBackupPortError::Conflict);
            }
        }
        let credential_profile = state.credential_profile_id()?;
        for current in self
            .credentials
            .list(&credential_profile)
            .map_err(map_credential_error)?
        {
            if !state.credentials.contains(&current) {
                return Err(WalletPortableBackupPortError::Conflict);
            }
        }
        Ok(())
    }
}

struct PreparedWalletState {
    profile: WalletProfile,
    associations: Option<WalletProfileAssociations>,
    dids: Vec<DidRecord>,
    credentials: Vec<CredentialRecord>,
}

impl PreparedWalletState {
    fn decode(archive: &CompleteWalletArchive) -> Result<Self, WalletPortableBackupPortError> {
        let (profile, associations) = decode_portable_profile_snapshot(archive.profile_snapshot())
            .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?;
        let dids = decode_portable_did_snapshot(archive.did_snapshot())
            .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?;
        let credentials = decode_portable_credential_snapshot(archive.credential_snapshot())
            .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?;
        if profile.id() != archive.profile_id()
            || dids
                .iter()
                .any(|record| record.profile_id().as_str() != profile.id().as_str())
            || credentials
                .iter()
                .any(|record| record.profile_id().as_str() != profile.id().as_str())
        {
            return Err(WalletPortableBackupPortError::InvalidPackage);
        }
        Ok(Self {
            profile,
            associations,
            dids,
            credentials,
        })
    }

    fn identity_profile_id(&self) -> Result<IdentityProfileId, WalletPortableBackupPortError> {
        IdentityProfileId::parse(self.profile.id().as_str())
            .map_err(|_| WalletPortableBackupPortError::InvalidPackage)
    }

    fn credential_profile_id(&self) -> Result<CredentialProfileId, WalletPortableBackupPortError> {
        CredentialProfileId::parse(self.profile.id().as_str())
            .map_err(|_| WalletPortableBackupPortError::InvalidPackage)
    }

    fn summary(&self, restored_key_count: usize) -> CompleteWalletRecoverySummary {
        CompleteWalletRecoverySummary {
            profile_id: self.profile.id().as_str().to_owned(),
            restored_key_count,
            restored_did_count: self.dids.len(),
            restored_credential_count: self.credentials.len(),
        }
    }
}

fn sorted_dids(mut records: Vec<DidRecord>) -> Vec<DidRecord> {
    records.sort_by(|left, right| {
        left.resolution()
            .document()
            .id()
            .cmp(right.resolution().document().id())
    });
    records
}

fn sorted_credentials(mut records: Vec<CredentialRecord>) -> Vec<CredentialRecord> {
    records.sort_by(|left, right| left.id().cmp(right.id()));
    records
}

fn strictly_sorted(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

fn reject_symlink(path: &Path) -> Result<(), RecoveryJournalError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| RecoveryJournalError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RecoveryJournalError::Integrity);
    }
    Ok(())
}

fn reject_symlink_if_present(path: &Path) -> Result<(), RecoveryJournalError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(RecoveryJournalError::Integrity)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(RecoveryJournalError::Unavailable),
    }
}

fn remove_stale_temporary(path: &Path) -> Result<(), RecoveryJournalError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            #[cfg(unix)]
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(RecoveryJournalError::Integrity);
            }
            fs::remove_file(path).map_err(|_| RecoveryJournalError::Unavailable)
        }
        Ok(_) => Err(RecoveryJournalError::Integrity),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(RecoveryJournalError::Unavailable),
    }
}

const fn map_journal_error(error: RecoveryJournalError) -> WalletPortableBackupPortError {
    match error {
        RecoveryJournalError::Integrity => WalletPortableBackupPortError::Conflict,
        RecoveryJournalError::Unavailable => WalletPortableBackupPortError::Unavailable,
    }
}

const fn map_profile_error(error: WalletProfileRepositoryError) -> WalletPortableBackupPortError {
    match error {
        WalletProfileRepositoryError::Conflict => WalletPortableBackupPortError::Conflict,
        WalletProfileRepositoryError::NotFound => WalletPortableBackupPortError::NotInitialized,
        WalletProfileRepositoryError::Unavailable => WalletPortableBackupPortError::Unavailable,
    }
}

const fn map_association_error(
    error: WalletProfileAssociationRepositoryError,
) -> WalletPortableBackupPortError {
    match error {
        WalletProfileAssociationRepositoryError::Integrity => {
            WalletPortableBackupPortError::Conflict
        }
        WalletProfileAssociationRepositoryError::Unavailable => {
            WalletPortableBackupPortError::Unavailable
        }
    }
}

const fn map_did_error(error: DidRecordRepositoryError) -> WalletPortableBackupPortError {
    match error {
        DidRecordRepositoryError::NotFound => WalletPortableBackupPortError::NotInitialized,
        DidRecordRepositoryError::CapacityExceeded => WalletPortableBackupPortError::Conflict,
        DidRecordRepositoryError::Integrity => WalletPortableBackupPortError::Conflict,
        DidRecordRepositoryError::Unavailable => WalletPortableBackupPortError::Unavailable,
    }
}

const fn map_credential_error(error: CredentialRepositoryError) -> WalletPortableBackupPortError {
    match error {
        CredentialRepositoryError::NotFound => WalletPortableBackupPortError::NotInitialized,
        CredentialRepositoryError::CapacityExceeded => WalletPortableBackupPortError::Conflict,
        CredentialRepositoryError::Integrity => WalletPortableBackupPortError::Conflict,
        CredentialRepositoryError::Unavailable => WalletPortableBackupPortError::Unavailable,
    }
}

const fn map_snapshot_profile_error(
    _: WalletProfileRepositoryError,
) -> WalletPortableBackupPortError {
    WalletPortableBackupPortError::InvalidOperation
}

const fn map_snapshot_did_export_error(
    _: DidRecordRepositoryError,
) -> WalletPortableBackupPortError {
    WalletPortableBackupPortError::InvalidOperation
}

const fn map_snapshot_credential_export_error(
    _: CredentialRepositoryError,
) -> WalletPortableBackupPortError {
    WalletPortableBackupPortError::InvalidOperation
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        sync::atomic::{AtomicU64, Ordering},
    };

    use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
    use oxid_adapter_storage_memory::{
        InMemoryCredentialRepository, InMemoryDidRecordRepository, InMemoryWalletProfileRepository,
    };
    use oxid_credential_domain::{
        CredentialDetachedProof, CredentialFormat, CredentialMetadata, CredentialPrivateMaterial,
        VerificationOutcome, VerificationReport, VerificationStage, VerificationStageName,
        VerificationStageStatus,
    };
    use oxid_foundation::UnixTimestampMillis;
    use oxid_identity_domain::{
        DID_CONTEXT, DidDocument, DidDocumentMetadata, DidDocumentParts, DidResolution,
        DidResolutionMetadata, DidResolutionSource, JWK_CONTEXT,
    };
    use oxid_platform_ports::{ClockPort, PlatformError};
    use oxid_wallet_application::{
        GenerateProtectedKeyRequest, WalletAccountAssociation, WalletKeyOperationPort,
        WalletPortableRecoverySummary, WalletProfileAssociationRepository, WalletProtectionPort,
    };
    use oxid_wallet_domain::{
        ChainNetworkId, ProfileName, WalletKeyAlgorithm, WalletKeyLabel, WalletKeyPurpose,
    };
    use zeroize::Zeroizing;

    use super::*;

    static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    struct IncrementingRandom(Mutex<u8>);

    impl IncrementingRandom {
        fn new(seed: u8) -> Self {
            Self(Mutex::new(seed))
        }
    }

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

    #[derive(Default)]
    struct MemoryJournal(Mutex<Option<RecoveryJournal>>);

    impl RecoveryJournalPort for MemoryJournal {
        fn load(&self) -> Result<Option<RecoveryJournal>, RecoveryJournalError> {
            self.0
                .lock()
                .map(|journal| journal.clone())
                .map_err(|_| RecoveryJournalError::Unavailable)
        }

        fn save(&self, journal: &RecoveryJournal) -> Result<(), RecoveryJournalError> {
            journal.validate()?;
            *self
                .0
                .lock()
                .map_err(|_| RecoveryJournalError::Unavailable)? = Some(journal.clone());
            Ok(())
        }

        fn clear(&self) -> Result<(), RecoveryJournalError> {
            *self
                .0
                .lock()
                .map_err(|_| RecoveryJournalError::Unavailable)? = None;
            Ok(())
        }
    }

    struct RejectingCustody;

    impl PortableCustodyVaultPort for RejectingCustody {
        fn export_custody_vault(
            &self,
            _: &WalletProfileId,
        ) -> Result<PortableCustodyVault, WalletPortableBackupPortError> {
            Err(WalletPortableBackupPortError::Unavailable)
        }

        fn preflight_custody_recovery(
            &self,
            vault: &PortableCustodyVault,
        ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
            Ok(WalletPortableRecoverySummary {
                restored_key_count: vault.keys().len(),
            })
        }

        fn recover_custody_vault(
            &self,
            _: &PortableCustodyVault,
        ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
            Err(WalletPortableBackupPortError::AuthorizationDenied)
        }

        fn verify_recovered_custody(
            &self,
            _: &PortableCustodyVault,
        ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
            Err(WalletPortableBackupPortError::NotInitialized)
        }
    }

    type Profiles = InMemoryWalletProfileRepository;
    type Dids = InMemoryDidRecordRepository;
    type Credentials = InMemoryCredentialRepository;
    type Security = DevelopmentWalletSecurity<FixedClock, IncrementingRandom>;
    type Journal = MemoryJournal;
    struct Fixture<C> {
        adapter: CompleteWalletBackupAdapter,
        custody: Arc<C>,
        profiles: Arc<Profiles>,
        dids: Arc<Dids>,
        credentials: Arc<Credentials>,
        journal: Arc<Journal>,
    }

    fn adapter<C: PortableCustodyVaultPort + 'static>(custody: Arc<C>, seed: u8) -> Fixture<C> {
        let profiles = Arc::new(Profiles::default());
        let dids = Arc::new(Dids::default());
        let credentials = Arc::new(Credentials::default());
        let random = Arc::new(IncrementingRandom::new(seed));
        let journal = Arc::new(Journal::default());
        let custody_port: Arc<dyn PortableCustodyVaultPort> = custody.clone();
        let profile_repository: Arc<dyn WalletProfileRepository> = profiles.clone();
        let did_repository: Arc<dyn DidRecordRepository> = dids.clone();
        let credential_repository: Arc<dyn CredentialRepository> = credentials.clone();
        let association_repository: Arc<dyn WalletProfileAssociationRepository> = profiles.clone();
        let random_port: Arc<dyn RandomPort> = random;
        let journal_port: Arc<dyn RecoveryJournalPort> = journal.clone();
        let adapter = CompleteWalletBackupAdapter::new(
            custody_port,
            profile_repository,
            did_repository,
            credential_repository,
            association_repository,
            random_port,
            journal_port,
        );
        Fixture {
            adapter,
            custody,
            profiles,
            dids,
            credentials,
            journal,
        }
    }

    fn security(seed: u8) -> Arc<Security> {
        Arc::new(DevelopmentWalletSecurity::new(
            Arc::new(FixedClock),
            Arc::new(IncrementingRandom::new(seed)),
        ))
    }

    fn profile_id() -> WalletProfileId {
        WalletProfileId::parse("profile_one").expect("profile")
    }

    fn profile() -> WalletProfile {
        WalletProfile::new(
            profile_id(),
            ProfileName::parse("Primary").expect("name"),
            UnixTimestampMillis::new(42),
        )
    }

    fn associations() -> WalletProfileAssociations {
        WalletProfileAssociations::new(
            ChainNetworkId::parse("devnet").expect("network"),
            vec![
                WalletAccountAssociation::new(
                    ChainNetworkId::parse("devnet").expect("network"),
                    3,
                    7,
                )
                .expect("coordinates"),
            ],
        )
        .expect("associations")
    }

    fn did_record() -> DidRecord {
        let profile = IdentityProfileId::parse("profile_one").expect("profile");
        let did =
            MidnightDid::parse(format!("did:midnight:undeployed:{}", "a".repeat(64))).expect("DID");
        let document = DidDocument::new(DidDocumentParts {
            contexts: vec![DID_CONTEXT.to_owned(), JWK_CONTEXT.to_owned()],
            id: did.clone(),
            controllers: vec![did],
            also_known_as: Vec::new(),
            verification_methods: Vec::new(),
            relationships: Vec::new(),
            services: Vec::new(),
        })
        .expect("document");
        DidRecord::new(
            profile,
            DidResolution::new(
                document,
                DidDocumentMetadata::default(),
                DidResolutionMetadata::default(),
                DidResolutionSource::Standalone,
            ),
        )
    }

    fn credential_record() -> CredentialRecord {
        let stages = VerificationStageName::ALL
            .into_iter()
            .map(|name| {
                VerificationStage::new(
                    name,
                    if matches!(
                        name,
                        VerificationStageName::Structural
                            | VerificationStageName::Issuer
                            | VerificationStageName::Proof
                    ) {
                        VerificationStageStatus::Passed
                    } else {
                        VerificationStageStatus::NotChecked
                    },
                    None,
                )
                .expect("stage")
            })
            .collect();
        CredentialRecord::new_with_proof_and_private_material(
            CredentialProfileId::parse("profile_one").expect("profile"),
            CredentialId::parse("vc_one").expect("id"),
            b"signed-private-credential".to_vec(),
            Some(
                CredentialDetachedProof::new(b"detached-credential-proof".to_vec())
                    .expect("proof"),
            ),
            Some(
                CredentialPrivateMaterial::new(b"claim-opening-material".to_vec())
                    .expect("private material"),
            ),
            CredentialMetadata::new(
                "Identity credential",
                "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                None,
                CredentialFormat::MidnightCompactVc,
                Some(UnixTimestampMillis::new(7)),
            )
            .expect("metadata"),
            VerificationReport::new(VerificationOutcome::Valid, stages).expect("report"),
        )
        .expect("record")
    }

    fn secret() -> WalletRecoverySecret {
        WalletRecoverySecret::parse("correct horse battery staple").expect("secret")
    }

    fn populated_source() -> Fixture<Security> {
        let source = adapter(security(17), 31);
        source.profiles.save(profile()).expect("profile save");
        source
            .profiles
            .set_active(&profile_id())
            .expect("active profile");
        source
            .profiles
            .save_associations(&profile_id(), associations())
            .expect("associations save");
        source.dids.upsert(did_record()).expect("DID save");
        source
            .credentials
            .upsert(credential_record())
            .expect("credential save");
        source
            .custody
            .initialize(&profile_id())
            .expect("custody initialize");
        source
            .custody
            .generate(
                &profile_id(),
                GenerateProtectedKeyRequest {
                    label: WalletKeyLabel::parse("Identity key").expect("label"),
                    algorithm: WalletKeyAlgorithm::Ed25519,
                    purpose: WalletKeyPurpose::Authentication,
                },
            )
            .expect("key generate");
        source
    }

    #[test]
    fn complete_archive_recovers_every_profile_scoped_store_into_a_fresh_install() {
        let source = populated_source();
        let backup = source
            .adapter
            .export_complete_wallet_backup(&profile_id(), &secret())
            .expect("complete export");
        let destination = adapter(security(71), 81);
        let summary = destination
            .adapter
            .recover_complete_wallet_backup(None, &backup, &secret())
            .expect("fresh install recovery");

        assert_eq!(
            summary,
            CompleteWalletRecoverySummary {
                profile_id: "profile_one".to_owned(),
                restored_key_count: 1,
                restored_did_count: 1,
                restored_credential_count: 1,
            }
        );
        assert_eq!(
            destination.profiles.active().expect("active"),
            Some(profile())
        );
        assert_eq!(
            destination
                .profiles
                .load_associations(&profile_id())
                .expect("associations"),
            Some(associations())
        );
        let dids = destination
            .dids
            .list(&IdentityProfileId::parse("profile_one").expect("profile"))
            .expect("DIDs");
        assert_eq!(dids.len(), 1);
        assert_eq!(dids[0].resolution().source(), DidResolutionSource::Stored);
        assert_eq!(
            destination
                .credentials
                .list(&CredentialProfileId::parse("profile_one").expect("profile"))
                .expect("credentials"),
            vec![credential_record()]
        );
        assert_eq!(
            destination
                .custody
                .list(&profile_id())
                .expect("restored keys")
                .len(),
            1
        );
        assert!(destination.journal.load().expect("journal").is_none());
    }

    #[test]
    fn authorization_failure_rolls_every_staged_public_record_back() {
        let source = populated_source();
        let backup = source
            .adapter
            .export_complete_wallet_backup(&profile_id(), &secret())
            .expect("complete export");
        let destination = adapter(Arc::new(RejectingCustody), 81);
        assert_eq!(
            destination.adapter.recover_complete_wallet_backup(
                Some(&profile_id()),
                &backup,
                &secret(),
            ),
            Err(WalletPortableBackupPortError::AuthorizationDenied)
        );
        assert!(destination.profiles.list().expect("profiles").is_empty());
        assert!(
            destination
                .dids
                .list(&IdentityProfileId::parse("profile_one").expect("profile"))
                .expect("DIDs")
                .is_empty()
        );
        assert!(
            destination
                .credentials
                .list(&CredentialProfileId::parse("profile_one").expect("profile"))
                .expect("credentials")
                .is_empty()
        );
        assert!(destination.journal.load().expect("journal").is_none());
    }

    #[test]
    fn retry_reconciles_a_crash_after_custody_commit() {
        let source = populated_source();
        let backup = source
            .adapter
            .export_complete_wallet_backup(&profile_id(), &secret())
            .expect("complete export");
        let destination = adapter(security(71), 81);
        let archive = open_complete_wallet_archive(&backup, &secret(), None).expect("open");
        let state = PreparedWalletState::decode(&archive).expect("decode");
        let mut journal = RecoveryJournal::prepared(&state, archive.custody(), None);
        destination
            .journal
            .save(&journal)
            .expect("journal prepared");
        destination
            .adapter
            .stage_public(&state)
            .expect("public staged");
        journal.phase = RecoveryPhase::PublicStaged;
        destination.journal.save(&journal).expect("journal staged");
        destination
            .custody
            .recover_custody_vault(archive.custody())
            .expect("custody committed");

        let summary = destination
            .adapter
            .recover_complete_wallet_backup(None, &backup, &secret())
            .expect("retry reconciles");
        assert_eq!(summary.restored_key_count, 1);
        assert_eq!(
            destination.profiles.active().expect("active"),
            Some(profile())
        );
        assert!(destination.journal.load().expect("journal").is_none());
    }

    #[test]
    fn file_journal_is_owner_private_strict_and_clearable() {
        let source = populated_source();
        let custody = source
            .custody
            .export_custody_vault(&profile_id())
            .expect("custody");
        let state = PreparedWalletState {
            profile: profile(),
            associations: Some(associations()),
            dids: vec![did_record()],
            credentials: vec![credential_record()],
        };
        let expected = RecoveryJournal::prepared(&state, &custody, None);
        let root = env::temp_dir().join(format!(
            "oxid-complete-backup-journal-{}-{}",
            std::process::id(),
            DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("private/recovery.json");
        let journal = FileRecoveryJournal::new(&path).expect("journal path");
        journal.save(&expected).expect("journal save");
        assert_eq!(journal.load().expect("journal load"), Some(expected));
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
            0o600
        );
        journal.clear().expect("journal clear");
        assert!(journal.load().expect("journal absent").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn archive_sections_do_not_expose_private_credentials_or_custody() {
        let source = populated_source();
        let backup = source
            .adapter
            .export_complete_wallet_backup(&profile_id(), &secret())
            .expect("complete export");
        for secret_bytes in [
            b"signed-private-credential".as_slice(),
            b"claim-opening-material".as_slice(),
            b"detached-credential-proof".as_slice(),
        ] {
            assert!(
                !backup
                    .as_bytes()
                    .windows(secret_bytes.len())
                    .any(|window| window == secret_bytes)
            );
        }
        assert!(!format!("{backup:?}").contains("signed-private-credential"));
    }

    #[test]
    fn credential_snapshot_inputs_remain_zeroized_types() {
        let encoded = encode_portable_credential_snapshot(&[credential_record()])
            .expect("credential snapshot");
        let _: &Zeroizing<Vec<u8>> = &encoded;
    }
}
