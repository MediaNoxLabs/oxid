#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

for required_command in jq node tailscale; do
  command -v "$required_command" >/dev/null 2>&1 || {
    echo "Required command '$required_command' is missing." >&2
    exit 1
  }
done

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
origin_policy="$repository_root/scripts/e2e/tailnet-origin-policy.mjs"
android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "$android_sdk" ] && [ "$(uname -s)" = Darwin ]; then
  android_sdk="$HOME/Library/Android/sdk"
fi
adb_command="$android_sdk/platform-tools/adb"
[ -x "$adb_command" ] || {
  echo "Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed Android SDK." >&2
  exit 1
}

if "$adb_command" devices | awk '$1 ~ /^emulator-/ && $2 == "device" { found=1 } END { exit !found }'; then
  echo "Stop the Android emulator before starting a physical-device tailnet build." >&2
  exit 1
fi
if xcrun simctl list devices 2>/dev/null | grep -q '(Booted)'; then
  echo "Shut down the iOS simulator before starting a physical-device tailnet build." >&2
  exit 1
fi

physical_devices="$($adb_command devices | awk 'NR > 1 && $2 == "device" && $1 !~ /^emulator-/ { print $1 }')"
device="${OXID_ANDROID_DEVICE:-}"
if [ -n "$device" ]; then
  [ "$(printf '%s\n' "$physical_devices" | awk -v selected="$device" '$0 == selected { count++ } END { print count + 0 }')" -eq 1 ] || {
    echo "OXID_ANDROID_DEVICE must select one authorized physical Android device." >&2
    exit 1
  }
else
  [ "$(printf '%s\n' "$physical_devices" | awk 'NF { count++ } END { print count + 0 }')" -eq 1 ] || {
    echo "Exactly one authorized physical Android device is required unless OXID_ANDROID_DEVICE selects one." >&2
    exit 1
  }
  device="$physical_devices"
fi
adb_device() { ANDROID_SERIAL="$device" "$adb_command" "$@"; }
[ "$(adb_device shell getprop ro.kernel.qemu | tr -d '\r\n')" = 0 ] || {
  echo "The selected Android device must be physical." >&2
  exit 1
}

status="$(tailscale status --json)"
[ "$(jq -r '.BackendState' <<<"$status")" = Running ] || {
  echo "Tailscale is not connected on the laptop." >&2
  exit 1
}
tailnet_dns_name="$(jq -r '.Self.DNSName | rtrimstr(".")' <<<"$status")"
OXID_TAILNET_ORIGIN_POLICY_INPUT="$tailnet_dns_name" node "$origin_policy" --host-env || {
  echo "Tailscale did not report a canonical MagicDNS identity." >&2
  exit 1
}
route_receipt="$repository_root/target/standalone-tailnet-routes/receipt.json"
if [ -f "$route_receipt" ] && [ ! -L "$route_receipt" ]; then
  "$repository_root/scripts/standalone-tailnet-routes.sh" status >/dev/null || {
    echo "The receipt-owned standalone Tailnet routes are unavailable." >&2
    exit 1
  }
  receipt_dns_name="$(jq -r '.dnsName' "$route_receipt")"
  [ "$receipt_dns_name" = "$tailnet_dns_name" ] || {
    echo "The standalone Tailnet route receipt belongs to a different host identity." >&2
    exit 1
  }
  indexer_port="$(jq -r '.routes[] | select(.name == "indexer") | .port' "$route_receipt")"
  node_port="$(jq -r '.routes[] | select(.name == "node") | .port' "$route_receipt")"
  proof_port="$(jq -r '.routes[] | select(.name == "proof") | .port' "$route_receipt")"
else
  serve_status="$(tailscale serve status --json)"
  jq -e '
    .TCP["443"].HTTPS == true
    and .TCP["8443"].HTTPS == true
    and .TCP["10000"].HTTPS == true
  ' >/dev/null <<<"$serve_status" || {
    echo "Protected standalone Serve routes are unavailable; prepare the Tailnet demo first." >&2
    exit 1
  }
  indexer_port=8443
  node_port=10000
  proof_port=443
fi

export OXID_ANDROID_DEVICE="$device"
export OXID_STANDALONE_NETWORK_PROFILE=tailnet
export OXID_BUILD_MIDNIGHT_INDEXER_WS_URL="wss://$tailnet_dns_name:$indexer_port/api/v4/graphql/ws"
export OXID_BUILD_MIDNIGHT_INDEXER_HTTP_URL="https://$tailnet_dns_name:$indexer_port/api/v4/graphql"
export OXID_BUILD_MIDNIGHT_NODE_WS_URL="wss://$tailnet_dns_name:$node_port"
export OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL="https://$tailnet_dns_name:$proof_port"

exec "$repository_root/scripts/run-android-emulator.sh"
