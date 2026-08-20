// SPDX-License-Identifier: Apache-2.0

//! Off-renderer, profile-scoped shielded synchronization controllers.

use std::{
    cell::Cell,
    collections::HashMap,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use midnight_base_crypto::{hash::HashOutput, schnorr::Signature};
use midnight_coin_structure::coin::{
    Info as CoinInfo, Nonce as CoinNonce, PublicKey as CoinPublicKey, ShieldedTokenType,
};
use midnight_ledger::structure::{ProofPreimageMarker, StandardTransaction, Transaction};
use midnight_storage::{DefaultDB, storage::HashMap as LedgerHashMap};
use midnight_transient_crypto::{
    commitment::PedersenRandomness, encryption::PublicKey as EncryptionPublicKey,
};
use midnight_zswap::{
    Offer as ZswapOffer, Output as ZswapOutput,
    keys::{SecretKeys as ZswapSecretKeys, Seed as ZswapSeed},
    local::State as ZswapState,
};
use oxid_diagnostics_application::{
    DiagnosticCode, DiagnosticEventSinkPort, DiagnosticSeverity, NoopDiagnosticEventSink,
};
use oxid_platform_ports::ClockPort;
use oxid_wallet_application::{
    WalletDerivedSecretUsePort, WalletHdPath, WalletHdPathComponent, WalletSecurityPortError,
    WalletShieldedSyncPortError,
};
use oxid_wallet_domain::{
    ChainNetworkId, WalletProfileId, WalletShieldedSyncFailure, WalletShieldedSyncSnapshot,
    WalletShieldedSyncState, WalletShieldedTokenBalance,
};
use sha2::{Digest as _, Sha256};

use crate::{
    BIP44_PURPOSE, MIDNIGHT_COIN_TYPE, ZSWAP_INDEX, ZSWAP_ROLE,
    indexer::MidnightIndexerConfig,
    shielded::project_zswap_state,
    shielded_checkpoint::{
        MidnightShieldedCheckpointStore, ShieldedCheckpointStoreError, StoredShieldedCheckpoint,
    },
    shielded_transport::{
        ShieldedSyncProgress, ShieldedTransportError, source_fingerprint,
        synchronize_shielded_with_control,
    },
};

type UnprovenTransaction =
    Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;

/// Adapter-private result of selecting and constructing one canonical Zswap
/// spend. The transaction and pending local state never cross an application
/// or incoming-adapter boundary.
pub(crate) struct MidnightShieldedTransferPlan {
    pub(crate) transaction: UnprovenTransaction,
    pub(crate) input_count: u16,
    pub(crate) change_atomic_units: u128,
    pub(crate) reservation_fingerprint: [u8; 32],
}

#[derive(Clone, Copy)]
pub(crate) struct MidnightShieldedTransferRequest {
    pub(crate) account_index: u32,
    pub(crate) recipient_coin_public_key: CoinPublicKey,
    pub(crate) recipient_encryption_public_key: EncryptionPublicKey,
    pub(crate) token_type: [u8; 32],
    pub(crate) amount_atomic_units: u128,
    pub(crate) expires_at_seconds: u64,
}

const SIMULATED_TARGET_CURSOR: u64 = 2;
const SIMULATED_TOKEN_TYPE: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
const SIMULATED_BALANCE_ATOMIC_UNITS: u128 = 5_000_000;

pub(crate) trait MidnightShieldedSyncController: Send + Sync {
    fn attach_diagnostic_sink(&self, _: Arc<dyn DiagnosticEventSinkPort>) {}

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

    fn prepare_transfer(
        &self,
        _: &WalletProfileId,
        _: &ChainNetworkId,
        _: MidnightShieldedTransferRequest,
    ) -> Result<MidnightShieldedTransferPlan, oxid_wallet_application::WalletTransactionPortError>
    {
        Err(oxid_wallet_application::WalletTransactionPortError::Unavailable)
    }
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

    fn prepare_transfer(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
        request: MidnightShieldedTransferRequest,
    ) -> Result<MidnightShieldedTransferPlan, oxid_wallet_application::WalletTransactionPortError>
    {
        use oxid_wallet_application::WalletTransactionPortError;

        let current = self
            .sessions
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?
            .get(&Self::key(profile_id, network_id))
            .cloned()
            .ok_or(WalletTransactionPortError::ShieldedStateNotCurrent)?;
        ensure_current_shielded_snapshot(&current)?;
        let path = shielded_path(request.account_index)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let mut plan = None;
        self.keys
            .use_derived_secret(profile_id, &path, &mut |seed| {
                let keys = ZswapSecretKeys::from(ZswapSeed::from(*seed));
                let token_type = ShieldedTokenType(HashOutput(request.token_type));
                let state = ZswapState::new()
                    .insert_coin(
                        &keys,
                        CoinInfo {
                            nonce: CoinNonce(HashOutput([0x53; 32])),
                            type_: token_type,
                            value: SIMULATED_BALANCE_ATOMIC_UNITS,
                        },
                    )
                    .map_err(|_| WalletSecurityPortError::InvalidOperation)?;
                plan = Some(build_shielded_transfer(state, &keys, network_id, request));
                Ok(())
            })
            .map_err(map_security_to_transaction_error)?;
        plan.ok_or(WalletTransactionPortError::InvalidData)?
    }
}

struct LiveSession {
    snapshot: WalletShieldedSyncSnapshot,
    cancellation: Arc<AtomicBool>,
    running: bool,
}

/// Native live controller. Transport, replay, and private checkpoint I/O run
/// only on a dedicated worker thread.
pub(crate) struct LiveMidnightShieldedSyncController<C, K> {
    config: MidnightIndexerConfig,
    checkpoints: Arc<dyn MidnightShieldedCheckpointStore>,
    clock: Arc<C>,
    keys: Arc<K>,
    sessions: Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    diagnostics: RwLock<Arc<dyn DiagnosticEventSinkPort>>,
}

impl<C, K> LiveMidnightShieldedSyncController<C, K> {
    pub(crate) fn new(
        config: MidnightIndexerConfig,
        checkpoints: Arc<dyn MidnightShieldedCheckpointStore>,
        clock: Arc<C>,
        keys: Arc<K>,
    ) -> Self {
        Self {
            config,
            checkpoints,
            clock,
            keys,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            diagnostics: RwLock::new(Arc::new(NoopDiagnosticEventSink)),
        }
    }
}

impl<C, K> MidnightShieldedSyncController for LiveMidnightShieldedSyncController<C, K>
where
    C: ClockPort + 'static,
    K: WalletDerivedSecretUsePort + 'static,
{
    fn attach_diagnostic_sink(&self, sink: Arc<dyn DiagnosticEventSinkPort>) {
        if let Ok(mut diagnostics) = self.diagnostics.write() {
            *diagnostics = sink;
        }
    }

    fn status(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        self.sessions
            .lock()
            .map_err(|_| WalletShieldedSyncPortError::Unavailable)?
            .get(&(profile_id.clone(), network_id.clone()))
            .map(|session| session.snapshot.clone())
            .map_or_else(
                || Ok(WalletShieldedSyncSnapshot::never_synced(network_id.clone())),
                Ok,
            )
    }

    fn start(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
        account_index: u32,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        if network_id != self.config.network_id() {
            return Err(WalletShieldedSyncPortError::UnsupportedNetwork);
        }
        let key = (profile_id.clone(), network_id.clone());
        let cancellation = Arc::new(AtomicBool::new(false));
        let started = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| WalletShieldedSyncPortError::Unavailable)?;
            if sessions.get(&key).is_some_and(|session| session.running) {
                return Err(WalletShieldedSyncPortError::Conflict);
            }
            let previous = sessions
                .get(&key)
                .map(|session| session.snapshot.clone())
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
        let diagnostics = self.diagnostics.read().map_or_else(
            |_| Arc::new(NoopDiagnosticEventSink) as Arc<dyn DiagnosticEventSinkPort>,
            |sink| Arc::clone(&*sink),
        );
        let worker_key = key.clone();
        let spawn = thread::Builder::new()
            .name("oxid-midnight-shielded-sync".to_owned())
            .spawn(move || {
                let completed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
                }));
                if completed.is_err() {
                    finish_with_failure(
                        &sessions,
                        &worker_key,
                        &worker_cancellation,
                        WalletShieldedSyncFailure::TransportUnavailable,
                    );
                    diagnostics.record(
                        DiagnosticCode::MidnightShieldedSyncWorkerPanicked,
                        DiagnosticSeverity::Error,
                    );
                    return;
                }
                let failed = sessions.lock().ok().and_then(|sessions| {
                    sessions
                        .get(&worker_key)
                        .map(|session| session.snapshot.failure().is_some())
                });
                if failed == Some(true) {
                    diagnostics.record(
                        DiagnosticCode::MidnightShieldedSyncFailed,
                        DiagnosticSeverity::Warning,
                    );
                }
            });
        if spawn.is_err() {
            finish_with_failure(
                &self.sessions,
                &key,
                &cancellation,
                WalletShieldedSyncFailure::TransportUnavailable,
            );
            self.diagnostics
                .read()
                .map_or_else(
                    |_| Arc::new(NoopDiagnosticEventSink) as Arc<dyn DiagnosticEventSinkPort>,
                    |sink| Arc::clone(&*sink),
                )
                .record(
                    DiagnosticCode::MidnightShieldedSyncWorkerSpawnFailed,
                    DiagnosticSeverity::Error,
                );
            return Err(WalletShieldedSyncPortError::Unavailable);
        }
        Ok(started)
    }

    fn cancel(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
    ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
        let key = (profile_id.clone(), network_id.clone());
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| WalletShieldedSyncPortError::Unavailable)?;
        let session = sessions
            .get_mut(&key)
            .ok_or(WalletShieldedSyncPortError::Conflict)?;
        if !session.running {
            return Err(WalletShieldedSyncPortError::Conflict);
        }
        session.cancellation.store(true, Ordering::Release);
        session.snapshot = snapshot(
            network_id.clone(),
            WalletShieldedSyncState::Cancelled,
            session.snapshot.current_cursor(),
            session.snapshot.target_cursor(),
            session.snapshot.events_processed(),
            session.snapshot.owned_note_count(),
            session.snapshot.commitment_count(),
            session.snapshot.balances().to_vec(),
            session.snapshot.updated_at(),
        )?;
        Ok(session.snapshot.clone())
    }

    fn prepare_transfer(
        &self,
        profile_id: &WalletProfileId,
        network_id: &ChainNetworkId,
        request: MidnightShieldedTransferRequest,
    ) -> Result<MidnightShieldedTransferPlan, oxid_wallet_application::WalletTransactionPortError>
    {
        use oxid_wallet_application::WalletTransactionPortError;

        if network_id != self.config.network_id() {
            return Err(WalletTransactionPortError::UnsupportedNetwork);
        }
        let current = self
            .sessions
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?
            .get(&(profile_id.clone(), network_id.clone()))
            .map(|session| session.snapshot.clone())
            .ok_or(WalletTransactionPortError::ShieldedStateNotCurrent)?;
        ensure_current_shielded_snapshot(&current)?;

        let path = shielded_path(request.account_index)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let source = source_fingerprint(self.config.websocket_url());
        let mut plan = None;
        self.keys
            .use_derived_secret(profile_id, &path, &mut |seed| {
                let keys = ZswapSecretKeys::from(ZswapSeed::from(*seed));
                let checkpoint = self
                    .checkpoints
                    .load(network_id, &keys, &source)
                    .map_err(|_| WalletSecurityPortError::InvalidOperation)?
                    .ok_or(WalletSecurityPortError::InvalidOperation)?;
                if Some(checkpoint.current_cursor) != current.current_cursor()
                    || Some(checkpoint.target_cursor) != current.target_cursor()
                    || checkpoint.current_cursor != checkpoint.target_cursor
                {
                    return Err(WalletSecurityPortError::InvalidOperation);
                }
                plan = Some(build_shielded_transfer(
                    checkpoint.state,
                    &keys,
                    network_id,
                    request,
                ));
                Ok(())
            })
            .map_err(map_security_to_transaction_error)?;
        plan.ok_or(WalletTransactionPortError::InvalidData)?
    }
}

fn ensure_current_shielded_snapshot(
    snapshot: &WalletShieldedSyncSnapshot,
) -> Result<(), oxid_wallet_application::WalletTransactionPortError> {
    if snapshot.state() != WalletShieldedSyncState::Synced
        || snapshot.failure().is_some()
        || snapshot.current_cursor().is_none()
        || snapshot.current_cursor() != snapshot.target_cursor()
        || snapshot.updated_at().is_none()
    {
        return Err(oxid_wallet_application::WalletTransactionPortError::ShieldedStateNotCurrent);
    }
    Ok(())
}

fn build_shielded_transfer(
    mut state: ZswapState<DefaultDB>,
    keys: &ZswapSecretKeys,
    network_id: &ChainNetworkId,
    request: MidnightShieldedTransferRequest,
) -> Result<MidnightShieldedTransferPlan, oxid_wallet_application::WalletTransactionPortError> {
    use oxid_wallet_application::WalletTransactionPortError;

    if request.amount_atomic_units == 0 || request.expires_at_seconds == 0 {
        return Err(WalletTransactionPortError::InvalidData);
    }
    let token_type = ShieldedTokenType(HashOutput(request.token_type));
    let mut owned_nullifiers = state
        .coins
        .iter()
        .map(|(nullifier, _)| nullifier.0.0)
        .collect::<Vec<_>>();
    owned_nullifiers.sort_unstable();
    let mut reservation = Sha256::new();
    reservation.update(b"oxid:midnight:shielded-note-state:v1\0");
    let owned_count = u64::try_from(owned_nullifiers.len())
        .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
    reservation.update(owned_count.to_be_bytes());
    for nullifier in owned_nullifiers {
        reservation.update(nullifier);
    }
    let reservation_fingerprint = reservation.finalize().into();
    let mut selected = Vec::new();
    let mut selected_total = 0_u128;
    for (_, coin) in state.coins.iter() {
        if coin.type_ != token_type {
            continue;
        }
        selected.push(*coin);
        selected_total = selected_total
            .checked_add(coin.value)
            .ok_or(WalletTransactionPortError::InvalidChainState)?;
        if selected_total >= request.amount_atomic_units {
            break;
        }
    }
    if selected_total < request.amount_atomic_units {
        return Err(WalletTransactionPortError::InsufficientFunds);
    }
    let input_count = u16::try_from(selected.len())
        .ok()
        .filter(|count| *count > 0 && *count <= oxid_wallet_domain::MAX_WALLET_TRANSFER_INPUTS)
        .ok_or(WalletTransactionPortError::InvalidData)?;
    let change_atomic_units = selected_total
        .checked_sub(request.amount_atomic_units)
        .ok_or(WalletTransactionPortError::InvalidChainState)?;
    let mut rng = rand::rngs::OsRng;
    let mut inputs = Vec::with_capacity(selected.len());
    for coin in selected {
        let (next, input) = state
            .spend(&mut rng, keys, &coin, None)
            .map_err(|_| WalletTransactionPortError::InvalidChainState)?;
        state = next;
        inputs.push(input);
    }
    let recipient = CoinInfo {
        nonce: rand::random(),
        type_: token_type,
        value: request.amount_atomic_units,
    };
    let mut outputs = vec![
        ZswapOutput::new(
            &mut rng,
            &recipient,
            None,
            &request.recipient_coin_public_key,
            Some(request.recipient_encryption_public_key),
        )
        .map_err(|_| WalletTransactionPortError::InvalidRecipient)?,
    ];
    if change_atomic_units > 0 {
        let change = CoinInfo {
            nonce: rand::random(),
            type_: token_type,
            value: change_atomic_units,
        };
        outputs.push(
            ZswapOutput::new(
                &mut rng,
                &change,
                None,
                &keys.coin_public_key(),
                Some(keys.enc_public_key()),
            )
            .map_err(|_| WalletTransactionPortError::InvalidChainState)?,
        );
    }
    let offer = ZswapOffer::new(inputs, outputs, Vec::new())
        .ok_or(WalletTransactionPortError::InvalidChainState)?;
    let transaction = Transaction::Standard(StandardTransaction::new(
        network_id.as_str(),
        LedgerHashMap::new(),
        Some(offer),
        LedgerHashMap::new(),
    ));
    Ok(MidnightShieldedTransferPlan {
        transaction,
        input_count,
        change_atomic_units,
        reservation_fingerprint,
    })
}

const fn map_security_to_transaction_error(
    error: WalletSecurityPortError,
) -> oxid_wallet_application::WalletTransactionPortError {
    use oxid_wallet_application::WalletTransactionPortError;
    match error {
        WalletSecurityPortError::NotInitialized => {
            WalletTransactionPortError::ProtectionNotInitialized
        }
        WalletSecurityPortError::Locked => WalletTransactionPortError::ProtectionLocked,
        WalletSecurityPortError::Unavailable => WalletTransactionPortError::Unavailable,
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::NotFound
        | WalletSecurityPortError::Conflict
        | WalletSecurityPortError::UnsupportedAlgorithm
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => WalletTransactionPortError::InvalidData,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_live_sync<C, K>(
    config: &MidnightIndexerConfig,
    checkpoints: &dyn MidnightShieldedCheckpointStore,
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
    let path = match shielded_path(account_index) {
        Ok(path) => path,
        Err(_) => {
            finish_with_failure(
                sessions,
                &key,
                cancellation,
                WalletShieldedSyncFailure::InvalidChainState,
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
        Ok(()) => sync_result.unwrap_or(Err(ShieldedTransportError::InvalidData)),
        Err(error) => {
            finish_with_failure(sessions, &key, cancellation, security_failure(error));
            return;
        }
    };
    match result {
        Ok(snapshot) => finish_with_snapshot(sessions, &key, cancellation, snapshot),
        Err(ShieldedTransportError::Cancelled) => {
            finish_cancelled(sessions, &key, cancellation);
        }
        Err(error) => finish_with_failure(sessions, &key, cancellation, sync_failure(error)),
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_live_with_seed<C>(
    config: &MidnightIndexerConfig,
    checkpoints: &dyn MidnightShieldedCheckpointStore,
    clock: &C,
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
    seed: &[u8; 32],
) -> Result<WalletShieldedSyncSnapshot, ShieldedTransportError>
where
    C: ClockPort,
{
    if cancellation.load(Ordering::Acquire) {
        return Err(ShieldedTransportError::Cancelled);
    }
    let zswap_keys = ZswapSecretKeys::from(ZswapSeed::from(*seed));
    let source = source_fingerprint(config.websocket_url());
    let latest = match checkpoints.load(&key.1, &zswap_keys, &source) {
        Ok(checkpoint) => checkpoint,
        Err(ShieldedCheckpointStoreError::InvalidData) => None,
        Err(ShieldedCheckpointStoreError::Unavailable) => {
            return Err(ShieldedTransportError::Storage);
        }
    };
    if let Some(checkpoint) = latest.as_ref() {
        let cached = progress_snapshot(
            &key.1,
            WalletShieldedSyncState::Cached,
            checkpoint.current_cursor,
            checkpoint.target_cursor,
            0,
            &checkpoint.state,
            checkpoint.updated_at,
        )?;
        update_running_snapshot(sessions, key, cancellation, cached)?;
    }
    if cancellation.load(Ordering::Acquire) {
        return Err(ShieldedTransportError::Cancelled);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ShieldedTransportError::Unavailable)?;
    runtime.block_on(async {
        let emitted_progress = Cell::new(false);
        let first = {
            let mut observe = |progress: &ShieldedSyncProgress| {
                emitted_progress.set(true);
                observe_progress(
                    checkpoints,
                    clock,
                    sessions,
                    key,
                    cancellation,
                    &zswap_keys,
                    &source,
                    progress,
                )
            };
            synchronize_shielded_with_control(
                config.websocket_url(),
                &zswap_keys,
                latest.clone(),
                cancellation,
                &mut observe,
            )
            .await
        };
        let synchronized = match first {
            Err(ShieldedTransportError::InvalidData)
                if latest.is_some() && !emitted_progress.get() =>
            {
                let mut observe = |progress: &ShieldedSyncProgress| {
                    observe_progress(
                        checkpoints,
                        clock,
                        sessions,
                        key,
                        cancellation,
                        &zswap_keys,
                        &source,
                        progress,
                    )
                };
                synchronize_shielded_with_control(
                    config.websocket_url(),
                    &zswap_keys,
                    None,
                    cancellation,
                    &mut observe,
                )
                .await?
            }
            Ok(synchronized) => synchronized,
            Err(error) => return Err(error),
        };
        if cancellation.load(Ordering::Acquire) {
            return Err(ShieldedTransportError::Cancelled);
        }
        let updated_at = clock
            .now()
            .map_err(|_| ShieldedTransportError::Unavailable)?;
        progress_snapshot(
            &key.1,
            WalletShieldedSyncState::Synced,
            synchronized.current_cursor,
            synchronized.target_cursor,
            u64::try_from(synchronized.events_processed)
                .map_err(|_| ShieldedTransportError::InvalidData)?,
            &synchronized.state,
            updated_at,
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn observe_progress<C>(
    checkpoints: &dyn MidnightShieldedCheckpointStore,
    clock: &C,
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
    zswap_keys: &ZswapSecretKeys,
    source: &[u8; 32],
    progress: &ShieldedSyncProgress,
) -> Result<(), ShieldedTransportError>
where
    C: ClockPort,
{
    if cancellation.load(Ordering::Acquire) {
        return Err(ShieldedTransportError::Cancelled);
    }
    let updated_at = clock
        .now()
        .map_err(|_| ShieldedTransportError::Unavailable)?;
    checkpoints
        .save(
            &key.1,
            zswap_keys,
            source,
            &StoredShieldedCheckpoint {
                current_cursor: progress.current_cursor,
                target_cursor: progress.target_cursor,
                updated_at,
                state: progress.state.clone(),
            },
        )
        .map_err(|_| ShieldedTransportError::Storage)?;
    let status = progress_snapshot(
        &key.1,
        WalletShieldedSyncState::Syncing,
        progress.current_cursor,
        progress.target_cursor,
        u64::try_from(progress.events_processed)
            .map_err(|_| ShieldedTransportError::InvalidData)?,
        &progress.state,
        updated_at,
    )?;
    update_running_snapshot(sessions, key, cancellation, status)
}

fn update_running_snapshot(
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
    snapshot: WalletShieldedSyncSnapshot,
) -> Result<(), ShieldedTransportError> {
    let mut sessions = sessions
        .lock()
        .map_err(|_| ShieldedTransportError::Unavailable)?;
    let session = sessions
        .get_mut(key)
        .ok_or(ShieldedTransportError::Unavailable)?;
    if !Arc::ptr_eq(&session.cancellation, cancellation) {
        return Err(ShieldedTransportError::InvalidData);
    }
    if cancellation.load(Ordering::Acquire) {
        return Err(ShieldedTransportError::Cancelled);
    }
    session.snapshot = snapshot;
    Ok(())
}

fn finish_with_snapshot(
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
    snapshot: WalletShieldedSyncSnapshot,
) {
    let mut sessions = match sessions.lock() {
        Ok(sessions) => sessions,
        Err(poisoned) => {
            sessions.clear_poison();
            poisoned.into_inner()
        }
    };
    if let Some(session) = sessions.get_mut(key)
        && Arc::ptr_eq(&session.cancellation, cancellation)
    {
        if cancellation.load(Ordering::Acquire) {
            if let Ok(cancelled) = cancelled_snapshot(key, session) {
                session.snapshot = cancelled;
            }
        } else {
            session.snapshot = snapshot;
        }
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
        if let Ok(cancelled) = cancelled_snapshot(key, session) {
            session.snapshot = cancelled;
        }
        session.running = false;
    }
}

fn finish_with_failure(
    sessions: &Arc<Mutex<HashMap<(WalletProfileId, ChainNetworkId), LiveSession>>>,
    key: &(WalletProfileId, ChainNetworkId),
    cancellation: &Arc<AtomicBool>,
    failure: WalletShieldedSyncFailure,
) {
    let mut sessions = match sessions.lock() {
        Ok(sessions) => sessions,
        Err(poisoned) => {
            sessions.clear_poison();
            poisoned.into_inner()
        }
    };
    if let Some(session) = sessions.get_mut(key)
        && Arc::ptr_eq(&session.cancellation, cancellation)
    {
        if cancellation.load(Ordering::Acquire) {
            if let Ok(cancelled) = cancelled_snapshot(key, session) {
                session.snapshot = cancelled;
            }
            session.running = false;
            return;
        }
        let state = match (
            session.snapshot.current_cursor(),
            session.snapshot.target_cursor(),
        ) {
            (Some(current), Some(target)) if current == target => WalletShieldedSyncState::Cached,
            _ => WalletShieldedSyncState::Stalled,
        };
        if let Ok(failed) = snapshot_with_failure(
            key.1.clone(),
            state,
            session.snapshot.current_cursor(),
            session.snapshot.target_cursor(),
            session.snapshot.events_processed(),
            session.snapshot.owned_note_count(),
            session.snapshot.commitment_count(),
            session.snapshot.balances().to_vec(),
            session.snapshot.updated_at(),
            Some(failure),
        ) {
            session.snapshot = failed;
        }
        session.running = false;
    }
}

fn cancelled_snapshot(
    key: &(WalletProfileId, ChainNetworkId),
    session: &LiveSession,
) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
    snapshot(
        key.1.clone(),
        WalletShieldedSyncState::Cancelled,
        session.snapshot.current_cursor(),
        session.snapshot.target_cursor(),
        session.snapshot.events_processed(),
        session.snapshot.owned_note_count(),
        session.snapshot.commitment_count(),
        session.snapshot.balances().to_vec(),
        session.snapshot.updated_at(),
    )
}

fn progress_snapshot(
    network_id: &ChainNetworkId,
    state: WalletShieldedSyncState,
    current_cursor: u64,
    target_cursor: u64,
    events_processed: u64,
    zswap_state: &midnight_zswap::local::State<midnight_storage::DefaultDB>,
    updated_at: oxid_foundation::UnixTimestampMillis,
) -> Result<WalletShieldedSyncSnapshot, ShieldedTransportError> {
    let projection =
        project_zswap_state(zswap_state).map_err(|_| ShieldedTransportError::InvalidData)?;
    let balances = projection
        .balances
        .into_iter()
        .map(|balance| {
            WalletShieldedTokenBalance::new(balance.token_type_hex, balance.atomic_units)
                .map_err(|_| ShieldedTransportError::InvalidData)
        })
        .collect::<Result<Vec<_>, _>>()?;
    snapshot(
        network_id.clone(),
        state,
        Some(current_cursor),
        Some(target_cursor),
        events_processed,
        Some(projection.owned_note_count),
        Some(projection.commitment_count),
        balances,
        Some(updated_at),
    )
    .map_err(map_port_error)
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
    snapshot_with_failure(
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
}

#[allow(clippy::too_many_arguments)]
fn snapshot_with_failure(
    network_id: ChainNetworkId,
    state: WalletShieldedSyncState,
    current_cursor: Option<u64>,
    target_cursor: Option<u64>,
    events_processed: u64,
    owned_note_count: Option<u64>,
    commitment_count: Option<u64>,
    balances: Vec<WalletShieldedTokenBalance>,
    updated_at: Option<oxid_foundation::UnixTimestampMillis>,
    failure: Option<WalletShieldedSyncFailure>,
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
        failure,
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

const fn security_failure(error: WalletSecurityPortError) -> WalletShieldedSyncFailure {
    match error {
        WalletSecurityPortError::NotInitialized => {
            WalletShieldedSyncFailure::ProtectionNotInitialized
        }
        WalletSecurityPortError::Locked => WalletShieldedSyncFailure::ProtectionLocked,
        WalletSecurityPortError::Unavailable => WalletShieldedSyncFailure::TransportUnavailable,
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::NotFound
        | WalletSecurityPortError::Conflict
        | WalletSecurityPortError::UnsupportedAlgorithm
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => WalletShieldedSyncFailure::InvalidChainState,
    }
}

const fn sync_failure(error: ShieldedTransportError) -> WalletShieldedSyncFailure {
    match error {
        ShieldedTransportError::Timeout => WalletShieldedSyncFailure::TimedOut,
        ShieldedTransportError::Unavailable => WalletShieldedSyncFailure::TransportUnavailable,
        ShieldedTransportError::Storage => WalletShieldedSyncFailure::StorageUnavailable,
        ShieldedTransportError::Cancelled => WalletShieldedSyncFailure::TransportUnavailable,
        ShieldedTransportError::InvalidData => WalletShieldedSyncFailure::InvalidChainState,
    }
}

const fn map_port_error(error: WalletShieldedSyncPortError) -> ShieldedTransportError {
    match error {
        WalletShieldedSyncPortError::Unavailable => ShieldedTransportError::Unavailable,
        WalletShieldedSyncPortError::Conflict
        | WalletShieldedSyncPortError::UnsupportedNetwork
        | WalletShieldedSyncPortError::ProtectionNotInitialized
        | WalletShieldedSyncPortError::ProtectionLocked
        | WalletShieldedSyncPortError::InvalidData => ShieldedTransportError::InvalidData,
    }
}

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, sync::atomic::AtomicUsize, thread, time::Duration};

    use futures::{SinkExt as _, StreamExt as _};
    use oxid_foundation::UnixTimestampMillis;
    use oxid_platform_ports::PlatformError;
    use serde_json::{Value, json};
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::{
            Message,
            handshake::server::{Request, Response},
        },
    };

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

    struct PanickingKeys;

    impl WalletDerivedSecretUsePort for PanickingKeys {
        fn use_derived_secret(
            &self,
            _: &WalletProfileId,
            _: &WalletHdPath,
            _: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
        ) -> Result<(), WalletSecurityPortError> {
            panic!("test-only shielded worker panic")
        }
    }

    #[derive(Default)]
    struct RecordingDiagnosticSink {
        events: Mutex<Vec<(DiagnosticCode, DiagnosticSeverity)>>,
    }

    impl DiagnosticEventSinkPort for RecordingDiagnosticSink {
        fn record(&self, code: DiagnosticCode, severity: DiagnosticSeverity) {
            self.events
                .lock()
                .expect("diagnostic event lock")
                .push((code, severity));
        }
    }

    struct MemoryCheckpointStore {
        checkpoint: Mutex<Option<StoredShieldedCheckpoint>>,
        saves: AtomicUsize,
    }

    impl MidnightShieldedCheckpointStore for MemoryCheckpointStore {
        fn load(
            &self,
            _: &ChainNetworkId,
            _: &ZswapSecretKeys,
            _: &[u8; 32],
        ) -> Result<Option<StoredShieldedCheckpoint>, ShieldedCheckpointStoreError> {
            self.checkpoint
                .lock()
                .map_err(|_| ShieldedCheckpointStoreError::Unavailable)
                .map(|checkpoint| checkpoint.clone())
        }

        fn save(
            &self,
            _: &ChainNetworkId,
            _: &ZswapSecretKeys,
            _: &[u8; 32],
            checkpoint: &StoredShieldedCheckpoint,
        ) -> Result<(), ShieldedCheckpointStoreError> {
            *self
                .checkpoint
                .lock()
                .map_err(|_| ShieldedCheckpointStoreError::Unavailable)? = Some(checkpoint.clone());
            self.saves.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    fn profile() -> WalletProfileId {
        WalletProfileId::parse("profile_test").expect("profile is valid")
    }

    fn network() -> ChainNetworkId {
        ChainNetworkId::parse("undeployed").expect("network is valid")
    }

    // Tungstenite fixes the handshake callback's error to a large HTTP response.
    #[allow(clippy::result_large_err)]
    fn current_checkpoint_server() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback binds");
        listener
            .set_nonblocking(true)
            .expect("listener becomes nonblocking");
        let address = listener.local_addr().expect("address exists");
        let handle = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime builds");
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("Tokio listener accepts the socket");
                let (stream, _) = listener.accept().await.expect("client connects");
                let mut socket =
                    accept_hdr_async(stream, |request: &Request, mut response: Response| {
                        assert_eq!(
                            request
                                .headers()
                                .get("Sec-WebSocket-Protocol")
                                .and_then(|value| value.to_str().ok()),
                            Some("graphql-transport-ws")
                        );
                        response.headers_mut().insert(
                            "Sec-WebSocket-Protocol",
                            "graphql-transport-ws".parse().expect("header is valid"),
                        );
                        Ok(response)
                    })
                    .await
                    .expect("WebSocket accepts");
                let _ = socket
                    .next()
                    .await
                    .expect("init exists")
                    .expect("init reads");
                socket
                    .send(Message::Text(
                        json!({ "type": "connection_ack" }).to_string().into(),
                    ))
                    .await
                    .expect("ack sends");
                let subscribe = socket
                    .next()
                    .await
                    .expect("subscribe exists")
                    .expect("subscribe reads");
                let subscribe: Value =
                    serde_json::from_str(subscribe.into_text().expect("text").as_str())
                        .expect("subscribe JSON");
                assert_eq!(subscribe["payload"]["variables"]["id"], 1);
                socket
                    .send(Message::Text(
                        json!({ "type": "complete", "id": "oxid-shielded" })
                            .to_string()
                            .into(),
                    ))
                    .await
                    .expect("completion sends");
                let _ = socket.next().await;
            });
        });
        (format!("ws://{address}/api/v1/graphql/ws"), handle)
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

    #[test]
    fn live_worker_refreshes_a_current_checkpoint_and_publishes_synced_state() {
        const ADDRESS: &str =
            "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
        let (endpoint, server) = current_checkpoint_server();
        let config = MidnightIndexerConfig::new("devnet", endpoint, ADDRESS)
            .expect("live fixture config is valid");
        let network = config.network_id().clone();
        let checkpoints = Arc::new(MemoryCheckpointStore {
            checkpoint: Mutex::new(Some(StoredShieldedCheckpoint {
                current_cursor: 0,
                target_cursor: 0,
                updated_at: UnixTimestampMillis::new(1_699_999_999_000),
                state: midnight_zswap::local::State::new(),
            })),
            saves: AtomicUsize::new(0),
        });
        let checkpoint_adapter: Arc<dyn MidnightShieldedCheckpointStore> = checkpoints.clone();
        let sync = LiveMidnightShieldedSyncController::new(
            config,
            checkpoint_adapter,
            Arc::new(FixedClock),
            Arc::new(AvailableKeys(7)),
        );

        assert_eq!(
            sync.start(&profile(), &network, 7)
                .expect("worker starts")
                .state(),
            WalletShieldedSyncState::Syncing
        );
        let mut final_status = None;
        for _ in 0..100 {
            let status = sync.status(&profile(), &network).expect("status reads");
            if status.state() != WalletShieldedSyncState::Syncing
                && status.state() != WalletShieldedSyncState::Cached
            {
                final_status = Some(status);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        server.join().expect("server exits");
        let final_status = final_status.expect("worker reaches a terminal state");
        assert_eq!(final_status.state(), WalletShieldedSyncState::Synced);
        assert_eq!(final_status.current_cursor(), Some(0));
        assert_eq!(final_status.target_cursor(), Some(0));
        assert_eq!(final_status.events_processed(), 0);
        assert_eq!(final_status.owned_note_count(), Some(0));
        assert_eq!(checkpoints.saves.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn live_worker_panic_becomes_a_terminal_redacted_snapshot() {
        const ADDRESS: &str =
            "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
        let config =
            MidnightIndexerConfig::new("devnet", "ws://127.0.0.1:9/api/v1/graphql/ws", ADDRESS)
                .expect("fixture config");
        let network = config.network_id().clone();
        let checkpoints: Arc<dyn MidnightShieldedCheckpointStore> =
            Arc::new(MemoryCheckpointStore {
                checkpoint: Mutex::new(None),
                saves: AtomicUsize::new(0),
            });
        let sync = LiveMidnightShieldedSyncController::new(
            config,
            checkpoints,
            Arc::new(FixedClock),
            Arc::new(PanickingKeys),
        );
        let diagnostics = Arc::new(RecordingDiagnosticSink::default());
        sync.attach_diagnostic_sink(diagnostics.clone());

        sync.start(&profile(), &network, 7).expect("worker starts");
        let mut terminal = None;
        for _ in 0..100 {
            let status = sync.status(&profile(), &network).expect("status reads");
            if status.state() != WalletShieldedSyncState::Syncing {
                terminal = Some(status);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let terminal = terminal.expect("panic becomes terminal");
        assert_eq!(terminal.state(), WalletShieldedSyncState::Stalled);
        assert_eq!(
            terminal.failure(),
            Some(WalletShieldedSyncFailure::TransportUnavailable)
        );
        assert_eq!(
            diagnostics.events.lock().expect("events").as_slice(),
            &[(
                (DiagnosticCode::MidnightShieldedSyncWorkerPanicked),
                DiagnosticSeverity::Error
            )]
        );
    }
}
