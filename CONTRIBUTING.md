# Contributing to Oxid

Thank you for helping build a free and reusable identity wallet foundation.

## Before you begin

Read `AGENT.md`, `OXID_IDENTITY_WALLET_BLUEPRINT.md`, the
[`RUST_MONOREPO_QUALITY.md`](RUST_MONOREPO_QUALITY.md) non-functional north
star, its
[versioned quality constitution](docs/site/src/quality-constitution.md),
and `docs/issue-branch-delivery.md`. Architectural work must preserve inward
dependencies and include an ADR. Open an issue before a large feature or
dependency migration so its capability boundary and security impact can be
agreed first.

## Development environment

Nix is the supported environment:

```bash
nix develop
```

Run fast validation before opening a pull request:

```bash
./run.sh --light --strict
```

Run the full quality and security gate for dependency, build, CI, or
release-facing changes:

```bash
./run.sh --strict
```

The aggregate gate also enforces at least 80% line coverage across the core and
outgoing adapters. Run it directly with `./run.sh coverage --strict`.

## Pull requests

- Create `<type>/issue-<number>` from the work item's explicit delivery base.
  Product work targets exactly one `milestone-<x.y.z>` train. Factory, harness,
  CI, documentation, dependency, and governance work may target `develop`
  directly. `main` is the human-controlled release branch and Pages source.
  Follow `docs/issue-branch-delivery.md` for the full authority and gate contract.
- Keep one bounded vertical slice per pull request.
- Open the pull request as a draft first.
- Add or update tests and public documentation with behavior.
- Repair critical findings in the current PR. A bounded non-critical review
  finding may be deferred only through a linked follow-up issue and visible PR
  triage comment; optional polish does not strand a coherent green increment.
- Explain security/privacy consequences and migration provenance where relevant.
- Do not include private infrastructure, tracker links, tokens, personal paths,
  pre-production keys, wallet seeds, or generated proof artifacts.
- Complete the pull request checklist and record commands actually run.

Use Conventional Commit and PR titles with a mandatory repository scope, such
as `feat(wallet): create profiles`, `fix(ui): preserve validation feedback`,
or `ci(nix): pin the setup action`. Branches use
`<type>/issue-<positive-number>` with no descriptive suffix, and the branch
type must equal the PR-title type. The closed type/scope vocabulary and
breaking-change rules are defined in
[`docs/factory/contribution-policy.md`](docs/factory/contribution-policy.md).

## DCO and signed commits

Every non-exempt commit must include an exact Developer Certificate of Origin
trailer matching its author identity. Every commit, including generated bot
commits, must carry an OpenPGP signature that GitHub verifies:

```bash
git commit -S --signoff -m "<type>(<scope>): <description>"
git log -1 --show-signature --pretty=fuller
```

Use the mandatory scope in real commands, for example:

```bash
git commit -S --signoff -m "feat(wallet): add profile creation"
delivery_base="${DELIVERY_BASE:?set DELIVERY_BASE to the recorded origin/... ref}"
./scripts/check-dco.sh "$(git merge-base HEAD "$delivery_base")" HEAD
```

Install the repository-scoped local hooks once per clone:

```bash
git config user.signingkey "YOUR_OPENPGP_KEY_ID"
./bootstrap.sh --configure-git
./bootstrap.sh --check
```

The hooks check signing configuration before a commit, reject an invalid
Conventional Commit or missing exact DCO trailer in `commit-msg`, and
cryptographically verify the complete outgoing issue-branch range in
`pre-push` before any objects are transferred. They never add the legal DCO
attestation automatically: continue to use `--signoff`/`-s`. Local hooks are
developer feedback, not a trust boundary; the hosted gate still verifies the
exact PR range and GitHub's OpenPGP result.

Managed worktrees record `branch.<name>.oxidDeliveryBase` in repository-local
Git configuration. If a branch was created manually, set that key to its exact
`origin/milestone-x.y.z` or `origin/develop` target before pushing. The key is
branch-specific, so concurrent sessions do not share mutable target state.

By adding the `Signed-off-by` trailer, you certify the contribution under the
[Developer Certificate of Origin 1.1](https://developercertificate.org/).

## License headers

Contributions must be compatible with Apache-2.0. Add an SPDX header to new
source and script files where practical:

```text
// SPDX-License-Identifier: Apache-2.0
```

Preserve upstream notices and record provenance for migrated or third-party
material.
