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
use oxid_wallet_application::{
    GenerateProtectedKeyRequest, WalletDerivedSecretUsePort, WalletHdPath, WalletKeyOperationPort,
    WalletSecurityPortError,
};
use oxid_wallet_domain::{
    WalletKeyDescriptor, WalletKeyReference, WalletProfileId, WalletSignature,
};
use zeroize::Zeroizing;

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

struct AllEffectsComposer;

struct IndexerPendingAfterDeploy;

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

impl WalletKeyOperationPort for TestCustody {
    fn generate(
        &self,
        _: &WalletProfileId,
        _: GenerateProtectedKeyRequest,
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }

    fn list(
        &self,
        _: &WalletProfileId,
    ) -> Result<Vec<WalletKeyDescriptor>, WalletSecurityPortError> {
        Ok(Vec::new())
    }

    fn sign(
        &self,
        _: &WalletProfileId,
        _: &WalletKeyReference,
        _: &[u8],
    ) -> Result<WalletSignature, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }

    fn delete(
        &self,
        _: &WalletProfileId,
        _: &WalletKeyReference,
    ) -> Result<(), WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }
}

struct ReadyContextSource;

impl DidDeploymentContextSource for ReadyContextSource {
    fn context<'a>(&'a self, _: &'a DidDeploymentOperation) -> DidDeploymentContextFuture<'a> {
        Box::pin(async {
            Ok(MidnightDidCallContext {
                contract_state: vec![1],
                contract_address: [2; 32],
                zswap_chain_state: Some(vec![3]),
                ledger_parameters: Some(vec![4]),
                network_id: "undeployed".to_owned(),
                timestamp_millis: 10_000,
                coin_public_key: [5; 32],
                encryption_public_key: [6; 32],
            })
        })
    }
}

impl DidDeploymentEffectComposer for AllEffectsComposer {
    fn compose<'a>(
        &'a self,
        operation: &'a DidDeploymentOperation,
        _: u32,
    ) -> DidDeploymentEffectFuture<'a> {
        Box::pin(async move {
            let did = (operation.effect() == DidDeploymentEffect::DeployContract).then(|| {
                MidnightDid::parse(format!("did:midnight:undeployed:{}", "a".repeat(64)))
                    .expect("did")
            });
            Ok(DidDeploymentEffectPlan {
                did,
                profile_id: operation.profile_id().as_str().to_owned(),
                network_id: operation.network().as_str().to_owned(),
                expires_at_seconds: 3_700,
                planning_fingerprint: Sha256::digest(operation.effect().as_str()).into(),
                transaction: Zeroizing::new(vec![1, 2, 3]),
            })
        })
    }
}

impl DidDeploymentEffectComposer for IndexerPendingAfterDeploy {
    fn compose<'a>(
        &'a self,
        operation: &'a DidDeploymentOperation,
        _: u32,
    ) -> DidDeploymentEffectFuture<'a> {
        Box::pin(async move {
            if operation.effect() != DidDeploymentEffect::DeployContract {
                return Err(NativeDidDeploymentError::IndexerPending);
            }
            Ok(DidDeploymentEffectPlan {
                did: Some(
                    MidnightDid::parse(format!("did:midnight:undeployed:{}", "a".repeat(64)))
                        .expect("did"),
                ),
                profile_id: operation.profile_id().as_str().to_owned(),
                network_id: operation.network().as_str().to_owned(),
                expires_at_seconds: 3_700,
                planning_fingerprint: Sha256::digest(operation.effect().as_str()).into(),
                transaction: Zeroizing::new(vec![1, 2, 3]),
            })
        })
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
        let attempt = self.submissions.fetch_add(1, Ordering::SeqCst);
        if attempt == 0 {
            return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
        }
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
    let mut methods = vec![authentication];
    let mut relationships = vec![VerificationRelationshipEntry::new(
        VerificationRelationship::Authentication,
        vec!["#authentication-1".to_owned()],
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
        methods.push(holder_binding);
        relationships.push(VerificationRelationshipEntry::new(
            VerificationRelationship::AssertionMethod,
            vec!["#holder-binding-1".to_owned()],
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
    NativeDidDeploymentService::with_effects(
        Arc::new(AllEffectsComposer),
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
fn native_effect_composer_authenticates_and_composes_the_first_maintenance_update() {
    if std::env::var_os("OXID_MIDNIGHT_DID_ARTIFACTS_DIR").is_none() {
        return;
    }
    let custody: Arc<dyn WalletDerivedSecretUsePort> = Arc::new(TestCustody);
    let keys: Arc<dyn WalletKeyOperationPort> = Arc::new(TestCustody);
    let executable = std::env::current_exe().expect("current executable");
    let effects = NativeDidDeploymentEffects::new(
        Arc::new(NativeMidnightDidDeploymentComposer::new(Arc::clone(
            &custody,
        ))),
        Arc::new(NativeMidnightDidMaintenanceComposer::new(custody)),
        Arc::new(
            NativeMidnightDidCallComposer::new(executable, Arc::new(TestCustody), keys)
                .expect("call composer"),
        ),
        Arc::new(ReadyContextSource),
    );
    let operation = DidDeploymentOperation::new(
        DidDeploymentOperationId::parse("deployment-native-effects").expect("operation id"),
        command().profile_id,
        MidnightNetwork::Undeployed,
        UnixTimestampMillis::new(10_000),
    )
    .expect("operation");
    let deploy =
        futures::executor::block_on(effects.compose(&operation, 0)).expect("deployment plan");
    let submission_id = effect_submission_id(&operation);
    let operation = operation
        .composed(
            deploy.did.expect("deployment DID"),
            submission_id,
            UnixTimestampMillis::new(10_000),
        )
        .expect("composed")
        .transition(
            DidDeploymentState::Proving,
            UnixTimestampMillis::new(10_000),
        )
        .expect("proving")
        .transition(
            DidDeploymentState::Submitting,
            UnixTimestampMillis::new(10_000),
        )
        .expect("submitting")
        .transition(
            DidDeploymentState::Confirming,
            UnixTimestampMillis::new(10_000),
        )
        .expect("confirming")
        .included(
            "1".repeat(64),
            "2".repeat(64),
            42,
            UnixTimestampMillis::new(10_000),
        )
        .expect("included deploy");
    assert_eq!(
        operation.effect(),
        DidDeploymentEffect::InstallVerificationMethodVerifier
    );
    let maintenance =
        futures::executor::block_on(effects.compose(&operation, 0)).expect("maintenance plan");
    assert!(maintenance.did.is_none());
    assert!(!maintenance.transaction.is_empty());
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
    assert_eq!(submissions.submissions.load(Ordering::SeqCst), 8);
}

#[test]
fn indexed_receipt_barrier_prevents_the_next_transaction_from_being_composed() {
    let submissions = Arc::new(IncludedSubmission {
        submissions: AtomicUsize::new(0),
    });
    let operations = Arc::new(MemoryOperations::default());
    let service = NativeDidDeploymentService::with_effects(
        Arc::new(IndexerPendingAfterDeploy),
        Arc::new(PassthroughFunding),
        Arc::clone(&submissions) as Arc<dyn MidnightContractCallSubmissionPort>,
        Arc::new(LiveResolver { complete: true }),
        operations,
        Arc::new(FixedClock),
        Arc::new(FixedRandom),
    );

    let operation =
        futures::executor::block_on(service.execute(command())).expect("durable deployment");

    assert_eq!(
        operation.effect(),
        DidDeploymentEffect::InstallVerificationMethodVerifier
    );
    assert_eq!(operation.state(), DidDeploymentState::Composing);
    assert_eq!(submissions.submissions.load(Ordering::SeqCst), 1);
    assert_eq!(operation.receipts().len(), 1);
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
    let service = NativeDidDeploymentService::with_effects(
        Arc::new(AllEffectsComposer),
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
    assert_eq!(submissions.submissions.load(Ordering::SeqCst), 8);
}
