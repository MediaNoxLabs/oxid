// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_adapter_midnight::protected_simulated_midnight_wallet;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{
    MidnightAccountCheckpointConfig, MidnightDustCheckpointConfig, MidnightIndexerConfig,
    MidnightShieldedCheckpointConfig, MidnightStandaloneConfig, MidnightSubmissionJournalConfig,
    protected_live_midnight_wallet, protected_live_midnight_wallet_with_checkpoint_options,
    protected_live_midnight_wallet_with_checkpoints,
    protected_simulated_midnight_wallet_with_submission_journal,
    protected_standalone_midnight_wallet,
    protected_standalone_midnight_wallet_with_all_checkpoints,
    protected_standalone_midnight_wallet_with_checkpoint_options,
    protected_standalone_midnight_wallet_with_checkpoints,
    protected_standalone_midnight_wallet_with_dust_checkpoints,
};

use super::identity::{CredentialPresentationComposition, HeadlessCredentialProfile};
#[cfg(not(target_arch = "wasm32"))]
use super::passport_vault::{
    node_anchored_passport_vault_state_source, with_passport_vault_state_source,
    with_simulated_passport_vault_calls,
};
use super::services::ApplicationServices;
#[cfg(not(target_arch = "wasm32"))]
use super::standalone_genesis::StandaloneDevelopmentRandom;
use super::wiring::{
    compose_with_adapters, compose_with_adapters_and_credential_profile,
    compose_with_adapters_and_presentation,
};
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
use oxid_adapter_storage_json::JsonWalletProfileRepository;

/// Wires persistent public profiles with an explicit process-local custody
/// adapter for the standalone development harness.
#[must_use]
pub fn compose_headless() -> ApplicationServices {
    compose_headless_with_presentation(CredentialPresentationComposition::Standalone)
}

#[cfg(test)]
#[path = "profile_headless/tests.rs"]
mod tests;

pub(super) fn compose_headless_with_presentation(
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    compose_headless_with_credential_profile(
        credential_presentation,
        HeadlessCredentialProfile::Standalone,
    )
}

pub(super) fn compose_headless_with_credential_profile(
    credential_presentation: CredentialPresentationComposition,
    credential_profile: HeadlessCredentialProfile,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    #[cfg(not(target_arch = "wasm32"))]
    let midnight = profiles
        .configured_path()
        .and_then(|path| path.parent())
        .map(|directory| directory.join("private/midnight-submissions.json"))
        .and_then(|path| MidnightSubmissionJournalConfig::new(path).ok())
        .map_or_else(
            || protected_simulated_midnight_wallet(Arc::clone(&clock), Arc::clone(&security)),
            |journal| {
                protected_simulated_midnight_wallet_with_submission_journal(
                    journal,
                    Arc::clone(&clock),
                    Arc::clone(&security),
                )
            },
        )
        .with_profile_association_repository(profiles.clone());
    #[cfg(target_arch = "wasm32")]
    let midnight = Arc::new(
        protected_simulated_midnight_wallet(Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    #[cfg(not(target_arch = "wasm32"))]
    let midnight = Arc::new(midnight);
    let services = compose_with_adapters_and_credential_profile(
        profiles,
        security,
        midnight,
        credential_presentation,
        credential_profile,
    );
    #[cfg(not(target_arch = "wasm32"))]
    {
        with_simulated_passport_vault_calls(services)
    }
    #[cfg(target_arch = "wasm32")]
    {
        services
    }
}

/// Wires optional public-account and private shielded checkpoints to a live indexer.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_live_with_checkpoint_options(
    config: MidnightIndexerConfig,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
) -> ApplicationServices {
    compose_headless_live_with_checkpoint_options_and_presentation(
        config,
        account_checkpoints,
        shielded_checkpoints,
        CredentialPresentationComposition::Standalone,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn compose_headless_live_with_checkpoint_options_and_presentation(
    config: MidnightIndexerConfig,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_live_midnight_wallet_with_checkpoint_options(
            config,
            account_checkpoints,
            shielded_checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    compose_with_adapters_and_presentation(profiles, security, midnight, credential_presentation)
}

/// Wires any reviewed combination of standalone checkpoint stores.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_checkpoint_options(
    config: MidnightStandaloneConfig,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    dust_checkpoints: Option<MidnightDustCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    submission_journal: Option<MidnightSubmissionJournalConfig>,
) -> ApplicationServices {
    compose_headless_standalone_with_checkpoint_options_and_presentation(
        config,
        account_checkpoints,
        dust_checkpoints,
        shielded_checkpoints,
        submission_journal,
        CredentialPresentationComposition::Standalone,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn compose_headless_standalone_with_checkpoint_options_and_presentation(
    config: MidnightStandaloneConfig,
    account_checkpoints: Option<MidnightAccountCheckpointConfig>,
    dust_checkpoints: Option<MidnightDustCheckpointConfig>,
    shielded_checkpoints: Option<MidnightShieldedCheckpointConfig>,
    submission_journal: Option<MidnightSubmissionJournalConfig>,
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_checkpoint_options(
            config,
            account_checkpoints,
            dust_checkpoints,
            shielded_checkpoints,
            submission_journal,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters_and_presentation(
            profiles,
            security,
            midnight,
            credential_presentation,
        ),
        passport_vault_state_source,
    )
}

/// Wires deterministic simulation to an explicit durable public submission journal.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_with_submission_journal(
    journal: MidnightSubmissionJournalConfig,
) -> ApplicationServices {
    compose_headless_with_submission_journal_and_presentation(
        journal,
        CredentialPresentationComposition::Standalone,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn compose_headless_with_submission_journal_and_presentation(
    journal: MidnightSubmissionJournalConfig,
    credential_presentation: CredentialPresentationComposition,
) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_simulated_midnight_wallet_with_submission_journal(
            journal,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_simulated_passport_vault_calls(compose_with_adapters_and_presentation(
        profiles,
        security,
        midnight,
        credential_presentation,
    ))
}

/// Wires persistent public profiles and development custody to an explicitly
/// configured live standalone indexer. Normal mobile composition never calls it.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_live(config: MidnightIndexerConfig) -> ApplicationServices {
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_live_midnight_wallet(config, Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    compose_with_adapters(profiles, security, midnight)
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
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_live_midnight_wallet_with_checkpoints(
            config,
            checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    compose_with_adapters(profiles, security, midnight)
}

/// Wires development custody to the complete, explicitly configured standalone stack.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone(config: MidnightStandaloneConfig) -> ApplicationServices {
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet(config, Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters(profiles, security, midnight),
        passport_vault_state_source,
    )
}

/// Wires the explicit compile-time mobile development profile to the public
/// undeployed genesis wallet. Ordinary runtime-selected headless standalone
/// composition continues to initialize an OS-random wallet root.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn compose_public_genesis_standalone(
    config: MidnightStandaloneConfig,
) -> ApplicationServices {
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(StandaloneDevelopmentRandom::for_network(
        config.indexer().network_id().as_str(),
    ));
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet(config, Arc::clone(&clock), Arc::clone(&security))
            .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters(profiles, security, midnight),
        passport_vault_state_source,
    )
}

/// Wires the complete standalone stack with durable public account checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_checkpoints(
    config: MidnightStandaloneConfig,
    checkpoints: MidnightAccountCheckpointConfig,
) -> ApplicationServices {
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_checkpoints(
            config,
            checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters(profiles, security, midnight),
        passport_vault_state_source,
    )
}

/// Wires the complete standalone stack with private key-scoped DUST checkpoints.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compose_headless_standalone_with_dust_checkpoints(
    config: MidnightStandaloneConfig,
    dust_checkpoints: MidnightDustCheckpointConfig,
) -> ApplicationServices {
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_dust_checkpoints(
            config,
            dust_checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters(profiles, security, midnight),
        passport_vault_state_source,
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
    let passport_vault_state_source = node_anchored_passport_vault_state_source(&config);
    let clock = Arc::new(SystemClock);
    let random = Arc::new(OsRandom);
    let security = Arc::new(DevelopmentWalletSecurity::new(Arc::clone(&clock), random));
    let profiles = Arc::new(JsonWalletProfileRepository::at_default_location());
    let midnight = Arc::new(
        protected_standalone_midnight_wallet_with_all_checkpoints(
            config,
            account_checkpoints,
            dust_checkpoints,
            Arc::clone(&clock),
            Arc::clone(&security),
        )
        .with_profile_association_repository(profiles.clone()),
    );
    with_passport_vault_state_source(
        compose_with_adapters(profiles, security, midnight),
        passport_vault_state_source,
    )
}
