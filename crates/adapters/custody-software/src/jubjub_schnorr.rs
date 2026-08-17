// SPDX-License-Identifier: Apache-2.0

//! Exact software boundary for the prototype's Jubjub Schnorr convention.

use midnight_serialize::Serializable as _;
use midnight_transient_crypto::{
    curve::{EmbeddedFr, EmbeddedGroupAffine, Fr},
    hash::transient_hash,
};
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use oxid_wallet_application::{
    JUBJUB_COMPACT_BYTES, WalletJubjubChallengeDeriver, WalletJubjubChallengeSignature,
    WalletSecurityPortError,
};

const NONCE_DOMAIN: &[u8] = b"midnight-did:jubjub-schnorr:v1";
const COMPRESSED_POINT_BYTES: usize = 32;
const SIGNATURE_BYTES: usize = 96;

pub(super) struct SigningKey {
    seed: Zeroizing<[u8; 32]>,
    public_key: EmbeddedGroupAffine,
}

impl SigningKey {
    pub(super) fn from_seed(seed: Zeroizing<[u8; 32]>) -> Option<Self> {
        let scalar = seed_to_scalar(&seed);
        if scalar == EmbeddedFr::from(0_u64) {
            return None;
        }
        let public_key = EmbeddedGroupAffine::generator() * scalar;
        if public_key.is_identity() {
            return None;
        }
        Some(Self { seed, public_key })
    }

    pub(super) fn compressed_public_key(&self) -> Result<Vec<u8>, WalletSecurityPortError> {
        let mut bytes = Vec::with_capacity(COMPRESSED_POINT_BYTES);
        self.public_key
            .serialize(&mut bytes)
            .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
        if bytes.len() != COMPRESSED_POINT_BYTES {
            return Err(WalletSecurityPortError::InvalidOperation);
        }
        Ok(bytes)
    }

    pub(super) fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, WalletSecurityPortError> {
        let secret = seed_to_scalar(&self.seed);
        let digest = payload_digest(payload);
        let nonce = deterministic_nonce(&self.seed, &digest);
        if nonce == EmbeddedFr::from(0_u64) {
            return Err(WalletSecurityPortError::InvalidOperation);
        }
        let announcement = EmbeddedGroupAffine::generator() * nonce;
        if announcement.is_identity() {
            return Err(WalletSecurityPortError::InvalidOperation);
        }
        let challenge = challenge(&announcement, &self.public_key, &digest)?;
        let response = nonce + challenge * secret;
        encode_signature(&announcement, &response)
    }

    pub(super) fn sign_challenge(
        &self,
        nonce_seed: &[u8; 32],
        derive_challenge: &mut WalletJubjubChallengeDeriver<'_>,
    ) -> Result<WalletJubjubChallengeSignature, WalletSecurityPortError> {
        let secret = seed_to_scalar(&self.seed);
        let nonce = challenge_nonce(&self.seed, nonce_seed);
        if nonce == EmbeddedFr::from(0_u64) {
            return Err(WalletSecurityPortError::InvalidOperation);
        }
        let announcement = EmbeddedGroupAffine::generator() * nonce;
        if announcement.is_identity() {
            return Err(WalletSecurityPortError::InvalidOperation);
        }
        let public_key = compressed_point(&self.public_key)?;
        let announcement_bytes = compressed_point(&announcement)?;
        let challenge_bytes = derive_challenge(&public_key, &announcement_bytes)?;
        let challenge = Fr::from_le_bytes(&challenge_bytes)
            .and_then(|field| EmbeddedFr::try_from(field).ok())
            .ok_or(WalletSecurityPortError::InvalidOperation)?;
        let response = nonce + challenge * secret;
        let response = Fr::from_le_bytes(&response.as_le_bytes())
            .ok_or(WalletSecurityPortError::InvalidOperation)?;
        Ok(WalletJubjubChallengeSignature {
            public_key,
            announcement: announcement_bytes,
            response: response
                .as_le_bytes()
                .try_into()
                .map_err(|_| WalletSecurityPortError::InvalidOperation)?,
        })
    }
}

fn challenge_nonce(secret_seed: &[u8; 32], random_seed: &[u8; 32]) -> EmbeddedFr {
    let mut preimage =
        Vec::with_capacity(NONCE_DOMAIN.len() + secret_seed.len() + random_seed.len() + 10);
    preimage.extend_from_slice(NONCE_DOMAIN);
    preimage.extend_from_slice(b":challenge");
    preimage.extend_from_slice(secret_seed);
    preimage.extend_from_slice(random_seed);
    hash_to_scalar(&Sha256::digest(preimage))
}

fn compressed_point(
    point: &EmbeddedGroupAffine,
) -> Result<[u8; JUBJUB_COMPACT_BYTES], WalletSecurityPortError> {
    let mut bytes = Vec::with_capacity(JUBJUB_COMPACT_BYTES);
    point
        .serialize(&mut bytes)
        .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
    bytes
        .try_into()
        .map_err(|_| WalletSecurityPortError::InvalidOperation)
}

fn seed_to_scalar(seed: &[u8; 32]) -> EmbeddedFr {
    hash_to_scalar(&Sha256::digest(seed))
}

fn hash_to_scalar(hash: &[u8]) -> EmbeddedFr {
    let mut little_endian = [0_u8; 32];
    for (index, byte) in hash.iter().take(32).rev().enumerate() {
        little_endian[index] = *byte;
    }
    EmbeddedFr::from_le_bytes_wide(&little_endian)
        .expect("a 32-byte value always reduces to a Jubjub scalar")
}

fn payload_digest(payload: &[u8]) -> [Fr; 4] {
    let hash = Sha256::digest(payload);
    std::array::from_fn(|index| {
        let start = index * 8;
        Fr::from(u64::from_be_bytes(
            hash[start..start + 8]
                .try_into()
                .expect("SHA-256 has four complete u64 limbs"),
        ))
    })
}

fn deterministic_nonce(seed: &[u8; 32], digest: &[Fr; 4]) -> EmbeddedFr {
    let mut preimage = Vec::with_capacity(NONCE_DOMAIN.len() + 32 + 4 * 32);
    preimage.extend_from_slice(NONCE_DOMAIN);
    preimage.extend_from_slice(seed);
    for field in digest {
        preimage.extend_from_slice(&field_be(field));
    }
    hash_to_scalar(&Sha256::digest(preimage))
}

fn challenge(
    announcement: &EmbeddedGroupAffine,
    public_key: &EmbeddedGroupAffine,
    digest: &[Fr; 4],
) -> Result<EmbeddedFr, WalletSecurityPortError> {
    let fields = [
        announcement
            .x()
            .ok_or(WalletSecurityPortError::InvalidOperation)?,
        announcement
            .y()
            .ok_or(WalletSecurityPortError::InvalidOperation)?,
        public_key
            .x()
            .ok_or(WalletSecurityPortError::InvalidOperation)?,
        public_key
            .y()
            .ok_or(WalletSecurityPortError::InvalidOperation)?,
        digest[0],
        digest[1],
        digest[2],
        digest[3],
    ];
    let bytes = transient_hash(&fields).as_le_bytes();
    let mut reduced = [0_u8; 32];
    reduced[..31].copy_from_slice(&bytes[..31]);
    EmbeddedFr::from_le_bytes(&reduced).ok_or(WalletSecurityPortError::InvalidOperation)
}

fn field_be(field: &Fr) -> [u8; 32] {
    let little_endian = field.as_le_bytes();
    std::array::from_fn(|index| little_endian[31 - index])
}

fn embedded_field_be(field: &EmbeddedFr) -> [u8; 32] {
    let little_endian = field.as_le_bytes();
    std::array::from_fn(|index| little_endian[31 - index])
}

fn encode_signature(
    announcement: &EmbeddedGroupAffine,
    response: &EmbeddedFr,
) -> Result<Vec<u8>, WalletSecurityPortError> {
    let x = announcement
        .x()
        .ok_or(WalletSecurityPortError::InvalidOperation)?;
    let y = announcement
        .y()
        .ok_or(WalletSecurityPortError::InvalidOperation)?;
    let mut bytes = Vec::with_capacity(SIGNATURE_BYTES);
    bytes.extend_from_slice(&field_be(&x));
    bytes.extend_from_slice(&field_be(&y));
    bytes.extend_from_slice(&embedded_field_be(response));
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_seed_vector_is_stable() {
        let key = SigningKey::from_seed(Zeroizing::new([0x23; 32])).expect("non-zero key");
        let signature = key.sign(b"Oxid holder statement").expect("signature");

        assert_eq!(signature.len(), SIGNATURE_BYTES);
        assert_eq!(
            hex::encode(signature),
            concat!(
                "583fe322acfa2db7c9328093c9c2fa83901fa81d81e6bab10af556ca91fc94bd",
                "519e689fcd0d1a7c988b864562a99be1774d88aa8bb69e79ecd1013ac9df0845",
                "08077115a06c82e6008f2f5496ce6d19e94c76d5909c9c1fa1da0d9f0e16dedb"
            )
        );
    }
}
