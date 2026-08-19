// SPDX-License-Identifier: Apache-2.0

//! User-facing labels and exact public-value formatting.
//!
//! Application views deliberately carry stable machine strings. They must
//! cross this module before reaching Dioxus copy: known values receive
//! reviewed language and unknown values fail closed to neutral copy instead
//! of being echoed to the user.

use oxid_protocol_application::IdentityRequestKind;

pub(crate) const NIGHT_DECIMALS: u8 = 6;
pub(crate) const DUST_DECIMALS: u8 = 15;

pub(crate) fn identity_request_kind(value: IdentityRequestKind) -> &'static str {
    match value {
        IdentityRequestKind::CredentialIssuance => "a credential offer",
        IdentityRequestKind::SelfIssuedAuthentication => "a DID login request",
        IdentityRequestKind::CredentialPresentation => "a credential presentation request",
    }
}

pub(crate) fn account_source(value: &str) -> &'static str {
    match value {
        "live" => "Live",
        "cached" => "Saved",
        "simulated" => "Simulated",
        "unavailable" => "Not connected",
        _ => "Source unavailable",
    }
}

pub(crate) fn account_source_note(value: &str) -> &'static str {
    match value {
        "live" => "Live account state reported by the configured Midnight adapter.",
        "cached" => "Showing local state from the most recent successful synchronization.",
        "simulated" => "Development-only public fixture state; no chain was contacted.",
        "unavailable" => "Native custody and a live Midnight account source are not connected yet.",
        _ => "The account source could not be identified safely.",
    }
}

pub(crate) fn sync_state(value: &str) -> &'static str {
    match value {
        "never_synced" => "Not synced",
        "syncing" => "Syncing",
        "synced" => "Synced",
        "cached" => "Saved checkpoint",
        "cancelled" => "Cancelled",
        "stalled" => "Needs attention",
        "unavailable" => "Unavailable",
        _ => "Status unavailable",
    }
}

pub(crate) fn wallet_security_state(value: &str) -> &'static str {
    match value {
        "Uninitialized" => "Protection not set up",
        "Locked" => "Wallet locked",
        "Unlocked" => "Wallet unlocked",
        "Unavailable" => "Protection unavailable",
        _ => "Security status unavailable",
    }
}

pub(crate) fn wallet_protection(value: &str) -> &'static str {
    match value {
        "Development only" => "Standalone custody",
        "Operating system" => "Device protected",
        "Hardware backed" => "Hardware backed",
        "Not connected" => "Custody not connected",
        _ => "Protection class unavailable",
    }
}

pub(crate) const fn backup_capability(supported: bool) -> &'static str {
    if supported {
        "Backup available"
    } else {
        "Backup unavailable"
    }
}

pub(crate) fn sync_failure(value: &str) -> &'static str {
    match value {
        "protection_not_initialized" => "wallet protection is not initialized",
        "protection_locked" => "unlock the wallet to continue",
        "unsupported_network" => "this network is not supported",
        "transport_unavailable" => "the Midnight connection is unavailable",
        "timed_out" => "the connection timed out",
        "invalid_chain_state" => "the received chain state could not be validated",
        "storage_unavailable" => "the saved checkpoint is unavailable",
        _ => "the reason is unavailable",
    }
}

pub(crate) fn submission_state(value: &str) -> &'static str {
    match value {
        "not_started" => "Not started",
        "running" => "Preparing",
        "cancellation_requested" => "Cancelling",
        "broadcasting" => "Broadcast",
        "cancelled" => "Cancelled",
        "included" => "Included",
        "rejected" => "Rejected",
        "expired" => "Expired",
        "outcome_unknown" => "Checking with the network…",
        _ => "Status unavailable",
    }
}

pub(crate) fn submission_heading(value: &str) -> &'static str {
    match value {
        "included" => "Transfer included",
        "broadcasting" => "Transfer broadcast",
        "outcome_unknown" => "Checking transfer outcome",
        "rejected" => "Transfer rejected",
        "expired" => "Transfer expired",
        "cancelled" => "Transfer cancelled",
        "not_started" | "running" | "cancellation_requested" => "Transfer in progress",
        _ => "Transfer status unavailable",
    }
}

pub(crate) fn submission_note(value: &str) -> &'static str {
    match value {
        "included" => {
            "The durable journal confirms this transfer was included in a finalized Midnight block."
        }
        "broadcasting" => {
            "This transaction was recorded before broadcast. Reconcile it before preparing a replacement."
        }
        "outcome_unknown" => {
            "The transaction may have reached Midnight. Oxid will not submit a duplicate while it checks finalized history."
        }
        "rejected" => {
            "Midnight finalized this submission as rejected. Its public record is retained for recovery history."
        }
        "expired" => "The submission was not included before its bounded validity window expired.",
        "cancelled" => "The submission stopped before broadcast and may be prepared again safely.",
        "not_started" | "running" | "cancellation_requested" => {
            "Oxid is still preparing this submission and has not crossed the broadcast boundary."
        }
        _ => "The transfer status could not be identified safely.",
    }
}

pub(crate) fn submission_mode(value: &str) -> &'static str {
    match value {
        "simulated" => "Simulated — runs locally, nothing on Midnight",
        "live" => "Submitted to Midnight",
        _ => "Mode unavailable",
    }
}

pub(crate) fn transfer_privacy(value: &str) -> &'static str {
    match value {
        "unshielded" => "Unshielded",
        "shielded" => "Shielded",
        _ => "Privacy unavailable",
    }
}

pub(crate) fn transfer_privacy_adverb(value: &str) -> &'static str {
    match value {
        "unshielded" => "publicly",
        "shielded" => "privately",
        _ => "with unavailable privacy",
    }
}

pub(crate) fn address_kind(value: &str) -> &'static str {
    match value {
        "unshielded" => "Unshielded",
        "shielded" => "Shielded",
        "dust" => "DUST",
        "reward" => "Reward",
        _ => "Address",
    }
}

pub(crate) fn address_purpose(value: &str) -> &'static str {
    match value {
        "unshielded" => "Send public NIGHT here",
        "shielded" => "Private NIGHT receive",
        "dust" => "Fee-token account",
        "reward" => "Reward address",
        _ => "Public receive address",
    }
}

pub(crate) fn transaction_mark(value: &str) -> &'static str {
    match value {
        "incoming" => "↓",
        "outgoing" => "↑",
        "self_transfer" => "↔",
        "unknown" => "◇",
        _ => "◇",
    }
}

pub(crate) fn transaction_direction(value: &str) -> &'static str {
    match value {
        "incoming" => "Received",
        "outgoing" => "Sent",
        "self_transfer" => "Self transfer",
        "unknown" => "Transaction",
        _ => "Transaction",
    }
}

pub(crate) fn transaction_status(value: &str) -> &'static str {
    match value {
        "pending" => "Pending",
        "confirmed" => "Confirmed",
        "partially_applied" => "Partially applied",
        "failed" => "Failed",
        _ => "Status unavailable",
    }
}

pub(crate) fn did_source(value: &str) -> &'static str {
    match value {
        "standalone" => "Standalone",
        "live" => "Midnight network",
        "stored" => "Saved copy",
        _ => "Source unavailable",
    }
}

pub(crate) fn midnight_network(value: &str) -> &'static str {
    match value {
        "mainnet" => "Mainnet",
        "preprod" => "Pre-production",
        "devnet" => "Development network",
        "undeployed" => "Standalone development",
        _ => "Network unavailable",
    }
}

pub(crate) fn key_curve(value: &str) -> &'static str {
    match value {
        "Ed25519" => "Ed25519",
        "P-256" => "P-256",
        "Jubjub" => "Jubjub",
        "secp256k1" => "secp256k1",
        "BLS12381G1" => "BLS12-381 G1",
        "BLS12381G2" => "BLS12-381 G2",
        _ => "Key type unavailable",
    }
}

pub(crate) fn protocol_state(value: &str) -> &'static str {
    match value {
        "awaiting_consent" => "Waiting for your consent",
        "authenticating" => "Authenticating",
        "issuing" => "Receiving credential",
        "presenting" => "Creating presentation",
        "cancellation_requested" => "Cancelling proof",
        "cancelled" => "Cancelled",
        "timed_out" => "Timed out",
        "succeeded" => "Completed",
        "refused" => "Refused",
        "failed" => "Failed",
        _ => "Status unavailable",
    }
}

pub(crate) fn protocol_failure(value: &str) -> &'static str {
    match value {
        "protocol_unavailable" => "This protocol is unavailable in the current build",
        "invalid_request" | "invalid_identity_request" => "The request is not valid",
        "unsupported_request" | "unsupported_identity_request" => {
            "This request type is not supported"
        }
        "ambiguous_identity_request" => "The request type could not be determined safely",
        "identity_request_routing_unavailable" => "Request routing is unavailable",
        "invalid_verifier" => "The verifier identity is not valid",
        "request_expired" => "The request has expired",
        "no_candidate" => "No matching credential is available",
        "holder_authorization_unavailable" => "Holder authorization is unavailable",
        "holder_not_authorized" => "The selected holder method is not authorized",
        "proof_unavailable" => "This build can't generate proofs yet",
        "proof_busy" => "Another proof is already running",
        "proof_cancelled" => "Proof generation was cancelled",
        "proof_backgrounded" => "Proof generation stopped when the app left the foreground",
        "proof_timed_out" => "Proof generation timed out",
        "invalid_proof" => "The proof could not be validated",
        "verifier_rejected" => "The verifier rejected the presentation",
        "invalid_offer" => "The credential offer is not valid",
        "unsupported_offer" => "This credential offer is not supported",
        "transaction_code_required" => "This offer requires an additional transaction code",
        "invalid_metadata" => "The issuer metadata is not valid",
        "unsupported_credential" => "This credential type is not supported",
        "issuer_rejected" => "The issuer rejected the request",
        "invalid_credential_response" => "The issuer returned an invalid credential",
        "protection_unavailable" => "Wallet protection is unavailable",
        "wallet_locked" => "Unlock the wallet to continue",
        "credential_store_unavailable" => "Credential storage is unavailable",
        "invalid_credential" => "The credential is not valid",
        _ => "The operation could not be completed",
    }
}

pub(crate) fn credential_format(value: &str) -> &'static str {
    match value {
        "midnight_cbor_phase1" | "midnight_cbor_v1" => "Midnight credential",
        "midnight_compact_vc" => "Digital Passport (Midnight format)",
        _ => "Credential format unavailable",
    }
}

pub(crate) fn verification_outcome(value: &str) -> &'static str {
    match value {
        "valid" => "Valid",
        "invalid" => "Invalid",
        "error" => "Could not verify",
        _ => "Verification unavailable",
    }
}

pub(crate) fn verification_stage(value: &str) -> &'static str {
    match value {
        "structural" => "Structure",
        "issuer" => "Issuer",
        "proof" => "Cryptographic proof",
        "temporal" => "Validity period",
        "status" => "Revocation status",
        "schema" => "Credential schema",
        "trust" => "Issuer trust",
        _ => "Verification check",
    }
}

pub(crate) fn verification_stage_status(value: &str) -> &'static str {
    match value {
        "passed" => "Passed",
        "failed" => "Failed",
        "not_checked" => "Not checked",
        _ => "Status unavailable",
    }
}

pub(crate) fn verification_policy_status(value: &str) -> &'static str {
    match value {
        "passed" => "passed",
        "failed" => "failed",
        "not_checked" => "not checked",
        _ => "status unavailable",
    }
}

pub(crate) fn verification_reason(value: &str) -> &'static str {
    match value {
        "claim_root_mismatch" => "The disclosed claims do not match the signed credential",
        "credential_expired" => "The credential has expired",
        "detached_proof_malformed" => "The attached proof is malformed",
        "detached_proof_missing" => "The required proof is missing",
        "expiration_precedes_issuance" => "The credential validity dates are inconsistent",
        "holder_binding_missing" => "The credential is not bound to a holder",
        "invalid_issuance_proof" => "The issuer proof is not valid",
        "invalid_issuer_did" => "The issuer DID is not valid",
        "invalid_signature" => "The credential signature is not valid",
        "issuance_in_future" => "The credential issuance time is in the future",
        "issuer_key_mismatch" => "The issuer key does not match the credential",
        "issuer_method_mismatch" => "The issuer method does not match the credential",
        "issuer_method_unsupported" => "The issuer method is not supported",
        "issuer_not_found" => "The issuer DID could not be resolved",
        "issuer_not_trusted" => "The issuer is not in this wallet's trust policy",
        "issuer_resolution_error" => "The issuer DID could not be verified",
        "issuer_subject_mismatch" => "The issuer and subject binding is inconsistent",
        "method_controller_mismatch" => "The verification method has the wrong controller",
        "method_not_assertion_authorized" => {
            "The verification method is not authorized for credential assertions"
        }
        "proof_after_expiration" => "The proof was created after the credential expired",
        "proof_before_issuance" => "The proof predates credential issuance",
        "proof_in_future" => "The proof creation time is in the future",
        "schema_mismatch" => "The credential schema does not match",
        "verification_method_missing" => "The issuer verification method was not found",
        "version_mismatch" => "The credential version is not supported",
        _ => "The verification check did not pass",
    }
}

pub(crate) fn claim_privacy(value: &str) -> &'static str {
    match value {
        "selective_disclosure" => "Selectively disclosed",
        "predicate_only" => "Predicate only",
        _ => "Disclosure policy unavailable",
    }
}

pub(crate) fn disclosure_outcome(value: &str) -> &'static str {
    match value {
        "local_preview_ready" => "Disclosure preview ready",
        _ => "Preview status unavailable",
    }
}

pub(crate) fn credential_schema(value: &str) -> &'static str {
    match value {
        "digital-passport:v1" => "Digital Passport claims",
        _ => "Credential claims",
    }
}

pub(crate) fn vault_call_mode(value: &str) -> &'static str {
    match value {
        "native_settlement" => "Midnight live",
        "deterministic_simulation" => "Simulated — runs locally, nothing on Midnight",
        _ => "Unavailable",
    }
}

pub(crate) fn vault_call_mode_note(value: &str) -> &'static str {
    match value {
        "native_settlement" => {
            "Calls use authenticated finalized state and the protected Midnight proving, submission, and reconciliation boundary."
        }
        "deterministic_simulation" => {
            "Calls exercise the complete retained lifecycle locally; no node broadcast occurs."
        }
        _ => {
            "Configure the complete standalone Midnight stack and authenticated Passport Vault artifacts to enable contract calls."
        }
    }
}

pub(crate) fn vault_contract_source(value: &str) -> &'static str {
    match value {
        "standalone" => "Standalone conformance ledger",
        "pinned_contract_layout" => "Decoded contract snapshot",
        "node_anchored_indexer" => "Midnight indexer",
        "authenticated_node" => "Midnight node",
        "finalized_node_replay" => "Finalized Midnight replay",
        "deterministic_simulation" => "Simulated — runs locally, nothing on Midnight",
        _ => "Source unavailable",
    }
}

pub(crate) fn vault_state_authentication(value: &str) -> &'static str {
    match value {
        "indexer_supplied_not_proven" => "Reported by an indexer — not yet verified",
        "canonical_finalized_replay" => "Verified against the Midnight network",
        "deterministic_simulation" | "simulated_or_unanchored" => {
            "Simulated — runs locally, nothing on Midnight"
        }
        _ => "Verification unavailable",
    }
}

pub(crate) fn vault_submission_mode(value: &str) -> &'static str {
    match value {
        "native_settlement" | "live" => "Submitted to Midnight",
        "deterministic_simulation" | "deterministic_simulation_only" | "simulated" => {
            "Simulated — runs locally, nothing on Midnight"
        }
        _ => "Mode unavailable",
    }
}

pub(crate) fn vault_operation(value: &str) -> &'static str {
    match value {
        "create_lock" => "Create lock",
        "deposit_to_lock" => "Deposit to lock",
        "claim_from_lock" => "Claim from lock",
        "withdraw_from_lock" => "Withdraw from lock",
        _ => "Vault operation",
    }
}

pub(crate) fn vault_draft_state(value: &str) -> &'static str {
    match value {
        "prepared" => "Prepared",
        "authorized" => "Authorized",
        "submitting" => "Submitting",
        "submitted" => "Submitted",
        "expired" => "Expired",
        _ => "Status unavailable",
    }
}

pub(crate) fn vault_submission_heading(value: &str) -> &'static str {
    match value {
        "included" => "Vault call included",
        "broadcasting" => "Vault call broadcast",
        "outcome_unknown" => "Checking vault-call outcome",
        "rejected" => "Vault call rejected",
        "expired" => "Vault call expired",
        "cancelled" => "Vault call cancelled",
        "not_started" | "running" | "cancellation_requested" => "Vault call in progress",
        _ => "Vault-call status unavailable",
    }
}

pub(crate) fn vault_submission_note(value: &str) -> &'static str {
    match value {
        "included" => "Midnight reported finalized public inclusion metadata for this call.",
        "broadcasting" => {
            "The broadcast boundary was crossed; cancellation and replacement are disabled."
        }
        "outcome_unknown" => {
            "Oxid will not submit a duplicate while it checks finalized Midnight history."
        }
        "rejected" => "Finalized history rejected this attempt; prepare a fresh call if allowed.",
        "expired" => "The retained authorization expired before safe completion.",
        "cancelled" => "The worker stopped before broadcast; the authorized call may be retried.",
        "not_started" | "running" | "cancellation_requested" => {
            "Proving or submission is still running."
        }
        _ => "The vault-call status could not be identified safely.",
    }
}

pub(crate) fn vault_persistence_note(value: &str) -> &'static str {
    match value {
        "owner_private_atomic_file" => {
            "Owner-private saved conformance ledger · survives app restart · no on-chain transaction submitted"
        }
        "process_local" => "Process-local conformance ledger · no on-chain transaction submitted",
        _ => "Standalone conformance ledger · no on-chain transaction submitted",
    }
}

/// Formats an unsigned decimal subunit string without floating point.
pub(crate) fn format_atomic_units(value: &str, decimals: u8) -> String {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return "—".to_owned();
    }
    let value = value.trim_start_matches('0');
    let value = if value.is_empty() { "0" } else { value };
    if decimals == 0 {
        return value.to_owned();
    }
    let decimals = usize::from(decimals);
    let padded = if value.len() <= decimals {
        format!("{}{}", "0".repeat(decimals + 1 - value.len()), value)
    } else {
        value.to_owned()
    };
    let split = padded.len() - decimals;
    let whole = &padded[..split];
    let fraction = padded[split..].trim_end_matches('0');
    if fraction.is_empty() {
        whole.to_owned()
    } else {
        format!("{whole}.{fraction}")
    }
}

pub(crate) fn format_asset_amount(value: &str, decimals: u8, symbol: &str) -> String {
    format!("{} {symbol}", format_atomic_units(value, decimals))
}

pub(crate) fn format_night_amount(value: impl ToString) -> String {
    format_asset_amount(&value.to_string(), NIGHT_DECIMALS, "NIGHT")
}

pub(crate) fn format_dust_amount(value: impl ToString) -> String {
    format_asset_amount(&value.to_string(), DUST_DECIMALS, "DUST")
}

pub(crate) fn format_shielded_amount(token_type_hex: &str, value: &str) -> String {
    if token_type_hex == "0".repeat(64) {
        format_night_amount(value)
    } else if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        format!("{} smallest token increments", group_digits(value))
    } else {
        "Token amount unavailable".to_owned()
    }
}

pub(crate) fn parse_night_amount(value: &str, allow_zero: bool) -> Result<String, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Err("enter a NIGHT amount");
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|part| !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("NIGHT amount must be a positive decimal number");
    }
    let fraction = fraction.unwrap_or_default();
    if fraction.len() > usize::from(NIGHT_DECIMALS) {
        return Err("NIGHT supports at most 6 decimal places");
    }
    let padded_fraction = format!("{fraction:0<6}");
    let atomic = format!("{whole}{padded_fraction}")
        .parse::<u128>()
        .map_err(|_| "NIGHT amount is too large")?;
    if atomic == 0 && !allow_zero {
        return Err("NIGHT amount must be greater than zero");
    }
    Ok(atomic.to_string())
}

/// Renders Unix milliseconds as a stable UTC civil time without locale or
/// platform dependencies. Milliseconds outside the civil range are hidden.
pub(crate) fn format_epoch_millis(value: u64) -> String {
    const MAX_SUPPORTED_MILLIS: u64 = 253_402_300_799_999;
    if value > MAX_SUPPORTED_MILLIS {
        return "Date unavailable".to_owned();
    }
    let total_seconds = value / 1_000;
    let days = i64::try_from(total_seconds / 86_400).unwrap_or_default();
    let seconds = total_seconds % 86_400;
    let (year, month, day) = civil_date_from_unix_days(days);
    let hour = seconds / 3_600;
    let minute = (seconds % 3_600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

fn civil_date_from_unix_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn group_digits(value: &str) -> String {
    let mut grouped = String::with_capacity(value.len() + value.len() / 3);
    for (index, character) in value.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(character);
    }
    grouped.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_machine_values_never_echo() {
        let raw = "future_machine_value";
        for label in [
            account_source(raw),
            sync_state(raw),
            sync_failure(raw),
            submission_state(raw),
            submission_mode(raw),
            protocol_state(raw),
            protocol_failure(raw),
            credential_format(raw),
            verification_outcome(raw),
            verification_reason(raw),
            vault_contract_source(raw),
            vault_state_authentication(raw),
            vault_submission_mode(raw),
            vault_operation(raw),
        ] {
            assert_ne!(label, raw);
            assert!(!label.contains('_'));
        }
    }

    #[test]
    fn asset_amounts_are_exact_and_named() {
        assert_eq!(format_night_amount("1500000"), "1.5 NIGHT");
        assert_eq!(format_dust_amount("2500000000000000"), "2.5 DUST");
        assert_eq!(format_night_amount("invalid"), "— NIGHT");
        assert_eq!(
            format_shielded_amount(&"1".repeat(64), ""),
            "Token amount unavailable"
        );
    }

    #[test]
    fn night_input_converts_to_exact_subunits() {
        assert_eq!(
            parse_night_amount("12.345678", false),
            Ok("12345678".to_owned())
        );
        assert_eq!(parse_night_amount("0", true), Ok("0".to_owned()));
        assert!(parse_night_amount("0", false).is_err());
        assert!(parse_night_amount("1.0000001", false).is_err());
    }

    #[test]
    fn unix_milliseconds_render_as_readable_utc() {
        assert_eq!(format_epoch_millis(0), "1970-01-01 00:00 UTC");
        assert_eq!(
            format_epoch_millis(1_700_000_000_000),
            "2023-11-14 22:13 UTC"
        );
        assert_eq!(format_epoch_millis(u64::MAX), "Date unavailable");
    }

    #[test]
    fn vocabulary_matches_the_design_contract() {
        assert_eq!(
            vault_state_authentication("canonical_finalized_replay"),
            "Verified against the Midnight network"
        );
        assert_eq!(
            vault_state_authentication("indexer_supplied_not_proven"),
            "Reported by an indexer — not yet verified"
        );
        assert_eq!(
            vault_call_mode("deterministic_simulation"),
            "Simulated — runs locally, nothing on Midnight"
        );
        assert_eq!(
            submission_state("outcome_unknown"),
            "Checking with the network…"
        );
        assert_eq!(
            credential_format("midnight_compact_vc"),
            "Digital Passport (Midnight format)"
        );
    }
}
