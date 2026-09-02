// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
};

use oxid_foundation::OpaqueIdError;
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

pub type PrepareIssuancePortFuture<'a> = Pin<
    Box<dyn Future<Output = Result<PreparedCredentialOffer, IssuanceProtocolError>> + Send + 'a>,
>;
pub type IssueCredentialPortFuture<'a> =
    Pin<Box<dyn Future<Output = Result<IssuedCredentialBytes, IssuanceProtocolError>> + Send + 'a>>;
pub type HolderProofFuture<'a> =
    Pin<Box<dyn Future<Output = Result<String, HolderProofError>> + Send + 'a>>;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolIssueRequest {
    pub profile_id: ProtocolProfileId,
    pub issuance_id: CredentialIssuanceId,
    pub holder_did: String,
    pub method_id: String,
    pub holder_binding_method_id: String,
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

pub trait CredentialIssuanceProtocolPort: Send + Sync {
    fn prepare<'a>(&'a self, request: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a>;
    fn issue<'a>(&'a self, request: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a>;
    fn discard(&self, issuance_id: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HolderProofRequest {
    pub profile_id: ProtocolProfileId,
    pub holder_did: String,
    pub method_id: String,
    pub audience: String,
    pub nonce: String,
}

pub trait CredentialHolderProofPort: Send + Sync {
    fn create<'a>(&'a self, request: HolderProofRequest) -> HolderProofFuture<'a>;
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

#[derive(Clone, Debug)]
struct Session {
    profile_id: ProtocolProfileId,
    preview: CredentialOfferPreview,
    state: CredentialIssuanceState,
    credential_id: Option<String>,
    failure_code: Option<String>,
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
    sessions: Mutex<BTreeMap<CredentialIssuanceId, Session>>,
}

/// Restores a recoverable terminal state if an issuance future is dropped or
/// unwinds after admission but before its normal success/error transition.
struct IssuanceAttempt<'a> {
    service: &'a CredentialIssuanceService,
    issuance_id: CredentialIssuanceId,
}

impl Drop for IssuanceAttempt<'_> {
    fn drop(&mut self) {
        self.service
            .fail_if_issuing(&self.issuance_id, ISSUANCE_INTERRUPTED_CODE);
    }
}

impl CredentialIssuanceService {
    #[must_use]
    pub fn new(
        protocol: Arc<dyn CredentialIssuanceProtocolPort>,
        sink: Arc<dyn IssuedCredentialSinkPort>,
    ) -> Self {
        Self {
            protocol,
            sink,
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    fn sessions(
        &self,
    ) -> Result<MutexGuard<'_, BTreeMap<CredentialIssuanceId, Session>>, CredentialIssuanceError>
    {
        self.sessions
            .lock()
            .map_err(|_| CredentialIssuanceError::Unavailable)
    }

    fn fail(&self, id: &CredentialIssuanceId, code: &str) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.get_mut(id)
        {
            session.state = CredentialIssuanceState::Failed;
            session.failure_code = Some(code.to_owned());
        }
    }

    fn fail_if_issuing(&self, id: &CredentialIssuanceId, code: &str) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.get_mut(id)
            && session.state == CredentialIssuanceState::Issuing
        {
            session.state = CredentialIssuanceState::Failed;
            session.failure_code = Some(code.to_owned());
        }
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
            {
                let mut sessions = self.sessions()?;
                let session = sessions
                    .get_mut(&issuance_id)
                    .ok_or(CredentialIssuanceError::NotFound)?;
                if session.profile_id != profile_id {
                    return Err(CredentialIssuanceError::NotFound);
                }
                if session.state != CredentialIssuanceState::AwaitingConsent {
                    return Err(CredentialIssuanceError::InvalidState);
                }
                session.state = CredentialIssuanceState::Issuing;
            }
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
                })
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    self.fail(&issuance_id, error.code());
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
                    self.fail(&issuance_id, code);
                    return Err(CredentialIssuanceError::Sink(error));
                }
            };
            let mut sessions = self.sessions()?;
            let session = sessions
                .get_mut(&issuance_id)
                .ok_or(CredentialIssuanceError::NotFound)?;
            session.state = CredentialIssuanceState::Succeeded;
            session.credential_id = Some(stored.credential_id);
            session.failure_code = None;
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
            let sessions = self.sessions()?;
            let session = sessions
                .get(&issuance_id)
                .ok_or(CredentialIssuanceError::NotFound)?;
            if session.profile_id != profile_id {
                return Err(CredentialIssuanceError::NotFound);
            }
            if !matches!(
                session.state,
                CredentialIssuanceState::AwaitingConsent | CredentialIssuanceState::Failed
            ) {
                return Err(CredentialIssuanceError::InvalidState);
            }
        }
        self.protocol
            .discard(&issuance_id)
            .map_err(CredentialIssuanceError::Protocol)?;
        let mut sessions = self.sessions()?;
        let session = sessions
            .get_mut(&issuance_id)
            .ok_or(CredentialIssuanceError::NotFound)?;
        session.state = CredentialIssuanceState::Refused;
        session.failure_code = None;
        Ok(session.view(&issuance_id))
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
pub type SelfIssuedProofFuture<'a> =
    Pin<Box<dyn Future<Output = Result<String, SelfIssuedProofError>> + Send + 'a>>;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolSelfIssuedAuthenticationRequest {
    pub profile_id: ProtocolProfileId,
    pub authentication_id: SelfIssuedAuthenticationId,
    pub holder_did: String,
    pub method_id: String,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfIssuedProofRequest {
    pub profile_id: ProtocolProfileId,
    pub holder_did: String,
    pub method_id: String,
    pub audience: String,
    pub nonce: String,
    pub issued_at_seconds: u64,
    pub expires_at_seconds: u64,
}

pub trait SelfIssuedIdentityProofPort: Send + Sync {
    fn create<'a>(&'a self, request: SelfIssuedProofRequest) -> SelfIssuedProofFuture<'a>;
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
    sessions: Mutex<BTreeMap<SelfIssuedAuthenticationId, SelfIssuedSession>>,
}

impl SelfIssuedAuthenticationService {
    #[must_use]
    pub fn new(protocol: Arc<dyn SelfIssuedAuthenticationProtocolPort>) -> Self {
        Self {
            protocol,
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
            {
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
                session.state = SelfIssuedAuthenticationState::Authenticating;
            }
            if let Err(error) = self
                .protocol
                .authenticate(ProtocolSelfIssuedAuthenticationRequest {
                    profile_id,
                    authentication_id: authentication_id.clone(),
                    holder_did: command.holder_did,
                    method_id: command.method_id,
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
                if request.holder_did == "did:midnight:undeployed:reject" {
                    Err(IssuanceProtocolError::InvalidProof)
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
        CredentialIssuanceService::new(Arc::new(Protocol), Arc::new(Sink))
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
                holder_did: "did:midnight:undeployed:holder".to_owned(),
                method_id: "did:midnight:undeployed:holder#auth-1".to_owned(),
                holder_binding_method_id: "did:midnight:undeployed:holder#holder-jubjub-1"
                    .to_owned(),
                confirmed: true,
                intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
            },
        ))
        .expect("issuance should succeed");
        assert_eq!(issued.state, "succeeded");
        assert_eq!(issued.credential_id.as_deref(), Some("vc_1"));
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
                holder_did: "did:midnight:undeployed:holder".to_owned(),
                method_id: "did:midnight:undeployed:holder#auth-1".to_owned(),
                holder_binding_method_id: "did:midnight:undeployed:holder#holder-jubjub-1"
                    .to_owned(),
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
        assert_eq!(
            RefuseCredentialIssuanceUseCase::execute(
                &service,
                RefuseCredentialIssuanceCommand {
                    profile_id: "profile_1".to_owned(),
                    issuance_id: prepared.id,
                }
            ),
            Err(CredentialIssuanceError::InvalidState)
        );
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
                holder_did: "did:midnight:undeployed:reject".to_owned(),
                method_id: "did:midnight:undeployed:reject#auth-1".to_owned(),
                holder_binding_method_id: "did:midnight:undeployed:reject#holder-jubjub-1"
                    .to_owned(),
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

        let discarded = RefuseCredentialIssuanceUseCase::execute(
            &service,
            RefuseCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id,
            },
        )
        .expect("failed issuance should be discardable");
        assert_eq!(discarded.state, "refused");
        assert_eq!(discarded.failure_code, None);
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

    #[test]
    fn interrupted_issuance_becomes_failed_and_can_be_discarded() {
        let service =
            CredentialIssuanceService::new(Arc::new(PanickingIssuanceProtocol), Arc::new(Sink));
        let prepared = prepare(&service);
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            futures_lite(AcceptCredentialIssuanceUseCase::execute(
                &service,
                AcceptCredentialIssuanceCommand {
                    profile_id: "profile_1".to_owned(),
                    issuance_id: prepared.id.clone(),
                    holder_did: "did:midnight:undeployed:holder".to_owned(),
                    method_id: "did:midnight:undeployed:holder#auth-1".to_owned(),
                    holder_binding_method_id: "did:midnight:undeployed:holder#holder-jubjub-1"
                        .to_owned(),
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
        assert_eq!(interrupted.state, "failed");
        assert_eq!(
            interrupted.failure_code.as_deref(),
            Some(ISSUANCE_INTERRUPTED_CODE),
        );

        let discarded = RefuseCredentialIssuanceUseCase::execute(
            &service,
            RefuseCredentialIssuanceCommand {
                profile_id: "profile_1".to_owned(),
                issuance_id: prepared.id,
            },
        )
        .expect("interrupted issuance should be discardable");
        assert_eq!(discarded.state, "refused");
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
        SelfIssuedAuthenticationService::new(Arc::new(AuthenticationProtocol))
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
                holder_did: "did:midnight:undeployed:holder".to_owned(),
                method_id: "did:midnight:undeployed:holder#auth-1".to_owned(),
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
                holder_did: "did:midnight:undeployed:holder".to_owned(),
                method_id: "did:midnight:undeployed:holder#auth-1".to_owned(),
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
