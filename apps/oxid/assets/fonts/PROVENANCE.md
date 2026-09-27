# Lunar Aegis bundled fonts

These offline font assets are copied from `design/lunar-aegis/fonts/`, whose
`README.md` records the 27 September 2026 Google Fonts OFL source and each
font's role. Their adjacent OFL license texts are included unchanged.

| Role | Asset | Fallback |
| --- | --- | --- |
| headings | `SpaceGrotesk[wght].ttf` | Noto Sans |
| body and controls | `PlusJakartaSans[wght].ttf` | Noto Sans |
| addresses, DIDs, hashes | `JetBrainsMono[wght].ttf` | Noto Sans Mono |
| Ukrainian/Cyrillic | `NotoSans[wdth,wght].ttf` | platform sans |

No network font request is required. `crates/ui-dioxus/assets/styles.css`
loads these app-packaged assets; device rendering remains subject to the
mobile visual qualification recorded for the issue.
