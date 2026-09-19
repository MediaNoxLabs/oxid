// SPDX-License-Identifier: Apache-2.0

//! Durable, adapter-neutral recovery boundary for DUST registration.

use std::sync::Mutex;

use oxid_wallet_domain::{
    ChainNetworkId, ChainTransactionId, WalletProfileId, WalletTransactionDraftId,
};

use crate::{
    WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION, WalletDustRegistrationRuntime,
    WalletDustRegistrationRuntimeCheckpoint,
    WalletDustRegistrationSettlementAuthorizationPhase as AuthorizationPhase,
    WalletDustRegistrationSettlementCheckpoint as Checkpoint,
    WalletDustRegistrationSettlementIdentity as Identity,
    WalletDustRegistrationSettlementRegistration as Registration,
    WalletDustRegistrationSettlementState as State, recovered_dust_registration_coordinator,
    recovered_dust_registration_projection,
};

pub const WALLET_DUST_REGISTRATION_RECOVERY_VERSION: u16 = 1;
const MAX_RECOVERY_BYTES: usize = 4096;
const MAGIC: &[u8; 4] = b"DRR1";

/// Explicit public state retained across restart. No runtime admission, custody,
/// authorization challenge, transaction body, or adapter error is represented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationRecoveryRecord {
    pub version: u16,
    pub profile_id: String,
    pub realm_id: String,
    pub generation: u64,
    pub eligibility_revision: u64,
    pub eligible: bool,
    pub preparation_revision: u64,
    pub recovery_revision: u64,
    pub state: State,
    pub registration: Option<WalletDustRegistrationRecoveryRegistration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletDustRegistrationRecoveryRegistration {
    pub draft_id: String,
    pub authorization_phase: AuthorizationPhase,
    pub transaction_id: Option<String>,
    pub observation_revision: u64,
    pub finality_revision: u64,
    pub reconciliation_revision: u64,
    pub dust_revision: u64,
    pub dust_observation_revision: u64,
    pub dust_ready: bool,
    pub included: bool,
    pub abandonment_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationRecoveryStoreError {
    Unavailable,
    Corrupt,
}

pub trait WalletDustRegistrationRecoveryStore: Send + Sync {
    fn save(
        &self,
        record: WalletDustRegistrationRecoveryRecord,
    ) -> Result<(), WalletDustRegistrationRecoveryStoreError>;
    fn load(
        &self,
    ) -> Result<
        Option<WalletDustRegistrationRecoveryRecord>,
        WalletDustRegistrationRecoveryStoreError,
    >;

    /// Removes a record when the runtime enters a state recovery v1 cannot represent.
    fn clear(&self) -> Result<(), WalletDustRegistrationRecoveryStoreError>;
}

#[derive(Default)]
pub struct InMemoryWalletDustRegistrationRecoveryStore(
    Mutex<Option<WalletDustRegistrationRecoveryRecord>>,
);
impl WalletDustRegistrationRecoveryStore for InMemoryWalletDustRegistrationRecoveryStore {
    fn save(
        &self,
        record: WalletDustRegistrationRecoveryRecord,
    ) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
        record
            .validate()
            .map_err(|_| WalletDustRegistrationRecoveryStoreError::Corrupt)?;
        *self
            .0
            .lock()
            .map_err(|_| WalletDustRegistrationRecoveryStoreError::Unavailable)? = Some(record);
        Ok(())
    }
    fn load(
        &self,
    ) -> Result<
        Option<WalletDustRegistrationRecoveryRecord>,
        WalletDustRegistrationRecoveryStoreError,
    > {
        self.0
            .lock()
            .map(|value| value.clone())
            .map_err(|_| WalletDustRegistrationRecoveryStoreError::Unavailable)
    }

    fn clear(&self) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
        *self
            .0
            .lock()
            .map_err(|_| WalletDustRegistrationRecoveryStoreError::Unavailable)? = None;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletDustRegistrationRecoveryError {
    UnsupportedVersion { found: u16 },
    InvalidProfileIdentifier,
    InvalidRealmIdentifier,
    InvalidDraftIdentifier,
    InvalidTransactionIdentifier,
    ImpossibleState,
    MalformedEncoding,
}

impl WalletDustRegistrationRecoveryRecord {
    pub fn from_quiescent_checkpoint(
        checkpoint: &WalletDustRegistrationRuntimeCheckpoint,
    ) -> Result<Self, WalletDustRegistrationRecoveryError> {
        if checkpoint.version != WALLET_DUST_REGISTRATION_RUNTIME_CHECKPOINT_VERSION {
            return Err(WalletDustRegistrationRecoveryError::UnsupportedVersion {
                found: checkpoint.version,
            });
        }
        let projection = checkpoint.coordinator.projection();
        // Parked/recoverable lifecycle bookkeeping is deliberately not durable in v1.
        if !projection.is_exact_recovery_state()
            || !matches!(
                projection.state,
                State::ActionRequired
                    | State::NotEligible
                    | State::AwaitingAuthorization
                    | State::Submitting
                    | State::Confirming
                    | State::Reconciling
                    | State::Ready
            )
        {
            return Err(WalletDustRegistrationRecoveryError::ImpossibleState);
        }
        let identity = projection
            .identity
            .as_ref()
            .ok_or(WalletDustRegistrationRecoveryError::ImpossibleState)?;
        let stable = projection
            .checkpoint
            .as_ref()
            .ok_or(WalletDustRegistrationRecoveryError::ImpossibleState)?;
        if stable.identity != *identity {
            return Err(WalletDustRegistrationRecoveryError::ImpossibleState);
        }
        let record = Self {
            version: WALLET_DUST_REGISTRATION_RECOVERY_VERSION,
            profile_id: identity.profile.as_str().to_owned(),
            realm_id: identity.realm.as_str().to_owned(),
            generation: identity.generation,
            eligibility_revision: stable.revision,
            eligible: stable.eligible,
            preparation_revision: projection.preparation_revision,
            recovery_revision: projection.recovery_revision,
            state: projection.state,
            registration: projection.registration.as_ref().map(record_registration),
        };
        record.validate()?;
        Ok(record)
    }

    /// Injective, size-bounded binary format. Every string is length-prefixed.
    pub fn encode(&self) -> Result<Vec<u8>, WalletDustRegistrationRecoveryError> {
        self.validate()?;
        let mut output = Vec::with_capacity(128);
        output.extend_from_slice(MAGIC);
        put_u16(&mut output, self.version);
        put_u8(&mut output, state_code(self.state));
        put_bool(&mut output, self.eligible);
        for value in [
            self.generation,
            self.eligibility_revision,
            self.preparation_revision,
            self.recovery_revision,
        ] {
            put_u64(&mut output, value);
        }
        put_string(&mut output, &self.profile_id)?;
        put_string(&mut output, &self.realm_id)?;
        match &self.registration {
            None => put_u8(&mut output, 0),
            Some(value) => {
                put_u8(&mut output, 1);
                put_string(&mut output, &value.draft_id)?;
                put_u8(&mut output, phase_code(value.authorization_phase));
                match &value.transaction_id {
                    None => put_u8(&mut output, 0),
                    Some(id) => {
                        put_u8(&mut output, 1);
                        put_string(&mut output, id)?;
                    }
                }
                for n in [
                    value.observation_revision,
                    value.finality_revision,
                    value.reconciliation_revision,
                    value.dust_revision,
                    value.dust_observation_revision,
                    value.abandonment_revision,
                ] {
                    put_u64(&mut output, n);
                }
                put_bool(&mut output, value.dust_ready);
                put_bool(&mut output, value.included);
            }
        }
        if output.len() > MAX_RECOVERY_BYTES {
            return Err(WalletDustRegistrationRecoveryError::MalformedEncoding);
        }
        Ok(output)
    }

    pub fn decode(input: &[u8]) -> Result<Self, WalletDustRegistrationRecoveryError> {
        if input.len() > MAX_RECOVERY_BYTES {
            return Err(WalletDustRegistrationRecoveryError::MalformedEncoding);
        }
        let mut r = Reader::new(input);
        if r.bytes(4)? != MAGIC {
            return Err(WalletDustRegistrationRecoveryError::MalformedEncoding);
        }
        let version = r.u16()?;
        let state = state_from_code(r.u8()?)?;
        let eligible = r.bool()?;
        let generation = r.u64()?;
        let eligibility_revision = r.u64()?;
        let preparation_revision = r.u64()?;
        let recovery_revision = r.u64()?;
        let profile_id = r.string()?;
        let realm_id = r.string()?;
        let registration = match r.u8()? {
            0 => None,
            1 => {
                let draft_id = r.string()?;
                let authorization_phase = phase_from_code(r.u8()?)?;
                let transaction_id = match r.u8()? {
                    0 => None,
                    1 => Some(r.string()?),
                    _ => return Err(WalletDustRegistrationRecoveryError::MalformedEncoding),
                };
                Some(WalletDustRegistrationRecoveryRegistration {
                    draft_id,
                    authorization_phase,
                    transaction_id,
                    observation_revision: r.u64()?,
                    finality_revision: r.u64()?,
                    reconciliation_revision: r.u64()?,
                    dust_revision: r.u64()?,
                    dust_observation_revision: r.u64()?,
                    abandonment_revision: r.u64()?,
                    dust_ready: r.bool()?,
                    included: r.bool()?,
                })
            }
            _ => return Err(WalletDustRegistrationRecoveryError::MalformedEncoding),
        };
        if !r.finished() {
            return Err(WalletDustRegistrationRecoveryError::MalformedEncoding);
        }
        let record = Self {
            version,
            profile_id,
            realm_id,
            generation,
            eligibility_revision,
            eligible,
            preparation_revision,
            recovery_revision,
            state,
            registration,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), WalletDustRegistrationRecoveryError> {
        if self.version != WALLET_DUST_REGISTRATION_RECOVERY_VERSION {
            return Err(WalletDustRegistrationRecoveryError::UnsupportedVersion {
                found: self.version,
            });
        }
        WalletProfileId::parse(self.profile_id.clone())
            .map_err(|_| WalletDustRegistrationRecoveryError::InvalidProfileIdentifier)?;
        ChainNetworkId::parse(self.realm_id.clone())
            .map_err(|_| WalletDustRegistrationRecoveryError::InvalidRealmIdentifier)?;
        if let Some(value) = &self.registration {
            WalletTransactionDraftId::parse(value.draft_id.clone())
                .map_err(|_| WalletDustRegistrationRecoveryError::InvalidDraftIdentifier)?;
            if let Some(id) = &value.transaction_id {
                ChainTransactionId::parse(id.clone()).map_err(|_| {
                    WalletDustRegistrationRecoveryError::InvalidTransactionIdentifier
                })?;
            }
            if value.authorization_phase == AuthorizationPhase::Submitted
                && value.transaction_id.is_none()
            {
                return Err(WalletDustRegistrationRecoveryError::ImpossibleState);
            }
            if value.authorization_phase != AuthorizationPhase::Submitted
                && value.transaction_id.is_some()
            {
                return Err(WalletDustRegistrationRecoveryError::ImpossibleState);
            }
            if value.authorization_phase != AuthorizationPhase::Submitted
                && (value.observation_revision != 0
                    || value.finality_revision != 0
                    || value.reconciliation_revision != 0
                    || value.dust_revision != 0
                    || value.dust_observation_revision != 0
                    || value.dust_ready
                    || value.included
                    || value.abandonment_revision != 0)
            {
                return Err(WalletDustRegistrationRecoveryError::ImpossibleState);
            }
            if value.authorization_phase == AuthorizationPhase::Submitted {
                if value.observation_revision
                    != value.finality_revision.max(value.reconciliation_revision)
                    || value.dust_ready && value.dust_revision == 0
                    || value.included && value.reconciliation_revision == 0
                {
                    return Err(WalletDustRegistrationRecoveryError::ImpossibleState);
                }
            }
        }
        match self.state {
            State::AwaitingAuthorization => require_phase(
                &self.registration,
                AuthorizationPhase::AwaitingAuthorization,
            )
            .map(|_| ()),
            State::Submitting => {
                require_phase(&self.registration, AuthorizationPhase::Submitting).map(|_| ())
            }
            State::Confirming => {
                let registration =
                    require_phase(&self.registration, AuthorizationPhase::Submitted)?;
                if registration.included
                    || registration.abandonment_revision != 0
                    || (registration.observation_revision != 0
                        && registration.finality_revision >= registration.reconciliation_revision)
                {
                    Err(WalletDustRegistrationRecoveryError::ImpossibleState)
                } else {
                    Ok(())
                }
            }
            State::Reconciling => {
                let registration =
                    require_phase(&self.registration, AuthorizationPhase::Submitted)?;
                if registration.abandonment_revision != 0
                    || registration.dust_ready
                        && registration.included
                        && registration.dust_observation_revision
                            == registration.observation_revision
                    || !registration.included && registration.observation_revision == 0
                    || !registration.included
                        && registration.finality_revision < registration.reconciliation_revision
                {
                    Err(WalletDustRegistrationRecoveryError::ImpossibleState)
                } else {
                    Ok(())
                }
            }
            State::Ready => {
                let registration =
                    require_phase(&self.registration, AuthorizationPhase::Submitted)?;
                if registration.included
                    && registration.dust_ready
                    && registration.dust_observation_revision == registration.observation_revision
                    && registration.abandonment_revision == 0
                {
                    Ok(())
                } else {
                    Err(WalletDustRegistrationRecoveryError::ImpossibleState)
                }
            }
            State::ActionRequired => {
                if !self.eligible {
                    return Err(WalletDustRegistrationRecoveryError::ImpossibleState);
                }
                validate_idle_or_retained_submission(&self.registration)
            }
            State::NotEligible => {
                if self.eligible {
                    return Err(WalletDustRegistrationRecoveryError::ImpossibleState);
                }
                validate_idle_or_retained_submission(&self.registration)
            }
            _ => Err(WalletDustRegistrationRecoveryError::ImpossibleState),
        }
    }

    /// Restores the exact public projection through a crate-private constructor.
    pub fn restore_runtime(
        &self,
    ) -> Result<WalletDustRegistrationRuntime, WalletDustRegistrationRecoveryError> {
        self.validate()?;
        let identity = Identity {
            profile: WalletProfileId::parse(self.profile_id.clone())
                .map_err(|_| WalletDustRegistrationRecoveryError::InvalidProfileIdentifier)?,
            realm: ChainNetworkId::parse(self.realm_id.clone())
                .map_err(|_| WalletDustRegistrationRecoveryError::InvalidRealmIdentifier)?,
            generation: self.generation,
        };
        let registration = self
            .registration
            .as_ref()
            .map(parse_registration)
            .transpose()?;
        let projection = recovered_dust_registration_projection(
            self.state,
            identity.clone(),
            registration,
            Checkpoint {
                identity,
                revision: self.eligibility_revision,
                eligible: self.eligible,
            },
            self.preparation_revision,
            self.recovery_revision,
        );
        Ok(WalletDustRegistrationRuntime::from_recovered_coordinator(
            recovered_dust_registration_coordinator(projection),
        ))
    }
}

fn record_registration(value: &Registration) -> WalletDustRegistrationRecoveryRegistration {
    WalletDustRegistrationRecoveryRegistration {
        draft_id: value.draft_id.as_str().to_owned(),
        authorization_phase: value.authorization_phase,
        transaction_id: value
            .transaction_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        observation_revision: value.observation_revision,
        finality_revision: value.finality_revision,
        reconciliation_revision: value.reconciliation_revision,
        dust_revision: value.dust_revision,
        dust_observation_revision: value.dust_observation_revision,
        dust_ready: value.dust_ready,
        included: value.included,
        abandonment_revision: value.abandonment_revision,
    }
}
fn parse_registration(
    value: &WalletDustRegistrationRecoveryRegistration,
) -> Result<Registration, WalletDustRegistrationRecoveryError> {
    Ok(Registration {
        draft_id: WalletTransactionDraftId::parse(value.draft_id.clone())
            .map_err(|_| WalletDustRegistrationRecoveryError::InvalidDraftIdentifier)?,
        authorization_phase: value.authorization_phase,
        transaction_id: value
            .transaction_id
            .clone()
            .map(ChainTransactionId::parse)
            .transpose()
            .map_err(|_| WalletDustRegistrationRecoveryError::InvalidTransactionIdentifier)?,
        observation_revision: value.observation_revision,
        finality_revision: value.finality_revision,
        reconciliation_revision: value.reconciliation_revision,
        dust_revision: value.dust_revision,
        dust_observation_revision: value.dust_observation_revision,
        dust_ready: value.dust_ready,
        included: value.included,
        abandonment_revision: value.abandonment_revision,
    })
}
fn require_phase(
    value: &Option<WalletDustRegistrationRecoveryRegistration>,
    phase: AuthorizationPhase,
) -> Result<&WalletDustRegistrationRecoveryRegistration, WalletDustRegistrationRecoveryError> {
    value
        .as_ref()
        .filter(|value| value.authorization_phase == phase)
        .ok_or(WalletDustRegistrationRecoveryError::ImpossibleState)
}
fn validate_idle_or_retained_submission(
    value: &Option<WalletDustRegistrationRecoveryRegistration>,
) -> Result<(), WalletDustRegistrationRecoveryError> {
    match value {
        None => Ok(()),
        Some(value)
            if value.authorization_phase == AuthorizationPhase::Submitted
                && value.abandonment_revision == 0 =>
        {
            Ok(())
        }
        Some(_) => Err(WalletDustRegistrationRecoveryError::ImpossibleState),
    }
}
fn put_u8(out: &mut Vec<u8>, n: u8) {
    out.push(n);
}
fn put_bool(out: &mut Vec<u8>, b: bool) {
    put_u8(out, u8::from(b));
}
fn put_u16(out: &mut Vec<u8>, n: u16) {
    out.extend_from_slice(&n.to_be_bytes());
}
fn put_u64(out: &mut Vec<u8>, n: u64) {
    out.extend_from_slice(&n.to_be_bytes());
}
fn put_string(out: &mut Vec<u8>, s: &str) -> Result<(), WalletDustRegistrationRecoveryError> {
    let n = u16::try_from(s.len())
        .map_err(|_| WalletDustRegistrationRecoveryError::MalformedEncoding)?;
    put_u16(out, n);
    out.extend_from_slice(s.as_bytes());
    Ok(())
}
struct Reader<'a> {
    input: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, at: 0 }
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], WalletDustRegistrationRecoveryError> {
        let end = self
            .at
            .checked_add(n)
            .ok_or(WalletDustRegistrationRecoveryError::MalformedEncoding)?;
        let value = self
            .input
            .get(self.at..end)
            .ok_or(WalletDustRegistrationRecoveryError::MalformedEncoding)?;
        self.at = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, WalletDustRegistrationRecoveryError> {
        Ok(self.bytes(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, WalletDustRegistrationRecoveryError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WalletDustRegistrationRecoveryError::MalformedEncoding),
        }
    }
    fn u16(&mut self) -> Result<u16, WalletDustRegistrationRecoveryError> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, WalletDustRegistrationRecoveryError> {
        Ok(u64::from_be_bytes(self.bytes(8)?.try_into().unwrap()))
    }
    fn string(&mut self) -> Result<String, WalletDustRegistrationRecoveryError> {
        let length = usize::from(self.u16()?);
        let raw = self.bytes(length)?;
        String::from_utf8(raw.to_vec())
            .map_err(|_| WalletDustRegistrationRecoveryError::MalformedEncoding)
    }
    fn finished(&self) -> bool {
        self.at == self.input.len()
    }
}
const fn phase_code(v: AuthorizationPhase) -> u8 {
    match v {
        AuthorizationPhase::AwaitingAuthorization => 0,
        AuthorizationPhase::Submitting => 1,
        AuthorizationPhase::Submitted => 2,
    }
}
fn phase_from_code(v: u8) -> Result<AuthorizationPhase, WalletDustRegistrationRecoveryError> {
    match v {
        0 => Ok(AuthorizationPhase::AwaitingAuthorization),
        1 => Ok(AuthorizationPhase::Submitting),
        2 => Ok(AuthorizationPhase::Submitted),
        _ => Err(WalletDustRegistrationRecoveryError::MalformedEncoding),
    }
}
const fn state_code(v: State) -> u8 {
    match v {
        State::Unavailable => 0,
        State::NotEligible => 1,
        State::ActionRequired => 2,
        State::AwaitingAuthorization => 3,
        State::Submitting => 4,
        State::Confirming => 5,
        State::Reconciling => 6,
        State::Ready => 7,
        State::Cancelled => 8,
        State::Offline => 9,
        State::TimedOut => 10,
        State::Degraded => 11,
        State::Suspended => 12,
    }
}
fn state_from_code(v: u8) -> Result<State, WalletDustRegistrationRecoveryError> {
    match v {
        0 => Ok(State::Unavailable),
        1 => Ok(State::NotEligible),
        2 => Ok(State::ActionRequired),
        3 => Ok(State::AwaitingAuthorization),
        4 => Ok(State::Submitting),
        5 => Ok(State::Confirming),
        6 => Ok(State::Reconciling),
        7 => Ok(State::Ready),
        8 => Ok(State::Cancelled),
        9 => Ok(State::Offline),
        10 => Ok(State::TimedOut),
        11 => Ok(State::Degraded),
        12 => Ok(State::Suspended),
        _ => Err(WalletDustRegistrationRecoveryError::MalformedEncoding),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        WalletDustRegistrationRuntimeAdmission, WalletDustRegistrationSettlementEvent, completion,
    };

    #[derive(Default)]
    struct DurableTestStore(Mutex<Option<Vec<u8>>>);

    impl WalletDustRegistrationRecoveryStore for DurableTestStore {
        fn save(
            &self,
            record: WalletDustRegistrationRecoveryRecord,
        ) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
            let encoded = record
                .encode()
                .map_err(|_| WalletDustRegistrationRecoveryStoreError::Corrupt)?;
            *self
                .0
                .lock()
                .map_err(|_| WalletDustRegistrationRecoveryStoreError::Unavailable)? =
                Some(encoded);
            Ok(())
        }

        fn load(
            &self,
        ) -> Result<
            Option<WalletDustRegistrationRecoveryRecord>,
            WalletDustRegistrationRecoveryStoreError,
        > {
            self.0
                .lock()
                .map_err(|_| WalletDustRegistrationRecoveryStoreError::Unavailable)?
                .as_deref()
                .map(|encoded| {
                    WalletDustRegistrationRecoveryRecord::decode(encoded)
                        .map_err(|_| WalletDustRegistrationRecoveryStoreError::Corrupt)
                })
                .transpose()
        }

        fn clear(&self) -> Result<(), WalletDustRegistrationRecoveryStoreError> {
            *self
                .0
                .lock()
                .map_err(|_| WalletDustRegistrationRecoveryStoreError::Unavailable)? = None;
            Ok(())
        }
    }

    fn submitted(
        state: State,
        eligible: bool,
        included: bool,
        ready: bool,
    ) -> WalletDustRegistrationRecoveryRecord {
        WalletDustRegistrationRecoveryRecord {
            version: 1,
            profile_id: "profile_test".to_owned(),
            realm_id: "undeployed".to_owned(),
            generation: 7,
            eligibility_revision: 11,
            eligible,
            preparation_revision: 13,
            recovery_revision: 17,
            state,
            registration: Some(WalletDustRegistrationRecoveryRegistration {
                draft_id: "dustreg_test".to_owned(),
                authorization_phase: AuthorizationPhase::Submitted,
                transaction_id: Some("tx_test".to_owned()),
                observation_revision: 29,
                finality_revision: 29,
                reconciliation_revision: 23,
                dust_revision: 31,
                dust_observation_revision: 29,
                dust_ready: ready,
                included,
                abandonment_revision: 0,
            }),
        }
    }

    #[test]
    fn restart_preserves_nonzero_revisions_and_ready_finality_after_reconciliation() {
        let identity = Identity {
            profile: WalletProfileId::parse("profile_test").unwrap(),
            realm: ChainNetworkId::parse("undeployed").unwrap(),
            generation: 7,
        };
        let draft = WalletTransactionDraftId::parse("dustreg_test").unwrap();
        let transaction = ChainTransactionId::parse("tx_test").unwrap();
        let mut original = WalletDustRegistrationRuntime::default();
        original.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity: identity.clone(),
            revision: 1,
            eligible: true,
        });
        original.observe(completion::prepared(identity.clone(), draft.clone(), 13));
        original.observe(completion::authorized(identity.clone(), draft.clone()));
        original.observe(completion::submitted(
            identity.clone(),
            draft,
            transaction.clone(),
        ));
        original.observe(completion::finality_observed(
            identity.clone(),
            transaction.clone(),
            29,
        ));
        original.observe(completion::reconciled(
            identity.clone(),
            transaction.clone(),
            23,
            crate::WalletDustRegistrationSettlementReconciliation::Included,
        ));
        original.observe(completion::dust_refreshed(
            identity.clone(),
            transaction,
            31,
            29,
            true,
        ));
        original.observe(WalletDustRegistrationSettlementEvent::Eligibility {
            identity,
            revision: 11,
            eligible: false,
        });
        let record = WalletDustRegistrationRecoveryRecord::from_quiescent_checkpoint(
            &original.quiescent_checkpoint().unwrap(),
        )
        .unwrap();
        let store = DurableTestStore::default();
        store.save(record).unwrap();
        let restored = store.load().unwrap().unwrap().restore_runtime().unwrap();
        assert_eq!(
            restored.coordinator().projection(),
            original.coordinator().projection()
        );
        let registration = restored
            .coordinator()
            .projection()
            .registration
            .as_ref()
            .unwrap();
        assert_eq!(registration.finality_revision, 29);
        assert_eq!(registration.reconciliation_revision, 23);
        assert_eq!(registration.dust_observation_revision, 29);
        assert_eq!(registration.abandonment_revision, 0);
        assert!(
            !restored
                .coordinator()
                .projection()
                .checkpoint
                .as_ref()
                .unwrap()
                .eligible
        );
    }

    #[test]
    fn supports_submitted_unknown_and_retained_action_states_and_rejects_encoding_trailing_data() {
        let mut confirming = submitted(State::Confirming, false, false, false);
        let confirming_registration = confirming.registration.as_mut().unwrap();
        confirming_registration.observation_revision = 0;
        confirming_registration.finality_revision = 0;
        confirming_registration.reconciliation_revision = 0;
        confirming_registration.dust_revision = 0;
        confirming_registration.dust_observation_revision = 0;
        for record in [
            confirming,
            submitted(State::Reconciling, false, false, false),
            submitted(State::Reconciling, false, true, false),
        ] {
            let state = record.state;
            assert_eq!(
                record
                    .restore_runtime()
                    .unwrap()
                    .coordinator()
                    .projection()
                    .state,
                state
            );
        }
        let record = submitted(State::ActionRequired, true, false, false);
        assert_eq!(
            record
                .restore_runtime()
                .unwrap()
                .coordinator()
                .projection()
                .state,
            State::ActionRequired
        );
        let mut encoded = submitted(State::Ready, true, true, true).encode().unwrap();
        encoded.push(0);
        assert_eq!(
            WalletDustRegistrationRecoveryRecord::decode(&encoded),
            Err(WalletDustRegistrationRecoveryError::MalformedEncoding)
        );

        let mut impossible = submitted(State::Ready, true, false, false);
        assert_eq!(
            impossible.validate(),
            Err(WalletDustRegistrationRecoveryError::ImpossibleState)
        );
        impossible.state = State::ActionRequired;
        impossible.eligible = false;
        assert_eq!(
            impossible.validate(),
            Err(WalletDustRegistrationRecoveryError::ImpossibleState)
        );
    }

    #[test]
    fn restored_runtime_rejects_an_admission_token_from_the_prior_instance() {
        let record = submitted(State::Reconciling, true, true, false);
        let mut before_restart = record.restore_runtime().unwrap();
        let effect = before_restart
            .coordinator()
            .active_effect()
            .unwrap()
            .clone();
        let token = match before_restart.admit_current(&effect) {
            WalletDustRegistrationRuntimeAdmission::Admitted { token, .. } => token,
            _ => panic!("expected admission"),
        };
        let mut after_restart = record.restore_runtime().unwrap();
        assert!(!after_restart.complete(
            token,
            WalletDustRegistrationSettlementEvent::Eligibility {
                identity: Identity {
                    profile: WalletProfileId::parse("profile_test").unwrap(),
                    realm: ChainNetworkId::parse("undeployed").unwrap(),
                    generation: 7,
                },
                revision: 99,
                eligible: true,
            },
        ));
    }

    #[test]
    fn unsupported_checkpoint_versions_fail_and_stale_store_records_can_be_cleared() {
        let runtime = submitted(State::Reconciling, true, true, false)
            .restore_runtime()
            .unwrap();
        let mut checkpoint = runtime.quiescent_checkpoint().unwrap();
        checkpoint.version += 1;
        assert!(matches!(
            WalletDustRegistrationRecoveryRecord::from_quiescent_checkpoint(&checkpoint),
            Err(WalletDustRegistrationRecoveryError::UnsupportedVersion { .. })
        ));

        let store = DurableTestStore::default();
        store
            .save(submitted(State::Reconciling, true, true, false))
            .unwrap();
        assert!(store.load().unwrap().is_some());
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }
}
