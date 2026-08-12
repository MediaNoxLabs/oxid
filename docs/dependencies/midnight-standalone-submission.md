# Midnight standalone submission dependency review

- Reviewed: 2026-08-12
- ADR: [ADR-0027](../adr/0027-complete-standalone-midnight-transaction-submission.md)
- Scope: native development/headless DUST synchronization, proving, and node submission

## Selected packages and sources

- `midnight-onchain-runtime 3.1.0` is a direct dependency from the same official
  `midnight-ledger` HTTPS Git source and full revision
  `d9414884db9da9e9b1f6f3a7f742d79a5732f817` as every other direct Midnight
  package.
- `midnight-ledger 8.2.0-rc.1` now enables its `proving` feature. That feature
  resolves the published crates `midnight-proofs 0.7.3`,
  `midnight-circuits 6.3.0`, and `midnight-zk-stdlib 1.3.0` transitively. Oxid
  does not declare a direct `midnight-zk` Git dependency. ADR-0028's local
  provider uses `midnight-zkir` from the full official ledger Git revision;
  that separate dependency is reviewed in `midnight-local-proving.md`.
- `reqwest 0.13.4` provides bounded HTTP transport for chain-tip and proof
  requests. Defaults are disabled; JSON, streaming bodies, and Rustls are the
  selected capabilities.
- `subxt 0.44.3` matches the reviewed prototype's runtime submission surface
  and constructs the dynamic unsigned `Midnight.send_mn_transaction(bytes)`
  call. It is exact-pinned. Moving to a later Subxt line requires a node
  interoperability test, not only a compile update.
- `anyhow 1.0.104` appears only at the external `ProvingProvider` trait edge.
  Oxid's application/domain errors remain typed and do not expose arbitrary
  error chains.

## License and maintenance review

The direct Midnight packages and the two published Midnight crates carrying a
`license-file` were manually checked as Apache-2.0; `midnight-proofs` declares
MIT OR Apache-2.0. Cargo deny reports the `license-file` packages as lacking a
license expression, so that warning remains visible rather than being silently
reclassified. Subxt 0.44.3 declares `Apache-2.0 OR GPL-3.0`; its deprecated SPDX
spelling produces a Cargo deny parse warning, while the upstream README records
the same dual-license grant. Oxid consumes it under Apache-2.0.

Subxt's active macro graph contains unmaintained build-time
`proc-macro-error2 2.0.1`; no maintained Subxt release had removed it at this
review. Its optional, disabled light-client dependency also leaves
`libsecp256k1 0.7.2` and `lru 0.12.5` in `Cargo.lock`, even though neither
appears in the enabled native or WebAssembly dependency trees. The exact
RustSec exceptions and removal gates are documented in
[advisory-exceptions.md](../security/advisory-exceptions.md).

## Security and privacy boundary

- The proof request contains private witness material. Plain HTTP is accepted
  only for loopback; remote proof endpoints require HTTPS and cannot contain
  credentials, query strings, or fragments.
- Indexer messages, decoded event bytes, proof requests/responses, transaction
  bytes, counts, and total replay bytes are bounded. Connect, acknowledgement,
  idle, replay, proof, and submission operations have explicit timeouts.
- DUST derives at `m/44'/2400'/account'/2/0` through a borrowed callback inside
  unlocked development custody. No seed or derived secret is returned through
  application or headless DTOs.
- ADR-0031's optional DUST checkpoint stores only official tagged wallet state,
  completed cursors, public scope fingerprints, and update time. A fresh chain
  tip and successful live catch-up remain mandatory before balancing.
- Transactions are submitted unsigned because the Midnight runtime validates
  this call through its unsigned transaction path. Successful runtime events
  are required before public transaction and block hashes are returned.
- A remote proof server can observe witnesses. ADR-0028 therefore keeps this
  mode development-only and adds private local proving as the production
  direction.

## Target and exit strategy

All new ledger/proof/HTTP/Subxt dependencies are native-target-only in
`adapters/midnight`; the `wasm32-unknown-unknown` graph stays on Oxid-owned read
types. Both Apple iOS and Android Tier-1 library builds remain mandatory.

Replace this development composition behind `WalletTransactionPort` when
native custody and durable submission reconciliation/reorg handling land.
Local proving is reviewed
separately in `midnight-local-proving.md`. Any upgrade must re-run source,
advisory, license, native mobile, proof, and node interoperability gates.
