<!-- SPDX-License-Identifier: Apache-2.0 -->

# Factory runbook — phase 1

How to actually run the factory on this repository. The charter says *why*
([charter.md](charter.md)), the FSM says *what state work is in*
([fsm.md](fsm.md)); this says *what to type* and *what will refuse to work*.

Phase 1 is deliberately narrow: **bounded review, impact-routed validation, and
a guarded delivery.** Routine changes use two draft lenses and two final
lenses; independent current-head review is reserved for high-risk work. Nothing
routes through a coordination server.

## What is installed, and from where

| Piece | Version | Source |
| --- | --- | --- |
| `pi-coding-agent` | Nix-pinned | immutable nixpkgs input in `flake.lock`; executable supplied by `devShells.default` |
| `dev-loops` | `0.9.0` | `.pi/settings.json` → project-local `.pi/npm` |
| `pi-subagents` | `0.42.1` | same |
| `@input-output-hk/agent-review-pi` | `0.5.0` | same, **GitHub Packages — needs a token** |
| `agent-review` loader skill | repository | `.pi/skills/agent-review/SKILL.md` |

The devshell's `shellHook` reads `.pi/settings.json`, compares each exact pin
against the common checkout's `.pi/npm/node_modules/<pkg>/package.json`, and
installs only what is missing or mismatched. Linked worktrees reuse that one
installation through one topology-checked `.pi/npm` link instead of creating a
second mutable package tree each. A pre-existing real directory or foreign link
is rejected for manual inspection, never deleted by shell entry. CI skips Pi
tooling entirely. The private review package is skipped with a printed notice
when no token is present, so the shell still works without one.
After exact-pin reconciliation, shell entry defaults Pi startup to offline mode;
this prevents Pi's own package manager from racing the common-store authority or
retrying a missing optional package. Explicit package maintenance may unset it.

**To get the review package**, export a GitHub token with `read:packages`
before entering the shell — `GITHUB_TOKEN`, `GH_TOKEN`, or `GH_TOKENS` are all
accepted, in that order of preference:

```bash
export GH_TOKEN="$(gh auth token)"   # if your gh login carries read:packages
./bootstrap.sh
```

Never write that token into repository configuration or diagnostics.

The pinned `agent-review-pi` extension registers correctly, but its `0.5.0`
skill frontmatter contains an unquoted YAML colon. Pi `0.84.0` therefore omits
the bundled skill from runtime discovery. The tracked `agent-review` loader is
a narrow compatibility shim: it checks the exact package version, then tells Pi
to read and follow the package's complete skill. It does not copy or change the
review policy. Remove it only after a reviewed package update exposes the
bundled skill directly.

Validate shell entry, the exact private package, all native review-tool
registrations, and runtime skill discovery without an LLM call or GitHub
mutation:

```bash
./bootstrap.sh --configure-git
./bootstrap.sh --check
```

The smoke uses the same bounded resolver as the tracked dev-loop wrappers: it
checks the active Git root first and, for a registered linked worktree only,
the common checkout second. Every configured npm package must match its exact
name and version and remain contained by one of those roots. The smoke does not
copy, link, or install a second package tree in the worktree.

Configure and audit the bounded package policy before the first start:

```bash
./bootstrap.sh --configure-pi
./bootstrap.sh --configure-git
./bootstrap.sh --check
./bootstrap.sh --audit-pi
./bootstrap.sh --pi
```

The default routine model is `openai-codex/gpt-5.6-terra:medium`; select a
stronger or different provider explicitly for a session when the issue risk
justifies it. `--pi` starts inside the pinned shell and refuses configuration
drift. `./bootstrap.sh -- <command>` runs another one-off repository command
there. See [pi-runtime-audit.md](pi-runtime-audit.md) for the exact budgets,
global-package config boundary, measured storage, and upgrade canaries. See
[worker-topology.md](worker-topology.md) before starting a second local session
or attaching a worker from another engineer or cloud host.

`--configure-git` copies the tracked contribution dispatchers into stable,
private Git-common state and sets only repository-local OpenPGP signing
defaults. It requires an existing author identity and signing-key selection,
refuses to replace another hook manager, and never reads or generates a key.
`--check` validates both the Pi runtime and the local hook installation.

## Three concurrency mechanisms, which are easy to confuse

This is the part most likely to be misread, because all three look like
"running several agents at once" and they compose rather than compete.

| | What fans out | Configured in | Who decides |
| --- | --- | --- | --- |
| **Gate fan-out** | Review **angles** over one diff — draft `scope`/`correctness`, final `correctness`/`security` | `.devloops` → `refinement.fanOut: 2`, `mode: parallel`, `roles` | dev-loops, automatically at a gate |
| **Sub-agent delegation** | Child **pi sessions** with their own jobs | `.pi/subagent-policy.json`, installed to the package's user-level config | the agent, when asked |
| **Panel review** | Multiple **requested reviewers** on a PR | GitHub review requests + the `ai-review` label | a human, by requesting review |

**Gate fan-out** is the one that runs without being asked. `refinement.fanOut`
is how many angle reviews run concurrently; `roles` is the pool they are drawn
from. Both refinement and gate concurrency are capped at two. Low-signal
refinement stops after two quiet rounds instead of spending a third round to
rediscover the same result.

**Sub-agent delegation** is foreground by default, caps concurrency at two,
session spawns at eight, and requires explicit async intent. It ships builtins
including `scout` (codebase recon),
`researcher` (external facts with sources), `worker` (implementation),
`reviewer` (review and small fixes), `oracle` (second opinion, edits nothing),
`delegate` (general). Installing the extension **does not** start a background
reviewer; it gives the session a delegation tool. If every implementation
should be reviewed, the project instructions have to say so. Rule of thumb from
the package: `scout` before you understand the code, `researcher` before you
trust an external fact, `worker` to implement, `reviewer` to check, `oracle`
when the decision itself is the risky part.

**Panel review** is `agent-peer-review`'s mechanism: when several reviewers are
requested, the **first to claim becomes the anchor** and posts the primary
review; every later claimant is an **enricher** that adds one consolidated
second opinion after the primary lands. That ordering is what stops five agents
posting five overlapping reviews.

## The label profile

The seven `factory:*` FSM labels are synchronized from the tracked,
dry-run-by-default definition:

```bash
node scripts/github/sync-factory-labels.mjs
node scripts/github/sync-factory-labels.mjs --execute
```

Creating the labels does not move an issue into the factory. While admission is
red, no issue may receive `factory:ready`.

`agent-peer-review` treats **GitHub as the source of truth**, so routing is
labels plus native review requests — not a queue we operate. The profile is
bootstrapped on this repository:

- **`ai-review`** (`#0e8a16`) — the trigger. Without it nothing routes.
- **Skill labels** (`#5319e7`) — `security`, `architecture`, `performance`,
  `testing`, `api`, `react-native`, `did`, `oid4vc`, `cryptography`,
  `second-opinion`. Each loads the matching review skill.

> `documentation` is part of the package's default profile but already existed
> here for issue triage, so it was **left untouched** rather than recoloured.
> A future `agent-review labels bootstrap` will try to reconcile it; that is
> cosmetic, and worth declining if it would confuse issue triage.

Typical request:

```bash
agent-review request --repo MediaNoxLabs/oxid --pr 42 --reviewers yshyn-iohk --skills security,architecture
```

`claim` pins the reviewed commit SHA in a claim-marker comment, so a review
cannot silently be attributed to a later push. `complete` posts a native PR
review.

## Running a loop

```bash
node scripts/dev-loops.mjs doctor    # environment readiness
node scripts/dev-loops.mjs gates     # resolve and print every configured angle
```

The tracked wrapper resolves only the exact project pin from the active Git
root or its linked-worktree common checkout; it never searches home/global
installs. Project Pi input is also preflighted against the effective packaged
agent tool allowlists before model dispatch. See
`docs/dev-loop-stability.md` for failure remediation, the REST GitHub probes,
forced integration-base wrappers, exact-head Claude command, and the explicit
upstream-only gap table.

`gates` is the authoritative dev-loop config validator — it exercises the real loader,
so a `.devloops` that `gates` parses is a `.devloops` that will run. Prefer it
over a YAML lint.

**`doctor` reports 3/4 and that is expected.** The warning is *"Subagent command
available"*, because `doctor` looks for a standalone `subagent` executable while
`pi-subagents@0.42.1` exposes the capability as a Pi extension. **Do not add a
dummy binary to make the check pass** — it would make a real absence
undetectable later. The check that matters is `gates` parsing.

## What will refuse to work, by design

- **No unguarded or cross-base automated merge.** `.devloops` permits the loop
  to reach merge, but `scripts/github/merge-integration-pr.mjs` accepts only an
  issue-backed `integration` PR and requires an explicit active owner
  authorization for `--execute`. It fails closed unless the base and head stay
  unchanged, the merge tree is conflict-free, all required checks (including
  GPG/DCO) pass, both gate verdicts match, and conversations are resolved.
  `main` and `develop` remain human-only.
- **Advisory checks stay advisory.** A red non-required check may make GitHub
  report the PR as `UNSTABLE`, but it does not expand the merge gate. The
  integration wrapper accepts that state only after `gh pr checks --required`
  returns a non-empty, fully passing exact-head set; stale, conflicting,
  blocked, or behind states remain ineligible.
- **Fan-out must show its work.** `gates.requireFanoutEvidence: true` and
  `requireFanoutProvenance: true` — a gate must record not just that five
  angles reported, but which reviewer produced which finding. Provenance is what
  makes a collapsed panel detectable, where one reviewer's output is replayed
  under several angle names.
- **Foreign angles are rejected.** `gates.rejectForeignAngles: true`, so an
  angle name not in the configured set cannot smuggle itself into evidence.
- **Draft first, without a CI stall.** `workflow.requireDraftFirst: true`, but
  the draft gate uses `requireCi: false`; hosted CI is required once at
  pre-approval. When aggregate CI is red on a draft, use gate coordination as
  the authority for progression: if it permits `run_draft_gate`, continue the
  draft loop and keep the PR draft. Commit authenticity remains required before
  pre-approval or merge; metadata and classification findings are advisory.
  Routine work requires the deterministic final-head metrics and closeout
  comment, not a separate model-driven retrospective.
- **No Copilot gate.** `refinement.maxCopilotRounds: 0` keeps unavailable
  Copilot review disabled. A manually invoked Claude CLI review is reserved for
  high-risk `full` changes, an owner request, or a disputed finding. It is not a hosted GitHub check and does not authenticate reviewer identity. Run
  `scripts/review/claude-current-head.mjs` once on the final clean head as
  documented in `docs/dev-loop-stability.md`.

## Model policy

**`models:` is deliberately absent.** Per-role model assignment
(`models.conductor`, `models.roles`) is the mechanism behind the factory's
provider-agnostic goal, but `dev-loops@0.9.0` documents no accepted identifier
schema for that field.

The project-level Pi parent and subagent defaults are instead pinned to
`openai-codex/gpt-5.6-terra:medium`. Every gate therefore has a balanced default
without inventing an unvalidated dev-loops field. Explicit session and agent
overrides remain available. Add per-role dev-loops values only after a package
canary proves the schema and identifiers before dispatch.

`personas.*.defaultModel` remains `null` for the same reason.

## Why there is no `worktree:` section

`copyOnInit` / `linkOnInit` provision **gitignored** files into a fresh
worktree. Oxid needs neither: the Compact and ZK artifact closures arrive as
devshell environment exports rather than files, `.envrc` is tracked, and raw
`target/` and `.direnv/` trees must not be shared. The repository wrapper keeps
the pinned generic worktree topology and branch logic but injects a zero-action
consumer provisioning callback. It therefore neither reads provisioning config
from a dirty primary checkout nor tries the generic package's own
`packages/core` workspace self-link. The devshell instead points every worktree
at one bounded 10 GiB `sccache`; disposable worktree targets stay isolated. The
Pi runtime resolver and devshell both reuse the common checkout's single
`.pi/npm` installation through the managed package-store link; other mutable
state is not linked into worktrees.

Recorded here because an empty section and a considered absence look identical
in a diff.

## Operating notes

- **Keep one remote candidate active per parent session.** `.devloops` sets
  `queue.maxParallel: 1` for one conductor; it is not a repository-wide mutex.
  Another parent may own another issue worktree locally or on a different host.
  Batch accepted findings locally and push a coherent candidate instead of
  invalidating CI and exact-head evidence after every small edit.
- **Recover after the one-hour conductor bound.** A Pi timeout does not delete
  the issue branch, managed worktree, draft PR, or private metrics. Re-run the
  startup resolver for the same issue, reuse its canonical worktree, verify the
  exact head and working-tree status, and continue from the last durable commit.
  Record the interrupted duration in closeout metrics; changing the ceiling is
  a tracked factory-policy change, not a per-session escape hatch.
- **Audit before creating another worktree.** `node scripts/worktree-lifecycle.mjs
  audit` lists target size, cleanliness, merge state/proof, and age. Direct
  ancestry is preferred; squash-merged heads require one exact merged GitHub PR
  with an `integration` base and a merge commit present on a remotely observed,
  current `origin/integration`. Unavailable, stale, malformed, or ambiguous
  evidence fails closed. The audit remains mutation-free, but non-ancestor heads
  require network access plus an installed, logged-in `gh`; offline runs retain
  local ancestry and mark hosted proof `unavailable`. Its human table appends
  the proof column, while automation should use the additive JSON shape. `remove` and
  `clean-target` require an exact path, expected head SHA, and `--execute`;
  removal additionally requires a clean head integrated into `origin/integration`
  and seven days of retention. Before creating a new canonical worktree, the
  tracked wrapper runs only host-capacity admission for that Git common
  checkout. A measured red worktree count or target-storage result blocks new
  creation. If the full lifecycle helper is unavailable, a conservative
  `git worktree` plus `du` fallback still admits a clean first checkout and
  blocks when capacity cannot be established. Pi configuration and private
  metrics remain visible in `--audit-pi` but do not prevent creating the first
  isolated worker. Reuse of an existing canonical worktree is always available
  so in-flight work can be recovered.
- **Leave one private metrics record per issue/PR/head.** Generate a closed v1
  template, replace every required `null`/empty target with measured values,
  and atomically store it. An untouched template is invalid. Audit is
  read-only and returns aggregate median/p90, per-check queue/execution timing,
  SLO/retention findings, duplicate identities, overflow markers, and malformed
  or missing-field counts without a model call:
  ```bash
  node scripts/factory/metrics.mjs template \
    --issue <n> --pr <n> --head "$(git rev-parse HEAD)"
  node scripts/factory/metrics.mjs write --record /private/path/metrics.json
  node scripts/factory/metrics.mjs audit --json
  ```
  The default store is `.git/oxid-factory/metrics-v1` in the common checkout,
  shared by linked worktrees but not committed. Token buckets must be exact and
  non-overlapping; otherwise use `tokens: null`. Raw records are owner-private
  for 90 days. The Quality Steward runs the audit weekly, after a harness
  incident, and before monthly tuning. Never paste prompts, transcripts,
  tokens/credentials, user identifiers, commands/output, or billing details
  into a record. Capture the final-head record before merge-ready and leave one
  bounded PR closeout comment with capture status, SLO/incident status, and any
  follow-up issue. That is the routine retrospective; deeper retrospectives
  remain conditional on an incident, SLO miss, high-risk change, or owner request.
- **Restart after harness changes.** A running Pi process retains its loaded
  extensions and instructions. Stop it, preserve its branch/head, then restart
  after changing `.pi/`, `.devloops`, or their installed pins.
- **Space out merges.** CI uses `cancel-in-progress`, so several merges in quick
  succession cancel intermediate `integration` runs and leave only the tip verified.
  Either pace them or state explicitly that verification is tip-only.
- **Verify then merge, in separate commands.** Chaining a check and a merge with
  `||` or `&&` has already merged a red PR once here. Assert zero non-passing
  checks as its own command:
  ```bash
  gh pr checks <n> -R MediaNoxLabs/oxid | awk -F'\t' '$2!="pass"'
  ```
- **A red gate is not always a reason to act.** A withdrawn dependency reports as
  a failure while the safe action is to change nothing — see issue #113. Read
  what the gate is actually asserting before remediating it.

## Phase 2 candidates, not built

- `models:` per-role assignment after the package documents and validates it.
- CI-built review references so the private package is not a per-machine
  install.
- Wiring `.pi/settings.json`'s `skills` key once a repository skill tree exists;
  it is absent today, so there is nothing to point at.
- Canarying `pi-subagents@0.58.0` under issue #195 and
  `agent-review-pi@0.6.0` under issue #196.
