# Design System

## Token architecture (Phase 0 of the rollout — everything depends on it)

Today `assets/styles.css` defines 24 `:root` custom properties but bypasses
them with 60+ hardcoded alpha literals, ~20 ad-hoc font sizes, and ~10 radii.
Issue #63 maps the previously undefined Vault vocabulary onto the shared
card/action/form rules, and `scripts/check-ui-css-classes.sh` now rejects a
new static Dioxus class literal without a stylesheet selector. Phase 0 still
replaces the compatibility vocabulary and raw values with a strict two-layer
system:

**Layer 1 — brand tokens** (supplied per brand, white-label.md):
palette primitives, type family, radius personality, logo/mascot assets.

**Layer 2 — semantic tokens** (fixed vocabulary, consumed by components;
brands may only re-point them at their primitives):

```css
/* surfaces */    --surface-0..4, --surface-raised, --surface-sheet
/* text */        --text-strong, --text, --text-soft, --text-muted
/* brand */       --accent, --accent-alt, --on-accent
/* semantics */   --positive, --warning, --critical, --info   /* NOT brandable */
/* products */    --family-assets, --family-identity, --family-vault
/* lines */       --line, --line-strong
/* type scale */  --font-display, --font-title, --font-body, --font-label,
                  --font-caption, --font-numeral  (6 steps, fluid)
/* space */       --space-1..8  (4 / 8 / 12 / 16 / 20 / 24 / 32 / 48)
/* radius */      --radius-card (20) / --radius-control (12) / --radius-pill (999)
/* motion */      --motion-fast (120ms) / --motion-base (200ms) / --motion-slow (320ms)
/* elevation */   --shadow-card, --shadow-sheet
```

Rules: no raw color/size literals in component CSS (lint in rollout.md);
semantic-state colors (`--positive/--warning/--critical`) are **fixed across
all brands** — a brand can restyle joy, never danger. Dark is the default
scheme (current `color-scheme: dark` stays); a first-class light palette is
part of the token schema from day one so brands must define both.

## Visual language

- **Card-first.** Cards are the unit of everything: accounts, credentials,
  vault locks, activity groups. Product families are color-coded
  (`--family-assets` cyan-family, `--family-identity` purple-family,
  `--family-vault` green-family by default) the way monobank codes card
  tiers — recognition before reading.
- **Credential cards are issuer-branded** from OpenID4VCI display metadata
  (name, logo, `background_color`, `text_color`) with an automatic contrast
  overlay, and a designed Oxid fallback card when metadata is absent. Status
  is a first-class card state: Valid (quiet), Expires soon (amber corner),
  Expired (muted + badge, blocked from presentation with a plain
  explanation), Revoked (critical badge).
- **Big rounded numerals** for money and counts (`--font-numeral`,
  tabular figures); typography carries the hierarchy, color carries meaning.
- **One saturated accent per brand** on dark neutrals (monobank/Radient
  pattern); the accent is reserved for primary actions and moments of joy.

## Component inventory (unified — kills the two vocabularies)

Shell: TabBar, TopBar, AvatarSheet, RouteStack, Toast, Banner (profile/mode
banners), SecurityStrip. Surfaces: Card (product/credential/lock variants),
Sheet (bottom), Stepper, SegmentedControl, ListRow, DetailDisclosure
("Details" expander — the progressive-truth primitive), StatusPill (the
Live/Cached/Simulated/Pending/Confirmed/Failed vocabulary, colored dot +
word), QuickActions (long-press card menu). Inputs: AmountField (big
numerals + max + unit), AddressField (paste/scan/recents + grouped echo),
SecretField (strength meter), ConsentChecklist (locked/optional attribute
rows), PrimaryButton/SecondaryButton/DangerButton, IconButton. Feedback:
Skeleton (shimmer, for hero/lists), EmptyState (always sells one action),
ErrorState (conversational + typed recovery actions), Celebration (confetti
tick — reduced-motion-aware), ProgressRing (sync). Identity: CredentialCard,
IssuerIdentityBlock (name + verified domain + trust indicator), PredicateRow
("Confirms you're over 18" + negative reassurance), ActivityItem (typed:
payment/share/issuance/login/vault).

Every list-bearing component ships all four states: loading skeleton, empty
(with CTA), error (with recovery), populated. This is an acceptance
criterion, not a nicety.

## Motion & haptics

Sheets slide (200 ms), cards spring subtly on swipe, status pills cross-fade;
one celebration animation (≤ 800 ms, skippable, disabled under
prefers-reduced-motion — which the codebase already respects). Haptics on:
consent confirm, celebration, error. Never animate during an authorization
ceremony beyond the OS biometric UI.

## Copy system

**The labeling layer (hard rule).** Every machine string crosses a label
function before rsx: states, modes, sources, formats, authentication labels,
reason codes. Today's `replace('_', " ")` and raw leaks
(`deterministic_simulation`, `canonical_finalized_replay`, `outcome_unknown`,
epoch-ms, cursor numbers, "base units") are all replaced by a single
`label(...)` module with exhaustive `match` — the compiler then enforces
that a new enum variant gets a human name. Raw values remain visible in
Details sheets and the dev profile.

**Vocabulary table (excerpt, to be completed in implementation):**

| Machine | User-facing |
| --- | --- |
| `deterministic_simulation` | Simulated — runs locally, nothing on Midnight |
| `canonical_finalized_replay` | Verified against the Midnight network |
| `indexer_supplied_not_proven` | Reported by an indexer — not yet verified |
| `outcome_unknown` | Checking with the network… |
| `proof_unavailable` | This build can't generate proofs yet |
| base/atomic units | NIGHT / DUST decimals (existing exact formatter) |
| epoch millis | "18 Aug 2026, 14:02" / "2 min ago" |
| `midnight_compact_vc` | Digital Passport (Midnight format) — Details |

**Voice rules.** Conversational, precise, short (word budgets in README).
EUDI-aligned nouns: *Documents*, "Who's asking", *verified issuer*.
Consent sentences name the exact object ("Send **12.5 NIGHT** to **mn1…k29x**"),
one sentence, then one affirmative button — literal checkbox sentences
("I reviewed…") are retired in favor of structured sheets + biometrics.
Humor placement: empty states, achievements, cheap errors only (monobank
rule); never in consent, custody, backup, or failure-with-consequence.
Ukrainian and English ship together; the label layer is the i18n seam.

**Celebrations** at: first wallet created, backup completed, first credential
received, first proof shared, recovery tested. Security hygiene earns visible
progress (the SecurityStrip), not nags.

## Accessibility

Keep and extend the existing discipline: focus-visible everywhere,
role=status/alert with aria-live, aria-busy on every async surface,
prefers-reduced-motion honored, safe-area insets. Add: minimum 44 pt touch
targets, WCAG 2.1 AA contrast enforced *per brand at build time*
(white-label.md), dynamic-type tolerance for the 6-step scale, and
VoiceOver/TalkBack labels for every StatusPill state (the dot alone never
carries meaning).
