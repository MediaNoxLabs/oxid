// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_wallet_application::{
    CancelSelectedWalletRealmSyncUseCase, GetSelectedWalletRealmSyncUseCase,
    GetWalletOperationTimelineUseCase, SyncSelectedWalletRealmUseCase,
};

use crate::WalletRealmSyncUiServices;

impl WalletRealmSyncUiServices {
    #[must_use]
    pub const fn new(
        sync: Arc<dyn SyncSelectedWalletRealmUseCase>,
        get: Arc<dyn GetSelectedWalletRealmSyncUseCase>,
        cancel: Arc<dyn CancelSelectedWalletRealmSyncUseCase>,
        timeline: Arc<dyn GetWalletOperationTimelineUseCase>,
    ) -> Self {
        Self {
            sync,
            get,
            cancel,
            timeline,
        }
    }
}
