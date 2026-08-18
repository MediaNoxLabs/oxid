// SPDX-License-Identifier: Apache-2.0

//! Software key operations used only inside protected outgoing adapters.
//!
//! This crate is not a custody implementation: it stores nothing, authorizes
//! nobody, and exposes no application use case. Platform adapters may use it
//! only after an authenticated native wrapping boundary has released one
//! transient 32-byte secret. Callers remain responsible for zeroization.

#![forbid(unsafe_code)]

use bip32::{ChildNumber, XPrv};
use ed25519_dalek::{Signer as _, SigningKey as Ed25519SigningKey};
use k256::schnorr::{SigningKey as Secp256k1SchnorrSigningKey, signature::Signer as _};
use oxid_wallet_application::{
    WalletHdPath, WalletJubjubChallengeDeriver, WalletJubjubChallengeSignature,
    WalletSecurityPortError,
};
use oxid_wallet_domain::{PublicKeyEncoding, WalletKeyAlgorithm, WalletPublicKey, WalletSignature};
use p256::ecdsa::{Signature as P256Signature, SigningKey as P256SigningKey};
use zeroize::Zeroizing;

mod jubjub_schnorr;

/// Reconstructs the safe public key for one transient software secret.
pub fn public_key_from_secret(
    algorithm: WalletKeyAlgorithm,
    secret: &[u8; 32],
) -> Result<WalletPublicKey, WalletSecurityPortError> {
    match algorithm {
        WalletKeyAlgorithm::Ed25519 => {
            let signing_key = Ed25519SigningKey::from_bytes(secret);
            Ok(WalletPublicKey::new(
                PublicKeyEncoding::Ed25519Compressed,
                signing_key.verifying_key().to_bytes().to_vec(),
            ))
        }
        WalletKeyAlgorithm::P256 => {
            let signing_key = P256SigningKey::from_slice(secret)
                .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
            Ok(WalletPublicKey::new(
                PublicKeyEncoding::Sec1Compressed,
                signing_key
                    .verifying_key()
                    .to_sec1_point(true)
                    .as_bytes()
                    .to_vec(),
            ))
        }
        WalletKeyAlgorithm::Secp256k1Schnorr => {
            let signing_key = Secp256k1SchnorrSigningKey::from_bytes(secret)
                .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
            Ok(WalletPublicKey::new(
                PublicKeyEncoding::Secp256k1XOnly,
                signing_key.verifying_key().to_bytes().to_vec(),
            ))
        }
        WalletKeyAlgorithm::Jubjub => {
            let signing_key = jubjub_schnorr::SigningKey::from_seed(Zeroizing::new(*secret))
                .ok_or(WalletSecurityPortError::InvalidOperation)?;
            Ok(WalletPublicKey::new(
                PublicKeyEncoding::JubjubCompressed,
                signing_key.compressed_public_key()?,
            ))
        }
    }
}

/// Signs with a transient software secret and returns only the public result.
pub fn sign_with_secret(
    algorithm: WalletKeyAlgorithm,
    secret: &[u8; 32],
    payload: &[u8],
) -> Result<WalletSignature, WalletSecurityPortError> {
    match algorithm {
        WalletKeyAlgorithm::Ed25519 => {
            let key = Ed25519SigningKey::from_bytes(secret);
            Ok(WalletSignature::new(
                algorithm,
                key.sign(payload).to_bytes().to_vec(),
            ))
        }
        WalletKeyAlgorithm::P256 => {
            let key = P256SigningKey::from_slice(secret)
                .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
            let signature: P256Signature = key.sign(payload);
            Ok(WalletSignature::new(
                algorithm,
                signature.to_bytes().to_vec(),
            ))
        }
        WalletKeyAlgorithm::Secp256k1Schnorr => {
            let key = Secp256k1SchnorrSigningKey::from_bytes(secret)
                .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
            let signature: k256::schnorr::Signature = key.sign(payload);
            Ok(WalletSignature::new(
                algorithm,
                signature.to_bytes().to_vec(),
            ))
        }
        WalletKeyAlgorithm::Jubjub => {
            let key = jubjub_schnorr::SigningKey::from_seed(Zeroizing::new(*secret))
                .ok_or(WalletSecurityPortError::InvalidOperation)?;
            Ok(WalletSignature::new(algorithm, key.sign(payload)?))
        }
    }
}

/// Completes the exact Midnight Jubjub callback protocol inside the adapter.
pub fn sign_jubjub_challenge_with_secret(
    secret: &[u8; 32],
    nonce_seed: &[u8; 32],
    derive_challenge: &mut WalletJubjubChallengeDeriver<'_>,
) -> Result<WalletJubjubChallengeSignature, WalletSecurityPortError> {
    let key = jubjub_schnorr::SigningKey::from_seed(Zeroizing::new(*secret))
        .ok_or(WalletSecurityPortError::InvalidOperation)?;
    key.sign_challenge(nonce_seed, derive_challenge)
}

/// Derives one BIP32 child without exposing an extended private-key object.
pub fn derive_bip32_secret(
    root_seed: &[u8; 32],
    path: &WalletHdPath,
) -> Result<Zeroizing<[u8; 32]>, WalletSecurityPortError> {
    let mut extended =
        XPrv::new(root_seed.as_slice()).map_err(|_| WalletSecurityPortError::InvalidOperation)?;
    for component in path.components() {
        let child = ChildNumber::new(component.index(), component.hardened())
            .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
        extended = extended
            .derive_child(child)
            .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
    }
    Ok(Zeroizing::new(extended.to_bytes()))
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
    use oxid_wallet_domain::WalletKeyAlgorithm;

    use super::*;

    #[test]
    fn ed25519_public_and_signature_are_reconstructed_without_retention() {
        let secret = [0x19; 32];
        let public =
            public_key_from_secret(WalletKeyAlgorithm::Ed25519, &secret).expect("valid public key");
        let signed = sign_with_secret(WalletKeyAlgorithm::Ed25519, &secret, b"oxid")
            .expect("valid signature");
        let verifying =
            VerifyingKey::from_bytes(public.bytes().try_into().expect("ed25519 public width"))
                .expect("valid verifying key");
        let signature = Signature::from_slice(signed.bytes()).expect("valid signature bytes");

        verifying
            .verify(b"oxid", &signature)
            .expect("signature verifies");
    }

    #[test]
    fn invalid_curve_scalars_fail_closed() {
        assert!(public_key_from_secret(WalletKeyAlgorithm::P256, &[0; 32]).is_err());
        assert!(public_key_from_secret(WalletKeyAlgorithm::Secp256k1Schnorr, &[0; 32]).is_err());
    }

    // Official BIP-32 Test Vector 4 (the only spec vector with a 32-byte
    // seed). Expected keys are the last 32 bytes of the spec's Base58Check
    // xprv strings, decoded with checksum verification. The m/0' key starts
    // with a zero byte on purpose: the vector exists to catch
    // leading-zero-trimming bugs in derivation code.
    const BIP32_VECTOR4_SEED: [u8; 32] = [
        0x3d, 0xdd, 0x56, 0x02, 0x28, 0x58, 0x99, 0xa9, 0x46, 0x11, 0x45, 0x06, 0x15, 0x7c, 0x79,
        0x97, 0xe5, 0x44, 0x45, 0x28, 0xf3, 0x00, 0x3f, 0x61, 0x34, 0x71, 0x21, 0x47, 0xdb, 0x19,
        0xb6, 0x78,
    ];
    const BIP32_VECTOR4_M_0H: [u8; 32] = [
        0x00, 0xd9, 0x48, 0xe9, 0x26, 0x1e, 0x41, 0x36, 0x2a, 0x68, 0x8b, 0x91, 0x6f, 0x29, 0x71,
        0x21, 0xba, 0x6b, 0xfb, 0x22, 0x74, 0xa3, 0x57, 0x5a, 0xc0, 0xe4, 0x56, 0x55, 0x1d, 0xfd,
        0x7f, 0x7e,
    ];
    const BIP32_VECTOR4_M_0H_1H: [u8; 32] = [
        0x3a, 0x20, 0x86, 0xed, 0xd7, 0xd9, 0xdf, 0x86, 0xc3, 0x48, 0x7a, 0x59, 0x05, 0xa1, 0x71,
        0x2a, 0x9a, 0xa6, 0x64, 0xbc, 0xe8, 0xcc, 0x26, 0x81, 0x41, 0xe0, 0x75, 0x49, 0xea, 0xa8,
        0x66, 0x1d,
    ];

    fn hardened_path(indices: &[u32]) -> WalletHdPath {
        WalletHdPath::new(
            indices
                .iter()
                .map(|index| {
                    oxid_wallet_application::WalletHdPathComponent::new(*index, true)
                        .expect("bounded index")
                })
                .collect(),
        )
        .expect("non-empty path")
    }

    #[test]
    fn bip32_hardened_child_matches_official_vector_4() {
        let derived = derive_bip32_secret(&BIP32_VECTOR4_SEED, &hardened_path(&[0]))
            .expect("derivation succeeds");
        assert_eq!(
            *derived, BIP32_VECTOR4_M_0H,
            "m/0' must match the BIP-32 spec, including its leading zero byte"
        );
    }

    #[test]
    fn bip32_double_hardened_child_matches_official_vector_4() {
        let derived = derive_bip32_secret(&BIP32_VECTOR4_SEED, &hardened_path(&[0, 1]))
            .expect("derivation succeeds");
        assert_eq!(
            *derived, BIP32_VECTOR4_M_0H_1H,
            "m/0'/1' must match the BIP-32 spec"
        );
    }

    #[test]
    fn bip32_hardened_and_unhardened_children_differ() {
        let hardened = derive_bip32_secret(&BIP32_VECTOR4_SEED, &hardened_path(&[0]))
            .expect("hardened derivation succeeds");
        let unhardened = derive_bip32_secret(
            &BIP32_VECTOR4_SEED,
            &WalletHdPath::new(vec![
                oxid_wallet_application::WalletHdPathComponent::new(0, false)
                    .expect("bounded index"),
            ])
            .expect("non-empty path"),
        )
        .expect("unhardened derivation succeeds");
        assert_ne!(
            *hardened, *unhardened,
            "hardened and unhardened children of the same index must differ"
        );
    }
}
