// SPDX-License-Identifier: Apache-2.0

use oxid_capabilities_application::{
    CapabilityManifestContext, CapabilityValue, capability_manifest as shared_capability_manifest,
};
use oxid_credential_application::{
    CredentialDisclosurePlanView, CredentialDisclosureView, CredentialView,
};
use oxid_diagnostics_application::DiagnosticSnapshotView;
use oxid_identity_application::DidRecordView;
use oxid_passport_vault_application::{
    PassportVaultCallPreviewView, PassportVaultCallSubmissionStatusView,
    PassportVaultCallSubmissionView, PassportVaultLockView, PassportVaultView,
};
use oxid_presentation_application::CredentialPresentationView;
use oxid_protocol_application::{CredentialIssuanceView, SelfIssuedAuthenticationView};
use oxid_wallet_application::{
    DerivedWalletAccountView, WalletAccountView, WalletDustRegistrationPreviewView,
    WalletDustRegistrationSubmissionStatusView, WalletDustRegistrationSubmissionView,
    WalletDustSyncView, WalletKeyView, WalletNetworkListView, WalletSecurityStatusView,
    WalletShieldedSyncView, WalletTransferPreviewView, WalletTransferSubmissionStatusView,
    WalletTransferSubmissionView,
};
use oxid_wallet_domain::{
    PublicKeyEncoding, WalletKeyAlgorithm, WalletKeyPurpose, WalletProtectionClass,
    WalletProtectionState,
};
use serde_json::{Value, json};

pub(super) fn network_list_value(networks: &WalletNetworkListView) -> Value {
    json!({
        "selectedNetworkId": networks.selected_network_id,
        "networks": networks.networks.iter().map(|network| json!({
            "chain": network.chain,
            "networkId": network.network_id,
            "displayName": network.display_name,
            "environment": network.environment,
            "selected": network.selected
        })).collect::<Vec<_>>()
    })
}

pub(super) fn account_value(account: &WalletAccountView) -> Value {
    json!({
        "chain": account.chain,
        "networkId": account.network_id,
        "networkName": account.network_name,
        "networkEnvironment": account.network_environment,
        "accountId": account.account_id,
        "source": account.source,
        "addresses": account.addresses.iter().map(address_value).collect::<Vec<_>>(),
        "balances": account.balances.iter().map(balance_value).collect::<Vec<_>>(),
        "sync": sync_value(account),
        "transactions": account.transactions.iter().map(transaction_value).collect::<Vec<_>>()
    })
}

pub(super) fn derived_account_value(account: &DerivedWalletAccountView) -> Value {
    json!({
        "networkId": account.network_id,
        "accountId": account.account_id,
        "accountIndex": account.account_index,
        "addressIndex": account.address_index,
        "receiveAddress": address_value(&account.receive_address),
        "addresses": account.addresses.iter().map(address_value).collect::<Vec<_>>(),
        "transactionKeyRef": account.transaction_key_reference,
        "custodyMode": "development_only"
    })
}

pub(super) fn address_value(address: &oxid_wallet_application::WalletAddressView) -> Value {
    json!({ "kind": address.kind, "value": address.value })
}

pub(super) fn balance_value(balance: &oxid_wallet_application::WalletAssetBalanceView) -> Value {
    json!({
        "assetId": balance.asset_id,
        "symbol": balance.symbol,
        "decimals": balance.decimals,
        "atomicUnits": balance.atomic_units
    })
}

pub(super) fn sync_value(account: &WalletAccountView) -> Value {
    json!({
        "state": account.sync.state,
        "currentCursor": account.sync.current_cursor,
        "targetCursor": account.sync.target_cursor,
        "chainTipHeight": account.sync.chain_tip_height,
        "updatedAtMillis": account.sync.updated_at_millis
    })
}

pub(super) fn dust_sync_value(status: &WalletDustSyncView) -> Value {
    json!({
        "networkId": status.network_id,
        "state": status.state,
        "currentCursor": status.current_cursor,
        "targetCursor": status.target_cursor,
        "eventsProcessed": status.events_processed,
        "balance": {
            "assetId": "midnight:dust",
            "symbol": "DUST",
            "decimals": 15,
            "atomicUnits": status.balance_atomic_units
        },
        "updatedAtMillis": status.updated_at_millis,
        "failure": status.failure
    })
}

pub(super) fn dust_registration_preview_value(
    preview: &WalletDustRegistrationPreviewView,
) -> Value {
    json!({
        "draftId": preview.draft_id,
        "authorizationChallenge": preview.authorization_challenge,
        "networkId": preview.network_id,
        "accountId": preview.account_id,
        "registeredNight": dust_registration_asset_value(&preview.registered_night),
        "inputCount": preview.input_count,
        "maximumFeeAllowance": dust_registration_asset_value(&preview.maximum_fee_allowance),
        "feeState": preview.fee_state,
        "expiresAtMillis": preview.expires_at_millis,
        "state": preview.state,
        "authorizationReady": preview.authorization_ready,
        "submissionReady": preview.submission_ready,
        "custodyMode": "protected_role_2"
    })
}

pub(super) fn dust_registration_submission_value(
    submission: &WalletDustRegistrationSubmissionView,
) -> Value {
    json!({
        "registration": dust_registration_preview_value(&submission.registration),
        "transactionId": submission.transaction_id,
        "blockId": submission.block_id,
        "fee": dust_registration_asset_value(&submission.fee),
        "mode": submission.mode,
        "registrationObservation": submission.registration_observation,
        "dustReadiness": submission.dust_readiness,
        "custodyMode": "protected_role_2"
    })
}

pub(super) fn dust_registration_status_value(
    status: &WalletDustRegistrationSubmissionStatusView,
) -> Value {
    json!({
        "draftId": status.draft_id,
        "state": status.state,
        "transactionId": status.transaction_id,
        "blockId": status.block_id,
        "fee": status.fee.as_ref().map(dust_registration_asset_value),
        "mode": status.mode,
        "registrationObservation": status.registration_observation,
        "dustReadiness": status.dust_readiness,
        "cancellationAllowed": status.cancellation_allowed,
        "reconciliationAllowed": status.reconciliation_allowed,
        "custodyMode": "protected_role_2"
    })
}

pub(super) fn dust_registration_asset_value(
    asset: &oxid_wallet_application::WalletDustRegistrationAssetView,
) -> Value {
    json!({
        "assetId": asset.asset_id,
        "symbol": asset.symbol,
        "decimals": asset.decimals,
        "atomicUnits": asset.atomic_units,
    })
}

pub(super) fn diagnostic_snapshot_value(snapshot: &DiagnosticSnapshotView) -> Value {
    json!({
        "persistence": "process_local",
        "telemetry": "off",
        "payloadsRetained": false,
        "capacity": snapshot.capacity(),
        "totalEvents": snapshot.total_events(),
        "retainedEvents": snapshot.recent().len(),
        "evictedEvents": snapshot.evicted_events(),
        "counts": snapshot.counts().iter().map(|count| json!({
            "code": count.code().as_str(),
            "severity": count.severity().as_str(),
            "occurrences": count.occurrences(),
        })).collect::<Vec<_>>(),
        "recent": snapshot.recent().iter().map(|event| json!({
            "sequence": event.sequence(),
            "code": event.code().as_str(),
            "severity": event.severity().as_str(),
        })).collect::<Vec<_>>(),
    })
}

pub(super) fn shielded_sync_value(status: &WalletShieldedSyncView) -> Value {
    json!({
        "networkId": status.network_id,
        "state": status.state,
        "currentCursor": status.current_cursor,
        "targetCursor": status.target_cursor,
        "eventsProcessed": status.events_processed,
        "ownedNoteCount": status.owned_note_count,
        "commitmentCount": status.commitment_count,
        "balances": status.balances.iter().map(|balance| json!({
            "tokenType": balance.token_type_hex,
            "atomicUnits": balance.atomic_units
        })).collect::<Vec<_>>(),
        "updatedAtMillis": status.updated_at_millis,
        "failure": status.failure
    })
}

pub(super) fn transaction_value(
    transaction: &oxid_wallet_application::WalletTransactionView,
) -> Value {
    json!({
        "transactionId": transaction.transaction_id,
        "direction": transaction.direction,
        "status": transaction.status,
        "blockHeight": transaction.block_height,
        "observedAtMillis": transaction.observed_at_millis,
        "changes": transaction.changes.iter().map(|change| json!({
            "direction": change.direction,
            "balance": balance_value(&change.balance)
        })).collect::<Vec<_>>(),
        "fee": transaction.fee.as_ref().map(balance_value)
    })
}

pub(super) fn transfer_preview_value(preview: &WalletTransferPreviewView) -> Value {
    json!({
        "draftId": preview.draft_id,
        "authorizationChallenge": preview.authorization_challenge,
        "networkId": preview.network_id,
        "accountId": preview.account_id,
        "recipientAddress": preview.recipient_address,
        "recipientKind": preview.recipient_kind,
        "amount": transfer_asset_value(&preview.amount),
        "change": transfer_asset_value(&preview.change),
        "fee": preview.fee.as_ref().map(transfer_asset_value),
        "feeState": preview.fee_state,
        "inputCount": preview.input_count,
        "expiresAtMillis": preview.expires_at_millis,
        "state": preview.state,
        "proofRequired": preview.proof_required,
        "submissionReady": preview.submission_ready,
        "custodyMode": "development_only"
    })
}

pub(super) fn transfer_submission_value(submission: &WalletTransferSubmissionView) -> Value {
    json!({
        "transfer": transfer_preview_value(&submission.transfer),
        "transactionId": submission.transaction_id,
        "blockId": submission.block_id,
        "fee": transfer_asset_value(&submission.fee),
        "mode": submission.mode,
        "custodyMode": "development_only"
    })
}

pub(super) fn transfer_submission_status_value(
    status: &WalletTransferSubmissionStatusView,
) -> Value {
    json!({
        "draftId": status.draft_id,
        "state": status.state,
        "cancellationAllowed": status.cancellation_allowed,
        "retryable": status.retryable,
        "replacementAllowed": status.replacement_allowed,
        "reconciliationAllowed": status.reconciliation_allowed,
        "transactionId": status.transaction_id,
        "blockId": status.block_id,
        "fee": status.fee.as_ref().map(transfer_asset_value),
        "mode": status.mode,
        "custodyMode": "development_only"
    })
}

pub(super) fn transfer_asset_value(
    asset: &oxid_wallet_application::WalletTransferAssetView,
) -> Value {
    json!({
        "assetId": asset.asset_id,
        "symbol": asset.symbol,
        "decimals": asset.decimals,
        "atomicUnits": asset.atomic_units,
    })
}

pub(super) fn did_record_value(record: &DidRecordView) -> Value {
    let document = &record.document;
    json!({
        "document": {
            "contexts": document.contexts,
            "id": document.id,
            "network": document.network,
            "alsoKnownAs": document.also_known_as,
            "verificationMethods": document.verification_methods.iter().map(|method| json!({
                "id": method.id,
                "controller": method.controller,
                "publicKeyJwk": {
                    "kty": method.public_key_jwk.key_type,
                    "crv": method.public_key_jwk.curve,
                    "x": method.public_key_jwk.x,
                    "y": method.public_key_jwk.y,
                }
            })).collect::<Vec<_>>(),
            "relationships": document.relationships.iter().map(|relationship| json!({
                "relationship": relationship.relationship,
                "methodIds": relationship.method_ids,
            })).collect::<Vec<_>>(),
            "services": document.services.iter().map(|service| json!({
                "id": service.id,
                "types": service.types,
                "endpoints": service.endpoints.iter().map(|endpoint| json!({
                    "value": endpoint.value,
                    "jsonObject": endpoint.is_json_object,
                })).collect::<Vec<_>>(),
                "endpointWasArray": service.endpoint_was_array,
            })).collect::<Vec<_>>(),
        },
        "documentMetadata": {
            "created": record.document_metadata.created,
            "updated": record.document_metadata.updated,
            "deactivated": record.document_metadata.deactivated,
            "versionId": record.document_metadata.version_id,
            "nextUpdate": record.document_metadata.next_update,
            "nextVersionId": record.document_metadata.next_version_id,
            "equivalentIds": record.document_metadata.equivalent_ids,
            "canonicalId": record.document_metadata.canonical_id,
        },
        "contentType": record.content_type,
        "source": record.source,
    })
}

pub(super) fn credential_value(credential: &CredentialView) -> Value {
    json!({
        "id": credential.id,
        "displayName": credential.display_name,
        "issuerDid": credential.issuer_did,
        "subjectDid": credential.subject_did,
        "format": credential.format,
        "issuedAtMs": credential.issued_at_ms,
        "verification": {
            "outcome": credential.verification_outcome,
            "stages": credential.verification_stages.iter().map(|stage| json!({
                "name": stage.name,
                "status": stage.status,
                "reasonCode": stage.reason_code,
            })).collect::<Vec<_>>(),
        },
    })
}

pub(super) fn credential_disclosure_value(disclosure: &CredentialDisclosureView) -> Value {
    json!({
        "credentialId": disclosure.credential_id,
        "schemaId": disclosure.schema_id,
        "candidates": disclosure.candidates.iter().map(|candidate| json!({
            "claimPath": candidate.claim_path,
            "label": candidate.label,
            "privacyTier": candidate.privacy_tier,
        })).collect::<Vec<_>>(),
    })
}

pub(super) fn credential_disclosure_plan_value(plan: &CredentialDisclosurePlanView) -> Value {
    json!({
        "credentialId": plan.credential_id,
        "schemaId": plan.schema_id,
        "reveals": plan.reveals.iter().map(|candidate| json!({
            "claimPath": candidate.claim_path,
            "label": candidate.label,
            "privacyTier": candidate.privacy_tier,
        })).collect::<Vec<_>>(),
        "predicates": plan.predicates.iter().map(|predicate| json!({
            "claimPath": predicate.claim_path,
            "label": predicate.label,
            "kind": predicate.kind,
            "threshold": predicate.threshold,
        })).collect::<Vec<_>>(),
        "outcome": plan.outcome,
        "presentationGenerated": plan.presentation_generated,
    })
}

pub(super) fn credential_issuance_value(issuance: &CredentialIssuanceView) -> Value {
    json!({
        "id": issuance.id,
        "issuer": issuance.issuer,
        "configurationIds": issuance.configuration_ids,
        "displayNames": issuance.display_names,
        "state": issuance.state,
        "credentialId": issuance.credential_id,
        "failureCode": issuance.failure_code,
    })
}

pub(super) fn credential_presentation_value(presentation: &CredentialPresentationView) -> Value {
    json!({
        "id": presentation.id,
        "verifier": presentation.verifier,
        "purpose": presentation.purpose,
        "queryId": presentation.query_id,
        "candidates": presentation.candidates.iter().map(|candidate| json!({
            "credentialId": candidate.credential_id,
            "displayName": candidate.display_name,
            "issuer": candidate.issuer,
        })).collect::<Vec<_>>(),
        "requestedClaims": presentation.requested_claims.iter().map(|claim| json!({
            "claimPath": claim.claim_path,
            "label": claim.label,
            "intent": claim.intent,
            "predicateKind": claim.predicate_kind,
            "threshold": claim.threshold,
        })).collect::<Vec<_>>(),
        "state": presentation.state,
        "presentationGenerated": presentation.presentation_generated,
        "verifierValidated": presentation.verifier_validated,
        "failureCode": presentation.failure_code,
    })
}

pub(super) fn self_issued_authentication_value(
    authentication: &SelfIssuedAuthenticationView,
) -> Value {
    json!({
        "id": authentication.id,
        "verifier": authentication.verifier,
        "purpose": authentication.purpose,
        "state": authentication.state,
        "failureCode": authentication.failure_code,
    })
}

pub(super) fn security_status_value(status: WalletSecurityStatusView) -> Value {
    json!({
        "state": match status.state {
            WalletProtectionState::Uninitialized => "uninitialized",
            WalletProtectionState::Locked => "locked",
            WalletProtectionState::Unlocked => "unlocked",
            WalletProtectionState::Unavailable => "unavailable",
        },
        "protection": match status.protection {
            WalletProtectionClass::DevelopmentOnly => "development_only",
            WalletProtectionClass::OperatingSystem => "operating_system",
            WalletProtectionClass::HardwareBacked => "hardware_backed",
            WalletProtectionClass::Unavailable => "unavailable",
        },
        "userPresenceRequired": status.user_presence_required,
        "portableBackupSupported": status.portable_backup_supported,
    })
}

pub(super) fn key_value(key: &WalletKeyView) -> Value {
    json!({
        "keyRef": key.key_reference,
        "label": key.label,
        "algorithm": algorithm_name(key.algorithm),
        "purpose": purpose_name(key.purpose),
        "publicKey": {
            "encoding": match key.public_key_encoding {
                PublicKeyEncoding::Ed25519Compressed => "ed25519-compressed",
                PublicKeyEncoding::Sec1Compressed => "sec1-compressed",
                PublicKeyEncoding::Secp256k1XOnly => "secp256k1-x-only",
                PublicKeyEncoding::JubjubCompressed => "jubjub-compressed",
            },
            "bytesHex": encode_hex(&key.public_key_bytes),
        },
        "createdAtMillis": key.created_at_millis,
    })
}

pub(super) const fn algorithm_name(algorithm: WalletKeyAlgorithm) -> &'static str {
    match algorithm {
        WalletKeyAlgorithm::Ed25519 => "ed25519",
        WalletKeyAlgorithm::P256 => "p256",
        WalletKeyAlgorithm::Secp256k1Schnorr => "secp256k1-schnorr",
        WalletKeyAlgorithm::Jubjub => "jubjub",
    }
}

const fn purpose_name(purpose: WalletKeyPurpose) -> &'static str {
    match purpose {
        WalletKeyPurpose::Transaction => "transaction",
        WalletKeyPurpose::Authentication => "authentication",
        WalletKeyPurpose::Assertion => "assertion",
        WalletKeyPurpose::KeyAgreement => "key_agreement",
        WalletKeyPurpose::Recovery => "recovery",
    }
}

pub(super) fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

pub(super) fn passport_vault_lock_value(lock: &PassportVaultLockView) -> Value {
    json!({
        "lockId": lock.lock_id,
        "creatorProfileId": lock.creator_profile_id,
        "policy": {
            "minimumAgeYears": lock.minimum_age_years,
            "requiredIssuingState": lock.required_issuing_state,
            "requiredDocumentNumber": lock.required_document_number,
            "maximumClaimAmount": lock.maximum_claim_amount,
            "verifierChallengeHex": lock.verifier_challenge_hex,
        },
        "totalDeposited": lock.total_deposited,
        "totalReleased": lock.total_released,
        "remaining": lock.remaining,
    })
}

pub(super) fn passport_vault_value(vault: &PassportVaultView) -> Value {
    json!({
        "source": vault.source,
        "chainAnchor": vault.chain_anchor.as_ref().map(|anchor| json!({
            "contractAddressHex": anchor.contract_address_hex,
            "transactionHashHex": anchor.transaction_hash_hex,
            "actionBlockHashHex": anchor.action_block_hash_hex,
            "actionBlockHeight": anchor.action_block_height,
            "finalizedHeadHashHex": anchor.finalized_head_hash_hex,
            "finalizedHeadHeight": anchor.finalized_head_height,
            "finalizedHeadTimeSeconds": anchor.finalized_head_time_seconds,
            "stateAuthentication": anchor.state_authentication,
        })),
        "contract": vault.contract.as_ref().map(|contract| json!({
            "version": contract.version,
            "trustedIssuerDidContractHex": contract.trusted_issuer_did_contract_hex,
            "trustedIssuerMethodHex": contract.trusted_issuer_method_hex,
            "trustedIssuerPublicKeyHashHex": contract.trusted_issuer_public_key_hash_hex,
            "consumedClaimCount": contract.consumed_claim_count,
            "lastVerifiedCurrentDay": contract.last_verified_current_day,
            "lastVerifiedThresholdYears": contract.last_verified_threshold_years,
            "lastReleasedAmount": contract.last_released_amount,
            "lastBusinessDecision": contract.last_business_decision,
        })),
        "totalDeposited": vault.total_deposited,
        "totalReleased": vault.total_released,
        "totalLocked": vault.total_locked,
        "claimCount": vault.claim_count,
        "locks": vault.locks.iter().map(passport_vault_lock_value).collect::<Vec<_>>(),
    })
}

pub(super) fn passport_vault_call_preview_value(call: &PassportVaultCallPreviewView) -> Value {
    json!({
        "draftId": call.draft_id,
        "authorizationChallenge": call.authorization_challenge,
        "contractAddressHex": call.contract_address_hex,
        "operation": call.operation,
        "lockId": call.lock_id,
        "amountAtomicUnits": call.amount_atomic_units,
        "stateAnchor": {
            "transactionHashHex": call.state_anchor_transaction_hash_hex,
            "blockHashHex": call.state_anchor_block_hash_hex,
            "blockHeight": call.state_anchor_block_height,
            "stateAuthentication": "canonical_finalized_replay",
        },
        "expiresAtMillis": call.expires_at_millis,
        "state": call.state,
        "feeAtomicUnits": call.fee_atomic_units,
        "proofRequired": call.proof_required,
        "submissionReady": call.submission_ready,
    })
}

pub(super) fn passport_vault_call_submission_value(
    submission: &PassportVaultCallSubmissionView,
) -> Value {
    json!({
        "call": passport_vault_call_preview_value(&submission.call),
        "transactionHashHex": submission.transaction_hash_hex,
        "blockHashHex": submission.block_hash_hex,
        "blockHeight": submission.block_height,
        "feeAtomicUnits": submission.fee_atomic_units,
        "mode": submission.mode,
    })
}

pub(super) fn passport_vault_call_submission_status_value(
    status: &PassportVaultCallSubmissionStatusView,
) -> Value {
    json!({
        "draftId": status.draft_id,
        "state": status.state,
        "cancellationAllowed": status.cancellation_allowed,
        "retryable": status.retryable,
        "replacementAllowed": status.replacement_allowed,
        "reconciliationAllowed": status.reconciliation_allowed,
        "transactionHashHex": status.transaction_hash_hex,
        "blockHashHex": status.block_hash_hex,
        "blockHeight": status.block_height,
        "feeAtomicUnits": status.fee_atomic_units,
        "mode": status.mode,
    })
}

pub(super) fn capability_value(value: &CapabilityValue) -> Value {
    match value {
        CapabilityValue::Text(value) => Value::String(value.clone()),
        CapabilityValue::Boolean(value) => Value::Bool(*value),
        CapabilityValue::TextList(values) => {
            Value::Array(values.iter().cloned().map(Value::String).collect())
        }
        CapabilityValue::Object(facts) => Value::Object(
            facts
                .iter()
                .map(|fact| (fact.key().to_owned(), capability_value(fact.value())))
                .collect(),
        ),
        CapabilityValue::Null => Value::Null,
    }
}

pub(super) fn capability_manifest(
    compact_presentation_proof_available: bool,
    passport_vault_call_mode: &str,
    passport_vault_state_persistence: &str,
) -> Value {
    Value::Array(
        shared_capability_manifest(CapabilityManifestContext::new(
            compact_presentation_proof_available,
            passport_vault_call_mode,
            passport_vault_state_persistence,
        ))
        .iter()
        .map(|capability| {
            let mut object = serde_json::Map::from_iter([
                (
                    "method".to_owned(),
                    Value::String(capability.method().to_owned()),
                ),
                (
                    "status".to_owned(),
                    Value::String(capability.status().to_owned()),
                ),
            ]);
            object.extend(
                capability
                    .facts()
                    .iter()
                    .map(|fact| (fact.key().to_owned(), capability_value(fact.value()))),
            );
            Value::Object(object)
        })
        .collect(),
    )
}
#[cfg(test)]
mod tests {
    use oxid_passport_vault_application::PassportVaultView;
    use serde_json::json;

    use super::{capability_manifest, passport_vault_value};

    #[test]
    fn native_settlement_manifest_includes_conformant_claim_and_reports_recovery() {
        let methods = capability_manifest(false, "native_settlement", "owner_private_atomic_file");
        let methods = methods.as_array().expect("capability array");
        let prepare = methods
            .iter()
            .find(|capability| capability["method"] == "vault.contract_call.prepare")
            .expect("prepare capability");
        assert_eq!(prepare["status"], "ready");
        assert_eq!(
            prepare["operations"],
            json!([
                "create_lock",
                "deposit_to_lock",
                "claim_from_lock",
                "withdraw_from_lock"
            ])
        );
        let history = methods
            .iter()
            .find(|capability| capability["method"] == "vault.contract_call.submission_history")
            .expect("history capability");
        assert_eq!(history["persistence"], "public_metadata_only");
        let reconcile = methods
            .iter()
            .find(|capability| capability["method"] == "vault.contract_call.reconcile_submission")
            .expect("reconciliation capability");
        assert_eq!(reconcile["scope"], "finalized_chain");
    }

    #[test]
    fn contract_state_projection_discloses_the_unproven_indexer_boundary() {
        let view = PassportVaultView {
            source: "node_anchored_indexer".to_owned(),
            chain_anchor: Some(
                oxid_passport_vault_application::PassportVaultChainAnchorView {
                    contract_address_hex: "11".repeat(32),
                    transaction_hash_hex: "22".repeat(32),
                    action_block_hash_hex: "33".repeat(32),
                    action_block_height: 40,
                    finalized_head_hash_hex: "44".repeat(32),
                    finalized_head_height: 42,
                    finalized_head_time_seconds: 1_700_000_000,
                    state_authentication: "indexer_supplied_not_proven".to_owned(),
                },
            ),
            contract: None,
            locks: Vec::new(),
            total_deposited: "0".to_owned(),
            total_released: "0".to_owned(),
            total_locked: "0".to_owned(),
            claim_count: 0,
        };
        let value = passport_vault_value(&view);
        assert_eq!(
            value["chainAnchor"]["stateAuthentication"],
            "indexer_supplied_not_proven"
        );
        assert_eq!(value["chainAnchor"]["actionBlockHeight"], 40);
        assert_eq!(value["chainAnchor"]["finalizedHeadHeight"], 42);
        assert_eq!(
            value["chainAnchor"]["finalizedHeadTimeSeconds"],
            1_700_000_000_u64
        );
    }
}
