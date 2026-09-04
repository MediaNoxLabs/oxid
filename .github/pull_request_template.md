# Overview

<!-- What changed, why, and which user/capability boundary owns it? -->

Delivery target: `milestone-x.y.z` for product work, or `develop` for eligible
factory work. Replace this line with the exact target recorded on the issue.
`develop` and `main` merges are human-controlled.
See `docs/issue-branch-delivery.md`.

## Validation

<!-- List the exact commands run and their results. -->

## Security and privacy

<!-- Describe data, key, consent, protocol, dependency, or workflow effects. -->

## Submission checklist

- [ ] The change is one bounded vertical slice
- [ ] The PR base matches the issue's explicit delivery target
- [ ] Tests cover behavior and boundary failures
- [ ] `./run.sh --light --strict` passed
- [ ] `./run.sh --strict` passed, or the omission is explained
- [ ] Architecture dependencies pass `scripts/check-architecture.sh`
- [ ] Public docs and ADRs are updated where relevant
- [ ] Significant dependencies have a review in `docs/dependencies`
- [ ] No secrets, claims, private identifiers, local paths, or generated state are committed
- [ ] Branch, PR title, and every commit follow `docs/factory/contribution-policy.md`
- [ ] Commits are OpenPGP signed, GitHub-verifiable, and include exact DCO `Signed-off-by` trailers
- [ ] The diff was self-reviewed and the PR remains a draft until gates pass
- [ ] Every finding is fixed as blocking or mapped to a concrete open follow-up issue
- [ ] The final-head private metrics record and bounded closeout comment are complete, or unavailable counters are identified without estimates

## Links

<!-- Link issues, ADRs, specifications, or immutable prototype source commits. -->
