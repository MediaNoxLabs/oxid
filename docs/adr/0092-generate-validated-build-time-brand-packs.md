# ADR-0092: Generate validated build-time brand packs

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 1–6, 12–18, and 21
- Design source: `docs/design/white-label.md`, rollout Phase 3
- Tracking: issues #2, #65, and #84
- Implementation state: the default Oxid thin app consumes one validated, generated build-time pack; repository and Nix gates enumerate every pack

## Context

ADR-0084 separated brand primitives from the fixed semantic component
vocabulary, but both layers still lived in the shared Dioxus stylesheet. The
Oxid mark and product strings were also embedded across components. A partner
build could therefore be produced only by editing shared UI source, with no
closed schema, contrast gate, bundle-identity check, or way for CI to discover
all brand inputs.

Branding must remain presentation configuration. It cannot choose adapters,
protocols, trust anchors, custody, confirmation semantics, or capability
claims, and it cannot become runtime-downloaded configuration. Logo input is
particularly sensitive because the UI embeds it as inline SVG.

## Decision

Add the build-only `oxid-brand-build` crate and place each pack under
`brands/<lowercase-slug>/`. A pack has exactly:

- a strict, flat `brand.toml` with schema version, bounded product identity,
  bundle identifier, publisher, logo path, and reviewed cosmetic flags;
- a deny-unknown `tokens.json` with closed font/radius enums and complete dark
  and light opaque `#RRGGBB` palettes; and
- a bounded regular SVG below the pack's `assets/` directory.

The validator rejects unknown, duplicate, missing, oversized, non-UTF-8,
symlinked, escaping, or malformed inputs. It rejects active or externally
referencing SVG constructs before the logo reaches `dangerous_inner_html`.
It computes WCAG relative luminance and fails the build unless every text token
meets 4.5:1 against every surface and accent, product-family, fixed safety, and
on-accent pairs meet their fixed 3:1 or 4.5:1 thresholds in both schemes.

Each thin application chooses its pack with a literal path in `build.rs`.
There is no environment selection or runtime discovery. The builder writes
only `brand.css`, `brand-logo.svg`, and a typed `BrandProfile` constant into
that application's `OUT_DIR`. `apps/oxid` injects the constant through the
existing Dioxus composition context. The shared UI consumes only immutable
product presentation values and generated semantic CSS; it does not depend on
the build crate or name the default brand.

The app manifest remains code reviewed. The builder requires its bundle
identifier and publisher to equal the pack and its camera/Face ID descriptions
to equal code-owned templates with only the validated product name
substituted. The same restriction applies to centralized recovery, ambiguous
submission, backup-receipt, presentation-consent, and Vault broadcast copy.
A default-app snapshot pins the resulting complete strings.

Fixed positive, warning, critical, and information colors are generator-owned,
not pack fields. Brand metadata cannot change protocol schemes, endpoint or
trust configuration, custody or proof modes, feature profiles, transaction
state, confirmation intents, settlement labels, or application data.

`scripts/check-brand-packs.sh` validates the complete directory from the UI
gate. Nix exposes `packages.brand-check`, keeps the default app as
`packages.oxid-app-oxid`, rejects invalid root entries, and creates one
auto-enumerated `checks.brand-<name>` derivation per real directory. A future
release brand still requires its own thin app crate and reviewed manifest;
adding a pack alone does not create a distributable application.

## Consequences

- The default UI remains visually and functionally Oxid while its brand values
  are now generated and selected before compilation.
- A malformed or inaccessible pack, unsafe logo, low-contrast palette, or
  manifest mismatch fails locally, in Cargo builds, and in Nix checks.
- Shared component CSS can use only semantic tokens. It cannot reach raw brand
  or fixed safety primitives; the font-family token is the sole direct brand
  value because it establishes the document typography.
- The current `show_vault_card` value is an immutable cosmetic projection in
  the selected default pack. A licensed product that needs code removed from
  the binary must additionally forward a reviewed compile-time feature from
  its thin app; metadata visibility is not an authorization or licensing gate.
- App icons, splash resources, a second branded app, and a user-selectable
  light theme remain later delivery work. No runtime theme/brand switch is
  authorized by this decision.

## Validation

- Brand-build unit tests cover the default pack and generated outputs, closed
  metadata/token schemas, malformed values, contrast failure, unsafe SVG,
  symlink/root shape, manifest identity, and purpose-copy drift.
- UI tests prove `BrandProfile` exposes only build-selected presentation data
  and security templates vary only at the product-name slot.
- The default app pins its identity plus complete consent, recovery, backup,
  broadcast, and submission-safety strings.
- `run.sh ui`, strict workspace gates, Nix evaluation, per-pack Nix builds, and
  the unchanged iOS/Android standalone smoke flows are required.

## Rejected alternatives

- Runtime brand downloads or `OXID_BRAND` selection make release identity
  mutable and can retain unreviewed/dead assets.
- One Cargo feature per brand is additive under feature unification and cannot
  own bundle identifiers or store manifests safely.
- Editing the shared stylesheet and string literals per release is not
  enumerable, reproducible, or reviewable as a closed input.
- Letting packs supply arbitrary CSS, HTML, safety copy, manifest purpose text,
  or remote assets creates injection and capability-honesty boundaries inside
  presentation data.
- Treating a cosmetic visibility flag as authorization would let branding
  change product capability or security semantics.
