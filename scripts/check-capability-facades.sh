#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C

fail() {
  echo "Capability façade check failed: $*" >&2
  exit 1
}

for override in CAPABILITY_FACADES_INVENTORY CAPABILITY_FACADES_TODAY CAPABILITY_FACADES_TEST_MODE; do
  if [ "${!override+x}" = x ]; then
    fail "production checker does not accept $override."
  fi
done
[ "$#" -eq 0 ] || fail "production checker does not accept arguments."

repository_root="$(git rev-parse --show-toplevel)"
baseline_path="scripts/architecture/capability-facades.json"
today="$(date -u +%F)"
temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT
baseline="$temporary_directory/capability-facades.json"
baseline_entries="$temporary_directory/baseline-index-entry"

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

path_has_glob() {
  case "$1" in
    *'*'*|*'?'*|*'['*) return 0 ;;
    *) return 1 ;;
  esac
}

encode_path() {
  printf '%s' "$1" | od -An -v -tx1 | tr -d ' \n'
}

decode_path() {
  local encoded="$1"
  local escaped=""
  while [ -n "$encoded" ]; do
    escaped="$escaped\\x${encoded:0:2}"
    encoded="${encoded:2}"
  done
  printf '%b' "$escaped"
}

display_path() {
  printf '%q' "$1"
}

inventory_has() {
  local encoded
  encoded="$(encode_path "$1")"
  awk -F '\t' -v path="$encoded" '$1 == path { found = 1 } END { exit !found }' "$inventory"
}

inventory_lines() {
  local encoded
  encoded="$(encode_path "$1")"
  awk -F '\t' -v path="$encoded" '$1 == path { print $2 }' "$inventory"
}

command -v jq >/dev/null 2>&1 || fail "jq is required; run this check from 'nix develop'."
is_iso_date "$today" || fail "UTC date '$today' is not an ISO-8601 calendar date."
if ! git -C "$repository_root" ls-files -s -z -- "$baseline_path" >"$baseline_entries"; then
  fail "could not enumerate the indexed capability façade baseline."
fi
baseline_entry_count=0
while IFS= read -r -d '' entry; do
  baseline_entry_count=$((baseline_entry_count + 1))
  case "$entry" in
    *$'\t'*) ;;
    *) fail "Git returned malformed baseline index metadata." ;;
  esac
  metadata="${entry%%$'\t'*}"
  indexed_path="${entry#*$'\t'}"
  mode=""
  baseline_object_id=""
  stage=""
  extra=""
  read -r mode baseline_object_id stage extra <<<"$metadata"
  [ "$indexed_path" = "$baseline_path" ] && [ "$stage" = 0 ] && [ -z "$extra" ] ||
    fail "capability façade baseline has an unresolved or malformed index entry."
  case "$mode" in
    100644|100755) ;;
    *) fail "capability façade baseline is not a regular indexed blob." ;;
  esac
done <"$baseline_entries"
[ "$baseline_entry_count" -eq 1 ] || fail "capability façade baseline is missing from the index."
if ! git -C "$repository_root" cat-file blob "$baseline_object_id" >"$baseline"; then
  fail "could not read the indexed capability façade baseline blob."
fi

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

source_roots=()
while IFS= read -r source_root; do
  case "$source_root" in
    /*|*..*|*/|*.rs) fail "invalid sourceRoot '$source_root'." ;;
  esac
  path_has_glob "$source_root" && fail "sourceRoot '$source_root' must not contain a glob."
  source_roots+=("$source_root")
done < <(jq -r '.crates[].sourceRoot' "$baseline")

inventory="$temporary_directory/inventory"
unsorted_inventory="$temporary_directory/unsorted-inventory"
index_entries="$temporary_directory/index-entries"
if ! git -C "$repository_root" ls-files -s -z -- "${source_roots[@]}" >"$index_entries"; then
  fail "could not enumerate governed index entries."
fi

while IFS= read -r -d '' entry; do
  case "$entry" in
    *$'\t'*) ;;
    *) fail "Git returned malformed index metadata." ;;
  esac
  metadata="${entry%%$'\t'*}"
  path="${entry#*$'\t'}"
  case "$path" in
    *.rs) ;;
    *) continue ;;
  esac

  mode=""
  object_id=""
  stage=""
  extra=""
  read -r mode object_id stage extra <<<"$metadata"
  [ -n "$mode" ] && [ -n "$object_id" ] && [ "$stage" = 0 ] && [ -z "$extra" ] ||
    fail "governed Rust path '$(display_path "$path")' has an unresolved or malformed index entry."
  case "$mode" in
    100644|100755) ;;
    *) fail "governed Rust path '$(display_path "$path")' is not a regular indexed blob." ;;
  esac
  case "$object_id" in
    *[!0-9a-f]*|'') fail "governed Rust path '$(display_path "$path")' has an invalid indexed object ID." ;;
  esac

  if ! lines="$(git -C "$repository_root" cat-file blob "$object_id" | wc -l | tr -d ' ')"; then
    fail "could not read indexed blob for governed Rust path '$(display_path "$path")'."
  fi
  printf '%s\t%s\n' "$(encode_path "$path")" "$lines" >>"$unsorted_inventory"
done <"$index_entries"

sort -t $'\t' -k1,1 "$unsorted_inventory" >"$inventory"
awk -F '\t' '
  NF != 2 || $1 == "" || $2 !~ /^[0-9]+$/ { exit 1 }
  seen[$1]++ { if (seen[$1] > 1) exit 1 }
' "$inventory" || fail "indexed inventory must contain unique path and physical-line records."

while IFS= read -r crate; do
  name="$(jq -r '.name' <<<"$crate")"
  source_root="$(jq -r '.sourceRoot' <<<"$crate")"
  maximum="$(jq -r '.facadeMaximumPhysicalLines' <<<"$crate")"

  duplicate_facade="$(jq -r '.facadeFiles[]' <<<"$crate" | sort | uniq -d)"
  [ -z "$duplicate_facade" ] || fail "$name repeats façade path '$duplicate_facade'."

  while IFS= read -r facade; do
    [ -n "$facade" ] || fail "$name has an empty façade path."
    path_has_glob "$facade" && fail "$name façade path '$facade' must be exact."
    case "$facade" in
      "$source_root"/*.rs) ;;
      *) fail "$name façade '$facade' is not a Rust file below '$source_root'." ;;
    esac
    inventory_has "$facade" || fail "$name façade '$facade' is missing from the indexed inventory."
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
    while IFS=$'\t' read -r encoded_path lines; do
      inventory_path="$(decode_path "$encoded_path")"
      case "$inventory_path" in
        "$prefix.rs"|"$prefix"/*) owned_paths=$((owned_paths + 1)) ;;
      esac
    done <"$inventory"
    [ "$owned_paths" -gt 0 ] || fail "$name owner '$owner' prefix '$prefix' owns no indexed Rust source."
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
    inventory_has "$exclusion" || fail "$name exclusion '$exclusion' is missing from the indexed inventory."
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
      lines="$(inventory_lines "$exception_path")"
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
    lines="$(inventory_lines "$facade")"
    facade_total=$((facade_total + lines))
    if ! jq -e --arg path "$facade" '[.temporaryExceptions[].paths[]] | index($path) != null' <<<"$crate" >/dev/null; then
      path_maximum="$(jq -r --arg path "$facade" '.facadeMaximumPhysicalLinesByPath[$path]' <<<"$crate")"
      [ "$lines" -le "$path_maximum" ] || fail "$name façade '$facade' has $lines lines; path maximum is $path_maximum."
    fi
  done < <(jq -r '.facadeFiles[]' <<<"$crate")
  allowed_total=$((maximum + exception_ceiling))
  [ "$facade_total" -le "$allowed_total" ] || fail "$name façade total $facade_total exceeds its allowed maximum $allowed_total."

  while IFS=$'\t' read -r encoded_path lines; do
    path="$(decode_path "$encoded_path")"
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
    [ "$owners" -eq 1 ] || fail "$name source '$(display_path "$path")' belongs to $owners capability owners; expected exactly one."
  done <"$inventory"
  rm -f "$prefixes"
done < <(jq -c '.crates[]' "$baseline")

echo "Capability façade ownership and line ratchets passed."
