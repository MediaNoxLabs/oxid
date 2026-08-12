// SPDX-License-Identifier: Apache-2.0

//! Bounded, adapter-private persistence for official Midnight DUST state.

use std::{
    collections::BTreeSet,
    fs,
    io::{Cursor, Read as _, Write as _},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use midnight_ledger::dust::{DustLocalState, DustParameters, DustPublicKey};
use midnight_storage::DefaultDB;
use oxid_foundation::UnixTimestampMillis;
use oxid_wallet_domain::ChainNetworkId;
use sha2::{Digest as _, Sha256};

const MAGIC: &[u8; 8] = b"OXIDDUST";
const SCHEMA_VERSION: u32 = 1;
const MAX_RECORDS: usize = 4;
const MAX_NETWORK_BYTES: usize = 64;
const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Validated location for key-specific DUST replay checkpoints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightDustCheckpointConfig {
    path: PathBuf,
}

impl MidnightDustCheckpointConfig {
    /// Accepts only an explicit normalized absolute file path.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, MidnightDustCheckpointConfigError> {
        let path = path.into();
        if !path.is_absolute()
            || path.file_name().is_none()
            || path
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(MidnightDustCheckpointConfigError::InvalidPath);
        }
        Ok(Self { path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Safe configuration failure that never renders private filesystem details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightDustCheckpointConfigError {
    InvalidPath,
}

impl std::fmt::Display for MidnightDustCheckpointConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Midnight DUST checkpoint path must be a normalized absolute file path")
    }
}

impl std::error::Error for MidnightDustCheckpointConfigError {}

#[derive(Clone, Debug)]
pub(crate) struct StoredDustCheckpoint {
    pub(crate) current_cursor: u64,
    pub(crate) target_cursor: u64,
    pub(crate) updated_at: UnixTimestampMillis,
    pub(crate) state: DustLocalState<DefaultDB>,
}

pub(crate) trait MidnightDustCheckpointStore: Send + Sync {
    fn load(
        &self,
        network_id: &ChainNetworkId,
        public_key: &DustPublicKey,
        parameters: DustParameters,
    ) -> Result<Option<StoredDustCheckpoint>, DustCheckpointStoreError>;

    fn save(
        &self,
        network_id: &ChainNetworkId,
        public_key: &DustPublicKey,
        checkpoint: &StoredDustCheckpoint,
    ) -> Result<(), DustCheckpointStoreError>;
}

pub(crate) struct UnavailableMidnightDustCheckpointStore;

impl MidnightDustCheckpointStore for UnavailableMidnightDustCheckpointStore {
    fn load(
        &self,
        _: &ChainNetworkId,
        _: &DustPublicKey,
        _: DustParameters,
    ) -> Result<Option<StoredDustCheckpoint>, DustCheckpointStoreError> {
        Ok(None)
    }

    fn save(
        &self,
        _: &ChainNetworkId,
        _: &DustPublicKey,
        _: &StoredDustCheckpoint,
    ) -> Result<(), DustCheckpointStoreError> {
        Ok(())
    }
}

pub(crate) struct BinaryMidnightDustCheckpointStore {
    path: PathBuf,
    access: Mutex<()>,
}

impl BinaryMidnightDustCheckpointStore {
    pub(crate) fn new(config: MidnightDustCheckpointConfig) -> Self {
        Self {
            path: config.path,
            access: Mutex::new(()),
        }
    }

    fn load_document(&self) -> Result<Vec<DustCheckpointRecord>, DustCheckpointStoreError> {
        reject_symlink(&self.path)?;
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(DustCheckpointStoreError::Unavailable),
        };
        let metadata = file
            .metadata()
            .map_err(|_| DustCheckpointStoreError::Unavailable)?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err(DustCheckpointStoreError::InvalidData);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(DustCheckpointStoreError::InvalidData);
            }
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| DustCheckpointStoreError::Unavailable)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(DustCheckpointStoreError::InvalidData);
        }
        decode_document(&bytes)
    }

    fn save_document(
        &self,
        records: &[DustCheckpointRecord],
    ) -> Result<(), DustCheckpointStoreError> {
        let bytes = encode_document(records)?;
        reject_symlink(&self.path)?;
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(DustCheckpointStoreError::InvalidData)?;
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
            .map_err(|_| DustCheckpointStoreError::Unavailable)?;
        if file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(DustCheckpointStoreError::Unavailable);
        }
        drop(file);
        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|_| DustCheckpointStoreError::Unavailable)?;
        }
        if fs::rename(&temporary_path, &self.path).is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(DustCheckpointStoreError::Unavailable);
        }
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| DustCheckpointStoreError::Unavailable)?;
        Ok(())
    }
}

impl MidnightDustCheckpointStore for BinaryMidnightDustCheckpointStore {
    fn load(
        &self,
        network_id: &ChainNetworkId,
        public_key: &DustPublicKey,
        parameters: DustParameters,
    ) -> Result<Option<StoredDustCheckpoint>, DustCheckpointStoreError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| DustCheckpointStoreError::Unavailable)?;
        let public_key_fingerprint = public_key_fingerprint(public_key)?;
        let parameters_fingerprint = parameters_fingerprint(&parameters)?;
        Ok(self
            .load_document()?
            .into_iter()
            .find(|record| {
                record.network_id == network_id.as_str()
                    && record.public_key_fingerprint == public_key_fingerprint
                    && record.parameters_fingerprint == parameters_fingerprint
            })
            .map(|record| StoredDustCheckpoint {
                current_cursor: record.current_cursor,
                target_cursor: record.target_cursor,
                updated_at: UnixTimestampMillis::new(record.updated_at_millis),
                state: record.state,
            }))
    }

    fn save(
        &self,
        network_id: &ChainNetworkId,
        public_key: &DustPublicKey,
        checkpoint: &StoredDustCheckpoint,
    ) -> Result<(), DustCheckpointStoreError> {
        validate_checkpoint(checkpoint)?;
        let _guard = self
            .access
            .lock()
            .map_err(|_| DustCheckpointStoreError::Unavailable)?;
        let public_key_fingerprint = public_key_fingerprint(public_key)?;
        let replacement = DustCheckpointRecord {
            network_id: network_id.as_str().to_owned(),
            public_key_fingerprint,
            parameters_fingerprint: parameters_fingerprint(&checkpoint.state.params)?,
            current_cursor: checkpoint.current_cursor,
            target_cursor: checkpoint.target_cursor,
            updated_at_millis: checkpoint.updated_at.value(),
            state: checkpoint.state.clone(),
        };
        let mut records = match self.load_document() {
            Ok(records) => records,
            Err(DustCheckpointStoreError::InvalidData) => Vec::new(),
            Err(error) => return Err(error),
        };
        if let Some(record) = records.iter_mut().find(|record| {
            record.network_id == replacement.network_id
                && record.public_key_fingerprint == replacement.public_key_fingerprint
        }) {
            *record = replacement;
        } else {
            if records.len() >= MAX_RECORDS {
                return Err(DustCheckpointStoreError::InvalidData);
            }
            records.push(replacement);
        }
        records.sort_by(|left, right| {
            (&left.network_id, left.public_key_fingerprint)
                .cmp(&(&right.network_id, right.public_key_fingerprint))
        });
        self.save_document(&records)
    }
}

struct DustCheckpointRecord {
    network_id: String,
    public_key_fingerprint: [u8; 32],
    parameters_fingerprint: [u8; 32],
    current_cursor: u64,
    target_cursor: u64,
    updated_at_millis: u64,
    state: DustLocalState<DefaultDB>,
}

fn encode_document(records: &[DustCheckpointRecord]) -> Result<Vec<u8>, DustCheckpointStoreError> {
    validate_records(records)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
    bytes.extend_from_slice(
        &u32::try_from(records.len())
            .map_err(|_| DustCheckpointStoreError::InvalidData)?
            .to_be_bytes(),
    );
    for record in records {
        let network = record.network_id.as_bytes();
        bytes.extend_from_slice(
            &u16::try_from(network.len())
                .map_err(|_| DustCheckpointStoreError::InvalidData)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(network);
        bytes.extend_from_slice(&record.public_key_fingerprint);
        bytes.extend_from_slice(&record.parameters_fingerprint);
        bytes.extend_from_slice(&record.current_cursor.to_be_bytes());
        bytes.extend_from_slice(&record.target_cursor.to_be_bytes());
        bytes.extend_from_slice(&record.updated_at_millis.to_be_bytes());
        let mut state = Vec::new();
        midnight_serialize::tagged_serialize(&record.state, &mut state)
            .map_err(|_| DustCheckpointStoreError::InvalidData)?;
        if state.len() > MAX_STATE_BYTES {
            return Err(DustCheckpointStoreError::InvalidData);
        }
        bytes.extend_from_slice(
            &u32::try_from(state.len())
                .map_err(|_| DustCheckpointStoreError::InvalidData)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&state);
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(DustCheckpointStoreError::InvalidData);
        }
    }
    Ok(bytes)
}

fn decode_document(bytes: &[u8]) -> Result<Vec<DustCheckpointRecord>, DustCheckpointStoreError> {
    let mut reader = Cursor::new(bytes);
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|_| DustCheckpointStoreError::InvalidData)?;
    if &magic != MAGIC || read_u32(&mut reader)? != SCHEMA_VERSION {
        return Err(DustCheckpointStoreError::InvalidData);
    }
    let count = usize::try_from(read_u32(&mut reader)?)
        .map_err(|_| DustCheckpointStoreError::InvalidData)?;
    if count > MAX_RECORDS {
        return Err(DustCheckpointStoreError::InvalidData);
    }
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let network_len = usize::from(read_u16(&mut reader)?);
        if network_len == 0 || network_len > MAX_NETWORK_BYTES {
            return Err(DustCheckpointStoreError::InvalidData);
        }
        let mut network = vec![0_u8; network_len];
        reader
            .read_exact(&mut network)
            .map_err(|_| DustCheckpointStoreError::InvalidData)?;
        let network_id =
            String::from_utf8(network).map_err(|_| DustCheckpointStoreError::InvalidData)?;
        let public_key_fingerprint = read_array(&mut reader)?;
        let parameters_fingerprint = read_array(&mut reader)?;
        let current_cursor = read_u64(&mut reader)?;
        let target_cursor = read_u64(&mut reader)?;
        let updated_at_millis = read_u64(&mut reader)?;
        let state_len = usize::try_from(read_u32(&mut reader)?)
            .map_err(|_| DustCheckpointStoreError::InvalidData)?;
        if state_len == 0 || state_len > MAX_STATE_BYTES {
            return Err(DustCheckpointStoreError::InvalidData);
        }
        let mut state_bytes = vec![0_u8; state_len];
        reader
            .read_exact(&mut state_bytes)
            .map_err(|_| DustCheckpointStoreError::InvalidData)?;
        let state = midnight_serialize::tagged_deserialize(&state_bytes[..])
            .map_err(|_| DustCheckpointStoreError::InvalidData)?;
        records.push(DustCheckpointRecord {
            network_id,
            public_key_fingerprint,
            parameters_fingerprint,
            current_cursor,
            target_cursor,
            updated_at_millis,
            state,
        });
    }
    if reader.position()
        != u64::try_from(bytes.len()).map_err(|_| DustCheckpointStoreError::InvalidData)?
    {
        return Err(DustCheckpointStoreError::InvalidData);
    }
    validate_records(&records)?;
    Ok(records)
}

fn validate_records(records: &[DustCheckpointRecord]) -> Result<(), DustCheckpointStoreError> {
    if records.len() > MAX_RECORDS {
        return Err(DustCheckpointStoreError::InvalidData);
    }
    let mut scopes = BTreeSet::new();
    for record in records {
        let network_id = ChainNetworkId::parse(record.network_id.clone())
            .map_err(|_| DustCheckpointStoreError::InvalidData)?;
        if record.network_id.len() > MAX_NETWORK_BYTES
            || crate::network_by_id(&network_id)
                .map_err(|_| DustCheckpointStoreError::InvalidData)?
                .is_none()
            || record.current_cursor != record.target_cursor
            || !scopes.insert((&record.network_id, record.public_key_fingerprint))
            || parameters_fingerprint(&record.state.params)? != record.parameters_fingerprint
        {
            return Err(DustCheckpointStoreError::InvalidData);
        }
    }
    Ok(())
}

fn validate_checkpoint(checkpoint: &StoredDustCheckpoint) -> Result<(), DustCheckpointStoreError> {
    if checkpoint.current_cursor != checkpoint.target_cursor {
        return Err(DustCheckpointStoreError::InvalidData);
    }
    Ok(())
}

fn public_key_fingerprint(key: &DustPublicKey) -> Result<[u8; 32], DustCheckpointStoreError> {
    let mut bytes = Vec::new();
    midnight_serialize::tagged_serialize(key, &mut bytes)
        .map_err(|_| DustCheckpointStoreError::InvalidData)?;
    Ok(Sha256::digest(bytes).into())
}

fn parameters_fingerprint(
    parameters: &DustParameters,
) -> Result<[u8; 32], DustCheckpointStoreError> {
    let mut bytes = Vec::new();
    midnight_serialize::tagged_serialize(parameters, &mut bytes)
        .map_err(|_| DustCheckpointStoreError::InvalidData)?;
    Ok(Sha256::digest(bytes).into())
}

fn read_u16(reader: &mut Cursor<&[u8]>) -> Result<u16, DustCheckpointStoreError> {
    Ok(u16::from_be_bytes(read_array(reader)?))
}

fn read_u32(reader: &mut Cursor<&[u8]>) -> Result<u32, DustCheckpointStoreError> {
    Ok(u32::from_be_bytes(read_array(reader)?))
}

fn read_u64(reader: &mut Cursor<&[u8]>) -> Result<u64, DustCheckpointStoreError> {
    Ok(u64::from_be_bytes(read_array(reader)?))
}

fn read_array<const N: usize>(
    reader: &mut Cursor<&[u8]>,
) -> Result<[u8; N], DustCheckpointStoreError> {
    let mut bytes = [0_u8; N];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| DustCheckpointStoreError::InvalidData)?;
    Ok(bytes)
}

fn reject_symlink(path: &Path) -> Result<(), DustCheckpointStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(DustCheckpointStoreError::InvalidData)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(DustCheckpointStoreError::Unavailable),
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), DustCheckpointStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(DustCheckpointStoreError::InvalidData);
                }
            }
            Ok(())
        }
        Ok(_) => Err(DustCheckpointStoreError::InvalidData),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(path)
                    .map_err(|_| DustCheckpointStoreError::Unavailable)?;
                fs::set_permissions(path, {
                    use std::os::unix::fs::PermissionsExt as _;
                    fs::Permissions::from_mode(0o700)
                })
                .map_err(|_| DustCheckpointStoreError::Unavailable)?;
            }
            #[cfg(not(unix))]
            fs::create_dir_all(path).map_err(|_| DustCheckpointStoreError::Unavailable)?;
            Ok(())
        }
        Err(_) => Err(DustCheckpointStoreError::Unavailable),
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("midnight-dust-checkpoints.bin");
    path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DustCheckpointStoreError {
    Unavailable,
    InvalidData,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use midnight_ledger::{
        dust::{DustLocalState, DustPublicKey, DustSecretKey},
        structure::INITIAL_PARAMETERS,
    };

    use super::*;

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct IsolatedDirectory(PathBuf);

    impl IsolatedDirectory {
        fn new() -> Self {
            let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "oxid-dust-checkpoint-test-{}-{sequence}",
                std::process::id()
            ));
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&path)
                    .expect("private test directory is created");
            }
            #[cfg(not(unix))]
            fs::create_dir(&path).expect("test directory is created");
            Self(path)
        }

        fn config(&self) -> MidnightDustCheckpointConfig {
            MidnightDustCheckpointConfig::new(self.0.join("dust-checkpoints.bin"))
                .expect("absolute fixture path is valid")
        }
    }

    impl Drop for IsolatedDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn network(value: &str) -> ChainNetworkId {
        ChainNetworkId::parse(value).expect("known network identity is valid")
    }

    fn public_key(seed: u8) -> DustPublicKey {
        DustPublicKey::from(DustSecretKey::derive_secret_key(&[seed; 32]))
    }

    fn checkpoint(parameters: DustParameters) -> StoredDustCheckpoint {
        StoredDustCheckpoint {
            current_cursor: 42,
            target_cursor: 42,
            updated_at: UnixTimestampMillis::new(1_700_000_000_123),
            state: DustLocalState::new(parameters),
        }
    }

    fn tagged_state(state: &DustLocalState<DefaultDB>) -> Vec<u8> {
        let mut bytes = Vec::new();
        midnight_serialize::tagged_serialize(state, &mut bytes).expect("state serializes");
        bytes
    }

    #[test]
    fn configuration_requires_a_normalized_absolute_file_path() {
        assert_eq!(
            MidnightDustCheckpointConfig::new("relative.bin"),
            Err(MidnightDustCheckpointConfigError::InvalidPath)
        );
        assert_eq!(
            MidnightDustCheckpointConfig::new("/tmp/../dust.bin"),
            Err(MidnightDustCheckpointConfigError::InvalidPath)
        );
    }

    #[test]
    fn checkpoint_round_trip_preserves_exact_tagged_state_and_scope() {
        let directory = IsolatedDirectory::new();
        let config = directory.config();
        let store = BinaryMidnightDustCheckpointStore::new(config.clone());
        let key = public_key(7);
        let expected = checkpoint(INITIAL_PARAMETERS.dust);

        store
            .save(&network("devnet"), &key, &expected)
            .expect("checkpoint saves");
        let restored = store
            .load(&network("devnet"), &key, INITIAL_PARAMETERS.dust)
            .expect("checkpoint loads")
            .expect("matching checkpoint exists");

        assert_eq!(restored.current_cursor, 42);
        assert_eq!(restored.target_cursor, 42);
        assert_eq!(restored.updated_at, expected.updated_at);
        assert_eq!(tagged_state(&restored.state), tagged_state(&expected.state));
        assert!(
            store
                .load(&network("preprod"), &key, INITIAL_PARAMETERS.dust)
                .expect("wrong network remains a clean miss")
                .is_none()
        );
        assert!(
            store
                .load(&network("devnet"), &public_key(8), INITIAL_PARAMETERS.dust)
                .expect("wrong key remains a clean miss")
                .is_none()
        );
        let mut changed_parameters = INITIAL_PARAMETERS.dust;
        changed_parameters.night_dust_ratio += 1;
        assert!(
            store
                .load(&network("devnet"), &key, changed_parameters)
                .expect("changed parameters remain a clean miss")
                .is_none()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(config.path())
                    .expect("checkpoint metadata is readable")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn incomplete_malformed_and_oversized_checkpoints_are_rejected_or_replaced() {
        let directory = IsolatedDirectory::new();
        let config = directory.config();
        let store = BinaryMidnightDustCheckpointStore::new(config.clone());
        let mut incomplete = checkpoint(INITIAL_PARAMETERS.dust);
        incomplete.target_cursor = 43;
        assert_eq!(
            store
                .save(&network("devnet"), &public_key(7), &incomplete)
                .err(),
            Some(DustCheckpointStoreError::InvalidData)
        );

        fs::write(config.path(), b"not a DUST checkpoint").expect("malformed fixture writes");
        #[cfg(unix)]
        fs::set_permissions(config.path(), {
            use std::os::unix::fs::PermissionsExt as _;
            fs::Permissions::from_mode(0o600)
        })
        .expect("fixture permissions are private");
        assert_eq!(
            store
                .load(&network("devnet"), &public_key(7), INITIAL_PARAMETERS.dust)
                .err(),
            Some(DustCheckpointStoreError::InvalidData)
        );
        store
            .save(
                &network("devnet"),
                &public_key(7),
                &checkpoint(INITIAL_PARAMETERS.dust),
            )
            .expect("valid live state replaces malformed regular data");

        let file = fs::OpenOptions::new()
            .write(true)
            .open(config.path())
            .expect("checkpoint opens");
        file.set_len(MAX_FILE_BYTES + 1)
            .expect("oversized fixture is allocated sparsely");
        assert_eq!(
            store
                .load(&network("devnet"), &public_key(7), INITIAL_PARAMETERS.dust)
                .err(),
            Some(DustCheckpointStoreError::InvalidData)
        );
    }

    #[cfg(unix)]
    #[test]
    fn checkpoint_store_rejects_symlinks_and_non_private_access() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};

        let directory = IsolatedDirectory::new();
        let config = directory.config();
        let target = directory.0.join("target.bin");
        fs::write(&target, b"target").expect("target fixture writes");
        symlink(&target, config.path()).expect("symlink fixture is created");
        let store = BinaryMidnightDustCheckpointStore::new(config.clone());
        assert_eq!(
            store
                .load(&network("devnet"), &public_key(7), INITIAL_PARAMETERS.dust)
                .err(),
            Some(DustCheckpointStoreError::InvalidData)
        );
        fs::remove_file(config.path()).expect("fixture symlink is removed");
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o755))
            .expect("directory permissions change");
        assert_eq!(
            store
                .save(
                    &network("devnet"),
                    &public_key(7),
                    &checkpoint(INITIAL_PARAMETERS.dust),
                )
                .err(),
            Some(DustCheckpointStoreError::InvalidData)
        );
    }
}
