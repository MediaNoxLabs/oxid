# Contribution Policy

Oxid binds every pull request to an issue, a declared change type, a primary
scope, an exact commit range, and cryptographic authorship evidence. The
machine-readable source is [`.github/contribution-policy.json`](../../.github/contribution-policy.json).
Workflows, local checks, contribution labels, and repository contract tests
consume that one vocabulary.

## Branch and pull-request identity

An issue-backed branch is exactly `<type>/issue-<positive-number>`. It has no
slug or other suffix. The branch type must match the PR-title type:

```text
feat/issue-191
feat(factory): enforce contribution provenance
```

The issue number is the durable identity; a human or agent PR body must close
that exact issue with a supported GitHub closing keyword. Dependabot and
Renovate branches are the only configured branch-name
exceptions because GitHub controls their names. They remain subject to title,
scope, OpenPGP, and label policy.

## Conventional Commits

Every commit subject and PR title is:

```text
<type>(<scope>)[!]: <description>
```

The scope is mandatory. Subjects are at most 100 characters, have no trailing
period, and cannot be fixup, squash, or WIP markers. A `!` requires a non-empty
`BREAKING CHANGE:` footer in the commit body or PR body.

Allowed types are `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`,
`refactor`, `revert`, `style`, and `test`.

| Group | Scopes | Meaning |
| --- | --- | --- |
| Product capabilities | `wallet`, `identity`, `credential`, `presentation`, `protocol`, `vault`, `diagnostics` | Business capability or bounded context |
| Application surfaces | `ui`, `headless`, `mcp` | User, machine, or agent-facing shell |
| Integrations and infrastructure | `mobile`, `platform`, `storage`, `midnight`, `openid`, `portal`, `compact`, `composition` | External boundary, runtime, proof, or wiring |
| Repository and factory | `architecture`, `deps`, `docs`, `harness`, `factory`, `ci`, `nix`, `test`, `security`, `repo` | Engineering policy and delivery infrastructure |

Choose the narrowest truthful scope. `harness` owns Pi/dev-loops execution;
`factory` owns the FSM, leases, metrics, supervisor, and authority protocol;
`test` is shared test infrastructure, not tests for one capability; and `repo`
is reserved for genuinely cross-cutting maintenance. CI routing remains based
on changed paths. A declared scope never suppresses a required target.

Historical names normalize as follows:

- `android` and `ios` → `mobile`
- `openid4vci`, `openid4vp`, and `siopv2` → `openid`
- `brand` → `ui`
- `custody`, `backup`, and wallet recovery → `wallet`
- `adr`, `site`, `design`, and documentation migrations → `docs` or `architecture`
- `cache` → `ci`, `nix`, or `harness`
- generic `adapter` → its concrete integration scope, or `architecture`

## DCO and OpenPGP

Every human or agent-authored commit contains an exact trailer matching its
Git author identity:

```text
Signed-off-by: Author Name <author@example.com>
```

Every commit also contains an OpenPGP signature. The hosted gate checks both
the OpenPGP envelope in the Git object and GitHub's cryptographic verification
result. An SSH or S/MIME “Verified” badge does not satisfy this repository's
OpenPGP rule.

Dependabot and Renovate are exempt only from DCO certification, and only when
both the PR actor and commit author match the closed bot policy. Their commits
still require conventional subjects and GitHub-verified OpenPGP signatures. A
generated update that cannot meet those rules must be recreated on a normal
issue branch; its gate is not waived.

The required check retains the historical name `Verify commit sign-offs` so
the active ruleset remains effective while its implementation now verifies the
full commit policy. A trusted `pull_request_target` workflow reads commit
metadata through GitHub's API, executes policy code from the exact trusted
workflow commit (`github.workflow_sha`), and posts the result directly to the
exact PR head SHA. It never checks out or
executes candidate files. The PR-title and body contexts use the same pattern,
so a PR cannot approve a weakened workflow or checker included in its own diff.

### Completed rollout

GitHub discovers `pull_request` workflow definitions from the candidate merge
ref but `pull_request_target` definitions from the trusted base ref. Therefore
the PR that installs the trusted workflows cannot also delete the legacy
required-context workflows: doing so would leave that bootstrap head with no
producer for its required contexts. Issue
[#193](https://github.com/MediaNoxLabs/oxid/issues/193) completes the bounded
second phase by deleting the two legacy workflows. The trusted workflows are
now the sole context producers.

### Required commit evidence and advisory metadata

Conventional Commits, DCO, and OpenPGP prove the provenance of the exact commit
history and remain a required merge status. Draft work can continue through the
draft gate while that status is red, but the history must be repaired before
pre-approval or merge.

PR title, scope, branch, and body checks are advisory. They publish successful
exact-head statuses with explicit advisory descriptions and workflow warnings
when metadata is invalid. This keeps actionable feedback visible without
turning administrative metadata into a merge blocker.

## Contribution labels

Each valid PR receives exactly one `type:<type>` and one `scope:<scope>` label
derived from its title. A metadata-only `pull_request_target` workflow reads
trusted base policy and never checks out candidate code. Other labels are
preserved.

PR labels are pull-request metadata, so the workflow grants only
`pull-requests: write` alongside `contents: read`. It does not grant Issues
write or alter the repository-wide default workflow permissions.

The label catalog is deterministic. Preview or synchronize it with:

```bash
node scripts/github/sync-contribution-labels.mjs
node scripts/github/sync-contribution-labels.mjs --execute
```

Label metadata helps routing and metrics; it is not permission to trust the
declared scope over the actual diff. Classification itself is informational:
an invalid title or label API failure emits a workflow warning but is not a
merge gate.
