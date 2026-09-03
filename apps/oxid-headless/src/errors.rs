// SPDX-License-Identifier: Apache-2.0

use oxid_credential_application::{
    CredentialDisclosurePortError, CredentialOperationError, CredentialRepositoryError,
    CredentialVerificationError,
};
use oxid_identity_application::{
    DidLifecyclePortError, DidOperationError, DidPublicationPortError, DidRecordRepositoryError,
    DidResolutionPortError,
};
use oxid_passport_vault_application::{
    PassportVaultCallError, PassportVaultCallPortError, PassportVaultContractStateError,
    PassportVaultContractStateReadError, PassportVaultContractStateSourceError,
    PassportVaultOperationError,
};
use oxid_passport_vault_domain::PassportVaultError;
use oxid_presentation_application::CredentialPresentationError;
use oxid_protocol_application::{
    CredentialIssuanceError, IdentityRequestRoutingError, SelfIssuedAuthenticationError,
};
use oxid_wallet_application::{
    SensitiveWalletOperationError, WalletAccountError, WalletAccountPortError,
    WalletDustRegistrationError, WalletDustRegistrationPortError, WalletDustSyncError,
    WalletDustSyncPortError, WalletKeyError, WalletSecurityError, WalletSecurityPortError,
    WalletShieldedSyncError, WalletShieldedSyncPortError, WalletTransactionError,
    WalletTransactionPortError,
};

use crate::protocol::{Dispatch, Response};

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
        DidOperationError::Publication(error) => {
            let code = match error {
                DidPublicationPortError::Unavailable
                | DidPublicationPortError::InvalidConfiguration
                | DidPublicationPortError::InvalidCapability => "capability_unavailable",
                DidPublicationPortError::Rejected => "publication_rejected",
            };
            Response::error(id, code, error.to_string())
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
