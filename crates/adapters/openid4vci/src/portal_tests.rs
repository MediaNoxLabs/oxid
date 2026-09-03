// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use base64::{Engine as _, engine::general_purpose};
use sha2::{Digest as _, Sha256};

use super::*;

const PROFILE_FIXTURE_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../fixtures/laceid-portal/76e8edf394a4cb37ca822037272d543c68f25f71/openid4vci-final"
);
const SOURCE_LOCK_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../fixtures/laceid-portal/25499870f84d77173c46e4af3021311decfb840b"
);

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn manifest_bytes(origin: &str) -> Vec<u8> {
    let x = general_purpose::URL_SAFE_NO_PAD.encode([7_u8; 32]);
    let y = general_purpose::URL_SAFE_NO_PAD.encode([9_u8; 32]);
    let jwk = format!(r#"{{"crv":"Jubjub","kty":"EC","x":"{x}","y":"{y}"}}"#);
    let jwk_digest = sha256(jwk.as_bytes());
    format!(
        concat!(
            r#"{{"integrationCommit":"25499870f84d77173c46e4af3021311decfb840b","integrationTree":"2d845d2293603dfd8adce5362c8a9941e6ba78a9","issuerDid":"did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","issuerJubjubJwk":{jwk},"issuerJubjubJwkSha256":"{jwk_digest}","issuerMethod":"did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef#key-assert","issuerOrigin":"{origin}","issuerResolverOrigin":"{origin}","provenanceSha256":"63d2dd182f1a315d8fe7677ae6481aecebd2fd9cff709cc438b6c0261a3cf4c7","schema":"oxid-portal-deployment-v3"}}"#
        ),
        jwk = jwk,
        jwk_digest = jwk_digest,
        origin = origin,
    )
    .into_bytes()
}

#[test]
fn exact_portal_source_lock_and_all_profile_fixtures_authenticate() {
    authenticate_bundled_portal_source().expect("exact checked-in Portal source must authenticate");
    let source_lock = fs::read(format!("{SOURCE_LOCK_ROOT}/source-lock.json"))
        .expect("self-contained source lock");
    let source_lock: serde_json::Value =
        serde_json::from_slice(&source_lock).expect("source-lock JSON");
    assert_eq!(
        source_lock["profileSourceCommit"],
        PORTAL_PROFILE_SOURCE_COMMIT
    );
    assert_eq!(
        source_lock["provenancePath"],
        "openid4vci-final/provenance.json"
    );
    let bundled_provenance = fs::read(format!(
        "{SOURCE_LOCK_ROOT}/openid4vci-final/provenance.json"
    ))
    .expect("self-contained provenance");
    assert_eq!(sha256(&bundled_provenance), PORTAL_PROVENANCE_SHA256);

    let provenance = fs::read(format!("{PROFILE_FIXTURE_ROOT}/provenance.json")).expect("manifest");
    assert_eq!(
        sha256(&provenance),
        "cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"
    );

    let value: serde_json::Value = serde_json::from_slice(&provenance).expect("provenance JSON");
    for fixture in value["fixtures"].as_array().expect("fixture list") {
        let upstream = fixture["path"].as_str().expect("path");
        let relative = upstream
            .strip_prefix("crates/issuer-integration/fixtures/openid4vci-final/")
            .expect("profile-relative path");
        let bytes = fs::read(format!("{PROFILE_FIXTURE_ROOT}/{relative}"))
            .unwrap_or_else(|_| panic!("missing vendored fixture {relative}"));
        assert_eq!(
            sha256(&bytes),
            fixture["sha256"],
            "fixture drift: {relative}"
        );
    }
}

#[test]
fn deployment_manifest_requires_exact_digest_source_profile_and_canonical_public_facts() {
    let bytes = manifest_bytes("http://127.0.0.1:32191");
    let manifest = PortalDeploymentManifest::from_bytes(&bytes, &sha256(&bytes))
        .expect("exact manifest should authenticate");
    assert_eq!(manifest.issuer_origin(), "http://127.0.0.1:32191");
    assert_eq!(manifest.issuer_jubjub_jwk().curve, "Jubjub");

    let mut drifted = bytes.clone();
    *drifted.last_mut().expect("bytes") = b' ';
    assert_eq!(
        PortalDeploymentManifest::from_bytes(&drifted, &sha256(&drifted)).err(),
        Some(PortalDeploymentManifestError::InvalidManifest)
    );
    assert_eq!(
        PortalDeploymentManifest::from_bytes(&bytes, &"0".repeat(64)).err(),
        Some(PortalDeploymentManifestError::DigestMismatch)
    );

    for replacement in [
        (
            "25499870f84d77173c46e4af3021311decfb840b",
            "a25ec8d04882eabd4ac7b784c70fc2f0c152faae",
        ),
        (
            "2d845d2293603dfd8adce5362c8a9941e6ba78a9",
            "68b4597524f88a0ae2253439a44dab0dc60cbb6f",
        ),
        (
            "63d2dd182f1a315d8fe7677ae6481aecebd2fd9cff709cc438b6c0261a3cf4c7",
            "df86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87",
        ),
    ] {
        let drifted = String::from_utf8(bytes.clone())
            .expect("utf8")
            .replace(replacement.0, replacement.1)
            .into_bytes();
        assert_eq!(
            PortalDeploymentManifest::from_bytes(&drifted, &sha256(&drifted)).err(),
            Some(PortalDeploymentManifestError::SourceLockMismatch)
        );
    }
}

#[test]
fn deployment_manifest_rejects_hostile_origins_jwk_drift_and_duplicate_fields() {
    let valid = manifest_bytes("https://issuer.example");
    for hostile in [
        String::from_utf8(valid.clone())
            .expect("utf8")
            .replace("https://issuer.example", "http://issuer.example"),
        String::from_utf8(valid.clone())
            .expect("utf8")
            .replace("https://issuer.example", "https://user:pass@issuer.example"),
        String::from_utf8(valid.clone())
            .expect("utf8")
            .replace("https://issuer.example", "https://issuer.example/path"),
        String::from_utf8(valid.clone()).expect("utf8").replace(
            "\"schema\":",
            "\"schema\":\"oxid-portal-deployment-v3\",\"schema\":",
        ),
        String::from_utf8(valid.clone())
            .expect("utf8")
            .replace("\"x\":\"", "\"x\":\"A"),
    ] {
        let hostile = hostile.into_bytes();
        assert!(PortalDeploymentManifest::from_bytes(&hostile, &sha256(&hostile)).is_err());
    }
}

#[test]
fn deployment_file_lock_rejects_symlink_nonregular_oversized_and_digest_drift() {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let root = std::env::temp_dir().join(format!(
        "oxid-portal-manifest-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).expect("temp directory");
    let path = root.join("deployment.json");
    let bytes = manifest_bytes("http://localhost:32191");
    fs::write(&path, &bytes).expect("manifest");
    PortalDeploymentManifest::from_file(&path, &sha256(&bytes)).expect("regular exact file");
    assert_eq!(
        PortalDeploymentManifest::from_file(&path, &"f".repeat(64)).err(),
        Some(PortalDeploymentManifestError::DigestMismatch)
    );
    let directory = root.join("directory");
    fs::create_dir(&directory).expect("directory");
    assert_eq!(
        PortalDeploymentManifest::from_file(&directory, &sha256(&bytes)).err(),
        Some(PortalDeploymentManifestError::InvalidFile)
    );
    let oversized = root.join("oversized");
    fs::write(&oversized, vec![b' '; 65_537]).expect("oversized");
    assert_eq!(
        PortalDeploymentManifest::from_file(&oversized, &sha256(&bytes)).err(),
        Some(PortalDeploymentManifestError::InvalidFile)
    );
    #[cfg(unix)]
    {
        let symlink = root.join("symlink");
        std::os::unix::fs::symlink(&path, &symlink).expect("symlink");
        assert_eq!(
            PortalDeploymentManifest::from_file(&symlink, &sha256(&bytes)).err(),
            Some(PortalDeploymentManifestError::InvalidFile)
        );
    }
    fs::remove_dir_all(root).expect("cleanup");
}
