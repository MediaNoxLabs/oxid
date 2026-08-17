// SPDX-License-Identifier: Apache-2.0

//! Production-facing mobile custody behind Oxid wallet ports.
//!
//! The native half stores one authenticated, device-bound sealed vault per
//! profile. Rust opens that vault only during an already-authorized operation,
//! validates every protected record, and zeroizes the plaintext before return.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    sync::{Arc, Mutex, MutexGuard},
};

#[cfg(any(target_os = "ios", target_os = "android"))]
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use oxid_adapter_backup_portable::{
    PortableCustodyKey, PortableCustodyVault, PortableKeyMaterialRef, open_portable_custody,
    seal_portable_custody,
};
use oxid_adapter_custody_software::{
    derive_bip32_secret, public_key_from_secret, sign_jubjub_challenge_with_secret,
    sign_with_secret,
};
use oxid_foundation::UnixTimestampMillis;
use oxid_platform_ports::{ClockPort, RandomPort};
use oxid_wallet_application::{
    DeriveProtectedKeyRequest, GenerateProtectedKeyRequest, JUBJUB_COMPACT_BYTES,
    PortableWalletBackup, WalletDerivedSecretUsePort, WalletHdPath, WalletHdPathComponent,
    WalletJubjubChallengeDeriver, WalletJubjubChallengeSignature, WalletJubjubChallengeSigningPort,
    WalletKeyDerivationPort, WalletKeyOperationPort, WalletPortableBackupPort,
    WalletPortableBackupPortError, WalletPortableRecoverySummary, WalletProtectionPort,
    WalletRecoverySecret, WalletSecurityPortError,
};
use oxid_wallet_domain::{
    WalletKeyAlgorithm, WalletKeyDescriptor, WalletKeyLabel, WalletKeyPurpose, WalletKeyReference,
    WalletProfileId, WalletProtectionClass, WalletProtectionState, WalletSecurityStatus,
    WalletSignature,
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize as _, Zeroizing};

const VAULT_VERSION: u32 = 1;
const MAX_VAULT_BYTES: usize = 512 * 1024;
const MAX_KEYS: usize = 256;
const KEY_REFERENCE_ATTEMPTS: usize = 8;
const SECRET_ATTEMPTS: usize = 128;
const AUTHORIZATION_REASON: &str = "Unlock Oxid wallet protection";
const PROTECTED_OPERATION_REASON: &str = "Authorize this protected Oxid operation";
const PORTABLE_BACKUP_EXPORT_REASON: &str = "Authorize portable Oxid wallet backup export";

/// Effective native wrapping class. It describes the wrapping key, never the
/// software algorithm inside the sealed vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealedVaultProtection {
    OperatingSystem,
    HardwareBacked,
}

/// Safe state projection returned without releasing protected bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealedVaultState {
    Uninitialized,
    Locked(SealedVaultProtection),
    Unlocked(SealedVaultProtection),
    Unavailable,
}

/// Failures from a platform sealed-vault implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealedVaultError {
    Unavailable,
    NotInitialized,
    AlreadyInitialized,
    Locked,
    AuthorizationDenied,
    Invalid,
}

/// Adapter-internal native boundary. Plaintext exists only for the duration of
/// one call and must be returned in zeroizing storage.
pub trait SealedVaultPort: Send + Sync {
    fn inspect(&self, profile_id: &WalletProfileId) -> Result<SealedVaultState, SealedVaultError>;

    fn initialize(
        &self,
        profile_id: &WalletProfileId,
        plaintext: &[u8],
    ) -> Result<SealedVaultProtection, SealedVaultError>;

    fn unlock(
        &self,
        profile_id: &WalletProfileId,
        reason: &str,
    ) -> Result<Zeroizing<Vec<u8>>, SealedVaultError>;

    fn load(&self, profile_id: &WalletProfileId) -> Result<Zeroizing<Vec<u8>>, SealedVaultError>;

    fn save(&self, profile_id: &WalletProfileId, plaintext: &[u8]) -> Result<(), SealedVaultError>;

    fn lock(&self, profile_id: &WalletProfileId) -> Result<(), SealedVaultError>;
}

/// Bridge-backed sealed vault for iOS and Android.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeMobileSealedVault;

impl SealedVaultPort for NativeMobileSealedVault {
    fn inspect(&self, profile_id: &WalletProfileId) -> Result<SealedVaultState, SealedVaultError> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            let response = oxid_adapter_mobile_native::inspect_custody_json(profile_id.as_str())
                .map_err(map_bridge_error)?;
            return parse_state_response(response);
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = profile_id;
            Err(SealedVaultError::Unavailable)
        }
    }

    fn initialize(
        &self,
        profile_id: &WalletProfileId,
        plaintext: &[u8],
    ) -> Result<SealedVaultProtection, SealedVaultError> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            validate_plaintext_size(plaintext)?;
            let payload = Zeroizing::new(BASE64_STANDARD.encode(plaintext));
            let response =
                oxid_adapter_mobile_native::initialize_custody_json(profile_id.as_str(), &payload)
                    .map_err(map_bridge_error)?;
            return parse_success_response(response).map(|response| response.protection);
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = (profile_id, plaintext);
            Err(SealedVaultError::Unavailable)
        }
    }

    fn unlock(
        &self,
        profile_id: &WalletProfileId,
        reason: &str,
    ) -> Result<Zeroizing<Vec<u8>>, SealedVaultError> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            let response =
                oxid_adapter_mobile_native::unlock_custody_json(profile_id.as_str(), reason)
                    .map_err(map_bridge_error)?;
            return decode_success_payload(response);
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = (profile_id, reason);
            Err(SealedVaultError::Unavailable)
        }
    }

    fn load(&self, profile_id: &WalletProfileId) -> Result<Zeroizing<Vec<u8>>, SealedVaultError> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            let response = oxid_adapter_mobile_native::load_custody_json(profile_id.as_str())
                .map_err(map_bridge_error)?;
            return decode_success_payload(response);
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = profile_id;
            Err(SealedVaultError::Unavailable)
        }
    }

    fn save(&self, profile_id: &WalletProfileId, plaintext: &[u8]) -> Result<(), SealedVaultError> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            validate_plaintext_size(plaintext)?;
            let payload = Zeroizing::new(BASE64_STANDARD.encode(plaintext));
            let response =
                oxid_adapter_mobile_native::save_custody_json(profile_id.as_str(), &payload)
                    .map_err(map_bridge_error)?;
            parse_success_response(response).map(|_| ())
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = (profile_id, plaintext);
            Err(SealedVaultError::Unavailable)
        }
    }

    fn lock(&self, profile_id: &WalletProfileId) -> Result<(), SealedVaultError> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            let response = oxid_adapter_mobile_native::lock_custody_json(profile_id.as_str())
                .map_err(map_bridge_error)?;
            parse_locked_response(response)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = profile_id;
            Err(SealedVaultError::Unavailable)
        }
    }
}

/// Production mobile implementation of every wallet custody capability used by
/// Midnight, DID, credential, and presentation adapters.
pub struct MobileWalletSecurity<C, N, B = NativeMobileSealedVault> {
    clock: Arc<C>,
    random: Arc<N>,
    backend: Arc<B>,
    operation_gate: Mutex<()>,
}

impl<C, N> MobileWalletSecurity<C, N, NativeMobileSealedVault> {
    #[must_use]
    pub fn native(clock: Arc<C>, random: Arc<N>) -> Self {
        Self::new(clock, random, Arc::new(NativeMobileSealedVault))
    }
}

impl<C, N, B> MobileWalletSecurity<C, N, B> {
    #[must_use]
    pub fn new(clock: Arc<C>, random: Arc<N>, backend: Arc<B>) -> Self {
        Self {
            clock,
            random,
            backend,
            operation_gate: Mutex::new(()),
        }
    }

    fn gate(&self) -> Result<MutexGuard<'_, ()>, WalletSecurityPortError> {
        self.operation_gate
            .lock()
            .map_err(|_| WalletSecurityPortError::Unavailable)
    }

    fn load_vault(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<MobileVault, WalletSecurityPortError>
    where
        B: SealedVaultPort,
    {
        let plaintext = match self.backend.load(profile_id) {
            Ok(plaintext) => plaintext,
            Err(SealedVaultError::Locked) => self
                .backend
                .unlock(profile_id, PROTECTED_OPERATION_REASON)
                .map_err(map_vault_error)?,
            Err(error) => return Err(map_vault_error(error)),
        };
        decode_vault(profile_id, &plaintext)
    }

    fn save_vault(
        &self,
        profile_id: &WalletProfileId,
        vault: &MobileVault,
    ) -> Result<(), WalletSecurityPortError>
    where
        B: SealedVaultPort,
    {
        let plaintext = encode_vault(vault)?;
        self.backend
            .save(profile_id, &plaintext)
            .map_err(map_vault_error)
    }

    fn new_reference(
        &self,
        vault: &MobileVault,
    ) -> Result<WalletKeyReference, WalletSecurityPortError>
    where
        N: RandomPort,
    {
        for _ in 0..KEY_REFERENCE_ATTEMPTS {
            let mut bytes = [0_u8; 16];
            self.random
                .fill_bytes(&mut bytes)
                .map_err(|_| WalletSecurityPortError::Unavailable)?;
            bytes[6] = (bytes[6] & 0x0f) | 0x40;
            bytes[8] = (bytes[8] & 0x3f) | 0x80;
            let mut value = String::with_capacity(36);
            value.push_str("key_");
            for byte in bytes {
                write!(&mut value, "{byte:02x}")
                    .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
            }
            if !vault.keys.iter().any(|key| key.reference == value) {
                return WalletKeyReference::parse(value)
                    .map_err(|_| WalletSecurityPortError::InvalidOperation);
            }
        }
        Err(WalletSecurityPortError::Conflict)
    }

    fn generate_secret(
        &self,
        algorithm: WalletKeyAlgorithm,
    ) -> Result<Zeroizing<[u8; 32]>, WalletSecurityPortError>
    where
        N: RandomPort,
    {
        for _ in 0..SECRET_ATTEMPTS {
            let mut secret = Zeroizing::new([0_u8; 32]);
            self.random
                .fill_bytes(secret.as_mut())
                .map_err(|_| WalletSecurityPortError::Unavailable)?;
            if public_key_from_secret(algorithm, &secret).is_ok() {
                return Ok(secret);
            }
        }
        Err(WalletSecurityPortError::InvalidOperation)
    }
}

impl<C, N, B> WalletProtectionPort for MobileWalletSecurity<C, N, B>
where
    C: ClockPort,
    N: RandomPort,
    B: SealedVaultPort,
{
    fn status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        let _gate = self.gate()?;
        match self.backend.inspect(profile_id).map_err(map_vault_error)? {
            SealedVaultState::Uninitialized => Ok(mobile_status(
                WalletProtectionState::Uninitialized,
                SealedVaultProtection::OperatingSystem,
            )),
            SealedVaultState::Locked(protection) => {
                Ok(mobile_status(WalletProtectionState::Locked, protection))
            }
            SealedVaultState::Unlocked(protection) => {
                Ok(mobile_status(WalletProtectionState::Unlocked, protection))
            }
            SealedVaultState::Unavailable => Ok(WalletSecurityStatus::unavailable()),
        }
    }

    fn initialize(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        let _gate = self.gate()?;
        let mut root_seed = Zeroizing::new([0_u8; 32]);
        self.random
            .fill_bytes(root_seed.as_mut())
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        let vault = MobileVault {
            version: VAULT_VERSION,
            profile_id: profile_id.as_str().to_owned(),
            root_seed: *root_seed,
            keys: Vec::new(),
        };
        let plaintext = encode_vault(&vault)?;
        let protection = self
            .backend
            .initialize(profile_id, &plaintext)
            .map_err(map_vault_error)?;
        Ok(mobile_status(WalletProtectionState::Unlocked, protection))
    }

    fn unlock(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        let _gate = self.gate()?;
        let plaintext = self
            .backend
            .unlock(profile_id, AUTHORIZATION_REASON)
            .map_err(map_vault_error)?;
        let _vault = decode_vault(profile_id, &plaintext)?;
        let state = self.backend.inspect(profile_id).map_err(map_vault_error)?;
        match state {
            SealedVaultState::Unlocked(protection) => {
                Ok(mobile_status(WalletProtectionState::Unlocked, protection))
            }
            _ => Err(WalletSecurityPortError::Locked),
        }
    }

    fn lock(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        let _gate = self.gate()?;
        self.backend.lock(profile_id).map_err(map_vault_error)?;
        let state = self.backend.inspect(profile_id).map_err(map_vault_error)?;
        match state {
            SealedVaultState::Locked(protection) => {
                Ok(mobile_status(WalletProtectionState::Locked, protection))
            }
            _ => Err(WalletSecurityPortError::InvalidOperation),
        }
    }
}

impl<C, N, B> WalletKeyOperationPort for MobileWalletSecurity<C, N, B>
where
    C: ClockPort,
    N: RandomPort,
    B: SealedVaultPort,
{
    fn generate(
        &self,
        profile_id: &WalletProfileId,
        request: GenerateProtectedKeyRequest,
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        let _gate = self.gate()?;
        let mut vault = self.load_vault(profile_id)?;
        if vault.keys.len() >= MAX_KEYS {
            return Err(WalletSecurityPortError::Conflict);
        }
        if vault
            .keys
            .iter()
            .any(|key| key.label == request.label.as_str())
        {
            return Err(WalletSecurityPortError::Conflict);
        }
        let reference = self.new_reference(&vault)?;
        let secret = self.generate_secret(request.algorithm)?;
        let created_at = self
            .clock
            .now()
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        let record = StoredKey {
            reference: reference.as_str().to_owned(),
            label: request.label.as_str().to_owned(),
            algorithm: algorithm_name(request.algorithm).to_owned(),
            purpose: purpose_name(request.purpose).to_owned(),
            created_at_millis: created_at.value(),
            material: StoredKeyMaterial::Generated { secret: *secret },
        };
        let descriptor = record.descriptor()?;
        vault.keys.push(record);
        self.save_vault(profile_id, &vault)?;
        Ok(descriptor)
    }

    fn list(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Vec<WalletKeyDescriptor>, WalletSecurityPortError> {
        let _gate = self.gate()?;
        let vault = self.load_vault(profile_id)?;
        vault
            .keys
            .iter()
            .map(|key| key.descriptor_with_root(&vault.root_seed))
            .collect()
    }

    fn sign(
        &self,
        profile_id: &WalletProfileId,
        key_reference: &WalletKeyReference,
        payload: &[u8],
    ) -> Result<WalletSignature, WalletSecurityPortError> {
        let _gate = self.gate()?;
        let vault = self.load_vault(profile_id)?;
        let record = vault
            .keys
            .iter()
            .find(|key| key.reference == key_reference.as_str())
            .ok_or(WalletSecurityPortError::NotFound)?;
        let algorithm = parse_algorithm(&record.algorithm)?;
        let secret = record.secret(&vault.root_seed)?;
        sign_with_secret(algorithm, &secret, payload)
    }

    fn delete(
        &self,
        profile_id: &WalletProfileId,
        key_reference: &WalletKeyReference,
    ) -> Result<(), WalletSecurityPortError> {
        let _gate = self.gate()?;
        let mut vault = self.load_vault(profile_id)?;
        let index = vault
            .keys
            .iter()
            .position(|key| key.reference == key_reference.as_str())
            .ok_or(WalletSecurityPortError::NotFound)?;
        vault.keys.remove(index);
        self.save_vault(profile_id, &vault)
    }
}

impl<C, N, B> WalletKeyDerivationPort for MobileWalletSecurity<C, N, B>
where
    C: ClockPort,
    N: RandomPort,
    B: SealedVaultPort,
{
    fn derive(
        &self,
        profile_id: &WalletProfileId,
        request: DeriveProtectedKeyRequest,
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        if request.algorithm != WalletKeyAlgorithm::Secp256k1Schnorr {
            return Err(WalletSecurityPortError::UnsupportedAlgorithm);
        }
        let _gate = self.gate()?;
        let mut vault = self.load_vault(profile_id)?;
        let stored_path = stored_path(&request.path);
        if let Some(existing) = vault.keys.iter().find(|key| {
            matches!(&key.material, StoredKeyMaterial::Derived { path } if path == &stored_path)
        }) {
            if existing.label == request.label.as_str()
                && parse_algorithm(&existing.algorithm)? == request.algorithm
                && parse_purpose(&existing.purpose)? == request.purpose
            {
                return existing.descriptor_with_root(&vault.root_seed);
            }
            return Err(WalletSecurityPortError::Conflict);
        }
        if vault.keys.len() >= MAX_KEYS
            || vault
                .keys
                .iter()
                .any(|key| key.label == request.label.as_str())
        {
            return Err(WalletSecurityPortError::Conflict);
        }
        let reference = self.new_reference(&vault)?;
        let created_at = self
            .clock
            .now()
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        let record = StoredKey {
            reference: reference.as_str().to_owned(),
            label: request.label.as_str().to_owned(),
            algorithm: algorithm_name(request.algorithm).to_owned(),
            purpose: purpose_name(request.purpose).to_owned(),
            created_at_millis: created_at.value(),
            material: StoredKeyMaterial::Derived { path: stored_path },
        };
        let descriptor = record.descriptor_with_root(&vault.root_seed)?;
        vault.keys.push(record);
        self.save_vault(profile_id, &vault)?;
        Ok(descriptor)
    }
}

impl<C, N, B> WalletDerivedSecretUsePort for MobileWalletSecurity<C, N, B>
where
    C: ClockPort,
    N: RandomPort,
    B: SealedVaultPort,
{
    fn use_derived_secret(
        &self,
        profile_id: &WalletProfileId,
        path: &WalletHdPath,
        operation: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
    ) -> Result<(), WalletSecurityPortError> {
        let _gate = self.gate()?;
        let vault = self.load_vault(profile_id)?;
        let secret = derive_bip32_secret(&vault.root_seed, path)?;
        operation(&secret)
    }
}

impl<C, N, B> WalletJubjubChallengeSigningPort for MobileWalletSecurity<C, N, B>
where
    C: ClockPort,
    N: RandomPort,
    B: SealedVaultPort,
{
    fn sign_jubjub_challenge(
        &self,
        profile_id: &WalletProfileId,
        key_reference: &WalletKeyReference,
        derive_challenge: &mut WalletJubjubChallengeDeriver<'_>,
    ) -> Result<WalletJubjubChallengeSignature, WalletSecurityPortError> {
        let _gate = self.gate()?;
        let vault = self.load_vault(profile_id)?;
        let record = vault
            .keys
            .iter()
            .find(|key| key.reference == key_reference.as_str())
            .ok_or(WalletSecurityPortError::NotFound)?;
        if parse_algorithm(&record.algorithm)? != WalletKeyAlgorithm::Jubjub {
            return Err(WalletSecurityPortError::UnsupportedAlgorithm);
        }
        let secret = record.secret(&vault.root_seed)?;
        let mut nonce_seed = Zeroizing::new([0_u8; JUBJUB_COMPACT_BYTES]);
        self.random
            .fill_bytes(nonce_seed.as_mut())
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        sign_jubjub_challenge_with_secret(&secret, &nonce_seed, derive_challenge)
    }
}

impl<C, N, B> WalletPortableBackupPort for MobileWalletSecurity<C, N, B>
where
    C: ClockPort,
    N: RandomPort,
    B: SealedVaultPort,
{
    fn export_portable_backup(
        &self,
        profile_id: &WalletProfileId,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
        let _gate = self.gate().map_err(map_backup_security_error)?;
        // Export always asks the native backend for a new authorization. An
        // existing unlocked session is intentionally not sufficient.
        let plaintext = self
            .backend
            .unlock(profile_id, PORTABLE_BACKUP_EXPORT_REASON)
            .map_err(map_backup_vault_error)?;
        let vault = decode_vault(profile_id, &plaintext).map_err(map_backup_security_error)?;
        let keys = vault
            .keys
            .iter()
            .map(|stored| {
                let descriptor = stored
                    .descriptor_with_root(&vault.root_seed)
                    .map_err(map_backup_security_error)?;
                match &stored.material {
                    StoredKeyMaterial::Generated { secret } => {
                        Ok(PortableCustodyKey::generated(descriptor, *secret))
                    }
                    StoredKeyMaterial::Derived { .. } => Ok(PortableCustodyKey::derived(
                        descriptor,
                        stored
                            .path()
                            .map_err(map_backup_security_error)?
                            .ok_or(WalletPortableBackupPortError::InvalidPackage)?,
                    )),
                }
            })
            .collect::<Result<Vec<_>, WalletPortableBackupPortError>>()?;
        let exported_at_millis = self
            .clock
            .now()
            .map_err(|_| WalletPortableBackupPortError::Unavailable)?
            .value();
        let portable = PortableCustodyVault::new(
            profile_id.clone(),
            exported_at_millis,
            vault.root_seed,
            keys,
        )?;
        seal_portable_custody(&portable, recovery_secret, self.random.as_ref())
    }

    fn recover_portable_backup(
        &self,
        profile_id: &WalletProfileId,
        backup: &PortableWalletBackup,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
        let _gate = self.gate().map_err(map_backup_security_error)?;
        match self
            .backend
            .inspect(profile_id)
            .map_err(map_backup_vault_error)?
        {
            SealedVaultState::Uninitialized => {}
            SealedVaultState::Locked(_) | SealedVaultState::Unlocked(_) => {
                return Err(WalletPortableBackupPortError::AlreadyInitialized);
            }
            SealedVaultState::Unavailable => {
                return Err(WalletPortableBackupPortError::Unavailable);
            }
        }

        let portable = open_portable_custody(backup, recovery_secret, profile_id)?;
        let mut keys = Vec::with_capacity(portable.keys().len());
        for key in portable.keys() {
            let descriptor = key.descriptor();
            let (public_key, material) = match key.material() {
                PortableKeyMaterialRef::Generated(secret) => (
                    public_key_from_secret(descriptor.algorithm(), secret)
                        .map_err(map_backup_security_error)?,
                    StoredKeyMaterial::Generated { secret: *secret },
                ),
                PortableKeyMaterialRef::Derived(path) => {
                    if descriptor.algorithm() != WalletKeyAlgorithm::Secp256k1Schnorr {
                        return Err(WalletPortableBackupPortError::InvalidPackage);
                    }
                    let secret = derive_bip32_secret(portable.root_seed(), path)
                        .map_err(map_backup_security_error)?;
                    (
                        public_key_from_secret(descriptor.algorithm(), &secret)
                            .map_err(map_backup_security_error)?,
                        StoredKeyMaterial::Derived {
                            path: stored_path(path),
                        },
                    )
                }
            };
            if descriptor.public_key() != &public_key {
                return Err(WalletPortableBackupPortError::InvalidPackage);
            }
            keys.push(StoredKey {
                reference: descriptor.reference().as_str().to_owned(),
                label: descriptor.label().as_str().to_owned(),
                algorithm: algorithm_name(descriptor.algorithm()).to_owned(),
                purpose: purpose_name(descriptor.purpose()).to_owned(),
                created_at_millis: descriptor.created_at().value(),
                material,
            });
        }
        let restored_key_count = keys.len();
        let vault = MobileVault {
            version: VAULT_VERSION,
            profile_id: profile_id.as_str().to_owned(),
            root_seed: *portable.root_seed(),
            keys,
        };
        validate_vault(profile_id, &vault).map_err(map_backup_security_error)?;
        let plaintext = encode_vault(&vault).map_err(map_backup_security_error)?;
        // Native initialization is the one-shot, fresh platform authorization
        // boundary and refuses any existing destination state.
        self.backend
            .initialize(profile_id, &plaintext)
            .map_err(map_backup_vault_error)?;
        Ok(WalletPortableRecoverySummary { restored_key_count })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MobileVault {
    version: u32,
    profile_id: String,
    root_seed: [u8; 32],
    keys: Vec<StoredKey>,
}

impl Drop for MobileVault {
    fn drop(&mut self) {
        self.root_seed.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredKey {
    reference: String,
    label: String,
    algorithm: String,
    purpose: String,
    created_at_millis: u64,
    material: StoredKeyMaterial,
}

impl Drop for StoredKey {
    fn drop(&mut self) {
        if let StoredKeyMaterial::Generated { secret } = &mut self.material {
            secret.zeroize();
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StoredKeyMaterial {
    Generated { secret: [u8; 32] },
    Derived { path: Vec<StoredPathComponent> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPathComponent {
    index: u32,
    hardened: bool,
}

impl StoredKey {
    fn path(&self) -> Result<Option<WalletHdPath>, WalletSecurityPortError> {
        match &self.material {
            StoredKeyMaterial::Generated { .. } => Ok(None),
            StoredKeyMaterial::Derived { path } => path
                .iter()
                .map(|component| WalletHdPathComponent::new(component.index, component.hardened))
                .collect::<Result<Vec<_>, _>>()
                .and_then(WalletHdPath::new)
                .map(Some)
                .map_err(|_| WalletSecurityPortError::InvalidOperation),
        }
    }

    fn secret(&self, root_seed: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>, WalletSecurityPortError> {
        match &self.material {
            StoredKeyMaterial::Generated { secret } => Ok(Zeroizing::new(*secret)),
            StoredKeyMaterial::Derived { .. } => derive_bip32_secret(
                root_seed,
                &self
                    .path()?
                    .ok_or(WalletSecurityPortError::InvalidOperation)?,
            ),
        }
    }

    fn descriptor(&self) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        match self.material {
            StoredKeyMaterial::Generated { .. } => self.descriptor_with_root(&[0; 32]),
            StoredKeyMaterial::Derived { .. } => Err(WalletSecurityPortError::InvalidOperation),
        }
    }

    fn descriptor_with_root(
        &self,
        root_seed: &[u8; 32],
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        let algorithm = parse_algorithm(&self.algorithm)?;
        let secret = self.secret(root_seed)?;
        let public_key = public_key_from_secret(algorithm, &secret)?;
        Ok(WalletKeyDescriptor::new(
            WalletKeyReference::parse(self.reference.clone())
                .map_err(|_| WalletSecurityPortError::InvalidOperation)?,
            WalletKeyLabel::parse(&self.label)
                .map_err(|_| WalletSecurityPortError::InvalidOperation)?,
            algorithm,
            parse_purpose(&self.purpose)?,
            public_key,
            UnixTimestampMillis::new(self.created_at_millis),
        ))
    }
}

fn encode_vault(vault: &MobileVault) -> Result<Zeroizing<Vec<u8>>, WalletSecurityPortError> {
    let bytes = serde_json::to_vec(vault).map_err(|_| WalletSecurityPortError::InvalidOperation)?;
    if bytes.is_empty() || bytes.len() > MAX_VAULT_BYTES {
        return Err(WalletSecurityPortError::InvalidOperation);
    }
    Ok(Zeroizing::new(bytes))
}

fn decode_vault(
    profile_id: &WalletProfileId,
    plaintext: &[u8],
) -> Result<MobileVault, WalletSecurityPortError> {
    if plaintext.is_empty() || plaintext.len() > MAX_VAULT_BYTES {
        return Err(WalletSecurityPortError::InvalidOperation);
    }
    let vault: MobileVault =
        serde_json::from_slice(plaintext).map_err(|_| WalletSecurityPortError::InvalidOperation)?;
    validate_vault(profile_id, &vault)?;
    Ok(vault)
}

fn validate_vault(
    profile_id: &WalletProfileId,
    vault: &MobileVault,
) -> Result<(), WalletSecurityPortError> {
    if vault.version != VAULT_VERSION
        || vault.profile_id != profile_id.as_str()
        || vault.keys.len() > MAX_KEYS
    {
        return Err(WalletSecurityPortError::InvalidOperation);
    }
    let mut references = BTreeSet::new();
    let mut labels = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for key in &vault.keys {
        if !references.insert(key.reference.clone()) || !labels.insert(key.label.clone()) {
            return Err(WalletSecurityPortError::InvalidOperation);
        }
        let algorithm = parse_algorithm(&key.algorithm)?;
        let _purpose = parse_purpose(&key.purpose)?;
        let path = key.path()?;
        if let Some(path) = path {
            if algorithm != WalletKeyAlgorithm::Secp256k1Schnorr || !paths.insert(path) {
                return Err(WalletSecurityPortError::InvalidOperation);
            }
            key.descriptor_with_root(&vault.root_seed)?;
        } else {
            key.descriptor()?;
        }
    }
    Ok(())
}

fn stored_path(path: &WalletHdPath) -> Vec<StoredPathComponent> {
    path.components()
        .iter()
        .map(|component| StoredPathComponent {
            index: component.index(),
            hardened: component.hardened(),
        })
        .collect()
}

const fn algorithm_name(algorithm: WalletKeyAlgorithm) -> &'static str {
    match algorithm {
        WalletKeyAlgorithm::Ed25519 => "ed25519",
        WalletKeyAlgorithm::P256 => "p256",
        WalletKeyAlgorithm::Secp256k1Schnorr => "secp256k1_schnorr",
        WalletKeyAlgorithm::Jubjub => "jubjub",
    }
}

fn parse_algorithm(value: &str) -> Result<WalletKeyAlgorithm, WalletSecurityPortError> {
    match value {
        "ed25519" => Ok(WalletKeyAlgorithm::Ed25519),
        "p256" => Ok(WalletKeyAlgorithm::P256),
        "secp256k1_schnorr" => Ok(WalletKeyAlgorithm::Secp256k1Schnorr),
        "jubjub" => Ok(WalletKeyAlgorithm::Jubjub),
        _ => Err(WalletSecurityPortError::InvalidOperation),
    }
}

const fn purpose_name(purpose: WalletKeyPurpose) -> &'static str {
    match purpose {
        WalletKeyPurpose::Transaction => "transaction",
        WalletKeyPurpose::Authentication => "authentication",
        WalletKeyPurpose::Assertion => "assertion",
        WalletKeyPurpose::KeyAgreement => "key_agreement",
        WalletKeyPurpose::Recovery => "recovery",
    }
}

fn parse_purpose(value: &str) -> Result<WalletKeyPurpose, WalletSecurityPortError> {
    match value {
        "transaction" => Ok(WalletKeyPurpose::Transaction),
        "authentication" => Ok(WalletKeyPurpose::Authentication),
        "assertion" => Ok(WalletKeyPurpose::Assertion),
        "key_agreement" => Ok(WalletKeyPurpose::KeyAgreement),
        "recovery" => Ok(WalletKeyPurpose::Recovery),
        _ => Err(WalletSecurityPortError::InvalidOperation),
    }
}

const fn mobile_status(
    state: WalletProtectionState,
    protection: SealedVaultProtection,
) -> WalletSecurityStatus {
    WalletSecurityStatus::new(
        state,
        match protection {
            SealedVaultProtection::OperatingSystem => WalletProtectionClass::OperatingSystem,
            SealedVaultProtection::HardwareBacked => WalletProtectionClass::HardwareBacked,
        },
        true,
        true,
    )
}

const fn map_backup_vault_error(error: SealedVaultError) -> WalletPortableBackupPortError {
    match error {
        SealedVaultError::Unavailable => WalletPortableBackupPortError::Unavailable,
        SealedVaultError::NotInitialized => WalletPortableBackupPortError::NotInitialized,
        SealedVaultError::AlreadyInitialized => WalletPortableBackupPortError::AlreadyInitialized,
        SealedVaultError::Locked => WalletPortableBackupPortError::Locked,
        SealedVaultError::AuthorizationDenied => WalletPortableBackupPortError::AuthorizationDenied,
        SealedVaultError::Invalid => WalletPortableBackupPortError::InvalidPackage,
    }
}

const fn map_backup_security_error(
    error: WalletSecurityPortError,
) -> WalletPortableBackupPortError {
    match error {
        WalletSecurityPortError::Unavailable => WalletPortableBackupPortError::Unavailable,
        WalletSecurityPortError::NotInitialized => WalletPortableBackupPortError::NotInitialized,
        WalletSecurityPortError::AlreadyInitialized => {
            WalletPortableBackupPortError::AlreadyInitialized
        }
        WalletSecurityPortError::Locked => WalletPortableBackupPortError::Locked,
        WalletSecurityPortError::AuthorizationDenied => {
            WalletPortableBackupPortError::AuthorizationDenied
        }
        WalletSecurityPortError::Conflict => WalletPortableBackupPortError::Conflict,
        WalletSecurityPortError::NotFound
        | WalletSecurityPortError::UnsupportedAlgorithm
        | WalletSecurityPortError::InvalidOperation => {
            WalletPortableBackupPortError::InvalidPackage
        }
    }
}

const fn map_vault_error(error: SealedVaultError) -> WalletSecurityPortError {
    match error {
        SealedVaultError::Unavailable => WalletSecurityPortError::Unavailable,
        SealedVaultError::NotInitialized => WalletSecurityPortError::NotInitialized,
        SealedVaultError::AlreadyInitialized => WalletSecurityPortError::AlreadyInitialized,
        SealedVaultError::Locked => WalletSecurityPortError::Locked,
        SealedVaultError::AuthorizationDenied => WalletSecurityPortError::AuthorizationDenied,
        SealedVaultError::Invalid => WalletSecurityPortError::InvalidOperation,
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn map_bridge_error(error: oxid_adapter_mobile_native::NativeBridgeError) -> SealedVaultError {
    match error {
        oxid_adapter_mobile_native::NativeBridgeError::Unavailable => SealedVaultError::Unavailable,
        oxid_adapter_mobile_native::NativeBridgeError::Failed => SealedVaultError::Invalid,
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeCustodyResponse {
    status: String,
    #[serde(default)]
    protection: Option<String>,
    #[serde(default)]
    payload: Option<String>,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
struct NativeSuccessResponse {
    protection: SealedVaultProtection,
    payload: Option<String>,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn parse_native_response(response: String) -> Result<NativeCustodyResponse, SealedVaultError> {
    if response.is_empty() || response.len() > MAX_VAULT_BYTES * 2 {
        return Err(SealedVaultError::Invalid);
    }
    serde_json::from_str(&response).map_err(|_| SealedVaultError::Invalid)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn parse_protection(value: Option<&str>) -> Result<SealedVaultProtection, SealedVaultError> {
    match value {
        Some("operating_system") => Ok(SealedVaultProtection::OperatingSystem),
        Some("hardware_backed") => Ok(SealedVaultProtection::HardwareBacked),
        _ => Err(SealedVaultError::Invalid),
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn response_error(status: &str) -> SealedVaultError {
    match status {
        "unavailable" => SealedVaultError::Unavailable,
        "not_initialized" => SealedVaultError::NotInitialized,
        "already_initialized" => SealedVaultError::AlreadyInitialized,
        "locked" => SealedVaultError::Locked,
        "authorization_denied" => SealedVaultError::AuthorizationDenied,
        _ => SealedVaultError::Invalid,
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn parse_state_response(response: String) -> Result<SealedVaultState, SealedVaultError> {
    let response = parse_native_response(response)?;
    match response.status.as_str() {
        "uninitialized" => Ok(SealedVaultState::Uninitialized),
        "locked" => Ok(SealedVaultState::Locked(parse_protection(
            response.protection.as_deref(),
        )?)),
        "unlocked" => Ok(SealedVaultState::Unlocked(parse_protection(
            response.protection.as_deref(),
        )?)),
        "unavailable" => Ok(SealedVaultState::Unavailable),
        status => Err(response_error(status)),
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn parse_success_response(response: String) -> Result<NativeSuccessResponse, SealedVaultError> {
    let response = parse_native_response(response)?;
    if response.status != "succeeded" {
        return Err(response_error(&response.status));
    }
    Ok(NativeSuccessResponse {
        protection: parse_protection(response.protection.as_deref())?,
        payload: response.payload,
    })
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn decode_success_payload(response: String) -> Result<Zeroizing<Vec<u8>>, SealedVaultError> {
    let response = parse_success_response(response)?;
    let payload = Zeroizing::new(response.payload.ok_or(SealedVaultError::Invalid)?);
    let bytes = BASE64_STANDARD
        .decode(payload.as_bytes())
        .map_err(|_| SealedVaultError::Invalid)?;
    validate_plaintext_size(&bytes)?;
    Ok(Zeroizing::new(bytes))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn parse_locked_response(response: String) -> Result<(), SealedVaultError> {
    let response = parse_native_response(response)?;
    match response.status.as_str() {
        "locked" => Ok(()),
        status => Err(response_error(status)),
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn validate_plaintext_size(plaintext: &[u8]) -> Result<(), SealedVaultError> {
    if plaintext.is_empty() || plaintext.len() > MAX_VAULT_BYTES {
        return Err(SealedVaultError::Invalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};

    use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
    use oxid_platform_ports::PlatformError;

    use super::*;

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    struct IncrementingRandom(Mutex<u8>);

    impl IncrementingRandom {
        fn new() -> Self {
            Self(Mutex::new(17))
        }
    }

    impl RandomPort for IncrementingRandom {
        fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
            let mut next = self
                .0
                .lock()
                .map_err(|_| PlatformError::RandomnessUnavailable)?;
            destination.fill(*next);
            *next = next.wrapping_add(1).max(1);
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestSealedVault {
        records: Mutex<BTreeMap<String, Vec<u8>>>,
        unlocked: Mutex<BTreeSet<String>>,
        unlock_reasons: Mutex<Vec<String>>,
        deny_initialize: AtomicBool,
        deny_unlock: AtomicBool,
    }

    impl TestSealedVault {
        fn corrupt(&self, profile_id: &WalletProfileId) {
            self.records
                .lock()
                .expect("records")
                .insert(profile_id.as_str().to_owned(), b"not-json".to_vec());
        }
    }

    impl SealedVaultPort for TestSealedVault {
        fn inspect(
            &self,
            profile_id: &WalletProfileId,
        ) -> Result<SealedVaultState, SealedVaultError> {
            if !self
                .records
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .contains_key(profile_id.as_str())
            {
                return Ok(SealedVaultState::Uninitialized);
            }
            let unlocked = self
                .unlocked
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .contains(profile_id.as_str());
            Ok(if unlocked {
                SealedVaultState::Unlocked(SealedVaultProtection::HardwareBacked)
            } else {
                SealedVaultState::Locked(SealedVaultProtection::HardwareBacked)
            })
        }

        fn initialize(
            &self,
            profile_id: &WalletProfileId,
            plaintext: &[u8],
        ) -> Result<SealedVaultProtection, SealedVaultError> {
            if self.deny_initialize.load(Ordering::Relaxed) {
                return Err(SealedVaultError::AuthorizationDenied);
            }
            let mut records = self
                .records
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?;
            if records.contains_key(profile_id.as_str()) {
                return Err(SealedVaultError::AlreadyInitialized);
            }
            records.insert(profile_id.as_str().to_owned(), plaintext.to_vec());
            self.unlocked
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .insert(profile_id.as_str().to_owned());
            Ok(SealedVaultProtection::HardwareBacked)
        }

        fn unlock(
            &self,
            profile_id: &WalletProfileId,
            reason: &str,
        ) -> Result<Zeroizing<Vec<u8>>, SealedVaultError> {
            self.unlock_reasons
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .push(reason.to_owned());
            if self.deny_unlock.load(Ordering::Relaxed) {
                return Err(SealedVaultError::AuthorizationDenied);
            }
            let bytes = self
                .records
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .get(profile_id.as_str())
                .cloned()
                .ok_or(SealedVaultError::NotInitialized)?;
            self.unlocked
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .insert(profile_id.as_str().to_owned());
            Ok(Zeroizing::new(bytes))
        }

        fn load(
            &self,
            profile_id: &WalletProfileId,
        ) -> Result<Zeroizing<Vec<u8>>, SealedVaultError> {
            if !self
                .unlocked
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .contains(profile_id.as_str())
            {
                return Err(SealedVaultError::Locked);
            }
            self.records
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .get(profile_id.as_str())
                .cloned()
                .map(Zeroizing::new)
                .ok_or(SealedVaultError::NotInitialized)
        }

        fn save(
            &self,
            profile_id: &WalletProfileId,
            plaintext: &[u8],
        ) -> Result<(), SealedVaultError> {
            if !self
                .unlocked
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .contains(profile_id.as_str())
            {
                return Err(SealedVaultError::Locked);
            }
            self.records
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .insert(profile_id.as_str().to_owned(), plaintext.to_vec());
            Ok(())
        }

        fn lock(&self, profile_id: &WalletProfileId) -> Result<(), SealedVaultError> {
            let exists = self
                .records
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .contains_key(profile_id.as_str());
            if !exists {
                return Err(SealedVaultError::NotInitialized);
            }
            self.unlocked
                .lock()
                .map_err(|_| SealedVaultError::Unavailable)?
                .remove(profile_id.as_str());
            Ok(())
        }
    }

    fn adapter(
        backend: Arc<TestSealedVault>,
    ) -> MobileWalletSecurity<FixedClock, IncrementingRandom, TestSealedVault> {
        MobileWalletSecurity::new(
            Arc::new(FixedClock),
            Arc::new(IncrementingRandom::new()),
            backend,
        )
    }

    #[test]
    fn sealed_keys_survive_restart_and_require_a_new_authorized_session() {
        let backend = Arc::new(TestSealedVault::default());
        let profile = WalletProfileId::parse("profile_mobile").expect("profile");
        let first = adapter(Arc::clone(&backend));
        let initialized = first.initialize(&profile).expect("initialize");
        assert_eq!(
            initialized.protection(),
            WalletProtectionClass::HardwareBacked
        );
        assert!(initialized.user_presence_required());
        let descriptor = first
            .generate(
                &profile,
                GenerateProtectedKeyRequest {
                    label: WalletKeyLabel::parse("Authentication").expect("label"),
                    algorithm: WalletKeyAlgorithm::Ed25519,
                    purpose: WalletKeyPurpose::Authentication,
                },
            )
            .expect("generate");
        let signed = first
            .sign(&profile, descriptor.reference(), b"oxid-native-custody")
            .expect("sign");
        let verifying = VerifyingKey::from_bytes(
            descriptor
                .public_key()
                .bytes()
                .try_into()
                .expect("ed25519 public width"),
        )
        .expect("verifying key");
        verifying
            .verify(
                b"oxid-native-custody",
                &Signature::from_slice(signed.bytes()).expect("signature"),
            )
            .expect("valid signature");
        first.lock(&profile).expect("lock");

        let restarted = adapter(Arc::clone(&backend));
        assert_eq!(
            restarted.status(&profile).expect("status").state(),
            WalletProtectionState::Locked
        );
        backend.deny_unlock.store(true, Ordering::Relaxed);
        assert_eq!(
            restarted.sign(&profile, descriptor.reference(), b"blocked"),
            Err(WalletSecurityPortError::AuthorizationDenied)
        );
        backend.deny_unlock.store(false, Ordering::Relaxed);
        restarted
            .sign(&profile, descriptor.reference(), b"authorized")
            .expect("last-responsible authorization");
        assert_eq!(restarted.list(&profile).expect("list").len(), 1);

        let path = WalletHdPath::new(vec![
            WalletHdPathComponent::new(44, true).expect("purpose"),
            WalletHdPathComponent::new(2400, true).expect("coin"),
            WalletHdPathComponent::new(0, true).expect("account"),
            WalletHdPathComponent::new(0, false).expect("role"),
            WalletHdPathComponent::new(0, false).expect("index"),
        ])
        .expect("path");
        let derived = restarted
            .derive(
                &profile,
                DeriveProtectedKeyRequest {
                    label: WalletKeyLabel::parse("NIGHT external").expect("label"),
                    algorithm: WalletKeyAlgorithm::Secp256k1Schnorr,
                    purpose: WalletKeyPurpose::Transaction,
                    path,
                },
            )
            .expect("derive");
        let repeated = restarted
            .derive(
                &profile,
                DeriveProtectedKeyRequest {
                    label: WalletKeyLabel::parse("NIGHT external").expect("label"),
                    algorithm: WalletKeyAlgorithm::Secp256k1Schnorr,
                    purpose: WalletKeyPurpose::Transaction,
                    path: WalletHdPath::new(vec![
                        WalletHdPathComponent::new(44, true).expect("purpose"),
                        WalletHdPathComponent::new(2400, true).expect("coin"),
                        WalletHdPathComponent::new(0, true).expect("account"),
                        WalletHdPathComponent::new(0, false).expect("role"),
                        WalletHdPathComponent::new(0, false).expect("index"),
                    ])
                    .expect("path"),
                },
            )
            .expect("idempotent derive");
        assert_eq!(repeated, derived);
        assert_eq!(restarted.list(&profile).expect("derived list").len(), 2);
    }

    #[test]
    fn authorization_denial_and_corrupt_plaintext_fail_closed() {
        let backend = Arc::new(TestSealedVault::default());
        let profile = WalletProfileId::parse("profile_denied").expect("profile");
        let security = adapter(Arc::clone(&backend));
        security.initialize(&profile).expect("initialize");
        security.lock(&profile).expect("lock");
        backend.deny_unlock.store(true, Ordering::Relaxed);
        assert_eq!(
            security.unlock(&profile),
            Err(WalletSecurityPortError::AuthorizationDenied)
        );
        backend.deny_unlock.store(false, Ordering::Relaxed);
        backend.corrupt(&profile);
        assert_eq!(
            security.unlock(&profile),
            Err(WalletSecurityPortError::InvalidOperation)
        );
    }

    #[test]
    fn portable_backup_requires_fresh_authorization_and_restores_atomically() {
        let source_backend = Arc::new(TestSealedVault::default());
        let profile = WalletProfileId::parse("profile_portable").expect("profile");
        let source = adapter(Arc::clone(&source_backend));
        source.initialize(&profile).expect("initialize source");
        let descriptor = source
            .generate(
                &profile,
                GenerateProtectedKeyRequest {
                    label: WalletKeyLabel::parse("Portable authentication").expect("label"),
                    algorithm: WalletKeyAlgorithm::Ed25519,
                    purpose: WalletKeyPurpose::Authentication,
                },
            )
            .expect("generate protected key");
        let secret =
            WalletRecoverySecret::parse("correct horse battery staple").expect("recovery secret");

        source_backend.deny_unlock.store(true, Ordering::Relaxed);
        assert_eq!(
            source.export_portable_backup(&profile, &secret),
            Err(WalletPortableBackupPortError::AuthorizationDenied)
        );
        source_backend.deny_unlock.store(false, Ordering::Relaxed);
        let backup = source
            .export_portable_backup(&profile, &secret)
            .expect("freshly authorized export");
        assert_eq!(
            source_backend
                .unlock_reasons
                .lock()
                .expect("unlock reasons")
                .last()
                .map(String::as_str),
            Some(PORTABLE_BACKUP_EXPORT_REASON)
        );

        let destination_backend = Arc::new(TestSealedVault::default());
        let destination = adapter(Arc::clone(&destination_backend));
        destination_backend
            .deny_initialize
            .store(true, Ordering::Relaxed);
        assert_eq!(
            destination.recover_portable_backup(&profile, &backup, &secret),
            Err(WalletPortableBackupPortError::AuthorizationDenied)
        );
        assert_eq!(
            destination.status(&profile).expect("empty status").state(),
            WalletProtectionState::Uninitialized
        );
        destination_backend
            .deny_initialize
            .store(false, Ordering::Relaxed);
        let summary = destination
            .recover_portable_backup(&profile, &backup, &secret)
            .expect("authorized recovery");
        assert_eq!(summary.restored_key_count, 1);
        assert_eq!(
            destination.list(&profile).expect("restored descriptors"),
            vec![descriptor.clone()]
        );
        let signature = destination
            .sign(&profile, descriptor.reference(), b"restored mobile custody")
            .expect("restored key should sign");
        let verifying = VerifyingKey::from_bytes(
            descriptor
                .public_key()
                .bytes()
                .try_into()
                .expect("ed25519 public width"),
        )
        .expect("verifying key");
        verifying
            .verify(
                b"restored mobile custody",
                &Signature::from_slice(signature.bytes()).expect("signature"),
            )
            .expect("restored signature should verify");
        assert_eq!(
            destination.recover_portable_backup(&profile, &backup, &secret),
            Err(WalletPortableBackupPortError::AlreadyInitialized)
        );
    }

    #[test]
    fn portable_recovery_rejects_wrong_secret_tamper_and_profile() {
        let source_backend = Arc::new(TestSealedVault::default());
        let profile = WalletProfileId::parse("profile_source").expect("profile");
        let source = adapter(source_backend);
        source.initialize(&profile).expect("initialize source");
        let secret =
            WalletRecoverySecret::parse("correct horse battery staple").expect("recovery secret");
        let backup = source
            .export_portable_backup(&profile, &secret)
            .expect("backup");
        let destination = adapter(Arc::new(TestSealedVault::default()));
        let wrong_secret = WalletRecoverySecret::parse("wrong horse battery staple indeed")
            .expect("bounded wrong secret");
        assert_eq!(
            destination.recover_portable_backup(&profile, &backup, &wrong_secret),
            Err(WalletPortableBackupPortError::AuthenticationFailed)
        );

        let mut tampered_bytes = backup.into_bytes();
        *tampered_bytes.last_mut().expect("ciphertext") ^= 1;
        let tampered = PortableWalletBackup::parse(tampered_bytes).expect("bounded package");
        assert_eq!(
            destination.recover_portable_backup(&profile, &tampered, &secret),
            Err(WalletPortableBackupPortError::AuthenticationFailed)
        );

        let other_profile = WalletProfileId::parse("profile_other").expect("profile");
        let source = adapter(Arc::new(TestSealedVault::default()));
        source
            .initialize(&profile)
            .expect("initialize source again");
        let backup = source
            .export_portable_backup(&profile, &secret)
            .expect("backup again");
        assert_eq!(
            destination.recover_portable_backup(&other_profile, &backup, &secret),
            Err(WalletPortableBackupPortError::WrongProfile)
        );
    }

    #[test]
    fn host_native_backend_is_explicitly_unavailable() {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let profile = WalletProfileId::parse("profile_host").expect("profile");
            assert_eq!(
                NativeMobileSealedVault.inspect(&profile),
                Err(SealedVaultError::Unavailable)
            );
        }
    }
}
