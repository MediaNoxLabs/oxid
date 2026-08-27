# Decision records

> Regenerated at build time from [`docs/adr/README.md`](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/README.md).

## Architecture decision records

ADRs record consequential technical decisions for Oxid. Use four-digit,
monotonically increasing filenames. Each record describes its status, context,
decision, consequences, and any superseded decisions.

Accepted ADRs are changed only by a later ADR that explicitly amends or
supersedes them. The root blueprint remains the broader product and engineering
constitution.

## How to read status

ADR status and delivery state answer different questions:

- **Accepted** means the rule governs new work, even if its capability is on a
  later milestone.
- **Proposed** means research or an explicit decision is still required; it
  does not authorize a dependency or implementation.
- **Implementation state** records current repository evidence and must not be
  read as product-readiness by itself.

## Blueprint and repository traceability

| ADR | Status | Blueprint decision or repository source | Current delivery state |
| --- | --- | --- | --- |
| [0001](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0001-modular-hexagonal-architecture.md) Modular hexagonal architecture | Accepted | §§3, 6 | M0 dependency graph implemented and checked |
| [0002](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0002-dioxus-as-incoming-adapter.md) Dioxus incoming adapter | Accepted | §§3, 6 | M0 UI and composition implemented |
| [0003](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0003-oxid-owned-domain-types.md) Oxid-owned domain types | Accepted | §§3, 6, 18 | Enforced in M0 core APIs |
| [0004](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0004-capability-specific-ports.md) Capability-specific ports | Accepted | §§3, 7 | Initial wallet/platform ports implemented |
| [0005](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0005-static-adapter-composition-for-mvp.md) Static adapter composition | Accepted | §§5, 6 | Implemented in `crates/composition` |
| [0006](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0006-rust-first-controlled-edge-fallbacks.md) Rust-first edge policy | Accepted | §§3, 4 | Enforced; no foreign runtime fallback in M0 |
| [0007](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0007-identity-is-a-peer-capability.md) Identity as a peer capability | Accepted | §§1, 4–6 | DID lifecycle and protected credential inventory delivered by ADR-0036–0038 |
| [0008](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0008-did-methods-as-capability-negotiated-adapters.md) DID adapters | Accepted | §§7, 9 | Resolution plus standalone lifecycle delivered by ADR-0036/0037; live Compact writes pending |
| [0009](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0009-separate-credential-models-from-serializations.md) Credential model separation | Accepted | §10 | Owned credential core and Midnight CBOR edge adapter delivered by ADR-0038 |
| [0010](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0010-oidc-and-didcomm-as-protocol-adapters.md) Protocol adapters | Accepted | §§7, 11 | Standalone OID4VCI issuance and SIOPv2 DID authentication delivered by ADR-0039/0040; live transport, OpenID4VP presentation, and DIDComm pending |
| [0011](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0011-secure-key-operations-behind-ports.md) Protected key operations | Accepted | §§3, 7, 12–13 | Opaque generation, HD derivation, and signing implemented in development; native adapters pending |
| [0012](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0012-mobile-first-target-priority.md) Mobile-first targets | Accepted | §§1, 4, 12–13, 16 | Features compile; native hosts deferred |
| [0013](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0013-local-first-and-telemetry-off.md) Local-first, telemetry-off | Accepted | §§4–5, 12–13 | Enforced for M0 |
| [0014](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0014-cardano-library-selection.md) Cardano libraries | Proposed | §§8, 17 | Research gate before M1 |
| [0015](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0015-midnight-library-selection.md) Midnight libraries and protocols | Accepted | §§8, 17 | Account model #6, live sync #7, and protected external NIGHT derivation #8 implemented |
| [0016](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0016-ssi-component-selection.md) SSI components | Proposed | §§9–11, 17 | Research gate before M3 |
| [0017](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0017-platform-backed-secret-storage.md) Platform custody, secret blobs, and authorization | Accepted | §§7, 12, 17 and prototype security review | Development generated-root/HD/signing harness implemented; native mobile adapters required |
| [0018](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0018-structured-error-taxonomy.md) Error taxonomy | Proposed | §§10, 13 | Partially exercised by M0 errors |
| [0019](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0019-explicit-application-events-only-when-needed.md) Event model | Proposed | §§3, 13 | No event infrastructure in M0 |
| [0020](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0020-layered-testing-strategy.md) Layered testing | Accepted | §§3, 13, 18 | M0 baseline and coverage gate implemented |
| [0021](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0021-staged-prototype-migration.md) Staged prototype migration | Accepted | §§14, 17–19 and prototype review | Profile lifecycle, presentation shell, and headless harness migrated from immutable baseline |
| [0022](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0022-nix-reproducible-development-and-ci.md) Reproducible Nix environment | Accepted | Repository harness | Implemented locally and in CI |
| [0023](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0023-prioritize-midnight-prototype-parity.md) Prioritize staged Midnight prototype parity | Accepted | Product direction and parity epic | Wallet shell, headless harness, and profile lifecycle implemented; capability backlog open |
| [0024](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0024-versioned-headless-wallet-protocol.md) Versioned headless wallet protocol | Accepted | Prototype parity epic and issues #4/#5/#8 | v1 profiles, protected keys, account derivation/sync, and shutdown implemented |
| [0025](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0025-persist-public-wallet-profile-metadata.md) Persist public profile metadata separately from secrets | Accepted | §§3, 7, 12–13, 17 and issue #1 | JSON profile metadata, selection, restore, UI, and headless flows implemented |
| [0026](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0026-stage-midnight-transfer-authorization.md) Stage Midnight transfer authorization before proving/submission | Accepted | §§3, 7–8, 12–13 and issue #9 | Canonical unshielded NIGHT prepare/authorize/draft flow implemented; proving/submission queued |
| [0027](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0027-complete-standalone-midnight-transaction-submission.md) Complete Midnight submission through bounded standalone adapters | Accepted | §§3, 7–8, 12–13 and issue #11 | Native development/headless DUST, proof-server, and node submission implemented; local path added by ADR-0028 |
| [0028](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0028-keep-midnight-proof-witnesses-local.md) Keep Midnight proof witnesses local by default | Accepted | §§3–5, 7–8, 12–13 and issue #12 | Native local DUST proving and iOS/Android resource harness implemented; production custody remains fail-closed |
| [0029](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0029-expose-standalone-wallet-flows-on-mobile.md) Expose standalone wallet flows on mobile | Accepted | §§3, 6–8, 12–13, 16–18 and issue #14 | Explicit development mobile composition, receive QR, and protected simulated transfer journey implemented; production remains fail-closed |
| [0030](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0030-persist-public-midnight-account-checkpoints.md) Persist public Midnight account checkpoints outside wallet core | Accepted | §§3, 5–8, 12–13, 17–18 and issue #15 | Public unshielded restart/cache/resume implemented; private DUST and shielded state are governed separately by ADR-0031/0033 |
| [0031](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0031-persist-private-midnight-dust-checkpoints.md) Persist private Midnight DUST checkpoints behind live catch-up | Accepted | §§3, 5–8, 12–13, 17–18 and issue #16 | Scoped DUST resume, controlled segmented cold replay, and live-before-spend gate implemented; shielded Zswap is governed by ADR-0033 |
| [0032](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0032-expose-resumable-dust-sync-sessions.md) Expose resumable DUST synchronization as an adapter-owned session | Accepted | §§3, 5–8, 12–13, 16–18 and issue #17 | Native worker, complete/drop-before-fold GraphQL-WebSocket replay/resume/cancel fixtures, partial checkpoints, headless lifecycle, and Assets progress implemented; production custody pending |
| [0033](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0033-keep-shielded-zswap-state-adapter-private.md) Keep shielded Zswap keys and replay state inside the Midnight adapter | Accepted | §§3, 5–8, 12–13, 16–18 and issue #18 | Protected receive address, controlled segmented native replay/worker, private checkpoints, shielded spending, and headless/mobile lifecycle implemented; production custody pending |
| [0034](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0034-expose-safe-transaction-submission-cancellation.md) Expose transaction submission status and safe cancellation | Accepted | §§7–8, 12–13, 16–18 and issue #19 | Adapter-owned pre-broadcast cancel boundary plus headless/mobile status, cancel, and retry implemented; durable follow-up delivered by ADR-0035 |
| [0035](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0035-persist-and-reconcile-midnight-submissions.md) Persist and reconcile Midnight transaction submissions | Accepted | §§3, 5–8, 12–13, 16–18 and issue #20 | Public persist-before-broadcast journal, restart duplicate prevention, finalized reconciliation, and headless/mobile recovery implemented |
| [0036](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0036-resolve-and-retain-public-midnight-dids.md) Resolve and retain public Midnight DIDs | Accepted | §§3–7, 9–13, 16–18 and issue #21 | Identity hexagon, bounded standalone/live resolution, separate public store, headless inventory, and mobile DIDs page implemented |
| [0037](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0037-manage-standalone-midnight-dids-with-opaque-custody.md) Manage standalone Midnight DIDs | Accepted | §§3–7, 9, 12–13, 16–18 and issue #22 | Protected standalone lifecycle/signing, complete update vocabulary, headless flow, and mobile operation builder implemented; live Compact writes pending |
| [0038](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0038-protect-and-verify-profile-scoped-credentials.md) Protect and verify profile-scoped credentials | Accepted | §§3–7, 9, 12–13, 16–18 and issue #23 | Protected standalone inventory, strict phase-1 CBOR verification, headless/mobile flow, and restart restoration implemented; native wrapping and protocol ingress pending |
| [0039](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0039-accept-pre-authorized-openid4vci-offers.md) Accept pre-authorized OpenID4VCI offers | Accepted | §§3–7, 9–13, 16–18 and issue #24 | Final-shape embedded standalone offer, consent, DID proof, verified import, headless/mobile flow, and credential restart restoration implemented; production HTTP/discovery and other grant variants pending |
| [0040](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0040-add-consented-standalone-siopv2-authentication.md) Add consented standalone SIOPv2 DID authentication | Accepted | §§3–7, 9–13, 16–18 and issue #25 | Draft-13 request-by-reference login, consent, managed-DID proof, independent verifier, and headless/mobile flow implemented; OpenID4VP presentation and production transport pending |
| [0041](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0041-protect-format-private-credential-material.md) Protect format-private credential material as opaque bytes | Accepted | §§3–7, 10, 12–13, 16–18 and issue #26 | Bounded opaque material, verified-import propagation, and encrypted schema migration implemented; Digital Passport interpretation/disclosure delivered by ADR-0042 |
| [0042](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0042-bind-digital-passport-disclosure-to-signed-commitments.md) Bind Digital Passport disclosure to signed commitments | Accepted | §§3–7, 9–13, 16–18 and issue #26 | Standalone five-claim issuance, commitment-bound private parts, safe headless planning, local Dioxus reveal, restart/deletion, and mobile smoke coverage implemented; OpenID4VP/proofs deferred |
| [0043](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0043-gate-openid4vp-on-reproducible-compact-proofs.md) Gate OpenID4VP on reproducible Compact proofs | Accepted | §§3–7, 9–13, 16–18 and issues #27/#28 | Strict Final-shaped DCQL request preview, matching, exact consent, and single-use lifecycle implemented; ADR-0050 opens proof/verification only for explicit native headless mode while mobile/live transport remain fail-closed |
| [0044](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0044-compose-reproducible-digital-passport-presentation-artifacts.md) Compose reproducible Digital Passport presentation artifacts | Accepted | §§3–7, 9–13, 16–18, 21 and issue #28 | Immutable artifacts plus exact Rust public-input construction, `MPS1` codec, generated-Compact oracle, and ADR-0048/0049 holder proof implemented; checked ZK execution and independent verification are delivered by ADR-0050 |
| [0045](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0045-preserve-and-verify-detached-midnight-compact-credentials.md) Preserve and verify detached Midnight Compact credentials | Accepted | §§3–7, 9–13, 16–18 and issue #29 | Exact Compact body/proof/private-material lifecycle, native issuance-proof verification, schema-3 encrypted persistence, and headless restart conformance implemented; ADR-0047/0048 bind and reauthorize the standalone holder while issuer anchoring/native custody/presentation proving remain fail-closed |
| [0046](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0046-protect-jubjub-signing-behind-opaque-custody.md) Protect Jubjub signing behind opaque custody | Accepted | §§3, 7, 9–13, 16–18 and issue #29 | Exact development Jubjub generation/signing and headless opaque-reference lifecycle implemented; ADR-0047/0048 bind issuance and reauthorize presentation while native custody and proving remain fail-closed |
| [0047](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0047-bind-standalone-compact-credentials-to-managed-jubjub-did-methods.md) Bind standalone Compact credentials to managed Jubjub DID methods | Accepted | §§3, 5–7, 9–13, 16–18 and issues #27–29 | Standalone DID creation and exact Compact issuance bind a managed Jubjub assertion method; ADR-0048 adds presentation-time re-authorization while native custody, issuer anchoring, and proving remain fail-closed |
| [0048](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0048-reauthorize-compact-holder-methods-at-presentation-time.md) Reauthorize Compact holder methods at presentation time | Accepted | §§3, 5–7, 9–13, 16–18 and issues #27–29 | Standalone preflight requires current protected control of the exact credential-bound method with explicit same-method rotation semantics; ADR-0050 consumes it in native headless proving while native custody/mobile remain fail-closed |
| [0049](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0049-sign-compact-holder-challenges-inside-opaque-custody.md) Sign Compact holder challenges inside opaque custody | Accepted | §§3, 5–7, 9–13, 16–18 and issues #27–29 | Standalone custody constructs and independently verifies the exact credential-family holder `Proof` through a synchronous public-transcript callback; ADR-0050 consumes it in native headless proving |
| [0050](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0050-prove-and-independently-verify-compact-presentations.md) Prove and independently verify Compact presentations | Accepted | §§3–7, 9–13, 16–18, 21 and issues #27–29 | Exact generated-runtime/Rust preimage parity, authenticated offline artifacts, checked proving, bounded `MZP1`, independent verification, headless OpenID4VP success, tamper/replay/restart tests, and resource baselines implemented; ADR-0083 enables only the explicit mobile conformance harness while production remains gated |
| [0051](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0051-isolate-passport-vault-as-product-specific-hexagon.md) Isolate Passport Vault as a product-specific hexagon | Accepted | §§3–7, 9–13, 16–18, prototype vault flow, and issues #2/#31 | Standalone multi-lock domain, exact Digital Passport policy adapter, headless flow, mobile journey, and ADR-0068 durable state implemented; real-node/device evidence remains #31 |
| [0052](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0052-authenticate-and-decode-passport-vault-contract-state.md) Authenticate and decode Passport Vault contract state natively | Accepted | §§3–8, 12–13, 16–18, 21, prototype vault bridge, and issue #31 | Immutable five-circuit Nix closure, exact native tagged-state decoder, and headless fixture implemented; authenticated acquisition and contract-call transactions pending #31 |
| [0053](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0053-distribute-passport-vault-source-from-oxid.md) Distribute the reviewed Passport Vault source from Oxid | Accepted | §§3–8, 12–13, 16–18, 21 and issue #31 | Byte-identical Apache-2.0 contract source and digest assertion replace the private upstream flake input so public CI remains secret-free; generated artifacts stay in Nix |
| [0054](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0054-anchor-passport-vault-indexer-state-to-finality.md) Anchor Passport Vault indexer state to node finality | Accepted | §§3–8, 12–13, 16–18, 21, prototype contract-state query, and issue #31 | Address-scoped finalized-height indexer reads verify the canonical action block and disclose that state bytes remain indexer-supplied; replay/proof authentication and calls remain pending |
| [0055](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0055-replay-canonical-passport-vault-history.md) Replay canonical Passport Vault history before mutation | Accepted | §§3–8, 12–13, 16–18, 21, node transaction/events, and issue #31 | Bounded native verifier, history-complete finalized-node collector, and opt-in authenticated replay reads implemented; cache and calls remain pending |
| [0056](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0056-stage-passport-vault-contract-calls.md) Stage Passport Vault contract calls before proof and submission | Accepted | §§3–8, 12–13, 16–18, 21, prototype vault bridge, and issue #31 | Typed four-operation retained lifecycle, canonical-replay gate, fail-closed composition, and headless protocol implemented; native composer/prover/submission adapter pending |
| [0057](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0057-exercise-passport-vault-calls-in-explicit-simulation.md) Exercise Passport Vault calls in explicit deterministic simulation | Accepted | §§3–8, 12–13, 16–18, 21, prototype vault bridge, and issue #31 | Zero-configuration headless/development composition executes all four retained calls with a distinct simulation-authentication class and explicit non-settlement labels; native adapter pending |
| [0058](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0058-authenticate-passport-vault-call-artifacts.md) Authenticate Passport Vault call artifacts at runtime | Accepted | §§3–8, 12–13, 16–18, 21, prototype vault composer/prover, and issue #31 | Generated client/ABI plus four wallet circuits are authenticated at runtime and exposed through a native resolver; composition, combined DUST proving, funding, and submission remain pending |
| [0059](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0059-isolate-passport-vault-compact-call-composition.md) Isolate Passport Vault generated-Compact call composition | Accepted | §§3–8, 12–13, 16–18, 21, prototype vault composer, and issue #31 | Reproducible closed-schema generated-client composer plus Rust ledger-codec conformance implemented for create/deposit/withdraw; claim custody, port wiring, funding, proving, and submission remain pending |
| [0060](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0060-retain-native-passport-vault-composed-drafts.md) Retain native Passport Vault composed drafts | Accepted | §§3–8, 12–13, 16–18, 21, prototype vault completion flow, and issue #31 | Canonical-replay create/deposit/withdraw composition is retained behind the native application port with zeroizing transaction custody; fresh Midnight context, claim custody, funding, proving, submission, and reconciliation remain pending |
| [0061](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0061-compose-finalized-passport-vault-call-context.md) Compose finalized Passport Vault call context | Accepted | §§3–8, 12–13, 16–18, 21, prototype wallet/indexer context flow, and issue #31 | Complete standalone composition joins exact profile public addresses with replay-matched bounded chain context and enables retained native create/deposit/withdraw drafts; claim, funding, proving, submission, and reconciliation remain pending |
| [0062](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0062-fund-passport-vault-calls-inside-protected-midnight-custody.md) Fund Passport Vault calls inside protected Midnight custody | Accepted | §§3–8, 12–13, 16–18, 21, prototype wallet funding flow, and issue #31 | Exact authorization triggers protected synchronized NIGHT input selection, change, per-input signing, and zeroizing retention for native create/deposit; claim, DUST completion, proving, submission, and reconciliation remain pending |
| [0063](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0063-settle-passport-vault-calls-through-the-midnight-submission-lifecycle.md) Settle Passport Vault calls through the Midnight submission lifecycle | Accepted | §§3–8, 12–13, 16–18, 21, prototype proving/submission flow, and issue #31 | Native create/deposit/withdraw reuse protected DUST proving, persist-before-broadcast, finalized submission, cancellation, and restart reconciliation; protected claim remains pending |
| [0064](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0064-build-passport-vault-claims-from-managed-holder-custody.md) Build Passport Vault claims from managed holder custody | Accepted | §§3–7, 9–13, 16–18, 21, prototype claim composer, and issue #31 | Exact credential/trust/policy checks plus managed holder reauthorization and independently verified fresh-custody proof now produce a zeroizing claim DTO; generated composition and settlement remain pending |
| [0065](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0065-compose-protected-vault-claims-only-after-call-authorization.md) Compose protected vault claims only after call authorization | Accepted | §§3–7, 9–13, 16–18, 21, prototype claim composer, and issue #31 | Exact authorization now gates managed presentation assembly, fixed generated `claimFromLock` composition, funding, and shared settlement; discovery gate delivered by ADR-0066 |
| [0066](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0066-enable-native-vault-claim-discovery-after-managed-conformance.md) Enable native vault claim discovery after managed conformance | Accepted | §§3–7, 9–13, 16–18, 21, standalone issuance/custody/composer flow, and issue #31 | Full managed-DID and holder-bound credential conformance reaches terminal native submission through the packaged composer; `native_settlement` now advertises all four wallet operations |
| [0067](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0067-drive-mobile-vault-settlement-through-typed-application-use-cases.md) Drive mobile vault settlement through typed application use cases | Accepted | §§3–8, 12–13, 16–18, 21, prototype mobile vault surface, and issue #31 | Dioxus reads truthfully labelled contract state and drives all four calls through explicit prepare/authorize/prove/submit/cancel/reconcile stages without WebView or secret-bearing bridges |
| [0068](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0068-persist-the-standalone-passport-vault-ledger.md) Persist the standalone Passport Vault ledger | Accepted | §§3–7, 12–13, 16–18, 21, prototype vault/headless flow, and issues #2/#31 | Owner-private bounded atomic storage preserves standalone accounting and claim replay across headless/mobile restarts without becoming native contract-state authority |
| [0069](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0069-route-native-identity-ingress-through-strict-protocol-links.md) Route native identity ingress through strict protocol links | Accepted | §§3–7, 9–13, 16–18, 21, prototype QR/mobile flow, and issues #2/#32 | Native QR adapters close exact cancellation/denial-or-unavailable/timeout generations and strict shared routing hands only bounded requests to existing preview/consent; physical Android success/cancel/timeout pass, while physical iOS remains #32 |
| [0070](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0070-constrain-mobile-links-and-public-text-export.md) Constrain mobile links and public text export | Accepted | §§3–7, 9–13, 16–18, 21 and issues #2/#32 | Packaged warm/cold custom-scheme links reuse the strict router and typed public receive addresses alone reach native clipboard/share; physical Android custom schemes pass, while verified HTTPS links, discovery, and iOS/resource evidence remain #32 |
| [0071](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0071-wrap-mobile-custody-with-device-user-presence.md) Wrap mobile custody with device user presence | Accepted | §§3, 7, 12–13, 16–18, prototype secret storage, and issues #2/#29/#30 | Normal mobile composition uses a bounded OS-wrapped sealed vault; locked public reads stay non-interactive and Android explicit-authorization/distinct-process/stable-root plus iOS capability/fail-closed simulator evidence exist, while physical-device and mobile-prover gates remain open |
| [0072](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0072-embed-authenticated-compact-artifacts-for-mobile-measurement.md) Embed authenticated Compact artifacts for mobile measurement | Accepted | §§3–7, 9–13, 16–18, 21 and issues #2/#27/#29/#30 | Opt-in native-custody mobile builds embed and authenticate the exact runtime-minimal Nix closure; ADR-0083 adds standalone worker execution while physical-device lifecycle/resource budgets remain gated |
| [0073](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0073-anchor-standalone-compact-credential-policy.md) Anchor standalone Compact credential policy | Accepted | §§3–7, 9–13, 16–18, 21 and issues #2/#29/#34 | Standalone composition resolves and authorizes the exact issuer Jubjub method, enforces current-time/expiry policy, and requires a pinned trust anchor; status remains not checked and production remains unavailable |
| [0074](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0074-package-portable-custody-for-one-shot-recovery.md) Package portable custody for one-shot recovery | Accepted | §§3, 7, 9–13, 16–18, 21 and issues #2/#33 | Versioned Argon2id/XChaCha20-Poly1305 custody packages restore exact keys only into empty development/mobile vaults; ADR-0075 adds native file UX and ADR-0076 composes custody into complete wallet recovery |
| [0075](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0075-transfer-wallet-backups-through-native-document-pickers.md) Transfer wallet backups through native document pickers | Accepted | §§3, 7, 9–13, 16–18, 21 and issues #2/#33 | Capability-selected fixed filenames and bounded iOS/Android document pickers serve complete export/fresh-install recovery plus the legacy custody-only importer; physical-device recovery remains #33 |
| [0076](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0076-recover-complete-wallet-state-as-one-transaction.md) Recover complete wallet state as one transaction | Accepted | §§3–7, 9–13, 16–18, 21 and issues #2/#33 | One-envelope journaled all-store recovery, fresh-install Dioxus UX, exact standalone composition, and iOS/Android virtual-device document round trips are implemented; physical interruption/resource evidence remains #33 |
| [0077](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0077-run-blocking-wallet-work-off-the-dioxus-executor.md) Run blocking wallet work off the Dioxus executor | Accepted | §§3, 6–7, 12–13, 16, 18, prototype mobile worker, and issues #2/#42 | Native synchronous and async-future wallet/SSI work uses an executor-neutral 8 MiB worker; only audited pure parsing, published snapshots, and non-waiting adapter-worker controls remain direct |
| [0078](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0078-harden-complete-wallet-backup-derivation.md) Harden complete-wallet backup derivation | Accepted | §§3, 7, 9–13, 16–18, 21 and issues #2/#33/#48 | New complete-wallet exports use a strict 64 MiB/t=3 Argon2id version-3 envelope; exact version-2 complete-wallet and version-1 custody policies remain readable without attacker-selected KDF work |
| [0079](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0079-spend-shielded-assets-from-current-adapter-private-state.md) Spend shielded assets from current adapter-private state | Accepted | §§3–8, 12–13, 16–18, 21 and issues #2/#59/#91 | Fresh-sync-gated canonical Zswap planning reuses staged authorization, DUST proving, and durable recovery; guarded real standalone finality/adapter-reconstruction/nullifier evidence and the typed protected-DUST registration prerequisite are complete, while funded registration-to-recovery/origination and physical gates remain open |
| [0080](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0080-bound-secret-safe-runtime-diagnostics.md) Bound runtime diagnostics to secret-safe closed codes | Accepted | §§3–7, 12–13, 16–18, 21, prototype diagnostics/worker boundaries, and issues #2/#46/#60 | Bounded process-local closed-code snapshots/reset are composed in headless and Dioxus; DUST/Zswap/transfer/vault worker loss is sanitized and cannot wedge active runtime state; persistent logs and telemetry remain excluded |
| [0081](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0081-clear-android-jni-exceptions-at-the-native-boundary.md) Clear Android JNI exceptions at the native boundary | Accepted | §§3–7, 12–13, 16–18, 21 and issues #2/#41 | All fallible shared-plugin JNI conversions clear pending Java exceptions before returning a payload-free failure; the Android smoke injects a debug-only throw and then completes the standalone wallet journey |
| [0082](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0082-require-explicit-presentation-credential-selection.md) Require explicit presentation credential selection | Accepted | §§3–7, 9–13, 16–18, 21 and issues #2/#64 | Claim-free previews name the issuer and opaque credential reference; Dioxus visibly auto-selects only a sole match and requires an explicit card choice before multi-candidate consent |
| [0083](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0083-run-mobile-compact-proofs-on-a-foreground-worker.md) Run mobile Compact proofs on a foreground worker | Accepted | §§3–7, 9–13, 16–18, 21 and issues #2/#27/#29/#30 | The explicit native-custody artifact build admits one foreground proof, independently verifies it, and safely reports cancel/background/timeout only after worker completion; physical-device release budgets remain open |
| [0084](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0084-enforce-two-layer-ui-design-tokens.md) Enforce two-layer UI design tokens | Accepted | §§1, 3–7, 9, 12–13, 16, 18, 21 and issues #2/#65/#67 | Dark/light brand primitives map into one semantic component vocabulary; fixed safety colors and a repository lint prevent brand drift and raw presentation literals |
| [0085](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0085-centralize-user-facing-machine-labels.md) Centralize user-facing machine labels | Accepted | §§1, 3–7, 9–13, 16, 18, 21 and issues #2/#65/#77 | One Dioxus label/format seam hides unknown values, names capability truth, formats exact assets/dates, and rejects raw machine copy |
| [0086](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0086-compose-the-mobile-shell-with-a-bounded-route-stack.md) Compose the mobile shell with a bounded route stack | Accepted | §§1, 3–7, 12–13, 16, 18; design Phase 1a; issues #2/#65/#78 | Four primary tabs and center Scan use an app-owned root-plus-secondary stack; every migrated page remains reachable without a new router dependency |
| [0087](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0087-compose-home-as-a-safe-read-only-projection.md) Compose Home as a safe read-only projection | Accepted | §§1, 3–7, 12–13, 16, 18; design Phase 1b; issues #2/#65/#79 | Home projects existing safe views and routes to existing flows; Wallet remains operational authority and security/activity copy cannot overclaim |
| [0088](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0088-present-night-transfer-as-a-bounded-send-wizard.md) Present NIGHT transfer as a bounded Send wizard | Accepted | §§3–7, 12–13, 16, 18; design Phase 2a; issues #2/#65/#80 | Two editable screens lead into preview-derived review, separate authorization/submission intents, and only the recovery permitted by the existing nine-state machine |
| [0089](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0089-compose-identity-consent-as-four-question-ceremonies.md) Compose identity consent as four-question ceremonies | Accepted | §§3–7, 9–13, 16, 18, 21; design Phase 2b; issues #2/#27/#65/#81 | OpenID4VP, OpenID4VCI, and SIOPv2 expose truthful WHO → WHAT → FROM → WHY review without changing exact consent intents or protocol authority |
| [0090](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0090-compose-onboarding-backup-receipts-and-account-sync.md) Compose onboarding, backup receipts, and account sync | Accepted | §§3–8, 12–13, 16, 18; design Phase 2c; issues #2/#33/#65/#82 | First launch forks create/restore, complete native export records the sole truthful backup receipt, and Wallet composes account/DUST/shielded refresh into one action without merging authority |
| [0091](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0091-present-receive-as-a-typed-address-sheet.md) Present receive as a typed address sheet | Accepted | §§3, 6–8, 12–13, 16, 18; Receive journey; issues #2/#65/#83 | Home opens a bounded modal over existing safe account and typed public-export ports; only protected returned address rails become selectable QR/Copy/Share destinations |
| [0092](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0092-generate-validated-build-time-brand-packs.md) Generate validated build-time brand packs | Accepted | §§1–6, 12–18, 21; white-label design/Phase 3; issues #2/#65/#84 | Default thin app generates immutable identity/CSS/logo from a strict pack; contrast, safety copy, manifest, and every enumerated Nix pack fail closed |
| [0093](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0093-mask-private-wallet-values-as-a-render-only-ui-profile.md) Mask private wallet values as a render-only UI profile | Accepted | §§1–7, 9–13, 16–18, 21; UI profiles P1–P5/Phase 4a; issues #2/#65/#85 | Every Dioxus build defaults to a process-local mask, permits one timed reveal, re-arms on lifecycle/unlock, and leaves exact consent objects visible |
| [0094](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0094-protect-mobile-snapshots-through-a-boolean-platform-port.md) Protect mobile snapshots through a boolean platform port | Accepted | §§3–7, 12–13, 16–18, 21; UI profile P6/Phase 4a; issues #2/#32/#65/#85 | One payload-free port sets Android `FLAG_SECURE` or an honest iOS background overlay; backup/reveal routes force protection and unsupported targets fail closed |
| [0095](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0095-share-a-closed-capability-manifest-with-developer-ui.md) Share a closed capability manifest with the developer UI | Accepted | §§3–7, 12–13, 16–18, 21; UI profiles P1/P2/P7/P9/P10; issues #2/#65/#69/#87 | One dependency-free manifest feeds headless JSON and an opt-in standalone developer viewer; confirmation metadata, feature guards, and normal-release marker exclusion are checked |
| [0096](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0096-sequence-standalone-fixtures-through-a-demo-drawer.md) Sequence standalone fixtures through a demo drawer | Accepted | §§3–7, 12–13, 16, 18, 21; UI profiles P1/P2/P5/P8–P10; issues #2/#65/#88 | An opt-in standalone-development drawer isolates setup in the named demo profile, sequences only existing safe fixtures, gates simulated funding, and stops offer/login/presentation fixtures at their unchanged review screens |
| [0097](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0097-build-standalone-phone-routes-at-compile-time.md) Build standalone mobile routes at compile time | Accepted | §§3–8, 12–13, 16–18, 21; reviewed prototype route profiles; issues #2/#32/#89 | Separate opt-in development builds use immutable localhost routes for simulators/desktop or local MagicDNS/TLS routes for a phone; production and native-custody composition remain unchanged |
| [0098](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0098-authenticate-deployment-profiles-and-isolate-standalone-funding.md) Authenticate deployment profiles and isolate standalone funding | Accepted | §§3–8, 12–13, 16–18, 21; reviewed prototype live path; issues #2/#90/#91 | Signed atomic Midnight/SSI routes plus node genesis gate are implemented without selecting a deployment; guarded harnesses prove funded unshielded finality/restart and genesis-authority shielded finality/adapter-reconstruction safety |
| [0099](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0099-expose-an-agent-tool-surface-over-mcp.md) Agent tool surface over MCP | Proposed | §§1, 3–7, 11–13, 16, 18, 21 and issues #69/#70 | Dependency-free stdio bridge in apps/oxid-mcp derives its tools from the capability manifest; a test-only workspace dependency proves the composed surface excludes consent, authorization, signing, cancellation, and reconciliation ceremonies; production SDK adoption and the policy engine remain issue #70 |
| [0100](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0100-register-protected-dust-keys-before-fresh-wallet-submission.md) Register protected DUST keys before fresh-wallet submission | Accepted | §§3–8, 12–13, 16–18, 21; prototype onboarding plan; ledger registration semantics; issues #2/#92 | Implemented in repository/headless/Dioxus with guarded public PreProd funding-manifest/read-only-observation foundations and an amount-observed one-output/one-note acceptance harness: a distinct prepare/consent/authorize/submit port registers only current owned NIGHT inputs through protected role-0/role-2 custody; a fresh wallet intentionally starts with zero DUST, and authoritative generated-DUST observation remains a separate spend-readiness gate |
| [0101](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0101-gate-laceid-portal-interoperability-on-final-openid4vci.md) Gate LaceID Portal interoperability on Final OpenID4VCI | Accepted | ADR-0039/0097; issue #124; LaceID Portal `804de0a9e58cf48ece3cc6c24b2245bb70bc80f1` | Source-derived negative contract evidence only: strict Final offer/metadata/request/response boundaries reject the pinned Portal shapes; no live HTTP, trust-manifest consumption, or headless/simulator/device/tailnet interoperability evidence |
| [0102](https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr/0102-admit-pinned-portal-final-in-headless-development.md) Admit pinned Portal Final issuance in headless development | Accepted | ADR-0039/0101; issue #124; Portal PR #17 / `integration@925ec8d` | Strict authenticated native-headless Portal HTTP issuance, exact three-part verification/import, encrypted persistence, and new-process restore; production/mobile Portal HTTP and physical/tailnet evidence remain unavailable |
| [0104](https://github.com/MediaNoxLabs/oxid/blob/a90b0cb5264ae1160736c555c8e1446b5499e362/docs/adr/0104-regrow-incoming-adapters-behind-capability-facades.md) Regrow oversized adapters behind capability façades | Accepted | §§3, 6, 13–14, 18–19; issue #145 | Governing policy and source-size baseline recorded; headless then desktop Dioxus source-module decomposition is first, composition and Midnight remain later, and no crate extraction is authorized |

## Current boundaries

M0 implements ADR-0001 through ADR-0006, the applicable policy portions of
ADR-0011 through ADR-0013, ADR-0020, ADR-0021, and ADR-0022. ADR-0007 through
ADR-0010 constrain future identity/protocol work without claiming it exists.
ADR-0014, ADR-0016, and ADR-0018 through ADR-0019 remain research or design
gates as stated in their individual records. ADR-0015 now binds all Midnight
adapter work to the selected official Git and protocol surfaces. ADR-0017 now binds the M1
custody design without claiming native adapters are complete. ADR-0023
reprioritizes prototype parity after M0 without bypassing those gates. ADR-0024
establishes a safe second incoming adapter for exercising each slice. ADR-0025
makes profile metadata durable without conflating it with protected custody.
Issue #8 composes ADR-0011, ADR-0015, ADR-0017, and ADR-0024 for a
development-only external NIGHT derivation flow; it does not change their
production-custody requirements. ADR-0026 composes those same boundaries into
canonical transaction authorization while keeping proving and submission
truthfully separate. ADR-0028 adds a measured private local-proving mode and
leaves ADR-0027's remote path as an explicit development option.
ADR-0029 exposes those existing account and transaction boundaries through a
separately selected development mobile composition without changing normal
production wiring. ADR-0030 keeps public Midnight replay persistence inside the
native outgoing adapter and requires a successful live catch-up before cached
UTXOs can become spendable inputs. ADR-0031 applies the same live-before-spend
rule to key-specific DUST state while using the official tagged ledger state
behind a distinct private, bounded adapter store. ADR-0032 adds an explicit
adapter-owned session and permits bounded partial checkpoints without weakening
that live-before-spend requirement. ADR-0033 keeps Zswap key use, ownership
replay, and checkpoints inside the native Midnight adapter while
allowing only public shielded addresses and bounded safe projections outward.
ADR-0071 replaces normal mobile's unavailable custody with a device-bound
sealed-vault adapter while retaining an explicit development composition for
simulator parity. It does not turn software multi-curve secrets into Secure
Enclave/StrongBox keys or authorize the still-resource-gated mobile prover.
ADR-0072 selects an executable-embedded, runtime-minimal Compact artifact
package for explicit native-custody mobile measurement builds. The adapter
authenticates that Nix-selected package without runtime discovery or extraction,
and ADR-0083 connects it to a non-UI, single-proof foreground worker only in the
explicit conformance build. Normal mobile remains `proof_unavailable`, and
physical-device budgets plus the remaining ADR-0071 release gate stay open.
ADR-0073 preserves ADR-0045's proof-only constructor for immutable conformance
vectors and gives active standalone composition an explicit issuer resolver,
clock, and pinned trust anchor. Exact DID controller, assertion relationship,
and Jubjub proof-key binding plus issuance/proof/expiry rules must pass before a
new standalone Compact credential is valid. Status stays `not_checked`, and
normal production composition receives none of the standalone trust policy.
ADR-0074 replaces the prototype's live-store JSON copy with a bounded,
profile-bound authenticated custody package. It restores the root and generated
keys only after complete validation and refuses existing destinations. Mobile
export forces a fresh native authorization and recovery uses authorized native
vault initialization. The format is not exposed through headless; explicit OS
document transfer and Dioxus Settings UX are supplied by ADR-0075; atomic
profile/DID/credential association recovery remains issue #33 work.
ADR-0075 keeps arbitrary paths outside the application and UI. A fixed-name,
bounded encrypted package crosses only the native document bridge: iOS uses a
protected, no-backup temporary export plus regular non-symlink copied imports,
and Android uses bounded openable content URIs. Cancellation is explicit,
Settings zeroizes entered recovery-secret state and truthfully labels this as
custody-only, and recovery remains limited to an empty matching profile.
ADR-0076 defines the all-store successor as one profile-scoped authenticated
archive and a custody-last, journaled transaction. Public Midnight associations
retain only network/account/address coordinates, and restored DID control is
reconstructed by unique exact public-key matching rather than persisted opaque
key handles. The single envelope, custody-last coordinator, rollback, and retry
reconciliation are implemented. Settings exports the complete archive,
first-run Dioxus recovers it without a caller-supplied profile, and the
standalone composition round trip verifies exact account, DID, credential, and
custody restoration. Complete mobile picker round trips and physical-device
resource evidence remain issue #33.
ADR-0077 retains the prototype worker's useful Android stack/thread separation
without copying its aggregate secret-bearing message facade. Dioxus must publish
busy state, send owned commands and cloned capability ports to the private 8 MiB
native worker, and apply typed results only after the one-shot returns. Native
authorization, key derivation/signing, Passport Vault call authorization,
backup KDF/recovery, and their surrounding profile/account/DID persistence paths
use that boundary. Encrypted credential inventory/disclosure/deletion,
standalone Vault persistence, submission-history reads, DUST/Zswap start, and
the complete async wallet/DID/VC/protocol/contract futures use it as well.
Worker failures expose one payload-free message; browser in-memory fallback is
not authorization for production Web UI blocking. The completed issue #42
audit leaves direct only strict identity parsing, published sync snapshots,
retained draft/status reads, and non-waiting cancellation signals whose port
contracts explicitly forbid filesystem, transport, custody, or ledger work.
ADR-0078 treats the portable everything-file as a higher-value offline target
than the first custody-only package. New complete-wallet exports use
`OXIDBAK1` version 3 with Argon2id at 65,536 KiB/t=3/p=1. The decoder maps
versions 1, 2, and 3 to exact reviewed policies before allocating KDF work;
version 2 remains read-only compatibility for existing complete-wallet files,
and version 1 remains the legacy custody-only format. Physical-device latency,
peak-memory, low-memory, interruption, and thermal evidence remains issue #33.
ADR-0080 permits useful runtime-health visibility without reversing the
telemetry-off or staged-migration exclusions. Only closed payload-free codes
enter a bounded process-local ring; headless and Dioxus can snapshot/reset it,
but it has no persistence, upload, free-form fields, timestamps, endpoints, or
process measurements and never participates in wallet authority. DUST and
shielded worker panics publish terminal sanitized snapshots; retained Passport
Vault completion unwind always releases its active process reservation.
ADR-0081 makes pending Java-exception cleanup a mandatory part of every failed
Android JNI conversion in the shared native plugin. It discards exception
details, returns the existing closed bridge error, and uses an emulator-only
throw-then-full-wallet smoke without adding a headless or application API.
ADR-0082 preserves the presentation application's exact candidate contract at
the Dioxus boundary. A sole matching credential is visible and preselected;
multiple matches require a radio-card choice before consent, and changing that
choice clears consent. Preview metadata remains bounded to opaque credential
identifier, display name, and issuer with no claim values or proof material.
ADR-0083 connects ADR-0050 proof success only to ADR-0072's explicit native
mobile artifact build. A dedicated worker admits one foreground proof;
profile-scoped cancel, background, and timeout signals cause late-result
disposal and are acknowledged only after the non-interruptible prover stops.
Retry starts from a fresh single-use OpenID4VP preview and consent. Normal
production and ordinary standalone builds remain proof-disabled, while
physical-device custody and resource budgets remain issues #29 and #30.
ADR-0093 keeps secret mode entirely inside the Dioxus incoming adapter. It
changes only rendering of reviewed public view strings, defaults to masked,
uses generation-bound timed reveal, re-arms after lifecycle and wallet-unlock
boundaries, and never masks exact consent or authorization objects. ADR-0094
adds the distinct host snapshot boundary: one boolean platform port sets
Android `FLAG_SECURE` or an iOS scene-background overlay and carries no wallet
value. iOS does not claim screenshot prevention, and physical-device evidence
remains issue #32.
ADR-0095 removes capability discovery from the headless adapter's private JSON
implementation. One dependency-free application manifest now feeds headless
serialization and the opt-in standalone Dioxus developer view. Its closed facts
cannot carry payloads, identifiers, claims, endpoints, logs, or telemetry;
unknown composition labels fail to `unavailable`. The developer profile has a
persistent build banner, cannot compile with normal composition, and a normal
release binary must not contain its marker.
ADR-0096 adds the separate presentation-only demo profile. Its drawer first
selects or creates the named `Oxid Demo Wallet`, leaving unrelated active
profiles untouched, then uses the existing custody, derivation, DID,
credential-inbox, account-sync, and
strict identity-routing boundaries; the funding step admits only the exact
undeployed simulator. Full setup stops at the existing credential-offer review,
and login/presentation fixtures likewise occupy the one-item pending review
without accepting, authorizing, proving, or submitting anything. Production
and native-standalone builds reject the feature, and normal release artifacts
exclude its stable markers.
ADR-0084 makes presentation theming a two-layer build boundary. Components
consume only the fixed semantic surface/text/accent/product/type/space/radius/
motion/elevation vocabulary, while raw palette values remain confined to
complete dark/light brand primitives. Safety-state colors are not brandable,
dark stays the only selected scheme, and the repository gate rejects drift;
this does not alter application truth, consent, or masking policy.
ADR-0034 separates submission-attempt status from retained draft state and
permits cancellation only before the adapter atomically enters broadcast.
ADR-0035 makes that boundary durable, restores safe public outcomes after a
restart, and permits replacement only after finalized rejection or expiry.
ADR-0036 begins the peer identity capability with validated public DID
resolution and profile-scoped inventory while keeping lifecycle mutation,
credentials, endpoints, and production-native storage outside the slice.
ADR-0037 adds the full development-only standalone lifecycle through opaque
custody handles while keeping restored records public-only and live Compact
mutation fail-closed.
ADR-0038 adds a peer credential hexagon, private original-byte boundary,
structured seven-stage verification, strict Midnight CBOR proof adapter, and
authenticated standalone persistence while keeping production native wrapping
fail-closed. ADR-0039 adds a protocol-neutral issuance hexagon and exact
OpenID4VCI 1.0 Final pre-authorized subset through an in-process deterministic
adapter; production transport/discovery and unsupported protocol variants
remain fail-closed. ADR-0040 separately adds the prototype's self-issued
login-with-DID behavior as a pinned SIOPv2 draft-13 standalone profile. It does
not claim OpenID4VP credential presentation: `vp_token`, DCQL, disclosure, and
live verifier transport remain fail-closed. ADR-0041 adds an atomic protected
route for format-private credential material without exposing or interpreting
claims in core; the Digital Passport adapter and local disclosure preview
are delivered by ADR-0042. ADR-0042 requires the adapter to recompute every
official Midnight commitment and signed claim root before exposing candidates,
keeps local reveal out of headless, and labels preview as non-presenting;
OpenID4VP and Compact proof generation remain fail-closed in normal/mobile
composition. ADR-0043 adds a strict OpenID4VP/DCQL request, consent, and session
boundary and requires a reproducible Compact proof plus independent verification
before any `vp_token` can exist. ADR-0044 delivers the reproducible final Compact composition and
authenticated artifact baseline and now exact public-statement construction,
portable `MPS1` round-trip, generated-Compact conformance, and independent
preflight reconstruction. ADR-0045
separately replaces standalone issuance's synthetic Digital Passport with the
prototype's exact Compact body, detached issuer proof, and private openings. It
verifies and persists that issuance bundle without confusing it with a
presentation proof. ADR-0073 now supplies the separate active-standalone issuer,
temporal, and trust policy; status and production trust remain explicit gates.
ADR-0048 separately requires current protected control of the credential-bound
holder method before proof execution, permits rotation only while preserving
the exact method identifier and assertion relationship, and keeps the generic
DID custody signature distinct from the credential-family presentation proof.
ADR-0049 now supplies that distinct presentation proof through an atomic
two-step Jubjub operation: wallet custody retains the key and nonce,
`did-midnight` binds the managed method, and `vc-midnight` owns and independently
checks the exact credential-family transcript. Those inputs are consumed by
ADR-0050's explicit native headless prover and verifier.
ADR-0050 governs the remaining native ZK boundary: the Rust preimage must match
generated Compact byte-for-byte, the Nix closure must carry every authenticated
offline artifact including p18 parameters, checked proving and independent
verification remain separate, and only a canonical public `MZP1` envelope may
cross the protocol boundary. That headless path now succeeds and rejects
tamper/replay/restart failures. Production/mobile composition stays fail-closed
until its packaging, resource, and native-custody gates are accepted.
ADR-0051 keeps the Passport Vault product separate from generic wallet and
credential core. Its standalone adapter exercises exact policy, accounting,
confirmation, and replay behavior but is visibly process-local and never claims
chain submission. ADR-0052 authenticates the five-circuit Compact closure and
adds exact native decoding of bounded tagged contract state without claiming
that caller-supplied bytes are live or fresh. Production operations remain
unavailable until issue #31 authenticates state acquisition and composes
contract calls through the existing Midnight submission and reconciliation
boundary.

ADR-0053 supersedes only ADR-0052's private companion-repository flake input.
Oxid distributes the byte-identical reviewed Compact source and asserts its
upstream digest before building; public CI needs no private repository token,
while generated clients, IR, parameters, and proving keys remain Nix outputs.
ADR-0054 separates finality anchoring from state authentication. The standalone
native source queries at a node-finalized height and verifies the action block's
canonical hash, while every view explicitly says the indexer-supplied state is
not proven. Those snapshots remain read-only until deterministic replay or a
reviewed storage proof authenticates the state bytes.
ADR-0055 selects deterministic replay and implements its pure native verifier.
Its finalized-node collector observes every canonical block from the validated
deployment, and the opt-in composed source now exposes authenticated replay
reads. ADR-0054's indexer snapshot remains explicitly unproven. ADR-0056 stages
the four user-facing Passport Vault operations behind a typed retained draft,
separate authorization and submission intents, authenticated-replay admission,
and the existing cancellation/reconciliation model. The headless boundary is
implemented. ADR-0057 wires it only in explicit zero-configuration development
composition with a distinct simulation-authentication class and
`settlesOnMidnight: false`; live and production composition still fail closed
until a fresh Midnight context, combined prover, funding, submission, and
durable public-journal adapter are composed. ADR-0058 authenticates the exact
generated client and exposes only the four wallet circuits through native
Midnight resolver traits; it deliberately excludes the administrative circuit
and does not change those live capability labels. ADR-0059 adds a separately
packaged one-request generated-client composer for typed create/deposit/withdraw
operations. It is not an incoming API, rejects claim and administration, and
does not change those labels before completion/submission is wired. ADR-0060
installs it behind a native retained-port adapter using only a fresh bounded
public context source. It requires real serialized Zswap state and ledger
parameters, zeroizes the retained transaction, and keeps submit unavailable;
the composition root remains `native_pending` until the Midnight stack supplies
that context plus funding, proving, durable submission, and reconciliation.
ADR-0061 supplies the public-context boundary in complete standalone
composition. The Midnight adapter alone decodes profile-scoped public address
payloads; the Passport Vault source retains bounded Zswap state and current
ledger parameters only beside an exact node-anchored action; canonical replay
must match its state and action byte-for-byte. The composition root joins those
ports. ADR-0062 adds the next protected boundary: only exact authorization may
select synchronized unshielded NIGHT inputs, return change, and sign once per
input inside Midnight custody. Complete standalone composition now reports
`native_funded_draft` when the packaged composer is present. Claim, DUST
completion, proving, submission, and reconciliation remain closed, so
`settlesOnMidnight` remains false.
ADR-0063 reuses the ordinary Midnight DUST, proving, persist-before-broadcast,
submission, cancellation, and finalized reconciliation machinery through a
composition-only contract-call port. The shared public journal adds finalized
block height while keeping schema-one reads compatible and namespaces Passport
Vault records away from transfer history. Complete standalone native
create/deposit/withdraw calls now report `native_settlement` and
`settlesOnMidnight: true`; protected claim remains closed.
ADR-0064 now prepares that claim's sensitive credential and presentation
material inside `vc-midnight`: exact stored credential verification,
contract-pinned issuer hashing, finalized-time policy checks, current managed
holder reauthorization, custody-owned fresh proof signing, independent proof
verification, and zeroizing fixed-shape output. It deliberately leaves the
public claim capability closed until the authenticated generated composer and
existing settlement lifecycle consume that output.
ADR-0065 consumes that protected output only after the existing exact
`AUTHORIZE_PASSPORT_VAULT_CALL` confirmation. Preparation binds a public plan
to byte-exact policy and trust decoded from canonical replay plus its finalized
timestamp; authorization then performs managed presentation assembly,
one-request generated `claimFromLock` composition, funding, and the shared
settlement lifecycle. Concurrent authorization and stale/invalid plans fail
closed, failures retain no presentation, and public claim discovery remains
off until a complete managed-custody generated-client settlement conformance
test passes. ADR-0066 supplies that evidence using the actual standalone
OpenID4VCI issuer, managed Jubjub DID custody, protected credential repository,
authenticated claim-ready ledger state, packaged generated client, Midnight
funding, and terminal completion port. Native settlement discovery therefore
advertises `claim_from_lock`; deterministic completion remains explicitly test
only, while real-node and mobile live-call fixtures stay on the backlog.
ADR-0067 connects those same typed state/call/recovery services to Dioxus. The
mobile page keeps the standalone conformance ledger visibly separate from
`deterministic_simulation` and `native_settlement`, requires exact review and
authorization before proving/submission, and never blind-retries an ambiguous
broadcast. Native development builds select the same environment-aware
standalone composition as headless; device resource baselines and real-node
fixtures remain pending.
ADR-0068 replaces only the standalone product repository with a separately
bounded owner-private atomic file. Complete domain snapshot validation preserves
accounting and credential-root replay rejection across process restarts;
capability/UI copy keeps that persistence distinct from simulated or native
contract settlement. The file cannot source or authenticate a Midnight call.
ADR-0069 separates camera capture from protocol classification. Native iOS and
Android adapters return one bounded opaque QR value to a strict shared router;
standalone `openid4vp` routes require exact registered endpoint pairs and
unknown pairs fail closed. Scanning only navigates to the existing preview and
consent surfaces. ADR-0070 adds OS link delivery through the same router and a
separate typed public-address export boundary. Production endpoint discovery,
verified HTTPS links, physical iOS camera evidence, and resource baselines
remain issue #32; physical Android QR and custom-scheme evidence is complete
for locally supported paths.
ADR-0085 makes product copy a Dioxus-owned truth boundary. Serialized states,
modes, sources, authentication classes, formats, and reason codes must cross
one reviewed label module; unknown values are hidden rather than echoed. Exact
NIGHT/DUST amounts and UTC dates replace internal subunit/timestamp copy, while
sync cursors remain adapter state instead of user vocabulary. This changes no
application transition, consent, capability, or settlement authority.
ADR-0086 replaces the prototype-derived flat destination signal with a bounded
Dioxus route stack. Home, Wallet, Documents, and Activity are the only primary
destinations; Scan remains a classified action, while Vault, DID management,
Settings, Diagnostics, and profiles are explicit secondary routes. Selecting a
primary clears the secondary stack and Back only pops presentation state. Home
and Wallet intentionally share the complete account view until Phase 1b splits
their presentation; no application state machine or adapter boundary moves.
ADR-0087 completes that split with a Dioxus-only read projection over existing
account, security, shielded-sync, credential, and Vault use cases. Home routes
actions to the existing operational surfaces and keeps optional failures
payload-free. Backup completion, biometric enrollment, identity/Vault activity,
secret masking, and all state-changing ceremonies remain outside this decision.
ADR-0088 presents the existing protected public/shielded transfer lifecycle as
two bounded editable screens followed by exact review, confirmation, and
status. It preserves separate authorization and prove/submit intents, safe
pre-broadcast cancellation, durable unknown-outcome reconciliation, and the
nine-state application machine. Clipboard import, payment scanning, and recent
recipients remain absent until reviewed ports exist.
ADR-0089 composes the existing OpenID4VP, OpenID4VCI, and SIOPv2 public plans as
ordered WHO, WHAT, FROM, and WHY consent ceremonies. Standalone endpoints are
explicitly unverified, required presentation claims remain locked to the exact
prepared plan, the supported age predicate includes negative reassurance, and
the existing literal confirmations, exact intents, credential chooser,
managed-DID custody, refusal, replay, verification, and fail-closed proof gates
remain authoritative. Verified endpoint discovery and optional claim selection
remain absent until dedicated trust and authorization ports exist.
ADR-0090 completes the next mobile ceremony slice: first launch forks cleanly
between create and restore, profile creation offers truthful skippable device
protection without exposing its opaque id, and Wallet presents public account,
DUST, and shielded refresh through one action without merging their authority.
A new profile-scoped timestamp-only application receipt is recorded only after
complete backup encryption and the fixed-kind native document export both
succeed; it is excluded from archives and is the sole authority for **Backed
up** copy and celebration.

ADR-0091 presents Receive as a Dioxus-owned non-primary sheet over the existing
safe account view and typed public export ports. Home remains the root under the
modal; only protected derived address rails returned by the account use case
become selectable Public/Private/Fee destinations, and the selected full value
alone feeds QR, Copy, and Share. Fixture/watch-only addresses fail closed to a
Wallet activation path, while the existing headless address methods remain the
UI-independent conformance harness.

ADR-0092 moves brand identity out of shared Dioxus source and into immutable
build-time packs selected by thin app crates. Closed metadata/token schemas,
bounded safe SVG, two-scheme contrast, exact manifest purpose templates, fixed
safety colors, and security-copy snapshots fail closed; generated semantic CSS
and `BrandProfile` are the only UI inputs. Nix and `run.sh` enumerate every
pack, while runtime brand selection and brand-controlled capability/security
semantics remain forbidden.

ADR-0097 replaces the prototype's runtime-selected localhost/personal-Tailscale
entries with separate compile-time-only development profiles. iOS Simulator
uses immutable loopback routes directly; Android emulator uses exact `adb
reverse` mappings so ADR-0027 still sees the prover as loopback. The phone
profile exposes repository-owned loopback services through temporary
TLS-terminated Tailscale Serve routes. The normal release gate rejects both
profile markers and the localhost endpoints, native custody cannot select
either, and verified public app links remain a separate open gate.

ADR-0098 adds a signed, canonical, audience/validity/sequence-bound deployment
profile that atomically carries Midnight chain identity/routes and SSI metadata
routes. Composition additionally requires the signed node to expose the exact
genesis hash, while normal composition stays unavailable and no production
trust root/profile is selected. A separate double-opt-in test harness accepts
an out-of-band zeroizing standalone funding root and proves exact unshielded
finality, public-journal restart reconciliation, bounded indexer convergence,
and no duplicate recipient delivery. The working prototype's millisecond chain
time and sparse global DUST cursors are retained behind Oxid's strict bounds;
the genesis fixture itself is not copied.

ADR-0100 accepts a distinct protected DUST-registration capability instead of
hiding registration in transfer or sync. Preparation requires live indexer
creation-time and unregistered-state evidence, conserves each selected NIGHT
input to its owner, and permits only the largest generated candidate in the
guaranteed offer to fund the maximum registration fee allowance. Exact consent
then binds role-0 authorization while the role-2 secret remains in protected
custody; generic proving, a registration-separated public journal, finality,
and later authoritative DUST-event observation remain separate stages. The
repository/headless/Dioxus path, public PreProd manifest/read-only observer,
test-only signed Midnight profile, and amount-observed ignored live harness are
implemented; the funded write, mobile, restart, and physical-device acceptance
evidence remain in progress.

ADR-0104 governs behavior-preserving source decomposition of the four oversized
crate roots without changing their dependency or public capability boundaries.
Headless and desktop Dioxus move first behind stable crate-root re-exports;
composition and Midnight retain that later order. Source modules must establish
cohesion before a separately approved, measured crate boundary can be proposed.
