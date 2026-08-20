# Midnight credential inventory and verification provenance

## Immutable sources

This slice was reconciled on 2026-08-13 against:

| Source | Commit | Surface used |
| --- | --- | --- |
| `midnightntwrk/midnight-ledger`, `feat/mobile-prototype` | `074b1a4bccbfee1740ee188374b606a022ecef42` | `mobile-bench/wallet-core/src/vc_store/{api,in_memory,mod,tables,types}.rs`, `wallet-core/src/vc_self_verify/mod.rs`, and `dioxus-wallet/src/vc_views/` |
| `midnightntwrk/midnight-did`, `main` | `6016f094f16228d008cc35c40eb2aa1bc1f7b01e` | DID/JWK verification-method and `assertionMethod` vocabulary |
| `midnightntwrk/midnight-did-resolver`, `main` | `70bec499287e31736f0775ad8e210bc59799749b` | resolved public DID document contract |
| `midnightntwrk/midnight-verifiable-credentials`, `develop` | `39b1354212620b396e914b29603e6a38f2656546` | separation of schema, claims, disclosure, capabilities, artifacts/codecs, and untrusted display metadata |
| OpenID Foundation | OpenID4VCI 1.0 Final, 2025-09-16 | normative offer, metadata, nonce, proof, request, response, security, and privacy behavior |
| OpenID Foundation | SIOPv2 draft 13 and OpenID4VP 1.0 Final | self-issued DID authentication boundary and explicit separation from credential presentation |

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
- preview and explicitly accept a pre-authorized credential offer, construct a
  DID-bound proof, and import the verified issued credential.
- preview and explicitly accept a standalone self-issued login request, prove
  control of a managed DID, and let the verifier independently validate it
  without disclosing a credential.
- retain all five Digital Passport value/opening pairs in the protected
  credential record, validate them against issuer-signed commitments, and
  expose local first/last reveal plus age-threshold planning.
- preserve the prototype's exact `midnight_compact_vc` body, detached issuance
  proof, and private opening envelope as three bounded values;
- bind standalone issuance to the selected profile's managed Jubjub assertion
  method and regenerate the canonical body plus detached issuer proof;
- reconstruct the upstream Compact body/payload roots and verify its Jubjub
  Schnorr issuance proof without exposing or mislabeling it as a presentation;
- migrate encrypted credential-store schemas 1 and 2 into schema 3 and prove
  exact Compact re-verification across a headless process restart.

## Deliberate 110% hardening

| Prototype behavior/risk | Oxid decision |
| --- | --- |
| Redb records contain plaintext body, proof, and openings | Encrypt the complete strict document with XChaCha20-Poly1305; keep original signed bytes and bounded opaque format-private material private to the repository/use-case boundary. |
| Verification collapses checks into a three-state top-level result | Preserve valid/invalid/error and add seven explicit stage states/reason codes. |
| Duplicate top-level proof handling is ambiguous | Require exactly one proof member before generic CBOR decoding. |
| Codec scan and semantic decode can diverge | Bound both, reject indefinite/trailing/deep data, and require a complete exact top-level scan. |
| Resolver/key mismatch can be obscured | Require subject equality, controller equality, exact method identity, and assertion authorization. |
| UI can drift toward displaying arbitrary claim values | Expose only bounded normalized metadata and stable verification codes; never project body, signature, openings, or claims. |
| Store deletion is a direct CRUD operation | Require explicit confirmation and exact deletion intent at the application boundary. |
| Prototype OID4VCI module follows pre-final request/response shapes | Reconcile against 1.0 Final: split issuer/OAuth metadata, use a separate nonce endpoint, `proofs` request object, and `credentials` response array. |
| Protocol flow can expose grant codes, tokens, nonce, proof, or issuer bodies to UI state | Keep all ephemeral protocol material in the outgoing adapter and expose only bounded preview/state/credential identifiers. |
| Issuance can proceed as soon as an offer is parsed | Require an untrusted-offer preview plus exact explicit consent and an active profile-scoped DID authentication method. |
| Successful HTTP-style response can be stored directly | Require ADR-0038 verification outcome `valid` before protected persistence. |
| Prototype names self-issued `id_token` login as OID4VP | Implement it as a pinned SIOPv2 draft-13 authentication capability; reserve OpenID4VP Final for `vp_token`/DCQL credential presentation. |
| Login proof can leak through UI state or be replayed | Keep nonce, state, signing input, and ID Token adapter-private; consume the verifier session before independent signature/claim verification. |
| Private values can be attached to an unrelated signed credential | Parse exact bounded private parts and recompute each official Midnight `persistentCommit` plus the signed domain-separated claim root before candidates or local values are available. |
| Disclosure planning can be mistaken for verifier proof | Headless returns claim-free candidates/plans only; Dioxus labels reveal as local and reports `presentationGenerated: false`; no OpenID4VP or Compact proof is constructed. |
| Compact body, detached issuance proof, and private openings can be collapsed or confused | Keep three separately bounded, debug-redacted record fields; route exact `MCV1` only; reject proof data for CBOR; never reuse an issuance signature as a presentation proof. |
| A self-contained Compact proof can be mistaken for issuer trust | Keep proof-only conformance distinct from active-wallet policy; standalone composition additionally requires exact issuer DID method/key binding, current-time validity, and the pinned trust anchor while status stays `not_checked`. |
| Prototype presentation derives a holder scalar from public claim data | Do not copy that shortcut into normal/mobile composition. ADR-0046 keeps exact Jubjub signing behind profile-scoped opaque development custody; ADR-0047 binds standalone issuance to the selected managed DID method; ADR-0048 reauthorizes the exact statement with the current managed protected key and discards the separately verified custody signature. |

The standalone fixture contains only public conformance material. Its issuer
secret was generated outside the repository and discarded for the phase-1 CBOR
path. The revised DID fixture version is `standalone-fixture-v2`; its Ed25519
method is authorized for both authentication and assertion so the signed
credential exercises the same resolver boundary as later protocol ingress. The
separate Compact issuer scalar is intentionally public reference-conformance
data and is selected only by standalone composition.

## Format contracts implemented

### Midnight phase-1 CBOR

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

### Midnight Compact VC

The second supported format is the prototype's exact `midnight_compact_vc`
bundle. Its original credential value is a canonical 18-chunk `MCV1` body; its
detached issuer proof is a canonical 9-chunk `MCV1` value. Both require exact
end-of-input and bounded little-endian chunks. The adapter reconstructs the
Digital Passport package/schema, claim root, credential body root, issuance
payload root, challenge, Jubjub points/scalar, and Schnorr equation. Identity
proof points, malformed/trailing containers, absent proof, package/schema
mismatch, body tampering, and proof tampering fail closed.

The public standalone vectors are exact reference-package outputs:

- body bytes SHA-256:
  `4d47be8d1aeeff5e06d9ba1b3ade3bab8e907f0939607cf46e100a9500ad4bcf`;
- detached proof bytes SHA-256:
  `fbf2c7e434c70d6f98fa7fae6cd146971db1fda6db96ff2ddea64fe9453e2e02`;
- reconstructed body root:
  `b42f1115042cefecbd5380a0a630c0ef5f18bb13e7615cb1de9d36256f100432`.

The proof-only verifier proves internal issuer-signature consistency and leaves
issuer, temporal-current-time, status, and trust `not_checked`; immutable
historical vectors use that mode. ADR-0073 gives active standalone composition
an explicit policy. It resolves the exact issuer DID method, requires the DID
controller and `assertionMethod` relationship, matches the method's canonical
Jubjub coordinates to the proof key, enforces issuance/proof/expiry chronology
against the current clock, and requires the pinned standalone trust anchor.
Status remains `not_checked` because the reviewed credential carries no status
reference and Oxid has no reviewed revocation resolver for this format.

Standalone issuance parses this exact static body, replaces only its holder DID
and method with the selected managed Jubjub assertion reference, canonically
re-encodes all 18 chunks, and rebuilds the nine-chunk detached proof over the
new body root. The public JWK is validated at issuance but is not embedded in
the credential because the reference family's `ExplicitHolderBinding` contains
only the DID/method reference. ADR-0048 re-resolves that exact method and
authorizes its current protected public key. Rotation preserves authority only
when the exact method identifier remains managed and assertion-authorized;
removal, deactivation, locked custody, or public-only restoration fail closed.

## Digital Passport protected-material contract

The adapter accepts Digital Passport interpretation only for a verified
phase-1 credential with the exact reviewed typed commitment map or a verified
Compact credential with the equivalent exact package/schema and five
commitments plus claim root. The private CBOR envelope contains exactly the
corresponding five padded values and five 32-byte openings. Input is bounded to
256 KiB, depth limited, duplicate and unknown fields are rejected, and trailing
bytes are forbidden.

Rust recomputation matches the immutable reference package's cross-language
vectors for first name, last name, date of birth, document number, issuing
state, and root. The adapter uses `midnight-base-crypto` and
`midnight-transient-crypto` from the same full-revision-pinned official ledger
source already selected by ADR-0015; no unpublished path or floating branch is
introduced.

The domain/application vocabulary remains schema neutral: selective-disclosure
or predicate-only privacy, bounded paths/labels, candidate inventory, and a
local plan. The standalone fixture has all five candidates. Dioxus mirrors the
prototype's visible first/last reveal and date-of-birth age-threshold control;
document number and issuing state remain available to future reviewed consent
surfaces without widening the current UI. Headless never reveals a value.

## Explicit exclusions and follow-ups

- Oxid now stores and verifies the prototype's detached Compact issuance proof.
  That signature authenticates a credential body only; it is not the
  Digital Passport selective-disclosure/age presentation proof. The standalone
  proof adapter now re-verifies that bundle, independently reconstructs the
  exact generated-Compact public statement, and reauthorizes it through the
  currently managed holder method. ADR-0050 connects checked runtime proving
  and a separate independent verifier only in explicit native headless mode.
- ADR-0041 provides atomic protected storage and ADR-0042 provides local
  interpretation and claim-free planning. ADR-0043 now provides strict
  Final-shaped OpenID4VP/DCQL request matching, exact consent, profile-scoped
  candidates, and single-use sessions. Exact public-input construction,
  authenticated proof construction, and independent verification are available
  in native headless mode and ADR-0083's explicit mobile conformance build;
  normal mobile proving and live transport remain fail-closed.
- Live OID4VCI HTTP/discovery, Authorization Code, by-reference offers,
  Transaction Code, batch/deferred issuance, notification, encrypted responses,
  wallet attestation, deep links, and QR scanning remain later protocol slices.
- Live OpenID4VP response delivery, production mobile proving, ecosystem interoperability,
  live SIOP verifier transport, and browser/native bridge ingress remain later
  adapters.
- Active standalone Compact verification now passes issuer, temporal, schema,
  and trust only after ADR-0073's explicit checks. Status/revocation remains
  visibly `not_checked`; the proof-only conformance constructor also keeps
  issuer/current-time/trust unchecked for immutable historical vectors.
- Exact process-local Jubjub generation/signing is implemented by ADR-0046 and
  cross-checked against the 0.5.0 Midnight DID package. ADR-0047 implements
  selected-DID/public-key validation and credential holder binding for
  standalone issuance. ADR-0048 implements presentation-time current-control
  authorization with explicit same-method rotation semantics. ADR-0073 anchors
  the separate standalone issuer proof key through its exact DID assertion
  method, current-time policy, and pinned trust input. Production issuer/status
  policy and native custody remain tracked separately. The
  prototype's public claim-root-derived holder scalar must not enter normal or
  mobile composition.
- The development key file is not backup, recovery, biometric authorization,
  or production platform custody.

## Privacy and threat review

| Risk | Boundary |
| --- | --- |
| Claim disclosure through protocol/UI/logs | Ordinary views include metadata and stage codes only; headless candidates/plans have no values; targeted Dioxus local reveal is never logged or returned by headless; errors are sanitized. |
| Cross-profile access | Incoming adapters derive profile scope from the active profile and accept no profile parameter. |
| Private material tampering or substitution | Exact codec plus recomputed commitments and signed root fail closed before candidate inventory, preview, or local reveal. |
| Detached proof substitution or format confusion | Exact Compact payload reconstruction and Schnorr verification fail closed; CBOR rejects any detached proof, and ordinary views omit proof bytes. |
| Ciphertext substitution/tampering | XChaCha20-Poly1305 authentication with fixed schema-associated data fails closed. |
| Nonce reuse | A fresh 192-bit OS-random nonce is generated for every whole-document write. |
| Weak key custody claim | Separate 256-bit owner-private key is labeled development-only; normal composition is unavailable. |
| Filesystem redirection | Direct symlinks and non-files are rejected; owner-private directories/files and same-directory atomic writes are used. Existing shared directories fail closed instead of being re-permissioned. |
| CBOR parser exhaustion | 1 MiB credential bound, depth 32, definite lengths, checked offsets, exact end-of-input. |
| Signature confusion | Standard-base64 proof signature, base64url JWK coordinates, curve-specific key construction, assertion relationship, and controller checks. |
| Cryptographic validity mistaken for trust | Proof-only conformance leaves policy stages unchecked; active standalone verification separately requires issuer-key anchoring, current-time validity, schema, and the pinned trust anchor, while status remains explicitly `not_checked`. |
| Malicious credential offer or metadata | Strict duplicate-rejecting bounded JSON; exact embedded-offer parameter; issuer/authorization metadata separation; HTTPS-only production endpoint policy; explicit loopback exception only for standalone. |
| Pre-authorized code replay or disclosure | Single-use adapter session; code/token/nonce never cross the protocol port or incoming DTO and are zeroized when retained. |
| Holder-key substitution | OpenID authentication requires active profile scope, a non-deactivated managed authentication method, exact controller, DID URL `kid`, supported curve, issuer audience, and nonce-bound typed JWS. Credential holder binding separately requires an active managed assertion method, exact controller, canonical EC/Jubjub coordinates, and a holder reference regenerated under the issuer proof. Presentation reloads that exact reference, requires current managed assertion authority, signs the exact statement through protected custody, independently verifies and discards the custody attestation, and emits no token until the distinct credential-family proof verifies. |
| Self-issued login replay or verifier substitution | Exact loopback standalone verifier/request/response endpoints, short-lived nonce/state session consumed before verification, audience/issuer/subject checks, and independent DID-signature verification. |
