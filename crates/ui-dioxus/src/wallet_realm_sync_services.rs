// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_wallet_application::{
    CancelSelectedWalletRealmSyncUseCase, GetSelectedWalletRealmSyncUseCase,
    GetWalletOperationTimelineUseCase, ManageWalletActionWatchUseCase,
    ReconcileWalletRealmLifecycleUseCase, SyncSelectedWalletRealmUseCase,
};

use crate::WalletUiServices;

/// Midnight account use cases consumed by the Assets page.
#[derive(Clone)]
pub struct WalletRealmSyncUiServices {
    sync: Arc<dyn SyncSelectedWalletRealmUseCase>,
    get: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
    lifecycle: Arc<dyn ReconcileWalletRealmLifecycleUseCase>,
    action_watch: Arc<dyn ManageWalletActionWatchUseCase>,
    cancel: Arc<dyn CancelSelectedWalletRealmSyncUseCase>,
    timeline: Arc<dyn GetWalletOperationTimelineUseCase>,
}

impl WalletRealmSyncUiServices {
    #[must_use]
    pub const fn new(
        sync: Arc<dyn SyncSelectedWalletRealmUseCase>,
        get: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
        lifecycle: Arc<dyn ReconcileWalletRealmLifecycleUseCase>,
        action_watch: Arc<dyn ManageWalletActionWatchUseCase>,
        cancel: Arc<dyn CancelSelectedWalletRealmSyncUseCase>,
        timeline: Arc<dyn GetWalletOperationTimelineUseCase>,
    ) -> Self {
        Self {
            sync,
            get,
            lifecycle,
            action_watch,
            cancel,
            timeline,
        }
    }
}

impl WalletUiServices {
    #[must_use]
    pub fn sync_selected_wallet_realm(&self) -> Arc<dyn SyncSelectedWalletRealmUseCase> {
        Arc::clone(&self.realm_sync.sync)
    }

    #[must_use]
    pub fn get_selected_wallet_realm_sync(&self) -> Arc<dyn GetSelectedWalletRealmSyncUseCase> {
        Arc::clone(&self.realm_sync.get)
    }

    #[must_use]
    pub fn reconcile_wallet_realm_lifecycle(
        &self,
    ) -> Arc<dyn ReconcileWalletRealmLifecycleUseCase> {
        Arc::clone(&self.realm_sync.lifecycle)
    }

    #[must_use]
    pub fn manage_wallet_action_watch(&self) -> Arc<dyn ManageWalletActionWatchUseCase> {
        Arc::clone(&self.realm_sync.action_watch)
    }

    #[must_use]
    pub fn cancel_selected_wallet_realm_sync(
        &self,
    ) -> Arc<dyn CancelSelectedWalletRealmSyncUseCase> {
        Arc::clone(&self.realm_sync.cancel)
    }

    #[must_use]
    pub fn get_wallet_operation_timeline(&self) -> Arc<dyn GetWalletOperationTimelineUseCase> {
        Arc::clone(&self.realm_sync.timeline)
    }
}
