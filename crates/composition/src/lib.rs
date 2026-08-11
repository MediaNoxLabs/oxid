// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_json::JsonWalletProfileRepository;
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_wallet_application::{
    CreateWalletProfileService, CreateWalletProfileUseCase, GetActiveWalletProfileService,
    GetActiveWalletProfileUseCase, ListWalletProfilesService, ListWalletProfilesUseCase,
    SelectWalletProfileService, SelectWalletProfileUseCase, WalletProfileRepository,
};

/// Application capabilities shared by every incoming adapter.
#[derive(Clone)]
pub struct ApplicationServices {
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
    list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
    select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
    get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
}

impl ApplicationServices {
    #[must_use]
    pub const fn new(
        create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
        list_wallet_profiles: Arc<dyn ListWalletProfilesUseCase>,
        select_wallet_profile: Arc<dyn SelectWalletProfileUseCase>,
        get_active_wallet_profile: Arc<dyn GetActiveWalletProfileUseCase>,
    ) -> Self {
        Self {
            create_wallet_profile,
            list_wallet_profiles,
            select_wallet_profile,
            get_active_wallet_profile,
        }
    }

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
}

/// Wires the application with persistent public-profile metadata storage.
#[must_use]
pub fn compose() -> ApplicationServices {
    compose_with_repository(Arc::new(JsonWalletProfileRepository::at_default_location()))
}

/// Wires deterministic process-local services for tests and development tools.
#[must_use]
pub fn compose_in_memory() -> ApplicationServices {
    compose_with_repository(Arc::new(InMemoryWalletProfileRepository::new()))
}

fn compose_with_repository<R>(repository: Arc<R>) -> ApplicationServices
where
    R: WalletProfileRepository + 'static,
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

    ApplicationServices::new(
        create_wallet_profile,
        list_wallet_profiles,
        select_wallet_profile,
        get_active_wallet_profile,
    )
}

#[cfg(test)]
mod tests {
    use oxid_wallet_application::CreateWalletProfileCommand;

    use super::*;

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
}
