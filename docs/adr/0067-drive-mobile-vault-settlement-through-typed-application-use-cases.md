<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0067: Drive mobile vault settlement through typed application use cases

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/src/app.rs`, `src/bridge.rs`, and `web/src/entry.ts`
- Related: ADR-0051 through ADR-0066, ADR-0068, and issue #31
- Superseded in part by: ADR-0068 for standalone-ledger persistence only
- Implementation state: Dioxus exposes the four wallet-facing contract calls through the retained application lifecycle; device resource baselines and real-node fixtures remain backlog work

## Context

The first Passport Vault mobile slice intentionally exercised only a
process-local conformance ledger. ADR-0068 later made that separate standalone
ledger restart-durable on supported native targets. Native Compact calls became available
through typed application services and the headless protocol, but the Dioxus
page still described live submission as a separate adapter and invoked the
standalone create, deposit, claim, and withdraw use cases directly.

The prototype supplied more behavior, but did so through an embedded dApp,
WebView JavaScript, iframe origins, ambient bridge state, a configurable admin
seed, and a high-level claim composer that derived holder material in
JavaScript. Those mechanisms conflict with Oxid's custody and adapter
boundaries. The mobile application still needs equivalent user-visible
functionality: contract state, all four calls, explicit review, authorization,
proving/submission, cancellation, durable status, and reconciliation.

## Decision

Add a product-specific Dioxus service bundle containing only the typed
Passport Vault application use cases. The mobile composition root supplies:

- authenticated contract-state read;
- prepare, authorize, submit, and retained-draft lookup;
- submission status, pre-broadcast cancellation, bounded history, and
  finalized-history reconciliation; and
- the truthful composition mode plus an optional fixed address used only by
  deterministic development simulation.

The page presents the process-local conformance ledger and Midnight contract
lifecycle as separate surfaces. `deterministic_simulation` explicitly says no
node broadcast occurs. `native_settlement` says it uses authenticated finalized
state and the protected Midnight proving/submission boundary. An unavailable
composition exposes neither as ready nor settled.

The contract-call surface supports exactly `create_lock`, `deposit_to_lock`,
`claim_from_lock`, and `withdraw_from_lock`. It rejects unknown operations,
non-canonical identifiers, zero transfer amounts, and claim requests without an
opaque verified credential identifier before invoking an adapter. The page
then requires these visible stages:

1. read/prepare from the selected contract and authenticated public state;
2. review the exact operation, amount, lock, anchor height, expiry-bound draft,
   and fee readiness;
3. authorize with the exact application challenge;
4. separately prove and submit; and
5. show public terminal inclusion or route ambiguous outcomes to
   reconciliation without creating a replacement.

Cancellation remains available only before broadcast. A cancelled authorized
draft may be retried; any other failed submission state defaults to
reconciliation. The UI never receives transaction bytes, witnesses, proofs,
credential bytes, openings, signatures, or secret keys.

Native standalone-development builds use the same environment-aware
composition as the headless executable. A complete reviewed configuration can
therefore select `native_settlement`; no configuration selects the explicit
deterministic simulator; partial or invalid configuration fails at startup.
Web builds retain the bounded simulator composition because native network and
custody adapters are unavailable there.

## Rejected alternatives

- Reusing the prototype iframe, WebView JavaScript bridge, or
  `prepareVaultClaim` would move credential and holder material outside the
  reviewed Rust custody boundary.
- Auto-authorizing and submitting from an operation button would collapse two
  security confirmations and hide the broadcast boundary.
- Treating process-local lock state as live contract state would make source
  and settlement claims misleading.
- Embedding a production contract address would recreate the prototype's
  environment coupling. Native mode requires an explicit reviewed address;
  only the deterministic simulator supplies a fixed fixture address.
- Retrying every submission failure would risk a duplicate after an ambiguous
  broadcast.

## Consequences

- Mobile, headless, and tests now enter the same application-owned vault-call
  lifecycle and receive the same public views and recovery rules.
- Managed credential claim construction remains hidden behind exact
  authorization and `vc-midnight`; the Dioxus page passes only an opaque
  credential identifier.
- Standalone behavior remains useful and visible without being confused with
  an on-chain call.
- A complete environment can drive the mobile app against the standalone
  Midnight stack without adding another bridge or composition path.
- iOS/Android live-node fixtures, screenshots, and resource baselines remain
  required before issue #31 is complete.

## Validation

- `cargo test -p oxid-ui-dioxus --lib`
- `cargo clippy -p oxid-ui-dioxus -p oxid-app --all-targets --all-features -- -D warnings`
- `./scripts/test-ios-profile-flow.sh`
- `./scripts/test-android-profile-flow.sh`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `nix develop --command ./run.sh --light --strict`
- `nix flake check`

The mobile smoke commands cover deterministic state read and the complete
prepare/authorize/prove/submit lifecycle on simulators. They are not evidence
of a real-node broadcast or device resource baseline.
