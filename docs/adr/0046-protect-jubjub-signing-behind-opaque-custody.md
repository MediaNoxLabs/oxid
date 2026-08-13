# ADR-0046: Protect Jubjub signing behind opaque wallet custody

- Status: Accepted
- Date: 2026-08-13
- Source: Blueprint §§3, 7, 9–13, 16–18 and [issue #29](https://github.com/MediaNoxLabs/oxid/issues/29)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/secret_storage/jubjub_schnorr.rs`
- Reference package: `@midnight-ntwrk/midnight-did-jubjub-schnorr` 0.5.0, `packages/jubjub-schnorr/src/signing.ts`
- Amends: ADR-0011, ADR-0017, ADR-0020, ADR-0021, ADR-0024, ADR-0037, ADR-0043, ADR-0044, and ADR-0045
- Implementation state: exact process-local Jubjub generation/signing, opaque references, public cross-language conformance, headless lifecycle, ADR-0047 standalone DID/credential issuance binding, and ADR-0048 presentation-time re-authorization are implemented; proof wiring, durable native wrapping, user presence, and production composition remain fail-closed

## Context

The reviewed prototype can generate and use Jubjub keys, but its Passport Vault
presentation shortcut derives a holder scalar from the public credential claim
root. Anyone with the credential body can reproduce that scalar, so it cannot
demonstrate holder possession. Copying it would turn a useful conformance demo
into a false custody claim.

Oxid already owns profile-scoped key-operation ports. They return opaque key
references, public metadata, and signatures while private material remains in
the adapter. The development adapter deliberately supported Ed25519, P-256,
and BIP32/secp256k1-Schnorr but rejected Jubjub. The presentation work now needs
the real Jubjub primitive without widening application or incoming DTOs to
accept a seed or scalar.

## Decision

Add Jubjub to the existing `WalletKeyOperationPort` implementation, not to the
presentation domain and not as a format-specific private-key DTO.

The development adapter:

- generates a fresh 32-byte seed through `RandomPort` inside the profile's
  unlocked custody boundary;
- stores that seed in a zeroizing process-local key object indexed only by an
  Oxid `WalletKeyReference`;
- derives the secret scalar as the reference package does: SHA-256 of the
  32-byte seed interpreted big-endian and reduced modulo the Jubjub subgroup
  order;
- exposes only Midnight's canonical 32-byte compressed public point with
  `JubjubCompressed` metadata;
- hashes a bounded payload into four big-endian 64-bit field limbs, derives the
  deterministic domain-separated nonce, and uses the generated Compact
  Schnorr challenge transcript reduced to 248 bits;
- returns the reference-compatible 96-byte
  `announcement.x || announcement.y || response` big-endian signature; and
- applies the existing profile isolation, lock, list, conflict, confirmation,
  and delete rules without special incoming parameters.

The public oracle for seed `[0x23; 32]` and UTF-8 payload
`Oxid holder statement` is retained in the adapter test. Its 96-byte signature
is:

```text
583fe322acfa2db7c9328093c9c2fa83901fa81d81e6bab10af556ca91fc94bd
519e689fcd0d1a7c988b864562a99be1774d88aa8bb69e79ecd1013ac9df0845
08077115a06c82e6008f2f5496ce6d19e94c76d5909c9c1fa1da0d9f0e16dedb
```

This fixture is public conformance data, not a production key. It matches the
0.5.0 TypeScript reference output exactly.

Custody support alone does not establish credential holder binding. Before a
presentation proof adapter may use this primitive, it must independently
prove that the selected profile-managed DID and method correspond to the
holder verification-method reference signed into the Compact credential, and
that the protected public key is the public key authorized by that method.
ADR-0047 establishes that mapping during standalone issuance by regenerating
the exact credential body and issuer proof for the selected managed Jubjub
assertion method. The presentation adapter must still reload and re-authorize
that method at proof time rather than silently trusting a persisted public DID.

Normal production composition continues to use unavailable custody. Native
Apple/Android adapters must implement ADR-0017's protected wrapping,
device-only storage, authorization, and truthful protection reporting before
this algorithm can be production-capable.

## Consequences

- Headless and incoming adapters using the explicitly selected standalone
  development composition can exercise real Jubjub generation, listing,
  signing, locking, and deletion using only opaque references.
- The Rust signer is wire-compatible with the 0.5.0 Midnight DID package and
  uses the same immutable ledger cryptography already pinned by ADR-0015.
- No seed, scalar, private key, claim opening, or proof witness enters an
  application, UI, or headless result.
- A holder signature remains distinct from the Digital Passport predicate
  proof and cannot by itself produce a `vp_token`.
- Issue #29 remains open for presentation-time binding and native custody;
  issue #28 remains open for proof execution and independent proof verification.
