// SPDX-License-Identifier: Apache-2.0

//! Native composition of a signed Midnight DID verifier-key maintenance update.
//!
//! A DID contract is deliberately deployed without operations. Before a
//! verification-method circuit can be called, its reviewed verifier key must
//! be installed through the contract maintenance authority. This adapter keeps
//! the maintenance secret inside wallet custody and returns only an unproven
//! ledger transaction to the shared funding/proving/submission boundary.

use std::{fmt, io::Cursor, sync::Arc};

use midnight_base_crypto::{hash::HashOutput, signatures::SigningKey, time::Timestamp};
use midnight_coin_structure::contract::ContractAddress;
use midnight_ledger::structure::{
    ContractOperationVersionedVerifierKey, Intent, MaintenanceUpdate, ProofPreimageMarker,
    SingleUpdate, StandardTransaction, Transaction,
};
use midnight_onchain_state::state::EntryPointBuf;
use midnight_serialize::tagged_deserialize;
use midnight_storage::{DefaultDB, storage::HashMap as LedgerHashMap};
use midnight_transient_crypto::{commitment::PedersenRandomness, proofs::VerifierKey};
use oxid_identity_application::DidLifecyclePortError;
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
const DEFAULT_KEY_INDEX: u32 = 0;
const MAINTENANCE_SEGMENT: u16 = 1;
const MAX_ENTRY_POINT_BYTES: usize = 256;
const MAX_VERIFIER_KEY_BYTES: usize = 16 * 1024 * 1024;
const MAX_TRANSACTION_BYTES: usize = 32 * 1024 * 1024;

type LedgerTransaction = Transaction<
    midnight_base_crypto::schnorr::Signature,
    ProofPreimageMarker,
    PedersenRandomness,
    DefaultDB,
>;

#[derive(Clone, PartialEq, Eq)]
pub struct NativeMidnightDidMaintenanceRequest {
    profile_id: WalletProfileId,
    network_id: String,
    account_index: u32,
    contract_address: [u8; 32],
    entry_point: String,
    verifier_key: Zeroizing<Vec<u8>>,
    maintenance_counter: u32,
    expires_at_millis: u64,
    composition_seed: [u8; 32],
}

impl fmt::Debug for NativeMidnightDidMaintenanceRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeMidnightDidMaintenanceRequest")
            .field("profile_id", &self.profile_id)
            .field("network_id", &self.network_id)
            .field("account_index", &self.account_index)
            .field("contract_address", &hex::encode(self.contract_address))
            .field("entry_point", &self.entry_point)
            .field("verifier_key_bytes", &self.verifier_key.len())
            .field("maintenance_counter", &self.maintenance_counter)
            .field("expires_at_millis", &self.expires_at_millis)
            .field("composition_seed", &"[PUBLIC RANDOM INPUT]")
            .finish()
    }
}

impl NativeMidnightDidMaintenanceRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        profile_id: WalletProfileId,
        network_id: impl Into<String>,
        account_index: u32,
        contract_address: [u8; 32],
        entry_point: impl Into<String>,
        verifier_key: Zeroizing<Vec<u8>>,
        maintenance_counter: u32,
        expires_at_millis: u64,
        composition_seed: [u8; 32],
    ) -> Result<Self, DidLifecyclePortError> {
        let network_id = network_id.into();
        let entry_point = entry_point.into();
        if network_id.is_empty()
            || network_id.chars().any(char::is_control)
            || account_index > WalletHdPathComponent::MAX_INDEX
            || contract_address == [0; 32]
            || entry_point.is_empty()
            || entry_point.len() > MAX_ENTRY_POINT_BYTES
            || entry_point.bytes().any(|byte| byte.is_ascii_control())
            || verifier_key.is_empty()
            || verifier_key.len() > MAX_VERIFIER_KEY_BYTES
            || expires_at_millis < 1_000
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        Ok(Self {
            profile_id,
            network_id,
            account_index,
            contract_address,
            entry_point,
            verifier_key,
            maintenance_counter,
            expires_at_millis,
            composition_seed,
        })
    }
}

pub struct NativeMidnightDidMaintenancePlan {
    profile_id: String,
    network_id: String,
    expires_at_seconds: u64,
    planning_fingerprint: [u8; 32],
    transaction: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for NativeMidnightDidMaintenancePlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeMidnightDidMaintenancePlan")
            .field("profile_id", &self.profile_id)
            .field("network_id", &self.network_id)
            .field("expires_at_seconds", &self.expires_at_seconds)
            .field(
                "planning_fingerprint",
                &hex::encode(self.planning_fingerprint),
            )
            .field("transaction_bytes", &self.transaction.len())
            .finish_non_exhaustive()
    }
}

impl NativeMidnightDidMaintenancePlan {
    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    #[must_use]
    pub fn network_id(&self) -> &str {
        &self.network_id
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

pub struct NativeMidnightDidMaintenanceComposer {
    custody: Arc<dyn WalletDerivedSecretUsePort>,
}

impl NativeMidnightDidMaintenanceComposer {
    #[must_use]
    pub const fn new(custody: Arc<dyn WalletDerivedSecretUsePort>) -> Self {
        Self { custody }
    }

    pub fn compose(
        &self,
        request: &NativeMidnightDidMaintenanceRequest,
    ) -> Result<NativeMidnightDidMaintenancePlan, DidLifecyclePortError> {
        let mut verifier_cursor = Cursor::new(request.verifier_key.as_slice());
        let verifier_key: VerifierKey = tagged_deserialize(&mut verifier_cursor)
            .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
        if usize::try_from(verifier_cursor.position()).ok() != Some(request.verifier_key.len())
            || verifier_key.init().is_err()
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }

        let single = SingleUpdate::VerifierKeyInsert(
            EntryPointBuf(request.entry_point.as_bytes().to_vec()),
            ContractOperationVersionedVerifierKey::V3(verifier_key),
        );
        let address = ContractAddress(HashOutput(request.contract_address));
        let update: MaintenanceUpdate<DefaultDB> =
            MaintenanceUpdate::new(address, vec![single], request.maintenance_counter);
        let payload = update.data_to_sign();
        let path = maintenance_path(request.account_index)?;
        let mut signed = None;
        let mut signing_seed = request.composition_seed;
        signing_seed[0] ^= 0x5a;
        self.custody
            .use_derived_secret(&request.profile_id, &path, &mut |secret| {
                let signing_key = SigningKey::from_bytes(secret)
                    .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
                let mut rng = StdRng::from_seed(signing_seed);
                signed = Some(
                    update
                        .clone()
                        .add_signature(0, signing_key.sign(&mut rng, &payload)),
                );
                Ok(())
            })
            .map_err(map_security_error)?;
        let signed = signed.ok_or(DidLifecyclePortError::ProtectionUnavailable)?;

        let mut intent_rng = StdRng::from_seed(request.composition_seed);
        let intent = Intent::empty(
            &mut intent_rng,
            Timestamp::from_secs(request.expires_at_millis / 1_000),
        )
        .add_maintenance_update(signed);
        let mut intents = LedgerHashMap::new();
        intents = intents.insert(MAINTENANCE_SEGMENT, intent);
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
        Ok(NativeMidnightDidMaintenancePlan {
            profile_id: request.profile_id.as_str().to_owned(),
            network_id: request.network_id.clone(),
            expires_at_seconds: request.expires_at_millis / 1_000,
            planning_fingerprint,
            transaction: encoded,
        })
    }
}

fn maintenance_path(account_index: u32) -> Result<WalletHdPath, DidLifecyclePortError> {
    let component = |value, hardened| {
        WalletHdPathComponent::new(value, hardened)
            .map_err(|_| DidLifecyclePortError::InvalidOperation)
    };
    WalletHdPath::new(vec![
        component(BIP44_PURPOSE, true)?,
        component(MIDNIGHT_COIN_TYPE, true)?,
        component(account_index, true)?,
        component(NIGHT_EXTERNAL_ROLE, false)?,
        component(DEFAULT_KEY_INDEX, false)?,
    ])
    .map_err(|_| DidLifecyclePortError::InvalidOperation)
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
    use super::*;

    fn request(
        verifier_key: Vec<u8>,
    ) -> Result<NativeMidnightDidMaintenanceRequest, DidLifecyclePortError> {
        NativeMidnightDidMaintenanceRequest::new(
            WalletProfileId::parse("profile-1".to_owned()).expect("profile"),
            "undeployed",
            0,
            [7; 32],
            "setVerificationMethod",
            Zeroizing::new(verifier_key),
            0,
            1_777_843_600_000,
            [0x42; 32],
        )
    }

    #[test]
    fn rejects_malformed_verifier_key_before_custody() {
        let request = request(vec![1, 2, 3]).expect("request");
        struct UnreachableCustody;
        impl WalletDerivedSecretUsePort for UnreachableCustody {
            fn use_derived_secret(
                &self,
                _: &WalletProfileId,
                _: &WalletHdPath,
                _: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
            ) -> Result<(), WalletSecurityPortError> {
                panic!("custody must not be reached")
            }
        }
        let composer = NativeMidnightDidMaintenanceComposer::new(Arc::new(UnreachableCustody));
        assert!(matches!(
            composer.compose(&request),
            Err(DidLifecyclePortError::InvalidOperation)
        ));
    }

    #[test]
    fn rejects_unbounded_or_ambiguous_public_inputs() {
        let profile = WalletProfileId::parse("profile-1".to_owned()).expect("profile");
        let invalid = NativeMidnightDidMaintenanceRequest::new(
            profile,
            "undeployed",
            0,
            [0; 32],
            "setVerificationMethod",
            Zeroizing::new(vec![1]),
            0,
            1_000,
            [1; 32],
        );
        assert_eq!(invalid, Err(DidLifecyclePortError::InvalidOperation));
    }
}
