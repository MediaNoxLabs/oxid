// SPDX-License-Identifier: Apache-2.0

//! Native retained Passport Vault call composition. The generated Compact
//! process receives only bounded public chain/account context. Its serialized
//! unproven transaction remains private to this adapter and is deliberately
//! not submitted until combined NIGHT funding, DUST balancing, proving, and a
//! durable submission journal are wired.

use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use midnight_base_crypto::schnorr::Signature;
use midnight_ledger::structure::{ProofPreimageMarker, Transaction};
use midnight_serialize::tagged_deserialize;
use midnight_storage::DefaultDB;
use midnight_transient_crypto::commitment::PedersenRandomness;
use oxid_foundation::{OpaqueId, UnixTimestampMillis};
use oxid_passport_vault_application::{
    AuthorizePassportVaultCallRequest, MAX_PASSPORT_VAULT_CALL_SUBMISSION_HISTORY,
    MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES, PassportVaultCallAuthorizationChallenge,
    PassportVaultCallDraftId, PassportVaultCallDraftState, PassportVaultCallOperation,
    PassportVaultCallPortError, PassportVaultCallPreview, PassportVaultCallStatusFuture,
    PassportVaultCallSubmissionFuture, PassportVaultCallSubmissionState,
    PassportVaultCallSubmissionStatus, PassportVaultContractCallPort,
    PassportVaultContractStateAuthentication, PreparePassportVaultCallRequest,
    SubmitPassportVaultCallRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const MAX_ZSWAP_CHAIN_STATE_BYTES: usize = 2 * 1024 * 1024;
const MAX_LEDGER_PARAMETERS_BYTES: usize = 512 * 1024;
const MAX_COMPOSER_REQUEST_BYTES: usize = 40 * 1024 * 1024;
const MAX_COMPOSER_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_COMPOSER_STDERR_BYTES: usize = 64 * 1024;
const COMPOSER_TIMEOUT: Duration = Duration::from_secs(60);

type UnprovenTransaction =
    Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;
type CallKey = (OpaqueId, PassportVaultCallDraftId);

/// Public Midnight values needed by the generated call builder. These values
/// are adapter-to-adapter composition inputs, never an incoming protocol shape.
#[derive(Clone, PartialEq, Eq)]
pub struct PassportVaultCallCompositionContext {
    network_id: String,
    zswap_chain_state: Vec<u8>,
    ledger_parameters: Vec<u8>,
    coin_public_key: [u8; 32],
    encryption_public_key: [u8; 32],
    unshielded_recipient: [u8; 32],
}

impl fmt::Debug for PassportVaultCallCompositionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportVaultCallCompositionContext")
            .field("network_id", &self.network_id)
            .field("zswap_chain_state_bytes", &self.zswap_chain_state.len())
            .field("ledger_parameters_bytes", &self.ledger_parameters.len())
            .finish_non_exhaustive()
    }
}

impl PassportVaultCallCompositionContext {
    pub fn new(
        network_id: impl Into<String>,
        zswap_chain_state: Vec<u8>,
        ledger_parameters: Vec<u8>,
        coin_public_key: [u8; 32],
        encryption_public_key: [u8; 32],
        unshielded_recipient: [u8; 32],
    ) -> Result<Self, PassportVaultCallPortError> {
        let network_id = network_id.into();
        if !valid_network_id(&network_id) {
            return Err(PassportVaultCallPortError::UnsupportedNetwork);
        }
        if zswap_chain_state.is_empty()
            || zswap_chain_state.len() > MAX_ZSWAP_CHAIN_STATE_BYTES
            || ledger_parameters.is_empty()
            || ledger_parameters.len() > MAX_LEDGER_PARAMETERS_BYTES
        {
            return Err(PassportVaultCallPortError::InvalidChainState);
        }
        if coin_public_key == [0; 32]
            || encryption_public_key == [0; 32]
            || unshielded_recipient == [0; 32]
        {
            return Err(PassportVaultCallPortError::InvalidData);
        }
        Ok(Self {
            network_id,
            zswap_chain_state,
            ledger_parameters,
            coin_public_key,
            encryption_public_key,
            unshielded_recipient,
        })
    }
}

/// Supplies fresh public wallet and chain context without coupling the
/// Passport Vault adapter to the generic Midnight wallet adapter.
pub trait PassportVaultCallCompositionContextSource: Send + Sync {
    fn context(
        &self,
        profile_id: &OpaqueId,
    ) -> Result<PassportVaultCallCompositionContext, PassportVaultCallPortError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultCallComposerConfigError {
    PathNotAbsolute,
    ExecutableUnavailable,
    ExecutableSymlink,
}

impl fmt::Display for PassportVaultCallComposerConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PathNotAbsolute => "Passport Vault composer path must be absolute",
            Self::ExecutableUnavailable => "Passport Vault composer executable is unavailable",
            Self::ExecutableSymlink => "Passport Vault composer executable must not be a symlink",
        })
    }
}

impl std::error::Error for PassportVaultCallComposerConfigError {}

trait PassportVaultCallComposer: Send + Sync {
    fn compose(
        &self,
        request: &PreparePassportVaultCallRequest,
        context: &PassportVaultCallCompositionContext,
    ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError>;
}

struct ProcessPassportVaultCallComposer {
    executable: PathBuf,
}

impl ProcessPassportVaultCallComposer {
    fn new(executable: impl AsRef<Path>) -> Result<Self, PassportVaultCallComposerConfigError> {
        let executable = executable.as_ref();
        if !executable.is_absolute() {
            return Err(PassportVaultCallComposerConfigError::PathNotAbsolute);
        }
        let metadata = fs::symlink_metadata(executable)
            .map_err(|_| PassportVaultCallComposerConfigError::ExecutableUnavailable)?;
        if metadata.file_type().is_symlink() {
            return Err(PassportVaultCallComposerConfigError::ExecutableSymlink);
        }
        if !metadata.is_file() {
            return Err(PassportVaultCallComposerConfigError::ExecutableUnavailable);
        }
        let canonical = fs::canonicalize(executable)
            .map_err(|_| PassportVaultCallComposerConfigError::ExecutableUnavailable)?;
        if canonical != executable {
            return Err(PassportVaultCallComposerConfigError::ExecutableSymlink);
        }
        Ok(Self {
            executable: canonical,
        })
    }
}

impl PassportVaultCallComposer for ProcessPassportVaultCallComposer {
    fn compose(
        &self,
        request: &PreparePassportVaultCallRequest,
        context: &PassportVaultCallCompositionContext,
    ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
        let composer_request = ComposerRequest::from_call(request, context)?;
        let body = Zeroizing::new(
            serde_json::to_vec(&composer_request)
                .map_err(|_| PassportVaultCallPortError::InvalidData)?,
        );
        if body.is_empty() || body.len() > MAX_COMPOSER_REQUEST_BYTES {
            return Err(PassportVaultCallPortError::InvalidData);
        }
        let mut output = run_composer(&self.executable, body)?;
        let status = output.status;
        let stderr_is_empty = output.stderr.is_empty();
        let response = serde_json::from_slice(&output.stdout);
        output.stdout.zeroize();
        output.stderr.zeroize();
        let response: ComposerResponse =
            response.map_err(|_| PassportVaultCallPortError::InvalidData)?;
        match response {
            ComposerResponse::Success(mut success) => {
                if !status.success()
                    || !stderr_is_empty
                    || success.schema_version != 1
                    || !success.ok
                    || success.operation_kind != request.operation.kind().name()
                    || success.circuit_id != circuit_id(&request.operation)
                {
                    success.unproven_transaction_hex.zeroize();
                    return Err(PassportVaultCallPortError::InvalidData);
                }
                let bytes = hex::decode(&success.unproven_transaction_hex);
                success.unproven_transaction_hex.zeroize();
                let mut bytes = bytes.map_err(|_| PassportVaultCallPortError::InvalidData)?;
                if bytes.is_empty()
                    || bytes.len() != success.unproven_transaction_bytes
                    || bytes.len() > MAX_COMPOSER_RESPONSE_BYTES
                {
                    bytes.zeroize();
                    return Err(PassportVaultCallPortError::InvalidData);
                }
                Ok(Zeroizing::new(bytes))
            }
            ComposerResponse::Failure(failure) => {
                if status.success()
                    || !stderr_is_empty
                    || failure.schema_version != 1
                    || failure.ok
                    || failure.error.message.is_empty()
                {
                    return Err(PassportVaultCallPortError::InvalidData);
                }
                Err(map_composer_error(&failure.error.code))
            }
        }
    }
}

struct ComposerOutput {
    status: ExitStatus,
    stdout: Zeroizing<Vec<u8>>,
    stderr: Zeroizing<Vec<u8>>,
}

fn run_composer(
    executable: &Path,
    request: Zeroizing<Vec<u8>>,
) -> Result<ComposerOutput, PassportVaultCallPortError> {
    let mut child = Command::new(executable)
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| PassportVaultCallPortError::Unavailable)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(PassportVaultCallPortError::Unavailable)?;
    let stderr = child
        .stderr
        .take()
        .ok_or(PassportVaultCallPortError::Unavailable)?;
    let stdout_reader = read_bounded(stdout, MAX_COMPOSER_RESPONSE_BYTES);
    let stderr_reader = read_bounded(stderr, MAX_COMPOSER_STDERR_BYTES);
    let write_result = child
        .stdin
        .take()
        .ok_or(PassportVaultCallPortError::Unavailable)
        .and_then(|mut stdin| {
            stdin
                .write_all(&request)
                .and_then(|()| stdin.flush())
                .map_err(|_| PassportVaultCallPortError::Unavailable)
        });
    if let Err(error) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        return Err(error);
    }

    let deadline = Instant::now() + COMPOSER_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(PassportVaultCallPortError::Timeout);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(PassportVaultCallPortError::Unavailable);
            }
        }
    };
    let stdout = join_reader(stdout_reader)?;
    let stderr = join_reader(stderr_reader)?;
    Ok(ComposerOutput {
        status,
        stdout,
        stderr,
    })
}

fn read_bounded<R>(
    mut reader: R,
    maximum: usize,
) -> thread::JoinHandle<Result<Zeroizing<Vec<u8>>, ()>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Zeroizing::new(Vec::new());
        reader
            .by_ref()
            .take((maximum + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ())?;
        if bytes.len() > maximum {
            return Err(());
        }
        Ok(bytes)
    })
}

fn join_reader(
    reader: thread::JoinHandle<Result<Zeroizing<Vec<u8>>, ()>>,
) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
    reader
        .join()
        .map_err(|_| PassportVaultCallPortError::Unavailable)?
        .map_err(|()| PassportVaultCallPortError::InvalidData)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerRequest {
    schema_version: u8,
    operation: ComposerOperation,
    chain: ComposerChain,
    wallet: ComposerWallet,
}

impl ComposerRequest {
    fn from_call(
        request: &PreparePassportVaultCallRequest,
        context: &PassportVaultCallCompositionContext,
    ) -> Result<Self, PassportVaultCallPortError> {
        let operation = match &request.operation {
            PassportVaultCallOperation::CreateLock {
                policy,
                initial_amount,
            } => ComposerOperation::Create {
                minimum_age_years: policy.minimum_age_years(),
                required_issuing_state_hex: policy.required_issuing_state().map(hex::encode),
                required_document_number_hex: policy.required_document_number().map(hex::encode),
                maximum_claim_amount: policy.maximum_claim_amount().to_string(),
                verifier_challenge_hash_hex: hex::encode(policy.verifier_challenge_hash()),
                initial_amount: initial_amount.to_string(),
            },
            PassportVaultCallOperation::DepositToLock { lock_id, amount } => {
                ComposerOperation::Deposit {
                    lock_id: lock_id.to_string(),
                    amount: amount.to_string(),
                }
            }
            PassportVaultCallOperation::WithdrawFromLock { lock_id, amount } => {
                ComposerOperation::Withdraw {
                    lock_id: lock_id.to_string(),
                    amount: amount.to_string(),
                    recipient_address_hex: hex::encode(context.unshielded_recipient),
                }
            }
            PassportVaultCallOperation::ClaimFromLock { .. } => {
                return Err(PassportVaultCallPortError::Unavailable);
            }
        };
        Ok(Self {
            schema_version: 1,
            operation,
            chain: ComposerChain {
                contract_state_hex: hex::encode(&request.contract_state.serialized_contract_state),
                contract_address_hex: request.contract_state.contract_address_hex.clone(),
                zswap_chain_state_hex: hex::encode(&context.zswap_chain_state),
                ledger_parameters_hex: hex::encode(&context.ledger_parameters),
                network_id: context.network_id.clone(),
            },
            wallet: ComposerWallet {
                coin_public_key_hex: hex::encode(context.coin_public_key),
                encryption_public_key_hex: hex::encode(context.encryption_public_key),
            },
        })
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ComposerOperation {
    #[serde(rename = "create_lock")]
    #[serde(rename_all = "camelCase")]
    Create {
        minimum_age_years: u8,
        required_issuing_state_hex: Option<String>,
        required_document_number_hex: Option<String>,
        maximum_claim_amount: String,
        verifier_challenge_hash_hex: String,
        initial_amount: String,
    },
    #[serde(rename = "deposit_to_lock", rename_all = "camelCase")]
    Deposit { lock_id: String, amount: String },
    #[serde(rename = "withdraw_from_lock")]
    #[serde(rename_all = "camelCase")]
    Withdraw {
        lock_id: String,
        amount: String,
        recipient_address_hex: String,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerChain {
    contract_state_hex: String,
    contract_address_hex: String,
    zswap_chain_state_hex: String,
    ledger_parameters_hex: String,
    network_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerWallet {
    coin_public_key_hex: String,
    encryption_public_key_hex: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ComposerResponse {
    Success(ComposerSuccess),
    Failure(ComposerFailure),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComposerSuccess {
    schema_version: u8,
    ok: bool,
    operation_kind: String,
    circuit_id: String,
    unproven_transaction_hex: String,
    unproven_transaction_bytes: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComposerFailure {
    schema_version: u8,
    ok: bool,
    error: ComposerFailureDetail,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComposerFailureDetail {
    code: String,
    message: String,
}

fn map_composer_error(code: &str) -> PassportVaultCallPortError {
    match code {
        "unavailable" => PassportVaultCallPortError::Unavailable,
        "composition_failed" => PassportVaultCallPortError::InvalidChainState,
        "claim_requires_protected_custody" => PassportVaultCallPortError::Unavailable,
        "invalid_request"
        | "request_too_large"
        | "unsupported_operation"
        | "administrative_circuit_forbidden" => PassportVaultCallPortError::InvalidData,
        _ => PassportVaultCallPortError::InvalidData,
    }
}

struct RetainedNativeCall {
    planning_fingerprint: [u8; 32],
    preview: PassportVaultCallPreview,
    submission_status: PassportVaultCallSubmissionStatus,
    unproven_transaction: Zeroizing<Vec<u8>>,
}

pub struct NativePassportVaultContractCall {
    contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
    composer: Arc<dyn PassportVaultCallComposer>,
    calls: Mutex<BTreeMap<CallKey, RetainedNativeCall>>,
}

impl NativePassportVaultContractCall {
    pub fn new(
        executable: impl AsRef<Path>,
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
    ) -> Result<Self, PassportVaultCallComposerConfigError> {
        Ok(Self {
            contexts,
            composer: Arc::new(ProcessPassportVaultCallComposer::new(executable)?),
            calls: Mutex::new(BTreeMap::new()),
        })
    }

    #[cfg(test)]
    fn with_composer(
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
        composer: Arc<dyn PassportVaultCallComposer>,
    ) -> Self {
        Self {
            contexts,
            composer,
            calls: Mutex::new(BTreeMap::new()),
        }
    }
}

impl PassportVaultContractCallPort for NativePassportVaultContractCall {
    fn prepare(
        &self,
        request: PreparePassportVaultCallRequest,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
        if request.contract_state.authentication
            != PassportVaultContractStateAuthentication::CanonicalFinalizedReplay
            || request.contract_state.serialized_contract_state.is_empty()
            || request.contract_state.serialized_contract_state.len()
                > MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES
            || !valid_hex_32(&request.contract_state.contract_address_hex)
            || !valid_hex_32(&request.contract_state.transaction_hash_hex)
            || !valid_hex_32(&request.contract_state.action_block_hash_hex)
            || !valid_hex_32(&request.contract_state.finalized_head_hash_hex)
            || request.contract_state.action_block_height
                > request.contract_state.finalized_head_height
        {
            return Err(PassportVaultCallPortError::InvalidChainState);
        }
        if matches!(
            request.operation,
            PassportVaultCallOperation::ClaimFromLock { .. }
        ) {
            return Err(PassportVaultCallPortError::Unavailable);
        }
        let context = self.contexts.context(&request.profile_id)?;
        let planning_fingerprint = planning_fingerprint(&request, &context);
        {
            let calls = self
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
        }

        let unproven_transaction = self.composer.compose(&request, &context)?;
        validate_unproven_transaction(&unproven_transaction, &context.network_id)?;
        let draft_id = PassportVaultCallDraftId::parse(hex::encode(planning_fingerprint))
            .map_err(|_| PassportVaultCallPortError::InvalidData)?;
        let authorization_challenge = authorization_challenge(
            &draft_id,
            &request.contract_state.action_block_hash_hex,
            &unproven_transaction,
        )?;
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
        let key = (request.profile_id.clone(), draft_id.clone());
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?;
        if let Some(existing) = calls.values().find(|retained| {
            retained.preview.contract_address_hex == preview.contract_address_hex
                && retained.planning_fingerprint == planning_fingerprint
        }) {
            return Ok(existing.preview.clone());
        }
        if let Some(existing) = calls.get(&key) {
            return if existing.preview == preview {
                Ok(existing.preview.clone())
            } else {
                Err(PassportVaultCallPortError::DraftConflict)
            };
        }
        let profile_count = calls
            .keys()
            .filter(|(profile_id, _)| profile_id == &request.profile_id)
            .count();
        if profile_count >= MAX_PASSPORT_VAULT_CALL_SUBMISSION_HISTORY {
            return Err(PassportVaultCallPortError::Unavailable);
        }
        calls.insert(
            key,
            RetainedNativeCall {
                planning_fingerprint,
                preview: preview.clone(),
                submission_status: empty_status(draft_id),
                unproven_transaction,
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
            .get_mut(&(profile_id.clone(), request.draft_id))
            .ok_or(PassportVaultCallPortError::DraftNotFound)?;
        expire_if_needed(retained, request.now);
        if retained.preview.state == PassportVaultCallDraftState::Expired {
            return Err(PassportVaultCallPortError::DraftExpired);
        }
        if retained.preview.authorization_challenge != request.authorization_challenge {
            return Err(PassportVaultCallPortError::AuthorizationChallengeMismatch);
        }
        match retained.preview.state {
            PassportVaultCallDraftState::Prepared => {
                retained.preview.state = PassportVaultCallDraftState::Authorized;
                Ok(retained.preview.clone())
            }
            PassportVaultCallDraftState::Authorized => Ok(retained.preview.clone()),
            PassportVaultCallDraftState::Submitting
            | PassportVaultCallDraftState::Submitted
            | PassportVaultCallDraftState::Expired => {
                Err(PassportVaultCallPortError::DraftConflict)
            }
        }
    }

    fn submit<'a>(
        &'a self,
        profile_id: &'a OpaqueId,
        request: SubmitPassportVaultCallRequest,
    ) -> PassportVaultCallSubmissionFuture<'a> {
        Box::pin(async move {
            let mut calls = self
                .calls
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            let retained = calls
                .get_mut(&(profile_id.clone(), request.draft_id))
                .ok_or(PassportVaultCallPortError::DraftNotFound)?;
            expire_if_needed(retained, request.now);
            match retained.preview.state {
                PassportVaultCallDraftState::Expired => {
                    Err(PassportVaultCallPortError::DraftExpired)
                }
                PassportVaultCallDraftState::Authorized
                    if !retained.unproven_transaction.is_empty() =>
                {
                    // The next adapter slice will consume this retained value
                    // behind protected funding/proving and submission ports.
                    Err(PassportVaultCallPortError::Unavailable)
                }
                PassportVaultCallDraftState::Submitting => {
                    Err(PassportVaultCallPortError::SubmissionInProgress)
                }
                _ => Err(PassportVaultCallPortError::DraftConflict),
            }
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
        self.submission_status(profile_id, draft_id)?;
        Err(PassportVaultCallPortError::SubmissionNotInProgress)
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
            self.submission_status(profile_id, draft_id)?;
            Err(PassportVaultCallPortError::SubmissionNotInProgress)
        })
    }
}

fn validate_unproven_transaction(
    bytes: &[u8],
    network_id: &str,
) -> Result<(), PassportVaultCallPortError> {
    let mut cursor = Cursor::new(bytes);
    let transaction: UnprovenTransaction =
        tagged_deserialize(&mut cursor).map_err(|_| PassportVaultCallPortError::InvalidData)?;
    if cursor.position() != bytes.len() as u64 {
        return Err(PassportVaultCallPortError::InvalidData);
    }
    let Transaction::Standard(standard) = transaction else {
        return Err(PassportVaultCallPortError::InvalidData);
    };
    if standard.network_id != network_id || standard.intents.iter().count() != 1 {
        return Err(PassportVaultCallPortError::InvalidData);
    }
    Ok(())
}

fn planning_fingerprint(
    request: &PreparePassportVaultCallRequest,
    context: &PassportVaultCallCompositionContext,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:native-passport-vault-plan:v1\0");
    digest.update(request.profile_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(request.contract_state.contract_address_hex.as_bytes());
    digest.update(request.contract_state.transaction_hash_hex.as_bytes());
    digest.update(request.contract_state.action_block_hash_hex.as_bytes());
    digest.update(request.contract_state.action_block_height.to_be_bytes());
    digest.update(request.contract_state.serialized_contract_state.as_slice());
    digest.update(request.expires_at.value().to_be_bytes());
    update_operation_digest(&mut digest, &request.operation);
    digest.update(context.network_id.as_bytes());
    digest.update(context.zswap_chain_state.as_slice());
    digest.update(context.ledger_parameters.as_slice());
    digest.update(context.coin_public_key);
    digest.update(context.encryption_public_key);
    digest.update(context.unshielded_recipient);
    digest.finalize().into()
}

fn authorization_challenge(
    draft_id: &PassportVaultCallDraftId,
    anchor_block_hash_hex: &str,
    transaction: &[u8],
) -> Result<PassportVaultCallAuthorizationChallenge, PassportVaultCallPortError> {
    let mut digest = Sha256::new();
    digest.update(b"oxid:native-passport-vault-authorization:v1\0");
    digest.update(draft_id.as_str().as_bytes());
    digest.update(anchor_block_hash_hex.as_bytes());
    digest.update(Sha256::digest(transaction));
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

fn circuit_id(operation: &PassportVaultCallOperation) -> &'static str {
    match operation {
        PassportVaultCallOperation::CreateLock { .. } => "createLock",
        PassportVaultCallOperation::DepositToLock { .. } => "depositToLock",
        PassportVaultCallOperation::ClaimFromLock { .. } => "claimFromLock",
        PassportVaultCallOperation::WithdrawFromLock { .. } => "withdrawFromLock",
    }
}

fn valid_network_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && (value.as_bytes()[0].is_ascii_lowercase() || value.as_bytes()[0].is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_hex_32(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn expire_if_needed(retained: &mut RetainedNativeCall, now: UnixTimestampMillis) {
    if now.value() >= retained.preview.expires_at.value()
        && matches!(
            retained.preview.state,
            PassportVaultCallDraftState::Prepared | PassportVaultCallDraftState::Authorized
        )
    {
        retained.preview.state = PassportVaultCallDraftState::Expired;
        retained.submission_status.state = PassportVaultCallSubmissionState::Expired;
        retained.unproven_transaction.zeroize();
    }
}

fn empty_status(draft_id: PassportVaultCallDraftId) -> PassportVaultCallSubmissionStatus {
    PassportVaultCallSubmissionStatus {
        draft_id,
        state: PassportVaultCallSubmissionState::NotStarted,
        transaction_hash_hex: None,
        block_hash_hex: None,
        block_height: None,
        fee_atomic_units: None,
        mode: None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::executor::block_on;
    use midnight_base_crypto::{schnorr::Signature, time::Timestamp};
    use midnight_ledger::structure::{Intent, ProofPreimageMarker, StandardTransaction};
    use midnight_serialize::tagged_serialize;
    use midnight_storage::{DefaultDB, storage::HashMap as LedgerHashMap};
    use midnight_transient_crypto::commitment::PedersenRandomness;
    use oxid_passport_vault_domain::PassportVaultPolicy;
    use rand::rngs::OsRng;

    use super::*;

    struct ContextSource {
        calls: AtomicUsize,
    }

    impl PassportVaultCallCompositionContextSource for ContextSource {
        fn context(
            &self,
            _: &OpaqueId,
        ) -> Result<PassportVaultCallCompositionContext, PassportVaultCallPortError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            context()
        }
    }

    struct Composer {
        calls: AtomicUsize,
    }

    impl PassportVaultCallComposer for Composer {
        fn compose(
            &self,
            _: &PreparePassportVaultCallRequest,
            context: &PassportVaultCallCompositionContext,
        ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut rng = OsRng;
            let intent: Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB> =
                Intent::empty(&mut rng, Timestamp::from_secs(1_800_000_000));
            let transaction = Transaction::Standard(StandardTransaction::new(
                &context.network_id,
                LedgerHashMap::from_iter([(7, intent)]),
                None,
                LedgerHashMap::new(),
            ));
            let mut bytes = Vec::new();
            tagged_serialize(&transaction, &mut bytes)
                .map_err(|_| PassportVaultCallPortError::InvalidData)?;
            Ok(Zeroizing::new(bytes))
        }
    }

    fn profile() -> OpaqueId {
        OpaqueId::parse("profile_native_vault").expect("profile")
    }

    fn context() -> Result<PassportVaultCallCompositionContext, PassportVaultCallPortError> {
        PassportVaultCallCompositionContext::new(
            "undeployed",
            vec![1, 2, 3],
            vec![4, 5, 6],
            [7; 32],
            [8; 32],
            [9; 32],
        )
    }

    fn request(operation: PassportVaultCallOperation) -> PreparePassportVaultCallRequest {
        PreparePassportVaultCallRequest {
            profile_id: profile(),
            contract_state: oxid_passport_vault_application::PassportVaultContractStateSnapshot {
                serialized_contract_state: vec![1, 2, 3],
                authentication: PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
                contract_address_hex: "11".repeat(32),
                transaction_hash_hex: "22".repeat(32),
                action_block_hash_hex: "33".repeat(32),
                action_block_height: 42,
                finalized_head_hash_hex: "44".repeat(32),
                finalized_head_height: 45,
            },
            operation,
            expires_at: UnixTimestampMillis::new(10_000),
        }
    }

    fn create_operation() -> PassportVaultCallOperation {
        PassportVaultCallOperation::CreateLock {
            policy: PassportVaultPolicy::new(18, None, None, 40, [5; 32]).expect("policy"),
            initial_amount: 10,
        }
    }

    fn adapter() -> (
        NativePassportVaultContractCall,
        Arc<ContextSource>,
        Arc<Composer>,
    ) {
        let contexts = Arc::new(ContextSource {
            calls: AtomicUsize::new(0),
        });
        let composer = Arc::new(Composer {
            calls: AtomicUsize::new(0),
        });
        (
            NativePassportVaultContractCall::with_composer(contexts.clone(), composer.clone()),
            contexts,
            composer,
        )
    }

    #[test]
    fn retains_a_native_composed_draft_without_claiming_submission() {
        let (adapter, contexts, composer) = adapter();
        let prepared = adapter
            .prepare(request(create_operation()))
            .expect("prepare");
        assert_eq!(prepared.state, PassportVaultCallDraftState::Prepared);
        let repeated = adapter
            .prepare(request(create_operation()))
            .expect("idempotent prepare");
        assert_eq!(repeated, prepared);
        assert_eq!(contexts.calls.load(Ordering::SeqCst), 2);
        assert_eq!(composer.calls.load(Ordering::SeqCst), 1);

        let authorized = adapter
            .authorize(
                &profile(),
                AuthorizePassportVaultCallRequest {
                    draft_id: prepared.draft_id.clone(),
                    authorization_challenge: prepared.authorization_challenge,
                    now: UnixTimestampMillis::new(1_000),
                },
            )
            .expect("authorize");
        assert_eq!(authorized.state, PassportVaultCallDraftState::Authorized);
        assert_eq!(
            block_on(adapter.submit(
                &profile(),
                SubmitPassportVaultCallRequest {
                    draft_id: prepared.draft_id.clone(),
                    now: UnixTimestampMillis::new(2_000),
                }
            )),
            Err(PassportVaultCallPortError::Unavailable)
        );
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    &prepared.draft_id,
                    UnixTimestampMillis::new(2_001)
                )
                .expect("retained")
                .state,
            PassportVaultCallDraftState::Authorized
        );
        assert_eq!(
            adapter
                .submission_status(&profile(), &prepared.draft_id)
                .expect("status")
                .state,
            PassportVaultCallSubmissionState::NotStarted
        );
    }

    #[test]
    fn maps_all_public_composer_operations_to_the_exact_schema() {
        let context = context().expect("context");
        let operations = [
            create_operation(),
            PassportVaultCallOperation::DepositToLock {
                lock_id: 2,
                amount: 5,
            },
            PassportVaultCallOperation::WithdrawFromLock {
                lock_id: 2,
                amount: 1,
            },
        ];
        let expected = ["create_lock", "deposit_to_lock", "withdraw_from_lock"];
        for (operation, expected_kind) in operations.into_iter().zip(expected) {
            let request = request(operation);
            let value = serde_json::to_value(
                ComposerRequest::from_call(&request, &context).expect("request"),
            )
            .expect("JSON");
            assert_eq!(value["operation"]["kind"], expected_kind);
            assert!(value.get("secret").is_none());
            assert!(value.get("credential").is_none());
            assert!(value.get("transaction").is_none());
        }
    }

    #[test]
    fn rejects_claims_and_unauthenticated_state_before_public_context_or_composition() {
        let (adapter, contexts, composer) = adapter();
        let claim = PassportVaultCallOperation::ClaimFromLock {
            lock_id: 1,
            amount: 1,
            credential_id: OpaqueId::parse("credential_1").expect("credential"),
        };
        assert_eq!(
            adapter.prepare(request(claim)),
            Err(PassportVaultCallPortError::Unavailable)
        );
        let mut unauthenticated = request(create_operation());
        unauthenticated.contract_state.authentication =
            PassportVaultContractStateAuthentication::IndexerSuppliedNotProven;
        assert_eq!(
            adapter.prepare(unauthenticated),
            Err(PassportVaultCallPortError::InvalidChainState)
        );
        assert_eq!(contexts.calls.load(Ordering::SeqCst), 0);
        assert_eq!(composer.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn expiry_erases_material_and_keeps_submission_closed() {
        let (adapter, _, _) = adapter();
        let prepared = adapter
            .prepare(request(create_operation()))
            .expect("prepare");
        assert_eq!(
            adapter.authorize(
                &profile(),
                AuthorizePassportVaultCallRequest {
                    draft_id: prepared.draft_id.clone(),
                    authorization_challenge: prepared.authorization_challenge,
                    now: UnixTimestampMillis::new(10_000),
                }
            ),
            Err(PassportVaultCallPortError::DraftExpired)
        );
        assert_eq!(
            adapter
                .submission_status(&profile(), &prepared.draft_id)
                .expect("status")
                .state,
            PassportVaultCallSubmissionState::Expired
        );
    }

    #[test]
    fn context_requires_real_chain_snapshots_and_nonzero_public_keys() {
        assert_eq!(
            PassportVaultCallCompositionContext::new(
                "UPPERCASE",
                vec![1],
                vec![2],
                [3; 32],
                [4; 32],
                [5; 32]
            ),
            Err(PassportVaultCallPortError::UnsupportedNetwork)
        );
        assert_eq!(
            PassportVaultCallCompositionContext::new(
                "undeployed",
                Vec::new(),
                vec![2],
                [3; 32],
                [4; 32],
                [5; 32]
            ),
            Err(PassportVaultCallPortError::InvalidChainState)
        );
        assert_eq!(
            PassportVaultCallCompositionContext::new(
                "undeployed",
                vec![1],
                vec![2],
                [0; 32],
                [4; 32],
                [5; 32]
            ),
            Err(PassportVaultCallPortError::InvalidData)
        );
    }
}
