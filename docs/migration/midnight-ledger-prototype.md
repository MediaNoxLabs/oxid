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
| `wallet-core` address, HD, balances, transaction, sync | Midnight addresses, derivation, NIGHT/DUST, build/sign/submit, indexer/node access | chain-neutral chain domain/use cases plus `adapters/midnight` | Deferred to M2 |
| `wallet-core/secret_storage` | Multi-curve keys, encrypted files, redb, key references | platform key/secret ports plus platform-backed and development adapters | Deferred; security review required |
| `wallet-core/did` and DID services | `did:midnight` create/resolve/update/deactivate | identity domain/use cases plus `adapters/did-midnight` | Deferred to M5 |
| `wallet-core/oid4vci_client` and `oid4vp_client` | Credential issuance, SIOP/OID4VP response flows | credential/presentation application plus protocol adapters | Deferred to M4 |
| `wallet-core/vc_store` and `vc_self_verify` | Signed credential bytes, metadata, self-verification | credential domain/store/verification ports and adapters | Deferred to M3/M5 |
| `wallet-core/vault` | Passport-vault contract interaction and selective-disclosure claim | product-specific Midnight adapter/example, not generic wallet core | Deferred; separate ADR |
| `dioxus-wallet` | Mobile/desktop UI, QR bridges, JS eval bridge, DID/credential/vault screens | `ui-dioxus`, platform adapters, protocol/chain adapters | Profile onboarding/management and safe presentation shell reimplemented; capability pages deferred |
| `headless-wallet` | Line-delimited JSON driver for use cases | `apps/oxid-headless` incoming CLI/test adapter | Safe versioned transport and complete profile lifecycle implemented; chain/SSI flows queued |
| `prover-core` | Local/HTTP proof execution and benchmark paths | Midnight proving adapter | Deferred to M2 |
| benchmark crates and fixtures | Mobile proving measurements and test circuits | dedicated benchmarks/fixtures only when an adapter needs them | Not product code |
| Android/iOS projects | WebView hosts, permissions, QR bridges | `apps/oxid` platform hosts | Dioxus-generated hosts build and launch through repository scripts; native bridges remain deferred |

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

## Gate for each later slice

Every migrated capability needs:

1. Oxid-owned domain and application types;
2. focused incoming/outgoing ports;
3. one adapter with provenance and dependency review;
4. unit plus port-contract/integration tests;
5. security/privacy review for sensitive data or authorization;
6. an ADR when the architecture or dependency direction changes;
7. a Tier-1 mobile smoke test when user-facing.
