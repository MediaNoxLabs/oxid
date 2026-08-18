# Building on Oxid

Oxid is an application today, but it is *built* as a toolkit: six
dependency-free capability hexagons, seventeen adapters, and two incoming
surfaces that any Midnight — and later Cardano — project can reuse. This
chapter is the map for developers who want to build identity-first (or
crypto) applications on top of it.

> Status honesty applies here too: reuse what's delivered (see
> [Delivery status](status.md)), and treat everything standalone-labeled as
> development scaffolding, not production capability.

## Three ways to build on Oxid

### 1. Consume the crates (Rust)

The workspace's core crates have **zero external dependencies** and stable,
small port traits — they are designed to be embedded:

| You want | Use | What you implement |
| --- | --- | --- |
| DID model + resolution flows | `identity/domain` + `identity/application` | a resolver port (or reuse `adapters/did-midnight`) |
| Credential records + 7-stage verification reports | `credential/domain` + `credential/application` | storage + verifier ports (or reuse `vc-midnight`, `storage-credential-json`) |
| OpenID4VP / SIOPv2 request handling with consent semantics | `presentation/*`, `protocol/*` | transport adapters (or reuse `openid4vp`/`siopv2` standalone ones) |
| Midnight accounts, sync, staged submission | `wallet/*` + `adapters/midnight` | custody port (or reuse the software/native custody adapters) |
| Bounded, owner-private, atomic persistence patterns | the storage adapters | — copy the discipline even if not the code |

The architecture gate (default-deny allowlist) documents every permitted
edge; your integration should follow the same inward-pointing rule.

### 2. Drive the headless wallet (any language)

`oxid-headless` is a complete wallet behind a **versioned NDJSON protocol**
on stdin/stdout: capability discovery, profiles, custody, accounts, sync,
transfers, DIDs, credentials, issuance, presentation. Any language that can
spawn a process can build on it — test harnesses, backends, CLIs, bots.
Start with [the headless protocol](headless-protocol.md); the capability
manifest tells you truthfully what your build can do.

### 3. Connect an AI agent (MCP — proposed)

The natural next surface: a **Model Context Protocol server** that exposes
the headless wallet's capability manifest as agent tools — read/status/
prepare operations available to agents, authorization and consent kept
human-gated by construction. The proposal, security model, and prototype
live in the repository (see the `oxid-mcp` proposal issue); when it lands,
AI agents get Midnight assets, transactions, and the SSI layer as a safe
tool surface — arguably the first identity-wallet MCP in the Midnight
ecosystem.

## What the Midnight community can use today

- **did:midnight resolution + lifecycle** (standalone writes), with the
  pinned trust-root discipline documented in `docs/migration/`.
- **Compact credential verification**: the exact `midnight_compact_vc`
   18-chunk reconstruction, detached issuance proofs, and the reproducible
  presentation artifact pipeline (`nix build .#presentation-compact-artifacts`).
- **Real ZK presentation**: the k=18 circuit, `MZP1`/`MPS1` envelope
  handling, and the independent-verifier gate — a working reference for
  "never fake a proof" engineering.
- **The staged submission machine**: persist-before-broadcast, ambiguous
  outcomes, finalized-history reconciliation — reusable far beyond wallets.
- **The Passport Vault contract** + canonical-replay verifier: a complete
  worked example of trusting a Midnight contract *without* trusting an
  indexer.

## Where Cardano fits

The blueprint reserves the Cardano vertical (ADR-0014, Proposed) after
Midnight parity. The architecture already anticipates it: `ChainNetworkId`
is chain-neutral, custody is curve-plural (Ed25519/P-256/Jubjub/secp256k1),
and every chain touchpoint is an adapter behind a port. A Cardano
contribution means: one `adapters/cardano` crate implementing the existing
wallet ports, an ADR, and zero changes to domain or application code — that
is the point of the hexagon.

## Contributing a new adapter (the 6-step recipe)

1. Read [Architecture](architecture.md) and the ADRs your capability
   touches; write/extend an ADR if you introduce a boundary.
2. Create `crates/adapters/<name>` and add it to the
   `check-architecture.sh` allowlist (the gate fails your build until the
   dependency edges are declared — by design).
3. Implement the application-owned port; convert every external type at
   your boundary. No secrets in DTOs or logs.
4. Match the storage discipline if you persist anything: bounded, owner-
   private, symlink-rejecting, atomic.
5. Wire it only in `composition`, behind the right mode; production
   composition stays fail-closed until review.
6. Tests: hermetic (loopback fixtures), adversarial negative paths, and —
   for crypto — known-answer vectors. `just check` is the gate.
