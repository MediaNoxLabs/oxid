// SPDX-License-Identifier: Apache-2.0

//! Bounded, adapter-local persistence for public Midnight account replay state.

use std::{
    collections::BTreeSet,
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use oxid_foundation::UnixTimestampMillis;
use oxid_wallet_domain::{ChainAddress, ChainNetworkId};
use serde::{Deserialize, Serialize};

use crate::indexer::IndexerSnapshot;

const SCHEMA_VERSION: u32 = 1;
const MAX_CHECKPOINT_COUNT: usize = 128;
const MAX_CHECKPOINT_BYTES: u64 = 16 * 1024 * 1024;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Validated location for public account replay checkpoints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightAccountCheckpointConfig {
    path: PathBuf,
}

impl MidnightAccountCheckpointConfig {
    /// Accepts only an explicit absolute file path.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, MidnightAccountCheckpointConfigError> {
        let path = path.into();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(MidnightAccountCheckpointConfigError::InvalidPath);
        }
        Ok(Self { path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Safe checkpoint configuration failure without rendering the supplied path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightAccountCheckpointConfigError {
    InvalidPath,
}

impl std::fmt::Display for MidnightAccountCheckpointConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Midnight account checkpoint path must be an absolute file path")
    }
}

impl std::error::Error for MidnightAccountCheckpointConfigError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredIndexerCheckpoint {
    pub(crate) updated_at: UnixTimestampMillis,
    pub(crate) snapshot: IndexerSnapshot,
}

pub(crate) trait MidnightAccountCheckpointStore: Send + Sync {
    fn load(
        &self,
        network_id: &ChainNetworkId,
        address: &ChainAddress,
    ) -> Result<Option<StoredIndexerCheckpoint>, CheckpointStoreError>;

    fn save(
        &self,
        network_id: &ChainNetworkId,
        address: &ChainAddress,
        checkpoint: &StoredIndexerCheckpoint,
    ) -> Result<(), CheckpointStoreError>;
}

pub(crate) struct UnavailableMidnightAccountCheckpointStore;

impl MidnightAccountCheckpointStore for UnavailableMidnightAccountCheckpointStore {
    fn load(
        &self,
        _: &ChainNetworkId,
        _: &ChainAddress,
    ) -> Result<Option<StoredIndexerCheckpoint>, CheckpointStoreError> {
        Ok(None)
    }

    fn save(
        &self,
        _: &ChainNetworkId,
        _: &ChainAddress,
        _: &StoredIndexerCheckpoint,
    ) -> Result<(), CheckpointStoreError> {
        Ok(())
    }
}

pub(crate) struct JsonMidnightAccountCheckpointStore {
    path: PathBuf,
    access: Mutex<()>,
}

impl JsonMidnightAccountCheckpointStore {
    pub(crate) fn new(config: MidnightAccountCheckpointConfig) -> Self {
        Self {
            path: config.path,
            access: Mutex::new(()),
        }
    }

    fn load_document(&self) -> Result<CheckpointDocument, CheckpointStoreError> {
        reject_symlink(&self.path)?;
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CheckpointDocument::default());
            }
            Err(_) => return Err(CheckpointStoreError::Unavailable),
        };
        let metadata = file
            .metadata()
            .map_err(|_| CheckpointStoreError::Unavailable)?;
        if !metadata.is_file() || metadata.len() > MAX_CHECKPOINT_BYTES {
            return Err(CheckpointStoreError::InvalidData);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(CheckpointStoreError::InvalidData);
            }
        }

        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_CHECKPOINT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CheckpointStoreError::Unavailable)?;
        if bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
            return Err(CheckpointStoreError::InvalidData);
        }
        let document: CheckpointDocument =
            serde_json::from_slice(&bytes).map_err(|_| CheckpointStoreError::InvalidData)?;
        validate_document(&document)?;
        Ok(document)
    }

    fn save_document(&self, document: &CheckpointDocument) -> Result<(), CheckpointStoreError> {
        validate_document(document)?;
        reject_symlink(&self.path)?;
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|_| CheckpointStoreError::Unavailable)?;
        }

        let bytes =
            serde_json::to_vec_pretty(document).map_err(|_| CheckpointStoreError::InvalidData)?;
        if bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
            return Err(CheckpointStoreError::InvalidData);
        }

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
            .map_err(|_| CheckpointStoreError::Unavailable)?;
        if file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(CheckpointStoreError::Unavailable);
        }
        drop(file);

        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|_| CheckpointStoreError::Unavailable)?;
        }
        if fs::rename(&temporary_path, &self.path).is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(CheckpointStoreError::Unavailable);
        }
        #[cfg(unix)]
        if let Some(parent) = self.path.parent() {
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| CheckpointStoreError::Unavailable)?;
        }
        Ok(())
    }
}

impl MidnightAccountCheckpointStore for JsonMidnightAccountCheckpointStore {
    fn load(
        &self,
        network_id: &ChainNetworkId,
        address: &ChainAddress,
    ) -> Result<Option<StoredIndexerCheckpoint>, CheckpointStoreError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| CheckpointStoreError::Unavailable)?;
        Ok(self
            .load_document()?
            .checkpoints
            .into_iter()
            .find(|record| {
                record.network_id == network_id.as_str() && record.address == address.value()
            })
            .map(|record| StoredIndexerCheckpoint {
                updated_at: UnixTimestampMillis::new(record.updated_at_millis),
                snapshot: record.snapshot,
            }))
    }

    fn save(
        &self,
        network_id: &ChainNetworkId,
        address: &ChainAddress,
        checkpoint: &StoredIndexerCheckpoint,
    ) -> Result<(), CheckpointStoreError> {
        checkpoint.snapshot.validate_checkpoint()?;
        let _guard = self
            .access
            .lock()
            .map_err(|_| CheckpointStoreError::Unavailable)?;
        let mut document = match self.load_document() {
            Ok(document) => document,
            Err(CheckpointStoreError::InvalidData) => CheckpointDocument::default(),
            Err(error) => return Err(error),
        };
        let replacement = CheckpointRecord {
            network_id: network_id.as_str().to_owned(),
            address: address.value().to_owned(),
            updated_at_millis: checkpoint.updated_at.value(),
            snapshot: checkpoint.snapshot.clone(),
        };
        if let Some(record) = document.checkpoints.iter_mut().find(|record| {
            record.network_id == replacement.network_id && record.address == replacement.address
        }) {
            *record = replacement;
        } else {
            if document.checkpoints.len() >= MAX_CHECKPOINT_COUNT {
                return Err(CheckpointStoreError::InvalidData);
            }
            document.checkpoints.push(replacement);
        }
        document.checkpoints.sort_by(|left, right| {
            (&left.network_id, &left.address).cmp(&(&right.network_id, &right.address))
        });
        self.save_document(&document)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CheckpointDocument {
    schema_version: u32,
    checkpoints: Vec<CheckpointRecord>,
}

impl Default for CheckpointDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            checkpoints: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CheckpointRecord {
    network_id: String,
    address: String,
    updated_at_millis: u64,
    snapshot: IndexerSnapshot,
}

fn validate_document(document: &CheckpointDocument) -> Result<(), CheckpointStoreError> {
    if document.schema_version != SCHEMA_VERSION
        || document.checkpoints.len() > MAX_CHECKPOINT_COUNT
    {
        return Err(CheckpointStoreError::InvalidData);
    }
    let mut keys = BTreeSet::new();
    for record in &document.checkpoints {
        if record.network_id.is_empty()
            || record.network_id.len() > 64
            || record.address.is_empty()
            || record.address.len() > 512
            || record.network_id.chars().any(char::is_control)
            || record.address.chars().any(char::is_control)
            || !keys.insert((&record.network_id, &record.address))
        {
            return Err(CheckpointStoreError::InvalidData);
        }
        let network_id = ChainNetworkId::parse(record.network_id.clone())
            .map_err(|_| CheckpointStoreError::InvalidData)?;
        if crate::network_by_id(&network_id)
            .map_err(|_| CheckpointStoreError::InvalidData)?
            .is_none()
            || crate::indexer::validate_unshielded_address(&network_id, &record.address).is_err()
        {
            return Err(CheckpointStoreError::InvalidData);
        }
        record.snapshot.validate_checkpoint()?;
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<(), CheckpointStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(CheckpointStoreError::InvalidData),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(CheckpointStoreError::Unavailable),
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("midnight-account-checkpoints.json");
    path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckpointStoreError {
    Unavailable,
    InvalidData,
}

impl From<crate::indexer::IndexerTransportError> for CheckpointStoreError {
    fn from(_: crate::indexer::IndexerTransportError) -> Self {
        Self::InvalidData
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use oxid_wallet_domain::ChainNetworkId;

    use super::*;
    use crate::indexer::{
        IndexerSnapshot, IndexerTransaction, IndexerTransactionStatus, IndexerUtxo,
    };

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "oxid-midnight-checkpoint-test-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("test directory should be created");
            Self(path)
        }

        fn file(&self) -> PathBuf {
            self.0.join("account-checkpoints.json")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn network() -> ChainNetworkId {
        ChainNetworkId::parse("devnet").expect("network fixture should be valid")
    }

    fn address() -> ChainAddress {
        crate::fixture_addresses(&network()).expect("address fixtures should be valid")[0].clone()
    }

    fn utxo(intent: char, value: u128) -> IndexerUtxo {
        IndexerUtxo {
            token_type: "00".repeat(32),
            value,
            intent_hash: intent.to_string().repeat(64),
            output_index: 0,
        }
    }

    fn snapshot() -> IndexerSnapshot {
        let available = utxo('a', u128::MAX);
        IndexerSnapshot {
            current_cursor: 7,
            target_cursor: 7,
            chain_tip_height: Some(42),
            utxos: vec![available.clone()],
            transactions: vec![IndexerTransaction {
                hash: "bc".repeat(32),
                block_height: 42,
                timestamp_millis: 1_700_000_000_000,
                status: IndexerTransactionStatus::Success,
                fee_specks: Some(u128::MAX),
                created: vec![available],
                spent: Vec::new(),
            }],
        }
    }

    fn store(path: PathBuf) -> JsonMidnightAccountCheckpointStore {
        JsonMidnightAccountCheckpointStore::new(
            MidnightAccountCheckpointConfig::new(path)
                .expect("absolute test checkpoint path should be valid"),
        )
    }

    #[test]
    fn configuration_requires_an_absolute_file_path() {
        assert_eq!(
            MidnightAccountCheckpointConfig::new("relative.json"),
            Err(MidnightAccountCheckpointConfigError::InvalidPath)
        );
        assert!(
            MidnightAccountCheckpointConfig::new(std::env::temp_dir().join("state.json")).is_ok()
        );
    }

    #[test]
    fn checkpoint_round_trip_preserves_exact_public_state_and_scope() {
        let directory = TestDirectory::new();
        let store = store(directory.file());
        let expected = StoredIndexerCheckpoint {
            updated_at: UnixTimestampMillis::new(1_700_000_000_123),
            snapshot: snapshot(),
        };
        store
            .save(&network(), &address(), &expected)
            .expect("checkpoint should save");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(directory.file())
                    .expect("checkpoint metadata should be readable")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        let loaded = store
            .load(&network(), &address())
            .expect("checkpoint should load")
            .expect("checkpoint should exist");
        assert_eq!(loaded.updated_at, expected.updated_at);
        assert_eq!(loaded.snapshot, expected.snapshot);
        assert_eq!(loaded.snapshot.utxos[0].value, u128::MAX);
        assert!(
            store
                .load(
                    &ChainNetworkId::parse("preprod").expect("network should parse"),
                    &address()
                )
                .expect("different scope should be readable")
                .is_none()
        );
    }

    #[test]
    fn checkpoint_validation_rejects_incomplete_duplicate_and_inconsistent_state() {
        let directory = TestDirectory::new();
        let store = store(directory.file());
        let save = |snapshot| {
            store.save(
                &network(),
                &address(),
                &StoredIndexerCheckpoint {
                    updated_at: UnixTimestampMillis::new(10),
                    snapshot,
                },
            )
        };

        let mut incomplete = snapshot();
        incomplete.target_cursor = incomplete.current_cursor + 1;
        assert_eq!(save(incomplete), Err(CheckpointStoreError::InvalidData));

        let mut duplicate = snapshot();
        duplicate.utxos.push(duplicate.utxos[0].clone());
        assert_eq!(save(duplicate), Err(CheckpointStoreError::InvalidData));

        let mut inconsistent_tip = snapshot();
        inconsistent_tip.chain_tip_height = Some(43);
        assert_eq!(
            save(inconsistent_tip),
            Err(CheckpointStoreError::InvalidData)
        );

        assert_eq!(
            store.save(
                &ChainNetworkId::parse("preprod").expect("network should parse"),
                &address(),
                &StoredIndexerCheckpoint {
                    updated_at: UnixTimestampMillis::new(10),
                    snapshot: snapshot(),
                }
            ),
            Err(CheckpointStoreError::InvalidData)
        );
    }

    #[test]
    fn malformed_checkpoint_is_rejected_and_a_fresh_sync_can_replace_it() {
        let directory = TestDirectory::new();
        let path = directory.file();
        fs::write(&path, b"not-json").expect("malformed fixture should write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .expect("fixture permissions should set");
        }
        let store = store(path);
        assert_eq!(
            store.load(&network(), &address()),
            Err(CheckpointStoreError::InvalidData)
        );

        let expected = StoredIndexerCheckpoint {
            updated_at: UnixTimestampMillis::new(10),
            snapshot: snapshot(),
        };
        store
            .save(&network(), &address(), &expected)
            .expect("fresh state should replace malformed data");
        assert_eq!(
            store
                .load(&network(), &address())
                .expect("replacement should load")
                .map(|checkpoint| checkpoint.snapshot),
            Some(expected.snapshot)
        );
    }

    #[cfg(unix)]
    #[test]
    fn checkpoint_store_rejects_symlink_targets_and_permissive_files() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};

        let directory = TestDirectory::new();
        let real = directory.0.join("real.json");
        fs::write(&real, b"{}").expect("target fixture should write");
        let link = directory.file();
        symlink(&real, &link).expect("symlink fixture should create");
        assert_eq!(
            store(link).load(&network(), &address()),
            Err(CheckpointStoreError::InvalidData)
        );

        fs::set_permissions(&real, fs::Permissions::from_mode(0o644))
            .expect("fixture permissions should set");
        assert_eq!(
            store(real).load(&network(), &address()),
            Err(CheckpointStoreError::InvalidData)
        );
    }

    #[test]
    fn checkpoint_store_rejects_oversized_files_before_parsing() {
        let directory = TestDirectory::new();
        let path = directory.file();
        let file = fs::File::create(&path).expect("oversized fixture should create");
        file.set_len(MAX_CHECKPOINT_BYTES + 1)
            .expect("oversized fixture should resize");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .expect("fixture permissions should set");
        }
        assert_eq!(
            store(path).load(&network(), &address()),
            Err(CheckpointStoreError::InvalidData)
        );
    }
}
