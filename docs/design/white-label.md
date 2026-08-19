# White-Labeling: Build-Time Brand Packs

Goal: a partner brand ships its own styled Oxid at build time — palette,
typography personality, logo, product name, bundle identity — with zero
runtime cost, no dead brand assets in the binary, and no ability to weaken
the security surface.

## Delivered baseline

ADR-0092 delivers the first pack without changing the default product:

- `apps/oxid/build.rs` selects the literal `brands/oxid/` path, validates it
  and the thin-app manifest, and generates CSS, SVG, and typed Rust into that
  app's `OUT_DIR`;
- `crates/ui-dioxus` embeds the generated layers through an immutable
  `BrandProfile` context and contains no default-brand presentation strings;
- safety copy and state colors stay code-owned, with a default-app snapshot
  proving that only the validated product name is substituted;
- `run.sh` validates the complete pack root and Nix exposes the checker, the
  named default app, a root check, and one auto-enumerated check per pack.

The repository currently has one release brand and therefore one thin app.
App icons, splash resources, another partner app, compile-time licensed feature
removal, and a user-selectable light theme remain later work; a pack directory
alone is not a distributable application.

## Architecture: brand packs + thin per-brand app crates

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
identity first-class and brands enumerable (`builtins.readDir ./brands` in Nix
creates per-pack checks; each reviewed thin app exposes its named
`packages.oxid-app-<brand>` build). This is idiomatic to the repository. A
runtime or environment-selected `OXID_BRAND` override is not part of the
accepted release or development boundary.

## The non-brandable surface (enforced by schema, documented by ADR)

Brands may change: palette (within contrast gates), typography, radii,
logo/mascot/icons, product name + tagline (as the only free-text fields,
substituted into copy via a single `{product_name}` slot), cosmetic feature
visibility (the default pack's Vault-card flag is presentation-only and cannot
grant capability). A licensed build that must remove code from its binary needs
a separately reviewed Cargo feature forwarded by its thin app; the pack flag
is not an authorization or licensing boundary.

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

Per-push: build the default brand plus schema/contrast checks for all packs.
Nightly: validate every pack (auto-enumerated) and build every reviewed thin
app. Adding a distributable brand requires a directory plus a thin crate; the
pack-check matrix itself needs no workflow edit.
