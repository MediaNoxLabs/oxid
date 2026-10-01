// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::DidApprovalError;

fn operations() -> Vec<DidUpdate> {
    use DidKeyAlgorithm::*;
    use DidUpdate::*;
    use VerificationRelationship::*;
    vec![
        AddAlsoKnownAs { value: "a".into() },
        AddAlsoKnownAs { value: "b".into() },
        RemoveAlsoKnownAs { value: "a".into() },
        RemoveAlsoKnownAs { value: "b".into() },
        AddVerificationMethod {
            fragment: "a".into(),
            algorithm: Ed25519,
        },
        AddVerificationMethod {
            fragment: "b".into(),
            algorithm: Ed25519,
        },
        AddVerificationMethod {
            fragment: "a".into(),
            algorithm: P256,
        },
        AddVerificationMethod {
            fragment: "a".into(),
            algorithm: Jubjub,
        },
        UpdateVerificationMethod {
            method_id: "a".into(),
            algorithm: Ed25519,
        },
        UpdateVerificationMethod {
            method_id: "b".into(),
            algorithm: Ed25519,
        },
        UpdateVerificationMethod {
            method_id: "a".into(),
            algorithm: P256,
        },
        UpdateVerificationMethod {
            method_id: "a".into(),
            algorithm: Jubjub,
        },
        RemoveVerificationMethod {
            method_id: "a".into(),
        },
        RemoveVerificationMethod {
            method_id: "b".into(),
        },
        AddVerificationRelationship {
            relationship: Authentication,
            method_id: "a".into(),
        },
        AddVerificationRelationship {
            relationship: Authentication,
            method_id: "b".into(),
        },
        AddVerificationRelationship {
            relationship: AssertionMethod,
            method_id: "a".into(),
        },
        AddVerificationRelationship {
            relationship: CapabilityInvocation,
            method_id: "a".into(),
        },
        AddVerificationRelationship {
            relationship: CapabilityDelegation,
            method_id: "a".into(),
        },
        RemoveVerificationRelationship {
            relationship: Authentication,
            method_id: "a".into(),
        },
        RemoveVerificationRelationship {
            relationship: Authentication,
            method_id: "b".into(),
        },
        RemoveVerificationRelationship {
            relationship: AssertionMethod,
            method_id: "a".into(),
        },
        RemoveVerificationRelationship {
            relationship: CapabilityInvocation,
            method_id: "a".into(),
        },
        RemoveVerificationRelationship {
            relationship: CapabilityDelegation,
            method_id: "a".into(),
        },
        AddService {
            id: "a".into(),
            service_type: "a".into(),
            endpoint: "a".into(),
        },
        AddService {
            id: "b".into(),
            service_type: "a".into(),
            endpoint: "a".into(),
        },
        AddService {
            id: "a".into(),
            service_type: "b".into(),
            endpoint: "a".into(),
        },
        AddService {
            id: "a".into(),
            service_type: "a".into(),
            endpoint: "b".into(),
        },
        UpdateService {
            id: "a".into(),
            service_type: "a".into(),
            endpoint: "a".into(),
        },
        UpdateService {
            id: "b".into(),
            service_type: "a".into(),
            endpoint: "a".into(),
        },
        UpdateService {
            id: "a".into(),
            service_type: "b".into(),
            endpoint: "a".into(),
        },
        UpdateService {
            id: "a".into(),
            service_type: "a".into(),
            endpoint: "b".into(),
        },
        RemoveService { id: "a".into() },
        RemoveService { id: "b".into() },
    ]
}

#[test]
fn every_update_variant_and_field_is_bound_before_effect() {
    let (service, lifecycle) = service_with_lifecycle((None, None), None);
    let profile = parse_profile(PROFILE.into()).unwrap();
    let did = parse_did(DID.into()).unwrap();
    let prior = current(&service, &profile, &did).unwrap();
    for approved in operations() {
        let request = update_request(hash(&service).unwrap(), &profile, &did, &approved);
        let capability = approvals(&service).unwrap().request(&request).unwrap();
        for changed in operations().into_iter().filter(|value| value != &approved) {
            let expected = update_request(hash(&service).unwrap(), &profile, &did, &changed);
            assert_eq!(
                consume(&service, &prior, &capability, &expected),
                Err(DidOperationError::Approval(
                    DidApprovalError::IntentMismatch
                ))
            );
        }
        consume(&service, &prior, &capability, &request).unwrap();
    }
    assert_eq!(lifecycle.update_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn signing_scope_method_payload_and_framing_are_exact() {
    let (service, _) = service_with_lifecycle((None, None), None);
    let hash = TestHash::default();
    let profile = parse_profile(PROFILE.into()).unwrap();
    let did = parse_did(DID.into()).unwrap();
    let prior = current(&service, &profile, &did).unwrap();
    let request = sign_request(&hash, &profile, &did, "ab", b"c");
    let capability = approvals(&service).unwrap().request(&request).unwrap();
    for (profile, did, method, payload) in [
        (
            parse_profile("other".into()).unwrap(),
            did.clone(),
            "ab",
            b"c".as_slice(),
        ),
        (
            profile.clone(),
            parse_did(format!("did:midnight:undeployed:{:064x}", 1)).unwrap(),
            "ab",
            b"c",
        ),
        (profile.clone(), did.clone(), "a", b"bc"),
        (profile.clone(), did.clone(), "ab", b"d"),
    ] {
        let expected = sign_request(&hash, &profile, &did, method, payload);
        assert_eq!(
            consume(&service, &prior, &capability, &expected),
            Err(DidOperationError::Approval(
                DidApprovalError::IntentMismatch
            ))
        );
    }
    let frames = hash.0.lock().unwrap();
    let mut expected = Vec::new();
    for field in ["oxid.did.lifecycle", "1", "sign", PROFILE, DID, "ab", "c"] {
        expected.extend_from_slice(&(field.len() as u64).to_be_bytes());
        expected.extend_from_slice(field.as_bytes());
    }
    assert_eq!(frames[0], expected);
}

#[test]
fn expiry_generation_foreign_issuer_and_concurrent_replay_never_reach_effect() {
    let (mut service, lifecycle) = service_with_lifecycle((None, None), None);
    let (approval, clock) = crate::approval::tests::service();
    let approval = Arc::new(approval);
    service = service.with_approvals(approval.clone(), Arc::new(TestHash::default()));
    let profile = parse_profile(PROFILE.into()).unwrap();
    let did = parse_did(DID.into()).unwrap();
    let prior = current(&service, &profile, &did).unwrap();
    let request = deactivate_request(hash(&service).unwrap(), &profile, &did);
    let expired = approval.request(&request).unwrap();
    clock
        .0
        .store(DidApprovalService::MAX_TTL_MILLIS, Ordering::SeqCst);
    assert_eq!(
        consume(&service, &prior, &expired, &request),
        Err(DidOperationError::Approval(DidApprovalError::Expired))
    );
    let stale = approval.request(&request).unwrap();
    approval.invalidate().unwrap();
    assert_eq!(
        consume(&service, &prior, &stale, &request),
        Err(DidOperationError::Approval(
            DidApprovalError::GenerationMismatch
        ))
    );
    let foreign = crate::approval::tests::service()
        .0
        .request(&request)
        .unwrap();
    assert_eq!(
        consume(&service, &prior, &foreign, &request),
        Err(DidOperationError::Approval(
            DidApprovalError::ForeignCapability
        ))
    );
    assert_eq!(lifecycle.deactivate_calls.load(Ordering::SeqCst), 0);
    let capability = approval.request(&request).unwrap();
    std::thread::scope(|scope| {
        let results: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    consume(&service, &prior, &capability, &request)?;
                    service
                        .lifecycle
                        .deactivate(prior.profile_id(), prior.resolution())
                        .map_err(DidOperationError::Lifecycle)?;
                    Ok::<(), DidOperationError>(())
                })
            })
            .collect();
        assert_eq!(
            results
                .into_iter()
                .map(|result| result.join().unwrap())
                .filter(Result::is_ok)
                .count(),
            1
        );
    });
    assert_eq!(lifecycle.deactivate_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        consume(&service, &prior, &capability, &request),
        Err(DidOperationError::Approval(
            DidApprovalError::AlreadyConsumed
        ))
    );
}

#[test]
fn defaults_and_retained_record_drift_reject_all_protected_effects() {
    for drift in [false, true] {
        let (mut service, lifecycle) = service_with_lifecycle((None, None), None);
        if !drift {
            service.approvals = None;
        }
        let expected = if drift {
            DidOperationError::RetainedRecordChanged
        } else {
            DidOperationError::Approval(DidApprovalError::Unavailable)
        };
        // Reset the repository per command: first read is the approved record,
        // second read changes publication state even with the same document.
        for operation in 0..3 {
            service.repository = Arc::new(TestRepository {
                get_error: None,
                upsert_error: None,
                drift,
                reads: AtomicUsize::new(0),
            });
            let result = match operation {
                0 => UpdateDidUseCase::execute(&service, update_command()).map(|_| ()),
                1 => DeactivateDidUseCase::execute(
                    &service,
                    DeactivateDidCommand {
                        profile_id: PROFILE.into(),
                        did: DID.into(),
                    },
                )
                .map(|_| ()),
                _ => SignDidPayloadUseCase::execute(&service, sign_command(b"payload")).map(|_| ()),
            };
            assert_eq!(result, Err(expected.clone()));
        }
        assert_eq!(lifecycle.update_calls.load(Ordering::SeqCst), 0);
        assert_eq!(lifecycle.deactivate_calls.load(Ordering::SeqCst), 0);
        assert!(lifecycle.sign_calls.lock().unwrap().is_empty());
    }
}

#[test]
fn normalized_values_are_used_for_both_intent_and_effect() {
    let (service, lifecycle) = service_with_lifecycle((None, None), None);
    let mut command = sign_command(b" payload ");
    command.method_id = "  #auth-1  ".into();
    SignDidPayloadUseCase::execute(&service, command).unwrap();
    let calls = lifecycle.sign_calls.lock().unwrap();
    assert_eq!(calls[0].method_id, "#auth-1");
    assert_eq!(calls[0].payload, b" payload ");
    for operation in operations() {
        assert_eq!(normalize_update(operation.clone()), operation);
    }
    assert_eq!(
        normalize_update(DidUpdate::AddService {
            id: " a ".into(),
            service_type: " b ".into(),
            endpoint: " c ".into()
        }),
        DidUpdate::AddService {
            id: "a".into(),
            service_type: "b".into(),
            endpoint: "c".into()
        }
    );
}

#[test]
fn failed_effect_does_not_restore_approval() {
    let (service, lifecycle) =
        service_with_lifecycle((None, None), Some(DidLifecyclePortError::Conflict));
    let profile = parse_profile(PROFILE.into()).unwrap();
    let did = parse_did(DID.into()).unwrap();
    let prior = current(&service, &profile, &did).unwrap();
    let request = deactivate_request(hash(&service).unwrap(), &profile, &did);
    let capability = approvals(&service).unwrap().request(&request).unwrap();
    consume(&service, &prior, &capability, &request).unwrap();
    assert_eq!(
        service.lifecycle.deactivate(&profile, prior.resolution()),
        Err(DidLifecyclePortError::Conflict)
    );
    assert_eq!(
        consume(&service, &prior, &capability, &request),
        Err(DidOperationError::Approval(
            DidApprovalError::AlreadyConsumed
        ))
    );
    assert_eq!(lifecycle.deactivate_calls.load(Ordering::SeqCst), 1);
}
