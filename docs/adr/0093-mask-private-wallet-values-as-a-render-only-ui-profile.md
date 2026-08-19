# ADR-0093: Mask private wallet values as a render-only UI profile

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 1–7, 9–13, 16–18, and 21
- Design source: `docs/design/ui-profiles.md` P1–P5, rollout Phase 4a
- Tracking: issues #2, #65, and #85
- Implementation state: every Dioxus build starts in a masked resting state, permits one explicit timed reveal, re-arms on lifecycle and wallet-unlock boundaries, and keeps exact consent/authorization objects visible

## Context

Oxid already keeps secrets behind opaque custody and exposes only reviewed
public view models to Dioxus. Those boundaries do not prevent a nearby person,
screen share, or app-switcher preview from observing a rendered balance,
address, DID, claim, or transaction value. A privacy affordance must not become
another wallet mode or acquire authority over adapters, storage, endpoints,
fixtures, custody, or protocol behavior.

Authorization creates the opposite requirement: masking an amount, recipient,
credential choice, or requested claim while asking for consent would make the
confirmation ambiguous.

## Decision

Define secret mode as Dioxus-owned presentation state. It starts masked in
every build and is controlled by one accessible eye action in the top bar. An
explicit reveal lasts at most 30 seconds. Every reveal receives a generation;
a stale timer cannot hide or extend a newer reveal. Background, resume, and a
successful wallet initialization or unlock increment the generation and
immediately restore masking. The state is process-local and is never persisted.

Mark only reviewed private display elements with `privacy-value` or
`privacy-qr`. While the root is masked, CSS replaces their visible text with a
layout-stable four-dot marker and obscures QR images. Holding an individual
value or QR reveals that rendered element only for the duration of the physical
press. The global eye action is the only way to reveal all marked values.

The matrix covers NIGHT, DUST, shielded and Passport Vault amounts; receive
addresses and QR images; non-consent DID identifiers and method references;
locally revealed credential claim values; and activity amounts, counterparties,
and transaction references. Profile names, capability labels, human status
copy, and non-value controls remain visible.

Masking consumes only strings already returned to the UI. No application DTO,
use-case result, adapter input, diagnostic event, persisted record, or headless
response changes. It is explicitly a visual shoulder-surfing affordance, not a
claim that public view strings are absent from memory or the accessibility tree.

Transfer, Passport Vault, SIOPv2, OpenID4VCI, and OpenID4VP review surfaces do
not carry masking classes. They render their exact authorization objects and
state `Details shown for authorization.` Secret mode therefore yields to the
existing confirmation and consent state machines without changing them.

## Consequences

- The same profile, account, DID, credential, transaction, and protocol
  services feed masked and revealed UI states byte-for-byte.
- A fresh launch and every return from background prefer privacy over retaining
  a prior reveal.
- CSS marking is an explicit review obligation for each new private display
  surface. Repository tests and mobile smoke flows must catch matrix drift.
- Accessibility clients can still receive the reviewed public view string.
  A future accessibility-specific privacy policy requires a separate decision;
  silently hiding semantics would make controls unusable.
- Dev and demo profiles may add metadata later, but they cannot weaken this
  policy or reveal data unavailable to the user profile.

## Validation

- Pure state tests cover the masked default, fresh reveal generations, stale
  timeout rejection, and current-timeout re-arming.
- The Android WebView smoke verifies the root state, computed visual mask,
  explicit reveal, and background re-arm before running the unchanged wallet
  journey.
- The isolated iOS UI test verifies the accessible toggle and background
  re-arm. Existing journey assertions continue to prove exact consent objects.
- Strict UI copy/token gates, workspace tests, coverage, and both mobile builds
  remain mandatory.

## Rejected alternatives

- Filtering or replacing values in application DTOs would couple privacy
  presentation to business behavior and could corrupt authorization inputs.
- Persisting a globally revealed state would leak across restart and make
  background protection timing-dependent.
- Masking consent or transaction review would trade shoulder-surfing privacy
  for authorization ambiguity.
- A separate secret-mode composition feature would incorrectly let
  presentation choose adapters or custody.

