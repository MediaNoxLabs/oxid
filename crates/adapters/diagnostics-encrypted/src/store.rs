// SPDX-License-Identifier: Apache-2.0

//! Owner-private, bounded storage for authenticated support-journal segments.

use std::{
    fs,
    path::{Path, PathBuf},
};

use oxid_adapter_store_atomic::{
    AtomicStoreError, ensure_private_directory, read_owner_private_bounded,
    reject_non_private_file, write_owner_private,
};
use oxid_diagnostics_application::{
    MAX_SUPPORT_JOURNAL_DURABLE_BYTES, MAX_SUPPORT_JOURNAL_DURABLE_RECORDS,
    MAX_SUPPORT_JOURNAL_RETENTION_DAYS, SupportJournalEvent, SupportJournalSessionEpoch,
};

use crate::{
    CHAIN_HEAD_BYTES, MAX_SEALED_SUPPORT_JOURNAL_SEGMENT_BYTES, SealedSupportJournalSegment,
    SupportJournalSegmentCodec, SupportJournalSegmentError,
};

const MANIFEST_NAME: &str = "manifest.bin";
const PREVIOUS_MANIFEST_NAME: &str = "manifest.previous.bin";
const MANIFEST_MAGIC: &[u8; 8] = b"OXIDSJM1";
const MANIFEST_VERSION: u16 = 1;
const MANIFEST_HEADER_BYTES: usize = 8 + 2 + 16 + 2 + 4 + 4 + 32 + 32 + 8;
const MANIFEST_ENTRY_BYTES: usize = 8 + 8 + 2 + 4 + 32 + 8;
const MAX_MANIFEST_BYTES: usize =
    MANIFEST_HEADER_BYTES + (MAX_SUPPORT_JOURNAL_DURABLE_RECORDS * MANIFEST_ENTRY_BYTES);
const MINUTES_PER_DAY: u64 = 24 * 60;

/// Closed archive failure surface. Filesystem details never cross this boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportJournalArchiveError {
    /// Stored bytes, permissions, file types, framing, or chain evidence are untrustworthy.
    Integrity,
    /// The owner-private store or protected key capability is unavailable.
    Unavailable,
    /// The segment is valid but cannot fit the fixed archive envelope.
    Capacity,
}

impl From<AtomicStoreError> for SupportJournalArchiveError {
    fn from(value: AtomicStoreError) -> Self {
        match value {
            AtomicStoreError::Integrity => Self::Integrity,
            AtomicStoreError::Unavailable => Self::Unavailable,
        }
    }
}

impl From<SupportJournalSegmentError> for SupportJournalArchiveError {
    fn from(value: SupportJournalSegmentError) -> Self {
        match value {
            SupportJournalSegmentError::Integrity | SupportJournalSegmentError::InvalidInput => {
                Self::Integrity
            }
            SupportJournalSegmentError::Unavailable => Self::Unavailable,
        }
    }
}

/// Recovered closed events and the head a protected platform store should retain.
#[derive(Debug, PartialEq, Eq)]
pub struct RecoveredSupportJournalArchive {
    events: Vec<SupportJournalEvent>,
    latest_head: [u8; CHAIN_HEAD_BYTES],
    evicted_records: u32,
    degraded: bool,
}

impl RecoveredSupportJournalArchive {
    #[must_use]
    pub fn events(&self) -> &[SupportJournalEvent] {
        &self.events
    }

    #[must_use]
    pub const fn latest_head(&self) -> [u8; CHAIN_HEAD_BYTES] {
        self.latest_head
    }

    #[must_use]
    pub const fn evicted_records(&self) -> u32 {
        self.evicted_records
    }

    /// True when recovery removed one damaged segment and its chained suffix.
    #[must_use]
    pub const fn is_degraded(&self) -> bool {
        self.degraded
    }
}

/// Storage root supplied by composition; it is never inferred from wallet state.
pub struct SupportJournalArchiveStore {
    root: PathBuf,
}

impl SupportJournalArchiveStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Atomically appends one already-authenticated segment and returns its sealed head.
    pub fn append(
        &self,
        codec: &SupportJournalSegmentCodec,
        protected_latest_head: [u8; CHAIN_HEAD_BYTES],
        segment: &SealedSupportJournalSegment,
    ) -> Result<[u8; CHAIN_HEAD_BYTES], SupportJournalArchiveError> {
        ensure_private_directory(&self.root)?;
        let mut manifest = self.load_manifest()?.unwrap_or_default();
        if manifest.latest_head != protected_latest_head {
            return Err(SupportJournalArchiveError::Integrity);
        }
        // A matching protected head confirms the current manifest. Cleanup from a prior
        // committed append is hygiene only and can never block the wallet operation.
        let _ = self.remove_orphan_segments(&manifest);
        let _ = self.remove_optional_private_file(&self.root.join(PREVIOUS_MANIFEST_NAME));
        write_owner_private(&self.root.join(PREVIOUS_MANIFEST_NAME), &manifest.encode()?)?;
        let (events, head) = codec.open(manifest.latest_head, segment.as_bytes())?;
        let first = events
            .first()
            .ok_or(SupportJournalArchiveError::Integrity)?;
        let last = events.last().ok_or(SupportJournalArchiveError::Integrity)?;
        if manifest
            .entries
            .last()
            .is_some_and(|entry| first.sequence() <= entry.last_sequence)
        {
            return Err(SupportJournalArchiveError::Integrity);
        }
        match manifest.epoch {
            Some(epoch) if epoch != first.session_epoch() => {
                return Err(SupportJournalArchiveError::Integrity);
            }
            None => manifest.epoch = Some(first.session_epoch()),
            _ => {}
        }

        let encoded_bytes = u32::try_from(segment.as_bytes().len())
            .map_err(|_| SupportJournalArchiveError::Capacity)?;
        let record_count =
            u16::try_from(events.len()).map_err(|_| SupportJournalArchiveError::Capacity)?;
        let entry = ManifestEntry {
            first_sequence: first.sequence(),
            last_sequence: last.sequence(),
            record_count,
            encoded_bytes,
            chain_head: head,
            last_event_minute: events
                .iter()
                .filter_map(|event| event.coarse_unix_minute())
                .max(),
        };
        let path = self.segment_path(entry.first_sequence, entry.last_sequence);
        write_owner_private(&path, segment.as_bytes())?;
        manifest.entries.push(entry);
        manifest.latest_head = head;
        manifest.last_event_minute = match (manifest.last_event_minute, entry.last_event_minute) {
            (Some(previous), Some(current)) => Some(previous.max(current)),
            (previous, current) => previous.or(current),
        };
        let _deferred_evictions = manifest.evict_to_limits()?;
        self.write_manifest(&manifest)?;
        Ok(head)
    }

    /// Recovers the chain in order and binds it to the protected latest head.
    pub fn recover(
        &self,
        codec: &SupportJournalSegmentCodec,
        protected_latest_head: [u8; CHAIN_HEAD_BYTES],
        now_unix_minute: u64,
    ) -> Result<RecoveredSupportJournalArchive, SupportJournalArchiveError> {
        ensure_private_directory(&self.root)?;
        let Some(manifest) = self.load_manifest()? else {
            if protected_latest_head != [0; CHAIN_HEAD_BYTES] {
                return Err(SupportJournalArchiveError::Integrity);
            }
            return Ok(RecoveredSupportJournalArchive {
                events: Vec::new(),
                latest_head: [0; CHAIN_HEAD_BYTES],
                evicted_records: 0,
                degraded: false,
            });
        };
        let mut manifest = self.reconcile_manifest(manifest, protected_latest_head)?;

        let mut expected = manifest.base_head;
        let mut events = Vec::with_capacity(manifest.record_count());
        for (index, entry) in manifest.entries.iter().copied().enumerate() {
            let path = self.segment_path(entry.first_sequence, entry.last_sequence);
            let bytes = match read_owner_private_bounded(
                &path,
                MAX_SEALED_SUPPORT_JOURNAL_SEGMENT_BYTES,
            )? {
                Some(bytes) => bytes,
                None => {
                    return self.quarantine_from(&mut manifest, index, &path, events, false);
                }
            };
            let (decoded, head) = match codec.open(expected, &bytes) {
                Ok(decoded) => decoded,
                Err(_) if index == 0 => return Err(SupportJournalArchiveError::Integrity),
                Err(_) => {
                    return self.quarantine_from(&mut manifest, index, &path, events, true);
                }
            };
            if decoded.first().map(|event| event.sequence()) != Some(entry.first_sequence)
                || decoded.last().map(|event| event.sequence()) != Some(entry.last_sequence)
                || decoded.len() != usize::from(entry.record_count)
                || u32::try_from(bytes.len()).ok() != Some(entry.encoded_bytes)
                || head != entry.chain_head
                || decoded
                    .iter()
                    .filter_map(|event| event.coarse_unix_minute())
                    .max()
                    != entry.last_event_minute
                || decoded
                    .iter()
                    .any(|event| Some(event.session_epoch()) != manifest.epoch)
            {
                // The authenticated segment opened successfully, so a disagreement here can be
                // an untrusted manifest mutation rather than corrupt ciphertext. Never destroy
                // a valid segment on ambiguous evidence.
                return Err(SupportJournalArchiveError::Integrity);
            }
            expected = head;
            events.extend(decoded);
        }
        if expected != manifest.latest_head {
            return Err(SupportJournalArchiveError::Integrity);
        }
        let authenticated_last_minute = events
            .iter()
            .filter_map(|event| event.coarse_unix_minute())
            .max();
        if authenticated_last_minute != manifest.last_event_minute {
            return Err(SupportJournalArchiveError::Integrity);
        }
        if manifest.is_expired_from(authenticated_last_minute, now_unix_minute) {
            self.clear_manifest_segments(&manifest)?;
            self.clear_quarantines()?;
            self.write_manifest(&Manifest::default())?;
            self.remove_optional_private_file(&self.root.join(PREVIOUS_MANIFEST_NAME))?;
            return Ok(RecoveredSupportJournalArchive {
                events: Vec::new(),
                latest_head: [0; CHAIN_HEAD_BYTES],
                evicted_records: 0,
                degraded: false,
            });
        }
        let degraded = self.remove_orphan_segments(&manifest);
        if self
            .remove_optional_private_file(&self.root.join(PREVIOUS_MANIFEST_NAME))
            .is_err()
        {
            // A stale rollback manifest is non-authoritative hygiene after the current chain and
            // protected head have both been verified.
        }
        Ok(RecoveredSupportJournalArchive {
            events,
            latest_head: manifest.latest_head,
            evicted_records: manifest.evicted_records,
            degraded,
        })
    }

    fn reconcile_manifest(
        &self,
        manifest: Manifest,
        protected_latest_head: [u8; CHAIN_HEAD_BYTES],
    ) -> Result<Manifest, SupportJournalArchiveError> {
        if manifest.latest_head == protected_latest_head {
            return Ok(manifest);
        }
        let previous = read_owner_private_bounded(
            &self.root.join(PREVIOUS_MANIFEST_NAME),
            MAX_MANIFEST_BYTES,
        )?
        .ok_or(SupportJournalArchiveError::Integrity)
        .and_then(|bytes| Manifest::decode(&bytes))?;
        if previous.latest_head != protected_latest_head {
            return Err(SupportJournalArchiveError::Integrity);
        }
        self.write_manifest(&previous)?;
        let _ = self.remove_orphan_segments(&previous);
        self.remove_optional_private_file(&self.root.join(PREVIOUS_MANIFEST_NAME))?;
        Ok(previous)
    }

    fn quarantine_from(
        &self,
        manifest: &mut Manifest,
        index: usize,
        path: &Path,
        events: Vec<SupportJournalEvent>,
        quarantine_existing: bool,
    ) -> Result<RecoveredSupportJournalArchive, SupportJournalArchiveError> {
        match fs::symlink_metadata(path) {
            Ok(_) if quarantine_existing => {
                reject_non_private_file(path)?;
                self.clear_quarantines()?;
                let quarantine = path.with_extension("quarantine");
                reject_non_private_file(&quarantine)?;
                fs::rename(path, quarantine)
                    .map_err(|_| SupportJournalArchiveError::Unavailable)?;
            }
            Ok(_) => return Err(SupportJournalArchiveError::Integrity),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(SupportJournalArchiveError::Unavailable),
        }
        let removed = manifest.entries.split_off(index);
        for entry in removed.iter().skip(1).copied() {
            self.remove_segment(entry)?;
        }
        manifest.latest_head = manifest
            .entries
            .last()
            .map_or(manifest.base_head, |entry| entry.chain_head);
        if manifest.entries.is_empty() {
            manifest.epoch = None;
            manifest.base_head = [0; CHAIN_HEAD_BYTES];
            manifest.latest_head = [0; CHAIN_HEAD_BYTES];
        }
        manifest.last_event_minute = manifest
            .entries
            .iter()
            .filter_map(|entry| entry.last_event_minute)
            .max();
        self.write_manifest(manifest)?;
        Ok(RecoveredSupportJournalArchive {
            events,
            latest_head: manifest.latest_head,
            evicted_records: manifest.evicted_records,
            degraded: true,
        })
    }

    fn load_manifest(&self) -> Result<Option<Manifest>, SupportJournalArchiveError> {
        read_owner_private_bounded(&self.root.join(MANIFEST_NAME), MAX_MANIFEST_BYTES)?
            .map(|bytes| Manifest::decode(&bytes))
            .transpose()
    }

    fn write_manifest(&self, manifest: &Manifest) -> Result<(), SupportJournalArchiveError> {
        let bytes = manifest.encode()?;
        write_owner_private(&self.root.join(MANIFEST_NAME), &bytes)?;
        Ok(())
    }

    fn segment_path(&self, first: u64, last: u64) -> PathBuf {
        self.root
            .join(format!("segment-{first:020}-{last:020}.bin"))
    }

    fn remove_segment(&self, entry: ManifestEntry) -> Result<(), SupportJournalArchiveError> {
        let path = self.segment_path(entry.first_sequence, entry.last_sequence);
        reject_non_private_file(&path)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SupportJournalArchiveError::Unavailable),
        }
    }

    fn clear_manifest_segments(
        &self,
        manifest: &Manifest,
    ) -> Result<(), SupportJournalArchiveError> {
        for entry in manifest.entries.iter().copied() {
            self.remove_segment(entry)?;
        }
        Ok(())
    }

    fn clear_quarantines(&self) -> Result<(), SupportJournalArchiveError> {
        for item in fs::read_dir(&self.root).map_err(|_| SupportJournalArchiveError::Unavailable)? {
            let item = item.map_err(|_| SupportJournalArchiveError::Unavailable)?;
            let path = item.path();
            let name = item.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("segment-") && name.ends_with(".quarantine") {
                reject_non_private_file(&path)?;
                fs::remove_file(path).map_err(|_| SupportJournalArchiveError::Unavailable)?;
            }
        }
        Ok(())
    }

    fn remove_optional_private_file(&self, path: &Path) -> Result<(), SupportJournalArchiveError> {
        match fs::symlink_metadata(path) {
            Ok(_) => {
                reject_non_private_file(path)?;
                fs::remove_file(path).map_err(|_| SupportJournalArchiveError::Unavailable)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SupportJournalArchiveError::Unavailable),
        }
    }

    fn remove_orphan_segments(&self, manifest: &Manifest) -> bool {
        let admitted = manifest
            .entries
            .iter()
            .map(|entry| self.segment_path(entry.first_sequence, entry.last_sequence))
            .collect::<Vec<_>>();
        let Ok(items) = fs::read_dir(&self.root) else {
            return true;
        };
        let mut degraded = false;
        for item in items {
            let Ok(item) = item else {
                degraded = true;
                continue;
            };
            let path = item.path();
            let name = item.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("segment-")
                && name.ends_with(".bin")
                && !admitted.contains(&path)
                && (reject_non_private_file(&path).is_err() || fs::remove_file(path).is_err())
            {
                degraded = true;
            }
        }
        degraded
    }
}

#[derive(Clone, Copy, Debug)]
struct ManifestEntry {
    first_sequence: u64,
    last_sequence: u64,
    record_count: u16,
    encoded_bytes: u32,
    chain_head: [u8; CHAIN_HEAD_BYTES],
    last_event_minute: Option<u64>,
}

#[derive(Debug)]
struct Manifest {
    epoch: Option<SupportJournalSessionEpoch>,
    base_head: [u8; CHAIN_HEAD_BYTES],
    latest_head: [u8; CHAIN_HEAD_BYTES],
    evicted_records: u32,
    last_event_minute: Option<u64>,
    entries: Vec<ManifestEntry>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            epoch: None,
            base_head: [0; CHAIN_HEAD_BYTES],
            latest_head: [0; CHAIN_HEAD_BYTES],
            evicted_records: 0,
            last_event_minute: None,
            entries: Vec::new(),
        }
    }
}

impl Manifest {
    fn record_count(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| usize::from(entry.record_count))
            .sum()
    }

    fn encoded_bytes(&self) -> usize {
        MANIFEST_HEADER_BYTES
            + (self.entries.len() * MANIFEST_ENTRY_BYTES)
            + self
                .entries
                .iter()
                .map(|entry| usize::try_from(entry.encoded_bytes).unwrap_or(usize::MAX))
                .sum::<usize>()
    }

    fn evict_to_limits(&mut self) -> Result<Vec<ManifestEntry>, SupportJournalArchiveError> {
        let mut removed = Vec::new();
        while self.record_count() > MAX_SUPPORT_JOURNAL_DURABLE_RECORDS
            || self.encoded_bytes() > MAX_SUPPORT_JOURNAL_DURABLE_BYTES
        {
            if self.entries.len() == 1 {
                return Err(SupportJournalArchiveError::Capacity);
            }
            let entry = self.entries.remove(0);
            self.base_head = entry.chain_head;
            self.evicted_records = self
                .evicted_records
                .saturating_add(u32::from(entry.record_count));
            removed.push(entry);
        }
        Ok(removed)
    }

    fn is_expired_from(&self, authenticated_last_minute: Option<u64>, now: u64) -> bool {
        authenticated_last_minute.is_some_and(|last| {
            now.saturating_sub(last) >= MAX_SUPPORT_JOURNAL_RETENTION_DAYS * MINUTES_PER_DAY
        })
    }

    fn encode(&self) -> Result<Vec<u8>, SupportJournalArchiveError> {
        let entry_count =
            u16::try_from(self.entries.len()).map_err(|_| SupportJournalArchiveError::Capacity)?;
        let mut bytes =
            Vec::with_capacity(MANIFEST_HEADER_BYTES + self.entries.len() * MANIFEST_ENTRY_BYTES);
        bytes.extend_from_slice(MANIFEST_MAGIC);
        bytes.extend_from_slice(&MANIFEST_VERSION.to_be_bytes());
        bytes.extend_from_slice(
            &self
                .epoch
                .map_or([0; 16], SupportJournalSessionEpoch::into_bytes),
        );
        bytes.extend_from_slice(&entry_count.to_be_bytes());
        bytes.extend_from_slice(&self.evicted_records.to_be_bytes());
        bytes.extend_from_slice(
            &u32::try_from(self.encoded_bytes())
                .map_err(|_| SupportJournalArchiveError::Capacity)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&self.base_head);
        bytes.extend_from_slice(&self.latest_head);
        bytes.extend_from_slice(&self.last_event_minute.unwrap_or(u64::MAX).to_be_bytes());
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.first_sequence.to_be_bytes());
            bytes.extend_from_slice(&entry.last_sequence.to_be_bytes());
            bytes.extend_from_slice(&entry.record_count.to_be_bytes());
            bytes.extend_from_slice(&entry.encoded_bytes.to_be_bytes());
            bytes.extend_from_slice(&entry.chain_head);
            bytes.extend_from_slice(&entry.last_event_minute.unwrap_or(u64::MAX).to_be_bytes());
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, SupportJournalArchiveError> {
        if bytes.len() < MANIFEST_HEADER_BYTES || bytes.get(..8) != Some(MANIFEST_MAGIC) {
            return Err(SupportJournalArchiveError::Integrity);
        }
        let version = read_u16(bytes, 8)?;
        let epoch_bytes = read_array::<16>(bytes, 10)?;
        let entry_count = usize::from(read_u16(bytes, 26)?);
        let evicted_records = read_u32(bytes, 28)?;
        let claimed_bytes = usize::try_from(read_u32(bytes, 32)?)
            .map_err(|_| SupportJournalArchiveError::Integrity)?;
        let base_head = read_array(bytes, 36)?;
        let latest_head = read_array(bytes, 68)?;
        let last_event_minute = decode_minute(read_u64(bytes, 100)?);
        if version != MANIFEST_VERSION
            || bytes.len() != MANIFEST_HEADER_BYTES + entry_count * MANIFEST_ENTRY_BYTES
            || entry_count > MAX_SUPPORT_JOURNAL_DURABLE_RECORDS
        {
            return Err(SupportJournalArchiveError::Integrity);
        }
        let mut entries = Vec::with_capacity(entry_count);
        let mut offset = MANIFEST_HEADER_BYTES;
        for _ in 0..entry_count {
            entries.push(ManifestEntry {
                first_sequence: read_u64(bytes, offset)?,
                last_sequence: read_u64(bytes, offset + 8)?,
                record_count: read_u16(bytes, offset + 16)?,
                encoded_bytes: read_u32(bytes, offset + 18)?,
                chain_head: read_array(bytes, offset + 22)?,
                last_event_minute: decode_minute(read_u64(bytes, offset + 54)?),
            });
            offset += MANIFEST_ENTRY_BYTES;
        }
        let manifest = Self {
            epoch: (epoch_bytes != [0; 16])
                .then(|| SupportJournalSessionEpoch::from_random_bytes(epoch_bytes)),
            base_head,
            latest_head,
            evicted_records,
            last_event_minute,
            entries,
        };
        if manifest.encoded_bytes() != claimed_bytes
            || manifest.record_count() > MAX_SUPPORT_JOURNAL_DURABLE_RECORDS
            || manifest.encoded_bytes() > MAX_SUPPORT_JOURNAL_DURABLE_BYTES
            || manifest
                .entries
                .windows(2)
                .any(|pair| pair[0].last_sequence >= pair[1].first_sequence)
            || manifest.entries.iter().any(|entry| {
                entry.record_count == 0
                    || entry.first_sequence > entry.last_sequence
                    || usize::try_from(entry.encoded_bytes).unwrap_or(usize::MAX)
                        > MAX_SEALED_SUPPORT_JOURNAL_SEGMENT_BYTES
            })
            || (manifest.entries.is_empty()
                && (manifest.epoch.is_some() || manifest.latest_head != manifest.base_head))
        {
            return Err(SupportJournalArchiveError::Integrity);
        }
        Ok(manifest)
    }
}

fn decode_minute(value: u64) -> Option<u64> {
    (value != u64::MAX).then_some(value)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, SupportJournalArchiveError> {
    Ok(u16::from_be_bytes(read_array(bytes, offset)?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, SupportJournalArchiveError> {
    Ok(u32::from_be_bytes(read_array(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, SupportJournalArchiveError> {
    Ok(u64::from_be_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(
    bytes: &[u8],
    offset: usize,
) -> Result<[u8; N], SupportJournalArchiveError> {
    bytes
        .get(offset..offset + N)
        .and_then(|value| value.try_into().ok())
        .ok_or(SupportJournalArchiveError::Integrity)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use oxid_diagnostics_application::{
        SupportJournalCode, SupportJournalOutcome, SupportJournalSeverity, SupportJournalStage,
        SupportJournalSubsystem,
    };
    use zeroize::Zeroizing;

    use super::*;
    use crate::SupportJournalSegmentKey;

    static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn scratch(label: &str) -> PathBuf {
        let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "oxid-support-journal-{}-{label}-{sequence}",
            std::process::id()
        ))
    }

    fn codec() -> SupportJournalSegmentCodec {
        SupportJournalSegmentCodec::new(SupportJournalSegmentKey::from_protected_bytes(
            Zeroizing::new([9; 32]),
        ))
    }

    fn event(sequence: u64, minute: u64) -> SupportJournalEvent {
        SupportJournalEvent::new(
            sequence,
            SupportJournalSubsystem::Runtime,
            SupportJournalCode::OperationTerminal,
            SupportJournalSeverity::Info,
            SupportJournalStage::Terminal,
            SupportJournalOutcome::Succeeded,
            SupportJournalSessionEpoch::from_random_bytes([7; 16]),
            None,
            Some(minute),
        )
    }

    fn append(
        store: &SupportJournalArchiveStore,
        codec: &SupportJournalSegmentCodec,
        previous: [u8; CHAIN_HEAD_BYTES],
        first_sequence: u64,
        count: usize,
    ) -> [u8; CHAIN_HEAD_BYTES] {
        let events = (0..count)
            .map(|offset| event(first_sequence + u64::try_from(offset).expect("offset"), 100))
            .collect::<Vec<_>>();
        let sealed = codec.seal(previous, &events).expect("seal");
        store.append(codec, previous, &sealed).expect("append")
    }

    #[test]
    fn append_and_recover_preserve_order_and_protected_head() {
        let root = scratch("round-trip");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let first = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 2);
        let latest = append(&store, &codec, first, 3, 2);

        let recovered = store.recover(&codec, latest, 101).expect("recover");
        assert_eq!(
            recovered
                .events()
                .iter()
                .map(|event| event.sequence())
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(recovered.latest_head(), latest);
        assert_eq!(recovered.evicted_records(), 0);
        assert_eq!(
            store.recover(&codec, [8; 32], 101),
            Err(SupportJournalArchiveError::Integrity)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn append_rejects_a_rolled_back_manifest() {
        let root = scratch("rollback");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let first = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let old_manifest = fs::read(root.join(MANIFEST_NAME)).expect("old manifest");
        let latest = append(&store, &codec, first, 2, 1);
        write_owner_private(&root.join(MANIFEST_NAME), &old_manifest).expect("rollback fixture");
        let sealed = codec.seal(latest, &[event(3, 100)]).expect("seal");

        assert_eq!(
            store.append(&codec, latest, &sealed),
            Err(SupportJournalArchiveError::Integrity)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manifest_disagreement_never_quarantines_valid_ciphertext() {
        let root = scratch("manifest-disagreement");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let latest = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let manifest_path = root.join(MANIFEST_NAME);
        let mut manifest = fs::read(&manifest_path).expect("manifest");
        manifest[MANIFEST_HEADER_BYTES + 16..MANIFEST_HEADER_BYTES + 18]
            .copy_from_slice(&2_u16.to_be_bytes());
        write_owner_private(&manifest_path, &manifest).expect("tamper manifest");

        assert_eq!(
            store.recover(&codec, latest, 101),
            Err(SupportJournalArchiveError::Integrity)
        );
        assert!(store.segment_path(1, 1).is_file());
        assert!(
            !store
                .segment_path(1, 1)
                .with_extension("quarantine")
                .exists()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unauthenticated_expiry_edit_never_deletes_segments() {
        let root = scratch("expiry-edit");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let latest = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let manifest_path = root.join(MANIFEST_NAME);
        let mut manifest = fs::read(&manifest_path).expect("manifest");
        manifest[100..108].copy_from_slice(&0_u64.to_be_bytes());
        write_owner_private(&manifest_path, &manifest).expect("tamper expiry");

        assert_eq!(
            store.recover(
                &codec,
                latest,
                100 + MAX_SUPPORT_JOURNAL_RETENTION_DAYS * MINUTES_PER_DAY,
            ),
            Err(SupportJournalArchiveError::Integrity)
        );
        assert!(store.segment_path(1, 1).is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_head_commit_rolls_back_to_the_protected_manifest() {
        let root = scratch("head-commit");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let protected = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let _uncommitted = append(&store, &codec, protected, 2, 1);

        let recovered = store
            .recover(&codec, protected, 101)
            .expect("rollback uncommitted append");
        assert_eq!(recovered.latest_head(), protected);
        assert_eq!(recovered.events()[0].sequence(), 1);
        assert!(!store.segment_path(2, 2).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn backwards_clock_does_not_regress_archive_retention() {
        let root = scratch("clock-rollback");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let first = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let sealed = codec.seal(first, &[event(2, 90)]).expect("seal");
        let latest = store.append(&codec, first, &sealed).expect("append");

        let recovered = store
            .recover(
                &codec,
                latest,
                100 + MAX_SUPPORT_JOURNAL_RETENTION_DAYS * MINUTES_PER_DAY - 1,
            )
            .expect("not expired");
        assert_eq!(recovered.events().len(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn permissive_orphan_is_degraded_hygiene_not_archive_failure() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = scratch("permissive-orphan");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let latest = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let orphan = store.segment_path(80, 80);
        fs::write(&orphan, b"not-owned-by-the-archive").expect("orphan");
        fs::set_permissions(&orphan, fs::Permissions::from_mode(0o644)).expect("permissions");

        let recovered = store.recover(&codec, latest, 101).expect("valid archive");
        assert!(recovered.is_degraded());
        assert_eq!(recovered.events()[0].sequence(), 1);
        assert!(orphan.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corruption_quarantines_only_the_failed_segment() {
        let root = scratch("corruption");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let first = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let latest = append(&store, &codec, first, 2, 1);
        let corrupt = store.segment_path(2, 2);
        let mut bytes = fs::read(&corrupt).expect("segment");
        let last = bytes.last_mut().expect("ciphertext");
        *last ^= 1;
        fs::write(&corrupt, bytes).expect("tamper");

        let recovered = store
            .recover(&codec, latest, 101)
            .expect("degraded recovery");
        assert!(recovered.is_degraded());
        assert_eq!(recovered.events()[0].sequence(), 1);
        assert!(corrupt.with_extension("quarantine").is_file());
        let recovered = store
            .recover(&codec, recovered.latest_head(), 101)
            .expect("repaired archive");
        assert_eq!(recovered.events()[0].sequence(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn wrong_key_never_quarantines_the_first_segment() {
        let root = scratch("wrong-key");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let latest = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let wrong = SupportJournalSegmentCodec::new(
            SupportJournalSegmentKey::from_protected_bytes(Zeroizing::new([3; 32])),
        );

        assert_eq!(
            store.recover(&wrong, latest, 101),
            Err(SupportJournalArchiveError::Integrity)
        );
        assert!(store.segment_path(1, 1).is_file());
        assert!(
            !store
                .segment_path(1, 1)
                .with_extension("quarantine")
                .exists()
        );
        assert_eq!(
            store
                .recover(&codec, latest, 101)
                .expect("correct key")
                .events()[0]
                .sequence(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_segment_truncates_the_unrecoverable_suffix() {
        let root = scratch("missing");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let first = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let latest = append(&store, &codec, first, 2, 1);
        fs::remove_file(store.segment_path(2, 2)).expect("remove second segment");

        let recovered = store
            .recover(&codec, latest, 101)
            .expect("degraded recovery");
        assert!(recovered.is_degraded());
        assert_eq!(recovered.events()[0].sequence(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn expiry_clears_segments_on_first_activation() {
        let root = scratch("expiry");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let latest = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let expired_at = 100 + MAX_SUPPORT_JOURNAL_RETENTION_DAYS * MINUTES_PER_DAY;
        let recovered = store
            .recover(&codec, latest, expired_at)
            .expect("expiry is a closed empty outcome");
        assert!(recovered.events().is_empty());
        assert!(!store.segment_path(1, 1).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn expiry_removes_quarantined_ciphertext() {
        let root = scratch("quarantine-expiry");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let first = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let latest = append(&store, &codec, first, 2, 1);
        let corrupt = store.segment_path(2, 2);
        let mut bytes = fs::read(&corrupt).expect("segment");
        *bytes.last_mut().expect("ciphertext") ^= 1;
        fs::write(&corrupt, bytes).expect("tamper");
        let recovered = store.recover(&codec, latest, 101).expect("quarantine");
        let quarantine = corrupt.with_extension("quarantine");
        assert!(quarantine.exists());

        store
            .recover(
                &codec,
                recovered.latest_head(),
                100 + MAX_SUPPORT_JOURNAL_RETENTION_DAYS * MINUTES_PER_DAY,
            )
            .expect("expiry");
        assert!(!quarantine.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_orphan_is_removed_but_unknown_files_are_untouched() {
        let root = scratch("orphan");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let latest = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let orphan = store.segment_path(90, 90);
        write_owner_private(&orphan, b"interrupted").expect("orphan fixture");
        let unrelated = root.join("keep.txt");
        write_owner_private(&unrelated, b"owner data").expect("unrelated fixture");

        store.recover(&codec, latest, 101).expect("recover");
        assert!(!orphan.exists());
        assert!(unrelated.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_segment_is_rejected_without_touching_target() {
        use std::os::unix::fs::symlink;

        let root = scratch("symlink");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let latest = append(&store, &codec, [0; CHAIN_HEAD_BYTES], 1, 1);
        let segment = store.segment_path(1, 1);
        fs::remove_file(&segment).expect("remove segment");
        let target = root.join("target");
        write_owner_private(&target, b"sentinel").expect("target");
        symlink(&target, &segment).expect("symlink");

        assert_eq!(
            store.recover(&codec, latest, 101),
            Err(SupportJournalArchiveError::Integrity)
        );
        assert_eq!(fs::read(target).expect("target remains"), b"sentinel");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn record_limit_evicts_oldest_segments_deterministically() {
        let root = scratch("eviction");
        let store = SupportJournalArchiveStore::new(root.clone());
        let codec = codec();
        let mut head = [0; CHAIN_HEAD_BYTES];
        let mut sequence = 1;
        for _ in 0..=(MAX_SUPPORT_JOURNAL_DURABLE_RECORDS / 32) {
            head = append(&store, &codec, head, sequence, 32);
            sequence += 32;
        }

        let recovered = store.recover(&codec, head, 101).expect("recover");
        assert_eq!(
            recovered.events().len(),
            MAX_SUPPORT_JOURNAL_DURABLE_RECORDS
        );
        assert_eq!(recovered.events()[0].sequence(), 33);
        assert_eq!(recovered.evicted_records(), 32);
        assert!(!store.segment_path(1, 32).exists());
        let _ = fs::remove_dir_all(root);
    }
}
