# Live prototype parity audit — 2026-08-19

## Scope and method

This read-only audit compares Oxid with the immutable reviewed
`midnight-ledger` mobile prototype baseline
`074b1a4bccbfee1740ee188374b606a022ecef42` under `mobile-bench/`. Repository
types, tests, build gates, and packaged-host evidence count; issue checkboxes,
labels, fixture copy, or source volume do not.

The original estimate was roughly **95% of useful prototype behavior
implemented**. After the 2026-08-20 ADR-0098/#91 evidence, the estimate remains
roughly **97%** because the prototype itself never wired a real shielded spend.
Oxid intentionally adds stricter recovery, custody, consent, SSI,
reproducibility, and headless requirements, so current progress against the
stated 110% target is approximately **104/110 (95%)**. Production-release
evidence is lower, approximately **78%**. Physical Android QR/custom-scheme and
tailnet account-sync evidence is real rather than fixture-derived, and guarded
headless standalone unshielded plus genesis-authority shielded finality are now
funded and proven. Physical iOS, verified HTTPS associations, device resource
budgets, fresh-wallet DUST registration, funded mobile/native-custody
transactions, and a provisioned production deployment remain unproven.

## Capability matrix

| Capability | Classification | Exact Oxid evidence | Remaining boundary |
| --- | --- | --- | --- |
| Profile lifecycle and first-run choice | Implemented | `crates/wallet/domain`, `crates/wallet/application`, `crates/adapters/storage-json`, `crates/ui-dioxus/src/lib.rs`, `apps/oxid-headless/src/lib.rs` | None for prototype parity |
| Portable complete recovery | Implemented; device evidence partial | `crates/wallet/application/src/backup.rs`, `crates/adapters/backup-portable`, `crates/adapters/storage-mobile`, ADR-0074–0078, mobile backup tests | Physical-device interruption, picker, storage-pressure, and resource evidence (#33) |
| Native custody and protected derivation | Implemented; release evidence partial | `crates/adapters/storage-mobile`, `crates/adapters/mobile-native-plugin`, ADR-0071, native custody mobile tests | Physical Keychain/Keystore/user-presence and resource evidence (#30/#33) |
| Public accounts, receive, sync, and checkpoints | Implemented; physical tailnet and local simulator sync proven | `crates/adapters/midnight/src/indexer.rs`, `crates/adapters/midnight/src/checkpoint.rs`, `crates/adapters/midnight/src/dust_sync.rs`, `crates/adapters/midnight/src/shielded_sync.rs`, shared durable profile association repository, physical Android and focused iOS/Android localhost `live-account` flows | Production background/session policy, provisioned signed deployment, and funded native-custody mobile account |
| Unshielded transfer lifecycle | Implemented; funded headless standalone E2E proven | `crates/wallet/application/src/transaction.rs`, `crates/adapters/midnight/src/transaction.rs`, `crates/adapters/midnight/src/submission.rs`, `crates/composition/src/standalone_funding_tests.rs`; exact prepare→authorize→DUST prove→submit→finalize→adapter reconstruction with included-status restoration→stable recipient balance | Funded native-custody mobile journey and real production deployment evidence |
| Shielded sync and spending | Implemented; funded headless standalone E2E proven | `crates/adapters/midnight/src/shielded_sync.rs`, `crates/adapters/midnight/src/shielded_transport.rs`, `crates/adapters/midnight/src/transaction.rs`, `crates/composition/src/standalone_funding_tests.rs`, ADR-0079/#91 | Typed DUST registration and fresh-wallet origination (#92), funded mobile/native custody, checkpoint-aware journal compaction (#93), and physical proof budgets (#30) |
| DID inventory/lifecycle | Implemented standalone; live writes missing | `crates/identity/domain`, `crates/identity/application`, `crates/adapters/did-midnight`, headless DID methods and Dioxus management | Authenticated discovery and live Compact writes |
| Credential storage and verification | Implemented standalone; production trust/status partial | `crates/credential/domain`, `crates/credential/application`, `crates/adapters/storage-credential-json`, `crates/adapters/vc-midnight` | Production issuer trust, status/revocation, and live transport |
| OpenID4VCI, SIOPv2, and OpenID4VP consent | Implemented standalone | `crates/protocol`, `crates/presentation`, `crates/adapters/openid4vci`, `crates/adapters/openid4vp`, `crates/adapters/siopv2` | Authenticated production discovery/transport and response delivery |
| Compact presentation proving | Implemented in headless and explicit mobile conformance | `crates/adapters/vc-midnight`, authenticated artifact closure, ADR-0050/0072/0083 | Physical latency/memory/thermal/size budgets (#30) |
| Passport Vault | Implemented standalone and native-lifecycle capable; live/device partial | `crates/passport-vault`, `crates/adapters/passport-vault`, ADR-0051–0068, headless/mobile journey | Authenticated live deployment/state/call evidence and device budgets (#31) |
| QR and OS identity ingress | Implemented; Android physical evidence complete for locally supported paths | `crates/platform/ports`, `crates/adapters/identity-ingress`, `crates/adapters/mobile-native-plugin`, `apps/oxid/android/MainActivity.kt`, `scripts/test-android-identity-ingress-physical.sh`, focused native tests, Samsung SM-S928B / Android 16 (API 36) success/cancel/timeout and warm/cold custom-scheme runs | Physical iOS camera/permission evidence and reviewed AASA/assetlinks/release-signing evidence (#32); Android app-owned denial is not applicable to permissionless Google Code Scanner |
| Developer capability visibility | Implemented | `crates/capabilities/application`, headless `system.capabilities`, ADR-0095, `scripts/check-ui-profile-release.sh` | None; unsafe prototype logs/benchmarks remain deliberately excluded |
| Repeatable demo setup | Implemented | compile-time `ui-profile-demo`, named isolated profile drawer, ADR-0096, focused Dioxus/mobile/release-exclusion tests | None; automated consent is deliberately excluded |

## Deliberate exclusions

Oxid must not copy the prototype's aggregate wallet facade, WebView/JavaScript
command bridge, public-derived holder scalar, fixed presentation nonce, demo or
genesis secrets, pre-production key files, raw log/benchmark surfaces,
environment-selected production behavior, or unreviewed relative/mutable
dependencies. These are security improvements, not parity gaps.

## Dependency-ordered backlog

1. Capture physical native custody and complete-recovery interruption/resource
   evidence (#33, with #30 budgets).
2. Provision reviewed production trust roots, a signed deployment profile, and
   independent SSI protocol transports on approved infrastructure. ADR-0098
   supplies signature/rollback/TLS and node-genesis composition gates but does
   not invent a deployment.
3. Implement protected DUST registration and prove a fresh-wallet shielded
   origination, then a funded mobile/native-custody journey (#92/#30).
4. Validate physical iOS QR/permission behavior and reviewed universal/app
   links using an approved HTTPS domain, AASA, associated-domain entitlement,
   `assetlinks.json`, and release identities (#32). Android physical
   success/cancel/timeout and warm/cold custom schemes are complete.
5. Define production background synchronization/session behavior.
6. Complete identity trust/status, live protocol delivery, and live DID writes.
7. Produce Passport Vault live-deployment and physical-device evidence (#31).
8. Close remaining mobile size, memory, latency, thermal, and storage budgets.

The compile-time localhost standalone profile shares the tailnet profile's
exact undeployed chain identity while differing only in loopback transport.
ADR-0098 implements the authenticated production profile/genesis boundary
without provisioning a root or deployment and completes funded real-node
unshielded plus genesis-authority shielded headless fixtures. The shielded run
also corrected the indexer v4 `ZswapLedgerEvent` envelope and sparse global
cursor contract, and proved included-fingerprint blocking, included-status
restoration after adapter reconstruction, and nullifier/balance safety. The
funded run did not exercise unknown-outcome chain rescanning. The next bounded
engineering slice is **typed DUST
registration followed by one fresh-wallet shielded spend** (#92); safe
checkpoint-acknowledged journal compaction is isolated in #93. Engineering-only
work is approximately three to six bounded waves; this is a scope estimate,
not a calendar promise. External evidence has no honest ETA until approved
domains, association files, release signing identities, physical devices,
funded production accounts, and live deployments are available.

## Completion tests

- Headless: create/restore the complete wallet, synchronize from authenticated
  sources, prepare and explicitly authorize one unshielded transfer, prove,
  submit, observe finalized inclusion, restart, and reconcile without exposing
  transaction or custody material.
- Headless shielded: live catch-up to an equal cursor, select adapter-private
  notes, authorize, prove, finalize, restart, and demonstrate nullifier/note
  replay safety.
- Identity: acquire one request through authenticated discovery, prove strict
  routing and four-question consent, and deliver the protocol response without
  raw request/token/proof output.
- Physical iOS: camera success/cancel/denial, timeout and stale-callback
  isolation, plus warm/cold custom and verified links. Physical Android still
  needs vendor/module-unavailable coverage without modifying a personal device,
  verified HTTPS App Links, custody/recovery interruption, and explicit
  size/RSS/latency/thermal/storage budgets; QR success/cancel/timeout,
  post-return liveness, consent isolation, and warm/cold custom schemes are
  already proven.
- Passport Vault: authenticate deployed state, prepare/authorize/prove/submit
  each supported call on live infrastructure, reconcile finality, and repeat
  the claim path with managed holder custody on physical devices.
