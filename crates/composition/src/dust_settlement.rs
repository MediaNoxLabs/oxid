// SPDX-License-Identifier: Apache-2.0

//! Composition-owned DUST registration settlement.
//!
//! Incoming adapters observe one presentation-safe projection and provide one
//! explicit consent record. The exact preview, transaction observation, realm
//! refresh, and stale-generation checks remain owned by composition.

use std::sync::{Arc, Mutex};

use tokio::sync::watch;

use oxid_wallet_application::{
    AuthorizeWalletDustRegistrationCommand, AuthorizeWalletDustRegistrationUseCase,
    ChainTransactionId, ExecuteWalletDustRegistrationOperation, GetSelectedWalletRealmSyncUseCase,
    GetWalletDustRegistrationStatusCommand, GetWalletDustRegistrationStatusUseCase,
    PrepareWalletDustRegistrationCommand, PrepareWalletDustRegistrationUseCase,
    ReconcileWalletDustRegistrationSubmissionCommand,
    ReconcileWalletDustRegistrationSubmissionUseCase, SelectedWalletRealmActionReadiness,
    SelectedWalletRealmProjection, SelectedWalletRealmSyncCommand, SensitiveOperationConfirmation,
    SubmitWalletDustRegistrationCommand, SubmitWalletDustRegistrationUseCase,
    SyncSelectedWalletRealmUseCase, WalletDustRegistrationDriver,
    WalletDustRegistrationDriverError, WalletDustRegistrationEffect,
    WalletDustRegistrationExecutorFailure, WalletDustRegistrationOperationCompletion,
    WalletDustRegistrationOperationFuture, WalletDustRegistrationPreviewView,
    WalletDustRegistrationRuntimeOperation, WalletDustRegistrationSettlementEvent,
    WalletDustRegistrationSettlementIdentity, WalletDustRegistrationSettlementProjection,
    WalletDustRegistrationSettlementReconciliation, WalletRealmFamilyView,
    WalletTransactionDraftId,
};

/// Composition-owned capability shared by headless and graphical adapters.
pub struct WalletDustSettlementCapability {
    selected_realm: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
    executor: Arc<ComposedDustRegistrationExecutor>,
    driver: WalletDustRegistrationDriver,
    projections: watch::Sender<WalletDustRegistrationSettlementProjection>,
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
    pub(crate) fn new(
        selected_realm: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
        sync_selected_realm: Arc<dyn SyncSelectedWalletRealmUseCase>,
        prepare: Arc<dyn PrepareWalletDustRegistrationUseCase>,
        authorize: Arc<dyn AuthorizeWalletDustRegistrationUseCase>,
        submit: Arc<dyn SubmitWalletDustRegistrationUseCase>,
        status: Arc<dyn GetWalletDustRegistrationStatusUseCase>,
        reconcile: Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase>,
    ) -> Self {
        let executor = Arc::new(ComposedDustRegistrationExecutor {
            selected_realm: Arc::clone(&selected_realm),
            sync_selected_realm,
            prepare,
            authorize,
            submit,
            status,
            reconcile,
            retained: Mutex::new(RetainedSettlement::default()),
        });
        let operation_executor: Arc<dyn ExecuteWalletDustRegistrationOperation> = executor.clone();
        let (projections, _) =
            watch::channel(WalletDustRegistrationSettlementProjection::default());
        let projection_sink = projections.clone();
        let observer: oxid_wallet_application::WalletDustRegistrationProjectionObserver =
            Arc::new(move |projection| {
                projection_sink.send_replace(projection);
            });
        Self {
            selected_realm,
            executor,
            driver: WalletDustRegistrationDriver::with_projection_observer(
                operation_executor,
                observer,
            ),
            projections,
        }
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
        if let Some(active) = current.identity.filter(|active| active != &identity) {
            self.driver
                .advance(WalletDustRegistrationSettlementEvent::Superseded { identity: active })
                .await
                .map_err(WalletDustSettlementError::Driver)?;
        }
        self.executor.bind(selected)?;
        self.driver
            .advance(WalletDustRegistrationSettlementEvent::Eligibility {
                identity,
                revision: self.executor.bound_revision()?,
                eligible,
            })
            .await
            .map_err(WalletDustSettlementError::Driver)
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
        result
    }

    /// Retries only a retained recoverable operation. It cannot create a
    /// replacement registration or cross a fresh authorization boundary.
    pub async fn retry(
        &self,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustSettlementError> {
        let projection = self.projection()?;
        if !matches!(
            projection.state,
            oxid_wallet_application::WalletDustRegistrationSettlementState::TimedOut
                | oxid_wallet_application::WalletDustRegistrationSettlementState::Degraded
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
        self.driver
            .advance(WalletDustRegistrationSettlementEvent::Retry { identity, revision })
            .await
            .map_err(WalletDustSettlementError::Driver)
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

    /// Observes ordered public projection changes without transferring
    /// scheduling or settlement policy to an incoming adapter.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<WalletDustRegistrationSettlementProjection> {
        self.projections.subscribe()
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
    operation_revision: u64,
}

struct ComposedDustRegistrationExecutor {
    selected_realm: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
    sync_selected_realm: Arc<dyn SyncSelectedWalletRealmUseCase>,
    prepare: Arc<dyn PrepareWalletDustRegistrationUseCase>,
    authorize: Arc<dyn AuthorizeWalletDustRegistrationUseCase>,
    submit: Arc<dyn SubmitWalletDustRegistrationUseCase>,
    status: Arc<dyn GetWalletDustRegistrationStatusUseCase>,
    reconcile: Arc<dyn ReconcileWalletDustRegistrationSubmissionUseCase>,
    retained: Mutex<RetainedSettlement>,
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

    fn execute_prepare(
        &self,
        identity: WalletDustRegistrationSettlementIdentity,
    ) -> Result<WalletDustRegistrationOperationCompletion, WalletDustRegistrationExecutorFailure>
    {
        let bound = self.validate_bound(&identity)?;
        let preview = self
            .prepare
            .execute(PrepareWalletDustRegistrationCommand {
                profile_id: identity.profile.as_str().to_owned(),
            })
            .map_err(map_registration_failure)?;
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

    fn execute_authorize(
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
            let confirmation = retained
                .confirmation
                .take()
                .ok_or(WalletDustRegistrationExecutorFailure::Unavailable)?;
            (preview, confirmation)
        };
        if preview.draft_id != draft_id.as_str() {
            return Err(WalletDustRegistrationExecutorFailure::Degraded);
        }
        if !confirmation.confirmed {
            return Ok(
                WalletDustRegistrationOperationCompletion::authorization_rejected(
                    identity, draft_id,
                ),
            );
        }
        let authorized = self
            .authorize
            .execute(AuthorizeWalletDustRegistrationCommand {
                profile_id: identity.profile.as_str().to_owned(),
                draft_id: preview.draft_id.clone(),
                authorization_challenge: preview.authorization_challenge,
                confirmation,
            })
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
        let submitted = self
            .submit
            .execute(SubmitWalletDustRegistrationCommand {
                profile_id: identity.profile.as_str().to_owned(),
                draft_id: draft_id.as_str().to_owned(),
                confirmation: continuation_confirmation(&preview),
            })
            .await;
        let submitted = match submitted {
            Ok(submitted) => submitted,
            Err(oxid_wallet_application::WalletDustRegistrationError::Operation(
                oxid_wallet_application::WalletDustRegistrationPortError::SubmissionOutcomeUnknown
                | oxid_wallet_application::WalletDustRegistrationPortError::SubmissionInProgress,
            )) => {
                let transaction_id = self
                    .recover_submitted_transaction(&identity, &draft_id)
                    .await?;
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
            Err(error) => return Err(map_registration_failure(error)),
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
        retained.finality_observed = false;
        Ok(WalletDustRegistrationOperationCompletion::submitted(
            identity,
            draft_id,
            transaction_id,
        ))
    }

    async fn recover_submitted_transaction(
        &self,
        identity: &WalletDustRegistrationSettlementIdentity,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<ChainTransactionId, WalletDustRegistrationExecutorFailure> {
        let command = GetWalletDustRegistrationStatusCommand {
            profile_id: identity.profile.as_str().to_owned(),
            draft_id: draft_id.as_str().to_owned(),
        };
        let status = match self.status.execute(command.clone()) {
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
        let transaction_id = status
            .transaction_id
            .ok_or(WalletDustRegistrationExecutorFailure::Degraded)?;
        ChainTransactionId::parse(transaction_id)
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
                    .map(|preview| preview.draft_id.clone())
                    .ok_or(WalletDustRegistrationExecutorFailure::Degraded)?,
                retained.finality_observed,
            )
        };
        let status = self.status.execute(GetWalletDustRegistrationStatusCommand {
            profile_id: identity.profile.as_str().to_owned(),
            draft_id: draft_id.clone(),
        });
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
            match operation {
                WalletDustRegistrationRuntimeOperation::Prepare(
                    WalletDustRegistrationEffect::Prepare { identity },
                ) => self.execute_prepare(identity),
                WalletDustRegistrationRuntimeOperation::RequestProtectedAuthorization(
                    WalletDustRegistrationEffect::RequestProtectedAuthorization {
                        identity,
                        draft_id,
                    },
                ) => self.execute_authorize(identity, draft_id),
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
        })
    }
}

fn selected_realm_is_eligible(selected: &SelectedWalletRealmProjection) -> bool {
    selected.fresh
        && selected.consistent
        && selected.actionable == SelectedWalletRealmActionReadiness::Ready
        && matches!(&selected.view.account, WalletRealmFamilyView::Ready(account) if
        account.network_id == selected.identity.realm.as_str()
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
        _ => WalletDustRegistrationExecutorFailure::Degraded,
    }
}

#[cfg(test)]
#[path = "dust_settlement_tests.rs"]
mod tests;
