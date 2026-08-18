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
