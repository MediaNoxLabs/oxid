// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

//! Authenticated fixed-width segments for the optional support journal.
//!
//! This crate owns no filesystem, key generation, queue, worker, retention,
//! export, or composition behavior. A caller supplies protected key material
//! and the expected previous sealed head. Only the closed application event
//! can become plaintext record material.

mod store;

pub use store::{
    RecoveredSupportJournalArchive, SupportJournalArchiveError, SupportJournalArchiveStore,
};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, Key, KeyInit as _, Payload},
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac as _};
use oxid_diagnostics_application::{
    MAX_SUPPORT_JOURNAL_FLUSH_BATCH, SUPPORT_JOURNAL_SCHEMA_VERSION, SupportJournalActionToken,
    SupportJournalCode, SupportJournalEvent, SupportJournalOutcome, SupportJournalSessionEpoch,
    SupportJournalSeverity, SupportJournalStage, SupportJournalSubsystem,
};
use sha2::Sha256;
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"OXIDSJ01";
const FORMAT_VERSION: u16 = 1;
const NONCE_BYTES: usize = 24;
const CHAIN_HEAD_BYTES: usize = 32;
const TAG_BYTES: usize = 16;
const FIXED_RECORD_BYTES: usize = 57;
const HEADER_BYTES: usize = MAGIC.len() + 2 + 2 + 2 + CHAIN_HEAD_BYTES + NONCE_BYTES;
/// Maximum encoded bytes for one authenticated flush segment.
pub const MAX_SEALED_SUPPORT_JOURNAL_SEGMENT_BYTES: usize =
    HEADER_BYTES + (FIXED_RECORD_BYTES * MAX_SUPPORT_JOURNAL_FLUSH_BATCH) + TAG_BYTES;
const KEY_DERIVATION_SALT: &[u8] = b"oxid.support-journal.epoch.v1";
const ENCRYPTION_KEY_INFO: &[u8] = b"segment-encryption-key";
const CHAIN_KEY_INFO: &[u8] = b"segment-chain-key";
const CHAIN_MESSAGE_DOMAIN: &[u8] = b"oxid.support-journal.segment-chain.v1";

/// Closed failure surface for support-journal segment processing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportJournalSegmentError {
    /// The caller supplied an empty, oversized, or internally inconsistent batch.
    InvalidInput,
    /// Authentication, version, framing, or chain validation failed.
    Integrity,
    /// A required operating-system capability such as secure randomness failed.
    Unavailable,
}

/// Caller-supplied data-encryption key for one support-journal epoch.
pub struct SupportJournalSegmentKey {
    encryption_key: Zeroizing<[u8; 32]>,
    chain_key: Zeroizing<[u8; 32]>,
}

impl SupportJournalSegmentKey {
    /// Takes ownership of protected key material supplied by platform custody.
    #[must_use]
    pub fn from_protected_bytes(bytes: Zeroizing<[u8; 32]>) -> Self {
        let derivation = Hkdf::<Sha256>::new(Some(KEY_DERIVATION_SALT), bytes.as_slice());
        let mut encryption_key = Zeroizing::new([0; 32]);
        let mut chain_key = Zeroizing::new([0; 32]);
        derivation
            .expand(ENCRYPTION_KEY_INFO, encryption_key.as_mut())
            .expect("SHA-256 HKDF supports a 32-byte encryption key");
        derivation
            .expand(CHAIN_KEY_INFO, chain_key.as_mut())
            .expect("SHA-256 HKDF supports a 32-byte chain key");
        Self {
            encryption_key,
            chain_key,
        }
    }

    fn cipher(&self) -> Result<XChaCha20Poly1305, SupportJournalSegmentError> {
        let key = Key::<XChaCha20Poly1305>::try_from(self.encryption_key.as_slice())
            .map_err(|_| SupportJournalSegmentError::Integrity)?;
        Ok(XChaCha20Poly1305::new(&key))
    }

    fn chain_head(&self, bytes: &[u8]) -> [u8; CHAIN_HEAD_BYTES] {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.chain_key.as_slice())
            .expect("support-journal epoch keys have the HMAC key shape");
        mac.update(CHAIN_MESSAGE_DOMAIN);
        mac.update(bytes);
        mac.finalize().into_bytes().into()
    }
}

/// Authenticated segment bytes plus the head required by the next segment.
pub struct SealedSupportJournalSegment {
    bytes: Vec<u8>,
    chain_head: [u8; CHAIN_HEAD_BYTES],
}

impl SealedSupportJournalSegment {
    /// Versioned encrypted bytes suitable for a later bounded store.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Authenticated content head that must bind the following segment.
    #[must_use]
    pub const fn chain_head(&self) -> [u8; CHAIN_HEAD_BYTES] {
        self.chain_head
    }

    /// Consumes the value without exposing plaintext record material.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Authenticated fixed-width codec for one journal epoch.
pub struct SupportJournalSegmentCodec {
    key: SupportJournalSegmentKey,
}

impl SupportJournalSegmentCodec {
    #[must_use]
    pub const fn new(key: SupportJournalSegmentKey) -> Self {
        Self { key }
    }

    /// Seals one non-empty, strictly ordered flush batch.
    pub fn seal(
        &self,
        previous_head: [u8; CHAIN_HEAD_BYTES],
        events: &[SupportJournalEvent],
    ) -> Result<SealedSupportJournalSegment, SupportJournalSegmentError> {
        let mut nonce = [0_u8; NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| SupportJournalSegmentError::Unavailable)?;
        self.seal_with_nonce(previous_head, events, nonce)
    }

    /// Opens one segment only when it extends the caller's exact chain head.
    pub fn open(
        &self,
        expected_previous_head: [u8; CHAIN_HEAD_BYTES],
        bytes: &[u8],
    ) -> Result<(Vec<SupportJournalEvent>, [u8; CHAIN_HEAD_BYTES]), SupportJournalSegmentError>
    {
        if bytes.len() < HEADER_BYTES + TAG_BYTES
            || bytes.len() > MAX_SEALED_SUPPORT_JOURNAL_SEGMENT_BYTES
        {
            return Err(SupportJournalSegmentError::Integrity);
        }
        if bytes.get(..MAGIC.len()) != Some(MAGIC) {
            return Err(SupportJournalSegmentError::Integrity);
        }
        let format_version = read_u16(bytes, MAGIC.len())?;
        let schema_version = read_u16(bytes, MAGIC.len() + 2)?;
        let count = usize::from(read_u16(bytes, MAGIC.len() + 4)?);
        if format_version != FORMAT_VERSION
            || schema_version != SUPPORT_JOURNAL_SCHEMA_VERSION
            || count == 0
            || count > MAX_SUPPORT_JOURNAL_FLUSH_BATCH
            || bytes.len() != HEADER_BYTES + (count * FIXED_RECORD_BYTES) + TAG_BYTES
        {
            return Err(SupportJournalSegmentError::Integrity);
        }
        let previous_offset = MAGIC.len() + 6;
        if bytes.get(previous_offset..previous_offset + CHAIN_HEAD_BYTES)
            != Some(expected_previous_head.as_slice())
        {
            return Err(SupportJournalSegmentError::Integrity);
        }
        let nonce_offset = previous_offset + CHAIN_HEAD_BYTES;
        let nonce = XNonce::try_from(
            bytes
                .get(nonce_offset..HEADER_BYTES)
                .ok_or(SupportJournalSegmentError::Integrity)?,
        )
        .map_err(|_| SupportJournalSegmentError::Integrity)?;
        let plaintext = Zeroizing::new(
            self.key
                .cipher()?
                .decrypt(
                    &nonce,
                    Payload {
                        msg: &bytes[HEADER_BYTES..],
                        aad: &bytes[..HEADER_BYTES],
                    },
                )
                .map_err(|_| SupportJournalSegmentError::Integrity)?,
        );
        if plaintext.len() != count * FIXED_RECORD_BYTES {
            return Err(SupportJournalSegmentError::Integrity);
        }
        let mut events = Vec::with_capacity(count);
        for frame in plaintext.chunks_exact(FIXED_RECORD_BYTES) {
            events.push(decode_event(frame)?);
        }
        validate_batch(&events).map_err(|_| SupportJournalSegmentError::Integrity)?;
        Ok((events, self.key.chain_head(bytes)))
    }

    fn seal_with_nonce(
        &self,
        previous_head: [u8; CHAIN_HEAD_BYTES],
        events: &[SupportJournalEvent],
        nonce_bytes: [u8; NONCE_BYTES],
    ) -> Result<SealedSupportJournalSegment, SupportJournalSegmentError> {
        validate_batch(events)?;
        let count =
            u16::try_from(events.len()).map_err(|_| SupportJournalSegmentError::InvalidInput)?;
        let mut header = Vec::with_capacity(HEADER_BYTES);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
        header.extend_from_slice(&SUPPORT_JOURNAL_SCHEMA_VERSION.to_be_bytes());
        header.extend_from_slice(&count.to_be_bytes());
        header.extend_from_slice(&previous_head);
        header.extend_from_slice(&nonce_bytes);
        debug_assert_eq!(header.len(), HEADER_BYTES);

        let mut plaintext = Zeroizing::new(Vec::with_capacity(events.len() * FIXED_RECORD_BYTES));
        for event in events {
            let record = encode_event(*event)?;
            plaintext.extend_from_slice(record.as_slice());
        }
        let nonce = XNonce::try_from(nonce_bytes.as_slice())
            .map_err(|_| SupportJournalSegmentError::Integrity)?;
        let ciphertext = self
            .key
            .cipher()?
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: &header,
                },
            )
            .map_err(|_| SupportJournalSegmentError::Integrity)?;
        let mut bytes = Vec::with_capacity(header.len() + ciphertext.len());
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(&ciphertext);
        let chain_head = self.key.chain_head(&bytes);
        Ok(SealedSupportJournalSegment { bytes, chain_head })
    }
}

fn validate_batch(events: &[SupportJournalEvent]) -> Result<(), SupportJournalSegmentError> {
    if events.is_empty() || events.len() > MAX_SUPPORT_JOURNAL_FLUSH_BATCH {
        return Err(SupportJournalSegmentError::InvalidInput);
    }
    let epoch = events[0].session_epoch();
    let mut previous_sequence = None;
    for event in events {
        if event.schema_version() != SUPPORT_JOURNAL_SCHEMA_VERSION
            || event.session_epoch() != epoch
            || previous_sequence.is_some_and(|previous| event.sequence() <= previous)
        {
            return Err(SupportJournalSegmentError::InvalidInput);
        }
        previous_sequence = Some(event.sequence());
    }
    Ok(())
}

fn encode_event(
    event: SupportJournalEvent,
) -> Result<Zeroizing<[u8; FIXED_RECORD_BYTES]>, SupportJournalSegmentError> {
    let mut output = Zeroizing::new([0; FIXED_RECORD_BYTES]);
    output[0..2].copy_from_slice(&event.schema_version().to_be_bytes());
    output[2..10].copy_from_slice(&event.sequence().to_be_bytes());
    output[10] = enum_index(SupportJournalSubsystem::ALL, event.subsystem())?;
    output[11] = enum_index(SupportJournalCode::ALL, event.code())?;
    output[12] = enum_index(SupportJournalSeverity::ALL, event.severity())?;
    output[13] = enum_index(SupportJournalStage::ALL, event.stage())?;
    output[14] = enum_index(SupportJournalOutcome::ALL, event.outcome())?;
    output[15..31].copy_from_slice(&event.session_epoch().into_bytes());
    if let Some(action) = event.action_token() {
        output[31] = 1;
        output[32..48].copy_from_slice(&action.into_bytes());
    }
    if let Some(minute) = event.coarse_unix_minute() {
        output[48] = 1;
        output[49..57].copy_from_slice(&minute.to_be_bytes());
    }
    Ok(output)
}

fn decode_event(frame: &[u8]) -> Result<SupportJournalEvent, SupportJournalSegmentError> {
    if frame.len() != FIXED_RECORD_BYTES {
        return Err(SupportJournalSegmentError::Integrity);
    }
    let schema_version = read_u16(frame, 0)?;
    if schema_version != SUPPORT_JOURNAL_SCHEMA_VERSION {
        return Err(SupportJournalSegmentError::Integrity);
    }
    let sequence = read_u64(frame, 2)?;
    let subsystem = enum_value(SupportJournalSubsystem::ALL, frame[10])?;
    let code = enum_value(SupportJournalCode::ALL, frame[11])?;
    let severity = enum_value(SupportJournalSeverity::ALL, frame[12])?;
    let stage = enum_value(SupportJournalStage::ALL, frame[13])?;
    let outcome = enum_value(SupportJournalOutcome::ALL, frame[14])?;
    let session_epoch = SupportJournalSessionEpoch::from_random_bytes(read_array(frame, 15)?);
    let action_token =
        decode_optional_fixed(frame, 31)?.map(SupportJournalActionToken::from_random_bytes);
    let coarse_unix_minute = decode_optional_u64(frame, 48)?;
    Ok(SupportJournalEvent::new(
        sequence,
        subsystem,
        code,
        severity,
        stage,
        outcome,
        session_epoch,
        action_token,
        coarse_unix_minute,
    ))
}

fn enum_index<T: Copy + PartialEq>(
    values: &[T],
    value: T,
) -> Result<u8, SupportJournalSegmentError> {
    values
        .iter()
        .position(|candidate| *candidate == value)
        .and_then(|index| u8::try_from(index).ok())
        .ok_or(SupportJournalSegmentError::InvalidInput)
}

fn enum_value<T: Copy>(values: &[T], index: u8) -> Result<T, SupportJournalSegmentError> {
    values
        .get(usize::from(index))
        .copied()
        .ok_or(SupportJournalSegmentError::Integrity)
}

fn decode_optional_fixed(
    frame: &[u8],
    offset: usize,
) -> Result<Option<[u8; 16]>, SupportJournalSegmentError> {
    let bytes = read_array(frame, offset + 1)?;
    match frame[offset] {
        0 if bytes == [0; 16] => Ok(None),
        1 => Ok(Some(bytes)),
        _ => Err(SupportJournalSegmentError::Integrity),
    }
}

fn decode_optional_u64(
    frame: &[u8],
    offset: usize,
) -> Result<Option<u64>, SupportJournalSegmentError> {
    let value = read_u64(frame, offset + 1)?;
    match frame[offset] {
        0 if value == 0 => Ok(None),
        1 => Ok(Some(value)),
        _ => Err(SupportJournalSegmentError::Integrity),
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, SupportJournalSegmentError> {
    Ok(u16::from_be_bytes(read_array(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, SupportJournalSegmentError> {
    Ok(u64::from_be_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(
    bytes: &[u8],
    offset: usize,
) -> Result<[u8; N], SupportJournalSegmentError> {
    bytes
        .get(offset..offset + N)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(SupportJournalSegmentError::Integrity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codec(byte: u8) -> SupportJournalSegmentCodec {
        SupportJournalSegmentCodec::new(SupportJournalSegmentKey::from_protected_bytes(
            Zeroizing::new([byte; 32]),
        ))
    }

    fn event(sequence: u64, action: bool, minute: bool) -> SupportJournalEvent {
        SupportJournalEvent::new(
            sequence,
            SupportJournalSubsystem::Identity,
            SupportJournalCode::ConsentResolved,
            SupportJournalSeverity::Info,
            SupportJournalStage::Terminal,
            SupportJournalOutcome::Refused,
            SupportJournalSessionEpoch::from_random_bytes([7; 16]),
            action.then(|| SupportJournalActionToken::from_random_bytes([8; 16])),
            minute.then_some(900),
        )
    }

    #[test]
    fn fixed_width_round_trip_preserves_closed_fields() {
        let codec = codec(3);
        let events = [event(1, false, false), event(2, true, true)];
        let sealed = codec
            .seal_with_nonce([0; 32], &events, [4; 24])
            .expect("seal");
        assert_eq!(
            sealed.as_bytes().len(),
            HEADER_BYTES + 2 * FIXED_RECORD_BYTES + TAG_BYTES
        );
        assert_eq!(
            codec.open([0; 32], sealed.as_bytes()).expect("open"),
            (events.to_vec(), sealed.chain_head())
        );
    }

    #[test]
    fn every_closed_enum_value_round_trips() {
        let codec = codec(5);
        let mut sequence = 0_u64;
        for subsystem in SupportJournalSubsystem::ALL {
            for code in SupportJournalCode::ALL {
                for severity in SupportJournalSeverity::ALL {
                    for stage in SupportJournalStage::ALL {
                        for outcome in SupportJournalOutcome::ALL {
                            sequence += 1;
                            let value = SupportJournalEvent::new(
                                sequence,
                                *subsystem,
                                *code,
                                *severity,
                                *stage,
                                *outcome,
                                SupportJournalSessionEpoch::from_random_bytes([9; 16]),
                                None,
                                None,
                            );
                            let sealed = codec
                                .seal_with_nonce([0; 32], &[value], [10; 24])
                                .expect("seal");
                            assert_eq!(
                                codec.open([0; 32], sealed.as_bytes()).expect("open"),
                                (vec![value], sealed.chain_head())
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn wrong_key_tamper_truncation_and_version_drift_fail_closed() {
        let primary = codec(11);
        let sealed = primary
            .seal_with_nonce([0; 32], &[event(1, true, true)], [12; 24])
            .expect("seal");
        assert_eq!(
            codec(13).open([0; 32], sealed.as_bytes()),
            Err(SupportJournalSegmentError::Integrity)
        );

        for mutation in [MAGIC.len(), HEADER_BYTES, sealed.as_bytes().len() - 1] {
            let mut bytes = sealed.as_bytes().to_vec();
            bytes[mutation] ^= 1;
            assert_eq!(
                primary.open([0; 32], &bytes),
                Err(SupportJournalSegmentError::Integrity)
            );
        }
        assert_eq!(
            primary.open([0; 32], &sealed.as_bytes()[..sealed.as_bytes().len() - 1]),
            Err(SupportJournalSegmentError::Integrity)
        );
    }

    #[test]
    fn chain_head_rejects_reorder_duplication_and_internal_gaps() {
        let codec = codec(14);
        let first = codec
            .seal_with_nonce([0; 32], &[event(1, false, false)], [15; 24])
            .expect("first");
        let second = codec
            .seal_with_nonce(first.chain_head(), &[event(2, false, false)], [16; 24])
            .expect("second");
        let third = codec
            .seal_with_nonce(second.chain_head(), &[event(3, false, false)], [17; 24])
            .expect("third");
        assert!(codec.open([0; 32], first.as_bytes()).is_ok());
        assert!(codec.open(first.chain_head(), second.as_bytes()).is_ok());
        assert_eq!(
            codec.open([0; 32], second.as_bytes()),
            Err(SupportJournalSegmentError::Integrity)
        );
        assert_eq!(
            codec.open(first.chain_head(), first.as_bytes()),
            Err(SupportJournalSegmentError::Integrity)
        );
        assert_eq!(
            codec.open(first.chain_head(), third.as_bytes()),
            Err(SupportJournalSegmentError::Integrity)
        );
    }

    #[test]
    fn batch_shape_is_strictly_bounded_and_ordered() {
        let codec = codec(17);
        assert!(matches!(
            codec.seal_with_nonce([0; 32], &[], [18; 24]),
            Err(SupportJournalSegmentError::InvalidInput)
        ));
        let oversized = (0..=MAX_SUPPORT_JOURNAL_FLUSH_BATCH)
            .map(|index| event(u64::try_from(index + 1).expect("sequence"), false, false))
            .collect::<Vec<_>>();
        assert!(matches!(
            codec.seal_with_nonce([0; 32], &oversized, [18; 24]),
            Err(SupportJournalSegmentError::InvalidInput)
        ));
        assert!(matches!(
            codec.seal_with_nonce(
                [0; 32],
                &[event(2, false, false), event(1, false, false)],
                [18; 24]
            ),
            Err(SupportJournalSegmentError::InvalidInput)
        ));
    }

    #[test]
    fn public_seal_uses_fresh_nonces_and_covers_the_maximum_batch() {
        let codec = codec(19);
        let events = (1..=MAX_SUPPORT_JOURNAL_FLUSH_BATCH)
            .map(|sequence| event(u64::try_from(sequence).expect("sequence"), true, true))
            .collect::<Vec<_>>();
        let first = codec.seal([0; 32], &events).expect("first seal");
        let second = codec.seal([0; 32], &events).expect("second seal");
        assert_eq!(
            first.as_bytes().len(),
            MAX_SEALED_SUPPORT_JOURNAL_SEGMENT_BYTES
        );
        assert_ne!(first.as_bytes(), second.as_bytes());
        assert_eq!(
            codec.open([0; 32], first.as_bytes()).expect("open"),
            (events, first.chain_head())
        );
    }

    #[test]
    fn sealed_payload_is_not_the_fixed_plaintext_record() {
        let codec = codec(20);
        let value = event(1, true, true);
        let plaintext = encode_event(value).expect("encode");
        let sealed = codec
            .seal_with_nonce([0; 32], &[value], [21; 24])
            .expect("seal");
        assert_ne!(
            &sealed.as_bytes()[HEADER_BYTES..HEADER_BYTES + FIXED_RECORD_BYTES],
            plaintext.as_slice()
        );
    }
}
