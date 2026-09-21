// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs::OpenOptions,
    io::Write as _,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

use bip32::{ChildNumber, XPrv};
use ed25519_dalek::{Signer as _, SigningKey as Ed25519SigningKey};
use k256::schnorr::{SigningKey as Secp256k1SchnorrSigningKey, signature::Signer as _};
use oxid_adapter_backup_portable::{
    PortableCustodyKey, PortableCustodyVault, PortableCustodyVaultPort, PortableKeyMaterialRef,
    open_portable_custody, seal_portable_custody,
};
use oxid_platform_ports::{ClockPort, RandomPort};
use oxid_wallet_application::{
    DeriveProtectedKeyRequest, GenerateProtectedKeyRequest, JUBJUB_COMPACT_BYTES,
    PortableWalletBackup, WalletDerivedSecretUsePort, WalletHdPath, WalletJubjubChallengeDeriver,
    WalletJubjubChallengeSignature, WalletJubjubChallengeSigningPort, WalletKeyDerivationPort,
    WalletKeyOperationPort, WalletOnboardingAuthorizationError, WalletOnboardingAuthorizationPort,
    WalletPortableBackupPort, WalletPortableBackupPortError, WalletPortableRecoverySummary,
    WalletProtectionPort, WalletRecoverySecret, WalletRootRecoveryPort, WalletRootSeed,
    WalletSecurityPortError,
};
use oxid_wallet_domain::{
    PublicKeyEncoding, WalletKeyAlgorithm, WalletKeyDescriptor, WalletKeyReference,
    WalletProfileId, WalletProtectionClass, WalletProtectionState, WalletPublicKey,
    WalletSecurityStatus, WalletSignature,
};
use p256::ecdsa::{Signature as P256Signature, SigningKey as P256SigningKey};
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

#[cfg(feature = "development-fixture")]
mod development_fixture;
mod jubjub_schnorr;

#[cfg(feature = "development-fixture")]
pub use development_fixture::DevelopmentWalletFixtureProtection;

const KEY_REFERENCE_ATTEMPTS: usize = 8;
const P256_SCALAR_ATTEMPTS: usize = 128;
const SECP256K1_SCALAR_ATTEMPTS: usize = 128;
const JUBJUB_SEED_ATTEMPTS: usize = 128;
const DEVELOPMENT_STORE_KEY_BYTES: usize = 32;
const DEVELOPMENT_STORE_MAX_BACKUP_BYTES: usize = 80 * 1024 * 1024;
const DEVELOPMENT_STORE_KEY_FILE: &str = "development-custody.key";

/// Explicit non-production desktop policy. It deliberately does not claim
/// native user presence; production desktop composition remains unavailable.
#[derive(Clone, Copy, Debug, Default)]
pub struct DevelopmentWalletOnboardingAuthorization;

impl WalletOnboardingAuthorizationPort for DevelopmentWalletOnboardingAuthorization {
    fn authorize_recovery_phrase_reveal(&self) -> Result<(), WalletOnboardingAuthorizationError> {
        Ok(())
    }
}

/// Explicitly non-production adapter for tests and headless flows.
///
/// Secret key objects stay inside this adapter and are zeroized by their
/// cryptography implementations when removed or dropped. [`Self::new`] stays
/// process-local. [`Self::persistent`] additionally seals each profile into an
/// owner-private development store so desktop demos survive a process restart;
/// it is not a production custody claim or a substitute for native user presence.
pub struct DevelopmentWalletSecurity<C, N> {
    clock: Arc<C>,
    random: Arc<N>,
    profiles: Mutex<BTreeMap<String, DevelopmentProfile>>,
    persistence: Option<DevelopmentCustodyPersistence>,
}

struct DevelopmentCustodyPersistence {
    directory: PathBuf,
}

impl DevelopmentCustodyPersistence {
    fn profile_path(&self, profile_id: &WalletProfileId) -> PathBuf {
        let digest = Sha256::digest(profile_id.as_str().as_bytes());
        self.directory
            .join(format!("profile-{}.oxidbak", hex::encode(digest)))
    }

    fn key_path(&self) -> PathBuf {
        self.directory.join(DEVELOPMENT_STORE_KEY_FILE)
    }

    fn load_secret(&self) -> Result<WalletRecoverySecret, WalletSecurityPortError> {
        let Some(bytes) = oxid_adapter_store_atomic::read_owner_private_bounded(
            &self.key_path(),
            DEVELOPMENT_STORE_KEY_BYTES,
        )
        .map_err(|_| WalletSecurityPortError::InvalidOperation)?
        else {
            return Err(WalletSecurityPortError::InvalidOperation);
        };
        Self::secret_from_bytes(bytes)
    }

    fn load_or_create_secret<N>(
        &self,
        random: &N,
    ) -> Result<WalletRecoverySecret, WalletSecurityPortError>
    where
        N: RandomPort,
    {
        if self.key_path().exists() {
            return self.load_secret();
        }
        oxid_adapter_store_atomic::ensure_private_directory(&self.directory)
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        let mut bytes = Zeroizing::new(vec![0_u8; DEVELOPMENT_STORE_KEY_BYTES]);
        random
            .fill_bytes(&mut bytes)
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        match options.open(self.key_path()) {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|_| WalletSecurityPortError::Unavailable)?;
                Self::secret_from_bytes(Vec::from(bytes.as_slice()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => self.load_secret(),
            Err(_) => Err(WalletSecurityPortError::Unavailable),
        }
    }

    fn secret_from_bytes(bytes: Vec<u8>) -> Result<WalletRecoverySecret, WalletSecurityPortError> {
        let bytes = Zeroizing::new(bytes);
        if bytes.len() != DEVELOPMENT_STORE_KEY_BYTES {
            return Err(WalletSecurityPortError::InvalidOperation);
        }
        let encoded = Zeroizing::new(hex::encode(bytes.as_slice()));
        WalletRecoverySecret::parse(encoded.as_str())
            .map_err(|_| WalletSecurityPortError::InvalidOperation)
    }
}

impl<C, N> DevelopmentWalletSecurity<C, N> {
    #[must_use]
    pub fn new(clock: Arc<C>, random: Arc<N>) -> Self {
        Self {
            clock,
            random,
            profiles: Mutex::new(BTreeMap::new()),
            persistence: None,
        }
    }

    /// Creates an explicit development-only encrypted custody store.
    ///
    /// The wrapping key and encrypted profile vaults are separate owner-only
    /// files. This protects against accidental disclosure and torn writes but
    /// deliberately does not claim hardware-backed or user-presence security.
    #[must_use]
    pub fn persistent(clock: Arc<C>, random: Arc<N>, directory: impl Into<PathBuf>) -> Self {
        Self {
            clock,
            random,
            profiles: Mutex::new(BTreeMap::new()),
            persistence: Some(DevelopmentCustodyPersistence {
                directory: directory.into(),
            }),
        }
    }

    /// Initializes one explicitly identified development profile from a typed root.
    ///
    /// This adapter remains process-local and explicitly insecure. Generic
    /// randomness is still used for every key reference, nonce, generated key,
    /// and ordinary profile root; the supplied value can never satisfy those
    /// requests accidentally.
    #[cfg(feature = "development-fixture")]
    pub(crate) fn initialize_with_root_seed(
        &self,
        profile_id: &WalletProfileId,
        root_seed: Zeroizing<[u8; 32]>,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError>
    where
        N: RandomPort,
    {
        self.initialize_profile(
            profile_id,
            Some(WalletRootSeed::from_raw_development(*root_seed)),
        )
    }

    fn profiles(
        &self,
    ) -> Result<MutexGuard<'_, BTreeMap<String, DevelopmentProfile>>, WalletSecurityPortError> {
        self.profiles
            .lock()
            .map_err(|_| WalletSecurityPortError::Unavailable)
    }

    fn ensure_loaded(&self, profile_id: &WalletProfileId) -> Result<(), WalletSecurityPortError> {
        let Some(persistence) = &self.persistence else {
            return Ok(());
        };
        let mut profiles = self.profiles()?;
        if profiles.contains_key(profile_id.as_str()) {
            return Ok(());
        }
        let path = persistence.profile_path(profile_id);
        let Some(bytes) = oxid_adapter_store_atomic::read_owner_private_bounded(
            &path,
            DEVELOPMENT_STORE_MAX_BACKUP_BYTES,
        )
        .map_err(|_| WalletSecurityPortError::InvalidOperation)?
        else {
            return Ok(());
        };
        let secret = persistence.load_secret()?;
        let backup = PortableWalletBackup::parse(bytes)
            .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
        let vault = open_portable_custody(&backup, &secret, profile_id)
            .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
        let profile = Self::restored_profile(&vault).map_err(map_backup_to_security_error)?;
        profiles.insert(profile_id.as_str().to_owned(), profile);
        Ok(())
    }

    fn persist_profile(
        &self,
        profile_id: &WalletProfileId,
        profile: &DevelopmentProfile,
    ) -> Result<(), WalletSecurityPortError>
    where
        C: ClockPort,
        N: RandomPort,
    {
        let Some(persistence) = &self.persistence else {
            return Ok(());
        };
        let vault = self
            .portable_vault(profile_id, profile)
            .map_err(map_backup_to_security_error)?;
        let secret = persistence.load_or_create_secret(self.random.as_ref())?;
        let backup = seal_portable_custody(&vault, &secret, self.random.as_ref())
            .map_err(map_backup_to_security_error)?;
        oxid_adapter_store_atomic::write_owner_private(
            &persistence.profile_path(profile_id),
            backup.as_bytes(),
        )
        .map_err(|_| WalletSecurityPortError::Unavailable)
    }

    fn portable_vault(
        &self,
        profile_id: &WalletProfileId,
        profile: &DevelopmentProfile,
    ) -> Result<PortableCustodyVault, WalletPortableBackupPortError>
    where
        C: ClockPort,
    {
        let keys = profile
            .keys
            .values()
            .map(|stored| {
                if let Some(path) = &stored.derivation {
                    return Ok(PortableCustodyKey::derived(
                        stored.descriptor.clone(),
                        path.clone(),
                    ));
                }
                let secret = match &stored.material {
                    DevelopmentKeyMaterial::Ed25519(key) => key.to_bytes(),
                    DevelopmentKeyMaterial::P256(key) => key.to_bytes().into(),
                    DevelopmentKeyMaterial::Secp256k1Schnorr(key) => key.to_bytes().into(),
                    DevelopmentKeyMaterial::Jubjub(key) => key.seed_bytes(),
                };
                Ok(PortableCustodyKey::generated(
                    stored.descriptor.clone(),
                    secret,
                ))
            })
            .collect::<Result<Vec<_>, WalletPortableBackupPortError>>()?;
        let exported_at_millis = self
            .clock
            .now()
            .map_err(|_| WalletPortableBackupPortError::Unavailable)?
            .value();
        PortableCustodyVault::new_with_root(
            profile_id.clone(),
            exported_at_millis,
            profile.root_seed.copy_for_protected_import(),
            keys,
        )
    }

    fn unlocked_profile<'a>(
        profiles: &'a BTreeMap<String, DevelopmentProfile>,
        profile_id: &WalletProfileId,
    ) -> Result<&'a DevelopmentProfile, WalletSecurityPortError> {
        let profile = profiles
            .get(profile_id.as_str())
            .ok_or(WalletSecurityPortError::NotInitialized)?;
        if profile.state != WalletProtectionState::Unlocked {
            return Err(WalletSecurityPortError::Locked);
        }
        Ok(profile)
    }

    fn unlocked_profile_mut<'a>(
        profiles: &'a mut BTreeMap<String, DevelopmentProfile>,
        profile_id: &WalletProfileId,
    ) -> Result<&'a mut DevelopmentProfile, WalletSecurityPortError> {
        let profile = profiles
            .get_mut(profile_id.as_str())
            .ok_or(WalletSecurityPortError::NotInitialized)?;
        if profile.state != WalletProtectionState::Unlocked {
            return Err(WalletSecurityPortError::Locked);
        }
        Ok(profile)
    }

    fn initialize_profile(
        &self,
        profile_id: &WalletProfileId,
        root_seed: Option<WalletRootSeed>,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError>
    where
        C: ClockPort,
        N: RandomPort,
    {
        self.ensure_loaded(profile_id)?;
        let mut profiles = self.profiles()?;
        if profiles.contains_key(profile_id.as_str()) {
            return Err(WalletSecurityPortError::AlreadyInitialized);
        }
        let root_seed = if let Some(root_seed) = root_seed {
            root_seed
        } else {
            let mut root_seed = Zeroizing::new([0_u8; 32]);
            self.random
                .fill_bytes(root_seed.as_mut())
                .map_err(|_| WalletSecurityPortError::Unavailable)?;
            WalletRootSeed::from_raw_development(*root_seed)
        };
        let profile = DevelopmentProfile {
            state: WalletProtectionState::Unlocked,
            root_seed,
            keys: BTreeMap::new(),
        };
        self.persist_profile(profile_id, &profile)?;
        profiles.insert(profile_id.as_str().to_owned(), profile);
        Ok(development_status(WalletProtectionState::Unlocked))
    }

    fn new_reference(
        &self,
        keys: &BTreeMap<String, StoredDevelopmentKey>,
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
            if !keys.contains_key(&value) {
                return WalletKeyReference::parse(value)
                    .map_err(|_| WalletSecurityPortError::InvalidOperation);
            }
        }
        Err(WalletSecurityPortError::Conflict)
    }

    fn generate_material(
        &self,
        algorithm: WalletKeyAlgorithm,
    ) -> Result<(DevelopmentKeyMaterial, WalletPublicKey), WalletSecurityPortError>
    where
        N: RandomPort,
    {
        match algorithm {
            WalletKeyAlgorithm::Ed25519 => {
                let mut secret = Zeroizing::new([0_u8; 32]);
                self.random
                    .fill_bytes(secret.as_mut())
                    .map_err(|_| WalletSecurityPortError::Unavailable)?;
                let signing_key = Ed25519SigningKey::from_bytes(&secret);
                let public_key = WalletPublicKey::new(
                    PublicKeyEncoding::Ed25519Compressed,
                    signing_key.verifying_key().to_bytes().to_vec(),
                );
                Ok((DevelopmentKeyMaterial::Ed25519(signing_key), public_key))
            }
            WalletKeyAlgorithm::P256 => {
                for _ in 0..P256_SCALAR_ATTEMPTS {
                    let mut secret = Zeroizing::new([0_u8; 32]);
                    self.random
                        .fill_bytes(secret.as_mut())
                        .map_err(|_| WalletSecurityPortError::Unavailable)?;
                    if let Ok(signing_key) = P256SigningKey::from_slice(secret.as_ref()) {
                        let public_key = WalletPublicKey::new(
                            PublicKeyEncoding::Sec1Compressed,
                            signing_key
                                .verifying_key()
                                .to_sec1_point(true)
                                .as_bytes()
                                .to_vec(),
                        );
                        return Ok((DevelopmentKeyMaterial::P256(signing_key), public_key));
                    }
                }
                Err(WalletSecurityPortError::InvalidOperation)
            }
            WalletKeyAlgorithm::Secp256k1Schnorr => {
                for _ in 0..SECP256K1_SCALAR_ATTEMPTS {
                    let mut secret = Zeroizing::new([0_u8; 32]);
                    self.random
                        .fill_bytes(secret.as_mut())
                        .map_err(|_| WalletSecurityPortError::Unavailable)?;
                    if let Ok(signing_key) = Secp256k1SchnorrSigningKey::from_bytes(secret.as_ref())
                    {
                        let public_key = WalletPublicKey::new(
                            PublicKeyEncoding::Secp256k1XOnly,
                            signing_key.verifying_key().to_bytes().to_vec(),
                        );
                        return Ok((
                            DevelopmentKeyMaterial::Secp256k1Schnorr(signing_key),
                            public_key,
                        ));
                    }
                }
                Err(WalletSecurityPortError::InvalidOperation)
            }
            WalletKeyAlgorithm::Jubjub => {
                for _ in 0..JUBJUB_SEED_ATTEMPTS {
                    let mut seed = Zeroizing::new([0_u8; 32]);
                    self.random
                        .fill_bytes(seed.as_mut())
                        .map_err(|_| WalletSecurityPortError::Unavailable)?;
                    if let Some(signing_key) = jubjub_schnorr::SigningKey::from_seed(seed) {
                        let public_key = WalletPublicKey::new(
                            PublicKeyEncoding::JubjubCompressed,
                            signing_key.compressed_public_key()?,
                        );
                        return Ok((DevelopmentKeyMaterial::Jubjub(signing_key), public_key));
                    }
                }
                Err(WalletSecurityPortError::InvalidOperation)
            }
        }
    }

    fn derive_material(
        root_seed: &[u8],
        path: &WalletHdPath,
        algorithm: WalletKeyAlgorithm,
    ) -> Result<(DevelopmentKeyMaterial, WalletPublicKey), WalletSecurityPortError> {
        if algorithm != WalletKeyAlgorithm::Secp256k1Schnorr {
            return Err(WalletSecurityPortError::UnsupportedAlgorithm);
        }

        let private_bytes = Self::derive_secret(root_seed, path)?;
        let signing_key = Secp256k1SchnorrSigningKey::from_bytes(private_bytes.as_ref())
            .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
        let public_key = WalletPublicKey::new(
            PublicKeyEncoding::Secp256k1XOnly,
            signing_key.verifying_key().to_bytes().to_vec(),
        );
        Ok((
            DevelopmentKeyMaterial::Secp256k1Schnorr(signing_key),
            public_key,
        ))
    }

    fn derive_secret(
        root_seed: &[u8],
        path: &WalletHdPath,
    ) -> Result<Zeroizing<[u8; 32]>, WalletSecurityPortError> {
        let mut extended =
            XPrv::new(root_seed).map_err(|_| WalletSecurityPortError::InvalidOperation)?;
        for component in path.components() {
            let child = ChildNumber::new(component.index(), component.hardened())
                .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
            extended = extended
                .derive_child(child)
                .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
        }
        Ok(Zeroizing::new(extended.to_bytes()))
    }

    fn material_from_secret(
        algorithm: WalletKeyAlgorithm,
        secret: [u8; 32],
    ) -> Result<(DevelopmentKeyMaterial, WalletPublicKey), WalletSecurityPortError> {
        let secret = Zeroizing::new(secret);
        match algorithm {
            WalletKeyAlgorithm::Ed25519 => {
                let signing_key = Ed25519SigningKey::from_bytes(&secret);
                let public_key = WalletPublicKey::new(
                    PublicKeyEncoding::Ed25519Compressed,
                    signing_key.verifying_key().to_bytes().to_vec(),
                );
                Ok((DevelopmentKeyMaterial::Ed25519(signing_key), public_key))
            }
            WalletKeyAlgorithm::P256 => {
                let signing_key = P256SigningKey::from_slice(secret.as_ref())
                    .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
                let public_key = WalletPublicKey::new(
                    PublicKeyEncoding::Sec1Compressed,
                    signing_key
                        .verifying_key()
                        .to_sec1_point(true)
                        .as_bytes()
                        .to_vec(),
                );
                Ok((DevelopmentKeyMaterial::P256(signing_key), public_key))
            }
            WalletKeyAlgorithm::Secp256k1Schnorr => {
                let signing_key = Secp256k1SchnorrSigningKey::from_bytes(secret.as_ref())
                    .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
                let public_key = WalletPublicKey::new(
                    PublicKeyEncoding::Secp256k1XOnly,
                    signing_key.verifying_key().to_bytes().to_vec(),
                );
                Ok((
                    DevelopmentKeyMaterial::Secp256k1Schnorr(signing_key),
                    public_key,
                ))
            }
            WalletKeyAlgorithm::Jubjub => {
                let signing_key = jubjub_schnorr::SigningKey::from_seed(secret)
                    .ok_or(WalletSecurityPortError::InvalidOperation)?;
                let public_key = WalletPublicKey::new(
                    PublicKeyEncoding::JubjubCompressed,
                    signing_key.compressed_public_key()?,
                );
                Ok((DevelopmentKeyMaterial::Jubjub(signing_key), public_key))
            }
        }
    }

    fn restored_profile(
        vault: &PortableCustodyVault,
    ) -> Result<DevelopmentProfile, WalletPortableBackupPortError> {
        let mut restored_keys = BTreeMap::new();
        for key in vault.keys() {
            let descriptor = key.descriptor().clone();
            let (material, public_key, derivation) = match key.material() {
                PortableKeyMaterialRef::Generated(secret) => {
                    let (material, public_key) =
                        Self::material_from_secret(descriptor.algorithm(), *secret)
                            .map_err(map_backup_security_error)?;
                    (material, public_key, None)
                }
                PortableKeyMaterialRef::Derived(path) => {
                    let (material, public_key) =
                        Self::derive_material(vault.root_seed(), path, descriptor.algorithm())
                            .map_err(map_backup_security_error)?;
                    (material, public_key, Some(path.clone()))
                }
            };
            if descriptor.public_key() != &public_key {
                return Err(WalletPortableBackupPortError::InvalidPackage);
            }
            restored_keys.insert(
                descriptor.reference().as_str().to_owned(),
                StoredDevelopmentKey {
                    descriptor,
                    material,
                    derivation,
                },
            );
        }
        Ok(DevelopmentProfile {
            state: WalletProtectionState::Unlocked,
            root_seed: vault.copy_root_seed_for_protected_import(),
            keys: restored_keys,
        })
    }
}

impl<C, N> WalletProtectionPort for DevelopmentWalletSecurity<C, N>
where
    C: ClockPort,
    N: RandomPort,
{
    fn status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let profiles = self.profiles()?;
        let state = profiles
            .get(profile_id.as_str())
            .map_or(WalletProtectionState::Uninitialized, |profile| {
                profile.state
            });
        Ok(development_status(state))
    }

    fn initialize(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        self.initialize_profile(profile_id, None)
    }

    fn unlock(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let mut profiles = self.profiles()?;
        let profile = profiles
            .get_mut(profile_id.as_str())
            .ok_or(WalletSecurityPortError::NotInitialized)?;
        profile.state = WalletProtectionState::Unlocked;
        Ok(development_status(profile.state))
    }

    fn lock(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let mut profiles = self.profiles()?;
        let profile = profiles
            .get_mut(profile_id.as_str())
            .ok_or(WalletSecurityPortError::NotInitialized)?;
        profile.state = WalletProtectionState::Locked;
        Ok(development_status(profile.state))
    }
}

impl<C, N> WalletRootRecoveryPort for DevelopmentWalletSecurity<C, N>
where
    C: ClockPort,
    N: RandomPort,
{
    fn recover_root(
        &self,
        profile_id: &WalletProfileId,
        root: WalletRootSeed,
    ) -> Result<(), WalletSecurityPortError> {
        self.initialize_profile(profile_id, Some(root)).map(|_| ())
    }
}

impl<C, N> WalletKeyOperationPort for DevelopmentWalletSecurity<C, N>
where
    C: ClockPort,
    N: RandomPort,
{
    fn generate(
        &self,
        profile_id: &WalletProfileId,
        request: GenerateProtectedKeyRequest,
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let mut profiles = self.profiles()?;
        let profile = Self::unlocked_profile_mut(&mut profiles, profile_id)?;
        if profile
            .keys
            .values()
            .any(|key| key.descriptor.label() == &request.label)
        {
            return Err(WalletSecurityPortError::Conflict);
        }

        let reference = self.new_reference(&profile.keys)?;
        let (material, public_key) = self.generate_material(request.algorithm)?;
        let created_at = self
            .clock
            .now()
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        let descriptor = WalletKeyDescriptor::new(
            reference.clone(),
            request.label,
            request.algorithm,
            request.purpose,
            public_key,
            created_at,
        );
        profile.keys.insert(
            reference.as_str().to_owned(),
            StoredDevelopmentKey {
                descriptor: descriptor.clone(),
                material,
                derivation: None,
            },
        );
        if let Err(error) = self.persist_profile(profile_id, profile) {
            profile.keys.remove(reference.as_str());
            return Err(error);
        }
        Ok(descriptor)
    }

    fn list(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Vec<WalletKeyDescriptor>, WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let profiles = self.profiles()?;
        let profile = Self::unlocked_profile(&profiles, profile_id)?;
        Ok(profile
            .keys
            .values()
            .map(|key| key.descriptor.clone())
            .collect())
    }

    fn sign(
        &self,
        profile_id: &WalletProfileId,
        key_reference: &WalletKeyReference,
        payload: &[u8],
    ) -> Result<WalletSignature, WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let profiles = self.profiles()?;
        let profile = Self::unlocked_profile(&profiles, profile_id)?;
        let key = profile
            .keys
            .get(key_reference.as_str())
            .ok_or(WalletSecurityPortError::NotFound)?;
        match &key.material {
            DevelopmentKeyMaterial::Ed25519(signing_key) => Ok(WalletSignature::new(
                WalletKeyAlgorithm::Ed25519,
                signing_key.sign(payload).to_bytes().to_vec(),
            )),
            DevelopmentKeyMaterial::P256(signing_key) => {
                let signature: P256Signature = signing_key.sign(payload);
                Ok(WalletSignature::new(
                    WalletKeyAlgorithm::P256,
                    signature.to_bytes().to_vec(),
                ))
            }
            DevelopmentKeyMaterial::Secp256k1Schnorr(signing_key) => {
                let signature: k256::schnorr::Signature = signing_key.sign(payload);
                Ok(WalletSignature::new(
                    WalletKeyAlgorithm::Secp256k1Schnorr,
                    signature.to_bytes().to_vec(),
                ))
            }
            DevelopmentKeyMaterial::Jubjub(signing_key) => Ok(WalletSignature::new(
                WalletKeyAlgorithm::Jubjub,
                signing_key.sign(payload)?,
            )),
        }
    }

    fn delete(
        &self,
        profile_id: &WalletProfileId,
        key_reference: &WalletKeyReference,
    ) -> Result<(), WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let mut profiles = self.profiles()?;
        let profile = Self::unlocked_profile_mut(&mut profiles, profile_id)?;
        let removed = profile
            .keys
            .remove(key_reference.as_str())
            .ok_or(WalletSecurityPortError::NotFound)?;
        if let Err(error) = self.persist_profile(profile_id, profile) {
            profile
                .keys
                .insert(key_reference.as_str().to_owned(), removed);
            return Err(error);
        }
        Ok(())
    }
}

impl<C, N> WalletJubjubChallengeSigningPort for DevelopmentWalletSecurity<C, N>
where
    C: ClockPort,
    N: RandomPort,
{
    fn sign_jubjub_challenge(
        &self,
        profile_id: &WalletProfileId,
        key_reference: &WalletKeyReference,
        derive_challenge: &mut WalletJubjubChallengeDeriver<'_>,
    ) -> Result<WalletJubjubChallengeSignature, WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let mut nonce_seed = Zeroizing::new([0_u8; JUBJUB_COMPACT_BYTES]);
        self.random
            .fill_bytes(nonce_seed.as_mut())
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        let profiles = self.profiles()?;
        let profile = Self::unlocked_profile(&profiles, profile_id)?;
        let key = profile
            .keys
            .get(key_reference.as_str())
            .ok_or(WalletSecurityPortError::NotFound)?;
        match &key.material {
            DevelopmentKeyMaterial::Jubjub(signing_key) => {
                signing_key.sign_challenge(&nonce_seed, derive_challenge)
            }
            _ => Err(WalletSecurityPortError::UnsupportedAlgorithm),
        }
    }
}

impl<C, N> WalletKeyDerivationPort for DevelopmentWalletSecurity<C, N>
where
    C: ClockPort,
    N: RandomPort,
{
    fn derive(
        &self,
        profile_id: &WalletProfileId,
        request: DeriveProtectedKeyRequest,
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let mut profiles = self.profiles()?;
        let profile = Self::unlocked_profile_mut(&mut profiles, profile_id)?;

        if let Some(existing) = profile
            .keys
            .values()
            .find(|key| key.derivation.as_ref() == Some(&request.path))
        {
            if existing.descriptor.label() == &request.label
                && existing.descriptor.algorithm() == request.algorithm
                && existing.descriptor.purpose() == request.purpose
            {
                return Ok(existing.descriptor.clone());
            }
            return Err(WalletSecurityPortError::Conflict);
        }
        if profile
            .keys
            .values()
            .any(|key| key.descriptor.label() == &request.label)
        {
            return Err(WalletSecurityPortError::Conflict);
        }

        let reference = self.new_reference(&profile.keys)?;
        let (material, public_key) = Self::derive_material(
            profile.root_seed.expose_for_protected_use(),
            &request.path,
            request.algorithm,
        )?;
        let created_at = self
            .clock
            .now()
            .map_err(|_| WalletSecurityPortError::Unavailable)?;
        let descriptor = WalletKeyDescriptor::new(
            reference.clone(),
            request.label,
            request.algorithm,
            request.purpose,
            public_key,
            created_at,
        );
        profile.keys.insert(
            reference.as_str().to_owned(),
            StoredDevelopmentKey {
                descriptor: descriptor.clone(),
                material,
                derivation: Some(request.path),
            },
        );
        if let Err(error) = self.persist_profile(profile_id, profile) {
            profile.keys.remove(reference.as_str());
            return Err(error);
        }
        Ok(descriptor)
    }
}

impl<C, N> WalletDerivedSecretUsePort for DevelopmentWalletSecurity<C, N>
where
    C: ClockPort,
    N: RandomPort,
{
    fn use_derived_secret(
        &self,
        profile_id: &WalletProfileId,
        path: &WalletHdPath,
        operation: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
    ) -> Result<(), WalletSecurityPortError> {
        self.ensure_loaded(profile_id)?;
        let secret = {
            let profiles = self.profiles()?;
            let profile = Self::unlocked_profile(&profiles, profile_id)?;
            Self::derive_secret(profile.root_seed.expose_for_protected_use(), path)?
        };
        operation(&secret)
    }
}

impl<C, N> PortableCustodyVaultPort for DevelopmentWalletSecurity<C, N>
where
    C: ClockPort,
    N: RandomPort,
{
    fn export_custody_vault(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<PortableCustodyVault, WalletPortableBackupPortError> {
        self.ensure_loaded(profile_id)
            .map_err(map_backup_security_error)?;
        let profiles = self.profiles().map_err(map_backup_security_error)?;
        let profile =
            Self::unlocked_profile(&profiles, profile_id).map_err(map_backup_security_error)?;
        self.portable_vault(profile_id, profile)
    }

    fn preflight_custody_recovery(
        &self,
        vault: &PortableCustodyVault,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
        self.ensure_loaded(vault.profile_id())
            .map_err(map_backup_security_error)?;
        let profiles = self.profiles().map_err(map_backup_security_error)?;
        if profiles.contains_key(vault.profile_id().as_str()) {
            return Err(WalletPortableBackupPortError::AlreadyInitialized);
        }
        let restored_key_count = Self::restored_profile(vault)?.keys.len();
        Ok(WalletPortableRecoverySummary { restored_key_count })
    }

    fn recover_custody_vault(
        &self,
        vault: &PortableCustodyVault,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
        self.preflight_custody_recovery(vault)?;
        let restored = Self::restored_profile(vault)?;
        let restored_key_count = restored.keys.len();
        self.persist_profile(vault.profile_id(), &restored)
            .map_err(map_backup_security_error)?;
        let mut profiles = self.profiles().map_err(map_backup_security_error)?;
        if profiles.contains_key(vault.profile_id().as_str()) {
            return Err(WalletPortableBackupPortError::Conflict);
        }
        profiles.insert(vault.profile_id().as_str().to_owned(), restored);
        Ok(WalletPortableRecoverySummary { restored_key_count })
    }

    fn verify_recovered_custody(
        &self,
        vault: &PortableCustodyVault,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
        let current = self.export_custody_vault(vault.profile_id())?;
        if !current.matches_recovered_state(vault) {
            return Err(WalletPortableBackupPortError::Conflict);
        }
        Ok(WalletPortableRecoverySummary {
            restored_key_count: current.keys().len(),
        })
    }
}

impl<C, N> WalletPortableBackupPort for DevelopmentWalletSecurity<C, N>
where
    C: ClockPort,
    N: RandomPort,
{
    fn export_portable_backup(
        &self,
        profile_id: &WalletProfileId,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
        let vault = self.export_custody_vault(profile_id)?;
        seal_portable_custody(&vault, recovery_secret, self.random.as_ref())
    }

    fn recover_portable_backup(
        &self,
        profile_id: &WalletProfileId,
        backup: &PortableWalletBackup,
        recovery_secret: &WalletRecoverySecret,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
        self.ensure_loaded(profile_id)
            .map_err(map_backup_security_error)?;
        {
            let profiles = self.profiles().map_err(map_backup_security_error)?;
            if profiles.contains_key(profile_id.as_str()) {
                return Err(WalletPortableBackupPortError::AlreadyInitialized);
            }
        }
        let vault = open_portable_custody(backup, recovery_secret, profile_id)?;
        self.recover_custody_vault(&vault)
    }
}

/// Fail-closed adapter used until a production platform adapter is composed.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableWalletSecurity;

impl WalletProtectionPort for UnavailableWalletSecurity {
    fn status(&self, _: &WalletProfileId) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        Ok(WalletSecurityStatus::unavailable())
    }

    fn initialize(
        &self,
        _: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }

    fn unlock(&self, _: &WalletProfileId) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }

    fn lock(&self, _: &WalletProfileId) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }
}

impl WalletRootRecoveryPort for UnavailableWalletSecurity {
    fn recover_root(
        &self,
        _: &WalletProfileId,
        _: WalletRootSeed,
    ) -> Result<(), WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }
}

impl WalletKeyOperationPort for UnavailableWalletSecurity {
    fn generate(
        &self,
        _: &WalletProfileId,
        _: GenerateProtectedKeyRequest,
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }

    fn list(
        &self,
        _: &WalletProfileId,
    ) -> Result<Vec<WalletKeyDescriptor>, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }

    fn sign(
        &self,
        _: &WalletProfileId,
        _: &WalletKeyReference,
        _: &[u8],
    ) -> Result<WalletSignature, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }

    fn delete(
        &self,
        _: &WalletProfileId,
        _: &WalletKeyReference,
    ) -> Result<(), WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }
}

impl WalletKeyDerivationPort for UnavailableWalletSecurity {
    fn derive(
        &self,
        _: &WalletProfileId,
        _: DeriveProtectedKeyRequest,
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }
}

impl WalletJubjubChallengeSigningPort for UnavailableWalletSecurity {
    fn sign_jubjub_challenge(
        &self,
        _: &WalletProfileId,
        _: &WalletKeyReference,
        _: &mut WalletJubjubChallengeDeriver<'_>,
    ) -> Result<WalletJubjubChallengeSignature, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }
}

impl WalletDerivedSecretUsePort for UnavailableWalletSecurity {
    fn use_derived_secret(
        &self,
        _: &WalletProfileId,
        _: &WalletHdPath,
        _: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
    ) -> Result<(), WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }
}

impl PortableCustodyVaultPort for UnavailableWalletSecurity {
    fn export_custody_vault(
        &self,
        _: &WalletProfileId,
    ) -> Result<PortableCustodyVault, WalletPortableBackupPortError> {
        Err(WalletPortableBackupPortError::Unavailable)
    }

    fn preflight_custody_recovery(
        &self,
        _: &PortableCustodyVault,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
        Err(WalletPortableBackupPortError::Unavailable)
    }

    fn recover_custody_vault(
        &self,
        _: &PortableCustodyVault,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
        Err(WalletPortableBackupPortError::Unavailable)
    }

    fn verify_recovered_custody(
        &self,
        _: &PortableCustodyVault,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
        Err(WalletPortableBackupPortError::Unavailable)
    }
}

impl WalletPortableBackupPort for UnavailableWalletSecurity {
    fn export_portable_backup(
        &self,
        _: &WalletProfileId,
        _: &WalletRecoverySecret,
    ) -> Result<PortableWalletBackup, WalletPortableBackupPortError> {
        Err(WalletPortableBackupPortError::Unavailable)
    }

    fn recover_portable_backup(
        &self,
        _: &WalletProfileId,
        _: &PortableWalletBackup,
        _: &WalletRecoverySecret,
    ) -> Result<WalletPortableRecoverySummary, WalletPortableBackupPortError> {
        Err(WalletPortableBackupPortError::Unavailable)
    }
}

struct DevelopmentProfile {
    state: WalletProtectionState,
    root_seed: WalletRootSeed,
    keys: BTreeMap<String, StoredDevelopmentKey>,
}

struct StoredDevelopmentKey {
    descriptor: WalletKeyDescriptor,
    material: DevelopmentKeyMaterial,
    derivation: Option<WalletHdPath>,
}

enum DevelopmentKeyMaterial {
    Ed25519(Ed25519SigningKey),
    P256(P256SigningKey),
    Secp256k1Schnorr(Secp256k1SchnorrSigningKey),
    Jubjub(jubjub_schnorr::SigningKey),
}

const fn development_status(state: WalletProtectionState) -> WalletSecurityStatus {
    WalletSecurityStatus::new(state, WalletProtectionClass::DevelopmentOnly, false, true)
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

const fn map_backup_to_security_error(
    error: WalletPortableBackupPortError,
) -> WalletSecurityPortError {
    match error {
        WalletPortableBackupPortError::Unavailable => WalletSecurityPortError::Unavailable,
        WalletPortableBackupPortError::NotInitialized => WalletSecurityPortError::NotInitialized,
        WalletPortableBackupPortError::AlreadyInitialized => {
            WalletSecurityPortError::AlreadyInitialized
        }
        WalletPortableBackupPortError::Locked => WalletSecurityPortError::Locked,
        WalletPortableBackupPortError::AuthorizationDenied => {
            WalletSecurityPortError::AuthorizationDenied
        }
        WalletPortableBackupPortError::Conflict => WalletSecurityPortError::Conflict,
        WalletPortableBackupPortError::InvalidPackage
        | WalletPortableBackupPortError::AuthenticationFailed
        | WalletPortableBackupPortError::WrongProfile
        | WalletPortableBackupPortError::InvalidOperation => {
            WalletSecurityPortError::InvalidOperation
        }
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature as Ed25519Signature, Verifier as _, VerifyingKey};
    use k256::schnorr::{
        Signature as SchnorrSignature, VerifyingKey as SchnorrVerifyingKey,
        signature::Verifier as _,
    };
    use oxid_foundation::UnixTimestampMillis;
    use oxid_platform_ports::PlatformError;
    use oxid_wallet_application::WalletHdPathComponent;
    use oxid_wallet_domain::{WalletKeyLabel, WalletKeyPurpose};
    use p256::ecdsa::{Signature as P256Signature, VerifyingKey as P256VerifyingKey};

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
            let mut value = self
                .0
                .lock()
                .map_err(|_| PlatformError::RandomnessUnavailable)?;
            destination.fill(*value);
            *value = value.wrapping_add(1).max(1);
            Ok(())
        }
    }

    struct SeedOneRandom;

    impl RandomPort for SeedOneRandom {
        fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
            destination.fill(1);
            Ok(())
        }
    }

    fn adapter() -> DevelopmentWalletSecurity<FixedClock, IncrementingRandom> {
        DevelopmentWalletSecurity::new(Arc::new(FixedClock), Arc::new(IncrementingRandom::new()))
    }

    fn profile_id() -> WalletProfileId {
        WalletProfileId::parse("profile_test").expect("profile reference is valid")
    }

    #[test]
    #[cfg(feature = "development-fixture")]
    fn typed_root_seed_applies_only_to_the_explicit_profile() {
        let mut expected = [0_u8; 32];
        expected[31] = 1;
        let adapter = DevelopmentWalletSecurity::new(
            Arc::new(FixedClock),
            Arc::new(IncrementingRandom::new()),
        );
        let first = profile_id();
        let second = WalletProfileId::parse("profile_second").expect("second profile");

        adapter.initialize(&first).expect("initialize first");
        adapter
            .initialize_with_root_seed(&second, Zeroizing::new(expected))
            .expect("initialize explicit fixture profile");

        let profiles = adapter.profiles().expect("profile state");
        assert_eq!(
            profiles[first.as_str()]
                .root_seed
                .expose_for_protected_use(),
            &[17_u8; 32]
        );
        assert_eq!(
            profiles[second.as_str()]
                .root_seed
                .expose_for_protected_use(),
            &expected
        );
    }

    fn generate(
        adapter: &DevelopmentWalletSecurity<FixedClock, IncrementingRandom>,
        algorithm: WalletKeyAlgorithm,
        label: &str,
    ) -> WalletKeyDescriptor {
        adapter
            .generate(
                &profile_id(),
                GenerateProtectedKeyRequest {
                    label: WalletKeyLabel::parse(label).expect("label is valid"),
                    algorithm,
                    purpose: WalletKeyPurpose::Authentication,
                },
            )
            .expect("development key should be generated")
    }

    fn midnight_night_path(account: u32, index: u32) -> WalletHdPath {
        WalletHdPath::new(vec![
            WalletHdPathComponent::new(44, true).expect("purpose is valid"),
            WalletHdPathComponent::new(2400, true).expect("coin type is valid"),
            WalletHdPathComponent::new(account, true).expect("account is valid"),
            WalletHdPathComponent::new(0, false).expect("role is valid"),
            WalletHdPathComponent::new(index, false).expect("index is valid"),
        ])
        .expect("path is valid")
    }

    fn midnight_dust_path(account: u32) -> WalletHdPath {
        WalletHdPath::new(vec![
            WalletHdPathComponent::new(44, true).expect("purpose is valid"),
            WalletHdPathComponent::new(2400, true).expect("coin type is valid"),
            WalletHdPathComponent::new(account, true).expect("account is valid"),
            WalletHdPathComponent::new(2, false).expect("role is valid"),
            WalletHdPathComponent::new(0, false).expect("index is valid"),
        ])
        .expect("path is valid")
    }

    #[test]
    fn portable_backup_restores_exact_development_custody_without_overwrite() {
        let source = adapter();
        source.initialize(&profile_id()).expect("initialize source");
        let descriptor = generate(&source, WalletKeyAlgorithm::Ed25519, "Portable key");
        let secret =
            WalletRecoverySecret::parse("correct horse battery staple").expect("recovery secret");
        source.lock(&profile_id()).expect("lock source");
        assert_eq!(
            source.export_portable_backup(&profile_id(), &secret),
            Err(WalletPortableBackupPortError::Locked)
        );
        source.unlock(&profile_id()).expect("unlock source");
        let backup = source
            .export_portable_backup(&profile_id(), &secret)
            .expect("export source");

        let destination = adapter();
        let summary = destination
            .recover_portable_backup(&profile_id(), &backup, &secret)
            .expect("recover destination");
        assert_eq!(summary.restored_key_count, 1);
        assert_eq!(
            destination.list(&profile_id()).expect("restored list"),
            vec![descriptor.clone()]
        );
        destination
            .sign(&profile_id(), descriptor.reference(), b"restored")
            .expect("restored key should sign");
        assert_eq!(
            destination.recover_portable_backup(&profile_id(), &backup, &secret),
            Err(WalletPortableBackupPortError::AlreadyInitialized)
        );
    }

    #[test]
    fn persistent_development_custody_restores_multiple_profiles_independently() {
        let directory = tempfile::tempdir().expect("temporary custody directory");
        let custody_directory = directory.path().join("custody");
        let first_profile = profile_id();
        let second_profile =
            WalletProfileId::parse("profile_second").expect("second profile is valid");
        let path = midnight_night_path(0, 0);
        let (first_descriptor, second_descriptor, first_secret, second_secret) = {
            let adapter = DevelopmentWalletSecurity::persistent(
                Arc::new(FixedClock),
                Arc::new(IncrementingRandom::new()),
                &custody_directory,
            );
            adapter
                .initialize(&first_profile)
                .expect("initialize first profile");
            adapter
                .initialize(&second_profile)
                .expect("initialize second profile");
            let request = |label: &str| DeriveProtectedKeyRequest {
                label: WalletKeyLabel::parse(label).expect("label is valid"),
                algorithm: WalletKeyAlgorithm::Secp256k1Schnorr,
                purpose: WalletKeyPurpose::Transaction,
                path: path.clone(),
            };
            let first_descriptor = adapter
                .derive(&first_profile, request("First NIGHT account"))
                .expect("derive first account");
            let second_descriptor = adapter
                .derive(&second_profile, request("Second NIGHT account"))
                .expect("derive second account");
            let mut first_secret = None;
            adapter
                .use_derived_secret(&first_profile, &path, &mut |secret| {
                    first_secret = Some(*secret);
                    Ok(())
                })
                .expect("read first bounded derived secret");
            let mut second_secret = None;
            adapter
                .use_derived_secret(&second_profile, &path, &mut |secret| {
                    second_secret = Some(*secret);
                    Ok(())
                })
                .expect("read second bounded derived secret");
            (
                first_descriptor,
                second_descriptor,
                first_secret.expect("first secret was observed"),
                second_secret.expect("second secret was observed"),
            )
        };

        assert_ne!(first_secret, second_secret);
        let restored = DevelopmentWalletSecurity::persistent(
            Arc::new(FixedClock),
            Arc::new(IncrementingRandom::new()),
            &custody_directory,
        );
        assert_eq!(
            restored
                .status(&first_profile)
                .expect("first status")
                .state(),
            WalletProtectionState::Unlocked
        );
        assert_eq!(
            restored
                .status(&second_profile)
                .expect("second status")
                .state(),
            WalletProtectionState::Unlocked
        );
        assert_eq!(
            restored.list(&first_profile).expect("first keys"),
            vec![first_descriptor]
        );
        assert_eq!(
            restored.list(&second_profile).expect("second keys"),
            vec![second_descriptor]
        );
        let mut restored_first = None;
        restored
            .use_derived_secret(&first_profile, &path, &mut |secret| {
                restored_first = Some(*secret);
                Ok(())
            })
            .expect("restore first bounded secret");
        let mut restored_second = None;
        restored
            .use_derived_secret(&second_profile, &path, &mut |secret| {
                restored_second = Some(*secret);
                Ok(())
            })
            .expect("restore second bounded secret");
        assert_eq!(restored_first, Some(first_secret));
        assert_eq!(restored_second, Some(second_secret));

        drop(restored);
        let persistence = DevelopmentCustodyPersistence {
            directory: custody_directory.clone(),
        };
        std::fs::write(
            persistence.profile_path(&first_profile),
            b"corrupt profile vault",
        )
        .expect("corrupt only the first profile fixture");
        let isolated = DevelopmentWalletSecurity::persistent(
            Arc::new(FixedClock),
            Arc::new(IncrementingRandom::new()),
            &custody_directory,
        );
        assert_eq!(
            isolated.status(&first_profile),
            Err(WalletSecurityPortError::InvalidOperation)
        );
        assert_eq!(
            isolated
                .status(&second_profile)
                .expect("second profile remains independently readable")
                .state(),
            WalletProtectionState::Unlocked
        );
    }

    #[test]
    fn lifecycle_starts_uninitialized_and_blocks_keys_while_locked() {
        let adapter = adapter();

        assert_eq!(
            adapter
                .status(&profile_id())
                .expect("status is available")
                .state(),
            WalletProtectionState::Uninitialized
        );
        assert_eq!(
            adapter
                .list(&profile_id())
                .expect_err("uninitialized wallet must reject keys"),
            WalletSecurityPortError::NotInitialized
        );
        adapter
            .initialize(&profile_id())
            .expect("setup should unlock the development wallet");
        adapter.lock(&profile_id()).expect("lock should succeed");
        assert_eq!(
            adapter
                .list(&profile_id())
                .expect_err("locked wallet must reject keys"),
            WalletSecurityPortError::Locked
        );
        assert_eq!(
            adapter
                .unlock(&profile_id())
                .expect("unlock should succeed")
                .state(),
            WalletProtectionState::Unlocked
        );
    }

    #[test]
    fn ed25519_key_signatures_verify_from_public_metadata() {
        let adapter = adapter();
        adapter.initialize(&profile_id()).expect("setup succeeds");
        let descriptor = generate(&adapter, WalletKeyAlgorithm::Ed25519, "Login key");
        let payload = b"standalone conformance challenge";
        let signature = adapter
            .sign(&profile_id(), descriptor.reference(), payload)
            .expect("sign succeeds");
        let public_bytes: [u8; 32] = descriptor
            .public_key()
            .bytes()
            .try_into()
            .expect("Ed25519 public key is 32 bytes");
        let verifying_key = VerifyingKey::from_bytes(&public_bytes).expect("public key is valid");
        let signature =
            Ed25519Signature::from_slice(signature.bytes()).expect("signature is valid");

        verifying_key
            .verify(payload, &signature)
            .expect("signature must verify");
    }

    #[test]
    fn p256_key_signatures_verify_from_public_metadata() {
        let adapter = adapter();
        adapter.initialize(&profile_id()).expect("setup succeeds");
        let descriptor = generate(&adapter, WalletKeyAlgorithm::P256, "P-256 key");
        let payload = b"standalone P-256 challenge";
        let signature = adapter
            .sign(&profile_id(), descriptor.reference(), payload)
            .expect("sign succeeds");
        let verifying_key = P256VerifyingKey::from_sec1_bytes(descriptor.public_key().bytes())
            .expect("public key is valid");
        let signature = P256Signature::from_slice(signature.bytes()).expect("signature is valid");

        verifying_key
            .verify(payload, &signature)
            .expect("signature must verify");
    }

    #[test]
    fn protected_hd_derivation_is_idempotent_and_signs_without_exporting_secrets() {
        let adapter = adapter();
        adapter.initialize(&profile_id()).expect("setup succeeds");
        let request = DeriveProtectedKeyRequest {
            label: WalletKeyLabel::parse("Midnight NIGHT account 0/0").expect("label is valid"),
            algorithm: WalletKeyAlgorithm::Secp256k1Schnorr,
            purpose: WalletKeyPurpose::Transaction,
            path: midnight_night_path(0, 0),
        };
        let first = adapter
            .derive(&profile_id(), request.clone())
            .expect("derivation succeeds");
        let second = adapter
            .derive(&profile_id(), request)
            .expect("repeated derivation succeeds");

        assert_eq!(first, second);
        assert_eq!(
            first.public_key().encoding(),
            PublicKeyEncoding::Secp256k1XOnly
        );
        assert_eq!(first.public_key().bytes().len(), 32);

        let payload = b"Midnight transaction intent";
        let signature = adapter
            .sign(&profile_id(), first.reference(), payload)
            .expect("opaque child key signs");
        let public_bytes: [u8; 32] = first
            .public_key()
            .bytes()
            .try_into()
            .expect("x-only public key is 32 bytes");
        let verifying_key =
            SchnorrVerifyingKey::from_bytes(&public_bytes).expect("x-only public key is valid");
        let signature =
            SchnorrSignature::try_from(signature.bytes()).expect("signature bytes are valid");
        verifying_key
            .verify(payload, &signature)
            .expect("BIP340 signature must verify");

        adapter.lock(&profile_id()).expect("lock succeeds");
        assert_eq!(
            adapter
                .derive(
                    &profile_id(),
                    DeriveProtectedKeyRequest {
                        label: WalletKeyLabel::parse("Midnight NIGHT account 0/1")
                            .expect("label is valid"),
                        algorithm: WalletKeyAlgorithm::Secp256k1Schnorr,
                        purpose: WalletKeyPurpose::Transaction,
                        path: midnight_night_path(0, 1),
                    },
                )
                .expect_err("locked derivation must fail"),
            WalletSecurityPortError::Locked
        );
    }

    #[test]
    fn hd_derivation_matches_the_pinned_wallet_sdk_public_key_vector() {
        let adapter = DevelopmentWalletSecurity::new(Arc::new(FixedClock), Arc::new(SeedOneRandom));
        adapter.initialize(&profile_id()).expect("setup succeeds");
        let descriptor = adapter
            .derive(
                &profile_id(),
                DeriveProtectedKeyRequest {
                    label: WalletKeyLabel::parse("Midnight NIGHT account 0/0")
                        .expect("label is valid"),
                    algorithm: WalletKeyAlgorithm::Secp256k1Schnorr,
                    purpose: WalletKeyPurpose::Transaction,
                    path: midnight_night_path(0, 0),
                },
            )
            .expect("derivation succeeds");
        let public_hex = descriptor
            .public_key()
            .bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();

        // Pinned Wallet SDK HDWallet.ts + @scure/bip32 2.2.0 for public
        // conformance input [0x01; 32] at m/44'/2400'/0'/0/0.
        assert_eq!(
            public_hex,
            "b193e54524dc796402870a883fbdcd83869c9c307dda8c0d99c5f769169fc883"
        );
    }

    #[test]
    fn bounded_dust_child_use_is_deterministic_and_requires_unlock() {
        let adapter = DevelopmentWalletSecurity::new(Arc::new(FixedClock), Arc::new(SeedOneRandom));
        adapter.initialize(&profile_id()).expect("setup succeeds");
        let path = midnight_dust_path(7);
        let mut first = None;
        adapter
            .use_derived_secret(&profile_id(), &path, &mut |secret| {
                first = Some(*secret);
                Ok(())
            })
            .expect("bounded child operation succeeds");
        let mut second = None;
        adapter
            .use_derived_secret(&profile_id(), &path, &mut |secret| {
                second = Some(*secret);
                Ok(())
            })
            .expect("repeated bounded child operation succeeds");
        assert_eq!(first, second);
        assert!(first.is_some_and(|secret| secret != [0; 32]));

        let propagated = adapter
            .use_derived_secret(&profile_id(), &path, &mut |_| {
                Err(WalletSecurityPortError::AuthorizationDenied)
            })
            .expect_err("operation failure is preserved");
        assert_eq!(propagated, WalletSecurityPortError::AuthorizationDenied);

        adapter.lock(&profile_id()).expect("lock succeeds");
        let mut called = false;
        let locked = adapter
            .use_derived_secret(&profile_id(), &path, &mut |_| {
                called = true;
                Ok(())
            })
            .expect_err("locked wallet rejects secret use");
        assert_eq!(locked, WalletSecurityPortError::Locked);
        assert!(!called);
    }

    #[test]
    fn jubjub_key_signatures_verify_from_opaque_protected_keys() {
        let adapter = adapter();
        adapter.initialize(&profile_id()).expect("setup succeeds");
        let descriptor = generate(&adapter, WalletKeyAlgorithm::Jubjub, "Holder presentation");
        assert_eq!(descriptor.algorithm(), WalletKeyAlgorithm::Jubjub);
        assert_eq!(
            descriptor.public_key().encoding(),
            PublicKeyEncoding::JubjubCompressed
        );
        assert_eq!(descriptor.public_key().bytes().len(), 32);
        assert!(descriptor.reference().as_str().starts_with("key_"));

        let payload = b"bounded holder presentation statement";
        let signature = adapter
            .sign(&profile_id(), descriptor.reference(), payload)
            .expect("Jubjub signing succeeds");
        assert_eq!(signature.algorithm(), WalletKeyAlgorithm::Jubjub);
        assert_eq!(signature.bytes().len(), 96);
        jubjub_schnorr::verify(descriptor.public_key().bytes(), payload, signature.bytes())
            .expect("public verification succeeds");

        let mut tampered = payload.to_vec();
        tampered[0] ^= 1;
        assert!(
            jubjub_schnorr::verify(
                descriptor.public_key().bytes(),
                &tampered,
                signature.bytes(),
            )
            .is_err()
        );

        adapter.lock(&profile_id()).expect("lock succeeds");
        assert_eq!(
            adapter
                .sign(&profile_id(), descriptor.reference(), payload)
                .expect_err("locked custody must reject signing"),
            WalletSecurityPortError::Locked
        );
    }

    #[test]
    fn delete_removes_keys() {
        let adapter = adapter();
        adapter.initialize(&profile_id()).expect("setup succeeds");

        let descriptor = generate(&adapter, WalletKeyAlgorithm::Ed25519, "Delete me");
        adapter
            .delete(&profile_id(), descriptor.reference())
            .expect("delete succeeds");
        assert!(
            adapter
                .list(&profile_id())
                .expect("list succeeds")
                .is_empty()
        );
    }

    #[test]
    fn unavailable_adapter_fails_closed() {
        let adapter = UnavailableWalletSecurity;

        assert_eq!(
            adapter.status(&profile_id()).expect("status is safe"),
            WalletSecurityStatus::unavailable()
        );
        assert_eq!(
            adapter
                .unlock(&profile_id())
                .expect_err("unlock must fail closed"),
            WalletSecurityPortError::Unavailable
        );
    }
}
