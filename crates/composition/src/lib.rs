// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{
    MidnightAccountCheckpointConfig, MidnightAccountCheckpointConfigError,
    MidnightDustCheckpointConfig, MidnightDustCheckpointConfigError, MidnightIndexerConfig,
    MidnightIndexerConfigError, MidnightLocalProvingConfig, MidnightLocalProvingConfigError,
    MidnightStandaloneConfig, MidnightStandaloneConfigError, protected_live_midnight_wallet,
    protected_live_midnight_wallet_with_checkpoints, protected_standalone_midnight_wallet,
    protected_standalone_midnight_wallet_with_all_checkpoints,
    protected_standalone_midnight_wallet_with_checkpoints,
    protected_standalone_midnight_wallet_with_dust_checkpoints,
};
use oxid_adapter_midnight::{protected_simulated_midnight_wallet, unavailable_midnight_wallet};
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_dev::{DevelopmentWalletSecurity, UnavailableWalletSecurity};
use oxid_adapter_storage_json::JsonWalletProfileRepository;
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_wallet_application::{
    AuthorizeWalletTransferUseCase, CancelWalletDustSyncUseCase, CreateWalletProfileService,
    CreateWalletProfileUseCase, DeleteWalletKeyUseCase, DeriveWalletAccountUseCase,
    GenerateWalletKeyUseCase, GetActiveWalletProfileService, GetActiveWalletProfileUseCase,
    GetWalletAccountUseCase, GetWalletDustSyncStatusUseCase, GetWalletSecurityStatusUseCase,
    GetWalletTransferDraftUseCase, InitializeWalletSecurityUseCase, ListWalletKeysUseCase,
    ListWalletNetworksUseCase, ListWalletProfilesService, ListWalletProfilesUseCase,
    LockWalletUseCase, PrepareWalletTransferUseCase, SelectWalletNetworkUseCase,
    SelectWalletProfileService, SelectWalletProfileUseCase, SignWalletDataUseCase,
    StartWalletDustSyncUseCase, SubmitWalletTransferUseCase, SyncWalletAccountUseCase,
    UnlockWalletUseCase, WalletAccountDerivationPort, WalletAccountDerivationService,
    WalletAccountReadPort, WalletAccountService, WalletDustSyncPort, WalletDustSyncService,
    WalletKeyOperationPort, WalletKeyService, WalletNetworkPort, WalletNetworkService,
    WalletProfileRepository, WalletProtectionPort, WalletProtectionService, WalletTransactionPort,
    WalletTransactionService,
};

/// Application capabilities shared by every incoming adapter.
#[derive(Clone)]
pub struct ApplicationServices {
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
    list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
    select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
    get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
    get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase>,
    initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase>,
    unlock_wallet: Arc<dyn UnlockWalletUseCase>,
    lock_wallet: Arc<dyn LockWalletUseCase>,
    generate_wallet_key: Arc<dyn GenerateWalletKeyUseCase>,
    list_wallet_keys: Arc<dyn ListWalletKeysUseCase>,
    sign_wallet_data: Arc<dyn SignWalletDataUseCase>,
    delete_wallet_key: Arc<dyn DeleteWalletKeyUseCase>,
    list_wallet_networks: Arc<dyn ListWalletNetworksUseCase>,
    select_wallet_network: Arc<dyn SelectWalletNetworkUseCase>,
    derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase>,
    get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
    sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
    get_wallet_dust_sync_status: Arc<dyn GetWalletDustSyncStatusUseCase>,
    start_wallet_dust_sync: Arc<dyn StartWalletDustSyncUseCase>,
    cancel_wallet_dust_sync: Arc<dyn CancelWalletDustSyncUseCase>,
    prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase>,
    authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase>,
    submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase>,
    get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase>,
}

impl ApplicationServices {
    #[must_use]
    pub fn create_wallet_profile(&self) -> Arc<dyn CreateWalletProfileUseCase> {
        Arc::clone(&self.create_wallet_profile)
    }

    #[must_use]
    pub fn list_wallet_profiles(&self) -> Arc<dyn ListWalletProfilesUseCase> {
        Arc::clone(&self.list_wallet_profiles)
    }

    #[must_use]
    pub fn select_wallet_profile(&self) -> Arc<dyn SelectWalletProfileUseCase> {
        Arc::clone(&self.select_wallet_profile)
    }

    #[must_use]
    pub fn get_active_wallet_profile(&self) -> Arc<dyn GetActiveWalletProfileUseCase> {
        Arc::clone(&self.get_active_wallet_profile)
    }

    #[must_use]
    pub fn get_wallet_security_status(&self) -> Arc<dyn GetWalletSecurityStatusUseCase> {
        Arc::clone(&self.get_wallet_security_status)
    }

    #[must_use]
    pub fn initialize_wallet_security(&self) -> Arc<dyn InitializeWalletSecurityUseCase> {
        Arc::clone(&self.initialize_wallet_security)
    }

    #[must_use]
    pub fn unlock_wallet(&self) -> Arc<dyn UnlockWalletUseCase> {
        Arc::clone(&self.unlock_wallet)
    }

    #[must_use]
    pub fn lock_wallet(&self) -> Arc<dyn LockWalletUseCase> {
        Arc::clone(&self.lock_wallet)
    }

    #[must_use]
    pub fn generate_wallet_key(&self) -> Arc<dyn GenerateWalletKeyUseCase> {
        Arc::clone(&self.generate_wallet_key)
    }

    #[must_use]
    pub fn list_wallet_keys(&self) -> Arc<dyn ListWalletKeysUseCase> {
        Arc::clone(&self.list_wallet_keys)
    }

    #[must_use]
    pub fn sign_wallet_data(&self) -> Arc<dyn SignWalletDataUseCase> {
        Arc::clone(&self.sign_wallet_data)
    }

    #[must_use]
    pub fn delete_wallet_key(&self) -> Arc<dyn DeleteWalletKeyUseCase> {
        Arc::clone(&self.delete_wallet_key)
    }

    #[must_use]
    pub fn list_wallet_networks(&self) -> Arc<dyn ListWalletNetworksUseCase> {
        Arc::clone(&self.list_wallet_networks)
    }

    #[must_use]
    pub fn select_wallet_network(&self) -> Arc<dyn SelectWalletNetworkUseCase> {
        Arc::clone(&self.select_wallet_network)
    }

    #[must_use]
    pub fn derive_wallet_account(&self) -> Arc<dyn DeriveWalletAccountUseCase> {
        Arc::clone(&self.derive_wallet_account)
    }

    #[must_use]
    pub fn get_wallet_account(&self) -> Arc<dyn GetWalletAccountUseCase> {
        Arc::clone(&self.get_wallet_account)
    }

    #[must_use]
    pub fn sync_wallet_account(&self) -> Arc<dyn SyncWalletAccountUseCase> {
        Arc::clone(&self.sync_wallet_account)
    }

    #[must_use]
    pub fn get_wallet_dust_sync_status(&self) -> Arc<dyn GetWalletDustSyncStatusUseCase> {
        Arc::clone(&self.get_wallet_dust_sync_status)
    }

    #[must_use]
    pub fn start_wallet_dust_sync(&self) -> Arc<dyn StartWalletDustSyncUseCase> {
        Arc::clone(&self.start_wallet_dust_sync)
    }

    #[must_use]
    pub fn cancel_wallet_dust_sync(&self) -> Arc<dyn CancelWalletDustSyncUseCase> {
        Arc::clone(&self.cancel_wallet_dust_sync)
    }

    #[must_use]
    pub fn prepare_wallet_transfer(&self) -> Arc<dyn PrepareWalletTransferUseCase> {
        Arc::clone(&self.prepare_wallet_transfer)
    }

    #[must_use]
    pub fn authorize_wallet_transfer(&self) -> Arc<dyn AuthorizeWalletTransferUseCase> {
        Arc::clone(&self.authorize_wallet_transfer)
    }

    #[must_use]
    pub fn submit_wallet_transfer(&self) -> Arc<dyn SubmitWalletTransferUseCase> {
        Arc::clone(&self.submit_wallet_transfer)
    }

    #[must_use]
    pub fn get_wallet_transfer_draft(&self) -> Arc<dyn GetWalletTransferDraftUseCase> {
        Arc::clone(&self.get_wallet_transfer_draft)
    }
}

/// Wires the application with persistent public-profile metadata storage.
#[must_use]
pub fn compose() -> ApplicationServices {
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        Arc::new(UnavailableWalletSecurity),
        Arc::new(unavailable_midnight_wallet()),
    )
}

/// Wires persistent public profiles with an explicit process-local custody
/// adapter for the standalone development harness.
#[must_use]
pub fn compose_headless() -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_simulated_midnight_wallet(
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

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
        };
        formatter.write_str(message)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for HeadlessCompositionError {}

/// Selects deterministic simulation when no live variables are present, a
/// read-only indexer when the three read values are present, or complete
/// standalone submission when every route and exactly one proving mode are valid.
#[cfg(not(target_arch = "wasm32"))]
pub fn compose_headless_from_environment() -> Result<ApplicationServices, HeadlessCompositionError>
{
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
    match (
        parse_optional_midnight_config(values)?,
        checkpoints,
        dust_checkpoints,
    ) {
        (Some(HeadlessMidnightConfig::Indexer(config)), Some(checkpoints), None) => {
            Ok(compose_headless_live_with_checkpoints(config, checkpoints))
        }
        (Some(HeadlessMidnightConfig::Standalone(config)), Some(checkpoints), None) => Ok(
            compose_headless_standalone_with_checkpoints(config, checkpoints),
        ),
        (Some(HeadlessMidnightConfig::Standalone(config)), None, Some(dust_checkpoints)) => Ok(
            compose_headless_standalone_with_dust_checkpoints(config, dust_checkpoints),
        ),
        (
            Some(HeadlessMidnightConfig::Standalone(config)),
            Some(account_checkpoints),
            Some(dust_checkpoints),
        ) => Ok(compose_headless_standalone_with_all_checkpoints(
            config,
            account_checkpoints,
            dust_checkpoints,
        )),
        (Some(HeadlessMidnightConfig::Indexer(config)), None, None) => {
            Ok(compose_headless_live(config))
        }
        (Some(HeadlessMidnightConfig::Standalone(config)), None, None) => {
            Ok(compose_headless_standalone(config))
        }
        (None, None, None) => Ok(compose_headless()),
        (Some(HeadlessMidnightConfig::Indexer(_)), _, Some(_)) | (None, _, _) => {
            Err(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        }
    }
}

/// Wires persistent public profiles and development custody to an explicitly
/// configured live standalone indexer. Normal mobile composition never calls it.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_live(config: MidnightIndexerConfig) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_live_midnight_wallet(
        config,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires development custody and a public checkpoint store to a live indexer.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_live_with_checkpoints(
    config: MidnightIndexerConfig,
    checkpoints: MidnightAccountCheckpointConfig,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_live_midnight_wallet_with_checkpoints(
        config,
        checkpoints,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires development custody to the complete, explicitly configured standalone stack.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone(config: MidnightStandaloneConfig) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_standalone_midnight_wallet(
        config,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires the complete standalone stack with durable public account checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_checkpoints(
    config: MidnightStandaloneConfig,
    checkpoints: MidnightAccountCheckpointConfig,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_standalone_midnight_wallet_with_checkpoints(
        config,
        checkpoints,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires the complete standalone stack with private key-scoped DUST checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_dust_checkpoints(
    config: MidnightStandaloneConfig,
    dust_checkpoints: MidnightDustCheckpointConfig,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_standalone_midnight_wallet_with_dust_checkpoints(
        config,
        dust_checkpoints,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

/// Wires the complete standalone stack with public account and private DUST checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_all_checkpoints(
    config: MidnightStandaloneConfig,
    account_checkpoints: MidnightAccountCheckpointConfig,
    dust_checkpoints: MidnightDustCheckpointConfig,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_standalone_midnight_wallet_with_all_checkpoints(
        config,
        account_checkpoints,
        dust_checkpoints,
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        midnight,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn read_optional_environment(key: &str) -> Result<Option<String>, HeadlessCompositionError> {
    std::env::var_os(key)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| HeadlessCompositionError::NonUnicodeMidnightIndexerConfiguration)
        })
        .transpose()
}

#[cfg(not(target_arch = "wasm32"))]
enum HeadlessMidnightConfig {
    Indexer(MidnightIndexerConfig),
    Standalone(MidnightStandaloneConfig),
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_optional_midnight_config(
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

/// Wires deterministic process-local services for tests and development tools.
#[must_use]
pub fn compose_in_memory() -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let midnight = Arc::new(protected_simulated_midnight_wallet(
        Arc::clone(&clock),
        Arc::clone(&security),
    ));
    compose_with_adapters(
        Arc::new(InMemoryWalletProfileRepository::new()),
        security,
        midnight,
    )
}

fn compose_with_adapters<R, S, M>(
    repository: Arc<R>,
    security: Arc<S>,
    midnight: Arc<M>,
) -> ApplicationServices
where
    R: WalletProfileRepository + 'static,
    S: WalletProtectionPort + WalletKeyOperationPort + 'static,
    M: WalletNetworkPort
        + WalletAccountReadPort
        + WalletAccountDerivationPort
        + WalletDustSyncPort
        + WalletTransactionPort
        + 'static,
{
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let create_wallet_profile = Arc::new(CreateWalletProfileService::new(
        Arc::clone(&repository),
        Arc::clone(&clock),
        random,
    ));
    let list_wallet_profiles = Arc::new(ListWalletProfilesService::new(Arc::clone(&repository)));
    let select_wallet_profile = Arc::new(SelectWalletProfileService::new(Arc::clone(&repository)));
    let get_active_wallet_profile = Arc::new(GetActiveWalletProfileService::new(repository));
    let protection = Arc::new(WalletProtectionService::new(Arc::clone(&security)));
    let keys = Arc::new(WalletKeyService::new(security));
    let networks = Arc::new(WalletNetworkService::new(Arc::clone(&midnight)));
    let account_derivation = Arc::new(WalletAccountDerivationService::new(Arc::clone(&midnight)));
    let accounts = Arc::new(WalletAccountService::new(Arc::clone(&midnight)));
    let dust = Arc::new(WalletDustSyncService::new(Arc::clone(&midnight)));
    let transactions = Arc::new(WalletTransactionService::new(midnight, clock));

    let get_wallet_security_status: Arc<dyn GetWalletSecurityStatusUseCase> = protection.clone();
    let initialize_wallet_security: Arc<dyn InitializeWalletSecurityUseCase> = protection.clone();
    let unlock_wallet: Arc<dyn UnlockWalletUseCase> = protection.clone();
    let lock_wallet: Arc<dyn LockWalletUseCase> = protection;
    let generate_wallet_key: Arc<dyn GenerateWalletKeyUseCase> = keys.clone();
    let list_wallet_keys: Arc<dyn ListWalletKeysUseCase> = keys.clone();
    let sign_wallet_data: Arc<dyn SignWalletDataUseCase> = keys.clone();
    let delete_wallet_key: Arc<dyn DeleteWalletKeyUseCase> = keys;
    let list_wallet_networks: Arc<dyn ListWalletNetworksUseCase> = networks.clone();
    let select_wallet_network: Arc<dyn SelectWalletNetworkUseCase> = networks;
    let derive_wallet_account: Arc<dyn DeriveWalletAccountUseCase> = account_derivation;
    let get_wallet_account: Arc<dyn GetWalletAccountUseCase> = accounts.clone();
    let sync_wallet_account: Arc<dyn SyncWalletAccountUseCase> = accounts;
    let get_wallet_dust_sync_status: Arc<dyn GetWalletDustSyncStatusUseCase> = dust.clone();
    let start_wallet_dust_sync: Arc<dyn StartWalletDustSyncUseCase> = dust.clone();
    let cancel_wallet_dust_sync: Arc<dyn CancelWalletDustSyncUseCase> = dust;
    let prepare_wallet_transfer: Arc<dyn PrepareWalletTransferUseCase> = transactions.clone();
    let authorize_wallet_transfer: Arc<dyn AuthorizeWalletTransferUseCase> = transactions.clone();
    let submit_wallet_transfer: Arc<dyn SubmitWalletTransferUseCase> = transactions.clone();
    let get_wallet_transfer_draft: Arc<dyn GetWalletTransferDraftUseCase> = transactions;

    ApplicationServices {
        create_wallet_profile,
        list_wallet_profiles,
        select_wallet_profile,
        get_active_wallet_profile,
        get_wallet_security_status,
        initialize_wallet_security,
        unlock_wallet,
        lock_wallet,
        generate_wallet_key,
        list_wallet_keys,
        sign_wallet_data,
        delete_wallet_key,
        list_wallet_networks,
        select_wallet_network,
        derive_wallet_account,
        get_wallet_account,
        sync_wallet_account,
        get_wallet_dust_sync_status,
        start_wallet_dust_sync,
        cancel_wallet_dust_sync,
        prepare_wallet_transfer,
        authorize_wallet_transfer,
        submit_wallet_transfer,
        get_wallet_transfer_draft,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_wallet_application::{
        CreateWalletProfileCommand, WalletAccountQuery, WalletDustSyncCommand,
        WalletProfileSecurityCommand,
    };

    #[test]
    fn composed_application_executes_the_vertical_slice() {
        let services = compose_in_memory();
        let result = services
            .create_wallet_profile()
            .execute(CreateWalletProfileCommand {
                display_name: "Composition smoke".to_owned(),
            })
            .expect("composed use case should succeed");

        assert_eq!(result.display_name, "Composition smoke");
        assert!(result.id.starts_with("profile_"));
        assert_eq!(
            services
                .list_wallet_profiles()
                .execute()
                .expect("composed query should succeed"),
            vec![result]
        );
    }

    #[test]
    fn composition_exposes_every_application_capability() {
        let services = compose_in_memory();

        drop(services.create_wallet_profile());
        drop(services.list_wallet_profiles());
        drop(services.select_wallet_profile());
        drop(services.get_active_wallet_profile());
        drop(services.get_wallet_security_status());
        drop(services.initialize_wallet_security());
        drop(services.unlock_wallet());
        drop(services.lock_wallet());
        drop(services.generate_wallet_key());
        drop(services.list_wallet_keys());
        drop(services.sign_wallet_data());
        drop(services.delete_wallet_key());
        drop(services.list_wallet_networks());
        drop(services.select_wallet_network());
        drop(services.derive_wallet_account());
        drop(services.get_wallet_account());
        drop(services.sync_wallet_account());
        drop(services.get_wallet_dust_sync_status());
        drop(services.start_wallet_dust_sync());
        drop(services.cancel_wallet_dust_sync());
        drop(services.prepare_wallet_transfer());
        drop(services.authorize_wallet_transfer());
        drop(services.submit_wallet_transfer());
        drop(services.get_wallet_transfer_draft());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn explicit_live_compositions_are_constructible_without_network_io() {
        const ADDRESS: &str =
            "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
        let indexer =
            MidnightIndexerConfig::new("devnet", "ws://127.0.0.1:8088/api/v1/graphql/ws", ADDRESS)
                .expect("indexer fixture is valid");
        drop(compose_headless_live(indexer.clone()));
        let checkpoint = MidnightAccountCheckpointConfig::new(
            std::env::temp_dir().join("oxid-composition-account-checkpoints.json"),
        )
        .expect("checkpoint fixture is valid");
        drop(compose_headless_live_with_checkpoints(
            indexer,
            checkpoint.clone(),
        ));

        let remote = MidnightStandaloneConfig::new(
            "devnet",
            "ws://127.0.0.1:8088/api/v1/graphql/ws",
            "http://127.0.0.1:8088/api/v1/graphql",
            "ws://127.0.0.1:9944",
            "http://127.0.0.1:6300",
            ADDRESS,
        )
        .expect("remote standalone fixture is valid");
        drop(compose_headless_standalone(remote.clone()));
        drop(compose_headless_standalone_with_checkpoints(
            remote.clone(),
            checkpoint.clone(),
        ));
        let dust_checkpoint = MidnightDustCheckpointConfig::new(
            std::env::temp_dir().join("oxid-composition-dust-checkpoints.bin"),
        )
        .expect("DUST checkpoint fixture is valid");
        drop(compose_headless_standalone_with_dust_checkpoints(
            remote.clone(),
            dust_checkpoint.clone(),
        ));
        drop(compose_headless_standalone_with_all_checkpoints(
            remote,
            checkpoint,
            dust_checkpoint,
        ));

        let local_proving = MidnightLocalProvingConfig::new(
            std::env::temp_dir().join("oxid-composition-local-proving"),
        )
        .expect("local proving fixture is valid");
        let private = MidnightStandaloneConfig::new_private(
            "devnet",
            "ws://127.0.0.1:8088/api/v1/graphql/ws",
            "http://127.0.0.1:8088/api/v1/graphql",
            "ws://127.0.0.1:9944",
            local_proving,
            ADDRESS,
        )
        .expect("private standalone fixture is valid");
        drop(compose_headless_standalone(private));

        drop(compose());
        drop(compose_headless());
    }

    #[test]
    fn in_memory_composition_exposes_only_development_protection() {
        let services = compose_in_memory();
        let command = WalletProfileSecurityCommand {
            profile_id: "profile_test".to_owned(),
        };
        let initial = services
            .get_wallet_security_status()
            .execute(command.clone())
            .expect("development status should be available");

        assert_eq!(initial.state_name(), "Uninitialized");
        assert_eq!(initial.protection_name(), "Development only");
        assert_eq!(
            services
                .initialize_wallet_security()
                .execute(command)
                .expect("development setup should succeed")
                .state_name(),
            "Unlocked"
        );
    }

    #[test]
    fn production_facing_composition_fails_closed_without_native_custody() {
        let services = compose_with_adapters(
            Arc::new(InMemoryWalletProfileRepository::new()),
            Arc::new(UnavailableWalletSecurity),
            Arc::new(unavailable_midnight_wallet()),
        );
        let status = services
            .get_wallet_security_status()
            .execute(WalletProfileSecurityCommand {
                profile_id: "profile_test".to_owned(),
            })
            .expect("unavailable status should be safely reportable");

        assert_eq!(status.state_name(), "Unavailable");
        assert_eq!(status.protection_name(), "Not connected");
        assert_eq!(
            services
                .get_wallet_account()
                .execute(WalletAccountQuery {
                    profile_id: "profile_test".to_owned(),
                })
                .expect("unavailable account state is safe")
                .source,
            "unavailable"
        );
        assert_eq!(
            services
                .get_wallet_dust_sync_status()
                .execute(WalletDustSyncCommand {
                    profile_id: "profile_test".to_owned(),
                })
                .expect("unavailable DUST status is safe")
                .state,
            "unavailable"
        );
        assert!(
            services
                .start_wallet_dust_sync()
                .execute(WalletDustSyncCommand {
                    profile_id: "profile_test".to_owned(),
                })
                .is_err()
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn standalone_live_configuration_is_all_or_nothing() {
        const ADDRESS: &str =
            "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
        assert!(matches!(
            parse_optional_midnight_config([None, None, None, None, None, None, None]),
            Ok(None)
        ));
        assert!(matches!(
            parse_optional_midnight_config([
                Some("devnet".to_owned()),
                Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
                None,
                None,
                None,
                Some(ADDRESS.to_owned()),
                None,
            ]),
            Ok(Some(HeadlessMidnightConfig::Indexer(_)))
        ));
        assert!(matches!(
            parse_optional_midnight_config([
                Some("devnet".to_owned()),
                Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
                Some("http://127.0.0.1:8088/api/v1/graphql".to_owned()),
                Some("ws://127.0.0.1:9944".to_owned()),
                Some("http://127.0.0.1:6300".to_owned()),
                Some(ADDRESS.to_owned()),
                None,
            ]),
            Ok(Some(HeadlessMidnightConfig::Standalone(_)))
        ));
        let local_cache = std::env::temp_dir().join("oxid-composition-proving-cache");
        assert!(matches!(
            parse_optional_midnight_config([
                Some("devnet".to_owned()),
                Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
                Some("http://127.0.0.1:8088/api/v1/graphql".to_owned()),
                Some("ws://127.0.0.1:9944".to_owned()),
                None,
                Some(ADDRESS.to_owned()),
                Some(local_cache.to_string_lossy().into_owned()),
            ]),
            Ok(Some(HeadlessMidnightConfig::Standalone(_)))
        ));
        assert_eq!(
            parse_optional_midnight_config([
                Some("undeployed".to_owned()),
                None,
                None,
                None,
                None,
                None,
                None,
            ])
            .err(),
            Some(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        );
        assert_eq!(
            parse_optional_midnight_config([
                Some("devnet".to_owned()),
                Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
                Some("http://127.0.0.1:8088/api/v1/graphql".to_owned()),
                Some("ws://127.0.0.1:9944".to_owned()),
                Some("http://127.0.0.1:6300".to_owned()),
                Some(ADDRESS.to_owned()),
                Some(local_cache.to_string_lossy().into_owned()),
            ])
            .err(),
            Some(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
        );
    }
}
