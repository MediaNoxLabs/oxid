// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

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

use oxid_identity_application::{DidRecordRepository, DidRecordRepositoryError};
use oxid_identity_domain::{
    DidDocument, DidDocumentMetadata, DidDocumentParts, DidRecord, DidResolution,
    DidResolutionMetadata, DidResolutionSource, IdentityProfileId, JwkCurve, JwkKeyType,
    MidnightDid, PublicJwk, Service, ServiceEndpointValue, VerificationMethod,
    VerificationRelationship, VerificationRelationshipEntry,
};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
const MAX_RECORDS: usize = 128;
const MAX_STORE_BYTES: u64 = 2 * 1_024 * 1_024;
const STORE_FILE_NAME: &str = "did-records.json";
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Durable public DID documents, isolated from profile labels and all private
/// key/credential material.
pub struct JsonDidRecordRepository {
    path: PathBuf,
    access: Mutex<()>,
}

impl JsonDidRecordRepository {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            access: Mutex::new(()),
        }
    }

    #[must_use]
    pub fn configured_path(&self) -> &Path {
        &self.path
    }

    fn load_document(&self) -> Result<StoreDocument, DidRecordRepositoryError> {
        reject_symlink(&self.path)?;
        if let Some(parent) = self.path.parent() {
            reject_symlink(parent)?;
        }
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StoreDocument::default());
            }
            Err(_) => return Err(DidRecordRepositoryError::Unavailable),
        };
        let metadata = file
            .metadata()
            .map_err(|_| DidRecordRepositoryError::Unavailable)?;
        if !metadata.is_file() || metadata.len() > MAX_STORE_BYTES {
            return Err(DidRecordRepositoryError::Integrity);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(DidRecordRepositoryError::Integrity);
            }
            let parent = self
                .path
                .parent()
                .ok_or(DidRecordRepositoryError::Integrity)?;
            let parent_metadata =
                fs::metadata(parent).map_err(|_| DidRecordRepositoryError::Unavailable)?;
            if !parent_metadata.is_dir() || parent_metadata.permissions().mode() & 0o077 != 0 {
                return Err(DidRecordRepositoryError::Integrity);
            }
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_STORE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| DidRecordRepositoryError::Unavailable)?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(DidRecordRepositoryError::Integrity);
        }
        let document: StoreDocument =
            serde_json::from_slice(&bytes).map_err(|_| DidRecordRepositoryError::Integrity)?;
        validate_document(&document)?;
        Ok(document)
    }

    fn save_document(&self, document: &StoreDocument) -> Result<(), DidRecordRepositoryError> {
        validate_document(document)?;
        let parent = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or(DidRecordRepositoryError::Unavailable)?;
        let parent_existed = parent
            .try_exists()
            .map_err(|_| DidRecordRepositoryError::Unavailable)?;
        fs::create_dir_all(parent).map_err(|_| DidRecordRepositoryError::Unavailable)?;
        reject_symlink(parent)?;
        reject_symlink(&self.path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if parent_existed {
                let metadata =
                    fs::metadata(parent).map_err(|_| DidRecordRepositoryError::Unavailable)?;
                if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
                    return Err(DidRecordRepositoryError::Integrity);
                }
            } else {
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
                    .map_err(|_| DidRecordRepositoryError::Unavailable)?;
            }
        }

        let bytes = serde_json::to_vec_pretty(document)
            .map_err(|_| DidRecordRepositoryError::Unavailable)?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(DidRecordRepositoryError::CapacityExceeded);
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
            .map_err(|_| DidRecordRepositoryError::Unavailable)?;
        if file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(DidRecordRepositoryError::Unavailable);
        }
        drop(file);
        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|_| DidRecordRepositoryError::Unavailable)?;
        }
        if fs::rename(&temporary_path, &self.path).is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(DidRecordRepositoryError::Unavailable);
        }
        #[cfg(unix)]
        fs::set_permissions(
            &self.path,
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )
        .map_err(|_| DidRecordRepositoryError::Unavailable)?;
        Ok(())
    }
}

fn reject_symlink(path: &Path) -> Result<(), DidRecordRepositoryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(DidRecordRepositoryError::Integrity)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(DidRecordRepositoryError::Unavailable),
    }
}

impl DidRecordRepository for JsonDidRecordRepository {
    fn upsert(&self, record: DidRecord) -> Result<(), DidRecordRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| DidRecordRepositoryError::Unavailable)?;
        let mut document = self.load_document()?;
        let profile = record.profile_id().as_str();
        let did = record.resolution().document().id().as_str();
        if let Some(existing) = document
            .records
            .iter_mut()
            .find(|entry| entry.profile_id == profile && entry.resolution.document.id == did)
        {
            *existing = StoredRecord::from(&record);
        } else {
            if document.records.len() >= MAX_RECORDS {
                return Err(DidRecordRepositoryError::CapacityExceeded);
            }
            document.records.push(StoredRecord::from(&record));
        }
        self.save_document(&document)
    }

    fn list(
        &self,
        profile_id: &IdentityProfileId,
    ) -> Result<Vec<DidRecord>, DidRecordRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| DidRecordRepositoryError::Unavailable)?;
        records_from_document(&self.load_document()?).map(|records| {
            records
                .into_iter()
                .filter(|record| record.profile_id() == profile_id)
                .collect()
        })
    }

    fn get(
        &self,
        profile_id: &IdentityProfileId,
        did: &MidnightDid,
    ) -> Result<DidRecord, DidRecordRepositoryError> {
        self.list(profile_id)?
            .into_iter()
            .find(|record| record.resolution().document().id() == did)
            .ok_or(DidRecordRepositoryError::NotFound)
    }

    fn remove(
        &self,
        profile_id: &IdentityProfileId,
        did: &MidnightDid,
    ) -> Result<(), DidRecordRepositoryError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| DidRecordRepositoryError::Unavailable)?;
        let mut document = self.load_document()?;
        let before = document.records.len();
        document.records.retain(|entry| {
            entry.profile_id != profile_id.as_str() || entry.resolution.document.id != did.as_str()
        });
        if document.records.len() == before {
            return Err(DidRecordRepositoryError::NotFound);
        }
        self.save_document(&document)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoreDocument {
    schema_version: u32,
    records: Vec<StoredRecord>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredRecord {
    profile_id: String,
    resolution: StoredResolution,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredResolution {
    document: StoredDocument,
    document_metadata: StoredMetadata,
    content_type: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredDocument {
    contexts: Vec<String>,
    id: String,
    controllers: Vec<String>,
    also_known_as: Vec<String>,
    verification_methods: Vec<StoredMethod>,
    relationships: Vec<StoredRelationship>,
    services: Vec<StoredService>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredMethod {
    id: String,
    controller: String,
    public_key_jwk: StoredJwk,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredJwk {
    key_type: String,
    curve: String,
    x: String,
    y: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredRelationship {
    relationship: String,
    method_ids: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredService {
    id: String,
    types: Vec<String>,
    endpoints: Vec<StoredEndpoint>,
    endpoint_was_array: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredEndpoint {
    value: String,
    json_object: bool,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredMetadata {
    created: Option<String>,
    updated: Option<String>,
    deactivated: Option<bool>,
    version_id: Option<String>,
    next_update: Option<String>,
    next_version_id: Option<String>,
    equivalent_ids: Vec<String>,
    canonical_id: Option<String>,
}

impl Default for StoreDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            records: Vec::new(),
        }
    }
}

impl From<&DidRecord> for StoredRecord {
    fn from(record: &DidRecord) -> Self {
        let resolution = record.resolution();
        let document = resolution.document();
        let metadata = resolution.document_metadata();
        Self {
            profile_id: record.profile_id().as_str().to_owned(),
            resolution: StoredResolution {
                document: StoredDocument {
                    contexts: document.contexts().to_vec(),
                    id: document.id().as_str().to_owned(),
                    controllers: vec![document.id().as_str().to_owned()],
                    also_known_as: document.also_known_as().to_vec(),
                    verification_methods: document
                        .verification_methods()
                        .iter()
                        .map(|method| StoredMethod {
                            id: method.id().to_owned(),
                            controller: method.controller().as_str().to_owned(),
                            public_key_jwk: StoredJwk {
                                key_type: method.public_key_jwk().key_type().as_str().to_owned(),
                                curve: method.public_key_jwk().curve().as_str().to_owned(),
                                x: method.public_key_jwk().x().to_owned(),
                                y: method.public_key_jwk().y().map(str::to_owned),
                            },
                        })
                        .collect(),
                    relationships: document
                        .relationships()
                        .iter()
                        .map(|entry| StoredRelationship {
                            relationship: entry.relationship().as_str().to_owned(),
                            method_ids: entry.method_ids().to_vec(),
                        })
                        .collect(),
                    services: document
                        .services()
                        .iter()
                        .map(|service| StoredService {
                            id: service.id().to_owned(),
                            types: service.types().to_vec(),
                            endpoints: service
                                .endpoints()
                                .iter()
                                .map(|endpoint| StoredEndpoint {
                                    value: endpoint.value().to_owned(),
                                    json_object: endpoint.is_json_object(),
                                })
                                .collect(),
                            endpoint_was_array: service.endpoint_was_array(),
                        })
                        .collect(),
                },
                document_metadata: StoredMetadata {
                    created: metadata.created.clone(),
                    updated: metadata.updated.clone(),
                    deactivated: metadata.deactivated,
                    version_id: metadata.version_id.clone(),
                    next_update: metadata.next_update.clone(),
                    next_version_id: metadata.next_version_id.clone(),
                    equivalent_ids: metadata.equivalent_ids.clone(),
                    canonical_id: metadata.canonical_id.clone(),
                },
                content_type: resolution.resolution_metadata().content_type.clone(),
            },
        }
    }
}

fn validate_document(document: &StoreDocument) -> Result<(), DidRecordRepositoryError> {
    if document.schema_version != SCHEMA_VERSION || document.records.len() > MAX_RECORDS {
        return Err(DidRecordRepositoryError::Integrity);
    }
    let records = records_from_document(document)?;
    let unique = records
        .iter()
        .map(|record| {
            (
                record.profile_id().as_str(),
                record.resolution().document().id().as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    if unique.len() != records.len() {
        return Err(DidRecordRepositoryError::Integrity);
    }
    Ok(())
}

fn records_from_document(
    document: &StoreDocument,
) -> Result<Vec<DidRecord>, DidRecordRepositoryError> {
    document
        .records
        .iter()
        .map(StoredRecord::to_domain)
        .collect()
}

impl StoredRecord {
    fn to_domain(&self) -> Result<DidRecord, DidRecordRepositoryError> {
        let profile = IdentityProfileId::parse(self.profile_id.clone())
            .map_err(|_| DidRecordRepositoryError::Integrity)?;
        let subject = MidnightDid::parse(self.resolution.document.id.clone())
            .map_err(|_| DidRecordRepositoryError::Integrity)?;
        let controllers = self
            .resolution
            .document
            .controllers
            .iter()
            .map(|value| {
                MidnightDid::parse(value.clone()).map_err(|_| DidRecordRepositoryError::Integrity)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let methods = self
            .resolution
            .document
            .verification_methods
            .iter()
            .map(|method| {
                let key_type = match method.public_key_jwk.key_type.as_str() {
                    "OKP" => JwkKeyType::Okp,
                    "EC" => JwkKeyType::Ec,
                    _ => return Err(DidRecordRepositoryError::Integrity),
                };
                let curve = match method.public_key_jwk.curve.as_str() {
                    "Ed25519" => JwkCurve::Ed25519,
                    "X25519" => JwkCurve::X25519,
                    "Jubjub" => JwkCurve::Jubjub,
                    "P-256" => JwkCurve::P256,
                    "secp256k1" => JwkCurve::Secp256k1,
                    "BLS12381G1" => JwkCurve::Bls12381G1,
                    "BLS12381G2" => JwkCurve::Bls12381G2,
                    _ => return Err(DidRecordRepositoryError::Integrity),
                };
                let jwk = PublicJwk::new(
                    key_type,
                    curve,
                    method.public_key_jwk.x.clone(),
                    method.public_key_jwk.y.clone(),
                )
                .map_err(|_| DidRecordRepositoryError::Integrity)?;
                let controller = MidnightDid::parse(method.controller.clone())
                    .map_err(|_| DidRecordRepositoryError::Integrity)?;
                VerificationMethod::new(&subject, &method.id, controller, jwk)
                    .map_err(|_| DidRecordRepositoryError::Integrity)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let relationships = self
            .resolution
            .document
            .relationships
            .iter()
            .map(|entry| {
                let relationship = match entry.relationship.as_str() {
                    "authentication" => VerificationRelationship::Authentication,
                    "assertionMethod" => VerificationRelationship::AssertionMethod,
                    "keyAgreement" => VerificationRelationship::KeyAgreement,
                    "capabilityInvocation" => VerificationRelationship::CapabilityInvocation,
                    "capabilityDelegation" => VerificationRelationship::CapabilityDelegation,
                    _ => return Err(DidRecordRepositoryError::Integrity),
                };
                Ok(VerificationRelationshipEntry::new(
                    relationship,
                    entry.method_ids.clone(),
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let services = self
            .resolution
            .document
            .services
            .iter()
            .map(|service| {
                let endpoints = service
                    .endpoints
                    .iter()
                    .map(|endpoint| {
                        if endpoint.json_object {
                            ServiceEndpointValue::json_object(endpoint.value.clone())
                        } else {
                            ServiceEndpointValue::uri(endpoint.value.clone())
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| DidRecordRepositoryError::Integrity)?;
                Service::new(
                    &service.id,
                    service.types.clone(),
                    endpoints,
                    service.endpoint_was_array,
                )
                .map_err(|_| DidRecordRepositoryError::Integrity)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let document = DidDocument::new(DidDocumentParts {
            contexts: self.resolution.document.contexts.clone(),
            id: subject,
            controllers,
            also_known_as: self.resolution.document.also_known_as.clone(),
            verification_methods: methods,
            relationships,
            services,
        })
        .map_err(|_| DidRecordRepositoryError::Integrity)?;
        let metadata = &self.resolution.document_metadata;
        if self
            .resolution
            .content_type
            .as_deref()
            .is_some_and(|value| {
                !matches!(value, "application/did+ld+json" | "application/did+json")
            })
        {
            return Err(DidRecordRepositoryError::Integrity);
        }
        let resolution = DidResolution::new(
            document,
            DidDocumentMetadata {
                created: metadata.created.clone(),
                updated: metadata.updated.clone(),
                deactivated: metadata.deactivated,
                version_id: metadata.version_id.clone(),
                next_update: metadata.next_update.clone(),
                next_version_id: metadata.next_version_id.clone(),
                equivalent_ids: metadata.equivalent_ids.clone(),
                canonical_id: metadata.canonical_id.clone(),
            },
            DidResolutionMetadata {
                content_type: self.resolution.content_type.clone(),
            },
            DidResolutionSource::Stored,
        );
        Ok(DidRecord::new(profile, resolution))
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(STORE_FILE_NAME);
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.tmp-{}-{sequence}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_identity_domain::{DID_CONTEXT, JWK_CONTEXT};
    use std::{
        env,
        sync::atomic::{AtomicU64, Ordering},
    };

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Store {
        root: PathBuf,
        path: PathBuf,
    }
    impl Store {
        fn new() -> Self {
            let root = env::temp_dir().join(format!(
                "oxid-did-store-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            Self {
                path: root.join("private").join(STORE_FILE_NAME),
                root,
            }
        }
    }
    impl Drop for Store {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn record(profile: &str) -> DidRecord {
        let profile = IdentityProfileId::parse(profile).expect("profile");
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

    #[test]
    fn reopens_profile_scoped_public_records_and_forgets_them() {
        let store = Store::new();
        let repository = JsonDidRecordRepository::new(&store.path);
        repository.upsert(record("profile_one")).expect("save");
        repository.upsert(record("profile_two")).expect("save");
        let reopened = JsonDidRecordRepository::new(&store.path);
        let profile = IdentityProfileId::parse("profile_one").expect("profile");
        let records = reopened.list(&profile).expect("list");
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].resolution().source(),
            DidResolutionSource::Stored
        );
        reopened
            .remove(&profile, records[0].resolution().document().id())
            .expect("remove");
        assert!(reopened.list(&profile).expect("list").is_empty());
    }

    #[test]
    fn rejects_unknown_fields_and_symlinks() {
        let store = Store::new();
        fs::create_dir_all(store.path.parent().expect("parent")).expect("directory");
        fs::write(
            &store.path,
            br#"{"schemaVersion":1,"records":[],"unexpected":true}"#,
        )
        .expect("write");
        assert_eq!(
            JsonDidRecordRepository::new(&store.path)
                .list(&IdentityProfileId::parse("profile_one").expect("profile")),
            Err(DidRecordRepositoryError::Integrity)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            fs::remove_file(&store.path).expect("remove");
            let target = store.root.join("target.json");
            fs::write(&target, br#"{"schemaVersion":1,"records":[]}"#).expect("target");
            symlink(&target, &store.path).expect("symlink");
            assert_eq!(
                JsonDidRecordRepository::new(&store.path)
                    .list(&IdentityProfileId::parse("profile_one").expect("profile")),
                Err(DidRecordRepositoryError::Integrity)
            );
        }
    }
}
