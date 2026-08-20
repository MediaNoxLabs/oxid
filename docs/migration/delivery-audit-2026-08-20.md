# Oxid migration delivery audit — 2026-08-20

## Executive summary

This is the stopping-point audit for the migration wave that followed Phase 4a.
It compares the immutable reviewed `midnight-ledger` prototype baseline
`074b1a4bccbfee1740ee188374b606a022ecef42` (`mobile-bench/`) with repository,
test, mobile-host, and live-environment evidence in Oxid. Issue labels and stale
checkboxes are not evidence.

The evidence supports approximately **98% of useful prototype behavior** and
**105/110 (about 95%) of Oxid's deliberately stricter migration target**. The
production-release evidence remains approximately **78%**. These are capability
estimates, not source-line or issue-count metrics. Oxid is a capable standalone
Midnight/SSI wallet and headless test harness; it is not yet a provisioned,
fully evidenced production wallet.

The implementation wave is published as signed commits on `develop`. Its last
feature commit before this audit is `887f18dc1dfb192ad7b28d1b87ce37d5c546c40a`;
the signed commit containing this file also replaces an unavailable upstream
`arrayref` Git fetch with the checksum-locked 0.3.9 registry archive that was
independently compared with the reviewed canonical revision. The 0.3.10
publication remains excluded. No dependency source was copied into Oxid.

## Delivered and evidenced

| Area | Delivered result | Repository or device evidence | Issue state |
| --- | --- | --- | --- |
| Wallet shell and profiles | First-run create/restore, persisted profile selection, four-tab shell, Home, Assets, Documents, Settings, profile route, protected Send and Receive | `crates/ui-dioxus`, `crates/wallet`, `crates/adapters/storage-json`, iOS/Android smoke journeys | [#1](https://github.com/MediaNoxLabs/oxid/issues/1), [#3](https://github.com/MediaNoxLabs/oxid/issues/3), [#78–#83](https://github.com/MediaNoxLabs/oxid/issues/78) closed |
| Headless wallet | Versioned NDJSON harness over the same application/composition boundaries, including profiles, accounts, transactions, DUST, shielded state, DID, credentials, consent, and Passport Vault flows | `apps/oxid-headless`, persistent multi-process tests | [#4](https://github.com/MediaNoxLabs/oxid/issues/4) closed |
| Custody and recovery | Opaque key operations, native sealed mobile vault, protected derivation, complete encrypted backup/recovery, restart/unlock tests | `crates/adapters/storage-mobile`, `crates/adapters/custody-software`, `crates/adapters/backup-*`, mobile smoke gates | Functionality delivered; physical interruption/resource evidence remains [#30](https://github.com/MediaNoxLabs/oxid/issues/30)/[#33](https://github.com/MediaNoxLabs/oxid/issues/33) |
| Midnight accounts and unshielded NIGHT | Protected addresses, exact balances, live/cached sync, durable checkpoints, prepare/authorize/prove/submit/reconcile, cancellation and unknown-outcome barriers | `crates/adapters/midnight`, funded standalone headless finality and adapter-reconstruction evidence | [#6–#20](https://github.com/MediaNoxLabs/oxid/issues/6) closed for their bounded scopes |
| Shielded wallet | Protected role-3 address, bounded Zswap replay, resumable checkpoints, exact token balances, protected shielded spend, nullifier-safe restart | `shielded.rs`, `shielded_transport.rs`, `shielded_sync.rs`, `transaction.rs`, funded genesis-authority standalone finality | [#18](https://github.com/MediaNoxLabs/oxid/issues/18), [#59](https://github.com/MediaNoxLabs/oxid/issues/59), [#91](https://github.com/MediaNoxLabs/oxid/issues/91) closed |
| Protected DUST registration | Separate domain/application/adapter/UI/headless state machine, protected DUST child metadata, canonical planning/composition, explicit two-step consent, restart/reconciliation boundaries, and guarded PreProd A/B harness | `crates/wallet/*/dust_registration.rs`, `crates/adapters/midnight/src/dust_registration.rs`, `crates/composition/src/standalone_funding_tests.rs`, Dioxus tests | Implemented; funded PreProd write/recovery and fresh-wallet spend evidence remain [#92](https://github.com/MediaNoxLabs/oxid/issues/92) |
| Cold replay safety | DUST and Zswap observers close bounded segments before CPU replay/checkpoint work; cached progress resumes without transport backpressure | `submission.rs`, `dust_sync.rs`, `shielded_transport.rs`, `shielded_sync.rs`; 119 focused Midnight adapter tests | Delivered safety/correctness slice; throughput and birthday acceleration remain [#115](https://github.com/MediaNoxLabs/oxid/issues/115)/[#116](https://github.com/MediaNoxLabs/oxid/issues/116) |
| DID and credentials | `did:midnight` inventory/lifecycle/signing, encrypted credential storage, structured verification, Digital Passport policy and disclosure planning | `crates/identity`, `crates/credential`, `crates/adapters/did-midnight`, `crates/adapters/vc-midnight` | Standalone scopes [#21–#26](https://github.com/MediaNoxLabs/oxid/issues/21) delivered |
| SSI protocols | Standalone OpenID4VCI, SIOPv2, strict OpenID4VP request/consent/proof boundary and independent proof verification | `crates/protocol`, `crates/presentation`, `crates/adapters/openid4vci`, `openid4vp`, `siopv2` | Core standalone paths delivered; production delivery remains [#27](https://github.com/MediaNoxLabs/oxid/issues/27)/[#29](https://github.com/MediaNoxLabs/oxid/issues/29)/[#34](https://github.com/MediaNoxLabs/oxid/issues/34) |
| Passport Vault | Typed standalone accounting, four wallet operations, authenticated artifact/state boundaries, protected claim, durable recovery and headless/mobile journeys | `crates/passport-vault`, `crates/adapters/passport-vault` | Standalone delivered; live deployment/device evidence remains [#31](https://github.com/MediaNoxLabs/oxid/issues/31) |
| Native identity ingress | One-item bounded native handoff, strict shared router, explicit consent, payload-free failures; Android physical QR success/cancel/timeout, post-return liveness, and warm/cold custom scheme | `crates/platform/ports`, `crates/adapters/identity-ingress`, mobile native plugin, `scripts/test-android-identity-ingress-physical.sh`; Samsung SM-S928B, Android 16/API 36 | Android locally supported evidence delivered; remaining evidence [#32](https://github.com/MediaNoxLabs/oxid/issues/32)/[#114](https://github.com/MediaNoxLabs/oxid/issues/114) |
| Development and demo profiles | Truthful capability viewer and explicitly opt-in demo drawer, both compile-time only and excluded from ordinary releases | `crates/capabilities/application`, Dioxus profile features, `scripts/check-ui-profile-release.sh` | [#87](https://github.com/MediaNoxLabs/oxid/issues/87)/[#88](https://github.com/MediaNoxLabs/oxid/issues/88) closed |
| Standalone mobile routing | Separate compile-time loopback simulator and MagicDNS/TLS tailnet profiles; no runtime production switch or committed personal endpoint | mobile launch scripts, deployment-profile adapter, iOS/Android smoke gates, physical Android tailnet sync | [#89](https://github.com/MediaNoxLabs/oxid/issues/89) closed; production discovery remains [#90](https://github.com/MediaNoxLabs/oxid/issues/90) |
| Reproducibility and supply chain | Nix development/build/check surfaces, full-revision Midnight Git policy, DCO/GPG commits, source gates, cargo-deny/audit, release exclusion checks | `flake.nix`, `nix/`, `run.sh`, source-policy scripts, signed Git history | Current ArrayRef follow-up remains tracked in [#113](https://github.com/MediaNoxLabs/oxid/issues/113) |

## Validation at this stopping point

The combined tree passed all locally applicable gates:

```text
nix develop -c cargo fmt --all -- --check                 PASS
nix develop -c ./run.sh --strict                         PASS
nix flake check --print-build-logs                       PASS
nix develop -c just ios-smoke                            PASS (7 scenarios)
nix develop -c just android-smoke                        PASS
git diff --check                                         PASS
all wave commits: GPG signature + DCO trailer            PASS
```

The exact iOS smoke device was iPhone 17 Pro / iOS 26.4 simulator
`76B99C81-BE72-4A93-A443-7F244723AAF3`, bundle `io.medianox.oxid`. Android
emulator evidence and iOS simulator evidence are not described as physical
device evidence. Physical Android evidence used the model/API above; its serial
is intentionally not recorded.

The first CI run at `887f18d` found that the reviewed public `arrayref` Git
repository had become unavailable after local validation. Repository, quality,
and Linux Nix jobs therefore failed at dependency fetch, not compilation or
tests. The publication commit containing this audit removes that network
dependency while retaining the reviewed 0.3.9 archive checksum and an explicit
source-policy gate. Final branch-CI status belongs to that commit's GitHub
checks, not to the failed predecessor run.

## Remaining work, in dependency order

1. **Measure and accelerate full-history replay.** Add a guarded, read-only,
   reproducible PreProd corpus/capacity test and complete at least two measured
   optimization iterations without raising safety caps blindly
   ([#115](https://github.com/MediaNoxLabs/oxid/issues/115)).
2. **Add birthday-gated fresh-wallet replay.** Write the ADR and implement an
   authenticated network-bound start reference, with genesis fallback and
   differential equivalence tests. This is the useful fast-start pattern
   identified from the reviewed Moth Wallet baseline; it is not a cacheless
   genesis "cold start" shortcut ([#116](https://github.com/MediaNoxLabs/oxid/issues/116)).
3. **Complete the funded PreProd DUST acceptance.** Once funding is observable,
   prove the exact A/B topology read-only, then—only after the explicit public
   prover privacy acknowledgement—run one protected registration, recovery,
   and fresh-wallet shielded spend without duplicate submission
   ([#92](https://github.com/MediaNoxLabs/oxid/issues/92)).
4. **Finish production composition.** Provision authenticated trust roots,
   signed deployment profiles, production discovery, background/session policy,
   and funded finality evidence ([#90](https://github.com/MediaNoxLabs/oxid/issues/90),
   migration epic [#2](https://github.com/MediaNoxLabs/oxid/issues/2)).
5. **Complete physical mobile evidence.** Validate iOS camera/permission and
   universal-link behavior on a physical device; add verified HTTPS association
   only with reviewed domains, AASA/`assetlinks.json`, and release identities
   ([#32](https://github.com/MediaNoxLabs/oxid/issues/32),
   [#114](https://github.com/MediaNoxLabs/oxid/issues/114)). Capture physical
   custody/recovery interruption and proving size/RSS/latency/thermal/storage
   budgets ([#30](https://github.com/MediaNoxLabs/oxid/issues/30),
   [#33](https://github.com/MediaNoxLabs/oxid/issues/33)).
6. **Complete live SSI and Passport Vault delivery.** Finish production issuer
   trust/status, live protocol response delivery, live DID writes, exact Compact
   bundles, and deployed Passport Vault calls
   ([#27](https://github.com/MediaNoxLabs/oxid/issues/27),
   [#29](https://github.com/MediaNoxLabs/oxid/issues/29),
   [#31](https://github.com/MediaNoxLabs/oxid/issues/31),
   [#34](https://github.com/MediaNoxLabs/oxid/issues/34)).
7. **Close recovery and submission maintenance boundaries.** Add authoritative
   checkpoint acknowledgement before compacting submission barriers
   ([#93](https://github.com/MediaNoxLabs/oxid/issues/93)) and finish the
   remaining physical recovery evidence ([#33](https://github.com/MediaNoxLabs/oxid/issues/33)).
8. **Resolve secondary platform/dependency debt.** Restore the Tier-2 web graph
   ([#13](https://github.com/MediaNoxLabs/oxid/issues/13),
   [#101](https://github.com/MediaNoxLabs/oxid/issues/101)), replace inherited
   unmaintained dependencies where upstream permits
   ([#10](https://github.com/MediaNoxLabs/oxid/issues/10),
   [#74](https://github.com/MediaNoxLabs/oxid/issues/74)), and keep architecture
   records/indexes consistent ([#73](https://github.com/MediaNoxLabs/oxid/issues/73)).

## Current blockers

### External evidence inputs

- The user-funded deterministic PreProd account has not yet been observed as
  funded; propagation was expected to take roughly an hour. No amount is
  assumed and no address or seed is published.
- A live PreProd write remains blocked on an exact read-only topology pass and
  the user's explicit acknowledgement that the configured public prover can
  observe proof-request metadata. Funding alone is not that acknowledgement.
- Physical iOS hardware, reviewed HTTPS domains, AASA/`assetlinks.json`, and
  release signing identities are not currently available.
- Production trust roots, deployment manifests, funded accounts, and a live
  Passport Vault deployment must be provided or approved outside this repository.
- Physical size, memory, latency, thermal, storage-pressure, and interruption
  evidence cannot be replaced by simulator fixtures.

### Engineering work (not external blockers)

- The current optimized full DUST replay reached 541,357 events and cursor
  553,478 of target 1,446,220 without failure at the 900-second observer bound;
  it did not finish. Throughput optimization is real engineering work, owned by
  [#115](https://github.com/MediaNoxLabs/oxid/issues/115).
- Birthday/reference-gated replay is not implemented, owned by
  [#116](https://github.com/MediaNoxLabs/oxid/issues/116).
- Production background synchronization, live SSI delivery, Passport Vault
  deployment, physical recovery/proving budgets, and checkpoint compaction are
  implemented only partially or not yet evidenced, as linked above.

### Deliberate boundaries, not defects

- A fresh wallet starts with zero DUST. DUST is recoverable from registered
  NIGHT and does not need a fabricated bootstrap balance.
- One configured master seed is sufficient for the test harness: domain-separated
  hardened account indices derive separate A/B wallets. Neither the seed nor
  derived private material is logged, committed, or placed in issue comments.
- Native capture never classifies, executes, logs, or persists identity
  requests. It hands at most one bounded item to the strict router and consent.
- The prototype's aggregate wallet facade, WebView JavaScript command bridge,
  demo/genesis secrets, public-derived holder scalar, fixed presentation nonce,
  free-form telemetry, and mutable/path dependencies remain deliberately
  excluded.

## Safe resume point

Do not begin with a write. First fetch and verify signed `develop`, check branch
CI, then run the read-only funded-account observer. If funding is visible,
record only the allowlisted public topology result. The next engineering slice
is [#115](https://github.com/MediaNoxLabs/oxid/issues/115), followed by the ADR
boundary in [#116](https://github.com/MediaNoxLabs/oxid/issues/116). The guarded
PreProd write in [#92](https://github.com/MediaNoxLabs/oxid/issues/92) remains a
separate consented evidence step.
