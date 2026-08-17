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
DIDs, protected credential inventory, embedded pre-authorized OpenID4VCI
issuance, and consented self-issued DID authentication are now functional in
standalone development. Digital Passport private parts are commitment-bound and
encrypted; safe headless disclosure planning plus explicit local Dioxus
first/last reveal and age-threshold planning are functional. Strict standalone
OpenID4VP request, matching, and consent are functional and the real Compact
presentation artifact set is reproducible. Standalone OpenID4VCI now issues,
encrypts, restores, and natively verifies the prototype's exact Compact
credential shape plus a detached issuance proof dynamically bound to the
selected profile's managed Jubjub assertion method. ADR-0051 now delivers the
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
production/mobile presentation proving
remain deferred.
Standalone presentation now reauthorizes the exact statement with the
credential-bound method's current managed protected key, runs the authenticated
k=18 Compact circuit, and independently verifies the public `MZP1` envelope
before permitting an internal `vp_token`. The headless executable enables that
path only when `OXID_PRESENTATION_ARTIFACTS_DIR` names the immutable Nix
artifact closure; without it, consent fails closed at `proof_unavailable`.
Headless views never expose the proof or token.

ADR-0072 adds only the first mobile Compact resource gate. The app feature
`standalone-native-proving-artifacts` implies native custody and embeds the
runtime-minimal 135,351,737-byte Nix input (manifest, prover, verifier, compiled
ZKIR, and p18 parameters) directly in the executable. The adapter authenticates
the compiled-in source/toolchain/circuit identity, exact sizes, digests,
circuit, and verifier key without runtime discovery, extraction, cache, or
network IO. Select it only through
`OXID_MOBILE_CUSTODY=native OXID_MOBILE_PRESENTATION_PROVING=artifacts just
ios-run|android-run`. This is a package/startup measurement harness: it must not
set `compact_presentation_proof_available`, and mobile consent must continue to
return `proof_unavailable` until a dedicated foreground worker, cooperative
cancellation, process-death/background policy, physical-device budgets, and the
remaining ADR-0071 release gate are accepted.

The first 2026-08-17 debug package evidence is deliberately non-release: an
iPhone 17e iOS 26.4 simulator produced a 257,526,696-byte uncompressed bundle
versus 173,593,496 bytes without the feature (83,933,200-byte debug delta) and
remained responsive at 455,136 KiB host-reported RSS after startup; the arm64
Android emulator produced a 539,163,753-byte APK versus 404,307,855 bytes
without the feature (134,855,898-byte debug delta) and remained responsive at
310,462 KB PSS / 427,424 KB RSS with no swap. Neither run executed the
presentation prover. Do not promote these virtual-device debug values into
budgets or claims about physical-device latency, thermal behavior, installed
size, or proof memory.

The focused aarch64-darwin release embedded-package test authenticates and
constructs the checked runtime in 3.92 seconds. macOS `/usr/bin/time -l`
reports 5.44 seconds wall, 440,074,240 bytes maximum RSS, 211,911,424 bytes peak
footprint, and no swaps. This is authentication-only host evidence, not mobile
or proof-execution evidence.

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
the signed body. Normal `compose()` remains unavailable pending native
Keychain/Keystore wrapping. Live OID4VCI/OpenID4VP transport, mobile proving,
status/trust policy, issuer-anchored Compact verification, and native custody
remain later slices. ADR-0045 adds
exact detached Compact issuance-proof verification without claiming issuer
trust or presentation proof generation; ADR-0046 adds the exact development
signing primitive, and ADR-0047 binds standalone issuance to the selected
managed Jubjub DID method. ADR-0048 reauthorizes that exact reference against
the current managed protected method before proof execution; ADR-0049 now
constructs and independently checks the distinct credential-family holder
`Proof`; ADR-0050 connects the ZK runtime only in explicit native headless
composition.

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
Transaction Code, batch/deferred issuance, OpenID4VP, deep links, and scanning
remain separate slices.
The standalone issuer must independently resolve the selected public DID
method and verify the Ed25519/P-256 proof JWS, nonce, anonymous-flow `iss`
omission, audience, algorithm, and bounded `iat`; structural JWT validation is
not sufficient.
`DidRecordView.managed_method_ids` is current-process capability metadata, not
persisted ownership. Credential issuance must select an active authentication
method from this set; never infer control merely because a resolved or restored
public DID document contains an authentication relationship.

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
nine-chunk holder `Proof`. The native headless-only runtime then constructs a
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
runtime, `presentationGenerated` and `verifierValidated` become true only after
the real proof succeeds. Normal/mobile composition keeps the prover unavailable.
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
Schnorr equation, including identity-point and tamper rejection. It marks only
structural/proof/schema passed; issuer method anchoring, current-time policy,
status, and trust remain `not_checked`.

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
prover and independent verifier do so only for explicit native headless mode.

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
presentation gate or claiming issuer trust/selected-DID native Jubjub custody.
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
| `crates/presentation/application` | Profile-scoped presentation use cases plus protocol, candidate, current-holder authorization, proof, and independent-verifier ports. |
| `crates/passport-vault/domain` | Dependency-free product lock policy, creator authorization, checked accounting, and per-lock credential replay invariants. |
| `crates/passport-vault/application` | Passport Vault list/create/deposit/claim/withdraw use cases plus focused repository, credential-policy, bounded contract-state source, and retained four-operation contract-call ports. |
| `crates/platform/ports` | Clock, randomness, and bounded native QR-scanner capabilities used by applications. |
| `crates/adapters/storage-memory` | Development/test implementations of wallet, DID, and credential persistence ports. |
| `crates/adapters/storage-json` | Versioned persistence for public profile metadata and active selection only. |
| `crates/adapters/storage-identity-json` | Strict versioned persistence for validated profile-scoped public DID documents only. |
| `crates/adapters/storage-credential-json` | Development-only authenticated encryption for bounded profile-scoped credential records, original signed bytes, detached proofs, and opaque format-private material. |
| `crates/adapters/storage-dev` | Process-local, development-only Ed25519/P-256/Jubjub generation plus protected BIP32/secp256k1-Schnorr derivation, one-shot signing, and atomic fresh-nonce Jubjub challenge completion. |
| `crates/adapters/midnight` | Midnight network/account and native canonical-transaction adapter with fail-closed production, simulation/live sources, protected public-account binding, retained development drafts, standalone DUST/proving/submission completion, and bounded public submission recovery. |
| `crates/adapters/did-midnight` | Single-fixture standalone and explicit bounded native Midnight DID resolution plus development Ed25519/P-256/Jubjub lifecycle and managed-method challenge-signing adapters. |
| `crates/adapters/vc-midnight` | Strict Midnight phase-1 CBOR verification, exact native Compact body/detached-issuance-proof verification and standalone holder-bound reissuance, commitment-bound Digital Passport private-part interpretation, generated-Compact presentation public-input conformance/preflight, current managed Jubjub holder reauthorization, exact credential-family holder-proof construction/verification, and public standalone fixtures. |
| `crates/adapters/passport-vault` | Product-specific bounded in-memory plus owner-private atomic standalone repositories, exact standalone Digital Passport policy bridge, native pinned-layout decoder, node-anchored unproven indexer read, pure canonical replay verifier, history-complete finalized-node collector, opt-in authenticated replay source, exact four-circuit generated-client/proof artifact resolver, generated-composer/Rust-codec conformance, and zeroizing authorization-bound settlement for create/deposit/claim/withdraw; managed-custody claim conformance is exercised through composition. |
| `crates/adapters/openid4vci` | Strict OpenID4VCI 1.0 Final embedded pre-authorized flow, separate authentication/holder-binding validation, in-process standalone issuer, DID proof bridge, and verified credential sink. |
| `crates/adapters/siopv2` | Strict SIOPv2 draft-13 standalone request-by-reference login, opaque DID proof bridge, and independent single-use verifier. |
| `crates/adapters/openid4vp` | Strict OpenID4VP 1.0 Final-shaped standalone DCQL request, candidate/consent session, and fail-closed Compact proof gate. |
| `crates/adapters/identity-ingress` | Strict credential-offer/registered-OpenID4VP classifier plus payload-redacted native iOS/Android QR scanner adapters. |
| `crates/adapters/mobile-native-plugin` | Single repository-owned Manganis Rust/Swift/Kotlin bridge for QR capture, Android OS-link queueing, and typed public receive-address clipboard/share operations. |
| `contracts/presentation` | Oxid-owned final Compact presentation compositions; generated artifacts remain Nix-store outputs and never enter Git. |
| `contracts/passport-vault` | Byte-identical Apache-2.0 Passport Vault Compact source distributed for secret-free public builds; its pinned private-upstream provenance and digest are ADR-0053 review boundaries. |
| `nix/packages/passport-vault-compact-artifacts.nix` | Immutable Passport Vault client/IR/key/parameter closure from the hash-checked distributed contract plus pinned VC and Compact toolchain revisions. |
| `nix/packages/passport-vault-call-composer.nix` | One-request Node 24 outgoing adapter package with locked Midnight compatibility dependencies, Nix-fixed authenticated artifacts, closed typed operations, and real generated-client install checks. |
| `tools/passport-vault-composer` | Internal generated-Compact composition implementation; never an incoming headless/mobile API and never a credential/private-witness bridge. |
| `crates/adapters/platform-system` | System clock, OS randomness, and typed public receive-address export implementations. |
| `crates/ui-dioxus` | Dioxus incoming adapter, exact amount/consent presentation state, public receive-QR rendering, standalone Passport Vault UI, and truthfully labelled typed native vault-call review/authorization/submission/cancellation/reconciliation. |
| `crates/composition` | Concrete dependency wiring with no product rules. |
| `apps/oxid` | Executable shell and platform launch point. |
| `apps/oxid-headless` | Standalone NDJSON incoming adapter and flow harness. |

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

ADR-0070 registers only `openid-credential-offer` and `openid4vp`. The app-level
Tao handler captures cold iOS events before the component tree exists; the
repository-owned Android `singleTop` activity captures both `onCreate` and
`onNewIntent`. Both enter the ADR-0069 router and remain pending until explicit
dismissal. `PublicTextExportPort` exposes copy/share only for bounded public
receive addresses; never widen it to arbitrary strings or protocol links.
Dioxus 0.7.10 compiles multiple Swift packages but embeds only the primary
framework, so all reviewed native operations must remain in one package until
an upgrade is proven. Android JNI calls use public methods on the activity
instance so the application class loader resolves the plugin from Rust worker
threads. Issue #32 owns physical-camera, universal-link, production-discovery,
and resource evidence.

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

`OXID_MOBILE_CUSTODY=development|native` selects the standalone mobile
composition; development is the default. Native mode combines production
custody with deterministic wallet/SSI adapters and must keep simulated
settlement labels. `just ios-native-custody-smoke` validates native capability
or fail-closed behavior because iOS Simulator can reject the passcode-bound
Keychain policy. `just android-native-custody-smoke` exercises a real system
credential prompt, opaque no-backup ciphertext, restart, and stable root; it
must remain restricted to a disposable `emulator-*` without an existing
credential and must clear its temporary PIN/app data on every exit. Physical
device recovery/resource evidence and issue #30 mobile Compact proving remain
release gates.

`just ios-smoke` generates an ignored XCUITest project from
`tests/mobile/ios/project.yml`, resets only the installed Oxid simulator data,
and must select only `OxidUITests/ProfileFlowTests`; the native-custody harness
selects only `NativeCustodyTests` so feature-specific assertions never run
against the other composition. The development suite verifies profile creation,
development account activation, receive QR,
native public-address copy/share, warm/cold identity links without auto-consent,
staged simulated transfer, OpenID4VCI offer preview/consent/issuance, protected
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
- Compact presentation artifacts are immutable build inputs, not wallet data.
  Native headless proving accepts only an absolute authenticated artifact root,
  rejects symlinked descendants and size/digest/circuit mismatches, runs the
  tagged IR check before checked proving, and verifies the proof again through
  a separately reconstructed public statement. `MZP1` may contain only the
  artifact identity, signed credential, detached issuer proof, `MPS1`, holder
  proof, public communications commitment, and tagged proof. Never add private
  claims, openings, custody references, scalars, nonces, communications
  randomness, or a serialized `ProofPreimage`.
- ADR-0072 measurement builds may borrow the exact runtime-minimal Compact
  artifacts from the signed executable image. Never add runtime artifact-path
  discovery, APK extraction, a mutable copied cache, or download fallback. A
  successful startup authentication is not proof-execution or device-budget
  evidence and must not change the mobile capability label.
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
