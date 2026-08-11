# Architecture decision records

ADRs record consequential technical decisions for Oxid. Use four-digit,
monotonically increasing filenames. Each record describes its status, context,
decision, consequences, and any superseded decisions.

Accepted ADRs are changed only by a later ADR that explicitly supersedes them.
The root blueprint remains the broader product and engineering constitution.

## How to read status

ADR status and delivery state answer different questions:

- **Accepted** means the rule governs new work, even if its capability is on a
  later milestone.
- **Proposed** means research or an explicit decision is still required; it
  does not authorize a dependency or implementation.
- **Implementation state** records current repository evidence and must not be
  read as product-readiness by itself.

## Blueprint and repository traceability

| ADR | Status | Blueprint decision or repository source | Current delivery state |
| --- | --- | --- | --- |
| [0001](0001-modular-hexagonal-architecture.md) Modular hexagonal architecture | Accepted | §§3, 6 | M0 dependency graph implemented and checked |
| [0002](0002-dioxus-as-incoming-adapter.md) Dioxus incoming adapter | Accepted | §§3, 6 | M0 UI and composition implemented |
| [0003](0003-oxid-owned-domain-types.md) Oxid-owned domain types | Accepted | §§3, 6, 18 | Enforced in M0 core APIs |
| [0004](0004-capability-specific-ports.md) Capability-specific ports | Accepted | §§3, 7 | Initial wallet/platform ports implemented |
| [0005](0005-static-adapter-composition-for-mvp.md) Static adapter composition | Accepted | §§5, 6 | Implemented in `crates/composition` |
| [0006](0006-rust-first-controlled-edge-fallbacks.md) Rust-first edge policy | Accepted | §§3, 4 | Enforced; no foreign runtime fallback in M0 |
| [0007](0007-identity-is-a-peer-capability.md) Identity as a peer capability | Accepted | §§1, 4–6 | Binding boundary; delivery begins M3 |
| [0008](0008-did-methods-as-capability-negotiated-adapters.md) DID adapters | Accepted | §§7, 9 | Planned for M3/M5 |
| [0009](0009-separate-credential-models-from-serializations.md) Credential model separation | Accepted | §10 | Planned for M3–M5 |
| [0010](0010-oidc-and-didcomm-as-protocol-adapters.md) Protocol adapters | Accepted | §§7, 11 | Planned for M4/M6 |
| [0011](0011-secure-key-operations-behind-ports.md) Protected key operations | Accepted | §§3, 7, 12–13 | Policy enforced; adapters begin M1 |
| [0012](0012-mobile-first-target-priority.md) Mobile-first targets | Accepted | §§1, 4, 12–13, 16 | Features compile; native hosts deferred |
| [0013](0013-local-first-and-telemetry-off.md) Local-first, telemetry-off | Accepted | §§4–5, 12–13 | Enforced for M0 |
| [0014](0014-cardano-library-selection.md) Cardano libraries | Proposed | §§8, 17 | Research gate before M1 |
| [0015](0015-midnight-library-selection.md) Midnight libraries | Proposed | §§8, 17 | Research gate before M2 |
| [0016](0016-ssi-component-selection.md) SSI components | Proposed | §§9–11, 17 | Research gate before M3 |
| [0017](0017-platform-backed-secret-storage.md) Secret storage | Proposed | §§7, 12, 17 | Decision required before M1 custody |
| [0018](0018-structured-error-taxonomy.md) Error taxonomy | Proposed | §§10, 13 | Partially exercised by M0 errors |
| [0019](0019-explicit-application-events-only-when-needed.md) Event model | Proposed | §§3, 13 | No event infrastructure in M0 |
| [0020](0020-layered-testing-strategy.md) Layered testing | Accepted | §§3, 13, 18 | M0 baseline and coverage gate implemented |
| [0021](0021-staged-prototype-migration.md) Staged prototype migration | Accepted | §§14, 17–19 and prototype review | M0 slice migrated from immutable baseline |
| [0022](0022-nix-reproducible-development-and-ci.md) Reproducible Nix environment | Accepted | Repository harness | Implemented locally and in CI |

## Current boundaries

M0 implements ADR-0001 through ADR-0006, the applicable policy portions of
ADR-0011 through ADR-0013, ADR-0020, ADR-0021, and ADR-0022. ADR-0007 through
ADR-0010 constrain future identity/protocol work without claiming it exists.
ADR-0014 through ADR-0019 remain research or design gates as stated in their
individual records.
