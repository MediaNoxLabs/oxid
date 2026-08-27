# Factory Metrics and Baselines

The factory watches a small set of measurements so that complexity growth is
caught as a trend, not as a crisis. The Quality Steward refreshes this file;
each entry is dated and states its measurement environment.

## Why these metrics

- **Local gate time** is the agent/engineer inner loop; when it grows, every
  work item slows down.
- **CI wall time** is target-based. Documentation, harness, and CI-only feature
  changes should not realize the Rust/Nix closure; affected targets fan out in
  parallel and `integration` remains the complete hosted backstop.
- **Time to merge-ready** captures review, canceled runs, and repeated pushes
  that a single job duration hides.
- **Per-crate build time** reveals decomposition problems (a crate growing
  into a bottleneck) before they dominate the critical path.

## Historical baselines — 2026-08-18, `develop` @ `ade6416`

Local, Apple M2 Max (12 cores), warm cargo registry, fresh worktree target:

| Target | Time |
| --- | --- |
| `cargo fmt --all --check` | 1 s |
| `scripts/check-architecture.sh` | 1 s |
| `scripts/check-midnight-sources.sh` | 1 s |
| `cargo check --workspace` (cold target dir) | 74 s |
| `cargo check --workspace` (warm no-op) | 1 s |
| `cargo clippy --workspace --all-targets -- -D warnings` (warm) | 5 s |
| `cargo test --workspace` (cold build + run) | 119 s |
| `cargo test --workspace` (warm, run only) | 17 s |
| `./run.sh --light --strict` (warm end-to-end) | 52 s wall * |

\* exits 1 outside `nix develop`: the light gate requires `cargo-llvm-cov`
even though coverage is not part of `--light`. Tracked as a finding.

GitHub Actions, before the caching and decomposition work (single job
"Build, Lint, Test, and Architecture"):

| Step | Duration |
| --- | --- |
| Run repository gate | ~16 min |
| Build the locked Nix package | ~26 min |
| Whole job | 44–67 min, bound 75 min |
| `Quality` workflow | 8–26 min |
| `Scan` / `Documentation links` | 1–4 min |

## Historical post-pipeline measurement — 2026-08-20, `develop` @ `319ca5d`

The CI decomposition landed in three steps: parallel gate/build jobs with
Nix-store and cargo caches, a single authoritative test execution per push,
and the hermetic sandbox suite moved to a nightly `nix flake check`.

| Measurement | Before | After |
| --- | --- | --- |
| `CI` workflow, warm caches | 44–67 min | **21–28 min** |
| `CI` workflow, cold caches | 59–67 min | 39–40 min |
| `Documentation links` | 3–4 min | **~40 s** |
| `Quality` | 8–26 min | 7–9 min |
| Hermetic `nix flake check` | never ran in CI | nightly, 57–60 min |

The docs link check dropped an order of magnitude because it moved to a
lychee-only devshell instead of realizing the compiler and Compact artifact
closure. The nightly is the one that changed character rather than duration:
the flake checks previously ran nowhere.

## Active budgets

| Measurement | Green | Amber | Red |
| --- | --- | --- | --- |
| Warm local strict-light gate | ≤ 2 min | 2–5 min | > 5 min |
| Cold `cargo check --workspace` | ≤ 2 min | 2–5 min | > 5 min |
| L0 basic envelope | ≤ 5 min | 5–7 min | > 7 min |
| L1 unit / L2 headless host lane | ≤ 10 min | 10–15 min | > 15 min |
| L3 UI lane | ≤ 15 min | 15–20 min | > 20 min |
| L3 coverage / quality / artifact lane | ≤ 25 min | 25–35 min | > 35 min |
| Routine PR to merge-ready | ≤ 60 min | 60–90 min | > 90 min |
| Automatic review sessions per routine PR | ≤ 4 | 5–6 | > 6 |
| Pushes after first hosted CI starts | 0 | 1 | > 1 |
| Active managed delivery worktrees | ≤ 2 | 3 | > 3 |

Amber requires a backlog item; red blocks new `factory:ready` labels until a
mitigation item is claimed.

## Historical trend log

These immutable measurements predate `integration` becoming the sole delivery
branch; `develop` identifies the branch on which the evidence was collected,
not a current delivery instruction.

| Date | Historical `develop` SHA | Cold check | Test (cold) | CI job | Notes |
| --- | --- | --- | --- | --- | --- |
| 2026-08-18 | ade6416 | 74 s | 119 s | 44–67 min | First baseline; CI long pole is the locked Nix package build (~26 min) + uncached repository gate (~16 min). |
| 2026-08-20 | 319ca5d | 74 s | 119 s | 21–39 min | Caching, job parallelism, single test execution, and the nightly hermetic split all landed. Local targets unchanged — the win was entirely in pipeline shape, not compile time. |

## Current incident baseline — 2026-08-27, PR #165

The exact-head monolithic repository gate reached its 45-minute timeout and
was canceled by GitHub seconds after coverage printed its table. Its command
ran serially for 44m09; the instrumented coverage compile alone took 6m12. In
the same run, Quality completed in 9m15 and the combined locked Nix
package/Compact job completed in 21m30. The repository log also emitted about
100 MB of failed Nix-store tar extraction diagnostics before compilation.

The staged workflow removes whole-store restoration from Rust feedback lanes,
upgrades `actions/cache` to v6.1 and `cache-nix-action` to v7, and starts the v7
Nix cache in a fresh namespace. This is the baseline for the new per-lane
timers; do not raise the old monolith's timeout as a substitute for isolating
its phases.

Local entry-point validation on the same Apple development host and a fresh
worktree target completed `basic` in 1m41 and `unit` (compile plus all unit
tests) in 3m11. The headless integration target completed in 49 s after the
unit lane had populated dependency outputs. These are local command-shape
measurements, not substitutes for three hosted samples.
