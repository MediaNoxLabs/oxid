#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Keeps the decision record graph navigable in both directions.
#
# An ADR that amends or supersedes another states so in its own header. The
# amended record is the one a reader arrives at first, so it must say that a
# later decision changed it — otherwise the corpus silently presents a
# superseded rule as current. This check enforces that every forward
# declaration has a matching acknowledgment, that the index lists every
# record exactly once with the status its own file declares, and that a
# proposal under review carries no number until it is accepted.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
adr_dir="$repo_root/docs/adr"
index="$adr_dir/README.md"
failures=0

fail() {
  echo "$1" >&2
  failures=$((failures + 1))
}

# Forward declarations: "- Amends: ADR-0024, ADR-0031 and ADR-0034".
# Acknowledgments: "- Amended by: ADR-0032 …" / "- Superseded in part by: …".
declare -a forward_edges=()
for file in "$adr_dir"/[0-9][0-9][0-9][0-9]-*.md; do
  number="$(basename "$file" | cut -c1-4)"
  while IFS= read -r target; do
    [ -n "$target" ] || continue
    forward_edges+=("$number:$target")
  done < <(
    sed -nE 's/^- (Amends|Supersedes)[^:]*:[[:space:]]*(.*)$/\2/p' "$file" |
      grep -oE 'ADR-[0-9]{4}' | sed 's/ADR-//' | sort -u
  )
done

for edge in "${forward_edges[@]:-}"; do
  [ -n "$edge" ] || continue
  source_number="${edge%%:*}"
  target_number="${edge##*:}"
  target_file="$(ls "$adr_dir/$target_number"-*.md 2>/dev/null | head -1 || true)"
  if [ -z "$target_file" ]; then
    fail "ADR-$source_number declares an amendment of ADR-$target_number, which does not exist."
    continue
  fi
  if ! grep -qE "^- (Amended|Superseded)[^:]*:.*ADR-$source_number" "$target_file"; then
    fail "ADR-$target_number is amended by ADR-$source_number but does not acknowledge it. Add an '- Amended by: ADR-$source_number …' or '- Superseded in part by: ADR-$source_number …' header to $(basename "$target_file")."
  fi
done

# Reverse declarations must name a record that actually claims the amendment,
# so a stale acknowledgment cannot outlive the change that caused it.
for file in "$adr_dir"/[0-9][0-9][0-9][0-9]-*.md; do
  number="$(basename "$file" | cut -c1-4)"
  while IFS= read -r claimed; do
    [ -n "$claimed" ] || continue
    claimed_file="$(ls "$adr_dir/$claimed"-*.md 2>/dev/null | head -1 || true)"
    if [ -z "$claimed_file" ]; then
      fail "ADR-$number is acknowledged as amended by ADR-$claimed, which does not exist."
      continue
    fi
    if ! grep -qE "^- (Amends|Supersedes)[^:]*:.*ADR-$number" "$claimed_file"; then
      fail "ADR-$number says ADR-$claimed amends it, but $(basename "$claimed_file") declares no matching 'Amends'/'Supersedes' header."
    fi
  done < <(
    sed -nE 's/^- (Amended|Superseded)[^:]*:[[:space:]]*(.*)$/\2/p' "$file" |
      grep -oE 'ADR-[0-9]{4}' | sed 's/ADR-//' | sort -u
  )
done

# Index coverage: one row per record, and the row's status matches the file.
for file in "$adr_dir"/[0-9][0-9][0-9][0-9]-*.md; do
  name="$(basename "$file")"
  number="$(echo "$name" | cut -c1-4)"
  rows="$(grep -cE "^\| \[$number\]\($name\)" "$index" || true)"
  if [ "$rows" != "1" ]; then
    fail "The ADR index has $rows rows for $name; expected exactly one."
    continue
  fi
  file_status="$(sed -nE 's/^- Status:[[:space:]]*([A-Za-z]+).*/\1/p' "$file" | head -1)"
  row="$(grep -E "^\| \[$number\]\($name\)" "$index")"
  if [ -n "$file_status" ] && ! printf '%s' "$row" | grep -q "| $file_status |"; then
    fail "ADR-$number declares Status: $file_status but its index row does not carry that status."
  fi
done

# A proposal under review carries no number until it is accepted, so a
# long-lived branch cannot collide with the sequence.
for draft in "$adr_dir"/draft-*.md; do
  [ -e "$draft" ] || continue
  if ! grep -q '^# ADR-DRAFT' "$draft"; then
    fail "$(basename "$draft") is an unnumbered proposal and must be titled '# ADR-DRAFT: …'."
  fi
  if grep -qE "^\| \[[0-9]{4}\]\($(basename "$draft")\)" "$index"; then
    fail "$(basename "$draft") is still a proposal but already has an index row."
  fi
done

if [ "$failures" -ne 0 ]; then
  echo "Decision record link check failed with $failures problem(s)." >&2
  exit 1
fi

echo "Decision record links, index coverage, and statuses are consistent."
