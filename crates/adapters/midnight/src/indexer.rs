// SPDX-License-Identifier: Apache-2.0

//! Native standalone-indexer transport for public Midnight account state.
//!
//! The transport is deliberately absent on `wasm32`: browser networking needs
//! a separate WebSocket adapter and origin policy. Native calls execute on a
//! short-lived worker runtime so network I/O never blocks an incoming adapter.

use std::{
    collections::{BTreeMap, HashMap},
    fmt, thread,
    time::Duration,
};

use bech32::{Bech32m, primitives::decode::CheckedHrpstring};
use futures::{SinkExt, StreamExt, channel::oneshot, future::BoxFuture};
use oxid_foundation::UnixTimestampMillis;
use oxid_platform_ports::ClockPort;
use oxid_wallet_application::{
    WalletAccountPortError, WalletAccountPortFuture, WalletKeyDerivationPort,
};
use oxid_wallet_domain::{
    AssetBalance, AssetBalanceChange, AssetSymbol, BalanceChangeDirection, ChainAccountId,
    ChainAddress, ChainAddressKind, ChainAsset, ChainAssetId, ChainNetwork, ChainNetworkId,
    ChainTransactionId, DerivedChainAccount, WalletAccountSnapshot, WalletAccountSource,
    WalletProfileId, WalletSyncState, WalletSyncStatus, WalletTransaction,
    WalletTransactionDirection, WalletTransactionStatus,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};

use super::{
    MidnightAccountSource, MidnightWalletAdapter, ProtectedMidnightAccountDeriver, SPECKS_PER_DUST,
    STARS_PER_NIGHT, decimal_places, midnight_asset, network_by_id,
};

const INDEXER_QUERY: &str = include_str!("../queries/unshielded_transactions.graphql");
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const ACK_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(5);
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const MAX_ENDPOINT_CHARACTERS: usize = 2_048;
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_FRAME_BYTES: usize = 256 * 1024;
const MAX_EVENTS: usize = 100_000;
const MAX_UTXO_RECORDS: usize = 100_000;
const NATIVE_NIGHT_TOKEN_TYPE: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Validated native indexer configuration supplied only at composition time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightIndexerConfig {
    network_id: ChainNetworkId,
    websocket_url: String,
    unshielded_address: ChainAddress,
}

impl MidnightIndexerConfig {
    /// Validates a route and public address without retaining credentials or secrets.
    pub fn new(
        network_id: impl Into<String>,
        websocket_url: impl AsRef<str>,
        unshielded_address: impl AsRef<str>,
    ) -> Result<Self, MidnightIndexerConfigError> {
        let network_id = ChainNetworkId::parse(network_id.into())
            .map_err(|_| MidnightIndexerConfigError::InvalidNetwork)?;
        if network_by_id(&network_id)
            .map_err(|_| MidnightIndexerConfigError::InvalidNetwork)?
            .is_none()
        {
            return Err(MidnightIndexerConfigError::InvalidNetwork);
        }

        let websocket_url = validate_websocket_url(websocket_url.as_ref())?;
        let unshielded_address =
            validate_unshielded_address(&network_id, unshielded_address.as_ref())?;

        Ok(Self {
            network_id,
            websocket_url,
            unshielded_address,
        })
    }

    #[must_use]
    pub const fn network_id(&self) -> &ChainNetworkId {
        &self.network_id
    }

    #[must_use]
    pub fn websocket_url(&self) -> &str {
        &self.websocket_url
    }

    #[must_use]
    pub const fn unshielded_address(&self) -> &ChainAddress {
        &self.unshielded_address
    }
}

/// Safe configuration failures; values are deliberately excluded from messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightIndexerConfigError {
    InvalidNetwork,
    InvalidEndpoint,
    EndpointTooLong,
    EndpointCredentialsForbidden,
    EndpointQueryForbidden,
    InvalidAddress,
    AddressNetworkMismatch,
}

impl fmt::Display for MidnightIndexerConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidNetwork => "Midnight indexer network is not supported",
            Self::InvalidEndpoint => "Midnight indexer endpoint must be a valid ws or wss URL",
            Self::EndpointTooLong => "Midnight indexer endpoint is too long",
            Self::EndpointCredentialsForbidden => {
                "Midnight indexer endpoint must not contain credentials"
            }
            Self::EndpointQueryForbidden => {
                "Midnight indexer endpoint must not contain a query or fragment"
            }
            Self::InvalidAddress => "Midnight unshielded address is invalid",
            Self::AddressNetworkMismatch => {
                "Midnight unshielded address does not match the configured network"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for MidnightIndexerConfigError {}

/// Builds the native live account adapter from explicit public configuration.
pub fn live_midnight_wallet<C>(
    config: MidnightIndexerConfig,
    clock: std::sync::Arc<C>,
) -> MidnightWalletAdapter<LiveMidnightAccountSource<C>>
where
    C: ClockPort,
{
    let default_network = config.network_id.clone();
    let source = LiveMidnightAccountSource::new(config, clock);
    MidnightWalletAdapter::with_default_network(source, default_network)
}

/// Builds live sync with the same protected derivation port used by custody.
pub fn protected_live_midnight_wallet<C, K>(
    config: MidnightIndexerConfig,
    clock: std::sync::Arc<C>,
    keys: std::sync::Arc<K>,
) -> MidnightWalletAdapter<LiveMidnightAccountSource<C>, ProtectedMidnightAccountDeriver<K>>
where
    C: ClockPort,
    K: WalletKeyDerivationPort,
{
    let default_network = config.network_id.clone();
    let source = LiveMidnightAccountSource::new(config, clock);
    MidnightWalletAdapter::with_default_network_and_deriver(
        source,
        default_network,
        ProtectedMidnightAccountDeriver::new(keys),
    )
}

/// Live unshielded account source backed by a replaceable indexer transport.
pub struct LiveMidnightAccountSource<C> {
    network_id: ChainNetworkId,
    address: ChainAddress,
    clock: std::sync::Arc<C>,
    transport: std::sync::Arc<dyn MidnightIndexerTransport>,
    cached: std::sync::Mutex<HashMap<WalletProfileId, WalletAccountSnapshot>>,
    derived_accounts: std::sync::Mutex<HashMap<WalletProfileId, DerivedChainAccount>>,
}

impl<C> LiveMidnightAccountSource<C> {
    fn new(config: MidnightIndexerConfig, clock: std::sync::Arc<C>) -> Self {
        let transport =
            std::sync::Arc::new(WebSocketMidnightIndexerTransport::new(config.websocket_url));
        Self::with_transport(
            config.network_id,
            config.unshielded_address,
            clock,
            transport,
        )
    }

    fn with_transport(
        network_id: ChainNetworkId,
        address: ChainAddress,
        clock: std::sync::Arc<C>,
        transport: std::sync::Arc<dyn MidnightIndexerTransport>,
    ) -> Self {
        Self {
            network_id,
            address,
            clock,
            transport,
            cached: std::sync::Mutex::new(HashMap::new()),
            derived_accounts: std::sync::Mutex::new(HashMap::new()),
        }
    }

    fn ensure_network(&self, network: &ChainNetwork) -> Result<(), WalletAccountPortError> {
        if network.id() == &self.network_id {
            Ok(())
        } else {
            Err(WalletAccountPortError::UnsupportedNetwork)
        }
    }

    fn initial_snapshot(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
        let (account_id, address) = self.active_account(profile_id)?;
        Ok(WalletAccountSnapshot::new(
            network.clone(),
            Some(account_id),
            WalletAccountSource::Live,
            vec![address],
            Vec::new(),
            WalletSyncStatus::new(WalletSyncState::NeverSynced, None, None, None, None),
            Vec::new(),
        ))
    }

    fn active_account(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<(ChainAccountId, ChainAddress), WalletAccountPortError> {
        self.derived_accounts
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .get(profile_id)
            .map(|derived| {
                (
                    derived.account_id().clone(),
                    derived.receive_address().clone(),
                )
            })
            .map_or_else(|| Ok((account_id(profile_id)?, self.address.clone())), Ok)
    }

    fn replace_sync_status(
        &self,
        snapshot: &WalletAccountSnapshot,
        source: WalletAccountSource,
        sync: WalletSyncStatus,
    ) -> WalletAccountSnapshot {
        WalletAccountSnapshot::new(
            snapshot.network().clone(),
            snapshot.account_id().cloned(),
            source,
            snapshot.addresses().to_vec(),
            snapshot.balances().to_vec(),
            sync,
            snapshot.transactions().to_vec(),
        )
    }

    fn store(
        &self,
        profile_id: WalletProfileId,
        snapshot: WalletAccountSnapshot,
    ) -> Result<(), WalletAccountPortError> {
        self.cached
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .insert(profile_id, snapshot);
        Ok(())
    }

    fn store_stalled(
        &self,
        profile_id: &WalletProfileId,
        previous: &WalletAccountSnapshot,
    ) -> Result<(), WalletAccountPortError> {
        let source = if previous.sync().state() == WalletSyncState::NeverSynced {
            WalletAccountSource::Live
        } else {
            WalletAccountSource::Cached
        };
        let stalled = self.replace_sync_status(
            previous,
            source,
            WalletSyncStatus::new(
                WalletSyncState::Stalled,
                previous.sync().current_cursor(),
                previous.sync().target_cursor(),
                previous.sync().chain_tip_height(),
                previous.sync().updated_at(),
            ),
        );
        self.store(profile_id.clone(), stalled)
    }
}

impl<C> MidnightAccountSource for LiveMidnightAccountSource<C>
where
    C: ClockPort + 'static,
{
    fn bind_derived_account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
        derived: &DerivedChainAccount,
    ) -> Result<(), WalletAccountPortError> {
        self.ensure_network(network)?;
        if derived.network_id() != network.id() {
            return Err(WalletAccountPortError::InvalidData);
        }
        let mut accounts = self
            .derived_accounts
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?;
        if accounts.get(profile_id) != Some(derived) {
            accounts.insert(profile_id.clone(), derived.clone());
            drop(accounts);
            self.cached
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)?
                .remove(profile_id);
        }
        Ok(())
    }

    fn account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
        self.ensure_network(network)?;
        let (expected_account_id, expected_address) = self.active_account(profile_id)?;
        self.cached
            .lock()
            .map_err(|_| WalletAccountPortError::Unavailable)?
            .get(profile_id)
            .cloned()
            .filter(|snapshot| {
                snapshot.account_id() == Some(&expected_account_id)
                    && snapshot.addresses() == [expected_address.clone()]
            })
            .map_or_else(|| self.initial_snapshot(profile_id, network), Ok)
    }

    fn sync<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        network: &'a ChainNetwork,
    ) -> WalletAccountPortFuture<'a> {
        Box::pin(async move {
            self.ensure_network(network)?;
            let previous = self.account(profile_id, network)?;
            let syncing = self.replace_sync_status(
                &previous,
                previous.source(),
                WalletSyncStatus::new(
                    WalletSyncState::Syncing,
                    previous.sync().current_cursor(),
                    previous.sync().target_cursor(),
                    previous.sync().chain_tip_height(),
                    previous.sync().updated_at(),
                ),
            );
            self.store(profile_id.clone(), syncing)?;

            let address = previous
                .addresses()
                .first()
                .ok_or(WalletAccountPortError::InvalidData)?
                .clone();

            let indexer = match self.transport.snapshot(address.value()).await {
                Ok(indexer) => indexer,
                Err(error) => {
                    self.store_stalled(profile_id, &previous)?;
                    return Err(error.wallet_error());
                }
            };
            let (balances, transactions) = match map_indexer_snapshot(&indexer) {
                Ok(mapped) => mapped,
                Err(error) => {
                    self.store_stalled(profile_id, &previous)?;
                    return Err(error);
                }
            };
            let updated_at = match self.clock.now() {
                Ok(updated_at) => updated_at,
                Err(_) => {
                    self.store_stalled(profile_id, &previous)?;
                    return Err(WalletAccountPortError::Unavailable);
                }
            };
            let sync = WalletSyncStatus::new(
                WalletSyncState::Synced,
                Some(indexer.current_cursor),
                Some(indexer.target_cursor),
                indexer.chain_tip_height,
                Some(updated_at),
            );
            let live = WalletAccountSnapshot::new(
                network.clone(),
                previous.account_id().cloned(),
                WalletAccountSource::Live,
                vec![address],
                balances,
                sync,
                transactions,
            );
            let cached =
                self.replace_sync_status(&live, WalletAccountSource::Cached, live.sync().clone());
            self.store(profile_id.clone(), cached)?;
            Ok(live)
        })
    }
}

trait MidnightIndexerTransport: Send + Sync {
    fn snapshot<'a>(
        &'a self,
        address: &'a str,
    ) -> BoxFuture<'a, Result<IndexerSnapshot, IndexerTransportError>>;
}

struct WebSocketMidnightIndexerTransport {
    endpoint: String,
}

impl WebSocketMidnightIndexerTransport {
    fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl MidnightIndexerTransport for WebSocketMidnightIndexerTransport {
    fn snapshot<'a>(
        &'a self,
        address: &'a str,
    ) -> BoxFuture<'a, Result<IndexerSnapshot, IndexerTransportError>> {
        let endpoint = self.endpoint.clone();
        let address = address.to_owned();
        Box::pin(async move {
            let (sender, receiver) = oneshot::channel();
            thread::Builder::new()
                .name("oxid-midnight-indexer".to_owned())
                .spawn(move || {
                    let result = tokio::runtime::Builder::new_current_thread()
                        .enable_io()
                        .enable_time()
                        .build()
                        .map_err(|_| IndexerTransportError::Runtime)
                        .and_then(|runtime| {
                            runtime.block_on(indexer_snapshot(&endpoint, &address))
                        });
                    let _ = sender.send(result);
                })
                .map_err(|_| IndexerTransportError::Runtime)?;
            receiver.await.map_err(|_| IndexerTransportError::Runtime)?
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexerTransportError {
    Runtime,
    Connect,
    Timeout,
    Protocol,
    InvalidData,
    LimitExceeded,
}

impl IndexerTransportError {
    const fn wallet_error(self) -> WalletAccountPortError {
        match self {
            Self::Runtime | Self::Connect | Self::Timeout => WalletAccountPortError::Unavailable,
            Self::Protocol | Self::InvalidData | Self::LimitExceeded => {
                WalletAccountPortError::InvalidData
            }
        }
    }
}

async fn indexer_snapshot(
    endpoint: &str,
    address: &str,
) -> Result<IndexerSnapshot, IndexerTransportError> {
    ensure_tls_provider()?;
    let mut request = endpoint
        .into_client_request()
        .map_err(|_| IndexerTransportError::Connect)?;
    request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        "graphql-transport-ws"
            .parse()
            .map_err(|_| IndexerTransportError::Protocol)?,
    );
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(MAX_MESSAGE_BYTES);
    config.max_frame_size = Some(MAX_FRAME_BYTES);
    let (mut socket, response) = timeout(
        CONNECT_TIMEOUT,
        connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| IndexerTransportError::Timeout)?
    .map_err(|_| IndexerTransportError::Connect)?;
    if response
        .headers()
        .get("Sec-WebSocket-Protocol")
        .and_then(|value| value.to_str().ok())
        != Some("graphql-transport-ws")
    {
        return Err(IndexerTransportError::Protocol);
    }

    socket
        .send(Message::Text(
            json!({ "type": "connection_init", "payload": {} })
                .to_string()
                .into(),
        ))
        .await
        .map_err(|_| IndexerTransportError::Protocol)?;
    wait_for_ack(&mut socket).await?;

    socket
        .send(Message::Text(
            json!({
                "type": "subscribe",
                "id": "oxid-account",
                "payload": {
                    "query": INDEXER_QUERY,
                    "variables": { "address": address, "transactionId": 0 }
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .map_err(|_| IndexerTransportError::Protocol)?;

    let mut accumulator = SnapshotAccumulator::default();
    timeout(SNAPSHOT_TIMEOUT, async {
        while !accumulator.complete() {
            let message = timeout(IDLE_TIMEOUT, socket.next())
                .await
                .map_err(|_| IndexerTransportError::Timeout)?
                .ok_or(IndexerTransportError::Protocol)?
                .map_err(|_| IndexerTransportError::Protocol)?;
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(text.as_str())
                        .map_err(|_| IndexerTransportError::InvalidData)?;
                    match message_type(&value)? {
                        "next" => {
                            if value.get("id").and_then(Value::as_str) != Some("oxid-account") {
                                return Err(IndexerTransportError::Protocol);
                            }
                            let payload = value
                                .get("payload")
                                .ok_or(IndexerTransportError::InvalidData)?;
                            if payload
                                .get("errors")
                                .and_then(Value::as_array)
                                .is_some_and(|errors| !errors.is_empty())
                            {
                                return Err(IndexerTransportError::Protocol);
                            }
                            let data = payload
                                .get("data")
                                .ok_or(IndexerTransportError::InvalidData)?;
                            accumulator.apply(decode_event(data, address)?)?;
                        }
                        "ping" => send_json_pong(&mut socket, &value).await?,
                        "pong" => {}
                        "complete" | "error" => return Err(IndexerTransportError::Protocol),
                        _ => return Err(IndexerTransportError::Protocol),
                    }
                }
                Message::Ping(payload) => socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|_| IndexerTransportError::Protocol)?,
                Message::Pong(_) => {}
                Message::Close(_) => return Err(IndexerTransportError::Protocol),
                _ => return Err(IndexerTransportError::Protocol),
            }
        }
        Ok::<(), IndexerTransportError>(())
    })
    .await
    .map_err(|_| IndexerTransportError::Timeout)??;

    let _ = socket
        .send(Message::Text(
            json!({ "type": "complete", "id": "oxid-account" })
                .to_string()
                .into(),
        ))
        .await;
    let _ = socket.close(None).await;
    accumulator.finish()
}

fn ensure_tls_provider() -> Result<(), IndexerTransportError> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    rustls::crypto::CryptoProvider::get_default()
        .map(|_| ())
        .ok_or(IndexerTransportError::Runtime)
}

async fn wait_for_ack<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Result<(), IndexerTransportError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    timeout(ACK_TIMEOUT, async {
        loop {
            let message = socket
                .next()
                .await
                .ok_or(IndexerTransportError::Protocol)?
                .map_err(|_| IndexerTransportError::Protocol)?;
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(text.as_str())
                        .map_err(|_| IndexerTransportError::InvalidData)?;
                    match message_type(&value)? {
                        "connection_ack" => return Ok(()),
                        "ping" => send_json_pong(socket, &value).await?,
                        _ => return Err(IndexerTransportError::Protocol),
                    }
                }
                Message::Ping(payload) => socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|_| IndexerTransportError::Protocol)?,
                Message::Pong(_) => {}
                _ => return Err(IndexerTransportError::Protocol),
            }
        }
    })
    .await
    .map_err(|_| IndexerTransportError::Timeout)?
}

async fn send_json_pong<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    ping: &Value,
) -> Result<(), IndexerTransportError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut pong = json!({ "type": "pong" });
    if let Some(payload) = ping.get("payload") {
        pong["payload"] = payload.clone();
    }
    socket
        .send(Message::Text(pong.to_string().into()))
        .await
        .map_err(|_| IndexerTransportError::Protocol)
}

fn message_type(value: &Value) -> Result<&str, IndexerTransportError> {
    value
        .get("type")
        .and_then(Value::as_str)
        .ok_or(IndexerTransportError::InvalidData)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IndexerUtxo {
    token_type: String,
    value: u128,
    intent_hash: String,
    output_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct UtxoKey {
    intent_hash: String,
    output_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IndexerTransaction {
    hash: String,
    block_height: u64,
    timestamp_millis: u64,
    status: IndexerTransactionStatus,
    fee_specks: Option<u128>,
    created: Vec<IndexerUtxo>,
    spent: Vec<IndexerUtxo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexerTransactionStatus {
    Success,
    PartialSuccess,
    Failure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum IndexerEvent {
    Transaction {
        cursor: u64,
        transaction: IndexerTransaction,
    },
    Progress {
        target: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IndexerSnapshot {
    current_cursor: u64,
    target_cursor: u64,
    chain_tip_height: Option<u64>,
    utxos: Vec<IndexerUtxo>,
    transactions: Vec<IndexerTransaction>,
}

#[derive(Default)]
struct SnapshotAccumulator {
    current_cursor: u64,
    target_cursor: Option<u64>,
    event_count: usize,
    utxo_record_count: usize,
    utxos: BTreeMap<UtxoKey, IndexerUtxo>,
    transactions: BTreeMap<String, IndexerTransaction>,
}

impl SnapshotAccumulator {
    fn apply(&mut self, event: IndexerEvent) -> Result<(), IndexerTransportError> {
        self.event_count = self
            .event_count
            .checked_add(1)
            .ok_or(IndexerTransportError::LimitExceeded)?;
        if self.event_count > MAX_EVENTS {
            return Err(IndexerTransportError::LimitExceeded);
        }
        match event {
            IndexerEvent::Progress { target } => {
                if target < self.current_cursor {
                    return Err(IndexerTransportError::Protocol);
                }
                if self.target_cursor.replace(target).is_some() {
                    return Err(IndexerTransportError::Protocol);
                }
            }
            IndexerEvent::Transaction {
                cursor,
                transaction,
            } => {
                if let Some(existing) = self.transactions.get(&transaction.hash) {
                    return if existing == &transaction && cursor == self.current_cursor {
                        Ok(())
                    } else {
                        Err(IndexerTransportError::InvalidData)
                    };
                }
                if cursor <= self.current_cursor && !self.transactions.is_empty() {
                    return Err(IndexerTransportError::Protocol);
                }
                if self.target_cursor.is_some_and(|target| cursor > target) {
                    return Err(IndexerTransportError::Protocol);
                }
                self.utxo_record_count = self
                    .utxo_record_count
                    .checked_add(transaction.created.len())
                    .and_then(|count| count.checked_add(transaction.spent.len()))
                    .ok_or(IndexerTransportError::LimitExceeded)?;
                if self.utxo_record_count > MAX_UTXO_RECORDS {
                    return Err(IndexerTransportError::LimitExceeded);
                }
                self.current_cursor = cursor;
                for utxo in &transaction.spent {
                    match self.utxos.remove(&utxo_key(utxo)) {
                        Some(existing) if existing == *utxo => {}
                        Some(_) | None => return Err(IndexerTransportError::InvalidData),
                    }
                }
                for utxo in &transaction.created {
                    if self.utxos.insert(utxo_key(utxo), utxo.clone()).is_some() {
                        return Err(IndexerTransportError::InvalidData);
                    }
                }
                self.transactions
                    .insert(transaction.hash.clone(), transaction);
            }
        }
        Ok(())
    }

    fn complete(&self) -> bool {
        self.target_cursor
            .is_some_and(|target| self.current_cursor >= target)
    }

    fn finish(self) -> Result<IndexerSnapshot, IndexerTransportError> {
        let target_cursor = self.target_cursor.ok_or(IndexerTransportError::Protocol)?;
        if self.current_cursor < target_cursor {
            return Err(IndexerTransportError::Protocol);
        }
        let chain_tip_height = self
            .transactions
            .values()
            .map(|transaction| transaction.block_height)
            .max();
        Ok(IndexerSnapshot {
            current_cursor: self.current_cursor,
            target_cursor,
            chain_tip_height,
            utxos: self.utxos.into_values().collect(),
            transactions: self.transactions.into_values().collect(),
        })
    }
}

fn utxo_key(utxo: &IndexerUtxo) -> UtxoKey {
    UtxoKey {
        intent_hash: utxo.intent_hash.clone(),
        output_index: utxo.output_index,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlEvent {
    #[serde(rename = "__typename")]
    typename: String,
    transaction: Option<GraphqlTransaction>,
    created_utxos: Option<Vec<GraphqlUtxo>>,
    spent_utxos: Option<Vec<GraphqlUtxo>>,
    highest_transaction_id: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlTransaction {
    id: i64,
    hash: String,
    block: GraphqlBlock,
    #[serde(rename = "__typename")]
    typename: String,
    transaction_result: Option<GraphqlTransactionResult>,
    fee: Option<String>,
}

#[derive(Deserialize)]
struct GraphqlBlock {
    height: i64,
    timestamp: i64,
}

#[derive(Deserialize)]
struct GraphqlTransactionResult {
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlUtxo {
    owner: String,
    token_type: String,
    value: String,
    intent_hash: String,
    output_index: i64,
}

fn decode_event(
    data: &Value,
    expected_address: &str,
) -> Result<IndexerEvent, IndexerTransportError> {
    let raw = data
        .get("unshieldedTransactions")
        .ok_or(IndexerTransportError::InvalidData)?;
    let event: GraphqlEvent =
        serde_json::from_value(raw.clone()).map_err(|_| IndexerTransportError::InvalidData)?;
    match event.typename.as_str() {
        "UnshieldedTransactionsProgress" => Ok(IndexerEvent::Progress {
            target: nonnegative_i64(
                event
                    .highest_transaction_id
                    .ok_or(IndexerTransportError::InvalidData)?,
            )?,
        }),
        "UnshieldedTransaction" => {
            let transaction = event
                .transaction
                .ok_or(IndexerTransportError::InvalidData)?;
            let cursor = nonnegative_i64(transaction.id)?;
            let created = event
                .created_utxos
                .ok_or(IndexerTransportError::InvalidData)?
                .into_iter()
                .map(|utxo| decode_utxo(utxo, expected_address))
                .collect::<Result<Vec<_>, _>>()?;
            let spent = event
                .spent_utxos
                .ok_or(IndexerTransportError::InvalidData)?
                .into_iter()
                .map(|utxo| decode_utxo(utxo, expected_address))
                .collect::<Result<Vec<_>, _>>()?;
            let (status, fee_specks) = match transaction.typename.as_str() {
                "RegularTransaction" => {
                    let status = transaction
                        .transaction_result
                        .ok_or(IndexerTransportError::InvalidData)?
                        .status;
                    let status = match status.as_str() {
                        "SUCCESS" => IndexerTransactionStatus::Success,
                        "PARTIAL_SUCCESS" => IndexerTransactionStatus::PartialSuccess,
                        "FAILURE" => IndexerTransactionStatus::Failure,
                        _ => return Err(IndexerTransportError::InvalidData),
                    };
                    let fee = transaction
                        .fee
                        .ok_or(IndexerTransportError::InvalidData)?
                        .parse::<u128>()
                        .map_err(|_| IndexerTransportError::InvalidData)?;
                    (status, Some(fee))
                }
                "SystemTransaction" => (IndexerTransactionStatus::Success, None),
                _ => return Err(IndexerTransportError::InvalidData),
            };
            Ok(IndexerEvent::Transaction {
                cursor,
                transaction: IndexerTransaction {
                    hash: normalize_hex_32(&transaction.hash)?,
                    block_height: nonnegative_i64(transaction.block.height)?,
                    timestamp_millis: nonnegative_i64(transaction.block.timestamp)?,
                    status,
                    fee_specks,
                    created,
                    spent,
                },
            })
        }
        _ => Err(IndexerTransportError::InvalidData),
    }
}

fn decode_utxo(
    utxo: GraphqlUtxo,
    expected_address: &str,
) -> Result<IndexerUtxo, IndexerTransportError> {
    if utxo.owner != expected_address {
        return Err(IndexerTransportError::InvalidData);
    }
    let output_index =
        u32::try_from(utxo.output_index).map_err(|_| IndexerTransportError::InvalidData)?;
    Ok(IndexerUtxo {
        token_type: normalize_hex_32(&utxo.token_type)?,
        value: utxo
            .value
            .parse::<u128>()
            .map_err(|_| IndexerTransportError::InvalidData)?,
        intent_hash: normalize_hex_32(&utxo.intent_hash)?,
        output_index,
    })
}

fn normalize_hex_32(value: &str) -> Result<String, IndexerTransportError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(IndexerTransportError::InvalidData);
    }
    Ok(value.to_ascii_lowercase())
}

fn nonnegative_i64(value: i64) -> Result<u64, IndexerTransportError> {
    u64::try_from(value).map_err(|_| IndexerTransportError::InvalidData)
}

fn map_indexer_snapshot(
    indexer: &IndexerSnapshot,
) -> Result<(Vec<AssetBalance>, Vec<WalletTransaction>), WalletAccountPortError> {
    let mut balance_by_token = BTreeMap::<String, u128>::new();
    for utxo in &indexer.utxos {
        let balance = balance_by_token.entry(utxo.token_type.clone()).or_default();
        *balance = balance
            .checked_add(utxo.value)
            .ok_or(WalletAccountPortError::InvalidData)?;
    }
    let balances = balance_by_token
        .into_iter()
        .map(|(token, amount)| Ok(AssetBalance::new(asset_for_token(&token)?, amount)))
        .collect::<Result<Vec<_>, WalletAccountPortError>>()?;

    let transactions = indexer
        .transactions
        .iter()
        .map(map_transaction)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((balances, transactions))
}

fn map_transaction(
    transaction: &IndexerTransaction,
) -> Result<WalletTransaction, WalletAccountPortError> {
    let direction = transaction_direction(&transaction.created, &transaction.spent)?;
    let mut changes = aggregate_changes(&transaction.created, BalanceChangeDirection::Credit)?;
    changes.extend(aggregate_changes(
        &transaction.spent,
        BalanceChangeDirection::Debit,
    )?);
    let status = match transaction.status {
        IndexerTransactionStatus::Success => WalletTransactionStatus::Confirmed,
        IndexerTransactionStatus::PartialSuccess => WalletTransactionStatus::PartiallyApplied,
        IndexerTransactionStatus::Failure => WalletTransactionStatus::Failed,
    };
    let fee = transaction
        .fee_specks
        .map(|amount| {
            Ok(AssetBalance::new(
                midnight_asset("midnight:dust", "DUST", SPECKS_PER_DUST)?,
                amount,
            ))
        })
        .transpose()?;
    Ok(WalletTransaction::new(
        ChainTransactionId::parse(transaction.hash.clone())
            .map_err(|_| WalletAccountPortError::InvalidData)?,
        direction,
        status,
        Some(transaction.block_height),
        Some(UnixTimestampMillis::new(transaction.timestamp_millis)),
        changes,
        fee,
    ))
}

fn transaction_direction(
    created: &[IndexerUtxo],
    spent: &[IndexerUtxo],
) -> Result<WalletTransactionDirection, WalletAccountPortError> {
    match (created.is_empty(), spent.is_empty()) {
        (false, true) => return Ok(WalletTransactionDirection::Incoming),
        (true, false) => return Ok(WalletTransactionDirection::Outgoing),
        (true, true) => return Ok(WalletTransactionDirection::Unknown),
        (false, false) => {}
    }
    let created = amounts_by_token(created)?;
    let spent = amounts_by_token(spent)?;
    let mut has_credit_surplus = false;
    let mut has_debit_surplus = false;
    for token in created.keys().chain(spent.keys()) {
        let credit = created.get(token).copied().unwrap_or_default();
        let debit = spent.get(token).copied().unwrap_or_default();
        has_credit_surplus |= credit > debit;
        has_debit_surplus |= debit > credit;
    }
    Ok(match (has_credit_surplus, has_debit_surplus) {
        (false, true) => WalletTransactionDirection::Outgoing,
        (true, false) => WalletTransactionDirection::Incoming,
        (false, false) => WalletTransactionDirection::SelfTransfer,
        (true, true) => WalletTransactionDirection::Unknown,
    })
}

fn amounts_by_token(
    utxos: &[IndexerUtxo],
) -> Result<BTreeMap<String, u128>, WalletAccountPortError> {
    let mut by_token = BTreeMap::<String, u128>::new();
    for utxo in utxos {
        let amount = by_token.entry(utxo.token_type.clone()).or_default();
        *amount = amount
            .checked_add(utxo.value)
            .ok_or(WalletAccountPortError::InvalidData)?;
    }
    Ok(by_token)
}

fn aggregate_changes(
    utxos: &[IndexerUtxo],
    direction: BalanceChangeDirection,
) -> Result<Vec<AssetBalanceChange>, WalletAccountPortError> {
    amounts_by_token(utxos)?
        .into_iter()
        .map(|(token, amount)| {
            Ok(AssetBalanceChange::new(
                direction,
                AssetBalance::new(asset_for_token(&token)?, amount),
            ))
        })
        .collect()
}

fn asset_for_token(token_type: &str) -> Result<ChainAsset, WalletAccountPortError> {
    if token_type == NATIVE_NIGHT_TOKEN_TYPE {
        return midnight_asset("midnight:night", "NIGHT", STARS_PER_NIGHT);
    }
    let prefix = token_type
        .get(..8)
        .ok_or(WalletAccountPortError::InvalidData)?;
    Ok(ChainAsset::new(
        ChainAssetId::parse(format!("midnight:unshielded:{token_type}"))
            .map_err(|_| WalletAccountPortError::InvalidData)?,
        AssetSymbol::parse(format!("TKN-{prefix}"))
            .map_err(|_| WalletAccountPortError::InvalidData)?,
        decimal_places(1).ok_or(WalletAccountPortError::InvalidData)?,
    ))
}

fn account_id(profile_id: &WalletProfileId) -> Result<ChainAccountId, WalletAccountPortError> {
    ChainAccountId::parse(profile_id.as_str().to_owned())
        .map_err(|_| WalletAccountPortError::InvalidData)
}

fn validate_websocket_url(value: &str) -> Result<String, MidnightIndexerConfigError> {
    if value.chars().count() > MAX_ENDPOINT_CHARACTERS {
        return Err(MidnightIndexerConfigError::EndpointTooLong);
    }
    if value.is_empty()
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
    {
        return Err(MidnightIndexerConfigError::InvalidEndpoint);
    }
    if value.contains('#') {
        return Err(MidnightIndexerConfigError::EndpointQueryForbidden);
    }
    let request = value
        .into_client_request()
        .map_err(|_| MidnightIndexerConfigError::InvalidEndpoint)?;
    let uri = request.uri();
    if !matches!(uri.scheme_str(), Some("ws" | "wss")) || uri.host().is_none() {
        return Err(MidnightIndexerConfigError::InvalidEndpoint);
    }
    if uri
        .authority()
        .is_some_and(|authority| authority.as_str().contains('@'))
    {
        return Err(MidnightIndexerConfigError::EndpointCredentialsForbidden);
    }
    if uri.query().is_some() {
        return Err(MidnightIndexerConfigError::EndpointQueryForbidden);
    }
    Ok(value.to_owned())
}

fn validate_unshielded_address(
    network_id: &ChainNetworkId,
    value: &str,
) -> Result<ChainAddress, MidnightIndexerConfigError> {
    if value.trim() != value {
        return Err(MidnightIndexerConfigError::InvalidAddress);
    }
    let address = ChainAddress::parse(ChainAddressKind::Unshielded, value)
        .map_err(|_| MidnightIndexerConfigError::InvalidAddress)?;
    let decoded = CheckedHrpstring::new::<Bech32m>(address.value())
        .map_err(|_| MidnightIndexerConfigError::InvalidAddress)?;
    if decoded.byte_iter().count() != 32 {
        return Err(MidnightIndexerConfigError::InvalidAddress);
    }
    let expected_hrp = if network_id.as_str() == "mainnet" {
        "mn_addr".to_owned()
    } else {
        format!("mn_addr_{}", network_id.as_str())
    };
    if decoded.hrp().as_str() != expected_hrp {
        return Err(MidnightIndexerConfigError::AddressNetworkMismatch);
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
        task::{Context, Poll, Waker},
    };

    use oxid_platform_ports::PlatformError;
    use serde_json::json;

    use super::*;
    use crate::{fixture_addresses, network_id};

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    struct ScriptedTransport {
        results: Mutex<VecDeque<Result<IndexerSnapshot, IndexerTransportError>>>,
        addresses: Mutex<Vec<String>>,
    }

    impl MidnightIndexerTransport for ScriptedTransport {
        fn snapshot<'a>(
            &'a self,
            address: &'a str,
        ) -> BoxFuture<'a, Result<IndexerSnapshot, IndexerTransportError>> {
            Box::pin(async move {
                self.addresses
                    .lock()
                    .expect("address lock should be available")
                    .push(address.to_owned());
                self.results
                    .lock()
                    .expect("script lock should be available")
                    .pop_front()
                    .expect("a scripted result should be present")
            })
        }
    }

    fn profile() -> WalletProfileId {
        WalletProfileId::parse("profile_live").expect("profile id is valid")
    }

    fn network() -> ChainNetwork {
        network_by_id(&network_id("undeployed").expect("network is valid"))
            .expect("catalog should be valid")
            .expect("standalone network exists")
    }

    fn address() -> ChainAddress {
        fixture_addresses(network().id()).expect("fixture addresses are valid")[0].clone()
    }

    fn resolve<T>(mut future: BoxFuture<'_, T>) -> T {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("scripted future must resolve immediately"),
        }
    }

    fn raw_utxo(token_type: &str, value: u128, intent: char, output_index: u32) -> IndexerUtxo {
        IndexerUtxo {
            token_type: token_type.to_owned(),
            value,
            intent_hash: intent.to_string().repeat(64),
            output_index,
        }
    }

    fn live_snapshot() -> IndexerSnapshot {
        let custom = "ab".repeat(32);
        IndexerSnapshot {
            current_cursor: 9,
            target_cursor: 9,
            chain_tip_height: Some(77),
            utxos: vec![
                raw_utxo(NATIVE_NIGHT_TOKEN_TYPE, 2_500_000, '1', 0),
                raw_utxo(&custom, u128::MAX - 1, '2', 0),
            ],
            transactions: vec![IndexerTransaction {
                hash: "cd".repeat(32),
                block_height: 77,
                timestamp_millis: 1_700_000_000_000,
                status: IndexerTransactionStatus::PartialSuccess,
                fee_specks: Some(1_500),
                created: vec![raw_utxo(NATIVE_NIGHT_TOKEN_TYPE, 2_500_000, '1', 0)],
                spent: vec![raw_utxo(NATIVE_NIGHT_TOKEN_TYPE, 3_000_000, '0', 0)],
            }],
        }
    }

    #[test]
    fn configuration_rejects_routes_with_credentials_queries_and_wrong_address_networks() {
        let standalone_address = address();
        assert!(
            MidnightIndexerConfig::new(
                "undeployed",
                "wss://indexer.invalid/api/v4/graphql/ws",
                standalone_address.value(),
            )
            .is_ok()
        );
        assert_eq!(
            MidnightIndexerConfig::new(
                "undeployed",
                "wss://user:pass@indexer.invalid/ws",
                standalone_address.value(),
            ),
            Err(MidnightIndexerConfigError::EndpointCredentialsForbidden)
        );
        assert_eq!(
            MidnightIndexerConfig::new(
                "undeployed",
                "wss://indexer.invalid/ws?token=secret",
                standalone_address.value(),
            ),
            Err(MidnightIndexerConfigError::EndpointQueryForbidden)
        );
        assert_eq!(
            MidnightIndexerConfig::new(
                "undeployed",
                "wss://indexer.invalid/ws#fragment",
                standalone_address.value(),
            ),
            Err(MidnightIndexerConfigError::EndpointQueryForbidden)
        );
        assert_eq!(
            MidnightIndexerConfig::new(
                "undeployed",
                " wss://indexer.invalid/ws",
                standalone_address.value(),
            ),
            Err(MidnightIndexerConfigError::InvalidEndpoint)
        );
        assert_eq!(
            MidnightIndexerConfig::new(
                "undeployed",
                "https://indexer.invalid/graphql",
                standalone_address.value(),
            ),
            Err(MidnightIndexerConfigError::InvalidEndpoint)
        );
        assert_eq!(
            MidnightIndexerConfig::new(
                "unsupported",
                "wss://indexer.invalid/ws",
                standalone_address.value(),
            ),
            Err(MidnightIndexerConfigError::InvalidNetwork)
        );
        assert_eq!(
            MidnightIndexerConfig::new(
                "undeployed",
                "wss://indexer.invalid/ws",
                format!(" {}", standalone_address.value()),
            ),
            Err(MidnightIndexerConfigError::InvalidAddress)
        );
        assert_eq!(
            MidnightIndexerConfig::new(
                "mainnet",
                "wss://indexer.invalid/ws",
                standalone_address.value(),
            ),
            Err(MidnightIndexerConfigError::AddressNetworkMismatch)
        );
    }

    #[test]
    fn native_transport_installs_a_tls_crypto_provider() {
        ensure_tls_provider().expect("the pinned ring provider should install");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[test]
    fn progress_first_fold_replays_create_and_spend_events_exactly() {
        let mut fold = SnapshotAccumulator::default();
        fold.apply(IndexerEvent::Progress { target: 4 })
            .expect("progress is valid");
        let spent = raw_utxo(NATIVE_NIGHT_TOKEN_TYPE, 10, 'a', 0);
        fold.apply(IndexerEvent::Transaction {
            cursor: 2,
            transaction: IndexerTransaction {
                hash: "01".repeat(32),
                block_height: 7,
                timestamp_millis: 10,
                status: IndexerTransactionStatus::Success,
                fee_specks: Some(1),
                created: vec![spent.clone()],
                spent: Vec::new(),
            },
        })
        .expect("create is valid");
        assert!(!fold.complete());
        fold.apply(IndexerEvent::Transaction {
            cursor: 4,
            transaction: IndexerTransaction {
                hash: "02".repeat(32),
                block_height: 8,
                timestamp_millis: 11,
                status: IndexerTransactionStatus::Success,
                fee_specks: Some(1),
                created: Vec::new(),
                spent: vec![spent],
            },
        })
        .expect("spend is valid");
        let snapshot = fold.finish().expect("fold is complete");
        assert!(snapshot.utxos.is_empty());
        assert_eq!(snapshot.current_cursor, 4);
        assert_eq!(snapshot.chain_tip_height, Some(8));
        assert_eq!(snapshot.transactions.len(), 2);
    }

    #[test]
    fn empty_progress_is_a_complete_empty_snapshot() {
        let mut fold = SnapshotAccumulator::default();
        fold.apply(IndexerEvent::Progress { target: 0 })
            .expect("progress is valid");
        assert!(fold.complete());
        let snapshot = fold.finish().expect("empty snapshot is complete");
        assert_eq!(snapshot.current_cursor, 0);
        assert!(snapshot.utxos.is_empty());
        assert!(snapshot.chain_tip_height.is_none());
    }

    #[test]
    fn fold_rejects_target_overrun_missing_spends_and_utxo_record_overflow() {
        let mut target_overrun = SnapshotAccumulator::default();
        target_overrun
            .apply(IndexerEvent::Progress { target: 1 })
            .expect("progress is valid");
        assert_eq!(
            target_overrun.apply(IndexerEvent::Transaction {
                cursor: 2,
                transaction: IndexerTransaction {
                    hash: "03".repeat(32),
                    block_height: 1,
                    timestamp_millis: 2,
                    status: IndexerTransactionStatus::Success,
                    fee_specks: Some(0),
                    created: Vec::new(),
                    spent: Vec::new(),
                },
            }),
            Err(IndexerTransportError::Protocol)
        );

        let missing = raw_utxo(NATIVE_NIGHT_TOKEN_TYPE, 1, 'f', 0);
        let mut missing_spend = SnapshotAccumulator::default();
        assert_eq!(
            missing_spend.apply(IndexerEvent::Transaction {
                cursor: 1,
                transaction: IndexerTransaction {
                    hash: "04".repeat(32),
                    block_height: 1,
                    timestamp_millis: 2,
                    status: IndexerTransactionStatus::Success,
                    fee_specks: Some(0),
                    created: Vec::new(),
                    spent: vec![missing],
                },
            }),
            Err(IndexerTransportError::InvalidData)
        );

        let mut too_many_records = SnapshotAccumulator {
            utxo_record_count: MAX_UTXO_RECORDS,
            ..SnapshotAccumulator::default()
        };
        assert_eq!(
            too_many_records.apply(IndexerEvent::Transaction {
                cursor: 1,
                transaction: IndexerTransaction {
                    hash: "05".repeat(32),
                    block_height: 1,
                    timestamp_millis: 2,
                    status: IndexerTransactionStatus::Success,
                    fee_specks: Some(0),
                    created: vec![raw_utxo(NATIVE_NIGHT_TOKEN_TYPE, 1, 'e', 0)],
                    spent: Vec::new(),
                },
            }),
            Err(IndexerTransportError::LimitExceeded)
        );
    }

    #[test]
    fn decoder_rejects_foreign_owner_numeric_amount_and_negative_cursor() {
        let template = |owner: &str, value: Value, transaction_id: i64| {
            json!({
                "unshieldedTransactions": {
                    "__typename": "UnshieldedTransaction",
                    "transaction": {
                        "id": transaction_id,
                        "hash": "ab".repeat(32),
                        "block": { "height": 1, "timestamp": 2 },
                        "__typename": "RegularTransaction",
                        "transactionResult": { "status": "SUCCESS" },
                        "fee": "3"
                    },
                    "createdUtxos": [{
                        "owner": owner,
                        "tokenType": NATIVE_NIGHT_TOKEN_TYPE,
                        "value": value,
                        "intentHash": "cd".repeat(32),
                        "outputIndex": 0
                    }],
                    "spentUtxos": []
                }
            })
        };
        let expected = address();
        assert_eq!(
            decode_event(
                &template("mn_addr_wrong1value", json!("1"), 1),
                expected.value()
            ),
            Err(IndexerTransportError::InvalidData)
        );
        assert_eq!(
            decode_event(&template(expected.value(), json!(1), 1), expected.value()),
            Err(IndexerTransportError::InvalidData)
        );
        assert_eq!(
            decode_event(
                &template(expected.value(), json!("1"), -1),
                expected.value()
            ),
            Err(IndexerTransportError::InvalidData)
        );
    }

    #[test]
    fn live_source_returns_live_then_cached_exact_account_state() {
        let transport = Arc::new(ScriptedTransport {
            results: Mutex::new(VecDeque::from([Ok(live_snapshot())])),
            addresses: Mutex::new(Vec::new()),
        });
        let source = LiveMidnightAccountSource::with_transport(
            network().id().clone(),
            address(),
            Arc::new(FixedClock),
            transport,
        );
        let before = source
            .account(&profile(), &network())
            .expect("configured account is readable");
        assert_eq!(before.source(), WalletAccountSource::Live);
        assert_eq!(before.sync().state(), WalletSyncState::NeverSynced);

        let live = resolve(source.sync(&profile(), &network())).expect("sync succeeds");
        assert_eq!(live.source(), WalletAccountSource::Live);
        assert_eq!(live.sync().state(), WalletSyncState::Synced);
        assert_eq!(live.balances()[0].asset().symbol().as_str(), "NIGHT");
        assert_eq!(live.balances()[0].atomic_units(), 2_500_000);
        assert_eq!(live.balances()[1].atomic_units(), u128::MAX - 1);
        assert_eq!(
            live.transactions()[0].status(),
            WalletTransactionStatus::PartiallyApplied
        );
        assert_eq!(
            live.transactions()[0].direction(),
            WalletTransactionDirection::Outgoing
        );
        assert_eq!(
            live.transactions()[0].fee().map(AssetBalance::atomic_units),
            Some(1_500)
        );

        let cached = source
            .account(&profile(), &network())
            .expect("cached account is readable");
        assert_eq!(cached.source(), WalletAccountSource::Cached);
        assert_eq!(cached.balances(), live.balances());
    }

    #[test]
    fn failed_refresh_preserves_cached_values_and_marks_them_stalled() {
        let transport = Arc::new(ScriptedTransport {
            results: Mutex::new(VecDeque::from([
                Ok(live_snapshot()),
                Err(IndexerTransportError::Connect),
            ])),
            addresses: Mutex::new(Vec::new()),
        });
        let source = LiveMidnightAccountSource::with_transport(
            network().id().clone(),
            address(),
            Arc::new(FixedClock),
            transport,
        );
        let live = resolve(source.sync(&profile(), &network())).expect("initial sync succeeds");
        assert_eq!(
            resolve(source.sync(&profile(), &network())),
            Err(WalletAccountPortError::Unavailable)
        );
        let stalled = source
            .account(&profile(), &network())
            .expect("stalled state is readable");
        assert_eq!(stalled.source(), WalletAccountSource::Cached);
        assert_eq!(stalled.sync().state(), WalletSyncState::Stalled);
        assert_eq!(stalled.balances(), live.balances());
        assert_eq!(stalled.transactions(), live.transactions());
        assert_eq!(stalled.sync().current_cursor(), Some(9));
    }

    #[test]
    fn binding_a_derived_account_resets_cache_and_scopes_the_next_sync() {
        let transport = Arc::new(ScriptedTransport {
            results: Mutex::new(VecDeque::from([Ok(live_snapshot()), Ok(live_snapshot())])),
            addresses: Mutex::new(Vec::new()),
        });
        let configured_address = address();
        let source = LiveMidnightAccountSource::with_transport(
            network().id().clone(),
            configured_address.clone(),
            Arc::new(FixedClock),
            transport.clone(),
        );

        resolve(source.sync(&profile(), &network())).expect("configured watch sync succeeds");
        let derived_address = crate::encode_midnight_address(
            network().id(),
            ChainAddressKind::Unshielded,
            "addr",
            &[0x2a; 32],
        )
        .expect("derived address fixture encodes");
        let derived = DerivedChainAccount::new(
            network().id().clone(),
            ChainAccountId::parse("midnight_account_0_0").expect("account id is valid"),
            0,
            0,
            derived_address.clone(),
            oxid_wallet_domain::WalletKeyReference::parse("key_derived")
                .expect("key reference is valid"),
        )
        .expect("derived account is valid");
        source
            .bind_derived_account(&profile(), &network(), &derived)
            .expect("derived account binds");

        let rebound = source
            .account(&profile(), &network())
            .expect("rebound account is readable");
        assert_eq!(rebound.sync().state(), WalletSyncState::NeverSynced);
        assert_eq!(rebound.account_id(), Some(derived.account_id()));
        assert_eq!(rebound.addresses(), std::slice::from_ref(&derived_address));
        resolve(source.sync(&profile(), &network())).expect("derived account sync succeeds");
        source
            .bind_derived_account(&profile(), &network(), &derived)
            .expect("identical account binding is idempotent");
        let unchanged = source
            .account(&profile(), &network())
            .expect("idempotent binding keeps cached state");
        assert_eq!(unchanged.source(), WalletAccountSource::Cached);
        assert_eq!(unchanged.sync().state(), WalletSyncState::Synced);
        assert_eq!(
            *transport
                .addresses
                .lock()
                .expect("recorded addresses are readable"),
            vec![
                configured_address.value().to_owned(),
                derived_address.value().to_owned(),
            ]
        );
    }
}
