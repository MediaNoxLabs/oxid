// SPDX-License-Identifier: Apache-2.0

//! Typed development custody for the public standalone genesis wallet.
//!
//! The undeployed chain assigns its public genesis funding authority to the
//! scalar-one root. This is not private wallet material: every byte is public
//! and anyone can spend assets assigned to it. The root enters the explicitly
//! insecure development adapter through its one-shot profile-root constructor;
//! generic randomness remains responsible for nonces, references, generated
//! keys, and every later profile.

use std::sync::Arc;

use oxid_adapter_storage_dev::DevelopmentWalletSecurity;

const PUBLIC_STANDALONE_GENESIS_ROOT: [u8; 32] = {
    let mut root = [0_u8; 32];
    root[31] = 1;
    root
};

fn public_root_for_network(network_id: &str) -> Option<[u8; 32]> {
    (network_id == "undeployed").then_some(PUBLIC_STANDALONE_GENESIS_ROOT)
}

pub(super) fn development_security_for_network<C, N>(
    network_id: &str,
    clock: Arc<C>,
    random: Arc<N>,
) -> DevelopmentWalletSecurity<C, N> {
    if let Some(root_seed) = public_root_for_network(network_id) {
        DevelopmentWalletSecurity::with_initial_root_seed(clock, random, root_seed)
    } else {
        DevelopmentWalletSecurity::new(clock, random)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_genesis_root_is_scoped_to_the_undeployed_network() {
        assert_eq!(
            public_root_for_network("undeployed"),
            Some(PUBLIC_STANDALONE_GENESIS_ROOT)
        );
        assert_eq!(public_root_for_network("preprod"), None);
    }
}
