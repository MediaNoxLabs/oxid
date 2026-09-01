// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_adapter_did_midnight::{StandaloneDidLifecycle, StandaloneDidResolver};
use oxid_adapter_midnight::protected_simulated_midnight_wallet;

use super::identity::{
    CredentialIssuanceComposition, CredentialPresentationComposition, IdentityAdapters,
    SelfIssuedAuthenticationComposition,
};
use super::passport_vault::PassportVaultRepositoryComposition;
#[cfg(not(target_arch = "wasm32"))]
use super::passport_vault::with_simulated_passport_vault_calls;
use super::services::ApplicationServices;
use super::wiring::compose_with_identity_adapters;
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
use oxid_adapter_storage_memory::{
    InMemoryCredentialRepository, InMemoryDidRecordRepository, InMemoryWalletProfileRepository,
};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_vc_midnight::{
    CompactPresentationArtifactsConfig, CompactPresentationRuntimeError,
    NativeCompactPresentationRuntime,
};
use oxid_adapter_vc_midnight::{
    DigitalPassportDisclosureAdapter, MidnightCredentialVerifier, StandaloneCredentialInbox,
    standalone_digital_passport_issuer_trust_anchor,
};
use oxid_identity_application::{DidJubjubChallengeSigningPort, DidLifecyclePort};
use oxid_wallet_application::{WalletJubjubChallengeSigningPort, WalletKeyOperationPort};

/// Wires deterministic process-local services for tests and development tools.
#[must_use]
pub fn compose_in_memory() -> ApplicationServices {
    compose_in_memory_with_presentation(CredentialPresentationComposition::Standalone)
}

#[cfg(test)]
#[path = "profile_in_memory/tests.rs"]
mod tests;

/// Wires deterministic process-local services to one authenticated Compact
/// presentation artifact set. This is the standalone end-to-end proof harness;
/// normal production and mobile composition remain fail-closed.
#[cfg(not(target_arch = "wasm32"))]
pub fn compose_in_memory_with_compact_presentation_artifacts(
    root: impl Into<std::path::PathBuf>,
) -> Result<ApplicationServices, CompactPresentationRuntimeError> {
    let config = CompactPresentationArtifactsConfig::new(root)?;
    let runtime = NativeCompactPresentationRuntime::load(&config)?;
    Ok(compose_in_memory_with_presentation(
        CredentialPresentationComposition::StandaloneZk(Arc::new(runtime)),
    ))
}

pub(super) fn compose_in_memory_with_presentation(
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let midnight = Arc::new(
        protected_simulated_midnight_wallet(Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    let key_operations: Arc<dyn WalletKeyOperationPort> = security.clone();
    let challenge_signing: Arc<dyn WalletJubjubChallengeSigningPort> = security.clone();
    let did_lifecycle = Arc::new(StandaloneDidLifecycle::with_jubjub_challenge_signing(
        key_operations,
        challenge_signing,
    ));
    let did_lifecycle_port: Arc<dyn DidLifecyclePort> = did_lifecycle.clone();
    let did_jubjub_challenge_signing: Arc<dyn DidJubjubChallengeSigningPort> = did_lifecycle;
    let services = compose_with_identity_adapters(
        profiles,
        security,
        midnight,
        IdentityAdapters {
            did_repository: Arc::new(InMemoryDidRecordRepository::new()),
            did_resolver: Arc::new(StandaloneDidResolver),
            did_lifecycle: did_lifecycle_port,
            did_jubjub_challenge_signing,
            credential_repository: Arc::new(InMemoryCredentialRepository::new()),
            credential_inbox: Arc::new(StandaloneCredentialInbox),
            credential_verifier: Arc::new(MidnightCredentialVerifier::with_compact_policy(
                Arc::new(StandaloneDidResolver),
                Arc::new(StandaloneDidResolver),
                clock.clone(),
                standalone_digital_passport_issuer_trust_anchor(),
            )),
            credential_disclosure: Arc::new(DigitalPassportDisclosureAdapter),
            credential_issuance: CredentialIssuanceComposition::Standalone,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Standalone,
            credential_presentation,
            portal_test_ingress: None,
        },
        PassportVaultRepositoryComposition::process_local(),
    );
    #[cfg(not(target_arch = "wasm32"))]
    {
        with_simulated_passport_vault_calls(services)
    }
    #[cfg(target_arch = "wasm32")]
    {
        services
    }
}
