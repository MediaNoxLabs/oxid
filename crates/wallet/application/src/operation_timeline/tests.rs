// SPDX-License-Identifier: Apache-2.0

use super::*;

fn resource(revision: u64) -> WalletOperationResource {
    WalletOperationResource {
        identity: WalletOperationResourceIdentity::SelectedWalletRealm {
            profile: WalletProfileId::parse("profile_test").expect("profile id"),
            realm: ChainNetworkId::parse("undeployed").expect("network id"),
        },
        revision,
    }
}

#[test]
fn timeline_orders_records_and_evicts_the_oldest_deterministically() {
    let timeline = WalletOperationTimeline::with_capacity(3).expect("capacity");
    let trigger = WalletOperationTrigger::ManualRefresh;
    let (operation, correlation, admission) = timeline
        .begin_operation(resource(7), trigger)
        .expect("admission");
    let planned = timeline
        .record(
            operation,
            correlation,
            Some(admission),
            resource(7),
            trigger,
            WalletOperationAttempt::new(1).expect("attempt"),
            WalletOperationDurationMillis::zero(),
            WalletOperationEvent::EffectPlanned(WalletOperationEffect::SyncAccount),
        )
        .expect("planned");
    let completed = timeline
        .record(
            operation,
            correlation,
            Some(planned),
            resource(7),
            trigger,
            WalletOperationAttempt::new(1).expect("attempt"),
            WalletOperationDurationMillis::new(4).expect("duration"),
            WalletOperationEvent::EffectCompleted {
                effect: WalletOperationEffect::SyncAccount,
                outcome: WalletOperationOutcome::Succeeded,
                failure: None,
            },
        )
        .expect("completed");
    timeline
        .record(
            operation,
            correlation,
            Some(completed),
            resource(7),
            trigger,
            WalletOperationAttempt::new(1).expect("attempt"),
            WalletOperationDurationMillis::new(5).expect("duration"),
            WalletOperationEvent::Terminal {
                outcome: WalletOperationOutcome::Succeeded,
                failure: None,
            },
        )
        .expect("terminal");

    let snapshot = timeline.query().expect("snapshot");
    assert_eq!(snapshot.total_records(), 4);
    assert_eq!(snapshot.evicted_records(), 1);
    assert_eq!(
        snapshot
            .records()
            .iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert_eq!(snapshot.records()[0].caused_by, Some(admission));
    assert_eq!(snapshot.records()[1].caused_by, Some(planned));
    assert_eq!(snapshot.records()[2].caused_by, Some(completed));
}

#[test]
fn retry_causality_and_duration_attempt_outcome_aggregates_are_typed() {
    let timeline = WalletOperationTimeline::with_capacity(8).expect("capacity");
    let trigger = WalletOperationTrigger::Initial;
    let (operation, correlation, admission) = timeline
        .begin_operation(resource(1), trigger)
        .expect("admission");
    let first_plan = timeline
        .record(
            operation,
            correlation,
            Some(admission),
            resource(1),
            trigger,
            WalletOperationAttempt::new(1).expect("attempt"),
            WalletOperationDurationMillis::zero(),
            WalletOperationEvent::EffectPlanned(WalletOperationEffect::SyncDust),
        )
        .expect("first plan");
    let first_outcome = timeline
        .record(
            operation,
            correlation,
            Some(first_plan),
            resource(1),
            trigger,
            WalletOperationAttempt::new(1).expect("attempt"),
            WalletOperationDurationMillis::new(7).expect("duration"),
            WalletOperationEvent::EffectCompleted {
                effect: WalletOperationEffect::SyncDust,
                outcome: WalletOperationOutcome::Failed,
                failure: Some(WalletOperationFailure::AdapterUnavailable),
            },
        )
        .expect("first outcome");
    let retry_plan = timeline
        .record(
            operation,
            correlation,
            Some(first_outcome),
            resource(2),
            trigger,
            WalletOperationAttempt::new(2).expect("attempt"),
            WalletOperationDurationMillis::zero(),
            WalletOperationEvent::EffectPlanned(WalletOperationEffect::SyncDust),
        )
        .expect("retry plan");
    let retry_outcome = timeline
        .record(
            operation,
            correlation,
            Some(retry_plan),
            resource(2),
            trigger,
            WalletOperationAttempt::new(2).expect("attempt"),
            WalletOperationDurationMillis::new(5).expect("duration"),
            WalletOperationEvent::EffectCompleted {
                effect: WalletOperationEffect::SyncDust,
                outcome: WalletOperationOutcome::Succeeded,
                failure: None,
            },
        )
        .expect("retry outcome");
    timeline
        .record(
            operation,
            correlation,
            Some(retry_outcome),
            resource(2),
            trigger,
            WalletOperationAttempt::new(2).expect("attempt"),
            WalletOperationDurationMillis::new(12).expect("duration"),
            WalletOperationEvent::Terminal {
                outcome: WalletOperationOutcome::PartialFailure,
                failure: None,
            },
        )
        .expect("terminal");

    let snapshot = timeline.query().expect("snapshot");
    let aggregate = snapshot.aggregates();
    assert_eq!(aggregate.total_duration_millis, 24);
    assert_eq!(aggregate.maximum_attempt, 2);
    assert_eq!(aggregate.outcomes.failed, 1);
    assert_eq!(aggregate.outcomes.succeeded, 1);
    assert_eq!(aggregate.outcomes.partial_failure, 1);
    assert_eq!(snapshot.records()[3].caused_by, Some(first_outcome));
    assert_eq!(snapshot.records()[4].caused_by, Some(retry_plan));
}

#[test]
fn measurements_are_closed_bounded_and_evicted_with_their_record() {
    let timeline = WalletOperationTimeline::with_capacity(1).expect("capacity");
    let trigger = WalletOperationTrigger::Initial;
    let (operation, correlation, admission) = timeline
        .begin_operation(resource(1), trigger)
        .expect("admission");
    let measurements = WalletOperationResourceMeasurements::from_values(vec![
        WalletOperationResourceMeasurement::CurrentCursor(4),
        WalletOperationResourceMeasurement::TargetCursor(9),
        WalletOperationResourceMeasurement::EventsProcessed(3),
    ]);
    timeline
        .record_with_measurements(
            operation,
            correlation,
            Some(admission),
            resource(1),
            trigger,
            WalletOperationAttempt::new(1).expect("attempt"),
            WalletOperationDurationMillis::zero(),
            measurements,
            WalletOperationEvent::EffectCompleted {
                effect: WalletOperationEffect::SyncDust,
                outcome: WalletOperationOutcome::Succeeded,
                failure: None,
            },
        )
        .expect("completion");

    let snapshot = timeline.query().expect("snapshot");
    assert_eq!(snapshot.evicted_records(), 1);
    assert_eq!(snapshot.records().len(), 1);
    assert_eq!(
        snapshot.records()[0].measurements.as_slice(),
        [
            WalletOperationResourceMeasurement::CurrentCursor(4),
            WalletOperationResourceMeasurement::TargetCursor(9),
            WalletOperationResourceMeasurement::EventsProcessed(3),
        ]
    );
}

#[test]
#[should_panic(expected = "resource measurements are bounded")]
fn measurements_reject_unbounded_values() {
    let _ = WalletOperationResourceMeasurements::from_values(vec![
        WalletOperationResourceMeasurement::CurrentCursor(0);
        WalletOperationResourceMeasurements::MAX_VALUES + 1
    ]);
}

#[test]
fn bounded_values_are_rejected_before_storage() {
    let timeline = WalletOperationTimeline::with_capacity(2).expect("capacity");
    assert_eq!(
        WalletOperationAttempt::new(0),
        Err(WalletOperationValueError::AttemptOutOfRange)
    );
    assert_eq!(
        WalletOperationDurationMillis::new(MAX_WALLET_OPERATION_DURATION_MILLIS + 1),
        Err(WalletOperationValueError::DurationOutOfRange)
    );
    assert!(timeline.query().expect("snapshot").records().is_empty());
}
