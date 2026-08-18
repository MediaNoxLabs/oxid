# Rollout Plan

Sliced to the repo's delivery discipline: each phase is independently
shippable, gated by the existing checks, and sized for the issue backlog.
No phase changes application/domain code — this is a presentation-layer
program (the two exceptions are called out).

## Phase 0 — Foundations (enables everything; also fixes live bugs)

1. **Token layer**: refactor `styles.css` into the two-layer token system
   (design-system.md); replace hardcoded literals; collapse type/radius/
   spacing scales. Includes styling the ~26 currently-undefined classes or
   migrating their pages to the shared vocabulary (the Vault page renders
   half-unstyled today — tracked as its own bug issue).
2. **Labeling layer**: the `label(...)` module with exhaustive matches for
   every user-visible enum/state; lint that fails review on raw
   `snake_case`, epoch-ms, or "base units" in rsx for user-profile surfaces.
3. **Credential chooser** (delivered by ADR-0082): the presentation flow shows
   the exact credential and requires an explicit choice when several match.

## Phase 1 — Shell & Home

Route stack; 4-tab + center-Scan shell; Home (hero, action row, card stack,
security strip, activity preview); Diagnostics folded into Settings; Vault
re-housed as a Home card + section; avatar sheet.

## Phase 2 — Journey ceremonies

Send wizard over the existing 9-state machine; receive sheet; consent sheet
(the four-question anatomy) for presentation/issuance/SIOPv2; backup
celebration flow; onboarding fork. Sync collapses into card state. Word
budgets enforced.

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
stack + shell, 1b Home, 2a send wizard, 2b consent sheet, 2c onboarding +
backup, 3 white-label infra, 4a secret mode + native privacy ops, 4b dev/demo
profiles, 5 per-item. Each slice references the relevant spec section as its
acceptance criteria, factory-work-item style.
