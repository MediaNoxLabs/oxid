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
| `wallet-core` address, HD, balances, transaction, sync | Midnight addresses, derivation, NIGHT/DUST, build/sign/submit, indexer/node access | chain-neutral chain domain/use cases plus `adapters/midnight` | Network/account reads, simulated/live sync, durable public unshielded plus private DUST/Zswap checkpoint/resume, protected NIGHT/DUST/Zswap receive derivation, native shielded replay lifecycle, and staged unshielded transfer through DUST proof, safe pre-broadcast cancellation, and node inclusion implemented for development/headless; shielded spending and durable production custody pending |
| `wallet-core/secret_storage` and `unlock` | Multi-curve keys, encrypted files, redb, opaque references, boot lock, attempt throttling | wallet-owned session/key-operation ports plus platform-backed and development adapters | ADR-0017/0046/0048 accepted; process-local Ed25519/P-256/Jubjub plus BIP32/secp256k1-Schnorr conformance, selected-DID issuance binding, and presentation-time current-holder reauthorization implemented; durable recovery and native custody pending |
| `wallet-core/did` and DID services | `did:midnight` create/resolve/update/deactivate | `identity/domain`, `identity/application`, `adapters/did-midnight`, separate public record storage | Current 0.5.0-shaped resolution, profile inventory/persistence, and standalone Ed25519/P-256/Jubjub create/update/deactivate/signing implemented by issues #21–22 and ADR-0036/0037/0047; live Compact writes pending |
| `wallet-core/oid4vp_client` | Self-issued DID authentication mislabeled alongside an unimplemented OID4VP presentation action | `protocol/domain`, `protocol/application`, `adapters/siopv2`; `presentation/domain`, `presentation/application`, `adapters/openid4vp` | SIOPv2 draft-13 login implemented by issue #25/ADR-0040; issue #27/ADR-0043 adds strict Final-shaped DCQL request preview, consent, and replay protection; ADR-0048 adds current-holder authorization and ADR-0050 adds explicit native headless Compact proof plus independent `vp_token` verification |
| `wallet-core/vc_store` and `vc_self_verify` | Signed credential bytes, metadata, self-verification, protected Digital Passport values/openings | `credential/domain`, `credential/application`, `adapters/vc-midnight`, protected credential storage | Profile-scoped protected inventory and strict phase-1 verification implemented by issue #23/ADR-0038; issue #26/ADR-0041/0042 adds atomic opaque material, commitment-bound five-claim Digital Passport interpretation, safe local planning/reveal, restart/deletion, and mobile coverage; Compact presentation proofs, status/schema/trust policy, and native wrapping remain pending |
| `wallet-core/oid4vci_client` and `oid4vci_issuance_e2e` | Pre-authorized offer, token/nonce, holder proof, credential request/store flow | `protocol/domain`, `protocol/application`, `adapters/openid4vci`, existing DID custody and verified credential sink | OpenID4VCI 1.0 Final embedded-offer standalone flow plus separate authentication and managed Jubjub holder-binding methods implemented by issue #24 and ADR-0039/0047; production transport/discovery and additional grant/response variants pending |
| `wallet-core/vault` | Passport-vault contract interaction and selective-disclosure claim | `passport-vault/domain`, `passport-vault/application`, product adapter, not generic wallet core | ADR-0051 delivers exact standalone multi-lock behavior; ADR-0052 adds the immutable five-circuit artifact closure and native tagged-state decoding; ADR-0054/0055 authenticate canonical history/state; ADR-0056/0057 add the retained call harness and explicit simulator; ADR-0058 authenticates the generated client/four wallet proof circuits; ADR-0059 adds closed-schema create/deposit/withdraw composition while claim custody, port completion/funding/submission, and durable standalone state remain issue #31 |
| `dioxus-wallet` | Mobile/desktop UI, QR bridges, JS eval bridge, DID/credential/vault screens | `ui-dioxus`, platform adapters, protocol/chain adapters | Profile lifecycle, account-aware Assets page, receive QR, protected development activation, staged transfer, DID lifecycle, protected credential inventory/verification, standalone issuance, consented self-issued DID authentication, Digital Passport local reveal/age plan, OpenID4VP request/consent proof gate, and the standalone Passport Vault journey are reimplemented; mobile proving and native bridges remain deferred |
| `headless-wallet` | Line-delimited JSON driver for use cases | `apps/oxid-headless` incoming CLI/test adapter | Safe versioned transport, wallet/identity flows, claim-free Digital Passport planning, Final-shaped OpenID4VP proof/verification, and complete standalone Passport Vault create/deposit/claim/replay/withdraw accounting are implemented while credential/proof private material stays hidden |
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
The lifecycle increment adds an Oxid-owned status/start/cancel boundary with exact
per-token balances and note/commitment counts. Its deterministic standalone
controller verifies the protected role-3 child and drives headless plus mobile
cancellation/resume flows. The native live increment connects the same port to
a bounded `graphql-transport-ws` worker over `zswapLedgerEvents`, saves every
consistent official-state batch, resumes from the next cursor, and retries an
incompatible cached delta once from zero. Headless environment composition
accepts the private store for read-only or complete standalone live modes.

Issue #19 and ADR-0034 add a deliberate improvement over the prototype's
submit-and-wait flow: Oxid-owned submission status plus explicit headless and
mobile cancellation before the adapter's atomic broadcast boundary. An
acknowledged cancellation restores the authorized draft for explicit retry;
after broadcast, cancellation is refused and unknown outcomes remain blocked.

[Issue #21](https://github.com/MediaNoxLabs/oxid/issues/21) and ADR-0036 begin
the identity peer capability with the prototype's DID inventory and resolve
flow. Oxid-owned dependency-free domain/application crates follow the current
Midnight DID 0.5.0 syntax and all seven public JWK curve profiles rather than
copying the prototype's older subset. A deterministic adapter resolves exactly
one documented fixture; a native adapter consumes the official bounded
`POST /resolve` contract only through explicit trusted configuration. Public
documents are profile-scoped in a separate strict owner-private JSON file and
survive headless/mobile restart. Full provenance and the threat boundary are in
[midnight-did-provenance.md](midnight-did-provenance.md).

[Issue #24](https://github.com/MediaNoxLabs/oxid/issues/24) and ADR-0039 migrate
the prototype's pre-authorized credential issuance behavior while replacing
its pre-final draft shapes with OpenID4VCI 1.0 Final. New dependency-free
protocol domain/application crates own metadata-only preview, consent, and
state. `adapters/openid4vci` strictly parses one embedded offer, separates
issuer and OAuth metadata, obtains a deterministic nonce, produces the final
`proofs.jwt` request with an existing managed DID authentication key, parses
the final `credentials` response, and sends bytes through a valid-only import
sink. The in-process standalone issuer keeps grant codes, tokens, nonces, and
proofs inside the adapter boundary; headless and Dioxus expose none of them.
Normal composition remains unavailable, and live HTTP/discovery plus every
non-reviewed flow variant are separate follow-ups.

[Issue #25](https://github.com/MediaNoxLabs/oxid/issues/25) and ADR-0040 split
the prototype's implemented login behavior from the unimplemented credential
presentation modes that shared its `oid4vp_client` name. A protocol-neutral
self-issued-authentication aggregate owns bounded preview, exact consent, and
metadata-only state. `adapters/siopv2` strictly accepts one deterministic
request-by-reference SIOPv2 draft-13 profile, creates an EdDSA or ES256 ID Token
through opaque DID custody, and has the in-process verifier independently
resolve and verify it once. Headless and Dioxus expose verifier, purpose, and
outcome but never nonce, state, signing input, or token. Final OpenID4VP
`vp_token`/DCQL presentation remains a distinct follow-up.

[Issue #26](https://github.com/MediaNoxLabs/oxid/issues/26), ADR-0041, and
ADR-0042 migrate the prototype's Digital Passport private parts and visible
claim controls without copying its schema into wallet core. Credential records
own one bounded opaque material blob inside the same encrypted atomic lifecycle
as the signed credential. `adapters/vc-midnight` parses the exact five values
and openings and recomputes their official Midnight commitments plus the signed
claim root before it exposes claim-value-free candidates. Dioxus reveals/hides
first and last name only after a local action and plans the prototype's
age-over-threshold choice; headless exposes candidates and plan only and has no
claim reveal method. Deterministic standalone issuance, process restart,
profile isolation, atomic deletion, and iOS/Android flows are covered. The plan
explicitly reports that no presentation was generated.

[Issue #27](https://github.com/MediaNoxLabs/oxid/issues/27) and ADR-0043 add
the missing “extra 10%” request and consent boundary without copying the
prototype's disabled-action fiction. `presentation/domain` and
`presentation/application` own schema-neutral preview, candidate, consent, and
single-use state. `adapters/openid4vp` strictly accepts one deterministic
request-by-reference OpenID4VP 1.0 Final-shaped DCQL profile and maps it to the
commitment-bound Digital Passport candidates. Headless and Dioxus show the
verifier, purpose, and exact claim intents without values. Acceptance always
re-verifies the protected exact Compact bundle, constructs and independently
checks the generated-circuit public statement, and consumes the session. Without
an explicit authenticated artifact root it fails with `proof_unavailable`.
ADR-0048 first reloads the credential-bound Jubjub
method, requires current managed assertion authority, signs and independently
verifies a disposable authorization over the exact statement, and applies
explicit same-method rotation semantics. ADR-0050 wires credential-family proof
execution and an independent proof verifier for native headless mode only.

[Issue #2](https://github.com/MediaNoxLabs/oxid/issues/2), ADR-0051, and
[issue #31](https://github.com/MediaNoxLabs/oxid/issues/31) migrate the
prototype's Passport Vault as a product-specific hexagon. The reviewed wallet
source remains `midnight-ledger` commit
`074b1a4bccbfee1740ee188374b606a022ecef42`. The prototype's ambient companion
workspace was independently resolved to `midnight-identity-solution-examples`
commit `e4a92a6be2cc6dc34f68261f10c19c9312043807`; the reviewed
`packages/contracts/vault/src/passport-vault.compact` has SHA-256
`2ebc5b34dd440bc9a9736408f29f5003e7a78f26a564b392be2af36de69102f4`.
ADR-0052 composes its five impure circuits, generated ledger client/schema, IR,
proving keys, parameters, and digest manifest in
`passport-vault-compact-artifacts`. The upstream repository is private, so
ADR-0053 distributes its byte-identical Apache-2.0 source at
`contracts/passport-vault/passport-vault.compact` and asserts the upstream
digest before compilation. Public CI remains secret-free and generated files
remain Nix outputs.

ADR-0058 adds the runtime half of that build boundary. The product adapter
streams every executable wallet artifact through compiled-in exact size and
SHA-256 expectations, verifies the generated four-operation ABI and encoded
ZKIR degrees, and implements Midnight's native resolver/parameter traits for
`createLock`, `depositToLock`, `claimFromLock`, and `withdrawFromLock` only.
`setTrustedIssuer` remains authenticated build evidence but cannot resolve
through the wallet capability. ADR-0059 now packages the generated client as a
one-request, closed-schema composer and proves that `createLock`,
`depositToLock`, and `withdrawFromLock` produce transactions accepted by the
pinned Rust ledger codec. It rejects claims until protected credential custody
and fresh presentation randomness are available, excludes administration, and
does not expose raw circuit arguments. The composer is not yet installed behind
the retained application port, so combined contract/DUST proof provision,
NIGHT funding, node submission, and durable reconciliation are the next issue
#31 increments.

The standalone adapter implements bounded multi-lock creation, deposit,
credential-policy claim, creator withdrawal, total accounting, exact consent,
and per-lock credential-root replay protection. The credential adapter verifies
the exact Compact issuer proof, signed commitments/openings, pinned standalone
issuer key, expiry, age, optional state/document predicates, and verifier time.
Headless and Dioxus call the same application use cases and expose neither
claim values nor credential roots. The UI and protocol label the source
`standalone`, state `process_local`, and do not claim chain submission.
Production remains fail-closed. The native adapter now decodes the exact public
version-1 tagged ledger with bounded layout/accounting/nullifier checks, and the
headless `vault.contract_state.decode` method exercises a generated-client
fixture without claiming that supplied bytes are live or fresh. Issue #31 owns
the remaining acquisition trust boundary: ADR-0054 now queries an explicit
address at a node-finalized height, verifies the action block's canonical node
hash, and exposes `indexer_supplied_not_proven` because block anchoring alone
does not authenticate returned state bytes. ADR-0055 selects deterministic
replay and adds a bounded native verifier for official raw transactions,
inner hashes, ordered node operation outcomes, guaranteed/fallible semantics,
exact public transcripts, effects, and contract balances. The node pallet emits
applied calls, deployments, and maintenance in separate typed batches rather
than raw action order; the verifier authenticates that exact ordering. The
native collector treats the deployment height as an untrusted hint, validates
the exact deployment event, reads the historical runtime schema from each
block's parent state, and observes every canonical finalized block through one
captured head. It rejects gaps, forks, wrapped target calls, missing archival
data, event/hash disagreement, and bounded-resource overflow. The opt-in
headless standalone composition joins collection and replay into a typed
`finalized_node_replay` / `canonical_finalized_replay` read source, exposes the
latest target transaction and captured finalized head, and permits only one
scan at a time. ADR-0056 now supplies the capability-specific boundary for all
four user operations: typed retained prepare/authorize/submit drafts, separate
confirmation intents, authenticated-replay admission, bounded public history,
safe cancellation, finalized reconciliation, and a headless flow harness.
ADR-0057 executes that full protocol in zero-configuration headless/development
composition with a fixed fixture address and a distinct
`deterministic_simulation` authentication class. Capability discovery reports
`settlesOnMidnight: false`; its deterministic transaction/block hashes and
`included` status are process-local harness outcomes only. Explicit live and
production composition still fail closed because the generated-Compact
composer is not yet connected to the retained call port and funding, DUST
balancing, proving, submission, durable public journaling, and optional state
caching remain issue #31 adapter work. WebView JavaScript, iframe
origins, hard-coded addresses, and relative workspace paths remain excluded.
The prototype claim composer also derives a holder scalar from the public
credential claim root and fixes the presentation nonce to `17`; Oxid requires
opaque managed holder custody and fresh randomness instead of migrating either
shortcut.

Shielded spending, internal/change address management, replacement handling,
live DID writes, live OpenID4VP response delivery and mobile Compact proving, camera/copy/share
bridges, production endpoint discovery, durable recovery, and native custody
remain separate follow-ups.

## Gate for each later slice

Every migrated capability needs:

1. Oxid-owned domain and application types;
2. focused incoming/outgoing ports;
3. one adapter with provenance and dependency review;
4. unit plus port-contract/integration tests;
5. security/privacy review for sensitive data or authorization;
6. an ADR when the architecture or dependency direction changes;
7. a Tier-1 mobile smoke test when user-facing.
