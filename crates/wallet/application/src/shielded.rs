// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, sync::Arc};

use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::{
    WalletProfileId, WalletShieldedSyncFailure, WalletShieldedSyncSnapshot, WalletShieldedSyncState,
};

/// Stable adapter failures for starting, reading, or cancelling shielded sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletShieldedSyncPortError {
    Conflict,
    UnsupportedNetwork,
    ProtectionNotInitialized,
    ProtectionLocked,
    Unavailable,
    InvalidData,
}

impl fmt::Display for WalletShieldedSyncPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Conflict => "shielded synchronization is already running",
            Self::UnsupportedNetwork => "wallet network is not supported",
            Self::ProtectionNotInitialized => "wallet protection is not initialized",
            Self::ProtectionLocked => "wallet is locked",
            Self::Unavailable => "shielded synchronization is unavailable",
            Self::InvalidData => "shielded synchronization returned invalid data",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletShieldedSyncPortError {}

/// Focused outgoing port for adapter-owned, off-renderer shielded sync.
pub trait WalletShieldedSyncPort: Send + Sync {
    /// Return an already-published bounded snapshot. Implementations must not
    /// perform custody, filesystem, transport, or ledger work in this method;
    /// the adapter-owned sync worker publishes that work's state separately.
    fn shielded_status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError>;

    fn start_shielded_sync(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError>;

    fn cancel_shielded_sync(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError>;
}

/// Profile-scoped query or command for shielded synchronization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletShieldedSyncCommand {
    pub profile_id: String,
}

/// Exact public balance for one shielded token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletShieldedTokenBalanceView {
    pub token_type_hex: String,
    pub atomic_units: String,
}

/// Safe shielded sync projection returned to UI, headless, and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletShieldedSyncView {
    pub network_id: String,
    pub state: String,
    pub current_cursor: Option<u64>,
    pub target_cursor: Option<u64>,
    pub events_processed: u64,
    pub owned_note_count: Option<u64>,
    pub commitment_count: Option<u64>,
    pub balances: Vec<WalletShieldedTokenBalanceView>,
    pub updated_at_millis: Option<u64>,
    pub failure: Option<String>,
}

impl From<&WalletShieldedSyncSnapshot> for WalletShieldedSyncView {
    fn from(snapshot: &WalletShieldedSyncSnapshot) -> Self {
        Self {
            network_id: snapshot.network_id().as_str().to_owned(),
            state: state_name(snapshot.state()).to_owned(),
            current_cursor: snapshot.current_cursor(),
            target_cursor: snapshot.target_cursor(),
            events_processed: snapshot.events_processed(),
            owned_note_count: snapshot.owned_note_count(),
            commitment_count: snapshot.commitment_count(),
            balances: snapshot
                .balances()
                .iter()
                .map(|balance| WalletShieldedTokenBalanceView {
                    token_type_hex: balance.token_type_hex().to_owned(),
                    atomic_units: balance.atomic_units().to_string(),
                })
                .collect(),
            updated_at_millis: snapshot.updated_at().map(|value| value.value()),
            failure: snapshot.failure().map(failure_name).map(str::to_owned),
        }
    }
}

/// Structured validation and adapter failures for shielded sync use cases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletShieldedSyncError {
    InvalidProfileIdentifier(OpaqueIdError),
    Port(WalletShieldedSyncPortError),
}

impl fmt::Display for WalletShieldedSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
            Self::Port(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletShieldedSyncError {}

pub trait GetWalletShieldedSyncStatusUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletShieldedSyncCommand,
    ) -> Result<WalletShieldedSyncView, WalletShieldedSyncError>;
}

pub trait StartWalletShieldedSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletShieldedSyncCommand,
    ) -> Result<WalletShieldedSyncView, WalletShieldedSyncError>;
}

pub trait CancelWalletShieldedSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletShieldedSyncCommand,
    ) -> Result<WalletShieldedSyncView, WalletShieldedSyncError>;
}

/// Application service preserving a narrow shielded synchronization boundary.
pub struct WalletShieldedSyncService<S> {
    shielded: Arc<S>,
}

impl<S> WalletShieldedSyncService<S> {
    #[must_use]
    pub const fn new(shielded: Arc<S>) -> Self {
        Self { shielded }
    }

    fn profile(
        command: WalletShieldedSyncCommand,
    ) -> Result<WalletProfileId, WalletShieldedSyncError> {
        WalletProfileId::parse(command.profile_id)
            .map_err(WalletShieldedSyncError::InvalidProfileIdentifier)
    }
}

impl<S> GetWalletShieldedSyncStatusUseCase for WalletShieldedSyncService<S>
where
    S: WalletShieldedSyncPort + 'static,
{
    fn execute(
        &self,
        command: WalletShieldedSyncCommand,
    ) -> Result<WalletShieldedSyncView, WalletShieldedSyncError> {
        let profile = Self::profile(command)?;
        self.shielded
            .shielded_status(&profile)
            .map(|snapshot| WalletShieldedSyncView::from(&snapshot))
            .map_err(WalletShieldedSyncError::Port)
    }
}

impl<S> StartWalletShieldedSyncUseCase for WalletShieldedSyncService<S>
where
    S: WalletShieldedSyncPort + 'static,
{
    fn execute(
        &self,
        command: WalletShieldedSyncCommand,
    ) -> Result<WalletShieldedSyncView, WalletShieldedSyncError> {
        let profile = Self::profile(command)?;
        self.shielded
            .start_shielded_sync(&profile)
            .map(|snapshot| WalletShieldedSyncView::from(&snapshot))
            .map_err(WalletShieldedSyncError::Port)
    }
}

impl<S> CancelWalletShieldedSyncUseCase for WalletShieldedSyncService<S>
where
    S: WalletShieldedSyncPort + 'static,
{
    fn execute(
        &self,
        command: WalletShieldedSyncCommand,
    ) -> Result<WalletShieldedSyncView, WalletShieldedSyncError> {
        let profile = Self::profile(command)?;
        self.shielded
            .cancel_shielded_sync(&profile)
            .map(|snapshot| WalletShieldedSyncView::from(&snapshot))
            .map_err(WalletShieldedSyncError::Port)
    }
}

const fn state_name(state: WalletShieldedSyncState) -> &'static str {
    match state {
        WalletShieldedSyncState::NeverSynced => "never_synced",
        WalletShieldedSyncState::Syncing => "syncing",
        WalletShieldedSyncState::Synced => "synced",
        WalletShieldedSyncState::Cached => "cached",
        WalletShieldedSyncState::Cancelled => "cancelled",
        WalletShieldedSyncState::Stalled => "stalled",
        WalletShieldedSyncState::Unavailable => "unavailable",
    }
}

const fn failure_name(failure: WalletShieldedSyncFailure) -> &'static str {
    match failure {
        WalletShieldedSyncFailure::ProtectionNotInitialized => "protection_not_initialized",
        WalletShieldedSyncFailure::ProtectionLocked => "protection_locked",
        WalletShieldedSyncFailure::UnsupportedNetwork => "unsupported_network",
        WalletShieldedSyncFailure::TransportUnavailable => "transport_unavailable",
        WalletShieldedSyncFailure::TimedOut => "timed_out",
        WalletShieldedSyncFailure::InvalidChainState => "invalid_chain_state",
        WalletShieldedSyncFailure::StorageUnavailable => "storage_unavailable",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{
        ChainNetworkId, WalletShieldedSyncSnapshot, WalletShieldedSyncState,
        WalletShieldedTokenBalance,
    };

    use super::*;

    struct RecordingShielded {
        state: Mutex<WalletShieldedSyncSnapshot>,
    }

    impl WalletShieldedSyncPort for RecordingShielded {
        fn shielded_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.state
                .lock()
                .map(|value| value.clone())
                .map_err(|_| WalletShieldedSyncPortError::Unavailable)
        }

        fn start_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| WalletShieldedSyncPortError::Unavailable)?;
            *state = snapshot(WalletShieldedSyncState::Syncing, 4, 9);
            Ok(state.clone())
        }

        fn cancel_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| WalletShieldedSyncPortError::Unavailable)?;
            *state = snapshot(WalletShieldedSyncState::Cancelled, 4, 9);
            Ok(state.clone())
        }
    }

    fn network() -> ChainNetworkId {
        ChainNetworkId::parse("undeployed").expect("network is valid")
    }

    fn snapshot(
        state: WalletShieldedSyncState,
        current: u64,
        target: u64,
    ) -> WalletShieldedSyncSnapshot {
        WalletShieldedSyncSnapshot::new(
            network(),
            state,
            Some(current),
            Some(target),
            4,
            Some(1),
            Some(5),
            vec![
                WalletShieldedTokenBalance::new("ab".repeat(32), u128::MAX)
                    .expect("token is valid"),
            ],
            Some(UnixTimestampMillis::new(42)),
            None,
        )
        .expect("fixture is valid")
    }

    #[test]
    fn maps_exact_shielded_progress_without_sdk_types() {
        let service = WalletShieldedSyncService::new(Arc::new(RecordingShielded {
            state: Mutex::new(WalletShieldedSyncSnapshot::never_synced(network())),
        }));
        let command = WalletShieldedSyncCommand {
            profile_id: "profile_test".to_owned(),
        };

        let initial = GetWalletShieldedSyncStatusUseCase::execute(&service, command.clone())
            .expect("status is available");
        assert_eq!(initial.state, "never_synced");
        let started = StartWalletShieldedSyncUseCase::execute(&service, command.clone())
            .expect("sync starts");
        assert_eq!(started.current_cursor, Some(4));
        assert_eq!(started.owned_note_count, Some(1));
        assert_eq!(started.balances[0].atomic_units, u128::MAX.to_string());
        let cancelled =
            CancelWalletShieldedSyncUseCase::execute(&service, command).expect("sync cancels");
        assert_eq!(cancelled.state, "cancelled");
    }
}
