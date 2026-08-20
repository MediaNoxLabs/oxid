# ADR-0026: Stage Midnight transfer authorization before proving and submission

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3, 7–8, 12–13 and [issue #9](https://github.com/MediaNoxLabs/oxid/issues/9)
- Implementation state: Canonical unshielded NIGHT prepare/authorize/draft flow implemented; ADR-0027 now supplies the subsequent native development/headless submission stage
- Amended by: ADR-0079

## Context

The prototype's `send_unshielded` method synchronizes UTXOs, selects inputs,
builds and signs a ledger intent, balances DUST, proves, serializes, and submits
in one call. That is useful evidence, but it combines independently fallible
capabilities and leaves no application-owned review boundary before key use.
Oxid also needs deterministic transaction-flow tests before a mobile-capable
prover and node submission adapter are selected.

## Decision

Split an unshielded NIGHT transfer into retained `prepared`, `authorized`, and
`expired` states. Application DTOs contain exact atomic-unit strings, recipient,
change, input count, fee state, expiry, and an opaque draft/challenge pair. They
never contain signing payloads, signatures, serialized transactions, or private
key material.

The native Midnight adapter consumes canonical types from `midnight-ledger` at
the ADR-0015 full Git revision with default features disabled. It preserves the
prototype's behavior: native NIGHT only, same-network Bech32m recipients,
largest-value-first greedy UTXO selection, sorted ledger inputs and outputs,
change to the derived account, segment `0xCAFE`, and a one-hour ledger TTL.
Public tie-breakers and a request-derived RNG seed make otherwise equivalent
plans reproducible.

Drafts are profile-scoped, process-local, and retained only inside the outgoing
adapter. Authorization requires the exact public challenge and an explicit
human-readable confirmation. The adapter sends the ledger signing payload to
custody through the existing opaque key reference, verifies the returned BIP340
signature, and retains the canonical signed transaction. Expiry clears signing
payload and signed transaction material.

The headless methods are:

- `wallet.transaction.prepare_unshielded`;
- `wallet.transaction.authorize_unshielded`;
- `wallet.transaction.draft`.

They are `development_only`. A prepared result reports
`submissionReady: false`; successful authorization changes it to `true` while
`proofRequired` remains true. At this ADR's initial delivery the prototype-named
`wallet.transaction.send_unshielded` capability remained queued. ADR-0027
subsequently added an explicitly confirmed submit stage and a staged
compatibility alias after DUST balancing, proving, serialization, node
submission, and outcome tracking became real in native development/headless
composition. Normal mobile/production composition remains fail-closed without
native custody.

## Consequences

- Canonical transaction construction is testable without network submission or
  secret export.
- The ledger dependency's unconditional graph is paid only by the native
  Midnight adapter and must continue to pass both Tier-1 mobile builds.
- Drafts do not survive process restart; durable queues require a separate
  protected-storage decision.
- UTXO reservation, replacement, durable confirmation tracking, and
  production-grade proving remain follow-up ports. Development submission is
  governed by ADR-0027.
- A future one-shot UI action may orchestrate these stages, but may not erase
  the preview/confirmation boundary or claim submission before it occurs.
