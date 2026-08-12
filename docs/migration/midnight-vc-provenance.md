# Midnight credential inventory and verification provenance

## Immutable sources

This slice was reconciled on 2026-08-13 against:

| Source | Commit | Surface used |
| --- | --- | --- |
| `midnightntwrk/midnight-ledger`, `feat/mobile-prototype` | `074b1a4bccbfee1740ee188374b606a022ecef42` | `mobile-bench/wallet-core/src/vc_store/{api,in_memory,mod,tables,types}.rs`, `wallet-core/src/vc_self_verify/mod.rs`, and `dioxus-wallet/src/vc_views/` |
| `midnightntwrk/midnight-did`, `main` | `6016f094f16228d008cc35c40eb2aa1bc1f7b01e` | DID/JWK verification-method and `assertionMethod` vocabulary |
| `midnightntwrk/midnight-did-resolver`, `main` | `70bec499287e31736f0775ad8e210bc59799749b` | resolved public DID document contract |
| `midnightntwrk/midnight-verifiable-credentials`, `develop` | `39b1354212620b396e914b29603e6a38f2656546` | separation of schema, claims, disclosure, capabilities, artifacts/codecs, and untrusted display metadata |

No source file is copied. Oxid reimplements the observed behavior with owned
domain/application types and controlled-edge adapters. There is no Cargo or npm
dependency on the three identity repositories.

## Retained behavior

- preserve original issuer-signed credential bytes;
- maintain separate normalized issuer, holder/subject, format, issued-time,
  display, and verification metadata;
- scope inventory operations to the selected wallet profile;
- reconstruct the exact proof-stripped phase-1 CBOR map bytes;
- resolve the issuer DID and require the referenced assertion method;
- verify Ed25519 and P-256 public JWK signatures;
- distinguish valid, invalid, and operational-error outcomes;
- list, inspect, reverify, delete, and restore credentials through headless and
  Dioxus incoming adapters.

## Deliberate 110% hardening

| Prototype behavior/risk | Oxid decision |
| --- | --- |
| Redb records contain plaintext body, proof, and openings | Encrypt the complete strict document with XChaCha20-Poly1305 and keep original bytes private to the repository/use-case boundary. |
| Verification collapses checks into a three-state top-level result | Preserve valid/invalid/error and add seven explicit stage states/reason codes. |
| Duplicate top-level proof handling is ambiguous | Require exactly one proof member before generic CBOR decoding. |
| Codec scan and semantic decode can diverge | Bound both, reject indefinite/trailing/deep data, and require a complete exact top-level scan. |
| Resolver/key mismatch can be obscured | Require subject equality, controller equality, exact method identity, and assertion authorization. |
| UI can drift toward displaying arbitrary claim values | Expose only bounded normalized metadata and stable verification codes; never project body, signature, openings, or claims. |
| Store deletion is a direct CRUD operation | Require explicit confirmation and exact deletion intent at the application boundary. |

The standalone fixture contains only public conformance material. Its issuer
secret was generated outside the repository and discarded. The revised DID
fixture version is `standalone-fixture-v2`; its Ed25519 method is authorized for
both authentication and assertion so the signed credential exercises the same
resolver boundary as later protocol ingress.

## Format contract implemented

The supported format is a definite CBOR map containing at least:

- `type`: an array containing `VerifiableCredential` and a bounded display type;
- `issuer`: a `did:midnight` string;
- optional integer `issuanceDate` in Unix milliseconds;
- optional `credentialSubject.id` string;
- `proof.verificationMethod`: an issuer-controlled method ID or fragment;
- `proof.signature`: standard-base64 signature bytes.

The signature input is the original outer map with the complete encoded proof
key/value pair removed and the map count decremented using the original header
width. No other item is reordered or re-encoded.

## Explicit exclusions and follow-ups

- The prototype `midnight_compact_vc` digital-passport proof uses a JavaScript
  bridge and Compact verifier artifacts. It is not represented as phase-1 CBOR
  validity and needs its own native proving/verification adapter.
- Selective-disclosure openings and predicate proofs need an owned disclosure
  domain and consent flow before storage migration.
- OID4VCI issuance, OID4VP/SIOP presentation, deep links, QR scanning, and
  browser/native bridge transport remain M4 protocol adapters.
- Status/revocation, temporal policy, schema validation, and issuer trust remain
  visible `not_checked` stages rather than fabricated success.
- Jubjub verification remains queued pending a reviewed current implementation.
- The development key file is not backup, recovery, biometric authorization,
  or production platform custody.

## Privacy and threat review

| Risk | Boundary |
| --- | --- |
| Claim disclosure through protocol/UI/logs | Incoming views include metadata and stage codes only; original bytes remain repository-private; errors are sanitized. |
| Cross-profile access | Incoming adapters derive profile scope from the active profile and accept no profile parameter. |
| Ciphertext substitution/tampering | XChaCha20-Poly1305 authentication with fixed schema-associated data fails closed. |
| Nonce reuse | A fresh 192-bit OS-random nonce is generated for every whole-document write. |
| Weak key custody claim | Separate 256-bit owner-private key is labeled development-only; normal composition is unavailable. |
| Filesystem redirection | Direct symlinks and non-files are rejected; owner-private directories/files and same-directory atomic writes are used. Existing shared directories fail closed instead of being re-permissioned. |
| CBOR parser exhaustion | 1 MiB credential bound, depth 32, definite lengths, checked offsets, exact end-of-input. |
| Signature confusion | Standard-base64 proof signature, base64url JWK coordinates, curve-specific key construction, assertion relationship, and controller checks. |
| Cryptographic validity mistaken for trust | Temporal/status/schema/trust are explicit `not_checked` stages. |
