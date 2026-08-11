# Midnight Git source policy

- Status: source policy established; initial M2 baseline selected by ADR-0015
- Reviewed: 2026-08-11
- Repositories: `midnightntwrk/midnight-ledger`, `midnightntwrk/midnight-zk`
- ADR: [ADR-0015](../adr/0015-midnight-library-selection.md)

## Current state

Oxid M0 had no direct dependency on a Midnight ledger or proof crate. ADR-0015
now selects the official source and protocol baseline for M2. A workspace crate
may still omit a selected package when its capability does not use it;
selection is not permission to add proving or aggregate wallet dependencies to
an account-read adapter.

The first `adapters/midnight` account-read implementation intentionally has no
direct Midnight Git dependency. A host build trial of `midnight-ledger` at the
selected revision with `default-features = false` succeeded, but Cargo still
resolved the ledger's unconditional transaction/proof dependency graph. The
adapter needs only the reviewed atomic-unit constants and public Wallet SDK
address vectors, so importing that graph would add coupling without providing a
runtime capability. The trial dependency was removed and no Midnight package is
present in `Cargo.lock`. A later transaction adapter that uses canonical ledger
types must use the exact source form below and pass both native target graphs.

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

ADR-0015 selects these as the initial compatibility baseline. Only the ledger
revision is appropriate for a future transaction adapter; neither revision is
currently selected in Cargo. The proof revision remains absent until a proving
adapter requires it. Each direct use must
still validate its exact feature set, license graph, security posture, and
Tier-1 native target builds.

## Enforcement

`scripts/check-midnight-sources.sh` rejects known ledger/proof packages when
they use local paths, unofficial repositories, branches, tags, or abbreviated
revisions. The check is part of `run.sh` and therefore the local and CI gate.
