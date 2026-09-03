// SPDX-License-Identifier: Apache-2.0

use super::*;
use oxid_credential_domain::{
    CredentialFormat, VerificationStage, VerificationStageName, VerificationStageStatus,
};
use oxid_foundation::UnixTimestampMillis;
use std::sync::Mutex;

const ISSUER_DID: &str =
    "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const PROFILE: &str = "profile_issue_157";
const CREDENTIAL: &str = "credential_issue_157";
const NEW_SIGNED_BYTES: &[u8] = b"new-signed-credential-secret";
const PRIOR_SIGNED_BYTES: &[u8] = b"prior-signed-credential-secret";

#[derive(Clone, Debug, PartialEq, Eq)]
enum RepositoryCall {
    Upsert(String, String),
    List(String),
    Get(String, String),
    Remove(String, String),
}

struct RecordingRepository {
    record: Mutex<Option<CredentialRecord>>,
    calls: Mutex<Vec<RepositoryCall>>,
    upsert_error: Option<CredentialRepositoryError>,
    read_error: Option<CredentialRepositoryError>,
}

impl RecordingRepository {
    fn empty() -> Self {
        Self {
            record: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
            upsert_error: None,
            read_error: None,
        }
    }

    fn with_prior(record: CredentialRecord, upsert_error: CredentialRepositoryError) -> Self {
        Self {
            record: Mutex::new(Some(record)),
            calls: Mutex::new(Vec::new()),
            upsert_error: Some(upsert_error),
            read_error: None,
        }
    }

    fn failing_reads(error: CredentialRepositoryError) -> Self {
        Self {
            record: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
            upsert_error: None,
            read_error: Some(error),
        }
    }

    fn calls(&self) -> Vec<RepositoryCall> {
        self.calls.lock().expect("calls lock").clone()
    }

    fn stored_record(&self) -> Option<CredentialRecord> {
        self.record.lock().expect("record lock").clone()
    }
}

impl CredentialRepository for RecordingRepository {
    fn upsert(&self, record: CredentialRecord) -> Result<(), CredentialRepositoryError> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(RepositoryCall::Upsert(
                record.profile_id().as_str().to_owned(),
                record.id().as_str().to_owned(),
            ));
        if let Some(error) = self.upsert_error {
            return Err(error);
        }
        *self.record.lock().expect("record lock") = Some(record);
        Ok(())
    }

    fn list(
        &self,
        profile_id: &CredentialProfileId,
    ) -> Result<Vec<CredentialRecord>, CredentialRepositoryError> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(RepositoryCall::List(profile_id.as_str().to_owned()));
        if let Some(error) = self.read_error {
            return Err(error);
        }
        Ok(self
            .stored_record()
            .into_iter()
            .filter(|record| record.profile_id() == profile_id)
            .collect())
    }

    fn get(
        &self,
        profile_id: &CredentialProfileId,
        credential_id: &CredentialId,
    ) -> Result<CredentialRecord, CredentialRepositoryError> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(RepositoryCall::Get(
                profile_id.as_str().to_owned(),
                credential_id.as_str().to_owned(),
            ));
        if let Some(error) = self.read_error {
            return Err(error);
        }
        self.stored_record()
            .filter(|record| record.profile_id() == profile_id && record.id() == credential_id)
            .ok_or(CredentialRepositoryError::NotFound)
    }

    fn remove(
        &self,
        profile_id: &CredentialProfileId,
        credential_id: &CredentialId,
    ) -> Result<(), CredentialRepositoryError> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(RepositoryCall::Remove(
                profile_id.as_str().to_owned(),
                credential_id.as_str().to_owned(),
            ));
        if let Some(error) = self.read_error {
            return Err(error);
        }
        let mut stored = self.record.lock().expect("record lock");
        if stored
            .as_ref()
            .is_some_and(|record| record.profile_id() == profile_id && record.id() == credential_id)
        {
            *stored = None;
            Ok(())
        } else {
            Err(CredentialRepositoryError::NotFound)
        }
    }
}

struct StaticVerifier(Result<CredentialInspection, CredentialVerificationError>);

impl CredentialVerificationPort for StaticVerifier {
    fn inspect<'a>(&'a self, _: &'a [u8], _: Option<&'a [u8]>) -> CredentialInspectionFuture<'a> {
        let result = self.0.clone();
        Box::pin(async move { result })
    }
}

fn valid_inspection(display_name: &str) -> CredentialInspection {
    let stages = VerificationStageName::ALL
        .into_iter()
        .map(|name| {
            VerificationStage::new(name, VerificationStageStatus::Passed, None)
                .expect("valid stage")
        })
        .collect();
    CredentialInspection {
        id: CredentialId::parse(CREDENTIAL).expect("credential id"),
        metadata: CredentialMetadata::new(
            display_name,
            ISSUER_DID,
            None,
            CredentialFormat::MidnightCborPhase1,
            Some(UnixTimestampMillis::new(157)),
        )
        .expect("metadata"),
        verification: VerificationReport::new(VerificationOutcome::Valid, stages)
            .expect("verification report"),
    }
}

fn prior_record() -> CredentialRecord {
    let inspection = valid_inspection("Prior credential");
    CredentialRecord::new(
        CredentialProfileId::parse(PROFILE).expect("profile"),
        inspection.id,
        PRIOR_SIGNED_BYTES.to_vec(),
        inspection.metadata,
        inspection.verification,
    )
    .expect("prior record")
}

fn service(
    repository: Arc<dyn CredentialRepository>,
    verifier: Arc<dyn CredentialVerificationPort>,
) -> CredentialService {
    CredentialService::from_ports(
        repository,
        Arc::new(UnavailableCredentialInbox),
        verifier,
        Arc::new(UnavailableCredentialDisclosure),
    )
}

fn import(service: &CredentialService) -> Result<CredentialView, CredentialOperationError> {
    poll(ImportVerifiedCredentialUseCase::execute(
        service,
        ImportVerifiedCredentialCommand {
            profile_id: PROFILE.to_owned(),
            signed_bytes: NEW_SIGNED_BYTES.to_vec(),
            detached_proof: None,
            private_material: None,
        },
    ))
}

#[test]
fn verifier_failure_never_attempts_repository_upsert() {
    let repository = Arc::new(RecordingRepository::empty());
    let service = service(
        repository.clone(),
        Arc::new(StaticVerifier(Err(
            CredentialVerificationError::InvalidCredential,
        ))),
    );

    let error = import(&service).expect_err("verification must fail closed");

    assert_eq!(
        error,
        CredentialOperationError::Verification(CredentialVerificationError::InvalidCredential)
    );
    assert!(repository.calls().is_empty());
    assert!(repository.stored_record().is_none());
    assert!(!format!("{error:?}").contains("new-signed-credential-secret"));
}

#[test]
fn repository_upsert_failure_is_redacted_and_preserves_prior_same_key_record() {
    let repository = Arc::new(RecordingRepository::with_prior(
        prior_record(),
        CredentialRepositoryError::Integrity,
    ));
    let service = service(
        repository.clone(),
        Arc::new(StaticVerifier(Ok(valid_inspection(
            "Replacement credential",
        )))),
    );

    let error = import(&service).expect_err("persistence must fail closed");

    assert_eq!(
        error,
        CredentialOperationError::Persistence(CredentialRepositoryError::Integrity)
    );
    assert_eq!(
        repository.calls(),
        vec![RepositoryCall::Upsert(
            PROFILE.to_owned(),
            CREDENTIAL.to_owned()
        )]
    );
    let preserved = repository.stored_record().expect("prior record remains");
    assert_eq!(preserved.profile_id().as_str(), PROFILE);
    assert_eq!(preserved.id().as_str(), CREDENTIAL);
    assert_eq!(preserved.signed_bytes(), PRIOR_SIGNED_BYTES);
    assert_eq!(preserved.metadata().display_name(), "Prior credential");
    let diagnostic = format!("{error:?}");
    assert!(!diagnostic.contains("new-signed-credential-secret"));
    assert!(!diagnostic.contains("prior-signed-credential-secret"));
}

#[test]
fn repository_failures_map_without_losing_requested_profile_scope() {
    let repository = Arc::new(RecordingRepository::failing_reads(
        CredentialRepositoryError::Unavailable,
    ));
    let service = service(
        repository.clone(),
        Arc::new(StaticVerifier(Ok(valid_inspection("Unused")))),
    );

    assert_eq!(
        ListCredentialsUseCase::execute(
            &service,
            CredentialProfileQuery {
                profile_id: PROFILE.to_owned()
            }
        ),
        Err(CredentialOperationError::Persistence(
            CredentialRepositoryError::Unavailable
        ))
    );
    assert_eq!(
        GetCredentialUseCase::execute(
            &service,
            CredentialQuery {
                profile_id: PROFILE.to_owned(),
                credential_id: CREDENTIAL.to_owned(),
            }
        ),
        Err(CredentialOperationError::Persistence(
            CredentialRepositoryError::Unavailable
        ))
    );
    assert_eq!(
        DeleteCredentialUseCase::execute(
            &service,
            DeleteCredentialCommand {
                profile_id: PROFILE.to_owned(),
                credential_id: CREDENTIAL.to_owned(),
                confirmed: true,
                intent: "DELETE_CREDENTIAL".to_owned(),
            }
        ),
        Err(CredentialOperationError::Persistence(
            CredentialRepositoryError::Unavailable
        ))
    );
    assert_eq!(
        repository.calls(),
        vec![
            RepositoryCall::List(PROFILE.to_owned()),
            RepositoryCall::Get(PROFILE.to_owned(), CREDENTIAL.to_owned()),
            RepositoryCall::Remove(PROFILE.to_owned(), CREDENTIAL.to_owned()),
        ]
    );
}

fn poll<T>(mut future: Pin<Box<dyn Future<Output = T> + Send + '_>>) -> T {
    use std::task::{Context, Poll, Waker};
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture future must be ready"),
    }
}
