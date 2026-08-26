// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{env, fs, path::PathBuf};

use sha2::{Digest as _, Sha256};

#[path = "src/portal_profile_authority.rs"]
mod portal_profile_authority;

const PORTAL_MANIFEST_PATH_ENV: &str = "OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH";
const PORTAL_MANIFEST_SHA256_ENV: &str = "OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256";
const MAX_PORTAL_MANIFEST_BYTES: usize = 65_536;
const PORTAL_PROFILE_AUTHORITY_PATH_ENV: &str = "OXID_BUILD_PORTAL_PROFILE_AUTHORITY_PATH";
const PORTAL_PROFILE_AUTHORITY_SHA256_ENV: &str = "OXID_BUILD_PORTAL_PROFILE_AUTHORITY_SHA256";
const PORTAL_PUBLIC_ORIGIN_ENV: &str = "OXID_BUILD_PORTAL_PUBLIC_ORIGIN";
const MAX_PORTAL_PROFILE_AUTHORITY_BYTES: usize = 512;

fn active_portal_profile() -> Option<portal_profile_authority::PortalProfile> {
    let local = env::var_os("CARGO_FEATURE_STANDALONE_PORTAL").is_some();
    let tailnet = env::var_os("CARGO_FEATURE_STANDALONE_PORTAL_TAILNET").is_some();
    assert!(
        usize::from(local) + usize::from(tailnet) <= 1,
        "select exactly one standalone Portal profile"
    );
    if local {
        Some(portal_profile_authority::PortalProfile::Local)
    } else if tailnet {
        Some(portal_profile_authority::PortalProfile::AndroidTailnet)
    } else {
        None
    }
}

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
    authorize_portal_profile();
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

fn authorize_portal_profile() {
    println!("cargo:rustc-check-cfg=cfg(oxid_portal_virtual_device_profile_authorized)");
    println!("cargo:rustc-check-cfg=cfg(oxid_portal_android_physical_profile_authorized)");
    println!("cargo:rerun-if-env-changed={PORTAL_PROFILE_AUTHORITY_PATH_ENV}");
    println!("cargo:rerun-if-env-changed={PORTAL_PROFILE_AUTHORITY_SHA256_ENV}");
    println!("cargo:rerun-if-env-changed={PORTAL_PUBLIC_ORIGIN_ENV}");
    let Some(profile) = active_portal_profile() else {
        return;
    };

    let path = PathBuf::from(env::var_os(PORTAL_PROFILE_AUTHORITY_PATH_ENV).unwrap_or_else(|| {
        panic!("standalone Portal requires repository virtual-device profile authority via {PORTAL_PROFILE_AUTHORITY_PATH_ENV}")
    }));
    assert!(
        path.is_absolute(),
        "{PORTAL_PROFILE_AUTHORITY_PATH_ENV} must be absolute"
    );
    let metadata = fs::symlink_metadata(&path).unwrap_or_else(|_| {
        panic!("standalone-portal profile authority must be a readable regular file")
    });
    assert!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "standalone-portal profile authority must be a regular non-symlink file"
    );
    let bytes = fs::read(&path).unwrap_or_else(|_| {
        panic!("standalone-portal profile authority must be a readable regular file")
    });
    assert!(
        !bytes.is_empty() && bytes.len() <= MAX_PORTAL_PROFILE_AUTHORITY_BYTES,
        "standalone-portal profile authority has an invalid size"
    );
    let digest = env::var(PORTAL_PROFILE_AUTHORITY_SHA256_ENV).unwrap_or_else(|_| {
        panic!("standalone-portal requires {PORTAL_PROFILE_AUTHORITY_SHA256_ENV}")
    });
    assert!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value)),
        "standalone-portal profile authority digest must be lowercase SHA-256"
    );
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        actual, digest,
        "standalone-portal profile authority digest mismatch"
    );
    let target = env::var("TARGET").expect("Cargo must provide TARGET");
    portal_profile_authority::validate_manifest(&bytes, profile, &target)
        .unwrap_or_else(|error| panic!("{error}"));
    if profile == portal_profile_authority::PortalProfile::AndroidTailnet {
        let public_origin = env::var(PORTAL_PUBLIC_ORIGIN_ENV).unwrap_or_else(|_| {
            panic!("Portal tailnet profile requires {PORTAL_PUBLIC_ORIGIN_ENV}")
        });
        portal_profile_authority::validate_tailnet_public_origin(&public_origin)
            .unwrap_or_else(|error| panic!("{error}"));
        println!("cargo:rustc-env=OXID_EMBEDDED_PORTAL_PUBLIC_ORIGIN={public_origin}");
        println!("cargo:rustc-cfg=oxid_portal_android_physical_profile_authorized");
    } else {
        println!("cargo:rustc-cfg=oxid_portal_virtual_device_profile_authorized");
    }
    println!("cargo:rerun-if-changed={}", path.display());
}

fn embed_portal_manifest(output_directory: &std::path::Path) {
    println!("cargo:rerun-if-env-changed={PORTAL_MANIFEST_PATH_ENV}");
    println!("cargo:rerun-if-env-changed={PORTAL_MANIFEST_SHA256_ENV}");
    let mobile_target = env::var("CARGO_CFG_TARGET_OS")
        .is_ok_and(|target| matches!(target.as_str(), "ios" | "android"));
    let wasm_target = env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|target| target == "wasm32");
    if active_portal_profile().is_none() || !mobile_target || wasm_target {
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
