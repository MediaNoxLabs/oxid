// SPDX-License-Identifier: Apache-2.0

//! Application execution seam for selected-realm lifecycle admission.
//!
//! Platform adapters supply typed lifecycle inputs; this owner preserves the
//! policy-selected trigger when it invokes the selected-realm reconciler.

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use crate::{
    SelectedWalletRealmProjection, SelectedWalletRealmSyncCommand, SelectedWalletRealmSyncError,
    SyncSelectedWalletRealmUseCase, WalletRealmFacetState, WalletRealmLifecycleDecision,
    WalletRealmLifecycleInput, WalletRealmLifecyclePolicy, WalletRealmLifecyclePolicyConfig,
    WalletRealmReconciliationState,
};

pub type WalletRealmLifecycleFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<WalletRealmLifecycleResult, SelectedWalletRealmSyncError>>
            + Send
            + 'a,
    >,
>;

/// Result of one lifecycle observation, including an optional admitted projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletRealmLifecycleResult {
    pub decision: WalletRealmLifecycleDecision,
    pub projection: Option<SelectedWalletRealmProjection>,
}

/// Incoming application seam shared by headless and presentation adapters.
pub trait ReconcileWalletRealmLifecycleUseCase: Send + Sync {
    fn execute(&self, input: WalletRealmLifecycleInput) -> WalletRealmLifecycleFuture<'_>;
}

/// Serializes policy admission and executes only the typed trigger it selects.
pub struct WalletRealmLifecycleService {
    policy: Mutex<WalletRealmLifecyclePolicy>,
    config: WalletRealmLifecyclePolicyConfig,
    sync: Arc<dyn SyncSelectedWalletRealmUseCase>,
}

impl WalletRealmLifecycleService {
    #[must_use]
    pub fn new(sync: Arc<dyn SyncSelectedWalletRealmUseCase>) -> Self {
        Self {
            policy: Mutex::new(WalletRealmLifecyclePolicy::default()),
            config: WalletRealmLifecyclePolicyConfig::default(),
            sync,
        }
    }
}

impl ReconcileWalletRealmLifecycleUseCase for WalletRealmLifecycleService {
    fn execute(&self, input: WalletRealmLifecycleInput) -> WalletRealmLifecycleFuture<'_> {
        Box::pin(async move {
            let decision = self
                .policy
                .lock()
                .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
                .reduce(self.config, input);
            let (WalletRealmLifecycleDecision::Request(request)
            | WalletRealmLifecycleDecision::Superseded { request, .. }) = &decision
            else {
                return Ok(WalletRealmLifecycleResult {
                    decision,
                    projection: None,
                });
            };
            let projection = self
                .sync
                .execute(SelectedWalletRealmSyncCommand {
                    profile_id: request.identity.profile.as_str().to_owned(),
                })
                .await?;
            let completion = WalletRealmLifecycleInput::ReconciliationFinished {
                identity: request.identity.clone(),
                sequence: request.sequence,
                now_millis: 0,
                facets: projection_facets(&projection),
                succeeded: true,
            };
            let _ = self
                .policy
                .lock()
                .map_err(|_| SelectedWalletRealmSyncError::Unavailable)?
                .reduce(self.config, completion);
            Ok(WalletRealmLifecycleResult {
                decision,
                projection: Some(projection),
            })
        })
    }
}

fn projection_facets(projection: &SelectedWalletRealmProjection) -> WalletRealmReconciliationState {
    let state = if projection.fresh {
        WalletRealmFacetState::Current
    } else {
        WalletRealmFacetState::Stale
    };
    WalletRealmReconciliationState {
        account: state,
        dust: state,
        shielded: state,
    }
}
