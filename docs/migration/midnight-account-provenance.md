# Midnight account read-model provenance

## Reviewed sources

The account slices were reimplemented from behavior observed at these immutable
revisions on 2026-08-11 and re-verified on 2026-08-12:

| Evidence | Revision and path | Retained behavior |
| --- | --- | --- |
| Mobile prototype | `midnight-ledger` `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core` and `mobile-bench/dioxus-wallet` | explicit connect/resync, network selection, receive addresses, NIGHT/DUST balances, activity presentation, and separate chain identity/transport concerns |
| Ledger semantics | `midnight-ledger` `d9414884db9da9e9b1f6f3a7f742d79a5732f817`, `ledger/src/structure.rs` | `STARS_PER_NIGHT = 1_000_000` and `SPECKS_PER_DUST = 1_000_000_000_000_000` |
| Wallet address vectors | `midnight-wallet` `25d0c3857fc0e20435e06a9225bd8709ecce1115`, `packages/address-format/test/addresses.json` | public unshielded, shielded, and DUST payloads for seed class `01`, plus mainnet/devnet Bech32m expectations |
| Indexer protocol | `midnight-indexer` `82759bf186184684f13a9ffa97b58b7b7684f47c`, `indexer-api/graphql/schema-v4.graphql` | `graphql-transport-ws`, progress-first `unshieldedTransactions`, decimal `u128` values, UTXO create/spend replay, block metadata, transaction status, and DUST fee shapes |
| Prototype live transport | `midnight-ledger` `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/unshielded/{snapshot,transport}.rs` | bounded connection/ack/idle behavior, progress-first snapshot termination, ping/pong, and address-scoped replay semantics |

No Rust or TypeScript implementation was copied. Oxid owns the domain model,
ports, simulation, and presentation. The adapter retains public vector payloads
only; it does not retain the upstream seed or derive/store private material.

## Implemented mapping

- prototype network picker -> `WalletNetworkPort` and profile-scoped selection;
- address pills -> typed public `ChainAddress` values encoded with Bech32m;
- NIGHT/DUST hero -> exact `u128` atomic values mapped to decimal strings and
  rendered without floating-point arithmetic;
- connect/resync -> asynchronous `SyncWalletAccountUseCase`;
- cursor/tip/account status -> owned `WalletSyncStatus` and truthful source;
- activity -> owned transaction direction, status, block/time, balance changes,
  and optional fee;
- headless commands and Assets page -> two incoming adapters over the same use
  cases.

## Live standalone-indexer mapping

Issue #7 adds an optional native headless composition over the same account
ports. It is enabled only when network identity, a GraphQL WebSocket route, and
one public unshielded address are supplied together at startup. Those values
are never written to profile metadata. Missing configuration retains the
deterministic public simulator; partial or invalid configuration fails startup.

The
[embedded query](../../crates/adapters/midnight/queries/unshielded_transactions.graphql)
is narrowed
from the selected v4 schema. The native adapter:

- requires successful `graphql-transport-ws` negotiation and handles
  protocol/WebSocket ping-pong;
- bounds endpoint length, connection/ack/idle/total snapshot time,
  frame/message size, replay event count, and decoded UTXO-record count;
- rejects URL credentials, queries, fragments, unsupported schemes, invalid
  Bech32m payloads, and network/address HRP mismatches;
- rejects foreign-owner UTXOs, malformed hex, negative cursors/heights/times,
  numeric rather than decimal-string values, cursor regression, inconsistent
  duplicate transactions, and arithmetic overflow;
- recognizes official native unshielded NIGHT as the 32-byte zero token type,
  retains custom unshielded tokens with deterministic raw identities, and maps
  fees to exact DUST specks;
- returns `live` for a completed refresh, `cached` for later reads, and a safe
  stalled state after transport failure without exposing external payloads.

The executable integration test starts an ephemeral local protocol fixture and
drives the real headless binary through create/select/account/connect/balance/
history/quit. No deployment endpoint, seed, or private key is committed.

The seven catalog IDs are `mainnet`, `preprod`, `preview`, `testnet`, `qanet`,
`devnet`, and `undeployed`. They carry identity and environment only. Runtime
node, indexer, and prover routes belong to future outgoing adapter
configuration.

## Dependency decision

A temporary host build added the selected `midnight-ledger` Git revision with
default features disabled. It compiled successfully, proving the official
unpublished Cargo source is consumable, but resolved 131 additional packages,
including the ledger transaction/proof graph. This read adapter uses neither
canonical transaction serialization nor proving, so that dependency was
removed. Adapter-local constants are pinned to the exact reviewed source line,
and conformance tests prevent silent drift.

The accepted Git pin remains mandatory for the future transaction adapter that
actually consumes ledger types. Proof packages remain outside the dependency
graph until an isolated proving adapter is designed and measured on both mobile
targets.

## Deliberate exclusions

- prototype demo, genesis, pre-production, and raw seeds;
- protected HD/Jubjub/root material or private derivation;
- committed local, tailnet, pre-production, node, indexer, or prover endpoints;
- persisted unshielded cursors, background subscriptions, chain checkpoints,
  shielded state, DUST generations, and DUST raw-ledger events;
- transaction construction, signing, proving, submission, replacement, or fee
  estimation;
- generated proof artifacts, native projects, JavaScript bridges, QR scanning,
  copy/share integration, databases, and captured diagnostics.

Production composition therefore exposes the network catalog but returns an
unavailable account snapshot with no account ID, address, balance, or activity
claim. Native headless composition selects either the deterministic simulator
or an explicitly configured public live source. Neither mode provides custody,
shielded assets, DUST generation state, transaction signing, or submission.
