# Design System

## Token architecture (Phase 0 of the rollout — delivered by ADR-0084)

`assets/styles.css` now implements the strict two-layer system. Issue #63
first mapped the previously undefined Vault vocabulary onto the shared
card/action/form rules; ADR-0084 then replaced the component palette literals,
ad-hoc type sizes, spacing, radii, and motion with the vocabulary below.
`scripts/check-ui-css-classes.sh` rejects unmatched static Dioxus classes and
`scripts/check-ui-design-tokens.sh` rejects token-schema drift or raw component
colors. Dark remains the selected scheme; complete light primitives exist as a
future reviewed mapping and do not activate an unshipped light theme.

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

Rules: no raw color literals in component CSS; type sizes, radii, and timed
motion also use their token scales (lint in rollout.md). Responsive widths,
touch-target dimensions, QR sizes, and safe-area geometry remain explicit
layout constraints rather than pretending to be spacing. The
semantic-state colors (`--positive/--warning/--critical`) are **fixed across
all brands** — a brand can restyle joy, never danger. Dark is the default
scheme (current `color-scheme: dark` stays); a first-class light palette is
part of the token schema from day one so brands must define both.

## Lunar Aegis crosswalk (0.2.0 foundation)

The published Lunar Aegis V1 design system is represented by a closed brand
pack rather than copied screen markup. `brands/oxid/tokens.json` supplies the
brand layer; `crates/brand-build` validates and emits the semantic layer used
by Dioxus. Dark is shipped; the complete light primitive set remains available
without selecting a light application mode.

| Lunar Aegis role | Brand primitive | Semantic consumer | Foundation seam |
| --- | --- | --- | --- |
| Midnight Obsidian | `#07090E` | `--surface-0` | app background |
| Deep Slate Navy | `#0F172A` | `--surface-raised` | cards and elevated surfaces |
| structural line | `#1E293B` | `--line` | controls, cards, sheets |
| Celestial Cyan | `#06B6D4` | `--accent`, `--family-assets` | primary action and active navigation |
| highlight cyan | `#38BDF8` | `--highlight` | focus treatment |
| Phantom Violet | `#8B5CF6` | `--accent-alt`, `--family-identity` | identity emphasis |
| Pure Titanium / Slate Steel | `#F8FAFC` / `#94A3B8` | `--text-strong` / `--text-muted` | headings and supporting copy |

Positive, warning, critical, and info remain fixed semantic colors emitted by
`crates/brand-build`; branding cannot redefine their truth. The app bundles
Space Grotesk for headings, Plus Jakarta Sans for body/controls, JetBrains Mono
for addresses/DIDs/hashes, and Noto Sans for Ukrainian fallback. Font files,
OFL texts, and provenance live in `apps/oxid/assets/fonts/`; no remote request
is needed.

The reusable Dioxus foundation is the shared CSS and component vocabulary:
`bottom-nav` (five labelled slots with stable `nav-*` IDs), primary/secondary
buttons, card surfaces, field controls, status pills, bottom sheets, and
empty/error/loading states. Every surface keeps the existing truthful state
rules: unavailable addresses cannot be copied, pending/unknown outcomes are
not success, and consent/custody/capability decisions remain bound to their
existing typed flows. This crosswalk deliberately does not introduce the
screen-specific work assigned to the 0.2.0 flow slices.

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
`label(...)` module with exhaustive `match`. The compiler enforces typed enum
matches; string-flattened application views use explicit known-value matches,
a safe unknown label, and the repository copy gate. Raw values remain visible
only in Details sheets and the dev profile.

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

ADR-0085 implements this boundary in `crates/ui-dioxus/src/labels.rs`.
`scripts/check-ui-copy-labels.sh` rejects direct machine-field interpolation
while exact unit/date tests protect the formatting seam.

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
