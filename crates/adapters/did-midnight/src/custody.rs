// SPDX-License-Identifier: Apache-2.0

//! Private derivation roles reserved for native `did:midnight` operations.

use oxid_identity_application::DidLifecyclePortError;
use oxid_wallet_application::{WalletHdPath, WalletHdPathComponent};

const BIP44_PURPOSE: u32 = 44;
const MIDNIGHT_COIN_TYPE: u32 = 2_400;
const DID_CONTROLLER_ROLE: u32 = 3;
const DID_MAINTENANCE_ROLE: u32 = 4;
const DID_REPLAY_RANDOMNESS_ROLE: u32 = 5;
const DID_RECOVERY_ROLE: u32 = 6;
const DEFAULT_KEY_INDEX: u32 = 0;

pub(super) fn controller_path(
    account_index: u32,
    controller_index: u32,
) -> Result<WalletHdPath, DidLifecyclePortError> {
    hd_path(
        account_index,
        DID_CONTROLLER_ROLE,
        false,
        controller_index,
        false,
    )
}

pub(super) fn maintenance_path(account_index: u32) -> Result<WalletHdPath, DidLifecyclePortError> {
    hd_path(
        account_index,
        DID_MAINTENANCE_ROLE,
        true,
        DEFAULT_KEY_INDEX,
        true,
    )
}

pub(super) fn replay_randomness_path(
    account_index: u32,
) -> Result<WalletHdPath, DidLifecyclePortError> {
    hd_path(
        account_index,
        DID_REPLAY_RANDOMNESS_ROLE,
        true,
        DEFAULT_KEY_INDEX,
        true,
    )
}

pub(super) fn recovery_path(account_index: u32) -> Result<WalletHdPath, DidLifecyclePortError> {
    hd_path(
        account_index,
        DID_RECOVERY_ROLE,
        true,
        DEFAULT_KEY_INDEX,
        true,
    )
}

fn hd_path(
    account_index: u32,
    role: u32,
    role_hardened: bool,
    index: u32,
    index_hardened: bool,
) -> Result<WalletHdPath, DidLifecyclePortError> {
    let component = |value, hardened| {
        WalletHdPathComponent::new(value, hardened)
            .map_err(|_| DidLifecyclePortError::InvalidOperation)
    };
    WalletHdPath::new(vec![
        component(BIP44_PURPOSE, true)?,
        component(MIDNIGHT_COIN_TYPE, true)?,
        component(account_index, true)?,
        component(role, role_hardened)?,
        component(index, index_hardened)?,
    ])
    .map_err(|_| DidLifecyclePortError::InvalidOperation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_roles_are_hardened_and_distinct_from_night_external() {
        let maintenance = maintenance_path(2).expect("maintenance path");
        let replay = replay_randomness_path(2).expect("replay path");
        let recovery = recovery_path(2).expect("recovery path");
        let night_external = hd_path(2, 0, false, 0, false).expect("NIGHT external path");

        assert_ne!(maintenance, replay);
        assert_ne!(maintenance, recovery);
        assert_ne!(replay, recovery);
        assert_ne!(maintenance, night_external);
        assert_ne!(replay, night_external);
        assert_ne!(recovery, night_external);
        for path in [&maintenance, &replay, &recovery] {
            assert!(path.components()[3].hardened());
            assert!(path.components()[4].hardened());
        }
    }
}
