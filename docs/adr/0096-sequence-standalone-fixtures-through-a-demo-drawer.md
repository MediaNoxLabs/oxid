# ADR-0096: Sequence standalone fixtures through a demo drawer

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 3–7, 12–13, 16, 18, and 21
- Design source: `docs/design/ui-profiles.md` P1, P2, P5, P8, P9, and P10; rollout Phase 4b
- Tracking: issues #2, #65, and #88
- Amends: ADR-0029
- Related: ADR-0069, ADR-0077, ADR-0089, and ADR-0095
- Implementation state: an opt-in standalone-development profile renders a truthful fixture drawer, sequences existing safe use cases, stops identity fixtures at existing review screens, and is excluded from normal release artifacts

## Context

Oxid already ships repeatable standalone wallet, Midnight, DID, credential
inbox, OpenID4VCI, SIOPv2, and OpenID4VP fixtures. They are deliberately spread
across the normal product journeys because the same application boundaries are
used by mobile and headless conformance. Recreating them in a privileged demo
service, adding a generic command channel, or changing a retained state machine
would make a screenshot convenient by weakening the architecture it is meant to
demonstrate.

The drawer must also remain truthful when a standalone-development binary is
started with live environment configuration. A UI profile cannot select
composition. In particular, the banner cannot imply that arbitrary app activity
is offline and a demo action cannot synchronize an account before proving that
the current public source is the deterministic undeployed simulator.

Identity fixtures cross consent boundaries. Loading an offer, login request, or
presentation request is safe; accepting it is not. The demo profile must not
turn preparation into consent or infer that a successful fixture bootstrap
means wallet, custody, proving, or chain readiness.

## Decision

Add the dependency-free `ui-profile-demo` Cargo feature to the app and Dioxus
incoming adapter. It requires `standalone-development`, is rejected with normal
production composition and native standalone custody, and is mutually exclusive
with `ui-profile-dev`. Selection remains a build feature; there is no runtime or
environment-selected UI profile.

The feature adds a persistent non-dismissible banner and a dismissible,
accessible bootstrap drawer. The banner says that fixture data is in use and
that the demo setup itself contacts no chain. The drawer repeats that it never
consents, authorizes, proves, submits, or marks wallet readiness.

The drawer calls only the existing typed Dioxus services:

1. keep the active profile only when it is the named `Oxid Demo Wallet`;
   otherwise select an existing profile with that exact display name or create
   and select it, leaving every unrelated active profile untouched;
2. initialize an uninitialized standalone wallet, unlock a locked session, or
   keep an unlocked session;
3. idempotently derive account/address index `0/0` through protected custody;
4. keep an active managed authentication DID or create one undeployed managed
   DID;
5. verify and upsert the existing public standalone inbox credential;
6. synchronize the public funding fixture only after the existing account view
   reports the exact closed triple `source=simulated`, `networkId=undeployed`,
   and `environment=development` (`undeployed` is the network identifier; the
   wallet domain classifies it as a development environment).

The full-setup button sequences those six operations. Its stop action is a
request to stop between application operations: ADR-0077 operations are not
force-cancelled and the UI says that the current typed use case will finish.
While any typed operation is running, the drawer serializes all other actions
and cannot be closed. Failures retain a per-action retry. Repeating successful
operations keeps the named demo profile, unlocked wallet, derived account,
managed DID, and upserted credential wherever their existing idempotency
permits.

Credential-offer, SIOPv2 login, and OpenID4VP presentation buttons retrieve
only the existing composition-owned fixture URI. Each URI is classified by the
existing strict identity-request router, occupies the same one-item pending
review handoff, and opens the existing review route. The drawer then closes and
marks that action `review required`. It never calls an accept/refuse,
authorization, proving, submission, or confirmation-bearing use case. Full
setup ends at the credential-offer review; login and presentation remain
separate explicit actions after that pending review is completed or dismissed.
A pending exact identity review blocks every demo action, including profile,
DID, inbox, and funding changes, so the reviewed context cannot be changed from
the drawer.

The normal release gate builds the actual app artifact and rejects the stable
demo profile, drawer, profile-name, and full-setup markers. It also proves the
feature fails with production or native-standalone composition.

## Consequences

- Presenters can create a repeatable wallet/identity starting point without a
  privileged service or secret-bearing fixture payload in Dioxus.
- Every visible consent object and exact intent remains owned by its existing
  issuance, login, presentation, custody, or transaction state machine.
- A live/cached source, non-`undeployed` network identifier, or non-development
  environment fails the funding action before sync. The
  demo profile does not silently replace that composition with simulation.
- The drawer reports `ready`, `working`, `complete`, `review required`, and
  retryable failure independently for every action, plus honest full-run stop,
  failure, and review states. It admits only one operation at a time and blocks
  every new operation while an exact identity review is pending.
- The profile is not available with production/native custody and its code and
  stable fixture labels are absent from the normal release binary.
- Completing the drawer is setup evidence only. It is not a production
  capability, custody, proof, settlement, or `wallet.bootstrap` readiness fact.

## Validation

- Focused Dioxus tests pin the closed nine-action order, six safe automatic
  actions, three review boundaries, distinct progress states, honest stop copy,
  serialized operation admission, pending-review exclusion, modal inertness,
  and exact simulator/undeployed funding gate.
- App checks prove the allowed standalone-development feature graph and reject
  production, native-standalone, and combined dev/demo graphs.
- UI CSS/token/copy gates cover the banner, drawer, progress, focus, disabled,
  status, and alert states.
- The release gate scans the normal optimized app binary for the stable demo
  profile, drawer, profile-name, and full-setup markers.
- The Android arm64 fresh-install flow on `emulator-5554` proves the banner
  before onboarding, modal semantics/background isolation, all six safe setup
  actions, the unchanged credential-offer review, and absence of automatic
  consent or acceptance.
- Clean demo artifacts build, install, launch, and visibly render before
  onboarding on the iOS 26.4 iPhone 17 Pro and iOS 17.5 iPhone 15 Pro
  simulators. Xcode 26.4 did not expose either WKWebView accessibility subtree
  to XCTest on this host, so the authored iOS interaction test remains an
  explicit local toolchain evidence item rather than a claimed pass.

## Rejected alternatives

- A demo composition selected by the UI feature would violate profile
  orthogonality and make presentation decide wallet authority.
- Calling accept methods with canned confirmations would bypass exact user
  consent and invalidate the demo.
- A generic JavaScript/native command bridge would widen the attack surface and
  duplicate the typed application boundary.
- Copying fixture credentials, keys, or protocol secrets into Dioxus would make
  the incoming adapter a secret-bearing store.
- Labeling a stopped operation `cancelled` would overstate ADR-0077; only the
  sequencer is stopped between operations.
- Synchronizing any account labelled live, cached, or non-undeployed would make
  the offline demo claim false.
