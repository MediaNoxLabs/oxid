// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_adapter_midnight::MidnightPublicCallContextSource;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::MidnightStandaloneConfig;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_midnight::{
    MidnightContractCallFundingPort, MidnightContractCallFundingRequest,
    MidnightContractCallSubmissionMode, MidnightContractCallSubmissionPort,
    MidnightContractCallSubmissionRequest, MidnightContractCallSubmissionState,
    MidnightContractCallSubmissionStatus,
};
use oxid_adapter_passport_vault::InMemoryPassportVaultRepository;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_passport_vault::{
    FundedPassportVaultCall, JsonPassportVaultRepository, NativePassportVaultContractCall,
    NativePassportVaultContractStateDecoder, NodeAnchoredPassportVaultStateSource,
    PassportVaultCallChainContextSource, PassportVaultCallCompletionPort,
    PassportVaultCallCompletionRequest, PassportVaultCallComposerConfigError,
    PassportVaultCallCompositionContext, PassportVaultCallCompositionContextSource,
    PassportVaultCallFundingPort, PassportVaultCallFundingRequest, PassportVaultStoreConfig,
    SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX, SimulatedPassportVaultContractCall,
    SimulatedPassportVaultStateSource,
};

#[cfg(not(target_arch = "wasm32"))]
use super::environment::PASSPORT_VAULT_STORE_PATH_ENV;
use super::services::ApplicationServices;
use oxid_adapter_platform_system::{OsRandom, SystemClock};
use oxid_adapter_storage_json::JsonWalletProfileRepository;
#[cfg(not(target_arch = "wasm32"))]
use oxid_passport_vault_application::{
    PassportVaultCallDraftId, PassportVaultCallInclusion, PassportVaultCallPortError,
    PassportVaultCallSubmissionState, PassportVaultCallSubmissionStatus,
    PassportVaultContractStateSnapshot,
};
use oxid_passport_vault_application::{
    PassportVaultContractCallService, PassportVaultContractStateService,
    PassportVaultContractStateSourcePort, PassportVaultRepository,
    UnavailablePassportVaultContractCall, UnavailablePassportVaultRepository,
};

/// Returns the fixed development-only Passport Vault address accepted by the
/// deterministic headless call harness.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub const fn simulated_passport_vault_contract_address_hex() -> &'static str {
    SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX
}

#[cfg(test)]
#[path = "passport_vault/tests.rs"]
mod tests;
pub(super) struct PassportVaultRepositoryComposition {
    pub(super) repository: Arc<dyn PassportVaultRepository>,
    pub(super) persistence: &'static str,
}

impl PassportVaultRepositoryComposition {
    pub(super) fn unavailable() -> Self {
        Self {
            repository: Arc::new(UnavailablePassportVaultRepository),
            persistence: "unavailable",
        }
    }

    pub(super) fn process_local() -> Self {
        Self {
            repository: Arc::new(InMemoryPassportVaultRepository::default()),
            persistence: "process_local",
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn node_anchored_passport_vault_state_source(
    config: &MidnightStandaloneConfig,
) -> Option<Arc<dyn PassportVaultContractStateSourcePort>> {
    NodeAnchoredPassportVaultStateSource::new(
        config.indexer_http_url(),
        config.node_websocket_url(),
    )
    .ok()
    .map(|source| Arc::new(source) as Arc<dyn PassportVaultContractStateSourcePort>)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn with_passport_vault_state_source(
    mut services: ApplicationServices,
    source: Option<Arc<dyn PassportVaultContractStateSourcePort>>,
) -> ApplicationServices {
    if let Some(source) = source {
        services.read_passport_vault_contract_state =
            Arc::new(PassportVaultContractStateService::with_source(
                Arc::new(NativePassportVaultContractStateDecoder),
                Arc::clone(&source),
            ));
        let calls = Arc::new(PassportVaultContractCallService::new(
            source,
            Arc::new(UnavailablePassportVaultContractCall),
            Arc::new(SystemClock),
            Arc::new(OsRandom),
        ));
        services.prepare_passport_vault_call = calls.clone();
        services.authorize_passport_vault_call = calls.clone();
        services.submit_passport_vault_call = calls.clone();
        services.get_passport_vault_call = calls.clone();
        services.get_passport_vault_call_submission_status = calls.clone();
        services.cancel_passport_vault_call_submission = calls.clone();
        services.list_passport_vault_call_submissions = calls.clone();
        services.reconcile_passport_vault_call_submission = calls;
        services.passport_vault_call_mode = "native_pending";
    }
    services
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct ComposedPassportVaultCallContextSource {
    pub(super) wallet: Arc<dyn MidnightPublicCallContextSource>,
    pub(super) chain: Arc<dyn PassportVaultCallChainContextSource>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PassportVaultCallCompositionContextSource for ComposedPassportVaultCallContextSource {
    fn context(
        &self,
        profile_id: &str,
        contract_state: &PassportVaultContractStateSnapshot,
    ) -> Result<PassportVaultCallCompositionContext, PassportVaultCallPortError> {
        let wallet = self
            .wallet
            .public_call_context(profile_id)
            .map_err(map_wallet_context_error)?;
        let chain = self.chain.chain_context(contract_state)?;
        PassportVaultCallCompositionContext::new(
            wallet.network_id().as_str(),
            chain.zswap_chain_state().to_vec(),
            chain.ledger_parameters().to_vec(),
            wallet.coin_public_key(),
            wallet.encryption_public_key(),
            wallet.unshielded_recipient(),
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
const fn map_wallet_context_error(
    error: oxid_wallet_application::WalletAccountPortError,
) -> PassportVaultCallPortError {
    match error {
        oxid_wallet_application::WalletAccountPortError::ProtectionNotInitialized => {
            PassportVaultCallPortError::ProtectionNotInitialized
        }
        oxid_wallet_application::WalletAccountPortError::ProtectionLocked => {
            PassportVaultCallPortError::ProtectionLocked
        }
        oxid_wallet_application::WalletAccountPortError::NotFound => {
            PassportVaultCallPortError::AccountNotDerived
        }
        oxid_wallet_application::WalletAccountPortError::UnsupportedNetwork => {
            PassportVaultCallPortError::UnsupportedNetwork
        }
        oxid_wallet_application::WalletAccountPortError::Unavailable => {
            PassportVaultCallPortError::Unavailable
        }
        oxid_wallet_application::WalletAccountPortError::InvalidData => {
            PassportVaultCallPortError::InvalidData
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct ComposedPassportVaultCallFunding {
    midnight: Arc<dyn MidnightContractCallFundingPort>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PassportVaultCallFundingPort for ComposedPassportVaultCallFunding {
    fn fund(
        &self,
        request: PassportVaultCallFundingRequest,
    ) -> Result<FundedPassportVaultCall, PassportVaultCallPortError> {
        let (profile_id, network_id, expires_at_seconds, requires_night_funding, transaction) =
            request.into_parts();
        let funded = self
            .midnight
            .fund_contract_call(MidnightContractCallFundingRequest::new(
                profile_id,
                network_id,
                expires_at_seconds,
                requires_night_funding,
                transaction,
            ))
            .map_err(map_wallet_transaction_error)?;
        let funded_night_atomic_units = funded.funded_night_atomic_units();
        let funding_input_count = funded.funding_input_count();
        Ok(FundedPassportVaultCall::new(
            funded.into_transaction(),
            funded_night_atomic_units,
            funding_input_count,
        ))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct ComposedPassportVaultCallCompletion {
    midnight: Arc<dyn MidnightContractCallSubmissionPort>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PassportVaultCallCompletionPort for ComposedPassportVaultCallCompletion {
    fn complete(
        &self,
        request: PassportVaultCallCompletionRequest,
    ) -> Result<PassportVaultCallInclusion, PassportVaultCallPortError> {
        let (
            profile_id,
            network_id,
            draft_id,
            planning_fingerprint,
            expires_at,
            updated_at,
            transaction,
        ) = request.into_parts();
        let outcome = self
            .midnight
            .complete_contract_call(MidnightContractCallSubmissionRequest::new(
                profile_id,
                network_id,
                draft_id,
                planning_fingerprint,
                expires_at,
                updated_at,
                transaction,
            ))
            .map_err(map_wallet_transaction_error)?;
        Ok(PassportVaultCallInclusion {
            transaction_hash_hex: encode_lower_hex(outcome.transaction_hash),
            block_hash_hex: encode_lower_hex(outcome.block_hash),
            block_height: outcome.block_height,
            fee_atomic_units: outcome.fee_specks,
            mode: midnight_submission_mode(outcome.mode).to_owned(),
        })
    }

    fn status(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        self.midnight
            .contract_call_submission_status(profile_id, draft_id)
            .map_err(map_wallet_transaction_error)
            .and_then(map_midnight_contract_call_status)
    }

    fn cancel(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        self.midnight
            .cancel_contract_call_submission(profile_id, draft_id)
            .map_err(map_wallet_transaction_error)
            .and_then(map_midnight_contract_call_status)
    }

    fn history(
        &self,
        profile_id: &str,
    ) -> Result<Vec<PassportVaultCallSubmissionStatus>, PassportVaultCallPortError> {
        self.midnight
            .contract_call_submission_history(profile_id)
            .map_err(map_wallet_transaction_error)?
            .into_iter()
            .map(map_midnight_contract_call_status)
            .collect()
    }

    fn reconcile(
        &self,
        profile_id: &str,
        draft_id: &str,
    ) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
        self.midnight
            .reconcile_contract_call_submission(profile_id, draft_id)
            .map_err(map_wallet_transaction_error)
            .and_then(map_midnight_contract_call_status)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn map_midnight_contract_call_status(
    status: MidnightContractCallSubmissionStatus,
) -> Result<PassportVaultCallSubmissionStatus, PassportVaultCallPortError> {
    let draft_id = PassportVaultCallDraftId::parse(status.draft_id)
        .map_err(|_| PassportVaultCallPortError::InvalidData)?;
    let state = match status.state {
        MidnightContractCallSubmissionState::Running => PassportVaultCallSubmissionState::Running,
        MidnightContractCallSubmissionState::CancellationRequested => {
            PassportVaultCallSubmissionState::CancellationRequested
        }
        MidnightContractCallSubmissionState::Broadcasting => {
            PassportVaultCallSubmissionState::Broadcasting
        }
        MidnightContractCallSubmissionState::Included => PassportVaultCallSubmissionState::Included,
        MidnightContractCallSubmissionState::Rejected => PassportVaultCallSubmissionState::Rejected,
        MidnightContractCallSubmissionState::Expired => PassportVaultCallSubmissionState::Expired,
        MidnightContractCallSubmissionState::OutcomeUnknown => {
            PassportVaultCallSubmissionState::OutcomeUnknown
        }
    };
    Ok(PassportVaultCallSubmissionStatus {
        draft_id,
        state,
        transaction_hash_hex: status.transaction_hash.map(encode_lower_hex),
        block_hash_hex: status.block_hash.map(encode_lower_hex),
        block_height: status.block_height,
        fee_atomic_units: status.fee_specks,
        mode: status.mode.map(midnight_submission_mode).map(str::to_owned),
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn encode_lower_hex(bytes: impl AsRef<[u8]>) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";

    let bytes = bytes.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(not(target_arch = "wasm32"))]
const fn midnight_submission_mode(mode: MidnightContractCallSubmissionMode) -> &'static str {
    match mode {
        MidnightContractCallSubmissionMode::Simulated => "simulated",
        MidnightContractCallSubmissionMode::Live => "live",
    }
}

#[cfg(not(target_arch = "wasm32"))]
const fn map_wallet_transaction_error(
    error: oxid_wallet_application::WalletTransactionPortError,
) -> PassportVaultCallPortError {
    use oxid_wallet_application::WalletTransactionPortError as WalletError;

    match error {
        WalletError::Unavailable => PassportVaultCallPortError::Unavailable,
        WalletError::ProtectionNotInitialized => {
            PassportVaultCallPortError::ProtectionNotInitialized
        }
        WalletError::ProtectionLocked => PassportVaultCallPortError::ProtectionLocked,
        WalletError::AccountNotDerived => PassportVaultCallPortError::AccountNotDerived,
        WalletError::AccountNotSynchronized => PassportVaultCallPortError::AccountNotSynchronized,
        WalletError::ShieldedStateNotCurrent => PassportVaultCallPortError::InvalidChainState,
        WalletError::UnsupportedNetwork => PassportVaultCallPortError::UnsupportedNetwork,
        WalletError::InvalidRecipient | WalletError::RecipientNetworkMismatch => {
            PassportVaultCallPortError::InvalidData
        }
        WalletError::InsufficientFunds => PassportVaultCallPortError::InsufficientFunds,
        WalletError::DraftNotFound => PassportVaultCallPortError::DraftNotFound,
        WalletError::DraftExpired => PassportVaultCallPortError::DraftExpired,
        WalletError::DraftConflict => PassportVaultCallPortError::DraftConflict,
        WalletError::SubmissionInProgress => PassportVaultCallPortError::SubmissionInProgress,
        WalletError::SubmissionNotInProgress => PassportVaultCallPortError::SubmissionNotInProgress,
        WalletError::SubmissionCancelled => PassportVaultCallPortError::SubmissionCancelled,
        WalletError::SubmissionCancellationUnsafe => {
            PassportVaultCallPortError::SubmissionCancellationUnsafe
        }
        WalletError::AuthorizationChallengeMismatch => {
            PassportVaultCallPortError::AuthorizationChallengeMismatch
        }
        WalletError::InsufficientDust => PassportVaultCallPortError::InsufficientDust,
        WalletError::InvalidChainState => PassportVaultCallPortError::InvalidChainState,
        WalletError::ProvingFailed => PassportVaultCallPortError::ProvingFailed,
        WalletError::SubmissionRejected => PassportVaultCallPortError::SubmissionRejected,
        WalletError::SubmissionOutcomeUnknown => {
            PassportVaultCallPortError::SubmissionOutcomeUnknown
        }
        WalletError::Timeout => PassportVaultCallPortError::Timeout,
        WalletError::InvalidData => PassportVaultCallPortError::InvalidData,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn with_native_passport_vault_calls(
    mut services: ApplicationServices,
    state_source: Arc<dyn PassportVaultContractStateSourcePort>,
    chain_source: Arc<dyn PassportVaultCallChainContextSource>,
    composer: impl AsRef<std::path::Path>,
) -> Result<ApplicationServices, PassportVaultCallComposerConfigError> {
    let contexts: Arc<dyn PassportVaultCallCompositionContextSource> =
        Arc::new(ComposedPassportVaultCallContextSource {
            wallet: Arc::clone(&services.midnight_public_call_context),
            chain: chain_source,
        });
    let funding: Arc<dyn PassportVaultCallFundingPort> =
        Arc::new(ComposedPassportVaultCallFunding {
            midnight: Arc::clone(&services.midnight_contract_call_funding),
        });
    let completion: Arc<dyn PassportVaultCallCompletionPort> =
        Arc::new(ComposedPassportVaultCallCompletion {
            midnight: Arc::clone(&services.midnight_contract_call_submission),
        });
    let native_calls =
        if let Some(presentations) = services.protected_passport_vault_presentations.clone() {
            NativePassportVaultContractCall::new_with_protected_claims_and_completion(
                composer,
                contexts,
                funding,
                completion,
                presentations,
            )?
        } else {
            NativePassportVaultContractCall::new_with_funding_and_completion(
                composer, contexts, funding, completion,
            )?
        };
    let calls = Arc::new(PassportVaultContractCallService::new(
        state_source,
        Arc::new(native_calls),
        Arc::new(SystemClock),
        Arc::new(OsRandom),
    ));
    services.prepare_passport_vault_call = calls.clone();
    services.authorize_passport_vault_call = calls.clone();
    services.submit_passport_vault_call = calls.clone();
    services.get_passport_vault_call = calls.clone();
    services.get_passport_vault_call_submission_status = calls.clone();
    services.cancel_passport_vault_call_submission = calls.clone();
    services.list_passport_vault_call_submissions = calls.clone();
    services.reconcile_passport_vault_call_submission = calls;
    services.passport_vault_call_mode = "native_settlement";
    Ok(services)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn with_simulated_passport_vault_calls(
    mut services: ApplicationServices,
) -> ApplicationServices {
    let Ok(source) = SimulatedPassportVaultStateSource::new() else {
        return services;
    };
    let source: Arc<dyn PassportVaultContractStateSourcePort> = Arc::new(source);
    services.read_passport_vault_contract_state =
        Arc::new(PassportVaultContractStateService::with_source(
            Arc::new(NativePassportVaultContractStateDecoder),
            Arc::clone(&source),
        ));
    let calls = Arc::new(PassportVaultContractCallService::new_simulated(
        source,
        Arc::new(SimulatedPassportVaultContractCall::new()),
        Arc::new(SystemClock),
        Arc::new(OsRandom),
    ));
    services.prepare_passport_vault_call = calls.clone();
    services.authorize_passport_vault_call = calls.clone();
    services.submit_passport_vault_call = calls.clone();
    services.get_passport_vault_call = calls.clone();
    services.get_passport_vault_call_submission_status = calls.clone();
    services.cancel_passport_vault_call_submission = calls.clone();
    services.list_passport_vault_call_submissions = calls.clone();
    services.reconcile_passport_vault_call_submission = calls;
    services.passport_vault_call_mode = "deterministic_simulation";
    services.passport_vault_call_contract_address_hex =
        Some(SIMULATED_PASSPORT_VAULT_CONTRACT_ADDRESS_HEX);
    services
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn headless_passport_vault_repository() -> PassportVaultRepositoryComposition {
    let path = std::env::var_os(PASSPORT_VAULT_STORE_PATH_ENV)
        .map(std::path::PathBuf::from)
        .or_else(|| {
            JsonWalletProfileRepository::at_default_location()
                .configured_path()
                .and_then(std::path::Path::parent)
                .map(|directory| directory.join("private/passport-vault.json"))
        });
    path.and_then(|path| PassportVaultStoreConfig::new(path).ok())
        .map_or_else(PassportVaultRepositoryComposition::unavailable, |config| {
            PassportVaultRepositoryComposition {
                repository: Arc::new(JsonPassportVaultRepository::new(config)),
                persistence: "owner_private_atomic_file",
            }
        })
}

#[cfg(target_arch = "wasm32")]
pub(super) fn headless_passport_vault_repository() -> PassportVaultRepositoryComposition {
    PassportVaultRepositoryComposition::process_local()
}
