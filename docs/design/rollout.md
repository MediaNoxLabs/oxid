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
   semantics remain unchanged.
4. **Receive sheet** (delivered by ADR-0091 / issue #83): Home opens a bounded
   secondary sheet in one tap. Only protected derived address rails returned by
   the existing account use case become human-labelled Public/Private/Fee
   selectors; the selected full value alone feeds the large QR and typed native
   Copy/Share ports. Fixture and watch-only addresses remain unavailable until
   activation, and Close restores Home without changing wallet state.

## Phase 3 — White-label infrastructure

Delivered by ADR-0092 / issue #84: `crates/brand-build` plus `brands/oxid/` as
the first pack, closed metadata/token and safe-SVG validation, two-scheme WCAG
contrast gates, default-brand security-copy snapshots, exact thin-app manifest
checks, and automatic `run.sh`/Nix brand enumeration. Brand selection remains a
thin-app build decision; no runtime brand configuration was added.

## Phase 4 — UI profiles

1. **Secret mode and native screen privacy** (delivered by ADR-0093/0094 and
   issue #85): every build defaults to a render-only matrix mask, one explicit
   reveal auto-re-arms after 30 seconds/background/unlock, exact consent stays
   visible, and one boolean platform port sets Android `FLAG_SECURE` or an
   honest iOS scene-background overlay. Settings/credential routes force host
   protection; physical-device evidence remains issue #32.
2. **Developer profile** (ADR-0095 / issue #87): one UI-neutral closed
   capability manifest feeds both `system.capabilities` and the opt-in
   standalone Dioxus viewer; confirmation declarations, build feature guards,
   persistent profile/composition copy, and a normal-release binary marker scan
   are repository gates. Prototype free-form logs, process statistics, HTTP
   histograms, and timing telemetry remain excluded. iOS/Android smoke evidence
   closes the slice.
3. **Demo profile** (ADR-0096 / issue #88): the opt-in
   standalone-development drawer isolates setup in the named demo profile and
   sequences existing custody,
   derivation, managed-DID, inbox, and exact simulated-funding use cases. Full
   setup then pauses at the unchanged credential-offer review; login and
   presentation fixtures use the same strict one-item router and never automate
   consent. Per-action/full-run progress, honest stop/retry states, feature
   guards, focused mobile checks, and normal-release marker exclusion are
   repository gates.
4. **Physical standalone evidence** (ADR-0097): the development-only tailnet
   build now derives and synchronizes a protected account through the real
   laptop stack, persists only public account coordinates, and proves with a
   physical Android tap that the contained Scan navigation target cannot steal
   the wallet activation action. The separate compile-time localhost profile
   sends iOS Simulator directly to loopback and gives Android emulator only the
   three required `adb reverse` mappings; focused live-account flows distinguish
   real synchronization from deterministic simulation.
5. **Authenticated deployment and funded finality** (ADR-0098 / issue #90): a
   signed atomic profile binds Midnight genesis/network/routes and SSI metadata
   routes behind audience, validity, revocation, and rollback checks; the node
   must prove the exact genesis hash before opt-in production composition. No
   production root/profile is selected. A separate external-seed guarded
   headless flow proves funded unshielded authorization, DUST proving,
   finalized inclusion, adapter reconstruction with included-status
   restoration, bounded indexer convergence, and no duplicate recipient
   delivery while remaining absent from releases.
6. **Funded shielded finality and nullifier-safe adapter reconstruction** (ADR-0079/0098 /
   issue #91): a second double-opt-in headless flow synchronizes real native
   Zswap genesis notes, uses the shared preview/consent/DUST+Zswap proof and
   finalized-submission lifecycle, blocks an unchanged-state duplicate, then
   reconstructs the adapter and proves exact sender/recipient balances after
   nullifier replay. Live evidence corrects the v4 `ZswapLedgerEvent` typename
   and sparse cursor contract. Fresh-wallet funded registration, later
   generation/recovery, and origination evidence remain issue #92; journal
   compaction after 128 retained barriers remains issue #93.
7. **Protected DUST registration and fresh-wallet origination** (ADR-0099 /
   issue #92, repository/headless implementation complete): a distinct
   registration port and prepare/consent/authorize/submit ceremony. Planning
   uses only live owned
   unregistered NIGHT with indexer creation-time evidence, returns exact NIGHT
   to the same owner, and limits the guaranteed offer and fee allowance to the
   largest generated candidate. Role-0 authorizes while the role-2 DUST child
   stays in protected custody. Generic proving, registration-separated durable
   recovery, finalized inclusion, and later official DUST-event spend readiness
   remain distinct. A fresh wallet intentionally starts at zero DUST; only
   later generation and authoritative recovery make it spend-ready. The
   guarded preprod flow is test-only; funded preprod, durable native restart,
   physical-device, and production evidence remain open.

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
(delivered by #80), 2b consent sheet (delivered by #81), 2c onboarding + backup
(delivered by #82), 2d receive sheet (delivered by #83), 3 white-label infra
(delivered by #84), 4a secret mode + native privacy ops (delivered by #85),
4b1 developer profile (delivered by #87), 4b2 demo profile (delivered by #88),
4c authenticated deployment/funded finality (#90), 4d funded shielded finality
(#91), 4e protected DUST registration/fresh-wallet shielded origination
(ADR-0099/#92, repository/headless implemented; funded evidence open),
5 per-item. Each remaining slice references the relevant spec section as its
acceptance criteria, factory-work-item style.
