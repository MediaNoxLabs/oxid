<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0060: Retain native Passport Vault composed drafts

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/web/src/entry.ts` and `mobile-bench/wallet-core/src/wallet.rs`
- Related: ADR-0013, ADR-0015, ADR-0017, ADR-0026 through ADR-0028, ADR-0033 through ADR-0035, ADR-0051 through ADR-0059, and issue #31
- Implementation state: canonical-replay create/deposit/withdraw operations can be composed and retained behind the native contract-call port; live context provision, protected claim composition, NIGHT funding, DUST completion, proving, submission, durable journaling, and reconciliation remain pending

## Context

ADR-0059 introduced a reproducible generated-Compact composer but exercised it
only as a process/codec conformance boundary. The application already owns a
typed retained Passport Vault call lifecycle under ADR-0056. Connecting those
pieces requires public account and chain inputs that do not belong in incoming
commands or the chain-neutral wallet read model: the selected network, current
global Zswap state, current ledger parameters, coin/encryption public keys, and
the active unshielded recipient.

The prototype passes those values and the resulting serialized transaction
through the Dioxus WebView bridge. It then combines funding, DUST balancing,
proof generation, and submission in Rust. Moving that bridge directly would
make UI infrastructure a transaction authority and would expose a serialized
unproven transaction outside the outgoing adapter.

## Decision

The Passport Vault outgoing adapter owns
`NativePassportVaultContractCall`. It implements the existing application
`PassportVaultContractCallPort` and accepts only snapshots authenticated as
`canonical_finalized_replay`. Claim remains unavailable before any public
context lookup or composer invocation.

A narrow adapter composition source supplies fresh public Midnight context by
opaque profile identifier. The source is separate from the generic wallet
display snapshot and contains no secret key, credential, opening, nonce,
witness, proof, signature, or serialized transaction. To prevent a conformance
shortcut from becoming live behavior, the native context requires non-empty
bounded serialized Zswap state and ledger parameters; the composer's `null`
defaults are not accepted on this retained path. Network identifiers and
32-byte public key/address payloads are validated before invocation.

The process adapter requires an absolute canonical regular executable and
rejects symlinks. It removes Node loader override variables, writes one bounded
request, drains bounded stdout/stderr concurrently, enforces a 60-second
deadline, and accepts only the exact success/failure response schemas from
ADR-0059. The returned transaction must decode completely as the pinned Rust
`UnprovenTransaction`, be standard, use the requested network, and contain
exactly one intent.

Preparation hashes the authenticated state, typed operation, expiry, and all
public composition context into a profile-scoped planning fingerprint. The
adapter retains the serialized unproven transaction in a zeroizing buffer and
exposes only the existing safe preview and challenge. Authorization changes
the retained public state but does not sign, prove, or submit the transaction.
Expiry erases the retained bytes.

Submission deliberately returns `Unavailable` while preserving an authorized
draft. Submission status remains `not_started`; cancellation and reconciliation
remain inapplicable. The composition root therefore remains `native_pending`
and `settlesOnMidnight: false` until the Midnight adapter can supply fresh
context and consume the retained transaction through protected funding,
combined DUST/contract proving, durable pre-broadcast journaling, node
submission, and finalized reconciliation.

## Rejected alternatives

- Decoding public coin/encryption keys from a UI address inside the Passport
  Vault adapter would create a second Midnight address authority.
- Making the Passport Vault adapter depend directly on the Midnight adapter
  would couple two outgoing adapters and bypass the composition root.
- Passing chain context through headless/mobile commands would let callers
  choose transaction authority and chain parameters.
- Using empty Zswap state or initial ledger parameters in live preparation
  would turn test defaults into false chain authority.
- Marking authorization as settlement-ready would conflate user confirmation
  with NIGHT signatures, DUST balancing, contract proof generation, and node
  acceptance.

## Consequences

- The generated composer is now installed behind a real retained application
  port implementation for the three public operations.
- Serialized unproven transaction bytes never cross the outgoing adapter and
  are erased on expiry or drop.
- A future composition bridge can join the Passport Vault adapter to fresh
  public context exported by the native Midnight stack without adding an
  adapter-to-adapter dependency.
- Claims and live settlement remain visibly unavailable, so existing mobile
  and headless capability labels stay truthful.

## Validation

- `cargo test -p oxid-adapter-passport-vault --lib`
- `cargo clippy -p oxid-adapter-passport-vault --all-targets -- -D warnings`
- `nix develop --command cargo test -p oxid-adapter-passport-vault --lib compact_composer_conformance::packaged_composer_emits_a_rust_compatible_unproven_call_when_configured -- --exact`
- The conformance test serializes real initial Rust Zswap/ledger state, invokes
  the Nix-fixed composer through `NativePassportVaultContractCall`, validates
  the official transaction codec, and retains only a safe prepared preview.
