# Midnight Git source policy

- Status: source policy established; runtime selection deferred to M2
- Reviewed: 2026-08-11
- Repositories: `midnightntwrk/midnight-ledger`, `midnightntwrk/midnight-zk`
- ADR: [ADR-0015](../adr/0015-midnight-library-selection.md)

## Current state

Oxid M0 has no direct dependency on a Midnight ledger or proof crate. This is
intentional: the M0 profile slice neither needs those capabilities nor has an
accepted library-selection decision. `scripts/check-midnight-sources.sh`
inspects direct workspace dependencies and currently reports that no M2 source
has been selected.

The reviewed prototype at commit
`074b1a4bccbfee1740ee188374b606a022ecef42` used paths relative to the
`midnight-ledger` monorepo for `midnight-ledger`, `midnight-zswap`,
`midnight-zkir`, runtime, serialization, storage, and cryptography crates. Its
root manifest also patched `midnight-proofs` from a mutable fork branch. Neither
form is valid in this standalone public repository.

## Required source form

When ADR-0015 selects a crate for an M2 adapter, use the official public HTTPS
repository and a full immutable Git commit:

```toml
midnight-ledger = { git = "https://github.com/midnightntwrk/midnight-ledger.git", rev = "<40-character-commit>", features = ["proving"] }
midnight-proofs = { git = "https://github.com/midnightntwrk/midnight-zk.git", rev = "<40-character-commit>" }
```

The ledger monorepo is also the source of its `midnight-zkir` crate; the
similarly named `midnight-zk` repository is the source of proof-system crates.
Do not replace `rev` with a branch or a floating tag. `Cargo.lock` then records
the resolved source commit, but the manifest pin remains the reviewable intent.

As of the review date, both official GitHub repositories were reachable. Their
remote default branches resolved to:

- `midnight-ledger`, `ledger-8`:
  `272c25fcaabcd8f18951bd38a5dd7b0112e37d4a`;
- `midnight-zk`, `main`:
  `cd2c27b2659de157409a9b96dba0dbaf1218f00b`.

These observations prove repository availability; they are not selected
dependency versions. A future adapter must still complete ADR-0015's mobile,
license, security, maintenance, and proving review and validate the chosen
commit with the relevant Cargo features.

## Enforcement

`scripts/check-midnight-sources.sh` rejects known ledger/proof packages when
they use local paths, unofficial repositories, branches, tags, or abbreviated
revisions. The check is part of `run.sh` and therefore the local and CI gate.
