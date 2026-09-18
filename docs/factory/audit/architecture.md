<!-- SPDX-License-Identifier: Apache-2.0 -->

# Architecture Audit — `OXA-ARC`

Criteria version: `1`.

## Objective

Establish whether the declared boundaries still constrain the code, and whether
the decision records still describe the system that exists.

The failure this type exists to catch is not a violated boundary — the
architecture gate catches those and fails the build. It is a boundary that has
become **decorative**: a ceiling nothing approaches from below, a governed set
that covers three files out of forty-five, a record marked accepted that the
code stopped honouring, a ratchet whose tightening clause nothing enforces. A
constraint that cannot bind is indistinguishable from an absent constraint,
except that it reports success.

## Scope selector

| Element | Value |
| --- | --- |
| Units | every workspace member, the allowlist governing it, and the decision records that claim to bind it |
| Artifacts | boundary allowlists, façade ceilings, decision records and their index, public interfaces between layers |
| Branches | every mainline, because record corpora diverge and merge silently |
| Excluded | internal implementation quality, naming preference, anything the rubric's "not a finding" list covers |

## Criteria

| ID | Requirement | Verification |
| --- | --- | --- |
| `OXA-ARC-01` | Every workspace member is governed by the boundary allowlist, and the policy is default-deny so a new member cannot escape governance by omission. | Diff workspace members against allowlist entries; confirm the default branch of the checker denies rather than permits. |
| `OXA-ARC-02` | Core domain, port, and application crates depend on no external technology. | Read each core crate's manifest; any technology dependency is a finding. |
| `OXA-ARC-03` | A façade ceiling binds: the governed set covers the files that actually grow, and the ratchet clause each record states is enforced by something that runs. | `facade.headroom` anchor. Report headroom per governed file, and the size of the largest ungoverned sibling in each governed crate. |
| `OXA-ARC-04` | The decision-record corpus is internally consistent: no duplicate numbers across mainlines, no dependency on a record whose status does not authorize one, no backlink the lint cannot evaluate. | `adr.collisions` anchor, evaluated against the merged corpus rather than either branch alone. |
| `OXA-ARC-05` | A recurring concern has one implementation: admission control, deadlines, cancellation, and error propagation each use one primitive rather than per-site reinvention. | Enumerate implementations of the concern; report the count, and for each whether it bounds the caller's wait and how it handles failure. |
| `OXA-ARC-06` | Closed domain concepts cross layer boundaries as closed types, not as strings that a consumer re-parses. | Trace each closed enum to its boundary; a display string re-parsed by consumers is a finding, and a default arm on an unknown value is the failure mode to name. |
| `OXA-ARC-07` | Every accepted decision record describes behaviour the code currently exhibits. | For each record touched by the window, read its enforcement point. Report the pair. |
| `OXA-ARC-08` | Public interfaces between layers are stable and documented where another layer depends on them. | Read cross-layer signatures against their documented contract. |
| `OXA-ANY-01` | Every gate this audit relied on fails against a known-bad input. | Name the gate, the known-bad state, and the observed result. |

### On `OXA-ARC-03`

Three properties, because a ratchet fails in three independent ways and an
audit that checks only the first learns nothing.

**Headroom** — a ceiling a file sits twenty lines beneath is about to be raised
or worked around, and either way it has stopped governing.

**Coverage** — a governed set of three files in a forty-five crate workspace
constrains three files. Report the largest ungoverned sibling, because that is
where the mass moves to.

**Tightening** — most ratchet records state that a lower total lowers the
maximum. That clause is the ratchet; without something that enforces it, a
one-time reduction leaves the ceiling where it was and the next growth is free.
Verify the clause is enforced, not merely written.

### On `OXA-ARC-04`

Corpora on two mainlines diverge silently. Two records can take the same number
on different branches with **distinct filenames**, so the merge produces no
conflict, keeps both files, and a lint that resolves a number by globbing picks
one alphabetically. The audit must reconstruct the merged corpus and run the
real lint against it, rather than checking each branch in isolation and finding
both clean.

Status also matters and lints rarely see it. An accepted record carrying an
unconditional backlink to a proposed record asserts a dependency the proposed
record's own status does not authorize.

## Roles

| Role | Angle | Primary criteria |
| --- | --- | --- |
| `boundary-conformance` | Allowlist coverage, core purity, interface stability | `01`, `02`, `08` |
| `ratchet-governance` | Façade headroom, coverage, tightening enforcement | `03` |
| `concern-duplication` | Admission control, deadlines, cancellation, state modelling | `05`, `06` |
| `record-integrity` | Corpus consistency, status-aware dependencies, record-versus-code | `04`, `07` |

Four roles, two passes of two.

## Collectors

`facade.headroom`, `adr.collisions`, `mainline.divergence`, `gate.cannotFail`.

## Exit questions

1. Which governed file has the least headroom, and which ungoverned sibling is
   the largest?
2. Which ratchet clauses are written but unenforced?
3. Does the *merged* record corpus lint clean?
4. Which recurring concern has the most independent implementations, and how
   many of them bound the caller's wait?
5. Which accepted record no longer describes the code?
6. Which closed concept is currently travelling as a string?
