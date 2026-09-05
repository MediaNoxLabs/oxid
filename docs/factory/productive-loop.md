<!-- SPDX-License-Identifier: Apache-2.0 -->

# Productive development loop

This is the operating policy for keeping Oxid's assurance proportional to the
change. It supersedes the “every lens and every build on every push” posture.
The security boundary remains strict; repetition and duplicate authority do
not.

## Service levels and hard bounds

- A draft-direction result should take at most 10 minutes.
- A routine PR should be merge-ready in 35–60 minutes of elapsed time.
- One review agent is the routine default and one automatic review/fix round is
  the limit. A second opinion requires high risk, a disputed finding, or an
  explicit owner request.
- Only one PR candidate is auto-driven remotely by each parent session.
- Keep at most two active managed delivery worktrees per Git common checkout
  on a host. An experiment may use a temporary third worktree only when its
  owner and deletion date are recorded.
- Multiple parents may work different issue branches locally or on other
  hosts. One mutating parent owns each issue worktree; repository merges remain
  serialized.
- A clean gate is evidence, not permission to push directly to a protected
  branch. An authorized agent may merge only an issue-backed PR to its declared
  `milestone-<x.y.z>` through the guarded audit. Humans alone merge to
  `develop` or `main`.

An SLO miss is a process finding. Do not answer it by adding retries, reviewers,
or a second implementation path. Record which phase consumed the time and fix
that phase.

## Two delivery profiles

Profile selection is explicit and local to each Pi invocation, so independent
engineers, local sessions, and cloud workers do not overwrite shared mode
state:

| Profile | Purpose | Required evidence | Remote posture |
| --- | --- | --- | --- |
| `prototype` | Answer one product or technical hypothesis quickly | `basic`, plus an explicitly needed focused unit or headless check | Local only; evidence is provisional and never merge-eligible |
| `production-ready` | Produce a reviewable, merge-eligible increment | Mandatory acceptance and safety evidence, affected fast targets, one review round, hosted CI, and complete finding triage | Guarded milestone flow or human durable-branch handoff |

Invoke the public entrypoint as:

```text
/dev-loop prototype issue <n>
/dev-loop production-ready issue <n>
```

The default is `production-ready`. A prototype aims for first feedback within
three minutes, a focused iteration within ten minutes, and a bounded work item
within one hour. It uses at most one scope/correctness reviewer and does not
infer full Nix, coverage, quality, real Lace ID Portal or Midnight stacks,
desktop/mobile, physical-device, Tailnet, hosted-CI, or review-panel work. Run
one of those only when it is the hypothesis being tested, and still classify
the result as provisional.

A prototype closes with its hypothesis, result, changed paths, checks run,
known gaps, resource use, and promotion plan. Promotion is a deliberate new
`production-ready` invocation: fetch and refresh from the recorded delivery base,
audit every shortcut and known gap, discard provisional gate claims, rebuild
the handoff envelope, recompute the affected targets, and execute the normal
production gates. Do not turn a prototype into a PR by merely pushing its head.

Both profiles retain issue-backed worktrees, branch and commit grammar,
DCO/GPG requirements, secret and custody boundaries, process ownership, and
disk limits. The machine-readable contract is
`.pi/delivery-profiles.json`; `scripts/factory/audit-pi.mjs` rejects drift.

## Delivery targets and concurrent trains

Several milestone trains may stream concurrently. Each session binds one work
item to one explicit base; there is no repository-global “current milestone”
that independent workers can overwrite. Product branches target their selected
`milestone-<x.y.z>`. Factory, harness, CI, documentation, dependency, and
governance branches can target `develop` so factory tuning remains isolated
from every active product train.

One branch is never merged separately into two trains. If both need the same
change, land it through human review in `develop` and create an explicit sync
or backport PR for each train. When stacked work is necessary, keep at most two
active engineering branches and preserve the final milestone target in both
work items. See [the delivery authority](../issue-branch-delivery.md) for train
lifecycle and human promotion rules.

Production-ready uses a 70% routine quality target during the current delivery
phase. This is a throughput budget, not permission to ship a known defect:
acceptance criteria, correctness, security, provenance, and selected required
checks remain 100% complete. Resolve blocking defects and the highest-value
quality improvements within one automatic review round. Preserve remaining
advisory improvements in the PR follow-up comment or an issue; do not change an
otherwise eligible exact head merely to polish it.
Both gates therefore configure `blockCleanOnFindingSeverities: [must-fix]`;
`worth-fixing-now` and `defer` findings remain visible in the disposition
ledger and PR comment without blocking a clean verdict.

## One candidate, two checkpoints

1. Resolve exactly one delivery base. Product work uses its criteria-backed
   `origin/milestone-<x.y.z>`; factory work may use `origin/develop`. Start
   from that fetched ref in a dedicated worktree. Run
   `node scripts/worktree-lifecycle.mjs audit` before creating another.
2. Make a bounded change and run the narrowest meaningful local test.
3. Run the draft gate for scope and correctness. It does not wait for hosted
   CI. Repair blocking findings together. Record bounded non-critical findings
   as linked follow-up issues instead of extending the current iteration.
4. Run the target planner locally against the intended base and head:

   ```bash
   delivery_base="${DELIVERY_BASE:?set DELIVERY_BASE to the recorded origin/... ref}"
   node scripts/ci/target-plan.mjs \
     --base "$(git merge-base HEAD "$delivery_base")" \
     --head HEAD \
     --event pull_request \
     --delivery-profile production-ready
   ```

5. Run the matching local gate, commit once, and push one coherent candidate.
   Do not push after each finding; every push cancels CI and stales exact-head
   evidence.
6. Pre-approval runs one correctness/security review against that candidate and
   waits for the protected contexts once. A bounded non-critical finding is
   complete for this increment only when its follow-up issue and mapping comment
   exist; a second automatic fix/review cycle is forbidden for advisory-only
   findings. Post one current-head receipt with `review-triage.mjs`; a new head
   invalidates it.
7. For a release-profile/high-risk change, an owner request, or a disputed finding, run
   the manually invoked current-head Claude review once after the last edit.
8. Recheck current-head and delivery-base freshness. Use
   `scripts/github/merge-milestone-pr.mjs` for an eligible product increment;
   it audits by default and mutates only with `--execute`. Hand every
   milestone promotion, direct factory PR, and release promotion to a human.
   Return a failed critical audit to remediation.

## Assurance levels and target routing

The planner is conservative and based on changed paths plus an explicit
`feature`, `integration`, or `release` profile. L0 basic evidence is emitted
for every PR within five minutes. L1 unit and L2 headless integration have
ten-minute bounds on the Linux host. A routine feature PR runs unit plus one
affected host consumer: headless for shared/headless/Compact code, or UI for
UI/platform code. Optimized UI release, coverage, quality, and Nix packaging
remain available on demand and run in the complete `develop`/`main` profiles;
Compact source changes retain their artifact check. The complete target/dependency inventory and missing platform
gates live in [the CI target matrix](ci-target-matrix.md).

The workflow keeps the existing required context names as aggregators, so
branch protection never treats an intentionally skipped lane as missing and no
one-step ruleset migration is needed.

The existing required scanner context remains path-independent because a
secret can be committed in any file. It is already a short parallel check and
is not on the Rust/Nix critical path.

Build/toolchain/lockfile changes and unknown or unavailable diff state select
every public hosted target. Shared core changes use the headless consumer on
feature PRs; focused components do not pay for unrelated consumers. The nightly
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
requires a clean head already integrated into its recorded milestone or
`origin/develop` delivery base and at least seven days old. The audit accepts
direct Git ancestry first. For a squash merge,
it uses one exact merged GitHub PR head/base/merge-commit match and verifies that
merge commit against a remotely observed, current delivery base. Stale,
unavailable, malformed, or ambiguous hosted evidence fails closed and is shown
in the `mergeProof` field. `audit` remains mutation-free but is no longer purely
local: non-ancestor heads make bounded read-only `git ls-remote` and authenticated
`gh api graphql` calls. Without network access, a logged-in `gh`, or a current
local delivery ref, those heads report `unavailable`; direct ancestry and the
rest of the inventory remain usable. The human table appends `proof` as its last
column so the pre-existing column positions remain stable; prefer `--json` for
automation:

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
per-required-check queue/execution time, pushes after first CI, failed/canceled
attempts, and peak target/worktree size. A zero
means a measured zero, never “unknown.” When the active harness exposes no
exact token counters, record `tokens: null`; the audit reports that coverage
gap and excludes it from token distributions and totals rather than inviting
an estimate. Token buckets must be non-overlapping; cached input cannot also be
counted as ordinary input.

Raw records live owner-private beneath the common Git directory and may contain
usage counts, but never prompts, transcripts, credentials, identifiers, raw
commands/output, or provider billing/account details. PR comments and committed
reports contain aggregates only. The Quality Steward audits weekly and after an
incident, reviews median/p90 and SLO violations monthly, and files bounded
findings. Keep raw records for 90 days, then remove them only through an
explicit owner maintenance action; the audit command is read-only.
