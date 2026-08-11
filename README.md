# Oxid Identity Wallet

[![CI](https://github.com/MediaNoxLabs/oxid/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/MediaNoxLabs/oxid/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Oxid is a free and open-source, Rust-first, identity-native wallet foundation.
It is designed for Android and iOS first, with desktop and web as secondary
targets. Crypto assets and self-sovereign identity are peer capabilities rather
than layers bolted onto one chain-specific frontend.

> **Status:** M0 foundation plus the first prototype-parity slices. The wallet
> profile lifecycle—create, list, select, persist, and restore—is available
> through Dioxus and the standalone headless harness. A development-only
> process-local adapter exercises opaque Ed25519/P-256 keys plus protected
> Midnight HD/BIP340 account derivation headlessly; a deterministic adapter
> exercises Midnight network, derived address, exact-balance, sync, and history
> flows without secret input. Native headless runs can instead
> opt into a real standalone-indexer unshielded account source using explicit
> public startup configuration. The Assets page consumes the same account use
> cases while production mobile custody and chain access remain fail-closed. The remaining shell
> destinations deliberately label unconnected capabilities; Oxid is not ready
> to hold real assets or identity credentials.

## Architecture

Oxid uses modular hexagonal architecture. Core types and use cases own their
boundaries; Dioxus, storage, operating systems, chains, and SSI libraries remain
replaceable adapters.

```text
apps/oxid --------> ui-dioxus --------+
                                        |
apps/oxid-headless ---------------------+--> wallet-application --> wallet-domain
          |                             |             |                    |
          +--> composition -------------+             v                    v
                    |                         platform-ports ------> foundation
                    +--> storage-json / storage-memory / storage-dev
                    +--> midnight (unavailable, simulated, or live headless source)
                    +--> platform-system
```

The detailed product and engineering definition is
[OXID_IDENTITY_WALLET_BLUEPRINT.md](OXID_IDENTITY_WALLET_BLUEPRINT.md). Accepted
decisions live in [docs/adr](docs/adr), and the staged prototype migration is
tracked in [docs/migration/midnight-ledger-prototype.md](docs/migration/midnight-ledger-prototype.md).
The complete parity backlog is [GitHub issue #2](https://github.com/MediaNoxLabs/oxid/issues/2).

## Quick start

Install [Nix](https://nixos.org/download/) with flakes enabled, then enter the
pinned development environment:

```bash
nix develop
./run.sh --light --strict
```

Launch the desktop shell:

```bash
cargo run -p oxid-app
```

Exercise the same application services through the versioned NDJSON harness:

```bash
printf '%s\n' '{"protocol":"oxid.headless.v1","id":"demo-1","method":"system.capabilities","params":{}}' | cargo run --quiet -p oxid-headless
```

Stdout is reserved for JSON responses. Start with `system.capabilities`; it
distinguishes implemented methods from queued parity work. The protocol never
accepts or returns raw private key, passphrase, recovery, or seed material. Its
key lifecycle is explicitly `development_only`, process-local, and ephemeral;
it is useful for conformance testing, not custody. Profile metadata persists in the
platform application-data directory by default; set
`OXID_PROFILE_STORE_PATH` to isolate an automation run.

The implemented account methods are `wallet.network.list`,
`wallet.network.select`, `wallet.account.derive`, `wallet.account.get`, `wallet.address.list`,
`wallet.address.unshielded`, `wallet.balance.snapshot`,
`wallet.transaction.history`, `wallet.connect`, and `wallet.sync.force`.
With no additional configuration their account data is explicitly `simulated`
and contacts no node, indexer, or prover. After
`wallet.security.initialize`, `wallet.account.derive` creates and retains the
canonical external NIGHT child key inside the process-local development
adapter, then returns only its public address and opaque transaction-key
reference. Account and address indices must be below `2^31`; seed, mnemonic,
private-key, and caller-defined path parameters are rejected.

For a native standalone-indexer run, set all three public values before starting
the headless binary:

```bash
export OXID_MIDNIGHT_NETWORK_ID='<network-id>'
export OXID_MIDNIGHT_INDEXER_WS_URL='<graphql-websocket-url>'
export OXID_MIDNIGHT_UNSHIELDED_ADDRESS='<public-unshielded-address>'
cargo run -p oxid-headless
```

The route must use `ws` or `wss` without credentials, query parameters, or a
fragment. The Bech32m address HRP must match the selected network. Supplying
only part of the configuration fails startup. A successful refresh reports
`live`; subsequent in-process reads report `cached`. The configured address is
an initial watch-only fallback; deriving an account binds subsequent sync to
the derived public address. This mode does not import recovery material, sync
shielded/DUST state, construct, prove, or submit transactions.

Common commands are also exposed through `just`:

```bash
just check
just test
just coverage
just run
just headless
just full
```

The Dioxus package has `desktop`, `mobile`, and `web` feature boundaries. The
desktop feature is the default for the first slice. On macOS with Xcode and
Rustup installed, build, install, and launch the mobile feature in an available
iPhone simulator with:

```bash
just ios-run
```

Set `OXID_IOS_DEVICE` to a simulator UDID to select a particular device. The
script obtains the pinned Dioxus CLI from the locked Nix flake but deliberately
uses the host Xcode and Rustup toolchain for Apple SDK discovery. Generated
platform output and signing state remain uncommitted; secure storage arrives as
an explicit mobile adapter.

With an Android SDK/NDK and a connected device or configured AVD, build,
install, and launch the same mobile feature with:

```bash
just android-run
```

Set `OXID_ANDROID_DEVICE` to an adb serial or `OXID_ANDROID_AVD` to an AVD name
when automatic selection is not appropriate.

The focused profile smoke tests reset Oxid's app data on their selected
simulator/emulator, create and select the default profile, restart the process,
and assert restoration:

```bash
just ios-smoke
just android-smoke
```

## Repository layout

| Path | Responsibility |
| --- | --- |
| `apps/oxid` | Executable shell and Dioxus launch configuration. |
| `apps/oxid-headless` | Versioned NDJSON flow and integration-test harness. |
| `crates/foundation` | Small Oxid-owned primitives. |
| `crates/wallet/domain` | Wallet entities and invariants. |
| `crates/wallet/application` | Use cases and wallet-owned ports. |
| `crates/platform/ports` | Time and randomness capability ports. |
| `crates/adapters` | Replaceable outgoing implementations. |
| `crates/ui-dioxus` | Incoming Dioxus UI adapter. |
| `crates/composition` | Static dependency wiring. |
| `docs/adr` | Architecture decision records. |

## Prototype migration

The capable Midnight/SSI prototype was researched at
`midnight-ledger` commit
`074b1a4bccbfee1740ee188374b606a022ecef42`, branch
`feat/mobile-prototype`, under `mobile-bench/`. Its features will be migrated
in vertical slices. Ledger-relative dependencies, demo seeds, pre-production
keys, generated proof artifacts, and vendored JavaScript are intentionally not
carried into M0.

The first post-M0 slice reimplements the prototype's recognizable mobile wallet
shell without its SDK coupling. Its exact retained/excluded surface is recorded
in [docs/migration/ui-shell-provenance.md](docs/migration/ui-shell-provenance.md).
The profile page is retained and now owns integrated onboarding, selection, and
public-metadata persistence. Custody and protected secrets remain explicitly
outside that record.

The first Midnight read adapter has no direct ledger/proof dependency because
it does not consume their runtime types. Future transaction and proof adapters
must use official GitHub URLs with immutable commit pins; the selected baseline,
build-trial result, and policy are recorded in
[docs/dependencies/midnight-git-sources.md](docs/dependencies/midnight-git-sources.md).

## Security

The JSON repository is durable only for public profile metadata; it is not a
secret store. The software signing and HD-derivation adapter is process-local development/test
infrastructure and production composition does not select it. Never use this
milestone to custody assets or credentials. See
[SECURITY.md](SECURITY.md) for reporting and the current threat boundaries.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) and [AGENT.md](AGENT.md) before making
changes. Contributions require DCO sign-off, and repository-facing commits must
be GPG signed.

Oxid is licensed under the [Apache License 2.0](LICENSE). Retained icon notices
are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
