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
    assert_eq!(calls[0].method_id, format!("{DID}#auth-1"));
    assert_eq!(calls[0].payload, b" payload ");
    let did = parse_did(DID.into()).unwrap();
    for operation in operations() {
        let normalized = normalize_update(&did, operation).unwrap();
        assert_eq!(
            normalize_update(&did, normalized.clone()).unwrap(),
            normalized
        );
    }
    assert_eq!(
        normalize_update(
            &did,
            DidUpdate::AddService {
                id: " a ".into(),
                service_type: " b ".into(),
                endpoint: " c ".into()
            }
        )
        .unwrap(),
        DidUpdate::AddService {
            id: format!("{DID}#a"),
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

mod normalization {
    use super::*;
    use crate::DidApprovalIntent;
    fn component_operations(id: &str) -> Vec<DidUpdate> {
        vec![
            DidUpdate::AddVerificationMethod {
                fragment: id.into(),
                algorithm: DidKeyAlgorithm::Ed25519,
            },
            DidUpdate::UpdateVerificationMethod {
                method_id: id.into(),
                algorithm: DidKeyAlgorithm::P256,
            },
            DidUpdate::RemoveVerificationMethod {
                method_id: id.into(),
            },
            DidUpdate::AddVerificationRelationship {
                method_id: id.into(),
                relationship: VerificationRelationship::Authentication,
            },
            DidUpdate::RemoveVerificationRelationship {
                method_id: id.into(),
                relationship: VerificationRelationship::Authentication,
            },
            DidUpdate::AddService {
                id: id.into(),
                service_type: " type ".into(),
                endpoint: " endpoint ".into(),
            },
            DidUpdate::UpdateService {
                id: id.into(),
                service_type: " type ".into(),
                endpoint: " endpoint ".into(),
            },
            DidUpdate::RemoveService { id: id.into() },
        ]
    }

    #[test]
    fn equivalent_component_spellings_have_identical_approval_and_effect() {
        let (service, lifecycle) = service_with_lifecycle((None, None), None);
        let recorder = Arc::new(Mutex::new(Vec::new()));
        let observed = recorder.clone();
        let approval = crate::approval::tests::service_with_observer(move |intent| {
            observed.lock().unwrap().push(intent.clone());
        });
        let hash = Arc::new(TestHash::default());
        let service = service.with_approvals(Arc::new(approval), hash.clone());
        let did = parse_did(DID.into()).unwrap();
        let profile = parse_profile(PROFILE.into()).unwrap();
        let canonical = format!("{DID}#auth-1");
        for spelling in [
            " auth-1 ".to_owned(),
            " #auth-1 ".into(),
            format!(" {canonical} "),
        ] {
            for (operation, expected) in component_operations(&spelling)
                .into_iter()
                .zip(component_operations(&canonical))
            {
                let expected = normalize_update(&did, expected).unwrap();
                let mut command = update_command();
                command.operation = operation;
                UpdateDidUseCase::execute(&service, command).unwrap();
                assert_eq!(lifecycle.updates.lock().unwrap().last(), Some(&expected));
                // The trusted producer saw exactly the independently reconstructed
                // intent for the canonical operation passed to the effect.
                let request = update_request(hash.as_ref(), &profile, &did, &expected);
                let capability = approvals(&service).unwrap().request(&request).unwrap();
                let intents = recorder.lock().unwrap();
                assert_eq!(intents[intents.len() - 2], intents[intents.len() - 1]);
                drop(capability);
            }
            let mut command = sign_command(b" payload ");
            command.method_id = spelling;
            SignDidPayloadUseCase::execute(&service, command).unwrap();
            assert_eq!(
                lifecycle
                    .sign_calls
                    .lock()
                    .unwrap()
                    .last()
                    .unwrap()
                    .method_id,
                canonical
            );
        }
        let intents = recorder.lock().unwrap();
        let signing: Vec<_> = intents
            .iter()
            .filter(|intent| matches!(intent, DidApprovalIntent::Sign { .. }))
            .collect();
        assert_eq!(signing.len(), 3);
        assert!(signing.windows(2).all(|pair| pair[0] == pair[1]));
        assert!(
            matches!(signing[0], DidApprovalIntent::Sign { method_id, .. } if method_id == &canonical)
        );
    }

    #[test]
    fn invalid_components_are_rejected_before_approval_or_effect() {
        let (service, lifecycle) = service_with_lifecycle((None, None), None);
        let recorder = Arc::new(Mutex::new(Vec::new()));
        let observed = recorder.clone();
        let service = service.with_approvals(
            Arc::new(crate::approval::tests::service_with_observer(
                move |intent| {
                    observed.lock().unwrap().push(intent.clone());
                },
            )),
            Arc::new(TestHash::default()),
        );
        for id in ["", "#", "a b", "a/b", "a#b", "did:example:other#auth-1"] {
            for operation in component_operations(id) {
                let mut command = update_command();
                command.operation = operation;
                assert_eq!(
                    UpdateDidUseCase::execute(&service, command),
                    Err(DidOperationError::Lifecycle(
                        DidLifecyclePortError::InvalidOperation
                    ))
                );
            }
            let mut command = sign_command(b"payload");
            command.method_id = id.into();
            assert_eq!(
                SignDidPayloadUseCase::execute(&service, command),
                Err(DidOperationError::Lifecycle(
                    DidLifecyclePortError::InvalidOperation
                ))
            );
        }
        assert!(recorder.lock().unwrap().is_empty());
        assert_eq!(lifecycle.update_calls.load(Ordering::SeqCst), 0);
        assert!(lifecycle.sign_calls.lock().unwrap().is_empty());
    }
}

mod serialization {
    // SPDX-License-Identifier: Apache-2.0
    use super::*;
    use crate::DidApprovalIntent;
    use std::{collections::BTreeMap, sync::mpsc, time::Duration};

    const WAIT: Duration = Duration::from_secs(5);

    struct Repository {
        records: Mutex<BTreeMap<String, DidRecord>>,
        persisting: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }
    impl DidRecordRepository for Repository {
        fn get(
            &self,
            _: &IdentityProfileId,
            did: &MidnightDid,
        ) -> Result<DidRecord, DidRecordRepositoryError> {
            Ok(self.records.lock().unwrap()[did.as_str()].clone())
        }
        fn upsert(&self, record: DidRecord) -> Result<(), DidRecordRepositoryError> {
            if record.resolution().document_metadata().deactivated == Some(true) {
                self.persisting.send(()).unwrap();
                self.release.lock().unwrap().recv_timeout(WAIT).unwrap();
            }
            self.records
                .lock()
                .unwrap()
                .insert(record.resolution().document().id().as_str().into(), record);
            Ok(())
        }
        fn list(&self, _: &IdentityProfileId) -> Result<Vec<DidRecord>, DidRecordRepositoryError> {
            Ok(vec![])
        }
        fn remove(
            &self,
            _: &IdentityProfileId,
            _: &MidnightDid,
        ) -> Result<(), DidRecordRepositoryError> {
            Ok(())
        }
    }

    struct Lifecycle {
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
        updates: AtomicUsize,
    }
    impl DidLifecyclePort for Lifecycle {
        fn create(
            &self,
            _: &IdentityProfileId,
            _: MidnightNetwork,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            unreachable!()
        }
        fn update(
            &self,
            _: &IdentityProfileId,
            current: &DidResolution,
            _: DidUpdate,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            self.updates.fetch_add(1, Ordering::SeqCst);
            // Deliberately trusts the supplied snapshot, exposing the original race.
            Ok(current.clone())
        }
        fn deactivate(
            &self,
            _: &IdentityProfileId,
            current: &DidResolution,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv_timeout(WAIT).unwrap();
            Ok(DidResolution::new(
                current.document().clone(),
                DidDocumentMetadata {
                    deactivated: Some(true),
                    ..DidDocumentMetadata::default()
                },
                DidResolutionMetadata::default(),
                DidResolutionSource::Standalone,
            ))
        }
        fn sign(
            &self,
            _: &IdentityProfileId,
            _: &DidResolution,
            method_id: &str,
            _: &[u8],
        ) -> Result<DidLifecycleSignature, DidLifecyclePortError> {
            Ok(DidLifecycleSignature {
                method_id: method_id.into(),
                algorithm: DidKeyAlgorithm::Ed25519,
                signature_bytes: vec![],
            })
        }
    }

    #[test]
    fn stale_update_cannot_overwrite_deactivation_across_service_instances() {
        // Fixed ordering, no sleeps: pause the effect, approve a second command from
        // A, then pause persistence. The shared lock must cover both pauses, while
        // a different DID completes. Only then release B and reject the stale A.
        let profile = parse_profile("serialization_profile".into()).unwrap();
        let did = parse_did(format!("did:midnight:undeployed:{:064x}", 914)).unwrap();
        let other = parse_did(format!("did:midnight:undeployed:{:064x}", 915)).unwrap();
        let records = [did.clone(), other.clone()]
            .into_iter()
            .map(|did| {
                let resolution = DidResolution::new(
                    DidDocument::new(DidDocumentParts {
                        contexts: vec![DID_CONTEXT.into(), JWK_CONTEXT.into()],
                        id: did.clone(),
                        controllers: vec![did.clone()],
                        also_known_as: vec![],
                        verification_methods: vec![],
                        relationships: vec![],
                        services: vec![],
                    })
                    .unwrap(),
                    DidDocumentMetadata::default(),
                    DidResolutionMetadata::default(),
                    DidResolutionSource::Standalone,
                );
                (
                    did.as_str().to_owned(),
                    DidRecord::new(profile.clone(), resolution),
                )
            })
            .collect();
        let (effect_tx, effect_rx) = mpsc::channel();
        let (release_effect_tx, release_effect_rx) = mpsc::channel();
        let (persist_tx, persist_rx) = mpsc::channel();
        let (release_persist_tx, release_persist_rx) = mpsc::channel();
        let (approved_tx, approved_rx) = mpsc::channel();
        let repository = Arc::new(Repository {
            records: Mutex::new(records),
            persisting: persist_tx,
            release: Mutex::new(release_persist_rx),
        });
        let lifecycle = Arc::new(Lifecycle {
            entered: effect_tx,
            release: Mutex::new(release_effect_rx),
            updates: AtomicUsize::new(0),
        });
        let approval = Arc::new(crate::approval::tests::service_with_observer(
            move |intent| {
                if matches!(intent, DidApprovalIntent::Update { .. }) {
                    approved_tx.send(()).unwrap();
                }
            },
        ));
        let make_service = || {
            DidService::from_ports(
                repository.clone(),
                Arc::new(UnavailableDidResolver),
                lifecycle.clone(),
            )
            .with_approvals(approval.clone(), Arc::new(TestHash::default()))
        };
        let deactivate = make_service();
        let update = make_service();
        let unrelated = make_service();
        std::thread::scope(|scope| {
            let deactivation = scope.spawn(|| {
                DeactivateDidUseCase::execute(
                    &deactivate,
                    DeactivateDidCommand {
                        profile_id: profile.as_str().into(),
                        did: did.as_str().into(),
                    },
                )
            });
            effect_rx.recv_timeout(WAIT).unwrap();
            let pending_update = scope.spawn(|| {
                UpdateDidUseCase::execute(
                    &update,
                    UpdateDidCommand {
                        profile_id: profile.as_str().into(),
                        did: did.as_str().into(),
                        operation: DidUpdate::AddAlsoKnownAs {
                            value: "stale".into(),
                        },
                    },
                )
            });
            approved_rx.recv_timeout(WAIT).unwrap();
            let lock = operation_lock(&profile, &did).unwrap();
            let effect_locked = matches!(lock.try_lock(), Err(std::sync::TryLockError::WouldBlock));
            // Must complete while the first DID is paused in its effect.
            let unrelated_result = SignDidPayloadUseCase::execute(
                &unrelated,
                SignDidPayloadCommand {
                    profile_id: profile.as_str().into(),
                    did: other.as_str().into(),
                    method_id: "auth-1".into(),
                    payload: b"payload",
                },
            );
            release_effect_tx.send(()).unwrap();
            persist_rx.recv_timeout(WAIT).unwrap();
            let persistence_locked =
                matches!(lock.try_lock(), Err(std::sync::TryLockError::WouldBlock));
            release_persist_tx.send(()).unwrap();
            assert!(deactivation.join().unwrap().is_ok());
            assert_eq!(
                pending_update.join().unwrap(),
                Err(DidOperationError::RetainedRecordChanged)
            );
            assert!(effect_locked && persistence_locked);
            assert!(unrelated_result.is_ok());
        });
        assert_eq!(lifecycle.updates.load(Ordering::SeqCst), 0);
        assert_eq!(
            repository
                .get(&profile, &did)
                .unwrap()
                .resolution()
                .document_metadata()
                .deactivated,
            Some(true)
        );
    }
}
