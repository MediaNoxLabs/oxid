# Oxid Identity Wallet

[![CI](https://github.com/MediaNoxLabs/oxid/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/MediaNoxLabs/oxid/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Oxid is a free and open-source, Rust-first, identity-native wallet foundation.
It is designed for Android and iOS first, with desktop and web as secondary
targets. Crypto assets and self-sovereign identity are peer capabilities rather
than layers bolted onto one chain-specific frontend.

> **Status:** M0 foundation plus the first prototype-parity presentation slice.
> Create Wallet Profile is still the only complete use case. The Assets, DIDs,
> Credentials, Diagnostics, and Settings shell deliberately labels unconnected
> capabilities; Oxid is not ready to hold real assets or identity credentials.

## Architecture

Oxid uses modular hexagonal architecture. Core types and use cases own their
boundaries; Dioxus, storage, operating systems, chains, and SSI libraries remain
replaceable adapters.

```text
apps/oxid
    |
    +--> ui-dioxus --------> wallet-application --> wallet-domain
    |                                |                    |
    +--> composition                 v                    v
             |                platform-ports ------> foundation
             +--> storage-memory
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

Common commands are also exposed through `just`:

```bash
just check
just test
just coverage
just run
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

## Repository layout

| Path | Responsibility |
| --- | --- |
| `apps/oxid` | Executable shell and Dioxus launch configuration. |
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
The profile page is retained during migration; final onboarding and profile
selection are tracked in [issue #1](https://github.com/MediaNoxLabs/oxid/issues/1).

M0 has no Midnight Cargo dependency. Future ledger and proof adapters must use
official GitHub URLs with immutable commit pins; the policy and current upstream
observations are recorded in
[docs/dependencies/midnight-git-sources.md](docs/dependencies/midnight-git-sources.md).

## Security

The current in-memory repository is development-only: it is neither durable nor
a secret store. Never use this milestone to custody assets or credentials. See
[SECURITY.md](SECURITY.md) for reporting and the current threat boundaries.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) and [AGENT.md](AGENT.md) before making
changes. Contributions require DCO sign-off, and repository-facing commits must
be GPG signed.

Oxid is licensed under the [Apache License 2.0](LICENSE). Retained icon notices
are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
