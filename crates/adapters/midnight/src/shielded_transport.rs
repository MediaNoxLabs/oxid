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
    shielded::{ZSWAP_LEDGER_EVENTS_QUERY, decode_zswap_event, replay_zswap_events},
    shielded_checkpoint::StoredShieldedCheckpoint,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const ACK_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(5);
const SYNCHRONIZATION_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_FRAME_BYTES: usize = 256 * 1024;
const MAX_REPLAY_BATCH_EVENTS: usize = 256;
const MAX_REPLAY_BATCH_BYTES: usize = 4 * 1024 * 1024;
const MAX_EVENTS: usize = 1_000_000;
const MAX_TOTAL_BYTES: usize = 512 * 1024 * 1024;
const SUBSCRIPTION_ID: &str = "oxid-shielded";
const PROTOCOL_IDENTITY: &[u8] = b"graphql-transport-ws\0oxid-shielded-v1\0";

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
    let started_with_checkpoint = checkpoint.is_some();
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
    let starting_id = starting_cursor.map_or(Ok(0), |cursor| {
        cursor
            .checked_add(1)
            .ok_or(ShieldedTransportError::InvalidData)
    })?;
    let starting_id =
        i64::try_from(starting_id).map_err(|_| ShieldedTransportError::InvalidData)?;

    ensure_tls_provider()?;
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
    let (mut socket, response) = timeout(
        CONNECT_TIMEOUT,
        connect_async_with_config(request, Some(websocket_config), false),
    )
    .await
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
    wait_for_ack(&mut socket).await?;
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

    let synchronization = timeout(SYNCHRONIZATION_TIMEOUT, async {
        let mut batch = Vec::<Event<DefaultDB>>::with_capacity(MAX_REPLAY_BATCH_EVENTS);
        let mut batch_bytes = 0_usize;
        let mut batch_last_cursor = None;
        let mut last_cursor = starting_cursor;
        let mut target_cursor = starting_target;
        let mut total_bytes = 0_usize;
        let mut event_count = 0_usize;
        let mut replayed_events = 0_usize;
        let mut saw_event = false;

        loop {
            ensure_active(cancellation)?;
            let message = match timeout(IDLE_TIMEOUT, socket.next()).await {
                Ok(Some(message)) => message.map_err(|_| ShieldedTransportError::InvalidData)?,
                Err(_) if started_with_checkpoint && !saw_event => break,
                Ok(None) => return Err(ShieldedTransportError::InvalidData),
                Err(_) => return Err(ShieldedTransportError::Timeout),
            };
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(text.as_str())
                        .map_err(|_| ShieldedTransportError::InvalidData)?;
                    match message_type(&value)? {
                        "next" => {
                            if value.get("id").and_then(Value::as_str) != Some(SUBSCRIPTION_ID) {
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
                            let expected_cursor = last_cursor.map_or(Ok(0), |cursor| {
                                cursor
                                    .checked_add(1)
                                    .ok_or(ShieldedTransportError::InvalidData)
                            })?;
                            if decoded.cursor != expected_cursor
                                || decoded.cursor > decoded.target_cursor
                                || target_cursor
                                    .is_some_and(|target| decoded.target_cursor < target)
                            {
                                return Err(ShieldedTransportError::InvalidData);
                            }
                            saw_event = true;
                            last_cursor = Some(decoded.cursor);
                            target_cursor = Some(decoded.target_cursor);
                            event_count = event_count
                                .checked_add(1)
                                .ok_or(ShieldedTransportError::InvalidData)?;
                            total_bytes = total_bytes
                                .checked_add(decoded.raw_bytes)
                                .ok_or(ShieldedTransportError::InvalidData)?;
                            if event_count > MAX_EVENTS || total_bytes > MAX_TOTAL_BYTES {
                                return Err(ShieldedTransportError::InvalidData);
                            }

                            if !batch.is_empty()
                                && batch_bytes
                                    .checked_add(decoded.raw_bytes)
                                    .is_none_or(|bytes| bytes > MAX_REPLAY_BATCH_BYTES)
                            {
                                let batch_cursor =
                                    batch_last_cursor.ok_or(ShieldedTransportError::InvalidData)?;
                                let target =
                                    target_cursor.ok_or(ShieldedTransportError::InvalidData)?;
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
                                batch_bytes = 0;
                            }
                            batch_bytes = batch_bytes
                                .checked_add(decoded.raw_bytes)
                                .ok_or(ShieldedTransportError::InvalidData)?;
                            batch.push(decoded.event);
                            batch_last_cursor = Some(decoded.cursor);
                            if batch.len() == MAX_REPLAY_BATCH_EVENTS
                                || decoded.cursor == decoded.target_cursor
                            {
                                let target =
                                    target_cursor.ok_or(ShieldedTransportError::InvalidData)?;
                                flush_batch(
                                    keys,
                                    cancellation,
                                    &mut state,
                                    &mut batch,
                                    decoded.cursor,
                                    target,
                                    &mut replayed_events,
                                    observe,
                                )?;
                                batch_bytes = 0;
                            }
                            if decoded.cursor == decoded.target_cursor {
                                break;
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
                            if value.get("id").and_then(Value::as_str) == Some(SUBSCRIPTION_ID)
                                && started_with_checkpoint
                                && !saw_event =>
                        {
                            break;
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

        if !batch.is_empty() {
            let current = batch_last_cursor.ok_or(ShieldedTransportError::InvalidData)?;
            let target = target_cursor.ok_or(ShieldedTransportError::InvalidData)?;
            flush_batch(
                keys,
                cancellation,
                &mut state,
                &mut batch,
                current,
                target,
                &mut replayed_events,
                observe,
            )?;
        }
        let current_cursor = last_cursor.ok_or(ShieldedTransportError::InvalidData)?;
        let target_cursor = target_cursor.ok_or(ShieldedTransportError::InvalidData)?;
        if current_cursor != target_cursor {
            return Err(ShieldedTransportError::InvalidData);
        }
        if !saw_event {
            observe(&ShieldedSyncProgress {
                state: state.clone(),
                current_cursor,
                target_cursor,
                events_processed: 0,
            })?;
        }
        Ok::<_, ShieldedTransportError>(ShieldedSynchronization {
            state,
            current_cursor,
            target_cursor,
            events_processed: replayed_events,
        })
    })
    .await
    .map_err(|_| ShieldedTransportError::Timeout)??;

    let _ = send_json(
        &mut socket,
        json!({ "type": "complete", "id": SUBSCRIPTION_ID }),
    )
    .await;
    let _ = socket.close(None).await;
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
    use std::{net::TcpListener, thread};

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
                        "__typename": "ZswapOutput",
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

    #[test]
    fn bounded_transport_negotiates_replays_and_observes_consistent_batches() {
        let keys = SecretKeys::from(Seed::from([7; 32]));
        let (endpoint, server) = server(
            0,
            vec![
                event_value(&output(&keys, 5, 0), 0, 1),
                event_value(&output(&keys, 7, 1), 1, 1),
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
            (1, 1)
        );
        assert_eq!(synchronized.events_processed, 2);
        assert_eq!(observed, vec![(1, 1)]);
        let projection = project_zswap_state(&synchronized.state).expect("state projects");
        assert_eq!(projection.owned_note_count, 2);
        assert_eq!(projection.commitment_count, 2);
        assert_eq!(projection.balances[0].atomic_units, 12);
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
