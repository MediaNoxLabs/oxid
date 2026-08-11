# ADR-0024: Use a versioned NDJSON headless wallet incoming adapter

- Status: Accepted
- Date: 2026-08-11
- Source: Prototype parity epic and [issue #4](https://github.com/MediaNoxLabs/oxid/issues/4)
- Implementation state: Version 1 transport, capability discovery, complete profile lifecycle, and shutdown implemented

## Context

The reviewed prototype includes a `headless-wallet` executable that drives many
of the same flows as its Dioxus application through line-delimited JSON. That
separation is useful for deterministic integration tests, automation, and
development on hosts without a mobile simulator. The prototype implementation
also couples directly to its wallet facade, requires seed material at startup,
returns raw external errors, and exposes `controllerSkHex` during bootstrap.
Those behaviors conflict with Oxid's application boundaries and opaque-key
policy.

Oxid also needs a stable place to exercise every migrated flow without making
the Dioxus adapter or a native host the test API. The existing composition root
returned a Dioxus-owned service object, so a second incoming adapter would have
introduced an inward dependency on the UI.

## Decision

Provide `oxid-headless` as a standalone incoming adapter. It and the Dioxus app
consume the same UI-neutral `ApplicationServices` assembled by
`oxid-composition`; composition does not depend on either incoming adapter.

Use one JSON request and one JSON response per line over stdin/stdout. Structured
requests carry:

- `protocol`, initially `oxid.headless.v1`;
- an optional bounded string `id` used only for response correlation;
- a namespaced `method`;
- an object-valued `params` field.

Responses repeat the protocol and valid correlation ID, carry `ok`, and contain
exactly one of `result` or a stable `{code, message}` error. Malformed input and
unknown methods do not terminate the stream. `system.capabilities` reports both
ready and queued methods so automation cannot mistake a planned flow for an
implemented one. `system.quit` ends cleanly; literal `quit` and `exit` remain
small compatibility aliases for prototype scripts.

Stdout is reserved for protocol data. Diagnostics belong on stderr. Request
validation occurs before application dispatch, and adapter error mapping must
not reproduce raw dependency errors. Seeds, private keys, credential contents,
recovery material, and other secrets are never protocol results. In particular,
Oxid will not reproduce the prototype's `controllerSkHex` response. Future key
operations use opaque references under ADR-0011.

The initial loop is synchronous because the only implemented use case is
synchronous. An async runtime may be added when a network, proving, or protocol
adapter needs concurrency; it is not part of the wire contract.

## Consequences

- Profile create, list, select, and active-restore flows can be exercised without
  Dioxus, Xcode, or a network.
- Each later parity slice gains a deterministic integration surface and must
  update capability discovery when its method becomes ready.
- Protocol evolution requires either backward-compatible additions or a new
  protocol identifier.
- The adapter may depend on serialization libraries, but core domain,
  application, and port crates remain independent of them.
- The v1 process is local development/test tooling, not a privileged remote RPC
  server or a production secret export interface.
