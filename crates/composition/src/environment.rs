// SPDX-License-Identifier: Apache-2.0

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
use super::portal::PortalIdentityConfiguration;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_did_midnight::{HttpDidResolverConfig, HttpDidResolverConfigError};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{
    MidnightAccountCheckpointConfig, MidnightAccountCheckpointConfigError,
    MidnightDustCheckpointConfig, MidnightDustCheckpointConfigError, MidnightIndexerConfig,
    MidnightIndexerConfigError, MidnightLocalProvingConfig, MidnightLocalProvingConfigError,
    MidnightShieldedCheckpointConfig, MidnightShieldedCheckpointConfigError,
    MidnightStandaloneConfig, MidnightStandaloneConfigError, MidnightSubmissionJournalConfig,
    MidnightSubmissionJournalConfigError,
};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_passport_vault::{
    AuthenticatedPassportVaultStateConfigError, PassportVaultCallComposerConfigError,
    PassportVaultStoreConfig, PassportVaultStoreConfigError,
};
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_vc_midnight::{
    CompactPresentationArtifactsConfig, CompactPresentationRuntimeError,
    NativeCompactPresentationRuntime,
};

#[cfg(not(target_arch = "wasm32"))]
use super::identity::CredentialPresentationComposition;

/// Environment variable holding the selected Midnight network identity.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_NETWORK_ID_ENV: &str = "OXID_MIDNIGHT_NETWORK_ID";
/// Environment variable holding the standalone indexer GraphQL WebSocket route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_INDEXER_WS_URL_ENV: &str = "OXID_MIDNIGHT_INDEXER_WS_URL";
/// Environment variable holding the public unshielded address to observe.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_UNSHIELDED_ADDRESS_ENV: &str = "OXID_MIDNIGHT_UNSHIELDED_ADDRESS";
/// Environment variable holding the standalone indexer GraphQL HTTP route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_INDEXER_HTTP_URL_ENV: &str = "OXID_MIDNIGHT_INDEXER_HTTP_URL";
/// Environment variable holding the standalone Midnight node WebSocket route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_NODE_WS_URL_ENV: &str = "OXID_MIDNIGHT_NODE_WS_URL";
/// Environment variable holding the standalone Midnight proof-server base route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_PROOF_SERVER_URL_ENV: &str = "OXID_MIDNIGHT_PROOF_SERVER_URL";
/// Environment variable holding the app-private authenticated proving cache.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_PROVING_CACHE_DIR_ENV: &str = "OXID_MIDNIGHT_PROVING_CACHE_DIR";
/// Environment variable holding the app-private public-account checkpoint file.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_ACCOUNT_CHECKPOINT_PATH_ENV: &str = "OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH";
/// Environment variable holding the app-private key-scoped DUST checkpoint file.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_DUST_CHECKPOINT_PATH_ENV: &str = "OXID_MIDNIGHT_DUST_CHECKPOINT_PATH";
/// Environment variable holding the app-private key-scoped shielded checkpoint file.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_SHIELDED_CHECKPOINT_PATH_ENV: &str = "OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH";
/// Environment variable holding the app-private public submission journal.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_SUBMISSION_JOURNAL_PATH_ENV: &str = "OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH";
/// Environment variable holding the explicitly trusted Midnight DID resolver base route.
#[cfg(not(target_arch = "wasm32"))]
pub const MIDNIGHT_DID_RESOLVER_URL_ENV: &str = "OXID_MIDNIGHT_DID_RESOLVER_URL";
/// Environment variable holding the untrusted Passport Vault deployment-height hint.
#[cfg(not(target_arch = "wasm32"))]
pub const PASSPORT_VAULT_DEPLOYMENT_HEIGHT_ENV: &str = "OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT";
/// Environment variable holding the immutable packaged Passport Vault call composer.
#[cfg(not(target_arch = "wasm32"))]
pub const PASSPORT_VAULT_COMPOSER_ENV: &str = "OXID_PASSPORT_VAULT_COMPOSER";
/// Environment variable holding the immutable Compact presentation artifact root.
#[cfg(not(target_arch = "wasm32"))]
pub const PRESENTATION_COMPACT_ARTIFACTS_DIR_ENV: &str = "OXID_PRESENTATION_ARTIFACTS_DIR";
/// Environment variable holding the app-private public DID record file.
pub const DID_STORE_PATH_ENV: &str = "OXID_DID_STORE_PATH";
/// Environment variable holding the app-private encrypted credential file.
pub const CREDENTIAL_STORE_PATH_ENV: &str = "OXID_CREDENTIAL_STORE_PATH";
/// Environment variable holding the development-only credential wrapping key.
pub const CREDENTIAL_KEY_PATH_ENV: &str = "OXID_CREDENTIAL_KEY_PATH";
/// Environment variable holding the absolute authenticated Portal deployment manifest path.
#[cfg(not(target_arch = "wasm32"))]
pub const OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH_ENV: &str =
    "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH";
/// Environment variable holding the expected SHA-256 of the exact Portal deployment manifest.
#[cfg(not(target_arch = "wasm32"))]
pub const OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256_ENV: &str =
    "OXID_OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256";
/// Environment variable holding the owner-private standalone Passport Vault file.
#[cfg(not(target_arch = "wasm32"))]
pub const PASSPORT_VAULT_STORE_PATH_ENV: &str = "OXID_PASSPORT_VAULT_STORE_PATH";

/// Safe startup failures for optional standalone-indexer composition.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadlessCompositionError {
    IncompleteMidnightIndexerConfiguration,
    NonUnicodeMidnightIndexerConfiguration,
    InvalidMidnightIndexerConfiguration(MidnightIndexerConfigError),
    InvalidMidnightLocalProvingConfiguration(MidnightLocalProvingConfigError),
    InvalidMidnightStandaloneConfiguration(MidnightStandaloneConfigError),
    InvalidMidnightAccountCheckpointConfiguration(MidnightAccountCheckpointConfigError),
    InvalidMidnightDustCheckpointConfiguration(MidnightDustCheckpointConfigError),
    InvalidMidnightShieldedCheckpointConfiguration(MidnightShieldedCheckpointConfigError),
    InvalidMidnightSubmissionJournalConfiguration(MidnightSubmissionJournalConfigError),
    InvalidMidnightDidResolverConfiguration(HttpDidResolverConfigError),
    InvalidPassportVaultDeploymentHeight,
    InvalidPassportVaultHistoryConfiguration(AuthenticatedPassportVaultStateConfigError),
    InvalidPassportVaultComposerConfiguration(PassportVaultCallComposerConfigError),
    InvalidPassportVaultStoreConfiguration(PassportVaultStoreConfigError),
    PassportVaultHistoryRequiresStandalone,
    InvalidCompactPresentationRuntime(CompactPresentationRuntimeError),
    IncompleteCredentialStoreConfiguration,
    IncompletePortalConfiguration,
    PortalConfigurationUnavailable,
    PortalRequiresStandaloneSimulation,
    InvalidPortalConfiguration,
    InvalidStandaloneDeploymentProfile,
    PublicStandaloneGenesisRequiresUndeployed,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for HeadlessCompositionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::IncompleteMidnightIndexerConfiguration => {
                "Midnight live mode requires the read-only indexer values or every submission route plus exactly one local-cache or remote-prover setting"
            }
            Self::NonUnicodeMidnightIndexerConfiguration => {
                "Midnight live-mode configuration must be valid Unicode"
            }
            Self::InvalidMidnightIndexerConfiguration(error) => return error.fmt(formatter),
            Self::InvalidMidnightLocalProvingConfiguration(error) => return error.fmt(formatter),
            Self::InvalidMidnightStandaloneConfiguration(error) => return error.fmt(formatter),
            Self::InvalidMidnightAccountCheckpointConfiguration(error) => {
                return error.fmt(formatter);
            }
            Self::InvalidMidnightDustCheckpointConfiguration(error) => return error.fmt(formatter),
            Self::InvalidMidnightShieldedCheckpointConfiguration(error) => {
                return error.fmt(formatter);
            }
            Self::InvalidMidnightSubmissionJournalConfiguration(error) => {
                return error.fmt(formatter);
            }
            Self::InvalidMidnightDidResolverConfiguration(error) => return error.fmt(formatter),
            Self::InvalidPassportVaultDeploymentHeight => {
                "Passport Vault deployment height must be a non-zero unsigned integer"
            }
            Self::InvalidPassportVaultHistoryConfiguration(error) => return error.fmt(formatter),
            Self::InvalidPassportVaultComposerConfiguration(error) => return error.fmt(formatter),
            Self::InvalidPassportVaultStoreConfiguration(error) => return error.fmt(formatter),
            Self::PassportVaultHistoryRequiresStandalone => {
                "Passport Vault canonical replay requires the complete standalone Midnight routes"
            }
            Self::InvalidCompactPresentationRuntime(error) => return error.fmt(formatter),
            Self::IncompleteCredentialStoreConfiguration => {
                "credential store and key paths must be configured together"
            }
            Self::IncompletePortalConfiguration => {
                "Portal manifest path and digest must be configured together"
            }
            Self::PortalConfigurationUnavailable => {
                "Portal issuance is available only to native desktop headless development"
            }
            Self::PortalRequiresStandaloneSimulation => {
                "Portal issuance cannot be combined with live Midnight or alternate resolver configuration"
            }
            Self::InvalidPortalConfiguration => "invalid Portal deployment configuration",
            Self::InvalidStandaloneDeploymentProfile => "invalid standalone deployment profile",
            Self::PublicStandaloneGenesisRequiresUndeployed => {
                "public standalone genesis custody requires the undeployed network"
            }
        };
        formatter.write_str(message)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for HeadlessCompositionError {}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HeadlessEnvironmentPolicy {
    General,
    #[cfg(any(feature = "headless-portal-local", feature = "desktop-portal-test"))]
    NativeHeadlessProcess,
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "ios", target_os = "android"))
))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PortalAdjacentEnvironmentSettings {
    presentation_artifacts: bool,
    midnight_did_resolver: bool,
    account_checkpoint: bool,
    dust_checkpoint: bool,
    shielded_checkpoint: bool,
    submission_journal: bool,
    passport_vault_deployment_height: bool,
    passport_vault_composer: bool,
    passport_vault_store: bool,
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "ios", target_os = "android"))
))]
impl PortalAdjacentEnvironmentSettings {
    fn conflicts_with_general_policy(self) -> bool {
        self.midnight_did_resolver
            || self.account_checkpoint
            || self.dust_checkpoint
            || self.shielded_checkpoint
            || self.submission_journal
            || self.passport_vault_deployment_height
            || self.passport_vault_composer
    }

    fn any(self) -> bool {
        self.presentation_artifacts
            || self.conflicts_with_general_policy()
            || self.passport_vault_store
    }

    #[cfg(all(
        test,
        any(feature = "headless-portal-local", feature = "desktop-portal-test")
    ))]
    pub(super) fn each_conflict() -> [Self; 9] {
        [
            Self {
                presentation_artifacts: true,
                ..Self::default()
            },
            Self {
                midnight_did_resolver: true,
                ..Self::default()
            },
            Self {
                account_checkpoint: true,
                ..Self::default()
            },
            Self {
                dust_checkpoint: true,
                ..Self::default()
            },
            Self {
                shielded_checkpoint: true,
                ..Self::default()
            },
            Self {
                submission_journal: true,
                ..Self::default()
            },
            Self {
                passport_vault_deployment_height: true,
                ..Self::default()
            },
            Self {
                passport_vault_composer: true,
                ..Self::default()
            },
            Self {
                passport_vault_store: true,
                ..Self::default()
            },
        ]
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "ios", target_os = "android"))
))]
pub(super) fn validate_portal_environment_combination(
    policy: HeadlessEnvironmentPolicy,
    midnight_values: &[Option<String>; 7],
    adjacent: &PortalAdjacentEnvironmentSettings,
) -> Result<(), HeadlessCompositionError> {
    let no_midnight = midnight_values.iter().all(Option::is_none);
    if no_midnight {
        return if adjacent.conflicts_with_general_policy() {
            Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation)
        } else {
            Ok(())
        };
    }
    if matches!(policy, HeadlessEnvironmentPolicy::General) || adjacent.any() {
        return Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation);
    }
    #[cfg(any(feature = "headless-portal-local", feature = "desktop-portal-test"))]
    {
        let placeholder = oxid_adapter_midnight::standalone_configuration_placeholder_address()
            .map_err(|_| HeadlessCompositionError::PortalRequiresStandaloneSimulation)?;
        let expected = [
            Some("undeployed"),
            Some("ws://127.0.0.1:8088/api/v4/graphql/ws"),
            Some("http://127.0.0.1:8088/api/v4/graphql"),
            Some("ws://127.0.0.1:9944"),
            Some("http://127.0.0.1:6300"),
            Some(placeholder.value()),
            None,
        ];
        if midnight_values
            .iter()
            .map(|value| value.as_deref())
            .eq(expected)
        {
            Ok(())
        } else {
            Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation)
        }
    }
    #[cfg(not(any(feature = "headless-portal-local", feature = "desktop-portal-test")))]
    Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct HeadlessEnvironmentPlan {
    #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
    pub(super) portal: Option<PortalIdentityConfiguration>,
    pub(super) credential_presentation: CredentialPresentationComposition,
    pub(super) midnight_config: Option<HeadlessMidnightConfig>,
    pub(super) checkpoints: Option<MidnightAccountCheckpointConfig>,
    pub(super) dust_checkpoints: Option<MidnightDustCheckpointConfig>,
    pub(super) shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    pub(super) submission_journal: Option<MidnightSubmissionJournalConfig>,
    pub(super) passport_vault_deployment_height: Option<u64>,
    pub(super) passport_vault_composer: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn load_headless_environment_plan(
    policy: HeadlessEnvironmentPolicy,
) -> Result<HeadlessEnvironmentPlan, HeadlessCompositionError> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let _ = policy;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if std::env::var_os(OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH_ENV).is_some()
        || std::env::var_os(OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256_ENV).is_some()
    {
        return Err(HeadlessCompositionError::PortalConfigurationUnavailable);
    }
    #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
    let portal = parse_optional_portal_configuration()?;
    let presentation_artifacts = read_optional_environment(PRESENTATION_COMPACT_ARTIFACTS_DIR_ENV)?;
    let credential_presentation = presentation_artifacts
        .as_deref()
        .map(|root| {
            CompactPresentationArtifactsConfig::new(root)
                .and_then(|config| NativeCompactPresentationRuntime::load(&config))
                .map(Arc::new)
        })
        .transpose()
        .map_err(HeadlessCompositionError::InvalidCompactPresentationRuntime)?
        .map_or(CredentialPresentationComposition::Standalone, |runtime| {
            CredentialPresentationComposition::StandaloneZk(runtime)
        });
    let credential_paths = (
        read_optional_environment(CREDENTIAL_STORE_PATH_ENV)?,
        read_optional_environment(CREDENTIAL_KEY_PATH_ENV)?,
    );
    if matches!(credential_paths, (Some(_), None) | (None, Some(_))) {
        return Err(HeadlessCompositionError::IncompleteCredentialStoreConfiguration);
    }
    let passport_vault_store = read_optional_environment(PASSPORT_VAULT_STORE_PATH_ENV)?;
    passport_vault_store
        .as_deref()
        .map(PassportVaultStoreConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidPassportVaultStoreConfiguration)?;
    let midnight_did_resolver = read_optional_environment(MIDNIGHT_DID_RESOLVER_URL_ENV)?
        .map(HttpDidResolverConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightDidResolverConfiguration)?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let _ = &midnight_did_resolver;
    let values = [
        read_optional_environment(MIDNIGHT_NETWORK_ID_ENV)?,
        read_optional_environment(MIDNIGHT_INDEXER_WS_URL_ENV)?,
        read_optional_environment(MIDNIGHT_INDEXER_HTTP_URL_ENV)?,
        read_optional_environment(MIDNIGHT_NODE_WS_URL_ENV)?,
        read_optional_environment(MIDNIGHT_PROOF_SERVER_URL_ENV)?,
        read_optional_environment(MIDNIGHT_UNSHIELDED_ADDRESS_ENV)?,
        read_optional_environment(MIDNIGHT_PROVING_CACHE_DIR_ENV)?,
    ];
    let checkpoints = read_optional_environment(MIDNIGHT_ACCOUNT_CHECKPOINT_PATH_ENV)?
        .map(MidnightAccountCheckpointConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightAccountCheckpointConfiguration)?;
    let dust_checkpoints = read_optional_environment(MIDNIGHT_DUST_CHECKPOINT_PATH_ENV)?
        .map(MidnightDustCheckpointConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightDustCheckpointConfiguration)?;
    let shielded_checkpoints = read_optional_environment(MIDNIGHT_SHIELDED_CHECKPOINT_PATH_ENV)?
        .map(MidnightShieldedCheckpointConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightShieldedCheckpointConfiguration)?;
    let submission_journal = read_optional_environment(MIDNIGHT_SUBMISSION_JOURNAL_PATH_ENV)?
        .map(MidnightSubmissionJournalConfig::new)
        .transpose()
        .map_err(HeadlessCompositionError::InvalidMidnightSubmissionJournalConfiguration)?;
    let passport_vault_deployment_height = parse_optional_passport_vault_deployment_height(
        read_optional_environment(PASSPORT_VAULT_DEPLOYMENT_HEIGHT_ENV)?,
    )?;
    let passport_vault_composer = read_optional_environment(PASSPORT_VAULT_COMPOSER_ENV)?;
    #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
    if portal.is_some() {
        validate_portal_environment_combination(
            policy,
            &values,
            &PortalAdjacentEnvironmentSettings {
                presentation_artifacts: presentation_artifacts.is_some(),
                midnight_did_resolver: midnight_did_resolver.is_some(),
                account_checkpoint: checkpoints.is_some(),
                dust_checkpoint: dust_checkpoints.is_some(),
                shielded_checkpoint: shielded_checkpoints.is_some(),
                submission_journal: submission_journal.is_some(),
                passport_vault_deployment_height: passport_vault_deployment_height.is_some(),
                passport_vault_composer: passport_vault_composer.is_some(),
                passport_vault_store: passport_vault_store.is_some(),
            },
        )?;
    }
    let midnight_config = parse_optional_midnight_config(values)?;
    if passport_vault_deployment_height.is_some()
        && !matches!(
            &midnight_config,
            Some(HeadlessMidnightConfig::Standalone(_))
        )
    {
        return Err(HeadlessCompositionError::PassportVaultHistoryRequiresStandalone);
    }
    Ok(HeadlessEnvironmentPlan {
        #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
        portal,
        credential_presentation,
        midnight_config,
        checkpoints,
        dust_checkpoints,
        shielded_checkpoints,
        submission_journal,
        passport_vault_deployment_height,
        passport_vault_composer,
    })
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "android")
))]
pub(super) fn parse_optional_portal_configuration()
-> Result<Option<PortalIdentityConfiguration>, HeadlessCompositionError> {
    let values = (
        read_optional_environment(OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_PATH_ENV)?,
        read_optional_environment(OPENID4VCI_PORTAL_DEPLOYMENT_MANIFEST_SHA256_ENV)?,
    );
    match values {
        (None, None) => Ok(None),
        (Some(path), Some(digest)) => PortalIdentityConfiguration::from_file(&path, &digest)
            .map(Some)
            .map_err(|_| HeadlessCompositionError::InvalidPortalConfiguration),
        _ => Err(HeadlessCompositionError::IncompletePortalConfiguration),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn read_optional_environment(
    key: &str,
) -> Result<Option<String>, HeadlessCompositionError> {
    std::env::var_os(key)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| HeadlessCompositionError::NonUnicodeMidnightIndexerConfiguration)
        })
        .transpose()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) enum HeadlessMidnightConfig {
    Indexer(MidnightIndexerConfig),
    Standalone(MidnightStandaloneConfig),
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn parse_optional_midnight_config(
    values: [Option<String>; 7],
) -> Result<Option<HeadlessMidnightConfig>, HeadlessCompositionError> {
    let [
        network_id,
        indexer_ws,
        indexer_http,
        node_ws,
        proof_server,
        address,
        proving_cache,
    ] = values;
    match (
        network_id,
        indexer_ws,
        indexer_http,
        node_ws,
        proof_server,
        address,
        proving_cache,
    ) {
        (None, None, None, None, None, None, None) => Ok(None),
        (Some(network), Some(indexer_ws), None, None, None, Some(address), None) => {
            MidnightIndexerConfig::new(network, indexer_ws, address)
                .map(HeadlessMidnightConfig::Indexer)
                .map(Some)
                .map_err(HeadlessCompositionError::InvalidMidnightIndexerConfiguration)
        }
        (
            Some(network),
            Some(indexer_ws),
            Some(indexer_http),
            Some(node_ws),
            Some(proof_server),
            Some(address),
            None,
        ) => MidnightStandaloneConfig::new(
            network,
            indexer_ws,
            indexer_http,
            node_ws,
            proof_server,
            address,
        )
        .map(HeadlessMidnightConfig::Standalone)
        .map(Some)
        .map_err(HeadlessCompositionError::InvalidMidnightStandaloneConfiguration),
        (
            Some(network),
            Some(indexer_ws),
            Some(indexer_http),
            Some(node_ws),
            None,
            Some(address),
            Some(proving_cache),
        ) => {
            let local_proving = MidnightLocalProvingConfig::new(proving_cache)
                .map_err(HeadlessCompositionError::InvalidMidnightLocalProvingConfiguration)?;
            MidnightStandaloneConfig::new_private(
                network,
                indexer_ws,
                indexer_http,
                node_ws,
                local_proving,
                address,
            )
            .map(HeadlessMidnightConfig::Standalone)
            .map(Some)
            .map_err(HeadlessCompositionError::InvalidMidnightStandaloneConfiguration)
        }
        _ => Err(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn parse_optional_passport_vault_deployment_height(
    value: Option<String>,
) -> Result<Option<u64>, HeadlessCompositionError> {
    value
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|height| *height > 0)
                .ok_or(HeadlessCompositionError::InvalidPassportVaultDeploymentHeight)
        })
        .transpose()
}

#[cfg(test)]
#[path = "environment/tests.rs"]
mod tests;
