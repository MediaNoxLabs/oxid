# Information Architecture

## Prototype-derived shell, and why it changed

Before ADR-0086, the shell used a 6-tab bottom bar (Assets, Vault, DIDs, Credentials,
Diagnostics, Settings) **plus** a hamburger menu duplicating all seven
destinations **plus** two header shortcuts, with no router, no back stack,
and every page a long single-column card stack
(`crates/ui-dioxus/src/lib.rs:1299-1864`). Six tabs is above the 3–5
convention, two of them (Diagnostics, Settings) are non-jobs for a consumer,
and the redundant hamburger signals unresolved IA. monobank's own audit
history warns exactly against this shape: navigation that "isn't scalable"
and buries key sections.

Phase 1a is now delivered by ADR-0086. The Dioxus adapter owns a bounded route
stack; Home, Wallet, Documents, and Activity are the primary destinations,
Scan is the elevated center action, and the former global pages remain
reachable through secondary routes. Home temporarily retains the complete
Assets view alongside Wallet so the shell cutover cannot hide behavior. Phase
1b owns the final Home composition and removes that presentation overlap.

## The new shell

**Bottom bar, 4 tabs + 1 center action:**

```text
┌─────────────────────────────────────────────┐
│ [avatar ▾]   Home                    [eye]  │  top bar
│                                             │
│                  content                    │
│                                             │
├─────────┬─────────┬───────┬────────┬────────┤
│  Home   │ Wallet  │ SCAN  │  IDs   │ Activity│
└─────────┴─────────┴───────┴────────┴────────┘
```

- **Home** — the monobank-style card stack: hero + product cards + quick
  actions + recent activity preview. The one screen that answers "how am I
  doing" in under a second.
- **Wallet** — accounts and money: balances, receive, send, sync, submission
  recovery. (Today's Assets page, decomposed.)
- **Scan** (center, visually elevated) — the universal camera entry. Every
  QR — payment address, credential offer, presentation request, login —
  routes through the existing `RouteIdentityRequestUseCase` classifier and
  lands in the right flow with a preview. Scan is the front door of the
  identity world; giving it the center slot is the single strongest
  "identity is a peer" statement the shell can make.
- **IDs** (user-facing name: *Documents* — EUDI vocabulary) — credential
  cards, DID management behind a secondary "Manage identities" surface,
  issuance entry ("Add document").
- **Activity** — the unified, filterable feed: transactions, credential
  issuances, shares/presentations, logins, vault events. Identity wallets
  are regulator-bound to a complete interaction log (EUDI); monobank's audit
  says users want one history. One feed, typed entries, per-item detail.

**Top bar:** profile avatar (tap = profile/settings sheet: profile switcher,
security, backup, preferences, diagnostics-when-dev), page title, and the
**secret-mode eye toggle** (ui-profiles.md) — always one tap away.

**Retired surfaces:**
- Hamburger menu: deleted (redundant with tabs + avatar sheet).
- Diagnostics tab: folded into Settings; expands into the full capability
  viewer in the dev profile (rendered from `system.capabilities` so it can
  never drift).
- Vault tab: Passport Vault becomes a **product card on Home** (and a
  section reachable from it), not a permanent global tab — it is one product
  in the stack, monobank-style, present when its capability is composed.

**Navigation mechanics:** introduce a route stack (Dioxus router or the
existing signal upgraded to a stack) so sub-screens (credential detail, tx
detail, consent) get native back behavior and deep links from QR ingress
land on a pushable route instead of mutating a tab. Bottom sheets for
transient actions (receive QR, confirm, quick actions); full pages for
multi-step flows (send wizard, onboarding, backup) — the NN/g sheet-vs-page
rule adopted verbatim.

## Home anatomy (the flagship screen)

Order, top to bottom:
1. **Hero**: total NIGHT value, large; DUST as a small capsule; the
   source/freshness pill (Live / Cached / Simulated) as a colored dot +
   word, tappable for the one-sentence explanation. Secret-mode masks the
   number, never the pill.
2. **Action row** (4 buttons): Receive · Send · Present · Scan.
3. **Card stack** (swipable, full-width, monobank-style): the NIGHT account
   card, the shielded card, the newest credential card ("Digital Passport"),
   the Passport Vault card. Each card: product-family color, one primary
   number/status, long-press for contextual quick actions.
4. **Security status strip** (one line): "Backed up ✓ · Biometrics ✓ ·
   Standalone mode" — trust signals as a glanceable row, linking to
   Settings.
5. **Recent activity** (3 items + "See all").

## Screen map (delta from today)

| Before Phase 1 (`Destination`) | Becomes | Notes |
| --- | --- | --- |
| Assets (2288-2651, one giant stack) | Home + Wallet | Hero/actions/cards to Home; sync panes collapse into the account card + a background-sync pattern (journeys.md §Sync); send panel becomes the Send wizard route. |
| Vault (5616-5925) | Home card → Vault section | Duplicated create-lock forms merged; contract-call lifecycle becomes a stepper (journeys.md §Vault). |
| DIDs (6015-6491) | IDs → "Manage identities" | DIDs are plumbing for most users; credentials are the product. Keep full DID management one level down, not a tab. |
| Credentials (7291-7672) | IDs (Documents) | Card grid of issuer-branded credential cards; protocol entry cards ("OpenID4VCI 1.0 Final" eyebrows) replaced by one "Add document" action + scan. |
| Diagnostics (7674-7751) | Settings → About/Diagnostics; expanded in dev profile | The 8 capability rows become the seed of the dev capability viewer. |
| Settings (7766-8227) | Avatar sheet → Settings | Split into Security, Backup, Preferences, About; backup copy rewritten (journeys.md §Backup). |
| Profile (8238-8288) | Avatar sheet → profile switcher | Same use cases, sheet presentation. |

Every page keeps its underlying use-case wiring; this is a re-housing, not a
re-plumbing.
