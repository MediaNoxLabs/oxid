// SPDX-License-Identifier: Apache-2.0

//! Composition-owned DUST registration settlement.
//!
//! Incoming adapters observe one presentation-safe projection and provide one
//! explicit consent record. The exact preview, transaction observation, realm
//! refresh, and stale-generation checks remain owned by composition.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const OPERATION_DEADLINE: Duration = Duration::from_secs(30);
const MAX_SETTLEMENT_RETRIES: u8 = 3;

use tokio::sync::watch;

use oxid_wallet_application::{
    AuthorizeDevelopmentWalletDustRegistrationCommand,
    AuthorizeDevelopmentWalletDustRegistrationUseCase, AuthorizeWalletDustRegistrationCommand,
    AuthorizeWalletDustRegistrationUseCase, ChainTransactionId,
    ExecuteWalletDustRegistrationOperation, GetSelectedWalletRealmSyncUseCase,
    GetWalletDustRegistrationStatusCommand, GetWalletDustRegistrationStatusUseCase,
    PrepareWalletDustRegistrationCommand, PrepareWalletDustRegistrationUseCase,
    ReconcileSelectedWalletRealmUseCase, ReconcileWalletDustRegistrationSubmissionCommand,
    ReconcileWalletDustRegistrationSubmissionUseCase, SelectedWalletRealmProjection,
    SelectedWalletRealmProjectionFuture, SelectedWalletRealmReconciliationFuture,
    SelectedWalletRealmSyncCommand, SensitiveOperationConfirmation,
    SubmitDevelopmentWalletDustRegistrationCommand, SubmitDevelopmentWalletDustRegistrationUseCase,
    SubmitWalletDustRegistrationCommand, SubmitWalletDustRegistrationUseCase,
    SyncSelectedWalletRealmUseCase, WalletDustRegistrationDriver,
    WalletDustRegistrationDriverError, WalletDustRegistrationEffect,
    WalletDustRegistrationExecutorFailure, WalletDustRegistrationOperationCompletion,
    WalletDustRegistrationOperationFuture, WalletDustRegistrationPreviewView,
    WalletDustRegistrationRecoveryRecord, WalletDustRegistrationRecoveryStore,
    WalletDustRegistrationRecoveryStoreError, WalletDustRegistrationRuntimeOperation,
    WalletDustRegistrationSettlementEvent, WalletDustRegistrationSettlementIdentity,
    WalletDustRegistrationSettlementProjection, WalletDustRegistrationSettlementReconciliation,
    WalletRealmFamilyView, WalletRealmReconciliationTrigger, WalletTransactionDraftId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DustSettlementAuthority {
    Explicit,
    AutomaticDevelopment,
}

enum DustRegistrationAuthorization {
    Explicit(Arc<dyn AuthorizeWalletDustRegistrationUseCase>),
    AutomaticDevelopment(Arc<dyn AuthorizeDevelopmentWalletDustRegistrationUseCase>),
}

enum DustRegistrationSubmission {
    Explicit(Arc<dyn SubmitWalletDustRegistrationUseCase>),
    AutomaticDevelopment(Arc<dyn SubmitDevelopmentWalletDustRegistrationUseCase>),
}

/// Composition-owned capability shared by headless and graphical adapters.
pub struct WalletDustSettlementCapability {
    selected_realm: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
    executor: Arc<ComposedDustRegistrationExecutor>,
    driver: WalletDustRegistrationDriver,
    projections: watch::Sender<WalletDustRegistrationSettlementProjection>,
    retry_attempts: Mutex<(Option<WalletDustRegistrationSettlementIdentity>, u8)>,
    authority: DustSettlementAuthority,
}

/// Public, presentation-safe facts for the one DUST authorization decision.
///
/// Draft identifiers, authorization challenges, and protected-key material stay
/// inside composition; adapters receive only the facts a person can review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustAuthorizationReview {
    pub network_id: String,
    pub registered_night: oxid_wallet_application::WalletDustRegistrationAssetView,
    pub input_count: u16,
    pub maximum_fee_allowance: oxid_wallet_application::WalletDustRegistrationAssetView,
}

impl WalletDustSettlementCapability {
    #[allow(clippy::too_many_arguments)]
    pub fn with_recovery_store(
        selected_realm: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
        sync_selected_realm: Arc<dyn SyncSelectedWalletRealmUseCase>,
        prepare: Arc<dyn PrepareWalletDustRegistrationUseCase>,
        authorize: Arc<dyn AuthorizeWalletDustRegistrationUseCase>,
        submit: Arc<dyn SubmitWalletDustRegistrationUseCase>,
        status: Arc<dyn GetWalletDustRegistrationStatusUseCase>,
        reconcile: Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase>,
        store: Arc<dyn WalletDustRegistrationRecoveryStore>,
    ) -> Result<Self, WalletDustSettlementError> {
        Self::with_recovery_store_authority_and_deadline(
            selected_realm,
            sync_selected_realm,
            prepare,
            DustRegistrationAuthorization::Explicit(authorize),
            DustRegistrationSubmission::Explicit(submit),
            status,
            reconcile,
            store,
            DustSettlementAuthority::Explicit,
            OPERATION_DEADLINE,
        )
    }

    /// Builds a capability that automatically admits the exact DUST
    /// registration preview for the local development realm only.
    #[allow(clippy::too_many_arguments)]
    pub fn with_automatic_development_authority_and_recovery_store(
        selected_realm: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
        sync_selected_realm: Arc<dyn SyncSelectedWalletRealmUseCase>,
        prepare: Arc<dyn PrepareWalletDustRegistrationUseCase>,
        authorize: Arc<dyn AuthorizeDevelopmentWalletDustRegistrationUseCase>,
        submit: Arc<dyn SubmitDevelopmentWalletDustRegistrationUseCase>,
        status: Arc<dyn GetWalletDustRegistrationStatusUseCase>,
        reconcile: Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase>,
        store: Arc<dyn WalletDustRegistrationRecoveryStore>,
    ) -> Result<Self, WalletDustSettlementError> {
        Self::with_recovery_store_authority_and_deadline(
            selected_realm,
            sync_selected_realm,
            prepare,
            DustRegistrationAuthorization::AutomaticDevelopment(authorize),
            DustRegistrationSubmission::AutomaticDevelopment(submit),
            status,
            reconcile,
            store,
            DustSettlementAuthority::AutomaticDevelopment,
            OPERATION_DEADLINE,
        )
    }

    /// Builds the capability with one application-owned deadline shared by
    /// every admitted settlement operation. Exposed for deterministic host
    /// tests and composition roots with an explicit runtime budget.
    #[allow(clippy::too_many_arguments)]
    pub fn with_recovery_store_and_deadline(
        selected_realm: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
        sync_selected_realm: Arc<dyn SyncSelectedWalletRealmUseCase>,
        prepare: Arc<dyn PrepareWalletDustRegistrationUseCase>,
        authorize: Arc<dyn AuthorizeWalletDustRegistrationUseCase>,
        submit: Arc<dyn SubmitWalletDustRegistrationUseCase>,
        status: Arc<dyn GetWalletDustRegistrationStatusUseCase>,
        reconcile: Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase>,
        store: Arc<dyn WalletDustRegistrationRecoveryStore>,
        operation_deadline: Duration,
    ) -> Result<Self, WalletDustSettlementError> {
        Self::with_recovery_store_authority_and_deadline(
            selected_realm,
            sync_selected_realm,
            prepare,
            DustRegistrationAuthorization::Explicit(authorize),
            DustRegistrationSubmission::Explicit(submit),
            status,
            reconcile,
            store,
            DustSettlementAuthority::Explicit,
            operation_deadline,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn with_recovery_store_authority_and_deadline(
        selected_realm: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
        sync_selected_realm: Arc<dyn SyncSelectedWalletRealmUseCase>,
        prepare: Arc<dyn PrepareWalletDustRegistrationUseCase>,
        authorize: DustRegistrationAuthorization,
        submit: DustRegistrationSubmission,
        status: Arc<dyn GetWalletDustRegistrationStatusUseCase>,
        reconcile: Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase>,
        store: Arc<dyn WalletDustRegistrationRecoveryStore>,
        authority: DustSettlementAuthority,
        operation_deadline: Duration,
    ) -> Result<Self, WalletDustSettlementError> {
        let (loaded, mut initially_durable) = match store.load() {
            Ok(record) => (record, true),
            Err(WalletDustRegistrationRecoveryStoreError::Corrupt) => {
                let cleared = store.clear().is_ok();
                (None, cleared)
            }
            // A policy/integrity failure may indicate an attacker-controlled path.
            // Never delete it as a recovery side effect; refuse durable submission.
            Err(WalletDustRegistrationRecoveryStoreError::Integrity)
            | Err(WalletDustRegistrationRecoveryStoreError::Unavailable) => (None, false),
        };
        let restored = match loaded {
            Some(record)
                if matches!(
                    record.state,
                    oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
                ) =>
            {
                // Protected authorization cannot be resumed after restart. Remove
                // the stale public recovery record and let refresh rebuild the
                // authorization boundary from the selected realm.
                initially_durable = store.clear().is_ok();
                None
            }
            Some(record) => match record.restore_runtime() {
                Ok(runtime) => Some(runtime),
                Err(_) => {
                    // An internally inconsistent public record is recoverable:
                    // discard it and start from the authoritative selected realm.
                    initially_durable = store.clear().is_ok();
                    None
                }
            },
            None => None,
        };
        let durable = Arc::new(AtomicBool::new(initially_durable));
        let recovered_revision = restored
            .as_ref()
            .and_then(|runtime| runtime.coordinator().projection().registration.as_ref())
            .map_or(0, |registration| {
                registration
                    .observation_revision
                    .max(registration.dust_revision)
                    .max(registration.finality_revision)
            });
        let executor = Arc::new(ComposedDustRegistrationExecutor {
            selected_realm: Arc::clone(&selected_realm),
            sync_selected_realm,
            prepare,
            authorize,
            submit,
            status,
            reconcile,
            retained: Mutex::new(RetainedSettlement {
                operation_revision: recovered_revision,
                ..RetainedSettlement::default()
            }),
            store: Arc::clone(&store),
            durable: Arc::clone(&durable),
            operation_deadline,
        });
        let operation_executor: Arc<dyn ExecuteWalletDustRegistrationOperation> = executor.clone();
        let initial = restored
            .as_ref()
            .map(|runtime| runtime.coordinator().projection().clone())
            .unwrap_or_default();
        let (projections, _) = watch::channel(initial.clone());
        let projection_sink = projections.clone();
        let observer: oxid_wallet_application::WalletDustRegistrationProjectionObserver =
            Arc::new(move |projection| {
                let persisted = WalletDustRegistrationRecoveryRecord::from_projection(&projection)
                    .is_ok_and(|record| store.save(record).is_ok());
                durable.store(persisted, Ordering::Release);
                projection_sink.send_replace(projection);
            });
        let driver = match restored {
            Some(runtime) => {
                WalletDustRegistrationDriver::with_recovered_runtime_and_projection_observer(
                    operation_executor,
                    runtime,
                    observer,
                )
            }
            None => {
                WalletDustRegistrationDriver::with_projection_observer(operation_executor, observer)
            }
        };
        Ok(Self {
            selected_realm,
            executor,
            driver,
            projections,
            retry_attempts: Mutex::new((initial.identity, 0)),
            authority,
        })
    }

    /// Observes the authoritative selected realm and advances only to the
    /// protected-authorization boundary.
    pub async fn refresh(
        &self,
        profile_id: String,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustSettlementError> {
        let selected = self
            .selected_realm
            .execute(SelectedWalletRealmSyncCommand { profile_id })
            .map_err(|_| WalletDustSettlementError::SelectedRealmUnavailable)?;
        let eligible = selected_realm_is_eligible(&selected);
        let identity = settlement_identity(&selected);
        let current = self.projection()?;
        if let Some(active) = current
            .identity
            .as_ref()
            .filter(|active| *active != &identity)
        {
            self.driver
                .advance(WalletDustRegistrationSettlementEvent::Superseded {
                    identity: active.clone(),
                })
                .await
                .map_err(WalletDustSettlementError::Driver)?;
        }
        self.executor.bind(selected)?;
        if current.identity.as_ref() != Some(&identity) {
            *self
                .retry_attempts
                .lock()
                .map_err(|_| WalletDustSettlementError::RetainedStateUnavailable)? =
                (Some(identity.clone()), 0);
        }
        let mut result = self
            .driver
            .advance(WalletDustRegistrationSettlementEvent::Eligibility {
                identity: identity.clone(),
                revision: self.executor.bound_revision()?,
                eligible,
            })
            .await
            .map_err(WalletDustSettlementError::Driver)?;
        if self.authority == DustSettlementAuthority::AutomaticDevelopment
            && identity.realm.as_str() == "undeployed"
            && result.state
                == oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
        {
            let authorized = self
                .driver
                .authorize()
                .await
                .map_err(WalletDustSettlementError::Driver);
            if authorized.is_err() {
                self.executor.clear_confirmation();
            }
            result = authorized?;
        }
        if !is_recoverable_state(result.state) {
            self.reset_retry_attempts(identity)?;
        }
        Ok(result)
    }

    /// Supplies the one explicit user decision for the exact retained preview.
    /// A declined confirmation becomes a typed rejection and performs no
    /// protected or chain operation.
    pub async fn authorize(
        &self,
        confirmation: SensitiveOperationConfirmation,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustSettlementError> {
        if self
            .driver
            .projection()
            .map_err(WalletDustSettlementError::Driver)?
            .state
            != oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
        {
            return Err(WalletDustSettlementError::Driver(
                WalletDustRegistrationDriverError::AuthorizationNotPending,
            ));
        }
        self.executor.stage_confirmation(confirmation)?;
        let result = self
            .driver
            .authorize()
            .await
            .map_err(WalletDustSettlementError::Driver);
        if result.is_err() {
            self.executor.clear_confirmation();
        }
        let result = result?;
        if let Some(identity) = result.identity.clone()
            && !is_recoverable_state(result.state)
        {
            self.reset_retry_attempts(identity)?;
        }
        Ok(result)
    }

    /// Retries only a retained recoverable operation. It cannot create a
    /// replacement registration or cross a fresh authorization boundary.
    pub async fn retry(
        &self,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustSettlementError> {
        let projection = self.projection()?;
        if !matches!(
            projection.state,
            oxid_wallet_application::WalletDustRegistrationSettlementState::Offline
                | oxid_wallet_application::WalletDustRegistrationSettlementState::TimedOut
                | oxid_wallet_application::WalletDustRegistrationSettlementState::Degraded
                | oxid_wallet_application::WalletDustRegistrationSettlementState::Suspended
        ) {
            return Err(WalletDustSettlementError::RetryNotAdmitted);
        }
        let identity = projection
            .identity
            .ok_or(WalletDustSettlementError::RetainedStateUnavailable)?;
        let revision = projection
            .recovery_revision
            .checked_add(1)
            .ok_or(WalletDustSettlementError::RetainedStateUnavailable)?;
        {
            let attempts = self
                .retry_attempts
                .lock()
                .map_err(|_| WalletDustSettlementError::RetainedStateUnavailable)?;
            if attempts.0.as_ref() != Some(&identity) || attempts.1 >= MAX_SETTLEMENT_RETRIES {
                return Err(WalletDustSettlementError::RetryNotAdmitted);
            }
        }
        let result = self
            .driver
            .advance(WalletDustRegistrationSettlementEvent::Retry {
                identity: identity.clone(),
                revision,
            })
            .await;
        match result {
            Ok(projection) => {
                if is_recoverable_state(projection.state) {
                    self.consume_retry_attempt(&identity)?;
                } else {
                    self.reset_retry_attempts(identity)?;
                }
                Ok(projection)
            }
            Err(WalletDustRegistrationDriverError::Busy) => Err(WalletDustSettlementError::Driver(
                WalletDustRegistrationDriverError::Busy,
            )),
            Err(error) => {
                self.consume_retry_attempt(&identity)?;
                Err(WalletDustSettlementError::Driver(error))
            }
        }
    }

    /// Returns the exact public facts currently awaiting the single protected
    /// authorization. This cannot reconstruct or expose a legacy draft.
    pub fn authorization_review(
        &self,
    ) -> Result<WalletDustAuthorizationReview, WalletDustSettlementError> {
        if self.projection()?.state
            != oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
        {
            return Err(WalletDustSettlementError::Driver(
                WalletDustRegistrationDriverError::AuthorizationNotPending,
            ));
        }
        self.executor.authorization_review()
    }

    pub fn projection(
        &self,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustSettlementError> {
        self.driver
            .projection()
            .map_err(WalletDustSettlementError::Driver)
    }

    fn consume_retry_attempt(
        &self,
        identity: &WalletDustRegistrationSettlementIdentity,
    ) -> Result<(), WalletDustSettlementError> {
        let mut attempts = self
            .retry_attempts
            .lock()
            .map_err(|_| WalletDustSettlementError::RetainedStateUnavailable)?;
        if attempts.0.as_ref() != Some(identity) {
            return Err(WalletDustSettlementError::RetainedStateUnavailable);
        }
        attempts.1 = attempts.1.saturating_add(1);
        Ok(())
    }

    fn reset_retry_attempts(
        &self,
        identity: WalletDustRegistrationSettlementIdentity,
    ) -> Result<(), WalletDustSettlementError> {
        *self
            .retry_attempts
            .lock()
            .map_err(|_| WalletDustSettlementError::RetainedStateUnavailable)? =
            (Some(identity), 0);
        Ok(())
    }

    /// Observes ordered public projection changes without transferring
    /// scheduling or settlement policy to an incoming adapter.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<WalletDustRegistrationSettlementProjection> {
        self.projections.subscribe()
    }
}

/// Adds automatic DUST convergence to both explicit and lifecycle-selected
/// realm reconciliation. A DUST failure remains visible in its own projection
/// and never hides a successful wallet-realm refresh.
pub struct AutomaticDustRealmReconciler {
    sync: Arc<dyn SyncSelectedWalletRealmUseCase>,
    reconcile: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
    get: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
    dust: Arc<WalletDustSettlementCapability>,
}

impl AutomaticDustRealmReconciler {
    #[must_use]
    pub fn new(
        sync: Arc<dyn SyncSelectedWalletRealmUseCase>,
        reconcile: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
        get: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
        dust: Arc<WalletDustSettlementCapability>,
    ) -> Self {
        Self {
            sync,
            reconcile,
            get,
            dust,
        }
    }
}

impl SyncSelectedWalletRealmUseCase for AutomaticDustRealmReconciler {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> SelectedWalletRealmProjectionFuture<'_> {
        Box::pin(async move {
            let profile_id = command.profile_id.clone();
            let projection = self.sync.execute(command).await?;
            let _ = self.dust.refresh(profile_id.clone()).await;
            Ok(self
                .get
                .execute(SelectedWalletRealmSyncCommand { profile_id })
                .unwrap_or(projection))
        })
    }
}

impl ReconcileSelectedWalletRealmUseCase for AutomaticDustRealmReconciler {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
        trigger: WalletRealmReconciliationTrigger,
    ) -> SelectedWalletRealmReconciliationFuture<'_> {
        Box::pin(async move {
            let profile_id = command.profile_id.clone();
            let mut reconciliation = self.reconcile.execute(command, trigger).await?;
            let _ = self.dust.refresh(profile_id.clone()).await;
            if let Ok(projection) = self
                .get
                .execute(SelectedWalletRealmSyncCommand { profile_id })
            {
                reconciliation.projection = projection;
            }
            Ok(reconciliation)
        })
    }
}

/// Bounded composition failures; adapter payloads and custody material never
/// cross the incoming boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustSettlementError {
    SelectedRealmUnavailable,
    StaleRealm,
    RetainedStateUnavailable,
    RetryNotAdmitted,
    Driver(WalletDustRegistrationDriverError),
}

impl std::fmt::Display for WalletDustSettlementError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::SelectedRealmUnavailable => "selected wallet realm is unavailable",
            Self::StaleRealm => "selected wallet realm changed during DUST registration",
            Self::RetainedStateUnavailable => "retained DUST registration state is unavailable",
            Self::RetryNotAdmitted => "DUST registration retry is not currently admitted",
            Self::Driver(error) => return error.fmt(formatter),
        })
    }
}

impl std::error::Error for WalletDustSettlementError {}

#[derive(Default)]
struct RetainedSettlement {
    bound: Option<SelectedWalletRealmProjection>,
    preview: Option<WalletDustRegistrationPreviewView>,
    confirmation: Option<SensitiveOperationConfirmation>,
    finality_observed: bool,
    submission_uncertain: bool,
    operation_revision: u64,
}

enum RecoveredSubmission {
    Observed(ChainTransactionId),
    PreBroadcastCancelled,
}

struct ComposedDustRegistrationExecutor {
    selected_realm: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
    sync_selected_realm: Arc<dyn SyncSelectedWalletRealmUseCase>,
    prepare: Arc<dyn PrepareWalletDustRegistrationUseCase>,
    authorize: DustRegistrationAuthorization,
    submit: DustRegistrationSubmission,
    status: Arc<dyn GetWalletDustRegistrationStatusUseCase>,
    reconcile: Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase>,
    retained: Mutex<RetainedSettlement>,
    store: Arc<dyn WalletDustRegistrationRecoveryStore>,
    durable: Arc<AtomicBool>,
    operation_deadline: Duration,
}

impl ComposedDustRegistrationExecutor {
    fn bind(
        &self,
        selected: SelectedWalletRealmProjection,
    ) -> Result<(), WalletDustSettlementError> {
        let mut retained = self
            .retained
            .lock()
            .map_err(|_| WalletDustSettlementError::RetainedStateUnavailable)?;
        let changed = retained.bound.as_ref().is_none_or(|bound| {
            bound.identity != selected.identity || bound.generation != selected.generation
        });
        if changed {
            retained.preview = None;
            retained.confirmation = None;
            retained.finality_observed = false;
            retained.submission_uncertain = false;
        }
        retained.operation_revision = retained.operation_revision.max(selected.revision);
        retained.bound = Some(selected);
        Ok(())
    }

    fn bound_revision(&self) -> Result<u64, WalletDustSettlementError> {
        self.retained
            .lock()
            .map_err(|_| WalletDustSettlementError::RetainedStateUnavailable)?
            .bound
            .as_ref()
            .map(|bound| bound.revision)
            .ok_or(WalletDustSettlementError::RetainedStateUnavailable)
    }

    fn stage_confirmation(
        &self,
        confirmation: SensitiveOperationConfirmation,
    ) -> Result<(), WalletDustSettlementError> {
        self.retained
            .lock()
            .map_err(|_| WalletDustSettlementError::RetainedStateUnavailable)?
            .confirmation = Some(confirmation);
        Ok(())
    }

    fn clear_confirmation(&self) {
        if let Ok(mut retained) = self.retained.lock() {
            retained.confirmation = None;
        }
    }

    fn authorization_review(
        &self,
    ) -> Result<WalletDustAuthorizationReview, WalletDustSettlementError> {
        let preview = self
            .retained
            .lock()
            .map_err(|_| WalletDustSettlementError::RetainedStateUnavailable)?
            .preview
            .clone()
            .ok_or(WalletDustSettlementError::RetainedStateUnavailable)?;
        Ok(WalletDustAuthorizationReview {
            network_id: preview.network_id,
            registered_night: preview.registered_night,
            input_count: preview.input_count,
            maximum_fee_allowance: preview.maximum_fee_allowance,
        })
    }

    fn validate_bound(
        &self,
        identity: &WalletDustRegistrationSettlementIdentity,
    ) -> Result<SelectedWalletRealmProjection, WalletDustRegistrationExecutorFailure> {
        let bound = self
            .retained
            .lock()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
            .bound
            .clone()
            .ok_or(WalletDustRegistrationExecutorFailure::Unavailable)?;
        if settlement_identity(&bound) != *identity {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        let current = self
            .selected_realm
            .execute(SelectedWalletRealmSyncCommand {
                profile_id: identity.profile.as_str().to_owned(),
            })
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?;
        if current.identity != bound.identity
            || current.generation != bound.generation
            || current.revision != bound.revision
        {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        Ok(bound)
    }

    fn next_revision(&self) -> Result<u64, WalletDustRegistrationExecutorFailure> {
        let mut retained = self
            .retained
            .lock()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?;
        retained.operation_revision = retained.operation_revision.saturating_add(1);
        Ok(retained.operation_revision)
    }

    fn recovery_record(
        &self,
        identity: &WalletDustRegistrationSettlementIdentity,
    ) -> Result<Option<WalletDustRegistrationRecoveryRecord>, WalletDustRegistrationExecutorFailure>
    {
        let record = self
            .store
            .load()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?;
        if record.as_ref().is_some_and(|record| {
            record.profile_id != identity.profile.as_str()
                || record.realm_id != identity.realm.as_str()
                || record.generation != identity.generation
        }) {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        Ok(record)
    }

    async fn execute_prepare(
        &self,
        identity: WalletDustRegistrationSettlementIdentity,
    ) -> Result<WalletDustRegistrationOperationCompletion, WalletDustRegistrationExecutorFailure>
    {
        let bound = self.validate_bound(&identity)?;
        let preview = match self
            .prepare
            .execute(PrepareWalletDustRegistrationCommand {
                profile_id: identity.profile.as_str().to_owned(),
            })
            .await
        {
            Ok(preview) => preview,
            Err(oxid_wallet_application::WalletDustRegistrationError::Operation(
                oxid_wallet_application::WalletDustRegistrationPortError::RegistrationAlreadyCurrent,
            )) => {
                return Ok(WalletDustRegistrationOperationCompletion::already_current(
                    identity,
                    bound.revision,
                ));
            }
            Err(error) => return Err(map_registration_failure(error)),
        };
        if preview.network_id != identity.realm.as_str() {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        let draft_id = WalletTransactionDraftId::parse(preview.draft_id.clone())
            .map_err(|_| WalletDustRegistrationExecutorFailure::Degraded)?;
        self.retained
            .lock()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
            .preview = Some(preview);
        Ok(WalletDustRegistrationOperationCompletion::prepared(
            identity,
            draft_id,
            bound.revision,
        ))
    }

    async fn execute_authorize(
        &self,
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
    ) -> Result<WalletDustRegistrationOperationCompletion, WalletDustRegistrationExecutorFailure>
    {
        self.validate_bound(&identity)?;
        let (preview, confirmation) = {
            let mut retained = self
                .retained
                .lock()
                .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?;
            let preview = retained
                .preview
                .clone()
                .ok_or(WalletDustRegistrationExecutorFailure::Degraded)?;
            let confirmation = retained.confirmation.take();
            (preview, confirmation)
        };
        if preview.draft_id != draft_id.as_str() {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        let authorized = match &self.authorize {
            DustRegistrationAuthorization::Explicit(authorize) => {
                let confirmation =
                    confirmation.ok_or(WalletDustRegistrationExecutorFailure::Unavailable)?;
                if !confirmation.confirmed {
                    return Ok(
                        WalletDustRegistrationOperationCompletion::authorization_rejected(
                            identity, draft_id,
                        ),
                    );
                }
                authorize
                    .execute(AuthorizeWalletDustRegistrationCommand {
                        profile_id: identity.profile.as_str().to_owned(),
                        draft_id: preview.draft_id.clone(),
                        authorization_challenge: preview.authorization_challenge,
                        confirmation,
                    })
                    .await
            }
            DustRegistrationAuthorization::AutomaticDevelopment(authorize) => {
                if identity.realm.as_str() != "undeployed" || preview.network_id != "undeployed" {
                    return Err(WalletDustRegistrationExecutorFailure::Unavailable);
                }
                authorize
                    .execute(AuthorizeDevelopmentWalletDustRegistrationCommand {
                        profile_id: identity.profile.as_str().to_owned(),
                        draft_id: preview.draft_id.clone(),
                        authorization_challenge: preview.authorization_challenge,
                    })
                    .await
            }
        }
        .map_err(map_registration_failure)?;
        if authorized.draft_id != draft_id.as_str() || !authorized.submission_ready {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        self.retained
            .lock()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
            .preview = Some(authorized);
        Ok(WalletDustRegistrationOperationCompletion::authorized(
            identity, draft_id,
        ))
    }

    async fn execute_submit(
        &self,
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
    ) -> Result<WalletDustRegistrationOperationCompletion, WalletDustRegistrationExecutorFailure>
    {
        self.validate_bound(&identity)?;
        if self
            .retained
            .lock()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
            .preview
            .is_none()
        {
            let RecoveredSubmission::Observed(transaction_id) =
                self.recover_submission(&identity, &draft_id).await?
            else {
                return Err(WalletDustRegistrationExecutorFailure::Degraded);
            };
            return Ok(WalletDustRegistrationOperationCompletion::submitted(
                identity,
                draft_id,
                transaction_id,
            ));
        }
        let preview = self
            .retained
            .lock()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
            .preview
            .clone()
            .ok_or(WalletDustRegistrationExecutorFailure::Degraded)?;
        if preview.draft_id != draft_id.as_str() || !preview.submission_ready {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        if self
            .retained
            .lock()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
            .submission_uncertain
        {
            match self.recover_submission(&identity, &draft_id).await? {
                RecoveredSubmission::Observed(transaction_id) => {
                    return Ok(WalletDustRegistrationOperationCompletion::submitted(
                        identity,
                        draft_id,
                        transaction_id,
                    ));
                }
                RecoveredSubmission::PreBroadcastCancelled => {
                    self.retained
                        .lock()
                        .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
                        .submission_uncertain = false;
                }
            }
        }
        // Fail closed unless the authorized draft was durably recorded before broadcast.
        if !self.durable.load(Ordering::Acquire)
            || !self.recovery_record(&identity)?.is_some_and(|record| {
                record.state
                    == oxid_wallet_application::WalletDustRegistrationSettlementState::Submitting
                    && record
                        .registration
                        .as_ref()
                        .is_some_and(|registration| registration.draft_id == draft_id.as_str())
            })
        {
            return Err(WalletDustRegistrationExecutorFailure::Unavailable);
        }
        // A failed or cancelled submit future can have broadcast before returning.
        self.retained
            .lock()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
            .submission_uncertain = true;
        let submitted = match &self.submit {
            DustRegistrationSubmission::Explicit(submit) => {
                submit
                    .execute(SubmitWalletDustRegistrationCommand {
                        profile_id: identity.profile.as_str().to_owned(),
                        draft_id: draft_id.as_str().to_owned(),
                        confirmation: continuation_confirmation(&preview),
                    })
                    .await
            }
            DustRegistrationSubmission::AutomaticDevelopment(submit) => {
                if identity.realm.as_str() != "undeployed" || preview.network_id != "undeployed" {
                    return Err(WalletDustRegistrationExecutorFailure::Unavailable);
                }
                submit
                    .execute(SubmitDevelopmentWalletDustRegistrationCommand {
                        profile_id: identity.profile.as_str().to_owned(),
                        draft_id: draft_id.as_str().to_owned(),
                    })
                    .await
            }
        };
        let submitted = match submitted {
            Ok(submitted) => submitted,
            Err(oxid_wallet_application::WalletDustRegistrationError::Operation(
                oxid_wallet_application::WalletDustRegistrationPortError::SubmissionOutcomeUnknown
                | oxid_wallet_application::WalletDustRegistrationPortError::SubmissionInProgress,
            )) => {
                let RecoveredSubmission::Observed(transaction_id) =
                    self.recover_submission(&identity, &draft_id).await?
                else {
                    return Err(WalletDustRegistrationExecutorFailure::Degraded);
                };
                self.retained
                    .lock()
                    .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
                    .finality_observed = false;
                return Ok(WalletDustRegistrationOperationCompletion::submitted(
                    identity,
                    draft_id,
                    transaction_id,
                ));
            }
            Err(error) => {
                use oxid_wallet_application::WalletDustRegistrationPortError as PortError;
                let outcome_is_uncertain = matches!(
                    &error,
                    oxid_wallet_application::WalletDustRegistrationError::Operation(
                        PortError::SubmissionOutcomeUnknown
                            | PortError::SubmissionInProgress
                            | PortError::Timeout
                    )
                );
                if !outcome_is_uncertain {
                    self.retained
                        .lock()
                        .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
                        .submission_uncertain = false;
                }
                return Err(map_registration_failure(error));
            }
        };
        if submitted.registration.draft_id != draft_id.as_str() {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        let transaction_id = ChainTransactionId::parse(submitted.transaction_id)
            .map_err(|_| WalletDustRegistrationExecutorFailure::Degraded)?;
        let mut retained = self
            .retained
            .lock()
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?;
        retained.preview = Some(submitted.registration);
        retained.submission_uncertain = false;
        retained.finality_observed = false;
        Ok(WalletDustRegistrationOperationCompletion::submitted(
            identity,
            draft_id,
            transaction_id,
        ))
    }

    async fn recover_submission(
        &self,
        identity: &WalletDustRegistrationSettlementIdentity,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<RecoveredSubmission, WalletDustRegistrationExecutorFailure> {
        let command = GetWalletDustRegistrationStatusCommand {
            profile_id: identity.profile.as_str().to_owned(),
            draft_id: draft_id.as_str().to_owned(),
        };
        let status = match self.status.execute(command.clone()).await {
            Ok(status) => status,
            Err(_) => self
                .reconcile
                .execute(ReconcileWalletDustRegistrationSubmissionCommand {
                    profile_id: command.profile_id,
                    draft_id: command.draft_id,
                })
                .await
                .map_err(map_registration_failure)?,
        };
        if status.state == "cancelled" && status.transaction_id.is_none() {
            return Ok(RecoveredSubmission::PreBroadcastCancelled);
        }
        let transaction_id = status
            .transaction_id
            .ok_or(WalletDustRegistrationExecutorFailure::Degraded)?;
        ChainTransactionId::parse(transaction_id)
            .map(RecoveredSubmission::Observed)
            .map_err(|_| WalletDustRegistrationExecutorFailure::Degraded)
    }

    async fn execute_observe(
        &self,
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
    ) -> Result<WalletDustRegistrationOperationCompletion, WalletDustRegistrationExecutorFailure>
    {
        self.validate_bound(&identity)?;
        let (draft_id, finality_observed) = {
            let retained = self
                .retained
                .lock()
                .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?;
            (
                retained
                    .preview
                    .as_ref()
                    .map(|preview| preview.draft_id.clone()),
                retained.finality_observed,
            )
        };
        let draft_id = draft_id
            .or_else(|| {
                self.recovery_record(&identity)
                    .ok()
                    .flatten()
                    .and_then(|record| {
                        record
                            .registration
                            .map(|registration| registration.draft_id)
                    })
            })
            .ok_or(WalletDustRegistrationExecutorFailure::Degraded)?;
        let status = self
            .status
            .execute(GetWalletDustRegistrationStatusCommand {
                profile_id: identity.profile.as_str().to_owned(),
                draft_id: draft_id.clone(),
            })
            .await;
        let status = match status {
            Ok(status) if status.state == "included" && !finality_observed => {
                let revision = self.next_revision()?;
                self.retained
                    .lock()
                    .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?
                    .finality_observed = true;
                return Ok(
                    WalletDustRegistrationOperationCompletion::finality_observed(
                        identity,
                        transaction_id,
                        revision,
                    ),
                );
            }
            Ok(status) if status.state == "included" => status,
            _ => self
                .reconcile
                .execute(ReconcileWalletDustRegistrationSubmissionCommand {
                    profile_id: identity.profile.as_str().to_owned(),
                    draft_id,
                })
                .await
                .map_err(map_registration_failure)?,
        };
        if status
            .transaction_id
            .as_deref()
            .is_some_and(|id| id != transaction_id.as_str())
        {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        let reconciliation = match status.state.as_str() {
            "included" => WalletDustRegistrationSettlementReconciliation::Included,
            "rejected" | "expired" | "cancelled" => {
                WalletDustRegistrationSettlementReconciliation::Dropped
            }
            _ => WalletDustRegistrationSettlementReconciliation::Pending,
        };
        Ok(WalletDustRegistrationOperationCompletion::reconciled(
            identity,
            transaction_id,
            self.next_revision()?,
            reconciliation,
        ))
    }

    async fn execute_refresh(
        &self,
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
    ) -> Result<WalletDustRegistrationOperationCompletion, WalletDustRegistrationExecutorFailure>
    {
        let previous = self.validate_bound(&identity)?;
        let refreshed = self
            .sync_selected_realm
            .execute(SelectedWalletRealmSyncCommand {
                profile_id: identity.profile.as_str().to_owned(),
            })
            .await
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?;
        if refreshed.identity != previous.identity || refreshed.generation != previous.generation {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        let ready = matches!(
            &refreshed.view.dust,
            WalletRealmFamilyView::Ready(dust) if dust.state == "synced"
        );
        let after_observation_revision = refreshed.revision;
        self.bind(refreshed)
            .map_err(|_| WalletDustRegistrationExecutorFailure::Unavailable)?;
        Ok(WalletDustRegistrationOperationCompletion::dust_refreshed(
            identity,
            transaction_id,
            self.next_revision()?,
            after_observation_revision,
            ready,
        ))
    }
}

impl ExecuteWalletDustRegistrationOperation for ComposedDustRegistrationExecutor {
    fn execute(
        &self,
        operation: WalletDustRegistrationRuntimeOperation,
    ) -> WalletDustRegistrationOperationFuture<'_> {
        Box::pin(async move {
            let future = async {
                match operation {
                    WalletDustRegistrationRuntimeOperation::Prepare(
                        WalletDustRegistrationEffect::Prepare { identity },
                    ) => self.execute_prepare(identity).await,
                    WalletDustRegistrationRuntimeOperation::RequestProtectedAuthorization(
                        WalletDustRegistrationEffect::RequestProtectedAuthorization {
                            identity,
                            draft_id,
                        },
                    ) => self.execute_authorize(identity, draft_id).await,
                    WalletDustRegistrationRuntimeOperation::Submit(
                        WalletDustRegistrationEffect::Submit { identity, draft_id },
                    ) => self.execute_submit(identity, draft_id).await,
                    WalletDustRegistrationRuntimeOperation::ObserveTransaction(
                        WalletDustRegistrationEffect::ObserveTransaction {
                            identity,
                            transaction_id,
                        },
                    ) => self.execute_observe(identity, transaction_id).await,
                    WalletDustRegistrationRuntimeOperation::RefreshDust(
                        WalletDustRegistrationEffect::RefreshDust {
                            identity,
                            transaction_id,
                        },
                    ) => self.execute_refresh(identity, transaction_id).await,
                    _ => Err(WalletDustRegistrationExecutorFailure::Degraded),
                }
            };
            // On a Tokio host every admitted operation has a wall-clock deadline.
            // Pure executor tests without a reactor use scripted timeout failures.
            if tokio::runtime::Handle::try_current().is_ok() {
                tokio::time::timeout(self.operation_deadline, future)
                    .await
                    .unwrap_or(Err(WalletDustRegistrationExecutorFailure::TimedOut))
            } else {
                future.await
            }
        })
    }
}

fn selected_realm_is_eligible(selected: &SelectedWalletRealmProjection) -> bool {
    matches!(&selected.view.account, WalletRealmFamilyView::Ready(account) if
    account.network_id == selected.identity.realm.as_str()
        && account.source == "live"
        && account.sync.state == "synced"
        && account.balances.iter().any(|balance| {
            balance.asset_id == "midnight:night"
                && balance.atomic_units.parse::<u128>().is_ok_and(|value| value > 0)
        }))
}

fn settlement_identity(
    selected: &SelectedWalletRealmProjection,
) -> WalletDustRegistrationSettlementIdentity {
    WalletDustRegistrationSettlementIdentity {
        profile: selected.identity.profile.clone(),
        realm: selected.identity.realm.clone(),
        generation: selected.generation,
    }
}

fn is_recoverable_state(
    state: oxid_wallet_application::WalletDustRegistrationSettlementState,
) -> bool {
    matches!(
        state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Offline
            | oxid_wallet_application::WalletDustRegistrationSettlementState::TimedOut
            | oxid_wallet_application::WalletDustRegistrationSettlementState::Degraded
            | oxid_wallet_application::WalletDustRegistrationSettlementState::Suspended
    )
}

fn continuation_confirmation(
    preview: &WalletDustRegistrationPreviewView,
) -> SensitiveOperationConfirmation {
    SensitiveOperationConfirmation {
        title: "Complete DUST registration".to_owned(),
        summary: format!(
            "Submit the authorized DUST registration {} on {}.",
            preview.draft_id, preview.network_id
        ),
        confirmed: true,
    }
}

fn map_registration_failure(
    error: oxid_wallet_application::WalletDustRegistrationError,
) -> WalletDustRegistrationExecutorFailure {
    use oxid_wallet_application::WalletDustRegistrationPortError as PortError;
    match error {
        oxid_wallet_application::WalletDustRegistrationError::Operation(PortError::Timeout) => {
            WalletDustRegistrationExecutorFailure::TimedOut
        }
        oxid_wallet_application::WalletDustRegistrationError::Operation(
            PortError::SubmissionOutcomeUnknown,
        ) => WalletDustRegistrationExecutorFailure::Degraded,
        oxid_wallet_application::WalletDustRegistrationError::Operation(PortError::Unavailable) => {
            WalletDustRegistrationExecutorFailure::Unavailable
        }
        oxid_wallet_application::WalletDustRegistrationError::Operation(
            PortError::ProtectionNotInitialized,
        ) => WalletDustRegistrationExecutorFailure::ProtectionNotInitialized,
        oxid_wallet_application::WalletDustRegistrationError::Operation(
            PortError::ProtectionLocked,
        ) => WalletDustRegistrationExecutorFailure::ProtectionLocked,
        _ => WalletDustRegistrationExecutorFailure::Degraded,
    }
}

#[cfg(test)]
#[path = "dust_settlement_tests.rs"]
mod tests;
