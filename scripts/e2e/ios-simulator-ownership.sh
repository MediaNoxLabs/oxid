#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

oxid_ios_operation() {
  local developer_dir="$1"
  shift
  local deadline="${OXID_IOS_OPERATION_TIMEOUT_SECONDS:-30}"
  timeout -k 2s "${deadline}s" env DEVELOPER_DIR="$developer_dir" "$@"
}

oxid_ios_xcrun() {
  local developer_dir="$1"
  shift
  oxid_ios_operation "$developer_dir" "${OXID_XCRUN:-/usr/bin/xcrun}" "$@"
}

oxid_ios_xcodebuild() {
  local developer_dir="$1"
  shift
  oxid_ios_operation "$developer_dir" "${OXID_XCODEBUILD:-/usr/bin/xcodebuild}" "$@"
}

oxid_ios_filesystem_identity() {
  local path="$1" deadline="${OXID_IOS_OPERATION_TIMEOUT_SECONDS:-30}"
  if identity="$(timeout -k 2s "${deadline}s" stat -c '%d:%i' -- "$path" 2>/dev/null)"; then
    printf '%s\n' "$identity"
    return 0
  fi
  timeout -k 2s "${deadline}s" stat -f '%d:%i' -- "$path" 2>/dev/null
}

oxid_ios_receipt_mode_is_private() {
  local receipt="$1" mode
  [ -f "$receipt" ] && [ ! -L "$receipt" ] || return 1
  if mode="$(stat -c '%a' -- "$receipt" 2>/dev/null)"; then :; else mode="$(stat -f '%Lp' -- "$receipt")"; fi
  [ "$mode" = 600 ]
}

oxid_ios_validate_selectors() {
  local developer_dir="$1" runtime_id="$2" device_type_id="$3"
  [[ "$developer_dir" = /* && "$developer_dir" != *$'\n'* ]] || return 1
  [ -d "$developer_dir" ] && [ ! -L "$developer_dir" ] || return 1
  [[ "$runtime_id" =~ ^com\.apple\.CoreSimulator\.SimRuntime\.iOS-[0-9]+-[0-9]+$ ]] || return 1
  [[ "$device_type_id" =~ ^com\.apple\.CoreSimulator\.SimDeviceType\.iPhone-[A-Za-z0-9-]+$ ]] || return 1
  [ -x "${OXID_XCRUN:-/usr/bin/xcrun}" ] && [ -x "${OXID_XCODEBUILD:-/usr/bin/xcodebuild}" ] || return 1
}

oxid_ios_preflight() {
  local developer_dir="$1" runtime_id="$2" device_type_id="$3" runtimes device_types
  [ "${OXID_IOS_KEEP_FAILED:-0}" = 0 ] || return 1
  oxid_ios_validate_selectors "$developer_dir" "$runtime_id" "$device_type_id" || return 1
  oxid_ios_xcodebuild "$developer_dir" -version >/dev/null 2>&1 || return 1
  runtimes="$(oxid_ios_xcrun "$developer_dir" simctl list runtimes -j)" || return 1
  jq -e --arg runtime "$runtime_id" '
    [.runtimes[] | select(.identifier == $runtime and .isAvailable == true)] | length == 1
  ' <<<"$runtimes" >/dev/null || return 1
  device_types="$(oxid_ios_xcrun "$developer_dir" simctl list devicetypes -j)" || return 1
  jq -e --arg deviceType "$device_type_id" '
    [.devicetypes[] | select(.identifier == $deviceType and (.name | startswith("iPhone")))] | length == 1
  ' <<<"$device_types" >/dev/null || return 1
}

oxid_ios_create_owned() {
  local developer_dir="$1" runtime_id="$2" device_type_id="$3" name="$4" receipt="$5"
  local udid candidate
  oxid_ios_preflight "$developer_dir" "$runtime_id" "$device_type_id" || return 1
  [[ "$name" =~ ^[A-Za-z0-9._-]{1,80}$ ]] || return 1
  [[ "$receipt" = /* ]] || return 1
  [ ! -e "$receipt" ] && [ ! -L "$receipt" ] || return 1
  [ -d "${receipt%/*}" ] && [ ! -L "${receipt%/*}" ] || return 1
  udid="$(oxid_ios_xcrun "$developer_dir" simctl create "$name" "$device_type_id" "$runtime_id")" || return 1
  udid="${udid//$'\r'/}"
  udid="${udid//$'\n'/}"
  if ! [[ "$udid" =~ ^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$ ]]; then
    return 1
  fi
  candidate="$(mktemp "${receipt%/*}/.ios-receipt.XXXXXX")" || return 1
  chmod 600 "$candidate" || { rm -f -- "$candidate"; return 1; }
  if ! jq -cn --arg developer "$developer_dir" --arg runtime "$runtime_id" \
    --arg deviceType "$device_type_id" --arg name "$name" --arg udid "$udid" \
    '{schema:"oxid-ios-simulator-owner-receipt-v1",developerDirectory:$developer,
      runtimeIdentifier:$runtime,deviceTypeIdentifier:$deviceType,name:$name,udid:$udid}' \
    >"$candidate"; then
    rm -f -- "$candidate"
    return 1
  fi
  if ! ln "$candidate" "$receipt" 2>/dev/null; then
    rm -f -- "$candidate"
    return 1
  fi
  rm -f -- "$candidate"
  printf '%s\n' "$udid"
}

oxid_ios_receipt_values() {
  local receipt="$1"
  oxid_ios_receipt_mode_is_private "$receipt" || return 1
  jq -er '
    if (keys | sort) != (["developerDirectory","deviceTypeIdentifier","name","runtimeIdentifier","schema","udid"] | sort)
      or .schema != "oxid-ios-simulator-owner-receipt-v1"
      or (.developerDirectory | type) != "string"
      or (.runtimeIdentifier | type) != "string"
      or (.deviceTypeIdentifier | type) != "string"
      or (.name | type) != "string"
      or (.udid | type) != "string"
    then error("invalid receipt")
    else [.developerDirectory,.runtimeIdentifier,.deviceTypeIdentifier,.name,.udid] | @tsv
    end
  ' "$receipt"
}

oxid_ios_receipt_matches_simulator() {
  local developer_dir="$1" receipt="$2" values receipt_developer runtime_id device_type_id name udid devices
  values="$(oxid_ios_receipt_values "$receipt")" || return 1
  IFS=$'\t' read -r receipt_developer runtime_id device_type_id name udid <<<"$values"
  [ "$receipt_developer" = "$developer_dir" ] || return 1
  oxid_ios_validate_selectors "$developer_dir" "$runtime_id" "$device_type_id" || return 1
  devices="$(oxid_ios_xcrun "$developer_dir" simctl list devices -j)" || return 1
  jq -e --arg runtime "$runtime_id" --arg deviceType "$device_type_id" \
    --arg name "$name" --arg udid "$udid" '
      (.devices[$runtime] // [])
      | [.[] | select(.udid == $udid and .name == $name
          and .deviceTypeIdentifier == $deviceType and .isAvailable == true)]
      | length == 1
    ' <<<"$devices" >/dev/null
}

oxid_ios_owned_simctl() {
  local developer_dir="$1" receipt="$2" operation="$3"
  shift 3
  local values receipt_developer runtime_id device_type_id name udid
  case "$operation" in
    boot|bootstatus|install|terminate|launch|openurl|get_app_container|spawn|uninstall) ;;
    *) return 1 ;;
  esac
  oxid_ios_receipt_matches_simulator "$developer_dir" "$receipt" || return 1
  values="$(oxid_ios_receipt_values "$receipt")" || return 1
  IFS=$'\t' read -r receipt_developer runtime_id device_type_id name udid <<<"$values"
  oxid_ios_xcrun "$developer_dir" simctl "$operation" "$udid" "$@"
}

oxid_ios_delete_owned() {
  local developer_dir="$1" receipt="$2" values receipt_developer runtime_id device_type_id name udid
  local devices state receipt_identity
  receipt_identity="$(oxid_ios_filesystem_identity "$receipt")" || return 1
  oxid_ios_receipt_matches_simulator "$developer_dir" "$receipt" || return 1
  values="$(oxid_ios_receipt_values "$receipt")" || return 1
  IFS=$'\t' read -r receipt_developer runtime_id device_type_id name udid <<<"$values"
  devices="$(oxid_ios_xcrun "$developer_dir" simctl list devices -j)" || return 1
  state="$(jq -er --arg runtime "$runtime_id" --arg udid "$udid" '
    first(.devices[$runtime][] | select(.udid == $udid) | .state)
  ' <<<"$devices")" || return 1
  [ "$(oxid_ios_filesystem_identity "$receipt")" = "$receipt_identity" ] || return 1
  if [ "$state" != Shutdown ]; then
    oxid_ios_xcrun "$developer_dir" simctl shutdown "$udid" || return 1
  fi
  oxid_ios_receipt_matches_simulator "$developer_dir" "$receipt" || return 1
  [ "$(oxid_ios_filesystem_identity "$receipt")" = "$receipt_identity" ] || return 1
  oxid_ios_xcrun "$developer_dir" simctl delete "$udid" || return 1
  devices="$(oxid_ios_xcrun "$developer_dir" simctl list devices -j)" || return 1
  if jq -e --arg udid "$udid" '[.devices[][] | select(.udid == $udid)] | length > 0' \
    <<<"$devices" >/dev/null; then
    return 1
  fi
  [ "$(oxid_ios_filesystem_identity "$receipt")" = "$receipt_identity" ] || return 1
  rm -f -- "$receipt"
}
