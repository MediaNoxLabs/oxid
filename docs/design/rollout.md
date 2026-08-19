# Rollout Plan

Sliced to the repo's delivery discipline: each phase is independently
shippable, gated by the existing checks, and sized for the issue backlog.
Most phases change only presentation. ADR-0090 adds the one application fact
needed to make a successful complete-backup claim truthful; later native screen
privacy operations remain the other called-out exception.

## Phase 0 — Foundations (enables everything; also fixes live bugs)

1. **Token layer** (delivered by ADR-0084 / issue #67): `styles.css` now uses
   the two-layer token system from `design-system.md`; component palette
   literals and ad-hoc type/radius/motion values are rejected by the repository
   gate, spacing is collapsed to the eight-step scale, and both dark/light
   brand schemas are complete while dark remains the selected scheme. Issue
   #63 previously unified the Vault compatibility vocabulary.
2. **Labeling layer** (delivered by ADR-0085 / issue #77): the centralized
   Dioxus label/format module explicitly names every known user-visible
   enum/state and hides unknown values; the repository gate rejects direct
   machine-field interpolation, underscore replacement, cursor prose,
   epoch-millisecond copy, and raw subunit terminology in user-profile rsx.
3. **Credential chooser** (delivered by ADR-0082): the presentation flow shows
   the exact credential and requires an explicit choice when several match.

## Phase 1 — Shell & Home

1. **Route stack and shell** (delivered by ADR-0086 / issue #78): Dioxus now
   owns a bounded root-plus-secondary stack; the bottom bar is Home, Wallet,
   center Scan, Documents, and Activity; the hamburger and primary
   Vault/Diagnostics/Settings destinations are retired; every migrated flow
   remains reachable through the reviewed hierarchy.
2. **Home composition** (delivered by ADR-0087 / issue #79): a safe read-only
   account hero, four quick actions, horizontally scrollable product cards,
   truthful security-capability strip, and bounded transaction preview replace
   the temporary complete-Assets duplication. Wallet retains all operational
   controls; Home owns no application transition.

## Phase 2 — Journey ceremonies

1. **Send wizard** (delivered by ADR-0088 / issue #80): two bounded editable
   screens lead into exact preview-derived review, separate device
   authorization and prove/submit intents, and truthful sending/confirmed/
   failure recovery over the unchanged nine-state machine. Clipboard import,
   payment scanning, and recent recipients remain follow-up ports rather than
   inert controls.
2. **Identity consent** (delivered by ADR-0089 / issue #81): presentation,
   issuance, and SIOPv2 use one ordered WHO → WHAT → FROM → WHY review while
   preserving their distinct semantics, exact confirmation intents, explicit
   refusal, managed custody, and fail-closed proof gates. Standalone endpoints
   are explicitly unverified and all current presentation claims are locked as
   required because no optional-claim authorization port exists.
3. **Onboarding, backup, and sync** (delivered by ADR-0090 / issue #82): fresh
   installs fork into create or restore, profile creation offers skippable
   device protection without exposing an opaque identifier, and recovery keeps
   its existing empty-install confirmation. A profile-scoped application
   receipt is recorded only after complete encryption and native document
   export both succeed, enabling truthful **Backed up** copy and an accessible
   celebration. Wallet now composes account, DUST, and shielded refresh into
   one sync card/action while their independent authority and cancellation
   semantics remain unchanged. The receive sheet remains a later ceremony.

## Phase 3 — White-label infrastructure

`crates/brand-build` + `brands/oxid/` as the first pack; contrast gate;
per-brand snapshot tests of security copy; Nix brand enumeration; the
non-brandable-surface ADR. (Build-layer change, no runtime code.)

## Phase 4 — UI profiles

Secret mode (toggle + matrix + auto-re-arm); the two mobile-native bridge
operations for FLAG_SECURE / iOS privacy overlay (the second exception:
a small, typed, reviewed native addition); dev capability viewer; demo
bootstrap drawer; the two profile ADRs (P1–P5 policy; native screen-privacy
operations); release-CI guard for profile features.

## Phase 5 — Delight & polish (owner-gated)

Mascot (see open questions), achievements for security hygiene, first-proof
/first-credential celebrations, jar-style shareable payment requests,
global search across assets/documents/activity, light theme ship.

## Acceptance gates (all phases)

- Existing state machines and consent semantics untouched (review checklist
  item + the security-copy snapshot tests from Phase 3 onward).
- Tap budgets and word budgets from README.md verified in smoke flows.
- All four component states (loading/empty/error/populated) present.
- a11y checklist (design-system.md) per new component.
- `just check` green; no new external UI dependencies without a dependency
  review doc.

## Open product questions (owner decisions, not blockers)

1. **Mascot**: adopt a monobank-style functional mascot (loading/status/
   achievements)? Proposal on the table: a character whose pattern derives
   from the profile's DID fingerprint (doubles as an anti-phishing visual
   anchor). Brandable asset set or Oxid-only?
2. **Product naming**: user-facing "Documents" (EUDI-aligned) vs
   "Credentials"; "Wallet" vs "Assets" for the money tab. Spec assumes
   Documents/Wallet.
3. **Light theme priority**: schema supports it from Phase 0; when do we
   ship the toggle — Phase 2 or 5?
4. **Gamification depth**: security-hygiene achievements only (spec's
   assumption) or monobank-style collectibles?
5. **Ukrainian localization timing**: the label layer is the i18n seam from
   Phase 0; when does uk-UA content land?

## Suggested backlog decomposition

One epic issue (this spec), then per-phase slices sized like existing
issues: 0a tokens, 0b labeling layer, 0c credential chooser (bug), 1a route
stack + shell (delivered by #78), 1b Home (delivered by #79), 2a send wizard
(delivered by #80), 2b consent sheet, 2c onboarding + backup, 3 white-label
infra, 4a secret mode + native privacy ops, 4b dev/demo
profiles, 5 per-item. Phase 2c is delivered by #82; each remaining slice references the relevant spec section as its
acceptance criteria, factory-work-item style.
