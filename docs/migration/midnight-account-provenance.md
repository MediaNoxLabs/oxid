# Midnight account read-model provenance

## Reviewed sources

The account slices were reimplemented from behavior observed at these immutable
revisions on 2026-08-11 and re-verified on 2026-08-12:

| Evidence | Revision and path | Retained behavior |
| --- | --- | --- |
| Mobile prototype | `midnight-ledger` `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core` and `mobile-bench/dioxus-wallet` | explicit connect/resync, network selection, receive addresses, NIGHT/DUST balances, activity presentation, and separate chain identity/transport concerns |
| Ledger semantics | `midnight-ledger` `d9414884db9da9e9b1f6f3a7f742d79a5732f817`, `ledger/src/structure.rs` | `STARS_PER_NIGHT = 1_000_000` and `SPECKS_PER_DUST = 1_000_000_000_000_000` |
| Wallet HD protocol | `midnight-wallet` `25d0c3857fc0e20435e06a9225bd8709ecce1115`, `packages/hd/src/HDWallet.ts`, `packages/hd/test/tests.test.ts`, and locked `@scure/bip32` 2.2.0 | `m/44'/2400'/account'/role/index`, hardened purpose/coin/account, roles, bounds, root clearing, and third-party BIP32 parity |
| Wallet key/address vectors | same wallet revision, `packages/spec-reference/src/{test-vectors,key-derivation-reference}.ts` and generated address JSON | the vector `seed` is used as the already-derived unshielded scalar; SHA-256 public-key payload and Bech32m codec expectations |
| Ledger key/address semantics | `midnight-ledger` `d9414884db9da9e9b1f6f3a7f742d79a5732f817`, `base-crypto/src/{schnorr,hash}.rs` and `coin-structure/src/coin.rs` | BIP340 x-only verifying-key bytes and SHA-256 `UserAddress` construction |
| Indexer protocol | `midnight-indexer` `82759bf186184684f13a9ffa97b58b7b7684f47c`, `indexer-api/graphql/schema-v4.graphql` | `graphql-transport-ws`, progress-first `unshieldedTransactions`, decimal `u128` values, UTXO create/spend replay, block metadata, transaction status, and DUST fee shapes |
| Prototype live transport | `midnight-ledger` `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/unshielded/{snapshot,transport}.rs` | bounded connection/ack/idle behavior, progress-first snapshot termination, ping/pong, and address-scoped replay semantics |

No Rust or TypeScript implementation was copied. Oxid owns the domain model,
ports, simulation, and presentation. Public address payloads remain codec
fixtures. The development custody adapter generates its own process-local root,
derives children behind an opaque port, and never retains an upstream seed or
accepts recovery material.

## Implemented mapping

- prototype network picker -> `WalletNetworkPort` and profile-scoped selection;
- address pills -> typed public `ChainAddress` values encoded with Bech32m;
- NIGHT/DUST hero -> exact `u128` atomic values mapped to decimal strings and
  rendered without floating-point arithmetic;
- connect/resync -> asynchronous `SyncWalletAccountUseCase`;
- cursor/tip/account status -> owned `WalletSyncStatus` and truthful source;
- activity -> owned transaction direction, status, block/time, balance changes,
  and optional fee;
- external NIGHT derivation -> bounded account/address indices, protected
  `m/44'/2400'/account'/0/index`, opaque BIP340 key reference, x-only public-key
  hash, and network-specific Bech32m address;
- headless commands and Assets page -> two incoming adapters over the same use
  cases.

## Protected account derivation mapping

Issue #8 connects ADR-0015's Midnight semantics to ADR-0017's custody boundary.
`wallet.account.derive` accepts only account and address indices in `[0, 2^31)`.
The application passes a typed path to custody; the Midnight adapter sees only
the resulting x-only public key and opaque reference. It hashes the public key,
encodes the address, and binds that public account to subsequent simulated or
live reads. Repeating the same derivation is idempotent. Uninitialized and
locked profiles fail closed, and protocol decoding rejects seed, mnemonic,
private-key, and caller-supplied path fields.

The committed cross-language fixture uses public conformance input `[0x01; 32]`
as an HD root and the pinned Wallet SDK path `m/44'/2400'/0'/0/0`. The locked
TypeScript implementation produces x-only public key
`b193e54524dc796402870a883fbdcd83869c9c307dda8c0d99c5f769169fc883`,
payload `8a27486764300ee8e1a54b1fd65195c0ec2c276bf6ffb65cf173b9a42f077460`,
and devnet address
`mn_addr_devnet13gn5semyxq8w3cd9fv0av5v4crkzcfmt7mlmvh83wwu6gtc8w3sqr2gnec`.
Oxid does not commit or expose the derived scalar; ordinary DTOs and headless
responses contain only the public address and opaque key reference.

## Live standalone-indexer mapping

Issue #7 adds an optional native headless composition over the same account
ports. It is enabled only when network identity, a GraphQL WebSocket route, and
one public unshielded address are supplied together at startup. Those values
are never written to profile metadata. Missing configuration retains the
deterministic public simulator; partial or invalid configuration fails startup.
The configured address is the initial watch-only account. Once a profile
derives a protected account, its address replaces that fallback for the
profile, clears the previous cached snapshot, and scopes the next subscription.

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
- caller-supplied roots, mnemonics, recovery/import/export, durable software
  roots, or production mobile custody;
- internal NIGHT/change roles beyond external receive derivation, shielded
  Zswap keys, DUST keys, and metadata keys;
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
or an explicitly configured public live source and adds process-local
development derivation/BIP340 signing by opaque reference. Neither mode
constructs, proves, or submits transactions, or provides shielded assets, DUST
generation state, durable recovery, or production custody.
