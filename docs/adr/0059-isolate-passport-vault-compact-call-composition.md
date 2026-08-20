<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0059: Isolate Passport Vault generated-Compact call composition

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/web/src/entry.ts` and `mobile-bench/wallet-core/src/wallet.rs`
- Contract source: `midnight-identity-solution-examples` commit `e4a92a6be2cc6dc34f68261f10c19c9312043807`, distributed byte-identically at `contracts/passport-vault/passport-vault.compact`
- Related: ADR-0013, ADR-0015, ADR-0017, ADR-0027, ADR-0028, ADR-0051 through ADR-0058, and issue #31
- Implementation state: a reproducible bounded composer produces Rust-compatible unproven `createLock`, `depositToLock`, and `withdrawFromLock` transactions; protected claim composition, application-port wiring, NIGHT/DUST completion, submission, and reconciliation remain pending
- Amended by: ADR-0065

## Context

The Passport Vault client is generated JavaScript. Reimplementing its circuit
execution in wallet core would create a second ABI and transcript authority.
The prototype instead runs the generated client in the Dioxus WebView and
accepts arbitrary circuit identifiers, raw arguments, private state, public
keys, and serialized chain state across a JavaScript bridge. Its claim helper
also derives a holder scalar from public credential data and uses fixed
presentation randomness. Those boundaries cannot move into Oxid.

ADR-0058 authenticates the exact generated module and four wallet proof
circuits. A generated-Compact executor is still needed to turn one typed
operation and authenticated state into an official unproven Midnight
transaction. That executor is an outgoing adapter detail, not a new incoming
wallet API and not evidence of funding, proof, broadcast, or settlement.

## Decision

Oxid packages a one-request Node 24 composer as
`passport-vault-call-composer`. Its dependency lock pins the generated
client-compatible published Midnight packages: Compact JS 2.5.0, Compact
runtime 0.15.0, ledger-v8 8.0.3, and midnight-js 4.0.2. The composer remains
outside every domain/application crate and is available only to the Passport
Vault outgoing adapter and its conformance gate.

The Nix wrapper fixes `OXID_PASSPORT_VAULT_ARTIFACTS_DIR` to the exact
`passport-vault-compact-artifacts` store closure and clears `NODE_OPTIONS` and
`NODE_PATH`. The generated module is loaded unchanged from that closure. A
narrow Node resolution hook supplies only its reviewed bare
`@midnight-ntwrk/compact-runtime` import from the locked composer dependency;
all other resolution follows the normal package graph. No generated module,
key, IR, parameter, or `node_modules` tree enters Git.

The process reads one bounded JSON object from stdin, writes one JSON result,
and exits. The schema admits only:

- `create_lock` with the typed policy and optional initial amount;
- `deposit_to_lock` with a canonical `Uint<64>` lock ID and positive
  `Uint<128>` amount; or
- `withdraw_from_lock` with the same bounds and the active wallet's public
  unshielded recipient address.

It also admits only the public account encryption/coin keys and bounded current
contract, Zswap, ledger-parameter, address, and network values needed by the
official builder. Object keys are exact; hexadecimal and decimal encodings are
canonical; policy and integer bounds mirror the Rust application boundary.
`claim_from_lock` fails with `claim_requires_protected_custody` before loading
the generated artifacts. `set_trusted_issuer` and unknown operations fail
closed. Private credentials, openings, holder keys, nonces, witnesses,
signatures, proofs, serialized input transactions, and raw circuit arguments
are not members of the schema.

Successful output contains the operation/circuit identity and the official
serialized unproven transaction. That result is adapter-owned material: it may
be deserialized and retained behind a call draft, but must never be projected
through the headless/mobile incoming protocols or logs. Errors use a fixed safe
taxonomy and never return a JavaScript stack, input value, or external body.

The Nix install check executes the real generated `createLock` circuit against
the pinned public contract-state fixture and public standalone wallet vector.
A Rust conformance test then decodes the result as the pinned
`midnight-ledger` `UnprovenTransaction` and requires one standard intent. This
establishes generated-client/ledger codec compatibility without claiming the
transaction is balanced or valid for broadcast.

## Rejected alternatives

- Keeping the prototype WebView bridge would make mobile UI infrastructure a
  transaction authority and preserve its raw/private call surface.
- Accepting arbitrary circuit names or argument arrays would bypass the
  application operation model and expose the administrative circuit.
- Implementing claim composition with public derived holder material or fixed
  nonces would migrate a known security shortcut.
- Copying the generated closure into the composer would create a second runtime
  artifact route; the fixed Nix input keeps one immutable source.
- Treating an unproven transaction as a submitted call would conflate
  composition with NIGHT funding, DUST balancing, proof, and node acceptance.

## Consequences

- Oxid now has an official generated-client composition oracle for three public
  wallet operations, reproducible on Darwin and Linux.
- The Rust/Nix build proves that Git Midnight dependencies and published
  JavaScript compatibility packages form one codec-compatible stack.
- Claim remains deliberately unavailable until the adapter can load the opaque
  credential and invoke managed holder custody with fresh randomness.
- Live `native_pending` and `settlesOnMidnight: false` capability labels do not
  change until the retained port owns composition plus completion/submission.

## Validation

- `npm test` rejects claim, administration, unknown fields, zero/non-canonical
  amounts, and secret-shaped schema expansion before artifact loading.
- `nix build .#passport-vault-call-composer --print-build-logs`
- The Nix install check composes a 1,014-byte `createLock` transaction with the
  authenticated generated module and fixture.
- `cargo test -p oxid-adapter-passport-vault --lib` independently decodes that
  output into the pinned Rust ledger transaction type.
- `cargo clippy -p oxid-adapter-passport-vault --all-targets -- -D warnings`
