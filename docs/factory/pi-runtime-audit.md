<!-- SPDX-License-Identifier: Apache-2.0 -->

# Pi runtime and package audit

Audit date: 2026-09-09. Tracking issues: [#194](https://github.com/MediaNoxLabs/oxid/issues/194) and [#195](https://github.com/MediaNoxLabs/oxid/issues/195).

## Outcome

The pinned devshell and package installation are healthy, but the effective
factory was not bounded. Routine work inherited `openai-codex/gpt-5.6-sol` at
`xhigh`; `pi-subagents` had no effective policy file and therefore allowed
async-by-default execution, 20 concurrent children per run, unlimited session
spawns, and no turn or usage budget. The active `/factory claim` skeleton also
performed three raw, non-atomic GitHub mutations while using the wrong branch
grammar.

The repository policy now defaults routine work to
`openai-codex/gpt-5.6-terra:medium`, permits one Pi retry, caps an individual
provider request at ten minutes, and makes compaction explicit. Tracked agents
have role-sized wall-clock and tool budgets. The user-level subagent policy
caps concurrency at two across independent Pi parents, permits exactly one
child launch in each parent session/run, limits each child to 60 tool calls
with a soft nudge at 40, retains recursion at two levels for one bounded
reviewer, and reports child usage against an 80k soft / 120k hard token
envelope. The package's token ceiling gates later launches but does not
interrupt an active model response, so the one-child rule, tool budget, and
wall-clock deadline are the enforceable stop controls. Async execution requires
an explicit request. Tune these starting bounds only from retained metrics.

The first supervised product runs established a cheaper default topology:
external supervisors invoke Pi directly as the sole issue worker. A nested
parent and child both loaded the repository contract and one 25-turn child
reached its ceiling before editing; the equivalent direct worker reached the
implementation. Terra completed routine repository changes more reliably than
Luna, while Sol was useful but materially more expensive. The tracked default
therefore stays Terra; Sol requires a concrete hard-reasoning need and Luna is
limited to bounded scouting or small documentation work.

Agent budget frontmatter intentionally uses a small machine-readable grammar:
`timeoutMs` and `maxSubagentDepth` are top-level integers, while `toolBudget` is
an inline JSON object. Do not convert these controls to YAML block mappings;
the startup audit rejects formats outside that tracked contract.

## Measured snapshot before remediation

| Surface | Evidence | Status |
| --- | --- | --- |
| Devshell Pi | Nix-pinned; `./bootstrap.sh --check` passed | healthy |
| Direct host Pi | outside Nix | unsupported path; use `./bootstrap.sh --pi` |
| Project packages | `dev-loops@1.0.2` (CLI/skills/agents; mutating extension filtered), `pi-subagents@0.66.0`, `agent-review-pi@0.6.0` plus exact peers | exact pins installed |
| npm production audit | 0 reported vulnerabilities | healthy at audit time |
| Common Pi package store | one shared store per Git common checkout | healthy |
| Registered worktrees | above the active green limit | red; exact counts remain private operational telemetry |
| Worktree-local Rust targets | above the 200 GiB ceiling | red; exact usage remains private operational telemetry |
| User Pi sessions | observable but private | retained transcripts were not read |
| Project subagent artifacts | above desired retention | move future artifacts to session retention |
| Private factory metrics | coverage not yet established | red; no tuning evidence exists yet |

The audit did not stop running Pi processes, read transcripts or credentials,
or delete dirty state. It identified only clean, merged, retained worktrees as
mechanically removable; deletion still required an exact path/head and explicit
`--execute`. The first bounded cleanup removed only audited, rebuildable state.
The owner-aware reconciliation of remaining dirty/unmerged state is tracked by
[#198](https://github.com/MediaNoxLabs/oxid/issues/198).

## Package posture

| Package | Pin | Available at audit | Decision |
| --- | --- | --- | --- |
| `pi-coding-agent` | `0.85.1` via locked Nix | `0.85.1` | required compatible runtime for `pi-subagents@0.66.0` native detached children |
| `dev-loops` | `1.0.2` | `1.0.2` | major update in [#303](https://github.com/MediaNoxLabs/oxid/issues/303) |
| `pi-subagents` | `0.66.0` | `0.66.0` | adopted directly in [#195](https://github.com/MediaNoxLabs/oxid/issues/195) |
| `agent-review-pi` | `0.6.0` | `0.6.0` | adopted with exact peers by [#301](https://github.com/MediaNoxLabs/oxid/issues/301) |
| `pi-taskflow` | `0.2.10` | `0.3.0-beta.1.2` | peer only; runtime resources disabled |
| `typebox` | `1.3.9` | `1.3.28` | minimum compatible exact peer; retain |

The `dev-loops@1.0.2` Pi extension is deliberately filtered while its exact
CLI, skills, and agent sources remain installed. Its `session_start` handler
overwrites an existing consumer `.pi/agents/` directory with generic packaged
agents. Oxid owns policy-bearing compatibility shadows at that path, so loading
the extension would erase runtime/tool budgets and delivery-profile handoff
rules immediately before model dispatch. A real offline Pi RPC startup must
leave every tracked agent hash unchanged.

`pi-coding-agent@0.85.1` is locked through the Nix input and is the supported
entrypoint for `pi-subagents@0.66.0`: its native child launcher receives the
package context that the former standalone Nix `0.84.0` executable lacked.
The devshell leaves `PI_CODING_AGENT_DIR` user-scoped so the existing Codex
authentication and bounded user policy remain available. It roots
`PI_CODING_AGENT_SESSION_DIR` and `PI_SUBAGENTS_TEMP_ROOT` at owner-private
`<git-common-dir>/oxid-factory/pi-runtime-v1/` directories (mode 0700), so Pi
sessions plus detached lifecycle/results survive exiting and re-entering
`nix develop` and are shared only by that checkout's linked worktrees, never by
the expiring Nix `TMPDIR`. The smoke rejects missing or misdirected runtime
state and an incompatible Pi version with actionable diagnostics before native
dispatch.

The `pi-subagents` releases between the pin and 0.66.0 contain fixes directly
related to recovered/detached runs, budget/timeout terminal classification,
smaller child context, exact model failures, and Codex priority propagation.
Issue #195 adopts that local, recoverable upgrade directly with one focused
smoke rather than a separate migration canary. `agent-review-pi@0.6.0` is small
enough to verify here: its exact peer
closure reports zero npm vulnerabilities, its 13 native tools register, and its
corrected bundled skill loads through Pi RPC. The former compatibility loader
is removed.

The local migration changed the shared package store from roughly 70 MiB in the
#158 preflight to 81,616 KiB with the complete 0.6.0 closure. A cold offline Pi
RPC command inventory completed in 0.88 seconds. Read-only `whoami`, skill-list,
and review-list calls succeeded; no review or label mutation was used as a
package test. Rollback is one command:
`git revert "$(git log --format=%H --grep='^fix(harness): align Pi runtime' -1)"`.
It restores the previous
locked Nix input and its exact Pi runtime; re-entering `./bootstrap.sh` then
reconstructs only that reverted closure. Owner-private session and async state
is intentionally retained for recovery and is not part of rollback.

### External repository mutation boundary (2026-09-12)

A supervised worker investigating an upstream watcher behavior created a useful
external issue without explicit approval. No credentials or payloads were
exposed, but the write exceeded the active Oxid issue authority. Pi and worker
guidance now limit issue-backed delivery writes to `MediaNoxLabs/oxid`; an
owner or supervisor must explicitly approve any external issue, PR, comment,
label, release, package publication, or other repository write. Workers may
instead prepare a local report for the supervisor. Contract tests cover the
worker guidance, while the supervisor retains the decision and publication
boundary.

### Supervised taskflow canary (2026-09-07)

Issue [#158](https://github.com/MediaNoxLabs/oxid/issues/158) was used as a
production-ready delivery canary. It did not reach a reviewable result. The
detached `pi-taskflow@0.2.10` runner exited in 70 ms before phase `main` because
its isolated package root could not resolve host peer dependencies. The runner
discarded stderr, so the parent retained only exit code 1. An inline retry then
wrapped the read-only `dev-loop` conductor around an async `pi-subagents` child.
Although the child was active, taskflow observed no nested output for 300
seconds, killed the conductor after 439 seconds, and left the child process
group alive. The supervisor terminated that exact owned process group.

The full canary lasted 1,243 seconds. Pi reported 91,383 input tokens, 3,470
output tokens, 492,160 cached-input tokens, and $0.6567644 model cost. The clone
grew from 16 MiB to 4.0 GiB, almost entirely a 3.9 GiB worktree-local Rust
target. It produced one uncommitted partial test edit, zero commits, zero pull
requests, and zero hosted-CI results. The partial implementation passed 20
focused Rust tests and formatting, then attempted the nonexistent
`npm run verify` fallback. It was not review-ready because its assertions no
longer proved that concrete private values stayed absent.

Issue [#301](https://github.com/MediaNoxLabs/oxid/issues/301) therefore applies
a local fail-closed mitigation: project settings suppress inherited taskflow
extensions and skills, `/dev-loop` requires direct bounded `pi-subagents`
dispatch, the smoke test proves taskflow is absent from effective commands, and
validation remains target-plan/Cargo/Just/Nix native. Re-enable taskflow only
after detached peer-resolution, nested-progress, cancellation, and orphan-cleanup
behavior is fixed and verified. General cumulative budget and terminal-
reconciliation improvements remain in #227.

## Required operator flow

Configure the bounded user-level package policy once, then start Pi only
through the pinned shell:

```bash
./bootstrap.sh --configure-pi
./bootstrap.sh --configure-git
./bootstrap.sh --check
./bootstrap.sh --audit-pi
./bootstrap.sh --pi
```

`./bootstrap.sh --check` also verifies that the parent/subagent default model
is present in the Nix-pinned Pi model catalog. This is a catalog canary without
making a billed provider request. The same smoke reads the exact
`pi-subagents` pin's `ExtensionConfig`, agent-frontmatter parser, and tool-budget
validator before the offline Pi RPC load. Those installed package sources are
the schema authority for every key written from `.pi/subagent-policy.json` and
for `timeoutMs` / `toolBudget` in tracked agents; the repository copy is not
treated as self-authenticating evidence.

`--configure-pi` changes only
`~/.pi/agent/extensions/subagent/config.json` (or the
`PI_CODING_AGENT_DIR` equivalent), preserves unrelated keys, writes mode 0600,
and preserves the first pre-policy non-empty file as mode-0600 `config.json.backup`
without accumulating repeated snapshots. It never reads or writes `auth.json`.
`--pi` refuses to start when the effective package policy drifts.
Restart Pi after any `.pi/`, `.devloops`, pin, or user-policy change because a
running process retains the configuration loaded at startup.

Delivery mode is not mutable global Pi state. Each public invocation names
`prototype` or `production-ready` according to
[the productive loop](productive-loop.md), and the latter is the safe default.
The tracked `.pi/delivery-profiles.json` contract keeps prototype work local,
provisional, single-reviewer, and non-mergeable. The read-only Pi audit checks
that contract and both agent entrypoint spellings. After this profile contract
changes, preserve the current branch/head and restart Pi from the canonical
checkout before relying on it.

The companion `--configure-git` command installs the repository-scoped local
contribution hooks and signing defaults. `--check` validates both surfaces; it
does not call an LLM or publish anything.

The project default is a preference, not provider lock-in. Use Pi's model
selector or `--provider`/`--model` for a deliberate session override. High-risk
security or architecture
work may select `openai-codex/gpt-5.6-sol:high` or `:xhigh`; another provider is
also valid when it can satisfy the same issue, validation, and evidence
contract. Do not weaken repository gates to accommodate a provider.

The bounds are scoped, not machine-global: `queue.maxParallel: 1` limits one
parent conductor, the subagent concurrency limit applies within one Pi run,
and the active-worktree threshold applies to one Git common checkout on one
host. Multiple local parents, independent engineer clones, and cloud workers
may operate concurrently when each owns a different issue worktree. The full
ownership and authentication contract is in
[worker-topology.md](worker-topology.md).

## Admission and retention

`node scripts/factory/audit-pi.mjs --json` is read-only. Configuration failures
block `./bootstrap.sh --pi`. Only measured host-capacity failures block admission
of another worktree in that common checkout: the tracked ensure-worktree wrapper
refuses creation when worktree count or target storage is red, but permits reuse
of an existing canonical worktree. Configuration and metrics findings remain
visible in the full audit without deadlocking first-worker creation; an
unavailable lifecycle helper falls back to conservative `git worktree` and
`du` evidence. If neither path can establish capacity, admission blocks. No
audit causes automatic process termination or deletion.

| Worktree-local target usage | State |
| --- | --- |
| at most 100 GiB | green |
| over 100 GiB through 200 GiB | amber; schedule cleanup |
| over 200 GiB | red; admit no new factory item |

Nix store and `sccache` are shared infrastructure and are reported separately;
they must not be copied into worktrees. Use the existing lifecycle command for
exact state and recoverable cleanup. Preserve dirty/untracked work and active
heads; clean only an exact audited path and head.

## Supervisor and telemetry

Every final PR head gets one owner-private v1 metric record before it becomes
merge-ready and one bounded PR closeout comment stating: record captured or
why counters were unavailable, SLO/incident status, and any follow-up issue.
This comment is the routine retrospective. A model-heavy retrospective remains
conditional on an incident, an SLO miss, high-risk work, or an owner request.

The existing JSON record store is sufficient locally: it is private, atomic,
schema-validated, and easy to aggregate. Do not deploy a database yet. Consider
SQLite only after at least 100 records or a demonstrated multi-host query need;
consider a remote service only when multiple operators require centralized
retention and its credentials/privacy/backup cost has an explicit owner.

Until a shared service is justified, each host retains raw records privately
and the bounded redacted PR closeout comment is the cross-host supervisor feed.

Useful local services remain Nix, `sccache`, `gh`, `jq`, Docker for selected
headless targets, and filesystem/disk telemetry. A database, message broker,
or always-on orchestration server is not required for the current factory.

The guarded claim transaction is deliberately disabled until
[#197](https://github.com/MediaNoxLabs/oxid/issues/197) implements the full
lease race, idempotency, recovery, and `{type}/issue-N` branch contract.
