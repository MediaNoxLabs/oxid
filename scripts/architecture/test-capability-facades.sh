#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
checker="$repository_root/scripts/check-capability-facades.sh"
fixtures="$repository_root/scripts/architecture/fixtures/capability-facades"

CAPABILITY_FACADES_INVENTORY="$fixtures/valid.inventory" \
  CAPABILITY_FACADES_TODAY="2026-01-02" \
  "$checker" "$fixtures/valid.json" >/dev/null

expect_failure() {
  local baseline="$1"
  local inventory="$2"
  local expected="$3"
  local output

  if output="$(
    CAPABILITY_FACADES_INVENTORY="$inventory" \
      CAPABILITY_FACADES_TODAY="2026-01-02" \
      "$checker" "$baseline" 2>&1
  )"; then
    echo "Expected capability façade fixture to fail: $inventory" >&2
    exit 1
  fi
  if [[ "$output" != *"$expected"* ]]; then
    echo "Capability façade fixture failed for an unexpected reason: $output" >&2
    exit 1
  fi
}

expect_failure \
  "$fixtures/invalid-schema.json" \
  "$fixtures/valid.inventory" \
  "baseline schema is invalid"
expect_failure \
  "$fixtures/invalid-facade-path.json" \
  "$fixtures/valid.inventory" \
  "façade path 'fixture/src/*.rs' must be exact"
expect_failure \
  "$fixtures/glob-exclusion.json" \
  "$fixtures/valid.inventory" \
  "exclusion 'fixture/src/*.rs' must be exact"
expect_failure \
  "$fixtures/overlapping-ownership.json" \
  "$fixtures/overlapping.inventory" \
  "ownership overlaps at 'fixture/src/capability/nested' and 'fixture/src/capability'"
expect_failure \
  "$fixtures/valid.json" \
  "$fixtures/over-limit.inventory" \
  "façade 'fixture/src/lib.rs' has 3 lines; path maximum is 2"
expect_failure \
  "$fixtures/valid.json" \
  "$fixtures/unowned.inventory" \
  "source 'fixture/src/orphan.rs' belongs to 0 capability owners"
expect_failure \
  "$fixtures/expired-exception.json" \
  "$fixtures/valid.inventory" \
  "expired temporary exception ending '2026-01-01'"
expect_failure \
  "$fixtures/path-scoped-exception.json" \
  "$fixtures/other-facade-over-limit.inventory" \
  "façade 'fixture/src/shell.rs' has 3 lines; path maximum is 2"

symlink_fixture="$(mktemp -d)"
trap 'rm -rf "$symlink_fixture"' EXIT
git init -q "$symlink_fixture"
mkdir -p "$symlink_fixture/fixture/src"
printf 'capability\n' >"$symlink_fixture/fixture/src/capability.rs"
printf 'generated\n' >"$symlink_fixture/fixture/src/generated.rs"
printf 'protocol fixture\n' >"$symlink_fixture/fixture/src/protocol_fixture.rs"
printf 'shell\n' >"$symlink_fixture/fixture/src/shell.rs"
ln -s /dev/null "$symlink_fixture/fixture/src/lib.rs"
git -C "$symlink_fixture" add fixture/src
if output="$(
  cd "$symlink_fixture"
  CAPABILITY_FACADES_TODAY="2026-01-02" "$checker" "$fixtures/valid.json" 2>&1
)"; then
  echo "Expected tracked symlink fixture to fail." >&2
  exit 1
fi
if [[ "$output" != *"governed Rust path 'fixture/src/lib.rs' is not a regular tracked file"* ]]; then
  echo "Tracked symlink fixture failed for an unexpected reason: $output" >&2
  exit 1
fi

echo "Capability façade negative fixtures passed."
