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
- The final merge is always human. A clean gate is evidence, not permission to
  mutate the delivery branch.

An SLO miss is a process finding. Do not answer it by adding retries, reviewers,
or a second implementation path. Record which phase consumed the time and fix
that phase.

## One candidate, two checkpoints

1. Start from fetched `origin/integration` in a dedicated worktree. Run
   `node scripts/worktree-lifecycle.mjs audit` before creating another.
2. Make a bounded change and run the narrowest meaningful local test.
3. Run the draft gate for scope and correctness. It does not wait for hosted
   CI. Fix accepted direction findings together.
4. Run the tier command locally against the intended base and head:

   ```bash
   node scripts/ci/change-tier.mjs \
     --base "$(git merge-base HEAD origin/integration)" \
     --head HEAD
   ```

5. Run the matching local gate, commit once, and push one coherent candidate.
   Do not push after each finding; every push cancels CI and stales exact-head
   evidence.
6. Pre-approval runs correctness/security review against that candidate and
   waits for the protected contexts once.
7. For a `full` high-risk change, an owner request, or a disputed finding, run
   the manually invoked current-head Claude review once after the last edit.
8. Stop at merge. The human operator checks current-head freshness and merges
   or returns the PR for remediation.

## Validation tiers

The classifier is conservative and based only on changed paths. The workflow
keeps existing required context names so no branch-protection migration is
needed.

| Tier | Local command | Hosted behavior |
| --- | --- | --- |
| `docs` | focused docs check | no Nix, Rust build, or package build |
| `harness` | `./run.sh repository --strict` | hermetic Node contract tests; no ignored local Pi installation |
| `rust` | `nix develop --command ./run.sh core --strict` | workspace contracts, clippy/tests, and quality |
| `full` | `nix develop --command ./run.sh --light --strict` | full gate, coverage, locked packages/artifacts, and quality |

The existing required scanner context remains path-independent because a
secret can be committed in any file. It is already a short parallel check and
is not on the Rust/Nix critical path.

Workflow, toolchain, Nix, lockfile, Compact contract, identity, protocol,
wallet, and custody changes are `full`. Unknown or unavailable diff state also
selects `full`. The nightly is the backstop for complete hermetic validation,
not an excuse to weaken a change-relevant PR gate.

## State and disk lifecycle

Pi packages are installed once beneath the common checkout and resolved from
linked worktrees. A running Pi process must be restarted after `.pi/`,
`.devloops`, or package-pin changes because already-loaded instructions and
extensions do not update in place.

Rust targets stay worktree-local. Compilation is reused through one 10 GiB
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

For each delivered PR, retain: changed-path tier, draft review duration,
pre-approval duration, hosted CI wall time, number of pushes after first CI,
automatic reviewer sessions, canceled runs, peak worktree target size, and
whether external review was required. Review the aggregate monthly. Raw token
or cost data belongs in private operational telemetry, not public PR comments.
