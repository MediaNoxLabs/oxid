<!-- SPDX-License-Identifier: Apache-2.0 -->

# Integration delivery authority

`integration` is Oxid's default branch, only writable delivery branch, and sole
GitHub Pages publishing source. Issue-backed product, refactor, quality, and
tooling work starts from `origin/integration`, opens a pull request to
`integration`, and is compared with that same base through review and merge.
Active repository ruleset `21481544` makes the historical `main` and
migration-era `develop` branches
read-only with no bypass actors; neither branch builds or deploys Pages.

There is no `integration -> main` release-promotion exception in repository
policy or guidance. Dependabot and Renovate derive their update base from the
repository's GitHub default branch rather than repeating `integration` in bot
configuration. This keeps default-branch authority in one owner-managed setting
and lets Dependabot security updates operate; an explicit Dependabot
`target-branch` disables security updates for that ecosystem. Open Dependabot
PRs [#138](https://github.com/MediaNoxLabs/oxid/pull/138) and
[#139](https://github.com/MediaNoxLabs/oxid/pull/139) predate the default-branch
change and remain based on `develop`. They are stale delivery artifacts: close
them after this configuration lands and let the bot recreate any
still-applicable updates against `integration`; do not merge or treat them as
current evidence.
Any future promotion requires a separate tracked issue, reviewed repository
policy change, and owner ruleset change before a promotion pull request is
opened; labels and caller prompts cannot bypass the active ruleset.

## One base through the complete loop

Fetch before creating or refreshing a worktree:

```bash
git fetch origin integration
node scripts/loop/ensure-worktree.mjs \
  --repo-root "$PWD" --issue <number>
```

The tracked wrapper supplies `--base origin/integration` and rejects any
conflicting caller value.

Create tracker-backed pull requests through the pinned local dev-loops CLI and
always state the base:

```bash
node scripts/dev-loops.mjs pr create \
  --repo MediaNoxLabs/oxid --head <branch> \
  --assignee @me --title "<type>: <subject>" --body-file <body-file>
```

The tracked wrapper supplies `--base integration` and rejects any conflicting
caller value. Use `Closes #<number>` only when the PR completes that issue's
contract. A bounded partial slice must use `Refs #<number>`, enumerate the
remaining rows, and leave the issue open (or move them to an explicitly linked
follow-up issue). Active owner ruleset `21481544` is the cross-base authority
preventing every update to `develop` and `main`. Repository
workflows deliberately make no cross-base enforcement claim: a workflow loaded
from one pull request base cannot authoritatively guard another base, and an
advisory base check would create false failures for stacked pull requests.

Local review commands must use the fetched integration ref rather than a
caller-selected default:

```bash
base_sha="$(git merge-base HEAD origin/integration)"
git diff --check "$base_sha"..HEAD
git diff "$base_sha"..HEAD
```

The pull request's `baseRefName` and base SHA are authoritative for hosted diff,
freshness, and conflict checks. If either is not `integration`, or if local and
GitHub facts disagree, stop. Immediately before merge, refresh and check
freshness plus conflict-freedom explicitly. The `git merge-tree --write-tree`
form requires Git 2.38 or newer:

```bash
git fetch origin integration
git merge-base --is-ancestor origin/integration HEAD
git merge-tree --write-tree origin/integration HEAD >/dev/null
```

Rerun every current-head gate after any integration update.

An agent may perform the squash merge only when the active owner request
explicitly authorizes automated merges for the current work and only through
the repository guard:

```bash
node scripts/github/merge-integration-pr.mjs \
  --repo MediaNoxLabs/oxid --pr <number> \
  --authorized-by-owner --execute
```

Run the command alone after writing and verifying gate evidence. The guard
accepts only an open, non-draft, issue-backed `integration` PR; rejects
cross-repository heads and title blockers; refreshes the exact base and head;
checks ancestry and the merge tree; requires every protected check including
GPG/DCO; delegates checkpoint and conversation verification to the pinned
dev-loops gate; re-reads the head and base; and pins the squash merge with
`--match-head-commit`. Without `--execute` it is a read-only audit. The wrapper
has no `--admin` bypass. `main` and `develop` are always human-only, as are
repository settings, tags, releases, and ADR acceptance.

## Required integration protection

The active `integration` protection policy must apply to administrators,
reject force pushes and deletion, require pull requests and signed commits,
require resolved conversations, and require branches to be current. Its review
settings intentionally use `required_approving_review_count: 0` and
`require_code_owner_reviews: false`: preserving the historical `main` human or
code-owner approval policy is not a delivery requirement. The repository-local
harness sets `autonomy.humanMergeOnly: false` and `autonomy.stopAt: []` so an
actively owner-authorized run can reach the integration-only guarded merge.
This does not authorize `main` or `develop`. Once
issue #144 has landed and every workflow has emitted an integration context,
require these exact status checks:

- `Verify commit sign-offs`
- `Repository gate (fmt, architecture, lint, tests, coverage)`
- `Locked Nix package and Compact artifacts`
- `Audit, Licenses, Sources, and Documentation`
- `scan`
- `Check documentation links`

The authorship context is required. `Validate PR title` and `Validate PR body`
keep their stable context names but are advisory. Contribution label
classification is advisory too. The three commit-status contexts are emitted
by trusted `pull_request_target` workflows onto the exact PR head SHA. Those
workflows execute only base-commit policy code, treat PR fields and commit API
responses as untrusted data, and never check out candidate files. This avoids
granting a candidate workflow authority to weaken the policy that judges its
own commits.

The migration is intentionally two-phase because GitHub resolves the two pull
request event types from different refs. Issue
[#193](https://github.com/MediaNoxLabs/oxid/issues/193) removes the legacy
bootstrap workflows immediately after the trusted workflows land.

Candidate pull requests may introduce same-repository links whose durable
`blob/integration/` destination does not exist on the base yet. The documentation
link wrapper validates only that exact URL prefix against tracked regular files
in the candidate checkout, rejects ambiguous or escaping paths, and remaps those
targets to local files for Lychee. This is a candidate substitution, not an
exclusion: every other URL retains Lychee's normal validation.

The workflow-contract test protects these names and trigger semantics in the
repository; branch rules are owner-managed GitHub state and must be verified
separately after changes. Ruleset `21481544` must continue to prohibit updates,
deletion, and non-fast-forward changes on both `main` and `develop`, with no
bypass actors. The Pages workflow must trigger and deploy only from
`integration`. The owner-managed `github-pages` environment must also retain
custom deployment branch policy `58259903`, whose only allowed branch is
`integration`; workflow triggers alone cannot bypass an environment policy.
Verify both owner controls after any settings change. Do not widen workflow
permissions to make a check required.

The two CI job names above are stable branch-protection aggregators, not a
promise that every diff runs the same command. `scripts/ci/target-plan.mjs`
maps the exact base-to-head paths to component lanes under one of three
profiles:

| Profile | Repository event | Required work |
| --- | --- | --- |
| `feature` | pull request | L0 basic plus the affected host, integration, quality, package, and artifact lanes |
| `integration` | delivery push | every deterministic public hosted lane, in parallel |
| `release` | explicit manual run | every deterministic public hosted lane; device/private release evidence remains explicit |

Documentation, harness, and CI-only feature changes run the basic contracts
without realizing the Rust/Nix graph. Shared core and unknown paths remain
conservative. Missing refs or an empty diff fail closed to the complete public
hosted set. The scheduled nightly still runs the complete hermetic flake check.
The commands, budgets, dependencies, mobile/live gaps, and promotion criteria
are versioned in [the CI target matrix](factory/ci-target-matrix.md).

## Independent current-head review

Copilot review is unavailable and remains disabled with
`refinement.maxCopilotRounds: 0`. A manually invoked fresh independent Claude
CLI review is reserved for release-profile/high-risk changes, an explicit owner request,
or a disputed finding; it is not repeated at both ordinary gates. It records a
local attestation, not a hosted GitHub status check or authenticated reviewer
identity. The attestation is usable only when it records:

- `claude --version`;
- the exact reviewed head SHA and integration merge-base SHA;
- the immutable diff artifact supplied to the CLI;
- findings (or an explicit `No findings` verdict);
- a review timestamp after the last push.

When this high-risk review is required, any push makes the evidence stale. Run the tracked
`scripts/review/claude-current-head.mjs` wrapper with a clean checkout and a
private XDG state directory (or an equally hardened explicit directory). It
invokes the CLI from outside the checkout in safe mode with no tools, supplies
caller-provided issue scope plus the exact diff artifact, and fails on stale
state or malformed output. Findings are persisted as structured attestational
evidence before the gate fails. Verify only a saved clean artifact with the same
wrapper, fix every accepted finding, and repeat against the new head. See
`docs/dev-loop-stability.md` for the exact command, limitations, and evidence
shape. Post required high-risk current-head evidence to the pull request before
merge; that evidence is the local attestation described above. Integration
branch protection intentionally does not require a hosted human or code-owner
approval. The local harness either hands off at merge or, under explicit active
owner authorization, runs the guarded integration-only merge command.
