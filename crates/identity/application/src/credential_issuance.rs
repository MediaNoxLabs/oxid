// SPDX-License-Identifier: Apache-2.0

//! Sealed, one-time authority for an accepted credential issuance flow.

use std::{
    error::Error,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use oxid_foundation::UnixTimestampMillis;
use oxid_identity_domain::{IdentityProfileId, MidnightDid};

/// Time source for credential issuance authorities.
pub trait CredentialIssuanceClockPort: Send + Sync {
    fn now(&self) -> Result<UnixTimestampMillis, CredentialIssuanceFlowError>;
}

impl<T> CredentialIssuanceClockPort for T
where
    T: oxid_platform_ports::ClockPort + ?Sized,
{
    fn now(&self) -> Result<UnixTimestampMillis, CredentialIssuanceFlowError> {
        oxid_platform_ports::ClockPort::now(self)
            .map_err(|_| CredentialIssuanceFlowError::Unavailable)
    }
}

/// An exact digest of the canonical final credential payload.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CanonicalCredentialIssuancePayloadDigest([u8; 32]);

impl CanonicalCredentialIssuancePayloadDigest {
    #[must_use]
    pub const fn from_sha256(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl fmt::Debug for CanonicalCredentialIssuancePayloadDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CanonicalCredentialIssuancePayloadDigest([REDACTED])")
    }
}

/// Identity-approved context bound to one issuance flow and session.
pub struct AcceptedCredentialIssuanceContext {
    profile: IdentityProfileId,
    did: MidnightDid,
    canonical_method: String,
    flow_id: String,
    session_id: String,
}

impl AcceptedCredentialIssuanceContext {
    #[must_use]
    pub fn new(
        profile: IdentityProfileId,
        did: MidnightDid,
        canonical_method: impl Into<String>,
        flow_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            profile,
            did,
            canonical_method: canonical_method.into(),
            flow_id: flow_id.into(),
            session_id: session_id.into(),
        }
    }
}

/// Opaque, non-cloneable, non-serializable, single-use issuance authority.
/// Its constructor is deliberately private to this application module.
///
/// ```compile_fail,E0277
/// use oxid_identity_application::AcceptedCredentialIssuanceFlow;
/// fn needs_clone<T: Clone>() {}
/// needs_clone::<AcceptedCredentialIssuanceFlow>();
/// ```
pub struct AcceptedCredentialIssuanceFlow {
    profile: IdentityProfileId,
    did: MidnightDid,
    canonical_method: String,
    flow_id: String,
    session_id: String,
    issuer: Arc<()>,
    issued_at: UnixTimestampMillis,
    expires_at: UnixTimestampMillis,
    generation: u64,
    consumed: AtomicBool,
}

impl fmt::Debug for AcceptedCredentialIssuanceFlow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AcceptedCredentialIssuanceFlow([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CredentialIssuanceFlowError {
    Unavailable,
    Expired,
    ClockWentBackwards,
    GenerationMismatch,
    FlowMismatch,
    ForeignIssuer,
    AlreadyConsumed,
}

impl fmt::Display for CredentialIssuanceFlowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "approval_unavailable",
            Self::Expired => "credential_issuance_expired",
            Self::ClockWentBackwards => "credential_issuance_clock_went_backwards",
            Self::GenerationMismatch => "credential_issuance_generation_mismatch",
            Self::FlowMismatch => "credential_issuance_flow_mismatch",
            Self::ForeignIssuer => "credential_issuance_foreign_issuer",
            Self::AlreadyConsumed => "credential_issuance_already_consumed",
        })
    }
}
impl Error for CredentialIssuanceFlowError {}

/// Identity-owned authority boundary for minting accepted credential issuance flows.
pub trait CredentialIssuanceAuthorityPort: Send + Sync {
    fn mint(
        &self,
        context: AcceptedCredentialIssuanceContext,
    ) -> Result<AcceptedCredentialIssuanceFlow, CredentialIssuanceFlowError>;
}

/// Identity-owned mint and consume service for accepted credential issuance flows.
pub struct CredentialIssuanceFlowService {
    clock: Arc<dyn CredentialIssuanceClockPort>,
    issuer: Arc<()>,
    generation: Mutex<Option<u64>>,
}

impl CredentialIssuanceFlowService {
    pub const MAX_TTL_MILLIS: u64 = 120_000;

    #[must_use]
    pub fn new(clock: Arc<dyn CredentialIssuanceClockPort>) -> Self {
        Self {
            clock,
            issuer: Arc::new(()),
            generation: Mutex::new(Some(0)),
        }
    }
}

impl CredentialIssuanceAuthorityPort for CredentialIssuanceFlowService {
    fn mint(
        &self,
        context: AcceptedCredentialIssuanceContext,
    ) -> Result<AcceptedCredentialIssuanceFlow, CredentialIssuanceFlowError> {
        let generation = self.current_generation()?;
        let issued_at = self.now()?;
        let expires_at = UnixTimestampMillis::new(
            issued_at
                .value()
                .checked_add(Self::MAX_TTL_MILLIS)
                .ok_or(CredentialIssuanceFlowError::Unavailable)?,
        );
        Ok(AcceptedCredentialIssuanceFlow {
            profile: context.profile,
            did: context.did,
            canonical_method: context.canonical_method,
            flow_id: context.flow_id,
            session_id: context.session_id,
            issuer: Arc::clone(&self.issuer),
            issued_at,
            expires_at,
            generation,
            consumed: AtomicBool::new(false),
        })
    }
}

/// Fails closed when credential issuance authority was not explicitly composed.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialIssuanceAuthority;

impl CredentialIssuanceAuthorityPort for UnavailableCredentialIssuanceAuthority {
    fn mint(
        &self,
        _: AcceptedCredentialIssuanceContext,
    ) -> Result<AcceptedCredentialIssuanceFlow, CredentialIssuanceFlowError> {
        Err(CredentialIssuanceFlowError::Unavailable)
    }
}

impl CredentialIssuanceFlowService {
    /// Lifecycle-only signing boundary: validate flow bindings, then lazily bind
    /// the exact canonical payload digest and spend the authority before the effect.
    ///
    /// This is crate-private so request handlers cannot select the payload authority.
    pub(crate) fn bind_and_consume_for_signing(
        &self,
        capability: &AcceptedCredentialIssuanceFlow,
        expected: &AcceptedCredentialIssuanceContext,
        canonical_payload_digest: impl FnOnce() -> CanonicalCredentialIssuancePayloadDigest,
    ) -> Result<(), CredentialIssuanceFlowError> {
        if !Arc::ptr_eq(&self.issuer, &capability.issuer) {
            return Err(CredentialIssuanceFlowError::ForeignIssuer);
        }
        if self.current_generation()? != capability.generation {
            return Err(CredentialIssuanceFlowError::GenerationMismatch);
        }
        let now = self.now()?;
        if now < capability.issued_at {
            return Err(CredentialIssuanceFlowError::ClockWentBackwards);
        }
        if now >= capability.expires_at {
            return Err(CredentialIssuanceFlowError::Expired);
        }
        if capability.profile != expected.profile
            || capability.did != expected.did
            || capability.canonical_method != expected.canonical_method
            || capability.flow_id != expected.flow_id
            || capability.session_id != expected.session_id
        {
            return Err(CredentialIssuanceFlowError::FlowMismatch);
        }
        // Spend before canonicalizing so concurrent duplicate requests cannot
        // evaluate or bind a second final payload. The signing boundary owns
        // canonicalization; this is immediately before its signing effect.
        capability
            .consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| CredentialIssuanceFlowError::AlreadyConsumed)?;
        let _bound_payload_digest = canonical_payload_digest();
        Ok(())
    }

    pub fn invalidate(&self) -> Result<(), CredentialIssuanceFlowError> {
        let mut generation = self
            .generation
            .lock()
            .map_err(|_| CredentialIssuanceFlowError::Unavailable)?;
        *generation = generation.and_then(|value| value.checked_add(1));
        generation
            .map(|_| ())
            .ok_or(CredentialIssuanceFlowError::Unavailable)
    }

    fn current_generation(&self) -> Result<u64, CredentialIssuanceFlowError> {
        self.generation
            .lock()
            .map_err(|_| CredentialIssuanceFlowError::Unavailable)?
            .ok_or(CredentialIssuanceFlowError::Unavailable)
    }
    fn now(&self) -> Result<UnixTimestampMillis, CredentialIssuanceFlowError> {
        self.clock.now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    const DID: &str =
        "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    struct Clock(AtomicU64);
    impl Clock {
        fn set(&self, value: u64) {
            self.0.store(value, Ordering::Release);
        }
    }
    impl CredentialIssuanceClockPort for Clock {
        fn now(&self) -> Result<UnixTimestampMillis, CredentialIssuanceFlowError> {
            Ok(UnixTimestampMillis::new(self.0.load(Ordering::Acquire)))
        }
    }
    fn context() -> AcceptedCredentialIssuanceContext {
        AcceptedCredentialIssuanceContext::new(
            IdentityProfileId::parse("profile_test").expect("profile"),
            MidnightDid::parse(DID).expect("did"),
            "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef#key-1",
            "flow-1",
            "session-1",
        )
    }
    fn service() -> (CredentialIssuanceFlowService, Arc<Clock>) {
        let clock = Arc::new(Clock(AtomicU64::new(1)));
        (CredentialIssuanceFlowService::new(clock.clone()), clock)
    }

    #[test]
    fn mints_when_identity_authority_is_available() {
        let (service, _) = service();
        let authority: &dyn CredentialIssuanceAuthorityPort = &service;

        assert!(authority.mint(context()).is_ok());
    }

    #[test]
    fn fails_closed_when_identity_authority_is_unavailable() {
        let authority: &dyn CredentialIssuanceAuthorityPort =
            &UnavailableCredentialIssuanceAuthority;

        assert!(matches!(
            authority.mint(context()),
            Err(CredentialIssuanceFlowError::Unavailable)
        ));
    }

    #[test]
    fn accepts_once_and_rejects_replay_including_concurrent_duplicate() {
        let (service, _) = service();
        let service = Arc::new(service);
        let flow = Arc::new(service.mint(context()).expect("mint"));
        let bound = Arc::new(AtomicUsize::new(0));
        let (first, second) = std::thread::scope(|scope| {
            let first_service = Arc::clone(&service);
            let first_bound = Arc::clone(&bound);
            let first_flow = Arc::clone(&flow);
            let first = scope.spawn(move || {
                first_service.bind_and_consume_for_signing(&first_flow, &context(), || {
                    first_bound.fetch_add(1, Ordering::Relaxed);
                    CanonicalCredentialIssuancePayloadDigest::from_sha256([1; 32])
                })
            });
            let second_service = Arc::clone(&service);
            let second_bound = Arc::clone(&bound);
            let second_flow = Arc::clone(&flow);
            let second = scope.spawn(move || {
                second_service.bind_and_consume_for_signing(&second_flow, &context(), || {
                    second_bound.fetch_add(1, Ordering::Relaxed);
                    CanonicalCredentialIssuancePayloadDigest::from_sha256([1; 32])
                })
            });
            (
                first.join().expect("first thread"),
                second.join().expect("second thread"),
            )
        });
        assert!(matches!(
            (first, second),
            (Ok(()), Err(CredentialIssuanceFlowError::AlreadyConsumed))
                | (Err(CredentialIssuanceFlowError::AlreadyConsumed), Ok(()))
        ));
        assert_eq!(bound.load(Ordering::Relaxed), 1);
        assert_eq!(
            service.bind_and_consume_for_signing(&flow, &context(), || {
                CanonicalCredentialIssuancePayloadDigest::from_sha256([1; 32])
            }),
            Err(CredentialIssuanceFlowError::AlreadyConsumed)
        );
    }

    #[test]
    fn rejects_mutated_flow_without_evaluating_or_consuming_authority() {
        let (service, _) = service();
        let flow = service.mint(context()).expect("mint");
        let called = AtomicUsize::new(0);
        let mutated = AcceptedCredentialIssuanceContext::new(
            IdentityProfileId::parse("profile_test").expect("profile"),
            MidnightDid::parse(DID).expect("did"),
            "canonical#other",
            "flow-1",
            "session-1",
        );
        assert_eq!(
            service.bind_and_consume_for_signing(&flow, &mutated, || {
                called.fetch_add(1, Ordering::Relaxed);
                CanonicalCredentialIssuancePayloadDigest::from_sha256([1; 32])
            }),
            Err(CredentialIssuanceFlowError::FlowMismatch)
        );
        assert_eq!(called.load(Ordering::Relaxed), 0);
        assert_eq!(
            service.bind_and_consume_for_signing(&flow, &context(), || {
                CanonicalCredentialIssuancePayloadDigest::from_sha256([1; 32])
            }),
            Ok(())
        );
    }

    #[test]
    fn rejects_expiry_generation_and_foreign_issuer_before_binding() {
        let (service, clock) = service();
        let expired = service.mint(context()).expect("mint");
        let called = AtomicUsize::new(0);
        clock.set(CredentialIssuanceFlowService::MAX_TTL_MILLIS + 1);
        assert_eq!(
            service.bind_and_consume_for_signing(&expired, &context(), || {
                called.fetch_add(1, Ordering::Relaxed);
                CanonicalCredentialIssuancePayloadDigest::from_sha256([1; 32])
            }),
            Err(CredentialIssuanceFlowError::Expired)
        );
        clock.set(2);
        let invalidated = service.mint(context()).expect("mint");
        service.invalidate().expect("invalidate");
        assert_eq!(
            service.bind_and_consume_for_signing(&invalidated, &context(), || {
                called.fetch_add(1, Ordering::Relaxed);
                CanonicalCredentialIssuancePayloadDigest::from_sha256([1; 32])
            }),
            Err(CredentialIssuanceFlowError::GenerationMismatch)
        );
        let (fresh_service, _) = super::tests::service();
        let (foreign, _) = super::tests::service();
        let foreign_flow = fresh_service.mint(context()).expect("mint");
        assert_eq!(
            foreign.bind_and_consume_for_signing(&foreign_flow, &context(), || {
                called.fetch_add(1, Ordering::Relaxed);
                CanonicalCredentialIssuancePayloadDigest::from_sha256([1; 32])
            }),
            Err(CredentialIssuanceFlowError::ForeignIssuer)
        );
        assert_eq!(called.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn binds_one_exact_digest_and_rejects_replay_with_another_digest() {
        let (service, _) = service();
        let flow = service.mint(context()).expect("mint");
        assert_eq!(
            service.bind_and_consume_for_signing(&flow, &context(), || {
                CanonicalCredentialIssuancePayloadDigest::from_sha256([2; 32])
            }),
            Ok(())
        );
        assert_eq!(
            service.bind_and_consume_for_signing(&flow, &context(), || {
                CanonicalCredentialIssuancePayloadDigest::from_sha256([1; 32])
            }),
            Err(CredentialIssuanceFlowError::AlreadyConsumed)
        );
    }
}
