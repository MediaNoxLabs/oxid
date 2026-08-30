#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly ROOT
# shellcheck source=ios-simulator-ownership.sh
source "$ROOT/scripts/e2e/ios-simulator-ownership.sh"

fail() {
  printf 'ios-simulator-ownership-contract: FAIL phase=%s\n' "$1" >&2
  exit 1
}

temporary="$(mktemp -d "${TMPDIR:-/tmp}/oxid-ios-ownership.XXXXXX")"
cleanup() { rm -rf -- "$temporary"; }
trap cleanup EXIT

readonly developer="$temporary/Xcode.app/Contents/Developer"
readonly fake_bin="$temporary/bin"
readonly log="$temporary/simctl.log"
readonly created_udid="AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE"
mkdir -p "$developer" "$fake_bin"

cat >"$fake_bin/xcodebuild" <<'EOF'
#!/usr/bin/env bash
printf 'Xcode fixture\n'
EOF
cat >"$fake_bin/xcrun" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$OXID_FAKE_SIMCTL_LOG"
[ "${1:-}" = simctl ] || exit 90
shift
case "${1:-}" in
  list)
    case "${2:-}" in
      runtimes)
        printf '%s\n' '{"runtimes":[{"identifier":"com.apple.CoreSimulator.SimRuntime.iOS-26-4","isAvailable":true,"version":"26.4"}]}'
        ;;
      devicetypes)
        printf '%s\n' '{"devicetypes":[{"identifier":"com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro","name":"iPhone 17 Pro"}]}'
        ;;
      devices)
        if [ "${OXID_FAKE_RECEIPT_CHANGED:-0}" = 1 ]; then name="foreign"; else name="oxid-owned"; fi
        printf '{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-26-4":['
        printf '{"udid":"99999999-8888-7777-6666-555555555555","name":"existing","state":"Booted","isAvailable":true,"deviceTypeIdentifier":"com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro"}'
        if [ ! -f "$OXID_FAKE_SIM_DELETED" ]; then
          printf ',{"udid":"AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE","name":"%s","state":"Booted","isAvailable":true,"deviceTypeIdentifier":"com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro"}' "$name"
        fi
        printf ']}}\n'
        ;;
      *) exit 91 ;;
    esac
    ;;
  create)
    printf '%s\n' 'AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE'
    ;;
  boot|bootstatus|install|terminate|launch|openurl|shutdown|delete)
    [ "${2:-}" = 'AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE' ] || exit 92
    [ "${1:-}" != delete ] || : >"$OXID_FAKE_SIM_DELETED"
    ;;
  *) exit 93 ;;
esac
EOF
chmod 700 "$fake_bin/xcodebuild" "$fake_bin/xcrun"

export OXID_XCRUN="$fake_bin/xcrun"
export OXID_XCODEBUILD="$fake_bin/xcodebuild"
export OXID_FAKE_SIMCTL_LOG="$log"
export OXID_FAKE_SIM_DELETED="$temporary/simulator.deleted"
export OXID_IOS_OPERATION_TIMEOUT_SECONDS=2
readonly runtime="com.apple.CoreSimulator.SimRuntime.iOS-26-4"
readonly device_type="com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro"

: >"$log"
if oxid_ios_preflight "" "$runtime" "$device_type"; then fail missing-xcode; fi
if oxid_ios_preflight "$developer" malformed "$device_type"; then fail malformed-runtime; fi
if oxid_ios_preflight "$developer" "$runtime" malformed; then fail malformed-device-type; fi
[ ! -s "$log" ] || fail malformed-mutated

OXID_IOS_KEEP_FAILED=1
export OXID_IOS_KEEP_FAILED
if oxid_ios_preflight "$developer" "$runtime" "$device_type"; then fail keep-failed; fi
unset OXID_IOS_KEEP_FAILED
[ ! -s "$log" ] || fail keep-failed-mutated

oxid_ios_preflight "$developer" "$runtime" "$device_type" || fail valid-preflight
if grep -q '^simctl create' "$log"; then fail preflight-created; fi

receipt="$temporary/receipt.json"
udid="$(oxid_ios_create_owned "$developer" "$runtime" "$device_type" oxid-owned "$receipt")" || fail create
[ "$udid" = "$created_udid" ] || fail returned-udid
[ "$(stat -c '%a' "$receipt" 2>/dev/null || stat -f '%Lp' "$receipt")" = 600 ] || fail receipt-mode
oxid_ios_owned_simctl "$developer" "$receipt" boot || fail boot
oxid_ios_owned_simctl "$developer" "$receipt" bootstatus -b || fail bootstatus
oxid_ios_owned_simctl "$developer" "$receipt" install "$temporary/OxidApp.app" || fail install
oxid_ios_owned_simctl "$developer" "$receipt" terminate io.medianox.oxid || fail terminate
if grep -Eq 'simctl (boot|bootstatus|install|terminate) 99999999-8888-7777-6666-555555555555' "$log"; then
  fail existing-booted-mutated
fi

before_delete="$(wc -l <"$log" | tr -d ' ')"
OXID_FAKE_RECEIPT_CHANGED=1
export OXID_FAKE_RECEIPT_CHANGED
if oxid_ios_delete_owned "$developer" "$receipt"; then fail changed-receipt-delete; fi
unset OXID_FAKE_RECEIPT_CHANGED
[ -f "$receipt" ] || fail changed-receipt-preserved
if tail -n "+$((before_delete + 1))" "$log" | grep -q '^simctl delete'; then fail changed-receipt-mutated; fi

oxid_ios_delete_owned "$developer" "$receipt" || fail exact-delete
[ ! -e "$receipt" ] || fail receipt-cleanup

grep -q '^simctl create oxid-owned com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro com.apple.CoreSimulator.SimRuntime.iOS-26-4$' "$log" \
  || fail create-selector
grep -q '^simctl shutdown AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE$' "$log" || fail exact-shutdown
grep -q '^simctl delete AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE$' "$log" || fail exact-delete-selector

cat >"$fake_bin/xcrun-timeout" <<'EOF'
#!/usr/bin/env bash
sleep 30
EOF
chmod 700 "$fake_bin/xcrun-timeout"
OXID_XCRUN="$fake_bin/xcrun-timeout"
OXID_IOS_OPERATION_TIMEOUT_SECONDS=0.1
export OXID_XCRUN OXID_IOS_OPERATION_TIMEOUT_SECONDS
started=$SECONDS
if oxid_ios_preflight "$developer" "$runtime" "$device_type"; then fail timeout-result; fi
[ $((SECONDS - started)) -lt 5 ] || fail timeout-bounded

printf 'ios-simulator-ownership-contract: PASS selection=explicit existing=ignored receipt=identity-bound cleanup=bounded keep-failed=rejected\n'
