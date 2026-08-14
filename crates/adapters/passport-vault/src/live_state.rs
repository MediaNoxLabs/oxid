// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    net::IpAddr,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use futures::{StreamExt, channel::oneshot};
use oxid_passport_vault_application::{
    MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES, PassportVaultCallPortError,
    PassportVaultContractStateAuthentication, PassportVaultContractStateReadFuture,
    PassportVaultContractStateSnapshot, PassportVaultContractStateSourceError,
    PassportVaultContractStateSourcePort,
};
use reqwest::{Certificate, Client, Method, Url, header::CONTENT_TYPE, redirect::Policy};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use subxt::{
    SubstrateConfig,
    backend::{legacy::LegacyRpcMethods, rpc::RpcClient},
    config::Header,
};
use tokio::time::timeout;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ENDPOINT_CHARACTERS: usize = 2_048;
const MAX_CHAIN_CONTEXTS: usize = 128;
const MAX_ZSWAP_CHAIN_STATE_BYTES: usize = 2 * 1024 * 1024;
const MAX_LEDGER_PARAMETERS_BYTES: usize = 512 * 1024;
const MAX_RESPONSE_BYTES: usize = (MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES
    + MAX_ZSWAP_CHAIN_STATE_BYTES
    + MAX_LEDGER_PARAMETERS_BYTES)
    * 2
    + 128 * 1024;

const CONTRACT_STATE_AT_FINALIZED_HEIGHT_QUERY: &str = r#"
query OxidPassportVaultState($address: HexEncoded!, $height: Int!) {
  block(offset: { height: $height }) {
    hash
    height
    ledgerParameters
  }
  contractAction(
    address: $address
    offset: { blockOffset: { height: $height } }
  ) {
    address
    state
    zswapState
    transaction {
      hash
      block {
        hash
        height
      }
    }
  }
}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeAnchoredPassportVaultStateConfigError {
    InvalidIndexerEndpoint,
    InvalidNodeEndpoint,
    ClientUnavailable,
}

impl fmt::Display for NodeAnchoredPassportVaultStateConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidIndexerEndpoint => "Passport Vault indexer endpoint is invalid",
            Self::InvalidNodeEndpoint => "Passport Vault node endpoint is invalid",
            Self::ClientUnavailable => "Passport Vault state client is unavailable",
        })
    }
}

impl Error for NodeAnchoredPassportVaultStateConfigError {}

struct NodeAnchoredPassportVaultStateConfig {
    indexer_endpoint: Url,
    node_endpoint: String,
    client: Client,
    contexts: Mutex<BTreeMap<String, PassportVaultCallChainContext>>,
}

/// Bounded public chain material retained beside the exact indexer snapshot
/// that supplied it. Contract state and anchor metadata are retained only as a
/// digest and public identifiers so callers cannot substitute chain context.
#[derive(Clone, PartialEq, Eq)]
pub struct PassportVaultCallChainContext {
    contract_address_hex: String,
    transaction_hash_hex: String,
    action_block_hash_hex: String,
    action_block_height: u64,
    finalized_head_hash_hex: String,
    finalized_head_height: u64,
    contract_state_digest: [u8; 32],
    zswap_chain_state: Vec<u8>,
    ledger_parameters: Vec<u8>,
}

impl fmt::Debug for PassportVaultCallChainContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportVaultCallChainContext")
            .field("contract_address_hex", &self.contract_address_hex)
            .field("action_block_height", &self.action_block_height)
            .field("finalized_head_height", &self.finalized_head_height)
            .field("zswap_chain_state_bytes", &self.zswap_chain_state.len())
            .field("ledger_parameters_bytes", &self.ledger_parameters.len())
            .finish_non_exhaustive()
    }
}

impl PassportVaultCallChainContext {
    pub fn from_snapshot(
        snapshot: &PassportVaultContractStateSnapshot,
        zswap_chain_state: Vec<u8>,
        ledger_parameters: Vec<u8>,
    ) -> Result<Self, PassportVaultContractStateSourceError> {
        if snapshot.serialized_contract_state.is_empty()
            || snapshot.serialized_contract_state.len() > MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES
            || normalize_hex_32(&snapshot.contract_address_hex).as_deref()
                != Some(snapshot.contract_address_hex.as_str())
            || normalize_hex_32(&snapshot.transaction_hash_hex).as_deref()
                != Some(snapshot.transaction_hash_hex.as_str())
            || normalize_hex_32(&snapshot.action_block_hash_hex).as_deref()
                != Some(snapshot.action_block_hash_hex.as_str())
            || normalize_hex_32(&snapshot.finalized_head_hash_hex).as_deref()
                != Some(snapshot.finalized_head_hash_hex.as_str())
            || snapshot.action_block_height > snapshot.finalized_head_height
        {
            return Err(PassportVaultContractStateSourceError::InvalidResponse);
        }
        if zswap_chain_state.is_empty() || ledger_parameters.is_empty() {
            return Err(PassportVaultContractStateSourceError::InvalidResponse);
        }
        if zswap_chain_state.len() > MAX_ZSWAP_CHAIN_STATE_BYTES
            || ledger_parameters.len() > MAX_LEDGER_PARAMETERS_BYTES
        {
            return Err(PassportVaultContractStateSourceError::CapacityExceeded);
        }
        Ok(Self {
            contract_address_hex: snapshot.contract_address_hex.clone(),
            transaction_hash_hex: snapshot.transaction_hash_hex.clone(),
            action_block_hash_hex: snapshot.action_block_hash_hex.clone(),
            action_block_height: snapshot.action_block_height,
            finalized_head_hash_hex: snapshot.finalized_head_hash_hex.clone(),
            finalized_head_height: snapshot.finalized_head_height,
            contract_state_digest: Sha256::digest(&snapshot.serialized_contract_state).into(),
            zswap_chain_state,
            ledger_parameters,
        })
    }

    #[must_use]
    pub fn zswap_chain_state(&self) -> &[u8] {
        &self.zswap_chain_state
    }

    #[must_use]
    pub fn ledger_parameters(&self) -> &[u8] {
        &self.ledger_parameters
    }

    fn matches_snapshot(&self, snapshot: &PassportVaultContractStateSnapshot) -> bool {
        self.contract_address_hex == snapshot.contract_address_hex
            && self.transaction_hash_hex == snapshot.transaction_hash_hex
            && self.action_block_hash_hex == snapshot.action_block_hash_hex
            && self.action_block_height == snapshot.action_block_height
            && self.finalized_head_height >= snapshot.finalized_head_height
            && (self.finalized_head_height != snapshot.finalized_head_height
                || self.finalized_head_hash_hex == snapshot.finalized_head_hash_hex)
            && self.contract_state_digest
                == <[u8; 32]>::from(Sha256::digest(&snapshot.serialized_contract_state))
    }
}

/// Resolves chain composition material only for an already authenticated,
/// exact Passport Vault state snapshot.
pub trait PassportVaultCallChainContextSource: Send + Sync {
    fn chain_context(
        &self,
        snapshot: &PassportVaultContractStateSnapshot,
    ) -> Result<PassportVaultCallChainContext, PassportVaultCallPortError>;
}

/// Reads an indexer snapshot at a node-finalized height and verifies that the
/// action's block hash is canonical at its reported height. The adapter does
/// not claim that this proves the indexer-supplied state bytes.
#[derive(Clone)]
pub struct NodeAnchoredPassportVaultStateSource(Arc<NodeAnchoredPassportVaultStateConfig>);

impl NodeAnchoredPassportVaultStateSource {
    pub fn new(
        indexer_endpoint: impl AsRef<str>,
        node_endpoint: impl AsRef<str>,
    ) -> Result<Self, NodeAnchoredPassportVaultStateConfigError> {
        ensure_tls_provider()
            .map_err(|()| NodeAnchoredPassportVaultStateConfigError::ClientUnavailable)?;
        let indexer_endpoint = validate_http_endpoint(indexer_endpoint.as_ref())
            .ok_or(NodeAnchoredPassportVaultStateConfigError::InvalidIndexerEndpoint)?;
        let node_endpoint = validate_node_endpoint(node_endpoint.as_ref())
            .ok_or(NodeAnchoredPassportVaultStateConfigError::InvalidNodeEndpoint)?;
        let trusted_roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|certificate| Certificate::from_der(certificate.as_ref()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| NodeAnchoredPassportVaultStateConfigError::ClientUnavailable)?;
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent("oxid-identity-wallet/0.1")
            .tls_certs_only(trusted_roots)
            .build()
            .map_err(|_| NodeAnchoredPassportVaultStateConfigError::ClientUnavailable)?;
        Ok(Self(Arc::new(NodeAnchoredPassportVaultStateConfig {
            indexer_endpoint,
            node_endpoint,
            client,
            contexts: Mutex::new(BTreeMap::new()),
        })))
    }
}

impl PassportVaultCallChainContextSource for NodeAnchoredPassportVaultStateSource {
    fn chain_context(
        &self,
        snapshot: &PassportVaultContractStateSnapshot,
    ) -> Result<PassportVaultCallChainContext, PassportVaultCallPortError> {
        let contexts = self
            .0
            .contexts
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?;
        let context = contexts
            .get(&snapshot.contract_address_hex)
            .filter(|context| context.matches_snapshot(snapshot))
            .ok_or(PassportVaultCallPortError::InvalidChainState)?;
        Ok(context.clone())
    }
}

impl PassportVaultContractStateSourcePort for NodeAnchoredPassportVaultStateSource {
    fn read<'a>(
        &'a self,
        contract_address_hex: &'a str,
    ) -> PassportVaultContractStateReadFuture<'a> {
        let config = Arc::clone(&self.0);
        let contract_address_hex = contract_address_hex.to_owned();
        let (sender, receiver) = oneshot::channel();
        let spawned = thread::Builder::new()
            .name("oxid-vault-state".to_owned())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| PassportVaultContractStateSourceError::Unavailable)
                    .and_then(|runtime| {
                        runtime.block_on(read_on_runtime(config, contract_address_hex))
                    });
                let _ = sender.send(result);
            });
        if spawned.is_err() {
            return Box::pin(async { Err(PassportVaultContractStateSourceError::Unavailable) });
        }
        Box::pin(async move {
            receiver
                .await
                .unwrap_or(Err(PassportVaultContractStateSourceError::Unavailable))
        })
    }
}

async fn read_on_runtime(
    config: Arc<NodeAnchoredPassportVaultStateConfig>,
    contract_address_hex: String,
) -> Result<PassportVaultContractStateSnapshot, PassportVaultContractStateSourceError> {
    let rpc_client = timeout(
        CONNECT_TIMEOUT,
        RpcClient::from_insecure_url(&config.node_endpoint),
    )
    .await
    .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?
    .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?;
    let rpc = LegacyRpcMethods::<SubstrateConfig>::new(rpc_client);
    let finalized_head = timeout(CONNECT_TIMEOUT, rpc.chain_get_finalized_head())
        .await
        .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?
        .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?;
    let finalized_header = timeout(CONNECT_TIMEOUT, rpc.chain_get_header(Some(finalized_head)))
        .await
        .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?
        .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?
        .ok_or(PassportVaultContractStateSourceError::InvalidResponse)?;
    let finalized_head_height: u64 = finalized_header.number().into();
    let graphql_height = i32::try_from(finalized_head_height)
        .map_err(|_| PassportVaultContractStateSourceError::CapacityExceeded)?;

    let (mut snapshot, context) = fetch_indexer_snapshot(
        &config,
        &contract_address_hex,
        graphql_height,
        hex::encode(finalized_head.as_ref()),
        finalized_head_height,
    )
    .await?;
    let action_height = snapshot.action_block_height;
    let canonical_action_hash = timeout(
        CONNECT_TIMEOUT,
        rpc.chain_get_block_hash(Some(action_height.into())),
    )
    .await
    .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?
    .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?
    .ok_or(PassportVaultContractStateSourceError::FinalityMismatch)?;
    let canonical_action_hash_hex = hex::encode(canonical_action_hash.as_ref());
    if canonical_action_hash_hex != snapshot.action_block_hash_hex {
        return Err(PassportVaultContractStateSourceError::FinalityMismatch);
    }
    snapshot.finalized_head_hash_hex = hex::encode(finalized_head.as_ref());
    retain_context(&config, context)?;
    Ok(snapshot)
}

fn retain_context(
    config: &NodeAnchoredPassportVaultStateConfig,
    context: PassportVaultCallChainContext,
) -> Result<(), PassportVaultContractStateSourceError> {
    let mut contexts = config
        .contexts
        .lock()
        .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?;
    if !contexts.contains_key(&context.contract_address_hex) && contexts.len() >= MAX_CHAIN_CONTEXTS
    {
        return Err(PassportVaultContractStateSourceError::CapacityExceeded);
    }
    contexts.insert(context.contract_address_hex.clone(), context);
    Ok(())
}

async fn fetch_indexer_snapshot(
    config: &NodeAnchoredPassportVaultStateConfig,
    contract_address_hex: &str,
    finalized_head_height: i32,
    finalized_head_hash_hex: String,
    finalized_head_height_u64: u64,
) -> Result<
    (
        PassportVaultContractStateSnapshot,
        PassportVaultCallChainContext,
    ),
    PassportVaultContractStateSourceError,
> {
    let body = serde_json::to_vec(&json!({
        "query": CONTRACT_STATE_AT_FINALIZED_HEIGHT_QUERY,
        "variables": {
            "address": contract_address_hex,
            "height": finalized_head_height,
        }
    }))
    .map_err(|_| PassportVaultContractStateSourceError::InvalidResponse)?;
    let mut request = reqwest::Request::new(Method::POST, config.indexer_endpoint.clone());
    request.headers_mut().insert(
        CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    *request.body_mut() = Some(reqwest::Body::from(body));
    let response = config
        .client
        .execute(request)
        .await
        .map_err(|_| PassportVaultContractStateSourceError::Unavailable)?;
    if !response.status().is_success() {
        return Err(PassportVaultContractStateSourceError::InvalidResponse);
    }
    let response = bounded_response(response).await?;
    decode_indexer_response(
        &response,
        contract_address_hex,
        finalized_head_hash_hex,
        finalized_head_height_u64,
    )
}

async fn bounded_response(
    response: reqwest::Response,
) -> Result<Vec<u8>, PassportVaultContractStateSourceError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(PassportVaultContractStateSourceError::CapacityExceeded);
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| PassportVaultContractStateSourceError::Unavailable)?;
        if bytes
            .len()
            .checked_add(chunk.len())
            .is_none_or(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err(PassportVaultContractStateSourceError::CapacityExceeded);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[derive(Deserialize)]
struct GraphqlResponse {
    data: Option<GraphqlData>,
    errors: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlData {
    block: Option<GraphqlFinalizedBlock>,
    contract_action: Option<GraphqlContractAction>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlContractAction {
    address: String,
    state: String,
    zswap_state: String,
    transaction: GraphqlTransaction,
}

#[derive(Deserialize)]
struct GraphqlTransaction {
    hash: String,
    block: GraphqlBlock,
}

#[derive(Deserialize)]
struct GraphqlBlock {
    hash: String,
    height: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlFinalizedBlock {
    hash: String,
    height: i64,
    ledger_parameters: String,
}

fn decode_indexer_response(
    response: &[u8],
    expected_contract_address_hex: &str,
    finalized_head_hash_hex: String,
    finalized_head_height: u64,
) -> Result<
    (
        PassportVaultContractStateSnapshot,
        PassportVaultCallChainContext,
    ),
    PassportVaultContractStateSourceError,
> {
    if response.len() > MAX_RESPONSE_BYTES {
        return Err(PassportVaultContractStateSourceError::CapacityExceeded);
    }
    let response: GraphqlResponse = serde_json::from_slice(response)
        .map_err(|_| PassportVaultContractStateSourceError::InvalidResponse)?;
    if response.errors.is_some_and(|errors| !errors.is_empty()) {
        return Err(PassportVaultContractStateSourceError::InvalidResponse);
    }
    let data = response
        .data
        .ok_or(PassportVaultContractStateSourceError::NotFound)?;
    let finalized_block = data
        .block
        .ok_or(PassportVaultContractStateSourceError::NotFound)?;
    let action = data
        .contract_action
        .ok_or(PassportVaultContractStateSourceError::NotFound)?;
    let returned_finalized_hash = normalize_hex_32(&finalized_block.hash)
        .ok_or(PassportVaultContractStateSourceError::InvalidResponse)?;
    let returned_finalized_height = u64::try_from(finalized_block.height)
        .map_err(|_| PassportVaultContractStateSourceError::InvalidResponse)?;
    if returned_finalized_hash != finalized_head_hash_hex
        || returned_finalized_height != finalized_head_height
    {
        return Err(PassportVaultContractStateSourceError::FinalityMismatch);
    }
    let contract_address_hex = normalize_hex_32(&action.address)
        .ok_or(PassportVaultContractStateSourceError::InvalidResponse)?;
    if contract_address_hex != expected_contract_address_hex {
        return Err(PassportVaultContractStateSourceError::InvalidResponse);
    }
    let transaction_hash_hex = normalize_hex_32(&action.transaction.hash)
        .ok_or(PassportVaultContractStateSourceError::InvalidResponse)?;
    let action_block_hash_hex = normalize_hex_32(&action.transaction.block.hash)
        .ok_or(PassportVaultContractStateSourceError::InvalidResponse)?;
    let action_block_height = u64::try_from(action.transaction.block.height)
        .map_err(|_| PassportVaultContractStateSourceError::InvalidResponse)?;
    if action_block_height > finalized_head_height {
        return Err(PassportVaultContractStateSourceError::FinalityMismatch);
    }
    let serialized_contract_state = decode_bounded_hex_state(&action.state)?;
    let zswap_chain_state = decode_bounded_hex(&action.zswap_state, MAX_ZSWAP_CHAIN_STATE_BYTES)?;
    let ledger_parameters = decode_bounded_hex(
        &finalized_block.ledger_parameters,
        MAX_LEDGER_PARAMETERS_BYTES,
    )?;
    let snapshot = PassportVaultContractStateSnapshot {
        serialized_contract_state,
        authentication: PassportVaultContractStateAuthentication::IndexerSuppliedNotProven,
        contract_address_hex,
        transaction_hash_hex,
        action_block_hash_hex,
        action_block_height,
        finalized_head_hash_hex,
        finalized_head_height,
    };
    let context = PassportVaultCallChainContext::from_snapshot(
        &snapshot,
        zswap_chain_state,
        ledger_parameters,
    )?;
    Ok((snapshot, context))
}

fn decode_bounded_hex_state(value: &str) -> Result<Vec<u8>, PassportVaultContractStateSourceError> {
    decode_bounded_hex(value, MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES)
}

fn decode_bounded_hex(
    value: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, PassportVaultContractStateSourceError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.is_empty() || !value.len().is_multiple_of(2) {
        return Err(PassportVaultContractStateSourceError::InvalidResponse);
    }
    if value.len() > max_bytes * 2 {
        return Err(PassportVaultContractStateSourceError::CapacityExceeded);
    }
    hex::decode(value).map_err(|_| PassportVaultContractStateSourceError::InvalidResponse)
}

fn normalize_hex_32(value: &str) -> Option<String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn validate_http_endpoint(value: &str) -> Option<Url> {
    let endpoint = validate_endpoint(value)?;
    match endpoint.scheme() {
        "https" => Some(endpoint),
        "http" if host_is_loopback(&endpoint) => Some(endpoint),
        _ => None,
    }
}

pub(super) fn validate_node_endpoint(value: &str) -> Option<String> {
    let endpoint = validate_endpoint(value)?;
    match endpoint.scheme() {
        "wss" => Some(endpoint.to_string()),
        "ws" if host_is_loopback(&endpoint) => Some(endpoint.to_string()),
        _ => None,
    }
}

fn validate_endpoint(value: &str) -> Option<Url> {
    if value.is_empty()
        || value.chars().count() > MAX_ENDPOINT_CHARACTERS
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
    {
        return None;
    }
    let endpoint = Url::parse(value).ok()?;
    if endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return None;
    }
    Some(endpoint)
}

fn host_is_loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}

pub(super) fn ensure_tls_provider() -> Result<(), ()> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    rustls::crypto::CryptoProvider::get_default()
        .map(|_| ())
        .ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_explicit_secure_or_loopback_routes() {
        assert!(validate_http_endpoint("https://indexer.example/graphql").is_some());
        assert!(validate_http_endpoint("http://127.0.0.1:8088/api/v1/graphql").is_some());
        assert!(validate_http_endpoint("http://indexer.example/graphql").is_none());
        assert!(validate_node_endpoint("wss://node.example").is_some());
        assert!(validate_node_endpoint("ws://localhost:9944").is_some());
        assert!(validate_node_endpoint("ws://node.example").is_none());
        assert!(validate_node_endpoint("wss://user@node.example").is_none());
    }

    #[test]
    fn decodes_a_bounded_snapshot_without_claiming_state_authentication() {
        let address = "11".repeat(32);
        let tx_hash = "22".repeat(32);
        let block_hash = "33".repeat(32);
        let finalized_hash = "44".repeat(32);
        let response = serde_json::to_vec(&json!({
            "data": {
                "block": {
                    "hash": finalized_hash,
                    "height": 42,
                    "ledgerParameters": "0506"
                },
                "contractAction": {
                    "address": format!("0x{address}"),
                    "state": "0102",
                    "zswapState": "0304",
                    "transaction": {
                        "hash": tx_hash,
                        "block": { "hash": block_hash, "height": 41 }
                    }
                }
            }
        }))
        .expect("response");
        let (snapshot, context) =
            decode_indexer_response(&response, &address, finalized_hash.clone(), 42)
                .expect("snapshot");
        assert_eq!(snapshot.serialized_contract_state, [1, 2]);
        assert_eq!(snapshot.action_block_height, 41);
        assert_eq!(snapshot.finalized_head_hash_hex, finalized_hash);
        assert_eq!(context.zswap_chain_state(), [3, 4]);
        assert_eq!(context.ledger_parameters(), [5, 6]);
        assert!(context.matches_snapshot(&snapshot));
    }

    #[test]
    fn rejects_wrong_addresses_future_blocks_and_graphql_errors() {
        let address = "11".repeat(32);
        let response = |returned_address: String, height: i64| {
            serde_json::to_vec(&json!({
                "data": {
                    "block": {
                        "hash": "44".repeat(32),
                        "height": 5,
                        "ledgerParameters": "05"
                    },
                    "contractAction": {
                        "address": returned_address,
                        "state": "00",
                        "zswapState": "03",
                        "transaction": {
                            "hash": "22".repeat(32),
                            "block": { "hash": "33".repeat(32), "height": height }
                        }
                    }
                }
            }))
            .expect("response")
        };
        assert_eq!(
            decode_indexer_response(&response("55".repeat(32), 4), &address, "44".repeat(32), 5),
            Err(PassportVaultContractStateSourceError::InvalidResponse)
        );
        assert_eq!(
            decode_indexer_response(&response(address.clone(), 6), &address, "44".repeat(32), 5),
            Err(PassportVaultContractStateSourceError::FinalityMismatch)
        );
        let errors = serde_json::to_vec(&json!({
            "data": null,
            "errors": [{ "message": "redacted by adapter" }]
        }))
        .expect("errors");
        assert_eq!(
            decode_indexer_response(&errors, &address, "44".repeat(32), 5),
            Err(PassportVaultContractStateSourceError::InvalidResponse)
        );
    }
}
