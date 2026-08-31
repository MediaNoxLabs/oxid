# AGENT

This file is the small, always-loaded operating contract for work in Oxid. Do
not expand it with delivery history or feature-by-feature status. Load the
[extended agent reference](docs/agent-reference.md), the
[identity-wallet blueprint](OXID_IDENTITY_WALLET_BLUEPRINT.md), and relevant
[ADRs](docs/adr/README.md) only when the task touches their subject.

## Repository purpose

Oxid is a Rust identity wallet organized around hexagonal boundaries. Domain
and application code own policy; ports describe capabilities; incoming and
outgoing adapters own transport, persistence, platform, protocol, and UI
details. Preserve truthful capability labels and fail closed when required live
evidence, custody, proof, or platform support is unavailable.

The extended reference preserves the detailed prototype provenance, shipped
feature state, architecture rationale, security constraints, and historical
delivery notes formerly embedded here. It remains authoritative where this
index points to it; it is not a required read for unrelated work.

## Delivery authority

- Fetch `origin/integration` before starting. It is the only writable delivery and Pages publishing branch. Historical `main` and `develop` are read-only.
- Use a dedicated worktree based on the fetched integration ref. Never develop
  in a dirty primary checkout and never delete unrelated user files.
- Name issue-backed branches `<type>/issue-<number>`, where `type` is the
  Conventional Commit type that will lead the pull-request title. Do not add a
  descriptive suffix.
- Target pull requests at `integration`. Start as draft.
- With explicit authorization in the active user request, automation may merge
  an issue-backed `integration` PR only through
  `scripts/github/merge-integration-pr.mjs` after its exact-head audit passes.
  `main` and `develop` merges remain human-only.
- Every commit and pull-request title follows the repository contribution
  policy: allowed Conventional Commit type and mandatory scope, exact DCO
  sign-off, and a GitHub-verifiable OpenPGP signature. Before any push, verify
  the full local commit range.
- Install and audit the repository-scoped contribution hooks with
  `./bootstrap.sh --configure-git` and `./bootstrap.sh --check`. Hooks provide
  early feedback; hosted exact-head verification remains authoritative.
- Do not push, merge, change repository settings, accept an ADR, tag, or release
  without the authority required by the active user request.

See [integration delivery](docs/integration-delivery.md) for branch protection,
freshness, and exact required contexts.

## Architecture and security invariants

- Dependency direction points inward. Domain/application crates must not import
  UI, storage, network, GitHub, mobile, or concrete protocol adapters.
- The consumer owns each port. Keep composition explicit and adapters thin.
- Do not invent protocol behavior, cryptographic claims, ledger freshness, or
  custody guarantees. Simulated, cached, indexer-supplied, and proven/live
  states stay distinguishable in types and user-facing behavior.
- Never log, persist, upload, or expose secrets, seed material, private keys,
  witnesses, raw credentials, or sensitive identifiers unless an accepted ADR
  explicitly defines the protected boundary.
- Keep unsafe Rust isolated to already-reviewed platform adapter boundaries.
  New unsafe code requires explicit review and an ADR-level justification.
- Preserve immutable dependency revisions, committed `Cargo.lock`, generated
  artifact provenance, and public-repository hygiene.
- Add or update tests and public documentation with behavior. A green aggregate
  must not hide a skipped change-relevant check.

Consult the extended reference before changing custody, identity, credential,
presentation, protocol, wallet, Compact/ZK, mobile persistence, or native
platform boundaries.

## Productive development loop

Follow [the productive loop](docs/factory/productive-loop.md):

- `/dev-loop prototype issue <n>` selects a local, provisional loop for one
  hypothesis: basic plus explicitly relevant focused checks, at most one
  reviewer, no push/PR/hosted-CI wait, and no merge-readiness claim.
- `/dev-loop production-ready issue <n>` selects the normal affected-target,
  draft, CI, and pre-approval loop. It is the default when no profile is named.
- Promotion is explicit: refresh `origin/integration`, audit prototype gaps,
  invalidate provisional evidence, recompute targets, and run production gates.
  Both profiles retain issue/worktree, contribution, security, process, and
  disk invariants.

1. Keep one remotely driven candidate per parent session and at most two active
   managed delivery worktrees per Git common checkout on a host. Parallel
   parents own different issue worktrees.
2. Run the narrowest meaningful check while editing.
3. Use the bounded draft review for direction; it does not wait for CI. When
   aggregate CI is red on a draft, follow gate coordination if it permits
   `run_draft_gate`, keep the PR draft, and repair required evidence before
   pre-approval.
4. Batch accepted findings, run the target plan locally, then push one coherent
   current-head candidate.
5. Run final correctness/security review and hosted CI once.
6. Invoke independent current-head Claude review only for a high-risk/release-profile
   change, an owner request, or a disputed finding.
7. At merge, either hand off to a human or, when the active request explicitly
   authorizes it, use the guarded integration-only merge wrapper.

Automatic review is capped at two concurrent reviewers. Low-signal refinement
stops. Do not add reviewers, retries, retrospective work, or a second gate to
compensate for a provider or transport failure.

Pi retains loaded instructions and extensions. After changing `.pi/`,
`.devloops`, or installed pins, preserve the branch/head, stop Pi, and restart
it from the canonical checkout. Never assume a long-running process is running
the configuration visible in its worktree.

## Validation

Before starting any emulator, simulator, container, heavy build, background
server, watcher, or disposable worktree, load the
[resource-hygiene skill](.claude/skills/resource-hygiene/SKILL.md). Its
receipt-scoped preflight, pressure-stop, sequential-execution, and cleanup
rules are mandatory even when the task is not primarily about resources.

For Lace ID Portal local or Tailscale E2E work, load the
[Portal Pi skill](.pi/skills/oxid-portal-e2e/SKILL.md) and follow the tracked
[macOS laptop](docs/factory/portal-macos-laptop.md),
[mobile simulator](docs/factory/portal-mobile-simulators.md), and
[physical Android Tailnet](docs/factory/portal-android-tailnet-physical.md)
runbooks.

Run focused checks first. The path planner chooses proportional target lanes:

```bash
node scripts/ci/target-plan.mjs \
  --base "$(git merge-base HEAD origin/integration)" \
  --head HEAD \
  --event pull_request \
  --delivery-profile production-ready
```

- L0 `basic`: policy, formatting, architecture, lint, and production compilation
  within five minutes; non-Rust changes avoid the Rust/Nix closure.
- L1 `unit-linux`: workspace unit tests on one target within ten minutes.
- L2 `headless-linux`: hermetic black-box integration within ten minutes.
- L3 lanes: UI profiles, coverage, quality, Nix package, and Compact artifacts
  run independently when selected.

Build/toolchain/lockfile changes and unknown diff state fail closed to every
public hosted target. Pull requests use affected lanes; each `integration`
delivery and release-profile run executes the complete hosted set. Platform,
Docker, cross-repository, and owner-private dependencies are inventoried in
`docs/factory/ci-target-matrix.md`. Quality/scanner schedules and the nightly
hermetic flake check remain backstops.

## Process, disk, and worktree ownership

- One mutating parent session owns one issue worktree. Never attach two writers
  to the same worktree, target directory, branch, or Pi session file. See the
  [worker topology](docs/factory/worker-topology.md) for local and cloud lanes.
- A parent that spawns a process owns its process group and must clean it in an
  exit/signal path. Tests put cleanup in a trap, not after happy-path assertions.
- Rust targets remain worktree-local; compilation reuse comes from the bounded
  shared `sccache`, not shared mutable target trees.
- Pi packages live once in the common checkout and are resolved from linked
  worktrees.
- Audit before cleanup with `node scripts/worktree-lifecycle.mjs audit`.
  Mutating commands require one exact path, expected head SHA, and `--execute`.
  Never bulk-delete based on a branch name or a gone upstream alone.

## Maintaining instructions

Keep this file below 2,000 words. Put durable architecture detail in ADRs,
current product state and historical rationale in `docs/agent-reference.md`, and
factory operations in `docs/factory/`. When instructions conflict, the active
user request and repository delivery/security authority win; record and fix the
contradiction rather than silently choosing the most expensive interpretation.
