// SPDX-License-Identifier: Apache-2.0

//! Process-local approval authority, independent of signing/deletion consumers.
//! Canonical digest derivation is a trusted-caller precondition, not validation
//! performed by this module. See `docs/wallet-approval-capabilities.md`.

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
use oxid_platform_ports::ClockPort;
use oxid_wallet_domain::{WalletDustRegistrationPreview, WalletProfileId, WalletTransferPreview};

mod movement;
pub use movement::*;

/// SHA-256 of the operation-specific canonical payload, including its key
/// reference and every command field affecting the operation. Trusted callers
/// must derive this independently at approval and consumption; incoming callers
/// must never supply an asserted digest in lieu of their concrete command.
/// Constructing this value conveys no authority and does not validate the hash.
#[derive(Clone, PartialEq, Eq)]
pub struct CanonicalApprovalDigest([u8; 32]);

impl CanonicalApprovalDigest {
    #[must_use]
    pub const fn from_sha256(value: [u8; 32]) -> Self {
        Self(value)
    }
}

/// Closed application-owned operations. No caller-provided approval prose.
#[derive(Clone, PartialEq, Eq)]
pub enum WalletApprovalIntent {
    SignData {
        profile: WalletProfileId,
        digest: CanonicalApprovalDigest,
    },
    DeleteKey {
        profile: WalletProfileId,
        digest: CanonicalApprovalDigest,
    },
    AuthorizeTransfer {
        profile: WalletProfileId,
        preview: Box<WalletTransferPreview>,
    },
    SubmitTransfer {
        profile: WalletProfileId,
        preview: Box<WalletTransferPreview>,
    },
    AuthorizeDustRegistration {
        profile: WalletProfileId,
        preview: Box<WalletDustRegistrationPreview>,
    },
    SubmitDustRegistration {
        profile: WalletProfileId,
        preview: Box<WalletDustRegistrationPreview>,
    },
}

mod sealed {
    pub trait Operation {}
}

/// Closed type-level operation set; downstream crates cannot add operations.
pub trait WalletApprovalOperation: sealed::Operation + Send + Sync {}

/// Approval for a direct signing operation only.
pub enum SignDataApproval {}
/// Approval for key deletion only.
pub enum DeleteKeyApproval {}
impl sealed::Operation for SignDataApproval {}
impl sealed::Operation for DeleteKeyApproval {}
impl WalletApprovalOperation for SignDataApproval {}
impl WalletApprovalOperation for DeleteKeyApproval {}

/// An intent is a request for approval, never evidence that approval occurred.
pub struct WalletApprovalRequest<O: WalletApprovalOperation> {
    intent: WalletApprovalIntent,
    operation: PhantomData<O>,
}

impl WalletApprovalRequest<SignDataApproval> {
    #[must_use]
    pub const fn sign_data(profile: WalletProfileId, digest: CanonicalApprovalDigest) -> Self {
        Self {
            intent: WalletApprovalIntent::SignData { profile, digest },
            operation: PhantomData,
        }
    }
}

impl WalletApprovalRequest<DeleteKeyApproval> {
    #[must_use]
    pub const fn delete_key(profile: WalletProfileId, digest: CanonicalApprovalDigest) -> Self {
        Self {
            intent: WalletApprovalIntent::DeleteKey { profile, digest },
            operation: PhantomData,
        }
    }
}

/// Trusted composition-only producer boundary. An implementation must obtain
/// explicit approval for this exact intent from a trusted surface. Merely
/// receiving a request or an incoming caller's boolean is not approval.
/// No approving implementation is provided in production by this crate.
pub trait TrustedWalletApprovalPort: Send + Sync {
    fn approve(&self, intent: &WalletApprovalIntent) -> Result<(), TrustedWalletApprovalError>;
}

/// Payload-free outcomes a trusted approval surface may report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrustedWalletApprovalError {
    Denied,
    Unavailable,
}

impl From<TrustedWalletApprovalError> for WalletApprovalError {
    fn from(value: TrustedWalletApprovalError) -> Self {
        match value {
            TrustedWalletApprovalError::Denied => Self::Denied,
            TrustedWalletApprovalError::Unavailable => Self::Unavailable,
        }
    }
}

struct UnavailableApproval;
impl TrustedWalletApprovalPort for UnavailableApproval {
    fn approve(&self, _: &WalletApprovalIntent) -> Result<(), TrustedWalletApprovalError> {
        Err(TrustedWalletApprovalError::Unavailable)
    }
}

/// Opaque, non-serializable, non-cloneable authority. Sharing a reference does
/// not duplicate authority: all consumers use the same atomic single-use bit.
/// No public constructor, token accessor, or cross-operation conversion exists.
///
/// ```compile_fail
/// use oxid_wallet_application::{WalletApprovalCapability, WalletApprovalRequest,
///     WalletApprovalService, SignDataApproval, DeleteKeyApproval};
/// fn substitute(service: &WalletApprovalService,
///     cap: &WalletApprovalCapability<SignDataApproval>,
///     request: &WalletApprovalRequest<DeleteKeyApproval>) {
///     service.consume(cap, request);
/// }
/// ```
///
/// ```compile_fail,E0277
/// use oxid_wallet_application::{WalletApprovalCapability, SignDataApproval};
/// fn needs_clone<T: Clone>() {}
/// needs_clone::<WalletApprovalCapability<SignDataApproval>>();
/// ```
pub struct WalletApprovalCapability<O: WalletApprovalOperation> {
    intent: WalletApprovalIntent,
    issuer: Arc<()>,
    issued_at: UnixTimestampMillis,
    expires_at: UnixTimestampMillis,
    generation: u64,
    consumed: AtomicBool,
    operation: PhantomData<O>,
}

/// Closed reason codes; errors never contain identifiers, payload, or authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WalletApprovalError {
    Unavailable,
    Denied,
    Expired,
    ClockWentBackwards,
    GenerationMismatch,
    IntentMismatch,
    ForeignCapability,
    AlreadyConsumed,
}

impl fmt::Display for WalletApprovalError {
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
impl Error for WalletApprovalError {}

impl fmt::Debug for CanonicalApprovalDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CanonicalApprovalDigest([REDACTED])")
    }
}
impl fmt::Debug for WalletApprovalIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WalletApprovalIntent([REDACTED])")
    }
}
impl<O: WalletApprovalOperation> fmt::Debug for WalletApprovalRequest<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WalletApprovalRequest([REDACTED])")
    }
}
impl<O: WalletApprovalOperation> fmt::Debug for WalletApprovalCapability<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WalletApprovalCapability([REDACTED])")
    }
}

/// Application-owned mint/consume boundary. Share this one service within a
/// composition; replacing it invalidates old capabilities through issuer identity.
/// Generation transitions serialize with minting and consumption, but the
/// trusted prompt runs outside the lock so invalidation never waits for a user.
pub struct WalletApprovalService {
    port: Arc<dyn TrustedWalletApprovalPort>,
    clock: Arc<dyn ClockPort>,
    issuer: Arc<()>,
    generation: Mutex<Option<u64>>,
}

impl WalletApprovalService {
    /// Fixed upper bound, including time spent waiting for the trusted prompt.
    pub const MAX_TTL_MILLIS: u64 = 120_000;

    /// Default/untrusted composition cannot approve any operation.
    #[must_use]
    pub fn new(clock: Arc<dyn ClockPort>) -> Self {
        Self::with_trusted_port(clock, Arc::new(UnavailableApproval))
    }

    /// Only a trusted composition root may inject an approving port. This is
    /// dependency injection, not a runtime switch exposed to incoming adapters.
    #[must_use]
    pub fn with_trusted_port(
        clock: Arc<dyn ClockPort>,
        port: Arc<dyn TrustedWalletApprovalPort>,
    ) -> Self {
        Self {
            port,
            clock,
            issuer: Arc::new(()),
            generation: Mutex::new(Some(0)),
        }
    }

    pub fn request<O: WalletApprovalOperation>(
        &self,
        request: &WalletApprovalRequest<O>,
    ) -> Result<WalletApprovalCapability<O>, WalletApprovalError> {
        let generation = self.current_generation()?;
        let issued_at = self.now()?;
        let expires_at = UnixTimestampMillis::new(
            issued_at
                .value()
                .checked_add(Self::MAX_TTL_MILLIS)
                .ok_or(WalletApprovalError::Unavailable)?,
        );
        let expires_at = match &request.intent {
            WalletApprovalIntent::AuthorizeTransfer { preview, .. }
            | WalletApprovalIntent::SubmitTransfer { preview, .. } => {
                expires_at.min(preview.expires_at())
            }
            WalletApprovalIntent::AuthorizeDustRegistration { preview, .. }
            | WalletApprovalIntent::SubmitDustRegistration { preview, .. } => {
                expires_at.min(preview.expires_at())
            }
            WalletApprovalIntent::SignData { .. } | WalletApprovalIntent::DeleteKey { .. } => {
                expires_at
            }
        };
        if issued_at >= expires_at {
            return Err(WalletApprovalError::Expired);
        }
        self.port.approve(&request.intent)?;
        let current = self
            .generation
            .lock()
            .map_err(|_| WalletApprovalError::Unavailable)?;
        if current.ok_or(WalletApprovalError::Unavailable)? != generation {
            return Err(WalletApprovalError::GenerationMismatch);
        }
        let now = self.now()?;
        if now < issued_at {
            return Err(WalletApprovalError::ClockWentBackwards);
        }
        if now >= expires_at {
            return Err(WalletApprovalError::Expired);
        }
        Ok(WalletApprovalCapability {
            intent: request.intent.clone(),
            issuer: Arc::clone(&self.issuer),
            issued_at,
            expires_at,
            generation,
            consumed: AtomicBool::new(false),
            operation: PhantomData,
        })
    }

    /// Compare independently reconstructed expected intent, then spend once.
    /// A mismatch conveys no authority and does not consume a valid approval.
    /// A successful consumption is terminal even if the subsequent protected
    /// operation fails; retry requires fresh approval, never restoring authority.
    pub fn consume<O: WalletApprovalOperation>(
        &self,
        capability: &WalletApprovalCapability<O>,
        expected: &WalletApprovalRequest<O>,
    ) -> Result<(), WalletApprovalError> {
        if !Arc::ptr_eq(&self.issuer, &capability.issuer) {
            return Err(WalletApprovalError::ForeignCapability);
        }
        let current = self
            .generation
            .lock()
            .map_err(|_| WalletApprovalError::Unavailable)?;
        if current.ok_or(WalletApprovalError::Unavailable)? != capability.generation {
            return Err(WalletApprovalError::GenerationMismatch);
        }
        let now = self.now()?;
        if now < capability.issued_at {
            return Err(WalletApprovalError::ClockWentBackwards);
        }
        if now >= capability.expires_at {
            return Err(WalletApprovalError::Expired);
        }
        if capability.intent != expected.intent {
            return Err(WalletApprovalError::IntentMismatch);
        }
        capability
            .consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| WalletApprovalError::AlreadyConsumed)?;
        Ok(())
    }

    /// Invalidate all issued and in-flight approvals on lock/profile/lifecycle
    /// transitions. Generation exhaustion permanently disables this composition.
    pub fn invalidate(&self) -> Result<(), WalletApprovalError> {
        let mut current = self
            .generation
            .lock()
            .map_err(|_| WalletApprovalError::Unavailable)?;
        *current = current.and_then(|value| value.checked_add(1));
        current.map(|_| ()).ok_or(WalletApprovalError::Unavailable)
    }

    fn current_generation(&self) -> Result<u64, WalletApprovalError> {
        self.generation
            .lock()
            .map_err(|_| WalletApprovalError::Unavailable)?
            .ok_or(WalletApprovalError::Unavailable)
    }

    fn now(&self) -> Result<UnixTimestampMillis, WalletApprovalError> {
        self.clock
            .now()
            .map_err(|_| WalletApprovalError::Unavailable)
    }
}

#[cfg(test)]
mod movement_tests;
#[cfg(test)]
pub(crate) mod tests;
