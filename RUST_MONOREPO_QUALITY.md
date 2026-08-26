# Rust Monorepo Quality Attributes

## 1. Purpose

This document defines measurable quality attributes and engineering guardrails for a Rust monorepo targeting mobile (iOS/Android) and headless consumers (CLI, daemon/service, native library, test harnesses, and optionally WASM/UniFFI).

The goal is to keep the repository maintainable for human and AI-assisted development while preserving portability, security, fast feedback, and architectural integrity.

**MUST**, **SHOULD**, and **MAY** indicate mandatory requirements, preferred defaults, and permitted choices.

## 2. Core Quality Attributes

The repository SHOULD optimize for:

- **Maintainability** — cohesive, understandable, reasonably sized code.
- **Modularity** — crate/module boundaries reflect architectural capabilities.
- **Portability** — domain logic is independent of mobile/platform implementations.
- **Testability** — core behavior is testable without devices or external services.
- **Reliability** — deterministic tests and explicit error handling.
- **Security** — strict controls for unsafe code, secrets, crypto, and dependencies.
- **Buildability** — bounded local and CI feedback time.
- **Documentation** — public APIs, architecture, invariants, and decisions are documented.
- **Compatibility** — supported Rust versions, targets, schemas, and APIs are explicit.
- **Reproducibility** — builds and dependency resolution are predictable.
- **Observability** — useful diagnostics without leaking secrets or PII.

## 3. Source Code Size and Complexity

### Rust files

| File size | Policy |
|---|---|
| `< 400 LOC` | Preferred |
| `400–600 LOC` | Acceptable |
| `600–1,000 LOC` | SHOULD be reviewed for decomposition |
| `> 1,000 LOC` | MUST have architectural justification or be decomposed |
| `> 2,000 LOC` | MUST NOT normally be accepted |

Generated code, generated bindings, protocol fixtures, test vectors, and large static lookup tables MAY be excluded.

LOC is an architectural signal, not a formatting target. Code MUST NOT be artificially split solely to satisfy a metric.

### Functions

Functions SHOULD normally remain below **50 LOC**.

- `50–80 LOC`: acceptable when cohesive.
- `80–100 LOC`: SHOULD be reviewed.
- `>100 LOC`: SHOULD normally be decomposed.

Algorithms MAY exceed these limits when decomposition would reduce clarity.

### Complexity

Suggested cyclomatic complexity policy:

```text
target       <= 10
warning      > 15
review       > 20
```

Complexity SHOULD take precedence over raw LOC when evaluating maintainability.

### Cohesion and naming

Catch-all modules SHOULD be avoided:

```text
utils.rs
helpers.rs
common.rs
misc.rs
manager.rs
service.rs
```

Prefer responsibility-oriented names:

```text
credential_verifier.rs
did_resolver.rs
key_repository.rs
presentation_builder.rs
issuance_session.rs
```

## 4. Crate Composition

A crate SHOULD represent an independently meaningful architectural capability. A new crate MUST NOT be introduced merely to reduce file size.

A representative layout:

```text
crates/
  core/
  crypto/
  identity/
  did/
  credentials/
  didcomm/
  oid4vc/
  storage/
  networking/

  adapters/
    sqlite/
    keychain/
    android-keystore/
    secure-enclave/
    http/

  bindings/
    uniffi/
    wasm/

apps/
  mobile/
  cli/
  daemon/
```

A mature repository will commonly contain roughly **10–30 meaningful first-party crates**, but this is a guideline, not a quota.

Typical capability crates SHOULD remain approximately **2k–10k LOC**. Crates above roughly **20k–30k LOC** SHOULD trigger an architectural decomposition review.

A large cohesive crate is preferable to many artificial micro-crates.

## 5. Dependency Architecture

The intended direction is:

```text
apps / platform entry points
            |
            v
      bindings / facade
            |
            v
        application
            |
            v
           core
       /     |      \
 identity  crypto  protocols
            ^
            |
        ports/traits
            ^
            |
         adapters
```

Conceptually:

```text
domain      -> std + foundational domain/crypto types
application -> domain + ports
adapters    -> domain/application + external implementations
bindings    -> stable application/public facade
apps        -> bindings/application + platform-specific code
```

Dependencies MUST follow the declared architecture rather than implementation convenience.

### Platform independence

Core/domain crates MUST NOT directly depend on:

- iOS APIs;
- Android APIs;
- Dioxus or another UI framework;
- UniFFI;
- platform keychain/keystore implementations;
- database implementations;
- concrete HTTP clients;
- mobile lifecycle APIs.

Platform capabilities SHOULD be exposed through ports or traits.

> Core business/domain logic MUST compile and be testable independently of mobile or other platform adapters.

Conditional compilation SHOULD remain near adapter/platform boundaries rather than being scattered throughout domain code.

### Public facade

Where appropriate, expose a stable facade crate such as:

```text
oxid-sdk
```

Internal crates such as `oxid-core`, `oxid-crypto`, `oxid-did`, `oxid-vc`, and `oxid-storage` SHOULD NOT automatically become part of the external API.

Public visibility SHOULD be minimized. Breaking public API changes MUST be explicitly reviewed.

## 6. Testing and Coverage

Coverage MUST NOT be treated as the sole measure of test quality.

| Scope | Target |
|---|---:|
| Entire workspace | `>= 80%` |
| Core/domain crates | `>= 85%` |
| Security/crypto-critical code | `>= 90%` |
| New/changed production code | `>= 90%` |
| Bindings/platform glue | `>= 70–80%` |

Generated code SHOULD be excluded. Workspace coverage below **75%** SHOULD fail CI unless an explicit exception exists.

The repository SHOULD use appropriate combinations of:

- unit tests;
- property-based tests;
- integration tests;
- contract tests;
- serialization compatibility tests;
- cross-platform tests;
- binding tests;
- end-to-end tests;
- fuzz tests.

Property-based tests are strongly recommended for invariants such as:

```text
decode(encode(value)) == value
deserialize(serialize(document)) == document
verify(sign(message)) == true
```

Parsers, protocol messages, crypto input handling, and untrusted serialized data SHOULD be fuzz tested where practical.

Tests MUST NOT unnecessarily depend on execution order, wall-clock timing, public network services, shared global state, or developer-specific configuration.

Flaky tests SHOULD be treated as defects. Retries MUST NOT hide known flaky tests.

### 6.1 Layered wallet end-to-end strategy

Wallet journeys MUST be validated through a promotion ladder. Expensive or hardware-dependent tests MUST NOT be the first place that protocol, state-machine, persistence, or navigation defects are discovered.

```text
Level 1: host-native headless journey
    -> Level 2: headless native app in Android/iOS emulator
        -> Level 3: UI/UX journey in Android/iOS emulator
            -> Level 4: final acceptance on a physical device
```

A higher level MUST run only after the required lower levels pass at the same source head and with the same profile, protocol fixtures, and journey contract. A lower-level failure stops promotion. Passing a higher level MUST NOT be used to waive a lower-level failure.

#### Level 1 — host-native headless wallet journey

The primary E2E feedback loop MUST be a wallet CLI, scenario runner, or equivalent headless composition built for the current host target. It SHOULD exercise the real domain, protocol, cryptographic verification, persistence, custody, and composition boundaries without requiring Android, iOS, a camera, or a connected device.

Level 1 MUST cover, where applicable:

- wallet/profile creation and activation;
- identity creation, selection, and duplicate handling;
- offer/request admission and explicit consent or refusal;
- successful issuance/presentation and relevant negative protocol paths;
- encrypted persistence;
- process termination and process-2 restoration;
- custody reactivation;
- listing and fresh application-backed reverification;
- exact cleanup and replay rejection.

A fixed-size desktop wallet shell MAY complement the CLI by rendering the same UI state machine at representative mobile viewports. It is useful for rapid visual and accessibility feedback, but it MUST NOT be represented as proof of Android/iOS platform integration.

Protocol and security logic SHOULD remain real at this level. Cameras, deep links, secure enclaves, mobile key stores, and external transports MAY be replaced only at explicit adapter boundaries. Offers, grants, tokens, credentials, keys, proofs, and capabilities MUST still use private in-process, file-descriptor, private-file, FIFO, or platform-test handoffs rather than command-line arguments or public logs.

#### Level 2 — headless Android/iOS emulator journey

After Level 1 passes, the same canonical scenarios MUST run in Android Emulator and iOS Simulator without depending on visual pixel interaction. Platform instrumentation, native test APIs, or a private automation bridge MAY drive the app.

Level 2 validates the native packaging and lifecycle seams that the host runner cannot prove:

- target-specific compilation and feature selection;
- app startup, backgrounding, termination, and cold restart;
- native storage and key-protection adapters;
- app/universal links and custom-scheme ingress;
- network-security and TLS policy;
- platform permission denial and recovery;
- process isolation and private capability handoff;
- emulator/simulator cleanup and repeatability.

Camera, QR, biometric, push-notification, and similar capabilities SHOULD be mocked at the operating-system or platform-adapter boundary. The mock MUST deliver the same typed input that the real adapter produces; it MUST NOT bypass the wallet reducer, protocol router, consent screen, verification, or persistence path.

Android and iOS emulator lanes SHOULD share one journey specification and evidence schema. Platform-specific assertions MAY differ, but acceptance semantics MUST remain equivalent.

#### Level 3 — Android/iOS emulator UI/UX journey

After Level 2 passes, UI automation MUST exercise the journeys as a user would experience them in Android Emulator and iOS Simulator. This is the normal final E2E gate for pull requests that change wallet journeys or mobile composition.

Level 3 SHOULD validate:

- visible navigation and retained-review routing;
- actionable loading, empty, failure, uncertain-outcome, and terminal states;
- explicit consent and refusal;
- focus order, labels, touch targets, keyboard behavior, and screen-reader semantics;
- fixed mobile viewports, rotation, text scaling, safe areas, and dark/light presentation where relevant;
- no raw offer, token, credential, DID, proof, key, or capability in screenshots, accessibility trees, diagnostics, or automation output;
- restart and restoration through visible UI;
- mocked camera/QR or other capability entry through the real platform-facing adapter.

Screenshot or snapshot comparison MAY assist review, but semantic assertions and authoritative application state MUST remain the acceptance source. A screenshot alone is not E2E evidence.

#### Level 4 — physical-device acceptance

Physical Android/iOS testing is the last confidence level, not the default development loop. It MUST run only after Levels 1–3 required for that journey are green at the exact same head.

Physical-device tests SHOULD be reserved for behavior that emulators cannot establish with sufficient confidence, including:

- real camera scan and operating-system link dispatch;
- hardware-backed key or custody behavior;
- real application process death and protected storage restoration;
- device TLS, VPN/tailnet, and network-security behavior;
- performance, resource, lifecycle, and OEM-specific behavior;
- final issuance/presentation, encrypted persistence, restart, listing, and fresh reverification.

A connected development phone MAY remain attached for convenient opt-in acceptance runs, but required PR CI and ordinary contributor workflows MUST NOT depend on that phone. The harness MUST discover an eligible device at runtime or accept an operator-private selector, require exactly one unambiguous target, and fail closed on emulators or unexpected devices. Device serials, developer hostnames, personal tailnet names, addresses, and account identifiers MUST NOT be committed or retained in public evidence.

Hardware absence SHOULD mark only the explicitly hardware-required lane as unavailable; it MUST NOT invalidate successful lower-level evidence. Conversely, a release or PR that claims physical-device acceptance remains blocked until the required hardware lane passes. Retries MUST NOT convert an unexplained physical-device failure into a pass.

#### Canonical journeys and promotion evidence

Each wallet use case SHOULD have one authoritative scenario definition consumed by every applicable level. Runners MAY adapt transport and interaction mechanics, but MUST NOT maintain divergent business acceptance rules.

Evidence for every level MUST bind at least:

- exact source head and tree;
- test level and target platform;
- build/profile authority;
- canonical journey identifier and completed steps;
- measured protocol counters or state transitions where applicable;
- restart/restoration and fresh-reverification results;
- cleanup result;
- declared mocks and the boundary at which they were applied.

Evidence MUST be sanitized and MUST NOT retain offers, grants, bearer tokens, nonces, credentials, DIDs, proofs, seeds, capabilities, private keys, device serials, private hostnames, or tailnet identities.

The recommended execution policy is:

| Trigger | Required levels |
|---|---|
| Local change / fast PR feedback | Level 1 |
| Mobile adapter, lifecycle, storage, or composition change | Levels 1–2 |
| Wallet journey or mobile UI/UX change | Levels 1–3 |
| Release candidate, hardware/security change, or explicit physical claim | Levels 1–4 |
| Nightly | Levels 1–3; Level 4 only on an intentionally managed device lane |

When a higher level finds a defect, the defect SHOULD be reproduced and pinned at the lowest practical level before the fix is accepted. This keeps the feedback loop deterministic and prevents physical-device automation from becoming the primary debugging environment.

## 7. CI Performance

CI duration is itself a quality attribute.

Suggested fast-check budgets:

```text
formatting                 < 1 min
workspace check            < 2 min
clippy                     < 3 min
unit tests                 < 5 min
architecture checks        < 1 min
dependency/security        < 2 min
```

Independent jobs SHOULD run in parallel.

Targets:

```text
fast developer feedback    <= 5 min
complete required PR CI    <= 10 min
investigation threshold    > 15 min
```

### Pull requests

Required PR validation SHOULD include:

```text
format
clippy
workspace check
unit tests
coverage
architecture checks
dependency/security checks
headless native build
mobile target compile checks
```

### Main branch

More expensive checks MAY include:

```text
Android build
iOS build
UniFFI/binding generation
integration tests
headless release builds
platform integration tests
```

### Nightly/scheduled

Long-running checks SHOULD include where useful:

```text
complete target matrix
expensive E2E tests
sanitizers
fuzzing
long-running property tests
dependency compatibility experiments
additional security analysis
```

Not every mobile target permutation SHOULD block every PR if equivalent confidence can be obtained more cheaply.

Meaningful CI-time regressions SHOULD be reviewed like runtime performance regressions.

## 8. Rust Quality Gates

Required PR checks SHOULD include equivalents of:

```bash
cargo fmt --all --check

cargo check --workspace --all-targets

cargo clippy \
  --workspace \
  --all-targets \
  --all-features \
  -- \
  -D warnings

cargo nextest run --workspace

cargo doc \
  --workspace \
  --no-deps
```

The repository SHOULD additionally enforce dependency/license policy, vulnerability checks, coverage, and dependency hygiene with suitable tools such as `cargo-deny`, `cargo-audit`, and `cargo-llvm-cov`.

Production code MUST compile without warnings under supported CI toolchains.

Project-specific lint suppressions SHOULD include justification where the reason is not self-evident.

## 9. Unsafe Rust

Safe Rust SHOULD be the default.

Crates that do not require unsafe code SHOULD use:

```rust
#![forbid(unsafe_code)]
```

Where unsafe Rust is genuinely necessary:

- scope MUST be minimized;
- every unsafe block MUST have a `// SAFETY:` explanation;
- safety invariants MUST be documented;
- unsafe changes MUST receive additional review;
- safe wrappers SHOULD surround unsafe internals.

Generated FFI code MAY follow a separately documented policy.

## 10. Dependencies and Supply Chain

Dependencies SHOULD be minimized.

New dependencies SHOULD be evaluated for:

- actual capability need;
- maintenance quality;
- security posture;
- platform support;
- license;
- compile-time cost;
- compatibility with supported targets.

The repository SHOULD centralize workspace dependency versions where practical, avoid duplicate versions without reason, monitor vulnerabilities, define allowed licenses, and commit `Cargo.lock` for an application-oriented monorepo.

AI-generated changes MUST NOT add a new dependency when an appropriate workspace dependency or existing internal capability already exists without explicit justification.

## 11. Documentation

Documentation has four responsibilities:

1. API documentation;
2. architecture documentation;
3. developer/contributor documentation;
4. user/integration documentation.

Public library APIs SHOULD be documented. Suitable crates SHOULD use:

```rust
#![warn(missing_docs)]
```

Stable public crates MAY eventually use:

```rust
#![deny(missing_docs)]
```

Important crates SHOULD include crate-level documentation describing purpose, architecture, examples, invariants, security assumptions, and important error behavior.

Comments SHOULD primarily explain **why**, rather than restating what code does.

Repository-level documentation SHOULD include:

```text
README.md
ARCHITECTURE.md
CONTRIBUTING.md
SECURITY.md
CHANGELOG.md

docs/
  architecture/
  adr/
  protocols/
  testing/
  security/
```

Significant long-term architectural, security, compatibility, or dependency decisions SHOULD be captured as ADRs.

Major public workflows SHOULD have executable or compile-checked examples.

## 12. Security and Secret Handling

Secrets MUST NOT appear in:

- logs;
- panic messages;
- tracing spans;
- snapshots;
- fixtures containing real credentials;
- externally exposed diagnostic errors.

Sensitive types SHOULD avoid accidental `Debug`, `Display`, serialization, or cloning where inappropriate.

Cryptographic operations SHOULD use reviewed implementations rather than custom cryptography unless there is a documented requirement and appropriate expert review.

Security-critical modules SHOULD receive stronger tests and coverage than ordinary platform glue.

## 13. Error Handling

Production library code SHOULD return typed errors.

`unwrap()`, `expect()`, and panic-based handling MUST NOT be used for expected runtime failures in domain/library code.

They MAY be used where an invariant makes failure impossible and the reason is clear, or in tests/examples.

Errors SHOULD preserve enough diagnostic context without exposing secrets.

## 14. Performance

Performance budgets SHOULD be defined where relevant for:

- mobile startup;
- memory usage;
- credential parsing;
- cryptographic operations;
- storage initialization/migration;
- FFI calls;
- large credential collections.

Representative benchmarks SHOULD be added for performance-sensitive or regression-prone paths.

Performance optimizations MUST NOT violate architectural boundaries without an explicit architectural decision.

## 15. Compatibility and Reproducibility

The repository MUST explicitly define supported targets, for example:

```text
macOS native
Linux native
iOS
Android
optional WASM
```

The project SHOULD define an MSRV when it is intended to support external library consumers.

CI SHOULD validate the supported Rust toolchain policy.

Schema, persistent storage, protocol, credential, and public API compatibility requirements SHOULD be explicitly versioned.

Persistent storage migrations MUST have upgrade tests when user data is involved.

## 16. AI-Assisted Development Guardrails

AI agents MUST follow the same architectural rules as human contributors.

AI-generated changes MUST NOT introduce without explicit justification:

- production files larger than 1,000 LOC;
- new crates without an architectural responsibility;
- unnecessary dependencies;
- duplicated domain types;
- parallel implementations of existing abstractions;
- platform dependencies in core/domain crates;
- catch-all utility modules;
- unexplained unsafe code;
- disabled tests;
- weakened quality gates;
- broad lint suppressions;
- coverage exclusions added solely to make CI pass.

Before adding an abstraction, an agent SHOULD search for an existing equivalent.

Before adding a dependency, an agent SHOULD inspect workspace dependencies and existing implementations.

Before creating a crate, an agent SHOULD determine why the capability cannot belong to an existing cohesive crate.

Agents SHOULD prefer small, reviewable changes over large speculative rewrites.

## 17. Exceptions

Quality thresholds are guardrails, not incentives to manipulate metrics.

Exceptions MAY be appropriate for:

- generated code;
- FFI bindings;
- generated protocol structures;
- cryptographic test vectors;
- large declarative mappings;
- compatibility shims;
- performance-critical implementations.

Exceptions SHOULD be explicit, narrowly scoped, documented, reviewable, and removed when no longer necessary.

A numeric metric MUST NOT override a clearly better architectural or security decision.

## 18. Quality Baseline

```text
Rust production file:
  preferred <= 400 LOC
  review    > 600 LOC
  justify   > 1,000 LOC

Function:
  preferred <= 50 LOC
  review    > 80–100 LOC

Crate:
  typical   2k–10k LOC
  review    > 20k–30k LOC

Cyclomatic complexity:
  target    <= 10
  warning   > 15
  review    > 20

Coverage:
  workspace >= 80%
  core      >= 85%
  critical  >= 90%
  new code  >= 90%

CI:
  fast feedback <= 5 min
  required PR   <= 10 min
  investigate   > 15 min

Compiler/Clippy warnings:
  0

Unsafe:
  0 by default
  documented exceptions only

Documentation:
  public APIs documented
  major workflows have examples
  architectural decisions captured as ADRs
```

## 19. Highest-Priority Rules

If only a small set can initially be enforced:

1. **Core logic MUST remain platform-independent.**
2. **Production Rust files SHOULD remain below 600 LOC; files above 1,000 LOC MUST be justified.**
3. **New/changed core code SHOULD maintain at least 90% coverage.**
4. **Required PR CI SHOULD complete within 10 minutes.**
5. **Crate dependencies MUST follow the declared architecture.**
6. **Compiler warnings and configured Clippy violations MUST fail CI.**
7. **Unsafe Rust MUST be forbidden by default or explicitly documented.**
8. **AI agents MUST NOT bypass quality gates to make CI pass.**

Dependency direction takes precedence over superficial metrics. Large files are often symptoms of poor decomposition; incorrect dependency direction creates long-term architectural debt.

## 20. Pull Request Quality Checklist

A pull request is quality-compliant when:

- [ ] Formatting passes.
- [ ] Required targets compile.
- [ ] Clippy passes with no configured warnings.
- [ ] Relevant unit/integration tests pass.
- [ ] Coverage thresholds are maintained.
- [ ] New code has appropriate tests.
- [ ] No unjustified large source files are introduced.
- [ ] No inappropriate crate dependency direction is introduced.
- [ ] New crates have a clear architectural responsibility.
- [ ] New dependencies are justified.
- [ ] Public APIs are documented.
- [ ] Security-sensitive changes have appropriate tests/review.
- [ ] Unsafe code is absent or explicitly documented.
- [ ] CI remains within the expected time budget.
- [ ] Relevant architecture/ADR documentation is updated.
- [ ] No test, lint, coverage, or security gate was weakened merely to make CI pass.
