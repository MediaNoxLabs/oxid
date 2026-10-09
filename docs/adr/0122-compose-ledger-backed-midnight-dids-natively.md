# ADR-0122: Compose ledger-backed Midnight DIDs natively

- Status: Accepted
- Date: 2026-10-09
- Source: ADR-0015, ADR-0017, ADR-0037, ADR-0046, ADR-0118, and [issue #1137](https://github.com/MediaNoxLabs/oxid/issues/1137)
- Depends on: immutable Ledger8-compatible revisions of `midnight-ledger`, `compact`, `midnight-zk`, and `midnight-identity`
- Supersedes: the external JavaScript DID call composer admitted as an interim bridge by ADR-0037
- Amends: ADR-0037's statement that live Compact writes remain queued

## Context

The physical Tailnet issuance journey requires a holder DID that the issuer can
resolve from the selected Midnight realm. A self-contained off-chain DID is a
valid network-free credential holder identifier, but it cannot stand in for a
ledger-backed DID in this journey. The previous deployment pipeline stopped at
an external JavaScript composer. That process boundary was unsuitable for iOS
and Android, depended on filesystem executables and environment variables, and
would have required secret-bearing DTOs or a WebView bridge.

The reusable `midnight-identity` workspace now exposes generated Rust bindings
for the official Midnight DID 0.5 Compact contract. The implementation remains
on Ledger8; Ledger9 and the experimental Compact AST branch are out of scope.

## Decision

Oxid composes DID deployment, verifier-key maintenance, and the four bootstrap
document writes inside the Rust process. `midnight-did-runtime` executes the
generated Compact constructor and circuits. The adapter converts their typed
outputs into Ledger8 unproven transactions; the existing Midnight wallet
boundary continues to own DUST balancing, proof generation, journal-before-
broadcast, submission, finality, and reconciliation.

Dependencies are immutable and coherent:

- `midnight-ledger` supplies one Ledger8 family with its mobile-safe storage
  feature boundary;
- `compact` supplies one Rust runtime generated for that Ledger family;
- `midnight-identity` supplies the DID 0.5 domain, Jubjub authorization, and
  generated runtime crates;
- authenticated DID 0.5 release artifacts supply prover and verifier keys;
- `midnight-zk` supplies the exact proof patch selected by Ledger8.

The repository source-policy check rejects mutable, local-path, duplicate, or
mixed revisions of these packages.

Desktop and headless profiles authenticate the closed artifact set from an
absolute immutable store path. Explicit mobile Portal profiles embed only the
three admitted bootstrap proving closures at build time and re-authenticate
their bytes against the same release manifest before installing the service.
Normal mobile builds do not carry this feature or its artifact weight.

Controller, recovery, maintenance, and replay-randomness roles use distinct HD
paths. Controller and recovery Jubjub seeds are borrowed only inside custody
callbacks. Authorization signs the complete four-field circuit digest through
the shared `midnight-identity` helper. The maintenance key is sampled
deterministically from protected seed material instead of interpreting an
arbitrary 32-byte wallet seed as a scalar. Private Compact state contains only
public keys and the current timestamp; secret witness outputs are not
serializable DTOs.

The adapter returns only a bounded serialized unproven transaction, its public
planning fingerprint, the public DID, and expiry metadata. It exposes no
private key, seed, mnemonic, witness transcript, child-process command, or
ambient executable path. The retired JavaScript composer, its Nix package, and
its environment variable are removed.

The holder becomes ready only after deployment and the ordered maintenance and
bootstrap effects have been submitted, finalized, indexed, and resolved with
the required authentication and assertion methods. No UI shortcut may publish
an undeployed identifier as though it were ledger-backed.

## Consequences

- Headless, desktop, iOS, and Android can share one native composition path.
- Mobile builds no longer depend on Node, a WebView, or an external executable
  to create or update a holder DID.
- Contract and proof artifacts remain authenticated build inputs rather than
  runtime downloads.
- A DID deployment still requires a funded wallet, ready DUST capability,
  healthy node/indexer/prover endpoints, and successful reconciliation.
- Portal interoperability remains an end-to-end qualification concern; this
  ADR does not claim readiness until the exact Tailnet issuance scenario passes.
- The currently pinned Portal uses Midnight DID 0.4 while this native runtime
  uses 0.5. Those contract shapes are intentionally not assumed compatible;
  Portal resolution must move to the reviewed 0.5 line in
  [lace-id-portal issue #127](https://github.com/input-output-hk/lace-id-portal/issues/127)
  before qualification.
- Upstream immutable commits must be published before Oxid's hermetic remote
  build can fetch and verify their recorded Nix source hashes.
