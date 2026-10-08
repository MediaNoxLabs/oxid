// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_platform_ports::PublicTextExportPort;
use oxid_wallet_application::{
    CancelSelectedWalletRealmSyncUseCase, DeriveWalletAccountUseCase,
    GetSelectedWalletRealmSyncUseCase, GetWalletAccountUseCase, GetWalletOperationTimelineUseCase,
    ListWalletNetworksUseCase, ManageWalletActionWatchUseCase,
    ReconcileWalletRealmLifecycleUseCase, SelectWalletNetworkUseCase,
    SyncSelectedWalletRealmUseCase, SyncWalletAccountUseCase,
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

/// Midnight account use cases consumed by the Assets page.
pub struct WalletAccountUiServices {
    pub(crate) list_wallet_networks: Arc<dyn ListWalletNetworksUseCase>,
    pub(crate) select_wallet_network: Arc<dyn SelectWalletNetworkUseCase>,
    pub(crate) derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase>,
    pub(crate) get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
    pub(crate) sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
    pub(crate) realm_sync: WalletRealmSyncUiServices,
    pub(crate) public_text_exporter: Arc<dyn PublicTextExportPort>,
}

impl WalletAccountUiServices {
    #[must_use]
    pub fn new(
        list_wallet_networks: Arc<dyn ListWalletNetworksUseCase>,
        select_wallet_network: Arc<dyn SelectWalletNetworkUseCase>,
        derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase>,
        get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
        sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
        realm_sync: WalletRealmSyncUiServices,
        public_text_exporter: Arc<dyn PublicTextExportPort>,
    ) -> Self {
        Self {
            list_wallet_networks,
            select_wallet_network,
            derive_wallet_account,
            get_wallet_account,
            sync_wallet_account,
            realm_sync,
            public_text_exporter,
        }
    }
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
