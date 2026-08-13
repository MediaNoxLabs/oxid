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
