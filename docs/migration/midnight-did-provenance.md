# Midnight DID inventory and resolution provenance

## Immutable sources

This slice was reconciled on 2026-08-12 against:

| Source | Commit | Surface used |
| --- | --- | --- |
| `midnightntwrk/midnight-ledger`, `feat/mobile-prototype` | `074b1a4bccbfee1740ee188374b606a022ecef42` | `mobile-bench/wallet-core/src/did/`, DID service/inventory concepts, Dioxus DIDs page, headless verbs |
| `midnightntwrk/midnight-did`, `main` | `6016f094f16228d008cc35c40eb2aa1bc1f7b01` | package version 0.5.0, DID syntax, document/JWK/relationship/service schemas |
| `midnightntwrk/midnight-did-resolver`, `main` | `70bec499287e31736f0775ad8e210bc59799749b` | `POST /resolve` request and W3C DID Resolution Result response contract |

No source file is copied. Oxid reimplements the required invariants in its own
domain and adapts the public HTTP contract at the edge. There is no Cargo or npm
dependency on either DID repository in this slice.

## Retained behavior

- a mobile DID inventory attached to the selected wallet profile;
- resolve and cache a `did:midnight` public document;
- inspect public method curve/count, services, version, source, and deactivation
  status;
- retrieve/list/forget records through the headless wallet;
- restore the inventory after a real process restart.
- create an undeployed DID backed by protected Ed25519 authentication and P-256
  assertion keys;
- add/remove aliases, add/rotate/remove verification methods, add/remove
  relationships, and add/update/remove services;
- authorize every visible mutation, sign bounded payloads, and deactivate a DID
  through explicit human-readable confirmation;
- exercise the complete lifecycle through headless and mobile incoming
  adapters without returning a private key or custody reference.

The standalone implementation preserves the operation semantics but does not
claim a ledger deployment. Live create/update/deactivate still require the
official Compact authorization, proving, submission, and finality boundaries.
The prototype's Schnorr-Jubjub assertion path is not copied because current
development custody cannot retain that key without a reviewed algorithm
adapter. Credential verification, OID4VCI/OID4VP, identity login, vault flows,
camera/share/deep links, and recovery remain separate slices.

## Current contract mapping

Oxid accepts the official networks `undeployed`, `devnet`, `testnet`,
`mainnet`, `preview`, `preprod`, and `offchain`. Ledger identifiers are 64 hex
characters; offchain identifiers are lowercase and may carry one bounded
unpadded base64url state component.

Resolved documents require the DID Core and JWK contexts first, a controller
equal to the subject when present, and public `JsonWebKey` methods. Supported
profiles are OKP Ed25519/X25519/BLS12381G1/BLS12381G2 and EC
Jubjub/P-256/secp256k1 with the official coordinate sizes. X25519 is permitted
only for key agreement; signing relationships reject it. Relations must point
to unique methods in the same document. Service string, object, and array
endpoints remain public bounded data.

## Threat and privacy review

| Risk | Boundary |
| --- | --- |
| Resolver SSRF or accidental credential disclosure | Route is operator-supplied only; non-loopback HTTP, credentials, query, fragment, redirect, and ambient proxy use are rejected. |
| Memory/CPU exhaustion | 15-second request timeout, 512 KiB response limit, depth 16, and bounded domain collections/text. |
| Private key injection | Any `publicKeyJwk.d` is rejected; the domain has no private-key field. |
| Document substitution | Returned subject must equal the requested DID; controllers and method references are subject-bound. |
| Poisoned verification graph | Curve/coordinate profiles, duplicates, dangling relations, and X25519 relationship use are validated before persistence. |
| Cross-profile access | Incoming methods derive profile scope from the active wallet profile and accept no profile parameter. |
| Persistence tampering | Strict version/unknown-field validation plus full domain reconstruction; bounded symlink-rejecting owner-only atomic store. |
| Secret leakage | Only public documents/metadata are persisted or projected; routes and remote response bodies are never returned or logged. |
| Prototype controller-key exposure | `controllerSkHex` is excluded. DID lifecycle receives only opaque protected-key handles, and DID protocol responses never contain them. |
| Standalone state mistaken for ledger state | Only `undeployed` creation is accepted, sources and version IDs say standalone, and live mutation remains unavailable. |
| Restart creates false ownership | Public records persist separately; mutation/signing fails `NotManaged` after process-local custody resets. |

## Standalone fixture

The only successful deterministic DID is:

```text
did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

It contains public Ed25519 authentication and X25519 key-agreement methods plus
one invalid-domain example service. It is a conformance fixture, not a deployed
identity or production trust assertion.
