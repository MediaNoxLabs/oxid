# ADR-0086: Compose the mobile shell with a bounded route stack

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 1, 3–7, 12–13, 16, and 18
- Design source: `docs/design/information-architecture.md`, `docs/design/journeys.md` §9, and `docs/design/rollout.md` Phase 1a
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/src/app.rs`
- Tracking: issues #2, #65, and #78
- Implementation state: Dioxus owns a bounded root-plus-secondary route stack, four primary destinations, the elevated Scan action, and explicit Back behavior while every migrated page remains reachable

## Context

The prototype-derived Oxid shell selected one of seven flat destinations with a
signal. Six of those destinations also appeared in a fixed bottom bar, all
seven were repeated in a hamburger menu, and QR or app-link ingress mutated the
selected tab. Passport Vault, Diagnostics, and Settings therefore had the same
global weight as the wallet and document jobs, and a secondary review had no
route to return from.

The accepted information architecture requires four primary destinations plus
one center Scan action. It also requires Passport Vault below Home, DID
management below Documents, and diagnostics below Settings. This is a
presentation decision: the existing application use cases, pending-request
rule, consent state, and adapter capabilities remain authoritative.

Adding a URL router for this local mobile hierarchy would introduce a new UI
dependency and browser-history policy before Oxid has a concrete public-route
or desktop deep-link requirement. The existing signal is sufficient if its
state and transitions become explicit and testable.

## Decision

Keep navigation inside the Dioxus incoming adapter as a bounded `RouteStack`.
The first entry is exactly one primary route and later entries are secondary
routes. Selecting a primary destination clears the secondary stack; opening a
secondary surface pushes once; Back pops only while a secondary route exists.
Reopening a secondary route already in the stack truncates back to that entry
instead of duplicating it, so the closed route vocabulary also bounds stack
depth. No route state is persisted and no route owns wallet or identity
behavior.

The bottom bar contains, in order:

1. Home;
2. Wallet;
3. Scan, as an action rather than a destination;
4. Documents;
5. Activity.

The Scan action keeps the existing `QrScannerPort` and
`RouteIdentityRequestUseCase`. A credential offer or presentation request
resets the root to Documents and pushes its credential-review route. A
self-issued authentication request resets the root to Documents and pushes its
DID-review route. App links use the same transition. Neither path grants
consent, and the one-pending-request rule is unchanged. Explicitly dismissing a
pending request removes it and pops its review route without changing an
application state machine.

Secondary placement is fixed as follows:

- Passport Vault opens from a Home product card.
- DID management opens from Documents.
- The avatar sheet opens wallet profiles or Settings.
- Diagnostics opens from Settings.
- Profile selection returns to Home.

Phase 1a deliberately keeps the complete account view on Home and Wallet so
the cutover cannot hide receive, sync, send, balance, or transaction-recovery
behavior. Activity renders the existing synchronized Midnight transaction
projection and submission recovery. Phase 1b will split the Home summary from
the Wallet detail view and extend Activity with identity and Vault events.

The top-bar secret-mode eye remains absent until Phase 4 defines masking and
native screen-privacy policy. A decorative or non-functional eye would imply a
security property that the application does not yet provide.

## Security and architecture boundaries

- `RouteStack` contains only closed presentation enums. It never contains a
  URI, credential, claim, transaction payload, proof, key reference, or error
  body.
- Routes render existing Oxid-owned views and call existing incoming use-case
  traits. They do not call storage, Midnight, SSI, protocol, or OS adapters.
- Scan and app-link routing still validate at their existing boundaries and
  only publish a bounded pending request after classification.
- Back changes presentation location only. It does not cancel, retry, submit,
  authorize, or discard an application state machine.
- Diagnostics remains payload-free and subordinate to Settings; moving its
  entry point does not expand ADR-0080.
- Home and Wallet sharing an account view is an explicit migration state, not
  permission to duplicate application state or adapter composition.

## Consequences

- The primary shell now has four jobs and one universal ingress action without
  losing any migrated capability.
- Secondary flows have deterministic Back behavior and can later become
  full-page wizards without changing the application layer.
- Dioxus Router or another navigation dependency remains deferred until a
  concrete URL/history requirement justifies its review.
- Home and Wallet intentionally overlap for one delivery slice. ADR-0086 must
  not be read as the final Home anatomy; issue #65 Phase 1b owns that split.
- Android system-back interception is not implied by the stack. The shared
  shell exposes an explicit Back control; a future typed native bridge may map
  the platform event to the same `pop` transition.

## Validation

- Dioxus unit tests cover primary order, stack reset/pop, secondary routes,
  and identity-ingress routing.
- The CSS class, design-token, and user-copy gates cover the new shell.
- iOS XCUITest and Android CDP harnesses navigate the full standalone wallet,
  DID, credential, Vault, backup, and native-custody flows through the new
  hierarchy.
- `just ios-smoke` remains the Tier-1 interactive acceptance gate for the
  ordinary standalone composition.

## Rejected alternatives

- Retaining the six-tab bar and hamburger would preserve the ambiguity this
  phase exists to remove.
- Treating Scan as a fifth destination would add a blank page and weaken its
  role as a universal action.
- Removing hidden pages until later phases would regress migrated behavior and
  break the staged-parity contract.
- Adding Dioxus Router now would add dependency and URL policy without a
  concrete requirement that the bounded stack cannot satisfy.
- Showing a non-functional secret-mode toggle would overstate privacy
  protection.
