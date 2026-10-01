// SPDX-License-Identifier: Apache-2.0

//! Sealed, one-time authority for accepted self-issued authentication.

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

/// Time source for self-issued authentication authorities.
pub trait SelfIssuedAuthenticationClockPort: Send + Sync {
    fn now(&self) -> Result<UnixTimestampMillis, SelfIssuedAuthenticationFlowError>;
}

impl<T> SelfIssuedAuthenticationClockPort for T
where
    T: oxid_platform_ports::ClockPort + ?Sized,
{
    fn now(&self) -> Result<UnixTimestampMillis, SelfIssuedAuthenticationFlowError> {
        oxid_platform_ports::ClockPort::now(self)
            .map_err(|_| SelfIssuedAuthenticationFlowError::Unavailable)
    }
}

/// An exact digest of the canonical final SIOPv2 ID-token signing input.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CanonicalSelfIssuedAuthenticationPayloadDigest([u8; 32]);

impl CanonicalSelfIssuedAuthenticationPayloadDigest {
    #[must_use]
    pub const fn from_sha256(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl fmt::Debug for CanonicalSelfIssuedAuthenticationPayloadDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CanonicalSelfIssuedAuthenticationPayloadDigest([REDACTED])")
    }
}

/// Identity-approved context bound to one SIOPv2 flow and session.
pub struct AcceptedSelfIssuedAuthenticationContext {
    profile: IdentityProfileId,
    did: MidnightDid,
    canonical_method: String,
    flow_id: String,
    session_id: String,
}

impl AcceptedSelfIssuedAuthenticationContext {
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

/// Opaque, non-cloneable, non-serializable, single-use authentication authority.
/// Its constructor is deliberately private to this application module.
///
/// ```compile_fail,E0277
/// use oxid_identity_application::AcceptedSelfIssuedAuthenticationFlow;
/// fn needs_clone<T: Clone>() {}
/// needs_clone::<AcceptedSelfIssuedAuthenticationFlow>();
/// ```
///
/// ```compile_fail,E0308
/// use oxid_identity_application::{
///     AcceptedCredentialIssuanceFlow, AcceptedSelfIssuedAuthenticationFlow,
/// };
/// fn authenticate(_: AcceptedSelfIssuedAuthenticationFlow) {}
/// fn cannot_substitute(issuance: AcceptedCredentialIssuanceFlow) {
///     authenticate(issuance);
/// }
/// ```
pub struct AcceptedSelfIssuedAuthenticationFlow {
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

impl fmt::Debug for AcceptedSelfIssuedAuthenticationFlow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AcceptedSelfIssuedAuthenticationFlow([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelfIssuedAuthenticationFlowError {
    Unavailable,
    Expired,
    ClockWentBackwards,
    GenerationMismatch,
    FlowMismatch,
    ForeignIssuer,
    AlreadyConsumed,
}

impl fmt::Display for SelfIssuedAuthenticationFlowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "approval_unavailable",
            Self::Expired => "self_issued_authentication_expired",
            Self::ClockWentBackwards => "self_issued_authentication_clock_went_backwards",
            Self::GenerationMismatch => "self_issued_authentication_generation_mismatch",
            Self::FlowMismatch => "self_issued_authentication_flow_mismatch",
            Self::ForeignIssuer => "self_issued_authentication_foreign_issuer",
            Self::AlreadyConsumed => "self_issued_authentication_already_consumed",
        })
    }
}
impl Error for SelfIssuedAuthenticationFlowError {}

/// Identity-owned boundary for minting accepted self-issued authentication flows.
pub trait SelfIssuedAuthenticationAuthorityPort: Send + Sync {
    fn mint(
        &self,
        context: AcceptedSelfIssuedAuthenticationContext,
    ) -> Result<AcceptedSelfIssuedAuthenticationFlow, SelfIssuedAuthenticationFlowError>;
}

/// Identity-owned mint and consume service for accepted authentication flows.
pub struct SelfIssuedAuthenticationFlowService {
    clock: Arc<dyn SelfIssuedAuthenticationClockPort>,
    issuer: Arc<()>,
    generation: Mutex<Option<u64>>,
}

impl SelfIssuedAuthenticationFlowService {
    pub const MAX_TTL_MILLIS: u64 = 120_000;

    #[must_use]
    pub fn new(clock: Arc<dyn SelfIssuedAuthenticationClockPort>) -> Self {
        Self {
            clock,
            issuer: Arc::new(()),
            generation: Mutex::new(Some(0)),
        }
    }
}

impl SelfIssuedAuthenticationAuthorityPort for SelfIssuedAuthenticationFlowService {
    fn mint(
        &self,
        context: AcceptedSelfIssuedAuthenticationContext,
    ) -> Result<AcceptedSelfIssuedAuthenticationFlow, SelfIssuedAuthenticationFlowError> {
        let generation = self.current_generation()?;
        let issued_at = self.now()?;
        let expires_at = UnixTimestampMillis::new(
            issued_at
                .value()
                .checked_add(Self::MAX_TTL_MILLIS)
                .ok_or(SelfIssuedAuthenticationFlowError::Unavailable)?,
        );
        Ok(AcceptedSelfIssuedAuthenticationFlow {
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

/// Fails closed when authentication authority was not explicitly composed.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableSelfIssuedAuthenticationAuthority;

impl SelfIssuedAuthenticationAuthorityPort for UnavailableSelfIssuedAuthenticationAuthority {
    fn mint(
        &self,
        _: AcceptedSelfIssuedAuthenticationContext,
    ) -> Result<AcceptedSelfIssuedAuthenticationFlow, SelfIssuedAuthenticationFlowError> {
        Err(SelfIssuedAuthenticationFlowError::Unavailable)
    }
}

impl SelfIssuedAuthenticationFlowService {
    /// Validate all flow bindings, bind the exact canonical signing-input digest,
    /// and spend the authority immediately before the signing effect.
    pub(crate) fn bind_and_consume_for_signing(
        &self,
        capability: &AcceptedSelfIssuedAuthenticationFlow,
        expected: &AcceptedSelfIssuedAuthenticationContext,
        canonical_payload_digest: impl FnOnce() -> CanonicalSelfIssuedAuthenticationPayloadDigest,
    ) -> Result<(), SelfIssuedAuthenticationFlowError> {
        if !Arc::ptr_eq(&self.issuer, &capability.issuer) {
            return Err(SelfIssuedAuthenticationFlowError::ForeignIssuer);
        }
        if self.current_generation()? != capability.generation {
            return Err(SelfIssuedAuthenticationFlowError::GenerationMismatch);
        }
        let now = self.now()?;
        if now < capability.issued_at {
            return Err(SelfIssuedAuthenticationFlowError::ClockWentBackwards);
        }
        if now >= capability.expires_at {
            return Err(SelfIssuedAuthenticationFlowError::Expired);
        }
        if capability.profile != expected.profile
            || capability.did != expected.did
            || capability.canonical_method != expected.canonical_method
            || capability.flow_id != expected.flow_id
            || capability.session_id != expected.session_id
        {
            return Err(SelfIssuedAuthenticationFlowError::FlowMismatch);
        }
        capability
            .consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| SelfIssuedAuthenticationFlowError::AlreadyConsumed)?;
        let _bound_payload_digest = canonical_payload_digest();
        Ok(())
    }

    pub fn invalidate(&self) -> Result<(), SelfIssuedAuthenticationFlowError> {
        let mut generation = self
            .generation
            .lock()
            .map_err(|_| SelfIssuedAuthenticationFlowError::Unavailable)?;
        *generation = generation.and_then(|value| value.checked_add(1));
        generation
            .map(|_| ())
            .ok_or(SelfIssuedAuthenticationFlowError::Unavailable)
    }

    fn current_generation(&self) -> Result<u64, SelfIssuedAuthenticationFlowError> {
        self.generation
            .lock()
            .map_err(|_| SelfIssuedAuthenticationFlowError::Unavailable)?
            .ok_or(SelfIssuedAuthenticationFlowError::Unavailable)
    }

    fn now(&self) -> Result<UnixTimestampMillis, SelfIssuedAuthenticationFlowError> {
        self.clock.now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    const DID: &str =
        "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    struct Clock(AtomicU64);

    impl SelfIssuedAuthenticationClockPort for Clock {
        fn now(&self) -> Result<UnixTimestampMillis, SelfIssuedAuthenticationFlowError> {
            Ok(UnixTimestampMillis::new(self.0.load(Ordering::Acquire)))
        }
    }

    fn context(session_id: &str) -> AcceptedSelfIssuedAuthenticationContext {
        AcceptedSelfIssuedAuthenticationContext::new(
            IdentityProfileId::parse("profile_test").expect("profile"),
            MidnightDid::parse(DID).expect("DID"),
            format!("{DID}#auth-1"),
            "siopv2",
            session_id,
        )
    }

    #[test]
    fn accepted_authentication_authority_is_exact_and_single_use() {
        let service = SelfIssuedAuthenticationFlowService::new(Arc::new(Clock(AtomicU64::new(1))));
        let authority = service.mint(context("authentication_1")).expect("mint");
        let evaluated = AtomicBool::new(false);
        assert_eq!(
            service.bind_and_consume_for_signing(
                &authority,
                &context("authentication_other"),
                || {
                    evaluated.store(true, Ordering::Release);
                    CanonicalSelfIssuedAuthenticationPayloadDigest::from_sha256([1; 32])
                },
            ),
            Err(SelfIssuedAuthenticationFlowError::FlowMismatch)
        );
        assert!(!evaluated.load(Ordering::Acquire));
        service
            .bind_and_consume_for_signing(&authority, &context("authentication_1"), || {
                evaluated.store(true, Ordering::Release);
                CanonicalSelfIssuedAuthenticationPayloadDigest::from_sha256([2; 32])
            })
            .expect("exact authority is accepted once");
        assert!(evaluated.load(Ordering::Acquire));
        assert_eq!(
            service.bind_and_consume_for_signing(&authority, &context("authentication_1"), || {
                CanonicalSelfIssuedAuthenticationPayloadDigest::from_sha256([2; 32])
            },),
            Err(SelfIssuedAuthenticationFlowError::AlreadyConsumed)
        );
    }

    #[test]
    fn unavailable_authentication_authority_fails_closed() {
        assert!(matches!(
            UnavailableSelfIssuedAuthenticationAuthority.mint(context("authentication_1")),
            Err(SelfIssuedAuthenticationFlowError::Unavailable)
        ));
    }
}
