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

## Work-item record and supervisor audit

The closed [v1 schema](work-item-metrics-v1.schema.json) binds one record to
`MediaNoxLabs/oxid`, an issue, an optional PR represented explicitly as a
number or `null`, and an exact 40-character head SHA. It retains canonical
start/completion/recording timestamps; development, review, validation, CI,
and elapsed durations; named validation outcomes; review session/turn/tool-call
counts; exact input/output/cache token counters when the active harness exposes
them; push/failure/cancellation counts;
required-check wall time plus per-check queue/execution outcomes; peak target
and whole-worktree bytes; and the selected profile, areas, and targets.

Create a template outside the checkout, replace every non-nullable `null`
placeholder and the empty routing target list with measured values, then write
it atomically.
All three draft timestamps are `null`; the template cannot guess when work
started. The draft intentionally fails validation until measurements are
supplied, so an untouched template cannot turn unknown work into a row of
zeroes. Zero is a measured zero, never a stand-in for unknown. Fill review
counters when the active harness exposes exact values; otherwise individual
`review.sessions`, `review.turns`, or `review.toolCalls` values must remain
`null`, never `0`. The same rule applies to the aggregate `tokens: null` value:

```bash
node scripts/factory/metrics.mjs template \
  --issue <n> --pr <n> --head "$(git rev-parse HEAD)" \
  > /private/path/outside-the-checkout/metrics.json
node scripts/factory/metrics.mjs write \
  --record /private/path/outside-the-checkout/metrics.json
```

The redirect target and any explicit `--output-dir` must remain outside the
worktree; never place raw telemetry in a commit candidate.

Token buckets must be non-overlapping: `input` means uncached input, while
`cacheRead` and `cacheWrite` contain their respective cache buckets. If a
provider exposes cached input only as a subset of `input`, the agent cannot
derive this decomposition exactly and must leave `tokens: null`. This keeps the
aggregate from double-counting one prompt under incompatible provider
accounting conventions.

`startedAt` is the first work-item action, `completedAt` is merge-ready (or the
terminal stopped state), and `recordedAt` is the later capture time. Phase
durations are measured wall clocks and may overlap. `totalElapsedMs` must equal
`completedAt - startedAt`, and the deliberately redundant `phases.ciMs` must
equal `ci.wallTimeMs`. `ci.wallTimeMs` spans the first selected
required-check submission through the last completion for the exact head.
Each `ci.checks` entry records one required context with queue time separated
from runner execution time. `ci.canceledRuns` counts canceled hosted run
attempts across the work item; it is not derived from the final exact-head
check outcomes. Use the hosted target identifier as the check name for a lane
(`basic`, `unit-linux`, and so on); protection-only contexts use other bounded
names. The target-budget SLO compares each selected target check's execution
duration with that target's authoritative budget. Provider queue time remains
visible in per-check distributions and `ci.wallTimeMs` without creating a false
execution-budget violation. Named validation durations/outcomes are aggregated separately
so slow or flaky local gates remain visible. `peakWorktreeBytes` is physical worktree usage
without following symlinks or including the common Git directory/shared caches;
`peakTargetBytes` is its worktree-local Rust target subset.

The CLI writer rejects unknown or missing fields, malformed/non-canonical values,
negative or non-finite counts, inconsistent timestamps/durations, duplicate
routing entries, unknown hosted targets, secret-bearing strings, oversized
records/arrays, the wrong repository, and a head that does not equal a clean
current checkout, and rejects raw record/output paths inside the worktree
(while allowing the default common-Git private store). Semantic validation is authoritative for cross-field rules
that standard JSON Schema cannot express: required-check counts must match the
check array and outcomes, target bytes cannot exceed whole-worktree bytes, and
the redundant elapsed/CI values must agree. The committed JSON Schema remains
the closed structural and bounded interchange contract. The writer uses mode
`0600` through a
temporary file plus an atomic no-overwrite link (or atomic rename for an
explicit correction). The default owner-private store is
`<git-common-dir>/oxid-factory/metrics-v1`, so linked worktrees share records
without placing them in the repository.

Use counters emitted by the active harness, not estimates derived from prompt
or transcript text. Sum parent and child token/turn/tool-call counters exactly
once according to that harness's accounting boundary; never add a child total
again when the parent already includes it. An unavailable review counter stays
`null` independently of the other review counters; do not infer it from gate
comments, transcript length, or elapsed time. Audits report token and
per-review-counter coverage and exclude unavailable values from distributions
and totals. The committed schema governs work-item inputs; aggregate audit
output is produced and contract-tested directly rather than accepted as a raw
record. Validation
entries use bounded labels such as `repository-contract`, never raw commands
or output.

The Quality Steward or periodic supervisor runs this weekly, after a harness
incident, and before monthly tuning:

```bash
node scripts/factory/metrics.mjs audit --json
```

Audit reads records once and performs no model call, retry, merge, branch or
GitHub mutation, cache/worktree deletion, or process cleanup. It reports valid
and invalid counts, missing required fields, median/p90 distributions, totals,
per-check queue/execution distributions, overflowed totals, duplicate identities,
and safe work-item identifiers for SLO and 90-day-retention findings. Public
reports may copy only these aggregates. Raw prompts, transcripts, credentials,
private identifiers, commands/output, and provider cost/account details are
forbidden. Owner-private raw records are retained for 90 days; deletion is an
explicit maintenance task, never an audit side effect.

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
| L3 UI lane | ≤ 20 min | 20–25 min | > 25 min |
| L3 quality lane | ≤ 20 min | 20–25 min | > 25 min |
| L3 coverage / UI release lane | ≤ 25 min | 25–35 min | > 35 min |
| L3 Compact artifact lane | ≤ 30 min | 30–40 min | > 40 min |
| L3 locked Nix package lane | ≤ 45 min | 45–55 min | > 55 min |
| Routine PR to merge-ready | ≤ 60 min | 60–90 min | > 90 min |
| Automatic review sessions per routine PR | ≤ 4 | 5–6 | > 6 |
| Pushes after first hosted CI starts | 0 | 1 | > 1 |
| Active managed delivery worktrees per Git common checkout/host | ≤ 2 | 3 | > 3 |
| Worktree-local target usage | ≤ 100 GiB | 100–200 GiB | > 200 GiB |

Amber requires a backlog item; red blocks new `factory:ready` labels until a
mitigation item is claimed.

The supervisor's exact per-target CI budgets are contract-tested against the authoritative
[CI target and dependency matrix](ci-target-matrix.md); every hosted target
must be named explicitly, with no fallback for a future target.

The 20/25/30/45-minute extended-lane rows above align the supervisor thresholds
with the already-active hard budgets in that target matrix as of 2026-08-28;
they do not change workflow timeouts in this metrics-only slice.

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
Nix cache in a fresh namespace. The first staged run then proved that the full
developer shell plus workspace-wide clippy could not satisfy L0: it was still
compiling at the five-minute bound. L0 therefore uses a minimal `ci-rust`
shell and compiles/lints the dependency-light architecture/domain canary; L1
and component lanes remain authoritative for complete source and test
compilation. UI/app tests belong only to the UI lane, removing their native
GTK/WebKit closure and duplicate execution from L1. On that first hosted run,
the already-separated lanes completed Compact artifacts in 3m13, headless in
7m57, Quality in 9m56, and coverage in 10m11. The old full-shell unit shape
hit its 10-minute ceiling, directly motivating the ownership split. The old
UI shape hit its 15-minute ceiling while combining profile/type checks with a
full optimized release artifact, so those now run as independent UI and
`ui-release-linux` lanes. The separated Nix package completed in 18m19. Do
not raise the old monolith or L0 timeout as a substitute for isolating phases.
The first minimal-shell run then failed in about one minute because `rg` was
implicit on the development host but absent from the hosted image; `ripgrep`
is now an explicit shared CI-shell dependency.

Cache telemetry added after that run exposed two previously silent failure
modes. Cargo's default incremental debug compilation made workspace/path
crates ineligible for sccache, and six concurrent GitHub-cache writers then
dropped every attempted write while the repository held 8.8 GB across eight
cache entries (including three approximately 2 GB immutable quality-devshell
archives). Minimal CI shells now default `CARGO_INCREMENTAL=0`, every Rust lane
prints sccache statistics, the unit lane is the single writer, and the other
Rust lanes are read-only consumers. Pull requests do not write either compiler
objects or Nix-store archives; only a trusted `integration` push may seed the
unit compiler cache and locked-package store cache. Quality uses an uncached
minimal shell; the locked package lane remains the only whole-store GitHub
cache while a proper signed Nix substituter is unprovisioned.

The first fully read-only PR sample also exposed an optimistic UI budget rather
than a product failure: its UI/app tests passed, then GitHub canceled remaining
mobile feature-profile compilation at 15m18. The L3 ceiling is 20 minutes so a
cold cache remains a supported fallback. L0, L1, and L2 keep their 5/10/10
minute bounds; the trusted seed and layered-cache work must reduce actual time
rather than hiding it behind a larger fast-gate budget.

Local entry-point validation on the same Apple development host and a fresh
worktree target completed the original `basic` in 1m41 and original `unit`
(compile plus all unit tests) in 3m11. After isolation, the minimal-shell L0
completed warm in 12 s and L1 completed cold in 1m46. The headless integration
target completed in 49 s after the unit lane had populated dependency outputs.
These are local command-shape measurements, not substitutes for three hosted
samples.
