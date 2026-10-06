// SPDX-License-Identifier: Apache-2.0

//! Bounded generated-Compact call composition for the four holder-DID
//! bootstrap writes. The controller secret is borrowed inside wallet custody,
//! written to one child-process stdin buffer, and zeroized before returning.

use std::{
    fmt, fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use midnight_base_crypto::schnorr::Signature;
use midnight_ledger::structure::{ProofPreimageMarker, Transaction};
use midnight_serialize::tagged_deserialize;
use midnight_storage::DefaultDB;
use midnight_transient_crypto::commitment::PedersenRandomness;
use oxid_identity_application::DidLifecyclePortError;
use oxid_wallet_application::{
    WalletDerivedSecretUsePort, WalletHdPath, WalletHdPathComponent, WalletSecurityPortError,
};
use oxid_wallet_domain::WalletProfileId;
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const BIP44_PURPOSE: u32 = 44;
const MIDNIGHT_COIN_TYPE: u32 = 2_400;
const DID_CONTROLLER_ROLE: u32 = 3;
const MAX_CONTRACT_STATE_BYTES: usize = 16 * 1024 * 1024;
const MAX_ZSWAP_STATE_BYTES: usize = 2 * 1024 * 1024;
const MAX_LEDGER_PARAMETERS_BYTES: usize = 512 * 1024;
const MAX_REQUEST_BYTES: usize = 40 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const COMPOSER_TIMEOUT: Duration = Duration::from_secs(60);

type UnprovenTransaction =
    Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightDidCallComposerConfigError {
    PathNotAbsolute,
    ExecutableUnavailable,
    ExecutableSymlink,
}

impl fmt::Display for MidnightDidCallComposerConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PathNotAbsolute => "Midnight DID composer path must be absolute",
            Self::ExecutableUnavailable => "Midnight DID composer executable is unavailable",
            Self::ExecutableSymlink => "Midnight DID composer executable must not be a symlink",
        })
    }
}

impl std::error::Error for MidnightDidCallComposerConfigError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidnightDidCallContext {
    pub contract_state: Vec<u8>,
    pub contract_address: [u8; 32],
    pub zswap_chain_state: Option<Vec<u8>>,
    pub ledger_parameters: Option<Vec<u8>>,
    pub network_id: String,
    pub timestamp_millis: u64,
    pub coin_public_key: [u8; 32],
    pub encryption_public_key: [u8; 32],
}

impl MidnightDidCallContext {
    fn validate(&self) -> Result<(), DidLifecyclePortError> {
        if self.contract_state.is_empty()
            || self.contract_state.len() > MAX_CONTRACT_STATE_BYTES
            || self.contract_address == [0; 32]
            || self.network_id.is_empty()
            || self.network_id.len() > 64
            || !self
                .network_id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || self.timestamp_millis == 0
            || self.coin_public_key == [0; 32]
            || self.encryption_public_key == [0; 32]
            || self
                .zswap_chain_state
                .as_ref()
                .is_some_and(|state| state.is_empty() || state.len() > MAX_ZSWAP_STATE_BYTES)
            || self.ledger_parameters.as_ref().is_some_and(|parameters| {
                parameters.is_empty() || parameters.len() > MAX_LEDGER_PARAMETERS_BYTES
            })
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MidnightDidCallOperation {
    AddAuthenticationMethod {
        method_id: String,
        x: [u8; 32],
    },
    AddAuthenticationRelationship {
        method_id: String,
    },
    AddAssertionMethod {
        method_id: String,
        x: [u8; 32],
        y: [u8; 32],
    },
    AddAssertionRelationship {
        method_id: String,
    },
}

impl MidnightDidCallOperation {
    fn kind(&self) -> &'static str {
        match self {
            Self::AddAuthenticationMethod { .. } => "add_authentication_method",
            Self::AddAuthenticationRelationship { .. } => "add_authentication_relationship",
            Self::AddAssertionMethod { .. } => "add_assertion_method",
            Self::AddAssertionRelationship { .. } => "add_assertion_relationship",
        }
    }

    fn circuit_id(&self) -> &'static str {
        match self {
            Self::AddAuthenticationMethod { .. } => "setVerificationMethod",
            Self::AddAssertionMethod { .. } => "setSchnorrJubjubVerificationMethod",
            Self::AddAuthenticationRelationship { .. } | Self::AddAssertionRelationship { .. } => {
                "setVerificationMethodRelation"
            }
        }
    }

    fn validate(&self) -> Result<(), DidLifecyclePortError> {
        let method_id = match self {
            Self::AddAuthenticationMethod { method_id, x } => {
                if *x == [0; 32] {
                    return Err(DidLifecyclePortError::InvalidOperation);
                }
                method_id
            }
            Self::AddAuthenticationRelationship { method_id }
            | Self::AddAssertionRelationship { method_id } => method_id,
            Self::AddAssertionMethod { method_id, x, y } => {
                if *x == [0; 32] || *y == [0; 32] {
                    return Err(DidLifecyclePortError::InvalidOperation);
                }
                method_id
            }
        };
        if !method_id.starts_with('#')
            || method_id.len() < 2
            || method_id.len() > 65
            || !method_id[1..].bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
            })
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        Ok(())
    }
}

pub struct NativeMidnightDidCallRequest {
    pub profile_id: WalletProfileId,
    pub account_index: u32,
    pub controller_index: u32,
    pub context: MidnightDidCallContext,
    pub operation: MidnightDidCallOperation,
}

pub struct NativeMidnightDidCallPlan {
    pub planning_fingerprint: [u8; 32],
    pub transaction: Zeroizing<Vec<u8>>,
}

pub struct NativeMidnightDidCallComposer {
    executable: PathBuf,
    custody: Arc<dyn WalletDerivedSecretUsePort>,
}

impl NativeMidnightDidCallComposer {
    pub fn new(
        executable: impl AsRef<Path>,
        custody: Arc<dyn WalletDerivedSecretUsePort>,
    ) -> Result<Self, MidnightDidCallComposerConfigError> {
        let executable = executable.as_ref();
        if !executable.is_absolute() {
            return Err(MidnightDidCallComposerConfigError::PathNotAbsolute);
        }
        let metadata = fs::symlink_metadata(executable)
            .map_err(|_| MidnightDidCallComposerConfigError::ExecutableUnavailable)?;
        if metadata.file_type().is_symlink() {
            return Err(MidnightDidCallComposerConfigError::ExecutableSymlink);
        }
        if !metadata.is_file() {
            return Err(MidnightDidCallComposerConfigError::ExecutableUnavailable);
        }
        let canonical = fs::canonicalize(executable)
            .map_err(|_| MidnightDidCallComposerConfigError::ExecutableUnavailable)?;
        if canonical != executable {
            return Err(MidnightDidCallComposerConfigError::ExecutableSymlink);
        }
        Ok(Self {
            executable: canonical,
            custody,
        })
    }

    pub fn compose(
        &self,
        request: &NativeMidnightDidCallRequest,
    ) -> Result<NativeMidnightDidCallPlan, DidLifecyclePortError> {
        request.context.validate()?;
        request.operation.validate()?;
        if request.account_index > WalletHdPathComponent::MAX_INDEX
            || request.controller_index > WalletHdPathComponent::MAX_INDEX
        {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        let path = controller_path(request.account_index, request.controller_index)?;
        let mut result = None;
        let mut composition_error = None;
        let custody_result =
            self.custody
                .use_derived_secret(&request.profile_id, &path, &mut |secret| {
                    match self.compose_with_secret(request, secret) {
                        Ok(plan) => result = Some(plan),
                        Err(error) => {
                            composition_error = Some(error);
                            return Err(WalletSecurityPortError::InvalidOperation);
                        }
                    }
                    Ok(())
                });
        if let Some(error) = composition_error {
            return Err(error);
        }
        custody_result.map_err(map_security_error)?;
        result.ok_or(DidLifecyclePortError::ProtectionUnavailable)
    }

    fn compose_with_secret(
        &self,
        request: &NativeMidnightDidCallRequest,
        secret: &[u8; 32],
    ) -> Result<NativeMidnightDidCallPlan, DidLifecyclePortError> {
        let composer_request = ComposerRequest::new(request, secret);
        let body = Zeroizing::new(
            serde_json::to_vec(&composer_request)
                .map_err(|_| DidLifecyclePortError::InvalidOperation)?,
        );
        if body.is_empty() || body.len() > MAX_REQUEST_BYTES {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        let mut output = run_composer(&self.executable, body)?;
        let status = output.status;
        let stderr_is_empty = output.stderr.is_empty();
        let response = serde_json::from_slice(&output.stdout);
        output.stdout.zeroize();
        output.stderr.zeroize();
        let response: ComposerResponse =
            response.map_err(|_| DidLifecyclePortError::InvalidOperation)?;
        match response {
            ComposerResponse::Success(mut success) => {
                if !status.success()
                    || !stderr_is_empty
                    || success.schema_version != 1
                    || !success.ok
                    || success.operation_kind != request.operation.kind()
                    || success.circuit_id != request.operation.circuit_id()
                {
                    success.unproven_transaction_hex.zeroize();
                    return Err(DidLifecyclePortError::InvalidOperation);
                }
                let decoded = hex::decode(&success.unproven_transaction_hex);
                success.unproven_transaction_hex.zeroize();
                let mut transaction =
                    decoded.map_err(|_| DidLifecyclePortError::InvalidOperation)?;
                if transaction.is_empty()
                    || transaction.len() != success.unproven_transaction_bytes
                    || transaction.len() > MAX_RESPONSE_BYTES
                    || validate_transaction(&transaction, &request.context.network_id).is_err()
                {
                    transaction.zeroize();
                    return Err(DidLifecyclePortError::InvalidOperation);
                }
                let planning_fingerprint = Sha256::digest(&transaction);
                Ok(NativeMidnightDidCallPlan {
                    planning_fingerprint: planning_fingerprint.into(),
                    transaction: Zeroizing::new(transaction),
                })
            }
            ComposerResponse::Failure(failure) => {
                if status.success()
                    || !stderr_is_empty
                    || failure.schema_version != 1
                    || failure.ok
                    || failure.error.message.is_empty()
                {
                    return Err(DidLifecyclePortError::InvalidOperation);
                }
                Err(match failure.error.code.as_str() {
                    "unavailable" | "composition_failed" => {
                        DidLifecyclePortError::ProtectionUnavailable
                    }
                    _ => DidLifecyclePortError::InvalidOperation,
                })
            }
        }
    }
}

fn controller_path(
    account_index: u32,
    controller_index: u32,
) -> Result<WalletHdPath, DidLifecyclePortError> {
    let component = |value, hardened| {
        WalletHdPathComponent::new(value, hardened)
            .map_err(|_| DidLifecyclePortError::InvalidOperation)
    };
    WalletHdPath::new(vec![
        component(BIP44_PURPOSE, true)?,
        component(MIDNIGHT_COIN_TYPE, true)?,
        component(account_index, true)?,
        component(DID_CONTROLLER_ROLE, false)?,
        component(controller_index, false)?,
    ])
    .map_err(|_| DidLifecyclePortError::InvalidOperation)
}

struct SecretHex<'a>(&'a [u8; 32]);

impl Serialize for SecretHex<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        struct DisplayHex<'a>(&'a [u8]);
        impl fmt::Display for DisplayHex<'_> {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in self.0 {
                    write!(formatter, "{byte:02x}")?;
                }
                Ok(())
            }
        }
        serializer.collect_str(&DisplayHex(self.0))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerRequest<'a> {
    schema_version: u8,
    operation: ComposerOperation<'a>,
    chain: ComposerChain<'a>,
    wallet: ComposerWallet,
    controller: ComposerController<'a>,
}

impl<'a> ComposerRequest<'a> {
    fn new(request: &'a NativeMidnightDidCallRequest, secret: &'a [u8; 32]) -> Self {
        Self {
            schema_version: 1,
            operation: ComposerOperation::from(&request.operation),
            chain: ComposerChain {
                contract_state_hex: hex::encode(&request.context.contract_state),
                contract_address_hex: hex::encode(request.context.contract_address),
                zswap_chain_state_hex: request.context.zswap_chain_state.as_ref().map(hex::encode),
                ledger_parameters_hex: request.context.ledger_parameters.as_ref().map(hex::encode),
                network_id: &request.context.network_id,
                timestamp_millis: request.context.timestamp_millis,
            },
            wallet: ComposerWallet {
                coin_public_key_hex: hex::encode(request.context.coin_public_key),
                encryption_public_key_hex: hex::encode(request.context.encryption_public_key),
            },
            controller: ComposerController {
                secret_hex: SecretHex(secret),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerOperation<'a> {
    kind: &'static str,
    method_id: &'a str,
    public_key: Option<ComposerPublicKey>,
}

impl<'a> From<&'a MidnightDidCallOperation> for ComposerOperation<'a> {
    fn from(operation: &'a MidnightDidCallOperation) -> Self {
        match operation {
            MidnightDidCallOperation::AddAuthenticationMethod { method_id, x } => Self {
                kind: operation.kind(),
                method_id,
                public_key: Some(ComposerPublicKey::Ed25519 {
                    x_hex: hex::encode(x),
                }),
            },
            MidnightDidCallOperation::AddAssertionMethod { method_id, x, y } => Self {
                kind: operation.kind(),
                method_id,
                public_key: Some(ComposerPublicKey::Jubjub {
                    x_hex: hex::encode(x),
                    y_hex: hex::encode(y),
                }),
            },
            MidnightDidCallOperation::AddAuthenticationRelationship { method_id }
            | MidnightDidCallOperation::AddAssertionRelationship { method_id } => Self {
                kind: operation.kind(),
                method_id,
                public_key: None,
            },
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum ComposerPublicKey {
    Ed25519 {
        #[serde(rename = "xHex")]
        x_hex: String,
    },
    Jubjub {
        #[serde(rename = "xHex")]
        x_hex: String,
        #[serde(rename = "yHex")]
        y_hex: String,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerChain<'a> {
    contract_state_hex: String,
    contract_address_hex: String,
    zswap_chain_state_hex: Option<String>,
    ledger_parameters_hex: Option<String>,
    network_id: &'a str,
    timestamp_millis: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerWallet {
    coin_public_key_hex: String,
    encryption_public_key_hex: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComposerController<'a> {
    secret_hex: SecretHex<'a>,
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

struct ComposerOutput {
    status: ExitStatus,
    stdout: Zeroizing<Vec<u8>>,
    stderr: Zeroizing<Vec<u8>>,
}

fn run_composer(
    executable: &Path,
    mut request: Zeroizing<Vec<u8>>,
) -> Result<ComposerOutput, DidLifecyclePortError> {
    let mut child = Command::new(executable)
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| DidLifecyclePortError::ProtectionUnavailable)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(DidLifecyclePortError::ProtectionUnavailable)?;
    let stderr = child
        .stderr
        .take()
        .ok_or(DidLifecyclePortError::ProtectionUnavailable)?;
    let stdout_reader = read_bounded(stdout, MAX_RESPONSE_BYTES);
    let stderr_reader = read_bounded(stderr, MAX_STDERR_BYTES);
    let write_result = child
        .stdin
        .take()
        .ok_or(DidLifecyclePortError::ProtectionUnavailable)
        .and_then(|mut stdin| {
            stdin
                .write_all(&request)
                .and_then(|()| stdin.flush())
                .map_err(|_| DidLifecyclePortError::ProtectionUnavailable)
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
                return Err(DidLifecyclePortError::ProtectionUnavailable);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(DidLifecyclePortError::ProtectionUnavailable);
            }
        }
    };
    Ok(ComposerOutput {
        status,
        stdout: join_reader(stdout_reader)?,
        stderr: join_reader(stderr_reader)?,
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
) -> Result<Zeroizing<Vec<u8>>, DidLifecyclePortError> {
    reader
        .join()
        .map_err(|_| DidLifecyclePortError::ProtectionUnavailable)?
        .map_err(|()| DidLifecyclePortError::InvalidOperation)
}

fn validate_transaction(bytes: &[u8], network_id: &str) -> Result<(), ()> {
    let mut cursor = Cursor::new(bytes);
    let transaction: UnprovenTransaction = tagged_deserialize(&mut cursor).map_err(|_| ())?;
    if cursor.position() != bytes.len() as u64 {
        return Err(());
    }
    let Transaction::Standard(standard) = transaction else {
        return Err(());
    };
    if standard.network_id != network_id || standard.intents.iter().count() != 1 {
        return Err(());
    }
    Ok(())
}

const fn map_security_error(error: WalletSecurityPortError) -> DidLifecyclePortError {
    match error {
        WalletSecurityPortError::Locked => DidLifecyclePortError::Locked,
        WalletSecurityPortError::UnsupportedAlgorithm => {
            DidLifecyclePortError::UnsupportedAlgorithm
        }
        WalletSecurityPortError::NotFound => DidLifecyclePortError::NotFound,
        WalletSecurityPortError::Conflict => DidLifecyclePortError::Conflict,
        WalletSecurityPortError::Unavailable | WalletSecurityPortError::NotInitialized => {
            DidLifecyclePortError::ProtectionUnavailable
        }
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => DidLifecyclePortError::InvalidOperation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UnreachableCustody;

    impl WalletDerivedSecretUsePort for UnreachableCustody {
        fn use_derived_secret(
            &self,
            _: &WalletProfileId,
            _: &WalletHdPath,
            _: &mut dyn FnMut(&[u8; 32]) -> Result<(), WalletSecurityPortError>,
        ) -> Result<(), WalletSecurityPortError> {
            panic!("invalid public input must not reach custody")
        }
    }

    fn context() -> MidnightDidCallContext {
        MidnightDidCallContext {
            contract_state: vec![1],
            contract_address: [2; 32],
            zswap_chain_state: None,
            ledger_parameters: None,
            network_id: "undeployed".to_owned(),
            timestamp_millis: 1,
            coin_public_key: [3; 32],
            encryption_public_key: [4; 32],
        }
    }

    #[test]
    fn requires_an_absolute_regular_executable() {
        assert_eq!(
            NativeMidnightDidCallComposer::new("relative/composer", Arc::new(UnreachableCustody))
                .err(),
            Some(MidnightDidCallComposerConfigError::PathNotAbsolute)
        );
    }

    #[test]
    fn rejects_invalid_public_material_before_custody() {
        let executable = std::env::current_exe().expect("current executable");
        let composer = NativeMidnightDidCallComposer::new(executable, Arc::new(UnreachableCustody))
            .expect("composer configuration");
        let request = NativeMidnightDidCallRequest {
            profile_id: WalletProfileId::parse("profile-1".to_owned()).expect("profile"),
            account_index: 0,
            controller_index: 0,
            context: context(),
            operation: MidnightDidCallOperation::AddAuthenticationMethod {
                method_id: "#key-auth".to_owned(),
                x: [0; 32],
            },
        };
        assert_eq!(
            composer.compose(&request).err(),
            Some(DidLifecyclePortError::InvalidOperation)
        );
    }

    #[test]
    fn serializes_public_keys_and_controller_secret_to_the_closed_schema() {
        let request = NativeMidnightDidCallRequest {
            profile_id: WalletProfileId::parse("profile-1".to_owned()).expect("profile"),
            account_index: 0,
            controller_index: 0,
            context: context(),
            operation: MidnightDidCallOperation::AddAssertionMethod {
                method_id: "#key-assert".to_owned(),
                x: [5; 32],
                y: [6; 32],
            },
        };
        let secret = [7; 32];
        let value = serde_json::to_value(ComposerRequest::new(&request, &secret))
            .expect("serialize request");
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["operation"]["kind"], "add_assertion_method");
        assert_eq!(value["operation"]["publicKey"]["xHex"], "05".repeat(32));
        assert_eq!(value["operation"]["publicKey"]["yHex"], "06".repeat(32));
        assert_eq!(value["controller"]["secretHex"], "07".repeat(32));
    }
}
