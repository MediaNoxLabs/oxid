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

oxid_android_avd_definition_exists() {
  local candidate="$1"
  [[ "$candidate" =~ ^[A-Za-z0-9._-]+$ ]] || return 1
  for avd_ini in "${ANDROID_AVD_HOME:-}/$candidate.ini" "${ANDROID_SDK_HOME:-}/avd/$candidate.ini" "$HOME/.android/avd/$candidate.ini"; do
    if [ -f "$avd_ini" ] && [ ! -L "$avd_ini" ]; then return 0; fi
  done
  return 1
}

oxid_android_discover_avd() {
  local emulator="$1" candidate
  [ -x "$emulator" ] || return 1
  while IFS= read -r candidate; do
    [ -n "$candidate" ] || continue
    if oxid_android_avd_definition_exists "$candidate"; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done < <("$emulator" -list-avds | LC_ALL=C sort -u)
  return 1
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
      # A scoped `adb -s <serial> reverse --list` may omit the already-bound
      # serial or report its private host transport label, while an unscoped
      # listing includes the serial. The caller scopes every query to the
      # expected serial, so accept only these exact shapes and one consistent
      # host label per snapshot.
      NF == 2 { serial = expected_serial; local = $1; remote = $2 }
      NF == 3 { serial = $1; local = $2; remote = $3 }
      NF != 2 && NF != 3 { invalid = 1; next }
      (local in managed) || (remote in managed) {
        if (serial != expected_serial) {
          if (serial !~ /^host-[1-9][0-9]*$/ || (host_label && serial != host_label)) invalid = 1
          host_label = serial
        }
        if (!(local in managed) || local != remote || ++seen[local] != 1) invalid = 1
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

oxid_emulator_process_start_identity() {
  local pid="$1" start
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  start="$(oxid_process_ps -p "$pid" -o lstart= 2>/dev/null)" || return 1
  start="$(timeout -k 1s "${OXID_PROCESS_PS_TIMEOUT_SECONDS:-5}s" awk '{$1=$1; print}' <<<"$start")" || return 1
  [ -n "$start" ] || return 1
  printf '%s\n' "$start"
}

oxid_emulator_process_is_live() {
  local pid="$1" state
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  state="$(oxid_process_ps -p "$pid" -o stat= 2>/dev/null)" || return 1
  [[ "$state" != Z* ]]
}

oxid_find_unique_emulator_process() {
  local executable="$1" avd="$2" port="$3" snapshot pid command_line found=""
  snapshot="$(oxid_process_ps -axo pid= -o command= 2>/dev/null)" || return 1
  while read -r pid command_line; do
    [[ "$pid" =~ ^[1-9][0-9]*$ ]] || continue
    case " $command_line " in
      *" -avd $avd "*" -port $port "*) ;;
      *) continue ;;
    esac
    if oxid_emulator_command_matches "$command_line" "$executable" "$avd" "$port"; then
      [ -z "$found" ] || return 1
      found="$pid"
    fi
  done <<<"$snapshot"
  [ -n "$found" ] || return 1
  printf '%s\n' "$found"
}

oxid_emulator_owner_receipt_write() {
  local receipt="$1" launch_pid="$2" current_pid="$3" current_start="$4"
  local executable="$5" avd="$6" port="$7" executable_identity="$8" temporary
  [[ "$launch_pid" =~ ^[1-9][0-9]*$ && "$current_pid" =~ ^[1-9][0-9]*$ ]] || return 1
  [[ "$avd" =~ ^[A-Za-z0-9._-]+$ && "$port" =~ ^[1-9][0-9]{0,4}$ ]] || return 1
  [[ "$executable_identity" =~ ^[0-9]+:[0-9]+$ && "$current_start" != *$'\t'* && -n "$current_start" ]] || return 1
  [ -e "$executable" ] && [ ! -L "$executable" ] || return 1
  temporary="$receipt.tmp.${BASHPID:-$$}"
  [ ! -e "$temporary" ] && [ ! -L "$temporary" ] || return 1
  umask 077
  printf 'android-emulator-owner-v1\nlaunch_pid\t%s\ncurrent_pid\t%s\ncurrent_start\t%s\nexecutable_identity\t%s\navd\t%s\nport\t%s\n' \
    "$launch_pid" "$current_pid" "$current_start" "$executable_identity" "$avd" "$port" >"$temporary" || return 1
  chmod 600 "$temporary" || { rm -f -- "$temporary"; return 1; }
  mv "$temporary" "$receipt"
}

oxid_emulator_owner_receipt_read() {
  local receipt="$1" mode key value count=0
  OXID_EMULATOR_RECEIPT_SCHEMA=""
  OXID_EMULATOR_RECEIPT_LAUNCH_PID=""
  OXID_EMULATOR_RECEIPT_CURRENT_PID=""
  OXID_EMULATOR_RECEIPT_CURRENT_START=""
  OXID_EMULATOR_RECEIPT_EXECUTABLE_IDENTITY=""
  OXID_EMULATOR_RECEIPT_AVD=""
  OXID_EMULATOR_RECEIPT_PORT=""
  [ -f "$receipt" ] && [ ! -L "$receipt" ] || return 1
  if mode="$(stat -c '%a' "$receipt" 2>/dev/null)"; then :; else mode="$(stat -f '%Lp' "$receipt")" || return 1; fi
  [ "$mode" = 600 ] || return 1
  IFS= read -r OXID_EMULATOR_RECEIPT_SCHEMA <"$receipt" || return 1
  [ "$OXID_EMULATOR_RECEIPT_SCHEMA" = android-emulator-owner-v1 ] || return 1
  while IFS=$'\t' read -r key value; do
    case "$key" in
      launch_pid) [ -z "$OXID_EMULATOR_RECEIPT_LAUNCH_PID" ] || return 1; OXID_EMULATOR_RECEIPT_LAUNCH_PID="$value" ;;
      current_pid) [ -z "$OXID_EMULATOR_RECEIPT_CURRENT_PID" ] || return 1; OXID_EMULATOR_RECEIPT_CURRENT_PID="$value" ;;
      current_start) [ -z "$OXID_EMULATOR_RECEIPT_CURRENT_START" ] || return 1; OXID_EMULATOR_RECEIPT_CURRENT_START="$value" ;;
      executable_identity) [ -z "$OXID_EMULATOR_RECEIPT_EXECUTABLE_IDENTITY" ] || return 1; OXID_EMULATOR_RECEIPT_EXECUTABLE_IDENTITY="$value" ;;
      avd) [ -z "$OXID_EMULATOR_RECEIPT_AVD" ] || return 1; OXID_EMULATOR_RECEIPT_AVD="$value" ;;
      port) [ -z "$OXID_EMULATOR_RECEIPT_PORT" ] || return 1; OXID_EMULATOR_RECEIPT_PORT="$value" ;;
      *) return 1 ;;
    esac
    count=$((count + 1))
  done < <(tail -n +2 "$receipt")
  [[ "$count" -eq 6 && "$OXID_EMULATOR_RECEIPT_LAUNCH_PID" =~ ^[1-9][0-9]*$ \
    && "$OXID_EMULATOR_RECEIPT_CURRENT_PID" =~ ^[1-9][0-9]*$ \
    && "$OXID_EMULATOR_RECEIPT_EXECUTABLE_IDENTITY" =~ ^[0-9]+:[0-9]+$ \
    && "$OXID_EMULATOR_RECEIPT_AVD" =~ ^[A-Za-z0-9._-]+$ \
    && "$OXID_EMULATOR_RECEIPT_PORT" =~ ^[1-9][0-9]{0,4}$ \
    && -n "$OXID_EMULATOR_RECEIPT_CURRENT_START" ]]
}

oxid_emulator_owner_receipt_matches() {
  local receipt="$1" launch_pid="$2" executable="$3" avd="$4" port="$5"
  local executable_identity start snapshot _parent _comm command_line
  oxid_emulator_owner_receipt_read "$receipt" || return 1
  [ "$OXID_EMULATOR_RECEIPT_LAUNCH_PID" = "$launch_pid" \
    ] && [ "$OXID_EMULATOR_RECEIPT_AVD" = "$avd" \
    ] && [ "$OXID_EMULATOR_RECEIPT_PORT" = "$port" ] || return 1
  executable_identity="$(oxid_filesystem_identity "$executable")" || return 1
  [ "$OXID_EMULATOR_RECEIPT_EXECUTABLE_IDENTITY" = "$executable_identity" ] || return 1
  oxid_emulator_process_is_live "$OXID_EMULATOR_RECEIPT_CURRENT_PID" || return 1
  start="$(oxid_emulator_process_start_identity "$OXID_EMULATOR_RECEIPT_CURRENT_PID")" || return 1
  [ "$start" = "$OXID_EMULATOR_RECEIPT_CURRENT_START" ] || return 1
  snapshot="$(oxid_direct_child_snapshot "$OXID_EMULATOR_RECEIPT_CURRENT_PID")" || return 1
  read -r _parent _comm command_line <<<"$snapshot"
  oxid_emulator_command_matches "$command_line" "$executable" "$avd" "$port"
}

oxid_emulator_owner_receipt_create() {
  local receipt="$1" launch_pid="$2" expected_parent="$3" executable="$4" avd="$5" port="$6"
  local current_pid current_start executable_identity
  [ ! -e "$receipt" ] && [ ! -L "$receipt" ] || return 1
  oxid_emulator_job_owned "$launch_pid" "$expected_parent" "$executable" "$avd" "$port" || return 1
  current_pid="$launch_pid"
  current_start="$(oxid_emulator_process_start_identity "$current_pid")" || return 1
  executable_identity="$(oxid_filesystem_identity "$executable")" || return 1
  oxid_emulator_owner_receipt_write "$receipt" "$launch_pid" "$current_pid" "$current_start" \
    "$executable" "$avd" "$port" "$executable_identity"
}

oxid_emulator_owner_receipt_refresh() {
  local receipt="$1" launch_pid="$2" executable="$3" avd="$4" port="$5"
  local previous_pid current_pid current_start executable_identity
  oxid_emulator_owner_receipt_matches "$receipt" "$launch_pid" "$executable" "$avd" "$port" && return 0
  oxid_emulator_owner_receipt_read "$receipt" || return 1
  [ "$OXID_EMULATOR_RECEIPT_LAUNCH_PID" = "$launch_pid" \
    ] && [ "$OXID_EMULATOR_RECEIPT_AVD" = "$avd" \
    ] && [ "$OXID_EMULATOR_RECEIPT_PORT" = "$port" ] || return 1
  executable_identity="$(oxid_filesystem_identity "$executable")" || return 1
  [ "$OXID_EMULATOR_RECEIPT_EXECUTABLE_IDENTITY" = "$executable_identity" ] || return 1
  previous_pid="$OXID_EMULATOR_RECEIPT_CURRENT_PID"
  oxid_emulator_process_is_live "$previous_pid" && return 1
  current_pid="$(oxid_find_unique_emulator_process "$executable" "$avd" "$port")" || return 1
  [ "$current_pid" != "$previous_pid" ] || return 1
  current_start="$(oxid_emulator_process_start_identity "$current_pid")" || return 1
  oxid_emulator_owner_receipt_write "$receipt" "$launch_pid" "$current_pid" "$current_start" \
    "$executable" "$avd" "$port" "$executable_identity"
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
  local pgid="$1" pids status=0 pid state
  [[ "$pgid" =~ ^[1-9][0-9]*$ ]] || return 0
  command -v pgrep >/dev/null 2>&1 || return 0
  pids="$(timeout -k 1s "${OXID_PROCESS_PS_TIMEOUT_SECONDS:-5}s" pgrep -g "$pgid" 2>/dev/null)" \
    || status=$?
  case "$status" in
    0) ;;
    1) return 1 ;;
    *) return 0 ;;
  esac
  while IFS= read -r pid; do
    [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 0
    state="$(oxid_process_ps -p "$pid" -o stat= 2>/dev/null)" || continue
    [[ "$state" = Z* ]] || return 0
  done <<<"$pids"
  return 1
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

oxid_terminate_emulator_receipt() {
  local receipt="$1" launch_pid="$2" executable="$3" avd="$4" port="$5"
  local current_pid attempts generation candidate status=0
  for generation in 1 2; do
    oxid_emulator_owner_receipt_refresh "$receipt" "$launch_pid" "$executable" "$avd" "$port" || return 2
    oxid_emulator_owner_receipt_read "$receipt" || return 2
    current_pid="$OXID_EMULATOR_RECEIPT_CURRENT_PID"
    kill -TERM "$current_pid" 2>/dev/null || return 1
    for ((attempts = 0; attempts < 200; attempts++)); do
      oxid_emulator_process_is_live "$current_pid" || break
      timeout -k 1s 2s sleep 0.1 || return 1
    done
    if oxid_emulator_process_is_live "$current_pid"; then
      oxid_emulator_owner_receipt_matches "$receipt" "$launch_pid" "$executable" "$avd" "$port" || return 1
      kill -KILL "$current_pid" 2>/dev/null || return 1
      for ((attempts = 0; attempts < 50; attempts++)); do
        oxid_emulator_process_is_live "$current_pid" || break
        timeout -k 1s 2s sleep 0.1 || return 1
      done
      oxid_emulator_process_is_live "$current_pid" && return 1
    fi
    candidate="$(oxid_find_unique_emulator_process "$executable" "$avd" "$port" 2>/dev/null || true)"
    [ -n "$candidate" ] || break
    [ "$generation" -lt 2 ] || return 1
  done
  if oxid_job_is_running "$launch_pid"; then
    wait "$launch_pid" 2>/dev/null || status=$?
    case "$status" in 0|137|143) ;; *) return 1 ;; esac
  else
    wait "$launch_pid" 2>/dev/null || true
  fi
  oxid_emulator_owner_receipt_read "$receipt" || return 1
  rm -f -- "$receipt"
  [ ! -e "$receipt" ]
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
