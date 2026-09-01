// SPDX-License-Identifier: Apache-2.0

//! Profile-bound development custody for the public standalone genesis wallet.
//!
//! The undeployed chain assigns its public genesis funding authority to the
//! scalar-one root. This is not private wallet material: every byte is public
//! and anyone can spend assets assigned to it. The fixture is selected only
//! for the uniquely named standalone demo profile; ordinary profiles continue
//! to initialize from generic OS randomness.

#[cfg(feature = "standalone-development")]
use std::sync::Arc;

#[cfg(feature = "standalone-development")]
use oxid_adapter_storage_dev::{DevelopmentWalletFixtureProtection, DevelopmentWalletSecurity};
#[cfg(feature = "standalone-development")]
use oxid_wallet_application::WalletProfileRepository;

pub(super) const PUBLIC_STANDALONE_PROFILE_NAME: &str = "Oxid Demo Wallet";

#[cfg(feature = "standalone-development")]
#[derive(Clone, Copy)]
pub(super) struct PublicStandaloneNetwork(());

#[cfg(feature = "standalone-development")]
pub(super) fn public_standalone_network(network_id: &str) -> Option<PublicStandaloneNetwork> {
    (network_id == "undeployed").then_some(PublicStandaloneNetwork(()))
}

#[cfg(feature = "standalone-development")]
pub(super) const PUBLIC_STANDALONE_GENESIS_ROOT: [u8; 32] = {
    let mut root = [0_u8; 32];
    root[31] = 1;
    root
};

#[cfg(feature = "standalone-development")]
pub(super) fn public_profile_protection<R, C, N>(
    _network: PublicStandaloneNetwork,
    profiles: Arc<R>,
    security: Arc<DevelopmentWalletSecurity<C, N>>,
) -> DevelopmentWalletFixtureProtection<R, C, N>
where
    R: WalletProfileRepository + 'static,
    C: oxid_platform_ports::ClockPort + 'static,
    N: oxid_platform_ports::RandomPort + 'static,
{
    DevelopmentWalletFixtureProtection::new(
        profiles,
        security,
        PUBLIC_STANDALONE_PROFILE_NAME,
        PUBLIC_STANDALONE_GENESIS_ROOT,
    )
}

#[cfg(all(test, feature = "standalone-development"))]
mod tests {
    use oxid_adapter_platform_system::{OsRandom, SystemClock};
    use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;

    use super::*;

    #[test]
    fn public_fixture_protection_is_scoped_to_the_undeployed_network() {
        let profiles = Arc::new(InMemoryWalletProfileRepository::new());
        let security = Arc::new(DevelopmentWalletSecurity::new(
            Arc::new(SystemClock),
            Arc::new(OsRandom),
        ));

        let undeployed = public_standalone_network("undeployed").expect("undeployed capability");
        let _protection = public_profile_protection(undeployed, profiles, security);
        assert!(public_standalone_network("preprod").is_none());
    }
}
