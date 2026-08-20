# ADR-0042: Bind Digital Passport disclosure to signed commitments

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3–7, 9–13, 16–18 and [issue #26](https://github.com/MediaNoxLabs/oxid/issues/26)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/oid4vci_client/credential/digital_passport.rs`, `wallet-core/src/vc_store/`, and `dioxus-wallet/src/vc_views/digital_passport.rs`
- Reference package: `midnight-verifiable-credentials` commit `39b1354212620b396e914b29603e6a38f2656546`
- Amends: ADR-0007, ADR-0009, ADR-0011, ADR-0013, ADR-0015, ADR-0017, ADR-0020, ADR-0021, ADR-0023, ADR-0029, ADR-0038, ADR-0039, and ADR-0041
- Implementation state: deterministic standalone Digital Passport issuance, protected private-part validation, schema-neutral disclosure inventory/planning, local Dioxus reveal, headless lifecycle, restart/deletion, and Tier-1 mobile smoke coverage implemented; verifier presentation and predicate-proof generation remain deferred
- Amended by: ADR-0043, ADR-0044, ADR-0045

## Context

The prototype retains five Digital Passport claim values and commitment
openings: first name, last name, date of birth, document number, and issuing
state. Its UI locally reveals first and last name and lets the holder select an
age threshold. Its presentation action is disabled; it does not generate an
OpenID4VP response or a Compact predicate proof.

Opaque storage from ADR-0041 is necessary but insufficient. Treating arbitrary
private bytes as claims would let an issuer, adapter, or corrupted store attach
values that are not bound to the signed credential. Moving the Digital
Passport field layout into credential core would also make one prototype schema
the wallet's general disclosure model.

## Decision

Credential core owns only schema-neutral privacy tiers, claim paths, labels,
candidate manifests, predicate selections, and a claim-free local preview
result. The application layer owns profile-scoped candidate, preview, and
targeted local-reveal use cases. Ordinary credential views remain unchanged and
never contain claim values, openings, or private material.

`adapters/vc-midnight` owns the exact Digital Passport mapping. Before exposing
even candidate metadata it must:

1. parse a bounded, depth-limited, exact-end private CBOR envelope with all five
   value/opening pairs and reject duplicate, unknown, missing, or malformed
   fields;
2. parse the public commitments and root from a verified phase-1 CBOR
   `DigitalPassportCredential`;
3. recompute every `persistentCommit` and the schema-domain-separated
   `persistentHash` root with the immutable, full-revision-pinned official
   Midnight cryptography packages; and
4. fail closed unless the private values/openings exactly match commitments
   covered by the issuer signature.

The adapter produces five claim-value-free candidates for the standalone
fixture. First name, last name, document number, and issuing state are locally
revealable; date of birth is predicate-only. The current Dioxus card follows
the prototype's visible behavior: it reveals/hides first and last name only
after a local action and plans an age-over-threshold predicate without showing
the date. Values are held only in component-local state and are cleared when
hidden or when the component is left.

The headless adapter exposes `credential.disclosure.candidates` and
`credential.disclosure.preview`. They return only schema ID, labels, paths,
privacy tiers, selected threshold, outcome, and the explicit fact that no
presentation was generated. Headless has no claim-reveal method.

Standalone OpenID4VCI uses a deterministic signed Digital Passport fixture and
matching private material to test the complete flow. Normal composition keeps
the disclosure adapter unavailable. This decision does not implement or claim
OpenID4VP, DCQL matching, `vp_token` construction, verifier delivery,
selective-disclosure proofs, or predicate proofs.

## Consequences

- Private claim material is useful only after both credential verification and
  format-specific commitment validation succeed.
- Schema-specific codecs and Midnight hashing stay at the outgoing edge while
  incoming adapters share small Oxid-owned use cases.
- Profile scope and atomic encrypted persistence/deletion continue to come from
  the credential repository rather than a parallel claims store.
- Local reveal and verifier disclosure remain visibly different actions.
- The checked-in fixture is public conformance data. Its issuer signing seed
  was used only to create the fixture and is not retained.
- Production-native wrapping, issuer transport, presentation/proving, status,
  schema policy, and trust policy remain explicit later gates.

## Validation

- Rust vectors match the immutable reference package's commitments and claim
  root for all five fields.
- Codec tests reject truncation, trailing bytes, oversized input, duplicate
  fields, and commitment/opening tampering without printing material.
- Application/headless tests prove active-profile isolation, safe candidate and
  plan responses, and absence of fixture values from ordinary output.
- Persistent-process tests prove encrypted restart restoration and atomic
  credential/private-material deletion.
- iOS XCUITest and Android WebView smoke flows cover issuance, hidden-by-default
  values, explicit first/last reveal and hide, age-predicate preview, and
  restart restoration.
