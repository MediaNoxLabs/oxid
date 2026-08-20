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
> process-local adapter exercises opaque Ed25519/P-256/Jubjub keys plus protected
> Midnight HD/BIP340 account derivation headlessly; a deterministic adapter
> exercises Midnight network, canonical unshielded and shielded receive
> addresses, exact-balance, sync, history, and
> staged unshielded NIGHT submission, durable public submission recovery, and
> finalized-chain reconciliation without secret input. The first peer identity
> slice resolves current `did:midnight` public documents into a profile-scoped
> inventory through standalone or explicitly configured native adapters. A
> deterministic OpenID4VCI 1.0 Final adapter now exercises embedded-offer
> preview, explicit consent, DID-bound proof, strict verification, and protected
> credential storage end to end. A separate deterministic SIOPv2 draft-13
> adapter previews a standalone verifier request, requires explicit consent,
> and independently verifies a single-use self-issued DID login without
> exposing the ID Token. The standalone issuer now delivers the prototype's
> exact `midnight_compact_vc` body shape, detached issuance proof, and five
> commitment-bound protected claims. It reissues that exact bundle to the
> selected profile's managed Jubjub assertion method, and a native verifier
> checks the resulting Compact roots and proof before encrypted storage. Exact
> development Jubjub signing stays behind opaque custody references. Normal
> mobile composition now seals the same multi-curve vault behind iOS Keychain
> or Android Keystore user presence. Active standalone verification now binds
> the exact issuer DID assertion key to the Compact proof, enforces current
> issuance/expiry policy, and requires the pinned standalone trust anchor while
> revocation remains explicitly not checked. One authenticated complete-wallet
> archive now restores the profile, Midnight association, DIDs, credentials,
> and custody into an empty installation through standalone composition and
> first-run Dioxus. Production issuer/status policy, complete mobile picker and
> physical-device recovery evidence, and mobile Compact proving remain explicit
> later gates.
> Headless can inspect a claim-free
> disclosure plan, while Dioxus explicitly reveals/hides first and last name
> locally and plans an age predicate without claiming a presentation or proof.
> Native headless runs
> can instead opt into a real standalone-indexer source for public-account and
> shielded Zswap synchronization, or the complete DUST/local-prover/node
> submission path using explicit public startup configuration; remote proving
> remains an explicit development option. The
> Assets, DIDs, and Credentials pages consume the same application use cases. The
> repository simulator/emulator launchers explicitly select process-local
> development custody so receive QR plus prepare/review/authorize/submit can be
> exercised end to end; normal production composition remains fail-closed. The remaining shell destinations deliberately label unconnected
> capabilities; Oxid is not ready to custody real assets, production identity
> keys, or externally issued credentials.

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
                    +--> identity-application --> identity-domain
                    |         ^                       ^
                    |         +-- DID resolver / public DID JSON adapters
                    +--> credential-application --> credential-domain
                    |         ^
                    |         +-- Midnight verifier/disclosure / encrypted storage adapters
                    +--> protocol-application --> protocol-domain
                    |         ^
                    |         +-- OpenID4VCI / SIOPv2 / verified credential adapters
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
./bootstrap.sh
./run.sh --light --strict
```

The bootstrap wrapper also starts the pinned Pi installation, validates its
project-local review integration, or runs a one-off command in the shell:

```bash
./bootstrap.sh --pi
./bootstrap.sh --check
./bootstrap.sh -- cargo test --workspace
```

It does not read, print, or persist credentials. The Nix shell remains the
single package-provisioning boundary; `nix develop` continues to work directly.

Launch the desktop shell:

```bash
cargo run -p oxid-app
```

The default thin app validates and embeds `brands/oxid` at build time. Check
every pack, or inspect the default generated semantic CSS, with:

```bash
./scripts/check-brand-packs.sh
cargo run -p oxid-brand-build --bin oxid-brand-check -- --css brands/oxid
```

Brand packs cannot select wallet adapters, protocols, custody, trust, consent,
or safety copy. See [ADR-0092](docs/adr/0092-generate-validated-build-time-brand-packs.md)
and the [white-label design](docs/design/white-label.md).

Launch the fully capable standalone mobile application with deterministic,
process-local development custody:

```bash
just ios-run
just android-run
```

For the standalone-only developer presentation, including the persistent build
banner and the shared `system.capabilities` viewer, use:

```bash
just ios-dev
just android-dev
```

These are aliases for `OXID_UI_PROFILE=dev` on the same launchers. They do not
change custody, storage, fixtures, or network composition; normal release
builds exclude the developer profile.

Focused simulator/emulator checks prove the banner is present before
onboarding and that the developer route renders the safe shared manifest:

```bash
just ios-dev-smoke
just android-dev-smoke
```

For the compile-time standalone demo presentation, use:

```bash
just ios-demo
just android-demo
```

The non-dismissible banner identifies fixture data. Its opt-in drawer can
idempotently select or create the isolated `Oxid Demo Wallet` profile, leaving
unrelated active profiles untouched, initialize or unlock standalone custody,
derive account `0/0`, create a managed DID, receive the public inbox fixture,
and load funding only from the exact undeployed simulator. Offer, login, and
presentation actions stop on their existing review screens and never automate
consent, authorization, proving, or submission. Normal and native-custody builds
reject this profile, and normal release artifacts exclude its code markers.

Focused fresh-install evidence is available after the standard UI gates pass:

```bash
just ios-demo-smoke
just android-demo-smoke
```

To exercise the same standalone wallet/SSI stack through native device custody,
select it explicitly:

```bash
OXID_MOBILE_CUSTODY=native just ios-run
OXID_MOBILE_CUSTODY=native just android-run
```

An opt-in standalone conformance build embeds and authenticates the exact
Compact presentation runtime package and enables one foreground proof worker:

```bash
OXID_MOBILE_CUSTODY=native OXID_MOBILE_PRESENTATION_PROVING=artifacts just ios-run
OXID_MOBILE_CUSTODY=native OXID_MOBILE_PRESENTATION_PROVING=artifacts just android-run
```

The launchers resolve the artifact package from the pinned Nix derivation and
print the resulting app/APK byte count. This explicit mode can generate and
independently verify the standalone OpenID4VP proof. Cancellation,
backgrounding, and timeout discard the result only after the worker stops; a
retry requires a fresh preview and consent. It remains an experimental
simulator/emulator harness, not physical-device or production readiness. The
ordinary `just ios-run` and `just android-run` paths remain unchanged and
proof-disabled.

`just ios-native-custody-smoke` accepts either a supported passcode-bound
Keychain capability or a truthful fail-closed simulator result. The Android
counterpart performs the full system-credential and restart test, but is
intentionally restricted to a disposable emulator with no existing PIN:

```bash
OXID_ANDROID_DEVICE=emulator-5554 just android-native-custody-smoke
```

Run the complete-wallet transaction without exposing recovery material through
the NDJSON protocol:

```bash
just standalone-recovery-smoke
```

That in-process standalone harness creates an exact Midnight account, managed
DID, holder-bound private credential, and protected custody; exports one v2
archive; and recovers all stores into a fresh composition. The public
`oxid.headless.v1` protocol intentionally has no backup or recovery method.

To exercise the same complete export/recovery UI with process-local standalone
custody on iOS Simulator, launch a clean development build:

```bash
OXID_IOS_RESET_DATA=1 OXID_MOBILE_CUSTODY=development just ios-run
```

Create and exercise the wallet, then use **Settings → Export complete wallet
backup** and save `oxid-wallet.oxidbak` through Files. Run the same clean-launch
command again and choose **Restore your complete wallet** on the first screen.
The recovery secret is never stored in the app. Resetting app data is
destructive to the selected simulator's local Oxid state, so keep the exported
document outside the app container before doing so.

The same standalone simulator build can exercise protected NIGHT spending.
Create and activate a development wallet, connect the account, run **Sync
shielded assets** until its state is **Synced**, then choose **Shielded NIGHT**
in the Assets send card. Select **Use my receive address** to fill the shielded
receive address, enter an amount no greater than the deterministic 5 NIGHT
balance, review the
exact privacy/amount/change preview, authorize, and submit. The resulting
transaction and block identifiers are standalone simulation evidence, not
live-chain inclusion. Simulator results do not satisfy the physical-device
custody, proving latency, memory, lifecycle, or thermal release gates.

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
The standalone Passport Vault ledger is a separate owner-private bounded file
at `private/passport-vault.json` beside that profile store. It preserves local
lock accounting and claim replay across restarts without becoming chain state;
set `OXID_PASSPORT_VAULT_STORE_PATH` to a normalized absolute file path when an
isolated harness route is required.

Inspect the bounded, payload-free runtime-health ring through the same
standalone process:

```bash
printf '%s\n' '{"protocol":"oxid.headless.v1","id":"health-1","method":"system.diagnostics.snapshot","params":{}}' | cargo run --quiet -p oxid-headless
```

The Dioxus **Diagnostics → Process-local diagnostics** panel exposes the same
closed codes. Telemetry, persistence, uploads, request payloads, endpoints,
credential data, and transaction material are not retained. Clearing the
headless ring requires `confirmed: true` plus the exact
`CLEAR_LOCAL_DIAGNOSTICS` intent; the ring also disappears on process exit.

The implemented account methods are `wallet.network.list`,
`wallet.network.select`, `wallet.account.derive`, `wallet.account.get`, `wallet.address.list`,
`wallet.address.unshielded`, `wallet.address.shielded`, `wallet.balance.snapshot`,
`wallet.transaction.history`, `wallet.transaction.prepare_unshielded`,
`wallet.transaction.prepare_shielded`, `wallet.transaction.authorize_unshielded`,
`wallet.transaction.authorize_shielded`, `wallet.transaction.draft`,
`wallet.transaction.submit_unshielded`, `wallet.transaction.send_unshielded`,
`wallet.transaction.submit_shielded`, `wallet.transaction.send_shielded`,
`wallet.transaction.start_submission`, `wallet.transaction.submission_status`,
`wallet.transaction.submission_history`, `wallet.transaction.reconcile_submission`,
`wallet.transaction.cancel_submission`,
`wallet.connect`, `wallet.sync.force`, `wallet.dust.sync.status`,
`wallet.dust.sync.start`, `wallet.dust.sync.cancel`,
`wallet.shielded.sync.status`, `wallet.shielded.sync.start`, and
`wallet.shielded.sync.cancel`. The implemented identity methods are
`did.create`, `did.resolve`, `did.list`, `did.get`, `did.update`, `did.sign`,
`did.deactivate`, and `did.forget`. Credential inventory methods are
`credential.receive`, `credential.list`, `credential.get`,
`credential.reverify`, `credential.delete`,
`credential.disclosure.candidates`, and `credential.disclosure.preview`.
Disclosure responses contain labels, paths, privacy tiers, and the selected
plan only—never claim values, openings, or presentation secrets. Standalone issuance adds
`credential.issuance.prepare`, `credential.issuance.accept`,
`credential.issuance.refuse`, `credential.issuance.get`, and
`credential.issuance.list`; their profile scope is always taken from the
active wallet profile rather than caller parameters. Acceptance uses
`methodId` for the OpenID authentication proof and a distinct
`holderBindingMethodId` for the managed Jubjub assertion method signed into the
Compact credential holder reference.
Standalone presentation preview adds `credential.presentation.prepare`,
`credential.presentation.accept`, `credential.presentation.refuse`,
`credential.presentation.get`, and `credential.presentation.list`. It strictly
matches a Final-shaped DCQL request and requires exact consent. Acceptance
re-verifies the encrypted exact Compact credential/private-opening bundle,
constructs and independently checks the generated circuit's public statement,
and always consumes the session. The default standalone composition then fails
closed with `proof_unavailable`. A native headless process launched from
`nix develop` uses the explicit authenticated `OXID_PRESENTATION_ARTIFACTS_DIR`
closure to create the real k=18 Compact proof, independently verify it, and
validate an internal `vp_token`; ordinary headless views expose neither proof
nor token bytes. ADR-0083 composes the same checked proof and independent
verification only in the explicit native-custody mobile artifact build; normal
mobile remains fail-closed.
Standalone self-issued login adds `identity.authentication.prepare`,
`identity.authentication.accept`, `identity.authentication.refuse`,
`identity.authentication.get`, and `identity.authentication.list`. Results are
metadata-only and never contain a nonce, state, signing input, or ID Token.
The prototype-oriented `identity.login` name is a prepare-only compatibility
alias so explicit consent cannot be bypassed.
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

With no DID configuration, explicit standalone development composition resolves
only this deterministic public fixture and returns not-found for every other
identifier:

```text
did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

Native headless runs may select the official resolver-service HTTP contract:

```bash
export OXID_MIDNIGHT_DID_RESOLVER_URL='<resolver-base-url>'
export OXID_DID_STORE_PATH='<absolute-app-private-public-did-file>' # optional
cargo run -p oxid-headless
```

The resolver base URL must use HTTPS, except for loopback HTTP, and may not
contain credentials, a query, or a fragment. Redirects and ambient proxies are
disabled. Responses are bounded and fully revalidated before the separate
versioned public DID store is changed. That store contains no private JWK,
credential, claim, token, route, or recovery material. Normal production
composition leaves both identity ports unavailable; this is not DID lifecycle
mutation or production identity custody.

Standalone composition accepts exactly one embedded, pre-authorized-code
OpenID4VCI 1.0 Final offer without Transaction Code. It previews issuer and
credential display metadata before explicit consent, signs a nonce-bound JWT
through an active managed DID authentication method, separately selects a
managed Jubjub assertion method for holder binding, verifies the issued
Midnight credential, and stores it in the protected profile inventory. It uses
the exact prototype `midnight_compact_vc` representation: separately bounded
body, detached issuance proof, and private openings. The standalone issuer
canonically replaces the credential holder reference with the selected DID and
method, rebuilds the detached proof over the changed root, and never receives a
private key reference. The adapter reconstructs the exact claim/body/payload
roots and verifies the Jubjub Schnorr equation; issuer DID anchoring, status,
trust, presentation-time holder reauthorization, and production custody remain
unavailable. Grant codes, access tokens, nonces, proofs, and original credential
bytes never enter headless or UI results. The deterministic issuer is in-process
and uses only loopback identifiers; normal production composition has no issuer
transport. Format-private credential material has a separate 256 KiB-bounded
opaque route through verified import and the same encrypted atomic record. It
remains absent from ordinary incoming DTOs. The Digital Passport adapter
interprets it only after recomputing all five official Midnight commitments and
the signed claim root. Headless exposes safe candidate/plan metadata but no
reveal operation.
The Dioxus card permits explicit device-local first/last reveal and age
threshold planning. A separate OpenID4VP 1.0 Final-shaped DCQL panel previews a
deterministic standalone verifier request, matching credential, and exact claim
intents before consent. Mobile composition performs exact credential/opening,
holder-control, holder-proof, and public-statement preflight but deliberately
stays fail-closed before the resource-intensive native prover. The explicit
headless artifact composition additionally performs checked Compact proving,
independent public verification, and internal `vp_token` validation. Live
verifier transport and mobile prover packaging remain unavailable.

Standalone composition also accepts exactly one request-by-reference SIOPv2
draft-13 login profile. It previews the verifier and purpose, requires exact
explicit consent, creates an EdDSA or ES256 self-issued ID Token through an
active managed DID authentication method, and has the in-process verifier
independently resolve and verify it once. The request object is deterministic
and unsigned because no network transport is involved. This is DID
authentication, not OpenID4VP credential presentation; `vp_token`, DCQL,
selective disclosure, native ingress, and live verifier transport remain
unavailable. Normal production composition has no SIOP adapter.

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

The local cache accepts only hash-pinned official DUST and Zswap artifacts, is
bounded to 64 entries and 256 MiB, applies smaller per-artifact bounds, and
never stores witnesses. To use the remote development alternative, unset the
cache variable and set the proof-server route instead:

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

An optional Passport Vault read can replace the unproven indexer state with
complete finalized-node replay. Supply the non-zero deployment height together
with the complete standalone routes, then call `vault.contract_state.read`
with the exact 32-byte contract address:

```bash
export OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT='<deployment-block-height>'
cargo run -p oxid-headless
```

The height is only a discovery hint: Oxid requires the target deployment event
there, scans every canonical block through one captured finalized head, and
replays the official public transcripts. The node must retain historical block
bodies, metadata, and `System.Events`; missing archive data fails unavailable.
Only this path reports `finalized_node_replay` with
`canonical_finalized_replay`. Without the variable, the standalone read keeps
the explicit `node_anchored_indexer` / `indexer_supplied_not_proven` boundary.

The headless `vault.contract_call.*` methods stage typed `create_lock`,
`deposit_to_lock`, `claim_from_lock`, and `withdraw_from_lock` operations as
retained drafts. Zero-configuration headless runs exercise the complete
prepare, authorize, submit, history, cancellation, and reconciliation protocol
against a deterministic fixture at
`9d57c7c697a747bac5b8c5828686728049d2e032cf98ff357607f086a3916fd0`.
Discover it through `system.capabilities`: the mode is always
`deterministic_simulation`, its state authentication is
`deterministic_simulation`, and `settlesOnMidnight` is `false`. Simulated
`included` responses are process-local harness outcomes, not Midnight blocks.

Explicit live composition requires canonical replay state. It remains
`native_pending` until the complete standalone routes, deployment height, and
packaged composer are configured. The Nix closure's generated client, exact ABI, four wallet
circuit keys/IR, and degree-10/11/17 parameters are authenticated at runtime.
The separate Nix composer executes typed `createLock`, `depositToLock`, and
`withdrawFromLock` calls plus the authorization-gated protected
`claimFromLock` schema; its output round-trips through the pinned Rust ledger
codec. The native retained adapter accepts only canonical-replay
state plus fresh bounded public Midnight context, requires real serialized
Zswap/ledger-parameter snapshots, and keeps the unproven transaction in a
zeroizing private buffer through prepare. Exact authorization then derives the
generated call's native NIGHT deficit, selects synchronized unshielded inputs,
returns change, and signs once per input inside protected Midnight custody.
Withdraw must require no NIGHT funding. The complete native capability reports
`native_settlement` for all four wallet operations. It reuses the standalone
Midnight DUST sync, exact fee balancing, proving, persist-before-broadcast
journal, node submission, pre-broadcast cancellation, and finalized
reconciliation path, so `settlesOnMidnight` is true. Configure
`OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH` for restart-safe public submission
metadata. Claim composition re-verifies the exact stored credential and uses
managed holder custody plus fresh randomness only after exact authorization;
the administrative circuit remains rejected.
Authorization and proving/submission use two separate exact intents in both
modes. Incoming JSON never accepts private credential material, witnesses,
signatures, proofs, or serialized transactions.

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

Use `just ios-dev` (or `OXID_UI_PROFILE=dev just ios-run`) to launch the
standalone developer capability profile. The default remains the user profile.

The repository iOS and Android launch scripts explicitly enable
`oxid-app/standalone-development`. Native builds select the same
environment-aware composition as the headless harness: with no live variables,
public profiles plus the standalone Passport Vault ledger persist, protected
roots and drafts are process-local, no chain service is contacted, and the UI
labels simulated results. A complete reviewed
standalone configuration selects authenticated native settlement; partial or
invalid configuration fails startup. A normal `cargo run -p oxid-app` does not
enable this feature and stays fail-closed.

To run the mobile UI against the real laptop-hosted standalone indexer, node,
and prover rather than deterministic simulation, select the separate localhost
profile at build time:

```bash
just standalone-up
just ios-standalone-local
# after stopping the iOS simulator:
just android-standalone-local
```

Both builds use the immutable `undeployed` loopback routes from ADR-0097. iOS
Simulator reaches host loopback directly. Android emulator receives only exact
`adb reverse` mappings for ports 8088, 9944, and 6300; the launcher rejects a
physical device. Do not substitute `10.0.2.2`, because the plaintext local
prover policy intentionally accepts only syntactic loopback. These profiles are
compile-time development composition, not a runtime production network picker.

The standalone mobile header includes **Scan QR**. A successful physical-device
scan strictly routes an OpenID credential offer or one of the registered
standalone SIOPv2/OpenID4VP requests into its existing preview and consent
page; it never accepts the request automatically. Apple simulators have no
camera and therefore show the expected unavailable message. The offer, login,
and presentation fixture buttons remain available for complete simulator flow
testing.

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
create and resolve standalone DIDs, preview and accept an OpenID4VCI offer,
complete a consented self-issued DID login,
read the truthfully labelled Passport Vault contract state, complete an exact
prepare/authorize/prove/submit call lifecycle,
restart the process, and assert public-profile, submission, DID-inventory, and
encrypted credential restoration plus standalone Passport Vault accounting and
claim-replay restoration:

```bash
just ios-smoke
just ios-standalone-local-smoke
just ios-backup-smoke
just android-smoke
just android-standalone-local-smoke
just android-backup-smoke
```

The two `standalone-local-smoke` commands start or reuse the repository-owned
stack, reset only Oxid data on the selected virtual device, activate a protected
profile account, and require `Live` plus `Synced · Live source` with both
derived address rails. They reject deterministic balances/labels. Run them
sequentially; do not keep the iOS Simulator and Android emulator active at the
same time when collecting evidence.

`just ios-backup-smoke` creates and later deletes a disposable iPhone
simulator. It exports a populated complete wallet through Files, uninstalls the
app, resets and reboots the simulator, reinstalls the standalone-development
build, imports the selected document through Files, and verifies the restored
profile, account association, DID, and credential. This is simulator evidence;
`just android-backup-smoke` performs the equivalent flow on an Android
emulator through DocumentsUI. It writes only to a uniquely named directory in
Downloads, removes and reboots the app, reinstalls the exact built APK, imports
the selected document, verifies the same restored state, and then removes only
that test directory. Both commands are simulator/emulator evidence;
physical-device measurements remain separate release gates.

## Repository layout

| Path | Responsibility |
| --- | --- |
| `apps/oxid` | Executable shell and Dioxus launch configuration. |
| `apps/oxid-headless` | Versioned NDJSON flow and integration-test harness. |
| `crates/foundation` | Small Oxid-owned primitives. |
| `crates/wallet/domain` | Wallet entities and invariants. |
| `crates/wallet/application` | Use cases and wallet-owned ports. |
| `crates/identity/domain` | DID document, public JWK, relationship, and resolution invariants. |
| `crates/identity/application` | Profile-scoped DID use cases, multi-curve lifecycle/signing, and identity-owned ports. |
| `crates/credential/domain` | Credential records, explicit formats, bounded detached proofs and opaque format-private material, schema-neutral disclosure candidates, and structured verification invariants. |
| `crates/credential/application` | Profile-scoped credential inventory, holder-bound issuer port, exact-bundle verified import/reverification, disclosure inventory/plan, and targeted local-reveal use cases. |
| `crates/protocol/domain` | Credential-offer and self-issued-authentication preview/lifecycle invariants. |
| `crates/protocol/application` | Protocol-neutral issuance, explicit holder-binding, and DID-authentication use cases and outgoing ports. |
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
The credential migration and exact Digital Passport safety boundary are
recorded in
[docs/migration/midnight-vc-provenance.md](docs/migration/midnight-vc-provenance.md)
and [ADR-0042](docs/adr/0042-bind-digital-passport-disclosure-to-signed-commitments.md).
The fail-closed presentation boundary is recorded in
[ADR-0043](docs/adr/0043-gate-openid4vp-on-reproducible-compact-proofs.md),
and exact Compact credential persistence/verification is recorded in
[ADR-0045](docs/adr/0045-preserve-and-verify-detached-midnight-compact-credentials.md),
while standalone selected-DID issuance binding is recorded in
[ADR-0047](docs/adr/0047-bind-standalone-compact-credentials-to-managed-jubjub-did-methods.md).

## Security

The JSON repository is durable only for public profile metadata; it is not a
secret store. The software signing and HD-derivation adapter is process-local development/test
infrastructure and production composition does not select it. The encrypted
credential repository, standalone issuer, and deterministic verifier request
are development conformance boundaries, not production custody or trust. Local
disclosure or OpenID4VP request preview does not create a verifier presentation
or prove a predicate. Never use this milestone to
custody real assets or externally issued credentials. See
[SECURITY.md](SECURITY.md) for reporting and the current threat boundaries.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) and [AGENT.md](AGENT.md) before making
changes. Contributions require DCO sign-off, and repository-facing commits must
be GPG signed.

Oxid is licensed under the [Apache License 2.0](LICENSE). Retained icon notices
are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
