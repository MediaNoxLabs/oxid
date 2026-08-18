# Architecture

Oxid is a strict hexagonal (ports and adapters) workspace. Dependencies point
inward, and the direction is machine-enforced on every push — not a diagram
aspiration.

```text
apps -> incoming adapters -> application -> domain
   +-> composition -> outgoing adapters -> platform ports -> foundation
```

## The layers

| Layer | Crates | Rules |
| --- | --- | --- |
| Foundation | `foundation` | Small dependency-free primitives. |
| Domain | `wallet`, `identity`, `credential`, `presentation`, `protocol`, `passport-vault` (each `…/domain`) | Invariants and entities. **Zero external dependencies.** |
| Application | the matching `…/application` crates | Use cases, incoming traits, and owned outgoing ports. **Zero external dependencies.** |
| Platform ports | `platform/ports` | OS capability traits (clock, randomness, QR, export). |
| Adapters | `adapters/*` (17 crates) | Chain, SSI protocol, storage, custody, mobile-native. External types are converted at this boundary and never leak inward. |
| Composition | `composition` | The only place adapters meet ports. Selects fail-closed production or explicit standalone wiring. |
| Apps | `apps/oxid` (Dioxus), `apps/oxid-headless` (NDJSON) | Incoming shells; render state and emit commands. |

Each business capability is its own hexagon with a domain/application pair, so
`credential` logic cannot reach into `wallet` internals — cross-context
collaboration goes through ports wired in composition.

## How the rules are enforced

- `scripts/check-architecture.sh` validates a per-crate dependency allowlist
  over the entire workspace against `cargo metadata`, with a default-deny
  sweep: a crate without an allowlist entry fails the gate.
- The 14 core crates (foundation, domains, applications, platform-ports) are
  additionally checked to have **no external dependencies of any kind** —
  the application layer hand-rolls its boxed-future aliases rather than pull
  `async-trait`.
- `unsafe` is denied workspace-wide, with a single reviewed exception (the
  Android profile-path JNI boundary) pinned by the gate.
- The gate runs inside `./run.sh --light --strict`, which is both the local
  `just check` and the CI repository gate — local and CI enforcement are the
  same script by construction.

## Ports, in Oxid's dialect

- **Incoming use cases** are single-method traits (`…UseCase`) owned by the
  application layer; the UI and headless adapters depend on those traits
  only.
- **Outgoing ports** (`…Port`, repositories, sources) are small and
  consumer-owned; adapters implement them.
- **Key material never crosses a port.** Custody operations lend secrets to
  callbacks or return opaque references — a port's type signature makes
  secret exfiltration unrepresentable rather than merely forbidden.

## Composition is a decision, not plumbing

`compose()` — the normal production path — wires *unavailable* adapters for
every capability that has not passed its review gate, and tests assert this
fail-closed behavior. Explicit compositions
(`compose_headless_from_environment`, the mobile standalone development
features) opt into development custody, deterministic simulations, and live
standalone transports. Feature guards (`compile_error!`) prevent
contradictory combinations from compiling at all.

For the reasoning behind these boundaries, the
[decision records](adr-catalog.md) are the authoritative log — start with
the blueprint constitution
([`OXID_IDENTITY_WALLET_BLUEPRINT.md`](https://github.com/MediaNoxLabs/oxid/blob/develop/OXID_IDENTITY_WALLET_BLUEPRINT.md))
and the ADR index.
