// SPDX-License-Identifier: Apache-2.0

//! Canonical, adapter-private Zswap event decoding and state replay.

use std::{borrow::Cow, collections::BTreeMap};

use midnight_coin_structure::{
    coin::Info as CoinInfo,
    transfer::{Recipient, SenderEvidence},
};
use midnight_ledger::events::{Event, EventDetails};
use midnight_storage::DefaultDB;
use midnight_zswap::{keys::SecretKeys, local::State as ZswapState};
use serde_json::Value;

pub(crate) const ZSWAP_LEDGER_EVENTS_QUERY: &str = r#"subscription ZswapLedgerEvents($id: Int) {
  zswapLedgerEvents(id: $id) {
    __typename
    id
    maxId
    raw
  }
}"#;

const MAX_RAW_EVENT_BYTES: usize = 64 * 1024;

/// One bounded event decoded from the standalone indexer protocol.
#[derive(Clone, Debug)]
pub(crate) struct DecodedZswapEvent {
    pub(crate) cursor: u64,
    pub(crate) target_cursor: u64,
    pub(crate) raw_bytes: usize,
    pub(crate) event: Event<DefaultDB>,
}

/// Safe, chain-neutral balance projection over adapter-private Zswap state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShieldedStateProjection {
    pub(crate) owned_note_count: u64,
    pub(crate) commitment_count: u64,
    pub(crate) balances: Vec<ShieldedTokenBalance>,
}

/// Exact atomic balance for one shielded token type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShieldedTokenBalance {
    pub(crate) token_type_hex: String,
    pub(crate) atomic_units: u128,
}

/// Sanitized event/replay failure; raw chain data is never rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShieldedReplayError {
    InvalidEnvelope,
    EventTooLarge,
    InvalidEvent,
    NonLinearCommitment,
    InvalidCommitmentTree,
    BalanceOverflow,
}

/// Decodes one `next.payload.data.zswapLedgerEvents` value.
pub(crate) fn decode_zswap_event(data: &Value) -> Result<DecodedZswapEvent, ShieldedReplayError> {
    let object = data
        .get("zswapLedgerEvents")
        .and_then(Value::as_object)
        .ok_or(ShieldedReplayError::InvalidEnvelope)?;
    let cursor = unsigned_field(object.get("id"))?;
    let target_cursor = unsigned_field(object.get("maxId"))?;
    if cursor > target_cursor {
        return Err(ShieldedReplayError::InvalidEnvelope);
    }
    let typename = object
        .get("__typename")
        .and_then(Value::as_str)
        .ok_or(ShieldedReplayError::InvalidEnvelope)?;
    if !matches!(typename, "ZswapInput" | "ZswapOutput") {
        return Err(ShieldedReplayError::InvalidEnvelope);
    }
    let raw = object
        .get("raw")
        .and_then(Value::as_str)
        .ok_or(ShieldedReplayError::InvalidEnvelope)?;
    let raw = raw.strip_prefix("0x").unwrap_or(raw);
    if raw.len() % 2 != 0 || raw.len() / 2 > MAX_RAW_EVENT_BYTES {
        return Err(ShieldedReplayError::EventTooLarge);
    }
    let bytes = hex::decode(raw).map_err(|_| ShieldedReplayError::InvalidEvent)?;
    let event: Event<DefaultDB> = midnight_serialize::tagged_deserialize(&bytes[..])
        .map_err(|_| ShieldedReplayError::InvalidEvent)?;
    let matches_typename = matches!(
        (typename, &event.content),
        ("ZswapInput", EventDetails::ZswapInput { .. })
            | ("ZswapOutput", EventDetails::ZswapOutput { .. })
    );
    if !matches_typename {
        return Err(ShieldedReplayError::InvalidEvent);
    }
    Ok(DecodedZswapEvent {
        cursor,
        target_cursor,
        raw_bytes: bytes.len(),
        event,
    })
}

/// Replays official ledger events into the official local Zswap state.
///
/// Commitments must arrive at the exact next tree index. Owned outputs are
/// retained only after local decryption/recipient matching and commitment
/// recomputation. Foreign branches are collapsed and spends clear both owned
/// and pending coins by nullifier. Rehashing is deferred to the batch boundary.
pub(crate) fn replay_zswap_events<'a>(
    keys: &SecretKeys,
    start: ZswapState<DefaultDB>,
    events: impl IntoIterator<Item = &'a Event<DefaultDB>>,
) -> Result<ZswapState<DefaultDB>, ShieldedReplayError> {
    let mut state = start;
    for event in events {
        match &event.content {
            EventDetails::ZswapOutput {
                commitment,
                preimage_evidence,
                mt_index,
                ..
            } => {
                if *mt_index != state.first_free {
                    return Err(ShieldedReplayError::NonLinearCommitment);
                }
                state.merkle_tree = state
                    .merkle_tree
                    .try_update_hash(*mt_index, commitment.0, ())
                    .map_err(|_| ShieldedReplayError::InvalidCommitmentTree)?;
                state.first_free = state
                    .first_free
                    .checked_add(1)
                    .ok_or(ShieldedReplayError::InvalidCommitmentTree)?;

                let pending = state.pending_outputs.get(commitment).copied();
                let owned = pending.or_else(|| preimage_evidence.try_with_keys(keys));
                if let Some(coin) = owned.filter(|coin| {
                    coin.commitment(&Recipient::User(keys.coin_public_key())) == *commitment
                }) {
                    retain_owned_coin(&mut state, keys, coin, *mt_index);
                    state.pending_outputs = state.pending_outputs.remove(commitment);
                } else {
                    state.merkle_tree = state.merkle_tree.collapse(*mt_index, *mt_index);
                }
            }
            EventDetails::ZswapInput { nullifier, .. } => {
                state.coins = state.coins.remove(nullifier);
                state.pending_spends = state.pending_spends.remove(nullifier);
            }
            _ => {}
        }
    }
    state.merkle_tree = state.merkle_tree.rehash();
    Ok(state)
}

pub(crate) fn project_zswap_state(
    state: &ZswapState<DefaultDB>,
) -> Result<ShieldedStateProjection, ShieldedReplayError> {
    let mut balances = BTreeMap::<String, u128>::new();
    for (_, coin) in state.coins.iter() {
        let token_type_hex = hex::encode(coin.type_.0.0);
        let current = balances.entry(token_type_hex).or_default();
        *current = current
            .checked_add(coin.value)
            .ok_or(ShieldedReplayError::BalanceOverflow)?;
    }
    Ok(ShieldedStateProjection {
        owned_note_count: u64::try_from(state.coins.size())
            .map_err(|_| ShieldedReplayError::BalanceOverflow)?,
        commitment_count: state.first_free,
        balances: balances
            .into_iter()
            .map(|(token_type_hex, atomic_units)| ShieldedTokenBalance {
                token_type_hex,
                atomic_units,
            })
            .collect(),
    })
}

fn retain_owned_coin(
    state: &mut ZswapState<DefaultDB>,
    keys: &SecretKeys,
    coin: CoinInfo,
    mt_index: u64,
) {
    let nullifier = coin.nullifier(&SenderEvidence::User(Cow::Borrowed(&keys.coin_secret_key)));
    state.coins = state.coins.insert(nullifier, coin.qualify(mt_index));
}

fn unsigned_field(value: Option<&Value>) -> Result<u64, ShieldedReplayError> {
    value
        .and_then(Value::as_i64)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(ShieldedReplayError::InvalidEnvelope)
}

#[cfg(test)]
mod tests {
    use midnight_coin_structure::coin::{Info as CoinInfo, ShieldedTokenType};
    use midnight_ledger::{
        events::{EventSource, ZswapPreimageEvidence},
        structure::TransactionHash,
    };
    use midnight_zswap::keys::Seed;
    use rand::{Rng as _, SeedableRng as _, rngs::StdRng};
    use serde_json::json;

    use super::*;

    fn source() -> EventSource {
        EventSource {
            transaction_hash: TransactionHash::default(),
            logical_segment: 0,
            physical_segment: 0,
        }
    }

    fn output(coin: CoinInfo, recipient: &SecretKeys, mt_index: u64) -> Event<DefaultDB> {
        Event {
            source: source(),
            content: EventDetails::ZswapOutput {
                commitment: coin.commitment(&Recipient::User(recipient.coin_public_key())),
                preimage_evidence: ZswapPreimageEvidence::PublicPreimage {
                    coin,
                    recipient: Recipient::User(recipient.coin_public_key()),
                },
                contract: None,
                mt_index,
            },
        }
    }

    #[test]
    fn decoder_accepts_tagged_official_events_and_rejects_ambiguous_envelopes() {
        let mut rng = StdRng::seed_from_u64(7);
        let keys = SecretKeys::from(Seed::from([7; 32]));
        let event = output(
            CoinInfo {
                nonce: rng.r#gen(),
                type_: ShieldedTokenType(rng.r#gen()),
                value: 42,
            },
            &keys,
            0,
        );
        let mut bytes = Vec::new();
        midnight_serialize::tagged_serialize(&event, &mut bytes)
            .expect("official event serializes");
        let decoded = decode_zswap_event(&json!({
            "zswapLedgerEvents": {
                "__typename": "ZswapOutput",
                "id": 4,
                "maxId": 9,
                "raw": format!("0x{}", hex::encode(&bytes))
            }
        }))
        .expect("official event decodes");
        assert_eq!((decoded.cursor, decoded.target_cursor), (4, 9));
        assert_eq!(decoded.raw_bytes, bytes.len());
        assert!(matches!(
            decoded.event.content,
            EventDetails::ZswapOutput { .. }
        ));

        assert_eq!(
            decode_zswap_event(&json!({
                "zswapLedgerEvents": {
                    "__typename": "ZswapInput",
                    "id": 4,
                    "maxId": 3,
                    "raw": "00"
                }
            }))
            .err(),
            Some(ShieldedReplayError::InvalidEnvelope)
        );
    }

    #[test]
    fn replay_keeps_only_commitment_verified_owned_coins_and_collapses_foreign_branches() {
        let mut rng = StdRng::seed_from_u64(11);
        let keys = SecretKeys::from(Seed::from([1; 32]));
        let foreign = SecretKeys::from(Seed::from([2; 32]));
        let token = ShieldedTokenType(rng.r#gen());
        let owned = CoinInfo {
            nonce: rng.r#gen(),
            type_: token,
            value: 7,
        };
        let foreign_coin = CoinInfo {
            nonce: rng.r#gen(),
            type_: token,
            value: 13,
        };
        let events = [output(owned, &keys, 0), output(foreign_coin, &foreign, 1)];

        let state = replay_zswap_events(&keys, ZswapState::new(), events.iter())
            .expect("linear replay succeeds");
        let projection = project_zswap_state(&state).expect("projection is bounded");
        assert_eq!(projection.owned_note_count, 1);
        assert_eq!(projection.commitment_count, 2);
        assert_eq!(projection.balances.len(), 1);
        assert_eq!(projection.balances[0].atomic_units, 7);
        assert_eq!(
            projection.balances[0].token_type_hex,
            hex::encode(token.0.0)
        );
    }

    #[test]
    fn replay_recomputes_commitments_and_removes_spent_coins_by_nullifier() {
        let mut rng = StdRng::seed_from_u64(13);
        let keys = SecretKeys::from(Seed::from([3; 32]));
        let foreign = SecretKeys::from(Seed::from([4; 32]));
        let token = ShieldedTokenType(rng.r#gen());
        let owned = CoinInfo {
            nonce: rng.r#gen(),
            type_: token,
            value: 17,
        };
        let nullifier =
            owned.nullifier(&SenderEvidence::User(Cow::Borrowed(&keys.coin_secret_key)));
        let owned_output = output(owned, &keys, 0);
        let spend = Event {
            source: source(),
            content: EventDetails::ZswapInput {
                nullifier,
                contract: None,
            },
        };
        let spent = replay_zswap_events(&keys, ZswapState::new(), [&owned_output, &spend])
            .expect("spend replay succeeds");
        assert_eq!(
            project_zswap_state(&spent)
                .expect("projection")
                .owned_note_count,
            0
        );

        let mismatched_coin = CoinInfo {
            nonce: rng.r#gen(),
            type_: token,
            value: 23,
        };
        let mut mismatched = output(mismatched_coin, &keys, 0);
        if let EventDetails::ZswapOutput { commitment, .. } = &mut mismatched.content {
            *commitment = mismatched_coin.commitment(&Recipient::User(foreign.coin_public_key()));
        }
        let rejected = replay_zswap_events(&keys, ZswapState::new(), [&mismatched])
            .expect("the tree retains but wallet rejects a mismatched preimage");
        assert_eq!(
            project_zswap_state(&rejected)
                .expect("projection")
                .owned_note_count,
            0
        );
    }

    #[test]
    fn replay_rejects_non_linear_commitment_indices() {
        let mut rng = StdRng::seed_from_u64(17);
        let keys = SecretKeys::from(Seed::from([5; 32]));
        let event = output(
            CoinInfo {
                nonce: rng.r#gen(),
                type_: ShieldedTokenType(rng.r#gen()),
                value: 29,
            },
            &keys,
            1,
        );
        assert_eq!(
            replay_zswap_events(&keys, ZswapState::new(), [&event]).err(),
            Some(ShieldedReplayError::NonLinearCommitment)
        );
    }
}
