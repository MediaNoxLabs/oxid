// SPDX-License-Identifier: Apache-2.0

//! Authenticated Midnight DID 0.4.0 Compact artifacts.
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
const EXPECTED_VERSION: &str = "0.4.0";
const EXPECTED_GIT_SHA: &str = "cf00aacb3e1bb300e87bc4dd11ec0897fab6e233";
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
    root: PathBuf,
    manifest: ArtifactManifest,
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
        let manifest: ArtifactManifest = serde_json::from_slice(&bytes)
            .map_err(|_| MidnightDidCompactArtifactError::ArtifactMismatch)?;
        if manifest.schema != EXPECTED_SCHEMA
            || manifest.version != EXPECTED_VERSION
            || manifest.git_sha != EXPECTED_GIT_SHA
        {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        for circuit in [
            MidnightDidBootstrapCircuit::VerificationMethod,
            MidnightDidBootstrapCircuit::SchnorrJubjubVerificationMethod,
            MidnightDidBootstrapCircuit::VerificationMethodRelation,
        ] {
            let entry = manifest
                .circuits
                .iter()
                .find(|entry| entry.id == circuit.id())
                .ok_or(MidnightDidCompactArtifactError::ArtifactMismatch)?;
            validate_relative_path(&entry.files.verifier)?;
            validate_relative_path(&entry.files.prover)?;
            validate_relative_path(&entry.files.zkir)?;
            validate_lower_hex_32(&entry.sha256.verifier)?;
            validate_lower_hex_32(&entry.sha256.prover)?;
            validate_lower_hex_32(&entry.sha256.zkir)?;
        }
        Ok(Self { root, manifest })
    }

    pub fn verifier_key(
        &self,
        circuit: MidnightDidBootstrapCircuit,
    ) -> Result<Zeroizing<Vec<u8>>, MidnightDidCompactArtifactError> {
        let entry = self
            .manifest
            .circuits
            .iter()
            .find(|entry| entry.id == circuit.id())
            .ok_or(MidnightDidCompactArtifactError::ArtifactMismatch)?;
        let path = self.root.join(&entry.files.verifier);
        let canonical = fs::canonicalize(&path)
            .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
        if !canonical.starts_with(&self.root) {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        let metadata = fs::metadata(&canonical)
            .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_VERIFIER_KEY_BYTES {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        let bytes = Zeroizing::new(
            fs::read(canonical)
                .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?,
        );
        if hex::encode(Sha256::digest(bytes.as_slice())) != entry.sha256.verifier {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        Ok(bytes)
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
            .find(|entry| entry.id == circuit.id())
            .ok_or(MidnightDidCompactArtifactError::ArtifactMismatch)?;
        Ok(ProvingKeyMaterial {
            prover_key: self.authenticated_file(
                &entry.files.prover,
                &entry.sha256.prover,
                MAX_PROVER_KEY_BYTES,
            )?,
            verifier_key: self.authenticated_file(
                &entry.files.verifier,
                &entry.sha256.verifier,
                MAX_VERIFIER_KEY_BYTES,
            )?,
            ir_source: self.authenticated_file(
                &entry.files.zkir,
                &entry.sha256.zkir,
                MAX_ZKIR_BYTES,
            )?,
        })
    }

    fn authenticated_file(
        &self,
        relative: &str,
        expected_sha256: &str,
        maximum_bytes: u64,
    ) -> Result<Vec<u8>, MidnightDidCompactArtifactError> {
        let canonical = fs::canonicalize(self.root.join(relative))
            .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
        if !canonical.starts_with(&self.root) {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        let metadata = fs::metadata(&canonical)
            .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum_bytes {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        let bytes = fs::read(canonical)
            .map_err(|_| MidnightDidCompactArtifactError::ArtifactUnavailable)?;
        if hex::encode(Sha256::digest(&bytes)) != expected_sha256 {
            return Err(MidnightDidCompactArtifactError::ArtifactMismatch);
        }
        Ok(bytes)
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
}
