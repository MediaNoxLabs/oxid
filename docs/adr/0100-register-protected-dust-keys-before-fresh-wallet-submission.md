# ADR-0100: Register protected DUST keys before fresh-wallet submission

- Status: Accepted
- Date: 2026-08-20
- Source: Blueprint §§3–8, 12–13, 16–18, 21; issue #92; reviewed
  prototype onboarding plan; accepted Midnight ledger registration semantics
- Prototype source: `midnight-ledger` commit
  `074b1a4bccbfee1740ee188374b606a022ecef42`
- Ledger source: `midnight-ledger` commit
  `d9414884db9da9e9b1f6f3a7f742d79a5732f817`
- Implementation state: Repository, headless, Dioxus, guarded public PreProd
  funding manifest and read-only observer, build-reviewed test-only signed
  profile, and ignored live acceptance harness are complete; funded write
  execution, mobile, process-restart, physical-device, and production
  live-node evidence remain open

## Context

Oxid can derive a protected Midnight DUST child, synchronize official DUST
events, balance a transaction with DUST, prove it, persist its public attempt
before broadcast, and reconcile finalized inclusion. ADR-0079 and ADR-0098
also provide guarded evidence for spending a genesis-authority Zswap note.
They do not let a newly funded wallet originate its own shielded transaction.
By design, a fresh wallet begins with zero DUST. Its unshielded NIGHT outputs
have not registered that protected DUST destination and therefore have not
begun producing the recoverable DUST that can later pay transaction fees.

The immutable mobile prototype documented the required onboarding step as
`registerNightUtxosForDustGeneration`, displayed the corresponding zero-state
copy, and planned the registration UI. Its wallet implementation did not
complete that operation. It is therefore behavioral and product evidence, not
source evidence for a production implementation.

The accepted ledger revision defines registration as a signed
`DustRegistration` inside an intent segment. Its fee allowance is generated
only from guaranteed, owned, previously unregistered NIGHT inputs at the DUST
action time. Applying the registration associates the NIGHT verifying key with
a DUST public key, returns the NIGHT in same-owner outputs, consumes no more
than the declared generationless fee allowance, and creates the initial DUST
generation state. Correct planning consequently needs the indexer's canonical
UTXO creation time and `registeredForDustGeneration` flag; the existing
version-one public account checkpoint does not contain either fact.

Treating registration as an ordinary transfer, a DUST synchronization alias,
or an implicit side effect of the first shielded spend would hide a distinct
authorization and could let stale public state select an ineligible input.

## Decision

Add a focused Oxid-owned `WalletDustRegistrationPort` and matching application
use cases. Registration is a distinct vertical capability, not another mode on
`WalletTransactionPort` and not part of `WalletDustSyncPort`. Public domain and
application types contain only bounded account scope, exact NIGHT totals and
input counts, the maximum DUST fee allowance, expiry and lifecycle state, plus
opaque draft/challenge/public-attempt identifiers. Ledger transactions,
signing payloads, signatures, DUST keys, proofs, witnesses, endpoint data, and
UTXO identifiers remain adapter-private.

The operation has three explicit stages:

1. **Prepare** requires a current successful live account snapshot. Select
   only owned native-NIGHT UTXOs whose indexer evidence says they are not
   registered and supplies a valid creation time. At the current authenticated
   chain time, compute generation for each candidate using the accepted live
   DUST parameters, order candidates by greatest generated amount first, place
   exactly the largest-generation input in the guaranteed offer, and place the
   remaining selected inputs in the fallible offer. Each consumed NIGHT amount
   is returned exactly to the same owner. Set the registration's maximum fee
   allowance to the generationless DUST available from the guaranteed input.
   Overflow, insufficient allowance, stale state, duplicate UTXOs,
   missing metadata, or an already registered input fails before custody use.
2. **Consent and authorize** bind the complete unexpired public preview to an
   exact human-readable registration confirmation. The NIGHT role-0 protected
   key signs the canonical segment-one intent payload and the adapter verifies
   the signature before applying that same authorization to the registration
   and selected NIGHT inputs as required by the canonical intent. The DUST
   secret child at `m/44'/2400'/account'/2/0` may be borrowed only inside
   protected custody; only the canonical public registration key enters the
   ledger transaction. No secret child, seed, derivation path, or signing
   payload may cross an incoming adapter.
3. **Submit** uses the existing official generic DUST completion, proving,
   sealing, node-submission, cancellation, and finalized-reconciliation
   machinery. The registration allowance must cover the canonical fee without
   borrowing another wallet's fee authority. The adapter writes a
   registration-domain-separated public attempt to the bounded submission
   journal before broadcast. Registration and transfer attempts must not
   collide in lookup, history, replay barriers, or compaction.

Extend the indexer projection with `ctime` and
`registeredForDustGeneration`. Public account checkpoints move to schema
version two and preserve only those public eligibility facts with the existing
network/address/size/count/permission/atomic-write checks. A version-one
checkpoint is treated as incompatible and ignored. It must never be hydrated
for display, fabricate registration eligibility, or delta-resume into
apparently complete metadata. The account starts from an empty projection and
registration remains blocked until a live replay from zero supplies
version-two evidence. The next successful write stores version two.

Finalized transaction inclusion and DUST readiness are separate observations.
An included registration may be reported as included from the authoritative
node outcome, but the wallet must not expose generated DUST as spendable until
the official DUST event stream observes the registered state, reaches its
advertised live target, and persists the matching private checkpoint. A
timeout between these observations is an included registration awaiting DUST
sync, not a failed or safely repeatable registration.

The guarded preprod funding-manifest foundation derives two test-only accounts
from an externally provisioned 32-byte master seed by using the existing
hardened Midnight BIP44 account index: account A is `2 * caseIndex`, and
account B is `A + 1`. The seed is accepted only as exactly 64 hexadecimal
characters from a secret environment variable after a second explicit live-
test opt-in. The scripts copy it into a non-exported shell variable, remove it
from the environment before Cargo or any build script runs, and supply it only
to the compiled observer and write-test processes. It is never accepted over a
headless command or written, logged, hashed for output, or committed. The public
funding manifest may expose only the repository commit, PreProd network, case
and account/address indices, A/B public NIGHT and shielded receive addresses,
positive-value requirements, exact eligible-output/note counts, and the final
transfer-selection policy. Wallet A receives one positive unshielded NIGHT
output and one positive shielded NIGHT note; wallet B
begins with zero balances, eligible public outputs, and shielded notes. The
external funding service need not provide a predetermined amount. The ignored
test records the exact observed A balances before authorization and proves
same-principal registration plus exact post-transfer deltas. A different
topology remains outside the reviewed test case. The final A-to-B shielded
transfer is selected once as half the observed shielded balance, rounded down
but with a minimum of one atomic unit; it is then frozen through preview,
authorization, duplicate checks, and final-balance evidence. DUST is never
externally funded. This amount-observed schema is manifest V2; V1's fixed
amounts remain historical and must not be silently reinterpreted.

The manifest command performs no network I/O and requires a clean worktree so
its public output is bound to the exact commit. A test-only public Ed25519 root
and static signed canonical envelope are compiled only into tests. The signing
key was generated in memory and discarded without being written or committed.
The envelope binds the exact PreProd indexer v4 HTTP/WebSocket paths, node
WebSocket route, public proof-server route, network, and genesis
`df831b09a8baa92badf47762ce5ac439b7e47e3ed3d39600cfdd44fad552361b`.
The ignored live test verifies that signature at trusted current time and then
uses the unchanged ADR-0098 chain-identity gate. This authenticates a reviewed
test configuration and exact chain identity, not endpoint ownership, indexer
correctness, protocol compatibility, or production authority.

The read-only observer and guarded write helper compile their ignored tests
through the repository-defined `preprod-live` Cargo profile. It inherits the
release optimizer used by production cryptographic code while retaining debug
symbols for bounded failure diagnosis. Plain debug replay is not accepted as a
live performance result: on the 2026-08-20 PreProd history it remained
correctly `syncing` without a transport failure but processed only 218,252
events in 900 seconds. The controlled 16,385-event segmentation regression took
9.41 seconds in the ordinary debug profile and 0.35 seconds in `preprod-live`
on the same host, while retaining the assertion that the subscription is closed
before the first fold. The optimized profile changes evidence tooling only; it
does not select routes, raise resource limits, enable a write, or alter a
production artifact.

Deployment-profile v1 requires SSI routes, but this acceptance harness never
composes SSI. Its signed SSI fields therefore use explicit `.invalid` hosts and
the profile is documented as Midnight-only. A capability-scoped successor is
required before such a profile could represent production deployment. The
signed proof route is the public PreProd prover. Because that operator can see
private proof preimages plus network timing despite TLS, the ignored write test
requires the separate
`OXID_ACKNOWLEDGE_PREPROD_PUBLIC_PROVER_PRIVACY=1` gate. It is interoperability
evidence, not the production-local privacy evidence required by ADR-0028; the
test must not silently splice in an unsigned local prover.

After out-of-band NIGHT/shielded funding, the ignored live flow must prove the
fresh account starts with zero DUST, use the same public prepare, explicit
consent, protected authorization, official proving/finality, reconciliation,
and DUST-observation boundaries as any other caller, wait for a positive,
fully synchronized generated-DUST observation, and only then perform A's
shielded spend to B. The registration fee is not treated as a transfer-fee
quote. If and only if canonical fee balancing returns the exact typed
`InsufficientDust` result before proving and broadcast, the harness verifies
that the same draft remains authorized, waits for a strictly greater
authoritative DUST balance, and retries that draft with a fresh confirmation.
The wait/retry count and total deadline are bounded; ambiguous or post-
broadcast outcomes are never retried. It reconstructs adapters with the same in-
process development custody, owner-private checkpoints, and public journals;
the subsequent DUST assertion proves adapter reconstruction plus authoritative
resynchronization, not direct checkpoint hydration, a process restart, or a
native-custody restart. The test drives the same application use cases without
a UI, but does not traverse the NDJSON
`oxid.headless.v1` adapter. Exact live NDJSON evidence requires a separate
incoming-boundary refactor rather than a production-callable development-
custody factory. Every case index is single-use. The repository script
first requires the separate read-only observation to prove the current funding
topology and readiness without a marker, then recompiles and revalidates the
same facts inside the write test. Only after the preflight succeeds does it
atomically create an ignored,
owner-only started marker below the shared Git common directory before a write,
so all worktrees for the same local clone refuse a second run for that case. It
never automatically clears the marker after success or failure. Private
account/DUST/Zswap checkpoints and the public submission journal are retained
owner-only below that marker on failure for forensic/manual chain audit only;
the current repository has no cross-process recovery command, stable test
profile IDs, or persisted development custody capable of safely resuming them.
An unknown broadcast therefore requires external chain audit and abandonment
of that case, never marker deletion or automatic retry. A complete successful
run removes only the private state and leaves the started marker. CI must
provision a fresh case index per funded write because an ephemeral runner
cannot make the marker globally durable. An explicit non-submitting recovery
mode is future work. Test custody, funding code, environment markers, and
fixtures remain excluded from normal release artifacts.

`just preprod-registration-observe` is the separately guarded read-only
preflight. It composes ephemeral custody with no checkpoint or journal paths,
derives the manifest addresses again, and may perform only live account,
shielded, DUST, and registration-readiness operations. Readiness preparation
retains one unsigned process-local draft, discarded when the test exits; the
observer performs no authorization, proof, persistence, broadcast, or chain
write, and creates no state directory or single-use marker. It does not require
the public-prover privacy acknowledgement. Its closed output contains only
public aggregate balances, counts, and readiness states. A cold PreProd DUST
replay has a separate 15-minute observation bound; changing that test bound
does not change standalone or production synchronization policy.

Read-only live attempts on 2026-08-20 exposed transport/performance evidence,
not a funding mismatch. Before controlled DUST segmentation, an unoptimized
run stalled because official folding occurred while the subscription remained
open. Signed commit `26505c81bde1a7c5e4bc13e559232cf0ebf8d97a`
closed bounded DUST receive segments before folding; its next unoptimized run
remained truthfully `syncing` with no failure after 218,252 events at the
observer's 900-second bound. The first `preprod-live` attempt on signed commit
`2763125bb71a445f608bc6a8a8f98cf51c49495a` then reached the analogous inline
shielded replay and exceeded that stage's 90-second observation bound before
emitting output. Signed commit
`a490dc0f754b9a3f89483c875dc68a77ea7f29d5` applies the same controlled
complete/drop-before-fold contract to Zswap. A clean optimized observer on
signed commit `fba4ad429fc59e73e9baba7d1af9bea4c9b37dea` passed shielded
synchronization, then remained `syncing` with no DUST failure at cursor 553,478
of target 1,446,220 after 541,357 events when the 900-second observation bound
expired. That is about 602 events/second and 2.5 times the debug rate. Applying
the observed 97.81% cursor density to the target estimates about 1.415 million
events and 39 minutes; this is an inference, not an exact remaining event or
byte count. It strongly suggests the existing one-million-event and 30-minute
adapter caps need a separately measured decision. The 512 MiB raw-input cap is
still unmeasured, and no limit has been raised. Every failed observer exited
before public funding output, authorization, proving, persistence, marker
creation, broadcast, or chain write.

## Required validation

Implementation evidence must include:

- focused domain/application port and native-adapter tests for selection,
  exact same-owner conservation, fee allowance, consent binding, locked or
  wrong custody, stale/missing/already-registered metadata, and journal domain
  separation;
- checkpoint version-one rejection without fabricated eligibility and a
  version-two live-replay/restart test;
- headless and Dioxus flows that use the same registration use cases and keep
  inclusion visibly distinct from DUST-event observation and spend readiness;
- a release-feature exclusion check for standalone funding code and fixtures;
- the ordinary strict, Nix, iOS Simulator, and Android emulator gates; and
- one guarded funded preprod flow that derives fresh A/B accounts, proves A
  begins at zero DUST, registers A's externally funded NIGHT, waits for
  authoritative generated-DUST observation and recovery, spends A's shielded
  note to B after explicit consent, and proves duplicate prevention and exact
  balances after reconstruction.

The guarded live flow must not be rerun merely to obtain duplicate evidence,
because it consumes real standalone development state. A simulator or fixture
result cannot close durable native-custody/process-restart, physical-device,
production deployment, or live production-node evidence.

## Consequences

- Fresh-wallet DUST registration and later recovery gain their own reviewable
  hexagonal boundary and cannot be hidden inside transfer preparation or
  synchronization. Zero DUST before registration is the expected initial
  state, not a failure.
- Role-0 NIGHT authorization and role-2 DUST custody remain distinct while
  producing the one canonical ledger intent required for registration.
- Eligibility depends on current authenticated chain parameters and explicit
  indexer facts; an older checkpoint is ignored and cannot render state or
  authorize registration until a live replay writes version two.
- A finalized registration does not imply that a local DUST checkpoint is
  current or that another transaction can spend DUST.
- Persist-before-broadcast and replay barriers apply independently to
  registration attempts without weakening existing transfer state machines.
- The prototype's useful onboarding intent is retained, while its planned-only
  status is recorded rather than overstated as migrated code.
- Durable native-custody process restart, physical-device registration and
  funded spending, a provisioned production deployment, production discovery,
  and live production-node evidence remain open after this repository decision.
