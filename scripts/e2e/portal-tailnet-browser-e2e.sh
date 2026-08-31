#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C
CDPATH=

readonly PORTAL_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
readonly PORTAL_COMMIT="22ae5369b6f939e6b20648f4b85dd993527748ef"
readonly PORTAL_TREE="74d8d1a5b87c160ea554006e47d5f3edc3cd3e10"
readonly REPOSITORY_ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly EVIDENCE_ROOT="$REPOSITORY_ROOT/target/portal-tailnet-browser-e2e"
readonly RUNTIME="$EVIDENCE_ROOT/runtime"
readonly SOURCE="$RUNTIME/portal-source"
readonly PRIVATE_LOG="$RUNTIME/private.log"
readonly READY_FIFO="$RUNTIME/ready.fifo"
readonly CAPABILITY_FIFO="$RUNTIME/capability.fifo"
readonly READY="$RUNTIME/ready.json"
readonly CONTROL_CONFIG="$RUNTIME/control-curl.conf"
readonly XDG_CONFIG="$RUNTIME/xdg-config"
readonly XDG_STATE="$RUNTIME/xdg-state"
readonly BROWSER_HOME="$RUNTIME/browser-home"
readonly MOCK_STATE="$RUNTIME/mock-state"
readonly MOCK_PAGE="$RUNTIME/mock-page.html"
readonly ORIGIN_POLICY="$REPOSITORY_ROOT/scripts/e2e/tailnet-origin-policy.mjs"
readonly MOCK_TRANSFORM="$REPOSITORY_ROOT/scripts/e2e/tailnet-mock-transform.mjs"
readonly MOCK_ROUTE="$REPOSITORY_ROOT/scripts/e2e/tailnet-mock-route.mjs"
readonly BROWSER_FLOW="$REPOSITORY_ROOT/scripts/e2e/portal-tailnet-browser-flow.mjs"
readonly PROFILE_SCRIPT_RELATIVE="scripts/tailscale-https-profile.sh"
readonly CONTROL_ORIGIN="http://127.0.0.1:18095"
readonly DEBUG_PORT=19096
readonly DEBUG_ENDPOINT="http://127.0.0.1:$DEBUG_PORT"
readonly CHROME_BIN="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
readonly SOURCE_INPUT="${OXID_PORTAL_SOURCE_REPOSITORY:-$PORTAL_REMOTE}"

support_pid=""
browser_pid=""
profile_active=0
cleanup_running=0
baseline=""

fail() {
  local phase="$1" browser_phase=""
  if [ "$phase" = browser-journey ] && [ -f "$PRIVATE_LOG" ]; then
    browser_phase="$(grep -E '^portal-tailnet-browser-flow: FAIL phase=(connect|page-enable|index|begin|approval|pending|complete|offer-check)$' "$PRIVATE_LOG" | tail -n 1 || true)"
    browser_phase="${browser_phase##*=}"
  fi
  if [ -n "$browser_phase" ]; then
    printf 'portal-tailnet-browser-e2e: FAIL phase=%s browser=%s\n' "$phase" "$browser_phase" >&2
  else
    printf 'portal-tailnet-browser-e2e: FAIL phase=%s\n' "$phase" >&2
  fi
  exit 1
}

file_mode() {
  if stat -c '%a' -- "$1" 2>/dev/null; then :; else stat -f '%Lp' -- "$1"; fi
}

private_regular_file() {
  [ -f "$1" ] && [ ! -L "$1" ] && [ "$(file_mode "$1")" = 600 ]
}

control_curl() {
  curl --config "$CONTROL_CONFIG" --noproxy '*' --fail --silent --show-error --max-time 30 "$@"
}

stop_owned_process() {
  local process_id="$1" wait_seconds="$2"
  kill -TERM "$process_id" >/dev/null 2>&1 || true
  for _attempt in $(seq 1 "$wait_seconds"); do
    kill -0 "$process_id" 2>/dev/null || return 0
    sleep 1
  done
  return 1
}

cleanup() {
  local incoming=$? cleanup_status=0 after_cleanup=""
  if [ "$cleanup_running" -eq 1 ]; then exit "$incoming"; fi
  cleanup_running=1
  trap - EXIT INT TERM HUP
  set +e

  if [ -n "$browser_pid" ]; then
    stop_owned_process "$browser_pid" 30 || cleanup_status=1
    wait "$browser_pid" >/dev/null 2>&1 || true
    browser_pid=""
  fi
  if [ -n "$support_pid" ]; then
    if [ -f "$CONTROL_CONFIG" ]; then
      control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null 2>&1 || true
    fi
    stop_owned_process "$support_pid" 120 || cleanup_status=1
    wait "$support_pid" >/dev/null 2>&1 || cleanup_status=1
    support_pid=""
  fi
  if [ "$profile_active" -eq 1 ]; then
    XDG_CONFIG_HOME="$XDG_CONFIG" XDG_STATE_HOME="$XDG_STATE" \
      "$SOURCE/$PROFILE_SCRIPT_RELATIVE" cleanup >>"$PRIVATE_LOG" 2>&1 || cleanup_status=1
    profile_active=0
    after_cleanup="$(tailscale serve status --json 2>/dev/null | jq -S -c '.')" || cleanup_status=1
    [ "$after_cleanup" = "$baseline" ] || cleanup_status=1
  fi
  if [ -n "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet 2>/dev/null)" ]; then
    cleanup_status=1
  fi
  rm -f -- "$READY_FIFO" "$CAPABILITY_FIFO" "$CONTROL_CONFIG"
  if [ "$cleanup_status" -eq 0 ]; then
    rm -rf -- "$RUNTIME"
    [ ! -e "$RUNTIME" ] && [ ! -L "$RUNTIME" ] || cleanup_status=1
  fi
  if [ "$cleanup_status" -ne 0 ]; then
    incoming=1
    printf 'portal-tailnet-browser-e2e: exact cleanup could not be proven\n' >&2
  fi
  exit "$incoming"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

for command_name in curl docker git jq node shasum tailscale; do
  command -v "$command_name" >/dev/null 2>&1 || fail missing-tool
done
[ -x "$CHROME_BIN" ] || fail browser
[ -f "$MOCK_TRANSFORM" ] && [ -f "$MOCK_ROUTE" ] && [ -f "$BROWSER_FLOW" ] || fail harness
[ -z "$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty
[ ! -e "$EVIDENCE_ROOT/evidence.json" ] && [ ! -L "$EVIDENCE_ROOT/evidence.json" ] || fail stale-evidence
[ ! -e "$RUNTIME" ] && [ ! -L "$RUNTIME" ] || fail stale-runtime
[ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] || fail occupied-project

status_json="$(tailscale status --json)"
[ "$(jq -r '.BackendState' <<<"$status_json")" = Running ] || fail tailscale
DNS_NAME="$(jq -r '.Self.DNSName | rtrimstr(".")' <<<"$status_json")"
OXID_TAILNET_ORIGIN_POLICY_INPUT="$DNS_NAME" node "$ORIGIN_POLICY" --host-env || fail tailscale-identity
baseline="$(tailscale serve status --json | jq -S -c '.')"
listener=""
for candidate in $(seq 11000 11999); do
  key="$DNS_NAME:$candidate"
  if jq -e --arg port "$candidate" --arg key "$key" \
    '(.TCP[$port] == null) and (.Web[$key] == null)' <<<"$baseline" >/dev/null; then
    listener="$candidate"
    break
  fi
done
[ -n "$listener" ] || fail listener
public_origin="https://$DNS_NAME:$listener"
OXID_TAILNET_ORIGIN_POLICY_INPUT="$public_origin" node "$ORIGIN_POLICY" --origin-env || fail origin
mock_route_config="$(node "$MOCK_ROUTE" --config "$public_origin" "$listener")" || fail mock-route-config
jq -e --argjson port "$listener" '
  . == {route:{path:"/kyc",httpsPort:$port,upstream:"http://127.0.0.1:9090"},externalRequestPath:"/kyc/mock-verification",upstreamRequestPath:"/mock-verification"}
' <<<"$mock_route_config" >/dev/null || fail mock-route-config
mock_external_path="$(jq -r '.externalRequestPath' <<<"$mock_route_config")"

umask 077
mkdir -p -- "$EVIDENCE_ROOT" "$RUNTIME" "$XDG_CONFIG/lace-id-portal" "$XDG_STATE" "$BROWSER_HOME" "$MOCK_STATE"
chmod 700 "$EVIDENCE_ROOT" "$RUNTIME" "$XDG_CONFIG" "$XDG_CONFIG/lace-id-portal" "$XDG_STATE" "$BROWSER_HOME" "$MOCK_STATE"
: >"$PRIVATE_LOG"
chmod 600 "$PRIVATE_LOG"
printf '%s' "$baseline" >"$RUNTIME/tailscale-baseline.json"
chmod 600 "$RUNTIME/tailscale-baseline.json"

git clone --no-checkout "$SOURCE_INPUT" "$SOURCE" >>"$PRIVATE_LOG" 2>&1 || fail source-clone
git -C "$SOURCE" remote set-url origin "$PORTAL_REMOTE"
git -C "$SOURCE" fetch origin integration >>"$PRIVATE_LOG" 2>&1 || fail source-fetch
[ "$(git -C "$SOURCE" rev-parse FETCH_HEAD^{commit})" = "$PORTAL_COMMIT" ] || fail source-commit
[ "$(git -C "$SOURCE" rev-parse FETCH_HEAD^{tree})" = "$PORTAL_TREE" ] || fail source-tree
git -C "$SOURCE" checkout --detach "$PORTAL_COMMIT" >>"$PRIVATE_LOG" 2>&1 || fail source-checkout
chmod 700 "$SOURCE"
[ -z "$(git -C "$SOURCE" status --porcelain --untracked-files=all)" ] || fail source-dirty
[ -x "$SOURCE/$PROFILE_SCRIPT_RELATIVE" ] || fail profile-source
grep -qF 'qr.addData(offerUri);' "$SOURCE/crates/issuer-http/web/issue/complete.html" || fail qr-source
node "$MOCK_TRANSFORM" --create "$SOURCE" "$MOCK_STATE" "$public_origin" || fail mock-transform
private_regular_file "$MOCK_STATE/didit-tailnet.yml" || fail mock-mode
private_regular_file "$MOCK_STATE/didit-tailnet-receipt.json" || fail mock-receipt

PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
OXID_PORTAL_CONSUMER_STATE_DIR="$RUNTIME/portal-consumer" \
  "$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" prerequisite >>"$PRIVATE_LOG" 2>&1 || fail standalone-prerequisite

jq -cn --arg dns "$DNS_NAME" --argjson port "$listener" --argjson mock_route "$mock_route_config" '
  {PORTAL_TAILSCALE_DNS_NAME:$dns,routes:[
    {path:"/",httpsPort:$port,upstream:"http://127.0.0.1:18090"},
    {path:"/issuer-resolver",httpsPort:$port,upstream:"http://127.0.0.1:18093"},
    {path:"/offer",httpsPort:$port,upstream:"http://127.0.0.1:18094"},
    $mock_route.route
  ]}' >"$XDG_CONFIG/lace-id-portal/tailscale-https.json"
chmod 600 "$XDG_CONFIG/lace-id-portal/tailscale-https.json"

mkfifo "$READY_FIFO" "$CAPABILITY_FIFO"
chmod 600 "$READY_FIFO" "$CAPABILITY_FIFO"
exec 8<>"$CAPABILITY_FIFO"
exec 9<>"$READY_FIFO"
PORTAL_INTEGRATION_CHECKOUT="$SOURCE" \
OXID_PORTAL_MOBILE_STATE_DIR="$RUNTIME" \
OXID_PORTAL_MOBILE_READY_FIFO="$READY_FIFO" \
OXID_PORTAL_MOBILE_CAPABILITY_FIFO="$CAPABILITY_FIFO" \
PORTAL_CONSUMER_LIFECYCLE="$REPOSITORY_ROOT/scripts/portal-consumer-lifecycle.sh" \
PORTAL_TAILNET_MOCK_STATE_DIR="$MOCK_STATE" \
OXID_BUILD_PORTAL_PUBLIC_ORIGIN="$public_origin" \
  nohup node "$REPOSITORY_ROOT/scripts/e2e/portal-android-support.mjs" >>"$PRIVATE_LOG" 2>&1 &
support_pid=$!
if ! IFS= read -r -t 900 -u 9 ready_status; then fail support-timeout; fi
exec 9>&-
rm -f -- "$READY_FIFO"
[ "$ready_status" = READY ] || fail "${ready_status#FAIL:}"
kill -0 "$support_pid" 2>/dev/null || fail support
control_capability="$(jq -r '.controlCapability // empty' "$READY")"
[ "$(jq -r '.schema // empty' "$READY")" = oxid-portal-android-ready-v2 ] \
  && [[ "$control_capability" =~ ^[0-9a-f]{64}$ ]] || fail support-receipt
printf 'header = "Authorization: Bearer %s"\n' "$control_capability" >"$CONTROL_CONFIG"
chmod 600 "$CONTROL_CONFIG"
control_capability=""

XDG_CONFIG_HOME="$XDG_CONFIG" XDG_STATE_HOME="$XDG_STATE" \
  "$SOURCE/$PROFILE_SCRIPT_RELATIVE" setup >>"$PRIVATE_LOG" 2>&1 || fail profile-setup
profile_active=1
curl --noproxy '*' --fail --silent --show-error --max-time 30 \
  "$public_origin$mock_external_path" >"$MOCK_PAGE" || fail mock-route
chmod 600 "$MOCK_PAGE"
grep -qF 'id="approve-btn"' "$MOCK_PAGE" || fail mock-route
rm -f -- "$MOCK_PAGE"

curl --noproxy '*' --silent --show-error --max-time 1 "$DEBUG_ENDPOINT/json/version" >/dev/null 2>&1 \
  && fail debug-port-occupied
"$CHROME_BIN" --headless=new --no-first-run --disable-background-networking \
  --remote-debugging-address=127.0.0.1 --remote-debugging-port="$DEBUG_PORT" \
  --user-data-dir="$BROWSER_HOME" about:blank >>"$PRIVATE_LOG" 2>&1 &
browser_pid=$!
for _attempt in $(seq 1 150); do
  curl --noproxy '*' --silent --show-error --max-time 1 "$DEBUG_ENDPOINT/json/version" >/dev/null 2>&1 && break
  kill -0 "$browser_pid" 2>/dev/null || fail browser-start
  sleep 0.1
done
curl --noproxy '*' --silent --show-error --max-time 1 "$DEBUG_ENDPOINT/json/version" >/dev/null 2>&1 || fail browser-timeout
node "$BROWSER_FLOW" --run "$DEBUG_ENDPOINT" "$public_origin" >>"$PRIVATE_LOG" 2>&1 || fail browser-journey
stop_owned_process "$browser_pid" 30 || fail browser-cleanup
wait "$browser_pid" >/dev/null 2>&1 || true
browser_pid=""

control_curl -X POST "$CONTROL_ORIGIN/complete" >/dev/null || fail support-complete
stop_owned_process "$support_pid" 120 || fail support-cleanup
wait "$support_pid" || fail support-cleanup
support_pid=""
[ -z "$(docker ps -a --filter label=com.docker.compose.project=oxid-portal-consumer --quiet)" ] || fail portal-cleanup
XDG_CONFIG_HOME="$XDG_CONFIG" XDG_STATE_HOME="$XDG_STATE" \
  "$SOURCE/$PROFILE_SCRIPT_RELATIVE" cleanup >>"$PRIVATE_LOG" 2>&1 || fail profile-cleanup
profile_active=0
after_cleanup="$(tailscale serve status --json | jq -S -c '.')"
[ "$after_cleanup" = "$baseline" ] || fail serve-restoration
[ -z "$(git -C "$SOURCE" status --porcelain --untracked-files=all)" ] || fail source-dirty
[ -z "$(git -C "$REPOSITORY_ROOT" status --porcelain --untracked-files=no)" ] || fail oxid-dirty

OXID_HEAD="$(git -C "$REPOSITORY_ROOT" rev-parse HEAD)"
OXID_TREE="$(git -C "$REPOSITORY_ROOT" rev-parse 'HEAD^{tree}')"
jq -cn --arg head "$OXID_HEAD" --arg tree "$OXID_TREE" --arg commit "$PORTAL_COMMIT" --arg portal_tree "$PORTAL_TREE" '
  {schema:"oxid-tailnet-browser-same-origin-v1",oxid:{head:$head,tree:$tree},portal:{integrationCommit:$commit,integrationTree:$portal_tree},browser:"chromium-headless",acceptance:{browserOnly:true,checkoutClean:true,exactServeRestoration:true,freshRuntime:true,httpsSingleOrigin:true,mockApprovalReached:true,pendingReached:true,completeReached:true,qrAndCopyOfferAgree:true}}
' >"$EVIDENCE_ROOT/evidence.json"
chmod 600 "$EVIDENCE_ROOT/evidence.json"
if grep -Eqi 'https?://|localhost|127\.0\.0\.1|openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|credential|offer|session|path|pid|timestamp' "$EVIDENCE_ROOT/evidence.json"; then
  fail evidence-redaction
fi
jq -e --arg head "$OXID_HEAD" --arg tree "$OXID_TREE" '
  .schema == "oxid-tailnet-browser-same-origin-v1"
  and .oxid == {head:$head,tree:$tree}
  and (.acceptance | to_entries | all(.value == true))' "$EVIDENCE_ROOT/evidence.json" >/dev/null || fail evidence-schema
printf 'portal-tailnet-browser-e2e: PASS evidence=target/portal-tailnet-browser-e2e/evidence.json\n'
