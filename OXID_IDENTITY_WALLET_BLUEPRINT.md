# Oxid Identity Wallet — Development Blueprint

> Source-of-truth starter document for Codex and human contributors.

## 1. Vision

Oxid is a free and open-source, Rust-first, identity-native, cross-chain wallet platform.

**Tier 1:** Android and iOS.
**Tier 2:** desktop and web.

A wallet should be a user-controlled capability container for money, identity, credentials and consent—not a proprietary frontend tied to a single blockchain.

## 2. Mission

Build a secure, minimal and reusable wallet foundation that does not couple product logic to one blockchain, DID method, credential format, communication protocol, storage engine, or UI runtime.

Oxid should work both as an end-user wallet and as a white-box foundation for downstream crypto and identity products.

## 3. Engineering constitution for Codex

1. Use modular hexagonal architecture.
2. Domain/application code MUST NOT depend on Dioxus, chain SDKs, SSI SDKs, databases, HTTP clients, OS APIs, or JS/WASM libraries.
3. Oxid owns its public domain types. Third-party SDK types are mapped at adapter boundaries.
4. Prefer small capability ports over god interfaces.
5. Dioxus is an incoming adapter. UI invokes application use cases; it never calls chain/SSI/storage SDKs directly.
6. Prefer Rust-native implementations. WASM/JS is allowed only behind an adapter when no adequate Rust implementation exists.
7. Raw private keys/seeds must not flow through ordinary UI/application DTOs. Use opaque key references and key-operation ports.
8. Production mobile storage must use platform-backed protection where practical.
9. No telemetry by default. Never log secrets, credential claims, private identifiers, or signing material.
10. Core use cases must be testable without UI/network/OS services. Adapters require contract/integration tests.
11. Architectural changes require an ADR.
12. Avoid speculative abstractions; add capabilities for concrete use cases.

## 4. Product principles

- User custody and explicit consent.
- Mobile-first.
- Rust-first; WASM only at controlled edges.
- Identity and crypto are peer capabilities.
- Open standards over proprietary protocols.
- Replaceable adapters and infrastructure.
- Privacy/local-first.
- Minimum disclosure.
- Auditable, deterministic core logic.
- Pragmatic interoperability over maximal protocol coverage.
- White-label/white-box friendly.

## 5. Business requirements

### Users
- End users managing crypto assets and credentials.
- Developers building white-label wallets.
- Identity issuers/verifiers integrating standards-based wallet flows.
- Blockchain/identity ecosystems needing an OSS reference wallet.

### Goals
- Reusable OSS wallet core.
- Practical Cardano + Midnight support.
- Standards-based identity interoperability.
- Stable extension ports.
- Small, auditable trusted core.
- No mandatory Oxid-hosted backend.

### MVP non-goals
- Every blockchain.
- Exchange/custodial banking.
- Every DID/VC protocol profile.
- Runtime-loaded native plugins on mobile.

## 6. Architecture

Oxid uses **modular hexagonal architecture**.

```text
          Incoming adapters
   Dioxus | QR | deep links | tests
                 |
                 v
+--------------------------------------+
|       APPLICATION / USE CASES        |
| wallet | chain | DID | VC | VP      |
+--------------------------------------+
|             DOMAIN MODEL             |
+--------------------------------------+
                 ^
                 | ports
                 |
+--------------------------------------+
|          OUTGOING ADAPTERS           |
| Cardano | Midnight | DID methods     |
| VC formats | OIDC | DIDComm          |
| storage | platform | resolver/indexer|
+--------------------------------------+
```

### Bounded modules
- **Wallet** — lifecycle, profiles, accounts, recovery references.
- **Chain** — chain-neutral account and transaction semantics.
- **Identity** — DIDs, DID URLs/Documents, verification relationships.
- **Credential** — credential envelopes, metadata, verification/status/storage.
- **Presentation** — requests, candidate selection, disclosure and consent.
- **Protocol** — maps OIDC/DIDComm wire models to use cases.
- **Platform** — secure storage, biometrics, QR, links, files/background work.

### Dependency rule
Dependencies point inward. Core modules never depend on adapter crates. External SDK models are converted at boundaries.

### Dioxus
Dioxus is the primary incoming UI adapter, not the application architecture. Components render state and emit intents/use-case commands. Platform-specific APIs remain behind ports.

### Composition
MVP uses statically linked Cargo adapter crates registered at a composition root. Runtime native plugin loading is deferred.

## 7. Capability ports

### Chain
- `AccountDiscoveryPort`
- `AccountDerivationPort`
- `BalanceQueryPort`
- `AssetMetadataPort`
- `TransactionBuilderPort`
- `TransactionSignerPort`
- `TransactionSubmissionPort`
- `TransactionHistoryPort`
- `FeeEstimationPort`
- `ChainSyncPort`

### DID
- `DidCreatePort`
- `DidUpdatePort`
- `DidDeactivatePort`
- `DidResolutionPort`
- `VerificationMethodPort`

### Credential/presentation
- `CredentialCodecPort`
- `CredentialVerificationPort`
- `CredentialStorePort`
- `StatusResolutionPort`
- `PresentationBuildPort`
- `PresentationVerifyPort`
- `DisclosurePlanningPort`

### Protocol
- `Oid4vciWalletPort`
- `Oid4vpWalletPort`
- `Siop2WalletPort`
- `DidcommPackPort`
- `DidcommTransportPort`
- `ProtocolLinkPort`

### Platform/security
- `SecretStoragePort`
- `KeyOperationPort`
- `BiometricAuthPort`
- `QrScannerPort`
- `DeepLinkPort`
- `FilePort`
- `ClockPort`
- `RandomPort`

Capability negotiation is mandatory: not every DID method supports update/deactivate and not every credential format supports selective disclosure.

## 8. Chain support

Initial chains:
- Cardano
- Midnight

Required wallet capabilities:
- create/import wallet;
- account discovery/derivation;
- addresses;
- balances/assets;
- transaction history;
- fee estimation;
- build/review/sign/submit;
- send/receive;
- network selection.

Other chains are future adapters.

Implementation research should evaluate maintained Rust Cardano crates (including Pallas-family capabilities) and official/maintained Midnight Rust/indexer interfaces. Selection must consider maintenance, license, security/audit status, Android/iOS/WASM support and API stability.

## 9. DID support

Prioritized methods:
- `did:key`
- `did:peer` numalgo 0/1/2/3/4 where specifications and interoperable implementations permit
- `did:web`
- `did:webvh`
- `did:prism`
- `did:midnight`

DID capabilities are method-specific: create, resolve, update, deactivate, verification methods and service endpoints.

## 10. Credential models and formats

Keep four axes separate:

1. **Identifier method** — DID lifecycle/resolution.
2. **Credential model/profile** — semantics, e.g. W3C VCDM.
3. **Serialization/proof format** — JWT, JSON-LD/Data Integrity, SD-JWT, mdoc.
4. **Communication protocol** — OID4VCI, OID4VP, SIOP 2.0, DIDComm.

Planned format/profile adapters:
- JWT VC / JWT VP for compatibility.
- JSON-LD VC/VP + Data Integrity.
- SD-JWT VC.
- ISO mdoc / mDL.
- Open Badges 3.0.
- VCDM 1.1 compatibility where required.
- VCDM 2.0 as the preferred W3C VC model.

The core `Credential` type MUST NOT equal one wire serialization.

### Verification pipeline
Parse → structural validation → cryptographic verification → issuer/key resolution → temporal checks → status/revocation → schema/profile checks → trust/policy decision.

Return structured verification outcomes; never collapse all checks into `valid: bool`.

### Presentation pipeline
Request validation → candidate discovery → disclosure planning → explicit user consent → proof/presentation creation → protocol response.

Store original signed credential bytes plus normalized searchable metadata. Do not silently rewrite signed payloads.

## 11. SSI communication protocols

### OpenID4VCI
Implement the wallet role for credential offers and issuance. Keep issuer metadata, authorization state and protocol wire models outside the credential domain.

### OpenID4VP
Handle presentation requests, request validation, credential selection, disclosure consent and response generation.

### SIOP 2.0
Support self-issued authentication while keeping authentication semantics distinct from VC presentation semantics.

### Presentation Exchange
Support only where required by selected interoperability profiles. Query-language mapping belongs behind an adapter/port.

### DIDComm v2
Support secure packing/unpacking, transport abstraction, peer relationships and routing/session concerns. DIDComm is a communication adapter; it must not own credential semantics.

## 12. Security model

Protected assets:
- private keys/seeds/recovery material;
- credentials and claims;
- DID relationship metadata;
- transaction authorization;
- authentication/presentation consent;
- wallet database and backups.

Critical boundaries:
- Dioxus/WebView ↔ Rust application;
- application ↔ key/secret adapter;
- core ↔ chain/SSI/protocol adapter;
- wallet ↔ deep link/QR;
- wallet ↔ issuer/verifier/resolver/indexer.

Required mitigations:
- human-readable signing/disclosure confirmation;
- strict URI/request validation;
- origin/audience/nonce/state checks in OpenID flows;
- DIDComm authentication/encryption validation;
- platform secure storage;
- no secrets in logs/crash reports;
- dependency pinning/auditing;
- malicious credential/document parser tests;
- backup/export re-authentication.

## 13. Non-functional requirements

- Android/iOS are Tier 1; desktop/web Tier 2.
- Core crates compile independently of Dioxus.
- Local-first storage and telemetry disabled by default.
- Structured error taxonomy.
- Idempotent import/sync where practical.
- No blocking crypto/network/storage work on UI execution.
- Stable capability ports and replaceable adapters.
- Rustdoc for public extension interfaces.

## 14. Suggested repository

```text
oxid/
  AGENTS.md
  README.md
  Cargo.toml
  apps/
    mobile/
    desktop/
    web/
  crates/
    foundation/
    wallet/{domain,application,ports}/
    chain/{domain,application,ports}/
    identity/{domain,application,ports}/
    credential/{domain,application,ports}/
    presentation/{domain,application,ports}/
    protocol/{domain,application,ports}/
    platform/ports/
    adapters/
      cardano/
      midnight/
      did-key/
      did-peer/
      did-web/
      did-webvh/
      did-prism/
      did-midnight/
      vc-jwt/
      vc-data-integrity/
      vc-sd-jwt/
      mdoc/
      openbadges/
      oid4vci/
      oid4vp/
      siop2/
      didcomm/
      storage-secure/
      storage-dev/
    ui-dioxus/
    composition/
  docs/
    adr/
```

Do not scaffold every crate immediately. Start with the first vertical slice and split when boundaries become real.

## 15. ADRs

### ADR-001 — Modular hexagonal architecture
**Accepted.** Bounded domain/application modules own ports; integrations are adapters.

### ADR-002 — Dioxus as incoming adapter
**Accepted.** Dioxus provides shared UI but stays outside core.

### ADR-003 — Oxid-owned domain types
**Accepted.** No Pallas/Midnight/SSI/OIDC SDK types in public core APIs.

### ADR-004 — Capability-specific ports
**Accepted.** Prefer focused traits to giant chain/identity plugin interfaces.

### ADR-005 — Static adapter plugins for MVP
**Accepted.** Cargo-selected adapters registered at composition root. Dynamic native plugins deferred.

### ADR-006 — Rust-first, WASM fallback
**Accepted.** WASM/JS only behind adapters when justified by implementation maturity.

### ADR-007 — Identity is first-class
**Accepted.** DID/VC/VP/authentication are peer domains to crypto wallet capabilities.

### ADR-008 — DID methods through adapters
**Accepted.** Method capabilities are negotiated; no universal lifecycle assumption.

### ADR-009 — Credential model separated from serialization
**Accepted.** VCDM/profile and JWT/JSON-LD/SD-JWT/mdoc/OpenBadges concerns remain separable.

### ADR-010 — OIDC and DIDComm are protocol adapters
**Accepted.** Protocols map wire/state flows to core use cases and do not define credential semantics.

### ADR-011 — Secure key operations behind ports
**Accepted.** UI/application code uses opaque key references and `KeyOperationPort`.

### ADR-012 — Mobile-first
**Accepted.** Android/iOS Tier 1, desktop/web Tier 2.

### ADR-013 — Local-first, telemetry-off
**Accepted.**

### ADR-014 — Cardano library selection
**Proposed.** Evaluate Pallas and maintained alternatives against wallet functionality, target support, maintenance, security and licensing.

### ADR-015 — Midnight library selection
**Proposed.** Prefer maintained official Rust/indexer interfaces and isolate evolving APIs.

### ADR-016 — SSI library selection
**Proposed.** Evaluate focused Rust components (including Spruce SSI ecosystem where suitable) by DID/format capability rather than adopting one monolith by default.

### ADR-017 — Secret storage
**Proposed.** Platform-backed mobile security by default; evaluate Askar where its encrypted storage/KMS model fits.

### ADR-018 — Typed error model
**Proposed.** Stable core error categories; adapters map external errors while retaining safe diagnostics.

### ADR-019 — Event model
**Proposed.** Explicit application events where useful; no event-sourcing/distributed architecture without concrete need.

### ADR-020 — Testing
**Accepted.** Unit/property tests for core, reusable port contract suites, adapter integration/interoperability tests, focused UI flows, fuzzing for security-critical parsers where practical.

## 16. MVP parity target

Use leading self-custody wallets as UX/capability references, not architecture templates. Baseline parity:
- simple onboarding/recovery;
- secure unlock;
- account/network management;
- balances/assets/history;
- send/receive;
- QR/deep links;
- transaction preview/confirmation;
- connection/session management;
- warnings and permissions.

Identity-wallet parity:
- credential inbox/store;
- issuer/verifier metadata;
- offer/request handling;
- disclosure preview/consent;
- selective disclosure;
- DID management;
- authentication.

## 17. Delivery roadmap

### M0 — Foundation
Workspace, CI, domain primitives, ports, composition registry, Dioxus mobile shell, in-memory adapters.

### M1 — Secure wallet + Cardano vertical slice
Create/import/unlock → secure key reference → Cardano account/balance → build/review/sign/submit → receive QR → history.

### M2 — Midnight
Equivalent supported capabilities through Midnight adapters without contaminating chain-neutral core semantics.

### M3 — Identity foundation
Identity profiles, DID resolution, `did:key`/`did:peer`/`did:web`, encrypted credential store, normalized metadata and verification pipeline.

### M4 — OpenID4VC
OID4VCI receive flow, OID4VP presentation, SIOP 2.0, deep/universal links and consent UI.

### M5 — Richer identity support
JSON-LD/Data Integrity, SD-JWT VC, mdoc/mDL, Open Badges 3.0, `did:webvh`, `did:prism`, `did:midnight`, VCDM 1.1 compatibility as needed.

### M6 — DIDComm
DIDComm v2 pack/unpack, peer relationships, transport/routing and selected credential/presentation workflows.

## 18. Definition of done

A capability requires:
- core use case and stable Oxid types;
- capability port;
- adapter;
- unit + contract/integration tests;
- security/privacy review when sensitive;
- ADR/docs updated;
- no external SDK types leaked inward;
- mobile smoke test for user-facing functionality.

## 19. First task for Codex

Do **not** implement the whole wallet.

Create the smallest compileable vertical architecture:
1. foundation primitives;
2. wallet domain;
3. wallet application;
4. platform ports;
5. in-memory storage adapter;
6. Dioxus UI adapter;
7. composition root.

Implement exactly one use case: **Create Wallet Profile** using in-memory adapters and a minimal Dioxus screen.

The purpose is to prove dependency direction, testing strategy and composition before introducing Cardano, Midnight or SSI SDKs.

## 20. Reference baseline

At the time this blueprint was prepared, Dioxus 0.7 documentation describes mobile as a first-class WebView-based target with experimental WGPU rendering, and supports platform feature selection for web/desktop/mobile. The W3C VC ecosystem publishes VCDM 2.0 as the current core model. Before implementing protocol adapters, Codex should verify current normative versions of OpenID4VCI, OpenID4VP, SIOP 2.0, DIDComm and each DID method, and record exact versions in dependency/standards ADRs.

## 21. Required dependency review template

For every significant external crate/library:
- project/repository;
- version/commit;
- license;
- maintenance/activity;
- security/audit evidence;
- Android support;
- iOS support;
- desktop support;
- WASM support;
- crypto primitives used;
- API stability;
- reason selected;
- alternatives considered;
- adapter boundary;
- exit/replacement strategy.
