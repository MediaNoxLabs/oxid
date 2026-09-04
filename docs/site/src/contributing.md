# Contributor quickstart

The full policy lives in
[`CONTRIBUTING.md`](https://github.com/MediaNoxLabs/oxid/blob/develop/CONTRIBUTING.md);
this page is the five-minute version for a human who wants to build, test,
and land one change.

## Build and test — this is all you need

```bash
nix develop
just check
```

That is the entire local requirement. The agent tooling you may see
referenced in the repository (Pi packages, dev-loops review gates) is **not**
needed to build, test, or contribute — the devshell installs it only in
interactive shells, and public packages need no credentials.

## The rules that will actually affect your change

1. **Architecture is enforced.** Domain and application crates take no
   external dependencies; adapters convert external types at the boundary;
   new crates must be added to the `scripts/check-architecture.sh`
   allowlist or the gate fails. Read [Architecture](architecture.md) first.
2. **Secrets never cross surfaces.** No key material, claim values, proofs,
   or tokens in DTOs, logs, fixtures, or committed config — reviewers and
   tests both check this.
3. **Architectural changes need an ADR.** Follow the format in
   [`docs/adr/`](https://github.com/MediaNoxLabs/oxid/tree/develop/docs/adr);
   an accepted ADR is binding even before it is fully delivered.
4. **Conventional commits, signed and signed-off.** `type(scope): subject`,
   GPG-signed (`-S`) with a DCO trailer (`-s`). CI verifies both on pull
   requests.
5. **Truthful capability labels.** If your change simulates, defers, or
   process-locals something, the UI copy and the headless manifest must say
   so. Overclaiming is treated as a defect.

## Landing a change

Open a draft pull request from `<type>/issue-<number>` against the exact target
recorded on the issue: a `milestone-<x.y.z>` train for product work, or
`develop` for eligible factory work. Let the path- and risk-relevant CI gate
run, then classify review findings. Correctness and invariant failures stay in
the PR; bounded non-critical debt needs a linked follow-up issue. Merges are
squash merges. An authorized factory worker may use the exact-head guard only
for milestone delivery. Merges to `develop` and `main` remain human-only.

Good first contributions: issues labeled
[`enhancement`](https://github.com/MediaNoxLabs/oxid/issues?q=is%3Aissue+is%3Aopen+label%3Aenhancement)
with acceptance criteria in the body — the backlog is written to be
executable without tribal knowledge.
