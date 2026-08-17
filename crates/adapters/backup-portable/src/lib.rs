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
use zeroize::{Zeroize as _, Zeroizing};

const MAGIC: &[u8; 8] = b"OXIDBAK1";
const FORMAT_VERSION: u16 = 1;
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

/// Encrypt one validated custody vault with fixed Argon2id and XChaCha20-Poly1305.
pub fn seal_portable_custody(
    vault: &PortableCustodyVault,
    recovery_secret: &WalletRecoverySecret,
    random: &dyn RandomPort,
) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
    let wire = WireVault::from_vault(vault);
    let plaintext = Zeroizing::new(
        serde_json::to_vec(&wire).map_err(|_| WalletPortableBackupPortError::InvalidOperation)?,
    );

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
    let header = encode_header(&salt, &nonce, ciphertext_len);
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
                msg: &plaintext,
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

/// Authenticate, decrypt, strictly decode, and profile-bind a custody package.
pub fn open_portable_custody(
    backup: &PortableWalletBackup,
    recovery_secret: &WalletRecoverySecret,
    expected_profile_id: &WalletProfileId,
) -> Result<PortableCustodyVault, WalletPortableBackupPortError> {
    let bytes = backup.as_bytes();
    let header = decode_header(bytes)?;
    let key = derive_key(recovery_secret, &header.salt)?;
    let key = Key::<XChaCha20Poly1305>::try_from(key.as_slice())
        .map_err(|_| WalletPortableBackupPortError::InvalidOperation)?;
    let cipher = XChaCha20Poly1305::new(&key);
    let nonce = XNonce::try_from(header.nonce.as_slice())
        .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &bytes[HEADER_BYTES..],
                    aad: &bytes[..HEADER_BYTES],
                },
            )
            .map_err(|_| WalletPortableBackupPortError::AuthenticationFailed)?,
    );
    let wire: WireVault = serde_json::from_slice(&plaintext)
        .map_err(|_| WalletPortableBackupPortError::InvalidPackage)?;
    let vault = wire.to_vault()?;
    if vault.profile_id() != expected_profile_id {
        return Err(WalletPortableBackupPortError::WrongProfile);
    }
    Ok(vault)
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
    salt: &[u8; SALT_BYTES],
    nonce: &[u8; NONCE_BYTES],
    ciphertext_len: u32,
) -> Vec<u8> {
    let mut header = Vec::with_capacity(HEADER_BYTES);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
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
    salt: [u8; SALT_BYTES],
    nonce: [u8; NONCE_BYTES],
}

fn decode_header(bytes: &[u8]) -> Result<DecodedHeader, WalletPortableBackupPortError> {
    if bytes.len() < HEADER_BYTES || &bytes[..MAGIC.len()] != MAGIC {
        return Err(WalletPortableBackupPortError::InvalidPackage);
    }
    let version = u16::from_be_bytes([bytes[8], bytes[9]]);
    let memory = u32::from_be_bytes(bytes[12..16].try_into().expect("fixed header range"));
    let iterations = u32::from_be_bytes(bytes[16..20].try_into().expect("fixed header range"));
    let lanes = u32::from_be_bytes(bytes[20..24].try_into().expect("fixed header range"));
    if version != FORMAT_VERSION
        || bytes[10] != KDF_ARGON2ID
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
    Ok(DecodedHeader { salt, nonce })
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
