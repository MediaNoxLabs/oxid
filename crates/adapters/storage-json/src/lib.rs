// SPDX-License-Identifier: Apache-2.0

#![deny(unsafe_code)]

use std::{
    collections::BTreeSet,
    env, fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(not(target_os = "android"))]
use directories::ProjectDirs;
use oxid_foundation::UnixTimestampMillis;
use oxid_wallet_application::{WalletProfileRepository, WalletProfileRepositoryError};
use oxid_wallet_domain::{ProfileName, WalletProfile, WalletProfileId};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
const MAX_PROFILE_COUNT: usize = 128;
const MAX_STORE_BYTES: u64 = 1024 * 1024;
const STORE_FILE_NAME: &str = "wallet-profiles.json";
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// JSON persistence for public profile labels and active selection.
///
/// This adapter must never be extended to contain seeds, private keys,
/// credentials, or recovery material. Those require protected platform ports.
pub struct JsonWalletProfileRepository {
    path: Option<PathBuf>,
    access: Mutex<()>,
}

impl JsonWalletProfileRepository {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
            access: Mutex::new(()),
        }
    }

    /// Uses an explicit override or a platform-conventional durable
    /// application data directory. Failure to resolve one is retained as an
    /// unavailable repository rather than silently selecting temporary storage.
    #[must_use]
    pub fn at_default_location() -> Self {
        let path = env::var_os("OXID_PROFILE_STORE_PATH")
            .map(PathBuf::from)
            .or_else(default_store_path);

        Self {
            path,
            access: Mutex::new(()),
        }
    }

    /// Returns the resolved public-profile store path so the composition root
    /// can colocate other independently bounded public metadata stores.
    #[must_use]
    pub fn configured_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn path(&self) -> Result<&Path, WalletProfileRepositoryError> {
        self.path
            .as_deref()
            .ok_or(WalletProfileRepositoryError::Unavailable)
    }

    fn load_document(&self) -> Result<StoreDocument, WalletProfileRepositoryError> {
        let path = self.path()?;
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StoreDocument::default());
            }
            Err(_) => return Err(WalletProfileRepositoryError::Unavailable),
        };
        let metadata = file
            .metadata()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        if metadata.len() > MAX_STORE_BYTES {
            return Err(WalletProfileRepositoryError::Unavailable);
        }

        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_STORE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(WalletProfileRepositoryError::Unavailable);
        }
        let document: StoreDocument = serde_json::from_slice(&bytes)
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        validate_document(&document)?;

        Ok(document)
    }

    fn save_document(&self, document: &StoreDocument) -> Result<(), WalletProfileRepositoryError> {
        validate_document(document)?;
        let path = self.path()?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        }

        let bytes = serde_json::to_vec_pretty(document)
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(WalletProfileRepositoryError::Unavailable);
        }

        let temporary_path = temporary_path(path);
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary_path)
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        if file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(WalletProfileRepositoryError::Unavailable);
        }
        drop(file);

        #[cfg(windows)]
        if path.exists() {
            fs::remove_file(path).map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        }
        if fs::rename(&temporary_path, path).is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(WalletProfileRepositoryError::Unavailable);
        }

        Ok(())
    }
}

#[cfg(not(target_os = "android"))]
fn default_store_path() -> Option<PathBuf> {
    ProjectDirs::from("io", "medianox", "oxid")
        .map(|directories| directories.data_local_dir().join(STORE_FILE_NAME))
}

#[cfg(target_os = "android")]
#[allow(unsafe_code)]
fn default_store_path() -> Option<PathBuf> {
    use jni::{
        JavaVM,
        objects::{JObject, JString},
    };

    let android_context = std::panic::catch_unwind(ndk_context::android_context).ok()?;

    // SAFETY: `ndk-context` is initialized by Dioxus/Tao before application
    // `main` and guarantees that `vm()` is the process JavaVM pointer.
    let java_vm = unsafe { JavaVM::from_raw(android_context.vm().cast()) }.ok()?;
    let mut environment = java_vm.attach_current_thread().ok()?;

    // SAFETY: `ndk-context` guarantees that `context()` remains a valid Android
    // Context reference while the activity is alive. This wrapper is scoped to
    // the attached JNI frame and does not delete or retain the reference.
    let context = unsafe { JObject::from_raw(android_context.context().cast()) };
    let files_directory = environment
        .call_method(&context, "getFilesDir", "()Ljava/io/File;", &[])
        .ok()?
        .l()
        .ok()?;
    if files_directory.is_null() {
        return None;
    }
    let absolute_path = environment
        .call_method(
            files_directory,
            "getAbsolutePath",
            "()Ljava/lang/String;",
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    if absolute_path.is_null() {
        return None;
    }
    let absolute_path = JString::from(absolute_path);
    let absolute_path: String = environment.get_string(&absolute_path).ok()?.into();

    Some(
        PathBuf::from(absolute_path)
            .join("oxid")
            .join(STORE_FILE_NAME),
    )
}

impl WalletProfileRepository for JsonWalletProfileRepository {
    fn save(&self, profile: WalletProfile) -> Result<(), WalletProfileRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        let mut document = self.load_document()?;
        if document
            .profiles
            .iter()
            .any(|stored| stored.id == profile.id().as_str())
        {
            return Err(WalletProfileRepositoryError::Conflict);
        }
        if document.profiles.len() >= MAX_PROFILE_COUNT {
            return Err(WalletProfileRepositoryError::Unavailable);
        }

        document.profiles.push(ProfileRecord::from(&profile));
        self.save_document(&document)
    }

    fn list(&self) -> Result<Vec<WalletProfile>, WalletProfileRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        profiles_from_document(&self.load_document()?)
    }

    fn set_active(
        &self,
        id: &WalletProfileId,
    ) -> Result<WalletProfile, WalletProfileRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        let mut document = self.load_document()?;
        let profile = profiles_from_document(&document)?
            .into_iter()
            .find(|profile| profile.id() == id)
            .ok_or(WalletProfileRepositoryError::NotFound)?;

        document.active_profile_id = Some(id.as_str().to_owned());
        self.save_document(&document)?;
        Ok(profile)
    }

    fn active(&self) -> Result<Option<WalletProfile>, WalletProfileRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
        let document = self.load_document()?;
        let Some(active_profile_id) = document.active_profile_id.as_deref() else {
            return Ok(None);
        };

        profiles_from_document(&document)?
            .into_iter()
            .find(|profile| profile.id().as_str() == active_profile_id)
            .map(Some)
            .ok_or(WalletProfileRepositoryError::Unavailable)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoreDocument {
    schema_version: u32,
    profiles: Vec<ProfileRecord>,
    active_profile_id: Option<String>,
}

impl Default for StoreDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            profiles: Vec::new(),
            active_profile_id: None,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileRecord {
    id: String,
    display_name: String,
    created_at_millis: u64,
}

impl From<&WalletProfile> for ProfileRecord {
    fn from(profile: &WalletProfile) -> Self {
        Self {
            id: profile.id().as_str().to_owned(),
            display_name: profile.display_name().as_str().to_owned(),
            created_at_millis: profile.created_at().value(),
        }
    }
}

fn validate_document(document: &StoreDocument) -> Result<(), WalletProfileRepositoryError> {
    if document.schema_version != SCHEMA_VERSION || document.profiles.len() > MAX_PROFILE_COUNT {
        return Err(WalletProfileRepositoryError::Unavailable);
    }

    let profiles = profiles_from_document(document)?;
    let unique_ids: BTreeSet<_> = profiles.iter().map(WalletProfile::id).collect();
    if unique_ids.len() != profiles.len() {
        return Err(WalletProfileRepositoryError::Unavailable);
    }
    if document
        .active_profile_id
        .as_ref()
        .is_some_and(|active_id| {
            !profiles
                .iter()
                .any(|profile| profile.id().as_str() == active_id)
        })
    {
        return Err(WalletProfileRepositoryError::Unavailable);
    }

    Ok(())
}

fn profiles_from_document(
    document: &StoreDocument,
) -> Result<Vec<WalletProfile>, WalletProfileRepositoryError> {
    document
        .profiles
        .iter()
        .map(|record| {
            let id = WalletProfileId::parse(record.id.clone())
                .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
            let display_name = ProfileName::parse(&record.display_name)
                .map_err(|_| WalletProfileRepositoryError::Unavailable)?;
            Ok(WalletProfile::new(
                id,
                display_name,
                UnixTimestampMillis::new(record.created_at_millis),
            ))
        })
        .collect()
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(STORE_FILE_NAME);
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(
        ".{file_name}.tmp-{}-{sequence}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestStore {
        root: PathBuf,
        path: PathBuf,
    }

    impl TestStore {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = env::temp_dir().join(format!(
                "oxid-storage-json-test-{}-{sequence}",
                std::process::id()
            ));
            Self {
                path: root.join(STORE_FILE_NAME),
                root,
            }
        }
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn profile(id: &str, name: &str, created_at_millis: u64) -> WalletProfile {
        WalletProfile::new(
            WalletProfileId::parse(id).expect("identifier should be valid"),
            ProfileName::parse(name).expect("name should be valid"),
            UnixTimestampMillis::new(created_at_millis),
        )
    }

    #[test]
    fn reopens_profiles_and_active_selection() {
        let store = TestStore::new();
        let profile = profile("profile_primary", "Primary", 42);
        let profile_id = profile.id().clone();
        let repository = JsonWalletProfileRepository::new(&store.path);
        repository
            .save(profile.clone())
            .expect("save should succeed");
        repository
            .set_active(&profile_id)
            .expect("selection should succeed");

        let reopened = JsonWalletProfileRepository::new(&store.path);
        assert_eq!(
            reopened.list().expect("list should load"),
            vec![profile.clone()]
        );
        assert_eq!(
            reopened.active().expect("active should load"),
            Some(profile)
        );
    }

    #[test]
    fn rejects_duplicate_profile_identifiers() {
        let store = TestStore::new();
        let repository = JsonWalletProfileRepository::new(&store.path);
        repository
            .save(profile("profile_primary", "Primary", 42))
            .expect("first save should succeed");

        assert_eq!(
            repository.save(profile("profile_primary", "Other", 43)),
            Err(WalletProfileRepositoryError::Conflict)
        );
    }

    #[test]
    fn file_contains_only_versioned_public_profile_metadata() {
        let store = TestStore::new();
        let repository = JsonWalletProfileRepository::new(&store.path);
        let profile = profile("profile_public", "Public label", 42);
        repository.save(profile).expect("save should succeed");

        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&store.path).expect("store should be readable"))
                .expect("store should contain JSON");
        assert_eq!(value["schemaVersion"], SCHEMA_VERSION);
        assert_eq!(value["profiles"][0]["displayName"], "Public label");
        let serialized = value.to_string();
        for forbidden in ["seed", "privateKey", "credential", "recovery"] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn rejects_corrupt_or_unknown_store_schemas() {
        let store = TestStore::new();
        fs::create_dir_all(&store.root).expect("test directory should be created");
        fs::write(
            &store.path,
            br#"{"schemaVersion":2,"profiles":[],"activeProfileId":null}"#,
        )
        .expect("fixture should be written");
        let repository = JsonWalletProfileRepository::new(&store.path);

        assert_eq!(
            repository.list(),
            Err(WalletProfileRepositoryError::Unavailable)
        );
    }

    #[test]
    fn rejects_unknown_fields_and_dangling_active_selection() {
        let store = TestStore::new();
        fs::create_dir_all(&store.root).expect("test directory should be created");
        fs::write(
            &store.path,
            br#"{"schemaVersion":1,"profiles":[],"activeProfileId":null,"secret":"no"}"#,
        )
        .expect("fixture should be written");
        let repository = JsonWalletProfileRepository::new(&store.path);
        assert_eq!(
            repository.list(),
            Err(WalletProfileRepositoryError::Unavailable)
        );

        fs::write(
            &store.path,
            br#"{"schemaVersion":1,"profiles":[],"activeProfileId":"profile_missing"}"#,
        )
        .expect("fixture should be written");
        assert_eq!(
            repository.active(),
            Err(WalletProfileRepositoryError::Unavailable)
        );
    }

    #[cfg(unix)]
    #[test]
    fn creates_owner_only_store_files() {
        use std::os::unix::fs::PermissionsExt as _;

        let store = TestStore::new();
        let repository = JsonWalletProfileRepository::new(&store.path);
        repository
            .save(profile("profile_private_file", "Public metadata", 42))
            .expect("save should succeed");

        let mode = fs::metadata(&store.path)
            .expect("store metadata should be readable")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
