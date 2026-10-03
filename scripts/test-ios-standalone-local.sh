#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "The iOS standalone-local smoke test requires macOS and Xcode." >&2
  exit 1
fi

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export OXID_STANDALONE_STATE_DIR="${OXID_STANDALONE_STATE_DIR:-$repository_root/target/mobile-tests/ios-standalone-stack}"

# Desktop supervisors do not always inherit the interactive shell profile even
# when Nix is installed. Admit the canonical multi-user profile explicitly so
# this owner-facing lane behaves the same from Terminal, Pi.dev, and Codex.
if ! command -v nix >/dev/null 2>&1 \
  && [ -x /nix/var/nix/profiles/default/bin/nix ]; then
  export PATH="/nix/var/nix/profiles/default/bin:$PATH"
fi

for command_name in nix jq; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

# This lane packages the checked-in mobile icons and therefore needs the
# repository's declared Pillow runtime. Re-enter the flake shell once when a
# desktop supervisor supplied only the Nix executable, not the dev-shell PATH.
if ! python3 -c 'from PIL import Image' >/dev/null 2>&1; then
  if [ "${OXID_IOS_STANDALONE_IN_NIX:-0}" = "1" ]; then
    echo "The Oxid Nix development shell does not provide Python Pillow." >&2
    exit 1
  fi
  export OXID_IOS_STANDALONE_IN_NIX=1
  exec nix develop --command "$0" "$@"
fi
if [ ! -x /usr/bin/xcodebuild ] || [ ! -x /usr/bin/xcrun ]; then
  echo "Xcode is required for the iOS standalone-local smoke test." >&2
  exit 1
fi

cd "$repository_root"

"$repository_root/scripts/standalone-up.sh" local

faucet_log="$repository_root/target/mobile-tests/ios-standalone-faucet.log"
mkdir -p "$(dirname "$faucet_log")"
"$repository_root/scripts/run-standalone-faucet-http.sh" >"$faucet_log" 2>&1 &
faucet_pid=$!
cleanup_faucet() {
  if kill -0 "$faucet_pid" 2>/dev/null; then
    kill "$faucet_pid" 2>/dev/null || true
    wait "$faucet_pid" 2>/dev/null || true
  fi
}
trap cleanup_faucet EXIT

faucet_ready=0
for _ in {1..120}; do
  if ! kill -0 "$faucet_pid" 2>/dev/null; then
    echo "The standalone faucet exited before becoming ready." >&2
    tail -n 80 "$faucet_log" >&2
    exit 1
  fi
  if curl --fail --silent --max-time 2 http://127.0.0.1:36301/health >/dev/null; then
    faucet_ready=1
    break
  fi
  sleep 1
done
if [ "$faucet_ready" != "1" ]; then
  echo "The standalone faucet did not become ready within two minutes." >&2
  tail -n 80 "$faucet_log" >&2
  exit 1
fi

device="${OXID_IOS_DEVICE:-}"
if [ -z "$device" ]; then
  device="$(
    /usr/bin/xcrun simctl list devices booted -j \
      | jq -r 'first(.devices[][] | select(.isAvailable and (.name | startswith("iPhone"))) | .udid) // empty'
  )"
fi
if [ -z "$device" ]; then
  device="$(
    /usr/bin/xcrun simctl list devices available -j \
      | jq -r 'first(.devices[][] | select(.isAvailable and (.name | startswith("iPhone"))) | .udid) // empty'
  )"
fi
if [ -z "$device" ]; then
  echo "No available iPhone simulator was found." >&2
  exit 1
fi

OXID_IOS_DEVICE="$device" \
OXID_IOS_RESET_DATA=1 \
OXID_STANDALONE_NETWORK_PROFILE=local \
  "$repository_root/scripts/run-ios-simulator.sh"

xcodegen_output="$(nix build .#xcodegen --no-link --print-out-paths)"
generated_project_root="$repository_root/target/mobile-tests/ios"
mkdir -p "$generated_project_root"
OXID_REPOSITORY_ROOT="$repository_root" \
  "$xcodegen_output/bin/xcodegen" generate \
    --spec "$repository_root/tests/mobile/ios/project.yml" \
    --project "$generated_project_root"

xcode_developer_dir="$(env -u DEVELOPER_DIR /usr/bin/xcode-select -p)"
host_user="$(id -un)"
app_bundle="$repository_root/target/dx/oxid-app/debug/ios/OxidApp.app"
bundle_identifier="$(/usr/bin/plutil -extract CFBundleIdentifier raw "$app_bundle/Info.plist")"

env -i \
  "DEVELOPER_DIR=$xcode_developer_dir" \
  "HOME=$HOME" \
  "LANG=${LANG:-en_US.UTF-8}" \
  "LOGNAME=$host_user" \
  "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
  "TMPDIR=${TMPDIR:-/tmp}" \
  "USER=$host_user" \
  /usr/bin/xcodebuild test \
  -project "$generated_project_root/OxidMobileSmoke.xcodeproj" \
  -scheme OxidUITests \
  -destination "platform=iOS Simulator,id=$device" \
  -derivedDataPath "$repository_root/target/mobile-tests/ios-standalone-local-derived-data" \
  -only-testing:"OxidUITests/StandaloneLocalAccountTests/testSynchronizesProtectedAccountFromLocalStandaloneStack" \
  CODE_SIGNING_ALLOWED=NO

/usr/bin/xcrun simctl launch "$device" "$bundle_identifier" >/dev/null
device_name="$(
  /usr/bin/xcrun simctl list devices -j \
    | jq -r --arg device "$device" 'first(.devices[][] | select(.udid == $device) | .name) // "unknown"'
)"
runtime="$(
  /usr/bin/xcrun simctl list devices -j \
    | jq -r --arg device "$device" '
        first(
          .devices | to_entries[] as $runtime
          | $runtime.value[]
          | select(.udid == $device)
          | $runtime.key
        ) // "unknown"
      '
)"
echo "iOS localhost standalone live-account smoke passed at $(git rev-parse HEAD) on $device_name ($runtime), bundle $bundle_identifier."
