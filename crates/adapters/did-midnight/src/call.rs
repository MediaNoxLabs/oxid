// SPDX-License-Identifier: Apache-2.0

//! Native Rust composition for the four holder-DID bootstrap writes.
//!
//! The generated Compact runtime executes in-process. Controller and recovery
//! seeds are borrowed only by the custody signer; they never cross a DTO,
//! filesystem, environment, WebView, or child-process boundary.

use std::{
    borrow::Cow,
    future::Future,
    io::Cursor,
    pin::Pin,
    sync::{Arc, Mutex},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use midnight_base_crypto::{hash::HashOutput, schnorr::Signature, time::Timestamp};
use midnight_compact_runtime::{ContractAddress, Fr, WitnessContext};
use midnight_did_domain::did_document::{CurveType, KeyType, VerificationMethodType};
use midnight_did_jubjub_schnorr::{
    derive_public_key_from_seed, field_from_scalar, sign_authorization_digest_from_seed,
};
#[cfg(test)]
use midnight_did_runtime::LedgerDeploymentConfig;
use midnight_did_runtime::{
    BackendError, DidAuthorizationSigner, DidContractCall, DidContractExecutor,
    DidPrivateStateStore, GeneratedDidExecutor, JubjubPointHex, LedgerContractCallConfig,
    LedgerPublicKeyJwk, LedgerSchnorrJubjubVerificationMethod, LedgerVerificationMethod,
    LedgerVerificationMethodRelation, MapMutation, NewJubjubPointHex, SetMutation,
};
use midnight_ledger::{
    construct::SegmentSpecifier,
    structure::{LedgerParameters, ProofPreimageMarker, StandardTransaction, Transaction},
};
use midnight_serialize::{Deserializable as _, tagged_deserialize, tagged_serialize};
use midnight_storage::{DefaultDB, storage::HashMap as LedgerHashMap};
use midnight_transient_crypto::{
    commitment::PedersenRandomness,
    curve::EmbeddedGroupAffine,
    proofs::{KeyLocation, ProofPreimage, VerifierKey},
};
use oxid_identity_application::DidLifecyclePortError;
use oxid_wallet_application::{
    GenerateProtectedKeyRequest, WalletDerivedSecretUsePort, WalletHdPath, WalletHdPathComponent,
    WalletKeyOperationPort, WalletSecurityPortError,
};
use oxid_wallet_domain::{
    PublicKeyEncoding, WalletKeyAlgorithm, WalletKeyDescriptor, WalletKeyLabel, WalletKeyPurpose,
    WalletProfileId,
};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    compact_artifacts::{MidnightDidBootstrapCircuit, MidnightDidCompactArtifacts},
    custody::{controller_path, recovery_path, replay_randomness_path},
    protected_randomness::{ProtectedDeterministicRng, derive_seed},
};

const MAX_CONTRACT_STATE_BYTES: usize = 16 * 1024 * 1024;
const MAX_ZSWAP_STATE_BYTES: usize = 2 * 1024 * 1024;
const MAX_LEDGER_PARAMETERS_BYTES: usize = 512 * 1024;
const MAX_TRANSACTION_BYTES: usize = 32 * 1024 * 1024;
const CALL_COMMUNICATION_DOMAIN: &[u8] = b"oxid:did-call:communication-rng:v1";
const CALL_INTENT_DOMAIN: &[u8] = b"oxid:did-call:intent-rng:v1";

type UnprovenTransaction =
    Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightDidCallContext {
    pub contract_state: Vec<u8>,
    pub contract_address: [u8; 32],
    pub zswap_chain_state: Option<Vec<u8>>,
    pub ledger_parameters: Option<Vec<u8>>,
    pub network_id: String,
    pub timestamp_millis: u64,
    pub expires_at_millis: u64,
    pub coin_public_key: [u8; 32],
    pub encryption_public_key: [u8; 32],
}

impl MidnightDidCallContext {
    fn validate(&self) -> Result<(), DidLifecyclePortError> {
        if self.contract_state.is_empty()
            || self.contract_state.len() > MAX_CONTRACT_STATE_BYTES
            || self.contract_address == [0; 32]
            || self.network_id.is_empty()
            || self.network_id.len() > 64
            || !self
                .network_id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || self.timestamp_millis == 0
            || self.expires_at_millis <= self.timestamp_millis
            || self.expires_at_millis / 1_000 <= self.timestamp_millis / 1_000
            || self.coin_public_key == [0; 32]
            || self.encryption_public_key == [0; 32]
            || self
                .zswap_chain_state
                .as_ref()
                .is_some_and(|state| state.is_empty() || state.len() > MAX_ZSWAP_STATE_BYTES)
            || self.ledger_parameters.as_ref().is_none_or(|parameters| {
                parameters.is_empty() || parameters.len() > MAX_LEDGER_PARAMETERS_BYTES
            })
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MidnightDidCallOperation {
    AddAuthenticationMethod {
        method_id: String,
        x: [u8; 32],
    },
    AddAuthenticationRelationship {
        method_id: String,
    },
    AddAssertionMethod {
        method_id: String,
        x: [u8; 32],
        y: [u8; 32],
    },
    AddAssertionRelationship {
        method_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightDidBootstrapCall {
    AddAuthenticationMethod,
    AddAuthenticationRelationship,
    AddAssertionMethod,
    AddAssertionRelationship,
}

impl MidnightDidCallOperation {
    fn circuit(&self) -> MidnightDidBootstrapCircuit {
        match self {
            Self::AddAuthenticationMethod { .. } => MidnightDidBootstrapCircuit::VerificationMethod,
            Self::AddAssertionMethod { .. } => {
                MidnightDidBootstrapCircuit::SchnorrJubjubVerificationMethod
            }
            Self::AddAuthenticationRelationship { .. } | Self::AddAssertionRelationship { .. } => {
                MidnightDidBootstrapCircuit::VerificationMethodRelation
            }
        }
    }

    fn validate(&self) -> Result<(), DidLifecyclePortError> {
        let method_id = match self {
            Self::AddAuthenticationMethod { method_id, x } => {
                if *x == [0; 32] {
                    return Err(DidLifecyclePortError::InvalidOperation);
                }
                method_id
            }
            Self::AddAuthenticationRelationship { method_id }
            | Self::AddAssertionRelationship { method_id } => method_id,
            Self::AddAssertionMethod { method_id, x, y } => {
                if *x == [0; 32] || *y == [0; 32] {
                    return Err(DidLifecyclePortError::InvalidOperation);
                }
                method_id
            }
        };
        if !method_id.starts_with('#')
            || method_id.len() < 2
            || method_id.len() > 65
            || !method_id[1..].bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
            })
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        Ok(())
    }

    fn into_runtime(self) -> Result<DidContractCall, DidLifecyclePortError> {
        Ok(match self {
            Self::AddAuthenticationMethod { method_id, x } => {
                DidContractCall::SetVerificationMethod {
                    method: LedgerVerificationMethod {
                        id: method_id,
                        typ: VerificationMethodType::JsonWebKey,
                        public_key_jwk: LedgerPublicKeyJwk {
                            kty: KeyType::OKP,
                            crv: CurveType::Ed25519,
                            x: URL_SAFE_NO_PAD.encode(x),
                            y: String::new(),
                        },
                    },
                    mutation: MapMutation::Insert,
                }
            }
            Self::AddAuthenticationRelationship { method_id } => {
                DidContractCall::SetVerificationMethodRelation {
                    relation: LedgerVerificationMethodRelation::Authentication,
                    method_id,
                    mutation: SetMutation::Insert,
                }
            }
            Self::AddAssertionMethod { method_id, x, y } => {
                DidContractCall::SetSchnorrJubjubVerificationMethod {
                    method: LedgerSchnorrJubjubVerificationMethod {
                        id: method_id,
                        public_key: JubjubPointHex::new(NewJubjubPointHex {
                            x: hex::encode(x),
                            y: hex::encode(y),
                        })
                        .map_err(|_| DidLifecyclePortError::InvalidOperation)?,
                    },
                    mutation: MapMutation::Insert,
                }
            }
            Self::AddAssertionRelationship { method_id } => {
                DidContractCall::SetVerificationMethodRelation {
                    relation: LedgerVerificationMethodRelation::AssertionMethod,
                    method_id,
                    mutation: SetMutation::Insert,
                }
            }
        })
    }
}

pub struct NativeMidnightDidCallRequest {
    pub profile_id: WalletProfileId,
    pub account_index: u32,
    pub controller_index: u32,
    pub context: MidnightDidCallContext,
    pub operation: MidnightDidCallOperation,
    pub replay_recipe: [u8; 32],
}

pub struct NativeMidnightDidCallPlan {
    pub planning_fingerprint: [u8; 32],
    pub transaction: Zeroizing<Vec<u8>>,
}

pub type MidnightDidCallCompositionFuture<'a> = Pin<
    Box<dyn Future<Output = Result<NativeMidnightDidCallPlan, DidLifecyclePortError>> + Send + 'a>,
>;

pub trait MidnightDidCallCompositionPort: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn compose_bootstrap_call<'a>(
        &'a self,
        profile_id: WalletProfileId,
        account_index: u32,
        controller_index: u32,
        operation_scope: String,
        context: MidnightDidCallContext,
        call: MidnightDidBootstrapCall,
    ) -> MidnightDidCallCompositionFuture<'a>;
}

pub struct NativeMidnightDidCallComposer {
    custody: Arc<dyn WalletDerivedSecretUsePort>,
    keys: Arc<dyn WalletKeyOperationPort>,
    artifacts: MidnightDidCompactArtifacts,
}

impl NativeMidnightDidCallComposer {
    #[must_use]
    pub fn new(
        custody: Arc<dyn WalletDerivedSecretUsePort>,
        keys: Arc<dyn WalletKeyOperationPort>,
        artifacts: MidnightDidCompactArtifacts,
    ) -> Self {
        Self {
            custody,
            keys,
            artifacts,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn compose_bootstrap(
        &self,
        profile_id: WalletProfileId,
        account_index: u32,
        controller_index: u32,
        operation_scope: &str,
        context: MidnightDidCallContext,
        call: MidnightDidBootstrapCall,
    ) -> Result<NativeMidnightDidCallPlan, DidLifecyclePortError> {
        if operation_scope.is_empty() || operation_scope.len() > 256 {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        let authentication = matches!(
            call,
            MidnightDidBootstrapCall::AddAuthenticationMethod
                | MidnightDidBootstrapCall::AddAuthenticationRelationship
        )
        .then(|| {
            self.bootstrap_key(
                &profile_id,
                operation_scope,
                "authentication",
                WalletKeyAlgorithm::Ed25519,
                WalletKeyPurpose::Authentication,
                PublicKeyEncoding::Ed25519Compressed,
            )
        })
        .transpose()?;
        let assertion = matches!(
            call,
            MidnightDidBootstrapCall::AddAssertionMethod
                | MidnightDidBootstrapCall::AddAssertionRelationship
        )
        .then(|| {
            self.bootstrap_key(
                &profile_id,
                operation_scope,
                "holder binding",
                WalletKeyAlgorithm::Jubjub,
                WalletKeyPurpose::Assertion,
                PublicKeyEncoding::JubjubCompressed,
            )
        })
        .transpose()?;
        let operation = match call {
            MidnightDidBootstrapCall::AddAuthenticationMethod => {
                MidnightDidCallOperation::AddAuthenticationMethod {
                    method_id: "#key-auth".to_owned(),
                    x: authentication
                        .ok_or(DidLifecyclePortError::InvalidOperation)?
                        .public_key()
                        .bytes()
                        .try_into()
                        .map_err(|_| DidLifecyclePortError::InvalidOperation)?,
                }
            }
            MidnightDidBootstrapCall::AddAuthenticationRelationship => {
                MidnightDidCallOperation::AddAuthenticationRelationship {
                    method_id: "#key-auth".to_owned(),
                }
            }
            MidnightDidBootstrapCall::AddAssertionMethod => {
                let (x, y) =
                    jubjub_coordinates(&assertion.ok_or(DidLifecyclePortError::InvalidOperation)?)?;
                MidnightDidCallOperation::AddAssertionMethod {
                    method_id: "#key-assert".to_owned(),
                    x,
                    y,
                }
            }
            MidnightDidBootstrapCall::AddAssertionRelationship => {
                MidnightDidCallOperation::AddAssertionRelationship {
                    method_id: "#key-assert".to_owned(),
                }
            }
        };
        let mut hasher = Sha256::new();
        hasher.update(b"oxid:did-call:replay:v1");
        hasher.update(operation_scope.as_bytes());
        hasher.update(operation.circuit().id().as_bytes());
        self.compose(&NativeMidnightDidCallRequest {
            profile_id,
            account_index,
            controller_index,
            context,
            operation,
            replay_recipe: hasher.finalize().into(),
        })
    }

    fn bootstrap_key(
        &self,
        profile_id: &WalletProfileId,
        operation_scope: &str,
        role: &str,
        algorithm: WalletKeyAlgorithm,
        purpose: WalletKeyPurpose,
        encoding: PublicKeyEncoding,
    ) -> Result<WalletKeyDescriptor, DidLifecyclePortError> {
        let digest = Sha256::digest(operation_scope.as_bytes());
        let label = WalletKeyLabel::parse(format!(
            "Midnight DID {role} {}",
            &hex::encode(digest)[..16]
        ))
        .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
        let matches = self
            .keys
            .list(profile_id)
            .map_err(map_security_error)?
            .into_iter()
            .filter(|descriptor| descriptor.label() == &label)
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err(DidLifecyclePortError::Conflict);
        }
        let descriptor = if let Some(descriptor) = matches.into_iter().next() {
            descriptor
        } else {
            self.keys
                .generate(
                    profile_id,
                    GenerateProtectedKeyRequest {
                        label,
                        algorithm,
                        purpose,
                    },
                )
                .map_err(map_security_error)?
        };
        if descriptor.algorithm() != algorithm
            || descriptor.purpose() != purpose
            || descriptor.public_key().encoding() != encoding
            || descriptor.public_key().bytes().len() != 32
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        Ok(descriptor)
    }

    pub fn compose(
        &self,
        request: &NativeMidnightDidCallRequest,
    ) -> Result<NativeMidnightDidCallPlan, DidLifecyclePortError> {
        request.context.validate()?;
        request.operation.validate()?;
        if request.account_index > WalletHdPathComponent::MAX_INDEX
            || request.controller_index > WalletHdPathComponent::MAX_INDEX
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }

        let controller = controller_path(request.account_index, request.controller_index)?;
        let recovery = recovery_path(request.account_index)?;
        let replay = replay_randomness_path(request.account_index)?;
        let private_state = NativeDidPrivateState {
            controller_public_key: self.public_key(&request.profile_id, &controller)?,
            recovery_public_key: self.public_key(&request.profile_id, &recovery)?,
            timestamp_millis: request.context.timestamp_millis,
        };
        let signer = Arc::new(CustodyAuthorizationSigner {
            custody: Arc::clone(&self.custody),
            profile_id: request.profile_id.clone(),
            controller_path: controller,
            recovery_path: recovery,
        });
        let executor = GeneratedDidExecutor::new(
            NativeDidWitnesses,
            Arc::new(NativeDidPrivateStateStore(Mutex::new(private_state))),
            signer,
            ContractAddress(HashOutput(request.context.contract_address)),
        );
        let state = midnight_did_runtime::state_decode::charged_state_from_bytes(
            &request.context.contract_state,
        )
        .map_err(map_backend_error)?;
        let circuit = request.operation.circuit();
        let call = executor
            .execute(state, request.operation.clone().into_runtime()?)
            .map_err(map_backend_error)?;

        let verifier = self
            .artifacts
            .verifier_key(circuit)
            .map_err(|_| DidLifecyclePortError::ProtectionUnavailable)?;
        let mut verifier_cursor = Cursor::new(verifier.as_slice());
        let verifier_key: VerifierKey = tagged_deserialize(&mut verifier_cursor)
            .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
        if usize::try_from(verifier_cursor.position()).ok() != Some(verifier.len())
            || verifier_key.init().is_err()
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        let parameters_bytes = request
            .context
            .ledger_parameters
            .as_deref()
            .ok_or(DidLifecyclePortError::InvalidOperation)?;
        let mut parameters_cursor = Cursor::new(parameters_bytes);
        let parameters: LedgerParameters = tagged_deserialize(&mut parameters_cursor)
            .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
        if usize::try_from(parameters_cursor.position()).ok() != Some(parameters_bytes.len()) {
            return Err(DidLifecyclePortError::InvalidOperation);
        }

        let mut seeds = None;
        self.custody
            .use_derived_secret(&request.profile_id, &replay, &mut |secret| {
                seeds = Some((
                    derive_seed(secret, CALL_COMMUNICATION_DOMAIN, &request.replay_recipe)?,
                    derive_seed(secret, CALL_INTENT_DOMAIN, &request.replay_recipe)?,
                ));
                Ok(())
            })
            .map_err(map_security_error)?;
        let (communication_seed, intent_seed) =
            seeds.ok_or(DidLifecyclePortError::ProtectionUnavailable)?;
        let mut communication_bytes = *communication_seed;
        communication_bytes[31] = 0;
        let communication_commitment_rand = Fr::from_le_bytes(&communication_bytes)
            .ok_or(DidLifecyclePortError::InvalidOperation)?;
        communication_bytes.zeroize();
        let prepartition = call.into_ledger_prepartition_contract_call(LedgerContractCallConfig {
            operation: midnight_compact_runtime::ContractOperation::new(Some(verifier_key)),
            communication_commitment_rand,
            key_location: KeyLocation(Cow::Owned(circuit.artifact_id().to_owned())),
        });
        let empty: StandardTransaction<
            Signature,
            ProofPreimageMarker,
            PedersenRandomness,
            DefaultDB,
        > = StandardTransaction::new(
            request.context.network_id.clone(),
            LedgerHashMap::new(),
            None,
            LedgerHashMap::new(),
        );
        let mut intent_rng = ProtectedDeterministicRng::new(intent_seed);
        let transaction = empty
            .add_calls::<ProofPreimage>(
                &mut intent_rng,
                SegmentSpecifier::First,
                &[prepartition],
                &parameters,
                Timestamp::from_secs(request.context.expires_at_millis / 1_000),
                &[],
                &[],
                &[],
            )
            .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
        let transaction = Transaction::Standard(transaction);
        let mut encoded = Zeroizing::new(Vec::new());
        tagged_serialize(&transaction, &mut *encoded)
            .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
        if encoded.is_empty() || encoded.len() > MAX_TRANSACTION_BYTES {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        validate_transaction(&encoded, &request.context.network_id)?;
        Ok(NativeMidnightDidCallPlan {
            planning_fingerprint: Sha256::digest(encoded.as_slice()).into(),
            transaction: encoded,
        })
    }

    fn public_key(
        &self,
        profile_id: &WalletProfileId,
        path: &WalletHdPath,
    ) -> Result<EmbeddedGroupAffine, DidLifecyclePortError> {
        let mut public_key = None;
        self.custody
            .use_derived_secret(profile_id, path, &mut |secret| {
                public_key = Some(derive_public_key_from_seed(secret));
                Ok(())
            })
            .map_err(map_security_error)?;
        public_key.ok_or(DidLifecyclePortError::ProtectionUnavailable)
    }
}

impl MidnightDidCallCompositionPort for NativeMidnightDidCallComposer {
    fn compose_bootstrap_call<'a>(
        &'a self,
        profile_id: WalletProfileId,
        account_index: u32,
        controller_index: u32,
        operation_scope: String,
        context: MidnightDidCallContext,
        call: MidnightDidBootstrapCall,
    ) -> MidnightDidCallCompositionFuture<'a> {
        Box::pin(async move {
            self.compose_bootstrap(
                profile_id,
                account_index,
                controller_index,
                &operation_scope,
                context,
                call,
            )
        })
    }
}

#[derive(Clone)]
struct NativeDidPrivateState {
    controller_public_key: EmbeddedGroupAffine,
    recovery_public_key: EmbeddedGroupAffine,
    timestamp_millis: u64,
}

#[derive(Clone, Copy)]
struct NativeDidWitnesses;

impl midnight_did_runtime::contract::Witnesses<NativeDidPrivateState> for NativeDidWitnesses {
    fn get_schnorr_reduction<'a>(
        &self,
        _: &WitnessContext<midnight_did_runtime::contract::Ledger<'a>, NativeDidPrivateState>,
        _: Fr,
    ) -> (NativeDidPrivateState, (u8, u128)) {
        unreachable!("unreachable for the pinned Ledger8 DID Compact output")
    }
    fn local_controller_public_key<'a>(
        &self,
        context: &WitnessContext<midnight_did_runtime::contract::Ledger<'a>, NativeDidPrivateState>,
    ) -> (NativeDidPrivateState, EmbeddedGroupAffine) {
        (
            context.private_state.clone(),
            context.private_state.controller_public_key,
        )
    }
    fn local_recovery_authority_public_key<'a>(
        &self,
        context: &WitnessContext<midnight_did_runtime::contract::Ledger<'a>, NativeDidPrivateState>,
    ) -> (NativeDidPrivateState, EmbeddedGroupAffine) {
        (
            context.private_state.clone(),
            context.private_state.recovery_public_key,
        )
    }
    fn current_timestamp<'a>(
        &self,
        context: &WitnessContext<midnight_did_runtime::contract::Ledger<'a>, NativeDidPrivateState>,
    ) -> (NativeDidPrivateState, u64) {
        (
            context.private_state.clone(),
            context.private_state.timestamp_millis,
        )
    }
}

struct NativeDidPrivateStateStore(Mutex<NativeDidPrivateState>);
impl DidPrivateStateStore<NativeDidPrivateState> for NativeDidPrivateStateStore {
    fn load(&self) -> Result<NativeDidPrivateState, BackendError> {
        self.0
            .lock()
            .map(|state| state.clone())
            .map_err(|_| BackendError::Other("DID private-state lock poisoned".to_owned()))
    }
    fn store(&self, state: NativeDidPrivateState) -> Result<(), BackendError> {
        *self
            .0
            .lock()
            .map_err(|_| BackendError::Other("DID private-state lock poisoned".to_owned()))? =
            state;
        Ok(())
    }
}

struct CustodyAuthorizationSigner {
    custody: Arc<dyn WalletDerivedSecretUsePort>,
    profile_id: WalletProfileId,
    controller_path: WalletHdPath,
    recovery_path: WalletHdPath,
}
impl CustodyAuthorizationSigner {
    fn sign(
        &self,
        path: &WalletHdPath,
        digest: [Fr; 4],
    ) -> Result<midnight_compact_runtime::SchnorrSignature, BackendError> {
        let mut signature = None;
        self.custody
            .use_derived_secret(&self.profile_id, path, &mut |secret| {
                let signed = sign_authorization_digest_from_seed(secret, &digest)
                    .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
                signature = Some(midnight_compact_runtime::SchnorrSignature {
                    announcement: signed.announcement,
                    response: field_from_scalar(&signed.response),
                });
                Ok(())
            })
            .map_err(|_| BackendError::Other("DID authorization custody unavailable".to_owned()))?;
        signature.ok_or_else(|| {
            BackendError::Other("DID authorization signature unavailable".to_owned())
        })
    }
}
impl DidAuthorizationSigner for CustodyAuthorizationSigner {
    fn sign_controller(
        &self,
        digest: [Fr; 4],
    ) -> Result<midnight_compact_runtime::SchnorrSignature, BackendError> {
        self.sign(&self.controller_path, digest)
    }
    fn sign_recovery(
        &self,
        digest: [Fr; 4],
    ) -> Result<midnight_compact_runtime::SchnorrSignature, BackendError> {
        self.sign(&self.recovery_path, digest)
    }
}

fn jubjub_coordinates(
    descriptor: &WalletKeyDescriptor,
) -> Result<([u8; 32], [u8; 32]), DidLifecyclePortError> {
    let mut encoded = descriptor.public_key().bytes();
    let point = EmbeddedGroupAffine::deserialize(&mut encoded, 0)
        .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
    if !encoded.is_empty() || point.is_identity() {
        return Err(DidLifecyclePortError::InvalidOperation);
    }
    let x = point
        .x()
        .ok_or(DidLifecyclePortError::InvalidOperation)?
        .as_le_bytes()
        .try_into()
        .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
    let y = point
        .y()
        .ok_or(DidLifecyclePortError::InvalidOperation)?
        .as_le_bytes()
        .try_into()
        .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
    Ok((x, y))
}

fn validate_transaction(bytes: &[u8], network_id: &str) -> Result<(), DidLifecyclePortError> {
    let mut cursor = Cursor::new(bytes);
    let transaction: UnprovenTransaction =
        tagged_deserialize(&mut cursor).map_err(|_| DidLifecyclePortError::InvalidOperation)?;
    if cursor.position() != bytes.len() as u64 {
        return Err(DidLifecyclePortError::InvalidOperation);
    }
    let Transaction::Standard(standard) = transaction else {
        return Err(DidLifecyclePortError::InvalidOperation);
    };
    if standard.network_id != network_id || standard.intents.iter().count() != 1 {
        return Err(DidLifecyclePortError::InvalidOperation);
    }
    Ok(())
}

const fn map_security_error(error: WalletSecurityPortError) -> DidLifecyclePortError {
    match error {
        WalletSecurityPortError::Locked => DidLifecyclePortError::Locked,
        WalletSecurityPortError::UnsupportedAlgorithm => {
            DidLifecyclePortError::UnsupportedAlgorithm
        }
        WalletSecurityPortError::NotFound => DidLifecyclePortError::NotFound,
        WalletSecurityPortError::Conflict => DidLifecyclePortError::Conflict,
        WalletSecurityPortError::Unavailable | WalletSecurityPortError::NotInitialized => {
            DidLifecyclePortError::ProtectionUnavailable
        }
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => DidLifecyclePortError::InvalidOperation,
    }
}
fn map_backend_error(_: BackendError) -> DidLifecyclePortError {
    DidLifecyclePortError::InvalidOperation
}

#[cfg(test)]
mod tests {
    use super::*;
    use midnight_ledger::structure::INITIAL_PARAMETERS;
    use midnight_onchain_state::state::ContractMaintenanceAuthority;
    use oxid_adapter_platform_system::{OsRandom, SystemClock};
    use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
    use oxid_wallet_application::WalletProtectionPort;

    #[test]
    fn rejects_expired_or_incomplete_chain_contexts() {
        let context = MidnightDidCallContext {
            contract_state: vec![1],
            contract_address: [2; 32],
            zswap_chain_state: None,
            ledger_parameters: None,
            network_id: "undeployed".to_owned(),
            timestamp_millis: 2_000,
            expires_at_millis: 1_000,
            coin_public_key: [3; 32],
            encryption_public_key: [4; 32],
        };
        assert_eq!(
            context.validate(),
            Err(DidLifecyclePortError::InvalidOperation)
        );
    }
    #[test]
    fn maps_bootstrap_operations_to_generated_runtime_calls() {
        let operation = MidnightDidCallOperation::AddAuthenticationMethod {
            method_id: "#key-auth".to_owned(),
            x: [7; 32],
        };
        assert!(matches!(
            operation.into_runtime().expect("runtime call"),
            DidContractCall::SetVerificationMethod { .. }
        ));
    }

    #[test]
    fn composes_a_generated_bootstrap_call_without_an_external_process() {
        let Some(root) = std::env::var_os("OXID_MIDNIGHT_DID_ARTIFACTS_DIR") else {
            return;
        };
        let profile_id =
            WalletProfileId::parse("native-did-call-test".to_owned()).expect("profile id");
        let security = Arc::new(DevelopmentWalletSecurity::new(
            Arc::new(SystemClock),
            Arc::new(OsRandom),
        ));
        security
            .initialize(&profile_id)
            .expect("initialize custody");
        let custody: Arc<dyn WalletDerivedSecretUsePort> = security.clone();
        let keys: Arc<dyn WalletKeyOperationPort> = security;
        let composer = NativeMidnightDidCallComposer::new(
            Arc::clone(&custody),
            keys,
            MidnightDidCompactArtifacts::load(root).expect("authenticated artifacts"),
        );
        let controller = controller_path(0, 7).expect("controller path");
        let recovery = recovery_path(0).expect("recovery path");
        let private_state = NativeDidPrivateState {
            controller_public_key: composer
                .public_key(&profile_id, &controller)
                .expect("controller key"),
            recovery_public_key: composer
                .public_key(&profile_id, &recovery)
                .expect("recovery key"),
            timestamp_millis: 10_000,
        };
        let signer = Arc::new(CustodyAuthorizationSigner {
            custody,
            profile_id: profile_id.clone(),
            controller_path: controller,
            recovery_path: recovery,
        });
        let executor = GeneratedDidExecutor::new(
            NativeDidWitnesses,
            Arc::new(NativeDidPrivateStateStore(Mutex::new(private_state))),
            signer,
            ContractAddress::default(),
        );
        let deployment = executor.deployment_request().expect("generated deployment");
        let deploy = deployment.to_contract_deploy(LedgerDeploymentConfig {
            operations: LedgerHashMap::new(),
            maintenance_authority: ContractMaintenanceAuthority::default(),
            nonce: HashOutput([7; 32]),
        });
        let contract_address = deploy.address().0.0;
        let mut contract_state = Vec::new();
        tagged_serialize(&deploy.initial_state, &mut contract_state)
            .expect("serialize contract state");
        let mut ledger_parameters = Vec::new();
        tagged_serialize(&INITIAL_PARAMETERS, &mut ledger_parameters)
            .expect("serialize ledger parameters");

        let plan = composer
            .compose_bootstrap(
                profile_id,
                0,
                7,
                "native-bootstrap-call",
                MidnightDidCallContext {
                    contract_state,
                    contract_address,
                    zswap_chain_state: None,
                    ledger_parameters: Some(ledger_parameters),
                    network_id: "undeployed".to_owned(),
                    timestamp_millis: 11_000,
                    expires_at_millis: 3_611_000,
                    coin_public_key: [3; 32],
                    encryption_public_key: [4; 32],
                },
                MidnightDidBootstrapCall::AddAuthenticationMethod,
            )
            .expect("native generated call");
        assert!(!plan.transaction.is_empty());
        validate_transaction(&plan.transaction, "undeployed").expect("valid unproven tx");
    }
}
