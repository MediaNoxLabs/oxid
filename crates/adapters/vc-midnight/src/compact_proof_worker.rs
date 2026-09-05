// SPDX-License-Identifier: Apache-2.0

//! Foreground-only admission and cancellation for native Compact proving.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use oxid_presentation_application::{
    CancelPresentationProofRequest, CreatePresentationProofFuture, PresentationProofControlPort,
    PresentationProofError, PresentationProofPort, PresentationProofRequest,
};

const NOT_CANCELLED: u8 = 0;
const CANCELLED_BY_USER: u8 = 1;
const CANCELLED_BY_BACKGROUND: u8 = 2;
const CANCELLED_BY_TIMEOUT: u8 = 3;

/// Conservative standalone timeout. Physical-device measurements must replace
/// this with an accepted release budget before production composition is
/// enabled.
pub const STANDALONE_MOBILE_COMPACT_PROOF_TIMEOUT: Duration = Duration::from_secs(5 * 60);

struct ActiveProof {
    profile_id: oxid_presentation_domain::PresentationProfileId,
    presentation_id: oxid_presentation_domain::CredentialPresentationId,
    cancellation: Arc<AtomicU8>,
    started_at: Instant,
}

struct WorkerState {
    foreground: bool,
    active: Option<ActiveProof>,
}

struct AdmissionLease {
    state: Arc<Mutex<WorkerState>>,
    cancellation: Arc<AtomicU8>,
    armed: bool,
}

impl AdmissionLease {
    fn new(state: Arc<Mutex<WorkerState>>, cancellation: Arc<AtomicU8>) -> Self {
        Self {
            state,
            cancellation,
            armed: true,
        }
    }

    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for AdmissionLease {
    fn drop(&mut self) {
        if self.armed
            && let Ok(mut state) = self.state.lock()
            && state
                .active
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(&active.cancellation, &self.cancellation))
        {
            state.active = None;
        }
    }
}

type WorkerResult =
    Result<oxid_presentation_application::PresentationProofArtifact, PresentationProofError>;

enum WorkerWaitOutcome {
    Completed(WorkerResult, AdmissionLease),
    TimedOut,
    Disconnected,
}

/// Runs at most one proof on a named worker and discards every result produced
/// after cancellation, backgrounding, or timeout.
///
/// The generated prover is currently non-interruptible while inside its proof
/// call. Consequently, a control request only sets an atomic flag. The proof
/// future waits for the worker to stop before acknowledging explicit
/// cancellation or backgrounding. A timeout bounds the caller's wait while an
/// admission lease keeps the slot occupied until the non-interruptible worker
/// actually exits.
pub struct ForegroundCompactPresentationProofWorker {
    inner: Arc<dyn PresentationProofPort>,
    state: Arc<Mutex<WorkerState>>,
    timeout: Duration,
}

impl ForegroundCompactPresentationProofWorker {
    #[must_use]
    pub fn new(inner: Arc<dyn PresentationProofPort>) -> Self {
        Self::with_timeout(inner, STANDALONE_MOBILE_COMPACT_PROOF_TIMEOUT)
    }

    #[must_use]
    pub fn with_timeout(inner: Arc<dyn PresentationProofPort>, timeout: Duration) -> Self {
        Self {
            inner,
            state: Arc::new(Mutex::new(WorkerState {
                foreground: true,
                active: None,
            })),
            timeout,
        }
    }

    fn admit(
        &self,
        request: &PresentationProofRequest,
    ) -> Result<(AdmissionLease, Instant), PresentationProofError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PresentationProofError::Unavailable)?;
        if !state.foreground {
            return Err(PresentationProofError::Backgrounded);
        }
        if state.active.is_some() {
            return Err(PresentationProofError::Busy);
        }
        let cancellation = Arc::new(AtomicU8::new(NOT_CANCELLED));
        let started_at = Instant::now();
        state.active = Some(ActiveProof {
            profile_id: request.profile_id.clone(),
            presentation_id: request.presentation_id.clone(),
            cancellation: Arc::clone(&cancellation),
            started_at,
        });
        Ok((
            AdmissionLease::new(Arc::clone(&self.state), cancellation),
            started_at,
        ))
    }
}

impl PresentationProofPort for ForegroundCompactPresentationProofWorker {
    fn create<'a>(
        &'a self,
        request: PresentationProofRequest,
    ) -> CreatePresentationProofFuture<'a> {
        Box::pin(async move {
            let (lease, started_at) = self.admit(&request)?;
            let cancellation = Arc::clone(&lease.cancellation);
            let inner = Arc::clone(&self.inner);
            let (worker_sender, worker_receiver) = mpsc::sync_channel(1);
            let spawn = std::thread::Builder::new()
                .name("oxid-compact-proof".to_owned())
                .stack_size(8 * 1_024 * 1_024)
                .spawn(move || {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        futures::executor::block_on(inner.create(request))
                    }))
                    .unwrap_or(Err(PresentationProofError::Rejected));
                    let _ = worker_sender.send((result, lease));
                });
            if spawn.is_err() {
                return Err(PresentationProofError::Unavailable);
            }

            let remaining = self.timeout.checked_sub(started_at.elapsed());
            let (outcome_sender, outcome_receiver) = futures::channel::oneshot::channel();
            let waiter = std::thread::Builder::new()
                .name("oxid-compact-proof-waiter".to_owned())
                .stack_size(256 * 1_024)
                .spawn(move || {
                    let outcome = match remaining {
                        Some(remaining) => match worker_receiver.recv_timeout(remaining) {
                            Ok((result, lease)) => WorkerWaitOutcome::Completed(result, lease),
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                let _ = cancellation.compare_exchange(
                                    NOT_CANCELLED,
                                    CANCELLED_BY_TIMEOUT,
                                    Ordering::AcqRel,
                                    Ordering::Acquire,
                                );
                                WorkerWaitOutcome::TimedOut
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                WorkerWaitOutcome::Disconnected
                            }
                        },
                        None => {
                            let _ = cancellation.compare_exchange(
                                NOT_CANCELLED,
                                CANCELLED_BY_TIMEOUT,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            );
                            WorkerWaitOutcome::TimedOut
                        }
                    };
                    let _ = outcome_sender.send(outcome);
                });
            if waiter.is_err() {
                return Err(PresentationProofError::Unavailable);
            }

            let outcome = outcome_receiver
                .await
                .unwrap_or(WorkerWaitOutcome::Disconnected);
            let (result, lease) = match outcome {
                WorkerWaitOutcome::Completed(result, lease) => (result, lease),
                WorkerWaitOutcome::TimedOut => return Err(PresentationProofError::TimedOut),
                WorkerWaitOutcome::Disconnected => {
                    return Err(PresentationProofError::Unavailable);
                }
            };
            let terminal = lease.cancellation.load(Ordering::Acquire);
            match terminal {
                NOT_CANCELLED => match result {
                    Ok(proof) => {
                        lease.disarm();
                        Ok(proof)
                    }
                    Err(error) => Err(error),
                },
                CANCELLED_BY_USER => Err(PresentationProofError::Cancelled),
                CANCELLED_BY_BACKGROUND => Err(PresentationProofError::Backgrounded),
                CANCELLED_BY_TIMEOUT => Err(PresentationProofError::TimedOut),
                _ => Err(PresentationProofError::Rejected),
            }
        })
    }
}

impl PresentationProofControlPort for ForegroundCompactPresentationProofWorker {
    fn cancel(
        &self,
        request: CancelPresentationProofRequest,
    ) -> Result<(), PresentationProofError> {
        let state = self
            .state
            .lock()
            .map_err(|_| PresentationProofError::Unavailable)?;
        let active = state
            .active
            .as_ref()
            .filter(|active| {
                active.profile_id == request.profile_id
                    && active.presentation_id == request.presentation_id
            })
            .ok_or(PresentationProofError::InvalidSelection)?;
        let _ = active.cancellation.compare_exchange(
            NOT_CANCELLED,
            CANCELLED_BY_USER,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        Ok(())
    }

    fn set_foreground(&self, foreground: bool) -> Result<(), PresentationProofError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PresentationProofError::Unavailable)?;
        state.foreground = foreground;
        if !foreground && let Some(active) = state.active.as_ref() {
            let _ = active.cancellation.compare_exchange(
                NOT_CANCELLED,
                CANCELLED_BY_BACKGROUND,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        Ok(())
    }

    fn finish(
        &self,
        request: CancelPresentationProofRequest,
    ) -> Result<(), PresentationProofError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PresentationProofError::Unavailable)?;
        let active = state
            .active
            .as_ref()
            .filter(|active| {
                active.profile_id == request.profile_id
                    && active.presentation_id == request.presentation_id
            })
            .ok_or(PresentationProofError::InvalidSelection)?;
        if active.started_at.elapsed() >= self.timeout {
            let _ = active.cancellation.compare_exchange(
                NOT_CANCELLED,
                CANCELLED_BY_TIMEOUT,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        let terminal = active.cancellation.load(Ordering::Acquire);
        state.active = None;
        match terminal {
            NOT_CANCELLED => Ok(()),
            CANCELLED_BY_USER => Err(PresentationProofError::Cancelled),
            CANCELLED_BY_BACKGROUND => Err(PresentationProofError::Backgrounded),
            CANCELLED_BY_TIMEOUT => Err(PresentationProofError::TimedOut),
            _ => Err(PresentationProofError::Rejected),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        pin::Pin,
        sync::{Condvar, MutexGuard},
        task::{Context, Poll},
    };

    use oxid_presentation_application::PresentationProofArtifact;

    struct ControlledProof {
        state: Arc<(Mutex<(bool, bool)>, Condvar)>,
    }

    struct ImmediateProof;

    impl PresentationProofPort for ImmediateProof {
        fn create<'a>(&'a self, _: PresentationProofRequest) -> CreatePresentationProofFuture<'a> {
            Box::pin(async { PresentationProofArtifact::new(vec![0x42]) })
        }
    }

    impl PresentationProofPort for ControlledProof {
        fn create<'a>(&'a self, _: PresentationProofRequest) -> CreatePresentationProofFuture<'a> {
            Box::pin(async move {
                let (lock, changed) = self.state.as_ref();
                let mut state = lock.lock().expect("control state");
                state.0 = true;
                changed.notify_all();
                while !state.1 {
                    state = changed.wait(state).expect("control wait");
                }
                PresentationProofArtifact::new(vec![0x42])
            })
        }
    }

    fn request(profile: &str, presentation: &str) -> PresentationProofRequest {
        PresentationProofRequest {
            profile_id: oxid_presentation_domain::PresentationProfileId::parse(profile)
                .expect("profile"),
            presentation_id: oxid_presentation_domain::CredentialPresentationId::parse(
                presentation,
            )
            .expect("presentation"),
            credential_id: "credential_one".to_owned(),
            verifier: "https://verifier.example".to_owned(),
            challenge_hash: [1; 32],
            verifier_domain_hash: [2; 32],
            requested_claims: Vec::new(),
        }
    }

    fn wait_started<'a>(state: &'a (Mutex<(bool, bool)>, Condvar)) -> MutexGuard<'a, (bool, bool)> {
        let mut guard = state.0.lock().expect("control state");
        while !guard.0 {
            guard = state.1.wait(guard).expect("control wait");
        }
        guard
    }

    fn release(state: &(Mutex<(bool, bool)>, Condvar)) {
        let mut guard = state.0.lock().expect("control state");
        guard.1 = true;
        state.1.notify_all();
    }

    fn poll_once(
        future: Pin<&mut (dyn Future<Output = WorkerResult> + Send)>,
    ) -> Poll<WorkerResult> {
        let waker = futures::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        future.poll(&mut context)
    }

    fn wait_until_slot_is_released(worker: &ForegroundCompactPresentationProofWorker) {
        for _ in 0..100 {
            if worker.state.lock().expect("worker state").active.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("proof worker did not release its admission slot");
    }

    #[test]
    fn cancellation_is_profile_scoped_and_acknowledged_after_worker_stops() {
        let state = Arc::new((Mutex::new((false, false)), Condvar::new()));
        let worker = Arc::new(ForegroundCompactPresentationProofWorker::new(Arc::new(
            ControlledProof {
                state: Arc::clone(&state),
            },
        )));
        let worker_for_thread = Arc::clone(&worker);
        let proof = std::thread::spawn(move || {
            futures::executor::block_on(
                worker_for_thread.create(request("profile_one", "presentation_one")),
            )
        });
        drop(wait_started(&state));

        assert_eq!(
            worker.cancel(CancelPresentationProofRequest {
                profile_id: oxid_presentation_domain::PresentationProfileId::parse("profile_two")
                    .expect("profile"),
                presentation_id: oxid_presentation_domain::CredentialPresentationId::parse(
                    "presentation_one",
                )
                .expect("presentation"),
            }),
            Err(PresentationProofError::InvalidSelection)
        );
        worker
            .cancel(CancelPresentationProofRequest {
                profile_id: oxid_presentation_domain::PresentationProfileId::parse("profile_one")
                    .expect("profile"),
                presentation_id: oxid_presentation_domain::CredentialPresentationId::parse(
                    "presentation_one",
                )
                .expect("presentation"),
            })
            .expect("cancel request");
        assert!(!proof.is_finished());
        release(&state);
        assert_eq!(
            proof.join().expect("proof thread"),
            Err(PresentationProofError::Cancelled)
        );
    }

    #[test]
    fn one_proof_is_admitted_and_backgrounding_discards_the_late_result() {
        let state = Arc::new((Mutex::new((false, false)), Condvar::new()));
        let worker = Arc::new(ForegroundCompactPresentationProofWorker::new(Arc::new(
            ControlledProof {
                state: Arc::clone(&state),
            },
        )));
        let worker_for_thread = Arc::clone(&worker);
        let proof = std::thread::spawn(move || {
            futures::executor::block_on(
                worker_for_thread.create(request("profile_one", "presentation_one")),
            )
        });
        drop(wait_started(&state));
        assert_eq!(
            futures::executor::block_on(worker.create(request("profile_one", "presentation_two"))),
            Err(PresentationProofError::Busy)
        );
        worker.set_foreground(false).expect("background");
        release(&state);
        assert_eq!(
            proof.join().expect("proof thread"),
            Err(PresentationProofError::Backgrounded)
        );
        assert_eq!(
            futures::executor::block_on(
                worker.create(request("profile_one", "presentation_three"))
            ),
            Err(PresentationProofError::Backgrounded)
        );
    }

    #[test]
    fn timeout_bounds_the_wait_but_holds_admission_until_worker_stops() {
        let state = Arc::new((Mutex::new((false, false)), Condvar::new()));
        let worker = Arc::new(ForegroundCompactPresentationProofWorker::with_timeout(
            Arc::new(ControlledProof {
                state: Arc::clone(&state),
            }),
            Duration::from_millis(10),
        ));
        assert_eq!(
            futures::executor::block_on(worker.create(request("profile_one", "presentation_one"))),
            Err(PresentationProofError::TimedOut)
        );
        drop(wait_started(&state));
        assert_eq!(
            futures::executor::block_on(worker.create(request("profile_one", "presentation_two"))),
            Err(PresentationProofError::Busy)
        );
        release(&state);
        wait_until_slot_is_released(&worker);
        let retry = request("profile_one", "presentation_three");
        let retry_for_finish = retry.clone();
        futures::executor::block_on(worker.create(retry)).expect("retry proof");
        worker
            .finish(CancelPresentationProofRequest {
                profile_id: retry_for_finish.profile_id,
                presentation_id: retry_for_finish.presentation_id,
            })
            .expect("finish retry");
    }

    #[test]
    fn dropping_the_create_future_releases_admission_after_worker_stops() {
        let state = Arc::new((Mutex::new((false, false)), Condvar::new()));
        let worker = ForegroundCompactPresentationProofWorker::new(Arc::new(ControlledProof {
            state: Arc::clone(&state),
        }));
        let mut proof = worker.create(request("profile_one", "presentation_one"));
        assert!(poll_once(proof.as_mut()).is_pending());
        drop(wait_started(&state));
        drop(proof);
        assert_eq!(
            futures::executor::block_on(worker.create(request("profile_one", "presentation_two"))),
            Err(PresentationProofError::Busy)
        );
        release(&state);
        wait_until_slot_is_released(&worker);

        let retry = request("profile_one", "presentation_three");
        let retry_for_finish = retry.clone();
        futures::executor::block_on(worker.create(retry)).expect("retry proof");
        worker
            .finish(CancelPresentationProofRequest {
                profile_id: retry_for_finish.profile_id,
                presentation_id: retry_for_finish.presentation_id,
            })
            .expect("finish retry");
    }

    #[test]
    fn admission_remains_held_through_independent_verification() {
        let worker = ForegroundCompactPresentationProofWorker::new(Arc::new(ImmediateProof));
        let first = request("profile_one", "presentation_one");
        futures::executor::block_on(worker.create(first.clone())).expect("proof");
        assert_eq!(
            futures::executor::block_on(worker.create(request("profile_one", "presentation_two"))),
            Err(PresentationProofError::Busy)
        );
        worker
            .set_foreground(false)
            .expect("background after proof");
        assert_eq!(
            worker.finish(CancelPresentationProofRequest {
                profile_id: first.profile_id,
                presentation_id: first.presentation_id,
            }),
            Err(PresentationProofError::Backgrounded)
        );
        worker.set_foreground(true).expect("foreground");
        let retry = request("profile_one", "presentation_three");
        futures::executor::block_on(worker.create(retry.clone())).expect("retry proof");
        worker
            .finish(CancelPresentationProofRequest {
                profile_id: retry.profile_id,
                presentation_id: retry.presentation_id,
            })
            .expect("finish verification");
    }
}
