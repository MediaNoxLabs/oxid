#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for required_command in jq tailscale; do
  if ! command -v "$required_command" >/dev/null 2>&1; then
    echo "Required command '$required_command' is missing." >&2
    exit 1
  fi
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = "Darwin" ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
adb_command="$android_sdk/platform-tools/adb"
if [ ! -x "$adb_command" ]; then
  echo "Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed Android SDK." >&2
  exit 1
fi

if "$adb_command" devices | awk '$1 ~ /^emulator-/ && $2 == "device" { found=1 } END { exit !found }'; then
  echo "Stop the Android emulator before starting a physical-device tailnet build." >&2
  exit 1
fi
if xcrun simctl list devices 2>/dev/null | grep -q '(Booted)'; then
  echo "Shut down the iOS simulator before starting a physical-device tailnet build." >&2
  exit 1
fi

device="${OXID_ANDROID_DEVICE:-}"
if [ -z "$device" ]; then
  device="$($adb_command devices -l | awk '$2 == "device" && $1 !~ /^emulator-/ {print $1; exit}')"
fi
if [ -z "$device" ] || [ "$($adb_command -s "$device" shell getprop ro.kernel.qemu | tr -d '\r')" != "0" ]; then
  echo "An authorized physical Android device is required." >&2
  exit 1
fi

if [ "$(tailscale status --json | jq -r '.BackendState')" != "Running" ]; then
  echo "Tailscale is not connected on the laptop." >&2
  exit 1
fi
tailnet_dns_name="$(tailscale status --json | jq -r '.Self.DNSName | rtrimstr(".")')"
if [ -z "$tailnet_dns_name" ] || [ "$tailnet_dns_name" = "null" ]; then
  echo "Tailscale did not report a MagicDNS name." >&2
  exit 1
fi
if [ "$(tailscale serve status 2>&1 || true)" = "No serve config" ]; then
  echo "Run 'just standalone-phone-up' before building the phone profile." >&2
  exit 1
fi

export OXID_ANDROID_DEVICE="$device"
export OXID_STANDALONE_NETWORK_PROFILE=tailnet
export OXID_BUILD_MIDNIGHT_INDEXER_WS_URL="wss://$tailnet_dns_name:8443/api/v4/graphql/ws"
export OXID_BUILD_MIDNIGHT_INDEXER_HTTP_URL="https://$tailnet_dns_name:8443/api/v4/graphql"
export OXID_BUILD_MIDNIGHT_NODE_WS_URL="wss://$tailnet_dns_name:10000"
export OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL="https://$tailnet_dns_name"

exec "$repository_root/scripts/run-android-emulator.sh"
