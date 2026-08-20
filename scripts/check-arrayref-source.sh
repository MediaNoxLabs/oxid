#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

expected_revision="f8d0299d863922db6c409d08098941e833b70d69"
expected_patch="arrayref = { git = \"https://github.com/droundy/arrayref.git\", rev = \"${expected_revision}\" }"
expected_lock_source="source = \"git+https://github.com/droundy/arrayref.git?rev=${expected_revision}#${expected_revision}\""

if ! grep -Fqx "$expected_patch" Cargo.toml; then
  echo "arrayref must remain pinned to the reviewed canonical revision ${expected_revision}." >&2
  exit 1
fi

arrayref_lock_block=$(awk '
  /^\[\[package\]\]$/ { in_package = 1; block = $0 ORS; next }
  in_package { block = block $0 ORS }
  in_package && /^name = "arrayref"$/ { is_arrayref = 1 }
  in_package && is_arrayref && /^$/ { printf "%s", block; exit }
' Cargo.lock)

if [[ "$arrayref_lock_block" != *'version = "0.3.9"'* ]] ||
  [[ "$arrayref_lock_block" != *"$expected_lock_source"* ]]; then
  echo "Cargo.lock does not resolve the reviewed arrayref 0.3.9 Git source." >&2
  exit 1
fi

if grep -Fqx 'name = "proc-macro1"' Cargo.lock; then
  echo "The unreviewed proc-macro1 dependency must not enter the Oxid graph." >&2
  exit 1
fi

echo "arrayref canonical source pin passed."
