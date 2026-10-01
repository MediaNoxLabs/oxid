// SPDX-License-Identifier: Apache-2.0

use super::*;
use oxid_platform_ports::PlatformError;
use std::sync::{Barrier, atomic::AtomicU64};

#[derive(Default)]
struct TestClock(AtomicU64);
impl ClockPort for TestClock {
    fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
        Ok(UnixTimestampMillis::new(self.0.load(Ordering::SeqCst)))
    }
}
struct TrustedFixture;
impl TrustedWalletApprovalPort for TrustedFixture {
    fn approve(&self, _: &WalletApprovalIntent) -> Result<(), TrustedWalletApprovalError> {
        Ok(())
    }
}
fn fixture() -> (WalletApprovalService, Arc<TestClock>) {
    let clock = Arc::new(TestClock::default());
    (
        WalletApprovalService::with_trusted_port(clock.clone(), Arc::new(TrustedFixture)),
        clock,
    )
}
fn sign(profile: &str, digest: u8) -> WalletApprovalRequest<SignDataApproval> {
    WalletApprovalRequest::sign_data(
        WalletProfileId::parse(profile).expect("profile"),
        CanonicalApprovalDigest::from_sha256([digest; 32]),
    )
}
#[test]
fn default_composition_cannot_mint_and_trusted_approval_is_single_use() {
    let request = sign("profile_private", 42);
    assert_eq!(
        WalletApprovalService::new(Arc::new(TestClock::default()))
            .request(&request)
            .unwrap_err(),
        WalletApprovalError::Unavailable
    );
    let (service, _) = fixture();
    let cap = service.request(&request).expect("approval");
    assert_eq!(service.consume(&cap, &request), Ok(()));
    assert_eq!(
        service.consume(&cap, &request),
        Err(WalletApprovalError::AlreadyConsumed)
    );
}
#[test]
fn profile_and_each_digest_byte_mismatch_preserve_matching_authority() {
    let (service, _) = fixture();
    let request = sign("profile_private", 42);
    let mut mismatches = vec![sign("profile_other", 42)];
    for index in 0..32 {
        let mut bytes = [42; 32];
        bytes[index] ^= 1;
        mismatches.push(WalletApprovalRequest::sign_data(
            WalletProfileId::parse("profile_private").unwrap(),
            CanonicalApprovalDigest::from_sha256(bytes),
        ));
    }
    for changed in mismatches {
        let cap = service.request(&request).unwrap();
        assert_eq!(
            service.consume(&cap, &changed),
            Err(WalletApprovalError::IntentMismatch)
        );
        assert_eq!(service.consume(&cap, &request), Ok(()));
    }
    let fresh = service.request(&request).unwrap();
    assert_eq!(service.consume(&fresh, &request), Ok(()));
}
#[test]
fn duplicate_concurrent_consumers_have_exactly_one_winner() {
    let (service, _) = fixture();
    let request = sign("profile_private", 42);
    let cap = service.request(&request).unwrap();
    let barrier = Barrier::new(8);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    service.consume(&cap, &request)
                })
            })
            .collect();
        let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|r| **r == Err(WalletApprovalError::AlreadyConsumed))
                .count(),
            7
        );
    });
}
#[test]
fn ttl_comes_from_application_clock_and_overflow_never_mints() {
    let (service, clock) = fixture();
    let request = sign("profile_private", 42);
    clock.0.store(10, Ordering::SeqCst);
    let cap = service.request(&request).unwrap();
    assert_eq!(cap.issued_at.value(), 10);
    assert_eq!(
        cap.expires_at.value(),
        10 + WalletApprovalService::MAX_TTL_MILLIS
    );
    clock.0.store(cap.expires_at.value() - 1, Ordering::SeqCst);
    assert_eq!(service.consume(&cap, &request), Ok(()));
    clock.0.store(u64::MAX, Ordering::SeqCst);
    assert_eq!(
        service.request(&request).unwrap_err(),
        WalletApprovalError::Unavailable
    );
}
#[test]
fn expiry_equality_and_backward_clock_have_distinct_safe_errors() {
    let (service, clock) = fixture();
    let request = sign("profile_private", 42);
    clock.0.store(10, Ordering::SeqCst);
    let cap = service.request(&request).unwrap();
    clock.0.store(9, Ordering::SeqCst);
    assert_eq!(
        service.consume(&cap, &request),
        Err(WalletApprovalError::ClockWentBackwards)
    );
    clock.0.store(cap.expires_at.value(), Ordering::SeqCst);
    assert_eq!(
        service.consume(&cap, &request),
        Err(WalletApprovalError::Expired)
    );
}
#[test]
fn invalidation_and_recomposition_reject_old_authority() {
    let (service, _) = fixture();
    let request = sign("profile_private", 42);
    let old = service.request(&request).unwrap();
    service.invalidate().unwrap();
    assert_eq!(
        service.consume(&old, &request),
        Err(WalletApprovalError::GenerationMismatch)
    );
    let cap = service.request(&request).unwrap();
    assert_eq!(
        fixture().0.consume(&cap, &request),
        Err(WalletApprovalError::ForeignCapability)
    );
    assert_eq!(service.consume(&cap, &request), Ok(()));
}
#[test]
fn delete_is_typed_and_operation_binding_checked_defensively() {
    let (service, _) = fixture();
    let request = WalletApprovalRequest::delete_key(
        WalletProfileId::parse("profile_private").unwrap(),
        CanonicalApprovalDigest::from_sha256([42; 32]),
    );
    let cap = service.request(&request).unwrap();
    assert_eq!(service.consume(&cap, &request), Ok(()));
    let cap = service.request(&sign("profile_private", 42)).unwrap();
    let forged = WalletApprovalRequest::<SignDataApproval> {
        intent: request.intent,
        operation: PhantomData,
    };
    assert_eq!(
        service.consume(&cap, &forged),
        Err(WalletApprovalError::IntentMismatch)
    );
}
struct RejectingPort(TrustedWalletApprovalError);
impl TrustedWalletApprovalPort for RejectingPort {
    fn approve(&self, _: &WalletApprovalIntent) -> Result<(), TrustedWalletApprovalError> {
        Err(self.0)
    }
}
#[test]
fn narrow_port_errors_and_clock_failure_map_to_service_errors() {
    let request = sign("profile_private", 42);
    for (port, expected) in [
        (
            TrustedWalletApprovalError::Denied,
            WalletApprovalError::Denied,
        ),
        (
            TrustedWalletApprovalError::Unavailable,
            WalletApprovalError::Unavailable,
        ),
    ] {
        let service = WalletApprovalService::with_trusted_port(
            Arc::new(TestClock::default()),
            Arc::new(RejectingPort(port)),
        );
        assert_eq!(service.request(&request).unwrap_err(), expected);
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
        service.request(&request).unwrap_err(),
        WalletApprovalError::Unavailable
    );
}
struct PausedApproval {
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
}
impl TrustedWalletApprovalPort for PausedApproval {
    fn approve(&self, _: &WalletApprovalIntent) -> Result<(), TrustedWalletApprovalError> {
        self.entered.wait();
        self.release.wait();
        Ok(())
    }
}
#[test]
fn stale_prompt_results_cannot_mint() {
    for scenario in 0..3 {
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let clock = Arc::new(TestClock(AtomicU64::new(10)));
        let service = WalletApprovalService::with_trusted_port(
            clock.clone(),
            Arc::new(PausedApproval {
                entered: entered.clone(),
                release: release.clone(),
            }),
        );
        let request = sign("profile_private", 42);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| service.request(&request));
            entered.wait();
            let expected = match scenario {
                0 => {
                    service.invalidate().unwrap();
                    WalletApprovalError::GenerationMismatch
                }
                1 => {
                    clock
                        .0
                        .store(10 + WalletApprovalService::MAX_TTL_MILLIS, Ordering::SeqCst);
                    WalletApprovalError::Expired
                }
                _ => {
                    clock.0.store(9, Ordering::SeqCst);
                    WalletApprovalError::ClockWentBackwards
                }
            };
            release.wait();
            assert_eq!(worker.join().unwrap().unwrap_err(), expected);
        });
    }
}
#[test]
fn poisoned_and_exhausted_state_never_mints_consumes_or_recovers() {
    for poison in [false, true] {
        let (service, _) = fixture();
        let request = sign("profile_private", 42);
        if !poison {
            *service.generation.lock().unwrap() = Some(u64::MAX);
        }
        let cap = service.request(&request).unwrap();
        if poison {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = service.generation.lock().unwrap();
                panic!("deliberate poison");
            }));
        }
        assert_eq!(service.invalidate(), Err(WalletApprovalError::Unavailable));
        assert_eq!(
            service.consume(&cap, &request),
            Err(WalletApprovalError::Unavailable)
        );
        assert_eq!(
            service.request(&request).unwrap_err(),
            WalletApprovalError::Unavailable
        );
    }
}
#[test]
fn independent_capabilities_and_consume_before_failed_effect() {
    let (service, _) = fixture();
    let request = sign("profile_private", 42);
    let first = service.request(&request).unwrap();
    let second = service.request(&request).unwrap();
    let mut effects = 0;
    let mut protected = |cap| -> Result<(), WalletApprovalError> {
        service.consume(cap, &request)?;
        effects += 1;
        Err(WalletApprovalError::Unavailable)
    };
    assert_eq!(protected(&first), Err(WalletApprovalError::Unavailable));
    assert_eq!(protected(&first), Err(WalletApprovalError::AlreadyConsumed));
    assert_eq!(protected(&second), Err(WalletApprovalError::Unavailable));
    assert_eq!(effects, 2);
}
#[test]
fn diagnostics_are_redacted() {
    let (service, _) = fixture();
    let request = sign("profile_private", 42);
    let cap = service.request(&request).unwrap();
    assert_eq!(
        format!("{:?}", request.intent),
        "WalletApprovalIntent([REDACTED])"
    );
    assert_eq!(format!("{request:?}"), "WalletApprovalRequest([REDACTED])");
    assert_eq!(format!("{cap:?}"), "WalletApprovalCapability([REDACTED])");
    assert_eq!(
        format!("{:?}", CanonicalApprovalDigest::from_sha256([42; 32])),
        "CanonicalApprovalDigest([REDACTED])"
    );
    assert_eq!(
        WalletApprovalError::ClockWentBackwards.to_string(),
        "approval_clock_went_backwards"
    );
    for error in [
        WalletApprovalError::Unavailable,
        WalletApprovalError::Denied,
        WalletApprovalError::Expired,
        WalletApprovalError::ClockWentBackwards,
        WalletApprovalError::GenerationMismatch,
        WalletApprovalError::IntentMismatch,
        WalletApprovalError::ForeignCapability,
        WalletApprovalError::AlreadyConsumed,
    ] {
        assert!(error.to_string().len() <= 32);
        assert!(!format!("{error:?} {error}").contains("profile_private"));
    }
}
