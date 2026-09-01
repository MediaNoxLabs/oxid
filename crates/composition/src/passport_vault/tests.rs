// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{compose_in_memory, standalone_oid4vci_offer};
#[cfg(not(target_arch = "wasm32"))]
use futures::executor::block_on;
#[cfg(not(target_arch = "wasm32"))]
use midnight_base_crypto::fab::AlignedValue;
#[cfg(not(target_arch = "wasm32"))]
use midnight_ledger::structure::INITIAL_PARAMETERS;
#[cfg(not(target_arch = "wasm32"))]
use midnight_onchain_runtime::state::{ChargedState, ContractState, StateValue};
#[cfg(not(target_arch = "wasm32"))]
use midnight_serialize::{tagged_deserialize, tagged_serialize};
#[cfg(not(target_arch = "wasm32"))]
use midnight_storage::{DefaultDB, arena::Sp, storage::Array};
#[cfg(not(target_arch = "wasm32"))]
use midnight_zswap::ledger::State as ZswapChainState;
#[cfg(not(target_arch = "wasm32"))]
use oxid_adapter_vc_midnight::standalone_digital_passport_issuer_trust_anchor;
use oxid_identity_application::CreateDidCommand;
#[cfg(not(target_arch = "wasm32"))]
use oxid_passport_vault_application::{
    AUTHORIZE_PASSPORT_VAULT_CALL_INTENT, AuthorizePassportVaultCallCommand,
    PassportVaultContractStateReadFuture, PassportVaultContractStateSourceError,
    PreparePassportVaultCallAction, PreparePassportVaultCallCommand,
    SUBMIT_PASSPORT_VAULT_CALL_INTENT, SubmitPassportVaultCallCommand,
};
#[cfg(not(target_arch = "wasm32"))]
use oxid_protocol_application::{
    AcceptCredentialIssuanceCommand, PrepareCredentialIssuanceCommand,
};
use oxid_wallet_application::{
    CreateWalletProfileCommand, DeriveWalletAccountCommand, WalletAccountQuery,
    WalletProfileSecurityCommand,
};

#[cfg(not(target_arch = "wasm32"))]
struct FixedVaultChainContext;

#[cfg(not(target_arch = "wasm32"))]
impl PassportVaultCallChainContextSource for FixedVaultChainContext {
    fn chain_context(
        &self,
        snapshot: &PassportVaultContractStateSnapshot,
    ) -> Result<
        oxid_adapter_passport_vault::PassportVaultCallChainContext,
        PassportVaultCallPortError,
    > {
        oxid_adapter_passport_vault::PassportVaultCallChainContext::from_snapshot(
            snapshot,
            vec![1],
            vec![2],
        )
        .map_err(|_| PassportVaultCallPortError::InvalidChainState)
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct ManagedClaimVaultChainContext;

#[cfg(not(target_arch = "wasm32"))]
impl PassportVaultCallChainContextSource for ManagedClaimVaultChainContext {
    fn chain_context(
        &self,
        snapshot: &PassportVaultContractStateSnapshot,
    ) -> Result<
        oxid_adapter_passport_vault::PassportVaultCallChainContext,
        PassportVaultCallPortError,
    > {
        let mut zswap_chain_state = Vec::new();
        tagged_serialize(&ZswapChainState::<DefaultDB>::new(), &mut zswap_chain_state)
            .map_err(|_| PassportVaultCallPortError::InvalidData)?;
        let mut ledger_parameters = Vec::new();
        tagged_serialize(&INITIAL_PARAMETERS, &mut ledger_parameters)
            .map_err(|_| PassportVaultCallPortError::InvalidData)?;
        oxid_adapter_passport_vault::PassportVaultCallChainContext::from_snapshot(
            snapshot,
            zswap_chain_state,
            ledger_parameters,
        )
        .map_err(|_| PassportVaultCallPortError::InvalidChainState)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct ManagedClaimVaultStateSource {
    snapshot: PassportVaultContractStateSnapshot,
}

#[cfg(not(target_arch = "wasm32"))]
impl PassportVaultContractStateSourcePort for ManagedClaimVaultStateSource {
    fn read<'a>(
        &'a self,
        contract_address_hex: &'a str,
    ) -> PassportVaultContractStateReadFuture<'a> {
        Box::pin(async move {
            if contract_address_hex != self.snapshot.contract_address_hex {
                return Err(PassportVaultContractStateSourceError::NotFound);
            }
            Ok(self.snapshot.clone())
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn managed_claim_contract_state() -> Vec<u8> {
    const FIXTURE: &str = include_str!("../../../../fixtures/passport-vault/contract-state-v1.hex");
    let mut cursor = std::io::Cursor::new(hex::decode(FIXTURE.trim()).expect("fixture bytes"));
    let mut contract: ContractState<DefaultDB> =
        tagged_deserialize(&mut cursor).expect("fixture state");
    let StateValue::Array(fields) = contract.data.get_ref() else {
        panic!("fixture ledger fields");
    };
    let mut fields: Vec<StateValue<DefaultDB>> = fields.iter_deref().cloned().collect();

    let trust = standalone_digital_passport_issuer_trust_anchor();
    let issuer_contract: [u8; 32] = hex::decode(
        trust
            .issuer_did()
            .strip_prefix("did:midnight:undeployed:")
            .expect("standalone issuer DID"),
    )
    .expect("issuer contract hex")
    .try_into()
    .expect("issuer contract bytes");
    fields[2] = StateValue::Cell(Sp::new(AlignedValue::from((
        issuer_contract,
        trust.method_id(),
    ))));
    fields[3] = StateValue::Cell(Sp::new(AlignedValue::from(trust.public_key_hash())));

    let locks = match &fields[4] {
        StateValue::Map(locks) => locks.clone(),
        _ => panic!("fixture locks"),
    };
    let record = (
        [9_u8; 32], 18_u8, false, [0_u8; 32], false, [0_u8; 32], 40_u128, [5_u8; 32], 100_u128,
        0_u128,
    );
    fields[4] = StateValue::Map(locks.insert(
        AlignedValue::from(0_u64),
        StateValue::Cell(Sp::new(AlignedValue::from(record))),
    ));
    fields[7] = StateValue::Cell(Sp::new(AlignedValue::from(100_u128)));
    contract.data = ChargedState::new(StateValue::Array(Array::new_from_slice(&fields)));

    let mut state = Vec::new();
    tagged_serialize(&contract, &mut state).expect("claim-ready state");
    state
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_vault_context_is_joined_only_inside_composition() {
    let services = compose_in_memory();
    let source = ComposedPassportVaultCallContextSource {
        wallet: Arc::clone(&services.midnight_public_call_context),
        chain: Arc::new(FixedVaultChainContext),
    };
    let snapshot = PassportVaultContractStateSnapshot {
            serialized_contract_state: vec![3],
            authentication:
                oxid_passport_vault_application::PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
            contract_address_hex: "11".repeat(32),
            transaction_hash_hex: "22".repeat(32),
            action_block_hash_hex: "33".repeat(32),
            action_block_height: 4,
            finalized_head_hash_hex: "44".repeat(32),
            finalized_head_height: 5,
            finalized_head_time_seconds: 1_700_000_000,
        };
    let context = source
        .context("profile_test", &snapshot)
        .expect("public contexts join");
    let debug = format!("{context:?}");
    assert!(debug.contains("undeployed"));
    assert!(debug.contains("zswap_chain_state_bytes: 1"));
    assert!(!debug.contains("094a9125"));

    let state = Arc::new(SimulatedPassportVaultStateSource::new().expect("simulated state source"));
    let state_port: Arc<dyn PassportVaultContractStateSourcePort> = state;
    let composer = std::fs::canonicalize(std::env::current_exe().expect("test executable"))
        .expect("canonical test executable");
    let services = with_native_passport_vault_calls(
        services,
        state_port,
        Arc::new(FixedVaultChainContext),
        composer,
    )
    .expect("native adapter wiring");
    assert_eq!(services.passport_vault_call_mode(), "native_settlement");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn standalone_managed_claim_composes_and_settles_through_the_native_stack() {
    let Some(composer) = std::env::var_os("OXID_PASSPORT_VAULT_COMPOSER") else {
        return;
    };
    let composer = std::fs::canonicalize(composer).expect("packaged composer");
    let services = compose_in_memory();
    let profile = services
        .create_wallet_profile()
        .execute(CreateWalletProfileCommand {
            display_name: "Managed vault claimant".to_owned(),
        })
        .expect("profile");
    services
        .initialize_wallet_security()
        .execute(WalletProfileSecurityCommand {
            profile_id: profile.id.clone(),
        })
        .expect("protected custody");
    services
        .derive_wallet_account()
        .execute(DeriveWalletAccountCommand {
            profile_id: profile.id.clone(),
            account_index: 0,
            address_index: 0,
        })
        .expect("managed Midnight account");
    block_on(services.sync_wallet_account().execute(WalletAccountQuery {
        profile_id: profile.id.clone(),
    }))
    .expect("synchronized Midnight account");

    let did = services
        .create_did()
        .execute(CreateDidCommand {
            profile_id: profile.id.clone(),
            network: "undeployed".to_owned(),
        })
        .expect("managed DID");
    let authentication_method = did
        .document
        .relationships
        .iter()
        .find(|relationship| relationship.relationship == "authentication")
        .and_then(|relationship| relationship.method_ids.first())
        .cloned()
        .expect("authentication method");
    let holder_method = did
        .document
        .verification_methods
        .iter()
        .find(|method| method.public_key_jwk.curve == "Jubjub")
        .map(|method| method.id.clone())
        .expect("managed Jubjub method");

    let issuance = block_on(services.prepare_credential_issuance().execute(
        PrepareCredentialIssuanceCommand {
            profile_id: profile.id.clone(),
            offer: standalone_oid4vci_offer(),
        },
    ))
    .expect("issuance plan");
    let issued = block_on(services.accept_credential_issuance().execute(
        AcceptCredentialIssuanceCommand {
            profile_id: profile.id.clone(),
            issuance_id: issuance.id,
            holder_did: did.document.id,
            method_id: authentication_method,
            holder_binding_method_id: holder_method,
            confirmed: true,
            intent: "ACCEPT_CREDENTIAL_ISSUANCE".to_owned(),
        },
    ))
    .expect("holder-bound credential");
    let credential_id = issued.credential_id.expect("credential identifier");

    let finalized_head_time_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_secs();
    let contract_address_hex = "aa".repeat(32);
    let state_source: Arc<dyn PassportVaultContractStateSourcePort> =
            Arc::new(ManagedClaimVaultStateSource {
                snapshot: PassportVaultContractStateSnapshot {
                    serialized_contract_state: managed_claim_contract_state(),
                    authentication: oxid_passport_vault_application::PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
                    contract_address_hex: contract_address_hex.clone(),
                    transaction_hash_hex: "bb".repeat(32),
                    action_block_hash_hex: "cc".repeat(32),
                    action_block_height: 40,
                    finalized_head_hash_hex: "dd".repeat(32),
                    finalized_head_height: 42,
                    finalized_head_time_seconds,
                },
            });
    let services = with_native_passport_vault_calls(
        services,
        state_source,
        Arc::new(ManagedClaimVaultChainContext),
        composer,
    )
    .expect("native protected claim composition");

    let prepared = block_on(services.prepare_passport_vault_call().execute(
        PreparePassportVaultCallCommand {
            profile_id: profile.id.clone(),
            contract_address_hex,
            action: PreparePassportVaultCallAction::ClaimFromLock {
                lock_id: 0,
                amount: "1".to_owned(),
                credential_id,
            },
        },
    ))
    .expect("protected claim plan");
    assert_eq!(prepared.operation, "claim_from_lock");
    assert_eq!(prepared.state, "prepared");
    assert!(!prepared.submission_ready);

    let authorized = services
        .authorize_passport_vault_call()
        .execute(AuthorizePassportVaultCallCommand {
            profile_id: profile.id.clone(),
            draft_id: prepared.draft_id.clone(),
            authorization_challenge: prepared.authorization_challenge,
            confirmed: true,
            intent: AUTHORIZE_PASSPORT_VAULT_CALL_INTENT.to_owned(),
        })
        .expect("managed claim authorization and composition");
    assert_eq!(authorized.state, "authorized");
    assert!(authorized.submission_ready);

    let submitted = block_on(services.submit_passport_vault_call().execute(
        SubmitPassportVaultCallCommand {
            profile_id: profile.id.clone(),
            draft_id: prepared.draft_id.clone(),
            confirmed: true,
            intent: SUBMIT_PASSPORT_VAULT_CALL_INTENT.to_owned(),
        },
    ))
    .expect("native claim settlement");
    assert_eq!(submitted.call.operation, "claim_from_lock");
    assert_eq!(submitted.call.state, "submitted");
    assert_eq!(submitted.mode, "simulated");
    assert_ne!(submitted.transaction_hash_hex, "00".repeat(32));
    assert_ne!(submitted.block_hash_hex, "00".repeat(32));
}
