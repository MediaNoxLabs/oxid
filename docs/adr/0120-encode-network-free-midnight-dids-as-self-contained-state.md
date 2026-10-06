# ADR-0120: Encode network-free Midnight DIDs as self-contained public state

- Status: Accepted
- Date: 2026-10-07
- Issue: [#1134](https://github.com/MediaNoxLabs/oxid/issues/1134)
- Depends on: ADR-0003, ADR-0007, ADR-0008, ADR-0011, ADR-0036, ADR-0037, and ADR-0047
- Canonical format source: `MediaNoxLabs/midnight-did@42a8e4aca1c6043f7f2b73463fa3aa3cd3d48b06`

## Context

Oxid's quick identity demo previously created a synthetic
`did:midnight:undeployed` identifier. Its document lived only in the selected
profile's public inventory, so another process could not resolve the DID from
the identifier alone. Presenting that identifier while a Tailnet realm was
selected also implied a ledger deployment which had not occurred.

The reviewed Midnight DID implementation defines a long-form off-chain DID
whose public initial state is encoded in the identifier and authenticated by a
BLAKE2s-256 digest. Oxid needs that interoperable, network-free path now, while
the reusable Ledger8 Rust runtime and Compact-backed deployment are integrated
separately. Private key material must remain behind Oxid's existing opaque
custody boundary.

## Decision

Oxid admits `did:midnight:offchain:<hash>:<state>` as a distinct DID network.
The identity domain owns a bounded, dependency-light codec for the canonical
`MOD1` state envelope:

- the state is URL-safe unpadded Base64;
- the identifier authenticates the exact framed bytes with lowercase
  BLAKE2s-256;
- the version, aliases, verification methods, relationships, and services use
  the canonical fixed-slot encoding and reviewed key tags;
- decoding rejects non-canonical encodings, unknown versions or bits, hidden
  values in absent slots, duplicate identifiers, malformed keys, control
  characters, and oversized input;
- resolution is local and deterministic and performs no HTTP, WebSocket,
  ledger, or profile-store read.

Creation still generates Ed25519 authentication, P-256 assertion, and Jubjub
holder-binding keys through `WalletKeyOperationPort`. Only public JWK material
enters the long-form state; opaque custody references remain adapter-private.
The resulting public document may be retained in the selected profile for
inventory and signing, but retention is not required for resolution.

Off-chain DIDs are immutable. Oxid does not disguise an inventory mutation as
an update to the encoded identifier: changing or deactivating the document
requires a new DID. The legacy `undeployed` lifecycle remains temporarily for
existing fixture and protocol migrations, but new quick-demo UI creation uses
the off-chain form and must not claim network publication.

Ledger-backed `did:midnight` deployment remains a different adapter. This ADR
does not authorize a JavaScript/WebView composer, a new Compact revision, or a
second SSI implementation. Once the reviewed reusable Ledger8 Rust surface is
available, Oxid should replace the local codec behind the same owned domain
boundary and retain the cross-language vector as compatibility evidence.

## Consequences

- A quick demo can create, copy, transfer, and resolve a standards-shaped
  Midnight DID without configuring a Midnight realm.
- The identifier is intentionally larger because it carries authenticated
  public state; it contains no private key, seed, mnemonic, or custody handle.
- Network-free identity and ledger-backed identity are no longer conflated in
  UI copy or resolver routing.
- Updates and deactivation fail closed instead of silently changing the public
  document represented by an immutable long-form DID.
- Existing `undeployed` fixtures require an issue-backed migration before that
  synthetic format can be removed.
