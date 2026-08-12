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
};

pub const MAX_CREDENTIAL_OFFER_BYTES: usize = 32 * 1_024;
const MAX_DID_CHARACTERS: usize = 8_192;
const MAX_METHOD_CHARACTERS: usize = 8_192;

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
}

#[derive(Debug, PartialEq, Eq)]
pub struct IssuedCredentialBytes(pub Vec<u8>);

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

#[derive(Debug, PartialEq, Eq)]
pub struct StoreIssuedCredentialRequest {
    pub profile_id: ProtocolProfileId,
    pub signed_bytes: Vec<u8>,
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
            let issued = match self
                .protocol
                .issue(ProtocolIssueRequest {
                    profile_id: profile_id.clone(),
                    issuance_id: issuance_id.clone(),
                    holder_did: command.holder_did,
                    method_id: command.method_id,
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
                    signed_bytes: issued.0,
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
            if session.state != CredentialIssuanceState::AwaitingConsent {
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

#[cfg(test)]
mod tests {
    use super::*;

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
                    Ok(IssuedCredentialBytes(vec![1, 2, 3]))
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
}
