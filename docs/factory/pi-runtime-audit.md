<!-- SPDX-License-Identifier: Apache-2.0 -->

# Pi runtime and package audit

Audit date: 2026-08-29. Tracking issue: [#194](https://github.com/MediaNoxLabs/oxid/issues/194).

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
have role-sized wall-clock and turn budgets. The user-level subagent policy
caps concurrency at two, session spawns at eight, recursion at two levels, and
reported child usage at a 120k soft / 200k hard token envelope. Async execution
requires an explicit request. These are starting bounds, not permanent
performance targets; tune them only from retained metrics.

## Measured snapshot before remediation

| Surface | Evidence | Status |
| --- | --- | --- |
| Devshell Pi | `0.84.0`; `./bootstrap.sh --check` passed | healthy |
| Direct host Pi | `0.82.1` outside Nix | unsupported path; use `./bootstrap.sh --pi` |
| Project packages | `dev-loops@0.9.0`, `pi-subagents@0.42.1`, `agent-review-pi@0.5.0` | exact pins installed |
| npm production audit | 127 dependencies, 0 reported vulnerabilities | healthy at audit time |
| Common Pi package store | about 68 MiB | healthy and shared by linked worktrees |
| Registered worktrees | 40 registered, 19 not proven merged, 6 dirty | red; active green limit is 2 |
| Worktree-local Rust targets | 231.9 GiB | red; see storage thresholds below |
| User Pi sessions | about 1.2 GiB | observable; retained transcripts were not read |
| Project subagent artifacts | 11 directories, about 259 MiB | move future artifacts to session retention |
| Private factory metrics | 0 valid records | red; no tuning evidence exists yet |

The audit did not stop running Pi processes, read transcripts or credentials,
or delete dirty state. The lifecycle audit identified four clean, merged,
seven-day-old worktrees as mechanically removable; deletion still required an
exact path/head and explicit `--execute`. The first bounded cleanup removed
those four zero-target worktrees and about 43 GiB of rebuildable target
data from three inactive merged worktrees. The owner-aware reconciliation of
remaining dirty/unmerged state is tracked by
[#198](https://github.com/MediaNoxLabs/oxid/issues/198).

## Package posture

| Package | Pin | Available at audit | Decision |
| --- | --- | --- | --- |
| `dev-loops` | `0.9.0` | `0.9.0` | retain |
| `pi-subagents` | `0.42.1` | `0.58.0` | canary in [#195](https://github.com/MediaNoxLabs/oxid/issues/195) |
| `agent-review-pi` | `0.5.0` | `0.6.0` | canary with new peers in [#196](https://github.com/MediaNoxLabs/oxid/issues/196) |

The `pi-subagents` releases between the pin and 0.58.0 contain fixes directly
related to recovered/detached runs, budget/timeout terminal classification,
smaller child context, exact model failures, and Codex priority propagation.
That makes an upgrade valuable and too risky to bundle blindly. Version 0.6.0
of `agent-review-pi` adds `pi-taskflow` and `typebox` peer requirements, so its
complete closure and the existing compatibility skill must be tested together.

## Required operator flow

Configure the bounded user-level package policy once, then start Pi only
through the pinned shell:

```bash
./bootstrap.sh --configure-pi
./bootstrap.sh --check
./bootstrap.sh --audit-pi
./bootstrap.sh --pi
```

`--configure-pi` changes only
`~/.pi/agent/extensions/subagent/config.json` (or the
`PI_CODING_AGENT_DIR` equivalent), preserves unrelated keys, writes mode 0600,
and backs up a pre-existing non-empty file. It never reads or writes
`auth.json`. `--pi` refuses to start when the effective package policy drifts.
Restart Pi after any `.pi/`, `.devloops`, pin, or user-policy change because a
running process retains the configuration loaded at startup.

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
block `./bootstrap.sh --pi`. Operational failures block admission of another
worktree in that common checkout: the tracked ensure-worktree wrapper refuses
to create a new canonical worktree while admission is red, but permits reuse of
an existing canonical worktree. No audit causes automatic process termination
or deletion.

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
