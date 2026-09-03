// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const PROFILE: &str = "profile_issue_157";
const ISSUANCE_ID: &str = "issuance_issue_157";
const AUTHENTICATION_ID: &str = "authentication_issue_157";
const SIGNED_SECRET: &[u8] = b"issued-signed-credential-secret";
const PROOF_SECRET: &[u8] = b"issued-detached-proof-secret";
const PRIVATE_SECRET: &[u8] = b"issued-private-material-secret";

struct IssuanceProtocol {
    discard_failures: usize,
    discard_calls: AtomicUsize,
}

impl IssuanceProtocol {
    fn new(discard_failures: usize) -> Self {
        Self {
            discard_failures,
            discard_calls: AtomicUsize::new(0),
        }
    }
}

impl CredentialIssuanceProtocolPort for IssuanceProtocol {
    fn prepare<'a>(&'a self, request: PrepareIssuanceRequest) -> PrepareIssuancePortFuture<'a> {
        Box::pin(async move {
            assert_eq!(request.profile_id.as_str(), PROFILE);
            Ok(PreparedCredentialOffer {
                id: CredentialIssuanceId::parse(ISSUANCE_ID).expect("issuance id"),
                preview: CredentialOfferPreview::new(
                    "https://issuer.example",
                    vec!["identity".to_owned()],
                    vec!["Identity credential".to_owned()],
                )
                .expect("preview"),
            })
        })
    }

    fn issue<'a>(&'a self, request: ProtocolIssueRequest) -> IssueCredentialPortFuture<'a> {
        Box::pin(async move {
            assert_eq!(request.profile_id.as_str(), PROFILE);
            assert_eq!(request.issuance_id.as_str(), ISSUANCE_ID);
            Ok(IssuedCredentialBytes {
                signed_bytes: SIGNED_SECRET.to_vec(),
                detached_proof: Some(PROOF_SECRET.to_vec()),
                private_material: Some(PRIVATE_SECRET.to_vec()),
            })
        })
    }

    fn discard(&self, issuance_id: &CredentialIssuanceId) -> Result<(), IssuanceProtocolError> {
        assert_eq!(issuance_id.as_str(), ISSUANCE_ID);
        let call = self.discard_calls.fetch_add(1, Ordering::SeqCst);
        if call < self.discard_failures {
            Err(IssuanceProtocolError::Unavailable)
        } else {
            Ok(())
        }
    }
}

struct FailingSink(IssuedCredentialSinkError);

impl IssuedCredentialSinkPort for FailingSink {
    fn store_verified<'a>(
        &'a self,
        request: StoreIssuedCredentialRequest,
    ) -> StoreIssuedCredentialFuture<'a> {
        let error = self.0;
        Box::pin(async move {
            assert_eq!(request.profile_id.as_str(), PROFILE);
            assert_eq!(request.signed_bytes, SIGNED_SECRET);
            assert_eq!(request.detached_proof.as_deref(), Some(PROOF_SECRET));
            assert_eq!(request.private_material.as_deref(), Some(PRIVATE_SECRET));
            Err(error)
        })
    }
}

fn issuance_service(
    sink_error: IssuedCredentialSinkError,
    discard_failures: usize,
) -> CredentialIssuanceService {
    CredentialIssuanceService::new(
        Arc::new(IssuanceProtocol::new(discard_failures)),
        Arc::new(FailingSink(sink_error)),
    )
}

fn prepare_issuance(service: &CredentialIssuanceService) -> CredentialIssuanceView {
    poll(PrepareCredentialIssuanceUseCase::execute(
        service,
        PrepareCredentialIssuanceCommand {
            profile_id: PROFILE.to_owned(),
            offer: "credential-offer-secret".to_owned(),
        },
    ))
    .expect("prepare issuance")
}

fn accept_command() -> AcceptCredentialIssuanceCommand {
    AcceptCredentialIssuanceCommand {
        profile_id: PROFILE.to_owned(),
        issuance_id: ISSUANCE_ID.to_owned(),
        holder_did: "did:midnight:undeployed:holder".to_owned(),
        method_id: "did:midnight:undeployed:holder#auth-1".to_owned(),
        holder_binding_method_id: "did:midnight:undeployed:holder#holder-jubjub-1".to_owned(),
        confirmed: true,
        intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
    }
}

fn inspect_issuance(service: &CredentialIssuanceService) -> CredentialIssuanceView {
    GetCredentialIssuanceUseCase::execute(
        service,
        CredentialIssuanceQuery {
            profile_id: PROFILE.to_owned(),
            issuance_id: ISSUANCE_ID.to_owned(),
        },
    )
    .expect("issuance remains inspectable")
}

#[test]
fn issued_credential_sink_errors_are_terminal_redacted_and_replay_safe() {
    let cases = [
        (
            IssuedCredentialSinkError::Unavailable,
            "credential_store_unavailable",
        ),
        (
            IssuedCredentialSinkError::InvalidCredential,
            "invalid_credential",
        ),
        (
            IssuedCredentialSinkError::VerificationFailed,
            "credential_verification_failed",
        ),
        (
            IssuedCredentialSinkError::PersistenceFailed,
            "credential_persistence_failed",
        ),
    ];

    for (sink_error, failure_code) in cases {
        let service = issuance_service(sink_error, 0);
        prepare_issuance(&service);

        let failure = poll(AcceptCredentialIssuanceUseCase::execute(
            &service,
            accept_command(),
        ));
        assert_eq!(failure, Err(CredentialIssuanceError::Sink(sink_error)));

        let failed = inspect_issuance(&service);
        assert_eq!(failed.state, "failed");
        assert_eq!(failed.failure_code.as_deref(), Some(failure_code));
        assert_eq!(failed.credential_id, None);
        assert_eq!(
            ListCredentialIssuancesUseCase::execute(
                &service,
                CredentialIssuanceProfileQuery {
                    profile_id: PROFILE.to_owned(),
                },
            )
            .expect("failed issuance is listed"),
            vec![failed.clone()]
        );
        assert_eq!(
            poll(AcceptCredentialIssuanceUseCase::execute(
                &service,
                accept_command(),
            )),
            Err(CredentialIssuanceError::InvalidState)
        );

        let diagnostic = format!("{failure:?} {failed:?}");
        assert!(!diagnostic.contains("issued-signed-credential-secret"));
        assert!(!diagnostic.contains("issued-detached-proof-secret"));
        assert!(!diagnostic.contains("issued-private-material-secret"));
    }
}

#[test]
fn issuance_discard_failure_preserves_awaiting_session_for_retry() {
    let service = issuance_service(IssuedCredentialSinkError::Unavailable, 1);
    prepare_issuance(&service);

    assert_eq!(
        RefuseCredentialIssuanceUseCase::execute(
            &service,
            RefuseCredentialIssuanceCommand {
                profile_id: PROFILE.to_owned(),
                issuance_id: ISSUANCE_ID.to_owned(),
            },
        ),
        Err(CredentialIssuanceError::Protocol(
            IssuanceProtocolError::Unavailable
        ))
    );
    let retained = inspect_issuance(&service);
    assert_eq!(retained.state, "awaiting_consent");
    assert_eq!(retained.failure_code, None);

    let refused = RefuseCredentialIssuanceUseCase::execute(
        &service,
        RefuseCredentialIssuanceCommand {
            profile_id: PROFILE.to_owned(),
            issuance_id: ISSUANCE_ID.to_owned(),
        },
    )
    .expect("discard can be retried");
    assert_eq!(refused.state, "refused");
}

#[test]
fn failed_issuance_discard_failure_preserves_failure_for_retry() {
    let service = issuance_service(IssuedCredentialSinkError::PersistenceFailed, 1);
    prepare_issuance(&service);
    assert_eq!(
        poll(AcceptCredentialIssuanceUseCase::execute(
            &service,
            accept_command(),
        )),
        Err(CredentialIssuanceError::Sink(
            IssuedCredentialSinkError::PersistenceFailed
        ))
    );

    assert_eq!(
        RefuseCredentialIssuanceUseCase::execute(
            &service,
            RefuseCredentialIssuanceCommand {
                profile_id: PROFILE.to_owned(),
                issuance_id: ISSUANCE_ID.to_owned(),
            },
        ),
        Err(CredentialIssuanceError::Protocol(
            IssuanceProtocolError::Unavailable
        ))
    );
    let retained = inspect_issuance(&service);
    assert_eq!(retained.state, "failed");
    assert_eq!(
        retained.failure_code.as_deref(),
        Some("credential_persistence_failed")
    );

    let refused = RefuseCredentialIssuanceUseCase::execute(
        &service,
        RefuseCredentialIssuanceCommand {
            profile_id: PROFILE.to_owned(),
            issuance_id: ISSUANCE_ID.to_owned(),
        },
    )
    .expect("failed-session discard can be retried");
    assert_eq!(refused.state, "refused");
    assert_eq!(refused.failure_code, None);
}

struct AuthenticationProtocol {
    discard_failures: usize,
    discard_calls: AtomicUsize,
}

impl AuthenticationProtocol {
    fn new(discard_failures: usize) -> Self {
        Self {
            discard_failures,
            discard_calls: AtomicUsize::new(0),
        }
    }
}

impl SelfIssuedAuthenticationProtocolPort for AuthenticationProtocol {
    fn prepare<'a>(
        &'a self,
        request: PrepareSelfIssuedAuthenticationRequest,
    ) -> PrepareSelfIssuedAuthenticationPortFuture<'a> {
        Box::pin(async move {
            assert_eq!(request.profile_id.as_str(), PROFILE);
            Ok(PreparedSelfIssuedAuthentication {
                id: SelfIssuedAuthenticationId::parse(AUTHENTICATION_ID)
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
        Box::pin(async { Ok(()) })
    }

    fn discard(
        &self,
        authentication_id: &SelfIssuedAuthenticationId,
    ) -> Result<(), SelfIssuedProtocolError> {
        assert_eq!(authentication_id.as_str(), AUTHENTICATION_ID);
        let call = self.discard_calls.fetch_add(1, Ordering::SeqCst);
        if call < self.discard_failures {
            Err(SelfIssuedProtocolError::Unavailable)
        } else {
            Ok(())
        }
    }
}

fn prepare_authentication(service: &SelfIssuedAuthenticationService) {
    poll(PrepareSelfIssuedAuthenticationUseCase::execute(
        service,
        PrepareSelfIssuedAuthenticationCommand {
            profile_id: PROFILE.to_owned(),
            request: "self-issued-request-secret".to_owned(),
        },
    ))
    .expect("prepare authentication");
}

fn inspect_authentication(
    service: &SelfIssuedAuthenticationService,
) -> SelfIssuedAuthenticationView {
    GetSelfIssuedAuthenticationUseCase::execute(
        service,
        SelfIssuedAuthenticationQuery {
            profile_id: PROFILE.to_owned(),
            authentication_id: AUTHENTICATION_ID.to_owned(),
        },
    )
    .expect("authentication remains inspectable")
}

#[test]
fn self_issued_discard_failure_preserves_awaiting_session_for_retry() {
    let service = SelfIssuedAuthenticationService::new(Arc::new(AuthenticationProtocol::new(1)));
    prepare_authentication(&service);

    assert_eq!(
        RefuseSelfIssuedAuthenticationUseCase::execute(
            &service,
            RefuseSelfIssuedAuthenticationCommand {
                profile_id: PROFILE.to_owned(),
                authentication_id: AUTHENTICATION_ID.to_owned(),
            },
        ),
        Err(SelfIssuedAuthenticationError::Protocol(
            SelfIssuedProtocolError::Unavailable
        ))
    );
    let retained = inspect_authentication(&service);
    assert_eq!(retained.state, "awaiting_consent");
    assert_eq!(retained.failure_code, None);

    let refused = RefuseSelfIssuedAuthenticationUseCase::execute(
        &service,
        RefuseSelfIssuedAuthenticationCommand {
            profile_id: PROFILE.to_owned(),
            authentication_id: AUTHENTICATION_ID.to_owned(),
        },
    )
    .expect("self-issued discard can be retried");
    assert_eq!(refused.state, "refused");
}

fn poll<T>(mut future: Pin<Box<dyn Future<Output = T> + Send + '_>>) -> T {
    use std::task::{Context, Poll, Waker};
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture future must be ready"),
    }
}
