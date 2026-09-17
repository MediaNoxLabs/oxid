// SPDX-License-Identifier: Apache-2.0

use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::{
    ChainNetworkId, WalletAccountSnapshot, WalletAccountSource, WalletDustSyncFailure,
    WalletDustSyncSnapshot, WalletDustSyncState, WalletProfileId, WalletShieldedSyncFailure,
    WalletShieldedSyncSnapshot, WalletShieldedSyncState, WalletSyncState,
};

use crate::{
    WalletAccountPortError, WalletAccountReadPort, WalletAccountView, WalletDustSyncPort,
    WalletDustSyncPortError, WalletDustSyncView, WalletNetworkPort, WalletNetworkSelectionObserver,
    WalletRealmCoordinatorEffect, WalletRealmCoordinatorInput, WalletRealmCoordinatorState,
    WalletRealmEffectOutcome, WalletRealmFacetState, WalletRealmReconciliationCoordinator,
    WalletRealmReconciliationEffect, WalletRealmReconciliationState,
    WalletRealmReconciliationTrigger, WalletShieldedSyncPort, WalletShieldedSyncPortError,
    WalletShieldedSyncView,
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

/// Typed identity and monotonically increasing revision of a selected realm observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedWalletRealmIdentity {
    pub profile: WalletProfileId,
    pub realm: ChainNetworkId,
}

/// Whether an observation can be used for a new user action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedWalletRealmActionReadiness {
    Ready,
    Refreshing,
    Unavailable,
}

/// Presentation-neutral observation policy for a projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedWalletRealmObservation {
    Settled,
    PollAfter(Duration),
}

impl SelectedWalletRealmObservation {
    #[must_use]
    pub const fn poll_after(self) -> Option<Duration> {
        match self {
            Self::Settled => None,
            Self::PollAfter(duration) => Some(duration),
        }
    }
}

/// The application-owned query observation consumed by every selected-realm client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedWalletRealmProjection {
    pub identity: SelectedWalletRealmIdentity,
    pub revision: u64,
    pub fresh: bool,
    pub consistent: bool,
    pub actionable: SelectedWalletRealmActionReadiness,
    pub observation: SelectedWalletRealmObservation,
    pub view: SelectedWalletRealmSyncView,
}

impl SelectedWalletRealmProjection {
    /// Rejects an observation belonging to another selected realm or an older revision.
    #[must_use]
    pub fn supersedes(&self, previous: &Self) -> bool {
        self.identity == previous.identity && self.revision >= previous.revision
    }
}

/// Validation or public-account failure for selected-realm reconciliation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectedWalletRealmSyncError {
    InvalidProfileIdentifier(OpaqueIdError),
    SelectedNetwork(WalletAccountPortError),
    SelectionChanged,
    ObservationSuperseded,
    Unavailable,
}

impl fmt::Display for SelectedWalletRealmSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
            Self::SelectedNetwork(error) => error.fmt(formatter),
            Self::SelectionChanged => {
                formatter.write_str("selected wallet realm changed during synchronization")
            }
            Self::ObservationSuperseded => {
                formatter.write_str("selected wallet realm observation was superseded")
            }
            Self::Unavailable => formatter.write_str("selected realm runtime is unavailable"),
        }
    }
}

impl Error for SelectedWalletRealmSyncError {}

pub type SelectedWalletRealmProjectionFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError>>
            + Send
            + 'a,
    >,
>;

/// Starts one bounded public/DUST/shielded reconciliation for the selected realm.
pub trait SyncSelectedWalletRealmUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> SelectedWalletRealmProjectionFuture<'_>;
}

/// Reads the most recently published aggregate without starting I/O.
pub trait GetSelectedWalletRealmSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError>;
}

/// Cooperatively cancels the private family workers and returns their state.
pub trait CancelSelectedWalletRealmSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError>;
}

/// Application-owned persistent coordinator registry for selected wallet realms.
/// The owner is intentionally synchronous: adapters decide how and when to run
/// effects, while all lease and stale-completion decisions remain deterministic.
#[derive(Default)]
pub struct SelectedWalletRealmRuntime {
    entries: Vec<SelectedWalletRealmRuntimeEntry>,
    selections: Vec<SelectedWalletRealmSelection>,
    observations: Vec<SelectedWalletRealmObservationGeneration>,
    projections: Vec<SelectedWalletRealmPublishedProjection>,
}

struct SelectedWalletRealmRuntimeEntry {
    profile: WalletProfileId,
    realm: ChainNetworkId,
    state: WalletRealmCoordinatorState,
}

struct SelectedWalletRealmSelection {
    profile: WalletProfileId,
    realm: ChainNetworkId,
}

#[derive(Clone)]
struct SelectedWalletRealmPublishedProjection {
    profile: WalletProfileId,
    realm: ChainNetworkId,
    revision: u64,
    view: SelectedWalletRealmSyncView,
}

struct SelectedWalletRealmObservationGeneration {
    profile: WalletProfileId,
    realm: ChainNetworkId,
    generation: u64,
}

impl SelectedWalletRealmRuntime {
    pub fn reconcile(
        &mut self,
        profile: WalletProfileId,
        realm: ChainNetworkId,
        observed: WalletRealmReconciliationState,
        trigger: WalletRealmReconciliationTrigger,
    ) -> Vec<WalletRealmCoordinatorEffect> {
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

    pub fn retire_other_realms(
        &mut self,
        profile: &WalletProfileId,
        selected_realm: &ChainNetworkId,
    ) -> Vec<ChainNetworkId> {
        let Some(previous) = self.observe_selected_realm(profile, selected_realm) else {
            return Vec::new();
        };
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| &entry.profile == profile && entry.realm == previous)
        {
            entry.state = *WalletRealmReconciliationCoordinator::reduce(
                entry.state,
                WalletRealmCoordinatorInput::Cancel,
            )
            .state();
        }
        vec![previous]
    }

    fn observe_selected_realm(
        &mut self,
        profile: &WalletProfileId,
        selected_realm: &ChainNetworkId,
    ) -> Option<ChainNetworkId> {
        let previous = if let Some(selection) = self
            .selections
            .iter_mut()
            .find(|selection| &selection.profile == profile)
        {
            if &selection.realm == selected_realm {
                return None;
            }
            Some(std::mem::replace(
                &mut selection.realm,
                selected_realm.clone(),
            ))
        } else {
            self.selections.push(SelectedWalletRealmSelection {
                profile: profile.clone(),
                realm: selected_realm.clone(),
            });
            None
        };
        let previous = previous?;
        self.invalidate_projection(profile, &previous);
        self.invalidate_projection(profile, selected_realm);
        Some(previous)
    }

    fn invalidate_projection(&mut self, profile: &WalletProfileId, realm: &ChainNetworkId) {
        if let Some(projection) = self
            .projections
            .iter_mut()
            .find(|projection| &projection.profile == profile && &projection.realm == realm)
        {
            projection.revision = projection.revision.saturating_add(1);
        }
        self.advance_observation_generation(profile, realm);
    }

    fn advance_observation_generation(
        &mut self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> u64 {
        if let Some(observation) = self
            .observations
            .iter_mut()
            .find(|observation| &observation.profile == profile && &observation.realm == realm)
        {
            observation.generation = observation.generation.saturating_add(1);
            return observation.generation;
        }
        self.observations
            .push(SelectedWalletRealmObservationGeneration {
                profile: profile.clone(),
                realm: realm.clone(),
                generation: 1,
            });
        1
    }

    fn begin_observation(
        &mut self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Option<u64> {
        match self
            .selections
            .iter()
            .find(|selection| &selection.profile == profile)
        {
            Some(selection) if &selection.realm != realm => None,
            Some(_) => Some(self.advance_observation_generation(profile, realm)),
            None => {
                self.selections.push(SelectedWalletRealmSelection {
                    profile: profile.clone(),
                    realm: realm.clone(),
                });
                Some(self.advance_observation_generation(profile, realm))
            }
        }
    }

    fn selection_matches(&self, profile: &WalletProfileId, realm: &ChainNetworkId) -> bool {
        self.selections
            .iter()
            .find(|selection| &selection.profile == profile)
            .is_some_and(|selection| &selection.realm == realm)
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
    pub fn effect_active(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
        effect: WalletRealmCoordinatorEffect,
    ) -> bool {
        self.state(profile, realm)
            .is_some_and(|state| state.accepts(effect))
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

    #[must_use]
    pub fn publish(
        &mut self,
        profile: WalletProfileId,
        realm: ChainNetworkId,
        observation_generation: u64,
        view: &SelectedWalletRealmSyncView,
    ) -> Option<u64> {
        if !self.observations.iter().any(|observation| {
            observation.profile == profile
                && observation.realm == realm
                && observation.generation == observation_generation
        }) {
            return None;
        }
        if let Some(projection) = self
            .projections
            .iter_mut()
            .find(|projection| projection.profile == profile && projection.realm == realm)
        {
            if &projection.view != view {
                projection.revision = projection.revision.saturating_add(1);
                projection.view.clone_from(view);
            }
            return Some(projection.revision);
        }
        self.projections
            .push(SelectedWalletRealmPublishedProjection {
                profile,
                realm,
                revision: 1,
                view: view.clone(),
            });
        Some(1)
    }

    fn published(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Option<SelectedWalletRealmPublishedProjection> {
        self.projections
            .iter()
            .find(|projection| &projection.profile == profile && &projection.realm == realm)
            .cloned()
    }

    pub fn expire(
        &mut self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
        effect: WalletRealmCoordinatorEffect,
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
            WalletRealmCoordinatorInput::EffectExpired(effect),
        );
        entry.state = *transition.state();
        transition.effects().to_vec()
    }
}

/// Application orchestration over the three focused wallet ports.
pub struct SelectedWalletRealmSyncService<W> {
    wallet: Arc<W>,
    runtime: Arc<Mutex<SelectedWalletRealmRuntime>>,
    operation_gate: Mutex<()>,
}

struct ObservedWalletRealm {
    view: SelectedWalletRealmSyncView,
    state: WalletRealmReconciliationState,
}

struct WalletRealmLeaseGuard<'a> {
    runtime: &'a Mutex<SelectedWalletRealmRuntime>,
    profile: WalletProfileId,
    realm: ChainNetworkId,
    active: Vec<WalletRealmCoordinatorEffect>,
}

impl WalletRealmLeaseGuard<'_> {
    fn effect_active(
        &self,
        effect: WalletRealmCoordinatorEffect,
    ) -> Result<bool, SelectedWalletRealmSyncError> {
        self.runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)
            .map(|runtime| runtime.effect_active(&self.profile, &self.realm, effect))
    }

    fn complete(
        &mut self,
        effect: WalletRealmCoordinatorEffect,
        outcome: WalletRealmEffectOutcome,
    ) -> Result<Vec<WalletRealmCoordinatorEffect>, SelectedWalletRealmSyncError> {
        let follow_up = self
            .runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
            .complete(&self.profile, &self.realm, effect, outcome);
        self.active.retain(|candidate| *candidate != effect);
        self.active.extend(follow_up.iter().copied());
        Ok(follow_up)
    }

    fn discard(&mut self, effect: WalletRealmCoordinatorEffect) {
        self.active.retain(|candidate| *candidate != effect);
    }
}

impl Drop for WalletRealmLeaseGuard<'_> {
    fn drop(&mut self) {
        let Ok(mut runtime) = self.runtime.lock() else {
            return;
        };
        while let Some(effect) = self.active.pop() {
            let follow_up = runtime.expire(&self.profile, &self.realm, effect);
            self.active.extend(follow_up);
        }
    }
}

impl<W> SelectedWalletRealmSyncService<W> {
    #[must_use]
    pub fn new(wallet: Arc<W>) -> Self {
        Self::with_runtime(
            wallet,
            Arc::new(Mutex::new(SelectedWalletRealmRuntime::default())),
        )
    }

    #[must_use]
    pub fn with_runtime(wallet: Arc<W>, runtime: Arc<Mutex<SelectedWalletRealmRuntime>>) -> Self {
        Self {
            wallet,
            runtime,
            operation_gate: Mutex::new(()),
        }
    }

    fn profile(
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<WalletProfileId, SelectedWalletRealmSyncError> {
        WalletProfileId::parse(command.profile_id)
            .map_err(SelectedWalletRealmSyncError::InvalidProfileIdentifier)
    }

    fn begin_observation(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Result<u64, SelectedWalletRealmSyncError> {
        self.runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)
            .and_then(|mut runtime| {
                runtime
                    .begin_observation(profile, realm)
                    .ok_or(SelectedWalletRealmSyncError::SelectionChanged)
            })
    }

    fn projection(
        &self,
        profile: WalletProfileId,
        realm: ChainNetworkId,
        observation_generation: u64,
        view: SelectedWalletRealmSyncView,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError>
    where
        W: WalletNetworkPort,
    {
        self.ensure_selected_realm(&profile, &realm)?;
        let published = {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
            if !runtime.selection_matches(&profile, &realm) {
                return Err(SelectedWalletRealmSyncError::SelectionChanged);
            }
            let _ = runtime.publish(
                profile.clone(),
                realm.clone(),
                observation_generation,
                &view,
            );
            runtime.published(&profile, &realm)
        };
        let published = published.ok_or(SelectedWalletRealmSyncError::ObservationSuperseded)?;
        let view = published.view;
        let fresh = selected_realm_is_fresh(&view);
        let consistent = selected_realm_is_consistent(&view);
        let refreshing = selected_realm_is_refreshing(&view);
        Ok(SelectedWalletRealmProjection {
            identity: SelectedWalletRealmIdentity { profile, realm },
            revision: published.revision,
            fresh,
            consistent,
            actionable: if fresh && consistent {
                SelectedWalletRealmActionReadiness::Ready
            } else if refreshing {
                SelectedWalletRealmActionReadiness::Refreshing
            } else {
                SelectedWalletRealmActionReadiness::Unavailable
            },
            observation: if refreshing {
                SelectedWalletRealmObservation::PollAfter(Duration::from_millis(150))
            } else {
                SelectedWalletRealmObservation::Settled
            },
            view,
        })
    }

    fn observed(&self, profile: &WalletProfileId, realm: &ChainNetworkId) -> ObservedWalletRealm
    where
        W: WalletAccountReadPort + WalletDustSyncPort + WalletShieldedSyncPort,
    {
        let (account, account_state) =
            observe_account(self.wallet.account_in_realm(profile, realm));
        let (dust, dust_state) = observe_dust(self.wallet.dust_status_in_realm(profile, realm));
        let (shielded, shielded_state) =
            observe_shielded(self.wallet.shielded_status_in_realm(profile, realm));
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
    W: WalletNetworkPort,
{
    fn ensure_selected_realm(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Result<(), SelectedWalletRealmSyncError> {
        let selected = self
            .wallet
            .selected_network(profile)
            .map_err(SelectedWalletRealmSyncError::SelectedNetwork)?;
        if &selected == realm {
            Ok(())
        } else {
            Err(SelectedWalletRealmSyncError::SelectionChanged)
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
    fn pin_selected_realm(
        &self,
        profile: &WalletProfileId,
        realm: &ChainNetworkId,
    ) -> Result<(), SelectedWalletRealmSyncError> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        let retired = self
            .runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
            .retire_other_realms(profile, realm);
        for retired_realm in retired {
            let _ = self
                .wallet
                .cancel_dust_sync_in_realm(profile, &retired_realm);
            let _ = self
                .wallet
                .cancel_shielded_sync_in_realm(profile, &retired_realm);
        }
        Ok(())
    }

    /// Reconciles the selected realm for one explicit application trigger.
    pub fn reconcile(
        &self,
        command: SelectedWalletRealmSyncCommand,
        trigger: WalletRealmReconciliationTrigger,
    ) -> SelectedWalletRealmProjectionFuture<'_> {
        Box::pin(async move {
            let profile = Self::profile(command)?;
            let realm = self
                .wallet
                .selected_network(&profile)
                .map_err(SelectedWalletRealmSyncError::SelectedNetwork)?;
            self.pin_selected_realm(&profile, &realm)?;
            let observation_generation = self.begin_observation(&profile, &realm)?;
            let observed = self.observed(&profile, &realm);
            let mut view = observed.view;
            let mut effects = self
                .runtime
                .lock()
                .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
                .reconcile(profile.clone(), realm.clone(), observed.state, trigger);
            let mut leases = WalletRealmLeaseGuard {
                runtime: &self.runtime,
                profile: profile.clone(),
                realm: realm.clone(),
                active: effects.clone(),
            };
            while !effects.is_empty() {
                let effect = effects.remove(0);
                let outcome = match effect.kind() {
                    WalletRealmReconciliationEffect::SyncAccount => {
                        if !leases.effect_active(effect)? {
                            leases.discard(effect);
                            continue;
                        }
                        let (family, state) =
                            observe_account(self.wallet.sync_in_realm(&profile, &realm).await);
                        view.account = family;
                        effect_outcome(state)
                    }
                    WalletRealmReconciliationEffect::SyncDust => {
                        let _operation = self
                            .operation_gate
                            .lock()
                            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
                        if !leases.effect_active(effect)? {
                            leases.discard(effect);
                            continue;
                        }
                        let (family, state) =
                            observe_dust(self.wallet.start_dust_sync_in_realm(&profile, &realm));
                        view.dust = family;
                        effect_outcome(state)
                    }
                    WalletRealmReconciliationEffect::SyncShielded => {
                        let _operation = self
                            .operation_gate
                            .lock()
                            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
                        if !leases.effect_active(effect)? {
                            leases.discard(effect);
                            continue;
                        }
                        let (family, state) = observe_shielded(
                            self.wallet.start_shielded_sync_in_realm(&profile, &realm),
                        );
                        view.shielded = family;
                        effect_outcome(state)
                    }
                };
                self.ensure_selected_realm(&profile, &realm)?;
                let follow_up = leases.complete(effect, outcome)?;
                effects.extend(follow_up);
            }
            self.projection(profile, realm, observation_generation, view)
        })
    }
}

impl<W> WalletNetworkSelectionObserver for SelectedWalletRealmSyncService<W>
where
    W: WalletNetworkPort
        + WalletAccountReadPort
        + WalletDustSyncPort
        + WalletShieldedSyncPort
        + 'static,
{
    fn selected(
        &self,
        profile: &WalletProfileId,
        network: &ChainNetworkId,
    ) -> Result<(), WalletAccountPortError> {
        self.pin_selected_realm(profile, network)
            .map_err(|_| WalletAccountPortError::Unavailable)
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
    ) -> SelectedWalletRealmProjectionFuture<'_> {
        self.reconcile(command, WalletRealmReconciliationTrigger::ManualRefresh)
    }
}

impl<W> GetSelectedWalletRealmSyncUseCase for SelectedWalletRealmSyncService<W>
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
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError> {
        let profile = Self::profile(command)?;
        let realm = self
            .wallet
            .selected_network(&profile)
            .map_err(SelectedWalletRealmSyncError::SelectedNetwork)?;
        let observation_generation = self.begin_observation(&profile, &realm)?;
        let view = self.observed(&profile, &realm).view;
        self.projection(profile, realm, observation_generation, view)
    }
}

impl<W> CancelSelectedWalletRealmSyncUseCase for SelectedWalletRealmSyncService<W>
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
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError> {
        let profile = Self::profile(command)?;
        let realm = self
            .wallet
            .selected_network(&profile)
            .map_err(SelectedWalletRealmSyncError::SelectedNetwork)?;
        self.pin_selected_realm(&profile, &realm)?;
        let observation_generation = self.begin_observation(&profile, &realm)?;
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?;
        self.runtime
            .lock()
            .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
            .cancel_profile(&profile);
        let account = self
            .wallet
            .account_in_realm(&profile, &realm)
            .map(|snapshot| {
                WalletRealmFamilyView::Ready(WalletAccountView::from_snapshot(&snapshot))
            })
            .unwrap_or_else(account_family_error);
        let dust = self
            .wallet
            .cancel_dust_sync_in_realm(&profile, &realm)
            .map(|snapshot| WalletRealmFamilyView::Ready(WalletDustSyncView::from(&snapshot)))
            .unwrap_or_else(dust_family_error);
        let shielded = self
            .wallet
            .cancel_shielded_sync_in_realm(&profile, &realm)
            .map(|snapshot| WalletRealmFamilyView::Ready(WalletShieldedSyncView::from(&snapshot)))
            .unwrap_or_else(shielded_family_error);
        self.projection(
            profile,
            realm,
            observation_generation,
            SelectedWalletRealmSyncView {
                account,
                dust,
                shielded,
            },
        )
    }
}

fn selected_realm_is_fresh(view: &SelectedWalletRealmSyncView) -> bool {
    matches!(&view.account, WalletRealmFamilyView::Ready(account) if account.sync.state == "synced" && account.source == "live")
        && matches!(&view.dust, WalletRealmFamilyView::Ready(dust) if dust.state == "synced" && dust.failure.is_none())
        && matches!(&view.shielded, WalletRealmFamilyView::Ready(shielded) if shielded.state == "synced" && shielded.failure.is_none())
}

fn selected_realm_is_refreshing(view: &SelectedWalletRealmSyncView) -> bool {
    matches!(&view.account, WalletRealmFamilyView::Ready(account) if account.sync.state == "syncing")
        || matches!(&view.dust, WalletRealmFamilyView::Ready(dust) if dust.state == "syncing")
        || matches!(&view.shielded, WalletRealmFamilyView::Ready(shielded) if shielded.state == "syncing")
        || matches!(&view.dust, WalletRealmFamilyView::Busy)
        || matches!(&view.shielded, WalletRealmFamilyView::Busy)
}

fn selected_realm_is_consistent(view: &SelectedWalletRealmSyncView) -> bool {
    !matches!(
        &view.account,
        WalletRealmFamilyView::InvalidData | WalletRealmFamilyView::Unavailable
    ) && !matches!(
        &view.dust,
        WalletRealmFamilyView::InvalidData | WalletRealmFamilyView::Unavailable
    ) && !matches!(
        &view.shielded,
        WalletRealmFamilyView::InvalidData | WalletRealmFamilyView::Unavailable
    )
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
        WalletRealmFacetState::Updating => WalletRealmEffectOutcome::InProgress,
        WalletRealmFacetState::Stale => WalletRealmEffectOutcome::Stale,
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
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
        task::{Context, Poll, Waker},
    };

    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_domain::{
        ChainKind, ChainNetwork, ChainNetworkId, NetworkDisplayName, NetworkEnvironment,
        WalletAccountSnapshot, WalletDustSyncFailure, WalletDustSyncSnapshot,
        WalletShieldedSyncFailure, WalletShieldedSyncSnapshot,
    };

    use crate::{SelectWalletNetworkCommand, SelectWalletNetworkUseCase, WalletNetworkService};

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

        fn account_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            self.account(profile)
        }

        fn sync_in_realm<'a>(
            &'a self,
            profile: &'a WalletProfileId,
            _: &'a ChainNetworkId,
        ) -> crate::WalletAccountPortFuture<'a> {
            self.sync(profile)
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

        fn dust_status_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_status(profile)
        }

        fn start_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.start_dust_sync(profile)
        }

        fn cancel_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.cancel_dust_sync(profile)
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

        fn shielded_status_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_status(profile)
        }

        fn start_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.start_shielded_sync(profile)
        }

        fn cancel_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.cancel_shielded_sync(profile)
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

        fn account_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            self.account(profile)
        }

        fn sync_in_realm<'a>(
            &'a self,
            profile: &'a WalletProfileId,
            _: &'a ChainNetworkId,
        ) -> crate::WalletAccountPortFuture<'a> {
            self.sync(profile)
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

        fn dust_status_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_status(profile)
        }

        fn start_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.start_dust_sync(profile)
        }

        fn cancel_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.cancel_dust_sync(profile)
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

        fn shielded_status_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_status(profile)
        }

        fn start_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.start_shielded_sync(profile)
        }

        fn cancel_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.cancel_shielded_sync(profile)
        }
    }

    struct CancelDuringAccountWallet {
        account_ready: AtomicBool,
        dust_starts: AtomicUsize,
        dust_cancels: AtomicUsize,
        shielded_cancels: AtomicUsize,
        selected: Mutex<ChainNetworkId>,
        account_realm: Mutex<Option<ChainNetworkId>>,
        dust_realm: Mutex<Option<ChainNetworkId>>,
    }

    impl Default for CancelDuringAccountWallet {
        fn default() -> Self {
            Self {
                account_ready: AtomicBool::new(false),
                dust_starts: AtomicUsize::new(0),
                dust_cancels: AtomicUsize::new(0),
                shielded_cancels: AtomicUsize::new(0),
                selected: Mutex::new(network_id()),
                account_realm: Mutex::new(None),
                dust_realm: Mutex::new(None),
            }
        }
    }

    impl WalletNetworkPort for CancelDuringAccountWallet {
        fn available_networks(&self) -> Result<Vec<ChainNetwork>, WalletAccountPortError> {
            Ok(vec![network(), preprod_network()])
        }

        fn selected_network(
            &self,
            _: &WalletProfileId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            self.selected
                .lock()
                .map(|selected| selected.clone())
                .map_err(|_| WalletAccountPortError::Unavailable)
        }

        fn select_network(
            &self,
            _: &WalletProfileId,
            network_id: &ChainNetworkId,
        ) -> Result<ChainNetworkId, WalletAccountPortError> {
            *self
                .selected
                .lock()
                .map_err(|_| WalletAccountPortError::Unavailable)? = network_id.clone();
            Ok(network_id.clone())
        }
    }

    impl WalletAccountReadPort for CancelDuringAccountWallet {
        fn account(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            Ok(WalletAccountSnapshot::unavailable(network()))
        }

        fn sync<'a>(&'a self, _: &'a WalletProfileId) -> crate::WalletAccountPortFuture<'a> {
            Box::pin(std::future::poll_fn(move |_| {
                if self.account_ready.load(Ordering::Relaxed) {
                    Poll::Ready(Ok(WalletAccountSnapshot::unavailable(network())))
                } else {
                    Poll::Pending
                }
            }))
        }

        fn account_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            self.account(profile)
        }

        fn sync_in_realm<'a>(
            &'a self,
            profile: &'a WalletProfileId,
            realm: &'a ChainNetworkId,
        ) -> crate::WalletAccountPortFuture<'a> {
            if let Ok(mut captured) = self.account_realm.lock() {
                *captured = Some(realm.clone());
            }
            self.sync(profile)
        }
    }

    impl WalletDustSyncPort for CancelDuringAccountWallet {
        fn dust_status(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Ok(WalletDustSyncSnapshot::never_synced(network_id()))
        }

        fn start_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_starts.fetch_add(1, Ordering::Relaxed);
            WalletDustSyncSnapshot::new(
                network_id(),
                WalletDustSyncState::Syncing,
                None,
                None,
                0,
                None,
                None,
                None,
            )
            .map_err(|_| WalletDustSyncPortError::InvalidData)
        }

        fn cancel_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_cancels.fetch_add(1, Ordering::Relaxed);
            Err(WalletDustSyncPortError::Conflict)
        }

        fn dust_status_in_realm(
            &self,
            _: &WalletProfileId,
            realm: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Ok(WalletDustSyncSnapshot::never_synced(realm.clone()))
        }

        fn start_dust_sync_in_realm(
            &self,
            _: &WalletProfileId,
            realm: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.dust_starts.fetch_add(1, Ordering::Relaxed);
            *self
                .dust_realm
                .lock()
                .map_err(|_| WalletDustSyncPortError::Unavailable)? = Some(realm.clone());
            WalletDustSyncSnapshot::new(
                realm.clone(),
                WalletDustSyncState::Syncing,
                None,
                None,
                0,
                None,
                None,
                None,
            )
            .map_err(|_| WalletDustSyncPortError::InvalidData)
        }

        fn cancel_dust_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            self.cancel_dust_sync(profile)
        }
    }

    impl WalletShieldedSyncPort for CancelDuringAccountWallet {
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
            self.shielded_cancels.fetch_add(1, Ordering::Relaxed);
            Err(WalletShieldedSyncPortError::UnsupportedNetwork)
        }

        fn shielded_status_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.shielded_status(profile)
        }

        fn start_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.start_shielded_sync(profile)
        }

        fn cancel_shielded_sync_in_realm(
            &self,
            profile: &WalletProfileId,
            _: &ChainNetworkId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            self.cancel_shielded_sync(profile)
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

    fn preprod_network() -> ChainNetwork {
        ChainNetwork::new(
            ChainKind::Midnight,
            ChainNetworkId::parse("preprod").expect("network id is valid"),
            NetworkDisplayName::parse("PreProd").expect("network name is valid"),
            NetworkEnvironment::PublicTest,
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
        assert!(
            runtime
                .retire_other_realms(&profile_a, &network_id())
                .is_empty()
        );
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
        let preprod = ChainNetworkId::parse("preprod").expect("network id");
        assert_eq!(
            runtime.retire_other_realms(&profile_a, &preprod),
            [network_id()]
        );
        let switched = runtime.reconcile(
            profile_a.clone(),
            preprod,
            stale,
            WalletRealmReconciliationTrigger::Initial,
        );
        assert_eq!(switched.len(), 1);
        let retired_revision = runtime
            .state(&profile_a, &network_id())
            .expect("retired generation remains as a tombstone")
            .revision();
        assert_eq!(retired_revision, 3);
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
        assert_eq!(
            runtime
                .state(&profile_a, &network_id())
                .expect("stale completion cannot replace tombstone")
                .revision(),
            retired_revision
        );

        assert_eq!(
            runtime.retire_other_realms(&profile_a, &network_id()),
            [ChainNetworkId::parse("preprod").expect("network id")]
        );
        let returned = runtime.reconcile(
            profile_a.clone(),
            network_id(),
            stale,
            WalletRealmReconciliationTrigger::Initial,
        );
        assert_eq!(returned[0].revision(), 4);
        assert_ne!(returned[0], repeated[0]);
    }

    #[test]
    fn abandoned_service_future_expires_every_owned_lease() {
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
        assert!(matches!(
            duplicate.view.account,
            WalletRealmFamilyView::Ready(_)
        ));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 1);

        drop(first);
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let expired = service
            .runtime
            .lock()
            .expect("runtime lock")
            .state(&profile, &network_id())
            .expect("runtime state");
        assert_eq!(expired.revision(), 2);
        assert_eq!(expired.status(), crate::WalletRealmCoordinatorStatus::Stale);

        let mut restarted = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        assert!(matches!(
            restarted.as_mut().poll(&mut context),
            Poll::Pending
        ));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn cancellation_prevents_queued_family_effects_from_restarting() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let mut reconcile = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(
            reconcile.as_mut().poll(&mut context),
            Poll::Pending
        ));

        CancelSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("cancel invalidates queued coordinator effects");
        wallet.account_ready.store(true, Ordering::Relaxed);
        let Poll::Ready(Ok(projection)) = reconcile.as_mut().poll(&mut context) else {
            panic!("superseded worker returns the newer cancellation projection");
        };
        assert!(matches!(projection.view.dust, WalletRealmFamilyView::Busy));
        assert_eq!(wallet.dust_starts.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn in_flight_reconciliation_rejects_an_unobserved_a_b_a_switch() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        let runtime = Arc::new(Mutex::new(SelectedWalletRealmRuntime::default()));
        let service = Arc::new(SelectedWalletRealmSyncService::with_runtime(
            Arc::clone(&wallet),
            Arc::clone(&runtime),
        ));
        let selection_observer: Arc<dyn WalletNetworkSelectionObserver> = service.clone();
        let network_service =
            WalletNetworkService::with_selection_observer(Arc::clone(&wallet), selection_observer);
        let mut reconcile = service.reconcile(command(), WalletRealmReconciliationTrigger::Initial);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(
            reconcile.as_mut().poll(&mut context),
            Poll::Pending
        ));

        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let preprod = ChainNetworkId::parse("preprod").expect("network id");
        SelectWalletNetworkUseCase::execute(
            &network_service,
            SelectWalletNetworkCommand {
                profile_id: profile.as_str().to_owned(),
                network_id: preprod.as_str().to_owned(),
            },
        )
        .expect("selection changes while the old operation is in flight");
        SelectWalletNetworkUseCase::execute(
            &network_service,
            SelectWalletNetworkCommand {
                profile_id: profile.as_str().to_owned(),
                network_id: network_id().as_str().to_owned(),
            },
        )
        .expect("selection returns before the old operation completes");
        wallet.account_ready.store(true, Ordering::Relaxed);
        assert!(matches!(
            reconcile.as_mut().poll(&mut context),
            Poll::Ready(Err(SelectedWalletRealmSyncError::ObservationSuperseded))
        ));

        assert_eq!(
            *wallet.account_realm.lock().expect("account realm lock"),
            Some(network_id())
        );
        assert_eq!(*wallet.dust_realm.lock().expect("DUST realm lock"), None);
        assert_eq!(wallet.dust_starts.load(Ordering::Relaxed), 0);
        assert_eq!(wallet.dust_cancels.load(Ordering::Relaxed), 2);
        assert_eq!(wallet.shielded_cancels.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn projection_rejects_a_realm_switch_deterministically() {
        let wallet = Arc::new(CancelDuringAccountWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let first = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("initial projection");
        let unchanged = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("unchanged projection");
        assert_eq!(first.revision, unchanged.revision);
        let mut refreshing_view = first.view.clone();
        refreshing_view.dust = WalletRealmFamilyView::Busy;
        let observation_generation = service
            .begin_observation(&first.identity.profile, &first.identity.realm)
            .expect("observation begins");
        let refreshing = service
            .projection(
                first.identity.profile.clone(),
                first.identity.realm.clone(),
                observation_generation,
                refreshing_view,
            )
            .expect("refreshing projection");
        assert_eq!(
            refreshing.observation,
            SelectedWalletRealmObservation::PollAfter(Duration::from_millis(150))
        );
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let preprod = ChainNetworkId::parse("preprod").expect("network id");
        wallet
            .select_network(&profile, &preprod)
            .expect("select replacement realm");
        WalletNetworkSelectionObserver::selected(&service, &profile, &preprod)
            .expect("record replacement realm");
        let replacement = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("replacement projection");
        assert_ne!(first.identity, replacement.identity);
        assert!(!replacement.supersedes(&first));

        wallet
            .select_network(&profile, &network_id())
            .expect("restore original realm");
        WalletNetworkSelectionObserver::selected(&service, &profile, &network_id())
            .expect("record restored realm");
        let restored = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("restored projection");
        assert_eq!(first.identity, restored.identity);
        assert!(restored.revision > first.revision);
        assert!(restored.supersedes(&first));
        assert!(!first.supersedes(&restored));
    }

    #[test]
    fn older_same_realm_observation_cannot_publish_after_a_newer_read() {
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let realm = network_id();
        let mut runtime = SelectedWalletRealmRuntime::default();
        let older = runtime
            .begin_observation(&profile, &realm)
            .expect("first observation begins");
        let newer = runtime
            .begin_observation(&profile, &realm)
            .expect("second observation begins");
        let view = SelectedWalletRealmSyncView {
            account: WalletRealmFamilyView::Unavailable,
            dust: WalletRealmFamilyView::Unavailable,
            shielded: WalletRealmFamilyView::Unavailable,
        };

        assert_eq!(
            runtime.publish(profile.clone(), realm.clone(), newer, &view),
            Some(1)
        );
        assert_eq!(runtime.publish(profile, realm, older, &view), None);
    }

    #[test]
    fn read_cannot_rewrite_a_selection_owned_by_the_mutation_boundary() {
        let profile = WalletProfileId::parse("profile_test").expect("profile id");
        let standalone = network_id();
        let preprod = ChainNetworkId::parse("preprod").expect("network id");
        let mut runtime = SelectedWalletRealmRuntime::default();

        assert!(runtime.retire_other_realms(&profile, &preprod).is_empty());
        assert_eq!(runtime.begin_observation(&profile, &standalone), None);
        assert!(runtime.selection_matches(&profile, &preprod));
        assert_eq!(
            runtime.retire_other_realms(&profile, &standalone),
            [preprod]
        );
    }

    #[test]
    fn selection_invalidation_never_recovers_a_cached_previous_realm() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet::default()));
        let first = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("initial projection");
        let stale = service
            .begin_observation(&first.identity.profile, &first.identity.realm)
            .expect("observation begins before selection changes");
        service
            .runtime
            .lock()
            .expect("runtime lock")
            .retire_other_realms(
                &first.identity.profile,
                &ChainNetworkId::parse("preprod").expect("network id"),
            );

        assert_eq!(
            service.projection(
                first.identity.profile,
                first.identity.realm,
                stale,
                first.view,
            ),
            Err(SelectedWalletRealmSyncError::SelectionChanged)
        );
    }

    #[test]
    fn superseded_action_returns_the_newer_published_projection() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet::default()));
        let first = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("initial projection");
        let older = service
            .begin_observation(&first.identity.profile, &first.identity.realm)
            .expect("action observation begins");
        let newer = service
            .begin_observation(&first.identity.profile, &first.identity.realm)
            .expect("poll observation begins");
        let mut newer_view = first.view.clone();
        newer_view.dust = WalletRealmFamilyView::Busy;
        let published = service
            .projection(
                first.identity.profile.clone(),
                first.identity.realm.clone(),
                newer,
                newer_view.clone(),
            )
            .expect("newer poll publishes");

        let recovered = service
            .projection(
                first.identity.profile,
                first.identity.realm,
                older,
                first.view,
            )
            .expect("superseded action recovers the latest projection");

        assert_eq!(recovered, published);
        assert_eq!(recovered.view, newer_view);
        assert_eq!(
            recovered.observation,
            SelectedWalletRealmObservation::PollAfter(Duration::from_millis(150))
        );
    }

    #[test]
    fn cached_account_snapshot_never_makes_the_realm_fresh() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet::default()));
        let first = GetSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("initial projection");
        let mut view = first.view;
        {
            let WalletRealmFamilyView::Ready(account) = &mut view.account else {
                panic!("account fixture must be present");
            };
            account.sync.state = "synced".to_owned();
            account.source = "cached".to_owned();
        }
        view.dust = WalletRealmFamilyView::Ready(WalletDustSyncView {
            network_id: "undeployed".to_owned(),
            state: "synced".to_owned(),
            current_cursor: Some(1),
            target_cursor: Some(1),
            events_processed: 1,
            balance_atomic_units: Some("1".to_owned()),
            updated_at_millis: Some(1),
            failure: None,
        });
        view.shielded = WalletRealmFamilyView::Ready(WalletShieldedSyncView {
            network_id: "undeployed".to_owned(),
            state: "synced".to_owned(),
            current_cursor: Some(1),
            target_cursor: Some(1),
            events_processed: 1,
            owned_note_count: Some(1),
            commitment_count: Some(1),
            balances: Vec::new(),
            updated_at_millis: Some(1),
            failure: None,
        });

        assert!(!selected_realm_is_fresh(&view));
        if let WalletRealmFamilyView::Ready(account) = &mut view.account {
            account.source = "live".to_owned();
        }
        assert!(selected_realm_is_fresh(&view));

        if let WalletRealmFamilyView::Ready(dust) = &mut view.dust {
            dust.failure = Some("transport_unavailable".to_owned());
        }
        assert!(!selected_realm_is_fresh(&view));
        if let WalletRealmFamilyView::Ready(dust) = &mut view.dust {
            dust.failure = None;
        }
        if let WalletRealmFamilyView::Ready(shielded) = &mut view.shielded {
            shielded.failure = Some("transport_unavailable".to_owned());
        }
        assert!(!selected_realm_is_fresh(&view));
        if let WalletRealmFamilyView::Ready(shielded) = &mut view.shielded {
            shielded.failure = None;
        }
        assert!(selected_realm_is_fresh(&view));
    }

    #[test]
    fn aggregate_preserves_partial_family_failures_as_typed_state() {
        let wallet = Arc::new(PartialWallet::default());
        let service = SelectedWalletRealmSyncService::new(Arc::clone(&wallet));
        let started = resolve(SyncSelectedWalletRealmUseCase::execute(&service, command()))
            .expect("aggregate starts");
        assert!(matches!(
            started.view.account,
            WalletRealmFamilyView::Ready(_)
        ));
        assert_eq!(started.view.dust, WalletRealmFamilyView::ProtectionLocked);
        assert!(matches!(
            started.view.shielded,
            WalletRealmFamilyView::Ready(WalletShieldedSyncView { ref state, .. })
                if state == "never_synced"
        ));
        assert_eq!(wallet.account_syncs.load(Ordering::Relaxed), 1);
        assert_eq!(wallet.dust_starts.load(Ordering::Relaxed), 1);
        assert_eq!(wallet.shielded_starts.load(Ordering::Relaxed), 1);
        assert_eq!(started.identity.realm, network_id());
        assert!(!started.fresh);
        assert!(started.consistent);
        assert_eq!(
            started.actionable,
            SelectedWalletRealmActionReadiness::Unavailable
        );
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
        assert_eq!(cancelled.view.dust, WalletRealmFamilyView::ProtectionLocked);
        assert_eq!(cancelled.view.shielded, WalletRealmFamilyView::Unavailable);
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
