// SPDX-License-Identifier: Apache-2.0

//! Native development composition bridge for the authenticated Portal profile.
//! Desktop/headless uses an absolute manifest file; explicit mobile profiles use
//! build-embedded bytes. Production, native-custody, and WebAssembly composition
//! cannot select this module.

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

use crate::PortalTestIngress;

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
            Self::MobileHarnessOriginMismatch => formatter
                .write_str("Portal mobile harness routes do not match the authenticated profile"),
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
    pub(crate) test_ingress: PortalTestIngress,
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
        Self::new(deployment, PortalTestIngress::None)
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
        Self::new(deployment, PortalTestIngress::Loopback)
    }

    #[cfg(all(feature = "mobile-portal-tailnet", target_os = "android"))]
    pub(crate) fn from_tailnet_bytes(
        bytes: &[u8],
        expected_sha256: &str,
        public_origin: &str,
    ) -> Result<Self, PortalIdentityConfigurationError> {
        let deployment = PortalDeploymentManifest::from_bytes(bytes, expected_sha256)
            .map_err(PortalIdentityConfigurationError::Manifest)?;
        validate_tailnet_harness_origins(&deployment, public_origin)?;
        Self::new(
            deployment,
            PortalTestIngress::Tailnet {
                public_origin: public_origin.to_owned(),
            },
        )
    }

    fn new(
        deployment: PortalDeploymentManifest,
        test_ingress: PortalTestIngress,
    ) -> Result<Self, PortalIdentityConfigurationError> {
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
            test_ingress,
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

#[cfg(any(test, all(feature = "mobile-portal-tailnet", target_os = "android")))]
fn validate_tailnet_harness_origins(
    deployment: &PortalDeploymentManifest,
    public_origin: &str,
) -> Result<(), PortalIdentityConfigurationError> {
    let expected_resolver = format!("{public_origin}/issuer-resolver");
    if deployment.issuer_origin() != public_origin
        || deployment.issuer_resolver_origin() != expected_resolver
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
                r#"{{"integrationCommit":"22ae5369b6f939e6b20648f4b85dd993527748ef","integrationTree":"74d8d1a5b87c160ea554006e47d5f3edc3cd3e10","issuerDid":"did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","issuerJubjubJwk":{jwk},"issuerJubjubJwkSha256":"{jwk_digest}","issuerMethod":"did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef#key-assert","issuerOrigin":"{issuer_origin}","issuerResolverOrigin":"{resolver_origin}","provenanceSha256":"cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87","schema":"oxid-portal-deployment-v3"}}"#
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
    fn tailnet_manifest_requires_one_exact_public_origin_and_resolver_prefix() {
        let origin = "https://oxid-demo.tail1234.ts.net:9443";
        validate_tailnet_harness_origins(
            &deployment(origin, &format!("{origin}/issuer-resolver")),
            origin,
        )
        .expect("exact tailnet routes");

        for (issuer, resolver) in [
            (
                "https://other.tail1234.ts.net:9443".to_owned(),
                format!("{origin}/issuer-resolver"),
            ),
            (origin.to_owned(), format!("{origin}/resolver")),
            (origin.to_owned(), format!("{origin}/issuer-resolution")),
        ] {
            assert_eq!(
                validate_tailnet_harness_origins(&deployment(&issuer, &resolver), origin),
                Err(PortalIdentityConfigurationError::MobileHarnessOriginMismatch)
            );
        }
    }

    #[test]
    fn headless_portal_configuration_preserves_generic_https_origins() {
        let deployment = deployment("https://issuer.example", "https://resolver.example");
        PortalIdentityConfiguration::new(deployment, PortalTestIngress::None)
            .expect("headless authenticated HTTPS routes remain supported");
    }
}
