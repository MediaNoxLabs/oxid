# ADR-0111: Keep testkit in-tree and out of product closures

- Status: Proposed
- Date: 2026-09-14
- Source: ADR-0003–0005, ADR-0020, ADR-0096, and issue #518
- Related: [#518](https://github.com/MediaNoxLabs/oxid/issues/518)
- Implementation state: decision only; no crate or product dependency is created by this ADR

## Context

Oxid repeatedly constructs deterministic wallets, public profiles, fixtures,
mocked ports, health checks, and scenario setup across unit tests, headless
integration tests, mobile smokes, and demos. The duplication slows delivery
and encourages direct use of the shared standalone genesis wallet.

The demo inventory is the repository's machine-readable source for
use-case/scenario relationships. ADR-0096 deliberately drives safe fixtures
through existing typed services rather than a privileged demo service.
ADR-0003 keeps domain types Oxid-owned, ADR-0004 keeps ports
capability-specific, and ADR-0005 keeps composition static. A reusable test
seam must not reverse those directions or become a second product architecture.

This decision does not introduce cold-start, preseed, birthday-gated replay,
or distributable chain-state references. Full-history DUST/Zswap measurement
and optimization remains issue #115. Improvements that belong to the Midnight
Rust ledger or wallet libraries should be implemented upstream and consumed
through reviewed, pinned dependencies rather than recreated as Oxid product
features.

### Current seams and extraction classification

| Current source | Repetition or responsibility | Classification |
| --- | --- | --- |
| [`apps/oxid-headless/tests/capability_contracts/support.rs`](../../apps/oxid-headless/tests/capability_contracts/support.rs) and sibling capability contracts | Repeated `HeadlessWallet::new(compose_in_memory())`, command execution, profile literals, and JSON response decoding | The concrete driver remains headless-local; only lower-level public profile builders and adapter-neutral response assertions are testkit candidates |
| [`apps/oxid-headless/tests/protocol_contract.rs`](../../apps/oxid-headless/tests/protocol_contract.rs) and [`apps/oxid-headless/tests/fixtures`](../../apps/oxid-headless/tests/fixtures/README.md) | Repeated profile identifiers plus small protocol-wire fixtures and decoders | Candidate builders/loaders only when reused by a second seam; reviewed synthetic fixture files remain in-tree |
| [`crates/composition/src/profile_in_memory.rs`](../../crates/composition/src/profile_in_memory.rs) and its tests | Explicit in-memory application composition used by many tests | Product composition seam: tests call the public facade and pass only lower-level public values to helpers; testkit never owns or copies the composition root |
| [`crates/adapters/storage-dev/src/development_fixture.rs`](../../crates/adapters/storage-dev/src/development_fixture.rs) | Shared-root authority bound to one explicitly named development profile | Seam-local security adapter; never move its root seed or authority into testkit |
| Wallet/application and adapter unit-test modules | Fixed clocks, random sources, repositories, and profile literals test private invariants | Remain seam-local until the same public-port fake is demonstrably duplicated without exposing private implementation state |
| [`docs/factory/demo-inventory.json`](../factory/demo-inventory.json) and [`scripts/demo-inventory.mjs`](../../scripts/demo-inventory.mjs) | Product use-case/scenario truth, prerequisites, targets, and evidence classes | Scenario source and validator; testkit may implement a named prerequisite but cannot own this vocabulary |
| Platform drivers such as [`scripts/test-android-profile-flow.sh`](../../scripts/test-android-profile-flow.sh) | Device lifecycle, build/install, native custody, and platform evidence | Scenario/platform script; never a Rust testkit module |

## Decision

Create no implementation in this decision. The proposed future boundary is a
non-published workspace crate at `crates/oxid-testkit` with package name
`oxid-testkit`. It is versioned with the Oxid monorepo, is never published, and
is excluded from normal production feature graphs and release closures.

Small synthetic, deterministic, reviewed, secret-free standalone fixtures
remain in this repository. Do not create an external fixture or replay-state
repository. Do not distribute Preview, PreProd, or Mainnet chain state as an
Oxid testkit concern.

### Dependency direction and responsibilities

`oxid-testkit` may depend only on public, test-oriented APIs of lower-level
Oxid crates plus ordinary test support dependencies approved by workspace
policy. It must not depend on an incoming app (`oxid-headless`, Dioxus, or MCP)
or a concrete composition root. Test binaries, integration tests, headless
tests, platform smoke harnesses, and scenario adapters may depend on it.
Production domain, application, adapter, composition, UI, MCP, and release
artifact crates must not depend on it; Cargo feature and closure checks must
prove that exclusion.

The minimum crate surface is five modules:

- `builders` for deterministic public wallet/profile inputs;
- `ports` for reusable public-port clocks, randomness, and in-memory fakes;
- `fixtures` for bounded typed synthetic loaders;
- `scenario` for payload-free prerequisites and health probes; and
- `assertions` for shared public-output checks.

None may expose a seed-returning API. A helper stays seam-local when it encodes
an incoming application type, concrete composition root, platform lifecycle,
UI rendering, a single adapter protocol, live-network timing, credentials,
custody, authorization, proving, submission, or an otherwise unique test.
Extraction requires demonstrated duplication and a focused contract test. The
first slice extracts one public profile builder plus adapter-neutral assertions
used by both composition and headless integration tests. Command execution and
construction of `HeadlessWallet` remain local to headless tests.

The crate owns deterministic test vocabulary and synthetic construction.
Product crates own domain invariants, capability ports, custody,
authorization, synchronization, composition, and production error semantics.
The demo inventory owns scenario-to-use-case truth; testkit may implement a
scenario prerequisite but cannot redefine a scenario or claim platform
evidence.

### Trust and secrets

Testkit fixtures may contain only reviewed synthetic public values. They must
contain no seed, mnemonic, private key, protected handle, credential private
material, owner checkpoint, endpoint credential, personal device/tailnet
identity, captured chain-state snapshot, or replay binary. Logs and assertion
failures remain payload-safe.

### Rollout and evidence

1. Keep current seam-local helpers and inventory intact; document duplicate
   candidates with source paths and focused tests.
2. Add the private workspace crate with one public profile builder and
   adapter-neutral assertion seam shared by composition and headless
   integration tests.
3. Keep the concrete headless driver local, and add dependency-closure and
   release-exclusion tests plus hermetic consumer equivalence.
4. Add more helpers only after a second real consumer proves duplication.
5. Add platform-specific consumers only with their existing simulator/device
   evidence; no all-platform scenario becomes a universal pull-request gate.

## Consequences

This decision makes shared test capability reusable without
production-to-test dependency cycles or a second composition architecture. It
keeps small fixtures reviewable in-tree and removes chain-state distribution,
signing, retention, revocation, and freshness governance from the testkit.

DUST synchronization performance is not solved by a wallet-owned snapshot or
startup shortcut. Oxid measures its real adapter behavior and prefers upstream
Rust ledger improvements that benefit every consumer.

## Alternatives rejected

- **Publish `oxid-testkit` or let product crates depend on it:** couples the
  production closure to test scaffolding and weakens ADR-0003–0005.
- **Move all fixtures to an external repository:** loses reviewable small
  synthetic fixtures and adds release/process overhead without a need.
- **Store chain state or replay binaries in Git, releases, or ordinary CI
  caches:** creates a second synchronization product with provenance,
  compatibility, privacy, and lifecycle risk.
- **Add a birthday/preseed startup shortcut in Oxid:** duplicates concerns
  better addressed by optimizing the Midnight Rust synchronization stack.
- **Move every helper immediately:** creates a testkit god-crate without proven
  reuse.

## Research sources

- Oxid ADR-0003, ADR-0004, ADR-0005, ADR-0020, ADR-0096, and issue #518.
- Current Oxid test, fixture, scenario, and composition seams linked above.
