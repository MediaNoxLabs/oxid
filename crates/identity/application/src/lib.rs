// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use oxid_foundation::OpaqueIdError;
use oxid_identity_domain::{
    DidDocument, DidRecord, DidResolution, IdentityProfileId, MidnightDid, MidnightDidError,
};

mod lifecycle;

pub use lifecycle::*;

pub type DidResolutionPortFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DidResolution, DidResolutionPortError>> + Send + 'a>>;

/// Resolver boundary. Implementations may call a configured service or a
/// deliberately narrow standalone fixture, but never receive a profile scope.
pub trait DidResolutionPort: Send + Sync {
    fn resolve<'a>(&'a self, did: &'a MidnightDid) -> DidResolutionPortFuture<'a>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidResolutionPortError {
    Unavailable,
    NotFound,
    InvalidDid,
    MethodNotSupported,
    InvalidResponse,
    Rejected,
}

impl fmt::Display for DidResolutionPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "DID resolution capability is unavailable",
            Self::NotFound => "DID was not found",
            Self::InvalidDid => "DID resolver rejected the identifier",
            Self::MethodNotSupported => "DID method is not supported by the resolver",
            Self::InvalidResponse => "DID resolver returned an invalid response",
            Self::Rejected => "DID resolution was rejected",
        })
    }
}

impl Error for DidResolutionPortError {}

/// Profile-scoped public DID document persistence boundary.
pub trait DidRecordRepository: Send + Sync {
    fn upsert(&self, record: DidRecord) -> Result<(), DidRecordRepositoryError>;
    fn list(
        &self,
        profile_id: &IdentityProfileId,
    ) -> Result<Vec<DidRecord>, DidRecordRepositoryError>;
    fn get(
        &self,
        profile_id: &IdentityProfileId,
        did: &MidnightDid,
    ) -> Result<DidRecord, DidRecordRepositoryError>;
    fn remove(
        &self,
        profile_id: &IdentityProfileId,
        did: &MidnightDid,
    ) -> Result<(), DidRecordRepositoryError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidRecordRepositoryError {
    NotFound,
    CapacityExceeded,
    Integrity,
    Unavailable,
}

impl fmt::Display for DidRecordRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "DID record was not found",
            Self::CapacityExceeded => "DID record capacity was exceeded",
            Self::Integrity => "DID record storage failed integrity validation",
            Self::Unavailable => "DID record storage is unavailable",
        })
    }
}

impl Error for DidRecordRepositoryError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveDidCommand {
    pub profile_id: String,
    pub did: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidRecordQuery {
    pub profile_id: String,
    pub did: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListDidRecordsQuery {
    pub profile_id: String,
}

pub trait ResolveDidUseCase: Send + Sync {
    fn execute<'a>(&'a self, command: ResolveDidCommand) -> DidRecordViewFuture<'a>;
}

pub type DidRecordViewFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DidRecordView, DidOperationError>> + Send + 'a>>;

pub trait ListDidRecordsUseCase: Send + Sync {
    fn execute(&self, query: ListDidRecordsQuery) -> Result<Vec<DidRecordView>, DidOperationError>;
}

pub trait GetDidRecordUseCase: Send + Sync {
    fn execute(&self, query: DidRecordQuery) -> Result<DidRecordView, DidOperationError>;
}

pub trait ForgetDidUseCase: Send + Sync {
    fn execute(&self, command: DidRecordQuery) -> Result<(), DidOperationError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DidOperationError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidDid(MidnightDidError),
    Resolution(DidResolutionPortError),
    Persistence(DidRecordRepositoryError),
    Lifecycle(DidLifecyclePortError),
    InvalidNetwork,
    EmptyPayload,
    PayloadTooLarge,
    ConfirmationRequired,
    InvalidConfirmation,
    SubjectMismatch,
}

impl fmt::Display for DidOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
            Self::InvalidDid(error) => error.fmt(formatter),
            Self::Resolution(error) => error.fmt(formatter),
            Self::Persistence(error) => error.fmt(formatter),
            Self::Lifecycle(error) => error.fmt(formatter),
            Self::InvalidNetwork => formatter.write_str("Midnight DID network is unsupported"),
            Self::EmptyPayload => formatter.write_str("DID signing payload must not be empty"),
            Self::PayloadTooLarge => {
                formatter.write_str("DID signing payload exceeds the application limit")
            }
            Self::ConfirmationRequired => formatter.write_str("explicit confirmation is required"),
            Self::InvalidConfirmation => formatter.write_str("confirmation intent is invalid"),
            Self::SubjectMismatch => {
                formatter.write_str("resolved DID document subject does not match the request")
            }
        }
    }
}

impl Error for DidOperationError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicJwkView {
    pub key_type: String,
    pub curve: String,
    pub x: String,
    pub y: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationMethodView {
    pub id: String,
    pub controller: String,
    pub public_key_jwk: PublicJwkView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationRelationshipView {
    pub relationship: String,
    pub method_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceEndpointView {
    pub value: String,
    pub is_json_object: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidServiceView {
    pub id: String,
    pub types: Vec<String>,
    pub endpoints: Vec<ServiceEndpointView>,
    pub endpoint_was_array: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidDocumentView {
    pub contexts: Vec<String>,
    pub id: String,
    pub network: String,
    pub also_known_as: Vec<String>,
    pub verification_methods: Vec<VerificationMethodView>,
    pub relationships: Vec<VerificationRelationshipView>,
    pub services: Vec<DidServiceView>,
}

impl From<&DidDocument> for DidDocumentView {
    fn from(document: &DidDocument) -> Self {
        Self {
            contexts: document.contexts().to_vec(),
            id: document.id().as_str().to_owned(),
            network: document.id().network().as_str().to_owned(),
            also_known_as: document.also_known_as().to_vec(),
            verification_methods: document
                .verification_methods()
                .iter()
                .map(|method| {
                    let jwk = method.public_key_jwk();
                    VerificationMethodView {
                        id: method.id().to_owned(),
                        controller: method.controller().as_str().to_owned(),
                        public_key_jwk: PublicJwkView {
                            key_type: jwk.key_type().as_str().to_owned(),
                            curve: jwk.curve().as_str().to_owned(),
                            x: jwk.x().to_owned(),
                            y: jwk.y().map(str::to_owned),
                        },
                    }
                })
                .collect(),
            relationships: document
                .relationships()
                .iter()
                .map(|entry| VerificationRelationshipView {
                    relationship: entry.relationship().as_str().to_owned(),
                    method_ids: entry.method_ids().to_vec(),
                })
                .collect(),
            services: document
                .services()
                .iter()
                .map(|service| DidServiceView {
                    id: service.id().to_owned(),
                    types: service.types().to_vec(),
                    endpoints: service
                        .endpoints()
                        .iter()
                        .map(|endpoint| ServiceEndpointView {
                            value: endpoint.value().to_owned(),
                            is_json_object: endpoint.is_json_object(),
                        })
                        .collect(),
                    endpoint_was_array: service.endpoint_was_array(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidDocumentMetadataView {
    pub created: Option<String>,
    pub updated: Option<String>,
    pub deactivated: Option<bool>,
    pub version_id: Option<String>,
    pub next_update: Option<String>,
    pub next_version_id: Option<String>,
    pub equivalent_ids: Vec<String>,
    pub canonical_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidRecordView {
    pub document: DidDocumentView,
    pub document_metadata: DidDocumentMetadataView,
    pub content_type: Option<String>,
    pub source: String,
}

impl From<&DidResolution> for DidRecordView {
    fn from(resolution: &DidResolution) -> Self {
        let metadata = resolution.document_metadata();
        Self {
            document: DidDocumentView::from(resolution.document()),
            document_metadata: DidDocumentMetadataView {
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
            source: resolution.source().as_str().to_owned(),
        }
    }
}

pub struct DidService {
    repository: Arc<dyn DidRecordRepository>,
    resolver: Arc<dyn DidResolutionPort>,
    lifecycle: Arc<dyn DidLifecyclePort>,
}

impl DidService {
    #[must_use]
    pub fn new<R, D>(repository: Arc<R>, resolver: Arc<D>) -> Self
    where
        R: DidRecordRepository + 'static,
        D: DidResolutionPort + 'static,
    {
        Self {
            repository,
            resolver,
            lifecycle: Arc::new(UnavailableDidLifecycle),
        }
    }

    #[must_use]
    pub const fn from_ports(
        repository: Arc<dyn DidRecordRepository>,
        resolver: Arc<dyn DidResolutionPort>,
        lifecycle: Arc<dyn DidLifecyclePort>,
    ) -> Self {
        Self {
            repository,
            resolver,
            lifecycle,
        }
    }
}

fn parse_profile(value: String) -> Result<IdentityProfileId, DidOperationError> {
    IdentityProfileId::parse(value).map_err(DidOperationError::InvalidProfileIdentifier)
}

fn parse_did(value: String) -> Result<MidnightDid, DidOperationError> {
    MidnightDid::parse(value).map_err(DidOperationError::InvalidDid)
}

impl ResolveDidUseCase for DidService {
    fn execute<'a>(&'a self, command: ResolveDidCommand) -> DidRecordViewFuture<'a> {
        Box::pin(async move {
            let profile_id = parse_profile(command.profile_id)?;
            let did = parse_did(command.did)?;
            let resolution = self
                .resolver
                .resolve(&did)
                .await
                .map_err(DidOperationError::Resolution)?;
            if resolution.document().id() != &did {
                return Err(DidOperationError::SubjectMismatch);
            }
            self.repository
                .upsert(DidRecord::new(profile_id, resolution.clone()))
                .map_err(DidOperationError::Persistence)?;
            Ok(DidRecordView::from(&resolution))
        })
    }
}

impl ListDidRecordsUseCase for DidService {
    fn execute(&self, query: ListDidRecordsQuery) -> Result<Vec<DidRecordView>, DidOperationError> {
        let profile_id = parse_profile(query.profile_id)?;
        let mut records = self
            .repository
            .list(&profile_id)
            .map_err(DidOperationError::Persistence)?;
        records.sort_by(|left, right| {
            left.resolution()
                .document()
                .id()
                .cmp(right.resolution().document().id())
        });
        Ok(records
            .iter()
            .map(|record| DidRecordView::from(record.resolution()))
            .collect())
    }
}

impl GetDidRecordUseCase for DidService {
    fn execute(&self, query: DidRecordQuery) -> Result<DidRecordView, DidOperationError> {
        let profile_id = parse_profile(query.profile_id)?;
        let did = parse_did(query.did)?;
        let record = self
            .repository
            .get(&profile_id, &did)
            .map_err(DidOperationError::Persistence)?;
        Ok(DidRecordView::from(record.resolution()))
    }
}

impl ForgetDidUseCase for DidService {
    fn execute(&self, command: DidRecordQuery) -> Result<(), DidOperationError> {
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        self.repository
            .remove(&profile_id, &did)
            .map_err(DidOperationError::Persistence)
    }
}

/// Fails closed when a production resolver has not been explicitly composed.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableDidResolver;

impl DidResolutionPort for UnavailableDidResolver {
    fn resolve<'a>(&'a self, _: &'a MidnightDid) -> DidResolutionPortFuture<'a> {
        Box::pin(async { Err(DidResolutionPortError::Unavailable) })
    }
}

/// Fails closed when a production identity store has not been reviewed.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableDidRecordRepository;

impl DidRecordRepository for UnavailableDidRecordRepository {
    fn upsert(&self, _: DidRecord) -> Result<(), DidRecordRepositoryError> {
        Err(DidRecordRepositoryError::Unavailable)
    }
    fn list(&self, _: &IdentityProfileId) -> Result<Vec<DidRecord>, DidRecordRepositoryError> {
        Err(DidRecordRepositoryError::Unavailable)
    }
    fn get(
        &self,
        _: &IdentityProfileId,
        _: &MidnightDid,
    ) -> Result<DidRecord, DidRecordRepositoryError> {
        Err(DidRecordRepositoryError::Unavailable)
    }
    fn remove(
        &self,
        _: &IdentityProfileId,
        _: &MidnightDid,
    ) -> Result<(), DidRecordRepositoryError> {
        Err(DidRecordRepositoryError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_identity_domain::{
        DID_CONTEXT, DidDocumentMetadata, DidDocumentParts, DidResolutionMetadata,
        DidResolutionSource, JWK_CONTEXT,
    };
    use std::sync::Mutex;

    const DID: &str =
        "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn resolution() -> DidResolution {
        let did = MidnightDid::parse(DID).expect("DID");
        DidResolution::new(
            DidDocument::new(DidDocumentParts {
                contexts: vec![DID_CONTEXT.to_owned(), JWK_CONTEXT.to_owned()],
                id: did.clone(),
                controllers: vec![did],
                also_known_as: Vec::new(),
                verification_methods: Vec::new(),
                relationships: Vec::new(),
                services: Vec::new(),
            })
            .expect("document"),
            DidDocumentMetadata::default(),
            DidResolutionMetadata::default(),
            DidResolutionSource::Standalone,
        )
    }

    #[derive(Default)]
    struct MemoryRepository(Mutex<Vec<DidRecord>>);
    impl DidRecordRepository for MemoryRepository {
        fn upsert(&self, record: DidRecord) -> Result<(), DidRecordRepositoryError> {
            let mut records = self.0.lock().expect("lock");
            records.retain(|old| {
                old.profile_id() != record.profile_id()
                    || old.resolution().document().id() != record.resolution().document().id()
            });
            records.push(record);
            Ok(())
        }
        fn list(
            &self,
            profile: &IdentityProfileId,
        ) -> Result<Vec<DidRecord>, DidRecordRepositoryError> {
            Ok(self
                .0
                .lock()
                .expect("lock")
                .iter()
                .filter(|record| record.profile_id() == profile)
                .cloned()
                .collect())
        }
        fn get(
            &self,
            profile: &IdentityProfileId,
            did: &MidnightDid,
        ) -> Result<DidRecord, DidRecordRepositoryError> {
            self.list(profile)?
                .into_iter()
                .find(|record| record.resolution().document().id() == did)
                .ok_or(DidRecordRepositoryError::NotFound)
        }
        fn remove(
            &self,
            profile: &IdentityProfileId,
            did: &MidnightDid,
        ) -> Result<(), DidRecordRepositoryError> {
            let mut records = self.0.lock().expect("lock");
            let before = records.len();
            records.retain(|record| {
                record.profile_id() != profile || record.resolution().document().id() != did
            });
            if records.len() == before {
                Err(DidRecordRepositoryError::NotFound)
            } else {
                Ok(())
            }
        }
    }
    struct FixedResolver;
    impl DidResolutionPort for FixedResolver {
        fn resolve<'a>(&'a self, _: &'a MidnightDid) -> DidResolutionPortFuture<'a> {
            Box::pin(async { Ok(resolution()) })
        }
    }

    struct FixedLifecycle;
    impl DidLifecyclePort for FixedLifecycle {
        fn create(
            &self,
            _: &IdentityProfileId,
            network: oxid_identity_domain::MidnightNetwork,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            if network == oxid_identity_domain::MidnightNetwork::Undeployed {
                Ok(resolution())
            } else {
                Err(DidLifecyclePortError::UnsupportedNetwork)
            }
        }

        fn update(
            &self,
            _: &IdentityProfileId,
            current: &DidResolution,
            _: DidUpdate,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            Ok(current.clone())
        }

        fn deactivate(
            &self,
            _: &IdentityProfileId,
            current: &DidResolution,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            Ok(DidResolution::new(
                current.document().clone(),
                DidDocumentMetadata {
                    deactivated: Some(true),
                    ..current.document_metadata().clone()
                },
                current.resolution_metadata().clone(),
                DidResolutionSource::Standalone,
            ))
        }

        fn sign(
            &self,
            _: &IdentityProfileId,
            _: &DidResolution,
            method_id: &str,
            _: &[u8],
        ) -> Result<DidLifecycleSignature, DidLifecyclePortError> {
            Ok(DidLifecycleSignature {
                method_id: method_id.to_owned(),
                algorithm: DidKeyAlgorithm::Ed25519,
                signature_bytes: vec![7; 64],
            })
        }
    }

    fn confirmation(confirmed: bool) -> DidOperationConfirmation {
        DidOperationConfirmation {
            title: "Authorize DID operation".to_owned(),
            summary: "Exercise the application lifecycle boundary".to_owned(),
            confirmed,
        }
    }

    #[test]
    fn resolves_persists_lists_gets_and_forgets_by_profile() {
        let service = DidService::new(
            Arc::new(MemoryRepository::default()),
            Arc::new(FixedResolver),
        );
        let command = ResolveDidCommand {
            profile_id: "profile_test".to_owned(),
            did: DID.to_owned(),
        };
        assert_eq!(
            futures_for_test::block_on(ResolveDidUseCase::execute(&service, command))
                .expect("resolve")
                .document
                .id,
            DID
        );
        assert_eq!(
            ListDidRecordsUseCase::execute(
                &service,
                ListDidRecordsQuery {
                    profile_id: "profile_test".to_owned()
                }
            )
            .expect("list")
            .len(),
            1
        );
        assert!(
            GetDidRecordUseCase::execute(
                &service,
                DidRecordQuery {
                    profile_id: "profile_other".to_owned(),
                    did: DID.to_owned()
                }
            )
            .is_err()
        );
        ForgetDidUseCase::execute(
            &service,
            DidRecordQuery {
                profile_id: "profile_test".to_owned(),
                did: DID.to_owned(),
            },
        )
        .expect("forget");
    }

    #[test]
    fn creates_updates_signs_and_deactivates_with_confirmation() {
        let service = DidService::from_ports(
            Arc::new(MemoryRepository::default()),
            Arc::new(FixedResolver),
            Arc::new(FixedLifecycle),
        );
        let created = CreateDidUseCase::execute(
            &service,
            CreateDidCommand {
                profile_id: "profile_test".to_owned(),
                network: "undeployed".to_owned(),
            },
        )
        .expect("create");
        assert_eq!(created.document.id, DID);

        let denied = UpdateDidUseCase::execute(
            &service,
            UpdateDidCommand {
                profile_id: "profile_test".to_owned(),
                did: DID.to_owned(),
                operation: DidUpdate::AddAlsoKnownAs {
                    value: "https://example.test/denied".to_owned(),
                },
                confirmation: confirmation(false),
            },
        );
        assert_eq!(denied, Err(DidOperationError::ConfirmationRequired));

        UpdateDidUseCase::execute(
            &service,
            UpdateDidCommand {
                profile_id: "profile_test".to_owned(),
                did: DID.to_owned(),
                operation: DidUpdate::AddAlsoKnownAs {
                    value: "https://example.test/accepted".to_owned(),
                },
                confirmation: confirmation(true),
            },
        )
        .expect("update");

        assert_eq!(
            SignDidPayloadUseCase::execute(
                &service,
                SignDidPayloadCommand {
                    profile_id: "profile_test".to_owned(),
                    did: DID.to_owned(),
                    method_id: "#auth-1".to_owned(),
                    payload: b"challenge".to_vec(),
                    confirmation: confirmation(true),
                }
            )
            .expect("sign")
            .signature_bytes
            .len(),
            64
        );
        assert_eq!(
            SignDidPayloadUseCase::execute(
                &service,
                SignDidPayloadCommand {
                    profile_id: "profile_test".to_owned(),
                    did: DID.to_owned(),
                    method_id: "#auth-1".to_owned(),
                    payload: Vec::new(),
                    confirmation: confirmation(true),
                }
            ),
            Err(DidOperationError::EmptyPayload)
        );

        let deactivated = DeactivateDidUseCase::execute(
            &service,
            DeactivateDidCommand {
                profile_id: "profile_test".to_owned(),
                did: DID.to_owned(),
                confirmation: confirmation(true),
            },
        )
        .expect("deactivate");
        assert_eq!(deactivated.document_metadata.deactivated, Some(true));
    }

    mod futures_for_test {
        use std::{
            future::Future,
            pin::pin,
            task::{Context, Poll, Waker},
        };
        pub fn block_on<F: Future>(future: F) -> F::Output {
            let mut future = pin!(future);
            let waker = Waker::noop();
            let mut context = Context::from_waker(waker);
            loop {
                if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                    return output;
                }
            }
        }
    }
}
