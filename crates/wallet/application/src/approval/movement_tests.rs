// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::*;
use oxid_platform_ports::PlatformError;
use oxid_wallet_domain::*;
use std::{
    future::Future,
    pin::pin,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize},
    },
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Clock(AtomicU64);
impl ClockPort for Clock {
    fn now(&self) -> Result<UnixTimestampMillis, PlatformError> {
        Ok(UnixTimestampMillis::new(self.0.load(Ordering::SeqCst)))
    }
}
fn profile() -> WalletProfileId {
    WalletProfileId::parse("profile_test").unwrap()
}
fn asset(id: &str, symbol: &str, decimals: u8, amount: u128) -> AssetBalance {
    AssetBalance::new(
        ChainAsset::new(
            ChainAssetId::parse(id).unwrap(),
            AssetSymbol::parse(symbol).unwrap(),
            decimals,
        ),
        amount,
    )
}
fn transfer(field: usize) -> WalletTransferPreview {
    let night = |amount| {
        asset(
            if field == 8 {
                "midnight:other"
            } else {
                "midnight:night"
            },
            if field == 9 { "OTHER" } else { "NIGHT" },
            if field == 10 { 5 } else { 6 },
            amount,
        )
    };
    WalletTransferPreview::new(
        WalletTransactionDraftId::parse(if field == 1 {
            "draft_other"
        } else {
            "draft_test"
        })
        .unwrap(),
        WalletTransactionAuthorizationChallenge::parse(if field == 2 {
            "challenge_other"
        } else {
            "challenge_test"
        })
        .unwrap(),
        ChainNetworkId::parse(if field == 3 {
            "network_other"
        } else {
            "undeployed"
        })
        .unwrap(),
        ChainAccountId::parse(if field == 4 {
            "account_other"
        } else {
            "account_test"
        })
        .unwrap(),
        ChainAddress::parse(
            if field == 6 {
                ChainAddressKind::Shielded
            } else {
                ChainAddressKind::Unshielded
            },
            if field == 5 {
                "recipient_other"
            } else {
                "recipient_test"
            },
        )
        .unwrap(),
        night(if field == 7 { 2 } else { 1 }),
        night(if field == 11 { 4 } else { 3 }),
        if field == 12 {
            Some(asset("midnight:dust", "DUST", 15, 1))
        } else {
            None
        },
        if field == 12 {
            WalletTransactionFeeState::Estimated
        } else {
            WalletTransactionFeeState::RequiresBalancing
        },
        if field == 13 { 2 } else { 1 },
        UnixTimestampMillis::new(if field == 14 { 900 } else { 1000 }),
        if field == 15 {
            WalletTransactionDraftState::Authorized
        } else {
            WalletTransactionDraftState::Prepared
        },
    )
    .unwrap()
}
fn dust(field: usize) -> WalletDustRegistrationPreview {
    WalletDustRegistrationPreview::new(
        WalletTransactionDraftId::parse(if field == 1 {
            "draft_other"
        } else {
            "draft_test"
        })
        .unwrap(),
        WalletTransactionAuthorizationChallenge::parse(if field == 2 {
            "challenge_other"
        } else {
            "challenge_test"
        })
        .unwrap(),
        ChainNetworkId::parse(if field == 3 {
            "network_other"
        } else {
            "undeployed"
        })
        .unwrap(),
        ChainAccountId::parse(if field == 4 {
            "account_other"
        } else {
            "account_test"
        })
        .unwrap(),
        asset("midnight:night", "NIGHT", 6, if field == 5 { 2 } else { 1 }),
        if field == 6 { 2 } else { 1 },
        asset("midnight:dust", "DUST", 15, if field == 7 { 2 } else { 1 }),
        if field == 8 {
            WalletTransactionFeeState::Estimated
        } else {
            WalletTransactionFeeState::RequiresBalancing
        },
        UnixTimestampMillis::new(if field == 9 { 900 } else { 1000 }),
        if field == 10 {
            WalletTransactionDraftState::Authorized
        } else {
            WalletTransactionDraftState::Prepared
        },
    )
    .unwrap()
}
fn intents() -> Vec<WalletApprovalIntent> {
    vec![
        WalletApprovalRequest::authorize_transfer(profile(), transfer(0)).intent,
        WalletApprovalRequest::submit_transfer(profile(), transfer(0)).intent,
        WalletApprovalRequest::authorize_dust_registration(profile(), dust(0)).intent,
        WalletApprovalRequest::submit_dust_registration(profile(), dust(0)).intent,
    ]
}
fn matrix<O: WalletApprovalOperation>(
    request: WalletApprovalRequest<O>,
    mutations: Vec<WalletApprovalRequest<O>>,
) {
    let clock = Arc::new(Clock::default());
    let service = super::tests::trusted_service(clock.clone());
    assert_eq!(
        WalletApprovalService::new(clock.clone())
            .request(&request)
            .unwrap_err(),
        WalletApprovalError::Unavailable
    );
    for changed in mutations {
        let cap = service.request(&request).unwrap();
        assert_eq!(
            service.consume(&cap, &changed),
            Err(WalletApprovalError::IntentMismatch)
        );
        assert_eq!(service.consume(&cap, &request), Ok(()));
        assert_eq!(
            service.consume(&cap, &request),
            Err(WalletApprovalError::AlreadyConsumed)
        );
    }
    for intent in intents() {
        if intent == request.intent {
            continue;
        }
        let cap = service.request(&request).unwrap();
        let changed = WalletApprovalRequest::<O> {
            intent,
            operation: PhantomData,
        };
        assert_eq!(
            service.consume(&cap, &changed),
            Err(WalletApprovalError::IntentMismatch)
        );
    }
    let cap = service.request(&request).unwrap();
    let foreign = super::tests::trusted_service(clock.clone());
    assert_eq!(
        foreign.consume(&cap, &request),
        Err(WalletApprovalError::ForeignCapability)
    );
    service.invalidate().unwrap();
    assert_eq!(
        service.consume(&cap, &request),
        Err(WalletApprovalError::GenerationMismatch)
    );
    let cap = service.request(&request).unwrap();
    clock.0.store(1000, Ordering::SeqCst);
    assert_eq!(
        service.consume(&cap, &request),
        Err(WalletApprovalError::Expired)
    );
    assert_eq!(
        service.request(&request).unwrap_err(),
        WalletApprovalError::Expired
    );
    clock.0.store(0, Ordering::SeqCst);
    let cap = service.request(&request).unwrap();
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    service.consume(&cap, &request)
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .filter(Result::is_ok)
                .count(),
            1
        );
    });
}
#[test]
fn four_operations_reject_every_structured_field_profile_replay_expiry_generation_and_cross_stage()
{
    macro_rules! check {
        ($constructor:ident, $preview:ident, $fields:expr) => {{
            let request = WalletApprovalRequest::$constructor(profile(), $preview(0));
            let mut changes: Vec<_> = (1..=$fields)
                .map(|field| WalletApprovalRequest::$constructor(profile(), $preview(field)))
                .collect();
            changes.push(WalletApprovalRequest::$constructor(
                WalletProfileId::parse("profile_other").unwrap(),
                $preview(0),
            ));
            matrix(request, changes);
        }};
    }
    check!(authorize_transfer, transfer, 15);
    check!(submit_transfer, transfer, 15);
    check!(authorize_dust_registration, dust, 10);
    check!(submit_dust_registration, dust, 10);
}

struct RegistrationPort {
    preview: Mutex<WalletDustRegistrationPreview>,
    reads: AtomicUsize,
    polls: AtomicUsize,
    pause_read: bool,
    pause_effect: bool,
}
impl RegistrationPort {
    fn new(submit: bool, pause_read: bool, pause_effect: bool) -> Self {
        Self {
            preview: Mutex::new(dust(if submit { 10 } else { 0 })),
            reads: AtomicUsize::new(0),
            polls: AtomicUsize::new(0),
            pause_read,
            pause_effect,
        }
    }
    async fn effect(
        &self,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        if self.pause_effect {
            std::future::pending::<()>().await;
        }
        Err(WalletDustRegistrationPortError::ProvingFailed)
    }
}
impl WalletDustRegistrationPort for RegistrationPort {
    fn prepare<'a>(
        &'a self,
        _: &'a WalletProfileId,
        _: PrepareWalletDustRegistrationRequest,
    ) -> WalletDustRegistrationPreviewPortFuture<'a> {
        Box::pin(async { Err(WalletDustRegistrationPortError::Unavailable) })
    }
    fn get<'a>(
        &'a self,
        _: &'a WalletProfileId,
        _: &'a WalletTransactionDraftId,
        _: UnixTimestampMillis,
    ) -> WalletDustRegistrationPreviewPortFuture<'a> {
        Box::pin(async move {
            if self.reads.fetch_add(1, Ordering::SeqCst) == 1 && self.pause_read {
                let mut first = true;
                std::future::poll_fn(|cx| {
                    if first {
                        first = false;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                })
                .await;
            }
            Ok(self.preview.lock().unwrap().clone())
        })
    }
    fn authorize<'a>(
        &'a self,
        _: &'a WalletProfileId,
        _: AuthorizeWalletDustRegistrationRequest,
    ) -> WalletDustRegistrationPreviewPortFuture<'a> {
        Box::pin(self.effect())
    }
    fn submit<'a>(
        &'a self,
        _: &'a WalletProfileId,
        _: SubmitWalletDustRegistrationRequest,
    ) -> WalletDustRegistrationPortFuture<'a> {
        Box::pin(async move {
            self.effect().await?;
            Err(WalletDustRegistrationPortError::InvalidData)
        })
    }
    fn status<'a>(
        &'a self,
        _: &'a WalletProfileId,
        _: &'a WalletTransactionDraftId,
    ) -> WalletDustRegistrationStatusPortFuture<'a> {
        Box::pin(async { Err(WalletDustRegistrationPortError::Unavailable) })
    }
    fn cancel_submission<'a>(
        &'a self,
        p: &'a WalletProfileId,
        d: &'a WalletTransactionDraftId,
    ) -> WalletDustRegistrationStatusPortFuture<'a> {
        self.status(p, d)
    }
    fn reconcile_submission<'a>(
        &'a self,
        p: &'a WalletProfileId,
        d: &'a WalletTransactionDraftId,
    ) -> WalletDustRegistrationStatusPortFuture<'a> {
        self.status(p, d)
    }
}
fn confirmation() -> SensitiveOperationConfirmation {
    SensitiveOperationConfirmation {
        title: "review".into(),
        summary: "exact plan".into(),
        confirmed: true,
    }
}
async fn execute(
    service: &WalletDustRegistrationService<RegistrationPort, Clock>,
    submit: bool,
) -> Result<(), WalletDustRegistrationError> {
    if submit {
        SubmitWalletDustRegistrationUseCase::execute(
            service,
            SubmitWalletDustRegistrationCommand {
                profile_id: "profile_test".into(),
                draft_id: "draft_test".into(),
                confirmation: confirmation(),
            },
        )
        .await
        .map(|_| ())
    } else {
        AuthorizeWalletDustRegistrationUseCase::execute(
            service,
            AuthorizeWalletDustRegistrationCommand {
                profile_id: "profile_test".into(),
                draft_id: "draft_test".into(),
                authorization_challenge: "challenge_test".into(),
                confirmation: confirmation(),
            },
        )
        .await
        .map(|_| ())
    }
}
#[test]
fn async_revalidation_rejects_generation_expiry_mutation_and_wrong_stage_before_effect_poll() {
    for submit in [false, true] {
        for scenario in 0..4 {
            let clock = Arc::new(Clock::default());
            let approvals = super::tests::trusted_service(clock.clone());
            let port = Arc::new(RegistrationPort::new(submit, true, false));
            let service = WalletDustRegistrationService::with_approvals(
                port.clone(),
                clock.clone(),
                approvals.clone(),
            );
            let mut future = pin!(execute(&service, submit));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(future.as_mut().poll(&mut cx).is_pending());
            match scenario {
                0 => approvals.invalidate().unwrap(),
                1 => clock.0.store(1000, Ordering::SeqCst),
                2 => {
                    *port.preview.lock().unwrap() = dust(5).with_state(if submit {
                        WalletTransactionDraftState::Authorized
                    } else {
                        WalletTransactionDraftState::Prepared
                    });
                }
                _ => {
                    *port.preview.lock().unwrap() =
                        dust(0).with_state(WalletTransactionDraftState::Submitted);
                }
            }
            assert!(matches!(future.as_mut().poll(&mut cx), Poll::Ready(Err(_))));
            assert_eq!(port.polls.load(Ordering::SeqCst), 0);
        }
    }
}
#[test]
fn failed_or_cancelled_effect_requires_a_new_trusted_decision_and_never_reuses_authority() {
    for submit in [false, true] {
        for cancel in [false, true] {
            let clock = Arc::new(Clock::default());
            let decisions = Arc::new(AtomicUsize::new(0));
            let count = decisions.clone();
            let approvals = super::tests::scripted_service(clock.clone(), move |_| {
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(())
                } else {
                    Err(TrustedWalletApprovalError::Denied)
                }
            });
            let port = Arc::new(RegistrationPort::new(submit, false, cancel));
            let service =
                WalletDustRegistrationService::with_approvals(port.clone(), clock, approvals);
            let mut cx = Context::from_waker(Waker::noop());
            {
                let mut first = pin!(execute(&service, submit));
                let result = first.as_mut().poll(&mut cx);
                if cancel {
                    assert!(result.is_pending());
                } else {
                    assert!(matches!(
                        result,
                        Poll::Ready(Err(WalletDustRegistrationError::Operation(
                            WalletDustRegistrationPortError::ProvingFailed
                        )))
                    ));
                }
                assert_eq!(port.polls.load(Ordering::SeqCst), 1);
            }
            let mut retry = pin!(execute(&service, submit));
            assert_eq!(
                retry.as_mut().poll(&mut cx),
                Poll::Ready(Err(WalletDustRegistrationError::Approval(
                    WalletApprovalError::Denied
                )))
            );
            assert_eq!(decisions.load(Ordering::SeqCst), 2);
            assert_eq!(port.polls.load(Ordering::SeqCst), 1);
        }
    }
}
