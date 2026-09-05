<!-- SPDX-License-Identifier: Apache-2.0 -->

# Integration promotion retrospective, 2026-09-03

This retrospective covers the temporary `integration` delivery stream from
2026-08-28 through its promotion to `develop` in PR #258. It uses GitHub pull
request facts and nine valid private v1 factory-metric records. Private records
remain local; only aggregates are recorded here.

## Evidence

| Signal | Result |
| --- | ---: |
| Merged PRs sampled | 38 (37 to `integration`, one promotion to `develop`) |
| PR open-to-merge time | median 39.2 min; p90 953.4 min |
| PRs over 60 min | 16 of 38 |
| Commits per PR | median 2; p90 17; maximum 55 |
| PRs over 10 commits | 7 of 38 |
| Changed files per PR | median 12.5; p90 31; maximum 362 |
| Private work-item elapsed time | median 63.1 min; p90 2,226.5 min |
| Hosted CI wall time | median 16.8 min; p90 20.5 min |
| Pushes after first CI | median 0; p90 6 |
| Failed / canceled local attempts | 11 / 5 |
| Canceled hosted CI runs | 11 |
| Worktree-local target size | median 0.95 GiB; p90 31.1 GiB |
| Token telemetry | unavailable in all 9 records |

The five-record hosted-lane sample reported median execution of 1m37 for
basic, 7m38 for unit, 5m09 for headless, 7m46 for coverage, 5m46 for quality,
17m52 for UI, 15m44 for optimized UI release, and 12m50 for the Nix package.
Coverage p90 was 15m40. These independent lanes are useful backstops, but
running all of them for routine feature work makes the slowest compile lane the
delivery clock.

The 2026-09-03 Pi audit also found 29 active worktrees and 373.1 GiB of
worktree-local targets. Only two worktrees were both clean, remotely proven
merged, and older than seven days. That is an admission/ownership backlog, not
authority for bulk deletion; issue #198 owns reconciliation.

## What worked

- The impact planner kept documentation, harness, and CI-only PRs on the basic
  lane when their paths were classified correctly.
- Required provenance, security scanning, exact-head checks, and guarded merge
  authority stayed independent of advisory metadata.
- Shared Nix inputs and `sccache` made the basic, unit, and headless lanes fit
  their intended bounds in the retained sample.
- Prototype and production-ready profiles stopped mobile, Tailnet, and real
  service evidence from being inferred for every change.

## What reduced throughput

- Review recommendations were repeatedly converted into edits, pushes, stale
  exact-head reviews, and canceled CI even when no correctness or security
  defect remained. Large examples reached 28, 47, and 55 commits.
- Routine Rust PRs selected coverage, quality, optimized UI release, and both
  public host consumers. The slowest lane commonly exceeded the ten-minute PR
  target while duplicating the complete `develop` backstop.
- Refinement required every optional matrix row to become complete, so polish
  could silently become scope.
- Resource admission correctly detects pressure, but unresolved historical
  worktrees mean a new session starts in a red operational state.
- Token measurement is still absent, so token policy is bounded by configured
  limits rather than calibrated from observed use.

## Decisions for the next delivery phase

1. Use a 70% routine quality and coverage target. Mandatory acceptance,
   correctness, security, provenance, and required evidence remain 100%.
2. Run one automatic review/fix round. Advisory-only residuals become a PR
   follow-up comment or issue and never force another exact-head CI cycle.
   Only `must-fix` severity blocks a clean gate verdict.
3. Feature PRs run basic, unit, and one affected host consumer. Coverage,
   quality, optimized UI release, and Nix packaging remain available on demand
   and run in complete `develop`/`main` profiles; Compact changes retain their
   artifact lane.
4. Reduce Pi subagent turn, spawn, and token ceilings. High-risk or disputed
   work can still use an explicitly requested second opinion.
5. Keep issue #198 as the owner-aware disk/worktree cleanup stream. Do not
   weaken admission or delete unproven work to make its audit green.

## Revisit criteria

Review this policy after ten additional valid production-ready metric records
or two weeks, whichever happens first. Reconsider the 70% threshold if escaped
defects rise, required `develop` backstops stay red, or routine PR median no
longer improves. Issue #181 owns the broader SLO calibration and the missing
token-telemetry decision.
