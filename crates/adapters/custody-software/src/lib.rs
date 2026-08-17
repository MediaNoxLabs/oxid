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
}
