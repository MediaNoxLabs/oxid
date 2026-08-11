# ADR-0015: Midnight library selection

- Status: Proposed
- Date: 2026-08-11
- Blueprint source: Sections 8 and 17
- Implementation state: Research required before M2

## Context

The prototype lives inside `midnight-ledger` and directly consumes internal
workspace crates, generated proving artifacts, indexer/node interfaces, and
pre-production configuration. Those dependencies cannot define the standalone
wallet boundary.

## Proposed decision

Prefer maintained official Midnight Rust, indexer, and node interfaces where
they satisfy wallet capabilities. Pin exact versions or immutable commits and
isolate evolving APIs in Midnight adapter crates. Evaluate target support,
license, maintenance, security, proving constraints, and replacement strategy
before adding dependencies.

Use the prototype at commit
`074b1a4bccbfee1740ee188374b606a022ecef42` as migration evidence, not as a
source of ledger-relative Cargo paths or production configuration.

## Consequences if accepted

- M2 preserves chain-neutral account and transaction semantics.
- Proving, indexer, node, and contract concerns may require separate ports and
  adapters.
- Mobile proof feasibility must be measured rather than inferred from desktop.
- This proposal does not select or authorize a dependency today.

The repository-location and immutable-revision prerequisites are documented and
enforced in [Midnight Git source policy](../dependencies/midnight-git-sources.md).
That policy narrows acceptable sources without accepting this ADR or selecting
a particular upstream commit.
