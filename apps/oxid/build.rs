// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{env, path::PathBuf};

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
    oxid_brand_build::generate_brand(&brand_directory, output_directory)
        .unwrap_or_else(|error| panic!("default brand generation failed: {error}"));

    println!("cargo:rerun-if-changed={}", brand_directory.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_directory.join("Dioxus.toml").display()
    );
}
