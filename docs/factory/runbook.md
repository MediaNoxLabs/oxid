<!-- SPDX-License-Identifier: Apache-2.0 -->

# Factory runbook — phase 1

How to actually run the factory on this repository. The charter says *why*
([charter.md](charter.md)), the FSM says *what state work is in*
([fsm.md](fsm.md)); this says *what to type* and *what will refuse to work*.

Phase 1 is deliberately narrow: **bounded review, risk-tiered validation, and a
human merge.** Routine changes use two draft lenses and two final lenses;
independent current-head review is reserved for high-risk work. Nothing routes
through a coordination server.

## What is installed, and from where

| Piece | Version | Source |
| --- | --- | --- |
| `pi-coding-agent` | nixpkgs pin | `nix/devshells/default.nix`, `devShells.default` |
| `dev-loops` | `0.9.0` | `.pi/settings.json` → project-local `.pi/npm` |
| `pi-subagents` | `0.42.1` | same |
| `@input-output-hk/agent-review-pi` | `0.5.0` | same, **GitHub Packages — needs a token** |
| `agent-review` loader skill | repository | `.pi/skills/agent-review/SKILL.md` |

The devshell's `shellHook` reads `.pi/settings.json`, compares each exact pin
against the common checkout's `.pi/npm/node_modules/<pkg>/package.json`, and
installs only what is missing or mismatched. Linked worktrees reuse that one
installation instead of creating a mutable package tree each. CI skips Pi
tooling entirely. The private review package is skipped with a printed notice
when no token is present, so the shell still works without one.

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
./bootstrap.sh --check
```

From a plain checkout, `./bootstrap.sh --pi` starts Pi inside the same pinned
shell. `./bootstrap.sh -- <command>` runs any other one-off repository command
there. The wrapper delegates package provisioning to `nix develop` and never
reads, prints, or persists credentials.

## Three concurrency mechanisms, which are easy to confuse

This is the part most likely to be misread, because all three look like
"running several agents at once" and they compose rather than compete.

| | What fans out | Configured in | Who decides |
| --- | --- | --- | --- |
| **Gate fan-out** | Review **angles** over one diff — draft `scope`/`correctness`, final `correctness`/`security` | `.devloops` → `refinement.fanOut: 2`, `mode: parallel`, `roles` | dev-loops, automatically at a gate |
| **Sub-agent delegation** | Child **pi sessions** with their own jobs | `pi-subagents`, prompt-driven — no config file | the agent, when asked |
| **Panel review** | Multiple **requested reviewers** on a PR | GitHub review requests + the `ai-review` label | a human, by requesting review |

**Gate fan-out** is the one that runs without being asked. `refinement.fanOut`
is how many angle reviews run concurrently; `roles` is the pool they are drawn
from. Both refinement and gate concurrency are capped at two. Low-signal
refinement stops after two quiet rounds instead of spending a third round to
rediscover the same result.

**Sub-agent delegation** ships six builtins — `scout` (codebase recon),
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

`gates` is the authoritative config validator — it exercises the real loader,
so a `.devloops` that `gates` parses is a `.devloops` that will run. Prefer it
over a YAML lint.

**`doctor` reports 3/4 and that is expected.** The warning is *"Subagent command
available"*, because `doctor` looks for a standalone `subagent` executable while
`pi-subagents@0.42.1` exposes the capability as a Pi extension. **Do not add a
dummy binary to make the check pass** — it would make a real absence
undetectable later. The check that matters is `gates` parsing.

## What will refuse to work, by design

- **No automated merge.** `autonomy.humanMergeOnly: true` and `stopAt: [merge]`
  are fixed boundaries. Automation may prepare a clean PR but cannot merge it.
- **Fan-out must show its work.** `gates.requireFanoutEvidence: true` and
  `requireFanoutProvenance: true` — a gate must record not just that five
  angles reported, but which reviewer produced which finding. Provenance is what
  makes a collapsed panel detectable, where one reviewer's output is replayed
  under several angle names.
- **Foreign angles are rejected.** `gates.rejectForeignAngles: true`, so an
  angle name not in the configured set cannot smuggle itself into evidence.
- **Draft first, without a CI stall.** `workflow.requireDraftFirst: true`, but
  the draft gate uses `requireCi: false`; hosted CI is required once at
  pre-approval. Routine work does not require a retrospective.
- **No Copilot gate.** `refinement.maxCopilotRounds: 0` keeps unavailable
  Copilot review disabled. A manually invoked Claude CLI review is reserved for
  high-risk `full` changes, an owner request, or a disputed finding. It is not a hosted GitHub check and does not authenticate reviewer identity. Run
  `scripts/review/claude-current-head.mjs` once on the final clean head as
  documented in `docs/dev-loop-stability.md`.

## One decision still needed from the owner

**`models:` is deliberately absent.** Per-role model assignment
(`models.conductor`, `models.roles`) is the mechanism behind the factory's
provider-agnostic goal — a cheap model for mechanical angles, a strong one for
`security` and `architecture` — and it is where the cost/quality tuning lives.

It is unset because `dev-loops@0.9.0` ships **no defaults for it and documents
no model-identifier format**, so any value written here would be a guess that
fails at dispatch rather than at load. Closing it needs one decision naming real
identifiers for the conductor and for the bounded roles. Until then every
angle runs on whatever the session's default model is, which works but wastes
the cheapest available saving.

`personas.*.defaultModel` is `null` for the same reason, and should be filled in
the same pass.

## Why there is no `worktree:` section

`copyOnInit` / `linkOnInit` provision **gitignored** files into a fresh
worktree. Oxid needs neither: the Compact and ZK artifact closures arrive as
devshell environment exports rather than files, `.envrc` is tracked, and raw
`target/` and `.direnv/` trees must not be shared. The devshell instead points
every worktree at one bounded 10 GiB `sccache`; disposable worktree targets stay
isolated. The Pi runtime resolver and devshell both reuse the common checkout's
single `.pi/npm` installation without symlinking it into worktrees.

Recorded here because an empty section and a considered absence look identical
in a diff.

## Operating notes

- **Keep one remote candidate active.** `.devloops` sets `queue.maxParallel: 1`.
  Batch accepted findings locally and push a coherent candidate instead of
  invalidating CI and exact-head evidence after every small edit.
- **Audit before creating another worktree.** `node scripts/worktree-lifecycle.mjs
  audit` lists target size, cleanliness, merge state, and age. `remove` and
  `clean-target` require an exact path, expected head SHA, and `--execute`;
  removal additionally requires a clean head merged to `origin/integration`
  and seven days of retention.
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

- `models:` per-role assignment (above) — the only blocking one.
- CI-built review references so the private package is not a per-machine
  install.
- Wiring `.pi/settings.json`'s `skills` key once a repository skill tree exists;
  it is absent today, so there is nothing to point at.
- Raising `pi-subagents` from `0.42.1` toward `0.52.1`; ten minor versions have
  shipped, and the pin is deliberate but ageing.
