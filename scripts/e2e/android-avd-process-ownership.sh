#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

oxid_process_ps() {
  local deadline="${OXID_PROCESS_PS_TIMEOUT_SECONDS:-5}"
  timeout -k 1s "${deadline}s" ps "$@"
}

oxid_job_is_running() {
  local expected="$1" job
  while IFS= read -r job; do
    [ "$job" = "$expected" ] && return 0
  done < <(jobs -pr)
  return 1
}

oxid_direct_child_snapshot() {
  local pid="$1"
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  oxid_process_ps -p "$pid" -o ppid= -o comm= -o command= 2>/dev/null
}

oxid_direct_child_owned() {
  local pid="$1" expected_parent="$2" snapshot parent
  oxid_job_is_running "$pid" || return 1
  snapshot="$(oxid_direct_child_snapshot "$pid")" || return 1
  read -r parent _ <<<"$snapshot"
  [ "$parent" = "$expected_parent" ]
}

oxid_emulator_command_matches() {
  local command_line="$1" executable="$2" avd="$3" port="$4"
  timeout -k 1s "${OXID_PROCESS_PS_TIMEOUT_SECONDS:-5}s" \
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

oxid_emulator_job_owned() {
  local pid="$1" expected_parent="$2" executable="$3" avd="$4" port="$5"
  local snapshot parent comm command_line
  oxid_job_is_running "$pid" || return 1
  snapshot="$(oxid_direct_child_snapshot "$pid")" || return 1
  read -r parent comm command_line <<<"$snapshot"
  [ "$parent" = "$expected_parent" ] || return 1
  [ "$comm" = "${executable##*/}" ] || [ "$comm" = "$executable" ] || return 1
  oxid_emulator_command_matches "$command_line" "$executable" "$avd" "$port"
}

oxid_terminate_supervised_job() {
  local pid="$1" status=0
  oxid_job_is_running "$pid" || return 2
  kill -TERM -- "-$pid" 2>/dev/null || return 1
  for ((_attempt = 0; _attempt < 50; _attempt++)); do
    kill -0 -- "-$pid" 2>/dev/null || break
    timeout -k 1s 2s sleep 0.1
  done
  if kill -0 -- "-$pid" 2>/dev/null; then
    kill -KILL -- "-$pid" 2>/dev/null || return 1
  fi
  wait "$pid" 2>/dev/null || status=$?
  case "$status" in
    0|124|137|143) return 0 ;;
    *) return 1 ;;
  esac
}
