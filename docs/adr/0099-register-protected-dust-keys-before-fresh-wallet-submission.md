# ADR-0099: Register protected DUST keys before fresh-wallet submission

- Status: Accepted
- Date: 2026-08-20
- Source: Blueprint §§3–8, 12–13, 16–18, 21; issue #92; reviewed
  prototype onboarding plan; accepted Midnight ledger registration semantics
- Prototype source: `midnight-ledger` commit
  `074b1a4bccbfee1740ee188374b606a022ecef42`
- Ledger source: `midnight-ledger` commit
  `d9414884db9da9e9b1f6f3a7f742d79a5732f817`
- Implementation state: Repository and headless implementation complete;
  guarded funded preprod, mobile, process-restart, physical-device, and
  production live-node evidence remain open

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

The guarded preprod acceptance flow derives two test-only accounts from an
externally provisioned 32-byte master seed by using the existing hardened
Midnight BIP44 account index: account A is `2 * caseIndex`, and account B is
`A + 1`. The seed is accepted only as exactly 64 hexadecimal characters from a
secret environment variable after a second explicit live-test opt-in. It is
never accepted over a headless command or written, logged, hashed for output,
or committed. The public funding manifest may expose only the repository
commit, preprod network, case and account/address indices, and A/B public NIGHT
and shielded receive addresses. DUST is never externally funded.

After out-of-band NIGHT/shielded funding, the flow must prove the fresh account
starts with zero DUST, use the same public prepare, explicit consent, protected
authorization, official proving/finality, reconciliation, and DUST-observation
boundaries as any other caller, wait for generated DUST to become recoverable
and fully synchronized, and only then perform A's shielded spend to B. Every
case index is single-use unless an exact, explicit recovery-resume mode is
being tested. Test custody, funding code, environment markers, and fixtures
remain excluded from normal release artifacts.

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
