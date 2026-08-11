# Midnight account read-model provenance

## Reviewed sources

The account slice was reimplemented from behavior observed at these immutable
revisions on 2026-08-11:

| Evidence | Revision and path | Retained behavior |
| --- | --- | --- |
| Mobile prototype | `midnight-ledger` `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core` and `mobile-bench/dioxus-wallet` | explicit connect/resync, network selection, receive addresses, NIGHT/DUST balances, activity presentation, and separate chain identity/transport concerns |
| Ledger semantics | `midnight-ledger` `d9414884db9da9e9b1f6f3a7f742d79a5732f817`, `ledger/src/structure.rs` | `STARS_PER_NIGHT = 1_000_000` and `SPECKS_PER_DUST = 1_000_000_000_000_000` |
| Wallet address vectors | `midnight-wallet` `25d0c3857fc0e20435e06a9225bd8709ecce1115`, `packages/address-format/test/addresses.json` | public unshielded, shielded, and DUST payloads for seed class `01`, plus mainnet/devnet Bech32m expectations |
| Indexer protocol research | `midnight-indexer` `82759bf186184684f13a9ffa97b58b7b7684f47c`, GraphQL v4 schema | decimal `u128` values, cursor-first progress/history ordering, block metadata, and transaction status/fee shapes for a later live adapter |

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
- local, tailnet, pre-production, node, indexer, or prover endpoints;
- live GraphQL subscriptions, cursor persistence, chain checkpoints, and DUST
  raw-ledger events;
- transaction construction, signing, proving, submission, replacement, or fee
  estimation;
- generated proof artifacts, native projects, JavaScript bridges, QR scanning,
  copy/share integration, databases, and captured diagnostics.

Production composition therefore exposes the network catalog but returns an
unavailable account snapshot with no account ID, address, balance, or activity
claim. Only development/headless composition can select the simulated source,
which remains empty until explicit synchronization and labels every response
`simulated`.
