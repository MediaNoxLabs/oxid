// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use oxid_adapter_midnight::{simulated_midnight_wallet, unavailable_midnight_wallet};
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_dev::{DevelopmentWalletSecurity, UnavailableWalletSecurity};
use oxid_adapter_storage_json::JsonWalletProfileRepository;
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_wallet_application::{
    CreateWalletProfileService, CreateWalletProfileUseCase, DeleteWalletKeyUseCase,
    GenerateWalletKeyUseCase, GetActiveWalletProfileService, GetActiveWalletProfileUseCase,
    GetWalletAccountUseCase, GetWalletSecurityStatusUseCase, InitializeWalletSecurityUseCase,
    ListWalletKeysUseCase, ListWalletNetworksUseCase, ListWalletProfilesService,
    ListWalletProfilesUseCase, LockWalletUseCase, SelectWalletNetworkUseCase,
    SelectWalletProfileService, SelectWalletProfileUseCase, SignWalletDataUseCase,
    SyncWalletAccountUseCase, UnlockWalletUseCase, WalletAccountReadPort, WalletAccountService,
    WalletKeyOperationPort, WalletKeyService, WalletNetworkPort, WalletNetworkService,
    WalletProfileRepository, WalletProtectionPort, WalletProtectionService,
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
    get_wallet_account: Arc<dyn GetWalletAccountUseCase>,
    sync_wallet_account: Arc<dyn SyncWalletAccountUseCase>,
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
    pub fn get_wallet_account(&self) -> Arc<dyn GetWalletAccountUseCase> {
        Arc::clone(&self.get_wallet_account)
    }

    #[must_use]
    pub fn sync_wallet_account(&self) -> Arc<dyn SyncWalletAccountUseCase> {
        Arc::clone(&self.sync_wallet_account)
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
    compose_with_adapters(
        Arc::new(JsonWalletProfileRepository::at_default_location()),
        security,
        Arc::new(simulated_midnight_wallet(clock)),
    )
}

/// Wires deterministic process-local services for tests and development tools.
#[must_use]
pub fn compose_in_memory() -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    compose_with_adapters(
        Arc::new(InMemoryWalletProfileRepository::new()),
        security,
        Arc::new(simulated_midnight_wallet(clock)),
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
    M: WalletNetworkPort + WalletAccountReadPort + 'static,
{
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let create_wallet_profile = Arc::new(CreateWalletProfileService::new(
        Arc::clone(&repository),
        clock,
        random,
    ));
    let list_wallet_profiles = Arc::new(ListWalletProfilesService::new(Arc::clone(&repository)));
    let select_wallet_profile = Arc::new(SelectWalletProfileService::new(Arc::clone(&repository)));
    let get_active_wallet_profile = Arc::new(GetActiveWalletProfileService::new(repository));
    let protection = Arc::new(WalletProtectionService::new(Arc::clone(&security)));
    let keys = Arc::new(WalletKeyService::new(security));
    let networks = Arc::new(WalletNetworkService::new(Arc::clone(&midnight)));
    let accounts = Arc::new(WalletAccountService::new(midnight));

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
    let get_wallet_account: Arc<dyn GetWalletAccountUseCase> = accounts.clone();
    let sync_wallet_account: Arc<dyn SyncWalletAccountUseCase> = accounts;

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
        get_wallet_account,
        sync_wallet_account,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_wallet_application::{
        CreateWalletProfileCommand, WalletAccountQuery, WalletProfileSecurityCommand,
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
    }
}
