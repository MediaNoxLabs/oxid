# ADR-0041: Protect format-private credential material as opaque bytes

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3–7, 10, 12–13, 16–18 and [issue #26](https://github.com/MediaNoxLabs/oxid/issues/26)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/vc_store/` and `wallet-core/src/oid4vci_client/credential/digital_passport.rs`
- Amends: ADR-0003, ADR-0007, ADR-0009, ADR-0011, ADR-0013, ADR-0017, ADR-0020, ADR-0021, ADR-0023, ADR-0038, and ADR-0039
- Implementation state: opaque bounded private material, issuance/import propagation, and encrypted-store schema migration implemented; Digital Passport interpretation, disclosure preview, and UI are delivered by ADR-0042

## Context

The prototype retains commitment openings and their claim inputs beside a
Digital Passport credential. That material is required for later selective
disclosure and predicate proofs, but it is not issuer-signed credential bytes,
searchable metadata, or safe UI/headless output. Oxid previously retained only
the signed credential, so an issuance adapter had no protected route for
format-private response material.

Copying the prototype's field-specific rows into credential core would couple
the domain to one Compact schema and make plaintext claims easy to expose. A
separate uncoordinated store would permit orphaned openings or a credential
without its required private material.

## Decision

`credential/domain` owns a single optional `CredentialPrivateMaterial` value.
It is an opaque byte string with a 256 KiB maximum and a redacted `Debug`
implementation. Core validates its presence and size but never parses its
format. `CredentialRecord` owns the value so repository upsert, replacement,
profile scoping, and deletion remain atomic with the signed credential.

The verified-import command and protocol credential result carry the optional
bytes only between trusted application and adapter boundaries. Ordinary
`CredentialView`, Dioxus inventory state, headless responses, errors, and
capability metadata continue to omit them.

`storage-credential-json` writes schema version 2, encoding the private bytes
inside the same authenticated XChaCha20-Poly1305 document as the original
credential. It can read version 1 documents as records with no private
material, so existing development wallets remain usable. Authentication,
base64, size, or domain validation failure remains an integrity error.

ADR-0042 adds the Digital Passport adapter that validates and interprets its
exact private-part encoding, exposes only schema-neutral claim candidates
through focused use cases, and permits local reveal only from an explicit UI
action. This ADR does not authorize returning claim values over the headless
protocol. OpenID4VP request handling, verifier disclosure consent, and proof
generation remain a separate presentation slice.

## Consequences

- Credential-private material shares the credential's encrypted, profile-
  scoped, atomic lifecycle without entering normalized metadata.
- Credential core stays independent of Digital Passport, CBOR/JSON codecs,
  Compact circuits, and OID4VC wire types.
- Issuance adapters can retain required format-private response material after
  successful credential verification.
- The standalone issuer still emits no private material until the matching
  Digital Passport validation adapter lands; synthetic unbound claims must not
  be attached to the existing public credential fixture.
- Native mobile key wrapping remains required before this development store can
  be described as production custody.

## Validation

- domain tests reject empty/oversized material and prove debug redaction;
- encrypted-store tests prove signed bytes, metadata, and private material are
  absent from ciphertext and survive reopen together;
- focused protocol/import tests keep legacy no-private-material issuance green;
- workspace all-target/all-feature compilation proves every composition and
  incoming adapter accepts the extended internal contract.
