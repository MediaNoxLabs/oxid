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
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{MidnightIndexerConfigError, MidnightStandaloneConfigError};
#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        not(any(target_os = "ios", target_os = "android")),
        test,
        feature = "standalone-development"
    )
))]
use oxid_adapter_midnight::{MidnightStandaloneConfig, protected_standalone_midnight_wallet};
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_midnight::{
    MidnightSubmissionJournalConfig, protected_simulated_midnight_wallet,
    protected_simulated_midnight_wallet_with_submission_journal,
};

#[cfg(not(target_arch = "wasm32"))]
use super::environment::HeadlessCompositionError;
use super::identity::CredentialPresentationComposition;
#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        not(any(target_os = "ios", target_os = "android")),
        feature = "mobile-portal"
    )
))]
use super::identity::HeadlessCredentialProfile;
#[cfg(any(target_os = "ios", target_os = "android"))]
use super::passport_vault::with_simulated_passport_vault_calls;
#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        not(any(target_os = "ios", target_os = "android")),
        feature = "mobile-portal"
    )
))]
use super::passport_vault::{
    node_anchored_passport_vault_state_source, with_passport_vault_state_source,
};
#[cfg(not(target_arch = "wasm32"))]
use super::profile_headless::compose_headless_standalone;
#[cfg(all(not(target_arch = "wasm32"), feature = "standalone-development"))]
use super::profile_headless::compose_public_genesis_standalone;
use super::services::ApplicationServices;
#[cfg(all(
    not(target_arch = "wasm32"),
    feature = "standalone-development",
    feature = "mobile-portal",
    any(target_os = "ios", target_os = "android")
))]
use super::standalone_genesis::public_profile_protection;
#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        not(any(target_os = "ios", target_os = "android")),
        feature = "mobile-portal"
    )
))]
use super::wiring::compose_with_adapters_and_credential_profile;
#[cfg(any(target_os = "ios", target_os = "android"))]
use super::wiring::compose_with_adapters_and_presentation;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_platform_system::OsRandom;
use oxid_adapter_platform_system::SystemClock;
#[cfg(all(
    not(target_arch = "wasm32"),
    any(
        not(any(target_os = "ios", target_os = "android")),
        feature = "mobile-portal"
    )
))]
use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
use oxid_adapter_storage_json::JsonWalletProfileRepository;
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_storage_mobile::MobileWalletSecurity;
#[cfg(all(
    feature = "mobile-compact-artifacts",
    any(target_os = "ios", target_os = "android")
))]
use oxid_adapter_vc_midnight::CompactPresentationRuntimeError;
#[cfg(not(target_arch = "wasm32"))]
use oxid_wallet_application::WalletProtectionPort;

/// Verifies that the Android Portal conformance composition is executing under
/// the repository's QEMU-only runtime boundary. iOS simulator authority is
/// already encoded by its distinct Rust target; non-mobile builds never reach
/// this feature because of the compile-time guard above.
#[cfg(all(feature = "mobile-portal", target_os = "android"))]
pub fn verify_android_portal_virtual_device_profile() -> Result<(), &'static str> {
    oxid_adapter_mobile_native::verify_android_qemu_profile()
        .map_err(|_| "standalone-portal requires Android QEMU at runtime")
}

#[cfg(test)]
#[path = "profile_mobile/tests.rs"]
mod tests;

/// Wires the complete standalone simulation through production mobile custody.
///
/// This opt-in harness exists so iOS/Android can exercise every wallet and SSI
/// flow against the same device-bound security adapter selected by normal
/// mobile composition. It never enables development custody and does not turn
/// simulated Midnight settlement into a production claim.
#[cfg(any(target_os = "ios", target_os = "android"))]
#[must_use]
pub fn compose_mobile_native_standalone() -> ApplicationServices {
    compose_mobile_native_standalone_with_presentation(
        CredentialPresentationComposition::Standalone,
    )
}

/// Wires the explicit standalone native-custody mobile harness to the
/// authenticated embedded Compact runtime through the foreground-only worker.
/// Normal production and ordinary standalone mobile composition do not call
/// this constructor.
#[cfg(all(
    feature = "mobile-compact-artifacts",
    any(target_os = "ios", target_os = "android")
))]
pub fn compose_mobile_native_standalone_with_compact_presentation()
-> Result<ApplicationServices, CompactPresentationRuntimeError> {
    let runtime =
        Arc::new(oxid_adapter_vc_midnight::load_embedded_mobile_compact_presentation_runtime()?);
    Ok(compose_mobile_native_standalone_with_presentation(
        CredentialPresentationComposition::StandaloneMobileZk(runtime),
    ))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn compose_mobile_native_standalone_with_presentation(
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(MobileWalletSecurity::native(
        Arc::clone(&clock),
        Arc::clone(&random),
    ));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = profiles
        .configured_path()
        .and_then(|path| path.parent())
        .map(|directory| directory.join("private/midnight-submissions.json"))
        .and_then(|path| MidnightSubmissionJournalConfig::new(path).ok())
        .map_or_else(
            || protected_simulated_midnight_wallet(Arc::clone(&clock), Arc::clone(&security)),
            |journal| {
                protected_simulated_midnight_wallet_with_submission_journal(
                    journal,
                    Arc::clone(&clock),
                    Arc::clone(&security),
                )
            },
        )
        .with_profile_association_repository(profiles.clone());
    let services = compose_with_adapters_and_presentation(
        profiles,
        security,
        Arc::new(midnight),
        credential_presentation,
    );
    with_simulated_passport_vault_calls(services)
}

/// Runs the explicit Android smoke probe for JNI exception recovery.
#[cfg(all(target_os = "android", feature = "android-jni-exception-recovery-test"))]
pub fn verify_android_jni_exception_recovery()
-> Result<(), oxid_adapter_mobile_native::NativeBridgeError> {
    oxid_adapter_mobile_native::verify_android_jni_exception_recovery()
}

/// Authenticates the immutable Compact presentation package selected by an
/// explicit mobile conformance build without changing composition by itself.
///
/// Callers that need proof execution must select
/// [`compose_mobile_native_standalone_with_compact_presentation`].
#[cfg(all(
    feature = "mobile-compact-artifacts",
    any(target_os = "ios", target_os = "android")
))]
pub fn authenticate_embedded_mobile_compact_presentation_artifacts()
-> Result<[u8; 32], CompactPresentationRuntimeError> {
    oxid_adapter_vc_midnight::load_embedded_mobile_compact_presentation_runtime()
        .map(|runtime| runtime.identity())
}

/// Wires the mobile development harness to an explicitly build-selected
/// standalone stack without making routes part of the network catalog.
///
/// The app crate exposes this constructor only behind its opt-in local or
/// tailnet live-stack route profile. Normal and native-custody mobile
/// composition never call it.
#[cfg(not(target_arch = "wasm32"))]
pub fn compose_mobile_development_standalone_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    let config = mobile_standalone_config_from_routes(
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
    )?;
    Ok(compose_headless_standalone(config))
}

/// Wires the explicitly named public-genesis development profile to standalone routes.
#[cfg(all(not(target_arch = "wasm32"), feature = "standalone-development"))]
pub fn compose_mobile_public_genesis_standalone_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    let config = mobile_standalone_config_from_routes(
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
    )?;
    Ok(compose_public_genesis_standalone(config))
}

/// Wires the exact manifest-authenticated Portal identity profile into the
/// explicit standalone-local mobile development composition.
///
/// Routes and deployment authority are build inputs owned by `oxid-app`'s
/// `standalone-portal` profile. No runtime environment, production, tailnet,
/// native-custody, or WebAssembly composition calls this constructor.
#[cfg(all(
    feature = "mobile-portal",
    any(target_os = "ios", target_os = "android"),
    not(target_arch = "wasm32")
))]
pub fn compose_mobile_development_portal_standalone_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
    deployment_manifest: &[u8],
    deployment_manifest_sha256: &str,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    let config = mobile_standalone_config_from_routes(
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
    )?;
    let portal =
        PortalIdentityConfiguration::from_bytes(deployment_manifest, deployment_manifest_sha256)
            .map_err(|_| HeadlessCompositionError::InvalidPortalConfiguration)?;
    Ok(compose_development_portal_from_config(
        config,
        portal,
        CredentialPresentationComposition::Standalone,
    ))
}

/// Wires Portal issuance into the authenticated physical Android tailnet profile.
#[cfg(all(
    feature = "mobile-portal-tailnet",
    target_os = "android",
    not(target_arch = "wasm32")
))]
pub fn compose_mobile_development_portal_tailnet_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
    deployment_manifest: &[u8],
    deployment_manifest_sha256: &str,
    public_origin: &str,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    let config = mobile_standalone_config_from_routes(
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
    )?;
    let portal = PortalIdentityConfiguration::from_tailnet_bytes(
        deployment_manifest,
        deployment_manifest_sha256,
        public_origin,
    )
    .map_err(|_| HeadlessCompositionError::InvalidPortalConfiguration)?;
    Ok(compose_development_portal_from_config(
        config,
        portal,
        CredentialPresentationComposition::Standalone,
    ))
}

/// Wires the explicitly named public-genesis profile to local Portal issuance.
#[cfg(all(
    feature = "mobile-portal",
    feature = "standalone-development",
    any(target_os = "ios", target_os = "android"),
    not(target_arch = "wasm32")
))]
pub fn compose_mobile_public_genesis_portal_standalone_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
    deployment_manifest: &[u8],
    deployment_manifest_sha256: &str,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    let config = mobile_standalone_config_from_routes(
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
    )?;
    let portal =
        PortalIdentityConfiguration::from_bytes(deployment_manifest, deployment_manifest_sha256)
            .map_err(|_| HeadlessCompositionError::InvalidPortalConfiguration)?;
    Ok(compose_mobile_public_genesis_portal_from_config(
        config,
        portal,
        CredentialPresentationComposition::Standalone,
    ))
}

/// Wires the explicitly named public-genesis profile to Tailnet Portal issuance.
#[cfg(all(
    feature = "mobile-portal-tailnet",
    feature = "standalone-development",
    target_os = "android",
    not(target_arch = "wasm32")
))]
pub fn compose_mobile_public_genesis_portal_tailnet_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
    deployment_manifest: &[u8],
    deployment_manifest_sha256: &str,
    public_origin: &str,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    let config = mobile_standalone_config_from_routes(
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
    )?;
    let portal = PortalIdentityConfiguration::from_tailnet_bytes(
        deployment_manifest,
        deployment_manifest_sha256,
        public_origin,
    )
    .map_err(|_| HeadlessCompositionError::InvalidPortalConfiguration)?;
    Ok(compose_mobile_public_genesis_portal_from_config(
        config,
        portal,
        CredentialPresentationComposition::Standalone,
    ))
}

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
pub(super) fn compose_development_portal_from_config(
    config: MidnightStandaloneConfig,
    portal: PortalIdentityConfiguration,
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::clone(&clock),
        Arc::new(OsRandom),
    ));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    compose_development_portal_with_security(
        config,
        portal,
        credential_presentation,
        clock,
        security,
        profiles,
        None,
    )
}

#[cfg(all(
    not(target_arch = "wasm32"),
    feature = "standalone-development",
    feature = "mobile-portal",
    any(target_os = "ios", target_os = "android")
))]
fn compose_mobile_public_genesis_portal_from_config(
    config: MidnightStandaloneConfig,
    portal: PortalIdentityConfiguration,
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::clone(&clock),
        Arc::new(OsRandom),
    ));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let protection = public_profile_protection(
        config.indexer().network_id().as_str(),
        Arc::clone(&profiles),
        Arc::clone(&security),
    )
    .map(|protection| Arc::new(protection) as Arc<dyn WalletProtectionPort>);
    compose_development_portal_with_security(
        config,
        portal,
        credential_presentation,
        clock,
        security,
        profiles,
        protection,
    )
}

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
fn compose_development_portal_with_security<N>(
    config: MidnightStandaloneConfig,
    portal: PortalIdentityConfiguration,
    credential_presentation: CredentialPresentationComposition,
    clock: Arc<SystemClock>,
    security: Arc<DevelopmentWalletSecurity<SystemClock, N>>,
    profiles: Arc<JsonWalletProfileRepository>,
    protection_for_security: Option<Arc<dyn WalletProtectionPort>>,
) -> ApplicationServices
where
    N: oxid_platform_ports::RandomPort + 'static,
{
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let midnight = Arc::new(
        protected_standalone_midnight_wallet(config, Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    let services = compose_with_adapters_and_credential_profile(
        profiles,
        security,
        midnight,
        credential_presentation,
        HeadlessCredentialProfile::Portal(Box::new(portal)),
        protection_for_security,
    );
    with_passport_vault_state_source(services, passport_vault_state_source)
}

#[cfg(not(target_arch = "wasm32"))]
fn mobile_standalone_config_from_routes(
    indexer_websocket_url: &str,
    indexer_http_url: &str,
    node_websocket_url: &str,
    proof_server_url: &str,
) -> Result<MidnightStandaloneConfig, HeadlessCompositionError> {
    let placeholder = oxid_adapter_midnight::standalone_configuration_placeholder_address()
        .map_err(|_| {
            HeadlessCompositionError::InvalidMidnightStandaloneConfiguration(
                MidnightStandaloneConfigError::Indexer(MidnightIndexerConfigError::InvalidAddress),
            )
        })?;
    MidnightStandaloneConfig::new(
        "undeployed",
        indexer_websocket_url,
        indexer_http_url,
        node_websocket_url,
        proof_server_url,
        placeholder.value(),
    )
    .map_err(HeadlessCompositionError::InvalidMidnightStandaloneConfiguration)
}
