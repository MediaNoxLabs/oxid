<!-- SPDX-License-Identifier: Apache-2.0 -->

# Integration delivery authority

`integration` is Oxid's delivery branch. Issue-backed product, refactor,
quality, and tooling work starts from `origin/integration`, opens a pull request
to `integration`, and is compared with that same base through review and merge.
`main` remains the release branch and `develop` remains available during the
migration; neither is the normal base for new issue-backed work.

The only issue-backed wrong-base exception is an `integration -> main` release
promotion. Dependency automation without an issue-closing reference is outside
this metadata contract. Any future exception needs a separately reviewed
repository change; labels and caller prompts cannot bypass the check.

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

The body must contain `Closes #<number>`. The protected base-branch `pull_request_target` metadata workflow is checkout-free. Install the same workflow on every retained
PR base (`integration`, `develop`, and `main`). Its `Require integration for issue-backed PRs` job rejects any other base except the
release promotion above.

Local review commands must use the fetched integration ref rather than a
caller-selected default:

```bash
base_sha="$(git merge-base HEAD origin/integration)"
git diff --check "$base_sha"..HEAD
git diff "$base_sha"..HEAD
```

The pull request's `baseRefName` and base SHA are authoritative for hosted diff,
freshness, and conflict checks. If either is not `integration` (outside the
release exception), or if local and GitHub facts disagree, stop. Immediately
before merge, refresh and check freshness plus conflict-freedom explicitly. The
`git merge-tree --write-tree` form requires Git 2.38 or newer:

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

- `Require integration for issue-backed PRs`
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
separately after changes. Do not widen workflow permissions to make a check
required.

## Independent current-head review

Copilot review is unavailable and remains disabled with
`refinement.maxCopilotRounds: 0`. The `external-review` gate instead requires a
fresh independent Claude CLI review. Evidence is valid only when it records:

- `claude --version`;
- the exact reviewed head SHA and integration merge-base SHA;
- the immutable diff artifact supplied to the CLI;
- findings (or an explicit `No findings` verdict);
- a review timestamp after the last push.

Any push makes the evidence stale. Run the CLI from outside the checkout in
safe mode with no tools, give it the issue contract plus the exact diff
artifact, fix every accepted finding, and repeat against the new head. This is
a review gate, never merge authorization.
