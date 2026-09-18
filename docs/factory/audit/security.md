<!-- SPDX-License-Identifier: Apache-2.0 -->

# Security Audit — `OXA-SEC`

Criteria version: `1`.

## Objective

Establish whether the security properties the product **claims** actually hold.
Not whether the code looks careful — whether each stated invariant is enforced
at every path that can reach it, and whether the manifest, the consent surface,
and the decision records tell the truth about what the wallet does.

This is a custody product. The asymmetry that shapes the whole type: a key
derivation policy, an envelope version, or a consent string is nearly free to
correct before the first user artifact exists and becomes a migration
afterwards. Findings in that class carry an expiry and invoke ranking rule 4.

## Scope selector

| Element | Value |
| --- | --- |
| Paths | custody, backup, vault, key derivation, consent and confirmation paths, capability manifest, headless dispatch, trust anchors, deployment profiles, platform bridges |
| Branches | every mainline; a defect present on one mainline and fixed on another is reported as both a fix and a divergence |
| Depth | every enforcement point on every reachable path, not a sample — an invariant enforced at four of five call sites is not enforced |
| Excluded | credentials, key material, session transcripts, live network probing, any write |

The depth rule is the reason a security audit costs more than the other types.
Sampling is valid for style and for debt; it is not valid for an invariant,
because the finding *is* the one unguarded path.

## External baseline

Criteria map to the [OpenSSF Open Source Project Security
Baseline](https://baseline.openssf.org/versions/2026-02-19.html) where a
control applies, so conformance is citable against an external standard rather
than only against local taste. Target maturity is **Level 2**. Where a local
criterion is stricter than the Baseline, the local one governs and says so.

| Baseline domain | Local coverage |
| --- | --- |
| Access Control (`OSPS-AC`) | `OXA-PRC-05`, and `OXA-MIL-01` per train |
| Build and Release (`OSPS-BR`) | `OXA-SUP-*` |
| Documentation (`OSPS-DO`) | `OXA-SEC-09` |
| Quality (`OSPS-QA`) | `OXA-PRC-02`, `OXA-PRC-03` |
| Security Assessment (`OSPS-SA`) | `OXA-SEC-10` |
| Vulnerability Management (`OSPS-VM`) | `OXA-SUP-01`, `OXA-SUP-02` |

## Criteria

| ID | Requirement | Verification |
| --- | --- | --- |
| `OXA-SEC-01` | Every sealing format version that carries secret material derives its key under the strongest policy the format supports; legacy policies apply only to versions that can no longer seal. | Enumerate format versions; for each, determine whether it is reachable from a seal path and which policy it maps to. Report the pair for every version, not only the violations. |
| `OXA-SEC-02` | Secret material exists in memory only in types that zeroize, for the shortest reachable span, on every platform path including foreign-function bridges. | Trace each secret from origin to last use across the platform boundary; immutable host-language strings are findings. |
| `OXA-SEC-03` | Every sensitive command pins its consent text to an application-owned constant; the confirmation the user sees is the one the command verifies. | Enumerate sensitive commands; for each, determine whether consent is pinned or merely shape-checked. Report the full table. |
| `OXA-SEC-04` | The capability manifest is truthful: every dispatchable method is declared, and each declared flag matches the behaviour of the code that serves it. | Diff declared methods against dispatch arms in both directions; verify each truthfulness flag against its implementation. |
| `OXA-SEC-05` | Recovery, unlock, and authorization paths are fail-closed at every layer that can be entered independently. | For each path, confirm the guard exists at both the application and adapter layer; a guard at one layer only is a finding. |
| `OXA-SEC-06` | Diagnostics, logs, and telemetry cannot carry secret or personally identifying payloads by construction. | Confirm the carrier type is closed and payload-free; an open string carrier is a finding regardless of current call sites. |
| `OXA-SEC-07` | Trust anchors and deployment profiles that ship are sourced from paths governed as shipped artifacts. | Resolve each embedded anchor to its source path; a shipped anchor read from a test-fixture tree is a finding even when its bytes are correct. |
| `OXA-SEC-08` | Persisted secret stores are created with owner-only permissions on every directory and file in the path, not only the leaf. | Read the creation path; confirm the mode applies to each component created. |
| `OXA-SEC-09` | Published security claims — in documentation, onboarding copy, and the site — match enforced behaviour. | For each published claim, read its enforcement point. A dismissible notice contradicting a "non-dismissible" claim is a finding against whichever is wrong. |
| `OXA-SEC-10` | A current threat model exists, covers each trust boundary the product actually has, and names which parties are trusted for what. | Read the threat model against the deployed topology; an absent or stale boundary is a finding. |
| `OXA-SEC-11` | Endpoint, URL, and identifier validation applies one policy from one implementation; divergent copies are findings even when each copy is individually defensible. | Enumerate validators; compare accepted grammar, scheme policy, and length limits including the unit of measurement. |
| `OXA-ANY-01` | Every gate this audit relied on fails against a known-bad input. | Name the gate, the known-bad state, and the observed result. |

### On trust boundaries

`OXA-SEC-10` requires care with this product's topology, because getting it
wrong in either direction produces a wrong audit. The indexer and node are
**peer services offered by the Midnight network**, whose endpoints the network
exposes: depending on them is an availability and network-participation
dependency, not a delegation of secrets. The **proof server** is different in
kind, because it receives witness material. An audit that treats all three as
equivalent trust-bearing infrastructure overstates the exposure; one that
treats none of them as a boundary understates it.

## Roles

| Role | Angle | Primary criteria |
| --- | --- | --- |
| `custody-crypto` | Format versions, key derivation, seed lifetime, zeroization | `01`, `02`, `08` |
| `consent-manifest` | Consent pinning, manifest truthfulness, fail-closed paths | `03`, `04`, `05` |
| `boundary-exposure` | Diagnostics payloads, trust anchors, validators, threat model | `06`, `07`, `10`, `11` |
| `claims-truth` | Published claims against enforced behaviour | `09` |

Four roles, two passes of two.

## Collectors

`advisory.state`, `branch.protection`, `gate.cannotFail`,
`mainline.divergence`.

A security audit reads source far more than it reads collected facts, so its
evidence needs are narrow. The collected facts it does need are the ones no
amount of source reading reveals: repository posture, and which of the gates it
is about to trust can actually fail.

## Exit questions

1. Which claimed invariants are enforced at every reachable path, and which at
   only some? Name the unguarded path.
2. Which format versions can still seal, and under which policy does each
   derive keys?
3. Which sensitive commands can be confirmed by text the command does not
   verify?
4. Is the manifest truthful in both directions?
5. Which findings are free now and a migration later? State the trigger.
6. Which trust boundaries does the threat model omit?
