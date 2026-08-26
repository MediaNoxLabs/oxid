<!-- SPDX-License-Identifier: Apache-2.0 -->

# Dev-loop stability contract

Issue [#150](https://github.com/MediaNoxLabs/oxid/issues/150) is broader than
this repository-owned first slice. This document distinguishes what the slice
actually enforces from defects that remain in upstream runtimes. It does not
claim that local wrappers repair upstream resume or provider state machines.

## Repository-owned entrypoints

Use only the tracked wrappers. They resolve the exact `dev-loops` npm pin in
`.pi/settings.json`; first from the active Git root, then from the linked
worktree's common checkout. Each candidate is realpath-checked for containment
and must have the exact package identity and version. There is no home, global,
personal-path, arbitrary-ancestor, or filesystem-search fallback.

```bash
# Managed issue worktree; origin/integration is added and any other base fails.
node scripts/loop/ensure-worktree.mjs \
  --repo-root "$PWD" --issue <number>

# Dev-loops CLI; PR creation adds integration and rejects another base.
node scripts/dev-loops.mjs pr create \
  --repo MediaNoxLabs/oxid --head <branch> \
  --assignee @me --title "<type>: <subject>" --body-file <body-file>
```

The project extension `.pi/extensions/dev-loop-preflight.ts` obtains Pi's
registered tool names after extension registration, applies the tracked
`subagents.agentOverrides`, and checks every effective packaged dev-loops agent
allowlist. An unavailable tool causes the input event to be handled without an
agent/model turn; defense-in-depth hooks abort programmatic turns before a
provider request. Correct the package installation or tracked override instead
of adding compatibility aliases.

The Nix default devshell includes `gh`. Before issue/PR link automation, probe
the exact read-only REST behavior and then use the timeline resolver:

```bash
node scripts/github/preflight-gh.mjs --repo MediaNoxLabs/oxid --issue <number>
node scripts/github/resolve-issue-pr-links.mjs \
  --repo MediaNoxLabs/oxid --issue <number>
```

Both commands use the issue and issue-timeline REST endpoints, including
pagination. They fail closed on CLI, authentication, capability, or response
shape errors and perform no mutation.

## Independent current-head review

Both draft and pre-approval gates make `external-review` mandatory, while
`maxCopilotRounds` remains zero. After committing all intended changes and
fetching `origin/integration`, run the reviewer with a clean worktree. Put its
artifacts outside the checkout so evidence generation cannot change the
reviewed state.

```bash
git fetch origin integration
node scripts/review/claude-current-head.mjs \
  --issue <number> \
  --expected-head "$(git rev-parse HEAD)" \
  --issue-contract-file <tracker-export.json> \
  --evidence-dir "${TMPDIR:-/tmp}/oxid-claude-review-<number>"

node scripts/review/claude-current-head.mjs \
  --verify-evidence <evidence.json>
```

The runner independently derives HEAD and the `origin/integration` merge base,
creates an immutable diff artifact and digest, invokes Claude outside the
checkout in safe mode with an empty tool set and no session persistence, and
records redacted authenticated-CLI status, CLI version, session id, timestamps,
raw-output digest, exit status, and structured verdict. It checks clean/head/base/diff facts again afterward.
Timeout, nonzero exit, malformed output, findings, mutation, or a changed ref is
a hard failure. Verification re-derives these facts, so a push or integration
advance makes old evidence stale. Post the verified evidence to the pull
request; it complements rather than bypasses CI, security, DCO/signature, and
merge controls.

## Exact issue traceability

| Issue #150 acceptance or definition-of-done item | First-slice status | Authority / remaining work |
| --- | --- | --- |
| Effective repository agent tool allowlists match installed Pi tools before model execution | Landed in this slice | `.pi/settings.json`, `scripts/lib/dev-loop-runtime.mjs`, and `.pi/extensions/dev-loop-preflight.ts` |
| Project-local package discovery works at root and linked worktrees | Landed in this slice | The bounded tracked resolver and wrappers above |
| Timeout, deadline, `usageBudget`, turn, tool, and control budgets survive resume exactly | **Upstream-only** | [pi-subagents #985](https://github.com/nicobailon/pi-subagents/issues/985) and the pinned [v0.42.1 async-resume source](https://github.com/nicobailon/pi-subagents/blob/v0.42.1/src/runs/background/async-resume.ts) |
| Provider payload compaction/checkpointing and streamed-mutation retry idempotency | Deferred / **upstream-only** | No exact upstream issue was established during this bounded slice. File a minimal upstream reproduction before claiming a fix; no repository wrapper can safely reconstruct provider stream state. |
| `integration` is the worktree, PR, diff, and evidence base | Landed in this slice | `scripts/loop/ensure-worktree.mjs`, `scripts/dev-loops.mjs`, and the Claude runner |
| Unavailable Copilot review routes to mandatory independent current-head Claude review | Landed in this slice | `.devloops` and the Claude runner; hosted Copilot stays disabled |
| Valid nested reviewer output cannot be overturned by a late unavailable-tool diagnostic | Deferred / **upstream-only** | Result/finalization ownership remains in pi-subagents. [pi-subagents #1434](https://github.com/nicobailon/pi-subagents/issues/1434) is the exact adjacent final-return serialization failure, not proof that the late-diagnostic case is fixed; that case still needs its own minimal upstream reproduction. |
| Supported GitHub CLI behavior is deterministic | Landed in this slice | Nix pin plus REST behavior probe and timeline resolver |
| dev-loops and pi-subagents share authenticated acceptance provenance | Partially landed / **upstream-only** | This slice records executable/version/session/artifact provenance for Claude. A shared cross-package provenance state machine requires upstream work in #1434 and #1460. |
| Reproduction coverage | Partial, first slice | Repository tests cover package roots, allowlists, integration normalization, REST normalization, Claude invocation/result contracts, policy, and docs. Resume, WebSocket interruption/idempotency, and upstream finalization reproduction stay upstream-only. |
| Bounded issue-backed canary through PR/CI/merge checkpoint | Deferred operational validation | Run only after the repository slice is committed and every current-head gate is available; merge and board mutations remain orchestrator-owned. |

The exact pinned-runtime resume gap is visible in the v0.42.1 recovery
descriptor allowlist: it preserves the absolute deadline, initial turn/tool
budgets, and control configuration, but carries no `usageBudget`. Upstream
[issue #985](https://github.com/nicobailon/pi-subagents/issues/985) covers
persisted turn-budget resume behavior; [issue #1460](https://github.com/nicobailon/pi-subagents/issues/1460)
covers the separate resumed structured-output contract; [issue #1434](https://github.com/nicobailon/pi-subagents/issues/1434)
covers the exact workflow final-return serialization failure. These are distinct invariants and
must not be collapsed into one caller-authored checkpoint.

Do not patch, modify, or vendor installed `.pi/npm` packages to close an
upstream-only row. Do not convert a successful side effect into a passed gate.
Open or update the linked upstream issue with a minimal reproduction, keep all
local budgets bounded, and stop at the existing failure boundary.
