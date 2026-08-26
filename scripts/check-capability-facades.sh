#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C

repository_root="$(git rev-parse --show-toplevel)"
baseline="${1:-$repository_root/scripts/architecture/capability-facades.json}"
inventory="${CAPABILITY_FACADES_INVENTORY:-}"
today="${CAPABILITY_FACADES_TODAY:-$(date -u +%F)}"

fail() {
  echo "Capability façade check failed: $*" >&2
  exit 1
}

is_iso_date() {
  local value="$1"
  local parsed
  if parsed="$(date -j -f '%Y-%m-%d' "$value" '+%Y-%m-%d' 2>/dev/null)"; then
    [ "$parsed" = "$value" ]
    return
  fi
  if parsed="$(date -d "$value" '+%Y-%m-%d' 2>/dev/null)"; then
    [ "$parsed" = "$value" ]
    return
  fi
  return 1
}

command -v jq >/dev/null 2>&1 || fail "jq is required; run this check from 'nix develop'."
is_iso_date "$today" || fail "comparison date '$today' is not an ISO-8601 calendar date."
[ -f "$baseline" ] || fail "baseline '$baseline' does not exist."

jq -e '
  (keys == ["crates", "schemaVersion"]) and
  (.schemaVersion == 1) and
  (.crates | type == "array" and length > 0) and
  ([.crates[].name] | length == (unique | length)) and
  ([.crates[].sourceRoot] | length == (unique | length)) and
  (all(.crates[];
    keys == ["capabilityOwners", "exclusions", "facadeFiles", "facadeMaximumPhysicalLines", "facadeMaximumPhysicalLinesByPath", "name", "sourceRoot", "temporaryExceptions"] and
    (.name | type == "string" and length > 0) and
    (.sourceRoot | type == "string" and length > 0) and
    (.facadeFiles | type == "array" and length > 0) and
    (.facadeMaximumPhysicalLines | type == "number" and floor == . and . >= 0) and
    (.facadeMaximumPhysicalLinesByPath as $ceilings |
      ($ceilings | type == "object") and
      (($ceilings | keys | sort) == (.facadeFiles | sort)) and
      (all($ceilings[]; type == "number" and floor == . and . >= 0)) and
      (([$ceilings[]] | add) == .facadeMaximumPhysicalLines)
    ) and
    (.capabilityOwners | type == "array") and
    (.exclusions | type == "array") and
    (.temporaryExceptions | type == "array")
  ))
' "$baseline" >/dev/null || fail "baseline schema is invalid."

if [ -z "$inventory" ]; then
  inventory="$(mktemp)"
  trap 'rm -f "$inventory"' EXIT
  while IFS= read -r source_root; do
    while IFS= read -r path; do
      case "$path" in
        *.rs)
          tracked_mode="$(git -C "$repository_root" ls-files --stage -- "$path" | awk 'NR == 1 { mode = $1 } END { if (NR == 1) print mode }')"
          case "$tracked_mode" in
            100644|100755) ;;
            *) fail "governed Rust path '$path' is not a regular tracked file." ;;
          esac
          [ ! -L "$repository_root/$path" ] || fail "governed Rust path '$path' is a working-tree symlink."
          [ -f "$repository_root/$path" ] || fail "governed Rust path '$path' is not a regular working-tree file."
          printf '%s\t%s\n' "$path" "$(wc -l <"$repository_root/$path" | tr -d ' ')"
          ;;
      esac
    done < <(git -C "$repository_root" ls-files -- "$source_root" | sort)
  done < <(jq -r '.crates[].sourceRoot' "$baseline") >"$inventory"
else
  [ -f "$inventory" ] || fail "inventory '$inventory' does not exist."
fi

awk -F '\t' '
  NF != 2 || $1 == "" || $2 !~ /^[0-9]+$/ { exit 1 }
  seen[$1]++ { if (seen[$1] > 1) exit 1 }
' "$inventory" || fail "inventory must contain unique PATH<TAB>PHYSICAL_LINES records."

path_has_glob() {
  case "$1" in
    *'*'*|*'?'*|*'['*) return 0 ;;
    *) return 1 ;;
  esac
}

while IFS= read -r crate; do
  name="$(jq -r '.name' <<<"$crate")"
  source_root="$(jq -r '.sourceRoot' <<<"$crate")"
  maximum="$(jq -r '.facadeMaximumPhysicalLines' <<<"$crate")"

  case "$source_root" in
    /*|*..*|*/|*.rs) fail "$name has an invalid sourceRoot '$source_root'." ;;
  esac
  path_has_glob "$source_root" && fail "$name sourceRoot must not contain a glob."

  duplicate_facade="$(jq -r '.facadeFiles[]' <<<"$crate" | sort | uniq -d)"
  [ -z "$duplicate_facade" ] || fail "$name repeats façade path '$duplicate_facade'."

  while IFS= read -r facade; do
    [ -n "$facade" ] || fail "$name has an empty façade path."
    path_has_glob "$facade" && fail "$name façade path '$facade' must be exact."
    case "$facade" in
      "$source_root"/*.rs) ;;
      *) fail "$name façade '$facade' is not a Rust file below '$source_root'." ;;
    esac
    grep -Fqx "$facade" < <(cut -f1 "$inventory") || fail "$name façade '$facade' is missing from the committed inventory."
  done < <(jq -r '.facadeFiles[]' <<<"$crate")

  jq -e 'all(.capabilityOwners[];
    keys == ["modulePathPrefixes", "name"] and
    (.name | type == "string" and length > 0) and
    (.modulePathPrefixes | type == "array" and length > 0) and
    all(.modulePathPrefixes[]; type == "string" and length > 0)
  )' <<<"$crate" >/dev/null || fail "$name capability-owner schema is invalid."
  duplicate_owner="$(jq -r '.capabilityOwners[].name' <<<"$crate" | sort | uniq -d)"
  [ -z "$duplicate_owner" ] || fail "$name repeats capability owner '$duplicate_owner'."

  prefixes="$(mktemp)"
  jq -r '.capabilityOwners[] | .name as $owner | .modulePathPrefixes[] | [$owner, .] | @tsv' <<<"$crate" >"$prefixes"
  while IFS=$'\t' read -r owner prefix; do
    path_has_glob "$prefix" && fail "$name owner '$owner' prefix '$prefix' must not contain a glob."
    case "$prefix" in
      "$source_root"/*) ;;
      *) fail "$name owner '$owner' prefix '$prefix' is outside '$source_root'." ;;
    esac
    case "$prefix" in
      *.rs|*/) fail "$name owner '$owner' prefix '$prefix' must be a Rust module path without an extension or trailing slash." ;;
    esac
    owned_paths=0
    while IFS= read -r inventory_path; do
      case "$inventory_path" in
        "$prefix.rs"|"$prefix"/*) owned_paths=$((owned_paths + 1)) ;;
      esac
    done < <(cut -f1 "$inventory")
    [ "$owned_paths" -gt 0 ] || fail "$name owner '$owner' prefix '$prefix' owns no committed Rust source."
  done <"$prefixes"
  while IFS=$'\t' read -r owner prefix; do
    while IFS=$'\t' read -r other_owner other_prefix; do
      [ "$owner:$prefix" = "$other_owner:$other_prefix" ] && continue
      case "$prefix" in
        "$other_prefix"|"$other_prefix"/*) fail "$name ownership overlaps at '$prefix' and '$other_prefix'." ;;
      esac
    done <"$prefixes"
  done <"$prefixes"

  jq -e 'all(.exclusions[];
    keys == ["classification", "path"] and
    (.classification == "fixture" or .classification == "generated") and
    (.path | type == "string" and length > 0)
  )' <<<"$crate" >/dev/null || fail "$name exclusions must be exact paths classified as generated or fixture."
  duplicate_exclusion="$(jq -r '.exclusions[].path' <<<"$crate" | sort | uniq -d)"
  [ -z "$duplicate_exclusion" ] || fail "$name repeats exclusion '$duplicate_exclusion'."
  while IFS= read -r exclusion; do
    [ -n "$exclusion" ] || continue
    path_has_glob "$exclusion" && fail "$name exclusion '$exclusion' must be exact."
    case "$exclusion" in
      "$source_root"/*.rs) ;;
      *) fail "$name exclusion '$exclusion' is not a Rust file below '$source_root'." ;;
    esac
    grep -Fqx "$exclusion" < <(cut -f1 "$inventory") || fail "$name exclusion '$exclusion' is missing from the committed inventory."
  done < <(jq -r '.exclusions[].path' <<<"$crate")

  jq -e 'all(.temporaryExceptions[];
    keys == ["expiresOn", "extraLineCeiling", "issue", "paths", "reason"] and
    (.paths | type == "array" and length > 0) and
    all(.paths[]; type == "string" and length > 0) and
    (.extraLineCeiling | type == "number" and floor == . and . > 0) and
    (.issue | type == "string" and length > 0) and
    (.reason | type == "string" and length > 0) and
    (.expiresOn | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"))
  )' <<<"$crate" >/dev/null || fail "$name temporary-exception schema is invalid."

  duplicate_exception_path="$(jq -r '.temporaryExceptions[].paths[]' <<<"$crate" | sort | uniq -d)"
  [ -z "$duplicate_exception_path" ] || fail "$name repeats temporary exception path '$duplicate_exception_path'."
  exception_ceiling=0
  while IFS= read -r exception; do
    expires_on="$(jq -r '.expiresOn' <<<"$exception")"
    is_iso_date "$expires_on" || fail "$name exception expiry '$expires_on' is not an ISO-8601 calendar date."
    [ "$expires_on" \> "$today" ] || fail "$name has an expired temporary exception ending '$expires_on'."
    exception_excess=0
    while IFS= read -r exception_path; do
      jq -e --arg path "$exception_path" '.facadeFiles | index($path) != null' <<<"$crate" >/dev/null || fail "$name exception path '$exception_path' is not an exact façade path."
      lines="$(awk -F '\t' -v path="$exception_path" '$1 == path { print $2 }' "$inventory")"
      path_maximum="$(jq -r --arg path "$exception_path" '.facadeMaximumPhysicalLinesByPath[$path]' <<<"$crate")"
      if [ "$lines" -gt "$path_maximum" ]; then
        exception_excess=$((exception_excess + lines - path_maximum))
      fi
    done < <(jq -r '.paths[]' <<<"$exception")
    ceiling="$(jq -r '.extraLineCeiling' <<<"$exception")"
    [ "$exception_excess" -le "$ceiling" ] || fail "$name temporary exception needs $exception_excess extra lines across its exact paths; ceiling is $ceiling."
    exception_ceiling=$((exception_ceiling + ceiling))
  done < <(jq -c '.temporaryExceptions[]' <<<"$crate")

  facade_total=0
  while IFS= read -r facade; do
    lines="$(awk -F '\t' -v path="$facade" '$1 == path { print $2 }' "$inventory")"
    facade_total=$((facade_total + lines))
    if ! jq -e --arg path "$facade" '[.temporaryExceptions[].paths[]] | index($path) != null' <<<"$crate" >/dev/null; then
      path_maximum="$(jq -r --arg path "$facade" '.facadeMaximumPhysicalLinesByPath[$path]' <<<"$crate")"
      [ "$lines" -le "$path_maximum" ] || fail "$name façade '$facade' has $lines lines; path maximum is $path_maximum."
    fi
  done < <(jq -r '.facadeFiles[]' <<<"$crate")
  allowed_total=$((maximum + exception_ceiling))
  [ "$facade_total" -le "$allowed_total" ] || fail "$name façade total $facade_total exceeds its allowed maximum $allowed_total."

  while IFS=$'\t' read -r path lines; do
    case "$path" in
      "$source_root"/*.rs) ;;
      *) continue ;;
    esac
    if jq -e --arg path "$path" '.facadeFiles | index($path) != null' <<<"$crate" >/dev/null; then
      continue
    fi
    if jq -e --arg path "$path" '.exclusions | map(.path) | index($path) != null' <<<"$crate" >/dev/null; then
      continue
    fi
    owners=0
    while IFS=$'\t' read -r owner prefix; do
      case "$path" in
        "$prefix.rs"|"$prefix"/*) owners=$((owners + 1)) ;;
      esac
    done <"$prefixes"
    [ "$owners" -eq 1 ] || fail "$name source '$path' belongs to $owners capability owners; expected exactly one."
  done <"$inventory"
  rm -f "$prefixes"
done < <(jq -c '.crates[]' "$baseline")

echo "Capability façade ownership and line ratchets passed."
