// SPDX-License-Identifier: Apache-2.0

//! Native decoder for the exact Passport Vault v1 Compact ledger layout.
//!
//! The layout is generated from the immutable source authenticated by the
//! `passport-vault-compact-artifacts` Nix closure. This decoder deliberately
//! consumes Midnight's tagged Rust `ContractState`; it does not embed a JS
//! runtime, generated TypeScript, a WebView bridge, or a contract address.

use std::io::Cursor;

use midnight_base_crypto::fab::{Aligned, AlignedValue, ValueAtom};
use midnight_onchain_runtime::state::{ContractState, StateValue};
use midnight_serialize::tagged_deserialize;
use oxid_passport_vault_application::{
    PassportVaultContractStateDecoderPort, PassportVaultContractStateError,
    PassportVaultContractView, PassportVaultLockView, PassportVaultView,
};

const PASSPORT_VAULT_CONTRACT_VERSION: u32 = 1;
const PASSPORT_VAULT_LEDGER_FIELDS: usize = 15;
const MAX_PASSPORT_VAULT_LOCKS: u64 = 4_096;

const CONTRACT_VERSION_INDEX: usize = 0;
const TRUSTED_ISSUER_INDEX: usize = 2;
const TRUSTED_ISSUER_KEY_HASH_INDEX: usize = 3;
const LOCKS_INDEX: usize = 4;
const LOCK_COUNT_INDEX: usize = 5;
const CONSUMED_CLAIMS_INDEX: usize = 6;
const TOTAL_DEPOSITED_INDEX: usize = 7;
const TOTAL_RELEASED_INDEX: usize = 8;
const CLAIM_COUNT_INDEX: usize = 9;
const LAST_CREDENTIAL_ROOT_INDEX: usize = 10;
const LAST_CURRENT_DAY_INDEX: usize = 11;
const LAST_THRESHOLD_INDEX: usize = 12;
const LAST_RELEASED_AMOUNT_INDEX: usize = 13;
const LAST_DECISION_INDEX: usize = 14;

type LockRecord = (
    [u8; 32],
    u8,
    bool,
    [u8; 32],
    bool,
    [u8; 32],
    u128,
    [u8; 32],
    u128,
    u128,
);

#[derive(Clone, Copy, Debug, Default)]
pub struct NativePassportVaultContractStateDecoder;

impl PassportVaultContractStateDecoderPort for NativePassportVaultContractStateDecoder {
    fn decode(
        &self,
        serialized_contract_state: &[u8],
    ) -> Result<PassportVaultView, PassportVaultContractStateError> {
        decode_contract_state(serialized_contract_state)
    }
}

fn decode_contract_state(
    serialized_contract_state: &[u8],
) -> Result<PassportVaultView, PassportVaultContractStateError> {
    let mut cursor = Cursor::new(serialized_contract_state);
    let contract: ContractState<midnight_storage::DefaultDB> = tagged_deserialize(&mut cursor)
        .map_err(|_| PassportVaultContractStateError::InvalidEncoding)?;
    if cursor.position() != serialized_contract_state.len() as u64 {
        return Err(PassportVaultContractStateError::InvalidEncoding);
    }
    let fields = match contract.data.get_ref() {
        StateValue::Array(fields) if fields.len() == PASSPORT_VAULT_LEDGER_FIELDS => fields,
        _ => return Err(PassportVaultContractStateError::LayoutMismatch),
    };

    let version = decode_cell::<u32>(fields.get(CONTRACT_VERSION_INDEX))?;
    if version != PASSPORT_VAULT_CONTRACT_VERSION {
        return Err(PassportVaultContractStateError::UnsupportedVersion);
    }
    let (issuer_contract, issuer_method) = decode_two_bytes32(fields.get(TRUSTED_ISSUER_INDEX))?;
    let issuer_key_hash = decode_bytes32(fields.get(TRUSTED_ISSUER_KEY_HASH_INDEX))?;
    let lock_count = decode_cell::<u64>(fields.get(LOCK_COUNT_INDEX))?;
    if lock_count > MAX_PASSPORT_VAULT_LOCKS {
        return Err(PassportVaultContractStateError::CapacityExceeded);
    }
    let locks = match fields.get(LOCKS_INDEX) {
        Some(StateValue::Map(locks)) if locks.size() == lock_count as usize => locks,
        _ => return Err(PassportVaultContractStateError::LayoutMismatch),
    };
    let consumed_claim_count = match fields.get(CONSUMED_CLAIMS_INDEX) {
        Some(StateValue::Map(consumed)) => u64::try_from(consumed.size())
            .map_err(|_| PassportVaultContractStateError::CapacityExceeded)?,
        _ => return Err(PassportVaultContractStateError::LayoutMismatch),
    };

    let mut decoded_locks = Vec::with_capacity(lock_count as usize);
    let mut summed_deposited = 0_u128;
    let mut summed_released = 0_u128;
    for lock_id in 0..lock_count {
        let key = AlignedValue::from(lock_id);
        let value = locks
            .get(&key)
            .ok_or(PassportVaultContractStateError::LayoutMismatch)?;
        let record = decode_lock_record(Some(&*value))?;
        let (
            locker,
            minimum_age,
            require_issuing_state,
            issuing_state,
            require_document_number,
            document_number,
            maximum_claim,
            challenge,
            total_deposited,
            total_released,
        ) = record;
        let remaining = total_deposited
            .checked_sub(total_released)
            .ok_or(PassportVaultContractStateError::Integrity)?;
        summed_deposited = summed_deposited
            .checked_add(total_deposited)
            .ok_or(PassportVaultContractStateError::Integrity)?;
        summed_released = summed_released
            .checked_add(total_released)
            .ok_or(PassportVaultContractStateError::Integrity)?;
        decoded_locks.push(PassportVaultLockView {
            lock_id,
            creator_profile_id: format!("coinpk:{}", hex::encode(locker)),
            minimum_age_years: minimum_age,
            required_issuing_state: require_issuing_state.then(|| policy_value(issuing_state)),
            required_document_number: require_document_number
                .then(|| policy_value(document_number)),
            maximum_claim_amount: maximum_claim.to_string(),
            total_deposited: total_deposited.to_string(),
            total_released: total_released.to_string(),
            remaining: remaining.to_string(),
            verifier_challenge_hex: hex::encode(challenge),
        });
    }

    let total_deposited = decode_cell::<u128>(fields.get(TOTAL_DEPOSITED_INDEX))?;
    let total_released = decode_cell::<u128>(fields.get(TOTAL_RELEASED_INDEX))?;
    let total_locked = total_deposited
        .checked_sub(total_released)
        .ok_or(PassportVaultContractStateError::Integrity)?;
    if total_deposited != summed_deposited || total_released != summed_released {
        return Err(PassportVaultContractStateError::Integrity);
    }
    let claim_count = decode_cell::<u64>(fields.get(CLAIM_COUNT_INDEX))?;
    if claim_count != consumed_claim_count {
        return Err(PassportVaultContractStateError::Integrity);
    }
    let last_decision = decode_cell::<u8>(fields.get(LAST_DECISION_INDEX))?;
    let last_business_decision = match last_decision {
        0 => "no_decision",
        1 => "released",
        _ => return Err(PassportVaultContractStateError::LayoutMismatch),
    };
    decode_bytes32(fields.get(LAST_CREDENTIAL_ROOT_INDEX))?;

    Ok(PassportVaultView {
        source: "pinned_contract_layout".to_owned(),
        contract: Some(PassportVaultContractView {
            version,
            trusted_issuer_did_contract_hex: hex::encode(issuer_contract),
            trusted_issuer_method_hex: hex::encode(issuer_method),
            trusted_issuer_public_key_hash_hex: hex::encode(issuer_key_hash),
            consumed_claim_count,
            last_verified_current_day: decode_cell::<u32>(fields.get(LAST_CURRENT_DAY_INDEX))?,
            last_verified_threshold_years: decode_cell::<u8>(fields.get(LAST_THRESHOLD_INDEX))?,
            last_released_amount: decode_cell::<u128>(fields.get(LAST_RELEASED_AMOUNT_INDEX))?
                .to_string(),
            last_business_decision: last_business_decision.to_owned(),
        }),
        locks: decoded_locks,
        total_deposited: total_deposited.to_string(),
        total_released: total_released.to_string(),
        total_locked: total_locked.to_string(),
        claim_count,
    })
}

fn decode_cell<T>(value: Option<&StateValue>) -> Result<T, PassportVaultContractStateError>
where
    T: Aligned + for<'a> TryFrom<&'a midnight_base_crypto::fab::ValueSlice>,
{
    let StateValue::Cell(value) = value.ok_or(PassportVaultContractStateError::LayoutMismatch)?
    else {
        return Err(PassportVaultContractStateError::LayoutMismatch);
    };
    if value.alignment != T::alignment() {
        return Err(PassportVaultContractStateError::LayoutMismatch);
    }
    T::try_from(&*value.value).map_err(|_| PassportVaultContractStateError::LayoutMismatch)
}

fn decode_bytes32(value: Option<&StateValue>) -> Result<[u8; 32], PassportVaultContractStateError> {
    let value = cell(value)?;
    if value.alignment != <[u8; 32]>::alignment() || value.value.0.len() != 1 {
        return Err(PassportVaultContractStateError::LayoutMismatch);
    }
    bytes32(&value.value.0[0])
}

fn decode_two_bytes32(
    value: Option<&StateValue>,
) -> Result<([u8; 32], [u8; 32]), PassportVaultContractStateError> {
    let value = cell(value)?;
    if value.alignment != <([u8; 32], [u8; 32])>::alignment() || value.value.0.len() != 2 {
        return Err(PassportVaultContractStateError::LayoutMismatch);
    }
    Ok((bytes32(&value.value.0[0])?, bytes32(&value.value.0[1])?))
}

fn decode_lock_record(
    value: Option<&StateValue>,
) -> Result<LockRecord, PassportVaultContractStateError> {
    let value = cell(value)?;
    let atoms = &value.value.0;
    if value.alignment != LockRecord::alignment() || atoms.len() != 10 {
        return Err(PassportVaultContractStateError::LayoutMismatch);
    }
    Ok((
        bytes32(&atoms[0])?,
        decode_atom(&atoms[1])?,
        decode_atom(&atoms[2])?,
        bytes32(&atoms[3])?,
        decode_atom(&atoms[4])?,
        bytes32(&atoms[5])?,
        decode_atom(&atoms[6])?,
        bytes32(&atoms[7])?,
        decode_atom(&atoms[8])?,
        decode_atom(&atoms[9])?,
    ))
}

fn cell(value: Option<&StateValue>) -> Result<&AlignedValue, PassportVaultContractStateError> {
    let StateValue::Cell(value) = value.ok_or(PassportVaultContractStateError::LayoutMismatch)?
    else {
        return Err(PassportVaultContractStateError::LayoutMismatch);
    };
    Ok(value)
}

fn bytes32(atom: &ValueAtom) -> Result<[u8; 32], PassportVaultContractStateError> {
    atom.clone()
        .try_into()
        .map_err(|_| PassportVaultContractStateError::LayoutMismatch)
}

fn decode_atom<T>(atom: &ValueAtom) -> Result<T, PassportVaultContractStateError>
where
    for<'a> T: TryFrom<&'a ValueAtom>,
{
    T::try_from(atom).map_err(|_| PassportVaultContractStateError::LayoutMismatch)
}

fn policy_value(bytes: [u8; 32]) -> String {
    let end = bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |index| index + 1);
    if bytes[..end]
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    } else {
        format!("0x{}", hex::encode(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_HEX: &str =
        include_str!("../../../../fixtures/passport-vault/contract-state-v1.hex");

    fn fixture() -> Vec<u8> {
        hex::decode(FIXTURE_HEX.trim()).expect("generated Compact fixture is valid hex")
    }

    #[test]
    fn decodes_the_pinned_generated_compact_contract_state_natively() {
        let view = decode_contract_state(&fixture()).expect("contract state decodes");
        assert_eq!(view.source, "pinned_contract_layout");
        assert_eq!(view.total_deposited, "0");
        assert_eq!(view.total_released, "0");
        assert_eq!(view.total_locked, "0");
        assert_eq!(view.claim_count, 0);
        assert_eq!(view.locks.len(), 2);
        assert_eq!(view.locks[0].lock_id, 0);
        assert_eq!(view.locks[0].minimum_age_years, 18);
        assert_eq!(view.locks[0].maximum_claim_amount, "40");
        assert_eq!(view.locks[0].verifier_challenge_hex, "05".repeat(32));
        assert_eq!(view.locks[1].lock_id, 1);
        assert_eq!(view.locks[1].minimum_age_years, 21);
        assert_eq!(view.locks[1].maximum_claim_amount, "25");
        assert_eq!(view.locks[1].verifier_challenge_hex, "07".repeat(32));
        let contract = view.contract.expect("contract metadata");
        assert_eq!(contract.version, 1);
        assert_eq!(contract.trusted_issuer_did_contract_hex, "02".repeat(32));
        assert_eq!(contract.trusted_issuer_method_hex, "03".repeat(32));
        assert_eq!(contract.consumed_claim_count, 0);
        assert_eq!(contract.last_business_decision, "no_decision");
    }

    #[test]
    fn rejects_trailing_or_tampered_layout_bytes() {
        let mut trailing = fixture();
        trailing.push(0);
        assert_eq!(
            decode_contract_state(&trailing),
            Err(PassportVaultContractStateError::InvalidEncoding)
        );

        let mut malformed = fixture();
        malformed[0] ^= 0xff;
        assert_eq!(
            decode_contract_state(&malformed),
            Err(PassportVaultContractStateError::InvalidEncoding)
        );
    }
}
