# Bundled typefaces

The profile uses Space Grotesk for headings, Plus Jakarta Sans for body and
controls, and JetBrains Mono for addresses, DIDs, and hashes. Their variable
TTF files were copied on 27 September 2026 from the official
[Google Fonts repository](https://github.com/google/fonts/tree/main/ofl).
The corresponding OFL licenses are stored beside the files.

| Role | File | Source directory |
| --- | --- | --- |
| Headings | `SpaceGrotesk[wght].ttf` | [spacegrotesk](https://github.com/google/fonts/tree/main/ofl/spacegrotesk) |
| Body | `PlusJakartaSans[wght].ttf` | [plusjakartasans](https://github.com/google/fonts/tree/main/ofl/plusjakartasans) |
| Technical data | `JetBrainsMono[wght].ttf` | [jetbrainsmono](https://github.com/google/fonts/tree/main/ofl/jetbrainsmono) |
| Ukrainian fallback | `NotoSans[wdth,wght].ttf` | [notosans](https://github.com/google/fonts/tree/main/ofl/notosans) |

The included Space Grotesk and Plus Jakarta Sans files do not contain the
Ukrainian glyphs checked in this export (`іїєґІЇЄҐ`). Noto Sans and
JetBrains Mono do. Use Noto Sans as the explicit Cyrillic fallback for
headings and body text. Verify the actual font stack and line wrapping on
Android and iOS before release; the UX Pilot screenshots loaded web fonts
and do not prove bundled-font rendering.
