# Lunar Aegis — Oxid mobile design profile

**Profile ID:** `lunar-aegis`.

**Status:** 0.2.0 design target; the running app has not yet adopted these tokens or assets.
**Source snapshot:** 27 September 2026.

This is the first repository-owned design profile for the Oxid crypto and
identity wallet. It lets engineers and AI agents inspect the approved visual
direction and all linked screens without the owner's UX Pilot account.
The profile contains design evidence and implementation guidance. It does not
alter the wallet's runtime `user`, `dev`, `secret`, or `demo` presentation
profiles.

## Start here

| Need | File |
| --- | --- |
| Exact, machine-readable visual target | [profile.json](profile.json) |
| Screen titles, state labels, source hashes, and local paths | [screens/manifest.json](screens/manifest.json) |
| Clickable index of all exported screens | [screens/README.md](screens/README.md) |
| Offline screen images | [screens/png/](screens/png/) |
| Exported screen markup for layout inspection | [screens/html/](screens/html/) |
| 37-node, 66-action navigation graph | [flow.json](flow.json) |
| Readable journey map | [flows.md](flows.md) |
| Component contracts and state examples | [components.md](components.md) |
| Existing-code mapping and implementation order | [implementation.md](implementation.md) |
| Open design-quality findings | [qa.md](qa.md) |
| Logo references and five navigation SVGs | [assets/](assets/) |
| Bundled typefaces and license files | [fonts/](fonts/) |
| Offline export integrity check | [verify.py](verify.py) |

Run `python3 design/lunar-aegis/verify.py` from the repository root to check
all screen hashes, graph references, icons, and bundled assets.

## Design source and boundaries

The owner-facing source is [Oxid Mobile — Complete
Flows](https://uxpilot.ai/a/ui-design?page=r30d8UxUa7xuLus8ZH2j), using
verified diagram `ybCLMfv8Y7LWO3Rfra3J`. The [published V1 UX Pilot design
system](https://uxpilot.ai/a/design-system/Oxid%20Mobile%20%C2%B7%20Lunar%20Aegis)
contains 20 conceptual components. The local export is the reviewable source
for teammates without UX Pilot access. Issue [#800](https://github.com/MediaNoxLabs/oxid/issues/800)
tracks implementation of the shared foundation; issues #789–#799 cover the
screen and flow backlog.

The manifest contains **34 distinct active screen designs** linked from the
diagram. Four diagram nodes represent the observed standalone Home, Wallet,
Documents, and Activity states; the rest are proposed journeys, a shared
navigation structure, or a documented login-review gap. “Observed” means the
screen depicts a state supported by the current app audit. The exported
Lunar Aegis rendering itself is still a design proposal. A balance, credential,
address, proof, or final transaction shown in another example is never evidence
that the application can produce it.

The PNGs are offline visual references. The exported HTML is exact UX Pilot
source, with its source SHA-256 in the manifest. It references Tailwind,
Font Awesome, Google Fonts, and the UX Pilot-hosted logo at runtime, so it is
not a self-contained app or production Dioxus code. Use it to inspect hierarchy,
spacing, copy, and SVG geometry; implement behavior through Oxid's existing
typed state and semantic-token layers.

## Core visual target

| Role | Value |
| --- | --- |
| Background | `#07090E` |
| Elevated card | `#0F172A` |
| Structural border | `#1E293B` |
| Primary action / active navigation | `#06B6D4` |
| Bright highlight | `#38BDF8` |
| Identity accent | `#8B5CF6` |
| Strong text | `#F8FAFC` |
| Muted text | `#94A3B8` |

Use Space Grotesk for Latin headings, Plus Jakarta Sans for Latin body and
controls, JetBrains Mono for technical strings, and bundled Noto Sans as the
Cyrillic fallback. The chosen Space Grotesk and Plus Jakarta Sans files do
not cover Ukrainian. Cards target a **24 px** radius, controls **12 px**, and
interactive targets at least **44 × 44 px**. Respect platform safe areas and
reduced motion. Dark is the 0.2.0 target; the existing light-token mapping
remains a future reviewed mode.

The fixed navigation order is **Home / Wallet / Scan / Documents / Activity**,
using the five local [outline SVGs](assets/icons/). Scan is emphasized.
Active selection uses cyan plus a short underline, with full text labels.
Colors for positive, warning, critical, and source/freshness states must remain
semantically distinct from the brand accents.

## How to implement from this export

1. Choose a flow node in [flow.json](flow.json), then find its `designId`
   in [screens/manifest.json](screens/manifest.json). Read its state label.
2. Inspect the local PNG and HTML. Review [qa.md](qa.md) before copying
   visual details or wording from a proposed screen.
3. Build reusable Dioxus components through the existing semantic token
   architecture; [implementation.md](implementation.md) names the concrete
   repository seams. Use the local fonts, vector mark, and navigation icons
   after resolving the logo's small-size treatment.
4. Bind labels and controls to real app state. Keep unavailable, simulated,
   cached, pending, confirmed, and unknown outcomes visibly distinct.
5. Compare on Android and iOS at 375 px and larger widths, with large text,
   safe areas, and reduced motion. Resolve the open findings before treating
   screenshots as acceptance baselines.

The original user-supplied raster concept says “0xID” and differs from its
master SVG. Both are saved in [assets/](assets/) for comparison; neither
settles the production app icon by itself. The current shipped logo in
`brands/oxid/assets/logo.svg` remains unchanged by this export.
