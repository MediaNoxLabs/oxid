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
uses Pi 0.84.0's public `getAllTools()`, `getActiveTools()`, and
`before_agent_start.systemPromptOptions.selectedTools` contracts. It validates
the selected agent against that agent's active tools, root execution against
all configured tools, and future child manifests against Pi's documented child
built-ins plus registered extension tools. A selected read/bash/subagent
`dev-loop` therefore does not make `edit` and `write` unavailable to future
implementation children. Unsupported aliases such as `search`, `execute`, and
`web_search` still fail closed.

The bounded, dependency-free parser reads `name` and `tools` in each installed
pinned package's top-level `agents/*.agent.md` manifests plus repository-local
`.pi/agents/*.agent.md` shadows. Packages that discover agents from any other
location are outside this bounded claim. Results are cached per session using
the checkout, settings, package, manifest mtime, active-agent identity, and
root/active/future tool-set facts; a changed manifest invalidates the cache.

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

Both commands require `gh >= 2.97.0`. The same floor applies to the sanctioned
CI watcher because the pinned dev-loop gate surfaces use the current GraphQL
field set. An older or malformed CLI fails before GitHub state is interpreted
and directs the operator to `nix develop`; the default devshell supplies the
pin. The issue-link commands use the issue and issue-timeline REST endpoints,
including pagination. They fail closed on CLI version, authentication,
capability, or response-shape errors and perform no mutation.

The tracked local mutation preflight is
`node scripts/loop/pre-flight-gate.mjs`. It resolves only the exact package pin
through the repository resolver and maps a depth-bounded Pi child runtime to
the package's advisory subagent capability flag. Do not invoke a missing
repository-relative package path or search for a global copy.

`node scripts/dev-loops.mjs --help` and `-h` are exact aliases for `help`.
Other unknown leading options remain rejected. `loop watch-ci` uses the tracked
attempt-aware watcher: for a check name and provider, only the newest check-run
attempt on the current head is authoritative. An obsolete failure cannot
outvote a newer success or pending rerun, while a newer failure and failures in
other current checks remain terminal.

When `maxCopilotRounds` is zero or Copilot is unavailable, the canonical review
route is the mandatory `external-review` current-head gate. A configuration
that disables Copilot without requiring that fallback is invalid; enabled,
required Copilot rounds remain routed to Copilot.

A same-head, contract-complete inline marker may be upgraded only through
`scripts/review/repair-gate-evidence.mjs`. The command requires fresh,
head/gate-bound, digest-bearing evidence from distinct reviewers. It edits the
existing comment with an audit block; it never deletes comments, creates a
missing lifecycle gate, repairs a stale head, converts findings to clean, or
accepts a clean verdict when the fan-out artifacts contain findings. An already
correct fanout marker is an idempotent no-op.

## Local wrapper performance

A 20-sample, one-warmup, no-network measurement on 2026-08-26 produced these
wall-clock values. `startup` and `info` are their help paths, so the data covers
local resolution/parser startup only. The preflight comparator is the pinned
package path because the tracked wrapper did not previously exist.

| Entrypoint | Before median / p90 | After median / p90 |
| --- | --- | --- |
| help | 86.4 / 89.3 ms | 85.4 / 87.9 ms |
| loop startup --help | 176.5 / 180.5 ms | 177.1 / 179.7 ms |
| loop info --help | 132.8 / 139.7 ms | 132.2 / 136.0 ms |
| pre-flight --help | 44.4 / 45.4 ms | 86.9 / 90.3 ms |

The tracked preflight's additional package identity and containment checks are
intentional fail-closed overhead, not a speedup. Repeated extension-hook scans
are cached only inside one Pi extension session, keyed by checkout, exact
settings/package roots, manifest size/mtime, selected-agent identity, and the
root/active/future tool sets. There is no process-global or cross-checkout
mutable cache. These local numbers make no claim about provider, network, or
hosted latency. Repository wrappers bound GitHub calls and reject empty or
malformed output. Provider `Request aborted` and WebSocket resume behavior stay
upstream-owned because a repository wrapper cannot reconstruct provider stream
state safely.

## Independent current-head review

Both draft and pre-approval gates make `external-review` mandatory, while
`maxCopilotRounds` remains zero. After committing all intended changes and
fetching `origin/integration`, run the reviewer with a clean worktree. By
default it writes beneath `${XDG_STATE_HOME:-$HOME/.local/state}/oxid/claude-reviews`.
The final directory must be a real, invoking-user-owned `0700` directory and
each artifact is an owned regular `0600` file; a symlink as the final evidence
directory and permissive modes fail closed. The runner resolves the effective
ancestor chain before checking it, so root-owned macOS aliases such as
`/var -> /private/var` are portable while their targets remain authoritative.
An explicit evidence directory must stay outside the checkout; every resolved
ancestor must be owned by root or the invoking user and must not be
group/world-writable without sticky protection. Root and sticky temporary
directories remain valid.

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
supports the deliberately bounded Claude CLI range `>= 2.1.228,< 2.2.0`, parses
exact flag tokens and the `dontAsk` permission choice from captured general
help, and confirms `claude auth status --json` from separately captured auth
help before parsing the actual account response. Evidence binds empty-tool
semantics to the exact captured `--tools ""` help contract and that version
range. A CLI upgrade outside the range requires an explicit source/test update
and a new captured capability pass; do not widen the range speculatively. The
provider prompt receives only the diff artifact basename and digest, never its
local absolute path. Claude runs outside the checkout in safe mode with an
empty tool set and no session persistence, then records CLI account readiness,
version, observed session id, timestamps, raw-output digest, exit status, and a
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
| Reproduction coverage | Repository-owned paths landed | Repository tests cover selected-dev-loop hook execution, root/future tool scopes, package roots, nested-worktree reuse/refusal, preflight resolution, conventional help, gh old/new/malformed versions, attempt-aware CI retries, canonical review routing, fanout evidence-upgrade invariants, integration normalization, REST normalization, Claude invocation/result contracts, policy, and docs. Resume, provider Request-aborted/WebSocket state, and upstream finalization reproduction stay upstream-only. |
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
