# ADR-0083: Run mobile Compact proofs on a foreground worker

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 3–7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, mobile presentation proving and worker lifecycle
- Tracking: issues #2, #27, #29, and #30
- Amends: ADR-0043, ADR-0050, and ADR-0072
- Implementation state: the explicit `standalone-native-proving-artifacts` iOS/Android composition authenticates the embedded runtime, admits one foreground proof on a dedicated worker, independently verifies it, and exposes Dioxus success/cancellation/timeout states; physical-device release budgets and ADR-0071/issue-#29 custody evidence remain open, so normal production and ordinary standalone builds stay proof-disabled

## Context

ADR-0072 packaged and authenticated the exact runtime-minimal Compact proving
closure but deliberately stopped before execution. ADR-0050 already supplies
the checked proof preimage, native prover, bounded `MZP1` container, independent
verification, and single-use OpenID4VP response construction. The missing
mobile boundary is execution admission and lifecycle control, not another
proof format.

The generated prover has no safe mid-call interruption hook. Killing a Rust
thread while it may own witness, custody, or proving-library state is not a
valid cancellation strategy. Dropping the Dioxus future would also create a
false acknowledgement while blocking work continued. Mobile needs an honest
state model that distinguishes requesting cancellation from the worker having
stopped.

## Decision

Add an outgoing proof-worker adapter around the existing checked Compact proof
port. It admits at most one proof per composed application and starts it on a
named 8 MiB native thread. The Dioxus operation continues to run through the
existing non-UI future runner, so artifact authentication, holder
authorization, proving, and verification never execute on the UI executor.

Admission requires the app to be foreground. The worker owns an atomic,
profile-and-presentation-scoped control token. Explicit cancellation,
backgrounding, and the conservative five-minute standalone timeout set only a
terminal reason. They do not stop or detach the non-interruptible prover. The
proof future waits until the worker exits. Successful proof bytes pass through
the existing independent verifier while the admission slot remains held. A
final control checkpoint then discards every result affected by a late signal,
releases admission, and only then permits success or reports
`proof_cancelled`, `proof_backgrounded`, or `proof_timed_out`. A panic or
disconnected worker fails closed without proof output.

Expose cancellation and foreground lifecycle through application-owned ports.
Application state moves from `presenting` to `cancellation_requested` when a
control signal is accepted, and only the completed proof future can publish
`cancelled` or `timed_out`. Cancellation must match both the profile and the
opaque presentation identifier. The native event handler signals suspended and
resumed states without receiving proof material. Dioxus offers an explicit
cancel action and states that acknowledgement waits for worker completion.

OpenID4VP continues to consume its prepared verifier session before proof
creation. No partial proof, preimage, authorization, or protocol session is
persisted. Therefore cancellation, backgrounding, timeout, low-memory process
termination, and ordinary process death cannot resume an attempt. Retry
requires a fresh request preview, exact credential selection, consent, holder
authorization, and proof.

Compose this worker only when the app selects
`standalone-native-proving-artifacts`, which already implies native mobile
custody and the authenticated embedded artifacts. The feature now means an
experimental standalone proof-execution harness rather than package-only
measurement. Normal production, ordinary standalone development, and ordinary
native-custody standalone builds remain unchanged and fail closed at
`proof_unavailable`.

## Security and truth boundaries

- Control and status surfaces carry only profile/presentation references and
  closed states; they never expose credentials, claims, witnesses, openings,
  holder signatures, proof bytes, nonces, or `vp_token` contents.
- A cancellation request is never labelled `cancelled` until the worker has
  stopped and any result has been discarded.
- Backgrounding prevents new admission and marks an active attempt for result
  disposal. Resuming cannot revive an old attempt.
- The five-minute value is a conservative standalone safety stop, not a
  production latency budget. Because the prover is non-interruptible, timeout
  acknowledgement may occur later than five minutes.
- Process death relies on the OS to terminate the process. No proof session or
  partial result is written for recovery.
- Successful simulator/emulator execution is conformance evidence only. It is
  not physical-device custody, memory, thermal, interruption, or release-budget
  evidence.

## Consequences

- The explicit mobile harness can reach the same independently verified
  OpenID4VP success as the authenticated headless composition.
- One-proof admission remains held through independent verification, bounds
  concurrent proving memory, and makes `proof_busy` a stable fail-closed result.
- Cancellation is safe but may feel delayed while the generated prover is
  inside its non-interruptible call.
- Physical iOS and Android measurements must still define release-mode prover
  and verifier latency, peak/resident memory, package delta, free-storage,
  thermal, low-memory, background, and restart budgets before any production
  composition is considered.

## Validation

- Worker tests cover one-proof admission through independent verification,
  profile-isolated cancellation, background rejection, timeout, late-result
  disposal, and acknowledgement only after worker completion.
- Application tests cover profile isolation and the distinct
  `cancellation_requested` lifecycle state.
- Existing native Compact tests continue to cover exact preimage conformance,
  proof success, tamper rejection, and independent verification.
- iOS Simulator and Android emulator standalone smokes remain mandatory for the
  normal application. The explicit proving build is additionally compiled and
  launched where native custody is available; physical-device proof/resource
  evidence remains tracked by issues #29 and #30.

## Rejected alternatives

- Force-stopping or detaching the prover thread would make cleanup and
  cancellation acknowledgement untrustworthy.
- Returning immediately after setting a cancellation flag would misstate that
  witness and custody use had ended.
- Allowing parallel proofs would multiply an already large memory footprint.
- Enabling the worker in normal production or ordinary simulator builds would
  bypass the explicit artifact, custody, and physical-device release gates.
