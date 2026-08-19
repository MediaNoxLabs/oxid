# Live prototype parity audit — 2026-08-19

## Scope and method

This read-only audit compares Oxid with the immutable reviewed
`midnight-ledger` mobile prototype baseline
`074b1a4bccbfee1740ee188374b606a022ecef42` under `mobile-bench/`. Repository
types, tests, build gates, and packaged-host evidence count; issue checkboxes,
labels, fixture copy, or source volume do not.

The estimate is roughly **95% of useful prototype behavior implemented**. The
Oxid target intentionally adds stricter recovery, custody, consent, SSI,
reproducibility, and headless requirements, so current progress against the
stated 110% target is approximately **100/110 (91%)**. Production-release
evidence is lower, approximately **75%**. Physical Android QR/custom-scheme and
tailnet account-sync evidence is now real rather than fixture-derived, but
physical iOS, verified HTTPS associations, device resource budgets, funded
protected accounts, and live transaction completion remain unproven.

## Capability matrix

| Capability | Classification | Exact Oxid evidence | Remaining boundary |
| --- | --- | --- | --- |
| Profile lifecycle and first-run choice | Implemented | `crates/wallet/domain`, `crates/wallet/application`, `crates/adapters/storage-json`, `crates/ui-dioxus/src/lib.rs`, `apps/oxid-headless/src/lib.rs` | None for prototype parity |
| Portable complete recovery | Implemented; device evidence partial | `crates/wallet/application/src/backup.rs`, `crates/adapters/backup-portable`, `crates/adapters/storage-mobile`, ADR-0074–0078, mobile backup tests | Physical-device interruption, picker, storage-pressure, and resource evidence (#33) |
| Native custody and protected derivation | Implemented; release evidence partial | `crates/adapters/storage-mobile`, `crates/adapters/mobile-native-plugin`, ADR-0071, native custody mobile tests | Physical Keychain/Keystore/user-presence and resource evidence (#30/#33) |
| Public accounts, receive, sync, and checkpoints | Implemented; physical tailnet sync proven | `crates/adapters/midnight/src/indexer.rs`, `crates/adapters/midnight/src/checkpoint.rs`, `crates/adapters/midnight/src/dust_sync.rs`, `crates/adapters/midnight/src/shielded_sync.rs`, shared durable profile association repository, physical Android `live-account` flow | Compile-time localhost simulator profile, production background/session policy, authenticated discovery, and funded protected account |
| Unshielded transfer lifecycle | Implemented in deterministic/live-capable adapters; live E2E partial | `crates/wallet/application/src/transaction.rs`, `crates/adapters/midnight/src/transaction.rs`, `crates/adapters/midnight/src/submission.rs`, Dioxus/headless tests | One funded real-node prepare→authorize→prove→submit→finalize fixture |
| Shielded sync and spending | Implemented in standalone; production partial | `crates/adapters/midnight/src/shielded_sync.rs`, `crates/adapters/midnight/src/shielded_transport.rs`, `crates/adapters/midnight/src/transaction.rs`, ADR-0079 | Funded real shielded spend plus physical proof budgets (#59/#30) |
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
2. Add the prototype-equivalent compile-time localhost standalone transport
   profile for simulator use; keep it distinct from simulation and tailnet.
3. Define authenticated production discovery/composition for Midnight and SSI;
   do not infer trust from environment routes.
4. Add one funded real-node unshielded end-to-end fixture through finalized
   completion.
5. Prove a funded real shielded spend and physical Compact budgets (#59/#30).
6. Validate physical iOS QR/permission behavior and reviewed universal/app
   links using an approved HTTPS domain, AASA, associated-domain entitlement,
   `assetlinks.json`, and release identities (#32). Android physical
   success/cancel/timeout and warm/cold custom schemes are complete.
7. Define production background synchronization/session behavior.
8. Complete identity trust/status, live protocol delivery, and live DID writes.
9. Produce Passport Vault live-deployment and physical-device evidence (#31).
10. Close remaining mobile size, memory, latency, thermal, and storage budgets.

The next bounded engineering slice is the **compile-time localhost standalone
profile for simulator/desktop use**, sharing the tailnet profile's exact
undeployed chain identity and differing only in its loopback routes. It must
remain distinct from deterministic simulation and from runtime production
discovery. The following slice is **authenticated production discovery plus
one funded real-node unshielded end-to-end fixture**. Engineering-only work is
approximately five to eight bounded waves; this is a scope estimate, not a
calendar promise. External evidence has no honest ETA until approved domains,
association files, release signing identities, physical devices, funded
accounts, and live deployments are available.

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
