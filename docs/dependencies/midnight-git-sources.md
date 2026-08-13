# Midnight Git source policy

- Status: source policy enforced; canonical transaction, standalone submission, Digital Passport commitment, and exact Compact issuance-proof packages selected by issues #9/#11/#26/#29
- Reviewed: 2026-08-12
- Repositories: `midnightntwrk/midnight-ledger`, `midnightntwrk/midnight-zk`
- ADR: [ADR-0015](../adr/0015-midnight-library-selection.md)

## Current state

Oxid M0 had no direct dependency on a Midnight ledger or proof crate. ADR-0015
now selects the official source and protocol baseline for M2. A workspace crate
may still omit a selected package when its capability does not use it;
selection is not permission to add proving or aggregate wallet dependencies to
an account-read adapter.

The account-read portion of `adapters/midnight` intentionally uses owned types
and protocols. Issue #9 adds a focused native transaction capability that does
consume canonical ledger, coin, storage, serialization, and cryptographic
types. Those packages are now direct Git dependencies at the selected revision
and are present in `Cargo.lock`; the target-specific dependency section keeps
them out of `wasm32`. Issue #11 enables the ledger's `proving` feature for DUST
spends and adds `midnight-onchain-runtime` from the same immutable Git revision.
Issue #12 adds `midnight-zkir 2.1.0` directly from that same official Git source
and revision with default features disabled.
Issue #18 adds `midnight-zswap 8.2.0-rc.1` as a direct native-only dependency
from the same immutable revision for canonical public-key derivation and the
adapter-private shielded state machine.
Issue #26 reuses `midnight-base-crypto` and `midnight-transient-crypto` from the
same immutable revision inside `adapters/vc-midnight` to reproduce the
reference Digital Passport `persistentCommit` and `persistentHash` contract.
Issue #29 reuses those same exact packages to reconstruct the upstream Compact
credential/issuance payload roots, decode Jubjub points/scalars, and verify the
detached Schnorr issuance proof. It adds no path dependency, repository,
revision, or proof-system package.
ADR-0046 reuses `midnight-transient-crypto` and `midnight-serialize` at that
same full revision inside the development custody adapter for canonical Jubjub
point arithmetic/compression and a 0.5.0-compatible Schnorr transcript. It adds
no new source or floating dependency.
ADR-0047 also uses those already pinned packages in `adapters/did-midnight` to
decode the canonical compressed public key into the official little-endian
EC/Jubjub JWK coordinates. Holder-bound standalone issuance therefore adds no
repository, revision, path dependency, or floating source.
The feature resolves published `midnight-proofs`, `midnight-circuits`, and
`midnight-zk-stdlib` releases transitively. There is no direct `midnight-zk`
dependency because the compatible proof crates are already selected by the
immutable ledger/ZKIR graph.

The reviewed prototype at commit
`074b1a4bccbfee1740ee188374b606a022ecef42` used paths relative to the
`midnight-ledger` monorepo for `midnight-ledger`, `midnight-zswap`,
`midnight-zkir`, runtime, serialization, storage, and cryptography crates. Its
root manifest also patched `midnight-proofs` from a mutable fork branch. Neither
form is valid in this standalone public repository.

## Required source form

For an M2 adapter, use the official public HTTPS repository and a full
immutable Git commit:

```toml
midnight-ledger = { git = "https://github.com/midnightntwrk/midnight-ledger.git", rev = "d9414884db9da9e9b1f6f3a7f742d79a5732f817", default-features = false }
midnight-zkir = { git = "https://github.com/midnightntwrk/midnight-ledger.git", rev = "d9414884db9da9e9b1f6f3a7f742d79a5732f817", default-features = false }
midnight-zswap = { git = "https://github.com/midnightntwrk/midnight-ledger.git", rev = "d9414884db9da9e9b1f6f3a7f742d79a5732f817", default-features = false }
midnight-proofs = { git = "https://github.com/midnightntwrk/midnight-zk.git", rev = "cd2c27b2659de157409a9b96dba0dbaf1218f00b" }
```

The ledger monorepo is also the source of its `midnight-zkir` crate; the
similarly named `midnight-zk` repository is the source of proof-system crates.
Do not replace `rev` with a branch or a floating tag. `Cargo.lock` then records
the resolved source commit, but the manifest pin remains the reviewable intent.

As of the review date, both official GitHub repositories were reachable. Their
reviewed branches resolved to:

- `midnight-ledger`, `ledger-8`:
  `d9414884db9da9e9b1f6f3a7f742d79a5732f817`;
- `midnight-zk`, `main`:
  `cd2c27b2659de157409a9b96dba0dbaf1218f00b`.

ADR-0015 selects these as the initial compatibility baseline. The ledger
revision is selected for canonical transaction authorization and both remote
and local DUST proof orchestration. `midnight-zkir` supplies the local provider
from that repository. The separate `midnight-zk` revision remains absent
because the compatible published proof crates are already selected by the
immutable ledger graph. Registry proof crates reached through the selected
ledger/ZKIR graph are lockfile-pinned transitive inputs. Each direct use must still validate
its exact feature set, license graph, security posture, and Tier-1 native target
builds.

## Enforcement

`scripts/check-midnight-sources.sh` rejects known ledger/proof packages when
they use local paths, unofficial repositories, branches, tags, or abbreviated
revisions. The check is part of `run.sh` and therefore the local and CI gate.
