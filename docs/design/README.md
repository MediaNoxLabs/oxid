# Oxid Design Specification

A product, UX, and design-system specification for making Oxid feel like a
consumer product — monobank-grade friendliness — without sacrificing one line
of the honesty and safety culture the codebase already enforces.

| Document | Contents |
| --- | --- |
| [information-architecture.md](information-architecture.md) | Navigation shell, screen map, per-screen anatomy. |
| [journeys.md](journeys.md) | The core user journeys, redesigned with step budgets, mapped to the existing flow state machines. |
| [design-system.md](design-system.md) | Tokens, components, motion, accessibility, and the copy system (including the machine-string labeling layer). |
| [white-label.md](white-label.md) | Build-time brand packs: architecture, schema, the non-brandable surface, CI gates. |
| [ui-profiles.md](ui-profiles.md) | The user / dev / secret / demo presentation profiles: rules, matrix, OS snapshot protection. |
| [rollout.md](rollout.md) | Phased delivery plan sliced for the backlog, success metrics, open product questions. |

Grounding: everything here is based on a full audit of the current UI
(`crates/ui-dioxus/src/lib.rs`, ~8,760 lines; `assets/styles.css`, 1,711
lines), current consumer-crypto-wallet conventions (Phantom, Rainbow, the
Figma/UI8 kit corpus), identity-wallet conventions (EUDI reference wallet and
ARF design guide, Microsoft Verified IDs, Apple/Google Wallet, SD-JWT
wallets), and the monobank design language the product owner named as the
quality bar.

## The product thesis

Oxid's engineering already delivers what most wallets fake: typed state
machines for every sensitive flow, fail-closed composition, capability labels
that never overclaim, consent that binds exactly what executes. The current
UI's weakness is purely presentational: it narrates the architecture to the
user (150–250 words per screen, raw `snake_case` states, 64-char hex fields,
literal confirmation sentences) instead of translating it.

**The redesign contract: keep the machine, replace the voice.** The flow
state machines, gates, and honesty semantics in the current implementation
are invariants. What changes is everything the user sees: structure, words,
color, motion, and how much truth is visible at each altitude.

## Design principles

1. **Truth, progressively disclosed.** Every honest label survives, but at
   the right altitude: a status pill on the surface, one plain sentence one
   tap deep, the full machine detail behind "Details" (and in the dev
   profile). Never lie; never lecture.
2. **One sentence per decision.** A screen asks the user to do at most one
   thing, framed in one human sentence. Word budgets are enforced in review:
   hero hint ≤ 15 words, card intro ≤ 20, consent summary = 1 sentence.
3. **Two taps to every top job.** See balance: 0. Show receive QR: 1.
   Present a credential or send to a recent recipient: 2. (monobank's
   interaction economy, adopted as an acceptance criterion.)
4. **The consent sheet is the product.** The moment we show who's asking and
   what leaves the wallet is the single surface users will judge us by —
   it gets the most design investment, the strictest rules, and biometric
   ceremony (see journeys.md §Consent).
5. **Personality where mistakes are cheap.** Micro-humor and celebration in
   empty states, achievements, and errors that cost nothing; sober precision
   wherever value moves or claims are shared (monobank's placement rule).
6. **Money and identity are peers, not roommates.** One home, one activity
   feed, one scan entry — but visually distinct product families and
   *distinct confirmation ceremonies* so paying never feels like disclosing,
   and vice versa.
7. **Design for the demo and the audit alike.** UI profiles (user / dev /
   secret / demo) are first-class presentation policies — because this
   product's audiences genuinely include end users, integrators, and
   regulators watching a screen-share.

## Personas and their jobs

- **Olena, the holder** (consumer): tops up, sends NIGHT to a friend, proves
  she's over 18 at a checkout, checks "what did I share last week?". Judges
  the app against monobank. Never wants to see the word "canonical".
- **Denys, the integrator/developer**: runs standalone builds, needs to see
  composition modes, sources, cursors, and capability truth instantly;
  today's Diagnostics page is *for him* — it should get richer, not die.
- **The presenter** (sales/conference/user-research): demos flows on a
  projector; needs bootstrap-in-one-tap and guaranteed masking of anything
  sensitive-looking; must never accidentally show a real wallet.

## What must not change (the invariants)

- Flow state machines and their recovery semantics: `TransferPanelState`
  (9 states incl. `Failed{retained, recovery}`), the vault call lifecycle,
  backup states, profile session gateway.
- Never-blind-retry and broadcast-boundary semantics, and their user-facing
  meaning ("Oxid will not create or submit a replacement while broadcast may
  have occurred" — rephrased, never weakened).
- Capability honesty: Live / Cached / Simulated / Not connected states,
  `settlesOnMidnight`, fail-closed production copy.
- Consent binding: what the user approves is exactly what executes; secret
  mode never masks a consent surface (ui-profiles.md, rule P5).
- The accessibility discipline already present: focus-visible, aria-live on
  status, aria-busy during async, prefers-reduced-motion, safe-area insets.

## Success metrics

- Time-to-first-wallet (fresh install → usable home) ≤ 60 s, no seed shown.
- Top-job tap budgets hold (0/1/2 as above), verified in the smoke flows.
- Static copy per screen ≤ 60 words in the user profile (today: 200–240).
- Zero raw machine strings (`snake_case`, hex, epoch-ms, cursor numbers)
  visible in the user profile — enforced by a lint listed in rollout.md.
- Every brand build passes the WCAG AA contrast gate (white-label.md).
- The consent sheet answers who/what/why in ≤ 12 seconds of reading (tested
  with usability sessions once flows are clickable).
