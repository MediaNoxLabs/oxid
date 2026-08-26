# AGENT

Engineering guide for agents and contributors working in `oxid`.

This repository is the public, standalone home of the Oxid identity wallet. The
root `OXID_IDENTITY_WALLET_BLUEPRINT.md` is the product and architecture north
star. `docs/integration-delivery.md` is the authoritative base, CI, and
merge contract for issue-backed work. When this guide and the blueprint
differ, preserve the blueprint's
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
DIDs, protected credential inventory, embedded pre-authorized OpenID4VCI
issuance, and consented self-issued DID authentication are now functional in
standalone development. Digital Passport private parts are commitment-bound and
encrypted; safe headless disclosure planning plus explicit local Dioxus
first/last reveal and age-threshold planning are functional. Strict standalone
OpenID4VP request, matching, and consent are functional and the real Compact
presentation artifact set is reproducible. Standalone OpenID4VCI now issues,
encrypts, restores, and natively verifies the prototype's exact Compact
credential shape plus a detached issuance proof dynamically bound to the
selected profile's managed Jubjub assertion method. ADR-0073 additionally
requires active standalone verification to resolve the exact issuer DID
assertion method, match its Jubjub key to the detached proof, enforce current
issuance/proof/expiry time policy, and match the pinned standalone trust anchor;
status remains explicitly `not_checked`. ADR-0051 now delivers the
Passport Vault as a separate standalone product hexagon with exact multi-lock
accounting, Digital Passport claim policy, replay rejection, a headless flow,
and a Dioxus mobile journey. It is visibly process-local and never an on-chain
claim. ADR-0052 builds all five impure circuits and adds bounded native Rust
decoding plus a headless generated-client fixture. ADR-0053 distributes the
byte-identical reviewed contract in Oxid and asserts its upstream digest so
public CI never needs private companion-repository credentials. ADR-0054 adds
an address-scoped standalone read source that queries the indexer at the node's
latest finalized height and verifies the action block's canonical node hash.
Its source is always `node_anchored_indexer` and its authentication label is
always `indexer_supplied_not_proven`: the transaction contains replayable call
transcripts, not the post-call state, so only deterministic replay from an
authenticated prior state or a reviewed storage proof can authenticate those
bytes. Never use this read model to authorize or compose a contract call.
ADR-0055 selects deterministic replay and implements the transport-independent
native verifier for official raw transactions, node operation outcomes,
guaranteed/fallible semantics, exact public transcripts/effects, and checked
contract balances. The native finalized-history collector treats a
non-genesis deployment height as an untrusted hint, authenticates its exact
deployment event, resolves each historical runtime schema at the block's parent
state, and verifies every header/parent link through one captured finalized
head. Opt-in headless standalone composition now exposes collected and replayed
state as `finalized_node_replay` / `canonical_finalized_replay`; the legacy
indexer route remains unproven. ADR-0056 adds the typed four-operation retained
contract-call lifecycle and headless harness. ADR-0057 wires that lifecycle only
into zero-configuration headless/development composition with a distinct
`deterministic_simulation` authentication class, fixed fixture address, and
`settlesOnMidnight: false`. Its included outcomes are process-local simulation,
not Midnight settlement. ADR-0058 authenticates the generated client/ABI and
exposes the four wallet proof circuits through a native resolver while excluding
the administrative circuit. ADR-0059 packages a bounded one-request generated-
Compact composer for typed create/deposit/withdraw operations and proves its
output matches the pinned Rust ledger codec. It rejects claims and
administration. ADR-0060 installs it behind a native retained application-port
adapter that requires canonical replay plus real bounded Zswap/ledger context,
keeps the unproven transaction in zeroizing adapter custody, and deliberately
leaves submit unavailable. ADR-0061 now supplies that context only in the
complete standalone composition: the Midnight adapter decodes exact
profile-scoped public address payloads, canonical replay must match the
node-anchored indexer action and state byte-for-byte, and the composition root
joins the ports when the immutable packaged composer is configured. ADR-0062 funds an
exactly authorized native create/deposit draft from synchronized unshielded
NIGHT UTXOs inside protected Midnight custody, returns change, verifies the
opaque Schnorr authorization, and supplies one signature per input. Withdraw
must require no NIGHT funding. Complete standalone composition reports
`native_funded_draft` while retaining the funded transaction only in zeroizing
adapter custody. ADR-0063 routes native create/deposit/withdraw through the
existing protected DUST, proving, persist-before-broadcast, finalized node
submission, cancellation, and reconciliation machinery. Complete standalone
composition now reports `native_settlement` and `settlesOnMidnight: true` for
the public create/deposit/withdraw operations.
ADR-0064 permits only `vc-midnight` to assemble the sensitive Digital Passport
claim material. Re-verify the stored credential/proof/private commitments,
match the issuer DID/method and `persistentHash<JubjubPoint>` against the
authenticated contract anchor, derive expiry/current day from finalized chain
time, reauthorize the exact current managed holder method, and independently
verify the custody-produced holder proof. Never derive a holder scalar from a
public credential field, reuse nonce `17`, accept lock policy/trust from an
incoming caller, or expose the zeroizing composer DTO through headless/mobile.
Keep claim capability absent until the authenticated generated composer and the
ADR-0063 authorization/funding/proving/submission lifecycle consume it.
ADR-0065 permits that consumption only inside the native adapter after the
application has accepted exact `AUTHORIZE_PASSPORT_VAULT_CALL` confirmation.
Prepare may decode and retain only authenticated public issuer/lock policy,
finalized-head time, opaque credential ID, and exact Midnight public context;
it must not read credentials or invoke holder custody/composition. Bind the
claim authorization challenge to the complete public planning fingerprint.
Authorize may then assemble the managed presentation, send the fixed zeroizing
DTO to the one-request generated `claimFromLock` composer, and use ADR-0063's
funding/settlement path. Failures must discard presentation material and reset
the in-progress marker; concurrency and expiry fail closed.
ADR-0066 supplies the discovery gate with a composition-level flow using the
real standalone OpenID4VCI issuer, a holder-bound credential, current managed
Jubjub DID custody, byte-exact contract trust, the packaged generated composer,
Midnight funding, and terminal completion. `native_settlement` may now
advertise all four wallet operations. The deterministic terminal completer is
test evidence only; do not relabel it as a real-node broadcast. Keep real-node
and device resource fixtures explicit backlog items.
ADR-0067 connects the typed contract-state and retained vault-call lifecycle to
Dioxus without WebView/iframe/JavaScript bridges. The page must keep the
standalone ledger, `deterministic_simulation`, and
`native_settlement` visibly distinct; require prepare, exact authorization, and
separate prove/submit stages; permit cancellation only before broadcast; and
route any non-authorized failed submission to reconciliation rather than a
replacement. Native standalone-development builds use
`compose_headless_from_environment`; no configuration selects explicit
simulation, complete reviewed configuration may select native settlement, and
partial invalid configuration fails startup. Only deterministic simulation may
supply a fixed fixture address; native mode requires an explicit deployment
address. The iOS and Android smoke flows cover deterministic state read and the
complete typed lifecycle; terminal copy preserves the adapter's
simulation-only qualifier. Those simulator checks are not real-node or device
resource-baseline evidence. Device real-node fixtures and resource baselines
remain backlog.
ADR-0068 persists only the separate standalone Passport Vault conformance
ledger. Native headless/mobile composition uses a schema-one, 8 MiB-bounded,
owner-private atomic `private/passport-vault.json` beside the profile store or
the normalized absolute `OXID_PASSPORT_VAULT_STORE_PATH`. Domain restore checks
contiguous lock IDs, totals, per-lock accounting, at most 4,096 locks and
16,384 consumed credential fingerprints, exact claim count, and replay-set
references. Corruption, permissive permissions, symlinks, and invalid explicit
paths fail closed; no fallback may erase replay evidence. The file is
`standalone` state only and can never source canonical replay, authorize a
native call, or imply Midnight settlement. In-memory/WASM composition remains
truthfully `process_local`; iOS, Android, and headless process-restart tests
cover durable standalone accounting and claim replay.
The finalized node's timestamp extrinsic is milliseconds and is normalized to
seconds by canonical history collection; the indexer GraphQL `block.timestamp`
is already Unix seconds and must not be divided again.
An authenticated replay cache remains issue #31. Live protocol transport and
production presentation proving remain deferred; ADR-0083 enables only the
explicit native-custody mobile conformance build.
Standalone presentation now reauthorizes the exact statement with the
credential-bound method's current managed protected key, runs the authenticated
k=18 Compact circuit, and independently verifies the public `MZP1` envelope
before permitting an internal `vp_token`. The headless executable enables that
path only when `OXID_PRESENTATION_ARTIFACTS_DIR` names the immutable Nix
artifact closure; without it, consent fails closed at `proof_unavailable`.
Headless views never expose the proof or token.

ADR-0072 adds the first mobile Compact resource gate. The app feature
`standalone-native-proving-artifacts` implies native custody and embeds the
runtime-minimal 135,351,737-byte Nix input (manifest, prover, verifier, compiled
ZKIR, and p18 parameters) directly in the executable. The adapter authenticates
the compiled-in source/toolchain/circuit identity, exact sizes, digests,
circuit, and verifier key without runtime discovery, extraction, cache, or
network IO. Select it only through
`OXID_MOBILE_CUSTODY=native OXID_MOBILE_PRESENTATION_PROVING=artifacts just
ios-run|android-run`. ADR-0083 turns only this explicit feature into a
standalone proof-execution harness: one named worker admits one foreground
proof, holds admission through independent verification, and sets
`compact_presentation_proof_available`. Profile-scoped cancellation,
backgrounding, and the five-minute standalone timeout set a terminal flag but
do not force-stop the generated non-interruptible prover. The future must wait
for the worker to stop, discard every late result, and only then publish
`cancelled`, `backgrounded`, or `timed_out`. Retry requires a fresh OpenID4VP
preview, exact credential selection, consent, holder authorization, and proof.
Normal production, ordinary development mobile, and native-custody mobile
without the artifact feature remain `proof_unavailable`; physical-device
budgets and the remaining ADR-0071 release gate stay open.

The first 2026-08-17 debug package evidence is deliberately non-release: an
iPhone 17e iOS 26.4 simulator produced a 257,526,696-byte uncompressed bundle
versus 173,593,496 bytes without the feature (83,933,200-byte debug delta) and
remained responsive at 455,136 KiB host-reported RSS after startup; the arm64
Android emulator produced a 539,163,753-byte APK versus 404,307,855 bytes
without the feature (134,855,898-byte debug delta) and remained responsive at
310,462 KB PSS / 427,424 KB RSS with no swap. Those first runs did not execute
the presentation prover. Do not promote these virtual-device debug values into
budgets or claims about physical-device latency, thermal behavior, installed
size, or proof memory.

The focused aarch64-darwin release embedded-package test authenticates and
constructs the checked runtime in 3.92 seconds. macOS `/usr/bin/time -l`
reports 5.44 seconds wall, 440,074,240 bytes maximum RSS, 211,911,424 bytes peak
footprint, and no swaps. This is authentication-only host evidence, not mobile
or proof-execution evidence.

The hosted CI job has a 75-minute bound. A cold strict gate plus locked Nix
package/check/artifact build has previously needed roughly 59 minutes; a
60-minute limit canceled an otherwise-progressing check phase when GitHub also
throttled pinned action downloads. Do not reduce the bound without first
shortening or caching the cold build while retaining every gate.

ADR-0017 is accepted. The first M1 security slice separates protection/session
state from key operations, secret blobs, and native user authorization. The
standalone harness has a process-local Ed25519/P-256/Jubjub plus
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
ADR-0090 composes public account refresh plus the independent DUST and shielded
sessions into one Dioxus account-sync card. The iOS and Android standalone
smoke flows use one `Sync now` action and assert the exact `12 DUST`, one owned
shielded note, and `5 NIGHT` fixture results before transfer checks. The action
becomes `Cancel sync` while either worker remains active; cursor and per-run
event counts stay out of normal user copy.
ADR-0091 makes Home **Receive** a non-primary Dioxus route rendered as a modal
sheet over the Home root. It must reuse `GetWalletAccountUseCase`,
`WalletAccountView`, deterministic Rust QR rendering, and the typed
`PublicReceiveAddress` export port; do not add receive-specific application
state or widen native export to arbitrary text. Only an account id beginning
`midnight_account_` with both protected unshielded and shielded rails may expose
returned addresses as holder-controlled receive destinations. Never synthesize
a Fee/DUST selector or admit simulation/watch-only fixtures merely because an
address parses. The selected complete address alone feeds QR, Copy, and Share;
the grouped/truncated preview is display-only. The UI-independent conformance
surface remains `wallet.address.list|unshielded|shielded`, and modal selection
state must not enter the headless protocol.
ADR-0092 makes branding an immutable build input rather than shared UI source
or runtime configuration. Each `brands/<slug>` pack is a closed, bounded,
symlink-resistant metadata/token/SVG input; `oxid-brand-build` validates both
dark/light contrast, rejects active/external SVG content, enforces exact
code-owned manifest purpose templates, and generates only CSS/logo/a typed
`BrandProfile` into the selecting thin app's `OUT_DIR`. Fixed safety colors,
consent/recovery/submission templates, trust, protocols, custody, composition,
and capability labels are never brandable. `show_vault_card` is cosmetic only,
not authorization or binary removal; a licensed removal needs a separately
reviewed thin-app Cargo feature. Runtime or environment-selected brands remain
forbidden. Every real pack directory is auto-enumerated by Nix and the
repository UI gate.
ADR-0093 keeps secret mode inside Dioxus as process-local render state. It
defaults masked, permits one generation-bound 30-second global reveal, and
re-arms after background/resume or successful initialization/unlock. Mark only
reviewed already-public strings with `privacy-value`/`privacy-qr`; never alter
application DTOs, persisted state, diagnostics, or headless responses. Exact
transfer, Vault, issuance, presentation, and SIOPv2 authorization objects must
remain unmasked and state `Details shown for authorization.` New private UI
surfaces must extend the reviewed matrix rather than use broad page-level
masking that could hide consent inputs.
ADR-0094 owns the separate OS snapshot boundary. `ScreenPrivacyPort` carries
only one boolean and closed payload-free failures. Android sets/clears
`FLAG_SECURE`; iOS adds an opaque overlay only while the scene is backgrounded
and must never claim foreground screenshot blocking. Settings and credential
routes force protection for backup-secret and local-reveal surfaces. Native
failure cannot unmask Dioxus or affect wallet authority. Physical-device and
multi-scene evidence remains issue #32.
ADR-0095 makes the capability manifest a UI-neutral, closed public projection.
`capabilities/application` is the only source for both headless
`system.capabilities` and the opt-in Dioxus developer viewer. Select the viewer
only with the compile-time `ui-profile-dev` feature in standalone-development
composition. Never add identifiers, routes, free-form adapter strings,
process statistics, timing samples, logs, or readiness authority. The normal
release must exclude the profile marker and viewer copy through
`scripts/check-ui-profile-release.sh`.
ADR-0096 keeps the separate compile-time `ui-profile-demo` feature inside
standalone-development composition. Its drawer may select or create only the
named `Oxid Demo Wallet` profile, initialize/unlock development custody, derive
account `0/0`, select or create one managed DID, receive the public inbox
fixture, and synchronize only the exact
`simulated`/`undeployed`/`development` fixture. Operations are serialized and
blocked while an identity request is pending. Full setup stops at the existing
credential-offer review; login and presentation remain separate strict-router
actions. Never automate consent, refusal, authorization, proving, submission,
or confirmation. Developer and demo features are mutually exclusive, invalid
outside standalone development, and excluded from normal release artifacts.
The accepted implementation commits are `8ec1b18812541ccceac84c347ba93e0fc2367d5e`
and `e841acc2fed8a6281744f37f79477437e3a9fa42`.
The native controller contract suite injects only the already-decoded chain
tip so pure Nix does not depend on HTTP loopback, then drives the real bounded
GraphQL-WebSocket worker. It proves an owned event projects exactly 12 DUST,
resume subscribes from `cursor + 1`, cancellation retains a 256-event partial
checkpoint, and transport failure publishes only its redacted stable category.
Cold replay receives at most 16,384 events/16 MiB, sends GraphQL `complete` and
drops the socket before any fold/checkpoint callback, retains the 256-event/
4 MiB durable cadence, and reconnects only from the last observer-accepted
sparse cursor. The one-million-event/512 MiB/30-minute bounds span reconnects.
Tests require server-observed completion before the first callback, reject
target regression across a segment, and keep observer failure out of the one-
time incompatible-checkpoint fallback. Production construction still uses the
bounded HTTP chain-tip source. The accepted implementation commit is
`26505c81bde1a7c5e4bc13e559232cf0ebf8d97a`.

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
retry an incompatible cached delta once from zero. The immutable prototype's
shielded v1 explicitly left its inline transport/fold as a pipeline follow-up;
Oxid now uses the same 16,384-event/16 MiB complete/drop-before-fold segmentation,
256-event/4 MiB checkpoint cadence, whole-run caps, and observer-accepted
cursor rule as DUST. Production composition remains fail-closed pending durable
native custody and endpoint discovery. The accepted implementation commit is
`a490dc0f754b9a3f89483c875dc68a77ea7f29d5`.

[Issue #59](https://github.com/MediaNoxLabs/oxid/issues/59) and ADR-0079 extend
that boundary to protected shielded spending. Preparation accepts only a fresh
`Synced` snapshot with equal current/target cursors and no failure, then reopens
the exact owner-private checkpoint for the same profile, network, role-3 key,
source, and cursor scope. Note selection, Zswap inputs/output/change, offer,
witnesses, nullifiers, and serialized transaction remain adapter-private; the
public preview exposes only recipient kind, lowercase token type, exact amount
and change, and input count. Authorization promotes the retained official
Zswap transaction, and submission reuses the protected DUST proving, durable
journal, cancellation, retry, and reconciliation lifecycle. Only one active
protected draft may reserve the process-local note set; an identical request is
idempotent and a competing request fails closed. The public journal may retain
only a domain-separated one-way fingerprint of the synchronized owned-note
state. Broadcasting, unknown, or included records must block every new plan
from that unchanged state until fresh replay advances it; never persist raw
coins or nullifiers. The standalone fixture owns
one 5,000,000-atomic-unit zero-token NIGHT note. Headless conformance spends
1,500,000 with one input and 3,500,000 change; iOS interaction coverage spends
one whole NIGHT because its simulator keyboard drops the decimal separator.
Cached, syncing, cancelled, or stalled shielded state must never fund a draft.
Production still requires native custody and the explicit live stack;
simulation must remain labelled and must never imply live inclusion.
Issue #91 adds the first funded live shielded proof, intentionally beyond the
prototype's unwired shielded helpers. The indexer v4 envelope's exact GraphQL
typename is `ZswapLedgerEvent`; deserialize its tagged payload and accept only
`ZswapInput`/`ZswapOutput` details. Zswap IDs, like DUST IDs, are sparse global
cursors: allow gaps but require strict forward movement, non-regressing
targets, and exact current/target equality before `synced`.
`just standalone-funded-shielded-finality` uses ADR-0098's funding opt-in and
out-of-band development seed. It synchronizes the genesis authority's public
account and native Zswap allocation, spends exactly 1,000,000 atomic units to
a fresh OS-random protected recipient, proves finality, blocks the unchanged-
state fingerprint, reconstructs the adapter from the private checkpoint/public
journal, restores the already-included status idempotently through the
reconciliation use case, and proves exact balances after nullifier replay. It
does not exercise unknown-outcome chain rescanning. This reuses in-process
development custody and is not process/native-custody restart evidence.
ADR-0100 now supplies the distinct typed protected-DUST registration boundary;
fresh-wallet origination remains issue #92 until a funded run proves
registration, later generated-DUST observation/resynchronization, and a next
spend. Fingerprint
lookup must prefer included over unresolved over failed attempts. Capacity may
evict only rejected/expired records; 128 included/unresolved barriers fail
unavailable before broadcast
until issue #93 proves checkpoint-acknowledged compaction.

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
payload. The v2 JSON journal is capped at 128 records/256 KiB, rejects symlinks
and permissive files, and uses owner-only atomic replacement. It reads legacy
v1 records without finalized block heights. Development
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
real Ed25519/P-256/Jubjub key generation and signing to opaque wallet custody
handles.
It supports aliases, verification-method add/rotate/remove, relationship
add/remove, service add/update/remove, signing with explicit confirmation, and
deactivation for `undeployed` DIDs. Every mutation, signing operation, and
deactivation requires bounded human-readable confirmation. Public documents persist, but custody
associations are process-local: after restart, records remain inspectable and
mutation/signing must return `NotManaged`. Normal production composition and
all non-undeployed/live Compact writes remain fail-closed. Standalone DID
creation provisions a managed Jubjub assertion method for holder binding; live
Compact writes and native custody remain later adapter slices, not reasons to
expose private key material. ADR-0048 requires current protected control before
standalone presentation proof execution.

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
the signed body. Normal `compose()` remains unavailable. Live
OID4VCI/OpenID4VP transport, production mobile proving, status/revocation, production
issuer trust, and native release evidence remain later slices. ADR-0045 adds
exact detached Compact issuance-proof verification without treating proof
validity alone as issuer trust or presentation proof generation; ADR-0073 adds
explicit standalone issuer/current-time/trust policy. ADR-0046 adds the exact
development signing primitive, and ADR-0047 binds standalone issuance to the selected
managed Jubjub DID method. ADR-0048 reauthorizes that exact reference against
the current managed protected method before proof execution; ADR-0049 now
constructs and independently checks the distinct credential-family holder
`Proof`; ADR-0050 connects the ZK runtime only in explicit native headless
composition.

[Issue #24](https://github.com/MediaNoxLabs/oxid/issues/24) and ADR-0039 add a
dependency-free protocol domain/application hexagon plus an exact OpenID4VCI
1.0 Final standalone subset. Embedded standalone issuance remains the mobile
and deterministic test journey; ADR-0102 adds one separately authenticated
native desktop/headless Portal HTTP journey using the same pre-authorized-code
grant without Transaction Code. The in-process
adapter strictly separates Credential Issuer and OAuth metadata, uses the Nonce
Endpoint model, builds `proofs.jwt`, parses the final `credentials` array, and
imports through the valid-only ADR-0038 sink. Offer preview and exact
`ACCEPT_CREDENTIAL_ISSUANCE` consent happen before DID key use. Codes, access
tokens, nonces, proofs, signing input, and credential bytes never enter incoming
DTOs. Plain HTTP is loopback-only in this standalone adapter; production
endpoint policy is HTTPS-only and normal `compose()` wires unavailable protocol
ports. Production HTTP/discovery, Authorization Code, by-reference offers,
Transaction Code, batch/deferred issuance, and runtime mobile Portal selection
remain unavailable.
The standalone issuer must independently resolve the selected public DID
method and verify the Ed25519/P-256 proof JWS, nonce, anonymous-flow `iss`
omission, audience, algorithm, and bounded `iat`; structural JWT validation is
not sufficient.
`DidRecordView.managed_method_ids` is current-process capability metadata, not
persisted ownership. Credential issuance must select an active authentication
method from this set; never infer control merely because a resolved or restored
public DID document contains an authentication relationship.

ADR-0101 keeps Portal `804de0a9e58cf48ece3cc6c24b2245bb70bc80f1`
as source-derived negative contract evidence only. ADR-0102 separately pins the
landed Portal `integration` squash `925ec8d04882eabd4ac7b784c70fc2f0c152faae`,
its tree-identical historical PR head `9c82db23eabe8b6d758b2731f2225910ea627c14`,
and profile source `76e8edf394a4cb37ca822037272d543c68f25f71`.
Only native desktop/headless development may select that strict HTTP adapter,
through an absolute manifest path plus exact digest. Keep ADR-0039's Final-only
wire contract, HTTPS-only nonloopback/loopback-only plaintext, explicit consent,
distinct managed authentication and Jubjub methods, exact three-part verified
import, encrypted persistence, unavailable production composition, and
compile-time mobile isolation. Never add a permissive Portal decoder or runtime
production/mobile route switch.

[Issue #25](https://github.com/MediaNoxLabs/oxid/issues/25) and ADR-0040 migrate
the prototype's actual `oid4vp_client` behavior as a separate SIOPv2 draft-13
self-issued-authentication capability. It is not credential presentation: this
slice supports only `response_type=id_token` with `direct_post`; `vp_token`,
DCQL, presentation definitions, and selective disclosure are rejected. The
standalone adapter resolves one exact loopback request-by-reference invocation,
keeps nonce/state/token private, signs through an active current-process managed
DID authentication method, consumes the verifier session once, and independently
resolves and verifies EdDSA/ES256 signature and claims. Headless methods are
`identity.authentication.prepare|accept|refuse|get|list`; Dioxus provides a
verifier/purpose preview and exact checkbox consent. Normal `compose()` remains
unavailable. Sessions deliberately reset on restart; no authentication artifact
is persisted. Live verifier transport, signed request objects, native ingress,
and Final OpenID4VP presentation remain later reviewed slices.

[Issue #26](https://github.com/MediaNoxLabs/oxid/issues/26), ADR-0041, and
ADR-0042 migrate the Digital Passport behavior actually implemented by the
prototype. `CredentialRecord` atomically owns an optional 256 KiB-bounded,
debug-redacted private-material envelope. `adapters/vc-midnight` alone knows its
five-field CBOR mapping and must recompute the official Midnight
`persistentCommit` values and signed domain-separated claim root before
candidates, preview, or local reveal. Credential domain/application own only
schema-neutral privacy tiers, paths, labels, candidate/plan views, and
profile-scoped use cases. Headless exposes
`credential.disclosure.candidates|preview` and never claim values. Dioxus
reveals/hides first and last name only in component-local state and plans a
date-of-birth age threshold; it must state that no verifier presentation or
proof was generated. The deterministic standalone issuer supplies all five
values/openings and, after ADR-0045, the exact public Compact body and detached
issuance-proof fixtures. Normal `compose()` stays unavailable. Do not add a
headless reveal method or describe local preview as
OpenID4VP/selective-disclosure/predicate proof.

[Issue #27](https://github.com/MediaNoxLabs/oxid/issues/27) and ADR-0043 add a
separate credential-presentation hexagon and strict standalone OpenID4VP 1.0
Final-shaped request boundary. `presentation/domain` and
`presentation/application` own only profile-scoped verifier/purpose/requested
claim metadata, credential candidates, exact consent, and terminal session
state. `adapters/openid4vp` owns bounded request-by-reference and exact DCQL
parsing plus the incubating `midnight_compact_vp` format profile. Never call
that custom format generally interoperable. Headless and Dioxus may prepare,
preview, refuse, get, and list without exposing values, openings, nonce/state,
proof bytes, or response tokens. Acceptance requires the literal
`ACCEPT_CREDENTIAL_PRESENTATION` intent and one previewed candidate.

[Issue #28](https://github.com/MediaNoxLabs/oxid/issues/28) is the hard proving
gate. At pinned `midnight-verifiable-credentials`
`39b1354212620b396e914b29603e6a38f2656546`, Digital Passport Compact source
and pure tests exist but generated managed artifacts are not committed. ADR-0044
adds the Oxid-owned final composition and reproducibly builds its real artifact
set with Compact CLI 0.5.1/compiler 0.30.0 from `midnight-did`
`05b237a5e51f9c22853b424e7d4236dfa9384c24`. Run
`nix build .#presentation-compact-artifacts`; generated material stays in the
Nix store. The reviewed Apple-silicon baseline is k=18, 156,301 rows, with an
85,011,711-byte prover key. Treat k, rows, source/toolchain/parameter identity,
and every manifest digest as a coordinated review boundary. The upstream full
pnpm Nix build is not the dependency path: its pinned offline closure currently
lacks `@midnight-ntwrk/midnight-did@0.5.0`.

ADR-0052 separately runs `nix build
.#passport-vault-compact-artifacts` from the byte-identical source distributed
under `contracts/passport-vault` and the same pinned VC/toolchain. ADR-0053
records its private companion provenance at revision
`e4a92a6be2cc6dc34f68261f10c19c9312043807` and requires the exact upstream
SHA-256 `2ebc5b34dd440bc9a9736408f29f5003e7a78f26a564b392be2af36de69102f4`.
All five impure circuits are in the closure: `setTrustedIssuer` k=13/5,416
rows, `createLock` k=11/1,823, `depositToLock` k=10/834, `claimFromLock`
k=17/124,785, and `withdrawFromLock` k=11/1,212. Required circuit parameters
are p10, p11, p13, and p17. Treat these values and every manifest digest as a
coordinated review boundary.

Artifact availability is not presentation readiness. Neither the generic
holder authorization signature nor the exact credential-family Schnorr
`Proof` is a selective-disclosure or age-predicate ZK proof. Keep
`PresentationProofPort` and `PresentationVerifierPort` separate. ADR-0050
permits `vp_token` construction only after checked proof creation and independent
verification bind the exact credential root, presentation root, verifier
challenge/domain, actual disclosure flags, threshold, time input, and ledger
context. Standalone wiring reloads and re-verifies the exact encrypted
credential/proof/opening bundle, constructs the generated circuit's public
statement, round-trips the fixed 524-byte `MPS1` public-input encoding, and
independently reconstructs it. ADR-0048 then reloads the bound DID method,
requires active managed Jubjub assertion authority, signs a domain-separated
authorization over the exact statement through protected custody, independently
verifies and discards that generic DID signature, and then invokes ADR-0049's
atomic Jubjub challenge operation. Wallet custody retains a fresh nonce and
protected scalar, `did-midnight` binds the current managed key, and
`vc-midnight` derives the exact presentation-context challenge. The adapter
constructs, decodes, and independently verifies the reference family's
nine-chunk holder `Proof`. The native headless runtime, or ADR-0083's explicit
mobile conformance worker, then constructs a
generated-runtime-identical `ProofPreimage`, checks it against the authenticated
binary ZKIR, proves with OS entropy and p18 parameters, and independently
verifies the public statement before OpenID validates its private response
container. The bounded checksummed `MZP1` envelope contains public verification
data only. No scalar, nonce, key reference, private value, opening, or serialized
proof preimage crosses that operation. `MPS1` contains selected public values/openings with
canonical zero padding and never contains the private date-of-birth
value/opening. OpenID derives verifier domain as
`SHA-256("oxid:openid4vp:verifier-domain:v1\0" || verifier_domain)` separately
from the nonce challenge. The generated-runtime oracle for challenge `11…11`,
domain `22…22`, current day 20000, first/last reveal, and age-over-18 is:
credential `b42f1115042cefecbd5380a0a630c0ef5f18bb13e7615cb1de9d36256f100432`,
presentation `cf7570efcabe17ba6aa6920aed951f2794a7d609a03a49920694c5c4e09d2876`,
consent `5a442aeb83cd3e589bfc27bd029c5e561ed0aca7109ca4e5642780c2f0bd20a3`,
statement `475caef55fc4b454931beb6b4435688ed36cc1740d33ade45741dcd31214011c`.
Without the explicit artifact root this remains fail-closed preflight and the
session ends at `proof_unavailable`; with the authenticated native headless
runtime or ADR-0083's explicit mobile conformance worker,
`presentationGenerated` and `verifierValidated` become true only after the real
proof succeeds. Normal production and ordinary mobile composition keep the
prover unavailable.
Do not substitute a synthetic boolean, local age calculation, signature, or
fixture bytes for a proof.

[Issue #29](https://github.com/MediaNoxLabs/oxid/issues/29) and ADR-0045 govern
the exact stored `midnight_compact_vc` representation. `CredentialRecord`
atomically separates a 1 MiB-bounded debug-redacted detached proof from the
original MCV1 body and 256 KiB-bounded private opening envelope. The encrypted
credential store writes schema v3; v1 reads as body-only and v2 as body plus
optional private material. `MidnightCredentialVerifier` routes only `MCV1` to
the native Compact verifier and rejects detached-proof confusion for CBOR. The
verifier exactly reconstructs the 18-chunk credential, 9-chunk issuance proof,
Digital Passport claim/body/payload roots, canonical Jubjub points/scalars, and
Schnorr equation, including identity-point and tamper rejection. Its proof-only
default marks structural/proof/schema passed and leaves issuer/current-time/
status/trust `not_checked` for immutable conformance vectors. ADR-0073's active
standalone composition additionally requires exact issuer DID/controller/
assertion-method key binding, current issuance/proof/expiry validity, and the
pinned trust anchor; status alone remains `not_checked` after success.

The public exact fixtures are
`standalone-digital-passport-compact-{body,proof}.b64`. Raw-byte SHA-256 values
are respectively
`4d47be8d1aeeff5e06d9ba1b3ade3bab8e907f0939607cf46e100a9500ad4bcf`
and `fbf2c7e434c70d6f98fa7fae6cd146971db1fda6db96ff2ddea64fe9453e2e02`.
The immutable upstream oracle body root is
`b42f1115042cefecbd5380a0a630c0ef5f18bb13e7615cb1de9d36256f100432`;
the issuance challenge in little-endian form is
`ac211b26c78ad2a361c034be79f11f67434d6f01dd4c26d2add5018b96b44700`.
Standalone OID4VCI uses this exact three-part bundle; standalone inbox retains
phase-1 CBOR as a second-format conformance path. Never copy the prototype's
public claim-root-derived holder scalar into normal or mobile composition.
Development Jubjub signing exists behind opaque references, but selected-DID
method/public-key authorization is now enforced during standalone issuance.
The signed `ExplicitHolderBinding` contains the DID/method reference, not key
bytes. ADR-0048 reauthorizes that method's current protected key before proof
construction: rotation is allowed only while preserving the exact managed
method identifier and assertion relationship; removal, deactivation, locked
custody, or a public-only restored record fail closed. Native custody and
issuer-method anchoring remain issue #29 acceptance gates. Detached issuance
verification, holder authorization, and the exact ADR-0049 holder `Proof` do
not themselves satisfy the ADR-0043/0044 ZK proof gate; ADR-0050's checked
prover and independent verifier do so for explicit native headless mode and
ADR-0083's explicit mobile conformance build, not normal production.

The 2026-08-14 `just ios-smoke` and `just android-smoke` runs pass the exact
Compact OID4VCI bundle through native verification, encrypted schema-v3
persistence, process restart, reverification, local reveal/hide,
disclosure-plan preview, and the fail-closed presentation gate. After ADR-0048,
the same full flows include current managed holder authorization over the exact
`MPS1` statement before the intentional `proof_unavailable` result; headless
coverage additionally rotates the same method, locks custody, removes the
assertion relationship, and restores a public-only DID without ever emitting a
`vp_token`. After ADR-0049, both mobile flows also construct and independently
verify the exact credential-family holder `Proof` before reaching that
headless-only ZK gate. ADR-0050 release tests additionally prove and independently
verify the real circuit, reject public-envelope/request/freshness tampering and
replay, and verify after runtime restart. The strict Nix-shell gate for this
iteration reports 78.68% region, 80.22% function, and 80.36% line coverage;
the real p18 proof tests remain separate ignored release gates so routine
coverage does not load the 135 MiB artifact closure or its prover state.

The 2026-08-17 standalone mobile runs add native-edge lifecycle evidence. On an
iPhone 17 Pro iOS 26.4 simulator, `just ios-smoke` passes all four XCUITests in
228.205 seconds: the complete wallet journey, warm/cold custom-scheme routing,
typed public-address clipboard/share, and QR simulator fail-closed behavior. On
the repository Android arm64 emulator, `just android-smoke` passes the complete
protected wallet/DID/credential/vault flow plus native clipboard/share chooser,
warm/cold app links, encrypted storage shape, and process restore. These results
do not substitute for physical-camera, universal-link, production-discovery,
or device resource evidence.

Preserve the prototype-compatible `Digital Passport` display name for this
schema: the current Dioxus claim controls use that
metadata contract. Replacing the display-name dispatch with an explicit schema
identifier is follow-up work, not a reason to silently rename issued records.

[Issue #13](https://github.com/MediaNoxLabs/oxid/issues/13) tracks the separate
Tier-2 browser build: `cargo check -p oxid-app --no-default-features --features
web --target wasm32-unknown-unknown`. The `getrandom` entropy split is
**resolved** — `.cargo/config.toml` supplies the `getrandom_backend="wasm_js"`
cfg for the browser triple and `apps/oxid/Cargo.toml` declares all three
majors with their JavaScript backends, as recorded in
`docs/dependencies/wasm-web-entropy.md`. Measured 2026-08-21 in the pinned
devshell, the check now reaches `blst`'s C build and fails there on the Nix
compiler wrapper rather than on any Oxid dependency:

```
Warning: supplying the --target wasm32-unknown-unknown != <host> argument to a
nix-wrapped compiler may not work correctly - cc-wrapper is currently not
designed with multi-target compilers in mind.
clang: error: unsupported option '-fzero-call-used-regs=used-gpr' for target
'wasm32-unknown-unknown'
```

The devshell's clang does support wasm targets; the wrapper injects host
hardening flags that clang rejects for that target, and the wrapper says so
itself. So the next step is a target-scoped compiler override, not a
dependency change. Keep that repair target-scoped; it must not add
browser-only dependencies to the green Tier-1 Android and iOS graphs, and it
must not relax hardening flags for the host targets to satisfy the browser
one.

## Prototype provenance

The prototype remains useful migration input, not an architecture template.
The reviewed baseline is:

- repository: `midnight-ledger`;
- branch: `feat/mobile-prototype`;
- commit: `074b1a4bccbfee1740ee188374b606a022ecef42` (2026-07-02);
- source area: `mobile-bench/`, especially `wallet-core/`,
  `dioxus-wallet/`, and `headless-wallet/`.

The remote `feat/mobile-prototype` ref was re-verified on 2026-08-19 and still
resolved to that exact commit. The separate remote `mobile-prototype` ref
resolved to `255f2caf8c728c203f554d6bc853d1f3b7e8bc15`; do not treat its older name
as a successor without a fresh provenance review.

The 2026-08-20 stopping-point audit compares that immutable baseline with Oxid
repository, test, mobile-host, and live-environment evidence in
`docs/migration/delivery-audit-2026-08-20.md`. Current evidence is roughly
98% of useful prototype behavior, or 105/110 (95%) of the deliberately harder
migration target, while production-release evidence is about 78%.
These are evidence classifications, not source-line counts. The dependency-
ordered gaps are physical custody/recovery evidence, a provisioned production
deployment, funded DUST registration-to-recovery/fresh-wallet shielded
origination, physical identity ingress plus verified HTTPS association,
production
background synchronization, live identity trust/transport and DID writes,
Passport Vault live/device evidence, and device resource budgets. The next
bounded engineering slice is one funded PreProd registration-to-recovery and
fresh-wallet shielded spend (#92); checkpoint-safe journal compaction is #93.
Approved domains/association files,
release signing identities, physical devices, and funded live infrastructure
are external evidence inputs and have no repository-only ETA.

That commit declares itself the successor to the earlier Dioxus/VC prototype
branches. Record a new immutable commit here before taking later prototype
changes. Do not copy ledger-relative path dependencies, demo secrets, generated
proof artifacts, pre-production keys, vendored JS, or environment-specific
mobile projects into Oxid without an explicit migration decision.
The prototype Passport Vault claim code in `web/src/entry.ts` derives its holder
scalar from the public credential claim root and uses the fixed presentation
nonce `17`. Never migrate either shortcut: use opaque managed holder custody and
fresh wallet-generated randomness, with no scalar, nonce, or private witness in
incoming adapters.

ADR-0058 authenticates the Passport Vault generated client and its four wallet
proof circuits at runtime. `NativePassportVaultCompactArtifacts` accepts only
an absolute canonical non-symlink root, streams exact size/SHA-256 checks, and
implements Midnight resolver/parameter traits for `createLock`,
`depositToLock`, `claimFromLock`, and `withdrawFromLock`. Never add
`setTrustedIssuer` or degree 13 to this wallet resolver. The generated module
is loaded by ADR-0059's composer only from its Nix-fixed artifact closure; the
wrapper clears Node loader overrides and accepts no artifact route or raw
circuit argument surface. Its serialized transaction output remains adapter-
owned and must never enter headless/mobile views.
ADR-0060's retained adapter additionally requires non-empty serialized Zswap
state and ledger parameters; never replace them with the composer's conformance
defaults in live preparation. Expiry/drop erases retained transaction bytes.
ADR-0061 sources those bytes only from the node-anchored indexer query associated
with the exact canonical replay state/action. The Midnight adapter is the sole
Bech32m address decoder; it requires the selected profile's exact network HRP,
one 32-byte unshielded payload, and one 64-byte shielded public payload. The
composition root is the only place those two outgoing sources are joined.

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
ADR-0040 adds a distinct self-issued-authentication aggregate and a pinned
SIOPv2 draft-13 standalone adapter without claiming OpenID4VP presentation.
ADR-0041 adds optional 256 KiB-bounded, debug-redacted format-private material
to `CredentialRecord`, carries it through verified issuance/import, and stores
it atomically inside credential-store schema v2. The store reads schema v1 as
material-absent. Ordinary credential/headless/UI views still expose no private
bytes or claims. ADR-0042 adds schema-neutral disclosure inventory/planning and
an exact Digital Passport adapter. It recomputes all five official Midnight
commitments and the signed root before any interpretation, keeps value reveal
local to Dioxus, and returns claim-free headless plans with
`presentationGenerated: false`. Never attach synthetic claims that are not
bound to the signed credential fixture.
ADR-0043 adds strict Final-shaped OpenID4VP/DCQL request preview, candidate
matching, exact consent, refusal, and replay protection, while preserving a
hard proof/independent-verifier gate before `vp_token`.
ADR-0044 adds the immutable final Compact composition, proving/verifying key
derivation, and artifact digest manifest without opening that runtime gate.
ADR-0045 adds explicit `midnight_compact_vc` body/proof/private-material
separation, native detached issuance-proof verification, encrypted schema-v3
migration, and exact headless restart conformance without opening the
presentation gate or treating self-contained proof validity as issuer trust.
ADR-0046 adds exact 0.5.0-compatible Jubjub generation/signing to the
development custody adapter. It exposes only a compressed public point, opaque
key reference, and 96-byte signature.
ADR-0047 adds a managed Jubjub assertion method to standalone DID creation,
keeps OpenID4VCI authentication and credential holder methods distinct, and
canonically reissues the exact Compact bundle to that selected holder. A public
restored DID record is not proof of ownership. ADR-0048 adds presentation-time
reauthorization, explicit same-method rotation semantics, typed locked/unmanaged
failures, and independent verification of the disposable custody attestation;
native wrapping remains an issue #29 gate.
ADR-0049 adds the reference family's distinct two-step presentation Schnorr
operation. Wallet custody generates and retains the fresh nonce, exposes only
canonical public points to a synchronous challenge callback, and returns only
the response. DID management resolves the opaque key reference, while the VC
adapter alone owns, encodes, and independently verifies the exact proof
transcript. This is an adapter-to-adapter capability, not an incoming arbitrary
challenge-signing oracle, and it still leaves ZK proving/verification closed.
ADR-0050 adds exact native headless Compact proving and an independently
reconstructed verifier behind authenticated Nix artifacts. ADR-0051 isolates
Passport Vault policy and accounting in a product-specific hexagon. Its
standalone repository began as bounded process-local state and is now
owner-private and restart-durable on supported native targets under ADR-0068;
its credential adapter rechecks the exact Compact Digital Passport and pinned
development trust anchor, and its incoming surfaces never label local state
movement as a chain submission. ADR-0052 authenticates the exact Passport
Vault/VC/toolchain inputs,
composes all five contract circuits, and decodes the 15-field version-1 tagged
ledger natively with bounded integrity checks. Valid decoding alone is not
proof of address authenticity, finality, or freshness. Issue #31 owns the
remaining authenticated acquisition and live contract-call adapter.
ADR-0053 supersedes only the private upstream flake-input choice: the reviewed
contract source is distributed byte-identically from Oxid and hash-checked so
public CI and forks remain secret-free.
ADR-0054 anchors indexer reads to finalized node hashes without authenticating
their state bytes. ADR-0055 selects deterministic canonical replay and owns the
pure verifier plus its finalized-node collector. The collector scans every
finalized block from a node-validated deployment and binds direct raw
`send_mn_transaction` payloads to the
pallet outcome and canonical typed action-event batches: calls first,
deployments second, maintenance third, with transaction order preserved inside
each batch. Indexer history or failed-segment data is not a completeness or
outcome authority. This order is pinned to `midnight-node` commit
`06858f9a7fe40866c2c074ff07eecc39d7d35ef7`.
ADR-0056 exposes only `create_lock`, `deposit_to_lock`, `claim_from_lock`, and
`withdraw_from_lock` through a retained application port. Preparation requires
`canonical_finalized_replay`; authorization and submission are separate exact
intents. `setTrustedIssuer` remains deployment/administration-only. Incoming
commands never carry private credential data, witnesses, signatures, proofs,
or serialized transactions. ADR-0058 supplies the runtime-authenticated
generated client plus a four-circuit native proof resolver. It does not supply
the combined contract/DUST provider, submission, or reconciliation. ADR-0059
supplies a separate closed-schema composition oracle
for create/deposit/withdraw. ADR-0060 connects that oracle to a retained native
port adapter through a bounded public context source, but rejects claim and
leaves submit closed. ADR-0061 composes the public context for the complete
standalone stack and labels the resulting prepare/authorize-only capability
`native_composed_draft`. ADR-0062 makes authorization consume that retained
call through a composition-only protected Midnight funding port. It derives the
exact native NIGHT deficit from ledger balance semantics, selects synchronized
bounded UTXOs, creates change, signs every input, and relabels the retained-only
capability `native_funded_draft`. ADR-0063 adds the DUST proof, broadcast,
public journal, and finalized outcome authority for native
create/deposit/withdraw while keeping claim closed.
ADR-0069 keeps QR capture behind a platform port and identity-link
classification in a strict protocol adapter. Native ingress may navigate only
to existing preview/consent flows; it cannot execute them. Unknown
`openid4vp` endpoint pairs remain ambiguous and fail closed.
ADR-0070 adds warm/cold iOS and Android custom-scheme capture through that same
router and permits native clipboard/share only for the bounded
`PublicReceiveAddress` type. Keep one pending identity request until explicit
dismissal; a new event must never replace active holder review.
ADR-0073 keeps historical Compact conformance proof-only but requires active
standalone wallet composition to receive an explicit issuer resolver, clock,
and pinned trust anchor. The exact issuer-controlled assertion method must be
EC/Jubjub and equal the detached proof key; current-time rules must pass; status
must remain `not_checked`. Never compose the standalone trust anchor into normal
production.
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
| `crates/wallet/application` | Incoming use cases and owned outgoing repository/protected-operation ports, including atomic public-transcript Jubjub challenge signing without credential semantics. |
| `crates/identity/domain` | Dependency-free Midnight DID, public JWK, document, and resolution invariants. |
| `crates/identity/application` | Profile-scoped DID resolution, inventory, lifecycle/signing use cases, managed-method challenge-signing binding, and owned outgoing ports. |
| `crates/credential/domain` | Dependency-free credential records, explicit formats, separately bounded/redacted original bytes, detached proofs, opaque format-private material, schema-neutral disclosure candidates, and structured verification invariants. |
| `crates/credential/application` | Profile-scoped receive/list/get/reverify/delete plus holder-bound issuance, exact bundle import, disclosure inventory/plan/local-reveal use cases, and repository/inbox/verifier/disclosure ports. |
| `crates/protocol/domain` | Dependency-free credential-offer and self-issued-authentication preview/lifecycle invariants. |
| `crates/protocol/application` | Profile-scoped issuance, explicit public holder-binding, and self-issued-authentication use cases plus protocol/proof/verified-sink ports. |
| `crates/presentation/domain` | Dependency-free credential-presentation preview, claim-intent, candidate, and lifecycle invariants. |
| `crates/presentation/application` | Profile-scoped presentation use cases plus protocol, candidate, current-holder authorization, proof, proof-control/lifecycle, and independent-verifier ports. |
| `crates/passport-vault/domain` | Dependency-free product lock policy, creator authorization, checked accounting, and per-lock credential replay invariants. |
| `crates/passport-vault/application` | Passport Vault list/create/deposit/claim/withdraw use cases plus focused repository, credential-policy, bounded contract-state source, and retained four-operation contract-call ports. |
| `crates/platform/ports` | Clock, randomness, and bounded native QR-scanner capabilities used by applications. |
| `crates/adapters/storage-memory` | Development/test implementations of wallet, DID, and credential persistence ports. |
| `crates/adapters/storage-json` | Versioned persistence for public profile metadata and active selection only. |
| `crates/adapters/storage-identity-json` | Strict versioned persistence for validated profile-scoped public DID documents only. |
| `crates/adapters/storage-credential-json` | Development-only authenticated encryption for bounded profile-scoped credential records, original signed bytes, detached proofs, and opaque format-private material. |
| `crates/adapters/storage-dev` | Process-local, development-only Ed25519/P-256/Jubjub generation plus protected BIP32/secp256k1-Schnorr derivation, one-shot signing, and atomic fresh-nonce Jubjub challenge completion. |
| `crates/adapters/midnight` | Midnight network/account and native canonical-transaction adapter with fail-closed production, simulation/live sources, protected public-account binding, retained transfer and protected-DUST registration drafts, version-two public eligibility checkpoints, standalone DUST/Zswap proving/submission completion, and domain-separated bounded public submission recovery. |
| `crates/adapters/did-midnight` | Standalone fixture and exact public Compact-issuer documents, explicit bounded native Midnight DID resolution, plus development Ed25519/P-256/Jubjub lifecycle and managed-method challenge-signing adapters. |
| `crates/adapters/vc-midnight` | Strict Midnight phase-1 CBOR verification, exact native Compact body/detached-issuance-proof verification, explicit standalone issuer/current-time/trust policy, holder-bound reissuance, commitment-bound Digital Passport private-part interpretation, generated-Compact presentation public-input conformance/proving/verification, current managed Jubjub holder reauthorization, a single-proof foreground mobile worker, and public standalone fixtures. |
| `crates/adapters/passport-vault` | Product-specific bounded in-memory plus owner-private atomic standalone repositories, exact standalone Digital Passport policy bridge, native pinned-layout decoder, node-anchored unproven indexer read, pure canonical replay verifier, history-complete finalized-node collector, opt-in authenticated replay source, exact four-circuit generated-client/proof artifact resolver, generated-composer/Rust-codec conformance, and zeroizing authorization-bound settlement for create/deposit/claim/withdraw; managed-custody claim conformance is exercised through composition. |
| `crates/adapters/openid4vci` | Strict OpenID4VCI 1.0 Final pre-authorized flow, separate authentication/holder-binding validation, in-process standalone issuer, native-headless pinned Portal HTTP client, DID proof bridge, and verified credential sink. |
| `crates/adapters/siopv2` | Strict SIOPv2 draft-13 standalone request-by-reference login, opaque DID proof bridge, and independent single-use verifier. |
| `crates/adapters/openid4vp` | Strict OpenID4VP 1.0 Final-shaped standalone DCQL request, candidate/consent session, fail-closed Compact proof gate, independent verification, and proof-worker completion control. |
| `crates/adapters/identity-ingress` | Strict credential-offer/registered-OpenID4VP classifier plus payload-redacted native iOS/Android QR scanner adapters. |
| `crates/adapters/mobile-native-plugin` | Single repository-owned Manganis Rust/Swift/Kotlin bridge for QR capture, Android OS-link queueing, and typed public receive-address clipboard/share operations. |
| `contracts/presentation` | Oxid-owned final Compact presentation compositions; generated artifacts remain Nix-store outputs and never enter Git. |
| `contracts/passport-vault` | Byte-identical Apache-2.0 Passport Vault Compact source distributed for secret-free public builds; its pinned private-upstream provenance and digest are ADR-0053 review boundaries. |
| `nix/packages/passport-vault-compact-artifacts.nix` | Immutable Passport Vault client/IR/key/parameter closure from the hash-checked distributed contract plus pinned VC and Compact toolchain revisions. |
| `nix/packages/passport-vault-call-composer.nix` | One-request Node 24 outgoing adapter package with locked Midnight compatibility dependencies, Nix-fixed authenticated artifacts, closed typed operations, and real generated-client install checks. |
| `tools/passport-vault-composer` | Internal generated-Compact composition implementation; never an incoming headless/mobile API and never a credential/private-witness bridge. |
| `crates/brand-build` | Build-only closed-schema brand-pack validator, two-scheme contrast checker, safe-SVG/manifest gate, and `OUT_DIR` Rust/CSS/logo generator; never a runtime configuration or wallet capability crate. |
| `brands` | Reviewed immutable presentation inputs only. Each real directory is one validated pack; no secrets, endpoints, trust, protocol, custody, confirmation, or application state. |
| `crates/adapters/platform-system` | System clock, OS randomness, and typed public receive-address export implementations. |
| `crates/ui-dioxus` | Brand-agnostic Dioxus incoming adapter, immutable `BrandProfile` presentation context, bounded mobile route stack, safe read-only Home projection, exact amount/consent presentation state, public receive-QR rendering, distinct protected-DUST registration review/authorization/submission/cancellation/reconciliation, standalone Passport Vault UI, and truthfully labelled typed native vault-call lifecycle. |
| `crates/composition` | Concrete dependency wiring with no product rules, including the authenticated native-headless-only Portal bridge that remains absent from production/mobile composition. |
| `apps/oxid` | Default-brand thin executable shell, literal `brands/oxid` build selection, and platform launch point. |
| `apps/oxid-headless` | Standalone NDJSON incoming adapter and flow harness. |

Every static `class: "..."` token in the Dioxus adapter must have a selector
in `crates/ui-dioxus/assets/styles.css`; `scripts/check-ui-css-classes.sh`
enforces that contract from the UI/repository gate. ADR-0084 also requires the
complete dark/light brand schema and fixed semantic component vocabulary;
`scripts/check-ui-design-tokens.sh` rejects raw component colors, legacy
palette aliases, and ad-hoc type sizes, radii, or motion durations. Safety
colors are not brandable and dark is still the only selected scheme.
`scripts/check-brand-packs.sh` validates the complete pack root before UI
compilation; Nix exposes `packages.brand-check`, `packages.oxid-app-oxid`, the
root `checks.brand-packs`, and one `checks.brand-<slug>` per real directory.
Keep responsive dimensions and safe-area geometry explicit, but put component
spacing on `--space-1..8`. The Passport Vault compatibility classes remain
mapped to the shared card/action/form rules; do not add a third vocabulary.
ADR-0085 makes `crates/ui-dioxus/src/labels.rs` the user-facing machine-value
boundary. States, modes, sources, formats, authentication labels, protocol and
verification reasons, network names, and disclosure vocabulary must never be
interpolated or underscore-normalized directly in rsx. Unknown values use
neutral unavailable copy and are never echoed. Use exact six-decimal NIGHT,
fifteen-decimal DUST, and readable UTC timestamp helpers; adapter cursors may
drive progress but are not user copy. `scripts/check-ui-copy-labels.sh` enforces
the boundary from `run.sh`; update both the mapping tests and required
vocabulary when a reviewed public view value is added.

ADR-0086 owns mobile navigation entirely inside the Dioxus incoming adapter.
The primary order is Home, Wallet, center Scan, Documents, Activity; Scan is an
action, not a route. The stack always has one primary root, selecting a primary
clears secondary routes, each secondary route appears at most once, and Back
pops presentation state only. Dismissing identity ingress also pops its review
route without consent. Passport Vault
opens from Home, DID management from Documents, profiles/Settings from the
avatar sheet, and Diagnostics from Settings. Credential and self-issued app
links reset the root to Documents and push the corresponding review without
consent. Phase 1a intentionally renders the complete account view on both Home
and Wallet only in commits before ADR-0087. Phase 1b removes that temporary
overlap: Wallet alone retains network selection, activation, sync, receive,
send, and submission recovery. Home is a Dioxus-only read projection over the
existing account/security, shielded-sync, credential-list, and Passport Vault
use cases. Its optional product reads fail independently with payload-free
unavailable copy; it never initializes, unlocks, derives, syncs, imports,
authorizes, proves, submits, reconciles, or changes an application state. Home
Receive/Send route to Wallet, Present routes to Documents, Scan uses the exact
shared scanner/classifier starter, Vault opens its existing secondary route,
the security strip opens Settings, and See all routes to Activity. Do not show
claims, DIDs, addresses, credential/transaction identifiers, cursors, block
heights, or epochs on Home. Backup support may be labelled available but never
completed, and user-presence requirements must not be called biometric
enrollment. The current recent preview is the existing transaction projection
only; identity/Vault activity requires a later application interaction-log
contract. Do not add a router dependency or persist routes without a new
concrete URL/history requirement and review. The shared Back control does not
claim Android system-back interception. Secret-mode UI remains absent until its
masking and native privacy policy is implemented.

ADR-0088 owns the Phase 2a Send presentation. Dioxus keeps exactly two
editable steps (recipient, then amount/privacy) over the unchanged nine-state
`TransferPanelState`; review and all confirmation copy must be derived from the
retained application preview. Device authorization and prove/submit remain two
separate explicit intents. Sending retains acknowledged pre-broadcast cancel;
failure exposes only Edit, safe retained-draft retry, or durable network
reconciliation as selected by `TransferRecovery`. When the development
self-address affordance is active, changing Public/Shielded updates it to the
matching address; manually entered recipients are never rewritten. Clipboard
import, payment-address scanning, and recent recipients stay absent until
focused ports are reviewed. iOS XCUITest must blur the decimal keyboard before
tapping the lower review control and scroll confirmation/retry submit controls
above the fixed navigation; Android CDP must wait for Home composition before
using a quick action. Both standalone mobile smokes traverse exact review,
authorization, cancellation-safe retry, and confirmed inclusion.

ADR-0089 owns the Phase 2b identity consent presentation. OpenID4VP,
OpenID4VCI, and SIOPv2 must render their existing public application plans as
ordered WHO, WHAT, FROM, and WHY questions without merging protocol semantics
or moving authority into Dioxus. Until a production trust-result port exists,
standalone verifier and issuer endpoints are labelled `Unverified endpoint`;
fixture equality is routing, not trust. Until an optional-claim authorization
port binds holder selection into the retained request and proof inputs, every
prepared presentation claim is shown checked, disabled, and required. The
`age_over` predicate must say that it confirms the threshold and does not share
date of birth. Preserve the literal confirmation checkboxes, exact acceptance
intents, explicit presentation credential chooser, managed DID selection,
one-tap refusal, replay controls, and fail-closed Compact proof result. SIOPv2
proves DID control without a credential; issuance receives a document and must
not invent an issuer purpose absent from `CredentialIssuanceView`.
`ProfileFlowTests.testIdentityConsentCeremoniesInStandaloneMode` is the focused
fresh-install iOS gate for this surface: it activates custody, creates a managed
DID when absent, exercises login and issuance, verifies the locked age
predicate, and reaches the fail-closed presentation result without entering the
numeric-keyboard Send path. The complete wallet-flow test retains the same
ceremony assertions and the Android CDP smoke retains the multi-credential
chooser. On iOS 26, a numeric-keyboard interaction can leave XCTest waiting one
minute for WebView idle after every later tap; an interrupted run in that state
is not a functional pass or failure, so use the focused test for diagnosis and
rerun the complete suite on a fresh simulator process for release evidence.

`oxid-composition` exposes UI-neutral `ApplicationServices`. Incoming adapters
adapt that object at their own boundary; composition must not depend on Dioxus,
the headless protocol, or another incoming adapter. The headless protocol is
`oxid.headless.v1`. Its stdout is protocol-only, invalid input must not poison
the stream, and capability discovery must label unimplemented methods as
`queued`; partially available methods must truthfully use `blocked` with their
gate. Never reproduce the prototype's `controllerSkHex` bootstrap result or
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
`OXID_MIDNIGHT_UNSHIELDED_ADDRESS`. Optional authenticated Passport Vault reads
also require the non-zero untrusted hint
`OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT`; it is rejected outside the complete
standalone stack. With canonical replay enabled,
`OXID_PASSPORT_VAULT_COMPOSER` optionally installs the packaged retained native
composer plus protected NIGHT funding and settlement bridge; present reports
`native_settlement`, missing keeps `native_pending`, while an invalid
configured path fails startup. `compose_in_memory()` uses the development
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
`credential.reverify`, confirmation-gated `credential.delete`,
`credential.disclosure.candidates`, and `credential.disclosure.preview`;
`credential.request` and `credential.verify` remain prototype aliases. These
methods derive profile scope from the active profile and never return signed
bytes, proofs, openings, or claim values. Disclosure output is limited to
schema, labels, paths, privacy tiers, selections, threshold, outcome, and the
fact that no presentation was generated. There is deliberately no headless
local-reveal operation.

The headless DUST surface is `wallet.dust.sync.status`,
`wallet.dust.sync.start`, and `wallet.dust.sync.cancel`. These commands never
accept a key, path, seed, endpoint, or checkpoint. The deterministic simulator
advances on status polls so tests can cover fresh, cancelled, resumed, and
already-current flows without timing races. Native start returns before network
or ledger work begins; incoming adapters must poll status and may cancel.

Protected registration is a separate headless lifecycle:
`wallet.dust.registration.prepare`, `authorize`, `submit`,
`start_submission`, `draft`, `status`, `cancel_submission`, and
`reconcile_submission`. Preparation exposes only the aggregate NIGHT amount,
eligible-input count, maximum generated-DUST fee allowance, expiry, and opaque
identifiers. Authorization and submission require separate exact confirmations.
No command accepts or returns UTXOs, paths, seeds, DUST keys, signatures,
witnesses, proofs, or transaction bytes. An included registration reports
`requires_synchronization`; only the official DUST event stream can establish
later spend readiness.

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

Passport Vault standalone composition follows the same exception. Its headless
methods are `vault.total_locked`, `vault.locks.list`,
`vault.credentials.list`, `vault.lock.create`, `vault.deposit`, `vault.claim`,
and `vault.withdraw`. State-changing calls require their exact declared intent.
Native headless/mobile state uses an owner-private atomic file and survives
restart; in-memory/WASM state remains `process_local`. All views retain the
`standalone` source label and capability discovery reports the independent
persistence mode. Production composition wires unavailable vault ports. Never
add a hard-coded contract address, JavaScript bridge, iframe, ambient
companion-repository lookup, or generated artifact to make it appear live;
issue #31 is the reviewed live boundary.

The staged chain-call harness is the `vault.contract_call.*` family:
`prepare`, `authorize`, `draft`, `submit`, `start_submission`,
`submission_status`, `submission_history`, `cancel_submission`, and
`reconcile_submission`. It supports exactly create, deposit, claim, and
withdraw. Preparation is active-profile-scoped and requires authenticated
canonical replay state in live composition. The development-only simulator
instead requires the distinct `deterministic_simulation` state class; neither
call-service constructor may admit the other's class. Claim input contains only an opaque credential ID; no
incoming method accepts credential bytes, openings, holder keys, witness data,
proofs, signatures, or serialized transactions. The native retained adapter is
composed with fresh replay-matched public Midnight context and protected NIGHT
funding in the complete standalone stack. When the packaged composer is
present, ADR-0063 reports `native_settlement` and submits create/deposit/withdraw
through protected DUST proving plus finalized reconciliation; otherwise
explicit live discovery uses `native_pending`. Native claim remains unavailable.
Zero-configuration headless/development
composition uses the fixed simulator address published by `system.capabilities`;
its mode is `deterministic_simulation`, result mode is
`deterministic_simulation_only`, history is process-local, and
`settlesOnMidnight` is always false.

The read-only native Passport Vault state boundary is the exception recorded by
ADR-0052. `vault.contract_state.decode` accepts only bounded tagged
`ContractState` hex and labels its result `pinned_contract_layout`; it means
only that the schema is authenticated and does not imply where the bytes came
from. The exact ledger has 15 fields and
contract version 1. Cap decoding at 16 MiB and 4,096 contiguous locks, require
per-lock/global accounting agreement and `claimCount == consumedClaims.size`,
and reject trailing bytes or unknown decisions. The deterministic fixture at
`fixtures/passport-vault/contract-state-v1.hex` is 2,013 bytes with SHA-256
`dc4a2f242b8a0a525310b1090ca1ad117cc0d7b019e16d8738f3c9505760a8c0`.
Do not relabel it live/cached without an authenticated acquisition/freshness
adapter.

The native replay verifier in `crates/adapters/passport-vault/src/replay.rs`
accepts only complete canonical observations supplied in block/extrinsic order.
It strictly decodes the official tagged proven transaction, matches its inner
hash and ordered applied operations to node events, replays every guaranteed
target transcript plus only uniquely identified fallible target actions, and
requires exact proven effects. Target maintenance, repeated-address outcome
ambiguity, missing global commitment indices, or any transcript mismatch fail
closed. It deliberately has no transport. The collector in
`crates/adapters/passport-vault/src/finalized_history.rs` derives `BlockContext`
from node timestamp, parent hash, prior-block timestamp, and the consensus
30-second uncertainty while scanning every finalized block from the validated
deployment. It reads metadata at the parent state so runtime upgrades decode
with their historical schema, accepts only direct Midnight transaction calls,
and fails closed for wrapper events whose raw payload cannot be authenticated.
The deployment height is a hint, never authority: exactly one target deployment
must appear there. `authenticated_state.rs` composes this collector and replay,
admits only one in-flight scan, and exposes both the latest target transaction
and the captured finalized head through application-owned provenance variants.
Only that source may use the authenticated replay label.

The Passport Vault upstream companion is private. Never add it back as a flake
input or require CI/forks to hold a repository token. ADR-0053 permits only the
byte-identical Apache-2.0 source at
`contracts/passport-vault/passport-vault.compact`; its 23,776 bytes and SHA-256
`2ebc5b34dd440bc9a9736408f29f5003e7a78f26a564b392be2af36de69102f4`
are coordinated source/layout/artifact review boundaries. Generated clients,
IR, parameters, and proving keys remain Nix outputs.
Runtime consumers must additionally pass those outputs through
`NativePassportVaultCompactArtifacts`; a manifest or Nix-looking path alone is
not authentication. Keep `OXID_PASSPORT_VAULT_ARTIFACTS_DIR` as an explicit
closure route and never treat its presence as permission to enable live calls.

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
`wallet.transaction.send_unshielded` alias. ADR-0079 adds the parallel
`wallet.transaction.prepare_shielded`,
`wallet.transaction.authorize_shielded`,
`wallet.transaction.submit_shielded`, and
`wallet.transaction.send_shielded` surface. The shielded request accepts only
the active profile's canonical shielded address, a lowercase 32-byte token
type, and exact decimal-string atomic units; no checkpoint, coin, opening,
witness, nullifier, offer, or transaction material crosses the protocol.
Controllable attempts add
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
./bootstrap.sh
```

The tracked bootstrap wrapper delegates directly to the flake: no arguments
enter the shell, `--pi` starts Pi, `--check` runs the deterministic Pi smoke
gate, and `-- <command>` runs one command inside the shell. It never reads,
prints, or persists credentials. Direct `nix develop` remains supported.

The Linux Dioxus desktop graph links `libxdo`; keep `pkgs.xdotool` in both the
Linux development-shell libraries and package build inputs. macOS validation
cannot detect this linker requirement, so the hosted Linux gate is the
cross-platform evidence for it.

Direnv users can run `direnv allow`. The shell provides Rust, Cargo tooling,
`dx`, `just`, Node.js, and the pinned project-local Pi packages from
`.pi/settings.json`. It also exports `OXID_PRESENTATION_ARTIFACTS_DIR` to the
self-contained `presentation-compact-artifacts` Nix closure and
`OXID_PASSPORT_VAULT_ARTIFACTS_DIR` to the authenticated vault closure. The
native headless composition authenticates its manifest, prover/verifier keys, binary
ZKIR, and p18 parameters before enabling presentation proof generation; do not
replace that path with a mutable cache or runtime download.

The Pi review integration is pinned as
`@input-output-hk/agent-review-pi@0.5.0`. That package declares both its review
extension and bundled review skill in Pi metadata; the shell installs them
together into the ignored project-local `.pi/npm` tree. Installation requires
an existing GitHub token with package-read access. Never write that token into
repository configuration or diagnostics. Pi `0.84.0` cannot parse the pinned
package's bundled skill because its YAML description contains an unquoted
colon. The tracked `.pi/skills/agent-review/SKILL.md` compatibility loader
checks version `0.5.0` and delegates to the complete package workflow without
copying it. `./bootstrap.sh --check` (equivalent to
`nix develop --command just pi-smoke`) deterministically verifies
the package metadata, all registered native review tools, and runtime skill
discovery without an LLM call or GitHub mutation. Remove the loader only after
a reviewed package update passes that same runtime inventory directly.

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
nix develop --command just ios-smoke
nix develop .#docs --command ./scripts/build-docs-site.sh
nix build --print-build-logs
nix develop --command cargo test --release -p oxid-adapter-vc-midnight \
  native_runtime_proves_restarts_and_rejects_public_tampering -- --ignored
nix develop --command cargo test --release -p oxid-headless \
  proves_and_independently_verifies_a_compact_presentation_end_to_end -- --ignored
```

The first aarch64-darwin release baseline completed the native tamper/restart
test in 22.60 seconds and the complete headless flow in 18.37 seconds. macOS
`time -l` reported roughly 5.07 GB maximum resident set size for both runs, so
ADR-0050 deliberately keeps the native prover out of mobile composition until
its packaging and memory strategy are separately reviewed.

On macOS with Xcode and Rustup installed, `just ios-run` uses the Dioxus CLI
from the locked flake and the host Apple/Rust toolchain to build, install, and
launch the mobile feature. The Nix shell's non-Apple `xcrun` compatibility tool
must not be used for simulator discovery. The launcher replaces Nix's
`DEVELOPER_DIR` with the selected Xcode installation and explicitly removes
`SDKROOT` for the Dioxus build. SwiftPM must compile its host-side package
manifest before Xcode selects the simulator SDK; exporting the simulator
`SDKROOT` globally makes that manifest fail to load. Preserve the host-tool and
simulator split so `nix develop --command just ios-smoke` remains valid. The
XCUITest invocation also uses a minimal host environment so Nix compiler/linker
variables cannot leak into Apple's build system.
`OXID_IOS_DEVICE=<UDID>` selects a specific simulator. The first verified smoke
test used an arm64 iPhone simulator. The prototype-derived shell and
first-launch profile gateway were subsequently built, launched, and visually
verified through the same command.
`just android-run` performs the equivalent Dioxus build, install, and launch
using an Android SDK/NDK plus a connected device or local AVD. Generated
Gradle/Xcode output remains under ignored `target/` paths.

ADR-0069 adds the first native identity-request ingress boundary. Manganis
0.7.10 is kept on the same release as Dioxus 0.7.10 and packages the single
static Swift/Kotlin bridge in `adapters/mobile-native-plugin`. iOS uses
AVFoundation and must return `unavailable` in a simulator; Android uses Google
Code Scanner 16.1.0 in QR-only mode without declaring app camera permission.
The native plugins capture bytes only. Keep the 32 KiB bound, payload-redacted
debug/error surface, and strict Rust router between QR/deep-link input and every
protocol flow. SIOPv2 and credential presentation both use `openid4vp`, so standalone
composition classifies only exact registered `client_id`/`request_uri` pairs;
unknown pairs must stay `ambiguous` until reviewed production discovery exists.
Scanning only populates the existing page and cannot bypass preview or consent.
The native lifecycle closure at commit
`a865dbf7572c28f549326c45406b0f93d4664aa4` distinguishes iOS denial,
cancellation, timeout, and unavailability; invalidates the exact active scan
generation on timeout; bounds successful UTF-8 payloads to 32 KiB; and keeps
Android vendor permission/module failures closed as unavailable rather than
inventing a denial state. Android may retain the system-owned scanner UI after
Oxid closes its logical generation, so tests must dismiss it before independent
link checks. Android also serializes an empty-authority offer with a `/` path;
the shared router accepts only `""` or `"/"` while retaining every host, field,
fragment, duplication, and smuggling rejection. Native code never normalizes
or classifies the value.

ADR-0070 registers only `openid-credential-offer` and `openid4vp`. The app-level
Tao handler captures cold iOS events before the component tree exists; the
repository-owned Android `singleTop` activity captures both `onCreate` and
`onNewIntent`. Because Wry does not emit Tao `Opened` for a foreground Android
`onNewIntent`, the rendered component polls only the one-item native handoff at
250 ms; it does not move or log the URL outside the existing ingress port. Both
platform paths enter the ADR-0069 router and remain pending until explicit
dismissal. `PublicTextExportPort` exposes copy/share only for bounded public
receive addresses; never widen it to arbitrary strings or protocol links.
Dioxus 0.7.10 compiles multiple Swift packages but embeds only the primary
framework, so all reviewed native operations must remain in one package until
an upgrade is proven. Android JNI calls use public methods on the activity
instance so the application class loader resolves the plugin from Rust worker
threads. Issue #32 owns physical iOS camera/permission, verified HTTPS links,
production discovery, and remaining device-resource evidence. Physical Android
QR and custom-scheme evidence is recorded below.

`scripts/test-android-identity-ingress.sh` is the focused packaged-host proof:
on `emulator-5554` it has passed scanner cancellation, exact 60-second timeout
closure, and warm/cold custom-scheme delivery into the unchanged consent
boundary. The complete `just android-smoke` flow also passes on the arm64
Pixel Fold API 35 AOSP emulator. Its symbolic `dumpsys window` flags and
full-screen share resolver differ from the Samsung/API 36 physical host: the
harness must accept either symbolic `SECURE` or numeric bit `0x2000`, and may
dismiss a bounded stack of currently resumed chooser activities without
sending Back after Oxid resumes. The complete iOS package and XCUITest suite
builds, installs, launches, and passes on the iPhone 17 Pro / iOS 26.4
simulator `76B99C81-BE72-4A93-A443-7F244723AAF3`, including unavailable-camera
fail-closed behavior, warm/cold custom schemes, wallet/identity consent, and
restart persistence. Xcode 26.4 still prints a duplicate
`UIAccessibilityLoaderWebShared` warning; it did not prevent the current suite
from interacting with the WKWebView. Do not weaken either virtual-device test
or substitute a simulator result for physical camera/permission evidence.

The repository-owned physical harness has also passed on a Samsung SM-S928B
running Android 16 / API 36 with application ID `io.medianox.oxid`: real-camera
credential-offer success reached exactly one strict review item without
consent; Back cancellation, the 60-second logical timeout, post-return controls,
and a fresh scan remained live; and foreground-warm plus force-stopped-cold
custom schemes each reached one dismissible review item. Google Code Scanner
16.1.0 is permissionless on Android, so app camera denial is not applicable.
Do not disable Play Services or alter a personal device to manufacture module
unavailability; retain the fail-closed fixture and record that physical path as
unavailable evidence. On this host, Back after the scanner takes foreground
returns `MlKitException.INTERNAL`, not the documented scanner-cancelled code.
Normalize it only when the owning activity observed suspension during that same
active generation; every pre-presentation/stale internal failure stays failed.
No payload, exception message, or device serial may enter logs, committed
artifacts, or public issue comments. Android 16 `dumpsys window` may expose only
the hexadecimal window mask; test `FLAG_SECURE` bit `0x2000`, not the optional
symbolic word `SECURE`.

ADR-0071 makes normal iOS/Android composition use `storage-mobile`; never add a
fallback from it to development custody. The adapter stores one bounded,
profile-bound sealed vault and keeps all multi-curve/HD operations behind the
existing opaque wallet ports. iOS uses a
`kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly` Keychain item with
`userPresence` and reports operating-system protection. Android uses a
user-authenticated AES-GCM Android Keystore key, requests StrongBox, reports
hardware backing only from `KeyInfo`, and retains only an authenticated
protection label plus IV/ciphertext in an atomic digest-named
`noBackupFilesDir/oxid-custody-v1` record. Both use a
30-second in-process authorization session; expiry or restart must reauthorize.
The selected Manganis 0.7.10 bridge requires one bounded JSON argument for
custody because its generated Swift FFI mishandles the needed multi-string
signature. Never log that request/response or widen the public native API.
Initial Assets rendering must read security status before account state and
must return the public unavailable placeholder while custody is uninitialized
or locked; a background/public read must never open the system credential
surface. Settings reads the mobile lifecycle wake signal and re-queries native
status after the authorization activity resumes, because page-local completion
can be discarded across pause/resume. Android custody smoke must prove the old
PID is gone, the replacement PID differs, the explicit action is `Unlock
wallet`, the active profile and opaque sealed record remain unchanged, and the
post-derivation unshielded address matches. Wait for the activation control to
disappear before capturing that address; the simulated source exposes public
fixture rows before protected derivation and its synchronized flag is
intentionally process-local.

ADR-0074 adds portable custody without making native sealed-vault ciphertext
portable. `oxid-adapter-backup-portable` owns exact `OXIDBAK1` version 1:
Argon2id v1.3 at 19,456 KiB/t=2/p=1 with a 16-byte random salt, then
XChaCha20-Poly1305 with a 24-byte random nonce and the complete fixed header as
AAD. Reject any version/algorithm/parameter/length change before the KDF;
wrong-secret and tamper failures must remain indistinguishable. Packages are
1 MiB maximum, profile-bound, limited to 256 exact keys, and contain the root,
generated secrets, or public derivation paths needed to reconstruct and verify
every public key. They may initialize only an empty destination after full
validation. Mobile export must always call native unlock with the dedicated
backup reason even during an active session; recovery must use native
initialize for fresh authorization. Do not add recovery to `oxid.headless.v1`,
copy raw secrets to UI/clipboard/logs, accept arbitrary paths, reuse device-vault
ciphertext, or claim all-store recovery until profile/DID/credential records are
staged atomically. ADR-0075 supplies native document transfer and the
legacy custody-only Dioxus recovery path; ADR-0076 adds the all-store archive
and complete export/fresh-install recovery UX.

ADR-0075 fixes portable-backup document authority at the OS boundary.
`PortableWalletBackupDocumentPort` transports only the bounded encrypted
package and exposes explicit cancellation/unavailable/timeout/invalid/failure;
callers receive no path and choose only a closed document kind. Custody-only
exports use `oxid-wallet-custody.oxidbak`; complete exports use
`oxid-wallet.oxidbak`. `oxid-adapter-backup-document-mobile` polls the
single repository-owned Manganis plugin for at most five minutes. iOS uses a
complete-file-protected, no-backup temporary export removed after
`UIDocumentPickerViewController` completion and requires one copied regular
non-symlink import no larger than the 80 MiB outer application bound. Android uses only
`ACTION_CREATE_DOCUMENT`/`ACTION_OPEN_DOCUMENT` openable content URIs and
enforces the same bound before and during streaming; the version-1 codec still
enforces its smaller custody-only framing. Keep the encrypted package
out of clipboard/share/app-link/WebView/headless surfaces. Settings recovery is
available only for an uninitialized matching profile, uses zeroizing Rust input
state and exact confirmations, and remains explicitly labelled as the legacy
custody-only path. Complete export uses the same document port; first-run
Dioxus recovery authenticates the archive before learning or selecting its
profile. The Swift and Kotlin bridges must allow exactly the two fixed names
`oxid-wallet-custody.oxidbak` and `oxid-wallet.oxidbak`; accepting only the
legacy name makes complete export fail before the picker. `just
ios-backup-smoke` creates a disposable simulator and proves complete native
export, app uninstall/keychain reset/reboot/reinstall, native import, and
restoration of the profile, Standalone account association and both receive
address projections, managed DID, and Digital Passport credential. The harness
does not inject backup bytes or call recovery directly. Android picker parity
is proven by `just android-backup-smoke`: it uses DocumentsUI, an isolated
`OxidBackupSmoke-<pid>` Downloads directory, app uninstall, emulator reboot,
exact-APK reinstall, and visible import before asserting the same restored
state. It refuses physical devices and removes only the exact backup file and
validated test directory. Physical-device evidence remains #33; never turn
either simulator result into a physical-device claim.

ADR-0076 is the accepted all-store recovery boundary. A complete backup is one
profile-scoped authenticated archive containing domain snapshots for the public
profile/associations, public DID records, complete credential records, and
portable custody. It is not a copy of repository files or native sealed-vault
ciphertext. Recovery must fully validate, reject every destination conflict,
journal only safe identifiers/counts, stage public and credential records,
initialize custody last, and roll back or reconcile after interruption. Fresh-
install recovery learns the destination profile only from the authenticated
archive; an existing-profile flow may additionally bind that exact profile.
Keep recovery absent from `oxid.headless.v1`. Public Midnight persistence may
retain only selected network plus bounded account/address indices—never
addresses, key references, endpoints, balances, or history. Reconstruct DID
control after restart only by a unique exact algorithm/public-JWK match against
authorized custody; never persist opaque DID key references in the public store.
The association/rebinding foundations, strict store snapshot codecs, single
authenticated envelope, and journaled custody-last recovery coordinator exist.
The file recovery journal shares the mobile `private/` directory with strict
JSON stores: create that immediate parent as owner-only mode `0700`, reject an
existing parent with any group/other access, and keep the journal itself mode
`0600`. Otherwise fresh complete recovery stages the profile, then fails when
the DID store rejects the insecure shared directory and rolls back.
Settings exports version 3 under ADR-0078; the empty-profile Dioxus gateway
recovers versions 2 and 3 without a caller-selected identifier and preserves a
legacy version-1 importer.
The in-process standalone composition test creates a profile, exact Midnight
account association, managed DID, holder-bound private credential, and custody,
then recovers all of them into a fresh composition. Keep the recovery methods
unchanged when evolving backup completion UX.

ADR-0090 adds a separate public backup-completion receipt: profile identifier
plus the latest successful complete-document-export timestamp only. Record it
only after complete archive encryption and
`PortableWalletBackupDocumentPort::export(CompleteWallet, ...)` both return
success; cancellation, errors, legacy custody recovery, and complete recovery
must never create it. The timestamp is monotonic, unknown profiles fail, profile
removal removes it, and profile-store schema v3 persists it in
`completeBackupReceipts`. Complete archives deliberately exclude receipts, so a
restored installation cannot inherit a stale **Backed up** claim. Never add a
path, filename, document-provider identity, archive bytes, native authorization
result, recovery secret, or key metadata to the receipt. Home and Settings may
say **Backed up** only from this application query and must warn that the
external document can later move or disappear.

Fresh onboarding is a Dioxus-local route: exactly **Create new wallet** or
**Restore from backup**, then profile naming, then skippable device protection.
Do not expose the opaque profile id, promise biometrics/hardware backing, invent
a seed phrase, or weaken the existing authenticated empty-install recovery.
absent from `oxid.headless.v1`. Both headless and in-memory standalone Midnight
adapters must stay connected to their profile association repository or exact
account rebinding silently disappears. The encrypted package boundary is 80 MiB
and both native document plugins enforce the same bound. The complete iOS
Simulator and Android emulator picker round trips pass; physical-device
peak-memory, latency, interruption, and thermal measurements remain release
gates.
When a persisted account association outlives process-local development custody,
the account read can correctly return `ProtectionNotInitialized`; the Dioxus
Assets page must retain a public, unavailable placeholder so reactivation stays
reachable rather than collapsing into a terminal account-load error.

ADR-0077 preserves the reviewed prototype's useful UI-worker separation without
its aggregate secret-bearing worker messages. On native targets,
`ui-dioxus::run_ui_blocking` owns one named 8 MiB thread and one-shot result for
each admitted synchronous operation; Dioxus signals never cross that boundary.
Profile/account persistence, wallet initialization/unlock/lock, account
derivation, transfer preparation/authorization, Passport Vault call
authorization, managed-DID persistence and custody, and complete/legacy backup
KDF/recovery use it. Encrypted credential reads/disclosure/deletion, standalone
Passport Vault persistence, submission-history reads, DUST/Zswap start, and
protocol refusal use it too. `run_ui_future` polls complete native application
futures on the same boundary because synchronous repository/crypto work can
surround an await. Publish busy state before dispatch, keep worker failures
payload-free, and do not claim cancellation after work starts. The completed
issue #42 audit permits direct calls only for strict bounded identity parsing,
already-published DUST/Zswap status snapshots, retained transfer/vault
draft/status reads, and non-waiting cancellation signals; their port contracts
must continue to forbid filesystem, transport, custody, ledger work, or waiting
for acknowledgement. The WASM branch is only for current in-memory Tier-2
adapters; a production browser adapter needs a reviewed Web Worker.

ADR-0078 hardens new complete-wallet exports without stranding existing files.
`OXIDBAK1` version 3 fixes Argon2id v1.3 at 65,536 KiB/t=3/p=1; all new
complete-wallet exports must use it. Version 2 complete-wallet files remain
readable only with their exact legacy 19,456 KiB/t=2/p=1 tuple, and version 1
remains the legacy custody-only policy. Map the wire version to an exact KDF
policy and reject unknown/cross-version parameter tuples before Argon2id; never
accept attacker-selected ranges. This is stronger offline-file protection, not
physical-device resource evidence. Issue #33 still gates iOS/Android latency,
peak memory, low-memory, interruption, and thermal behavior.

ADR-0081 requires every failed Android JNI operation in the shared native
plugin to check and clear a pending Java exception before returning the closed
bridge failure. Never inspect, describe, retain, or log the throwable. The
Android profile smoke enables a debug-only throw, verifies an immediate second
native call, and then completes the standalone wallet flow; this is emulator
process-liveness evidence, not physical-device or production-custody evidence.

ADR-0082 requires presentation consent to bind an exact visible credential.
Safe candidate views may include only the bounded opaque credential identifier,
display name, and issuer. Dioxus may preselect a sole visible match; multiple
matches must begin unselected, keep consent disabled until a radio-card choice,
and clear consent whenever the choice changes. Never index or silently fall
back to the first candidate. Headless keeps requiring the exact previewed
`credentialId`; no candidate view may expose claim values, openings, proof
material, protocol state, or tokens.

The iOS XCUITest `scrollTo` helper must require content controls to finish at
least 90 points above the application frame bottom before tapping. WKWebView can
otherwise report a control as hittable when only a sliver is exposed above the
fixed Oxid navigation, causing the synthesized tap to miss without invoking the
wallet operation.

`OXID_MOBILE_CUSTODY=development|native` selects the standalone mobile
composition; development is the default. Native mode combines production
custody with deterministic wallet/SSI adapters and must keep simulated
settlement labels. `just ios-native-custody-smoke` validates native capability
or fail-closed behavior because iOS Simulator can reject the passcode-bound
Keychain policy. `just android-native-custody-smoke` exercises a real system
credential prompt, opaque no-backup ciphertext, restart, and stable root; it
must remain restricted to a disposable `emulator-*` without an existing
credential and must clear its temporary PIN/app data on every exit. Physical
device recovery/resource evidence and issue #30 mobile Compact proving budgets
remain release gates. Simulator/emulator proof success is conformance evidence
only.

`just ios-smoke` generates an ignored XCUITest project from
`tests/mobile/ios/project.yml`, discovers every `ProfileFlowTests` method, and
runs each against a freshly reinstalled Oxid app container. Keep that per-test
isolation: onboarding requires an empty profile and the stable standalone
fixtures intentionally exercise replay protection, so a shared container makes
otherwise independent scenarios order-dependent. The harness must select only
`OxidUITests/ProfileFlowTests`; the native-custody harness
selects only `NativeCustodyTests` so feature-specific assertions never run
against the other composition. The development suite verifies profile creation,
development account activation, receive QR,
native public-address copy/share, warm/cold identity links without auto-consent,
fresh shielded sync plus a 1 NIGHT protected transfer with explicit privacy
selection, exact review, authorization, prove/submit, cancellation-safe retry,
and durable inclusion restoration, OpenID4VCI offer preview/consent/issuance, protected
Digital Passport verification/restore, hidden-by-default first/last values,
explicit local reveal/hide, age-predicate preview, consented self-issued DID
authentication, OpenID4VP/DCQL request preview/exact consent/fail-closed Compact
proof gate, and profile restore through visible UI elements.
`just android-smoke` resets only Oxid's Android app data, drives the equivalent
development flow, asserts the native clipboard/share chooser and warm/cold app
links, validates the durable public JSON document plus authenticated credential
envelope/key shape, and verifies restart. Both now assert that the
included public submission journal and encrypted issued-credential inventory
survive while development custody and incomplete issuance sessions reset. The
commands are destructive to the selected simulator's Oxid test profile state;
protected development roots and transaction drafts are process-local and are
expected to disappear on restart.

`just android-backup-smoke` is emulator-only and intentionally more
destructive than the ordinary Android smoke: it exports a populated complete
wallet through DocumentsUI, uninstalls Oxid, reboots, reinstalls the exact
development APK, imports through DocumentsUI into an empty install, and checks
the restored public profile schema/account association, receive-address
projections, managed DID, and encrypted Digital Passport inventory. The
recovery secret is test-only. The harness owns only its uniquely named
Downloads directory and must never broaden cleanup or claim physical-device
evidence.

The Android profile assertion targets current public store schema v3 and must
validate the active profile's `undeployed` account association, account index,
address index, and empty backup-receipt list before any completed export. Schema
v1 and v2 are read-only compatibility inputs and are upgraded on write; a
mobile smoke harness must not require either legacy output shape.
After dismissing Android's native share chooser, wait until Oxid's MainActivity
is the resumed activity before delivering a warm app link. Sending the link
while the chooser still owns the task can produce only a task-front restart
attempt and skip the repository-owned `onNewIntent` capture seam. Some
foldable AOSP images require two Back events to dismiss the full-screen chooser;
inspect only the current `topResumedActivity`/`ResumedActivity`, use a bounded
dismissal loop, and stop immediately when Oxid resumes so the harness cannot
back out to the launcher.

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

1. Fetch `origin/integration` and start issue-backed product, refactor, quality,
   or tooling work from that exact ref. Pull requests target `integration`, the
   only writable delivery and Pages publishing branch. Historical `main` and
   migration-era `develop` are read-only under repository ruleset `21481544`.
   Follow `docs/integration-delivery.md`.
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
- `scripts/check-brand-packs.sh` plus the auto-enumerated Nix brand checks must
  reject schema, path/SVG, two-scheme contrast, and pack-root drift before a
  thin app ships.
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
- ADR-0080 permits runtime-health visibility only through the diagnostics
  application port's closed payload-free codes and the bounded process-local
  memory adapter (default 256, hard cap 1,024). The ring has no timestamps,
  custom strings, identifiers, endpoints, persistence, upload, or process
  statistics; reset requires exact `CLEAR_LOCAL_DIAGNOSTICS`. Diagnostics are
  best-effort and never authorize readiness or retry. DUST/Zswap worker panics
  must publish terminal sanitized snapshots, and retained contract-call unwind
  must always release its active process reservation.
- Never log secrets, seeds, private identifiers, credential claims, signing
  payloads, or raw external error bodies that may contain them.
- Every new fallible Android JNI call in `oxid-adapter-mobile-native` must use
  the ADR-0081 mapper so a Java throw is cleared before the existing
  payload-free failure returns. Do not call exception describe or expose a
  throwable through diagnostics.
- Standalone indexer and proof-server HTTP routes are explicit trust-boundary
  configuration and intentionally ignore ambient process proxy variables.
  Keep their HTTP request/status/body tests client-free and transport-free:
  reqwest client construction and loopback are not reliable inside pure Linux
  Nix derivations even when the WebSocket loopback harness succeeds.
- Validate profile labels and all future QR/deep-link/protocol input at the
  boundary before use.
- The JSON profile store contains only public labels, identifiers, creation and
  backup-receipt timestamps, active selection, and bounded public Midnight
  account coordinates. Schema v3 is current; v1/v2 are strict compatibility
  reads and must not fabricate receipts. It serializes one repository instance;
  overlapping headless processes must use distinct `OXID_PROFILE_STORE_PATH`
  values.
- The DID JSON store contains validated public DID documents/metadata only. It
  is a separate 128-record/2 MiB owner-private atomic file, rejects symlinks and
  unknown fields, and revalidates domain invariants on read. Never add private
  JWK `d`, credential claims, tokens, endpoint configuration, recovery data, or
  keys. Treat every resolver response as hostile and keep route/body details
  out of errors and logs.
- Compact presentation artifacts are immutable build inputs, not wallet data.
  Native headless proving accepts only an absolute authenticated artifact root,
  rejects symlinked descendants and size/digest/circuit mismatches, runs the
  tagged IR check before checked proving, and verifies the proof again through
  a separately reconstructed public statement. `MZP1` may contain only the
  artifact identity, signed credential, detached issuer proof, `MPS1`, holder
  proof, public communications commitment, and tagged proof. Never add private
  claims, openings, custody references, scalars, nonces, communications
  randomness, or a serialized `ProofPreimage`.
- ADR-0072/0083 conformance builds may borrow the exact runtime-minimal Compact
  artifacts from the signed executable image. Never add runtime artifact-path
  discovery, APK extraction, a mutable copied cache, or download fallback. A
  successful startup authentication or virtual-device proof is not a physical
  device budget. Only the explicit artifact feature may change the mobile
  capability label; normal mobile composition must remain proof-disabled.
- The ADR-0083 proof-control port is non-blocking and payload-free. A control
  signal is `cancellation_requested`, never proof cancellation acknowledgement.
  Measure its timeout from admission, including worker scheduling delay; a
  result produced after that budget is late and must be discarded.
  Do not release the one-proof admission slot or publish a terminal state until
  the worker has stopped using witness/custody material, independent
  verification has completed, and any late result has been discarded. Never
  detach or force-stop the prover thread.
- ADR-0073 standalone credential policy must keep resolver, clock, and trust
  inputs explicit. A detached proof key is not issuer authority by itself;
  require exact DID controller/assertion authorization and canonical Jubjub
  coordinate equality. Never mark status passed without a reviewed status
  reference and resolver, and never reuse the standalone trust anchor in normal
  production composition.
- Passport Vault incoming views may expose public policy and aggregate amounts,
  public contract issuer anchors, and redacted public audit fields, but never
  credential roots, openings, claim values, detached proof bytes, private
  witnesses, or custody references.
  Standalone deposits, claims, and withdrawals are local state transitions and
  must never be described as submitted, included, or settled on Midnight. Live
  current-day, credential-root nullifier authority, and expiry enforcement must
  come from authenticated chain state, not the standalone clock or repository.
- Passport Vault `node_anchored_indexer` views prove only that the reported
  action block hash is canonical at or below the node's finalized head. They do
  not prove indexer state bytes or transaction provenance. Preserve the
  `indexer_supplied_not_proven` label, explicit caller-supplied address, HTTPS/WSS
  remote-route rule, proxy/redirect prohibition, response bounds, and closed
  mutation ports until replay or a reviewed node proof authenticates state.
- Passport Vault finalized-history acquisition requires an explicit deployment
  height, archival node bodies/metadata/events, canonical header continuity,
  exact direct-call success/outcome/address hashes, and the node's historical
  runtime schema. Missing archival data, wrapped target calls, or any gap fail
  closed. The one-million-block and per-response bounds are security limits,
  not permission to truncate and report partial state.
- `OXID_PASSPORT_VAULT_DEPLOYMENT_HEIGHT` is accepted only with the complete
  standalone routes. It enables one-at-a-time canonical replay reads; it does
  not enable calls, cache partial history, or turn indexer bytes into authority.
- Passport Vault contract-call preparation must admit only
  `canonical_finalized_replay` in live composition. The development-only call
  service must admit only `deterministic_simulation`; its state, call mode, and
  `settlesOnMidnight: false` capability label must never be relabelled live,
  persisted as chain history, or composed with authenticated live replay.
  Retain chain-specific call/proof material behind an opaque draft ID, require
  distinct authorization and submission intents, and never project a
  credential ID, private credential data, witness, holder key, nonce, proof,
  signature, or serialized transaction into headless output or errors.
- Passport Vault call composition/proving may use only the exact ADR-0058
  artifact identity. Authenticate the generated client, ABI, keys, verifier
  keys, binary ZKIR, and parameters by compiled-in size/digest; reject symlink
  traversal. The wallet resolver is closed to the four user circuits and
  degrees 10/11/17. `setTrustedIssuer` and degree 13 are administrative or DUST
  concerns and must not resolve through it. Exact IR digest plus encoded degree
  is the startup gate; expand the large claim model only during proof checking.
- The ADR-0059 composer is one-request and internal. Keep its artifact closure
  fixed by Nix, clear `NODE_OPTIONS`/`NODE_PATH`, retain exact object schemas and
  canonical bounds. ADR-0065 permits claim material/private state only through
  the owned zeroizing DTO produced after exact call authorization; never accept
  those fields from headless/mobile, add arbitrary circuit names/arguments, or
  expose administration. Its unproven transaction output is adapter-owned and
  cannot appear in headless/mobile output or logs.
- ADR-0060 native preparation accepts only canonical replay and a fresh public
  context source with real non-empty bounded Zswap state and ledger parameters.
  Never decode those values from an incoming UI address, let callers supply
  them, or couple the Passport Vault adapter directly to the Midnight adapter.
  Keep the serialized transaction in zeroizing retained custody; until the
  protected DUST proving/journal/submission path consumes the funded result,
  submit must fail without changing the authorized draft or `not_started`
  status.
- ADR-0061 permits that public context only after the node-anchored indexer
  state/action matches canonical replay byte-for-byte. Keep Zswap state and
  current ledger parameters bounded and snapshot-bound; require exact selected-
  network address HRPs and payload lengths inside the Midnight adapter; join
  the two sources only in composition.
- ADR-0062 permits NIGHT funding only after the exact unexpired authorization
  challenge. Derive the deficit from the decoded generated transaction; never
  accept it from incoming code. Create/deposit require exactly one native NIGHT
  deficit, withdraw requires none, and any other negative unshielded token or
  segment ambiguity fails closed. Select only synchronized account UTXOs,
  return exact change, sign through opaque protected custody, verify the
  signature, and provide one signature per input. Keep both pre-funding and
  funded serialized transactions zeroizing and composition-private. A failure
  must preserve the prepared draft for explicit retry. `native_funded_draft`
  still means prepare/authorize only and must keep `settlesOnMidnight: false`.
- ADR-0063 composes native Passport Vault create/deposit/withdraw with the same
  protected DUST, proving, persist-before-broadcast, node submission,
  cancellation, and finalized reconciliation path as ordinary transfers. The
  public journal schema is version 2 with optional finalized block height and
  backward-compatible version-1 reads. Vault records use a domain-separated
  profile key plus `vault-` draft prefix so they never appear in transfer
  history. `native_settlement` may report `settlesOnMidnight: true` and, after
  ADR-0066's managed-custody packaged-client conformance, advertise all four
  wallet operations. Never reintroduce the prototype's public-derived holder
  scalar or fixed nonce 17, and never describe deterministic test completion
  as live node inclusion.
  Configure `OXID_MIDNIGHT_SUBMISSION_JOURNAL_PATH` for restart-safe public
  status; never persist transactions, proofs, signatures, witnesses, or keys.
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
  bounds, strictly increasing sparse cursor and non-regressing target checks,
  16,384-event/16 MiB complete/drop-before-fold receive segments,
  256-event/4 MiB replay batches, and one-million-event/512 MiB run limits.
- Transaction attempt status must remain separate from draft lifecycle. Retain
  cancellation primitives inside the Midnight adapter, mark broadcast before
  the node call, restore `Authorized` only after acknowledged pre-broadcast
  cancellation, and never label broadcasting or unknown outcomes retryable.
- Keep production secret storage behind platform-backed adapters. The in-memory
  adapter is development/test infrastructure and must never be presented as
  durable or secure storage.
- ADR-0097's `standalone-local` and `standalone-tailnet` profiles are
  compile-time-only development composition and incompatible with native
  custody or each other. Never commit a personal tailnet IP, MagicDNS name,
  standalone password, or endpoint. Use `just standalone-phone-up` to keep
  Docker services on loopback and expose the indexer/node/prover through
  Oxid-owned TLS Tailscale Serve routes; use `just standalone-down` to remove
  only the Serve configuration marked as Oxid-owned. The localhost profile
  embeds only the reviewed `undeployed` routes
  `ws://127.0.0.1:8088/api/v4/graphql/ws`,
  `http://127.0.0.1:8088/api/v4/graphql`, `ws://127.0.0.1:9944`, and
  `http://127.0.0.1:6300`. iOS Simulator reaches laptop loopback directly;
  Android emulator must be verified as qemu and use exact `adb reverse`
  mappings for 8088, 9944, and 6300. Never use `10.0.2.2`: plaintext proving
  is allowed only to syntactic loopback under ADR-0027. Leave those exact
  reverse mappings in place for the installed development app; never remove
  unrelated mappings with `reverse --remove-all`. The prototype exposes its
  localhost/Tailscale entries through a runtime network picker; Oxid's
  compile-time split is intentional hardening, not copied behavior. The public
  undeployed placeholder validates composition only and must be replaced by
  profile-derived account binding before sync. Every persistent live/standalone
  composition must attach the same `JsonWalletProfileRepository` instance to
  the Midnight adapter; otherwise schema-v3 public account coordinates
  disappear on restart. The exact `indexer-standalone:4.0.0` image rejects the
  newer singular `fee` query field. The prototype's
  `mobile-bench/wallet-core/queries/midnight-indexer/unshielded_transactions.subscription.graphql`
  deliberately requests neither fee field; Oxid's richer transaction history
  therefore uses the image-compatible `fees { paidFees }` shape. Keep that
  compatibility choice unless a pinned image/schema upgrade is made atomically.
  Development custody remains process-local: after process death, retain the
  public association but report uninitialized protection and withhold the
  former addresses. This private harness is not verified public App Link or
  production-discovery evidence. Both live profiles share one undeployed chain
  identity, the same typed adapters, and the same durable public
  profile/account binding; only transport differs. Deterministic
  `standalone-development` remains a third, distinct simulator mode.
  Local acceptance evidence from 2026-08-20 is reproducible with `just
  ios-standalone-local-smoke` and `just android-standalone-local-smoke`, run
  sequentially. The iOS flow passed on iPhone 17 Pro / iOS 26.4; the Android
  flow passed from a stopped `sdk_gphone64_arm64` AVD on Android 15 / API 35.
  Both require a newly derived account to report `Live`, synchronized
  live-source state and both address rails while excluding the simulation
  labels and balances. Android additionally verifies exact reverse mappings for
  ports 8088, 9944, and 6300. Android WebView automation must wait for the
  computed masked-value CSS invariant after `data-secret-mode=masked`; that
  attribute can settle one render before the transparent text and four-dot
  overlay. Emulator 34.2.16 can print a crash-report setup
  notice to standard output before its `-list-avds` result, so AVD discovery
  must accept only a returned name backed by an actual `.ini` file.
- ADR-0098 production composition requires an
  `oxid.deployment-profile.v1` canonical Ed25519 envelope that atomically binds
  application audience, validity/sequence, Midnight network/genesis and all
  Midnight/SSI routes. Trust roots and sequence floors must be reviewed
  build-time inputs; environment variables are never production trust or route
  authority. After signature verification, require the signed node's exact
  genesis hash before composition. The default `compose()` remains fail-closed,
  no production root/profile is currently selected, and issuer/verifier
  protocol transports remain unavailable. The ignored funded standalone gate
  is `just standalone-funded-finality`; it additionally requires
  `OXID_ENABLE_LIVE_STANDALONE_FUNDING=1` and the out-of-band
  `OXID_STANDALONE_FUNDER_SEED_HEX`. Never print, commit, persist, or place that
  seed in an issue. It is zeroized after one development-root generation; every
  recipient and later nonce uses OS randomness. The 2026-08-20 run proved exact
  five-NIGHT authorization, DUST proof, node finality, public-journal restart
  reconciliation, bounded recipient indexer convergence, and a stable second
  read without duplicate delivery. Standalone readiness must compare indexer
  and node heights because Docker health can become green during replay.
  Midnight indexer v4 block timestamps are milliseconds and must be divided by
  1,000 at the ledger boundary. DUST event IDs are sparse global cursors: accept
  only strict forward movement and a nondecreasing advertised target, not
  artificial contiguity. These facts match the immutable prototype baseline
  `074b1a4bccbfee1740ee188374b606a022ecef42` and must remain focused tests.
  The parallel ignored gate is `just standalone-funded-shielded-finality`. Its
  2026-08-20 run proved a real 1,000,000-atomic native Zswap transfer from the
  development genesis authority to a fresh protected recipient, exact consent,
  DUST/Zswap proof, finalized inclusion, included-fingerprint duplicate
  blocking, adapter reconstruction, idempotent included-status restoration,
  nullifier replay, and stable exact balances. It does not prove unknown-
  outcome chain rescanning. The v4 Zswap envelope typename is
  `ZswapLedgerEvent`, and its event IDs are sparse monotonic global cursors.
  Never call this a process/native-custody restart or fresh-wallet origination;
  issue #92 owns the funded registration-to-generation/resynchronization and
  stronger spend proof.
  Until
  issue #93 adds checkpoint-acknowledged compaction, a full 128-record journal
  of included/unresolved barriers must fail unavailable before broadcast.
- ADR-0100 implements protected DUST registration as a separate
  `WalletDustRegistrationPort`, not a transfer mode or sync side effect. A
  fresh wallet intentionally starts at zero DUST. Preparation requires a
  current live account fold with authoritative `ctime` and
  `registeredForDustGeneration == false`, rejects zero/duplicate inputs, puts
  only the greatest generated-DUST candidate in the guaranteed offer, returns
  every exact NIGHT amount to its owner, and caps the fee allowance to that
  guaranteed candidate. Separate exact confirmations gate role-0 authorization
  and submission; the role-2 DUST child stays inside custody. Registration and
  transfer drafts/journal lookups are domain-separated. Inclusion means only
  that the registration transaction finalized; spend readiness still requires
  a fully caught-up official DUST event/checkpoint. Public account checkpoints
  are schema version two; schema-one files are ignored and replay starts from
  zero rather than fabricating eligibility.
  The repository/headless/Dioxus lifecycle is implemented and covered by the
  full Midnight adapter suite plus focused domain/application/UI/headless tests.
  `just preprod-registration-funding-manifest` is the guarded no-network
  foundation for live evidence. It additionally requires a clean worktree,
  `OXID_ENABLE_LIVE_PREPROD_E2E=1`, exactly 64 hexadecimal characters in the
  secret `OXID_PREPROD_MASTER_SEED_HEX`, and a canonical
  `OXID_PREPROD_E2E_CASE_INDEX`. Never print or persist that root. The existing
  hardened BIP44 account index supplies A=`2*caseIndex` and B=A+1 through two
  separate test-only custody instances. Output is a closed public manifest of
  exact commit/network/case/account/address indices, A/B NIGHT/shielded receive
  addresses, positive-value requirements, exact eligible-output/note counts,
  and the deterministic transfer policy;
  it contains no DUST address/key, secret, digest, UTXO identifier, or
  transaction material. Manifest V2 replaces historical V1's fixed amounts;
  never reinterpret a V1 result as V2. Fund only A with one positive public
  NIGHT output and one positive shielded NIGHT note. The external
  service need not supply a predetermined amount: the live test binds the
  exact observed balances before authorization, requires the one-output/
  one-note topology, and asserts exact principal and transfer deltas. B begins
  with no eligible public outputs or shielded notes. Select the A-to-B transfer
  once as half the observed shielded balance rounded down, with a one-atomic
  minimum, and freeze that amount through preview/authorization/finality. DUST
  must not be externally funded. The scripts unset the exported root before
  Cargo/build scripts run and pass it only to the compiled observer and
  write-test processes. Case indices are single-use.
  The live script first runs the no-write observer and requires positive A,
  one eligible public output, one shielded note, empty B, zero initial DUST,
  and a prepared registration whose exact principal matches the public
  balance. It then recompiles and the write test revalidates those facts. Only
  after that preflight succeeds does it atomically create the ignored
  owner-only directory
  `<git-common-dir>/oxid-state/preprod-registration-e2e/case-<index>.started`
  before the write and refuses reuse across all worktrees in the local clone.
  Never clear it merely to rerun an unknown outcome. Ephemeral CI must receive
  a fresh funded case index for every write because the marker is not globally
  durable. Owner-only checkpoints and the public journal remain below the
  marker on failure for forensic/manual chain audit only: random profile IDs,
  process-local development custody, and the fresh-directory guard mean the
  current harness cannot resume them. Unknown outcomes require external chain
  audit and case abandonment, not retry. A complete successful run removes only
  that retained state and leaves the single-use marker; an explicit
  non-submitting recovery mode remains future work.
  A static test-only `oxid.deployment-profile.v1` envelope and public Ed25519
  root bind the exact PreProd v4 indexer paths, node/proof routes, network, and
  genesis
  `df831b09a8baa92badf47762ce5ac439b7e47e3ed3d39600cfdd44fad552361b`.
  The disposable signing key was generated in memory and discarded. Mandatory
  SSI fields use `.invalid` hosts and are never composed: this is Midnight-only
  test authority, not a production or SSI deployment. The ignored write enters
  the unchanged signature and live node-genesis gate. Its public proving route
  additionally requires
  `OXID_ACKNOWLEDGE_PREPROD_PUBLIC_PROVER_PRIVACY=1`, because TLS does not hide
  proof preimages or timing from the prover operator; never call it production
  privacy evidence or splice in an unsigned local route.
  `just preprod-registration-observe` is a clean-commit-bound, no-write
  preflight. It composes no checkpoint or journal paths. Readiness preparation
  retains one unsigned process-local draft which is discarded at test exit;
  there is no authorization, proof, persistence, broadcast, chain write, state
  directory, single-use marker, or prover contact. It emits only a closed set
  of public aggregate account/shielded/DUST/readiness fields. Cold PreProd DUST
  replay has a test-only 15-minute bound; ordinary standalone
  synchronization retains its 120-second bound. On 2026-08-20, early attempts
  exceeded the old DUST or shielded stage bounds before public output. A plain
  debug run on signed `26505c81bde1a7c5e4bc13e559232cf0ebf8d97a`
  proved DUST transport segmentation stayed `syncing` with no failure and
  reached cursor 227,235 of target 1,445,979 after 218,252 events, but exceeded
  the observer's 900-second DUST wait. It used the unavailable checkpoint store,
  so no checkpoint was serialized or persisted. The focused 16,385-event DUST
  regression took 9.41 seconds in debug and 0.35 seconds in the optimized
  `preprod-live` profile introduced by signed
  `2763125bb71a445f608bc6a8a8f98cf51c49495a`. That commit's first optimized
  live attempt then exceeded the shielded stage's 90-second wait because Zswap
  still folded inline under an open subscription. Signed
  `a490dc0f754b9a3f89483c875dc68a77ea7f29d5` closes the analogous shielded
  backpressure gap. A clean optimized observer on signed
  `fba4ad429fc59e73e9baba7d1af9bea4c9b37dea` passed shielded sync and reached
  DUST cursor 553,478 of target 1,446,220 after 541,357 events at the 900-second
  wait, still `syncing` with no failure. Its ~602 events/second is 2.5 times the
  debug rate; applying the observed 97.81% cursor density estimates roughly
  1.415 million events and 39 minutes. Treat that as inference, not an exact
  count. It suggests the one-million-event/30-minute caps need an explicit
  measured review; raw bytes against the 512 MiB cap remain unknown and no cap
  has been raised. Issue #115 owns a guarded capacity harness: first preserve
  an offline corpus of at least 131,072 official-shaped events, then perform
  two profile-backed optimization iterations before repeating the read-only
  PreProd observation. Keep its aggregate report closed and public-safe. Never
  raise an event, byte, or time cap from a partial-prefix extrapolation.
  Issue #116 separately owns an ADR-first, birthday-gated replay-reference
  design inspired by Moth Wallet's "pre-seed reference" pattern. Its immutable
  research baseline is Moth commit
  `f17a8bd9ff57fe58854c86e2a61f92cb20e8eb14`; Moth calls the cacheless
  genesis benchmark a genuine cold start, while the fast fresh-wallet path
  starts from a reviewed reference and then catches up live. Oxid must define
  its own authenticated, bounded Rust artifact and chain-derived account
  birthday. Imported, restored, legacy, or unknown-birthday wallets must keep
  full replay, and upstream authenticated sparse synchronization may supersede
  the proposal. Do not copy Moth's JavaScript, JSON/key-swapping state format,
  NPM dependencies, generated caches, or pre-seed artifacts.
  All attempts failed before public output and created no
  write marker, checkpoint/journal file, proof, transaction, prover contact,
  or chain write. Do not treat these transport observations as a funding
  mismatch or retry a write because of them.
  `just preprod-registration-e2e` is implemented but remains unrun. The
  out-of-band root/case are configured and case 0 has been externally funded,
  but the exact indexed topology still needs a successful read-only
  observation and the public-prover privacy tradeoff still needs explicit user
  acknowledgement. The write must prove zero initial DUST, registration finality,
  later generated-DUST observation, application-level adapter reconstruction
  plus authoritative resynchronization/duplicate suppression, and the exact
  A-to-B shielded spend. A positive DUST observation is not a fee quote: only
  exact pre-broadcast `InsufficientDust` may trigger a bounded wait and
  resubmission of the same authorized draft. It does not traverse the
  NDJSON headless adapter and does not prove a process/native-custody restart.
- Physical Android identity-ingress evidence must use
  `scripts/test-android-identity-ingress-physical.sh`. It refuses virtual
  devices and a concurrently booted iOS simulator, never clears application
  data, and separates scan preparation from holder-controlled scan,
  cancellation, and timeout actions. Its QR is a public deterministic offer;
  do not commit device serials or generated `target/physical-evidence` files.
  The exact full Android smoke may clear only `io.medianox.oxid` after explicit
  approval and must parse numeric `FLAG_SECURE` bit `0x2000` because current
  Samsung/API 36 `dumpsys` omits the symbolic label.
- Use opaque key references. Key-generation and signing ports must not return
  raw private keys to application or UI layers.
- Record every significant dependency using the review template in the
  blueprint before an adapter becomes production-facing.
- On 2026-08-20 the crates.io owner yanked `arrayref` 0.3.5 through 0.3.9 and
  published 0.3.10, while the canonical repository still ended at reviewed
  commit `f8d0299d863922db6c409d08098941e833b70d69`/version 0.3.9. The registry
  0.3.10 manifest adds `proc-macro1 1.0.107`, which is absent from that
  canonical commit. Do not resolve or compile that unreviewed publication.
  The canonical Git repository then became unavailable after the first signed
  pin reached `develop`. The already-published 0.3.9 crate archive was compared
  with that reviewed checkout: all authored source, manifest, examples, README,
  license, and CI files match; only Cargo publication metadata/normalization is
  added. Oxid therefore uses the checksum-locked 0.3.9 registry archive
  (`76a2e8124351fda1ef8aaaa3bbd7ebbcb486bbcd4225aca0aa0d84bb2db8fecb`)
  without an unavailable Git fetch. `scripts/check-arrayref-source.sh` enforces
  the version, source, checksum, and absence of `proc-macro1`. Change this pin
  only after independent review and a signed, green dependency-source change.
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
