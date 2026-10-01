// SPDX-License-Identifier: Apache-2.0

//! Sealed, one-time authority for an accepted credential presentation.

use std::{
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

pub use oxid_foundation::AcceptedCredentialPresentationFlow;
use oxid_foundation::{
    AcceptedFlowIssuer, AcceptedFlowTokenUseError, CREDENTIAL_PRESENTATION_FLOW_KIND,
    UnixTimestampMillis,
};
use oxid_identity_domain::IdentityProfileId;

pub trait CredentialPresentationClockPort: Send + Sync {
    fn now(&self) -> Result<UnixTimestampMillis, CredentialPresentationFlowError>;
}

impl<T> CredentialPresentationClockPort for T
where
    T: oxid_platform_ports::ClockPort + ?Sized,
{
    fn now(&self) -> Result<UnixTimestampMillis, CredentialPresentationFlowError> {
        oxid_platform_ports::ClockPort::now(self)
            .map_err(|_| CredentialPresentationFlowError::Unavailable)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CanonicalCredentialPresentationBundleDigest([u8; 32]);

impl CanonicalCredentialPresentationBundleDigest {
    #[must_use]
    pub const fn from_sha256(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl fmt::Debug for CanonicalCredentialPresentationBundleDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CanonicalCredentialPresentationBundleDigest([REDACTED])")
    }
}

#[derive(PartialEq, Eq)]
pub struct AcceptedCredentialPresentationContext {
    profile: IdentityProfileId,
    flow_id: String,
    session_id: String,
    credential_id: String,
}

impl AcceptedCredentialPresentationContext {
    #[must_use]
    pub fn new(
        profile: IdentityProfileId,
        flow_id: impl Into<String>,
        session_id: impl Into<String>,
        credential_id: impl Into<String>,
    ) -> Self {
        Self {
            profile,
            flow_id: flow_id.into(),
            session_id: session_id.into(),
            credential_id: credential_id.into(),
        }
    }
}

/// Opaque, non-cloneable, non-serializable, single-use authority for exactly
/// one accepted presentation signing bundle.
///
/// ```compile_fail,E0277
/// use oxid_identity_application::AcceptedCredentialPresentationFlow;
/// fn needs_clone<T: Clone>() {}
/// needs_clone::<AcceptedCredentialPresentationFlow>();
/// ```
///
/// ```compile_fail,E0308
/// use oxid_identity_application::{
///     AcceptedCredentialIssuanceFlow, AcceptedCredentialPresentationFlow,
/// };
/// fn present(_: AcceptedCredentialPresentationFlow) {}
/// fn cannot_substitute(value: AcceptedCredentialIssuanceFlow) { present(value); }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CredentialPresentationFlowError {
    Unavailable,
    Expired,
    ClockWentBackwards,
    GenerationMismatch,
    FlowMismatch,
    ForeignIssuer,
    AlreadyConsumed,
}

impl fmt::Display for CredentialPresentationFlowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "approval_unavailable",
            Self::Expired => "credential_presentation_expired",
            Self::ClockWentBackwards => "credential_presentation_clock_went_backwards",
            Self::GenerationMismatch => "credential_presentation_generation_mismatch",
            Self::FlowMismatch => "credential_presentation_flow_mismatch",
            Self::ForeignIssuer => "credential_presentation_foreign_issuer",
            Self::AlreadyConsumed => "credential_presentation_already_consumed",
        })
    }
}
impl Error for CredentialPresentationFlowError {}

pub trait CredentialPresentationAuthorityPort: Send + Sync {
    fn mint(
        &self,
        context: AcceptedCredentialPresentationContext,
    ) -> Result<AcceptedCredentialPresentationFlow, CredentialPresentationFlowError>;
}

pub struct CredentialPresentationFlowService {
    clock: Arc<dyn CredentialPresentationClockPort>,
    issuer: AcceptedFlowIssuer<CREDENTIAL_PRESENTATION_FLOW_KIND>,
    generation: Mutex<Option<u64>>,
}

impl CredentialPresentationFlowService {
    pub const MAX_TTL_MILLIS: u64 = 120_000;

    #[must_use]
    pub fn new(clock: Arc<dyn CredentialPresentationClockPort>) -> Self {
        Self {
            clock,
            issuer: AcceptedFlowIssuer::new(),
            generation: Mutex::new(Some(0)),
        }
    }

    pub fn mint_for_accepted_transport(
        &self,
        profile_id: String,
        flow_id: String,
        session_id: String,
        credential_id: String,
    ) -> Result<AcceptedCredentialPresentationFlow, CredentialPresentationFlowError> {
        let profile = IdentityProfileId::parse(profile_id)
            .map_err(|_| CredentialPresentationFlowError::FlowMismatch)?;
        self.mint(AcceptedCredentialPresentationContext::new(
            profile,
            flow_id,
            session_id,
            credential_id,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    struct Clock(AtomicU64);
    impl CredentialPresentationClockPort for Clock {
        fn now(&self) -> Result<UnixTimestampMillis, CredentialPresentationFlowError> {
            Ok(UnixTimestampMillis::new(self.0.load(Ordering::Acquire)))
        }
    }

    fn context(session: &str) -> AcceptedCredentialPresentationContext {
        AcceptedCredentialPresentationContext::new(
            IdentityProfileId::parse("profile_test").expect("profile"),
            "openid4vp",
            session,
            "credential_test",
        )
    }

    #[test]
    fn presentation_authority_binds_one_bundle_and_rejects_replay() {
        let service = CredentialPresentationFlowService::new(Arc::new(Clock(AtomicU64::new(1))));
        let authority = service.mint(context("presentation_1")).expect("mint");
        let evaluated = AtomicBool::new(false);
        assert_eq!(
            service.bind_and_consume_for_signing(
                &authority,
                &context("presentation_other"),
                || {
                    evaluated.store(true, Ordering::Release);
                    CanonicalCredentialPresentationBundleDigest::from_sha256([1; 32])
                }
            ),
            Err(CredentialPresentationFlowError::FlowMismatch)
        );
        assert!(!evaluated.load(Ordering::Acquire));
        service
            .bind_and_consume_for_signing(&authority, &context("presentation_1"), || {
                evaluated.store(true, Ordering::Release);
                CanonicalCredentialPresentationBundleDigest::from_sha256([2; 32])
            })
            .expect("consume exact bundle");
        assert!(evaluated.load(Ordering::Acquire));
        assert_eq!(
            service.bind_and_consume_for_signing(&authority, &context("presentation_1"), || {
                CanonicalCredentialPresentationBundleDigest::from_sha256([2; 32])
            }),
            Err(CredentialPresentationFlowError::AlreadyConsumed)
        );
    }
}

impl CredentialPresentationAuthorityPort for CredentialPresentationFlowService {
    fn mint(
        &self,
        context: AcceptedCredentialPresentationContext,
    ) -> Result<AcceptedCredentialPresentationFlow, CredentialPresentationFlowError> {
        let generation = self.current_generation()?;
        let issued_at = self.clock.now()?;
        let expires_at = UnixTimestampMillis::new(
            issued_at
                .value()
                .checked_add(Self::MAX_TTL_MILLIS)
                .ok_or(CredentialPresentationFlowError::Unavailable)?,
        );
        Ok(self.issuer.mint(context, issued_at, expires_at, generation))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialPresentationAuthority;

impl CredentialPresentationAuthorityPort for UnavailableCredentialPresentationAuthority {
    fn mint(
        &self,
        _: AcceptedCredentialPresentationContext,
    ) -> Result<AcceptedCredentialPresentationFlow, CredentialPresentationFlowError> {
        Err(CredentialPresentationFlowError::Unavailable)
    }
}

impl CredentialPresentationFlowService {
    pub(crate) fn bind_and_consume_for_signing(
        &self,
        capability: &AcceptedCredentialPresentationFlow,
        expected: &AcceptedCredentialPresentationContext,
        digest: impl FnOnce() -> CanonicalCredentialPresentationBundleDigest,
    ) -> Result<(), CredentialPresentationFlowError> {
        if !self.issuer.owns(capability) {
            return Err(CredentialPresentationFlowError::ForeignIssuer);
        }
        if self.current_generation()? != capability.generation() {
            return Err(CredentialPresentationFlowError::GenerationMismatch);
        }
        let now = self.clock.now()?;
        if now < capability.issued_at() {
            return Err(CredentialPresentationFlowError::ClockWentBackwards);
        }
        if now >= capability.expires_at() {
            return Err(CredentialPresentationFlowError::Expired);
        }
        if !capability.binding_matches(expected) {
            return Err(CredentialPresentationFlowError::FlowMismatch);
        }
        self.issuer
            .try_consume(capability)
            .map_err(|error| match error {
                AcceptedFlowTokenUseError::ForeignIssuer => {
                    CredentialPresentationFlowError::ForeignIssuer
                }
                AcceptedFlowTokenUseError::AlreadyConsumed => {
                    CredentialPresentationFlowError::AlreadyConsumed
                }
            })?;
        let _bound_bundle_digest = digest();
        Ok(())
    }

    fn current_generation(&self) -> Result<u64, CredentialPresentationFlowError> {
        self.generation
            .lock()
            .map_err(|_| CredentialPresentationFlowError::Unavailable)?
            .ok_or(CredentialPresentationFlowError::Unavailable)
    }
}
