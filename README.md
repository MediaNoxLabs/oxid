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
> exercises Midnight network, canonical unshielded and shielded receive
> addresses, exact-balance, sync, history, and
> staged unshielded NIGHT submission, durable public submission recovery, and
> finalized-chain reconciliation without secret input. Native headless runs
> can instead opt into a real standalone-indexer source for public-account and
> shielded Zswap synchronization, or the complete DUST/local-prover/node
> submission path using explicit public startup configuration; remote proving
> remains an explicit development option. The
> Assets page consumes the same account and transaction use cases. The
> repository simulator/emulator launchers explicitly select process-local
> development custody so receive QR plus prepare/review/authorize/submit can be
> exercised end to end; normal production composition remains fail-closed. The remaining shell destinations deliberately label unconnected
> capabilities; Oxid is not ready to hold real assets or identity credentials.

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
`wallet.address.unshielded`, `wallet.address.shielded`, `wallet.balance.snapshot`,
`wallet.transaction.history`, `wallet.transaction.prepare_unshielded`,
`wallet.transaction.authorize_unshielded`, `wallet.transaction.draft`,
`wallet.transaction.submit_unshielded`, `wallet.transaction.send_unshielded`,
`wallet.transaction.start_submission`, `wallet.transaction.submission_status`,
`wallet.transaction.submission_history`, `wallet.transaction.reconcile_submission`,
`wallet.transaction.cancel_submission`,
`wallet.connect`, `wallet.sync.force`, `wallet.dust.sync.status`,
`wallet.dust.sync.start`, `wallet.dust.sync.cancel`,
`wallet.shielded.sync.status`, `wallet.shielded.sync.start`, and
`wallet.shielded.sync.cancel`.
With no additional configuration their account data is explicitly `simulated`
and contacts no node, indexer, or prover. After
`wallet.security.initialize`, `wallet.account.derive` creates and retains the
canonical external NIGHT child key and role-3 Zswap receive keys inside the
process-local development adapter, then returns only their public addresses and
the opaque transaction-key reference. Account and address indices must be below
`2^31`; seed, mnemonic, private-key, and caller-defined path parameters are
rejected.

After derivation and sync, the transaction methods prepare an exact native
NIGHT preview, authorize its retained canonical ledger intent through the
opaque development key reference, submit it, and query draft state. The
zero-configuration harness completes submission deterministically and labels
the outcome `simulated`; it covers state/error/idempotency flows without
contacting a node or prover. Live standalone mode synchronizes the DUST child,
balances canonical fees, proves DUST spends locally when configured with an
app-private cache, submits `Midnight.send_mn_transaction` unsigned, and returns only successful
public transaction/block identifiers. No method returns signing payloads,
signatures, proof witnesses, derived secrets, or serialized transactions.
The asynchronous start/status/cancel methods expose a deliberate
pre-broadcast cancellation window. Once node broadcast begins, cancellation is
refused; an acknowledged cancellation restores the authorized draft for an
explicit retry.
The DUST methods expose only exact atomic balance, bounded cursor progress,
freshness, and sanitized state. Cached or cancelled checkpoints remain
resumable but are never labelled live enough to spend.
If transport is lost after node submission, the adapter reports
`submission_unknown` and leaves the draft `submitting`; it never risks a blind
duplicate while the external outcome is ambiguous. The adapter durably records
the public extrinsic hash and finalized pre-broadcast anchor before contacting
the node. Submission status/history survive restart, and explicit
reconciliation scans a bounded finalized ancestor window before classifying an
attempt as included, rejected, expired, or still unknown.

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
the derived public address. This read-only live mode does not import recovery
material, sync DUST state, prove, or submit transactions. It can run the
explicit protected shielded sync lifecycle; without a checkpoint path that
state lasts only for the process.

To restore public unshielded balances/history after restart and resume from the
next indexer cursor, optionally provide an absolute app-private file path:

```bash
export OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH='<absolute-app-private-checkpoint-file>'
cargo run -p oxid-headless
```

The versioned file contains only bounded public replay state and is written
atomically with owner-only permissions. A restored view is labeled `cached`;
new transaction inputs remain unavailable until a live synchronization
succeeds. Invalid state is ignored and rebuilt from cursor zero. The path by
itself is incomplete configuration and fails startup.

To enable the complete private standalone submission path, supply the same
three values plus the indexer/node routes and an absolute app-private cache:

```bash
export OXID_MIDNIGHT_INDEXER_HTTP_URL='<graphql-http-url>'
export OXID_MIDNIGHT_NODE_WS_URL='<node-websocket-url>'
export OXID_MIDNIGHT_PROVING_CACHE_DIR='<absolute-app-private-cache-path>'
cargo run -p oxid-headless
```

The local cache accepts only hash-pinned official DUST artifacts, is bounded to
8 MiB, and never stores witnesses. To use the remote development alternative,
unset the cache variable and set the proof-server route instead:

```bash
unset OXID_MIDNIGHT_PROVING_CACHE_DIR
export OXID_MIDNIGHT_PROOF_SERVER_URL='<proof-server-base-url>'
cargo run -p oxid-headless
```

The five common route/address values and exactly one proving mode must be
present together. Proof-server HTTP is accepted only on loopback; remote
proving requires HTTPS. The proof server receives private witness material, so
that mode is development-only. The root is process-local and ephemeral; fund
and exercise a newly derived address in the same run.

For a complete standalone run, DUST replay can also resume from a private
key-scoped checkpoint:

```bash
export OXID_MIDNIGHT_DUST_CHECKPOINT_PATH='<absolute-app-private-dust-checkpoint-file>'
cargo run -p oxid-headless
```

This versioned binary file contains the official tagged DUST wallet state,
completed cursor, network identity, parameter identity, and a one-way public
DUST-key fingerprint. It never contains the DUST seed or secret scalar. Every
submission still fetches current parameters and completes a live indexer
catch-up before cached state may be used for balancing. Wrong-scope or changed
parameters cause a clean replay, an incompatible delta retries once from zero,
and transport failure fails the submission closed. The DUST checkpoint path is
invalid with simulation or the read-only live-indexer configuration.

Either live indexer mode can persist protected shielded replay state when an
absolute app-private file path is supplied:

```bash
export OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH='<absolute-app-private-shielded-checkpoint-file>'
cargo run -p oxid-headless
```

The native worker borrows the role-3 Zswap child only inside custody, resumes
`zswapLedgerEvents` at the next cursor, folds bounded batches through the
official local state machine, and atomically saves each consistent batch. The
checksummed v1 binary store is bounded to four key/network/source-scoped
records, 32 MiB per tagged state, and 128 MiB total. It contains no seed,
secret scalar, endpoint, profile metadata, proof, or witness. Cached,
cancelled, or stalled projections are display/resume state only. Invalid state
is ignored and rebuilt from zero; an incompatible delta retries once from
zero. Development roots remain ephemeral, so useful cross-process resume
awaits durable native custody of the same root.

Standalone development composition automatically keeps bounded public
submission metadata in an owner-private journal beside the resolved profile
store. Headless automation may select an explicit normalized absolute path:

```bash
export OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH='<absolute-app-private-submission-journal>'
cargo run -p oxid-headless
```

The v1 JSON journal is capped at 128 records and 256 KiB and is atomically
written before network broadcast. It contains profile/network/draft scope, a
one-way planning fingerprint, expiry/update times, fee, extrinsic/finalized
anchor hashes, optional inclusion block, mode, and state—never signed or sealed
transactions, signatures, proofs, witnesses, keys, seeds, or routes. The path
can also be used with deterministic simulation for multi-process flow tests.
For live reconciliation it must accompany the complete standalone submission
configuration; it is intentionally rejected with the read-only live stack.

An opt-in headless proving harness constructs one synthetic DUST spend, proves
and seals it locally, and checks tagged-codec interoperability without node
submission. It emits bounded first/warm JSON measurements and commits no proof
artifacts:

```bash
export OXID_MIDNIGHT_PROVING_CACHE_DIR='<absolute-app-private-cache-path>'
cargo run --release -p oxid-adapter-midnight \
  --features proving-bench --example local-proving
```

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

The repository iOS and Android launch scripts explicitly enable
`oxid-app/standalone-development`. This composes the same deterministic
development wallet as the headless harness: public profiles persist, protected
roots and drafts are process-local, no chain service is contacted, and the UI
labels simulated results. A normal `cargo run -p oxid-app` does not enable this
feature and stays fail-closed.

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

The focused wallet smoke tests reset Oxid's app data on their selected
simulator/emulator, create the default profile, activate the protected
development account, render receive QR, complete a staged simulated transfer,
restart the process, and assert public-profile restoration:

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

The Midnight read model uses owned types, while its native canonical transaction
and local-proving adapter consumes full-revision-pinned official ledger
packages. The selected baseline, dependency reviews, and source policy are recorded in
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
