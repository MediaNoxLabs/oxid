// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt};

use oxid_foundation::{OpaqueId, OpaqueIdError, UnixTimestampMillis};

use crate::{WalletKeyReference, WalletPublicKey};

/// Largest valid non-hardened BIP32 account or address index.
pub const MAX_HD_CHILD_INDEX: u32 = (1 << 31) - 1;

/// A blockchain family supported by Oxid-owned account semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChainKind {
    Cardano,
    Midnight,
}

/// Stable network identity, independent from HTTP or WebSocket routes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainNetworkId(OpaqueId);

impl ChainNetworkId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for ChainNetworkId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Broad environment classification for presentation and safety policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkEnvironment {
    Mainnet,
    PublicTest,
    Development,
    Custom,
}

/// A validated, user-facing network label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkDisplayName(String);

impl NetworkDisplayName {
    pub const MAX_CHARACTERS: usize = 64;

    pub fn parse(value: impl AsRef<str>) -> Result<Self, PublicChainTextError> {
        parse_public_text(value.as_ref(), Self::MAX_CHARACTERS).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Chain and network identity exposed by a wallet adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainNetwork {
    chain: ChainKind,
    id: ChainNetworkId,
    display_name: NetworkDisplayName,
    environment: NetworkEnvironment,
}

impl ChainNetwork {
    #[must_use]
    pub const fn new(
        chain: ChainKind,
        id: ChainNetworkId,
        display_name: NetworkDisplayName,
        environment: NetworkEnvironment,
    ) -> Self {
        Self {
            chain,
            id,
            display_name,
            environment,
        }
    }

    #[must_use]
    pub const fn chain(&self) -> ChainKind {
        self.chain
    }

    #[must_use]
    pub const fn id(&self) -> &ChainNetworkId {
        &self.id
    }

    #[must_use]
    pub const fn display_name(&self) -> &NetworkDisplayName {
        &self.display_name
    }

    #[must_use]
    pub const fn environment(&self) -> NetworkEnvironment {
        self.environment
    }
}

/// Stable public identifier for one account within a wallet profile.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainAccountId(OpaqueId);

impl ChainAccountId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Address semantics known to the chain-neutral wallet boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainAddressKind {
    Unshielded,
    Shielded,
    Dust,
    Reward,
}

/// A validated public receive address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainAddress {
    kind: ChainAddressKind,
    value: String,
}

impl ChainAddress {
    pub const MAX_CHARACTERS: usize = 512;

    pub fn parse(
        kind: ChainAddressKind,
        value: impl AsRef<str>,
    ) -> Result<Self, ChainAddressError> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(ChainAddressError::Empty);
        }
        if value.chars().count() > Self::MAX_CHARACTERS {
            return Err(ChainAddressError::TooLong);
        }
        if value.chars().any(char::is_whitespace) {
            return Err(ChainAddressError::ContainsWhitespace);
        }
        if value.chars().any(char::is_control) {
            return Err(ChainAddressError::ContainsControlCharacter);
        }

        Ok(Self {
            kind,
            value: value.to_owned(),
        })
    }

    #[must_use]
    pub const fn kind(&self) -> ChainAddressKind {
        self.kind
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Address validation failures at the domain boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainAddressError {
    Empty,
    TooLong,
    ContainsWhitespace,
    ContainsControlCharacter,
}

impl fmt::Display for ChainAddressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "chain address must not be empty",
            Self::TooLong => "chain address must not exceed 512 characters",
            Self::ContainsWhitespace => "chain address must not contain whitespace",
            Self::ContainsControlCharacter => "chain address must not contain control characters",
        };
        formatter.write_str(message)
    }
}

impl Error for ChainAddressError {}

/// Safe public result of deriving one chain account from protected material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedChainAccount {
    network_id: ChainNetworkId,
    account_id: ChainAccountId,
    account_index: u32,
    address_index: u32,
    receive_address: ChainAddress,
    transaction_public_key: WalletPublicKey,
    transaction_key: WalletKeyReference,
}

impl DerivedChainAccount {
    pub fn new(
        network_id: ChainNetworkId,
        account_id: ChainAccountId,
        account_index: u32,
        address_index: u32,
        receive_address: ChainAddress,
        transaction_public_key: WalletPublicKey,
        transaction_key: WalletKeyReference,
    ) -> Result<Self, ChainAccountDerivationError> {
        if account_index > MAX_HD_CHILD_INDEX {
            return Err(ChainAccountDerivationError::AccountIndexOutOfBounds);
        }
        if address_index > MAX_HD_CHILD_INDEX {
            return Err(ChainAccountDerivationError::AddressIndexOutOfBounds);
        }
        Ok(Self {
            network_id,
            account_id,
            account_index,
            address_index,
            receive_address,
            transaction_public_key,
            transaction_key,
        })
    }

    #[must_use]
    pub const fn network_id(&self) -> &ChainNetworkId {
        &self.network_id
    }

    #[must_use]
    pub const fn account_id(&self) -> &ChainAccountId {
        &self.account_id
    }

    #[must_use]
    pub const fn account_index(&self) -> u32 {
        self.account_index
    }

    #[must_use]
    pub const fn address_index(&self) -> u32 {
        self.address_index
    }

    #[must_use]
    pub const fn receive_address(&self) -> &ChainAddress {
        &self.receive_address
    }

    #[must_use]
    pub const fn transaction_public_key(&self) -> &WalletPublicKey {
        &self.transaction_public_key
    }

    #[must_use]
    pub const fn transaction_key(&self) -> &WalletKeyReference {
        &self.transaction_key
    }
}

/// Derivation-index validation failures enforced by the chain domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainAccountDerivationError {
    AccountIndexOutOfBounds,
    AddressIndexOutOfBounds,
}

impl fmt::Display for ChainAccountDerivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::AccountIndexOutOfBounds => "account index must be less than 2^31",
            Self::AddressIndexOutOfBounds => "address index must be less than 2^31",
        };
        formatter.write_str(message)
    }
}

impl Error for ChainAccountDerivationError {}

/// Stable asset identity within a chain adapter.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainAssetId(OpaqueId);

impl ChainAssetId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Short public symbol such as NIGHT or DUST.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetSymbol(String);

impl AssetSymbol {
    pub const MAX_CHARACTERS: usize = 24;

    pub fn parse(value: impl AsRef<str>) -> Result<Self, PublicChainTextError> {
        parse_public_text(value.as_ref(), Self::MAX_CHARACTERS).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Public metadata required to render an asset without floating-point math.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainAsset {
    id: ChainAssetId,
    symbol: AssetSymbol,
    decimals: u8,
}

impl ChainAsset {
    #[must_use]
    pub const fn new(id: ChainAssetId, symbol: AssetSymbol, decimals: u8) -> Self {
        Self {
            id,
            symbol,
            decimals,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &ChainAssetId {
        &self.id
    }

    #[must_use]
    pub const fn symbol(&self) -> &AssetSymbol {
        &self.symbol
    }

    #[must_use]
    pub const fn decimals(&self) -> u8 {
        self.decimals
    }
}

/// Exact balance in an asset's atomic units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetBalance {
    asset: ChainAsset,
    atomic_units: u128,
}

impl AssetBalance {
    #[must_use]
    pub const fn new(asset: ChainAsset, atomic_units: u128) -> Self {
        Self {
            asset,
            atomic_units,
        }
    }

    #[must_use]
    pub const fn asset(&self) -> &ChainAsset {
        &self.asset
    }

    #[must_use]
    pub const fn atomic_units(&self) -> u128 {
        self.atomic_units
    }
}

/// Whether a transaction changed the wallet's balance in either direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalanceChangeDirection {
    Credit,
    Debit,
}

/// One exact asset change attributed to a transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetBalanceChange {
    direction: BalanceChangeDirection,
    balance: AssetBalance,
}

impl AssetBalanceChange {
    #[must_use]
    pub const fn new(direction: BalanceChangeDirection, balance: AssetBalance) -> Self {
        Self { direction, balance }
    }

    #[must_use]
    pub const fn direction(&self) -> BalanceChangeDirection {
        self.direction
    }

    #[must_use]
    pub const fn balance(&self) -> &AssetBalance {
        &self.balance
    }
}

/// Stable transaction identity owned by Oxid.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainTransactionId(OpaqueId);

impl ChainTransactionId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Stable public block identity owned by Oxid.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainBlockId(OpaqueId);

impl ChainBlockId {
    pub fn parse(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
        OpaqueId::parse(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Wallet-relative transaction direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletTransactionDirection {
    Incoming,
    Outgoing,
    SelfTransfer,
    Unknown,
}

/// Chain application result without importing an SDK enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletTransactionStatus {
    Pending,
    Confirmed,
    PartiallyApplied,
    Failed,
}

/// Public transaction-history entry for an account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTransaction {
    id: ChainTransactionId,
    direction: WalletTransactionDirection,
    status: WalletTransactionStatus,
    block_height: Option<u64>,
    observed_at: Option<UnixTimestampMillis>,
    changes: Vec<AssetBalanceChange>,
    fee: Option<AssetBalance>,
}

impl WalletTransaction {
    #[must_use]
    pub const fn new(
        id: ChainTransactionId,
        direction: WalletTransactionDirection,
        status: WalletTransactionStatus,
        block_height: Option<u64>,
        observed_at: Option<UnixTimestampMillis>,
        changes: Vec<AssetBalanceChange>,
        fee: Option<AssetBalance>,
    ) -> Self {
        Self {
            id,
            direction,
            status,
            block_height,
            observed_at,
            changes,
            fee,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &ChainTransactionId {
        &self.id
    }

    #[must_use]
    pub const fn direction(&self) -> WalletTransactionDirection {
        self.direction
    }

    #[must_use]
    pub const fn status(&self) -> WalletTransactionStatus {
        self.status
    }

    #[must_use]
    pub const fn block_height(&self) -> Option<u64> {
        self.block_height
    }

    #[must_use]
    pub const fn observed_at(&self) -> Option<UnixTimestampMillis> {
        self.observed_at
    }

    #[must_use]
    pub fn changes(&self) -> &[AssetBalanceChange] {
        &self.changes
    }

    #[must_use]
    pub const fn fee(&self) -> Option<&AssetBalance> {
        self.fee.as_ref()
    }
}

/// State of a chain synchronization attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletSyncState {
    NeverSynced,
    Syncing,
    Synced,
    Stalled,
    Unavailable,
}

/// Lifecycle of the key-scoped Midnight DUST event index.
///
/// DUST synchronization is deliberately separate from the public account
/// snapshot: a cached DUST state is useful for display and resumption, but it
/// is not evidence that the wallet is current enough to spend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustSyncState {
    NeverSynced,
    Syncing,
    Synced,
    Cached,
    Cancelled,
    Stalled,
    Unavailable,
}

/// Sanitized reason why a DUST synchronization is not currently live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustSyncFailure {
    ProtectionNotInitialized,
    ProtectionLocked,
    UnsupportedNetwork,
    TransportUnavailable,
    TimedOut,
    InvalidChainState,
    StorageUnavailable,
}

/// Oxid-owned projection of one profile's DUST synchronization state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustSyncSnapshot {
    network_id: ChainNetworkId,
    state: WalletDustSyncState,
    current_cursor: Option<u64>,
    target_cursor: Option<u64>,
    events_processed: u64,
    balance_atomic_units: Option<u128>,
    updated_at: Option<UnixTimestampMillis>,
    failure: Option<WalletDustSyncFailure>,
}

impl WalletDustSyncSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        network_id: ChainNetworkId,
        state: WalletDustSyncState,
        current_cursor: Option<u64>,
        target_cursor: Option<u64>,
        events_processed: u64,
        balance_atomic_units: Option<u128>,
        updated_at: Option<UnixTimestampMillis>,
        failure: Option<WalletDustSyncFailure>,
    ) -> Result<Self, WalletDustSyncSnapshotError> {
        if current_cursor.is_some() != target_cursor.is_some()
            || current_cursor
                .zip(target_cursor)
                .is_some_and(|(current, target)| current > target)
        {
            return Err(WalletDustSyncSnapshotError::InvalidCursorRange);
        }
        if state == WalletDustSyncState::Synced
            && (current_cursor
                .zip(target_cursor)
                .is_none_or(|(current, target)| current != target)
                || balance_atomic_units.is_none()
                || updated_at.is_none())
        {
            return Err(WalletDustSyncSnapshotError::IncompleteSynchronizedState);
        }
        if matches!(
            state,
            WalletDustSyncState::NeverSynced | WalletDustSyncState::Unavailable
        ) && (current_cursor.is_some()
            || target_cursor.is_some()
            || balance_atomic_units.is_some())
        {
            return Err(WalletDustSyncSnapshotError::UnexpectedIndexedState);
        }

        Ok(Self {
            network_id,
            state,
            current_cursor,
            target_cursor,
            events_processed,
            balance_atomic_units,
            updated_at,
            failure,
        })
    }

    #[must_use]
    pub fn never_synced(network_id: ChainNetworkId) -> Self {
        Self {
            network_id,
            state: WalletDustSyncState::NeverSynced,
            current_cursor: None,
            target_cursor: None,
            events_processed: 0,
            balance_atomic_units: None,
            updated_at: None,
            failure: None,
        }
    }

    #[must_use]
    pub fn unavailable(network_id: ChainNetworkId) -> Self {
        Self {
            network_id,
            state: WalletDustSyncState::Unavailable,
            current_cursor: None,
            target_cursor: None,
            events_processed: 0,
            balance_atomic_units: None,
            updated_at: None,
            failure: Some(WalletDustSyncFailure::TransportUnavailable),
        }
    }

    #[must_use]
    pub const fn network_id(&self) -> &ChainNetworkId {
        &self.network_id
    }

    #[must_use]
    pub const fn state(&self) -> WalletDustSyncState {
        self.state
    }

    #[must_use]
    pub const fn current_cursor(&self) -> Option<u64> {
        self.current_cursor
    }

    #[must_use]
    pub const fn target_cursor(&self) -> Option<u64> {
        self.target_cursor
    }

    #[must_use]
    pub const fn events_processed(&self) -> u64 {
        self.events_processed
    }

    #[must_use]
    pub const fn balance_atomic_units(&self) -> Option<u128> {
        self.balance_atomic_units
    }

    #[must_use]
    pub const fn updated_at(&self) -> Option<UnixTimestampMillis> {
        self.updated_at
    }

    #[must_use]
    pub const fn failure(&self) -> Option<WalletDustSyncFailure> {
        self.failure
    }
}

/// Invalid combinations rejected by the DUST status projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustSyncSnapshotError {
    InvalidCursorRange,
    IncompleteSynchronizedState,
    UnexpectedIndexedState,
}

impl fmt::Display for WalletDustSyncSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidCursorRange => "DUST synchronization cursors are invalid",
            Self::IncompleteSynchronizedState => {
                "synchronized DUST state requires a current cursor, balance, and timestamp"
            }
            Self::UnexpectedIndexedState => {
                "unavailable or unsynchronized DUST state cannot contain indexed values"
            }
        };
        formatter.write_str(message)
    }
}

impl Error for WalletDustSyncSnapshotError {}

/// Safe synchronization metadata surfaced to incoming adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletSyncStatus {
    state: WalletSyncState,
    current_cursor: Option<u64>,
    target_cursor: Option<u64>,
    chain_tip_height: Option<u64>,
    updated_at: Option<UnixTimestampMillis>,
}

impl WalletSyncStatus {
    #[must_use]
    pub const fn new(
        state: WalletSyncState,
        current_cursor: Option<u64>,
        target_cursor: Option<u64>,
        chain_tip_height: Option<u64>,
        updated_at: Option<UnixTimestampMillis>,
    ) -> Self {
        Self {
            state,
            current_cursor,
            target_cursor,
            chain_tip_height,
            updated_at,
        }
    }

    #[must_use]
    pub const fn unavailable() -> Self {
        Self::new(WalletSyncState::Unavailable, None, None, None, None)
    }

    #[must_use]
    pub const fn state(&self) -> WalletSyncState {
        self.state
    }

    #[must_use]
    pub const fn current_cursor(&self) -> Option<u64> {
        self.current_cursor
    }

    #[must_use]
    pub const fn target_cursor(&self) -> Option<u64> {
        self.target_cursor
    }

    #[must_use]
    pub const fn chain_tip_height(&self) -> Option<u64> {
        self.chain_tip_height
    }

    #[must_use]
    pub const fn updated_at(&self) -> Option<UnixTimestampMillis> {
        self.updated_at
    }
}

/// Provenance of account values returned by an adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletAccountSource {
    Live,
    Cached,
    Simulated,
    Unavailable,
}

/// Complete public read model for one profile's selected chain account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletAccountSnapshot {
    network: ChainNetwork,
    account_id: Option<ChainAccountId>,
    source: WalletAccountSource,
    addresses: Vec<ChainAddress>,
    balances: Vec<AssetBalance>,
    sync: WalletSyncStatus,
    transactions: Vec<WalletTransaction>,
}

impl WalletAccountSnapshot {
    #[must_use]
    pub const fn new(
        network: ChainNetwork,
        account_id: Option<ChainAccountId>,
        source: WalletAccountSource,
        addresses: Vec<ChainAddress>,
        balances: Vec<AssetBalance>,
        sync: WalletSyncStatus,
        transactions: Vec<WalletTransaction>,
    ) -> Self {
        Self {
            network,
            account_id,
            source,
            addresses,
            balances,
            sync,
            transactions,
        }
    }

    #[must_use]
    pub fn unavailable(network: ChainNetwork) -> Self {
        Self::new(
            network,
            None,
            WalletAccountSource::Unavailable,
            Vec::new(),
            Vec::new(),
            WalletSyncStatus::unavailable(),
            Vec::new(),
        )
    }

    #[must_use]
    pub const fn network(&self) -> &ChainNetwork {
        &self.network
    }

    #[must_use]
    pub const fn account_id(&self) -> Option<&ChainAccountId> {
        self.account_id.as_ref()
    }

    #[must_use]
    pub const fn source(&self) -> WalletAccountSource {
        self.source
    }

    #[must_use]
    pub fn addresses(&self) -> &[ChainAddress] {
        &self.addresses
    }

    #[must_use]
    pub fn balances(&self) -> &[AssetBalance] {
        &self.balances
    }

    #[must_use]
    pub const fn sync(&self) -> &WalletSyncStatus {
        &self.sync
    }

    #[must_use]
    pub fn transactions(&self) -> &[WalletTransaction] {
        &self.transactions
    }
}

/// Validation failures shared by short public labels and symbols.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicChainTextError {
    Empty,
    TooLong,
    ContainsControlCharacter,
}

impl fmt::Display for PublicChainTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "public chain text must not be empty",
            Self::TooLong => "public chain text exceeds its maximum length",
            Self::ContainsControlCharacter => {
                "public chain text must not contain control characters"
            }
        };
        formatter.write_str(message)
    }
}

impl Error for PublicChainTextError {}

fn parse_public_text(value: &str, maximum: usize) -> Result<String, PublicChainTextError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(PublicChainTextError::Empty);
    }
    if value.chars().count() > maximum {
        return Err(PublicChainTextError::TooLong);
    }
    if value.chars().any(char::is_control) {
        return Err(PublicChainTextError::ContainsControlCharacter);
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network() -> ChainNetwork {
        ChainNetwork::new(
            ChainKind::Midnight,
            ChainNetworkId::parse("undeployed").expect("network id is valid"),
            NetworkDisplayName::parse("Standalone").expect("label is valid"),
            NetworkEnvironment::Development,
        )
    }

    fn night() -> ChainAsset {
        ChainAsset::new(
            ChainAssetId::parse("midnight:night").expect("asset id is valid"),
            AssetSymbol::parse("NIGHT").expect("symbol is valid"),
            6,
        )
    }

    #[test]
    fn network_identity_contains_no_transport_route() {
        let network = network();

        assert_eq!(network.chain(), ChainKind::Midnight);
        assert_eq!(network.id().as_str(), "undeployed");
        assert_eq!(network.display_name().as_str(), "Standalone");
        assert_eq!(network.environment(), NetworkEnvironment::Development);
    }

    #[test]
    fn public_text_normalizes_and_rejects_control_characters() {
        assert_eq!(
            NetworkDisplayName::parse("  PreProd  ")
                .expect("label is valid")
                .as_str(),
            "PreProd"
        );
        assert_eq!(
            AssetSymbol::parse("NIGHT\n"),
            Ok(AssetSymbol("NIGHT".to_owned()))
        );
        assert_eq!(
            AssetSymbol::parse("NI\0GHT"),
            Err(PublicChainTextError::ContainsControlCharacter)
        );
        assert_eq!(
            AssetSymbol::parse("X".repeat(AssetSymbol::MAX_CHARACTERS + 1)),
            Err(PublicChainTextError::TooLong)
        );
    }

    #[test]
    fn address_is_public_but_strictly_bounded() {
        let address =
            ChainAddress::parse(ChainAddressKind::Unshielded, "mn_addr_undeployed1example")
                .expect("address is valid");

        assert_eq!(address.kind(), ChainAddressKind::Unshielded);
        assert_eq!(address.value(), "mn_addr_undeployed1example");
        assert_eq!(
            ChainAddress::parse(ChainAddressKind::Shielded, "mn shielded"),
            Err(ChainAddressError::ContainsWhitespace)
        );
        assert_eq!(
            ChainAddress::parse(ChainAddressKind::Dust, " "),
            Err(ChainAddressError::Empty)
        );
        assert_eq!(
            ChainAddress::parse(
                ChainAddressKind::Reward,
                "a".repeat(ChainAddress::MAX_CHARACTERS + 1)
            ),
            Err(ChainAddressError::TooLong)
        );
    }

    #[test]
    fn account_snapshot_preserves_exact_u128_values_and_source() {
        let amount = u128::MAX - 7;
        let balance = AssetBalance::new(night(), amount);
        let snapshot = WalletAccountSnapshot::new(
            network(),
            Some(ChainAccountId::parse("account_0").expect("account id is valid")),
            WalletAccountSource::Cached,
            Vec::new(),
            vec![balance],
            WalletSyncStatus::new(
                WalletSyncState::Synced,
                Some(42),
                Some(42),
                Some(101),
                Some(UnixTimestampMillis::new(2_000)),
            ),
            Vec::new(),
        );

        assert_eq!(snapshot.source(), WalletAccountSource::Cached);
        assert_eq!(snapshot.balances()[0].atomic_units(), amount);
        assert_eq!(snapshot.sync().chain_tip_height(), Some(101));
    }

    #[test]
    fn transaction_records_changes_and_fees_without_signed_arithmetic() {
        let asset = night();
        let transaction = WalletTransaction::new(
            ChainTransactionId::parse("tx_42").expect("transaction id is valid"),
            WalletTransactionDirection::Outgoing,
            WalletTransactionStatus::Confirmed,
            Some(77),
            Some(UnixTimestampMillis::new(3_000)),
            vec![AssetBalanceChange::new(
                BalanceChangeDirection::Debit,
                AssetBalance::new(asset.clone(), 12),
            )],
            Some(AssetBalance::new(asset, 2)),
        );

        assert_eq!(transaction.id().as_str(), "tx_42");
        assert_eq!(
            transaction.direction(),
            WalletTransactionDirection::Outgoing
        );
        assert_eq!(
            transaction.changes()[0].direction(),
            BalanceChangeDirection::Debit
        );
        assert_eq!(transaction.fee().map(AssetBalance::atomic_units), Some(2));
    }

    #[test]
    fn unavailable_snapshot_carries_no_account_claims() {
        let snapshot = WalletAccountSnapshot::unavailable(network());

        assert_eq!(snapshot.source(), WalletAccountSource::Unavailable);
        assert!(snapshot.account_id().is_none());
        assert!(snapshot.addresses().is_empty());
        assert!(snapshot.balances().is_empty());
        assert_eq!(snapshot.sync().state(), WalletSyncState::Unavailable);
    }

    #[test]
    fn dust_sync_projection_enforces_cursor_and_live_state_invariants() {
        let network_id = ChainNetworkId::parse("undeployed").expect("network is valid");
        assert_eq!(
            WalletDustSyncSnapshot::new(
                network_id.clone(),
                WalletDustSyncState::Syncing,
                Some(3),
                Some(2),
                4,
                Some(9),
                Some(UnixTimestampMillis::new(42)),
                None,
            )
            .err(),
            Some(WalletDustSyncSnapshotError::InvalidCursorRange)
        );
        assert_eq!(
            WalletDustSyncSnapshot::new(
                network_id.clone(),
                WalletDustSyncState::Synced,
                Some(2),
                Some(2),
                3,
                None,
                Some(UnixTimestampMillis::new(42)),
                None,
            )
            .err(),
            Some(WalletDustSyncSnapshotError::IncompleteSynchronizedState)
        );
        let cached = WalletDustSyncSnapshot::new(
            network_id,
            WalletDustSyncState::Cached,
            Some(1),
            Some(2),
            0,
            Some(u128::MAX),
            Some(UnixTimestampMillis::new(42)),
            Some(WalletDustSyncFailure::TransportUnavailable),
        )
        .expect("partial cached state is valid");
        assert_eq!(cached.balance_atomic_units(), Some(u128::MAX));
        assert_eq!(cached.current_cursor(), Some(1));
    }

    #[test]
    fn derived_account_rejects_bip32_indices_at_two_to_the_thirty_first() {
        let make = |account_index, address_index| {
            DerivedChainAccount::new(
                network().id().clone(),
                ChainAccountId::parse("midnight_account_0_0").expect("account id is valid"),
                account_index,
                address_index,
                ChainAddress::parse(ChainAddressKind::Unshielded, "mn_addr_undeployed1derived")
                    .expect("address is valid"),
                WalletPublicKey::new(crate::PublicKeyEncoding::Secp256k1XOnly, vec![7; 32]),
                WalletKeyReference::parse("key_derived").expect("key reference is valid"),
            )
        };

        assert!(make(MAX_HD_CHILD_INDEX, MAX_HD_CHILD_INDEX).is_ok());
        assert_eq!(
            make(MAX_HD_CHILD_INDEX + 1, 0),
            Err(ChainAccountDerivationError::AccountIndexOutOfBounds)
        );
        assert_eq!(
            make(0, MAX_HD_CHILD_INDEX + 1),
            Err(ChainAccountDerivationError::AddressIndexOutOfBounds)
        );
    }
}
