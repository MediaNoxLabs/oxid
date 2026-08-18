# ADR-0082: Require explicit presentation credential selection

- Status: Accepted
- Date: 2026-08-18
- Blueprint source: Sections 3–7, 9–13, 16–18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`; its mobile presentation action is disabled and supplies no reviewed multi-credential consent flow
- Tracking: issues #2 and #64
- Amends: ADR-0010 and ADR-0043
- Implementation state: presentation previews expose bounded claim-free credential name, issuer, and opaque identifier metadata; Dioxus auto-selects only a sole match, requires an explicit card choice for multiple matches, resets consent when the choice changes, and passes the exact selected identifier to the existing application use case

## Context

The presentation application already requires acceptance to name one candidate
identifier from the prepared, profile-scoped request. The headless adapter also
requires that identifier. The Dioxus incoming adapter weakened this contract by
always passing `candidates[0]`, even when the preview contained several matching
credentials. A user could therefore approve the verifier and requested claims
without knowing which credential the wallet would use.

Display name alone is insufficient when several Digital Passports match. The
preview needs enough public metadata to identify each credential without
revealing claim values, openings, holder keys, proof material, protocol state,
or tokens.

## Decision

Keep candidate validation and exact acceptance in `presentation/application`.
Extend the schema-neutral candidate domain value and safe view with its issuer.
The preview may expose only the bounded opaque credential identifier, display
name, and issuer that are already public credential-inventory metadata.
Headless includes the issuer as an additive preview field and continues to
require the exact `credentialId` on acceptance.

The Dioxus presentation panel renders every matching candidate as a radio-card
choice containing the credential name, shortened issuer, and shortened opaque
reference. A sole candidate is selected automatically but remains visible.
Two or more candidates begin with no selection. The consent checkbox and
affirmative action stay disabled until one candidate is selected. Changing the
selection clears any prior consent. Acceptance reads the selected identifier;
indexing, first-match fallback, and hidden default selection are forbidden.

Candidate metadata remains bounded and claim-free. It must not be logged or
treated as proof of holder authority. Presentation-time credential validation,
current holder authorization, Compact proving, and independent verification
remain unchanged and fail closed at their existing boundaries.

## Consequences

- Consent answers which exact stored credential will be used as well as who is
  requesting what and why.
- Identical names and issuers remain distinguishable by their shortened opaque
  references without exposing protected credential contents.
- The Dioxus adapter no longer invents selection policy; it preserves the exact
  candidate identifier already required by the application hexagon.
- Production transport and mobile Compact proving remain separately gated.

## Validation

- Domain and application tests cover bounded issuer metadata, multiple
  candidates, rejection of an unlisted identifier, and forwarding of the exact
  selected candidate.
- The OpenID4VP adapter test produces two matching credentials while retaining
  claim-free debug output.
- UI tests prove only a sole candidate is selected automatically.
- iOS and Android standalone smokes issue two distinct matching Digital
  Passports, assert presentation consent is initially disabled, choose the
  second credential, and then reach the existing fail-closed mobile proof gate.
