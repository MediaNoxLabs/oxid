# Security model

Oxid's security posture is defined by what the code *refuses* to do, and the
refusals are tested. This page describes the model at a working level; the
repository [`SECURITY.md`](https://github.com/MediaNoxLabs/oxid/blob/develop/SECURITY.md)
carries the disclosure policy, and the [ADRs](adr-catalog.md) carry the
binding decisions.

> **Not production-ready.** No claim on this page is an invitation to store
> real assets or identities. The point of the model is that the gaps are
> explicit and fail closed instead of silently degrading.

## Custody

- Key use goes through **opaque references and key-operation ports** — no
  seed, mnemonic, private key, or scalar appears in DTOs, logs, fixtures, or
  the headless protocol, and tests reject secret-bearing requests without
  echoing them.
- On iOS and Android, protected storage seals a multi-curve vault behind the
  platform keystore (Keychain / Android Keystore) with device user-presence
  authorization; blocking authorization waits run on a dedicated worker
  thread, never the UI executor.
- The standalone development composition uses process-local software custody
  that intentionally forgets its root on restart — inconvenient, and
  deliberately so.

## Storage

Every persistent store follows the same hardened pattern: bounded size with
double length checks, `deny_unknown_fields` schemas, owner-only permissions,
symlink rejection, atomic create-new/write/fsync/rename, and separation of
public metadata from protected material. Credentials at rest are encrypted
with XChaCha20-Poly1305; private claim material is commitment-bound and
size-bounded; checkpoint and journal stores persist public progress only.

## Backups

Portable backups are a versioned envelope: Argon2id key derivation with the
KDF parameters, salt, nonce, and lengths bound as authenticated data.
Complete-wallet exports (format v3) use hardened parameters (64 MiB, t=3),
and each readable version maps to exactly one accepted KDF policy — a header
cannot request arbitrary work, and legacy packages remain read-only
recoverable. Recovery preflights destination emptiness and compares restored
custody in constant time.

## Transactions and proofs

- Submission follows **persist-before-broadcast**: the public attempt is
  journaled before the node sees the transaction, ambiguous outcomes stay
  `Submitting` and block blind retries, and reconciliation walks finalized
  chain history before permitting a replacement.
- Contract state shown as authenticated is either deterministically replayed
  from finalized node history (`canonical_finalized_replay`) or labeled as
  unproven (`indexer_supplied_not_proven`) — read models never authorize
  calls.
- Zero-knowledge presentations execute the real Compact circuits against
  authenticated, digest-pinned artifacts, with independent verification
  before any token is produced. Without the artifact closure, consent ends
  at `proof_unavailable` — never a simulated boolean.

## Supply chain

Exact-pinned dependencies with a committed lockfile, SHA-pinned GitHub
Actions, a rev-pinned Midnight ledger dependency enforced by script,
content-addressed Nix builds, cargo-audit/cargo-deny gates, and a checksum-pinned
`arrayref` 0.3.9 archive verified against its reviewed canonical revision after
the 2026-08-20 crates.io publication mismatch. The gate also rejects the
unreviewed `proc-macro1` dependency introduced only by the later registry
publication. A dated, per-advisory exception register lives under
[`docs/security/`](https://github.com/MediaNoxLabs/oxid/tree/integration/docs/security).

## Known limits (the honest part)

- Production composition ships no live chain or SSI capability yet — that is
  the fail-closed design working as intended while custody, transport, and
  proving pass review ([delivery status](status.md)).
- Development custody and simulations are clearly labeled but present in
  development builds; never point them at real value.
- Reporting a vulnerability: see
  [`SECURITY.md`](https://github.com/MediaNoxLabs/oxid/blob/develop/SECURITY.md).
