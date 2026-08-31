// SPDX-License-Identifier: Apache-2.0

use oxid_identity_application::{DidKeyAlgorithm, DidOperationConfirmation, DidUpdate};
use oxid_identity_domain::VerificationRelationship;
use oxid_passport_vault_application::PreparePassportVaultCallAction;
use oxid_wallet_application::SensitiveOperationConfirmation;
use oxid_wallet_domain::{WalletKeyAlgorithm, WalletKeyPurpose};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    errors::{invalid_vault_amount, invalid_vault_policy_value},
    protocol::{Dispatch, Response},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClearDiagnosticsParams {
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SelectNetworkParams {
    pub(super) network_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DeriveAccountParams {
    #[serde(default)]
    pub(super) account_index: u32,
    #[serde(default)]
    pub(super) address_index: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CreateVaultLockParams {
    pub(super) minimum_age_years: u8,
    #[serde(default)]
    pub(super) required_issuing_state: Option<String>,
    #[serde(default)]
    pub(super) required_document_number: Option<String>,
    pub(super) maximum_claim_amount: String,
    pub(super) initial_amount: String,
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DecodeVaultContractStateParams {
    pub(super) contract_state_hex: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReadVaultContractStateParams {
    pub(super) contract_address_hex: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PrepareVaultContractCallParams {
    pub(super) contract_address_hex: String,
    pub(super) action: VaultContractCallActionParams,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum VaultContractCallActionParams {
    #[serde(rename = "create_lock")]
    Create {
        minimum_age_years: u8,
        #[serde(default)]
        required_issuing_state: Option<String>,
        #[serde(default)]
        required_document_number: Option<String>,
        maximum_claim_amount: String,
        initial_amount: String,
    },
    #[serde(rename = "deposit_to_lock")]
    Deposit { lock_id: u64, amount: String },
    #[serde(rename = "claim_from_lock")]
    Claim {
        lock_id: u64,
        credential_id: String,
        amount: String,
    },
    #[serde(rename = "withdraw_from_lock")]
    Withdraw { lock_id: u64, amount: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AuthorizeVaultContractCallParams {
    pub(super) draft_id: String,
    pub(super) authorization_challenge: String,
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmitVaultContractCallParams {
    pub(super) draft_id: String,
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct VaultContractCallDraftParams {
    pub(super) draft_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct VaultAmountParams {
    pub(super) lock_id: u64,
    pub(super) amount: String,
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ClaimVaultLockParams {
    pub(super) lock_id: u64,
    pub(super) credential_id: String,
    pub(super) amount: String,
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PrepareTransferParams {
    pub(super) recipient_address: String,
    pub(super) amount_atomic_units: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PrepareShieldedTransferParams {
    pub(super) recipient_address: String,
    pub(super) token_type: String,
    pub(super) amount_atomic_units: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AuthorizeTransferParams {
    pub(super) draft_id: String,
    pub(super) authorization_challenge: String,
    pub(super) confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AuthorizeDustRegistrationParams {
    pub(super) draft_id: String,
    pub(super) authorization_challenge: String,
    pub(super) confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TransactionDraftParams {
    pub(super) draft_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmitTransferParams {
    pub(super) draft_id: String,
    pub(super) confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct GenerateKeyParams {
    pub(super) label: String,
    pub(super) algorithm: String,
    pub(super) purpose: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ConfirmationParams {
    pub(super) title: String,
    pub(super) summary: String,
    pub(super) confirmed: bool,
}

impl From<ConfirmationParams> for SensitiveOperationConfirmation {
    fn from(value: ConfirmationParams) -> Self {
        Self {
            title: value.title,
            summary: value.summary,
            confirmed: value.confirmed,
        }
    }
}

impl From<ConfirmationParams> for DidOperationConfirmation {
    fn from(value: ConfirmationParams) -> Self {
        Self {
            title: value.title,
            summary: value.summary,
            confirmed: value.confirmed,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SignParams {
    #[serde(rename = "keyRef")]
    pub(super) key_reference: String,
    pub(super) payload_hex: String,
    pub(super) confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DeleteKeyParams {
    #[serde(rename = "keyRef")]
    pub(super) key_reference: String,
    pub(super) confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DidParams {
    pub(super) did: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CredentialParams {
    pub(super) credential_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DeleteCredentialParams {
    pub(super) credential_id: String,
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DisclosurePreviewParams {
    pub(super) credential_id: String,
    pub(super) reveal_claim_paths: Vec<String>,
    pub(super) predicates: Vec<DisclosurePredicateParams>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DisclosurePredicateParams {
    pub(super) claim_path: String,
    pub(super) kind: String,
    pub(super) threshold: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrepareCredentialIssuanceParams {
    pub(super) offer: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RouteIdentityRequestParams {
    pub(super) request_uri: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CredentialIssuanceParams {
    pub(super) issuance_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AcceptCredentialIssuanceParams {
    pub(super) issuance_id: String,
    pub(super) holder_did: String,
    pub(super) method_id: String,
    pub(super) holder_binding_method_id: String,
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrepareCredentialPresentationParams {
    pub(super) request: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CredentialPresentationParams {
    pub(super) presentation_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AcceptCredentialPresentationParams {
    pub(super) presentation_id: String,
    pub(super) credential_id: String,
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrepareSelfIssuedAuthenticationParams {
    pub(super) request: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SelfIssuedAuthenticationParams {
    pub(super) authentication_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AcceptSelfIssuedAuthenticationParams {
    pub(super) authentication_id: String,
    pub(super) holder_did: String,
    pub(super) method_id: String,
    pub(super) confirmed: bool,
    pub(super) intent: String,
}

pub(super) fn undeployed_network() -> String {
    "undeployed".to_owned()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CreateDidParams {
    #[serde(default = "undeployed_network")]
    pub(super) network: String,
}

#[derive(Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum DidUpdateParams {
    AddAlsoKnownAs {
        did: String,
        value: String,
        confirmation: ConfirmationParams,
    },
    RemoveAlsoKnownAs {
        did: String,
        value: String,
        confirmation: ConfirmationParams,
    },
    AddVerificationMethod {
        did: String,
        fragment: String,
        algorithm: String,
        confirmation: ConfirmationParams,
    },
    UpdateVerificationMethod {
        did: String,
        method_id: String,
        algorithm: String,
        confirmation: ConfirmationParams,
    },
    RemoveVerificationMethod {
        did: String,
        method_id: String,
        confirmation: ConfirmationParams,
    },
    AddVerificationRelationship {
        did: String,
        relationship: String,
        method_id: String,
        confirmation: ConfirmationParams,
    },
    RemoveVerificationRelationship {
        did: String,
        relationship: String,
        method_id: String,
        confirmation: ConfirmationParams,
    },
    AddService {
        did: String,
        id: String,
        service_type: String,
        endpoint: String,
        confirmation: ConfirmationParams,
    },
    UpdateService {
        did: String,
        id: String,
        service_type: String,
        endpoint: String,
        confirmation: ConfirmationParams,
    },
    RemoveService {
        did: String,
        id: String,
        confirmation: ConfirmationParams,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SignDidParams {
    pub(super) did: String,
    pub(super) method_id: String,
    pub(super) payload_hex: String,
    pub(super) confirmation: ConfirmationParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DeactivateDidParams {
    pub(super) did: String,
    pub(super) confirmation: ConfirmationParams,
}

pub(super) fn did_update(
    params: DidUpdateParams,
) -> Option<(String, DidUpdate, DidOperationConfirmation)> {
    let value = match params {
        DidUpdateParams::AddAlsoKnownAs {
            did,
            value,
            confirmation,
        } => (
            did,
            DidUpdate::AddAlsoKnownAs { value },
            confirmation.into(),
        ),
        DidUpdateParams::RemoveAlsoKnownAs {
            did,
            value,
            confirmation,
        } => (
            did,
            DidUpdate::RemoveAlsoKnownAs { value },
            confirmation.into(),
        ),
        DidUpdateParams::AddVerificationMethod {
            did,
            fragment,
            algorithm,
            confirmation,
        } => (
            did,
            DidUpdate::AddVerificationMethod {
                fragment,
                algorithm: did_key_algorithm(&algorithm)?,
            },
            confirmation.into(),
        ),
        DidUpdateParams::UpdateVerificationMethod {
            did,
            method_id,
            algorithm,
            confirmation,
        } => (
            did,
            DidUpdate::UpdateVerificationMethod {
                method_id,
                algorithm: did_key_algorithm(&algorithm)?,
            },
            confirmation.into(),
        ),
        DidUpdateParams::RemoveVerificationMethod {
            did,
            method_id,
            confirmation,
        } => (
            did,
            DidUpdate::RemoveVerificationMethod { method_id },
            confirmation.into(),
        ),
        DidUpdateParams::AddVerificationRelationship {
            did,
            relationship,
            method_id,
            confirmation,
        } => (
            did,
            DidUpdate::AddVerificationRelationship {
                relationship: VerificationRelationship::parse(&relationship)?,
                method_id,
            },
            confirmation.into(),
        ),
        DidUpdateParams::RemoveVerificationRelationship {
            did,
            relationship,
            method_id,
            confirmation,
        } => (
            did,
            DidUpdate::RemoveVerificationRelationship {
                relationship: VerificationRelationship::parse(&relationship)?,
                method_id,
            },
            confirmation.into(),
        ),
        DidUpdateParams::AddService {
            did,
            id,
            service_type,
            endpoint,
            confirmation,
        } => (
            did,
            DidUpdate::AddService {
                id,
                service_type,
                endpoint,
            },
            confirmation.into(),
        ),
        DidUpdateParams::UpdateService {
            did,
            id,
            service_type,
            endpoint,
            confirmation,
        } => (
            did,
            DidUpdate::UpdateService {
                id,
                service_type,
                endpoint,
            },
            confirmation.into(),
        ),
        DidUpdateParams::RemoveService {
            did,
            id,
            confirmation,
        } => (did, DidUpdate::RemoveService { id }, confirmation.into()),
    };
    Some(value)
}

pub(super) fn did_key_algorithm(value: &str) -> Option<DidKeyAlgorithm> {
    match value {
        "ed25519" => Some(DidKeyAlgorithm::Ed25519),
        "jubjub" => Some(DidKeyAlgorithm::Jubjub),
        "p256" => Some(DidKeyAlgorithm::P256),
        _ => None,
    }
}

pub(super) fn dust_registration_draft_params(
    id: Option<String>,
    params: Value,
    method: &'static str,
) -> Result<TransactionDraftParams, Response> {
    serde_json::from_value(params).map_err(|_| {
        Response::error(
            id,
            "invalid_params",
            format!("{method} requires only a string draftId"),
        )
    })
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
