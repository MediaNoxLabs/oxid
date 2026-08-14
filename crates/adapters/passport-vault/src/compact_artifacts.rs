// SPDX-License-Identifier: Apache-2.0

//! Authenticated generated-Compact and proving artifacts for Passport Vault calls.
//!
//! The wallet accepts one reviewed Nix artifact set. The generated client,
//! contract metadata, user-call proving keys, verifier keys, ZKIR, and circuit
//! parameters are pinned by size and digest. `setTrustedIssuer` remains visible
//! in the authenticated ABI but is deliberately absent from the wallet resolver.

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Cursor, Read as _},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

#[cfg(test)]
use std::borrow::Cow;

use midnight_transient_crypto::proofs::{
    KeyLocation, ParamsProver, ParamsProverProvider, ProvingKeyMaterial, Resolver, Zkir as _,
};
use midnight_zkir::IrSource;
use oxid_passport_vault_application::PassportVaultCallKind;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_MAX_BYTES: u64 = 1024 * 1024;
const ARTIFACT_DIRECTORY: &str = "artifacts";
const EXPECTED_ARTIFACT_SET: &str = "oxid-passport-vault-v1";
const EXPECTED_VAULT_REVISION: &str = "e4a92a6be2cc6dc34f68261f10c19c9312043807";
const EXPECTED_CREDENTIAL_REVISION: &str = "39b1354212620b396e914b29603e6a38f2656546";
const EXPECTED_COMPILER_REVISION: &str = "05b237a5e51f9c22853b424e7d4236dfa9384c24";
const EXPECTED_CONTRACT_SHA256: &str =
    "2ebc5b34dd440bc9a9736408f29f5003e7a78f26a564b392be2af36de69102f4";

const CONTRACT_INFO: ExpectedArtifact = ExpectedArtifact {
    path: "compiler/contract-info.json",
    bytes: 538_588,
    sha256: "999f3a6166d1f8825d3faf68fbae42e18c291a1ba8e289b6299675d67e2a4262",
};
const GENERATED_TYPES: ExpectedArtifact = ExpectedArtifact {
    path: "contract/index.d.ts",
    bytes: 68_707,
    sha256: "fa1d7839666bf60d02229d63b31f7242784df57b9902639777918f9e7d208070",
};
const GENERATED_MODULE: ExpectedArtifact = ExpectedArtifact {
    path: "contract/index.js",
    bytes: 812_261,
    sha256: "4fa1d35012cee2a4ff999e05d03af12d4d7ee89d9151514411b6a0e448879780",
};
const CREATE_LOCK_PROVER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/createLock.prover",
    bytes: 552_824,
    sha256: "6d1581894e6a91e0629591a9805574fc9c34178c3de7c77a12328eaaadb910bd",
};
const CREATE_LOCK_VERIFIER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/createLock.verifier",
    bytes: 1_351,
    sha256: "809dbeb1063f2bfb67bd54f97af5476e75949c3e887ae7106080bbc856af0fbb",
};
const CREATE_LOCK_IR: ExpectedArtifact = ExpectedArtifact {
    path: "zkir/createLock.bzkir",
    bytes: 731,
    sha256: "b5c8562386b64268a2e4183037258f8319cc9c2f4e8bc49d1043bd55a023d3c5",
};
const DEPOSIT_TO_LOCK_PROVER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/depositToLock.prover",
    bytes: 287_933,
    sha256: "48e8c2e5283bef392ab6425b7014fdd4669f5d0a6a8f45b5503318a6b62d187e",
};
const DEPOSIT_TO_LOCK_VERIFIER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/depositToLock.verifier",
    bytes: 1_351,
    sha256: "b02e0d2f635f854dcb310fba804bae47c502bf583d2be3c54cf6a09bc7281806",
};
const DEPOSIT_TO_LOCK_IR: ExpectedArtifact = ExpectedArtifact {
    path: "zkir/depositToLock.bzkir",
    bytes: 643,
    sha256: "f94426db85f375e7aef1a52443798a0b76d258544bf9ad9499b9f1a22561f164",
};
const CLAIM_FROM_LOCK_PROVER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/claimFromLock.prover",
    bytes: 42_858_912,
    sha256: "f79d143d7bd916fafe048174509f6fa8e9526b370051ca556726606ff48c6891",
};
const CLAIM_FROM_LOCK_VERIFIER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/claimFromLock.verifier",
    bytes: 2_311,
    sha256: "172bf8a2f05c6a52d8e3438855ce712da9b67d04d504e34d340aae057e2fc57e",
};
const CLAIM_FROM_LOCK_IR: ExpectedArtifact = ExpectedArtifact {
    path: "zkir/claimFromLock.bzkir",
    bytes: 4_545,
    sha256: "502c9c7ec83b2f0023e143395138673b3e384559fb75ab4fcc5fcd6a32ebabe6",
};
const WITHDRAW_FROM_LOCK_PROVER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/withdrawFromLock.prover",
    bytes: 552_418,
    sha256: "208cda55350c91d8e6b1e2fdb345c2b64e7dda57976502e9fc3ad528420cd9b5",
};
const WITHDRAW_FROM_LOCK_VERIFIER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/withdrawFromLock.verifier",
    bytes: 1_351,
    sha256: "dfef09b18e846dbc99631f63a5f5e8e036d3a31528a858b72af774c4a77ef5fd",
};
const WITHDRAW_FROM_LOCK_IR: ExpectedArtifact = ExpectedArtifact {
    path: "zkir/withdrawFromLock.bzkir",
    bytes: 827,
    sha256: "235e222cf2bb2e3bbe55848857e937e6bcfe5714a3e9596d17bb6f8d505218c3",
};
const PARAMETERS_2P10: ExpectedArtifact = ExpectedArtifact {
    path: "params/bls_midnight_2p10",
    bytes: 196_996,
    sha256: "46b2290933cbed4c378889e4ba971f1a92888331ffb09466acd4ff61a1e2cb42",
};
const PARAMETERS_2P11: ExpectedArtifact = ExpectedArtifact {
    path: "params/bls_midnight_2p11",
    bytes: 393_604,
    sha256: "9901589d7956ff58be0d85569b2f455b77b58c3758026ffb5bbe4807000b96d1",
};
const PARAMETERS_2P17: ExpectedArtifact = ExpectedArtifact {
    path: "params/bls_midnight_2p17",
    bytes: 25_166_212,
    sha256: "4a9ef6c7c0619aab74eede44b13e753e3ba54508a02dd3b7106a949aabb73b74",
};

const REQUIRED_ARTIFACTS: [ExpectedArtifact; 18] = [
    CONTRACT_INFO,
    GENERATED_TYPES,
    GENERATED_MODULE,
    CREATE_LOCK_PROVER,
    CREATE_LOCK_VERIFIER,
    CREATE_LOCK_IR,
    DEPOSIT_TO_LOCK_PROVER,
    DEPOSIT_TO_LOCK_VERIFIER,
    DEPOSIT_TO_LOCK_IR,
    CLAIM_FROM_LOCK_PROVER,
    CLAIM_FROM_LOCK_VERIFIER,
    CLAIM_FROM_LOCK_IR,
    WITHDRAW_FROM_LOCK_PROVER,
    WITHDRAW_FROM_LOCK_VERIFIER,
    WITHDRAW_FROM_LOCK_IR,
    PARAMETERS_2P10,
    PARAMETERS_2P11,
    PARAMETERS_2P17,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultCompactCircuit {
    CreateLock,
    DepositToLock,
    ClaimFromLock,
    WithdrawFromLock,
}

impl PassportVaultCompactCircuit {
    pub const ALL: [Self; 4] = [
        Self::CreateLock,
        Self::DepositToLock,
        Self::ClaimFromLock,
        Self::WithdrawFromLock,
    ];

    #[must_use]
    pub const fn for_call_kind(kind: PassportVaultCallKind) -> Self {
        match kind {
            PassportVaultCallKind::CreateLock => Self::CreateLock,
            PassportVaultCallKind::DepositToLock => Self::DepositToLock,
            PassportVaultCallKind::ClaimFromLock => Self::ClaimFromLock,
            PassportVaultCallKind::WithdrawFromLock => Self::WithdrawFromLock,
        }
    }

    #[must_use]
    pub const fn circuit_id(self) -> &'static str {
        match self {
            Self::CreateLock => "createLock",
            Self::DepositToLock => "depositToLock",
            Self::ClaimFromLock => "claimFromLock",
            Self::WithdrawFromLock => "withdrawFromLock",
        }
    }

    #[must_use]
    pub const fn k(self) -> u8 {
        match self {
            Self::CreateLock | Self::WithdrawFromLock => 11,
            Self::DepositToLock => 10,
            Self::ClaimFromLock => 17,
        }
    }

    #[must_use]
    pub const fn rows(self) -> u64 {
        match self {
            Self::CreateLock => 1_823,
            Self::DepositToLock => 834,
            Self::ClaimFromLock => 124_785,
            Self::WithdrawFromLock => 1_212,
        }
    }

    fn from_key_location(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|circuit| circuit.circuit_id() == value)
    }

    const fn prover(self) -> ExpectedArtifact {
        match self {
            Self::CreateLock => CREATE_LOCK_PROVER,
            Self::DepositToLock => DEPOSIT_TO_LOCK_PROVER,
            Self::ClaimFromLock => CLAIM_FROM_LOCK_PROVER,
            Self::WithdrawFromLock => WITHDRAW_FROM_LOCK_PROVER,
        }
    }

    const fn verifier(self) -> ExpectedArtifact {
        match self {
            Self::CreateLock => CREATE_LOCK_VERIFIER,
            Self::DepositToLock => DEPOSIT_TO_LOCK_VERIFIER,
            Self::ClaimFromLock => CLAIM_FROM_LOCK_VERIFIER,
            Self::WithdrawFromLock => WITHDRAW_FROM_LOCK_VERIFIER,
        }
    }

    const fn ir(self) -> ExpectedArtifact {
        match self {
            Self::CreateLock => CREATE_LOCK_IR,
            Self::DepositToLock => DEPOSIT_TO_LOCK_IR,
            Self::ClaimFromLock => CLAIM_FROM_LOCK_IR,
            Self::WithdrawFromLock => WITHDRAW_FROM_LOCK_IR,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultCompactArtifactsConfig {
    root: PathBuf,
}

impl PassportVaultCompactArtifactsConfig {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, PassportVaultCompactArtifactError> {
        let root = root.into();
        if !root.is_absolute()
            || root
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(PassportVaultCompactArtifactError::InvalidConfiguration);
        }
        Ok(Self { root })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultCompactArtifactError {
    InvalidConfiguration,
    ArtifactUnavailable,
    ArtifactMismatch,
    CircuitMismatch,
}

impl std::fmt::Display for PassportVaultCompactArtifactError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidConfiguration => "invalid Passport Vault Compact artifact configuration",
            Self::ArtifactUnavailable => "Passport Vault Compact artifacts are unavailable",
            Self::ArtifactMismatch => "Passport Vault Compact artifact authentication failed",
            Self::CircuitMismatch => "Passport Vault Compact circuit identity is invalid",
        })
    }
}

impl std::error::Error for PassportVaultCompactArtifactError {}

#[derive(Clone)]
pub struct NativePassportVaultCompactArtifacts {
    artifact_root: PathBuf,
    identity: [u8; 32],
    parameters: Arc<Mutex<BTreeMap<u8, ParamsProver>>>,
}

impl NativePassportVaultCompactArtifacts {
    pub fn load(
        config: &PassportVaultCompactArtifactsConfig,
    ) -> Result<Self, PassportVaultCompactArtifactError> {
        let root = canonical_artifact_root(config.root())?;
        let manifest_bytes = read_regular_file(&root.join(MANIFEST_FILE), MANIFEST_MAX_BYTES)?;
        let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| PassportVaultCompactArtifactError::ArtifactMismatch)?;
        validate_manifest(&manifest)?;

        let artifact_root = root.join(ARTIFACT_DIRECTORY);
        let metadata = fs::symlink_metadata(&artifact_root)
            .map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
        }
        for expected in REQUIRED_ARTIFACTS {
            authenticate_artifact(&artifact_root, expected)?;
        }
        validate_contract_info(&read_artifact(&artifact_root, CONTRACT_INFO)?)?;
        for circuit in PassportVaultCompactCircuit::ALL {
            let ir = IrSource::load_from_tagged(Cursor::new(read_artifact(
                &artifact_root,
                circuit.ir(),
            )?))
            .map_err(|_| PassportVaultCompactArtifactError::CircuitMismatch)?;
            // Row counts are bound by the exact IR digest and manifest. Building
            // the full claim constraint model here costs roughly a minute on a
            // phone-class CPU, so runtime admission checks the encoded degree
            // and leaves model expansion to actual proof checking.
            if ir.k() != circuit.k() {
                return Err(PassportVaultCompactArtifactError::CircuitMismatch);
            }
        }

        Ok(Self {
            artifact_root,
            identity: artifact_identity(),
            parameters: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    #[must_use]
    pub const fn identity(&self) -> [u8; 32] {
        self.identity
    }

    /// Returns the exact authenticated generated ES module for a bounded
    /// headless composer. The module is data, not an ambient filesystem route.
    pub fn generated_contract_module(&self) -> Result<Vec<u8>, PassportVaultCompactArtifactError> {
        read_artifact(&self.artifact_root, GENERATED_MODULE)
    }

    fn parameter_artifact(k: u8) -> Option<ExpectedArtifact> {
        match k {
            10 => Some(PARAMETERS_2P10),
            11 => Some(PARAMETERS_2P11),
            17 => Some(PARAMETERS_2P17),
            _ => None,
        }
    }
}

impl Resolver for NativePassportVaultCompactArtifacts {
    async fn resolve_key(&self, key: KeyLocation) -> std::io::Result<Option<ProvingKeyMaterial>> {
        let Some(circuit) = PassportVaultCompactCircuit::from_key_location(key.0.as_ref()) else {
            return Ok(None);
        };
        let read = |artifact| {
            read_artifact(&self.artifact_root, artifact).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Passport Vault proving artifact authentication failed",
                )
            })
        };
        Ok(Some(ProvingKeyMaterial {
            prover_key: read(circuit.prover())?,
            verifier_key: read(circuit.verifier())?,
            ir_source: read(circuit.ir())?,
        }))
    }
}

impl ParamsProverProvider for NativePassportVaultCompactArtifacts {
    async fn get_params(&self, k: u8) -> std::io::Result<ParamsProver> {
        if let Some(parameters) = self
            .parameters
            .lock()
            .map_err(|_| std::io::Error::other("Passport Vault parameter lock poisoned"))?
            .get(&k)
            .cloned()
        {
            return Ok(parameters);
        }
        let artifact = Self::parameter_artifact(k).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "unsupported Passport Vault circuit degree",
            )
        })?;
        let bytes = read_artifact(&self.artifact_root, artifact).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Passport Vault parameter authentication failed",
            )
        })?;
        let parsed = ParamsProver::read(bytes.as_slice())?;
        self.parameters
            .lock()
            .map_err(|_| std::io::Error::other("Passport Vault parameter lock poisoned"))?
            .insert(k, parsed.clone());
        Ok(parsed)
    }
}

fn canonical_artifact_root(root: &Path) -> Result<PathBuf, PassportVaultCompactArtifactError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(PassportVaultCompactArtifactError::InvalidConfiguration);
    }
    let canonical = fs::canonicalize(root)
        .map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
    if canonical != root {
        return Err(PassportVaultCompactArtifactError::InvalidConfiguration);
    }
    Ok(canonical)
}

fn read_regular_file(
    path: &Path,
    maximum_bytes: u64,
) -> Result<Vec<u8>, PassportVaultCompactArtifactError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > maximum_bytes
    {
        return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
    }
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| PassportVaultCompactArtifactError::ArtifactMismatch)?;
    let file =
        fs::File::open(path).map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
    let mut reader = file.take(maximum_bytes.saturating_add(1));
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .read_to_end(&mut bytes)
        .map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
    if u64::try_from(bytes.len()).ok() != Some(metadata.len()) {
        return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
    }
    Ok(bytes)
}

fn artifact_path(
    artifact_root: &Path,
    relative_path: &str,
) -> Result<PathBuf, PassportVaultCompactArtifactError> {
    let components = Path::new(relative_path).components().collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
    }
    let mut current = artifact_root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
        if metadata.file_type().is_symlink()
            || (index + 1 == components.len() && !metadata.is_file())
            || (index + 1 != components.len() && !metadata.is_dir())
        {
            return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
        }
    }
    Ok(current)
}

fn authenticate_artifact(
    artifact_root: &Path,
    expected: ExpectedArtifact,
) -> Result<(), PassportVaultCompactArtifactError> {
    let path = artifact_path(artifact_root, expected.path)?;
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
    if metadata.len() != expected.bytes {
        return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
    }
    let file =
        fs::File::open(path).map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
    let mut reader = file.take(expected.bytes.saturating_add(1));
    let mut digest = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| PassportVaultCompactArtifactError::ArtifactUnavailable)?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(
                u64::try_from(read)
                    .map_err(|_| PassportVaultCompactArtifactError::ArtifactMismatch)?,
            )
            .ok_or(PassportVaultCompactArtifactError::ArtifactMismatch)?;
        digest.update(&buffer[..read]);
    }
    if copied != expected.bytes || hex::encode(digest.finalize()) != expected.sha256 {
        return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
    }
    Ok(())
}

fn read_artifact(
    artifact_root: &Path,
    expected: ExpectedArtifact,
) -> Result<Vec<u8>, PassportVaultCompactArtifactError> {
    authenticate_artifact(artifact_root, expected)?;
    read_regular_file(
        &artifact_path(artifact_root, expected.path)?,
        expected.bytes,
    )
}

fn artifact_identity() -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:passport-vault-compact-artifact-set:v1\0");
    digest.update(EXPECTED_ARTIFACT_SET.as_bytes());
    digest.update(EXPECTED_VAULT_REVISION.as_bytes());
    digest.update(EXPECTED_CREDENTIAL_REVISION.as_bytes());
    digest.update(EXPECTED_COMPILER_REVISION.as_bytes());
    for artifact in REQUIRED_ARTIFACTS {
        digest.update(artifact.path.as_bytes());
        digest.update([0]);
        digest.update(artifact.sha256.as_bytes());
        digest.update(artifact.bytes.to_be_bytes());
    }
    digest.finalize().into()
}

fn validate_manifest(manifest: &ArtifactManifest) -> Result<(), PassportVaultCompactArtifactError> {
    if manifest.schema_version != 1
        || manifest.artifact_set != EXPECTED_ARTIFACT_SET
        || manifest.source.revision != EXPECTED_VAULT_REVISION
        || manifest.source.contract_sha256 != EXPECTED_CONTRACT_SHA256
        || manifest.source.credential_revision != EXPECTED_CREDENTIAL_REVISION
        || manifest.toolchain.compact_cli_version != "0.5.1"
        || manifest.toolchain.compiler_version != "0.30.0"
        || manifest.toolchain.compiler_language_version != "0.22.0"
        || manifest.toolchain.generated_runtime_version != "0.15.0"
        || manifest.toolchain.source_revision != EXPECTED_COMPILER_REVISION
    {
        return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
    }

    let expected_parameters = [PARAMETERS_2P10, PARAMETERS_2P11, PARAMETERS_2P17];
    for expected in expected_parameters {
        let name = expected
            .path
            .strip_prefix("params/")
            .ok_or(PassportVaultCompactArtifactError::ArtifactMismatch)?;
        if manifest
            .toolchain
            .circuit_parameters
            .iter()
            .filter(|parameter| parameter.name == name && parameter.sha256 == expected.sha256)
            .count()
            != 1
        {
            return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
        }
    }

    let expected_circuits = [
        ("setTrustedIssuer", 13, 5_416),
        ("createLock", 11, 1_823),
        ("depositToLock", 10, 834),
        ("claimFromLock", 17, 124_785),
        ("withdrawFromLock", 11, 1_212),
    ];
    if manifest.circuits.len() != expected_circuits.len()
        || expected_circuits.iter().any(|(id, k, rows)| {
            manifest
                .circuits
                .iter()
                .filter(|circuit| circuit.id == *id && circuit.k == *k && circuit.rows == *rows)
                .count()
                != 1
        })
    {
        return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
    }

    let mut paths = HashSet::with_capacity(manifest.artifacts.len());
    if manifest
        .artifacts
        .iter()
        .any(|artifact| !paths.insert(artifact.path.as_str()))
    {
        return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
    }
    for expected in REQUIRED_ARTIFACTS {
        if manifest
            .artifacts
            .iter()
            .filter(|artifact| {
                artifact.path == expected.path
                    && artifact.bytes == expected.bytes
                    && artifact.sha256 == expected.sha256
            })
            .count()
            != 1
        {
            return Err(PassportVaultCompactArtifactError::ArtifactMismatch);
        }
    }
    Ok(())
}

fn validate_contract_info(bytes: &[u8]) -> Result<(), PassportVaultCompactArtifactError> {
    let info: ContractInfo = serde_json::from_slice(bytes)
        .map_err(|_| PassportVaultCompactArtifactError::CircuitMismatch)?;
    if info.compiler_version != "0.30.0"
        || info.language_version != "0.22.0"
        || info.runtime_version != "0.15.0"
    {
        return Err(PassportVaultCompactArtifactError::CircuitMismatch);
    }
    let expected = [
        (
            "setTrustedIssuer",
            ["newIssuerRef", "newIssuerPublicKey"].as_slice(),
        ),
        (
            "createLock",
            [
                "minAge",
                "reqIssuingState",
                "reqIssuingStateValue",
                "reqDocumentNumber",
                "reqDocumentNumberValue",
                "maxClaim",
                "configuredVerifierChallengeHash",
                "initialAmount",
            ]
            .as_slice(),
        ),
        ("depositToLock", ["lockId", "amount"].as_slice()),
        (
            "claimFromLock",
            [
                "lockId",
                "credential",
                "credentialProof",
                "presentation",
                "presentationProof",
                "currentDay",
                "requestedAmount",
                "recipientAddress",
            ]
            .as_slice(),
        ),
        (
            "withdrawFromLock",
            ["lockId", "amount", "recipientAddress"].as_slice(),
        ),
    ];
    for (name, argument_names) in expected {
        let matches = info
            .circuits
            .iter()
            .filter(|circuit| circuit.name == name && !circuit.pure && circuit.proof)
            .collect::<Vec<_>>();
        if matches.len() != 1
            || matches[0]
                .arguments
                .iter()
                .map(|argument| argument.name.as_str())
                .ne(argument_names.iter().copied())
        {
            return Err(PassportVaultCompactArtifactError::CircuitMismatch);
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ExpectedArtifact {
    path: &'static str,
    bytes: u64,
    sha256: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactManifest {
    schema_version: u64,
    artifact_set: String,
    source: SourceManifest,
    toolchain: ToolchainManifest,
    circuits: Vec<CircuitManifest>,
    artifacts: Vec<ArtifactEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceManifest {
    revision: String,
    contract_sha256: String,
    credential_revision: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolchainManifest {
    compact_cli_version: String,
    compiler_version: String,
    compiler_language_version: String,
    generated_runtime_version: String,
    source_revision: String,
    circuit_parameters: Vec<ParameterManifest>,
}

#[derive(Deserialize)]
struct ParameterManifest {
    name: String,
    sha256: String,
}

#[derive(Deserialize)]
struct CircuitManifest {
    id: String,
    k: u8,
    rows: u64,
}

#[derive(Deserialize)]
struct ArtifactEntry {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct ContractInfo {
    #[serde(rename = "compiler-version")]
    compiler_version: String,
    #[serde(rename = "language-version")]
    language_version: String,
    #[serde(rename = "runtime-version")]
    runtime_version: String,
    circuits: Vec<ContractInfoCircuit>,
}

#[derive(Deserialize)]
struct ContractInfoCircuit {
    name: String,
    pure: bool,
    proof: bool,
    arguments: Vec<ContractInfoArgument>,
}

#[derive(Deserialize)]
struct ContractInfoArgument {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_requires_an_absolute_normalized_root() {
        assert_eq!(
            PassportVaultCompactArtifactsConfig::new("relative/artifacts"),
            Err(PassportVaultCompactArtifactError::InvalidConfiguration)
        );
        assert_eq!(
            PassportVaultCompactArtifactsConfig::new("/tmp/../artifacts"),
            Err(PassportVaultCompactArtifactError::InvalidConfiguration)
        );
        assert!(PassportVaultCompactArtifactsConfig::new("/nix/store/artifacts").is_ok());
    }

    #[test]
    fn wallet_circuit_mapping_is_closed_and_excludes_administration() {
        let mappings = [
            (PassportVaultCallKind::CreateLock, "createLock", 11),
            (PassportVaultCallKind::DepositToLock, "depositToLock", 10),
            (PassportVaultCallKind::ClaimFromLock, "claimFromLock", 17),
            (
                PassportVaultCallKind::WithdrawFromLock,
                "withdrawFromLock",
                11,
            ),
        ];
        for (kind, id, k) in mappings {
            let circuit = PassportVaultCompactCircuit::for_call_kind(kind);
            assert_eq!(circuit.circuit_id(), id);
            assert_eq!(circuit.k(), k);
            assert_eq!(
                PassportVaultCompactCircuit::from_key_location(id),
                Some(circuit)
            );
        }
        assert_eq!(
            PassportVaultCompactCircuit::from_key_location("setTrustedIssuer"),
            None
        );
    }

    #[test]
    fn packaged_artifacts_authenticate_and_resolve_only_wallet_circuits_when_configured() {
        let Some(root) = std::env::var_os("OXID_PASSPORT_VAULT_ARTIFACTS_DIR") else {
            return;
        };
        let config = PassportVaultCompactArtifactsConfig::new(root).expect("artifact config");
        let artifacts = NativePassportVaultCompactArtifacts::load(&config)
            .expect("Nix Passport Vault artifacts authenticate");
        assert_ne!(artifacts.identity(), [0; 32]);
        let module = artifacts
            .generated_contract_module()
            .expect("generated module remains authenticated");
        assert!(module.starts_with(b"import * as __compactRuntime"));

        let deposit = futures::executor::block_on(
            artifacts.resolve_key(KeyLocation(Cow::Borrowed("depositToLock"))),
        )
        .expect("resolver")
        .expect("wallet circuit");
        assert_eq!(
            deposit.prover_key.len(),
            DEPOSIT_TO_LOCK_PROVER.bytes as usize
        );
        assert_eq!(
            deposit.verifier_key.len(),
            DEPOSIT_TO_LOCK_VERIFIER.bytes as usize
        );
        assert_eq!(deposit.ir_source.len(), DEPOSIT_TO_LOCK_IR.bytes as usize);
        assert!(
            futures::executor::block_on(
                artifacts.resolve_key(KeyLocation(Cow::Borrowed("setTrustedIssuer")))
            )
            .expect("resolver")
            .is_none()
        );
        assert!(futures::executor::block_on(artifacts.get_params(10)).is_ok());
        assert!(futures::executor::block_on(artifacts.get_params(13)).is_err());
    }
}
