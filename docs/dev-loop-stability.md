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

# Public dev-loops PR creation (`create-draft` is the deprecated alias) adds
# integration and rejects another base. Other dev-loops subcommands pass through.
node scripts/dev-loops.mjs pr create \
  --repo MediaNoxLabs/oxid --head <branch> \
  --assignee @me --title "<type>: <subject>" --body-file <body-file>
```

These wrappers govern only the public routes shown above. They do not rewrite
raw `gh`, direct package-script calls, or arbitrary internal dev-loops commands;
repository rules and contributor policy remain authoritative for those paths.

The repository selects `subagents.projectRootResolution: "git-root"` for the
exact `pi-subagents@0.42.1` pin and uses tracked
`.pi/agents/*.agent.md` project shadows because a custom agent's frontmatter
owns its tool list. A pinned-runtime smoke test confirms project precedence
when the local Pi installation is present; public CI tests the repository
contract without claiming to validate unavailable user-level installations.
`subagents.agentOverrides` is deliberately not presented as a tool-list repair.

The project extension keeps its runtime-independent logic in
`scripts/lib/dev-loop-preflight-core.mjs`; `.pi/extensions/dev-loop-preflight.ts`
is the only auto-loaded Pi registrar. Registration is idempotent. The preflight
obtains Pi's registered tool names and uses a bounded, dependency-free parser
for `name` and `tools` in every installed package pinned by `.pi/settings.json`
plus repository-local shadows. Only `*.agent.md` manifests are scanned. Results
are cached per session using the checkout, settings, package, manifest mtime and
tool-set facts; a changed manifest invalidates the cache.

An unavailable declared tool causes the input event to be handled without an
agent/model turn, and defense-in-depth hooks abort before an agent or provider
request. A missing or unprovisioned package instead produces a prominent input
warning and leaves the interactive turn available for diagnosis/provisioning;
agent and provider launch remains blocked until the environment is valid. Fix
the tracked manifest or exact installation instead of adding aliases.
Separately installed user agents remain outside this repository preflight and
must be governed by their owning installation.

The Nix default devshell includes `gh`. Before issue/PR link automation, probe
the exact read-only REST behavior and then use the timeline resolver:

```bash
node scripts/github/preflight-gh.mjs --repo MediaNoxLabs/oxid --issue <number>
node scripts/github/resolve-issue-pr-links.mjs \
  --repo MediaNoxLabs/oxid --issue <number>
```

Both commands require `gh >= 2.67.0` and use the issue and issue-timeline REST
endpoints, including pagination. They fail closed on CLI version,
authentication, capability, or response-shape errors and perform no mutation.

## Independent current-head review

Both draft and pre-approval gates make `external-review` mandatory, while
`maxCopilotRounds` remains zero. After committing all intended changes and
fetching `origin/integration`, run the reviewer with a clean worktree. By
default it writes beneath `${XDG_STATE_HOME:-$HOME/.local/state}/oxid/claude-reviews`.
The final directory must be a real, invoking-user-owned `0700` directory and
each artifact is an owned regular `0600` file; symlinks and permissive modes
fail closed. An explicit evidence directory must meet the same rules and stay
outside the checkout.

```bash
git fetch origin integration
node scripts/review/claude-current-head.mjs \
  --issue <number> \
  --expected-head "$(git rev-parse HEAD)" \
  --issue-contract-file <tracker-export.json>

node scripts/review/claude-current-head.mjs \
  --verify-evidence <evidence.json>
```

The runner independently derives HEAD and the `origin/integration` merge base,
rejects any binary path before review, and creates a `git diff --binary` UTF-8
text artifact whose exact bytes are both hashed and sent to the reviewer. It
probes the actual Claude CLI help/auth/version contracts; the private evidence
includes the observed help artifact proving that `--tools ""` disables all
tools. It invokes Claude outside the checkout in safe mode with that empty tool
set and no session persistence, then records CLI account readiness, version,
observed session id, timestamps, raw-output digest, exit status, and a structured
verdict. It checks clean/head/base/exact-diff facts again afterward.
Timeout, nonzero exit, malformed output, mutation, or a changed ref is a hard
failure. A findings verdict is also a failed gate, but its structured findings
attestation is written first so the next fix pass has durable evidence.
Verification accepts only clean evidence and re-derives all revision facts, so
a push or integration advance makes it stale.

This is **local attestational evidence**, not cryptographic reviewer-identity,
GitHub-hosted, or dev-loops-native provenance. Caller-supplied tracker data and
artifact digests bind bytes within the local record; they do not authenticate
who operated the CLI. Post the current-head attestation to the pull request; it
complements rather than bypasses CI, security, DCO/signature, and merge controls.

## Exact issue traceability

| Issue #150 acceptance or definition-of-done item | First-slice status | Authority / remaining work |
| --- | --- | --- |
| Effective repository agent tool allowlists match installed Pi tools before model execution | Landed in this slice | `.pi/agents/`, `scripts/lib/dev-loop-runtime.mjs`, `scripts/lib/dev-loop-preflight-core.mjs`, and the thin `.pi/extensions/dev-loop-preflight.ts` registrar; `.pi/settings.json` owns only the supported git-root selection |
| Project-local package discovery works at root and linked worktrees | Landed in this slice | The bounded tracked resolver and wrappers above |
| Timeout, deadline, `usageBudget`, turn, tool, and control budgets survive resume exactly | **Upstream-only** | [pi-subagents #985](https://github.com/nicobailon/pi-subagents/issues/985) and the pinned [v0.42.1 async-resume source](https://github.com/nicobailon/pi-subagents/blob/v0.42.1/src/runs/background/async-resume.ts) |
| Provider payload compaction/checkpointing and streamed-mutation retry idempotency | Deferred / **upstream-only** | No exact upstream issue was established during this bounded slice. File a minimal upstream reproduction before claiming a fix; no repository wrapper can safely reconstruct provider stream state. |
| `integration` is the worktree, PR, diff, and evidence base | Landed in this slice | `scripts/loop/ensure-worktree.mjs`, `scripts/dev-loops.mjs`, and the Claude runner |
| Unavailable Copilot review routes to mandatory independent current-head Claude review | Landed in this slice | `.devloops` and the Claude runner; hosted Copilot stays disabled |
| Valid nested reviewer output cannot be overturned by a late unavailable-tool diagnostic | Deferred / **upstream-only** | Result/finalization ownership remains in pi-subagents. [pi-subagents #1434](https://github.com/nicobailon/pi-subagents/issues/1434) is the exact adjacent final-return serialization failure, not proof that the late-diagnostic case is fixed; that case still needs its own minimal upstream reproduction. |
| Supported GitHub CLI behavior is deterministic | Landed in this slice | Nix pin plus REST behavior probe and timeline resolver |
| dev-loops and pi-subagents share authenticated acceptance provenance | **Upstream-only** | This slice records explicit local attestational facts and does not claim reviewer authentication. A shared cross-package provenance state machine requires upstream work in #1434 and #1460. |
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
local budgets bounded, and stop at the existing failure boundary. PR #153 uses
`Refs #150`, not a closing keyword: issue #150 remains the follow-up authority
for every upstream-only and deferred row above until each is either delivered
or moved to an explicitly linked issue with its own acceptance contract.
