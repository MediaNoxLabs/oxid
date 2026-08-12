# ADR-0029: Expose standalone wallet flows on mobile without weakening production composition

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3, 6–8, 12–13, 16–18 and [issue #14](https://github.com/MediaNoxLabs/oxid/issues/14)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`
- Implementation state: Explicit development mobile composition, protected account activation, receive QR, and staged simulated transfer UI implemented; native custody and live production routes remain fail-closed

## Context

Oxid already implements the complete development/headless unshielded NIGHT
transaction sequence: protected derivation, synchronization, canonical prepare,
human-readable authorization, DUST balancing, proving, submission, retry-safe
state, and public inclusion results. The Dioxus incoming adapter previously
consumed only account reads, however, so the simulator application could not
exercise the same wallet journey. The prototype exposes receive QR and mobile
wallet actions, but also couples its UI directly to aggregate wallet services,
process secrets, network clients, and JavaScript bridges.

The normal Oxid composition deliberately uses unavailable custody and chain
adapters. Selecting development custody implicitly in that composition would
turn a simulator convenience into a false production security claim.

## Decision

Add an explicit `oxid-app/standalone-development` Cargo feature. Repository
simulator and emulator launch scripts select it, while default application
builds continue to call `oxid_composition::compose()` and remain fail-closed.
The feature calls the existing `compose_headless()` development composition:
public profiles remain in the durable JSON store, while generated root and
derived children remain process-local and disappear on restart. No network,
node, indexer, or prover is contacted by this zero-configuration mode.

Extend the Dioxus service bundle with the existing focused protection,
derivation, and transaction use cases. Dioxus renders state and emits commands;
it does not call storage, custody, or Midnight SDKs. Account activation follows
status → initialize or unlock → derive `m/44'/2400'/0'/0/0` → synchronize.
Normal composition reports the same controls unavailable.

Expose the retained transaction stages separately:

1. parse a user-entered NIGHT decimal into an exact six-decimal atomic string;
2. prepare and render recipient, amount, change, network, input count, and the
   still-pending DUST fee;
3. authorize only after an explicit human-readable review action;
4. prove and submit only after a second explicit action; and
5. render only the final public transaction ID, block ID, mode, and DUST fee.

Draft IDs and public authorization challenges may enter UI state. Signing
payloads, signatures, transaction bytes, proof witnesses, root/child secrets,
and private key references may not. Submission remains off the UI thread.
After a submission error, the UI queries the retained draft and offers a retry
only when the adapter positively reports `authorized`. A `submitting` draft,
any other unexpected state, or a failed draft lookup requires reconciliation
and never offers an editable replacement flow.
Process-local drafts do not survive restart. The headless protocol remains the
reference automation adapter for error, cancellation, retry, and idempotency
contracts; a later mobile slice may add an explicit cancellable task control
without changing those contracts.

Generate receive QR images inside the Dioxus adapter with `qrcode 0.14.1`,
default features disabled and only SVG enabled. The input is exactly one
already-validated public address from the account view. The generated matrix is
deterministic, no address is interpolated into markup, and no JavaScript,
camera, clipboard, file, or network capability is involved. Camera scanning and
native copy/share remain separate platform adapters.

## Consequences

- iOS simulators and Android emulators can exercise the same safe application
  boundaries as the standalone headless wallet.
- The development warning and process restart make ephemeral custody explicit;
  this mode must never be used to hold real assets.
- Default desktop/mobile/web composition retains the production fail-closed
  boundary from ADR-0017.
- Receive QR parity no longer depends on the prototype's WebView JavaScript or
  generated native projects.
- Live mobile standalone routes remain out of scope because route discovery,
  native custody, background synchronization, and production policy are not yet
  implemented.
