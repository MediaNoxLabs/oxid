// SPDX-License-Identifier: Apache-2.0

//! Native composition of a real `did:midnight` contract deployment.
//!
//! This module deliberately stops at an unproven ledger transaction. DUST
//! balancing, proving, journal-before-broadcast, finality, and reconciliation
//! remain owned by `oxid-adapter-midnight`. Controller and maintenance secrets
//! are borrowed only inside the custody callback and never cross this adapter.

use std::{fmt, sync::Arc};

use midnight_base_crypto::{
    fab::AlignedValue,
    hash::{HashOutput, PersistentHashWriter},
    repr::BinaryHashRepr,
    signatures::{SigningKey, VerifyingKey},
    time::Timestamp,
};
use midnight_ledger::structure::{
    ContractDeploy, Intent, ProofPreimageMarker, StandardTransaction, Transaction,
};
use midnight_onchain_state::state::{
    ChargedState, ContractMaintenanceAuthority, ContractOperation, ContractState, EntryPointBuf,
    StateValue,
};
use midnight_storage::{
    DefaultDB,
    arena::Sp,
    storage::{Array, HashMap as LedgerHashMap},
};
use midnight_transient_crypto::{commitment::PedersenRandomness, fab::ValueReprAlignedValue};
use oxid_foundation::UnixTimestampMillis;
use oxid_identity_application::DidLifecyclePortError;
use oxid_identity_domain::{MidnightDid, MidnightNetwork};
use oxid_wallet_application::{
    WalletDerivedSecretUsePort, WalletHdPath, WalletHdPathComponent, WalletSecurityPortError,
};
use oxid_wallet_domain::WalletProfileId;
use rand::{SeedableRng, rngs::StdRng};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const BIP44_PURPOSE: u32 = 44;
const MIDNIGHT_COIN_TYPE: u32 = 2_400;
const NIGHT_EXTERNAL_ROLE: u32 = 0;
const DID_CONTROLLER_ROLE: u32 = 3;
const DEFAULT_KEY_INDEX: u32 = 0;
const DEPLOY_SEGMENT: u16 = 1;
const MAX_TRANSACTION_BYTES: usize = 16 * 1024 * 1024;

type LedgerTransaction = Transaction<
    midnight_base_crypto::schnorr::Signature,
    ProofPreimageMarker,
    PedersenRandomness,
    DefaultDB,
>;

/// Stable inputs that must be journaled before a deployment is submitted.
///
/// The nonce and composition seed are random public composition inputs, not
/// controller material. Reusing them after a crash reproduces the same DID and
/// exact unproven transaction without exposing a protected secret.
#[derive(Clone, PartialEq, Eq)]
pub struct NativeMidnightDidDeploymentRequest {
    profile_id: WalletProfileId,
    network: MidnightNetwork,
    network_id: String,
    account_index: u32,
    controller_index: u32,
    created_at: UnixTimestampMillis,
    expires_at: UnixTimestampMillis,
    nonce: [u8; 32],
    composition_seed: [u8; 32],
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
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .field("nonce", &"[PUBLIC RANDOM INPUT]")
            .field("composition_seed", &"[PUBLIC RANDOM INPUT]")
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
        created_at: UnixTimestampMillis,
        expires_at: UnixTimestampMillis,
        nonce: [u8; 32],
        composition_seed: [u8; 32],
    ) -> Result<Self, DidLifecyclePortError> {
        let network_id = network_id.into();
        if matches!(network, MidnightNetwork::Offchain)
            || network_id != network.as_str()
            || network_id.is_empty()
            || created_at.value() == 0
            || expires_at.value() <= created_at.value()
            || expires_at.value() / 1_000 <= created_at.value() / 1_000
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
            created_at,
            expires_at,
            nonce,
            composition_seed,
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
        let controller_path = did_controller_path(request.account_index, request.controller_index)?;
        let maintenance_path = maintenance_path(request.account_index)?;

        let mut controller_public_key = None;
        self.custody
            .use_derived_secret(&request.profile_id, &controller_path, &mut |secret| {
                controller_public_key = Some(controller_public_key_for(secret));
                Ok(())
            })
            .map_err(map_security_error)?;
        let controller_public_key =
            controller_public_key.ok_or(DidLifecyclePortError::ProtectionUnavailable)?;

        let mut maintenance_key = None;
        self.custody
            .use_derived_secret(&request.profile_id, &maintenance_path, &mut |secret| {
                let signing_key = SigningKey::from_bytes(secret)
                    .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
                maintenance_key = Some(signing_key.verifying_key());
                Ok(())
            })
            .map_err(map_security_error)?;
        let maintenance_key =
            maintenance_key.ok_or(DidLifecyclePortError::ProtectionUnavailable)?;

        let deploy = compose_deploy(
            controller_public_key,
            request.created_at.value(),
            request.nonce,
            vec![maintenance_key],
        );
        let address = deploy.address().0.0;
        let did = MidnightDid::parse(format!(
            "did:midnight:{}:{}",
            request.network.as_str(),
            hex::encode(address)
        ))
        .map_err(|_| DidLifecyclePortError::InvalidOperation)?;

        let mut rng = StdRng::from_seed(request.composition_seed);
        let ttl = Timestamp::from_secs(request.expires_at.value() / 1_000);
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
            expires_at_seconds: request.expires_at.value() / 1_000,
            planning_fingerprint,
            transaction: encoded,
        })
    }
}

fn controller_public_key_for(secret: &[u8; 32]) -> [u8; 32] {
    // `midnight-did` 0.4.0 calls Compact's persistentHash over a fixed vector
    // containing this domain and the 32-byte controller secret. Reproduce the
    // same FAB value-only encoding instead of treating the controller as a
    // Jubjub signing key.
    let domain = AlignedValue::from(*b"did:controller:pk\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    let secret = AlignedValue::from(*secret);
    let input = AlignedValue::concat([&domain, &secret]);
    let mut writer = PersistentHashWriter::default();
    ValueReprAlignedValue(input).binary_repr(&mut writer);
    writer.finalize().0
}

fn did_controller_path(
    account_index: u32,
    controller_index: u32,
) -> Result<WalletHdPath, DidLifecyclePortError> {
    hd_path(account_index, DID_CONTROLLER_ROLE, controller_index)
}

fn maintenance_path(account_index: u32) -> Result<WalletHdPath, DidLifecyclePortError> {
    hd_path(account_index, NIGHT_EXTERNAL_ROLE, DEFAULT_KEY_INDEX)
}

fn hd_path(
    account_index: u32,
    role: u32,
    index: u32,
) -> Result<WalletHdPath, DidLifecyclePortError> {
    let component = |value, hardened| {
        WalletHdPathComponent::new(value, hardened)
            .map_err(|_| DidLifecyclePortError::InvalidOperation)
    };
    WalletHdPath::new(vec![
        component(BIP44_PURPOSE, true)?,
        component(MIDNIGHT_COIN_TYPE, true)?,
        component(account_index, true)?,
        component(role, false)?,
        component(index, false)?,
    ])
    .map_err(|_| DidLifecyclePortError::InvalidOperation)
}

fn compose_deploy(
    controller_public_key: [u8; 32],
    timestamp_ms: u64,
    nonce: [u8; 32],
    committee: Vec<VerifyingKey>,
) -> ContractDeploy<DefaultDB> {
    ContractDeploy {
        initial_state: compose_initial_state(controller_public_key, timestamp_ms, committee),
        nonce: HashOutput(nonce),
    }
}

fn compose_initial_state(
    controller_public_key: [u8; 32],
    timestamp_ms: u64,
    committee: Vec<VerifyingKey>,
) -> ContractState<DefaultDB> {
    // This is the generated Compact `Ledger` schema for midnight-did 0.4.0.
    // Keep the immutable and mutable arrays explicit so schema drift fails the
    // conformance tests instead of silently writing values into stale slots.
    let constants = state_array(vec![
        cell(AlignedValue::from(1_u32)),
        cell(AlignedValue::from(controller_public_key)),
        cell(AlignedValue::from([0_u8; 32])),
    ]);
    let mutable = state_array(vec![
        empty_map(),
        cell(AlignedValue::from(0_u64)),
        cell(AlignedValue::from(timestamp_ms)),
        cell(AlignedValue::from(timestamp_ms)),
        cell(AlignedValue::from(false)),
        cell(AlignedValue::from(true)),
        cell(AlignedValue::from(0_u64)),
        empty_map(),
        empty_map(),
        empty_map(),
        empty_map(),
        empty_map(),
        empty_map(),
        empty_map(),
        empty_map(),
    ]);
    ContractState {
        data: ChargedState::new(state_array(vec![constants, mutable])),
        operations: LedgerHashMap::<EntryPointBuf, ContractOperation, DefaultDB>::new(),
        maintenance_authority: ContractMaintenanceAuthority {
            threshold: if committee.is_empty() { 0 } else { 1 },
            committee,
            counter: 0,
        },
        balance: LedgerHashMap::new(),
    }
}

fn state_array(values: Vec<StateValue<DefaultDB>>) -> StateValue<DefaultDB> {
    let mut array = Array::new();
    for value in values {
        array = array.push(value);
    }
    StateValue::Array(array)
}

fn empty_map() -> StateValue<DefaultDB> {
    StateValue::Map(LedgerHashMap::new())
}

fn cell(value: AlignedValue) -> StateValue<DefaultDB> {
    StateValue::Cell(Sp::new(value))
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

    use oxid_wallet_application::WalletDerivedSecretUsePort;

    use super::*;

    struct RecordingCustody {
        paths: Mutex<Vec<WalletHdPath>>,
    }

    impl RecordingCustody {
        fn new() -> Self {
            Self {
                paths: Mutex::new(Vec::new()),
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
            let secret = if role == DID_CONTROLLER_ROLE {
                [7_u8; 32]
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
            UnixTimestampMillis::new(1_777_840_000_000),
            UnixTimestampMillis::new(1_777_843_600_000),
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
        assert_eq!(custody.paths.lock().expect("paths").len(), 4);
    }

    #[test]
    fn changing_the_controller_index_changes_the_did_path_but_not_secret_exposure() {
        let custody = Arc::new(RecordingCustody::new());
        let composer = NativeMidnightDidDeploymentComposer::new(custody.clone());
        let mut request = request();
        request.controller_index = 8;

        let _ = composer.compose(&request).expect("plan");
        let paths = custody.paths.lock().expect("paths");
        assert_eq!(paths[0].components()[3].index(), DID_CONTROLLER_ROLE);
        assert_eq!(paths[0].components()[4].index(), 8);
        assert_eq!(paths[1].components()[3].index(), NIGHT_EXTERNAL_ROLE);
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
            UnixTimestampMillis::new(1_000),
            UnixTimestampMillis::new(3_000),
            [1; 32],
            [2; 32],
        );
        assert_eq!(invalid, Err(DidLifecyclePortError::InvalidOperation));
    }
}
