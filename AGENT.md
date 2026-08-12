# AGENT

Engineering guide for agents and contributors working in `oxid`.

This repository is the public, standalone home of the Oxid identity wallet. The
root `OXID_IDENTITY_WALLET_BLUEPRINT.md` is the product and architecture north
star. When this guide and the blueprint differ, preserve the blueprint's
dependency and security rules and update this file in the same change.

## Purpose and current phase

Oxid is a Rust-first, mobile-first wallet in which crypto and self-sovereign
identity are peer capabilities. Dioxus is an incoming UI adapter; it is not the
application architecture.

The M0 foundation is complete. It proved the smallest vertical architecture
before adding Cardano, Midnight, DID, VC, OIDC, or DIDComm SDKs:

1. foundation primitives;
2. wallet domain;
3. wallet application/use cases and outgoing ports;
4. platform ports;
5. in-memory and system adapters;
6. Dioxus UI adapter;
7. composition root.

The complete profile lifecycle now includes **create, list, select, and restore
active wallet profile**. It is available through both the Dioxus shell and the
standalone `oxid-headless` incoming adapter, with public metadata persisted by a
replaceable JSON adapter. ADR-0023 now
prioritizes staged functional parity with the reviewed Midnight mobile wallet.
The ordered public backlog is
[issue #2](https://github.com/MediaNoxLabs/oxid/issues/2); implement it in
bounded slices and never turn parity work into a bulk source copy. The wallet
presentation shell is the first post-M0 slice. Its deferred destinations are
status surfaces, not claims of working custody or identity capabilities. The
DIDs, protected credential inventory, and embedded pre-authorized OpenID4VCI
issuance are now functional in standalone development; vault, live issuer
transport, presentation, and disclosure behavior remain deferred.

ADR-0017 is accepted. The first M1 security slice separates protection/session
state from key operations, secret blobs, and native user authorization. The
standalone harness has a process-local Ed25519/P-256 plus
BIP32/secp256k1-Schnorr adapter for conformance;
normal mobile composition uses a fail-closed unavailable adapter until native
Apple Keychain/Secure Enclave and Android Keystore/BiometricPrompt adapters are
implemented. Track that slice and its follow-ups in
[issue #5](https://github.com/MediaNoxLabs/oxid/issues/5).

ADR-0015 is accepted with the immutable upstream baseline recorded in the ADR.
The first M2 account slice owns chain-neutral networks, addresses, exact
NIGHT/DUST balances, sync state, and transaction history. Production
composition is unavailable until custody and a live source exist; headless and
in-memory composition use a development-only simulated Midnight source made
from public address payloads. Track the slice and its live follow-ups in
[issue #6](https://github.com/MediaNoxLabs/oxid/issues/6).

The next M2 slice, [issue #7](https://github.com/MediaNoxLabs/oxid/issues/7),
implements native live unshielded sync against the pinned indexer GraphQL v4
`unshieldedTransactions` subscription. The normal mobile `compose()` remains
unavailable. `oxid-headless` selects the live source only when
`OXID_MIDNIGHT_NETWORK_ID`, `OXID_MIDNIGHT_INDEXER_WS_URL`, and
`OXID_MIDNIGHT_UNSHIELDED_ADDRESS` are all present and valid; zero variables
retains simulation and partial configuration fails startup. These are public,
process-local adapter values and are never persisted with profile metadata.

[Issue #8](https://github.com/MediaNoxLabs/oxid/issues/8) connects those
boundaries for external NIGHT accounts. The development adapter generates its
root during profile security initialization and implements typed protected
derivation for `m/44'/2400'/account'/0/index`. Simulation and live sources bind
the public derived address and opaque transaction-key reference; the live
source clears its previous watch-only cache before the next subscription. The
headless protocol accepts only bounded account/address indices and never a
seed, mnemonic, private key, or path string. Normal mobile composition remains
fail-closed pending native custody.

[Issue #9](https://github.com/MediaNoxLabs/oxid/issues/9) adds canonical
unshielded NIGHT preparation and authorization under ADR-0026. Native
`adapters/midnight` consumes the accepted ledger Git revision, retains
profile-scoped one-hour drafts, follows the prototype's descending greedy UTXO
selection and sorted `0xCAFE` intent construction, and signs through the opaque
custody reference. Headless prepare/authorize/draft methods expose public
previews only. This is the first stage of the transaction flow.

[Issue #11](https://github.com/MediaNoxLabs/oxid/issues/11) and ADR-0027 complete
the native development/headless flow. Submission derives the canonical DUST
child at `m/44'/2400'/account'/2/0`, replays bounded DUST ledger events against
the indexer's current ledger parameters, proves locally or through the explicitly
configured development proof service, and submits the unsigned extrinsic to a standalone
node. `wallet.transaction.submit_unshielded` and its prototype-named staged
`wallet.transaction.send_unshielded` alias expose public outcomes only;
zero-configuration headless runs use a deterministic simulated completion
adapter. A retryable failure restores an authorized draft; cancelling the
caller signals cooperative cancellation while leaving the worker responsible
for the eventual transition so a second send cannot race it. Live completion
checks cancellation at safe pre-broadcast boundaries and restores `Authorized`;
it never makes a possibly broadcast transaction retryable. An ambiguous post-submit node outcome or unexpected worker
termination remains `Submitting` and forbids blind retry until durable
reconciliation resolves the public attempt. A submitted draft is idempotent.
[Issue #12](https://github.com/MediaNoxLabs/oxid/issues/12) and ADR-0028 add
private local DUST proving through the official pinned ZKIR provider. The cache
is explicit, app-private, hash-authenticated, symlink-rejecting, and bounded to
8 MiB; the DUST circuit is k=13 with 5,646 modeled rows. A real proof/seal/codec
harness runs on macOS, iOS simulator, and Android emulator. Remote proving is
still an explicit development mode. Production/mobile composition remains
fail-closed until native custody and production chain access are reviewed.

[Issue #14](https://github.com/MediaNoxLabs/oxid/issues/14) and ADR-0029 expose
the protected external account, receive QR, and staged unshielded transfer
journey through Dioxus. Repository iOS/Android launch scripts select the
explicit `oxid-app/standalone-development` feature, which reuses persistent
public profiles plus process-local development custody and deterministic
simulated completion. A default app build still calls `compose()` and remains
fail-closed. Restarting the development app intentionally loses its protected
root and retained drafts; reactivate the account before another flow. Public
submission outcomes remain visible through the separate durable journal.

[Issue #15](https://github.com/MediaNoxLabs/oxid/issues/15) and ADR-0030 add an
explicit native public-account checkpoint store. Headless live modes enable it
only with `OXID_MIDNIGHT_ACCOUNT_CHECKPOINT_PATH`, which must be an absolute
app-private file. The v1 JSON is keyed by network/address, bounded to 16 MiB and
128 records, uses decimal-string `u128` values and owner-only atomic writes,
and contains no endpoint, profile metadata, key reference, secret, draft,
signature, proof, or witness. A valid restart read is `cached`; subscribe from
`current_cursor + 1`, retry protocol/data incompatibility once from zero, and
preserve cached values as `stalled` on transport failure. Never expose cached
UTXOs to transaction preparation until a live catch-up succeeds.

[Issue #16](https://github.com/MediaNoxLabs/oxid/issues/16) and ADR-0031 add a
separate adapter-private DUST checkpoint enabled only by the complete
standalone/headless stack through `OXID_MIDNIGHT_DUST_CHECKPOINT_PATH`. Its v1
binary envelope is bounded to four records, 16 MiB per tagged
`DustLocalState`, and 64 MiB total; records are scoped by network, SHA-256 of
the tagged public DUST key, and SHA-256 of live tagged DUST parameters. It
stores the last folded current cursor, advertised target, and update time but
never the seed or secret scalar. Resume subscribes from `cursor + 1`, folds at most 256 events or
4 MiB per batch, and bounds a run to one million events, 512 MiB, and 30
minutes. Every spend still fetches live parameters and catches up to the live
target. Incompatible cached deltas retry once from zero; transport/timeouts
fail closed. Development roots remain ephemeral, so cross-process reuse awaits
native durable custody.

[Issue #17](https://github.com/MediaNoxLabs/oxid/issues/17) and ADR-0032 expose
DUST synchronization through an Oxid-owned status/start/cancel boundary. The
native Midnight adapter owns a profile/network worker and saves every folded
bounded batch, so partial checkpoints satisfy `current_cursor <= target_cursor`
and cancellation resumes from the last consistent offset. Core and incoming
types contain only lifecycle, cursors, per-run event count, exact atomic DUST,
freshness, and sanitized failure category. Headless v1 methods are
`wallet.dust.sync.status`, `wallet.dust.sync.start`, and
`wallet.dust.sync.cancel`; Dioxus polls the same use cases. Cached, cancelled,
or stalled DUST is display/resume state only and never live spend authority.
The iOS standalone smoke flow exercises `Sync DUST`, the exact `12 DUST`
fixture result, and the resulting `Resync DUST` action before transfer checks.
The Android CDP smoke flow asserts the same DUST result and resync transition.

[Issue #18](https://github.com/MediaNoxLabs/oxid/issues/18) and ADR-0033 keep
shielded Zswap custody, replay, and checkpoints inside the native Midnight
adapter. Protected role `3/0` derivation exposes only the canonical public
shielded receive address. The first explicit lifecycle adds
`wallet.shielded.sync.status`, `wallet.shielded.sync.start`, and
`wallet.shielded.sync.cancel`, exact decimal-string token balances, and bounded
owned-note/commitment counts. The deterministic standalone session advances on
polls for headless and mobile cancellation/resume coverage. Explicit live
headless configurations run a bounded native `zswapLedgerEvents` worker,
checkpoint every consistent official-state batch, resume at `cursor + 1`, and
retry an incompatible cached delta once from zero. Production composition
remains fail-closed pending durable native custody and endpoint discovery.

[Issue #19](https://github.com/MediaNoxLabs/oxid/issues/19) and ADR-0034 expose
transaction submission status and deliberate pre-broadcast cancellation. The
Midnight adapter retains a profile/draft-scoped control object and atomically
marks the broadcast boundary immediately before node submission. An
acknowledged cancellation restores `Authorized` and records the attempt as
`Cancelled`; broadcasting, included, and unknown attempts cannot be cancelled
or made retryable. Headless adds asynchronous start/status/cancel methods, and
Dioxus uses the same application boundary for its Cancel and safe-retry flow.

[Issue #20](https://github.com/MediaNoxLabs/oxid/issues/20) and ADR-0035 make
the post-broadcast attempt durable without persisting the signed transaction.
The Midnight adapter must save the public fee, extrinsic hash, finalized
pre-broadcast anchor, expiry/update time, profile/network/draft scope, one-way
planning fingerprint, mode, and state before calling the node. Store no signed
or sealed transaction, proof, witness, secret, key, route, or authorization
payload. The v1 JSON journal is capped at 128 records/256 KiB, rejects symlinks
and permissive files, and uses owner-only atomic replacement. Development
mobile composition derives its private journal path beside the resolved profile
store; headless can override it with the normalized absolute
`OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH`. Restored `Broadcasting` and
`OutcomeUnknown` attempts block duplicate planning. Live reconciliation scans
at most 2,048 finalized ancestors to the saved anchor and permits a fresh
replacement only after finalized rejection or chain-time expiry. Headless
methods are `wallet.transaction.submission_history` and
`wallet.transaction.reconcile_submission`; Dioxus shows the latest restored
public attempt even when process-local custody is unavailable.

[Issue #21](https://github.com/MediaNoxLabs/oxid/issues/21) and ADR-0036 begin
the peer identity capability with profile-scoped DID inventory and resolution.
`identity/domain` and `identity/application` have no external dependencies and
own current Midnight DID 0.5.0 syntax/document invariants plus resolve/list/get/
forget ports. `adapters/did-midnight` provides exactly one successful
standalone fixture and a native official `POST /resolve` adapter selected only
by `OXID_MIDNIGHT_DID_RESOLVER_URL`; unknown standalone DIDs are not found.
The HTTP adapter uses the exact-pinned `webpki-root-certs 1.0.9` public root
bundle, not ambient platform CA state; local or enterprise roots are therefore
not trusted implicitly.
`adapters/storage-identity-json` stores validated public documents separately
under `OXID_DID_STORE_PATH` or `private/did-records.json` beside the profile
store. Headless DID params never accept a profile, route, key, or credential.
Normal `compose()` remains unavailable; DID create/update/deactivate,
credentials, lifecycle authorization, and production-native storage remain
follow-ups. Preserve the immutable conformance sources in
`docs/migration/midnight-did-provenance.md` when upgrading the contract.

[Issue #22](https://github.com/MediaNoxLabs/oxid/issues/22) and ADR-0037 add the
development-only standalone `did:midnight` lifecycle without copying the
prototype's `controllerSkHex` exposure. `identity/application` owns create,
update, sign, and deactivate ports/use cases; `adapters/did-midnight` delegates
real Ed25519/P-256 key generation and signing to opaque wallet custody handles.
It supports aliases, verification-method add/rotate/remove, relationship
add/remove, service add/update/remove, signing with explicit confirmation, and
deactivation for `undeployed` DIDs. Every mutation, signing operation, and
deactivation requires bounded human-readable confirmation. Public documents persist, but custody
associations are process-local: after restart, records remain inspectable and
mutation/signing must return `NotManaged`. Normal production composition and
all non-undeployed/live Compact writes remain fail-closed. The remaining
Jubjub/Compact proving/submission/finality gap is a later adapter slice, not a
reason to expose private key material.

[Issue #23](https://github.com/MediaNoxLabs/oxid/issues/23) and ADR-0038 add the
peer credential foundation. `credential/domain` and
`credential/application` have no external dependencies and keep original
signed bytes separate from normalized display/search metadata. Verification is
always a seven-stage structural/issuer/proof/temporal/status/schema/trust
report, never a boolean. `adapters/vc-midnight` implements strict phase-1 CBOR
proof stripping and Ed25519/P-256 issuer assertion verification;
`adapters/storage-credential-json` encrypts the complete bounded document with
XChaCha20-Poly1305. Its separate owner-private key file is development-only,
not native custody. Standalone headless and mobile flows receive, list,
reverify, confirmation-delete, and restore the public fixture without exposing
the signed body. Normal `compose()` remains unavailable pending native
Keychain/Keystore wrapping. Live OID4VCI transport, OID4VP/SIOP, disclosure
openings, status/schema/trust policy, Compact passport proofs, and Jubjub remain
later slices.

[Issue #24](https://github.com/MediaNoxLabs/oxid/issues/24) and ADR-0039 add a
dependency-free protocol domain/application hexagon plus an exact OpenID4VCI
1.0 Final standalone subset. The only implemented journey is an embedded offer
using the pre-authorized-code grant without Transaction Code. The in-process
adapter strictly separates Credential Issuer and OAuth metadata, uses the Nonce
Endpoint model, builds `proofs.jwt`, parses the final `credentials` array, and
imports through the valid-only ADR-0038 sink. Offer preview and exact
`ACCEPT_CREDENTIAL_ISSUANCE` consent happen before DID key use. Codes, access
tokens, nonces, proofs, signing input, and credential bytes never enter incoming
DTOs. Plain HTTP is loopback-only in this standalone adapter; production
endpoint policy is HTTPS-only and normal `compose()` wires unavailable protocol
ports. Live HTTP/discovery, Authorization Code, by-reference offers,
Transaction Code, batch/deferred issuance, OID4VP/SIOP, deep links, and scanning
remain separate slices.
The standalone issuer must independently resolve the selected public DID
method and verify the Ed25519/P-256 proof JWS, nonce, anonymous-flow `iss`
omission, audience, algorithm, and bounded `iat`; structural JWT validation is
not sufficient.
`DidRecordView.managed_method_ids` is current-process capability metadata, not
persisted ownership. Credential issuance must select an active authentication
method from this set; never infer control merely because a resolved or restored
public DID document contains an authentication relationship.

[Issue #13](https://github.com/MediaNoxLabs/oxid/issues/13) tracks the separate
Tier-2 browser build: `cargo check -p oxid-app --no-default-features --features
web --target wasm32-unknown-unknown` currently stops in the pre-existing
`getrandom 0.2` graph because its JavaScript backend feature is not enabled.
Keep that repair target-scoped; it must not add browser-only dependencies to
the green Tier-1 Android and iOS graphs.

## Prototype provenance

The prototype remains useful migration input, not an architecture template.
The reviewed baseline is:

- repository: `midnight-ledger`;
- branch: `feat/mobile-prototype`;
- commit: `074b1a4bccbfee1740ee188374b606a022ecef42` (2026-07-02);
- source area: `mobile-bench/`, especially `wallet-core/`,
  `dioxus-wallet/`, and `headless-wallet/`.

That commit declares itself the successor to the earlier Dioxus/VC prototype
branches. Record a new immutable commit here before taking later prototype
changes. Do not copy ledger-relative path dependencies, demo secrets, generated
proof artifacts, pre-production keys, vendored JS, or environment-specific
mobile projects into Oxid without an explicit migration decision.

The staged component inventory and destination map live in
`docs/migration/midnight-ledger-prototype.md`. Presentation-specific provenance
and exclusions live in `docs/migration/ui-shell-provenance.md`. Account-specific
upstream evidence, vectors, and exclusions live in
`docs/migration/midnight-account-provenance.md`. Credential-store and verifier
evidence, deliberate hardening, and exclusions live in
`docs/migration/midnight-vc-provenance.md`.

## Architecture boundaries

Dependencies point inward:

```text
apps -> incoming adapters -> application -> domain
   +-> composition -> outgoing adapters -> platform ports -> foundation
```

Rules:

- Domain and application crates must not depend on Dioxus, chain/SSI SDKs,
  persistence engines, HTTP clients, OS APIs, or JavaScript/WASM libraries.
- Oxid owns all public core types. Convert external types at adapter boundaries.
- Put incoming use-case traits and outgoing capability ports in the application
  boundary that owns them; prefer small traits over aggregate service objects.
- Dioxus renders state and emits application commands. It never calls storage,
  chain, SSI, or platform SDKs directly.
- Private key material, seeds, credential claims, and recovery data must not
  appear in ordinary UI/application DTOs, logs, fixtures, or committed config.
- Key use is expressed through opaque references and key-operation ports.
- Use static Cargo composition for the MVP. Runtime native plugin loading is out
  of scope.
- Add an ADR for architectural changes. Do not silently reverse an accepted ADR.
- Keep the core independently testable without UI, network, or OS services.

The blueprint's ADR summaries are materialized as ADR-0001 through ADR-0020 in
`docs/adr/README.md`. ADR-0021 records the staged prototype migration and
ADR-0022 records Nix as the reproducible environment. ADR-0023 records the
post-M0 prototype-parity priority. ADR-0024 records the versioned NDJSON
headless adapter and forbids secret-bearing results. ADR-0025 separates durable
public profile metadata from protected secret storage. ADR-0026 stages Midnight
transaction authorization before proving/submission. ADR-0027 defines and
implements standalone DUST synchronization, proving, and node submission for
development/headless use. ADR-0028 makes private local proving the production
direction and records its cache, cancellation, interoperability, and mobile
resource bounds. ADR-0029 separates simulator/emulator standalone wallet flows
from production wiring and records receive-QR plus transaction-UI boundaries.
ADR-0030, ADR-0031, and ADR-0033 keep public unshielded, private DUST, and
private shielded checkpoints in separate native adapter stores. ADR-0032 adds
the adapter-owned DUST session and partial-checkpoint cancellation/resume rule
without weakening live-before-spend. ADR-0033 keeps Zswap keys/state
adapter-private and owns the explicit shielded sync lifecycle and worker
without exposing ledger or secret types.
ADR-0034 keeps transaction cancellation adapter-owned, requires an atomic
pre-broadcast boundary, and separates attempt status from retained draft state.
ADR-0035 adds the bounded public submission journal, persist-before-broadcast
rule, restart duplicate prevention, and finalized-chain reconciliation.
ADR-0036 adds the identity hexagon, current Midnight DID public-document
validation, bounded resolver adapters, and separate public DID inventory.
ADR-0037 adds standalone DID lifecycle operations through opaque development
custody. ADR-0038 adds the credential hexagon, protected original-byte store,
structured verification pipeline, and strict Midnight phase-1 CBOR verifier.
ADR-0039 adds the protocol hexagon, final-shape pre-authorized issuance,
adapter-private protocol secrets, explicit offer consent, and verified import.
ADR-0017 records the accepted platform-custody split.
ADR status
and delivery state are deliberately separate: an accepted future boundary is
binding but does not mean the capability is implemented. Proposed ADRs are
gates, not dependency authorization.

Current package ownership:

| Path | Responsibility |
| --- | --- |
| `crates/foundation` | Small dependency-free primitives shared across core boundaries. |
| `crates/wallet/domain` | Wallet profile invariants and entities. |
| `crates/wallet/application` | Incoming use cases and owned outgoing repository ports. |
| `crates/identity/domain` | Dependency-free Midnight DID, public JWK, document, and resolution invariants. |
| `crates/identity/application` | Profile-scoped DID resolution, inventory, lifecycle/signing use cases, and owned outgoing ports. |
| `crates/credential/domain` | Dependency-free credential records, metadata separation, and structured verification invariants. |
| `crates/credential/application` | Profile-scoped receive/list/get/reverify/delete use cases and repository/inbox/verifier ports. |
| `crates/protocol/domain` | Dependency-free credential-offer preview and issuance lifecycle invariants. |
| `crates/protocol/application` | Profile-scoped prepare/accept/refuse/get/list use cases plus protocol/proof/verified-sink ports. |
| `crates/platform/ports` | Clock and randomness capabilities used by applications. |
| `crates/adapters/storage-memory` | Development/test implementations of wallet, DID, and credential persistence ports. |
| `crates/adapters/storage-json` | Versioned persistence for public profile metadata and active selection only. |
| `crates/adapters/storage-identity-json` | Strict versioned persistence for validated profile-scoped public DID documents only. |
| `crates/adapters/storage-credential-json` | Development-only authenticated encryption for bounded profile-scoped credential records and original signed bytes. |
| `crates/adapters/storage-dev` | Process-local, development-only Ed25519/P-256 generation plus protected BIP32/secp256k1-Schnorr derivation and signing. |
| `crates/adapters/midnight` | Midnight network/account and native canonical-transaction adapter with fail-closed production, simulation/live sources, protected public-account binding, retained development drafts, standalone DUST/proving/submission completion, and bounded public submission recovery. |
| `crates/adapters/did-midnight` | Single-fixture standalone and explicit bounded native Midnight DID resolution plus development lifecycle adapters. |
| `crates/adapters/vc-midnight` | Strict Midnight phase-1 CBOR credential verification and public standalone credential ingress. |
| `crates/adapters/openid4vci` | Strict OpenID4VCI 1.0 Final embedded pre-authorized flow, in-process standalone issuer, DID proof bridge, and verified credential sink. |
| `crates/adapters/platform-system` | System clock and OS randomness implementations. |
| `crates/ui-dioxus` | Dioxus incoming adapter, exact amount/consent presentation state, and public receive-QR rendering. |
| `crates/composition` | Concrete dependency wiring with no product rules. |
| `apps/oxid` | Executable shell and platform launch point. |
| `apps/oxid-headless` | Standalone NDJSON incoming adapter and flow harness. |

`oxid-composition` exposes UI-neutral `ApplicationServices`. Incoming adapters
adapt that object at their own boundary; composition must not depend on Dioxus,
the headless protocol, or another incoming adapter. The headless protocol is
`oxid.headless.v1`. Its stdout is protocol-only, invalid input must not poison
the stream, and capability discovery must label unimplemented methods as
`queued`. Never reproduce the prototype's `controllerSkHex` bootstrap result or
require a seed before an implemented chain use case needs an opaque key
reference.

`compose()` is the production-facing composition and deliberately reports
wallet protection and Midnight account state unavailable. `compose_headless()`
combines persistent public profiles with the ephemeral development key adapter
and public simulated Midnight source; `compose_headless_from_environment()`
selects the zero-configuration simulated path, a read-only live source when the
three original Midnight variables are present, or full standalone submission
when the five common route/address variables and exactly one proving mode are
present. The submission route variables are
`OXID_MIDNIGHT_INDEXER_HTTP_URL` and `OXID_MIDNIGHT_NODE_WS_URL`. Private local
proving uses `OXID_MIDNIGHT_PROVING_CACHE_DIR`; the explicit development
alternative uses `OXID_MIDNIGHT_PROOF_SERVER_URL`. Supplying neither or both
fails startup. The original
three are `OXID_MIDNIGHT_NETWORK_ID`, `OXID_MIDNIGHT_INDEXER_WS_URL`, and
`OXID_MIDNIGHT_UNSHIELDED_ADDRESS`. `compose_in_memory()` uses the development
adapters for tests. Never change `compose()` to select `storage-dev`, simulation,
or environment-derived indexer, node, or proof configuration. Headless
protected-key methods accept only public labels,
algorithms, purposes, bounded payloads, opaque references, and explicit
human-readable confirmations. Passphrases, seeds, recovery phrases, and raw
private keys are rejected by strict parameter decoding.

Identity composition is independent of chain mode. Native headless startup
uses deterministic `StandaloneDidResolver` unless the complete explicit
`OXID_MIDNIGHT_DID_RESOLVER_URL` is valid; non-loopback HTTP, credentials,
query, fragment, redirects, and ambient proxies are forbidden. The HTTP result
is capped at 512 KiB/depth 16 and every document collection is bounded. The
public store uses `OXID_DID_STORE_PATH` when set, otherwise a private sibling of
the public profile file. The headless surface is `did.resolve`, `did.list`,
`did.get`, and `did.forget`; profile scope always comes from the active profile.
The standalone fixture is
`did:midnight:undeployed:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`.
Its `standalone-fixture-v2` Ed25519 method is both authentication- and
assertion-authorized so the public credential fixture can exercise issuer proof
verification.

Credential composition is independent of chain mode. Standalone development
uses `OXID_CREDENTIAL_STORE_PATH` and `OXID_CREDENTIAL_KEY_PATH` only when both
are set; otherwise it derives `private/credentials.enc` and
`private/credentials.key` beside the configured profile store. Partial explicit
configuration fails startup. The key file is a temporary development wrapping
boundary and must never be described as platform-backed or recoverable. Normal
`compose()` wires unavailable credential ports. The headless surface is
`credential.receive`, `credential.list`, `credential.get`,
`credential.reverify`, and confirmation-gated `credential.delete`;
`credential.request` and `credential.verify` remain prototype aliases. These
methods derive profile scope from the active profile and never return signed
bytes, proofs, openings, or claim values.

The headless DUST surface is `wallet.dust.sync.status`,
`wallet.dust.sync.start`, and `wallet.dust.sync.cancel`. These commands never
accept a key, path, seed, endpoint, or checkpoint. The deterministic simulator
advances on status polls so tests can cover fresh, cancelled, resumed, and
already-current flows without timing races. Native start returns before network
or ledger work begins; incoming adapters must poll status and may cancel.

The headless shielded surface mirrors that lifecycle at
`wallet.shielded.sync.status`, `wallet.shielded.sync.start`, and
`wallet.shielded.sync.cancel`. It never accepts keys, paths, endpoints, seeds,
or checkpoint data. Exact `u128` balances cross the application/headless
boundary only as decimal strings, and token types as lowercase 32-byte hex.
Cached/cancelled/stalled state is display/resume state, never spend authority.
Native start returns before the bounded worker borrows the role-3 child and
connects. `OXID_MIDNIGHT_SHIELDED_CHECKPOINT_PATH` optionally enables the
owner-private store only when the rest of a read-only or complete live
configuration is present; it is invalid with simulation or by itself.

The controllable headless submission surface is
`wallet.transaction.start_submission`,
`wallet.transaction.submission_status`, and
`wallet.transaction.cancel_submission`. Start validates the same explicit
human-readable confirmation as synchronous submit, returns once adapter-owned
work is running, and never exposes worker handles or chain material.
Cancellation is allowed only in `running`; `cancellation_requested` becomes
`cancelled` after worker acknowledgement. `broadcasting`, `included`, and
`outcome_unknown` are non-retryable and cancellation must fail closed.

`oxid-app/standalone-development` is the only mobile-development exception: it
selects the same zero-configuration `compose_headless()` stack explicitly at
compile time. Repository simulator/emulator scripts enable it; default
desktop/mobile/web builds do not. It is for flow testing only, never real funds.

The development root and every derived child remain inside `storage-dev`.
`wallet.account.derive` exposes only bounded public indices, the selected
network, account/address metadata, and an opaque transaction-key reference.
Preserve idempotence for identical path metadata, fail closed on conflict or
lock state, and reset account sync state whenever a newly bound public address
replaces a fixture or watch-only address.

The headless transaction surface is
`wallet.transaction.prepare_unshielded`,
`wallet.transaction.authorize_unshielded`, `wallet.transaction.draft`,
`wallet.transaction.submit_unshielded`, and the
`wallet.transaction.send_unshielded` alias. Controllable attempts add
`wallet.transaction.start_submission`,
`wallet.transaction.submission_status`, and
`wallet.transaction.cancel_submission`. Pre-submission previews use
decimal-string atomic units and report DUST balancing/proving/submission as
pending; the submitted outcome exposes the final DUST fee plus public
transaction and block identifiers. Never add signing payload, signature,
transaction bytes, proof witness, seed, or private-key fields to these DTOs.
Drafts are process-local and profile-scoped; authorization must bind the exact
public challenge and explicit human-readable confirmation. Retryable worker
failure restores `Authorized`; cancelling the caller signals the worker and
leaves the draft `Submitting` until that worker publishes its result. A live
worker cancelled before broadcast restores `Authorized`. A completed submission is
replayed idempotently without using custody again. A post-submit timeout or
transport loss remains `Submitting` because its external outcome is unknown;
never make that state retryable without chain reconciliation.

The accepted ledger compatibility revision is
`d9414884db9da9e9b1f6f3a7f742d79a5732f817`. The native Midnight transaction
adapter consumes its ledger/base-crypto/coin/serialize/storage/transient
packages from the official HTTPS Git URL at that full `rev`, with ledger default
features disabled and its `proving` feature enabled. It also consumes the
official runtime and `midnight-zkir 2.1.0` at the same Git revision, with ZKIR
default features disabled. Proving resolves published
`midnight-proofs 0.7.3`, `midnight-circuits 6.3.0`, and
`midnight-zk-stdlib 1.3.0` transitively; Oxid has no direct `midnight-zk` Git
dependency. The upstream unconditional graph is substantial, so keep it
target-gated out of `wasm32`, out of read-model/core APIs, and validated on iOS
and Android.

The development HD adapter pins `bip32` 0.5.3, `k256` 0.13.4, and `sha2`
0.10.9. The stable BIP32 release already selects the same `k256` generation;
do not upgrade the direct Schnorr dependency independently and duplicate the
secp256k1 stack. The path and cross-language fixture are recorded in
`docs/dependencies/rustcrypto-midnight-hd-derivation.md`. The official address
JSON treats its seed as an already-derived scalar, so it is a codec fixture,
not an HD root-to-child fixture.

Receive QR rendering pins pure-Rust `qrcode` 0.14.1 in `ui-dioxus`, with
default features disabled and only SVG enabled. It receives already-validated
public address strings and has no core, camera, clipboard, file, network, or
JavaScript role. The dependency review is
`docs/dependencies/qrcode-0.14.md`.

Standalone credential persistence pins RustCrypto `chacha20poly1305` 0.11.0
with XChaCha20-Poly1305 and zeroization support. Cipher, key, nonce, and
ciphertext types stay private to `storage-credential-json`; the review is
`docs/dependencies/chacha20poly1305-0.11.md`.

The live indexer route is implemented with native-only Tokio 1.53.1 and
tokio-tungstenite 0.30.0 using Rustls 0.23.43 with the explicit Ring provider
and WebPKI roots. It runs on a short-lived worker
runtime so incoming adapters do not block their executor. The embedded query is
pinned to indexer revision `82759bf186184684f13a9ffa97b58b7b7684f47c`.
Preserve the bounds on endpoint length, credentials/query/fragment rejection,
connect/ack/idle/total snapshot timeouts, required subprotocol negotiation,
message/frame sizes, event and UTXO-record counts, cursor monotonicity, exact
decimal `u128` decoding, address ownership, and checked aggregation. The
`wasm32` graph intentionally excludes this native transport; browser WebSockets
require a separate reviewed adapter.

Public unshielded, private DUST, and private shielded checkpoint persistence
belong inside that native Midnight adapter, not `wallet-domain`,
`wallet-application`, or the public profile repository. Keep their formats
separate. Preserve
schema/count/size/scope/cursor/parameter validation, direct-target symlink
rejection, owner-only permissions, same-directory atomic replacement, and
safe disk semantics. Transaction catch-up treats checkpoint writes as
best-effort; explicit DUST and shielded sync surface storage failures and retain
the last consistent checkpoint. Zswap uses its official local state machine
and may retry an incompatible cached delta from zero only before publishing
new progress.

Standalone completion has separate bounded HTTP indexer replay, local or remote
proof, and node-WebSocket paths. It rejects stale or malformed chain-tip parameters,
replays canonical DUST events with checked decay and ordering, permits plain
HTTP proof service access only on loopback, and otherwise requires HTTPS. The
local mode authenticates only the pinned k=13 DUST artifacts in an 8 MiB
app-private cache and serializes proving on the submission worker. The proof
witness must never be logged or returned. Node submission waits for a
successful finalized block event and exposes only public hashes. Keep every
external error and response body behind sanitized adapter errors.

The reviewed WebPKI root store brings Mozilla CA certificate data under
`CDLA-Permissive-2.0`; `deny.toml` narrowly permits that permissive data license.

## Development environment

Nix is the supported environment and the flake lock is authoritative:

```bash
nix develop
```

The Linux Dioxus desktop graph links `libxdo`; keep `pkgs.xdotool` in both the
Linux development-shell libraries and package build inputs. macOS validation
cannot detect this linker requirement, so the hosted Linux gate is the
cross-platform evidence for it.

Direnv users can run `direnv allow`. The shell provides Rust, Cargo tooling,
`dx`, `just`, Node.js, and the pinned project-local Pi packages from
`.pi/settings.json`.

The Pi review integration is pinned as
`@input-output-hk/agent-review-pi@0.5.0`. That package declares both its review
extension and bundled review skill in Pi metadata; the shell installs them
together into the ignored project-local `.pi/npm` tree. Installation requires
an existing GitHub token with package-read access. Never write that token into
repository configuration or diagnostics.

Fast validation:

```bash
./run.sh --light --strict
```

Full local validation:

```bash
./run.sh --strict
```

Useful focused commands:

```bash
cargo test -p oxid-wallet-domain
cargo test -p oxid-wallet-application
cargo test -p oxid-credential-domain
cargo test -p oxid-credential-application
cargo test -p oxid-adapter-storage-memory
cargo test -p oxid-adapter-storage-credential-json
cargo test -p oxid-adapter-storage-json
cargo test -p oxid-adapter-storage-dev
cargo test -p oxid-adapter-midnight
cargo test -p oxid-adapter-vc-midnight
cargo test -p oxid-headless
cargo check -p oxid-app
./run.sh coverage --strict
./scripts/check-architecture.sh
./scripts/check-midnight-sources.sh
```

On macOS with Xcode and Rustup installed, `just ios-run` uses the Dioxus CLI
from the locked flake and the host Apple/Rust toolchain to build, install, and
launch the mobile feature. The Nix shell's non-Apple `xcrun` compatibility tool
must not be used for simulator discovery. The launcher also replaces Nix's
`DEVELOPER_DIR` and macOS `SDKROOT` with the selected Xcode installation and
its `iphonesimulator` SDK for the Dioxus build; preserve those overrides so
`nix develop --command just ios-smoke` remains valid. The XCUITest invocation
also uses a minimal host environment so Nix compiler/linker variables cannot
leak into Apple's build system.
`OXID_IOS_DEVICE=<UDID>` selects a specific simulator. The first verified smoke
test used an arm64 iPhone simulator. The prototype-derived shell and
first-launch profile gateway were subsequently built, launched, and visually
verified through the same command.
`just android-run` performs the equivalent Dioxus build, install, and launch
using an Android SDK/NDK plus a connected device or local AVD. Generated
Gradle/Xcode output remains under ignored `target/` paths.

`just ios-smoke` generates an ignored XCUITest project from
`tests/mobile/ios/project.yml`, resets only the installed Oxid simulator data,
and verifies profile creation, development account activation, receive QR,
staged simulated transfer, OpenID4VCI offer preview/consent/issuance, protected
credential verification/restore, and profile restore through visible UI
elements.
`just android-smoke` resets only Oxid's Android app data, drives the equivalent
development flow, validates the durable public JSON document plus authenticated
credential envelope/key shape, and verifies restart. Both now assert that the
included public submission journal and encrypted issued-credential inventory
survive while development custody and incomplete issuance sessions reset. The
commands are destructive to the selected simulator's Oxid test profile state;
protected development roots and transaction drafts are process-local and are
expected to disappear on restart.

Android processes do not reliably provide `HOME`, so `directories` cannot
resolve the intended durable location there. The JSON adapter deliberately uses
the initialized `ndk-context` plus JNI to resolve `Context.getFilesDir()` on
Android. Path-resolution failure makes the repository unavailable; it never
falls back to temporary or cache storage. Keep that audited unsafe boundary
isolated and do not replace it with cache storage or a package-name-derived
filesystem path. Workspace linting denies unsafe code, every other crate
explicitly forbids it, and the architecture checker rejects unsafe source
outside that reviewed adapter file.

Run repository commands from `nix develop` unless CI performs the equivalent
setup. Keep `Cargo.lock` committed and use workspace dependencies rather than
duplicating versions across manifests.

## Development cycle

1. Start from the current remote base requested for the work. Normal feature
   work integrates into `develop`; `main` is the release branch.
2. Use a dedicated worktree. Do not implement in a dirty primary checkout.
3. Read this file and the blueprint before changing code.
4. Change tests and public documentation with behavior.
5. Run focused checks first, then `./run.sh --light --strict`.
6. Run `npx dev-loops@0.9.0 doctor` and `npx dev-loops@0.9.0 gates`
   before a PR loop. Configuration failures are blockers.
7. Create pull requests as drafts. Do not mark them ready until validation and
   review evidence are recorded.
8. Keep the worktree clean. Never delete unrelated user files or changes.
9. Commit repository-facing work with DCO and GPG:

   ```bash
   git commit -S --signoff -m "<type>: <subject>"
   ```

10. Before pushing, verify both the signature and trailer:

    ```bash
    git log -1 --show-signature --pretty=fuller
    ```

Use conventional commit and PR titles such as `feat(wallet): create profiles`
or `ci: add Rust quality gates`.

`dev-loops@0.9.0 doctor` currently reports 3/4 from a plain shell because it
looks for a standalone `subagent` executable. `pi-subagents@0.42.1` exposes
`subagent` as an in-process Pi tool instead. Confirm that the pinned package is
installed and `dev-loops gates` parses successfully; do not add a dummy binary
to silence the shell probe.

## Validation and coverage

- `cargo fmt --all --check` is mandatory.
- Clippy runs workspace-wide with warnings denied.
- Unit and integration tests run workspace-wide.
- `cargo llvm-cov` enforces 80% line coverage across the core and outgoing
  adapters; incoming adapters and executable shells are excluded from this core
  threshold and remain test/compile-gated.
- `scripts/check-architecture.sh` enforces the initial inward dependency graph.
- `scripts/check-midnight-sources.sh` permits known Midnight ledger/proof crates
  only from the official GitHub repositories with full immutable `rev` pins.
  ADR-0015/ADR-0026 and the dependency reviews remain the gate.
- Security and dependency-policy checks remain distinct from test coverage.
- Bounded RustSec exceptions are documented in
  `docs/security/advisory-exceptions.md`. Review the Dioxus exceptions on every
  Dioxus/Wry update. The pinned Midnight graph also retains unmaintained
  `bincode 2.0.1` through its ZK stack; issue #10 tracks removal. Review it on
  every Midnight update and before production custody or release work. Subxt
  also retains an active build-time `proc-macro-error2` exception; its
  `subxt-lightclient`/Smoldot `libsecp256k1` and `lru` advisories are lockfile-only
  because that feature is disabled. Enabling the light client requires a new
  review rather than another blanket ignore.
- A green aggregate must not hide a skipped core, architecture, security, or UI
  compile lane.
- Coverage thresholds are enforced locally and in CI; hosted reporting may
  visualize results but must never decide whether the gate passes.

## Security and privacy

- Telemetry is off by default. New telemetry requires an ADR and explicit user
  opt-in.
- Never log secrets, seeds, private identifiers, credential claims, signing
  payloads, or raw external error bodies that may contain them.
- Standalone indexer and proof-server HTTP routes are explicit trust-boundary
  configuration and intentionally ignore ambient process proxy variables.
  Keep their HTTP request/status/body tests client-free and transport-free:
  reqwest client construction and loopback are not reliable inside pure Linux
  Nix derivations even when the WebSocket loopback harness succeeds.
- Validate profile labels and all future QR/deep-link/protocol input at the
  boundary before use.
- The JSON profile store contains public labels, identifiers, timestamps, and
  active selection only. It serializes one repository instance; overlapping
  headless processes must use distinct `OXID_PROFILE_STORE_PATH` values.
- The DID JSON store contains validated public DID documents/metadata only. It
  is a separate 128-record/2 MiB owner-private atomic file, rejects symlinks and
  unknown fields, and revalidates domain invariants on read. Never add private
  JWK `d`, credential claims, tokens, endpoint configuration, recovery data, or
  keys. Treat every resolver response as hostile and keep route/body details
  out of errors and logs.
- The Midnight checkpoint file contains public replay state only and supports
  one process writer. It must not be merged into the profile document or used
  as proof that cached inputs are fresh enough to spend.
- The separate Midnight DUST checkpoint is privacy-sensitive tagged wallet
  state scoped by a public-key fingerprint. Never put it in the public account
  JSON, serialize its secret key, or bypass the live-before-spend catch-up.
- Shielded account derivation uses Wallet SDK role `3/0`. Only the canonical
  64-byte coin/encryption public-key address payload may leave the Midnight
  adapter. Zswap secret keys, nullifiers, Merkle paths, witnesses, and tagged
  local state remain adapter-private under ADR-0033; every checkpoint must
  be separately key/network scoped and owner-private.
- Shielded indexer replay decodes only bounded tagged `ZswapInput` and
  `ZswapOutput` events, enforces exact `mt_index == first_free`, recomputes owned
  commitments after local key matching/decryption, collapses foreign branches,
  rehashes at batch boundaries, and removes owned/pending spends by nullifier.
- Shielded checkpoints use a distinct `OXIDZSWP` binary schema, are scoped by
  network, source/protocol identity, and SHA-256 of both Zswap public keys,
  retain partial cursors, and must stay checksummed, bounded, owner-private,
  symlink-resistant, and atomic.
- Shielded sync snapshots expose only network/lifecycle/cursors, current-run
  event count, bounded owned-note/commitment counts, exact public token totals,
  freshness, and sanitized failures. The standalone simulator and native worker
  must borrow the role-3 child before starting and retain no secret material.
  Preserve native connect/ack/idle/total timeouts, WebSocket message/frame
  bounds, linear cursor and non-regressing target checks, 256-event/4 MiB
  replay batches, and one-million-event/512 MiB run limits.
- Transaction attempt status must remain separate from draft lifecycle. Retain
  cancellation primitives inside the Midnight adapter, mark broadcast before
  the node call, restore `Authorized` only after acknowledged pre-broadcast
  cancellation, and never label broadcasting or unknown outcomes retryable.
- Keep production secret storage behind platform-backed adapters. The in-memory
  adapter is development/test infrastructure and must never be presented as
  durable or secure storage.
- Use opaque key references. Key-generation and signing ports must not return
  raw private keys to application or UI layers.
- Record every significant dependency using the review template in the
  blueprint before an adapter becomes production-facing.
- Report vulnerabilities through GitHub private vulnerability reporting, not a
  public issue.

## Public repository hygiene

- Keep documentation and automation public-safe: no tokens, private tracker
  links, private infrastructure names, personal machine paths, or unredacted
  diagnostic output.
- New source files should use an SPDX Apache-2.0 header where practical.
- Pin GitHub Actions to immutable commit SHAs.
- Keep least-privilege workflow permissions and disable persisted checkout
  credentials.
- Do not commit generated `target/`, Dioxus build output, mobile signing data,
  local databases, `.env` files, Pi package installs, or editor state.
- Preserve third-party licenses and provenance when code or assets are migrated.
- Inline Lucide icons retained from the prototype require the ISC notice in
  `THIRD_PARTY_NOTICES.md`; do not remove it while those paths remain.

## Maintaining this guide

Update `AGENT.md` whenever a session establishes a durable fact that a later
engineer would otherwise have to rediscover: selected source commits, accepted
boundaries, non-obvious validation commands, migration decisions, or known
toolchain constraints. Do not use it as a chronological work log or store
ephemeral status that belongs in an issue or PR.
