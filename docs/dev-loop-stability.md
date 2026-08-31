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
location are outside this bounded claim. Extension results are cached only in
that extension session. The cache key hashes settings, package manifests, and
agent manifests and also binds checkout, active-agent identity, and the
root/active/future tool sets. Same-size rewrites with restored mtimes therefore
invalidate it, and no cache crosses a working directory or Pi session.

Pi 0.84 has no supported cancellation result for `before_agent_start` or
`before_provider_request`. Its runner logs and swallows hook errors, and
`ctx.abort()` before a run has no active run to abort. The extension is therefore
advisory: failures produce prominent input, agent-start, and provider-time
warnings, and each provider check derives the current `getActiveTools()` value.
It never claims to prevent a custom provider that ignores an aborted signal.

Fail-closed enforcement lives at
`node scripts/loop/pre-flight-gate.mjs --check-subagents`. The selected
`dev-loop` must run that tracked wrapper from the canonical worktree before
startup, every routed action, and every delegation, and stop on nonzero status.
The wrapper rejects every non-empty `DEVLOOPS_PREFLIGHT_BYPASS` value and strips
an empty or whitespace-only value before child delegation. The same
caller-injected environment is used for repository scope validation and child
dispatch. The wrapper then
validates exact package and content-bound manifest identity against the
repository's pinned Pi tool contract and delegates worktree/branch checking to
the exact pinned package.
Fix the tracked manifest or exact installation instead of adding aliases.
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

When Pi child/depth markers are present, they are authoritative for the pinned
package's advisory `DEVLOOPS_SUBAGENT_AVAILABLE` input. Depth wins over a stale
explicit value: max depth is unavailable and remaining depth is available.
Explicit `0|1` remains bounded to non-Pi callers. Do not invoke a missing
repository-relative package path or search for a global copy.

`node scripts/dev-loops.mjs --help` and `-h` are exact aliases for `help`.
Other unknown leading options remain rejected. The tracked `loop build-envelope`
route is authoritative for handoff construction: it uses the exact pinned parser,
builder, jq emitter, and validator, loads `.devloops` from the invoking candidate
checkout, and normalizes only the cwd boundary against Git's common-checkout
worktree topology. Main-checkout and linked-worktree invocations both derive
authorized paths from the resolver's issue, PR, local-branch, or phase target; a
candidate cwd never authorizes itself. An identity-matching canonical checkout
is reused;
`local_branch` retains the pinned core's flattened canonical slug, while target
kinds that the repository adapter does not model fail closed. Foreign,
symlinked, missing/relative cwd, mismatched, ambiguous, or nested-namespace
checkout topology fails before an envelope is emitted.

A canonical absent target derived from the main checkout remains a prospective
path under the common root. `loop watch-ci` is
delegated unchanged to `dev-loops@0.9.0`; this repository does not intercept CI
selection.
Obsolete-attempt selection is an upstream/pin residual because a local watcher
cannot safely duplicate expected-check rollup, pagination, suite/attempt
identity, no-check handling, head bracketing, heartbeat/ownership, and global
output-option semantics.

`.devloops` sets `maxCopilotRounds: 0`, caps automatic gate review at two
concurrent reviewers, and stops low-signal refinement. Independent external
review is an explicit high-risk or owner-requested action rather than a
mandatory angle repeated at both gates. Contradictory aggregate loop-info is a
pinned upstream residual. For a draft PR, the pinned gate coordinator remains
the authority: when it explicitly permits `run_draft_gate` under
`requireCi: false`, continue bounded review and keep the PR draft. Stop and
obtain a consistent authoritative state for every other contradiction rather
than overriding the pinned coordinator locally.

There is no gate-evidence repair command. The sanctioned response to incomplete
inline evidence is stop, preserve findings, and re-draft/re-run the canonical
lifecycle. Canonical parser, findings ledger, reviewer identity, mandatory
angles, artifact hashing, and lifecycle coordination remain pinned-tooling
responsibilities; comment-only repair is unsupported and must not be described
as an upgraded gate.

## Local wrapper performance

No portable speed or improvement claim is made. The previous timing table was
removed because its raw samples, clean-head identities, host facts, and exact
commands were not retained. The authoritative pre-flight wrapper deliberately
adds content hashing, exact package identity, and containment checks before the
pinned worktree gate, so callers should expect integrity overhead rather than a
speedup. Repeated extension checks use only one content-safe Pi-session cache;
there is no process-global or cross-checkout cache. Provider, network, hosted,
`Request aborted`, and WebSocket resume latency remain outside this local
no-network contract.

## Independent current-head review

The ordinary draft and pre-approval gates do not invoke an external model.
For a high-risk `full` change, an explicit owner request, or a disputed
finding, commit all intended changes, fetch `origin/integration`, and manually
invoke the reviewer once from a clean worktree. `maxCopilotRounds` remains
zero. By
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
  --effort medium \
  --timeout-ms 300000 \
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
version, selected effort, observed session id, timestamps, raw-output digest,
exit status, and a structured verdict. It checks clean/head/base/exact-diff
facts again afterward.
Timeout, nonzero exit, malformed output, mutation, or a changed ref is a hard
failure. A findings verdict is also a failed gate, but its structured findings
attestation is written first so the next fix pass has durable evidence.
Verification accepts only clean evidence and re-derives all revision facts, so
a push or integration advance makes it stale.

The wrapper passes `--effort medium` by default and records that choice in the
attestation. Medium effort bounds reasoning cost while retaining a substantive
review inside the five-minute default deadline; the example above states that
deadline explicitly so copied commands retain the SLA if defaults later change.
The CLI capability set is closed (`low`, `medium`, `high`, `xhigh`, or `max`),
but high-risk exact-head attestations enforce a recorded floor of `medium`.
The selected level must also appear in the installed CLI's captured choice
list. Accepted help grammar is deliberately narrow: one comma- or
pipe-delimited enumeration must either follow the `--effort <level>` value,
occupy a parenthesized continuation line, or carry an explicit `choices:`
marker; an optional trailing `default:` or `recommended:` token is ignored.
At least two known levels must remain after filtering. Any other layout is a
capability-probe failure that requires a parser/test update. The wrapper and
verifier reject deadlines above
300,000 ms, so increasing effort cannot extend the SLA; a timeout never counts
as a review pass. The recorded budget must be from USD 1 through USD 10, and
the verifier binds that range; the USD 10 default remains a cost ceiling rather
than a spend target. New effort-bound attestations use schema v3. Version 2
records do not bind effort and are intentionally rejected after this upgrade;
rerun the exact-head review to issue a v3 record instead of relabeling old
evidence. These records live in private local state outside the repository; at
this migration checkpoint, the wrapper verifier remains the repository's
evidence authority. In-flight v2 records must be rerun rather than translated.
No attestation artifact or executable consumer was tracked in the repository at
the migration checkpoint; the one known in-flight reviewed PR already required
a fresh exact-head run because its integration base was advancing.
The CLI reports this migration as JSON on stderr with exit status 3; successful
verification JSON remains on stdout.

For a large diff that cannot finish inside five minutes, split the issue and PR
at a coherent architecture boundary; do not lower the attested review beneath
`medium`, lengthen the deadline, or reinterpret a timeout as approval. `high`,
`xhigh`, and `max` are useful only for small,
reasoning-dense diffs that can still finish within the same cap; do not escalate
effort to retry a timed-out large diff. Split that diff instead.

The effort capability record is derived from the captured `--help` artifact,
not from the CLI installed during later verification. Generation requires one
unambiguous comma- or pipe-delimited enumeration in the `--effort` option block,
ignores an optional trailing documented default annotation, then requires the
selected factory-supported level to occur in that enumeration.
The normalized factory-supported subset is recorded. Verification requires that
record to match the subset re-derived from the captured help and binds the
selected effort to it. The raw bounded `--effort` help entry is recorded for
diagnosis. A help-layout change fails closed and requires an explicit parser and
test update. The bounded entry ends at a blank, non-indented, or hyphen-leading
line so a following option cannot supply the effort list; a conflicting layout
also fails closed. This is wrapper-generated operational evidence that the value was
validated and placed in the constructed argv; it cannot independently prove
what argv the process received or that the provider honored the requested
effort internally.

Safety-critical flags remain exact, case-sensitive long-option matches in the
capability probe. Only the non-safety `--effort` entry may include a short alias;
the long option and documented level casing must still match exactly.

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
| Effective repository agent tool allowlists match installed Pi tools before model execution | Repository pin contract enforced; live-runtime mismatch advisory | The tracked pre-flight wrapper fails closed against exact pinned package/manifests and selected `dev-loop` tools. `.pi/extensions/dev-loop-preflight.ts` reports live `getAllTools()`/`getActiveTools()` mismatch but Pi 0.84 cannot hard-cancel these hooks. |
| Project-local package discovery works at root and linked worktrees | Landed in this slice | The bounded tracked resolver and wrappers above |
| Timeout, deadline, `usageBudget`, turn, tool, and control budgets survive resume exactly | Pin-upgrade / upstream-owned | Closed/completed [pi-subagents #985](https://github.com/nicobailon/pi-subagents/issues/985) documents adjacent persisted turn-budget recovery and was fixed by merged [PR #987](https://github.com/nicobailon/pi-subagents/pull/987). The pinned [v0.42.1 async-resume source](https://github.com/nicobailon/pi-subagents/blob/v0.42.1/src/runs/background/async-resume.ts) remains repository authority pending a separately tested upgrade. |
| Provider payload compaction/checkpointing and streamed-mutation retry idempotency | Deferred / **upstream-only** | No exact upstream issue was established during this bounded slice. File a minimal upstream reproduction before claiming a fix; no repository wrapper can safely reconstruct provider stream state. |
| `integration` is the worktree, PR, diff, and evidence base | Landed in this slice | `scripts/loop/ensure-worktree.mjs`, `scripts/dev-loops.mjs`, and the Claude runner |
| Unavailable Copilot review has a bounded independent current-head Claude route | Landed, policy right-sized by issue #161 | Hosted Copilot stays disabled; the tracked Claude runner is manually invoked for high-risk work, an owner request, or a disputed finding rather than every ordinary gate. |
| Valid nested reviewer output cannot be overturned by a late unavailable-tool diagnostic | Deferred / upstream-owned | Closed/completed [pi-subagents #1434](https://github.com/nicobailon/pi-subagents/issues/1434) documents the adjacent final-return serialization failure and was fixed by merged [PR #1448](https://github.com/nicobailon/pi-subagents/pull/1448). The late-diagnostic case still needs its own minimal reproduction and a separately tested repository pin upgrade. |
| Supported GitHub CLI behavior is deterministic | Landed in this slice | Nix pin plus REST behavior probe and timeline resolver |
| dev-loops and pi-subagents share authenticated acceptance provenance | Upstream / pin-upgrade only | This slice records explicit local attestational facts and does not claim reviewer authentication. Closed/completed #1434 and #1460 document adjacent defects fixed upstream by merged PRs #1448 and #1461; neither establishes shared authenticated provenance in pinned v0.42.1. |
| Reproduction coverage | Repository-owned paths landed | Repository tests cover Pi 0.84 runner/provider hook behavior with a local fake provider, current provider-time tool activation/deactivation, root/future tool scopes, content-bound cache invalidation, package roots, issue/PR nested-worktree reuse/refusal, tracked preflight resolution, conventional help, gh old/new/malformed versions, integration normalization, REST normalization, Claude invocation/result contracts, policy, and docs. CI attempt selection, evidence repair, routing contradictions, resume, provider Request-aborted/WebSocket state, and upstream finalization stay upstream/pin-owned. |
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
#1448. The preserved upstream evidence includes the observed workflow failure
`return[0].status ... undefined`; it is not reinterpreted as a repository-local
success. These are distinct invariants. Their upstream completion does not
change the repository's pinned v0.42.1 behavior; only a separately tested pin
upgrade can do that.

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
