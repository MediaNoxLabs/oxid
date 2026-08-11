# ADR-0014: Cardano library selection

- Status: Proposed
- Date: 2026-08-11
- Blueprint source: Sections 8 and 17
- Implementation state: Research required before M1

## Context

M1 requires a complete Cardano vertical slice across account discovery,
addresses, balances, history, fee estimation, transaction construction,
review, signing, submission, and receive QR flows. A library choice will shape
mobile portability and the adapter maintenance burden.

## Proposed decision

Evaluate Pallas-family crates and other maintained Rust alternatives against
the required capabilities. Record exact versions, licenses, maintenance and
audit evidence, Android/iOS/desktop/WASM support, cryptographic dependencies,
API stability, and an exit strategy using the repository dependency-review
template.

Select the smallest maintained set that can remain inside Cardano adapter
crates. Do not expose library models or assume Cardano semantics in the
chain-neutral domain.

## Consequences if accepted

- M1 starts only after a capability matrix and executable mobile spike exist.
- Multiple focused libraries may be preferable to one broad SDK.
- Signing must still use key-operation ports and protected key references.
- This proposal does not select or authorize a dependency today.
