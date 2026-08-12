# Midnight ledger prototype migration

## Baseline

This inventory was prepared from the latest wallet prototype branch available
on 2026-08-11:

- repository: `midnight-ledger`;
- branch: `feat/mobile-prototype`;
- commit: `074b1a4bccbfee1740ee188374b606a022ecef42`;
- source root: `mobile-bench/`.

The selected commit describes itself as superseding the earlier
`dioxus-vc-demo`, `feature/dioxus-vc-verification`, and `mobile-prototype`
branches. Always re-check the remote and record a new immutable source commit
before migrating later work.

## Source inventory and destinations

| Prototype area | Capabilities observed | Oxid destination | Migration state |
| --- | --- | --- | --- |
| `wallet-core` profile/wallet service concepts | Wallet construction, service façade, UI port | `wallet/domain`, `wallet/application`, focused ports | Create/list/select/restore profile lifecycle implemented |
| `wallet-core` address, HD, balances, transaction, sync | Midnight addresses, derivation, NIGHT/DUST, build/sign/submit, indexer/node access | chain-neutral chain domain/use cases plus `adapters/midnight` | Network/account reads, simulated/live sync, durable public unshielded and private DUST checkpoint/resume, protected NIGHT/DUST/Zswap receive derivation, canonical shielded event decoding/replay/checkpointing, simulated shielded lifecycle, and staged unshielded transfer through DUST proof and node inclusion implemented for development/headless; native live shielded session wiring pending |
| `wallet-core/secret_storage` and `unlock` | Multi-curve keys, encrypted files, redb, opaque references, boot lock, attempt throttling | wallet-owned session/key-operation ports plus platform-backed and development adapters | ADR-0017 accepted; process-local Ed25519/P-256 plus BIP32/secp256k1-Schnorr conformance implemented; durable recovery and native custody pending |
| `wallet-core/did` and DID services | `did:midnight` create/resolve/update/deactivate | identity domain/use cases plus `adapters/did-midnight` | Deferred to M5 |
| `wallet-core/oid4vci_client` and `oid4vp_client` | Credential issuance, SIOP/OID4VP response flows | credential/presentation application plus protocol adapters | Deferred to M4 |
| `wallet-core/vc_store` and `vc_self_verify` | Signed credential bytes, metadata, self-verification | credential domain/store/verification ports and adapters | Deferred to M3/M5 |
| `wallet-core/vault` | Passport-vault contract interaction and selective-disclosure claim | product-specific Midnight adapter/example, not generic wallet core | Deferred; separate ADR |
| `dioxus-wallet` | Mobile/desktop UI, QR bridges, JS eval bridge, DID/credential/vault screens | `ui-dioxus`, platform adapters, protocol/chain adapters | Profile lifecycle, account-aware Assets page, receive QR, protected development activation, and staged transfer UI reimplemented; remaining capability pages and native bridges deferred |
| `headless-wallet` | Line-delimited JSON driver for use cases | `apps/oxid-headless` incoming CLI/test adapter | Safe versioned transport, protected key/account flows, simulated/live reads, and staged canonical transfer submission implemented; SSI flows queued |
| `prover-core` | Local/HTTP proof execution and benchmark paths | Midnight proving adapter | Private local DUST proving implemented with an authenticated bounded cache; remote proving retained for explicit development |
| benchmark crates and fixtures | Mobile proving measurements and test circuits | dedicated opt-in adapter harness | One real DUST proof/seal/codec harness implemented and measured on iOS/Android; generated artifacts remain uncommitted |
| Android/iOS projects | WebView hosts, permissions, QR bridges | `apps/oxid` platform hosts | Dioxus-generated hosts build and launch the explicit standalone-development composition through repository scripts; native camera/copy/share/custody bridges remain deferred |

## M0 migration decisions

- No prototype source is copied verbatim. The first use case is reimplemented
  against Oxid-owned types because the source `wallet-core` directly depends on
  internal ledger workspace crates.
- The prototype's useful separation between headless and Dioxus drivers informs
  the incoming use-case trait, but UI prompting is not generalized before a
  concrete second incoming adapter exists.
- Dioxus is upgraded from the source manifest's 0.6 line to the current stable
  0.7 line selected by the blueprint and isolated in `ui-dioxus`/`apps/oxid`.
- The initial profile contains only an identifier, normalized public label, and
  creation time. It contains no seed, private key, DID, or credential material.
- Future ledger and proof dependencies must replace prototype-relative paths
  and mutable fork branches with the official GitHub sources and full commit
  pins defined in [the Midnight Git source policy](../dependencies/midnight-git-sources.md).

## First post-M0 slice: wallet presentation shell

ADR-0023 prioritizes the complete parity backlog in
[issue #2](https://github.com/MediaNoxLabs/oxid/issues/2). The first slice,
[issue #3](https://github.com/MediaNoxLabs/oxid/issues/3), reimplements the
recognizable navigation, design tokens, safe-area layout, and capability-status
surfaces. The precise source mapping and exclusions are recorded in
[ui-shell-provenance.md](ui-shell-provenance.md).

This is presentation parity, not functional parity. Assets, DIDs, credentials,
diagnostics, and settings expose only composed behavior and label missing
adapters as queued. Create Wallet Profile remains the only complete use case
until subsequent vertical slices land.

## Second post-M0 slice: standalone headless harness

[Issue #4](https://github.com/MediaNoxLabs/oxid/issues/4) establishes a
versioned NDJSON executable over the same UI-neutral composition used by the
mobile application. It implements capability discovery, Create Wallet Profile,
safe error recovery, and graceful shutdown. Its discovery result lists the
remaining wallet, vault, identity, credential, DID, and diagnostics operations
as queued rather than claiming them prematurely.

The implementation retains the useful one-request/one-response streaming model
and literal shutdown alias from the prototype. It deliberately does not retain
the mandatory startup seed, raw external errors, wallet-facade coupling, or the
bootstrap response containing `controllerSkHex`. ADR-0024 defines the durable
protocol and secret-handling boundary.

## Third post-M0 slice: integrated profile lifecycle

[Issue #1](https://github.com/MediaNoxLabs/oxid/issues/1) turns the M0 profile
form into the application entry point. First launch now gates on profile
creation, an existing public profile can be selected from onboarding or the
wallet profile page, and the active selection restores on a later launch. The
same create/list/select/active sequence is exposed through the headless harness
for deterministic flow testing.

The JSON adapter introduced by ADR-0025 persists only versioned public profile
metadata. It is not the prototype's key database or encrypted secret store and
does not resolve ADR-0017. Dioxus continues to call application use cases rather
than storage directly. Both mobile target graphs compile from the same
composition, with repository scripts providing local simulator/emulator smoke
entry points.

## Material intentionally excluded

Do not migrate these without explicit review:

- hard-coded demo/genesis seeds and `preprod_keys.json`;
- generated `.zkir`, `.bzkir`, prover, verifier, and managed artifacts;
- ledger-relative Cargo path dependencies;
- vendored npm/WASM packages and WebView JavaScript bridges;
- local endpoints, standalone secrets, Tailscale instructions, databases, and
  captured diagnostics;
- generated Android/iOS project output and signing configuration;
- benchmark-only probes, tabs, and telemetry panels.

## Fourth post-M0 slice: protected wallet boundaries

ADR-0017 decomposes the prototype's aggregate secret store into wallet
protection/session, key-operation, secret-blob, and user-authorization
capabilities. Oxid retains the boot-locked lifecycle, opaque references,
multi-curve metadata, confirmation before sensitive operations, and safe
lockout semantics. It permanently excludes the prototype's pre-filled
`midnight` passphrase, `seed_hex` wallet DTO, raw private-key/seed inputs on
ordinary ports, and accidental backup of device-bound ciphertext.

The first implementation is deliberately split by composition: the standalone
headless wallet can use a process-local development adapter for deterministic
flow testing, while production mobile composition reports custody unavailable
until native Keychain/Keystore adapters meet the accepted policy. The
development adapter is evidence for application sequencing and cryptographic
contracts, never for production secret protection.

## Fifth post-M0 slice: Midnight account read model

[Issue #6](https://github.com/MediaNoxLabs/oxid/issues/6) introduces
Oxid-owned network, account, address, asset, exact balance, synchronization, and
transaction-history types. Focused application ports keep the domain free of
SDK, transport, and UI types. Network selection is profile-scoped and network
identity contains no HTTP, WebSocket, node, indexer, or prover route.

`crates/adapters/midnight` supplies the seven reviewed Midnight network IDs,
Bech32m encoding checked against official public vectors, and exact NIGHT/DUST
unit semantics. Production composition returns an explicit unavailable
snapshot until native protected derivation and a production-approved live
source exist. Development and headless composition can bind a process-local
derived public address and clearly mark simulated data; balances and history
remain empty until an explicit connect/sync request.

The Assets page now renders the selected network, exact decimal balances,
source/sync truth, public receive addresses, and recent activity through the
same application use cases as the headless driver. The executable headless test
covers profile creation, network discovery and selection, protected derivation,
BIP340 signing, pre-sync state, explicit synchronization, balances, address HRP
changes, history, and rejected inputs. Detailed retained evidence and exclusions are recorded in
[midnight-account-provenance.md](midnight-account-provenance.md).

[Issue #9](https://github.com/MediaNoxLabs/oxid/issues/9) adds the next bounded
write slice. Oxid now prepares, previews, explicitly authorizes, expires, and
retrieves canonical unshielded NIGHT drafts with the pinned ledger types. The
headless executable covers that flow and never exposes the signing payload,
signature, or serialized transaction. ADR-0026 deliberately kept completion
outside that slice.

[Issue #11](https://github.com/MediaNoxLabs/oxid/issues/11) and ADR-0027 add the
next stage. The native adapter borrows only the canonical DUST child for one
worker, replays bounded DUST events, uses live chain parameters/time, converges
canonical DUST fees, proves locally or through the configured development proof server, seals and
tagged-serializes internally, submits the unsigned Midnight runtime call, and
returns public inclusion identifiers. Simulation exercises the same state,
confirmation, failure-restoration, worker-owned cancellation, and idempotency
contract without a network. An ambiguous node outcome remains `submitting` and
blocks a blind duplicate. Remote proving is development-only.

[Issue #12](https://github.com/MediaNoxLabs/oxid/issues/12) and ADR-0028 add the
private path. The same completion adapter can prove DUST spends on-device using
the official full-revision-pinned ZKIR provider and an authenticated app-private
cache. Local proofs are serialized on the existing worker and cancellation is
checked at every safe pre-broadcast boundary. A feature-gated fixture proves,
seals, and tagged-codec round-trips a real synthetic DUST spend; release runs on
arm64 iOS and Android simulators record k=13, 5,646 rows, proof/transaction
sizes, latency, and peak RSS without committing proving artifacts.

Issue #7 adds the next bounded account slice: native headless startup can opt
into a real v4 standalone-indexer WebSocket route and public unshielded address.
The executable harness contract-tests the protocol against an ephemeral local
fixture and truthfully distinguishes live refreshes from later cached reads.
No route is committed and normal mobile composition remains fail-closed.

[Issue #8](https://github.com/MediaNoxLabs/oxid/issues/8) adds protected
external NIGHT derivation. A generated process-local development root remains
inside `storage-dev`; typed BIP32 paths produce retained BIP340 keys, public
addresses, and opaque references. The same derived address replaces simulation
fixtures or the live source's configured watch-only fallback for that profile.
The real headless executable covers initialize/derive/repeat/sign/sync without
accepting or returning secret material.

[Issue #14](https://github.com/MediaNoxLabs/oxid/issues/14) and ADR-0029 connect
the same application services to Dioxus through an explicit
`standalone-development` app feature. The repository mobile launchers select
that feature for simulator/emulator flow testing; ordinary app builds keep
production composition unavailable. The Assets page can initialize or unlock
the ephemeral development wallet, derive the public external account, sync,
render a deterministic Rust/SVG receive QR, prepare and review exact NIGHT,
authorize the retained draft, and complete a simulated submission. No
prototype wallet facade, seed/key DTO, WebView JavaScript bundle, native
generated project, or live endpoint is copied.

[Issue #15](https://github.com/MediaNoxLabs/oxid/issues/15) and ADR-0030 migrate
the prototype backlog's public unshielded checkpoint/resume behavior without
copying its aggregate database boundary. The Midnight adapter persists a
versioned, bounded public replay snapshot keyed by network and address, restores
it as a cached read after process restart, and subscribes from the next cursor.
Malformed or incompatible state is rebuilt through one full replay. Cached
UTXOs cannot become spendable inputs until a live catch-up succeeds. The real
headless binary is exercised across three processes: initial replay,
incremental resume, then outage with preserved stalled state.

[Issue #16](https://github.com/MediaNoxLabs/oxid/issues/16) and ADR-0031 migrate
the prototype's persisted DUST replay behavior behind a distinct private
adapter store. The bounded binary envelope preserves the official tagged
`DustLocalState`, completed cursor, live-parameter identity, network, and a
one-way public-key fingerprint without persisting the DUST seed or scalar.
Standalone completion resumes from the next cursor and folds events in small
batches instead of retaining the prototype's history-sized queue. A current
checkpoint still needs a successful live subscription; wrong scope or
parameters replay cleanly, incompatible deltas retry once from zero, and
transport failure never authorizes a cached-only spend. Headless composition
accepts the store only with the complete standalone route set.

[Issue #17](https://github.com/MediaNoxLabs/oxid/issues/17) and ADR-0032 add the
prototype's explicit DUST sync lifecycle without copying its wallet facade or
history-sized channel. Oxid exposes owned start/status/cancel use cases,
executes native transport and official-state folding on an adapter worker,
persists each bounded completed batch as a resumable partial checkpoint, and
renders exact progress/balance in both the headless harness and Assets page.
Cached state remains visibly non-live and cannot independently authorize a
spend.

[Issue #18](https://github.com/MediaNoxLabs/oxid/issues/18) and ADR-0033 begin
the shielded slice at the custody/public-address boundary. Protected account
derivation now borrows the Wallet SDK role-3 child, builds official Zswap public
keys, and exposes the canonical network-specific shielded Bech32m address next
to the primary unshielded address. Headless responses and the Dioxus receive
list/QR use the same safe application projection. The seed, decryption key, and
nullifier material remain adapter-private. This replay increment adds a bounded
decoder for the official tagged `zswapLedgerEvents` payload and folds it into
the official local state with exact Merkle indices, local ownership plus
commitment verification, foreign-branch collapse, batch rehashing, and
nullifier spend removal. The following checkpoint increment persists the
official tagged state and partial cursor behind a bounded, checksummed,
owner-private, symlink-resistant, atomic binary store scoped by network,
source/protocol identity, and a one-way fingerprint of both public Zswap keys.
This increment adds an Oxid-owned status/start/cancel lifecycle with exact
per-token balances and note/commitment counts. Its deterministic standalone
controller verifies the protected role-3 child and drives headless plus mobile
cancellation/resume flows. Native live transport and checkpoint wiring remain
open in #18.

Shielded Zswap replay/checkpoints, internal/change
address management, replacement and
durable confirmation tracking, camera/copy/share bridges, explicit mobile
submission cancellation, production endpoint discovery, recovery, and native
custody remain separate follow-ups.

## Gate for each later slice

Every migrated capability needs:

1. Oxid-owned domain and application types;
2. focused incoming/outgoing ports;
3. one adapter with provenance and dependency review;
4. unit plus port-contract/integration tests;
5. security/privacy review for sensitive data or authorization;
6. an ADR when the architecture or dependency direction changes;
7. a Tier-1 mobile smoke test when user-facing.
