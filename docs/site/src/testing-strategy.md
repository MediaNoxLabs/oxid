# Testing strategy

What "tested" means in this repository, layer by layer, and the policies a
change is reviewed against. The
[quality constitution](quality-constitution.md) is the versioned policy
authority; this page explains the repository's current
practice. It codifies the practice that already exists (≈500 tests, hermetic by
construction) and the gaps the 2026-08 independent review closed or scheduled.

## The pyramid, as practiced here

| Layer | What lives here | Policy |
| --- | --- | --- |
| **Domain invariants** | Constructor/`parse` rejection, checked accounting, state-machine transitions — inline `#[test]`s in every domain crate | Every invariant a constructor enforces has a rejection test. Money-adjacent arithmetic is `checked_*` and tested at bounds. |
| **Known-answer vectors** | Crypto primitives against external truth: BIP-32 Vector 4 (incl. the leading-zero key), official address-codec vectors, the pinned generated-runtime oracle roots | Any function that derives, signs, or encodes value-bearing bytes must be anchored to a spec vector or pinned oracle — never only to itself. |
| **Property-based tests** | `proptest` (exact-pinned, dev-only): backup-envelope round-trip, any-single-byte-corruption and truncation fail-closed | Codecs and parsers facing untrusted bytes get: a round-trip property, a corruption/no-panic property. Grow coverage opportunistically with each codec touched. |
| **Adapter conformance** | Hermetic fixtures: loopback GraphQL-WS servers for live sync (DUST + shielded: resume, cancel-with-consistent-checkpoint, redacted failure), storage permission/symlink/tamper rejection, JNI exception recovery | No test may touch non-loopback network, ambient time, or shared state. Failure paths are first-class: every fail-closed claim has the test that proves it. |
| **Black-box integration** | The headless binary driven over NDJSON (`persistent_profile_flow`), asserting protocol truthfulness incl. secrets-rejected-without-echo | New headless methods ship with black-box coverage of success, rejection, and restart persistence. |
| **Mobile end-to-end** | XCUITest + CDP smoke flows (profile → custody → credential → vault → backup roundtrip) on simulator/emulator | Evidence of the full journey per platform; explicitly *not* physical-device or performance evidence — those are release-gate items. |
| **Release gates** | `#[ignore]`d real-proving tests (p18 artifacts), `nix flake check` hermetic suite (nightly), physical-device budgets (backlog) | Heavy truth runs on cadence, not per push — but it runs: the nightly executes every flake check daily. |

## Coverage policy

The strict gate enforces **80% line coverage** via `cargo llvm-cov`
(UI and app shells excluded from measurement). Policy, not just threshold:

- Coverage is a *floor*, not a target; the review question is always "is the
  risky path tested", not "is the number green".
- The known headroom risk (the gate passing within a fraction of a point)
  is treated as amber: slices that add substantial code must not dilute
  coverage below the floor minus noise — add tests with the slice.
- Aspiration (tracked, not yet enforced): per-crate floors for the custody,
  midnight, and backup adapters, which carry the highest-consequence code.

## What a slice must ship (reviewer checklist)

1. Tests in the same commit as the behavior — never "tests later".
2. Negative paths for every new failure variant (typed errors make the
   enumeration mechanical).
3. Hermeticity: injected clocks/randomness, loopback-only network,
   temp-dir isolation with owner-only permission assertions where relevant.
4. For crypto/codec code: KAT or property tests per the policies above.
5. For mobile-visible behavior: the smoke flows extended, not bypassed.
6. For claims in ADRs/docs ("fails closed", "never persists X"): the test
   that would fail if the claim broke.

## Anti-goals

- No snapshot-everything testing: assert semantics, not markup dumps
  (except the deliberate brand security-copy snapshots in white-labeling).
- No mocking of the unit under test's own layer; fakes live at ports.
- No network "integration" tests against real services in CI — live truth
  belongs to explicitly configured standalone runs and release gates.
