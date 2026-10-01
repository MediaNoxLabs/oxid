// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::sync::{Barrier, atomic::AtomicU64};

#[derive(Default)]
struct TestClock(AtomicU64);
impl DidApprovalClockPort for TestClock {
    fn now(&self) -> Result<UnixTimestampMillis, DidApprovalClockError> {
        Ok(UnixTimestampMillis::new(self.0.load(Ordering::SeqCst)))
    }
}

struct Trusted;
impl TrustedDidApprovalPort for Trusted {
    fn approve(&self, _: &DidApprovalIntent) -> Result<(), TrustedDidApprovalError> {
        Ok(())
    }
}

fn profile(value: &str) -> IdentityProfileId {
    IdentityProfileId::parse(value).expect("profile")
}
fn did(value: u8) -> MidnightDid {
    MidnightDid::parse(format!("did:midnight:undeployed:{value:064x}")).expect("did")
}
fn update(profile_id: &str, did_byte: u8, digest: u8) -> DidApprovalRequest<UpdateDidApproval> {
    DidApprovalRequest::update(
        profile(profile_id),
        did(did_byte),
        CanonicalDidApprovalDigest::from_sha256([digest; 32]),
    )
}
fn deactivate(profile_id: &str, did_byte: u8) -> DidApprovalRequest<DeactivateDidApproval> {
    DidApprovalRequest::deactivate(profile(profile_id), did(did_byte))
}
fn sign(
    profile_id: &str,
    did_byte: u8,
    method_id: &str,
    digest: u8,
) -> DidApprovalRequest<SignDidApproval> {
    DidApprovalRequest::sign(
        profile(profile_id),
        did(did_byte),
        method_id,
        CanonicalDidApprovalDigest::from_sha256([digest; 32]),
    )
}
fn service() -> (DidApprovalService, Arc<TestClock>) {
    let clock = Arc::new(TestClock::default());
    (
        DidApprovalService::with_trusted_port(clock.clone(), Arc::new(Trusted)),
        clock,
    )
}

#[test]
fn default_service_is_inert_and_a_trusted_capability_is_single_use() {
    let request = update("profile_private", 1, 9);
    assert_eq!(
        DidApprovalService::new(Arc::new(TestClock::default()))
            .request(&request)
            .unwrap_err(),
        DidApprovalError::Unavailable
    );
    let (service, _) = service();
    let capability = service.request(&request).expect("approval");
    assert_eq!(service.consume(&capability, &request), Ok(()));
    assert_eq!(
        service.consume(&capability, &request),
        Err(DidApprovalError::AlreadyConsumed)
    );
}

#[test]
fn update_intent_binds_every_profile_did_and_digest_field_without_spending_on_mismatch() {
    let (service, _) = service();
    let request = update("profile_private", 1, 9);
    let mut mutations = vec![
        update("profile_other", 1, 9),
        update("profile_private", 2, 9),
    ];
    for byte in 0..32 {
        let mut digest = [9; 32];
        digest[byte] ^= 1;
        mutations.push(DidApprovalRequest::update(
            profile("profile_private"),
            did(1),
            CanonicalDidApprovalDigest::from_sha256(digest),
        ));
    }
    for mutation in mutations {
        let capability = service.request(&request).expect("approval");
        assert_eq!(
            service.consume(&capability, &mutation),
            Err(DidApprovalError::IntentMismatch)
        );
        assert_eq!(service.consume(&capability, &request), Ok(()));
    }
}

#[test]
fn deactivate_intent_binds_every_profile_and_did_field() {
    let (service, _) = service();
    let request = deactivate("profile_private", 1);
    for mutation in [
        deactivate("profile_other", 1),
        deactivate("profile_private", 2),
    ] {
        let capability = service.request(&request).expect("approval");
        assert_eq!(
            service.consume(&capability, &mutation),
            Err(DidApprovalError::IntentMismatch)
        );
        assert_eq!(service.consume(&capability, &request), Ok(()));
    }
}

#[test]
fn sign_intent_binds_every_profile_did_method_and_digest_field() {
    let (service, _) = service();
    let request = sign("profile_private", 1, "#auth-1", 9);
    let mut mutations = vec![
        sign("profile_other", 1, "#auth-1", 9),
        sign("profile_private", 2, "#auth-1", 9),
        sign("profile_private", 1, "#auth-2", 9),
    ];
    for byte in 0..32 {
        let mut digest = [9; 32];
        digest[byte] ^= 1;
        mutations.push(DidApprovalRequest::sign(
            profile("profile_private"),
            did(1),
            "#auth-1",
            CanonicalDidApprovalDigest::from_sha256(digest),
        ));
    }
    for mutation in mutations {
        let capability = service.request(&request).expect("approval");
        assert_eq!(
            service.consume(&capability, &mutation),
            Err(DidApprovalError::IntentMismatch)
        );
        assert_eq!(service.consume(&capability, &request), Ok(()));
    }
}

#[test]
fn operation_substitution_is_type_closed_and_defensively_rejected() {
    let (service, _) = service();
    let request = sign("profile_private", 1, "#auth-1", 9);
    let capability = service.request(&request).expect("approval");
    let substituted = DidApprovalRequest::<SignDidApproval> {
        intent: DidApprovalIntent::Update {
            profile: profile("profile_private"),
            did: did(1),
            digest: CanonicalDidApprovalDigest::from_sha256([9; 32]),
        },
        operation: PhantomData,
    };
    assert_eq!(
        service.consume(&capability, &substituted),
        Err(DidApprovalError::IntentMismatch)
    );
    assert_eq!(service.consume(&capability, &request), Ok(()));
}

#[test]
fn expiry_generation_and_foreign_issuer_reject_without_authorizing() {
    let (service, clock) = service();
    let request = update("profile_private", 1, 9);
    let expired = service.request(&request).expect("approval");
    clock
        .0
        .store(DidApprovalService::MAX_TTL_MILLIS, Ordering::SeqCst);
    assert_eq!(
        service.consume(&expired, &request),
        Err(DidApprovalError::Expired)
    );
    clock.0.store(0, Ordering::SeqCst);
    let stale = service.request(&request).expect("approval");
    service.invalidate().expect("invalidate");
    assert_eq!(
        service.consume(&stale, &request),
        Err(DidApprovalError::GenerationMismatch)
    );
    let current = service.request(&request).expect("approval");
    assert_eq!(
        DidApprovalService::with_trusted_port(clock, Arc::new(Trusted)).consume(&current, &request),
        Err(DidApprovalError::ForeignCapability)
    );
}

#[test]
fn concurrent_consumption_has_exactly_one_winner() {
    let (service, _) = service();
    let request = sign("profile_private", 1, "#auth-1", 9);
    let capability = service.request(&request).expect("approval");
    let barrier = Barrier::new(8);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    service.consume(&capability, &request)
                })
            })
            .collect();
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == Err(DidApprovalError::AlreadyConsumed))
                .count(),
            7
        );
    });
}
