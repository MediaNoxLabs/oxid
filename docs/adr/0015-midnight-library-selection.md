# ADR-0015: Midnight library and protocol selection

- Status: Accepted
- Date: 2026-08-11
- Blueprint source: Sections 8 and 17
- Implementation state: Binding for M2; account read model implemented by #6
  and native standalone-indexer sync implemented by #7

## Context

The prototype lives inside `midnight-ledger` and directly consumes internal
workspace crates, generated proving artifacts, indexer/node interfaces, and
pre-production configuration. Those dependencies cannot define the standalone
wallet boundary.

The maintained upstream surface is intentionally split:

- `midnight-ledger` defines transaction and ledger-state semantics and exposes
  Rust packages, but does not publish all of them to crates.io;
- `midnight-zk` contains the proof-system crates;
- `midnight-wallet` is the canonical Wallet SDK and executable specification,
  but its runtime packages are TypeScript;
- `midnight-indexer` exposes the wallet read protocol as GraphQL v4; and
- `midnight-node` exposes chain status and transaction submission interfaces.

No one repository is therefore an appropriate aggregate dependency for Oxid.
The selection must also preserve native iOS/Android compilation, protected key
boundaries, replaceable transports, and chain-neutral application semantics.

## Decision

Oxid owns every domain and application type. Midnight types are converted only
inside capability-specific outgoing adapters.

Use the official public repositories and the following reviewed immutable
revisions as the initial M2 compatibility baseline:

| Concern | Repository | Revision | Use in Oxid |
| --- | --- | --- | --- |
| Ledger semantics | `https://github.com/midnightntwrk/midnight-ledger.git` | `d9414884db9da9e9b1f6f3a7f742d79a5732f817` | Semantic source; a Git dependency only for adapters that consume canonical transaction/state types or serialization |
| Proof system | `https://github.com/midnightntwrk/midnight-zk.git` | `cd2c27b2659de157409a9b96dba0dbaf1218f00b` | A proving adapter only when proving is implemented and measured |
| Wallet protocol and vectors | `https://github.com/midnightntwrk/midnight-wallet.git` | `25d0c3857fc0e20435e06a9225bd8709ecce1115` | Protocol reference and public conformance vectors; not a runtime dependency |
| Indexer GraphQL v4 | `https://github.com/midnightntwrk/midnight-indexer.git` | `82759bf186184684f13a9ffa97b58b7b7684f47c` | Narrow HTTP/WebSocket adapter documents and schema-contract tests |
| Node interface | `https://github.com/midnightntwrk/midnight-node.git` | `3edc67697668f8e3a762e5ffa36116bfa187fb71` | Status/submission adapter and reviewed metadata only |

Unpublished Cargo packages must use the official HTTPS Git source and a full
40-character `rev` in `Cargo.toml`. A lock-file resolution alone is not an
acceptable pin. Use the smallest feature set for each adapter. In particular,
do not add `midnight-zk`, `midnight-zkir`, or proving features to account-read
adapters that do not prove or verify transactions.

Treat the official Wallet SDK as the behavioral reference for network IDs,
HD roles, Bech32m address formats, synchronization state, transaction history,
and public test vectors. Do not embed its TypeScript runtime, Node polyfills,
or WebView bridge in the Rust application.

Consume indexer and node capabilities as protocols, not implementation-crate
dependencies. Embed only the GraphQL documents or reviewed metadata a focused
adapter needs and pin their provenance to an upstream commit. Contract tests
must cover cursor ordering, decimal `u128` values, transaction result mapping,
and schema drift.

The first live indexer adapter uses the v4 `unshieldedTransactions`
`graphql-transport-ws` subscription from the selected revision. Its WebSocket
route and public address are explicit startup configuration for the native
headless composition; they are not persisted in chain identity or selected by
normal mobile composition. The adapter enforces progress-first replay,
protocol and resource bounds, exact values, and safe transport-error mapping.

Network identity is not a transport route. Core types may contain `mainnet`,
`preprod`, `preview`, `qanet`, `devnet`, `testnet`, `undeployed`, or a validated
custom network identifier. HTTP, WebSocket, prover, and dApp URLs are runtime
adapter configuration and must never be hard-coded into domain objects.

The prototype commit
`074b1a4bccbfee1740ee188374b606a022ecef42` remains migration evidence, not a
source of ledger-relative Cargo paths, raw seeds, or deployment configuration.

Every selected Rust dependency must compile in the workspace and for the two
Tier-1 native target graphs before its adapter is considered delivered. Local
proof feasibility on mobile must be measured separately; success compiling
ledger types does not imply acceptable proving latency or memory use.

## Consequences

- M2 preserves chain-neutral network, account, balance, sync, and transaction
  semantics while allowing the upstream protocol to evolve behind adapters.
- Ledger, indexer, node, and proving concerns remain separate ports and may
  advance at different rates.
- The first account slice can use ledger semantics and Wallet SDK vectors
  without importing a foreign runtime or granting access to private key bytes.
- The first read-model adapter does not directly depend on `midnight-ledger`:
  a reviewed build trial showed that its minimal feature set still resolves an
  unnecessary transaction/proof graph. This does not waive the Git pin for a
  later adapter that actually consumes ledger types.
- Live unshielded NIGHT synchronization is available to explicitly configured
  native headless runs. Shielded and DUST synchronization remain incremental;
  every headless adapter identifies live, cached, or simulated data truthfully.
- A source revision update requires dependency review, conformance tests,
  mobile builds, and an update to the recorded compatibility baseline.
- If maintained Rust wallet packages are published later, a follow-up ADR may
  replace Git sources after checking semantic and mobile equivalence.

The repository-location and immutable-revision rules are enforced by
[Midnight Git source policy](../dependencies/midnight-git-sources.md).
