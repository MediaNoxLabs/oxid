<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0052: Authenticate and decode Passport Vault contract state natively

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/vault` and its WebView `readVaultLedger`/`readVaultLocks` bridge
- Contract source: `midnight-identity-solution-examples` commit `e4a92a6be2cc6dc34f68261f10c19c9312043807`, `packages/contracts/vault/src/passport-vault.compact`, SHA-256 `2ebc5b34dd440bc9a9736408f29f5003e7a78f26a564b392be2af36de69102f4`
- Related: ADR-0003, ADR-0004, ADR-0006, ADR-0015, ADR-0020, ADR-0022, ADR-0035, ADR-0044, ADR-0050, ADR-0051, and issue #31
- Implementation state: immutable Nix artifact composition, exact native Rust state decoding, deterministic generated-client fixture, and a read-only headless decoding method are implemented; authenticated indexer acquisition, cached/freshness composition, and contract-call transactions remain issue #31

## Context

The prototype reads a tagged Midnight `ContractState` from the indexer, passes
its hex bytes into a WebView, loads generated JavaScript and the Compact
runtime, and calls the generated `ledger(...)` reader. Its contract-call path
similarly composes an unproven transaction in JavaScript before returning the
private transaction bytes to Rust for balancing, proving, and submission.

That approach preserved demo behavior but made state interpretation depend on
an ambient companion checkout, ignored generated files, Node module resolution,
and a foreign runtime inside the wallet. The existing Oxid transaction port is
intentionally specific to unshielded NIGHT transfers. Treating that port as a
generic contract-call facility would erase operation arguments, contract state,
proof artifacts, and reconciliation semantics.

The reviewed contract also has five impure circuits, including the admin-only
`setTrustedIssuer`, even though the user journey uses create, deposit, claim,
and withdraw. Its public ledger has an exact 15-field layout and contains the
trusted issuer anchor, lock map, per-lock credential-root nullifier set,
accounting totals, and last-claim audit fields.

## Decision

Oxid authenticates the Passport Vault contract through an immutable Nix input
at the exact companion revision. The artifact derivation also pins the VC
Compact sources, Compact toolchain, and circuit parameters. It compiles all
five impure circuits, records each artifact digest, and fixes the measured
complexities at `k=13/5416`, `k=11/1823`, `k=10/834`, `k=17/124785`, and
`k=11/1212` rows for issuer rotation, create, deposit, claim, and withdraw.
Generated sources and proving artifacts remain Nix-store outputs rather than
Git content.

The Passport Vault application owns a bounded contract-state decoder port and
incoming use case. The native product adapter implements it with the already
pinned Midnight Rust ledger types and tagged serializer. It requires contract
version 1, the exact 15-field ledger shape, at most 4,096 contiguous locks,
matching lock/global accounting, a consumed-nullifier count equal to the claim
count, valid enum values, no trailing bytes, and a 16 MiB input limit.

Decoded views are labelled `pinned_contract_layout`, not `live` or `cached`.
That label authenticates only the decoder schema, never the caller-supplied
state bytes. They expose only public policy, coin-public-key locker identity,
aggregate accounting, issuer anchor, and redacted public audit fields; the last
credential root is decoded for layout integrity but not projected. The adapter
does not choose an address, fetch an indexer, infer block freshness, or
authorize a claim. A deterministic 2,013-byte fixture is produced by the
generated Compact 0.15.0 client from the pinned source and checked natively and
through the headless `vault.contract_state.decode` method.

Contract transactions require a new capability-specific port that preserves
the existing prepare/authorize/prove/submit/reconcile safety model while owning
contract address, authenticated initial state, circuit arguments, retained
private witnesses, artifact identity, and public submission journal state.
Until that port exists, no Passport Vault operation is reported as live,
submitted, included, or retryable.

## Rejected alternatives

- Importing the prototype's JavaScript bridge, Node runtime, iframe, or
  relative workspace lookup would violate the Rust-first and reproducibility
  boundaries.
- Checking generated JavaScript or proving keys into Git would duplicate large
  derivable artifacts and weaken source authentication.
- Decoding only aggregate totals would omit the trusted issuer, per-lock
  policy, nullifier accounting, and integrity relationships required by a
  claim decision.
- Labelling a caller-supplied fixture `live` would conflate valid decoding with
  authenticated acquisition and recent finalized chain state.
- Reusing the unshielded transfer port for Compact calls would create a false
  abstraction and unsafe retry behavior.

## Consequences

- Oxid can interpret the exact public Passport Vault ledger on iOS, Android,
  desktop, and headless native targets without a foreign runtime.
- Source, compiler, circuit parameters, generated schema, keys, and measured
  circuit size are one reproducible closure.
- The decoder is useful independently of network availability and can be
  fixture-tested, but it does not by itself establish address authenticity,
  finality, freshness, or transaction capability.
- Issue #31 is narrowed to authenticated state acquisition/caching, explicit
  contract configuration, native call composition/proving/submission, and
  truthful mobile live/cached/unavailable presentation.

## Validation

- `nix build .#passport-vault-compact-artifacts` compiles and hashes all five
  circuits from the immutable inputs.
- Adapter tests decode the generated-client fixture and reject malformed or
  trailing data.
- Headless tests expose the same decoded public view and stable safe failures.
- The normal production composition still fails closed for every state-changing
  Passport Vault operation.
