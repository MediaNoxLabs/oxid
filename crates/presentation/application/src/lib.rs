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
use oxid_presentation_domain::{
    CredentialPresentationId, CredentialPresentationPreview, CredentialPresentationState,
    PresentationCredentialCandidate, PresentationProfileId, RequestedPresentationClaim,
};

pub const MAX_PRESENTATION_REQUEST_BYTES: usize = 64 * 1_024;
const MAX_CREDENTIAL_IDENTIFIER_CHARACTERS: usize = 256;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolPresentCredentialRequest {
    pub profile_id: PresentationProfileId,
    pub presentation_id: CredentialPresentationId,
    pub credential_id: String,
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

#[derive(Clone, PartialEq, Eq)]
pub struct PresentationProofRequest {
    pub profile_id: PresentationProfileId,
    pub credential_id: String,
    pub verifier: String,
    pub challenge_hash: [u8; 32],
    pub verifier_domain_hash: [u8; 32],
    pub requested_claims: Vec<RequestedPresentationClaim>,
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
            .field("credential_id", &self.credential_id)
            .field("verifier", &self.verifier)
            .field("requested_claim_count", &self.requested_claims.len())
            .finish_non_exhaustive()
    }
}

pub trait PresentationProofPort: Send + Sync {
    fn create<'a>(&'a self, request: PresentationProofRequest)
    -> CreatePresentationProofFuture<'a>;
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

#[derive(Clone, Debug)]
struct Session {
    profile_id: PresentationProfileId,
    preview: CredentialPresentationPreview,
    state: CredentialPresentationState,
    presentation_generated: bool,
    verifier_validated: bool,
    failure_code: Option<String>,
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
    sessions: Mutex<BTreeMap<CredentialPresentationId, Session>>,
}

impl CredentialPresentationService {
    #[must_use]
    pub fn new(protocol: Arc<dyn CredentialPresentationProtocolPort>) -> Self {
        Self {
            protocol,
            sessions: Mutex::new(BTreeMap::new()),
        }
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

    fn fail(&self, id: &CredentialPresentationId, code: &str) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.get_mut(id)
        {
            session.state = CredentialPresentationState::Failed;
            session.presentation_generated = false;
            session.verifier_validated = false;
            session.failure_code = Some(code.to_owned());
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
            {
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
                session.state = CredentialPresentationState::Presenting;
            }
            let outcome = match self
                .protocol
                .present(ProtocolPresentCredentialRequest {
                    profile_id,
                    presentation_id: presentation_id.clone(),
                    credential_id: command.credential_id,
                })
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    self.fail(&presentation_id, error.code());
                    return Err(CredentialPresentationError::Protocol(error));
                }
            };
            if !outcome.verifier_validated {
                self.fail(
                    &presentation_id,
                    PresentationProtocolError::VerifierRejected.code(),
                );
                return Err(CredentialPresentationError::Protocol(
                    PresentationProtocolError::VerifierRejected,
                ));
            }
            let mut sessions = self.sessions()?;
            let session = sessions
                .get_mut(&presentation_id)
                .ok_or(CredentialPresentationError::NotFound)?;
            session.state = CredentialPresentationState::Succeeded;
            session.presentation_generated = true;
            session.verifier_validated = true;
            session.failure_code = None;
            Ok(session.view(&presentation_id))
        })
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
            let sessions = self.sessions()?;
            let session = sessions
                .get(&presentation_id)
                .ok_or(CredentialPresentationError::NotFound)?;
            if session.profile_id != profile_id {
                return Err(CredentialPresentationError::NotFound);
            }
            if session.state != CredentialPresentationState::AwaitingConsent {
                return Err(CredentialPresentationError::InvalidState);
            }
        }
        self.protocol
            .discard(&presentation_id)
            .map_err(CredentialPresentationError::Protocol)?;
        let mut sessions = self.sessions()?;
        let session = sessions
            .get_mut(&presentation_id)
            .ok_or(CredentialPresentationError::NotFound)?;
        session.state = CredentialPresentationState::Refused;
        Ok(session.view(&presentation_id))
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
    use std::task::{Context, Poll, Waker};

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
            Box::pin(async move {
                *self
                    .selected_credential_id
                    .lock()
                    .expect("selected credential lock") = Some(request.credential_id);
                Err(PresentationProtocolError::ProofUnavailable)
            })
        }

        fn discard(&self, _: &CredentialPresentationId) -> Result<(), PresentationProtocolError> {
            Ok(())
        }
    }

    #[test]
    fn exact_consent_is_profile_scoped_and_proof_failure_is_terminal() {
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
