// SPDX-License-Identifier: Apache-2.0

use oxid_adapter_platform_system::SystemSha256;
use oxid_foundation::UnixTimestampMillis;
use oxid_platform_ports::{ClockPort, PlatformError};
use oxid_wallet_application::*;
use oxid_wallet_domain::{
    WalletKeyAlgorithm, WalletKeyDescriptor, WalletKeyReference, WalletProfileId, WalletSignature,
};
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

#[test]
fn protection_and_profile_transitions_invalidate_even_when_adapter_fails() {
    struct Protection;
    impl WalletProtectionPort for Protection {
        fn status(
            &self,
            _: &WalletProfileId,
        ) -> Result<oxid_wallet_domain::WalletSecurityStatus, WalletSecurityPortError> {
            Err(WalletSecurityPortError::Unavailable)
        }
        fn initialize(
            &self,
            p: &WalletProfileId,
        ) -> Result<oxid_wallet_domain::WalletSecurityStatus, WalletSecurityPortError> {
            self.status(p)
        }
        fn unlock(
            &self,
            p: &WalletProfileId,
        ) -> Result<oxid_wallet_domain::WalletSecurityStatus, WalletSecurityPortError> {
            self.status(p)
        }
        fn lock(
            &self,
            p: &WalletProfileId,
        ) -> Result<oxid_wallet_domain::WalletSecurityStatus, WalletSecurityPortError> {
            self.status(p)
        }
    }
    struct Profiles;
    impl WalletProfileRepository for Profiles {
        fn save(
            &self,
            _: oxid_wallet_domain::WalletProfile,
        ) -> Result<(), WalletProfileRepositoryError> {
            Err(WalletProfileRepositoryError::Unavailable)
        }
        fn list(
            &self,
        ) -> Result<Vec<oxid_wallet_domain::WalletProfile>, WalletProfileRepositoryError> {
            Ok(vec![])
        }
        fn remove(&self, _: &WalletProfileId) -> Result<(), WalletProfileRepositoryError> {
            Err(WalletProfileRepositoryError::Unavailable)
        }
        fn set_active(
            &self,
            _: &WalletProfileId,
        ) -> Result<oxid_wallet_domain::WalletProfile, WalletProfileRepositoryError> {
            Err(WalletProfileRepositoryError::Unavailable)
        }
        fn active(
            &self,
        ) -> Result<Option<oxid_wallet_domain::WalletProfile>, WalletProfileRepositoryError>
        {
            Ok(None)
        }
    }
    for transition in 0..4 {
        let f = fixture();
        let sign = sign_cap(&f);
        let delete = delete_cap(&f);
        let protection =
            WalletProtectionService::with_approvals(Arc::new(Protection), f.approval.clone());
        let command = WalletProfileSecurityCommand {
            profile_id: "profile_private".into(),
        };
        match transition {
            0 => {
                assert!(LockWalletUseCase::execute(&protection, command).is_err());
            }
            1 => {
                assert!(UnlockWalletUseCase::execute(&protection, command).is_err());
            }
            2 => {
                assert!(InitializeWalletSecurityUseCase::execute(&protection, command).is_err());
            }
            _ => {
                let selection = SelectWalletProfileService::with_approvals(
                    Arc::new(Profiles),
                    f.approval.clone(),
                );
                assert!(
                    selection
                        .execute(SelectWalletProfileCommand {
                            profile_id: "profile_other".into()
                        })
                        .is_err()
                );
            }
        }
        assert_eq!(
            SignWalletDataUseCase::execute(&f.service, sign_command(&sign)),
            Err(SensitiveWalletOperationError::Approval(
                WalletApprovalError::GenerationMismatch
            ))
        );
        assert_eq!(
            DeleteWalletKeyUseCase::execute(&f.service, delete_command(&delete)),
            Err(SensitiveWalletOperationError::Approval(
                WalletApprovalError::GenerationMismatch
            ))
        );
        assert_eq!(f.custody.calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn canonical_framing_is_independently_recomputed_and_domain_separated() {
    #[derive(Default)]
    struct Hash(std::sync::Mutex<Vec<Vec<u8>>>);
    impl oxid_platform_ports::Sha256Port for Hash {
        fn sha256(&self, bytes: &[u8]) -> [u8; 32] {
            self.0.lock().unwrap().push(bytes.to_vec());
            oxid_platform_ports::Sha256Port::sha256(&SystemSha256, bytes)
        }
    }
    let f = fixture();
    let hash = Arc::new(Hash::default());
    let service =
        WalletSensitiveKeyService::new(f.custody.clone(), f.approval.clone(), hash.clone());
    let sign = service
        .request_sign_approval("profile_private", "key_private", b"private payload")
        .unwrap();
    SignWalletDataUseCase::execute(&service, sign_command(&sign)).unwrap();
    let delete = service
        .request_delete_approval("profile_private", "key_private")
        .unwrap();
    DeleteWalletKeyUseCase::execute(&service, delete_command(&delete)).unwrap();
    let calls = hash.0.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[0], calls[1]);
    assert_eq!(calls[2], calls[3]);
    assert_ne!(calls[0], calls[2]);
    let mut expected = Vec::new();
    for (len, field) in [
        (25_u64, b"oxid.wallet.delete-key.v1".as_slice()),
        (15, b"profile_private"),
        (11, b"key_private"),
        (0, b""),
    ] {
        expected.extend_from_slice(&len.to_be_bytes());
        expected.extend_from_slice(field);
    }
    assert_eq!(calls[2], expected);
}

#[test]
fn malformed_scope_and_payload_bounds_never_reach_custody() {
    let f = fixture();
    let cap = sign_cap(&f);
    for (payload, expected) in [
        (vec![], SensitiveWalletOperationError::EmptyPayload),
        (
            vec![0; MAX_SIGNING_PAYLOAD_BYTES + 1],
            SensitiveWalletOperationError::PayloadTooLarge,
        ),
    ] {
        let mut cmd = sign_command(&cap);
        cmd.payload = payload;
        assert_eq!(
            SignWalletDataUseCase::execute(&f.service, cmd),
            Err(expected)
        );
    }
    let mut cmd = sign_command(&cap);
    cmd.key_reference = "".into();
    assert!(matches!(
        SignWalletDataUseCase::execute(&f.service, cmd),
        Err(SensitiveWalletOperationError::InvalidKeyReference(_))
    ));
    let cap = delete_cap(&f);
    let mut cmd = delete_command(&cap);
    cmd.profile_id = "".into();
    assert!(matches!(
        DeleteWalletKeyUseCase::execute(&f.service, cmd),
        Err(SensitiveWalletOperationError::InvalidProfileIdentifier(_))
    ));
    assert_eq!(f.custody.calls.load(Ordering::SeqCst), 0);
}

#[derive(Default)]
struct Clock(AtomicU64);
impl ClockPort for Clock {
    fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
        Ok(UnixTimestampMillis::new(self.0.load(Ordering::SeqCst)))
    }
}
struct TrustedFixture;
impl TrustedWalletApprovalPort for TrustedFixture {
    fn approve(&self, _: &WalletApprovalIntent) -> Result<(), TrustedWalletApprovalError> {
        Ok(())
    }
}
#[derive(Default)]
struct Custody {
    calls: AtomicUsize,
    fail: AtomicBool,
}
impl WalletKeyOperationPort for Custody {
    fn generate(
        &self,
        _: &WalletProfileId,
        _: GenerateProtectedKeyRequest,
    ) -> Result<WalletKeyDescriptor, WalletSecurityPortError> {
        Err(WalletSecurityPortError::Unavailable)
    }
    fn list(
        &self,
        _: &WalletProfileId,
    ) -> Result<Vec<WalletKeyDescriptor>, WalletSecurityPortError> {
        Ok(vec![])
    }
    fn sign(
        &self,
        _: &WalletProfileId,
        _: &WalletKeyReference,
        _: &[u8],
    ) -> Result<WalletSignature, WalletSecurityPortError> {
        self.effect()?;
        Ok(WalletSignature::new(
            WalletKeyAlgorithm::Ed25519,
            vec![0; 64],
        ))
    }
    fn delete(
        &self,
        _: &WalletProfileId,
        _: &WalletKeyReference,
    ) -> Result<(), WalletSecurityPortError> {
        self.effect()
    }
}
impl Custody {
    fn effect(&self) -> Result<(), WalletSecurityPortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            Err(WalletSecurityPortError::Unavailable)
        } else {
            Ok(())
        }
    }
}
struct Fixture {
    clock: Arc<Clock>,
    approval: Arc<WalletApprovalService>,
    custody: Arc<Custody>,
    service: WalletSensitiveKeyService<Custody>,
}
fn fixture() -> Fixture {
    let clock = Arc::new(Clock::default());
    let approval = Arc::new(WalletApprovalService::with_trusted_port(
        clock.clone(),
        Arc::new(TrustedFixture),
    ));
    let custody = Arc::new(Custody::default());
    let service =
        WalletSensitiveKeyService::new(custody.clone(), approval.clone(), Arc::new(SystemSha256));
    Fixture {
        clock,
        approval,
        custody,
        service,
    }
}
fn sign_command(cap: &WalletApprovalCapability<SignDataApproval>) -> SignWalletDataCommand<'_> {
    SignWalletDataCommand {
        profile_id: "profile_private".into(),
        key_reference: "key_private".into(),
        payload: b"private payload".to_vec(),
        approval: cap,
    }
}
fn delete_command(cap: &WalletApprovalCapability<DeleteKeyApproval>) -> DeleteWalletKeyCommand<'_> {
    DeleteWalletKeyCommand {
        profile_id: "profile_private".into(),
        key_reference: "key_private".into(),
        approval: cap,
    }
}
fn sign_cap(f: &Fixture) -> WalletApprovalCapability<SignDataApproval> {
    f.service
        .request_sign_approval("profile_private", "key_private", b"private payload")
        .unwrap()
}
fn delete_cap(f: &Fixture) -> WalletApprovalCapability<DeleteKeyApproval> {
    f.service
        .request_delete_approval("profile_private", "key_private")
        .unwrap()
}
#[test]
fn sign_and_delete_success_then_replay_call_custody_once_each() {
    let f = fixture();
    let sign = sign_cap(&f);
    let delete = delete_cap(&f);
    assert!(SignWalletDataUseCase::execute(&f.service, sign_command(&sign)).is_ok());
    assert!(DeleteWalletKeyUseCase::execute(&f.service, delete_command(&delete)).is_ok());
    assert_eq!(
        SignWalletDataUseCase::execute(&f.service, sign_command(&sign)),
        Err(SensitiveWalletOperationError::Approval(
            WalletApprovalError::AlreadyConsumed
        ))
    );
    assert_eq!(
        DeleteWalletKeyUseCase::execute(&f.service, delete_command(&delete)),
        Err(SensitiveWalletOperationError::Approval(
            WalletApprovalError::AlreadyConsumed
        ))
    );
    assert_eq!(f.custody.calls.load(Ordering::SeqCst), 2);
}
#[test]
fn changed_concrete_fields_fail_before_custody_and_preserve_matching_authority() {
    for field in 0..3 {
        let f = fixture();
        let cap = sign_cap(&f);
        let mut cmd = sign_command(&cap);
        match field {
            0 => cmd.profile_id = "profile_other".into(),
            1 => cmd.key_reference = "key_other".into(),
            _ => cmd.payload.push(0),
        }
        assert_eq!(
            SignWalletDataUseCase::execute(&f.service, cmd),
            Err(SensitiveWalletOperationError::Approval(
                WalletApprovalError::IntentMismatch
            ))
        );
        assert_eq!(f.custody.calls.load(Ordering::SeqCst), 0);
        assert!(SignWalletDataUseCase::execute(&f.service, sign_command(&cap)).is_ok());
    }
    for field in 0..2 {
        let f = fixture();
        let cap = delete_cap(&f);
        let mut cmd = delete_command(&cap);
        if field == 0 {
            cmd.profile_id = "profile_other".into();
        } else {
            cmd.key_reference = "key_other".into();
        }
        assert_eq!(
            DeleteWalletKeyUseCase::execute(&f.service, cmd),
            Err(SensitiveWalletOperationError::Approval(
                WalletApprovalError::IntentMismatch
            ))
        );
        assert_eq!(f.custody.calls.load(Ordering::SeqCst), 0);
        assert!(DeleteWalletKeyUseCase::execute(&f.service, delete_command(&cap)).is_ok());
    }
}
#[test]
fn expiry_generation_and_replacement_reject_both_operations_before_custody() {
    for case in 0..3 {
        let f = fixture();
        let sign = sign_cap(&f);
        let delete = delete_cap(&f);
        let other = fixture();
        let (service, reason) = match case {
            0 => {
                f.clock
                    .0
                    .store(WalletApprovalService::MAX_TTL_MILLIS, Ordering::SeqCst);
                (&f.service, WalletApprovalError::Expired)
            }
            1 => {
                f.approval.invalidate().unwrap();
                (&f.service, WalletApprovalError::GenerationMismatch)
            }
            _ => (&other.service, WalletApprovalError::ForeignCapability),
        };
        assert_eq!(
            SignWalletDataUseCase::execute(service, sign_command(&sign)),
            Err(SensitiveWalletOperationError::Approval(reason))
        );
        assert_eq!(
            DeleteWalletKeyUseCase::execute(service, delete_command(&delete)),
            Err(SensitiveWalletOperationError::Approval(reason))
        );
        assert_eq!(
            f.custody.calls.load(Ordering::SeqCst) + other.custody.calls.load(Ordering::SeqCst),
            0
        );
    }
}
#[test]
fn failed_custody_does_not_restore_either_capability() {
    let f = fixture();
    f.custody.fail.store(true, Ordering::SeqCst);
    let sign = sign_cap(&f);
    let delete = delete_cap(&f);
    assert_eq!(
        SignWalletDataUseCase::execute(&f.service, sign_command(&sign)),
        Err(SensitiveWalletOperationError::Operation(
            WalletSecurityPortError::Unavailable
        ))
    );
    assert_eq!(
        DeleteWalletKeyUseCase::execute(&f.service, delete_command(&delete)),
        Err(SensitiveWalletOperationError::Operation(
            WalletSecurityPortError::Unavailable
        ))
    );
    f.custody.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        SignWalletDataUseCase::execute(&f.service, sign_command(&sign)),
        Err(SensitiveWalletOperationError::Approval(
            WalletApprovalError::AlreadyConsumed
        ))
    );
    assert_eq!(
        DeleteWalletKeyUseCase::execute(&f.service, delete_command(&delete)),
        Err(SensitiveWalletOperationError::Approval(
            WalletApprovalError::AlreadyConsumed
        ))
    );
    assert_eq!(f.custody.calls.load(Ordering::SeqCst), 2);
}
#[test]
fn concurrent_commands_spend_once_before_effect() {
    let f = fixture();
    let sign = sign_cap(&f);
    let delete = delete_cap(&f);
    let barrier = Barrier::new(8);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    (
                        SignWalletDataUseCase::execute(&f.service, sign_command(&sign)).is_ok(),
                        DeleteWalletKeyUseCase::execute(&f.service, delete_command(&delete))
                            .is_ok(),
                    )
                })
            })
            .collect();
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|r| r.0).count(), 1);
        assert_eq!(results.iter().filter(|r| r.1).count(), 1);
    });
    assert_eq!(f.custody.calls.load(Ordering::SeqCst), 2);
}
#[test]
fn command_diagnostics_are_redacted_and_default_composition_cannot_approve() {
    let f = fixture();
    let sign = sign_cap(&f);
    let delete = delete_cap(&f);
    assert_eq!(
        format!("{:?}", sign_command(&sign)),
        "SignWalletDataCommand([REDACTED])"
    );
    assert_eq!(
        format!("{:?}", delete_command(&delete)),
        "DeleteWalletKeyCommand([REDACTED])"
    );
    let service = WalletSensitiveKeyService::new(
        f.custody.clone(),
        Arc::new(WalletApprovalService::new(f.clock.clone())),
        Arc::new(SystemSha256),
    );
    assert_eq!(
        service
            .request_sign_approval("profile_private", "key_private", b"private payload")
            .unwrap_err(),
        SensitiveWalletOperationError::Approval(WalletApprovalError::Unavailable)
    );
    assert_eq!(
        service
            .request_delete_approval("profile_private", "key_private")
            .unwrap_err()
            .to_string(),
        "approval_unavailable"
    );
    assert_eq!(f.custody.calls.load(Ordering::SeqCst), 0);
}
