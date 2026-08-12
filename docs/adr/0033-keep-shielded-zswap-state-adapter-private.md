# ADR-0033: Keep shielded Zswap keys and replay state inside the Midnight adapter

- Status: Accepted
- Date: 2026-08-12
- Source: Blueprint §§3, 5–8, 12–13, 16–18 and [issue #18](https://github.com/MediaNoxLabs/oxid/issues/18)
- Prototype source: `midnight-ledger` commit `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/shielded`, `address.rs`, and the Dioxus receive/assets surfaces
- Canonical sources: `midnight-ledger` commit `d9414884db9da9e9b1f6f3a7f742d79a5732f817`, `midnight-wallet` commit `25d0c3857fc0e20435e06a9225bd8709ecce1115`, and `midnight-indexer` commit `82759bf186184684f13a9ffa97b58b7b7684f47c`
- Amends: ADR-0015, ADR-0017, ADR-0024, ADR-0029, and ADR-0030
- Implementation state: protected role-3 derivation, official shielded address-vector conformance, bounded tagged-event decoding, canonical adapter-private replay, safe account/headless projection, and mobile QR presentation implemented; checkpointing and explicit shielded sync sessions remain in issue #18

## Context

The prototype derives a Zswap child, presents its shielded receive address, and
folds the chain-wide `zswapLedgerEvents` stream into the official local state.
That state contains spendable coins, nullifiers, pending spends, and the
commitment tree needed by later proofs. It is privacy-sensitive even though a
shielded receive address is intentionally public.

Copying the prototype's aggregate wallet service or network-only database key
would couple domain/UI code to ledger types and could bind one account's notes
to another account after derivation changes. Reimplementing the state machine
with an Oxid approximation would risk commitment-tree or ownership divergence.

## Decision

Use the Wallet SDK path `m/44'/2400'/<account>'/3/0` through the existing
borrowed-secret custody operation. The Midnight adapter constructs official
`midnight-zswap` keys only inside that callback. It serializes only the coin
and encryption public keys into the canonical 64-byte Wallet SDK payload and
Bech32m-encodes `mn_shield-addr` on mainnet or
`mn_shield-addr_<network>` elsewhere. Seed, child scalar, decryption key, and
nullifier material never cross the adapter boundary.

The account domain retains one primary unshielded address for transaction
planning plus a validated collection of distinct public receive addresses.
Application, headless, and Dioxus adapters may expose the shielded address and
its exact QR payload. They do not receive Zswap keys or ledger state.
`oxid.headless.v1` adds `wallet.address.shielded` while retaining
`wallet.address.list` and the primary `receiveAddress` field for compatibility.

The replay core keeps the official `zswap::local::State<DefaultDB>`
adapter-private. Every output commitment must
be inserted at its exact Merkle index; an output is owned only after local
decryption and commitment recomputation; foreign branches are collapsed; and
spends remove notes by nullifier. Oxid-owned projections may contain only
bounded lifecycle/progress metadata, exact per-token balances, owned-note
count, freshness, and sanitized failures.

Any durable Zswap checkpoint must be a separate versioned binary store scoped
by network, a one-way public-key fingerprint, source/protocol identity, and
cursor. It must be owner-private, symlink-resistant, bounded, atomically
replaced, and resumable at the next cursor. Cached state is display/replay
input, not proof of spend readiness. Native work runs off the renderer and is
cancellable only at consistent checkpoint boundaries. Production composition
remains fail-closed until approved custody and endpoint configuration exist.

## Consequences

- The protected account exposes both canonical public receive rails without
  broadening ordinary DTOs to carry secret material.
- `midnight-zswap` becomes a direct native-only dependency from the same full
  immutable ledger revision already selected by ADR-0015.
- Existing unshielded preparation keeps using the primary receive address, so
  adding shielded presentation does not change transaction-recipient policy.
- Bounded indexer envelopes are cross-checked against their tagged official
  event variant before replay, and malformed/non-linear streams fail closed.
- Future shielded spending can reuse the official retained state without
  making its nullifiers, Merkle paths, or witnesses public application data.
- Browser-only builds retain the existing reduced account surface because the
  canonical Zswap dependency remains native-target gated.
