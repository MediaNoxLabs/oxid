// SPDX-License-Identifier: Apache-2.0

//! Development-only randomness for the public standalone genesis wallet.
//!
//! The undeployed chain assigns its public genesis funding authority to the
//! scalar-one root. This is not private wallet material: every byte is public
//! and anyone can spend assets assigned to it. The adapter supplies that root
//! exactly once for development-custody initialization, then delegates every
//! key reference, nonce, and later profile root to operating-system randomness.

use std::sync::{Mutex, MutexGuard};

use oxid_adapter_platform_system::OsRandom;
use oxid_platform_ports::{PlatformError, RandomPort};
const PUBLIC_STANDALONE_GENESIS_ROOT: [u8; 32] = {
    let mut root = [0_u8; 32];
    root[31] = 1;
    root
};

pub(super) struct StandaloneDevelopmentRandom {
    root: Mutex<Option<[u8; 32]>>,
}

impl StandaloneDevelopmentRandom {
    pub(super) fn for_network(network_id: &str) -> Self {
        Self {
            root: Mutex::new(
                (network_id == "undeployed").then_some(PUBLIC_STANDALONE_GENESIS_ROOT),
            ),
        }
    }

    fn root(&self) -> Result<MutexGuard<'_, Option<[u8; 32]>>, PlatformError> {
        self.root
            .lock()
            .map_err(|_| PlatformError::RandomnessUnavailable)
    }
}

impl RandomPort for StandaloneDevelopmentRandom {
    fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
        let mut root = self.root()?;
        if let Some(seed) = root.as_ref() {
            if destination.len() != seed.len() {
                return Err(PlatformError::RandomnessUnavailable);
            }
            destination.copy_from_slice(seed.as_ref());
            root.take();
            return Ok(());
        }
        drop(root);
        OsRandom.fill_bytes(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_genesis_root_is_supplied_once_before_os_randomness() {
        let random = StandaloneDevelopmentRandom::for_network("undeployed");
        let mut root = [0_u8; 32];
        random.fill_bytes(&mut root).expect("public fixture root");
        assert_eq!(root, PUBLIC_STANDALONE_GENESIS_ROOT);

        let mut nonce = [0_u8; 32];
        random.fill_bytes(&mut nonce).expect("OS nonce randomness");
        assert_ne!(nonce, PUBLIC_STANDALONE_GENESIS_ROOT);
    }

    #[test]
    fn public_genesis_root_refuses_an_unexpected_first_request() {
        let random = StandaloneDevelopmentRandom::for_network("undeployed");
        let mut wrong_size = [0_u8; 16];
        assert_eq!(
            random.fill_bytes(&mut wrong_size),
            Err(PlatformError::RandomnessUnavailable)
        );

        let mut root = [0_u8; 32];
        random
            .fill_bytes(&mut root)
            .expect("a malformed request must not consume the public root");
        assert_eq!(root, PUBLIC_STANDALONE_GENESIS_ROOT);
    }

    #[test]
    fn other_networks_never_receive_the_public_standalone_root() {
        let random = StandaloneDevelopmentRandom::for_network("preprod");
        assert!(random.root().expect("root state").is_none());
    }
}
