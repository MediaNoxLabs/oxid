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
| `wallet-core` address, HD, balances, transaction, sync | Midnight addresses, derivation, NIGHT/DUST, build/sign/submit, indexer/node access | chain-neutral chain domain/use cases plus `adapters/midnight` | Network/account reads, simulated/live sync, durable public unshielded plus private DUST/Zswap checkpoint/resume, protected NIGHT/DUST/Zswap receive derivation, native shielded replay, fresh-sync-gated shielded spend, and staged public/private transfer through DUST/Zswap proof, safe pre-broadcast cancellation, and finalized node inclusion implemented for standalone/headless/mobile; ADR-0098/#91 prove funded unshielded and genesis-authority shielded headless finality/adapter-reconstruction flows and add signed-profile plus node-genesis production gates; ADR-0100 implements the distinct protected DUST-registration repository/headless/Dioxus boundary, guarded public PreProd funding manifest/read-only observer, test-only signed Midnight profile, and amount-observed one-output/one-note acceptance harness, while the funded PreProd write/recovery, durable production custody, provisioned deployment, funded mobile flows, and physical-device proof budgets remain gated |
| `wallet-core/secret_storage` and `unlock` | Multi-curve keys, encrypted files, redb, opaque references, boot lock, attempt throttling | wallet-owned session/key-operation ports plus platform-backed and development adapters | ADR-0017/0046/0048/0071/0074–0076 accepted; process-local development custody remains the deterministic harness, normal mobile uses a passcode/user-presence-bound iOS Keychain or Android Keystore sealed vault, and one authenticated complete archive restores custody plus profile-scoped state; complete iOS Simulator and Android emulator picker round trips pass, while physical-device release evidence remains pending |
| `wallet-core/did` and DID services | `did:midnight` create/resolve/update/deactivate | `identity/domain`, `identity/application`, `adapters/did-midnight`, separate public record storage | Current 0.5.0-shaped resolution, profile inventory/persistence, and standalone Ed25519/P-256/Jubjub create/update/deactivate/signing implemented by issues #21–22 and ADR-0036/0037/0047; live Compact writes pending |
| `wallet-core/oid4vp_client` | Self-issued DID authentication mislabeled alongside an unimplemented OID4VP presentation action | `protocol/domain`, `protocol/application`, `adapters/siopv2`; `presentation/domain`, `presentation/application`, `adapters/openid4vp` | SIOPv2 draft-13 login implemented by issue #25/ADR-0040; issue #27/ADR-0043 adds strict Final-shaped DCQL request preview, consent, and replay protection; ADR-0048 adds current-holder authorization and ADR-0050 adds explicit native headless Compact proof plus independent `vp_token` verification |
| `wallet-core/vc_store` and `vc_self_verify` | Signed credential bytes, metadata, self-verification, protected Digital Passport values/openings | `credential/domain`, `credential/application`, `adapters/vc-midnight`, protected credential storage | Profile-scoped protected inventory and strict phase-1 verification implemented by issue #23/ADR-0038; issue #26/ADR-0041/0042 adds atomic opaque material, commitment-bound five-claim Digital Passport interpretation, safe local planning/reveal, restart/deletion, and mobile coverage; ADR-0073 adds active standalone Compact issuer-key, current-time, and pinned-trust policy while status/revocation, production trust, mobile presentation proofs, and remaining native release evidence stay pending |
| `wallet-core/oid4vci_client` and `oid4vci_issuance_e2e` | Pre-authorized offer, token/nonce, holder proof, credential request/store flow | `protocol/domain`, `protocol/application`, `adapters/openid4vci`, existing DID custody and verified credential sink | OpenID4VCI 1.0 Final embedded-offer standalone flow plus separate authentication and managed Jubjub holder-binding methods implemented by issue #24 and ADR-0039/0047; production transport/discovery and additional grant/response variants pending |
| `wallet-core/vault` | Passport-vault contract interaction and selective-disclosure claim | `passport-vault/domain`, `passport-vault/application`, product adapter, not generic wallet core | ADR-0051 delivers exact standalone multi-lock behavior; ADR-0052 adds the immutable five-circuit artifact closure and native tagged-state decoding; ADR-0054/0055 authenticate canonical history/state; ADR-0056/0057 add the retained call harness and explicit simulator; ADR-0058 authenticates the generated client/four wallet proof circuits; ADR-0059 through ADR-0063 add closed-schema public-call composition, exact finalized context, protected NIGHT/DUST proving, finalized submission, cancellation, and restart recovery; ADR-0064 through ADR-0066 add authorization-bound managed-custody claim composition and native discovery; ADR-0068 adds durable owner-private standalone accounting/replay state while real-node/mobile live fixtures remain issue #31 |
| `dioxus-wallet` | Mobile/desktop UI, QR bridges, JS eval bridge, DID/credential/vault screens | `ui-dioxus`, platform adapters, protocol/chain adapters | Profile lifecycle, first-run complete recovery, complete Settings export plus legacy custody import, account-aware Wallet page, receive QR, typed native public-address copy/share, protected development activation, public/shielded Send wizard, DID lifecycle, protected credential inventory/verification, standalone issuance, consented self-issued DID authentication, Digital Passport local reveal/age plan, OpenID4VP request/consent proof gate, standalone Passport Vault journey, typed native vault-call lifecycle, and scan/app-link identity routing are reimplemented without WebView/iframe bridges; ADR-0085 through ADR-0097 add safe labels, the bounded route shell, read-only Home, protected transfer, four-question identity consent, the create/restore onboarding fork, evidence-based backup celebration, unified account sync, the protected-address Receive sheet, build-time brands, render-only secret mode with native snapshot protection, a closed developer capability viewer, the consent-stopping standalone demo drawer, and a compile-time physical-phone tailnet route profile without committed endpoints; physical Android QR success/cancel/timeout, warm/cold custom schemes, protected tailnet account sync, and non-overlapping wallet activation pass, while physical iOS camera, verified HTTPS links, funded device live-transaction fixtures, payment requests, and resource baselines remain pending |
| `headless-wallet` | Line-delimited JSON driver for use cases | `apps/oxid-headless` incoming CLI/test adapter | Safe versioned transport, wallet/identity flows, claim-free Digital Passport planning, Final-shaped OpenID4VP proof/verification, strict redacted identity-request routing, complete standalone Passport Vault accounting, and an ignored double-opt-in funded real-node unshielded finality/restart harness are implemented while raw request URIs, funding roots, and credential/proof private material stay hidden; recovery stays absent from the JSON protocol, with an in-process standalone composition test covering the all-store round trip |
| `dioxus-wallet/src/logs.rs`, `telemetry_panel.rs`, `proc_stats.rs`, and worker boundaries | Persistent tracing, free-form fields, HTTP/operation/process measurements, and background worker visibility | `diagnostics/application`, `adapters/diagnostics-memory`, composed closed-code sinks, headless snapshot/reset, and the Dioxus Diagnostics page | ADR-0080 reimplements only bounded payload-free runtime health and worker recovery; storage, upload, tracing strings, endpoints, process statistics, and benchmark telemetry remain excluded |
| capability/worker visibility adjacent to the prototype diagnostics tabs | Useful development discovery was mixed with unsafe log and benchmark surfaces | `capabilities/application`, headless `system.capabilities`, and the opt-in standalone Dioxus developer profile | ADR-0095 keeps only closed public composition facts, corrects confirmation declarations, reports timing as `not_collected`, and proves the normal release excludes the developer marker; ADR-0096 adds a separate compile-time demo drawer that serializes safe setup and stops every protocol fixture at unchanged review |
| `prover-core` | Local/HTTP proof execution and benchmark paths | Midnight proving adapter | Private local DUST proving implemented with an authenticated bounded cache; remote proving retained for explicit development |
| benchmark crates and fixtures | Mobile proving measurements and test circuits | dedicated opt-in adapter harness | One real DUST proof/seal/codec harness is measured on iOS/Android; ADR-0072 adds an authenticated executable-embedded presentation artifact package and ADR-0083 runs it through an explicit foreground worker, while physical-device budgets remain gated; generated artifacts remain uncommitted |
| Android/iOS projects | WebView hosts, permissions, QR bridges | `apps/oxid` platform hosts plus the shared `adapters/mobile-native-plugin` | Dioxus-generated hosts build and launch explicit development or native-custody standalone composition; the single static Swift/Kotlin plugin packages QR, links, typed clipboard/share, device custody, bounded backup documents, and one boolean screen-privacy operation (`FLAG_SECURE`/iOS background overlay); Android JNI failures clear pending Java exceptions without exposing details, and an emulator throw-then-full-wallet regression covers continued bridge use; disposable iOS Simulator and Android emulator flows verify complete native export/reset/import/recovery round trips through their system document pickers; Samsung SM-S928B / Android 16 physical evidence proves QR success/cancel/timeout, post-return liveness, consent isolation, warm/cold custom schemes, numeric `FLAG_SECURE`, protected tailnet account sync, durable public binding, honest process-local restart state, and real-touch Scan/activation separation; ADR-0097 adds a development-only MagicDNS/TLS physical-phone launcher without copying the prototype's personal endpoint, while physical iOS, multi-vendor screenshot behavior, universal links, funded live transactions, and resource baselines remain deferred |

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
  captured/free-form diagnostic logs; ADR-0080 permits only reviewed closed
  codes in a non-persistent bounded process-local ring;
- generated Android/iOS project output and signing configuration;
- benchmark-only probes, tabs, process statistics, and telemetry panels.

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
Standalone completion resumes from the next sparse cursor, receives at most
16,384 events/16 MiB, closes the subscription, then folds and checkpoints in
256-event/4 MiB batches instead of retaining the prototype's history-sized
queue or folding under transport backpressure. A current checkpoint still
needs a successful live subscription; wrong scope or parameters replay
cleanly, incompatible deltas retry once from zero, and transport failure never
authorizes a cached-only spend. Headless composition accepts the store only
with the complete standalone route set.

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
consistent official-state batch, resumes from the next sparse cursor, and
retries an incompatible cached delta once from zero. Unlike the prototype's
explicitly provisional inline v1, cold catch-up receives a bounded segment,
completes and drops its subscription, then replays/checkpoints before
reconnecting from the observer-accepted cursor. Headless environment
composition accepts the private store for read-only or complete standalone
live modes.

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
ADR-0083 reuses the same proof/verification boundary only in the explicit
native-custody mobile artifact build, behind one foreground worker with
cancel/background/timeout late-result disposal. Normal mobile remains
proof-disabled.
ADR-0082 closes the multi-credential consent gap left by the prototype and the
first mobile adapter: safe candidate previews now include display name, issuer,
and opaque reference; Dioxus visibly selects a sole match but requires an exact
card choice before consent when several credentials match. Headless continues
to require the exact candidate identifier and never exposes claim values.
ADR-0048 first reloads the credential-bound Jubjub
method, requires current managed assertion authority, signs and independently
verifies a disposable authorization over the exact statement, and applies
explicit same-method rotation semantics. ADR-0050 wires credential-family proof
execution and an independent proof verifier for native headless mode; ADR-0083
reuses them only in the explicit mobile conformance build.
ADR-0073 separately hardens acceptance of each newly issued standalone Compact
credential: the exact issuer DID assertion method must resolve to the detached
proof's Jubjub key, issuance/proof/expiry times must be current, and the pinned
standalone trust anchor must match. Revocation remains visibly not checked.

ADR-0074 begins the prototype backup migration without copying its unsafe
storage boundary. The reviewed `WalletBackupCard` and
`wallet-core/src/store/backup.rs` reuse live-store ciphertext/password state,
accept arbitrary paths, overwrite conflicts, and can continue after partial
record failures. Oxid instead has one versioned, bounded, profile-bound
Argon2id/XChaCha20-Poly1305 custody package behind an application-owned port.
Development and OS-wrapped mobile custody can restore an exact root, generated
keys, derivation paths, and opaque references only into an empty destination;
mobile export forces fresh native authorization. ADR-0075 now transfers only
the encrypted package through fixed-name, user-selected iOS/Android document
pickers and exposes exact-confirmation custody-only Settings UX. ADR-0076 adds
the authenticated all-store archive, custody-last coordinator, complete
Settings export, fresh-install recovery, and in-process standalone round trip.
ADR-0078 additionally hardens new complete-wallet exports as strict
`OXIDBAK1` version 3 with Argon2id at 65,536 KiB/t=3/p=1 while preserving exact
read-only version-2 compatibility and rejecting cross-version parameter tuples
before KDF allocation. The complete iOS Simulator and Android emulator
document-picker round trips are covered by `just ios-backup-smoke` and `just
android-backup-smoke`; physical-device resource evidence remains issue #33 work
and is not represented as release parity.

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
pinned Rust ledger codec. Its original public schema rejects claims, excludes
administration, and does not expose raw circuit arguments. ADR-0060 installs
the composer behind a native retained application-port adapter. That path
requires canonical replay
plus real bounded serialized Zswap/ledger-parameter state, retains transaction
bytes only in a zeroizing adapter buffer, and supports truthful
prepare/authorize behavior. ADR-0061 supplies its exact replay-matched public
Midnight context. ADR-0062 then derives the native NIGHT deficit from the
generated call only after exact authorization, selects synchronized unshielded
UTXOs, returns change, and signs every input inside protected custody. Submit
was completed by ADR-0063 through the shared DUST proof, durable journal, node
submission, and reconciliation path. ADR-0064/0065 add a distinct protected
claim schema: preparation retains authenticated public plan inputs only, while
exact call authorization triggers credential re-verification, managed holder
authorization/signing, generated `claimFromLock` composition, and the same
funding/settlement path. ADR-0066 proves that path from standalone OpenID4VCI
issuance and managed Jubjub DID custody through the packaged client and terminal
Midnight completion, then enables native claim capability discovery.

The standalone adapter implements bounded multi-lock creation, deposit,
credential-policy claim, creator withdrawal, total accounting, exact consent,
and per-lock credential-root replay protection. The credential adapter verifies
the exact Compact issuer proof, signed commitments/openings, pinned standalone
issuer key, expiry, age, optional state/document predicates, and verifier time.
Headless and Dioxus call the same application use cases and expose neither
claim values nor credential roots. The UI and protocol label the source
`standalone`, report its independent `owner_private_atomic_file` persistence,
and do not claim chain submission.
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
`included` status are process-local harness outcomes only. ADR-0061 now connects
the retained native adapter in complete standalone composition to exact
profile-scoped public Midnight addresses plus bounded Zswap state and current
ledger parameters. The indexer action and state must match canonical replay
before composition. ADR-0062 funds exact authorized create/deposit calls through
the protected Midnight account boundary and rejects NIGHT funding for withdraw.
ADR-0063 then routes create/deposit/withdraw through the existing DUST
balancing, proving, persist-before-broadcast, node submission, cancellation,
and finalized reconciliation path. A configured submission journal restores
public status across process restarts. ADR-0064 replaces the prototype's unsafe
claim-presentation construction with a protected `vc-midnight` source:
it re-verifies the exact credential/private material and contract issuer anchor,
derives policy time from finalized chain time, reauthorizes the current managed
holder method, obtains a fresh custody-backed Jubjub proof, independently
verifies it, and returns a zeroizing fixed-shape composer DTO. ADR-0065 consumes
the DTO only after exact `AUTHORIZE_PASSPORT_VAULT_CALL`
confirmation. It obtains byte-exact issuer/lock policy and finalized time from
canonical replay, binds them with the credential ID and Midnight public context
in a claim-plan challenge, and invokes the fixed generated schema only during
authorization. Failures retain no presentation; concurrency and expiry fail
closed. ADR-0066 completes the managed-custody generated-client conformance run
using a holder-bound issued credential and terminal Midnight completion, so
`native_settlement` discovery reports `settlesOnMidnight: true` and includes all
four wallet operations. WebView JavaScript, iframe origins, hard-coded
addresses, and relative workspace paths remain excluded. Optional authenticated
state caching, real-node fixtures, and device resource baselines remain explicit
issue #31 backlog work.

ADR-0067 exposes the native state and call lifecycle on the Dioxus Vault page
without importing the prototype's WebView or iframe bridge. Standalone ledger,
deterministic call simulation, and authenticated Midnight settlement are
labelled as distinct sources. All four wallet-facing operations require a
public preview, exact authorization, separate proving/submission, and
reconciliation after ambiguous broadcast outcomes. Native development builds
reuse the headless environment-aware standalone composition; device live-node
fixtures and resource baselines remain pending.

ADR-0068 persists the separate standalone conformance ledger at a bounded,
owner-private, atomic path while retaining its `standalone` source. Complete
domain snapshot validation restores locks, totals, next identifier, and the
per-lock credential-root replay set across headless and mobile process restarts.
The file is never a source for canonical replay, native call authorization, or
Midnight settlement.

ADR-0069 migrates the prototype's scan-first identity entry through a new
platform port and a separate strict protocol adapter. iOS packages an
AVFoundation QR scanner, Android packages Google Code Scanner 16.1.0 in QR-only
mode, and Dioxus routes a successful classification into the existing
OpenID4VCI, SIOPv2, or OpenID4VP preview/consent journey. The headless
`identity.request.route` method proves the same three routes without echoing the
raw request. Because SIOPv2 and presentation share the `openid4vp` scheme,
standalone classification requires exact registered client/request endpoint
pairs and unknown links fail closed. ADR-0070 adds warm/cold OS custom-scheme
delivery through that exact router and typed public-address-only native
copy/share. Issue #32 owns physical-device scanning, universal HTTPS links,
production discovery, and resource evidence.

ADR-0070 deliberately exceeds the prototype's QR-only/no-op-clipboard mobile
edge. iOS captures Tao open events before component construction; Android's
repository-owned `singleTop` activity handles both launch and `onNewIntent`.
One pending request cannot replace an active review, and no link bypasses the
existing preview/consent flow. `PublicTextExportPort` has no generic string
method, so credential, proof, protocol, and secret material cannot be exported
through it. A single repository-owned Manganis package avoids the selected
Dioxus 0.7.10 iOS multiple-framework embedding limitation.

ADR-0077 retains the prototype's deliberate heavy-operation worker separation
without migrating its aggregate `WorkMsg` wallet facade, thread-local outcome
router, seed/controller-secret messages, or UI coupling. Native Oxid Dioxus
dispatches every wallet/SSI use-case path that can reach persistence, custody,
cryptography, transport, or non-trivial protocol work to a private named 8 MiB
thread and receives only the existing typed application result over a one-shot.
This includes polling complete async application futures off the UI executor,
because their bodies can contain synchronous encrypted-repository or crypto
work around awaits. Dioxus signals remain on the UI executor, busy state
prevents duplicate dispatch, and worker failures expose no adapter or payload
detail. Issue #42's audit is complete: only strict bounded identity parsing,
already-published DUST/Zswap snapshots, retained draft/status reads, and
non-waiting cancellation signals remain direct under explicit port contracts.

ADR-0071's mobile custody path also keeps initial public account rendering out
of protected derivation while the vault is uninitialized or locked, so an app
launch cannot summon a device-credential prompt without explicit user intent.
Settings re-reads native status after the OS authorization activity resumes.
The Android conformance harness proves a distinct process, explicit restart
unlock, unchanged opaque sealed record, current schema-2 account association,
and the same protected public address after derivation has actually completed;
fixture addresses and process-local sync state are not accepted as that proof.

The prototype claim composer also derives a holder scalar from the public
credential claim root and fixes the presentation nonce to `17`; Oxid requires
opaque managed holder custody and fresh randomness instead of migrating either
shortcut.

Additional internal/change address management, replacement handling, live DID
writes, live OpenID4VP response delivery and physical-device Compact proving
budgets,
credential status/revocation plus production issuer trust, physical iOS camera
and native-custody physical-device evidence, universal HTTPS links, production
endpoint discovery, physical-device recovery interruption/resource evidence,
and device resource baselines remain separate follow-ups.

The current evidence classification, exact implemented paths,
dependency-ordered gaps, blockers, and acceptance status are recorded in
[the 2026-08-20 migration delivery audit](delivery-audit-2026-08-20.md). Stale
issue checkboxes are not evidence for that assessment.

ADR-0097 reimplements the prototype's runtime-selected localhost/Tailscale
transport aliasing as separate compile-time development profiles without
copying its personal endpoint, public genesis wallet, or runtime production
switch. The physical Android tailnet build derives a protected
profile account, synchronizes it through the laptop-hosted
`indexer-standalone:4.0.0`, and persists only its public network/derivation
coordinates through the same profile repository. A process restart truthfully
withholds the address because development custody is process-local. The
prototype's exact unshielded subscription requests neither fee field and
therefore avoids the schema discrepancy. Oxid's richer history needs the value;
the live image rejected the singular `fee` selection despite the reviewed
schema advertising it, so Oxid uses the image-proven `fees { paidFees }`
response shape. The localhost profile uses the same undeployed chain identity
and shared profile binding with immutable loopback routes: iOS Simulator reaches
them directly and Android emulator reaches them through exact `adb reverse`
mappings for 8088, 9944, and 6300. It is distinct from deterministic simulation
and cannot be combined with native custody, tailnet routes, or WebAssembly.

ADR-0098 consumes the reviewed prototype's working live-flow semantics without
copying its public genesis funding fixture. A closed Ed25519-signed canonical
profile atomically binds Midnight network/genesis/routes and SSI metadata
routes; the node must prove the signed genesis before opt-in composition, and
normal composition remains unavailable without reviewed roots and a deployed
profile. The test-only funding harness receives the development root
out-of-band, zeroizes it, transfers exactly five NIGHT to a fresh OS-random
recipient, proves finalized inclusion, reconstructs the public journal with
included-status restoration through the reconciliation use case, and waits
boundedly for a stable recipient read. Live evidence
also corrects indexer v4 millisecond time and sparse monotonic DUST global
cursors, both matching the prototype's successful path.

Issue #91 extends that guarded evidence beyond the prototype, whose shielded
sync/balance helpers had no production Dioxus or headless call sites and whose
final vault flow remained unshielded. Oxid now validates the indexer v4
`ZswapLedgerEvent` envelope, treats its IDs as sparse monotonic global cursors,
spends a real genesis-funded native Zswap note after exact consent, waits for
finality, blocks an unchanged-state duplicate, reconstructs the adapter, and
proves exact sender/recipient balances after nullifier replay. This remains a
genesis-authority development proof: the repository now implements typed DUST
registration, but a fresh protected recipient still needs guarded funded
registration, later generation/recovery, and spend evidence under issue #92.

ADR-0100 records the registration boundary implemented in Oxid.
The reviewed prototype is precise about the product sequence—sync, wait for
NIGHT, register for DUST, then wait for generated DUST—and its indexer fixture
includes `registeredForDustGeneration`. Its executable wallet did not
implement the documented `registerNightUtxosForDustGeneration` step, and its
plan left register/deregister unchecked. Oxid therefore does not copy or claim
prototype code parity for this operation.

Issue #92 instead maps the accepted ledger revision's native semantics through
a distinct `WalletDustRegistrationPort`. Preparation requires live public
`ctime` and unregistered-state evidence, puts the largest-generation owned
NIGHT input alone in the guaranteed offer, puts the remaining selected inputs
in the fallible offer, and returns every exact NIGHT amount to the same owner.
Explicit
consent then gates the role-0 NIGHT signature while the role-2 DUST child stays
inside protected custody. Generic proving, registration-domain-separated
persist-before-broadcast recovery, node finality, and official DUST-event
observation are separate stages. Public checkpoint schema version two carries
the eligibility fields; version one is rejected and ignored. The
repository/headless/Dioxus slice is implemented, including explicit consent,
transfer-journal separation, and a guarded public PreProd A/B funding manifest.
A fresh wallet intentionally begins with zero DUST; a test-only signed PreProd
Midnight profile and ignored acceptance harness are implemented, while funded
registration-to-generation/authoritative-resynchronization, mobile, restart,
physical-device, and production results remain open. The funding case binds
one eligible positive public output carrying all observed public NIGHT and one
positive shielded note, not balances alone.

ADR-0084 begins the accepted product-UX rollout without copying the
prototype's ad-hoc stylesheet values. The migrated Dioxus surface now separates
complete dark/light brand primitives from one semantic component vocabulary;
fixed safety colors cannot be rebranded, dark remains the only selected scheme,
and `run.sh` rejects raw component colors plus legacy type/radius/motion drift.
This is a presentation-only prerequisite for the four-tab route shell and does
not change wallet state machines, capability truth, consent, or composition.
ADR-0092 realizes that boundary as a strict build-only pack: `apps/oxid`
selects `brands/oxid`, generates semantic CSS/logo/typed identity into its own
`OUT_DIR`, and injects only immutable presentation context. The closed schema,
two-scheme contrast matrix, exact manifest purpose strings, safe-SVG checks,
security-copy snapshots, and auto-enumerated Nix checks add reproducible
white-label infrastructure without importing prototype branding, runtime brand
selection, or any wallet/SSI authority.

## Gate for each later slice

Every migrated capability needs:

1. Oxid-owned domain and application types;
2. focused incoming/outgoing ports;
3. one adapter with provenance and dependency review;
4. unit plus port-contract/integration tests;
5. security/privacy review for sensitive data or authorization;
6. an ADR when the architecture or dependency direction changes;
7. a Tier-1 mobile smoke test when user-facing.
