#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

oxid_process_ps() {
  local deadline="${OXID_PROCESS_PS_TIMEOUT_SECONDS:-5}"
  timeout -k 1s "${deadline}s" ps "$@"
}

oxid_adb_inventory_is_empty() {
  local inventory="$1"
  timeout -k 1s "${OXID_ADB_INVENTORY_PARSE_TIMEOUT_SECONDS:-5}s" awk '
    NR == 1 { if ($0 != "List of devices attached") exit 2; next }
    NF > 0 { found=1 }
    END { exit found ? 1 : 0 }
  ' <<<"$inventory"
}

oxid_adb_inventory_is_exact_online() {
  local inventory="$1" expected_serial="$2"
  [[ "$expected_serial" =~ ^emulator-[0-9]+$ ]] || return 1
  timeout -k 1s "${OXID_ADB_INVENTORY_PARSE_TIMEOUT_SECONDS:-5}s" \
    awk -v expected="$expected_serial" '
      NR == 1 { if ($0 != "List of devices attached") exit 2; next }
      NF > 0 {
        count++
        if ($1 != expected || $2 != "device") invalid=1
      }
      END { exit !(count == 1 && !invalid) }
    ' <<<"$inventory"
}

oxid_adb_inventory_snapshot() {
  local adb="$1" deadline="${OXID_ADB_INVENTORY_TIMEOUT_SECONDS:-15}"
  [ -x "$adb" ] || return 1
  timeout -k 2s "${deadline}s" env -u ANDROID_SERIAL "$adb" devices -l
}

oxid_require_empty_adb_inventory() {
  local adb="$1" inventory
  inventory="$(oxid_adb_inventory_snapshot "$adb")" || return 1
  oxid_adb_inventory_is_empty "$inventory"
}

# ADB reverse has no owner metadata. These parsers therefore make ownership
# explicit: a managed route may be absent, or exactly one route on the expected
# serial with equal local and remote TCP ports. Any other use of a managed port
# is ambiguous and must be preserved rather than removed.
oxid_adb_reverse_snapshot_managed_routes_are_exact_or_absent() {
  local snapshot="$1" serial="$2" ports
  shift 2
  [[ "$serial" =~ ^emulator-[0-9]+$ ]] || return 1
  [ "$#" -gt 0 ] || return 1
  ports=""
  for port in "$@"; do
    [[ "$port" =~ ^[1-9][0-9]{0,4}$ ]] && [ "$port" -le 65535 ] || return 1
    case " $ports " in *" $port "*) return 1 ;; esac
    ports+="${ports:+ }$port"
  done
  timeout -k 1s "${OXID_ADB_REVERSE_PARSE_TIMEOUT_SECONDS:-5}s" \
    awk -v expected_serial="$serial" -v ports="$ports" '
      BEGIN {
        split(ports, values, " ")
        for (i in values) managed["tcp:" values[i]] = 1
      }
      # ADB may delimit reverse-list records with CRLF; normalize only the
      # record terminator before applying the exact three-field contract.
      { sub(/\r$/, "") }
      NF == 0 { next }
      NF != 3 { invalid = 1; next }
      ($2 in managed) || ($3 in managed) {
        if ($1 != expected_serial || !($2 in managed) || $2 != $3 || ++seen[$2] != 1) invalid = 1
      }
      END { exit invalid ? 1 : 0 }
    ' <<<"$snapshot"
}

oxid_adb_reverse_snapshot_has_no_managed_routes() {
  local snapshot="$1" serial="$2" ports
  shift 2
  oxid_adb_reverse_snapshot_managed_routes_are_exact_or_absent "$snapshot" "$serial" "$@" || return 1
  ports="$*"
  timeout -k 1s "${OXID_ADB_REVERSE_PARSE_TIMEOUT_SECONDS:-5}s" \
    awk -v ports="$ports" '
      BEGIN {
        split(ports, values, " ")
        for (i in values) managed["tcp:" values[i]] = 1
      }
      ($2 in managed) || ($3 in managed) { found = 1 }
      END { exit found ? 1 : 0 }
    ' <<<"$snapshot"
}

oxid_adb_reverse_snapshot_has_exact_managed_routes() {
  local snapshot="$1" serial="$2" ports
  shift 2
  oxid_adb_reverse_snapshot_managed_routes_are_exact_or_absent "$snapshot" "$serial" "$@" || return 1
  ports="$*"
  timeout -k 1s "${OXID_ADB_REVERSE_PARSE_TIMEOUT_SECONDS:-5}s" \
    awk -v ports="$ports" '
      BEGIN {
        split(ports, values, " ")
        for (i in values) managed["tcp:" values[i]] = 1
      }
      $2 in managed { seen[$2]++ }
      END {
        for (route in managed) if (seen[route] != 1) exit 1
      }
    ' <<<"$snapshot"
}

oxid_epoch_seconds_are_close() {
  local host_epoch="$1" emulator_epoch="$2" tolerance="$3" delta
  [[ "$host_epoch" =~ ^[0-9]{10,11}$ && "$emulator_epoch" =~ ^[0-9]{10,11}$ \
    && "$tolerance" =~ ^[0-9]{1,4}$ ]] || return 1
  if [ "$host_epoch" -ge "$emulator_epoch" ]; then
    delta=$((host_epoch - emulator_epoch))
  else
    delta=$((emulator_epoch - host_epoch))
  fi
  [ "$delta" -le "$tolerance" ]
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
  local qemu_prefix="${executable%/*}/qemu/"
  timeout -k 1s "${OXID_PROCESS_PS_TIMEOUT_SECONDS:-5}s" \
    awk -v executable="$executable" -v qemu_prefix="$qemu_prefix" -v avd="$avd" -v port="$port" '
    {
      if ($1 != executable) {
        if (index($1, qemu_prefix) != 1) exit 1
        qemu_relative = substr($1, length(qemu_prefix) + 1)
        if (qemu_relative !~ /^[A-Za-z0-9._-]+\/qemu-system-[A-Za-z0-9._-]+$/) exit 1
      }
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
  local snapshot parent command_line
  oxid_job_is_running "$pid" || return 1
  snapshot="$(oxid_direct_child_snapshot "$pid")" || return 1
  # `comm` is process-controlled and is not a portable executable identity:
  # Node reports `MainThread` on Linux, and emulator launchers may rename their
  # main task. Ownership instead binds the live shell job, its direct parent,
  # and the exact executable/AVD/port/safety arguments from the command line.
  read -r parent _ command_line <<<"$snapshot"
  [ "$parent" = "$expected_parent" ] || return 1
  oxid_emulator_command_matches "$command_line" "$executable" "$avd" "$port"
}

oxid_poll_job_dead() {
  local pid="$1" attempts="$2"
  for ((_attempt = 0; _attempt < attempts; _attempt++)); do
    oxid_job_is_running "$pid" || return 0
    timeout -k 1s 2s sleep 0.1 || return 1
  done
  ! oxid_job_is_running "$pid"
}

oxid_process_group_is_live() {
  local pgid="$1" snapshot
  snapshot="$(oxid_process_ps -axo pgid=,stat= 2>/dev/null)" || return 0
  timeout -k 1s "${OXID_PROCESS_PS_TIMEOUT_SECONDS:-5}s" \
    awk -v pgid="$pgid" '$1 == pgid && $2 !~ /^Z/ { found=1 } END { exit !found }' <<<"$snapshot"
  case "$?" in
    0) return 0 ;;
    1) return 1 ;;
    *) return 0 ;;
  esac
}

oxid_poll_process_group_dead() {
  local pgid="$1" attempts="$2"
  for ((_attempt = 0; _attempt < attempts; _attempt++)); do
    oxid_process_group_is_live "$pgid" || return 0
    timeout -k 1s 2s sleep 0.1 || return 1
  done
  ! oxid_process_group_is_live "$pgid"
}

oxid_terminate_supervised_job() {
  local pid="$1" status=0
  oxid_job_is_running "$pid" || return 2
  kill -TERM -- "-$pid" 2>/dev/null || return 1
  if ! oxid_poll_process_group_dead "$pid" 50; then
    kill -KILL -- "-$pid" 2>/dev/null || return 1
    oxid_poll_process_group_dead "$pid" 50 || return 1
  fi
  oxid_job_is_running "$pid" && return 1
  wait "$pid" 2>/dev/null || status=$?
  case "$status" in
    0|124|137|143) return 0 ;;
    *) return 1 ;;
  esac
}

oxid_terminate_emulator_job() {
  local pid="$1" expected_parent="$2" executable="$3" avd="$4" port="$5" status=0
  oxid_emulator_job_owned "$pid" "$expected_parent" "$executable" "$avd" "$port" || return 2
  kill -TERM "$pid" 2>/dev/null || return 1
  if ! oxid_poll_job_dead "$pid" 200; then
    oxid_emulator_job_owned "$pid" "$expected_parent" "$executable" "$avd" "$port" || return 1
    kill -KILL "$pid" 2>/dev/null || return 1
    oxid_poll_job_dead "$pid" 50 || return 1
  fi
  oxid_job_is_running "$pid" && return 1
  wait "$pid" 2>/dev/null || status=$?
  case "$status" in
    0|137|143) return 0 ;;
    *) return 1 ;;
  esac
}

oxid_filesystem_identity() {
  local path="$1" deadline="${OXID_PROCESS_STAT_TIMEOUT_SECONDS:-5}" identity
  if identity="$(timeout -k 1s "${deadline}s" stat -c '%d:%i' -- "$path" 2>/dev/null)"; then
    printf '%s\n' "$identity"
    return 0
  fi
  timeout -k 1s "${deadline}s" stat -f '%d:%i' -- "$path" 2>/dev/null
}

oxid_path_has_identity() {
  local path="$1" expected="$2" actual
  [ -n "$expected" ] && [ -e "$path" ] && [ ! -L "$path" ] || return 1
  actual="$(oxid_filesystem_identity "$path")" || return 1
  [ "$actual" = "$expected" ]
}

oxid_android_avd_failure_marker_reset() {
  OXID_ANDROID_AVD_FAILURE_MARKER_EMITTED=0
}

oxid_android_avd_emit_failure_marker() {
  local status="$1" phase="${2:-unreported-timeout-or-abort}"
  [ "$status" -ne 0 ] || return 0
  [ "${OXID_ANDROID_AVD_FAILURE_MARKER_EMITTED:-0}" -eq 0 ] || return 0
  [[ "$phase" =~ ^[a-z0-9][a-z0-9-]{0,63}$ ]] || phase="unreported-timeout-or-abort"
  OXID_ANDROID_AVD_FAILURE_MARKER_EMITTED=1
  printf 'android-portal-exact-sequence-avd: FAIL phase=%s\n' "$phase" >&2
}
