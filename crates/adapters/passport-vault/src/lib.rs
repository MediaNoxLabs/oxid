// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(not(target_arch = "wasm32"))]
mod authenticated_state;
#[cfg(not(target_arch = "wasm32"))]
mod compact_artifacts;
#[cfg(all(not(target_arch = "wasm32"), test))]
mod compact_composer_conformance;
#[cfg(not(target_arch = "wasm32"))]
mod contract_state;
#[cfg(not(target_arch = "wasm32"))]
mod finalized_history;
#[cfg(not(target_arch = "wasm32"))]
mod live_state;
#[cfg(not(target_arch = "wasm32"))]
mod native_call;
#[cfg(not(target_arch = "wasm32"))]
mod replay;
#[cfg(not(target_arch = "wasm32"))]
mod simulated_call;
#[cfg(not(target_arch = "wasm32"))]
mod simulated_state;

#[cfg(not(target_arch = "wasm32"))]
pub use authenticated_state::{
    AuthenticatedPassportVaultStateConfigError, AuthenticatedPassportVaultStateSource,
};
#[cfg(not(target_arch = "wasm32"))]
pub use compact_artifacts::{
    NativePassportVaultCompactArtifacts, PassportVaultCompactArtifactError,
    PassportVaultCompactArtifactsConfig, PassportVaultCompactCircuit,
};
#[cfg(not(target_arch = "wasm32"))]
pub use contract_state::NativePassportVaultContractStateDecoder;
#[cfg(not(target_arch = "wasm32"))]
pub use finalized_history::{
    FinalizedMidnightHistory, FinalizedMidnightHistoryCollector,
    FinalizedMidnightHistoryCollectorConfigError, FinalizedMidnightHistoryError,
};
#[cfg(not(target_arch = "wasm32"))]
pub use live_state::{
    NodeAnchoredPassportVaultStateConfigError, NodeAnchoredPassportVaultStateSource,
    PassportVaultCallChainContext, PassportVaultCallChainContextSource,
};
#[cfg(not(target_arch = "wasm32"))]
pub use native_call::{
    FundedPassportVaultCall, NativePassportVaultContractCall, PassportVaultCallCompletionPort,
    PassportVaultCallCompletionRequest, PassportVaultCallComposerConfigError,
    PassportVaultCallCompositionContext, PassportVaultCallCompositionContextSource,
    PassportVaultCallFundingPort, PassportVaultCallFundingRequest,
};
#[cfg(not(target_arch = "wasm32"))]
pub use replay::{
    CanonicalMidnightBlockContext, CanonicalMidnightOperation, CanonicalMidnightTransaction,
    PassportVaultReplayError, ReplayedPassportVaultState, replay_canonical_passport_vault_history,
};
#[cfg(not(target_arch = "wasm32"))]
pub use simulated_call::SimulatedPassportVaultContractCall;
#[cfg(not(target_arch = "wasm32"))]
pub use simulated_state::{
    SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX, SimulatedPassportVaultStateSource,
};

use std::sync::{Arc, Mutex};

use oxid_adapter_vc_midnight::{
    DigitalPassportIssuerTrustAnchor, DigitalPassportPolicyError, DigitalPassportPolicyRequest,
    verify_digital_passport_policy,
};
use oxid_credential_application::{CredentialRepository, CredentialRepositoryError};
use oxid_credential_domain::{CredentialId, CredentialProfileId, VerificationOutcome};
use oxid_passport_vault_application::{
    PassportVaultCredentialError, PassportVaultCredentialPort, PassportVaultEvidenceFuture,
    PassportVaultRepository, PassportVaultRepositoryError, VerifiedPassportVaultCredential,
    VerifyPassportVaultCredentialRequest,
};
use oxid_passport_vault_domain::PassportVaultState;
use oxid_platform_ports::ClockPort;

#[derive(Default)]
pub struct InMemoryPassportVaultRepository {
    state: Mutex<PassportVaultState>,
}

impl PassportVaultRepository for InMemoryPassportVaultRepository {
    fn load(&self) -> Result<PassportVaultState, PassportVaultRepositoryError> {
        self.state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| PassportVaultRepositoryError::Unavailable)
    }

    fn save(&self, state: &PassportVaultState) -> Result<(), PassportVaultRepositoryError> {
        *self
            .state
            .lock()
            .map_err(|_| PassportVaultRepositoryError::Unavailable)? = state.clone();
        Ok(())
    }
}

pub struct StandalonePassportVaultCredential {
    repository: Arc<dyn CredentialRepository>,
    clock: Arc<dyn ClockPort>,
    trust_anchor: DigitalPassportIssuerTrustAnchor,
}

impl StandalonePassportVaultCredential {
    #[must_use]
    pub fn new(
        repository: Arc<dyn CredentialRepository>,
        clock: Arc<dyn ClockPort>,
        trust_anchor: DigitalPassportIssuerTrustAnchor,
    ) -> Self {
        Self {
            repository,
            clock,
            trust_anchor,
        }
    }
}

impl PassportVaultCredentialPort for StandalonePassportVaultCredential {
    fn verify<'a>(
        &'a self,
        request: VerifyPassportVaultCredentialRequest,
    ) -> PassportVaultEvidenceFuture<'a> {
        Box::pin(async move {
            let profile = CredentialProfileId::parse(request.profile_id)
                .map_err(|_| PassportVaultCredentialError::Invalid)?;
            let credential_id = CredentialId::parse(request.credential_id)
                .map_err(|_| PassportVaultCredentialError::Invalid)?;
            let record = self
                .repository
                .get(&profile, &credential_id)
                .map_err(map_repository_error)?;
            if record.verification().outcome() != VerificationOutcome::Valid {
                return Err(PassportVaultCredentialError::Invalid);
            }
            let proof = record
                .detached_proof()
                .ok_or(PassportVaultCredentialError::Invalid)?;
            let private = record
                .private_material()
                .ok_or(PassportVaultCredentialError::MissingPrivateMaterial)?;
            let now = self
                .clock
                .now()
                .map_err(|_| PassportVaultCredentialError::Unavailable)?;
            let evidence = verify_digital_passport_policy(
                record.signed_bytes(),
                proof.as_bytes(),
                private.as_bytes(),
                &self.trust_anchor,
                &DigitalPassportPolicyRequest {
                    minimum_age_years: request.policy.minimum_age_years(),
                    required_issuing_state: request.policy.required_issuing_state(),
                    required_document_number: request.policy.required_document_number(),
                    current_time_seconds: now.value() / 1_000,
                },
            )
            .map_err(map_policy_error)?;
            Ok(VerifiedPassportVaultCredential {
                credential_fingerprint: evidence.credential_root,
                current_day: evidence.current_day,
            })
        })
    }
}

fn map_repository_error(error: CredentialRepositoryError) -> PassportVaultCredentialError {
    match error {
        CredentialRepositoryError::NotFound => PassportVaultCredentialError::NotFound,
        CredentialRepositoryError::CapacityExceeded | CredentialRepositoryError::Integrity => {
            PassportVaultCredentialError::Invalid
        }
        CredentialRepositoryError::Unavailable => PassportVaultCredentialError::Unavailable,
    }
}

fn map_policy_error(error: DigitalPassportPolicyError) -> PassportVaultCredentialError {
    match error {
        DigitalPassportPolicyError::InvalidCredential | DigitalPassportPolicyError::InvalidTime => {
            PassportVaultCredentialError::Invalid
        }
        DigitalPassportPolicyError::InvalidPrivateMaterial => {
            PassportVaultCredentialError::MissingPrivateMaterial
        }
        DigitalPassportPolicyError::IssuerNotTrusted => {
            PassportVaultCredentialError::IssuerNotTrusted
        }
        DigitalPassportPolicyError::Expired => PassportVaultCredentialError::Expired,
        DigitalPassportPolicyError::AgeRequirementNotMet => {
            PassportVaultCredentialError::AgeRequirementNotMet
        }
        DigitalPassportPolicyError::IssuingStateMismatch => {
            PassportVaultCredentialError::IssuingStateMismatch
        }
        DigitalPassportPolicyError::DocumentNumberMismatch => {
            PassportVaultCredentialError::DocumentNumberMismatch
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_local_repository_round_trips_an_owned_snapshot() {
        let repository = InMemoryPassportVaultRepository::default();
        let mut state = PassportVaultState::default();
        state
            .create_lock(
                oxid_passport_vault_domain::VaultActorId::parse("profile_creator").expect("actor"),
                oxid_passport_vault_domain::PassportVaultPolicy::new(18, None, None, 40, [7; 32])
                    .expect("policy"),
                100,
            )
            .expect("lock");
        repository.save(&state).expect("save");
        let restored = repository.load().expect("load");
        assert_eq!(restored.total_locked(), 100);
        assert_eq!(restored.locks().count(), 1);
    }

    #[test]
    fn repository_and_policy_errors_keep_stable_application_categories() {
        assert_eq!(
            map_repository_error(CredentialRepositoryError::NotFound),
            PassportVaultCredentialError::NotFound
        );
        assert_eq!(
            map_repository_error(CredentialRepositoryError::Unavailable),
            PassportVaultCredentialError::Unavailable
        );
        assert_eq!(
            map_repository_error(CredentialRepositoryError::CapacityExceeded),
            PassportVaultCredentialError::Invalid
        );
        assert_eq!(
            map_repository_error(CredentialRepositoryError::Integrity),
            PassportVaultCredentialError::Invalid
        );

        let mappings = [
            (
                DigitalPassportPolicyError::InvalidCredential,
                PassportVaultCredentialError::Invalid,
            ),
            (
                DigitalPassportPolicyError::InvalidPrivateMaterial,
                PassportVaultCredentialError::MissingPrivateMaterial,
            ),
            (
                DigitalPassportPolicyError::IssuerNotTrusted,
                PassportVaultCredentialError::IssuerNotTrusted,
            ),
            (
                DigitalPassportPolicyError::Expired,
                PassportVaultCredentialError::Expired,
            ),
            (
                DigitalPassportPolicyError::InvalidTime,
                PassportVaultCredentialError::Invalid,
            ),
            (
                DigitalPassportPolicyError::AgeRequirementNotMet,
                PassportVaultCredentialError::AgeRequirementNotMet,
            ),
            (
                DigitalPassportPolicyError::IssuingStateMismatch,
                PassportVaultCredentialError::IssuingStateMismatch,
            ),
            (
                DigitalPassportPolicyError::DocumentNumberMismatch,
                PassportVaultCredentialError::DocumentNumberMismatch,
            ),
        ];
        for (input, expected) in mappings {
            assert_eq!(map_policy_error(input), expected);
        }
    }
}
