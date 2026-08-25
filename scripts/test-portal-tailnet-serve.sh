#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Unit contract for exact receipt-matched Serve mutation. The real `tailscale`
# binary is never invoked.
set -euo pipefail
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
stub="$scratch/tailscale-stub"
state="$scratch/serve.json"
origin="https://oxid-demo.tail1234.ts.net:9443"
web_key="oxid-demo.tail1234.ts.net:9443"
cat >"$state" <<'JSON'
{"TCP":{"10000":{"HTTPS":true},"443":{"HTTPS":true},"8443":{"HTTPS":true}},"Web":{"oxid-demo.tail1234.ts.net:10000":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:9944"}}},"oxid-demo.tail1234.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:6300"}}},"oxid-demo.tail1234.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:8088"}}}}}
JSON
cat >"$stub" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1:$2:$3" = "serve:status:--json" ]; then cat "$STUB_STATE"; exit 0; fi
[ "$1" = serve ]; shift
path=/
target="${!#}"
for argument in "$@"; do
  case "$argument" in --set-path=*) path="${argument#--set-path=}" ;; esac
done
if [ "$target" = off ]; then
  jq -c --arg key "$STUB_WEB" 'del(.TCP["9443"]) | del(.Web[$key])' "$STUB_STATE" >"$STUB_STATE.next"
else
  jq -c --arg key "$STUB_WEB" --arg path "$path" --arg target "$target" '
    .TCP["9443"]={"HTTPS":true} |
    .Web[$key].Handlers[$path]={"Proxy":$target}
  ' "$STUB_STATE" >"$STUB_STATE.next"
fi
mv "$STUB_STATE.next" "$STUB_STATE"
STUB
chmod +x "$stub"
baseline="$(jq -Sce . "$state")"
STUB_STATE="$state" STUB_WEB="$web_key" \
OXID_TAILSCALE_CLI="$stub" OXID_PORTAL_TAILNET_STATE_DIR="$scratch/receipt" \
  ./scripts/portal-tailnet-serve.sh up "$origin" >/dev/null
jq -e --arg key "$web_key" '
  .TCP["9443"].HTTPS == true and
  .Web[$key].Handlers == {
    "/":{"Proxy":"http://127.0.0.1:18090"},
    "/issuer-resolver":{"Proxy":"http://127.0.0.1:18093"},
    "/mock-verification":{"Proxy":"http://127.0.0.1:9090/mock-verification"},
    "/offer":{"Proxy":"http://127.0.0.1:18091/offer"}
  }
' "$state" >/dev/null
STUB_STATE="$state" STUB_WEB="$web_key" \
OXID_TAILSCALE_CLI="$stub" OXID_PORTAL_TAILNET_STATE_DIR="$scratch/receipt" \
  ./scripts/portal-tailnet-serve.sh down "$origin" >/dev/null
[ "$(jq -Sce . "$state")" = "$baseline" ]
[ ! -e "$scratch/receipt/receipt.json" ]
printf 'portal-tailnet-serve: unit PASS (stub only)\n'
