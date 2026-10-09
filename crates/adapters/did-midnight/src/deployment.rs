// SPDX-License-Identifier: Apache-2.0

//! Native composition of a real `did:midnight` contract deployment.
//!
//! This module deliberately stops at an unproven ledger transaction. DUST
//! balancing, proving, journal-before-broadcast, finality, and reconciliation
//! remain owned by `oxid-adapter-midnight`. Controller and maintenance secrets
//! are borrowed only inside the custody callback and never cross this adapter.

use std::{
    fmt,
    sync::{Arc, Mutex},
};

use midnight_base_crypto::{hash::HashOutput, signatures::VerifyingKey, time::Timestamp};
use midnight_compact_runtime::{ContractAddress, Fr, WitnessContext};
use midnight_did_jubjub_schnorr::derive_public_key_from_seed;
use midnight_did_runtime::{
    BackendError, DidAuthorizationSigner, DidPrivateStateStore, GeneratedDidExecutor,
    LedgerDeploymentConfig,
};
use midnight_ledger::structure::{Intent, ProofPreimageMarker, StandardTransaction, Transaction};
use midnight_onchain_state::state::ContractMaintenanceAuthority;
use midnight_storage::{DefaultDB, storage::HashMap as LedgerHashMap};
use midnight_transient_crypto::{commitment::PedersenRandomness, curve::EmbeddedGroupAffine};
use oxid_identity_application::DidLifecyclePortError;
use oxid_identity_domain::{MidnightDid, MidnightNetwork};
use oxid_wallet_application::{
    WalletDerivedSecretUsePort, WalletHdPathComponent, WalletSecurityPortError,
};
use oxid_wallet_domain::WalletProfileId;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    custody::{controller_path, maintenance_path, recovery_path, replay_randomness_path},
    protected_randomness::{ProtectedDeterministicRng, derive_seed, maintenance_signing_key},
};

const DEPLOY_SEGMENT: u16 = 1;
const MAX_TRANSACTION_BYTES: usize = 16 * 1024 * 1024;
const DEPLOY_NONCE_DOMAIN: &[u8] = b"oxid:did-deploy:contract-nonce:v1";
const DEPLOY_INTENT_DOMAIN: &[u8] = b"oxid:did-deploy:intent-rng:v1";

type LedgerTransaction = Transaction<
    midnight_base_crypto::schnorr::Signature,
    ProofPreimageMarker,
    PedersenRandomness,
    DefaultDB,
>;

/// Stable inputs that must be journaled before a deployment is submitted.
///
/// The recipes are public journal inputs, not randomness. Custody combines
/// them with the dedicated protected replay role so a crash reproduces the
/// exact transaction without making its nonce or intent randomness public.
#[derive(Clone, PartialEq, Eq)]
pub struct NativeMidnightDidDeploymentRequest {
    profile_id: WalletProfileId,
    network: MidnightNetwork,
    network_id: String,
    account_index: u32,
    controller_index: u32,
    created_at_millis: u64,
    expires_at_millis: u64,
    nonce_recipe: [u8; 32],
    intent_recipe: [u8; 32],
}

impl fmt::Debug for NativeMidnightDidDeploymentRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeMidnightDidDeploymentRequest")
            .field("profile_id", &self.profile_id)
            .field("network", &self.network)
            .field("network_id", &self.network_id)
            .field("account_index", &self.account_index)
            .field("controller_index", &self.controller_index)
            .field("created_at_millis", &self.created_at_millis)
            .field("expires_at_millis", &self.expires_at_millis)
            .field("nonce_recipe", &"[PUBLIC REPLAY RECIPE]")
            .field("intent_recipe", &"[PUBLIC REPLAY RECIPE]")
            .finish()
    }
}

impl NativeMidnightDidDeploymentRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        profile_id: WalletProfileId,
        network: MidnightNetwork,
        network_id: impl Into<String>,
        account_index: u32,
        controller_index: u32,
        created_at_millis: u64,
        expires_at_millis: u64,
        nonce_recipe: [u8; 32],
        intent_recipe: [u8; 32],
    ) -> Result<Self, DidLifecyclePortError> {
        let network_id = network_id.into();
        if matches!(network, MidnightNetwork::Offchain)
            || network_id != network.as_str()
            || network_id.is_empty()
            || created_at_millis == 0
            || expires_at_millis <= created_at_millis
            || expires_at_millis / 1_000 <= created_at_millis / 1_000
            || account_index > WalletHdPathComponent::MAX_INDEX
            || controller_index > WalletHdPathComponent::MAX_INDEX
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        Ok(Self {
            profile_id,
            network,
            network_id,
            account_index,
            controller_index,
            created_at_millis,
            expires_at_millis,
            nonce_recipe,
            intent_recipe,
        })
    }
}

/// Public result of native DID deployment composition.
pub struct NativeMidnightDidDeploymentPlan {
    profile_id: String,
    network_id: String,
    did: MidnightDid,
    expires_at_seconds: u64,
    planning_fingerprint: [u8; 32],
    transaction: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for NativeMidnightDidDeploymentPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeMidnightDidDeploymentPlan")
            .field("profile_id", &self.profile_id)
            .field("network_id", &self.network_id)
            .field("did", &self.did)
            .field("expires_at_seconds", &self.expires_at_seconds)
            .field(
                "planning_fingerprint",
                &hex::encode(self.planning_fingerprint),
            )
            .field("transaction_bytes", &self.transaction.len())
            .finish_non_exhaustive()
    }
}

impl NativeMidnightDidDeploymentPlan {
    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    #[must_use]
    pub fn network_id(&self) -> &str {
        &self.network_id
    }

    #[must_use]
    pub const fn did(&self) -> &MidnightDid {
        &self.did
    }

    #[must_use]
    pub const fn expires_at_seconds(&self) -> u64 {
        self.expires_at_seconds
    }

    #[must_use]
    pub const fn planning_fingerprint(&self) -> [u8; 32] {
        self.planning_fingerprint
    }

    #[must_use]
    pub fn into_transaction(self) -> Zeroizing<Vec<u8>> {
        self.transaction
    }
}

/// Composes a contract deploy while keeping all controller material in custody.
pub struct NativeMidnightDidDeploymentComposer {
    custody: Arc<dyn WalletDerivedSecretUsePort>,
}

impl NativeMidnightDidDeploymentComposer {
    #[must_use]
    pub const fn new(custody: Arc<dyn WalletDerivedSecretUsePort>) -> Self {
        Self { custody }
    }

    pub fn compose(
        &self,
        request: &NativeMidnightDidDeploymentRequest,
    ) -> Result<NativeMidnightDidDeploymentPlan, DidLifecyclePortError> {
        let controller_path = controller_path(request.account_index, request.controller_index)?;
        let maintenance_path = maintenance_path(request.account_index)?;
        let recovery_path = recovery_path(request.account_index)?;
        let replay_path = replay_randomness_path(request.account_index)?;

        let mut controller_public_key = None;
        self.custody
            .use_derived_secret(&request.profile_id, &controller_path, &mut |secret| {
                controller_public_key = Some(derive_public_key_from_seed(secret));
                Ok(())
            })
            .map_err(map_security_error)?;
        let controller_public_key =
            controller_public_key.ok_or(DidLifecyclePortError::ProtectionUnavailable)?;

        let mut recovery_public_key = None;
        self.custody
            .use_derived_secret(&request.profile_id, &recovery_path, &mut |secret| {
                recovery_public_key = Some(derive_public_key_from_seed(secret));
                Ok(())
            })
            .map_err(map_security_error)?;
        let recovery_public_key =
            recovery_public_key.ok_or(DidLifecyclePortError::ProtectionUnavailable)?;

        let mut maintenance_key = None;
        self.custody
            .use_derived_secret(&request.profile_id, &maintenance_path, &mut |secret| {
                let signing_key = maintenance_signing_key(secret)?;
                maintenance_key = Some(signing_key.verifying_key());
                Ok(())
            })
            .map_err(map_security_error)?;
        let maintenance_key =
            maintenance_key.ok_or(DidLifecyclePortError::ProtectionUnavailable)?;

        let mut result = None;
        let mut composition_error = None;
        self.custody
            .use_derived_secret(&request.profile_id, &replay_path, &mut |secret| {
                match compose_with_protected_randomness(
                    request,
                    controller_public_key,
                    recovery_public_key,
                    maintenance_key.clone(),
                    secret,
                ) {
                    Ok(plan) => result = Some(plan),
                    Err(error) => {
                        composition_error = Some(error);
                        return Err(WalletSecurityPortError::InvalidOperation);
                    }
                }
                Ok(())
            })
            .map_err(map_security_error)?;
        if let Some(error) = composition_error {
            return Err(error);
        }
        result.ok_or(DidLifecyclePortError::ProtectionUnavailable)
    }
}

fn compose_with_protected_randomness(
    request: &NativeMidnightDidDeploymentRequest,
    controller_public_key: EmbeddedGroupAffine,
    recovery_public_key: EmbeddedGroupAffine,
    maintenance_key: VerifyingKey,
    replay_secret: &[u8; 32],
) -> Result<NativeMidnightDidDeploymentPlan, DidLifecyclePortError> {
    let nonce = derive_seed(replay_secret, DEPLOY_NONCE_DOMAIN, &request.nonce_recipe)
        .map_err(map_security_error)?;
    let intent_seed = derive_seed(replay_secret, DEPLOY_INTENT_DOMAIN, &request.intent_recipe)
        .map_err(map_security_error)?;
    let private_state = NativeDidPrivateState {
        controller_public_key,
        recovery_public_key,
        timestamp_millis: request.created_at_millis,
    };
    let executor = GeneratedDidExecutor::new(
        NativeDidWitnesses,
        Arc::new(NativeDidPrivateStateStore(Mutex::new(private_state))),
        Arc::new(ConstructorOnlySigner),
        ContractAddress::default(),
    );
    let deployment = executor.deployment_request().map_err(map_backend_error)?;
    let deploy = deployment.to_contract_deploy(LedgerDeploymentConfig {
        operations: LedgerHashMap::new(),
        maintenance_authority: ContractMaintenanceAuthority {
            threshold: 1,
            committee: vec![maintenance_key],
            counter: 0,
        },
        nonce: HashOutput(*nonce),
    });
    let address = deploy.address().0.0;
    let did = MidnightDid::parse(format!(
        "did:midnight:{}:{}",
        request.network.as_str(),
        hex::encode(address)
    ))
    .map_err(|_| DidLifecyclePortError::InvalidOperation)?;

    let mut rng = ProtectedDeterministicRng::new(intent_seed);
    let ttl = Timestamp::from_secs(request.expires_at_millis / 1_000);
    let intent = Intent::empty(&mut rng, ttl).add_deploy(deploy);
    let mut intents = LedgerHashMap::new();
    intents = intents.insert(DEPLOY_SEGMENT, intent);
    let transaction = LedgerTransaction::Standard(StandardTransaction::new(
        &request.network_id,
        intents,
        None,
        LedgerHashMap::new(),
    ));
    let mut encoded = Zeroizing::new(Vec::new());
    midnight_serialize::tagged_serialize(&transaction, &mut *encoded)
        .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
    if encoded.is_empty() || encoded.len() > MAX_TRANSACTION_BYTES {
        return Err(DidLifecyclePortError::InvalidOperation);
    }
    let planning_fingerprint = Sha256::digest(encoded.as_slice()).into();

    Ok(NativeMidnightDidDeploymentPlan {
        profile_id: request.profile_id.as_str().to_owned(),
        network_id: request.network_id.clone(),
        did,
        expires_at_seconds: request.expires_at_millis / 1_000,
        planning_fingerprint,
        transaction: encoded,
    })
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

struct ConstructorOnlySigner;

impl DidAuthorizationSigner for ConstructorOnlySigner {
    fn sign_controller(
        &self,
        _: [Fr; 4],
    ) -> Result<midnight_compact_runtime::SchnorrSignature, BackendError> {
        Err(BackendError::Other(
            "constructor must not request controller authorization".to_owned(),
        ))
    }

    fn sign_recovery(
        &self,
        _: [Fr; 4],
    ) -> Result<midnight_compact_runtime::SchnorrSignature, BackendError> {
        Err(BackendError::Other(
            "constructor must not request recovery authorization".to_owned(),
        ))
    }
}

fn map_backend_error(_: BackendError) -> DidLifecyclePortError {
    DidLifecyclePortError::InvalidOperation
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use oxid_wallet_application::{WalletDerivedSecretUsePort, WalletHdPath};

    use super::*;

    struct RecordingCustody {
        paths: Mutex<Vec<WalletHdPath>>,
        replay_secret: [u8; 32],
    }

    impl RecordingCustody {
        fn new() -> Self {
            Self {
                paths: Mutex::new(Vec::new()),
                replay_secret: [11; 32],
            }
        }

        fn with_replay_secret(replay_secret: [u8; 32]) -> Self {
            Self {
                paths: Mutex::new(Vec::new()),
                replay_secret,
            }
        }
    }

    impl WalletDerivedSecretUsePort for RecordingCustody {
        fn use_derived_secret(
            &self,
            _: &WalletProfileId,
            path: &WalletHdPath,
            operation: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
        ) -> Result<(), WalletSecurityPortError> {
            self.paths.lock().expect("paths").push(path.clone());
            let role = path.components()[3].index();
            let controller_role =
                controller_path(0, 0).expect("controller path").components()[3].index();
            let replay_role =
                replay_randomness_path(0).expect("replay path").components()[3].index();
            let secret = if role == controller_role {
                [7_u8; 32]
            } else if role == replay_role {
                self.replay_secret
            } else {
                [9_u8; 32]
            };
            operation(&secret)
        }
    }

    fn request() -> NativeMidnightDidDeploymentRequest {
        NativeMidnightDidDeploymentRequest::new(
            WalletProfileId::parse("profile-1".to_owned()).expect("profile"),
            MidnightNetwork::Undeployed,
            "undeployed",
            2,
            4,
            10_000,
            3_610_000,
            [0x99; 32],
            [0x42; 32],
        )
        .expect("request")
    }

    #[test]
    fn composes_the_same_real_deploy_for_the_same_journaled_inputs() {
        let custody = Arc::new(RecordingCustody::new());
        let composer = NativeMidnightDidDeploymentComposer::new(custody.clone());

        let first = composer.compose(&request()).expect("first");
        let first_did = first.did().clone();
        let first_fingerprint = first.planning_fingerprint();
        let first_transaction = first.into_transaction();
        let second = composer.compose(&request()).expect("second");

        assert_eq!(second.did(), &first_did);
        assert_eq!(second.planning_fingerprint(), first_fingerprint);
        assert_eq!(
            second.into_transaction().as_slice(),
            first_transaction.as_slice()
        );
        assert_eq!(custody.paths.lock().expect("paths").len(), 8);
    }

    #[test]
    fn changing_the_controller_index_changes_the_did_path_but_not_secret_exposure() {
        let custody = Arc::new(RecordingCustody::new());
        let composer = NativeMidnightDidDeploymentComposer::new(custody.clone());
        let mut request = request();
        request.controller_index = 8;

        let _ = composer.compose(&request).expect("plan");
        let paths = custody.paths.lock().expect("paths");
        assert_eq!(paths[0], controller_path(2, 8).expect("controller path"));
        assert_eq!(paths[1], recovery_path(2).expect("recovery path"));
        assert_eq!(paths[2], maintenance_path(2).expect("maintenance path"));
        assert_eq!(paths[3], replay_randomness_path(2).expect("replay path"));
    }

    #[test]
    fn public_replay_recipe_requires_the_same_protected_randomness_secret() {
        let first = NativeMidnightDidDeploymentComposer::new(Arc::new(
            RecordingCustody::with_replay_secret([11; 32]),
        ))
        .compose(&request())
        .expect("first");
        let second = NativeMidnightDidDeploymentComposer::new(Arc::new(
            RecordingCustody::with_replay_secret([12; 32]),
        ))
        .compose(&request())
        .expect("second");

        assert_ne!(first.did(), second.did());
        assert_ne!(first.planning_fingerprint(), second.planning_fingerprint());
    }

    #[test]
    fn rejects_offchain_or_mismatched_ledger_networks() {
        let profile = WalletProfileId::parse("profile-1".to_owned()).expect("profile");
        let invalid = NativeMidnightDidDeploymentRequest::new(
            profile,
            MidnightNetwork::Offchain,
            "offchain",
            0,
            0,
            1_000,
            3_000,
            [1; 32],
            [2; 32],
        );
        assert_eq!(invalid, Err(DidLifecyclePortError::InvalidOperation));
    }
}
