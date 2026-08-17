// SPDX-License-Identifier: Apache-2.0

//! Strict adapter-private format for portable wallet custody.
//!
//! Version and algorithm parameters are authenticated as associated data.
//! Plaintext custody values are zeroized on every normal return path and never
//! implement a secret-revealing `Debug` representation.

#![forbid(unsafe_code)]

use std::{collections::BTreeSet, fmt};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    KeyInit as _, XChaCha20Poly1305, XNonce,
    aead::{Aead as _, Key, Payload},
};
use oxid_foundation::UnixTimestampMillis;
use oxid_platform_ports::RandomPort;
use oxid_wallet_application::{
    PortableWalletBackup, WalletHdPath, WalletHdPathComponent, WalletPortableBackupPortError,
    WalletRecoverySecret,
};
use oxid_wallet_domain::{
    PublicKeyEncoding, WalletKeyAlgorithm, WalletKeyDescriptor, WalletKeyLabel, WalletKeyPurpose,
    WalletKeyReference, WalletProfileId, WalletPublicKey,
};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq as _;
use zeroize::{Zeroize as _, Zeroizing};

const MAGIC: &[u8; 8] = b"OXIDBAK1";
const CUSTODY_FORMAT_VERSION: u16 = 1;
const COMPLETE_WALLET_FORMAT_VERSION: u16 = 2;
const KDF_ARGON2ID: u8 = 1;
const AEAD_XCHACHA20_POLY1305: u8 = 1;
const ARGON2_MEMORY_KIB: u32 = 19 * 1024;
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_LANES: u32 = 1;
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 24;
const TAG_BYTES: usize = 16;
const HEADER_BYTES: usize = 8 + 2 + 1 + 1 + 4 + 4 + 4 + SALT_BYTES + NONCE_BYTES + 4;
const MAX_KEYS: usize = 256;
const MAX_PUBLIC_KEY_BYTES: usize = 128;
const COMPLETE_FRAME_MAGIC: &[u8; 8] = b"OXIDALL1";
const COMPLETE_FRAME_VERSION: u16 = 1;
const COMPLETE_FRAME_FIXED_BYTES: usize = COMPLETE_FRAME_MAGIC.len() + 2 + 2 + (4 * 4);
const MAX_ARCHIVE_PROFILE_ID_BYTES: usize = 128;
/// Independent maximum for the canonical public profile section.
pub const MAX_PORTABLE_PROFILE_SNAPSHOT_BYTES: usize = 1024 * 1024;
/// Independent maximum for the canonical public DID section.
pub const MAX_PORTABLE_DID_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
/// Independent maximum for the canonical complete credential section.
pub const MAX_PORTABLE_CREDENTIAL_SNAPSHOT_BYTES: usize = 67_174_400;
/// Independent maximum for the adapter-private custody section.
pub const MAX_PORTABLE_CUSTODY_SNAPSHOT_BYTES: usize = 1024 * 1024;

/// One protected key retained in an opened, adapter-private custody package.
pub struct PortableCustodyKey {
    descriptor: WalletKeyDescriptor,
    material: PortableKeyMaterial,
}

impl PortableCustodyKey {
    #[must_use]
    pub const fn generated(descriptor: WalletKeyDescriptor, secret: [u8; 32]) -> Self {
        Self {
            descriptor,
            material: PortableKeyMaterial::Generated(secret),
        }
    }

    #[must_use]
    pub const fn derived(descriptor: WalletKeyDescriptor, path: WalletHdPath) -> Self {
        Self {
            descriptor,
            material: PortableKeyMaterial::Derived(path),
        }
    }

    #[must_use]
    pub const fn descriptor(&self) -> &WalletKeyDescriptor {
        &self.descriptor
    }

    #[must_use]
    pub const fn material(&self) -> PortableKeyMaterialRef<'_> {
        match &self.material {
            PortableKeyMaterial::Generated(secret) => PortableKeyMaterialRef::Generated(secret),
            PortableKeyMaterial::Derived(path) => PortableKeyMaterialRef::Derived(path),
        }
    }
}

impl fmt::Debug for PortableCustodyKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortableCustodyKey")
            .field("reference", &self.descriptor.reference())
            .field("algorithm", &self.descriptor.algorithm())
            .field("material", &"[REDACTED]")
            .finish()
    }
}

enum PortableKeyMaterial {
    Generated([u8; 32]),
    Derived(WalletHdPath),
}

impl Drop for PortableKeyMaterial {
    fn drop(&mut self) {
        if let Self::Generated(secret) = self {
            secret.zeroize();
        }
    }
}

#[derive(Clone, Copy)]
pub enum PortableKeyMaterialRef<'a> {
    Generated(&'a [u8; 32]),
    Derived(&'a WalletHdPath),
}

impl fmt::Debug for PortableKeyMaterialRef<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Generated(_) => "Generated([REDACTED])",
            Self::Derived(_) => "Derived([PUBLIC PATH])",
        })
    }
}

/// Fully authenticated plaintext custody state. It remains inside custody adapters.
pub struct PortableCustodyVault {
    profile_id: WalletProfileId,
    exported_at_millis: u64,
    root_seed: Zeroizing<[u8; 32]>,
    keys: Vec<PortableCustodyKey>,
}

impl PortableCustodyVault {
    pub fn new(
        profile_id: WalletProfileId,
        exported_at_millis: u64,
        root_seed: [u8; 32],
        keys: Vec<PortableCustodyKey>,
    ) -> Result<Self, WalletPortableBackupPortError> {
        let root_seed = Zeroizing::new(root_seed);
        if keys.len() > MAX_KEYS {
            return Err(WalletPortableBackupPortError::InvalidPackage);
        }
        let mut references = BTreeSet::new();
        let mut labels = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for key in &keys {
            if !references.insert(key.descriptor.reference().as_str())
                || !labels.insert(key.descriptor.label().as_str())
            {
                return Err(WalletPortableBackupPortError::Conflict);
            }
            if let PortableKeyMaterial::Derived(path) = &key.material
                && !paths.insert(path)
            {
                return Err(WalletPortableBackupPortError::Conflict);
            }
        }
        Ok(Self {
            profile_id,
            exported_at_millis,
            root_seed,
            keys,
        })
    }

    #[must_use]
    pub const fn profile_id(&self) -> &WalletProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn exported_at_millis(&self) -> u64 {
        self.exported_at_millis
    }

    #[must_use]
    pub fn root_seed(&self) -> &[u8; 32] {
        &self.root_seed
    }

    #[must_use]
    pub fn keys(&self) -> &[PortableCustodyKey] {
        &self.keys
    }

    /// Compares authenticated custody content while keeping secret byte
    /// comparisons constant-time. Export timestamps are intentionally ignored.
    #[must_use]
    pub fn matches_recovered_state(&self, other: &Self) -> bool {
        if self.profile_id != other.profile_id || self.keys.len() != other.keys.len() {
            return false;
        }
        let mut matches = self.root_seed().ct_eq(other.root_seed());
        for key in &self.keys {
            let Some(candidate) = other
                .keys
                .iter()
                .find(|candidate| candidate.descriptor.reference() == key.descriptor.reference())
            else {
                return false;
            };
            if candidate.descriptor != key.descriptor {
                return false;
            }
            matches &= match (key.material(), candidate.material()) {
                (
                    PortableKeyMaterialRef::Generated(left),
                    PortableKeyMaterialRef::Generated(right),
                ) => left.ct_eq(right),
                (PortableKeyMaterialRef::Derived(left), PortableKeyMaterialRef::Derived(right)) => {
                    subtle::Choice::from(u8::from(left == right))
                }
                _ => subtle::Choice::from(0),
            };
        }
        bool::from(matches)
    }
}

impl fmt::Debug for PortableCustodyVault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortableCustodyVault")
            .field("profile_id", &self.profile_id)
            .field("exported_at_millis", &self.exported_at_millis)
            .field("root_seed", &"[REDACTED]")
            .field("key_count", &self.keys.len())
            .finish()
    }
}

/// Fully authenticated complete-wallet state below every incoming adapter.
///
/// Repository snapshots are canonical domain encodings, not copies of their
/// live files. Credential bytes and custody material are zeroized on drop.
pub struct CompleteWalletArchive {
    profile_id: WalletProfileId,
    profile_snapshot: Vec<u8>,
    did_snapshot: Vec<u8>,
    credential_snapshot: Zeroizing<Vec<u8>>,
    custody: PortableCustodyVault,
}

impl CompleteWalletArchive {
    pub fn new(
        profile_id: WalletProfileId,
        profile_snapshot: Vec<u8>,
        did_snapshot: Vec<u8>,
        credential_snapshot: Zeroizing<Vec<u8>>,
        custody: PortableCustodyVault,
    ) -> Result<Self, WalletPortableBackupPortError> {
        if custody.profile_id() != &profile_id {
            return Err(WalletPortableBackupPortError::WrongProfile);
        }
        validate_section(&profile_snapshot, MAX_PORTABLE_PROFILE_SNAPSHOT_BYTES)?;
        validate_section(&did_snapshot, MAX_PORTABLE_DID_SNAPSHOT_BYTES)?;
        validate_section(&credential_snapshot, MAX_PORTABLE_CREDENTIAL_SNAPSHOT_BYTES)?;
        Ok(Self {
            profile_id,
            profile_snapshot,
            did_snapshot,
            credential_snapshot,
            custody,
        })
    }

    #[must_use]
    pub const fn profile_id(&self) -> &WalletProfileId {
        &self.profile_id
    }

    #[must_use]
    pub fn profile_snapshot(&self) -> &[u8] {
        &self.profile_snapshot
    }

    #[must_use]
    pub fn did_snapshot(&self) -> &[u8] {
        &self.did_snapshot
    }

    #[must_use]
    pub fn credential_snapshot(&self) -> &[u8] {
        &self.credential_snapshot
    }

    #[must_use]
    pub const fn custody(&self) -> &PortableCustodyVault {
        &self.custody
    }
}

impl fmt::Debug for CompleteWalletArchive {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompleteWalletArchive")
            .field("profile_id", &self.profile_id)
            .field("profile_snapshot_bytes", &self.profile_snapshot.len())
            .field("did_snapshot_bytes", &self.did_snapshot.len())
            .field("credential_snapshot_bytes", &self.credential_snapshot.len())
            .field("custody", &"[REDACTED]")
            .finish()
    }
}

/// Adapter-to-adapter custody boundary used by the complete-wallet coordinator.
///
/// This trait never crosses an incoming adapter. It deliberately carries the
/// opened custody vault so the complete archive can use one KDF/AEAD envelope
/// rather than nesting the legacy encrypted custody package inside another.
pub trait PortableCustodyVaultPort: Send + Sync {
    fn export_custody_vault(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<PortableCustodyVault, WalletPortableBackupPortError>;

    /// Checks destination emptiness and validates every restored key without
    /// mutating custody. Recovery repeats these checks at its commit boundary.
    fn preflight_custody_recovery(
        &self,
        vault: &PortableCustodyVault,
    ) -> Result<oxid_wallet_application::WalletPortableRecoverySummary, WalletPortableBackupPortError>;

    /// Performs the one-shot custody initialization after public state stages.
    fn recover_custody_vault(
        &self,
        vault: &PortableCustodyVault,
    ) -> Result<oxid_wallet_application::WalletPortableRecoverySummary, WalletPortableBackupPortError>;

    /// Confirms that committed custody exactly matches an authenticated
    /// archive. Implementations may require a fresh platform authorization.
    fn verify_recovered_custody(
        &self,
        vault: &PortableCustodyVault,
    ) -> Result<oxid_wallet_application::WalletPortableRecoverySummary, WalletPortableBackupPortError>;
}

/// Encrypt one validated custody vault with fixed Argon2id and XChaCha20-Poly1305.
pub fn seal_portable_custody(
    vault: &PortableCustodyVault,
    recovery_secret: &WalletRecoverySecret,
    random: &dyn RandomPort,
) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
    let plaintext = encode_custody(vault)?;
    seal_payload(CUSTODY_FORMAT_VERSION, &plaintext, recovery_secret, random)
}

/// Authenticate, decrypt, strictly decode, and profile-bind a custody package.
pub fn open_portable_custody(
    backup: &PortableWalletBackup,
    recovery_secret: &WalletRecoverySecret,
    expected_profile_id: &WalletProfileId,
) -> Result<PortableCustodyVault, WalletPortableBackupPortError> {
    let plaintext = open_payload(backup, recovery_secret, CUSTODY_FORMAT_VERSION)?;
    let vault = decode_custody(&plaintext)?;
    if vault.profile_id() != expected_profile_id {
        return Err(WalletPortableBackupPortError::WrongProfile);
    }
    Ok(vault)
}

/// Encrypt one complete wallet archive under a single authenticated envelope.
pub fn seal_complete_wallet_archive(
    archive: &CompleteWalletArchive,
    recovery_secret: &WalletRecoverySecret,
    random: &dyn RandomPort,
) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
    let plaintext = encode_complete_archive(archive)?;
    seal_payload(
        COMPLETE_WALLET_FORMAT_VERSION,
        &plaintext,
        recovery_secret,
        random,
    )
}

/// Authenticate and strictly decode one complete wallet archive.
///
/// Fresh-install recovery passes `None` and learns the destination profile only
/// from the authenticated plaintext. Existing-profile flows bind the exact
/// expected identifier with `Some`.
pub fn open_complete_wallet_archive(
    backup: &PortableWalletBackup,
    recovery_secret: &WalletRecoverySecret,
    expected_profile_id: Option<&WalletProfileId>,
) -> Result<CompleteWalletArchive, WalletPortableBackupPortError> {
    let plaintext = open_payload(backup, recovery_secret, COMPLETE_WALLET_FORMAT_VERSION)?;
    let archive = decode_complete_archive(&plaintext)?;
    if expected_profile_id.is_some_and(|expected| archive.profile_id() != expected) {
        return Err(WalletPortableBackupPortError::WrongProfile);
    }
    Ok(archive)
}

fn seal_payload(
    format_version: u16,
    plaintext: &[u8],
    recovery_secret: &WalletRecoverySecret,
    random: &dyn RandomPort,
) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
    let mut salt = [0_u8; SALT_BYTES];
    let mut nonce = [0_u8; NONCE_BYTES];
    random
        .fill_bytes(&mut salt)
        .map_err(|_| WalletPortableBackupPortError::Unavailable)?;
    random
        .fill_bytes(&mut nonce)
        .map_err(|_| WalletPortableBackupPortError::Unavailable)?;

    let ciphertext_len = plaintext
        .len()
        .checked_add(TAG_BYTES)
        .and_then(|length| u32::try_from(length).ok())
        .ok_or(WalletPortableBackupPortError::InvalidOperation)?;
    let header = encode_header(format_version, &salt, &nonce, ciphertext_len);
    if HEADER_BYTES + ciphertext_len as usize
        > oxid_wallet_application::MAX_PORTABLE_WALLET_BACKUP_BYTES
    {
        return Err(WalletPortableBackupPortError::InvalidOperation);
    }

    let key = derive_key(recovery_secret, &salt)?;
    let key = Key::<XChaCha20Poly1305>::try_from(key.as_slice())
        .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;
    let cipher = XChaCha20Poly1305::new(&key);
    let nonce = XNonce::try_from(nonce.as_slice())
        .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &header,
            },
        )
        .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;

    let mut package = Vec::with_capacity(HEADER_BYTES + ciphertext.len());
    package.extend_from_slice(&header);
    package.extend_from_slice(&ciphertext);
    PortableWalletBackup::parse(package)
        .map_err(|_| WalletPortableBackupPortError::InvalidOperation)
}

fn open_payload(
    backup: &PortableWalletBackup,
    recovery_secret: &WalletRecoverySecret,
    expected_format_version: u16,
) -> Result<Zeroizing<Vec<u8>>, WalletPortableBackupPortError> {
    let bytes = backup.as_bytes();
    let header = decode_header(bytes)?;
    if header.format_version != expected_format_version {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    let key = derive_key(recovery_secret, &header.salt)?;
    let key = Key::<XChaCha20Poly1305>::try_from(key.as_slice())
        .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;
    let cipher = XChaCha20Poly1305::new(&key);
    let nonce = XNonce::try_from(header.nonce.as_slice())
        .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?;
    cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &bytes[HEADER_BYTES..],
                aad: &bytes[..HEADER_BYTES],
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| WalletPortableBackupPortError::AuthenticationFailed)
}

fn encode_custody(
    vault: &PortableCustodyVault,
) -> Result<Zeroizing<Vec<u8>>, WalletPortableBackupPortError> {
    let wire = WireVault::from_vault(vault);
    let bytes = Zeroizing::new(
        serde_json::to_vec(&wire).map_err(|_| WalletPortableBackupPortError::InvalidOperation)?,
    );
    validate_section(&bytes, MAX_PORTABLE_CUSTODY_SNAPSHOT_BYTES)?;
    Ok(bytes)
}

fn decode_custody(bytes: &[u8]) -> Result<PortableCustodyVault, WalletPortableBackupPortError> {
    validate_section(bytes, MAX_PORTABLE_CUSTODY_SNAPSHOT_BYTES)?;
    let wire: WireVault =
        serde_json::from_slice(bytes).map_err(|_| WalletPortableBackupPortError::InvalidPackage)?;
    wire.to_vault()
}

fn encode_complete_archive(
    archive: &CompleteWalletArchive,
) -> Result<Zeroizing<Vec<u8>>, WalletPortableBackupPortError> {
    let custody = encode_custody(archive.custody())?;
    let profile_id = archive.profile_id().as_str().as_bytes();
    if profile_id.is_empty() || profile_id.len() > MAX_ARCHIVE_PROFILE_ID_BYTES {
        return Err(WalletPortableBackupPortError::InvalidOperation);
    }
    let profile_id_len = u16::try_from(profile_id.len())
        .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;
    let section_lengths = [
        section_length(archive.profile_snapshot())?,
        section_length(archive.did_snapshot())?,
        section_length(archive.credential_snapshot())?,
        section_length(&custody)?,
    ];
    let capacity = COMPLETE_FRAME_FIXED_BYTES
        .checked_add(profile_id.len())
        .and_then(|length| length.checked_add(archive.profile_snapshot().len()))
        .and_then(|length| length.checked_add(archive.did_snapshot().len()))
        .and_then(|length| length.checked_add(archive.credential_snapshot().len()))
        .and_then(|length| length.checked_add(custody.len()))
        .ok_or(WalletPortableBackupPortError::InvalidOperation)?;
    if capacity + HEADER_BYTES + TAG_BYTES
        > oxid_wallet_application::MAX_PORTABLE_WALLET_BACKUP_BYTES
    {
        return Err(WalletPortableBackupPortError::InvalidOperation);
    }
    let mut plaintext = Zeroizing::new(Vec::with_capacity(capacity));
    plaintext.extend_from_slice(COMPLETE_FRAME_MAGIC);
    plaintext.extend_from_slice(&COMPLETE_FRAME_VERSION.to_be_bytes());
    plaintext.extend_from_slice(&profile_id_len.to_be_bytes());
    for length in section_lengths {
        plaintext.extend_from_slice(&length.to_be_bytes());
    }
    plaintext.extend_from_slice(profile_id);
    plaintext.extend_from_slice(archive.profile_snapshot());
    plaintext.extend_from_slice(archive.did_snapshot());
    plaintext.extend_from_slice(archive.credential_snapshot());
    plaintext.extend_from_slice(&custody);
    Ok(plaintext)
}

fn decode_complete_archive(
    plaintext: &[u8],
) -> Result<CompleteWalletArchive, WalletPortableBackupPortError> {
    if plaintext.len() < COMPLETE_FRAME_FIXED_BYTES
        || &plaintext[..COMPLETE_FRAME_MAGIC.len()] != COMPLETE_FRAME_MAGIC
    {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    let version = u16::from_be_bytes([plaintext[8], plaintext[9]]);
    if version != COMPLETE_FRAME_VERSION {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    let profile_id_len = usize::from(u16::from_be_bytes([plaintext[10], plaintext[11]]));
    if profile_id_len == 0 || profile_id_len > MAX_ARCHIVE_PROFILE_ID_BYTES {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    let mut section_lengths = [0_usize; 4];
    for (index, length) in section_lengths.iter_mut().enumerate() {
        let offset = 12 + (index * 4);
        *length = u32::from_be_bytes(
            plaintext[offset..offset + 4]
                .try_into()
                .expect("fixed complete-wallet header range"),
        ) as usize;
    }
    for (length, maximum) in section_lengths.iter().zip([
        MAX_PORTABLE_PROFILE_SNAPSHOT_BYTES,
        MAX_PORTABLE_DID_SNAPSHOT_BYTES,
        MAX_PORTABLE_CREDENTIAL_SNAPSHOT_BYTES,
        MAX_PORTABLE_CUSTODY_SNAPSHOT_BYTES,
    ]) {
        if *length == 0 || *length > maximum {
            return Err(WalletPortableBackupPortError::InvalidPackage);
        }
    }
    let expected_length = section_lengths
        .iter()
        .try_fold(
            COMPLETE_FRAME_FIXED_BYTES + profile_id_len,
            |total, length| total.checked_add(*length),
        )
        .ok_or(WalletPortableBackupPortError::InvalidPackage)?;
    if expected_length != plaintext.len() {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    let mut cursor = COMPLETE_FRAME_FIXED_BYTES;
    let profile_id_bytes = take_section(plaintext, &mut cursor, profile_id_len)?;
    let profile_id = std::str::from_utf8(profile_id_bytes)
        .map_err(|_| WalletPortableBackupPortError::InvalidPackage)
        .and_then(|value| {
            WalletProfileId::parse(value.to_owned())
                .map_err(|_| WalletPortableBackupPortError::InvalidPackage)
        })?;
    let profile_snapshot = take_section(plaintext, &mut cursor, section_lengths[0])?.to_vec();
    let did_snapshot = take_section(plaintext, &mut cursor, section_lengths[1])?.to_vec();
    let credential_snapshot =
        Zeroizing::new(take_section(plaintext, &mut cursor, section_lengths[2])?.to_vec());
    let custody = decode_custody(take_section(plaintext, &mut cursor, section_lengths[3])?)?;
    if cursor != plaintext.len() {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    CompleteWalletArchive::new(
        profile_id,
        profile_snapshot,
        did_snapshot,
        credential_snapshot,
        custody,
    )
    .map_err(|error| match error {
        WalletPortableBackupPortError::WrongProfile => {
            WalletPortableBackupPortError::InvalidPackage
        }
        other => other,
    })
}

fn section_length(bytes: &[u8]) -> Result<u32, WalletPortableBackupPortError> {
    u32::try_from(bytes.len()).map_err(|_| WalletPortableBackupPortError::InvalidOperation)
}

fn validate_section(bytes: &[u8], maximum: usize) -> Result<(), WalletPortableBackupPortError> {
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    Ok(())
}

fn take_section<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    length: usize,
) -> Result<&'a [u8], WalletPortableBackupPortError> {
    let end = cursor
        .checked_add(length)
        .ok_or(WalletPortableBackupPortError::InvalidPackage)?;
    let section = bytes
        .get(*cursor..end)
        .ok_or(WalletPortableBackupPortError::InvalidPackage)?;
    *cursor = end;
    Ok(section)
}

fn derive_key(
    recovery_secret: &WalletRecoverySecret,
    salt: &[u8; SALT_BYTES],
) -> Result<Zeroizing<[u8; 32]>, WalletPortableBackupPortError> {
    let params = Params::new(ARGON2_MEMORY_KIB, ARGON2_ITERATIONS, ARGON2_LANES, Some(32))
        .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0_u8; 32]);
    argon2
        .hash_password_into(
            recovery_secret.expose_to_backup_adapter(),
            salt,
            key.as_mut(),
        )
        .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;
    Ok(key)
}

fn encode_header(
    format_version: u16,
    salt: &[u8; SALT_BYTES],
    nonce: &[u8; NONCE_BYTES],
    ciphertext_len: u32,
) -> Vec<u8> {
    let mut header = Vec::with_capacity(HEADER_BYTES);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&format_version.to_be_bytes());
    header.push(KDF_ARGON2ID);
    header.push(AEAD_XCHACHA20_POLY1305);
    header.extend_from_slice(&ARGON2_MEMORY_KIB.to_be_bytes());
    header.extend_from_slice(&ARGON2_ITERATIONS.to_be_bytes());
    header.extend_from_slice(&ARGON2_LANES.to_be_bytes());
    header.extend_from_slice(salt);
    header.extend_from_slice(nonce);
    header.extend_from_slice(&ciphertext_len.to_be_bytes());
    header
}

struct DecodedHeader {
    format_version: u16,
    salt: [u8; SALT_BYTES],
    nonce: [u8; NONCE_BYTES],
}

fn decode_header(bytes: &[u8]) -> Result<DecodedHeader, WalletPortableBackupPortError> {
    if bytes.len() < HEADER_BYTES || &bytes[..MAGIC.len()] != MAGIC {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    let format_version = u16::from_be_bytes([bytes[8], bytes[9]]);
    let memory = u32::from_be_bytes(bytes[12..16].try_into().expect("fixed header range"));
    let iterations = u32::from_be_bytes(bytes[16..20].try_into().expect("fixed header range"));
    let lanes = u32::from_be_bytes(bytes[20..24].try_into().expect("fixed header range"));
    if !matches!(
        format_version,
        CUSTODY_FORMAT_VERSION | COMPLETE_WALLET_FORMAT_VERSION
    ) || bytes[10] != KDF_ARGON2ID
        || bytes[11] != AEAD_XCHACHA20_POLY1305
        || memory != ARGON2_MEMORY_KIB
        || iterations != ARGON2_ITERATIONS
        || lanes != ARGON2_LANES
    {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    let ciphertext_len = u32::from_be_bytes(
        bytes[HEADER_BYTES - 4..HEADER_BYTES]
            .try_into()
            .expect("fixed header range"),
    ) as usize;
    if ciphertext_len < TAG_BYTES || bytes.len() != HEADER_BYTES + ciphertext_len {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    let mut salt = [0_u8; SALT_BYTES];
    salt.copy_from_slice(&bytes[24..40]);
    let mut nonce = [0_u8; NONCE_BYTES];
    nonce.copy_from_slice(&bytes[40..64]);
    Ok(DecodedHeader {
        format_version,
        salt,
        nonce,
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireVault {
    profile_id: String,
    exported_at_millis: u64,
    root_seed: [u8; 32],
    keys: Vec<WireKey>,
}

impl Drop for WireVault {
    fn drop(&mut self) {
        self.root_seed.zeroize();
    }
}

impl WireVault {
    fn from_vault(vault: &PortableCustodyVault) -> Self {
        Self {
            profile_id: vault.profile_id.as_str().to_owned(),
            exported_at_millis: vault.exported_at_millis,
            root_seed: *vault.root_seed,
            keys: vault.keys.iter().map(WireKey::from_key).collect(),
        }
    }

    fn to_vault(&self) -> Result<PortableCustodyVault, WalletPortableBackupPortError> {
        let profile_id = WalletProfileId::parse(&self.profile_id)
            .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?;
        let keys = self
            .keys
            .iter()
            .map(WireKey::to_key)
            .collect::<Result<Vec<_>, _>>()?;
        PortableCustodyVault::new(profile_id, self.exported_at_millis, self.root_seed, keys)
            .map_err(|error| match error {
                WalletPortableBackupPortError::Conflict => {
                    WalletPortableBackupPortError::InvalidPackage
                }
                other => other,
            })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireKey {
    reference: String,
    label: String,
    algorithm: WireAlgorithm,
    purpose: WirePurpose,
    public_key_encoding: WirePublicKeyEncoding,
    public_key_bytes: Vec<u8>,
    created_at_millis: u64,
    material: WireMaterial,
}

impl WireKey {
    fn from_key(key: &PortableCustodyKey) -> Self {
        let descriptor = key.descriptor();
        Self {
            reference: descriptor.reference().as_str().to_owned(),
            label: descriptor.label().as_str().to_owned(),
            algorithm: descriptor.algorithm().into(),
            purpose: descriptor.purpose().into(),
            public_key_encoding: descriptor.public_key().encoding().into(),
            public_key_bytes: descriptor.public_key().bytes().to_vec(),
            created_at_millis: descriptor.created_at().value(),
            material: match key.material() {
                PortableKeyMaterialRef::Generated(secret) => {
                    WireMaterial::Generated { secret: *secret }
                }
                PortableKeyMaterialRef::Derived(path) => WireMaterial::Derived {
                    path: path
                        .components()
                        .iter()
                        .map(|component| WirePathComponent {
                            index: component.index(),
                            hardened: component.hardened(),
                        })
                        .collect(),
                },
            },
        }
    }

    fn to_key(&self) -> Result<PortableCustodyKey, WalletPortableBackupPortError> {
        if self.public_key_bytes.is_empty() || self.public_key_bytes.len() > MAX_PUBLIC_KEY_BYTES {
            return Err(WalletPortableBackupPortError::InvalidPackage);
        }
        let descriptor = WalletKeyDescriptor::new(
            WalletKeyReference::parse(&self.reference)
                .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?,
            WalletKeyLabel::parse(&self.label)
                .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?,
            self.algorithm.into(),
            self.purpose.into(),
            WalletPublicKey::new(
                self.public_key_encoding.into(),
                self.public_key_bytes.clone(),
            ),
            UnixTimestampMillis::new(self.created_at_millis),
        );
        match &self.material {
            WireMaterial::Generated { secret } => {
                Ok(PortableCustodyKey::generated(descriptor, *secret))
            }
            WireMaterial::Derived { path } => {
                let path = path
                    .iter()
                    .map(|component| {
                        WalletHdPathComponent::new(component.index, component.hardened)
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .and_then(WalletHdPath::new)
                    .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?;
                Ok(PortableCustodyKey::derived(descriptor, path))
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WireMaterial {
    Generated { secret: [u8; 32] },
    Derived { path: Vec<WirePathComponent> },
}

impl Drop for WireMaterial {
    fn drop(&mut self) {
        if let Self::Generated { secret } = self {
            secret.zeroize();
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePathComponent {
    index: u32,
    hardened: bool,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireAlgorithm {
    Ed25519,
    P256,
    Secp256k1Schnorr,
    Jubjub,
}

impl From<WalletKeyAlgorithm> for WireAlgorithm {
    fn from(value: WalletKeyAlgorithm) -> Self {
        match value {
            WalletKeyAlgorithm::Ed25519 => Self::Ed25519,
            WalletKeyAlgorithm::P256 => Self::P256,
            WalletKeyAlgorithm::Secp256k1Schnorr => Self::Secp256k1Schnorr,
            WalletKeyAlgorithm::Jubjub => Self::Jubjub,
        }
    }
}

impl From<WireAlgorithm> for WalletKeyAlgorithm {
    fn from(value: WireAlgorithm) -> Self {
        match value {
            WireAlgorithm::Ed25519 => Self::Ed25519,
            WireAlgorithm::P256 => Self::P256,
            WireAlgorithm::Secp256k1Schnorr => Self::Secp256k1Schnorr,
            WireAlgorithm::Jubjub => Self::Jubjub,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WirePurpose {
    Transaction,
    Authentication,
    Assertion,
    KeyAgreement,
    Recovery,
}

impl From<WalletKeyPurpose> for WirePurpose {
    fn from(value: WalletKeyPurpose) -> Self {
        match value {
            WalletKeyPurpose::Transaction => Self::Transaction,
            WalletKeyPurpose::Authentication => Self::Authentication,
            WalletKeyPurpose::Assertion => Self::Assertion,
            WalletKeyPurpose::KeyAgreement => Self::KeyAgreement,
            WalletKeyPurpose::Recovery => Self::Recovery,
        }
    }
}

impl From<WirePurpose> for WalletKeyPurpose {
    fn from(value: WirePurpose) -> Self {
        match value {
            WirePurpose::Transaction => Self::Transaction,
            WirePurpose::Authentication => Self::Authentication,
            WirePurpose::Assertion => Self::Assertion,
            WirePurpose::KeyAgreement => Self::KeyAgreement,
            WirePurpose::Recovery => Self::Recovery,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WirePublicKeyEncoding {
    Ed25519Compressed,
    Sec1Compressed,
    Secp256k1XOnly,
    JubjubCompressed,
}

impl From<PublicKeyEncoding> for WirePublicKeyEncoding {
    fn from(value: PublicKeyEncoding) -> Self {
        match value {
            PublicKeyEncoding::Ed25519Compressed => Self::Ed25519Compressed,
            PublicKeyEncoding::Sec1Compressed => Self::Sec1Compressed,
            PublicKeyEncoding::Secp256k1XOnly => Self::Secp256k1XOnly,
            PublicKeyEncoding::JubjubCompressed => Self::JubjubCompressed,
        }
    }
}

impl From<WirePublicKeyEncoding> for PublicKeyEncoding {
    fn from(value: WirePublicKeyEncoding) -> Self {
        match value {
            WirePublicKeyEncoding::Ed25519Compressed => Self::Ed25519Compressed,
            WirePublicKeyEncoding::Sec1Compressed => Self::Sec1Compressed,
            WirePublicKeyEncoding::Secp256k1XOnly => Self::Secp256k1XOnly,
            WirePublicKeyEncoding::JubjubCompressed => Self::JubjubCompressed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use oxid_platform_ports::PlatformError;

    use super::*;

    struct IncrementingRandom(Mutex<u8>);

    impl IncrementingRandom {
        fn new() -> Self {
            Self(Mutex::new(1))
        }
    }

    impl RandomPort for IncrementingRandom {
        fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
            let mut byte = self
                .0
                .lock()
                .map_err(|_| PlatformError::RandomnessUnavailable)?;
            destination.fill(*byte);
            *byte = byte.wrapping_add(1);
            Ok(())
        }
    }

    fn profile(value: &str) -> WalletProfileId {
        WalletProfileId::parse(value).expect("profile should be valid")
    }

    fn descriptor(reference: &str, label: &str) -> WalletKeyDescriptor {
        WalletKeyDescriptor::new(
            WalletKeyReference::parse(reference).expect("reference should be valid"),
            WalletKeyLabel::parse(label).expect("label should be valid"),
            WalletKeyAlgorithm::Ed25519,
            WalletKeyPurpose::Authentication,
            WalletPublicKey::new(PublicKeyEncoding::Ed25519Compressed, vec![9; 32]),
            UnixTimestampMillis::new(1_700_000_000_000),
        )
    }

    fn vault() -> PortableCustodyVault {
        PortableCustodyVault::new(
            profile("profile_one"),
            1_700_000_000_001,
            [7; 32],
            vec![PortableCustodyKey::generated(
                descriptor("key_one", "Signing key"),
                [8; 32],
            )],
        )
        .expect("vault should be valid")
    }

    fn secret() -> WalletRecoverySecret {
        WalletRecoverySecret::parse("correct horse battery staple")
            .expect("recovery secret should be valid")
    }

    fn archive() -> CompleteWalletArchive {
        CompleteWalletArchive::new(
            profile("profile_one"),
            br#"{"profile":"profile_one"}"#.to_vec(),
            br#"{"dids":[]}"#.to_vec(),
            Zeroizing::new(br#"{"credentials":[]}"#.to_vec()),
            vault(),
        )
        .expect("complete archive should be valid")
    }

    #[test]
    fn round_trip_is_profile_bound_and_redacted() {
        let backup = seal_portable_custody(&vault(), &secret(), &IncrementingRandom::new())
            .expect("vault should encrypt");
        assert!(
            !backup
                .as_bytes()
                .windows(32)
                .any(|window| window == [7; 32])
        );
        let opened = open_portable_custody(&backup, &secret(), &profile("profile_one"))
            .expect("vault should decrypt");
        assert_eq!(opened.root_seed(), &[7; 32]);
        assert_eq!(opened.keys().len(), 1);
        assert!(format!("{opened:?}").contains("[REDACTED]"));
        assert!(!format!("{:?}", opened.keys()[0]).contains("080808"));
        assert_eq!(
            open_portable_custody(&backup, &secret(), &profile("profile_two"))
                .expect_err("profile mismatch must fail"),
            WalletPortableBackupPortError::WrongProfile
        );
    }

    #[test]
    fn wrong_secret_and_ciphertext_tamper_are_indistinguishable() {
        let backup = seal_portable_custody(&vault(), &secret(), &IncrementingRandom::new())
            .expect("vault should encrypt");
        let wrong = WalletRecoverySecret::parse("this is definitely the wrong password")
            .expect("wrong secret should still be valid input");
        assert_eq!(
            open_portable_custody(&backup, &wrong, &profile("profile_one"))
                .expect_err("wrong secret must fail"),
            WalletPortableBackupPortError::AuthenticationFailed
        );
        let mut bytes = backup.into_bytes();
        *bytes.last_mut().expect("package should have ciphertext") ^= 1;
        let tampered = PortableWalletBackup::parse(bytes).expect("tampered bytes stay bounded");
        assert_eq!(
            open_portable_custody(&tampered, &secret(), &profile("profile_one"))
                .expect_err("tamper must fail"),
            WalletPortableBackupPortError::AuthenticationFailed
        );
    }

    #[test]
    fn complete_wallet_round_trip_is_single_envelope_and_fresh_install_safe() {
        let backup =
            seal_complete_wallet_archive(&archive(), &secret(), &IncrementingRandom::new())
                .expect("complete archive should encrypt");
        assert_eq!(
            u16::from_be_bytes([backup.as_bytes()[8], backup.as_bytes()[9]]),
            COMPLETE_WALLET_FORMAT_VERSION
        );
        for plaintext in [
            b"profile_one".as_slice(),
            b"credentials".as_slice(),
            &[7; 32],
        ] {
            assert!(
                !backup
                    .as_bytes()
                    .windows(plaintext.len())
                    .any(|window| window == plaintext)
            );
        }

        let opened = open_complete_wallet_archive(&backup, &secret(), None)
            .expect("fresh install should authenticate before learning the profile");
        assert_eq!(opened.profile_id(), &profile("profile_one"));
        assert_eq!(opened.profile_snapshot(), br#"{"profile":"profile_one"}"#);
        assert_eq!(opened.did_snapshot(), br#"{"dids":[]}"#);
        assert_eq!(opened.credential_snapshot(), br#"{"credentials":[]}"#);
        assert_eq!(opened.custody().root_seed(), &[7; 32]);
        assert!(format!("{opened:?}").contains("[REDACTED]"));

        assert_eq!(
            open_complete_wallet_archive(&backup, &secret(), Some(&profile("profile_two")))
                .expect_err("an existing-profile flow must remain exact"),
            WalletPortableBackupPortError::WrongProfile
        );
    }

    #[test]
    fn custody_and_complete_wallet_versions_cannot_be_confused() {
        let custody = seal_portable_custody(&vault(), &secret(), &IncrementingRandom::new())
            .expect("custody should encrypt");
        assert_eq!(
            open_complete_wallet_archive(&custody, &secret(), None)
                .expect_err("custody-only is not a complete wallet"),
            WalletPortableBackupPortError::InvalidPackage
        );
        let complete =
            seal_complete_wallet_archive(&archive(), &secret(), &IncrementingRandom::new())
                .expect("complete archive should encrypt");
        assert_eq!(
            open_portable_custody(&complete, &secret(), &profile("profile_one"))
                .expect_err("complete wallet is not custody-only"),
            WalletPortableBackupPortError::InvalidPackage
        );
    }

    #[test]
    fn complete_wallet_wrong_secret_and_tamper_are_indistinguishable() {
        let backup =
            seal_complete_wallet_archive(&archive(), &secret(), &IncrementingRandom::new())
                .expect("complete archive should encrypt");
        let wrong = WalletRecoverySecret::parse("this is definitely the wrong password")
            .expect("wrong secret should still be valid input");
        assert_eq!(
            open_complete_wallet_archive(&backup, &wrong, None)
                .expect_err("wrong secret must fail"),
            WalletPortableBackupPortError::AuthenticationFailed
        );
        let mut bytes = backup.into_bytes();
        *bytes.last_mut().expect("package should have ciphertext") ^= 1;
        let tampered = PortableWalletBackup::parse(bytes).expect("tampered bytes stay bounded");
        assert_eq!(
            open_complete_wallet_archive(&tampered, &secret(), None).expect_err("tamper must fail"),
            WalletPortableBackupPortError::AuthenticationFailed
        );
    }

    #[test]
    fn complete_wallet_sections_are_independently_bounded() {
        let error = CompleteWalletArchive::new(
            profile("profile_one"),
            Vec::new(),
            br#"{"dids":[]}"#.to_vec(),
            Zeroizing::new(br#"{"credentials":[]}"#.to_vec()),
            vault(),
        )
        .expect_err("an empty canonical section must fail");
        assert_eq!(error, WalletPortableBackupPortError::InvalidPackage);
    }

    #[test]
    fn future_versions_and_parameter_downgrades_fail_before_authentication() {
        for offset in [8_usize, 12, 16, 20] {
            let backup = seal_portable_custody(&vault(), &secret(), &IncrementingRandom::new())
                .expect("vault should encrypt");
            let mut bytes = backup.into_bytes();
            bytes[offset] ^= 1;
            let changed = PortableWalletBackup::parse(bytes).expect("package stays bounded");
            assert_eq!(
                open_portable_custody(&changed, &secret(), &profile("profile_one"))
                    .expect_err("metadata change must fail"),
                WalletPortableBackupPortError::InvalidPackage
            );
        }
    }

    #[test]
    fn duplicates_are_rejected_before_encryption() {
        let error = PortableCustodyVault::new(
            profile("profile_one"),
            1,
            [1; 32],
            vec![
                PortableCustodyKey::generated(descriptor("key_one", "First"), [2; 32]),
                PortableCustodyKey::generated(descriptor("key_one", "Second"), [3; 32]),
            ],
        )
        .expect_err("duplicate key references must fail");
        assert_eq!(error, WalletPortableBackupPortError::Conflict);
    }
}
