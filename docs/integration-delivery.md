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
policy or guidance. Dependabot and Renovate are configured to open updates only
against `integration`; they cannot update a read-only branch. Open Dependabot
PRs [#138](https://github.com/MediaNoxLabs/oxid/pull/138) and
[#139](https://github.com/MediaNoxLabs/oxid/pull/139) predate that configuration
and remain based on `develop`. They are stale delivery artifacts: close them
after this configuration lands and let the bot recreate any still-applicable
updates against `integration`; do not merge or treat them as current evidence.
Any future promotion requires a separate tracked issue, reviewed repository
policy change, and owner ruleset change before a promotion pull request is
opened; labels and caller prompts cannot bypass the active ruleset.

## One base through the complete loop

Fetch before creating or refreshing a worktree:

```bash
git fetch origin integration
node .pi/npm/node_modules/dev-loops/scripts/loop/ensure-worktree.mjs \
  --repo-root "$PWD" --issue <number> --base origin/integration
```

Create tracker-backed pull requests through the pinned local dev-loops CLI and
always state the base:

```bash
node .pi/npm/node_modules/dev-loops/cli/index.mjs pr create \
  --repo MediaNoxLabs/oxid --base integration --head <branch> \
  --assignee @me --title "<type>: <subject>" --body-file <body-file>
```

The body must contain `Closes #<number>`. Active owner ruleset `21481544` is the
cross-base authority preventing every update to `develop` and `main`. Repository
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

## Required integration protection

The active `integration` protection policy must apply to administrators,
reject force pushes and deletion, require pull requests and signed commits, dismiss stale
reviews, require resolved conversations, and require branches to be current.
Once issue #144 has landed and every workflow has emitted an integration
context, require these exact status checks:

- `Verify commit sign-offs`
- `Validate PR title`
- `Validate PR body`
- `Repository gate (fmt, architecture, lint, tests, coverage)`
- `Locked Nix package and Compact artifacts`
- `Audit, Licenses, Sources, and Documentation`
- `scan`
- `Check documentation links`

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

## Independent current-head review

Copilot review is unavailable and remains disabled with
`refinement.maxCopilotRounds: 0`. The `external-review` angle is configured in
both local `draft` and `preApproval` gates. It requires a manually invoked fresh
independent Claude CLI review and records that evidence in the local gate; it is
not a hosted GitHub status check. Evidence is valid only when it records:

- `claude --version`;
- the exact reviewed head SHA and integration merge-base SHA;
- the immutable diff artifact supplied to the CLI;
- findings (or an explicit `No findings` verdict);
- a review timestamp after the last push.

Any push makes the evidence stale. Run the CLI from outside the checkout in
safe mode with no tools, give it the issue contract plus the exact diff
artifact, fix every accepted finding, and repeat against the new head. This is
a review gate, never merge authorization.
