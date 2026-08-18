# UI Profiles: user / dev / secret / demo

A UI profile is a **presentation policy**. It decides what is rendered,
masked, hidden, or offered as navigation. It never decides what is wired —
composition modes (fail-closed `compose()`, standalone-development,
native-standalone) remain the only authority over adapters, custody, and
data, exactly as AGENT.md demands.

## The rules

- **P1 — Orthogonality.** Profiles live in `crates/ui-dioxus` and change
  rendering only. No profile may select storage, simulation, endpoints, or
  custody. (Composition modes already do that under `compile_error!`
  guards; profiles compose *with* them.)
- **P2 — Selection.** `user` is the only profile in a default build; no
  flag. `dev` and `demo` are cargo features of the app crate
  (`ui-profile-dev`, `ui-profile-demo`) that **require a standalone
  composition feature** — `compile_error!` otherwise — so neither can exist
  in a production build. Release CI additionally asserts the distributed
  artifacts contain neither feature.
- **P3 — Secret mode.** A runtime quick toggle in every build (the eye in
  the top bar, ≤ 2 taps from anywhere). Masks by the matrix below and
  **auto-re-arms**: re-mask on app background, on unlock, and after a
  configurable timeout. Momentary reveal per-value (press-and-hold), never a
  global un-mask without the toggle.
- **P4 — Display-only masking.** Masking applies at render time to already-
  public view strings. DTOs, use-case outputs, logs, and the headless
  protocol are byte-identical with masking on or off. (Masking is a privacy
  affordance against shoulder-surfing and screen-sharing — it makes no
  claims about memory.)
- **P5 — Consent exemption.** Authorization and consent surfaces always
  render their exact objects — amounts, recipients, attribute lists —
  unmasked, in every profile. The user authorizes precisely what executes;
  secret mode yields to that rule, visibly ("Details shown for
  authorization").
- **P6 — OS snapshot protection.** Two typed operations on the existing
  mobile-native bridge: Android sets/clears `FLAG_SECURE` (blocks
  screenshots/recording and blanks the app-switcher preview); iOS installs a
  privacy overlay on scene-background (there is no screenshot blocking on
  iOS — the spec claims only what the OS provides). Wired to secret mode and
  always-on for the backup-secret and reveal surfaces.
- **P7 — Dev adds diagnostics, never wider data.** The dev profile adds: a
  capability viewer rendered directly from `system.capabilities` (so it can
  never drift), composition/source/cursor/freshness detail on cards, raw
  machine strings alongside human labels, and timing info. It never renders
  secrets, claims, or anything the user profile couldn't request — more
  metadata, not more data.
- **P8 — Demo is a drawer over existing fixtures.** The demo profile adds a
  bootstrap drawer that only sequences use cases that already exist in
  standalone composition: create profile → initialize/unlock → derive →
  standalone demo offer (the exact Compact Digital Passport bundle) →
  fixture inbox receive → standalone login/verifier requests → simulated
  sync/funding. One tap each, plus "Run full demo setup" chaining them with
  progress. No new capabilities; a thin ribbon over the fixtures the UI
  already ships.
- **P9 — Truthful banners.** dev and demo builds render a persistent,
  non-dismissable banner naming profile + composition ("Standalone demo —
  fixture data, no chain contacted"). Screenshots from a demo can never
  masquerade as production.
- **P10 — Testing.** Smoke coverage mirrors ios/android-smoke: secret mode
  masks the matrix surfaces and the app-switcher snapshot is opaque
  (Android); demo bootstrap runs end-to-end; dev capability viewer matches
  the manifest; release build contains no dev/demo symbols.

## The matrix

| Surface | user | dev | secret | demo |
| --- | --- | --- | --- | --- |
| Balances (NIGHT/DUST/shielded) | visible | visible + provenance (source, cursors, freshness) | masked `••••` (layout-stable) | visible + fixture banner |
| Addresses & QR | visible | + derivation detail | masked; QR behind press-and-hold | visible |
| DIDs | truncated | full + method detail | masked | visible |
| Credential claims | per disclosure UX | + raw format/field names | masked previews; reveal per-value | fixture credentials |
| Activity/history | human labels | + raw states, hashes, timing | amounts/counterparties masked | fixture entries |
| Diagnostics | Settings → About (8 rows) | full capability viewer + modes | as user | as user + banner |
| Bootstrap actions | hidden | hidden | hidden | drawer (P8) |
| Consent/authorization sheets | exact, always | exact + raw detail expander | **exact — P5, never masked** | exact |

## Relationship to composition modes

| | production compose() | standalone-development | native standalone |
| --- | --- | --- | --- |
| user | ✓ (default ship) | ✓ | ✓ |
| secret | ✓ (runtime toggle) | ✓ | ✓ |
| dev | ✗ (compile_error!) | ✓ | ✓ |
| demo | ✗ (compile_error!) | ✓ | ✗ (fixtures need standalone) |

Precedents adopted: Revolut hide-balances (mask-as-resting-state, momentary
physical reveal), OWASP MASTG FLAG_SECURE guidance, iOS privacySensitive
background redaction, feature-flag hygiene (auto-generated dev screens,
no flag UI in release), and Oxid's own local-reveal pattern
("Reveal is local to this screen…") which secret mode generalizes.
