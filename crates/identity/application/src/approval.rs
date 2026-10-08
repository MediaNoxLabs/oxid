// SPDX-License-Identifier: Apache-2.0

//! Process-local sealed approval authority for protected DID operations.
//! Consumers independently derive the canonical digest from the concrete command;
//! an asserted digest alone never grants approval.

use std::{
    error::Error,
    fmt,
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use oxid_foundation::UnixTimestampMillis;
use oxid_identity_domain::{IdentityProfileId, MidnightDid};

/// Narrow time boundary owned by the identity application. Outer composition
/// adapts its platform clock without pulling platform concerns into this hexagon.
pub trait DidApprovalClockPort: Send + Sync {
    fn now(&self) -> Result<UnixTimestampMillis, DidApprovalClockError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DidApprovalClockError {
    Unavailable,
}

/// SHA-256 of an operation's canonical payload. Construction conveys no authority.
#[derive(Clone, PartialEq, Eq)]
pub struct CanonicalDidApprovalDigest([u8; 32]);

impl CanonicalDidApprovalDigest {
    #[must_use]
    pub const fn from_sha256(value: [u8; 32]) -> Self {
        Self(value)
    }
}

/// Closed canonical DID operation intents. Each includes all identity and payload
/// fields that a protected consumer must independently reconstruct.
#[derive(Clone, PartialEq, Eq)]
pub enum DidApprovalIntent {
    Update {
        profile: IdentityProfileId,
        did: MidnightDid,
        digest: CanonicalDidApprovalDigest,
    },
    Deactivate {
        profile: IdentityProfileId,
        did: MidnightDid,
        digest: CanonicalDidApprovalDigest,
    },
    Sign {
        profile: IdentityProfileId,
        did: MidnightDid,
        method_id: String,
        digest: CanonicalDidApprovalDigest,
    },
}

mod sealed {
    pub trait Operation {}
}

/// Closed type-level DID operation set; downstream crates cannot add operations.
pub trait DidApprovalOperation: sealed::Operation + Send + Sync {}

/// Approval for one DID update operation.
pub enum UpdateDidApproval {}
/// Approval for one DID deactivation operation.
pub enum DeactivateDidApproval {}
/// Approval for one DID signing operation.
pub enum SignDidApproval {}
impl sealed::Operation for UpdateDidApproval {}
impl sealed::Operation for DeactivateDidApproval {}
impl sealed::Operation for SignDidApproval {}
impl DidApprovalOperation for UpdateDidApproval {}
impl DidApprovalOperation for DeactivateDidApproval {}
impl DidApprovalOperation for SignDidApproval {}

/// A request describes an exact operation but is not approval evidence.
pub struct DidApprovalRequest<O: DidApprovalOperation> {
    intent: DidApprovalIntent,
    operation: PhantomData<O>,
}

impl DidApprovalRequest<UpdateDidApproval> {
    #[must_use]
    pub const fn update(
        profile: IdentityProfileId,
        did: MidnightDid,
        digest: CanonicalDidApprovalDigest,
    ) -> Self {
        Self {
            intent: DidApprovalIntent::Update {
                profile,
                did,
                digest,
            },
            operation: PhantomData,
        }
    }
}

impl DidApprovalRequest<DeactivateDidApproval> {
    #[must_use]
    pub const fn deactivate(
        profile: IdentityProfileId,
        did: MidnightDid,
        digest: CanonicalDidApprovalDigest,
    ) -> Self {
        Self {
            intent: DidApprovalIntent::Deactivate {
                profile,
                did,
                digest,
            },
            operation: PhantomData,
        }
    }
}

impl DidApprovalRequest<SignDidApproval> {
    #[must_use]
    pub fn sign(
        profile: IdentityProfileId,
        did: MidnightDid,
        method_id: impl Into<String>,
        digest: CanonicalDidApprovalDigest,
    ) -> Self {
        Self {
            intent: DidApprovalIntent::Sign {
                profile,
                did,
                method_id: method_id.into(),
                digest,
            },
            operation: PhantomData,
        }
    }
}

/// Trusted composition-only producer boundary. This crate provides no production
/// implementation that approves an intent.
pub trait TrustedDidApprovalPort: Send + Sync {
    fn approve(&self, intent: &DidApprovalIntent) -> Result<(), TrustedDidApprovalError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrustedDidApprovalError {
    Denied,
    Unavailable,
}

struct UnavailableApproval;
impl TrustedDidApprovalPort for UnavailableApproval {
    fn approve(&self, _: &DidApprovalIntent) -> Result<(), TrustedDidApprovalError> {
        Err(TrustedDidApprovalError::Unavailable)
    }
}

/// Opaque, non-cloneable, non-serializable, single-use authority.
///
/// Operation types cannot be substituted by downstream callers:
///
/// ```compile_fail
/// use oxid_identity_application::{
///     DeactivateDidApproval, DidApprovalCapability, DidApprovalRequest,
///     DidApprovalService, UpdateDidApproval,
/// };
/// fn substitute(
///     service: &DidApprovalService,
///     capability: &DidApprovalCapability<UpdateDidApproval>,
///     request: &DidApprovalRequest<DeactivateDidApproval>,
/// ) {
///     service.consume(capability, request);
/// }
/// ```
///
/// Capabilities cannot be duplicated:
///
/// ```compile_fail,E0277
/// use oxid_identity_application::{DidApprovalCapability, SignDidApproval};
/// fn needs_clone<T: Clone>() {}
/// needs_clone::<DidApprovalCapability<SignDidApproval>>();
/// ```
pub struct DidApprovalCapability<O: DidApprovalOperation> {
    intent: DidApprovalIntent,
    issuer: Arc<()>,
    issued_at: UnixTimestampMillis,
    expires_at: UnixTimestampMillis,
    generation: u64,
    consumed: AtomicBool,
    operation: PhantomData<O>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DidApprovalError {
    Unavailable,
    Denied,
    Expired,
    ClockWentBackwards,
    GenerationMismatch,
    IntentMismatch,
    ForeignCapability,
    AlreadyConsumed,
}

impl From<TrustedDidApprovalError> for DidApprovalError {
    fn from(value: TrustedDidApprovalError) -> Self {
        match value {
            TrustedDidApprovalError::Denied => Self::Denied,
            TrustedDidApprovalError::Unavailable => Self::Unavailable,
        }
    }
}

impl fmt::Display for DidApprovalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "approval_unavailable",
            Self::Denied => "approval_denied",
            Self::Expired => "approval_expired",
            Self::ClockWentBackwards => "approval_clock_went_backwards",
            Self::GenerationMismatch => "approval_generation_mismatch",
            Self::IntentMismatch => "approval_intent_mismatch",
            Self::ForeignCapability => "approval_foreign_capability",
            Self::AlreadyConsumed => "approval_already_consumed",
        })
    }
}
impl Error for DidApprovalError {}

impl fmt::Debug for CanonicalDidApprovalDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CanonicalDidApprovalDigest([REDACTED])")
    }
}
impl fmt::Debug for DidApprovalIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DidApprovalIntent([REDACTED])")
    }
}
impl<O: DidApprovalOperation> fmt::Debug for DidApprovalRequest<O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DidApprovalRequest([REDACTED])")
    }
}
impl<O: DidApprovalOperation> fmt::Debug for DidApprovalCapability<O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DidApprovalCapability([REDACTED])")
    }
}

/// Application-owned mint/consume boundary. Recomposition uses a distinct issuer;
/// protection changes invalidate every capability by advancing its generation.
pub struct DidApprovalService {
    port: Arc<dyn TrustedDidApprovalPort>,
    clock: Arc<dyn DidApprovalClockPort>,
    issuer: Arc<()>,
    generation: Mutex<Option<u64>>,
}

impl DidApprovalService {
    pub const MAX_TTL_MILLIS: u64 = 120_000;

    #[must_use]
    pub fn new(clock: Arc<dyn DidApprovalClockPort>) -> Self {
        Self::with_trusted_did_port(clock, Arc::new(UnavailableApproval))
    }

    #[must_use]
    pub fn with_trusted_did_port(
        clock: Arc<dyn DidApprovalClockPort>,
        port: Arc<dyn TrustedDidApprovalPort>,
    ) -> Self {
        Self {
            port,
            clock,
            issuer: Arc::new(()),
            generation: Mutex::new(Some(0)),
        }
    }

    pub fn request<O: DidApprovalOperation>(
        &self,
        request: &DidApprovalRequest<O>,
    ) -> Result<DidApprovalCapability<O>, DidApprovalError> {
        let generation = self.current_generation()?;
        let issued_at = self.now()?;
        let expires_at = UnixTimestampMillis::new(
            issued_at
                .value()
                .checked_add(Self::MAX_TTL_MILLIS)
                .ok_or(DidApprovalError::Unavailable)?,
        );
        self.port.approve(&request.intent)?;
        let current = self
            .generation
            .lock()
            .map_err(|_| DidApprovalError::Unavailable)?;
        if current.ok_or(DidApprovalError::Unavailable)? != generation {
            return Err(DidApprovalError::GenerationMismatch);
        }
        let now = self.now()?;
        if now < issued_at {
            return Err(DidApprovalError::ClockWentBackwards);
        }
        if now >= expires_at {
            return Err(DidApprovalError::Expired);
        }
        Ok(DidApprovalCapability {
            intent: request.intent.clone(),
            issuer: Arc::clone(&self.issuer),
            issued_at,
            expires_at,
            generation,
            consumed: AtomicBool::new(false),
            operation: PhantomData,
        })
    }

    /// Compare a reconstructed intent, then atomically spend the capability.
    pub fn consume<O: DidApprovalOperation>(
        &self,
        capability: &DidApprovalCapability<O>,
        expected: &DidApprovalRequest<O>,
    ) -> Result<(), DidApprovalError> {
        if !Arc::ptr_eq(&self.issuer, &capability.issuer) {
            return Err(DidApprovalError::ForeignCapability);
        }
        let current = self
            .generation
            .lock()
            .map_err(|_| DidApprovalError::Unavailable)?;
        if current.ok_or(DidApprovalError::Unavailable)? != capability.generation {
            return Err(DidApprovalError::GenerationMismatch);
        }
        let now = self.now()?;
        if now < capability.issued_at {
            return Err(DidApprovalError::ClockWentBackwards);
        }
        if now >= capability.expires_at {
            return Err(DidApprovalError::Expired);
        }
        if capability.intent != expected.intent {
            return Err(DidApprovalError::IntentMismatch);
        }
        capability
            .consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| DidApprovalError::AlreadyConsumed)?;
        Ok(())
    }

    pub fn invalidate(&self) -> Result<(), DidApprovalError> {
        let mut current = self
            .generation
            .lock()
            .map_err(|_| DidApprovalError::Unavailable)?;
        *current = current.and_then(|value| value.checked_add(1));
        current.map(|_| ()).ok_or(DidApprovalError::Unavailable)
    }

    fn current_generation(&self) -> Result<u64, DidApprovalError> {
        self.generation
            .lock()
            .map_err(|_| DidApprovalError::Unavailable)?
            .ok_or(DidApprovalError::Unavailable)
    }

    fn now(&self) -> Result<UnixTimestampMillis, DidApprovalError> {
        self.clock.now().map_err(|_| DidApprovalError::Unavailable)
    }
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(any(test, feature = "development-approval"))]
mod development;
#[cfg(any(test, feature = "development-approval"))]
pub use development::development_did_approvals;
