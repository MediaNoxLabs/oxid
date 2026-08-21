// SPDX-License-Identifier: Apache-2.0

//! Native desktop/headless-only composition bridge for the exact authenticated
//! Portal PR #17 deployment profile. Production and mobile composition cannot
//! compile this module.

use std::{path::Path, sync::Arc};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortalIdentityConfigurationError {
    Manifest(PortalDeploymentManifestError),
    Resolver(HttpDidResolverConfigError),
    TrustAnchor(DigitalPassportIssuerTrustAnchorError),
    ManifestPathMustBeAbsolute,
}

impl std::fmt::Display for PortalIdentityConfigurationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(error) => error.fmt(formatter),
            Self::Resolver(error) => error.fmt(formatter),
            Self::TrustAnchor(error) => error.fmt(formatter),
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
