// SPDX-License-Identifier: Apache-2.0

//! Closed build-authority manifest for the virtual-device Portal profile.
//!
//! Cargo target cfgs distinguish an iOS simulator from an iPhone, but Android
//! uses the same Rust target triples for QEMU and physical devices. The
//! repository launchers therefore validate the live virtual device first and
//! create this exact, transient manifest only for the reviewed
//! standalone-local conformance build. `build.rs` authenticates the file and
//! emits the private cfg consumed by `main.rs`; selecting the Cargo feature
//! directly is deliberately insufficient.

pub const PROFILE: &str = "standalone-local-development-portal";
pub const SCHEMA: &str = "oxid-app-profile-authority-v1";

fn platform_for_target(target: &str) -> Option<&'static str> {
    match target {
        "aarch64-apple-ios-sim" | "x86_64-apple-ios" => Some("ios_simulator"),
        "aarch64-linux-android" | "x86_64-linux-android" => Some("android_qemu"),
        _ => None,
    }
}

#[must_use]
pub fn canonical_manifest(target: &str) -> Option<String> {
    let platform = platform_for_target(target)?;
    Some(format!(
        r#"{{"platform":"{platform}","profile":"{PROFILE}","schema":"{SCHEMA}","target":"{target}"}}"#
    ))
}

pub fn validate_manifest(bytes: &[u8], target: &str) -> Result<(), &'static str> {
    let expected = canonical_manifest(target)
        .ok_or("standalone-portal authority permits only iOS Simulator or Android QEMU targets")?;
    if bytes == expected.as_bytes() {
        Ok(())
    } else {
        Err(
            "standalone-portal authority manifest is not the exact canonical virtual-device profile",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_virtual_device_manifests_are_authorized() {
        for target in [
            "aarch64-apple-ios-sim",
            "x86_64-apple-ios",
            "aarch64-linux-android",
            "x86_64-linux-android",
        ] {
            let manifest = canonical_manifest(target).expect("authorized target");
            assert_eq!(validate_manifest(manifest.as_bytes(), target), Ok(()));
        }
    }

    #[test]
    fn physical_and_non_mobile_targets_are_rejected() {
        for target in [
            "aarch64-apple-ios",
            "armv7-linux-androideabi",
            "aarch64-unknown-linux-gnu",
            "wasm32-unknown-unknown",
        ] {
            assert!(canonical_manifest(target).is_none());
            assert!(validate_manifest(b"{}", target).is_err());
        }
    }

    #[test]
    fn manifests_fail_closed_on_profile_platform_target_or_shape_drift() {
        let target = "aarch64-linux-android";
        let valid = canonical_manifest(target).expect("manifest");
        for invalid in [
            valid.replace("android_qemu", "android_physical"),
            valid.replace(PROFILE, "standalone-local"),
            valid.replace(target, "x86_64-linux-android"),
            valid.replace("}", ",\"extra\":true}"),
            format!("{valid}\n"),
        ] {
            assert!(validate_manifest(invalid.as_bytes(), target).is_err());
        }
    }
}
