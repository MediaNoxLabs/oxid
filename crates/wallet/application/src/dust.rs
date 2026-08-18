// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, sync::Arc};

use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::{
    WalletDustSyncFailure, WalletDustSyncSnapshot, WalletDustSyncState, WalletProfileId,
};

/// Stable adapter failures for starting, reading, or cancelling DUST sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustSyncPortError {
    Conflict,
    UnsupportedNetwork,
    ProtectionNotInitialized,
    ProtectionLocked,
    Unavailable,
    InvalidData,
}

impl fmt::Display for WalletDustSyncPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Conflict => "DUST synchronization is already running",
            Self::UnsupportedNetwork => "wallet network is not supported",
            Self::ProtectionNotInitialized => "wallet protection is not initialized",
            Self::ProtectionLocked => "wallet is locked",
            Self::Unavailable => "DUST synchronization is unavailable",
            Self::InvalidData => "DUST synchronization returned invalid data",
        };
        formatter.write_str(message)
    }
}

impl Error for WalletDustSyncPortError {}

/// Focused outgoing port for an adapter-owned, off-renderer DUST sync session.
pub trait WalletDustSyncPort: Send + Sync {
    /// Return an already-published bounded snapshot. Implementations must not
    /// perform custody, filesystem, transport, or ledger work in this method;
    /// the adapter-owned sync worker publishes that work's state separately.
    fn dust_status(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError>;

    fn start_dust_sync(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError>;

    fn cancel_dust_sync(
        &self,
        profile_id: &WalletProfileId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError>;
}

/// Profile-scoped query or command for the DUST sync capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustSyncCommand {
    pub profile_id: String,
}

/// Safe DUST sync projection returned to UI, headless, and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustSyncView {
    pub network_id: String,
    pub state: String,
    pub current_cursor: Option<u64>,
    pub target_cursor: Option<u64>,
    pub events_processed: u64,
    pub balance_atomic_units: Option<String>,
    pub updated_at_millis: Option<u64>,
    pub failure: Option<String>,
}

impl From<&WalletDustSyncSnapshot> for WalletDustSyncView {
    fn from(snapshot: &WalletDustSyncSnapshot) -> Self {
        Self {
            network_id: snapshot.network_id().as_str().to_owned(),
            state: state_name(snapshot.state()).to_owned(),
            current_cursor: snapshot.current_cursor(),
            target_cursor: snapshot.target_cursor(),
            events_processed: snapshot.events_processed(),
            balance_atomic_units: snapshot
                .balance_atomic_units()
                .map(|value| value.to_string()),
            updated_at_millis: snapshot.updated_at().map(|value| value.value()),
            failure: snapshot.failure().map(failure_name).map(str::to_owned),
        }
    }
}

/// Structured validation and adapter failures for DUST sync use cases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletDustSyncError {
    InvalidProfileIdentifier(OpaqueIdError),
    Port(WalletDustSyncPortError),
}

impl fmt::Display for WalletDustSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
            Self::Port(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletDustSyncError {}

pub trait GetWalletDustSyncStatusUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletDustSyncCommand,
    ) -> Result<WalletDustSyncView, WalletDustSyncError>;
}

pub trait StartWalletDustSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletDustSyncCommand,
    ) -> Result<WalletDustSyncView, WalletDustSyncError>;
}

pub trait CancelWalletDustSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: WalletDustSyncCommand,
    ) -> Result<WalletDustSyncView, WalletDustSyncError>;
}

/// Application service preserving a narrow DUST synchronization boundary.
pub struct WalletDustSyncService<D> {
    dust: Arc<D>,
}

impl<D> WalletDustSyncService<D> {
    #[must_use]
    pub const fn new(dust: Arc<D>) -> Self {
        Self { dust }
    }

    fn profile(command: WalletDustSyncCommand) -> Result<WalletProfileId, WalletDustSyncError> {
        WalletProfileId::parse(command.profile_id)
            .map_err(WalletDustSyncError::InvalidProfileIdentifier)
    }
}

impl<D> GetWalletDustSyncStatusUseCase for WalletDustSyncService<D>
where
    D: WalletDustSyncPort + 'static,
{
    fn execute(
        &self,
        command: WalletDustSyncCommand,
    ) -> Result<WalletDustSyncView, WalletDustSyncError> {
        let profile = Self::profile(command)?;
        self.dust
            .dust_status(&profile)
            .map(|snapshot| WalletDustSyncView::from(&snapshot))
            .map_err(WalletDustSyncError::Port)
    }
}

impl<D> StartWalletDustSyncUseCase for WalletDustSyncService<D>
where
    D: WalletDustSyncPort + 'static,
{
    fn execute(
        &self,
        command: WalletDustSyncCommand,
    ) -> Result<WalletDustSyncView, WalletDustSyncError> {
        let profile = Self::profile(command)?;
        self.dust
            .start_dust_sync(&profile)
            .map(|snapshot| WalletDustSyncView::from(&snapshot))
            .map_err(WalletDustSyncError::Port)
    }
}

impl<D> CancelWalletDustSyncUseCase for WalletDustSyncService<D>
where
    D: WalletDustSyncPort + 'static,
{
    fn execute(
        &self,
        command: WalletDustSyncCommand,
    ) -> Result<WalletDustSyncView, WalletDustSyncError> {
        let profile = Self::profile(command)?;
        self.dust
            .cancel_dust_sync(&profile)
            .map(|snapshot| WalletDustSyncView::from(&snapshot))
            .map_err(WalletDustSyncError::Port)
    }
}

const fn state_name(state: WalletDustSyncState) -> &'static str {
    match state {
        WalletDustSyncState::NeverSynced => "never_synced",
        WalletDustSyncState::Syncing => "syncing",
        WalletDustSyncState::Synced => "synced",
        WalletDustSyncState::Cached => "cached",
        WalletDustSyncState::Cancelled => "cancelled",
        WalletDustSyncState::Stalled => "stalled",
        WalletDustSyncState::Unavailable => "unavailable",
    }
}

const fn failure_name(failure: WalletDustSyncFailure) -> &'static str {
    match failure {
        WalletDustSyncFailure::ProtectionNotInitialized => "protection_not_initialized",
        WalletDustSyncFailure::ProtectionLocked => "protection_locked",
        WalletDustSyncFailure::UnsupportedNetwork => "unsupported_network",
        WalletDustSyncFailure::TransportUnavailable => "transport_unavailable",
        WalletDustSyncFailure::TimedOut => "timed_out",
        WalletDustSyncFailure::InvalidChainState => "invalid_chain_state",
        WalletDustSyncFailure::StorageUnavailable => "storage_unavailable",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{ChainNetworkId, WalletDustSyncSnapshot};

    use super::*;

    struct RecordingDust {
        state: Mutex<WalletDustSyncSnapshot>,
    }

    impl WalletDustSyncPort for RecordingDust {
        fn dust_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.state
                .lock()
                .map(|value| value.clone())
                .map_err(|_| WalletDustSyncPortError::Unavailable)
        }

        fn start_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| WalletDustSyncPortError::Unavailable)?;
            *state = snapshot(WalletDustSyncState::Syncing, Some(4), Some(9));
            Ok(state.clone())
        }

        fn cancel_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| WalletDustSyncPortError::Unavailable)?;
            *state = snapshot(WalletDustSyncState::Cancelled, Some(4), Some(9));
            Ok(state.clone())
        }
    }

    fn network() -> ChainNetworkId {
        ChainNetworkId::parse("undeployed").expect("network is valid")
    }

    fn snapshot(
        state: WalletDustSyncState,
        current: Option<u64>,
        target: Option<u64>,
    ) -> WalletDustSyncSnapshot {
        WalletDustSyncSnapshot::new(
            network(),
            state,
            current,
            target,
            4,
            Some(12_000_000_000_000_000),
            Some(UnixTimestampMillis::new(42)),
            None,
        )
        .expect("fixture is valid")
    }

    #[test]
    fn maps_exact_progress_without_sdk_types() {
        let service = WalletDustSyncService::new(Arc::new(RecordingDust {
            state: Mutex::new(WalletDustSyncSnapshot::never_synced(network())),
        }));
        let command = WalletDustSyncCommand {
            profile_id: "profile_test".to_owned(),
        };

        let initial = GetWalletDustSyncStatusUseCase::execute(&service, command.clone())
            .expect("status is available");
        assert_eq!(initial.state, "never_synced");
        let started =
            StartWalletDustSyncUseCase::execute(&service, command.clone()).expect("sync starts");
        assert_eq!(started.current_cursor, Some(4));
        assert_eq!(
            started.balance_atomic_units.as_deref(),
            Some("12000000000000000")
        );
        let cancelled =
            CancelWalletDustSyncUseCase::execute(&service, command).expect("sync cancels");
        assert_eq!(cancelled.state, "cancelled");
    }
}
