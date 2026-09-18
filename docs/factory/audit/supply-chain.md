<!-- SPDX-License-Identifier: Apache-2.0 -->

# Supply-Chain Audit — `OXA-SUP`

Criteria version: `1`.

## Objective

Establish that what ships is made of what we think it is, from sources we have
reason to trust, and that the machinery deciding this can still say no.

Largely mechanical, and therefore the cheapest type to run often. Its value is
not in judgment but in recurrence: the findings it catches are the ones that
appear between audits without anyone changing anything, because the outside
world moved.

## Scope selector

| Element | Value |
| --- | --- |
| Inputs | manifests and lock files, pinned toolchains, vendored sources, embedded fixtures, container images, workflow action pins |
| Outputs | built artifacts, their receipts, and published packages |
| Branches | every mainline, plus the pinned toolchain each declares |
| Excluded | live network probing beyond read-only registry metadata, any write, any dependency upgrade |

An audit never runs a dependency update. A refresh command that rewrites a lock
file is a remediation with its own review, and running one inside an audit
destroys the evidence the audit exists to record.

## Criteria

| ID | Requirement | Verification |
| --- | --- | --- |
| `OXA-SUP-01` | Every known advisory affecting a dependency is either absent, remediated, or carries a recorded exception naming its class, rationale, and review date. | `advisory.state` anchor against [`docs/security/advisory-exceptions.md`](../../security/advisory-exceptions.md). An exception without a review date is a finding. |
| `OXA-SUP-02` | The advisory gate distinguishes advisory classes and fails on the classes policy says it must. | Read the gate's per-class policy; confirm a vulnerability class fails and that yanked-only status does not silently pass as remediation. |
| `OXA-SUP-03` | A yanked or withdrawn dependency is assessed on its successor's contents, not only on its version number. | For each yanked pin, read what the next version introduces. A successor that adds a build-time dependency on an unfamiliar publisher is a finding, and upgrading to it is not remediation. |
| `OXA-SUP-04` | Every third-party workflow action is pinned to a full commit SHA, not a tag or branch. | Grep `uses:` across workflows; an unpinned or tag-pinned reference is a finding. |
| `OXA-SUP-05` | Toolchain and package pins are exact, and the pin in effect matches the pin declared. | Compare declared pins against the resolved environment; report drift in both directions. |
| `OXA-SUP-06` | Every embedded fixture or vendored source carries verifiable provenance — a recorded digest checked by something that runs. | Resolve each embedded artifact to a lock entry and confirm a check verifies it. An unchecked digest is equivalent to no digest. |
| `OXA-SUP-07` | Pinned upstream references are consistent across documentation and executable use. | Compare each pinned commit or digest between its documented and executed occurrences; disagreement is a finding regardless of which is right. |
| `OXA-SUP-08` | Build artifacts are reproducible from a declared exact input set, and the receipt records that set. | Read the receipt contract; confirm it binds head, tree, profile, manifest digest, target, and toolchain. |
| `OXA-SUP-09` | Workflow permissions are least-privilege, and no workflow grants write scope to untrusted input. | Read `permissions:` per workflow and per job; examine any trigger that runs on unreviewed input. |
| `OXA-SUP-10` | Repository posture matches the checks an external scorer would apply: protected default branch, required review, signed commits, no binary artifacts in tree. | `branch.protection` anchor plus a tree scan; cite the corresponding [Scorecard check](https://github.com/ossf/scorecard/blob/main/docs/checks.md) per row. |
| `OXA-ANY-01` | Every gate this audit relied on fails against a known-bad input. | Name the gate, the known-bad state, and the observed result. |

### On `OXA-SUP-03`

Stated because this repository has already paid for it. A dependency in the
tree was yanked, and the obvious remediation — take the next version — would
have introduced a build-time dependency on a package whose name and publisher
closely imitated a well-known one, pulling a network and TLS stack into the
build. Remaining on the yanked version with a recorded exception was the
correct call.

The criterion exists because the reflex it resists is strong and usually right:
a red advisory gate invites the cheapest action that turns it green. **Yanked
is not compromised, and newer is not safer.** An audit that reports only
"advisory present, upgrade available" has not done this criterion's work.

## Roles

| Role | Angle | Primary criteria |
| --- | --- | --- |
| `dependency-posture` | Advisories, exceptions, yanked assessment, pin exactness | `01`–`03`, `05` |
| `provenance-workflow` | Action pins, fixture provenance, permissions, receipts, repository posture | `04`, `06`–`10` |

Two roles, one pass. This type is cheap by design; run it on a schedule rather
than on request.

## Collectors

`advisory.state`, `branch.protection`, `gate.cannotFail`.

## Exit questions

1. Which advisories are live, and which exception is carrying each?
2. Does any exception lack a rationale or a review date?
3. For each yanked pin, what does its successor actually introduce?
4. Which third-party references are not SHA-pinned?
5. Which pinned reference disagrees between its documented and executed form?
6. Which embedded artifact has a digest that nothing verifies?
