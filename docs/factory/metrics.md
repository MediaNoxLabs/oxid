# Factory Metrics and Baselines

The factory watches a small set of measurements so that complexity growth is
caught as a trend, not as a crisis. The Quality Steward refreshes this file;
each entry is dated and states its measurement environment.

## Why these metrics

- **Local gate time** is the agent/engineer inner loop; when it grows, every
  work item slows down.
- **CI wall time** is bounded at 75 minutes (see AGENT.md); the cold gate has
  already consumed ~59 minutes once. Approaching the bound cancels
  otherwise-green runs.
- **Per-crate build time** reveals decomposition problems (a crate growing
  into a bottleneck) before they dominate the critical path.

## Baselines — 2026-08-18, develop @ ade6416

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

GitHub Actions (`CI` workflow, single job "Build, Lint, Test, and
Architecture"):

| Step | Recent duration |
| --- | --- |
| Run repository gate | ~16 min |
| Build the locked Nix package | ~26 min |
| Whole job (recent runs) | 44–67 min, bound 75 min |
| `Quality` workflow | 8–26 min |
| `Scan` / `Documentation links` | 1–4 min |

## Budgets (proposed)

| Measurement | Green | Amber | Red |
| --- | --- | --- | --- |
| Warm local strict-light gate | ≤ 2 min | 2–5 min | > 5 min |
| Cold `cargo check --workspace` | ≤ 2 min | 2–5 min | > 5 min |
| CI job wall time | ≤ 45 min | 45–60 min | > 60 min |
| CI headroom to bound | ≥ 25 min | 10–25 min | < 10 min |

Amber requires a backlog item; red blocks new `factory:ready` labels until a
mitigation item is claimed.

## Trend log

| Date | develop SHA | Cold check | Test (cold) | CI job | Notes |
| --- | --- | --- | --- | --- | --- |
| 2026-08-18 | ade6416 | 74 s | 119 s | 44–67 min | First baseline; CI long pole is the locked Nix package build (~26 min) + uncached repository gate (~16 min). |
