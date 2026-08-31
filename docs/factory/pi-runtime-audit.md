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

Agent budget frontmatter intentionally uses a small machine-readable grammar:
`timeoutMs` and `maxSubagentDepth` are top-level integers, while `turnBudget` is
an inline JSON object. Do not convert these controls to YAML block mappings;
the startup audit rejects formats outside that tracked contract.

## Measured snapshot before remediation

| Surface | Evidence | Status |
| --- | --- | --- |
| Devshell Pi | Nix-pinned; `./bootstrap.sh --check` passed | healthy |
| Direct host Pi | outside Nix | unsupported path; use `./bootstrap.sh --pi` |
| Project packages | `dev-loops@0.9.0`, `pi-subagents@0.42.1`, `agent-review-pi@0.5.0` | exact pins installed |
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
./bootstrap.sh --configure-git
./bootstrap.sh --check
./bootstrap.sh --audit-pi
./bootstrap.sh --pi
```

`./bootstrap.sh --check` also verifies that the parent/subagent default model
is present in the Nix-pinned Pi model catalog. This is a catalog canary without
making a billed provider request. The same smoke reads the exact
`pi-subagents` pin's `ExtensionConfig`, agent-frontmatter parser, and turn-budget
validator before the offline Pi RPC load. Those installed package sources are
the schema authority for every key written from `.pi/subagent-policy.json` and
for `timeoutMs` / `turnBudget` in tracked agents; the repository copy is not
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
