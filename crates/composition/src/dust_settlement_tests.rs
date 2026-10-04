// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use futures::executor::block_on;
use oxid_wallet_application::{
    AuthorizeWalletDustRegistrationCommand, ChainNetworkId, GetSelectedWalletRealmSyncUseCase,
    GetWalletDustRegistrationStatusCommand, GetWalletDustSyncStatusUseCase,
    InMemoryWalletDustRegistrationRecoveryStore, PrepareWalletDustRegistrationCommand,
    ReconcileWalletDustRegistrationSubmissionCommand, SelectedWalletRealmActionReadiness,
    SelectedWalletRealmIdentity, SelectedWalletRealmObservation,
    SelectedWalletRealmProjectionFuture, SelectedWalletRealmSyncError, SelectedWalletRealmSyncView,
    SubmitWalletDustRegistrationCommand, WalletAccountSource, WalletAccountView,
    WalletAssetBalanceView, WalletDustRegistrationAssetView, WalletDustRegistrationError,
    WalletDustRegistrationPortError, WalletDustRegistrationPreviewView,
    WalletDustRegistrationPreviewViewFuture, WalletDustRegistrationStatusViewFuture,
    WalletDustRegistrationSubmissionStatusView, WalletDustRegistrationSubmissionView,
    WalletDustRegistrationSubmissionViewFuture, WalletDustSyncCommand, WalletDustSyncError,
    WalletDustSyncView, WalletProfileId, WalletRealmFamilyView, WalletShieldedSyncView,
    WalletSyncState, WalletSyncStatusView,
};

use super::*;

struct FakeServices {
    selected: Mutex<SelectedWalletRealmProjection>,
    reconcile_state: Mutex<String>,
    status_state: Mutex<String>,
    sync_ready: Mutex<Result<bool, ()>>,
    submit_unknown: Mutex<bool>,
    submit_timeout: Mutex<bool>,
    submit_rejected: Mutex<bool>,
    registration_already_current: Mutex<bool>,
    registration_prepare_failure: Mutex<Option<WalletDustRegistrationPortError>>,
    prepare_pending: AtomicBool,
    prepare_drops: AtomicUsize,
    calls: Mutex<Vec<&'static str>>,
}

impl FakeServices {
    fn new() -> Self {
        Self {
            selected: Mutex::new(selected_projection(1, 7, true)),
            reconcile_state: Mutex::new("included".to_owned()),
            status_state: Mutex::new("broadcasting".to_owned()),
            sync_ready: Mutex::new(Ok(true)),
            submit_unknown: Mutex::new(false),
            submit_timeout: Mutex::new(false),
            submit_rejected: Mutex::new(false),
            registration_already_current: Mutex::new(false),
            registration_prepare_failure: Mutex::new(None),
            prepare_pending: AtomicBool::new(false),
            prepare_drops: AtomicUsize::new(0),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }

    fn preview(&self, submission_ready: bool) -> WalletDustRegistrationPreviewView {
        preview(
            submission_ready,
            self.selected.lock().unwrap().identity.realm.as_str(),
        )
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

impl GetWalletDustSyncStatusUseCase for FakeServices {
    fn execute(&self, _: WalletDustSyncCommand) -> Result<WalletDustSyncView, WalletDustSyncError> {
        let selected = self.selected.lock().unwrap();
        match &selected.view.dust {
            WalletRealmFamilyView::Ready(dust) => Ok(dust.clone()),
            _ => Ok(dust_view("syncing")),
        }
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

impl ReconcileSelectedWalletRealmUseCase for FakeServices {
    fn execute(
        &self,
        command: SelectedWalletRealmSyncCommand,
        _: WalletRealmReconciliationTrigger,
    ) -> SelectedWalletRealmReconciliationFuture<'_> {
        Box::pin(async move {
            let projection = SyncSelectedWalletRealmUseCase::execute(self, command).await?;
            Ok(oxid_wallet_application::SelectedWalletRealmReconciliation {
                projection,
                facets: oxid_wallet_application::WalletRealmReconciliationState {
                    account: oxid_wallet_application::WalletRealmFacetState::Current,
                    dust: oxid_wallet_application::WalletRealmFacetState::Current,
                    shielded: oxid_wallet_application::WalletRealmFacetState::Current,
                },
            })
        })
    }
}

impl PrepareWalletDustRegistrationUseCase for FakeServices {
    fn execute<'a>(
        &'a self,
        _: PrepareWalletDustRegistrationCommand,
    ) -> WalletDustRegistrationPreviewViewFuture<'a> {
        Box::pin(async move {
            self.record("prepare");
            if self.prepare_pending.load(Ordering::Acquire) {
                struct CountDrop<'a>(&'a AtomicUsize);
                impl Drop for CountDrop<'_> {
                    fn drop(&mut self) {
                        self.0.fetch_add(1, Ordering::AcqRel);
                    }
                }
                let _count_drop = CountDrop(&self.prepare_drops);
                futures::future::pending::<()>().await;
            }
            if let Some(error) = *self.registration_prepare_failure.lock().unwrap() {
                return Err(WalletDustRegistrationError::Operation(error));
            }
            if *self.registration_already_current.lock().unwrap() {
                return Err(WalletDustRegistrationError::Operation(
                    WalletDustRegistrationPortError::RegistrationAlreadyCurrent,
                ));
            }
            Ok(self.preview(false))
        })
    }
}

#[test]
fn missing_or_locked_custody_is_not_flattened_into_generic_degradation() {
    for (port_failure, expected) in [
        (
            WalletDustRegistrationPortError::ProtectionNotInitialized,
            WalletDustRegistrationExecutorFailure::ProtectionNotInitialized,
        ),
        (
            WalletDustRegistrationPortError::ProtectionLocked,
            WalletDustRegistrationExecutorFailure::ProtectionLocked,
        ),
    ] {
        let fake = Arc::new(FakeServices::new());
        *fake.registration_prepare_failure.lock().unwrap() = Some(port_failure);
        let capability = capability(&fake);

        assert_eq!(
            block_on(capability.refresh("profile_test".to_owned())),
            Err(WalletDustSettlementError::Driver(
                WalletDustRegistrationDriverError::Executor(expected)
            ))
        );
    }
}

impl AuthorizeWalletDustRegistrationUseCase for FakeServices {
    fn execute<'a>(
        &'a self,
        command: AuthorizeWalletDustRegistrationCommand,
    ) -> WalletDustRegistrationPreviewViewFuture<'a> {
        Box::pin(async move {
            self.record("authorize");
            assert_eq!(command.draft_id, "dustreg_test");
            assert_eq!(command.authorization_challenge, "dustauth_test");
            assert!(command.confirmation.confirmed);
            Ok(self.preview(true))
        })
    }
}

impl AuthorizeDevelopmentWalletDustRegistrationUseCase for FakeServices {
    fn execute<'a>(
        &'a self,
        command: AuthorizeDevelopmentWalletDustRegistrationCommand,
    ) -> WalletDustRegistrationPreviewViewFuture<'a> {
        Box::pin(async move {
            self.record("authorize");
            assert_eq!(command.draft_id, "dustreg_test");
            assert_eq!(command.authorization_challenge, "dustauth_test");
            Ok(self.preview(true))
        })
    }
}

impl SubmitWalletDustRegistrationUseCase for FakeServices {
    fn execute<'a>(
        &'a self,
        _: SubmitWalletDustRegistrationCommand,
    ) -> WalletDustRegistrationSubmissionViewFuture<'a> {
        Box::pin(async move {
            self.record("submit");
            if *self.submit_timeout.lock().unwrap() {
                return Err(WalletDustRegistrationError::Operation(
                    WalletDustRegistrationPortError::Timeout,
                ));
            }
            if *self.submit_unknown.lock().unwrap() {
                return Err(WalletDustRegistrationError::Operation(
                    WalletDustRegistrationPortError::SubmissionOutcomeUnknown,
                ));
            }
            if *self.submit_rejected.lock().unwrap() {
                return Err(WalletDustRegistrationError::Operation(
                    WalletDustRegistrationPortError::SubmissionRejected,
                ));
            }
            Ok(WalletDustRegistrationSubmissionView {
                registration: self.preview(true),
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

impl SubmitDevelopmentWalletDustRegistrationUseCase for FakeServices {
    fn execute<'a>(
        &'a self,
        _: SubmitDevelopmentWalletDustRegistrationCommand,
    ) -> WalletDustRegistrationSubmissionViewFuture<'a> {
        SubmitWalletDustRegistrationUseCase::execute(
            self,
            SubmitWalletDustRegistrationCommand {
                profile_id: "profile_test".to_owned(),
                draft_id: "dustreg_test".to_owned(),
                confirmation: confirmation(true),
            },
        )
    }
}

impl GetWalletDustRegistrationStatusUseCase for FakeServices {
    fn execute<'a>(
        &'a self,
        _: GetWalletDustRegistrationStatusCommand,
    ) -> WalletDustRegistrationStatusViewFuture<'a> {
        Box::pin(async move {
            self.record("status");
            Ok(status(&self.status_state.lock().unwrap()))
        })
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
    capability_with_store(
        fake,
        Arc::new(InMemoryWalletDustRegistrationRecoveryStore::default()),
    )
}

fn automatic_capability(fake: &Arc<FakeServices>) -> WalletDustSettlementCapability {
    WalletDustSettlementCapability::with_automatic_development_authority_and_recovery_store(
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        Arc::new(InMemoryWalletDustRegistrationRecoveryStore::default()),
    )
    .unwrap()
}

fn capability_with_store(
    fake: &Arc<FakeServices>,
    store: Arc<dyn WalletDustRegistrationRecoveryStore>,
) -> WalletDustSettlementCapability {
    WalletDustSettlementCapability::with_recovery_store(
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        store,
    )
    .unwrap()
}

fn capability_with_deadline(
    fake: &Arc<FakeServices>,
    deadline: Duration,
) -> WalletDustSettlementCapability {
    WalletDustSettlementCapability::with_recovery_store_and_deadline(
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        fake.clone(),
        Arc::new(InMemoryWalletDustRegistrationRecoveryStore::default()),
        deadline,
    )
    .unwrap()
}

fn confirmation(confirmed: bool) -> SensitiveOperationConfirmation {
    SensitiveOperationConfirmation {
        title: "Authorize DUST registration".to_owned(),
        summary: "Authorize the exact retained registration preview.".to_owned(),
        confirmed,
    }
}

struct UnavailableRecoveryStore;

impl WalletDustRegistrationRecoveryStore for UnavailableRecoveryStore {
    fn save(
        &self,
        _: WalletDustRegistrationRecoveryRecord,
    ) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
        Err(WalletDustRegistrationRecoveryStoreError::Unavailable)
    }

    fn load(
        &self,
    ) -> Result<
        Option<WalletDustRegistrationRecoveryRecord>,
        WalletDustRegistrationRecoveryStoreError,
    > {
        Err(WalletDustRegistrationRecoveryStoreError::Unavailable)
    }

    fn clear(&self) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
        Err(WalletDustRegistrationRecoveryStoreError::Unavailable)
    }
}

struct IntegrityRecoveryStore {
    clear_calls: Arc<AtomicUsize>,
}

#[derive(Default)]
struct BlockingRecoveryStore {
    released: Mutex<bool>,
    release_changed: Condvar,
    record: Mutex<Option<WalletDustRegistrationRecoveryRecord>>,
}

impl BlockingRecoveryStore {
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.release_changed.notify_all();
    }
}

impl WalletDustRegistrationRecoveryStore for BlockingRecoveryStore {
    fn save(
        &self,
        record: WalletDustRegistrationRecoveryRecord,
    ) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.release_changed.wait(released).unwrap();
        }
        *self.record.lock().unwrap() = Some(record);
        Ok(())
    }

    fn load(
        &self,
    ) -> Result<
        Option<WalletDustRegistrationRecoveryRecord>,
        WalletDustRegistrationRecoveryStoreError,
    > {
        Ok(self.record.lock().unwrap().clone())
    }

    fn clear(&self) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
        *self.record.lock().unwrap() = None;
        Ok(())
    }
}

impl WalletDustRegistrationRecoveryStore for IntegrityRecoveryStore {
    fn save(
        &self,
        _: WalletDustRegistrationRecoveryRecord,
    ) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
        Err(WalletDustRegistrationRecoveryStoreError::Integrity)
    }

    fn load(
        &self,
    ) -> Result<
        Option<WalletDustRegistrationRecoveryRecord>,
        WalletDustRegistrationRecoveryStoreError,
    > {
        Err(WalletDustRegistrationRecoveryStoreError::Integrity)
    }

    fn clear(&self) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
        self.clear_calls.fetch_add(1, Ordering::AcqRel);
        Err(WalletDustRegistrationRecoveryStoreError::Integrity)
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

fn preview(submission_ready: bool, network_id: &str) -> WalletDustRegistrationPreviewView {
    WalletDustRegistrationPreviewView {
        draft_id: "dustreg_test".to_owned(),
        authorization_challenge: "dustauth_test".to_owned(),
        network_id: network_id.to_owned(),
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
    let recorded = !matches!(
        state,
        "not_started" | "running" | "cancellation_requested" | "cancelled"
    );
    WalletDustRegistrationSubmissionStatusView {
        draft_id: "dustreg_test".to_owned(),
        state: state.to_owned(),
        transaction_id: recorded.then(|| "tx_registration".to_owned()),
        block_id: (state == "included").then(|| "block_registration".to_owned()),
        fee: None,
        mode: recorded.then(|| "live".to_owned()),
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
        spendable_night:
            oxid_wallet_application::SelectedWalletRealmSpendableNightView::Unavailable,
        view: SelectedWalletRealmSyncView {
            account: WalletRealmFamilyView::Ready(WalletAccountView {
                chain: "midnight".to_owned(),
                network_id: "undeployed".to_owned(),
                network_name: "Standalone".to_owned(),
                network_environment: "standalone".to_owned(),
                account_id: Some("midnight_account_test".to_owned()),
                source: WalletAccountSource::Live,
                addresses: Vec::new(),
                balances: vec![WalletAssetBalanceView {
                    asset_id: "midnight:night".to_owned(),
                    symbol: "NIGHT".to_owned(),
                    decimals: 6,
                    atomic_units: if eligible { "50000000" } else { "0" }.to_owned(),
                }],
                sync: WalletSyncStatusView {
                    state: WalletSyncState::Synced,
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
fn registration_readiness_requires_a_synced_positive_dust_snapshot() {
    let positive = selected_projection(1, 7, true);
    assert!(dust_registration_is_ready(&positive));

    let mut zero = positive.clone();
    let WalletRealmFamilyView::Ready(dust) = &mut zero.view.dust else {
        panic!("DUST fixture is ready");
    };
    dust.balance_atomic_units = Some("0".to_owned());
    assert!(!dust_registration_is_ready(&zero));

    let mut syncing = positive;
    let WalletRealmFamilyView::Ready(dust) = &mut syncing.view.dust else {
        panic!("DUST fixture is ready");
    };
    dust.state = "syncing".to_owned();
    assert!(!dust_registration_is_ready(&syncing));
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
fn development_realm_converges_without_an_incoming_authorization_action() {
    let fake = Arc::new(FakeServices::new());
    let capability = automatic_capability(&fake);

    let ready = block_on(capability.refresh("profile_test".to_owned())).unwrap();

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

    let repeated = block_on(capability.refresh("profile_test".to_owned())).unwrap();
    assert_eq!(
        repeated.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
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
fn automatic_authority_stops_at_the_boundary_outside_the_development_realm() {
    let fake = Arc::new(FakeServices::new());
    let mut selected = fake.selected.lock().unwrap();
    selected.identity.realm = ChainNetworkId::parse("preprod").unwrap();
    if let WalletRealmFamilyView::Ready(account) = &mut selected.view.account {
        account.network_id = "preprod".to_owned();
    }
    if let WalletRealmFamilyView::Ready(dust) = &mut selected.view.dust {
        dust.network_id = "preprod".to_owned();
    }
    if let WalletRealmFamilyView::Ready(shielded) = &mut selected.view.shielded {
        shielded.network_id = "preprod".to_owned();
    }
    drop(selected);
    let capability = automatic_capability(&fake);

    let pending = block_on(capability.refresh("profile_test".to_owned())).unwrap();

    assert_eq!(
        pending.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
    );
    assert_eq!(*fake.calls.lock().unwrap(), ["prepare"]);
}

#[test]
fn selected_realm_lifecycle_triggers_the_same_automatic_convergence() {
    let fake = Arc::new(FakeServices::new());
    let dust = Arc::new(automatic_capability(&fake));
    let reconciler =
        AutomaticDustRealmReconciler::new(fake.clone(), fake.clone(), fake.clone(), dust.clone());

    block_on(ReconcileSelectedWalletRealmUseCase::execute(
        &reconciler,
        SelectedWalletRealmSyncCommand {
            profile_id: "profile_test".to_owned(),
        },
        WalletRealmReconciliationTrigger::Initial,
    ))
    .unwrap();

    assert_eq!(
        dust.projection().unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
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
fn funded_live_night_is_eligible_before_private_facets_are_synchronized() {
    let mut selected = selected_projection(1, 7, true);
    selected.fresh = false;
    selected.actionable = SelectedWalletRealmActionReadiness::Unavailable;
    selected.view.dust = WalletRealmFamilyView::Ready(dust_view("never_synced"));

    assert!(selected_realm_is_eligible(&selected));
}

#[test]
fn already_registered_night_is_ready_without_duplicate_authorization_or_submission() {
    let fake = Arc::new(FakeServices::new());
    *fake.registration_already_current.lock().unwrap() = true;
    let capability = capability(&fake);

    let ready = block_on(capability.refresh("profile_test".to_owned())).unwrap();

    assert_eq!(
        ready.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
    );
    assert!(ready.registration.is_none());
    assert_eq!(*fake.calls.lock().unwrap(), ["prepare"]);
}

#[test]
fn authorization_review_exposes_only_presentation_safe_facts() {
    let fake = Arc::new(FakeServices::new());
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();

    let review = capability.authorization_review().unwrap();
    assert_eq!(review.network_id, "undeployed");
    assert_eq!(review.registered_night.asset_id, "midnight:night");
    assert_eq!(review.input_count, 1);
    assert_eq!(review.maximum_fee_allowance.asset_id, "midnight:dust");
    assert!(!format!("{review:?}").contains("authorization_challenge"));
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
fn profile_switch_supersedes_the_previous_identity_even_at_the_same_generation() {
    let fake = Arc::new(FakeServices::new());
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();

    let mut selected = fake.selected.lock().unwrap();
    selected.identity.profile = WalletProfileId::parse("profile_other").unwrap();
    drop(selected);

    let replacement = block_on(capability.refresh("profile_other".to_owned())).unwrap();
    assert_eq!(
        replacement.identity.unwrap().profile.as_str(),
        "profile_other"
    );
    assert_eq!(
        replacement.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
    );
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "prepare")
            .count(),
        2
    );
}

#[test]
fn uncertain_submission_recovers_the_retained_transaction_without_a_second_submit() {
    let fake = Arc::new(FakeServices::new());
    *fake.submit_unknown.lock().unwrap() = true;
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();

    assert_eq!(
        block_on(capability.authorize(confirmation(true)))
            .unwrap()
            .state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
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
fn timed_out_submit_retry_reconciles_public_status_without_a_second_submit() {
    let fake = Arc::new(FakeServices::new());
    *fake.submit_timeout.lock().unwrap() = true;
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();
    assert!(block_on(capability.authorize(confirmation(true))).is_err());
    assert_eq!(
        capability.projection().unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::TimedOut
    );
    assert_eq!(
        block_on(capability.retry()).unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
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
fn pre_broadcast_timeout_retries_only_after_cancellation_is_observed() {
    let fake = Arc::new(FakeServices::new());
    *fake.submit_timeout.lock().unwrap() = true;
    *fake.status_state.lock().unwrap() = "cancelled".to_owned();
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();
    assert!(block_on(capability.authorize(confirmation(true))).is_err());
    assert_eq!(
        capability.projection().unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::TimedOut
    );

    *fake.submit_timeout.lock().unwrap() = false;
    assert_eq!(
        block_on(capability.retry()).unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
    );
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "submit")
            .count(),
        2
    );
}

#[test]
fn definitive_submit_failure_retries_submission_instead_of_false_reconciliation() {
    let fake = Arc::new(FakeServices::new());
    *fake.submit_rejected.lock().unwrap() = true;
    let capability = capability(&fake);
    block_on(capability.refresh("profile_test".to_owned())).unwrap();
    assert!(block_on(capability.authorize(confirmation(true))).is_err());

    *fake.submit_rejected.lock().unwrap() = false;
    assert_eq!(
        block_on(capability.retry()).unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
    );
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "submit")
            .count(),
        2
    );
}

#[test]
fn restart_reconciles_public_transaction_without_reauthorization_or_resubmission() {
    let fake = Arc::new(FakeServices::new());
    let store: Arc<dyn WalletDustRegistrationRecoveryStore> =
        Arc::new(InMemoryWalletDustRegistrationRecoveryStore::default());
    *fake.reconcile_state.lock().unwrap() = "broadcasting".to_owned();
    let first = capability_with_store(&fake, Arc::clone(&store));
    block_on(first.refresh("profile_test".to_owned())).unwrap();
    assert_eq!(
        block_on(first.authorize(confirmation(true))).unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Confirming
    );
    drop(first);
    *fake.reconcile_state.lock().unwrap() = "included".to_owned();
    let restarted = capability_with_store(&fake, store);
    assert_eq!(
        block_on(restarted.refresh("profile_test".to_owned()))
            .unwrap()
            .state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
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
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "authorize")
            .count(),
        1
    );
}

#[test]
fn unavailable_recovery_store_keeps_wallet_available_but_blocks_broadcast() {
    let fake = Arc::new(FakeServices::new());
    let capability = capability_with_store(&fake, Arc::new(UnavailableRecoveryStore));

    assert_eq!(
        block_on(capability.refresh("profile_test".to_owned()))
            .unwrap()
            .state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
    );
    assert_eq!(
        block_on(capability.authorize(confirmation(true))),
        Err(WalletDustSettlementError::Driver(
            WalletDustRegistrationDriverError::Executor(
                WalletDustRegistrationExecutorFailure::Unavailable
            )
        ))
    );
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "submit")
            .count(),
        0
    );
}

#[test]
fn blocked_recovery_save_does_not_block_projection_refresh() {
    let fake = Arc::new(FakeServices::new());
    let store = Arc::new(BlockingRecoveryStore::default());
    let capability = capability_with_store(&fake, store.clone());
    let (result_sender, result_receiver) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        result_sender
            .send(block_on(capability.refresh("profile_test".to_owned())))
            .unwrap();
    });

    let result = result_receiver.recv_timeout(Duration::from_secs(1));
    store.release();
    assert_eq!(
        result
            .expect("projection refresh must not wait for recovery I/O")
            .unwrap()
            .state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
    );
}

#[test]
fn blocked_recovery_save_holds_broadcast_at_the_durability_barrier() {
    let fake = Arc::new(FakeServices::new());
    let store = Arc::new(BlockingRecoveryStore::default());
    let capability = capability_with_store(&fake, store.clone());
    block_on(capability.refresh("profile_test".to_owned())).unwrap();
    let (result_sender, result_receiver) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        result_sender
            .send(block_on(capability.authorize(confirmation(true))))
            .unwrap();
    });

    std::thread::sleep(Duration::from_millis(25));
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "submit")
            .count(),
        0
    );
    assert!(result_receiver.try_recv().is_err());
    store.release();
    assert_eq!(
        result_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("authorization must resume after recovery persistence")
            .unwrap()
            .state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
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
fn later_projection_cannot_replace_the_submission_checkpoint_before_verification() {
    let fake = Arc::new(FakeServices::new());
    let capability = capability(&fake);
    let projection = block_on(capability.refresh("profile_test".to_owned())).unwrap();
    let mut submitting = WalletDustRegistrationRecoveryRecord::from_projection(&projection)
        .expect("prepared projection has a recovery record");
    let later = submitting.clone();
    submitting.state = oxid_wallet_application::WalletDustRegistrationSettlementState::Submitting;
    submitting
        .registration
        .as_mut()
        .expect("prepared record retains its registration")
        .authorization_phase =
        oxid_wallet_application::WalletDustRegistrationSettlementAuthorizationPhase::Submitting;
    let store = Arc::new(InMemoryWalletDustRegistrationRecoveryStore::default());
    let persistence =
        RecoveryPersistence::spawn(store.clone(), Arc::new(AtomicBool::new(false))).unwrap();

    persistence.enqueue(submitting);
    persistence.enqueue(later);

    let submission_generation = block_on(persistence.await_submission())
        .expect("the exact submission checkpoint must be pinned");
    assert_eq!(
        store.load().unwrap().unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Submitting
    );
    persistence.release_submission(submission_generation);
    assert!(block_on(persistence.await_latest()));
    assert_eq!(
        store.load().unwrap().unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
    );
}

#[test]
fn stale_projection_cannot_overwrite_a_newer_submission_checkpoint_after_release() {
    let fake = Arc::new(FakeServices::new());
    let capability = capability(&fake);
    let projection = block_on(capability.refresh("profile_test".to_owned())).unwrap();
    let stale = WalletDustRegistrationRecoveryRecord::from_projection(&projection)
        .expect("prepared projection has a recovery record");
    let mut submitting = stale.clone();
    submitting.state = oxid_wallet_application::WalletDustRegistrationSettlementState::Submitting;
    submitting
        .registration
        .as_mut()
        .expect("prepared record retains its registration")
        .authorization_phase =
        oxid_wallet_application::WalletDustRegistrationSettlementAuthorizationPhase::Submitting;
    let store = Arc::new(InMemoryWalletDustRegistrationRecoveryStore::default());
    let persistence =
        RecoveryPersistence::spawn(store.clone(), Arc::new(AtomicBool::new(false))).unwrap();

    persistence.enqueue(stale);
    persistence.enqueue(submitting);

    let submission_generation = block_on(persistence.await_submission())
        .expect("the newer submission checkpoint must be pinned");
    persistence.release_submission(submission_generation);
    assert!(block_on(persistence.await_latest()));
    assert_eq!(
        store.load().unwrap().unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Submitting
    );
}

#[test]
fn each_submission_waiter_releases_the_generation_the_worker_pinned() {
    let fake = Arc::new(FakeServices::new());
    let capability = capability(&fake);
    let projection = block_on(capability.refresh("profile_test".to_owned())).unwrap();
    let later = WalletDustRegistrationRecoveryRecord::from_projection(&projection)
        .expect("prepared projection has a recovery record");
    let mut first = WalletDustRegistrationRecoveryRecord::from_projection(&projection)
        .expect("prepared projection has a recovery record");
    first.state = oxid_wallet_application::WalletDustRegistrationSettlementState::Submitting;
    first
        .registration
        .as_mut()
        .expect("prepared record retains its registration")
        .authorization_phase =
        oxid_wallet_application::WalletDustRegistrationSettlementAuthorizationPhase::Submitting;
    let mut second = first.clone();
    second
        .registration
        .as_mut()
        .expect("prepared record retains its registration")
        .draft_id = "draft_test_2".to_owned();
    let store = Arc::new(InMemoryWalletDustRegistrationRecoveryStore::default());
    let persistence = RecoveryPersistence::spawn(store, Arc::new(AtomicBool::new(false))).unwrap();

    persistence.enqueue(first);
    let first_generation = block_on(persistence.await_submission())
        .expect("the first submission checkpoint must be pinned");
    persistence.enqueue(second);
    persistence.release_submission(first_generation);
    let second_generation = block_on(persistence.await_submission())
        .expect("the second submission checkpoint must be pinned");
    assert!(second_generation > first_generation);
    persistence.enqueue(later);
    persistence.release_submission(second_generation);
    assert!(block_on(persistence.await_latest()));
}

#[test]
fn integrity_failure_is_preserved_and_blocks_broadcast_without_cleanup() {
    let fake = Arc::new(FakeServices::new());
    let clear_calls = Arc::new(AtomicUsize::new(0));
    let capability = capability_with_store(
        &fake,
        Arc::new(IntegrityRecoveryStore {
            clear_calls: clear_calls.clone(),
        }),
    );

    assert_eq!(clear_calls.load(Ordering::Acquire), 0);
    assert_eq!(
        block_on(capability.refresh("profile_test".to_owned()))
            .unwrap()
            .state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
    );
    assert_eq!(
        block_on(capability.authorize(confirmation(true))),
        Err(WalletDustSettlementError::Driver(
            WalletDustRegistrationDriverError::Executor(
                WalletDustRegistrationExecutorFailure::Unavailable
            )
        ))
    );
    assert_eq!(clear_calls.load(Ordering::Acquire), 0);
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "submit")
            .count(),
        0
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
fn automatic_reconciler_continues_a_pending_settlement_without_another_realm_tick() {
    let fake = Arc::new(FakeServices::new());
    *fake.reconcile_state.lock().unwrap() = "broadcasting".to_owned();
    let capability = Arc::new(automatic_capability(&fake));
    let reconciler = AutomaticDustRealmReconciler::new(
        fake.clone(),
        fake.clone(),
        fake.clone(),
        Arc::clone(&capability),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime builds");

    runtime.block_on(async {
        ReconcileSelectedWalletRealmUseCase::execute(
            &reconciler,
            SelectedWalletRealmSyncCommand {
                profile_id: "profile_test".to_owned(),
            },
            WalletRealmReconciliationTrigger::Initial,
        )
        .await
        .expect("initial reconciliation");
        assert_eq!(
            capability.projection().unwrap().state,
            oxid_wallet_application::WalletDustRegistrationSettlementState::Confirming
        );

        *fake.reconcile_state.lock().unwrap() = "included".to_owned();
        let continuation = tokio::time::timeout(Duration::from_millis(250), async {
            loop {
                if capability.projection().unwrap().state
                    == oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(
            continuation.is_ok(),
            "continuation reaches ready: projection={:?} calls={:?}",
            capability.projection(),
            fake.calls.lock().unwrap()
        );
    });

    assert!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "reconcile")
            .count()
            >= 2
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
    let failed = capability.projection().unwrap();
    assert_eq!(
        failed.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Degraded
    );
    assert_eq!(failed.checkpoint.unwrap().revision, 7);
    *fake.sync_ready.lock().unwrap() = Ok(true);
    assert_eq!(
        block_on(capability.retry()).unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::Ready
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
fn retries_are_capped_without_repeating_preparation_or_submission() {
    let fake = Arc::new(FakeServices::new());
    *fake.registration_prepare_failure.lock().unwrap() =
        Some(WalletDustRegistrationPortError::Timeout);
    let capability = capability(&fake);
    assert!(block_on(capability.refresh("profile_test".to_owned())).is_err());
    for _ in 0..MAX_SETTLEMENT_RETRIES {
        assert!(block_on(capability.retry()).is_err());
    }
    assert_eq!(
        block_on(capability.retry()),
        Err(WalletDustSettlementError::RetryNotAdmitted)
    );
    assert_eq!(
        fake.calls.lock().unwrap().len(),
        usize::from(MAX_SETTLEMENT_RETRIES) + 1
    );
    assert_eq!(
        capability.projection().unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::TimedOut
    );
}

#[test]
fn preparation_failure_retry_keeps_the_selected_checkpoint() {
    let fake = Arc::new(FakeServices::new());
    *fake.registration_prepare_failure.lock().unwrap() =
        Some(WalletDustRegistrationPortError::Timeout);
    let capability = capability(&fake);
    assert!(block_on(capability.refresh("profile_test".to_owned())).is_err());
    let failed = capability.projection().unwrap();
    assert_eq!(
        failed.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::TimedOut
    );
    assert_eq!(failed.checkpoint.unwrap().revision, 7);
    *fake.registration_prepare_failure.lock().unwrap() = None;
    assert_eq!(
        block_on(capability.retry()).unwrap().state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
    );
    assert_eq!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "submit")
            .count(),
        0
    );
}

#[test]
fn application_deadline_drops_pending_prepare_and_releases_admission() {
    let fake = Arc::new(FakeServices::new());
    fake.prepare_pending.store(true, Ordering::Release);
    let capability = capability_with_deadline(&fake, Duration::from_millis(5));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime builds");

    assert!(
        runtime
            .block_on(capability.refresh("profile_test".to_owned()))
            .is_err()
    );
    let timed_out = capability.projection().expect("projection is available");
    assert_eq!(
        timed_out.state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::TimedOut
    );
    assert_eq!(
        timed_out.checkpoint.expect("checkpoint retained").revision,
        7
    );
    assert_eq!(fake.prepare_drops.load(Ordering::Acquire), 1);

    fake.prepare_pending.store(false, Ordering::Release);
    assert_eq!(
        runtime
            .block_on(capability.retry())
            .expect("released admission permits retry")
            .state,
        oxid_wallet_application::WalletDustRegistrationSettlementState::AwaitingAuthorization
    );
}
