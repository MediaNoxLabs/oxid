#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Receipt-scoped Tailnet routes for the existing standalone stack.
set -euo pipefail
export LC_ALL=C

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
state="$root/target/standalone-tailnet-routes"
receipt="$state/receipt.json"
mode="${1:-}"

fail() { printf 'standalone-tailnet-routes: FAIL phase=%s\n' "$1" >&2; exit 1; }
private_file() {
  local mode
  [ -f "$1" ] && [ ! -L "$1" ] || return 1
  if stat -f '%Lp' "$1" >/dev/null 2>&1; then mode="$(stat -f '%Lp' "$1")"; else mode="$(stat -c '%a' "$1")"; fi
  [ "$mode" = 600 ]
}
canonical_serve() { tailscale serve status --json | jq -S -c '.'; }
remove_owned_state() { rm -f -- "$receipt"; rmdir -- "$state"; }

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
    and all(.routes[]; (.port | type == "number" and . >= 12000 and . <= 12999) and (.target | type == "string" and test("^http://127\\.0\\.0\\.1:(6300|8088|9944)$")))
  ' "$receipt" >/dev/null
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
  umask 077; mkdir -p "$state"; chmod 700 "$state"
  configured=()
  cleanup_start() {
    local status="$1" after=""
    trap - EXIT INT TERM HUP
    for port in "${configured[@]}"; do tailscale serve --yes --https="$port" off >/dev/null 2>&1 || true; done
    after="$(canonical_serve 2>/dev/null || true)"
    [ "$after" = "$baseline" ] && remove_owned_state >/dev/null 2>&1 || true
    exit "$status"
  }
  trap 'cleanup_start $?' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  trap 'exit 129' HUP
  targets=(http://127.0.0.1:8088 http://127.0.0.1:9944 http://127.0.0.1:6300)
  for index in 0 1 2; do
    tailscale serve --yes --bg --https="${ports[$index]}" "${targets[$index]}" >/dev/null || fail serve-add
    configured+=("${ports[$index]}")
  done
  active="$(canonical_serve)" || fail serve-active
  jq -cn --arg baseline "$baseline" --arg active "$active" --arg dns "$dns" \
    --argjson routes "$(jq -cn --argjson indexer "${ports[0]}" --argjson node "${ports[1]}" --argjson proof "${ports[2]}" '[{name:"indexer",port:$indexer,target:"http://127.0.0.1:8088"},{name:"node",port:$node,target:"http://127.0.0.1:9944"},{name:"proof",port:$proof,target:"http://127.0.0.1:6300"}]')" \
    '{schema:"oxid-standalone-tailnet-routes-v1",realm:"undeployed",fingerprint:"undeployed",baseline:$baseline,active:$active,dnsName:$dns,routes:$routes}' >"$receipt"
  chmod 600 "$receipt"
  trap - EXIT INT TERM HUP
  printf '%s\n' 'standalone-tailnet-routes: READY (private Tailnet routes configured)'
  ;;
status)
  load_receipt || fail receipt
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
  [ "$(canonical_serve)" = "$(jq -r '.active' "$receipt")" ] || fail serve-drift
  while read -r port; do tailscale serve --yes --https="$port" off >/dev/null || fail serve-remove; done < <(jq -r '.routes[].port' "$receipt")
  [ "$(canonical_serve)" = "$(jq -r '.baseline' "$receipt")" ] || fail serve-restore
  remove_owned_state || fail state-cleanup
  printf '%s\n' 'standalone-tailnet-routes: STOPPED'
  ;;
esac
