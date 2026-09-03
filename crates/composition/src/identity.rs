// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        all(not(target_os = "ios"), not(target_os = "android")),
        all(
            feature = "mobile-portal",
            any(target_os = "ios", target_os = "android")
        )
    )
))]
use super::portal::PortalIdentityConfiguration;
use oxid_adapter_did_midnight::StandaloneDidResolver;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "ios", target_os = "android"))
))]
use oxid_adapter_did_midnight::{HttpDidResolver, HttpDidResolverConfig};
#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        all(not(target_os = "ios"), not(target_os = "android")),
        all(
            feature = "mobile-portal",
            any(target_os = "ios", target_os = "android")
        )
    )
))]
use oxid_adapter_openid4vci::PortalOid4vciClientFactory;

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "ios", target_os = "android"))
))]
use super::environment::MIDNIGHT_DID_RESOLVER_URL_ENV;
use super::environment::{CREDENTIAL_KEY_PATH_ENV, CREDENTIAL_STORE_PATH_ENV, DID_STORE_PATH_ENV};
use oxid_adapter_storage_credential_json::EncryptedJsonCredentialRepository;
use oxid_adapter_storage_identity_json::JsonDidRecordRepository;
use oxid_adapter_storage_json::JsonWalletProfileRepository;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_vc_midnight::NativeCompactPresentationRuntime;
use oxid_credential_application::{
    CredentialDisclosurePort, CredentialInboxPort, CredentialRepository,
    CredentialVerificationPort, UnavailableCredentialRepository,
};
use oxid_identity_application::{
    DidJubjubChallengeSigningPort, DidLifecyclePort, DidPublicationPort, DidRecordRepository,
    DidResolutionPort, UnavailableDidRecordRepository,
};
use oxid_platform_ports::IdentityLinkIngressPort;

/// Returns the public embedded offer for the deterministic standalone issuer.
/// Production composition keeps the issuer port unavailable.
#[must_use]
pub fn standalone_oid4vci_offer() -> String {
    oxid_adapter_openid4vci::standalone_credential_offer()
}

/// Returns the public request-by-reference URI for the deterministic
/// standalone self-issued verifier. Production composition keeps it unavailable.
#[must_use]
pub fn standalone_siopv2_request() -> String {
    oxid_adapter_siopv2::standalone_self_issued_request()
}

/// Returns the public request-by-reference URI for the deterministic
/// standalone OpenID4VP verifier. Production composition keeps it unavailable.
#[must_use]
pub fn standalone_openid4vp_request() -> String {
    oxid_adapter_openid4vp::standalone_openid4vp_request()
}

pub(super) enum CredentialIssuanceComposition {
    Unavailable,
    Standalone,
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(
            all(not(target_os = "ios"), not(target_os = "android")),
            all(
                feature = "mobile-portal",
                any(target_os = "ios", target_os = "android")
            )
        )
    ))]
    Portal(Box<PortalOid4vciClientFactory>),
}

pub(super) enum HeadlessCredentialProfile {
    Standalone,
    #[cfg(all(
        not(target_arch = "wasm32"),
        any(
            all(not(target_os = "ios"), not(target_os = "android")),
            all(
                feature = "mobile-portal",
                any(target_os = "ios", target_os = "android")
            )
        )
    ))]
    Portal(Box<PortalIdentityConfiguration>),
}

#[derive(Clone, Copy)]
pub(super) enum SelfIssuedAuthenticationComposition {
    Unavailable,
    Standalone,
}

#[derive(Clone)]
pub(super) enum CredentialPresentationComposition {
    Unavailable,
    Standalone,
    #[cfg(not(target_arch = "wasm32"))]
    StandaloneZk(Arc<NativeCompactPresentationRuntime>),
    #[cfg(all(
        feature = "mobile-compact-artifacts",
        any(target_os = "ios", target_os = "android")
    ))]
    StandaloneMobileZk(Arc<NativeCompactPresentationRuntime>),
}

pub(super) struct IdentityAdapters {
    pub(super) did_repository: Arc<dyn DidRecordRepository>,
    pub(super) did_resolver: Arc<dyn DidResolutionPort>,
    pub(super) did_lifecycle: Arc<dyn DidLifecyclePort>,
    pub(super) did_jubjub_challenge_signing: Arc<dyn DidJubjubChallengeSigningPort>,
    pub(super) did_publisher: Option<Arc<dyn DidPublicationPort>>,
    pub(super) credential_repository: Arc<dyn CredentialRepository>,
    pub(super) credential_inbox: Arc<dyn CredentialInboxPort>,
    pub(super) credential_verifier: Arc<dyn CredentialVerificationPort>,
    pub(super) credential_disclosure: Arc<dyn CredentialDisclosurePort>,
    pub(super) credential_issuance: CredentialIssuanceComposition,
    pub(super) self_issued_authentication: SelfIssuedAuthenticationComposition,
    pub(super) credential_presentation: CredentialPresentationComposition,
    pub(super) portal_test_ingress: Option<Arc<dyn IdentityLinkIngressPort>>,
}

pub(super) fn headless_credential_repository() -> Arc<dyn CredentialRepository> {
    let configured = (
        std::env::var_os(CREDENTIAL_STORE_PATH_ENV),
        std::env::var_os(CREDENTIAL_KEY_PATH_ENV),
    );
    let paths = match configured {
        (Some(path), Some(key)) => Some((
            std::path::PathBuf::from(path),
            std::path::PathBuf::from(key),
        )),
        (None, None) => JsonWalletProfileRepository::at_default_location()
            .configured_path()
            .and_then(std::path::Path::parent)
            .map(|directory| {
                (
                    directory.join("private/credentials.enc"),
                    directory.join("private/credentials.key"),
                )
            }),
        _ => None,
    };
    paths.map_or_else(
        || Arc::new(UnavailableCredentialRepository) as Arc<dyn CredentialRepository>,
        |(path, key)| {
            Arc::new(EncryptedJsonCredentialRepository::new(path, key))
                as Arc<dyn CredentialRepository>
        },
    )
}

pub(super) fn headless_did_repository() -> Arc<dyn DidRecordRepository> {
    let path = std::env::var_os(DID_STORE_PATH_ENV)
        .map(std::path::PathBuf::from)
        .or_else(|| {
            JsonWalletProfileRepository::at_default_location()
                .configured_path()
                .and_then(std::path::Path::parent)
                .map(|directory| directory.join("private/did-records.json"))
        });
    path.map_or_else(
        || Arc::new(UnavailableDidRecordRepository) as Arc<dyn DidRecordRepository>,
        |path| Arc::new(JsonDidRecordRepository::new(path)) as Arc<dyn DidRecordRepository>,
    )
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "ios", target_os = "android"))
))]
pub(super) fn headless_did_resolver() -> Arc<dyn DidResolutionPort> {
    std::env::var_os(MIDNIGHT_DID_RESOLVER_URL_ENV)
        .and_then(|value| value.into_string().ok())
        .and_then(|value| HttpDidResolverConfig::new(value).ok())
        .map_or_else(
            || Arc::new(StandaloneDidResolver) as Arc<dyn DidResolutionPort>,
            |config| Arc::new(HttpDidResolver::new(config)) as Arc<dyn DidResolutionPort>,
        )
}

#[cfg(any(target_arch = "wasm32", target_os = "ios", target_os = "android"))]
pub(super) fn headless_did_resolver() -> Arc<dyn DidResolutionPort> {
    Arc::new(StandaloneDidResolver)
}
