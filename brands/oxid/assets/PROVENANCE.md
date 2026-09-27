# Lunar Aegis vector mark provenance

`logo.svg` is the safe inline-SVG normalization of the approved Lunar Aegis
master at `design/lunar-aegis/assets/logo-master.svg`. The source master SHA-256
is `923cc80416d667e7fa33e99e5c134174539e66ae04e61871df70300309167ccc`, as
recorded in `design/lunar-aegis/assets/vector-sha256.json` and verified by
`design/lunar-aegis/verify.py`.

The normalization preserves the shield, crescent, cross, and identity-aperture
silhouette while removing filters and URL-valued paint servers. This keeps the
app brand pack compatible with its fail-closed inline SVG policy and legible at
its 16 px favicon and 32 px navigation targets. Store-icon raster generation
is intentionally outside this foundation.
