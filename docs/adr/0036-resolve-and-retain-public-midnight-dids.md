# ADR-0036: Resolve and retain public Midnight DIDs through the identity hexagon

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3–7, 9–13, 16–18 and [issue #21](https://github.com/MediaNoxLabs/oxid/issues/21)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/did/`, DID services, DIDs page, and headless wallet
- Standards source: `midnight-did` commit `6016f094f16228d008cc35c40eb2aa1bc1f7b01` and `midnight-did-resolver` commit `70bec499287e31736f0775ad8e210bc59799749b`
- Amends: ADR-0007, ADR-0008, ADR-0013, ADR-0021, ADR-0023, ADR-0024, and ADR-0029
- Implementation state: DID inventory, bounded live/standalone resolution, public persistence, headless flow, and mobile presentation implemented; lifecycle mutation and native production storage remain queued

## Context

The prototype exposes DID create, resolve, update, deactivate, and inventory
behavior, but its Rust types predate the current Midnight DID 0.5.0 public-key
profiles and are coupled to the prototype wallet service. Credential
verification, OID4VP, OID4VCI, and later DID mutation all need a trustworthy
public document boundary first.

A resolver response is remote, attacker-controlled structured data. Treating it
as a generic JSON blob would allow private JWK material, inconsistent
controllers, dangling relationships, unsupported curves, oversized documents,
or cross-subject records to enter persistence and later verification paths.
Conversely, coupling the application to the TypeScript DID SDK or resolver
service would invert the hexagonal dependency direction.

## Decision

Oxid owns dependency-free `identity/domain` and `identity/application` crates.
The domain implements the current `did:midnight` syntax, including bounded
long-form offchain identifiers, and validates the Midnight DID 0.5.0 document
profile:

- required DID Core and JWK contexts;
- subject-only controllers;
- public `JsonWebKey` methods for Ed25519, X25519, Jubjub, P-256,
  secp256k1, BLS12381G1, and BLS12381G2;
- canonical fixed-size base64url coordinates and no private `d` member;
- unique referenced methods and curve-compatible relationships;
- bounded aliases, services, endpoint objects, and document collections.

The application owns resolver and record-repository ports plus resolve, list,
get, and forget use cases. Every record is scoped to an Oxid profile identifier.
Incoming adapters obtain that scope from the active profile; callers cannot
choose another profile in DID method parameters.

`adapters/did-midnight` provides two implementations. The deterministic
standalone adapter resolves exactly one documented public fixture and returns
not-found for every other syntactically valid DID. The native HTTP adapter uses
the official `POST /resolve` result shape. Its base URL is accepted only through
`OXID_MIDNIGHT_DID_RESOLVER_URL`, forbids credentials/query/fragment and
redirects, ignores ambient proxies, requires HTTPS outside loopback, and caps
time, bytes, JSON depth, and every modeled collection. The resolver uses an
exact-pinned WebPKI public root bundle so Nix packages do not depend on ambient
CA-store state. Remote bodies and routes never enter user-facing errors or logs.

`adapters/storage-identity-json` stores only validated public documents and
resolution metadata in a separate versioned file. It is capped at 128 records
and 2 MiB, rejects unknown fields and symlinks, uses an owner-only directory and
atomic owner-only replacement, and revalidates every domain invariant on read.
The default standalone path is `private/did-records.json` beside public profile
storage; `OXID_DID_STORE_PATH` can isolate an automation run. It never stores a
private key, credential, claim, endpoint configuration, token, or resolution
request.

Production `compose()` retains unavailable identity ports. The explicit
standalone mobile/headless composition selects the public store and fixture,
or the configured HTTP resolver for native headless runs. Headless v1 exposes
`did.resolve`, `did.list`, `did.get`, and `did.forget`; create, update, and
deactivate stay queued. Dioxus uses the same use cases and performs live
resolution on an asynchronous worker path.

## Consequences

- Later credential and presentation verification can consume an Oxid-owned,
  validated public document rather than raw resolver JSON.
- DID inventory survives restart independently of public profile labels and
  private wallet/credential storage.
- Deterministic tests cannot accidentally claim that arbitrary DIDs exist.
- Explicit live resolution is useful without making endpoint discovery,
  mutation authorization, native custody, or credential verification appear
  production-ready.
- The Rust model intentionally mirrors the current public contract rather than
  copying the prototype's older subset; source upgrades require conformance
  review and new fixtures.
