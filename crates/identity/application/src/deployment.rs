// SPDX-License-Identifier: Apache-2.0

//! Durable application model for ledger-backed DID deployment.
//!
//! The state machine is intentionally adapter-neutral. Transaction bytes,
//! controller witnesses, proofs, and composition randomness remain in native
//! adapters; the application retains only safe progress and inclusion evidence.

use std::{error::Error, fmt};

use oxid_foundation::{UnixTimestampMillis, opaque_id_type};
use oxid_identity_domain::{IdentityProfileId, MidnightDid, MidnightNetwork};

opaque_id_type! {
    pub struct DidDeploymentOperationId;
}

/// Durable user-visible phases of one native DID deployment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidDeploymentState {
    Composing,
    Funding,
    Proving,
    Submitting,
    Confirming,
    Resolving,
    Ready,
    RetryableFailure,
    OutcomeUnknown,
}

impl DidDeploymentState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Composing => "composing",
            Self::Funding => "funding",
            Self::Proving => "proving",
            Self::Submitting => "submitting",
            Self::Confirming => "confirming",
            Self::Resolving => "resolving",
            Self::Ready => "ready",
            Self::RetryableFailure => "retryable_failure",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    #[must_use]
    pub const fn terminal(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Bounded, secret-free reason why an operation paused for a retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidDeploymentFailure {
    ProtectionLocked,
    AccountUnavailable,
    FundingUnavailable,
    InsufficientDust,
    ProvingUnavailable,
    SubmissionRejected,
    ResolutionUnavailable,
    ResolutionMismatch,
    PersistenceUnavailable,
}

impl DidDeploymentFailure {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProtectionLocked => "protection_locked",
            Self::AccountUnavailable => "account_unavailable",
            Self::FundingUnavailable => "funding_unavailable",
            Self::InsufficientDust => "insufficient_dust",
            Self::ProvingUnavailable => "proving_unavailable",
            Self::SubmissionRejected => "submission_rejected",
            Self::ResolutionUnavailable => "resolution_unavailable",
            Self::ResolutionMismatch => "resolution_mismatch",
            Self::PersistenceUnavailable => "persistence_unavailable",
        }
    }
}

/// Safe durable metadata for one ledger-backed DID deployment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidDeploymentOperation {
    operation_id: DidDeploymentOperationId,
    profile_id: IdentityProfileId,
    network: MidnightNetwork,
    state: DidDeploymentState,
    resume_from: Option<DidDeploymentState>,
    failure: Option<DidDeploymentFailure>,
    did: Option<MidnightDid>,
    submission_id: Option<String>,
    transaction_hash_hex: Option<String>,
    block_hash_hex: Option<String>,
    block_height: Option<u64>,
    created_at: UnixTimestampMillis,
    updated_at: UnixTimestampMillis,
}

/// Complete safe snapshot used by persistence adapters. It deliberately omits
/// composition randomness, transaction bytes, proofs, and controller material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidDeploymentOperationParts {
    pub operation_id: DidDeploymentOperationId,
    pub profile_id: IdentityProfileId,
    pub network: MidnightNetwork,
    pub state: DidDeploymentState,
    pub resume_from: Option<DidDeploymentState>,
    pub failure: Option<DidDeploymentFailure>,
    pub did: Option<MidnightDid>,
    pub submission_id: Option<String>,
    pub transaction_hash_hex: Option<String>,
    pub block_hash_hex: Option<String>,
    pub block_height: Option<u64>,
    pub created_at: UnixTimestampMillis,
    pub updated_at: UnixTimestampMillis,
}

impl DidDeploymentOperation {
    pub fn new(
        operation_id: DidDeploymentOperationId,
        profile_id: IdentityProfileId,
        network: MidnightNetwork,
        now: UnixTimestampMillis,
    ) -> Result<Self, DidDeploymentOperationError> {
        if network == MidnightNetwork::Offchain || now.value() == 0 {
            return Err(DidDeploymentOperationError::InvalidData);
        }
        Ok(Self {
            operation_id,
            profile_id,
            network,
            state: DidDeploymentState::Composing,
            resume_from: None,
            failure: None,
            did: None,
            submission_id: None,
            transaction_hash_hex: None,
            block_hash_hex: None,
            block_height: None,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn restore(
        parts: DidDeploymentOperationParts,
    ) -> Result<Self, DidDeploymentOperationError> {
        let operation = Self {
            operation_id: parts.operation_id,
            profile_id: parts.profile_id,
            network: parts.network,
            state: parts.state,
            resume_from: parts.resume_from,
            failure: parts.failure,
            did: parts.did,
            submission_id: parts.submission_id,
            transaction_hash_hex: parts.transaction_hash_hex,
            block_hash_hex: parts.block_hash_hex,
            block_height: parts.block_height,
            created_at: parts.created_at,
            updated_at: parts.updated_at,
        };
        operation.validate()?;
        Ok(operation)
    }

    #[must_use]
    pub fn to_parts(&self) -> DidDeploymentOperationParts {
        DidDeploymentOperationParts {
            operation_id: self.operation_id.clone(),
            profile_id: self.profile_id.clone(),
            network: self.network,
            state: self.state,
            resume_from: self.resume_from,
            failure: self.failure,
            did: self.did.clone(),
            submission_id: self.submission_id.clone(),
            transaction_hash_hex: self.transaction_hash_hex.clone(),
            block_hash_hex: self.block_hash_hex.clone(),
            block_height: self.block_height,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }

    #[must_use]
    pub const fn operation_id(&self) -> &DidDeploymentOperationId {
        &self.operation_id
    }

    #[must_use]
    pub const fn profile_id(&self) -> &IdentityProfileId {
        &self.profile_id
    }

    #[must_use]
    pub const fn network(&self) -> MidnightNetwork {
        self.network
    }

    #[must_use]
    pub const fn state(&self) -> DidDeploymentState {
        self.state
    }

    #[must_use]
    pub const fn resume_from(&self) -> Option<DidDeploymentState> {
        self.resume_from
    }

    #[must_use]
    pub const fn failure(&self) -> Option<DidDeploymentFailure> {
        self.failure
    }

    #[must_use]
    pub const fn did(&self) -> Option<&MidnightDid> {
        self.did.as_ref()
    }

    #[must_use]
    pub fn submission_id(&self) -> Option<&str> {
        self.submission_id.as_deref()
    }

    #[must_use]
    pub fn transaction_hash_hex(&self) -> Option<&str> {
        self.transaction_hash_hex.as_deref()
    }

    #[must_use]
    pub fn block_hash_hex(&self) -> Option<&str> {
        self.block_hash_hex.as_deref()
    }

    #[must_use]
    pub const fn block_height(&self) -> Option<u64> {
        self.block_height
    }

    #[must_use]
    pub const fn created_at(&self) -> UnixTimestampMillis {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> UnixTimestampMillis {
        self.updated_at
    }

    pub fn composed(
        mut self,
        did: MidnightDid,
        submission_id: String,
        now: UnixTimestampMillis,
    ) -> Result<Self, DidDeploymentOperationError> {
        if did.network() != self.network
            || submission_id.is_empty()
            || submission_id.len() > 256
            || self.did.as_ref().is_some_and(|current| current != &did)
            || self
                .submission_id
                .as_ref()
                .is_some_and(|current| current != &submission_id)
        {
            return Err(DidDeploymentOperationError::InvalidData);
        }
        self.did = Some(did);
        self.submission_id = Some(submission_id);
        self.transition(DidDeploymentState::Funding, now)
    }

    pub fn transition(
        mut self,
        next: DidDeploymentState,
        now: UnixTimestampMillis,
    ) -> Result<Self, DidDeploymentOperationError> {
        if now.value() < self.updated_at.value()
            || !valid_transition(self.state, next, self.resume_from)
        {
            return Err(DidDeploymentOperationError::InvalidTransition);
        }
        self.state = next;
        self.updated_at = now;
        if !matches!(
            next,
            DidDeploymentState::RetryableFailure | DidDeploymentState::OutcomeUnknown
        ) {
            self.resume_from = None;
            self.failure = None;
        }
        Ok(self)
    }

    pub fn retryable_failure(
        mut self,
        failure: DidDeploymentFailure,
        now: UnixTimestampMillis,
    ) -> Result<Self, DidDeploymentOperationError> {
        if self.state.terminal()
            || matches!(
                self.state,
                DidDeploymentState::RetryableFailure | DidDeploymentState::OutcomeUnknown
            )
            || now.value() < self.updated_at.value()
        {
            return Err(DidDeploymentOperationError::InvalidTransition);
        }
        self.resume_from = Some(self.state);
        self.failure = Some(failure);
        self.state = DidDeploymentState::RetryableFailure;
        self.updated_at = now;
        Ok(self)
    }

    pub fn outcome_unknown(
        mut self,
        now: UnixTimestampMillis,
    ) -> Result<Self, DidDeploymentOperationError> {
        if !matches!(
            self.state,
            DidDeploymentState::Submitting | DidDeploymentState::Confirming
        ) || now.value() < self.updated_at.value()
        {
            return Err(DidDeploymentOperationError::InvalidTransition);
        }
        self.resume_from = Some(DidDeploymentState::Confirming);
        self.failure = None;
        self.state = DidDeploymentState::OutcomeUnknown;
        self.updated_at = now;
        Ok(self)
    }

    pub fn included(
        mut self,
        transaction_hash_hex: String,
        block_hash_hex: String,
        block_height: u64,
        now: UnixTimestampMillis,
    ) -> Result<Self, DidDeploymentOperationError> {
        if !matches!(
            self.state,
            DidDeploymentState::Confirming | DidDeploymentState::OutcomeUnknown
        ) || !is_lower_hex_32(&transaction_hash_hex)
            || !is_lower_hex_32(&block_hash_hex)
        {
            return Err(DidDeploymentOperationError::InvalidData);
        }
        self.transaction_hash_hex = Some(transaction_hash_hex);
        self.block_hash_hex = Some(block_hash_hex);
        self.block_height = Some(block_height);
        self.transition(DidDeploymentState::Resolving, now)
    }

    fn validate(&self) -> Result<(), DidDeploymentOperationError> {
        if self.network == MidnightNetwork::Offchain
            || self.created_at.value() == 0
            || self.updated_at.value() < self.created_at.value()
            || self
                .did
                .as_ref()
                .is_some_and(|did| did.network() != self.network)
            || self
                .submission_id
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > 256)
            || self
                .transaction_hash_hex
                .as_ref()
                .is_some_and(|value| !is_lower_hex_32(value))
            || self
                .block_hash_hex
                .as_ref()
                .is_some_and(|value| !is_lower_hex_32(value))
        {
            return Err(DidDeploymentOperationError::Integrity);
        }
        let composed = self.did.is_some() && self.submission_id.is_some();
        let inclusion = self.transaction_hash_hex.is_some()
            && self.block_hash_hex.is_some()
            && self.block_height.is_some();
        let valid = match self.state {
            DidDeploymentState::Composing => {
                !composed && !inclusion && self.resume_from.is_none() && self.failure.is_none()
            }
            DidDeploymentState::Funding
            | DidDeploymentState::Proving
            | DidDeploymentState::Submitting
            | DidDeploymentState::Confirming => {
                composed && !inclusion && self.resume_from.is_none() && self.failure.is_none()
            }
            DidDeploymentState::Resolving | DidDeploymentState::Ready => {
                composed && inclusion && self.resume_from.is_none() && self.failure.is_none()
            }
            DidDeploymentState::RetryableFailure => {
                self.resume_from.is_some() && self.failure.is_some()
            }
            DidDeploymentState::OutcomeUnknown => {
                composed
                    && !inclusion
                    && self.resume_from == Some(DidDeploymentState::Confirming)
                    && self.failure.is_none()
            }
        };
        if valid {
            Ok(())
        } else {
            Err(DidDeploymentOperationError::Integrity)
        }
    }
}

fn valid_transition(
    current: DidDeploymentState,
    next: DidDeploymentState,
    resume_from: Option<DidDeploymentState>,
) -> bool {
    if current == next {
        return true;
    }
    match (current, next) {
        (DidDeploymentState::Composing, DidDeploymentState::Funding)
        | (DidDeploymentState::Funding, DidDeploymentState::Proving)
        | (DidDeploymentState::Proving, DidDeploymentState::Submitting)
        | (DidDeploymentState::Submitting, DidDeploymentState::Confirming)
        | (DidDeploymentState::Confirming, DidDeploymentState::Resolving)
        | (DidDeploymentState::Resolving, DidDeploymentState::Ready) => true,
        (DidDeploymentState::RetryableFailure, resumed) => resume_from == Some(resumed),
        (DidDeploymentState::OutcomeUnknown, DidDeploymentState::Confirming) => true,
        _ => false,
    }
}

fn is_lower_hex_32(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidDeploymentOperationError {
    Unavailable,
    NotFound,
    Conflict,
    CapacityExceeded,
    Integrity,
    InvalidData,
    InvalidTransition,
}

impl fmt::Display for DidDeploymentOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "DID deployment operation storage is unavailable",
            Self::NotFound => "DID deployment operation was not found",
            Self::Conflict => "DID deployment operation conflicts with retained state",
            Self::CapacityExceeded => "DID deployment operation capacity was exceeded",
            Self::Integrity => "DID deployment operation integrity validation failed",
            Self::InvalidData => "DID deployment operation data is invalid",
            Self::InvalidTransition => "DID deployment state transition is invalid",
        })
    }
}

impl Error for DidDeploymentOperationError {}

/// Durable storage boundary. Exactly one non-ready operation may be active for
/// a profile/network pair; implementations must enforce that invariant.
pub trait DidDeploymentOperationRepository: Send + Sync {
    fn upsert(&self, operation: DidDeploymentOperation) -> Result<(), DidDeploymentOperationError>;

    fn get(
        &self,
        operation_id: &DidDeploymentOperationId,
    ) -> Result<DidDeploymentOperation, DidDeploymentOperationError>;

    fn active(
        &self,
        profile_id: &IdentityProfileId,
        network: MidnightNetwork,
    ) -> Result<Option<DidDeploymentOperation>, DidDeploymentOperationError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation() -> DidDeploymentOperation {
        DidDeploymentOperation::new(
            DidDeploymentOperationId::parse("deployment-1").expect("operation id"),
            IdentityProfileId::parse("profile-1").expect("profile id"),
            MidnightNetwork::Undeployed,
            UnixTimestampMillis::new(1_000),
        )
        .expect("operation")
    }

    fn did() -> MidnightDid {
        MidnightDid::parse(format!("did:midnight:undeployed:{}", "a".repeat(64))).expect("did")
    }

    #[test]
    fn accepts_the_canonical_happy_path() {
        let mut operation = operation()
            .composed(did(), "draft-1".to_owned(), UnixTimestampMillis::new(2_000))
            .expect("composed");
        for (state, time) in [
            (DidDeploymentState::Proving, 3_000),
            (DidDeploymentState::Submitting, 4_000),
            (DidDeploymentState::Confirming, 5_000),
        ] {
            operation = operation
                .transition(state, UnixTimestampMillis::new(time))
                .expect("transition");
        }
        operation = operation
            .included(
                "1".repeat(64),
                "2".repeat(64),
                42,
                UnixTimestampMillis::new(6_000),
            )
            .expect("included")
            .transition(DidDeploymentState::Ready, UnixTimestampMillis::new(7_000))
            .expect("ready");

        assert_eq!(operation.state(), DidDeploymentState::Ready);
        assert_eq!(operation.did(), Some(&did()));
        assert_eq!(operation.block_height(), Some(42));
    }

    #[test]
    fn retries_only_the_phase_that_failed() {
        let operation = operation()
            .composed(did(), "draft-1".to_owned(), UnixTimestampMillis::new(2_000))
            .expect("composed")
            .retryable_failure(
                DidDeploymentFailure::InsufficientDust,
                UnixTimestampMillis::new(3_000),
            )
            .expect("failure");
        assert_eq!(operation.resume_from(), Some(DidDeploymentState::Funding));
        assert!(
            operation
                .clone()
                .transition(
                    DidDeploymentState::Composing,
                    UnixTimestampMillis::new(4_000)
                )
                .is_err()
        );
        assert_eq!(
            operation
                .transition(DidDeploymentState::Funding, UnixTimestampMillis::new(4_000))
                .expect("resume")
                .state(),
            DidDeploymentState::Funding
        );
    }

    #[test]
    fn unknown_submission_can_only_reconcile_confirmation() {
        let operation = operation()
            .composed(did(), "draft-1".to_owned(), UnixTimestampMillis::new(2_000))
            .expect("composed")
            .transition(DidDeploymentState::Proving, UnixTimestampMillis::new(3_000))
            .expect("proving")
            .transition(
                DidDeploymentState::Submitting,
                UnixTimestampMillis::new(4_000),
            )
            .expect("submitting")
            .outcome_unknown(UnixTimestampMillis::new(5_000))
            .expect("unknown");
        assert!(
            operation
                .clone()
                .transition(
                    DidDeploymentState::Submitting,
                    UnixTimestampMillis::new(6_000)
                )
                .is_err()
        );
        assert_eq!(
            operation
                .transition(
                    DidDeploymentState::Confirming,
                    UnixTimestampMillis::new(6_000)
                )
                .expect("reconcile")
                .state(),
            DidDeploymentState::Confirming
        );
    }

    #[test]
    fn ready_is_terminal_and_evidence_is_strict() {
        let operation = operation()
            .composed(did(), "draft-1".to_owned(), UnixTimestampMillis::new(2_000))
            .expect("composed")
            .transition(DidDeploymentState::Proving, UnixTimestampMillis::new(3_000))
            .expect("proving")
            .transition(
                DidDeploymentState::Submitting,
                UnixTimestampMillis::new(4_000),
            )
            .expect("submitting")
            .transition(
                DidDeploymentState::Confirming,
                UnixTimestampMillis::new(5_000),
            )
            .expect("confirming");
        assert!(
            operation
                .clone()
                .included(
                    "A".repeat(64),
                    "2".repeat(64),
                    42,
                    UnixTimestampMillis::new(6_000),
                )
                .is_err()
        );
        let ready = operation
            .included(
                "1".repeat(64),
                "2".repeat(64),
                42,
                UnixTimestampMillis::new(6_000),
            )
            .expect("included")
            .transition(DidDeploymentState::Ready, UnixTimestampMillis::new(7_000))
            .expect("ready");
        assert!(
            ready
                .transition(
                    DidDeploymentState::Resolving,
                    UnixTimestampMillis::new(8_000)
                )
                .is_err()
        );
    }
}
