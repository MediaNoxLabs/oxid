#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Receipt-scoped Tailnet HTTPS lifecycle for the fixed standalone faucet.
set -euo pipefail
export LC_ALL=C

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
state="$root/target/standalone-faucet-tailnet"
receipt="$state/receipt.json"
svg="$state/setup.svg"
log="$state/faucet.log"
pid_file="$state/faucet.pid"
profile_store="$state/profiles.json"
mode="${1:-}"

fail() { printf 'standalone-faucet-tailnet: FAIL phase=%s\n' "$1" >&2; exit 1; }
private_file() {
  local permissions
  [ -f "$1" ] && [ ! -L "$1" ] || return 1
  permissions="$(stat -f '%Lp' "$1" 2>/dev/null)" || permissions="$(stat -c '%a' "$1" 2>/dev/null)" || return 1
  [ "$permissions" = 600 ]
}
canonical_serve() { tailscale serve status --json | jq -S -c '.'; }
process_matches() {
  local pid="$1" expected="$2" actual
  kill -0 "$pid" 2>/dev/null || return 1
  actual="$(ps -p "$pid" -o command= 2>/dev/null | shasum -a 256 | awk '{print $1}')" || return 1
  [ "$actual" = "$expected" ]
}
process_has_exited() {
  local pid="$1" state
  if ! kill -0 "$pid" 2>/dev/null; then return 0; fi
  state="$(ps -p "$pid" -o stat= 2>/dev/null || true)"
  [[ "$state" == Z* ]]
}
stop_owned_process() {
  local pid="$1"
  kill -TERM "$pid" >/dev/null 2>&1 || true
  for _ in $(seq 1 30); do
    process_has_exited "$pid" && return 0
    sleep 1
  done
  return 1
}
remove_owned_state() {
  rm -f -- "$receipt" "$svg" "$log" "$pid_file" "$profile_store"
  rmdir -- "$state"
}

for command in cargo curl grep jq node tailscale qrencode shasum; do command -v "$command" >/dev/null 2>&1 || fail "missing-${command}"; done
case "$mode" in start|status|stop|accept) ;; *) fail usage ;; esac

load_receipt() {
  [ -d "$state" ] && [ ! -L "$state" ] && private_file "$receipt" && private_file "$svg" && private_file "$pid_file" || return 1
  jq -e ' .schema == "oxid-standalone-faucet-tailnet-v1"
    and (.port | type == "number" and . >= 11000 and . <= 11999)
    and (.dnsName | type == "string" and test("^[A-Za-z0-9][A-Za-z0-9.-]*$"))
    and (.baseline | type == "string") and (.active | type == "string")
    and (.faucet.pid | type == "number" and . > 1)
    and (.faucet.commandSha256 | test("^[0-9a-f]{64}$"))' "$receipt" >/dev/null
}

case "$mode" in
start)
  [ ! -e "$state" ] && [ ! -L "$state" ] || fail session-exists
  [ "$(tailscale status --json | jq -r '.BackendState')" = Running ] || fail tailscale-offline
  baseline="$(canonical_serve)" || fail serve-baseline
  dns="$(tailscale status --json | jq -r '.Self.DNSName | rtrimstr(".")')"
  [[ "$dns" =~ ^[A-Za-z0-9][A-Za-z0-9.-]*$ ]] || fail magicdns
  port=""
  for candidate in $(seq 11000 11999); do
    if jq -e --arg port "$candidate" --arg host "$dns:$candidate" '(.TCP[$port] == null) and (.Web[$host] == null)' <<<"$baseline" >/dev/null; then port="$candidate"; break; fi
  done
  [ -n "$port" ] || fail route-unavailable
  faucet_pid=""
  serve_configured=0
  start_cleanup() {
    local incoming="$1" cleanup_status=0 after=""
    trap - EXIT INT TERM HUP
    if [ "$serve_configured" -eq 1 ]; then
      tailscale serve --yes --https="$port" off >/dev/null 2>&1 || cleanup_status=1
      after="$(canonical_serve 2>/dev/null)" || cleanup_status=1
      [ "$after" = "$baseline" ] || cleanup_status=1
    fi
    if [ -n "$faucet_pid" ]; then
      stop_owned_process "$faucet_pid" || cleanup_status=1
    fi
    if [ -d "$state" ] && [ ! -L "$state" ]; then
      remove_owned_state || cleanup_status=1
    elif [ -e "$state" ] || [ -L "$state" ]; then
      cleanup_status=1
    fi
    [ "$cleanup_status" -eq 0 ] || printf '%s\n' 'standalone-faucet-tailnet: exact cleanup could not be proven' >&2
    exit "$incoming"
  }
  trap 'start_cleanup $?' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  trap 'exit 129' HUP
  umask 077; mkdir -p "$state"; chmod 700 "$state"
  # A normal phone camera can open this private Tailnet HTTPS page directly.
  # The page itself presents the fixed realm and grant; the QR carries no
  # wallet, recipient, key, or configurable funding policy.
  payload="https://$dns:$port/"
  qrencode --type=SVG --output="$svg" "$payload" || fail qr
  chmod 600 "$svg"
  "$root/scripts/standalone-status.sh" local >/dev/null || fail standalone
  : >"$log"; chmod 600 "$log"
  build_json="$(cargo build -p oxid-headless --features standalone-faucet \
    --bin oxid-standalone-faucet-http --message-format=json-render-diagnostics 2>>"$log")" || fail faucet-build
  executable="$(jq -r 'select(.reason == "compiler-artifact" and .target.name == "oxid-standalone-faucet-http") | .executable // empty' <<<"$build_json" | tail -n 1)"
  [ -n "$executable" ] && [ -x "$executable" ] || fail faucet-binary
  faucet_pid="$(env -i PATH="$PATH" HOME="$HOME" TMPDIR="${TMPDIR:-/tmp}" \
    OXID_ENABLE_STANDALONE_FAUCET=1 OXID_PROFILE_STORE_PATH="$profile_store" \
    OXID_STANDALONE_FAUCET_SETUP_SVG_PATH="$svg" \
    node "$root/scripts/lib/spawn-detached.mjs" "$executable" "$log")" || fail faucet-launch
  [[ "$faucet_pid" =~ ^[0-9]+$ ]] && [ "$faucet_pid" -gt 1 ] || fail faucet-launch
  printf '%s\n' "$faucet_pid" >"$pid_file"; chmod 600 "$pid_file"
  faucet_ready=0
  for _ in $(seq 1 30); do
    kill -0 "$faucet_pid" 2>/dev/null || fail faucet-process
    if grep -Fqx 'Standalone faucet HTTP ready on 127.0.0.1:36301; loopback only.' "$log" \
      && curl --noproxy '*' --silent --fail --max-time 1 http://127.0.0.1:36301/health >/dev/null; then
      faucet_ready=1
      break
    fi
    sleep 1
  done
  [ "$faucet_ready" -eq 1 ] && kill -0 "$faucet_pid" 2>/dev/null || fail faucet
  # A new port is absent from the baseline, so this invocation owns no route
  # that existed before this receipt. It never uses Serve reset or Funnel.
  tailscale serve --yes --bg --https="$port" http://127.0.0.1:36301 >/dev/null || fail serve-api
  serve_configured=1
  active="$(canonical_serve)" || fail serve-active
  command_sha="$(ps -p "$faucet_pid" -o command= | shasum -a 256 | awk '{print $1}')"
  jq -cn --arg baseline "$baseline" --arg active "$active" --arg dns "$dns" --argjson port "$port" --argjson pid "$faucet_pid" --arg command "$command_sha" \
    '{schema:"oxid-standalone-faucet-tailnet-v1",baseline:$baseline,active:$active,dnsName:$dns,port:$port,faucet:{pid:$pid,commandSha256:$command}}' >"$receipt"
  chmod 600 "$receipt"
  trap - EXIT INT TERM HUP
  printf '%s\n' 'standalone-faucet-tailnet: READY (private Tailnet HTTPS route configured)'
  ;;
status)
  load_receipt || fail receipt
  active="$(canonical_serve)" || fail serve-status
  [ "$active" = "$(jq -r '.active' "$receipt")" ] || fail serve-drift
  process_matches "$(jq -r '.faucet.pid' "$receipt")" "$(jq -r '.faucet.commandSha256' "$receipt")" || fail faucet-process
  curl --noproxy '*' --silent --fail --max-time 3 http://127.0.0.1:36301/health >/dev/null || fail faucet-health
  printf '%s\n' 'standalone-faucet-tailnet: READY'
  ;;
stop)
  load_receipt || fail receipt
  active="$(canonical_serve)" || fail serve-status
  # Refuse to alter Serve after any drift: this proves the selected port and all
  # unrelated baseline routes still match the receipt before removal.
  [ "$active" = "$(jq -r '.active' "$receipt")" ] || fail serve-drift
  pid="$(jq -r '.faucet.pid' "$receipt")"; command_sha="$(jq -r '.faucet.commandSha256' "$receipt")"
  process_alive=0
  if kill -0 "$pid" 2>/dev/null; then
    process_matches "$pid" "$command_sha" || fail faucet-process
    process_alive=1
  fi
  port="$(jq -r '.port' "$receipt")"
  tailscale serve --yes --https="$port" off >/dev/null || fail serve-remove
  [ "$(canonical_serve)" = "$(jq -r '.baseline' "$receipt")" ] || fail serve-restore
  if [ "$process_alive" -eq 1 ]; then stop_owned_process "$pid" || fail faucet-stop; fi
  remove_owned_state || fail state-cleanup
  printf '%s\n' 'standalone-faucet-tailnet: STOPPED'
  ;;
accept)
  [ "${OXID_ENABLE_OWNER_TAILNET_FAUCET_ACCEPTANCE:-}" = 1 ] || fail owner-authorization-required
  [[ "${OXID_FAUCET_RECIPIENT_ADDRESS:-}" =~ ^mn_addr_undeployed ]] || fail undeployed-recipient-required
  load_receipt || fail receipt
  active="$(canonical_serve)"; [ "$active" = "$(jq -r '.active' "$receipt")" ] || fail serve-drift
  dns="$(jq -r '.dnsName' "$receipt")"; port="$(jq -r '.port' "$receipt")"
  curl --noproxy '*' --silent --fail --max-time 10 "https://$dns:$port/health" >/dev/null || fail https-health
  curl --noproxy '*' --silent --fail --max-time 120 -H 'content-type: application/json' \
    --data "$(jq -cn --arg address "$OXID_FAUCET_RECIPIENT_ADDRESS" '{requestId:"owner-tailnet-acceptance",recipientAddress:$address}')" \
    "https://$dns:$port/fund" | jq -e '.ok == true and .result.receipt.amount.atomicUnits == "50000000000"' >/dev/null || fail https-funding
  printf '%s\n' 'standalone-faucet-tailnet: ACCEPTED (owner-authorized HTTPS health and fixed grant)'
  ;;
esac
