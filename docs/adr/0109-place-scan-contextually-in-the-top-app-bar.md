# ADR-0109: Place Scan contextually in the top app bar

- Status: Proposed
- Date: 2026-09-13
- Design source: issue #349; `docs/design/information-architecture.md`
- Amends: ADR-0086 and ADR-0087
- Implementation state: research only; no UI change is authorized

## Context

ADR-0086 puts Scan in the center of the bottom bar and ADR-0087 repeats it in
Home quick actions. Scan is an action, not a destination, and the current
scanner admits only strictly classified identity requests. The current
identity router recognizes credential issuance, credential presentation, and
self-issued authentication. It rejects payment QR, network-profile QR, and
arbitrary QR data. The existing `QrScannerPort` also has closed cancelled,
denied, unavailable, timed-out, invalid, and failed outcomes.

The current route implementation has four primary destinations: Home, Wallet,
Documents, and Activity. A successful classified scan moves the root to
Documents and pushes either credential review or DID-login review. It does not
accept consent. A rejected or unavailable scan leaves the route unchanged; a
pending request remains subject to the one-pending-request rule. Back pops a
secondary review only, and dismissal removes that review without changing the
application state machine.

## Research basis

The proposed placement is consistent with primary platform and accessibility
guidance, while the exact control remains an Oxid presentation decision:

- [Apple Human Interface Guidelines: Navigation and
  search](https://developer.apple.com/design/human-interface-guidelines/navigation-and-search)
  treats navigation as a way to move among destinations and places actions in
  the relevant bar or context rather than making every action a destination.
- [Material 3: App bars](https://m3.material.io/components/app-bars/overview)
  describes the top app bar as the place for screen identity and contextual
  actions, with overflow for less frequent actions.
- [WCAG 2.2, SC 2.5.8 Target Size (Minimum)](https://www.w3.org/TR/WCAG22/#target-size-minimum)
  requires a 24 by 24 CSS-pixel minimum for pointer targets, subject to its
  exceptions. Oxid will use a 44 by 44 CSS-pixel touch target for Scan, also
  aligning with [SC 2.5.5 Target Size
  (Enhanced)](https://www.w3.org/TR/WCAG22/#target-size-enhanced) where the
  target is author-sized.

These sources justify contextual placement and target sizing; they do not
claim that payment or network QR is supported.

## Decision

**Proposed:** replace the persistent center-bar and Home quick-action Scan
controls with one labeled Scan action in the top app bar on scan-capable
surfaces. Scan remains an action using the existing `QrScannerPort` and
`RouteIdentityRequestUseCase`; this ADR does not add a route, scanner format,
protocol, consent, or application capability.

The visibility contract is deliberately closed:

| Current surface/state | Scan control | Result |
| --- | --- | --- |
| Home, Wallet, Documents, Activity with scanner capability and no scan running | Visible in top app bar | Starts one scan |
| Same surfaces while a scan is running | Visible but disabled and announced busy | No second scan |
| Same surfaces with unavailable scanner composition | Visible only if the surface can explain the unavailable capability; otherwise omitted by the existing capability policy | Never pretends to scan |
| Credential/DID review with a pending request | Not added as a second entry point | Existing review and one-pending-request rule win |
| Settings, Profile, Passport Vault, Diagnostics, backup/recovery, and developer routes | Hidden | No contextual scan affordance |
| Compact/mobile layout | Same single top-bar control, 44 x 44 CSS-pixel minimum target, accessible name “Scan identity QR code” | No bottom-bar duplicate |
| Cancel, denial, invalid/unsupported, timeout, or failure | No new control or route | Existing payload-free outcome; current route/pending state is preserved |
| Classified success | No new control or route | Existing Documents-root transition and review route |

A future extension point may be introduced only as a typed, closed capability
owned by the application boundary, for example:

```text
ScanCapability = IdentityRequest(IdentityRequestKind)
               | PaymentRequest(PaymentRequestKind)
               | NetworkProfile(NetworkProfileId)
```

This is a design seam, not a current enum or implementation request. Each
future variant requires its own strict classifier, security review, route and
consent contract, capability advertisement, and visibility-matrix entry.
Unknown or unclassified payloads remain rejected and are never routed through
an identity variant.

Top-bar placement does not change profile switching, Back, overflow, pending
reviews, developer-only route reachability, or route-stack ownership. Overflow
may contain Settings/profile actions as it does today, but Scan is not hidden
there on a surface where it is advertised. The control must have a programmatic
name, keyboard/assistive-technology access, visible focus, and no icon-only
meaning.

## Amendments to ADR-0086 and ADR-0087

If accepted, this ADR supersedes only their Scan placement clauses:

- ADR-0086's bottom-bar list changes from four destinations plus center Scan to
  the four destinations only; its shared scanner, strict routing, pending
  request, and review transitions remain unchanged.
- ADR-0087's Home anatomy retains Scan as a quick action semantically, but the
  repeated Home Scan control is removed; Home invokes the same contextual
  top-bar action and retains all other projection boundaries.

All other decisions, including the bounded route stack, Wallet/Documents
ownership, explicit consent, fail-closed unavailable behavior, and no
production implementation before owner acceptance, remain in force.

## Consequences and non-goals

- One contextual action removes duplicate entry points and preserves navigation
  space for destinations.
- Scan is discoverable on the four current primary surfaces without implying
  that it is a destination.
- Contextual visibility must be tested across capability, busy, pending, and
  compact-layout states.
- This ADR does not implement the move, add payment/network support, alter QR
  classification, add telemetry, or change native scanner behavior.

## Validation required after acceptance

Documentation contracts must validate the ADR index, Markdown links, and the
closed matrix. A later implementation must add UI tests for one-control
visibility, disabled busy behavior, accessible name/target size, unchanged
review routing, and hidden secondary/developer surfaces. The focused gate must
also verify that ADR-0086/0087 are not silently edited to Accepted or otherwise
rewritten by this research slice.
