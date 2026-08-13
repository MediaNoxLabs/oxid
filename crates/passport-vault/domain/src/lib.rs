// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const MAX_VAULT_ACTOR_CHARACTERS: usize = 128;
pub const MAX_VAULT_LOCKS: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VaultActorId(String);

impl VaultActorId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, PassportVaultError> {
        let value = value.as_ref();
        if value.is_empty()
            || value.chars().count() > MAX_VAULT_ACTOR_CHARACTERS
            || value.trim() != value
            || value.chars().any(|character| character.is_control())
        {
            return Err(PassportVaultError::InvalidActor);
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VaultLockId(u64);

impl VaultLockId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CredentialFingerprint([u8; 32]);

impl CredentialFingerprint {
    pub fn new(value: [u8; 32]) -> Result<Self, PassportVaultError> {
        if value == [0; 32] {
            return Err(PassportVaultError::InvalidCredentialEvidence);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for CredentialFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialFingerprint([redacted])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultPolicy {
    minimum_age_years: u8,
    required_issuing_state: Option<[u8; 32]>,
    required_document_number: Option<[u8; 32]>,
    maximum_claim_amount: u128,
    verifier_challenge_hash: [u8; 32],
}

impl PassportVaultPolicy {
    pub fn new(
        minimum_age_years: u8,
        required_issuing_state: Option<[u8; 32]>,
        required_document_number: Option<[u8; 32]>,
        maximum_claim_amount: u128,
        verifier_challenge_hash: [u8; 32],
    ) -> Result<Self, PassportVaultError> {
        if minimum_age_years > 120 {
            return Err(PassportVaultError::InvalidMinimumAge);
        }
        if maximum_claim_amount == 0 {
            return Err(PassportVaultError::InvalidAmount);
        }
        if verifier_challenge_hash == [0; 32]
            || required_issuing_state == Some([0; 32])
            || required_document_number == Some([0; 32])
        {
            return Err(PassportVaultError::InvalidPolicy);
        }
        Ok(Self {
            minimum_age_years,
            required_issuing_state,
            required_document_number,
            maximum_claim_amount,
            verifier_challenge_hash,
        })
    }

    #[must_use]
    pub const fn minimum_age_years(&self) -> u8 {
        self.minimum_age_years
    }
    #[must_use]
    pub const fn required_issuing_state(&self) -> Option<[u8; 32]> {
        self.required_issuing_state
    }
    #[must_use]
    pub const fn required_document_number(&self) -> Option<[u8; 32]> {
        self.required_document_number
    }
    #[must_use]
    pub const fn maximum_claim_amount(&self) -> u128 {
        self.maximum_claim_amount
    }
    #[must_use]
    pub const fn verifier_challenge_hash(&self) -> [u8; 32] {
        self.verifier_challenge_hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultLock {
    id: VaultLockId,
    creator: VaultActorId,
    policy: PassportVaultPolicy,
    total_deposited: u128,
    total_released: u128,
}

impl PassportVaultLock {
    #[must_use]
    pub const fn id(&self) -> VaultLockId {
        self.id
    }
    #[must_use]
    pub const fn creator(&self) -> &VaultActorId {
        &self.creator
    }
    #[must_use]
    pub const fn policy(&self) -> &PassportVaultPolicy {
        &self.policy
    }
    #[must_use]
    pub const fn total_deposited(&self) -> u128 {
        self.total_deposited
    }
    #[must_use]
    pub const fn total_released(&self) -> u128 {
        self.total_released
    }
    #[must_use]
    pub const fn remaining(&self) -> u128 {
        self.total_deposited - self.total_released
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultClaimReceipt {
    pub lock_id: VaultLockId,
    pub amount: u128,
    pub current_day: u32,
    pub remaining: u128,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PassportVaultState {
    next_lock_id: u64,
    locks: BTreeMap<VaultLockId, PassportVaultLock>,
    consumed_claims: BTreeSet<(VaultLockId, CredentialFingerprint)>,
    total_deposited: u128,
    total_released: u128,
    claim_count: u64,
}

impl PassportVaultState {
    pub fn create_lock(
        &mut self,
        creator: VaultActorId,
        policy: PassportVaultPolicy,
        initial_amount: u128,
    ) -> Result<VaultLockId, PassportVaultError> {
        if self.locks.len() >= MAX_VAULT_LOCKS {
            return Err(PassportVaultError::CapacityExceeded);
        }
        let id = VaultLockId::new(self.next_lock_id);
        let next_lock_id = self
            .next_lock_id
            .checked_add(1)
            .ok_or(PassportVaultError::Overflow)?;
        let total_deposited = self
            .total_deposited
            .checked_add(initial_amount)
            .ok_or(PassportVaultError::Overflow)?;
        let lock = PassportVaultLock {
            id,
            creator,
            policy,
            total_deposited: initial_amount,
            total_released: 0,
        };
        self.locks.insert(id, lock);
        self.next_lock_id = next_lock_id;
        self.total_deposited = total_deposited;
        Ok(id)
    }

    pub fn deposit(
        &mut self,
        actor: &VaultActorId,
        lock_id: VaultLockId,
        amount: u128,
    ) -> Result<(), PassportVaultError> {
        if amount == 0 {
            return Err(PassportVaultError::InvalidAmount);
        }
        let next_total = self
            .total_deposited
            .checked_add(amount)
            .ok_or(PassportVaultError::Overflow)?;
        let lock = self
            .locks
            .get_mut(&lock_id)
            .ok_or(PassportVaultError::LockNotFound)?;
        if lock.creator != *actor {
            return Err(PassportVaultError::NotLockCreator);
        }
        lock.total_deposited = lock
            .total_deposited
            .checked_add(amount)
            .ok_or(PassportVaultError::Overflow)?;
        self.total_deposited = next_total;
        Ok(())
    }

    pub fn withdraw(
        &mut self,
        actor: &VaultActorId,
        lock_id: VaultLockId,
        amount: u128,
    ) -> Result<u128, PassportVaultError> {
        if amount == 0 {
            return Err(PassportVaultError::InvalidAmount);
        }
        let next_total = self
            .total_released
            .checked_add(amount)
            .ok_or(PassportVaultError::Overflow)?;
        let lock = self
            .locks
            .get_mut(&lock_id)
            .ok_or(PassportVaultError::LockNotFound)?;
        if lock.creator != *actor {
            return Err(PassportVaultError::NotLockCreator);
        }
        if amount > lock.remaining() {
            return Err(PassportVaultError::InsufficientLockBalance);
        }
        lock.total_released = lock
            .total_released
            .checked_add(amount)
            .ok_or(PassportVaultError::Overflow)?;
        self.total_released = next_total;
        Ok(lock.remaining())
    }

    pub fn claim(
        &mut self,
        lock_id: VaultLockId,
        credential: CredentialFingerprint,
        amount: u128,
        current_day: u32,
    ) -> Result<PassportVaultClaimReceipt, PassportVaultError> {
        if amount == 0 || current_day == 0 {
            return Err(PassportVaultError::InvalidAmount);
        }
        if self.consumed_claims.contains(&(lock_id, credential)) {
            return Err(PassportVaultError::CredentialAlreadyClaimed);
        }
        let next_total = self
            .total_released
            .checked_add(amount)
            .ok_or(PassportVaultError::Overflow)?;
        let next_count = self
            .claim_count
            .checked_add(1)
            .ok_or(PassportVaultError::Overflow)?;
        let lock = self
            .locks
            .get_mut(&lock_id)
            .ok_or(PassportVaultError::LockNotFound)?;
        if amount > lock.policy.maximum_claim_amount {
            return Err(PassportVaultError::ClaimExceedsMaximum);
        }
        if amount > lock.remaining() {
            return Err(PassportVaultError::InsufficientLockBalance);
        }
        lock.total_released = lock
            .total_released
            .checked_add(amount)
            .ok_or(PassportVaultError::Overflow)?;
        let remaining = lock.remaining();
        self.consumed_claims.insert((lock_id, credential));
        self.total_released = next_total;
        self.claim_count = next_count;
        Ok(PassportVaultClaimReceipt {
            lock_id,
            amount,
            current_day,
            remaining,
        })
    }

    #[must_use]
    pub fn lock(&self, id: VaultLockId) -> Option<&PassportVaultLock> {
        self.locks.get(&id)
    }
    pub fn locks(&self) -> impl Iterator<Item = &PassportVaultLock> {
        self.locks.values()
    }
    #[must_use]
    pub const fn total_deposited(&self) -> u128 {
        self.total_deposited
    }
    #[must_use]
    pub const fn total_released(&self) -> u128 {
        self.total_released
    }
    #[must_use]
    pub const fn total_locked(&self) -> u128 {
        self.total_deposited - self.total_released
    }
    #[must_use]
    pub const fn claim_count(&self) -> u64 {
        self.claim_count
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultError {
    InvalidActor,
    InvalidMinimumAge,
    InvalidPolicy,
    InvalidAmount,
    InvalidCredentialEvidence,
    CapacityExceeded,
    LockNotFound,
    NotLockCreator,
    ClaimExceedsMaximum,
    InsufficientLockBalance,
    CredentialAlreadyClaimed,
    Overflow,
}

impl fmt::Display for PassportVaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidActor => "vault actor identifier is invalid",
            Self::InvalidMinimumAge => "minimum age must be between 0 and 120",
            Self::InvalidPolicy => "vault policy is invalid",
            Self::InvalidAmount => "vault amount must be positive",
            Self::InvalidCredentialEvidence => "credential evidence is invalid",
            Self::CapacityExceeded => "vault lock capacity was exceeded",
            Self::LockNotFound => "vault lock was not found",
            Self::NotLockCreator => "only the lock creator may perform this operation",
            Self::ClaimExceedsMaximum => "claim exceeds the lock maximum",
            Self::InsufficientLockBalance => "lock balance is insufficient",
            Self::CredentialAlreadyClaimed => "credential has already claimed from this lock",
            Self::Overflow => "vault accounting overflowed",
        })
    }
}

impl Error for PassportVaultError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(value: &str) -> VaultActorId {
        VaultActorId::parse(value).expect("actor")
    }
    fn policy() -> PassportVaultPolicy {
        PassportVaultPolicy::new(18, None, None, 40, [7; 32]).expect("policy")
    }

    #[test]
    fn enforces_multi_lock_accounting_creator_authority_and_claim_replay() {
        let mut vault = PassportVaultState::default();
        let first = vault
            .create_lock(actor("profile_creator"), policy(), 100)
            .expect("lock");
        let second = vault
            .create_lock(actor("profile_other"), policy(), 50)
            .expect("lock");
        assert_eq!(vault.total_locked(), 150);
        assert_eq!(
            vault.deposit(&actor("profile_other"), first, 1),
            Err(PassportVaultError::NotLockCreator)
        );
        vault
            .deposit(&actor("profile_creator"), first, 20)
            .expect("deposit");
        let fingerprint = CredentialFingerprint::new([9; 32]).expect("fingerprint");
        let receipt = vault.claim(first, fingerprint, 40, 20_000).expect("claim");
        assert_eq!(receipt.remaining, 80);
        assert_eq!(
            vault.claim(first, fingerprint, 1, 20_000),
            Err(PassportVaultError::CredentialAlreadyClaimed)
        );
        assert!(vault.claim(second, fingerprint, 1, 20_000).is_ok());
        assert_eq!(vault.withdraw(&actor("profile_creator"), first, 80), Ok(0));
        assert_eq!(vault.total_deposited(), 170);
        assert_eq!(vault.total_released(), 121);
        assert_eq!(vault.claim_count(), 2);
    }

    #[test]
    fn rejects_unsafe_policy_and_amount_boundaries() {
        assert_eq!(
            PassportVaultPolicy::new(121, None, None, 1, [1; 32]),
            Err(PassportVaultError::InvalidMinimumAge)
        );
        assert_eq!(
            PassportVaultPolicy::new(18, None, None, 0, [1; 32]),
            Err(PassportVaultError::InvalidAmount)
        );
        assert_eq!(
            PassportVaultPolicy::new(18, None, None, 1, [0; 32]),
            Err(PassportVaultError::InvalidPolicy)
        );
        assert_eq!(
            CredentialFingerprint::new([0; 32]),
            Err(PassportVaultError::InvalidCredentialEvidence)
        );
    }
}
