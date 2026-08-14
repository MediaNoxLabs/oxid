// SPDX-License-Identifier: Apache-2.0

//! Bounded owner-private persistence for the standalone conformance ledger.

use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use oxid_passport_vault_application::{PassportVaultRepository, PassportVaultRepositoryError};
use oxid_passport_vault_domain::{
    CredentialFingerprint, MAX_VAULT_CONSUMED_CLAIMS, MAX_VAULT_LOCKS,
    PassportVaultConsumedClaimSnapshot, PassportVaultLockSnapshot, PassportVaultPolicy,
    PassportVaultState, PassportVaultStateSnapshot, VaultActorId, VaultLockId,
};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
const MAX_STORE_BYTES: u64 = 8 * 1_024 * 1_024;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Validated location for the standalone Passport Vault state file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultStoreConfig {
    path: PathBuf,
}

impl PassportVaultStoreConfig {
    /// Accepts only a normalized absolute file path.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, PassportVaultStoreConfigError> {
        let path = path.into();
        if !path.is_absolute()
            || path.file_name().is_none()
            || path
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(PassportVaultStoreConfigError::InvalidPath);
        }
        Ok(Self { path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Safe configuration failure that never renders the supplied path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultStoreConfigError {
    InvalidPath,
}

impl std::fmt::Display for PassportVaultStoreConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Passport Vault store path must be a normalized absolute file path")
    }
}

impl std::error::Error for PassportVaultStoreConfigError {}

/// JSON repository for the standalone product ledger.
///
/// This file is not authenticated chain state and must never be used by the
/// native contract-call source. It retains only the local conformance ledger,
/// including one-way credential fingerprints used for replay prevention.
pub struct JsonPassportVaultRepository {
    path: PathBuf,
    access: Mutex<()>,
}

impl JsonPassportVaultRepository {
    #[must_use]
    pub fn new(config: PassportVaultStoreConfig) -> Self {
        Self {
            path: config.path,
            access: Mutex::new(()),
        }
    }

    #[must_use]
    pub fn configured_path(&self) -> &Path {
        &self.path
    }

    fn load_document(&self) -> Result<StoreDocument, PassportVaultRepositoryError> {
        reject_symlink(&self.path)?;
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                validate_parent_if_present(&self.path)?;
                return Ok(StoreDocument::default());
            }
            Err(_) => return Err(PassportVaultRepositoryError::Unavailable),
        };
        let metadata = file
            .metadata()
            .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        if !metadata.is_file() || metadata.len() > MAX_STORE_BYTES {
            return Err(PassportVaultRepositoryError::Integrity);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(PassportVaultRepositoryError::Integrity);
            }
        }
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(PassportVaultRepositoryError::Integrity)?;
        validate_private_directory(parent)?;

        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_STORE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(PassportVaultRepositoryError::Integrity);
        }
        serde_json::from_slice(&bytes).map_err(|_| PassportVaultRepositoryError::Integrity)
    }

    fn save_document(&self, document: &StoreDocument) -> Result<(), PassportVaultRepositoryError> {
        let bytes = serde_json::to_vec_pretty(document)
            .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(PassportVaultRepositoryError::Integrity);
        }
        reject_symlink(&self.path)?;
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or(PassportVaultRepositoryError::Integrity)?;
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
            .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        if file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(PassportVaultRepositoryError::Unavailable);
        }
        drop(file);

        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        }
        if fs::rename(&temporary_path, &self.path).is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(PassportVaultRepositoryError::Unavailable);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
                .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        }
        Ok(())
    }
}

impl PassportVaultRepository for JsonPassportVaultRepository {
    fn load(&self) -> Result<PassportVaultState, PassportVaultRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        state_from_document(self.load_document()?)
    }

    fn save(&self, state: &PassportVaultState) -> Result<(), PassportVaultRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        let document = StoreDocument::from(state);
        // Validate the encoded projection before it can replace the last good
        // snapshot, even though it originated from an already-valid state.
        state_from_document(document.clone())?;
        self.save_document(&document)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoreDocument {
    schema_version: u32,
    next_lock_id: String,
    locks: Vec<StoredLock>,
    consumed_claims: Vec<StoredConsumedClaim>,
    total_deposited: String,
    total_released: String,
    claim_count: String,
}

impl Default for StoreDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            next_lock_id: "0".to_owned(),
            locks: Vec::new(),
            consumed_claims: Vec::new(),
            total_deposited: "0".to_owned(),
            total_released: "0".to_owned(),
            claim_count: "0".to_owned(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredLock {
    lock_id: String,
    creator_profile_id: String,
    minimum_age_years: u8,
    required_issuing_state_hex: Option<String>,
    required_document_number_hex: Option<String>,
    maximum_claim_amount: String,
    verifier_challenge_hex: String,
    total_deposited: String,
    total_released: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredConsumedClaim {
    lock_id: String,
    credential_fingerprint_hex: String,
}

impl From<&PassportVaultState> for StoreDocument {
    fn from(state: &PassportVaultState) -> Self {
        let snapshot = state.snapshot();
        Self {
            schema_version: SCHEMA_VERSION,
            next_lock_id: snapshot.next_lock_id().to_string(),
            locks: snapshot.locks().iter().map(StoredLock::from).collect(),
            consumed_claims: snapshot
                .consumed_claims()
                .iter()
                .copied()
                .map(StoredConsumedClaim::from)
                .collect(),
            total_deposited: snapshot.total_deposited().to_string(),
            total_released: snapshot.total_released().to_string(),
            claim_count: snapshot.claim_count().to_string(),
        }
    }
}

impl From<&PassportVaultLockSnapshot> for StoredLock {
    fn from(lock: &PassportVaultLockSnapshot) -> Self {
        Self {
            lock_id: lock.id().value().to_string(),
            creator_profile_id: lock.creator().as_str().to_owned(),
            minimum_age_years: lock.policy().minimum_age_years(),
            required_issuing_state_hex: lock.policy().required_issuing_state().map(hex::encode),
            required_document_number_hex: lock.policy().required_document_number().map(hex::encode),
            maximum_claim_amount: lock.policy().maximum_claim_amount().to_string(),
            verifier_challenge_hex: hex::encode(lock.policy().verifier_challenge_hash()),
            total_deposited: lock.total_deposited().to_string(),
            total_released: lock.total_released().to_string(),
        }
    }
}

impl From<PassportVaultConsumedClaimSnapshot> for StoredConsumedClaim {
    fn from(claim: PassportVaultConsumedClaimSnapshot) -> Self {
        Self {
            lock_id: claim.lock_id().value().to_string(),
            credential_fingerprint_hex: hex::encode(claim.credential().bytes()),
        }
    }
}

fn state_from_document(
    document: StoreDocument,
) -> Result<PassportVaultState, PassportVaultRepositoryError> {
    if document.schema_version != SCHEMA_VERSION
        || document.locks.len() > MAX_VAULT_LOCKS
        || document.consumed_claims.len() > MAX_VAULT_CONSUMED_CLAIMS
    {
        return Err(PassportVaultRepositoryError::Integrity);
    }
    let locks = document
        .locks
        .into_iter()
        .map(lock_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    let consumed_claims = document
        .consumed_claims
        .into_iter()
        .map(claim_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    let snapshot = PassportVaultStateSnapshot::new(
        parse_u64(&document.next_lock_id)?,
        locks,
        consumed_claims,
        parse_u128(&document.total_deposited)?,
        parse_u128(&document.total_released)?,
        parse_u64(&document.claim_count)?,
    );
    PassportVaultState::restore(snapshot).map_err(|_| PassportVaultRepositoryError::Integrity)
}

fn lock_from_record(
    record: StoredLock,
) -> Result<PassportVaultLockSnapshot, PassportVaultRepositoryError> {
    let policy = PassportVaultPolicy::new(
        record.minimum_age_years,
        record
            .required_issuing_state_hex
            .as_deref()
            .map(parse_hex_32)
            .transpose()?,
        record
            .required_document_number_hex
            .as_deref()
            .map(parse_hex_32)
            .transpose()?,
        parse_u128(&record.maximum_claim_amount)?,
        parse_hex_32(&record.verifier_challenge_hex)?,
    )
    .map_err(|_| PassportVaultRepositoryError::Integrity)?;
    Ok(PassportVaultLockSnapshot::new(
        VaultLockId::new(parse_u64(&record.lock_id)?),
        VaultActorId::parse(record.creator_profile_id)
            .map_err(|_| PassportVaultRepositoryError::Integrity)?,
        policy,
        parse_u128(&record.total_deposited)?,
        parse_u128(&record.total_released)?,
    ))
}

fn claim_from_record(
    record: StoredConsumedClaim,
) -> Result<PassportVaultConsumedClaimSnapshot, PassportVaultRepositoryError> {
    Ok(PassportVaultConsumedClaimSnapshot::new(
        VaultLockId::new(parse_u64(&record.lock_id)?),
        CredentialFingerprint::new(parse_hex_32(&record.credential_fingerprint_hex)?)
            .map_err(|_| PassportVaultRepositoryError::Integrity)?,
    ))
}

fn parse_u64(value: &str) -> Result<u64, PassportVaultRepositoryError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(PassportVaultRepositoryError::Integrity);
    }
    value
        .parse()
        .map_err(|_| PassportVaultRepositoryError::Integrity)
}

fn parse_u128(value: &str) -> Result<u128, PassportVaultRepositoryError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(PassportVaultRepositoryError::Integrity);
    }
    value
        .parse()
        .map_err(|_| PassportVaultRepositoryError::Integrity)
}

fn parse_hex_32(value: &str) -> Result<[u8; 32], PassportVaultRepositoryError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(PassportVaultRepositoryError::Integrity);
    }
    let mut bytes = [0_u8; 32];
    hex::decode_to_slice(value, &mut bytes).map_err(|_| PassportVaultRepositoryError::Integrity)?;
    Ok(bytes)
}

fn reject_symlink(path: &Path) -> Result<(), PassportVaultRepositoryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(PassportVaultRepositoryError::Integrity)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(PassportVaultRepositoryError::Unavailable),
    }
}

fn validate_parent_if_present(path: &Path) -> Result<(), PassportVaultRepositoryError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or(PassportVaultRepositoryError::Integrity)?;
    match parent.try_exists() {
        Ok(true) => validate_private_directory(parent),
        Ok(false) => Ok(()),
        Err(_) => Err(PassportVaultRepositoryError::Unavailable),
    }
}

fn validate_private_directory(path: &Path) -> Result<(), PassportVaultRepositoryError> {
    reject_symlink(path)?;
    let metadata = fs::metadata(path).map_err(|_| PassportVaultRepositoryError::Unavailable)?;
    if !metadata.is_dir() {
        return Err(PassportVaultRepositoryError::Integrity);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(PassportVaultRepositoryError::Integrity);
        }
    }
    Ok(())
}

fn ensure_private_directory(path: &Path) -> Result<(), PassportVaultRepositoryError> {
    let existed = path
        .try_exists()
        .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
    if !existed {
        fs::create_dir_all(path).map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .map_err(|_| PassportVaultRepositoryError::Unavailable)?;
        }
    }
    validate_private_directory(path)
}

fn temporary_path(path: &Path) -> PathBuf {
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("passport-vault.json");
    path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestStore {
        root: PathBuf,
        path: PathBuf,
    }

    impl TestStore {
        fn new() -> Self {
            let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "oxid-passport-vault-store-{}-{sequence}",
                std::process::id()
            ));
            let path = root.join("private/passport-vault.json");
            Self { root, path }
        }

        fn repository(&self) -> JsonPassportVaultRepository {
            JsonPassportVaultRepository::new(
                PassportVaultStoreConfig::new(&self.path).expect("config"),
            )
        }
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn populated_state() -> PassportVaultState {
        let mut state = PassportVaultState::default();
        let creator = VaultActorId::parse("profile-holder").expect("actor");
        let lock_id = state
            .create_lock(
                creator.clone(),
                PassportVaultPolicy::new(18, Some([1; 32]), None, 40, [7; 32]).expect("policy"),
                100,
            )
            .expect("create");
        state.deposit(&creator, lock_id, 20).expect("deposit");
        state
            .claim(
                lock_id,
                CredentialFingerprint::new([9; 32]).expect("fingerprint"),
                40,
                20_000,
            )
            .expect("claim");
        state.withdraw(&creator, lock_id, 10).expect("withdraw");
        state
    }

    #[test]
    fn round_trips_the_complete_bounded_state() {
        let store = TestStore::new();
        let repository = store.repository();
        let state = populated_state();
        repository.save(&state).expect("save");

        let restored = repository.load().expect("load");
        assert_eq!(restored.snapshot(), state.snapshot());
        assert_eq!(restored.total_locked(), 70);
        assert_eq!(restored.claim_count(), 1);
        let serialized = fs::read_to_string(&store.path).expect("stored document");
        assert!(!serialized.contains("privateMaterial"));
        assert!(!serialized.contains("credentialId"));
    }

    #[test]
    fn rejects_tampered_accounting_and_unknown_fields() {
        let store = TestStore::new();
        let repository = store.repository();
        repository.save(&populated_state()).expect("save");
        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(&store.path).expect("document")).expect("json");
        document["totalDeposited"] = serde_json::Value::String("999".to_owned());
        fs::write(
            &store.path,
            serde_json::to_vec_pretty(&document).expect("encode"),
        )
        .expect("tamper");
        assert_eq!(
            repository.load(),
            Err(PassportVaultRepositoryError::Integrity)
        );

        document["unexpected"] = serde_json::Value::Bool(true);
        fs::write(
            &store.path,
            serde_json::to_vec_pretty(&document).expect("encode"),
        )
        .expect("tamper");
        assert_eq!(
            repository.load(),
            Err(PassportVaultRepositoryError::Integrity)
        );

        document
            .as_object_mut()
            .expect("document object")
            .remove("unexpected");
        document["totalDeposited"] = serde_json::Value::String("120".to_owned());
        document["locks"][0]["verifierChallengeHex"] = serde_json::Value::String("AB".repeat(32));
        fs::write(
            &store.path,
            serde_json::to_vec_pretty(&document).expect("encode"),
        )
        .expect("tamper");
        assert_eq!(
            repository.load(),
            Err(PassportVaultRepositoryError::Integrity)
        );
    }

    #[cfg(unix)]
    #[test]
    fn creates_owner_private_directory_and_file_and_rejects_symlinks() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};

        let store = TestStore::new();
        let repository = store.repository();
        repository.save(&populated_state()).expect("save");
        assert_eq!(
            fs::metadata(store.path.parent().expect("parent"))
                .expect("parent metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&store.path)
                .expect("file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        fs::remove_file(&store.path).expect("remove file");
        let target = store.root.join("target.json");
        fs::write(&target, b"{}").expect("target");
        symlink(&target, &store.path).expect("symlink");
        assert_eq!(
            repository.load(),
            Err(PassportVaultRepositoryError::Integrity)
        );
        assert_eq!(
            repository.save(&populated_state()),
            Err(PassportVaultRepositoryError::Integrity)
        );

        fs::remove_file(&store.path).expect("remove symlink");
        fs::set_permissions(
            store.path.parent().expect("parent"),
            fs::Permissions::from_mode(0o755),
        )
        .expect("make parent permissive");
        assert_eq!(
            repository.load(),
            Err(PassportVaultRepositoryError::Integrity)
        );
    }

    #[test]
    fn configuration_rejects_relative_and_parent_components_without_echoing_them() {
        for path in ["passport-vault.json", "/tmp/../passport-vault.json"] {
            let error = PassportVaultStoreConfig::new(path).expect_err("invalid path");
            assert_eq!(error, PassportVaultStoreConfigError::InvalidPath);
            assert!(!error.to_string().contains(path));
        }
    }
}
