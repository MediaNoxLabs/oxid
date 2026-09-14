# ADR-0111: Keep testkit in-tree and reserve public replay references

- Status: Proposed
- Date: 2026-09-14
- Source: ADR-0003–0005, ADR-0020, ADR-0030–0033, ADR-0096, ADR-0098, issue #518
- Related: [#115](https://github.com/MediaNoxLabs/oxid/issues/115), [#116](https://github.com/MediaNoxLabs/oxid/issues/116), and [#518](https://github.com/MediaNoxLabs/oxid/issues/518)
- Implementation state: decision only; no crate, artifact repository, or replay-reference consumer is created by this ADR

## Context

Oxid has repeatable standalone setup across Rust tests, headless journeys, mobile smokes, and the opt-in demo drawer. The demo inventory is the repository's machine-readable source for use-case/scenario relationships, while ADR-0096 deliberately drives safe fixtures through existing typed services rather than a privileged demo service. ADR-0003 keeps domain types Oxid-owned, ADR-0004 keeps ports capability-specific, and ADR-0005 keeps composition static; a reusable test seam must not reverse those directions.

Cold wallet synchronization is different from test setup. ADR-0030–0033 keep public and adapter-private checkpoints separate and require live catch-up before cached state becomes spendable. Issue #116 owns whether authenticated replay references can safely accelerate fresh synchronization. Moth Wallet ADRs 0003, 0004, and 0005 are design evidence for its test and replay boundaries, not code or data to copy.

### Current seams and extraction classification

| Current source | Repetition or responsibility | Classification |
| --- | --- | --- |
| [`apps/oxid-headless/tests/capability_contracts/support.rs`](../../apps/oxid-headless/tests/capability_contracts/support.rs) and sibling capability contracts | Repeated `HeadlessWallet::new(compose_in_memory())`, command execution, profile literals, and JSON response decoding | The concrete driver remains headless-local; only lower-level public profile builders and adapter-neutral response assertions are testkit candidates |
| [`apps/oxid-headless/tests/protocol_contract.rs`](../../apps/oxid-headless/tests/protocol_contract.rs) and [`apps/oxid-headless/tests/fixtures`](../../apps/oxid-headless/tests/fixtures/README.md) | Repeated profile identifiers plus small protocol-wire fixtures and decoders | Candidate builders/loaders only when reused by a second seam; the reviewed synthetic fixture files remain in-tree |
| [`crates/composition/src/profile_in_memory.rs`](../../crates/composition/src/profile_in_memory.rs) and its tests | The explicit in-memory application composition used by many tests | Product composition seam: testkit may call its public façade but must not replace, own, or copy the composition root |
| [`crates/adapters/storage-dev/src/development_fixture.rs`](../../crates/adapters/storage-dev/src/development_fixture.rs) | Shared-root authority bound to one explicitly named development profile | Seam-local security adapter; never move its root seed or authority into testkit |
| Wallet/application and adapter unit-test modules, for example [`crates/wallet/application/src/lib.rs`](../../crates/wallet/application/src/lib.rs) | Multiple fixed clocks, random sources, repositories, and profile literals test private invariants | Remain seam-local until the same public-port fake is demonstrably duplicated without exposing private implementation state |
| [`docs/factory/demo-inventory.json`](../factory/demo-inventory.json) and [`scripts/demo-inventory.mjs`](../../scripts/demo-inventory.mjs) | Product use-case/scenario truth, prerequisites, targets, and evidence classes | Scenario source and validator; testkit may implement a named prerequisite but cannot own this vocabulary |
| Platform drivers such as [`scripts/test-android-profile-flow.sh`](../../scripts/test-android-profile-flow.sh) | Device lifecycle, build/install, native custody, and platform-specific evidence | Scenario/platform script; never a Rust testkit module |
| Preview/PreProd replay history owned by issues #115/#116 | Large, refreshed public network state with an independent trust lifecycle | External release artifact concern; never a fixture, Git-history object, or CI cache entry |

## Decision

Create no implementation in this decision. The proposed future boundary is a non-published workspace crate at `crates/oxid-testkit` with package name `oxid-testkit`. It is versioned with the Oxid monorepo, is never published, and is excluded from normal production feature graphs and release closures.

Small synthetic, deterministic, reviewed, secret-free standalone fixtures remain in this repository. Do not create an external fixture repository. If and only if issue #116 proves its compatibility and safety conditions, `MediaNoxLabs/oxid-replay-references` is reserved solely for signed public Preview/PreProd replay-reference release artifacts. It is not created by this ADR, must not contain Git-history binaries or mutable CI cache state, and Mainnet remains excluded pending a separate security and governance decision.

### Dependency direction and responsibilities

`oxid-testkit` may depend only on public, test-oriented APIs of lower-level Oxid crates plus ordinary test support dependencies approved by workspace policy. It must not depend on an Oxid incoming app (`oxid-headless`, Dioxus, or MCP) or on a concrete composition root. Test binaries, integration tests, headless tests, platform smoke harnesses, and scenario adapters may depend on it. Production domain, application, adapter, composition, UI, MCP, and release artifact crates must not depend on it; Cargo feature/closure checks must prove that exclusion. It must not become a composition root, a service locator, or a replacement application façade.

The minimum crate surface is five modules: `builders` for deterministic public wallet/profile inputs, `ports` for reusable public-port clocks/randomness and in-memory fakes, `fixtures` for bounded typed synthetic loaders, `scenario` for payload-free prerequisites/health probes, and `assertions` for shared public-output checks. None may expose a seed-returning API. A helper stays seam-local when it encodes an incoming application type, concrete composition root, platform lifecycle, UI rendering, a single adapter protocol, live-network timing, credentials, custody, authorization, proving, submission, or an otherwise unique test. Extraction requires demonstrated duplication and a focused contract test; the first slice extracts one public profile builder plus adapter-neutral assertions used by both composition and headless integration tests. Command execution and construction of `HeadlessWallet` remain local to the headless tests.

The crate owns deterministic test vocabulary and synthetic construction. Product crates own domain invariants, capability ports, custody, authorization, replay, composition, and production error semantics. The demo inventory owns scenario-to-use-case truth; testkit may implement a scenario prerequisite but cannot redefine a scenario or claim platform evidence. Issue #116 owns replay-reference authentication and consumer semantics; the future artifact repository owns signed release distribution only.

### Trust, secrets, and replay inputs

Testkit fixtures may contain only reviewed synthetic public values. They must contain no seed, mnemonic, private key, protected handle, credential private material, owner checkpoint, endpoint credential, personal device/tailnet identity, or replay binary. Logs and assertion failures remain payload-safe.

A future replay reference is an authenticated public input, not a wallet fixture, backup, checkpoint, or authority. Its release manifest must bind schema/version, network identifier and genesis, reference range and birthday eligibility boundary, source/indexer cursor witnesses, uncompressed and compressed sizes, content digest, producer revision/build provenance, creation/expiry/refresh metadata, and a signature/key identity plus revocation location. Consumers pin an allow-list of release signers in Oxid policy, or accept a signer only through an unexpired delegation authenticated by the independent revocation root; the manifest cannot authorize its own key. Signer addition, removal, and rotation require a root-authenticated delegation with an explicit validity interval and monotonic sequence, and conflicting or rolled-back delegations fail closed. Consumers must then verify the authorized signature, manifest identity, network/genesis, bounds, digest, bounded decompression, and cursor witnesses before use. They must also verify the artifact attestation against the trusted artifact repository, publication workflow identity, immutable release reference, and downloaded subject digest, then bind its producer revision to the manifest. An invalid, revoked, expired, incompatible, unavailable, or stale reference is rejected rather than repaired.

The future artifact repository owns generation, signing, releases, retention, and revocation; Oxid owns strict download/verification/fallback behavior. Release tags use `<network>-<YYYY>-W<ww>-r<n>` and manifests carry a separate schema version, so data refreshes do not imply an Oxid crate release. A scheduled build may publish at most weekly and only when the source finalized range advanced; on-demand release/demo refresh is allowed through the same reviewed workflow. Retain at least the latest eight valid weekly releases per network plus every release referenced by a supported Oxid version. Assets are immutable: correction creates a higher `r<n>`. Consumers pin an offline revocation-root identity that is independent of the online release signer; only a revocation record authenticated by that root disables an artifact digest or release-signing key and forces genesis replay. Rotating the revocation root is an owner-approved Oxid security-policy change distributed in a new application release; ambiguous, missing, or conflicting rotation evidence fails closed. The publication workflow must produce a GitHub artifact attestation and a manifest signature from the separately governed release identity; neither ordinary pull-request CI nor the testkit may publish.

References may accelerate only a wallet proved fresh relative to the declared birthday boundary. Import/recovery, ambiguous birthday, an existing profile, or any evidence that prior wallet history may exist requires the normal full replay. Even an accepted reference never removes node/indexer validation, authenticated compatibility checks, or the ADR-0030–0033 live-before-spend catch-up gate. A valid unexpired reference may be behind the current tip and therefore add live catch-up time. `Stale` means its signed expiry or maximum-age policy has been exceeded; a stale reference is rejected and falls back to genesis replay rather than weakening those gates.

### Rollout and evidence

1. Keep current seam-local helpers and inventory intact; document duplicate candidates with source paths and focused tests.
2. Add the private workspace crate with one public profile builder and adapter-neutral assertion seam shared by composition and headless integration tests. Keep the concrete headless driver local, and add dependency-closure/release-exclusion tests plus hermetic consumer equivalence.
3. Add platform-specific consumers only with their existing simulator/device evidence; no all-platform scenario becomes a universal PR gate.
4. Issue #116 independently proves Preview/PreProd replay compatibility, manifest verification, birthday/import fallback, witness handling, bounded decompression, revocation, and fresh-wallet live-before-spend equivalence.
5. Only then create the reserved repository and publish immutable signed release artifacts with provenance and retention/revocation policy. A compromise, missing provenance, or revoked key stops consumption and falls back to full replay; artifact replacement is not permitted.

## Consequences

This decision makes shared test capability reusable without production-to-test dependency cycles or a second composition architecture. It preserves in-tree review for small fixtures and defers external operational ownership until replay compatibility is demonstrated. Preview and PreProd artifact distribution stays explicitly public, signed, release-based, and revocable; Mainnet is deliberately outside this decision.

## Alternatives rejected

- **Publish `oxid-testkit` or let product crates depend on it:** couples production closure to test scaffolding and weakens ADR-0003–0005 boundaries.
- **Move all fixtures to an external repository:** loses reviewable small synthetic fixtures and adds release/process overhead before a need exists.
- **Store replay binaries in this repository or ordinary CI caches:** creates mutable or history-bound artifact distribution with poor revocation and provenance controls.
- **Treat a replay reference as a restore/checkpoint shortcut:** could hide imported-wallet history and violates the existing live-before-spend model.
- **Admit Mainnet now:** requires separate security, signing, retention, incident-response, and governance approval.

## Research sources

- Oxid: ADR-0003, ADR-0004, ADR-0005, ADR-0020, ADR-0030–0033, ADR-0096, ADR-0098; `docs/factory/demo-inventory.md`; issues #115, #116, and #518.
- Moth Wallet: [`docs/adr/0003-preseed-reference.md`](https://github.com/shieldedtech/moth-wallet/blob/main/docs/adr/0003-preseed-reference.md), [`docs/adr/0004-preseed-distribution.md`](https://github.com/shieldedtech/moth-wallet/blob/main/docs/adr/0004-preseed-distribution.md), and [`docs/adr/0005-preseed-for-cli-tui.md`](https://github.com/shieldedtech/moth-wallet/blob/main/docs/adr/0005-preseed-for-cli-tui.md), inspected as design evidence only.
