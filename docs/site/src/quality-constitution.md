<!-- SPDX-License-Identifier: Apache-2.0 -->

# Oxid quality constitution v1.1

This document turns the owner-authored
[`RUST_MONOREPO_QUALITY.md`](quality-north-star.md) north star into an
operational policy for Oxid. The north star defines the desired quality
attributes and guardrails. This constitution identifies which rules are
binding now, which targets require measured follow-up, and how exceptions are
reviewed.

The product and dependency architecture remains governed by
[`OXID_IDENTITY_WALLET_BLUEPRINT.md`](https://github.com/MediaNoxLabs/oxid/blob/develop/OXID_IDENTITY_WALLET_BLUEPRINT.md),
accepted [ADRs](adr-catalog.md), and the executable architecture allowlist.
This constitution does not authorize behavior, dependency, security, or gate
changes that those authorities forbid.

## Scope and precedence

Version 1 applies to public, host-native headless and desktop development and
to the public CI that validates them. Android, iOS, simulators, emulators,
physical devices, and private-credential evidence are explicitly deferred from
this initial policy slice. Existing mobile and security gates remain binding;
the deferral is not permission to remove, skip, or weaken them.

When rules appear to conflict, use this order:

1. security and privacy invariants, accepted ADRs, and the blueprint;
2. executable architecture, source, DCO/signature, and CI gates;
3. the mandatory rules in this constitution;
4. staged targets in the north star.

A numeric metric never justifies artificial splitting, a micro-crate, an
inward dependency violation, reduced test depth, or weaker security.

## Mandatory now

These rules reflect current repository controls and apply to every change:

- Issue-backed work starts from `origin/develop`, targets `develop`,
  and follows
  [`docs/issue-branch-delivery.md`](https://github.com/MediaNoxLabs/oxid/blob/develop/docs/issue-branch-delivery.md).
- Architectural changes require an ADR. Dependencies continue to point inward;
  domain and application crates remain independent of platform and adapter
  implementations.
- A new crate must own a meaningful capability. It must be added to the
  default-deny architecture allowlist and must not be introduced merely to
  reduce file size.
- New dependencies require a concrete capability need, source/license/security
  review, an exact workspace-level version where applicable, and target-impact
  review. Existing capabilities and workspace dependencies must be considered
  first.
- Compiler and configured Clippy warnings fail the strict gate.
- Safe Rust is the default. The workspace denies unsafe code, and the
  architecture check has a file-level allowlist that rejects any `unsafe` token
  outside `crates/adapters/storage-json/src/lib.rs`. The compiler requires an
  explicit allowance, while ADR and security review constrain the allowed
  file's use to the reviewed Android profile-path JNI boundary. The architecture
  check does not by itself pin a function, allowance attribute, or unsafe-block
  count. New or widened unsafe code requires an ADR and security review, not an
  ordinary quality exception.
- Changed behavior ships with tests at the lowest deterministic layer that can
  prove it. Failure and boundary cases are first-class test requirements.
- Tests must be deterministic and isolated from public networks, execution
  order, ambient developer configuration, and shared mutable state unless an
  explicitly configured evidence lane owns that dependency.
- The current delivery phase uses a uniform 70% line-coverage floor for the
  workspace, package classes, and changed production code. This owner-approved
  throughput policy is reviewed after the next metrics window; exclusions may
  not be added merely to make a change pass.
- DCO sign-off, commit signature, architecture, lint, test, coverage,
  dependency, source, documentation, and existing platform gates may not be
  bypassed or weakened.
- AI-assisted changes follow the same requirements. They may not add parallel
  domain types, speculative abstractions, broad lint suppressions, disabled
  tests, unreviewed dependencies, micro-crates, or metric-only refactors.

### AI instruction assets

`AGENT.md`, agent skills, prompt templates, and reader-oriented runbooks are
maintained as reviewed operational documentation unless production or factory
tooling parses them as executable input. Do not add dedicated tests merely to
pin their prose, frontmatter, headings, duplicated copies, command lists, or
other agent guidance. Such tests create a second representation of the
instruction and accumulate maintenance without proving product behavior.

Prefer one canonical instruction, an always-loaded pointer to it, focused human
review, and the ordinary documentation/link checks. An executable test is
justified only when tooling actually parses or dispatches from the asset, or
when it protects concrete product, security, or delivery behavior beyond the
wording itself. In that case, test the parser or observable behavior rather
than reproducing the prose, and record the failure mode that makes the added
maintenance worthwhile.

Existing size and complexity debt is a baseline, not a blanket exception.
Unrelated work is not required to refactor it, but a production file already
above 1,000 physical lines must not grow without an issue-linked justification
and a decomposition assessment. A touched large file should become smaller or
stay size-neutral unless cohesion, generated material, a protocol fixture, or
another narrow exception makes growth safer.

## Staged targets

The following north-star targets are review signals, not new failing gates in
v1:

| Attribute | Staged target | Promotion requirement |
| --- | --- | --- |
| Production Rust file size | Prefer below 400 lines; review above 600; justify above 1,000; normally reject above 2,000 | A syntax-aware, generated/test-aware inventory and issue-backed remediation plan |
| Function size | Prefer below 50 lines; review above 80–100 | A reviewed Rust-aware measurement with macro and closure handling |
| Cyclomatic complexity | Target 10 or less; warn above 15; review above 20 | A pinned reproducible tool, baseline, and false-positive policy |
| Crate size | Typical capability crate 2k–10k lines; review above 20k–30k | Capability and dependency analysis; no quota-driven splitting |
| Coverage | Current gate 70% across measured scopes; later ratchet toward workspace 80%, core/domain 85%, critical and changed production code 90% | Stable scope definitions, per-scope baselines, non-gameable CI reporting, and measured throughput headroom |
| Documentation | Public APIs and major workflows documented | Baseline missing-doc coverage and stage warning-to-deny by public surface |
| CI latency | Fast feedback at most 5 minutes; required PR CI at most 10 minutes; investigate above 15 | Per-job history, cache correctness, and separate problem-focused CI work |

No staged threshold becomes mandatory merely by being listed here. Tightening a
gate requires its own issue, baseline evidence, review of false positives and
runtime cost, and a green change that does not weaken another gate.

## Test strategy

Tests are placed by the risk they prove, not by a desired pyramid shape.
Host-native headless evidence is the first promotion level and the focus of v1.
The repository's reader-oriented current practice is described in the
[testing strategy](testing-strategy.md).

| Risk or boundary | Primary evidence | Required policy |
| --- | --- | --- |
| Domain invariants and state transitions | Unit tests beside the domain code | Cover success, rejection, boundaries, checked arithmetic, and transition legality without adapters |
| Application orchestration and ports | Use-case tests with focused in-memory fakes | Prove authorization order, error mapping, idempotence, cancellation, and secret-free views |
| Parsers, codecs, and untrusted structures | Unit, property, compatibility, and fuzz tests as practical | Preserve known-answer vectors; add round-trip, truncation, corruption, and no-panic properties for touched high-risk inputs |
| Storage, network, custody, and protocol adapters | Contract and hermetic integration tests | Test permissions, bounds, restart, tamper, protocol negatives, and redacted errors at the adapter boundary |
| Bindings and public facades | API/ABI and serialization compatibility tests | Prevent internal types, secrets, or unstable implementation details from becoming public contracts |
| Headless incoming adapter | NDJSON black-box and process-restart journeys | Cover success, malformed input, explicit consent, replay rejection, restoration, reverification, and protocol-only stdout |
| Desktop Dioxus shell | Component/state tests and fixed-viewport host smoke where relevant | Reuse application truth; never represent desktop rendering as mobile integration evidence |
| Cross-capability journeys | Host-native headless E2E first | Use the real domain, protocol, persistence, and composition boundaries with only explicit adapter substitutions |
| Performance-sensitive host paths | Benchmarks or bounded timing evidence | Establish a stable fixture and environment before adopting a regression budget |

Mobile packaging, lifecycle, UI automation, hardware custody, camera, private
infrastructure, and physical-device performance remain later promotion levels.
A higher-level result never waives a lower-level failure. This v1 slice neither
runs nor changes those lanes.

Every behavior-changing pull request records the tests added or updated and the
commands actually run. Retrying a flaky test does not convert an unexplained
failure into evidence; the flake is a defect or the lane remains non-green.

## CI strategy and budgets

CI tiers separate fast feedback from complete and externally constrained
evidence. The current implementation is recorded in the
[baseline](quality-baseline-2026-08-26.md); the target tiering below must be
implemented through separate reviewed issues rather than by deleting checks.

| Tier | Intended evidence | Duration policy in v1 |
| --- | --- | --- |
| Fast PR | Formatting, static architecture/source/docs checks, and focused headless/desktop checks that do not require the full artifact graph | Target at most 5 minutes; not yet a separately required hosted context |
| Required PR | Change-relevant public merge evidence: strict basic gate, affected unit/host consumer, docs, DCO/signature, and security checks | Target at most 10 minutes; expensive coverage, quality, optimized release, and packaging remain explicit on-demand lanes and complete-branch backstops |
| Integration branch | The required-PR contract at the merged head plus branch publishing checks | Same job budgets as required PR; no merge result is accepted if required evidence is stale for the head |
| Scheduled | Full hermetic Nix checks and other expensive deterministic public evidence | Explicit 120-minute ceiling; failures remain failures and may not be hidden with retries |
| Owner-private | Credentials, funded infrastructure, physical devices, and private environments | Deferred; no v1 runtime budget or required public status is claimed |

The owner-private tier must never place credentials, device identifiers, private
routes, or unredacted artifacts in public CI. A later issue must define its
sanitized evidence schema, operator authority, timeout, and promotion rule
before it can support a release claim.

### Cache and reuse contract

A cache is an optimization, never evidence. Cache keys and validation must bind
the inputs that affect the reused result, including the Rust toolchain,
`Cargo.lock`, Nix inputs, selected features/targets, source head where needed,
and generated-artifact identities. A hit must still execute the authoritative
check. Security-sensitive artifacts, private inputs, signatures, proofs, and
unreviewed outputs must not cross trust boundaries through a shared cache.

CI-duration improvements must preserve semantic coverage. Jobs may run in
parallel, avoid duplicate compilation, or reuse authenticated immutable inputs;
they may not skip a test family, reduce a target matrix, lower a threshold, add
retries for known flakes, or trust an artifact whose inputs are not bound.

## Exception policy

An exception is a temporary, narrow decision that leaves the default rule in
force. It is not an inline suppression without review and cannot override a
security invariant, accepted ADR, DCO/signature requirement, or required gate.

A new quality exception must be committed under `docs/quality/exceptions/` in
the same issue-backed change and include:

- a stable identifier, owner, issue, approval date, and expiry or removal
  condition;
- the exact rule, file/module/target, and smallest affected scope;
- measured evidence showing why compliance is currently less safe or practical;
- risk, security/privacy impact, and compensating tests or controls;
- why alternatives, including decomposition without a new crate, were rejected;
- the command that detects whether the exception still applies;
- a removal plan and the follow-up issue.

Reviewers reject an exception that is open-ended, repository-wide, metric-only,
or phrased as permission to reduce coverage or skip a gate. Expiry does not make
a red gate green; the exception must be removed, renewed through a fresh review,
or the affected change must stop.

Generated code, bindings, protocol fixtures, test vectors, declarative tables,
and cohesive algorithms may justify an exception, but are not automatically
excluded. Existing RustSec allowances remain governed separately by
[`docs/security/advisory-exceptions.md`](https://github.com/MediaNoxLabs/oxid/blob/develop/docs/security/advisory-exceptions.md).

## Change control

Changes to mandatory rules, tier definitions, or exception requirements update
the version in this document and require issue-backed review. Changes that
alter architecture, security boundaries, supported targets, or public API
policy also require the relevant ADR. Baseline refreshes do not change this
constitution and belong in a dated baseline document.
