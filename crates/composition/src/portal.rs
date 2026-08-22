// SPDX-License-Identifier: Apache-2.0

//! Native development composition bridge for the exact authenticated Portal
//! PR #17 deployment profile. Desktop/headless uses an absolute manifest file;
//! the explicit standalone-local mobile profile uses build-embedded bytes.
//! Production, native-custody, tailnet, and WebAssembly compositions cannot
//! select this module.

use std::sync::Arc;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::path::Path;

use oxid_adapter_did_midnight::{
    HttpDidResolver, HttpDidResolverConfig, HttpDidResolverConfigError,
};
use oxid_adapter_openid4vci::{
    PortalCredentialMaterialDecoder, PortalCredentialMaterialError, PortalDeploymentManifest,
    PortalDeploymentManifestError, PortalOid4vciClientFactory,
};
use oxid_adapter_vc_midnight::{
    DigitalPassportIssuerTrustAnchor, DigitalPassportIssuerTrustAnchorError,
    convert_portal_private_parts,
};
use oxid_identity_application::DidResolutionPort;

#[cfg(any(test, target_os = "ios", target_os = "android"))]
const MOBILE_PORTAL_ISSUER_ORIGIN: &str = "http://127.0.0.1:18090";
#[cfg(any(test, target_os = "ios", target_os = "android"))]
const MOBILE_PORTAL_ISSUER_RESOLVER_ORIGIN: &str = "http://127.0.0.1:18093";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortalIdentityConfigurationError {
    Manifest(PortalDeploymentManifestError),
    Resolver(HttpDidResolverConfigError),
    TrustAnchor(DigitalPassportIssuerTrustAnchorError),
    #[cfg(any(test, target_os = "ios", target_os = "android"))]
    MobileHarnessOriginMismatch,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    ManifestPathMustBeAbsolute,
}

impl std::fmt::Display for PortalIdentityConfigurationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(error) => error.fmt(formatter),
            Self::Resolver(error) => error.fmt(formatter),
            Self::TrustAnchor(error) => error.fmt(formatter),
            #[cfg(any(test, target_os = "ios", target_os = "android"))]
            Self::MobileHarnessOriginMismatch => {
                formatter.write_str("Portal mobile harness origins do not match the local profile")
            }
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            Self::ManifestPathMustBeAbsolute => {
                formatter.write_str("Portal deployment manifest path must be absolute")
            }
        }
    }
}

impl std::error::Error for PortalIdentityConfigurationError {}

pub(crate) struct PortalIdentityConfiguration {
    pub(crate) client_factory: PortalOid4vciClientFactory,
    pub(crate) issuer_resolver: Arc<dyn DidResolutionPort>,
    pub(crate) trust_anchor: DigitalPassportIssuerTrustAnchor,
}

impl PortalIdentityConfiguration {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_file(
        path: &str,
        expected_sha256: &str,
    ) -> Result<Self, PortalIdentityConfigurationError> {
        let path = Path::new(path);
        if !path.is_absolute() {
            return Err(PortalIdentityConfigurationError::ManifestPathMustBeAbsolute);
        }
        let deployment = PortalDeploymentManifest::from_file(path, expected_sha256)
            .map_err(PortalIdentityConfigurationError::Manifest)?;
        Self::new(deployment)
    }

    #[cfg(all(
        feature = "mobile-portal",
        any(target_os = "ios", target_os = "android")
    ))]
    pub(crate) fn from_bytes(
        bytes: &[u8],
        expected_sha256: &str,
    ) -> Result<Self, PortalIdentityConfigurationError> {
        let deployment = PortalDeploymentManifest::from_bytes(bytes, expected_sha256)
            .map_err(PortalIdentityConfigurationError::Manifest)?;
        validate_mobile_harness_origins(&deployment)?;
        Self::new(deployment)
    }

    fn new(deployment: PortalDeploymentManifest) -> Result<Self, PortalIdentityConfigurationError> {
        let resolver = HttpDidResolverConfig::new(deployment.issuer_resolver_origin())
            .map(HttpDidResolver::new)
            .map_err(PortalIdentityConfigurationError::Resolver)?;
        let jwk = deployment.issuer_jubjub_jwk();
        let trust_anchor = DigitalPassportIssuerTrustAnchor::from_portal_jubjub(
            deployment.issuer_did(),
            deployment.issuer_method(),
            &jwk.x,
            &jwk.y,
            deployment.issuer_jubjub_jwk_sha256(),
        )
        .map_err(PortalIdentityConfigurationError::TrustAnchor)?;
        let client_factory = PortalOid4vciClientFactory::new(deployment)
            .map_err(PortalIdentityConfigurationError::Manifest)?;
        Ok(Self {
            client_factory,
            issuer_resolver: Arc::new(resolver),
            trust_anchor,
        })
    }
}

#[cfg(any(test, target_os = "ios", target_os = "android"))]
fn validate_mobile_harness_origins(
    deployment: &PortalDeploymentManifest,
) -> Result<(), PortalIdentityConfigurationError> {
    if deployment.issuer_origin() != MOBILE_PORTAL_ISSUER_ORIGIN
        || deployment.issuer_resolver_origin() != MOBILE_PORTAL_ISSUER_RESOLVER_ORIGIN
    {
        return Err(PortalIdentityConfigurationError::MobileHarnessOriginMismatch);
    }
    Ok(())
}

pub(crate) struct PortalPrivateMaterialDecoder;

impl PortalCredentialMaterialDecoder for PortalPrivateMaterialDecoder {
    fn decode(
        &self,
        signed_credential: &[u8],
        portal_private_json: &[u8],
    ) -> Result<Vec<u8>, PortalCredentialMaterialError> {
        convert_portal_private_parts(signed_credential, portal_private_json)
            .map_err(|_| PortalCredentialMaterialError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest as _, Sha256};

    use super::*;

    fn sha256(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn deployment(issuer_origin: &str, resolver_origin: &str) -> PortalDeploymentManifest {
        let jwk = r#"{"crv":"Jubjub","kty":"EC","x":"YS5_Q9FFCqvQpIwrvWqri2m4zOV-zs0vb3tcDABKFQs","y":"Nk88frhxJfALBtWKBoNlOs9BnT06nzZUQOxbWsDrd2M"}"#;
        let jwk_digest = sha256(jwk.as_bytes());
        let bytes = format!(
            concat!(
                r#"{{"integrationCommit":"925ec8d04882eabd4ac7b784c70fc2f0c152faae","integrationTree":"58b4597524f88a0ae2253439a44dab0dc60cbb6f","issuerDid":"did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","issuerJubjubJwk":{jwk},"issuerJubjubJwkSha256":"{jwk_digest}","issuerMethod":"did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef#key-assert","issuerOrigin":"{issuer_origin}","issuerResolverOrigin":"{resolver_origin}","portalPrHead":"9c82db23eabe8b6d758b2731f2225910ea627c14","profileSourceCommit":"76e8edf394a4cb37ca822037272d543c68f25f71","provenanceSha256":"cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87","schema":"oxid-portal-deployment-v2"}}"#
            ),
            jwk = jwk,
            jwk_digest = jwk_digest,
            issuer_origin = issuer_origin,
            resolver_origin = resolver_origin,
        )
        .into_bytes();
        PortalDeploymentManifest::from_bytes(&bytes, &sha256(&bytes))
            .expect("digest- and schema-authenticated deployment")
    }

    #[test]
    fn mobile_manifest_requires_the_exact_local_harness_origins() {
        let exact = deployment(
            MOBILE_PORTAL_ISSUER_ORIGIN,
            MOBILE_PORTAL_ISSUER_RESOLVER_ORIGIN,
        );
        validate_mobile_harness_origins(&exact).expect("exact mobile routes");

        for (issuer, resolver) in [
            (
                "http://127.0.0.1:18091",
                MOBILE_PORTAL_ISSUER_RESOLVER_ORIGIN,
            ),
            (MOBILE_PORTAL_ISSUER_ORIGIN, "http://127.0.0.1:18094"),
            (
                "https://issuer.example",
                MOBILE_PORTAL_ISSUER_RESOLVER_ORIGIN,
            ),
            (MOBILE_PORTAL_ISSUER_ORIGIN, "https://resolver.example"),
        ] {
            let authenticated = deployment(issuer, resolver);
            assert_eq!(
                validate_mobile_harness_origins(&authenticated),
                Err(PortalIdentityConfigurationError::MobileHarnessOriginMismatch)
            );
        }
    }

    #[test]
    fn headless_portal_configuration_preserves_generic_https_origins() {
        let deployment = deployment("https://issuer.example", "https://resolver.example");
        PortalIdentityConfiguration::new(deployment)
            .expect("headless authenticated HTTPS routes remain supported");
    }
}
