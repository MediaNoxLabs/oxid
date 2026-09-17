// SPDX-License-Identifier: Apache-2.0

use futures::{
    executor::block_on,
    future::{Either, select},
    pin_mut,
};
use futures_timer::Delay;
use oxid_wallet_application::{
    ReconcileWalletRealmLifecycleUseCase, WalletAccountQuery, WalletRealmFacetState,
    WalletRealmLifecycleIdentity, WalletRealmLifecycleInput, WalletRealmReconciliationState,
};
use std::{future::Future, sync::Arc, time::Duration};

use crate::HeadlessWallet;

const HEADLESS_STARTUP_RECONCILIATION_TIMEOUT_MILLIS: u64 = 1_000;

impl HeadlessWallet {
    pub(super) fn initialize_active_realm(&self) {
        let Ok(Some(profile)) = self.application.get_active_wallet_profile().execute() else {
            return;
        };
        let _ = self.drive_selected_realm(profile.id);
    }

    pub(super) fn drive_selected_realm(&self, profile_id: String) -> Result<(), ()> {
        let networks = self
            .application
            .list_wallet_networks()
            .execute(WalletAccountQuery {
                profile_id: profile_id.clone(),
            })
            .map_err(|_| ())?;
        let identity =
            WalletRealmLifecycleIdentity::parse(profile_id, networks.selected_network_id)
                .map_err(|_| ())?;
        let lifecycle = self.application.reconcile_wallet_realm_lifecycle();
        let status = lifecycle.status().map_err(|_| ())?;
        let facets = if status.identity.as_ref() == Some(&identity) {
            status.facets
        } else {
            missing_facets()
        };
        let input = match status.identity {
            None => WalletRealmLifecycleInput::Initialized {
                identity,
                now_millis: self.monotonic_millis(),
                facets,
            },
            Some(active) if active != identity => WalletRealmLifecycleInput::RealmSelected {
                identity,
                now_millis: self.monotonic_millis(),
                facets,
            },
            Some(_) => WalletRealmLifecycleInput::Foreground {
                now_millis: self.monotonic_millis(),
                facets,
            },
        };
        execute_bounded(lifecycle, input)
    }

    pub(super) fn monotonic_millis(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

fn execute_bounded(
    lifecycle: Arc<dyn ReconcileWalletRealmLifecycleUseCase>,
    input: WalletRealmLifecycleInput,
) -> Result<(), ()> {
    let timeout_millis = lifecycle
        .status()
        .map_err(|_| ())?
        .request_timeout_millis
        .min(HEADLESS_STARTUP_RECONCILIATION_TIMEOUT_MILLIS);
    match block_on_with_timeout(lifecycle.execute(input), timeout_millis) {
        Some(Ok(_)) => Ok(()),
        Some(Err(_)) => Err(()),
        None => {
            let status = lifecycle.status().map_err(|_| ())?;
            let request = status.in_flight.ok_or(())?;
            block_on(
                lifecycle.execute(WalletRealmLifecycleInput::ReconciliationTimedOut {
                    identity: request.identity,
                    sequence: request.sequence,
                    now_millis: status.observed_at_millis.saturating_add(timeout_millis),
                    facets: status.facets,
                }),
            )
            .map(|_| ())
            .map_err(|_| ())
        }
    }
}

fn block_on_with_timeout<F>(future: F, timeout_millis: u64) -> Option<F::Output>
where
    F: Future,
{
    block_on(async move {
        let timeout = Delay::new(Duration::from_millis(timeout_millis.max(1)));
        pin_mut!(future, timeout);
        match select(future, timeout).await {
            Either::Left((output, _)) => Some(output),
            Either::Right(((), _)) => None,
        }
    })
}

const fn missing_facets() -> WalletRealmReconciliationState {
    WalletRealmReconciliationState {
        account: WalletRealmFacetState::Missing,
        dust: WalletRealmFacetState::Missing,
        shielded: WalletRealmFacetState::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_wallet_application::{CreateWalletProfileCommand, SelectWalletProfileCommand};
    use std::future::pending;

    #[test]
    fn startup_initializes_the_active_selected_realm_without_a_sync_request() {
        let application = oxid_composition::compose_in_memory();
        let created = application
            .create_wallet_profile()
            .execute(CreateWalletProfileCommand {
                display_name: "Headless lifecycle".to_owned(),
            })
            .expect("profile creation");
        application
            .select_wallet_profile()
            .execute(SelectWalletProfileCommand {
                profile_id: created.id,
            })
            .expect("profile selection");
        let wallet = HeadlessWallet::new(application);
        let active = wallet
            .application
            .get_active_wallet_profile()
            .execute()
            .expect("active profile read")
            .expect("active profile");
        let status = wallet
            .application
            .reconcile_wallet_realm_lifecycle()
            .status()
            .expect("lifecycle status");

        assert_eq!(
            status
                .identity
                .as_ref()
                .map(|identity| identity.profile.as_str()),
            Some(active.id.as_str())
        );
        assert!(status.in_flight.is_none());
        assert!(status.next_wakeup_millis.is_some());
    }

    #[test]
    fn stalled_startup_work_is_bounded_before_the_request_loop() {
        assert_eq!(block_on_with_timeout(pending::<()>(), 1), None);
    }
}
