// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use base64::{Engine as _, engine::general_purpose};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, Key, KeyInit as _, Payload},
};
use oxid_credential_application::{CredentialRepository, CredentialRepositoryError};
use oxid_credential_domain::{
    CredentialDetachedProof, CredentialFormat, CredentialId, CredentialMetadata,
    CredentialPrivateMaterial, CredentialProfileId, CredentialRecord, VerificationOutcome,
    VerificationReport, VerificationStage, VerificationStageName, VerificationStageStatus,
};
use oxid_foundation::UnixTimestampMillis;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"OXIDVC01";
const AAD: &[u8] = b"oxid.credentials.v1";
const NONCE_BYTES: usize = 24;
const KEY_BYTES: usize = 32;
const SCHEMA_VERSION: u32 = 3;
const MAX_RECORDS: usize = 64;
const MAX_DOCUMENT_BYTES: u64 = 67_174_400;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Whole-document XChaCha20-Poly1305 persistence for standalone development.
/// The separate owner-private key file is a temporary native-harness boundary,
/// not a production custody claim.
pub struct EncryptedJsonCredentialRepository {
    path: PathBuf,
    key_path: PathBuf,
    access: Mutex<()>,
}

impl EncryptedJsonCredentialRepository {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, key_path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            key_path: key_path.into(),
            access: Mutex::new(()),
        }
    }

    #[must_use]
    pub fn configured_path(&self) -> &Path {
        &self.path
    }

    fn read_records(&self) -> Result<Vec<CredentialRecord>, CredentialRepositoryError> {
        ensure_regular_or_absent(&self.path)?;
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        ensure_private_parent(&self.path)?;
        let metadata =
            fs::metadata(&self.path).map_err(|_| CredentialRepositoryError::Unavailable)?;
        if metadata.len() > MAX_DOCUMENT_BYTES {
            return Err(CredentialRepositoryError::Integrity);
        }
        let mut envelope = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
        File::open(&self.path)
            .and_then(|mut file| file.read_to_end(&mut envelope))
            .map_err(|_| CredentialRepositoryError::Unavailable)?;
        let plaintext = self.decrypt(&envelope)?;
        let document: StoreDocument =
            serde_json::from_slice(&plaintext).map_err(|_| CredentialRepositoryError::Integrity)?;
        document.to_domain()
    }

    fn write_records(&self, records: &[CredentialRecord]) -> Result<(), CredentialRepositoryError> {
        if records.len() > MAX_RECORDS {
            return Err(CredentialRepositoryError::CapacityExceeded);
        }
        let document = StoreDocument::from_domain(records);
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&document).map_err(|_| CredentialRepositoryError::Integrity)?,
        );
        if u64::try_from(plaintext.len()).unwrap_or(u64::MAX) > MAX_DOCUMENT_BYTES {
            return Err(CredentialRepositoryError::CapacityExceeded);
        }
        let envelope = self.encrypt(&plaintext)?;
        atomic_private_write(&self.path, &envelope)
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CredentialRepositoryError> {
        let key_bytes = self.load_or_create_key()?;
        let key = Key::<XChaCha20Poly1305>::try_from(key_bytes.as_slice())
            .map_err(|_| CredentialRepositoryError::Integrity)?;
        let cipher = XChaCha20Poly1305::new(&key);
        let mut nonce_bytes = [0_u8; NONCE_BYTES];
        getrandom::fill(&mut nonce_bytes).map_err(|_| CredentialRepositoryError::Unavailable)?;
        let nonce = XNonce::try_from(nonce_bytes.as_slice())
            .map_err(|_| CredentialRepositoryError::Integrity)?;
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: AAD,
                },
            )
            .map_err(|_| CredentialRepositoryError::Unavailable)?;
        let mut envelope = Vec::with_capacity(MAGIC.len() + NONCE_BYTES + ciphertext.len());
        envelope.extend_from_slice(MAGIC);
        envelope.extend_from_slice(&nonce_bytes);
        envelope.extend_from_slice(&ciphertext);
        Ok(envelope)
    }

    fn decrypt(&self, envelope: &[u8]) -> Result<Zeroizing<Vec<u8>>, CredentialRepositoryError> {
        let header = MAGIC.len() + NONCE_BYTES;
        if envelope.len() <= header || envelope.get(..MAGIC.len()) != Some(MAGIC) {
            return Err(CredentialRepositoryError::Integrity);
        }
        let key_bytes = self.load_existing_key()?;
        let key = Key::<XChaCha20Poly1305>::try_from(key_bytes.as_slice())
            .map_err(|_| CredentialRepositoryError::Integrity)?;
        let cipher = XChaCha20Poly1305::new(&key);
        let nonce = XNonce::try_from(&envelope[MAGIC.len()..header])
            .map_err(|_| CredentialRepositoryError::Integrity)?;
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &envelope[header..],
                    aad: AAD,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| CredentialRepositoryError::Integrity)
    }

    fn load_or_create_key(&self) -> Result<Zeroizing<Vec<u8>>, CredentialRepositoryError> {
        ensure_regular_or_absent(&self.key_path)?;
        if self.key_path.exists() {
            return self.load_existing_key();
        }
        let parent = self
            .key_path
            .parent()
            .ok_or(CredentialRepositoryError::Unavailable)?;
        create_private_directory(parent)?;
        let mut bytes = Zeroizing::new(vec![0_u8; KEY_BYTES]);
        getrandom::fill(&mut bytes).map_err(|_| CredentialRepositoryError::Unavailable)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        private_mode(&mut options);
        match options.open(&self.key_path) {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|_| CredentialRepositoryError::Unavailable)?;
                Ok(bytes)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                self.load_existing_key()
            }
            Err(_) => Err(CredentialRepositoryError::Unavailable),
        }
    }

    fn load_existing_key(&self) -> Result<Zeroizing<Vec<u8>>, CredentialRepositoryError> {
        ensure_regular_or_absent(&self.key_path)?;
        ensure_private_parent(&self.key_path)?;
        let mut bytes = Zeroizing::new(Vec::new());
        File::open(&self.key_path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|_| CredentialRepositoryError::Unavailable)?;
        if bytes.len() != KEY_BYTES {
            return Err(CredentialRepositoryError::Integrity);
        }
        Ok(bytes)
    }
}

impl CredentialRepository for EncryptedJsonCredentialRepository {
    fn upsert(&self, record: CredentialRecord) -> Result<(), CredentialRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| CredentialRepositoryError::Unavailable)?;
        let mut records = self.read_records()?;
        records.retain(|old| old.profile_id() != record.profile_id() || old.id() != record.id());
        if records.len() >= MAX_RECORDS {
            return Err(CredentialRepositoryError::CapacityExceeded);
        }
        records.push(record);
        self.write_records(&records)
    }

    fn list(
        &self,
        profile_id: &CredentialProfileId,
    ) -> Result<Vec<CredentialRecord>, CredentialRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| CredentialRepositoryError::Unavailable)?;
        Ok(self
            .read_records()?
            .into_iter()
            .filter(|record| record.profile_id() == profile_id)
            .collect())
    }

    fn get(
        &self,
        profile_id: &CredentialProfileId,
        credential_id: &CredentialId,
    ) -> Result<CredentialRecord, CredentialRepositoryError> {
        self.list(profile_id)?
            .into_iter()
            .find(|record| record.id() == credential_id)
            .ok_or(CredentialRepositoryError::NotFound)
    }

    fn remove(
        &self,
        profile_id: &CredentialProfileId,
        credential_id: &CredentialId,
    ) -> Result<(), CredentialRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| CredentialRepositoryError::Unavailable)?;
        let mut records = self.read_records()?;
        let before = records.len();
        records.retain(|record| record.profile_id() != profile_id || record.id() != credential_id);
        if records.len() == before {
            return Err(CredentialRepositoryError::NotFound);
        }
        self.write_records(&records)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoreDocument {
    schema_version: u32,
    records: Vec<StoredRecord>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredRecord {
    profile_id: String,
    credential_id: String,
    signed_bytes_base64: String,
    #[serde(default)]
    detached_proof_base64: Option<String>,
    #[serde(default)]
    private_material_base64: Option<String>,
    display_name: String,
    issuer_did: String,
    subject_did: Option<String>,
    format: String,
    issued_at_ms: Option<u64>,
    verification_outcome: String,
    verification_stages: Vec<StoredStage>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredStage {
    name: String,
    status: String,
    reason_code: Option<String>,
}

impl StoreDocument {
    fn from_domain(records: &[CredentialRecord]) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            records: records.iter().map(StoredRecord::from_domain).collect(),
        }
    }

    fn to_domain(&self) -> Result<Vec<CredentialRecord>, CredentialRepositoryError> {
        if !matches!(self.schema_version, 1 | 2 | SCHEMA_VERSION)
            || self.records.len() > MAX_RECORDS
            || (self.schema_version < SCHEMA_VERSION
                && self
                    .records
                    .iter()
                    .any(|record| record.detached_proof_base64.is_some()))
        {
            return Err(CredentialRepositoryError::Integrity);
        }
        let records = self
            .records
            .iter()
            .map(StoredRecord::to_domain)
            .collect::<Result<Vec<_>, _>>()?;
        let unique = records
            .iter()
            .map(|record| (record.profile_id().as_str(), record.id().as_str()))
            .collect::<BTreeSet<_>>();
        if unique.len() != records.len() {
            return Err(CredentialRepositoryError::Integrity);
        }
        Ok(records)
    }
}

impl StoredRecord {
    fn from_domain(record: &CredentialRecord) -> Self {
        let metadata = record.metadata();
        Self {
            profile_id: record.profile_id().as_str().to_owned(),
            credential_id: record.id().as_str().to_owned(),
            signed_bytes_base64: general_purpose::STANDARD.encode(record.signed_bytes()),
            detached_proof_base64: record
                .detached_proof()
                .map(|proof| general_purpose::STANDARD.encode(proof.as_bytes())),
            private_material_base64: record
                .private_material()
                .map(|material| general_purpose::STANDARD.encode(material.as_bytes())),
            display_name: metadata.display_name().to_owned(),
            issuer_did: metadata.issuer_did().to_owned(),
            subject_did: metadata.subject_did().map(str::to_owned),
            format: metadata.format().as_str().to_owned(),
            issued_at_ms: metadata.issued_at().map(|value| value.value()),
            verification_outcome: record.verification().outcome().as_str().to_owned(),
            verification_stages: record
                .verification()
                .stages()
                .iter()
                .map(|stage| StoredStage {
                    name: stage.name().as_str().to_owned(),
                    status: stage.status().as_str().to_owned(),
                    reason_code: stage.reason_code().map(str::to_owned),
                })
                .collect(),
        }
    }

    fn to_domain(&self) -> Result<CredentialRecord, CredentialRepositoryError> {
        let profile_id = CredentialProfileId::parse(self.profile_id.clone())
            .map_err(|_| CredentialRepositoryError::Integrity)?;
        let credential_id = CredentialId::parse(self.credential_id.clone())
            .map_err(|_| CredentialRepositoryError::Integrity)?;
        let signed_bytes = general_purpose::STANDARD
            .decode(&self.signed_bytes_base64)
            .map_err(|_| CredentialRepositoryError::Integrity)?;
        let detached_proof = self
            .detached_proof_base64
            .as_deref()
            .map(|encoded| {
                general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| CredentialRepositoryError::Integrity)
                    .and_then(|bytes| {
                        CredentialDetachedProof::new(bytes)
                            .map_err(|_| CredentialRepositoryError::Integrity)
                    })
            })
            .transpose()?;
        let private_material = self
            .private_material_base64
            .as_deref()
            .map(|encoded| {
                general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| CredentialRepositoryError::Integrity)
                    .and_then(|bytes| {
                        CredentialPrivateMaterial::new(bytes)
                            .map_err(|_| CredentialRepositoryError::Integrity)
                    })
            })
            .transpose()?;
        let format =
            CredentialFormat::parse(&self.format).ok_or(CredentialRepositoryError::Integrity)?;
        let metadata = CredentialMetadata::new(
            self.display_name.clone(),
            self.issuer_did.clone(),
            self.subject_did.clone(),
            format,
            self.issued_at_ms.map(UnixTimestampMillis::new),
        )
        .map_err(|_| CredentialRepositoryError::Integrity)?;
        let stages = self
            .verification_stages
            .iter()
            .map(|stage| {
                VerificationStage::new(
                    VerificationStageName::parse(&stage.name)
                        .ok_or(CredentialRepositoryError::Integrity)?,
                    VerificationStageStatus::parse(&stage.status)
                        .ok_or(CredentialRepositoryError::Integrity)?,
                    stage.reason_code.clone(),
                )
                .map_err(|_| CredentialRepositoryError::Integrity)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let verification = VerificationReport::new(
            VerificationOutcome::parse(&self.verification_outcome)
                .ok_or(CredentialRepositoryError::Integrity)?,
            stages,
        )
        .map_err(|_| CredentialRepositoryError::Integrity)?;
        CredentialRecord::new_with_proof_and_private_material(
            profile_id,
            credential_id,
            signed_bytes,
            detached_proof,
            private_material,
            metadata,
            verification,
        )
        .map_err(|_| CredentialRepositoryError::Integrity)
    }
}

fn ensure_regular_or_absent(path: &Path) -> Result<(), CredentialRepositoryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(CredentialRepositoryError::Integrity)
        }
        #[cfg(unix)]
        Ok(metadata) => {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o077 == 0 {
                Ok(())
            } else {
                Err(CredentialRepositoryError::Integrity)
            }
        }
        #[cfg(not(unix))]
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(CredentialRepositoryError::Unavailable),
    }
}

fn create_private_directory(path: &Path) -> Result<(), CredentialRepositoryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(CredentialRepositoryError::Integrity);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(CredentialRepositoryError::Integrity);
                }
            }
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(CredentialRepositoryError::Unavailable),
    }
    fs::create_dir_all(path).map_err(|_| CredentialRepositoryError::Unavailable)?;
    let metadata =
        fs::symlink_metadata(path).map_err(|_| CredentialRepositoryError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CredentialRepositoryError::Integrity);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| CredentialRepositoryError::Unavailable)?;
    }
    Ok(())
}

fn ensure_private_parent(path: &Path) -> Result<(), CredentialRepositoryError> {
    let parent = path
        .parent()
        .ok_or(CredentialRepositoryError::Unavailable)?;
    create_private_directory(parent)
}

fn atomic_private_write(path: &Path, bytes: &[u8]) -> Result<(), CredentialRepositoryError> {
    ensure_regular_or_absent(path)?;
    let parent = path
        .parent()
        .ok_or(CredentialRepositoryError::Unavailable)?;
    create_private_directory(parent)?;
    let temporary = temporary_path(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    private_mode(&mut options);
    let mut file = options
        .open(&temporary)
        .map_err(|_| CredentialRepositoryError::Unavailable)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| CredentialRepositoryError::Unavailable)?;
    fs::rename(&temporary, path).map_err(|_| CredentialRepositoryError::Unavailable)
}

#[cfg(unix)]
fn private_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt as _;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn private_mode(_: &mut OpenOptions) {}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("credentials.enc");
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.tmp-{}-{sequence}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        sync::atomic::{AtomicU64, Ordering},
    };

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Store {
        root: PathBuf,
        path: PathBuf,
        key: PathBuf,
    }
    impl Store {
        fn new() -> Self {
            let root = env::temp_dir().join(format!(
                "oxid-credential-store-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            Self {
                path: root.join("private/credentials.enc"),
                key: root.join("private/credentials.key"),
                root,
            }
        }
    }
    impl Drop for Store {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn record() -> CredentialRecord {
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
                    .expect("detached proof"),
            ),
            Some(
                CredentialPrivateMaterial::new(b"claim-opening-material".to_vec())
                    .expect("private material"),
            ),
            CredentialMetadata::new("Identity credential", "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", None, CredentialFormat::MidnightCompactVc, Some(UnixTimestampMillis::new(7))).expect("metadata"),
            VerificationReport::new(VerificationOutcome::Valid, stages).expect("report"),
        ).expect("record")
    }

    #[test]
    fn encrypts_original_bytes_and_reopens_records() {
        let store = Store::new();
        let repository = EncryptedJsonCredentialRepository::new(&store.path, &store.key);
        repository.upsert(record()).expect("save");
        let envelope = fs::read(&store.path).expect("envelope");
        assert!(envelope.starts_with(MAGIC));
        assert!(
            !envelope
                .windows(b"signed-private-credential".len())
                .any(|window| window == b"signed-private-credential")
        );
        assert!(
            !envelope
                .windows(b"Identity credential".len())
                .any(|window| window == b"Identity credential")
        );
        assert!(
            !envelope
                .windows(b"claim-opening-material".len())
                .any(|window| window == b"claim-opening-material")
        );
        assert!(
            !envelope
                .windows(b"detached-credential-proof".len())
                .any(|window| window == b"detached-credential-proof")
        );
        let reopened = EncryptedJsonCredentialRepository::new(&store.path, &store.key);
        let records = reopened
            .list(&CredentialProfileId::parse("profile_one").expect("profile"))
            .expect("list");
        assert_eq!(records, vec![record()]);
    }

    #[test]
    fn reads_schema_one_records_as_private_material_absent() {
        let mut value = serde_json::to_value(StoreDocument::from_domain(&[record()]))
            .expect("document serializes");
        value["schemaVersion"] = serde_json::json!(1);
        value["records"][0]
            .as_object_mut()
            .expect("stored record")
            .remove("privateMaterialBase64");
        value["records"][0]
            .as_object_mut()
            .expect("stored record")
            .remove("detachedProofBase64");
        let legacy: StoreDocument = serde_json::from_value(value).expect("legacy document parses");
        let records = legacy.to_domain().expect("legacy document migrates");
        assert_eq!(records.len(), 1);
        assert!(records[0].private_material().is_none());
        assert!(records[0].detached_proof().is_none());
    }

    #[test]
    fn reads_schema_two_records_as_detached_proof_absent() {
        let mut value = serde_json::to_value(StoreDocument::from_domain(&[record()]))
            .expect("document serializes");
        value["schemaVersion"] = serde_json::json!(2);
        value["records"][0]
            .as_object_mut()
            .expect("stored record")
            .remove("detachedProofBase64");
        let legacy: StoreDocument = serde_json::from_value(value).expect("legacy document parses");
        let records = legacy.to_domain().expect("legacy document migrates");
        assert_eq!(records.len(), 1);
        assert!(records[0].detached_proof().is_none());
        assert!(records[0].private_material().is_some());
    }

    #[test]
    fn rejects_detached_proof_mislabeled_as_a_legacy_schema() {
        let mut value = serde_json::to_value(StoreDocument::from_domain(&[record()]))
            .expect("document serializes");
        value["schemaVersion"] = serde_json::json!(2);
        let legacy: StoreDocument = serde_json::from_value(value).expect("document parses");
        assert_eq!(
            legacy.to_domain(),
            Err(CredentialRepositoryError::Integrity)
        );
    }

    #[test]
    fn authentication_fails_closed_after_ciphertext_tampering() {
        let store = Store::new();
        let repository = EncryptedJsonCredentialRepository::new(&store.path, &store.key);
        repository.upsert(record()).expect("save");
        let mut envelope = fs::read(&store.path).expect("envelope");
        let last = envelope.last_mut().expect("ciphertext");
        *last ^= 1;
        fs::write(&store.path, envelope).expect("tamper");
        assert_eq!(
            repository.list(&CredentialProfileId::parse("profile_one").expect("profile")),
            Err(CredentialRepositoryError::Integrity)
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_store_or_key_files_visible_to_other_users() {
        use std::os::unix::fs::PermissionsExt as _;

        let store = Store::new();
        let repository = EncryptedJsonCredentialRepository::new(&store.path, &store.key);
        repository.upsert(record()).expect("save");
        fs::set_permissions(&store.path, fs::Permissions::from_mode(0o644))
            .expect("make store insecure");
        assert_eq!(
            repository.list(&CredentialProfileId::parse("profile_one").expect("profile")),
            Err(CredentialRepositoryError::Integrity)
        );

        fs::set_permissions(&store.path, fs::Permissions::from_mode(0o600))
            .expect("restore store permissions");
        fs::set_permissions(&store.key, fs::Permissions::from_mode(0o640))
            .expect("make key insecure");
        assert_eq!(
            repository.list(&CredentialProfileId::parse("profile_one").expect("profile")),
            Err(CredentialRepositoryError::Integrity)
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_and_does_not_repermission_a_shared_parent_directory() {
        use std::os::unix::fs::PermissionsExt as _;

        let store = Store::new();
        let shared = store.root.join("shared");
        fs::create_dir_all(&shared).expect("shared directory");
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o755))
            .expect("shared permissions");
        let repository = EncryptedJsonCredentialRepository::new(
            shared.join("credentials.enc"),
            shared.join("credentials.key"),
        );
        assert_eq!(
            repository.upsert(record()),
            Err(CredentialRepositoryError::Integrity)
        );
        assert_eq!(
            fs::metadata(&shared)
                .expect("shared metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );

        let protected = Store::new();
        let repository = EncryptedJsonCredentialRepository::new(&protected.path, &protected.key);
        repository.upsert(record()).expect("protected save");
        let parent = protected.path.parent().expect("private parent");
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755))
            .expect("weaken parent permissions");
        assert_eq!(
            repository.list(&CredentialProfileId::parse("profile_one").expect("profile")),
            Err(CredentialRepositoryError::Integrity)
        );
    }
}
