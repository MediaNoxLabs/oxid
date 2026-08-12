// SPDX-License-Identifier: Apache-2.0

//! Off-renderer, profile-scoped DUST synchronization controllers.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use midnight_base_crypto::time::Timestamp;
use midnight_ledger::dust::{DustPublicKey, DustSecretKey};
use oxid_platform_ports::ClockPort;
use oxid_wallet_application::{
    WalletDerivedSecretUsePort, WalletDustSyncPortError, WalletHdPath, WalletHdPathComponent,
    WalletSecurityPortError, WalletTransactionPortError,
};
use oxid_wallet_domain::{
    ChainNetworkId, WalletDustSyncFailure, WalletDustSyncSnapshot, WalletDustSyncState,
    WalletProfileId,
};

use crate::{
    BIP44_PURPOSE, DUST_INDEX, DUST_ROLE, MIDNIGHT_COIN_TYPE, SPECKS_PER_DUST,
    dust_checkpoint::{
        DustCheckpointStoreError, MidnightDustCheckpointStore, StoredDustCheckpoint,
    },
    submission::{
        DustSyncProgress, MidnightStandaloneConfig, ensure_submission_active, fetch_chain_tip,
        synchronize_dust_with_control,
    },
};

#[cfg(test)]
const DUST_ACCOUNT_INDEX: u32 = 0;
const SIMULATED_TARGET_CURSOR: u64 = 2;
const SIMULATED_BALANCE_ATOMIC_UNITS: u128 = 12 * SPECKS_PER_DUST;

pub(crate) trait MidnightDustSyncController: Send + Sync {
    fn status(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError>;

    fn start(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
        account_index: u32,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError>;

    fn cancel(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UnavailableMidnightDustSyncController;

impl MidnightDustSyncController for UnavailableMidnightDustSyncController {
    fn status(
        &self,
        _: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
        Ok(WalletDustSyncSnapshot::unavailable(network_id.clone()))
    }

    fn start(
        &self,
        _: &WalletProfileId,
        _: &ChainNetworkId,
        _: u32,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
        Err(WalletDustSyncPortError::Unavailable)
    }

    fn cancel(
        &self,
        _: &WalletProfileId,
        _: &ChainNetworkId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
        Err(WalletDustSyncPortError::Unavailable)
    }
}

/// Deterministic poll-driven controller used by the headless conformance stack.
pub(crate) struct SimulatedMidnightDustSyncController<C, K> {
    clock: Arc<C>,
    keys: Arc<K>,
    sessions: Mutex<HashMap<(WalletProfileId, ChainNetworkId), WalletDustSyncSnapshot>>,
}

impl<C, K> SimulatedMidnightDustSyncController<C, K> {
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

impl<C, K> MidnightDustSyncController for SimulatedMidnightDustSyncController<C, K>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + 'static,
{
    fn status(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
        let key = Self::key(profile_id, network_id);
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| WalletDustSyncPortError::Unavailable)?;
        let Some(current) = sessions.get(&key).cloned() else {
            return Ok(WalletDustSyncSnapshot::never_synced(network_id.clone()));
        };
        if current.state() != WalletDustSyncState::Syncing {
            return Ok(current);
        }

        let (cursor, processed, state) = match current.current_cursor() {
            None => (0, 1, WalletDustSyncState::Syncing),
            Some(0) => (1, 2, WalletDustSyncState::Syncing),
            Some(1) => (2, 3, WalletDustSyncState::Synced),
            Some(_) => (2, 0, WalletDustSyncState::Synced),
        };
        let balance = SIMULATED_BALANCE_ATOMIC_UNITS
            .checked_mul(u128::from(cursor + 1))
            .and_then(|value| value.checked_div(u128::from(SIMULATED_TARGET_CURSOR + 1)))
            .ok_or(WalletDustSyncPortError::InvalidData)?;
        let next = snapshot(
            network_id.clone(),
            state,
            Some(cursor),
            Some(SIMULATED_TARGET_CURSOR),
            processed,
            Some(balance),
            Some(now(self.clock.as_ref())?),
            None,
        )?;
        sessions.insert(key, next.clone());
        Ok(next)
    }

    fn start(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
        account_index: u32,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
        let path = dust_path(account_index)?;
        self.keys
            .use_derived_secret(profile_id, &path, &mut |_| Ok(()))
            .map_err(map_security_error)?;
        let key = Self::key(profile_id, network_id);
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| WalletDustSyncPortError::Unavailable)?;
        if sessions
            .get(&key)
            .is_some_and(|status| status.state() == WalletDustSyncState::Syncing)
        {
            return Err(WalletDustSyncPortError::Conflict);
        }
        let previous = sessions
            .get(&key)
            .cloned()
            .unwrap_or_else(|| WalletDustSyncSnapshot::never_synced(network_id.clone()));
        let started = snapshot(
            network_id.clone(),
            WalletDustSyncState::Syncing,
            previous.current_cursor(),
            previous.target_cursor(),
            0,
            previous.balance_atomic_units(),
            previous.updated_at(),
            None,
        )?;
        sessions.insert(key, started.clone());
        Ok(started)
    }

    fn cancel(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
        let key = Self::key(profile_id, network_id);
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| WalletDustSyncPortError::Unavailable)?;
        let current = sessions
            .get(&key)
            .cloned()
            .ok_or(WalletDustSyncPortError::Conflict)?;
        if current.state() != WalletDustSyncState::Syncing {
            return Err(WalletDustSyncPortError::Conflict);
        }
        let cancelled = snapshot(
            network_id.clone(),
            WalletDustSyncState::Cancelled,
            current.current_cursor(),
            current.target_cursor(),
            current.events_processed(),
            current.balance_atomic_units(),
            current.updated_at(),
            None,
        )?;
        sessions.insert(key, cancelled.clone());
        Ok(cancelled)
    }
}

struct LiveSession {
    snapshot: WalletDustSyncSnapshot,
    cancellation: Arc<AtomicBool>,
    running: bool,
}

/// Native standalone controller. Network I/O and ledger folding run only on a
/// dedicated worker thread; incoming adapters read bounded snapshots.
pub(crate) struct LiveMidnightDustSyncController<C, K> {
    config: MidnightStandaloneConfig,
    checkpoints: Arc<dyn MidnightDustCheckpointStore>,
    clock: Arc<C>,
    keys: Arc<K>,
    sessions: Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
}

impl<C, K> LiveMidnightDustSyncController<C, K> {
    pub(crate) fn new(
        config: MidnightStandaloneConfig,
        checkpoints: Arc<dyn MidnightDustCheckpointStore>,
        clock: Arc<C>,
        keys: Arc<K>,
    ) -> Self {
        Self {
            config,
            checkpoints,
            clock,
            keys,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<C, K> MidnightDustSyncController for LiveMidnightDustSyncController<C, K>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + 'static,
{
    fn status(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
        self.sessions
            .lock()
            .map_err(|_| WalletDustSyncPortError::Unavailable)?
            .get(&(profile_id.clone(), network_id.clone()))
            .map(|session| session.snapshot.clone())
            .map_or_else(
                || Ok(WalletDustSyncSnapshot::never_synced(network_id.clone())),
                Ok,
            )
    }

    fn start(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
        account_index: u32,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
        if network_id != self.config.indexer().network_id() {
            return Err(WalletDustSyncPortError::UnsupportedNetwork);
        }
        let key = (profile_id.clone(), network_id.clone());
        let cancellation = Arc::new(AtomicBool::new(false));
        let started = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| WalletDustSyncPortError::Unavailable)?;
            if sessions.get(&key).is_some_and(|session| session.running) {
                return Err(WalletDustSyncPortError::Conflict);
            }
            let previous = sessions
                .get(&key)
                .map(|session| session.snapshot.clone())
                .unwrap_or_else(|| WalletDustSyncSnapshot::never_synced(network_id.clone()));
            let started = snapshot(
                network_id.clone(),
                WalletDustSyncState::Syncing,
                previous.current_cursor(),
                previous.target_cursor(),
                0,
                previous.balance_atomic_units(),
                previous.updated_at(),
                None,
            )?;
            sessions.insert(
                key.clone(),
                LiveSession {
                    snapshot: started.clone(),
                    cancellation: Arc::clone(&cancellation),
                    running: true,
                },
            );
            started
        };

        let config = self.config.clone();
        let checkpoints = Arc::clone(&self.checkpoints);
        let clock = Arc::clone(&self.clock);
        let keys = Arc::clone(&self.keys);
        let sessions = Arc::clone(&self.sessions);
        let profile = profile_id.clone();
        let network = network_id.clone();
        let worker_cancellation = Arc::clone(&cancellation);
        let spawn = thread::Builder::new()
            .name("oxid-midnight-dust-sync".to_owned())
            .spawn(move || {
                run_live_sync(
                    &config,
                    checkpoints.as_ref(),
                    clock.as_ref(),
                    keys.as_ref(),
                    &sessions,
                    &profile,
                    &network,
                    account_index,
                    &worker_cancellation,
                );
            });
        if spawn.is_err() {
            finish_with_failure(
                &self.sessions,
                &key,
                &cancellation,
                WalletDustSyncFailure::TransportUnavailable,
            );
            return Err(WalletDustSyncPortError::Unavailable);
        }
        Ok(started)
    }

    fn cancel(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
        let key = (profile_id.clone(), network_id.clone());
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| WalletDustSyncPortError::Unavailable)?;
        let session = sessions
            .get_mut(&key)
            .ok_or(WalletDustSyncPortError::Conflict)?;
        if !session.running {
            return Err(WalletDustSyncPortError::Conflict);
        }
        session.cancellation.store(true, Ordering::Release);
        session.snapshot = snapshot(
            network_id.clone(),
            WalletDustSyncState::Cancelled,
            session.snapshot.current_cursor(),
            session.snapshot.target_cursor(),
            session.snapshot.events_processed(),
            session.snapshot.balance_atomic_units(),
            session.snapshot.updated_at(),
            None,
        )?;
        Ok(session.snapshot.clone())
    }
}

#[allow(clippy::too_many_arguments)]
fn run_live_sync<C, K>(
    config: &MidnightStandaloneConfig,
    checkpoints: &dyn MidnightDustCheckpointStore,
    clock: &C,
    keys: &K,
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    profile_id: &WalletProfileId,
    network_id: &ChainNetworkId,
    account_index: u32,
    cancellation: &Arc<AtomicBool>,
) where
    C: ClockPort,
    K: WalletDerivedSecretUsePort,
{
    let key = (profile_id.clone(), network_id.clone());
    let path = match dust_path(account_index) {
        Ok(path) => path,
        Err(_) => {
            finish_with_failure(
                sessions,
                &key,
                cancellation,
                WalletDustSyncFailure::InvalidChainState,
            );
            return;
        }
    };
    let mut sync_result = None;
    let security_result = keys.use_derived_secret(profile_id, &path, &mut |seed| {
        sync_result = Some(sync_live_with_seed(
            config,
            checkpoints,
            clock,
            sessions,
            &key,
            cancellation,
            seed,
        ));
        Ok(())
    });
    let result = match security_result {
        Ok(()) => sync_result.unwrap_or(Err(WalletTransactionPortError::InvalidData)),
        Err(error) => {
            finish_with_failure(sessions, &key, cancellation, security_failure(error));
            return;
        }
    };
    match result {
        Ok(snapshot) => finish_with_snapshot(sessions, &key, cancellation, snapshot),
        Err(WalletTransactionPortError::SubmissionCancelled) => {
            finish_cancelled(sessions, &key, cancellation)
        }
        Err(error) => finish_with_failure(sessions, &key, cancellation, sync_failure(error)),
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_live_with_seed<C>(
    config: &MidnightStandaloneConfig,
    checkpoints: &dyn MidnightDustCheckpointStore,
    clock: &C,
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
    seed: &[u8; 32],
) -> Result<WalletDustSyncSnapshot, WalletTransactionPortError>
where
    C: ClockPort,
{
    ensure_submission_active(cancellation)?;
    let secret_key = DustSecretKey::derive_secret_key(seed);
    let public_key = DustPublicKey::from(secret_key.clone());
    let latest = match checkpoints.load_latest(&key.1, &public_key) {
        Ok(checkpoint) => checkpoint,
        Err(DustCheckpointStoreError::InvalidData) => None,
        Err(DustCheckpointStoreError::Unavailable) => {
            return Err(WalletTransactionPortError::Unavailable);
        }
    };
    if let Some(checkpoint) = latest.as_ref() {
        let cached = progress_snapshot(
            &key.1,
            WalletDustSyncState::Cached,
            checkpoint.current_cursor,
            checkpoint.target_cursor,
            0,
            checkpoint.state.wallet_balance(checkpoint.state.sync_time),
            checkpoint.updated_at,
            None,
        )?;
        update_running_snapshot(sessions, key, cancellation, cached)
            .map_err(map_dust_to_transaction_error)?;
    }
    ensure_submission_active(cancellation)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    runtime.block_on(async {
        let chain_tip = fetch_chain_tip(config.indexer_http_url()).await?;
        ensure_submission_active(cancellation)?;
        let checkpoint =
            latest.filter(|checkpoint| checkpoint.state.params == chain_tip.parameters.dust);
        let current_time = |state_time: Timestamp| {
            if chain_tip.timestamp > state_time {
                chain_tip.timestamp
            } else {
                state_time
            }
        };
        let mut observe = |progress: &DustSyncProgress| {
            ensure_submission_active(cancellation)?;
            let updated_at = now(clock).map_err(map_dust_to_transaction_error)?;
            checkpoints
                .save(
                    &key.1,
                    &public_key,
                    &StoredDustCheckpoint {
                        current_cursor: progress.current_cursor,
                        target_cursor: progress.target_cursor,
                        updated_at,
                        state: progress.state.clone(),
                    },
                )
                .map_err(map_checkpoint_error)?;
            let status = progress_snapshot(
                &key.1,
                WalletDustSyncState::Syncing,
                progress.current_cursor,
                progress.target_cursor,
                u64::try_from(progress.events_processed)
                    .map_err(|_| WalletTransactionPortError::InvalidData)?,
                progress
                    .state
                    .wallet_balance(current_time(progress.state.sync_time)),
                updated_at,
                None,
            )?;
            update_running_snapshot(sessions, key, cancellation, status)
                .map_err(map_dust_to_transaction_error)
        };
        let synchronized = synchronize_dust_with_control(
            config.indexer().websocket_url(),
            &secret_key,
            chain_tip.parameters.dust,
            checkpoint,
            cancellation,
            &mut observe,
        )
        .await?;
        ensure_submission_active(cancellation)?;
        let updated_at = now(clock).map_err(map_dust_to_transaction_error)?;
        let balance = synchronized
            .state
            .wallet_balance(current_time(synchronized.state.sync_time));
        progress_snapshot(
            &key.1,
            WalletDustSyncState::Synced,
            synchronized.current_cursor,
            synchronized.target_cursor,
            u64::try_from(synchronized.events_processed)
                .map_err(|_| WalletTransactionPortError::InvalidData)?,
            balance,
            updated_at,
            None,
        )
    })
}

fn update_running_snapshot(
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
    snapshot: WalletDustSyncSnapshot,
) -> Result<(), WalletDustSyncPortError> {
    let mut sessions = sessions
        .lock()
        .map_err(|_| WalletDustSyncPortError::Unavailable)?;
    let session = sessions
        .get_mut(key)
        .ok_or(WalletDustSyncPortError::Unavailable)?;
    if !Arc::ptr_eq(&session.cancellation, cancellation) {
        return Err(WalletDustSyncPortError::Conflict);
    }
    session.snapshot = snapshot;
    Ok(())
}

fn finish_with_snapshot(
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
    snapshot: WalletDustSyncSnapshot,
) {
    if let Ok(mut sessions) = sessions.lock()
        && let Some(session) = sessions.get_mut(key)
        && Arc::ptr_eq(&session.cancellation, cancellation)
    {
        session.snapshot = snapshot;
        session.running = false;
    }
}

fn finish_cancelled(
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
) {
    if let Ok(mut sessions) = sessions.lock()
        && let Some(session) = sessions.get_mut(key)
        && Arc::ptr_eq(&session.cancellation, cancellation)
    {
        if let Ok(cancelled) = snapshot(
            key.1.clone(),
            WalletDustSyncState::Cancelled,
            session.snapshot.current_cursor(),
            session.snapshot.target_cursor(),
            session.snapshot.events_processed(),
            session.snapshot.balance_atomic_units(),
            session.snapshot.updated_at(),
            None,
        ) {
            session.snapshot = cancelled;
        }
        session.running = false;
    }
}

fn finish_with_failure(
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
    failure: WalletDustSyncFailure,
) {
    if let Ok(mut sessions) = sessions.lock()
        && let Some(session) = sessions.get_mut(key)
        && Arc::ptr_eq(&session.cancellation, cancellation)
    {
        let state = match (
            session.snapshot.current_cursor(),
            session.snapshot.target_cursor(),
        ) {
            (Some(current), Some(target)) if current == target => WalletDustSyncState::Cached,
            _ => WalletDustSyncState::Stalled,
        };
        if let Ok(failed) = snapshot(
            key.1.clone(),
            state,
            session.snapshot.current_cursor(),
            session.snapshot.target_cursor(),
            session.snapshot.events_processed(),
            session.snapshot.balance_atomic_units(),
            session.snapshot.updated_at(),
            Some(failure),
        ) {
            session.snapshot = failed;
        }
        session.running = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn progress_snapshot(
    network_id: &ChainNetworkId,
    state: WalletDustSyncState,
    current_cursor: u64,
    target_cursor: u64,
    events_processed: u64,
    balance_atomic_units: u128,
    updated_at: oxid_foundation::UnixTimestampMillis,
    failure: Option<WalletDustSyncFailure>,
) -> Result<WalletDustSyncSnapshot, WalletTransactionPortError> {
    snapshot(
        network_id.clone(),
        state,
        Some(current_cursor),
        Some(target_cursor),
        events_processed,
        Some(balance_atomic_units),
        Some(updated_at),
        failure,
    )
    .map_err(map_dust_to_transaction_error)
}

#[allow(clippy::too_many_arguments)]
fn snapshot(
    network_id: ChainNetworkId,
    state: WalletDustSyncState,
    current_cursor: Option<u64>,
    target_cursor: Option<u64>,
    events_processed: u64,
    balance_atomic_units: Option<u128>,
    updated_at: Option<oxid_foundation::UnixTimestampMillis>,
    failure: Option<WalletDustSyncFailure>,
) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
    WalletDustSyncSnapshot::new(
        network_id,
        state,
        current_cursor,
        target_cursor,
        events_processed,
        balance_atomic_units,
        updated_at,
        failure,
    )
    .map_err(|_| WalletDustSyncPortError::InvalidData)
}

fn now<C: ClockPort>(
    clock: &C,
) -> Result<oxid_foundation::UnixTimestampMillis, WalletDustSyncPortError> {
    clock
        .now()
        .map_err(|_| WalletDustSyncPortError::Unavailable)
}

fn dust_path(account_index: u32) -> Result<WalletHdPath, WalletDustSyncPortError> {
    let component = |index, hardened| {
        WalletHdPathComponent::new(index, hardened)
            .map_err(|_| WalletDustSyncPortError::InvalidData)
    };
    WalletHdPath::new(vec![
        component(BIP44_PURPOSE, true)?,
        component(MIDNIGHT_COIN_TYPE, true)?,
        component(account_index, true)?,
        component(DUST_ROLE, false)?,
        component(DUST_INDEX, false)?,
    ])
    .map_err(|_| WalletDustSyncPortError::InvalidData)
}

const fn map_security_error(error: WalletSecurityPortError) -> WalletDustSyncPortError {
    match error {
        WalletSecurityPortError::NotInitialized => {
            WalletDustSyncPortError::ProtectionNotInitialized
        }
        WalletSecurityPortError::Locked => WalletDustSyncPortError::ProtectionLocked,
        WalletSecurityPortError::Unavailable => WalletDustSyncPortError::Unavailable,
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::NotFound
        | WalletSecurityPortError::Conflict
        | WalletSecurityPortError::UnsupportedAlgorithm
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => WalletDustSyncPortError::InvalidData,
    }
}

const fn security_failure(error: WalletSecurityPortError) -> WalletDustSyncFailure {
    match error {
        WalletSecurityPortError::NotInitialized => WalletDustSyncFailure::ProtectionNotInitialized,
        WalletSecurityPortError::Locked => WalletDustSyncFailure::ProtectionLocked,
        WalletSecurityPortError::Unavailable => WalletDustSyncFailure::TransportUnavailable,
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::NotFound
        | WalletSecurityPortError::Conflict
        | WalletSecurityPortError::UnsupportedAlgorithm
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => WalletDustSyncFailure::InvalidChainState,
    }
}

const fn sync_failure(error: WalletTransactionPortError) -> WalletDustSyncFailure {
    match error {
        WalletTransactionPortError::ProtectionNotInitialized => {
            WalletDustSyncFailure::ProtectionNotInitialized
        }
        WalletTransactionPortError::ProtectionLocked => WalletDustSyncFailure::ProtectionLocked,
        WalletTransactionPortError::UnsupportedNetwork => WalletDustSyncFailure::UnsupportedNetwork,
        WalletTransactionPortError::Timeout => WalletDustSyncFailure::TimedOut,
        WalletTransactionPortError::Unavailable
        | WalletTransactionPortError::SubmissionOutcomeUnknown => {
            WalletDustSyncFailure::TransportUnavailable
        }
        WalletTransactionPortError::SubmissionCancelled => {
            WalletDustSyncFailure::TransportUnavailable
        }
        WalletTransactionPortError::AccountNotDerived
        | WalletTransactionPortError::AccountNotSynchronized
        | WalletTransactionPortError::InvalidRecipient
        | WalletTransactionPortError::RecipientNetworkMismatch
        | WalletTransactionPortError::InsufficientFunds
        | WalletTransactionPortError::DraftNotFound
        | WalletTransactionPortError::DraftExpired
        | WalletTransactionPortError::DraftConflict
        | WalletTransactionPortError::SubmissionInProgress
        | WalletTransactionPortError::SubmissionNotInProgress
        | WalletTransactionPortError::SubmissionCancellationUnsafe
        | WalletTransactionPortError::AuthorizationChallengeMismatch
        | WalletTransactionPortError::InsufficientDust
        | WalletTransactionPortError::InvalidChainState
        | WalletTransactionPortError::ProvingFailed
        | WalletTransactionPortError::SubmissionRejected
        | WalletTransactionPortError::InvalidData => WalletDustSyncFailure::InvalidChainState,
    }
}

const fn map_checkpoint_error(error: DustCheckpointStoreError) -> WalletTransactionPortError {
    match error {
        DustCheckpointStoreError::Unavailable => WalletTransactionPortError::Unavailable,
        DustCheckpointStoreError::InvalidData => WalletTransactionPortError::InvalidData,
    }
}

const fn map_dust_to_transaction_error(
    error: WalletDustSyncPortError,
) -> WalletTransactionPortError {
    match error {
        WalletDustSyncPortError::UnsupportedNetwork => {
            WalletTransactionPortError::UnsupportedNetwork
        }
        WalletDustSyncPortError::ProtectionNotInitialized => {
            WalletTransactionPortError::ProtectionNotInitialized
        }
        WalletDustSyncPortError::ProtectionLocked => WalletTransactionPortError::ProtectionLocked,
        WalletDustSyncPortError::Conflict => WalletTransactionPortError::SubmissionInProgress,
        WalletDustSyncPortError::Unavailable => WalletTransactionPortError::Unavailable,
        WalletDustSyncPortError::InvalidData => WalletTransactionPortError::InvalidData,
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
                    (2, false),
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
    fn simulated_sync_is_monotonic_resumable_and_cancellable() {
        let sync = SimulatedMidnightDustSyncController::new(
            Arc::new(FixedClock),
            Arc::new(AvailableKeys(0)),
        );
        assert_eq!(
            sync.status(&profile(), &network())
                .expect("initial status")
                .state(),
            WalletDustSyncState::NeverSynced
        );
        assert_eq!(
            sync.start(&profile(), &network(), DUST_ACCOUNT_INDEX)
                .expect("sync starts")
                .state(),
            WalletDustSyncState::Syncing
        );
        let first = sync.status(&profile(), &network()).expect("first progress");
        assert_eq!(
            (first.current_cursor(), first.target_cursor()),
            (Some(0), Some(2))
        );
        let cancelled = sync.cancel(&profile(), &network()).expect("sync cancels");
        assert_eq!(cancelled.state(), WalletDustSyncState::Cancelled);
        let resumed = sync
            .start(&profile(), &network(), DUST_ACCOUNT_INDEX)
            .expect("sync resumes");
        assert_eq!(resumed.current_cursor(), Some(0));
        assert_eq!(
            sync.status(&profile(), &network())
                .expect("second progress")
                .current_cursor(),
            Some(1)
        );
        let complete = sync.status(&profile(), &network()).expect("sync completes");
        assert_eq!(complete.state(), WalletDustSyncState::Synced);
        assert_eq!(
            complete.balance_atomic_units(),
            Some(SIMULATED_BALANCE_ATOMIC_UNITS)
        );
        let current = sync
            .start(&profile(), &network(), DUST_ACCOUNT_INDEX)
            .expect("current check starts");
        assert_eq!(current.current_cursor(), Some(2));
        assert_eq!(
            sync.status(&profile(), &network())
                .expect("current check completes")
                .events_processed(),
            0
        );
    }

    #[test]
    fn simulated_sync_uses_the_active_account_index_for_the_dust_child() {
        let sync = SimulatedMidnightDustSyncController::new(
            Arc::new(FixedClock),
            Arc::new(AvailableKeys(7)),
        );

        assert_eq!(
            sync.start(&profile(), &network(), 7)
                .expect("account-scoped sync starts")
                .state(),
            WalletDustSyncState::Syncing
        );
    }
}
