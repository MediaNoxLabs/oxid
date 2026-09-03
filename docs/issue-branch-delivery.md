<!-- SPDX-License-Identifier: Apache-2.0 -->

# Issue-branch delivery authority

Oxid uses a two-stage durable branch model:

```text
<type>/issue-<number> -> develop -> main
```

`integration` was a temporary branch used to combine Oxid and Lace ID Portal
work. It is not a permanent SDLC tier and must not be used as a worktree base,
pull-request target, cache authority, Pages source, or default branch.

## Branch contract

- `main` is the stable release branch and GitHub Pages source. Promotion to it
  is human-controlled and must pass the complete release gate.
- `develop` is the shared development branch. Issue-backed pull requests target
  it. A trusted push to it runs the complete deterministic hosted suite and may
  seed shared compiler caches.
- Work branches are named exactly `<type>/issue-<number>`, where `type` is one
  of the repository's allowed Conventional Commit types (`feat`, `fix`, `docs`,
  `refactor`, `perf`, `test`, `build`, `ci`, `chore`, or `revert`). The commit
  and PR title must also include an approved scope.

Fetch the durable base before starting or refreshing a managed worktree:

```bash
git fetch origin develop
node scripts/loop/ensure-worktree.mjs \
  --repo-root "$PWD" --issue <number>
```

The worktree wrapper pins `origin/develop`; a conflicting base fails closed.
Create the pull request through the tracked wrapper:

```bash
node scripts/dev-loops.mjs pr create \
  --repo MediaNoxLabs/oxid --head <type>/issue-<number> \
  --assignee @me --title "<type>(<scope>): <subject>" \
  --body-file <body-file>
```

The wrapper pins `develop`. Use `Closes #<number>` only when the PR completes
the issue. A partial slice uses `Refs #<number>` and leaves the issue open.

## Gates and promotion

Pull requests to `develop` use the path-aware `feature` profile. The basic gate
always runs; affected unit, headless, UI, coverage, quality, Nix, and Compact
lanes fan out independently. Documentation, harness, and CI-only changes avoid
the Rust/Nix closure unless they modify a fail-closed build input.

A trusted push to `develop` uses the internal `integration` assurance profile.
Here, “integration” names a complete test profile—not a Git branch. Pull
requests and pushes to `main` use the complete `release` profile. The stable
required contexts are:

- `Verify commit sign-offs`
- `Repository gate (fmt, architecture, lint, tests, coverage)`
- `Locked Nix package and Compact artifacts`
- `Audit, Licenses, Sources, and Documentation`
- `scan`
- `Check documentation links`

`develop` requires these checks, signed commits, and a current conflict-free
head but no additional approval while the core team is small. `main` additionally
requires human/code-owner review. Force pushes and deletion are prohibited on
both durable branches. The repository default branch is `develop`, so dependency
bots naturally open updates against the correct target; do not add a Dependabot
`target-branch`, because that changes its security-update behavior.

## Guarded exact-head merge

When the active owner request explicitly authorizes it, an agent may merge an
issue-backed `develop` PR only through:

```bash
node scripts/github/merge-develop-pr.mjs \
  --repo MediaNoxLabs/oxid --pr <number> \
  --authorized-by-owner --execute
```

The guard requires an open non-draft PR that closes a substantive issue,
refreshes the exact base and head, checks ancestry and conflict freedom,
requires all protected contexts including GPG/DCO, verifies current-head gate
evidence and resolved conversations, re-reads both SHAs, and pins the squash
merge with `--match-head-commit`. It has no administrator bypass and cannot
merge to `main`. Without `--execute`, it is a read-only audit.

## Freshness and local policy

All local comparison, review, and contribution checks use the same base:

```bash
base_sha="$(git merge-base HEAD origin/develop)"
git diff --check "$base_sha"..HEAD
./scripts/check-dco.sh "$base_sha" HEAD
```

Repository hooks validate branch grammar, Conventional Commit scope, exact DCO
identity, and OpenPGP signatures before a candidate is published. Current-head
review evidence becomes stale after either the work branch or `develop` moves.

Candidate documentation may link to files that exist only on the proposed
`develop` tree. `scripts/docs/check-links.mjs --candidate` validates exact
`blob/develop/` repository links as tracked regular files and remaps only those
links to the candidate checkout for Lychee; other URLs receive normal outbound
validation.

## Temporary-branch retirement

Delete `integration` only after this contract lands, GitHub's default branch is
`develop`, remaining open PRs have been retargeted or closed, Pages is configured
for `main`, the `develop` and `integration` trees are rechecked as identical at
the migration boundary, and no workflow or harness component still consumes
the temporary ref. Branch deletion is an owner-authorized, one-time migration
step—not a recurring factory action.
