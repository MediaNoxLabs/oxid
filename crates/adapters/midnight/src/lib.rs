// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(not(target_arch = "wasm32"))]
mod checkpoint;
#[cfg(not(target_arch = "wasm32"))]
mod dust_checkpoint;
#[cfg(not(target_arch = "wasm32"))]
mod dust_sync;
#[cfg(not(target_arch = "wasm32"))]
mod indexer;
#[cfg(not(target_arch = "wasm32"))]
mod local_proving;
#[cfg(not(target_arch = "wasm32"))]
mod shielded;
#[cfg(not(target_arch = "wasm32"))]
mod shielded_checkpoint;
#[cfg(not(target_arch = "wasm32"))]
mod shielded_sync;
#[cfg(not(target_arch = "wasm32"))]
mod shielded_transport;
#[cfg(not(target_arch = "wasm32"))]
mod submission;
#[cfg(not(target_arch = "wasm32"))]
mod submission_journal;
#[cfg(not(target_arch = "wasm32"))]
mod transaction;

#[cfg(not(target_arch = "wasm32"))]
pub use checkpoint::{MidnightAccountCheckpointConfig, MidnightAccountCheckpointConfigError};
#[cfg(not(target_arch = "wasm32"))]
pub use dust_checkpoint::{MidnightDustCheckpointConfig, MidnightDustCheckpointConfigError};
#[cfg(not(target_arch = "wasm32"))]
pub use indexer::{
    LiveMidnightAccountSource, MidnightIndexerConfig, MidnightIndexerConfigError,
    live_midnight_wallet, live_midnight_wallet_with_checkpoints, protected_live_midnight_wallet,
    protected_live_midnight_wallet_with_checkpoint_options,
    protected_live_midnight_wallet_with_checkpoints,
};
#[cfg(not(target_arch = "wasm32"))]
pub use local_proving::{
    MidnightLocalProvingConfig, MidnightLocalProvingConfigError, MidnightLocalProvingMetrics,
};
#[cfg(all(not(target_arch = "wasm32"), feature = "proving-bench"))]
pub use local_proving::{MidnightLocalProvingFixtureReport, run_local_proving_fixture};
#[cfg(not(target_arch = "wasm32"))]
pub use shielded_checkpoint::{
    MidnightShieldedCheckpointConfig, MidnightShieldedCheckpointConfigError,
};
#[cfg(not(target_arch = "wasm32"))]
pub use submission::{
    MidnightProvingMode, MidnightStandaloneConfig, MidnightStandaloneConfigError,
};
#[cfg(not(target_arch = "wasm32"))]
pub use submission_journal::{
    MidnightSubmissionJournalConfig, MidnightSubmissionJournalConfigError,
};
#[cfg(not(target_arch = "wasm32"))]
pub use transaction::{
    FundedMidnightContractCall, MidnightContractCallFundingPort,
    MidnightContractCallFundingRequest, MidnightContractCallSubmissionMode,
    MidnightContractCallSubmissionOutcome, MidnightContractCallSubmissionPort,
    MidnightContractCallSubmissionRequest, MidnightContractCallSubmissionState,
    MidnightContractCallSubmissionStatus,
};

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use bech32::{Bech32m, Hrp, primitives::decode::CheckedHrpstring};
#[cfg(not(target_arch = "wasm32"))]
use midnight_serialize::Serializable as _;
#[cfg(not(target_arch = "wasm32"))]
use midnight_zswap::keys::{SecretKeys as ZswapSecretKeys, Seed as ZswapSeed};
use oxid_platform_ports::ClockPort;
use oxid_wallet_application::{
    DeriveProtectedKeyRequest, WalletAccountAssociation, WalletAccountDerivationPort,
    WalletAccountPortError, WalletAccountPortFuture, WalletAccountReadPort,
    WalletDerivedSecretUsePort, WalletDustSyncPort, WalletDustSyncPortError, WalletHdPath,
    WalletHdPathComponent, WalletKeyDerivationPort, WalletKeyOperationPort, WalletNetworkPort,
    WalletProfileAssociationRepository, WalletProfileAssociationRepositoryError,
    WalletProfileAssociations, WalletSecurityPortError, WalletShieldedSyncPort,
    WalletShieldedSyncPortError,
};
use oxid_wallet_domain::{
    AssetBalance, AssetBalanceChange, AssetSymbol, BalanceChangeDirection, ChainAccountId,
    ChainAddress, ChainAddressKind, ChainAsset, ChainAssetId, ChainKind, ChainNetwork,
    ChainNetworkId, ChainTransactionId, DerivedChainAccount, NetworkDisplayName,
    NetworkEnvironment, PublicKeyEncoding, WalletAccountSnapshot, WalletAccountSource,
    WalletKeyAlgorithm, WalletKeyLabel, WalletKeyPurpose, WalletProfileId, WalletSyncState,
    WalletSyncStatus, WalletTransaction, WalletTransactionDirection, WalletTransactionStatus,
};
use sha2::{Digest, Sha256};

const DEFAULT_NETWORK_ID: &str = "undeployed";
pub(crate) const BIP44_PURPOSE: u32 = 44;
pub(crate) const MIDNIGHT_COIN_TYPE: u32 = 2400;
const NIGHT_EXTERNAL_ROLE: u32 = 0;
pub(crate) const DUST_ROLE: u32 = 2;
pub(crate) const DUST_INDEX: u32 = 0;
pub(crate) const ZSWAP_ROLE: u32 = 3;
pub(crate) const ZSWAP_INDEX: u32 = 0;

// Canonical ledger-8 atomic-unit semantics reviewed at
// midnight-ledger d9414884db9da9e9b1f6f3a7f742d79a5732f817,
// ledger/src/structure.rs. Keeping these adapter-local avoids importing the
// ledger's transaction/proof graph into a read-model-only capability.
pub(crate) const STARS_PER_NIGHT: u128 = 1_000_000;
pub(crate) const SPECKS_PER_DUST: u128 = 1_000_000_000_000_000;

// Public payloads from the official Midnight Wallet SDK address conformance
// vector for seed class 01. No seed or private key is retained in Oxid.
const FIXTURE_UNSHIELDED_PAYLOAD: &str =
    "ec3925bdbd24aa1cfd002f9ccf52f2bc30061721d8841e86c93c087c1fd2bdcb";
const FIXTURE_SHIELDED_PAYLOAD: &str = concat!(
    "094a912589509407d05de805bf9ffdc612d3f2e2d956a965c642083d5172ab43",
    "54612cf65f5b75e6f033d97e5bfcfc2ed1b5cb66b91a783540fe5f48b5a5e7e9"
);
const FIXTURE_DUST_PAYLOAD: &str =
    "73f97e7f53047cfa3a995ccb2f708363ebe76b272c492c7190261bfd9602b34164";

/// Account state provider used behind the common Midnight network adapter.
pub trait MidnightAccountSource: Send + Sync {
    fn bind_derived_account(
        &self,
        _: &WalletProfileId,
        _: &ChainNetwork,
        _: &DerivedChainAccount,
    ) -> Result<(), WalletAccountPortError> {
        Err(WalletAccountPortError::Unavailable)
    }

    fn account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<WalletAccountSnapshot, WalletAccountPortError>;

    fn sync<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        network: &'a ChainNetwork,
    ) -> WalletAccountPortFuture<'a>;
}

/// Midnight-specific account derivation implemented over a protected key port.
pub trait MidnightAccountDeriver: Send + Sync {
    fn derive(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
        account_index: u32,
        address_index: u32,
    ) -> Result<DerivedChainAccount, WalletAccountPortError>;
}

/// Public Midnight account values needed to compose a contract call. The
/// source retains address-codec authority; incoming adapters never provide
/// these byte payloads.
#[derive(Clone, PartialEq, Eq)]
pub struct MidnightPublicCallContext {
    network_id: ChainNetworkId,
    coin_public_key: [u8; 32],
    encryption_public_key: [u8; 32],
    unshielded_recipient: [u8; 32],
}

impl std::fmt::Debug for MidnightPublicCallContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MidnightPublicCallContext")
            .field("network_id", &self.network_id)
            .finish_non_exhaustive()
    }
}

impl MidnightPublicCallContext {
    #[must_use]
    pub const fn network_id(&self) -> &ChainNetworkId {
        &self.network_id
    }

    #[must_use]
    pub const fn coin_public_key(&self) -> [u8; 32] {
        self.coin_public_key
    }

    #[must_use]
    pub const fn encryption_public_key(&self) -> [u8; 32] {
        self.encryption_public_key
    }

    #[must_use]
    pub const fn unshielded_recipient(&self) -> [u8; 32] {
        self.unshielded_recipient
    }
}

/// Supplies only public, profile-scoped Midnight account context for native
/// contract composition.
pub trait MidnightPublicCallContextSource: Send + Sync {
    fn public_call_context(
        &self,
        profile_id: &str,
    ) -> Result<MidnightPublicCallContext, WalletAccountPortError>;
}

/// Fail-closed derivation adapter for compositions without protected root material.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableMidnightAccountDeriver;

impl MidnightAccountDeriver for UnavailableMidnightAccountDeriver {
    fn derive(
        &self,
        _: &WalletProfileId,
        _: &ChainNetwork,
        _: u32,
        _: u32,
    ) -> Result<DerivedChainAccount, WalletAccountPortError> {
        Err(WalletAccountPortError::Unavailable)
    }
}

/// Converts Midnight's canonical account path into an opaque protected child key.
pub struct ProtectedMidnightAccountDeriver<K> {
    keys: Arc<K>,
}

impl<K> Clone for ProtectedMidnightAccountDeriver<K> {
    fn clone(&self) -> Self {
        Self {
            keys: Arc::clone(&self.keys),
        }
    }
}

impl<K> ProtectedMidnightAccountDeriver<K> {
    #[must_use]
    pub const fn new(keys: Arc<K>) -> Self {
        Self { keys }
    }
}

impl<K> MidnightAccountDeriver for ProtectedMidnightAccountDeriver<K>
where
    K: WalletDerivedSecretUsePort + WalletKeyDerivationPort + 'static,
{
    fn derive(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
        account_index: u32,
        address_index: u32,
    ) -> Result<DerivedChainAccount, WalletAccountPortError> {
        let component = |index, hardened| {
            WalletHdPathComponent::new(index, hardened)
                .map_err(|_| WalletAccountPortError::InvalidData)
        };
        let path = WalletHdPath::new(vec![
            component(BIP44_PURPOSE, true)?,
            component(MIDNIGHT_COIN_TYPE, true)?,
            component(account_index, true)?,
            component(NIGHT_EXTERNAL_ROLE, false)?,
            component(address_index, false)?,
        ])
        .map_err(|_| WalletAccountPortError::InvalidData)?;
        let label = WalletKeyLabel::parse(format!(
            "Midnight NIGHT account {account_index}/{address_index}"
        ))
        .map_err(|_| WalletAccountPortError::InvalidData)?;
        let descriptor = self
            .keys
            .derive(
                profile_id,
                DeriveProtectedKeyRequest {
                    label,
                    algorithm: WalletKeyAlgorithm::Secp256k1Schnorr,
                    purpose: WalletKeyPurpose::Transaction,
                    path,
                },
            )
            .map_err(map_security_error)?;
        if descriptor.algorithm() != WalletKeyAlgorithm::Secp256k1Schnorr
            || descriptor.public_key().encoding() != PublicKeyEncoding::Secp256k1XOnly
            || descriptor.public_key().bytes().len() != 32
        {
            return Err(WalletAccountPortError::InvalidData);
        }

        let payload = Sha256::digest(descriptor.public_key().bytes());
        let address = encode_midnight_address(
            network.id(),
            ChainAddressKind::Unshielded,
            "addr",
            payload.as_slice(),
        )?;
        #[cfg(not(target_arch = "wasm32"))]
        let shielded_address = {
            let zswap_path = WalletHdPath::new(vec![
                component(BIP44_PURPOSE, true)?,
                component(MIDNIGHT_COIN_TYPE, true)?,
                component(account_index, true)?,
                component(ZSWAP_ROLE, false)?,
                component(ZSWAP_INDEX, false)?,
            ])
            .map_err(|_| WalletAccountPortError::InvalidData)?;
            let mut derived = None;
            self.keys
                .use_derived_secret(profile_id, &zswap_path, &mut |seed| {
                    let keys = ZswapSecretKeys::from(ZswapSeed::from(*seed));
                    let mut payload = Vec::with_capacity(64);
                    keys.coin_public_key()
                        .serialize(&mut payload)
                        .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
                    keys.enc_public_key()
                        .serialize(&mut payload)
                        .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
                    if payload.len() != 64 {
                        return Err(WalletSecurityPortError::InvalidOperation);
                    }
                    derived = Some(encode_midnight_address(
                        network.id(),
                        ChainAddressKind::Shielded,
                        "shield-addr",
                        &payload,
                    ));
                    Ok(())
                })
                .map_err(map_security_error)?;
            derived.ok_or(WalletAccountPortError::InvalidData)??
        };
        let account_id =
            ChainAccountId::parse(format!("midnight_account_{account_index}_{address_index}"))
                .map_err(|_| WalletAccountPortError::InvalidData)?;
        let derived = DerivedChainAccount::new(
            network.id().clone(),
            account_id,
            account_index,
            address_index,
            address,
            descriptor.public_key().clone(),
            descriptor.reference().clone(),
        )
        .map_err(|_| WalletAccountPortError::InvalidData)?;
        #[cfg(not(target_arch = "wasm32"))]
        let derived = derived
            .with_additional_address(shielded_address)
            .map_err(|_| WalletAccountPortError::InvalidData)?;
        Ok(derived)
    }
}

/// Midnight adapter with profile-scoped network selection and replaceable data source.
pub struct MidnightWalletAdapter<S, D = UnavailableMidnightAccountDeriver> {
    source: S,
    deriver: D,
    selections: Mutex<HashMap<WalletProfileId, ChainNetworkId>>,
    account_coordinates: Mutex<HashMap<(WalletProfileId, ChainNetworkId), (u32, u32)>>,
    hydrated_profiles: Mutex<HashSet<WalletProfileId>>,
    association_repository: Option<Arc<dyn WalletProfileAssociationRepository>>,
    default_network: Option<ChainNetworkId>,
    #[cfg(not(target_arch = "wasm32"))]
    completer: Arc<dyn transaction::MidnightTransactionCompleter>,
    #[cfg(not(target_arch = "wasm32"))]
    dust_sync: Arc<dyn dust_sync::MidnightDustSyncController>,
    #[cfg(not(target_arch = "wasm32"))]
    shielded_sync: Arc<dyn shielded_sync::MidnightShieldedSyncController>,
    #[cfg(not(target_arch = "wasm32"))]
    submission_journal: Arc<dyn submission_journal::MidnightSubmissionJournalStore>,
    #[cfg(not(target_arch = "wasm32"))]
    submission_reconciler: Arc<dyn transaction::MidnightSubmissionReconciler>,
    #[cfg(not(target_arch = "wasm32"))]
    drafts: Arc<
        Mutex<
            HashMap<
                (
                    WalletProfileId,
                    oxid_wallet_domain::WalletTransactionDraftId,
                ),
                transaction::RetainedMidnightDraft,
            >,
        >,
    >,
    #[cfg(not(target_arch = "wasm32"))]
    contract_call_submissions: transaction::RetainedContractCallSubmissions,
}

impl<S> MidnightWalletAdapter<S, UnavailableMidnightAccountDeriver> {
    #[must_use]
    pub fn new(source: S) -> Self {
        Self {
            source,
            deriver: UnavailableMidnightAccountDeriver,
            selections: Mutex::new(HashMap::new()),
            account_coordinates: Mutex::new(HashMap::new()),
            hydrated_profiles: Mutex::new(HashSet::new()),
            association_repository: None,
            default_network: None,
            #[cfg(not(target_arch = "wasm32"))]
            completer: Arc::new(transaction::UnavailableMidnightTransactionCompleter),
            #[cfg(not(target_arch = "wasm32"))]
            dust_sync: Arc::new(dust_sync::UnavailableMidnightDustSyncController),
            #[cfg(not(target_arch = "wasm32"))]
            shielded_sync: Arc::new(shielded_sync::UnavailableMidnightShieldedSyncController),
            #[cfg(not(target_arch = "wasm32"))]
            submission_journal: Arc::new(
                submission_journal::UnavailableMidnightSubmissionJournalStore,
            ),
            #[cfg(not(target_arch = "wasm32"))]
            submission_reconciler: Arc::new(transaction::UnavailableMidnightSubmissionReconciler),
            #[cfg(not(target_arch = "wasm32"))]
            drafts: Arc::new(Mutex::new(HashMap::new())),
            contract_call_submissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Uses an explicitly configured initial network while preserving
    /// profile-scoped selection after the first user choice.
    #[must_use]
    pub fn with_default_network(source: S, default_network: ChainNetworkId) -> Self {
        Self {
            source,
            deriver: UnavailableMidnightAccountDeriver,
            selections: Mutex::new(HashMap::new()),
            account_coordinates: Mutex::new(HashMap::new()),
            hydrated_profiles: Mutex::new(HashSet::new()),
            association_repository: None,
            default_network: Some(default_network),
            #[cfg(not(target_arch = "wasm32"))]
            completer: Arc::new(transaction::UnavailableMidnightTransactionCompleter),
            #[cfg(not(target_arch = "wasm32"))]
            dust_sync: Arc::new(dust_sync::UnavailableMidnightDustSyncController),
            #[cfg(not(target_arch = "wasm32"))]
            shielded_sync: Arc::new(shielded_sync::UnavailableMidnightShieldedSyncController),
            #[cfg(not(target_arch = "wasm32"))]
            submission_journal: Arc::new(
                submission_journal::UnavailableMidnightSubmissionJournalStore,
            ),
            #[cfg(not(target_arch = "wasm32"))]
            submission_reconciler: Arc::new(transaction::UnavailableMidnightSubmissionReconciler),
            #[cfg(not(target_arch = "wasm32"))]
            drafts: Arc::new(Mutex::new(HashMap::new())),
            contract_call_submissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<S, D> MidnightWalletAdapter<S, D> {
    /// Persists only public, derivable profile/account coordinates. Protected
    /// key handles and rendered addresses remain in custody and read models.
    #[must_use]
    pub fn with_profile_association_repository(
        mut self,
        repository: Arc<dyn WalletProfileAssociationRepository>,
    ) -> Self {
        self.association_repository = Some(repository);
        self
    }

    fn hydrate_associations(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<(), WalletAccountPortError> {
        if self
            .hydrated_profiles
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .contains(profile_id)
        {
            return Ok(());
        }
        let Some(repository) = &self.association_repository else {
            return Ok(());
        };
        let associations = repository
            .load_associations(profile_id)
            .map_err(map_association_error)?;
        if let Some(associations) = associations {
            self.selections
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)?
                .entry(profile_id.clone())
                .or_insert_with(|| associations.selected_network_id().clone());
            let mut coordinates = self
                .account_coordinates
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)?;
            for account in associations.accounts() {
                coordinates
                    .entry((profile_id.clone(), account.network_id().clone()))
                    .or_insert((account.account_index(), account.address_index()));
            }
        }
        self.hydrated_profiles
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .insert(profile_id.clone());
        Ok(())
    }

    fn persist_associations(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<(), WalletAccountPortError> {
        let Some(repository) = &self.association_repository else {
            return Ok(());
        };
        let selected = self
            .selections
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .get(profile_id)
            .cloned()
            .or_else(|| self.default_network.clone())
            .map_or_else(|| network_id(DEFAULT_NETWORK_ID), Ok)?;
        let accounts = self
            .account_coordinates
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .iter()
            .filter(|((profile, _), _)| profile == profile_id)
            .map(|((_, network), (account_index, address_index))| {
                WalletAccountAssociation::new(network.clone(), *account_index, *address_index)
                    .map_err(|_| WalletAccountPortError::InvalidData)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let associations = WalletProfileAssociations::new(selected, accounts)
            .map_err(|_| WalletAccountPortError::InvalidData)?;
        repository
            .save_associations(profile_id, associations)
            .map_err(map_association_error)
    }

    #[must_use]
    pub fn with_deriver(source: S, deriver: D) -> Self {
        Self {
            source,
            deriver,
            selections: Mutex::new(HashMap::new()),
            account_coordinates: Mutex::new(HashMap::new()),
            hydrated_profiles: Mutex::new(HashSet::new()),
            association_repository: None,
            default_network: None,
            #[cfg(not(target_arch = "wasm32"))]
            completer: Arc::new(transaction::UnavailableMidnightTransactionCompleter),
            #[cfg(not(target_arch = "wasm32"))]
            dust_sync: Arc::new(dust_sync::UnavailableMidnightDustSyncController),
            #[cfg(not(target_arch = "wasm32"))]
            shielded_sync: Arc::new(shielded_sync::UnavailableMidnightShieldedSyncController),
            #[cfg(not(target_arch = "wasm32"))]
            submission_journal: Arc::new(
                submission_journal::MemoryMidnightSubmissionJournalStore::default(),
            ),
            #[cfg(not(target_arch = "wasm32"))]
            submission_reconciler: Arc::new(transaction::UnavailableMidnightSubmissionReconciler),
            #[cfg(not(target_arch = "wasm32"))]
            drafts: Arc::new(Mutex::new(HashMap::new())),
            contract_call_submissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[must_use]
    pub fn with_default_network_and_deriver(
        source: S,
        default_network: ChainNetworkId,
        deriver: D,
    ) -> Self {
        Self {
            source,
            deriver,
            selections: Mutex::new(HashMap::new()),
            account_coordinates: Mutex::new(HashMap::new()),
            hydrated_profiles: Mutex::new(HashSet::new()),
            association_repository: None,
            default_network: Some(default_network),
            #[cfg(not(target_arch = "wasm32"))]
            completer: Arc::new(transaction::UnavailableMidnightTransactionCompleter),
            #[cfg(not(target_arch = "wasm32"))]
            dust_sync: Arc::new(dust_sync::UnavailableMidnightDustSyncController),
            #[cfg(not(target_arch = "wasm32"))]
            shielded_sync: Arc::new(shielded_sync::UnavailableMidnightShieldedSyncController),
            #[cfg(not(target_arch = "wasm32"))]
            submission_journal: Arc::new(
                submission_journal::MemoryMidnightSubmissionJournalStore::default(),
            ),
            #[cfg(not(target_arch = "wasm32"))]
            submission_reconciler: Arc::new(transaction::UnavailableMidnightSubmissionReconciler),
            #[cfg(not(target_arch = "wasm32"))]
            drafts: Arc::new(Mutex::new(HashMap::new())),
            contract_call_submissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn with_default_network_deriver_and_completer(
        source: S,
        default_network: ChainNetworkId,
        deriver: D,
        completer: Arc<dyn transaction::MidnightTransactionCompleter>,
    ) -> Self {
        Self {
            source,
            deriver,
            selections: Mutex::new(HashMap::new()),
            account_coordinates: Mutex::new(HashMap::new()),
            hydrated_profiles: Mutex::new(HashSet::new()),
            association_repository: None,
            default_network: Some(default_network),
            completer,
            dust_sync: Arc::new(dust_sync::UnavailableMidnightDustSyncController),
            shielded_sync: Arc::new(shielded_sync::UnavailableMidnightShieldedSyncController),
            submission_journal: Arc::new(
                submission_journal::MemoryMidnightSubmissionJournalStore::default(),
            ),
            submission_reconciler: Arc::new(transaction::UnavailableMidnightSubmissionReconciler),
            drafts: Arc::new(Mutex::new(HashMap::new())),
            contract_call_submissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn with_deriver_and_completer(
        source: S,
        deriver: D,
        completer: Arc<dyn transaction::MidnightTransactionCompleter>,
    ) -> Self {
        Self {
            source,
            deriver,
            selections: Mutex::new(HashMap::new()),
            account_coordinates: Mutex::new(HashMap::new()),
            hydrated_profiles: Mutex::new(HashSet::new()),
            association_repository: None,
            default_network: None,
            completer,
            dust_sync: Arc::new(dust_sync::UnavailableMidnightDustSyncController),
            shielded_sync: Arc::new(shielded_sync::UnavailableMidnightShieldedSyncController),
            submission_journal: Arc::new(
                submission_journal::MemoryMidnightSubmissionJournalStore::default(),
            ),
            submission_reconciler: Arc::new(transaction::UnavailableMidnightSubmissionReconciler),
            drafts: Arc::new(Mutex::new(HashMap::new())),
            contract_call_submissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn selected(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<ChainNetworkId, WalletAccountPortError> {
        self.hydrate_associations(profile_id)?;
        let selections = self
            .selections
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?;
        selections
            .get(profile_id)
            .cloned()
            .or_else(|| self.default_network.clone())
            .map_or_else(|| network_id(DEFAULT_NETWORK_ID), Ok)
    }

    fn account_index(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<u32, WalletDustSyncPortError> {
        self.hydrate_associations(profile_id)
            .map_err(|_| WalletDustSyncPortError::Unavailable)?;
        self.account_coordinates
            .lock()
            .map_err(|_| WalletDustSyncPortError::Unavailable)
            .map(|indices| {
                indices
                    .get(&(profile_id.clone(), network_id.clone()))
                    .map(|(account_index, _)| *account_index)
                    .unwrap_or(0)
            })
    }

    fn shielded_account_index(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<u32, WalletShieldedSyncPortError> {
        self.hydrate_associations(profile_id)
            .map_err(|_| WalletShieldedSyncPortError::Unavailable)?;
        self.account_coordinates
            .lock()
            .map_err(|_| WalletShieldedSyncPortError::Unavailable)
            .map(|indices| {
                indices
                    .get(&(profile_id.clone(), network_id.clone()))
                    .map(|(account_index, _)| *account_index)
                    .unwrap_or(0)
            })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn with_dust_sync(mut self, dust_sync: Arc<dyn dust_sync::MidnightDustSyncController>) -> Self {
        self.dust_sync = dust_sync;
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn with_shielded_sync(
        mut self,
        shielded_sync: Arc<dyn shielded_sync::MidnightShieldedSyncController>,
    ) -> Self {
        self.shielded_sync = shielded_sync;
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn with_submission_recovery(
        mut self,
        journal: Arc<dyn submission_journal::MidnightSubmissionJournalStore>,
        reconciler: Arc<dyn transaction::MidnightSubmissionReconciler>,
    ) -> Self {
        self.submission_journal = journal;
        self.submission_reconciler = reconciler;
        self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S, D> WalletDustSyncPort for MidnightWalletAdapter<S, D>
where
    S: Send + Sync,
    D: Send + Sync,
{
    fn dust_status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletDustSyncSnapshot, WalletDustSyncPortError> {
        let network = self
            .selected(profile_id)
            .map_err(map_account_to_dust_error)?;
        self.dust_sync.status(profile_id, &network)
    }

    fn start_dust_sync(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletDustSyncSnapshot, WalletDustSyncPortError> {
        let network = self
            .selected(profile_id)
            .map_err(map_account_to_dust_error)?;
        let account_index = self.account_index(profile_id, &network)?;
        self.dust_sync.start(profile_id, &network, account_index)
    }

    fn cancel_dust_sync(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletDustSyncSnapshot, WalletDustSyncPortError> {
        let network = self
            .selected(profile_id)
            .map_err(map_account_to_dust_error)?;
        self.dust_sync.cancel(profile_id, &network)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S, D> WalletShieldedSyncPort for MidnightWalletAdapter<S, D>
where
    S: Send + Sync,
    D: Send + Sync,
{
    fn shielded_status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        let network = self
            .selected(profile_id)
            .map_err(map_account_to_shielded_error)?;
        self.shielded_sync.status(profile_id, &network)
    }

    fn start_shielded_sync(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        let network = self
            .selected(profile_id)
            .map_err(map_account_to_shielded_error)?;
        let account_index = self.shielded_account_index(profile_id, &network)?;
        self.shielded_sync
            .start(profile_id, &network, account_index)
    }

    fn cancel_shielded_sync(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        let network = self
            .selected(profile_id)
            .map_err(map_account_to_shielded_error)?;
        self.shielded_sync.cancel(profile_id, &network)
    }
}

#[cfg(target_arch = "wasm32")]
impl<S, D> WalletDustSyncPort for MidnightWalletAdapter<S, D>
where
    S: Send + Sync,
    D: Send + Sync,
{
    fn dust_status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletDustSyncSnapshot, WalletDustSyncPortError> {
        let network = self
            .selected(profile_id)
            .map_err(map_account_to_dust_error)?;
        Ok(oxid_wallet_domain::WalletDustSyncSnapshot::unavailable(
            network,
        ))
    }

    fn start_dust_sync(
        &self,
        _: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletDustSyncSnapshot, WalletDustSyncPortError> {
        Err(WalletDustSyncPortError::Unavailable)
    }

    fn cancel_dust_sync(
        &self,
        _: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletDustSyncSnapshot, WalletDustSyncPortError> {
        Err(WalletDustSyncPortError::Unavailable)
    }
}

#[cfg(target_arch = "wasm32")]
impl<S, D> WalletShieldedSyncPort for MidnightWalletAdapter<S, D>
where
    S: Send + Sync,
    D: Send + Sync,
{
    fn shielded_status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        let network = self
            .selected(profile_id)
            .map_err(map_account_to_shielded_error)?;
        Ok(oxid_wallet_domain::WalletShieldedSyncSnapshot::unavailable(
            network,
        ))
    }

    fn start_shielded_sync(
        &self,
        _: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        Err(WalletShieldedSyncPortError::Unavailable)
    }

    fn cancel_shielded_sync(
        &self,
        _: &WalletProfileId,
    ) -> Result<oxid_wallet_domain::WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        Err(WalletShieldedSyncPortError::Unavailable)
    }
}

#[cfg(target_arch = "wasm32")]
impl<S, D> oxid_wallet_application::WalletTransactionPort for MidnightWalletAdapter<S, D>
where
    S: Send + Sync,
    D: Send + Sync,
{
    fn prepare(
        &self,
        _: &WalletProfileId,
        _: oxid_wallet_application::PrepareWalletTransferRequest,
    ) -> Result<
        oxid_wallet_domain::WalletTransferPreview,
        oxid_wallet_application::WalletTransactionPortError,
    > {
        Err(oxid_wallet_application::WalletTransactionPortError::Unavailable)
    }

    fn authorize(
        &self,
        _: &WalletProfileId,
        _: oxid_wallet_application::AuthorizeWalletTransferRequest,
    ) -> Result<
        oxid_wallet_domain::WalletTransferPreview,
        oxid_wallet_application::WalletTransactionPortError,
    > {
        Err(oxid_wallet_application::WalletTransactionPortError::Unavailable)
    }

    fn submit<'a>(
        &'a self,
        _: &'a WalletProfileId,
        _: oxid_wallet_application::SubmitWalletTransferRequest,
    ) -> oxid_wallet_application::WalletTransactionPortFuture<'a> {
        Box::pin(async { Err(oxid_wallet_application::WalletTransactionPortError::Unavailable) })
    }

    fn get(
        &self,
        _: &WalletProfileId,
        _: &oxid_wallet_domain::WalletTransactionDraftId,
        _: oxid_foundation::UnixTimestampMillis,
    ) -> Result<
        oxid_wallet_domain::WalletTransferPreview,
        oxid_wallet_application::WalletTransactionPortError,
    > {
        Err(oxid_wallet_application::WalletTransactionPortError::Unavailable)
    }

    fn submission_status(
        &self,
        _: &WalletProfileId,
        _: &oxid_wallet_domain::WalletTransactionDraftId,
    ) -> Result<
        oxid_wallet_domain::WalletTransactionSubmissionStatus,
        oxid_wallet_application::WalletTransactionPortError,
    > {
        Err(oxid_wallet_application::WalletTransactionPortError::Unavailable)
    }

    fn cancel_submission(
        &self,
        _: &WalletProfileId,
        _: &oxid_wallet_domain::WalletTransactionDraftId,
    ) -> Result<
        oxid_wallet_domain::WalletTransactionSubmissionStatus,
        oxid_wallet_application::WalletTransactionPortError,
    > {
        Err(oxid_wallet_application::WalletTransactionPortError::Unavailable)
    }

    fn submission_history(
        &self,
        _: &WalletProfileId,
    ) -> Result<
        Vec<oxid_wallet_domain::WalletTransactionSubmissionStatus>,
        oxid_wallet_application::WalletTransactionPortError,
    > {
        Ok(Vec::new())
    }

    fn reconcile_submission<'a>(
        &'a self,
        _: &'a WalletProfileId,
        _: &'a oxid_wallet_domain::WalletTransactionDraftId,
    ) -> oxid_wallet_application::WalletTransactionStatusPortFuture<'a> {
        Box::pin(async { Err(oxid_wallet_application::WalletTransactionPortError::Unavailable) })
    }
}

impl<S, D> WalletNetworkPort for MidnightWalletAdapter<S, D>
where
    S: MidnightAccountSource,
    D: Send + Sync,
{
    fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
        network_catalog()
    }

    fn selected_network(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<ChainNetworkId, WalletAccountPortError> {
        self.selected(profile_id)
    }

    fn select_network(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<ChainNetworkId, WalletAccountPortError> {
        if network_by_id(network_id)?.is_none() {
            return Err(WalletAccountPortError::UnsupportedNetwork);
        }
        self.hydrate_associations(profile_id)?;
        let previous = self
            .selections
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .insert(profile_id.clone(), network_id.clone());
        if let Err(error) = self.persist_associations(profile_id) {
            let mut selections = self
                .selections
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)?;
            if let Some(previous) = previous {
                selections.insert(profile_id.clone(), previous);
            } else {
                selections.remove(profile_id);
            }
            return Err(error);
        }
        Ok(network_id.clone())
    }
}

impl<S, D> MidnightWalletAdapter<S, D>
where
    S: MidnightAccountSource,
    D: MidnightAccountDeriver,
{
    fn ensure_associated_account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<(), WalletAccountPortError> {
        self.hydrate_associations(profile_id)?;
        let coordinates = self
            .account_coordinates
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .get(&(profile_id.clone(), network.id().clone()))
            .copied();
        let Some((account_index, address_index)) = coordinates else {
            return Ok(());
        };
        let derived = self
            .deriver
            .derive(profile_id, network, account_index, address_index)?;
        self.source
            .bind_derived_account(profile_id, network, &derived)
    }
}

impl<S, D> MidnightPublicCallContextSource for MidnightWalletAdapter<S, D>
where
    S: MidnightAccountSource,
    D: MidnightAccountDeriver,
{
    fn public_call_context(
        &self,
        profile_id: &str,
    ) -> Result<MidnightPublicCallContext, WalletAccountPortError> {
        let profile_id = WalletProfileId::parse(profile_id.to_owned())
            .map_err(|_| WalletAccountPortError::InvalidData)?;
        let network_id = self.selected(&profile_id)?;
        let network =
            network_by_id(&network_id)?.ok_or(WalletAccountPortError::UnsupportedNetwork)?;
        self.ensure_associated_account(&profile_id, &network)?;
        let account = self.source.account(&profile_id, &network)?;
        if account.network().id() != &network_id {
            return Err(WalletAccountPortError::InvalidData);
        }
        let unshielded = account
            .addresses()
            .iter()
            .filter(|address| address.kind() == ChainAddressKind::Unshielded)
            .collect::<Vec<_>>();
        let shielded = account
            .addresses()
            .iter()
            .filter(|address| address.kind() == ChainAddressKind::Shielded)
            .collect::<Vec<_>>();
        if unshielded.len() != 1 || shielded.len() != 1 {
            return Err(WalletAccountPortError::NotFound);
        }
        let unshielded_recipient =
            decode_midnight_address_payload(unshielded[0], &network_id, "addr", 32)?
                .try_into()
                .map_err(|_| WalletAccountPortError::InvalidData)?;
        let shielded_payload =
            decode_midnight_address_payload(shielded[0], &network_id, "shield-addr", 64)?;
        let coin_public_key = shielded_payload[..32]
            .try_into()
            .map_err(|_| WalletAccountPortError::InvalidData)?;
        let encryption_public_key = shielded_payload[32..]
            .try_into()
            .map_err(|_| WalletAccountPortError::InvalidData)?;
        Ok(MidnightPublicCallContext {
            network_id,
            coin_public_key,
            encryption_public_key,
            unshielded_recipient,
        })
    }
}

impl<S, D> WalletAccountReadPort for MidnightWalletAdapter<S, D>
where
    S: MidnightAccountSource,
    D: MidnightAccountDeriver,
{
    fn account(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
        let selected = self.selected(profile_id)?;
        let network =
            network_by_id(&selected)?.ok_or(WalletAccountPortError::UnsupportedNetwork)?;
        self.ensure_associated_account(profile_id, &network)?;
        self.source.account(profile_id, &network)
    }

    fn sync<'a>(&'a self, profile_id: &'a WalletProfileId) -> WalletAccountPortFuture<'a> {
        Box::pin(async move {
            let selected = self.selected(profile_id)?;
            let network =
                network_by_id(&selected)?.ok_or(WalletAccountPortError::UnsupportedNetwork)?;
            self.ensure_associated_account(profile_id, &network)?;
            self.source.sync(profile_id, &network).await
        })
    }
}

impl<S, D> WalletAccountDerivationPort for MidnightWalletAdapter<S, D>
where
    S: MidnightAccountSource,
    D: MidnightAccountDeriver,
{
    fn derive_account(
        &self,
        profile_id: &WalletProfileId,
        account_index: u32,
        address_index: u32,
    ) -> Result<DerivedChainAccount, WalletAccountPortError> {
        let selected = self.selected(profile_id)?;
        let network =
            network_by_id(&selected)?.ok_or(WalletAccountPortError::UnsupportedNetwork)?;
        let derived = self
            .deriver
            .derive(profile_id, &network, account_index, address_index)?;
        self.hydrate_associations(profile_id)?;
        let key = (profile_id.clone(), selected);
        let previous = self
            .account_coordinates
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .insert(key.clone(), (account_index, address_index));
        if let Err(error) = self.persist_associations(profile_id) {
            let mut coordinates = self
                .account_coordinates
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)?;
            if let Some(previous) = previous {
                coordinates.insert(key, previous);
            } else {
                coordinates.remove(&key);
            }
            return Err(error);
        }
        self.source
            .bind_derived_account(profile_id, &network, &derived)?;
        Ok(derived)
    }
}

/// Fail-closed account source used by production composition before custody is wired.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableMidnightAccountSource;

impl MidnightAccountSource for UnavailableMidnightAccountSource {
    fn account(
        &self,
        _: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
        Ok(WalletAccountSnapshot::unavailable(network.clone()))
    }

    fn sync<'a>(
        &'a self,
        _: &'a WalletProfileId,
        network: &'a ChainNetwork,
    ) -> WalletAccountPortFuture<'a> {
        Box::pin(async move { Ok(WalletAccountSnapshot::unavailable(network.clone())) })
    }
}

/// Deterministic public account simulator for headless flow conformance.
pub struct SimulatedMidnightAccountSource<C> {
    clock: Arc<C>,
    synchronized: Mutex<HashSet<(WalletProfileId, ChainNetworkId)>>,
    derived_accounts: Mutex<HashMap<(WalletProfileId, ChainNetworkId), DerivedChainAccount>>,
}

impl<C> SimulatedMidnightAccountSource<C> {
    #[must_use]
    pub fn new(clock: Arc<C>) -> Self {
        Self {
            clock,
            synchronized: Mutex::new(HashSet::new()),
            derived_accounts: Mutex::new(HashMap::new()),
        }
    }

    fn snapshot(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
        synchronized: bool,
    ) -> Result<WalletAccountSnapshot, WalletAccountPortError>
    where
        C: ClockPort,
    {
        let derived = self
            .derived_accounts
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .get(&(profile_id.clone(), network.id().clone()))
            .cloned();
        let (account_id, addresses) = match derived {
            Some(derived) => (derived.account_id().clone(), derived.addresses().to_vec()),
            None => (
                ChainAccountId::parse(profile_id.as_str().to_owned())
                    .map_err(|_| WalletAccountPortError::InvalidData)?,
                fixture_addresses(network.id())?,
            ),
        };
        let sync = if synchronized {
            WalletSyncStatus::new(
                WalletSyncState::Synced,
                Some(2),
                Some(2),
                Some(42),
                Some(
                    self.clock
                        .now()
                        .map_err(|_| WalletAccountPortError::Unavailable)?,
                ),
            )
        } else {
            WalletSyncStatus::new(WalletSyncState::NeverSynced, None, None, None, None)
        };
        let (balances, transactions) = if synchronized {
            simulated_ledger_state(
                self.clock
                    .now()
                    .map_err(|_| WalletAccountPortError::Unavailable)?,
            )?
        } else {
            (Vec::new(), Vec::new())
        };

        Ok(WalletAccountSnapshot::new(
            network.clone(),
            Some(account_id),
            WalletAccountSource::Simulated,
            addresses,
            balances,
            sync,
            transactions,
        ))
    }

    fn is_synchronized(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<bool, WalletAccountPortError> {
        self.synchronized
            .lock()
            .map(|entries| entries.contains(&(profile_id.clone(), network_id.clone())))
            .map_err(|_| WalletAccountPortError::Unavailable)
    }
}

impl<C> MidnightAccountSource for SimulatedMidnightAccountSource<C>
where
    C: ClockPort + 'static,
{
    fn bind_derived_account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
        derived: &DerivedChainAccount,
    ) -> Result<(), WalletAccountPortError> {
        if derived.network_id() != network.id() {
            return Err(WalletAccountPortError::InvalidData);
        }
        let key = (profile_id.clone(), network.id().clone());
        let mut accounts = self
            .derived_accounts
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?;
        if accounts.get(&key) != Some(derived) {
            accounts.insert(key.clone(), derived.clone());
            drop(accounts);
            self.synchronized
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)?
                .remove(&key);
        }
        Ok(())
    }

    fn account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
        self.snapshot(
            profile_id,
            network,
            self.is_synchronized(profile_id, network.id())?,
        )
    }

    fn sync<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        network: &'a ChainNetwork,
    ) -> WalletAccountPortFuture<'a> {
        Box::pin(async move {
            self.synchronized
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)?
                .insert((profile_id.clone(), network.id().clone()));
            self.snapshot(profile_id, network, true)
        })
    }
}

/// Production-facing adapter: network catalog is available, account state is not.
#[must_use]
pub fn unavailable_midnight_wallet() -> MidnightWalletAdapter<UnavailableMidnightAccountSource> {
    MidnightWalletAdapter::new(UnavailableMidnightAccountSource)
}

/// Development-only adapter with public simulated account state.
#[must_use]
pub fn simulated_midnight_wallet<C>(
    clock: Arc<C>,
) -> MidnightWalletAdapter<SimulatedMidnightAccountSource<C>>
where
    C: ClockPort,
{
    MidnightWalletAdapter::new(SimulatedMidnightAccountSource::new(clock))
}

/// Development-only adapter that binds protected HD-derived public accounts.
#[must_use]
pub fn protected_simulated_midnight_wallet<C, K>(
    clock: Arc<C>,
    keys: Arc<K>,
) -> MidnightWalletAdapter<SimulatedMidnightAccountSource<C>, ProtectedMidnightAccountDeriver<K>>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + WalletKeyDerivationPort + WalletKeyOperationPort + 'static,
{
    let dust_sync = Arc::new(dust_sync::SimulatedMidnightDustSyncController::new(
        Arc::clone(&clock),
        Arc::clone(&keys),
    ));
    let shielded_sync = Arc::new(shielded_sync::SimulatedMidnightShieldedSyncController::new(
        Arc::clone(&clock),
        Arc::clone(&keys),
    ));
    MidnightWalletAdapter::with_deriver_and_completer(
        SimulatedMidnightAccountSource::new(clock),
        ProtectedMidnightAccountDeriver::new(keys),
        Arc::new(transaction::SimulatedMidnightTransactionCompleter),
    )
    .with_dust_sync(dust_sync)
    .with_shielded_sync(shielded_sync)
}

/// Development-only simulated adapter with a durable public submission journal.
#[must_use]
pub fn protected_simulated_midnight_wallet_with_submission_journal<C, K>(
    journal: MidnightSubmissionJournalConfig,
    clock: Arc<C>,
    keys: Arc<K>,
) -> MidnightWalletAdapter<SimulatedMidnightAccountSource<C>, ProtectedMidnightAccountDeriver<K>>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + WalletKeyDerivationPort + WalletKeyOperationPort + 'static,
{
    protected_simulated_midnight_wallet(clock, keys).with_submission_recovery(
        Arc::new(submission_journal::JsonMidnightSubmissionJournalStore::new(
            journal,
        )),
        Arc::new(transaction::UnavailableMidnightSubmissionReconciler),
    )
}

/// Development-only live standalone adapter with real DUST proving and node submission.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn protected_standalone_midnight_wallet<C, K>(
    config: MidnightStandaloneConfig,
    clock: Arc<C>,
    keys: Arc<K>,
) -> MidnightWalletAdapter<LiveMidnightAccountSource<C>, ProtectedMidnightAccountDeriver<K>>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + WalletKeyDerivationPort + WalletKeyOperationPort + 'static,
{
    let indexer = config.indexer().clone();
    let default_network = indexer.network_id().clone();
    let dust_store = Arc::new(dust_checkpoint::UnavailableMidnightDustCheckpointStore);
    let dust_sync = Arc::new(dust_sync::LiveMidnightDustSyncController::new(
        config.clone(),
        dust_store,
        Arc::clone(&clock),
        Arc::clone(&keys),
    ));
    let shielded_sync =
        live_shielded_controller(indexer.clone(), Arc::clone(&clock), Arc::clone(&keys));
    MidnightWalletAdapter::with_default_network_deriver_and_completer(
        LiveMidnightAccountSource::new(indexer, Arc::clone(&clock)),
        default_network,
        ProtectedMidnightAccountDeriver::new(keys),
        Arc::new(submission::LiveMidnightTransactionCompleter::new(
            config, clock,
        )),
    )
    .with_dust_sync(dust_sync)
    .with_shielded_sync(shielded_sync)
}

/// Development-only standalone adapter with durable public account checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn protected_standalone_midnight_wallet_with_checkpoints<C, K>(
    config: MidnightStandaloneConfig,
    checkpoints: MidnightAccountCheckpointConfig,
    clock: Arc<C>,
    keys: Arc<K>,
) -> MidnightWalletAdapter<LiveMidnightAccountSource<C>, ProtectedMidnightAccountDeriver<K>>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + WalletKeyDerivationPort + WalletKeyOperationPort + 'static,
{
    let indexer = config.indexer().clone();
    let default_network = indexer.network_id().clone();
    let dust_store = Arc::new(dust_checkpoint::UnavailableMidnightDustCheckpointStore);
    let dust_sync = Arc::new(dust_sync::LiveMidnightDustSyncController::new(
        config.clone(),
        dust_store,
        Arc::clone(&clock),
        Arc::clone(&keys),
    ));
    let shielded_sync =
        live_shielded_controller(indexer.clone(), Arc::clone(&clock), Arc::clone(&keys));
    MidnightWalletAdapter::with_default_network_deriver_and_completer(
        LiveMidnightAccountSource::new_with_checkpoints(indexer, checkpoints, Arc::clone(&clock)),
        default_network,
        ProtectedMidnightAccountDeriver::new(keys),
        Arc::new(submission::LiveMidnightTransactionCompleter::new(
            config, clock,
        )),
    )
    .with_dust_sync(dust_sync)
    .with_shielded_sync(shielded_sync)
}

/// Development-only standalone adapter with key-scoped private DUST checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn protected_standalone_midnight_wallet_with_dust_checkpoints<C, K>(
    config: MidnightStandaloneConfig,
    dust_checkpoints: MidnightDustCheckpointConfig,
    clock: Arc<C>,
    keys: Arc<K>,
) -> MidnightWalletAdapter<LiveMidnightAccountSource<C>, ProtectedMidnightAccountDeriver<K>>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + WalletKeyDerivationPort + WalletKeyOperationPort + 'static,
{
    let indexer = config.indexer().clone();
    let default_network = indexer.network_id().clone();
    let dust_store: Arc<dyn dust_checkpoint::MidnightDustCheckpointStore> = Arc::new(
        dust_checkpoint::BinaryMidnightDustCheckpointStore::new(dust_checkpoints),
    );
    let dust_sync = Arc::new(dust_sync::LiveMidnightDustSyncController::new(
        config.clone(),
        Arc::clone(&dust_store),
        Arc::clone(&clock),
        Arc::clone(&keys),
    ));
    let shielded_sync =
        live_shielded_controller(indexer.clone(), Arc::clone(&clock), Arc::clone(&keys));
    MidnightWalletAdapter::with_default_network_deriver_and_completer(
        LiveMidnightAccountSource::new(indexer, Arc::clone(&clock)),
        default_network,
        ProtectedMidnightAccountDeriver::new(keys),
        Arc::new(
            submission::LiveMidnightTransactionCompleter::new_with_dust_store(
                config, dust_store, clock,
            ),
        ),
    )
    .with_dust_sync(dust_sync)
    .with_shielded_sync(shielded_sync)
}

/// Development-only standalone adapter with public account and private DUST checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn protected_standalone_midnight_wallet_with_all_checkpoints<C, K>(
    config: MidnightStandaloneConfig,
    account_checkpoints: MidnightAccountCheckpointConfig,
    dust_checkpoints: MidnightDustCheckpointConfig,
    clock: Arc<C>,
    keys: Arc<K>,
) -> MidnightWalletAdapter<LiveMidnightAccountSource<C>, ProtectedMidnightAccountDeriver<K>>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + WalletKeyDerivationPort + WalletKeyOperationPort + 'static,
{
    let indexer = config.indexer().clone();
    let default_network = indexer.network_id().clone();
    let dust_store: Arc<dyn dust_checkpoint::MidnightDustCheckpointStore> = Arc::new(
        dust_checkpoint::BinaryMidnightDustCheckpointStore::new(dust_checkpoints),
    );
    let dust_sync = Arc::new(dust_sync::LiveMidnightDustSyncController::new(
        config.clone(),
        Arc::clone(&dust_store),
        Arc::clone(&clock),
        Arc::clone(&keys),
    ));
    let shielded_sync =
        live_shielded_controller(indexer.clone(), Arc::clone(&clock), Arc::clone(&keys));
    MidnightWalletAdapter::with_default_network_deriver_and_completer(
        LiveMidnightAccountSource::new_with_checkpoints(
            indexer,
            account_checkpoints,
            Arc::clone(&clock),
        ),
        default_network,
        ProtectedMidnightAccountDeriver::new(keys),
        Arc::new(
            submission::LiveMidnightTransactionCompleter::new_with_dust_store(
                config, dust_store, clock,
            ),
        ),
    )
    .with_dust_sync(dust_sync)
    .with_shielded_sync(shielded_sync)
}

/// Wires any reviewed combination of standalone account, DUST, and shielded
/// checkpoint stores without changing the public application boundary.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn protected_standalone_midnight_wallet_with_checkpoint_options<C, K>(
    config: MidnightStandaloneConfig,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    dust_checkpoints: Option<MidnightDustCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    submission_journal: Option<MidnightSubmissionJournalConfig>,
    clock: Arc<C>,
    keys: Arc<K>,
) -> MidnightWalletAdapter<LiveMidnightAccountSource<C>, ProtectedMidnightAccountDeriver<K>>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + WalletKeyDerivationPort + WalletKeyOperationPort + 'static,
{
    let indexer = config.indexer().clone();
    let default_network = indexer.network_id().clone();
    let source = account_checkpoints.map_or_else(
        || LiveMidnightAccountSource::new(indexer.clone(), Arc::clone(&clock)),
        |checkpoints| {
            LiveMidnightAccountSource::new_with_checkpoints(
                indexer.clone(),
                checkpoints,
                Arc::clone(&clock),
            )
        },
    );
    let dust_store: Arc<dyn dust_checkpoint::MidnightDustCheckpointStore> = dust_checkpoints
        .map_or_else(
            || Arc::new(dust_checkpoint::UnavailableMidnightDustCheckpointStore) as Arc<_>,
            |checkpoints| {
                Arc::new(dust_checkpoint::BinaryMidnightDustCheckpointStore::new(
                    checkpoints,
                )) as Arc<_>
            },
        );
    let shielded_store: Arc<dyn shielded_checkpoint::MidnightShieldedCheckpointStore> =
        shielded_checkpoints.map_or_else(
            || Arc::new(shielded_checkpoint::UnavailableMidnightShieldedCheckpointStore) as Arc<_>,
            |checkpoints| {
                Arc::new(
                    shielded_checkpoint::BinaryMidnightShieldedCheckpointStore::new(checkpoints),
                ) as Arc<_>
            },
        );
    let submission_store: Arc<dyn submission_journal::MidnightSubmissionJournalStore> =
        submission_journal.map_or_else(
            || {
                Arc::new(submission_journal::MemoryMidnightSubmissionJournalStore::default())
                    as Arc<_>
            },
            |journal| {
                Arc::new(submission_journal::JsonMidnightSubmissionJournalStore::new(
                    journal,
                )) as Arc<_>
            },
        );
    let dust_sync = Arc::new(dust_sync::LiveMidnightDustSyncController::new(
        config.clone(),
        Arc::clone(&dust_store),
        Arc::clone(&clock),
        Arc::clone(&keys),
    ));
    let shielded_sync = Arc::new(shielded_sync::LiveMidnightShieldedSyncController::new(
        indexer,
        shielded_store,
        Arc::clone(&clock),
        Arc::clone(&keys),
    ));
    let reconciler = Arc::new(submission::LiveMidnightSubmissionReconciler::new(
        config.clone(),
    ));
    MidnightWalletAdapter::with_default_network_deriver_and_completer(
        source,
        default_network,
        ProtectedMidnightAccountDeriver::new(keys),
        Arc::new(
            submission::LiveMidnightTransactionCompleter::new_with_dust_store(
                config, dust_store, clock,
            ),
        ),
    )
    .with_dust_sync(dust_sync)
    .with_shielded_sync(shielded_sync)
    .with_submission_recovery(submission_store, reconciler)
}

#[cfg(not(target_arch = "wasm32"))]
fn live_shielded_controller<C, K>(
    indexer: MidnightIndexerConfig,
    clock: Arc<C>,
    keys: Arc<K>,
) -> Arc<dyn shielded_sync::MidnightShieldedSyncController>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + 'static,
{
    Arc::new(shielded_sync::LiveMidnightShieldedSyncController::new(
        indexer,
        Arc::new(shielded_checkpoint::UnavailableMidnightShieldedCheckpointStore),
        clock,
        keys,
    ))
}

const fn map_account_to_dust_error(error: WalletAccountPortError) -> WalletDustSyncPortError {
    match error {
        WalletAccountPortError::UnsupportedNetwork => WalletDustSyncPortError::UnsupportedNetwork,
        WalletAccountPortError::ProtectionNotInitialized => {
            WalletDustSyncPortError::ProtectionNotInitialized
        }
        WalletAccountPortError::ProtectionLocked => WalletDustSyncPortError::ProtectionLocked,
        WalletAccountPortError::NotFound | WalletAccountPortError::Unavailable => {
            WalletDustSyncPortError::Unavailable
        }
        WalletAccountPortError::InvalidData => WalletDustSyncPortError::InvalidData,
    }
}

const fn map_account_to_shielded_error(
    error: WalletAccountPortError,
) -> WalletShieldedSyncPortError {
    match error {
        WalletAccountPortError::UnsupportedNetwork => {
            WalletShieldedSyncPortError::UnsupportedNetwork
        }
        WalletAccountPortError::ProtectionNotInitialized => {
            WalletShieldedSyncPortError::ProtectionNotInitialized
        }
        WalletAccountPortError::ProtectionLocked => WalletShieldedSyncPortError::ProtectionLocked,
        WalletAccountPortError::NotFound | WalletAccountPortError::Unavailable => {
            WalletShieldedSyncPortError::Unavailable
        }
        WalletAccountPortError::InvalidData => WalletShieldedSyncPortError::InvalidData,
    }
}

fn network_catalog() -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
    [
        ("mainnet", "Mainnet", NetworkEnvironment::Mainnet),
        ("preprod", "PreProd", NetworkEnvironment::PublicTest),
        ("preview", "Preview", NetworkEnvironment::PublicTest),
        ("testnet", "TestNet", NetworkEnvironment::PublicTest),
        ("qanet", "QANet", NetworkEnvironment::PublicTest),
        ("devnet", "DevNet", NetworkEnvironment::Development),
        (
            DEFAULT_NETWORK_ID,
            "Standalone",
            NetworkEnvironment::Development,
        ),
    ]
    .into_iter()
    .map(|(id, name, environment)| {
        Ok(ChainNetwork::new(
            ChainKind::Midnight,
            network_id(id)?,
            NetworkDisplayName::parse(name).map_err(|_| WalletAccountPortError::InvalidData)?,
            environment,
        ))
    })
    .collect()
}

pub(crate) fn network_by_id(
    network_id: &ChainNetworkId,
) -> Result<Option<ChainNetwork>, WalletAccountPortError> {
    Ok(network_catalog()?
        .into_iter()
        .find(|network| network.id() == network_id))
}

pub(crate) fn network_id(value: &str) -> Result<ChainNetworkId, WalletAccountPortError> {
    ChainNetworkId::parse(value.to_owned()).map_err(|_| WalletAccountPortError::InvalidData)
}

pub(crate) fn fixture_addresses(
    network_id: &ChainNetworkId,
) -> Result<Vec<ChainAddress>, WalletAccountPortError> {
    [
        (
            ChainAddressKind::Unshielded,
            "addr",
            FIXTURE_UNSHIELDED_PAYLOAD,
        ),
        (
            ChainAddressKind::Shielded,
            "shield-addr",
            FIXTURE_SHIELDED_PAYLOAD,
        ),
        (ChainAddressKind::Dust, "dust", FIXTURE_DUST_PAYLOAD),
    ]
    .into_iter()
    .map(|(kind, address_type, payload)| {
        let payload = hex::decode(payload).map_err(|_| WalletAccountPortError::InvalidData)?;
        encode_midnight_address(network_id, kind, address_type, &payload)
    })
    .collect()
}

fn encode_midnight_address(
    network_id: &ChainNetworkId,
    kind: ChainAddressKind,
    address_type: &str,
    payload: &[u8],
) -> Result<ChainAddress, WalletAccountPortError> {
    let hrp = if network_id.as_str() == "mainnet" {
        format!("mn_{address_type}")
    } else {
        format!("mn_{address_type}_{}", network_id.as_str())
    };
    let hrp = Hrp::parse(&hrp).map_err(|_| WalletAccountPortError::InvalidData)?;
    let encoded =
        bech32::encode::<Bech32m>(hrp, payload).map_err(|_| WalletAccountPortError::InvalidData)?;
    ChainAddress::parse(kind, encoded).map_err(|_| WalletAccountPortError::InvalidData)
}

fn decode_midnight_address_payload(
    address: &ChainAddress,
    network_id: &ChainNetworkId,
    address_type: &str,
    expected_bytes: usize,
) -> Result<Vec<u8>, WalletAccountPortError> {
    let decoded = CheckedHrpstring::new::<Bech32m>(address.value())
        .map_err(|_| WalletAccountPortError::InvalidData)?;
    let expected_hrp = if network_id.as_str() == "mainnet" {
        format!("mn_{address_type}")
    } else {
        format!("mn_{address_type}_{}", network_id.as_str())
    };
    let payload = decoded.byte_iter().collect::<Vec<_>>();
    if decoded.hrp().as_str() != expected_hrp || payload.len() != expected_bytes {
        return Err(WalletAccountPortError::InvalidData);
    }
    Ok(payload)
}

const fn map_security_error(error: WalletSecurityPortError) -> WalletAccountPortError {
    match error {
        WalletSecurityPortError::NotInitialized => WalletAccountPortError::ProtectionNotInitialized,
        WalletSecurityPortError::Locked => WalletAccountPortError::ProtectionLocked,
        WalletSecurityPortError::Unavailable => WalletAccountPortError::Unavailable,
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::NotFound
        | WalletSecurityPortError::Conflict
        | WalletSecurityPortError::UnsupportedAlgorithm
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => WalletAccountPortError::InvalidData,
    }
}

const fn map_association_error(
    error: WalletProfileAssociationRepositoryError,
) -> WalletAccountPortError {
    match error {
        WalletProfileAssociationRepositoryError::Integrity => WalletAccountPortError::InvalidData,
        WalletProfileAssociationRepositoryError::Unavailable => WalletAccountPortError::Unavailable,
    }
}

fn simulated_ledger_state(
    now: oxid_foundation::UnixTimestampMillis,
) -> Result<(Vec<AssetBalance>, Vec<WalletTransaction>), WalletAccountPortError> {
    let night = midnight_asset("midnight:night", "NIGHT", STARS_PER_NIGHT)?;
    let dust = midnight_asset("midnight:dust", "DUST", SPECKS_PER_DUST)?;
    let balances = vec![
        AssetBalance::new(night.clone(), 5 * STARS_PER_NIGHT),
        AssetBalance::new(dust.clone(), 12 * SPECKS_PER_DUST),
    ];
    let older = oxid_foundation::UnixTimestampMillis::new(now.value().saturating_sub(60_000));
    let transactions = vec![
        WalletTransaction::new(
            ChainTransactionId::parse("simulated_incoming")
                .map_err(|_| WalletAccountPortError::InvalidData)?,
            WalletTransactionDirection::Incoming,
            WalletTransactionStatus::Confirmed,
            Some(41),
            Some(older),
            vec![AssetBalanceChange::new(
                BalanceChangeDirection::Credit,
                AssetBalance::new(night.clone(), 6 * STARS_PER_NIGHT),
            )],
            None,
        ),
        WalletTransaction::new(
            ChainTransactionId::parse("simulated_outgoing")
                .map_err(|_| WalletAccountPortError::InvalidData)?,
            WalletTransactionDirection::Outgoing,
            WalletTransactionStatus::Confirmed,
            Some(42),
            Some(now),
            vec![AssetBalanceChange::new(
                BalanceChangeDirection::Debit,
                AssetBalance::new(night, STARS_PER_NIGHT),
            )],
            Some(AssetBalance::new(dust, SPECKS_PER_DUST / 10)),
        ),
    ];
    Ok((balances, transactions))
}

pub(crate) fn midnight_asset(
    id: &str,
    symbol: &str,
    atomic_units_per_whole: u128,
) -> Result<ChainAsset, WalletAccountPortError> {
    let decimals =
        decimal_places(atomic_units_per_whole).ok_or(WalletAccountPortError::InvalidData)?;
    Ok(ChainAsset::new(
        ChainAssetId::parse(id.to_owned()).map_err(|_| WalletAccountPortError::InvalidData)?,
        AssetSymbol::parse(symbol).map_err(|_| WalletAccountPortError::InvalidData)?,
        decimals,
    ))
}

pub(crate) fn decimal_places(mut units: u128) -> Option<u8> {
    if units == 0 {
        return None;
    }
    let mut decimals = 0_u8;
    while units > 1 {
        if !units.is_multiple_of(10) {
            return None;
        }
        units /= 10;
        decimals = decimals.checked_add(1)?;
    }
    Some(decimals)
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    use oxid_foundation::UnixTimestampMillis;
    use oxid_platform_ports::PlatformError;
    use oxid_wallet_domain::{WalletKeyDescriptor, WalletKeyReference, WalletPublicKey};

    use super::*;

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    struct WalletSdkVectorKeys;

    #[derive(Default)]
    struct TestAssociationRepository(Mutex<HashMap<WalletProfileId, WalletProfileAssociations>>);

    impl WalletProfileAssociationRepository for TestAssociationRepository {
        fn load_associations(
            &self,
            profile_id: &WalletProfileId,
        ) -> Result<
            Option<WalletProfileAssociations>,
            oxid_wallet_application::WalletProfileAssociationRepositoryError,
        > {
            self.0
                .lock()
                .map(|records| records.get(profile_id).cloned())
                .map_err(|_| {
                    oxid_wallet_application::WalletProfileAssociationRepositoryError::Unavailable
                })
        }

        fn save_associations(
            &self,
            profile_id: &WalletProfileId,
            associations: WalletProfileAssociations,
        ) -> Result<(), oxid_wallet_application::WalletProfileAssociationRepositoryError> {
            self.0
                .lock()
                .map_err(|_| {
                    oxid_wallet_application::WalletProfileAssociationRepositoryError::Unavailable
                })?
                .insert(profile_id.clone(), associations);
            Ok(())
        }

        fn remove_associations(
            &self,
            profile_id: &WalletProfileId,
        ) -> Result<(), oxid_wallet_application::WalletProfileAssociationRepositoryError> {
            self.0
                .lock()
                .map_err(|_| {
                    oxid_wallet_application::WalletProfileAssociationRepositoryError::Unavailable
                })?
                .remove(profile_id);
            Ok(())
        }
    }

    impl WalletKeyDerivationPort for WalletSdkVectorKeys {
        fn derive(
            &self,
            _: &WalletProfileId,
            request: DeriveProtectedKeyRequest,
        ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
            let path = request
                .path
                .components()
                .iter()
                .map(|component| (component.index(), component.hardened()))
                .collect::<Vec<_>>();
            assert_eq!(
                path,
                vec![(44, true), (2400, true), (0, true), (0, false), (0, false)]
            );
            assert_eq!(request.algorithm, WalletKeyAlgorithm::Secp256k1Schnorr);
            assert_eq!(request.purpose, WalletKeyPurpose::Transaction);
            let public_key =
                hex::decode("b193e54524dc796402870a883fbdcd83869c9c307dda8c0d99c5f769169fc883")
                    .expect("public vector is valid hex");
            Ok(WalletKeyDescriptor::new(
                WalletKeyReference::parse("key_wallet_sdk_vector").expect("key reference is valid"),
                request.label,
                request.algorithm,
                request.purpose,
                WalletPublicKey::new(PublicKeyEncoding::Secp256k1XOnly, public_key),
                UnixTimestampMillis::new(1_700_000_000_000),
            ))
        }
    }

    impl WalletDerivedSecretUsePort for WalletSdkVectorKeys {
        fn use_derived_secret(
            &self,
            _: &WalletProfileId,
            path: &WalletHdPath,
            operation: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
        ) -> Result<(), WalletSecurityPortError> {
            let path = path
                .components()
                .iter()
                .map(|component| (component.index(), component.hardened()))
                .collect::<Vec<_>>();
            assert_eq!(
                path,
                vec![(44, true), (2400, true), (0, true), (3, false), (0, false)]
            );
            operation(&[1_u8; 32])
        }
    }

    fn profile() -> WalletProfileId {
        WalletProfileId::parse("profile_test").expect("profile id is valid")
    }

    fn resolve<T>(mut future: Pin<Box<dyn Future<Output = T> + Send + '_>>) -> T {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("simulated future must resolve immediately"),
        }
    }

    #[test]
    fn catalog_uses_network_identity_without_routes() {
        let adapter = unavailable_midnight_wallet();
        let networks = adapter.available_networks().expect("catalog is valid");

        assert_eq!(networks.len(), 7);
        assert!(networks.iter().any(|network| {
            network.id().as_str() == "undeployed" && network.display_name().as_str() == "Standalone"
        }));
        assert!(networks.iter().all(|network| {
            !network.id().as_str().contains("://")
                && !network.display_name().as_str().contains("://")
        }));
    }

    #[test]
    fn address_codec_matches_official_mainnet_and_devnet_vectors() {
        let mainnet = fixture_addresses(&network_id("mainnet").expect("network is valid"))
            .expect("addresses encode");
        let devnet = fixture_addresses(&network_id("devnet").expect("network is valid"))
            .expect("addresses encode");

        assert_eq!(
            mainnet[0].value(),
            "mn_addr1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9s6e0fs3"
        );
        assert_eq!(
            mainnet[1].value(),
            concat!(
                "mn_shield-addr1p99fzfvf2z2q05zaaqzml8laccfd8uhzm9t2jewxggyr65tj4dp4g",
                "cfv7e04ka0x7qeajljmln7za5d4edntjxncx4q0uh6gkkj706g3tr2at"
            )
        );
        assert_eq!(
            mainnet[2].value(),
            "mn_dust1w0uhul6nq3705w5etn9j7uyrv047w6e893yjcuvsycdlm9szkdqkgkerpav"
        );
        assert_eq!(
            devnet[0].value(),
            "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y"
        );
    }

    #[test]
    fn protected_deriver_matches_the_pinned_wallet_sdk_address_vector() {
        let devnet = network_by_id(&network_id("devnet").expect("network is valid"))
            .expect("catalog is valid")
            .expect("devnet exists");
        let derived = ProtectedMidnightAccountDeriver::new(Arc::new(WalletSdkVectorKeys))
            .derive(&profile(), &devnet, 0, 0)
            .expect("public account derives");

        assert_eq!(derived.account_id().as_str(), "midnight_account_0_0");
        assert_eq!(derived.transaction_key().as_str(), "key_wallet_sdk_vector");
        assert_eq!(
            derived.receive_address().value(),
            "mn_addr_devnet13gn5semyxq8w3cd9fv0av5v4crkzcfmt7mlmvh83wwu6gtc8w3sqr2gnec"
        );
        assert_eq!(derived.addresses().len(), 2);
        assert_eq!(derived.addresses()[1].kind(), ChainAddressKind::Shielded);
        assert_eq!(
            derived.addresses()[1].value(),
            concat!(
                "mn_shield-addr_devnet1p99fzfvf2z2q05zaaqzml8laccfd8uhzm9t2jewxggyr65tj4dp4g",
                "cfv7e04ka0x7qeajljmln7za5d4edntjxncx4q0uh6gkkj706ggme77n"
            )
        );
    }

    #[test]
    fn selected_ledger_revision_defines_exact_night_and_dust_decimals() {
        assert_eq!(STARS_PER_NIGHT, 1_000_000);
        assert_eq!(SPECKS_PER_DUST, 1_000_000_000_000_000);
        assert_eq!(decimal_places(STARS_PER_NIGHT), Some(6));
        assert_eq!(decimal_places(SPECKS_PER_DUST), Some(15));
        assert_eq!(decimal_places(12), None);
        assert_eq!(decimal_places(0), None);
    }

    #[test]
    fn simulated_account_is_empty_until_explicit_sync() {
        let adapter = simulated_midnight_wallet(Arc::new(FixedClock));
        let before = adapter.account(&profile()).expect("account is available");

        assert_eq!(before.source(), WalletAccountSource::Simulated);
        assert_eq!(before.sync().state(), WalletSyncState::NeverSynced);
        assert_eq!(before.addresses().len(), 3);
        assert!(before.balances().is_empty());

        let after = resolve(adapter.sync(&profile())).expect("sync succeeds");
        assert_eq!(after.sync().state(), WalletSyncState::Synced);
        assert_eq!(after.sync().chain_tip_height(), Some(42));
        assert_eq!(after.balances().len(), 2);
        assert_eq!(after.transactions().len(), 2);
        assert_eq!(after.balances()[0].asset().decimals(), 6);
        assert_eq!(after.balances()[1].asset().decimals(), 15);
    }

    #[test]
    fn network_selection_is_profile_scoped_and_changes_address_hrp() {
        let adapter = simulated_midnight_wallet(Arc::new(FixedClock));
        let second = WalletProfileId::parse("profile_second").expect("profile id is valid");
        let preprod = network_id("preprod").expect("network is valid");
        adapter
            .select_network(&profile(), &preprod)
            .expect("selection succeeds");

        let first_account = adapter.account(&profile()).expect("account is available");
        let second_account = adapter.account(&second).expect("account is available");
        assert_eq!(first_account.network().id().as_str(), "preprod");
        assert!(
            first_account.addresses()[0]
                .value()
                .starts_with("mn_addr_preprod1")
        );
        assert_eq!(second_account.network().id().as_str(), "undeployed");
        assert!(
            second_account.addresses()[0]
                .value()
                .starts_with("mn_addr_undeployed1")
        );
        assert_eq!(
            adapter.select_network(
                &profile(),
                &network_id("unknown").expect("identifier shape is valid")
            ),
            Err(WalletAccountPortError::UnsupportedNetwork)
        );
    }

    #[test]
    fn rebinds_the_exact_derived_account_from_persisted_public_coordinates() {
        let repository = Arc::new(TestAssociationRepository::default());
        let devnet = network_id("devnet").expect("network is valid");
        let first = MidnightWalletAdapter::with_deriver(
            SimulatedMidnightAccountSource::new(Arc::new(FixedClock)),
            ProtectedMidnightAccountDeriver::new(Arc::new(WalletSdkVectorKeys)),
        )
        .with_profile_association_repository(repository.clone());
        first
            .select_network(&profile(), &devnet)
            .expect("selection persists");
        let derived = first
            .derive_account(&profile(), 0, 0)
            .expect("account derives");
        let expected_address = derived.receive_address().clone();
        drop(first);

        let reopened = MidnightWalletAdapter::with_deriver(
            SimulatedMidnightAccountSource::new(Arc::new(FixedClock)),
            ProtectedMidnightAccountDeriver::new(Arc::new(WalletSdkVectorKeys)),
        )
        .with_profile_association_repository(repository);
        assert_eq!(
            reopened
                .selected_network(&profile())
                .expect("selection reloads"),
            devnet
        );
        assert_eq!(
            reopened
                .account(&profile())
                .expect("account rebinds")
                .addresses()[0],
            expected_address
        );
    }

    #[test]
    fn public_call_context_decodes_exact_profile_scoped_address_payloads() {
        let adapter = simulated_midnight_wallet(Arc::new(FixedClock));
        let context = adapter
            .public_call_context(profile().as_str())
            .expect("fixture public context is available");

        assert_eq!(context.network_id().as_str(), "undeployed");
        assert_eq!(
            hex::encode(context.coin_public_key()),
            &FIXTURE_SHIELDED_PAYLOAD[..64]
        );
        assert_eq!(
            hex::encode(context.encryption_public_key()),
            &FIXTURE_SHIELDED_PAYLOAD[64..]
        );
        assert_eq!(
            hex::encode(context.unshielded_recipient()),
            FIXTURE_UNSHIELDED_PAYLOAD
        );
        let debug = format!("{context:?}");
        assert!(debug.contains("network_id"));
        assert!(!debug.contains(FIXTURE_UNSHIELDED_PAYLOAD));
        assert!(!debug.contains(FIXTURE_SHIELDED_PAYLOAD));
    }

    #[test]
    fn public_call_context_rejects_accounts_without_exact_required_addresses() {
        let adapter = unavailable_midnight_wallet();
        assert_eq!(
            adapter.public_call_context(profile().as_str()),
            Err(WalletAccountPortError::NotFound)
        );
        let address = fixture_addresses(&network_id("preprod").expect("network id"))
            .expect("fixture addresses")
            .remove(0);
        assert_eq!(
            decode_midnight_address_payload(
                &address,
                &network_id("undeployed").expect("network id"),
                "addr",
                32,
            ),
            Err(WalletAccountPortError::InvalidData)
        );
    }

    #[test]
    fn production_source_fails_closed_without_claiming_an_account() {
        let adapter = unavailable_midnight_wallet();
        let account = adapter.account(&profile()).expect("status is available");
        let synced = resolve(adapter.sync(&profile())).expect("status is available");

        for snapshot in [account, synced] {
            assert_eq!(snapshot.source(), WalletAccountSource::Unavailable);
            assert!(snapshot.account_id().is_none());
            assert!(snapshot.addresses().is_empty());
            assert!(snapshot.balances().is_empty());
            assert_eq!(snapshot.sync().state(), WalletSyncState::Unavailable);
        }
    }
}
