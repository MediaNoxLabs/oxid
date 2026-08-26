<!-- SPDX-License-Identifier: Apache-2.0 -->

# Dev-loop stability contract

Issue [#150](https://github.com/MediaNoxLabs/oxid/issues/150) is broader than
this repository-owned first slice. This document distinguishes what the slice
actually enforces from gaps in the repository's pinned runtime. Several linked
defects are already fixed upstream; those fixes are not repository authority
until a separately tested pin upgrade lands. Local wrappers do not repair
resume or provider state machines.

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
# integration. Every wrapper route rejects an explicit non-integration base.
node scripts/dev-loops.mjs pr create \
  --repo MediaNoxLabs/oxid --head <branch> \
  --assignee @me --title "<type>: <subject>" --body-file <body-file>
```

The public wrapper accepts only the global option forms supported by the exact
`dev-loops@0.9.0` pin (`--repo`, `--cwd`, `--config`, `--jq`, `--silent`/`-s`,
and `--json`) before the route. Unknown leading options fail closed. A pin
upgrade must update the shared parser and contract tests before new option
shapes are accepted. These wrappers do not rewrite raw `gh` or direct package
scripts; repository rules and contributor policy remain authoritative there.

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

Every preflight failure produces a prominent input warning and leaves the
interactive turn available for diagnosis or repair. Defense-in-depth hooks
still abort before every agent or provider request, including unavailable
declared tools, missing packages, invalid settings, and pinned third-party
manifest shapes the bounded parser cannot model. Fix the tracked manifest or
exact installation instead of adding aliases.
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
fail closed. An explicit evidence directory must meet the same rules, stay
outside the checkout, and have no symlink or group/world-writable non-sticky
ancestor (the root directory and sticky temporary directories remain valid).

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
requires Claude CLI `>= 2.1.228`, probes the actual CLI flags/auth/version, and
retains the observed help artifact. The evidence binds empty-tool semantics to
the supported-version contract for the documented `--tools ""` form rather
than brittle help prose. It invokes Claude outside the checkout in safe mode
with that empty tool
set and no session persistence, then records CLI account readiness, version,
observed session id, timestamps, raw-output digest, exit status, and a
structured verdict. It checks clean/head/base/exact-diff facts again afterward.
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
Repository verification never invokes a model, spends API budget, or requires a
Claude login by default. The real capability smoke is available only through
explicit `OXID_CLAUDE_LIVE_SMOKE=1`; absent, unauthenticated, or unsupported
CLIs skip that optional smoke. The manually invoked current-head review itself
continues to fail closed on CLI or account incompatibility.

## Exact issue traceability

| Issue #150 acceptance or definition-of-done item | First-slice status | Authority / remaining work |
| --- | --- | --- |
| Effective repository agent tool allowlists match installed Pi tools before model execution | Landed in this slice | `.pi/agents/`, `scripts/lib/dev-loop-runtime.mjs`, `scripts/lib/dev-loop-preflight-core.mjs`, and the thin `.pi/extensions/dev-loop-preflight.ts` registrar; `.pi/settings.json` owns only the supported git-root selection |
| Project-local package discovery works at root and linked worktrees | Landed in this slice | The bounded tracked resolver and wrappers above |
| Timeout, deadline, `usageBudget`, turn, tool, and control budgets survive resume exactly | Pin-upgrade / upstream-owned | Closed/completed [pi-subagents #985](https://github.com/nicobailon/pi-subagents/issues/985) documents adjacent persisted turn-budget recovery and was fixed by merged [PR #987](https://github.com/nicobailon/pi-subagents/pull/987). The pinned [v0.42.1 async-resume source](https://github.com/nicobailon/pi-subagents/blob/v0.42.1/src/runs/background/async-resume.ts) remains repository authority pending a separately tested upgrade. |
| Provider payload compaction/checkpointing and streamed-mutation retry idempotency | Deferred / **upstream-only** | No exact upstream issue was established during this bounded slice. File a minimal upstream reproduction before claiming a fix; no repository wrapper can safely reconstruct provider stream state. |
| `integration` is the worktree, PR, diff, and evidence base | Landed in this slice | `scripts/loop/ensure-worktree.mjs`, `scripts/dev-loops.mjs`, and the Claude runner |
| Unavailable Copilot review routes to mandatory independent current-head Claude review | Landed in this slice | `.devloops` and the Claude runner; hosted Copilot stays disabled |
| Valid nested reviewer output cannot be overturned by a late unavailable-tool diagnostic | Deferred / upstream-owned | Closed/completed [pi-subagents #1434](https://github.com/nicobailon/pi-subagents/issues/1434) documents the adjacent final-return serialization failure and was fixed by merged [PR #1448](https://github.com/nicobailon/pi-subagents/pull/1448). The late-diagnostic case still needs its own minimal reproduction and a separately tested repository pin upgrade. |
| Supported GitHub CLI behavior is deterministic | Landed in this slice | Nix pin plus REST behavior probe and timeline resolver |
| dev-loops and pi-subagents share authenticated acceptance provenance | Upstream / pin-upgrade only | This slice records explicit local attestational facts and does not claim reviewer authentication. Closed/completed #1434 and #1460 document adjacent defects fixed upstream by merged PRs #1448 and #1461; neither establishes shared authenticated provenance in pinned v0.42.1. |
| Reproduction coverage | Partial, first slice | Repository tests cover package roots, allowlists, integration normalization, REST normalization, Claude invocation/result contracts, policy, and docs. Resume, WebSocket interruption/idempotency, and upstream finalization reproduction stay upstream-only. |
| Bounded issue-backed canary through PR/CI/merge checkpoint | Deferred operational validation | Run only after the repository slice is committed and every current-head gate is available; merge and board mutations remain orchestrator-owned. |

The exact pinned-runtime resume gap is visible in the v0.42.1 recovery
descriptor allowlist: it preserves the absolute deadline, initial turn/tool
budgets, and control configuration, but carries no `usageBudget`. Closed/completed
[issue #985](https://github.com/nicobailon/pi-subagents/issues/985) documented
persisted turn-budget recovery and was fixed by merged PR #987;
[issue #1460](https://github.com/nicobailon/pi-subagents/issues/1460) documented
the resumed structured-output contract and was fixed by merged PR #1461; and
[issue #1434](https://github.com/nicobailon/pi-subagents/issues/1434) documented
the workflow final-return serialization failure and was fixed by merged PR
#1448. These are distinct invariants. Their upstream completion does not change
the repository's pinned v0.42.1 behavior; only a separately tested pin upgrade
can do that.

Do not patch, modify, or vendor installed `.pi/npm` packages to close an
upstream/pin-upgrade row. Do not convert a successful side effect into a passed
gate. Where an exact defect is not already documented, open or update an
upstream issue with a minimal reproduction; otherwise preserve the closed issue
as historical evidence. Keep all local budgets bounded and stop at the pinned
runtime's existing failure boundary. PR #153 uses
`Refs #150`, not a closing keyword: issue #150 remains the follow-up authority
for every upstream-owned, pin-upgrade, and deferred row above until each is
either delivered or moved to an explicitly linked issue with its own acceptance
contract.
