# Oxid

[![CI](https://github.com/MediaNoxLabs/oxid/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/MediaNoxLabs/oxid/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**A mobile-first wallet for crypto and digital identity, built in the open.**

Oxid explores how one wallet can help people manage assets, digital
identifiers, and verifiable credentials while staying in control of their keys
and what they share. It is also a reusable Rust foundation for teams building
wallet products. Android and iOS are the primary targets; desktop and a
command-driven test app help us develop and verify the same core.

The prototype focuses on Midnight today. Support for more chains and identity
systems is a goal of the architecture, not a claim about what the app can do
now.

> [!WARNING]
> **Oxid is in active prototyping and is not a production wallet.** Do not use
> it to hold real assets, production identity keys, or externally issued
> credentials. Development builds may use simulated data or explicitly
> configured test infrastructure. The normal app keeps capabilities
> unavailable until their security and platform requirements have been met.

## What we are building

- **One place for assets and identity.** A person should be able to see wallet activity, manage decentralized identifiers (DIDs) and credentials, and understand which action they are taking.
- **Clear consent and honest status.** Sending value and sharing identity data require separate, explicit approval. The app distinguishes simulated, cached, unavailable, and live information.
- **A foundation others can reuse.** Core rules are written in Rust. A chain,
  identity protocol, storage system, or interface can be connected without
  rewriting those rules.

The standalone development app exercises wallet, DID, credential, and consent
flows. Some flows use test fixtures; others can connect to explicitly
configured Midnight test infrastructure. [Delivery status](docs/site/src/status.md)
explains which capabilities exist in each mode and what still blocks a
shippable wallet.

## Find your way around

| If you want to… | Start here |
| --- | --- |
| Build or run Oxid | [Getting started](docs/site/src/getting-started.md) |
| See what works today and what is still planned | [Delivery status](docs/site/src/status.md) and the [issue backlog](https://github.com/MediaNoxLabs/oxid/issues) |
| Understand the product and code structure | [Development blueprint](OXID_IDENTITY_WALLET_BLUEPRINT.md), [architecture guide](docs/site/src/architecture.md), and [decision records](docs/adr/README.md) |
| Explore the mobile experience | [Design specification](docs/design/README.md) |
| Run a specific prototype or test flow | [Detailed development notes](docs/detailed-development-notes.md) |
| Explore the supervised Android demo | [Operator demo kit](demo/README.md) |
| Report a security concern or contribute | [Security policy](SECURITY.md) and [contribution guide](CONTRIBUTING.md) |

## Try the development environment

Install [Nix with flakes enabled](https://nixos.org/download/), then run the
repository checks in the pinned toolchain:

```bash
git clone https://github.com/MediaNoxLabs/oxid.git
cd oxid
./bootstrap.sh -- just check
```

The first run downloads a sizeable toolchain. To launch a development app on an
iOS Simulator or Android emulator, follow the platform prerequisites and
commands in [Getting started](docs/site/src/getting-started.md). Those builds
are for experimentation and testing; the default app deliberately leaves
unfinished features unavailable.

## Repository map

| Path | What it contains |
| --- | --- |
| [`apps/oxid`](apps/oxid/) and [`crates/ui-dioxus`](crates/ui-dioxus/) | The app entry point and shared Dioxus interface. |
| [`apps/oxid-headless`](apps/oxid-headless/) | A command-driven app for testing flows without a screen. |
| [`crates/`](crates/) | Wallet, identity, credential, presentation, and protocol rules and use cases. |
| [`crates/adapters`](crates/adapters/) and [`crates/composition`](crates/composition/) | Integrations and the explicit wiring that selects them. |
| [`brands/oxid`](brands/oxid/) and [`docs/design`](docs/design/) | The current brand assets and mobile design direction. |
| [`docs/factory`](docs/factory/) | The repository's build, review, and delivery system. |

The dependency direction is deliberate: the app and integrations depend on the
core, while the core does not depend on a particular UI, blockchain SDK, or
storage engine. The [architecture guide](docs/site/src/architecture.md) explains
the boundaries in detail.

## Contributing

Issues and pull requests are welcome. Start with
[CONTRIBUTING.md](CONTRIBUTING.md) for the development setup and review process.
Changes use an issue-backed branch and signed commits; the
[contribution policy](docs/factory/contribution-policy.md) has the exact rules.

Oxid is licensed under [Apache 2.0](LICENSE). Third-party notices are in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
