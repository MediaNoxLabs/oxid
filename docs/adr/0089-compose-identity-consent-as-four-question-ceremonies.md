# ADR-0089: Compose identity consent as four-question ceremonies

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 3–7, 9–13, 16, 18, and 21
- Design source: `docs/design/journeys.md` Add document and Present / prove journeys and `docs/design/rollout.md` Phase 2b
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet` and `mobile-bench/wallet-core`
- Tracking: issues #2, #27, #65, and #81
- Implementation state: Dioxus presents existing OpenID4VP, OpenID4VCI, and SIOPv2 authorization plans as ordered WHO → WHAT → FROM → WHY ceremonies

## Context

Oxid already exceeds the reviewed prototype's identity-protocol behavior. Its
hexagons prepare and retain bounded requests, keep protocol secrets inside
adapters, require exact confirmation intents, select managed DID methods,
verify issued credentials, require an exact presentation credential, and fail
closed when Compact presentation proving is unavailable. Dioxus previously
rendered those plans as compact definition lists followed by literal consent
checkboxes. The controls were correct, but a holder had to infer the relying
party, disclosure, source, and purpose from several differently shaped panels.

The design calls for one four-question consent anatomy across presentation,
issuance, and self-issued authentication. The current public application views
do not expose verified-domain or trust-registry results for protocol endpoints,
and the presentation command accepts the complete prepared claim plan rather
than a holder-edited optional subset. Dioxus therefore cannot honestly mark an
endpoint verified or render mutable optional-claim controls.

These protocols also have different semantics. Issuance receives a document,
OpenID4VP presents credential-derived claims, and SIOPv2 proves control of a DID
without disclosing a credential. A shared visual grammar must not collapse
those distinctions.

## Decision

Keep every domain aggregate, application use case, adapter session, confirmation
intent, and worker boundary unchanged. For each `awaiting_consent` public view,
Dioxus renders one ordered review:

1. **WHO** names the public verifier or issuer endpoint and labels it
   **Unverified endpoint**. Supporting copy states that standalone mode has no
   production trust-registry or verified-domain signal.
2. **WHAT** states the protocol-specific operation. Presentation renders each
   prepared claim as checked and disabled because all are required by the
   authorized plan. Reveal claims say that their values will be shared. The
   supported age predicate says that only the threshold result is confirmed
   and the date of birth is not shared. Issuance names every offered document;
   SIOPv2 states that DID control, not document claims, is proved.
3. **FROM** shows the existing exact credential chooser for presentation,
   identifies the active managed DID for SIOPv2, and explains the protected DID
   binding required by issuance. Multiple presentation candidates continue to
   require an explicit card selection.
4. **WHY** renders the verifier-stated purpose where the protocol supplies one.
   Issuance instead states the holder-local outcome: protected storage under
   holder control. It does not invent an issuer purpose absent from the view.

The existing literal confirmation checkbox remains after the questions and is
still required. Presentation's affirmative label becomes **Share proof**; it
executes the same exact `ACCEPT_CREDENTIAL_PRESENTATION` intent. Issuance and
SIOPv2 retain their existing affirmative labels and exact intents. Refusal
remains a peer one-tap action. Proof-unavailable, cancellation, success, and
failure states retain their current truthful application-derived copy.

## Security and architecture boundaries

- Dioxus remains an incoming adapter and cannot edit, reinterpret, or execute a
  protocol plan outside its application use case.
- Checked required-claim controls are read-only disclosure facts, not inputs.
  Optional controls remain absent until an application port can bind the exact
  holder selection into authorization and proof public inputs.
- `Unverified endpoint` means no trust signal is available. It is not an
  assertion that the endpoint is malicious or that transport validation was
  skipped.
- Raw offers, request objects, tokens, nonces, state, signatures, proofs,
  credential values, key references, and method identifiers remain absent.
- Managed DID availability and compatibility are still enforced by the same
  application/adapters at acceptance. Presentation cannot bypass the exact
  candidate chosen in the prepared session.
- Production composition remains fail closed; this decision adds no discovery,
  trust registry, network transport, or protocol format.

## Consequences

- The holder can review the same four decisions in the same order across three
  distinct protocols while their semantics and authority stay separate.
- Standalone testing visibly communicates its trust limitation instead of
  borrowing a verified visual treatment.
- Predicate disclosure becomes understandable without exposing the protected
  source value.
- The UI truthfully shows that the current presentation plan has no optional
  claims. Supporting optional selection requires a future application/domain
  decision rather than local checkbox state.
- Issuance cannot show an exact DID before acceptance without adding a public
  prepare-time binding; it therefore describes the requirement and preserves
  fail-closed method selection at the command boundary.

## Validation

- Unit tests cover reveal, supported predicate, generic predicate, and unknown
  claim copy without inspecting protected values.
- Dioxus copy, CSS vocabulary, design-token, accessibility, and architecture
  gates cover the shared ceremony styles and labels.
- iOS XCUITest and Android standalone smoke assert the ordered question anatomy,
  unverified state, negative predicate reassurance, exact credential chooser,
  issuance/login success, and fail-closed presentation proof result.
- Existing headless protocol and strict repository gates demonstrate that the
  underlying standalone flows and authority boundaries are unchanged.

## Rejected alternatives

- Marking loopback fixtures verified based on endpoint equality would conflate
  deterministic routing with a production trust decision.
- Enabling optional-looking claim switches only in Dioxus would display consent
  that the prepared command and proof plan cannot honor.
- Reusing presentation language for issuance or SIOPv2 would misstate whether a
  credential is being received, disclosed, or not involved.
- Replacing literal confirmation with the four questions alone would weaken the
  existing exact explicit-consent boundary.
