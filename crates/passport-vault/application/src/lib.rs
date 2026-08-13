// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use oxid_passport_vault_domain::{
    CredentialFingerprint, PassportVaultError, PassportVaultLock, PassportVaultPolicy,
    PassportVaultState, VaultActorId, VaultLockId,
};
use oxid_platform_ports::{PlatformError, RandomPort};

pub type PassportVaultClaimFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PassportVaultClaimView, PassportVaultOperationError>>
            + Send
            + 'a,
    >,
>;
pub type PassportVaultEvidenceFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<VerifiedPassportVaultCredential, PassportVaultCredentialError>>
            + Send
            + 'a,
    >,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultRepositoryError {
    Unavailable,
    Integrity,
}

impl fmt::Display for PassportVaultRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "passport vault state is unavailable",
            Self::Integrity => "passport vault state failed integrity validation",
        })
    }
}
impl Error for PassportVaultRepositoryError {}

pub trait PassportVaultRepository: Send + Sync {
    fn load(&self) -> Result<PassportVaultState, PassportVaultRepositoryError>;
    fn save(&self, state: &PassportVaultState) -> Result<(), PassportVaultRepositoryError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassportVaultCredentialError {
    Unavailable,
    NotFound,
    Invalid,
    MissingPrivateMaterial,
    IssuerNotTrusted,
    Expired,
    AgeRequirementNotMet,
    IssuingStateMismatch,
    DocumentNumberMismatch,
}

impl fmt::Display for PassportVaultCredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "passport credential verification is unavailable",
            Self::NotFound => "passport credential was not found",
            Self::Invalid => "passport credential is invalid",
            Self::MissingPrivateMaterial => "passport credential has no protected claim material",
            Self::IssuerNotTrusted => "passport credential issuer is not trusted by this vault",
            Self::Expired => "passport credential has expired",
            Self::AgeRequirementNotMet => "passport credential does not satisfy the minimum age",
            Self::IssuingStateMismatch => "passport issuing state does not satisfy the lock policy",
            Self::DocumentNumberMismatch => {
                "passport document number does not satisfy the lock policy"
            }
        })
    }
}
impl Error for PassportVaultCredentialError {}

#[derive(Clone, PartialEq, Eq)]
pub struct VerifyPassportVaultCredentialRequest {
    pub profile_id: String,
    pub credential_id: String,
    pub policy: PassportVaultPolicy,
}

impl fmt::Debug for VerifyPassportVaultCredentialRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifyPassportVaultCredentialRequest")
            .field("profile_id", &self.profile_id)
            .field("credential_id", &self.credential_id)
            .field("minimum_age_years", &self.policy.minimum_age_years())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifiedPassportVaultCredential {
    pub credential_fingerprint: [u8; 32],
    pub current_day: u32,
}

pub trait PassportVaultCredentialPort: Send + Sync {
    fn verify<'a>(
        &'a self,
        request: VerifyPassportVaultCredentialRequest,
    ) -> PassportVaultEvidenceFuture<'a>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PassportVaultConfirmation {
    pub confirmed: bool,
    pub intent: &'static str,
}

pub const CREATE_LOCK_INTENT: &str = "CREATE_PASSPORT_VAULT_LOCK";
pub const DEPOSIT_INTENT: &str = "DEPOSIT_TO_PASSPORT_VAULT";
pub const CLAIM_INTENT: &str = "CLAIM_FROM_PASSPORT_VAULT";
pub const WITHDRAW_INTENT: &str = "WITHDRAW_FROM_PASSPORT_VAULT";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatePassportVaultLockCommand {
    pub profile_id: String,
    pub minimum_age_years: u8,
    pub required_issuing_state: Option<[u8; 32]>,
    pub required_document_number: Option<[u8; 32]>,
    pub maximum_claim_amount: u128,
    pub initial_amount: u128,
    pub confirmed: bool,
    pub intent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultAmountCommand {
    pub profile_id: String,
    pub lock_id: u64,
    pub amount: u128,
    pub confirmed: bool,
    pub intent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimPassportVaultLockCommand {
    pub profile_id: String,
    pub lock_id: u64,
    pub credential_id: String,
    pub amount: u128,
    pub confirmed: bool,
    pub intent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultLockView {
    pub lock_id: u64,
    pub creator_profile_id: String,
    pub minimum_age_years: u8,
    pub required_issuing_state: Option<String>,
    pub required_document_number: Option<String>,
    pub maximum_claim_amount: String,
    pub total_deposited: String,
    pub total_released: String,
    pub remaining: String,
    pub verifier_challenge_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultView {
    pub source: String,
    pub locks: Vec<PassportVaultLockView>,
    pub total_deposited: String,
    pub total_released: String,
    pub total_locked: String,
    pub claim_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassportVaultClaimView {
    pub lock: PassportVaultLockView,
    pub released_amount: String,
    pub current_day: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassportVaultOperationError {
    Domain(PassportVaultError),
    Repository(PassportVaultRepositoryError),
    Credential(PassportVaultCredentialError),
    Platform(PlatformError),
    ConfirmationRequired,
    InvalidConfirmation,
    PolicyChanged,
}

impl fmt::Display for PassportVaultOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(formatter),
            Self::Repository(error) => error.fmt(formatter),
            Self::Credential(error) => error.fmt(formatter),
            Self::Platform(error) => error.fmt(formatter),
            Self::ConfirmationRequired => {
                formatter.write_str("passport vault operation requires explicit confirmation")
            }
            Self::InvalidConfirmation => {
                formatter.write_str("passport vault confirmation intent is invalid")
            }
            Self::PolicyChanged => {
                formatter.write_str("passport vault policy changed during credential verification")
            }
        }
    }
}
impl Error for PassportVaultOperationError {}

pub trait ListPassportVaultLocksUseCase: Send + Sync {
    fn execute(&self) -> Result<PassportVaultView, PassportVaultOperationError>;
}
pub trait CreatePassportVaultLockUseCase: Send + Sync {
    fn execute(
        &self,
        command: CreatePassportVaultLockCommand,
    ) -> Result<PassportVaultLockView, PassportVaultOperationError>;
}
pub trait DepositPassportVaultLockUseCase: Send + Sync {
    fn execute(
        &self,
        command: PassportVaultAmountCommand,
    ) -> Result<PassportVaultLockView, PassportVaultOperationError>;
}
pub trait WithdrawPassportVaultLockUseCase: Send + Sync {
    fn execute(
        &self,
        command: PassportVaultAmountCommand,
    ) -> Result<PassportVaultLockView, PassportVaultOperationError>;
}
pub trait ClaimPassportVaultLockUseCase: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: ClaimPassportVaultLockCommand,
    ) -> PassportVaultClaimFuture<'a>;
}

pub struct PassportVaultService {
    repository: Arc<dyn PassportVaultRepository>,
    credential: Arc<dyn PassportVaultCredentialPort>,
    random: Arc<dyn RandomPort>,
    transaction: Mutex<()>,
}

impl PassportVaultService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn PassportVaultRepository>,
        credential: Arc<dyn PassportVaultCredentialPort>,
        random: Arc<dyn RandomPort>,
    ) -> Self {
        Self {
            repository,
            credential,
            random,
            transaction: Mutex::new(()),
        }
    }
}

fn confirmation(
    confirmed: bool,
    actual: &str,
    expected: &str,
) -> Result<(), PassportVaultOperationError> {
    if !confirmed {
        return Err(PassportVaultOperationError::ConfirmationRequired);
    }
    if actual != expected {
        return Err(PassportVaultOperationError::InvalidConfirmation);
    }
    Ok(())
}

fn text32(value: Option<[u8; 32]>) -> Option<String> {
    value.map(|bytes| {
        let end = bytes
            .iter()
            .rposition(|byte| *byte != 0)
            .map_or(0, |index| index + 1);
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    })
}

fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(TABLE[usize::from(byte >> 4)]));
        output.push(char::from(TABLE[usize::from(byte & 0x0f)]));
    }
    output
}

fn lock_view(lock: &PassportVaultLock) -> PassportVaultLockView {
    PassportVaultLockView {
        lock_id: lock.id().value(),
        creator_profile_id: lock.creator().as_str().to_owned(),
        minimum_age_years: lock.policy().minimum_age_years(),
        required_issuing_state: text32(lock.policy().required_issuing_state()),
        required_document_number: text32(lock.policy().required_document_number()),
        maximum_claim_amount: lock.policy().maximum_claim_amount().to_string(),
        total_deposited: lock.total_deposited().to_string(),
        total_released: lock.total_released().to_string(),
        remaining: lock.remaining().to_string(),
        verifier_challenge_hex: hex(&lock.policy().verifier_challenge_hash()),
    }
}

impl ListPassportVaultLocksUseCase for PassportVaultService {
    fn execute(&self) -> Result<PassportVaultView, PassportVaultOperationError> {
        let state = self
            .repository
            .load()
            .map_err(PassportVaultOperationError::Repository)?;
        Ok(PassportVaultView {
            source: "standalone".to_owned(),
            locks: state.locks().map(lock_view).collect(),
            total_deposited: state.total_deposited().to_string(),
            total_released: state.total_released().to_string(),
            total_locked: state.total_locked().to_string(),
            claim_count: state.claim_count(),
        })
    }
}

impl CreatePassportVaultLockUseCase for PassportVaultService {
    fn execute(
        &self,
        command: CreatePassportVaultLockCommand,
    ) -> Result<PassportVaultLockView, PassportVaultOperationError> {
        confirmation(command.confirmed, &command.intent, CREATE_LOCK_INTENT)?;
        let actor =
            VaultActorId::parse(command.profile_id).map_err(PassportVaultOperationError::Domain)?;
        let mut challenge = [0_u8; 32];
        self.random
            .fill_bytes(&mut challenge)
            .map_err(PassportVaultOperationError::Platform)?;
        if challenge == [0; 32] {
            challenge[0] = 1;
        }
        let policy = PassportVaultPolicy::new(
            command.minimum_age_years,
            command.required_issuing_state,
            command.required_document_number,
            command.maximum_claim_amount,
            challenge,
        )
        .map_err(PassportVaultOperationError::Domain)?;
        let _guard = self.transaction.lock().map_err(|_| {
            PassportVaultOperationError::Repository(PassportVaultRepositoryError::Unavailable)
        })?;
        let mut state = self
            .repository
            .load()
            .map_err(PassportVaultOperationError::Repository)?;
        let id = state
            .create_lock(actor, policy, command.initial_amount)
            .map_err(PassportVaultOperationError::Domain)?;
        let view = state
            .lock(id)
            .map(lock_view)
            .ok_or(PassportVaultOperationError::Repository(
                PassportVaultRepositoryError::Integrity,
            ))?;
        self.repository
            .save(&state)
            .map_err(PassportVaultOperationError::Repository)?;
        Ok(view)
    }
}

impl DepositPassportVaultLockUseCase for PassportVaultService {
    fn execute(
        &self,
        command: PassportVaultAmountCommand,
    ) -> Result<PassportVaultLockView, PassportVaultOperationError> {
        confirmation(command.confirmed, &command.intent, DEPOSIT_INTENT)?;
        mutate_amount(self, command, |state, actor, id, amount| {
            state.deposit(actor, id, amount)
        })
    }
}

impl WithdrawPassportVaultLockUseCase for PassportVaultService {
    fn execute(
        &self,
        command: PassportVaultAmountCommand,
    ) -> Result<PassportVaultLockView, PassportVaultOperationError> {
        confirmation(command.confirmed, &command.intent, WITHDRAW_INTENT)?;
        mutate_amount(self, command, |state, actor, id, amount| {
            state.withdraw(actor, id, amount).map(|_| ())
        })
    }
}

fn mutate_amount(
    service: &PassportVaultService,
    command: PassportVaultAmountCommand,
    operation: impl FnOnce(
        &mut PassportVaultState,
        &VaultActorId,
        VaultLockId,
        u128,
    ) -> Result<(), PassportVaultError>,
) -> Result<PassportVaultLockView, PassportVaultOperationError> {
    let actor =
        VaultActorId::parse(command.profile_id).map_err(PassportVaultOperationError::Domain)?;
    let id = VaultLockId::new(command.lock_id);
    let _guard = service.transaction.lock().map_err(|_| {
        PassportVaultOperationError::Repository(PassportVaultRepositoryError::Unavailable)
    })?;
    let mut state = service
        .repository
        .load()
        .map_err(PassportVaultOperationError::Repository)?;
    operation(&mut state, &actor, id, command.amount)
        .map_err(PassportVaultOperationError::Domain)?;
    let view = state
        .lock(id)
        .map(lock_view)
        .ok_or(PassportVaultOperationError::Repository(
            PassportVaultRepositoryError::Integrity,
        ))?;
    service
        .repository
        .save(&state)
        .map_err(PassportVaultOperationError::Repository)?;
    Ok(view)
}

impl ClaimPassportVaultLockUseCase for PassportVaultService {
    fn execute<'a>(
        &'a self,
        command: ClaimPassportVaultLockCommand,
    ) -> PassportVaultClaimFuture<'a> {
        Box::pin(async move {
            confirmation(command.confirmed, &command.intent, CLAIM_INTENT)?;
            VaultActorId::parse(&command.profile_id)
                .map_err(PassportVaultOperationError::Domain)?;
            let id = VaultLockId::new(command.lock_id);
            let policy = self
                .repository
                .load()
                .map_err(PassportVaultOperationError::Repository)?
                .lock(id)
                .map(|lock| lock.policy().clone())
                .ok_or(PassportVaultOperationError::Domain(
                    PassportVaultError::LockNotFound,
                ))?;
            let evidence = self
                .credential
                .verify(VerifyPassportVaultCredentialRequest {
                    profile_id: command.profile_id,
                    credential_id: command.credential_id,
                    policy: policy.clone(),
                })
                .await
                .map_err(PassportVaultOperationError::Credential)?;
            let fingerprint = CredentialFingerprint::new(evidence.credential_fingerprint)
                .map_err(PassportVaultOperationError::Domain)?;
            let _guard = self.transaction.lock().map_err(|_| {
                PassportVaultOperationError::Repository(PassportVaultRepositoryError::Unavailable)
            })?;
            let mut state = self
                .repository
                .load()
                .map_err(PassportVaultOperationError::Repository)?;
            if state.lock(id).map(PassportVaultLock::policy) != Some(&policy) {
                return Err(PassportVaultOperationError::PolicyChanged);
            }
            let receipt = state
                .claim(id, fingerprint, command.amount, evidence.current_day)
                .map_err(PassportVaultOperationError::Domain)?;
            let lock =
                state
                    .lock(id)
                    .map(lock_view)
                    .ok_or(PassportVaultOperationError::Repository(
                        PassportVaultRepositoryError::Integrity,
                    ))?;
            self.repository
                .save(&state)
                .map_err(PassportVaultOperationError::Repository)?;
            Ok(PassportVaultClaimView {
                lock,
                released_amount: receipt.amount.to_string(),
                current_day: receipt.current_day,
            })
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailablePassportVaultRepository;
impl PassportVaultRepository for UnavailablePassportVaultRepository {
    fn load(&self) -> Result<PassportVaultState, PassportVaultRepositoryError> {
        Err(PassportVaultRepositoryError::Unavailable)
    }
    fn save(&self, _: &PassportVaultState) -> Result<(), PassportVaultRepositoryError> {
        Err(PassportVaultRepositoryError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailablePassportVaultCredential;
impl PassportVaultCredentialPort for UnavailablePassportVaultCredential {
    fn verify<'a>(
        &'a self,
        _: VerifyPassportVaultCredentialRequest,
    ) -> PassportVaultEvidenceFuture<'a> {
        Box::pin(async { Err(PassportVaultCredentialError::Unavailable) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll, Waker};

    #[derive(Default)]
    struct Repository(Mutex<PassportVaultState>);

    impl PassportVaultRepository for Repository {
        fn load(&self) -> Result<PassportVaultState, PassportVaultRepositoryError> {
            Ok(self.0.lock().expect("repository").clone())
        }

        fn save(&self, state: &PassportVaultState) -> Result<(), PassportVaultRepositoryError> {
            *self.0.lock().expect("repository") = state.clone();
            Ok(())
        }
    }

    struct Random {
        byte: u8,
        unavailable: bool,
    }

    impl RandomPort for Random {
        fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), PlatformError> {
            if self.unavailable {
                return Err(PlatformError::RandomnessUnavailable);
            }
            destination.fill(self.byte);
            Ok(())
        }
    }

    struct Credential(Result<VerifiedPassportVaultCredential, PassportVaultCredentialError>);

    impl PassportVaultCredentialPort for Credential {
        fn verify<'a>(
            &'a self,
            _: VerifyPassportVaultCredentialRequest,
        ) -> PassportVaultEvidenceFuture<'a> {
            let result = self.0;
            Box::pin(async move { result })
        }
    }

    fn ready<T>(mut future: Pin<Box<dyn Future<Output = T> + Send + '_>>) -> T {
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }

    fn service(
        repository: Arc<dyn PassportVaultRepository>,
        credential: Arc<dyn PassportVaultCredentialPort>,
        random: Arc<dyn RandomPort>,
    ) -> PassportVaultService {
        PassportVaultService::new(repository, credential, random)
    }

    fn create_command() -> CreatePassportVaultLockCommand {
        CreatePassportVaultLockCommand {
            profile_id: "profile_creator".to_owned(),
            minimum_age_years: 18,
            required_issuing_state: Some(
                *b"US\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            ),
            required_document_number: None,
            maximum_claim_amount: 40,
            initial_amount: 100,
            confirmed: true,
            intent: CREATE_LOCK_INTENT.to_owned(),
        }
    }

    #[test]
    fn executes_the_complete_public_vault_use_case_flow() {
        let repository: Arc<dyn PassportVaultRepository> = Arc::new(Repository::default());
        let credential: Arc<dyn PassportVaultCredentialPort> =
            Arc::new(Credential(Ok(VerifiedPassportVaultCredential {
                credential_fingerprint: [9; 32],
                current_day: 20_000,
            })));
        let vault = service(
            Arc::clone(&repository),
            credential,
            Arc::new(Random {
                byte: 0,
                unavailable: false,
            }),
        );

        let created =
            CreatePassportVaultLockUseCase::execute(&vault, create_command()).expect("create");
        assert_eq!(created.lock_id, 0);
        assert_eq!(created.required_issuing_state.as_deref(), Some("US"));
        assert_eq!(
            created.verifier_challenge_hex,
            format!("01{}", "00".repeat(31))
        );

        DepositPassportVaultLockUseCase::execute(
            &vault,
            PassportVaultAmountCommand {
                profile_id: "profile_creator".to_owned(),
                lock_id: 0,
                amount: 20,
                confirmed: true,
                intent: DEPOSIT_INTENT.to_owned(),
            },
        )
        .expect("deposit");
        assert_eq!(
            DepositPassportVaultLockUseCase::execute(
                &vault,
                PassportVaultAmountCommand {
                    profile_id: "profile_other".to_owned(),
                    lock_id: 0,
                    amount: 1,
                    confirmed: true,
                    intent: DEPOSIT_INTENT.to_owned(),
                },
            ),
            Err(PassportVaultOperationError::Domain(
                PassportVaultError::NotLockCreator
            ))
        );

        let claimed = ready(ClaimPassportVaultLockUseCase::execute(
            &vault,
            ClaimPassportVaultLockCommand {
                profile_id: "profile_holder".to_owned(),
                lock_id: 0,
                credential_id: "credential_passport".to_owned(),
                amount: 40,
                confirmed: true,
                intent: CLAIM_INTENT.to_owned(),
            },
        ))
        .expect("claim");
        assert_eq!(claimed.released_amount, "40");
        assert_eq!(claimed.current_day, 20_000);
        assert_eq!(claimed.lock.remaining, "80");

        assert_eq!(
            ready(ClaimPassportVaultLockUseCase::execute(
                &vault,
                ClaimPassportVaultLockCommand {
                    profile_id: "profile_holder".to_owned(),
                    lock_id: 0,
                    credential_id: "credential_passport".to_owned(),
                    amount: 1,
                    confirmed: true,
                    intent: CLAIM_INTENT.to_owned(),
                },
            )),
            Err(PassportVaultOperationError::Domain(
                PassportVaultError::CredentialAlreadyClaimed
            ))
        );

        WithdrawPassportVaultLockUseCase::execute(
            &vault,
            PassportVaultAmountCommand {
                profile_id: "profile_creator".to_owned(),
                lock_id: 0,
                amount: 80,
                confirmed: true,
                intent: WITHDRAW_INTENT.to_owned(),
            },
        )
        .expect("withdraw");
        let view = ListPassportVaultLocksUseCase::execute(&vault).expect("list");
        assert_eq!(view.source, "standalone");
        assert_eq!(view.total_deposited, "120");
        assert_eq!(view.total_released, "120");
        assert_eq!(view.total_locked, "0");
        assert_eq!(view.claim_count, 1);
    }

    #[test]
    fn confirmations_and_unavailable_ports_fail_closed() {
        let repository: Arc<dyn PassportVaultRepository> = Arc::new(Repository::default());
        let vault = service(
            Arc::clone(&repository),
            Arc::new(UnavailablePassportVaultCredential),
            Arc::new(Random {
                byte: 7,
                unavailable: false,
            }),
        );
        let mut command = create_command();
        command.confirmed = false;
        assert_eq!(
            CreatePassportVaultLockUseCase::execute(&vault, command),
            Err(PassportVaultOperationError::ConfirmationRequired)
        );
        let mut command = create_command();
        command.intent = "wrong".to_owned();
        assert_eq!(
            CreatePassportVaultLockUseCase::execute(&vault, command),
            Err(PassportVaultOperationError::InvalidConfirmation)
        );
        CreatePassportVaultLockUseCase::execute(&vault, create_command()).expect("create");
        assert_eq!(
            ready(ClaimPassportVaultLockUseCase::execute(
                &vault,
                ClaimPassportVaultLockCommand {
                    profile_id: "profile_holder".to_owned(),
                    lock_id: 0,
                    credential_id: "credential_passport".to_owned(),
                    amount: 1,
                    confirmed: true,
                    intent: CLAIM_INTENT.to_owned(),
                },
            )),
            Err(PassportVaultOperationError::Credential(
                PassportVaultCredentialError::Unavailable
            ))
        );

        let no_random = service(
            repository,
            Arc::new(UnavailablePassportVaultCredential),
            Arc::new(Random {
                byte: 0,
                unavailable: true,
            }),
        );
        assert_eq!(
            CreatePassportVaultLockUseCase::execute(&no_random, create_command()),
            Err(PassportVaultOperationError::Platform(
                PlatformError::RandomnessUnavailable
            ))
        );

        let unavailable = service(
            Arc::new(UnavailablePassportVaultRepository),
            Arc::new(UnavailablePassportVaultCredential),
            Arc::new(Random {
                byte: 1,
                unavailable: false,
            }),
        );
        assert_eq!(
            ListPassportVaultLocksUseCase::execute(&unavailable),
            Err(PassportVaultOperationError::Repository(
                PassportVaultRepositoryError::Unavailable
            ))
        );
    }
}
