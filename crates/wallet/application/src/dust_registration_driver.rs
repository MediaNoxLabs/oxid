// SPDX-License-Identifier: Apache-2.0

//! Application-owned execution of the DUST registration settlement runtime.
//!
//! The driver keeps presentation adapters outside workflow policy. It advances
//! preparation only as far as protected authorization, then requires one
//! explicit [`WalletDustRegistrationDriver::authorize`] call before draining
//! submission and settlement effects.
//!
//! | Command | Accepted event | Result |
//! | --- | --- | --- |
//! | `advance(Eligibility)` | prepared draft | stop at authorization |
//! | `authorize()` | authorized | submit, observe, and refresh automatically |
//! | either | duplicate/stale completion | reject and release admission |
//! | `advance(recovery event)` | cancellation/offline/timeout/suspend | retain projection |
//! | any | superseding generation | stale work cannot complete |

use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use oxid_wallet_domain::{ChainTransactionId, WalletTransactionDraftId};

use crate::{
    WalletDustRegistrationEffect, WalletDustRegistrationRuntime,
    WalletDustRegistrationRuntimeAdmission, WalletDustRegistrationRuntimeOperation,
    WalletDustRegistrationSettlementEvent, WalletDustRegistrationSettlementIdentity,
    WalletDustRegistrationSettlementProjection, WalletDustRegistrationSettlementReconciliation,
    completion,
};

const MAX_DRAINED_OPERATIONS: usize = 8;

/// Bounded failures returned by a concrete operation executor.
///
/// Adapter error payloads deliberately do not cross this boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationExecutorFailure {
    Unavailable,
    Offline,
    TimedOut,
    Cancelled,
    ProtectionNotInitialized,
    ProtectionLocked,
    Degraded,
}

impl fmt::Display for WalletDustRegistrationExecutorFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "DUST registration operation is unavailable",
            Self::Offline => "DUST registration operation is offline",
            Self::TimedOut => "DUST registration operation timed out",
            Self::Cancelled => "DUST registration operation was cancelled",
            Self::ProtectionNotInitialized => {
                "DUST registration requires wallet recovery or initialization"
            }
            Self::ProtectionLocked => "DUST registration requires wallet unlock",
            Self::Degraded => "DUST registration operation is degraded",
        })
    }
}

impl Error for WalletDustRegistrationExecutorFailure {}

/// Exact public completion values an operation executor may return.
///
/// The inner event stays private so adapters cannot inject lifecycle or
/// supersession observations through the operation-completion channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationOperationCompletion(WalletDustRegistrationSettlementEvent);

impl WalletDustRegistrationOperationCompletion {
    #[must_use]
    pub fn prepared(
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
        revision: u64,
    ) -> Self {
        Self(completion::prepared(identity, draft_id, revision))
    }

    #[must_use]
    pub fn already_current(
        identity: WalletDustRegistrationSettlementIdentity,
        revision: u64,
    ) -> Self {
        Self(completion::already_current(identity, revision))
    }

    #[must_use]
    pub fn authorized(
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
    ) -> Self {
        Self(completion::authorized(identity, draft_id))
    }

    #[must_use]
    pub fn authorization_rejected(
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
    ) -> Self {
        Self(completion::authorization_rejected(identity, draft_id))
    }

    #[must_use]
    pub fn submitted(
        identity: WalletDustRegistrationSettlementIdentity,
        draft_id: WalletTransactionDraftId,
        transaction_id: ChainTransactionId,
    ) -> Self {
        Self(completion::submitted(identity, draft_id, transaction_id))
    }

    #[must_use]
    pub fn finality_observed(
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
        revision: u64,
    ) -> Self {
        Self(completion::finality_observed(
            identity,
            transaction_id,
            revision,
        ))
    }

    #[must_use]
    pub fn reconciled(
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
        revision: u64,
        reconciliation: WalletDustRegistrationSettlementReconciliation,
    ) -> Self {
        Self(completion::reconciled(
            identity,
            transaction_id,
            revision,
            reconciliation,
        ))
    }

    #[must_use]
    pub fn dropped_registration_abandoned(
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
        after_observation_revision: u64,
    ) -> Self {
        Self(completion::dropped_registration_abandoned(
            identity,
            transaction_id,
            after_observation_revision,
        ))
    }

    #[must_use]
    pub fn dust_refreshed(
        identity: WalletDustRegistrationSettlementIdentity,
        transaction_id: ChainTransactionId,
        revision: u64,
        after_observation_revision: u64,
        ready: bool,
    ) -> Self {
        Self(completion::dust_refreshed(
            identity,
            transaction_id,
            revision,
            after_observation_revision,
            ready,
        ))
    }

    fn into_event(self) -> WalletDustRegistrationSettlementEvent {
        self.0
    }
}

/// One operation future. Concrete composition adapters may perform blocking
/// work on their own reviewed worker before resolving this future.
pub type WalletDustRegistrationOperationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    WalletDustRegistrationOperationCompletion,
                    WalletDustRegistrationExecutorFailure,
                >,
            > + Send
            + 'a,
    >,
>;

/// Executes one already-admitted application operation.
pub trait ExecuteWalletDustRegistrationOperation: Send + Sync {
    fn execute(
        &self,
        operation: WalletDustRegistrationRuntimeOperation,
    ) -> WalletDustRegistrationOperationFuture<'_>;
}

/// Presentation-neutral observer notified after each accepted projection
/// transition. Observers receive only the public settlement projection and do
/// not participate in workflow policy or effect execution.
pub type WalletDustRegistrationProjectionObserver =
    Arc<dyn Fn(WalletDustRegistrationSettlementProjection) + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationDriverError {
    Poisoned,
    Busy,
    AuthorizationNotPending,
    InvalidObservation,
    InvalidCompletion,
    DrainLimit,
    Executor(WalletDustRegistrationExecutorFailure),
}

impl fmt::Display for WalletDustRegistrationDriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poisoned => formatter.write_str("DUST registration state is unavailable"),
            Self::Busy => {
                formatter.write_str("DUST registration driver is busy; retry the command")
            }
            Self::AuthorizationNotPending => {
                formatter.write_str("DUST registration authorization is not pending")
            }
            Self::InvalidObservation => {
                formatter.write_str("DUST registration observation must come from its executor")
            }
            Self::InvalidCompletion => {
                formatter.write_str("DUST registration completion is stale or invalid")
            }
            Self::DrainLimit => formatter.write_str("DUST registration drain limit reached"),
            Self::Executor(error) => error.fmt(formatter),
        }
    }
}

impl Error for WalletDustRegistrationDriverError {}

/// Single-flight application service around [`WalletDustRegistrationRuntime`].
pub struct WalletDustRegistrationDriver {
    runtime: Mutex<WalletDustRegistrationRuntime>,
    executor: Arc<dyn ExecuteWalletDustRegistrationOperation>,
    observer: Option<WalletDustRegistrationProjectionObserver>,
    driving: AtomicBool,
}

impl WalletDustRegistrationDriver {
    #[must_use]
    pub fn new(executor: Arc<dyn ExecuteWalletDustRegistrationOperation>) -> Self {
        Self::with_runtime_and_observer(executor, WalletDustRegistrationRuntime::default(), None)
    }

    #[must_use]
    pub fn with_projection_observer(
        executor: Arc<dyn ExecuteWalletDustRegistrationOperation>,
        observer: WalletDustRegistrationProjectionObserver,
    ) -> Self {
        Self::with_runtime_and_observer(
            executor,
            WalletDustRegistrationRuntime::default(),
            Some(observer),
        )
    }

    #[must_use]
    pub fn with_runtime(
        executor: Arc<dyn ExecuteWalletDustRegistrationOperation>,
        runtime: WalletDustRegistrationRuntime,
    ) -> Self {
        Self::with_runtime_and_observer(executor, runtime, None)
    }

    fn with_runtime_and_observer(
        executor: Arc<dyn ExecuteWalletDustRegistrationOperation>,
        runtime: WalletDustRegistrationRuntime,
        observer: Option<WalletDustRegistrationProjectionObserver>,
    ) -> Self {
        Self {
            runtime: Mutex::new(runtime),
            executor,
            observer,
            driving: AtomicBool::new(false),
        }
    }

    fn publish(&self, projection: &WalletDustRegistrationSettlementProjection) {
        if let Some(observer) = &self.observer {
            observer(projection.clone());
        }
    }

    pub fn projection(
        &self,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustRegistrationDriverError> {
        self.runtime
            .lock()
            .map(|runtime| runtime.coordinator().projection().clone())
            .map_err(|_| WalletDustRegistrationDriverError::Poisoned)
    }

    /// Accepts an external eligibility/lifecycle/recovery observation and
    /// advances preparation, but never crosses protected authorization.
    pub async fn advance(
        &self,
        observation: WalletDustRegistrationSettlementEvent,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustRegistrationDriverError> {
        if is_executor_completion(&observation) {
            return Err(WalletDustRegistrationDriverError::InvalidObservation);
        }
        let (previous, observed) = {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| WalletDustRegistrationDriverError::Poisoned)?;
            let previous = runtime.coordinator().projection().clone();
            runtime.observe(observation);
            (previous, runtime.coordinator().projection().clone())
        };
        if observed != previous {
            self.publish(&observed);
        }
        self.drain(None).await
    }

    /// Executes the exact pending protected authorization and, only after it
    /// succeeds, drains the remaining non-interactive settlement effects.
    pub async fn authorize(
        &self,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustRegistrationDriverError> {
        let authorization_target = self
            .runtime
            .lock()
            .map_err(|_| WalletDustRegistrationDriverError::Poisoned)?
            .coordinator()
            .active_effect()
            .cloned()
            .filter(|effect| {
                matches!(
                    effect,
                    WalletDustRegistrationEffect::RequestProtectedAuthorization { .. }
                )
            });
        let Some(authorization_target) = authorization_target else {
            return Err(WalletDustRegistrationDriverError::AuthorizationNotPending);
        };
        self.drain(Some(authorization_target)).await
    }

    async fn drain(
        &self,
        authorization_target: Option<WalletDustRegistrationEffect>,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustRegistrationDriverError> {
        if self
            .driving
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return if authorization_target.is_some() {
                Err(WalletDustRegistrationDriverError::Busy)
            } else {
                self.projection()
            };
        }
        let _driver_admission = WalletDustRegistrationDriverAdmission(&self.driving);

        let mut authorization_target = authorization_target;
        for _ in 0..MAX_DRAINED_OPERATIONS {
            let admission = {
                let mut runtime = self
                    .runtime
                    .lock()
                    .map_err(|_| WalletDustRegistrationDriverError::Poisoned)?;
                let Some(effect) = runtime.coordinator().active_effect().cloned() else {
                    return Ok(runtime.coordinator().projection().clone());
                };
                match &authorization_target {
                    Some(target) if &effect != target => {
                        return Err(WalletDustRegistrationDriverError::AuthorizationNotPending);
                    }
                    None if matches!(
                        effect,
                        WalletDustRegistrationEffect::RequestProtectedAuthorization { .. }
                    ) =>
                    {
                        return Ok(runtime.coordinator().projection().clone());
                    }
                    _ => {}
                }
                runtime.admit_current(&effect)
            };

            let WalletDustRegistrationRuntimeAdmission::Admitted { operation, token } = admission
            else {
                return match admission {
                    WalletDustRegistrationRuntimeAdmission::Busy
                    | WalletDustRegistrationRuntimeAdmission::Idle => self.projection(),
                    WalletDustRegistrationRuntimeAdmission::Stale => {
                        Err(WalletDustRegistrationDriverError::InvalidCompletion)
                    }
                    WalletDustRegistrationRuntimeAdmission::Admitted { .. } => unreachable!(),
                };
            };

            let expected_operation = operation.clone();
            let mut runtime_admission =
                WalletDustRegistrationRuntimeAdmissionGuard::new(&self.runtime, token);
            let completion = self.executor.execute(operation).await;
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| WalletDustRegistrationDriverError::Poisoned)?;
            match completion {
                Ok(completion) => {
                    let pause_after_completion = completion.requests_pause();
                    let event = completion.into_event();
                    if !completion_matches_operation(&expected_operation, &event) {
                        let _ = runtime.release(token);
                        runtime_admission.disarm();
                        return Err(WalletDustRegistrationDriverError::InvalidCompletion);
                    }
                    let previous = runtime.coordinator().projection().clone();
                    if !runtime.complete(token, event) {
                        let still_owned = runtime.release(token);
                        runtime_admission.disarm();
                        if authorization_target.is_some() || still_owned {
                            return Err(WalletDustRegistrationDriverError::InvalidCompletion);
                        }
                        continue;
                    }
                    runtime_admission.disarm();
                    let projection = runtime.coordinator().projection().clone();
                    drop(runtime);
                    if projection != previous {
                        self.publish(&projection);
                    }
                    authorization_target = None;
                    if pause_after_completion {
                        return Ok(projection);
                    }
                }
                Err(error) => {
                    let _ = runtime.release(token);
                    runtime_admission.disarm();
                    return Err(WalletDustRegistrationDriverError::Executor(error));
                }
            }
        }

        let runtime = self
            .runtime
            .lock()
            .map_err(|_| WalletDustRegistrationDriverError::Poisoned)?;
        if drain_is_quiescent(runtime.coordinator().active_effect()) {
            Ok(runtime.coordinator().projection().clone())
        } else {
            Err(WalletDustRegistrationDriverError::DrainLimit)
        }
    }
}

impl WalletDustRegistrationOperationCompletion {
    fn requests_pause(&self) -> bool {
        matches!(
            &self.0,
            WalletDustRegistrationSettlementEvent::RegistrationReconciled {
                reconciliation: WalletDustRegistrationSettlementReconciliation::Pending,
                ..
            } | WalletDustRegistrationSettlementEvent::DustRefreshed { ready: false, .. }
        )
    }
}

fn drain_is_quiescent(effect: Option<&WalletDustRegistrationEffect>) -> bool {
    effect.is_none_or(|effect| {
        matches!(
            effect,
            WalletDustRegistrationEffect::RequestProtectedAuthorization { .. }
        )
    })
}

struct WalletDustRegistrationDriverAdmission<'a>(&'a AtomicBool);

impl Drop for WalletDustRegistrationDriverAdmission<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct WalletDustRegistrationRuntimeAdmissionGuard<'a> {
    runtime: &'a Mutex<WalletDustRegistrationRuntime>,
    token: Option<crate::WalletDustRegistrationRuntimeAdmissionToken>,
}

impl<'a> WalletDustRegistrationRuntimeAdmissionGuard<'a> {
    fn new(
        runtime: &'a Mutex<WalletDustRegistrationRuntime>,
        token: crate::WalletDustRegistrationRuntimeAdmissionToken,
    ) -> Self {
        Self {
            runtime,
            token: Some(token),
        }
    }

    fn disarm(&mut self) {
        self.token = None;
    }
}

impl Drop for WalletDustRegistrationRuntimeAdmissionGuard<'_> {
    fn drop(&mut self) {
        let Some(token) = self.token.take() else {
            return;
        };
        if let Ok(mut runtime) = self.runtime.lock() {
            let _ = runtime.release(token);
        }
    }
}

fn is_executor_completion(event: &WalletDustRegistrationSettlementEvent) -> bool {
    matches!(
        event,
        WalletDustRegistrationSettlementEvent::AuthorizationRequested { .. }
            | WalletDustRegistrationSettlementEvent::RegistrationAlreadyCurrent { .. }
            | WalletDustRegistrationSettlementEvent::AuthorizationSucceeded { .. }
            | WalletDustRegistrationSettlementEvent::AuthorizationRejected { .. }
            | WalletDustRegistrationSettlementEvent::SubmissionAccepted { .. }
            | WalletDustRegistrationSettlementEvent::FinalityObserved { .. }
            | WalletDustRegistrationSettlementEvent::RegistrationReconciled { .. }
            | WalletDustRegistrationSettlementEvent::DustRefreshed { .. }
            | WalletDustRegistrationSettlementEvent::DroppedRegistrationAbandoned { .. }
    )
}

fn completion_matches_operation(
    operation: &WalletDustRegistrationRuntimeOperation,
    event: &WalletDustRegistrationSettlementEvent,
) -> bool {
    match (operation, event) {
        (
            WalletDustRegistrationRuntimeOperation::Prepare(
                WalletDustRegistrationEffect::Prepare { identity: expected },
            ),
            WalletDustRegistrationSettlementEvent::AuthorizationRequested { identity, .. },
        )
        | (
            WalletDustRegistrationRuntimeOperation::Prepare(
                WalletDustRegistrationEffect::Prepare { identity: expected },
            ),
            WalletDustRegistrationSettlementEvent::RegistrationAlreadyCurrent { identity, .. },
        ) => identity == expected,
        (
            WalletDustRegistrationRuntimeOperation::RequestProtectedAuthorization(
                WalletDustRegistrationEffect::RequestProtectedAuthorization {
                    identity: expected_identity,
                    draft_id: expected_draft,
                },
            ),
            WalletDustRegistrationSettlementEvent::AuthorizationSucceeded { identity, draft_id }
            | WalletDustRegistrationSettlementEvent::AuthorizationRejected { identity, draft_id },
        ) => identity == expected_identity && draft_id == expected_draft,
        (
            WalletDustRegistrationRuntimeOperation::Submit(WalletDustRegistrationEffect::Submit {
                identity: expected_identity,
                draft_id: expected_draft,
            }),
            WalletDustRegistrationSettlementEvent::SubmissionAccepted {
                identity, draft_id, ..
            },
        ) => identity == expected_identity && draft_id == expected_draft,
        (
            WalletDustRegistrationRuntimeOperation::ObserveTransaction(
                WalletDustRegistrationEffect::ObserveTransaction {
                    identity: expected_identity,
                    transaction_id: expected_transaction,
                },
            ),
            WalletDustRegistrationSettlementEvent::FinalityObserved {
                identity,
                transaction_id,
                ..
            }
            | WalletDustRegistrationSettlementEvent::RegistrationReconciled {
                identity,
                transaction_id,
                ..
            }
            | WalletDustRegistrationSettlementEvent::DroppedRegistrationAbandoned {
                identity,
                transaction_id,
                ..
            },
        ) => identity == expected_identity && transaction_id == expected_transaction,
        (
            WalletDustRegistrationRuntimeOperation::RefreshDust(
                WalletDustRegistrationEffect::RefreshDust {
                    identity: expected_identity,
                    transaction_id: expected_transaction,
                },
            ),
            WalletDustRegistrationSettlementEvent::DustRefreshed {
                identity,
                transaction_id,
                ..
            },
        ) => identity == expected_identity && transaction_id == expected_transaction,
        _ => false,
    }
}

#[cfg(test)]
#[path = "dust_registration_driver_tests.rs"]
mod tests;
