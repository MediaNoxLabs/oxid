# ADR-0027: Complete Midnight submission through bounded standalone adapters

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3, 7–8, 12–13 and [issue #11](https://github.com/MediaNoxLabs/oxid/issues/11)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`
- Implementation state: Implemented for native development/headless composition; production mobile composition remains fail-closed

## Context

ADR-0026 stops after a canonical unshielded NIGHT intent is reviewed and
authorized. The prototype then synchronizes wallet DUST, balances the ledger
fee, proves every DUST spend, seals and tagged-serializes the transaction, sends
`Midnight.send_mn_transaction` as an unsigned extrinsic, and waits for a
successful best or finalized block. Those are necessary wallet behaviors, but
the prototype performs them in one broad wallet object and permits fallbacks
that are not suitable for a public, standalone repository.

DUST is derived from the same wallet root at
`m/44'/2400'/<account>'/2/0`. Replay and spending need the derived secret in
memory; they cannot be expressed as a normal signing operation. Passing that
child secret through an incoming command or returning it from a general key
port would violate the custody boundary.

The prototype's in-process prover was built with ledger-relative paths and, in
the reviewed branch, a mutable proof-system fork. The standalone stack already
offers the protocol-compatible Midnight proof-server `/prove` endpoint. Using
that explicit development edge completes a real transaction without importing
the mutable fork, but it discloses the proof witness to that configured process
and therefore cannot be the production privacy default.

## Decision

Add a focused, internal derived-secret-use port. It accepts only an Oxid-owned,
validated HD path and a callback. The custody adapter derives a temporary child
inside an unlocked session, lends it to that callback, zeroizes the temporary
buffer afterwards, and returns no secret. There is no incoming use case for
this port. The Midnight adapter uses it only to construct a `DustSecretKey` for
the duration of one submission worker. The development implementation remains
process-local; native production custody is still required by ADR-0017.

Submission remains a stage after explicit prepare and authorize operations.
The application exposes asynchronous submit/send use cases over the opaque
draft identifier. Chain-specific signed, balanced, proven, and serialized
transactions remain retained inside the Midnight adapter. A profile-scoped
state transition prevents concurrent submission. Repeating a completed request
returns the same public outcome; a failure known to precede or reject submission
restores the authorized draft for an explicit retry. A timeout or transport
failure after node submission, or unexpected termination of the submission
worker, is classified as an unknown outcome and remains `Submitting` until a
later reconciliation capability resolves it. Cancelling
the asynchronous caller does not cancel
the already-started worker or prematurely make the draft retryable: the draft
remains `Submitting` until that worker records either its public outcome or a
retryable failure. This prevents a second send racing an in-flight external
side effect. Expiry clears retained transaction material.

The live standalone completion adapter performs the following bounded sequence:

1. read one current chain tip, reject malformed live ledger parameters, and
   retain its timestamp;
2. subscribe to the pinned GraphQL v4 `dustLedgerEvents` query with the tip's
   DUST parameters and enforce
   connection, acknowledgement, idle, overall, event-count, message-size, and
   aggregate-byte limits;
3. decode tagged ledger events, replay them with the borrowed DUST key, and use
   the greater of tip time and DUST sync time;
4. iteratively add a single DUST intent at segment `0xFEED` until canonical
   fees converge, using fresh operating-system entropy and skipping fully
   decayed outputs;
5. send each DUST proof preimage to the configured Midnight proof server,
   seal with fresh operating-system entropy, and tagged-serialize internally;
6. submit the runtime call as an unsigned extrinsic, accept best or finalized
   inclusion only after successful events, and return public transaction and
   block identifiers.

Indexer HTTP, indexer WebSocket, node WebSocket, and proof-server endpoints are
validated at composition. Credentials, query strings, and fragments are
forbidden. Plain HTTP proving is accepted only on loopback; any non-loopback
proof server must use HTTPS. Plain `ws` node/indexer routes remain an explicit
standalone-development option because local and private development stacks may
not terminate TLS.

Network and proving work runs on a dedicated adapter worker rather than the
incoming/UI thread. The headless protocol exposes the same staged flow and
labels deterministic conformance outcomes as simulated. It never emits DUST
keys, proof inputs, signatures, or transaction bytes.

Local in-process proving is deferred to
[issue #12](https://github.com/MediaNoxLabs/oxid/issues/12). No production or
privacy-preserving proving claim is made until that issue selects a compatible
immutable proof source, measures mobile resources, and passes iOS/Android
interoperability tests.

## Consequences

- Oxid obtains a truthful end-to-end standalone NIGHT send path without
  weakening the ADR-0026 review boundary.
- The core remains independent of Midnight, networking, Substrate, proof, and
  storage SDK types.
- Arbitrary Midnight witness material may exist temporarily in a reviewed
  chain adapter callback, but it is never a normal port result or incoming DTO.
- A remote proof server can observe private proof witnesses. Endpoint policy
  reduces accidental disclosure but does not make remote proving private.
- Live DUST snapshots remain process-local and replay from the bounded query;
  durable checkpointing and reorg recovery require a later storage decision.
- An ambiguous node outcome deliberately blocks blind retry; durable transaction
  reconciliation is required before this adapter becomes production-facing.
- Production mobile composition remains fail-closed pending native custody and
  local proving.
