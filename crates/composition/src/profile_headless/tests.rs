// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::compose;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::MidnightLocalProvingConfig;

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
