// SPDX-License-Identifier: Apache-2.0

//! Closed build-authority manifests for virtual-device Portal profiles.
//!
//! Cargo target cfgs distinguish an iOS simulator from an iPhone, but Android
//! uses the same Rust target triples for QEMU and physical devices. Repository
//! launchers validate the live virtual device first and create one exact,
//! transient authority manifest. `build.rs` authenticates it and emits the
//! private cfg consumed by `main.rs`; selecting a Cargo feature directly is
//! deliberately insufficient.

use url::Url;

pub const LOCAL_PROFILE: &str = "standalone-local-development-portal";
pub const IOS_TAILNET_PROFILE: &str = "standalone-tailnet-development-portal-ios-simulator";
pub const SCHEMA: &str = "oxid-app-profile-authority-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalProfile {
    Local,
    IosTailnetSimulator,
}

impl PortalProfile {
    const fn name(self) -> &'static str {
        match self {
            Self::Local => LOCAL_PROFILE,
            Self::IosTailnetSimulator => IOS_TAILNET_PROFILE,
        }
    }
}

fn platform_for_target(profile: PortalProfile, target: &str) -> Option<&'static str> {
    match (profile, target) {
        (PortalProfile::Local, "aarch64-apple-ios-sim" | "x86_64-apple-ios")
        | (PortalProfile::IosTailnetSimulator, "aarch64-apple-ios-sim" | "x86_64-apple-ios") => {
            Some("ios_simulator")
        }
        (PortalProfile::Local, "aarch64-linux-android" | "x86_64-linux-android") => {
            Some("android_qemu")
        }
        _ => None,
    }
}

#[must_use]
pub fn canonical_manifest(profile: PortalProfile, target: &str) -> Option<String> {
    let platform = platform_for_target(profile, target)?;
    let profile = profile.name();
    Some(format!(
        r#"{{"platform":"{platform}","profile":"{profile}","schema":"{SCHEMA}","target":"{target}"}}"#
    ))
}

pub fn validate_manifest(
    bytes: &[u8],
    profile: PortalProfile,
    target: &str,
) -> Result<(), &'static str> {
    let expected = canonical_manifest(profile, target).ok_or(
        "standalone Portal authority does not permit this profile/virtual-device target pair",
    )?;
    if bytes == expected.as_bytes() {
        Ok(())
    } else {
        Err(
            "standalone Portal authority manifest is not the exact canonical virtual-device profile",
        )
    }
}

/// Requires the one demo origin to be an exact canonical Tailscale MagicDNS
/// HTTPS origin on the Oxid-owned temporary port. No route, credentials,
/// query, fragment, IP literal, or implicit port is accepted.
pub fn validate_tailnet_public_origin(value: &str) -> Result<(), &'static str> {
    if value.len() > 512 {
        return Err("Portal tailnet public origin is invalid");
    }
    let url = Url::parse(value).map_err(|_| "Portal tailnet public origin is invalid")?;
    let host = url
        .host_str()
        .ok_or("Portal tailnet public origin is invalid")?;
    let labels_are_canonical = host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    });
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port() != Some(9443)
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !host.ends_with(".ts.net")
        || host == "ts.net"
        || !labels_are_canonical
        || url.origin().ascii_serialization() != value
    {
        return Err("Portal tailnet public origin is invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_local_virtual_device_manifests_are_authorized() {
        for target in [
            "aarch64-apple-ios-sim",
            "x86_64-apple-ios",
            "aarch64-linux-android",
            "x86_64-linux-android",
        ] {
            let manifest = canonical_manifest(PortalProfile::Local, target).expect("authorized");
            assert_eq!(
                validate_manifest(manifest.as_bytes(), PortalProfile::Local, target),
                Ok(())
            );
        }
    }

    #[test]
    fn tailnet_profile_authorizes_only_ios_simulator_targets() {
        for target in ["aarch64-apple-ios-sim", "x86_64-apple-ios"] {
            let manifest = canonical_manifest(PortalProfile::IosTailnetSimulator, target)
                .expect("authorized simulator");
            assert_eq!(
                validate_manifest(
                    manifest.as_bytes(),
                    PortalProfile::IosTailnetSimulator,
                    target
                ),
                Ok(())
            );
            assert!(validate_manifest(manifest.as_bytes(), PortalProfile::Local, target).is_err());
        }
        for target in [
            "aarch64-apple-ios",
            "aarch64-linux-android",
            "x86_64-linux-android",
            "aarch64-unknown-linux-gnu",
            "wasm32-unknown-unknown",
        ] {
            assert!(canonical_manifest(PortalProfile::IosTailnetSimulator, target).is_none());
        }
    }

    #[test]
    fn manifests_fail_closed_on_profile_platform_target_or_shape_drift() {
        let target = "aarch64-linux-android";
        let valid = canonical_manifest(PortalProfile::Local, target).expect("manifest");
        for invalid in [
            valid.replace("android_qemu", "android_physical"),
            valid.replace(LOCAL_PROFILE, "standalone-local"),
            valid.replace(target, "x86_64-linux-android"),
            valid.replace('}', ",\"extra\":true}"),
            format!("{valid}\n"),
        ] {
            assert!(validate_manifest(invalid.as_bytes(), PortalProfile::Local, target).is_err());
        }
    }

    #[test]
    fn tailnet_public_origin_is_exact_https_magic_dns_on_9443() {
        assert_eq!(
            validate_tailnet_public_origin("https://oxid-demo.tail1234.ts.net:9443"),
            Ok(())
        );
        for invalid in [
            "http://oxid-demo.tail1234.ts.net:9443",
            "https://oxid-demo.tail1234.ts.net",
            "https://oxid-demo.tail1234.ts.net:9443/offer",
            "https://user@oxid-demo.tail1234.ts.net:9443",
            "https://127.0.0.1:9443",
            "https://Oxid-demo.tail1234.ts.net:9443",
            "https://-oxid.tail1234.ts.net:9443",
            "https://oxid.example:9443",
        ] {
            assert!(
                validate_tailnet_public_origin(invalid).is_err(),
                "{invalid}"
            );
        }
    }
}
