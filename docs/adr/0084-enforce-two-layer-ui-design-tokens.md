# ADR-0084: Enforce two-layer UI design tokens

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 1, 3–7, 9, 12–13, 16, 18, and 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/UX_DESIGN.md`, `mobile-bench/MOBILE_WALLET.md`, and the Dioxus stylesheet
- Tracking: issues #2, #65, and #67
- Implementation state: the Dioxus stylesheet defines dark and light brand primitives, maps the shipped dark scheme into one fixed semantic vocabulary, and fails the repository gate when component CSS introduces raw colors, legacy palette aliases, ad-hoc type sizes, radii, or motion durations

## Context

The migrated wallet shell retained the prototype's visual character, but its
single stylesheet mixed reusable variables with more than sixty direct color
and alpha values, roughly twenty font sizes, and ten radii. That made a future
white-label pack able to change some surfaces while silently leaving other
components behind. It also allowed a brand change to alter the visual meaning
of warning, success, or destructive consent states.

Oxid's accepted design specification makes the token foundation the first
presentation slice. The later navigation shell, consent ceremonies, secret
mode, and white-label builds should consume one stable vocabulary rather than
add another set of component-specific literals. This changes presentation
ownership only; application state machines and capability truth remain the
authority for what the UI may say or do.

## Decision

Use two layers in `crates/ui-dioxus/assets/styles.css`:

1. Build-selected brand primitives own dark and light palette values, the type
   family, accent personality, product-family colors, and decorative overlay
   and shadow inks. Both palette schemes are complete, but dark remains the
   only selected mapping until a later reviewed UI-profile/theme slice enables
   another scheme.
2. Components consume the stable semantic vocabulary from the design spec:
   surfaces, text levels, accent/on-accent, product families, lines, the six
   type steps, eight spacing steps, three radii, three motion durations, and
   card/sheet elevation.

Positive, warning, critical, and informational colors are fixed safety
semantics outside the brand palette. A brand pack may not redirect them.
Alpha treatments derive from semantic colors with `color-mix`; they do not
restate RGB values in component selectors. Existing component spacing is
collapsed onto the eight-step scale where it is spacing, while dimensions
such as touch-target height, QR size, responsive width, and safe-area geometry
remain explicit layout constraints.

Add `scripts/check-ui-design-tokens.sh` to the aggregate repository gate. It
requires the complete dark/light and semantic token schemas, rejects the old
palette aliases, and rejects raw color literals outside the marked definition
block. It also requires component font sizes, radii, and timed motion to use
their reviewed scales. The existing static-class coverage check remains a
separate guard.

## Security and truth boundaries

- Critical, warning, positive, and informational meanings are fixed across
  brands; decoration cannot relabel risk.
- A token changes only presentation. It cannot change capability state,
  consent wording, confirmation intent, masking policy, or application flow.
- Secret mode and consent masking remain later policy decisions. A palette is
  not permission to conceal a recipient, amount, issuer, credential choice, or
  any other authorization input.
- Light primitives being present is not a shipped light theme. Runtime theme
  selection remains disabled until its accessibility and profile policy are
  reviewed.
- No external CSS framework, runtime theme loader, or network asset is added.

## Consequences

- Phase 1 shell and journey components can use one predictable vocabulary.
- A future brand pack has a narrow build-time mapping surface and cannot reach
  fixed safety colors through ordinary brand primitives.
- The token lint is intentionally conservative. A genuinely new semantic
  category requires design-system and ADR review instead of an inline literal.
- Consolidating the existing scale causes small intentional spacing, radius,
  and type-normalization changes while retaining the established dark palette.

## Validation

- `scripts/check-ui-design-tokens.sh` validates schema completeness and the
  component boundary, and is invoked by `run.sh`.
- `scripts/check-ui-css-classes.sh` continues to prove that every static Dioxus
  class has a selector.
- The Dioxus crate tests and full Nix repository gate compile the embedded
  stylesheet.
- The standalone iOS Simulator smoke visually exercises the resulting mobile
  shell at the Tier-1 viewport.

## Rejected alternatives

- Keeping raw component values until white-label implementation would make the
  first brand migration a broad, difficult-to-review visual rewrite.
- Making semantic safety colors brand primitives would allow branding to
  weaken consistent risk cues.
- Automatically selecting the light palette from OS preference would ship an
  unreviewed theme before the profile, contrast, and snapshot gates exist.
- Adding a CSS framework or runtime theme service would expand dependencies
  without improving the hexagonal boundary.
