// SPDX-License-Identifier: Apache-2.0

use std::{fs, io::Cursor, path::PathBuf, process::Command, sync::Arc};

use midnight_base_crypto::schnorr::Signature;
use midnight_ledger::structure::{INITIAL_PARAMETERS, ProofPreimageMarker, Transaction};
use midnight_serialize::{tagged_deserialize, tagged_serialize};
use midnight_storage::DefaultDB;
use midnight_transient_crypto::commitment::PedersenRandomness;
use midnight_zswap::ledger::State as ZswapChainState;
use oxid_foundation::{OpaqueId, UnixTimestampMillis};
use oxid_passport_vault_application::{
    PassportVaultCallDraftState, PassportVaultCallOperation, PassportVaultCallPortError,
    PassportVaultContractCallPort, PassportVaultContractStateAuthentication,
    PassportVaultContractStateSnapshot, PreparePassportVaultCallRequest,
};
use oxid_passport_vault_domain::PassportVaultPolicy;
use serde::Deserialize;

use crate::{
    NativePassportVaultContractCall, PassportVaultCallCompositionContext,
    PassportVaultCallCompositionContextSource,
};

const MAX_COMPOSER_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const CONTRACT_STATE_FIXTURE: &str =
    include_str!("../../../../fixtures/passport-vault/contract-state-v1.hex");

type UnprovenTransaction =
    Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComposerResponse {
    schema_version: u8,
    ok: bool,
    operation_kind: String,
    circuit_id: String,
    unproven_transaction_hex: String,
    unproven_transaction_bytes: usize,
}

struct PublicContext;

impl PassportVaultCallCompositionContextSource for PublicContext {
    fn context(
        &self,
        _: &OpaqueId,
    ) -> Result<PassportVaultCallCompositionContext, PassportVaultCallPortError> {
        let mut zswap_chain_state = Vec::new();
        tagged_serialize(&ZswapChainState::<DefaultDB>::new(), &mut zswap_chain_state)
            .map_err(|_| PassportVaultCallPortError::InvalidData)?;
        let mut ledger_parameters = Vec::new();
        tagged_serialize(&INITIAL_PARAMETERS, &mut ledger_parameters)
            .map_err(|_| PassportVaultCallPortError::InvalidData)?;
        PassportVaultCallCompositionContext::new(
            "undeployed",
            zswap_chain_state,
            ledger_parameters,
            hex::decode("1bd4f827be97ff013c4a702e4b08f30ec378728a54670cf7cc92cb9b1a14eff6")
                .map_err(|_| PassportVaultCallPortError::InvalidData)?
                .try_into()
                .map_err(|_| PassportVaultCallPortError::InvalidData)?,
            hex::decode("b62e630a030171b5e11af2487f0103e650cc703f284d0a478b2a3abdf9715b70")
                .map_err(|_| PassportVaultCallPortError::InvalidData)?
                .try_into()
                .map_err(|_| PassportVaultCallPortError::InvalidData)?,
            [3; 32],
        )
    }
}

#[test]
fn packaged_composer_emits_a_rust_compatible_unproven_call_when_configured() {
    let Some(executable) = std::env::var_os("OXID_PASSPORT_VAULT_COMPOSER") else {
        return;
    };
    let executable = PathBuf::from(executable);
    assert!(executable.is_absolute());
    assert_eq!(
        fs::canonicalize(&executable).expect("canonical composer"),
        executable
    );
    let metadata = fs::symlink_metadata(&executable).expect("composer metadata");
    assert!(metadata.is_file());
    assert!(!metadata.file_type().is_symlink());

    let request = serde_json::json!({
        "schemaVersion": 1,
        "operation": {
            "kind": "create_lock",
            "minimumAgeYears": 18,
            "requiredIssuingStateHex": null,
            "requiredDocumentNumberHex": null,
            "maximumClaimAmount": "40",
            "verifierChallengeHashHex": "01".repeat(32),
            "initialAmount": "0"
        },
        "chain": {
            "contractStateHex": CONTRACT_STATE_FIXTURE.trim(),
            "contractAddressHex": "00".repeat(32),
            "zswapChainStateHex": null,
            "ledgerParametersHex": null,
            "networkId": "undeployed"
        },
        "wallet": {
            "coinPublicKeyHex": "1bd4f827be97ff013c4a702e4b08f30ec378728a54670cf7cc92cb9b1a14eff6",
            "encryptionPublicKeyHex": "b62e630a030171b5e11af2487f0103e650cc703f284d0a478b2a3abdf9715b70"
        }
    });
    let request = serde_json::to_vec(&request).expect("composer request");
    let output = Command::new(&executable)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;

            let mut stdin = child.stdin.take().expect("piped composer stdin");
            stdin.write_all(&request)?;
            stdin.flush()?;
            drop(stdin);
            child.wait_with_output()
        })
        .expect("run packaged composer");
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(output.stdout.len() <= MAX_COMPOSER_RESPONSE_BYTES);

    let response: ComposerResponse =
        serde_json::from_slice(&output.stdout).expect("bounded composer response");
    assert_eq!(response.schema_version, 1);
    assert!(response.ok);
    assert_eq!(response.operation_kind, "create_lock");
    assert_eq!(response.circuit_id, "createLock");
    let bytes = hex::decode(&response.unproven_transaction_hex).expect("transaction hex");
    assert_eq!(bytes.len(), response.unproven_transaction_bytes);
    assert!(bytes.len() > 100);

    let transaction: UnprovenTransaction =
        tagged_deserialize(&mut Cursor::new(bytes)).expect("official unproven transaction");
    let Transaction::Standard(standard) = transaction else {
        panic!("generated Compact composer must emit a standard transaction");
    };
    assert!(!standard.network_id.is_empty());
    assert_eq!(standard.intents.iter().count(), 1);

    let adapter = NativePassportVaultContractCall::new(&executable, Arc::new(PublicContext))
        .expect("native composer adapter");
    let preview = adapter
        .prepare(PreparePassportVaultCallRequest {
            profile_id: OpaqueId::parse("profile_composer_conformance").expect("profile"),
            contract_state: PassportVaultContractStateSnapshot {
                serialized_contract_state: hex::decode(CONTRACT_STATE_FIXTURE.trim())
                    .expect("contract state"),
                authentication: PassportVaultContractStateAuthentication::CanonicalFinalizedReplay,
                contract_address_hex: "00".repeat(32),
                transaction_hash_hex: "11".repeat(32),
                action_block_hash_hex: "22".repeat(32),
                action_block_height: 10,
                finalized_head_hash_hex: "33".repeat(32),
                finalized_head_height: 12,
            },
            operation: PassportVaultCallOperation::CreateLock {
                policy: PassportVaultPolicy::new(18, None, None, 40, [1; 32]).expect("policy"),
                initial_amount: 0,
            },
            expires_at: UnixTimestampMillis::new(1_800_000_000_000),
        })
        .expect("retained native composition");
    assert_eq!(preview.state, PassportVaultCallDraftState::Prepared);
}
