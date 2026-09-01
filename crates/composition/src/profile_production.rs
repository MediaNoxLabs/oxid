// SPDX-License-Identifier: Apache-2.0

use std::{fmt, sync::Arc};

#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_deployment_profile::AuthenticatedDeploymentProfile;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_did_midnight::{HttpDidResolver, HttpDidResolverConfig};
use oxid_adapter_midnight::unavailable_midnight_wallet;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{
    MidnightStandaloneConfig, authenticate_midnight_chain_identity,
    configuration_placeholder_address, protected_standalone_midnight_wallet,
};

use super::identity::{
    CredentialIssuanceComposition, CredentialPresentationComposition, IdentityAdapters,
    SelfIssuedAuthenticationComposition,
};
use super::passport_vault::PassportVaultRepositoryComposition;
use super::services::ApplicationServices;
use super::wiring::compose_with_identity_adapters;
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_platform_system::OsRandom;
use oxid_adapter_platform_system::SystemClock;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use oxid_adapter_storage_dev::UnavailableWalletSecurity;
use oxid_adapter_storage_json::JsonWalletProfileRepository;
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_storage_mobile::MobileWalletSecurity;
use oxid_credential_application::{
    UnavailableCredentialDisclosure, UnavailableCredentialInbox, UnavailableCredentialRepository,
    UnavailableCredentialVerifier,
};
use oxid_identity_application::{
    UnavailableDidLifecycle, UnavailableDidRecordRepository, UnavailableDidResolver,
};

/// A signed deployment profile after the configured node has also proven the
/// exact genesis hash bound by that profile.
#[cfg(not(target_arch = "wasm32"))]
pub struct AuthenticatedProductionDeployment {
    pub(super) profile: AuthenticatedDeploymentProfile,
    pub(super) midnight: MidnightStandaloneConfig,
}

#[cfg(test)]
#[path = "profile_production/tests.rs"]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
impl fmt::Debug for AuthenticatedProductionDeployment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedProductionDeployment")
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AuthenticatedProductionDeployment {
    #[must_use]
    pub const fn profile(&self) -> &AuthenticatedDeploymentProfile {
        &self.profile
    }
}

/// Payload-free failures from the production deployment composition gate.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductionDeploymentCompositionError {
    InvalidMidnightProfile,
    ChainIdentityUnavailable,
    ChainIdentityMismatch,
    InvalidSsiProfile,
}

#[cfg(not(target_arch = "wasm32"))]
impl fmt::Display for ProductionDeploymentCompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidMidnightProfile => "authenticated Midnight deployment profile is invalid",
            Self::ChainIdentityUnavailable => {
                "authenticated Midnight chain identity is unavailable"
            }
            Self::ChainIdentityMismatch => {
                "authenticated Midnight chain identity does not match the node"
            }
            Self::InvalidSsiProfile => "authenticated SSI deployment profile is invalid",
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for ProductionDeploymentCompositionError {}

/// Binds a signed deployment profile to the genesis hash returned by its
/// reviewed node route. The caller cannot provide alternate endpoints after
/// this asynchronous gate succeeds.
#[cfg(not(target_arch = "wasm32"))]
pub async fn authenticate_production_deployment(
    profile: AuthenticatedDeploymentProfile,
) -> Result<AuthenticatedProductionDeployment, ProductionDeploymentCompositionError> {
    let midnight = profile.midnight();
    let placeholder = configuration_placeholder_address(midnight.network_id())
        .map_err(|_| ProductionDeploymentCompositionError::InvalidMidnightProfile)?;
    let config = MidnightStandaloneConfig::new(
        midnight.network_id(),
        midnight.indexer_websocket_url(),
        midnight.indexer_http_url(),
        midnight.node_websocket_url(),
        midnight.proof_server_url(),
        placeholder.value(),
    )
    .map_err(|_| ProductionDeploymentCompositionError::InvalidMidnightProfile)?;
    authenticate_midnight_chain_identity(midnight.node_websocket_url(), midnight.genesis_hash())
        .await
        .map_err(|error| match error {
            oxid_adapter_midnight::MidnightChainIdentityError::GenesisMismatch => {
                ProductionDeploymentCompositionError::ChainIdentityMismatch
            }
            oxid_adapter_midnight::MidnightChainIdentityError::InvalidNodeEndpoint
            | oxid_adapter_midnight::MidnightChainIdentityError::NodeUnavailable => {
                ProductionDeploymentCompositionError::ChainIdentityUnavailable
            }
        })?;
    Ok(AuthenticatedProductionDeployment {
        profile,
        midnight: config,
    })
}

/// Composes the live Midnight path only after profile-signature and node
/// genesis authentication. The default [`compose`] function remains
/// fail-closed and never calls this opt-in constructor.
///
/// The authenticated DID resolver is enabled from the same signed profile.
/// Issuer and verifier HTTP protocol adapters remain unavailable until their
/// independent metadata/transport implementation is reviewed.
#[cfg(not(target_arch = "wasm32"))]
pub fn compose_authenticated_production(
    deployment: AuthenticatedProductionDeployment,
) -> Result<ApplicationServices, ProductionDeploymentCompositionError> {
    let did_resolver = HttpDidResolverConfig::new(deployment.profile.ssi().did_resolver_url())
        .map(HttpDidResolver::new)
        .map_err(|_| ProductionDeploymentCompositionError::InvalidSsiProfile)?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let security = {
        let clock = Arc::new(SystemClock);
        let random = Arc::new(OsRandom);
        Arc::new(MobileWalletSecurity::native(clock, random))
    };
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let security = Arc::new(UnavailableWalletSecurity);
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let clock = Arc::new(SystemClock);
    let midnight = Arc::new(
        protected_standalone_midnight_wallet(
            deployment.midnight,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    Ok(compose_with_identity_adapters(
        profiles,
        security,
        midnight,
        IdentityAdapters {
            did_repository: Arc::new(UnavailableDidRecordRepository),
            did_resolver: Arc::new(did_resolver),
            did_lifecycle: Arc::new(UnavailableDidLifecycle),
            did_jubjub_challenge_signing: Arc::new(UnavailableDidLifecycle),
            credential_repository: Arc::new(UnavailableCredentialRepository),
            credential_inbox: Arc::new(UnavailableCredentialInbox),
            credential_verifier: Arc::new(UnavailableCredentialVerifier),
            credential_disclosure: Arc::new(UnavailableCredentialDisclosure),
            credential_issuance: CredentialIssuanceComposition::Unavailable,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Unavailable,
            credential_presentation: CredentialPresentationComposition::Unavailable,
            portal_test_ingress: None,
        },
        PassportVaultRepositoryComposition::unavailable(),
    ))
}

/// Wires the application with persistent public-profile metadata storage.
#[must_use]
pub fn compose() -> ApplicationServices {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let security = {
        let clock = Arc::new(SystemClock);
        let random = Arc::new(OsRandom);
        Arc::new(MobileWalletSecurity::native(clock, random))
    };
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let security = Arc::new(UnavailableWalletSecurity);
    compose_with_identity_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        Arc::new(unavailable_midnight_wallet()),
        IdentityAdapters {
            did_repository: Arc::new(UnavailableDidRecordRepository),
            did_resolver: Arc::new(UnavailableDidResolver),
            did_lifecycle: Arc::new(UnavailableDidLifecycle),
            did_jubjub_challenge_signing: Arc::new(UnavailableDidLifecycle),
            credential_repository: Arc::new(UnavailableCredentialRepository),
            credential_inbox: Arc::new(UnavailableCredentialInbox),
            credential_verifier: Arc::new(UnavailableCredentialVerifier),
            credential_disclosure: Arc::new(UnavailableCredentialDisclosure),
            credential_issuance: CredentialIssuanceComposition::Unavailable,
            self_issued_authentication: SelfIssuedAuthenticationComposition::Unavailable,
            credential_presentation: CredentialPresentationComposition::Unavailable,
            portal_test_ingress: None,
        },
        PassportVaultRepositoryComposition::unavailable(),
    )
}
