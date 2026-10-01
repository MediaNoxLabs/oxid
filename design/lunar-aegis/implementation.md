# Mapping Lunar Aegis into Oxid

This export names the target. It does not change the running app.

| Target | Current implementation seam | Work needed |
| --- | --- | --- |
| Dark palette | `brands/oxid/tokens.json` | Map the approved core values through the brand pack and retain a complete reviewed dark/light schema. |
| Semantic surfaces and lines | `crates/brand-build/src/lib.rs`, `crates/ui-dioxus/assets/styles.css` | `surface_0` → background; `surface_2` / `--surface-raised` → elevated; `--line` target → border. Other surface slots remain a design decision. |
| Assets/identity colors | Generated `--family-assets` from accent and `--family-identity` from accent_alt | Map primary cyan and identity violet; keep Vault and fixed status colors independently reviewed. |
| Typography | `FontFamily` in `crates/brand-build/src/lib.rs` currently offers only `system_sans` and `humanist_sans` | Add the bundled role-based font stack without inserting remote font requests. Test Ukrainian with Noto fallback. |
| Radius | `RadiusPersonality::Rounded` currently emits a 20 px card and 12 px control | Add an explicit 24 px card mapping or reviewed profile-specific token; do not claim default Tailwind `rounded-2xl` is 24 px. |
| Logo | `brands/oxid/assets/logo.svg` | Use the owner-approved fingerprint-crescent mark and platform exports in [brand-icons.md](brand-icons.md); preserve the original UX Pilot logo master as historical evidence. |
| Bottom navigation | `crates/ui-dioxus/src/lib.rs` and shared CSS | Use one five-icon implementation matching [assets/icons](assets/icons/), full labels, active underline, and ≥44 px targets. |
| Screen flows | Existing Dioxus routes and typed view state | Follow [flow.json](flow.json) and issue #789's observed/proposed inventory; bind each screen to real prerequisites, review gates, and outcomes. |

The brand-build schema denies unknown keys, so `profile.json` is **not**
a drop-in `brands/oxid/tokens.json` replacement. It records only values
approved in the supplied brand guide and latest screens. In particular,
`surface_1`, `surface_3`, `surface_4`, normal/soft text, Vault accent,
on-accent, and the light palette still need a reviewed mapping that passes
the existing contrast validator. The current `--line` is derived from
`--text-muted`; reproducing the target `#1E293B` requires an explicit
implementation decision.

The current design token check is
`scripts/check-ui-design-tokens.sh`. After implementation, run the brand
validator, focused Dioxus checks, and Android/iOS visual and accessibility
checks against the exported PNGs. The snapshots are references, not a
pixel-equality requirement: fix their documented design defects first.
