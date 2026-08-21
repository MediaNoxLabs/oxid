// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{env, fs, path::PathBuf};

use sha2::{Digest as _, Sha256};

const PORTAL_MANIFEST_PATH_ENV: &str = "OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH";
const PORTAL_MANIFEST_SHA256_ENV: &str = "OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256";
const MAX_PORTAL_MANIFEST_BYTES: usize = 65_536;

fn main() {
    let manifest_directory = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo must provide CARGO_MANIFEST_DIR"),
    );
    let repository_root = manifest_directory
        .parent()
        .and_then(|apps| apps.parent())
        .expect("oxid app must remain under apps/");
    let brand_directory = repository_root.join("brands/oxid");
    let output_directory =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR"));
    let brand = oxid_brand_build::load_brand_pack(&brand_directory)
        .unwrap_or_else(|error| panic!("default brand pack is invalid: {error}"));
    oxid_brand_build::validate_app_manifest(&brand, manifest_directory.join("Dioxus.toml"))
        .unwrap_or_else(|error| panic!("thin app manifest does not match its brand: {error}"));
    oxid_brand_build::generate_brand(&brand_directory, output_directory.clone())
        .unwrap_or_else(|error| panic!("default brand generation failed: {error}"));
    embed_portal_manifest(&output_directory);

    println!("cargo:rerun-if-changed={}", brand_directory.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_directory.join("Dioxus.toml").display()
    );
}

fn embed_portal_manifest(output_directory: &std::path::Path) {
    println!("cargo:rerun-if-env-changed={PORTAL_MANIFEST_PATH_ENV}");
    println!("cargo:rerun-if-env-changed={PORTAL_MANIFEST_SHA256_ENV}");
    let mobile_target = env::var("CARGO_CFG_TARGET_OS")
        .is_ok_and(|target| matches!(target.as_str(), "ios" | "android"));
    let wasm_target = env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|target| target == "wasm32");
    if env::var_os("CARGO_FEATURE_STANDALONE_PORTAL").is_none() || !mobile_target || wasm_target {
        return;
    }

    let path = PathBuf::from(
        env::var_os(PORTAL_MANIFEST_PATH_ENV)
            .unwrap_or_else(|| panic!("standalone-portal requires {PORTAL_MANIFEST_PATH_ENV}")),
    );
    assert!(
        path.is_absolute(),
        "{PORTAL_MANIFEST_PATH_ENV} must be absolute"
    );
    let metadata = fs::symlink_metadata(&path)
        .unwrap_or_else(|_| panic!("standalone-portal manifest must be a readable regular file"));
    assert!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "standalone-portal manifest must be a regular non-symlink file"
    );
    let bytes = fs::read(&path)
        .unwrap_or_else(|_| panic!("standalone-portal manifest must be a readable regular file"));
    assert!(
        !bytes.is_empty() && bytes.len() <= MAX_PORTAL_MANIFEST_BYTES,
        "standalone-portal manifest has an invalid size"
    );
    let digest = env::var(PORTAL_MANIFEST_SHA256_ENV)
        .unwrap_or_else(|_| panic!("standalone-portal requires {PORTAL_MANIFEST_SHA256_ENV}"));
    assert!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value)),
        "standalone-portal manifest digest must be lowercase SHA-256"
    );
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(actual, digest, "standalone-portal manifest digest mismatch");

    fs::write(output_directory.join("portal-deployment.json"), bytes)
        .expect("standalone-portal manifest must embed into OUT_DIR");
    println!("cargo:rustc-env=OXID_EMBEDDED_PORTAL_DEPLOYMENT_SHA256={digest}");
    println!("cargo:rerun-if-changed={}", path.display());
}
