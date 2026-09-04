<!-- SPDX-License-Identifier: Apache-2.0 -->

# Streaming milestone delivery authority

Oxid uses short-lived, versioned milestone trains so product work can stream
without turning `develop` or `main` into agent-owned integration branches:

```text
product:  <type>/issue-<number> -> milestone-<x.y.z> -> develop -> main
factory:  <type>/issue-<number> ----------------------> develop
```

`integration` was a temporary branch used to combine Oxid and Lace ID Portal
work. It is not an SDLC tier. The internal CI profile named `integration`
means complete deterministic hosted assurance; it does not name a Git branch.

## Branch contract

- `main` is the stable release branch and GitHub Pages source. Only a human
  release manager promotes `develop` to it, after the complete release gate.
- `develop` is the stable-enough shared engineering baseline. Humans alone
  merge pull requests to it. Factory, harness, CI, documentation, dependency,
  and governance changes may target it directly so factory evolution does not
  perturb an active product train.
- `milestone-<x.y.z>` is a protected, short-lived product integration train,
  for example `milestone-0.4.0`. An authorized factory worker may merge a
  feature PR to it only through the milestone guard after the exact candidate
  satisfies every critical required context.
- Work branches are named exactly `<type>/issue-<number>`, where `type` is an
  allowed Conventional Commit type. The commit and PR title also include an
  approved scope.

Agents never push directly to a milestone, `develop`, or `main`. Promotion
from a milestone to `develop`, promotion from `develop` to `main`, branch
creation/deletion, rulesets, releases, credentials, and security policy remain
human-controlled operations.

Pages is configured for `main` only. Dependency automation inherits the
repository default branch, currently `develop`: do not set Dependabot
`target-branch`, because doing so disables or changes its security-update
behavior. Renovate likewise follows default-branch authority.

## Concurrent milestone trains

One or more milestone trains may be active when product sequencing requires
it. Concurrency is explicit rather than inferred:

1. Each train has a criteria issue or native GitHub milestone that names its
   branch, scope, owner, promotion criteria, and target date.
2. Every product work item records exactly one delivery target
   `milestone-<x.y.z>`. Its branch starts from that fetched remote ref and its
   PR targets the same ref.
3. Factory work records `develop` as its delivery target. Direct-to-`develop`
   authority is limited to factory, harness, CI, documentation, dependency,
   security-policy, and governance scopes; a product feature cannot use it to
   bypass a milestone.
4. A work item and PR never guess the highest or newest version. An absent,
   malformed, missing, or ambiguous target fails before worktree creation.
5. Changes do not leak between trains. A shared fix lands in `develop` through
   human review, then each affected train receives an explicit issue-backed
   sync or backport PR. One issue branch is not merged independently into two
   trains.

Stacked PRs may temporarily target their parent issue branch. After the parent
merges, the child is promptly rebased or retargeted to the same declared
milestone. A stack does not change the work item's delivery target.

## Streaming review and bounded follow-up debt

The factory optimizes for small, coherent increments rather than polishing
every PR in place. A review finding is either **blocking** or **follow-up**.

Blocking findings are limited to:

- a correctness failure in the changed capability or an unmet security,
  privacy, custody, cryptographic, data-integrity, or accepted architecture
  invariant;
- compilation failure or a failure in a critical change-relevant test;
- a missing issue, invalid branch/target, invalid Conventional Commit, DCO or
  OpenPGP evidence, merge conflict, stale head, or secret exposure;
- a PR that is not a coherent usable increment or that misrepresents simulated,
  cached, estimated, or unverified behavior as live truth.

A blocking finding is repaired in the current PR. It cannot be relabeled or
waived by an agent.

A bounded polish, maintainability, additional-test, optional-platform,
documentation refinement, or unrelated baseline finding may move to the next
increment only when all of the following are true:

1. the current PR still satisfies the critical contract above;
2. a concrete open follow-up issue exists with acceptance criteria, target
   milestone or `develop`, and a link to the originating PR;
3. the PR contains one visible triage comment mapping the finding to that issue;
4. the required CI contexts are green at the exact head.

The follow-up issue is delivery work, not a silent waiver. Optional reviews and
non-critical checks inform this triage but are not branch-protection contexts.
“Green” therefore means all hardened critical contexts pass; it does not mean
every advisory reviewer suggested no improvement.

## Gates and authority

Pull requests to a milestone use path- and risk-aware feature gates. The basic
gate, contribution authenticity, scanner, relevant unit/headless coverage,
quality policy, and any risk-escalated lane form the critical set. Optional
platform matrices, independent reviews, and non-critical quality refinements
may remain advisory. A trusted milestone update runs the complete deterministic
hosted backstop so interaction failures stop the train before another merge.

A milestone-to-`develop` promotion PR is human-only and runs the complete
integration assurance profile. A `develop`-to-`main` promotion is human-only
and runs the strongest release profile. No prior feature result substitutes
for exact-head promotion evidence.

The stable aggregate check names remain suitable for branch rulesets and
guarded dynamic selection:

- `Verify commit sign-offs`
- `Repository gate (fmt, architecture, lint, tests, coverage)`
- `Locked Nix package and Compact artifacts`
- `Audit, Licenses, Sources, and Documentation`
- `scan`
- `Check documentation links`

Milestone rulesets require commit authenticity, the repository aggregate, the
locked-Nix aggregate, and the scanner. The milestone guard additionally
requires `quality` or another lane when the risk plan selects it. Documentation
links and broad optional matrices are scheduled/on-demand advisory evidence for
milestones; they become required only when a human promotion or release policy
selects them. The guard always requires freshness, conflict freedom, the
critical target plan, and recorded finding triage. It can merge only to
`milestone-<x.y.z>` and has no administrator bypass.

## Milestone lifecycle

1. **Create:** a human creates the GitHub milestone/criteria issue and protected
   `milestone-<x.y.z>` branch from a named `develop` commit.
2. **Stream:** workers deliver issue-backed increments through the guarded
   exact-head milestone path. A red milestone pauses new automatic merges.
3. **Synchronize:** when shared `develop` changes are needed, an explicit sync
   PR brings them into the train and reruns the complete backstop.
4. **Promote:** a human opens `milestone-<x.y.z> -> develop`, confirms its
   criteria and follow-up inventory, and merges only after complete exact-head
   assurance. When two trains overlap, the later promotion first incorporates
   current `develop` and resolves interaction failures in its own PR.
5. **Close:** preserve the promotion and metric evidence, retarget any legitimate
   remaining work, then let a human delete the milestone branch. Releases are
   tagged only from `main`, never from a milestone branch.

## Local freshness

All comparison and contribution checks use the work item's explicit base:

```bash
delivery_base="origin/milestone-0.4.0" # or origin/develop for factory work
git fetch origin "${delivery_base#origin/}"
base_sha="$(git merge-base HEAD "$delivery_base")"
git diff --check "$base_sha"..HEAD
./scripts/check-dco.sh "$base_sha" HEAD
```

The executable resolution, CI-routing, merge, and follow-up-triage contracts
are implemented separately from this normative policy so their behavior can be
tested without weakening the branch constitution.
