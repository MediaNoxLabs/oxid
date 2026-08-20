#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

# Dioxus 0.7.10 -> dioxus-desktop 0.7.10 -> Wry 0.53.5 retains the
# target-specific GTK3 graph below. The exceptions are documented and bounded in
# docs/security/advisory-exceptions.md. Every advisory not named here is denied.
ignored_advisories=(
  RUSTSEC-2024-0411 # gdkwayland-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0412 # gdk: unmaintained GTK3 binding
  RUSTSEC-2024-0413 # atk: unmaintained GTK3 binding
  RUSTSEC-2024-0414 # gdkx11-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0415 # gtk: unmaintained GTK3 binding
  RUSTSEC-2024-0416 # atk-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0418 # gdk-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0419 # gtk3-macros: unmaintained GTK3 binding
  RUSTSEC-2024-0420 # gtk-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0429 # glib: VariantStrIter unsoundness in GTK3 graph
  RUSTSEC-2024-0370 # proc-macro-error: unmaintained GTK3 macro dependency
  RUSTSEC-2024-0436 # paste: unmaintained image-codec build dependency
  RUSTSEC-2025-0057 # fxhash: unmaintained Wry HTML parser dependency
  RUSTSEC-2026-0097 # rand 0.7: constrained build-dependency unsoundness
  RUSTSEC-2025-0141 # bincode: unmaintained Midnight ZK dependency
  RUSTSEC-2025-0161 # libsecp256k1: inactive optional Subxt light-client dependency
  RUSTSEC-2026-0002 # lru 0.12: inactive optional Subxt light-client dependency
  RUSTSEC-2026-0173 # proc-macro-error2: unmaintained Subxt build-time dependency
  RUSTSEC-2026-0253 # lru 0.12: inactive optional Subxt light-client dependency
)

# Yanked crate versions this repository knowingly stays on.
#
# A yank is a *withdrawal* notice, not a vulnerability. `Cargo.lock` pins the
# exact bytes by checksum, so a yanked-but-pinned dependency is byte-identical
# to the one that was already audited; a yank removes a version from *new*
# resolution and changes nothing already resolved.
#
# The distinction is load-bearing rather than pedantic. `cargo audit --deny`
# cannot express "permit this one yank" — yanks carry no advisory id, so
# `--ignore` cannot name them — so denying the whole class turned an upstream
# withdrawal into a repo-wide red gate, and made the obvious remedy,
# `cargo update`, the dangerous action. Policy therefore lives here, evaluated
# from the JSON report, where it can be as narrow as a single crate version.
#
# Entries are `name@version` and MUST carry a justification and an issue.
# Anything yanked and *not* listed here still fails the gate.
allowed_yanked=(
  # #113: arrayref 0.3.5-0.3.9 were all yanked on 2026-08-20, and the
  # replacement 0.3.10, published the same day, adds a normal dependency on
  # `proc-macro1` -- created that day by `dtolney` (display name "David
  # Tolnay", impersonating `dtolnay`) carrying build-dependencies `ureq`,
  # `rustls`, and `base64`, i.e. build-time network access. Remaining on the
  # checksum-pinned 0.3.9 is the safe action here, not the risky one.
  "arrayref@0.3.9"
)

audit_arguments=()
for advisory in "${ignored_advisories[@]}"; do
  audit_arguments+=(--ignore "$advisory")
done

report="$(mktemp)"
trap 'rm -f "$report"' EXIT

# Deliberately no `--deny`: without it `cargo audit` exits non-zero only for
# vulnerabilities, and every informational class is judged below from the
# report. That keeps the vulnerability verdict as cargo's own concern while the
# per-class policy stays here, at a granularity the flag cannot express.
audit_status=0
cargo audit --json "${audit_arguments[@]}" >"$report" || audit_status=$?

if [ ! -s "$report" ]; then
  echo "cargo audit produced no JSON report (exit ${audit_status}); re-running for diagnostics." >&2
  cargo audit "${audit_arguments[@]}" >&2 || true
  exit 1
fi

failures=0

if [ "$(jq -r '.vulnerabilities.found' "$report")" != "false" ]; then
  echo "Denied: cargo audit reported vulnerabilities." >&2
  jq -r '.vulnerabilities.list[]? | "  \(.advisory.id) \(.package.name) \(.package.version)"' "$report" >&2
  failures=$((failures + 1))
fi

# Every informational class other than `yanked` still fails closed, so an
# unmaintained or unsound finding absent from the ignore list above stops the
# gate exactly as it did before this change.
while IFS=$'\t' read -r kind name version; do
  [ -n "$kind" ] || continue
  echo "Denied: ${kind} finding for ${name} ${version}." >&2
  failures=$((failures + 1))
done < <(jq -r '
  .warnings // {}
  | to_entries[]
  | select(.key != "yanked")
  | .key as $kind
  | .value[]?
  | [$kind, .package.name, .package.version]
  | @tsv
' "$report")

while IFS=$'\t' read -r name version; do
  [ -n "$name" ] || continue
  entry="${name}@${version}"
  permitted=0
  for allowed in "${allowed_yanked[@]}"; do
    if [ "$allowed" = "$entry" ]; then
      permitted=1
      break
    fi
  done
  if [ "$permitted" -eq 1 ]; then
    echo "Note: ${entry} is yanked upstream and explicitly permitted; see scripts/check-advisories.sh."
  else
    echo "Denied: ${entry} is yanked and is not in the permitted list." >&2
    echo "  A yank is a withdrawal, not a vulnerability. Read the replacement's" >&2
    echo "  dependency diff before updating, then either pin deliberately and add" >&2
    echo "  an entry here with justification, or update. Do not reflexively run" >&2
    echo "  'cargo update' to clear this." >&2
    failures=$((failures + 1))
  fi
done < <(jq -r '.warnings.yanked // [] | .[] | [.package.name, .package.version] | @tsv' "$report")

if [ "$failures" -ne 0 ]; then
  echo "Advisory gate failed with ${failures} problem(s)." >&2
  exit 1
fi

echo "Advisory gate passed: no vulnerabilities, no unmaintained or unsound findings, every yanked crate permitted."
