// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt};

use oxid_foundation::OpaqueIdError;
use oxid_identity_domain::{
    DidRecord, DidResolution, IdentityProfileId, MidnightDid, MidnightDidError, MidnightNetwork,
    VerificationRelationship,
};

use crate::{DidOperationError, DidRecordRepositoryError, DidRecordView, DidService};

pub const MAX_DID_SIGNING_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_CONFIRMATION_TITLE_CHARACTERS: usize = 96;
const MAX_CONFIRMATION_SUMMARY_CHARACTERS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidKeyAlgorithm {
    Ed25519,
    Jubjub,
    P256,
}

impl DidKeyAlgorithm {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::Jubjub => "jubjub",
            Self::P256 => "p256",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DidUpdate {
    AddAlsoKnownAs {
        value: String,
    },
    RemoveAlsoKnownAs {
        value: String,
    },
    AddVerificationMethod {
        fragment: String,
        algorithm: DidKeyAlgorithm,
    },
    UpdateVerificationMethod {
        method_id: String,
        algorithm: DidKeyAlgorithm,
    },
    RemoveVerificationMethod {
        method_id: String,
    },
    AddVerificationRelationship {
        relationship: VerificationRelationship,
        method_id: String,
    },
    RemoveVerificationRelationship {
        relationship: VerificationRelationship,
        method_id: String,
    },
    AddService {
        id: String,
        service_type: String,
        endpoint: String,
    },
    UpdateService {
        id: String,
        service_type: String,
        endpoint: String,
    },
    RemoveService {
        id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidOperationConfirmation {
    pub title: String,
    pub summary: String,
    pub confirmed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateDidCommand {
    pub profile_id: String,
    pub network: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateDidCommand {
    pub profile_id: String,
    pub did: String,
    pub operation: DidUpdate,
    pub confirmation: DidOperationConfirmation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeactivateDidCommand {
    pub profile_id: String,
    pub did: String,
    pub confirmation: DidOperationConfirmation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignDidPayloadCommand {
    pub profile_id: String,
    pub did: String,
    pub method_id: String,
    pub payload: Vec<u8>,
    pub confirmation: DidOperationConfirmation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidSignatureView {
    pub method_id: String,
    pub algorithm: String,
    pub signature_bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidLifecycleSignature {
    pub method_id: String,
    pub algorithm: DidKeyAlgorithm,
    pub signature_bytes: Vec<u8>,
}

pub trait CreateDidUseCase: Send + Sync {
    fn execute(&self, command: CreateDidCommand) -> Result<DidRecordView, DidOperationError>;
}

pub trait UpdateDidUseCase: Send + Sync {
    fn execute(&self, command: UpdateDidCommand) -> Result<DidRecordView, DidOperationError>;
}

pub trait DeactivateDidUseCase: Send + Sync {
    fn execute(&self, command: DeactivateDidCommand) -> Result<DidRecordView, DidOperationError>;
}

pub trait SignDidPayloadUseCase: Send + Sync {
    fn execute(
        &self,
        command: SignDidPayloadCommand,
    ) -> Result<DidSignatureView, DidOperationError>;
}

/// Mutable DID boundary. A live adapter may prove and submit Compact calls;
/// the standalone adapter performs the same state transitions in process.
pub trait DidLifecyclePort: Send + Sync {
    /// Returns the verification methods whose private keys are available to
    /// this lifecycle adapter in the current process. Persisted or resolved
    /// public documents must not be presented as locally controlled merely
    /// because they contain an authentication relationship.
    fn managed_method_ids(
        &self,
        _profile_id: &IdentityProfileId,
        _current: &DidResolution,
    ) -> Result<Vec<String>, DidLifecyclePortError> {
        Ok(Vec::new())
    }

    fn create(
        &self,
        profile_id: &IdentityProfileId,
        network: MidnightNetwork,
    ) -> Result<DidResolution, DidLifecyclePortError>;

    fn update(
        &self,
        profile_id: &IdentityProfileId,
        current: &DidResolution,
        operation: DidUpdate,
    ) -> Result<DidResolution, DidLifecyclePortError>;

    fn deactivate(
        &self,
        profile_id: &IdentityProfileId,
        current: &DidResolution,
    ) -> Result<DidResolution, DidLifecyclePortError>;

    fn sign(
        &self,
        profile_id: &IdentityProfileId,
        current: &DidResolution,
        method_id: &str,
        payload: &[u8],
    ) -> Result<DidLifecycleSignature, DidLifecyclePortError>;
}

/// Public output from a protected Jubjub challenge signature by one currently
/// managed DID method. Points are canonical Midnight compressed encodings and
/// the response is a canonical little-endian field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidJubjubChallengeSignature {
    pub method_id: String,
    pub public_key: [u8; 32],
    pub announcement: [u8; 32],
    pub response: [u8; 32],
}

pub type DidJubjubChallengeDeriver<'a> =
    dyn FnMut(&[u8; 32], &[u8; 32]) -> Result<[u8; 32], DidLifecyclePortError> + 'a;

/// Adapter-to-adapter capability for DID-bound Schnorr protocols whose exact
/// challenge is derived by the consuming protocol adapter.
///
/// Implementations must resolve the method only from current managed custody.
/// The callback sees public points only; the DID private key and nonce remain
/// inside custody throughout the synchronous operation.
pub trait DidJubjubChallengeSigningPort: Send + Sync {
    fn sign_jubjub_challenge(
        &self,
        profile_id: &IdentityProfileId,
        did: &MidnightDid,
        method_id: &str,
        expected_public_key: &[u8; 32],
        derive_challenge: &mut DidJubjubChallengeDeriver<'_>,
    ) -> Result<DidJubjubChallengeSignature, DidLifecyclePortError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidLifecyclePortError {
    Unavailable,
    UnsupportedNetwork,
    UnsupportedAlgorithm,
    NotManaged,
    NotFound,
    Conflict,
    Deactivated,
    ProtectionUnavailable,
    Locked,
    InvalidOperation,
}

impl fmt::Display for DidLifecyclePortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "DID lifecycle capability is unavailable",
            Self::UnsupportedNetwork => "DID network does not support this lifecycle adapter",
            Self::UnsupportedAlgorithm => "DID key algorithm is unsupported",
            Self::NotManaged => "DID is not managed by the current protected session",
            Self::NotFound => "DID document entry was not found",
            Self::Conflict => "DID document entry already exists or is still referenced",
            Self::Deactivated => "DID is deactivated",
            Self::ProtectionUnavailable => "protected DID key operation is unavailable",
            Self::Locked => "wallet must be unlocked for this DID operation",
            Self::InvalidOperation => "DID lifecycle operation is invalid",
        })
    }
}

impl Error for DidLifecyclePortError {}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableDidLifecycle;

impl DidLifecyclePort for UnavailableDidLifecycle {
    fn create(
        &self,
        _: &IdentityProfileId,
        _: MidnightNetwork,
    ) -> Result<DidResolution, DidLifecyclePortError> {
        Err(DidLifecyclePortError::Unavailable)
    }

    fn update(
        &self,
        _: &IdentityProfileId,
        _: &DidResolution,
        _: DidUpdate,
    ) -> Result<DidResolution, DidLifecyclePortError> {
        Err(DidLifecyclePortError::Unavailable)
    }

    fn deactivate(
        &self,
        _: &IdentityProfileId,
        _: &DidResolution,
    ) -> Result<DidResolution, DidLifecyclePortError> {
        Err(DidLifecyclePortError::Unavailable)
    }

    fn sign(
        &self,
        _: &IdentityProfileId,
        _: &DidResolution,
        _: &str,
        _: &[u8],
    ) -> Result<DidLifecycleSignature, DidLifecyclePortError> {
        Err(DidLifecyclePortError::Unavailable)
    }
}

impl DidJubjubChallengeSigningPort for UnavailableDidLifecycle {
    fn sign_jubjub_challenge(
        &self,
        _: &IdentityProfileId,
        _: &MidnightDid,
        _: &str,
        _: &[u8; 32],
        _: &mut DidJubjubChallengeDeriver<'_>,
    ) -> Result<DidJubjubChallengeSignature, DidLifecyclePortError> {
        Err(DidLifecyclePortError::Unavailable)
    }
}

fn parse_profile(value: String) -> Result<IdentityProfileId, DidOperationError> {
    IdentityProfileId::parse(value).map_err(DidOperationError::InvalidProfileIdentifier)
}

fn parse_did(value: String) -> Result<MidnightDid, DidOperationError> {
    MidnightDid::parse(value).map_err(DidOperationError::InvalidDid)
}

fn validate_confirmation(value: &DidOperationConfirmation) -> Result<(), DidOperationError> {
    if !value.confirmed {
        return Err(DidOperationError::ConfirmationRequired);
    }
    let title = value.title.trim();
    let summary = value.summary.trim();
    if title.is_empty()
        || summary.is_empty()
        || title.chars().count() > MAX_CONFIRMATION_TITLE_CHARACTERS
        || summary.chars().count() > MAX_CONFIRMATION_SUMMARY_CHARACTERS
        || title.chars().any(char::is_control)
        || summary.chars().any(char::is_control)
    {
        return Err(DidOperationError::InvalidConfirmation);
    }
    Ok(())
}

fn persist(
    service: &DidService,
    profile_id: IdentityProfileId,
    resolution: DidResolution,
) -> Result<DidRecordView, DidOperationError> {
    service
        .repository
        .upsert(DidRecord::new(profile_id.clone(), resolution.clone()))
        .map_err(DidOperationError::Persistence)?;
    Ok(super::record_view(service, &profile_id, &resolution))
}

fn current(
    service: &DidService,
    profile_id: &IdentityProfileId,
    did: &MidnightDid,
) -> Result<DidResolution, DidOperationError> {
    service
        .repository
        .get(profile_id, did)
        .map(DidRecord::into_resolution)
        .map_err(DidOperationError::Persistence)
}

impl CreateDidUseCase for DidService {
    fn execute(&self, command: CreateDidCommand) -> Result<DidRecordView, DidOperationError> {
        let profile_id = parse_profile(command.profile_id)?;
        let network = MidnightNetwork::parse(command.network.trim())
            .ok_or(DidOperationError::InvalidNetwork)?;
        let resolution = self
            .lifecycle
            .create(&profile_id, network)
            .map_err(DidOperationError::Lifecycle)?;
        persist(self, profile_id, resolution)
    }
}

impl UpdateDidUseCase for DidService {
    fn execute(&self, command: UpdateDidCommand) -> Result<DidRecordView, DidOperationError> {
        validate_confirmation(&command.confirmation)?;
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        let prior = current(self, &profile_id, &did)?;
        let resolution = self
            .lifecycle
            .update(&profile_id, &prior, command.operation)
            .map_err(DidOperationError::Lifecycle)?;
        persist(self, profile_id, resolution)
    }
}

impl DeactivateDidUseCase for DidService {
    fn execute(&self, command: DeactivateDidCommand) -> Result<DidRecordView, DidOperationError> {
        validate_confirmation(&command.confirmation)?;
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        let prior = current(self, &profile_id, &did)?;
        let resolution = self
            .lifecycle
            .deactivate(&profile_id, &prior)
            .map_err(DidOperationError::Lifecycle)?;
        persist(self, profile_id, resolution)
    }
}

impl SignDidPayloadUseCase for DidService {
    fn execute(
        &self,
        command: SignDidPayloadCommand,
    ) -> Result<DidSignatureView, DidOperationError> {
        validate_confirmation(&command.confirmation)?;
        if command.payload.is_empty() {
            return Err(DidOperationError::EmptyPayload);
        }
        if command.payload.len() > MAX_DID_SIGNING_PAYLOAD_BYTES {
            return Err(DidOperationError::PayloadTooLarge);
        }
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        let prior = current(self, &profile_id, &did)?;
        self.lifecycle
            .sign(
                &profile_id,
                &prior,
                command.method_id.trim(),
                &command.payload,
            )
            .map(|signature| DidSignatureView {
                method_id: signature.method_id,
                algorithm: signature.algorithm.as_str().to_owned(),
                signature_bytes: signature.signature_bytes,
            })
            .map_err(DidOperationError::Lifecycle)
    }
}

impl From<OpaqueIdError> for DidOperationError {
    fn from(error: OpaqueIdError) -> Self {
        Self::InvalidProfileIdentifier(error)
    }
}

impl From<MidnightDidError> for DidOperationError {
    fn from(error: MidnightDidError) -> Self {
        Self::InvalidDid(error)
    }
}

impl From<DidRecordRepositoryError> for DidOperationError {
    fn from(error: DidRecordRepositoryError) -> Self {
        Self::Persistence(error)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use oxid_identity_domain::{
        DID_CONTEXT, DidDocument, DidDocumentMetadata, DidDocumentParts, DidResolutionMetadata,
        DidResolutionSource, JWK_CONTEXT,
    };

    use super::*;
    use crate::{DidRecordRepository, DidResolutionPort, UnavailableDidResolver};

    const DID: &str =
        "did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const PROFILE: &str = "profile_lifecycle";

    fn resolution() -> DidResolution {
        let did = MidnightDid::parse(DID).expect("DID");
        DidResolution::new(
            DidDocument::new(DidDocumentParts {
                contexts: vec![DID_CONTEXT.to_owned(), JWK_CONTEXT.to_owned()],
                id: did.clone(),
                controllers: vec![did],
                also_known_as: Vec::new(),
                verification_methods: Vec::new(),
                relationships: Vec::new(),
                services: Vec::new(),
            })
            .expect("document"),
            DidDocumentMetadata::default(),
            DidResolutionMetadata::default(),
            DidResolutionSource::Standalone,
        )
    }

    struct TestRepository {
        get_error: Option<DidRecordRepositoryError>,
        upsert_error: Option<DidRecordRepositoryError>,
    }

    impl DidRecordRepository for TestRepository {
        fn upsert(&self, _: DidRecord) -> Result<(), DidRecordRepositoryError> {
            self.upsert_error.map_or(Ok(()), Err)
        }

        fn list(&self, _: &IdentityProfileId) -> Result<Vec<DidRecord>, DidRecordRepositoryError> {
            Ok(Vec::new())
        }

        fn get(
            &self,
            profile_id: &IdentityProfileId,
            _: &MidnightDid,
        ) -> Result<DidRecord, DidRecordRepositoryError> {
            self.get_error
                .map_or_else(|| Ok(DidRecord::new(profile_id.clone(), resolution())), Err)
        }

        fn remove(
            &self,
            _: &IdentityProfileId,
            _: &MidnightDid,
        ) -> Result<(), DidRecordRepositoryError> {
            Ok(())
        }
    }

    struct TestLifecycle {
        error: Option<DidLifecyclePortError>,
    }

    impl DidLifecyclePort for TestLifecycle {
        fn create(
            &self,
            _: &IdentityProfileId,
            _: MidnightNetwork,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            self.error.map_or_else(|| Ok(resolution()), Err)
        }

        fn update(
            &self,
            _: &IdentityProfileId,
            current: &DidResolution,
            _: DidUpdate,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            self.error.map_or_else(|| Ok(current.clone()), Err)
        }

        fn deactivate(
            &self,
            _: &IdentityProfileId,
            current: &DidResolution,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            self.error.map_or_else(|| Ok(current.clone()), Err)
        }

        fn sign(
            &self,
            _: &IdentityProfileId,
            _: &DidResolution,
            method_id: &str,
            _: &[u8],
        ) -> Result<DidLifecycleSignature, DidLifecyclePortError> {
            self.error.map_or_else(
                || {
                    Ok(DidLifecycleSignature {
                        method_id: method_id.to_owned(),
                        algorithm: DidKeyAlgorithm::Ed25519,
                        signature_bytes: vec![7; 64],
                    })
                },
                Err,
            )
        }
    }

    fn service(
        repository_error: (
            Option<DidRecordRepositoryError>,
            Option<DidRecordRepositoryError>,
        ),
        lifecycle_error: Option<DidLifecyclePortError>,
    ) -> DidService {
        let repository: Arc<dyn DidRecordRepository> = Arc::new(TestRepository {
            get_error: repository_error.0,
            upsert_error: repository_error.1,
        });
        let resolver: Arc<dyn DidResolutionPort> = Arc::new(UnavailableDidResolver);
        let lifecycle: Arc<dyn DidLifecyclePort> = Arc::new(TestLifecycle {
            error: lifecycle_error,
        });
        DidService::from_ports(repository, resolver, lifecycle)
    }

    fn confirmation(title: String, summary: String, confirmed: bool) -> DidOperationConfirmation {
        DidOperationConfirmation {
            title,
            summary,
            confirmed,
        }
    }

    fn valid_confirmation() -> DidOperationConfirmation {
        confirmation(
            "Authorize DID operation".to_owned(),
            "Review the public DID change".to_owned(),
            true,
        )
    }

    fn update_command(confirmation: DidOperationConfirmation) -> UpdateDidCommand {
        UpdateDidCommand {
            profile_id: PROFILE.to_owned(),
            did: DID.to_owned(),
            operation: DidUpdate::AddAlsoKnownAs {
                value: "https://example.test/identity".to_owned(),
            },
            confirmation,
        }
    }

    fn sign_command(payload: Vec<u8>) -> SignDidPayloadCommand {
        SignDidPayloadCommand {
            profile_id: PROFILE.to_owned(),
            did: DID.to_owned(),
            method_id: format!("{DID}#auth-1"),
            payload,
            confirmation: valid_confirmation(),
        }
    }

    #[test]
    fn confirmation_requires_explicit_intent_and_bounded_printable_text() {
        let cases = [
            (
                confirmation("Title".to_owned(), "Summary".to_owned(), false),
                DidOperationError::ConfirmationRequired,
            ),
            (
                confirmation(" ".to_owned(), "Summary".to_owned(), true),
                DidOperationError::InvalidConfirmation,
            ),
            (
                confirmation("Title".to_owned(), "\t".to_owned(), true),
                DidOperationError::InvalidConfirmation,
            ),
            (
                confirmation("x".repeat(97), "Summary".to_owned(), true),
                DidOperationError::InvalidConfirmation,
            ),
            (
                confirmation("Title".to_owned(), "x".repeat(513), true),
                DidOperationError::InvalidConfirmation,
            ),
            (
                confirmation("Title\nInjected".to_owned(), "Summary".to_owned(), true),
                DidOperationError::InvalidConfirmation,
            ),
        ];
        let service = service((None, None), None);
        for (confirmation, expected) in cases {
            assert_eq!(
                UpdateDidUseCase::execute(&service, update_command(confirmation)),
                Err(expected)
            );
        }
    }

    #[test]
    fn confirmation_accepts_exact_character_bounds() {
        let result = UpdateDidUseCase::execute(
            &service((None, None), None),
            update_command(confirmation("t".repeat(96), "s".repeat(512), true)),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_invalid_profile_network_and_did_inputs() {
        let service = service((None, None), None);
        assert!(matches!(
            CreateDidUseCase::execute(
                &service,
                CreateDidCommand {
                    profile_id: "".to_owned(),
                    network: "undeployed".to_owned(),
                }
            ),
            Err(DidOperationError::InvalidProfileIdentifier(_))
        ));
        for network in ["", "production", "MAINNET"] {
            assert_eq!(
                CreateDidUseCase::execute(
                    &service,
                    CreateDidCommand {
                        profile_id: PROFILE.to_owned(),
                        network: network.to_owned(),
                    }
                ),
                Err(DidOperationError::InvalidNetwork)
            );
        }
        let mut command = update_command(valid_confirmation());
        command.did = "did:example:not-midnight".to_owned();
        assert!(matches!(
            UpdateDidUseCase::execute(&service, command),
            Err(DidOperationError::InvalidDid(_))
        ));
    }

    #[test]
    fn signing_payload_bounds_fail_closed() {
        let service = service((None, None), None);
        for (payload, expected) in [
            (Vec::new(), DidOperationError::EmptyPayload),
            (
                vec![0x5a; MAX_DID_SIGNING_PAYLOAD_BYTES + 1],
                DidOperationError::PayloadTooLarge,
            ),
        ] {
            assert_eq!(
                SignDidPayloadUseCase::execute(&service, sign_command(payload)),
                Err(expected)
            );
        }
        assert!(
            SignDidPayloadUseCase::execute(
                &service,
                sign_command(vec![0x5a; MAX_DID_SIGNING_PAYLOAD_BYTES])
            )
            .is_ok()
        );
    }

    #[test]
    fn lifecycle_errors_preserve_their_closed_categories() {
        assert_eq!(
            CreateDidUseCase::execute(
                &service((None, None), Some(DidLifecyclePortError::Unavailable)),
                CreateDidCommand {
                    profile_id: PROFILE.to_owned(),
                    network: "undeployed".to_owned(),
                }
            ),
            Err(DidOperationError::Lifecycle(
                DidLifecyclePortError::Unavailable
            ))
        );
        assert_eq!(
            UpdateDidUseCase::execute(
                &service((None, None), Some(DidLifecyclePortError::Conflict)),
                update_command(valid_confirmation())
            ),
            Err(DidOperationError::Lifecycle(
                DidLifecyclePortError::Conflict
            ))
        );
        assert_eq!(
            SignDidPayloadUseCase::execute(
                &service((None, None), Some(DidLifecyclePortError::Locked)),
                sign_command(b"challenge".to_vec())
            ),
            Err(DidOperationError::Lifecycle(DidLifecyclePortError::Locked))
        );
    }

    #[test]
    fn repository_read_and_write_errors_are_not_collapsed() {
        assert_eq!(
            DeactivateDidUseCase::execute(
                &service((Some(DidRecordRepositoryError::Integrity), None), None),
                DeactivateDidCommand {
                    profile_id: PROFILE.to_owned(),
                    did: DID.to_owned(),
                    confirmation: valid_confirmation(),
                }
            ),
            Err(DidOperationError::Persistence(
                DidRecordRepositoryError::Integrity
            ))
        );
        assert_eq!(
            CreateDidUseCase::execute(
                &service(
                    (None, Some(DidRecordRepositoryError::CapacityExceeded)),
                    None
                ),
                CreateDidCommand {
                    profile_id: PROFILE.to_owned(),
                    network: "undeployed".to_owned(),
                }
            ),
            Err(DidOperationError::Persistence(
                DidRecordRepositoryError::CapacityExceeded
            ))
        );
    }

    #[test]
    fn signing_failures_do_not_echo_payload_or_confirmation_text() {
        let payload = b"private-signing-payload-sentinel".to_vec();
        let mut command = sign_command(payload.clone());
        command.confirmation.summary = "private-confirmation-sentinel".to_owned();
        let error = SignDidPayloadUseCase::execute(
            &service((None, None), Some(DidLifecyclePortError::Locked)),
            command,
        )
        .expect_err("locked signing must fail");
        let diagnostic = format!("{error:?} {error}");
        assert!(!diagnostic.contains(std::str::from_utf8(&payload).expect("UTF-8 sentinel")));
        assert!(!diagnostic.contains("private-confirmation-sentinel"));
        assert_eq!(
            error,
            DidOperationError::Lifecycle(DidLifecyclePortError::Locked)
        );
    }

    #[test]
    fn unavailable_lifecycle_rejects_every_operation_without_a_payload() {
        let lifecycle = UnavailableDidLifecycle;
        let profile = IdentityProfileId::parse(PROFILE).expect("profile");
        let current = resolution();
        let did = MidnightDid::parse(DID).expect("DID");
        assert_eq!(
            lifecycle.create(&profile, MidnightNetwork::Undeployed),
            Err(DidLifecyclePortError::Unavailable)
        );
        assert_eq!(
            lifecycle.update(
                &profile,
                &current,
                DidUpdate::RemoveAlsoKnownAs {
                    value: "https://example.test/identity".to_owned(),
                }
            ),
            Err(DidLifecyclePortError::Unavailable)
        );
        assert_eq!(
            lifecycle.deactivate(&profile, &current),
            Err(DidLifecyclePortError::Unavailable)
        );
        assert_eq!(
            lifecycle.sign(&profile, &current, "#auth-1", b"challenge"),
            Err(DidLifecyclePortError::Unavailable)
        );
        let mut derive = |_: &[u8; 32], _: &[u8; 32]| Ok([0; 32]);
        assert_eq!(
            lifecycle.sign_jubjub_challenge(&profile, &did, "#assert-1", &[0; 32], &mut derive),
            Err(DidLifecyclePortError::Unavailable)
        );
    }
}
