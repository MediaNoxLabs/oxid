#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Receipt-scoped Tailnet routes for the existing standalone stack.
set -euo pipefail
export LC_ALL=C

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
state="$root/target/standalone-tailnet-routes"
receipt="$state/receipt.json"
receipt_next="$state/receipt.next"
mode="${1:-}"

fail() { printf 'standalone-tailnet-routes: FAIL phase=%s\n' "$1" >&2; exit 1; }
private_file() {
  local mode
  [ -f "$1" ] && [ ! -L "$1" ] || return 1
  if stat -f '%Lp' "$1" >/dev/null 2>&1; then mode="$(stat -f '%Lp' "$1")"; else mode="$(stat -c '%a' "$1")"; fi
  [ "$mode" = 600 ]
}
canonical_serve() {
  tailscale serve status --json | jq -S -c '
    if .TCP == {} then del(.TCP) else . end
    | if .Web == {} then del(.Web) else . end
  '
}
remove_owned_state() { rm -f -- "$receipt" "$receipt_next"; rmdir -- "$state"; }
write_receipt_update() {
  chmod 600 "$receipt_next"
  mv -f -- "$receipt_next" "$receipt"
}
append_progress() {
  jq --arg active "$1" --argjson configured "$2" \
    '.active = $active | .configured = $configured | .states += [$active] | .pending = null | .removing = null' \
    "$receipt" >"$receipt_next" && write_receipt_update
}
mark_pending() {
  jq --argjson pending "$1" '.pending = $pending' \
    "$receipt" >"$receipt_next" && write_receipt_update
}
clear_pending() {
  jq '.pending = null' "$receipt" >"$receipt_next" && write_receipt_update
}
mark_removing() {
  jq --argjson removing "$1" '.removing = $removing' \
    "$receipt" >"$receipt_next" && write_receipt_update
}
clear_removing() {
  jq '.removing = null' "$receipt" >"$receipt_next" && write_receipt_update
}
rewind_progress() {
  jq --arg active "$1" --argjson configured "$2" \
    '.active = $active | .configured = $configured | .states = .states[0:($configured + 1)] | .removing = null' \
    "$receipt" >"$receipt_next" && write_receipt_update
}
route_transition_matches() {
  jq -en --argjson before "$1" --argjson after "$2" --arg port "$3" \
    --arg host "$4:$3" --arg target "$5" '
      def normalize_empty_roots:
        if .TCP == {} then del(.TCP) else . end
        | if .Web == {} then del(.Web) else . end;
      $after.TCP[$port].HTTPS == true
      and $after.Web[$host].Handlers["/"].Proxy == $target
      and ($after | del(.TCP[$port]) | del(.Web[$host]) | normalize_empty_roots)
        == ($before | normalize_empty_roots)
    ' >/dev/null
}

for command in curl jq tailscale; do command -v "$command" >/dev/null 2>&1 || fail "missing-${command}"; done
case "$mode" in start|status|stop) ;; *) fail usage ;; esac

load_receipt() {
  [ -d "$state" ] && [ ! -L "$state" ] && private_file "$receipt" || return 1
  jq -e '
    .schema == "oxid-standalone-tailnet-routes-v1"
    and .realm == "undeployed" and .fingerprint == "undeployed"
    and (.dnsName | type == "string" and test("^[A-Za-z0-9][A-Za-z0-9.-]*$"))
    and (.baseline | type == "string") and (.active | type == "string")
    and (.routes | type == "array" and length == 3)
    and (.configured | type == "number" and floor == . and . >= 0 and . <= 3)
    and ((.pending == null) or ((.pending | type) == "number" and .pending == .configured and .pending >= 0 and .pending < 3))
    and ((.removing == null) or ((.removing | type) == "number" and .removing == (.configured - 1) and .removing >= 0 and .removing < 3))
    and ((.pending == null) or (.removing == null))
    and (.states | type == "array")
    and (.states | length) == (.configured + 1)
    and all(.states[]; type == "string")
    and .states[0] == .baseline and .states[-1] == .active
    and all(.routes[]; (.port | type == "number" and . >= 12000 and . <= 12999) and (.target | type == "string" and test("^http://127\\.0\\.0\\.1:(6300|8088|9944)$")))
  ' "$receipt" >/dev/null
}

cleanup_start() {
  local status="$1"
  trap - EXIT INT TERM HUP
  if [ -d "$state" ] && [ ! -L "$state" ]; then
    if load_receipt; then
      "$root/scripts/standalone-tailnet-routes.sh" stop >/dev/null 2>&1 || true
    else
      # No route changes before a complete initial receipt exists. Remove only
      # the exact private files and directory created by this start attempt.
      rm -f -- "$receipt" "$receipt_next"
      rmdir -- "$state" 2>/dev/null || true
    fi
  fi
  exit "$status"
}

case "$mode" in
start)
  [ ! -e "$state" ] && [ ! -L "$state" ] || fail session-exists
  [ "$(tailscale status --json | jq -r '.BackendState')" = Running ] || fail tailscale-offline
  "$root/scripts/standalone-status.sh" local >/dev/null || fail standalone
  baseline="$(canonical_serve)" || fail serve-baseline
  dns="$(tailscale status --json | jq -r '.Self.DNSName | rtrimstr(".")')"
  [[ "$dns" =~ ^[A-Za-z0-9][A-Za-z0-9.-]*$ ]] || fail magicdns
  ports=()
  for candidate in $(seq 12000 12999); do
    if jq -e --arg port "$candidate" --arg host "$dns:$candidate" '(.TCP[$port] == null) and (.Web[$host] == null)' <<<"$baseline" >/dev/null; then
      ports+=("$candidate")
      [ "${#ports[@]}" -eq 3 ] && break
    fi
  done
  [ "${#ports[@]}" -eq 3 ] || fail route-unavailable
  trap 'cleanup_start $?' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  trap 'exit 129' HUP
  umask 077; mkdir -p "$state"; chmod 700 "$state"
  targets=(http://127.0.0.1:8088 http://127.0.0.1:9944 http://127.0.0.1:6300)
  jq -cn --arg baseline "$baseline" --arg dns "$dns" \
    --argjson routes "$(jq -cn --argjson indexer "${ports[0]}" --argjson node "${ports[1]}" --argjson proof "${ports[2]}" '[{name:"indexer",port:$indexer,target:"http://127.0.0.1:8088"},{name:"node",port:$node,target:"http://127.0.0.1:9944"},{name:"proof",port:$proof,target:"http://127.0.0.1:6300"}]')" \
    '{schema:"oxid-standalone-tailnet-routes-v1",realm:"undeployed",fingerprint:"undeployed",baseline:$baseline,active:$baseline,dnsName:$dns,routes:$routes,configured:0,pending:null,removing:null,states:[$baseline]}' >"$receipt"
  chmod 600 "$receipt"
  for index in 0 1 2; do
    previous="$(jq -r '.active' "$receipt")"
    mark_pending "$index" || fail receipt-write
    tailscale serve --yes --bg --https="${ports[$index]}" "${targets[$index]}" >/dev/null 2>&1 || true
    after="$(canonical_serve)" || fail serve-active
    if [ "$after" = "$previous" ]; then
      clear_pending || fail receipt-write
      fail serve-add
    fi
    route_transition_matches "$previous" "$after" "${ports[$index]}" "$dns" "${targets[$index]}" || fail serve-add
    append_progress "$after" "$((index + 1))" || fail receipt-write
  done
  trap - EXIT INT TERM HUP
  printf '%s\n' 'standalone-tailnet-routes: READY (private Tailnet routes configured)'
  ;;
status)
  load_receipt || fail receipt
  [ "$(jq -r '.configured' "$receipt")" -eq 3 ] || fail incomplete
  current="$(canonical_serve)" || fail serve-status
  expected="$(jq -r '.active' "$receipt")"
  if [ "$current" != "$expected" ]; then
    # The round-trip lifecycle deliberately starts the faucet after these
    # service routes. Admit only that exact receipt-proven nested addition:
    # its baseline must be this receipt's active state and its active state
    # must be the complete current Serve configuration.
    faucet_receipt="$root/target/standalone-faucet-tailnet/receipt.json"
    private_file "$faucet_receipt" || fail serve-drift
    jq -e --arg baseline "$expected" --arg active "$current" '
      .schema == "oxid-standalone-faucet-tailnet-v1"
      and .baseline == $baseline and .active == $active
    ' "$faucet_receipt" >/dev/null || fail serve-drift
  fi
  "$root/scripts/standalone-status.sh" local >/dev/null || fail standalone
  dns="$(jq -r '.dnsName' "$receipt")"
  indexer_port="$(jq -r '.routes[] | select(.name == "indexer") | .port' "$receipt")"
  node_port="$(jq -r '.routes[] | select(.name == "node") | .port' "$receipt")"
  proof_port="$(jq -r '.routes[] | select(.name == "proof") | .port' "$receipt")"
  curl --noproxy '*' --fail --silent --max-time 10 -o /dev/null "https://$dns:$proof_port/" || fail proof-health
  curl --noproxy '*' --fail --silent --max-time 10 -o /dev/null "https://$dns:$node_port/health" || fail node-health
  curl --noproxy '*' --fail --silent --max-time 10 -H 'content-type: application/json' --data '{"query":"query StandaloneReadiness { block { height } }"}' -o /dev/null "https://$dns:$indexer_port/api/v4/graphql" || fail indexer-health
  printf '%s\n' 'standalone-tailnet-routes: READY realm=undeployed fingerprint=undeployed'
  ;;
stop)
  load_receipt || fail receipt
  removing="$(jq -r '.removing // "none"' "$receipt")"
  if [ "$removing" != none ]; then
    previous="$(jq -r '.active' "$receipt")"
    expected="$(jq -r --argjson index "$removing" '.states[$index]' "$receipt")"
    current="$(canonical_serve)" || fail serve-status
    if [ "$current" = "$expected" ]; then
      rewind_progress "$current" "$removing" || fail receipt-write
    elif [ "$current" = "$previous" ]; then
      clear_removing || fail receipt-write
    else
      fail serve-drift
    fi
  fi
  pending="$(jq -r '.pending // "none"' "$receipt")"
  if [ "$pending" != none ]; then
    previous="$(jq -r '.active' "$receipt")"
    current="$(canonical_serve)" || fail serve-status
    if [ "$current" = "$previous" ]; then
      clear_pending || fail receipt-write
    else
      port="$(jq -r --argjson index "$pending" '.routes[$index].port' "$receipt")"
      target="$(jq -r --argjson index "$pending" '.routes[$index].target' "$receipt")"
      dns="$(jq -r '.dnsName' "$receipt")"
      route_transition_matches "$previous" "$current" "$port" "$dns" "$target" || fail serve-drift
      append_progress "$current" "$((pending + 1))" || fail receipt-write
    fi
  fi
  [ "$(canonical_serve)" = "$(jq -r '.active' "$receipt")" ] || fail serve-drift
  configured="$(jq -r '.configured' "$receipt")"
  while [ "$configured" -gt 0 ]; do
    index="$((configured - 1))"
    port="$(jq -r --argjson index "$index" '.routes[$index].port' "$receipt")"
    expected="$(jq -r --argjson index "$index" '.states[$index]' "$receipt")"
    current="$(jq -r '.active' "$receipt")"
    mark_removing "$index" || fail receipt-write
    tailscale serve --yes --https="$port" off >/dev/null 2>&1 || true
    after="$(canonical_serve)" || fail serve-status
    if [ "$after" = "$expected" ]; then
      configured="$index"
      rewind_progress "$after" "$configured" || fail receipt-write
    elif [ "$after" = "$current" ]; then
      fail serve-remove
    else
      fail serve-drift
    fi
  done
  [ "$(canonical_serve)" = "$(jq -r '.baseline' "$receipt")" ] || fail serve-restore
  remove_owned_state || fail state-cleanup
  printf '%s\n' 'standalone-tailnet-routes: STOPPED'
  ;;
esac
