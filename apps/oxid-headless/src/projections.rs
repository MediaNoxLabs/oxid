// SPDX-License-Identifier: Apache-2.0

use oxid_capabilities_application::{
    CapabilityManifestContext, CapabilityValue, capability_manifest as shared_capability_manifest,
};
use oxid_credential_application::{
    CredentialDisclosurePlanView, CredentialDisclosurePortError, CredentialDisclosureView,
    CredentialOperationError, CredentialRepositoryError, CredentialVerificationError,
    CredentialView,
};
use oxid_diagnostics_application::DiagnosticSnapshotView;
use oxid_identity_application::{
    DidLifecyclePortError, DidOperationError, DidRecordRepositoryError, DidRecordView,
    DidResolutionPortError,
};
use oxid_passport_vault_application::{
    PassportVaultCallError, PassportVaultCallPortError, PassportVaultCallPreviewView,
    PassportVaultCallSubmissionStatusView, PassportVaultCallSubmissionView,
    PassportVaultContractStateError, PassportVaultContractStateReadError,
    PassportVaultContractStateSourceError, PassportVaultLockView, PassportVaultOperationError,
    PassportVaultView, PreparePassportVaultCallAction,
};
use oxid_passport_vault_domain::PassportVaultError;
use oxid_presentation_application::{CredentialPresentationError, CredentialPresentationView};
use oxid_protocol_application::{
    CredentialIssuanceError, CredentialIssuanceView, IdentityRequestRoutingError,
    SelfIssuedAuthenticationError, SelfIssuedAuthenticationView,
};
use oxid_wallet_application::{
    DerivedWalletAccountView, SensitiveWalletOperationError, WalletAccountError,
    WalletAccountPortError, WalletAccountView, WalletDustRegistrationError,
    WalletDustRegistrationPortError, WalletDustRegistrationPreviewView,
    WalletDustRegistrationSubmissionStatusView, WalletDustRegistrationSubmissionView,
    WalletDustSyncError, WalletDustSyncPortError, WalletDustSyncView, WalletKeyError,
    WalletKeyView, WalletNetworkListView, WalletSecurityError, WalletSecurityPortError,
    WalletSecurityStatusView, WalletShieldedSyncError, WalletShieldedSyncPortError,
    WalletShieldedSyncView, WalletTransactionError, WalletTransactionPortError,
    WalletTransferPreviewView, WalletTransferSubmissionStatusView, WalletTransferSubmissionView,
};
use oxid_wallet_domain::{
    PublicKeyEncoding, WalletKeyAlgorithm, WalletKeyPurpose, WalletProtectionClass,
    WalletProtectionState,
};
use serde_json::{Value, json};

use crate::{
    parameters::VaultContractCallActionParams,
    protocol::{Dispatch, Response},
};

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

pub(super) fn identity_request_routing_error(
    id: Option<String>,
    error: IdentityRequestRoutingError,
) -> Response {
    let message = match error {
        IdentityRequestRoutingError::InvalidRequest => "identity request is invalid",
        IdentityRequestRoutingError::UnsupportedRequest => {
            "identity request protocol is unsupported"
        }
        IdentityRequestRoutingError::AmbiguousRequest => {
            "OpenID4VP endpoint is not registered and cannot be classified safely"
        }
        IdentityRequestRoutingError::Unavailable => {
            "identity request routing capability is unavailable"
        }
    };
    Response::error(id, error.code(), message)
}

pub(super) fn credential_issuance_error(
    id: Option<String>,
    error: CredentialIssuanceError,
) -> Response {
    let (code, message) = match error {
        CredentialIssuanceError::InvalidProfileIdentifier(_)
        | CredentialIssuanceError::InvalidIssuanceIdentifier(_)
        | CredentialIssuanceError::InvalidOffer
        | CredentialIssuanceError::InvalidHolder => (
            "invalid_argument",
            "credential issuance request contains invalid input",
        ),
        CredentialIssuanceError::ConfirmationRequired
        | CredentialIssuanceError::InvalidConfirmation => (
            "confirmation_required",
            "valid explicit credential issuance consent is required",
        ),
        CredentialIssuanceError::NotFound => (
            "not_found",
            "credential issuance session was not found for the active profile",
        ),
        CredentialIssuanceError::InvalidState => (
            "failed_precondition",
            "credential issuance session is not awaiting this operation",
        ),
        CredentialIssuanceError::Protocol(protocol) => (
            protocol.code(),
            "credential issuer protocol rejected or could not complete the request",
        ),
        CredentialIssuanceError::Sink(_) => (
            "credential_store_failed",
            "issued credential could not be verified and stored",
        ),
        CredentialIssuanceError::Unavailable => (
            "capability_unavailable",
            "credential issuance capability is unavailable",
        ),
    };
    Response::error(id, code, message)
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

pub(super) fn credential_presentation_error(
    id: Option<String>,
    error: CredentialPresentationError,
) -> Response {
    let (code, message) = match error {
        CredentialPresentationError::InvalidProfileIdentifier(_)
        | CredentialPresentationError::InvalidPresentationIdentifier(_)
        | CredentialPresentationError::InvalidRequest
        | CredentialPresentationError::InvalidCredential => (
            "invalid_argument",
            "credential presentation request contains invalid input",
        ),
        CredentialPresentationError::ConfirmationRequired
        | CredentialPresentationError::InvalidConfirmation => (
            "confirmation_required",
            "valid explicit credential presentation consent is required",
        ),
        CredentialPresentationError::NotFound => (
            "not_found",
            "credential presentation session was not found for the active profile",
        ),
        CredentialPresentationError::InvalidState => (
            "failed_precondition",
            "credential presentation session is not awaiting this operation",
        ),
        CredentialPresentationError::Protocol(protocol) => (
            protocol.code(),
            "credential presentation protocol rejected or could not complete the request",
        ),
        CredentialPresentationError::Unavailable => (
            "capability_unavailable",
            "credential presentation capability is unavailable",
        ),
    };
    Response::error(id, code, message)
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

pub(super) fn self_issued_authentication_error(
    id: Option<String>,
    error: SelfIssuedAuthenticationError,
) -> Response {
    let (code, message) = match error {
        SelfIssuedAuthenticationError::InvalidProfileIdentifier(_)
        | SelfIssuedAuthenticationError::InvalidAuthenticationIdentifier(_)
        | SelfIssuedAuthenticationError::InvalidRequest
        | SelfIssuedAuthenticationError::InvalidHolder => (
            "invalid_argument",
            "self-issued authentication request contains invalid input",
        ),
        SelfIssuedAuthenticationError::ConfirmationRequired
        | SelfIssuedAuthenticationError::InvalidConfirmation => (
            "confirmation_required",
            "valid explicit DID authentication consent is required",
        ),
        SelfIssuedAuthenticationError::NotFound => (
            "not_found",
            "self-issued authentication session was not found for the active profile",
        ),
        SelfIssuedAuthenticationError::InvalidState => (
            "failed_precondition",
            "self-issued authentication session is not awaiting this operation",
        ),
        SelfIssuedAuthenticationError::Protocol(protocol) => (
            protocol.code(),
            "self-issued authentication protocol rejected or could not complete the request",
        ),
        SelfIssuedAuthenticationError::Unavailable => (
            "capability_unavailable",
            "self-issued authentication capability is unavailable",
        ),
    };
    Response::error(id, code, message)
}

pub(super) fn credential_error(id: Option<String>, error: CredentialOperationError) -> Response {
    match error {
        CredentialOperationError::InvalidProfileIdentifier(_)
        | CredentialOperationError::InvalidCredentialIdentifier(_)
        | CredentialOperationError::Domain(_) => Response::error(
            id,
            "invalid_argument",
            "credential request contains invalid identifiers or metadata",
        ),
        CredentialOperationError::ConfirmationRequired
        | CredentialOperationError::InvalidConfirmation => Response::error(
            id,
            "confirmation_required",
            "valid explicit credential deletion confirmation is required",
        ),
        CredentialOperationError::VerificationNotValid => Response::error(
            id,
            "credential_verification_failed",
            "credential verification did not produce a valid outcome",
        ),
        CredentialOperationError::Ingress(_) => Response::error(
            id,
            "capability_unavailable",
            "credential ingress capability is unavailable",
        ),
        CredentialOperationError::Verification(error) => match error {
            CredentialVerificationError::Unavailable => Response::error(
                id,
                "capability_unavailable",
                "credential verification capability is unavailable",
            ),
            CredentialVerificationError::UnsupportedFormat => {
                Response::error(id, "unsupported_format", "credential format is unsupported")
            }
            CredentialVerificationError::InvalidCredential => Response::error(
                id,
                "invalid_credential",
                "credential structure or proof encoding is invalid",
            ),
        },
        CredentialOperationError::Disclosure(error) => match error {
            CredentialDisclosurePortError::Unavailable => Response::error(
                id,
                "capability_unavailable",
                "credential disclosure capability is unavailable",
            ),
            CredentialDisclosurePortError::UnsupportedCredential => Response::error(
                id,
                "unsupported_format",
                "credential schema does not support disclosure preview",
            ),
            CredentialDisclosurePortError::MissingPrivateMaterial => Response::error(
                id,
                "failed_precondition",
                "credential has no protected claim material",
            ),
            CredentialDisclosurePortError::InvalidPrivateMaterial => Response::error(
                id,
                "invalid_credential",
                "credential protected claim material is invalid",
            ),
            CredentialDisclosurePortError::ClaimNotFound
            | CredentialDisclosurePortError::ClaimNotRevealable => Response::error(
                id,
                "invalid_argument",
                "credential disclosure selection is invalid",
            ),
        },
        CredentialOperationError::Persistence(error) => match error {
            CredentialRepositoryError::NotFound => {
                Response::error(id, "not_found", "credential was not found")
            }
            CredentialRepositoryError::CapacityExceeded => {
                Response::error(id, "capacity_exceeded", "credential capacity was exceeded")
            }
            CredentialRepositoryError::Integrity => Response::error(
                id,
                "integrity_error",
                "credential storage failed integrity validation",
            ),
            CredentialRepositoryError::Unavailable => Response::error(
                id,
                "capability_unavailable",
                "credential storage is unavailable",
            ),
        },
    }
}

pub(super) fn did_error(id: Option<String>, error: DidOperationError) -> Response {
    match error {
        DidOperationError::InvalidProfileIdentifier(_) | DidOperationError::InvalidDid(_) => {
            Response::error(
                id,
                "invalid_argument",
                "active profile or Midnight DID is invalid",
            )
        }
        DidOperationError::SubjectMismatch => Response::error(
            id,
            "invalid_response",
            "resolved DID document does not match the requested subject",
        ),
        DidOperationError::InvalidNetwork => Response::error(
            id,
            "unsupported_network",
            "Midnight DID network is unsupported",
        ),
        DidOperationError::EmptyPayload | DidOperationError::PayloadTooLarge => {
            Response::error(id, "invalid_argument", "DID signing payload is invalid")
        }
        DidOperationError::ConfirmationRequired | DidOperationError::InvalidConfirmation => {
            Response::error(
                id,
                "confirmation_required",
                "valid explicit confirmation is required",
            )
        }
        DidOperationError::Lifecycle(error) => match error {
            DidLifecyclePortError::Unavailable | DidLifecyclePortError::ProtectionUnavailable => {
                Response::error(
                    id,
                    "capability_unavailable",
                    "DID lifecycle capability is unavailable",
                )
            }
            DidLifecyclePortError::UnsupportedNetwork => Response::error(
                id,
                "unsupported_network",
                "DID network does not support standalone lifecycle operations",
            ),
            DidLifecyclePortError::UnsupportedAlgorithm => Response::error(
                id,
                "unsupported_algorithm",
                "DID key algorithm is unsupported",
            ),
            DidLifecyclePortError::NotManaged => Response::error(
                id,
                "failed_precondition",
                "DID is not managed by the current protected session",
            ),
            DidLifecyclePortError::NotFound => {
                Response::error(id, "not_found", "DID document entry was not found")
            }
            DidLifecyclePortError::Conflict => Response::error(
                id,
                "conflict",
                "DID document update conflicts with current state",
            ),
            DidLifecyclePortError::Deactivated => {
                Response::error(id, "failed_precondition", "DID is deactivated")
            }
            DidLifecyclePortError::Locked => {
                Response::error(id, "wallet_locked", "wallet is locked")
            }
            DidLifecyclePortError::InvalidOperation => {
                Response::error(id, "invalid_argument", "DID lifecycle operation is invalid")
            }
        },
        DidOperationError::Resolution(error) => match error {
            DidResolutionPortError::Unavailable => Response::error(
                id,
                "capability_unavailable",
                "DID resolution capability is unavailable",
            ),
            DidResolutionPortError::NotFound => {
                Response::error(id, "not_found", "DID was not found")
            }
            DidResolutionPortError::InvalidDid => Response::error(
                id,
                "invalid_argument",
                "DID resolver rejected the identifier",
            ),
            DidResolutionPortError::MethodNotSupported => Response::error(
                id,
                "unsupported_method",
                "DID method is not supported by the resolver",
            ),
            DidResolutionPortError::InvalidResponse => Response::error(
                id,
                "invalid_response",
                "DID resolver returned an invalid response",
            ),
            DidResolutionPortError::Rejected => {
                Response::error(id, "resolution_rejected", "DID resolution was rejected")
            }
        },
        DidOperationError::Persistence(error) => match error {
            DidRecordRepositoryError::NotFound => {
                Response::error(id, "not_found", "DID record was not found")
            }
            DidRecordRepositoryError::CapacityExceeded => {
                Response::error(id, "resource_exhausted", "DID record capacity was exceeded")
            }
            DidRecordRepositoryError::Integrity => Response::error(
                id,
                "integrity_error",
                "DID record storage failed integrity validation",
            ),
            DidRecordRepositoryError::Unavailable => Response::error(
                id,
                "storage_unavailable",
                "DID record storage is unavailable",
            ),
        },
    }
}

pub(super) fn transaction_error(id: Option<String>, error: WalletTransactionError) -> Response {
    match error {
        WalletTransactionError::InvalidProfileIdentifier(_)
        | WalletTransactionError::InvalidDraftIdentifier(_)
        | WalletTransactionError::InvalidAuthorizationChallenge(_)
        | WalletTransactionError::InvalidRecipient(_)
        | WalletTransactionError::InvalidAmount
        | WalletTransactionError::InvalidTokenType
        | WalletTransactionError::ZeroAmount => Response::error(
            id,
            "invalid_argument",
            "transfer recipient, amount, draft, or authorization challenge is invalid",
        ),
        WalletTransactionError::ConfirmationRequired => Response::error(
            id,
            "confirmation_required",
            "explicit human-readable confirmation is required",
        ),
        WalletTransactionError::InvalidConfirmation => Response::error(
            id,
            "invalid_argument",
            "confirmation title and summary must be non-empty and bounded",
        ),
        WalletTransactionError::Clock(_) => Response::error(
            id,
            "platform_unavailable",
            "required platform clock is unavailable",
        ),
        WalletTransactionError::Operation(error) => transaction_port_error(id, error),
    }
}

pub(super) fn dust_registration_error(
    id: Option<String>,
    error: WalletDustRegistrationError,
) -> Response {
    match error {
        WalletDustRegistrationError::InvalidProfileIdentifier(_)
        | WalletDustRegistrationError::InvalidDraftIdentifier(_)
        | WalletDustRegistrationError::InvalidAuthorizationChallenge(_) => Response::error(
            id,
            "invalid_argument",
            "DUST registration draft or authorization challenge is invalid",
        ),
        WalletDustRegistrationError::ConfirmationRequired => Response::error(
            id,
            "confirmation_required",
            "explicit human-readable confirmation is required",
        ),
        WalletDustRegistrationError::InvalidConfirmation => Response::error(
            id,
            "invalid_argument",
            "confirmation title and summary must be non-empty and bounded",
        ),
        WalletDustRegistrationError::Clock(_) => Response::error(
            id,
            "platform_unavailable",
            "required platform clock is unavailable",
        ),
        WalletDustRegistrationError::Operation(error) => dust_registration_port_error(id, error),
    }
}

pub(super) fn dust_registration_port_error(
    id: Option<String>,
    error: WalletDustRegistrationPortError,
) -> Response {
    let code = match error {
        WalletDustRegistrationPortError::Unavailable => "capability_unavailable",
        WalletDustRegistrationPortError::ProtectionNotInitialized
        | WalletDustRegistrationPortError::AccountNotDerived
        | WalletDustRegistrationPortError::AccountNotSynchronized
        | WalletDustRegistrationPortError::NoEligibleNight
        | WalletDustRegistrationPortError::InsufficientRegistrationAllowance => {
            "failed_precondition"
        }
        WalletDustRegistrationPortError::ProtectionLocked => "wallet_locked",
        WalletDustRegistrationPortError::RegistrationAlreadyCurrent => "already_registered",
        WalletDustRegistrationPortError::DraftNotFound => "not_found",
        WalletDustRegistrationPortError::DraftExpired
        | WalletDustRegistrationPortError::DraftConflict
        | WalletDustRegistrationPortError::SubmissionInProgress
        | WalletDustRegistrationPortError::SubmissionNotInProgress
        | WalletDustRegistrationPortError::SubmissionCancellationUnsafe => "conflict",
        WalletDustRegistrationPortError::AuthorizationChallengeMismatch => "invalid_argument",
        WalletDustRegistrationPortError::InvalidChainState => "invalid_chain_state",
        WalletDustRegistrationPortError::ProvingFailed => "proving_failed",
        WalletDustRegistrationPortError::SubmissionRejected => "submission_rejected",
        WalletDustRegistrationPortError::SubmissionOutcomeUnknown => "submission_outcome_unknown",
        WalletDustRegistrationPortError::Timeout => "timeout",
        WalletDustRegistrationPortError::InvalidData => "internal_error",
    };
    Response::error(id, code, error.to_string())
}

pub(super) fn transaction_port_error(
    id: Option<String>,
    error: WalletTransactionPortError,
) -> Response {
    match error {
        WalletTransactionPortError::Unavailable => Response::error(
            id,
            "capability_unavailable",
            "wallet transaction capability is unavailable",
        ),
        WalletTransactionPortError::ProtectionNotInitialized => Response::error(
            id,
            "failed_precondition",
            "wallet protection is not initialized",
        ),
        WalletTransactionPortError::ProtectionLocked => {
            Response::error(id, "wallet_locked", "wallet is locked")
        }
        WalletTransactionPortError::AccountNotDerived => Response::error(
            id,
            "failed_precondition",
            "a protected wallet account must be derived first",
        ),
        WalletTransactionPortError::AccountNotSynchronized => Response::error(
            id,
            "failed_precondition",
            "wallet account must be synchronized first",
        ),
        WalletTransactionPortError::ShieldedStateNotCurrent => Response::error(
            id,
            "failed_precondition",
            "shielded wallet state must finish a fresh synchronization first",
        ),
        WalletTransactionPortError::UnsupportedNetwork => Response::error(
            id,
            "unsupported_network",
            "selected wallet network is not supported",
        ),
        WalletTransactionPortError::InvalidRecipient => {
            Response::error(id, "invalid_argument", "recipient address is invalid")
        }
        WalletTransactionPortError::RecipientNetworkMismatch => Response::error(
            id,
            "invalid_argument",
            "recipient address belongs to another network",
        ),
        WalletTransactionPortError::InsufficientFunds => Response::error(
            id,
            "insufficient_funds",
            "wallet has insufficient funds for the requested transfer",
        ),
        WalletTransactionPortError::DraftNotFound => {
            Response::error(id, "not_found", "transaction draft was not found")
        }
        WalletTransactionPortError::DraftExpired => {
            Response::error(id, "failed_precondition", "transaction draft has expired")
        }
        WalletTransactionPortError::DraftConflict => Response::error(
            id,
            "conflict",
            "transaction draft conflicts with current wallet state",
        ),
        WalletTransactionPortError::SubmissionInProgress => Response::error(
            id,
            "conflict",
            "transaction submission is already in progress",
        ),
        WalletTransactionPortError::SubmissionNotInProgress => Response::error(
            id,
            "failed_precondition",
            "transaction submission is not in progress",
        ),
        WalletTransactionPortError::SubmissionCancelled => Response::error(
            id,
            "submission_cancelled",
            "transaction submission was cancelled before broadcast",
        ),
        WalletTransactionPortError::SubmissionCancellationUnsafe => Response::error(
            id,
            "failed_precondition",
            "transaction submission can no longer be cancelled safely",
        ),
        WalletTransactionPortError::AuthorizationChallengeMismatch => Response::error(
            id,
            "authorization_mismatch",
            "authorization does not match the prepared transfer preview",
        ),
        WalletTransactionPortError::InsufficientDust => Response::error(
            id,
            "insufficient_funds",
            "wallet has insufficient DUST for the transaction fee",
        ),
        WalletTransactionPortError::InvalidChainState => Response::error(
            id,
            "chain_state_unavailable",
            "current Midnight chain state could not be used safely",
        ),
        WalletTransactionPortError::ProvingFailed => {
            Response::error(id, "proving_failed", "transaction proof generation failed")
        }
        WalletTransactionPortError::SubmissionRejected => Response::error(
            id,
            "submission_rejected",
            "Midnight rejected the transaction submission",
        ),
        WalletTransactionPortError::SubmissionOutcomeUnknown => Response::error(
            id,
            "submission_unknown",
            "Midnight transaction submission is still awaiting reconciliation",
        ),
        WalletTransactionPortError::Timeout => {
            Response::error(id, "timeout", "transaction operation timed out")
        }
        WalletTransactionPortError::InvalidData => Response::error(
            id,
            "internal_error",
            "transaction material could not be constructed safely",
        ),
    }
}

pub(super) fn account_error(id: Option<String>, error: WalletAccountError) -> Response {
    match error {
        WalletAccountError::InvalidProfileIdentifier(_)
        | WalletAccountError::InvalidNetworkIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "profile or network identifier is invalid",
        ),
        WalletAccountError::AccountIndexOutOfBounds
        | WalletAccountError::AddressIndexOutOfBounds => Response::error(
            id,
            "invalid_argument",
            "accountIndex and addressIndex must be less than 2^31",
        ),
        WalletAccountError::Port(WalletAccountPortError::NotFound) => {
            Response::error(id, "not_found", "wallet account was not found")
        }
        WalletAccountError::Port(WalletAccountPortError::UnsupportedNetwork) => Response::error(
            id,
            "unsupported_network",
            "selected wallet network is not supported",
        ),
        WalletAccountError::Port(WalletAccountPortError::ProtectionNotInitialized) => {
            Response::error(
                id,
                "failed_precondition",
                "wallet protection is not initialized",
            )
        }
        WalletAccountError::Port(WalletAccountPortError::ProtectionLocked) => {
            Response::error(id, "wallet_locked", "wallet is locked")
        }
        WalletAccountError::Port(WalletAccountPortError::Unavailable) => Response::error(
            id,
            "capability_unavailable",
            "wallet account capability is unavailable",
        ),
        WalletAccountError::Port(WalletAccountPortError::InvalidData) => Response::error(
            id,
            "internal_error",
            "wallet account state could not be decoded safely",
        ),
    }
}

pub(super) fn dust_sync_error(id: Option<String>, error: WalletDustSyncError) -> Response {
    match error {
        WalletDustSyncError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        WalletDustSyncError::Port(WalletDustSyncPortError::Conflict) => Response::error(
            id,
            "conflict",
            "DUST synchronization is already running or cannot be cancelled",
        ),
        WalletDustSyncError::Port(WalletDustSyncPortError::UnsupportedNetwork) => Response::error(
            id,
            "unsupported_network",
            "selected wallet network does not support DUST synchronization",
        ),
        WalletDustSyncError::Port(WalletDustSyncPortError::ProtectionNotInitialized) => {
            Response::error(
                id,
                "failed_precondition",
                "wallet protection is not initialized",
            )
        }
        WalletDustSyncError::Port(WalletDustSyncPortError::ProtectionLocked) => {
            Response::error(id, "wallet_locked", "wallet is locked")
        }
        WalletDustSyncError::Port(WalletDustSyncPortError::Unavailable) => Response::error(
            id,
            "capability_unavailable",
            "DUST synchronization is unavailable",
        ),
        WalletDustSyncError::Port(WalletDustSyncPortError::InvalidData) => Response::error(
            id,
            "chain_state_unavailable",
            "DUST synchronization state could not be used safely",
        ),
    }
}

pub(super) fn shielded_sync_error(id: Option<String>, error: WalletShieldedSyncError) -> Response {
    match error {
        WalletShieldedSyncError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::Conflict) => Response::error(
            id,
            "conflict",
            "shielded synchronization is already running or cannot be cancelled",
        ),
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::UnsupportedNetwork) => {
            Response::error(
                id,
                "unsupported_network",
                "selected wallet network does not support shielded synchronization",
            )
        }
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::ProtectionNotInitialized) => {
            Response::error(
                id,
                "failed_precondition",
                "wallet protection is not initialized",
            )
        }
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::ProtectionLocked) => {
            Response::error(id, "wallet_locked", "wallet is locked")
        }
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::Unavailable) => Response::error(
            id,
            "capability_unavailable",
            "shielded synchronization is unavailable",
        ),
        WalletShieldedSyncError::Port(WalletShieldedSyncPortError::InvalidData) => Response::error(
            id,
            "chain_state_unavailable",
            "shielded synchronization state could not be used safely",
        ),
    }
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

pub(super) fn key_algorithm(value: &str) -> Option<WalletKeyAlgorithm> {
    match value {
        "ed25519" => Some(WalletKeyAlgorithm::Ed25519),
        "p256" => Some(WalletKeyAlgorithm::P256),
        "secp256k1-schnorr" => Some(WalletKeyAlgorithm::Secp256k1Schnorr),
        "jubjub" => Some(WalletKeyAlgorithm::Jubjub),
        _ => None,
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

pub(super) fn key_purpose(value: &str) -> Option<WalletKeyPurpose> {
    match value {
        "transaction" => Some(WalletKeyPurpose::Transaction),
        "authentication" => Some(WalletKeyPurpose::Authentication),
        "assertion" => Some(WalletKeyPurpose::Assertion),
        "key_agreement" => Some(WalletKeyPurpose::KeyAgreement),
        "recovery" => Some(WalletKeyPurpose::Recovery),
        _ => None,
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

pub(super) fn decode_hex(value: &str) -> Option<Vec<u8>> {
    decode_hex_bounded(value, oxid_wallet_application::MAX_SIGNING_PAYLOAD_BYTES)
}

pub(super) fn decode_hex_bounded(value: &str, maximum_bytes: usize) -> Option<Vec<u8>> {
    if value.is_empty()
        || value.len() > maximum_bytes.checked_mul(2)?
        || !value.len().is_multiple_of(2)
        || !value.is_ascii()
    {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub(super) fn invalid_empty_params(id: Option<String>, method: &'static str) -> Dispatch {
    let message = match method {
        "wallet.security.status" => "wallet.security.status does not accept parameters",
        "wallet.security.initialize" => "wallet.security.initialize does not accept parameters",
        "wallet.security.unlock" => "wallet.security.unlock does not accept parameters",
        "wallet.security.lock" => "wallet.security.lock does not accept parameters",
        "wallet.key.list" => "wallet.key.list does not accept parameters",
        "wallet.network.list" => "wallet.network.list does not accept parameters",
        "wallet.account.get" => "wallet.account.get does not accept parameters",
        "wallet.address.list" => "wallet.address.list does not accept parameters",
        "wallet.address.unshielded" => "wallet.address.unshielded does not accept parameters",
        "wallet.address.shielded" => "wallet.address.shielded does not accept parameters",
        "wallet.balance.snapshot" => "wallet.balance.snapshot does not accept parameters",
        "wallet.transaction.history" => "wallet.transaction.history does not accept parameters",
        "wallet.transaction.submission_history" => {
            "wallet.transaction.submission_history does not accept parameters"
        }
        "wallet.connect" => "wallet.connect does not accept parameters",
        "wallet.sync.force" => "wallet.sync.force does not accept parameters",
        "wallet.dust.sync.status" => "wallet.dust.sync.status does not accept parameters",
        "wallet.dust.sync.start" => "wallet.dust.sync.start does not accept parameters",
        "wallet.dust.sync.cancel" => "wallet.dust.sync.cancel does not accept parameters",
        "wallet.shielded.sync.status" => "wallet.shielded.sync.status does not accept parameters",
        "wallet.shielded.sync.start" => "wallet.shielded.sync.start does not accept parameters",
        "wallet.shielded.sync.cancel" => "wallet.shielded.sync.cancel does not accept parameters",
        _ => "method does not accept parameters",
    };
    Dispatch::continue_with(Response::error(id, "invalid_params", message))
}

pub(super) fn security_error(id: Option<String>, error: WalletSecurityError) -> Response {
    match error {
        WalletSecurityError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        WalletSecurityError::Operation(error) => security_port_error(id, error),
    }
}

pub(super) fn key_error(id: Option<String>, error: WalletKeyError) -> Response {
    match error {
        WalletKeyError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        WalletKeyError::InvalidKeyReference(_) => {
            Response::error(id, "invalid_argument", "keyRef is invalid")
        }
        WalletKeyError::InvalidLabel(_) => Response::error(
            id,
            "invalid_argument",
            "key label must be non-empty, bounded, and contain no control characters",
        ),
        WalletKeyError::Operation(error) => security_port_error(id, error),
    }
}

pub(super) fn sensitive_error(
    id: Option<String>,
    error: SensitiveWalletOperationError,
) -> Response {
    match error {
        SensitiveWalletOperationError::InvalidProfileIdentifier(_) => Response::error(
            id,
            "invalid_argument",
            "active profile identifier is invalid",
        ),
        SensitiveWalletOperationError::InvalidKeyReference(_) => {
            Response::error(id, "invalid_argument", "keyRef is invalid")
        }
        SensitiveWalletOperationError::EmptyPayload => {
            Response::error(id, "invalid_argument", "signing payload must not be empty")
        }
        SensitiveWalletOperationError::PayloadTooLarge => Response::error(
            id,
            "invalid_argument",
            "signing payload exceeds the application limit",
        ),
        SensitiveWalletOperationError::ConfirmationRequired => Response::error(
            id,
            "confirmation_required",
            "explicit human-readable confirmation is required",
        ),
        SensitiveWalletOperationError::InvalidConfirmation => Response::error(
            id,
            "invalid_argument",
            "confirmation title and summary must be non-empty and bounded",
        ),
        SensitiveWalletOperationError::Operation(error) => security_port_error(id, error),
    }
}

pub(super) fn security_port_error(id: Option<String>, error: WalletSecurityPortError) -> Response {
    match error {
        WalletSecurityPortError::Unavailable => Response::error(
            id,
            "capability_unavailable",
            "wallet protection is unavailable",
        ),
        WalletSecurityPortError::NotInitialized => Response::error(
            id,
            "failed_precondition",
            "wallet protection is not initialized",
        ),
        WalletSecurityPortError::AlreadyInitialized => {
            Response::error(id, "conflict", "wallet protection is already initialized")
        }
        WalletSecurityPortError::Locked => Response::error(id, "wallet_locked", "wallet is locked"),
        WalletSecurityPortError::NotFound => {
            Response::error(id, "not_found", "protected key was not found")
        }
        WalletSecurityPortError::Conflict => {
            Response::error(id, "conflict", "protected key metadata conflicts")
        }
        WalletSecurityPortError::UnsupportedAlgorithm => Response::error(
            id,
            "unsupported_algorithm",
            "key algorithm is not supported by this adapter",
        ),
        WalletSecurityPortError::AuthorizationDenied => Response::error(
            id,
            "authorization_denied",
            "wallet authorization was denied",
        ),
        WalletSecurityPortError::InvalidOperation => Response::error(
            id,
            "internal_error",
            "protected operation could not be completed",
        ),
    }
}

pub(super) fn decimal_u128(value: &str) -> Option<u128> {
    if value.is_empty()
        || value.len() > 39
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }
    value.parse().ok()
}

pub(super) fn policy_value(value: Option<String>) -> Result<Option<[u8; 32]>, ()> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty()
        || value.trim() != value
        || value.len() > 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
    {
        return Err(());
    }
    let mut padded = [0_u8; 32];
    padded[..value.len()].copy_from_slice(value.as_bytes());
    Ok(Some(padded))
}

pub(super) fn vault_contract_call_action(
    id: Option<String>,
    action: VaultContractCallActionParams,
) -> Result<PreparePassportVaultCallAction, Box<Dispatch>> {
    Ok(match action {
        VaultContractCallActionParams::Create {
            minimum_age_years,
            required_issuing_state,
            required_document_number,
            maximum_claim_amount,
            initial_amount,
        } => {
            if decimal_u128(&maximum_claim_amount).is_none() {
                return Err(Box::new(invalid_vault_amount(id, "maximumClaimAmount")));
            }
            if decimal_u128(&initial_amount).is_none() {
                return Err(Box::new(invalid_vault_amount(id, "initialAmount")));
            }
            let required_issuing_state = policy_value(required_issuing_state).map_err(|()| {
                Box::new(invalid_vault_policy_value(
                    id.clone(),
                    "requiredIssuingState",
                ))
            })?;
            let required_document_number = policy_value(required_document_number)
                .map_err(|()| Box::new(invalid_vault_policy_value(id, "requiredDocumentNumber")))?;
            PreparePassportVaultCallAction::CreateLock {
                minimum_age_years,
                required_issuing_state,
                required_document_number,
                maximum_claim_amount,
                initial_amount,
            }
        }
        VaultContractCallActionParams::Deposit { lock_id, amount } => {
            if decimal_u128(&amount).is_none() {
                return Err(Box::new(invalid_vault_amount(id, "amount")));
            }
            PreparePassportVaultCallAction::DepositToLock { lock_id, amount }
        }
        VaultContractCallActionParams::Claim {
            lock_id,
            credential_id,
            amount,
        } => {
            if decimal_u128(&amount).is_none() {
                return Err(Box::new(invalid_vault_amount(id, "amount")));
            }
            PreparePassportVaultCallAction::ClaimFromLock {
                lock_id,
                credential_id,
                amount,
            }
        }
        VaultContractCallActionParams::Withdraw { lock_id, amount } => {
            if decimal_u128(&amount).is_none() {
                return Err(Box::new(invalid_vault_amount(id, "amount")));
            }
            PreparePassportVaultCallAction::WithdrawFromLock { lock_id, amount }
        }
    })
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

pub(super) fn passport_vault_contract_state_read_error(
    id: Option<String>,
    error: PassportVaultContractStateReadError,
) -> Response {
    match error {
        PassportVaultContractStateReadError::Decode(error) => {
            passport_vault_contract_state_error(id, error)
        }
        PassportVaultContractStateReadError::Source(error) => {
            let code = match error {
                PassportVaultContractStateSourceError::InvalidAddress => "invalid_argument",
                PassportVaultContractStateSourceError::NotFound => "not_found",
                PassportVaultContractStateSourceError::Unavailable
                | PassportVaultContractStateSourceError::InvalidConfiguration => {
                    "capability_unavailable"
                }
                PassportVaultContractStateSourceError::InvalidResponse
                | PassportVaultContractStateSourceError::CapacityExceeded
                | PassportVaultContractStateSourceError::FinalityMismatch => "invalid_chain_state",
            };
            Response::error(id, code, error.to_string())
        }
    }
}

pub(super) fn passport_vault_contract_state_error(
    id: Option<String>,
    error: PassportVaultContractStateError,
) -> Response {
    let code = match error {
        PassportVaultContractStateError::Unavailable => "capability_unavailable",
        PassportVaultContractStateError::InvalidEncoding
        | PassportVaultContractStateError::LayoutMismatch
        | PassportVaultContractStateError::UnsupportedVersion
        | PassportVaultContractStateError::CapacityExceeded
        | PassportVaultContractStateError::Integrity => "invalid_contract_state",
    };
    Response::error(id, code, error.to_string())
}

pub(super) fn invalid_vault_amount(id: Option<String>, field: &str) -> Dispatch {
    Dispatch::continue_with(Response::error(
        id,
        "invalid_params",
        format!("{field} must be a canonical unsigned decimal string"),
    ))
}

pub(super) fn invalid_vault_policy_value(id: Option<String>, field: &str) -> Dispatch {
    Dispatch::continue_with(Response::error(
        id,
        "invalid_params",
        format!("{field} must be 1-32 printable ASCII bytes when present"),
    ))
}

pub(super) fn passport_vault_error(
    id: Option<String>,
    error: PassportVaultOperationError,
) -> Response {
    let (code, message) = match &error {
        PassportVaultOperationError::Repository(_) | PassportVaultOperationError::Platform(_) => {
            ("capability_unavailable", error.to_string())
        }
        PassportVaultOperationError::Credential(
            oxid_passport_vault_application::PassportVaultCredentialError::Unavailable,
        ) => ("capability_unavailable", error.to_string()),
        PassportVaultOperationError::Credential(
            oxid_passport_vault_application::PassportVaultCredentialError::NotFound,
        )
        | PassportVaultOperationError::Domain(PassportVaultError::LockNotFound) => {
            ("not_found", error.to_string())
        }
        PassportVaultOperationError::ConfirmationRequired
        | PassportVaultOperationError::InvalidConfirmation => {
            ("confirmation_required", error.to_string())
        }
        PassportVaultOperationError::Domain(PassportVaultError::CredentialAlreadyClaimed) => {
            ("conflict", error.to_string())
        }
        PassportVaultOperationError::Credential(_)
        | PassportVaultOperationError::Domain(_)
        | PassportVaultOperationError::PolicyChanged => ("failed_precondition", error.to_string()),
    };
    Response::error(id, code, message)
}

pub(super) fn passport_vault_call_error(
    id: Option<String>,
    error: PassportVaultCallError,
) -> Response {
    match error {
        PassportVaultCallError::InvalidIdentifier(_)
        | PassportVaultCallError::InvalidAddress
        | PassportVaultCallError::InvalidAmount
        | PassportVaultCallError::ZeroAmount
        | PassportVaultCallError::InvalidPolicy => {
            Response::error(id, "invalid_argument", error.to_string())
        }
        PassportVaultCallError::ConfirmationRequired
        | PassportVaultCallError::InvalidConfirmation => {
            Response::error(id, "confirmation_required", error.to_string())
        }
        PassportVaultCallError::UnauthenticatedState => {
            Response::error(id, "failed_precondition", error.to_string())
        }
        PassportVaultCallError::Clock(_) | PassportVaultCallError::Random(_) => {
            Response::error(id, "capability_unavailable", error.to_string())
        }
        PassportVaultCallError::State(state) => passport_vault_contract_state_read_error(
            id,
            PassportVaultContractStateReadError::Source(state),
        ),
        PassportVaultCallError::Operation(operation) => {
            passport_vault_call_port_error(id, operation)
        }
    }
}

pub(super) fn passport_vault_call_port_error(
    id: Option<String>,
    error: PassportVaultCallPortError,
) -> Response {
    let code = match error {
        PassportVaultCallPortError::Unavailable => "capability_unavailable",
        PassportVaultCallPortError::ProtectionNotInitialized
        | PassportVaultCallPortError::AccountNotDerived
        | PassportVaultCallPortError::AccountNotSynchronized
        | PassportVaultCallPortError::InsufficientFunds
        | PassportVaultCallPortError::InsufficientDust => "failed_precondition",
        PassportVaultCallPortError::ProtectionLocked => "wallet_locked",
        PassportVaultCallPortError::UnsupportedNetwork
        | PassportVaultCallPortError::AuthorizationChallengeMismatch => "invalid_argument",
        PassportVaultCallPortError::DraftNotFound => "not_found",
        PassportVaultCallPortError::DraftExpired
        | PassportVaultCallPortError::DraftConflict
        | PassportVaultCallPortError::SubmissionInProgress
        | PassportVaultCallPortError::SubmissionNotInProgress
        | PassportVaultCallPortError::SubmissionCancellationUnsafe => "conflict",
        PassportVaultCallPortError::SubmissionCancelled => "cancelled",
        PassportVaultCallPortError::InvalidChainState => "invalid_chain_state",
        PassportVaultCallPortError::ProvingFailed => "proving_failed",
        PassportVaultCallPortError::SubmissionRejected => "submission_rejected",
        PassportVaultCallPortError::SubmissionOutcomeUnknown => "submission_outcome_unknown",
        PassportVaultCallPortError::Timeout => "timeout",
        PassportVaultCallPortError::InvalidData => "internal_error",
    };
    Response::error(id, code, error.to_string())
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
