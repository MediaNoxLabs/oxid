// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use oxid_foundation::OpaqueIdError;
use oxid_wallet_domain::WalletProfileId;

use crate::{
    WalletAccountPortError, WalletAccountReadPort, WalletAccountView, WalletDustSyncPort,
    WalletDustSyncPortError, WalletDustSyncView, WalletShieldedSyncPort,
    WalletShieldedSyncPortError, WalletShieldedSyncView,
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

/// Validation or public-account failure for selected-realm reconciliation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectedWalletRealmSyncError {
    InvalidProfileIdentifier(OpaqueIdError),
}

impl fmt::Display for SelectedWalletRealmSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) => error.fmt(formatter),
        }
    }
}

impl Error for SelectedWalletRealmSyncError {}

pub type SelectedWalletRealmSyncViewFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError>>
            + Send
            + 'a,
    >,
>;

/// Starts one bounded public/DUST/shielded reconciliation for the selected realm.
pub trait SyncSelectedWalletRealmUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> SelectedWalletRealmSyncViewFuture<'_>;
}

/// Reads the most recently published aggregate without starting I/O.
pub trait GetSelectedWalletRealmSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError>;
}

/// Cooperatively cancels the private family workers and returns their state.
pub trait CancelSelectedWalletRealmSyncUseCase: Send + Sync {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError>;
}

/// Application orchestration over the three focused wallet ports.
pub struct SelectedWalletRealmSyncService<W> {
    wallet: Arc<W>,
}

impl<W> SelectedWalletRealmSyncService<W> {
    #[must_use]
    pub const fn new(wallet: Arc<W>) -> Self {
        Self { wallet }
    }

    fn profile(
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<WalletProfileId, SelectedWalletRealmSyncError> {
        WalletProfileId::parse(command.profile_id)
            .map_err(SelectedWalletRealmSyncError::InvalidProfileIdentifier)
    }

    fn current(
        &self,
        profile: &WalletProfileId,
    ) -> Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError>
    where
        W: WalletAccountReadPort + WalletDustSyncPort + WalletShieldedSyncPort,
    {
        let account = self
            .wallet
            .account(profile)
            .map(|snapshot| {
                WalletRealmFamilyView::Ready(WalletAccountView::from_snapshot(&snapshot))
            })
            .unwrap_or_else(account_family_error);
        Ok(SelectedWalletRealmSyncView {
            account,
            dust: self
                .wallet
                .dust_status(profile)
                .map(|snapshot| WalletRealmFamilyView::Ready(WalletDustSyncView::from(&snapshot)))
                .unwrap_or_else(dust_family_error),
            shielded: self
                .wallet
                .shielded_status(profile)
                .map(|snapshot| {
                    WalletRealmFamilyView::Ready(WalletShieldedSyncView::from(&snapshot))
                })
                .unwrap_or_else(shielded_family_error),
        })
    }
}

impl<W> SyncSelectedWalletRealmUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletAccountReadPort + WalletDustSyncPort + WalletShieldedSyncPort + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> SelectedWalletRealmSyncViewFuture<'_> {
        Box::pin(async move {
            let profile = Self::profile(command)?;
            let account = self
                .wallet
                .sync(&profile)
                .await
                .map(|snapshot| {
                    WalletRealmFamilyView::Ready(WalletAccountView::from_snapshot(&snapshot))
                })
                .unwrap_or_else(account_family_error);
            let dust = self
                .wallet
                .start_dust_sync(&profile)
                .map(|snapshot| WalletRealmFamilyView::Ready(WalletDustSyncView::from(&snapshot)))
                .unwrap_or_else(dust_family_error);
            let shielded = self
                .wallet
                .start_shielded_sync(&profile)
                .map(|snapshot| {
                    WalletRealmFamilyView::Ready(WalletShieldedSyncView::from(&snapshot))
                })
                .unwrap_or_else(shielded_family_error);
            Ok(SelectedWalletRealmSyncView {
                account,
                dust,
                shielded,
            })
        })
    }
}

impl<W> GetSelectedWalletRealmSyncUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletAccountReadPort + WalletDustSyncPort + WalletShieldedSyncPort + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError> {
        self.current(&Self::profile(command)?)
    }
}

impl<W> CancelSelectedWalletRealmSyncUseCase for SelectedWalletRealmSyncService<W>
where
    W: WalletAccountReadPort + WalletDustSyncPort + WalletShieldedSyncPort + 'static,
{
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmSyncView, SelectedWalletRealmSyncError> {
        let profile = Self::profile(command)?;
        let account = self
            .wallet
            .account(&profile)
            .map(|snapshot| {
                WalletRealmFamilyView::Ready(WalletAccountView::from_snapshot(&snapshot))
            })
            .unwrap_or_else(account_family_error);
        let dust = self
            .wallet
            .cancel_dust_sync(&profile)
            .map(|snapshot| WalletRealmFamilyView::Ready(WalletDustSyncView::from(&snapshot)))
            .unwrap_or_else(dust_family_error);
        let shielded = self
            .wallet
            .cancel_shielded_sync(&profile)
            .map(|snapshot| WalletRealmFamilyView::Ready(WalletShieldedSyncView::from(&snapshot)))
            .unwrap_or_else(shielded_family_error);
        Ok(SelectedWalletRealmSyncView {
            account,
            dust,
            shielded,
        })
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
    use std::task::{Context, Poll, Waker};

    use oxid_wallet_domain::{
        ChainKind, ChainNetwork, ChainNetworkId, NetworkDisplayName, NetworkEnvironment,
        WalletAccountSnapshot, WalletDustSyncSnapshot, WalletShieldedSyncSnapshot,
    };

    use super::*;

    struct PartialWallet;

    impl WalletAccountReadPort for PartialWallet {
        fn account(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletAccountSnapshot, WalletAccountPortError> {
            Ok(WalletAccountSnapshot::unavailable(network()))
        }

        fn sync<'a>(&'a self, _: &'a WalletProfileId) -> crate::WalletAccountPortFuture<'a> {
            Box::pin(async { Ok(WalletAccountSnapshot::unavailable(network())) })
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
            Err(WalletDustSyncPortError::ProtectionLocked)
        }

        fn cancel_dust_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletDustSyncSnapshot, WalletDustSyncPortError> {
            Err(WalletDustSyncPortError::ProtectionLocked)
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
            Ok(WalletShieldedSyncSnapshot::never_synced(network_id()))
        }

        fn cancel_shielded_sync(
            &self,
            _: &WalletProfileId,
        ) -> Result<WalletShieldedSyncSnapshot, WalletShieldedSyncPortError> {
            Err(WalletShieldedSyncPortError::Unavailable)
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
    fn aggregate_preserves_partial_family_failures_as_typed_state() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet));
        let started = resolve(SyncSelectedWalletRealmUseCase::execute(&service, command()))
            .expect("aggregate starts");
        assert!(matches!(started.account, WalletRealmFamilyView::Ready(_)));
        assert_eq!(started.dust, WalletRealmFamilyView::ProtectionLocked);
        assert!(matches!(
            started.shielded,
            WalletRealmFamilyView::Ready(WalletShieldedSyncView { ref state, .. })
                if state == "never_synced"
        ));

        let cancelled = CancelSelectedWalletRealmSyncUseCase::execute(&service, command())
            .expect("aggregate cancellation reports every family");
        assert_eq!(cancelled.dust, WalletRealmFamilyView::ProtectionLocked);
        assert_eq!(cancelled.shielded, WalletRealmFamilyView::Unavailable);
    }

    #[test]
    fn aggregate_rejects_an_invalid_profile_before_calling_ports() {
        let service = SelectedWalletRealmSyncService::new(Arc::new(PartialWallet));
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
}
