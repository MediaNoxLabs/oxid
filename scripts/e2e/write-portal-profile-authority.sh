#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo "usage: write-portal-profile-authority.sh <platform> <target> <absolute-output> [local|tailnet-ios-simulator|tailnet-android-physical]" >&2
  exit 2
fi

platform="$1"
target="$2"
output="$3"
profile="${4:-local}"
case "$profile:$platform:$target" in
  local:ios_simulator:aarch64-apple-ios-sim|local:ios_simulator:x86_64-apple-ios|\
  local:android_qemu:aarch64-linux-android|local:android_qemu:x86_64-linux-android|\
  tailnet-ios-simulator:ios_simulator:aarch64-apple-ios-sim|tailnet-ios-simulator:ios_simulator:x86_64-apple-ios|\
  tailnet-android-physical:android_physical_tailnet:aarch64-linux-android)
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

case "$profile" in
  local) authority_profile="standalone-local-development-portal" ;;
  tailnet-ios-simulator) authority_profile="standalone-tailnet-development-portal-ios-simulator" ;;
  tailnet-android-physical) authority_profile="standalone-tailnet-development-portal-android-physical" ;;
esac

umask 077
printf '{"platform":"%s","profile":"%s","schema":"oxid-app-profile-authority-v1","target":"%s"}' \
  "$platform" "$authority_profile" "$target" >"$output"
chmod 600 "$output"
