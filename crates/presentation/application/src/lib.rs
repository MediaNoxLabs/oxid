// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use oxid_foundation::{AcceptedCredentialPresentationFlow, OpaqueIdError};
use oxid_presentation_domain::{
    CredentialPresentationId, CredentialPresentationPreview, CredentialPresentationState,
    PresentationCredentialCandidate, PresentationProfileId, RequestedPresentationClaim,
};

pub const MAX_PRESENTATION_REQUEST_BYTES: usize = 64 * 1_024;
const MAX_CREDENTIAL_IDENTIFIER_CHARACTERS: usize = 256;
pub const OPENID4VP_CREDENTIAL_PRESENTATION_FLOW_ID: &str = "openid4vp";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialPresentationApprovalError {
    Unavailable,
}

impl fmt::Display for CredentialPresentationApprovalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("approval_unavailable")
    }
}

impl Error for CredentialPresentationApprovalError {}

#[derive(PartialEq, Eq)]
pub struct CredentialPresentationAuthorityRequest {
    pub profile_id: String,
    pub flow_id: &'static str,
    pub session_id: String,
    pub credential_id: String,
}

pub trait CredentialPresentationAuthorityPort: Send + Sync {
    fn mint(
        &self,
        request: CredentialPresentationAuthorityRequest,
    ) -> Result<AcceptedCredentialPresentationFlow, CredentialPresentationApprovalError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialPresentationAuthority;

impl CredentialPresentationAuthorityPort for UnavailableCredentialPresentationAuthority {
    fn mint(
        &self,
        _: CredentialPresentationAuthorityRequest,
    ) -> Result<AcceptedCredentialPresentationFlow, CredentialPresentationApprovalError> {
        Err(CredentialPresentationApprovalError::Unavailable)
    }
}

pub type PreparePresentationPortFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PreparedCredentialPresentation, PresentationProtocolError>>
            + Send
            + 'a,
    >,
>;
pub type PresentCredentialPortFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PresentationProtocolOutcome, PresentationProtocolError>>
            + Send
            + 'a,
    >,
>;
pub type PresentationViewFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<CredentialPresentationView, CredentialPresentationError>>
            + Send
            + 'a,
    >,
>;
pub type FindPresentationCandidatesFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<Vec<PresentationCredentialCandidate>, PresentationCandidateError>,
            > + Send
            + 'a,
    >,
>;
pub type CreatePresentationProofFuture<'a> = Pin<
    Box<dyn Future<Output = Result<PresentationProofArtifact, PresentationProofError>> + Send + 'a>,
>;
pub type AuthorizePresentationHolderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), PresentationHolderAuthorizationError>> + Send + 'a>>;
pub type VerifyPresentationProofFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), PresentationVerificationError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareCredentialPresentationRequest {
    pub profile_id: PresentationProfileId,
    pub request: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCredentialPresentation {
    pub id: CredentialPresentationId,
    pub preview: CredentialPresentationPreview,
}

pub struct ProtocolPresentCredentialRequest {
    pub profile_id: PresentationProfileId,
    pub presentation_id: CredentialPresentationId,
    pub credential_id: String,
    pub authority: AcceptedCredentialPresentationFlow,
}

impl fmt::Debug for ProtocolPresentCredentialRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProtocolPresentCredentialRequest")
            .field("profile_id", &self.profile_id)
            .field("presentation_id", &self.presentation_id)
            .field("credential_id", &self.credential_id)
            .field("authority", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationProtocolOutcome {
    pub verifier_validated: bool,
}

pub trait CredentialPresentationProtocolPort: Send + Sync {
    fn prepare<'a>(
        &'a self,
        request: PrepareCredentialPresentationRequest,
    ) -> PreparePresentationPortFuture<'a>;

    fn present<'a>(
        &'a self,
        request: ProtocolPresentCredentialRequest,
    ) -> PresentCredentialPortFuture<'a>;

    fn discard(
        &self,
        presentation_id: &CredentialPresentationId,
    ) -> Result<(), PresentationProtocolError>;

    fn cancel(
        &self,
        _request: CancelPresentationProofRequest,
    ) -> Result<(), PresentationProtocolError> {
        Err(PresentationProtocolError::ProofUnavailable)
    }

    fn set_foreground(&self, _foreground: bool) -> Result<(), PresentationProtocolError> {
        Err(PresentationProtocolError::ProofUnavailable)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationCandidateQuery {
    pub profile_id: PresentationProfileId,
    pub schema_id: String,
    pub requested_claims: Vec<RequestedPresentationClaim>,
}

pub trait PresentationCandidateSourcePort: Send + Sync {
    fn find<'a>(
        &'a self,
        query: PresentationCandidateQuery,
    ) -> FindPresentationCandidatesFuture<'a>;
}

pub struct PresentationProofRequest {
    pub profile_id: PresentationProfileId,
    pub presentation_id: CredentialPresentationId,
    pub credential_id: String,
    pub verifier: String,
    pub challenge_hash: [u8; 32],
    pub verifier_domain_hash: [u8; 32],
    pub requested_claims: Vec<RequestedPresentationClaim>,
    pub authority: AcceptedCredentialPresentationFlow,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PresentationProofArtifact(Vec<u8>);

impl PresentationProofArtifact {
    pub fn new(bytes: Vec<u8>) -> Result<Self, PresentationProofError> {
        if bytes.is_empty() || bytes.len() > 4 * 1_024 * 1_024 {
            return Err(PresentationProofError::Rejected);
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for PresentationProofArtifact {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PresentationProofArtifact")
            .field("length", &self.0.len())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for PresentationProofRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PresentationProofRequest")
            .field("profile_id", &self.profile_id)
            .field("presentation_id", &self.presentation_id)
            .field("credential_id", &self.credential_id)
            .field("verifier", &self.verifier)
            .field("requested_claim_count", &self.requested_claims.len())
            .field("authority", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

pub trait PresentationProofPort: Send + Sync {
    fn create<'a>(&'a self, request: PresentationProofRequest)
    -> CreatePresentationProofFuture<'a>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancelPresentationProofRequest {
    pub profile_id: PresentationProfileId,
    pub presentation_id: CredentialPresentationId,
}

/// Non-blocking control boundary for a proof worker.
///
/// Cancellation is a request, not an acknowledgement. The proof future must
/// resolve only after the worker has stopped using witness and custody
/// material and has discarded any late result.
pub trait PresentationProofControlPort: Send + Sync {
    fn cancel(&self, request: CancelPresentationProofRequest)
    -> Result<(), PresentationProofError>;

    fn set_foreground(&self, foreground: bool) -> Result<(), PresentationProofError>;

    /// Releases admission after independent verification and returns any
    /// cancellation/background/timeout reason that arrived after proving.
    fn finish(&self, request: CancelPresentationProofRequest)
    -> Result<(), PresentationProofError>;
}

/// Current-control check for the holder method named by a credential.
///
/// This is deliberately separate from [`PresentationProofPort`]. A successful
/// authorization proves only that the current protected DID key approved the
/// consented presentation statement; it is not a credential-family proof and
/// must never be serialized as a `vp_token`.
#[derive(Clone, PartialEq, Eq)]
pub struct PresentationHolderAuthorizationRequest {
    pub profile_id: PresentationProfileId,
    pub holder_did: String,
    pub holder_method_id: String,
    pub verifier: String,
    pub presentation_statement: [u8; 32],
}

/// Accepted-flow request for the closed generic-authorization plus Jubjub
/// holder-proof signature bundle. The legacy request remains unchanged for
/// Passport Vault's separately tracked authorization path.
pub struct AcceptedPresentationHolderAuthorizationRequest {
    pub request: PresentationHolderAuthorizationRequest,
    pub presentation_id: CredentialPresentationId,
    pub credential_id: String,
    pub presentation_root: [u8; 32],
    pub verifier_challenge_hash: [u8; 32],
    pub created_at_seconds: u64,
    pub authority: AcceptedCredentialPresentationFlow,
}

impl fmt::Debug for AcceptedPresentationHolderAuthorizationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcceptedPresentationHolderAuthorizationRequest")
            .field("request", &self.request)
            .field("presentation_id", &self.presentation_id)
            .field("credential_id", &self.credential_id)
            .field("created_at_seconds", &self.created_at_seconds)
            .field("authority", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

pub type AuthorizeAcceptedPresentationHolderFuture<'a> = Pin<
    Box<dyn Future<Output = Result<Vec<u8>, PresentationHolderAuthorizationError>> + Send + 'a>,
>;

pub trait AcceptedPresentationHolderAuthorizationPort: Send + Sync {
    fn authorize_accepted<'a>(
        &'a self,
        request: AcceptedPresentationHolderAuthorizationRequest,
    ) -> AuthorizeAcceptedPresentationHolderFuture<'a>;
}

impl fmt::Debug for PresentationHolderAuthorizationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PresentationHolderAuthorizationRequest")
            .field("profile_id", &self.profile_id)
            .field("holder_did", &self.holder_did)
            .field("holder_method_id", &self.holder_method_id)
            .field("verifier", &self.verifier)
            .finish_non_exhaustive()
    }
}

pub trait PresentationHolderAuthorizationPort: Send + Sync {
    fn authorize<'a>(
        &'a self,
        request: PresentationHolderAuthorizationRequest,
    ) -> AuthorizePresentationHolderFuture<'a>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationHolderAuthorizationError {
    Unavailable,
    InvalidBinding,
    NotManaged,
    Locked,
    Rejected,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PresentationVerificationRequest {
    pub profile_id: PresentationProfileId,
    pub credential_id: String,
    pub verifier: String,
    pub challenge_hash: [u8; 32],
    pub verifier_domain_hash: [u8; 32],
    pub requested_claims: Vec<RequestedPresentationClaim>,
    pub proof: PresentationProofArtifact,
}

impl fmt::Debug for PresentationVerificationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PresentationVerificationRequest")
            .field("profile_id", &self.profile_id)
            .field("credential_id", &self.credential_id)
            .field("verifier", &self.verifier)
            .field("requested_claim_count", &self.requested_claims.len())
            .field("proof_length", &self.proof.as_bytes().len())
            .finish_non_exhaustive()
    }
}

pub trait PresentationVerifierPort: Send + Sync {
    fn verify<'a>(
        &'a self,
        request: PresentationVerificationRequest,
    ) -> VerifyPresentationProofFuture<'a>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationProtocolError {
    Unavailable,
    InvalidRequest,
    UnsupportedRequest,
    InvalidVerifier,
    RequestExpired,
    NoCandidate,
    HolderAuthorizationUnavailable,
    HolderNotAuthorized,
    ProofUnavailable,
    ProofBusy,
    ProofCancelled,
    ProofBackgrounded,
    ProofTimedOut,
    InvalidProof,
    VerifierRejected,
}

impl PresentationProtocolError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "protocol_unavailable",
            Self::InvalidRequest => "invalid_request",
            Self::UnsupportedRequest => "unsupported_request",
            Self::InvalidVerifier => "invalid_verifier",
            Self::RequestExpired => "request_expired",
            Self::NoCandidate => "no_candidate",
            Self::HolderAuthorizationUnavailable => "holder_authorization_unavailable",
            Self::HolderNotAuthorized => "holder_not_authorized",
            Self::ProofUnavailable => "proof_unavailable",
            Self::ProofBusy => "proof_busy",
            Self::ProofCancelled => "proof_cancelled",
            Self::ProofBackgrounded => "proof_backgrounded",
            Self::ProofTimedOut => "proof_timed_out",
            Self::InvalidProof => "invalid_proof",
            Self::VerifierRejected => "verifier_rejected",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationCandidateError {
    Unavailable,
    InvalidQuery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationProofError {
    Unavailable,
    Busy,
    Cancelled,
    Backgrounded,
    TimedOut,
    InvalidCredential,
    InvalidSelection,
    HolderAuthorizationUnavailable,
    HolderNotAuthorized,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationVerificationError {
    Unavailable,
    InvalidProof,
    Rejected,
}

macro_rules! display_code_error {
    ($type:ty, $body:expr) => {
        impl fmt::Display for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str($body(*self))
            }
        }
        impl Error for $type {}
    };
}

display_code_error!(PresentationProtocolError, PresentationProtocolError::code);
display_code_error!(PresentationCandidateError, |error| match error {
    PresentationCandidateError::Unavailable => "presentation candidates are unavailable",
    PresentationCandidateError::InvalidQuery => "presentation candidate query is invalid",
});
display_code_error!(PresentationProofError, |error| match error {
    PresentationProofError::Unavailable => "presentation proof capability is unavailable",
    PresentationProofError::Busy => "another presentation proof is already running",
    PresentationProofError::Cancelled => "presentation proof was cancelled",
    PresentationProofError::Backgrounded => "presentation proof stopped after app backgrounding",
    PresentationProofError::TimedOut => "presentation proof timed out",
    PresentationProofError::InvalidCredential => "presentation credential is invalid",
    PresentationProofError::InvalidSelection => "presentation selection is invalid",
    PresentationProofError::HolderAuthorizationUnavailable =>
        "presentation holder authorization is unavailable",
    PresentationProofError::HolderNotAuthorized => "presentation holder is not authorized",
    PresentationProofError::Rejected => "presentation proof was rejected",
});
display_code_error!(PresentationHolderAuthorizationError, |error| match error {
    PresentationHolderAuthorizationError::Unavailable =>
        "presentation holder authorization is unavailable",
    PresentationHolderAuthorizationError::InvalidBinding =>
        "presentation holder binding is invalid",
    PresentationHolderAuthorizationError::NotManaged => "presentation holder method is not managed",
    PresentationHolderAuthorizationError::Locked => "presentation holder key is locked",
    PresentationHolderAuthorizationError::Rejected =>
        "presentation holder authorization was rejected",
});
display_code_error!(PresentationVerificationError, |error| match error {
    PresentationVerificationError::Unavailable => "presentation verification is unavailable",
    PresentationVerificationError::InvalidProof => "presentation proof is invalid",
    PresentationVerificationError::Rejected => "presentation verifier rejected the proof",
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareCredentialPresentationCommand {
    pub profile_id: String,
    pub request: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptCredentialPresentationCommand {
    pub profile_id: String,
    pub presentation_id: String,
    pub credential_id: String,
    pub confirmed: bool,
    pub intent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefuseCredentialPresentationCommand {
    pub profile_id: String,
    pub presentation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancelCredentialPresentationCommand {
    pub profile_id: String,
    pub presentation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialPresentationQuery {
    pub profile_id: String,
    pub presentation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialPresentationProfileQuery {
    pub profile_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestedPresentationClaimView {
    pub claim_path: String,
    pub label: String,
    pub intent: String,
    pub predicate_kind: Option<String>,
    pub threshold: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationCredentialCandidateView {
    pub credential_id: String,
    pub display_name: String,
    pub issuer: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialPresentationView {
    pub id: String,
    pub verifier: String,
    pub purpose: String,
    pub query_id: String,
    pub candidates: Vec<PresentationCredentialCandidateView>,
    pub requested_claims: Vec<RequestedPresentationClaimView>,
    pub state: String,
    pub presentation_generated: bool,
    pub verifier_validated: bool,
    pub failure_code: Option<String>,
}

/// Maximum number of presentation activity records retained per process.
/// This privacy-safe projection is deleted on restart and is never backed up.
pub const MAX_CREDENTIAL_PRESENTATION_ACTIVITY_RECORDS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialPresentationActivityId(u64);

impl CredentialPresentationActivityId {
    #[must_use]
    pub const fn from_value(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialPresentationActivitySource {
    OpenId4Vp,
}

impl CredentialPresentationActivitySource {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenId4Vp => "openid4vp",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialPresentationActivityStatus {
    Pending,
    Shared,
    Failed,
    Refused,
    Cancelled,
    TimedOut,
    OutcomeUnknown,
}

impl CredentialPresentationActivityStatus {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Shared => "shared",
            Self::Failed => "failed",
            Self::Refused => "refused",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    const fn is_final(self) -> bool {
        matches!(
            self,
            Self::Shared | Self::Failed | Self::Refused | Self::Cancelled
        )
    }

    const fn is_evictable(self) -> bool {
        self.is_final() || matches!(self, Self::TimedOut | Self::OutcomeUnknown)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialPresentationActivityFinality {
    Pending,
    Final,
    Unknown,
}

impl CredentialPresentationActivityFinality {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Final => "final",
            Self::Unknown => "unknown",
        }
    }
}

impl From<CredentialPresentationActivityStatus> for CredentialPresentationActivityFinality {
    fn from(status: CredentialPresentationActivityStatus) -> Self {
        match status {
            CredentialPresentationActivityStatus::Pending => Self::Pending,
            CredentialPresentationActivityStatus::TimedOut
            | CredentialPresentationActivityStatus::OutcomeUnknown => Self::Unknown,
            CredentialPresentationActivityStatus::Shared
            | CredentialPresentationActivityStatus::Failed
            | CredentialPresentationActivityStatus::Refused
            | CredentialPresentationActivityStatus::Cancelled => Self::Final,
        }
    }
}

/// Privacy-safe presentation metadata. Claim names and values, selected
/// credentials, proof material, protocol payloads, keys, and raw errors are
/// intentionally absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialPresentationActivityRecord {
    pub id: CredentialPresentationActivityId,
    pub profile_id: String,
    pub source: CredentialPresentationActivitySource,
    pub purpose: String,
    pub presentation_type: String,
    pub verifier: Option<String>,
    pub status: CredentialPresentationActivityStatus,
    pub finality: CredentialPresentationActivityFinality,
    pub observed_at_millis: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialPresentationActivityView {
    pub source: String,
    pub retention: String,
    pub records: Vec<CredentialPresentationActivityRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialPresentationActivityError {
    Unavailable,
}

impl fmt::Display for CredentialPresentationActivityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("credential presentation activity is unavailable")
    }
}

impl Error for CredentialPresentationActivityError {}

pub trait ListCredentialPresentationActivityUseCase: Send + Sync {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<CredentialPresentationActivityView, CredentialPresentationActivityError>;
}

#[derive(Default)]
struct CredentialPresentationActivityState {
    next_id: u64,
    records: VecDeque<CredentialPresentationActivityRecord>,
    presentation_ids: BTreeMap<CredentialPresentationId, CredentialPresentationActivityId>,
}

/// Bounded application-owned producer and projection. It is process-local,
/// explicitly deletable, and deliberately has neither persistence nor backup.
pub struct CredentialPresentationActivityStore {
    state: Mutex<CredentialPresentationActivityState>,
}

impl Default for CredentialPresentationActivityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialPresentationActivityStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(CredentialPresentationActivityState::default()),
        }
    }

    pub fn clear_profile(
        &self,
        profile_id: &str,
    ) -> Result<usize, CredentialPresentationActivityError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CredentialPresentationActivityError::Unavailable)?;
        let removed: BTreeSet<_> = state
            .records
            .iter()
            .filter(|record| record.profile_id == profile_id)
            .map(|record| record.id)
            .collect();
        state
            .records
            .retain(|record| record.profile_id != profile_id);
        state.presentation_ids.retain(|_, id| !removed.contains(id));
        Ok(removed.len())
    }

    fn now() -> Option<u64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|value| u64::try_from(value.as_millis()).ok())
    }

    fn begin(
        &self,
        presentation_id: &CredentialPresentationId,
        session: &Session,
    ) -> Option<CredentialPresentationActivityId> {
        let mut state = self.state.lock().ok()?;
        if let Some(id) = state.presentation_ids.get(presentation_id) {
            return Some(*id);
        }
        if state.records.len() == MAX_CREDENTIAL_PRESENTATION_ACTIVITY_RECORDS {
            let evict_at = state
                .records
                .iter()
                .position(|record| record.status.is_evictable())?;
            let evicted = state.records.remove(evict_at)?;
            state.presentation_ids.retain(|_, id| *id != evicted.id);
        }
        state.next_id = state.next_id.checked_add(1)?;
        let id = CredentialPresentationActivityId(state.next_id);
        state
            .records
            .push_back(CredentialPresentationActivityRecord {
                id,
                profile_id: session.profile_id.as_str().to_owned(),
                source: CredentialPresentationActivitySource::OpenId4Vp,
                purpose: session.preview.purpose().to_owned(),
                presentation_type: session.preview.query_id().to_owned(),
                verifier: Some(session.preview.verifier().to_owned()),
                status: CredentialPresentationActivityStatus::Pending,
                finality: CredentialPresentationActivityFinality::Pending,
                observed_at_millis: Self::now(),
            });
        state.presentation_ids.insert(presentation_id.clone(), id);
        Some(id)
    }

    fn update(
        &self,
        presentation_id: &CredentialPresentationId,
        status: CredentialPresentationActivityStatus,
    ) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let Some(id) = state.presentation_ids.get(presentation_id).copied() else {
            return;
        };
        let Some(record) = state.records.iter_mut().find(|record| record.id == id) else {
            return;
        };
        if record.status == status
            || record.status.is_final()
            || matches!(status, CredentialPresentationActivityStatus::Pending)
        {
            return;
        }
        if matches!(
            record.status,
            CredentialPresentationActivityStatus::TimedOut
                | CredentialPresentationActivityStatus::OutcomeUnknown
        ) && matches!(
            status,
            CredentialPresentationActivityStatus::TimedOut
                | CredentialPresentationActivityStatus::OutcomeUnknown
                | CredentialPresentationActivityStatus::Refused
                | CredentialPresentationActivityStatus::Cancelled
        ) {
            return;
        }
        record.status = status;
        record.finality = status.into();
        record.observed_at_millis = Self::now();
    }
}

impl ListCredentialPresentationActivityUseCase for CredentialPresentationActivityStore {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<CredentialPresentationActivityView, CredentialPresentationActivityError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CredentialPresentationActivityError::Unavailable)?;
        Ok(CredentialPresentationActivityView {
            source: "application_event_projection".to_owned(),
            retention: "process_local_bounded_not_backed_up".to_owned(),
            records: state
                .records
                .iter()
                .rev()
                .filter(|record| record.profile_id == profile_id)
                .cloned()
                .collect(),
        })
    }
}

#[derive(Clone, Debug)]
struct Session {
    profile_id: PresentationProfileId,
    preview: CredentialPresentationPreview,
    state: CredentialPresentationState,
    presentation_generated: bool,
    verifier_validated: bool,
    failure_code: Option<String>,
    refusal_in_progress: bool,
    protocol_discarded: bool,
}

impl Session {
    fn view(&self, id: &CredentialPresentationId) -> CredentialPresentationView {
        CredentialPresentationView {
            id: id.as_str().to_owned(),
            verifier: self.preview.verifier().to_owned(),
            purpose: self.preview.purpose().to_owned(),
            query_id: self.preview.query_id().to_owned(),
            candidates: self
                .preview
                .candidates()
                .iter()
                .map(|candidate| PresentationCredentialCandidateView {
                    credential_id: candidate.credential_id().to_owned(),
                    display_name: candidate.display_name().to_owned(),
                    issuer: candidate.issuer().to_owned(),
                })
                .collect(),
            requested_claims: self
                .preview
                .requested_claims()
                .iter()
                .map(|claim| RequestedPresentationClaimView {
                    claim_path: claim.path().to_owned(),
                    label: claim.label().to_owned(),
                    intent: claim.intent().as_str().to_owned(),
                    predicate_kind: claim.predicate_kind().map(str::to_owned),
                    threshold: claim.threshold(),
                })
                .collect(),
            state: self.state.as_str().to_owned(),
            presentation_generated: self.presentation_generated,
            verifier_validated: self.verifier_validated,
            failure_code: self.failure_code.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialPresentationError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidPresentationIdentifier(OpaqueIdError),
    InvalidRequest,
    InvalidCredential,
    ConfirmationRequired,
    InvalidConfirmation,
    NotFound,
    InvalidState,
    Approval(CredentialPresentationApprovalError),
    Protocol(PresentationProtocolError),
    Unavailable,
}

impl fmt::Display for CredentialPresentationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) | Self::InvalidPresentationIdentifier(error) => {
                error.fmt(formatter)
            }
            Self::InvalidRequest => {
                formatter.write_str("credential presentation request is invalid")
            }
            Self::InvalidCredential => {
                formatter.write_str("credential presentation selection is invalid")
            }
            Self::ConfirmationRequired => {
                formatter.write_str("credential presentation requires explicit consent")
            }
            Self::InvalidConfirmation => {
                formatter.write_str("credential presentation consent intent is invalid")
            }
            Self::NotFound => formatter.write_str("credential presentation was not found"),
            Self::InvalidState => formatter.write_str("credential presentation state is invalid"),
            Self::Approval(error) => error.fmt(formatter),
            Self::Protocol(error) => error.fmt(formatter),
            Self::Unavailable => {
                formatter.write_str("credential presentation state is unavailable")
            }
        }
    }
}

impl Error for CredentialPresentationError {}

pub trait PrepareCredentialPresentationUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: PrepareCredentialPresentationCommand,
    ) -> PresentationViewFuture<'a>;
}

pub trait AcceptCredentialPresentationUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: AcceptCredentialPresentationCommand,
    ) -> PresentationViewFuture<'a>;
}

pub trait RefuseCredentialPresentationUseCase: Send + Sync {
    fn execute(
        &self,
        command: RefuseCredentialPresentationCommand,
    ) -> Result<CredentialPresentationView, CredentialPresentationError>;
}

pub trait CancelCredentialPresentationUseCase: Send + Sync {
    fn execute(
        &self,
        command: CancelCredentialPresentationCommand,
    ) -> Result<CredentialPresentationView, CredentialPresentationError>;
}

/// Applies native foreground/background lifecycle changes to the active proof
/// worker without exposing proof material to the incoming adapter.
pub trait SetCredentialPresentationForegroundUseCase: Send + Sync {
    fn execute(&self, foreground: bool) -> Result<(), CredentialPresentationError>;
}

pub trait GetCredentialPresentationUseCase: Send + Sync {
    fn execute(
        &self,
        query: CredentialPresentationQuery,
    ) -> Result<CredentialPresentationView, CredentialPresentationError>;
}

pub trait ListCredentialPresentationsUseCase: Send + Sync {
    fn execute(
        &self,
        query: CredentialPresentationProfileQuery,
    ) -> Result<Vec<CredentialPresentationView>, CredentialPresentationError>;
}

pub struct CredentialPresentationService {
    protocol: Arc<dyn CredentialPresentationProtocolPort>,
    authority: Arc<dyn CredentialPresentationAuthorityPort>,
    activity: Arc<CredentialPresentationActivityStore>,
    sessions: Mutex<BTreeMap<CredentialPresentationId, Session>>,
}

struct PresentationAttempt<'a> {
    service: &'a CredentialPresentationService,
    presentation_id: CredentialPresentationId,
}

impl Drop for PresentationAttempt<'_> {
    fn drop(&mut self) {
        self.service.interrupt_if_presenting(&self.presentation_id);
    }
}

struct PresentationRefusalAttempt<'a> {
    service: &'a CredentialPresentationService,
    presentation_id: CredentialPresentationId,
}

impl Drop for PresentationRefusalAttempt<'_> {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.service.sessions.lock()
            && let Some(session) = sessions.get_mut(&self.presentation_id)
        {
            session.refusal_in_progress = false;
        }
    }
}

impl CredentialPresentationService {
    #[must_use]
    pub fn new(protocol: Arc<dyn CredentialPresentationProtocolPort>) -> Self {
        Self::with_authority_and_activity(
            protocol,
            Arc::new(UnavailableCredentialPresentationAuthority),
            Arc::new(CredentialPresentationActivityStore::new()),
        )
    }

    #[must_use]
    pub fn with_authority(
        protocol: Arc<dyn CredentialPresentationProtocolPort>,
        authority: Arc<dyn CredentialPresentationAuthorityPort>,
    ) -> Self {
        Self::with_authority_and_activity(
            protocol,
            authority,
            Arc::new(CredentialPresentationActivityStore::new()),
        )
    }

    #[must_use]
    pub fn with_authority_and_activity(
        protocol: Arc<dyn CredentialPresentationProtocolPort>,
        authority: Arc<dyn CredentialPresentationAuthorityPort>,
        activity: Arc<CredentialPresentationActivityStore>,
    ) -> Self {
        Self {
            protocol,
            authority,
            activity,
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    #[must_use]
    pub fn activity(&self) -> Arc<CredentialPresentationActivityStore> {
        Arc::clone(&self.activity)
    }

    fn sessions(
        &self,
    ) -> Result<
        MutexGuard<'_, BTreeMap<CredentialPresentationId, Session>>,
        CredentialPresentationError,
    > {
        self.sessions
            .lock()
            .map_err(|_| CredentialPresentationError::Unavailable)
    }

    fn fail(&self, id: &CredentialPresentationId, error: PresentationProtocolError) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.get_mut(id)
            && matches!(
                session.state,
                CredentialPresentationState::Presenting
                    | CredentialPresentationState::CancellationRequested
            )
        {
            let (state, status) = match error {
                PresentationProtocolError::ProofCancelled
                | PresentationProtocolError::ProofBackgrounded => (
                    CredentialPresentationState::Cancelled,
                    CredentialPresentationActivityStatus::Cancelled,
                ),
                PresentationProtocolError::ProofTimedOut => (
                    CredentialPresentationState::TimedOut,
                    CredentialPresentationActivityStatus::TimedOut,
                ),
                PresentationProtocolError::Unavailable => (
                    CredentialPresentationState::Failed,
                    CredentialPresentationActivityStatus::OutcomeUnknown,
                ),
                _ => (
                    CredentialPresentationState::Failed,
                    CredentialPresentationActivityStatus::Failed,
                ),
            };
            session.state = state;
            session.presentation_generated = false;
            session.verifier_validated = false;
            session.failure_code = Some(error.code().to_owned());
            self.activity.update(id, status);
        }
    }

    fn interrupt_if_presenting(&self, id: &CredentialPresentationId) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.get_mut(id)
            && session.state == CredentialPresentationState::Presenting
        {
            session.state = CredentialPresentationState::Failed;
            session.failure_code = Some("presentation_outcome_unknown".to_owned());
            self.activity
                .update(id, CredentialPresentationActivityStatus::OutcomeUnknown);
        }
    }
}

fn profile(value: String) -> Result<PresentationProfileId, CredentialPresentationError> {
    PresentationProfileId::parse(value)
        .map_err(CredentialPresentationError::InvalidProfileIdentifier)
}

fn presentation_id(value: String) -> Result<CredentialPresentationId, CredentialPresentationError> {
    CredentialPresentationId::parse(value)
        .map_err(CredentialPresentationError::InvalidPresentationIdentifier)
}

impl PrepareCredentialPresentationUseCase for CredentialPresentationService {
    fn execute<'a>(
        &'a self,
        command: PrepareCredentialPresentationCommand,
    ) -> PresentationViewFuture<'a> {
        Box::pin(async move {
            let profile_id = profile(command.profile_id)?;
            if command.request.is_empty() || command.request.len() > MAX_PRESENTATION_REQUEST_BYTES
            {
                return Err(CredentialPresentationError::InvalidRequest);
            }
            let prepared = self
                .protocol
                .prepare(PrepareCredentialPresentationRequest {
                    profile_id: profile_id.clone(),
                    request: command.request,
                })
                .await
                .map_err(CredentialPresentationError::Protocol)?;
            let session = Session {
                profile_id,
                preview: prepared.preview,
                state: CredentialPresentationState::AwaitingConsent,
                presentation_generated: false,
                verifier_validated: false,
                failure_code: None,
                refusal_in_progress: false,
                protocol_discarded: false,
            };
            let view = session.view(&prepared.id);
            if self.sessions()?.insert(prepared.id, session).is_some() {
                return Err(CredentialPresentationError::InvalidState);
            }
            Ok(view)
        })
    }
}

impl AcceptCredentialPresentationUseCase for CredentialPresentationService {
    fn execute<'a>(
        &'a self,
        command: AcceptCredentialPresentationCommand,
    ) -> PresentationViewFuture<'a> {
        Box::pin(async move {
            if !command.confirmed {
                return Err(CredentialPresentationError::ConfirmationRequired);
            }
            if command.intent != "ACCEPT_CREDENTIAL_PRESENTATION" {
                return Err(CredentialPresentationError::InvalidConfirmation);
            }
            if command.credential_id.is_empty()
                || command.credential_id.len() > MAX_CREDENTIAL_IDENTIFIER_CHARACTERS
            {
                return Err(CredentialPresentationError::InvalidCredential);
            }
            let profile_id = profile(command.profile_id)?;
            let presentation_id = presentation_id(command.presentation_id)?;
            let authority = {
                let mut sessions = self.sessions()?;
                let session = sessions
                    .get_mut(&presentation_id)
                    .ok_or(CredentialPresentationError::NotFound)?;
                if session.profile_id != profile_id {
                    return Err(CredentialPresentationError::NotFound);
                }
                if session.state != CredentialPresentationState::AwaitingConsent {
                    return Err(CredentialPresentationError::InvalidState);
                }
                if !session
                    .preview
                    .candidates()
                    .iter()
                    .any(|candidate| candidate.credential_id() == command.credential_id)
                {
                    return Err(CredentialPresentationError::InvalidCredential);
                }
                let authority = self
                    .authority
                    .mint(CredentialPresentationAuthorityRequest {
                        profile_id: profile_id.as_str().to_owned(),
                        flow_id: OPENID4VP_CREDENTIAL_PRESENTATION_FLOW_ID,
                        session_id: presentation_id.as_str().to_owned(),
                        credential_id: command.credential_id.clone(),
                    })
                    .map_err(CredentialPresentationError::Approval)?;
                self.activity
                    .begin(&presentation_id, session)
                    .ok_or(CredentialPresentationError::Unavailable)?;
                session.state = CredentialPresentationState::Presenting;
                authority
            };
            let _attempt = PresentationAttempt {
                service: self,
                presentation_id: presentation_id.clone(),
            };
            let outcome = match self
                .protocol
                .present(ProtocolPresentCredentialRequest {
                    profile_id,
                    presentation_id: presentation_id.clone(),
                    credential_id: command.credential_id,
                    authority,
                })
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    self.fail(&presentation_id, error);
                    return Err(CredentialPresentationError::Protocol(error));
                }
            };
            if !outcome.verifier_validated {
                self.fail(
                    &presentation_id,
                    PresentationProtocolError::VerifierRejected,
                );
                return Err(CredentialPresentationError::Protocol(
                    PresentationProtocolError::VerifierRejected,
                ));
            }
            self.activity.update(
                &presentation_id,
                CredentialPresentationActivityStatus::Shared,
            );
            let mut sessions = self.sessions()?;
            let session = sessions
                .get_mut(&presentation_id)
                .ok_or(CredentialPresentationError::NotFound)?;
            if session.state != CredentialPresentationState::Presenting {
                return Err(CredentialPresentationError::InvalidState);
            }
            session.state = CredentialPresentationState::Succeeded;
            session.presentation_generated = true;
            session.verifier_validated = true;
            session.failure_code = None;
            Ok(session.view(&presentation_id))
        })
    }
}

impl CancelCredentialPresentationUseCase for CredentialPresentationService {
    fn execute(
        &self,
        command: CancelCredentialPresentationCommand,
    ) -> Result<CredentialPresentationView, CredentialPresentationError> {
        let profile_id = profile(command.profile_id)?;
        let presentation_id = presentation_id(command.presentation_id)?;
        {
            let sessions = self.sessions()?;
            let session = sessions
                .get(&presentation_id)
                .ok_or(CredentialPresentationError::NotFound)?;
            if session.profile_id != profile_id {
                return Err(CredentialPresentationError::NotFound);
            }
            if session.state != CredentialPresentationState::Presenting {
                return Err(CredentialPresentationError::InvalidState);
            }
        }
        self.protocol
            .cancel(CancelPresentationProofRequest {
                profile_id,
                presentation_id: presentation_id.clone(),
            })
            .map_err(CredentialPresentationError::Protocol)?;
        let mut sessions = self.sessions()?;
        let session = sessions
            .get_mut(&presentation_id)
            .ok_or(CredentialPresentationError::NotFound)?;
        if session.state == CredentialPresentationState::Presenting {
            session.state = CredentialPresentationState::CancellationRequested;
            session.failure_code = None;
        }
        Ok(session.view(&presentation_id))
    }
}

impl SetCredentialPresentationForegroundUseCase for CredentialPresentationService {
    fn execute(&self, foreground: bool) -> Result<(), CredentialPresentationError> {
        self.protocol
            .set_foreground(foreground)
            .map_err(CredentialPresentationError::Protocol)?;
        if !foreground {
            for session in self.sessions()?.values_mut() {
                if session.state == CredentialPresentationState::Presenting {
                    session.state = CredentialPresentationState::CancellationRequested;
                    session.failure_code = None;
                }
            }
        }
        Ok(())
    }
}

impl RefuseCredentialPresentationUseCase for CredentialPresentationService {
    fn execute(
        &self,
        command: RefuseCredentialPresentationCommand,
    ) -> Result<CredentialPresentationView, CredentialPresentationError> {
        let profile_id = profile(command.profile_id)?;
        let presentation_id = presentation_id(command.presentation_id)?;
        {
            let mut sessions = self.sessions()?;
            let session = sessions
                .get_mut(&presentation_id)
                .ok_or(CredentialPresentationError::NotFound)?;
            if session.profile_id != profile_id {
                return Err(CredentialPresentationError::NotFound);
            }
            if session.protocol_discarded {
                return Ok(session.view(&presentation_id));
            }
            if session.state != CredentialPresentationState::AwaitingConsent
                || session.refusal_in_progress
            {
                return Err(CredentialPresentationError::InvalidState);
            }
            session.refusal_in_progress = true;
        }
        let refusal_attempt = PresentationRefusalAttempt {
            service: self,
            presentation_id: presentation_id.clone(),
        };
        self.protocol
            .discard(&presentation_id)
            .map_err(CredentialPresentationError::Protocol)?;
        let mut sessions = self.sessions()?;
        let session = sessions
            .get_mut(&presentation_id)
            .ok_or(CredentialPresentationError::NotFound)?;
        session.protocol_discarded = true;
        session.state = CredentialPresentationState::Refused;
        session.failure_code = None;
        let _ = self.activity.begin(&presentation_id, session);
        self.activity.update(
            &presentation_id,
            CredentialPresentationActivityStatus::Refused,
        );
        let view = session.view(&presentation_id);
        drop(sessions);
        drop(refusal_attempt);
        Ok(view)
    }
}

impl GetCredentialPresentationUseCase for CredentialPresentationService {
    fn execute(
        &self,
        query: CredentialPresentationQuery,
    ) -> Result<CredentialPresentationView, CredentialPresentationError> {
        let profile_id = profile(query.profile_id)?;
        let presentation_id = presentation_id(query.presentation_id)?;
        let sessions = self.sessions()?;
        let session = sessions
            .get(&presentation_id)
            .filter(|session| session.profile_id == profile_id)
            .ok_or(CredentialPresentationError::NotFound)?;
        Ok(session.view(&presentation_id))
    }
}

impl ListCredentialPresentationsUseCase for CredentialPresentationService {
    fn execute(
        &self,
        query: CredentialPresentationProfileQuery,
    ) -> Result<Vec<CredentialPresentationView>, CredentialPresentationError> {
        let profile_id = profile(query.profile_id)?;
        Ok(self
            .sessions()?
            .iter()
            .filter(|(_, session)| session.profile_id == profile_id)
            .map(|(id, session)| session.view(id))
            .collect())
    }
}

impl ListCredentialPresentationActivityUseCase for CredentialPresentationService {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<CredentialPresentationActivityView, CredentialPresentationActivityError> {
        self.activity.execute(profile_id)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialPresentationProtocol;

impl CredentialPresentationProtocolPort for UnavailableCredentialPresentationProtocol {
    fn prepare<'a>(
        &'a self,
        _: PrepareCredentialPresentationRequest,
    ) -> PreparePresentationPortFuture<'a> {
        Box::pin(async { Err(PresentationProtocolError::Unavailable) })
    }

    fn present<'a>(
        &'a self,
        _: ProtocolPresentCredentialRequest,
    ) -> PresentCredentialPortFuture<'a> {
        Box::pin(async { Err(PresentationProtocolError::Unavailable) })
    }

    fn discard(&self, _: &CredentialPresentationId) -> Result<(), PresentationProtocolError> {
        Err(PresentationProtocolError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailablePresentationProof;

impl PresentationProofPort for UnavailablePresentationProof {
    fn create<'a>(&'a self, _: PresentationProofRequest) -> CreatePresentationProofFuture<'a> {
        Box::pin(async { Err(PresentationProofError::Unavailable) })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailablePresentationHolderAuthorization;

impl PresentationHolderAuthorizationPort for UnavailablePresentationHolderAuthorization {
    fn authorize<'a>(
        &'a self,
        _: PresentationHolderAuthorizationRequest,
    ) -> AuthorizePresentationHolderFuture<'a> {
        Box::pin(async { Err(PresentationHolderAuthorizationError::Unavailable) })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailablePresentationVerifier;

impl PresentationVerifierPort for UnavailablePresentationVerifier {
    fn verify<'a>(
        &'a self,
        _: PresentationVerificationRequest,
    ) -> VerifyPresentationProofFuture<'a> {
        Box::pin(async { Err(PresentationVerificationError::Unavailable) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::task::{Context, Poll, Waker};

    struct PresentationAuthority(
        oxid_foundation::AcceptedFlowIssuer<{ oxid_foundation::CREDENTIAL_PRESENTATION_FLOW_KIND }>,
    );

    impl CredentialPresentationAuthorityPort for PresentationAuthority {
        fn mint(
            &self,
            request: CredentialPresentationAuthorityRequest,
        ) -> Result<AcceptedCredentialPresentationFlow, CredentialPresentationApprovalError>
        {
            Ok(self.0.mint(
                request,
                oxid_foundation::UnixTimestampMillis::new(1),
                oxid_foundation::UnixTimestampMillis::new(2),
                0,
            ))
        }
    }

    fn approved_service(protocol: Arc<Protocol>) -> CredentialPresentationService {
        CredentialPresentationService::with_authority(
            protocol,
            Arc::new(PresentationAuthority(
                oxid_foundation::AcceptedFlowIssuer::new(),
            )),
        )
    }

    fn ready<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly yielded"),
        }
    }

    #[derive(Default)]
    struct Protocol {
        selected_credential_id: Mutex<Option<String>>,
        cancelled_presentation_id: Mutex<Option<String>>,
        foreground_events: Mutex<Vec<bool>>,
        discard_count: AtomicUsize,
        present_succeeds: AtomicBool,
        present_yields_once: AtomicBool,
    }

    impl CredentialPresentationProtocolPort for Protocol {
        fn prepare<'a>(
            &'a self,
            request: PrepareCredentialPresentationRequest,
        ) -> PreparePresentationPortFuture<'a> {
            Box::pin(async move {
                let first_candidate = PresentationCredentialCandidate::new(
                    "vc_one",
                    "Digital Passport",
                    "did:midnight:undeployed:issuer",
                )
                .expect("candidate");
                let second_candidate = PresentationCredentialCandidate::new(
                    "vc_two",
                    "Digital Passport",
                    "did:midnight:undeployed:second-issuer",
                )
                .expect("candidate");
                let claims = vec![
                    RequestedPresentationClaim::reveal(
                        "/credentialSubject/firstName",
                        "First name",
                    )
                    .expect("claim"),
                    RequestedPresentationClaim::predicate(
                        "/credentialSubject/dateOfBirth",
                        "Age over 18",
                        "age_over",
                        18,
                    )
                    .expect("predicate"),
                ];
                Ok(PreparedCredentialPresentation {
                    id: CredentialPresentationId::parse("presentation_one").expect("id"),
                    preview: CredentialPresentationPreview::new(
                        "https://verifier.example",
                        format!("Purpose for {}", request.profile_id.as_str()),
                        "digital_passport",
                        vec![first_candidate, second_candidate],
                        claims,
                    )
                    .expect("preview"),
                })
            })
        }

        fn present<'a>(
            &'a self,
            request: ProtocolPresentCredentialRequest,
        ) -> PresentCredentialPortFuture<'a> {
            Box::pin(std::future::poll_fn(move |_| {
                *self
                    .selected_credential_id
                    .lock()
                    .expect("selected credential lock") = Some(request.credential_id.clone());
                if self.present_yields_once.swap(false, Ordering::SeqCst) {
                    return Poll::Pending;
                }
                if self.present_succeeds.load(Ordering::SeqCst) {
                    Poll::Ready(Ok(PresentationProtocolOutcome {
                        verifier_validated: true,
                    }))
                } else {
                    Poll::Ready(Err(PresentationProtocolError::ProofUnavailable))
                }
            }))
        }

        fn discard(&self, _: &CredentialPresentationId) -> Result<(), PresentationProtocolError> {
            self.discard_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn cancel(
            &self,
            request: CancelPresentationProofRequest,
        ) -> Result<(), PresentationProtocolError> {
            *self
                .cancelled_presentation_id
                .lock()
                .expect("cancelled presentation lock") =
                Some(request.presentation_id.as_str().to_owned());
            Ok(())
        }

        fn set_foreground(&self, foreground: bool) -> Result<(), PresentationProtocolError> {
            self.foreground_events
                .lock()
                .expect("foreground events lock")
                .push(foreground);
            Ok(())
        }
    }

    #[test]
    fn exact_consent_is_profile_scoped_and_proof_failure_is_terminal() {
        let protocol = Arc::new(Protocol::default());
        let service = approved_service(protocol.clone());
        let prepared = ready(PrepareCredentialPresentationUseCase::execute(
            &service,
            PrepareCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                request: "openid4vp://authorize".to_owned(),
            },
        ))
        .expect("prepare");
        assert!(!prepared.presentation_generated);
        assert!(!prepared.verifier_validated);
        assert_eq!(prepared.requested_claims[1].threshold, Some(18));
        assert_eq!(prepared.candidates.len(), 2);
        assert_eq!(
            prepared.candidates[1].issuer,
            "did:midnight:undeployed:second-issuer"
        );

        assert_eq!(
            GetCredentialPresentationUseCase::execute(
                &service,
                CredentialPresentationQuery {
                    profile_id: "profile_two".to_owned(),
                    presentation_id: prepared.id.clone(),
                },
            ),
            Err(CredentialPresentationError::NotFound)
        );
        assert_eq!(
            ready(AcceptCredentialPresentationUseCase::execute(
                &service,
                AcceptCredentialPresentationCommand {
                    profile_id: "profile_one".to_owned(),
                    presentation_id: prepared.id.clone(),
                    credential_id: "vc_not_listed".to_owned(),
                    confirmed: true,
                    intent: "ACCEPT_CREDENTIAL_PRESENTATION".to_owned(),
                },
            )),
            Err(CredentialPresentationError::InvalidCredential)
        );
        assert_eq!(
            ready(AcceptCredentialPresentationUseCase::execute(
                &service,
                AcceptCredentialPresentationCommand {
                    profile_id: "profile_one".to_owned(),
                    presentation_id: prepared.id.clone(),
                    credential_id: "vc_two".to_owned(),
                    confirmed: true,
                    intent: "ACCEPT_CREDENTIAL_PRESENTATION".to_owned(),
                },
            )),
            Err(CredentialPresentationError::Protocol(
                PresentationProtocolError::ProofUnavailable
            ))
        );
        assert_eq!(
            protocol
                .selected_credential_id
                .lock()
                .expect("selected credential lock")
                .as_deref(),
            Some("vc_two")
        );
        let failed = GetCredentialPresentationUseCase::execute(
            &service,
            CredentialPresentationQuery {
                profile_id: "profile_one".to_owned(),
                presentation_id: prepared.id,
            },
        )
        .expect("failed view");
        assert_eq!(failed.state, "failed");
        assert_eq!(failed.failure_code.as_deref(), Some("proof_unavailable"));
        assert!(!failed.presentation_generated);
    }

    #[test]
    fn default_acceptance_retains_preview_and_has_zero_protocol_effects() {
        let protocol = Arc::new(Protocol::default());
        let service = CredentialPresentationService::new(protocol.clone());
        let prepared = ready(PrepareCredentialPresentationUseCase::execute(
            &service,
            PrepareCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                request: "openid4vp://authorize".to_owned(),
            },
        ))
        .expect("preview");
        assert_eq!(
            ready(AcceptCredentialPresentationUseCase::execute(
                &service,
                AcceptCredentialPresentationCommand {
                    profile_id: "profile_one".to_owned(),
                    presentation_id: prepared.id.clone(),
                    credential_id: "vc_one".to_owned(),
                    confirmed: true,
                    intent: "ACCEPT_CREDENTIAL_PRESENTATION".to_owned(),
                },
            )),
            Err(CredentialPresentationError::Approval(
                CredentialPresentationApprovalError::Unavailable,
            ))
        );
        assert!(
            protocol
                .selected_credential_id
                .lock()
                .expect("protocol effects")
                .is_none()
        );
        let retained = GetCredentialPresentationUseCase::execute(
            &service,
            CredentialPresentationQuery {
                profile_id: "profile_one".to_owned(),
                presentation_id: prepared.id,
            },
        )
        .expect("retained preview");
        assert_eq!(retained.state, "awaiting_consent");
        assert!(!retained.presentation_generated);
    }

    #[test]
    fn cancellation_request_is_profile_scoped_and_not_an_acknowledgement() {
        let protocol = Arc::new(Protocol::default());
        let service = CredentialPresentationService::new(protocol.clone());
        let prepared = ready(PrepareCredentialPresentationUseCase::execute(
            &service,
            PrepareCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                request: "openid4vp://authorize".to_owned(),
            },
        ))
        .expect("prepare");
        service
            .sessions
            .lock()
            .expect("sessions")
            .get_mut(
                &CredentialPresentationId::parse(prepared.id.clone()).expect("presentation id"),
            )
            .expect("session")
            .state = CredentialPresentationState::Presenting;

        assert_eq!(
            CancelCredentialPresentationUseCase::execute(
                &service,
                CancelCredentialPresentationCommand {
                    profile_id: "profile_two".to_owned(),
                    presentation_id: prepared.id.clone(),
                },
            ),
            Err(CredentialPresentationError::NotFound)
        );
        let cancelling = CancelCredentialPresentationUseCase::execute(
            &service,
            CancelCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                presentation_id: prepared.id.clone(),
            },
        )
        .expect("request cancellation");
        assert_eq!(cancelling.state, "cancellation_requested");
        assert_ne!(cancelling.state, "cancelled");
        assert_eq!(
            protocol
                .cancelled_presentation_id
                .lock()
                .expect("cancelled presentation lock")
                .as_deref(),
            Some(prepared.id.as_str())
        );
    }

    #[test]
    fn accept_does_not_overwrite_an_intervening_session_state_change() {
        let protocol = Arc::new(Protocol::default());
        protocol.present_succeeds.store(true, Ordering::SeqCst);
        protocol.present_yields_once.store(true, Ordering::SeqCst);
        let service = approved_service(protocol);
        let prepared = ready(PrepareCredentialPresentationUseCase::execute(
            &service,
            PrepareCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                request: "openid4vp://authorize".to_owned(),
            },
        ))
        .expect("prepare");
        let presentation_id =
            CredentialPresentationId::parse(prepared.id.clone()).expect("presentation id");
        let mut accept = Box::pin(AcceptCredentialPresentationUseCase::execute(
            &service,
            AcceptCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                presentation_id: prepared.id,
                credential_id: "vc_one".to_owned(),
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_PRESENTATION".to_owned(),
            },
        ));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(accept.as_mut().poll(&mut context), Poll::Pending));

        service
            .sessions
            .lock()
            .expect("sessions")
            .get_mut(&presentation_id)
            .expect("session")
            .state = CredentialPresentationState::CancellationRequested;

        assert_eq!(
            accept.as_mut().poll(&mut context),
            Poll::Ready(Err(CredentialPresentationError::InvalidState))
        );
        let view = GetCredentialPresentationUseCase::execute(
            &service,
            CredentialPresentationQuery {
                profile_id: "profile_one".to_owned(),
                presentation_id: presentation_id.as_str().to_owned(),
            },
        )
        .expect("view");
        assert_eq!(view.state, "cancellation_requested");
    }

    #[test]
    fn backgrounding_marks_presenting_sessions_as_cancellation_requested() {
        let protocol = Arc::new(Protocol::default());
        let service = CredentialPresentationService::new(protocol.clone());
        let prepared = ready(PrepareCredentialPresentationUseCase::execute(
            &service,
            PrepareCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                request: "openid4vp://authorize".to_owned(),
            },
        ))
        .expect("prepare");
        service
            .sessions
            .lock()
            .expect("sessions")
            .get_mut(
                &CredentialPresentationId::parse(prepared.id.clone()).expect("presentation id"),
            )
            .expect("session")
            .state = CredentialPresentationState::Presenting;

        SetCredentialPresentationForegroundUseCase::execute(&service, false).expect("background");
        let view = GetCredentialPresentationUseCase::execute(
            &service,
            CredentialPresentationQuery {
                profile_id: "profile_one".to_owned(),
                presentation_id: prepared.id,
            },
        )
        .expect("view");
        assert_eq!(view.state, "cancellation_requested");
        assert_eq!(
            *protocol
                .foreground_events
                .lock()
                .expect("foreground events lock"),
            vec![false]
        );
    }

    #[test]
    fn presentation_activity_exists_before_disclosure_and_records_safe_success() {
        let protocol = Arc::new(Protocol::default());
        protocol.present_succeeds.store(true, Ordering::SeqCst);
        protocol.present_yields_once.store(true, Ordering::SeqCst);
        let activity = Arc::new(CredentialPresentationActivityStore::new());
        let service = CredentialPresentationService::with_authority_and_activity(
            protocol,
            Arc::new(PresentationAuthority(
                oxid_foundation::AcceptedFlowIssuer::new(),
            )),
            activity.clone(),
        );
        let prepared = ready(PrepareCredentialPresentationUseCase::execute(
            &service,
            PrepareCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                request: "openid4vp://authorize".to_owned(),
            },
        ))
        .expect("prepare");
        let mut accept = Box::pin(AcceptCredentialPresentationUseCase::execute(
            &service,
            AcceptCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                presentation_id: prepared.id,
                credential_id: "vc_one".to_owned(),
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_PRESENTATION".to_owned(),
            },
        ));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(accept.as_mut().poll(&mut context), Poll::Pending));

        let pending = activity
            .execute("profile_one".to_owned())
            .expect("pending activity");
        assert_eq!(pending.records.len(), 1);
        assert_eq!(pending.records[0].id.value(), 1);
        assert_eq!(
            pending.records[0].status,
            CredentialPresentationActivityStatus::Pending
        );
        assert_eq!(pending.records[0].purpose, "Purpose for profile_one");
        assert_eq!(pending.records[0].presentation_type, "digital_passport");
        assert_eq!(
            pending.records[0].verifier.as_deref(),
            Some("https://verifier.example")
        );

        assert!(matches!(
            accept.as_mut().poll(&mut context),
            Poll::Ready(Ok(_))
        ));
        let shared = activity
            .execute("profile_one".to_owned())
            .expect("shared activity");
        assert_eq!(shared.records[0].id.value(), 1);
        assert_eq!(
            shared.records[0].status,
            CredentialPresentationActivityStatus::Shared
        );
        assert_eq!(
            shared.records[0].finality,
            CredentialPresentationActivityFinality::Final
        );
    }

    #[test]
    fn presentation_activity_command_event_matrix_is_idempotent_and_non_regressing() {
        let store = CredentialPresentationActivityStore::new();
        let session = Session {
            profile_id: PresentationProfileId::parse("profile_one").expect("profile"),
            preview: CredentialPresentationPreview::new(
                "https://verifier.example",
                "Age assurance",
                "digital_passport",
                vec![
                    PresentationCredentialCandidate::new(
                        "vc_one",
                        "Digital Passport",
                        "did:example:issuer",
                    )
                    .expect("candidate"),
                ],
                vec![
                    RequestedPresentationClaim::reveal("/givenName", "Given name").expect("claim"),
                ],
            )
            .expect("preview"),
            state: CredentialPresentationState::AwaitingConsent,
            presentation_generated: false,
            verifier_validated: false,
            failure_code: None,
            refusal_in_progress: false,
            protocol_discarded: false,
        };

        let first = CredentialPresentationId::parse("presentation_first").expect("first");
        let stable = store.begin(&first, &session).expect("first activity");
        assert_eq!(store.begin(&first, &session), Some(stable));
        store.update(&first, CredentialPresentationActivityStatus::Pending);
        store.update(&first, CredentialPresentationActivityStatus::TimedOut);
        store.update(&first, CredentialPresentationActivityStatus::Shared);
        store.update(&first, CredentialPresentationActivityStatus::Failed);

        let refused = CredentialPresentationId::parse("presentation_refused").expect("refused");
        store.begin(&refused, &session).expect("refused activity");
        store.update(&refused, CredentialPresentationActivityStatus::Refused);
        store.update(&refused, CredentialPresentationActivityStatus::Shared);

        let cancelled =
            CredentialPresentationId::parse("presentation_cancelled").expect("cancelled");
        store
            .begin(&cancelled, &session)
            .expect("cancelled activity");
        store.update(&cancelled, CredentialPresentationActivityStatus::Cancelled);
        store.update(&cancelled, CredentialPresentationActivityStatus::Pending);

        let unknown = CredentialPresentationId::parse("presentation_unknown").expect("unknown");
        store.begin(&unknown, &session).expect("unknown activity");
        store.update(
            &unknown,
            CredentialPresentationActivityStatus::OutcomeUnknown,
        );
        store.update(&unknown, CredentialPresentationActivityStatus::Shared);

        let failed = CredentialPresentationId::parse("presentation_failed").expect("failed");
        store.begin(&failed, &session).expect("failed activity");
        store.update(&failed, CredentialPresentationActivityStatus::Failed);
        store.update(&failed, CredentialPresentationActivityStatus::Shared);

        let view = store
            .execute("profile_one".to_owned())
            .expect("matrix view");
        let statuses = view
            .records
            .iter()
            .map(|record| record.status)
            .collect::<Vec<_>>();
        assert_eq!(
            statuses,
            vec![
                CredentialPresentationActivityStatus::Failed,
                CredentialPresentationActivityStatus::Shared,
                CredentialPresentationActivityStatus::Cancelled,
                CredentialPresentationActivityStatus::Refused,
                CredentialPresentationActivityStatus::Shared,
            ]
        );
    }

    #[test]
    fn refusal_activity_is_idempotent_and_process_local() {
        let protocol = Arc::new(Protocol::default());
        let activity = Arc::new(CredentialPresentationActivityStore::new());
        let service = CredentialPresentationService::with_authority_and_activity(
            protocol.clone(),
            Arc::new(PresentationAuthority(
                oxid_foundation::AcceptedFlowIssuer::new(),
            )),
            activity.clone(),
        );
        let prepared = ready(PrepareCredentialPresentationUseCase::execute(
            &service,
            PrepareCredentialPresentationCommand {
                profile_id: "profile_one".to_owned(),
                request: "openid4vp://authorize".to_owned(),
            },
        ))
        .expect("prepare");
        let command = RefuseCredentialPresentationCommand {
            profile_id: "profile_one".to_owned(),
            presentation_id: prepared.id,
        };
        RefuseCredentialPresentationUseCase::execute(&service, command.clone()).expect("refuse");
        RefuseCredentialPresentationUseCase::execute(&service, command).expect("duplicate refuse");
        assert_eq!(protocol.discard_count.load(Ordering::SeqCst), 1);
        let view = activity
            .execute("profile_one".to_owned())
            .expect("refused activity");
        assert_eq!(view.records.len(), 1);
        assert_eq!(
            view.records[0].status,
            CredentialPresentationActivityStatus::Refused
        );
        assert_eq!(activity.clear_profile("profile_one").expect("clear"), 1);
        assert!(
            activity
                .execute("profile_one".to_owned())
                .expect("cleared")
                .records
                .is_empty()
        );
        assert!(
            CredentialPresentationActivityStore::new()
                .execute("profile_one".to_owned())
                .expect("restart projection")
                .records
                .is_empty()
        );
    }

    #[test]
    fn holder_authorization_redacts_the_exact_statement_and_fails_closed_by_default() {
        let request = PresentationHolderAuthorizationRequest {
            profile_id: PresentationProfileId::parse("profile_one").expect("profile"),
            holder_did: "did:midnight:undeployed:holder".to_owned(),
            holder_method_id: "did:midnight:undeployed:holder#jubjub-1".to_owned(),
            verifier: "https://verifier.example".to_owned(),
            presentation_statement: [0x5a; 32],
        };

        let debug = format!("{request:?}");
        assert!(debug.contains("jubjub-1"));
        assert!(!debug.contains("presentation_statement"));
        assert!(!debug.contains("5a5a5a"));
        assert_eq!(
            ready(UnavailablePresentationHolderAuthorization.authorize(request)),
            Err(PresentationHolderAuthorizationError::Unavailable)
        );
    }
}
