# Midnight account read-model provenance

## Reviewed sources

The account slices were reimplemented from behavior observed at these immutable
revisions on 2026-08-11 and re-verified on 2026-08-12:

| Evidence | Revision and path | Retained behavior |
| --- | --- | --- |
| Mobile prototype | `midnight-ledger` `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core` and `mobile-bench/dioxus-wallet` | explicit connect/resync, network selection, receive addresses, NIGHT/DUST balances, activity presentation, and separate chain identity/transport concerns |
| Ledger semantics | `midnight-ledger` `d9414884db9da9e9b1f6f3a7f742d79a5732f817`, `ledger/src/structure.rs` | `STARS_PER_NIGHT = 1_000_000` and `SPECKS_PER_DUST = 1_000_000_000_000_000` |
| Wallet HD protocol | `midnight-wallet` `25d0c3857fc0e20435e06a9225bd8709ecce1115`, `packages/hd/src/HDWallet.ts`, `packages/hd/test/tests.test.ts`, and locked `@scure/bip32` 2.2.0 | `m/44'/2400'/account'/role/index`, hardened purpose/coin/account, roles, bounds, root clearing, and third-party BIP32 parity |
| Wallet key/address vectors | same wallet revision, `packages/spec-reference/src/{test-vectors,key-derivation-reference}.ts` and `packages/spec-reference/test-vectors/addresses.json` | the vector `seed` is used as already-derived role material; SHA-256 unshielded payload plus 64-byte shielded coin/encryption public-key payload and Bech32m codec expectations |
| Ledger key/address semantics | `midnight-ledger` `d9414884db9da9e9b1f6f3a7f742d79a5732f817`, `base-crypto/src/{schnorr,hash}.rs` and `coin-structure/src/coin.rs` | BIP340 x-only verifying-key bytes and SHA-256 `UserAddress` construction |
| Indexer protocol | `midnight-indexer` `82759bf186184684f13a9ffa97b58b7b7684f47c`, `indexer-api/graphql/schema-v4.graphql` | `graphql-transport-ws`, progress-first `unshieldedTransactions`, decimal `u128` values, UTXO create/spend replay, block metadata, transaction status, and DUST fee shapes |
| Prototype live transport | `midnight-ledger` `074b1a4bccbfee1740ee188374b606a022ecef42`, `mobile-bench/wallet-core/src/unshielded/{snapshot,transport}.rs` | bounded connection/ack/idle behavior, progress-first snapshot termination, ping/pong, and address-scoped replay semantics |
| Prototype DUST sync | same prototype revision, `mobile-bench/wallet-core/src/dust/syncer.rs` and `mobile-bench/dioxus-wallet/src/app.rs` (`WalletSyncPane`) | explicit DUST status/progress, next-cursor resume, exact official-state balance, resync control, and a separate NIGHT/DUST presentation lifecycle |
| Prototype transfer | same prototype revision, `mobile-bench/wallet-core/src/{wallet.rs,unshielded/mod.rs}` | native NIGHT, same-network recipient decoding, descending greedy selection, sorted spends/outputs, change, `0xCAFE` intent segment, one-hour TTL, and BIP340 authorization before DUST/proving/submission |
| Prototype completion | same prototype revision, `mobile-bench/wallet-core/src/{wallet.rs,dust/snapshot.rs,tx/balance.rs,tx/prove_http.rs,node/client.rs}` | DUST role `2/0`, event replay, live time/parameters, iterative `0xFEED` fee balancing, proof-server wire format, sealing, tagged serialization, and unsigned runtime submission |

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
- DUST sync/status/cancel -> a focused `WalletDustSyncPort`, adapter-owned
  worker sessions, Oxid-owned progress/failure projection, and versioned
  headless plus Dioxus incoming adapters;
- cursor/tip/account status -> owned `WalletSyncStatus` and truthful source;
- activity -> owned transaction direction, status, block/time, balance changes,
  and optional fee;
- external NIGHT derivation -> bounded account/address indices, protected
  `m/44'/2400'/account'/0/index`, opaque BIP340 key reference, x-only public-key
  hash, and network-specific Bech32m address;
- shielded receive derivation -> protected `m/44'/2400'/account'/3/0`, official
  Zswap public keys serialized into the Wallet SDK's canonical 64-byte payload,
  and a distinct network-specific Bech32m address;
- headless commands and Assets page -> two incoming adapters over the same use
  cases.

## Canonical transfer authorization mapping

[Issue #9](https://github.com/MediaNoxLabs/oxid/issues/9) adds the first write
slice under ADR-0026. The native adapter uses the selected official Git
revision's canonical ledger types. It retains profile-scoped drafts internally,
constructs native NIGHT spends and recipient/change outputs, uses OS-generated
binding randomness, uses a public-data fingerprint only to make repeated
prepare requests idempotent, and sends only the canonical intent payload
through the opaque custody signing port. The returned BIP340 signature is
verified before the signed transaction is retained.

Headless prepare/authorize/draft responses contain exact public preview data
only. Their fee is `requires_balancing` and `proofRequired` is true. Prepared
drafts report `submissionReady: false`; successfully authorized drafts report
`submissionReady: true`. Drafts expire after one hour and expired signing or
transaction material is cleared. ADR-0027 adds the subsequent explicitly
confirmed submit stage and keeps this review boundary intact.

## Standalone completion mapping

Issue #11 completes the development/headless path through a focused
`WalletTransactionPort::submit` operation. The DUST secret is derived at
`m/44'/2400'/account'/2/0` and borrowed only inside a custody callback. The
adapter bounds and replays DUST ledger events, rejects malformed live chain
parameters, balances fees at `0xFEED`, sends DUST proof preimages to a validated
proof-server route, seals and serializes internally, and submits the dynamic
`Midnight.send_mn_transaction` call unsigned. Only the final fee plus public
transaction/block identifiers cross the application boundary.

Headless simulation covers submission without contacting external services and
labels its outcome. Locked custody and failures known to precede or reject node
submission restore the authorized state. An ambiguous post-submit node outcome
stays `submitting` rather than permitting a duplicate. Cancelling the async
future leaves the worker responsible for publishing its eventual final or
retryable state, so another send cannot race an external side effect. Completed
retries return the identical outcome. No response contains the DUST child,
proof input, signature, or transaction bytes.

Issue #19 and ADR-0034 make that worker control explicit without broadening the
chain-material boundary. Oxid-owned status distinguishes running,
cancellation-requested, broadcasting, cancelled, included, and unknown
attempts. The Midnight adapter atomically closes cancellation immediately
before its node call. A pre-broadcast cancellation restores `Authorized` only
after worker acknowledgement; broadcasting and ambiguous outcomes remain
non-retryable. Headless and Dioxus consume the same status/cancel use cases.

Issue #20 and ADR-0035 make the post-broadcast boundary durable without moving
chain material into core. The Midnight adapter atomically journals the public
extrinsic hash, finalized pre-broadcast anchor, fee, expiry, profile/network/
draft scope, and one-way planning fingerprint before node submission. Restart
status/history block a matching duplicate. Explicit reconciliation walks at
most 2,048 finalized ancestors to the anchor, inspects the exact extrinsic's
System result, and otherwise uses the authoritative chain-tip timestamp only
after the anchor is reached. Included, rejected, expired, and unresolved
results are persisted. Headless simulation has a two-process recovery fixture;
the Assets page shows the latest restored outcome and offers reconciliation
only when it is safe and meaningful. The iOS and Android standalone smoke flows
assert that included public metadata survives an application restart while the
development root and signed draft do not.

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

Issue #18 and ADR-0033 extend the same protected account with its public
shielded receive rail. The Zswap child is borrowed at
`m/44'/2400'/<account>'/3/0`; official `midnight-zswap` key derivation produces
the coin and encryption public keys, which are serialized as the Wallet SDK's
64-byte address payload. The `[0x01; 32]` public conformance seed maps to
`mn_shield-addr_devnet1p99fzfvf2z2q05zaaqzml8laccfd8uhzm9t2jewxggyr65tj4dp4gcfv7e04ka0x7qeajljmln7za5d4edntjxncx4q0uh6gkkj706ggme77n`.
Only that address crosses the account/application boundary; the child seed and
Zswap secret keys are dropped inside custody.

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

## Durable public unshielded checkpoint mapping

[Issue #15](https://github.com/MediaNoxLabs/oxid/issues/15) retains the useful
offset-plus-state pattern from the prototype backlog while keeping its redb
wallet aggregate out of Oxid core. An explicit native adapter store records
only the validated public unshielded fold under `(network, address)`. On
restart, account reads project the snapshot as `cached`; the next subscription
uses `current_cursor + 1` and folds the delta over the retained UTXOs and public
history. Protocol/data incompatibility retries once from zero, while transport
failure preserves the last values and marks them stalled.

The v1 JSON document is capped at 16 MiB and 128 accounts, encodes every
`u128` as a decimal string, rejects duplicate records and malformed snapshots,
and uses owner-only atomic replacement. It contains no route, profile label,
key reference, secret, draft, signature, witness, proof, or transaction bytes.
Hydration does not unlock spendable inputs; a successful live catch-up in the
current process is required before transfer preparation. The executable test
proves initial cursor `0`, restart cursor `3` after a checkpoint at `2`, exact
delta history/balance, and an offline cached/stalled read in a third process.

## Durable private DUST checkpoint mapping

[Issue #16](https://github.com/MediaNoxLabs/oxid/issues/16) and ADR-0031 retain
the prototype syncer's official tagged-state-plus-offset behavior without its
network-only cache scope or 524,288-event in-memory channel. An adapter-private
v1 binary envelope is keyed by network plus SHA-256 of the tagged public DUST
key and records the live parameter fingerprint, last folded current cursor,
advertised target, update time, and tagged `DustLocalState<DefaultDB>`. It is bounded to
four records, 16 MiB per state, and 64 MiB overall and uses owner-private atomic
replacement. No seed or secret scalar is serialized.

Standalone submission fetches the live tip first, loads only an exact
network/key/parameter match, subscribes at `current_cursor + 1`, checks exact
event continuity, and folds batches of at most 256 events/4 MiB. Catch-up is
bounded to one million events, 512 MiB processed bytes, and 30 minutes. Cached
state becomes eligible for balancing only after the live stream reaches its
advertised target (including an explicit empty completion for an already
current cursor). Invalid cached delta data gets one fresh replay; timeouts and
connection failures fail closed and do not cause a second history request.

[Issue #17](https://github.com/MediaNoxLabs/oxid/issues/17) and ADR-0032 expose
that same fold as an explicit session without moving ledger state into core.
The application projection carries lifecycle, current/target work, processed
events, exact atomic DUST, freshness, and a sanitized failure category. Native
sync runs on a dedicated adapter worker and saves every successfully folded
bounded batch, so cancellation or restart resumes from the last consistent
partial cursor. The deterministic headless controller covers fresh, cancelled,
resumed, and already-current flows; the Assets pane polls the same use cases and
labels cached or stalled balance as not live enough to spend. Production
composition remains explicitly unavailable.

The native worker is covered separately from the lower-level replay helper. A
fixed adapter-private chain-tip source keeps HTTP transport out of pure-Nix
tests, while the real GraphQL-WebSocket fixture proves owned-event replay to an
exact 12 DUST balance, `cursor + 1` resume, partial-batch cancellation, durable
checkpoint publication, and redacted transport failure. Normal construction
still obtains the chain tip through the bounded explicit HTTP route.

The seven catalog IDs are `mainnet`, `preprod`, `preview`, `testnet`, `qanet`,
`devnet`, and `undeployed`. They carry identity and environment only. Runtime
node, indexer, and prover routes belong to future outgoing adapter
configuration.

## Dependency decision

The public account model does not consume ledger runtime types. The native
Midnight adapter now also uses `midnight-zswap` directly for protected public
shielded-address derivation and the forthcoming canonical replay engine. The transaction
slice now justifies direct native dependencies on the selected
`midnight-ledger` packages with default ledger features disabled. Cargo still
resolves the upstream unconditional transaction/proof graph, so the dependency
is target-gated away from `wasm32` and must pass both native mobile graphs.
The ledger `proving` feature is enabled for DUST proof orchestration and resolves
published Midnight proof crates transitively. ADR-0028 adds the unpublished
`midnight-zkir 2.1.0` package from the same full official ledger Git revision;
there is no local path or direct `midnight-zk` Git dependency. The private
prover's source, resource bounds, and mobile measurements are recorded in its
dependency review.

## Deliberate exclusions

- prototype demo, genesis, pre-production, and raw seeds;
- runtime caller-supplied roots, mnemonics, recovery/import/export, durable
  software roots, or production mobile custody; an ADR-0098 ignored test may
  accept one explicit out-of-band development funding root and must zeroize it;
- internal NIGHT/change roles beyond external receive derivation, exported
  Zswap/DUST keys, and metadata keys;
- committed local, tailnet, pre-production, node, indexer, or prover endpoints;
- continuous background subscriptions and retained raw DUST/Zswap event
  history;
- automatic replacement, fee preview/estimation, UTXO reservation, or durable
  signed-draft queues;
- generated proof artifacts, native projects, JavaScript bridges, QR scanning,
  copy/share integration, databases, and captured diagnostics.

Default production composition therefore exposes the network catalog but
returns an unavailable account snapshot with no account ID, address, balance,
or activity claim. ADR-0098 adds a separate opt-in constructor that can only
receive an opaque signed-profile value after audience/validity/sequence/TLS
verification and an exact node-genesis check; no reviewed root/profile is
selected by the default app. Native headless composition selects either the deterministic simulator
or an explicitly configured public live source and adds process-local
development derivation/BIP340 signing by opaque reference. Full standalone
configuration additionally proves and submits canonical unshielded NIGHT
intents through either private local proving or an explicit development proof
server. Explicit complete-standalone mode may persist key-scoped DUST state.
Either explicit live mode may persist key-scoped official Zswap state and
expose only bounded shielded balance/note projections. Protected development
accounts expose a canonical shielded receive address and the typed transfer
boundary supports fresh-sync-gated shielded spending. ADR-0098/#91 proves one
genesis-authority spend against the real standalone stack, including finalized
inclusion, adapter reconstruction, included-status restoration, and nullifier
replay; the funded test does not exercise unknown-outcome chain rescanning and
it does not prove process/native-custody restart or a fresh recipient's ability
to pay DUST. Development composition persists owner-private DUST/Zswap
checkpoints plus public submission recovery metadata; protected roots and
signed drafts remain process-local. Production custody remains unavailable.
