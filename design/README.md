# Oxid design profiles

This directory contains versioned, account-independent design handoffs for
Oxid. A profile is a visual and interaction target for the wallet; it is not
a runtime UI profile or a new brand build.

| Profile | Status | Contents |
| --- | --- | --- |
| [lunar-aegis](lunar-aegis/README.md) | First mobile design target for 0.2.0 | Brand assets, fonts, tokens, navigation icons, 34 screen references, and verified flow graph. |

The shipped app still uses `brands/oxid/` and the two-layer semantic token
system in `crates/ui-dioxus/assets/styles.css`. Implement a design profile
through those existing layers, with behavior and capability state supplied by
the application. Do not copy AI mock data, generated React examples, or static
HTML into production state handling.
