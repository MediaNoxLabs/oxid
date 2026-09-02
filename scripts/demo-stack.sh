#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
state_directory="$repository_root/target/demo/tailnet-identity"
receipt="$state_directory/receipt.json"
operation="${1:-}"

case "$operation" in
  start|status|stop) ;;
  *)
    echo "Usage: $0 <start|status|stop>" >&2
    exit 1
    ;;
esac

for command_name in docker git jq just; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command '$command_name' is missing." >&2
    exit 1
  fi
done

file_mode() {
  if stat -c '%a' -- "$1" 2>/dev/null; then :; else stat -f '%Lp' -- "$1"; fi
}

load_receipt() {
  local head tree
  [ -d "$state_directory" ] && [ ! -L "$state_directory" ] \
    && [ "$(file_mode "$state_directory")" = 700 ] || return 1
  [ -f "$receipt" ] && [ ! -L "$receipt" ] \
    && [ "$(file_mode "$receipt")" = 600 ] || return 1
  head="$(git -C "$repository_root" rev-parse HEAD)"
  tree="$(git -C "$repository_root" rev-parse 'HEAD^{tree}')"
  jq -e --arg head "$head" --arg tree "$tree" '
    .schema == "oxid-tailnet-identity-demo-v1"
    and .oxid == {head:$head, tree:$tree}
    and (.standaloneOwned | type == "boolean")
  ' "$receipt" >/dev/null
}

write_receipt() {
  local standalone_owned="$1" candidate
  umask 077
  mkdir -p "$state_directory"
  chmod 700 "$state_directory"
  candidate="$(mktemp "$state_directory/.receipt.XXXXXX")"
  jq -cn \
    --arg head "$(git -C "$repository_root" rev-parse HEAD)" \
    --arg tree "$(git -C "$repository_root" rev-parse 'HEAD^{tree}')" \
    --argjson owned "$standalone_owned" '
      {schema:"oxid-tailnet-identity-demo-v1", oxid:{head:$head, tree:$tree}, standaloneOwned:$owned}
    ' >"$candidate"
  chmod 600 "$candidate"
  mv "$candidate" "$receipt"
}

query_standalone_containers() {
  docker ps -a \
    --filter label=com.docker.compose.project=oxid-standalone \
    --format '{{.ID}}'
}

start_demo() {
  local existing standalone_owned=false cleanup_needed=true
  if [ -n "$(git -C "$repository_root" status --porcelain)" ]; then
    echo "The demo requires a clean exact-head checkout." >&2
    exit 1
  fi
  [ ! -e "$state_directory" ] && [ ! -L "$state_directory" ] || {
    echo "A demo receipt already exists; run demo/status.sh or demo/stop.sh." >&2
    exit 1
  }
  if ! existing="$(query_standalone_containers)"; then
    echo "Could not determine standalone ownership; no stack command was run." >&2
    exit 1
  fi
  cleanup_start_failure() {
    local incoming=$?
    if [ "$cleanup_needed" = true ]; then
      if [ "$standalone_owned" = true ]; then
        just -f "$repository_root/Justfile" standalone-down >/dev/null 2>&1 || true
      fi
      rm -rf -- "$state_directory"
    fi
    exit "$incoming"
  }
  trap cleanup_start_failure EXIT
  if [ -z "$existing" ]; then
    just -f "$repository_root/Justfile" standalone-phone-up
    standalone_owned=true
  fi
  "$repository_root/scripts/standalone-status.sh" phone
  just -f "$repository_root/Justfile" android-phone
  write_receipt "$standalone_owned"
  cleanup_needed=false
  trap - EXIT
  echo "Oxid Tailnet identity demo: READY"
  echo "Use demo/status.sh to check it and demo/stop.sh for receipt-scoped cleanup."
}

status_demo() {
  load_receipt || {
    echo "The demo receipt is missing, stale, or unsafe." >&2
    exit 1
  }
  "$repository_root/scripts/standalone-status.sh" phone
  echo "Oxid Tailnet identity demo: READY"
}

stop_demo() {
  local standalone_owned
  load_receipt || {
    echo "The demo receipt is missing, stale, or unsafe; preserving all state." >&2
    exit 1
  }
  standalone_owned="$(jq -r '.standaloneOwned' "$receipt")"
  if [ "$standalone_owned" = true ]; then
    just -f "$repository_root/Justfile" standalone-down
  fi
  rm -f -- "$receipt"
  rmdir "$state_directory"
  echo "Oxid Tailnet identity demo: STOPPED"
  if [ "$standalone_owned" = false ]; then
    echo "The pre-existing standalone stack and its routes were left running."
  fi
}

case "$operation" in
  start) start_demo ;;
  status) status_demo ;;
  stop) stop_demo ;;
esac
