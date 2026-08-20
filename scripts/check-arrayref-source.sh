#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

expected_lock_source='source = "registry+https://github.com/rust-lang/crates.io-index"'
expected_checksum='checksum = "76a2e8124351fda1ef8aaaa3bbd7ebbcb486bbcd4225aca0aa0d84bb2db8fecb"'

if grep -Eq '^arrayref[[:space:]]*=' Cargo.toml; then
  echo "arrayref must not be patched to an independently fetched source." >&2
  exit 1
fi

arrayref_lock_block=$(awk '
  /^\[\[package\]\]$/ { in_package = 1; block = $0 ORS; next }
  in_package { block = block $0 ORS }
  in_package && /^name = "arrayref"$/ { is_arrayref = 1 }
  in_package && is_arrayref && /^$/ { printf "%s", block; exit }
' Cargo.lock)

if [[ "$arrayref_lock_block" != *'version = "0.3.9"'* ]] ||
  [[ "$arrayref_lock_block" != *"$expected_lock_source"* ]] ||
  [[ "$arrayref_lock_block" != *"$expected_checksum"* ]]; then
  echo "Cargo.lock does not resolve the reviewed arrayref 0.3.9 registry archive." >&2
  exit 1
fi

if grep -Fqx 'name = "proc-macro1"' Cargo.lock; then
  echo "The unreviewed proc-macro1 dependency must not enter the Oxid graph." >&2
  exit 1
fi

if grep -Fq 'arrayrefOutputHash' nix/packages/default.nix ||
  grep -Eq '"arrayref-0\.3\.9"[[:space:]]*=' nix/packages/default.nix; then
  echo "Nix must consume the checksum-locked registry archive without a Git output hash." >&2
  exit 1
fi

echo "arrayref reviewed registry archive pin passed."
