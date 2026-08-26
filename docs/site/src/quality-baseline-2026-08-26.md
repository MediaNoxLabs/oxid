<!-- SPDX-License-Identifier: Apache-2.0 -->

# Quality baseline — 2026-08-26

This is the initial reproducible quality baseline for
[`Oxid quality constitution v1.0`](quality-constitution.md). It is bound to
`origin/integration` commit `21ec1234ff26390464535043a58fc183cda83fd5`.
Measurements are inventory, not permission to preserve or expand debt.

This phase deliberately did not run timed Cargo builds, coverage, Compact
artifact builds, mobile targets, emulators, simulators, physical devices, or
private-credential flows. Static repository measurements and current hosted CI
metadata were used so the performance lane was not contaminated.

Run repository commands inside `nix develop` and from the repository root.

## Source size

Production source here means tracked Rust files below `apps/*/src` and
`crates/*/src`. Physical lines include comments and blank lines; generated
outputs outside those paths are not counted.

```bash
files="$(git ls-files 'apps/**/src/*.rs' 'crates/**/src/*.rs')"
printf '%s\n' "$files" | xargs wc -l | awk '
  $2 != "total" {
    n=$1; total+=n
    if (n < 400) a++
    else if (n <= 600) b++
    else if (n <= 1000) c++
    else if (n <= 2000) d++
    else e++
  }
  END {
    printf "files=%d total=%d <400=%d 400-600=%d 601-1000=%d 1001-2000=%d >2000=%d\n", \
      a+b+c+d+e,total,a,b,c,d,e
  }'
```

Output:

```text
files=104 total=115748 <400=28 400-600=14 601-1000=25 1001-2000=27 >2000=10
```

There are 62 files above 600 lines and 37 above 1,000. The ten above 2,000 are:

```text
13489 crates/ui-dioxus/src/lib.rs
 9020 apps/oxid-headless/src/lib.rs
 4514 crates/composition/src/lib.rs
 4190 crates/adapters/midnight/src/transaction.rs
 2667 crates/adapters/passport-vault/src/native_call.rs
 2558 crates/adapters/midnight/src/submission.rs
 2402 crates/adapters/midnight/src/indexer.rs
 2340 crates/adapters/midnight/src/lib.rs
 2241 crates/adapters/vc-midnight/src/compact_presentation.rs
 2145 crates/composition/src/standalone_funding_tests.rs
```

The last entry is test-only code colocated under `src`; future automation must
classify such modules before enforcing a production threshold. The headless
incoming adapter is the largest current headless source file and is a priority
for a separate cohesion-preserving decomposition issue.

Reproduce the ranked list with:

```bash
git ls-files -z 'apps/**/src/*.rs' 'crates/**/src/*.rs' |
  xargs -0 wc -l | sort -nr
```

## Crates and dependencies

The workspace contains 45 first-party packages. Source-line totals use each
package's `src` directory. The five largest are:

```text
21677 oxid-adapter-midnight
14437 oxid-ui-dioxus
 9083 oxid-adapter-passport-vault
 9043 oxid-headless
 8548 oxid-adapter-vc-midnight
```

Only `oxid-adapter-midnight` is above the north star's 20k crate-review signal;
no crate reaches 30k. This does not authorize splitting it into micro-crates.
A decomposition review must begin with capability cohesion and dependency
direction.

Reproduce the inventory with:

```bash
cargo metadata --no-deps --format-version 1 |
  jq -r '.packages[] | [.name, .manifest_path] | @tsv' |
  while IFS=$'\t' read -r name manifest; do
    dir=${manifest%/Cargo.toml}
    if [ -d "$dir/src" ]; then
      loc=$(find "$dir/src" -type f -name '*.rs' -print0 |
        xargs -0 cat | wc -l | tr -d ' ')
    else
      loc=0
    fi
    printf '%7d %s\n' "$loc" "$name"
  done | sort -nr
```

Workspace dependency direction is already machine-enforced by
`scripts/check-architecture.sh`. It uses `cargo metadata --no-deps`, a
per-package allowlist, a default-deny package sweep, and zero external
dependencies for 16 core/application/platform packages. The script remains the
authority; raw edge counts are descriptive only.

## Functions and complexity

No syntax-aware function-length or cyclomatic-complexity tool is pinned in the
current devshell, and no corresponding Clippy threshold is configured. A regex
count would misclassify macros, closures, generated code, test modules, and
multiline signatures, so this baseline records the measurement gap rather than
publishing a misleading number.

Current enforcement status:

```text
function-length numeric baseline: unavailable
cyclomatic-complexity numeric baseline: unavailable
failing function/complexity threshold: none
```

This is a gate on tightening: no function-length or complexity threshold may be
made required until a problem-focused follow-up pins a Rust-aware tool, records
its version and exclusions, publishes the complete baseline, and reviews false
positives. Reviewers still assess cohesion and complexity in touched code.

Confirm the current configuration with:

```bash
git grep -nE 'cognitive_complexity|too_many_lines' -- \
  Cargo.toml 'apps/**/*.rs' 'crates/**/*.rs' || true
```

## Tests and coverage

Static test inventory:

```text
Rust source files containing #[cfg(test)]: 92
Rust files under a tests/ directory:        3
Rust files referring to proptest:           1
Ignored Rust tests:                          8
Cargo fuzz manifests:                        0
```

Reproduce it with:

```bash
printf 'inline test modules: '
git grep -l '#\[cfg(test)\]' -- 'apps/**/*.rs' 'crates/**/*.rs' | wc -l
printf 'Rust files under tests/: '
find apps crates tests -type f -path '*/tests/*.rs' | wc -l
printf 'proptest references: '
git grep -l 'proptest' -- 'apps/**/*.rs' 'crates/**/*.rs' 'tests/**/*.rs' | wc -l
printf 'ignored tests: '
git grep -n '#\[ignore' -- '*.rs' | wc -l
printf 'fuzz manifests: '
find . \( -path '*/fuzz/Cargo.toml' -o -name 'cargo-fuzz.toml' \) | wc -l
```

`run.sh` enforces at least 80% aggregate line coverage across the workspace
except `oxid-ui-dioxus`, `oxid-app`, and `oxid-headless`; those three test
suites run separately. The latest recorded strict-gate evidence in `AGENT.md`
is 78.68% region, 80.22% function, and 80.36% line coverage. It predates this
snapshot and is historical evidence, not a freshly timed measurement for this
commit. Per-core, security-critical, changed-line, and platform-glue coverage
are not yet separately measured.

Reproduce the authoritative aggregate measurement in a dedicated performance
lane, not as part of this docs-only baseline:

```bash
./run.sh coverage --strict
```

## Unsafe Rust, warnings, and documentation

The workspace sets `unsafe_code = "deny"`. The architecture checker permits
unsafe tokens only in `crates/adapters/storage-json/src/lib.rs`, the reviewed
Android profile-path JNI boundary. That file contains one `#[allow(unsafe_code)]`
and two unsafe blocks. No other Rust source file may contain unsafe code.

```bash
git grep -n '\bunsafe\b' -- 'apps/**/*.rs' 'crates/**/*.rs'
./scripts/check-architecture.sh
```

The strict gate runs workspace Clippy with `-D warnings`, and strict quality
validation runs rustdoc with `RUSTDOCFLAGS="-D warnings"`. The current base's
hosted CI and quality workflows passed. No crate currently enables
`warn(missing_docs)` or `deny(missing_docs)`, so public-API documentation
coverage has no numeric baseline or failing threshold yet.

```bash
git grep -nE 'warn\(missing_docs\)|deny\(missing_docs\)' -- '*.rs' || true
sed -n '/\[workspace.lints.rust\]/,/^$/p' Cargo.toml
sed -n '/\[workspace.lints.clippy\]/,/^$/p' Cargo.toml
```

## CI duration and tier baseline

Read-only GitHub metadata was captured for the base commit. Durations below are
job wall times, not local benchmark results:

| Workflow/job | Run | Result | Wall time |
| --- | ---: | --- | ---: |
| CI / Repository gate | 32967440061 | passed | 42m43s |
| CI / Locked Nix package and Compact artifacts | 32967440061 | passed | 19m28s |
| Quality / Audit, Licenses, Sources, and Documentation | 32967440027 | passed | 8m38s |
| Documentation links | 32967440074 | passed | 9s |
| Nightly / Hermetic Nix flake check | 32927158478 | failed | 56m53s |

The required CI workflow wall time was 42m49s. The repository-gate step itself
used 39m28s. This exceeds the north star's 10-minute required-PR target and
15-minute investigation threshold. It is baseline debt, not grounds to reduce
coverage or skip work. The latest nightly failure is residual evidence that
must be diagnosed separately; no green scheduled claim is made here.

Current configured ceilings are 45 minutes for repository and quality jobs, 60
minutes for the locked Nix build, and 120 minutes for nightly. The AGENT guide
also records a prior cold strict plus locked Nix/artifact run of roughly 59
minutes and warns that a 60-minute limit cancelled an otherwise progressing
check during action-download throttling.

Reproduce current workflow history and inspect jobs with read-only commands:

```bash
gh run list --repo MediaNoxLabs/oxid --workflow CI --branch integration \
  --limit 10 --json databaseId,headSha,status,conclusion,startedAt,updatedAt,url
gh run view 32967440061 --repo MediaNoxLabs/oxid --json jobs
gh run list --repo MediaNoxLabs/oxid --workflow Quality --branch integration \
  --limit 10 --json databaseId,headSha,status,conclusion,startedAt,updatedAt,url
gh run list --repo MediaNoxLabs/oxid --workflow Nightly --branch integration \
  --limit 5 --json databaseId,headSha,status,conclusion,startedAt,updatedAt,url
```

Run IDs are snapshot evidence and will be replaced in a future dated baseline;
commands and source-head binding are the reproducibility contract.

## Deferred evidence

The following are intentionally not claimed by this baseline:

- Android or iOS compilation and packaging;
- simulator, emulator, or physical-device journeys;
- mobile startup, memory, latency, thermal, camera, link, custody, or recovery
  measurements;
- private credentials, funded/private infrastructure, or owner-private CI;
- a fresh coverage run or any timed Cargo/Compact build;
- function length, cyclomatic complexity, per-scope coverage, or missing-doc
  percentages without pinned measurement tools.

Existing gates for deferred areas remain unchanged. Each gap requires a
separate issue with a bounded evidence contract before enforcement is added.
