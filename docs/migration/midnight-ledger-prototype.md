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
| `wallet-core` profile/wallet service concepts | Wallet construction, service façade, UI port | `wallet/domain`, `wallet/application`, focused ports | M0 profile slice reimplemented |
| `wallet-core` address, HD, balances, transaction, sync | Midnight addresses, derivation, NIGHT/DUST, build/sign/submit, indexer/node access | chain-neutral chain domain/use cases plus `adapters/midnight` | Deferred to M2 |
| `wallet-core/secret_storage` | Multi-curve keys, encrypted files, redb, key references | platform key/secret ports plus platform-backed and development adapters | Deferred; security review required |
| `wallet-core/did` and DID services | `did:midnight` create/resolve/update/deactivate | identity domain/use cases plus `adapters/did-midnight` | Deferred to M5 |
| `wallet-core/oid4vci_client` and `oid4vp_client` | Credential issuance, SIOP/OID4VP response flows | credential/presentation application plus protocol adapters | Deferred to M4 |
| `wallet-core/vc_store` and `vc_self_verify` | Signed credential bytes, metadata, self-verification | credential domain/store/verification ports and adapters | Deferred to M3/M5 |
| `wallet-core/vault` | Passport-vault contract interaction and selective-disclosure claim | product-specific Midnight adapter/example, not generic wallet core | Deferred; separate ADR |
| `dioxus-wallet` | Mobile/desktop UI, QR bridges, JS eval bridge, DID/credential/vault screens | `ui-dioxus`, platform adapters, protocol/chain adapters | M0 profile screen reimplemented |
| `headless-wallet` | Line-delimited JSON driver for use cases | future incoming CLI/test adapter | Deferred; retain protocol lessons |
| `prover-core` | Local/HTTP proof execution and benchmark paths | Midnight proving adapter | Deferred to M2 |
| benchmark crates and fixtures | Mobile proving measurements and test circuits | dedicated benchmarks/fixtures only when an adapter needs them | Not product code |
| Android/iOS projects | WebView hosts, permissions, QR bridges | `apps/oxid` platform hosts | Deferred until mobile capability slice |

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
