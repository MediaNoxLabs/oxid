// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::{
    AssetBalance, AssetBalanceChange, BalanceChangeDirection, ChainAddress, ChainAddressKind,
    ChainKind, ChainNetwork, ChainNetworkId, DerivedChainAccount, MAX_HD_CHILD_INDEX,
    NetworkEnvironment, WalletAccountSnapshot, WalletAccountSource, WalletProfileId,
    WalletSyncState, WalletTransaction, WalletTransactionDirection, WalletTransactionStatus,
};

/// Asynchronous result returned by a chain-account adapter.
pub type WalletAccountPortFuture<'a> = Pin<
    Box<dyn Future<Output = Result<WalletAccountSnapshot, WalletAccountPortError>> + Send + 'a>,
>;

/// Asynchronous account view returned to incoming adapters.
pub type WalletAccountViewFuture<'a> =
    Pin<Box<dyn Future<Output = Result<WalletAccountView, WalletAccountError>> + Send + 'a>>;

/// Stable failures exposed by network and account adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletAccountPortError {
    NotFound,
    UnsupportedNetwork,
    ProtectionNotInitialized,
    ProtectionLocked,
    Unavailable,
    InvalidData,
}

impl fmt::Display for WalletAccountPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotFound => "wallet chain account was not found",
            Self::UnsupportedNetwork => "wallet network is not supported",
            Self::ProtectionNotInitialized => "wallet protection is not initialized",
            Self::ProtectionLocked => "wallet is locked",
            Self::Unavailable => "wallet chain capability is unavailable",
            Self::InvalidData => "wallet chain adapter returned invalid data",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletAccountPortError {}

/// Focused outgoing port for network catalog and selection state.
pub trait WalletNetworkPort: Send + Sync {
    fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError>;

    fn selected_network(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<ChainNetworkId, WalletAccountPortError>;

    fn select_network(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<ChainNetworkId, WalletAccountPortError>;
}

/// Focused outgoing port for cached and synchronized public account state.
pub trait WalletAccountReadPort: Send + Sync {
    fn account(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletAccountSnapshot, WalletAccountPortError>;

    fn sync<'a>(&'a self, profile_id: &'a WalletProfileId) -> WalletAccountPortFuture<'a>;
}

/// Focused outgoing port for deriving an account through protected key custody.
pub trait WalletAccountDerivationPort: Send + Sync {
    fn derive_account(
        &self,
        profile_id: &WalletProfileId,
        account_index: u32,
        address_index: u32,
    ) -> Result<DerivedChainAccount, WalletAccountPortError>;
}

/// Profile-scoped query shared by network and account use cases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletAccountQuery {
    pub profile_id: String,
}

/// Input for selecting a chain network without changing transport routes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectWalletNetworkCommand {
    pub profile_id: String,
    pub network_id: String,
}

/// Input for deriving one account on the profile's selected network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeriveWalletAccountCommand {
    pub profile_id: String,
    pub account_index: u32,
    pub address_index: u32,
}

/// Public network metadata returned to incoming adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletNetworkView {
    pub chain: String,
    pub network_id: String,
    pub display_name: String,
    pub environment: String,
    pub selected: bool,
}

/// Complete network-selection result for a wallet profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletNetworkListView {
    pub selected_network_id: String,
    pub networks: Vec<WalletNetworkView>,
}

/// Public receive address returned by the account use case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletAddressView {
    pub kind: String,
    pub value: String,
}

/// Safe public account-derivation result returned to incoming adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedWalletAccountView {
    pub network_id: String,
    pub account_id: String,
    pub account_index: u32,
    pub address_index: u32,
    pub receive_address: WalletAddressView,
    pub addresses: Vec<WalletAddressView>,
    pub transaction_key_reference: String,
}

impl From<&DerivedChainAccount> for DerivedWalletAccountView {
    fn from(account: &DerivedChainAccount) -> Self {
        Self {
            network_id: account.network_id().as_str().to_owned(),
            account_id: account.account_id().as_str().to_owned(),
            account_index: account.account_index(),
            address_index: account.address_index(),
            receive_address: address_view(account.receive_address()),
            addresses: account.addresses().iter().map(address_view).collect(),
            transaction_key_reference: account.transaction_key().as_str().to_owned(),
        }
    }
}

/// Exact asset amount represented as a decimal integer string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletAssetBalanceView {
    pub asset_id: String,
    pub symbol: String,
    pub decimals: u8,
    pub atomic_units: String,
}

/// Wallet-relative asset change for one transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletAssetChangeView {
    pub direction: String,
    pub balance: WalletAssetBalanceView,
}

/// Safe synchronization state for presentation and automation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletSyncStatusView {
    pub state: String,
    pub current_cursor: Option<u64>,
    pub target_cursor: Option<u64>,
    pub chain_tip_height: Option<u64>,
    pub updated_at_millis: Option<u64>,
}

/// Public transaction-history row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransactionView {
    pub transaction_id: String,
    pub direction: String,
    pub status: String,
    pub block_height: Option<u64>,
    pub observed_at_millis: Option<u64>,
    pub changes: Vec<WalletAssetChangeView>,
    pub fee: Option<WalletAssetBalanceView>,
}

/// Complete public account read model returned to incoming adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletAccountView {
    pub chain: String,
    pub network_id: String,
    pub network_name: String,
    pub network_environment: String,
    pub account_id: Option<String>,
    pub source: String,
    pub addresses: Vec<WalletAddressView>,
    pub balances: Vec<WalletAssetBalanceView>,
    pub sync: WalletSyncStatusView,
    pub transactions: Vec<WalletTransactionView>,
}

impl WalletAccountView {
    fn from_snapshot(snapshot: &WalletAccountSnapshot) -> Self {
        let mut addresses = snapshot
            .addresses()
            .iter()
            .map(address_view)
            .collect::<Vec<_>>();
        addresses.sort_by(|left, right| {
            address_rank(&left.kind)
                .cmp(&address_rank(&right.kind))
                .then_with(|| left.value.cmp(&right.value))
        });

        let mut balances = snapshot
            .balances()
            .iter()
            .map(balance_view)
            .collect::<Vec<_>>();
        balances.sort_by(|left, right| left.symbol.cmp(&right.symbol));

        let mut transactions = snapshot
            .transactions()
            .iter()
            .map(transaction_view)
            .collect::<Vec<_>>();
        transactions.sort_by(|left, right| {
            right
                .observed_at_millis
                .cmp(&left.observed_at_millis)
                .then_with(|| right.block_height.cmp(&left.block_height))
                .then_with(|| left.transaction_id.cmp(&right.transaction_id))
        });

        Self {
            chain: chain_name(snapshot.network().chain()).to_owned(),
            network_id: snapshot.network().id().as_str().to_owned(),
            network_name: snapshot.network().display_name().as_str().to_owned(),
            network_environment: environment_name(snapshot.network().environment()).to_owned(),
            account_id: snapshot.account_id().map(|id| id.as_str().to_owned()),
            source: account_source_name(snapshot.source()).to_owned(),
            addresses,
            balances,
            sync: WalletSyncStatusView {
                state: sync_state_name(snapshot.sync().state()).to_owned(),
                current_cursor: snapshot.sync().current_cursor(),
                target_cursor: snapshot.sync().target_cursor(),
                chain_tip_height: snapshot.sync().chain_tip_height(),
                updated_at_millis: snapshot.sync().updated_at().map(|value| value.value()),
            },
            transactions,
        }
    }
}

/// Structured input and adapter failures for chain-account use cases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletAccountError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidNetworkIdentifier(OpaqueIdError),
    AccountIndexOutOfBounds,
    AddressIndexOutOfBounds,
    Port(WalletAccountPortError),
}

impl fmt::Display for WalletAccountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) | Self::InvalidNetworkIdentifier(error) => {
                error.fmt(formatter)
            }
            Self::AccountIndexOutOfBounds => {
                formatter.write_str("account index must be less than 2^31")
            }
            Self::AddressIndexOutOfBounds => {
                formatter.write_str("address index must be less than 2^31")
            }
            Self::Port(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletAccountError {}

/// Incoming query for networks and the profile's current selection.
pub trait ListWalletNetworksUseCase: Send + Sync {
    fn execute(
        &self,
        query: WalletAccountQuery,
    ) -> Result<WalletNetworkListView, WalletAccountError>;
}

/// Incoming command for changing only the selected network identity.
pub trait SelectWalletNetworkUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectWalletNetworkCommand,
    ) -> Result<WalletNetworkListView, WalletAccountError>;
}

/// Incoming use case for deriving an account without handling private bytes.
pub trait DeriveWalletAccountUseCase: Send + Sync {
    fn execute(
        &self,
        command: DeriveWalletAccountCommand,
    ) -> Result<DerivedWalletAccountView, WalletAccountError>;
}

/// Incoming query for the most recent public account state.
pub trait GetWalletAccountUseCase: Send + Sync {
    fn execute(&self, query: WalletAccountQuery) -> Result<WalletAccountView, WalletAccountError>;
}

/// Incoming command for explicitly synchronizing public account state.
pub trait SyncWalletAccountUseCase: Send + Sync {
    fn execute(&self, query: WalletAccountQuery) -> WalletAccountViewFuture<'_>;
}

/// Application service for catalog and selection operations.
pub struct WalletNetworkService<N> {
    networks: Arc<N>,
}

impl<N> WalletNetworkService<N> {
    #[must_use]
    pub const fn new(networks: Arc<N>) -> Self {
        Self { networks }
    }

    fn view(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletNetworkListView, WalletAccountError>
    where
        N: WalletNetworkPort,
    {
        let selected = self
            .networks
            .selected_network(profile_id)
            .map_err(WalletAccountError::Port)?;
        let mut networks = self
            .networks
            .available_networks()
            .map_err(WalletAccountError::Port)?;
        networks.sort_by(|left, right| {
            network_rank(left.environment())
                .cmp(&network_rank(right.environment()))
                .then_with(|| {
                    left.display_name()
                        .as_str()
                        .cmp(right.display_name().as_str())
                })
        });

        if !networks.iter().any(|network| network.id() == &selected) {
            return Err(WalletAccountError::Port(
                WalletAccountPortError::InvalidData,
            ));
        }

        Ok(WalletNetworkListView {
            selected_network_id: selected.as_str().to_owned(),
            networks: networks
                .iter()
                .map(|network| network_view(network, network.id() == &selected))
                .collect(),
        })
    }
}

impl<N> ListWalletNetworksUseCase for WalletNetworkService<N>
where
    N: WalletNetworkPort + 'static,
{
    fn execute(
        &self,
        query: WalletAccountQuery,
    ) -> Result<WalletNetworkListView, WalletAccountError> {
        let profile_id = WalletProfileId::parse(query.profile_id)
            .map_err(WalletAccountError::InvalidProfileIdentifier)?;
        self.view(&profile_id)
    }
}

impl<N> SelectWalletNetworkUseCase for WalletNetworkService<N>
where
    N: WalletNetworkPort + 'static,
{
    fn execute(
        &self,
        command: SelectWalletNetworkCommand,
    ) -> Result<WalletNetworkListView, WalletAccountError> {
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletAccountError::InvalidProfileIdentifier)?;
        let network_id = ChainNetworkId::parse(command.network_id)
            .map_err(WalletAccountError::InvalidNetworkIdentifier)?;
        self.networks
            .select_network(&profile_id, &network_id)
            .map_err(WalletAccountError::Port)?;
        self.view(&profile_id)
    }
}

/// Application service for profile-scoped protected account derivation.
pub struct WalletAccountDerivationService<D> {
    derivation: Arc<D>,
}

impl<D> WalletAccountDerivationService<D> {
    #[must_use]
    pub const fn new(derivation: Arc<D>) -> Self {
        Self { derivation }
    }
}

impl<D> DeriveWalletAccountUseCase for WalletAccountDerivationService<D>
where
    D: WalletAccountDerivationPort + 'static,
{
    fn execute(
        &self,
        command: DeriveWalletAccountCommand,
    ) -> Result<DerivedWalletAccountView, WalletAccountError> {
        if command.account_index > MAX_HD_CHILD_INDEX {
            return Err(WalletAccountError::AccountIndexOutOfBounds);
        }
        if command.address_index > MAX_HD_CHILD_INDEX {
            return Err(WalletAccountError::AddressIndexOutOfBounds);
        }
        let profile_id = WalletProfileId::parse(command.profile_id)
            .map_err(WalletAccountError::InvalidProfileIdentifier)?;
        let derived = self
            .derivation
            .derive_account(&profile_id, command.account_index, command.address_index)
            .map_err(WalletAccountError::Port)?;
        Ok(DerivedWalletAccountView::from(&derived))
    }
}

/// Application service for cached and explicitly synchronized account state.
pub struct WalletAccountService<A> {
    accounts: Arc<A>,
}

impl<A> WalletAccountService<A> {
    #[must_use]
    pub const fn new(accounts: Arc<A>) -> Self {
        Self { accounts }
    }
}

impl<A> GetWalletAccountUseCase for WalletAccountService<A>
where
    A: WalletAccountReadPort + 'static,
{
    fn execute(&self, query: WalletAccountQuery) -> Result<WalletAccountView, WalletAccountError> {
        let profile_id = WalletProfileId::parse(query.profile_id)
            .map_err(WalletAccountError::InvalidProfileIdentifier)?;
        self.accounts
            .account(&profile_id)
            .map(|snapshot| WalletAccountView::from_snapshot(&snapshot))
            .map_err(WalletAccountError::Port)
    }
}

impl<A> SyncWalletAccountUseCase for WalletAccountService<A>
where
    A: WalletAccountReadPort + 'static,
{
    fn execute(&self, query: WalletAccountQuery) -> WalletAccountViewFuture<'_> {
        Box::pin(async move {
            let profile_id = WalletProfileId::parse(query.profile_id)
                .map_err(WalletAccountError::InvalidProfileIdentifier)?;
            self.accounts
                .sync(&profile_id)
                .await
                .map(|snapshot| WalletAccountView::from_snapshot(&snapshot))
                .map_err(WalletAccountError::Port)
        })
    }
}

fn network_view(network: &ChainNetwork, selected: bool) -> WalletNetworkView {
    WalletNetworkView {
        chain: chain_name(network.chain()).to_owned(),
        network_id: network.id().as_str().to_owned(),
        display_name: network.display_name().as_str().to_owned(),
        environment: environment_name(network.environment()).to_owned(),
        selected,
    }
}

const fn chain_name(chain: ChainKind) -> &'static str {
    match chain {
        ChainKind::Cardano => "cardano",
        ChainKind::Midnight => "midnight",
    }
}

const fn environment_name(environment: NetworkEnvironment) -> &'static str {
    match environment {
        NetworkEnvironment::Mainnet => "mainnet",
        NetworkEnvironment::PublicTest => "public_test",
        NetworkEnvironment::Development => "development",
        NetworkEnvironment::Custom => "custom",
    }
}

const fn network_rank(environment: NetworkEnvironment) -> u8 {
    match environment {
        NetworkEnvironment::Development => 0,
        NetworkEnvironment::PublicTest => 1,
        NetworkEnvironment::Mainnet => 2,
        NetworkEnvironment::Custom => 3,
    }
}

fn address_view(address: &ChainAddress) -> WalletAddressView {
    WalletAddressView {
        kind: address_kind_name(address.kind()).to_owned(),
        value: address.value().to_owned(),
    }
}

const fn address_kind_name(kind: ChainAddressKind) -> &'static str {
    match kind {
        ChainAddressKind::Unshielded => "unshielded",
        ChainAddressKind::Shielded => "shielded",
        ChainAddressKind::Dust => "dust",
        ChainAddressKind::Reward => "reward",
    }
}

const fn address_rank(kind: &str) -> u8 {
    match kind.as_bytes() {
        b"unshielded" => 0,
        b"shielded" => 1,
        b"dust" => 2,
        _ => 3,
    }
}

fn balance_view(balance: &AssetBalance) -> WalletAssetBalanceView {
    WalletAssetBalanceView {
        asset_id: balance.asset().id().as_str().to_owned(),
        symbol: balance.asset().symbol().as_str().to_owned(),
        decimals: balance.asset().decimals(),
        atomic_units: balance.atomic_units().to_string(),
    }
}

fn change_view(change: &AssetBalanceChange) -> WalletAssetChangeView {
    WalletAssetChangeView {
        direction: match change.direction() {
            BalanceChangeDirection::Credit => "credit",
            BalanceChangeDirection::Debit => "debit",
        }
        .to_owned(),
        balance: balance_view(change.balance()),
    }
}

fn transaction_view(transaction: &WalletTransaction) -> WalletTransactionView {
    WalletTransactionView {
        transaction_id: transaction.id().as_str().to_owned(),
        direction: transaction_direction_name(transaction.direction()).to_owned(),
        status: transaction_status_name(transaction.status()).to_owned(),
        block_height: transaction.block_height(),
        observed_at_millis: transaction.observed_at().map(|value| value.value()),
        changes: transaction.changes().iter().map(change_view).collect(),
        fee: transaction.fee().map(balance_view),
    }
}

const fn transaction_direction_name(direction: WalletTransactionDirection) -> &'static str {
    match direction {
        WalletTransactionDirection::Incoming => "incoming",
        WalletTransactionDirection::Outgoing => "outgoing",
        WalletTransactionDirection::SelfTransfer => "self_transfer",
        WalletTransactionDirection::Unknown => "unknown",
    }
}

const fn transaction_status_name(status: WalletTransactionStatus) -> &'static str {
    match status {
        WalletTransactionStatus::Pending => "pending",
        WalletTransactionStatus::Confirmed => "confirmed",
        WalletTransactionStatus::PartiallyApplied => "partially_applied",
        WalletTransactionStatus::Failed => "failed",
    }
}

const fn sync_state_name(state: WalletSyncState) -> &'static str {
    match state {
        WalletSyncState::NeverSynced => "never_synced",
        WalletSyncState::Syncing => "syncing",
        WalletSyncState::Synced => "synced",
        WalletSyncState::Stalled => "stalled",
        WalletSyncState::Unavailable => "unavailable",
    }
}

const fn account_source_name(source: WalletAccountSource) -> &'static str {
    match source {
        WalletAccountSource::Live => "live",
        WalletAccountSource::Cached => "cached",
        WalletAccountSource::Simulated => "simulated",
        WalletAccountSource::Unavailable => "unavailable",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        sync::Mutex,
        task::{Context, Poll, Waker},
    };

    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{
        AssetSymbol, ChainAccountId, ChainAsset, ChainAssetId, NetworkDisplayName,
        PublicKeyEncoding, WalletKeyReference, WalletPublicKey, WalletSyncStatus,
    };

    use super::*;

    struct RecordingAdapter {
        selected: Mutex<ChainNetworkId>,
    }

    impl RecordingAdapter {
        fn new() -> Self {
            Self {
                selected: Mutex::new(network_id("undeployed")),
            }
        }

        fn network(id: &str, name: &str, environment: NetworkEnvironment) -> ChainNetwork {
            ChainNetwork::new(
                ChainKind::Midnight,
                network_id(id),
                NetworkDisplayName::parse(name).expect("network name is valid"),
                environment,
            )
        }

        fn snapshot(&self, profile_id: &WalletProfileId) -> WalletAccountSnapshot {
            let id = self
                .selected
                .lock()
                .expect("selection is available")
                .clone();
            let network = Self::network(
                id.as_str(),
                if id.as_str() == "undeployed" {
                    "Standalone"
                } else {
                    "PreProd"
                },
                if id.as_str() == "undeployed" {
                    NetworkEnvironment::Development
                } else {
                    NetworkEnvironment::PublicTest
                },
            );
            let night = ChainAsset::new(
                ChainAssetId::parse("midnight:night").expect("asset id is valid"),
                AssetSymbol::parse("NIGHT").expect("asset symbol is valid"),
                6,
            );
            let older = WalletTransaction::new(
                oxid_wallet_domain::ChainTransactionId::parse("tx_old")
                    .expect("transaction id is valid"),
                WalletTransactionDirection::Incoming,
                WalletTransactionStatus::Confirmed,
                Some(4),
                Some(UnixTimestampMillis::new(10)),
                Vec::new(),
                None,
            );
            let newer = WalletTransaction::new(
                oxid_wallet_domain::ChainTransactionId::parse("tx_new")
                    .expect("transaction id is valid"),
                WalletTransactionDirection::Outgoing,
                WalletTransactionStatus::Confirmed,
                Some(5),
                Some(UnixTimestampMillis::new(20)),
                Vec::new(),
                None,
            );
            WalletAccountSnapshot::new(
                network,
                Some(
                    ChainAccountId::parse(format!("account_{}", profile_id.as_str()))
                        .expect("account id is valid"),
                ),
                WalletAccountSource::Simulated,
                vec![
                    ChainAddress::parse(ChainAddressKind::Dust, "mn_dust1fixture")
                        .expect("address is valid"),
                    ChainAddress::parse(ChainAddressKind::Unshielded, "mn_addr1fixture")
                        .expect("address is valid"),
                ],
                vec![AssetBalance::new(night, u128::MAX)],
                WalletSyncStatus::new(
                    WalletSyncState::Synced,
                    Some(5),
                    Some(5),
                    Some(9),
                    Some(UnixTimestampMillis::new(20)),
                ),
                vec![older, newer],
            )
        }
    }

    impl WalletNetworkPort for RecordingAdapter {
        fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
            Ok(vec![
                Self::network("preprod", "PreProd", NetworkEnvironment::PublicTest),
                Self::network("undeployed", "Standalone", NetworkEnvironment::Development),
            ])
        }

        fn selected_network(
            &self,
            _: &WalletProfileId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            self.selected
                .lock()
                .map(|value| value.clone())
                .map_err(|_| WalletAccountPortError::Unavailable)
        }

        fn select_network(
            &self,
            _: &WalletProfileId,
            network_id: &ChainNetworkId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            if !matches!(network_id.as_str(), "undeployed" | "preprod") {
                return Err(WalletAccountPortError::UnsupportedNetwork);
            }
            *self
                .selected
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)? = network_id.clone();
            Ok(network_id.clone())
        }
    }

    impl WalletAccountReadPort for RecordingAdapter {
        fn account(
            &self,
            profile_id: &WalletProfileId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            Ok(self.snapshot(profile_id))
        }

        fn sync<'a>(&'a self, profile_id: &'a WalletProfileId) -> WalletAccountPortFuture<'a> {
            Box::pin(async move { Ok(self.snapshot(profile_id)) })
        }
    }

    impl WalletAccountDerivationPort for RecordingAdapter {
        fn derive_account(
            &self,
            _: &WalletProfileId,
            account_index: u32,
            address_index: u32,
        ) -> Result<DerivedChainAccount, WalletAccountPortError> {
            let network_id = self
                .selected
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)?
                .clone();
            DerivedChainAccount::new(
                network_id,
                ChainAccountId::parse(format!("account_{account_index}_{address_index}"))
                    .map_err(|_| WalletAccountPortError::InvalidData)?,
                account_index,
                address_index,
                ChainAddress::parse(ChainAddressKind::Unshielded, "mn_addr1derived")
                    .map_err(|_| WalletAccountPortError::InvalidData)?,
                WalletPublicKey::new(PublicKeyEncoding::Secp256k1XOnly, vec![7; 32]),
                WalletKeyReference::parse("key_derived")
                    .map_err(|_| WalletAccountPortError::InvalidData)?,
            )
            .map_err(|_| WalletAccountPortError::InvalidData)
        }
    }

    fn network_id(value: &str) -> ChainNetworkId {
        ChainNetworkId::parse(value).expect("network id is valid")
    }

    fn resolve<T>(mut future: Pin<Box<dyn Future<Output = T> + Send + '_>>) -> T {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("test future must resolve immediately"),
        }
    }

    #[test]
    fn network_service_sorts_and_selects_without_routes() {
        let adapter = Arc::new(RecordingAdapter::new());
        let service = WalletNetworkService::new(adapter);
        let initial = ListWalletNetworksUseCase::execute(
            &service,
            WalletAccountQuery {
                profile_id: "profile_test".to_owned(),
            },
        )
        .expect("network query succeeds");

        assert_eq!(initial.selected_network_id, "undeployed");
        assert_eq!(initial.networks[0].network_id, "undeployed");
        assert!(!initial.networks[0].display_name.contains("http"));

        let selected = SelectWalletNetworkUseCase::execute(
            &service,
            SelectWalletNetworkCommand {
                profile_id: "profile_test".to_owned(),
                network_id: "preprod".to_owned(),
            },
        )
        .expect("network selection succeeds");
        assert_eq!(selected.selected_network_id, "preprod");
        assert!(
            selected
                .networks
                .iter()
                .any(|network| { network.network_id == "preprod" && network.selected })
        );
    }

    #[test]
    fn account_service_maps_exact_values_and_sorts_activity() {
        let adapter = Arc::new(RecordingAdapter::new());
        let service = WalletAccountService::new(adapter);
        let view = GetWalletAccountUseCase::execute(
            &service,
            WalletAccountQuery {
                profile_id: "profile_test".to_owned(),
            },
        )
        .expect("account query succeeds");

        assert_eq!(view.source, "simulated");
        assert_eq!(view.balances[0].atomic_units, u128::MAX.to_string());
        assert_eq!(view.addresses[0].kind, "unshielded");
        assert_eq!(view.transactions[0].transaction_id, "tx_new");
        assert_eq!(view.sync.state, "synced");
    }

    #[test]
    fn sync_use_case_uses_the_async_port_without_sdk_types() {
        let adapter = Arc::new(RecordingAdapter::new());
        let service = WalletAccountService::new(adapter);
        let view = resolve(SyncWalletAccountUseCase::execute(
            &service,
            WalletAccountQuery {
                profile_id: "profile_test".to_owned(),
            },
        ))
        .expect("sync succeeds");

        assert_eq!(view.network_id, "undeployed");
        assert_eq!(view.sync.chain_tip_height, Some(9));
    }

    #[test]
    fn derivation_service_validates_indices_and_returns_only_public_metadata() {
        let adapter = Arc::new(RecordingAdapter::new());
        let service = WalletAccountDerivationService::new(adapter);
        let view = DeriveWalletAccountUseCase::execute(
            &service,
            DeriveWalletAccountCommand {
                profile_id: "profile_test".to_owned(),
                account_index: 7,
                address_index: 3,
            },
        )
        .expect("derivation succeeds");

        assert_eq!(view.network_id, "undeployed");
        assert_eq!(view.account_id, "account_7_3");
        assert_eq!(view.receive_address.kind, "unshielded");
        assert_eq!(view.transaction_key_reference, "key_derived");
        assert_eq!(
            DeriveWalletAccountUseCase::execute(
                &service,
                DeriveWalletAccountCommand {
                    profile_id: "profile_test".to_owned(),
                    account_index: MAX_HD_CHILD_INDEX + 1,
                    address_index: 0,
                },
            ),
            Err(WalletAccountError::AccountIndexOutOfBounds)
        );
        assert_eq!(
            DeriveWalletAccountUseCase::execute(
                &service,
                DeriveWalletAccountCommand {
                    profile_id: "profile_test".to_owned(),
                    account_index: 0,
                    address_index: MAX_HD_CHILD_INDEX + 1,
                },
            ),
            Err(WalletAccountError::AddressIndexOutOfBounds)
        );
    }

    #[test]
    fn invalid_input_and_adapter_failures_remain_typed() {
        let adapter = Arc::new(RecordingAdapter::new());
        let networks = WalletNetworkService::new(Arc::clone(&adapter));
        let accounts = WalletAccountService::new(adapter);

        assert!(matches!(
            ListWalletNetworksUseCase::execute(
                &networks,
                WalletAccountQuery {
                    profile_id: "profile invalid".to_owned()
                }
            ),
            Err(WalletAccountError::InvalidProfileIdentifier(_))
        ));
        assert_eq!(
            SelectWalletNetworkUseCase::execute(
                &networks,
                SelectWalletNetworkCommand {
                    profile_id: "profile_test".to_owned(),
                    network_id: "unknown".to_owned(),
                }
            ),
            Err(WalletAccountError::Port(
                WalletAccountPortError::UnsupportedNetwork
            ))
        );
        assert!(matches!(
            GetWalletAccountUseCase::execute(
                &accounts,
                WalletAccountQuery {
                    profile_id: "bad profile".to_owned()
                }
            ),
            Err(WalletAccountError::InvalidProfileIdentifier(_))
        ));
    }
}
