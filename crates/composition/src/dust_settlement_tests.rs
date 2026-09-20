// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use oxid_wallet_application::{
    AuthorizeWalletDustRegistrationCommand, ChainNetworkId, GetSelectedWalletRealmSyncUseCase,
    GetWalletDustRegistrationStatusCommand, PrepareWalletDustRegistrationCommand,
    ReconcileWalletDustRegistrationSubmissionCommand, SelectedWalletRealmIdentity,
    SelectedWalletRealmObservation, SelectedWalletRealmProjectionFuture,
    SelectedWalletRealmSyncError, SelectedWalletRealmSyncView, SubmitWalletDustRegistrationCommand,
    WalletAccountView, WalletAssetBalanceView, WalletDustRegistrationAssetView,
    WalletDustRegistrationError, WalletDustRegistrationPortError,
    WalletDustRegistrationPreviewView, WalletDustRegistrationStatusViewFuture,
    WalletDustRegistrationSubmissionStatusView, WalletDustRegistrationSubmissionView,
    WalletDustRegistrationSubmissionViewFuture, WalletDustSyncView, WalletProfileId,
    WalletRealmFamilyView, WalletShieldedSyncView, WalletSyncStatusView,
};

use super::*;

struct FakeServices {
    selected: Mutex<SelectedWalletRealmProjection>,
    reconcile_state: Mutex<String>,
    sync_ready: Mutex<Result<bool, ()>>,
    submit_unknown: Mutex<bool>,
    calls: Mutex<Vec<&'static str>>,
}

impl FakeServices {
    fn new() -> Self {
        Self {
            selected: Mutex::new(selected_projection(1, 7, true)),
            reconcile_state: Mutex::new("included".to_owned()),
            sync_ready: Mutex::new(Ok(true)),
            submit_unknown: Mutex::new(false),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }
}

impl GetSelectedWalletRealmSyncUseCase for FakeServices {
    fn execute(
        &self,
        _: SelectedWalletRealmSyncCommand,
    ) -> Result<SelectedWalletRealmProjection, SelectedWalletRealmSyncError> {
        Ok(self.selected.lock().unwrap().clone())
    }
}

impl SyncSelectedWalletRealmUseCase for FakeServices {
    fn execute(
        &self,
        _: SelectedWalletRealmSyncCommand,
    ) -> SelectedWalletRealmProjectionFuture<'_> {
        Box::pin(async move {
            self.record("refresh");
            let ready = *self.sync_ready.lock().unwrap();
            let ready = ready.map_err(|()| SelectedWalletRealmSyncError::Unavailable)?;
            let mut selected = self.selected.lock().unwrap();
            selected.revision += 1;
            selected.view.dust =
                WalletRealmFamilyView::Ready(dust_view(if ready { "synced" } else { "syncing" }));
            Ok(selected.clone())
        })
    }
}

impl PrepareWalletDustRegistrationUseCase for FakeServices {
    fn execute(
        &self,
        _: PrepareWalletDustRegistrationCommand,
    ) -> Result<WalletDustRegistrationPreviewView, WalletDustRegistrationError> {
        self.record("prepare");
        Ok(preview(false))
    }
}

impl AuthorizeWalletDustRegistrationUseCase for FakeServices {
    fn execute(
        &self,
        command: AuthorizeWalletDustRegistrationCommand,
    ) -> Result<WalletDustRegistrationPreviewView, WalletDustRegistrationError> {
        self.record("authorize");
        assert_eq!(command.draft_id, "dustreg_test");
        assert_eq!(command.authorization_challenge, "dustauth_test");
        assert!(command.confirmation.confirmed);
        Ok(preview(true))
    }
}

impl SubmitWalletDustRegistrationUseCase for FakeServices {
    fn execute<'a>(
        &'a self,
        _: SubmitWalletDustRegistrationCommand,
    ) -> WalletDustRegistrationSubmissionViewFuture<'a> {
        Box::pin(async move {
            self.record("submit");
            if *self.submit_unknown.lock().unwrap() {
                return Err(WalletDustRegistrationError::Operation(
                    WalletDustRegistrationPortError::SubmissionOutcomeUnknown,
                ));
            }
            Ok(WalletDustRegistrationSubmissionView {
                registration: preview(true),
                transaction_id: "tx_registration".to_owned(),
                block_id: "block_registration".to_owned(),
                fee: asset("midnight:dust", "DUST", 15, "42"),
                mode: "live".to_owned(),
                registration_observation: "included".to_owned(),
                dust_readiness: "requires_synchronization".to_owned(),
            })
        })
    }
}

impl GetWalletDustRegistrationStatusUseCase for FakeServices {
    fn execute(
        &self,
        _: GetWalletDustRegistrationStatusCommand,
    ) -> Result<WalletDustRegistrationSubmissionStatusView, WalletDustRegistrationError> {
        self.record("status");
        Ok(status("broadcasting"))
    }
}

impl ReconcileWalletDustRegistrationSubmissionUseCase for FakeServices {
    fn execute<'a>(
        &'a self,
        _: ReconcileWalletDustRegistrationSubmissionCommand,
    ) -> WalletDustRegistrationStatusViewFuture<'a> {
        Box::pin(async move {
            self.record("reconcile");
            Ok(status(&self.reconcile_state.lock().unwrap()))
        })
    }
}

fn capability(fake: &Arc<FakeServices>) -> WalletDustSettlementCapability {
    WalletDustSettlementCapability::new(
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
    )
}

fn confirmation(confirmed: bool) -> SensitiveOperationConfirmation {
    SensitiveOperationConfirmation {
        title: "Authorize DUST registration".to_owned(),
        summary: "Authorize the exact retained registration preview.".to_owned(),
        confirmed,
    }
}

fn asset(id: &str, symbol: &str, decimals: u8, units: &str) -> WalletDustRegistrationAssetView {
    WalletDustRegistrationAssetView {
        asset_id: id.to_owned(),
        symbol: symbol.to_owned(),
        decimals,
        atomic_units: units.to_owned(),
    }
}

fn preview(submission_ready: bool) -> WalletDustRegistrationPreviewView {
    WalletDustRegistrationPreviewView {
        draft_id: "dustreg_test".to_owned(),
        authorization_challenge: "dustauth_test".to_owned(),
        network_id: "undeployed".to_owned(),
        account_id: "midnight_account_test".to_owned(),
        registered_night: asset("midnight:night", "NIGHT", 6, "50000000"),
        input_count: 1,
        maximum_fee_allowance: asset("midnight:dust", "DUST", 15, "100"),
        fee_state: "estimated".to_owned(),
        expires_at_millis: 1_700_003_600_000,
        state: if submission_ready {
            "authorized".to_owned()
        } else {
            "prepared".to_owned()
        },
        authorization_ready: !submission_ready,
        submission_ready,
    }
}

fn status(state: &str) -> WalletDustRegistrationSubmissionStatusView {
    WalletDustRegistrationSubmissionStatusView {
        draft_id: "dustreg_test".to_owned(),
        state: state.to_owned(),
        transaction_id: Some("tx_registration".to_owned()),
        block_id: (state == "included").then(|| "block_registration".to_owned()),
        fee: None,
        mode: Some("live".to_owned()),
        registration_observation: if state == "included" {
            "included".to_owned()
        } else {
            "not_observed".to_owned()
        },
        dust_readiness: "requires_synchronization".to_owned(),
        cancellation_allowed: false,
        reconciliation_allowed: state != "included",
    }
}

fn dust_view(state: &str) -> WalletDustSyncView {
    WalletDustSyncView {
        network_id: "undeployed".to_owned(),
        state: state.to_owned(),
        current_cursor: Some(1),
        target_cursor: Some(1),
        events_processed: 1,
        balance_atomic_units: Some("1".to_owned()),
        updated_at_millis: Some(1),
        failure: None,
    }
}

fn selected_projection(
    generation: u64,
    revision: u64,
    eligible: bool,
) -> SelectedWalletRealmProjection {
    SelectedWalletRealmProjection {
        identity: SelectedWalletRealmIdentity {
            profile: WalletProfileId::parse("profile_test").unwrap(),
            realm: ChainNetworkId::parse("undeployed").unwrap(),
        },
        generation,
        revision,
        fresh: true,
        consistent: true,
        actionable: SelectedWalletRealmActionReadiness::Ready,
        observation: SelectedWalletRealmObservation::Settled,
        view: SelectedWalletRealmSyncView {
            account: WalletRealmFamilyView::Ready(WalletAccountView {
                chain: "midnight".to_owned(),
                network_id: "undeployed".to_owned(),
                network_name: "Standalone".to_owned(),
                network_environment: "standalone".to_owned(),
                account_id: Some("midnight_account_test".to_owned()),
                source: "live".to_owned(),
                addresses: Vec::new(),
                balances: vec![WalletAssetBalanceView {
                    asset_id: "midnight:night".to_owned(),
                    symbol: "NIGHT".to_owned(),
                    decimals: 6,
                    atomic_units: if eligible { "50000000" } else { "0" }.to_owned(),
                }],
                sync: WalletSyncStatusView {
                    state: "synced".to_owned(),
                    current_cursor: Some(1),
                    target_cursor: Some(1),
                    chain_tip_height: Some(1),
                    updated_at_millis: Some(1),
                },
                transactions: Vec::new(),
            }),
            dust: WalletRealmFamilyView::Ready(dust_view("synced")),
            shielded: WalletRealmFamilyView::Ready(WalletShieldedSyncView {
                network_id: "undeployed".to_owned(),
                state: "synced".to_owned(),
                current_cursor: Some(1),
                target_cursor: Some(1),
                events_processed: 1,
                owned_note_count: Some(0),
                commitment_count: Some(0),
                balances: Vec::new(),
                updated_at_millis: Some(1),
                failure: None,
            }),
        },
    }
}

#[test]
fn one_authorization_drives_submission_reconciliation_and_refresh() {
    let fake = Arc::new(FakeServices::new());
    let capability = capability(&fake);

    let prepared = block_on(capability.refresh("profile_test".to_owned())).unwrap();
    assert_eq!(
        prepared.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
    );
    let ready = block_on(capability.authorize(confirmation(true))).unwrap();
    assert_eq!(
        ready.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
    );
    assert_eq!(
        *fake.calls.lock().unwrap(),
        [
            "prepare",
            "authorize",
            "submit",
            "status",
            "reconcile",
            "refresh"
        ]
    );
}

#[test]
fn declined_authorization_performs_no_protected_or_chain_operation() {
    let fake = Arc::new(FakeServices::new());
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();

    let projection = block_on(capability.authorize(confirmation(false))).unwrap();
    assert_eq!(
        projection.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::ActionRequired
    );
    assert_eq!(*fake.calls.lock().unwrap(), ["prepare"]);
}

#[test]
fn stale_generation_cannot_authorize_the_retained_preview() {
    let fake = Arc::new(FakeServices::new());
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();
    fake.selected.lock().unwrap().generation += 1;

    assert_eq!(
        block_on(capability.authorize(confirmation(true))),
        Err(WalletDustSettlementError::Driver(
            WalletDustRegistrationDriverError::Executor(
                WalletDustRegistrationExecutorFailure::Degraded
            )
        ))
    );
    assert_eq!(*fake.calls.lock().unwrap(), ["prepare"]);
}

#[test]
fn uncertain_submission_is_retained_without_a_second_submit() {
    let fake = Arc::new(FakeServices::new());
    *fake.submit_unknown.lock().unwrap() = true;
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();

    assert_eq!(
        block_on(capability.authorize(confirmation(true))),
        Err(WalletDustSettlementError::Driver(
            WalletDustRegistrationDriverError::Executor(
                WalletDustRegistrationExecutorFailure::Degraded
            )
        ))
    );
    assert_eq!(
        capability.projection().unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Submitting
    );
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "submit")
            .count(),
        1
    );
}

#[test]
fn pending_reconciliation_returns_a_stable_projection_without_hot_looping() {
    let fake = Arc::new(FakeServices::new());
    *fake.reconcile_state.lock().unwrap() = "broadcasting".to_owned();
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();

    let pending = block_on(capability.authorize(confirmation(true))).unwrap();
    assert_eq!(
        pending.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Confirming
    );
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "reconcile")
            .count(),
        1
    );
}

#[test]
fn refresh_failure_retains_the_included_registration_for_retry() {
    let fake = Arc::new(FakeServices::new());
    *fake.sync_ready.lock().unwrap() = Err(());
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();

    assert_eq!(
        block_on(capability.authorize(confirmation(true))),
        Err(WalletDustSettlementError::Driver(
            WalletDustRegistrationDriverError::Executor(
                WalletDustRegistrationExecutorFailure::Unavailable
            )
        ))
    );
    assert_eq!(
        capability.projection().unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Reconciling
    );
}
