<!-- SPDX-License-Identifier: Apache-2.0 -->

# CI target and dependency matrix

This matrix is the authority for what Oxid means by “all targets.” A target is
required only when its dependencies and trust boundary are available. Public,
deterministic targets may gate a pull request; credentialed networks, physical
devices, and a separate repository remain explicit owner evidence.

## Assurance ladder

| Level | Required evidence | Budget | When it runs |
| --- | --- | --- | --- |
| L0 basic | PR title/body, DCO, GitHub-verified commit signature, repository contracts, formatting, architecture, lint, production compilation | 0–5 min | Every PR. Rust compilation is omitted only when the impact plan proves no Rust/build surface changed. |
| L1 host | Workspace unit tests on one Linux host | 5–10 min | Rust, UI, headless, platform, Compact, or build changes; on demand for any PR. |
| L2 component integration | Hermetic headless black-box tests, then deterministic Docker integration when its fixture is ready | 5–10 min for the current hermetic lane; Docker budget pending measurement | Affected host/component changes and on demand. |
| L3 extended | UI feature profiles, optimized UI release audit, coverage, quality, locked Nix package, Compact artifacts | 10–30 min per parallel lane | Affected high-risk/build changes; every `integration` delivery; release profile. |
| L4 platform/release | WASM, Android, iOS, Portal, standalone Midnight, PreProd, physical-device and real-proof evidence | Target-specific | Scheduled, on demand, or owner-private until each row below has a hermetic hosted runner. |

L0 is an envelope of existing required contexts rather than one serial job.
`Validate PR title`, `Validate PR body`, `Verify commit sign-offs`, scanner,
and the new `Basic gate` run in parallel. Commit signatures remain enforced by
GitHub ruleset `21481544`; a checkout cannot independently verify an unknown
contributor's GPG keyring.

## Runnable and planned targets

| Target | Command/evidence | Dependencies | Trigger areas | Budget/evidence | State |
| --- | --- | --- | --- | --- | --- |
| `basic` | `./run.sh repository`; for Rust-affecting changes `./run.sh basic` compiles/lints the dependency-light architectural/domain canary | Node; minimal `ci-rust` shell only for Rust | all | hard 5 min | Hosted PR gate |
| `unit-linux` | `./run.sh unit` (workspace core/headless/MCP units; UI/app excluded to their lane) | x86_64 Linux, minimal `ci-rust` shell, sccache | core, UI, headless, mobile/platform, Compact, build | hard 10 min; historical cold workspace tests 119 s locally | Hosted PR lane |
| `headless-linux` | `./run.sh headless-integration` | same Linux host; no live services | core, headless, platform, Compact | hard 10 min | Hosted PR lane |
| `ui-linux` | `./run.sh ui` (profile guards, feature compilation and UI/app tests) | minimal `ci-ui` shell plus Linux GTK/WebKit libraries | shared core, UI, platform, Compact | hard 20 min; a cold read-only run with no trusted seed crossed the former 15 min ceiling after its tests passed | Hosted PR lane |
| `ui-release-linux` | `./run.sh ui-release` (optimized build and forbidden-marker audit) | minimal `ci-ui` shell, release compilation | shared core, UI, platform, Compact | hard 25 min | Hosted PR artifact lane |
| `coverage-linux` | `./run.sh coverage` | minimal `ci-coverage` shell and `cargo-llvm-cov` | every Rust-affecting area | hard 25 min, 80% line floor | Hosted PR lane |
| `quality` | `./run.sh quality --strict` | minimal uncached `ci-quality` audit/deny/rustdoc shell | every Rust-affecting area | hard 20 min; 9m15 on PR #165 | Hosted required context |
| `nix-package` | `nix build` | x86_64 Linux, locked Nix graph | build and Compact | hard 45 min | Hosted PR lane |
| `compact-artifacts` | build presentation, Passport Vault artifacts and call composer together | Compact toolchain and locked p18 sources | build and Compact | hard 30 min | Hosted PR lane |
| `nightly-hermetic` | `nix flake check --print-build-logs` | full Nix sandbox | complete repository | 57–60 min observed | Hosted nightly backstop |
| `standalone-midnight` | `standalone-up.sh` plus funded finality scripts | Docker; proof server, indexer and node images; reviewed public funding fixture | Midnight transaction/Compact | startup can wait up to 20 min; full budget unmeasured | On demand; not yet safe as a public required gate |
| `portal-headless-local` | `scripts/e2e/portal-headless-e2e.sh` / ignored live headless test | existing three-service `oxid-standalone` Docker project; Oxid-owned ephemeral issuer mock; loopback only | OID4VCI, identity, headless composition, Midnight indexer sync | unmeasured | On demand; local indexer-sync evidence only, with node/prover use unproven |
| `preprod-live` | `test-preprod-registration-e2e.sh` | private master seed, funded cases, public prover privacy acknowledgement | live Midnight delivery | variable/non-hermetic | Owner-private release evidence only |
| `real-proof` | ignored p18 proof tests | authenticated 135 MiB artifact closure and prover resources | presentation and Passport Vault proofs | device/host budgets pending | Release/performance evidence |
| `wasm-compile` | `cargo check -p oxid-app --no-default-features --features web --target wasm32-unknown-unknown` | target-scoped compiler override | web/shared UI/dependencies | currently fails in Nix cc-wrapper hardening flags | Blocked; issue #13 |
| `android-compile` | Dioxus Android build through repository scripts | Android SDK/NDK, arm64 target | mobile/shared/dependencies | unmeasured in hosted CI | Candidate Linux hosted lane |
| `android-emulator` | Android smoke, standalone, demo/dev, backup, custody and ingress scripts | KVM emulator, SDK/NDK; Docker for standalone | mobile/user-visible changes | journey evidence exists; hosted budget unmeasured | On demand until runner is provisioned |
| `android-physical` | tailnet and identity-ingress physical scripts | owned device, ADB, network/profile credentials | native ingress/custody/release | device-specific | Owner release evidence |
| `ios-compile` | Dioxus iOS build | macOS, Xcode, simulator SDK | mobile/shared/dependencies | unmeasured in hosted CI | Candidate macOS hosted lane |
| `ios-simulator` | iOS smoke, standalone, demo/dev, backup, custody and ingress scripts | macOS/Xcode simulator; Docker for standalone | mobile/user-visible changes | full smoke measured at 228.205 s locally | On demand until macOS runner is provisioned |
| `ios-physical` | camera, universal-link, custody and resource evidence | owned signed device, release identity/domains | native/release | not yet complete | Owner release evidence |

The two current stable required CI names remain aggregators. They fail when a
selected child lane fails and succeed when an unselected lane is intentionally
skipped. This changes execution topology without requiring an unsafe one-step
branch-protection migration.

## Dependency graph and parallelism

```text
changed paths + profile
          |
          v
       target plan
          |
          +--> basic ------------------------------+
          +--> unit-linux -------------------------+--> repository aggregator
          +--> headless-linux ---------------------+
          +--> ui-linux ---------------------------+
          +--> ui-release-linux -------------------+
          +--> coverage-linux ---------------------+
          |
          +--> quality (independent stable context)
          |
          +--> nix-package ------------------------+--> locked-Nix aggregator
          +--> compact-artifacts ------------------+
          |
          +--> scanners, DCO, PR metadata, docs links (independent workflows)
```

The target jobs do not depend on `basic`, so a five-minute failure is visible
quickly without turning every other lane into serial wall time. They all depend
only on the immutable plan. In hosted CI, `sccache` uses its object-level
GitHub Actions backend. This lets parallel lanes share compiler objects
without creating an immutable multi-gigabyte
archive for every commit. Minimal CI shells default `CARGO_INCREMENTAL=0`:
rustc incremental state is local `target/` data that sccache cannot store, so
hosted gates prefer cacheable compiler objects and print per-lane sccache
hit/miss/non-cacheable statistics. The interactive developer shell keeps
Cargo's normal incremental behavior. Local CI shells use a bounded 2 GiB cache
and the local default remains a bounded 10 GiB shared cache. Rust
feedback lanes do not restore the whole Nix store. Pull requests are read-only
cache consumers. On a trusted `integration` push, the unit lane is the sole
hosted object-cache writer; the other five Rust lanes remain read-only.
GitHub's cache-service write quota is shared by a workflow run, and concurrent
writers otherwise lose throttled objects before sccache can reuse them.
Quality uses a minimal shell without archiving the Nix store, preventing a new
roughly 2 GiB immutable cache whenever a Nix expression changes. The locked
package lane may update its bounded Nix-store cache only on trusted
`integration` pushes; PRs restore it without allocating a branch-scoped copy.
It uses `cache-nix-action` v7 in a new namespace so the noisy v6 archive
observed on PR #165 cannot be reused.

### Freezing dependency and crate layers

The current Nix packages use `buildRustPackage` with the whole repository as
one `src`, so any included workspace source change invalidates the final Rust
derivation. The Cargo vendor derivation can be reused while `Cargo.lock` is
unchanged, but Nix cannot yet substitute an unchanged Oxid crate independently.

Introduce finer reuse as a separately measured package migration:

1. Filter Nix sources to manifests, Rust sources, required build assets, and
   fixtures so documentation and harness files cannot alter Rust store paths.
2. Build third-party dependencies as a lockfile/toolchain/target/profile keyed
   derivation (for example, Crane `buildDepsOnly` plus `cargoArtifacts`).
3. Add source-addressed workspace layers in dependency order: foundation and
   ports; domains; applications; adapters/composition; then UI/headless/apps.
   A layer changes only when its source, feature set, compiler, native inputs,
   or an upstream layer changes.
4. Publish those derivation outputs from trusted `integration`/release builds
   to a signed Nix binary cache. PRs use it only as a substituter; GitHub cache
   remains for the Cargo registry and read-mostly compiler objects, never raw
   `target/` trees.

Do not call a crate frozen merely because its path did not change. Reverse
dependencies, features, build scripts, compiler flags, target triple, native
libraries, and toolchain version are part of the cache identity. Promotion
requires two same-input hosted samples, cache-hit telemetry, and a measured
storage ceiling before any new layer becomes required.

## Change ownership

`scripts/ci/target-plan.mjs` maps paths to `docs`, `harness`, `ci`, `core`,
`headless`, `ui`, `platform`, `compact`, and `build` areas.

- Documentation, harness, and CI-only feature changes run L0 and their
  independent policy/scanner contexts without realizing the Rust/Nix build
  graph.
- UI and headless changes select their own consumer lanes rather than both;
  the UI's optimized artifact audit remains separate from its profile/tests.
- Shared core, platform, and Compact changes select both UI and headless
  consumers because their dependency fan-out is cross-cutting.
- Build/toolchain/lockfile changes and an unavailable or empty diff fail closed
  to every hosted public target.
- Root `run.sh` is the repository-wide gate driver rather than a harness-only
  helper. An edit can change the command owned by every lane, so it is a global
  build input and deliberately selects the full hosted matrix.
- `--targets` adds known hosted lanes on demand; unknown or private target names
  are rejected rather than silently accepted.

## Branch profiles

| Repository event | Effective profile | Gate set |
| --- | --- | --- |
| PR to `integration` | `feature` | L0 plus change-relevant hosted lanes and requested extras |
| push to `integration` | `integration` | every deterministic public hosted lane, in parallel |
| manual workflow | selected `feature`, `integration`, or `release` | impacted, public-full, or public-full respectively; extra hosted targets may be named |
| nightly schedule | release backstop | complete hermetic Nix suite |

`integration` is currently the only writable delivery branch. `main` and
`develop` are read-only under ruleset `21481544`, so implementing distinct
write-time policies for them would contradict current repository authority.
If owners later reintroduce that branch model, map feature branches to
`feature`, `develop` to `integration`, and `main` to `release` in a separate
tracked ruleset migration. “Release” here means all deterministic public
targets; owner-private/device evidence remains a separate explicit checklist.

## Promotion criteria for missing L4 lanes

A target becomes a required hosted lane only after it has: a deterministic
fixture, a documented credential boundary, a provisioned runner, three timing
samples, a hard timeout, bounded disk/cache use, and a named owner for failures.
Until then it is useful evidence on demand, not a gate that can strand every
developer.
