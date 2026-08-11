// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    sync::{Arc, Mutex, MutexGuard},
};

use ed25519_dalek::{Signer as _, SigningKey as Ed25519SigningKey};
use oxid_platform_ports::{ClockPort, RandomPort};
use oxid_wallet_application::{
    GenerateProtectedKeyRequest, WalletKeyOperationPort, WalletProtectionPort,
    WalletSecurityPortError,
};
use oxid_wallet_domain::{
    PublicKeyEncoding, WalletKeyAlgorithm, WalletKeyDescriptor, WalletKeyReference,
    WalletProfileId, WalletProtectionClass, WalletProtectionState, WalletPublicKey,
    WalletSecurityStatus, WalletSignature,
};
use p256::ecdsa::{Signature as P256Signature, SigningKey as P256SigningKey};
use zeroize::Zeroizing;

const KEY_REFERENCE_ATTEMPTS: usize = 8;
const P256_SCALAR_ATTEMPTS: usize = 128;

/// Explicitly insecure, process-local adapter for tests and headless flows.
///
/// Secret key objects stay inside this adapter and are zeroized by their
/// cryptography implementations when removed or dropped. Nothing is persisted.
pub struct DevelopmentWalletSecurity<C, N> {
    clock: Arc<C>,
    random: Arc<N>,
    profiles: Mutex<BTreeMap<String, DevelopmentProfile>>,
}

impl<C, N> DevelopmentWalletSecurity<C, N> {
    #[must_use]
    pub fn new(clock: Arc<C>, random: Arc<N>) -> Self {
        Self {
            clock,
            random,
            profiles: Mutex::new(BTreeMap::new()),
        }
    }

    fn profiles(
        &self,
    ) -> Result<MutexGuard<'_, BTreeMap<String, DevelopmentProfile>>, WalletSecurityPortError> {
        self.profiles
            .lock()
            .map_err(|_| WalletSecurityPortError::Unavailable)
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
            WalletKeyAlgorithm::Jubjub => Err(WalletSecurityPortError::UnsupportedAlgorithm),
        }
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
        let mut profiles = self.profiles()?;
        if profiles.contains_key(profile_id.as_str()) {
            return Err(WalletSecurityPortError::AlreadyInitialized);
        }
        profiles.insert(
            profile_id.as_str().to_owned(),
            DevelopmentProfile {
                state: WalletProtectionState::Unlocked,
                keys: BTreeMap::new(),
            },
        );
        Ok(development_status(WalletProtectionState::Unlocked))
    }

    fn unlock(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletSecurityStatus, WalletSecurityPortError> {
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
        let mut profiles = self.profiles()?;
        let profile = profiles
            .get_mut(profile_id.as_str())
            .ok_or(WalletSecurityPortError::NotInitialized)?;
        profile.state = WalletProtectionState::Locked;
        Ok(development_status(profile.state))
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
            },
        );
        Ok(descriptor)
    }

    fn list(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<Vec<WalletKeyDescriptor>, WalletSecurityPortError> {
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
        }
    }

    fn delete(
        &self,
        profile_id: &WalletProfileId,
        key_reference: &WalletKeyReference,
    ) -> Result<(), WalletSecurityPortError> {
        let mut profiles = self.profiles()?;
        let profile = Self::unlocked_profile_mut(&mut profiles, profile_id)?;
        profile
            .keys
            .remove(key_reference.as_str())
            .map(|_| ())
            .ok_or(WalletSecurityPortError::NotFound)
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

struct DevelopmentProfile {
    state: WalletProtectionState,
    keys: BTreeMap<String, StoredDevelopmentKey>,
}

struct StoredDevelopmentKey {
    descriptor: WalletKeyDescriptor,
    material: DevelopmentKeyMaterial,
}

enum DevelopmentKeyMaterial {
    Ed25519(Ed25519SigningKey),
    P256(P256SigningKey),
}

const fn development_status(state: WalletProtectionState) -> WalletSecurityStatus {
    WalletSecurityStatus::new(state, WalletProtectionClass::DevelopmentOnly, false, false)
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature as Ed25519Signature, Verifier as _, VerifyingKey};
    use oxid_foundation::UnixTimestampMillis;
    use oxid_platform_ports::PlatformError;
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

    fn adapter() -> DevelopmentWalletSecurity<FixedClock, IncrementingRandom> {
        DevelopmentWalletSecurity::new(Arc::new(FixedClock), Arc::new(IncrementingRandom::new()))
    }

    fn profile_id() -> WalletProfileId {
        WalletProfileId::parse("profile_test").expect("profile reference is valid")
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
    fn unsupported_jubjub_is_reported_and_delete_removes_keys() {
        let adapter = adapter();
        adapter.initialize(&profile_id()).expect("setup succeeds");
        let error = adapter
            .generate(
                &profile_id(),
                GenerateProtectedKeyRequest {
                    label: WalletKeyLabel::parse("Jubjub key").expect("label is valid"),
                    algorithm: WalletKeyAlgorithm::Jubjub,
                    purpose: WalletKeyPurpose::Authentication,
                },
            )
            .expect_err("Jubjub must not be emulated");
        assert_eq!(error, WalletSecurityPortError::UnsupportedAlgorithm);

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
