# Wallet presentation-shell provenance

## Source baseline

The first post-M0 migration slice was derived from the presentation structure
of `midnight-ledger` commit
`074b1a4bccbfee1740ee188374b606a022ecef42`, specifically:

- `mobile-bench/dioxus-wallet/src/app.rs` for the five primary destinations and
  mobile bottom-navigation hierarchy;
- `mobile-bench/dioxus-wallet/assets/styles.css` for visual-system and safe-area
  design evidence;
- the inline Lucide icon paths retained by that application.

The source repository and the reimplemented Oxid code are Apache-2.0 licensed.
The selected Lucide icons retain their ISC notice in
[`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md).

## Retained behavior and presentation

- primary Assets, DIDs, Credentials, Diagnostics, and Settings destinations;
- fixed mobile navigation with active-state icon and label treatment;
- dark navy surfaces with cyan, mint, and purple capability accents;
- iOS safe-area-aware page and navigation spacing;
- compact application header, overflow menu, responsive cards, focus states,
  and reduced-motion handling;
- an explicit diagnostics surface.

The Oxid mark, wordmark, components, and stylesheet are reauthored. Midnight
trademarks and wordmark assets are not republished as Oxid branding.

## Intentionally not copied

- the prototype's monolithic `app.rs` or complete stylesheet;
- wallet, ledger, indexer, node, proving, DID, credential, vault, or JavaScript
  bridge state held directly by UI components;
- remote font imports or other runtime presentation dependencies;
- splash timers, benchmark panels, demo-wallet controls, telemetry, and test
  tabs as production surfaces;
- generated native hosts, signing configuration, secrets, endpoints, databases,
  proof artifacts, or vendored JavaScript.

Until each capability slice is composed, its destination renders an explicit
unavailable/migration state. The existing Create Wallet Profile use case remains
functional as a temporary profile destination. Its final lifecycle integration
is tracked by [issue #1](https://github.com/MediaNoxLabs/oxid/issues/1).

[Issue #17](https://github.com/MediaNoxLabs/oxid/issues/17) and ADR-0032 restore
the useful behavior of the prototype's `WalletSyncPane`: a separate DUST row,
exact official-state balance, bounded current/target progress, resync, and
cancellation. Oxid reimplements the component over application use cases and
polls only an adapter-owned worker status. No ledger event, database, transport,
or key type enters Dioxus, and cached/stalled state is explicitly distinguished
from live spend readiness.

ADR-0090 keeps those independent DUST and shielded state machines but composes
them with public account refresh as one Wallet account-sync card. The one
**Sync now** / **Cancel sync** action invokes only existing application ports;
combined progress is presentation-only, and cursor/event-count diagnostics no
longer appear in the normal user surface. The same decision splits onboarding
into create/restore routes and introduces a timestamp-only application receipt
so Home/Settings can say **Backed up** only after the native document exporter
actually succeeds.

ADR-0091 reuses the prototype `AddressCard` / `AddressRow` evidence without
copying its component state or bridge facade. Home now opens one bounded
Receive secondary route with a large selected QR, dynamic human address-kind
selectors, grouped display-only preview, and typed native Copy/Share. Oxid adds
a stricter protected-account admission rule: simulation fixtures and
watch-only fallbacks cannot be presented as holder-controlled destinations.
The existing headless `wallet.address.list|unshielded|shielded` methods remain
the UI-independent conformance surface; no modal state enters the protocol.
