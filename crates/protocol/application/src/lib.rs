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

use oxid_foundation::{
    AcceptedCredentialIssuanceFlow, AcceptedSelfIssuedAuthenticationFlow, OpaqueIdError,
};
use oxid_protocol_domain::{
    CredentialIssuanceId, CredentialIssuanceState, CredentialOfferPreview, ProtocolProfileId,
    SelfIssuedAuthenticationId, SelfIssuedAuthenticationPreview, SelfIssuedAuthenticationState,
};

pub const MAX_CREDENTIAL_OFFER_BYTES: usize = 32 * 1_024;
pub const MAX_SELF_ISSUED_REQUEST_BYTES: usize = 32 * 1_024;
pub const MAX_IDENTITY_REQUEST_URI_BYTES: usize = 32 * 1_024;
const MAX_DID_CHARACTERS: usize = 8_192;
const MAX_METHOD_CHARACTERS: usize = 8_192;
const ISSUANCE_INTERRUPTED_CODE: &str = "issuance_interrupted";
pub const OID4VCI_CREDENTIAL_ISSUANCE_FLOW_ID: &str = "openid4vci";
pub const SIOPV2_SELF_ISSUED_AUTHENTICATION_FLOW_ID: &str = "siopv2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptedFlowApprovalError {
    Unavailable,
}

impl fmt::Display for AcceptedFlowApprovalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("approval_unavailable")
    }
}

impl Error for AcceptedFlowApprovalError {}

#[derive(PartialEq, Eq)]
pub struct CredentialIssuanceAuthorityRequest {
    pub profile_id: String,
    pub holder_did: String,
    pub method_id: String,
    pub flow_id: &'static str,
    pub session_id: String,
}

pub trait CredentialIssuanceAuthorityPort: Send + Sync {
    fn mint(
        &self,
        request: CredentialIssuanceAuthorityRequest,
    ) -> Result<AcceptedCredentialIssuanceFlow, AcceptedFlowApprovalError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialIssuanceAuthority;

impl CredentialIssuanceAuthorityPort for UnavailableCredentialIssuanceAuthority {
    fn mint(
        &self,
        _: CredentialIssuanceAuthorityRequest,
    ) -> Result<AcceptedCredentialIssuanceFlow, AcceptedFlowApprovalError> {
        Err(AcceptedFlowApprovalError::Unavailable)
    }
}

#[derive(PartialEq, Eq)]
pub struct SelfIssuedAuthenticationAuthorityRequest {
    pub profile_id: String,
    pub holder_did: String,
    pub method_id: String,
    pub flow_id: &'static str,
    pub session_id: String,
}

pub trait SelfIssuedAuthenticationAuthorityPort: Send + Sync {
    fn mint(
        &self,
        request: SelfIssuedAuthenticationAuthorityRequest,
    ) -> Result<AcceptedSelfIssuedAuthenticationFlow, AcceptedFlowApprovalError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableSelfIssuedAuthenticationAuthority;

impl SelfIssuedAuthenticationAuthorityPort for UnavailableSelfIssuedAuthenticationAuthority {
    fn mint(
        &self,
        _: SelfIssuedAuthenticationAuthorityRequest,
    ) -> Result<AcceptedSelfIssuedAuthenticationFlow, AcceptedFlowApprovalError> {
        Err(AcceptedFlowApprovalError::Unavailable)
    }
}

/// A safe routing result for an inbound identity protocol link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityRequestKind {
    CredentialIssuance,
    SelfIssuedAuthentication,
    CredentialPresentation,
}

impl IdentityRequestKind {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::CredentialIssuance => "credential_issuance",
            Self::SelfIssuedAuthentication => "self_issued_authentication",
            Self::CredentialPresentation => "credential_presentation",
        }
    }
}

/// Secret-bearing command whose debug representation never exposes the link.
#[derive(Clone, PartialEq, Eq)]
pub struct RouteIdentityRequestCommand {
    pub request_uri: String,
}

impl fmt::Debug for RouteIdentityRequestCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RouteIdentityRequestCommand")
            .field("request_uri_length", &self.request_uri.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityRequestRoutingError {
    InvalidRequest,
    UnsupportedRequest,
    AmbiguousRequest,
    Unavailable,
}

impl IdentityRequestRoutingError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_identity_request",
            Self::UnsupportedRequest => "unsupported_identity_request",
            Self::AmbiguousRequest => "ambiguous_identity_request",
            Self::Unavailable => "identity_request_routing_unavailable",
        }
    }
}

impl fmt::Display for IdentityRequestRoutingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for IdentityRequestRoutingError {}

/// Classifies the wire format at the protocol edge.
pub trait IdentityRequestRouterPort: Send + Sync {
    fn route(&self, request_uri: &str) -> Result<IdentityRequestKind, IdentityRequestRoutingError>;
}

pub trait RouteIdentityRequestUseCase: Send + Sync {
    fn execute(
        &self,
        command: RouteIdentityRequestCommand,
    ) -> Result<IdentityRequestKind, IdentityRequestRoutingError>;
}

pub struct IdentityRequestRoutingService {
    router: Arc<dyn IdentityRequestRouterPort>,
}

impl IdentityRequestRoutingService {
    #[must_use]
    pub fn new(router: Arc<dyn IdentityRequestRouterPort>) -> Self {
        Self { router }
    }
}

impl RouteIdentityRequestUseCase for IdentityRequestRoutingService {
    fn execute(
        &self,
        command: RouteIdentityRequestCommand,
    ) -> Result<IdentityRequestKind, IdentityRequestRoutingError> {
        let request_uri = command.request_uri;
        if request_uri.is_empty()
            || request_uri.len() > MAX_IDENTITY_REQUEST_URI_BYTES
            || request_uri.chars().any(char::is_control)
            || request_uri.trim() != request_uri
        {
            return Err(IdentityRequestRoutingError::InvalidRequest);
        }
        self.router.route(&request_uri)
    }
}

pub struct UnavailableIdentityRequestRouter;

impl IdentityRequestRouterPort for UnavailableIdentityRequestRouter {
    fn route(&self, _: &str) -> Result<IdentityRequestKind, IdentityRequestRoutingError> {
        Err(IdentityRequestRoutingError::Unavailable)
    }
}

pub trait HolderProofJwt: Send {
    fn as_str(&self) -> &str;
}

pub type PrepareIssuancePortFuture<'a> = Pin<
    Box<dyn Future<Output = Result<PreparedCredentialOffer, IssuanceProtocolError>> + Send + 'a>,
>;
pub type IssueCredentialPortFuture<'a> =
    Pin<Box<dyn Future<Output = Result<IssuedCredentialBytes, IssuanceProtocolError>> + Send + 'a>>;
pub type HolderProofFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn HolderProofJwt>, HolderProofError>> + Send + 'a>>;
pub type StoreIssuedCredentialFuture<'a> =
    Pin<Box<dyn Future<Output = Result<StoredCredential, IssuedCredentialSinkError>> + Send + 'a>>;
pub type IssuanceViewFuture<'a> = Pin<
    Box<dyn Future<Output = Result<CredentialIssuanceView, CredentialIssuanceError>> + Send + 'a>,
>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareIssuanceRequest {
    pub profile_id: ProtocolProfileId,
    pub offer: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCredentialOffer {
    pub id: CredentialIssuanceId,
    pub preview: CredentialOfferPreview,
}

pub struct ProtocolIssueRequest {
    pub profile_id: ProtocolProfileId,
    pub issuance_id: CredentialIssuanceId,
    pub holder_did: String,
    pub method_id: String,
    pub holder_binding_method_id: String,
    pub authority: AcceptedCredentialIssuanceFlow,
}

impl fmt::Debug for ProtocolIssueRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProtocolIssueRequest")
            .field("profile_id", &self.profile_id)
            .field("issuance_id", &self.issuance_id)
            .field("holder_did", &self.holder_did)
            .field("method_id", &self.method_id)
            .field("holder_binding_method_id", &self.holder_binding_method_id)
            .field("authority", &"[REDACTED]")
            .finish()
    }
}

#[derive(PartialEq, Eq)]
pub struct IssuedCredentialBytes {
    pub signed_bytes: Vec<u8>,
    pub detached_proof: Option<Vec<u8>>,
    pub private_material: Option<Vec<u8>>,
}

impl fmt::Debug for IssuedCredentialBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedCredentialBytes")
            .field("signed_bytes_length", &self.signed_bytes.len())
            .field(
                "detached_proof_length",
                &self.detached_proof.as_ref().map(Vec::len),
            )
            .field(
                "private_material_length",
                &self.private_material.as_ref().map(Vec::len),
            )
            .finish_non_exhaustive()
    }
}

/// A prepared preview must bind its issuer endpoint and public configuration
/// metadata to validated issuer metadata, never merely echo an untrusted offer.
pub trait CredentialIssuanceProtocolPort: Send + Sync {
    fn prepare<'a>(&'a self, request: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a>;
    fn issue<'a>(&'a self, request: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a>;
    fn discard(&self, issuance_id: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError>;
}

pub struct HolderProofRequest<'a> {
    pub profile_id: ProtocolProfileId,
    pub holder_did: String,
    pub method_id: String,
    pub audience: String,
    pub nonce: &'a str,
    pub flow_id: &'static str,
    pub session_id: String,
    pub authority: AcceptedCredentialIssuanceFlow,
}

impl fmt::Debug for HolderProofRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HolderProofRequest")
            .field("profile_id", &self.profile_id)
            .field("holder_did", &self.holder_did)
            .field("method_id", &self.method_id)
            .field("audience", &self.audience)
            .field("nonce", &"[REDACTED]")
            .field("flow_id", &self.flow_id)
            .field("session_id", &self.session_id)
            .field("authority", &"[REDACTED]")
            .finish()
    }
}

pub trait CredentialHolderProofPort: Send + Sync {
    fn create<'a>(&'a self, request: HolderProofRequest<'a>) -> HolderProofFuture<'a>;
}

#[derive(PartialEq, Eq)]
pub struct StoreIssuedCredentialRequest {
    pub profile_id: ProtocolProfileId,
    pub signed_bytes: Vec<u8>,
    pub detached_proof: Option<Vec<u8>>,
    pub private_material: Option<Vec<u8>>,
}

impl fmt::Debug for StoreIssuedCredentialRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoreIssuedCredentialRequest")
            .field("profile_id", &self.profile_id)
            .field("signed_bytes_length", &self.signed_bytes.len())
            .field(
                "detached_proof_length",
                &self.detached_proof.as_ref().map(Vec::len),
            )
            .field(
                "private_material_length",
                &self.private_material.as_ref().map(Vec::len),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredCredential {
    pub credential_id: String,
}

pub trait IssuedCredentialSinkPort: Send + Sync {
    fn store_verified<'a>(
        &'a self,
        request: StoreIssuedCredentialRequest,
    ) -> StoreIssuedCredentialFuture<'a>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssuanceProtocolError {
    Unavailable,
    InvalidOffer,
    UnsupportedOffer,
    TransactionCodeRequired,
    InvalidMetadata,
    UnsupportedCredential,
    IssuerRejected,
    InvalidCredentialResponse,
    ProtectionUnavailable,
    WalletLocked,
    InvalidProof,
}

impl IssuanceProtocolError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "protocol_unavailable",
            Self::InvalidOffer => "invalid_offer",
            Self::UnsupportedOffer => "unsupported_offer",
            Self::TransactionCodeRequired => "transaction_code_required",
            Self::InvalidMetadata => "invalid_metadata",
            Self::UnsupportedCredential => "unsupported_credential",
            Self::IssuerRejected => "issuer_rejected",
            Self::InvalidCredentialResponse => "invalid_credential_response",
            Self::ProtectionUnavailable => "protection_unavailable",
            Self::WalletLocked => "wallet_locked",
            Self::InvalidProof => "invalid_proof",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HolderProofError {
    Unavailable,
    DidNotFound,
    MethodNotFound,
    MethodNotAuthorized,
    UnsupportedAlgorithm,
    WalletLocked,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssuedCredentialSinkError {
    Unavailable,
    InvalidCredential,
    VerificationFailed,
    PersistenceFailed,
}

macro_rules! display_code_error {
    ($type:ty, $code:expr) => {
        impl fmt::Display for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str($code(*self))
            }
        }
        impl Error for $type {}
    };
}

display_code_error!(IssuanceProtocolError, IssuanceProtocolError::code);

impl fmt::Display for HolderProofError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "credential holder proof is unavailable",
            Self::DidNotFound => "credential holder DID was not found",
            Self::MethodNotFound => "credential holder method was not found",
            Self::MethodNotAuthorized => "credential holder method is not authorized",
            Self::UnsupportedAlgorithm => "credential holder algorithm is unsupported",
            Self::WalletLocked => "wallet must be unlocked for holder proof",
            Self::Rejected => "credential holder proof was rejected",
        })
    }
}

impl Error for HolderProofError {}

impl fmt::Display for IssuedCredentialSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "issued credential storage is unavailable",
            Self::InvalidCredential => "issued credential is invalid",
            Self::VerificationFailed => "issued credential verification failed",
            Self::PersistenceFailed => "issued credential persistence failed",
        })
    }
}

impl Error for IssuedCredentialSinkError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareCredentialIssuanceCommand {
    pub profile_id: String,
    pub offer: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptCredentialIssuanceCommand {
    pub profile_id: String,
    pub issuance_id: String,
    pub holder_did: String,
    pub method_id: String,
    pub holder_binding_method_id: String,
    pub confirmed: bool,
    pub intent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefuseCredentialIssuanceCommand {
    pub profile_id: String,
    pub issuance_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialIssuanceQuery {
    pub profile_id: String,
    pub issuance_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialIssuanceProfileQuery {
    pub profile_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialIssuanceView {
    pub id: String,
    pub issuer: String,
    pub configuration_ids: Vec<String>,
    pub display_names: Vec<String>,
    pub state: String,
    pub credential_id: Option<String>,
    pub failure_code: Option<String>,
}

/// Maximum number of credential issuance activity records retained per process.
/// This in-memory projection is deleted on restart and is never backed up.
pub const MAX_CREDENTIAL_ISSUANCE_ACTIVITY_RECORDS: usize = 128;

/// Application-owned, bounded monotonic activity identity. This is deliberately
/// distinct from the protocol issuance identifier and is the only identifier
/// exposed by the activity read model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialIssuanceActivityId(u64);

impl CredentialIssuanceActivityId {
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
pub enum CredentialIssuanceActivitySource {
    OpenId4Vci,
}

impl CredentialIssuanceActivitySource {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenId4Vci => "openid4vci",
        }
    }
}

/// Status values are written only from application/protocol/sink lifecycle
/// events. UI navigation never creates cancellation or timeout evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialIssuanceActivityStatus {
    Pending,
    Stored,
    Failed,
    Refused,
    Cancelled,
    TimedOut,
    OutcomeUnknown,
}

impl CredentialIssuanceActivityStatus {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Stored => "stored",
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
            Self::Stored | Self::Failed | Self::Refused | Self::Cancelled
        )
    }

    const fn is_evictable(self) -> bool {
        self.is_final() || matches!(self, Self::TimedOut | Self::OutcomeUnknown)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialIssuanceActivityFinality {
    Pending,
    Final,
    Unknown,
}

impl CredentialIssuanceActivityFinality {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Final => "final",
            Self::Unknown => "unknown",
        }
    }
}

impl From<CredentialIssuanceActivityStatus> for CredentialIssuanceActivityFinality {
    fn from(status: CredentialIssuanceActivityStatus) -> Self {
        match status {
            CredentialIssuanceActivityStatus::Pending => Self::Pending,
            CredentialIssuanceActivityStatus::TimedOut
            | CredentialIssuanceActivityStatus::OutcomeUnknown => Self::Unknown,
            CredentialIssuanceActivityStatus::Stored
            | CredentialIssuanceActivityStatus::Failed
            | CredentialIssuanceActivityStatus::Refused
            | CredentialIssuanceActivityStatus::Cancelled => Self::Final,
        }
    }
}

/// A privacy-safe activity record. It intentionally has no protocol identifier,
/// offer, claims, proofs, key material, credential bytes, or raw errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialIssuanceActivityRecord {
    pub id: CredentialIssuanceActivityId,
    pub profile_id: String,
    pub source: CredentialIssuanceActivitySource,
    pub issuer: String,
    pub credential_configuration_ids: Vec<String>,
    pub status: CredentialIssuanceActivityStatus,
    pub finality: CredentialIssuanceActivityFinality,
    pub observed_at_millis: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialIssuanceActivityView {
    pub source: String,
    pub retention: String,
    pub records: Vec<CredentialIssuanceActivityRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialIssuanceActivityError {
    Unavailable,
}

impl fmt::Display for CredentialIssuanceActivityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("credential issuance activity is unavailable")
    }
}

impl Error for CredentialIssuanceActivityError {}

pub trait ListCredentialIssuanceActivityUseCase: Send + Sync {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<CredentialIssuanceActivityView, CredentialIssuanceActivityError>;
}

#[derive(Default)]
struct CredentialIssuanceActivityState {
    next_id: u64,
    records: VecDeque<CredentialIssuanceActivityRecord>,
    issuance_ids: BTreeMap<CredentialIssuanceId, CredentialIssuanceActivityId>,
}

/// Bounded application-owned producer/read projection. It is process-local,
/// deleted on restart, and deliberately has neither persistence nor backup.
pub struct CredentialIssuanceActivityStore {
    state: Mutex<CredentialIssuanceActivityState>,
}

impl Default for CredentialIssuanceActivityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialIssuanceActivityStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(CredentialIssuanceActivityState::default()),
        }
    }

    /// Purge one profile's process-local activity after its issuance sessions
    /// have been discarded. No record is included in wallet backup.
    pub fn clear_profile(
        &self,
        profile_id: &str,
    ) -> Result<usize, CredentialIssuanceActivityError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CredentialIssuanceActivityError::Unavailable)?;
        let removed: BTreeSet<_> = state
            .records
            .iter()
            .filter(|record| record.profile_id == profile_id)
            .map(|record| record.id)
            .collect();
        state
            .records
            .retain(|record| record.profile_id != profile_id);
        state.issuance_ids.retain(|_, id| !removed.contains(id));
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
        issuance_id: &CredentialIssuanceId,
        session: &Session,
    ) -> Option<CredentialIssuanceActivityId> {
        let mut state = self.state.lock().ok()?;
        if let Some(id) = state.issuance_ids.get(issuance_id) {
            return Some(*id);
        }
        if state.records.len() == MAX_CREDENTIAL_ISSUANCE_ACTIVITY_RECORDS {
            let evict_at = state
                .records
                .iter()
                .position(|record| record.status.is_evictable())?;
            let evicted = state.records.remove(evict_at)?;
            state.issuance_ids.retain(|_, value| *value != evicted.id);
        }
        state.next_id = state.next_id.checked_add(1)?;
        let id = CredentialIssuanceActivityId(state.next_id);
        state.records.push_back(CredentialIssuanceActivityRecord {
            id,
            profile_id: session.profile_id.as_str().to_owned(),
            source: CredentialIssuanceActivitySource::OpenId4Vci,
            issuer: session.preview.issuer().to_owned(),
            credential_configuration_ids: session.preview.configuration_ids().to_vec(),
            status: CredentialIssuanceActivityStatus::Pending,
            finality: CredentialIssuanceActivityFinality::Pending,
            observed_at_millis: Self::now(),
        });
        state.issuance_ids.insert(issuance_id.clone(), id);
        Some(id)
    }

    fn update(&self, issuance_id: &CredentialIssuanceId, status: CredentialIssuanceActivityStatus) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let Some(id) = state.issuance_ids.get(issuance_id).copied() else {
            return;
        };
        let Some(record) = state.records.iter_mut().find(|record| record.id == id) else {
            return;
        };
        if record.status == status
            || record.status.is_final()
            || matches!(status, CredentialIssuanceActivityStatus::Pending)
        {
            return;
        }
        if matches!(
            record.status,
            CredentialIssuanceActivityStatus::TimedOut
                | CredentialIssuanceActivityStatus::OutcomeUnknown
        ) && matches!(
            status,
            CredentialIssuanceActivityStatus::TimedOut
                | CredentialIssuanceActivityStatus::OutcomeUnknown
                | CredentialIssuanceActivityStatus::Refused
                | CredentialIssuanceActivityStatus::Cancelled
        ) {
            return;
        }
        record.status = status;
        record.finality = status.into();
        record.observed_at_millis = Self::now();
    }
}

impl ListCredentialIssuanceActivityUseCase for CredentialIssuanceActivityStore {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<CredentialIssuanceActivityView, CredentialIssuanceActivityError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CredentialIssuanceActivityError::Unavailable)?;
        Ok(CredentialIssuanceActivityView {
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
    profile_id: ProtocolProfileId,
    preview: CredentialOfferPreview,
    state: CredentialIssuanceState,
    credential_id: Option<String>,
    failure_code: Option<String>,
    refusal_in_progress: bool,
    protocol_discarded: bool,
}

impl Session {
    fn view(&self, id: &CredentialIssuanceId) -> CredentialIssuanceView {
        CredentialIssuanceView {
            id: id.as_str().to_owned(),
            issuer: self.preview.issuer().to_owned(),
            configuration_ids: self.preview.configuration_ids().to_vec(),
            display_names: self.preview.display_names().to_vec(),
            state: self.state.as_str().to_owned(),
            credential_id: self.credential_id.clone(),
            failure_code: self.failure_code.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialIssuanceError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidIssuanceIdentifier(OpaqueIdError),
    InvalidOffer,
    InvalidHolder,
    ConfirmationRequired,
    InvalidConfirmation,
    NotFound,
    InvalidState,
    Approval(AcceptedFlowApprovalError),
    Protocol(IssuanceProtocolError),
    Sink(IssuedCredentialSinkError),
    Unavailable,
}

impl fmt::Display for CredentialIssuanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error) | Self::InvalidIssuanceIdentifier(error) => {
                error.fmt(formatter)
            }
            Self::InvalidOffer => formatter.write_str("credential offer input is invalid"),
            Self::InvalidHolder => formatter.write_str("credential holder selection is invalid"),
            Self::ConfirmationRequired => {
                formatter.write_str("credential issuance requires explicit consent")
            }
            Self::InvalidConfirmation => {
                formatter.write_str("credential issuance consent intent is invalid")
            }
            Self::NotFound => formatter.write_str("credential issuance session was not found"),
            Self::InvalidState => formatter.write_str("credential issuance state is invalid"),
            Self::Approval(error) => error.fmt(formatter),
            Self::Protocol(error) => error.fmt(formatter),
            Self::Sink(error) => error.fmt(formatter),
            Self::Unavailable => formatter.write_str("credential issuance state is unavailable"),
        }
    }
}

impl Error for CredentialIssuanceError {}

pub trait PrepareCredentialIssuanceUseCase: Send + Sync {
    fn execute<'a>(&'a self, command: PrepareCredentialIssuanceCommand) -> IssuanceViewFuture<'a>;
}

pub trait AcceptCredentialIssuanceUseCase: Send + Sync {
    fn execute<'a>(&'a self, command: AcceptCredentialIssuanceCommand) -> IssuanceViewFuture<'a>;
}

pub trait RefuseCredentialIssuanceUseCase: Send + Sync {
    fn execute(
        &self,
        command: RefuseCredentialIssuanceCommand,
    ) -> Result<CredentialIssuanceView, CredentialIssuanceError>;
}

pub trait GetCredentialIssuanceUseCase: Send + Sync {
    fn execute(
        &self,
        query: CredentialIssuanceQuery,
    ) -> Result<CredentialIssuanceView, CredentialIssuanceError>;
}

pub trait ListCredentialIssuancesUseCase: Send + Sync {
    fn execute(
        &self,
        query: CredentialIssuanceProfileQuery,
    ) -> Result<Vec<CredentialIssuanceView>, CredentialIssuanceError>;
}

pub struct CredentialIssuanceService {
    protocol: Arc<dyn CredentialIssuanceProtocolPort>,
    sink: Arc<dyn IssuedCredentialSinkPort>,
    authority: Arc<dyn CredentialIssuanceAuthorityPort>,
    activity: Arc<CredentialIssuanceActivityStore>,
    sessions: Mutex<BTreeMap<CredentialIssuanceId, Session>>,
}

/// Restores a recoverable session state if an issuance future is dropped or
/// unwinds after admission. The external protocol/sink outcome is unknown.
struct IssuanceAttempt<'a> {
    service: &'a CredentialIssuanceService,
    issuance_id: CredentialIssuanceId,
}

impl Drop for IssuanceAttempt<'_> {
    fn drop(&mut self) {
        self.service
            .interrupt_if_issuing(&self.issuance_id, ISSUANCE_INTERRUPTED_CODE);
    }
}

/// Releases a per-session refusal reservation even when the adapter panics.
/// The adapter is deliberately called without holding `sessions`.
struct RefusalAttempt<'a> {
    service: &'a CredentialIssuanceService,
    issuance_id: CredentialIssuanceId,
}

impl Drop for RefusalAttempt<'_> {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.service.sessions.lock()
            && let Some(session) = sessions.get_mut(&self.issuance_id)
        {
            session.refusal_in_progress = false;
        }
    }
}

impl CredentialIssuanceService {
    #[must_use]
    pub fn new(
        protocol: Arc<dyn CredentialIssuanceProtocolPort>,
        sink: Arc<dyn IssuedCredentialSinkPort>,
    ) -> Self {
        Self::with_authority_and_activity(
            protocol,
            sink,
            Arc::new(UnavailableCredentialIssuanceAuthority),
            Arc::new(CredentialIssuanceActivityStore::new()),
        )
    }

    #[must_use]
    pub fn with_authority(
        protocol: Arc<dyn CredentialIssuanceProtocolPort>,
        sink: Arc<dyn IssuedCredentialSinkPort>,
        authority: Arc<dyn CredentialIssuanceAuthorityPort>,
    ) -> Self {
        Self::with_authority_and_activity(
            protocol,
            sink,
            authority,
            Arc::new(CredentialIssuanceActivityStore::new()),
        )
    }

    #[must_use]
    pub fn with_authority_and_activity(
        protocol: Arc<dyn CredentialIssuanceProtocolPort>,
        sink: Arc<dyn IssuedCredentialSinkPort>,
        authority: Arc<dyn CredentialIssuanceAuthorityPort>,
        activity: Arc<CredentialIssuanceActivityStore>,
    ) -> Self {
        Self {
            protocol,
            sink,
            authority,
            activity,
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    #[must_use]
    pub fn activity(&self) -> Arc<CredentialIssuanceActivityStore> {
        Arc::clone(&self.activity)
    }

    fn sessions(
        &self,
    ) -> Result<MutexGuard<'_, BTreeMap<CredentialIssuanceId, Session>>, CredentialIssuanceError>
    {
        self.sessions
            .lock()
            .map_err(|_| CredentialIssuanceError::Unavailable)
    }

    fn fail_if_issuing(&self, id: &CredentialIssuanceId, code: &str) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.get_mut(id)
            && session.state == CredentialIssuanceState::Issuing
        {
            session.state = CredentialIssuanceState::Failed;
            session.failure_code = Some(code.to_owned());
            self.activity
                .update(id, CredentialIssuanceActivityStatus::Failed);
        }
    }

    fn unknown_if_issuing(&self, id: &CredentialIssuanceId, code: &str) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.get_mut(id)
            && session.state == CredentialIssuanceState::Issuing
        {
            session.state = CredentialIssuanceState::OutcomeUnknown;
            session.failure_code = Some(code.to_owned());
            self.activity
                .update(id, CredentialIssuanceActivityStatus::OutcomeUnknown);
        }
    }

    fn interrupt_if_issuing(&self, id: &CredentialIssuanceId, code: &str) {
        self.unknown_if_issuing(id, code);
    }
}

fn profile(value: String) -> Result<ProtocolProfileId, CredentialIssuanceError> {
    ProtocolProfileId::parse(value).map_err(CredentialIssuanceError::InvalidProfileIdentifier)
}

fn issuance_id(value: String) -> Result<CredentialIssuanceId, CredentialIssuanceError> {
    CredentialIssuanceId::parse(value).map_err(CredentialIssuanceError::InvalidIssuanceIdentifier)
}

fn valid_holder_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

impl PrepareCredentialIssuanceUseCase for CredentialIssuanceService {
    fn execute<'a>(&'a self, command: PrepareCredentialIssuanceCommand) -> IssuanceViewFuture<'a> {
        Box::pin(async move {
            let profile_id = profile(command.profile_id)?;
            if command.offer.is_empty() || command.offer.len() > MAX_CREDENTIAL_OFFER_BYTES {
                return Err(CredentialIssuanceError::InvalidOffer);
            }
            let prepared = self
                .protocol
                .prepare(PrepareIssuanceRequest {
                    profile_id: profile_id.clone(),
                    offer: command.offer,
                })
                .await
                .map_err(CredentialIssuanceError::Protocol)?;
            let session = Session {
                profile_id,
                preview: prepared.preview,
                state: CredentialIssuanceState::AwaitingConsent,
                credential_id: None,
                failure_code: None,
                refusal_in_progress: false,
                protocol_discarded: false,
            };
            let view = session.view(&prepared.id);
            if self.sessions()?.insert(prepared.id, session).is_some() {
                return Err(CredentialIssuanceError::InvalidState);
            }
            Ok(view)
        })
    }
}

impl AcceptCredentialIssuanceUseCase for CredentialIssuanceService {
    fn execute<'a>(&'a self, command: AcceptCredentialIssuanceCommand) -> IssuanceViewFuture<'a> {
        Box::pin(async move {
            if !command.confirmed {
                return Err(CredentialIssuanceError::ConfirmationRequired);
            }
            if command.intent != "ACCEPT_CREDENTIAL_ISSUANCE" {
                return Err(CredentialIssuanceError::InvalidConfirmation);
            }
            if !valid_holder_text(&command.holder_did, MAX_DID_CHARACTERS)
                || !valid_holder_text(&command.method_id, MAX_METHOD_CHARACTERS)
                || !valid_holder_text(&command.holder_binding_method_id, MAX_METHOD_CHARACTERS)
            {
                return Err(CredentialIssuanceError::InvalidHolder);
            }
            let profile_id = profile(command.profile_id)?;
            let issuance_id = issuance_id(command.issuance_id)?;
            let authority = {
                let mut sessions = self.sessions()?;
                let session = sessions
                    .get_mut(&issuance_id)
                    .ok_or(CredentialIssuanceError::NotFound)?;
                if session.profile_id != profile_id {
                    return Err(CredentialIssuanceError::NotFound);
                }
                if session.state != CredentialIssuanceState::AwaitingConsent
                    || session.refusal_in_progress
                {
                    return Err(CredentialIssuanceError::InvalidState);
                }
                let authority = self
                    .authority
                    .mint(CredentialIssuanceAuthorityRequest {
                        profile_id: profile_id.as_str().to_owned(),
                        holder_did: command.holder_did.clone(),
                        method_id: command.method_id.clone(),
                        flow_id: OID4VCI_CREDENTIAL_ISSUANCE_FLOW_ID,
                        session_id: issuance_id.as_str().to_owned(),
                    })
                    .map_err(CredentialIssuanceError::Approval)?;
                // Admission is durable within this process before protocol.issue can run.
                self.activity
                    .begin(&issuance_id, session)
                    .ok_or(CredentialIssuanceError::Unavailable)?;
                session.state = CredentialIssuanceState::Issuing;
                authority
            };
            let _interrupted_attempt = IssuanceAttempt {
                service: self,
                issuance_id: issuance_id.clone(),
            };
            let issued = match self
                .protocol
                .issue(ProtocolIssueRequest {
                    profile_id: profile_id.clone(),
                    issuance_id: issuance_id.clone(),
                    holder_did: command.holder_did,
                    method_id: command.method_id,
                    holder_binding_method_id: command.holder_binding_method_id,
                    authority,
                })
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    if error == IssuanceProtocolError::Unavailable {
                        self.unknown_if_issuing(&issuance_id, error.code());
                    } else {
                        self.fail_if_issuing(&issuance_id, error.code());
                    }
                    return Err(CredentialIssuanceError::Protocol(error));
                }
            };
            let stored = match self
                .sink
                .store_verified(StoreIssuedCredentialRequest {
                    profile_id,
                    signed_bytes: issued.signed_bytes,
                    detached_proof: issued.detached_proof,
                    private_material: issued.private_material,
                })
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    let code = match error {
                        IssuedCredentialSinkError::Unavailable => "credential_store_unavailable",
                        IssuedCredentialSinkError::InvalidCredential => "invalid_credential",
                        IssuedCredentialSinkError::VerificationFailed => {
                            "credential_verification_failed"
                        }
                        IssuedCredentialSinkError::PersistenceFailed => {
                            "credential_persistence_failed"
                        }
                    };
                    if matches!(
                        error,
                        IssuedCredentialSinkError::Unavailable
                            | IssuedCredentialSinkError::PersistenceFailed
                    ) {
                        self.unknown_if_issuing(&issuance_id, code);
                    } else {
                        self.fail_if_issuing(&issuance_id, code);
                    }
                    return Err(CredentialIssuanceError::Sink(error));
                }
            };
            let mut sessions = self.sessions()?;
            let session = sessions
                .get_mut(&issuance_id)
                .ok_or(CredentialIssuanceError::NotFound)?;
            if !matches!(
                session.state,
                CredentialIssuanceState::Issuing | CredentialIssuanceState::OutcomeUnknown
            ) {
                return Err(CredentialIssuanceError::InvalidState);
            }
            session.state = CredentialIssuanceState::Succeeded;
            session.credential_id = Some(stored.credential_id);
            session.failure_code = None;
            self.activity
                .update(&issuance_id, CredentialIssuanceActivityStatus::Stored);
            Ok(session.view(&issuance_id))
        })
    }
}

impl RefuseCredentialIssuanceUseCase for CredentialIssuanceService {
    fn execute(
        &self,
        command: RefuseCredentialIssuanceCommand,
    ) -> Result<CredentialIssuanceView, CredentialIssuanceError> {
        let profile_id = profile(command.profile_id)?;
        let issuance_id = issuance_id(command.issuance_id)?;
        {
            let mut sessions = self.sessions()?;
            let session = sessions
                .get_mut(&issuance_id)
                .ok_or(CredentialIssuanceError::NotFound)?;
            if session.profile_id != profile_id {
                return Err(CredentialIssuanceError::NotFound);
            }
            if session.protocol_discarded {
                return Ok(session.view(&issuance_id));
            }
            if !matches!(
                session.state,
                CredentialIssuanceState::AwaitingConsent
                    | CredentialIssuanceState::Failed
                    | CredentialIssuanceState::OutcomeUnknown
            ) || session.refusal_in_progress
            {
                return Err(CredentialIssuanceError::InvalidState);
            }
            session.refusal_in_progress = true;
        }
        let refusal_attempt = RefusalAttempt {
            service: self,
            issuance_id: issuance_id.clone(),
        };
        self.protocol
            .discard(&issuance_id)
            .map_err(CredentialIssuanceError::Protocol)?;

        let mut sessions = self.sessions()?;
        let session = sessions
            .get_mut(&issuance_id)
            .ok_or(CredentialIssuanceError::NotFound)?;
        session.protocol_discarded = true;
        // Discard only changes an unaccepted offer to refused. It cannot
        // rewrite an earlier failed or uncertain issuance outcome.
        if session.state == CredentialIssuanceState::AwaitingConsent {
            session.state = CredentialIssuanceState::Refused;
            session.failure_code = None;
        }
        // Activity is observational: lack of an evictable slot must not make
        // refusal unavailable. It is only admitted after discard succeeds.
        let _ = self.activity.begin(&issuance_id, session);
        self.activity
            .update(&issuance_id, CredentialIssuanceActivityStatus::Refused);
        let view = session.view(&issuance_id);
        drop(sessions);
        drop(refusal_attempt);
        Ok(view)
    }
}

impl GetCredentialIssuanceUseCase for CredentialIssuanceService {
    fn execute(
        &self,
        query: CredentialIssuanceQuery,
    ) -> Result<CredentialIssuanceView, CredentialIssuanceError> {
        let profile_id = profile(query.profile_id)?;
        let issuance_id = issuance_id(query.issuance_id)?;
        let sessions = self.sessions()?;
        let session = sessions
            .get(&issuance_id)
            .filter(|session| session.profile_id == profile_id)
            .ok_or(CredentialIssuanceError::NotFound)?;
        Ok(session.view(&issuance_id))
    }
}

impl ListCredentialIssuancesUseCase for CredentialIssuanceService {
    fn execute(
        &self,
        query: CredentialIssuanceProfileQuery,
    ) -> Result<Vec<CredentialIssuanceView>, CredentialIssuanceError> {
        let profile_id = profile(query.profile_id)?;
        Ok(self
            .sessions()?
            .iter()
            .filter(|(_, session)| session.profile_id == profile_id)
            .map(|(id, session)| session.view(id))
            .collect())
    }
}

impl ListCredentialIssuanceActivityUseCase for CredentialIssuanceService {
    fn execute(
        &self,
        profile_id: String,
    ) -> Result<CredentialIssuanceActivityView, CredentialIssuanceActivityError> {
        self.activity.execute(profile_id)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCredentialIssuanceProtocol;

impl CredentialIssuanceProtocolPort for UnavailableCredentialIssuanceProtocol {
    fn prepare<'a>(&'a self, _: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
        Box::pin(async { Err(IssuanceProtocolError::Unavailable) })
    }

    fn issue<'a>(&'a self, _: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a> {
        Box::pin(async { Err(IssuanceProtocolError::Unavailable) })
    }

    fn discard(&self, _: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError> {
        Err(IssuanceProtocolError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableIssuedCredentialSink;

impl IssuedCredentialSinkPort for UnavailableIssuedCredentialSink {
    fn store_verified<'a>(
        &'a self,
        _: StoreIssuedCredentialRequest,
    ) -> StoreIssuedCredentialFuture<'a> {
        Box::pin(async { Err(IssuedCredentialSinkError::Unavailable) })
    }
}

pub type PrepareSelfIssuedAuthenticationPortFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PreparedSelfIssuedAuthentication, SelfIssuedProtocolError>>
            + Send
            + 'a,
    >,
>;
pub type AuthenticateSelfIssuedPortFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), SelfIssuedProtocolError>> + Send + 'a>>;
pub trait SelfIssuedProofJwt: Send {
    fn as_str(&self) -> &str;
}

pub type SelfIssuedProofFuture<'a> = Pin<
    Box<dyn Future<Output = Result<Box<dyn SelfIssuedProofJwt>, SelfIssuedProofError>> + Send + 'a>,
>;
pub type SelfIssuedAuthenticationViewFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<SelfIssuedAuthenticationView, SelfIssuedAuthenticationError>>
            + Send
            + 'a,
    >,
>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareSelfIssuedAuthenticationRequest {
    pub profile_id: ProtocolProfileId,
    pub request: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedSelfIssuedAuthentication {
    pub id: SelfIssuedAuthenticationId,
    pub preview: SelfIssuedAuthenticationPreview,
}

pub struct ProtocolSelfIssuedAuthenticationRequest {
    pub profile_id: ProtocolProfileId,
    pub authentication_id: SelfIssuedAuthenticationId,
    pub holder_did: String,
    pub method_id: String,
    pub authority: AcceptedSelfIssuedAuthenticationFlow,
}

impl fmt::Debug for ProtocolSelfIssuedAuthenticationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProtocolSelfIssuedAuthenticationRequest")
            .field("profile_id", &self.profile_id)
            .field("authentication_id", &self.authentication_id)
            .field("holder_did", &self.holder_did)
            .field("method_id", &self.method_id)
            .field("authority", &"[REDACTED]")
            .finish()
    }
}

pub trait SelfIssuedAuthenticationProtocolPort: Send + Sync {
    fn prepare<'a>(
        &'a self,
        request: PrepareSelfIssuedAuthenticationRequest,
    ) -> PrepareSelfIssuedAuthenticationPortFuture<'a>;

    fn authenticate<'a>(
        &'a self,
        request: ProtocolSelfIssuedAuthenticationRequest,
    ) -> AuthenticateSelfIssuedPortFuture<'a>;

    fn discard(
        &self,
        authentication_id: &SelfIssuedAuthenticationId,
    ) -> Result<(), SelfIssuedProtocolError>;
}

pub struct SelfIssuedProofRequest<'a> {
    pub profile_id: ProtocolProfileId,
    pub holder_did: String,
    pub method_id: String,
    pub audience: String,
    pub nonce: &'a str,
    pub issued_at_seconds: u64,
    pub expires_at_seconds: u64,
    pub flow_id: &'static str,
    pub session_id: String,
    pub authority: AcceptedSelfIssuedAuthenticationFlow,
}

impl fmt::Debug for SelfIssuedProofRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelfIssuedProofRequest")
            .field("profile_id", &self.profile_id)
            .field("holder_did", &self.holder_did)
            .field("method_id", &self.method_id)
            .field("audience", &self.audience)
            .field("nonce", &"[REDACTED]")
            .field("issued_at_seconds", &self.issued_at_seconds)
            .field("expires_at_seconds", &self.expires_at_seconds)
            .field("flow_id", &self.flow_id)
            .field("session_id", &self.session_id)
            .field("authority", &"[REDACTED]")
            .finish()
    }
}

pub trait SelfIssuedIdentityProofPort: Send + Sync {
    fn create<'a>(&'a self, request: SelfIssuedProofRequest<'a>) -> SelfIssuedProofFuture<'a>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfIssuedProtocolError {
    Unavailable,
    InvalidRequest,
    UnsupportedRequest,
    InvalidVerifier,
    RequestExpired,
    InvalidProof,
    VerifierRejected,
    ProtectionUnavailable,
    WalletLocked,
}

impl SelfIssuedProtocolError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "protocol_unavailable",
            Self::InvalidRequest => "invalid_request",
            Self::UnsupportedRequest => "unsupported_request",
            Self::InvalidVerifier => "invalid_verifier",
            Self::RequestExpired => "request_expired",
            Self::InvalidProof => "invalid_proof",
            Self::VerifierRejected => "verifier_rejected",
            Self::ProtectionUnavailable => "protection_unavailable",
            Self::WalletLocked => "wallet_locked",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfIssuedProofError {
    Unavailable,
    DidNotFound,
    MethodNotFound,
    MethodNotAuthorized,
    UnsupportedAlgorithm,
    WalletLocked,
    Rejected,
}

display_code_error!(SelfIssuedProtocolError, SelfIssuedProtocolError::code);

impl fmt::Display for SelfIssuedProofError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "self-issued proof is unavailable",
            Self::DidNotFound => "self-issued subject DID was not found",
            Self::MethodNotFound => "self-issued authentication method was not found",
            Self::MethodNotAuthorized => "self-issued method is not authorized for authentication",
            Self::UnsupportedAlgorithm => "self-issued proof algorithm is unsupported",
            Self::WalletLocked => "wallet must be unlocked for self-issued authentication",
            Self::Rejected => "self-issued proof was rejected",
        })
    }
}

impl Error for SelfIssuedProofError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareSelfIssuedAuthenticationCommand {
    pub profile_id: String,
    pub request: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptSelfIssuedAuthenticationCommand {
    pub profile_id: String,
    pub authentication_id: String,
    pub holder_did: String,
    pub method_id: String,
    pub confirmed: bool,
    pub intent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefuseSelfIssuedAuthenticationCommand {
    pub profile_id: String,
    pub authentication_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfIssuedAuthenticationQuery {
    pub profile_id: String,
    pub authentication_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfIssuedAuthenticationProfileQuery {
    pub profile_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfIssuedAuthenticationView {
    pub id: String,
    pub verifier: String,
    pub purpose: String,
    pub state: String,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug)]
struct SelfIssuedSession {
    profile_id: ProtocolProfileId,
    preview: SelfIssuedAuthenticationPreview,
    state: SelfIssuedAuthenticationState,
    failure_code: Option<String>,
}

impl SelfIssuedSession {
    fn view(&self, id: &SelfIssuedAuthenticationId) -> SelfIssuedAuthenticationView {
        SelfIssuedAuthenticationView {
            id: id.as_str().to_owned(),
            verifier: self.preview.verifier().to_owned(),
            purpose: self.preview.purpose().to_owned(),
            state: self.state.as_str().to_owned(),
            failure_code: self.failure_code.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelfIssuedAuthenticationError {
    InvalidProfileIdentifier(OpaqueIdError),
    InvalidAuthenticationIdentifier(OpaqueIdError),
    InvalidRequest,
    InvalidHolder,
    ConfirmationRequired,
    InvalidConfirmation,
    NotFound,
    InvalidState,
    Approval(AcceptedFlowApprovalError),
    Protocol(SelfIssuedProtocolError),
    Unavailable,
}

impl fmt::Display for SelfIssuedAuthenticationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfileIdentifier(error)
            | Self::InvalidAuthenticationIdentifier(error) => error.fmt(formatter),
            Self::InvalidRequest => formatter.write_str("self-issued request input is invalid"),
            Self::InvalidHolder => formatter.write_str("self-issued holder selection is invalid"),
            Self::ConfirmationRequired => {
                formatter.write_str("self-issued authentication requires explicit consent")
            }
            Self::InvalidConfirmation => {
                formatter.write_str("self-issued authentication consent intent is invalid")
            }
            Self::NotFound => formatter.write_str("self-issued authentication was not found"),
            Self::InvalidState => {
                formatter.write_str("self-issued authentication state is invalid")
            }
            Self::Approval(error) => error.fmt(formatter),
            Self::Protocol(error) => error.fmt(formatter),
            Self::Unavailable => {
                formatter.write_str("self-issued authentication state is unavailable")
            }
        }
    }
}

impl Error for SelfIssuedAuthenticationError {}

pub trait PrepareSelfIssuedAuthenticationUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: PrepareSelfIssuedAuthenticationCommand,
    ) -> SelfIssuedAuthenticationViewFuture<'a>;
}

pub trait AcceptSelfIssuedAuthenticationUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: AcceptSelfIssuedAuthenticationCommand,
    ) -> SelfIssuedAuthenticationViewFuture<'a>;
}

pub trait RefuseSelfIssuedAuthenticationUseCase: Send + Sync {
    fn execute(
        &self,
        command: RefuseSelfIssuedAuthenticationCommand,
    ) -> Result<SelfIssuedAuthenticationView, SelfIssuedAuthenticationError>;
}

pub trait GetSelfIssuedAuthenticationUseCase: Send + Sync {
    fn execute(
        &self,
        query: SelfIssuedAuthenticationQuery,
    ) -> Result<SelfIssuedAuthenticationView, SelfIssuedAuthenticationError>;
}

pub trait ListSelfIssuedAuthenticationsUseCase: Send + Sync {
    fn execute(
        &self,
        query: SelfIssuedAuthenticationProfileQuery,
    ) -> Result<Vec<SelfIssuedAuthenticationView>, SelfIssuedAuthenticationError>;
}

pub struct SelfIssuedAuthenticationService {
    protocol: Arc<dyn SelfIssuedAuthenticationProtocolPort>,
    authority: Arc<dyn SelfIssuedAuthenticationAuthorityPort>,
    sessions: Mutex<BTreeMap<SelfIssuedAuthenticationId, SelfIssuedSession>>,
}

impl SelfIssuedAuthenticationService {
    #[must_use]
    pub fn new(protocol: Arc<dyn SelfIssuedAuthenticationProtocolPort>) -> Self {
        Self {
            protocol,
            authority: Arc::new(UnavailableSelfIssuedAuthenticationAuthority),
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    #[must_use]
    pub fn with_authority(
        protocol: Arc<dyn SelfIssuedAuthenticationProtocolPort>,
        authority: Arc<dyn SelfIssuedAuthenticationAuthorityPort>,
    ) -> Self {
        Self {
            protocol,
            authority,
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    fn sessions(
        &self,
    ) -> Result<
        MutexGuard<'_, BTreeMap<SelfIssuedAuthenticationId, SelfIssuedSession>>,
        SelfIssuedAuthenticationError,
    > {
        self.sessions
            .lock()
            .map_err(|_| SelfIssuedAuthenticationError::Unavailable)
    }

    fn fail(&self, id: &SelfIssuedAuthenticationId, code: &str) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.get_mut(id)
        {
            session.state = SelfIssuedAuthenticationState::Failed;
            session.failure_code = Some(code.to_owned());
        }
    }
}

fn authentication_profile(
    value: String,
) -> Result<ProtocolProfileId, SelfIssuedAuthenticationError> {
    ProtocolProfileId::parse(value).map_err(SelfIssuedAuthenticationError::InvalidProfileIdentifier)
}

fn authentication_id(
    value: String,
) -> Result<SelfIssuedAuthenticationId, SelfIssuedAuthenticationError> {
    SelfIssuedAuthenticationId::parse(value)
        .map_err(SelfIssuedAuthenticationError::InvalidAuthenticationIdentifier)
}

impl PrepareSelfIssuedAuthenticationUseCase for SelfIssuedAuthenticationService {
    fn execute<'a>(
        &'a self,
        command: PrepareSelfIssuedAuthenticationCommand,
    ) -> SelfIssuedAuthenticationViewFuture<'a> {
        Box::pin(async move {
            let profile_id = authentication_profile(command.profile_id)?;
            if command.request.is_empty() || command.request.len() > MAX_SELF_ISSUED_REQUEST_BYTES {
                return Err(SelfIssuedAuthenticationError::InvalidRequest);
            }
            let prepared = self
                .protocol
                .prepare(PrepareSelfIssuedAuthenticationRequest {
                    profile_id: profile_id.clone(),
                    request: command.request,
                })
                .await
                .map_err(SelfIssuedAuthenticationError::Protocol)?;
            let session = SelfIssuedSession {
                profile_id,
                preview: prepared.preview,
                state: SelfIssuedAuthenticationState::AwaitingConsent,
                failure_code: None,
            };
            let view = session.view(&prepared.id);
            if self.sessions()?.insert(prepared.id, session).is_some() {
                return Err(SelfIssuedAuthenticationError::InvalidState);
            }
            Ok(view)
        })
    }
}

impl AcceptSelfIssuedAuthenticationUseCase for SelfIssuedAuthenticationService {
    fn execute<'a>(
        &'a self,
        command: AcceptSelfIssuedAuthenticationCommand,
    ) -> SelfIssuedAuthenticationViewFuture<'a> {
        Box::pin(async move {
            if !command.confirmed {
                return Err(SelfIssuedAuthenticationError::ConfirmationRequired);
            }
            if command.intent != "ACCEPT_SELF_ISSUED_AUTHENTICATION" {
                return Err(SelfIssuedAuthenticationError::InvalidConfirmation);
            }
            if !valid_holder_text(&command.holder_did, MAX_DID_CHARACTERS)
                || !valid_holder_text(&command.method_id, MAX_METHOD_CHARACTERS)
            {
                return Err(SelfIssuedAuthenticationError::InvalidHolder);
            }
            let profile_id = authentication_profile(command.profile_id)?;
            let authentication_id = authentication_id(command.authentication_id)?;
            let authority = {
                let mut sessions = self.sessions()?;
                let session = sessions
                    .get_mut(&authentication_id)
                    .ok_or(SelfIssuedAuthenticationError::NotFound)?;
                if session.profile_id != profile_id {
                    return Err(SelfIssuedAuthenticationError::NotFound);
                }
                if session.state != SelfIssuedAuthenticationState::AwaitingConsent {
                    return Err(SelfIssuedAuthenticationError::InvalidState);
                }
                let authority = self
                    .authority
                    .mint(SelfIssuedAuthenticationAuthorityRequest {
                        profile_id: profile_id.as_str().to_owned(),
                        holder_did: command.holder_did.clone(),
                        method_id: command.method_id.clone(),
                        flow_id: SIOPV2_SELF_ISSUED_AUTHENTICATION_FLOW_ID,
                        session_id: authentication_id.as_str().to_owned(),
                    })
                    .map_err(SelfIssuedAuthenticationError::Approval)?;
                session.state = SelfIssuedAuthenticationState::Authenticating;
                authority
            };
            if let Err(error) = self
                .protocol
                .authenticate(ProtocolSelfIssuedAuthenticationRequest {
                    profile_id,
                    authentication_id: authentication_id.clone(),
                    holder_did: command.holder_did,
                    method_id: command.method_id,
                    authority,
                })
                .await
            {
                self.fail(&authentication_id, error.code());
                return Err(SelfIssuedAuthenticationError::Protocol(error));
            }
            let mut sessions = self.sessions()?;
            let session = sessions
                .get_mut(&authentication_id)
                .ok_or(SelfIssuedAuthenticationError::NotFound)?;
            session.state = SelfIssuedAuthenticationState::Succeeded;
            session.failure_code = None;
            Ok(session.view(&authentication_id))
        })
    }
}

impl RefuseSelfIssuedAuthenticationUseCase for SelfIssuedAuthenticationService {
    fn execute(
        &self,
        command: RefuseSelfIssuedAuthenticationCommand,
    ) -> Result<SelfIssuedAuthenticationView, SelfIssuedAuthenticationError> {
        let profile_id = authentication_profile(command.profile_id)?;
        let authentication_id = authentication_id(command.authentication_id)?;
        {
            let sessions = self.sessions()?;
            let session = sessions
                .get(&authentication_id)
                .ok_or(SelfIssuedAuthenticationError::NotFound)?;
            if session.profile_id != profile_id {
                return Err(SelfIssuedAuthenticationError::NotFound);
            }
            if session.state != SelfIssuedAuthenticationState::AwaitingConsent {
                return Err(SelfIssuedAuthenticationError::InvalidState);
            }
        }
        self.protocol
            .discard(&authentication_id)
            .map_err(SelfIssuedAuthenticationError::Protocol)?;
        let mut sessions = self.sessions()?;
        let session = sessions
            .get_mut(&authentication_id)
            .ok_or(SelfIssuedAuthenticationError::NotFound)?;
        session.state = SelfIssuedAuthenticationState::Refused;
        Ok(session.view(&authentication_id))
    }
}

impl GetSelfIssuedAuthenticationUseCase for SelfIssuedAuthenticationService {
    fn execute(
        &self,
        query: SelfIssuedAuthenticationQuery,
    ) -> Result<SelfIssuedAuthenticationView, SelfIssuedAuthenticationError> {
        let profile_id = authentication_profile(query.profile_id)?;
        let authentication_id = authentication_id(query.authentication_id)?;
        let sessions = self.sessions()?;
        let session = sessions
            .get(&authentication_id)
            .filter(|session| session.profile_id == profile_id)
            .ok_or(SelfIssuedAuthenticationError::NotFound)?;
        Ok(session.view(&authentication_id))
    }
}

impl ListSelfIssuedAuthenticationsUseCase for SelfIssuedAuthenticationService {
    fn execute(
        &self,
        query: SelfIssuedAuthenticationProfileQuery,
    ) -> Result<Vec<SelfIssuedAuthenticationView>, SelfIssuedAuthenticationError> {
        let profile_id = authentication_profile(query.profile_id)?;
        Ok(self
            .sessions()?
            .iter()
            .filter(|(_, session)| session.profile_id == profile_id)
            .map(|(id, session)| session.view(id))
            .collect())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableSelfIssuedAuthenticationProtocol;

impl SelfIssuedAuthenticationProtocolPort for UnavailableSelfIssuedAuthenticationProtocol {
    fn prepare<'a>(
        &'a self,
        _: PrepareSelfIssuedAuthenticationRequest,
    ) -> PrepareSelfIssuedAuthenticationPortFuture<'a> {
        Box::pin(async { Err(SelfIssuedProtocolError::Unavailable) })
    }

    fn authenticate<'a>(
        &'a self,
        _: ProtocolSelfIssuedAuthenticationRequest,
    ) -> AuthenticateSelfIssuedPortFuture<'a> {
        Box::pin(async { Err(SelfIssuedProtocolError::Unavailable) })
    }

    fn discard(&self, _: &SelfIssuedAuthenticationId) -> Result<(), SelfIssuedProtocolError> {
        Err(SelfIssuedProtocolError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const HOLDER_DID: &str =
        "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const REJECT_DID: &str =
        "did:midnight:undeployed:1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const UNAVAILABLE_DID: &str =
        "did:midnight:undeployed:2123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    struct TestIssuanceAuthority(
        oxid_foundation::AcceptedFlowIssuer<{ oxid_foundation::CREDENTIAL_ISSUANCE_FLOW_KIND }>,
    );

    impl CredentialIssuanceAuthorityPort for TestIssuanceAuthority {
        fn mint(
            &self,
            request: CredentialIssuanceAuthorityRequest,
        ) -> Result<AcceptedCredentialIssuanceFlow, AcceptedFlowApprovalError> {
            Ok(self.0.mint(
                request,
                oxid_foundation::UnixTimestampMillis::new(1),
                oxid_foundation::UnixTimestampMillis::new(2),
                0,
            ))
        }
    }

    fn issuance_authority() -> Arc<TestIssuanceAuthority> {
        Arc::new(TestIssuanceAuthority(
            oxid_foundation::AcceptedFlowIssuer::new(),
        ))
    }

    struct TestAuthenticationAuthority(
        oxid_foundation::AcceptedFlowIssuer<
            { oxid_foundation::SELF_ISSUED_AUTHENTICATION_FLOW_KIND },
        >,
    );

    impl SelfIssuedAuthenticationAuthorityPort for TestAuthenticationAuthority {
        fn mint(
            &self,
            request: SelfIssuedAuthenticationAuthorityRequest,
        ) -> Result<AcceptedSelfIssuedAuthenticationFlow, AcceptedFlowApprovalError> {
            Ok(self.0.mint(
                request,
                oxid_foundation::UnixTimestampMillis::new(1),
                oxid_foundation::UnixTimestampMillis::new(2),
                0,
            ))
        }
    }

    fn authentication_authority() -> Arc<TestAuthenticationAuthority> {
        Arc::new(TestAuthenticationAuthority(
            oxid_foundation::AcceptedFlowIssuer::new(),
        ))
    }

    struct PendingHolderProof;

    impl CredentialHolderProofPort for PendingHolderProof {
        fn create<'a>(&'a self, request: HolderProofRequest<'a>) -> HolderProofFuture<'a> {
            Box::pin(async move {
                let _request = request;
                std::future::pending().await
            })
        }
    }

    #[test]
    fn holder_proof_nonce_is_borrowed_and_redacted_when_futures_are_dropped() {
        let nonce = "sensitive-nonce";
        let authority = issuance_authority();
        let method_id = format!("{HOLDER_DID}#key-1");
        let request = HolderProofRequest {
            profile_id: ProtocolProfileId::parse("profile_test").expect("profile"),
            holder_did: HOLDER_DID.to_owned(),
            method_id: method_id.clone(),
            audience: "https://issuer.example".to_owned(),
            nonce,
            flow_id: OID4VCI_CREDENTIAL_ISSUANCE_FLOW_ID,
            session_id: "issuance_test".to_owned(),
            authority: authority
                .mint(CredentialIssuanceAuthorityRequest {
                    profile_id: "profile_test".to_owned(),
                    holder_did: HOLDER_DID.to_owned(),
                    method_id,
                    flow_id: OID4VCI_CREDENTIAL_ISSUANCE_FLOW_ID,
                    session_id: "issuance_test".to_owned(),
                })
                .expect("authority"),
        };
        let request_debug = format!("{request:?}");
        assert!(request_debug.contains("[REDACTED]"));
        assert!(!request_debug.contains("sensitive-nonce"));

        let future = PendingHolderProof.create(request);
        drop(future);
    }

    struct PendingSelfIssuedProof;

    impl SelfIssuedIdentityProofPort for PendingSelfIssuedProof {
        fn create<'a>(&'a self, request: SelfIssuedProofRequest<'a>) -> SelfIssuedProofFuture<'a> {
            Box::pin(async move {
                let _request = request;
                std::future::pending().await
            })
        }
    }

    #[test]
    fn self_issued_proof_nonce_is_borrowed_and_redacted_when_future_is_dropped() {
        let nonce = "sensitive-nonce";
        let authority = authentication_authority();
        let method_id = format!("{HOLDER_DID}#key-1");
        let request = SelfIssuedProofRequest {
            profile_id: ProtocolProfileId::parse("profile_test").expect("profile"),
            holder_did: HOLDER_DID.to_owned(),
            method_id: method_id.clone(),
            audience: "https://verifier.example".to_owned(),
            nonce,
            issued_at_seconds: 1,
            expires_at_seconds: 2,
            flow_id: SIOPV2_SELF_ISSUED_AUTHENTICATION_FLOW_ID,
            session_id: "authentication_test".to_owned(),
            authority: authority
                .mint(SelfIssuedAuthenticationAuthorityRequest {
                    profile_id: "profile_test".to_owned(),
                    holder_did: HOLDER_DID.to_owned(),
                    method_id,
                    flow_id: SIOPV2_SELF_ISSUED_AUTHENTICATION_FLOW_ID,
                    session_id: "authentication_test".to_owned(),
                })
                .expect("authority"),
        };
        let request_debug = format!("{request:?}");
        assert!(request_debug.contains("[REDACTED]"));
        assert!(!request_debug.contains(nonce));

        let future = PendingSelfIssuedProof.create(request);
        drop(future);
    }

    struct RoutingPort;

    impl IdentityRequestRouterPort for RoutingPort {
        fn route(
            &self,
            request_uri: &str,
        ) -> Result<IdentityRequestKind, IdentityRequestRoutingError> {
            assert_eq!(
                request_uri,
                "openid-credential-offer://?credential_offer=%7B%7D"
            );
            Ok(IdentityRequestKind::CredentialIssuance)
        }
    }

    #[test]
    fn identity_request_routing_bounds_input_and_redacts_debug_output() {
        let service = IdentityRequestRoutingService::new(Arc::new(RoutingPort));
        let request_uri = "openid-credential-offer://?credential_offer=%7B%7D".to_owned();
        let command = RouteIdentityRequestCommand {
            request_uri: request_uri.clone(),
        };
        let debug = format!("{command:?}");
        assert!(debug.contains("request_uri_length"));
        assert!(!debug.contains("credential_offer"));
        assert_eq!(
            service.execute(command),
            Ok(IdentityRequestKind::CredentialIssuance)
        );
        assert_eq!(
            service.execute(RouteIdentityRequestCommand {
                request_uri: format!("{}\n", request_uri),
            }),
            Err(IdentityRequestRoutingError::InvalidRequest)
        );
        assert_eq!(
            service.execute(RouteIdentityRequestCommand {
                request_uri: "x".repeat(MAX_IDENTITY_REQUEST_URI_BYTES + 1),
            }),
            Err(IdentityRequestRoutingError::InvalidRequest)
        );
    }

    #[test]
    fn issued_credential_debug_output_redacts_all_bytes() {
        let issued = IssuedCredentialBytes {
            signed_bytes: b"signed-credential-secret".to_vec(),
            detached_proof: Some(b"detached-proof".to_vec()),
            private_material: Some(b"opening-secret".to_vec()),
        };
        let debug = format!("{issued:?}");
        assert!(debug.contains("signed_bytes_length"));
        assert!(debug.contains("detached_proof_length"));
        assert!(debug.contains("private_material_length"));
        assert!(!debug.contains("signed-credential-secret"));
        assert!(!debug.contains("detached-proof"));
        assert!(!debug.contains("opening-secret"));
    }

    struct Protocol;

    impl CredentialIssuanceProtocolPort for Protocol {
        fn prepare<'a>(&'a self, request: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
            Box::pin(async move {
                if request.offer == "reject" {
                    return Err(IssuanceProtocolError::InvalidOffer);
                }
                Ok(PreparedCredentialOffer {
                    id: CredentialIssuanceId::parse("issuance_1").expect("valid fixture id"),
                    preview: CredentialOfferPreview::new(
                        "https://issuer.example",
                        vec!["identity".to_owned()],
                        vec!["Identity credential".to_owned()],
                    )
                    .expect("valid preview"),
                })
            })
        }

        fn issue<'a>(&'a self, request: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a> {
            Box::pin(async move {
                if request.holder_did == REJECT_DID {
                    Err(IssuanceProtocolError::InvalidProof)
                } else if request.holder_did == UNAVAILABLE_DID {
                    Err(IssuanceProtocolError::Unavailable)
                } else {
                    Ok(IssuedCredentialBytes {
                        signed_bytes: vec![1, 2, 3],
                        detached_proof: Some(vec![4, 5, 6]),
                        private_material: None,
                    })
                }
            })
        }

        fn discard(&self, _: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError> {
            Ok(())
        }
    }

    struct Sink;

    impl IssuedCredentialSinkPort for Sink {
        fn store_verified<'a>(
            &'a self,
            request: StoreIssuedCredentialRequest,
        ) -> StoreIssuedCredentialFuture<'a> {
            Box::pin(async move {
                assert_eq!(request.signed_bytes, [1, 2, 3]);
                assert_eq!(request.detached_proof, Some(vec![4, 5, 6]));
                Ok(StoredCredential {
                    credential_id: "vc_1".to_owned(),
                })
            })
        }
    }

    fn service() -> CredentialIssuanceService {
        CredentialIssuanceService::with_authority(
            Arc::new(Protocol),
            Arc::new(Sink),
            issuance_authority(),
        )
    }

    fn prepare(service: &CredentialIssuanceService) -> CredentialIssuanceView {
        futures_lite(service.prepare_for_test())
    }

    fn futures_lite<T>(future: impl Future<Output = T>) -> T {
        std::task::Waker::noop().wake_by_ref();
        let mut future = std::pin::pin!(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
            std::thread::yield_now();
        }
    }

    impl CredentialIssuanceService {
        async fn prepare_for_test(&self) -> CredentialIssuanceView {
            PrepareCredentialIssuanceUseCase::execute(
                self,
                PrepareCredentialIssuanceCommand {
                    profile_id: "profile_1".to_owned(),
                    offer: "offer".to_owned(),
                },
            )
            .await
            .expect("prepare should succeed")
        }
    }

    #[test]
    fn explicit_consent_issues_and_records_only_metadata() {
        let service = service();
        let prepared = prepare(&service);
        assert_eq!(prepared.state, "awaiting_consent");
        let issued = futures_lite(AcceptCredentialIssuanceUseCase::execute(
            &service,
            AcceptCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id,
                holder_did: HOLDER_DID.to_owned(),
                method_id: format!("{HOLDER_DID}#auth-1"),
                holder_binding_method_id: format!("{HOLDER_DID}#holder-jubjub-1"),
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
            },
        ))
        .expect("issuance should succeed");
        assert_eq!(issued.state, "succeeded");
        assert_eq!(issued.credential_id.as_deref(), Some("vc_1"));

        let activity =
            ListCredentialIssuanceActivityUseCase::execute(&service, "profile_1".to_owned())
                .expect("activity projection");
        assert_eq!(activity.source, "application_event_projection");
        assert_eq!(activity.retention, "process_local_bounded_not_backed_up");
        assert_eq!(activity.records.len(), 1);
        let record = &activity.records[0];
        assert_eq!(record.id.value(), 1);
        assert_eq!(record.source, CredentialIssuanceActivitySource::OpenId4Vci);
        assert_eq!(record.issuer, "https://issuer.example");
        assert_eq!(record.credential_configuration_ids, ["identity"]);
        assert_eq!(record.status, CredentialIssuanceActivityStatus::Stored);
        assert_eq!(record.finality, CredentialIssuanceActivityFinality::Final);
        assert!(record.observed_at_millis.is_some());
    }

    struct ActivityObservingProtocol {
        activity: Arc<CredentialIssuanceActivityStore>,
    }

    impl CredentialIssuanceProtocolPort for ActivityObservingProtocol {
        fn prepare<'a>(&'a self, _: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
            Box::pin(async {
                Ok(PreparedCredentialOffer {
                    id: CredentialIssuanceId::parse("issuance_activity")
                        .expect("valid fixture issuance id"),
                    preview: CredentialOfferPreview::new(
                        "https://issuer.example",
                        vec!["identity".to_owned()],
                        vec!["Identity credential".to_owned()],
                    )
                    .expect("valid preview"),
                })
            })
        }

        fn issue<'a>(&'a self, _: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a> {
            let activity = Arc::clone(&self.activity);
            Box::pin(async move {
                let records = ListCredentialIssuanceActivityUseCase::execute(
                    activity.as_ref(),
                    "profile_1".to_owned(),
                )
                .expect("activity is available");
                assert_eq!(records.records.len(), 1);
                assert_eq!(
                    records.records[0].status,
                    CredentialIssuanceActivityStatus::Pending
                );
                Ok(IssuedCredentialBytes {
                    signed_bytes: vec![1, 2, 3],
                    detached_proof: Some(vec![4, 5, 6]),
                    private_material: None,
                })
            })
        }

        fn discard(&self, _: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError> {
            Ok(())
        }
    }

    #[test]
    fn activity_is_admitted_before_protocol_issue_and_refusal_is_terminal() {
        let activity = Arc::new(CredentialIssuanceActivityStore::new());
        let issuance_service = CredentialIssuanceService::with_authority_and_activity(
            Arc::new(ActivityObservingProtocol {
                activity: Arc::clone(&activity),
            }),
            Arc::new(Sink),
            issuance_authority(),
            activity,
        );
        let prepared = prepare(&issuance_service);
        futures_lite(AcceptCredentialIssuanceUseCase::execute(
            &issuance_service,
            AcceptCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id,
                holder_did: HOLDER_DID.to_owned(),
                method_id: format!("{HOLDER_DID}#auth-1"),
                holder_binding_method_id: format!("{HOLDER_DID}#holder-jubjub-1"),
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
            },
        ))
        .expect("issuance succeeds");

        let refusal_service = service();
        let refused = prepare(&refusal_service);
        RefuseCredentialIssuanceUseCase::execute(
            &refusal_service,
            RefuseCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: refused.id,
            },
        )
        .expect("refusal succeeds");
        let records = ListCredentialIssuanceActivityUseCase::execute(
            &refusal_service,
            "profile_1".to_owned(),
        )
        .expect("activity projection");
        assert_eq!(
            records.records[0].status,
            CredentialIssuanceActivityStatus::Refused
        );
    }

    struct CountingProtocol(AtomicUsize);

    impl CredentialIssuanceProtocolPort for CountingProtocol {
        fn prepare<'a>(&'a self, _: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
            Box::pin(async {
                Ok(PreparedCredentialOffer {
                    id: CredentialIssuanceId::parse("issuance_no_authority").expect("issuance id"),
                    preview: CredentialOfferPreview::new(
                        "https://issuer.example",
                        vec!["identity".to_owned()],
                        vec!["Identity credential".to_owned()],
                    )
                    .expect("preview"),
                })
            })
        }

        fn issue<'a>(&'a self, _: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Box::pin(async { Err(IssuanceProtocolError::IssuerRejected) })
        }

        fn discard(&self, _: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError> {
            Ok(())
        }
    }

    struct CountingSink(AtomicUsize);
    impl IssuedCredentialSinkPort for CountingSink {
        fn store_verified<'a>(
            &'a self,
            _: StoreIssuedCredentialRequest,
        ) -> StoreIssuedCredentialFuture<'a> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Box::pin(async { Err(IssuedCredentialSinkError::Unavailable) })
        }
    }

    #[test]
    fn unavailable_authority_preserves_awaiting_consent_and_has_zero_effects() {
        let protocol = Arc::new(CountingProtocol(AtomicUsize::new(0)));
        let sink = Arc::new(CountingSink(AtomicUsize::new(0)));
        let service = CredentialIssuanceService::new(protocol.clone(), sink.clone());
        let prepared = prepare(&service);
        let result = futures_lite(AcceptCredentialIssuanceUseCase::execute(
            &service,
            AcceptCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id.clone(),
                holder_did: HOLDER_DID.to_owned(),
                method_id: format!("{HOLDER_DID}#auth-1"),
                holder_binding_method_id: format!("{HOLDER_DID}#holder-jubjub-1"),
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
            },
        ));
        assert_eq!(
            result,
            Err(CredentialIssuanceError::Approval(
                AcceptedFlowApprovalError::Unavailable
            ))
        );
        assert_eq!(protocol.0.load(Ordering::Relaxed), 0);
        assert_eq!(sink.0.load(Ordering::Relaxed), 0);
        assert_eq!(
            GetCredentialIssuanceUseCase::execute(
                &service,
                CredentialIssuanceQuery {
                    profile_id: "profile_1".to_owned(),
                    issuance_id: prepared.id,
                },
            )
            .expect("retained session")
            .state,
            "awaiting_consent"
        );
    }

    #[test]
    fn consent_and_profile_scope_fail_closed() {
        let service = service();
        let prepared = prepare(&service);
        let denied = futures_lite(AcceptCredentialIssuanceUseCase::execute(
            &service,
            AcceptCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id.clone(),
                holder_did: HOLDER_DID.to_owned(),
                method_id: format!("{HOLDER_DID}#auth-1"),
                holder_binding_method_id: format!("{HOLDER_DID}#holder-jubjub-1"),
                confirmed: false,
                intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
            },
        ));
        assert_eq!(denied, Err(CredentialIssuanceError::ConfirmationRequired));
        assert_eq!(
            GetCredentialIssuanceUseCase::execute(
                &service,
                CredentialIssuanceQuery {
                    profile_id: "profile_2".to_owned(),
                    issuance_id: prepared.id,
                }
            ),
            Err(CredentialIssuanceError::NotFound)
        );
    }

    #[test]
    fn refusal_discards_offer_and_is_terminal() {
        let service = service();
        let prepared = prepare(&service);
        let refused = RefuseCredentialIssuanceUseCase::execute(
            &service,
            RefuseCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id.clone(),
            },
        )
        .expect("refusal should succeed");
        assert_eq!(refused.state, "refused");
        let repeated = RefuseCredentialIssuanceUseCase::execute(
            &service,
            RefuseCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id,
            },
        )
        .expect("repeated refusal should be idempotent");
        assert_eq!(repeated.state, "refused");
    }

    #[test]
    fn failed_issuance_can_be_explicitly_discarded() {
        let service = service();
        let prepared = prepare(&service);
        let failure = futures_lite(AcceptCredentialIssuanceUseCase::execute(
            &service,
            AcceptCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id.clone(),
                holder_did: REJECT_DID.to_owned(),
                method_id: format!("{REJECT_DID}#auth-1"),
                holder_binding_method_id: format!("{REJECT_DID}#holder-jubjub-1"),
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
            },
        ));
        assert_eq!(
            failure,
            Err(CredentialIssuanceError::Protocol(
                IssuanceProtocolError::InvalidProof
            ))
        );

        let failed = GetCredentialIssuanceUseCase::execute(
            &service,
            CredentialIssuanceQuery {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id.clone(),
            },
        )
        .expect("failed issuance remains inspectable");
        assert_eq!(failed.state, "failed");
        assert_eq!(failed.failure_code.as_deref(), Some("invalid_proof"));
        let activity =
            ListCredentialIssuanceActivityUseCase::execute(&service, "profile_1".to_owned())
                .expect("activity projection");
        assert_eq!(activity.records.len(), 1);
        assert_eq!(
            activity.records[0].status,
            CredentialIssuanceActivityStatus::Failed
        );
        assert_eq!(
            activity.records[0].finality,
            CredentialIssuanceActivityFinality::Final
        );

        let discarded = RefuseCredentialIssuanceUseCase::execute(
            &service,
            RefuseCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id,
            },
        )
        .expect("failed issuance should be discardable");
        assert_eq!(discarded.state, "failed");
        assert_eq!(discarded.failure_code.as_deref(), Some("invalid_proof"));
    }

    struct PanickingIssuanceProtocol;

    impl CredentialIssuanceProtocolPort for PanickingIssuanceProtocol {
        fn prepare<'a>(&'a self, _: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
            Box::pin(async {
                Ok(PreparedCredentialOffer {
                    id: CredentialIssuanceId::parse("issuance_interrupted")
                        .expect("valid fixture id"),
                    preview: CredentialOfferPreview::new(
                        "https://issuer.example",
                        vec!["identity".to_owned()],
                        vec!["Identity credential".to_owned()],
                    )
                    .expect("valid preview"),
                })
            })
        }

        fn issue<'a>(&'a self, _: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a> {
            Box::pin(async { panic!("closed test-only issuance worker failure") })
        }

        fn discard(&self, _: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError> {
            Ok(())
        }
    }

    struct PanickingDiscardProtocol(AtomicUsize);

    impl CredentialIssuanceProtocolPort for PanickingDiscardProtocol {
        fn prepare<'a>(&'a self, _: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
            Box::pin(async {
                Ok(PreparedCredentialOffer {
                    id: CredentialIssuanceId::parse("issuance_discard_panic")
                        .expect("valid fixture id"),
                    preview: CredentialOfferPreview::new(
                        "https://issuer.example",
                        vec!["identity".to_owned()],
                        vec!["Identity credential".to_owned()],
                    )
                    .expect("valid preview"),
                })
            })
        }

        fn issue<'a>(&'a self, _: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a> {
            Box::pin(async { Err(IssuanceProtocolError::IssuerRejected) })
        }

        fn discard(&self, _: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError> {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                panic!("closed test-only discard worker failure");
            }
            Ok(())
        }
    }

    #[test]
    fn panicking_discard_releases_refusal_reservation_for_retry() {
        let protocol = Arc::new(PanickingDiscardProtocol(AtomicUsize::new(0)));
        let service = CredentialIssuanceService::with_authority(
            protocol.clone(),
            Arc::new(Sink),
            issuance_authority(),
        );
        let prepared = prepare(&service);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                RefuseCredentialIssuanceUseCase::execute(
                    &service,
                    RefuseCredentialIssuanceCommand {
                        profile_id: "profile_1".to_owned(),
                        issuance_id: prepared.id.clone(),
                    },
                )
            }))
            .is_err()
        );
        assert_eq!(
            RefuseCredentialIssuanceUseCase::execute(
                &service,
                RefuseCredentialIssuanceCommand {
                    profile_id: "profile_1".to_owned(),
                    issuance_id: prepared.id.clone(),
                },
            )
            .expect("panic cleanup permits retry")
            .state,
            "refused"
        );
        assert_eq!(
            RefuseCredentialIssuanceUseCase::execute(
                &service,
                RefuseCredentialIssuanceCommand {
                    profile_id: "profile_1".to_owned(),
                    issuance_id: prepared.id,
                },
            )
            .expect("duplicate refusal is idempotent")
            .state,
            "refused"
        );
        assert_eq!(protocol.0.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn interrupted_issuance_has_unknown_outcome_and_can_be_discarded() {
        let service = CredentialIssuanceService::with_authority(
            Arc::new(PanickingIssuanceProtocol),
            Arc::new(Sink),
            issuance_authority(),
        );
        let prepared = prepare(&service);
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            futures_lite(AcceptCredentialIssuanceUseCase::execute(
                &service,
                AcceptCredentialIssuanceCommand {
                    profile_id: "profile_1".to_owned(),
                    issuance_id: prepared.id.clone(),
                    holder_did: HOLDER_DID.to_owned(),
                    method_id: format!("{HOLDER_DID}#auth-1"),
                    holder_binding_method_id: format!("{HOLDER_DID}#holder-jubjub-1"),
                    confirmed: true,
                    intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
                },
            ))
        }));
        assert!(failure.is_err());

        let interrupted = GetCredentialIssuanceUseCase::execute(
            &service,
            CredentialIssuanceQuery {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id.clone(),
            },
        )
        .expect("interrupted issuance remains inspectable");
        assert_eq!(interrupted.state, "outcome_unknown");
        assert_eq!(
            interrupted.failure_code.as_deref(),
            Some(ISSUANCE_INTERRUPTED_CODE),
        );
        let activity =
            ListCredentialIssuanceActivityUseCase::execute(&service, "profile_1".to_owned())
                .expect("activity projection");
        assert_eq!(
            activity.records[0].status,
            CredentialIssuanceActivityStatus::OutcomeUnknown
        );
        assert_eq!(
            activity.records[0].finality,
            CredentialIssuanceActivityFinality::Unknown
        );

        let discarded = RefuseCredentialIssuanceUseCase::execute(
            &service,
            RefuseCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id,
            },
        )
        .expect("interrupted issuance should be discardable");
        assert_eq!(discarded.state, "outcome_unknown");
        let activity =
            ListCredentialIssuanceActivityUseCase::execute(&service, "profile_1".to_owned())
                .expect("activity projection");
        assert_eq!(
            activity.records[0].status,
            CredentialIssuanceActivityStatus::OutcomeUnknown
        );
        assert_eq!(
            activity.records[0].finality,
            CredentialIssuanceActivityFinality::Unknown
        );
    }

    #[test]
    fn activity_ids_are_monotonic_private_and_terminal_transitions_are_not_stale() {
        let store = CredentialIssuanceActivityStore::new();
        let preview = CredentialOfferPreview::new(
            "https://issuer.example",
            vec!["identity".to_owned()],
            vec!["Identity credential".to_owned()],
        )
        .expect("valid preview");
        let first_session = Session {
            profile_id: ProtocolProfileId::parse("profile_1").expect("valid profile"),
            preview: preview.clone(),
            state: CredentialIssuanceState::Issuing,
            credential_id: None,
            failure_code: None,
            refusal_in_progress: false,
            protocol_discarded: false,
        };
        let second_session = Session {
            profile_id: ProtocolProfileId::parse("profile_2").expect("valid profile"),
            preview,
            state: CredentialIssuanceState::Issuing,
            credential_id: None,
            failure_code: None,
            refusal_in_progress: false,
            protocol_discarded: false,
        };
        let first = CredentialIssuanceId::parse("protocol_issuance_a").expect("valid id");
        let second = CredentialIssuanceId::parse("protocol_issuance_b").expect("valid id");
        assert_eq!(
            store
                .begin(&first, &first_session)
                .expect("activity")
                .value(),
            1
        );
        assert_eq!(
            store
                .begin(&first, &first_session)
                .expect("same activity")
                .value(),
            1
        );
        assert_eq!(
            store
                .begin(&second, &second_session)
                .expect("activity")
                .value(),
            2
        );

        store.update(&first, CredentialIssuanceActivityStatus::Stored);
        store.update(&first, CredentialIssuanceActivityStatus::Failed);
        store.update(&first, CredentialIssuanceActivityStatus::Pending);

        let first_profile = store.execute("profile_1".to_owned()).expect("projection");
        assert_eq!(first_profile.records.len(), 1);
        assert_eq!(first_profile.records[0].id.value(), 1);
        assert_eq!(
            first_profile.records[0].status,
            CredentialIssuanceActivityStatus::Stored
        );
        assert_eq!(
            first_profile.records[0].finality,
            CredentialIssuanceActivityFinality::Final
        );
        let second_profile = store.execute("profile_2".to_owned()).expect("projection");
        assert_eq!(second_profile.records.len(), 1);
        assert_eq!(second_profile.records[0].id.value(), 2);
        store.update(&second, CredentialIssuanceActivityStatus::OutcomeUnknown);
        store.update(&second, CredentialIssuanceActivityStatus::Refused);
        let uncertain = store.execute("profile_2".to_owned()).expect("projection");
        assert_eq!(
            uncertain.records[0].status,
            CredentialIssuanceActivityStatus::OutcomeUnknown
        );
        // Definitive sink success may reconcile an earlier uncertain outcome.
        store.update(&second, CredentialIssuanceActivityStatus::Stored);
        assert_eq!(
            store
                .execute("profile_2".to_owned())
                .expect("projection")
                .records[0]
                .status,
            CredentialIssuanceActivityStatus::Stored
        );
        assert_eq!(store.clear_profile("profile_2").expect("profile purge"), 1);
        assert!(
            store
                .execute("profile_2".to_owned())
                .expect("projection after purge")
                .records
                .is_empty()
        );
        assert_eq!(
            store
                .execute("profile_1".to_owned())
                .expect("other profile remains")
                .records
                .len(),
            1
        );

        let after_restart = CredentialIssuanceActivityStore::new()
            .execute("profile_1".to_owned())
            .expect("fresh process-local projection");
        assert!(after_restart.records.is_empty());
    }

    #[test]
    fn activity_capacity_never_evicts_an_unresolved_issuance() {
        let store = CredentialIssuanceActivityStore::new();
        let session = Session {
            profile_id: ProtocolProfileId::parse("profile_1").expect("valid profile"),
            preview: CredentialOfferPreview::new(
                "https://issuer.example",
                vec!["identity".to_owned()],
                vec!["Identity credential".to_owned()],
            )
            .expect("valid preview"),
            state: CredentialIssuanceState::Issuing,
            credential_id: None,
            failure_code: None,
            refusal_in_progress: false,
            protocol_discarded: false,
        };
        for index in 0..MAX_CREDENTIAL_ISSUANCE_ACTIVITY_RECORDS {
            let id = CredentialIssuanceId::parse(format!("issuance_{index}")).expect("valid id");
            assert!(store.begin(&id, &session).is_some());
        }
        let overflow = CredentialIssuanceId::parse("issuance_overflow").expect("valid id");
        assert!(store.begin(&overflow, &session).is_none());
        let first = CredentialIssuanceId::parse("issuance_0").expect("valid id");
        store.update(&first, CredentialIssuanceActivityStatus::Stored);
        let new_id = store
            .begin(&overflow, &session)
            .expect("terminal slot evicted");
        assert_eq!(
            new_id.value(),
            (MAX_CREDENTIAL_ISSUANCE_ACTIVITY_RECORDS + 1) as u64
        );
        assert_eq!(
            store
                .execute("profile_1".to_owned())
                .expect("projection")
                .records
                .len(),
            MAX_CREDENTIAL_ISSUANCE_ACTIVITY_RECORDS
        );
        let second = CredentialIssuanceId::parse("issuance_1").expect("valid id");
        store.update(&second, CredentialIssuanceActivityStatus::OutcomeUnknown);
        let replacement = CredentialIssuanceId::parse("issuance_after_unknown").expect("valid id");
        assert!(
            store.begin(&replacement, &session).is_some(),
            "uncertain records must not block future issuance forever"
        );
        assert!(
            store
                .execute("profile_1".to_owned())
                .expect("projection")
                .records
                .iter()
                .all(|record| record.id.value() != 2)
        );
    }

    #[test]
    fn refusal_remains_available_when_activity_capacity_is_unresolved() {
        let service = service();
        let prepared = prepare(&service);
        let session = Session {
            profile_id: ProtocolProfileId::parse("profile_1").expect("valid profile"),
            preview: CredentialOfferPreview::new(
                "https://issuer.example",
                vec!["identity".to_owned()],
                vec!["Identity credential".to_owned()],
            )
            .expect("valid preview"),
            state: CredentialIssuanceState::Issuing,
            credential_id: None,
            failure_code: None,
            refusal_in_progress: false,
            protocol_discarded: false,
        };
        let activity = service.activity();
        for index in 0..MAX_CREDENTIAL_ISSUANCE_ACTIVITY_RECORDS {
            let id = CredentialIssuanceId::parse(format!("pending_{index}")).expect("valid id");
            assert!(activity.begin(&id, &session).is_some());
        }

        let refused = RefuseCredentialIssuanceUseCase::execute(
            &service,
            RefuseCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id,
            },
        )
        .expect("activity capacity must not disable refusal");
        assert_eq!(refused.state, "refused");
    }

    #[test]
    fn unavailable_protocol_keeps_activity_outcome_unknown() {
        let service = service();
        let prepared = prepare(&service);
        let result = futures_lite(AcceptCredentialIssuanceUseCase::execute(
            &service,
            AcceptCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id,
                holder_did: UNAVAILABLE_DID.to_owned(),
                method_id: format!("{UNAVAILABLE_DID}#auth-1"),
                holder_binding_method_id: format!("{UNAVAILABLE_DID}#holder-jubjub-1"),
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
            },
        ));
        assert_eq!(
            result,
            Err(CredentialIssuanceError::Protocol(
                IssuanceProtocolError::Unavailable
            ))
        );
        let activity =
            ListCredentialIssuanceActivityUseCase::execute(&service, "profile_1".to_owned())
                .expect("activity projection");
        assert_eq!(
            activity.records[0].status,
            CredentialIssuanceActivityStatus::OutcomeUnknown
        );
        assert_eq!(
            activity.records[0].finality,
            CredentialIssuanceActivityFinality::Unknown
        );
    }

    struct AuthenticationProtocol;

    impl SelfIssuedAuthenticationProtocolPort for AuthenticationProtocol {
        fn prepare<'a>(
            &'a self,
            request: PrepareSelfIssuedAuthenticationRequest,
        ) -> PrepareSelfIssuedAuthenticationPortFuture<'a> {
            Box::pin(async move {
                if request.request == "reject" {
                    return Err(SelfIssuedProtocolError::InvalidRequest);
                }
                Ok(PreparedSelfIssuedAuthentication {
                    id: SelfIssuedAuthenticationId::parse("authentication_1")
                        .expect("valid fixture id"),
                    preview: SelfIssuedAuthenticationPreview::new(
                        "https://verifier.example",
                        "Authenticate with the selected DID.",
                    )
                    .expect("valid preview"),
                })
            })
        }

        fn authenticate<'a>(
            &'a self,
            request: ProtocolSelfIssuedAuthenticationRequest,
        ) -> AuthenticateSelfIssuedPortFuture<'a> {
            Box::pin(async move {
                if request.holder_did == "did:midnight:undeployed:reject" {
                    Err(SelfIssuedProtocolError::InvalidProof)
                } else {
                    Ok(())
                }
            })
        }

        fn discard(&self, _: &SelfIssuedAuthenticationId) -> Result<(), SelfIssuedProtocolError> {
            Ok(())
        }
    }

    fn authentication_service() -> SelfIssuedAuthenticationService {
        SelfIssuedAuthenticationService::with_authority(
            Arc::new(AuthenticationProtocol),
            authentication_authority(),
        )
    }

    struct CountingAuthenticationProtocol {
        authenticate_calls: Arc<AtomicUsize>,
    }

    impl SelfIssuedAuthenticationProtocolPort for CountingAuthenticationProtocol {
        fn prepare<'a>(
            &'a self,
            _: PrepareSelfIssuedAuthenticationRequest,
        ) -> PrepareSelfIssuedAuthenticationPortFuture<'a> {
            Box::pin(async {
                Ok(PreparedSelfIssuedAuthentication {
                    id: SelfIssuedAuthenticationId::parse("authentication_closed")
                        .expect("authentication id"),
                    preview: SelfIssuedAuthenticationPreview::new(
                        "https://verifier.example",
                        "Authenticate with the selected DID.",
                    )
                    .expect("preview"),
                })
            })
        }

        fn authenticate<'a>(
            &'a self,
            _: ProtocolSelfIssuedAuthenticationRequest,
        ) -> AuthenticateSelfIssuedPortFuture<'a> {
            self.authenticate_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        }

        fn discard(&self, _: &SelfIssuedAuthenticationId) -> Result<(), SelfIssuedProtocolError> {
            Ok(())
        }
    }

    #[test]
    fn default_self_issued_acceptance_fails_closed_before_protocol_effects() {
        let authenticate_calls = Arc::new(AtomicUsize::new(0));
        let service =
            SelfIssuedAuthenticationService::new(Arc::new(CountingAuthenticationProtocol {
                authenticate_calls: Arc::clone(&authenticate_calls),
            }));
        let prepared = prepare_authentication(&service);
        let result = futures_lite(AcceptSelfIssuedAuthenticationUseCase::execute(
            &service,
            AcceptSelfIssuedAuthenticationCommand {
                profile_id: "profile_1".to_owned(),
                authentication_id: prepared.id.clone(),
                holder_did: HOLDER_DID.to_owned(),
                method_id: format!("{HOLDER_DID}#auth-1"),
                confirmed: true,
                intent: "ACCEPT_SELF_ISSUED_AUTHENTICATION".to_owned(),
            },
        ));
        assert_eq!(
            result,
            Err(SelfIssuedAuthenticationError::Approval(
                AcceptedFlowApprovalError::Unavailable,
            ))
        );
        assert_eq!(authenticate_calls.load(Ordering::SeqCst), 0);
        let retained = GetSelfIssuedAuthenticationUseCase::execute(
            &service,
            SelfIssuedAuthenticationQuery {
                profile_id: "profile_1".to_owned(),
                authentication_id: prepared.id,
            },
        )
        .expect("preview remains retained");
        assert_eq!(retained.state, "awaiting_consent");
        assert_eq!(retained.failure_code, None);
    }

    fn prepare_authentication(
        service: &SelfIssuedAuthenticationService,
    ) -> SelfIssuedAuthenticationView {
        futures_lite(PrepareSelfIssuedAuthenticationUseCase::execute(
            service,
            PrepareSelfIssuedAuthenticationCommand {
                profile_id: "profile_1".to_owned(),
                request: "request".to_owned(),
            },
        ))
        .expect("prepare should succeed")
    }

    #[test]
    fn self_issued_authentication_requires_exact_consent_and_profile_scope() {
        let service = authentication_service();
        let prepared = prepare_authentication(&service);
        assert_eq!(prepared.state, "awaiting_consent");
        let denied = futures_lite(AcceptSelfIssuedAuthenticationUseCase::execute(
            &service,
            AcceptSelfIssuedAuthenticationCommand {
                profile_id: "profile_1".to_owned(),
                authentication_id: prepared.id.clone(),
                holder_did: HOLDER_DID.to_owned(),
                method_id: format!("{HOLDER_DID}#auth-1"),
                confirmed: false,
                intent: "ACCEPT_SELF_ISSUED_AUTHENTICATION".to_owned(),
            },
        ));
        assert_eq!(
            denied,
            Err(SelfIssuedAuthenticationError::ConfirmationRequired)
        );
        assert_eq!(
            GetSelfIssuedAuthenticationUseCase::execute(
                &service,
                SelfIssuedAuthenticationQuery {
                    profile_id: "profile_2".to_owned(),
                    authentication_id: prepared.id,
                }
            ),
            Err(SelfIssuedAuthenticationError::NotFound)
        );
    }

    #[test]
    fn self_issued_authentication_succeeds_and_refusal_is_terminal() {
        let service = authentication_service();
        let prepared = prepare_authentication(&service);
        let authenticated = futures_lite(AcceptSelfIssuedAuthenticationUseCase::execute(
            &service,
            AcceptSelfIssuedAuthenticationCommand {
                profile_id: "profile_1".to_owned(),
                authentication_id: prepared.id,
                holder_did: HOLDER_DID.to_owned(),
                method_id: format!("{HOLDER_DID}#auth-1"),
                confirmed: true,
                intent: "ACCEPT_SELF_ISSUED_AUTHENTICATION".to_owned(),
            },
        ))
        .expect("authentication should succeed");
        assert_eq!(authenticated.state, "succeeded");
        assert!(authenticated.failure_code.is_none());

        let refusal_service = authentication_service();
        let second = prepare_authentication(&refusal_service);
        let refused = RefuseSelfIssuedAuthenticationUseCase::execute(
            &refusal_service,
            RefuseSelfIssuedAuthenticationCommand {
                profile_id: "profile_1".to_owned(),
                authentication_id: second.id.clone(),
            },
        )
        .expect("refusal should succeed");
        assert_eq!(refused.state, "refused");
        assert_eq!(
            RefuseSelfIssuedAuthenticationUseCase::execute(
                &refusal_service,
                RefuseSelfIssuedAuthenticationCommand {
                    profile_id: "profile_1".to_owned(),
                    authentication_id: second.id,
                }
            ),
            Err(SelfIssuedAuthenticationError::InvalidState)
        );
    }
}

#[cfg(test)]
mod issue_157_tests;
