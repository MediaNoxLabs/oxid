# Midnight Compact Digital Passport presentation dependency review

- Reviewed: 2026-08-13
- ADR: [ADR-0044](../adr/0044-compose-reproducible-digital-passport-presentation-artifacts.md)
- Scope: final Digital Passport predicate-proof composition and reproducible proving artifacts

## Selected projects and versions

The credential-family source is the official public
`midnightntwrk/midnight-verifiable-credentials` repository at immutable commit
`39b1354212620b396e914b29603e6a38f2656546`. Oxid consumes only
`packages/prototypes/credential-families/digital-passport` from that source.
The package is private/reference-level and does not publish a managed contract
or proving artifacts.

The Compact CLI, compiler, ZKIR tools, and circuit parameters come through the
official public `midnightntwrk/midnight-did` flake at immutable commit
`05b237a5e51f9c22853b424e7d4236dfa9384c24`. The selected versions are Compact
CLI 0.5.1, compiler 0.30.0, Compact language 0.22.0, and generated runtime
0.15.0. `flake.lock` records the source revisions and NAR hashes.

## License, maintenance, and security evidence

Both selected repositories declare Apache-2.0 licensing. Their source is
maintained in the official Midnight GitHub organization, and the
`midnight-verifiable-credentials` `develop` reference was rechecked on the
review date. The Digital Passport package is explicitly prototype/reference
material, so activity is not treated as API-stability or production-readiness
evidence.

The composition uses the package's Jubjub issuer and holder/context signature
checks, domain-separated persistent hashing and commitments, private opening
validation, and the Midnight ZKIR/Halo2 proving stack. No independent audit of
this Oxid composition is claimed. Source revision, compiler/runtime versions,
the exact `bls_midnight_2p18` parameter digest, circuit model, and generated
artifact digests are recorded together. Proof artifacts are public but
security-critical and are never accepted without their manifest identity.

The pinned upstream repository's complete offline pnpm build currently fails
because its Nix dependency closure omits the
`@midnight-ntwrk/midnight-did@0.5.0` tarball. Oxid does not loosen hashes or use
network fetches during a build to bypass that failure; it consumes only the
immutable source subtree and separately pinned official toolchain.

## Platform evidence

The artifact derivation is supported for `aarch64-darwin` and `x86_64-linux`.
It builds locally on Apple silicon macOS and is an explicit hosted Linux CI
gate. Generated artifacts are target-independent data, but no Android, iOS,
desktop application, or WASM runtime integration is claimed by this source-only
slice. Each such consumer must be tested independently, with iOS/Android
latency, peak RSS, package-size, cancellation, and backgrounding measurements
before mobile enablement.

## API stability and adapter boundary

Only Nix and the Oxid-owned final Compact source know the upstream repository
layout. Core, application, headless, and UI packages receive no Compact,
credential-family, generated-runtime, or proof-system type. The future runtime
implementation belongs behind the existing `PresentationProofPort` and
`PresentationVerifierPort`; its portable proof DTO must remain an Oxid-owned,
bounded, redacted type.

The selected APIs are pre-stable prototype surfaces. Any source, compiler,
runtime, parameter, circuit k/row, artifact-shape, or hash change triggers a
coordinated compatibility and privacy review.

## Selection and alternatives

This source was selected because it is the exact official Digital Passport
family already used for Oxid's commitment-bound fixture and provides the
required age-predicate validation. Alternatives rejected for this slice were:

- committing generated managed artifacts or the 85 MB proving key to Git;
- floating a branch/tag or copying an external checkout into the repository;
- substituting a local age calculation or generic holder signature for a ZK
  predicate proof;
- inheriting the broken complete upstream pnpm build; and
- using experimental Rust code generation that is not part of the selected
  official toolchain release.

## Exit and replacement strategy

The immutable source include and toolchain packages can be replaced behind the
same artifact and presentation ports if Midnight publishes a stable managed
package, supported native Rust code generation, or a smaller mobile prover.
Replacement must preserve exact credential/challenge/domain/consent binding,
independent verification, tamper coverage, and fail-closed OpenID behavior.
