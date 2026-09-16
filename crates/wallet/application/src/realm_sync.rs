// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::{
    ChainNetworkId, WalletAccountSnapshot, WalletAccountSource, WalletDustSyncFailure,
    WalletDustSyncSnapshot, WalletDustSyncState, WalletProfileId, WalletShieldedSyncFailure,
    WalletShieldedSyncSnapshot, WalletShieldedSyncState, WalletSyncState,
};

use crate::{
    WalletAccountPortError, WalletAccountReadPort, WalletAccountView, WalletDustSyncPort,
    WalletDustSyncPortError, WalletDustSyncView, WalletNetworkPort, WalletRealmCoordinatorEffect,
    WalletRealmCoordinatorInput, WalletRealmCoordinatorState, WalletRealmEffectOutcome,
    WalletRealmFacetState, WalletRealmReconciliationCoordinator, WalletRealmReconciliationEffect,
    WalletRealmReconciliationState, WalletRealmReconciliationTrigger, WalletShieldedSyncPort,
    WalletShieldedSyncPortError, WalletShieldedSyncView,
};

/// Profile-scoped command for reconciling the currently selected network realm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedWalletRealmSyncCommand {
    pub profile_id: String,
}

/// A typed family outcome. Absence and temporary failure never masquerade as a
/// zero balance, while a ready view retains the adapter's exact sync state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletRealmFamilyView<T> {
    Ready(T),
    Busy,
    NotFound,
    Unsupported,
    ProtectionNotInitialized,
    ProtectionLocked,
    Unavailable,
    InvalidData,
}

impl<T> WalletRealmFamilyView<T> {
    #[must_use]
    pub const fn state_name(&self) -> &'static str {
        match self {
            Self::Ready(_) => "ready",
            Self::Busy => "busy",
            Self::NotFound => "not_found",
            Self::Unsupported => "unsupported",
            Self::ProtectionNotInitialized => "protection_not_initialized",
            Self::ProtectionLocked => "protection_locked",
            Self::Unavailable => "unavailable",
            Self::InvalidData => "invalid_data",
        }
    }
}

/// One coherent public projection for the selected network realm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedWalletRealmSyncView {
    pub account: WalletRealmFamilyView<WalletAccountView>,
    pub dust: WalletRealmFamilyView<WalletDustSyncView>,
    pub shielded: WalletRealmFamilyView<WalletShieldedSyncView>,
}

/// Validation or public-account failure for selected-realm reconciliation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectedWalletRealmSyncError {
    InvalidProfileIdentifier(OpaqueIdError),
    SelectedNetwork(WalletAccountPortError),
    Unavailable,
}

impl fmt::Display for SelectedWalletRealmSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
            Self::SelectedNetwork(error) => error.fmt(formatter),
            Self::Unavailable => formatter.write_str("selected realm runtime is unavailable"),
        }
    }
}

impl Error for SelectedWalletRealmSyncError {}

pub type SelectedWalletRealmSyncViewFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError>>
            + Send
            + 'a,
    >,
>;

/// Starts one bounded public/DUST/shielded reconciliation for the selected realm.
pub trait SyncSelectedWalletRealmUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> SelectedWalletRealmSyncViewFuture<'_>;
}

/// Reads the most recently published aggregate without starting I/O.
pub trait GetSelectedWalletRealmSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError>;
}

/// Cooperatively cancels the private family workers and returns their state.
pub trait CancelSelectedWalletRealmSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError>;
}

/// Application-owned persistent coordinator registry for selected wallet realms.
/// The owner is intentionally synchronous: adapters decide how and when to run
/// effects, while all lease and stale-completion decisions remain deterministic.
#[derive(Default)]
pub struct SelectedWalletRealmRuntime {
    entries: Vec<SelectedWalletRealmRuntimeEntry>,
}

struct SelectedWalletRealmRuntimeEntry {
    profile: WalletProfileId,
    realm: ChainNetworkId,
    state: WalletRealmCoordinatorState,
}

impl SelectedWalletRealmRuntime {
    pub fn reconcile(
        &mut self,
        profile: WalletProfileId,
        realm: ChainNetworkId,
        observed: WalletRealmReconciliationState,
        trigger: WalletRealmReconciliationTrigger,
    ) -> Vec<WalletRealmCoordinatorEffect> {
        for entry in self
            .entries
            .iter_mut()
            .filter(|entry| entry.profile == profile && entry.realm != realm)
        {
            entry.state = *WalletRealmReconciliationCoordinator::reduce(
                entry.state,
                WalletRealmCoordinatorInput::Cancel,
            )
            .state();
        }
        let entry = if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.profile == profile && entry.realm == realm)
        {
            entry
        } else {
            self.entries.push(SelectedWalletRealmRuntimeEntry {
                profile,
                realm,
                state: WalletRealmCoordinatorState::new(observed),
            });
            self.entries.last_mut().expect("entry inserted")
        };
        let observed = WalletRealmReconciliationCoordinator::reduce(
            entry.state,
            WalletRealmCoordinatorInput::Observe(observed),
        );
        let transition = WalletRealmReconciliationCoordinator::reduce(
            *observed.state(),
            WalletRealmCoordinatorInput::Reconcile(trigger),
        );
        entry.state = *transition.state();
        transition.effects().to_vec()
    }

    pub fn complete(
        &mut self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
        effect: WalletRealmCoordinatorEffect,
        outcome: WalletRealmEffectOutcome,
    ) -> Vec<WalletRealmCoordinatorEffect> {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| &entry.profile == profile && &entry.realm == realm)
        else {
            return Vec::new();
        };
        let transition = WalletRealmReconciliationCoordinator::reduce(
            entry.state,
            WalletRealmCoordinatorInput::EffectCompleted { effect, outcome },
        );
        entry.state = *transition.state();
        transition.effects().to_vec()
    }

    pub fn cancel_profile(&mut self, profile: &WalletProfileId) {
        for entry in self
            .entries
            .iter_mut()
            .filter(|entry| &entry.profile == profile)
        {
            entry.state = *WalletRealmReconciliationCoordinator::reduce(
                entry.state,
                WalletRealmCoordinatorInput::Cancel,
            )
            .state();
        }
    }

    #[must_use]
    pub fn state(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Option<WalletRealmCoordinatorState> {
        self.entries
            .iter()
            .find(|entry| &entry.profile == profile && &entry.realm == realm)
            .map(|entry| entry.state)
    }

    pub fn expire(
        &mut self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
        effect: WalletRealmCoordinatorEffect,
        outcome: WalletRealmEffectOutcome,
    ) -> Vec<WalletRealmCoordinatorEffect> {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| &entry.profile == profile && &entry.realm == realm)
        else {
            return Vec::new();
        };
        let transition = WalletRealmReconciliationCoordinator::reduce(
            entry.state,
            WalletRealmCoordinatorInput::EffectExpired { effect, outcome },
        );
        entry.state = *transition.state();
        transition.effects().to_vec()
    }
}

/// Application orchestration over the three focused wallet ports.
pub struct SelectedWalletRealmSyncService<W> {
    wallet: Arc<W>,
    runtime: std::sync::Mutex<SelectedWalletRealmRuntime>,
}

struct ObservedWalletRealm {
    view: SelectedWalletRealmSyncView,
    state: WalletRealmReconciliationState,
}

impl<W> SelectedWalletRealmSyncService<W> {
    #[must_use]
    pub fn new(wallet: Arc<W>) -> Self {
        Self {
            wallet,
            runtime: std::sync::Mutex::new(SelectedWalletRealmRuntime::default()),
        }
    }

    fn profile(
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<WalletProfileId, SelectedWalletRealmSyncError> {
        WalletProfileId::parse(command.profile_id)
            .map_err(SelectedWalletRealmSyncError::InvalidProfileIdentifier)
    }

    fn observed(&self, profile: &WalletProfileId) -> ObservedWalletRealm
    where
        W: WalletAccountReadPort + WalletDustSyncPort + WalletShieldedSyncPort,
    {
        let (account, account_state) = observe_account(self.wallet.account(profile));
        let (dust, dust_state) = observe_dust(self.wallet.dust_status(profile));
        let (shielded, shielded_state) = observe_shielded(self.wallet.shielded_status(profile));
        ObservedWalletRealm {
            view: SelectedWalletRealmSyncView {
                account,
                dust,
                shielded,
            },
            state: WalletRealmReconciliationState {
                account: account_state,
                dust: dust_state,
                shielded: shielded_state,
            },
        }
    }
}

impl<W> SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort
        + WalletAccountReadPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + 'static,
{
    /// Reconciles the selected realm for one explicit application trigger.
    pub fn reconcile(
        &self,
        command: SelectedWalletRealmSyncCommand,
        trigger: WalletRealmReconciliationTrigger,
    ) -> SelectedWalletRealmSyncViewFuture<'_> {
        Box::pin(async move {
            let profile = Self::profile(command)?;
            let realm = self
                .wallet
                .selected_network(&profile)
                .map_err(SelectedWalletRealmSyncError::SelectedNetwork)?;
            let observed = self.observed(&profile);
            let mut view = observed.view;
            let mut effects = self
                .runtime
                .lock()
                .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
                .reconcile(profile.clone(), realm.clone(), observed.state, trigger);
            while !effects.is_empty() {
                let effect = effects.remove(0);
                let outcome = match effect.kind() {
                    WalletRealmReconciliationEffect::SyncAccount => {
                        let (family, state) = observe_account(self.wallet.sync(&profile).await);
                        view.account = family;
                        effect_outcome(state)
                    }
                    WalletRealmReconciliationEffect::SyncDust => {
                        let (family, state) = observe_dust(self.wallet.start_dust_sync(&profile));
                        view.dust = family;
                        effect_outcome(state)
                    }
                    WalletRealmReconciliationEffect::SyncShielded => {
                        let (family, state) =
                            observe_shielded(self.wallet.start_shielded_sync(&profile));
                        view.shielded = family;
                        effect_outcome(state)
                    }
                };
                let follow_up = self
                    .runtime
                    .lock()
                    .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
                    .complete(&profile, &realm, effect, outcome);
                effects.extend(follow_up);
            }
            Ok(view)
        })
    }
}

impl<W> SyncSelectedWalletRealmUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort
        + WalletAccountReadPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> SelectedWalletRealmSyncViewFuture<'_> {
        self.reconcile(command, WalletRealmReconciliationTrigger::ManualRefresh)
    }
}

impl<W> GetSelectedWalletRealmSyncUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletAccountReadPort + WalletDustSyncPort + WalletShieldedSyncPort + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError> {
        Ok(self.observed(&Self::profile(command)?).view)
    }
}

impl<W> CancelSelectedWalletRealmSyncUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletAccountReadPort + WalletDustSyncPort + WalletShieldedSyncPort + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError> {
        let profile = Self::profile(command)?;
        self.runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
            .cancel_profile(&profile);
        let account = self
            .wallet
            .account(&profile)
            .map(|snapshot| {
                WalletRealmFamilyView::Ready(WalletAccountView::from_snapshot(&snapshot))
            })
            .unwrap_or_else(account_family_error);
        let dust = self
            .wallet
            .cancel_dust_sync(&profile)
            .map(|snapshot| WalletRealmFamilyView::Ready(WalletDustSyncView::from(&snapshot)))
            .unwrap_or_else(dust_family_error);
        let shielded = self
            .wallet
            .cancel_shielded_sync(&profile)
            .map(|snapshot| WalletRealmFamilyView::Ready(WalletShieldedSyncView::from(&snapshot)))
            .unwrap_or_else(shielded_family_error);
        Ok(SelectedWalletRealmSyncView {
            account,
            dust,
            shielded,
        })
    }
}

fn observe_account(
    result: Result<WalletAccountSnapshot, WalletAccountPortError>,
) -> (
    WalletRealmFamilyView<WalletAccountView>,
    WalletRealmFacetState,
) {
    match result {
        Ok(snapshot) => {
            let state = match snapshot.sync().state() {
                WalletSyncState::Synced if snapshot.source() == WalletAccountSource::Live => {
                    WalletRealmFacetState::Current
                }
                WalletSyncState::Syncing => WalletRealmFacetState::Updating,
                WalletSyncState::NeverSynced => WalletRealmFacetState::Missing,
                WalletSyncState::Synced
                | WalletSyncState::Stalled
                | WalletSyncState::Unavailable => WalletRealmFacetState::Stale,
            };
            (
                WalletRealmFamilyView::Ready(WalletAccountView::from_snapshot(&snapshot)),
                state,
            )
        }
        Err(error) => (account_family_error(error), account_error_state(error)),
    }
}

fn observe_dust(
    result: Result<WalletDustSyncSnapshot, WalletDustSyncPortError>,
) -> (
    WalletRealmFamilyView<WalletDustSyncView>,
    WalletRealmFacetState,
) {
    match result {
        Ok(snapshot) => {
            let state = snapshot.failure().map_or_else(
                || match snapshot.state() {
                    WalletDustSyncState::Synced => WalletRealmFacetState::Current,
                    WalletDustSyncState::Syncing => WalletRealmFacetState::Updating,
                    WalletDustSyncState::NeverSynced => WalletRealmFacetState::Missing,
                    WalletDustSyncState::Cached
                    | WalletDustSyncState::Cancelled
                    | WalletDustSyncState::Stalled
                    | WalletDustSyncState::Unavailable => WalletRealmFacetState::Stale,
                },
                dust_failure_state,
            );
            (
                WalletRealmFamilyView::Ready(WalletDustSyncView::from(&snapshot)),
                state,
            )
        }
        Err(error) => (dust_family_error(error), dust_error_state(error)),
    }
}

fn observe_shielded(
    result: Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError>,
) -> (
    WalletRealmFamilyView<WalletShieldedSyncView>,
    WalletRealmFacetState,
) {
    match result {
        Ok(snapshot) => {
            let state = snapshot.failure().map_or_else(
                || match snapshot.state() {
                    WalletShieldedSyncState::Synced => WalletRealmFacetState::Current,
                    WalletShieldedSyncState::Syncing => WalletRealmFacetState::Updating,
                    WalletShieldedSyncState::NeverSynced => WalletRealmFacetState::Missing,
                    WalletShieldedSyncState::Cached
                    | WalletShieldedSyncState::Cancelled
                    | WalletShieldedSyncState::Stalled
                    | WalletShieldedSyncState::Unavailable => WalletRealmFacetState::Stale,
                },
                shielded_failure_state,
            );
            (
                WalletRealmFamilyView::Ready(WalletShieldedSyncView::from(&snapshot)),
                state,
            )
        }
        Err(error) => (shielded_family_error(error), shielded_error_state(error)),
    }
}

const fn dust_failure_state(failure: WalletDustSyncFailure) -> WalletRealmFacetState {
    match failure {
        WalletDustSyncFailure::ProtectionNotInitialized
        | WalletDustSyncFailure::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletDustSyncFailure::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletDustSyncFailure::TransportUnavailable
        | WalletDustSyncFailure::TimedOut
        | WalletDustSyncFailure::InvalidChainState
        | WalletDustSyncFailure::StorageUnavailable => WalletRealmFacetState::Stale,
    }
}

const fn shielded_failure_state(failure: WalletShieldedSyncFailure) -> WalletRealmFacetState {
    match failure {
        WalletShieldedSyncFailure::ProtectionNotInitialized
        | WalletShieldedSyncFailure::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletShieldedSyncFailure::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletShieldedSyncFailure::TransportUnavailable
        | WalletShieldedSyncFailure::TimedOut
        | WalletShieldedSyncFailure::InvalidChainState
        | WalletShieldedSyncFailure::StorageUnavailable => WalletRealmFacetState::Stale,
    }
}

const fn account_error_state(error: WalletAccountPortError) -> WalletRealmFacetState {
    match error {
        WalletAccountPortError::NotFound => WalletRealmFacetState::Missing,
        WalletAccountPortError::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletAccountPortError::ProtectionNotInitialized
        | WalletAccountPortError::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletAccountPortError::Unavailable | WalletAccountPortError::InvalidData => {
            WalletRealmFacetState::Stale
        }
    }
}

const fn dust_error_state(error: WalletDustSyncPortError) -> WalletRealmFacetState {
    match error {
        WalletDustSyncPortError::Conflict => WalletRealmFacetState::Updating,
        WalletDustSyncPortError::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletDustSyncPortError::ProtectionNotInitialized
        | WalletDustSyncPortError::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletDustSyncPortError::Unavailable | WalletDustSyncPortError::InvalidData => {
            WalletRealmFacetState::Stale
        }
    }
}

const fn shielded_error_state(error: WalletShieldedSyncPortError) -> WalletRealmFacetState {
    match error {
        WalletShieldedSyncPortError::Conflict => WalletRealmFacetState::Updating,
        WalletShieldedSyncPortError::UnsupportedNetwork => WalletRealmFacetState::Unsupported,
        WalletShieldedSyncPortError::ProtectionNotInitialized
        | WalletShieldedSyncPortError::ProtectionLocked => WalletRealmFacetState::Blocked,
        WalletShieldedSyncPortError::Unavailable | WalletShieldedSyncPortError::InvalidData => {
            WalletRealmFacetState::Stale
        }
    }
}

const fn effect_outcome(state: WalletRealmFacetState) -> WalletRealmEffectOutcome {
    match state {
        WalletRealmFacetState::Current => WalletRealmEffectOutcome::Current,
        WalletRealmFacetState::Missing => WalletRealmEffectOutcome::Missing,
        WalletRealmFacetState::Blocked => WalletRealmEffectOutcome::Blocked,
        WalletRealmFacetState::Unsupported => WalletRealmEffectOutcome::Unsupported,
        WalletRealmFacetState::Stale | WalletRealmFacetState::Updating => {
            WalletRealmEffectOutcome::Stale
        }
    }
}

const fn account_family_error(
    error: WalletAccountPortError,
) -> WalletRealmFamilyView<WalletAccountView> {
    match error {
        WalletAccountPortError::NotFound => WalletRealmFamilyView::NotFound,
        WalletAccountPortError::UnsupportedNetwork => WalletRealmFamilyView::Unsupported,
        WalletAccountPortError::ProtectionNotInitialized => {
            WalletRealmFamilyView::ProtectionNotInitialized
        }
        WalletAccountPortError::ProtectionLocked => WalletRealmFamilyView::ProtectionLocked,
        WalletAccountPortError::Unavailable => WalletRealmFamilyView::Unavailable,
        WalletAccountPortError::InvalidData => WalletRealmFamilyView::InvalidData,
    }
}

const fn dust_family_error(
    error: WalletDustSyncPortError,
) -> WalletRealmFamilyView<WalletDustSyncView> {
    match error {
        WalletDustSyncPortError::Conflict => WalletRealmFamilyView::Busy,
        WalletDustSyncPortError::UnsupportedNetwork => WalletRealmFamilyView::Unsupported,
        WalletDustSyncPortError::ProtectionNotInitialized => {
            WalletRealmFamilyView::ProtectionNotInitialized
        }
        WalletDustSyncPortError::ProtectionLocked => WalletRealmFamilyView::ProtectionLocked,
        WalletDustSyncPortError::Unavailable => WalletRealmFamilyView::Unavailable,
        WalletDustSyncPortError::InvalidData => WalletRealmFamilyView::InvalidData,
    }
}

const fn shielded_family_error(
    error: WalletShieldedSyncPortError,
) -> WalletRealmFamilyView<WalletShieldedSyncView> {
    match error {
        WalletShieldedSyncPortError::Conflict => WalletRealmFamilyView::Busy,
        WalletShieldedSyncPortError::UnsupportedNetwork => WalletRealmFamilyView::Unsupported,
        WalletShieldedSyncPortError::ProtectionNotInitialized => {
            WalletRealmFamilyView::ProtectionNotInitialized
        }
        WalletShieldedSyncPortError::ProtectionLocked => WalletRealmFamilyView::ProtectionLocked,
        WalletShieldedSyncPortError::Unavailable => WalletRealmFamilyView::Unavailable,
        WalletShieldedSyncPortError::InvalidData => WalletRealmFamilyView::InvalidData,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Poll, Waker},
    };

    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{
        ChainKind, ChainNetwork, ChainNetworkId, NetworkDisplayName, NetworkEnvironment,
        WalletAccountSnapshot, WalletDustSyncFailure, WalletDustSyncSnapshot,
        WalletShieldedSyncFailure, WalletShieldedSyncSnapshot,
    };

    use super::*;

    #[derive(Default)]
    struct PartialWallet {
        account_syncs: AtomicUsize,
        dust_starts: AtomicUsize,
        shielded_starts: AtomicUsize,
    }

    impl WalletNetworkPort for PartialWallet {
        fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
            Ok(vec![network()])
        }

        fn selected_network(
            &self,
            _: &WalletProfileId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            Ok(network_id())
        }

        fn select_network(
            &self,
            _: &WalletProfileId,
            network_id: &ChainNetworkId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            Ok(network_id.clone())
        }
    }

    impl WalletAccountReadPort for PartialWallet {
        fn account(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            Ok(WalletAccountSnapshot::unavailable(network()))
        }

        fn sync<'a>(&'a self, _: &'a WalletProfileId) -> crate::WalletAccountPortFuture<'a> {
            self.account_syncs.fetch_add(1, Ordering::Relaxed);
            Box::pin(async { Ok(WalletAccountSnapshot::unavailable(network())) })
        }
    }

    impl WalletDustSyncPort for PartialWallet {
        fn dust_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::ProtectionLocked)
        }

        fn start_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_starts.fetch_add(1, Ordering::Relaxed);
            Err(WalletDustSyncPortError::ProtectionLocked)
        }

        fn cancel_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::ProtectionLocked)
        }
    }

    impl WalletShieldedSyncPort for PartialWallet {
        fn shielded_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Ok(WalletShieldedSyncSnapshot::never_synced(network_id()))
        }

        fn start_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_starts.fetch_add(1, Ordering::Relaxed);
            Ok(WalletShieldedSyncSnapshot::never_synced(network_id()))
        }

        fn cancel_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Err(WalletShieldedSyncPortError::Unavailable)
        }
    }

    #[derive(Default)]
    struct PendingAccountWallet {
        account_syncs: AtomicUsize,
    }

    impl WalletNetworkPort for PendingAccountWallet {
        fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
            Ok(vec![network()])
        }

        fn selected_network(
            &self,
            _: &WalletProfileId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            Ok(network_id())
        }

        fn select_network(
            &self,
            _: &WalletProfileId,
            network_id: &ChainNetworkId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            Ok(network_id.clone())
        }
    }

    impl WalletAccountReadPort for PendingAccountWallet {
        fn account(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            Ok(WalletAccountSnapshot::unavailable(network()))
        }

        fn sync<'a>(&'a self, _: &'a WalletProfileId) -> crate::WalletAccountPortFuture<'a> {
            self.account_syncs.fetch_add(1, Ordering::Relaxed);
            Box::pin(async {
                std::future::pending::<()>().await;
                unreachable!("pending fixture never completes")
            })
        }
    }

    impl WalletDustSyncPort for PendingAccountWallet {
        fn dust_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::UnsupportedNetwork)
        }

        fn start_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::UnsupportedNetwork)
        }

        fn cancel_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::UnsupportedNetwork)
        }
    }

    impl WalletShieldedSyncPort for PendingAccountWallet {
        fn shielded_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Err(WalletShieldedSyncPortError::UnsupportedNetwork)
        }

        fn start_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Err(WalletShieldedSyncPortError::UnsupportedNetwork)
        }

        fn cancel_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Err(WalletShieldedSyncPortError::UnsupportedNetwork)
        }
    }

    fn network_id() -> ChainNetworkId {
        ChainNetworkId::parse("undeployed").expect("network id is valid")
    }

    fn network() -> ChainNetwork {
        ChainNetwork::new(
            ChainKind::Midnight,
            network_id(),
            NetworkDisplayName::parse("Standalone").expect("network name is valid"),
            NetworkEnvironment::Development,
        )
    }

    fn command() -> SelectedWalletRealmSyncCommand {
        SelectedWalletRealmSyncCommand {
            profile_id: "profile_test".to_owned(),
        }
    }

    fn resolve<T>(future: impl Future<Output = T>) -> T {
        let mut future = Box::pin(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("fixture future must resolve immediately"),
        }
    }

    #[test]
    fn runtime_isolates_profiles_and_invalidates_previous_realm_leases() {
        let mut runtime = SelectedWalletRealmRuntime::default();
        let stale = WalletRealmReconciliationState {
            account: WalletRealmFacetState::Stale,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Current,
        };
        let profile_a = WalletProfileId::parse("profile_a").expect("profile id");
        let profile_b = WalletProfileId::parse("profile_b").expect("profile id");
        let first = runtime.reconcile(
            profile_a.clone(),
            network_id(),
            stale,
            WalletRealmReconciliationTrigger::Initial,
        );
        assert_eq!(first.len(), 1);
        assert!(
            runtime
                .complete(
                    &profile_a,
                    &network_id(),
                    first[0],
                    WalletRealmEffectOutcome::Current,
                )
                .is_empty()
        );
        let current = WalletRealmReconciliationState {
            account: WalletRealmFacetState::Current,
            dust: WalletRealmFacetState::Current,
            shielded: WalletRealmFacetState::Current,
        };
        let repeated = runtime.reconcile(
            profile_a.clone(),
            network_id(),
            current,
            WalletRealmReconciliationTrigger::ManualRefresh,
        );
        assert_eq!(repeated.len(), 3);
        assert_eq!(
            runtime
                .state(&profile_a, &network_id())
                .expect("same-key state persists")
                .revision(),
            2
        );
        let other = runtime.reconcile(
            profile_b,
            network_id(),
            stale,
            WalletRealmReconciliationTrigger::Initial,
        );
        assert_eq!(other.len(), 1);
        let switched = runtime.reconcile(
            profile_a.clone(),
            ChainNetworkId::parse("preprod").expect("network id"),
            stale,
            WalletRealmReconciliationTrigger::Initial,
        );
        assert_eq!(switched.len(), 1);
        assert!(
            runtime
                .complete(
                    &profile_a,
                    &network_id(),
                    repeated[0],
                    WalletRealmEffectOutcome::Current
                )
                .is_empty()
        );
    }

    #[test]
    fn service_keeps_one_in_flight_family_effect_and_cancel_releases_its_lease() {
        let wallet = Arc::new(PendingAccountWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let mut first = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 1);

        let duplicate = resolve(
            service.reconcile(command(), WalletRealmReconciliationTrigger::ActionPreflight),
        )
        .expect("duplicate reconciliation returns its observation");
        assert!(matches!(duplicate.account, WalletRealmFamilyView::Ready(_)));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 1);

        drop(first);
        CancelSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("cancellation invalidates the application lease");
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let canceled = service
            .runtime
            .lock()
            .expect("runtime lock")
            .state(&profile, &network_id())
            .expect("runtime state");
        assert_eq!(canceled.revision(), 2);

        let mut restarted = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        assert!(matches!(
            restarted.as_mut().poll(&mut context),
            Poll::Pending
        ));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn aggregate_preserves_partial_family_failures_as_typed_state() {
        let wallet = Arc::new(PartialWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let started = resolve(SyncSelectedWalletRealmUseCase::execute(&service, command()))
            .expect("aggregate starts");
        assert!(matches!(started.account, WalletRealmFamilyView::Ready(_)));
        assert_eq!(started.dust, WalletRealmFamilyView::ProtectionLocked);
        assert!(matches!(
            started.shielded,
            WalletRealmFamilyView::Ready(WalletShieldedSyncView { ref state, .. })
                if state == "never_synced"
        ));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 1);
        assert_eq!(wallet.dust_starts.load(Ordering::Relaxed), 1);
        assert_eq!(wallet.shielded_starts.load(Ordering::Relaxed), 1);
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let state = service
            .runtime
            .lock()
            .expect("runtime lock")
            .state(&profile, &network_id())
            .expect("runtime state");
        assert_eq!(
            state.facets(),
            WalletRealmReconciliationState {
                account: WalletRealmFacetState::Stale,
                dust: WalletRealmFacetState::Blocked,
                shielded: WalletRealmFacetState::Missing,
            }
        );

        let cancelled = CancelSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("aggregate cancellation reports every family");
        assert_eq!(cancelled.dust, WalletRealmFamilyView::ProtectionLocked);
        assert_eq!(cancelled.shielded, WalletRealmFamilyView::Unavailable);
    }

    #[test]
    fn aggregate_rejects_an_invalid_profile_before_calling_ports() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet::default()));
        assert!(matches!(
            GetSelectedWalletRealmSyncUseCase::execute(
                &service,
                SelectedWalletRealmSyncCommand {
                    profile_id: " invalid".to_owned(),
                },
            ),
            Err(SelectedWalletRealmSyncError::InvalidProfileIdentifier(_))
        ));
    }

    #[test]
    fn published_failures_classify_blocked_or_unsupported_reconciliation() {
        let dust = WalletDustSyncSnapshot::new(
            network_id(),
            WalletDustSyncState::Cached,
            Some(4),
            Some(9),
            3,
            Some(42),
            Some(UnixTimestampMillis::new(42)),
            Some(WalletDustSyncFailure::ProtectionLocked),
        )
        .expect("cached DUST fixture is valid");
        let (_, dust_state) = observe_dust(Ok(dust));
        assert_eq!(dust_state, WalletRealmFacetState::Blocked);

        let shielded = WalletShieldedSyncSnapshot::new(
            network_id(),
            WalletShieldedSyncState::Stalled,
            Some(4),
            Some(9),
            3,
            Some(1),
            Some(2),
            Vec::new(),
            Some(UnixTimestampMillis::new(42)),
            Some(WalletShieldedSyncFailure::UnsupportedNetwork),
        )
        .expect("stalled shielded fixture is valid");
        let (_, shielded_state) = observe_shielded(Ok(shielded));
        assert_eq!(shielded_state, WalletRealmFacetState::Unsupported);
    }
}
