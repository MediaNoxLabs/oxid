# Oxid Identity Wallet

Oxid is a **Rust-first, mobile-first wallet in which crypto and self-sovereign
identity are peer capabilities** — one application that holds accounts and
transactions on the [Midnight](https://midnight.network) network alongside
DIDs, verifiable credentials, and privacy-preserving presentations, without
treating either side as a plugin to the other.

> **Status: pre-production.** Oxid is not ready to custody real assets,
> production identity keys, or externally issued credentials. Everything
> demonstrable today runs in an explicit standalone development composition;
> the normal production composition deliberately fails closed until each
> capability passes its review gates. [Delivery status](status.md) tracks
> exactly what works in which mode.

## What makes it different

**Architecture before features.** Oxid is a strict hexagonal (ports and
adapters) workspace of 37 crates. Domain and application crates have *zero*
external dependencies — no UI framework, no chain SDK, no persistence engine,
no HTTP client — and a CI-enforced gate keeps it that way. The
[architecture](architecture.md) chapter shows how the pieces fit.

**Privacy as an invariant, not a promise.** Key material lives behind opaque
references and operation ports; secrets never appear in DTOs, logs, or
fixtures; file stores are bounded, owner-private, and atomic; zero-knowledge
proofs are real Compact circuit executions, never simulated booleans. The
[security model](security-model.md) chapter explains the custody boundaries.

**Honest capability labels.** Every surface — the Dioxus mobile UI and the
NDJSON [headless protocol](headless-protocol.md) — reports what is real,
what is simulated, and what is deliberately unavailable. A state labeled
`deterministic_simulation` or `indexer_supplied_not_proven` is exactly that.

**Decisions on the record.** Over seventy
[architecture decision records](adr-catalog.md) govern the codebase, each
tracking both its binding status and its actual delivery state.

## Where to go next

| You want to… | Read |
| --- | --- |
| Build and run Oxid locally | [Getting started](getting-started.md) |
| Drive the wallet from scripts or tests | [The headless protocol](headless-protocol.md) |
| Know what works today, in which mode | [Delivery status](status.md) |
| Understand the crate layout and rules | [Architecture](architecture.md) |
| Contribute a change | [Contributor quickstart](contributing.md) |
| See how an AI-driven repo stays governed | [How this project is built](agent-process.md) |

Oxid is Apache-2.0 licensed and developed in the open at
[github.com/MediaNoxLabs/oxid](https://github.com/MediaNoxLabs/oxid).
