// SPDX-License-Identifier: Apache-2.0

//! Authenticated native runtime for the reviewed Digital Passport Compact
//! presentation circuit.
//!
//! The runtime accepts one immutable, reproducibly generated artifact set. It
//! never downloads keys or parameters, never follows artifact symlinks, checks
//! every runtime file against a compiled-in digest and size, runs the IR before
//! proving, and independently verifies the resulting proof against a public
//! statement reconstructed outside the prover.

use std::{
    fs,
    io::{Cursor, Read as _},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

use midnight_serialize::{tagged_deserialize, tagged_serialize};
use midnight_transient_crypto::{
    curve::Fr,
    proofs::{
        KeyLocation, ParamsProver, ParamsProverProvider, ParamsVerifier, Proof, ProofPreimage,
        ProvingKeyMaterial, Resolver, VerifierKey,
    },
};
use midnight_zkir::IrSource;
use rand::rngs::OsRng;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::compact_proving::PRESENTATION_CIRCUIT_KEY_LOCATION;

const PORTABLE_MAGIC: &[u8; 4] = b"MZP1";
const PORTABLE_VERSION: u16 = 1;
const PORTABLE_CHUNKS: u16 = 7;
const PORTABLE_MAX_BYTES: usize = 4 * 1024 * 1024;
const PORTABLE_MAX_PROOF_BYTES: usize = 3 * 1024 * 1024;
const PORTABLE_PUBLIC_INPUT_BYTES: usize = 524;
const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_MAX_BYTES: u64 = 1024 * 1024;
const ARTIFACT_DIRECTORY: &str = "artifacts";
const EXPECTED_ARTIFACT_SET: &str = "oxid-digital-passport-presentation-v1";
const EXPECTED_UPSTREAM_REVISION: &str = "39b1354212620b396e914b29603e6a38f2656546";
const EXPECTED_COMPILER_REVISION: &str = "05b237a5e51f9c22853b424e7d4236dfa9384c24";
const EXPECTED_CONTRACT_SHA256: &str =
    "0c8dc4c3a29f3bff631188a2a235399baed0619d6aec71d7663847474c48ac47";
const EXPECTED_CIRCUIT_ID: &str = "proveDigitalPassportPresentation";
const EXPECTED_PARAMETER_NAME: &str = "bls_midnight_2p18";
const EXPECTED_CIRCUIT_K: u8 = 18;
const EXPECTED_CIRCUIT_ROWS: u64 = 156_301;
const EXPECTED_INPUT_FIELDS: usize = 117;
const EXPECTED_PRIVATE_TRANSCRIPT_FIELDS: usize = 3;
const EXPECTED_PUBLIC_TRANSCRIPT_FIELDS: usize = 12;
const EXPECTED_PUBLIC_OUTPUT_FIELDS: usize = 0;

const PROVER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/proveDigitalPassportPresentation.prover",
    bytes: 85_011_711,
    sha256: "40361acbd4e86fff1b908d9929a2957345b78e6c24a31f9016922b73470e82e4",
};
const VERIFIER: ExpectedArtifact = ExpectedArtifact {
    path: "keys/proveDigitalPassportPresentation.verifier",
    bytes: 2_311,
    sha256: "626619840ff1b0640c9a953e00c013c9fbf68a7f94596b4ac1f4d6e3de76fbfd",
};
const IR: ExpectedArtifact = ExpectedArtifact {
    path: "zkir/proveDigitalPassportPresentation.bzkir",
    bytes: 2_915,
    sha256: "82b01c79b4947ab870e79bdbe52b020919e299985778ac15c9e96e7aa8ab27a0",
};
const PARAMETERS: ExpectedArtifact = ExpectedArtifact {
    path: "params/bls_midnight_2p18",
    bytes: 50_332_036,
    sha256: "e8436dc5d8b598f169c127c745135d889744007e6d384ff126df8d1332522f86",
};
const REQUIRED_ARTIFACTS: [ExpectedArtifact; 4] = [PROVER, VERIFIER, IR, PARAMETERS];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactPresentationArtifactsConfig {
    root: PathBuf,
}

impl CompactPresentationArtifactsConfig {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, CompactPresentationRuntimeError> {
        let root = root.into();
        if !root.is_absolute()
            || root
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(CompactPresentationRuntimeError::InvalidConfiguration);
        }
        Ok(Self { root })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactPresentationRuntimeError {
    InvalidConfiguration,
    ArtifactUnavailable,
    ArtifactMismatch,
    CircuitMismatch,
    InvalidPreimage,
    ProvingFailed,
    InvalidProof,
}

impl std::fmt::Display for CompactPresentationRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidConfiguration => "invalid Compact presentation artifact configuration",
            Self::ArtifactUnavailable => "Compact presentation artifact set is unavailable",
            Self::ArtifactMismatch => "Compact presentation artifact authentication failed",
            Self::CircuitMismatch => "Compact presentation circuit identity is invalid",
            Self::InvalidPreimage => "Compact presentation proof preimage is invalid",
            Self::ProvingFailed => "Compact presentation proving failed",
            Self::InvalidProof => "Compact presentation proof verification failed",
        })
    }
}

impl std::error::Error for CompactPresentationRuntimeError {}

#[derive(Clone)]
pub struct NativeCompactPresentationRuntime {
    identity: [u8; 32],
    prover_key: RuntimeArtifactBytes,
    verifier_key_bytes: RuntimeArtifactBytes,
    ir_bytes: RuntimeArtifactBytes,
    parameter_bytes: RuntimeArtifactBytes,
    ir: IrSource,
    params_prover: Arc<Mutex<Option<ParamsProver>>>,
    params_verifier: Arc<Mutex<Option<ParamsVerifier>>>,
    verifier_key: VerifierKey,
}

impl NativeCompactPresentationRuntime {
    pub fn load(
        config: &CompactPresentationArtifactsConfig,
    ) -> Result<Self, CompactPresentationRuntimeError> {
        let root = canonical_artifact_root(config.root())?;
        let manifest_bytes = read_regular_file(&root.join(MANIFEST_FILE), MANIFEST_MAX_BYTES)?;
        let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| CompactPresentationRuntimeError::ArtifactMismatch)?;
        validate_manifest(&manifest)?;

        let artifact_root = root.join(ARTIFACT_DIRECTORY);
        let artifact_metadata = fs::symlink_metadata(&artifact_root)
            .map_err(|_| CompactPresentationRuntimeError::ArtifactUnavailable)?;
        if !artifact_metadata.is_dir() || artifact_metadata.file_type().is_symlink() {
            return Err(CompactPresentationRuntimeError::ArtifactMismatch);
        }

        let mut authenticated = Vec::with_capacity(REQUIRED_ARTIFACTS.len());
        for expected in REQUIRED_ARTIFACTS {
            let bytes = read_artifact_file(&artifact_root, expected.path, expected.bytes)?;
            validate_authenticated_artifact(&manifest, expected, &bytes)?;
            authenticated.push(RuntimeArtifactBytes::owned(bytes));
        }
        let authenticated = authenticated
            .try_into()
            .map_err(|_| CompactPresentationRuntimeError::ArtifactMismatch)?;
        Self::from_authenticated_artifacts(authenticated)
    }

    fn from_authenticated_artifacts(
        [prover_key, verifier_key_bytes, ir_bytes, parameter_bytes]: [RuntimeArtifactBytes; 4],
    ) -> Result<Self, CompactPresentationRuntimeError> {
        let ir = IrSource::load_from_tagged(Cursor::new(ir_bytes.as_slice()))
            .map_err(|_| CompactPresentationRuntimeError::CircuitMismatch)?;
        let model = ir.model();
        if model.k() != EXPECTED_CIRCUIT_K
            || ir.num_inputs as usize != EXPECTED_INPUT_FIELDS
            || !ir.do_communications_commitment
            || u64::try_from(model.rows()).ok() != Some(EXPECTED_CIRCUIT_ROWS)
        {
            return Err(CompactPresentationRuntimeError::CircuitMismatch);
        }

        let mut verifier_reader = verifier_key_bytes.as_slice();
        let verifier_key: VerifierKey = tagged_deserialize(&mut verifier_reader)
            .map_err(|_| CompactPresentationRuntimeError::ArtifactMismatch)?;
        if !verifier_reader.is_empty() || verifier_key.init().is_err() {
            return Err(CompactPresentationRuntimeError::ArtifactMismatch);
        }
        let identity = artifact_identity();

        Ok(Self {
            identity,
            prover_key,
            verifier_key_bytes,
            ir_bytes,
            parameter_bytes,
            ir,
            params_prover: Arc::new(Mutex::new(None)),
            params_verifier: Arc::new(Mutex::new(None)),
            verifier_key,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> [u8; 32] {
        self.identity
    }

    pub fn check_preimage(
        &self,
        preimage: &ProofPreimage,
    ) -> Result<(), CompactPresentationRuntimeError> {
        if preimage.key_location.0.as_ref() != PRESENTATION_CIRCUIT_KEY_LOCATION
            || preimage.inputs.len() != EXPECTED_INPUT_FIELDS
            || preimage.private_transcript.len() != EXPECTED_PRIVATE_TRANSCRIPT_FIELDS
            || preimage.public_transcript_inputs.len() != EXPECTED_PUBLIC_TRANSCRIPT_FIELDS
            || preimage.public_transcript_outputs.len() != EXPECTED_PUBLIC_OUTPUT_FIELDS
            || preimage.binding_input != Fr::from(0_u64)
            || !matches!(
                preimage.communications_commitment,
                Some((_, randomness)) if randomness == Fr::from(0_u64)
            )
        {
            return Err(CompactPresentationRuntimeError::InvalidPreimage);
        }
        let skips = preimage
            .check(&self.ir)
            .map_err(|_| CompactPresentationRuntimeError::InvalidPreimage)?;
        if skips.iter().any(Option::is_some) {
            return Err(CompactPresentationRuntimeError::InvalidPreimage);
        }
        Ok(())
    }

    pub async fn prove(
        &self,
        preimage: &ProofPreimage,
    ) -> Result<Proof, CompactPresentationRuntimeError> {
        self.check_preimage(preimage)?;
        let (proof, skips) = preimage
            .prove::<IrSource>(OsRng, self, self)
            .await
            .map_err(|_| CompactPresentationRuntimeError::ProvingFailed)?;
        if skips.iter().any(Option::is_some) {
            return Err(CompactPresentationRuntimeError::ProvingFailed);
        }
        let (_, commitment) = public_binding(preimage)?;
        self.verify_public(
            preimage.binding_input,
            commitment,
            &preimage.public_transcript_inputs,
            &proof,
        )?;
        Ok(proof)
    }

    pub fn verify_public(
        &self,
        binding_input: Fr,
        communications_commitment: Fr,
        public_transcript_inputs: &[Fr],
        proof: &Proof,
    ) -> Result<(), CompactPresentationRuntimeError> {
        if binding_input != Fr::from(0_u64)
            || public_transcript_inputs.len() != EXPECTED_PUBLIC_TRANSCRIPT_FIELDS
        {
            return Err(CompactPresentationRuntimeError::InvalidProof);
        }
        let statement = std::iter::once(binding_input)
            .chain(std::iter::once(communications_commitment))
            .chain(public_transcript_inputs.iter().copied());
        let params = self.verifier_params()?;
        self.verifier_key
            .verify(&params, proof, statement)
            .map_err(|_| CompactPresentationRuntimeError::InvalidProof)
    }

    fn prover_params(&self) -> std::io::Result<ParamsProver> {
        let mut params = self
            .params_prover
            .lock()
            .map_err(|_| std::io::Error::other("prover parameter lock poisoned"))?;
        if let Some(params) = params.as_ref() {
            return Ok(params.clone());
        }
        let parsed = ParamsProver::read(self.parameter_bytes.as_slice())?;
        *params = Some(parsed.clone());
        Ok(parsed)
    }

    fn verifier_params(&self) -> Result<ParamsVerifier, CompactPresentationRuntimeError> {
        let mut params = self
            .params_verifier
            .lock()
            .map_err(|_| CompactPresentationRuntimeError::InvalidProof)?;
        if let Some(params) = params.as_ref() {
            return Ok(params.clone());
        }
        let parsed = ParamsVerifier::read(self.parameter_bytes.as_slice())
            .map_err(|_| CompactPresentationRuntimeError::InvalidProof)?;
        *params = Some(parsed.clone());
        Ok(parsed)
    }
}

impl ParamsProverProvider for NativeCompactPresentationRuntime {
    async fn get_params(&self, k: u8) -> std::io::Result<ParamsProver> {
        if k != EXPECTED_CIRCUIT_K {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unexpected Compact presentation circuit degree",
            ));
        }
        self.prover_params()
    }
}

impl Resolver for NativeCompactPresentationRuntime {
    async fn resolve_key(&self, key: KeyLocation) -> std::io::Result<Option<ProvingKeyMaterial>> {
        if key.0.as_ref() != PRESENTATION_CIRCUIT_KEY_LOCATION {
            return Ok(None);
        }
        Ok(Some(ProvingKeyMaterial {
            prover_key: self.prover_key.to_vec(),
            verifier_key: self.verifier_key_bytes.to_vec(),
            ir_source: self.ir_bytes.to_vec(),
        }))
    }
}

#[derive(Clone)]
enum RuntimeArtifactBytes {
    Owned(Arc<Vec<u8>>),
    #[cfg(feature = "mobile-compact-artifacts")]
    Embedded(&'static [u8]),
}

impl RuntimeArtifactBytes {
    fn owned(bytes: Vec<u8>) -> Self {
        Self::Owned(Arc::new(bytes))
    }

    #[cfg(feature = "mobile-compact-artifacts")]
    const fn embedded(bytes: &'static [u8]) -> Self {
        Self::Embedded(bytes)
    }

    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(bytes) => bytes.as_slice(),
            #[cfg(feature = "mobile-compact-artifacts")]
            Self::Embedded(bytes) => bytes,
        }
    }

    fn to_vec(&self) -> Vec<u8> {
        self.as_slice().to_vec()
    }
}

/// Authenticates the runtime-minimal Compact presentation closure embedded in
/// an explicit mobile measurement build.
///
/// The build must provide `OXID_PRESENTATION_ARTIFACTS_DIR` as an immutable Nix
/// store path. `include_bytes!` copies the reviewed prover, verifier, compiled
/// ZKIR, and p18 parameter bytes into the application binary, so runtime loading
/// performs no path discovery, mutable cache lookup, extraction, or network IO.
#[cfg(feature = "mobile-compact-artifacts")]
pub fn load_embedded_mobile_compact_presentation_runtime()
-> Result<NativeCompactPresentationRuntime, CompactPresentationRuntimeError> {
    const MANIFEST: &[u8] = include_bytes!(concat!(
        env!("OXID_PRESENTATION_ARTIFACTS_DIR"),
        "/manifest.json"
    ));
    const PROVER_BYTES: &[u8] = include_bytes!(concat!(
        env!("OXID_PRESENTATION_ARTIFACTS_DIR"),
        "/artifacts/keys/proveDigitalPassportPresentation.prover"
    ));
    const VERIFIER_BYTES: &[u8] = include_bytes!(concat!(
        env!("OXID_PRESENTATION_ARTIFACTS_DIR"),
        "/artifacts/keys/proveDigitalPassportPresentation.verifier"
    ));
    const IR_BYTES: &[u8] = include_bytes!(concat!(
        env!("OXID_PRESENTATION_ARTIFACTS_DIR"),
        "/artifacts/zkir/proveDigitalPassportPresentation.bzkir"
    ));
    const PARAMETER_BYTES: &[u8] = include_bytes!(concat!(
        env!("OXID_PRESENTATION_ARTIFACTS_DIR"),
        "/artifacts/params/bls_midnight_2p18"
    ));

    if u64::try_from(MANIFEST.len()).is_err()
        || MANIFEST.is_empty()
        || u64::try_from(MANIFEST.len()).is_ok_and(|length| length > MANIFEST_MAX_BYTES)
    {
        return Err(CompactPresentationRuntimeError::ArtifactMismatch);
    }
    let manifest: ArtifactManifest = serde_json::from_slice(MANIFEST)
        .map_err(|_| CompactPresentationRuntimeError::ArtifactMismatch)?;
    validate_manifest(&manifest)?;
    let embedded = [PROVER_BYTES, VERIFIER_BYTES, IR_BYTES, PARAMETER_BYTES];
    for (expected, bytes) in REQUIRED_ARTIFACTS.into_iter().zip(embedded) {
        validate_authenticated_artifact(&manifest, expected, bytes)?;
    }
    NativeCompactPresentationRuntime::from_authenticated_artifacts([
        RuntimeArtifactBytes::embedded(PROVER_BYTES),
        RuntimeArtifactBytes::embedded(VERIFIER_BYTES),
        RuntimeArtifactBytes::embedded(IR_BYTES),
        RuntimeArtifactBytes::embedded(PARAMETER_BYTES),
    ])
}

pub(crate) fn encode_zk_proof(proof: &Proof) -> Result<Vec<u8>, CompactPresentationRuntimeError> {
    let mut output = Vec::new();
    tagged_serialize(proof, &mut output)
        .map_err(|_| CompactPresentationRuntimeError::InvalidProof)?;
    Ok(output)
}

pub(crate) fn decode_zk_proof(bytes: &[u8]) -> Result<Proof, CompactPresentationRuntimeError> {
    let mut reader = bytes;
    let proof = tagged_deserialize(&mut reader)
        .map_err(|_| CompactPresentationRuntimeError::InvalidProof)?;
    if !reader.is_empty() {
        return Err(CompactPresentationRuntimeError::InvalidProof);
    }
    Ok(proof)
}

pub(crate) fn public_binding(
    preimage: &ProofPreimage,
) -> Result<(Fr, Fr), CompactPresentationRuntimeError> {
    preimage
        .communications_commitment
        .map(|(commitment, _)| (preimage.binding_input, commitment))
        .ok_or(CompactPresentationRuntimeError::InvalidPreimage)
}

pub(crate) struct PortableCompactPresentation {
    pub(crate) artifact_identity: [u8; 32],
    pub(crate) credential: Vec<u8>,
    pub(crate) issuer_proof: Vec<u8>,
    pub(crate) public_input: Vec<u8>,
    pub(crate) holder_proof: Vec<u8>,
    pub(crate) communications_commitment: Fr,
    pub(crate) proof: Proof,
}

pub(crate) fn encode_portable_presentation(
    presentation: &PortableCompactPresentation,
) -> Result<Vec<u8>, CompactPresentationRuntimeError> {
    let commitment = presentation.communications_commitment.as_le_bytes();
    let proof = encode_zk_proof(&presentation.proof)?;
    let chunks: [&[u8]; PORTABLE_CHUNKS as usize] = [
        &presentation.artifact_identity,
        &presentation.credential,
        &presentation.issuer_proof,
        &presentation.public_input,
        &presentation.holder_proof,
        &commitment,
        &proof,
    ];
    if presentation.credential.is_empty()
        || presentation.credential.len() > 1024 * 1024
        || presentation.issuer_proof.is_empty()
        || presentation.issuer_proof.len() > 1024 * 1024
        || presentation.public_input.len() != PORTABLE_PUBLIC_INPUT_BYTES
        || presentation.holder_proof.is_empty()
        || presentation.holder_proof.len() > 1024 * 1024
        || commitment.len() != 32
        || proof.is_empty()
        || proof.len() > PORTABLE_MAX_PROOF_BYTES
    {
        return Err(CompactPresentationRuntimeError::InvalidProof);
    }
    let mut output = Vec::new();
    output.extend_from_slice(PORTABLE_MAGIC);
    output.extend_from_slice(&PORTABLE_VERSION.to_be_bytes());
    output.extend_from_slice(&PORTABLE_CHUNKS.to_be_bytes());
    for chunk in chunks {
        let length = u32::try_from(chunk.len())
            .map_err(|_| CompactPresentationRuntimeError::InvalidProof)?;
        output.extend_from_slice(&length.to_be_bytes());
        output.extend_from_slice(chunk);
    }
    if output
        .len()
        .checked_add(32)
        .is_none_or(|length| length > PORTABLE_MAX_BYTES)
    {
        return Err(CompactPresentationRuntimeError::InvalidProof);
    }
    let checksum = Sha256::digest(&output);
    output.extend_from_slice(&checksum);
    Ok(output)
}

pub(crate) fn decode_portable_presentation(
    bytes: &[u8],
) -> Result<PortableCompactPresentation, CompactPresentationRuntimeError> {
    if bytes.len() > PORTABLE_MAX_BYTES
        || bytes.len() < 4 + 2 + 2 + PORTABLE_CHUNKS as usize * 4 + 32
        || &bytes[..4] != PORTABLE_MAGIC
        || u16::from_be_bytes(
            bytes[4..6]
                .try_into()
                .map_err(|_| CompactPresentationRuntimeError::InvalidProof)?,
        ) != PORTABLE_VERSION
        || u16::from_be_bytes(
            bytes[6..8]
                .try_into()
                .map_err(|_| CompactPresentationRuntimeError::InvalidProof)?,
        ) != PORTABLE_CHUNKS
    {
        return Err(CompactPresentationRuntimeError::InvalidProof);
    }
    let payload_end = bytes
        .len()
        .checked_sub(32)
        .ok_or(CompactPresentationRuntimeError::InvalidProof)?;
    if Sha256::digest(&bytes[..payload_end]).as_slice() != &bytes[payload_end..] {
        return Err(CompactPresentationRuntimeError::InvalidProof);
    }
    let mut offset: usize = 8;
    let mut chunks = Vec::with_capacity(PORTABLE_CHUNKS as usize);
    for _ in 0..PORTABLE_CHUNKS {
        let length_end = offset
            .checked_add(4)
            .filter(|end| *end <= payload_end)
            .ok_or(CompactPresentationRuntimeError::InvalidProof)?;
        let length = u32::from_be_bytes(
            bytes[offset..length_end]
                .try_into()
                .map_err(|_| CompactPresentationRuntimeError::InvalidProof)?,
        ) as usize;
        offset = length_end;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= payload_end)
            .ok_or(CompactPresentationRuntimeError::InvalidProof)?;
        chunks.push(&bytes[offset..end]);
        offset = end;
    }
    if offset != payload_end
        || chunks[0].len() != 32
        || chunks[1].is_empty()
        || chunks[1].len() > 1024 * 1024
        || chunks[2].is_empty()
        || chunks[2].len() > 1024 * 1024
        || chunks[3].len() != PORTABLE_PUBLIC_INPUT_BYTES
        || chunks[4].is_empty()
        || chunks[4].len() > 1024 * 1024
        || chunks[5].len() != 32
        || chunks[6].is_empty()
        || chunks[6].len() > PORTABLE_MAX_PROOF_BYTES
    {
        return Err(CompactPresentationRuntimeError::InvalidProof);
    }
    let artifact_identity = chunks[0]
        .try_into()
        .map_err(|_| CompactPresentationRuntimeError::InvalidProof)?;
    let communications_commitment =
        Fr::from_le_bytes(chunks[5]).ok_or(CompactPresentationRuntimeError::InvalidProof)?;
    let proof = decode_zk_proof(chunks[6])?;
    Ok(PortableCompactPresentation {
        artifact_identity,
        credential: chunks[1].to_vec(),
        issuer_proof: chunks[2].to_vec(),
        public_input: chunks[3].to_vec(),
        holder_proof: chunks[4].to_vec(),
        communications_commitment,
        proof,
    })
}

fn canonical_artifact_root(root: &Path) -> Result<PathBuf, CompactPresentationRuntimeError> {
    let canonical =
        fs::canonicalize(root).map_err(|_| CompactPresentationRuntimeError::ArtifactUnavailable)?;
    let metadata = fs::symlink_metadata(&canonical)
        .map_err(|_| CompactPresentationRuntimeError::ArtifactUnavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(CompactPresentationRuntimeError::InvalidConfiguration);
    }
    Ok(canonical)
}

fn read_regular_file(
    path: &Path,
    maximum_bytes: u64,
) -> Result<Vec<u8>, CompactPresentationRuntimeError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| CompactPresentationRuntimeError::ArtifactUnavailable)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > maximum_bytes
    {
        return Err(CompactPresentationRuntimeError::ArtifactMismatch);
    }
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| CompactPresentationRuntimeError::ArtifactMismatch)?;
    let file =
        fs::File::open(path).map_err(|_| CompactPresentationRuntimeError::ArtifactUnavailable)?;
    let mut reader = file.take(maximum_bytes.saturating_add(1));
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .read_to_end(&mut bytes)
        .map_err(|_| CompactPresentationRuntimeError::ArtifactUnavailable)?;
    if u64::try_from(bytes.len()).ok() != Some(metadata.len()) {
        return Err(CompactPresentationRuntimeError::ArtifactMismatch);
    }
    Ok(bytes)
}

fn read_artifact_file(
    artifact_root: &Path,
    relative_path: &str,
    maximum_bytes: u64,
) -> Result<Vec<u8>, CompactPresentationRuntimeError> {
    let components = Path::new(relative_path).components().collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CompactPresentationRuntimeError::ArtifactMismatch);
    }
    let mut current = artifact_root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| CompactPresentationRuntimeError::ArtifactUnavailable)?;
        if metadata.file_type().is_symlink()
            || (index + 1 == components.len() && !metadata.is_file())
            || (index + 1 != components.len() && !metadata.is_dir())
        {
            return Err(CompactPresentationRuntimeError::ArtifactMismatch);
        }
    }
    read_regular_file(&current, maximum_bytes)
}

fn artifact_identity() -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:compact-presentation-artifact-set:v1\0");
    digest.update(EXPECTED_ARTIFACT_SET.as_bytes());
    for artifact in REQUIRED_ARTIFACTS {
        digest.update(artifact.path.as_bytes());
        digest.update([0]);
        digest.update(artifact.sha256.as_bytes());
        digest.update(artifact.bytes.to_be_bytes());
    }
    digest.finalize().into()
}

fn validate_manifest(manifest: &ArtifactManifest) -> Result<(), CompactPresentationRuntimeError> {
    if manifest.schema_version != 1
        || manifest.artifact_set != EXPECTED_ARTIFACT_SET
        || manifest.source.oxid_contract_sha256 != EXPECTED_CONTRACT_SHA256
        || manifest.source.upstream_revision != EXPECTED_UPSTREAM_REVISION
        || manifest.toolchain.compact_cli_version != "0.5.1"
        || manifest.toolchain.compiler_version != "0.30.0"
        || manifest.toolchain.compiler_language_version != "0.22.0"
        || manifest.toolchain.generated_runtime_version != "0.15.0"
        || manifest.toolchain.toolchain_source_revision != EXPECTED_COMPILER_REVISION
        || manifest.toolchain.circuit_parameter != EXPECTED_PARAMETER_NAME
        || manifest.toolchain.circuit_parameter_sha256 != PARAMETERS.sha256
        || manifest.circuit.id != EXPECTED_CIRCUIT_ID
        || manifest.circuit.k != EXPECTED_CIRCUIT_K
        || manifest.circuit.rows != EXPECTED_CIRCUIT_ROWS
        || manifest.circuit.public_statement_domain != "oxid:midnight-compact-vp:v1"
    {
        return Err(CompactPresentationRuntimeError::ArtifactMismatch);
    }
    Ok(())
}

fn validate_authenticated_artifact(
    manifest: &ArtifactManifest,
    expected: ExpectedArtifact,
    bytes: &[u8],
) -> Result<(), CompactPresentationRuntimeError> {
    let declared = manifest
        .artifacts
        .iter()
        .find(|entry| entry.path == expected.path)
        .ok_or(CompactPresentationRuntimeError::ArtifactMismatch)?;
    if declared.bytes != expected.bytes
        || declared.sha256 != expected.sha256
        || u64::try_from(bytes.len()).ok() != Some(expected.bytes)
        || hex::encode(Sha256::digest(bytes)) != expected.sha256
    {
        return Err(CompactPresentationRuntimeError::ArtifactMismatch);
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
    circuit: CircuitManifest,
    artifacts: Vec<ArtifactEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceManifest {
    oxid_contract_sha256: String,
    upstream_revision: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolchainManifest {
    compact_cli_version: String,
    compiler_version: String,
    compiler_language_version: String,
    generated_runtime_version: String,
    toolchain_source_revision: String,
    circuit_parameter: String,
    circuit_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CircuitManifest {
    id: String,
    k: u8,
    rows: u64,
    public_statement_domain: String,
}

#[derive(Deserialize)]
struct ArtifactEntry {
    path: String,
    bytes: u64,
    sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_requires_an_absolute_normalized_root() {
        assert_eq!(
            CompactPresentationArtifactsConfig::new("relative/artifacts"),
            Err(CompactPresentationRuntimeError::InvalidConfiguration)
        );
        assert_eq!(
            CompactPresentationArtifactsConfig::new("/tmp/../artifacts"),
            Err(CompactPresentationRuntimeError::InvalidConfiguration)
        );
        assert!(CompactPresentationArtifactsConfig::new("/tmp/artifacts").is_ok());
    }

    #[cfg(feature = "mobile-compact-artifacts")]
    #[test]
    fn embedded_mobile_package_authenticates_without_runtime_discovery() {
        let runtime = load_embedded_mobile_compact_presentation_runtime()
            .expect("the Nix-selected embedded artifact package authenticates");
        assert_eq!(runtime.identity(), artifact_identity());
    }

    #[test]
    fn proof_codec_rejects_trailing_data() {
        let proof = Proof(vec![1, 2, 3]);
        let mut encoded = encode_zk_proof(&proof).expect("proof encodes");
        assert_eq!(decode_zk_proof(&encoded).expect("proof decodes"), proof);
        encoded.push(0);
        assert_eq!(
            decode_zk_proof(&encoded),
            Err(CompactPresentationRuntimeError::InvalidProof)
        );
    }

    #[test]
    fn portable_presentation_round_trips_and_authenticates_its_container() {
        let portable = PortableCompactPresentation {
            artifact_identity: [0x11; 32],
            credential: vec![0x22; 96],
            issuer_proof: vec![0x33; 96],
            public_input: vec![0x44; PORTABLE_PUBLIC_INPUT_BYTES],
            holder_proof: vec![0x55; 96],
            communications_commitment: Fr::from(7_u64),
            proof: Proof(vec![0x66; 128]),
        };
        let encoded = encode_portable_presentation(&portable).expect("portable presentation");
        let decoded = decode_portable_presentation(&encoded).expect("round trip");
        assert_eq!(decoded.artifact_identity, portable.artifact_identity);
        assert_eq!(decoded.credential, portable.credential);
        assert_eq!(decoded.issuer_proof, portable.issuer_proof);
        assert_eq!(decoded.public_input, portable.public_input);
        assert_eq!(decoded.holder_proof, portable.holder_proof);
        assert_eq!(
            decoded.communications_commitment,
            portable.communications_commitment
        );
        assert_eq!(decoded.proof, portable.proof);

        let mut tampered = encoded;
        tampered[8] ^= 1;
        assert!(matches!(
            decode_portable_presentation(&tampered),
            Err(CompactPresentationRuntimeError::InvalidProof)
        ));
    }
}
