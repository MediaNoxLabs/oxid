// SPDX-License-Identifier: Apache-2.0

//! Composition bridge for a native, ledger-backed Midnight DID deployment.
//!
//! This service is intentionally not exposed through the legacy synchronous
//! `CreateDidUseCase` yet. It first proves the durable headless path and stays
//! fail-closed until the post-deploy verification methods and relationships are
//! independently resolvable.

use std::{error::Error, fmt, future::Future, io, pin::Pin, sync::Arc};

use oxid_adapter_did_midnight::{
    MidnightDidBootstrapCall, MidnightDidBootstrapCircuit, MidnightDidCallCompositionPort,
    MidnightDidCallContext, MidnightDidCompactArtifacts, NativeMidnightDidCallComposer,
    NativeMidnightDidDeploymentComposer, NativeMidnightDidDeploymentRequest,
    NativeMidnightDidMaintenanceComposer, NativeMidnightDidMaintenanceRequest,
};
use oxid_adapter_midnight::{
    MidnightContractCallFundingPort, MidnightContractCallFundingRequest,
    MidnightContractCallSubmissionPort, MidnightContractCallSubmissionRequest,
    MidnightContractCallSubmissionState, MidnightPublicCallContextSource,
    MidnightRemoteProvingMaterialSource, MidnightStandaloneConfig,
};
use oxid_adapter_passport_vault::{
    NodeAnchoredPassportVaultStateSource, PassportVaultCallChainContextSource,
};
use oxid_foundation::UnixTimestampMillis;
use oxid_identity_application::{
    DeployDidCommand, DeployDidFuture, DeployDidUseCase, DidDeploymentEffect, DidDeploymentFailure,
    DidDeploymentOperation, DidDeploymentOperationError, DidDeploymentOperationId,
    DidDeploymentOperationRepository, DidDeploymentState, DidDeploymentUseCaseError, DidResolution,
    DidResolutionPort, DidResolutionPortError, DidResolutionSource, IdentityProfileId, JwkCurve,
    MidnightDid, MidnightNetwork, VerificationRelationship,
};
use oxid_passport_vault_application::{
    PassportVaultContractStateSourceError, PassportVaultContractStateSourcePort,
};
use oxid_platform_ports::{ClockPort, RandomPort};
use oxid_wallet_application::{
    WalletDerivedSecretUsePort, WalletKeyOperationPort, WalletProfileId, WalletTransactionPortError,
};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const DEPLOYMENT_TTL_MILLIS: u64 = 60 * 60 * 1_000;
const DID_CALL_COMPOSER_ENV: &str = "OXID_MIDNIGHT_DID_CALL_COMPOSER";

struct NativeDidProvingMaterialSource {
    artifacts: MidnightDidCompactArtifacts,
}

impl MidnightRemoteProvingMaterialSource for NativeDidProvingMaterialSource {
    fn resolve_key(
        &self,
        key_location: &str,
    ) -> io::Result<Option<midnight_transient_crypto::proofs::ProvingKeyMaterial>> {
        let circuit = match key_location {
            "setVerificationMethod" => MidnightDidBootstrapCircuit::VerificationMethod,
            "setSchnorrJubjubVerificationMethod" => {
                MidnightDidBootstrapCircuit::SchnorrJubjubVerificationMethod
            }
            "setVerificationMethodRelation" => {
                MidnightDidBootstrapCircuit::VerificationMethodRelation
            }
            _ => return Ok(None),
        };
        self.artifacts
            .proving_key_material(circuit)
            .map(Some)
            .map_err(|_| io::Error::other("DID proving artifact authentication failed"))
    }
}

pub(super) fn native_did_proving_material() -> Option<Arc<dyn MidnightRemoteProvingMaterialSource>>
{
    let artifacts = std::env::var_os("OXID_MIDNIGHT_DID_ARTIFACTS_DIR")
        .and_then(|root| MidnightDidCompactArtifacts::load(root).ok())?;
    Some(Arc::new(NativeDidProvingMaterialSource { artifacts }))
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn with_native_did_deployment<K, M>(
    mut services: super::services::ApplicationServices,
    config: &MidnightStandaloneConfig,
    security: Arc<K>,
    midnight: Arc<M>,
) -> super::services::ApplicationServices
where
    K: WalletDerivedSecretUsePort + WalletKeyOperationPort + 'static,
    M: MidnightPublicCallContextSource
        + MidnightContractCallFundingPort
        + MidnightContractCallSubmissionPort
        + 'static,
{
    let Some(executable) = std::env::var_os(DID_CALL_COMPOSER_ENV) else {
        return services;
    };
    let wallet: Arc<dyn MidnightPublicCallContextSource> = midnight.clone();
    let contexts = match NodeAnchoredDidDeploymentContextSource::new(config, wallet) {
        Ok(contexts) => Arc::new(contexts) as Arc<dyn DidDeploymentContextSource>,
        Err(_) => return services,
    };
    let custody: Arc<dyn WalletDerivedSecretUsePort> = security.clone();
    let keys: Arc<dyn WalletKeyOperationPort> = security;
    let calls = match NativeMidnightDidCallComposer::new(executable, Arc::clone(&custody), keys) {
        Ok(calls) => Arc::new(calls) as Arc<dyn MidnightDidCallCompositionPort>,
        Err(_) => return services,
    };
    let funding: Arc<dyn MidnightContractCallFundingPort> = midnight.clone();
    let submission: Arc<dyn MidnightContractCallSubmissionPort> = midnight;
    services.deploy_did = Arc::new(NativeDidDeploymentService::new(
        Arc::new(NativeMidnightDidDeploymentComposer::new(Arc::clone(
            &custody,
        ))),
        Arc::new(NativeMidnightDidMaintenanceComposer::new(custody)),
        calls,
        contexts,
        funding,
        submission,
        Arc::clone(&services.did_resolution_port),
        super::identity::headless_did_deployment_repository(),
        Arc::new(oxid_adapter_platform_system::SystemClock),
        Arc::new(oxid_adapter_platform_system::OsRandom),
    ));
    services
}

pub(super) struct NativeDidDeploymentService {
    effects: Arc<dyn DidDeploymentEffectComposer>,
    funding: Arc<dyn MidnightContractCallFundingPort>,
    submission: Arc<dyn MidnightContractCallSubmissionPort>,
    resolver: Arc<dyn DidResolutionPort>,
    operations: Arc<dyn DidDeploymentOperationRepository>,
    clock: Arc<dyn ClockPort>,
    random: Arc<dyn RandomPort>,
}

struct DidDeploymentEffectPlan {
    did: Option<MidnightDid>,
    profile_id: String,
    network_id: String,
    expires_at_seconds: u64,
    planning_fingerprint: [u8; 32],
    transaction: Zeroizing<Vec<u8>>,
}

type DidDeploymentEffectFuture<'a> = Pin<
    Box<dyn Future<Output = Result<DidDeploymentEffectPlan, NativeDidDeploymentError>> + Send + 'a>,
>;

trait DidDeploymentEffectComposer: Send + Sync {
    fn compose<'a>(
        &'a self,
        operation: &'a DidDeploymentOperation,
        account_index: u32,
    ) -> DidDeploymentEffectFuture<'a>;
}

type DidDeploymentContextFuture<'a> = Pin<
    Box<dyn Future<Output = Result<MidnightDidCallContext, NativeDidDeploymentError>> + Send + 'a>,
>;

pub(super) trait DidDeploymentContextSource: Send + Sync {
    fn context<'a>(
        &'a self,
        operation: &'a DidDeploymentOperation,
    ) -> DidDeploymentContextFuture<'a>;
}

struct NodeAnchoredDidDeploymentContextSource {
    chain: Arc<NodeAnchoredPassportVaultStateSource>,
    wallet: Arc<dyn MidnightPublicCallContextSource>,
}

impl NodeAnchoredDidDeploymentContextSource {
    fn new(
        config: &MidnightStandaloneConfig,
        wallet: Arc<dyn MidnightPublicCallContextSource>,
    ) -> Result<Self, NativeDidDeploymentError> {
        let chain = NodeAnchoredPassportVaultStateSource::new(
            config.indexer_http_url(),
            config.node_websocket_url(),
        )
        .map_err(|_| NativeDidDeploymentError::Unavailable)?;
        Ok(Self {
            chain: Arc::new(chain),
            wallet,
        })
    }
}

impl DidDeploymentContextSource for NodeAnchoredDidDeploymentContextSource {
    fn context<'a>(
        &'a self,
        operation: &'a DidDeploymentOperation,
    ) -> DidDeploymentContextFuture<'a> {
        Box::pin(async move {
            let did = operation.did().ok_or(NativeDidDeploymentError::Integrity)?;
            let address = hex::encode(did_contract_address(did)?);
            let receipt = operation
                .receipts()
                .last()
                .ok_or(NativeDidDeploymentError::Integrity)?;
            let snapshot = self
                .chain
                .read(&address)
                .await
                .map_err(map_contract_state_error)?;
            // The submission receipt identifies the enclosing Substrate extrinsic,
            // while the indexer exposes the embedded Midnight transaction hash.
            // Those are intentionally different hash domains. Bind the indexed
            // contract action to the receipt's exact finalized block and contract
            // address before composing the next effect.
            if snapshot.contract_address_hex != address
                || snapshot.action_block_hash_hex != receipt.block_hash_hex()
                || snapshot.action_block_height != receipt.block_height()
            {
                return Err(NativeDidDeploymentError::IndexerPending);
            }
            let chain = self
                .chain
                .chain_context(&snapshot)
                .map_err(|_| NativeDidDeploymentError::IndexerPending)?;
            let wallet = self
                .wallet
                .public_call_context(operation.profile_id().as_str())
                .map_err(|_| NativeDidDeploymentError::Unavailable)?;
            if wallet.network_id().as_str() != operation.network().as_str() {
                return Err(NativeDidDeploymentError::Integrity);
            }
            let timestamp_millis = snapshot
                .finalized_head_time_seconds
                .checked_mul(1_000)
                .ok_or(NativeDidDeploymentError::Integrity)?;
            Ok(MidnightDidCallContext {
                contract_state: snapshot.serialized_contract_state,
                contract_address: did_contract_address(did)?,
                zswap_chain_state: Some(chain.zswap_chain_state().to_vec()),
                ledger_parameters: Some(chain.ledger_parameters().to_vec()),
                network_id: wallet.network_id().as_str().to_owned(),
                timestamp_millis,
                coin_public_key: wallet.coin_public_key(),
                encryption_public_key: wallet.encryption_public_key(),
            })
        })
    }
}

struct NativeDidDeploymentEffects {
    deployment: Arc<NativeMidnightDidDeploymentComposer>,
    maintenance: Arc<NativeMidnightDidMaintenanceComposer>,
    calls: Arc<dyn MidnightDidCallCompositionPort>,
    contexts: Arc<dyn DidDeploymentContextSource>,
    artifacts: Option<MidnightDidCompactArtifacts>,
}

impl NativeDidDeploymentEffects {
    fn new(
        deployment: Arc<NativeMidnightDidDeploymentComposer>,
        maintenance: Arc<NativeMidnightDidMaintenanceComposer>,
        calls: Arc<dyn MidnightDidCallCompositionPort>,
        contexts: Arc<dyn DidDeploymentContextSource>,
    ) -> Self {
        let artifacts = std::env::var_os("OXID_MIDNIGHT_DID_ARTIFACTS_DIR")
            .and_then(|root| MidnightDidCompactArtifacts::load(root).ok());
        Self {
            deployment,
            maintenance,
            calls,
            contexts,
            artifacts,
        }
    }
}

impl DidDeploymentEffectComposer for NativeDidDeploymentEffects {
    fn compose<'a>(
        &'a self,
        operation: &'a DidDeploymentOperation,
        account_index: u32,
    ) -> DidDeploymentEffectFuture<'a> {
        Box::pin(async move {
            if operation.effect() == DidDeploymentEffect::DeployContract {
                let request = deployment_request(operation, account_index)?;
                let plan = self.deployment.compose(&request)?;
                return Ok(DidDeploymentEffectPlan {
                    did: Some(plan.did().clone()),
                    profile_id: plan.profile_id().to_owned(),
                    network_id: plan.network_id().to_owned(),
                    expires_at_seconds: plan.expires_at_seconds(),
                    planning_fingerprint: plan.planning_fingerprint(),
                    transaction: plan.into_transaction(),
                });
            }
            let context = self.contexts.context(operation).await?;
            let profile_id = WalletProfileId::parse(operation.profile_id().as_str().to_owned())
                .map_err(|_| NativeDidDeploymentError::InvalidRequest)?;
            let call = match operation.effect() {
                DidDeploymentEffect::AddAuthenticationMethod => {
                    Some(MidnightDidBootstrapCall::AddAuthenticationMethod)
                }
                DidDeploymentEffect::AddAuthenticationRelationship => {
                    Some(MidnightDidBootstrapCall::AddAuthenticationRelationship)
                }
                DidDeploymentEffect::AddAssertionMethod => {
                    Some(MidnightDidBootstrapCall::AddAssertionMethod)
                }
                DidDeploymentEffect::AddAssertionRelationship => {
                    Some(MidnightDidBootstrapCall::AddAssertionRelationship)
                }
                _ => None,
            };
            if let Some(call) = call {
                let plan = self
                    .calls
                    .compose_bootstrap_call(
                        profile_id,
                        account_index,
                        controller_index(operation)?,
                        operation.operation_id().as_str().to_owned(),
                        context,
                        call,
                    )
                    .await?;
                return Ok(DidDeploymentEffectPlan {
                    did: None,
                    profile_id: operation.profile_id().as_str().to_owned(),
                    network_id: operation.network().as_str().to_owned(),
                    expires_at_seconds: request_expires_at(operation)?.value() / 1_000,
                    planning_fingerprint: plan.planning_fingerprint,
                    transaction: plan.transaction,
                });
            }
            let (circuit, counter) = match operation.effect() {
                DidDeploymentEffect::InstallVerificationMethodVerifier => {
                    (MidnightDidBootstrapCircuit::VerificationMethod, 0)
                }
                DidDeploymentEffect::InstallJubjubVerifier => (
                    MidnightDidBootstrapCircuit::SchnorrJubjubVerificationMethod,
                    1,
                ),
                DidDeploymentEffect::InstallRelationshipVerifier => {
                    (MidnightDidBootstrapCircuit::VerificationMethodRelation, 2)
                }
                DidDeploymentEffect::ResolveDocument => {
                    return Err(NativeDidDeploymentError::Unavailable);
                }
                DidDeploymentEffect::AddAuthenticationMethod
                | DidDeploymentEffect::AddAuthenticationRelationship
                | DidDeploymentEffect::AddAssertionMethod
                | DidDeploymentEffect::AddAssertionRelationship => unreachable!(),
                DidDeploymentEffect::DeployContract => unreachable!(),
            };
            let artifacts = self
                .artifacts
                .as_ref()
                .ok_or(NativeDidDeploymentError::Unavailable)?;
            let did = operation.did().ok_or(NativeDidDeploymentError::Integrity)?;
            let address = did_contract_address(did)?;
            let request = NativeMidnightDidMaintenanceRequest::new(
                profile_id,
                operation.network().as_str(),
                account_index,
                address,
                circuit.id(),
                artifacts
                    .verifier_key(circuit)
                    .map_err(|_| NativeDidDeploymentError::Unavailable)?,
                counter,
                request_expires_at(operation)?.value(),
                effect_recipe(operation, b"maintenance"),
            )?;
            let plan = self.maintenance.compose(&request)?;
            Ok(DidDeploymentEffectPlan {
                did: None,
                profile_id: plan.profile_id().to_owned(),
                network_id: plan.network_id().to_owned(),
                expires_at_seconds: plan.expires_at_seconds(),
                planning_fingerprint: plan.planning_fingerprint(),
                transaction: plan.into_transaction(),
            })
        })
    }
}

impl NativeDidDeploymentService {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        deployment: Arc<NativeMidnightDidDeploymentComposer>,
        maintenance: Arc<NativeMidnightDidMaintenanceComposer>,
        calls: Arc<dyn MidnightDidCallCompositionPort>,
        contexts: Arc<dyn DidDeploymentContextSource>,
        funding: Arc<dyn MidnightContractCallFundingPort>,
        submission: Arc<dyn MidnightContractCallSubmissionPort>,
        resolver: Arc<dyn DidResolutionPort>,
        operations: Arc<dyn DidDeploymentOperationRepository>,
        clock: Arc<dyn ClockPort>,
        random: Arc<dyn RandomPort>,
    ) -> Self {
        Self {
            effects: Arc::new(NativeDidDeploymentEffects::new(
                deployment,
                maintenance,
                calls,
                contexts,
            )),
            funding,
            submission,
            resolver,
            operations,
            clock,
            random,
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn with_effects(
        effects: Arc<dyn DidDeploymentEffectComposer>,
        funding: Arc<dyn MidnightContractCallFundingPort>,
        submission: Arc<dyn MidnightContractCallSubmissionPort>,
        resolver: Arc<dyn DidResolutionPort>,
        operations: Arc<dyn DidDeploymentOperationRepository>,
        clock: Arc<dyn ClockPort>,
        random: Arc<dyn RandomPort>,
    ) -> Self {
        Self {
            effects,
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

        for _ in 0..16 {
            match operation.state() {
                DidDeploymentState::Ready => return Ok(operation),
                DidDeploymentState::Resolving => return self.resolve(operation).await,
                DidDeploymentState::OutcomeUnknown => {
                    operation = self.reconcile(operation)?;
                    if operation.state() == DidDeploymentState::OutcomeUnknown {
                        return Ok(operation);
                    }
                    continue;
                }
                DidDeploymentState::RetryableFailure => {
                    let resume = operation
                        .resume_from()
                        .ok_or(NativeDidDeploymentError::Integrity)?;
                    operation = operation.transition(resume, self.now()?)?;
                    self.operations.upsert(operation.clone())?;
                    continue;
                }
                DidDeploymentState::Composing
                | DidDeploymentState::Funding
                | DidDeploymentState::Proving
                | DidDeploymentState::Submitting
                | DidDeploymentState::Confirming => {}
            }

            let plan = match self.effects.compose(&operation, account_index).await {
                Ok(plan) => plan,
                Err(NativeDidDeploymentError::IndexerPending) => return Ok(operation),
                Err(NativeDidDeploymentError::Unavailable) => {
                    return self.pause(operation, DidDeploymentFailure::CompositionUnavailable);
                }
                Err(error) => return Err(error),
            };
            let draft_id = effect_submission_id(&operation);
            if operation.state() == DidDeploymentState::Composing {
                operation = if operation.effect() == DidDeploymentEffect::DeployContract {
                    operation.composed(
                        plan.did
                            .clone()
                            .ok_or(NativeDidDeploymentError::Integrity)?,
                        draft_id.clone(),
                        self.now()?,
                    )?
                } else {
                    if plan.did.is_some() {
                        return Err(NativeDidDeploymentError::Integrity);
                    }
                    operation.prepared_effect(draft_id.clone(), self.now()?)?
                };
                self.operations.upsert(operation.clone())?;
            } else if operation.submission_id() != Some(&draft_id)
                || (operation.effect() == DidDeploymentEffect::DeployContract
                    && operation.did() != plan.did.as_ref())
            {
                return Err(NativeDidDeploymentError::Integrity);
            }

            let funded =
                match self
                    .funding
                    .fund_contract_call(MidnightContractCallFundingRequest::new(
                        plan.profile_id,
                        plan.network_id,
                        plan.expires_at_seconds,
                        false,
                        plan.transaction,
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
                    plan.planning_fingerprint,
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
        }
        Err(NativeDidDeploymentError::Integrity)
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
    let nonce_recipe = derive_public_recipe(b"oxid:did-deploy:nonce:v1", id);
    let intent_recipe = derive_public_recipe(b"oxid:did-deploy:rng:v1", id);
    NativeMidnightDidDeploymentRequest::new(
        WalletProfileId::parse(operation.profile_id().as_str().to_owned())
            .map_err(|_| NativeDidDeploymentError::InvalidRequest)?,
        operation.network(),
        operation.network().as_str(),
        account_index,
        controller_index(operation)?,
        operation.created_at().value(),
        request_expires_at(operation)?.value(),
        nonce_recipe,
        intent_recipe,
    )
    .map_err(|_| NativeDidDeploymentError::Lifecycle)
}

fn controller_index(operation: &DidDeploymentOperation) -> Result<u32, NativeDidDeploymentError> {
    let digest = derive_public_recipe(
        b"oxid:did-controller:index:v1",
        operation.operation_id().as_str().as_bytes(),
    );
    Ok(u32::from_be_bytes(
        digest[..4]
            .try_into()
            .map_err(|_| NativeDidDeploymentError::Integrity)?,
    ) & oxid_wallet_application::WalletHdPathComponent::MAX_INDEX)
}

fn did_contract_address(did: &MidnightDid) -> Result<[u8; 32], NativeDidDeploymentError> {
    let encoded = did
        .as_str()
        .rsplit(':')
        .next()
        .ok_or(NativeDidDeploymentError::Integrity)?;
    let decoded = hex::decode(encoded).map_err(|_| NativeDidDeploymentError::Integrity)?;
    decoded
        .try_into()
        .map_err(|_| NativeDidDeploymentError::Integrity)
}

fn effect_submission_id(operation: &DidDeploymentOperation) -> String {
    format!(
        "{}-{}",
        operation.operation_id().as_str(),
        operation.effect().as_str()
    )
}

fn effect_recipe(operation: &DidDeploymentOperation, domain: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"oxid:did-deployment:effect:v1");
    hasher.update(domain);
    hasher.update(operation.operation_id().as_str().as_bytes());
    hasher.update(operation.effect().as_str().as_bytes());
    hasher.finalize().into()
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

const fn map_contract_state_error(
    error: PassportVaultContractStateSourceError,
) -> NativeDidDeploymentError {
    match error {
        PassportVaultContractStateSourceError::NotFound
        | PassportVaultContractStateSourceError::Unavailable => {
            NativeDidDeploymentError::IndexerPending
        }
        PassportVaultContractStateSourceError::InvalidConfiguration
        | PassportVaultContractStateSourceError::InvalidAddress
        | PassportVaultContractStateSourceError::CapacityExceeded
        | PassportVaultContractStateSourceError::InvalidResponse
        | PassportVaultContractStateSourceError::FinalityMismatch => {
            NativeDidDeploymentError::Integrity
        }
    }
}

fn required_holder_methods_resolve(resolution: &DidResolution) -> bool {
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
                && ids.iter().any(|id| {
                    entry.method_ids().iter().any(|candidate| {
                        candidate == id
                            || candidate.strip_prefix('#').is_some_and(|fragment| {
                                *id == format!("{}#{fragment}", document.id().as_str())
                            })
                    })
                })
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
    IndexerPending,
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
            Self::IndexerPending => "native DID deployment is waiting for indexed finality",
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
            NativeDidDeploymentError::IndexerPending => Self::Unavailable,
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
