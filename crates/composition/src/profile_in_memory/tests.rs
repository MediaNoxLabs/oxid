// SPDX-License-Identifier: Apache-2.0

use super::*;
use oxid_wallet_application::WalletProfileSecurityCommand;

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
