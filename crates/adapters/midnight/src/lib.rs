// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(not(target_arch = "wasm32"))]
mod indexer;

#[cfg(not(target_arch = "wasm32"))]
pub use indexer::{
    LiveMidnightAccountSource, MidnightIndexerConfig, MidnightIndexerConfigError,
    live_midnight_wallet,
};

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use bech32::{Bech32m, Hrp};
use oxid_platform_ports::ClockPort;
use oxid_wallet_application::{
    WalletAccountPortError, WalletAccountPortFuture, WalletAccountReadPort, WalletNetworkPort,
};
use oxid_wallet_domain::{
    AssetBalance, AssetBalanceChange, AssetSymbol, BalanceChangeDirection, ChainAccountId,
    ChainAddress, ChainAddressKind, ChainAsset, ChainAssetId, ChainKind, ChainNetwork,
    ChainNetworkId, ChainTransactionId, NetworkDisplayName, NetworkEnvironment,
    WalletAccountSnapshot, WalletAccountSource, WalletProfileId, WalletSyncState, WalletSyncStatus,
    WalletTransaction, WalletTransactionDirection, WalletTransactionStatus,
};

const DEFAULT_NETWORK_ID: &str = "undeployed";

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

/// Midnight adapter with profile-scoped network selection and replaceable data source.
pub struct MidnightWalletAdapter<S> {
    source: S,
    selections: Mutex<HashMap<WalletProfileId, ChainNetworkId>>,
    default_network: Option<ChainNetworkId>,
}

impl<S> MidnightWalletAdapter<S> {
    #[must_use]
    pub fn new(source: S) -> Self {
        Self {
            source,
            selections: Mutex::new(HashMap::new()),
            default_network: None,
        }
    }

    /// Uses an explicitly configured initial network while preserving
    /// profile-scoped selection after the first user choice.
    #[must_use]
    pub fn with_default_network(source: S, default_network: ChainNetworkId) -> Self {
        Self {
            source,
            selections: Mutex::new(HashMap::new()),
            default_network: Some(default_network),
        }
    }

    fn selected(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<ChainNetworkId, WalletAccountPortError> {
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
}

impl<S> WalletNetworkPort for MidnightWalletAdapter<S>
where
    S: MidnightAccountSource,
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
        self.selections
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .insert(profile_id.clone(), network_id.clone());
        Ok(network_id.clone())
    }
}

impl<S> WalletAccountReadPort for MidnightWalletAdapter<S>
where
    S: MidnightAccountSource,
{
    fn account(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
        let selected = self.selected(profile_id)?;
        let network =
            network_by_id(&selected)?.ok_or(WalletAccountPortError::UnsupportedNetwork)?;
        self.source.account(profile_id, &network)
    }

    fn sync<'a>(&'a self, profile_id: &'a WalletProfileId) -> WalletAccountPortFuture<'a> {
        Box::pin(async move {
            let selected = self.selected(profile_id)?;
            let network =
                network_by_id(&selected)?.ok_or(WalletAccountPortError::UnsupportedNetwork)?;
            self.source.sync(profile_id, &network).await
        })
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
}

impl<C> SimulatedMidnightAccountSource<C> {
    #[must_use]
    pub fn new(clock: Arc<C>) -> Self {
        Self {
            clock,
            synchronized: Mutex::new(HashSet::new()),
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
        let account_id = ChainAccountId::parse(profile_id.as_str().to_owned())
            .map_err(|_| WalletAccountPortError::InvalidData)?;
        let addresses = fixture_addresses(network.id())?;
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
        let hrp = if network_id.as_str() == "mainnet" {
            format!("mn_{address_type}")
        } else {
            format!("mn_{address_type}_{}", network_id.as_str())
        };
        let hrp = Hrp::parse(&hrp).map_err(|_| WalletAccountPortError::InvalidData)?;
        let encoded = bech32::encode::<Bech32m>(hrp, &payload)
            .map_err(|_| WalletAccountPortError::InvalidData)?;
        ChainAddress::parse(kind, encoded).map_err(|_| WalletAccountPortError::InvalidData)
    })
    .collect()
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

    use super::*;

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
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
