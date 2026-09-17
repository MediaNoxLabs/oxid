// SPDX-License-Identifier: Apache-2.0

use crate::{
    ReconcileSelectedWalletRealmUseCase, SelectedWalletRealmProjection,
    SelectedWalletRealmSyncCommand, SelectedWalletRealmSyncError, WalletRealmLifecycleDecision,
    WalletRealmLifecycleIdentity, WalletRealmLifecycleInput, WalletRealmLifecyclePolicy,
    WalletRealmLifecyclePolicyConfig, WalletRealmReconciliationState,
};
use oxid_platform_ports::{ClockPort, PlatformError};
use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

pub type WalletRealmLifecycleFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<WalletRealmLifecycleResult, WalletRealmLifecycleError>>
            + Send
            + 'a,
    >,
>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletRealmLifecycleError {
    Clock(PlatformError),
    Sync(SelectedWalletRealmSyncError),
    Poisoned,
}
impl fmt::Display for WalletRealmLifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock(e) => e.fmt(f),
            Self::Sync(e) => e.fmt(f),
            Self::Poisoned => f.write_str("wallet realm lifecycle state is unavailable"),
        }
    }
}
impl Error for WalletRealmLifecycleError {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRealmLifecycleResult {
    pub decision: WalletRealmLifecycleDecision,
    pub projection: Option<SelectedWalletRealmProjection>,
}
pub trait ReconcileWalletRealmLifecycleUseCase: Send + Sync {
    fn execute(&self, input: WalletRealmLifecycleInput) -> WalletRealmLifecycleFuture<'_>;
}
pub struct WalletRealmLifecycleService {
    policy: Mutex<WalletRealmLifecyclePolicy>,
    config: WalletRealmLifecyclePolicyConfig,
    clock: Arc<dyn ClockPort>,
    sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
    facets: Mutex<WalletRealmReconciliationState>,
}
impl WalletRealmLifecycleService {
    pub fn new(
        clock: Arc<dyn ClockPort>,
        sync: Arc<dyn ReconcileSelectedWalletRealmUseCase>,
        facets: WalletRealmReconciliationState,
    ) -> Self {
        Self {
            policy: Mutex::new(WalletRealmLifecyclePolicy::default()),
            config: WalletRealmLifecyclePolicyConfig::default(),
            clock,
            sync,
            facets: Mutex::new(facets),
        }
    }
    fn complete(
        &self,
        identity: WalletRealmLifecycleIdentity,
        sequence: u64,
        facets: WalletRealmReconciliationState,
        succeeded: bool,
    ) -> Result<(), WalletRealmLifecycleError> {
        let now_millis = self
            .clock
            .now()
            .map_err(WalletRealmLifecycleError::Clock)?
            .value();
        *self
            .facets
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)? = facets;
        let _ = self
            .policy
            .lock()
            .map_err(|_| WalletRealmLifecycleError::Poisoned)?
            .reduce(
                self.config,
                WalletRealmLifecycleInput::ReconciliationFinished {
                    identity,
                    sequence,
                    now_millis,
                    facets,
                    succeeded,
                },
            );
        Ok(())
    }
}
impl ReconcileWalletRealmLifecycleUseCase for WalletRealmLifecycleService {
    fn execute(&self, input: WalletRealmLifecycleInput) -> WalletRealmLifecycleFuture<'_> {
        Box::pin(async move {
            let decision = self
                .policy
                .lock()
                .map_err(|_| WalletRealmLifecycleError::Poisoned)?
                .reduce(self.config, input);
            let (WalletRealmLifecycleDecision::Request(request)
            | WalletRealmLifecycleDecision::Superseded { request, .. }) = &decision
            else {
                return Ok(WalletRealmLifecycleResult {
                    decision,
                    projection: None,
                });
            };
            match self
                .sync
                .execute(
                    SelectedWalletRealmSyncCommand {
                        profile_id: request.identity.profile.as_str().to_owned(),
                    },
                    request.trigger,
                )
                .await
            {
                Ok(result) => {
                    self.complete(
                        request.identity.clone(),
                        request.sequence,
                        result.facets,
                        true,
                    )?;
                    Ok(WalletRealmLifecycleResult {
                        decision,
                        projection: Some(result.projection),
                    })
                }
                Err(error) => {
                    let facets = *self
                        .facets
                        .lock()
                        .map_err(|_| WalletRealmLifecycleError::Poisoned)?;
                    self.complete(request.identity.clone(), request.sequence, facets, false)?;
                    Err(WalletRealmLifecycleError::Sync(error))
                }
            }
        })
    }
}
