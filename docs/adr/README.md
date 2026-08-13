# Architecture decision records

ADRs record consequential technical decisions for Oxid. Use four-digit,
monotonically increasing filenames. Each record describes its status, context,
decision, consequences, and any superseded decisions.

Accepted ADRs are changed only by a later ADR that explicitly supersedes them.
The root blueprint remains the broader product and engineering constitution.

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
| [0001](0001-modular-hexagonal-architecture.md) Modular hexagonal architecture | Accepted | §§3, 6 | M0 dependency graph implemented and checked |
| [0002](0002-dioxus-as-incoming-adapter.md) Dioxus incoming adapter | Accepted | §§3, 6 | M0 UI and composition implemented |
| [0003](0003-oxid-owned-domain-types.md) Oxid-owned domain types | Accepted | §§3, 6, 18 | Enforced in M0 core APIs |
| [0004](0004-capability-specific-ports.md) Capability-specific ports | Accepted | §§3, 7 | Initial wallet/platform ports implemented |
| [0005](0005-static-adapter-composition-for-mvp.md) Static adapter composition | Accepted | §§5, 6 | Implemented in `crates/composition` |
| [0006](0006-rust-first-controlled-edge-fallbacks.md) Rust-first edge policy | Accepted | §§3, 4 | Enforced; no foreign runtime fallback in M0 |
| [0007](0007-identity-is-a-peer-capability.md) Identity as a peer capability | Accepted | §§1, 4–6 | DID lifecycle and protected credential inventory delivered by ADR-0036–0038 |
| [0008](0008-did-methods-as-capability-negotiated-adapters.md) DID adapters | Accepted | §§7, 9 | Resolution plus standalone lifecycle delivered by ADR-0036/0037; live Compact writes pending |
| [0009](0009-separate-credential-models-from-serializations.md) Credential model separation | Accepted | §10 | Owned credential core and Midnight CBOR edge adapter delivered by ADR-0038 |
| [0010](0010-oidc-and-didcomm-as-protocol-adapters.md) Protocol adapters | Accepted | §§7, 11 | Standalone OID4VCI issuance and SIOPv2 DID authentication delivered by ADR-0039/0040; live transport, OpenID4VP presentation, and DIDComm pending |
| [0011](0011-secure-key-operations-behind-ports.md) Protected key operations | Accepted | §§3, 7, 12–13 | Opaque generation, HD derivation, and signing implemented in development; native adapters pending |
| [0012](0012-mobile-first-target-priority.md) Mobile-first targets | Accepted | §§1, 4, 12–13, 16 | Features compile; native hosts deferred |
| [0013](0013-local-first-and-telemetry-off.md) Local-first, telemetry-off | Accepted | §§4–5, 12–13 | Enforced for M0 |
| [0014](0014-cardano-library-selection.md) Cardano libraries | Proposed | §§8, 17 | Research gate before M1 |
| [0015](0015-midnight-library-selection.md) Midnight libraries and protocols | Accepted | §§8, 17 | Account model #6, live sync #7, and protected external NIGHT derivation #8 implemented |
| [0016](0016-ssi-component-selection.md) SSI components | Proposed | §§9–11, 17 | Research gate before M3 |
| [0017](0017-platform-backed-secret-storage.md) Platform custody, secret blobs, and authorization | Accepted | §§7, 12, 17 and prototype security review | Development generated-root/HD/signing harness implemented; native mobile adapters required |
| [0018](0018-structured-error-taxonomy.md) Error taxonomy | Proposed | §§10, 13 | Partially exercised by M0 errors |
| [0019](0019-explicit-application-events-only-when-needed.md) Event model | Proposed | §§3, 13 | No event infrastructure in M0 |
| [0020](0020-layered-testing-strategy.md) Layered testing | Accepted | §§3, 13, 18 | M0 baseline and coverage gate implemented |
| [0021](0021-staged-prototype-migration.md) Staged prototype migration | Accepted | §§14, 17–19 and prototype review | Profile lifecycle, presentation shell, and headless harness migrated from immutable baseline |
| [0022](0022-nix-reproducible-development-and-ci.md) Reproducible Nix environment | Accepted | Repository harness | Implemented locally and in CI |
| [0023](0023-prioritize-midnight-prototype-parity.md) Prioritize staged Midnight prototype parity | Accepted | Product direction and parity epic | Wallet shell, headless harness, and profile lifecycle implemented; capability backlog open |
| [0024](0024-versioned-headless-wallet-protocol.md) Versioned headless wallet protocol | Accepted | Prototype parity epic and issues #4/#5/#8 | v1 profiles, protected keys, account derivation/sync, and shutdown implemented |
| [0025](0025-persist-public-wallet-profile-metadata.md) Persist public profile metadata separately from secrets | Accepted | §§3, 7, 12–13, 17 and issue #1 | JSON profile metadata, selection, restore, UI, and headless flows implemented |
| [0026](0026-stage-midnight-transfer-authorization.md) Stage Midnight transfer authorization before proving/submission | Accepted | §§3, 7–8, 12–13 and issue #9 | Canonical unshielded NIGHT prepare/authorize/draft flow implemented; proving/submission queued |
| [0027](0027-complete-standalone-midnight-transaction-submission.md) Complete Midnight submission through bounded standalone adapters | Accepted | §§3, 7–8, 12–13 and issue #11 | Native development/headless DUST, proof-server, and node submission implemented; local path added by ADR-0028 |
| [0028](0028-keep-midnight-proof-witnesses-local.md) Keep Midnight proof witnesses local by default | Accepted | §§3–5, 7–8, 12–13 and issue #12 | Native local DUST proving and iOS/Android resource harness implemented; production custody remains fail-closed |
| [0029](0029-expose-standalone-wallet-flows-on-mobile.md) Expose standalone wallet flows on mobile | Accepted | §§3, 6–8, 12–13, 16–18 and issue #14 | Explicit development mobile composition, receive QR, and protected simulated transfer journey implemented; production remains fail-closed |
| [0030](0030-persist-public-midnight-account-checkpoints.md) Persist public Midnight account checkpoints outside wallet core | Accepted | §§3, 5–8, 12–13, 17–18 and issue #15 | Public unshielded restart/cache/resume implemented; private DUST and shielded state are governed separately by ADR-0031/0033 |
| [0031](0031-persist-private-midnight-dust-checkpoints.md) Persist private Midnight DUST checkpoints behind live catch-up | Accepted | §§3, 5–8, 12–13, 17–18 and issue #16 | Scoped DUST resume, bounded incremental replay, and live-before-spend gate implemented; shielded Zswap is governed by ADR-0033 |
| [0032](0032-expose-resumable-dust-sync-sessions.md) Expose resumable DUST synchronization as an adapter-owned session | Accepted | §§3, 5–8, 12–13, 16–18 and issue #17 | Native worker, partial checkpoints, headless lifecycle, and Assets progress implemented; production custody pending |
| [0033](0033-keep-shielded-zswap-state-adapter-private.md) Keep shielded Zswap keys and replay state inside the Midnight adapter | Accepted | §§3, 5–8, 12–13, 16–18 and issue #18 | Protected receive address, bounded native replay/worker, private checkpoints, and headless/mobile lifecycle implemented; spending and production custody pending |
| [0034](0034-expose-safe-transaction-submission-cancellation.md) Expose transaction submission status and safe cancellation | Accepted | §§7–8, 12–13, 16–18 and issue #19 | Adapter-owned pre-broadcast cancel boundary plus headless/mobile status, cancel, and retry implemented; durable follow-up delivered by ADR-0035 |
| [0035](0035-persist-and-reconcile-midnight-submissions.md) Persist and reconcile Midnight transaction submissions | Accepted | §§3, 5–8, 12–13, 16–18 and issue #20 | Public persist-before-broadcast journal, restart duplicate prevention, finalized reconciliation, and headless/mobile recovery implemented |
| [0036](0036-resolve-and-retain-public-midnight-dids.md) Resolve and retain public Midnight DIDs | Accepted | §§3–7, 9–13, 16–18 and issue #21 | Identity hexagon, bounded standalone/live resolution, separate public store, headless inventory, and mobile DIDs page implemented |
| [0037](0037-manage-standalone-midnight-dids-with-opaque-custody.md) Manage standalone Midnight DIDs | Accepted | §§3–7, 9, 12–13, 16–18 and issue #22 | Protected standalone lifecycle/signing, complete update vocabulary, headless flow, and mobile operation builder implemented; live Compact writes pending |
| [0038](0038-protect-and-verify-profile-scoped-credentials.md) Protect and verify profile-scoped credentials | Accepted | §§3–7, 9, 12–13, 16–18 and issue #23 | Protected standalone inventory, strict phase-1 CBOR verification, headless/mobile flow, and restart restoration implemented; native wrapping and protocol ingress pending |
| [0039](0039-accept-pre-authorized-openid4vci-offers.md) Accept pre-authorized OpenID4VCI offers | Accepted | §§3–7, 9–13, 16–18 and issue #24 | Final-shape embedded standalone offer, consent, DID proof, verified import, headless/mobile flow, and credential restart restoration implemented; production HTTP/discovery and other grant variants pending |
| [0040](0040-add-consented-standalone-siopv2-authentication.md) Add consented standalone SIOPv2 DID authentication | Accepted | §§3–7, 9–13, 16–18 and issue #25 | Draft-13 request-by-reference login, consent, managed-DID proof, independent verifier, and headless/mobile flow implemented; OpenID4VP presentation and production transport pending |
| [0041](0041-protect-format-private-credential-material.md) Protect format-private credential material as opaque bytes | Accepted | §§3–7, 10, 12–13, 16–18 and issue #26 | Bounded opaque material, verified-import propagation, and encrypted schema migration implemented; Digital Passport interpretation/disclosure delivered by ADR-0042 |
| [0042](0042-bind-digital-passport-disclosure-to-signed-commitments.md) Bind Digital Passport disclosure to signed commitments | Accepted | §§3–7, 9–13, 16–18 and issue #26 | Standalone five-claim issuance, commitment-bound private parts, safe headless planning, local Dioxus reveal, restart/deletion, and mobile smoke coverage implemented; OpenID4VP/proofs deferred |
| [0043](0043-gate-openid4vp-on-reproducible-compact-proofs.md) Gate OpenID4VP on reproducible Compact proofs | Accepted | §§3–7, 9–13, 16–18 and issues #27/#28 | Strict Final-shaped DCQL request preview, matching, exact consent, single-use headless/mobile lifecycle implemented; proof, verifier response, and `vp_token` remain fail-closed |
| [0044](0044-compose-reproducible-digital-passport-presentation-artifacts.md) Compose reproducible Digital Passport presentation artifacts | Accepted | §§3–7, 9–13, 16–18, 21 and issue #28 | Immutable source/toolchain plus real prover/verifier artifact generation and digest manifest implemented; runtime proof, independent verification, tamper vectors, and `vp_token` remain fail-closed |
| [0045](0045-preserve-and-verify-detached-midnight-compact-credentials.md) Preserve and verify detached Midnight Compact credentials | Accepted | §§3–7, 9–13, 16–18 and issue #29 | Exact Compact body/proof/private-material lifecycle, native issuance-proof verification, schema-3 encrypted persistence, and headless restart conformance implemented; issuer anchoring, holder custody, and presentation proving remain fail-closed |

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
OpenID4VP and Compact proof generation remain fail-closed. ADR-0043 adds a
strict OpenID4VP/DCQL request, consent, and session boundary, but requires a
reproducible Compact proof plus independent verification before any `vp_token`
can exist. ADR-0044 delivers the reproducible final Compact composition and
authenticated artifact baseline without changing that runtime gate: proof
execution, exact public-statement reconstruction, independent verification,
tamper coverage, and response construction are still required. ADR-0045
separately replaces standalone issuance's synthetic Digital Passport with the
prototype's exact Compact body, detached issuer proof, and private openings. It
verifies and persists that issuance bundle without confusing it with a
presentation proof or claiming issuer trust; protected holder Jubjub custody
and issuer-method anchoring remain explicit gates.
