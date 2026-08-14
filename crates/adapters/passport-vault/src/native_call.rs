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
use oxid_adapter_vc_midnight::{
    PreparedDigitalPassportPresentation, ProtectedDigitalPassportPresentationError,
    ProtectedDigitalPassportPresentationRequest, ProtectedDigitalPassportPresentationSource,
};
use oxid_foundation::{OpaqueId, UnixTimestampMillis};
use oxid_passport_vault_application::{
    AuthorizePassportVaultCallRequest, MAX_PASSPORT_VAULT_CALL_SUBMISSION_HISTORY,
    MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES, PassportVaultCallAuthorizationChallenge,
    PassportVaultCallDraftId, PassportVaultCallDraftState, PassportVaultCallInclusion,
    PassportVaultCallOperation, PassportVaultCallPortError, PassportVaultCallPreview,
    PassportVaultCallStatusFuture, PassportVaultCallSubmissionFuture,
    PassportVaultCallSubmissionState, PassportVaultCallSubmissionStatus,
    PassportVaultContractCallPort, PassportVaultContractStateAuthentication,
    PreparePassportVaultCallRequest, SubmitPassportVaultCallRequest, SubmittedPassportVaultCall,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use crate::contract_state::{PassportVaultProtectedClaimContext, decode_protected_claim_context};

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
        profile_id: &str,
        contract_state: &oxid_passport_vault_application::PassportVaultContractStateSnapshot,
    ) -> Result<PassportVaultCallCompositionContext, PassportVaultCallPortError>;
}

/// Sensitive generated transaction passed only to a composition-local funding
/// adapter after the user authorizes the exact Passport Vault operation.
pub struct PassportVaultCallFundingRequest {
    profile_id: String,
    network_id: String,
    expires_at_seconds: u64,
    requires_night_funding: bool,
    transaction: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for PassportVaultCallFundingRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportVaultCallFundingRequest")
            .field("profile_id", &self.profile_id)
            .field("network_id", &self.network_id)
            .field("expires_at_seconds", &self.expires_at_seconds)
            .field("requires_night_funding", &self.requires_night_funding)
            .field("transaction_bytes", &self.transaction.len())
            .finish_non_exhaustive()
    }
}

impl PassportVaultCallFundingRequest {
    #[must_use]
    pub fn into_parts(self) -> (String, String, u64, bool, Zeroizing<Vec<u8>>) {
        (
            self.profile_id,
            self.network_id,
            self.expires_at_seconds,
            self.requires_night_funding,
            self.transaction,
        )
    }
}

/// Funded transaction returned directly to retained adapter custody.
pub struct FundedPassportVaultCall {
    transaction: Zeroizing<Vec<u8>>,
    funded_night_atomic_units: u128,
    funding_input_count: u16,
}

impl fmt::Debug for FundedPassportVaultCall {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FundedPassportVaultCall")
            .field("transaction_bytes", &self.transaction.len())
            .field("funded_night_atomic_units", &self.funded_night_atomic_units)
            .field("funding_input_count", &self.funding_input_count)
            .finish_non_exhaustive()
    }
}

impl FundedPassportVaultCall {
    #[must_use]
    pub fn new(
        transaction: Zeroizing<Vec<u8>>,
        funded_night_atomic_units: u128,
        funding_input_count: u16,
    ) -> Self {
        Self {
            transaction,
            funded_night_atomic_units,
            funding_input_count,
        }
    }

    #[must_use]
    pub fn into_transaction(self) -> Zeroizing<Vec<u8>> {
        self.transaction
    }
}

/// Composition-only protected funding boundary. Implementations must never
/// project the serialized transaction through an incoming/application view.
pub trait PassportVaultCallFundingPort: Send + Sync {
    fn fund(
        &self,
        request: PassportVaultCallFundingRequest,
    ) -> Result<FundedPassportVaultCall, PassportVaultCallPortError>;
}

/// Funded call plus public identifiers passed only to the protected Midnight
/// completion bridge. The transaction remains zeroizing adapter-owned data.
pub struct PassportVaultCallCompletionRequest {
    profile_id: String,
    network_id: String,
    draft_id: String,
    planning_fingerprint: [u8; 32],
    expires_at: UnixTimestampMillis,
    updated_at: UnixTimestampMillis,
    transaction: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for PassportVaultCallCompletionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassportVaultCallCompletionRequest")
            .field("profile_id", &self.profile_id)
            .field("network_id", &self.network_id)
            .field("draft_id", &self.draft_id)
            .field("expires_at", &self.expires_at)
            .field("updated_at", &self.updated_at)
            .field("transaction_bytes", &self.transaction.len())
            .finish_non_exhaustive()
    }
}

impl PassportVaultCallCompletionRequest {
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        String,
        String,
        String,
        [u8; 32],
        UnixTimestampMillis,
        UnixTimestampMillis,
        Zeroizing<Vec<u8>>,
    ) {
        (
            self.profile_id,
            self.network_id,
            self.draft_id,
            self.planning_fingerprint,
            self.expires_at,
            self.updated_at,
            self.transaction,
        )
    }
}

/// Composition-only settlement and recovery boundary for native vault calls.
pub trait PassportVaultCallCompletionPort: Send + Sync {
    fn complete(
        &self,
        request: PassportVaultCallCompletionRequest,
    ) -> Result<PassportVaultCallInclusion, PassportVaultCallPortError>;

    fn status(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError>;

    fn cancel(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError>;

    fn history(
        &self,
        profile_id: &str,
    ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError>;

    fn reconcile(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError>;
}

#[derive(Clone, Copy, Debug, Default)]
struct UnavailablePassportVaultCallCompletion;

impl PassportVaultCallCompletionPort for UnavailablePassportVaultCallCompletion {
    fn complete(
        &self,
        _: PassportVaultCallCompletionRequest,
    ) -> Result<PassportVaultCallInclusion, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }

    fn status(
        &self,
        _: &str,
        _: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::DraftNotFound)
    }

    fn cancel(
        &self,
        _: &str,
        _: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::SubmissionNotInProgress)
    }

    fn history(
        &self,
        _: &str,
    ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError> {
        Ok(Vec::new())
    }

    fn reconcile(
        &self,
        _: &str,
        _: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::DraftNotFound)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct UnavailablePassportVaultCallFunding;

impl PassportVaultCallFundingPort for UnavailablePassportVaultCallFunding {
    fn fund(
        &self,
        _: PassportVaultCallFundingRequest,
    ) -> Result<FundedPassportVaultCall, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
struct PassthroughTestFunding;

#[cfg(test)]
impl PassportVaultCallFundingPort for PassthroughTestFunding {
    fn fund(
        &self,
        request: PassportVaultCallFundingRequest,
    ) -> Result<FundedPassportVaultCall, PassportVaultCallPortError> {
        let (_, _, _, _, transaction) = request.into_parts();
        Ok(FundedPassportVaultCall::new(transaction, 0, 0))
    }
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

    fn compose_claim(
        &self,
        _: &PreparePassportVaultCallRequest,
        _: &PassportVaultCallCompositionContext,
        _: PreparedDigitalPassportPresentation,
    ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }
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

    fn compose_request(
        &self,
        request: &PreparePassportVaultCallRequest,
        composer_request: ComposerRequest,
    ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
        let body = Zeroizing::new(
            serde_json::to_vec(&composer_request)
                .map_err(|_| PassportVaultCallPortError::InvalidData)?,
        );
        drop(composer_request);
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

impl PassportVaultCallComposer for ProcessPassportVaultCallComposer {
    fn compose(
        &self,
        request: &PreparePassportVaultCallRequest,
        context: &PassportVaultCallCompositionContext,
    ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
        let composer_request = ComposerRequest::from_call(request, context)?;
        self.compose_request(request, composer_request)
    }

    fn compose_claim(
        &self,
        request: &PreparePassportVaultCallRequest,
        context: &PassportVaultCallCompositionContext,
        presentation: PreparedDigitalPassportPresentation,
    ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
        let composer_request = ComposerRequest::from_claim(request, context, presentation)?;
        self.compose_request(request, composer_request)
    }
}

struct ComposerOutput {
    status: ExitStatus,
    stdout: Zeroizing<Vec<u8>>,
    stderr: Zeroizing<Vec<u8>>,
}

fn run_composer(
    executable: &Path,
    mut request: Zeroizing<Vec<u8>>,
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
    request.zeroize();
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

    fn from_claim(
        request: &PreparePassportVaultCallRequest,
        context: &PassportVaultCallCompositionContext,
        material: PreparedDigitalPassportPresentation,
    ) -> Result<Self, PassportVaultCallPortError> {
        let PassportVaultCallOperation::ClaimFromLock {
            lock_id, amount, ..
        } = &request.operation
        else {
            return Err(PassportVaultCallPortError::InvalidData);
        };
        Ok(Self {
            schema_version: 1,
            operation: ComposerOperation::Claim {
                lock_id: lock_id.to_string(),
                amount: amount.to_string(),
                recipient_address_hex: hex::encode(context.unshielded_recipient),
                material: Box::new(material),
            },
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
    #[serde(rename = "claim_from_lock")]
    #[serde(rename_all = "camelCase")]
    Claim {
        lock_id: String,
        amount: String,
        recipient_address_hex: String,
        material: Box<PreparedDigitalPassportPresentation>,
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

trait PassportVaultProtectedClaimComposer: Send + Sync {
    fn compose_after_authorization(
        &self,
        request: &PreparePassportVaultCallRequest,
        context: &PassportVaultCallCompositionContext,
        policy: &PassportVaultProtectedClaimContext,
    ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError>;
}

struct UnavailablePassportVaultProtectedClaimComposer;

impl PassportVaultProtectedClaimComposer for UnavailablePassportVaultProtectedClaimComposer {
    fn compose_after_authorization(
        &self,
        _: &PreparePassportVaultCallRequest,
        _: &PassportVaultCallCompositionContext,
        _: &PassportVaultProtectedClaimContext,
    ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
        Err(PassportVaultCallPortError::Unavailable)
    }
}

struct ManagedPassportVaultProtectedClaimComposer {
    presentations: Arc<ProtectedDigitalPassportPresentationSource>,
    composer: ProcessPassportVaultCallComposer,
}

impl PassportVaultProtectedClaimComposer for ManagedPassportVaultProtectedClaimComposer {
    fn compose_after_authorization(
        &self,
        request: &PreparePassportVaultCallRequest,
        context: &PassportVaultCallCompositionContext,
        policy: &PassportVaultProtectedClaimContext,
    ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
        let PassportVaultCallOperation::ClaimFromLock { credential_id, .. } = &request.operation
        else {
            return Err(PassportVaultCallPortError::InvalidData);
        };
        let presentation = futures::executor::block_on(self.presentations.prepare(
            ProtectedDigitalPassportPresentationRequest {
                profile_id: request.profile_id.as_str().to_owned(),
                credential_id: credential_id.as_str().to_owned(),
                verifier: format!(
                    "midnight:passport-vault:{}",
                    request.contract_state.contract_address_hex
                ),
                verifier_challenge_hash: policy.verifier_challenge_hash,
                trusted_issuer_did_contract: policy.trusted_issuer_did_contract,
                trusted_issuer_method: policy.trusted_issuer_method,
                trusted_issuer_public_key_hash: policy.trusted_issuer_public_key_hash,
                minimum_age_years: policy.minimum_age_years,
                required_issuing_state: policy.required_issuing_state,
                required_document_number: policy.required_document_number,
                finalized_time_seconds: request.contract_state.finalized_head_time_seconds,
            },
        ))
        .map_err(map_protected_presentation_error)?;
        self.composer.compose_claim(request, context, presentation)
    }
}

const fn map_protected_presentation_error(
    error: ProtectedDigitalPassportPresentationError,
) -> PassportVaultCallPortError {
    match error {
        ProtectedDigitalPassportPresentationError::ProtectionLocked => {
            PassportVaultCallPortError::ProtectionLocked
        }
        ProtectedDigitalPassportPresentationError::Unavailable => {
            PassportVaultCallPortError::Unavailable
        }
        ProtectedDigitalPassportPresentationError::InvalidRequest
        | ProtectedDigitalPassportPresentationError::NotFound
        | ProtectedDigitalPassportPresentationError::InvalidCredential
        | ProtectedDigitalPassportPresentationError::IssuerNotTrusted
        | ProtectedDigitalPassportPresentationError::Expired
        | ProtectedDigitalPassportPresentationError::PolicyNotSatisfied
        | ProtectedDigitalPassportPresentationError::HolderNotManaged
        | ProtectedDigitalPassportPresentationError::Rejected => {
            PassportVaultCallPortError::InvalidData
        }
    }
}

#[derive(Clone)]
struct PendingProtectedClaim {
    request: PreparePassportVaultCallRequest,
    context: PassportVaultCallCompositionContext,
    policy: PassportVaultProtectedClaimContext,
}

struct RetainedNativeCall {
    planning_fingerprint: [u8; 32],
    network_id: String,
    preview: PassportVaultCallPreview,
    submission_status: PassportVaultCallSubmissionStatus,
    inclusion: Option<PassportVaultCallInclusion>,
    unproven_transaction: Zeroizing<Vec<u8>>,
    pending_protected_claim: Option<PendingProtectedClaim>,
    authorization_in_progress: bool,
}

pub struct NativePassportVaultContractCall {
    contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
    funding: Arc<dyn PassportVaultCallFundingPort>,
    completion: Arc<dyn PassportVaultCallCompletionPort>,
    composer: Arc<dyn PassportVaultCallComposer>,
    protected_claim_composer: Arc<dyn PassportVaultProtectedClaimComposer>,
    calls: Arc<Mutex<BTreeMap<CallKey, RetainedNativeCall>>>,
}

impl NativePassportVaultContractCall {
    pub fn new(
        executable: impl AsRef<Path>,
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
    ) -> Result<Self, PassportVaultCallComposerConfigError> {
        Ok(Self {
            contexts,
            funding: Arc::new(UnavailablePassportVaultCallFunding),
            completion: Arc::new(UnavailablePassportVaultCallCompletion),
            composer: Arc::new(ProcessPassportVaultCallComposer::new(executable)?),
            protected_claim_composer: Arc::new(UnavailablePassportVaultProtectedClaimComposer),
            calls: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn new_with_funding(
        executable: impl AsRef<Path>,
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
        funding: Arc<dyn PassportVaultCallFundingPort>,
    ) -> Result<Self, PassportVaultCallComposerConfigError> {
        Ok(Self {
            contexts,
            funding,
            completion: Arc::new(UnavailablePassportVaultCallCompletion),
            composer: Arc::new(ProcessPassportVaultCallComposer::new(executable)?),
            protected_claim_composer: Arc::new(UnavailablePassportVaultProtectedClaimComposer),
            calls: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn new_with_funding_and_completion(
        executable: impl AsRef<Path>,
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
        funding: Arc<dyn PassportVaultCallFundingPort>,
        completion: Arc<dyn PassportVaultCallCompletionPort>,
    ) -> Result<Self, PassportVaultCallComposerConfigError> {
        Ok(Self {
            contexts,
            funding,
            completion,
            composer: Arc::new(ProcessPassportVaultCallComposer::new(executable)?),
            protected_claim_composer: Arc::new(UnavailablePassportVaultProtectedClaimComposer),
            calls: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn new_with_protected_claims_and_completion(
        executable: impl AsRef<Path>,
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
        funding: Arc<dyn PassportVaultCallFundingPort>,
        completion: Arc<dyn PassportVaultCallCompletionPort>,
        presentations: Arc<ProtectedDigitalPassportPresentationSource>,
    ) -> Result<Self, PassportVaultCallComposerConfigError> {
        let composer = ProcessPassportVaultCallComposer::new(executable.as_ref())?;
        let protected_composer = ProcessPassportVaultCallComposer::new(executable)?;
        Ok(Self {
            contexts,
            funding,
            completion,
            composer: Arc::new(composer),
            protected_claim_composer: Arc::new(ManagedPassportVaultProtectedClaimComposer {
                presentations,
                composer: protected_composer,
            }),
            calls: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    #[cfg(test)]
    fn with_composer(
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
        composer: Arc<dyn PassportVaultCallComposer>,
    ) -> Self {
        Self::with_composer_and_funding(contexts, composer, Arc::new(PassthroughTestFunding))
    }

    #[cfg(test)]
    fn with_composer_and_funding(
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
        composer: Arc<dyn PassportVaultCallComposer>,
        funding: Arc<dyn PassportVaultCallFundingPort>,
    ) -> Self {
        Self {
            contexts,
            funding,
            completion: Arc::new(UnavailablePassportVaultCallCompletion),
            composer,
            protected_claim_composer: Arc::new(UnavailablePassportVaultProtectedClaimComposer),
            calls: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    #[cfg(test)]
    fn with_composer_funding_and_completion(
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
        composer: Arc<dyn PassportVaultCallComposer>,
        funding: Arc<dyn PassportVaultCallFundingPort>,
        completion: Arc<dyn PassportVaultCallCompletionPort>,
    ) -> Self {
        Self {
            contexts,
            funding,
            completion,
            composer,
            protected_claim_composer: Arc::new(UnavailablePassportVaultProtectedClaimComposer),
            calls: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    #[cfg(test)]
    fn with_composers_and_funding(
        contexts: Arc<dyn PassportVaultCallCompositionContextSource>,
        composer: Arc<dyn PassportVaultCallComposer>,
        protected_claim_composer: Arc<dyn PassportVaultProtectedClaimComposer>,
        funding: Arc<dyn PassportVaultCallFundingPort>,
    ) -> Self {
        Self {
            contexts,
            funding,
            completion: Arc::new(UnavailablePassportVaultCallCompletion),
            composer,
            protected_claim_composer,
            calls: Arc::new(Mutex::new(BTreeMap::new())),
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
            || request.contract_state.finalized_head_time_seconds == 0
        {
            return Err(PassportVaultCallPortError::InvalidChainState);
        }
        let claim_policy = match &request.operation {
            PassportVaultCallOperation::ClaimFromLock {
                lock_id, amount, ..
            } => Some(
                decode_protected_claim_context(
                    &request.contract_state.serialized_contract_state,
                    *lock_id,
                    *amount,
                )
                .map_err(|_| PassportVaultCallPortError::InvalidChainState)?,
            ),
            _ => None,
        };
        let context = self
            .contexts
            .context(request.profile_id.as_str(), &request.contract_state)?;
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

        let unproven_transaction = if claim_policy.is_some() {
            Zeroizing::new(Vec::new())
        } else {
            let transaction = self.composer.compose(&request, &context)?;
            validate_unproven_transaction(&transaction, &context.network_id)?;
            transaction
        };
        let draft_id = PassportVaultCallDraftId::parse(hex::encode(planning_fingerprint))
            .map_err(|_| PassportVaultCallPortError::InvalidData)?;
        let authorization_challenge = if claim_policy.is_some() {
            claim_authorization_challenge(
                &draft_id,
                &request.contract_state.action_block_hash_hex,
                planning_fingerprint,
            )?
        } else {
            authorization_challenge(
                &draft_id,
                &request.contract_state.action_block_hash_hex,
                &unproven_transaction,
            )?
        };
        let pending_protected_claim = claim_policy.map(|policy| PendingProtectedClaim {
            request: request.clone(),
            context: context.clone(),
            policy,
        });
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
                network_id: context.network_id,
                preview: preview.clone(),
                submission_status: empty_status(draft_id),
                inclusion: None,
                unproven_transaction,
                pending_protected_claim,
                authorization_in_progress: false,
            },
        );
        Ok(preview)
    }

    fn authorize(
        &self,
        profile_id: &OpaqueId,
        request: AuthorizePassportVaultCallRequest,
    ) -> Result<PassportVaultCallPreview, PassportVaultCallPortError> {
        let key = (profile_id.clone(), request.draft_id.clone());
        let claim_work = {
            let mut calls = self
                .calls
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            let retained = calls
                .get_mut(&key)
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
                    if let Some(pending) = retained.pending_protected_claim.clone() {
                        if retained.authorization_in_progress {
                            return Err(PassportVaultCallPortError::DraftConflict);
                        }
                        retained.authorization_in_progress = true;
                        Some((
                            pending,
                            retained.planning_fingerprint,
                            retained.preview.expires_at,
                        ))
                    } else {
                        let requires_night_funding = matches!(
                            &retained.preview.operation,
                            PassportVaultCallOperation::CreateLock { .. }
                                | PassportVaultCallOperation::DepositToLock { .. }
                        );
                        let funded = self.funding.fund(PassportVaultCallFundingRequest {
                            profile_id: profile_id.as_str().to_owned(),
                            network_id: retained.network_id.clone(),
                            expires_at_seconds: retained.preview.expires_at.value() / 1_000,
                            requires_night_funding,
                            transaction: Zeroizing::new(retained.unproven_transaction.to_vec()),
                        })?;
                        let transaction = funded.into_transaction();
                        validate_funded_transaction(&transaction, &retained.network_id)?;
                        retained.unproven_transaction = transaction;
                        retained.preview.state = PassportVaultCallDraftState::Authorized;
                        return Ok(retained.preview.clone());
                    }
                }
                PassportVaultCallDraftState::Authorized => return Ok(retained.preview.clone()),
                PassportVaultCallDraftState::Submitting
                | PassportVaultCallDraftState::Submitted
                | PassportVaultCallDraftState::Expired => {
                    return Err(PassportVaultCallPortError::DraftConflict);
                }
            }
        };

        let (pending, expected_fingerprint, expires_at) =
            claim_work.ok_or(PassportVaultCallPortError::InvalidData)?;
        let transaction = (|| {
            let transaction = self.protected_claim_composer.compose_after_authorization(
                &pending.request,
                &pending.context,
                &pending.policy,
            )?;
            validate_unproven_transaction(&transaction, &pending.context.network_id)?;
            let funded = self.funding.fund(PassportVaultCallFundingRequest {
                profile_id: profile_id.as_str().to_owned(),
                network_id: pending.context.network_id.clone(),
                expires_at_seconds: expires_at.value() / 1_000,
                requires_night_funding: false,
                transaction,
            })?;
            let transaction = funded.into_transaction();
            validate_funded_transaction(&transaction, &pending.context.network_id)?;
            Ok(transaction)
        })();
        let transaction = match transaction {
            Ok(transaction) => transaction,
            Err(error) => {
                if let Ok(mut calls) = self.calls.lock()
                    && let Some(retained) = calls.get_mut(&key)
                    && retained.planning_fingerprint == expected_fingerprint
                    && retained.preview.state == PassportVaultCallDraftState::Prepared
                {
                    retained.authorization_in_progress = false;
                }
                return Err(error);
            }
        };
        let mut calls = self
            .calls
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?;
        let retained = calls
            .get_mut(&key)
            .ok_or(PassportVaultCallPortError::DraftNotFound)?;
        expire_if_needed(retained, request.now);
        if retained.preview.state == PassportVaultCallDraftState::Expired {
            return Err(PassportVaultCallPortError::DraftExpired);
        }
        if retained.planning_fingerprint != expected_fingerprint
            || retained.preview.authorization_challenge != request.authorization_challenge
            || retained.preview.state != PassportVaultCallDraftState::Prepared
            || !retained.authorization_in_progress
        {
            return Err(PassportVaultCallPortError::DraftConflict);
        }
        retained.unproven_transaction = transaction;
        retained.pending_protected_claim = None;
        retained.authorization_in_progress = false;
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
            let completion_request = {
                let mut calls = self
                    .calls
                    .lock()
                    .map_err(|_| PassportVaultCallPortError::Unavailable)?;
                let retained = calls
                    .get_mut(&key)
                    .ok_or(PassportVaultCallPortError::DraftNotFound)?;
                expire_if_needed(retained, request.now);
                match retained.preview.state {
                    PassportVaultCallDraftState::Submitted => {
                        return Ok(SubmittedPassportVaultCall {
                            preview: retained.preview.clone(),
                            inclusion: retained
                                .inclusion
                                .clone()
                                .ok_or(PassportVaultCallPortError::InvalidData)?,
                        });
                    }
                    PassportVaultCallDraftState::Expired => {
                        return Err(PassportVaultCallPortError::DraftExpired);
                    }
                    PassportVaultCallDraftState::Submitting => {
                        return Err(PassportVaultCallPortError::SubmissionInProgress);
                    }
                    PassportVaultCallDraftState::Authorized
                        if !retained.unproven_transaction.is_empty() => {}
                    _ => return Err(PassportVaultCallPortError::DraftConflict),
                }
                retained.preview.state = PassportVaultCallDraftState::Submitting;
                retained.submission_status.state = PassportVaultCallSubmissionState::Running;
                PassportVaultCallCompletionRequest {
                    profile_id: profile_id.as_str().to_owned(),
                    network_id: retained.network_id.clone(),
                    draft_id: request.draft_id.as_str().to_owned(),
                    planning_fingerprint: retained.planning_fingerprint,
                    expires_at: retained.preview.expires_at,
                    updated_at: request.now,
                    transaction: Zeroizing::new(retained.unproven_transaction.to_vec()),
                }
            };

            let completion = Arc::clone(&self.completion);
            let worker_completion = Arc::clone(&completion);
            let calls = Arc::clone(&self.calls);
            let worker_key = key.clone();
            let worker_profile = profile_id.as_str().to_owned();
            let worker_draft = request.draft_id.as_str().to_owned();
            let (sender, receiver) = futures::channel::oneshot::channel();
            let spawn = thread::Builder::new()
                .name("oxid-passport-vault-submit".to_owned())
                .spawn(move || {
                    let result = worker_completion.complete(completion_request);
                    let result = finish_native_submission(
                        calls.as_ref(),
                        &worker_key,
                        &worker_profile,
                        &worker_draft,
                        worker_completion.as_ref(),
                        result,
                    );
                    let _ = sender.send(result);
                });
            if spawn.is_err() {
                restore_native_authorized(self.calls.as_ref(), &key)?;
                return Err(PassportVaultCallPortError::Unavailable);
            }
            let mut cancel_on_drop = CancelNativeSubmissionOnDrop::new(
                completion,
                profile_id.as_str().to_owned(),
                request.draft_id.as_str().to_owned(),
            );
            match receiver.await {
                Ok(result) => {
                    cancel_on_drop.disarm();
                    result
                }
                Err(_) => Err(PassportVaultCallPortError::SubmissionOutcomeUnknown),
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
        match self
            .completion
            .status(profile_id.as_str(), draft_id.as_str())
        {
            Ok(status) => {
                update_native_from_status(self.calls.as_ref(), profile_id, draft_id, &status)?;
                Ok(status)
            }
            Err(PassportVaultCallPortError::DraftNotFound) => self
                .calls
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?
                .get(&(profile_id.clone(), draft_id.clone()))
                .map(|retained| retained.submission_status.clone())
                .ok_or(PassportVaultCallPortError::DraftNotFound),
            Err(error) => Err(error),
        }
    }

    fn cancel_submission(
        &self,
        profile_id: &OpaqueId,
        draft_id: &PassportVaultCallDraftId,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        let status = self
            .completion
            .cancel(profile_id.as_str(), draft_id.as_str())?;
        update_native_from_status(self.calls.as_ref(), profile_id, draft_id, &status)?;
        Ok(status)
    }

    fn submission_history(
        &self,
        profile_id: &OpaqueId,
    ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError> {
        let mut statuses = self.completion.history(profile_id.as_str())?;
        let calls = self
            .calls
            .lock()
            .map_err(|_| PassportVaultCallPortError::Unavailable)?;
        for ((stored_profile_id, draft_id), retained) in calls.iter() {
            if stored_profile_id == profile_id
                && !statuses.iter().any(|status| &status.draft_id == draft_id)
            {
                if statuses.len() == MAX_PASSPORT_VAULT_CALL_SUBMISSION_HISTORY {
                    statuses.pop();
                }
                statuses.insert(0, retained.submission_status.clone());
            }
        }
        Ok(statuses)
    }

    fn reconcile_submission<'a>(
        &'a self,
        profile_id: &'a OpaqueId,
        draft_id: &'a PassportVaultCallDraftId,
    ) -> PassportVaultCallStatusFuture<'a> {
        Box::pin(async move {
            let completion = Arc::clone(&self.completion);
            let profile = profile_id.as_str().to_owned();
            let draft = draft_id.as_str().to_owned();
            let (sender, receiver) = futures::channel::oneshot::channel();
            thread::Builder::new()
                .name("oxid-passport-vault-reconcile".to_owned())
                .spawn(move || {
                    let _ = sender.send(completion.reconcile(&profile, &draft));
                })
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            let status = receiver
                .await
                .unwrap_or(Err(PassportVaultCallPortError::Unavailable))?;
            update_native_from_status(self.calls.as_ref(), profile_id, draft_id, &status)?;
            Ok(status)
        })
    }
}

struct CancelNativeSubmissionOnDrop {
    completion: Arc<dyn PassportVaultCallCompletionPort>,
    profile_id: String,
    draft_id: String,
    armed: bool,
}

impl CancelNativeSubmissionOnDrop {
    fn new(
        completion: Arc<dyn PassportVaultCallCompletionPort>,
        profile_id: String,
        draft_id: String,
    ) -> Self {
        Self {
            completion,
            profile_id,
            draft_id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CancelNativeSubmissionOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.completion.cancel(&self.profile_id, &self.draft_id);
        }
    }
}

fn restore_native_authorized(
    calls: &Mutex<BTreeMap<CallKey, RetainedNativeCall>>,
    key: &CallKey,
) -> Result<(), PassportVaultCallPortError> {
    let mut calls = calls
        .lock()
        .map_err(|_| PassportVaultCallPortError::Unavailable)?;
    let retained = calls
        .get_mut(key)
        .ok_or(PassportVaultCallPortError::DraftNotFound)?;
    retained.preview.state = PassportVaultCallDraftState::Authorized;
    retained.submission_status = empty_status(retained.preview.draft_id.clone());
    Ok(())
}

fn finish_native_submission(
    calls: &Mutex<BTreeMap<CallKey, RetainedNativeCall>>,
    key: &CallKey,
    profile_id: &str,
    draft_id: &str,
    completion: &dyn PassportVaultCallCompletionPort,
    result: Result<PassportVaultCallInclusion, PassportVaultCallPortError>,
) -> Result<SubmittedPassportVaultCall, PassportVaultCallPortError> {
    match result {
        Ok(inclusion) => {
            let mut retained_calls = calls
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            let retained = retained_calls
                .get_mut(key)
                .ok_or(PassportVaultCallPortError::DraftNotFound)?;
            retained.preview.state = PassportVaultCallDraftState::Submitted;
            retained.preview.fee_atomic_units = Some(inclusion.fee_atomic_units);
            retained.submission_status = status_from_inclusion(
                retained.preview.draft_id.clone(),
                PassportVaultCallSubmissionState::Included,
                &inclusion,
            );
            retained.inclusion = Some(inclusion.clone());
            retained.unproven_transaction.zeroize();
            Ok(SubmittedPassportVaultCall {
                preview: retained.preview.clone(),
                inclusion,
            })
        }
        Err(PassportVaultCallPortError::SubmissionCancelled) => {
            let mut retained_calls = calls
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            let retained = retained_calls
                .get_mut(key)
                .ok_or(PassportVaultCallPortError::DraftNotFound)?;
            retained.preview.state = PassportVaultCallDraftState::Authorized;
            retained.submission_status = empty_status(retained.preview.draft_id.clone());
            retained.submission_status.state = PassportVaultCallSubmissionState::Cancelled;
            Err(PassportVaultCallPortError::SubmissionCancelled)
        }
        Err(PassportVaultCallPortError::DraftExpired) => {
            let mut retained_calls = calls
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            let retained = retained_calls
                .get_mut(key)
                .ok_or(PassportVaultCallPortError::DraftNotFound)?;
            retained.preview.state = PassportVaultCallDraftState::Expired;
            retained.submission_status = empty_status(retained.preview.draft_id.clone());
            retained.submission_status.state = PassportVaultCallSubmissionState::Expired;
            retained.unproven_transaction.zeroize();
            Err(PassportVaultCallPortError::DraftExpired)
        }
        Err(PassportVaultCallPortError::SubmissionRejected) => {
            calls
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?
                .remove(key);
            Err(PassportVaultCallPortError::SubmissionRejected)
        }
        Err(PassportVaultCallPortError::SubmissionOutcomeUnknown) => {
            let status = completion.status(profile_id, draft_id).unwrap_or(
                PassportVaultCallSubmissionStatus {
                    draft_id: key.1.clone(),
                    state: PassportVaultCallSubmissionState::OutcomeUnknown,
                    transaction_hash_hex: None,
                    block_hash_hex: None,
                    block_height: None,
                    fee_atomic_units: None,
                    mode: None,
                },
            );
            let mut retained_calls = calls
                .lock()
                .map_err(|_| PassportVaultCallPortError::Unavailable)?;
            if let Some(retained) = retained_calls.get_mut(key) {
                retained.submission_status = status;
                retained.unproven_transaction.zeroize();
            }
            Err(PassportVaultCallPortError::SubmissionOutcomeUnknown)
        }
        Err(error) => {
            restore_native_authorized(calls, key)?;
            Err(error)
        }
    }
}

fn status_from_inclusion(
    draft_id: PassportVaultCallDraftId,
    state: PassportVaultCallSubmissionState,
    inclusion: &PassportVaultCallInclusion,
) -> PassportVaultCallSubmissionStatus {
    PassportVaultCallSubmissionStatus {
        draft_id,
        state,
        transaction_hash_hex: Some(inclusion.transaction_hash_hex.clone()),
        block_hash_hex: Some(inclusion.block_hash_hex.clone()),
        block_height: Some(inclusion.block_height),
        fee_atomic_units: Some(inclusion.fee_atomic_units),
        mode: Some(inclusion.mode.clone()),
    }
}

fn update_native_from_status(
    calls: &Mutex<BTreeMap<CallKey, RetainedNativeCall>>,
    profile_id: &OpaqueId,
    draft_id: &PassportVaultCallDraftId,
    status: &PassportVaultCallSubmissionStatus,
) -> Result<(), PassportVaultCallPortError> {
    let mut calls = calls
        .lock()
        .map_err(|_| PassportVaultCallPortError::Unavailable)?;
    let Some(retained) = calls.get_mut(&(profile_id.clone(), draft_id.clone())) else {
        return Ok(());
    };
    retained.submission_status = status.clone();
    match status.state {
        PassportVaultCallSubmissionState::Included => {
            let inclusion = PassportVaultCallInclusion {
                transaction_hash_hex: status
                    .transaction_hash_hex
                    .clone()
                    .ok_or(PassportVaultCallPortError::InvalidData)?,
                block_hash_hex: status
                    .block_hash_hex
                    .clone()
                    .ok_or(PassportVaultCallPortError::InvalidData)?,
                block_height: status
                    .block_height
                    .ok_or(PassportVaultCallPortError::InvalidData)?,
                fee_atomic_units: status
                    .fee_atomic_units
                    .ok_or(PassportVaultCallPortError::InvalidData)?,
                mode: status
                    .mode
                    .clone()
                    .ok_or(PassportVaultCallPortError::InvalidData)?,
            };
            retained.preview.state = PassportVaultCallDraftState::Submitted;
            retained.preview.fee_atomic_units = Some(inclusion.fee_atomic_units);
            retained.inclusion = Some(inclusion);
            retained.unproven_transaction.zeroize();
        }
        PassportVaultCallSubmissionState::Broadcasting
        | PassportVaultCallSubmissionState::OutcomeUnknown
        | PassportVaultCallSubmissionState::Rejected
        | PassportVaultCallSubmissionState::Expired => {
            retained.unproven_transaction.zeroize();
        }
        PassportVaultCallSubmissionState::NotStarted
        | PassportVaultCallSubmissionState::Running
        | PassportVaultCallSubmissionState::CancellationRequested
        | PassportVaultCallSubmissionState::Cancelled => {}
    }
    Ok(())
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

fn validate_funded_transaction(
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
    let intent_count = standard.intents.iter().count();
    if standard.network_id != network_id || !(1..=2).contains(&intent_count) {
        return Err(PassportVaultCallPortError::InvalidData);
    }
    Ok(())
}

fn planning_fingerprint(
    request: &PreparePassportVaultCallRequest,
    context: &PassportVaultCallCompositionContext,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:native-passport-vault-plan:v2\0");
    update_length_prefixed(&mut digest, request.profile_id.as_str().as_bytes());
    digest.update(request.contract_state.contract_address_hex.as_bytes());
    digest.update(request.contract_state.transaction_hash_hex.as_bytes());
    digest.update(request.contract_state.action_block_hash_hex.as_bytes());
    digest.update(request.contract_state.action_block_height.to_be_bytes());
    digest.update(request.contract_state.finalized_head_hash_hex.as_bytes());
    digest.update(request.contract_state.finalized_head_height.to_be_bytes());
    digest.update(
        request
            .contract_state
            .finalized_head_time_seconds
            .to_be_bytes(),
    );
    update_length_prefixed(
        &mut digest,
        request.contract_state.serialized_contract_state.as_slice(),
    );
    digest.update(request.expires_at.value().to_be_bytes());
    update_operation_digest(&mut digest, &request.operation);
    update_length_prefixed(&mut digest, context.network_id.as_bytes());
    update_length_prefixed(&mut digest, context.zswap_chain_state.as_slice());
    update_length_prefixed(&mut digest, context.ledger_parameters.as_slice());
    digest.update(context.coin_public_key);
    digest.update(context.encryption_public_key);
    digest.update(context.unshielded_recipient);
    digest.finalize().into()
}

fn claim_authorization_challenge(
    draft_id: &PassportVaultCallDraftId,
    anchor_block_hash_hex: &str,
    planning_fingerprint: [u8; 32],
) -> Result<PassportVaultCallAuthorizationChallenge, PassportVaultCallPortError> {
    let mut digest = Sha256::new();
    digest.update(b"oxid:native-passport-vault-claim-authorization:v1\0");
    digest.update(draft_id.as_str().as_bytes());
    digest.update(anchor_block_hash_hex.as_bytes());
    digest.update(planning_fingerprint);
    PassportVaultCallAuthorizationChallenge::parse(hex::encode(digest.finalize()))
        .map_err(|_| PassportVaultCallPortError::InvalidData)
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
            update_length_prefixed(digest, credential_id.as_str().as_bytes());
        }
        PassportVaultCallOperation::WithdrawFromLock { lock_id, amount } => {
            digest.update([3]);
            digest.update(lock_id.to_be_bytes());
            digest.update(amount.to_be_bytes());
        }
    }
}

fn update_length_prefixed(digest: &mut Sha256, value: &[u8]) {
    digest.update(value.len().to_be_bytes());
    digest.update(value);
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
        retained.pending_protected_claim = None;
        retained.authorization_in_progress = false;
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
    use midnight_base_crypto::{fab::AlignedValue, schnorr::Signature, time::Timestamp};
    use midnight_ledger::structure::{Intent, ProofPreimageMarker, StandardTransaction};
    use midnight_onchain_runtime::state::{ChargedState, ContractState, StateValue};
    use midnight_serialize::tagged_serialize;
    use midnight_storage::{
        DefaultDB,
        arena::Sp,
        storage::{Array, HashMap as LedgerHashMap},
    };
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
            _: &str,
            _: &oxid_passport_vault_application::PassportVaultContractStateSnapshot,
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

    struct ProtectedClaimComposer {
        calls: AtomicUsize,
        fail_first: bool,
    }

    impl PassportVaultProtectedClaimComposer for ProtectedClaimComposer {
        fn compose_after_authorization(
            &self,
            _: &PreparePassportVaultCallRequest,
            context: &PassportVaultCallCompositionContext,
            _: &PassportVaultProtectedClaimContext,
        ) -> Result<Zeroizing<Vec<u8>>, PassportVaultCallPortError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_first && call == 0 {
                return Err(PassportVaultCallPortError::Unavailable);
            }
            Composer {
                calls: AtomicUsize::new(0),
            }
            .compose(&request(create_operation()), context)
        }
    }

    struct RecordingFunding {
        calls: AtomicUsize,
        requires_night_funding: Mutex<Vec<bool>>,
        failure: Option<PassportVaultCallPortError>,
    }

    impl PassportVaultCallFundingPort for RecordingFunding {
        fn fund(
            &self,
            request: PassportVaultCallFundingRequest,
        ) -> Result<FundedPassportVaultCall, PassportVaultCallPortError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let (_, _, _, requires_night_funding, transaction) = request.into_parts();
            self.requires_night_funding
                .lock()
                .expect("funding observations")
                .push(requires_night_funding);
            if let Some(error) = self.failure {
                return Err(error);
            }
            Ok(FundedPassportVaultCall::new(transaction, 10, 1))
        }
    }

    #[derive(Default)]
    struct IncludedCompletion {
        calls: AtomicUsize,
    }

    impl PassportVaultCallCompletionPort for IncludedCompletion {
        fn complete(
            &self,
            request: PassportVaultCallCompletionRequest,
        ) -> Result<PassportVaultCallInclusion, PassportVaultCallPortError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let (profile, network, draft, fingerprint, expires_at, updated_at, transaction) =
                request.into_parts();
            assert_eq!(profile, "profile_native_vault");
            assert_eq!(network, "undeployed");
            assert_eq!(draft, hex::encode(fingerprint));
            assert!(expires_at.value() > updated_at.value());
            assert!(!transaction.is_empty());
            Ok(PassportVaultCallInclusion {
                transaction_hash_hex: "55".repeat(32),
                block_hash_hex: "66".repeat(32),
                block_height: 46,
                fee_atomic_units: 17,
                mode: "live".to_owned(),
            })
        }

        fn status(
            &self,
            _: &str,
            _: &str,
        ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
            Err(PassportVaultCallPortError::DraftNotFound)
        }

        fn cancel(
            &self,
            _: &str,
            _: &str,
        ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
            Err(PassportVaultCallPortError::SubmissionNotInProgress)
        }

        fn history(
            &self,
            _: &str,
        ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError> {
            Ok(Vec::new())
        }

        fn reconcile(
            &self,
            _: &str,
            _: &str,
        ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
            Err(PassportVaultCallPortError::DraftNotFound)
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
                finalized_head_time_seconds: 1_700_000_000,
            },
            operation,
            expires_at: UnixTimestampMillis::new(10_000),
        }
    }

    fn claim_ready_request() -> PreparePassportVaultCallRequest {
        const FIXTURE: &str =
            include_str!("../../../../fixtures/passport-vault/contract-state-v1.hex");
        let mut cursor = Cursor::new(hex::decode(FIXTURE.trim()).expect("fixture bytes"));
        let mut contract: ContractState<DefaultDB> =
            tagged_deserialize(&mut cursor).expect("fixture state");
        let StateValue::Array(fields) = contract.data.get_ref() else {
            panic!("fixture ledger fields");
        };
        let mut fields: Vec<StateValue<DefaultDB>> = fields.iter_deref().cloned().collect();
        let locks = match &fields[4] {
            StateValue::Map(locks) => locks.clone(),
            _ => panic!("fixture locks"),
        };
        let record = (
            [9_u8; 32], 18_u8, false, [0_u8; 32], false, [0_u8; 32], 40_u128, [5_u8; 32], 100_u128,
            0_u128,
        );
        fields[4] = StateValue::Map(locks.insert(
            AlignedValue::from(0_u64),
            StateValue::Cell(Sp::new(AlignedValue::from(record))),
        ));
        fields[7] = StateValue::Cell(Sp::new(AlignedValue::from(100_u128)));
        contract.data = ChargedState::new(StateValue::Array(Array::new_from_slice(&fields)));
        let mut serialized_contract_state = Vec::new();
        tagged_serialize(&contract, &mut serialized_contract_state).expect("claim-ready state");
        let mut request = request(PassportVaultCallOperation::ClaimFromLock {
            lock_id: 0,
            amount: 1,
            credential_id: OpaqueId::parse("credential_1").expect("credential"),
        });
        request.contract_state.serialized_contract_state = serialized_contract_state;
        request
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

    fn adapter_with_funding(
        failure: Option<PassportVaultCallPortError>,
    ) -> (NativePassportVaultContractCall, Arc<RecordingFunding>) {
        let contexts = Arc::new(ContextSource {
            calls: AtomicUsize::new(0),
        });
        let composer = Arc::new(Composer {
            calls: AtomicUsize::new(0),
        });
        let funding = Arc::new(RecordingFunding {
            calls: AtomicUsize::new(0),
            requires_night_funding: Mutex::new(Vec::new()),
            failure,
        });
        (
            NativePassportVaultContractCall::with_composer_and_funding(
                contexts,
                composer,
                funding.clone(),
            ),
            funding,
        )
    }

    fn adapter_with_completion() -> (NativePassportVaultContractCall, Arc<IncludedCompletion>) {
        let contexts = Arc::new(ContextSource {
            calls: AtomicUsize::new(0),
        });
        let composer = Arc::new(Composer {
            calls: AtomicUsize::new(0),
        });
        let funding = Arc::new(RecordingFunding {
            calls: AtomicUsize::new(0),
            requires_night_funding: Mutex::new(Vec::new()),
            failure: None,
        });
        let completion = Arc::new(IncludedCompletion::default());
        (
            NativePassportVaultContractCall::with_composer_funding_and_completion(
                contexts,
                composer,
                funding,
                completion.clone(),
            ),
            completion,
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
    fn native_completion_returns_only_public_inclusion_and_erases_the_retained_transaction() {
        let (adapter, completion) = adapter_with_completion();
        let prepared = adapter
            .prepare(request(create_operation()))
            .expect("prepare");
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
        let submitted = block_on(adapter.submit(
            &profile(),
            SubmitPassportVaultCallRequest {
                draft_id: authorized.draft_id.clone(),
                now: UnixTimestampMillis::new(2_000),
            },
        ))
        .expect("completion succeeds");

        assert_eq!(completion.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            submitted.preview.state,
            PassportVaultCallDraftState::Submitted
        );
        assert_eq!(submitted.inclusion.transaction_hash_hex, "55".repeat(32));
        assert_eq!(submitted.inclusion.block_height, 46);
        assert_eq!(submitted.inclusion.fee_atomic_units, 17);
        let status = adapter
            .submission_status(&profile(), &authorized.draft_id)
            .expect("included status remains available");
        assert_eq!(status.state, PassportVaultCallSubmissionState::Included);
        let retained = adapter.calls.lock().expect("retained calls");
        assert!(
            retained
                .get(&(profile(), authorized.draft_id))
                .expect("retained public result")
                .unproven_transaction
                .is_empty()
        );
    }

    #[test]
    fn funding_runs_only_after_exact_authorization_and_failure_keeps_prepared_draft() {
        let (adapter, funding) =
            adapter_with_funding(Some(PassportVaultCallPortError::InsufficientFunds));
        let prepared = adapter
            .prepare(request(create_operation()))
            .expect("prepare");
        let wrong_challenge =
            PassportVaultCallAuthorizationChallenge::parse("00".repeat(32)).expect("challenge");
        assert_eq!(
            adapter.authorize(
                &profile(),
                AuthorizePassportVaultCallRequest {
                    draft_id: prepared.draft_id.clone(),
                    authorization_challenge: wrong_challenge,
                    now: UnixTimestampMillis::new(1_000),
                }
            ),
            Err(PassportVaultCallPortError::AuthorizationChallengeMismatch)
        );
        assert_eq!(funding.calls.load(Ordering::SeqCst), 0);

        assert_eq!(
            adapter.authorize(
                &profile(),
                AuthorizePassportVaultCallRequest {
                    draft_id: prepared.draft_id.clone(),
                    authorization_challenge: prepared.authorization_challenge,
                    now: UnixTimestampMillis::new(1_001),
                }
            ),
            Err(PassportVaultCallPortError::InsufficientFunds)
        );
        assert_eq!(funding.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            funding
                .requires_night_funding
                .lock()
                .expect("funding observations")
                .as_slice(),
            &[true]
        );
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    &prepared.draft_id,
                    UnixTimestampMillis::new(1_002)
                )
                .expect("retained draft")
                .state,
            PassportVaultCallDraftState::Prepared
        );
    }

    #[test]
    fn authorization_types_night_funding_by_operation() {
        for (operation, expected) in [
            (create_operation(), true),
            (
                PassportVaultCallOperation::DepositToLock {
                    lock_id: 2,
                    amount: 5,
                },
                true,
            ),
            (
                PassportVaultCallOperation::WithdrawFromLock {
                    lock_id: 2,
                    amount: 1,
                },
                false,
            ),
        ] {
            let (adapter, funding) = adapter_with_funding(None);
            let prepared = adapter.prepare(request(operation)).expect("prepare");
            let authorized = adapter
                .authorize(
                    &profile(),
                    AuthorizePassportVaultCallRequest {
                        draft_id: prepared.draft_id,
                        authorization_challenge: prepared.authorization_challenge,
                        now: UnixTimestampMillis::new(1_000),
                    },
                )
                .expect("authorize");
            assert_eq!(authorized.state, PassportVaultCallDraftState::Authorized);
            assert_eq!(
                funding
                    .requires_night_funding
                    .lock()
                    .expect("funding observations")
                    .as_slice(),
                &[expected]
            );
        }
    }

    #[test]
    fn protected_funding_boundaries_redact_serialized_transactions() {
        let request = PassportVaultCallFundingRequest {
            profile_id: "profile_redaction".to_owned(),
            network_id: "undeployed".to_owned(),
            expires_at_seconds: 7,
            requires_night_funding: true,
            transaction: Zeroizing::new(vec![0xde, 0xad, 0xbe, 0xef]),
        };
        let debug = format!("{request:?}");
        assert!(debug.contains("transaction_bytes: 4"));
        assert!(!debug.contains("deadbeef"));

        let funded =
            FundedPassportVaultCall::new(Zeroizing::new(vec![0xde, 0xad, 0xbe, 0xef]), 9, 1);
        let debug = format!("{funded:?}");
        assert!(debug.contains("transaction_bytes: 4"));
        assert!(!debug.contains("deadbeef"));
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
    fn rejects_invalid_claim_state_and_unauthenticated_state_before_composition() {
        let (adapter, contexts, composer) = adapter();
        let claim = PassportVaultCallOperation::ClaimFromLock {
            lock_id: 1,
            amount: 1,
            credential_id: OpaqueId::parse("credential_1").expect("credential"),
        };
        assert_eq!(
            adapter.prepare(request(claim)),
            Err(PassportVaultCallPortError::InvalidChainState)
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
    fn protected_claim_composition_starts_only_after_exact_authorization() {
        let contexts = Arc::new(ContextSource {
            calls: AtomicUsize::new(0),
        });
        let public_composer = Arc::new(Composer {
            calls: AtomicUsize::new(0),
        });
        let protected_composer = Arc::new(ProtectedClaimComposer {
            calls: AtomicUsize::new(0),
            fail_first: false,
        });
        let funding = Arc::new(RecordingFunding {
            calls: AtomicUsize::new(0),
            requires_night_funding: Mutex::new(Vec::new()),
            failure: None,
        });
        let adapter = NativePassportVaultContractCall::with_composers_and_funding(
            contexts,
            public_composer.clone(),
            protected_composer.clone(),
            funding.clone(),
        );
        let prepared = adapter.prepare(claim_ready_request()).expect("claim plan");
        assert_eq!(prepared.state, PassportVaultCallDraftState::Prepared);
        assert_eq!(public_composer.calls.load(Ordering::SeqCst), 0);
        assert_eq!(protected_composer.calls.load(Ordering::SeqCst), 0);
        assert_eq!(funding.calls.load(Ordering::SeqCst), 0);

        let wrong = PassportVaultCallAuthorizationChallenge::parse("00".repeat(32))
            .expect("wrong challenge");
        assert_eq!(
            adapter.authorize(
                &profile(),
                AuthorizePassportVaultCallRequest {
                    draft_id: prepared.draft_id.clone(),
                    authorization_challenge: wrong,
                    now: UnixTimestampMillis::new(1_000),
                },
            ),
            Err(PassportVaultCallPortError::AuthorizationChallengeMismatch)
        );
        assert_eq!(protected_composer.calls.load(Ordering::SeqCst), 0);
        assert_eq!(funding.calls.load(Ordering::SeqCst), 0);

        let authorized = adapter
            .authorize(
                &profile(),
                AuthorizePassportVaultCallRequest {
                    draft_id: prepared.draft_id,
                    authorization_challenge: prepared.authorization_challenge,
                    now: UnixTimestampMillis::new(1_001),
                },
            )
            .expect("authorized protected claim");
        assert_eq!(authorized.state, PassportVaultCallDraftState::Authorized);
        assert_eq!(protected_composer.calls.load(Ordering::SeqCst), 1);
        assert_eq!(funding.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            funding
                .requires_night_funding
                .lock()
                .expect("funding observation")
                .as_slice(),
            &[false]
        );
    }

    #[test]
    fn protected_claim_composition_failure_leaves_the_plan_retryable() {
        let contexts = Arc::new(ContextSource {
            calls: AtomicUsize::new(0),
        });
        let public_composer = Arc::new(Composer {
            calls: AtomicUsize::new(0),
        });
        let protected_composer = Arc::new(ProtectedClaimComposer {
            calls: AtomicUsize::new(0),
            fail_first: true,
        });
        let funding = Arc::new(RecordingFunding {
            calls: AtomicUsize::new(0),
            requires_night_funding: Mutex::new(Vec::new()),
            failure: None,
        });
        let adapter = NativePassportVaultContractCall::with_composers_and_funding(
            contexts,
            public_composer,
            protected_composer.clone(),
            funding.clone(),
        );
        let prepared = adapter.prepare(claim_ready_request()).expect("claim plan");
        let authorization = AuthorizePassportVaultCallRequest {
            draft_id: prepared.draft_id.clone(),
            authorization_challenge: prepared.authorization_challenge,
            now: UnixTimestampMillis::new(1_000),
        };

        assert_eq!(
            adapter.authorize(&profile(), authorization.clone()),
            Err(PassportVaultCallPortError::Unavailable)
        );
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    &prepared.draft_id,
                    UnixTimestampMillis::new(1_001)
                )
                .expect("retryable claim plan")
                .state,
            PassportVaultCallDraftState::Prepared
        );
        assert_eq!(funding.calls.load(Ordering::SeqCst), 0);

        let authorized = adapter
            .authorize(&profile(), authorization)
            .expect("claim retry authorizes");
        assert_eq!(authorized.state, PassportVaultCallDraftState::Authorized);
        assert_eq!(protected_composer.calls.load(Ordering::SeqCst), 2);
        assert_eq!(funding.calls.load(Ordering::SeqCst), 1);
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
