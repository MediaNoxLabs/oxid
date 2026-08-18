# The headless protocol

`oxid-headless` is a first-class incoming adapter that exposes the same
application use cases as the mobile UI over **newline-delimited JSON** on
stdin/stdout. It exists so tests, scripts, and other agents can drive a real
wallet — same domain logic, same composition rules, no rendering.

It is governed by ADR-0024 and its successors; the complete method reference
lives in the repository
[README](https://github.com/MediaNoxLabs/oxid/blob/develop/README.md).

## The envelope

Every request carries a protocol version and an id; every response echoes
both. Unknown fields are rejected (`deny_unknown_fields`), and error codes
are stable strings, not prose:

```json
{"v": 1, "id": "1", "method": "wallet.profile.list", "params": {}}
{"v": 1, "id": "1", "ok": true, "result": {"profiles": []}}
{"v": 1, "id": "2", "ok": false, "error": {"code": "invalid_params", "message": "…"}}
```

## Capability discovery is the front door

The discovery method returns, for **every** method: its status
(`ready`, `queued`, or `blocked` with a blocker URL), any `aliasFor`
compatibility naming, and a per-method `secretsExposed` flag. Tooling should
branch on this manifest rather than hard-coding assumptions — capabilities
appear as slices land, and the manifest is the truthful index of what this
build can actually do.

## Method namespaces

| Namespace | Examples |
| --- | --- |
| `wallet.profile.*` | create, list, select, restore active profile |
| `wallet.security.*` | initialize, unlock, lock protected custody |
| `wallet.account.*` / `wallet.transaction.*` | accounts, sync, prepare/authorize/submit/cancel/reconcile |
| `wallet.dust.sync.*` / `wallet.shielded.sync.*` | resumable background sync lifecycles |
| `system.diagnostics.*` | bounded payload-free runtime-health snapshot and confirmed local reset |
| `identity.*` | DID inventory, resolution, lifecycle, self-issued authentication |
| `credential.*` | inventory, verification reports, disclosure planning |
| `presentation.*` | OpenID4VP request preview, consent, refusal |

## Secret hygiene, by contract

The protocol never accepts or returns seeds, mnemonics, private keys,
passphrases, credential claim values, proofs, or tokens. Requests that try
to smuggle secret-bearing parameters are rejected **without echoing them**,
and tests assert this. Sensitive operations require explicit, literal
confirmation strings (for example `ACCEPT_CREDENTIAL_ISSUANCE`) so a driver
cannot stumble into consent.

Long-running operations (sync, submission) follow an asynchronous
start/status/cancel shape: cancellation is acknowledged at safe boundaries
only, and a possibly-broadcast transaction is never made blindly retryable —
the same fail-closed semantics the UI gets, because both sit on the same
application ports.

## Secret-safe runtime health

`system.diagnostics.snapshot` returns only closed event codes, severities,
counts, monotonic process-local sequence numbers, capacity, and eviction
totals. It explicitly reports `persistence: process_local`, `telemetry: off`,
and `payloadsRetained: false`. It never contains the rejected request, request
id, endpoint, profile, credential, transaction material, external response, or
free-form error text.

`system.diagnostics.clear` requires `confirmed: true` and the exact
`CLEAR_LOCAL_DIAGNOSTICS` intent. The ring is not part of profile state,
credential storage, or complete-wallet backup and disappears on process exit.
Diagnostics are operator visibility only; wallet readiness, retry, and
authorization continue to use their typed application state.

## Protected shielded transfer flow

Protected spending is deliberately staged. First derive and synchronize the
public account, then run `wallet.shielded.sync.start` and poll
`wallet.shielded.sync.status` until it reports `synced` with equal current and
target cursors. Only then call `wallet.transaction.prepare_shielded` with a
canonical shielded recipient, exactly 64 lowercase hexadecimal token-type
characters, and a decimal-string atomic amount. Cached or incomplete shielded
state fails closed.

The response contains an exact public preview with `recipientKind`, amount,
change, input count, fee state, draft handle, and authorization challenge. It
never contains notes, nullifiers, Merkle paths, output nonces, ciphertexts,
proof preimages, keys, or transaction bytes. A profile may have only one active
shielded draft, preventing concurrent plans from selecting the same private
note.

Confirm that exact preview through `wallet.transaction.authorize_shielded`,
then call `wallet.transaction.submit_shielded` (or the prototype-compatible
`wallet.transaction.send_shielded` alias). Submission status, history,
cancellation, and reconciliation use the shared `wallet.transaction.*`
methods. Zero-configuration standalone composition exercises a real official
Zswap offer and simulated completion; its identifiers are harness evidence,
not Midnight inclusion claims.
