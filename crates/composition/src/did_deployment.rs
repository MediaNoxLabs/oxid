// SPDX-License-Identifier: Apache-2.0

//! Composition bridge for a native, ledger-backed Midnight DID deployment.
//!
//! This service is intentionally not exposed through the legacy synchronous
//! `CreateDidUseCase` yet. It first proves the durable headless path and stays
//! fail-closed until the post-deploy verification methods and relationships are
//! independently resolvable.

use std::{error::Error, fmt, sync::Arc};

use oxid_adapter_did_midnight::{
    NativeMidnightDidDeploymentComposer, NativeMidnightDidDeploymentRequest,
};
use oxid_adapter_midnight::{
    MidnightContractCallFundingPort, MidnightContractCallFundingRequest,
    MidnightContractCallSubmissionPort, MidnightContractCallSubmissionRequest,
    MidnightContractCallSubmissionState,
};
use oxid_foundation::UnixTimestampMillis;
use oxid_identity_application::{
    DeployDidCommand, DeployDidFuture, DeployDidUseCase, DidDeploymentFailure,
    DidDeploymentOperation, DidDeploymentOperationError, DidDeploymentOperationId,
    DidDeploymentOperationRepository, DidDeploymentState, DidDeploymentUseCaseError,
    DidResolutionPort, DidResolutionPortError,
};
use oxid_identity_domain::{
    DidResolutionSource, IdentityProfileId, JwkCurve, MidnightNetwork, VerificationRelationship,
};
use oxid_platform_ports::{ClockPort, RandomPort};
use oxid_wallet_application::WalletTransactionPortError;
use oxid_wallet_domain::WalletProfileId;
use sha2::{Digest, Sha256};

const DEPLOYMENT_TTL_MILLIS: u64 = 60 * 60 * 1_000;

pub(super) struct NativeDidDeploymentService {
    composer: Arc<NativeMidnightDidDeploymentComposer>,
    funding: Arc<dyn MidnightContractCallFundingPort>,
    submission: Arc<dyn MidnightContractCallSubmissionPort>,
    resolver: Arc<dyn DidResolutionPort>,
    operations: Arc<dyn DidDeploymentOperationRepository>,
    clock: Arc<dyn ClockPort>,
    random: Arc<dyn RandomPort>,
}

impl NativeDidDeploymentService {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        composer: Arc<NativeMidnightDidDeploymentComposer>,
        funding: Arc<dyn MidnightContractCallFundingPort>,
        submission: Arc<dyn MidnightContractCallSubmissionPort>,
        resolver: Arc<dyn DidResolutionPort>,
        operations: Arc<dyn DidDeploymentOperationRepository>,
        clock: Arc<dyn ClockPort>,
        random: Arc<dyn RandomPort>,
    ) -> Self {
        Self {
            composer,
            funding,
            submission,
            resolver,
            operations,
            clock,
            random,
        }
    }

    pub(super) async fn run(
        &self,
        profile_id: IdentityProfileId,
        network: MidnightNetwork,
        account_index: u32,
    ) -> Result<DidDeploymentOperation, NativeDidDeploymentError> {
        if network == MidnightNetwork::Offchain {
            return Err(NativeDidDeploymentError::InvalidRequest);
        }
        let mut operation = match self.operations.active(&profile_id, network)? {
            Some(operation) => operation,
            None => match self.operations.latest(&profile_id, network)? {
                Some(operation) if operation.state() == DidDeploymentState::Ready => {
                    return Ok(operation);
                }
                Some(_) => return Err(NativeDidDeploymentError::Integrity),
                None => self.new_operation(profile_id, network)?,
            },
        };

        if operation.state() == DidDeploymentState::Resolving {
            return self.resolve(operation).await;
        }
        if operation.state() == DidDeploymentState::Ready {
            return Ok(operation);
        }
        if operation.state() == DidDeploymentState::OutcomeUnknown {
            operation = self.reconcile(operation)?;
            if operation.state() == DidDeploymentState::Resolving {
                return self.resolve(operation).await;
            }
            if operation.state() == DidDeploymentState::OutcomeUnknown {
                return Ok(operation);
            }
        }
        if operation.state() == DidDeploymentState::RetryableFailure {
            let resume = operation
                .resume_from()
                .ok_or(NativeDidDeploymentError::Integrity)?;
            operation = operation.transition(resume, self.now()?)?;
            self.operations.upsert(operation.clone())?;
            if operation.state() == DidDeploymentState::Resolving {
                return self.resolve(operation).await;
            }
        }

        let request = deployment_request(&operation, account_index)?;
        let plan = self.composer.compose(&request)?;
        let draft_id = operation.operation_id().as_str().to_owned();
        if operation.state() == DidDeploymentState::Composing {
            operation = operation.composed(plan.did().clone(), draft_id.clone(), self.now()?)?;
            self.operations.upsert(operation.clone())?;
        } else if operation.did() != Some(plan.did())
            || operation.submission_id() != Some(&draft_id)
        {
            return Err(NativeDidDeploymentError::Integrity);
        }
        let planning_fingerprint = plan.planning_fingerprint();
        let expires_at_seconds = plan.expires_at_seconds();
        let profile = plan.profile_id().to_owned();
        let network_id = plan.network_id().to_owned();

        let funded = match self
            .funding
            .fund_contract_call(MidnightContractCallFundingRequest::new(
                profile,
                network_id,
                expires_at_seconds,
                false,
                plan.into_transaction(),
            )) {
            Ok(funded) => funded,
            Err(error) => return self.pause(operation, map_transaction_failure(error)),
        };

        operation = operation.transition(DidDeploymentState::Proving, self.now()?)?;
        self.operations.upsert(operation.clone())?;
        operation = operation.transition(DidDeploymentState::Submitting, self.now()?)?;
        self.operations.upsert(operation.clone())?;

        let outcome = match self.submission.complete_contract_call(
            MidnightContractCallSubmissionRequest::new(
                operation.profile_id().as_str(),
                operation.network().as_str(),
                draft_id,
                planning_fingerprint,
                request_expires_at(&operation)?,
                self.now()?,
                funded.into_transaction(),
            ),
        ) {
            Ok(outcome) => outcome,
            Err(WalletTransactionPortError::SubmissionOutcomeUnknown) => {
                let unknown = operation.outcome_unknown(self.now()?)?;
                self.operations.upsert(unknown.clone())?;
                return Ok(unknown);
            }
            Err(error) => return self.pause(operation, map_transaction_failure(error)),
        };
        operation = operation.transition(DidDeploymentState::Confirming, self.now()?)?;
        operation = operation.included(
            hex::encode(outcome.transaction_hash),
            hex::encode(outcome.block_hash),
            outcome.block_height,
            self.now()?,
        )?;
        self.operations.upsert(operation.clone())?;
        self.resolve(operation).await
    }

    fn new_operation(
        &self,
        profile_id: IdentityProfileId,
        network: MidnightNetwork,
    ) -> Result<DidDeploymentOperation, NativeDidDeploymentError> {
        let mut random = [0_u8; 16];
        self.random
            .fill_bytes(&mut random)
            .map_err(|_| NativeDidDeploymentError::Unavailable)?;
        let id = DidDeploymentOperationId::parse(format!("did-deployment-{}", hex::encode(random)))
            .map_err(|_| NativeDidDeploymentError::InvalidRequest)?;
        let operation = DidDeploymentOperation::new(id, profile_id, network, self.now()?)?;
        self.operations.upsert(operation.clone())?;
        Ok(operation)
    }

    fn reconcile(
        &self,
        operation: DidDeploymentOperation,
    ) -> Result<DidDeploymentOperation, NativeDidDeploymentError> {
        let draft_id = operation
            .submission_id()
            .ok_or(NativeDidDeploymentError::Integrity)?;
        let status = self
            .submission
            .reconcile_contract_call_submission(operation.profile_id().as_str(), draft_id)
            .map_err(|_| NativeDidDeploymentError::Transaction)?;
        match status.state {
            MidnightContractCallSubmissionState::Included => {
                let confirming =
                    operation.transition(DidDeploymentState::Confirming, self.now()?)?;
                let included = confirming.included(
                    hex::encode(
                        status
                            .transaction_hash
                            .ok_or(NativeDidDeploymentError::Integrity)?,
                    ),
                    hex::encode(
                        status
                            .block_hash
                            .ok_or(NativeDidDeploymentError::Integrity)?,
                    ),
                    status
                        .block_height
                        .ok_or(NativeDidDeploymentError::Integrity)?,
                    self.now()?,
                )?;
                self.operations.upsert(included.clone())?;
                Ok(included)
            }
            MidnightContractCallSubmissionState::Running
            | MidnightContractCallSubmissionState::CancellationRequested
            | MidnightContractCallSubmissionState::Broadcasting
            | MidnightContractCallSubmissionState::OutcomeUnknown => Ok(operation),
            MidnightContractCallSubmissionState::Rejected
            | MidnightContractCallSubmissionState::Expired => {
                self.pause(operation, DidDeploymentFailure::SubmissionRejected)
            }
        }
    }

    async fn resolve(
        &self,
        operation: DidDeploymentOperation,
    ) -> Result<DidDeploymentOperation, NativeDidDeploymentError> {
        let did = operation
            .did()
            .cloned()
            .ok_or(NativeDidDeploymentError::Integrity)?;
        let resolution = match self.resolver.resolve(&did).await {
            Ok(resolution) => resolution,
            Err(DidResolutionPortError::Unavailable | DidResolutionPortError::NotFound) => {
                return self.pause(operation, DidDeploymentFailure::ResolutionUnavailable);
            }
            Err(_) => return Err(NativeDidDeploymentError::Resolution),
        };
        if resolution.source() != DidResolutionSource::Live
            || resolution.document().id() != &did
            || !required_holder_methods_resolve(&resolution)
        {
            return self.pause(operation, DidDeploymentFailure::ResolutionMismatch);
        }
        let ready = operation.transition(DidDeploymentState::Ready, self.now()?)?;
        self.operations.upsert(ready.clone())?;
        Ok(ready)
    }

    fn pause(
        &self,
        operation: DidDeploymentOperation,
        failure: DidDeploymentFailure,
    ) -> Result<DidDeploymentOperation, NativeDidDeploymentError> {
        let paused = operation.retryable_failure(failure, self.now()?)?;
        self.operations.upsert(paused.clone())?;
        Ok(paused)
    }

    fn now(&self) -> Result<UnixTimestampMillis, NativeDidDeploymentError> {
        self.clock
            .now()
            .map_err(|_| NativeDidDeploymentError::Unavailable)
    }
}

impl DeployDidUseCase for NativeDidDeploymentService {
    fn execute<'a>(&'a self, command: DeployDidCommand) -> DeployDidFuture<'a> {
        Box::pin(async move {
            self.run(command.profile_id, command.network, command.account_index)
                .await
                .map_err(DidDeploymentUseCaseError::from)
        })
    }
}

fn deployment_request(
    operation: &DidDeploymentOperation,
    account_index: u32,
) -> Result<NativeMidnightDidDeploymentRequest, NativeDidDeploymentError> {
    let id = operation.operation_id().as_str().as_bytes();
    let nonce = derive_public_recipe(b"oxid:did-deploy:nonce:v1", id);
    let composition_seed = derive_public_recipe(b"oxid:did-deploy:rng:v1", id);
    let controller_digest = derive_public_recipe(b"oxid:did-controller:index:v1", id);
    let controller_index = u32::from_be_bytes(
        controller_digest[..4]
            .try_into()
            .map_err(|_| NativeDidDeploymentError::Integrity)?,
    ) & oxid_wallet_application::WalletHdPathComponent::MAX_INDEX;
    NativeMidnightDidDeploymentRequest::new(
        WalletProfileId::parse(operation.profile_id().as_str().to_owned())
            .map_err(|_| NativeDidDeploymentError::InvalidRequest)?,
        operation.network(),
        operation.network().as_str(),
        account_index,
        controller_index,
        operation.created_at(),
        request_expires_at(operation)?,
        nonce,
        composition_seed,
    )
    .map_err(|_| NativeDidDeploymentError::Lifecycle)
}

fn request_expires_at(
    operation: &DidDeploymentOperation,
) -> Result<UnixTimestampMillis, NativeDidDeploymentError> {
    operation
        .created_at()
        .value()
        .checked_add(DEPLOYMENT_TTL_MILLIS)
        .map(UnixTimestampMillis::new)
        .ok_or(NativeDidDeploymentError::InvalidRequest)
}

fn derive_public_recipe(domain: &[u8], operation_id: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(operation_id);
    hasher.finalize().into()
}

fn required_holder_methods_resolve(resolution: &oxid_identity_domain::DidResolution) -> bool {
    let document = resolution.document();
    let ed25519 = document
        .verification_methods()
        .iter()
        .filter(|method| method.public_key_jwk().curve() == JwkCurve::Ed25519)
        .map(|method| method.id())
        .collect::<Vec<_>>();
    let jubjub = document
        .verification_methods()
        .iter()
        .filter(|method| method.public_key_jwk().curve() == JwkCurve::Jubjub)
        .map(|method| method.id())
        .collect::<Vec<_>>();
    let relationship_contains = |relationship, ids: &[&str]| {
        document.relationships().iter().any(|entry| {
            entry.relationship() == relationship
                && ids
                    .iter()
                    .any(|id| entry.method_ids().iter().any(|candidate| candidate == id))
        })
    };
    !ed25519.is_empty()
        && !jubjub.is_empty()
        && relationship_contains(VerificationRelationship::Authentication, &ed25519)
        && relationship_contains(VerificationRelationship::AssertionMethod, &jubjub)
}

fn map_transaction_failure(error: WalletTransactionPortError) -> DidDeploymentFailure {
    match error {
        WalletTransactionPortError::ProtectionLocked
        | WalletTransactionPortError::ProtectionNotInitialized => {
            DidDeploymentFailure::ProtectionLocked
        }
        WalletTransactionPortError::InsufficientDust => DidDeploymentFailure::InsufficientDust,
        WalletTransactionPortError::ProvingFailed => DidDeploymentFailure::ProvingUnavailable,
        WalletTransactionPortError::SubmissionRejected
        | WalletTransactionPortError::DraftExpired => DidDeploymentFailure::SubmissionRejected,
        WalletTransactionPortError::AccountNotDerived
        | WalletTransactionPortError::AccountNotSynchronized
        | WalletTransactionPortError::ShieldedStateNotCurrent => {
            DidDeploymentFailure::AccountUnavailable
        }
        _ => DidDeploymentFailure::FundingUnavailable,
    }
}

#[derive(Debug)]
pub(super) enum NativeDidDeploymentError {
    Unavailable,
    InvalidRequest,
    Integrity,
    Operation,
    Lifecycle,
    Transaction,
    Resolution,
}

impl fmt::Display for NativeDidDeploymentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "native DID deployment dependencies are unavailable",
            Self::InvalidRequest => "native DID deployment request is invalid",
            Self::Integrity => "native DID deployment state failed integrity validation",
            Self::Operation => "native DID deployment operation persistence failed",
            Self::Lifecycle => "native DID deployment composition failed",
            Self::Transaction => "native DID deployment transaction failed",
            Self::Resolution => "native DID deployment resolution failed",
        })
    }
}

impl Error for NativeDidDeploymentError {}

impl From<DidDeploymentOperationError> for NativeDidDeploymentError {
    fn from(_: DidDeploymentOperationError) -> Self {
        Self::Operation
    }
}

impl From<oxid_identity_application::DidLifecyclePortError> for NativeDidDeploymentError {
    fn from(_: oxid_identity_application::DidLifecyclePortError) -> Self {
        Self::Lifecycle
    }
}

impl From<WalletTransactionPortError> for NativeDidDeploymentError {
    fn from(_: WalletTransactionPortError) -> Self {
        Self::Transaction
    }
}

impl From<NativeDidDeploymentError> for DidDeploymentUseCaseError {
    fn from(error: NativeDidDeploymentError) -> Self {
        match error {
            NativeDidDeploymentError::Unavailable => Self::Unavailable,
            NativeDidDeploymentError::InvalidRequest => Self::InvalidRequest,
            NativeDidDeploymentError::Integrity => Self::Integrity,
            NativeDidDeploymentError::Operation => Self::Persistence,
            NativeDidDeploymentError::Lifecycle => Self::Composition,
            NativeDidDeploymentError::Transaction => Self::Transaction,
            NativeDidDeploymentError::Resolution => Self::Resolution,
        }
    }
}

#[cfg(test)]
#[path = "did_deployment/tests.rs"]
mod tests;
