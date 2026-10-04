#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
checker="$repository_root/scripts/check-capability-facades.sh"
architecture_checker="$repository_root/scripts/check-architecture.sh"
fixtures="$repository_root/scripts/architecture/fixtures/capability-facades"
fixture_root="$(mktemp -d)"
trap 'rm -rf "$fixture_root"' EXIT

deadline=()
if command -v timeout >/dev/null 2>&1; then
  deadline=(timeout 45)
elif command -v gtimeout >/dev/null 2>&1; then
  deadline=(gtimeout 45)
fi

init_fixture() {
  local repo="$1"
  local baseline="${2:-$fixtures/valid.json}"
  git init -q "$repo"
  mkdir -p "$repo/fixture/src/capability" "$repo/scripts/architecture"
  cp "$baseline" "$repo/scripts/architecture/capability-facades.json"
  printf 'one\ntwo\n' >"$repo/fixture/src/lib.rs"
  printf 'one\ntwo\n' >"$repo/fixture/src/shell.rs"
  printf 'capability\n' >"$repo/fixture/src/capability.rs"
  printf 'nested\n' >"$repo/fixture/src/capability/nested.rs"
  printf 'generated\n' >"$repo/fixture/src/generated.rs"
  printf 'protocol fixture\n' >"$repo/fixture/src/protocol_fixture.rs"
  git -C "$repo" add fixture/src scripts/architecture/capability-facades.json
}

run_checker() {
  local repo="$1"
  (
    cd "$repo"
    env -u CAPABILITY_FACADES_INVENTORY \
      -u CAPABILITY_FACADES_TODAY \
      -u CAPABILITY_FACADES_TEST_MODE \
      -u GIT_NO_REPLACE_OBJECTS \
      "${deadline[@]}" "$checker"
  )
}

if rg -n 'jq .*<<<|done[[:space:]]*< <\(jq' "$checker" >/dev/null; then
  echo "Capability façade checker must materialize jq inputs; here-strings and nested process substitutions can deadlock Bash 5.3." >&2
  exit 1
fi

expect_failure() {
  local repo="$1"
  local expected="$2"
  local output

  if output="$(run_checker "$repo" 2>&1)"; then
    echo "Expected capability façade fixture to fail: $repo" >&2
    exit 1
  fi
  if [[ "$output" != *"$expected"* ]]; then
    echo "Capability façade fixture failed for an unexpected reason: $output" >&2
    exit 1
  fi
}

valid_repo="$fixture_root/valid"
init_fixture "$valid_repo"
run_checker "$valid_repo" >/dev/null

invalid_schema_repo="$fixture_root/invalid-schema"
init_fixture "$invalid_schema_repo" "$fixtures/invalid-schema.json"
expect_failure "$invalid_schema_repo" "baseline schema is invalid"

invalid_facade_repo="$fixture_root/invalid-facade"
init_fixture "$invalid_facade_repo" "$fixtures/invalid-facade-path.json"
expect_failure "$invalid_facade_repo" "façade path 'fixture/src/*.rs' must be exact"

invalid_exclusion_repo="$fixture_root/invalid-exclusion"
init_fixture "$invalid_exclusion_repo" "$fixtures/glob-exclusion.json"
expect_failure "$invalid_exclusion_repo" "exclusion 'fixture/src/*.rs' must be exact"

overlap_repo="$fixture_root/overlap"
init_fixture "$overlap_repo" "$fixtures/overlapping-ownership.json"
expect_failure "$overlap_repo" "ownership overlaps at 'fixture/src/capability/nested' and 'fixture/src/capability'"

over_limit_repo="$fixture_root/over-limit"
init_fixture "$over_limit_repo"
printf 'one\ntwo\nthree\n' >"$over_limit_repo/fixture/src/lib.rs"
git -C "$over_limit_repo" add fixture/src/lib.rs
expect_failure "$over_limit_repo" "façade 'fixture/src/lib.rs' has 3 lines; path maximum is 2"

unterminated_over_limit_repo="$fixture_root/unterminated-over-limit"
init_fixture "$unterminated_over_limit_repo"
printf 'one\ntwo\nthree' >"$unterminated_over_limit_repo/fixture/src/lib.rs"
git -C "$unterminated_over_limit_repo" add fixture/src/lib.rs
expect_failure "$unterminated_over_limit_repo" "façade 'fixture/src/lib.rs' has 3 lines; path maximum is 2"

stale_headroom_repo="$fixture_root/stale-headroom"
init_fixture "$stale_headroom_repo"
jq '
  .crates[0].facadeMaximumPhysicalLines = 5 |
  .crates[0].facadeMaximumPhysicalLinesByPath["fixture/src/lib.rs"] = 3
' "$stale_headroom_repo/scripts/architecture/capability-facades.json" >"$stale_headroom_repo/baseline.json"
mv "$stale_headroom_repo/baseline.json" "$stale_headroom_repo/scripts/architecture/capability-facades.json"
git -C "$stale_headroom_repo" add scripts/architecture/capability-facades.json
expect_failure "$stale_headroom_repo" "façade 'fixture/src/lib.rs' has 2 lines below path maximum 3; lower the ratchet"

baseline_rise_repo="$fixture_root/baseline-rise"
init_fixture "$baseline_rise_repo"
git -C "$baseline_rise_repo" -c user.name=fixture -c user.email=fixture@example.test commit -qm baseline
prior_head="$(git -C "$baseline_rise_repo" rev-parse HEAD)"
printf 'one\ntwo\nthree\n' >"$baseline_rise_repo/fixture/src/lib.rs"
jq '
  .crates[0].facadeMaximumPhysicalLines = 5 |
  .crates[0].facadeMaximumPhysicalLinesByPath["fixture/src/lib.rs"] = 3
' "$baseline_rise_repo/scripts/architecture/capability-facades.json" >"$baseline_rise_repo/baseline.json"
mv "$baseline_rise_repo/baseline.json" "$baseline_rise_repo/scripts/architecture/capability-facades.json"
git -C "$baseline_rise_repo" add fixture/src/lib.rs scripts/architecture/capability-facades.json
git -C "$baseline_rise_repo" -c user.name=fixture -c user.email=fixture@example.test commit -qm raised-ceiling
git -C "$baseline_rise_repo" remote add origin "$baseline_rise_repo"
git -C "$baseline_rise_repo" update-ref refs/remotes/origin/milestone-0.2.0 "$prior_head"
git -C "$baseline_rise_repo" update-ref refs/remotes/origin/fix/issue-757 "$(git -C "$baseline_rise_repo" rev-parse HEAD)"
git -C "$baseline_rise_repo" branch --set-upstream-to=origin/fix/issue-757 >/dev/null
expect_failure "$baseline_rise_repo" "a façade maximum increased from the comparison revision; use a temporary exception instead of raising the ratchet"

missing_protected_ref_repo="$fixture_root/missing-protected-ref"
init_fixture "$missing_protected_ref_repo"
if output="$(GITHUB_BASE_REF=milestone-0.2.0 run_checker "$missing_protected_ref_repo" 2>&1)"; then
  echo "Expected pull-request checker to reject an unavailable protected base ref." >&2
  exit 1
fi
[[ "$output" == *"protected delivery ref 'refs/remotes/origin/milestone-0.2.0' is unavailable"* ]] || {
  echo "Pull-request checker rejected missing protected ref for an unexpected reason: $output" >&2
  exit 1
}

stale_exception_repo="$fixture_root/stale-exception"
init_fixture "$stale_exception_repo"
jq '
  .crates[0].facadeMaximumPhysicalLines = 3 |
  .crates[0].facadeMaximumPhysicalLinesByPath["fixture/src/lib.rs"] = 1 |
  .crates[0].temporaryExceptions = [{"paths": ["fixture/src/lib.rs"], "extraLineCeiling": 2, "issue": "#757", "reason": "fixture", "expiresOn": "2099-12-31"}]
' "$stale_exception_repo/scripts/architecture/capability-facades.json" >"$stale_exception_repo/baseline.json"
mv "$stale_exception_repo/baseline.json" "$stale_exception_repo/scripts/architecture/capability-facades.json"
git -C "$stale_exception_repo" add scripts/architecture/capability-facades.json
expect_failure "$stale_exception_repo" "temporary exception has stale headroom: 1 extra lines, ceiling 2"

empty_source_repo="$fixture_root/empty-source"
init_fixture "$empty_source_repo"
rm -rf "$empty_source_repo/fixture/src"
git -C "$empty_source_repo" add -u fixture/src
expect_failure "$empty_source_repo" "façade 'fixture/src/lib.rs' is missing from the indexed inventory"

unowned_repo="$fixture_root/unowned"
init_fixture "$unowned_repo"
printf 'orphan\n' >"$unowned_repo/fixture/src/orphan.rs"
git -C "$unowned_repo" add fixture/src/orphan.rs
expect_failure "$unowned_repo" "belongs to 0 capability owners; expected exactly one"

expired_repo="$fixture_root/expired"
init_fixture "$expired_repo" "$fixtures/expired-exception.json"
expect_failure "$expired_repo" "expired temporary exception ending '2026-01-01'"

path_scoped_repo="$fixture_root/path-scoped"
init_fixture "$path_scoped_repo" "$fixtures/path-scoped-exception.json"
printf 'one\ntwo\nthree\n' >"$path_scoped_repo/fixture/src/lib.rs"
printf 'one\ntwo\nthree\n' >"$path_scoped_repo/fixture/src/shell.rs"
git -C "$path_scoped_repo" add fixture/src/lib.rs fixture/src/shell.rs
expect_failure "$path_scoped_repo" "façade 'fixture/src/shell.rs' has 3 lines; path maximum is 2"

tracked_symlink_repo="$fixture_root/tracked-symlink"
init_fixture "$tracked_symlink_repo"
rm "$tracked_symlink_repo/fixture/src/lib.rs"
ln -s /dev/null "$tracked_symlink_repo/fixture/src/lib.rs"
git -C "$tracked_symlink_repo" add fixture/src/lib.rs
expect_failure "$tracked_symlink_repo" "governed Rust path 'fixture/src/lib.rs' is not a regular indexed blob"

production_override_repo="$fixture_root/production-overrides"
init_fixture "$production_override_repo"
for override in CAPABILITY_FACADES_INVENTORY CAPABILITY_FACADES_TODAY CAPABILITY_FACADES_TEST_MODE; do
  if output="$(
    cd "$production_override_repo"
    env "$override=untrusted" "$checker" 2>&1
  )"; then
    echo "Expected production checker to reject $override." >&2
    exit 1
  fi
  [[ "$output" == *"production checker does not accept $override"* ]] || {
    echo "Production checker rejected $override for an unexpected reason: $output" >&2
    exit 1
  }
  if output="$(env "$override=untrusted" "$architecture_checker" 2>&1)"; then
    echo "Expected architecture checker to reject $override." >&2
    exit 1
  fi
  [[ "$output" == *"Architecture check does not accept $override"* ]] || {
    echo "Architecture checker rejected $override for an unexpected reason: $output" >&2
    exit 1
  }
done

if output="$(cd "$production_override_repo" && "$checker" unexpected 2>&1)"; then
  echo "Expected production checker to reject arguments." >&2
  exit 1
fi
[[ "$output" == *"production checker does not accept arguments"* ]] || {
  echo "Production checker rejected arguments for an unexpected reason: $output" >&2
  exit 1
}
if output="$("$architecture_checker" unexpected 2>&1)"; then
  echo "Expected architecture checker to reject arguments." >&2
  exit 1
fi
[[ "$output" == *"Architecture check does not accept arguments"* ]] || {
  echo "Architecture checker rejected arguments for an unexpected reason: $output" >&2
  exit 1
}

prepare_staged_over_limit() {
  local repo="$1"
  init_fixture "$repo"
  printf 'one\ntwo\nthree\n' >"$repo/fixture/src/lib.rs"
  git -C "$repo" add fixture/src/lib.rs
}

parent_symlink_repo="$fixture_root/parent-symlink"
prepare_staged_over_limit "$parent_symlink_repo"
external_parent="$fixture_root/external-parent"
mkdir -p "$external_parent"
printf 'short\n' >"$external_parent/lib.rs"
rm -rf "$parent_symlink_repo/fixture/src"
ln -s "$external_parent" "$parent_symlink_repo/fixture/src"
expect_failure "$parent_symlink_repo" "façade 'fixture/src/lib.rs' has 3 lines; path maximum is 2"

final_symlink_repo="$fixture_root/final-symlink"
prepare_staged_over_limit "$final_symlink_repo"
external_final="$fixture_root/external-final.rs"
printf 'short\n' >"$external_final"
rm "$final_symlink_repo/fixture/src/lib.rs"
ln -s "$external_final" "$final_symlink_repo/fixture/src/lib.rs"
expect_failure "$final_symlink_repo" "façade 'fixture/src/lib.rs' has 3 lines; path maximum is 2"

hardlink_repo="$fixture_root/hardlink"
prepare_staged_over_limit "$hardlink_repo"
external_hardlink="$fixture_root/external-hardlink.rs"
printf 'short\n' >"$external_hardlink"
rm "$hardlink_repo/fixture/src/lib.rs"
ln "$external_hardlink" "$hardlink_repo/fixture/src/lib.rs"
expect_failure "$hardlink_repo" "façade 'fixture/src/lib.rs' has 3 lines; path maximum is 2"

worktree_mismatch_repo="$fixture_root/worktree-mismatch"
prepare_staged_over_limit "$worktree_mismatch_repo"
printf 'short\n' >"$worktree_mismatch_repo/fixture/src/lib.rs"
expect_failure "$worktree_mismatch_repo" "façade 'fixture/src/lib.rs' has 3 lines; path maximum is 2"

source_replace_repo="$fixture_root/source-replace"
prepare_staged_over_limit "$source_replace_repo"
indexed_source_object="$(git -C "$source_replace_repo" ls-files -s -- fixture/src/lib.rs | awk '{print $2}')"
short_source_object="$(printf 'short\n' | git -C "$source_replace_repo" hash-object -w --stdin)"
git -C "$source_replace_repo" replace "$indexed_source_object" "$short_source_object"
expect_failure "$source_replace_repo" "façade 'fixture/src/lib.rs' has 3 lines; path maximum is 2"

baseline_replace_repo="$fixture_root/baseline-replace"
init_fixture "$baseline_replace_repo"
printf 'orphan\n' >"$baseline_replace_repo/fixture/src/orphan.rs"
git -C "$baseline_replace_repo" add fixture/src/orphan.rs
indexed_baseline_object="$(git -C "$baseline_replace_repo" ls-files -s -- scripts/architecture/capability-facades.json | awk '{print $2}')"
replacement_baseline_object="$(
  jq '.crates[0].exclusions += [{"path": "fixture/src/orphan.rs", "classification": "fixture"}]' \
    "$fixtures/valid.json" | git -C "$baseline_replace_repo" hash-object -w --stdin
)"
git -C "$baseline_replace_repo" replace "$indexed_baseline_object" "$replacement_baseline_object"
expect_failure "$baseline_replace_repo" "source 'fixture/src/orphan.rs' belongs to 0 capability owners; expected exactly one"

unusual_path_repo="$fixture_root/unusual-path"
init_fixture "$unusual_path_repo"
unusual_path=$'fixture/src/orphan-é\n.rs'
printf 'orphan\n' >"$unusual_path_repo/$unusual_path"
git -C "$unusual_path_repo" add -- "$unusual_path"
expect_failure "$unusual_path_repo" "belongs to 0 capability owners; expected exactly one"

echo "Capability façade index-authority and negative fixtures passed."
