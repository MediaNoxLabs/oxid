// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::compose;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::MidnightLocalProvingConfig;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn ordinary_standalone_composition_keeps_os_random_profile_custody() {
    use std::sync::Mutex;

    use oxid_adapter_storage_memory::InMemoryWalletProfileRepository;
    use oxid_platform_ports::{PlatformError, RandomPort};
    use oxid_wallet_application::{
        CreateWalletProfileCommand, WalletDerivedSecretUsePort, WalletHdPath,
        WalletHdPathComponent, WalletProfileSecurityCommand,
    };
    use oxid_wallet_domain::WalletProfileId;

    struct FixedRandom(Mutex<u8>);

    impl RandomPort for FixedRandom {
        fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
            let mut value = self
                .0
                .lock()
                .map_err(|_| PlatformError::RandomnessUnavailable)?;
            destination.fill(*value);
            *value = value.wrapping_add(1);
            Ok(())
        }
    }

    fn first_child<N: RandomPort>(
        security: &DevelopmentWalletSecurity<SystemClock, N>,
        profile_id: &WalletProfileId,
    ) -> [u8; 32] {
        let path = WalletHdPath::new(vec![
            WalletHdPathComponent::new(0, true).expect("child path"),
        ])
        .expect("child path");
        let mut child = [0_u8; 32];
        security
            .use_derived_secret(profile_id, &path, &mut |secret| {
                child.copy_from_slice(secret);
                Ok(())
            })
            .expect("derive test child");
        child
    }

    let placeholder = oxid_adapter_midnight::standalone_configuration_placeholder_address()
        .expect("placeholder address");
    let config = MidnightStandaloneConfig::new(
        "undeployed",
        "ws://127.0.0.1:8088/api/v4/graphql/ws",
        "http://127.0.0.1:8088/api/v4/graphql",
        "ws://127.0.0.1:9944",
        "http://127.0.0.1:6300",
        placeholder.value(),
    )
    .expect("standalone configuration");
    let clock = Arc::new(SystemClock);
    let security = Arc::new(DevelopmentWalletSecurity::new(
        Arc::clone(&clock),
        Arc::new(FixedRandom(Mutex::new(17))),
    ));
    let profiles = Arc::new(InMemoryWalletProfileRepository::new());
    let application = compose_headless_standalone_with_security(
        config,
        clock,
        Arc::clone(&security),
        profiles,
        |security| security,
    );
    let profile = application
        .create_wallet_profile()
        .execute(CreateWalletProfileCommand {
            display_name: crate::standalone_genesis::PUBLIC_STANDALONE_PROFILE_NAME.to_owned(),
        })
        .expect("create named profile");
    application
        .initialize_wallet_security()
        .execute(WalletProfileSecurityCommand {
            profile_id: profile.id.clone(),
        })
        .expect("ordinary initialization");
    let profile_id = WalletProfileId::parse(profile.id).expect("profile id");

    let expected = DevelopmentWalletSecurity::new(
        Arc::new(SystemClock),
        Arc::new(FixedRandom(Mutex::new(17))),
    );
    expected
        .initialize(&profile_id)
        .expect("expected random initialization");

    assert_eq!(
        first_child(security.as_ref(), &profile_id),
        first_child(&expected, &profile_id)
    );
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
    let shielded_checkpoint = MidnightShieldedCheckpointConfig::new(
        std::env::temp_dir().join("oxid-composition-shielded-checkpoints.bin"),
    )
    .expect("shielded checkpoint fixture is valid");
    let submission_journal = MidnightSubmissionJournalConfig::new(
        std::env::temp_dir().join("oxid-composition-submission-journal.json"),
    )
    .expect("submission journal fixture is valid");
    drop(compose_headless_live_with_checkpoint_options(
        remote.indexer().clone(),
        Some(checkpoint.clone()),
        Some(shielded_checkpoint.clone()),
    ));
    drop(compose_headless_standalone_with_dust_checkpoints(
        remote.clone(),
        dust_checkpoint.clone(),
    ));
    drop(compose_headless_standalone_with_all_checkpoints(
        remote.clone(),
        checkpoint.clone(),
        dust_checkpoint.clone(),
    ));
    drop(compose_headless_standalone_with_checkpoint_options(
        remote,
        Some(checkpoint),
        Some(dust_checkpoint),
        Some(shielded_checkpoint),
        Some(submission_journal),
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
