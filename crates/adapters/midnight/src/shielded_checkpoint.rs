// SPDX-License-Identifier: Apache-2.0

//! Owner-private, key/network-scoped persistence for official Zswap state.

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

use midnight_serialize::Serializable as _;
use midnight_storage::DefaultDB;
use midnight_zswap::{keys::SecretKeys, local::State as ZswapState};
use oxid_foundation::UnixTimestampMillis;
use oxid_wallet_domain::ChainNetworkId;
use sha2::{Digest as _, Sha256};

use crate::shielded::project_zswap_state;

const MAGIC: &[u8; 8] = b"OXIDZSWP";
const SCHEMA_VERSION: u32 = 1;
const MAX_RECORDS: usize = 4;
const MAX_NETWORK_BYTES: usize = 64;
const MAX_STATE_BYTES: usize = 32 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const CHECKSUM_BYTES: usize = 32;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Validated location for private Zswap replay checkpoints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightShieldedCheckpointConfig {
    path: PathBuf,
}

impl MidnightShieldedCheckpointConfig {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, MidnightShieldedCheckpointConfigError> {
        let path = path.into();
        if !path.is_absolute()
            || path.file_name().is_none()
            || path
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(MidnightShieldedCheckpointConfigError::InvalidPath);
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
pub enum MidnightShieldedCheckpointConfigError {
    InvalidPath,
}

impl std::fmt::Display for MidnightShieldedCheckpointConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .write_str("Midnight shielded checkpoint path must be a normalized absolute file path")
    }
}

impl std::error::Error for MidnightShieldedCheckpointConfigError {}

#[derive(Clone, Debug)]
pub(crate) struct StoredShieldedCheckpoint {
    pub(crate) current_cursor: u64,
    pub(crate) target_cursor: u64,
    pub(crate) updated_at: UnixTimestampMillis,
    pub(crate) state: ZswapState<DefaultDB>,
}

pub(crate) trait MidnightShieldedCheckpointStore: Send + Sync {
    fn load(
        &self,
        network_id: &ChainNetworkId,
        keys: &SecretKeys,
        source_fingerprint: &[u8; 32],
    ) -> Result<Option<StoredShieldedCheckpoint>, ShieldedCheckpointStoreError>;

    fn save(
        &self,
        network_id: &ChainNetworkId,
        keys: &SecretKeys,
        source_fingerprint: &[u8; 32],
        checkpoint: &StoredShieldedCheckpoint,
    ) -> Result<(), ShieldedCheckpointStoreError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UnavailableMidnightShieldedCheckpointStore;

impl MidnightShieldedCheckpointStore for UnavailableMidnightShieldedCheckpointStore {
    fn load(
        &self,
        _: &ChainNetworkId,
        _: &SecretKeys,
        _: &[u8; 32],
    ) -> Result<Option<StoredShieldedCheckpoint>, ShieldedCheckpointStoreError> {
        Ok(None)
    }

    fn save(
        &self,
        _: &ChainNetworkId,
        _: &SecretKeys,
        _: &[u8; 32],
        _: &StoredShieldedCheckpoint,
    ) -> Result<(), ShieldedCheckpointStoreError> {
        Ok(())
    }
}

pub(crate) struct BinaryMidnightShieldedCheckpointStore {
    path: PathBuf,
    access: Mutex<()>,
}

impl BinaryMidnightShieldedCheckpointStore {
    pub(crate) fn new(config: MidnightShieldedCheckpointConfig) -> Self {
        Self {
            path: config.path,
            access: Mutex::new(()),
        }
    }

    fn load_document(&self) -> Result<Vec<ShieldedCheckpointRecord>, ShieldedCheckpointStoreError> {
        reject_symlink(&self.path)?;
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(ShieldedCheckpointStoreError::Unavailable),
        };
        let metadata = file
            .metadata()
            .map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err(ShieldedCheckpointStoreError::InvalidData);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(ShieldedCheckpointStoreError::InvalidData);
            }
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(ShieldedCheckpointStoreError::InvalidData);
        }
        decode_document(&bytes)
    }

    fn save_document(
        &self,
        records: &[ShieldedCheckpointRecord],
    ) -> Result<(), ShieldedCheckpointStoreError> {
        let bytes = encode_document(records)?;
        reject_symlink(&self.path)?;
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(ShieldedCheckpointStoreError::InvalidData)?;
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
            .map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
        if file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(ShieldedCheckpointStoreError::Unavailable);
        }
        drop(file);
        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
        }
        if fs::rename(&temporary_path, &self.path).is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(ShieldedCheckpointStoreError::Unavailable);
        }
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
        Ok(())
    }
}

impl MidnightShieldedCheckpointStore for BinaryMidnightShieldedCheckpointStore {
    fn load(
        &self,
        network_id: &ChainNetworkId,
        keys: &SecretKeys,
        source_fingerprint: &[u8; 32],
    ) -> Result<Option<StoredShieldedCheckpoint>, ShieldedCheckpointStoreError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
        let key_fingerprint = key_fingerprint(keys)?;
        Ok(self
            .load_document()?
            .into_iter()
            .find(|record| {
                record.network_id == network_id.as_str()
                    && record.key_fingerprint == key_fingerprint
                    && record.source_fingerprint == *source_fingerprint
            })
            .map(|record| StoredShieldedCheckpoint {
                current_cursor: record.current_cursor,
                target_cursor: record.target_cursor,
                updated_at: UnixTimestampMillis::new(record.updated_at_millis),
                state: record.state,
            }))
    }

    fn save(
        &self,
        network_id: &ChainNetworkId,
        keys: &SecretKeys,
        source_fingerprint: &[u8; 32],
        checkpoint: &StoredShieldedCheckpoint,
    ) -> Result<(), ShieldedCheckpointStoreError> {
        validate_checkpoint(checkpoint)?;
        let _guard = self
            .access
            .lock()
            .map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
        let replacement = ShieldedCheckpointRecord {
            network_id: network_id.as_str().to_owned(),
            key_fingerprint: key_fingerprint(keys)?,
            source_fingerprint: *source_fingerprint,
            current_cursor: checkpoint.current_cursor,
            target_cursor: checkpoint.target_cursor,
            updated_at_millis: checkpoint.updated_at.value(),
            state: checkpoint.state.clone(),
        };
        let mut records = match self.load_document() {
            Ok(records) => records,
            Err(ShieldedCheckpointStoreError::InvalidData) => Vec::new(),
            Err(error) => return Err(error),
        };
        if let Some(record) = records.iter_mut().find(|record| {
            record.network_id == replacement.network_id
                && record.key_fingerprint == replacement.key_fingerprint
                && record.source_fingerprint == replacement.source_fingerprint
        }) {
            *record = replacement;
        } else {
            if records.len() >= MAX_RECORDS {
                return Err(ShieldedCheckpointStoreError::InvalidData);
            }
            records.push(replacement);
        }
        records.sort_by(|left, right| {
            (
                &left.network_id,
                left.key_fingerprint,
                left.source_fingerprint,
            )
                .cmp(&(
                    &right.network_id,
                    right.key_fingerprint,
                    right.source_fingerprint,
                ))
        });
        self.save_document(&records)
    }
}

struct ShieldedCheckpointRecord {
    network_id: String,
    key_fingerprint: [u8; 32],
    source_fingerprint: [u8; 32],
    current_cursor: u64,
    target_cursor: u64,
    updated_at_millis: u64,
    state: ZswapState<DefaultDB>,
}

fn encode_document(
    records: &[ShieldedCheckpointRecord],
) -> Result<Vec<u8>, ShieldedCheckpointStoreError> {
    validate_records(records)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
    bytes.extend_from_slice(
        &u32::try_from(records.len())
            .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?
            .to_be_bytes(),
    );
    for record in records {
        let network = record.network_id.as_bytes();
        bytes.extend_from_slice(
            &u16::try_from(network.len())
                .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(network);
        bytes.extend_from_slice(&record.key_fingerprint);
        bytes.extend_from_slice(&record.source_fingerprint);
        bytes.extend_from_slice(&record.current_cursor.to_be_bytes());
        bytes.extend_from_slice(&record.target_cursor.to_be_bytes());
        bytes.extend_from_slice(&record.updated_at_millis.to_be_bytes());
        let mut state = Vec::new();
        midnight_serialize::tagged_serialize(&record.state, &mut state)
            .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
        if state.is_empty() || state.len() > MAX_STATE_BYTES {
            return Err(ShieldedCheckpointStoreError::InvalidData);
        }
        bytes.extend_from_slice(
            &u32::try_from(state.len())
                .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&state);
        if bytes.len() as u64 > MAX_FILE_BYTES - CHECKSUM_BYTES as u64 {
            return Err(ShieldedCheckpointStoreError::InvalidData);
        }
    }
    let checksum: [u8; CHECKSUM_BYTES] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&checksum);
    Ok(bytes)
}

fn decode_document(
    bytes: &[u8],
) -> Result<Vec<ShieldedCheckpointRecord>, ShieldedCheckpointStoreError> {
    if bytes.len() < MAGIC.len() + 8 + CHECKSUM_BYTES {
        return Err(ShieldedCheckpointStoreError::InvalidData);
    }
    let payload_len = bytes
        .len()
        .checked_sub(CHECKSUM_BYTES)
        .ok_or(ShieldedCheckpointStoreError::InvalidData)?;
    let (payload, checksum) = bytes.split_at(payload_len);
    if Sha256::digest(payload).as_slice() != checksum {
        return Err(ShieldedCheckpointStoreError::InvalidData);
    }
    let mut reader = Cursor::new(payload);
    let mut magic = [0_u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
    if &magic != MAGIC || read_u32(&mut reader)? != SCHEMA_VERSION {
        return Err(ShieldedCheckpointStoreError::InvalidData);
    }
    let count = usize::try_from(read_u32(&mut reader)?)
        .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
    if count > MAX_RECORDS {
        return Err(ShieldedCheckpointStoreError::InvalidData);
    }
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let network_len = usize::from(read_u16(&mut reader)?);
        if network_len == 0 || network_len > MAX_NETWORK_BYTES {
            return Err(ShieldedCheckpointStoreError::InvalidData);
        }
        let mut network = vec![0_u8; network_len];
        reader
            .read_exact(&mut network)
            .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
        let network_id =
            String::from_utf8(network).map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
        let key_fingerprint = read_array(&mut reader)?;
        let source_fingerprint = read_array(&mut reader)?;
        let current_cursor = read_u64(&mut reader)?;
        let target_cursor = read_u64(&mut reader)?;
        let updated_at_millis = read_u64(&mut reader)?;
        let state_len = usize::try_from(read_u32(&mut reader)?)
            .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
        if state_len == 0 || state_len > MAX_STATE_BYTES {
            return Err(ShieldedCheckpointStoreError::InvalidData);
        }
        let mut state_bytes = vec![0_u8; state_len];
        reader
            .read_exact(&mut state_bytes)
            .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
        let state = midnight_serialize::tagged_deserialize(&state_bytes[..])
            .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
        records.push(ShieldedCheckpointRecord {
            network_id,
            key_fingerprint,
            source_fingerprint,
            current_cursor,
            target_cursor,
            updated_at_millis,
            state,
        });
    }
    if reader.position()
        != u64::try_from(payload.len()).map_err(|_| ShieldedCheckpointStoreError::InvalidData)?
    {
        return Err(ShieldedCheckpointStoreError::InvalidData);
    }
    validate_records(&records)?;
    Ok(records)
}

fn validate_records(
    records: &[ShieldedCheckpointRecord],
) -> Result<(), ShieldedCheckpointStoreError> {
    if records.len() > MAX_RECORDS {
        return Err(ShieldedCheckpointStoreError::InvalidData);
    }
    let mut scopes = BTreeSet::new();
    for record in records {
        let network_id = ChainNetworkId::parse(record.network_id.clone())
            .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
        if record.network_id.len() > MAX_NETWORK_BYTES
            || crate::network_by_id(&network_id)
                .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?
                .is_none()
            || record.current_cursor > record.target_cursor
            || !scopes.insert((
                &record.network_id,
                record.key_fingerprint,
                record.source_fingerprint,
            ))
            || project_zswap_state(&record.state).is_err()
        {
            return Err(ShieldedCheckpointStoreError::InvalidData);
        }
    }
    Ok(())
}

fn validate_checkpoint(
    checkpoint: &StoredShieldedCheckpoint,
) -> Result<(), ShieldedCheckpointStoreError> {
    if checkpoint.current_cursor > checkpoint.target_cursor
        || project_zswap_state(&checkpoint.state).is_err()
    {
        return Err(ShieldedCheckpointStoreError::InvalidData);
    }
    Ok(())
}

fn key_fingerprint(keys: &SecretKeys) -> Result<[u8; 32], ShieldedCheckpointStoreError> {
    let mut bytes = Vec::with_capacity(64);
    keys.coin_public_key()
        .serialize(&mut bytes)
        .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
    keys.enc_public_key()
        .serialize(&mut bytes)
        .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
    if bytes.len() != 64 {
        return Err(ShieldedCheckpointStoreError::InvalidData);
    }
    Ok(Sha256::digest(bytes).into())
}

fn read_u16(reader: &mut Cursor<&[u8]>) -> Result<u16, ShieldedCheckpointStoreError> {
    Ok(u16::from_be_bytes(read_array(reader)?))
}

fn read_u32(reader: &mut Cursor<&[u8]>) -> Result<u32, ShieldedCheckpointStoreError> {
    Ok(u32::from_be_bytes(read_array(reader)?))
}

fn read_u64(reader: &mut Cursor<&[u8]>) -> Result<u64, ShieldedCheckpointStoreError> {
    Ok(u64::from_be_bytes(read_array(reader)?))
}

fn read_array<const N: usize>(
    reader: &mut Cursor<&[u8]>,
) -> Result<[u8; N], ShieldedCheckpointStoreError> {
    let mut bytes = [0_u8; N];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| ShieldedCheckpointStoreError::InvalidData)?;
    Ok(bytes)
}

fn reject_symlink(path: &Path) -> Result<(), ShieldedCheckpointStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ShieldedCheckpointStoreError::InvalidData)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ShieldedCheckpointStoreError::Unavailable),
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), ShieldedCheckpointStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(ShieldedCheckpointStoreError::InvalidData);
                }
            }
            Ok(())
        }
        Ok(_) => Err(ShieldedCheckpointStoreError::InvalidData),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(path)
                    .map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
                fs::set_permissions(path, {
                    use std::os::unix::fs::PermissionsExt as _;
                    fs::Permissions::from_mode(0o700)
                })
                .map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
            }
            #[cfg(not(unix))]
            fs::create_dir_all(path).map_err(|_| ShieldedCheckpointStoreError::Unavailable)?;
            Ok(())
        }
        Err(_) => Err(ShieldedCheckpointStoreError::Unavailable),
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("midnight-shielded-checkpoints.bin");
    path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShieldedCheckpointStoreError {
    Unavailable,
    InvalidData,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use midnight_coin_structure::coin::{Info as CoinInfo, ShieldedTokenType};
    use midnight_zswap::keys::Seed;
    use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

    use super::*;

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct IsolatedDirectory(PathBuf);

    impl IsolatedDirectory {
        fn new() -> Self {
            let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "oxid-shielded-checkpoint-test-{}-{sequence}",
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

        fn config(&self) -> MidnightShieldedCheckpointConfig {
            MidnightShieldedCheckpointConfig::new(self.0.join("shielded-checkpoints.bin"))
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

    fn keys(seed: u8) -> SecretKeys {
        SecretKeys::from(Seed::from([seed; 32]))
    }

    const fn source(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    fn checkpoint(keys: &SecretKeys, target_cursor: u64) -> StoredShieldedCheckpoint {
        let mut rng = StdRng::seed_from_u64(23);
        let state = ZswapState::new()
            .insert_coin(
                keys,
                CoinInfo {
                    nonce: rng.r#gen(),
                    type_: ShieldedTokenType(rng.r#gen()),
                    value: 31,
                },
            )
            .expect("owned state inserts");
        StoredShieldedCheckpoint {
            current_cursor: 4,
            target_cursor,
            updated_at: UnixTimestampMillis::new(1_700_000_000_123),
            state,
        }
    }

    fn tagged_state(state: &ZswapState<DefaultDB>) -> Vec<u8> {
        let mut bytes = Vec::new();
        midnight_serialize::tagged_serialize(state, &mut bytes).expect("state serializes");
        bytes
    }

    #[test]
    fn configuration_requires_a_normalized_absolute_file_path() {
        assert_eq!(
            MidnightShieldedCheckpointConfig::new("relative.bin"),
            Err(MidnightShieldedCheckpointConfigError::InvalidPath)
        );
        assert_eq!(
            MidnightShieldedCheckpointConfig::new("/tmp/../shielded.bin"),
            Err(MidnightShieldedCheckpointConfigError::InvalidPath)
        );
    }

    #[test]
    fn checkpoint_round_trip_preserves_exact_state_and_scope() {
        let directory = IsolatedDirectory::new();
        let config = directory.config();
        let store = BinaryMidnightShieldedCheckpointStore::new(config.clone());
        let key = keys(7);
        let expected = checkpoint(&key, 4);

        store
            .save(&network("devnet"), &key, &source(1), &expected)
            .expect("checkpoint saves");
        let restored = store
            .load(&network("devnet"), &key, &source(1))
            .expect("checkpoint loads")
            .expect("matching checkpoint exists");
        assert_eq!(restored.current_cursor, 4);
        assert_eq!(restored.target_cursor, 4);
        assert_eq!(restored.updated_at, expected.updated_at);
        assert_eq!(tagged_state(&restored.state), tagged_state(&expected.state));
        assert!(
            store
                .load(&network("preprod"), &key, &source(1))
                .expect("wrong network is a clean miss")
                .is_none()
        );
        assert!(
            store
                .load(&network("devnet"), &keys(8), &source(1))
                .expect("wrong key is a clean miss")
                .is_none()
        );
        assert!(
            store
                .load(&network("devnet"), &key, &source(2))
                .expect("wrong source is a clean miss")
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
    fn partial_corrupt_and_oversized_checkpoints_are_resumable_or_replaceable() {
        let directory = IsolatedDirectory::new();
        let config = directory.config();
        let store = BinaryMidnightShieldedCheckpointStore::new(config.clone());
        let key = keys(7);
        store
            .save(&network("devnet"), &key, &source(1), &checkpoint(&key, 9))
            .expect("partial checkpoint saves");
        assert_eq!(
            store
                .load(&network("devnet"), &key, &source(1))
                .expect("partial checkpoint loads")
                .expect("partial checkpoint exists")
                .target_cursor,
            9
        );

        let mut bytes = fs::read(config.path()).expect("checkpoint reads");
        bytes[12] ^= 0x01;
        fs::write(config.path(), bytes).expect("corrupt fixture writes");
        assert_eq!(
            store.load(&network("devnet"), &key, &source(1)).err(),
            Some(ShieldedCheckpointStoreError::InvalidData)
        );
        store
            .save(&network("devnet"), &key, &source(1), &checkpoint(&key, 4))
            .expect("valid state replaces corrupt regular data");

        let file = fs::OpenOptions::new()
            .write(true)
            .open(config.path())
            .expect("checkpoint opens");
        file.set_len(MAX_FILE_BYTES + 1)
            .expect("oversized fixture is allocated sparsely");
        assert_eq!(
            store.load(&network("devnet"), &key, &source(1)).err(),
            Some(ShieldedCheckpointStoreError::InvalidData)
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
        let store = BinaryMidnightShieldedCheckpointStore::new(config.clone());
        assert_eq!(
            store.load(&network("devnet"), &keys(7), &source(1)).err(),
            Some(ShieldedCheckpointStoreError::InvalidData)
        );
        fs::remove_file(config.path()).expect("fixture symlink is removed");
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o755))
            .expect("directory permissions change");
        let key = keys(7);
        assert_eq!(
            store
                .save(&network("devnet"), &key, &source(1), &checkpoint(&key, 4),)
                .err(),
            Some(ShieldedCheckpointStoreError::InvalidData)
        );
    }
}
