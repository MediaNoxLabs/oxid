# Midnight DID resolution dependency review

- Reviewed: 2026-08-12
- ADR: [ADR-0036](../adr/0036-resolve-and-retain-public-midnight-dids.md)
- Scope: native public DID Resolution Result transport and separate public record persistence

## Selection

No Midnight DID SDK is linked into Oxid. The current public behavior contract
is pinned as provenance to `midnight-did` commit
`6016f094f16228d008cc35c40eb2aa1bc1f7b01` (packages 0.5.0) and
`midnight-did-resolver` commit
`70bec499287e31736f0775ad8e210bc59799749b` (service 0.1.0).

The adapter reuses already exact-pinned workspace dependencies:

- `reqwest 0.13.4`, default features disabled, with Rustls, JSON, and streaming
  response bodies;
- `tokio 1.53.1` for the bounded native HTTP worker runtime;
- `futures 0.3.33` for executor-neutral one-shot completion and streaming;
- `serde 1.0.229` and `serde_json 1.0.151` for strict external/persistence
  envelopes.

Their license, maintenance, advisory, and target posture is already reviewed by
the Serde, Tokio/WebSocket, and Midnight standalone submission reviews. This
slice adds no dependency or cryptographic implementation. Public JWK
coordinates are syntax/length validated; signing and verification remain
behind later capability ports.

## Security and target boundary

The HTTP adapter is native-only. It disables redirects and ambient proxies,
uses Rustls, requires HTTPS outside loopback, rejects route credentials/query/
fragment, enforces a 15-second timeout, and streams at most 512 KiB before
strict parsing. A dedicated thread/runtime keeps transport work off incoming UI
executors and lets the headless harness use the same future without assuming a
Tokio caller.

`serde_json::Value` exists only in the outgoing adapter and headless projection;
identity core crates have no external dependencies. Unknown resolver extension
fields are ignored only after all modeled security invariants pass. Persisted
records use strict `deny_unknown_fields` DTOs and reconstruct domain objects on
every read.

Tier-1 iOS and Android builds remain mandatory. The HTTP implementation is
excluded on `wasm32`; standalone resolution stays available there without
network access. Any SDK adoption, resolver discovery, signature verification,
or source-contract upgrade requires a new compatibility/security review.

## Alternatives and exit strategy

Linking the TypeScript packages would add a foreign runtime and couple core to
SDK data types. Calling the prototype Rust wrapper would preserve its older key
subset. Storing raw resolver JSON would defer validation until the highest-risk
verification path. All three were rejected.

The resolver and repository remain replaceable application ports. A native
production store, verified resolver discovery policy, or future official Rust
SDK can replace the adapters without changing identity domain/use-case APIs.
