#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: write-portal-profile-authority.sh <platform> <target> <absolute-output>" >&2
  exit 2
fi

platform="$1"
target="$2"
output="$3"
case "$platform:$target" in
  ios_simulator:aarch64-apple-ios-sim|ios_simulator:x86_64-apple-ios|\
  android_qemu:aarch64-linux-android|android_qemu:x86_64-linux-android)
    ;;
  *)
    echo "Portal profile authority permits only reviewed virtual-device target pairs." >&2
    exit 1
    ;;
esac

if [[ "$output" != /* ]] || [ ! -d "$(dirname -- "$output")" ] || \
  [ -L "$(dirname -- "$output")" ] || [ -e "$output" ] || [ -L "$output" ]; then
  echo "Portal profile authority output must be a new file in an existing absolute non-symlink directory." >&2
  exit 1
fi

umask 077
printf '{"platform":"%s","profile":"standalone-local-development-portal","schema":"oxid-app-profile-authority-v1","target":"%s"}' \
  "$platform" "$target" >"$output"
chmod 600 "$output"
