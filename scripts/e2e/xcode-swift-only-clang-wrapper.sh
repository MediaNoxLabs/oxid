#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

readonly REAL_CLANG="${OXID_XCODE_REAL_CLANG:-}"
[[ "$REAL_CLANG" = /* && "$REAL_CLANG" != *$'\n'* ]] || exit 64
[ -x "$REAL_CLANG" ] && [ ! -L "$REAL_CLANG" ] || exit 64

# Xcode 26.4 can deadlock while planning a Swift-only target when its clang
# capability probe fills an internal 16 KiB pipe. The generated Portal XCTest
# project has no C-family sources, so omit only that metadata probe and delegate
# every real compile or link invocation to the selected Xcode toolchain.
probe=0
null_input=0
for argument in "$@"; do
  [ "$argument" != -dM ] || probe=1
  [ "$argument" != /dev/null ] || null_input=1
done
if [ "$probe" = 1 ] && [ "$null_input" = 1 ]; then
  exit 0
fi

exec "$REAL_CLANG" "$@"
