// SPDX-License-Identifier: Apache-2.0

//! Bounded standalone completion of an authorized Midnight transfer.

use std::{
    fmt,
    net::IpAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures::{SinkExt, StreamExt};
use midnight_base_crypto::{schnorr::Signature, time::Timestamp};
use midnight_coin_structure::coin::TokenType;
use midnight_ledger::{
    dust::{DustActions, DustLocalState, DustOutput, DustParameters, DustPublicKey, DustSecretKey},
    events::Event,
    structure::{
        Intent, LedgerParameters, ProofPreimageMarker, ProofPreimageVersioned, ProofVersioned,
        StandardTransaction, Transaction,
    },
};
use midnight_onchain_runtime::cost_model::INITIAL_COST_MODEL;
use midnight_storage::{
    DefaultDB,
    arena::Sp,
    storage::{Array, HashMap as LedgerHashMap},
};
use midnight_transient_crypto::{
    commitment::PedersenRandomness,
    curve::Fr,
    proofs::{Proof, ProofPreimage, ProvingKeyMaterial, ProvingProvider},
};
use oxid_platform_ports::ClockPort;
use oxid_wallet_application::WalletTransactionPortError;
use oxid_wallet_domain::WalletTransferSubmissionMode;
use rand::rngs::OsRng;
use reqwest::{Method, StatusCode, Url, header::CONTENT_TYPE};
use serde_json::{Value, json};
use subxt::{OnlineClient, SubstrateConfig, dynamic};
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};

use crate::{
    MidnightIndexerConfig, MidnightIndexerConfigError, MidnightLocalProvingConfig,
    dust_checkpoint::{
        MidnightDustCheckpointStore, StoredDustCheckpoint, UnavailableMidnightDustCheckpointStore,
    },
    local_proving,
    submission_journal::{StoredSubmissionJournalEntry, StoredSubmissionState},
    transaction::{
        MidnightCompletionOutcome, MidnightCompletionRequest, MidnightSubmissionReconciler,
        MidnightSubmissionReconciliation, MidnightTransactionCompleter,
    },
};

const DUST_QUERY: &str = include_str!("../queries/dust_ledger_events.graphql");
const CHAIN_TIP_QUERY: &str = include_str!("../queries/chain_tip.graphql");
const DUST_BALANCE_SEGMENT: u16 = 0xFEED;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const ACK_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(5);
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const PROOF_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SUBMISSION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const MAX_ENDPOINT_CHARACTERS: usize = 2_048;
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_FRAME_BYTES: usize = 256 * 1024;
const MAX_DUST_EVENTS: usize = 1_000_000;
const MAX_DUST_EVENT_BYTES: usize = 1024 * 1024;
const MAX_DUST_TOTAL_BYTES: usize = 512 * 1024 * 1024;
const DUST_REPLAY_BATCH_EVENTS: usize = 256;
const MAX_DUST_REPLAY_BATCH_BYTES: usize = 4 * 1024 * 1024;
const MAX_CHAIN_TIP_BYTES: usize = 1024 * 1024;
const MAX_PROOF_REQUEST_BYTES: usize = 16 * 1024 * 1024;
const MAX_PROOF_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TRANSACTION_BYTES: usize = 16 * 1024 * 1024;
const MAX_BALANCE_ITERATIONS: usize = 16;
const MAX_RECONCILIATION_BLOCKS: usize = 2_048;

type UnprovenTransaction =
    Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;

/// Validated public routes for the complete standalone transaction path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightStandaloneConfig {
    indexer: MidnightIndexerConfig,
    indexer_http_url: String,
    node_websocket_url: String,
    proving: MidnightProvingMode,
}

/// Selected proof boundary for an explicitly configured standalone wallet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MidnightProvingMode {
    /// Keep proof witnesses on-device and use an authenticated bounded cache.
    Local(MidnightLocalProvingConfig),
    /// Development-only remote proof service using loopback HTTP or HTTPS.
    Remote { proof_server_url: String },
}

impl MidnightStandaloneConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        network_id: impl Into<String>,
        indexer_websocket_url: impl AsRef<str>,
        indexer_http_url: impl AsRef<str>,
        node_websocket_url: impl AsRef<str>,
        proof_server_url: impl AsRef<str>,
        unshielded_address: impl AsRef<str>,
    ) -> Result<Self, MidnightStandaloneConfigError> {
        let indexer =
            MidnightIndexerConfig::new(network_id, indexer_websocket_url, unshielded_address)
                .map_err(MidnightStandaloneConfigError::Indexer)?;
        let indexer_http_url = validate_http_url(indexer_http_url.as_ref(), false)
            .map_err(|_| MidnightStandaloneConfigError::InvalidIndexerHttpEndpoint)?;
        let node_websocket_url =
            super::indexer::validate_websocket_url(node_websocket_url.as_ref())
                .map_err(|_| MidnightStandaloneConfigError::InvalidNodeEndpoint)?;
        let proof_server_url = validate_http_url(proof_server_url.as_ref(), true)
            .map_err(|_| MidnightStandaloneConfigError::InvalidProofEndpoint)?;
        Ok(Self {
            indexer,
            indexer_http_url,
            node_websocket_url,
            proving: MidnightProvingMode::Remote { proof_server_url },
        })
    }

    /// Builds a standalone configuration that keeps proof witnesses on-device.
    pub fn new_private(
        network_id: impl Into<String>,
        indexer_websocket_url: impl AsRef<str>,
        indexer_http_url: impl AsRef<str>,
        node_websocket_url: impl AsRef<str>,
        local_proving: MidnightLocalProvingConfig,
        unshielded_address: impl AsRef<str>,
    ) -> Result<Self, MidnightStandaloneConfigError> {
        let indexer =
            MidnightIndexerConfig::new(network_id, indexer_websocket_url, unshielded_address)
                .map_err(MidnightStandaloneConfigError::Indexer)?;
        let indexer_http_url = validate_http_url(indexer_http_url.as_ref(), false)
            .map_err(|_| MidnightStandaloneConfigError::InvalidIndexerHttpEndpoint)?;
        let node_websocket_url =
            super::indexer::validate_websocket_url(node_websocket_url.as_ref())
                .map_err(|_| MidnightStandaloneConfigError::InvalidNodeEndpoint)?;
        Ok(Self {
            indexer,
            indexer_http_url,
            node_websocket_url,
            proving: MidnightProvingMode::Local(local_proving),
        })
    }

    #[must_use]
    pub const fn indexer(&self) -> &MidnightIndexerConfig {
        &self.indexer
    }

    #[must_use]
    pub fn indexer_http_url(&self) -> &str {
        &self.indexer_http_url
    }

    #[must_use]
    pub fn node_websocket_url(&self) -> &str {
        &self.node_websocket_url
    }

    #[must_use]
    pub const fn proving(&self) -> &MidnightProvingMode {
        &self.proving
    }
}

/// Safe standalone route validation errors. Endpoint values are never rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightStandaloneConfigError {
    Indexer(MidnightIndexerConfigError),
    InvalidIndexerHttpEndpoint,
    InvalidNodeEndpoint,
    InvalidProofEndpoint,
}

impl fmt::Display for MidnightStandaloneConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Indexer(error) => error.fmt(formatter),
            Self::InvalidIndexerHttpEndpoint => {
                formatter.write_str("Midnight indexer HTTP endpoint is invalid")
            }
            Self::InvalidNodeEndpoint => {
                formatter.write_str("Midnight node WebSocket endpoint is invalid")
            }
            Self::InvalidProofEndpoint => formatter.write_str(
                "Midnight proof endpoint must use loopback HTTP or HTTPS without credentials",
            ),
        }
    }
}

impl std::error::Error for MidnightStandaloneConfigError {}

/// Safe failures while binding an authenticated deployment profile to the
/// chain actually exposed by its reviewed node route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightChainIdentityError {
    InvalidNodeEndpoint,
    NodeUnavailable,
    GenesisMismatch,
}

impl fmt::Display for MidnightChainIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidNodeEndpoint => "Midnight node endpoint is invalid",
            Self::NodeUnavailable => "Midnight node chain identity is unavailable",
            Self::GenesisMismatch => {
                "Midnight node chain identity does not match the deployment profile"
            }
        })
    }
}

impl std::error::Error for MidnightChainIdentityError {}

/// Checks the node genesis hash before an authenticated production profile can
/// be composed. Endpoint values and observed identifiers are never logged or
/// returned in failures.
pub async fn authenticate_midnight_chain_identity(
    node_websocket_url: &str,
    expected_genesis_hash: &[u8; 32],
) -> Result<(), MidnightChainIdentityError> {
    super::indexer::validate_websocket_url(node_websocket_url)
        .map_err(|_| MidnightChainIdentityError::InvalidNodeEndpoint)?;
    let client = timeout(
        CONNECT_TIMEOUT,
        OnlineClient::<SubstrateConfig>::from_insecure_url(node_websocket_url),
    )
    .await
    .map_err(|_| MidnightChainIdentityError::NodeUnavailable)?
    .map_err(|_| MidnightChainIdentityError::NodeUnavailable)?;
    let genesis_hash = client.genesis_hash();
    let observed: &[u8] = genesis_hash.as_ref();
    if observed == expected_genesis_hash {
        Ok(())
    } else {
        Err(MidnightChainIdentityError::GenesisMismatch)
    }
}

#[derive(Clone)]
pub(crate) struct LiveMidnightTransactionCompleter<C> {
    config: MidnightStandaloneConfig,
    local_proving_gate: Arc<Mutex<()>>,
    dust_checkpoints: Arc<dyn MidnightDustCheckpointStore>,
    clock: Arc<C>,
}

impl<C> LiveMidnightTransactionCompleter<C> {
    pub(crate) fn new(config: MidnightStandaloneConfig, clock: Arc<C>) -> Self {
        Self {
            config,
            local_proving_gate: Arc::new(Mutex::new(())),
            dust_checkpoints: Arc::new(UnavailableMidnightDustCheckpointStore),
            clock,
        }
    }

    pub(crate) fn new_with_dust_store(
        config: MidnightStandaloneConfig,
        dust_checkpoints: Arc<dyn MidnightDustCheckpointStore>,
        clock: Arc<C>,
    ) -> Self {
        Self {
            config,
            local_proving_gate: Arc::new(Mutex::new(())),
            dust_checkpoints,
            clock,
        }
    }
}

#[derive(Clone)]
pub(crate) struct LiveMidnightSubmissionReconciler {
    config: MidnightStandaloneConfig,
}

impl LiveMidnightSubmissionReconciler {
    pub(crate) const fn new(config: MidnightStandaloneConfig) -> Self {
        Self { config }
    }
}

impl MidnightSubmissionReconciler for LiveMidnightSubmissionReconciler {
    fn reconcile(
        &self,
        entry: &StoredSubmissionJournalEntry,
    ) -> Result<MidnightSubmissionReconciliation, WalletTransactionPortError> {
        if entry.mode != WalletTransferSubmissionMode::Live
            || !matches!(
                entry.state,
                StoredSubmissionState::Broadcasting | StoredSubmissionState::OutcomeUnknown
            )
            || &entry.network_id != self.config.indexer().network_id()
        {
            return Err(WalletTransactionPortError::InvalidData);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        runtime.block_on(reconcile_live_submission(&self.config, entry))
    }
}

async fn reconcile_live_submission(
    config: &MidnightStandaloneConfig,
    entry: &StoredSubmissionJournalEntry,
) -> Result<MidnightSubmissionReconciliation, WalletTransactionPortError> {
    let client = timeout(
        CONNECT_TIMEOUT,
        OnlineClient::<SubstrateConfig>::from_insecure_url(config.node_websocket_url()),
    )
    .await
    .map_err(|_| WalletTransactionPortError::Timeout)?
    .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let mut block = timeout(CONNECT_TIMEOUT, client.blocks().at_latest())
        .await
        .map_err(|_| WalletTransactionPortError::Timeout)?
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let mut reached_anchor = false;
    for _ in 0..MAX_RECONCILIATION_BLOCKS {
        if block.hash().0 == entry.anchor_block_hash {
            reached_anchor = true;
            break;
        }
        let extrinsics = timeout(CONNECT_TIMEOUT, block.extrinsics())
            .await
            .map_err(|_| WalletTransactionPortError::Timeout)?
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        for extrinsic in extrinsics.iter() {
            if extrinsic.hash().0 != entry.transaction_hash {
                continue;
            }
            let events = timeout(CONNECT_TIMEOUT, extrinsic.events())
                .await
                .map_err(|_| WalletTransactionPortError::Timeout)?
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            let mut succeeded = false;
            let mut failed = false;
            for event in events.iter() {
                let event = event.map_err(|_| WalletTransactionPortError::InvalidChainState)?;
                if event.pallet_name() == "System" && event.variant_name() == "ExtrinsicSuccess" {
                    succeeded = true;
                }
                if event.pallet_name() == "System" && event.variant_name() == "ExtrinsicFailed" {
                    failed = true;
                }
            }
            return match (succeeded, failed) {
                (true, false) => Ok(MidnightSubmissionReconciliation::Included {
                    block_hash: block.hash().0,
                    block_height: u64::from(block.header().number),
                }),
                (false, true) => Ok(MidnightSubmissionReconciliation::Rejected),
                _ => Err(WalletTransactionPortError::InvalidChainState),
            };
        }
        let parent_hash = block.header().parent_hash;
        block = timeout(CONNECT_TIMEOUT, client.blocks().at(parent_hash))
            .await
            .map_err(|_| WalletTransactionPortError::Timeout)?
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
    }
    if block.hash().0 == entry.anchor_block_hash {
        reached_anchor = true;
    }
    if !reached_anchor {
        return Ok(MidnightSubmissionReconciliation::Unresolved);
    }
    let chain_tip = fetch_chain_tip(config.indexer_http_url()).await?;
    if Timestamp::from_secs(entry.expires_at.value() / 1_000) <= chain_tip.timestamp {
        Ok(MidnightSubmissionReconciliation::Expired)
    } else {
        Ok(MidnightSubmissionReconciliation::Unresolved)
    }
}

impl<C> MidnightTransactionCompleter for LiveMidnightTransactionCompleter<C>
where
    C: ClockPort + 'static,
{
    fn complete(
        &self,
        request: MidnightCompletionRequest,
        dust_seed: &[u8; 32],
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
        let _local_proving_permit =
            if matches!(self.config.proving(), MidnightProvingMode::Local(_)) {
                Some(
                    self.local_proving_gate
                        .lock()
                        .map_err(|_| WalletTransactionPortError::Unavailable)?,
                )
            } else {
                None
            };
        let dust_key = DustSecretKey::derive_secret_key(dust_seed);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        runtime.block_on(complete_live(
            &self.config,
            request,
            &dust_key,
            self.dust_checkpoints.as_ref(),
            self.clock.as_ref(),
        ))
    }
}

async fn complete_live<C>(
    config: &MidnightStandaloneConfig,
    request: MidnightCompletionRequest,
    dust_key: &DustSecretKey,
    checkpoints: &dyn MidnightDustCheckpointStore,
    clock: &C,
) -> Result<MidnightCompletionOutcome, WalletTransactionPortError>
where
    C: ClockPort,
{
    let cancellation = request.cancellation_token();
    ensure_submission_active(&cancellation)?;
    let chain_tip = fetch_chain_tip(config.indexer_http_url()).await?;
    ensure_submission_active(&cancellation)?;
    let dust_public_key = DustPublicKey::from(dust_key.clone());
    let checkpoint = checkpoints
        .load(
            config.indexer.network_id(),
            &dust_public_key,
            chain_tip.parameters.dust,
        )
        .ok()
        .flatten();
    let mut persist_progress = |progress: &DustSyncProgress| {
        if let Ok(updated_at) = clock.now() {
            let _ = checkpoints.save(
                config.indexer.network_id(),
                &dust_public_key,
                &StoredDustCheckpoint {
                    current_cursor: progress.current_cursor,
                    target_cursor: progress.target_cursor,
                    updated_at,
                    state: progress.state.clone(),
                },
            );
        }
        Ok(())
    };
    let synchronized = synchronize_dust_with_control(
        config.indexer.websocket_url(),
        dust_key,
        chain_tip.parameters.dust,
        checkpoint,
        &cancellation,
        &mut persist_progress,
    )
    .await?;
    let mut dust_state = synchronized.state;
    ensure_submission_active(&cancellation)?;
    if dust_state.params != chain_tip.parameters.dust {
        return Err(WalletTransactionPortError::InvalidChainState);
    }
    if let Ok(updated_at) = clock.now() {
        let _ = checkpoints.save(
            config.indexer.network_id(),
            &dust_public_key,
            &StoredDustCheckpoint {
                current_cursor: synchronized.current_cursor,
                target_cursor: synchronized.target_cursor,
                updated_at,
                state: dust_state.clone(),
            },
        );
    }
    let current_time = if chain_tip.timestamp > dust_state.sync_time {
        chain_tip.timestamp
    } else {
        dust_state.sync_time
    };
    let ttl = Timestamp::from_secs(request.expires_at_seconds);
    if ttl <= current_time {
        return Err(WalletTransactionPortError::DraftExpired);
    }
    let (balanced, fee_specks) = balance_dust(
        request.transaction.clone(),
        &mut dust_state,
        dust_key,
        &chain_tip.parameters,
        current_time,
        ttl,
        config.indexer.network_id().as_str(),
    )?;
    let sealed = match config.proving() {
        MidnightProvingMode::Local(local_config) => {
            let outcome =
                local_proving::prove_transaction(balanced, local_config, &cancellation).await?;
            let _metrics = outcome.metrics;
            outcome.transaction
        }
        MidnightProvingMode::Remote { proof_server_url } => {
            prove_via_http(balanced, proof_server_url).await?
        }
    };
    ensure_submission_active(&cancellation)?;
    let sealed_fee = sealed
        .fees(&chain_tip.parameters, false)
        .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    if sealed_fee != fee_specks {
        return Err(WalletTransactionPortError::InvalidChainState);
    }
    let mut transaction_bytes = Vec::new();
    midnight_serialize::tagged_serialize(&sealed, &mut transaction_bytes)
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
    if transaction_bytes.len() > MAX_TRANSACTION_BYTES {
        return Err(WalletTransactionPortError::InvalidData);
    }
    ensure_submission_active(&cancellation)?;
    let (transaction_hash, block_hash, block_height) = submit_unsigned(
        config.node_websocket_url(),
        transaction_bytes,
        &request,
        fee_specks,
    )
    .await?;
    Ok(MidnightCompletionOutcome {
        fee_specks,
        transaction_hash,
        block_hash,
        block_height,
        mode: WalletTransferSubmissionMode::Live,
    })
}

pub(crate) fn ensure_submission_active(
    cancellation: &AtomicBool,
) -> Result<(), WalletTransactionPortError> {
    if cancellation.load(Ordering::Acquire) {
        Err(WalletTransactionPortError::SubmissionCancelled)
    } else {
        Ok(())
    }
}

pub(crate) struct ChainTip {
    pub(crate) timestamp: Timestamp,
    pub(crate) parameters: LedgerParameters,
}

pub(crate) async fn fetch_chain_tip(
    endpoint: &str,
) -> Result<ChainTip, WalletTransactionPortError> {
    ensure_tls_provider()?;
    let client = chain_tip_client()?;
    let request = chain_tip_request(endpoint)?;
    let response = client
        .execute(request)
        .await
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    validate_chain_tip_status(response.status())?;
    let body = bounded_response(response, MAX_CHAIN_TIP_BYTES)
        .await
        .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    decode_chain_tip_body(&body)
}

fn chain_tip_request(endpoint: &str) -> Result<reqwest::Request, WalletTransactionPortError> {
    let endpoint = Url::parse(endpoint).map_err(|_| WalletTransactionPortError::Unavailable)?;
    let body = serde_json::to_vec(&json!({ "query": CHAIN_TIP_QUERY, "variables": {} }))
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
    let mut request = reqwest::Request::new(Method::POST, endpoint);
    request.headers_mut().insert(
        CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    *request.body_mut() = Some(reqwest::Body::from(body));
    Ok(request)
}

fn chain_tip_client() -> Result<reqwest::Client, WalletTransactionPortError> {
    reqwest::Client::builder()
        // Standalone wallet routes are explicit trust-boundary configuration. Do not let
        // ambient proxy variables silently redirect them.
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|_| WalletTransactionPortError::Unavailable)
}

fn validate_chain_tip_status(status: StatusCode) -> Result<(), WalletTransactionPortError> {
    status
        .is_success()
        .then_some(())
        .ok_or(WalletTransactionPortError::InvalidChainState)
}

fn decode_chain_tip_body(body: &[u8]) -> Result<ChainTip, WalletTransactionPortError> {
    if body.len() > MAX_CHAIN_TIP_BYTES {
        return Err(WalletTransactionPortError::InvalidChainState);
    }
    let root: Value =
        serde_json::from_slice(body).map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    decode_chain_tip(&root)
}

fn decode_chain_tip(root: &Value) -> Result<ChainTip, WalletTransactionPortError> {
    if root
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err(WalletTransactionPortError::InvalidChainState);
    }
    let block = root
        .pointer("/data/block")
        .and_then(Value::as_object)
        .ok_or(WalletTransactionPortError::InvalidChainState)?;
    let timestamp_millis = block
        .get("timestamp")
        .and_then(Value::as_i64)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(WalletTransactionPortError::InvalidChainState)?;
    let parameters_hex = block
        .get("ledgerParameters")
        .and_then(Value::as_str)
        .ok_or(WalletTransactionPortError::InvalidChainState)?;
    let parameters_bytes = decode_bounded_hex(parameters_hex, MAX_CHAIN_TIP_BYTES)?;
    let parameters = midnight_serialize::tagged_deserialize(&parameters_bytes[..])
        .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    Ok(ChainTip {
        // Midnight indexer v4 exposes its DateTime scalar as Unix
        // milliseconds. The ledger Timestamp is second-granular.
        timestamp: Timestamp::from_secs(timestamp_millis / 1_000),
        parameters,
    })
}

#[cfg(test)]
async fn synchronize_dust(
    endpoint: &str,
    dust_key: &DustSecretKey,
    parameters: DustParameters,
    checkpoint: Option<StoredDustCheckpoint>,
) -> Result<DustSynchronization, WalletTransactionPortError> {
    let cancellation = AtomicBool::new(false);
    let mut ignore_progress = |_: &DustSyncProgress| Ok(());
    synchronize_dust_controlled(
        endpoint,
        dust_key,
        parameters,
        checkpoint,
        &cancellation,
        &mut ignore_progress,
    )
    .await
}

pub(crate) async fn synchronize_dust_controlled(
    endpoint: &str,
    dust_key: &DustSecretKey,
    parameters: DustParameters,
    checkpoint: Option<StoredDustCheckpoint>,
    cancellation: &AtomicBool,
    observe: &mut dyn FnMut(&DustSyncProgress) -> Result<(), WalletTransactionPortError>,
) -> Result<DustSynchronization, WalletTransactionPortError> {
    ensure_submission_active(cancellation)?;
    let started_with_checkpoint = checkpoint.is_some();
    let (mut state, starting_cursor, starting_target) = checkpoint.map_or_else(
        || (DustLocalState::new(parameters), None, None),
        |checkpoint| {
            (
                checkpoint.state,
                Some(checkpoint.current_cursor),
                Some(checkpoint.target_cursor),
            )
        },
    );
    if state.params != parameters {
        return Err(WalletTransactionPortError::InvalidChainState);
    }
    let starting_id = match starting_cursor {
        Some(cursor) => cursor
            .checked_add(1)
            .ok_or(WalletTransactionPortError::InvalidChainState)?,
        None => 0,
    };
    let starting_id =
        i64::try_from(starting_id).map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    ensure_tls_provider()?;
    let mut request = endpoint
        .into_client_request()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        "graphql-transport-ws"
            .parse()
            .map_err(|_| WalletTransactionPortError::InvalidChainState)?,
    );
    let mut websocket_config = WebSocketConfig::default();
    websocket_config.max_message_size = Some(MAX_MESSAGE_BYTES);
    websocket_config.max_frame_size = Some(MAX_FRAME_BYTES);
    let (mut socket, response) = timeout(
        CONNECT_TIMEOUT,
        connect_async_with_config(request, Some(websocket_config), false),
    )
    .await
    .map_err(|_| WalletTransactionPortError::Timeout)?
    .map_err(|_| WalletTransactionPortError::Unavailable)?;
    if response
        .headers()
        .get("Sec-WebSocket-Protocol")
        .and_then(|value| value.to_str().ok())
        != Some("graphql-transport-ws")
    {
        return Err(WalletTransactionPortError::InvalidChainState);
    }
    send_websocket_json(
        &mut socket,
        json!({ "type": "connection_init", "payload": {} }),
    )
    .await?;
    wait_for_ack(&mut socket).await?;
    send_websocket_json(
        &mut socket,
        json!({
            "type": "subscribe",
            "id": "oxid-dust",
            "payload": { "query": DUST_QUERY, "variables": { "id": starting_id } }
        }),
    )
    .await?;

    let synchronization = timeout(SNAPSHOT_TIMEOUT, async {
        let mut batch = Vec::<Event<DefaultDB>>::with_capacity(DUST_REPLAY_BATCH_EVENTS);
        let mut last_id = starting_cursor;
        let mut target_id = starting_target;
        let mut total_bytes = 0_usize;
        let mut batch_bytes = 0_usize;
        let mut event_count = 0_usize;
        let mut replayed_events = 0_usize;
        let mut batch_last_id = None;
        let mut saw_event = false;
        loop {
            ensure_submission_active(cancellation)?;
            let message = match timeout(IDLE_TIMEOUT, socket.next()).await {
                Ok(Some(message)) => {
                    message.map_err(|_| WalletTransactionPortError::InvalidChainState)?
                }
                Err(_) if started_with_checkpoint && !saw_event => break,
                Ok(None) => return Err(WalletTransactionPortError::InvalidChainState),
                Err(_) => return Err(WalletTransactionPortError::Timeout),
            };
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(text.as_str())
                        .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
                    match websocket_message_type(&value)? {
                        "next" => {
                            if value.get("id").and_then(Value::as_str) != Some("oxid-dust") {
                                return Err(WalletTransactionPortError::InvalidChainState);
                            }
                            let data = value
                                .pointer("/payload/data/dustLedgerEvents")
                                .ok_or(WalletTransactionPortError::InvalidChainState)?;
                            let decoded = decode_dust_event(data)?;
                            let sequence_valid = match last_id {
                                // DUST IDs are sparse global indexer cursors:
                                // unrelated ledger activity can create gaps.
                                // They must still move strictly forward.
                                Some(last) => decoded.id > last,
                                None => true,
                            };
                            if !sequence_valid
                                || decoded.id > decoded.max_id
                                || target_id.is_some_and(|target| decoded.max_id < target)
                            {
                                return Err(WalletTransactionPortError::InvalidChainState);
                            }
                            saw_event = true;
                            target_id = Some(decoded.max_id);
                            last_id = Some(decoded.id);
                            event_count = event_count
                                .checked_add(1)
                                .ok_or(WalletTransactionPortError::InvalidChainState)?;
                            total_bytes = total_bytes
                                .checked_add(decoded.raw_bytes)
                                .ok_or(WalletTransactionPortError::InvalidChainState)?;
                            if total_bytes > MAX_DUST_TOTAL_BYTES || event_count > MAX_DUST_EVENTS {
                                return Err(WalletTransactionPortError::InvalidChainState);
                            }
                            if !batch.is_empty()
                                && batch_bytes
                                    .checked_add(decoded.raw_bytes)
                                    .is_none_or(|bytes| bytes > MAX_DUST_REPLAY_BATCH_BYTES)
                            {
                                ensure_submission_active(cancellation)?;
                                state = state
                                    .replay_events(dust_key, batch.iter())
                                    .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
                                replayed_events = replayed_events
                                    .checked_add(batch.len())
                                    .ok_or(WalletTransactionPortError::InvalidChainState)?;
                                batch.clear();
                                batch_bytes = 0;
                                let current_cursor = batch_last_id
                                    .ok_or(WalletTransactionPortError::InvalidChainState)?;
                                let target_cursor = target_id
                                    .ok_or(WalletTransactionPortError::InvalidChainState)?;
                                observe(&DustSyncProgress {
                                    state: state.clone(),
                                    current_cursor,
                                    target_cursor,
                                    events_processed: replayed_events,
                                })?;
                            }
                            batch_bytes = batch_bytes
                                .checked_add(decoded.raw_bytes)
                                .ok_or(WalletTransactionPortError::InvalidChainState)?;
                            batch.push(decoded.event);
                            batch_last_id = Some(decoded.id);
                            if batch.len() == DUST_REPLAY_BATCH_EVENTS
                                || decoded.id == decoded.max_id
                            {
                                ensure_submission_active(cancellation)?;
                                state = state
                                    .replay_events(dust_key, batch.iter())
                                    .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
                                replayed_events = replayed_events
                                    .checked_add(batch.len())
                                    .ok_or(WalletTransactionPortError::InvalidChainState)?;
                                batch.clear();
                                batch_bytes = 0;
                                let current_cursor = batch_last_id
                                    .ok_or(WalletTransactionPortError::InvalidChainState)?;
                                let target_cursor = target_id
                                    .ok_or(WalletTransactionPortError::InvalidChainState)?;
                                observe(&DustSyncProgress {
                                    state: state.clone(),
                                    current_cursor,
                                    target_cursor,
                                    events_processed: replayed_events,
                                })?;
                            }
                            if decoded.id == decoded.max_id {
                                break;
                            }
                        }
                        "ping" => {
                            send_websocket_json(
                                &mut socket,
                                json!({ "type": "pong", "payload": value.get("payload") }),
                            )
                            .await?;
                        }
                        "pong" => {}
                        "complete"
                            if value.get("id").and_then(Value::as_str) == Some("oxid-dust")
                                && started_with_checkpoint
                                && !saw_event =>
                        {
                            break;
                        }
                        _ => return Err(WalletTransactionPortError::InvalidChainState),
                    }
                }
                Message::Ping(payload) => socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|_| WalletTransactionPortError::Unavailable)?,
                Message::Pong(_) => {}
                _ => return Err(WalletTransactionPortError::InvalidChainState),
            }
        }
        if !batch.is_empty() {
            ensure_submission_active(cancellation)?;
            state = state
                .replay_events(dust_key, batch.iter())
                .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
            replayed_events = replayed_events
                .checked_add(batch.len())
                .ok_or(WalletTransactionPortError::InvalidChainState)?;
            let current_cursor =
                batch_last_id.ok_or(WalletTransactionPortError::InvalidChainState)?;
            let target_cursor = target_id.ok_or(WalletTransactionPortError::InvalidChainState)?;
            observe(&DustSyncProgress {
                state: state.clone(),
                current_cursor,
                target_cursor,
                events_processed: replayed_events,
            })?;
        }
        let current_cursor = last_id.ok_or(WalletTransactionPortError::InvalidChainState)?;
        let target_cursor = target_id.ok_or(WalletTransactionPortError::InvalidChainState)?;
        if current_cursor != target_cursor {
            return Err(WalletTransactionPortError::InvalidChainState);
        }
        if !saw_event {
            observe(&DustSyncProgress {
                state: state.clone(),
                current_cursor,
                target_cursor,
                events_processed: 0,
            })?;
        }
        Ok::<_, WalletTransactionPortError>(DustSynchronization {
            state,
            current_cursor,
            target_cursor,
            events_processed: replayed_events,
        })
    })
    .await
    .map_err(|_| WalletTransactionPortError::Timeout)??;

    let _ = send_websocket_json(
        &mut socket,
        json!({ "type": "complete", "id": "oxid-dust" }),
    )
    .await;
    let _ = socket.close(None).await;
    Ok(synchronization)
}

#[cfg(test)]
async fn synchronize_dust_with_fallback(
    endpoint: &str,
    dust_key: &DustSecretKey,
    parameters: DustParameters,
    checkpoint: Option<StoredDustCheckpoint>,
) -> Result<DustSynchronization, WalletTransactionPortError> {
    let cancellation = AtomicBool::new(false);
    let mut ignore_progress = |_: &DustSyncProgress| Ok(());
    synchronize_dust_with_control(
        endpoint,
        dust_key,
        parameters,
        checkpoint,
        &cancellation,
        &mut ignore_progress,
    )
    .await
}

pub(crate) async fn synchronize_dust_with_control(
    endpoint: &str,
    dust_key: &DustSecretKey,
    parameters: DustParameters,
    checkpoint: Option<StoredDustCheckpoint>,
    cancellation: &AtomicBool,
    observe: &mut dyn FnMut(&DustSyncProgress) -> Result<(), WalletTransactionPortError>,
) -> Result<DustSynchronization, WalletTransactionPortError> {
    let had_checkpoint = checkpoint.is_some();
    let mut emitted_progress = false;
    let result = {
        let mut tracking_observer = |progress: &DustSyncProgress| {
            emitted_progress = true;
            observe(progress)
        };
        synchronize_dust_controlled(
            endpoint,
            dust_key,
            parameters,
            checkpoint,
            cancellation,
            &mut tracking_observer,
        )
        .await
    };
    match result {
        Err(WalletTransactionPortError::InvalidChainState)
            if had_checkpoint && !emitted_progress =>
        {
            synchronize_dust_controlled(endpoint, dust_key, parameters, None, cancellation, observe)
                .await
        }
        result => result,
    }
}

pub(crate) struct DustSynchronization {
    pub(crate) state: DustLocalState<DefaultDB>,
    pub(crate) current_cursor: u64,
    pub(crate) target_cursor: u64,
    pub(crate) events_processed: usize,
}

pub(crate) struct DustSyncProgress {
    pub(crate) state: DustLocalState<DefaultDB>,
    pub(crate) current_cursor: u64,
    pub(crate) target_cursor: u64,
    pub(crate) events_processed: usize,
}

struct DecodedDustEvent {
    id: u64,
    max_id: u64,
    raw_bytes: usize,
    event: Event<DefaultDB>,
}

fn decode_dust_event(value: &Value) -> Result<DecodedDustEvent, WalletTransactionPortError> {
    let id = value
        .get("id")
        .and_then(Value::as_i64)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(WalletTransactionPortError::InvalidChainState)?;
    let max_id = value
        .get("maxId")
        .and_then(Value::as_i64)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(WalletTransactionPortError::InvalidChainState)?;
    let raw = value
        .get("raw")
        .and_then(Value::as_str)
        .ok_or(WalletTransactionPortError::InvalidChainState)?;
    let bytes = decode_bounded_hex(raw, MAX_DUST_EVENT_BYTES)?;
    let raw_bytes = bytes.len();
    let event = midnight_serialize::tagged_deserialize(&bytes[..])
        .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    Ok(DecodedDustEvent {
        id,
        max_id,
        raw_bytes,
        event,
    })
}

#[allow(clippy::too_many_arguments)]
fn balance_dust(
    transaction: UnprovenTransaction,
    dust_state: &mut DustLocalState<DefaultDB>,
    dust_key: &DustSecretKey,
    parameters: &LedgerParameters,
    current_time: Timestamp,
    ttl: Timestamp,
    network_id: &str,
) -> Result<(UnprovenTransaction, u128), WalletTransactionPortError> {
    let original_transaction = transaction.clone();
    let original_dust = dust_state.clone();
    let mut current = transaction;
    let mut accumulated_dust = 0_u128;
    for _ in 0..MAX_BALANCE_ITERATIONS {
        let fees = current
            .fees(parameters, false)
            .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
        let balance = current
            .balance(Some(fees))
            .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
        let shortfall = match balance.get(&(TokenType::Dust, 0)).copied() {
            Some(value) if value < 0 => value
                .checked_neg()
                .and_then(|value| u128::try_from(value).ok())
                .ok_or(WalletTransactionPortError::InvalidChainState)?,
            _ => 0,
        };
        if shortfall == 0 {
            return Ok((current, fees));
        }
        accumulated_dust = accumulated_dust
            .checked_add(shortfall)
            .ok_or(WalletTransactionPortError::InvalidChainState)?;
        *dust_state = original_dust.clone();
        let mut remaining = accumulated_dust;
        let mut spends = Array::new();
        let outputs = dust_state.utxos().collect::<Vec<_>>();
        for output in outputs {
            if remaining == 0 {
                break;
            }
            let generation = dust_state
                .generation_info(&output)
                .ok_or(WalletTransactionPortError::InvalidChainState)?;
            let value =
                DustOutput::from(output).updated_value(&generation, current_time, &parameters.dust);
            if value == 0 {
                continue;
            }
            let spend_value = value.min(remaining);
            let (next_state, spend) = dust_state
                .clone()
                .spend(dust_key, &output, spend_value, current_time)
                .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
            *dust_state = next_state;
            spends = spends.push(spend);
            remaining = remaining.saturating_sub(spend_value);
        }
        if remaining > 0 {
            return Err(WalletTransactionPortError::InsufficientDust);
        }
        let mut intent = Intent::empty(&mut OsRng, ttl);
        intent.dust_actions = Some(Sp::new(DustActions {
            spends,
            registrations: Array::new(),
            ctime: current_time,
        }));
        let mut intents = LedgerHashMap::new();
        intents = intents.insert(DUST_BALANCE_SEGMENT, intent);
        let dust_transaction = Transaction::Standard(StandardTransaction::new(
            network_id,
            intents,
            None,
            LedgerHashMap::new(),
        ));
        current = original_transaction
            .merge(&dust_transaction)
            .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    }
    Err(WalletTransactionPortError::InvalidChainState)
}

#[derive(Clone)]
struct HttpDustProvingProvider {
    client: reqwest::Client,
    endpoint: String,
}

impl ProvingProvider for HttpDustProvingProvider {
    async fn check(&self, _: &ProofPreimage) -> Result<Vec<Option<usize>>, anyhow::Error> {
        Err(anyhow::anyhow!(
            "standalone DUST prover does not support contract proof checks"
        ))
    }

    async fn prove(
        self,
        preimage: &ProofPreimage,
        overwrite_binding_input: Option<Fr>,
    ) -> Result<Proof, anyhow::Error> {
        let payload = (
            ProofPreimageVersioned::V2(Arc::new(preimage.clone())),
            Option::<ProvingKeyMaterial>::None,
            overwrite_binding_input,
        );
        let mut body = Vec::new();
        midnight_serialize::tagged_serialize(&payload, &mut body)?;
        if body.len() > MAX_PROOF_REQUEST_BYTES {
            return Err(anyhow::anyhow!(
                "proof request exceeds the configured limit"
            ));
        }
        let response = self
            .client
            .post(format!("{}/prove", self.endpoint.trim_end_matches('/')))
            .body(body)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("proof server rejected the request"));
        }
        let body = bounded_response(response, MAX_PROOF_RESPONSE_BYTES)
            .await
            .map_err(|_| anyhow::anyhow!("proof response exceeds the configured limit"))?;
        let proof: ProofVersioned = midnight_serialize::tagged_deserialize(&body[..])?;
        match proof {
            ProofVersioned::V2(proof) => Ok(proof),
            _ => Err(anyhow::anyhow!(
                "proof server returned an unsupported proof"
            )),
        }
    }

    fn split(&mut self) -> Self {
        self.clone()
    }
}

async fn prove_via_http(
    transaction: UnprovenTransaction,
    endpoint: &str,
) -> Result<
    Transaction<
        Signature,
        midnight_ledger::structure::ProofMarker,
        midnight_transient_crypto::commitment::PureGeneratorPedersen,
        DefaultDB,
    >,
    WalletTransactionPortError,
> {
    ensure_tls_provider()?;
    let client = reqwest::Client::builder()
        // Keep proof material on the explicitly configured route rather than an ambient
        // process proxy. This also preserves loopback proving inside pure Nix builds.
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(PROOF_TIMEOUT)
        .build()
        .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    let provider = HttpDustProvingProvider {
        client,
        endpoint: endpoint.to_owned(),
    };
    let proved = timeout(
        PROOF_TIMEOUT,
        transaction.prove(provider, &INITIAL_COST_MODEL),
    )
    .await
    .map_err(|_| WalletTransactionPortError::Timeout)?
    .map_err(|_| WalletTransactionPortError::ProvingFailed)?;
    Ok(proved.seal(OsRng))
}

async fn submit_unsigned(
    endpoint: &str,
    transaction: Vec<u8>,
    request: &MidnightCompletionRequest,
    fee_specks: u128,
) -> Result<([u8; 32], [u8; 32], u64), WalletTransactionPortError> {
    let client = timeout(
        CONNECT_TIMEOUT,
        OnlineClient::<SubstrateConfig>::from_insecure_url(endpoint),
    )
    .await
    .map_err(|_| WalletTransactionPortError::Timeout)?
    .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let call = dynamic::tx(
        "Midnight",
        "send_mn_transaction",
        vec![dynamic::Value::from_bytes(transaction)],
    );
    let unsigned = client
        .tx()
        .create_unsigned(&call)
        .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    let anchor = timeout(CONNECT_TIMEOUT, client.blocks().at_latest())
        .await
        .map_err(|_| WalletTransactionPortError::Timeout)?
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let transaction_hash = unsigned.hash().0;
    request.begin_broadcast(
        fee_specks,
        transaction_hash,
        anchor.hash().0,
        WalletTransferSubmissionMode::Live,
    )?;
    let mut progress = timeout(SUBMISSION_TIMEOUT, unsigned.submit_and_watch())
        .await
        .map_err(|_| WalletTransactionPortError::SubmissionOutcomeUnknown)?
        .map_err(|_| WalletTransactionPortError::SubmissionOutcomeUnknown)?;
    timeout(SUBMISSION_TIMEOUT, async {
        use subxt::tx::TxStatus;
        loop {
            let status = progress
                .next()
                .await
                .ok_or(WalletTransactionPortError::SubmissionOutcomeUnknown)?
                .map_err(|_| WalletTransactionPortError::SubmissionOutcomeUnknown)?;
            match status {
                TxStatus::InFinalizedBlock(in_block) => {
                    if in_block.extrinsic_hash().0 != transaction_hash {
                        return Err(WalletTransactionPortError::InvalidChainState);
                    }
                    let events = in_block
                        .fetch_events()
                        .await
                        .map_err(|_| WalletTransactionPortError::SubmissionOutcomeUnknown)?;
                    let mut succeeded = false;
                    let mut failed = false;
                    for event in events.iter() {
                        let event = event
                            .map_err(|_| WalletTransactionPortError::SubmissionOutcomeUnknown)?;
                        if event.pallet_name() == "System"
                            && event.variant_name() == "ExtrinsicSuccess"
                        {
                            succeeded = true;
                        }
                        if event.pallet_name() == "System"
                            && event.variant_name() == "ExtrinsicFailed"
                        {
                            failed = true;
                        }
                    }
                    return match (succeeded, failed) {
                        (true, false) => {
                            let finalized = client
                                .blocks()
                                .at(in_block.block_hash())
                                .await
                                .map_err(|_| {
                                    WalletTransactionPortError::SubmissionOutcomeUnknown
                                })?;
                            Ok((
                                transaction_hash,
                                in_block.block_hash().0,
                                u64::from(finalized.header().number),
                            ))
                        }
                        (false, true) => Err(WalletTransactionPortError::SubmissionRejected),
                        _ => Err(WalletTransactionPortError::SubmissionOutcomeUnknown),
                    };
                }
                TxStatus::InBestBlock(_) => {}
                TxStatus::Error { .. } | TxStatus::Invalid { .. } | TxStatus::Dropped { .. } => {
                    // Subxt explicitly documents these stream-terminal states as
                    // probabilistic: the transaction can still reach a block.
                    // Only a finalized extrinsic failure is safe to replace.
                    return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| WalletTransactionPortError::SubmissionOutcomeUnknown)?
}

async fn bounded_response(
    response: reqwest::Response,
    maximum: usize,
) -> Result<Vec<u8>, WalletTransactionPortError> {
    bounded_stream(response.content_length(), response.bytes_stream(), maximum).await
}

async fn bounded_stream<S, B, E>(
    content_length: Option<u64>,
    mut stream: S,
    maximum: usize,
) -> Result<Vec<u8>, WalletTransactionPortError>
where
    S: futures::Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
{
    if content_length.is_some_and(|length| length > maximum as u64) {
        return Err(WalletTransactionPortError::InvalidData);
    }
    let mut result = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| WalletTransactionPortError::Unavailable)?;
        let chunk = chunk.as_ref();
        if result
            .len()
            .checked_add(chunk.len())
            .is_none_or(|length| length > maximum)
        {
            return Err(WalletTransactionPortError::InvalidData);
        }
        result.extend_from_slice(chunk);
    }
    Ok(result)
}

fn decode_bounded_hex(
    value: &str,
    maximum_bytes: usize,
) -> Result<Vec<u8>, WalletTransactionPortError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() > maximum_bytes.saturating_mul(2) || !value.len().is_multiple_of(2) {
        return Err(WalletTransactionPortError::InvalidChainState);
    }
    hex::decode(value).map_err(|_| WalletTransactionPortError::InvalidChainState)
}

fn validate_http_url(value: &str, proof_endpoint: bool) -> Result<String, ()> {
    if value.is_empty() || value.chars().count() > MAX_ENDPOINT_CHARACTERS {
        return Err(());
    }
    let url = Url::parse(value).map_err(|_| ())?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(());
    }
    match url.scheme() {
        "https" => {}
        "http" if !proof_endpoint || is_loopback(&url) => {}
        _ => return Err(()),
    }
    Ok(url.to_string())
}

fn is_loopback(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let ip_literal = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || ip_literal
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn ensure_tls_provider() -> Result<(), WalletTransactionPortError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    rustls::crypto::CryptoProvider::get_default()
        .map(|_| ())
        .ok_or(WalletTransactionPortError::Unavailable)
}

async fn send_websocket_json<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    value: Value,
) -> Result<(), WalletTransactionPortError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .map_err(|_| WalletTransactionPortError::Unavailable)
}

async fn wait_for_ack<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Result<(), WalletTransactionPortError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    timeout(ACK_TIMEOUT, async {
        loop {
            let message = socket
                .next()
                .await
                .ok_or(WalletTransactionPortError::InvalidChainState)?
                .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(text.as_str())
                        .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
                    match websocket_message_type(&value)? {
                        "connection_ack" => return Ok(()),
                        "ping" => {
                            send_websocket_json(
                                socket,
                                json!({ "type": "pong", "payload": value.get("payload") }),
                            )
                            .await?;
                        }
                        _ => return Err(WalletTransactionPortError::InvalidChainState),
                    }
                }
                Message::Ping(payload) => socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|_| WalletTransactionPortError::Unavailable)?,
                Message::Pong(_) => {}
                _ => return Err(WalletTransactionPortError::InvalidChainState),
            }
        }
    })
    .await
    .map_err(|_| WalletTransactionPortError::Timeout)?
}

fn websocket_message_type(value: &Value) -> Result<&str, WalletTransactionPortError> {
    value
        .get("type")
        .and_then(Value::as_str)
        .ok_or(WalletTransactionPortError::InvalidChainState)
}

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, thread};

    use midnight_ledger::{
        events::{EventDetails, EventSource},
        structure::{INITIAL_PARAMETERS, TransactionHash},
    };
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::handshake::server::{Request, Response},
    };

    use super::*;

    const ADDRESS: &str =
        "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
    type DustSubscriptionScenario = (u64, Vec<(u64, u64, String)>);

    fn config(proof: &str) -> Result<MidnightStandaloneConfig, MidnightStandaloneConfigError> {
        MidnightStandaloneConfig::new(
            "devnet",
            "ws://127.0.0.1:8088/api/v1/graphql/ws",
            "http://127.0.0.1:8088/api/v1/graphql",
            "ws://127.0.0.1:9944",
            proof,
            ADDRESS,
        )
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime builds")
    }

    #[allow(clippy::result_large_err)] // tungstenite's test-server callback owns the error type.
    fn serve_dust_event(raw: String) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener binds");
        listener
            .set_nonblocking(true)
            .expect("test listener becomes nonblocking");
        let address = listener.local_addr().expect("test listener has an address");
        let worker = thread::spawn(move || {
            runtime().block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("Tokio listener accepts the socket");
                let (stream, _) = listener.accept().await.expect("client connects");
                let mut socket =
                    accept_hdr_async(stream, |request: &Request, mut response: Response| {
                        assert_eq!(
                            request
                                .headers()
                                .get("Sec-WebSocket-Protocol")
                                .and_then(|value| value.to_str().ok()),
                            Some("graphql-transport-ws")
                        );
                        response.headers_mut().insert(
                            "Sec-WebSocket-Protocol",
                            "graphql-transport-ws"
                                .parse()
                                .expect("protocol header is valid"),
                        );
                        Ok(response)
                    })
                    .await
                    .expect("WebSocket handshake succeeds");
                let initialization = socket
                    .next()
                    .await
                    .expect("initialization arrives")
                    .expect("initialization is valid");
                assert!(initialization.to_text().is_ok_and(|text| {
                    serde_json::from_str::<Value>(text)
                        .ok()
                        .and_then(|value| value.get("type").cloned())
                        == Some(Value::String("connection_init".to_owned()))
                }));
                socket
                    .send(Message::Text(
                        json!({ "type": "connection_ack", "payload": {} })
                            .to_string()
                            .into(),
                    ))
                    .await
                    .expect("acknowledgement sends");
                let subscription = socket
                    .next()
                    .await
                    .expect("subscription arrives")
                    .expect("subscription is valid");
                assert!(subscription.to_text().is_ok_and(|text| {
                    serde_json::from_str::<Value>(text)
                        .ok()
                        .and_then(|value| value.get("type").cloned())
                        == Some(Value::String("subscribe".to_owned()))
                }));
                socket
                    .send(Message::Text(
                        json!({ "type": "ping", "payload": { "request": "keepalive" } })
                            .to_string()
                            .into(),
                    ))
                    .await
                    .expect("protocol ping sends");
                let pong = socket
                    .next()
                    .await
                    .expect("protocol pong arrives")
                    .expect("protocol pong is valid");
                assert!(pong.to_text().is_ok_and(|text| {
                    serde_json::from_str::<Value>(text).is_ok_and(|value| {
                        value.get("type").and_then(Value::as_str) == Some("pong")
                            && value.pointer("/payload/request").and_then(Value::as_str)
                                == Some("keepalive")
                    })
                }));
                socket
                    .send(Message::Text(
                        json!({
                            "type": "next",
                            "id": "oxid-dust",
                            "payload": {
                                "data": {
                                    "dustLedgerEvents": {
                                        "id": 1,
                                        "maxId": 1,
                                        "raw": raw
                                    }
                                }
                            }
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .expect("event sends");
            });
        });
        (format!("ws://{address}/graphql/ws"), worker)
    }

    #[allow(clippy::result_large_err)]
    fn serve_dust_subscriptions(
        scenarios: Vec<DustSubscriptionScenario>,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener binds");
        listener
            .set_nonblocking(true)
            .expect("test listener becomes nonblocking");
        let address = listener.local_addr().expect("test listener has an address");
        let worker = thread::spawn(move || {
            runtime().block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("Tokio listener accepts sockets");
                for (expected_start, events) in scenarios {
                    let (stream, _) = listener.accept().await.expect("client connects");
                    let mut socket =
                        accept_hdr_async(stream, |_: &Request, mut response: Response| {
                            response.headers_mut().insert(
                                "Sec-WebSocket-Protocol",
                                "graphql-transport-ws"
                                    .parse()
                                    .expect("protocol header is valid"),
                            );
                            Ok(response)
                        })
                        .await
                        .expect("WebSocket handshake succeeds");
                    let _ = socket.next().await.expect("initialization arrives");
                    socket
                        .send(Message::Text(
                            json!({ "type": "connection_ack", "payload": {} })
                                .to_string()
                                .into(),
                        ))
                        .await
                        .expect("acknowledgement sends");
                    let subscription = socket
                        .next()
                        .await
                        .expect("subscription arrives")
                        .expect("subscription is valid");
                    let request: Value = serde_json::from_str(
                        subscription.to_text().expect("subscription is textual"),
                    )
                    .expect("subscription is JSON");
                    assert_eq!(
                        request
                            .pointer("/payload/variables/id")
                            .and_then(Value::as_u64),
                        Some(expected_start)
                    );
                    if events.is_empty() {
                        socket
                            .send(Message::Text(
                                json!({ "type": "complete", "id": "oxid-dust" })
                                    .to_string()
                                    .into(),
                            ))
                            .await
                            .expect("completion sends");
                    } else {
                        for (id, max_id, raw) in events {
                            socket
                                .send(Message::Text(
                                    json!({
                                        "type": "next",
                                        "id": "oxid-dust",
                                        "payload": {
                                            "data": {
                                                "dustLedgerEvents": {
                                                    "id": id,
                                                    "maxId": max_id,
                                                    "raw": raw
                                                }
                                            }
                                        }
                                    })
                                    .to_string()
                                    .into(),
                                ))
                                .await
                                .expect("event sends");
                        }
                    }
                }
            });
        });
        (format!("ws://{address}/graphql/ws"), worker)
    }

    fn parameter_change_event_hex() -> String {
        let event = Event::<DefaultDB> {
            source: EventSource {
                transaction_hash: TransactionHash::default(),
                logical_segment: 0,
                physical_segment: 0,
            },
            content: EventDetails::ParamChange(Sp::new(INITIAL_PARAMETERS)),
        };
        let mut bytes = Vec::new();
        midnight_serialize::tagged_serialize(&event, &mut bytes).expect("event serializes");
        hex::encode(bytes)
    }

    #[test]
    fn standalone_routes_accept_loopback_http_proving() {
        let value = config("http://127.0.0.1:6300").expect("routes are valid");
        assert_eq!(value.indexer().network_id().as_str(), "devnet");
        assert_eq!(
            value.indexer_http_url(),
            "http://127.0.0.1:8088/api/v1/graphql"
        );
        assert_eq!(value.node_websocket_url(), "ws://127.0.0.1:9944");
        assert!(matches!(
            value.proving(),
            MidnightProvingMode::Remote { proof_server_url }
                if proof_server_url == "http://127.0.0.1:6300/"
        ));
    }

    #[test]
    fn proof_route_rejects_remote_plaintext_and_credentials() {
        assert_eq!(
            config("http://proof.example.test"),
            Err(MidnightStandaloneConfigError::InvalidProofEndpoint)
        );
        assert_eq!(
            config("https://user:secret@proof.example.test"),
            Err(MidnightStandaloneConfigError::InvalidProofEndpoint)
        );
        let bad_http = MidnightStandaloneConfig::new(
            "devnet",
            "ws://127.0.0.1:8088/graphql/ws",
            "ftp://127.0.0.1/graphql",
            "ws://127.0.0.1:9944",
            "http://127.0.0.1:6300",
            ADDRESS,
        )
        .expect_err("non-HTTP indexer route is rejected");
        assert_eq!(
            bad_http.to_string(),
            "Midnight indexer HTTP endpoint is invalid"
        );
        let bad_node = MidnightStandaloneConfig::new(
            "devnet",
            "ws://127.0.0.1:8088/graphql/ws",
            "http://127.0.0.1/graphql",
            "http://127.0.0.1:9944",
            "http://127.0.0.1:6300",
            ADDRESS,
        )
        .expect_err("non-WebSocket node route is rejected");
        assert_eq!(
            bad_node.to_string(),
            "Midnight node WebSocket endpoint is invalid"
        );
        let bad_network = MidnightStandaloneConfig::new(
            "unknown-network",
            "ws://127.0.0.1:8088/graphql/ws",
            "http://127.0.0.1/graphql",
            "ws://127.0.0.1:9944",
            "http://127.0.0.1:6300",
            ADDRESS,
        )
        .expect_err("unknown network is rejected");
        assert!(matches!(
            bad_network,
            MidnightStandaloneConfigError::Indexer(_)
        ));
        assert_eq!(
            bad_network.to_string(),
            "Midnight indexer network is not supported"
        );
    }

    #[test]
    fn proof_route_accepts_remote_tls_without_rendering_it_in_errors() {
        assert!(config("https://proof.example.test/base").is_ok());
        let error = config("http://proof.example.test")
            .expect_err("plaintext remote route is rejected")
            .to_string();
        assert!(!error.contains("proof.example.test"));
    }

    #[test]
    fn route_validation_rejects_ambiguous_and_unbounded_urls() {
        assert!(validate_http_url("", false).is_err());
        assert!(validate_http_url(&format!("https://{}", "a".repeat(2_048)), false).is_err());
        assert!(validate_http_url("https://indexer.test/graphql?token=public", false).is_err());
        assert!(validate_http_url("https://indexer.test/graphql#fragment", false).is_err());
        assert!(validate_http_url("ftp://indexer.test/graphql", false).is_err());
        assert!(validate_http_url("http://proof.example.test", true).is_err());
        assert!(validate_http_url("http://localhost:6300", true).is_ok());
        assert!(validate_http_url("http://[::1]:6300", true).is_ok());
        assert!(!is_loopback(
            &Url::parse("https://proof.example.test").expect("URL is valid")
        ));
    }

    #[test]
    fn malformed_dust_envelopes_fail_before_ledger_decode() {
        let missing = decode_dust_event(&json!({ "id": 0, "maxId": 0 }));
        assert_eq!(
            missing.err(),
            Some(WalletTransactionPortError::InvalidChainState)
        );
        let oversized = "00".repeat(MAX_DUST_EVENT_BYTES + 1);
        let result = decode_dust_event(&json!({ "id": 0, "maxId": 0, "raw": oversized }));
        assert_eq!(
            result.err(),
            Some(WalletTransactionPortError::InvalidChainState)
        );
    }

    #[test]
    fn chain_tip_timestamp_converts_indexer_milliseconds_to_ledger_seconds() {
        let mut parameters = Vec::new();
        midnight_serialize::tagged_serialize(&INITIAL_PARAMETERS, &mut parameters)
            .expect("initial parameters serialize");
        let tip = decode_chain_tip(&json!({
            "data": {
                "block": {
                    "timestamp": 1_750_000_000_123_i64,
                    "ledgerParameters": hex::encode(parameters)
                }
            }
        }))
        .expect("valid chain tip decodes");

        assert_eq!(tip.timestamp, Timestamp::from_secs(1_750_000_000));
        assert_eq!(tip.parameters, INITIAL_PARAMETERS);
    }

    #[test]
    fn chain_tip_decoder_rejects_missing_negative_and_malformed_fields() {
        for value in [
            json!({ "data": { "block": null } }),
            json!({ "data": { "block": { "timestamp": -1, "ledgerParameters": "00" } } }),
            json!({ "data": { "block": { "timestamp": 1 } } }),
            json!({ "data": { "block": { "timestamp": 1, "ledgerParameters": "0" } } }),
            json!({ "data": { "block": { "timestamp": 1, "ledgerParameters": "zz" } } }),
        ] {
            assert_eq!(
                decode_chain_tip(&value).err(),
                Some(WalletTransactionPortError::InvalidChainState)
            );
        }
        assert_eq!(
            decode_bounded_hex(&"00".repeat(3), 2).err(),
            Some(WalletTransactionPortError::InvalidChainState)
        );
    }

    #[test]
    fn chain_tip_http_response_uses_the_bounded_decoder() {
        let mut parameters = Vec::new();
        midnight_serialize::tagged_serialize(&INITIAL_PARAMETERS, &mut parameters)
            .expect("initial parameters serialize");
        let body = serde_json::to_vec(&json!({
            "data": {
                "block": {
                    "timestamp": 1_750_000_123_999_i64,
                    "ledgerParameters": format!("0x{}", hex::encode(parameters))
                }
            }
        }))
        .expect("response serializes");
        validate_chain_tip_status(StatusCode::OK).expect("successful status is accepted");
        let tip = decode_chain_tip_body(&body).expect("bounded chain tip succeeds");

        assert_eq!(tip.timestamp, Timestamp::from_secs(1_750_000_123));
        assert_eq!(tip.parameters, INITIAL_PARAMETERS);
        let request = chain_tip_request("http://127.0.0.1:8088/api/v1/graphql")
            .expect("chain tip request builds");
        assert_eq!(request.method(), reqwest::Method::POST);
        assert_eq!(request.url().path(), "/api/v1/graphql");
        assert_eq!(
            request
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        assert!(
            request
                .body()
                .and_then(reqwest::Body::as_bytes)
                .is_some_and(|body| serde_json::from_slice::<Value>(body)
                    .ok()
                    .and_then(|value| value.get("query").cloned())
                    == Some(Value::String(CHAIN_TIP_QUERY.to_owned())))
        );
        assert_eq!(
            chain_tip_request("://invalid").err(),
            Some(WalletTransactionPortError::Unavailable)
        );
        assert_eq!(
            decode_chain_tip_body(&vec![0_u8; MAX_CHAIN_TIP_BYTES + 1]).err(),
            Some(WalletTransactionPortError::InvalidChainState)
        );
    }

    #[test]
    fn chain_tip_http_response_rejects_http_and_graphql_failures() {
        assert_eq!(
            validate_chain_tip_status(StatusCode::SERVICE_UNAVAILABLE).err(),
            Some(WalletTransactionPortError::InvalidChainState)
        );

        let body = serde_json::to_vec(&json!({
            "errors": [{ "message": "not exposed by the adapter" }],
            "data": { "block": null }
        }))
        .expect("response serializes");
        assert_eq!(
            decode_chain_tip_body(&body).err(),
            Some(WalletTransactionPortError::InvalidChainState)
        );
        assert_eq!(
            decode_chain_tip_body(b"not-json").err(),
            Some(WalletTransactionPortError::InvalidChainState)
        );
    }

    #[test]
    fn bounded_stream_rejects_declared_streamed_and_transport_overflow() {
        let success = runtime()
            .block_on(bounded_stream(
                Some(4),
                futures::stream::iter([Ok::<_, ()>(vec![1_u8, 2]), Ok(vec![3_u8, 4])]),
                4,
            ))
            .expect("bounded chunks collect");
        assert_eq!(success, vec![1, 2, 3, 4]);
        assert_eq!(
            runtime()
                .block_on(bounded_stream(
                    Some(5),
                    futures::stream::iter([Ok::<_, ()>(vec![1_u8])]),
                    4,
                ))
                .err(),
            Some(WalletTransactionPortError::InvalidData)
        );
        assert_eq!(
            runtime()
                .block_on(bounded_stream(
                    None,
                    futures::stream::iter([Ok::<_, ()>(vec![1_u8, 2, 3]), Ok(vec![4, 5])]),
                    4,
                ))
                .err(),
            Some(WalletTransactionPortError::InvalidData)
        );
        assert_eq!(
            runtime()
                .block_on(bounded_stream(
                    None,
                    futures::stream::iter([Err::<Vec<u8>, _>(())]),
                    4,
                ))
                .err(),
            Some(WalletTransactionPortError::Unavailable)
        );
    }

    #[test]
    fn dust_snapshot_negotiates_graphql_and_replays_tagged_events() {
        let event = Event::<DefaultDB> {
            source: EventSource {
                transaction_hash: TransactionHash::default(),
                logical_segment: 0,
                physical_segment: 0,
            },
            content: EventDetails::ParamChange(Sp::new(INITIAL_PARAMETERS)),
        };
        let mut event_bytes = Vec::new();
        midnight_serialize::tagged_serialize(&event, &mut event_bytes).expect("event serializes");
        let (endpoint, worker) = serve_dust_event(hex::encode(event_bytes));
        let dust_key = DustSecretKey::derive_secret_key(&[7; 32]);
        let state = runtime()
            .block_on(synchronize_dust(
                &endpoint,
                &dust_key,
                INITIAL_PARAMETERS.dust,
                None,
            ))
            .expect("bounded DUST snapshot succeeds");
        worker.join().expect("WebSocket worker completes");

        assert_eq!(state.state.sync_time, Timestamp::from_secs(0));
        assert_eq!(state.state.params, INITIAL_PARAMETERS.dust);
        assert_eq!(state.current_cursor, 1);
        assert_eq!(state.target_cursor, 1);
    }

    #[test]
    fn dust_checkpoint_resumes_from_the_next_cursor_and_accepts_live_empty_catchup() {
        use oxid_foundation::UnixTimestampMillis;

        let raw = parameter_change_event_hex();
        let (endpoint, worker) = serve_dust_subscriptions(vec![(43, vec![(43, 43, raw)])]);
        let dust_key = DustSecretKey::derive_secret_key(&[7; 32]);
        let checkpoint = StoredDustCheckpoint {
            current_cursor: 42,
            target_cursor: 42,
            updated_at: UnixTimestampMillis::new(1_700_000_000_000),
            state: DustLocalState::new(INITIAL_PARAMETERS.dust),
        };
        let synchronized = runtime()
            .block_on(synchronize_dust(
                &endpoint,
                &dust_key,
                INITIAL_PARAMETERS.dust,
                Some(checkpoint),
            ))
            .expect("delta replay succeeds");
        worker.join().expect("WebSocket worker completes");
        assert_eq!(synchronized.current_cursor, 43);
        assert_eq!(synchronized.target_cursor, 43);

        let (endpoint, worker) = serve_dust_subscriptions(vec![(44, Vec::new())]);
        let caught_up = runtime()
            .block_on(synchronize_dust(
                &endpoint,
                &dust_key,
                INITIAL_PARAMETERS.dust,
                Some(StoredDustCheckpoint {
                    current_cursor: 43,
                    target_cursor: 43,
                    updated_at: UnixTimestampMillis::new(1_700_000_000_001),
                    state: synchronized.state,
                }),
            ))
            .expect("live empty delta confirms an up-to-date checkpoint");
        worker.join().expect("WebSocket worker completes");
        assert_eq!(caught_up.current_cursor, 43);
        assert_eq!(caught_up.target_cursor, 43);
    }

    #[test]
    fn dust_sync_accepts_sparse_global_cursors_but_preserves_order() {
        let raw = parameter_change_event_hex();
        let (endpoint, worker) =
            serve_dust_subscriptions(vec![(0, vec![(1, 30, raw.clone()), (30, 30, raw)])]);
        let dust_key = DustSecretKey::derive_secret_key(&[7; 32]);
        let synchronized = runtime()
            .block_on(synchronize_dust(
                &endpoint,
                &dust_key,
                INITIAL_PARAMETERS.dust,
                None,
            ))
            .expect("sparse global DUST cursors replay");
        worker.join().expect("WebSocket worker completes");

        assert_eq!(synchronized.current_cursor, 30);
        assert_eq!(synchronized.target_cursor, 30);
        assert_eq!(synchronized.events_processed, 2);
    }

    #[test]
    fn controlled_dust_sync_reports_a_consistent_batch_before_cancellation() {
        let raw = parameter_change_event_hex();
        let events = (1_u64..=257)
            .map(|id| (id, 257, raw.clone()))
            .collect::<Vec<_>>();
        let (endpoint, worker) = serve_dust_subscriptions(vec![(0, events)]);
        let dust_key = DustSecretKey::derive_secret_key(&[7; 32]);
        let cancellation = AtomicBool::new(false);
        let mut observed = Vec::new();
        let result = runtime().block_on(synchronize_dust_controlled(
            &endpoint,
            &dust_key,
            INITIAL_PARAMETERS.dust,
            None,
            &cancellation,
            &mut |progress| {
                observed.push((
                    progress.current_cursor,
                    progress.target_cursor,
                    progress.events_processed,
                ));
                cancellation.store(true, Ordering::Release);
                Ok(())
            },
        ));
        worker.join().expect("WebSocket worker completes");

        assert_eq!(
            result.err(),
            Some(WalletTransactionPortError::SubmissionCancelled)
        );
        assert_eq!(observed, vec![(256, 257, 256)]);
    }

    #[test]
    fn incompatible_dust_delta_replays_once_from_zero() {
        use oxid_foundation::UnixTimestampMillis;

        let raw = parameter_change_event_hex();
        let (endpoint, worker) =
            serve_dust_subscriptions(vec![(1, vec![(0, 2, raw.clone())]), (0, vec![(1, 1, raw)])]);
        let dust_key = DustSecretKey::derive_secret_key(&[8; 32]);
        let synchronized = runtime()
            .block_on(synchronize_dust_with_fallback(
                &endpoint,
                &dust_key,
                INITIAL_PARAMETERS.dust,
                Some(StoredDustCheckpoint {
                    current_cursor: 0,
                    target_cursor: 0,
                    updated_at: UnixTimestampMillis::new(1_700_000_000_000),
                    state: DustLocalState::new(INITIAL_PARAMETERS.dust),
                }),
            ))
            .expect("incompatible delta recovers with one clean replay");
        worker.join().expect("both WebSocket attempts complete");
        assert_eq!(synchronized.current_cursor, 1);
        assert_eq!(synchronized.target_cursor, 1);
    }

    #[test]
    fn cached_dust_state_does_not_hide_a_live_transport_failure() {
        use oxid_foundation::UnixTimestampMillis;

        let listener = TcpListener::bind("127.0.0.1:0").expect("unused endpoint reserves a port");
        let address = listener.local_addr().expect("listener has an address");
        drop(listener);
        let dust_key = DustSecretKey::derive_secret_key(&[9; 32]);
        let result = runtime().block_on(synchronize_dust_with_fallback(
            &format!("ws://{address}/graphql/ws"),
            &dust_key,
            INITIAL_PARAMETERS.dust,
            Some(StoredDustCheckpoint {
                current_cursor: 7,
                target_cursor: 7,
                updated_at: UnixTimestampMillis::new(1_700_000_000_000),
                state: DustLocalState::new(INITIAL_PARAMETERS.dust),
            }),
        ));
        assert_eq!(result.err(), Some(WalletTransactionPortError::Unavailable));
    }

    #[test]
    fn chain_identity_authentication_rejects_invalid_routes_without_network_io() {
        assert_eq!(
            runtime().block_on(authenticate_midnight_chain_identity(
                "http://node.example.test",
                &[0; 32],
            )),
            Err(MidnightChainIdentityError::InvalidNodeEndpoint)
        );
    }

    #[test]
    fn empty_standard_transaction_fails_closed_without_dust() {
        let transaction = Transaction::Standard(StandardTransaction::new(
            "undeployed",
            LedgerHashMap::new(),
            None,
            LedgerHashMap::new(),
        ));
        let dust_key = DustSecretKey::derive_secret_key(&[9; 32]);
        let mut dust_state = DustLocalState::new(INITIAL_PARAMETERS.dust);
        assert_eq!(
            balance_dust(
                transaction,
                &mut dust_state,
                &dust_key,
                &INITIAL_PARAMETERS,
                Timestamp::from_secs(1_700_000_000),
                Timestamp::from_secs(1_700_003_600),
                "undeployed",
            )
            .err(),
            Some(WalletTransactionPortError::InsufficientDust)
        );
    }
}
