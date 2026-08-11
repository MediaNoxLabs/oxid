// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
use oxid_ui_dioxus::WalletUiServices;
use oxid_wallet_application::CreateWalletProfileService;

/// Wires the M0 application with development-safe concrete adapters.
#[must_use]
pub fn compose() -> WalletUiServices {
    let repository = Arc::new(InMemoryWalletProfileRepository::new());
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let create_wallet_profile =
        Arc::new(CreateWalletProfileService::new(repository, clock, random));

    WalletUiServices::new(create_wallet_profile)
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
