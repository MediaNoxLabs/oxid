<!-- SPDX-License-Identifier: Apache-2.0 -->

# Productive development loop

This is the operating policy for keeping Oxid's assurance proportional to the
change. It supersedes the “every lens and every build on every push” posture.
The security boundary remains strict; repetition and duplicate authority do
not.

## Service levels and hard bounds

- A draft-direction result should take at most 10 minutes.
- A routine PR should be merge-ready in 35–60 minutes of elapsed time.
- At most two review agents run concurrently and at most four automatic review
  sessions are expected across ordinary draft and pre-approval gates.
- Only one PR candidate is auto-driven remotely at a time.
- Keep at most two active managed delivery worktrees. An experiment may use a
  temporary third worktree only when its owner and deletion date are recorded.
- A clean gate is evidence, not permission to mutate the delivery branch. An
  agent may merge only an issue-backed `integration` PR when the active owner
  request explicitly authorizes it and the guarded merge audit passes.

An SLO miss is a process finding. Do not answer it by adding retries, reviewers,
or a second implementation path. Record which phase consumed the time and fix
that phase.

## One candidate, two checkpoints

1. Start from fetched `origin/integration` in a dedicated worktree. Run
   `node scripts/worktree-lifecycle.mjs audit` before creating another.
2. Make a bounded change and run the narrowest meaningful local test.
3. Run the draft gate for scope and correctness. It does not wait for hosted
   CI. Fix accepted direction findings together.
4. Run the target planner locally against the intended base and head:

   ```bash
   node scripts/ci/target-plan.mjs \
     --base "$(git merge-base HEAD origin/integration)" \
     --head HEAD \
     --event pull_request
   ```

5. Run the matching local gate, commit once, and push one coherent candidate.
   Do not push after each finding; every push cancels CI and stales exact-head
   evidence.
6. Pre-approval runs correctness/security review against that candidate and
   waits for the protected contexts once.
7. For a release-profile/high-risk change, an owner request, or a disputed finding, run
   the manually invoked current-head Claude review once after the last edit.
8. Recheck current-head freshness. Hand off to the human operator, or use the
   guarded integration-only merge wrapper when the active owner request
   explicitly authorizes automated merge. Return any failed audit to
   remediation.

## Assurance levels and target routing

The planner is conservative and based on changed paths plus an explicit
`feature`, `integration`, or `release` profile. L0 basic evidence is emitted
for every PR within five minutes. L1 unit and L2 headless integration have
ten-minute bounds on the Linux host. L3 UI profiles/tests, the optimized UI
release audit, coverage, quality, Nix package, and Compact artifact lanes run
independently when their component or profile selects them. The complete target/dependency inventory and missing platform
gates live in [the CI target matrix](ci-target-matrix.md).

The workflow keeps the existing required context names as aggregators, so
branch protection never treats an intentionally skipped lane as missing and no
one-step ruleset migration is needed.

The existing required scanner context remains path-independent because a
secret can be committed in any file. It is already a short parallel check and
is not on the Rust/Nix critical path.

Build/toolchain/lockfile changes and unknown or unavailable diff state select
every public hosted target. Shared core changes select both headless and UI
consumers; focused components do not pay for unrelated consumers. The nightly
is the backstop for complete hermetic validation, not an excuse to weaken a
change-relevant PR gate.

## State and disk lifecycle

Pi packages are installed once beneath the common checkout and resolved from
linked worktrees. A running Pi process must be restarted after `.pi/`,
`.devloops`, or package-pin changes because already-loaded instructions and
extensions do not update in place.

Rust targets stay worktree-local. Compilation is reused through one bounded 10 GiB
`sccache`, so an old target can be deleted without paying the entire historical
compile cost again.

Audit is read-only:

```bash
node scripts/worktree-lifecycle.mjs audit
node scripts/worktree-lifecycle.mjs audit --json
```

Mutation is intentionally awkward and single-target. It requires an exact
registered path, the expected head, and `--execute`. Worktree removal also
requires a clean head already merged into `origin/integration` and at least
seven days old:

```bash
node scripts/worktree-lifecycle.mjs clean-target \
  --path /absolute/worktree --expect-head <sha> --execute

node scripts/worktree-lifecycle.mjs remove \
  --path /absolute/worktree --expect-head <sha> \
  --older-than-days 7 --execute
```

Never bulk-delete worktrees based only on branch names or “gone” upstreams.
Preserve dirty/untracked files and open PR heads first.

## Failure and cancellation rules

- On cancellation, the owner of a spawned process group terminates its children
  and escalates to `KILL` after a bounded grace period. Tests must put cleanup
  in an exit trap, not only after the happy-path assertion.
- A canceled hosted run is not a failed product gate. Inspect only the latest
  run for the current head.
- A provider, transport, or pinned-runtime failure is not a code finding. Retry
  once only when the operation is idempotent and the same head remains current;
  otherwise preserve state and stop.
- If a fix changes the head, accumulate all known findings before starting the
  next remote cycle.

## Metrics to retain

For each issue/PR/head, leave one v1 record through
`scripts/factory/metrics.mjs`. Retain routing, phase and validation durations,
hosted CI wall time, review sessions/turns/tool calls, harness token counters,
pushes after first CI, failed/canceled attempts, and peak target size. A zero
means a measured zero, never “unknown.” When the active harness exposes no
exact token counters, record `tokens: null`; the audit reports that coverage
gap and excludes it from token distributions and totals rather than inviting
an estimate.

Raw records live owner-private beneath the common Git directory and may contain
usage counts, but never prompts, transcripts, credentials, identifiers, raw
commands/output, or provider billing/account details. PR comments and committed
reports contain aggregates only. The Quality Steward audits weekly and after an
incident, reviews median/p90 and SLO violations monthly, and files bounded
findings. Keep raw records for 90 days, then remove them only through an
explicit owner maintenance action; the audit command is read-only.
