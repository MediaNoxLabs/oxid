# ADR-0044: Compose reproducible Digital Passport presentation artifacts

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3–7, 9–13, 16–18, 21 and [issue #28](https://github.com/MediaNoxLabs/oxid/issues/28)
- Credential-family source: `midnight-verifiable-credentials` commit `39b1354212620b396e914b29603e6a38f2656546`
- Toolchain source: `midnight-did` commit `05b237a5e51f9c22853b424e7d4236dfa9384c24`
- Amends: ADR-0006, ADR-0010, ADR-0013, ADR-0015, ADR-0020, ADR-0022, ADR-0028, ADR-0042, and ADR-0043
- Implementation state: immutable source/toolchain inputs, an Oxid-owned final Compact composition, real prover/verifier artifact generation, and a digest manifest are implemented; proof-preimage execution, portable proof encoding, independent runtime verification, tamper vectors, and `vp_token` remain fail-closed

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
- Proof execution, independent verification, positive/tamper fixtures,
  portable proof encoding, OpenID response construction, and verifier delivery
  remain issue #28 work. Until all are present, acceptance continues to return
  `proof_unavailable` and no `vp_token` exists.

## Validation

- The derivation asserts exact source/toolchain versions, contract-info
  versions, `pure=false`, `proof=true`, `k=18`, and 156,301 modeled rows.
- It asserts all eight required compiler, generated-contract, key, and ZKIR
  files are non-empty before producing the manifest.
- Manifest digests are independently rehashed after the Nix build.
- Hosted Linux CI builds the same flake package; the local reviewed artifact
  build is on `aarch64-darwin`.
- Later runtime delivery must add a valid proof fixture plus challenge, domain,
  disclosure, threshold, credential, ledger-context, and proof-byte tamper
  failures before ADR-0043's fail-closed response gate changes.
