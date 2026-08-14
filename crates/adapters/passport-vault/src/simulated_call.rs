// SPDX-License-Identifier: Apache-2.0

//! Deterministic, process-local Passport Vault call lifecycle used only by the
//! headless development harness. It retains opaque call material, models the
//! pre-broadcast cancellation boundary, and labels every outcome as simulation.

use std::{collections::BTreeMap, sync::Mutex, thread, time::Duration};

use oxid_foundation::{OpaqueId, UnixTimestampMillis};
use oxid_passport_vault_application::{
    AuthorizePassportVaultCallRequest, MAX_PASSPORT_VAULT_CALL_SUBMISSION_HISTORY,
    PassportVaultCallAuthorizationChallenge, PassportVaultCallDraftId, PassportVaultCallDraftState,
    PassportVaultCallInclusion, PassportVaultCallOperation, PassportVaultCallPortError,
    PassportVaultCallPreview, PassportVaultCallStatusFuture, PassportVaultCallSubmissionFuture,
    PassportVaultCallSubmissionState, PassportVaultCallSubmissionStatus,
    PassportVaultContractCallPort, PassportVaultContractStateAuthentication,
    PreparePassportVaultCallRequest, SubmitPassportVaultCallRequest, SubmittedPassportVaultCall,
};
use sha2::{Digest, Sha256};

const SIMULATED_FEE_ATOMIC_UNITS: u128 = 1_000_000;
const SIMULATED_SUBMISSION_STEPS: usize = 40;
const SIMULATED_SUBMISSION_STEP_MILLIS: u64 = 10;
const SIMULATED_MODE: &str = "deterministic_simulation_only";

#[derive(Clone)]
struct RetainedSimulatedCall {
    planning_fingerprint: [u8; 32],
    preview: PassportVaultCallPreview,
    submission_status: PassportVaultCallSubmissionStatus,
}

type CallKey = (OpaqueId, PassportVaultCallDraftId);

#[derive(Default)]
pub struct SimulatedPassportVaultContractCall {
    calls: Mutex<BTreeMap<CallKey, RetainedSimulatedCall>>,
}

impl SimulatedPassportVaultContractCall {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            calls: Mutex::new(BTreeMap::new()),
        }
    }
}

impl PassportVaultContractCallPort for SimulatedPassportVaultContractCall {
    fn prepare(
        &self,
        request: PreparePassportVaultCallRequest,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
        if request.contract_state.authentication
            != PassportVaultContractStateAuthentication::DeterministicSimulation
        {
            return Err(PassportVaultCallPortError::InvalidChainState);
        }
        let planning_fingerprint = planning_fingerprint(&request);
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?;
        if let Some(existing) = calls.values().find(|retained| {
            retained.preview.contract_address_hex == request.contract_state.contract_address_hex
                && retained.planning_fingerprint == planning_fingerprint
        }) {
            return Ok(existing.preview.clone());
        }
        let profile_count = calls
            .keys()
            .filter(|(profile_id, _)| profile_id == &request.profile_id)
            .count();
        if profile_count >= MAX_PASSPORT_VAULT_CALL_SUBMISSION_HISTORY {
            return Err(PassportVaultCallPortError::Unavailable);
        }

        let draft_id = PassportVaultCallDraftId::parse(hex::encode(planning_fingerprint))
            .map_err(|_| PassportVaultCallPortError::InvalidData)?;
        let authorization_challenge = authorization_challenge(&draft_id, &request)?;
        let preview = PassportVaultCallPreview {
            draft_id: draft_id.clone(),
            authorization_challenge,
            contract_address_hex: request.contract_state.contract_address_hex,
            operation: request.operation,
            state_anchor_transaction_hash_hex: request.contract_state.transaction_hash_hex,
            state_anchor_block_hash_hex: request.contract_state.action_block_hash_hex,
            state_anchor_block_height: request.contract_state.action_block_height,
            expires_at: request.expires_at,
            state: PassportVaultCallDraftState::Prepared,
            fee_atomic_units: None,
        };
        let key = (request.profile_id, draft_id.clone());
        if let Some(existing) = calls.get(&key) {
            return if existing.preview == preview {
                Ok(existing.preview.clone())
            } else {
                Err(PassportVaultCallPortError::DraftConflict)
            };
        }
        calls.insert(
            key,
            RetainedSimulatedCall {
                planning_fingerprint,
                preview: preview.clone(),
                submission_status: status(
                    draft_id,
                    PassportVaultCallSubmissionState::NotStarted,
                    None,
                ),
            },
        );
        Ok(preview)
    }

    fn authorize(
        &self,
        profile_id: &OpaqueId,
        request: AuthorizePassportVaultCallRequest,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?;
        let retained = calls
            .get_mut(&(profile_id.clone(), request.draft_id.clone()))
            .ok_or(PassportVaultCallPortError::DraftNotFound)?;
        expire_if_needed(retained, request.now);
        if retained.preview.state == PassportVaultCallDraftState::Expired {
            return Err(PassportVaultCallPortError::DraftExpired);
        }
        if retained.preview.authorization_challenge != request.authorization_challenge {
            return Err(PassportVaultCallPortError::AuthorizationChallengeMismatch);
        }
        if retained.preview.state != PassportVaultCallDraftState::Prepared {
            return Err(PassportVaultCallPortError::DraftConflict);
        }
        retained.preview.state = PassportVaultCallDraftState::Authorized;
        Ok(retained.preview.clone())
    }

    fn submit<'a>(
        &'a self,
        profile_id: &'a OpaqueId,
        request: SubmitPassportVaultCallRequest,
    ) -> PassportVaultCallSubmissionFuture<'a> {
        Box::pin(async move {
            let key = (profile_id.clone(), request.draft_id.clone());
            {
                let mut calls = self
                    .calls
                    .lock()
                    .map_err(|_| PassportVaultCallPortError::Unavailable)?;
                let retained = calls
                    .get_mut(&key)
                    .ok_or(PassportVaultCallPortError::DraftNotFound)?;
                expire_if_needed(retained, request.now);
                match retained.preview.state {
                    PassportVaultCallDraftState::Authorized => {}
                    PassportVaultCallDraftState::Expired => {
                        return Err(PassportVaultCallPortError::DraftExpired);
                    }
                    PassportVaultCallDraftState::Submitting => {
                        return Err(PassportVaultCallPortError::SubmissionInProgress);
                    }
                    PassportVaultCallDraftState::Prepared
                    | PassportVaultCallDraftState::Submitted => {
                        return Err(PassportVaultCallPortError::DraftConflict);
                    }
                }
                retained.preview.state = PassportVaultCallDraftState::Submitting;
                retained.submission_status = status(
                    request.draft_id.clone(),
                    PassportVaultCallSubmissionState::Running,
                    None,
                );
            }

            for _ in 0..SIMULATED_SUBMISSION_STEPS {
                thread::sleep(Duration::from_millis(SIMULATED_SUBMISSION_STEP_MILLIS));
                let mut calls = self
                    .calls
                    .lock()
                    .map_err(|_| PassportVaultCallPortError::Unavailable)?;
                let retained = calls
                    .get_mut(&key)
                    .ok_or(PassportVaultCallPortError::DraftNotFound)?;
                if retained.submission_status.state
                    == PassportVaultCallSubmissionState::CancellationRequested
                {
                    retained.preview.state = PassportVaultCallDraftState::Authorized;
                    retained.submission_status = status(
                        request.draft_id.clone(),
                        PassportVaultCallSubmissionState::Cancelled,
                        None,
                    );
                    return Err(PassportVaultCallPortError::SubmissionCancelled);
                }
            }

            let transaction_hash_hex = simulated_hash(
                b"oxid:simulated-passport-vault-transaction:v1\0",
                request.draft_id.as_str().as_bytes(),
            );
            {
                let mut calls = self
                    .calls
                    .lock()
                    .map_err(|_| PassportVaultCallPortError::Unavailable)?;
                let retained = calls
                    .get_mut(&key)
                    .ok_or(PassportVaultCallPortError::DraftNotFound)?;
                if retained.submission_status.state
                    == PassportVaultCallSubmissionState::CancellationRequested
                {
                    retained.preview.state = PassportVaultCallDraftState::Authorized;
                    retained.submission_status = status(
                        request.draft_id.clone(),
                        PassportVaultCallSubmissionState::Cancelled,
                        None,
                    );
                    return Err(PassportVaultCallPortError::SubmissionCancelled);
                }
                retained.submission_status = status(
                    request.draft_id.clone(),
                    PassportVaultCallSubmissionState::Broadcasting,
                    Some((&transaction_hash_hex, None)),
                );
            }

            thread::sleep(Duration::from_millis(25));
            let block_hash_hex = simulated_hash(
                b"oxid:simulated-passport-vault-block:v1\0",
                transaction_hash_hex.as_bytes(),
            );
            let mut calls = self
                .calls
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            let retained = calls
                .get_mut(&key)
                .ok_or(PassportVaultCallPortError::DraftNotFound)?;
            let block_height = retained
                .preview
                .state_anchor_block_height
                .checked_add(1)
                .ok_or(PassportVaultCallPortError::InvalidData)?;
            let inclusion = PassportVaultCallInclusion {
                transaction_hash_hex,
                block_hash_hex,
                block_height,
                fee_atomic_units: SIMULATED_FEE_ATOMIC_UNITS,
                mode: SIMULATED_MODE.to_owned(),
            };
            retained.preview.state = PassportVaultCallDraftState::Submitted;
            retained.preview.fee_atomic_units = Some(SIMULATED_FEE_ATOMIC_UNITS);
            retained.submission_status = status(
                request.draft_id,
                PassportVaultCallSubmissionState::Included,
                Some((
                    &inclusion.transaction_hash_hex,
                    Some((&inclusion.block_hash_hex, inclusion.block_height)),
                )),
            );
            Ok(SubmittedPassportVaultCall {
                preview: retained.preview.clone(),
                inclusion,
            })
        })
    }

    fn get(
        &self,
        profile_id: &OpaqueId,
        draft_id: &PassportVaultCallDraftId,
        now: UnixTimestampMillis,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?;
        let retained = calls
            .get_mut(&(profile_id.clone(), draft_id.clone()))
            .ok_or(PassportVaultCallPortError::DraftNotFound)?;
        expire_if_needed(retained, now);
        Ok(retained.preview.clone())
    }

    fn submission_status(
        &self,
        profile_id: &OpaqueId,
        draft_id: &PassportVaultCallDraftId,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        self.calls
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?
            .get(&(profile_id.clone(), draft_id.clone()))
            .map(|retained| retained.submission_status.clone())
            .ok_or(PassportVaultCallPortError::DraftNotFound)
    }

    fn cancel_submission(
        &self,
        profile_id: &OpaqueId,
        draft_id: &PassportVaultCallDraftId,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?;
        let retained = calls
            .get_mut(&(profile_id.clone(), draft_id.clone()))
            .ok_or(PassportVaultCallPortError::DraftNotFound)?;
        match retained.submission_status.state {
            PassportVaultCallSubmissionState::Running => {
                retained.submission_status.state =
                    PassportVaultCallSubmissionState::CancellationRequested;
                Ok(retained.submission_status.clone())
            }
            PassportVaultCallSubmissionState::CancellationRequested => {
                Ok(retained.submission_status.clone())
            }
            PassportVaultCallSubmissionState::Broadcasting
            | PassportVaultCallSubmissionState::Included
            | PassportVaultCallSubmissionState::OutcomeUnknown => {
                Err(PassportVaultCallPortError::SubmissionCancellationUnsafe)
            }
            _ => Err(PassportVaultCallPortError::SubmissionNotInProgress),
        }
    }

    fn submission_history(
        &self,
        profile_id: &OpaqueId,
    ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError> {
        Ok(self
            .calls
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?
            .iter()
            .filter(|((stored_profile_id, _), _)| stored_profile_id == profile_id)
            .map(|(_, retained)| retained.submission_status.clone())
            .collect())
    }

    fn reconcile_submission<'a>(
        &'a self,
        profile_id: &'a OpaqueId,
        draft_id: &'a PassportVaultCallDraftId,
    ) -> PassportVaultCallStatusFuture<'a> {
        Box::pin(async move {
            let current = self.submission_status(profile_id, draft_id)?;
            if current.reconciliation_allowed() {
                Ok(current)
            } else {
                Err(PassportVaultCallPortError::SubmissionNotInProgress)
            }
        })
    }
}

fn planning_fingerprint(request: &PreparePassportVaultCallRequest) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:simulated-passport-vault-plan:v1\0");
    digest.update(request.profile_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(request.contract_state.contract_address_hex.as_bytes());
    digest.update(request.contract_state.transaction_hash_hex.as_bytes());
    digest.update(request.contract_state.action_block_hash_hex.as_bytes());
    digest.update(request.contract_state.action_block_height.to_be_bytes());
    digest.update(request.expires_at.value().to_be_bytes());
    update_operation_digest(&mut digest, &request.operation);
    digest.finalize().into()
}

fn authorization_challenge(
    draft_id: &PassportVaultCallDraftId,
    request: &PreparePassportVaultCallRequest,
) -> Result<PassportVaultCallAuthorizationChallenge, PassportVaultCallPortError> {
    let mut digest = Sha256::new();
    digest.update(b"oxid:simulated-passport-vault-authorization:v1\0");
    digest.update(draft_id.as_str().as_bytes());
    digest.update(request.contract_state.action_block_hash_hex.as_bytes());
    PassportVaultCallAuthorizationChallenge::parse(hex::encode(digest.finalize()))
        .map_err(|_| PassportVaultCallPortError::InvalidData)
}

fn update_operation_digest(digest: &mut Sha256, operation: &PassportVaultCallOperation) {
    match operation {
        PassportVaultCallOperation::CreateLock {
            policy,
            initial_amount,
        } => {
            digest.update([0]);
            digest.update([policy.minimum_age_years()]);
            update_optional_bytes(digest, policy.required_issuing_state());
            update_optional_bytes(digest, policy.required_document_number());
            digest.update(policy.maximum_claim_amount().to_be_bytes());
            digest.update(policy.verifier_challenge_hash());
            digest.update(initial_amount.to_be_bytes());
        }
        PassportVaultCallOperation::DepositToLock { lock_id, amount } => {
            digest.update([1]);
            digest.update(lock_id.to_be_bytes());
            digest.update(amount.to_be_bytes());
        }
        PassportVaultCallOperation::ClaimFromLock {
            lock_id,
            amount,
            credential_id,
        } => {
            digest.update([2]);
            digest.update(lock_id.to_be_bytes());
            digest.update(amount.to_be_bytes());
            digest.update(credential_id.as_str().as_bytes());
        }
        PassportVaultCallOperation::WithdrawFromLock { lock_id, amount } => {
            digest.update([3]);
            digest.update(lock_id.to_be_bytes());
            digest.update(amount.to_be_bytes());
        }
    }
}

fn update_optional_bytes(digest: &mut Sha256, value: Option<[u8; 32]>) {
    match value {
        Some(value) => {
            digest.update([1]);
            digest.update(value);
        }
        None => digest.update([0]),
    }
}

fn simulated_hash(domain: &[u8], value: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(value);
    hex::encode(digest.finalize())
}

fn expire_if_needed(retained: &mut RetainedSimulatedCall, now: UnixTimestampMillis) {
    if now.value() >= retained.preview.expires_at.value()
        && matches!(
            retained.preview.state,
            PassportVaultCallDraftState::Prepared | PassportVaultCallDraftState::Authorized
        )
    {
        retained.preview.state = PassportVaultCallDraftState::Expired;
        retained.submission_status = status(
            retained.preview.draft_id.clone(),
            PassportVaultCallSubmissionState::Expired,
            None,
        );
    }
}

fn status(
    draft_id: PassportVaultCallDraftId,
    state: PassportVaultCallSubmissionState,
    inclusion: Option<(&str, Option<(&str, u64)>)>,
) -> PassportVaultCallSubmissionStatus {
    let (transaction_hash_hex, block_hash_hex, block_height, fee_atomic_units, mode) = inclusion
        .map_or((None, None, None, None, None), |(transaction, block)| {
            (
                Some(transaction.to_owned()),
                block.map(|(hash, _)| hash.to_owned()),
                block.map(|(_, height)| height),
                Some(SIMULATED_FEE_ATOMIC_UNITS),
                Some(SIMULATED_MODE.to_owned()),
            )
        });
    PassportVaultCallSubmissionStatus {
        draft_id,
        state,
        transaction_hash_hex,
        block_hash_hex,
        block_height,
        fee_atomic_units,
        mode,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::executor::block_on;
    use oxid_passport_vault_application::PassportVaultContractStateSourcePort;
    use oxid_passport_vault_domain::PassportVaultPolicy;

    use super::*;
    use crate::SimulatedPassportVaultStateSource;

    fn profile() -> OpaqueId {
        OpaqueId::parse("profile_simulated_vault").expect("profile")
    }

    fn request(operation: PassportVaultCallOperation) -> PreparePassportVaultCallRequest {
        let source = SimulatedPassportVaultStateSource::new().expect("fixture source");
        let contract_state = block_on(source.read(source.contract_address_hex())).expect("state");
        PreparePassportVaultCallRequest {
            profile_id: profile(),
            contract_state,
            operation,
            expires_at: UnixTimestampMillis::new(10_000),
        }
    }

    fn create_operation() -> PassportVaultCallOperation {
        PassportVaultCallOperation::CreateLock {
            policy: PassportVaultPolicy::new(18, None, None, 40, [7; 32]).expect("policy"),
            initial_amount: 100,
        }
    }

    #[test]
    fn complete_simulated_lifecycle_is_deterministic_and_explicitly_labelled() {
        let adapter = SimulatedPassportVaultContractCall::new();
        let prepared = adapter
            .prepare(request(create_operation()))
            .expect("prepare");
        let repeated = adapter
            .prepare(request(create_operation()))
            .expect("idempotent");
        assert_eq!(repeated, prepared);
        let authorized = adapter
            .authorize(
                &profile(),
                AuthorizePassportVaultCallRequest {
                    draft_id: prepared.draft_id.clone(),
                    authorization_challenge: prepared.authorization_challenge.clone(),
                    now: UnixTimestampMillis::new(1_000),
                },
            )
            .expect("authorize");
        assert_eq!(authorized.state, PassportVaultCallDraftState::Authorized);
        let submitted = block_on(adapter.submit(
            &profile(),
            SubmitPassportVaultCallRequest {
                draft_id: prepared.draft_id.clone(),
                now: UnixTimestampMillis::new(2_000),
            },
        ))
        .expect("submit");
        assert_eq!(
            submitted.preview.state,
            PassportVaultCallDraftState::Submitted
        );
        assert_eq!(submitted.inclusion.mode, SIMULATED_MODE);
        assert_eq!(
            adapter
                .submission_status(&profile(), &prepared.draft_id)
                .expect("status")
                .state,
            PassportVaultCallSubmissionState::Included
        );
    }

    #[test]
    fn all_four_operations_have_distinct_retained_plans() {
        let adapter = SimulatedPassportVaultContractCall::new();
        let operations = [
            create_operation(),
            PassportVaultCallOperation::DepositToLock {
                lock_id: 1,
                amount: 10,
            },
            PassportVaultCallOperation::ClaimFromLock {
                lock_id: 1,
                amount: 5,
                credential_id: OpaqueId::parse("credential_1").expect("credential"),
            },
            PassportVaultCallOperation::WithdrawFromLock {
                lock_id: 1,
                amount: 4,
            },
        ];
        let ids = operations
            .into_iter()
            .map(|operation| {
                adapter
                    .prepare(request(operation))
                    .expect("prepare")
                    .draft_id
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), 4);
        assert_eq!(
            adapter
                .submission_history(&profile())
                .expect("history")
                .len(),
            4
        );
    }

    #[test]
    fn cancellation_restores_authorized_state_before_broadcast() {
        let adapter = Arc::new(SimulatedPassportVaultContractCall::new());
        let prepared = adapter
            .prepare(request(create_operation()))
            .expect("prepare");
        adapter
            .authorize(
                &profile(),
                AuthorizePassportVaultCallRequest {
                    draft_id: prepared.draft_id.clone(),
                    authorization_challenge: prepared.authorization_challenge,
                    now: UnixTimestampMillis::new(1_000),
                },
            )
            .expect("authorize");
        let worker_adapter = Arc::clone(&adapter);
        let worker_draft = prepared.draft_id.clone();
        let worker = thread::spawn(move || {
            block_on(worker_adapter.submit(
                &profile(),
                SubmitPassportVaultCallRequest {
                    draft_id: worker_draft,
                    now: UnixTimestampMillis::new(2_000),
                },
            ))
        });
        for _ in 0..100 {
            if adapter
                .submission_status(&profile(), &prepared.draft_id)
                .is_ok_and(|status| status.state == PassportVaultCallSubmissionState::Running)
            {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let requested = adapter
            .cancel_submission(&profile(), &prepared.draft_id)
            .expect("request cancellation");
        assert_eq!(
            requested.state,
            PassportVaultCallSubmissionState::CancellationRequested
        );
        assert_eq!(
            worker.join().expect("worker"),
            Err(PassportVaultCallPortError::SubmissionCancelled)
        );
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    &prepared.draft_id,
                    UnixTimestampMillis::new(3_000)
                )
                .expect("restored")
                .state,
            PassportVaultCallDraftState::Authorized
        );
    }

    #[test]
    fn rejects_live_authentication_and_expires_at_the_exact_boundary() {
        let adapter = SimulatedPassportVaultContractCall::new();
        let mut live = request(create_operation());
        live.contract_state.authentication =
            PassportVaultContractStateAuthentication::CanonicalFinalizedReplay;
        assert_eq!(
            adapter.prepare(live),
            Err(PassportVaultCallPortError::InvalidChainState)
        );
        let prepared = adapter
            .prepare(request(create_operation()))
            .expect("prepare");
        assert_eq!(
            adapter.authorize(
                &profile(),
                AuthorizePassportVaultCallRequest {
                    draft_id: prepared.draft_id,
                    authorization_challenge: prepared.authorization_challenge,
                    now: UnixTimestampMillis::new(10_000),
                }
            ),
            Err(PassportVaultCallPortError::DraftExpired)
        );
    }
}
