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
| [0011](0011-secure-key-operations-behind-ports.md) Protected key operations | Accepted | §§3, 7, 12–13 | Opaque generation, HD derivation, and signing implemented in development; native adapters pending |
| [0012](0012-mobile-first-target-priority.md) Mobile-first targets | Accepted | §§1, 4, 12–13, 16 | Features compile; native hosts deferred |
| [0013](0013-local-first-and-telemetry-off.md) Local-first, telemetry-off | Accepted | §§4–5, 12–13 | Enforced for M0 |
| [0014](0014-cardano-library-selection.md) Cardano libraries | Proposed | §§8, 17 | Research gate before M1 |
| [0015](0015-midnight-library-selection.md) Midnight libraries and protocols | Accepted | §§8, 17 | Account model #6, live sync #7, and protected external NIGHT derivation #8 implemented |
| [0016](0016-ssi-component-selection.md) SSI components | Proposed | §§9–11, 17 | Research gate before M3 |
| [0017](0017-platform-backed-secret-storage.md) Platform custody, secret blobs, and authorization | Accepted | §§7, 12, 17 and prototype security review | Development generated-root/HD/signing harness implemented; native mobile adapters required |
| [0018](0018-structured-error-taxonomy.md) Error taxonomy | Proposed | §§10, 13 | Partially exercised by M0 errors |
| [0019](0019-explicit-application-events-only-when-needed.md) Event model | Proposed | §§3, 13 | No event infrastructure in M0 |
| [0020](0020-layered-testing-strategy.md) Layered testing | Accepted | §§3, 13, 18 | M0 baseline and coverage gate implemented |
| [0021](0021-staged-prototype-migration.md) Staged prototype migration | Accepted | §§14, 17–19 and prototype review | Profile lifecycle, presentation shell, and headless harness migrated from immutable baseline |
| [0022](0022-nix-reproducible-development-and-ci.md) Reproducible Nix environment | Accepted | Repository harness | Implemented locally and in CI |
| [0023](0023-prioritize-midnight-prototype-parity.md) Prioritize staged Midnight prototype parity | Accepted | Product direction and parity epic | Wallet shell, headless harness, and profile lifecycle implemented; capability backlog open |
| [0024](0024-versioned-headless-wallet-protocol.md) Versioned headless wallet protocol | Accepted | Prototype parity epic and issues #4/#5/#8 | v1 profiles, protected keys, account derivation/sync, and shutdown implemented |
| [0025](0025-persist-public-wallet-profile-metadata.md) Persist public profile metadata separately from secrets | Accepted | §§3, 7, 12–13, 17 and issue #1 | JSON profile metadata, selection, restore, UI, and headless flows implemented |
| [0026](0026-stage-midnight-transfer-authorization.md) Stage Midnight transfer authorization before proving/submission | Accepted | §§3, 7–8, 12–13 and issue #9 | Canonical unshielded NIGHT prepare/authorize/draft flow implemented; proving/submission queued |
| [0027](0027-complete-standalone-midnight-transaction-submission.md) Complete Midnight submission through bounded standalone adapters | Accepted | §§3, 7–8, 12–13 and issue #11 | Native development/headless DUST, proof-server, and node submission implemented; local path added by ADR-0028 |
| [0028](0028-keep-midnight-proof-witnesses-local.md) Keep Midnight proof witnesses local by default | Accepted | §§3–5, 7–8, 12–13 and issue #12 | Native local DUST proving and iOS/Android resource harness implemented; production custody remains fail-closed |
| [0029](0029-expose-standalone-wallet-flows-on-mobile.md) Expose standalone wallet flows on mobile | Accepted | §§3, 6–8, 12–13, 16–18 and issue #14 | Explicit development mobile composition, receive QR, and protected simulated transfer journey implemented; production remains fail-closed |
| [0030](0030-persist-public-midnight-account-checkpoints.md) Persist public Midnight account checkpoints outside wallet core | Accepted | §§3, 5–8, 12–13, 17–18 and issue #15 | Public unshielded restart/cache/resume implemented; private DUST and shielded state are governed separately by ADR-0031/0033 |
| [0031](0031-persist-private-midnight-dust-checkpoints.md) Persist private Midnight DUST checkpoints behind live catch-up | Accepted | §§3, 5–8, 12–13, 17–18 and issue #16 | Scoped DUST resume, bounded incremental replay, and live-before-spend gate implemented; shielded Zswap is governed by ADR-0033 |
| [0032](0032-expose-resumable-dust-sync-sessions.md) Expose resumable DUST synchronization as an adapter-owned session | Accepted | §§3, 5–8, 12–13, 16–18 and issue #17 | Native worker, partial checkpoints, headless lifecycle, and Assets progress implemented; production custody pending |
| [0033](0033-keep-shielded-zswap-state-adapter-private.md) Keep shielded Zswap keys and replay state inside the Midnight adapter | Accepted | §§3, 5–8, 12–13, 16–18 and issue #18 | Protected receive address, bounded native replay/worker, private checkpoints, and headless/mobile lifecycle implemented; spending and production custody pending |
| [0034](0034-expose-safe-transaction-submission-cancellation.md) Expose transaction submission status and safe cancellation | Accepted | §§7–8, 12–13, 16–18 and issue #19 | Adapter-owned pre-broadcast cancel boundary plus headless/mobile status, cancel, and retry implemented; durable follow-up delivered by ADR-0035 |
| [0035](0035-persist-and-reconcile-midnight-submissions.md) Persist and reconcile Midnight transaction submissions | Accepted | §§3, 5–8, 12–13, 16–18 and issue #20 | Public persist-before-broadcast journal, restart duplicate prevention, finalized reconciliation, and headless/mobile recovery implemented |

## Current boundaries

M0 implements ADR-0001 through ADR-0006, the applicable policy portions of
ADR-0011 through ADR-0013, ADR-0020, ADR-0021, and ADR-0022. ADR-0007 through
ADR-0010 constrain future identity/protocol work without claiming it exists.
ADR-0014, ADR-0016, and ADR-0018 through ADR-0019 remain research or design
gates as stated in their individual records. ADR-0015 now binds all Midnight
adapter work to the selected official Git and protocol surfaces. ADR-0017 now binds the M1
custody design without claiming native adapters are complete. ADR-0023
reprioritizes prototype parity after M0 without bypassing those gates. ADR-0024
establishes a safe second incoming adapter for exercising each slice. ADR-0025
makes profile metadata durable without conflating it with protected custody.
Issue #8 composes ADR-0011, ADR-0015, ADR-0017, and ADR-0024 for a
development-only external NIGHT derivation flow; it does not change their
production-custody requirements. ADR-0026 composes those same boundaries into
canonical transaction authorization while keeping proving and submission
truthfully separate. ADR-0028 adds a measured private local-proving mode and
leaves ADR-0027's remote path as an explicit development option.
ADR-0029 exposes those existing account and transaction boundaries through a
separately selected development mobile composition without changing normal
production wiring. ADR-0030 keeps public Midnight replay persistence inside the
native outgoing adapter and requires a successful live catch-up before cached
UTXOs can become spendable inputs. ADR-0031 applies the same live-before-spend
rule to key-specific DUST state while using the official tagged ledger state
behind a distinct private, bounded adapter store. ADR-0032 adds an explicit
adapter-owned session and permits bounded partial checkpoints without weakening
that live-before-spend requirement. ADR-0033 keeps Zswap key use, ownership
replay, and checkpoints inside the native Midnight adapter while
allowing only public shielded addresses and bounded safe projections outward.
ADR-0034 separates submission-attempt status from retained draft state and
permits cancellation only before the adapter atomically enters broadcast.
ADR-0035 makes that boundary durable, restores safe public outcomes after a
restart, and permits replacement only after finalized rejection or expiry.
