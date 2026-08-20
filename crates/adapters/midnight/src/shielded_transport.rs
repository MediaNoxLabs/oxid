// SPDX-License-Identifier: Apache-2.0

//! Bounded native GraphQL transport for the chain-wide Zswap event stream.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use futures::{SinkExt as _, StreamExt as _};
use midnight_ledger::events::Event;
use midnight_storage::DefaultDB;
use midnight_zswap::{keys::SecretKeys, local::State as ZswapState};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest as _, protocol::WebSocketConfig},
};

use crate::{
    shielded::{
        DecodedZswapEvent, ZSWAP_LEDGER_EVENTS_QUERY, decode_zswap_event, replay_zswap_events,
    },
    shielded_checkpoint::StoredShieldedCheckpoint,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const ACK_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(5);
const RECEIVE_SEGMENT_QUIET_TIMEOUT: Duration = Duration::from_secs(1);
const SYNCHRONIZATION_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_FRAME_BYTES: usize = 256 * 1024;
const MAX_REPLAY_BATCH_EVENTS: usize = 256;
const MAX_REPLAY_BATCH_BYTES: usize = 4 * 1024 * 1024;
const MAX_RECEIVE_SEGMENT_EVENTS: usize = 16 * 1024;
const RECEIVE_SEGMENT_WIRE_BATCHES: usize = 4;
const MAX_RECEIVE_SEGMENT_BYTES: usize = MAX_REPLAY_BATCH_BYTES * RECEIVE_SEGMENT_WIRE_BATCHES;
const MAX_EVENTS: usize = 1_000_000;
const MAX_TOTAL_BYTES: usize = 512 * 1024 * 1024;
const SUBSCRIPTION_ID: &str = "oxid-shielded";
const PROTOCOL_IDENTITY: &[u8] = b"graphql-transport-ws\0oxid-shielded-v2\0";

#[derive(Clone)]
pub(crate) struct ShieldedSyncProgress {
    pub(crate) state: ZswapState<DefaultDB>,
    pub(crate) current_cursor: u64,
    pub(crate) target_cursor: u64,
    pub(crate) events_processed: usize,
}

pub(crate) struct ShieldedSynchronization {
    pub(crate) state: ZswapState<DefaultDB>,
    pub(crate) current_cursor: u64,
    pub(crate) target_cursor: u64,
    pub(crate) events_processed: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShieldedTransportError {
    Cancelled,
    Timeout,
    Unavailable,
    Storage,
    InvalidData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShieldedReceiveOutcome {
    Saturated,
    TargetReached,
    IdleCheckpoint,
    EmptyCheckpoint,
}

pub(crate) fn source_fingerprint(endpoint: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(PROTOCOL_IDENTITY);
    digest.update(endpoint.as_bytes());
    digest.update([0]);
    digest.update(ZSWAP_LEDGER_EVENTS_QUERY.as_bytes());
    digest.finalize().into()
}

pub(crate) async fn synchronize_shielded_with_control(
    endpoint: &str,
    keys: &SecretKeys,
    checkpoint: Option<StoredShieldedCheckpoint>,
    cancellation: &AtomicBool,
    observe: &mut dyn FnMut(&ShieldedSyncProgress) -> Result<(), ShieldedTransportError>,
) -> Result<ShieldedSynchronization, ShieldedTransportError> {
    ensure_active(cancellation)?;
    let (mut state, starting_cursor, starting_target) = checkpoint.map_or_else(
        || (ZswapState::new(), None, None),
        |checkpoint| {
            (
                checkpoint.state,
                Some(checkpoint.current_cursor),
                Some(checkpoint.target_cursor),
            )
        },
    );
    ensure_tls_provider()?;
    let synchronization = timeout(SYNCHRONIZATION_TIMEOUT, async {
        let mut durable_cursor = starting_cursor;
        let mut target_cursor = starting_target;
        let mut total_bytes = 0_usize;
        let mut event_count = 0_usize;
        let mut replayed_events = 0_usize;
        loop {
            ensure_active(cancellation)?;
            let starting_id = match durable_cursor {
                Some(cursor) => cursor
                    .checked_add(1)
                    .ok_or(ShieldedTransportError::InvalidData)?,
                None => 0,
            };
            let starting_id =
                i64::try_from(starting_id).map_err(|_| ShieldedTransportError::InvalidData)?;
            let mut request = endpoint
                .into_client_request()
                .map_err(|_| ShieldedTransportError::Unavailable)?;
            request.headers_mut().insert(
                "Sec-WebSocket-Protocol",
                "graphql-transport-ws"
                    .parse()
                    .map_err(|_| ShieldedTransportError::InvalidData)?,
            );
            let mut websocket_config = WebSocketConfig::default();
            websocket_config.max_message_size = Some(MAX_MESSAGE_BYTES);
            websocket_config.max_frame_size = Some(MAX_FRAME_BYTES);
            let connected = timeout(
                CONNECT_TIMEOUT,
                connect_async_with_config(request, Some(websocket_config), false),
            )
            .await;
            ensure_active(cancellation)?;
            let (mut socket, response) = connected
                .map_err(|_| ShieldedTransportError::Timeout)?
                .map_err(|_| ShieldedTransportError::Unavailable)?;
            if response
                .headers()
                .get("Sec-WebSocket-Protocol")
                .and_then(|value| value.to_str().ok())
                != Some("graphql-transport-ws")
            {
                return Err(ShieldedTransportError::InvalidData);
            }
            send_json(
                &mut socket,
                json!({ "type": "connection_init", "payload": {} }),
            )
            .await?;
            ensure_active(cancellation)?;
            wait_for_ack(&mut socket).await?;
            ensure_active(cancellation)?;
            send_json(
                &mut socket,
                json!({
                    "type": "subscribe",
                    "id": SUBSCRIPTION_ID,
                    "payload": {
                        "query": ZSWAP_LEDGER_EVENTS_QUERY,
                        "variables": { "id": starting_id }
                    }
                }),
            )
            .await?;
            ensure_active(cancellation)?;

            let mut segment = Vec::<DecodedZswapEvent>::with_capacity(MAX_RECEIVE_SEGMENT_EVENTS);
            let mut segment_bytes = 0_usize;
            let mut received_cursor = durable_cursor;
            let outcome = loop {
                ensure_active(cancellation)?;
                let receive_timeout = if segment.len() >= MAX_REPLAY_BATCH_EVENTS {
                    RECEIVE_SEGMENT_QUIET_TIMEOUT
                } else {
                    IDLE_TIMEOUT
                };
                let received = timeout(receive_timeout, socket.next()).await;
                ensure_active(cancellation)?;
                let message = match received {
                    Ok(Some(message)) => {
                        message.map_err(|_| ShieldedTransportError::InvalidData)?
                    }
                    Err(_) if segment.is_empty() && durable_cursor.is_some() => {
                        break ShieldedReceiveOutcome::IdleCheckpoint;
                    }
                    Err(_) if !segment.is_empty() => break ShieldedReceiveOutcome::Saturated,
                    Ok(None) => return Err(ShieldedTransportError::InvalidData),
                    Err(_) => return Err(ShieldedTransportError::Timeout),
                };
                match message {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(text.as_str())
                            .map_err(|_| ShieldedTransportError::InvalidData)?;
                        match message_type(&value)? {
                            "next" => {
                                if value.get("id").and_then(Value::as_str) != Some(SUBSCRIPTION_ID)
                                {
                                    return Err(ShieldedTransportError::InvalidData);
                                }
                                let payload = value
                                    .get("payload")
                                    .ok_or(ShieldedTransportError::InvalidData)?;
                                if payload
                                    .get("errors")
                                    .and_then(Value::as_array)
                                    .is_some_and(|errors| !errors.is_empty())
                                {
                                    return Err(ShieldedTransportError::InvalidData);
                                }
                                let data = payload
                                    .get("data")
                                    .ok_or(ShieldedTransportError::InvalidData)?;
                                let decoded = decode_zswap_event(data)
                                    .map_err(|_| ShieldedTransportError::InvalidData)?;
                                let sequence_valid = match received_cursor {
                                    // Zswap IDs are sparse global indexer cursors:
                                    // unrelated ledger activity can create gaps.
                                    // They must still move strictly forward.
                                    Some(last) => decoded.cursor > last,
                                    None => true,
                                };
                                if !sequence_valid
                                    || decoded.cursor > decoded.target_cursor
                                    || target_cursor
                                        .is_some_and(|target| decoded.target_cursor < target)
                                {
                                    return Err(ShieldedTransportError::InvalidData);
                                }
                                target_cursor = Some(decoded.target_cursor);
                                let next_segment_bytes = segment_bytes
                                    .checked_add(decoded.raw_bytes)
                                    .ok_or(ShieldedTransportError::InvalidData)?;
                                if !segment.is_empty()
                                    && next_segment_bytes > MAX_RECEIVE_SEGMENT_BYTES
                                {
                                    break ShieldedReceiveOutcome::Saturated;
                                }
                                event_count = event_count
                                    .checked_add(1)
                                    .ok_or(ShieldedTransportError::InvalidData)?;
                                total_bytes = total_bytes
                                    .checked_add(decoded.raw_bytes)
                                    .ok_or(ShieldedTransportError::InvalidData)?;
                                if event_count > MAX_EVENTS || total_bytes > MAX_TOTAL_BYTES {
                                    return Err(ShieldedTransportError::InvalidData);
                                }
                                segment_bytes = next_segment_bytes;
                                received_cursor = Some(decoded.cursor);
                                let reached_target = decoded.cursor == decoded.target_cursor;
                                segment.push(decoded);
                                if reached_target {
                                    break ShieldedReceiveOutcome::TargetReached;
                                }
                                if segment.len() == MAX_RECEIVE_SEGMENT_EVENTS {
                                    break ShieldedReceiveOutcome::Saturated;
                                }
                            }
                            "ping" => {
                                let mut pong = json!({ "type": "pong" });
                                if let Some(payload) = value.get("payload") {
                                    pong["payload"] = payload.clone();
                                }
                                send_json(&mut socket, pong).await?;
                            }
                            "pong" => {}
                            "complete"
                                if value.get("id").and_then(Value::as_str)
                                    == Some(SUBSCRIPTION_ID)
                                    && segment.is_empty()
                                    && durable_cursor.is_some() =>
                            {
                                break ShieldedReceiveOutcome::EmptyCheckpoint;
                            }
                            _ => return Err(ShieldedTransportError::InvalidData),
                        }
                    }
                    Message::Ping(payload) => socket
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|_| ShieldedTransportError::Unavailable)?,
                    Message::Pong(_) => {}
                    _ => return Err(ShieldedTransportError::InvalidData),
                }
            };

            // The indexer must be free to continue producing while official
            // ledger replay and durable checkpoint observation run. Close the
            // bounded receive segment before either operation begins.
            let _ = timeout(
                IDLE_TIMEOUT,
                send_json(
                    &mut socket,
                    json!({ "type": "complete", "id": SUBSCRIPTION_ID }),
                ),
            )
            .await;
            drop(socket);

            if segment.is_empty() {
                let current_cursor = durable_cursor.ok_or(ShieldedTransportError::InvalidData)?;
                let target_cursor = target_cursor.ok_or(ShieldedTransportError::InvalidData)?;
                if current_cursor != target_cursor {
                    return Err(if outcome == ShieldedReceiveOutcome::IdleCheckpoint {
                        ShieldedTransportError::Timeout
                    } else {
                        ShieldedTransportError::InvalidData
                    });
                }
                observe(&ShieldedSyncProgress {
                    state: state.clone(),
                    current_cursor,
                    target_cursor,
                    events_processed: 0,
                })?;
                ensure_active(cancellation)?;
                return Ok(ShieldedSynchronization {
                    state,
                    current_cursor,
                    target_cursor,
                    events_processed: replayed_events,
                });
            }

            let target = target_cursor.ok_or(ShieldedTransportError::InvalidData)?;
            let segment_cursor = received_cursor.ok_or(ShieldedTransportError::InvalidData)?;
            let mut batch = Vec::<Event<DefaultDB>>::with_capacity(MAX_REPLAY_BATCH_EVENTS);
            let mut batch_bytes = 0_usize;
            let mut batch_last_cursor = None;
            for decoded in segment {
                let next_batch_bytes = batch_bytes
                    .checked_add(decoded.raw_bytes)
                    .ok_or(ShieldedTransportError::InvalidData)?;
                if !batch.is_empty() && next_batch_bytes > MAX_REPLAY_BATCH_BYTES {
                    let batch_cursor =
                        batch_last_cursor.ok_or(ShieldedTransportError::InvalidData)?;
                    flush_batch(
                        keys,
                        cancellation,
                        &mut state,
                        &mut batch,
                        batch_cursor,
                        target,
                        &mut replayed_events,
                        observe,
                    )?;
                    durable_cursor = Some(batch_cursor);
                    batch_bytes = 0;
                }
                batch_bytes = batch_bytes
                    .checked_add(decoded.raw_bytes)
                    .ok_or(ShieldedTransportError::InvalidData)?;
                batch_last_cursor = Some(decoded.cursor);
                batch.push(decoded.event);
                if batch.len() == MAX_REPLAY_BATCH_EVENTS {
                    let batch_cursor =
                        batch_last_cursor.ok_or(ShieldedTransportError::InvalidData)?;
                    flush_batch(
                        keys,
                        cancellation,
                        &mut state,
                        &mut batch,
                        batch_cursor,
                        target,
                        &mut replayed_events,
                        observe,
                    )?;
                    durable_cursor = Some(batch_cursor);
                    batch_bytes = 0;
                }
            }
            if !batch.is_empty() {
                let batch_cursor = batch_last_cursor.ok_or(ShieldedTransportError::InvalidData)?;
                flush_batch(
                    keys,
                    cancellation,
                    &mut state,
                    &mut batch,
                    batch_cursor,
                    target,
                    &mut replayed_events,
                    observe,
                )?;
                durable_cursor = Some(batch_cursor);
            }
            if durable_cursor != Some(segment_cursor) {
                return Err(ShieldedTransportError::InvalidData);
            }
            ensure_active(cancellation)?;
            if outcome == ShieldedReceiveOutcome::TargetReached {
                if segment_cursor != target {
                    return Err(ShieldedTransportError::InvalidData);
                }
                return Ok(ShieldedSynchronization {
                    state,
                    current_cursor: segment_cursor,
                    target_cursor: target,
                    events_processed: replayed_events,
                });
            }
        }
    })
    .await
    .map_err(|_| ShieldedTransportError::Timeout)??;
    Ok(synchronization)
}

#[allow(clippy::too_many_arguments)]
fn flush_batch(
    keys: &SecretKeys,
    cancellation: &AtomicBool,
    state: &mut ZswapState<DefaultDB>,
    batch: &mut Vec<Event<DefaultDB>>,
    current_cursor: u64,
    target_cursor: u64,
    replayed_events: &mut usize,
    observe: &mut dyn FnMut(&ShieldedSyncProgress) -> Result<(), ShieldedTransportError>,
) -> Result<(), ShieldedTransportError> {
    ensure_active(cancellation)?;
    *state = replay_zswap_events(keys, state.clone(), batch.iter())
        .map_err(|_| ShieldedTransportError::InvalidData)?;
    *replayed_events = replayed_events
        .checked_add(batch.len())
        .ok_or(ShieldedTransportError::InvalidData)?;
    batch.clear();
    observe(&ShieldedSyncProgress {
        state: state.clone(),
        current_cursor,
        target_cursor,
        events_processed: *replayed_events,
    })
}

fn ensure_active(cancellation: &AtomicBool) -> Result<(), ShieldedTransportError> {
    if cancellation.load(Ordering::Acquire) {
        Err(ShieldedTransportError::Cancelled)
    } else {
        Ok(())
    }
}

fn ensure_tls_provider() -> Result<(), ShieldedTransportError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    rustls::crypto::CryptoProvider::get_default()
        .map(|_| ())
        .ok_or(ShieldedTransportError::Unavailable)
}

async fn send_json<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    value: Value,
) -> Result<(), ShieldedTransportError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .map_err(|_| ShieldedTransportError::Unavailable)
}

async fn wait_for_ack<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Result<(), ShieldedTransportError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    timeout(ACK_TIMEOUT, async {
        loop {
            let message = socket
                .next()
                .await
                .ok_or(ShieldedTransportError::InvalidData)?
                .map_err(|_| ShieldedTransportError::InvalidData)?;
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(text.as_str())
                        .map_err(|_| ShieldedTransportError::InvalidData)?;
                    match message_type(&value)? {
                        "connection_ack" => return Ok(()),
                        "ping" => {
                            let mut pong = json!({ "type": "pong" });
                            if let Some(payload) = value.get("payload") {
                                pong["payload"] = payload.clone();
                            }
                            send_json(socket, pong).await?;
                        }
                        _ => return Err(ShieldedTransportError::InvalidData),
                    }
                }
                Message::Ping(payload) => socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|_| ShieldedTransportError::Unavailable)?,
                Message::Pong(_) => {}
                _ => return Err(ShieldedTransportError::InvalidData),
            }
        }
    })
    .await
    .map_err(|_| ShieldedTransportError::Timeout)?
}

fn message_type(value: &Value) -> Result<&str, ShieldedTransportError> {
    value
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ShieldedTransportError::InvalidData)
}

#[cfg(test)]
mod tests {
    use std::{
        net::TcpListener,
        sync::{Arc, Condvar, Mutex},
        thread,
    };

    use midnight_coin_structure::{
        coin::{Info as CoinInfo, ShieldedTokenType},
        transfer::Recipient,
    };
    use midnight_ledger::{
        events::{EventDetails, EventSource, ZswapPreimageEvidence},
        structure::TransactionHash,
    };
    use midnight_zswap::keys::Seed;
    use rand::{Rng as _, SeedableRng as _, rngs::StdRng};
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::handshake::server::{Request, Response},
    };

    use crate::shielded::project_zswap_state;

    use super::*;

    fn output(keys: &SecretKeys, value: u128, index: u64) -> Event<DefaultDB> {
        let mut rng = StdRng::seed_from_u64(index + 31);
        let coin = CoinInfo {
            nonce: rng.r#gen(),
            type_: ShieldedTokenType::default(),
            value,
        };
        Event {
            source: EventSource {
                transaction_hash: TransactionHash::default(),
                logical_segment: 0,
                physical_segment: 0,
            },
            content: EventDetails::ZswapOutput {
                commitment: coin.commitment(&Recipient::User(keys.coin_public_key())),
                preimage_evidence: ZswapPreimageEvidence::PublicPreimage {
                    coin,
                    recipient: Recipient::User(keys.coin_public_key()),
                },
                contract: None,
                mt_index: index,
            },
        }
    }

    fn event_value(event: &Event<DefaultDB>, id: u64, max_id: u64) -> Value {
        let mut raw = Vec::new();
        midnight_serialize::tagged_serialize(event, &mut raw).expect("event serializes");
        json!({
            "type": "next",
            "id": SUBSCRIPTION_ID,
            "payload": {
                "data": {
                    "zswapLedgerEvents": {
                        "__typename": "ZswapLedgerEvent",
                        "id": id,
                        "maxId": max_id,
                        "raw": format!("0x{}", hex::encode(raw))
                    }
                }
            }
        })
    }

    // Tungstenite fixes the handshake callback's error to a large HTTP response.
    #[allow(clippy::result_large_err)]
    fn server(expected_start: i64, events: Vec<Value>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback binds");
        listener
            .set_nonblocking(true)
            .expect("listener becomes nonblocking");
        let address = listener.local_addr().expect("address exists");
        let handle = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime builds");
            runtime.block_on(async move {
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
                            "graphql-transport-ws".parse().expect("header is valid"),
                        );
                        Ok(response)
                    })
                    .await
                    .expect("WebSocket accepts");
                let init = socket
                    .next()
                    .await
                    .expect("init exists")
                    .expect("init reads");
                assert_eq!(
                    serde_json::from_str::<Value>(init.into_text().expect("text").as_str())
                        .expect("init JSON")["type"],
                    "connection_init"
                );
                socket
                    .send(Message::Text(
                        json!({ "type": "connection_ack" }).to_string().into(),
                    ))
                    .await
                    .expect("ack sends");
                let subscribe = socket
                    .next()
                    .await
                    .expect("subscribe exists")
                    .expect("subscribe reads");
                let subscribe: Value =
                    serde_json::from_str(subscribe.into_text().expect("text").as_str())
                        .expect("subscribe JSON");
                assert_eq!(subscribe["payload"]["query"], ZSWAP_LEDGER_EVENTS_QUERY);
                assert_eq!(subscribe["payload"]["variables"]["id"], expected_start);
                for event in events {
                    socket
                        .send(Message::Text(event.to_string().into()))
                        .await
                        .expect("event sends");
                }
                socket
                    .send(Message::Text(
                        json!({ "type": "complete", "id": SUBSCRIPTION_ID })
                            .to_string()
                            .into(),
                    ))
                    .await
                    .expect("completion sends");
                let _ = socket.next().await;
            });
        });
        (format!("ws://{address}"), handle)
    }

    #[allow(clippy::result_large_err)]
    fn segmented_server(
        scenarios: Vec<(i64, Vec<Value>)>,
        first_segment_closed: Arc<(Mutex<bool>, Condvar)>,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback binds");
        listener
            .set_nonblocking(true)
            .expect("listener becomes nonblocking");
        let address = listener.local_addr().expect("address exists");
        let handle = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime builds");
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("Tokio listener accepts sockets");
                for (scenario_index, (expected_start, events)) in scenarios.into_iter().enumerate()
                {
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
                        .expect("WebSocket accepts");
                    let _ = socket.next().await.expect("init exists");
                    socket
                        .send(Message::Text(
                            json!({ "type": "connection_ack" }).to_string().into(),
                        ))
                        .await
                        .expect("ack sends");
                    let subscribe = socket
                        .next()
                        .await
                        .expect("subscribe exists")
                        .expect("subscribe reads");
                    let subscribe: Value = serde_json::from_str(
                        subscribe.into_text().expect("subscribe is text").as_str(),
                    )
                    .expect("subscribe is JSON");
                    assert_eq!(subscribe["payload"]["variables"]["id"], expected_start);
                    for event in events {
                        socket
                            .send(Message::Text(event.to_string().into()))
                            .await
                            .expect("event sends");
                    }
                    while let Some(message) = socket.next().await {
                        let Ok(message) = message else {
                            break;
                        };
                        if message.to_text().is_ok_and(|text| {
                            serde_json::from_str::<Value>(text).is_ok_and(|value| {
                                value.get("type").and_then(Value::as_str) == Some("complete")
                                    && value.get("id").and_then(Value::as_str)
                                        == Some(SUBSCRIPTION_ID)
                            })
                        }) {
                            if scenario_index == 0 {
                                let (closed, ready) = &*first_segment_closed;
                                *closed.lock().expect("close signal locks") = true;
                                ready.notify_all();
                            }
                            break;
                        }
                    }
                }
            });
        });
        (format!("ws://{address}"), handle)
    }

    #[test]
    fn bounded_transport_accepts_sparse_cursors_and_observes_consistent_batches() {
        let keys = SecretKeys::from(Seed::from([7; 32]));
        let (endpoint, server) = server(
            0,
            vec![
                event_value(&output(&keys, 5, 0), 2, 9),
                event_value(&output(&keys, 7, 1), 9, 9),
            ],
        );
        let cancellation = AtomicBool::new(false);
        let mut observed = Vec::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime builds");
        let synchronized = runtime
            .block_on(synchronize_shielded_with_control(
                &endpoint,
                &keys,
                None,
                &cancellation,
                &mut |progress| {
                    observed.push((progress.current_cursor, progress.target_cursor));
                    Ok(())
                },
            ))
            .expect("live replay succeeds");
        server.join().expect("server exits");
        assert_eq!(
            (synchronized.current_cursor, synchronized.target_cursor),
            (9, 9)
        );
        assert_eq!(synchronized.events_processed, 2);
        assert_eq!(observed, vec![(9, 9)]);
        let projection = project_zswap_state(&synchronized.state).expect("state projects");
        assert_eq!(projection.owned_note_count, 2);
        assert_eq!(projection.commitment_count, 2);
        assert_eq!(projection.balances[0].atomic_units, 12);
    }

    #[test]
    fn bounded_transport_closes_before_observation_and_resumes_from_durable_cursor() {
        let keys = SecretKeys::from(Seed::from([17; 32]));
        let first_segment_closed = Arc::new((Mutex::new(false), Condvar::new()));
        let first = (0_u64..MAX_REPLAY_BATCH_EVENTS as u64)
            .map(|cursor| {
                event_value(
                    &output(&keys, 1, cursor),
                    cursor,
                    MAX_REPLAY_BATCH_EVENTS as u64,
                )
            })
            .collect();
        let final_cursor = MAX_REPLAY_BATCH_EVENTS as u64;
        let (endpoint, server) = segmented_server(
            vec![
                (0, first),
                (
                    i64::try_from(final_cursor).expect("cursor fits"),
                    vec![event_value(
                        &output(&keys, 1, final_cursor),
                        final_cursor,
                        final_cursor,
                    )],
                ),
            ],
            Arc::clone(&first_segment_closed),
        );
        let cancellation = AtomicBool::new(false);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime builds");
        let mut observed = Vec::new();
        let synchronized = runtime
            .block_on(synchronize_shielded_with_control(
                &endpoint,
                &keys,
                None,
                &cancellation,
                &mut |progress| {
                    if observed.is_empty() {
                        let (closed, ready) = &*first_segment_closed;
                        let (closed, wait) = ready
                            .wait_timeout_while(
                                closed.lock().expect("close signal locks"),
                                Duration::from_secs(2),
                                |closed| !*closed,
                            )
                            .expect("close signal waits");
                        if wait.timed_out() || !*closed {
                            return Err(ShieldedTransportError::Unavailable);
                        }
                    }
                    observed.push((
                        progress.current_cursor,
                        progress.target_cursor,
                        progress.events_processed,
                    ));
                    Ok(())
                },
            ))
            .expect("bounded replay follows the closed subscription");
        server.join().expect("both subscriptions complete");

        assert_eq!(
            observed,
            vec![
                (
                    MAX_REPLAY_BATCH_EVENTS as u64 - 1,
                    final_cursor,
                    MAX_REPLAY_BATCH_EVENTS,
                ),
                (final_cursor, final_cursor, MAX_REPLAY_BATCH_EVENTS + 1),
            ]
        );
        assert_eq!(synchronized.current_cursor, final_cursor);
        assert_eq!(synchronized.target_cursor, final_cursor);
        assert_eq!(synchronized.events_processed, MAX_REPLAY_BATCH_EVENTS + 1);
        assert_eq!(
            project_zswap_state(&synchronized.state)
                .expect("state projects")
                .owned_note_count,
            (MAX_REPLAY_BATCH_EVENTS + 1) as u64
        );
    }

    #[test]
    fn bounded_transport_rejects_a_target_regression_after_reconnect() {
        let keys = SecretKeys::from(Seed::from([19; 32]));
        let first_segment_closed = Arc::new((Mutex::new(false), Condvar::new()));
        let first_target = MAX_REPLAY_BATCH_EVENTS as u64 + 100;
        let first = (0_u64..MAX_REPLAY_BATCH_EVENTS as u64)
            .map(|cursor| event_value(&output(&keys, 1, cursor), cursor, first_target))
            .collect();
        let next_cursor = MAX_REPLAY_BATCH_EVENTS as u64;
        let (endpoint, server) = segmented_server(
            vec![
                (0, first),
                (
                    i64::try_from(next_cursor).expect("cursor fits"),
                    vec![event_value(
                        &output(&keys, 1, next_cursor),
                        next_cursor,
                        first_target - 1,
                    )],
                ),
            ],
            first_segment_closed,
        );
        let cancellation = AtomicBool::new(false);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime builds");
        let mut observations = 0_usize;
        let result = runtime.block_on(synchronize_shielded_with_control(
            &endpoint,
            &keys,
            None,
            &cancellation,
            &mut |_| {
                observations += 1;
                Ok(())
            },
        ));
        server.join().expect("both subscriptions complete");

        assert_eq!(result.err(), Some(ShieldedTransportError::InvalidData));
        assert_eq!(observations, 1, "the first segment became durable");
    }

    #[test]
    fn observer_error_after_checkpoint_progress_is_returned_without_resubscribe() {
        let keys = SecretKeys::from(Seed::from([23; 32]));
        let checkpoint = StoredShieldedCheckpoint {
            current_cursor: 0,
            target_cursor: 0,
            updated_at: oxid_foundation::UnixTimestampMillis::new(12),
            state: ZswapState::new(),
        };
        let (endpoint, server) = server(1, vec![event_value(&output(&keys, 1, 0), 1, 1)]);
        let cancellation = AtomicBool::new(false);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime builds");
        let mut observations = 0_usize;
        let result = runtime.block_on(synchronize_shielded_with_control(
            &endpoint,
            &keys,
            Some(checkpoint),
            &cancellation,
            &mut |_| {
                observations += 1;
                Err(ShieldedTransportError::InvalidData)
            },
        ));
        server.join().expect("single subscription completes");

        assert_eq!(result.err(), Some(ShieldedTransportError::InvalidData));
        assert_eq!(observations, 1);
    }

    #[test]
    fn bounded_transport_rejects_duplicate_backward_regressing_and_incomplete_cursors() {
        let keys = SecretKeys::from(Seed::from([13; 32]));
        let cases = [
            vec![
                event_value(&output(&keys, 5, 0), 2, 9),
                event_value(&output(&keys, 7, 1), 2, 9),
            ],
            vec![
                event_value(&output(&keys, 5, 0), 3, 9),
                event_value(&output(&keys, 7, 1), 2, 9),
            ],
            vec![
                event_value(&output(&keys, 5, 0), 2, 9),
                event_value(&output(&keys, 7, 1), 3, 8),
            ],
            vec![event_value(&output(&keys, 5, 0), 2, 9)],
        ];
        for events in cases {
            let (endpoint, server) = server(0, events);
            let cancellation = AtomicBool::new(false);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime builds");
            assert_eq!(
                runtime
                    .block_on(synchronize_shielded_with_control(
                        &endpoint,
                        &keys,
                        None,
                        &cancellation,
                        &mut |_| Ok(()),
                    ))
                    .err(),
                Some(ShieldedTransportError::InvalidData)
            );
            server.join().expect("invalid fixture exits");
        }
    }

    #[test]
    fn current_checkpoint_resumes_at_the_next_cursor_without_replaying_notes() {
        let keys = SecretKeys::from(Seed::from([11; 32]));
        let cancellation = AtomicBool::new(false);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime builds");
        let (first_endpoint, first_server) = server(
            0,
            vec![
                event_value(&output(&keys, 5, 0), 0, 1),
                event_value(&output(&keys, 7, 1), 1, 1),
            ],
        );
        let first = runtime
            .block_on(synchronize_shielded_with_control(
                &first_endpoint,
                &keys,
                None,
                &cancellation,
                &mut |_| Ok(()),
            ))
            .expect("initial live replay succeeds");
        first_server.join().expect("initial server exits");

        let checkpoint = StoredShieldedCheckpoint {
            current_cursor: first.current_cursor,
            target_cursor: first.target_cursor,
            updated_at: oxid_foundation::UnixTimestampMillis::new(12),
            state: first.state,
        };
        let (resume_endpoint, resume_server) = server(2, Vec::new());
        let mut observations = 0;
        let resumed = runtime
            .block_on(synchronize_shielded_with_control(
                &resume_endpoint,
                &keys,
                Some(checkpoint),
                &cancellation,
                &mut |_| {
                    observations += 1;
                    Ok(())
                },
            ))
            .expect("current checkpoint is accepted after bounded idle");
        resume_server.join().expect("resume server exits");

        assert_eq!((resumed.current_cursor, resumed.target_cursor), (1, 1));
        assert_eq!(resumed.events_processed, 0);
        assert_eq!(observations, 1);
        let projection = project_zswap_state(&resumed.state).expect("resumed state projects");
        assert_eq!(projection.owned_note_count, 2);
        assert_eq!(projection.commitment_count, 2);
        assert_eq!(projection.balances[0].atomic_units, 12);
    }

    #[test]
    fn fingerprint_binds_endpoint_query_and_protocol_and_cancellation_is_preflight() {
        assert_eq!(
            source_fingerprint("ws://127.0.0.1:1"),
            source_fingerprint("ws://127.0.0.1:1")
        );
        assert_ne!(
            source_fingerprint("ws://127.0.0.1:1"),
            source_fingerprint("ws://127.0.0.1:2")
        );
        let mut legacy = Sha256::new();
        legacy.update(b"graphql-transport-ws\0oxid-shielded-v1\0");
        legacy.update(b"ws://127.0.0.1:1");
        legacy.update([0]);
        legacy.update(ZSWAP_LEDGER_EVENTS_QUERY.as_bytes());
        assert_ne!(
            source_fingerprint("ws://127.0.0.1:1"),
            <[u8; 32]>::from(legacy.finalize()),
            "v1 checkpoints must replay under the corrected envelope and cursor contract"
        );
        let cancellation = AtomicBool::new(true);
        let keys = SecretKeys::from(Seed::from([9; 32]));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime builds");
        assert_eq!(
            runtime
                .block_on(synchronize_shielded_with_control(
                    "ws://127.0.0.1:1",
                    &keys,
                    None,
                    &cancellation,
                    &mut |_| Ok(()),
                ))
                .err(),
            Some(ShieldedTransportError::Cancelled)
        );
    }
}
