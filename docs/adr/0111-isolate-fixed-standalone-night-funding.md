# ADR-0111: Isolate fixed standalone NIGHT funding

- Status: Accepted
- Date: 2026-09-15
- Issue: [#538](https://github.com/MediaNoxLabs/oxid/issues/538)

## Context

Fresh development wallets need NIGHT before DUST registration and transfer
scenarios can begin. The reviewed undeployed genesis fixture already owns public,
shared, spendable test authority, but exposing the ordinary wallet protocol as a
faucet would also expose unrelated custody and transaction operations. Runtime
selection of the public fixture would make that authority reachable from normal
headless or release compositions.

## Decision

Provide a separate `oxid-standalone-faucet` binary behind the
`standalone-faucet` Cargo feature. Starting it also requires
`OXID_ENABLE_STANDALONE_FAUCET=1`. Its composition fixes the realm and routes to
the local `undeployed` stack and fixes custody to the reviewed public genesis
fixture; callers cannot select either.

The versioned line-delimited JSON protocol has only `faucet.health`,
`faucet.fund`, and `faucet.shutdown`. A grant accepts a bounded public request
identifier and an unshielded undeployed address, then uses the existing typed
prepare, authorize, prove, submit, and finalized transaction path. Every grant
is exactly 50,000 NIGHT (`50_000_000_000` atomic units).

One process handles one request at a time and retains at most 256 safe receipts.
Retries are idempotent within that process by request identifier and recipient.
Receipt state is deliberately not durable: restarting the development faucet
starts a new idempotency window, which the operator runbook states explicitly.
Responses contain public addresses and transaction receipts but no seed,
mnemonic, key, signature, serialized transaction, or arbitrary adapter error.

The real-stack acceptance uses two separately persisted headless processes with
OS-random wallet roots. It funds both, observes finalized NIGHT, explicitly
registers each wallet for DUST, and requires positive DUST within ten minutes.
It is an on-demand localhost development scenario, not a per-PR CI gate.

## Consequences

- Browser, QR, Tailnet, and PreProd adapters can later depend on the narrow
  funding protocol without acquiring wallet custody operations.
- Normal headless, desktop, mobile, and release builds cannot select the faucet
  composition through runtime input.
- The fixed amount changes only through reviewed code.
- Process-local idempotency is adequate for the first standalone slice; a
  durable request ledger is required before any multi-process or remote
  deployment.
- Shielded funding remains outside this decision.

## Alternatives rejected

- Add a funding method to the ordinary headless wallet protocol: grants a
  broader adapter unnecessary authority.
- Accept an operator seed: creates a secret-bearing service boundary despite an
  existing reviewed public fixture.
- Accept amount, network, or route parameters: weakens reproducibility and can
  cross realm boundaries.
- Run this in required CI: live proving and DUST generation exceed the fast-line
  budget and are already suitable for on-demand and before-release evidence.
