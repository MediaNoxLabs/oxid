// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_wallet_application::{CreateWalletProfileService, CreateWalletProfileUseCase};

/// Application capabilities shared by every incoming adapter.
#[derive(Clone)]
pub struct ApplicationServices {
    create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>,
}

impl ApplicationServices {
    #[must_use]
    pub const fn new(create_wallet_profile: Arc<dyn CreateWalletProfileUseCase>) -> Self {
        Self {
            create_wallet_profile,
        }
    }

    #[must_use]
    pub fn create_wallet_profile(&self) -> Arc<dyn CreateWalletProfileUseCase> {
        Arc::clone(&self.create_wallet_profile)
    }
}

/// Wires the M0 application with development-safe concrete adapters.
#[must_use]
pub fn compose() -> ApplicationServices {
    let repository = Arc::new(InMemoryWalletProfileRepository::new());
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let create_wallet_profile =
        Arc::new(CreateWalletProfileService::new(repository, clock, random));

    ApplicationServices::new(create_wallet_profile)
}

#[cfg(test)]
mod tests {
    use oxid_wallet_application::CreateWalletProfileCommand;

    use super::*;

    #[test]
    fn composed_application_executes_the_vertical_slice() {
        let result = compose()
            .create_wallet_profile()
            .execute(CreateWalletProfileCommand {
                display_name: "Composition smoke".to_owned(),
            })
            .expect("composed use case should succeed");

        assert_eq!(result.display_name, "Composition smoke");
        assert!(result.id.starts_with("profile_"));
    }
}
