// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt};

use oxid_foundation::OpaqueIdError;
use oxid_identity_domain::{
    DidPublicationState, DidRecord, DidResolution, IdentityProfileId, JwkCurve, MidnightDid,
    MidnightDidError, MidnightNetwork, VerificationRelationship,
};

use crate::{DidOperationError, DidRecordRepositoryError, DidRecordView, DidService};

pub const MAX_DID_SIGNING_PAYLOAD_BYTES: usize = 64 * 1024;
mod intent;
mod serialization;
use crate::{
    AcceptedCredentialIssuanceContext, AcceptedCredentialIssuanceFlow,
    AcceptedCredentialPresentationContext, AcceptedCredentialPresentationFlow,
    AcceptedSelfIssuedAuthenticationContext, AcceptedSelfIssuedAuthenticationFlow,
    CanonicalCredentialIssuancePayloadDigest, CanonicalCredentialPresentationBundleDigest,
    CanonicalSelfIssuedAuthenticationPayloadDigest, CredentialIssuanceFlowError,
    CredentialIssuanceFlowService, CredentialPresentationFlowError,
    CredentialPresentationFlowService, DidApprovalCapability, DidApprovalError,
    DidApprovalOperation, DidApprovalRequest, DidApprovalService,
    SelfIssuedAuthenticationFlowError, SelfIssuedAuthenticationFlowService,
};
use intent::{
    canonical_component_id, deactivate_request, normalize_update, sign_request, update_request,
};
use serialization::operation_lock;

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
pub struct CreateDidCommand {
    pub profile_id: String,
    pub network: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateDidCommand {
    pub profile_id: String,
    pub did: String,
    pub operation: DidUpdate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeactivateDidCommand {
    pub profile_id: String,
    pub did: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SignDidPayloadCommand<'a> {
    pub profile_id: String,
    pub did: String,
    pub method_id: String,
    pub payload: &'a [u8],
}

impl fmt::Debug for SignDidPayloadCommand<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignDidPayloadCommand")
            .field("profile_id", &self.profile_id)
            .field("did", &self.did)
            .field("method_id", &self.method_id)
            .field("payload", &"[REDACTED]")
            .finish()
    }
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
        command: SignDidPayloadCommand<'_>,
    ) -> Result<DidSignatureView, DidOperationError>;
}

/// Exact JWS signing input plus the accepted issuance authority that permits
/// this one holder-proof signature. The authority is deliberately moved.
pub struct SignCredentialIssuancePayloadCommand<'a> {
    pub profile_id: String,
    pub did: String,
    pub method_id: String,
    pub algorithm: DidKeyAlgorithm,
    pub flow_id: String,
    pub session_id: String,
    pub payload: &'a [u8],
    pub authority: AcceptedCredentialIssuanceFlow,
}

impl fmt::Debug for SignCredentialIssuancePayloadCommand<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignCredentialIssuancePayloadCommand")
            .field("profile_id", &self.profile_id)
            .field("did", &self.did)
            .field("method_id", &self.method_id)
            .field("algorithm", &self.algorithm)
            .field("flow_id", &self.flow_id)
            .field("session_id", &self.session_id)
            .field("payload", &"[REDACTED]")
            .field("authority", &"[REDACTED]")
            .finish()
    }
}

pub trait SignCredentialIssuancePayloadUseCase: Send + Sync {
    fn execute(
        &self,
        command: SignCredentialIssuancePayloadCommand<'_>,
    ) -> Result<DidSignatureView, DidOperationError>;
}

/// Exact canonical SIOPv2 ID-token JWS signing input plus its moved authority.
pub struct SignSelfIssuedAuthenticationPayloadCommand<'a> {
    pub profile_id: String,
    pub did: String,
    pub method_id: String,
    pub algorithm: DidKeyAlgorithm,
    pub flow_id: String,
    pub session_id: String,
    pub payload: &'a [u8],
    pub authority: AcceptedSelfIssuedAuthenticationFlow,
}

impl fmt::Debug for SignSelfIssuedAuthenticationPayloadCommand<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignSelfIssuedAuthenticationPayloadCommand")
            .field("profile_id", &self.profile_id)
            .field("did", &self.did)
            .field("method_id", &self.method_id)
            .field("algorithm", &self.algorithm)
            .field("flow_id", &self.flow_id)
            .field("session_id", &self.session_id)
            .field("payload", &"[REDACTED]")
            .field("authority", &"[REDACTED]")
            .finish()
    }
}

pub trait SignSelfIssuedAuthenticationPayloadUseCase: Send + Sync {
    fn execute(
        &self,
        command: SignSelfIssuedAuthenticationPayloadCommand<'_>,
    ) -> Result<DidSignatureView, DidOperationError>;
}

pub struct SignCredentialPresentationBundleCommand<'a> {
    pub profile_id: String,
    pub did: String,
    pub method_id: String,
    pub flow_id: String,
    pub session_id: String,
    pub credential_id: String,
    pub verifier: String,
    pub authorization_payload: [u8; 32],
    pub presentation_root: [u8; 32],
    pub verifier_challenge_hash: [u8; 32],
    pub created_at_seconds: u64,
    pub expected_public_key: [u8; 32],
    pub derive_challenge: &'a mut DidJubjubChallengeDeriver<'a>,
    pub authority: AcceptedCredentialPresentationFlow,
}

impl fmt::Debug for SignCredentialPresentationBundleCommand<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignCredentialPresentationBundleCommand")
            .field("profile_id", &self.profile_id)
            .field("did", &self.did)
            .field("method_id", &self.method_id)
            .field("flow_id", &self.flow_id)
            .field("session_id", &self.session_id)
            .field("credential_id", &self.credential_id)
            .field("verifier", &self.verifier)
            .field("inputs", &"[REDACTED]")
            .field("authority", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialPresentationSignatureBundle {
    pub authorization: DidSignatureView,
    pub holder_proof: DidJubjubChallengeSignature,
}

pub trait SignCredentialPresentationBundleUseCase: Send + Sync {
    fn execute(
        &self,
        command: SignCredentialPresentationBundleCommand<'_>,
    ) -> Result<CredentialPresentationSignatureBundle, DidOperationError>;
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

fn hash(service: &DidService) -> Result<&dyn oxid_platform_ports::Sha256Port, DidOperationError> {
    service
        .approvals
        .as_ref()
        .map(|(_, hash)| hash.as_ref())
        .ok_or(DidOperationError::Approval(DidApprovalError::Unavailable))
}

fn approvals(service: &DidService) -> Result<&DidApprovalService, DidOperationError> {
    service
        .approvals
        .as_ref()
        .map(|(approval, _)| approval.as_ref())
        .ok_or(DidOperationError::Approval(DidApprovalError::Unavailable))
}

fn credential_issuance(
    service: &DidService,
) -> Result<
    (
        &CredentialIssuanceFlowService,
        &dyn oxid_platform_ports::Sha256Port,
    ),
    DidOperationError,
> {
    service
        .credential_issuance
        .as_ref()
        .map(|(authority, hash)| (authority.as_ref(), hash.as_ref()))
        .ok_or(DidOperationError::CredentialIssuance(
            CredentialIssuanceFlowError::Unavailable,
        ))
}

fn self_issued_authentication(
    service: &DidService,
) -> Result<
    (
        &SelfIssuedAuthenticationFlowService,
        &dyn oxid_platform_ports::Sha256Port,
    ),
    DidOperationError,
> {
    service
        .self_issued_authentication
        .as_ref()
        .map(|(authority, hash)| (authority.as_ref(), hash.as_ref()))
        .ok_or(DidOperationError::SelfIssuedAuthentication(
            SelfIssuedAuthenticationFlowError::Unavailable,
        ))
}

fn credential_presentation(
    service: &DidService,
) -> Result<
    (
        &CredentialPresentationFlowService,
        &dyn oxid_platform_ports::Sha256Port,
        &dyn DidJubjubChallengeSigningPort,
    ),
    DidOperationError,
> {
    service
        .credential_presentation
        .as_ref()
        .map(|(authority, hash, challenge)| (authority.as_ref(), hash.as_ref(), challenge.as_ref()))
        .ok_or(DidOperationError::CredentialPresentation(
            CredentialPresentationFlowError::Unavailable,
        ))
}

// The caller holds the profile/DID operation lock from this re-read through
// the lifecycle effect and persistence. Approval callbacks run outside the lock.
// Re-read after approval, then atomically spend the exact reconstructed intent.
// No callback or other fallible work occurs between consumption and the effect.
fn consume<O: DidApprovalOperation>(
    service: &DidService,
    prior: &DidRecord,
    capability: &DidApprovalCapability<O>,
    expected: &DidApprovalRequest<O>,
) -> Result<(), DidOperationError> {
    if current(
        service,
        prior.profile_id(),
        prior.resolution().document().id(),
    )? != *prior
    {
        return Err(DidOperationError::RetainedRecordChanged);
    }
    approvals(service)?
        .consume(capability, expected)
        .map_err(DidOperationError::Approval)
}

fn persist(
    service: &DidService,
    profile_id: IdentityProfileId,
    resolution: DidResolution,
    publication_state: DidPublicationState,
) -> Result<DidRecordView, DidOperationError> {
    let record = DidRecord::new(profile_id, resolution).with_publication_state(publication_state);
    service
        .repository
        .upsert(record.clone())
        .map_err(DidOperationError::Persistence)?;
    Ok(super::record_view(service, &record))
}

fn current(
    service: &DidService,
    profile_id: &IdentityProfileId,
    did: &MidnightDid,
) -> Result<DidRecord, DidOperationError> {
    service
        .repository
        .get(profile_id, did)
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
        persist(
            self,
            profile_id,
            resolution,
            DidPublicationState::Unpublished,
        )
    }
}

impl UpdateDidUseCase for DidService {
    fn execute(&self, command: UpdateDidCommand) -> Result<DidRecordView, DidOperationError> {
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        let operation = normalize_update(&did, command.operation)?;
        approvals(self)?;
        let prior = current(self, &profile_id, &did)?;
        let publication_state = prior.publication_state();
        let request = update_request(hash(self)?, &profile_id, &did, &operation);
        let capability = approvals(self)?
            .request(&request)
            .map_err(DidOperationError::Approval)?;
        let lock = operation_lock(&profile_id, &did)?;
        let _guard = lock.lock().map_err(|_| serialization::unavailable())?;
        consume(
            self,
            &prior,
            &capability,
            &update_request(hash(self)?, &profile_id, &did, &operation),
        )?;
        let resolution = self
            .lifecycle
            .update(&profile_id, prior.resolution(), operation)
            .map_err(DidOperationError::Lifecycle)?;
        persist(self, profile_id, resolution, publication_state)
    }
}

impl DeactivateDidUseCase for DidService {
    fn execute(&self, command: DeactivateDidCommand) -> Result<DidRecordView, DidOperationError> {
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        approvals(self)?;
        let prior = current(self, &profile_id, &did)?;
        let publication_state = prior.publication_state();
        let request = deactivate_request(hash(self)?, &profile_id, &did);
        let capability = approvals(self)?
            .request(&request)
            .map_err(DidOperationError::Approval)?;
        let lock = operation_lock(&profile_id, &did)?;
        let _guard = lock.lock().map_err(|_| serialization::unavailable())?;
        consume(
            self,
            &prior,
            &capability,
            &deactivate_request(hash(self)?, &profile_id, &did),
        )?;
        let resolution = self
            .lifecycle
            .deactivate(&profile_id, prior.resolution())
            .map_err(DidOperationError::Lifecycle)?;
        persist(self, profile_id, resolution, publication_state)
    }
}

impl SignDidPayloadUseCase for DidService {
    fn execute(
        &self,
        command: SignDidPayloadCommand<'_>,
    ) -> Result<DidSignatureView, DidOperationError> {
        if command.payload.is_empty() {
            return Err(DidOperationError::EmptyPayload);
        }
        if command.payload.len() > MAX_DID_SIGNING_PAYLOAD_BYTES {
            return Err(DidOperationError::PayloadTooLarge);
        }
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        let method_id = canonical_component_id(&did, &command.method_id)?;
        approvals(self)?;
        let prior = current(self, &profile_id, &did)?;
        let request = sign_request(hash(self)?, &profile_id, &did, &method_id, command.payload);
        let capability = approvals(self)?
            .request(&request)
            .map_err(DidOperationError::Approval)?;
        let lock = operation_lock(&profile_id, &did)?;
        let _guard = lock.lock().map_err(|_| serialization::unavailable())?;
        consume(
            self,
            &prior,
            &capability,
            &sign_request(hash(self)?, &profile_id, &did, &method_id, command.payload),
        )?;
        self.lifecycle
            .sign(&profile_id, prior.resolution(), &method_id, command.payload)
            .map(|signature| DidSignatureView {
                method_id: signature.method_id,
                algorithm: signature.algorithm.as_str().to_owned(),
                signature_bytes: signature.signature_bytes,
            })
            .map_err(DidOperationError::Lifecycle)
    }
}

impl SignCredentialIssuancePayloadUseCase for DidService {
    fn execute(
        &self,
        command: SignCredentialIssuancePayloadCommand<'_>,
    ) -> Result<DidSignatureView, DidOperationError> {
        if command.payload.is_empty() {
            return Err(DidOperationError::EmptyPayload);
        }
        if command.payload.len() > MAX_DID_SIGNING_PAYLOAD_BYTES {
            return Err(DidOperationError::PayloadTooLarge);
        }
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        let method_id = canonical_component_id(&did, &command.method_id)?;
        let (issuance, hash) = credential_issuance(self)?;
        let lock = operation_lock(&profile_id, &did)?;
        let _guard = lock.lock().map_err(|_| serialization::unavailable())?;

        // This is the authoritative read: the current retained DID and exact
        // authentication method are checked under the same lock held through
        // capability consumption and the custody signing effect.
        let prior = current(self, &profile_id, &did)?;
        let document = prior.resolution().document();
        let method = document
            .verification_methods()
            .iter()
            .find(|method| method.id() == method_id)
            .ok_or(DidOperationError::Lifecycle(
                DidLifecyclePortError::NotFound,
            ))?;
        let current_algorithm = match method.public_key_jwk().curve() {
            JwkCurve::Ed25519 => DidKeyAlgorithm::Ed25519,
            JwkCurve::P256 => DidKeyAlgorithm::P256,
            _ => {
                return Err(DidOperationError::Lifecycle(
                    DidLifecyclePortError::UnsupportedAlgorithm,
                ));
            }
        };
        if current_algorithm != command.algorithm
            || method.controller() != &did
            || !document.relationships().iter().any(|relationship| {
                relationship.relationship() == VerificationRelationship::Authentication
                    && relationship.method_ids().iter().any(|id| id == &method_id)
            })
        {
            return Err(DidOperationError::CredentialIssuance(
                CredentialIssuanceFlowError::FlowMismatch,
            ));
        }
        let context = AcceptedCredentialIssuanceContext::new(
            profile_id.clone(),
            did.clone(),
            method_id.clone(),
            command.flow_id,
            command.session_id,
        );
        issuance
            .bind_and_consume_for_signing(&command.authority, &context, || {
                CanonicalCredentialIssuancePayloadDigest::from_sha256(hash.sha256(command.payload))
            })
            .map_err(DidOperationError::CredentialIssuance)?;
        self.lifecycle
            .sign(&profile_id, prior.resolution(), &method_id, command.payload)
            .map(|signature| DidSignatureView {
                method_id: signature.method_id,
                algorithm: signature.algorithm.as_str().to_owned(),
                signature_bytes: signature.signature_bytes,
            })
            .map_err(DidOperationError::Lifecycle)
    }
}

impl SignSelfIssuedAuthenticationPayloadUseCase for DidService {
    fn execute(
        &self,
        command: SignSelfIssuedAuthenticationPayloadCommand<'_>,
    ) -> Result<DidSignatureView, DidOperationError> {
        if command.payload.is_empty() {
            return Err(DidOperationError::EmptyPayload);
        }
        if command.payload.len() > MAX_DID_SIGNING_PAYLOAD_BYTES {
            return Err(DidOperationError::PayloadTooLarge);
        }
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        let method_id = canonical_component_id(&did, &command.method_id)?;
        let (authentication, hash) = self_issued_authentication(self)?;
        let lock = operation_lock(&profile_id, &did)?;
        let _guard = lock.lock().map_err(|_| serialization::unavailable())?;

        // The retained DID and exact authentication relationship are authoritative
        // only when re-read under the lock held through consumption and signing.
        let prior = current(self, &profile_id, &did)?;
        let document = prior.resolution().document();
        let method = document
            .verification_methods()
            .iter()
            .find(|method| method.id() == method_id)
            .ok_or(DidOperationError::Lifecycle(
                DidLifecyclePortError::NotFound,
            ))?;
        let current_algorithm = match method.public_key_jwk().curve() {
            JwkCurve::Ed25519 => DidKeyAlgorithm::Ed25519,
            JwkCurve::P256 => DidKeyAlgorithm::P256,
            _ => {
                return Err(DidOperationError::Lifecycle(
                    DidLifecyclePortError::UnsupportedAlgorithm,
                ));
            }
        };
        if current_algorithm != command.algorithm
            || method.controller() != &did
            || !document.relationships().iter().any(|relationship| {
                relationship.relationship() == VerificationRelationship::Authentication
                    && relationship.method_ids().iter().any(|id| id == &method_id)
            })
        {
            return Err(DidOperationError::SelfIssuedAuthentication(
                SelfIssuedAuthenticationFlowError::FlowMismatch,
            ));
        }
        let context = AcceptedSelfIssuedAuthenticationContext::new(
            profile_id.clone(),
            did.clone(),
            method_id.clone(),
            command.flow_id,
            command.session_id,
        );
        authentication
            .bind_and_consume_for_signing(&command.authority, &context, || {
                CanonicalSelfIssuedAuthenticationPayloadDigest::from_sha256(
                    hash.sha256(command.payload),
                )
            })
            .map_err(DidOperationError::SelfIssuedAuthentication)?;
        self.lifecycle
            .sign(&profile_id, prior.resolution(), &method_id, command.payload)
            .map(|signature| DidSignatureView {
                method_id: signature.method_id,
                algorithm: signature.algorithm.as_str().to_owned(),
                signature_bytes: signature.signature_bytes,
            })
            .map_err(DidOperationError::Lifecycle)
    }
}

impl SignCredentialPresentationBundleUseCase for DidService {
    fn execute(
        &self,
        command: SignCredentialPresentationBundleCommand<'_>,
    ) -> Result<CredentialPresentationSignatureBundle, DidOperationError> {
        let profile_id = parse_profile(command.profile_id)?;
        let did = parse_did(command.did)?;
        let method_id = canonical_component_id(&did, &command.method_id)?;
        let (presentation, hash, challenge_signing) = credential_presentation(self)?;
        let lock = operation_lock(&profile_id, &did)?;
        let _guard = lock.lock().map_err(|_| serialization::unavailable())?;

        let prior = current(self, &profile_id, &did)?;
        let document = prior.resolution().document();
        let method = document
            .verification_methods()
            .iter()
            .find(|method| method.id() == method_id)
            .ok_or(DidOperationError::Lifecycle(
                DidLifecyclePortError::NotFound,
            ))?;
        let managed = self
            .lifecycle
            .managed_method_ids(&profile_id, prior.resolution())
            .map_err(DidOperationError::Lifecycle)?;
        if method.public_key_jwk().curve() != JwkCurve::Jubjub
            || method.controller() != &did
            || !managed.iter().any(|candidate| candidate == &method_id)
            || !document.relationships().iter().any(|relationship| {
                relationship.relationship() == VerificationRelationship::AssertionMethod
                    && relationship.method_ids().iter().any(|id| id == &method_id)
            })
        {
            return Err(DidOperationError::CredentialPresentation(
                CredentialPresentationFlowError::FlowMismatch,
            ));
        }
        let context = AcceptedCredentialPresentationContext::new(
            profile_id.clone(),
            command.flow_id,
            command.session_id,
            command.credential_id,
        );
        presentation
            .bind_and_consume_for_signing(&command.authority, &context, || {
                let mut canonical = Vec::with_capacity(32 * 5 + 8);
                canonical.extend_from_slice(b"oxid:credential-presentation-bundle:v1\0");
                canonical.extend_from_slice(&command.authorization_payload);
                canonical.extend_from_slice(&command.presentation_root);
                canonical.extend_from_slice(&command.verifier_challenge_hash);
                canonical.extend_from_slice(&command.created_at_seconds.to_be_bytes());
                canonical.extend_from_slice(&command.expected_public_key);
                canonical.extend_from_slice(command.verifier.as_bytes());
                CanonicalCredentialPresentationBundleDigest::from_sha256(hash.sha256(&canonical))
            })
            .map_err(DidOperationError::CredentialPresentation)?;

        // One closed, ordered operation: neither signature is returned unless
        // both protected effects complete successfully.
        let authorization = self
            .lifecycle
            .sign(
                &profile_id,
                prior.resolution(),
                &method_id,
                &command.authorization_payload,
            )
            .map(|signature| DidSignatureView {
                method_id: signature.method_id,
                algorithm: signature.algorithm.as_str().to_owned(),
                signature_bytes: signature.signature_bytes,
            })
            .map_err(DidOperationError::Lifecycle)?;
        let holder_proof = challenge_signing
            .sign_jubjub_challenge(
                &profile_id,
                &did,
                &method_id,
                &command.expected_public_key,
                command.derive_challenge,
            )
            .map_err(DidOperationError::Lifecycle)?;
        Ok(CredentialPresentationSignatureBundle {
            authorization,
            holder_proof,
        })
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
pub(crate) mod tests {
    mod approval_enforcement;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use oxid_identity_domain::{
        DID_CONTEXT, DidDocument, DidDocumentMetadata, DidDocumentParts, DidResolutionMetadata,
        DidResolutionSource, JWK_CONTEXT,
    };

    use super::*;
    use crate::{DidRecordRepository, DidResolutionPort, UnavailableDidResolver};

    // Collision-free recording test double, not a cryptographic implementation.
    // Production composition injects SystemSha256; framing is asserted separately.
    #[derive(Default)]
    pub(crate) struct TestHash(Mutex<Vec<Vec<u8>>>);
    impl oxid_platform_ports::Sha256Port for TestHash {
        fn sha256(&self, payload: &[u8]) -> [u8; 32] {
            let mut frames = self.0.lock().expect("frames");
            let index = frames
                .iter()
                .position(|frame| frame == payload)
                .unwrap_or_else(|| {
                    frames.push(payload.to_vec());
                    frames.len() - 1
                });
            let mut digest = [0; 32];
            digest[..8].copy_from_slice(&(index as u64).to_be_bytes());
            digest
        }
    }

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
        drift: bool,
        reads: AtomicUsize,
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
            self.get_error.map_or_else(
                || {
                    let changed = self.reads.fetch_add(1, Ordering::SeqCst) > 0 && self.drift;
                    Ok(
                        DidRecord::new(profile_id.clone(), resolution()).with_publication_state(
                            if changed {
                                DidPublicationState::Published
                            } else {
                                DidPublicationState::Unknown
                            },
                        ),
                    )
                },
                Err,
            )
        }

        fn remove(
            &self,
            _: &IdentityProfileId,
            _: &MidnightDid,
        ) -> Result<(), DidRecordRepositoryError> {
            Ok(())
        }
    }

    struct SignCall {
        profile_id: String,
        did: String,
        method_id: String,
        payload: Vec<u8>,
    }

    struct TestLifecycle {
        error: Option<DidLifecyclePortError>,
        update_calls: AtomicUsize,
        updates: Mutex<Vec<DidUpdate>>,
        deactivate_calls: AtomicUsize,
        sign_calls: Mutex<Vec<SignCall>>,
    }

    impl TestLifecycle {
        fn new(error: Option<DidLifecyclePortError>) -> Self {
            Self {
                error,
                update_calls: AtomicUsize::new(0),
                updates: Mutex::new(Vec::new()),
                deactivate_calls: AtomicUsize::new(0),
                sign_calls: Mutex::new(Vec::new()),
            }
        }
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
            operation: DidUpdate,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            self.updates.lock().unwrap().push(operation);
            self.update_calls.fetch_add(1, Ordering::SeqCst);
            self.error.map_or_else(|| Ok(current.clone()), Err)
        }

        fn deactivate(
            &self,
            _: &IdentityProfileId,
            current: &DidResolution,
        ) -> Result<DidResolution, DidLifecyclePortError> {
            self.deactivate_calls.fetch_add(1, Ordering::Relaxed);
            self.error.map_or_else(|| Ok(current.clone()), Err)
        }

        fn sign(
            &self,
            profile_id: &IdentityProfileId,
            current: &DidResolution,
            method_id: &str,
            payload: &[u8],
        ) -> Result<DidLifecycleSignature, DidLifecyclePortError> {
            self.sign_calls.lock().expect("sign calls").push(SignCall {
                profile_id: profile_id.as_str().to_owned(),
                did: current.document().id().as_str().to_owned(),
                method_id: method_id.to_owned(),
                payload: payload.to_vec(),
            });
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

    fn service_with_lifecycle(
        repository_error: (
            Option<DidRecordRepositoryError>,
            Option<DidRecordRepositoryError>,
        ),
        lifecycle_error: Option<DidLifecyclePortError>,
    ) -> (DidService, Arc<TestLifecycle>) {
        let repository: Arc<dyn DidRecordRepository> = Arc::new(TestRepository {
            get_error: repository_error.0,
            upsert_error: repository_error.1,
            drift: false,
            reads: AtomicUsize::new(0),
        });
        let resolver: Arc<dyn DidResolutionPort> = Arc::new(UnavailableDidResolver);
        let lifecycle = Arc::new(TestLifecycle::new(lifecycle_error));
        let service = DidService::from_ports(repository, resolver, lifecycle.clone())
            .with_approvals(
                Arc::new(crate::approval::tests::service().0),
                Arc::new(TestHash::default()),
            );
        (service, lifecycle)
    }

    fn service(
        repository_error: (
            Option<DidRecordRepositoryError>,
            Option<DidRecordRepositoryError>,
        ),
        lifecycle_error: Option<DidLifecyclePortError>,
    ) -> DidService {
        service_with_lifecycle(repository_error, lifecycle_error).0
    }

    fn update_command() -> UpdateDidCommand {
        UpdateDidCommand {
            profile_id: PROFILE.to_owned(),
            did: DID.to_owned(),
            operation: DidUpdate::AddAlsoKnownAs {
                value: "https://example.test/identity".to_owned(),
            },
        }
    }

    fn sign_command(payload: &[u8]) -> SignDidPayloadCommand<'_> {
        SignDidPayloadCommand {
            profile_id: PROFILE.to_owned(),
            did: DID.to_owned(),
            method_id: format!("{DID}#auth-1"),
            payload,
        }
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
        let mut command = update_command();
        command.did = "did:example:not-midnight".to_owned();
        assert!(matches!(
            UpdateDidUseCase::execute(&service, command),
            Err(DidOperationError::InvalidDid(_))
        ));
    }

    #[test]
    fn signing_payload_bounds_fail_closed_and_exact_maximum_is_forwarded() {
        let (service, lifecycle) = service_with_lifecycle((None, None), None);
        for (payload, expected) in [
            (Vec::new(), DidOperationError::EmptyPayload),
            (
                vec![0x5a; MAX_DID_SIGNING_PAYLOAD_BYTES + 1],
                DidOperationError::PayloadTooLarge,
            ),
        ] {
            assert_eq!(
                SignDidPayloadUseCase::execute(&service, sign_command(&payload)),
                Err(expected)
            );
        }
        assert!(lifecycle.sign_calls.lock().expect("sign calls").is_empty());

        let payload: Vec<u8> = (0..MAX_DID_SIGNING_PAYLOAD_BYTES)
            .map(|index| (index % 251) as u8)
            .collect();
        assert!(SignDidPayloadUseCase::execute(&service, sign_command(&payload)).is_ok());

        let calls = lifecycle.sign_calls.lock().expect("sign calls");
        assert_eq!(calls.len(), 1);
        let call = &calls[0];
        assert_eq!(call.profile_id, PROFILE);
        assert_eq!(call.did, DID);
        assert_eq!(call.method_id, format!("{DID}#auth-1"));
        assert_eq!(call.payload.len(), MAX_DID_SIGNING_PAYLOAD_BYTES);
        assert!(
            call.payload
                .iter()
                .copied()
                .eq((0..MAX_DID_SIGNING_PAYLOAD_BYTES).map(|index| (index % 251) as u8)),
            "forwarded signing payload differs"
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
                update_command()
            ),
            Err(DidOperationError::Lifecycle(
                DidLifecyclePortError::Conflict
            ))
        );
        assert_eq!(
            SignDidPayloadUseCase::execute(
                &service((None, None), Some(DidLifecyclePortError::Locked)),
                sign_command(b"challenge")
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
    fn signing_failures_do_not_echo_payload() {
        let payload = b"private-signing-payload-sentinel".to_vec();
        let command = sign_command(&payload);
        let command_debug = format!("{command:?}");
        assert!(command_debug.contains("[REDACTED]"));
        assert!(!command_debug.contains("private-signing-payload-sentinel"));
        let error = SignDidPayloadUseCase::execute(
            &service((None, None), Some(DidLifecyclePortError::Locked)),
            command,
        )
        .expect_err("locked signing must fail");
        let diagnostic = format!("{error:?} {error}");
        assert!(!diagnostic.contains(std::str::from_utf8(&payload).expect("UTF-8 sentinel")));
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
