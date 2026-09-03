#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

[ "$#" -ge 3 ] && [ "$#" -le 4 ] || exit 2
platform="$1"
target="$2"
output="$3"
profile="${4:-local}"
[[ "$output" = /* ]] || exit 1
[ ! -e "$output" ] && [ ! -L "$output" ] || exit 1
case "$platform:$target:$profile" in
  ios_simulator:aarch64-apple-ios-sim:local|ios_simulator:x86_64-apple-ios:local)
    authority_profile="standalone-local-development-portal"
    ;;
  android_qemu:aarch64-linux-android:local|android_qemu:x86_64-linux-android:local)
    authority_profile="standalone-local-development-portal"
    ;;
  android_physical:aarch64-linux-android:tailnet-android)
    authority_profile="standalone-tailnet-development-portal-android"
    ;;
  *) exit 1 ;;
esac
umask 077
printf '{"platform":"%s","profile":"%s","schema":"oxid-app-profile-authority-v2","target":"%s"}' \
  "$platform" "$authority_profile" "$target" >"$output"
chmod 600 "$output"
