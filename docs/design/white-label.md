# White-Labeling: Build-Time Brand Packs

Goal: a partner brand ships its own styled Oxid at build time — palette,
typography personality, logo, product name, bundle identity — with zero
runtime cost, no dead brand assets in the binary, and no ability to weaken
the security surface.

## Current reality (what we build on)

- Styling is one static CSS file embedded via `include_str!` into a
  `const STYLES` and injected inline; no manganis `asset!` usage, no
  `build.rs` anywhere in the workspace.
- Brand identity is scattered: a pure-markup logo (`.oxid-mark` + literal
  `"oxid"` wordmark strings), ~18 hardcoded "Oxid" occurrences (mostly
  inside safety-critical copy), and bundle identity in `apps/oxid/Dioxus.toml`
  (`io.medianox.oxid`, publisher, iOS plist purpose strings).
- Two repo idioms to reuse: mutually-exclusive cargo features enforced by
  `compile_error!` (apps/oxid/src/main.rs), and env-vars-as-pure-Nix-inputs
  (`OXID_*_ARTIFACTS_DIR` in nix/packages).

## Architecture (recommended): brand packs + thin per-brand app crates

```text
brands/<name>/
  brand.toml      # product_name, tagline, bundle identifier, publisher,
                  # cosmetic feature toggles (e.g. show-vault-card)
  tokens.json     # layer-1 tokens: dark + light palettes, type family,
                  # radius personality, per-family hues
  assets/         # logo.svg, app icons, splash, (optional) mascot set
apps/oxid/        # the default brand's thin app crate (as today)
apps/<brand>/     # ~60-line clone per brand: own Cargo.toml + Dioxus.toml
                  # (bundle id, icons, plist strings), same feature matrix
crates/brand-build/  # shared build-time codegen library ("oxid-brand-build")
```

At build, the app crate's `build.rs` calls `oxid-brand-build`, which:
1. Validates `brand.toml` + `tokens.json` against a **deny-unknown-keys**
   schema (a brand cannot introduce fields we didn't design for — the same
   discipline the storage schemas use).
2. Runs the **WCAG 2.1 AA contrast gate** over a fixed token-pair matrix
   (every text token × every surface it may sit on at 4.5:1 body / 3:1
   large-text-and-UI; accent pairs at 3:1) — for both dark and light
   palettes. A failing brand fails the build, not the review.
3. Generates `OUT_DIR/brand.css` (layer-2 semantic tokens bound to the
   brand's primitives) and `OUT_DIR/brand.rs` (a typed `BrandProfile` const:
   product name, tagline, toggles). CSS delivery stays exactly as today —
   `include_str!` + inline style — no new asset pipeline on the critical
   path; the dx `asset!` pipeline remains reserved for images/icons.
4. Emits `cargo:rerun-if-changed=brands/<name>/`.

`crates/ui-dioxus` stays 100 % brand-agnostic: it consumes `BrandProfile`
via the existing composition-root context pattern and never names a brand.

**Why not the alternatives.** One-cargo-feature-per-brand breaks under
feature unification (features are additive; `--all-features` CI would merge
brands) and bloats the matrix. A pure `OXID_BRAND` env var on a single crate
can't give each brand its own bundle id/icons/store identity and makes brand
enumeration invisible to CI. The pack + thin-crate model keeps store
identity first-class, brands enumerable (`builtins.readDir ./brands` in Nix
→ `packages.oxid-app-<brand>` + per-brand flake checks), and is idiomatic to
this repo. An `OXID_BRAND` override may exist as a *developer convenience*
for quick theme iteration on the default crate — never for releases.

## The non-brandable surface (enforced by schema, documented by ADR)

Brands may change: palette (within contrast gates), typography, radii,
logo/mascot/icons, product name + tagline (as the only free-text fields,
substituted into copy via a single `{product_name}` slot), cosmetic feature
toggles (e.g. hide the Vault card where unlicensed — implemented as cargo
features forwarded by the brand crate so dead code leaves the binary).

Brands may **never** change:
- consent and confirmation semantics or their sentence templates;
- submission-safety, broadcast-boundary, and backup-safety copy;
- capability-honesty labels (Live/Cached/Simulated…, `settlesOnMidnight`)
  and the semantic state colors (`--positive/--warning/--critical`);
- plist/manifest purpose strings beyond the `{product_name}` slot;
- protocol schemes, trust anchors, or anything outside the UI layer.

Per-brand snapshot tests pin the security copy: a brand build renders the
consent/backup/submission sentences and compares against the code-owned
templates with only the product name substituted.

## UI profiles stay orthogonal

Profiles (ui-profiles.md) are the existing composition/feature axis and are
replicated identically in every brand crate — a brand cannot ship a demo
profile to a store because the same `compile_error!` + release-CI guards
apply to all app crates uniformly.

## CI

Per-push: build the default brand + schema/contrast checks for all packs.
Nightly: build every brand (auto-enumerated). Adding a brand = adding a
directory + a thin crate; no workflow edits.
