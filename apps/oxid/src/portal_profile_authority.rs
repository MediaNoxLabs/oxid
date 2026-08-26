// SPDX-License-Identifier: Apache-2.0

use url::Url;

pub const LOCAL_PROFILE: &str = "standalone-local-development-portal";
pub const ANDROID_TAILNET_PROFILE: &str = "standalone-tailnet-development-portal-android";
pub const SCHEMA: &str = "oxid-app-profile-authority-v2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalProfile {
    Local,
    AndroidTailnet,
}

impl PortalProfile {
    const fn name(self) -> &'static str {
        match self {
            Self::Local => LOCAL_PROFILE,
            Self::AndroidTailnet => ANDROID_TAILNET_PROFILE,
        }
    }
}

fn platform_for_target(profile: PortalProfile, target: &str) -> Option<&'static str> {
    match (profile, target) {
        (PortalProfile::Local, "aarch64-apple-ios-sim" | "x86_64-apple-ios") => {
            Some("ios_simulator")
        }
        (PortalProfile::Local, "aarch64-linux-android" | "x86_64-linux-android") => {
            Some("android_qemu")
        }
        (PortalProfile::AndroidTailnet, "aarch64-linux-android") => Some("android_physical"),
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
    let expected = canonical_manifest(profile, target)
        .ok_or("standalone Portal authority does not permit this profile and target")?;
    if bytes == expected.as_bytes() {
        Ok(())
    } else {
        Err("standalone Portal authority manifest is not canonical")
    }
}

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
    let port = url
        .port()
        .filter(|port| *port >= 1024 && !matches!(*port, 8443 | 10_000))
        .ok_or("Portal tailnet public origin is invalid")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !host.ends_with(".ts.net")
        || host == "ts.net"
        || !labels_are_canonical
        || url.origin().ascii_serialization() != value
        || port == 443
    {
        return Err("Portal tailnet public origin is invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_authority_accepts_only_virtual_mobile_targets() {
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
        assert!(canonical_manifest(PortalProfile::Local, "aarch64-apple-ios").is_none());
    }

    #[test]
    fn tailnet_authority_accepts_only_physical_android_target() {
        let target = "aarch64-linux-android";
        let manifest =
            canonical_manifest(PortalProfile::AndroidTailnet, target).expect("authorized");
        assert_eq!(
            validate_manifest(manifest.as_bytes(), PortalProfile::AndroidTailnet, target),
            Ok(())
        );
        for rejected in [
            "x86_64-linux-android",
            "aarch64-apple-ios",
            "aarch64-apple-ios-sim",
            "aarch64-unknown-linux-gnu",
            "wasm32-unknown-unknown",
        ] {
            assert!(canonical_manifest(PortalProfile::AndroidTailnet, rejected).is_none());
        }
    }

    #[test]
    fn manifests_fail_closed_on_shape_drift() {
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
    fn tailnet_origin_is_canonical_dynamic_magic_dns_https() {
        for valid in [
            "https://oxid-demo.tail1234.ts.net:9443",
            "https://wallet.tailabcd.ts.net:12001",
        ] {
            assert_eq!(validate_tailnet_public_origin(valid), Ok(()));
        }
        for invalid in [
            "http://oxid-demo.tail1234.ts.net:9443",
            "https://oxid-demo.tail1234.ts.net",
            "https://oxid-demo.tail1234.ts.net:443",
            "https://oxid-demo.tail1234.ts.net:8443",
            "https://oxid-demo.tail1234.ts.net:10000",
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
