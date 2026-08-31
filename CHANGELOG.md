# Changelog

All notable changes to Oxid will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once public releases begin.

## [Unreleased]

### Added

- Reproducible Nix development shell and build.
- Public repository contribution, security, dependency, and CI harness.
- Modular hexagonal Rust workspace.
- Create Wallet Profile use case with in-memory persistence and Dioxus UI.
- Migration inventory for the Midnight ledger wallet prototype.
- Blueprint-to-repository architecture decision catalog with explicit delivery
  states and research gates.
- Project-local Pi peer-review extension and skill at version 0.5.0.
- Immutable official-GitHub source enforcement for future Midnight ledger and
  proof dependencies.
- Reproducible Dioxus build, install, and launch command for the iOS simulator.
- Prototype-derived mobile wallet shell with Oxid branding, responsive
  navigation, safe-area handling, and honest deferred-capability states.
- Public staged-parity epic and focused wallet-profile integration backlog.
- Presentation migration provenance and third-party icon notices.
- UI-neutral application composition plus a versioned NDJSON headless wallet
  harness with capability discovery, profile creation, safe errors, and
  graceful shutdown.
- Linux Nix inputs for Dioxus's `libxdo` linker requirement.
- Profile onboarding, listing, active selection, management, and launch-time
  restoration across the Dioxus shell and headless protocol.
- Versioned write-through-temp JSON persistence for public wallet profile
  metadata, with strict validation and no secret-bearing fields.
- Automated iOS XCUITest and Android emulator profile-flow smoke harnesses,
  including process restart and active-profile restoration.
- Accepted platform-custody policy, opaque key-operation ports, fail-closed
  production composition, and a development-only headless Ed25519/P-256
  initialize/lock/unlock/generate/list/sign/delete conformance flow.
- Made the iOS simulator and XCUITest harness select the host Xcode SDK and
  isolate Apple builds from Nix compiler/linker environment variables.
- Explicit resumable DUST synchronization across the native Midnight worker,
  versioned headless lifecycle, and mobile Assets progress pane, with exact
  atomic balance, cancellation, partial private checkpoints, and cached-state
  fail-closed semantics.
- Protected canonical Midnight shielded receive-address derivation, public
  headless projection, and Dioxus/iOS/Android receive rendering without
  exposing Zswap private material.
- Bounded official `zswapLedgerEvents` decoding and adapter-private canonical
  replay with exact Merkle ordering, verified ownership, foreign-branch
  collapse, and nullifier spend removal.
- Checksummed, owner-private, key/network/source-scoped Zswap replay checkpoints
  with partial resume cursors, strict size/record limits, and atomic replacement.
- Explicit shielded synchronization status/start/cancel use cases with exact
  per-token balances, a protected standalone session, versioned headless flow,
  and iOS/Android Assets-page coverage.
- Bounded native `zswapLedgerEvents` synchronization on an off-renderer worker,
  with optional durable shielded checkpoint wiring for read-only and complete
  standalone headless configurations.
- Explicit transaction submission status and cooperative pre-broadcast
  cancellation across application, Midnight adapter, headless, and mobile
  flows, with acknowledged cancellation restoring safe retryability and
  post-broadcast cancellation refused.
- Bounded owner-private persistence for public Midnight submission metadata,
  committed before node broadcast, with restart status/history, duplicate
  prevention, finalized-chain reconciliation, two-process headless coverage,
  and a mobile recovery/reconcile surface.
- Dependency-free identity domain/application boundaries plus current
  Midnight DID 0.5.0 syntax, public JWK, document, and relationship validation.
- Explicit bounded native `POST /resolve` and single-fixture standalone DID
  adapters, with profile-scoped list/get/forget headless methods.
- Separate versioned owner-private public DID-record persistence with restart
  coverage and a functional Dioxus/iOS/Android DID inventory page.

### Changed

- Exact-head Claude reviews now select and attest a bounded reasoning effort.
  Their default deadline is five minutes, reduced from fifteen to keep the
  review checkpoint inside the factory SLA; the wrapper and verifier reject
  longer deadlines, callers may select a shorter one, and a timeout is not a
  pass.
  New attestations use schema v3; rerun reviews whose legacy v2 evidence no
  longer verifies instead of relabeling records that did not capture effort.
  In-flight branches must rerun the exact-head review; legacy records are not
  translated into the stronger shape.
  Verification reports this migration through the distinct
  `ClaudeReviewEvidenceVersionError` type rather than as a non-clean verdict.
