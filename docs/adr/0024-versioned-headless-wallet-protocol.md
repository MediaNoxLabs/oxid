# ADR-0024: Use a versioned NDJSON headless wallet incoming adapter

- Status: Accepted
- Date: 2026-08-11
- Source: Prototype parity epic and [issue #4](https://github.com/MediaNoxLabs/oxid/issues/4)
- Implementation state: Version 1 transport, profile lifecycle,
  development-only protected-key and Midnight account-derivation flows, live
  account/DUST sync, canonical transfer authorization, and shutdown implemented
- Amended by: ADR-0032 adds non-blocking adapter-owned DUST sessions

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

ADR-0017 permits the local process to exercise an ephemeral development key
adapter. Its capabilities must report `development_only`; initialization and
unlock accept no passphrase, seed, recovery phrase, or private-key parameter.
Signing accepts a bounded public payload plus an explicit human-readable
confirmation and returns only an opaque reference's public metadata or
signature. Recovery/import/export remain outside v1.

`wallet.account.derive` accepts only bounded public account/address indices. It
returns the selected network, public receive address, account identifier, and
opaque transaction-key reference. It never accepts a seed or path string and
never returns private derivation data. Capability discovery labels the method
`development_only` until native custody is composed.

Canonical transfer staging uses strict
`wallet.transaction.prepare_unshielded`,
`wallet.transaction.authorize_unshielded`, and `wallet.transaction.draft`
methods. Responses contain a public preview and opaque draft/challenge only.
They do not expose signing or serialized transaction bytes, and they must keep
proof/submission readiness false until those separate adapters are composed.

The request loop stays synchronous and deterministic. Long DUST network/fold
work is started inside the outgoing adapter and observed through separate v1
status/start/cancel methods, so the loop never owns a ledger task or exposes a
transport stream. Other long-running capabilities require their own explicit
session or async protocol decision; concurrency is not implicit in the wire
contract.

## Consequences

- Profile create, list, select, and active-restore flows can be exercised without
  Dioxus, Xcode, or a network.
- Initialize/lock/unlock and opaque generate/list/sign/delete sequencing can be
  tested with ephemeral Ed25519/P-256 keys, and the protected Midnight account
  flow can derive BIP340 keys and bind their public address, without weakening
  production composition.
- Each later parity slice gains a deterministic integration surface and must
  update capability discovery when its method becomes ready.
- Protocol evolution requires either backward-compatible additions or a new
  protocol identifier.
- The adapter may depend on serialization libraries, but core domain,
  application, and port crates remain independent of them.
- The v1 process is local development/test tooling, not a privileged remote RPC
  server or a production secret export interface.
