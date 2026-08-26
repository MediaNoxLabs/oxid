# Overview

<!-- What changed, why, and which user/capability boundary owns it? -->

Target branch: `integration`; historical `main` and `develop` are read-only.
See `docs/integration-delivery.md`.

## Validation

<!-- List the exact commands run and their results. -->

## Security and privacy

<!-- Describe data, key, consent, protocol, dependency, or workflow effects. -->

## Submission checklist

- [ ] The change is one bounded vertical slice
- [ ] Tests cover behavior and boundary failures
- [ ] `./run.sh --light --strict` passed
- [ ] `./run.sh --strict` passed, or the omission is explained
- [ ] Architecture dependencies pass `scripts/check-architecture.sh`
- [ ] Public docs and ADRs are updated where relevant
- [ ] Significant dependencies have a review in `docs/dependencies`
- [ ] No secrets, claims, private identifiers, local paths, or generated state are committed
- [ ] Commits are GPG signed and include DCO `Signed-off-by` trailers
- [ ] The diff was self-reviewed and the PR remains a draft until gates pass

## Links

<!-- Link issues, ADRs, specifications, or immutable prototype source commits. -->
