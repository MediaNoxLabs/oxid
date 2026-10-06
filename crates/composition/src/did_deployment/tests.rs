// SPDX-License-Identifier: Apache-2.0

// Focused orchestration tests live here so production composition remains
// readable. Scenarios are added with the native bridge wiring in this slice.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use oxid_adapter_midnight::{
    FundedMidnightContractCall, MidnightContractCallFundingRequest,
    MidnightContractCallSubmissionMode, MidnightContractCallSubmissionOutcome,
    MidnightContractCallSubmissionRequest, MidnightContractCallSubmissionStatus,
};
use oxid_foundation::UnixTimestampMillis;
use oxid_identity_application::{
    DeployDidCommand, DeployDidUseCase, DidDeploymentOperationId, DidResolutionPortFuture,
};
use oxid_identity_domain::{
    DID_CONTEXT, DidDocument, DidDocumentMetadata, DidDocumentParts, DidResolution,
    DidResolutionMetadata, DidResolutionSource, IdentityProfileId, JWK_CONTEXT, JwkCurve,
    JwkKeyType, MidnightDid, PublicJwk, VerificationMethod, VerificationRelationship,
    VerificationRelationshipEntry,
};
use oxid_platform_ports::{PlatformError, RandomPort};
use oxid_wallet_application::{WalletDerivedSecretUsePort, WalletHdPath, WalletSecurityPortError};
use oxid_wallet_domain::WalletProfileId;

use super::*;

struct FixedClock;

impl ClockPort for FixedClock {
    fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
        Ok(UnixTimestampMillis::new(10_000))
    }
}

struct FixedRandom;

impl RandomPort for FixedRandom {
    fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
        destination.fill(0x2a);
        Ok(())
    }
}

struct TestCustody;

impl WalletDerivedSecretUsePort for TestCustody {
    fn use_derived_secret(
        &self,
        _: &WalletProfileId,
        _: &WalletHdPath,
        operation: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
    ) -> Result<(), WalletSecurityPortError> {
        operation(&[7; 32])
    }
}

#[derive(Default)]
struct MemoryOperations(Mutex<Vec<DidDeploymentOperation>>);

impl DidDeploymentOperationRepository for MemoryOperations {
    fn upsert(&self, operation: DidDeploymentOperation) -> Result<(), DidDeploymentOperationError> {
        let mut operations = self
            .0
            .lock()
            .map_err(|_| DidDeploymentOperationError::Unavailable)?;
        if let Some(current) = operations
            .iter_mut()
            .find(|candidate| candidate.operation_id() == operation.operation_id())
        {
            *current = operation;
        } else {
            operations.push(operation);
        }
        Ok(())
    }

    fn get(
        &self,
        operation_id: &DidDeploymentOperationId,
    ) -> Result<DidDeploymentOperation, DidDeploymentOperationError> {
        self.0
            .lock()
            .map_err(|_| DidDeploymentOperationError::Unavailable)?
            .iter()
            .find(|candidate| candidate.operation_id() == operation_id)
            .cloned()
            .ok_or(DidDeploymentOperationError::NotFound)
    }

    fn active(
        &self,
        profile_id: &IdentityProfileId,
        network: MidnightNetwork,
    ) -> Result<Option<DidDeploymentOperation>, DidDeploymentOperationError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| DidDeploymentOperationError::Unavailable)?
            .iter()
            .find(|candidate| {
                candidate.profile_id() == profile_id
                    && candidate.network() == network
                    && !candidate.state().terminal()
            })
            .cloned())
    }

    fn latest(
        &self,
        profile_id: &IdentityProfileId,
        network: MidnightNetwork,
    ) -> Result<Option<DidDeploymentOperation>, DidDeploymentOperationError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| DidDeploymentOperationError::Unavailable)?
            .iter()
            .filter(|candidate| {
                candidate.profile_id() == profile_id && candidate.network() == network
            })
            .max_by_key(|candidate| candidate.updated_at().value())
            .cloned())
    }
}

struct PassthroughFunding;

impl MidnightContractCallFundingPort for PassthroughFunding {
    fn fund_contract_call(
        &self,
        request: MidnightContractCallFundingRequest,
    ) -> Result<FundedMidnightContractCall, WalletTransactionPortError> {
        Ok(FundedMidnightContractCall::new(
            request.into_transaction(),
            0,
            0,
        ))
    }
}

struct IncludedSubmission {
    submissions: AtomicUsize,
}

struct UnknownThenIncludedSubmission {
    submissions: AtomicUsize,
}

impl IncludedSubmission {
    fn status(state: MidnightContractCallSubmissionState) -> MidnightContractCallSubmissionStatus {
        MidnightContractCallSubmissionStatus {
            draft_id: "did-deployment".to_owned(),
            state,
            transaction_hash: (state == MidnightContractCallSubmissionState::Included)
                .then_some([1; 32]),
            block_hash: (state == MidnightContractCallSubmissionState::Included).then_some([2; 32]),
            block_height: (state == MidnightContractCallSubmissionState::Included).then_some(42),
            fee_specks: Some(3),
            mode: Some(MidnightContractCallSubmissionMode::Live),
        }
    }
}

impl MidnightContractCallSubmissionPort for IncludedSubmission {
    fn complete_contract_call(
        &self,
        _: MidnightContractCallSubmissionRequest,
    ) -> Result<MidnightContractCallSubmissionOutcome, WalletTransactionPortError> {
        self.submissions.fetch_add(1, Ordering::SeqCst);
        Ok(MidnightContractCallSubmissionOutcome {
            transaction_hash: [1; 32],
            block_hash: [2; 32],
            block_height: 42,
            fee_specks: 3,
            mode: MidnightContractCallSubmissionMode::Live,
        })
    }

    fn contract_call_submission_status(
        &self,
        _: &str,
        _: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
        Ok(Self::status(MidnightContractCallSubmissionState::Included))
    }

    fn cancel_contract_call_submission(
        &self,
        _: &str,
        _: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
        Ok(Self::status(MidnightContractCallSubmissionState::Included))
    }

    fn contract_call_submission_history(
        &self,
        _: &str,
    ) -> Result<Vec<MidnightContractCallSubmissionStatus>, WalletTransactionPortError> {
        Ok(vec![Self::status(
            MidnightContractCallSubmissionState::Included,
        )])
    }

    fn reconcile_contract_call_submission(
        &self,
        _: &str,
        _: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
        Ok(Self::status(MidnightContractCallSubmissionState::Included))
    }
}

impl MidnightContractCallSubmissionPort for UnknownThenIncludedSubmission {
    fn complete_contract_call(
        &self,
        _: MidnightContractCallSubmissionRequest,
    ) -> Result<MidnightContractCallSubmissionOutcome, WalletTransactionPortError> {
        self.submissions.fetch_add(1, Ordering::SeqCst);
        Err(WalletTransactionPortError::SubmissionOutcomeUnknown)
    }

    fn contract_call_submission_status(
        &self,
        _: &str,
        _: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
        Ok(IncludedSubmission::status(
            MidnightContractCallSubmissionState::Included,
        ))
    }

    fn cancel_contract_call_submission(
        &self,
        _: &str,
        _: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
        self.contract_call_submission_status("", "")
    }

    fn contract_call_submission_history(
        &self,
        _: &str,
    ) -> Result<Vec<MidnightContractCallSubmissionStatus>, WalletTransactionPortError> {
        Ok(vec![IncludedSubmission::status(
            MidnightContractCallSubmissionState::Included,
        )])
    }

    fn reconcile_contract_call_submission(
        &self,
        _: &str,
        _: &str,
    ) -> Result<MidnightContractCallSubmissionStatus, WalletTransactionPortError> {
        Ok(IncludedSubmission::status(
            MidnightContractCallSubmissionState::Included,
        ))
    }
}

struct LiveResolver {
    complete: bool,
}

impl DidResolutionPort for LiveResolver {
    fn resolve<'a>(&'a self, did: &'a MidnightDid) -> DidResolutionPortFuture<'a> {
        let resolution = live_resolution(did.clone(), self.complete);
        Box::pin(async move { resolution })
    }
}

fn live_resolution(
    did: MidnightDid,
    complete: bool,
) -> Result<DidResolution, DidResolutionPortError> {
    let authentication = VerificationMethod::new(
        &did,
        "#authentication-1",
        did.clone(),
        PublicJwk::new(
            JwkKeyType::Okp,
            JwkCurve::Ed25519,
            "4A3l3ITUWOFUgNTdtN9BS3HEIpnEhewcfd_rEb3iSEo",
            None,
        )
        .map_err(|_| DidResolutionPortError::InvalidResponse)?,
    )
    .map_err(|_| DidResolutionPortError::InvalidResponse)?;
    let authentication_id = authentication.id().to_owned();
    let mut methods = vec![authentication];
    let mut relationships = vec![VerificationRelationshipEntry::new(
        VerificationRelationship::Authentication,
        vec![authentication_id],
    )];
    if complete {
        let holder_binding = VerificationMethod::new(
            &did,
            "#holder-binding-1",
            did.clone(),
            PublicJwk::new(
                JwkKeyType::Ec,
                JwkCurve::Jubjub,
                "r3S3KuAV2Y2wviagxqTsKNuUFmqHlVjfWwQvZaV_pQA",
                Some("b8GewrvMw5hldx4dBHZSAqBhYb_p7bVdcVqC2FU08mM".to_owned()),
            )
            .map_err(|_| DidResolutionPortError::InvalidResponse)?,
        )
        .map_err(|_| DidResolutionPortError::InvalidResponse)?;
        let holder_binding_id = holder_binding.id().to_owned();
        methods.push(holder_binding);
        relationships.push(VerificationRelationshipEntry::new(
            VerificationRelationship::AssertionMethod,
            vec![holder_binding_id],
        ));
    }
    let document = DidDocument::new(DidDocumentParts {
        contexts: vec![DID_CONTEXT.to_owned(), JWK_CONTEXT.to_owned()],
        id: did.clone(),
        controllers: vec![did],
        also_known_as: Vec::new(),
        verification_methods: methods,
        relationships,
        services: Vec::new(),
    })
    .map_err(|_| DidResolutionPortError::InvalidResponse)?;
    Ok(DidResolution::new(
        document,
        DidDocumentMetadata::default(),
        DidResolutionMetadata::default(),
        DidResolutionSource::Live,
    ))
}

fn service(
    resolver: Arc<dyn DidResolutionPort>,
    submissions: Arc<IncludedSubmission>,
    operations: Arc<MemoryOperations>,
) -> NativeDidDeploymentService {
    NativeDidDeploymentService::new(
        Arc::new(NativeMidnightDidDeploymentComposer::new(Arc::new(
            TestCustody,
        ))),
        Arc::new(PassthroughFunding),
        submissions,
        resolver,
        operations,
        Arc::new(FixedClock),
        Arc::new(FixedRandom),
    )
}

fn command() -> DeployDidCommand {
    DeployDidCommand {
        profile_id: IdentityProfileId::parse("profile-1").expect("profile"),
        network: MidnightNetwork::Undeployed,
        account_index: 0,
    }
}

#[test]
fn deployment_reaches_ready_and_repeated_requests_do_not_resubmit() {
    let submissions = Arc::new(IncludedSubmission {
        submissions: AtomicUsize::new(0),
    });
    let operations = Arc::new(MemoryOperations::default());
    let service = service(
        Arc::new(LiveResolver { complete: true }),
        Arc::clone(&submissions),
        operations,
    );

    let first = futures::executor::block_on(service.execute(command())).expect("first deployment");
    let second =
        futures::executor::block_on(service.execute(command())).expect("idempotent deployment");

    assert_eq!(first.state(), DidDeploymentState::Ready);
    assert_eq!(second.operation_id(), first.operation_id());
    assert_eq!(submissions.submissions.load(Ordering::SeqCst), 1);
}

#[test]
fn incomplete_live_resolution_pauses_without_exposing_the_did_as_ready() {
    let submissions = Arc::new(IncludedSubmission {
        submissions: AtomicUsize::new(0),
    });
    let operations = Arc::new(MemoryOperations::default());
    let service = service(
        Arc::new(LiveResolver { complete: false }),
        submissions,
        operations,
    );

    let operation =
        futures::executor::block_on(service.execute(command())).expect("durable failure snapshot");

    assert_eq!(operation.state(), DidDeploymentState::RetryableFailure);
    assert_eq!(
        operation.failure(),
        Some(DidDeploymentFailure::ResolutionMismatch)
    );
    assert_eq!(operation.resume_from(), Some(DidDeploymentState::Resolving));
}

#[test]
fn outcome_unknown_reconciles_without_a_second_submission() {
    let submissions = Arc::new(UnknownThenIncludedSubmission {
        submissions: AtomicUsize::new(0),
    });
    let operations = Arc::new(MemoryOperations::default());
    let service = NativeDidDeploymentService::new(
        Arc::new(NativeMidnightDidDeploymentComposer::new(Arc::new(
            TestCustody,
        ))),
        Arc::new(PassthroughFunding),
        submissions.clone(),
        Arc::new(LiveResolver { complete: true }),
        operations,
        Arc::new(FixedClock),
        Arc::new(FixedRandom),
    );

    let unknown =
        futures::executor::block_on(service.execute(command())).expect("unknown is durable");
    let reconciled =
        futures::executor::block_on(service.execute(command())).expect("reconciled deployment");

    assert_eq!(unknown.state(), DidDeploymentState::OutcomeUnknown);
    assert_eq!(reconciled.state(), DidDeploymentState::Ready);
    assert_eq!(reconciled.operation_id(), unknown.operation_id());
    assert_eq!(submissions.submissions.load(Ordering::SeqCst), 1);
}
