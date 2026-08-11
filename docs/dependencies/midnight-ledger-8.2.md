# Midnight ledger transaction dependency review

- Project: [midnight-ledger](https://github.com/midnightntwrk/midnight-ledger)
- Selected revision: `d9414884db9da9e9b1f6f3a7f742d79a5732f817`
  from the official HTTPS Git repository; `midnight-ledger` reports
  `8.2.0-rc.1` at this commit
- Direct packages: `midnight-ledger`, `midnight-base-crypto`,
  `midnight-coin-structure`, `midnight-onchain-runtime`, `midnight-serialize`,
  `midnight-storage`, and `midnight-transient-crypto`
- Features: ledger default features disabled; its `proving` feature is selected
  for canonical DUST proof orchestration
- License: Apache-2.0 for the selected direct packages; transitive license and
  source policy remain enforced by `cargo-deny` and repository checks
- Maintenance/activity: official active Midnight monorepo revision reviewed on
  2026-08-12; updates require a new immutable revision and compatibility review
- Security/audit evidence: no independent Oxid audit. Canonical structures and
  BIP340 verification are isolated inside the outgoing adapter. Inputs are
  bounded and typed, external errors are not surfaced raw, and key use remains
  behind an opaque custody port. The graph's unmaintained `bincode 2.0.1`
  advisory is explicitly bounded and tracked in issue #10; see
  `docs/security/advisory-exceptions.md`. The transitive published proof graph
  and transport/runtime dependencies are reviewed separately in
  `midnight-standalone-submission.md`.
- Android/iOS/desktop support: native Rust dependency, gated by workspace tests
  and both Tier-1 mobile target builds
- WASM/web support: deliberately excluded by the adapter's target-specific
  dependency section; browser transaction construction needs its own size and
  runtime review
- API stability: release-candidate/internal packages are not a stable public
  wallet API. Oxid-owned domain/application types and focused adapter mappings
  are the compatibility boundary.
- Reason selected: issues #9/#11 need the exact `Intent`, `UnshieldedOffer`,
  UTXO, DUST, fee, proof, runtime-cost, user-address, BIP340, and transaction
  types used by the reviewed prototype. Reimplementing their serialization or
  fee/proof rules would risk consensus incompatibility.
- Alternatives considered: copying prototype code, recreating the transaction
  format, waiting for crates.io publication, or importing the aggregate wallet
  runtime. These either lose provenance/compatibility, block current parity, or
  cross Oxid's adapter boundary.
- Cost: even with ledger defaults disabled, upstream unconditional dependencies
  include substantial transaction/proof-related code. Keep the dependency
  native-only and do not use it for constants or read models.
- Exit strategy: replace or upgrade only behind `WalletTransactionPort`, after
  canonical conformance, source policy, mobile compilation, and headless flow
  tests pass at the new source.
