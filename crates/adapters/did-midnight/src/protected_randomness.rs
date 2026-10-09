// SPDX-License-Identifier: Apache-2.0

//! Secret-bound deterministic randomness for crash-replayable DID operations.

use hmac::{Hmac, Mac};
use midnight_base_crypto::signatures::SigningKey;
use rand::{CryptoRng, RngCore};
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

use oxid_wallet_application::WalletSecurityPortError;

const DERIVATION_DOMAIN: &[u8] = b"oxid:did:protected-randomness:v1";
const BLOCK_DOMAIN: &[u8] = b"oxid:did:protected-rng-block:v1";
const MAINTENANCE_KEY_DOMAIN: &[u8] = b"oxid:did:maintenance-key:v1";

type HmacSha256 = Hmac<Sha256>;

pub(super) fn maintenance_signing_key(
    protected_secret: &[u8; 32],
) -> Result<SigningKey, WalletSecurityPortError> {
    let seed = derive_seed(protected_secret, MAINTENANCE_KEY_DOMAIN, &[0; 32])?;
    Ok(SigningKey::sample(ProtectedDeterministicRng::new(seed)))
}

pub(super) fn derive_seed(
    protected_secret: &[u8; 32],
    purpose: &[u8],
    public_recipe: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, WalletSecurityPortError> {
    let mut mac = HmacSha256::new_from_slice(protected_secret)
        .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
    mac.update(DERIVATION_DOMAIN);
    mac.update(&(purpose.len() as u64).to_be_bytes());
    mac.update(purpose);
    mac.update(public_recipe);
    let mut output = mac.finalize().into_bytes();
    let mut seed = Zeroizing::new([0_u8; 32]);
    seed.copy_from_slice(&output);
    output.as_mut_slice().zeroize();
    Ok(seed)
}

/// HMAC-based deterministic RNG whose retained key and output block are wiped
/// on drop. This avoids retaining a secret-derived generic RNG state after a
/// signature or intent has been composed.
pub(super) struct ProtectedDeterministicRng {
    key: Zeroizing<[u8; 32]>,
    counter: u64,
    block: Zeroizing<[u8; 32]>,
    offset: usize,
}

impl ProtectedDeterministicRng {
    pub(super) fn new(key: Zeroizing<[u8; 32]>) -> Self {
        Self {
            key,
            counter: 0,
            block: Zeroizing::new([0; 32]),
            offset: 32,
        }
    }

    fn refill(&mut self) {
        let mut mac = HmacSha256::new_from_slice(self.key.as_ref())
            .expect("HMAC accepts a fixed 32-byte key");
        mac.update(BLOCK_DOMAIN);
        mac.update(&self.counter.to_be_bytes());
        let mut output = mac.finalize().into_bytes();
        self.block.copy_from_slice(&output);
        output.as_mut_slice().zeroize();
        self.counter = self.counter.wrapping_add(1);
        self.offset = 0;
    }
}

impl RngCore for ProtectedDeterministicRng {
    fn next_u32(&mut self) -> u32 {
        let mut bytes = [0_u8; 4];
        self.fill_bytes(&mut bytes);
        let value = u32::from_le_bytes(bytes);
        bytes.zeroize();
        value
    }

    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0_u8; 8];
        self.fill_bytes(&mut bytes);
        let value = u64::from_le_bytes(bytes);
        bytes.zeroize();
        value
    }

    fn fill_bytes(&mut self, destination: &mut [u8]) {
        let mut written = 0;
        while written < destination.len() {
            if self.offset == self.block.len() {
                self.refill();
            }
            let available = self.block.len() - self.offset;
            let count = available.min(destination.len() - written);
            destination[written..written + count]
                .copy_from_slice(&self.block[self.offset..self.offset + count]);
            self.offset += count;
            written += count;
        }
    }

    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(destination);
        Ok(())
    }
}

impl CryptoRng for ProtectedDeterministicRng {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_inputs_replay_identically_but_public_recipes_do_not_reveal_the_seed() {
        let first = derive_seed(&[7; 32], b"intent", &[9; 32]).expect("first");
        let replay = derive_seed(&[7; 32], b"intent", &[9; 32]).expect("replay");
        let other_secret = derive_seed(&[8; 32], b"intent", &[9; 32]).expect("other secret");
        let other_recipe = derive_seed(&[7; 32], b"intent", &[10; 32]).expect("other recipe");

        assert_eq!(*first, *replay);
        assert_ne!(*first, *other_secret);
        assert_ne!(*first, *other_recipe);
        assert_ne!(*first, [9; 32]);
    }

    #[test]
    fn deterministic_rng_replays_the_same_stream() {
        let seed = derive_seed(&[7; 32], b"intent", &[9; 32]).expect("seed");
        let mut first = ProtectedDeterministicRng::new(seed);
        let mut replay = ProtectedDeterministicRng::new(
            derive_seed(&[7; 32], b"intent", &[9; 32]).expect("replay seed"),
        );
        let mut first_bytes = Zeroizing::new([0_u8; 96]);
        let mut replay_bytes = Zeroizing::new([0_u8; 96]);
        first.fill_bytes(&mut first_bytes[..]);
        replay.fill_bytes(&mut replay_bytes[..]);
        assert_eq!(*first_bytes, *replay_bytes);
    }
}
