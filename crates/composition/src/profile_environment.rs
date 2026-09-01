// SPDX-License-Identifier: Apache-2.0

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(all(not(target_arch = "wasm32"), feature = "desktop-portal-test"))]
use oxid_adapter_identity_ingress::DesktopPortalTestQrScanner;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_passport_vault::{
    AuthenticatedPassportVaultStateSource, PassportVaultCallChainContextSource,
};
#[cfg(not(target_arch = "wasm32"))]
use oxid_passport_vault_application::PassportVaultContractStateSourcePort;

#[cfg(not(target_arch = "wasm32"))]
use super::environment::{
    HeadlessCompositionError, HeadlessEnvironmentPlan, HeadlessEnvironmentPolicy,
    HeadlessMidnightConfig, load_headless_environment_plan,
};
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
use super::identity::HeadlessCredentialProfile;
#[cfg(not(target_arch = "wasm32"))]
use super::passport_vault::{with_native_passport_vault_calls, with_passport_vault_state_source};
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
use super::profile_headless::compose_headless_with_credential_profile;
#[cfg(not(target_arch = "wasm32"))]
use super::profile_headless::{
    compose_headless_live_with_checkpoint_options_and_presentation,
    compose_headless_standalone_with_checkpoint_options_and_presentation,
    compose_headless_with_presentation, compose_headless_with_submission_journal_and_presentation,
};
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
use super::profile_mobile::compose_development_portal_from_config;
#[cfg(not(target_arch = "wasm32"))]
use super::services::ApplicationServices;

/// Selects deterministic simulation when no live variables are present, a
/// read-only indexer when the three read values are present, or complete
/// standalone submission when every route and exactly one proving mode are valid.
#[cfg(not(target_arch = "wasm32"))]
pub fn compose_headless_from_environment() -> Result<ApplicationServices, HeadlessCompositionError>
{
    compose_headless_from_environment_with_policy(HeadlessEnvironmentPolicy::General)
}

/// Selects the ordinary headless environment composition while admitting one
/// exact Portal-plus-local-standalone bundle for the native `oxid-headless`
/// process. Other incoming adapters retain [`compose_headless_from_environment`].
#[cfg(all(not(target_arch = "wasm32"), feature = "headless-portal-local"))]
pub fn compose_native_headless_process_from_environment()
-> Result<ApplicationServices, HeadlessCompositionError> {
    compose_headless_from_environment_with_policy(HeadlessEnvironmentPolicy::NativeHeadlessProcess)
}

/// Selects the exact Phase 1 Portal + local-standalone policy for the
/// owner-invoked native Dioxus desktop test and replaces only its unavailable
/// desktop scanner with the one-shot test adapter.
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop-portal-test"))]
pub fn compose_native_desktop_test_from_environment()
-> Result<ApplicationServices, HeadlessCompositionError> {
    let mut services = compose_headless_from_environment_with_policy(
        HeadlessEnvironmentPolicy::NativeHeadlessProcess,
    )?;
    services.qr_scanner = Arc::new(DesktopPortalTestQrScanner::default());
    Ok(services)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn compose_headless_from_environment_with_policy(
    policy: HeadlessEnvironmentPolicy,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    assemble_headless_environment(load_headless_environment_plan(policy)?)
}

#[cfg(not(target_arch = "wasm32"))]
fn assemble_headless_environment(
    plan: HeadlessEnvironmentPlan,
) -> Result<ApplicationServices, HeadlessCompositionError> {
    match plan.midnight_config {
        Some(HeadlessMidnightConfig::Indexer(config))
            if plan.dust_checkpoints.is_none() && plan.submission_journal.is_none() =>
        {
            Ok(
                compose_headless_live_with_checkpoint_options_and_presentation(
                    config,
                    plan.checkpoints,
                    plan.shielded_checkpoints,
                    plan.credential_presentation,
                ),
            )
        }
        Some(HeadlessMidnightConfig::Standalone(config)) => {
            #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
            if let Some(portal) = plan.portal {
                // This bounded branch changes only the Midnight and Portal adapters.
                // The shared headless profile, DID, and encrypted credential repository
                // constructors still resolve their validated environment paths; every
                // vault/checkpoint/journal override was rejected while loading the plan.
                return Ok(compose_development_portal_from_config(
                    config,
                    portal,
                    plan.credential_presentation,
                ));
            }
            let passport_vault_source = plan
                .passport_vault_deployment_height
                .map(|height| {
                    AuthenticatedPassportVaultStateSource::new_with_indexer(
                        config.indexer_http_url(),
                        config.node_websocket_url(),
                        height,
                    )
                    .map(Arc::new)
                })
                .transpose()
                .map_err(HeadlessCompositionError::InvalidPassportVaultHistoryConfiguration)?;
            let services = compose_headless_standalone_with_checkpoint_options_and_presentation(
                config,
                plan.checkpoints,
                plan.dust_checkpoints,
                plan.shielded_checkpoints,
                plan.submission_journal,
                plan.credential_presentation,
            );
            let Some(source) = passport_vault_source else {
                return Ok(services);
            };
            let state_source: Arc<dyn PassportVaultContractStateSourcePort> = source.clone();
            let services = with_passport_vault_state_source(services, Some(state_source.clone()));
            let Some(composer) = plan.passport_vault_composer else {
                return Ok(services);
            };
            let chain_source: Arc<dyn PassportVaultCallChainContextSource> = source;
            with_native_passport_vault_calls(services, state_source, chain_source, composer)
                .map_err(HeadlessCompositionError::InvalidPassportVaultComposerConfiguration)
        }
        Some(HeadlessMidnightConfig::Indexer(_))
            if plan.checkpoints.is_some()
                || plan.dust_checkpoints.is_some()
                || plan.shielded_checkpoints.is_some()
                || plan.submission_journal.is_some() =>
        {
            Err(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        }
        None if plan.checkpoints.is_some()
            || plan.dust_checkpoints.is_some()
            || plan.shielded_checkpoints.is_some() =>
        {
            Err(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        }
        None => {
            #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
            if let Some(portal) = plan.portal {
                return Ok(compose_headless_with_credential_profile(
                    plan.credential_presentation,
                    HeadlessCredentialProfile::Portal(Box::new(portal)),
                ));
            }
            Ok(plan.submission_journal.map_or_else(
                || compose_headless_with_presentation(plan.credential_presentation.clone()),
                |journal| {
                    compose_headless_with_submission_journal_and_presentation(
                        journal,
                        plan.credential_presentation.clone(),
                    )
                },
            ))
        }
        Some(HeadlessMidnightConfig::Indexer(_)) => {
            Err(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        }
    }
}
