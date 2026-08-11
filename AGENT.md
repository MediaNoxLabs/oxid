# AGENT

Engineering guide for agents and contributors working in `oxid`.

This repository is the public, standalone home of the Oxid identity wallet. The
root `OXID_IDENTITY_WALLET_BLUEPRINT.md` is the product and architecture north
star. When this guide and the blueprint differ, preserve the blueprint's
dependency and security rules and update this file in the same change.

## Purpose and current phase

Oxid is a Rust-first, mobile-first wallet in which crypto and self-sovereign
identity are peer capabilities. Dioxus is an incoming UI adapter; it is not the
application architecture.

The current milestone is M0. Implement only the smallest complete vertical
slice needed to prove the architecture before adding Cardano, Midnight, DID,
VC, OIDC, or DIDComm SDKs:

1. foundation primitives;
2. wallet domain;
3. wallet application/use cases and outgoing ports;
4. platform ports;
5. in-memory and system adapters;
6. Dioxus UI adapter;
7. composition root.

The first and only M0 use case is **Create Wallet Profile**.

## Prototype provenance

The prototype remains useful migration input, not an architecture template.
The reviewed baseline is:

- repository: `midnight-ledger`;
- branch: `feat/mobile-prototype`;
- commit: `074b1a4bccbfee1740ee188374b606a022ecef42` (2026-07-02);
- source area: `mobile-bench/`, especially `wallet-core/`,
  `dioxus-wallet/`, and `headless-wallet/`.

That commit declares itself the successor to the earlier Dioxus/VC prototype
branches. Record a new immutable commit here before taking later prototype
changes. Do not copy ledger-relative path dependencies, demo secrets, generated
proof artifacts, pre-production keys, vendored JS, or environment-specific
mobile projects into Oxid without an explicit migration decision.

The staged component inventory and destination map live in
`docs/migration/midnight-ledger-prototype.md`.

## Architecture boundaries

Dependencies point inward:

```text
apps/composition -> incoming UI + outgoing adapters -> application -> domain
                                                    -> platform ports -> foundation
```

Rules:

- Domain and application crates must not depend on Dioxus, chain/SSI SDKs,
  persistence engines, HTTP clients, OS APIs, or JavaScript/WASM libraries.
- Oxid owns all public core types. Convert external types at adapter boundaries.
- Put incoming use-case traits and outgoing capability ports in the application
  boundary that owns them; prefer small traits over aggregate service objects.
- Dioxus renders state and emits application commands. It never calls storage,
  chain, SSI, or platform SDKs directly.
- Private key material, seeds, credential claims, and recovery data must not
  appear in ordinary UI/application DTOs, logs, fixtures, or committed config.
- Key use is expressed through opaque references and key-operation ports.
- Use static Cargo composition for the MVP. Runtime native plugin loading is out
  of scope.
- Add an ADR for architectural changes. Do not silently reverse an accepted ADR.
- Keep the core independently testable without UI, network, or OS services.

The blueprint's ADR summaries are materialized as ADR-0001 through ADR-0020 in
`docs/adr/README.md`. ADR-0021 records the staged prototype migration and
ADR-0022 records Nix as the reproducible environment. ADR status and delivery
state are deliberately separate: an accepted future boundary is binding but
does not mean the capability is implemented. Proposed ADRs are gates, not
dependency authorization.

Current package ownership:

| Path | Responsibility |
| --- | --- |
| `crates/foundation` | Small dependency-free primitives shared across core boundaries. |
| `crates/wallet/domain` | Wallet profile invariants and entities. |
| `crates/wallet/application` | Incoming use cases and owned outgoing repository ports. |
| `crates/platform/ports` | Clock and randomness capabilities used by applications. |
| `crates/adapters/storage-memory` | Development/test implementation of wallet persistence. |
| `crates/adapters/platform-system` | System clock and OS randomness implementations. |
| `crates/ui-dioxus` | Dioxus incoming adapter and presentation state. |
| `crates/composition` | Concrete dependency wiring with no product rules. |
| `apps/oxid` | Executable shell and platform launch point. |

## Development environment

Nix is the supported environment and the flake lock is authoritative:

```bash
nix develop
```

Direnv users can run `direnv allow`. The shell provides Rust, Cargo tooling,
`dx`, `just`, Node.js, and the pinned project-local Pi packages from
`.pi/settings.json`.

Fast validation:

```bash
./run.sh --light --strict
```

Full local validation:

```bash
./run.sh --strict
```

Useful focused commands:

```bash
cargo test -p oxid-wallet-domain
cargo test -p oxid-wallet-application
cargo test -p oxid-adapter-storage-memory
cargo check -p oxid-app
./run.sh coverage --strict
./scripts/check-architecture.sh
```

Run repository commands from `nix develop` unless CI performs the equivalent
setup. Keep `Cargo.lock` committed and use workspace dependencies rather than
duplicating versions across manifests.

## Development cycle

1. Start from the current remote base requested for the work. Normal feature
   work integrates into `develop`; `main` is the release branch.
2. Use a dedicated worktree. Do not implement in a dirty primary checkout.
3. Read this file and the blueprint before changing code.
4. Change tests and public documentation with behavior.
5. Run focused checks first, then `./run.sh --light --strict`.
6. Run `npx dev-loops@0.9.0 doctor` and `npx dev-loops@0.9.0 gates`
   before a PR loop. Configuration failures are blockers.
7. Create pull requests as drafts. Do not mark them ready until validation and
   review evidence are recorded.
8. Keep the worktree clean. Never delete unrelated user files or changes.
9. Commit repository-facing work with DCO and GPG:

   ```bash
   git commit -S --signoff -m "<type>: <subject>"
   ```

10. Before pushing, verify both the signature and trailer:

    ```bash
    git log -1 --show-signature --pretty=fuller
    ```

Use conventional commit and PR titles such as `feat(wallet): create profiles`
or `ci: add Rust quality gates`.

`dev-loops@0.9.0 doctor` currently reports 3/4 from a plain shell because it
looks for a standalone `subagent` executable. `pi-subagents@0.42.1` exposes
`subagent` as an in-process Pi tool instead. Confirm that the pinned package is
installed and `dev-loops gates` parses successfully; do not add a dummy binary
to silence the shell probe.

## Validation and coverage

- `cargo fmt --all --check` is mandatory.
- Clippy runs workspace-wide with warnings denied.
- Unit and integration tests run workspace-wide.
- `cargo llvm-cov` enforces 80% line coverage across the core and outgoing
  adapters; the Dioxus UI and executable shell are excluded from this core
  threshold and remain compile-gated.
- `scripts/check-architecture.sh` enforces the initial inward dependency graph.
- Security and dependency-policy checks remain distinct from test coverage.
- The Dioxus desktop graph's bounded RustSec exceptions are documented in
  `docs/security/advisory-exceptions.md`; review them on every Dioxus/Wry update
  and before production custody work.
- A green aggregate must not hide a skipped core, architecture, security, or UI
  compile lane.
- Coverage thresholds are enforced locally and in CI; hosted reporting may
  visualize results but must never decide whether the gate passes.

## Security and privacy

- Telemetry is off by default. New telemetry requires an ADR and explicit user
  opt-in.
- Never log secrets, seeds, private identifiers, credential claims, signing
  payloads, or raw external error bodies that may contain them.
- Validate profile labels and all future QR/deep-link/protocol input at the
  boundary before use.
- Keep production secret storage behind platform-backed adapters. The in-memory
  adapter is development/test infrastructure and must never be presented as
  durable or secure storage.
- Use opaque key references. Key-generation and signing ports must not return
  raw private keys to application or UI layers.
- Record every significant dependency using the review template in the
  blueprint before an adapter becomes production-facing.
- Report vulnerabilities through GitHub private vulnerability reporting, not a
  public issue.

## Public repository hygiene

- Keep documentation and automation public-safe: no tokens, private tracker
  links, private infrastructure names, personal machine paths, or unredacted
  diagnostic output.
- New source files should use an SPDX Apache-2.0 header where practical.
- Pin GitHub Actions to immutable commit SHAs.
- Keep least-privilege workflow permissions and disable persisted checkout
  credentials.
- Do not commit generated `target/`, Dioxus build output, mobile signing data,
  local databases, `.env` files, Pi package installs, or editor state.
- Preserve third-party licenses and provenance when code or assets are migrated.

## Maintaining this guide

Update `AGENT.md` whenever a session establishes a durable fact that a later
engineer would otherwise have to rediscover: selected source commits, accepted
boundaries, non-obvious validation commands, migration decisions, or known
toolchain constraints. Do not use it as a chronological work log or store
ephemeral status that belongs in an issue or PR.
