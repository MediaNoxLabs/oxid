// SPDX-License-Identifier: Apache-2.0

//! Bounded persistence for public Midnight submission reconciliation metadata.

use std::{
    collections::BTreeSet,
    fs,
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use oxid_foundation::UnixTimestampMillis;
use oxid_wallet_domain::{
    ChainNetworkId, WalletProfileId, WalletTransactionDraftId, WalletTransferSubmissionMode,
};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
const MAX_RECORDS: usize = 128;
const MAX_FILE_BYTES: u64 = 256 * 1024;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Validated location for public transaction submission metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightSubmissionJournalConfig {
    path: PathBuf,
}

impl MidnightSubmissionJournalConfig {
    /// Accepts only an explicit normalized absolute file path.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, MidnightSubmissionJournalConfigError> {
        let path = path.into();
        if !path.is_absolute()
            || path.file_name().is_none()
            || path
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(MidnightSubmissionJournalConfigError::InvalidPath);
        }
        Ok(Self { path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Safe configuration failure that never renders a filesystem path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightSubmissionJournalConfigError {
    InvalidPath,
}

impl std::fmt::Display for MidnightSubmissionJournalConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .write_str("Midnight submission journal path must be a normalized absolute file path")
    }
}

impl std::error::Error for MidnightSubmissionJournalConfigError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoredSubmissionState {
    Broadcasting,
    OutcomeUnknown,
    Included,
    Rejected,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredSubmissionJournalEntry {
    pub(crate) profile_id: WalletProfileId,
    pub(crate) network_id: ChainNetworkId,
    pub(crate) draft_id: WalletTransactionDraftId,
    pub(crate) planning_fingerprint: [u8; 32],
    pub(crate) expires_at: UnixTimestampMillis,
    pub(crate) updated_at: UnixTimestampMillis,
    pub(crate) fee_specks: u128,
    pub(crate) transaction_hash: [u8; 32],
    pub(crate) anchor_block_hash: [u8; 32],
    pub(crate) block_hash: Option<[u8; 32]>,
    pub(crate) state: StoredSubmissionState,
    pub(crate) mode: WalletTransferSubmissionMode,
}

pub(crate) trait MidnightSubmissionJournalStore: Send + Sync {
    fn load(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<Option<StoredSubmissionJournalEntry>, SubmissionJournalStoreError>;

    fn find_planning_fingerprint(
        &self,
        profile_id: &WalletProfileId,
        fingerprint: &[u8; 32],
    ) -> Result<Option<StoredSubmissionJournalEntry>, SubmissionJournalStoreError>;

    fn list(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Vec<StoredSubmissionJournalEntry>, SubmissionJournalStoreError>;

    fn save(&self, entry: &StoredSubmissionJournalEntry)
    -> Result<(), SubmissionJournalStoreError>;
}

pub(crate) struct UnavailableMidnightSubmissionJournalStore;

impl MidnightSubmissionJournalStore for UnavailableMidnightSubmissionJournalStore {
    fn load(
        &self,
        _: &WalletProfileId,
        _: &WalletTransactionDraftId,
    ) -> Result<Option<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        Ok(None)
    }

    fn find_planning_fingerprint(
        &self,
        _: &WalletProfileId,
        _: &[u8; 32],
    ) -> Result<Option<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        Ok(None)
    }

    fn list(
        &self,
        _: &WalletProfileId,
    ) -> Result<Vec<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        Ok(Vec::new())
    }

    fn save(&self, _: &StoredSubmissionJournalEntry) -> Result<(), SubmissionJournalStoreError> {
        Err(SubmissionJournalStoreError::Unavailable)
    }
}

#[derive(Default)]
pub(crate) struct MemoryMidnightSubmissionJournalStore {
    entries: Mutex<Vec<StoredSubmissionJournalEntry>>,
}

impl MidnightSubmissionJournalStore for MemoryMidnightSubmissionJournalStore {
    fn load(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<Option<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        self.entries
            .lock()
            .map_err(|_| SubmissionJournalStoreError::Unavailable)
            .map(|entries| {
                entries
                    .iter()
                    .find(|entry| &entry.profile_id == profile_id && &entry.draft_id == draft_id)
                    .cloned()
            })
    }

    fn find_planning_fingerprint(
        &self,
        profile_id: &WalletProfileId,
        fingerprint: &[u8; 32],
    ) -> Result<Option<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        self.entries
            .lock()
            .map_err(|_| SubmissionJournalStoreError::Unavailable)
            .map(|entries| {
                entries
                    .iter()
                    .find(|entry| {
                        &entry.profile_id == profile_id
                            && &entry.planning_fingerprint == fingerprint
                    })
                    .cloned()
            })
    }

    fn list(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Vec<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        self.entries
            .lock()
            .map_err(|_| SubmissionJournalStoreError::Unavailable)
            .map(|entries| sorted_profile_entries(entries.as_slice(), profile_id))
    }

    fn save(
        &self,
        entry: &StoredSubmissionJournalEntry,
    ) -> Result<(), SubmissionJournalStoreError> {
        validate_entry(entry)?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        upsert_entry(&mut entries, entry.clone());
        Ok(())
    }
}

pub(crate) struct JsonMidnightSubmissionJournalStore {
    path: PathBuf,
    access: Mutex<()>,
}

impl JsonMidnightSubmissionJournalStore {
    pub(crate) fn new(config: MidnightSubmissionJournalConfig) -> Self {
        Self {
            path: config.path,
            access: Mutex::new(()),
        }
    }

    fn load_entries(
        &self,
    ) -> Result<Vec<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        reject_symlink(&self.path)?;
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(SubmissionJournalStoreError::Unavailable),
        };
        let metadata = file
            .metadata()
            .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err(SubmissionJournalStoreError::InvalidData);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(SubmissionJournalStoreError::InvalidData);
            }
        }
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(SubmissionJournalStoreError::InvalidData)?;
        validate_private_directory(parent)?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(SubmissionJournalStoreError::InvalidData);
        }
        decode_document(&bytes)
    }

    fn save_entries(
        &self,
        entries: &[StoredSubmissionJournalEntry],
    ) -> Result<(), SubmissionJournalStoreError> {
        let bytes = encode_document(entries)?;
        reject_symlink(&self.path)?;
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(SubmissionJournalStoreError::InvalidData)?;
        ensure_private_directory(parent)?;
        let temporary_path = temporary_path(&self.path);
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary_path)
            .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        if file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(SubmissionJournalStoreError::Unavailable);
        }
        drop(file);
        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        }
        if fs::rename(&temporary_path, &self.path).is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(SubmissionJournalStoreError::Unavailable);
        }
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        Ok(())
    }
}

impl MidnightSubmissionJournalStore for JsonMidnightSubmissionJournalStore {
    fn load(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<Option<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        self.load_entries().map(|entries| {
            entries
                .into_iter()
                .find(|entry| &entry.profile_id == profile_id && &entry.draft_id == draft_id)
        })
    }

    fn find_planning_fingerprint(
        &self,
        profile_id: &WalletProfileId,
        fingerprint: &[u8; 32],
    ) -> Result<Option<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        self.load_entries().map(|entries| {
            entries.into_iter().find(|entry| {
                &entry.profile_id == profile_id && &entry.planning_fingerprint == fingerprint
            })
        })
    }

    fn list(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Vec<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        self.load_entries()
            .map(|entries| sorted_profile_entries(&entries, profile_id))
    }

    fn save(
        &self,
        entry: &StoredSubmissionJournalEntry,
    ) -> Result<(), SubmissionJournalStoreError> {
        validate_entry(entry)?;
        let _guard = self
            .access
            .lock()
            .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        let mut entries = self.load_entries()?;
        upsert_entry(&mut entries, entry.clone());
        self.save_entries(&entries)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubmissionJournalStoreError {
    Unavailable,
    InvalidData,
}

fn upsert_entry(
    entries: &mut Vec<StoredSubmissionJournalEntry>,
    entry: StoredSubmissionJournalEntry,
) {
    if let Some(existing) = entries.iter_mut().find(|existing| {
        existing.profile_id == entry.profile_id && existing.draft_id == entry.draft_id
    }) {
        *existing = entry;
    } else {
        entries.push(entry);
    }
    entries.sort_by_key(|entry| entry.updated_at.value());
    if entries.len() > MAX_RECORDS {
        entries.drain(..entries.len() - MAX_RECORDS);
    }
}

fn sorted_profile_entries(
    entries: &[StoredSubmissionJournalEntry],
    profile_id: &WalletProfileId,
) -> Vec<StoredSubmissionJournalEntry> {
    let mut selected = entries
        .iter()
        .filter(|entry| &entry.profile_id == profile_id)
        .cloned()
        .collect::<Vec<_>>();
    selected.sort_by_key(|entry| std::cmp::Reverse(entry.updated_at.value()));
    selected
}

fn validate_entry(entry: &StoredSubmissionJournalEntry) -> Result<(), SubmissionJournalStoreError> {
    if entry.transaction_hash == [0; 32]
        || (entry.mode == WalletTransferSubmissionMode::Live && entry.anchor_block_hash == [0; 32])
        || (entry.state == StoredSubmissionState::Included) != entry.block_hash.is_some()
    {
        return Err(SubmissionJournalStoreError::InvalidData);
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalDocument {
    version: u32,
    records: Vec<JournalRecord>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalRecord {
    profile_id: String,
    network_id: String,
    draft_id: String,
    planning_fingerprint: String,
    expires_at_millis: u64,
    updated_at_millis: u64,
    fee_specks: String,
    transaction_hash: String,
    anchor_block_hash: String,
    block_hash: Option<String>,
    state: String,
    mode: String,
}

fn encode_document(
    entries: &[StoredSubmissionJournalEntry],
) -> Result<Vec<u8>, SubmissionJournalStoreError> {
    if entries.len() > MAX_RECORDS {
        return Err(SubmissionJournalStoreError::InvalidData);
    }
    let mut keys = BTreeSet::new();
    let records = entries
        .iter()
        .map(|entry| {
            validate_entry(entry)?;
            if !keys.insert((
                entry.profile_id.as_str().to_owned(),
                entry.draft_id.as_str().to_owned(),
            )) {
                return Err(SubmissionJournalStoreError::InvalidData);
            }
            Ok(JournalRecord::from(entry))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let bytes = serde_json::to_vec_pretty(&JournalDocument {
        version: SCHEMA_VERSION,
        records,
    })
    .map_err(|_| SubmissionJournalStoreError::InvalidData)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(SubmissionJournalStoreError::InvalidData);
    }
    Ok(bytes)
}

fn decode_document(
    bytes: &[u8],
) -> Result<Vec<StoredSubmissionJournalEntry>, SubmissionJournalStoreError> {
    let document: JournalDocument =
        serde_json::from_slice(bytes).map_err(|_| SubmissionJournalStoreError::InvalidData)?;
    if document.version != SCHEMA_VERSION || document.records.len() > MAX_RECORDS {
        return Err(SubmissionJournalStoreError::InvalidData);
    }
    let mut keys = BTreeSet::new();
    document
        .records
        .into_iter()
        .map(|record| {
            let entry = StoredSubmissionJournalEntry::try_from(record)?;
            if !keys.insert((
                entry.profile_id.as_str().to_owned(),
                entry.draft_id.as_str().to_owned(),
            )) {
                return Err(SubmissionJournalStoreError::InvalidData);
            }
            Ok(entry)
        })
        .collect()
}

impl From<&StoredSubmissionJournalEntry> for JournalRecord {
    fn from(entry: &StoredSubmissionJournalEntry) -> Self {
        Self {
            profile_id: entry.profile_id.as_str().to_owned(),
            network_id: entry.network_id.as_str().to_owned(),
            draft_id: entry.draft_id.as_str().to_owned(),
            planning_fingerprint: hex::encode(entry.planning_fingerprint),
            expires_at_millis: entry.expires_at.value(),
            updated_at_millis: entry.updated_at.value(),
            fee_specks: entry.fee_specks.to_string(),
            transaction_hash: hex::encode(entry.transaction_hash),
            anchor_block_hash: hex::encode(entry.anchor_block_hash),
            block_hash: entry.block_hash.map(hex::encode),
            state: stored_state_name(entry.state).to_owned(),
            mode: submission_mode_name(entry.mode).to_owned(),
        }
    }
}

impl TryFrom<JournalRecord> for StoredSubmissionJournalEntry {
    type Error = SubmissionJournalStoreError;

    fn try_from(record: JournalRecord) -> Result<Self, Self::Error> {
        let entry = Self {
            profile_id: WalletProfileId::parse(record.profile_id)
                .map_err(|_| SubmissionJournalStoreError::InvalidData)?,
            network_id: ChainNetworkId::parse(record.network_id)
                .map_err(|_| SubmissionJournalStoreError::InvalidData)?,
            draft_id: WalletTransactionDraftId::parse(record.draft_id)
                .map_err(|_| SubmissionJournalStoreError::InvalidData)?,
            planning_fingerprint: decode_hash(&record.planning_fingerprint)?,
            expires_at: UnixTimestampMillis::new(record.expires_at_millis),
            updated_at: UnixTimestampMillis::new(record.updated_at_millis),
            fee_specks: record
                .fee_specks
                .parse()
                .map_err(|_| SubmissionJournalStoreError::InvalidData)?,
            transaction_hash: decode_hash(&record.transaction_hash)?,
            anchor_block_hash: decode_hash(&record.anchor_block_hash)?,
            block_hash: record
                .block_hash
                .map(|value| decode_hash(&value))
                .transpose()?,
            state: parse_stored_state(&record.state)?,
            mode: parse_submission_mode(&record.mode)?,
        };
        validate_entry(&entry)?;
        Ok(entry)
    }
}

fn decode_hash(value: &str) -> Result<[u8; 32], SubmissionJournalStoreError> {
    let bytes = hex::decode(value).map_err(|_| SubmissionJournalStoreError::InvalidData)?;
    bytes
        .try_into()
        .map_err(|_| SubmissionJournalStoreError::InvalidData)
}

const fn stored_state_name(state: StoredSubmissionState) -> &'static str {
    match state {
        StoredSubmissionState::Broadcasting => "broadcasting",
        StoredSubmissionState::OutcomeUnknown => "outcome_unknown",
        StoredSubmissionState::Included => "included",
        StoredSubmissionState::Rejected => "rejected",
        StoredSubmissionState::Expired => "expired",
    }
}

fn parse_stored_state(value: &str) -> Result<StoredSubmissionState, SubmissionJournalStoreError> {
    match value {
        "broadcasting" => Ok(StoredSubmissionState::Broadcasting),
        "outcome_unknown" => Ok(StoredSubmissionState::OutcomeUnknown),
        "included" => Ok(StoredSubmissionState::Included),
        "rejected" => Ok(StoredSubmissionState::Rejected),
        "expired" => Ok(StoredSubmissionState::Expired),
        _ => Err(SubmissionJournalStoreError::InvalidData),
    }
}

const fn submission_mode_name(mode: WalletTransferSubmissionMode) -> &'static str {
    match mode {
        WalletTransferSubmissionMode::Simulated => "simulated",
        WalletTransferSubmissionMode::Live => "live",
    }
}

fn parse_submission_mode(
    value: &str,
) -> Result<WalletTransferSubmissionMode, SubmissionJournalStoreError> {
    match value {
        "simulated" => Ok(WalletTransferSubmissionMode::Simulated),
        "live" => Ok(WalletTransferSubmissionMode::Live),
        _ => Err(SubmissionJournalStoreError::InvalidData),
    }
}

fn reject_symlink(path: &Path) -> Result<(), SubmissionJournalStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(SubmissionJournalStoreError::InvalidData)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SubmissionJournalStoreError::Unavailable),
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), SubmissionJournalStoreError> {
    reject_symlink(path)?;
    fs::create_dir_all(path).map_err(|_| SubmissionJournalStoreError::Unavailable)?;
    reject_symlink(path)?;
    let metadata =
        fs::symlink_metadata(path).map_err(|_| SubmissionJournalStoreError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(SubmissionJournalStoreError::InvalidData);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = metadata.permissions();
        if permissions.mode() & 0o077 != 0 {
            permissions.set_mode(0o700);
            fs::set_permissions(path, permissions)
                .map_err(|_| SubmissionJournalStoreError::Unavailable)?;
        }
    }
    validate_private_directory(path)
}

fn validate_private_directory(path: &Path) -> Result<(), SubmissionJournalStoreError> {
    reject_symlink(path)?;
    let metadata =
        fs::symlink_metadata(path).map_err(|_| SubmissionJournalStoreError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(SubmissionJournalStoreError::InvalidData);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(SubmissionJournalStoreError::InvalidData);
        }
    }
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("tmp-{}-{sequence}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "oxid-submission-journal-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temporary directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn entry(state: StoredSubmissionState) -> StoredSubmissionJournalEntry {
        StoredSubmissionJournalEntry {
            profile_id: WalletProfileId::parse("profile-test").expect("profile"),
            network_id: ChainNetworkId::parse("undeployed").expect("network"),
            draft_id: WalletTransactionDraftId::parse("txdraft-test").expect("draft"),
            planning_fingerprint: [1; 32],
            expires_at: UnixTimestampMillis::new(20),
            updated_at: UnixTimestampMillis::new(10),
            fee_specks: 42,
            transaction_hash: [2; 32],
            anchor_block_hash: [3; 32],
            block_hash: (state == StoredSubmissionState::Included).then_some([4; 32]),
            state,
            mode: WalletTransferSubmissionMode::Live,
        }
    }

    #[test]
    fn journal_round_trip_preserves_only_public_bounded_metadata() {
        let directory = TestDirectory::new("round-trip");
        let config = MidnightSubmissionJournalConfig::new(directory.path().join("journal.json"))
            .expect("valid path");
        let store = JsonMidnightSubmissionJournalStore::new(config.clone());
        let included = entry(StoredSubmissionState::Included);
        store.save(&included).expect("save succeeds");
        assert_eq!(
            store
                .load(&included.profile_id, &included.draft_id)
                .expect("load succeeds"),
            Some(included.clone())
        );
        let bytes = fs::read(config.path()).expect("journal is readable");
        let text = String::from_utf8(bytes).expect("journal is utf8");
        assert!(!text.contains("proof"));
        assert!(!text.contains("signature"));
        assert!(!text.contains("endpoint"));
        assert_eq!(
            store
                .find_planning_fingerprint(&included.profile_id, &[1; 32])
                .expect("fingerprint lookup succeeds"),
            Some(included)
        );
    }

    #[test]
    fn journal_rejects_invalid_paths_and_permissive_or_malformed_files() {
        assert_eq!(
            MidnightSubmissionJournalConfig::new("relative.json"),
            Err(MidnightSubmissionJournalConfigError::InvalidPath)
        );
        let directory = TestDirectory::new("invalid");
        let path = directory.path().join("journal.json");
        fs::write(&path, b"{}").expect("fixture writes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .expect("permissions update");
        }
        let store = JsonMidnightSubmissionJournalStore::new(
            MidnightSubmissionJournalConfig::new(path).expect("valid path"),
        );
        assert_eq!(
            store.list(&WalletProfileId::parse("profile-test").expect("profile")),
            Err(SubmissionJournalStoreError::InvalidData)
        );

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(directory.path(), directory.path().join("linked"))
                .expect("fixture symlink is created");
            let linked = JsonMidnightSubmissionJournalStore::new(
                MidnightSubmissionJournalConfig::new(
                    directory.path().join("linked/submissions.json"),
                )
                .expect("linked path is structurally valid"),
            );
            assert_eq!(
                linked.save(&entry(StoredSubmissionState::Broadcasting)),
                Err(SubmissionJournalStoreError::InvalidData)
            );
        }
    }
}
