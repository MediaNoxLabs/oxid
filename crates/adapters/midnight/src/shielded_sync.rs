// SPDX-License-Identifier: Apache-2.0

//! Off-renderer, profile-scoped shielded synchronization controllers.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use oxid_platform_ports::ClockPort;
use oxid_wallet_application::{
    WalletDerivedSecretUsePort, WalletHdPath, WalletHdPathComponent, WalletSecurityPortError,
    WalletShieldedSyncPortError,
};
use oxid_wallet_domain::{
    ChainNetworkId, WalletProfileId, WalletShieldedSyncSnapshot, WalletShieldedSyncState,
    WalletShieldedTokenBalance,
};

use crate::{BIP44_PURPOSE, MIDNIGHT_COIN_TYPE, ZSWAP_INDEX, ZSWAP_ROLE};

const SIMULATED_TARGET_CURSOR: u64 = 2;
const SIMULATED_TOKEN_TYPE: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
const SIMULATED_BALANCE_ATOMIC_UNITS: u128 = 5_000_000;

pub(crate) trait MidnightShieldedSyncController: Send + Sync {
    fn status(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError>;

    fn start(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
        account_index: u32,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError>;

    fn cancel(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UnavailableMidnightShieldedSyncController;

impl MidnightShieldedSyncController for UnavailableMidnightShieldedSyncController {
    fn status(
        &self,
        _: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        Ok(WalletShieldedSyncSnapshot::unavailable(network_id.clone()))
    }

    fn start(
        &self,
        _: &WalletProfileId,
        _: &ChainNetworkId,
        _: u32,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        Err(WalletShieldedSyncPortError::Unavailable)
    }

    fn cancel(
        &self,
        _: &WalletProfileId,
        _: &ChainNetworkId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        Err(WalletShieldedSyncPortError::Unavailable)
    }
}

/// Deterministic poll-driven controller used by the headless conformance stack.
pub(crate) struct SimulatedMidnightShieldedSyncController<C, K> {
    clock: Arc<C>,
    keys: Arc<K>,
    sessions: Mutex<HashMap<(WalletProfileId, ChainNetworkId), WalletShieldedSyncSnapshot>>,
}

impl<C, K> SimulatedMidnightShieldedSyncController<C, K> {
    pub(crate) fn new(clock: Arc<C>, keys: Arc<K>) -> Self {
        Self {
            clock,
            keys,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn key(
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> (WalletProfileId, ChainNetworkId) {
        (profile_id.clone(), network_id.clone())
    }
}

impl<C, K> MidnightShieldedSyncController for SimulatedMidnightShieldedSyncController<C, K>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + 'static,
{
    fn status(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        let key = Self::key(profile_id, network_id);
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| WalletShieldedSyncPortError::Unavailable)?;
        let Some(current) = sessions.get(&key).cloned() else {
            return Ok(WalletShieldedSyncSnapshot::never_synced(network_id.clone()));
        };
        if current.state() != WalletShieldedSyncState::Syncing {
            return Ok(current);
        }

        let (cursor, processed, state) = match current.current_cursor() {
            None => (0, 1, WalletShieldedSyncState::Syncing),
            Some(0) => (1, 2, WalletShieldedSyncState::Syncing),
            Some(1) => (2, 3, WalletShieldedSyncState::Synced),
            Some(_) => (2, 0, WalletShieldedSyncState::Synced),
        };
        let balance = SIMULATED_BALANCE_ATOMIC_UNITS
            .checked_mul(u128::from(cursor + 1))
            .and_then(|value| value.checked_div(u128::from(SIMULATED_TARGET_CURSOR + 1)))
            .ok_or(WalletShieldedSyncPortError::InvalidData)?;
        let next = snapshot(
            network_id.clone(),
            state,
            Some(cursor),
            Some(SIMULATED_TARGET_CURSOR),
            processed,
            Some(u64::from(cursor == SIMULATED_TARGET_CURSOR)),
            Some(cursor + 1),
            vec![token_balance(balance)?],
            Some(now(self.clock.as_ref())?),
        )?;
        sessions.insert(key, next.clone());
        Ok(next)
    }

    fn start(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
        account_index: u32,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        let path = shielded_path(account_index)?;
        self.keys
            .use_derived_secret(profile_id, &path, &mut |_| Ok(()))
            .map_err(map_security_error)?;
        let key = Self::key(profile_id, network_id);
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| WalletShieldedSyncPortError::Unavailable)?;
        if sessions
            .get(&key)
            .is_some_and(|status| status.state() == WalletShieldedSyncState::Syncing)
        {
            return Err(WalletShieldedSyncPortError::Conflict);
        }
        let previous = sessions
            .get(&key)
            .cloned()
            .unwrap_or_else(|| WalletShieldedSyncSnapshot::never_synced(network_id.clone()));
        let started = snapshot(
            network_id.clone(),
            WalletShieldedSyncState::Syncing,
            previous.current_cursor(),
            previous.target_cursor(),
            0,
            previous.owned_note_count(),
            previous.commitment_count(),
            previous.balances().to_vec(),
            previous.updated_at(),
        )?;
        sessions.insert(key, started.clone());
        Ok(started)
    }

    fn cancel(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        let key = Self::key(profile_id, network_id);
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| WalletShieldedSyncPortError::Unavailable)?;
        let current = sessions
            .get(&key)
            .cloned()
            .ok_or(WalletShieldedSyncPortError::Conflict)?;
        if current.state() != WalletShieldedSyncState::Syncing {
            return Err(WalletShieldedSyncPortError::Conflict);
        }
        let cancelled = snapshot(
            network_id.clone(),
            WalletShieldedSyncState::Cancelled,
            current.current_cursor(),
            current.target_cursor(),
            current.events_processed(),
            current.owned_note_count(),
            current.commitment_count(),
            current.balances().to_vec(),
            current.updated_at(),
        )?;
        sessions.insert(key, cancelled.clone());
        Ok(cancelled)
    }
}

#[allow(clippy::too_many_arguments)]
fn snapshot(
    network_id: ChainNetworkId,
    state: WalletShieldedSyncState,
    current_cursor: Option<u64>,
    target_cursor: Option<u64>,
    events_processed: u64,
    owned_note_count: Option<u64>,
    commitment_count: Option<u64>,
    balances: Vec<WalletShieldedTokenBalance>,
    updated_at: Option<oxid_foundation::UnixTimestampMillis>,
) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
    WalletShieldedSyncSnapshot::new(
        network_id,
        state,
        current_cursor,
        target_cursor,
        events_processed,
        owned_note_count,
        commitment_count,
        balances,
        updated_at,
        None,
    )
    .map_err(|_| WalletShieldedSyncPortError::InvalidData)
}

fn token_balance(
    atomic_units: u128,
) -> Result<WalletShieldedTokenBalance, WalletShieldedSyncPortError> {
    WalletShieldedTokenBalance::new(SIMULATED_TOKEN_TYPE, atomic_units)
        .map_err(|_| WalletShieldedSyncPortError::InvalidData)
}

fn now<C: ClockPort>(
    clock: &C,
) -> Result<oxid_foundation::UnixTimestampMillis, WalletShieldedSyncPortError> {
    clock
        .now()
        .map_err(|_| WalletShieldedSyncPortError::Unavailable)
}

fn shielded_path(account_index: u32) -> Result<WalletHdPath, WalletShieldedSyncPortError> {
    let component = |index, hardened| {
        WalletHdPathComponent::new(index, hardened)
            .map_err(|_| WalletShieldedSyncPortError::InvalidData)
    };
    WalletHdPath::new(vec![
        component(BIP44_PURPOSE, true)?,
        component(MIDNIGHT_COIN_TYPE, true)?,
        component(account_index, true)?,
        component(ZSWAP_ROLE, false)?,
        component(ZSWAP_INDEX, false)?,
    ])
    .map_err(|_| WalletShieldedSyncPortError::InvalidData)
}

const fn map_security_error(error: WalletSecurityPortError) -> WalletShieldedSyncPortError {
    match error {
        WalletSecurityPortError::NotInitialized => {
            WalletShieldedSyncPortError::ProtectionNotInitialized
        }
        WalletSecurityPortError::Locked => WalletShieldedSyncPortError::ProtectionLocked,
        WalletSecurityPortError::Unavailable => WalletShieldedSyncPortError::Unavailable,
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::NotFound
        | WalletSecurityPortError::Conflict
        | WalletSecurityPortError::UnsupportedAlgorithm
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => WalletShieldedSyncPortError::InvalidData,
    }
}

#[cfg(test)]
mod tests {
    use oxid_foundation::UnixTimestampMillis;
    use oxid_platform_ports::PlatformError;

    use super::*;

    struct FixedClock;

    impl ClockPort for FixedClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Ok(UnixTimestampMillis::new(1_700_000_000_000))
        }
    }

    struct AvailableKeys(u32);

    impl WalletDerivedSecretUsePort for AvailableKeys {
        fn use_derived_secret(
            &self,
            _: &WalletProfileId,
            path: &WalletHdPath,
            operation: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
        ) -> Result<(), WalletSecurityPortError> {
            assert_eq!(
                path.components()
                    .iter()
                    .map(|component| (component.index(), component.hardened()))
                    .collect::<Vec<_>>(),
                vec![
                    (44, true),
                    (2400, true),
                    (self.0, true),
                    (3, false),
                    (0, false)
                ]
            );
            operation(&[7; 32])
        }
    }

    fn profile() -> WalletProfileId {
        WalletProfileId::parse("profile_test").expect("profile is valid")
    }

    fn network() -> ChainNetworkId {
        ChainNetworkId::parse("undeployed").expect("network is valid")
    }

    #[test]
    fn simulated_sync_is_exact_resumable_and_uses_the_shielded_child() {
        let sync = SimulatedMidnightShieldedSyncController::new(
            Arc::new(FixedClock),
            Arc::new(AvailableKeys(7)),
        );
        assert_eq!(
            sync.status(&profile(), &network())
                .expect("initial status")
                .state(),
            WalletShieldedSyncState::NeverSynced
        );
        sync.start(&profile(), &network(), 7).expect("sync starts");
        let first = sync.status(&profile(), &network()).expect("first progress");
        assert_eq!(first.current_cursor(), Some(0));
        assert_eq!(first.commitment_count(), Some(1));
        assert_eq!(
            sync.cancel(&profile(), &network())
                .expect("sync cancels")
                .state(),
            WalletShieldedSyncState::Cancelled
        );
        sync.start(&profile(), &network(), 7).expect("sync resumes");
        assert_eq!(
            sync.status(&profile(), &network())
                .expect("second progress")
                .current_cursor(),
            Some(1)
        );
        let complete = sync.status(&profile(), &network()).expect("sync completes");
        assert_eq!(complete.state(), WalletShieldedSyncState::Synced);
        assert_eq!(complete.owned_note_count(), Some(1));
        assert_eq!(
            complete.balances()[0].atomic_units(),
            SIMULATED_BALANCE_ATOMIC_UNITS
        );
    }
}
