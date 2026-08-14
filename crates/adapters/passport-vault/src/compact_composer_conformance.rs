// SPDX-License-Identifier: Apache-2.0

use std::{fs, io::Cursor, path::PathBuf, process::Command};

use midnight_base_crypto::schnorr::Signature;
use midnight_ledger::structure::{ProofPreimageMarker, Transaction};
use midnight_serialize::tagged_deserialize;
use midnight_storage::DefaultDB;
use midnight_transient_crypto::commitment::PedersenRandomness;
use serde::Deserialize;

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
}
