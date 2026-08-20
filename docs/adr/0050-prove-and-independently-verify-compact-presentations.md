<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-0050: Prove and independently verify Compact presentations

- Status: Accepted
- Date: 2026-08-14
- Blueprint: §§3–7, 9–13, 16–18, 21
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, Digital Passport presentation path
- Reference package: `midnight-verifiable-credentials` commit `39b1354212620b396e914b29603e6a38f2656546`
- Ledger runtime: `midnight-ledger` commit `d9414884db9da9e9b1f6f3a7f742d79a5732f817`
- Related: ADR-0010, ADR-0013, ADR-0015, ADR-0020, ADR-0022, ADR-0028, ADR-0043 through ADR-0049, issues #27–29
- Implementation state: exact generated-runtime/Rust `ProofPreimage` parity, a self-contained authenticated Nix artifact closure, native checked proving and independent verification, the bounded portable envelope, and explicit standalone headless protocol wiring are implemented; mobile packaging remains fail-closed pending the resource and native-custody gates below
- Amended by: ADR-0083

## Context

ADR-0044 reproducibly generates the final Digital Passport circuit and keys.
ADR-0045 through ADR-0049 provide the exact signed credential, protected
openings, current-holder authorization, and credential-family holder proof.
The remaining boundary is security-critical: a Rust preimage that is merely
similar to generated Compact output can prove a different statement, and a
prover's own success is not independent verification.

The pinned Midnight APIs can load a tagged binary ZKIR, lazily initialize the
85 MB prover key, prove a checked `ProofPreimage`, and verify with the p18
parameters. They also expose `prove_unchecked`, but that API accepts internal
preprocessed witness state and is intended for tests. The static verifier
parameters embedded by `midnight-transient-crypto` stop at p14 and cannot
verify this k=18 circuit.

The generated JavaScript runtime is the normative codec oracle for circuit
inputs and transcripts. In particular, ordinary circuit output contributes to
the communications commitment but not to `public_transcript_outputs`; only
`popeq` query results enter that latter vector. Missing this distinction
produces a well-shaped but different proof preimage.

For optimization tracking this is a prototype/business composition around the
reusable Digital Passport credential family, not a new reusable core circuit.
Source-level inspection attributes the large verification cone to complete
credential-proof, holder-proof, request/disclosure, commitment, and private age
predicate validation. Oxid's statement hash and two ledger assignments are
thin wiring; no per-subtree row report is available, so this is classification,
not a fabricated numerical attribution. Proof acceptance is already split from
issuance and wallet bookkeeping and should remain so.

## Decision

Oxid constructs the presentation `ProofPreimage` in Rust only after credential,
opening, selection, time, current-control, and holder-proof checks succeed. A
conformance vector must tagged-serialize byte-for-byte identically to generated
Compact runtime 0.15.0 for the same inputs. The resolver label is the stable
artifact-set identifier `oxid-digital-passport-presentation-v1`.

The native runtime consumes one Nix-produced, self-contained artifact root. It
contains the prover key, verifier key, tagged binary ZKIR, and
`bls_midnight_2p18` parameters. Startup canonicalizes an absolute root, rejects
relative or parent components, refuses symlinked/non-regular descendants,
bounds every read, authenticates exact compiled-in byte sizes and SHA-256
digests, validates the manifest's immutable source/toolchain identities, and
checks k=18, 156,301 rows, 117 input fields, and communications-commitment use.
It performs no network fetch and accepts no mutable artifact discovery.

Before proving, `IrSource::check` must consume the exact preimage successfully.
This circuit's public-input skip vector must contain no skips. Production uses
only `ProofPreimage::prove::<IrSource>` with operating-system entropy and the
authenticated resolver/parameter provider. `prove_unchecked` is forbidden.

Proof creation's built-in self-check is necessary but insufficient. After
proving, and again at the protocol verifier boundary, Oxid independently loads
the authenticated verifier key and p18 verifier parameters and reconstructs
the verifier statement as:

1. the fixed zero binding input;
2. the public communications commitment; and
3. the twelve exact field elements encoding the two generated state pushes and
   final insert.

The verifier also independently re-verifies the signed credential and holder
proof, decodes and reconstructs `MPS1` from the consumed request and consent,
checks verifier domain/challenge and verifier-controlled current day, and
rejects a different artifact-set identity. It never receives protected claim
material or communications randomness.

The portable `MZP1` envelope contains only the authenticated artifact-set
identity, signed credential body, detached issuer proof, `MPS1` public input,
holder proof, public communications commitment, and tagged ZK proof. It is
versioned, bounded, canonical, and checksummed. No private date of birth,
opening, custody reference, scalar, nonce, or full `ProofPreimage` may be
serialized. OpenID4VP may construct `vp_token` only after decoding this envelope
and completing the independent verifier path.

Normal production composition and development mobile composition remain
fail-closed until a platform-specific artifact packaging/custody decision is
accepted. The standalone headless composition may enable the runtime only from
an explicit authenticated artifact-root configuration.

## Rejected alternatives

- Calling generated JavaScript/WASM at runtime would add a second execution
  stack, complicate mobile packaging, and weaken the Rust-first boundary.
- Trusting `ProofPreimage::prove` self-verification would combine prover and
  verifier failure domains and would not re-establish request/consent policy.
- Using the embedded p14 verifier parameters for a k=18 circuit is invalid.
- Loading keys by filename alone, accepting a mutable cache, or downloading on
  demand would make proof semantics depend on ambient state.
- Serializing the full preimage would disclose protected claim witnesses and
  openings.
- Omitting the signed credential or holder proof from the portable envelope
  would prevent a verifier from independently checking the public protocol
  bindings.

## Consequences

- The generated runtime remains the executable codec oracle while shipping no
  JavaScript runtime in the wallet.
- The Nix closure grows by the 50 MB p18 parameter file; the complete native
  proving input is immutable and works offline.
- Loading p18 prover and verifier parameters has measurable startup CPU/RSS;
  the headless harness records it. On the first aarch64-darwin release
  baseline, the complete flow takes 18.37 seconds and macOS `time -l` reports
  5,073,895,424 bytes maximum resident set size and 160,252,576 bytes peak
  memory footprint. These platform metrics require interpretation, but the
  maximum-resident result is already too high to enable the prover in mobile
  composition without a separate packaging, latency, and memory decision.
- Any source revision, compiler/runtime version, k/rows, input count, artifact
  size, or digest change fails closed and requires a reviewed ADR/baseline
  update.
- Proof bytes alone are intentionally insufficient. Protocol validity also
  depends on the exact public envelope, request, domain, freshness, issuer
  proof, and holder proof.

## Validation

- Rust's tagged preimage is exactly 1,506 bytes with SHA-256
  `5f0618c1ef46d61aa3a9848907ca46a6ea5ac8bb75714b1baa9f3f2b6d32830a`,
  matching generated Compact/WASM for the standalone vector.
- The authenticated native loader runs the real tagged IR check against that
  preimage and rejects path, manifest, digest, size, k, rows, input-count, and
  transcript-shape mismatches.
- Release-mode native proof validation independently verifies and round-trips
  `MZP1`; rejects checksum, proof, commitment, public-input, credential, issuer
  proof, holder proof, request challenge, freshness, and artifact-identity
  tampering; reloads the authenticated runtime; and verifies the same portable
  artifact after restart without protected witness material.
- The complete release-mode headless DID → bound issuance → OpenID4VP consent →
  checked proof → independent verifier flow succeeds in 18.37 seconds and
  rejects replayed consent. Headless views expose neither `vp_token` nor claim,
  opening, private-material, or proof bytes.
- `nix develop` exports the immutable artifact closure explicitly, while
  missing or invalid roots fail closed. Normal production, default mobile, and
  standalone-development mobile compositions do not connect the native prover.
- Full strict Nix/coverage validation, isolated flake package checks, and the
  complete iOS and Android simulator/emulator smoke flows passed on 2026-08-14.
  Mobile proof enablement remains a separate decision regardless of those
  build gates.
