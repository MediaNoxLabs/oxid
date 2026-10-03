# Prototype presentation classification

The reviewed `midnight-ledger` Dioxus wallet at
`074b1a4bccbfee1740ee188374b606a022ecef42` is an evidence library, not an
Oxid visual or architectural authority. Oxid owns the behavior, ports,
presentation models, components, and tests listed below. The product contract
is [the Oxid design specification](../design/README.md).

This inventory uses exactly four classifications:

- **Product capability** — preserve the user outcome and state-machine
  semantics; present it through Oxid-owned components.
- **Reusable engineering pattern** — retain the solved platform technique
  behind a focused port or adapter and regression evidence.
- **Presentation debt** — preserve behavior while replacing the remaining
  prototype-shaped composition with the linked design contract.
- **Prototype-only behavior** — keep absent, or remove only after the named
  executable dependency check proves that no supported journey depends on it.

## Inventory

| Area | Classification | Current Oxid source or evidence | Governing authority | Disposition |
| --- | --- | --- | --- | --- |
| Profile, wallet, and realm selection | Product capability | `crates/wallet/application`; `crates/ui-dioxus/src/profile_quick_switcher.rs`; profile and realm Maestro flows | ADR-0025; `docs/design/information-architecture.md` | Preserve the one-realm/multiple-wallet model and focused switcher; complete task-focus reconciliation under #151. |
| Public NIGHT, shielded NIGHT, DUST, receive, and send | Product capability | `crates/adapters/midnight`; `crates/ui-dioxus/src/assets_page.rs`; wallet synchronization and transfer tests | ADR-0088, ADR-0090, ADR-0100, ADR-0114; `docs/design/journeys.md` | Preserve typed freshness and automatic reconciliation; do not restore prototype manual subsystem-sync controls. |
| DID inventory and lifecycle | Product capability | `crates/identity`; `crates/adapters/did-midnight`; `crates/ui-dioxus/src/dids.rs`; DID Maestro/XCTest flows | ADR-0036, ADR-0037, ADR-0047; `docs/design/journeys.md` | Preserve explicit DID authority and managed/public distinctions; visual evidence remains under #798. |
| Credential inventory, issuance, presentation, and consent | Product capability | `crates/credential`; `crates/presentation`; `crates/protocol`; focused identity review surfaces and hermetic tests | ADR-0038 through ADR-0050; `docs/design/journeys.md` | Preserve explicit four-question consent, retained review, and safe terminal states; finish bounded journey reconciliation under #151. |
| Closed-code diagnostics | Product capability | `crates/diagnostics`; `crates/ui-dioxus/src/diagnostics.rs`; diagnostics repository and UI tests | ADR-0080; `docs/design/ui-profiles.md` | Keep payload-free runtime health in the developer profile. The encrypted support-journal expansion remains separately governed by #119. |
| Embedded proof demonstration | Product capability | `crates/ui-dioxus/src/proof_benchmark.rs`; authenticated proving artifacts and focused benchmark tests | ADR-0072; #527 | Keep the bounded development demonstration behind an explicit feature/profile. General-purpose proving benchmarks and performance research remain upstream. |
| Android/iOS TLS and trust initialization | Reusable engineering pattern | `crates/adapters/platform-system`; native transport trust guard and mobile build tests | ADR-0077 and ADR-0078 | Retain canonical platform trust behind the transport adapter; never copy prototype endpoint or personal-network configuration. |
| UI-thread isolation and bounded workers | Reusable engineering pattern | `crates/ui-dioxus/src/work.rs`; application ports; lifecycle and worker tests | ADR-0021, ADR-0114; `docs/architecture/runtime-domain-model.md` | Keep blocking and I/O work outside render callbacks and publish only typed results. |
| Suspend, resume, reconnect, and checkpoint recovery | Reusable engineering pattern | wallet reconcilers, durable checkpoints, iOS/Android lifecycle tests | ADR-0090, ADR-0114 | Retain idempotent recovery from durable cursors; never depend on unrestricted background execution. |
| Safe areas, snapshot privacy, and native lifecycle bridges | Reusable engineering pattern | `apps/oxid/android`; `apps/oxid/ios`; `crates/adapters/mobile-native-plugin`; mobile evidence contracts | ADR-0070, ADR-0074 through ADR-0078, ADR-0094 | Retain focused native operations and platform privacy. Bootstrap accessibility hardening remains #887. |
| QR, app-link, and native identity ingress | Reusable engineering pattern | `crates/adapters/identity-ingress`; native ingress scripts and physical Android evidence | ADR-0069 and ADR-0070 | Preserve typed ingress and permission/lifecycle handling; physical iOS evidence remains separately bounded. |
| Route-shell copy, density, and broad page composition | Presentation debt | `crates/ui-dioxus/src/lib.rs`; Lunar Aegis components; modular Maestro inventory | `docs/design/README.md`; `docs/design/information-architecture.md` | Continue bounded vertical slices under #151 and audit actual captures under #789/#798; do not perform a wholesale rewrite. |
| Developer/demo surface density | Presentation debt | `crates/ui-dioxus/src/developer_tools.rs`; capability, diagnostics, benchmark, and event-log routes | `docs/design/ui-profiles.md`; `docs/design/design-system.md` | Keep developer facts out of the user profile and reconcile remaining hierarchy/copy through #789. |
| Persistent free-form logs, arbitrary tracing fields, and process telemetry | Prototype-only behavior | Release-exclusion and diagnostics negative tests; closed `DiagnosticCode` model | ADR-0080; #119 | Keep absent. Before any removal of transitional code, run diagnostics negative tests and release marker scans; #119 requires a new accepted privacy design before expansion. |
| WebView/iframe command bridges and raw JavaScript wallet control | Prototype-only behavior | typed native plugin, identity-ingress, and release-exclusion tests | ADR-0006, ADR-0021, ADR-0070 | Keep absent. Run native bridge, ingress, and production-closure tests before deleting any remaining compatibility shim. |
| Automatic consent, DID publication, or fixture execution | Prototype-only behavior | approval/DID-authority guards; explicit demo drawer review boundaries | ADR-0087, ADR-0096 | Keep absent. Run approval-composition, DID-authority, and Maestro consent-boundary tests before removing legacy fixture paths. |
| Hard-coded seeds, genesis authority, raw recovery, and private payload UI | Prototype-only behavior | custody ports, secret-mode policy, source/release scans | ADR-0017, ADR-0046, ADR-0093 | Keep absent from normal builds. Run release-profile, secret-source, and backup/custody tests before removing a compatibility fixture. |

## Removal rule

Prototype provenance alone never authorizes preservation or deletion. A
prototype-only row can be removed only when its listed executable checks pass
on the exact candidate and the scenario inventory contains no supported use
case that names it. Product-capability and reusable-pattern rows require a
replacement with equivalent tests before old code disappears. Presentation
debt is delivered as small issue-backed vertical slices; #528 is not a license
for a monolithic UI rewrite.

## Remaining bounded owners

- #151 owns task-focused journey reconciliation.
- #789 owns comparison of actual current captures with the proposed design.
- #798 owns cross-platform visual and accessibility release evidence.
- #119 owns any encrypted, persisted support-journal expansion.
- #527 owns the development proof-demonstration feature boundary.
- #887 owns the remaining bootstrap safe-area accessibility hardening.

