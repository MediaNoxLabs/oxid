# Oxid fingerprint-crescent mark provenance

The owner selected the fingerprint-shaped crescent for the Oxid app after
reviewing the earlier Lunar Aegis shield/lock concept. The four ridge paths
in `scripts/generate-oxid-brand-assets.py` generate the safe inline `logo.svg`
and every app-icon variant. The inline mark uses `currentColor` and avoids
filters or URL-valued paint servers to satisfy the brand pack's SVG policy.

The original UX Pilot master at
`design/lunar-aegis/assets/logo-master.svg` remains unchanged. Its SHA-256,
`923cc80416d667e7fa33e99e5c134174539e66ae04e61871df70300309167ccc`,
is recorded in `design/lunar-aegis/assets/vector-sha256.json` and checked by
`design/lunar-aegis/verify.py`. See
`design/lunar-aegis/brand-icons.md` for the production choice and platform
usage.
