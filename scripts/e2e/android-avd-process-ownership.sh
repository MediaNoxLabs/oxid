#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

oxid_process_ps() {
  if [ -n "${OXID_PROCESS_TIMEOUT_COMMAND:-}" ]; then
    "$OXID_PROCESS_TIMEOUT_COMMAND" -k 1s 5s ps "$@"
  else
    ps "$@"
  fi
}

oxid_process_identity() {
  local pid="$1" birth executable command_line
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  birth="$(oxid_process_ps -p "$pid" -o lstart= 2>/dev/null)" || return 1
  executable="$(oxid_process_ps -p "$pid" -o comm= 2>/dev/null)" || return 1
  command_line="$(oxid_process_ps -p "$pid" -o command= 2>/dev/null)" || return 1
  [ -n "$birth" ] && [ -n "$executable" ] && [ -n "$command_line" ] || return 1
  printf '%s\034%s\034%s\034%s' "$pid" "$birth" "$executable" "$command_line"
}

oxid_same_process_identity() {
  [ -n "${1:-}" ] && [ "$1" = "${2:-}" ]
}

oxid_process_still_owned() {
  local pid="$1" expected="$2" current
  current="$(oxid_process_identity "$pid")" || return 1
  oxid_same_process_identity "$expected" "$current"
}

oxid_emulator_command_matches() {
  local command_line="$1" executable="$2" avd="$3" port="$4"
  awk -v executable="$executable" -v avd="$avd" -v port="$port" '
    {
      if ($1 != executable) exit 1
      avd_count = port_count = readonly_count = snapshot_count = snapshot_save_count = 0
      for (i = 2; i <= NF; i++) {
        if ($i == "-avd" && $(i + 1) == avd) avd_count++
        if ($i == "-port" && $(i + 1) == port) port_count++
        if ($i == "-read-only") readonly_count++
        if ($i == "-no-snapshot") snapshot_count++
        if ($i == "-no-snapshot-save") snapshot_save_count++
      }
      exit !(avd_count == 1 && port_count == 1 && readonly_count == 1 && snapshot_count == 1 && snapshot_save_count == 1)
    }
  ' <<<"$command_line"
}

oxid_identity_command() {
  local identity="$1"
  printf '%s' "${identity##*$'\034'}"
}

oxid_terminate_owned_process() {
  local pid="$1" identity="$2" grace_attempts="${3:-50}"
  oxid_process_still_owned "$pid" "$identity" || return 2
  kill -TERM "$pid" 2>/dev/null || return 1
  for ((_attempt = 0; _attempt < grace_attempts; _attempt++)); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.1
  done
  oxid_process_still_owned "$pid" "$identity" || return 2
  kill -KILL "$pid" 2>/dev/null || return 1
  for ((_attempt = 0; _attempt < 20; _attempt++)); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.1
  done
  return 1
}
