// SPDX-License-Identifier: Apache-2.0

use std::sync::{Barrier, atomic::AtomicU64};

use oxid_platform_ports::PlatformError;

use super::*;

#[derive(Default)]
struct TestClock(AtomicU64);

impl ClockPort for TestClock {
    fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
        Ok(UnixTimestampMillis::new(self.0.load(Ordering::SeqCst)))
    }
}

struct TrustedFixture;

impl TrustedWalletApprovalPort for TrustedFixture {
    fn approve(&self, _: &WalletApprovalIntent) -> Result<(), WalletApprovalError> {
        Ok(())
    }
}

fn fixture() -> (WalletApprovalService, Arc<TestClock>) {
    let clock = Arc::new(TestClock::default());
    let service = WalletApprovalService::with_trusted_port(clock.clone(), Arc::new(TrustedFixture));
    (service, clock)
}

fn sign(profile: &str, digest: u8) -> WalletApprovalRequest<SignDataApproval> {
    WalletApprovalRequest::sign_data(
        WalletProfileId::parse(profile).expect("profile"),
        CanonicalApprovalDigest::from_sha256([digest; 32]),
    )
}

fn expiry() -> UnixTimestampMillis {
    UnixTimestampMillis::new(100)
}

#[test]
fn default_composition_cannot_mint_but_explicit_trusted_fixture_can() {
    let request = sign("profile_private", 42);
    let unavailable = WalletApprovalService::new(Arc::new(TestClock::default()));
    assert_eq!(
        unavailable.request(&request, expiry()).unwrap_err(),
        WalletApprovalError::Unavailable
    );
    let (service, _) = fixture();
    let capability = service
        .request(&request, expiry())
        .expect("trusted approval");
    assert_eq!(service.consume(&capability, &request), Ok(()));
    assert_eq!(
        service.consume(&capability, &request),
        Err(WalletApprovalError::AlreadyConsumed)
    );
}

#[test]
fn independent_profile_and_digest_binding_fail_closed_without_burning_matching_authority() {
    let (service, _) = fixture();
    let request = sign("profile_private", 42);
    let capability = service.request(&request, expiry()).expect("approval");
    for changed in [sign("profile_other", 42), sign("profile_private", 43)] {
        assert_eq!(
            service.consume(&capability, &changed),
            Err(WalletApprovalError::IntentMismatch)
        );
    }
    assert_eq!(service.consume(&capability, &request), Ok(()));
}

#[test]
fn duplicate_concurrent_consumers_have_exactly_one_winner() {
    let (service, _) = fixture();
    let request = sign("profile_private", 42);
    let capability = service.request(&request, expiry()).expect("approval");
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
                .filter(|result| **result == Err(WalletApprovalError::AlreadyConsumed))
                .count(),
            7
        );
    });
}

#[test]
fn expiry_boundary_and_backward_clock_fail_closed() {
    let (service, clock) = fixture();
    let request = sign("profile_private", 42);
    clock.0.store(10, Ordering::SeqCst);
    let capability = service.request(&request, expiry()).expect("approval");
    clock.0.store(9, Ordering::SeqCst);
    assert_eq!(
        service.consume(&capability, &request),
        Err(WalletApprovalError::Expired)
    );
    clock.0.store(100, Ordering::SeqCst);
    assert_eq!(
        service.consume(&capability, &request),
        Err(WalletApprovalError::Expired)
    );
    assert_eq!(
        service.request(&request, expiry()).unwrap_err(),
        WalletApprovalError::Expired
    );
}

#[test]
fn generation_change_and_recomposition_reject_old_authority() {
    let (service, _) = fixture();
    let request = sign("profile_private", 42);
    let old = service.request(&request, expiry()).expect("approval");
    service.invalidate().expect("invalidate");
    assert_eq!(
        service.consume(&old, &request),
        Err(WalletApprovalError::GenerationMismatch)
    );
    let fresh = service.request(&request, expiry()).expect("fresh approval");
    let (replacement, _) = fixture();
    assert_eq!(
        replacement.consume(&fresh, &request),
        Err(WalletApprovalError::ForeignCapability)
    );
    assert_eq!(service.consume(&fresh, &request), Ok(()));
}

#[test]
fn delete_is_separately_typed_and_operation_binding_is_checked_defensively() {
    let (service, _) = fixture();
    let request = WalletApprovalRequest::delete_key(
        WalletProfileId::parse("profile_private").expect("profile"),
        CanonicalApprovalDigest::from_sha256([42; 32]),
    );
    let capability = service
        .request(&request, expiry())
        .expect("delete approval");
    assert_eq!(service.consume(&capability, &request), Ok(()));
    let signing = sign("profile_private", 42);
    let capability = service.request(&signing, expiry()).expect("sign approval");
    // Only module-internal tests can violate the sealed operation invariant.
    let forged = WalletApprovalRequest::<SignDataApproval> {
        intent: request.intent,
        operation: PhantomData,
    };
    assert_eq!(
        service.consume(&capability, &forged),
        Err(WalletApprovalError::IntentMismatch)
    );
}

struct RejectingPort(WalletApprovalError);
impl TrustedWalletApprovalPort for RejectingPort {
    fn approve(&self, _: &WalletApprovalIntent) -> Result<(), WalletApprovalError> {
        Err(self.0)
    }
}

#[test]
fn denial_unavailability_and_clock_failure_never_mint() {
    let request = sign("profile_private", 42);
    for error in [
        WalletApprovalError::Denied,
        WalletApprovalError::Unavailable,
    ] {
        let service = WalletApprovalService::with_trusted_port(
            Arc::new(TestClock::default()),
            Arc::new(RejectingPort(error)),
        );
        assert_eq!(service.request(&request, expiry()).unwrap_err(), error);
    }
    struct BrokenClock;
    impl ClockPort for BrokenClock {
        fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
            Err(PlatformError::ClockUnavailable)
        }
    }
    let service =
        WalletApprovalService::with_trusted_port(Arc::new(BrokenClock), Arc::new(TrustedFixture));
    assert_eq!(
        service.request(&request, expiry()).unwrap_err(),
        WalletApprovalError::Unavailable
    );
}

struct PausedApproval {
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
}
impl TrustedWalletApprovalPort for PausedApproval {
    fn approve(&self, _: &WalletApprovalIntent) -> Result<(), WalletApprovalError> {
        self.entered.wait();
        self.release.wait();
        Ok(())
    }
}

#[test]
fn late_approval_after_invalidation_or_expiry_cannot_mint() {
    for invalidate in [true, false] {
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let clock = Arc::new(TestClock::default());
        let service = WalletApprovalService::with_trusted_port(
            clock.clone(),
            Arc::new(PausedApproval {
                entered: entered.clone(),
                release: release.clone(),
            }),
        );
        let request = sign("profile_private", 42);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| service.request(&request, expiry()));
            entered.wait();
            let expected = if invalidate {
                service.invalidate().expect("invalidate");
                WalletApprovalError::GenerationMismatch
            } else {
                clock.0.store(100, Ordering::SeqCst);
                WalletApprovalError::Expired
            };
            release.wait();
            assert_eq!(worker.join().expect("worker").unwrap_err(), expected);
        });
    }
}

#[test]
fn exhausted_generation_never_wraps_or_recovers_old_authority() {
    let (service, _) = fixture();
    *service.generation.lock().expect("generation") = Some(u64::MAX);
    let request = sign("profile_private", 42);
    let capability = service.request(&request, expiry()).expect("approval");
    assert_eq!(service.invalidate(), Err(WalletApprovalError::Unavailable));
    assert_eq!(
        service.consume(&capability, &request),
        Err(WalletApprovalError::Unavailable)
    );
    assert_eq!(
        service.request(&request, expiry()).unwrap_err(),
        WalletApprovalError::Unavailable
    );
}

#[test]
fn diagnostics_are_closed_and_redacted() {
    let (service, _) = fixture();
    let request = sign("profile_private", 42);
    let capability = service.request(&request, expiry()).expect("approval");
    assert_eq!(
        format!("{:?}", request.intent),
        "WalletApprovalIntent([REDACTED])"
    );
    assert_eq!(format!("{request:?}"), "WalletApprovalRequest([REDACTED])");
    assert_eq!(
        format!("{capability:?}"),
        "WalletApprovalCapability([REDACTED])"
    );
    assert_eq!(
        format!("{:?}", CanonicalApprovalDigest::from_sha256([42; 32])),
        "CanonicalApprovalDigest([REDACTED])"
    );
    for error in [
        WalletApprovalError::Unavailable,
        WalletApprovalError::Denied,
        WalletApprovalError::Expired,
        WalletApprovalError::GenerationMismatch,
        WalletApprovalError::IntentMismatch,
        WalletApprovalError::ForeignCapability,
        WalletApprovalError::AlreadyConsumed,
    ] {
        assert!(error.to_string().len() <= 32);
        assert!(!format!("{error:?} {error}").contains("profile_private"));
    }
}
