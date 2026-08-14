<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0058: Authenticate Passport Vault call artifacts at runtime

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–8, 12–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/dioxus-wallet/web/src/entry.ts` and `mobile-bench/wallet-core/src/wallet.rs`
- Contract source: `midnight-identity-solution-examples` commit `e4a92a6be2cc6dc34f68261f10c19c9312043807`, distributed byte-identically at `contracts/passport-vault/passport-vault.compact`
- Related: ADR-0013, ADR-0015, ADR-0017, ADR-0027, ADR-0028, ADR-0048 through ADR-0057, and issue #31
- Implementation state: exact generated-client/ABI authentication and a native four-circuit key/IR/parameter resolver are implemented; call composition, combined DUST proving, funding, submission, and reconciliation remain pending

## Context

ADR-0052 produces the complete Passport Vault Compact artifact closure, and
ADR-0056 defines a typed retained call lifecycle. A live call adapter still
needs to execute one generated wallet circuit and prove both that circuit and
the transaction's DUST spend. A Nix store path alone is not a runtime trust
decision: a caller could supply another directory, a changed manifest, or a
symlinked file. The generic DUST proving path also resolves only the DUST
circuit, while the artifact closure contains the administrative
`setTrustedIssuer` circuit beside four wallet circuits.

The generated client is important conformance evidence, but the prototype's
WebView bridge is not an acceptable wallet boundary. In particular, its claim
path derives holder material from public credential data and uses fixed
randomness. Authenticating the generated module must not authorize that claim
construction or expose a general raw-circuit interface.

## Decision

The Passport Vault adapter owns a native runtime artifact capability configured
by one absolute, normalized, canonical directory. It rejects a symlinked root,
artifact directory, parent component, or file. It reads a bounded manifest and
requires the exact reviewed contract, credential, Compact toolchain revisions,
compiler/runtime versions, five-circuit inventory, degrees, row counts, and
parameter digests.

Every artifact the wallet will execute is independently pinned in Rust by
relative path, exact byte length, and SHA-256 digest. This set contains the
generated ES module and TypeScript ABI, compiler contract metadata, and the
prover key, verifier key, binary ZKIR, and parameters for `createLock`,
`depositToLock`, `claimFromLock`, and `withdrawFromLock`. Admission streams the
large files through a fixed buffer rather than retaining roughly 70 MiB merely
to authenticate them. It parses each exact binary ZKIR and verifies its encoded
circuit degree. Row counts remain bound by the exact ZKIR digest and manifest;
expanding the 124,785-row claim model is deferred to proof checking rather than
adding about a minute to wallet startup.

The capability implements Midnight's native key resolver and proving-parameter
provider for exactly the four wallet circuit identifiers. The shared degree-11
parameters serve create and withdraw; degrees 10 and 17 serve deposit and
claim. Unknown identifiers and `setTrustedIssuer` resolve to no key, and degree
13 is unavailable through this product resolver. DUST continues to use its
separate official resolver until a reviewed combined provider is composed.

A future bounded headless generated-Compact composer may receive the
authenticated module bytes as adapter-owned data. It may not receive an
ambient artifact route, expose arbitrary circuit identifiers or arguments, or
move credential/opening/key/nonce material into the incoming protocol. Claim
composition must reload the opaque protected credential, use current managed
holder custody and fresh randomness, and independently check the holder proof.

The Nix workspace test build receives the immutable artifact closure explicitly
and executes runtime authentication/resolution. Development shells export the
same closure through `OXID_PASSPORT_VAULT_ARTIFACTS_DIR`; presence of this
variable alone does not enable live calls.

## Rejected alternatives

- Trusting only `manifest.json` would allow an attacker to change the manifest
  and artifacts together.
- Reusing the DUST-only resolver would fail contract proofs or encourage an
  unauthenticated ambient lookup.
- Resolving `setTrustedIssuer` through a wallet adapter would turn deployment
  authority into an end-user capability.
- Loading the prototype WebView bridge would violate the Rust-first boundary
  and retain unsafe claim key/nonce construction.
- Expanding the full claim constraint model during startup would impose proof-
  scale latency before the user has requested a proof.

## Consequences

- Generated client identity, exact wallet ABI, proof keys, ZKIR, and parameters
  now have a runtime authentication boundary instead of only a build-time one.
- The eventual native submission path can use standard Midnight resolver traits
  without giving the wallet administrative circuit authority.
- Large proof artifacts are authenticated with bounded memory and loaded as
  proof material only when their circuit is selected.
- This change does not compose a call transaction, fund it, prove DUST, submit
  it, or change `native_pending`/`settlesOnMidnight: false` capability labels.

## Validation

- Configuration tests reject relative and parent-traversing roots.
- Mapping tests cover all four call kinds and exclude `setTrustedIssuer`.
- The real Nix closure is streamed, authenticated, ABI-checked, and used to
  resolve the deposit circuit plus degree-10 parameters.
- Degree 13 and administrative key lookup fail closed.
- `cargo test -p oxid-adapter-passport-vault --lib`
- `cargo clippy -p oxid-adapter-passport-vault --all-targets -- -D warnings`
- `nix flake check --print-build-logs`
