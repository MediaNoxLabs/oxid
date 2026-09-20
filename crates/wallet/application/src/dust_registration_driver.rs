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
    sync::{Arc, Mutex},
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
    Degraded,
}

impl fmt::Display for WalletDustRegistrationExecutorFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "DUST registration operation is unavailable",
            Self::Offline => "DUST registration operation is offline",
            Self::TimedOut => "DUST registration operation timed out",
            Self::Cancelled => "DUST registration operation was cancelled",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationDriverError {
    Poisoned,
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
}

impl WalletDustRegistrationDriver {
    #[must_use]
    pub fn new(executor: Arc<dyn ExecuteWalletDustRegistrationOperation>) -> Self {
        Self::with_runtime(executor, WalletDustRegistrationRuntime::default())
    }

    #[must_use]
    pub fn with_runtime(
        executor: Arc<dyn ExecuteWalletDustRegistrationOperation>,
        runtime: WalletDustRegistrationRuntime,
    ) -> Self {
        Self {
            runtime: Mutex::new(runtime),
            executor,
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
        self.runtime
            .lock()
            .map_err(|_| WalletDustRegistrationDriverError::Poisoned)?
            .observe(observation);
        self.drain(false).await
    }

    /// Executes the exact pending protected authorization and, only after it
    /// succeeds, drains the remaining non-interactive settlement effects.
    pub async fn authorize(
        &self,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustRegistrationDriverError> {
        let authorization_pending = self
            .runtime
            .lock()
            .map_err(|_| WalletDustRegistrationDriverError::Poisoned)?
            .coordinator()
            .active_effect()
            .is_some_and(|effect| {
                matches!(
                    effect,
                    WalletDustRegistrationEffect::RequestProtectedAuthorization { .. }
                )
            });
        if !authorization_pending {
            return Err(WalletDustRegistrationDriverError::AuthorizationNotPending);
        }
        self.drain(true).await
    }

    async fn drain(
        &self,
        allow_authorization: bool,
    ) -> Result<WalletDustRegistrationSettlementProjection, WalletDustRegistrationDriverError> {
        for _ in 0..MAX_DRAINED_OPERATIONS {
            let admission = {
                let mut runtime = self
                    .runtime
                    .lock()
                    .map_err(|_| WalletDustRegistrationDriverError::Poisoned)?;
                let Some(effect) = runtime.coordinator().active_effect().cloned() else {
                    return Ok(runtime.coordinator().projection().clone());
                };
                if !allow_authorization
                    && matches!(
                        effect,
                        WalletDustRegistrationEffect::RequestProtectedAuthorization { .. }
                    )
                {
                    return Ok(runtime.coordinator().projection().clone());
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

            let completion = self.executor.execute(operation).await;
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| WalletDustRegistrationDriverError::Poisoned)?;
            match completion {
                Ok(completion) => {
                    if !runtime.complete(token, completion.into_event()) {
                        let _ = runtime.release(token);
                        return Err(WalletDustRegistrationDriverError::InvalidCompletion);
                    }
                }
                Err(error) => {
                    let _ = runtime.release(token);
                    return Err(WalletDustRegistrationDriverError::Executor(error));
                }
            }
        }

        Err(WalletDustRegistrationDriverError::DrainLimit)
    }
}

fn is_executor_completion(event: &WalletDustRegistrationSettlementEvent) -> bool {
    matches!(
        event,
        WalletDustRegistrationSettlementEvent::AuthorizationRequested { .. }
            | WalletDustRegistrationSettlementEvent::AuthorizationSucceeded { .. }
            | WalletDustRegistrationSettlementEvent::AuthorizationRejected { .. }
            | WalletDustRegistrationSettlementEvent::SubmissionAccepted { .. }
            | WalletDustRegistrationSettlementEvent::FinalityObserved { .. }
            | WalletDustRegistrationSettlementEvent::RegistrationReconciled { .. }
            | WalletDustRegistrationSettlementEvent::DustRefreshed { .. }
            | WalletDustRegistrationSettlementEvent::DroppedRegistrationAbandoned { .. }
    )
}

#[cfg(test)]
#[path = "dust_registration_driver_tests.rs"]
mod tests;
