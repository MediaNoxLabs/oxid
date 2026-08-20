# ADR-0091: Present receive as a typed address sheet

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 3, 6–8, 12–13, 16, and 18
- Design source: `docs/design/journeys.md` Receive journey
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/src/app.rs` `AddressCard` / `AddressRow`
- Tracking: issues #2, #65, and #83
- Implementation state: Dioxus owns a bounded Receive secondary route over existing account and typed public-text ports

## Context

Oxid already projects every public address returned by the wallet account use
case, renders deterministic Rust QR codes, and permits native Copy and Share
only through `PublicReceiveAddress`. The Home **Receive** quick action still
switches to the complete Wallet page, where activation, network, synchronization,
activity, receive, and send controls compete for attention.

The reviewed prototype renders unshielded and shielded addresses as compact
rows with independent QR toggles. The product design asks for the same useful
address choice as a one-tap sheet with one large selected QR. It also names a
future **Fee account** choice. That label cannot authorize inventing an address:
some account snapshots contain a public DUST fixture, while protected derived
accounts currently expose only their actual unshielded and shielded rails.

## Decision

Add **Receive** as a non-primary route in the existing bounded Dioxus route
stack. Home pushes that route instead of selecting Wallet. The app keeps the
current primary surface underneath and renders one modal bottom sheet with an
explicit Close action; closing or Back pops presentation state only.

The sheet reads the active profile through the existing
`GetWalletAccountUseCase` on `run_ui_blocking`. It renders loading, failed,
no-protected-address, and populated states. It never calls a repository,
custody adapter, Midnight adapter, or native plugin directly.

Only addresses present in the returned account view become selector capsules.
The user labels are **Public** for `unshielded`, **Private** for `shielded`,
and **Fee account** for `dust`; other known or unknown values receive neutral
labels. No absent rail is synthesized. Before any destination is shown, the
account must have the protected derived-account identity and both protected
receive rails already required by the Wallet page. Development fixture or
watch-only addresses therefore cannot be presented as holder-controlled
receive destinations.

The selected full address is the sole input to all three outputs:

1. deterministic Rust `qrcode` SVG rendering;
2. `PublicReceiveAddress` followed by the existing typed native Copy port; and
3. `PublicReceiveAddress` followed by the existing typed native Share port.

The visible preview may group and middle-truncate the address for a narrow
screen, but its title/accessibility label retains the complete public value and
the sheet keeps the existing exact-address guarantee. Selection and export
notice state are component-local and disappear when the route is popped.

## Security and architecture boundaries

- This decision adds no address derivation, discovery, rotation, payment
  request, amount, or transaction authority.
- Dioxus remains an incoming adapter. `WalletAccountView` and
  `PublicReceiveAddress` remain the only data boundaries used by the sheet.
- Copy and Share remain closed to typed public receive addresses. There is no
  generic text, credential, proof, protocol, key, or secret export method.
- QR output contains exactly the selected complete address; the grouped preview
  is never used as payload input.
- Simulation/source labels remain visible. A public fixture never becomes a
  protected receive address merely because its syntax is valid.
- The existing headless `wallet.address.list|unshielded|shielded` methods remain
  the UI-independent conformance harness and gain no modal/UI protocol state.

## Consequences

- Home reaches a focused receive ceremony in one tap without losing its root
  route or exposing unrelated Wallet controls.
- Public/private address choice is obvious, while future returned address kinds
  can appear without changing the route contract.
- A user without a protected derived account gets a truthful activation path to
  Wallet instead of a simulated fixture destination.
- Wallet may retain its complete operational receive card for account details
  and regression compatibility; the sheet is a composition of the same public
  values, not a second source of truth.

## Validation

- Dioxus tests cover route push/pop, human selector labels, protected-address
  admission, grouped preview behavior, and address-specific QR output.
- iOS and Android standalone flows cover the fresh activation fallback, then
  Public/Private selection, large QR, native Copy/Share, and Close.
- Existing headless derivation/address-list tests prove both protected address
  rails without adding incoming modal state.
- Strict architecture, source, UI label/token, coverage, advisory, license, and
  documentation gates remain required.

## Rejected alternatives

- Keeping Home **Receive** as a Wallet-tab redirect misses the one-tap ceremony
  and exposes unrelated controls.
- Adding a receive-specific application aggregate would duplicate the existing
  safe account view without a new domain rule.
- Always rendering a Fee account capsule would fabricate capability when no
  DUST address exists.
- Passing the grouped preview to QR/Copy/Share would corrupt the destination.
- Widening native export to arbitrary strings would violate ADR-0070.
