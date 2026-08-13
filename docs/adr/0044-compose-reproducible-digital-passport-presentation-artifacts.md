# ADR-0044: Compose reproducible Digital Passport presentation artifacts

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3–7, 9–13, 16–18, 21 and [issue #28](https://github.com/MediaNoxLabs/oxid/issues/28)
- Credential-family source: `midnight-verifiable-credentials` commit `39b1354212620b396e914b29603e6a38f2656546`
- Toolchain source: `midnight-did` commit `05b237a5e51f9c22853b424e7d4236dfa9384c24`
- Amends: ADR-0006, ADR-0010, ADR-0013, ADR-0015, ADR-0020, ADR-0022, ADR-0028, ADR-0042, and ADR-0043
- Implementation state: immutable source/toolchain inputs, an Oxid-owned final Compact composition, real prover/verifier artifact generation, a digest manifest, exact Rust public-input construction, a portable public-input codec, and independent statement reconstruction are implemented; ADR-0046 supplies the development Jubjub primitive and ADR-0047 binds standalone issuance to the selected holder, while presentation-time reauthorization, proof execution/encoding, independent proof verification, and `vp_token` remain fail-closed

## Context

The pinned Digital Passport package is a pure credential family. It owns the
credential, request, presentation, signature, disclosure, and age-predicate
semantics, but deliberately does not publish a generated managed contract or
proving keys. The upstream architecture assigns those artifacts to a final
deployable composition.

Oxid therefore cannot consume a published Cargo crate or treat the generic
holder/context Jubjub signature as the requested predicate proof. Copying a
large generated key into Git would also obscure its inputs, make upgrades hard
to review, and violate repository hygiene.

The complete pinned upstream pnpm/Nix artifact build is not a usable dependency
boundary: during this review its offline closure lacked the
`@midnight-ntwrk/midnight-did@0.5.0` package tarball. The credential-family
source and official Compact toolchain are independently reproducible and do
not require Oxid to inherit that incomplete application build.

## Decision

Oxid owns the final composition at
`contracts/presentation/digital-passport-presentation.compact`. The Nix flake
supplies the credential-family source and Compact toolchain as separate,
immutable Git inputs. The artifact derivation must use:

- Compact CLI 0.5.1;
- Compact compiler 0.30.0, language 0.22.0, and generated runtime 0.15.0;
- the full authenticated `bls_midnight_2p18` parameter file; and
- direct `compactc --skip-zk`, `zkir mock-compile`, and `zkir compile` steps.

The final circuit validates the signed credential, holder/context signature,
request satisfaction, verifier challenge, and private date-of-birth opening.
Its disclosed, domain-separated public statement binds the exact:

1. signed credential body root;
2. presentation body root;
3. verifier challenge hash;
4. verifier domain hash;
5. hash of the actual five disclosure flags and age threshold;
6. verifier-controlled current day; and
7. requested age threshold.

The circuit writes only that public statement to its ledger field. This makes
the final composition a genuine proof circuit rather than a pure local
calculation; it does not authorize deployment or submission. A runtime adapter
must account for the precise initial/current ledger context and must not expose
private claim values, openings, or witnesses.

Generated compiler output, source maps, proving keys, verifier keys, and ZKIR
stay in the Nix store and out of Git. Each build emits a JSON manifest recording
the source and toolchain revisions, versions, source/lock/parameter hashes,
circuit size, byte sizes, and SHA-256 of every artifact. The reviewed
`aarch64-darwin` baseline is `k=18`, 156,301 rows, with an 85,011,711-byte
prover key. A change in rows, k, or an artifact digest requires an explicit
review, not a silent baseline refresh.

This artifact set does not open the protocol gate. `PresentationProofPort` and
`PresentationVerifierPort` remain separate. The verifier must independently
reconstruct the complete public statement from the consumed OpenID request,
credential, and exact consent; verify freshness and domain policy outside the
proof; then verify the proof with the authenticated verifier key. Only that
successful path may create a `vp_token`.

The adapter boundary uses the Oxid-owned, fixed-size `MPS1` public-input
encoding. Version 1 is exactly 524 bytes and contains the five statement roots
or hashes, verifier-controlled current day, age threshold, disclosure flags,
only the selected public values/openings with canonical zero padding for all
unselected slots, and the final statement. The private date-of-birth value and
opening are never encoded. The verifier-domain input is
`SHA-256("oxid:openid4vp:verifier-domain:v1\0" || verifier_domain)`; it is
derived from the already validated request extension and is distinct from the
nonce-derived verifier challenge.

Standalone composition now runs an exact preflight behind
`PresentationProofPort`: it reloads the profile-scoped encrypted credential,
re-verifies the detached issuance proof, validates all protected openings,
applies the request selection and age precondition, round-trips `MPS1`, and
independently reconstructs the statement. It then deliberately returns
`proof_unavailable`. This exercises the real proof-preimage boundary without
manufacturing proof bytes or changing the OpenID gate.

## Consequences

- `nix build .#presentation-compact-artifacts` is the single reproducible
  artifact-generation entry point; CI builds it on Linux.
- Oxid consumes no unpublished Cargo package, mutable branch, locally copied
  upstream checkout, or committed generated proof material for this slice.
- The credential family can evolve independently of the final presentation
  composition, but either immutable revision change requires a new dependency,
  circuit-size, privacy, and interoperability review.
- The 85 MB prover key and proving latency/RSS remain material mobile risks.
  iOS and Android packaging and measurements are required before enabling a
  production or standalone-development mobile prover.
- Presentation-time protected-holder reauthorization/signing, proof execution,
  portable proof encoding, independent proof verification,
  proof-byte/ledger-context vectors, OpenID response construction, and verifier
  delivery remain issue #28/#29 work.
  Until all are present, acceptance continues to return `proof_unavailable`
  and no `vp_token` exists.

## Validation

- The derivation asserts exact source/toolchain versions, contract-info
  versions, `pure=false`, `proof=true`, `k=18`, and 156,301 modeled rows.
- It asserts all eight required compiler, generated-contract, key, and ZKIR
  files are non-empty before producing the manifest.
- Manifest digests are independently rehashed after the Nix build.
- Hosted Linux CI builds the same flake package; the local reviewed artifact
  build is on `aarch64-darwin`.
- Rust conformance tests reproduce the generated Compact oracle roots for the
  exact standalone credential and first-name/last-name/age-over-18 selection:
  credential `b42f1115…00432`, presentation `cf7570ef…d2876`, consent
  `5a442aeb…d20a3`, and statement `475caef5…4011c`.
- Codec/context tests reject truncation, non-canonical hidden slots, statement
  tampering, challenge mismatch, and request mismatch without logging values.
- Later runtime delivery must add a valid proof fixture plus ledger-context and
  proof-byte tamper failures before ADR-0043's fail-closed response gate
  changes.
