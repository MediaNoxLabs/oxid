// SPDX-License-Identifier: Apache-2.0

//! Authenticated Midnight DID 0.5.0 Compact artifacts.
//!
//! The release manifest is part of the trust boundary: callers select one of
//! the three bootstrap circuits by enum, and this loader verifies the pinned
//! release/source identity plus each verifier-key digest before returning
//! bytes to the maintenance composer.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use midnight_transient_crypto::proofs::ProvingKeyMaterial;

const EXPECTED_SCHEMA: &str = "midnight-did-zk-artifacts";
const EXPECTED_VERSION: &str = "0.5.0";
const EXPECTED_GIT_SHA: &str = "a14267cec3c1ab7e00bb0f058a54267d913a321b";
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_VERIFIER_KEY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PROVER_KEY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ZKIR_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightDidBootstrapCircuit {
    VerificationMethod,
    SchnorrJubjubVerificationMethod,
    VerificationMethodRelation,
}

impl MidnightDidBootstrapCircuit {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::VerificationMethod => "set_verification_method",
            Self::SchnorrJubjubVerificationMethod => "set_schnorr_jubjub_verification_method",
            Self::VerificationMethodRelation => "set_verification_method_relation",
        }
    }

    #[must_use]
    pub const fn artifact_id(self) -> &'static str {
        match self {
            Self::VerificationMethod => "setVerificationMethod",
            Self::SchnorrJubjubVerificationMethod => "setSchnorrJubjubVerificationMethod",
            Self::VerificationMethodRelation => "setVerificationMethodRelation",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidnightDidCompactArtifactError {
    InvalidConfiguration,
    ArtifactUnavailable,
    ArtifactMismatch,
}

#[derive(Clone, Debug)]
pub struct MidnightDidCompactArtifacts {
    source: ArtifactSource,
    manifest: ArtifactManifest,
}

#[derive(Clone, Debug)]
enum ArtifactSource {
    Directory(PathBuf),
    #[cfg(all(
        feature = "mobile-compact-artifacts",
        any(target_os = "ios", target_os = "android")
    ))]
    Embedded,
}

#[derive(Clone, Copy)]
enum ArtifactKind {
    Prover,
    Verifier,
    Zkir,
}

impl MidnightDidCompactArtifacts {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, MidnightDidCompactArtifactError> {
        let root = canonical_root(root.as_ref())?;
        let manifest_path = root.join("manifest.json");
        let metadata = fs::metadata(&manifest_path)
            .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_MANIFEST_BYTES {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        let bytes = fs::read(&manifest_path)
            .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
        let manifest = parse_manifest(&bytes)?;
        Ok(Self {
            source: ArtifactSource::Directory(root),
            manifest,
        })
    }

    /// Loads the reviewed runtime-minimal DID bootstrap closure embedded in an
    /// explicit mobile Portal build. No mutable path or runtime extraction is
    /// involved; each byte slice is still authenticated against the release
    /// manifest before the adapter becomes available.
    #[cfg(all(
        feature = "mobile-compact-artifacts",
        any(target_os = "ios", target_os = "android")
    ))]
    pub fn load_embedded_mobile() -> Result<Self, MidnightDidCompactArtifactError> {
        const MANIFEST: &[u8] = include_bytes!(concat!(
            env!("OXID_MIDNIGHT_DID_ARTIFACTS_DIR"),
            "/manifest.json"
        ));
        let manifest = parse_manifest(MANIFEST)?;
        let artifacts = Self {
            source: ArtifactSource::Embedded,
            manifest,
        };
        for circuit in bootstrap_circuits() {
            artifacts.authenticated_bytes(circuit, ArtifactKind::Prover)?;
            artifacts.authenticated_bytes(circuit, ArtifactKind::Verifier)?;
            artifacts.authenticated_bytes(circuit, ArtifactKind::Zkir)?;
        }
        Ok(artifacts)
    }

    pub fn verifier_key(
        &self,
        circuit: MidnightDidBootstrapCircuit,
    ) -> Result<Zeroizing<Vec<u8>>, MidnightDidCompactArtifactError> {
        let entry = self
            .manifest
            .circuits
            .iter()
            .find(|entry| entry.id == circuit.artifact_id())
            .ok_or(MidnightDidCompactArtifactError::ArtifactMismatch)?;
        Ok(Zeroizing::new(self.authenticated_file(
            circuit,
            ArtifactKind::Verifier,
            &entry.files.verifier,
            &entry.sha256.verifier,
            MAX_VERIFIER_KEY_BYTES,
        )?))
    }

    /// Returns the authenticated public proving closure for one admitted DID
    /// bootstrap circuit. Callers select circuits through the closed enum;
    /// manifest paths never become an ambient filesystem resolver.
    pub fn proving_key_material(
        &self,
        circuit: MidnightDidBootstrapCircuit,
    ) -> Result<ProvingKeyMaterial, MidnightDidCompactArtifactError> {
        let entry = self
            .manifest
            .circuits
            .iter()
            .find(|entry| entry.id == circuit.artifact_id())
            .ok_or(MidnightDidCompactArtifactError::ArtifactMismatch)?;
        Ok(ProvingKeyMaterial {
            prover_key: self.authenticated_file(
                circuit,
                ArtifactKind::Prover,
                &entry.files.prover,
                &entry.sha256.prover,
                MAX_PROVER_KEY_BYTES,
            )?,
            verifier_key: self.authenticated_file(
                circuit,
                ArtifactKind::Verifier,
                &entry.files.verifier,
                &entry.sha256.verifier,
                MAX_VERIFIER_KEY_BYTES,
            )?,
            ir_source: self.authenticated_file(
                circuit,
                ArtifactKind::Zkir,
                &entry.files.zkir,
                &entry.sha256.zkir,
                MAX_ZKIR_BYTES,
            )?,
        })
    }

    fn authenticated_file(
        &self,
        _circuit: MidnightDidBootstrapCircuit,
        _kind: ArtifactKind,
        relative: &str,
        expected_sha256: &str,
        maximum_bytes: u64,
    ) -> Result<Vec<u8>, MidnightDidCompactArtifactError> {
        let bytes = match &self.source {
            ArtifactSource::Directory(root) => {
                let canonical = fs::canonicalize(root.join(relative))
                    .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
                if !canonical.starts_with(root) {
                    return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
                }
                let metadata = fs::metadata(&canonical)
                    .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
                if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum_bytes {
                    return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
                }
                fs::read(canonical)
                    .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?
            }
            #[cfg(all(
                feature = "mobile-compact-artifacts",
                any(target_os = "ios", target_os = "android")
            ))]
            ArtifactSource::Embedded => self.authenticated_bytes(_circuit, _kind)?.to_vec(),
        };
        if bytes.is_empty()
            || u64::try_from(bytes.len()).map_or(true, |length| length > maximum_bytes)
        {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        if hex::encode(Sha256::digest(&bytes)) != expected_sha256 {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        Ok(bytes)
    }

    #[cfg(all(
        feature = "mobile-compact-artifacts",
        any(target_os = "ios", target_os = "android")
    ))]
    fn authenticated_bytes(
        &self,
        circuit: MidnightDidBootstrapCircuit,
        kind: ArtifactKind,
    ) -> Result<&'static [u8], MidnightDidCompactArtifactError> {
        let entry = self
            .manifest
            .circuits
            .iter()
            .find(|entry| entry.id == circuit.artifact_id())
            .ok_or(MidnightDidCompactArtifactError::ArtifactMismatch)?;
        let (bytes, expected, maximum) = match kind {
            ArtifactKind::Prover => (
                embedded_bytes(circuit, kind),
                entry.sha256.prover.as_str(),
                MAX_PROVER_KEY_BYTES,
            ),
            ArtifactKind::Verifier => (
                embedded_bytes(circuit, kind),
                entry.sha256.verifier.as_str(),
                MAX_VERIFIER_KEY_BYTES,
            ),
            ArtifactKind::Zkir => (
                embedded_bytes(circuit, kind),
                entry.sha256.zkir.as_str(),
                MAX_ZKIR_BYTES,
            ),
        };
        if bytes.is_empty()
            || u64::try_from(bytes.len()).map_or(true, |length| length > maximum)
            || hex::encode(Sha256::digest(bytes)) != expected
        {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        Ok(bytes)
    }
}

fn bootstrap_circuits() -> [MidnightDidBootstrapCircuit; 3] {
    [
        MidnightDidBootstrapCircuit::VerificationMethod,
        MidnightDidBootstrapCircuit::SchnorrJubjubVerificationMethod,
        MidnightDidBootstrapCircuit::VerificationMethodRelation,
    ]
}

fn parse_manifest(bytes: &[u8]) -> Result<ArtifactManifest, MidnightDidCompactArtifactError> {
    if bytes.is_empty()
        || u64::try_from(bytes.len()).map_or(true, |length| length > MAX_MANIFEST_BYTES)
    {
        return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
    }
    let manifest: ArtifactManifest = serde_json::from_slice(bytes)
        .map_err(|_| MidnightDidCompactArtifactError::ArtifactMismatch)?;
    if manifest.schema != EXPECTED_SCHEMA
        || manifest.version != EXPECTED_VERSION
        || manifest.git_sha != EXPECTED_GIT_SHA
    {
        return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
    }
    for circuit in bootstrap_circuits() {
        let entry = manifest
            .circuits
            .iter()
            .find(|entry| entry.id == circuit.artifact_id())
            .ok_or(MidnightDidCompactArtifactError::ArtifactMismatch)?;
        validate_relative_path(&entry.files.verifier)?;
        validate_relative_path(&entry.files.prover)?;
        validate_relative_path(&entry.files.zkir)?;
        validate_lower_hex_32(&entry.sha256.verifier)?;
        validate_lower_hex_32(&entry.sha256.prover)?;
        validate_lower_hex_32(&entry.sha256.zkir)?;
    }
    Ok(manifest)
}

#[cfg(all(
    feature = "mobile-compact-artifacts",
    any(target_os = "ios", target_os = "android")
))]
fn embedded_bytes(circuit: MidnightDidBootstrapCircuit, kind: ArtifactKind) -> &'static [u8] {
    macro_rules! artifact {
        ($path:literal) => {
            include_bytes!(concat!(env!("OXID_MIDNIGHT_DID_ARTIFACTS_DIR"), $path)).as_slice()
        };
    }
    match (circuit, kind) {
        (MidnightDidBootstrapCircuit::VerificationMethod, ArtifactKind::Prover) => {
            artifact!("/keys/setVerificationMethod.prover")
        }
        (MidnightDidBootstrapCircuit::VerificationMethod, ArtifactKind::Verifier) => {
            artifact!("/keys/setVerificationMethod.verifier")
        }
        (MidnightDidBootstrapCircuit::VerificationMethod, ArtifactKind::Zkir) => {
            artifact!("/zkir/setVerificationMethod.bzkir")
        }
        (MidnightDidBootstrapCircuit::SchnorrJubjubVerificationMethod, ArtifactKind::Prover) => {
            artifact!("/keys/setSchnorrJubjubVerificationMethod.prover")
        }
        (MidnightDidBootstrapCircuit::SchnorrJubjubVerificationMethod, ArtifactKind::Verifier) => {
            artifact!("/keys/setSchnorrJubjubVerificationMethod.verifier")
        }
        (MidnightDidBootstrapCircuit::SchnorrJubjubVerificationMethod, ArtifactKind::Zkir) => {
            artifact!("/zkir/setSchnorrJubjubVerificationMethod.bzkir")
        }
        (MidnightDidBootstrapCircuit::VerificationMethodRelation, ArtifactKind::Prover) => {
            artifact!("/keys/setVerificationMethodRelation.prover")
        }
        (MidnightDidBootstrapCircuit::VerificationMethodRelation, ArtifactKind::Verifier) => {
            artifact!("/keys/setVerificationMethodRelation.verifier")
        }
        (MidnightDidBootstrapCircuit::VerificationMethodRelation, ArtifactKind::Zkir) => {
            artifact!("/zkir/setVerificationMethodRelation.bzkir")
        }
    }
}

fn canonical_root(root: &Path) -> Result<PathBuf, MidnightDidCompactArtifactError> {
    if !root.is_absolute()
        || root
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(MidnightDidCompactArtifactError::InvalidConfiguration);
    }
    let canonical =
        fs::canonicalize(root).map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
    if !canonical.is_dir() {
        return Err(MidnightDidCompactArtifactError::InvalidConfiguration);
    }
    Ok(canonical)
}

fn validate_relative_path(value: &str) -> Result<(), MidnightDidCompactArtifactError> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
    }
    Ok(())
}

fn validate_lower_hex_32(value: &str) -> Result<(), MidnightDidCompactArtifactError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(MidnightDidCompactArtifactError::ArtifactMismatch)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactManifest {
    schema: String,
    version: String,
    git_sha: String,
    circuits: Vec<CircuitManifest>,
}

#[derive(Clone, Debug, Deserialize)]
struct CircuitManifest {
    id: String,
    files: CircuitFiles,
    sha256: CircuitDigests,
}

#[derive(Clone, Debug, Deserialize)]
struct CircuitFiles {
    prover: String,
    verifier: String,
    zkir: String,
}

#[derive(Clone, Debug, Deserialize)]
struct CircuitDigests {
    prover: String,
    verifier: String,
    zkir: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_roots() {
        assert_eq!(
            MidnightDidCompactArtifacts::load("relative/artifacts").unwrap_err(),
            MidnightDidCompactArtifactError::InvalidConfiguration
        );
    }

    #[test]
    #[ignore = "midnight-did-conformance: requires Nix-packaged DID artifacts"]
    fn pinned_release_artifacts_authenticate_when_configured() {
        let Some(root) = std::env::var_os("OXID_MIDNIGHT_DID_ARTIFACTS_DIR") else {
            return;
        };
        let artifacts = MidnightDidCompactArtifacts::load(root).expect("authenticated artifacts");
        for circuit in [
            MidnightDidBootstrapCircuit::VerificationMethod,
            MidnightDidBootstrapCircuit::SchnorrJubjubVerificationMethod,
            MidnightDidBootstrapCircuit::VerificationMethodRelation,
        ] {
            assert!(
                !artifacts
                    .verifier_key(circuit)
                    .expect("verifier key")
                    .is_empty()
            );
            let material = artifacts
                .proving_key_material(circuit)
                .expect("proving key material");
            assert!(!material.prover_key.is_empty());
            assert!(!material.verifier_key.is_empty());
            assert!(!material.ir_source.is_empty());
        }
    }

    #[cfg(all(
        feature = "mobile-compact-artifacts",
        any(target_os = "ios", target_os = "android")
    ))]
    #[test]
    fn embedded_mobile_bootstrap_closure_authenticates() {
        let artifacts =
            MidnightDidCompactArtifacts::load_embedded_mobile().expect("embedded artifacts");
        for circuit in bootstrap_circuits() {
            assert!(
                !artifacts
                    .verifier_key(circuit)
                    .expect("verifier")
                    .is_empty()
            );
            let material = artifacts
                .proving_key_material(circuit)
                .expect("proving closure");
            assert!(!material.prover_key.is_empty());
            assert!(!material.verifier_key.is_empty());
            assert!(!material.ir_source.is_empty());
        }
    }
}
