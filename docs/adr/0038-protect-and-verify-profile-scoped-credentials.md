# ADR-0038: Protect and verify profile-scoped credentials

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3–7, 9, 12–13, 16–18 and [issue #23](https://github.com/MediaNoxLabs/oxid/issues/23)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/vc_store/`, `wallet-core/src/vc_self_verify/mod.rs`, and the Dioxus credential views
- Supersedes: ADR-0037 statements that credential verification is entirely queued
- Amends: ADR-0003, ADR-0004, ADR-0007, ADR-0009, ADR-0011, ADR-0013, ADR-0017, ADR-0018, ADR-0020, ADR-0021, ADR-0023, ADR-0024, and ADR-0029
- Implementation state: protected standalone inventory, strict Midnight phase-1 CBOR proof verification, headless lifecycle, process restoration, and Dioxus mobile inventory implemented; native platform wrapping, protocol ingress, selective disclosure, status/trust policy, and Compact passport proofs remain queued

## Context

The prototype has useful credential behavior in two places. `vc_store` retains
the original signed body, proof/opening material, normalized issuer/holder
metadata, verification metadata, and display order in Redb. `vc_self_verify`
removes the embedded CBOR `proof` member from the exact wire map, resolves the
issuer DID assertion method, and verifies Ed25519, P-256, or Jubjub signatures.
The Dioxus application displays a credential inventory and a separate digital
passport proof flow.

Copying those modules would import an aggregate database model, plaintext
credential claims, application-global state, and protocol/serialization types
into core. Returning a boolean verification result would also hide which
checks were performed. Oxid needs a profile-scoped credential foundation that
can support later OID4VC and disclosure protocols without treating its
standalone fixture or a cryptographically valid proof as a trust decision.

## Decision

Create peer `credential/domain` and `credential/application` hexagons.
`CredentialRecord` stores original signed bytes separately from bounded,
normalized metadata. Its incoming view never contains the original body,
proof, openings, or claims. Repository, inbox, and verifier are distinct
outgoing ports; receive, list, get, reverify, and delete are capability-specific
incoming use cases. Every query is scoped by the active profile at the incoming
adapter, and delete requires the exact explicit `DELETE_CREDENTIAL` intent.

Verification is a structured report with exactly these stages:

1. structural;
2. issuer;
3. proof;
4. temporal;
5. status;
6. schema;
7. trust.

Each stage is `passed`, `failed` with a stable reason code, or `not_checked`.
The overall state is `valid`, `invalid`, or `error`; it is never a boolean.
`valid` in this slice means the implemented structural, issuer, and proof
checks passed. The remaining visibly `not_checked` stages prevent that result
from being presented as freshness, schema conformance, revocation, or issuer
trust.

`adapters/vc-midnight` implements the prototype's phase-1 CBOR signing rule at
the controlled edge. It accepts one bounded definite top-level map, locates
exactly one text-keyed `proof` member, preserves the original map-header width
and all remaining encoded bytes/order, decrements only the map count, and
verifies that exact proof-stripped byte string. It rejects indefinite items,
trailing bytes, excessive nesting, duplicate proof members, duplicate required
decoded fields, invalid base64, issuer substitution, controller mismatch,
methods outside `assertionMethod`, X25519 and other unsupported curves, and
invalid signatures. Ed25519 and P-256 verification use the existing reviewed
RustCrypto stack. Jubjub remains unavailable until a reviewed current adapter
exists.

Standalone development uses one documented public fixture whose issuer is the
single standalone DID. Its disposable issuer secret is not committed; only the
public key, credential bytes, and signature are present. This fixture is
protocol evidence, not an issuer trust assertion.

`adapters/storage-credential-json` encrypts the entire strict versioned JSON
document with XChaCha20-Poly1305, a new random 192-bit nonce per write, and fixed
schema-associated data. The envelope has only magic/version, nonce, and
ciphertext. The 256-bit development wrapping key is generated from OS
randomness in a separate owner-private file. Both files reject direct symlinks,
require an owner-private parent directory, use owner-only modes and
same-directory atomic replacement, and enforce record, body, and document
bounds. Pre-existing shared directories are rejected and never silently
re-permissioned. Authentication or schema failure is an integrity error; no
partial data is returned.

The file-key boundary is explicitly development-only. Normal `compose()` wires
unavailable credential ports until Apple Keychain/Secure Enclave and Android
Keystore/BiometricPrompt wrapping and recovery semantics are reviewed.
Standalone mobile/headless composition uses `OXID_CREDENTIAL_STORE_PATH` and
`OXID_CREDENTIAL_KEY_PATH` only as a complete pair, otherwise a private sibling
of the profile store. Partial explicit configuration fails startup.

Headless v1 exposes metadata-only `credential.receive`, `list`, `get`,
`reverify`, and `delete`; prototype names `credential.request` and
`credential.verify` are aliases. Dioxus consumes the same use cases. Real
process tests prove authenticated restoration, and iOS/Android smoke flows
receive, verify, restart, and restore the inventory.

## Alternatives considered

- Reuse the prototype Redb store: rejected because its schema mixes wire data,
  searchable metadata, disclosure openings, and plaintext claims in one
  persistence technology.
- Store only normalized claims: rejected because later verification and
  presentation need the issuer-signed original bytes and codec-specific
  evidence.
- Put CBOR and DID types in credential core: rejected because serialization and
  method-specific verification belong to adapters.
- Return `true`/`false`: rejected because it cannot distinguish malformed
  input, issuer resolution failure, invalid proof, or unimplemented policy.
- Treat the standalone key file as production custody: rejected; file
  encryption without platform authorization and recovery policy is not the
  accepted production boundary.
- Include OID4VCI/OID4VP, openings, status, and passport proofs now: rejected as
  a cross-protocol bulk migration that would weaken review and testability.

## Consequences

- Credential core stays independent of CBOR, DID, crypto, storage, Dioxus, and
  protocol SDKs.
- Original signed bytes are recoverable for future protocol use but remain
  absent from ordinary UI and headless DTOs.
- The implementation improves the prototype's plaintext-at-rest and duplicate
  proof behavior while preserving its useful signing semantics.
- Adding another credential format requires a verifier/codec adapter, not a
  domain rewrite.
- OID4VCI, OID4VP/SIOP, selective disclosure/openings, Compact digital
  passport verification, credential status, schema validation, trust policy,
  and native wrapping remain explicit dependency-ordered follow-ups.

## Validation

- domain report and signed-byte separation tests;
- strict fixture, tamper, duplicate-proof, relationship, and signature tests;
- authenticated encrypted-store reopen and tamper tests;
- headless receive/list/get/reverify/confirmation-delete test;
- real binary restart test that inspects the encrypted envelope and restores a
  valid report;
- architecture, format, clippy, workspace, coverage, advisory, license,
  source, and rustdoc gates;
- iOS simulator and Android emulator receive/verify/restart/restore smoke flows.
