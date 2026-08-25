#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Installs/removes only the temporary issue #140 HTTPS-9443 path routes. The
# existing 443/8443/10000 configuration is treated as immutable baseline state.
set -euo pipefail
export LC_ALL=C

if [ "$#" -ne 2 ]; then
  echo "usage: portal-tailnet-serve.sh <up|down> <https-magicdns-origin:9443>" >&2
  exit 2
fi

action="$1"
public_origin="$2"
case "$action" in up|down) ;; *) echo "portal-tailnet-serve: invalid action" >&2; exit 2 ;; esac
if ! [[ "$public_origin" =~ ^https://([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.ts\.net:9443$ ]]; then
  echo "portal-tailnet-serve: invalid public origin" >&2
  exit 1
fi

for command_name in jq shasum; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "portal-tailnet-serve: missing required command" >&2
    exit 1
  }
done
tailscale_cli="${OXID_TAILSCALE_CLI:-tailscale}"
command -v "$tailscale_cli" >/dev/null 2>&1 || {
  echo "portal-tailnet-serve: Tailscale CLI unavailable" >&2
  exit 1
}

state_directory="${OXID_PORTAL_TAILNET_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/oxid/portal-tailnet-serve}"
case "$state_directory" in /*) ;; *) echo "portal-tailnet-serve: state directory must be absolute" >&2; exit 1 ;; esac
receipt="$state_directory/receipt.json"
baseline="$state_directory/baseline.json"
installed="$state_directory/installed.json"
public_host="${public_origin#https://}"
web_key="$public_host"

file_mode() {
  if stat -c '%a' -- "$1" >/dev/null 2>&1; then stat -c '%a' -- "$1"; else stat -f '%Lp' -- "$1"; fi
}
sha256_file() { shasum -a 256 "$1" | awk '{print $1}'; }
canonical_status() {
  "$tailscale_cli" serve status --json | jq -Sce .
}
status_without_demo() {
  jq -Sce --arg key "$web_key" '
    del(.TCP["9443"]) | del(.Web[$key]) |
    if (.TCP? == {}) then del(.TCP) else . end |
    if (.Web? == {}) then del(.Web) else . end
  ' "$1"
}
private_regular() {
  [ -f "$1" ] && [ ! -L "$1" ] && [ "$(file_mode "$1")" = 600 ]
}

if [ "$action" = up ]; then
  [ ! -e "$receipt" ] && [ ! -L "$receipt" ] || {
    echo "portal-tailnet-serve: an ownership receipt already exists" >&2
    exit 1
  }
  umask 077
  mkdir -p "$state_directory"
  chmod 700 "$state_directory"
  [ -d "$state_directory" ] && [ ! -L "$state_directory" ] || exit 1
  baseline_candidate="$state_directory/.baseline.$$"
  installed_candidate="$state_directory/.installed.$$"
  receipt_candidate="$state_directory/.receipt.$$"
  cleanup_partial() {
    "$tailscale_cli" serve --yes --https=9443 off >/dev/null 2>&1 || true
    rm -f -- "$baseline_candidate" "$installed_candidate" "$receipt_candidate"
  }
  trap cleanup_partial ERR INT TERM
  canonical_status >"$baseline_candidate"
  chmod 600 "$baseline_candidate"
  if jq -e --arg key "$web_key" '.TCP["9443"] != null or .Web[$key] != null' \
    "$baseline_candidate" >/dev/null; then
    echo "portal-tailnet-serve: HTTPS 9443 is already owned" >&2
    false
  fi

  "$tailscale_cli" serve --yes --bg --https=9443 http://127.0.0.1:18090 >/dev/null
  "$tailscale_cli" serve --yes --bg --https=9443 --set-path=/issuer-resolver \
    http://127.0.0.1:18093 >/dev/null
  "$tailscale_cli" serve --yes --bg --https=9443 --set-path=/mock-verification \
    http://127.0.0.1:9090/mock-verification >/dev/null
  "$tailscale_cli" serve --yes --bg --https=9443 --set-path=/offer \
    http://127.0.0.1:18091/offer >/dev/null
  canonical_status >"$installed_candidate"
  chmod 600 "$installed_candidate"

  jq -e --arg key "$web_key" '
    .TCP["9443"] == {"HTTPS":true} and
    .Web[$key].Handlers == {
      "/":{"Proxy":"http://127.0.0.1:18090"},
      "/issuer-resolver":{"Proxy":"http://127.0.0.1:18093"},
      "/mock-verification":{"Proxy":"http://127.0.0.1:9090/mock-verification"},
      "/offer":{"Proxy":"http://127.0.0.1:18091/offer"}
    }
  ' "$installed_candidate" >/dev/null
  [ "$(status_without_demo "$baseline_candidate")" = \
    "$(status_without_demo "$installed_candidate")" ] || {
    echo "portal-tailnet-serve: unrelated Serve configuration changed" >&2
    false
  }

  mv -- "$baseline_candidate" "$baseline"
  mv -- "$installed_candidate" "$installed"
  jq -cnS \
    --arg baselineSha256 "$(sha256_file "$baseline")" \
    --arg installedSha256 "$(sha256_file "$installed")" \
    --arg publicOrigin "$public_origin" \
    '{baselineSha256:$baselineSha256,installedSha256:$installedSha256,publicOrigin:$publicOrigin,schema:"oxid-portal-tailnet-serve-receipt-v1"}' \
    >"$receipt_candidate"
  chmod 600 "$receipt_candidate"
  mv -- "$receipt_candidate" "$receipt"
  trap - ERR INT TERM
  echo "portal-tailnet-serve: exact HTTPS 9443 routes installed"
  exit 0
fi

for owned_file in "$receipt" "$baseline" "$installed"; do
  private_regular "$owned_file" || {
    echo "portal-tailnet-serve: exact ownership receipt unavailable" >&2
    exit 1
  }
done
jq -e --arg origin "$public_origin" '
  .schema == "oxid-portal-tailnet-serve-receipt-v1" and
  .publicOrigin == $origin and
  (.baselineSha256 | test("^[0-9a-f]{64}$")) and
  (.installedSha256 | test("^[0-9a-f]{64}$")) and
  keys == ["baselineSha256","installedSha256","publicOrigin","schema"]
' "$receipt" >/dev/null || {
  echo "portal-tailnet-serve: ownership receipt mismatch" >&2
  exit 1
}
[ "$(jq -r .baselineSha256 "$receipt")" = "$(sha256_file "$baseline")" ] && \
  [ "$(jq -r .installedSha256 "$receipt")" = "$(sha256_file "$installed")" ] || {
  echo "portal-tailnet-serve: ownership receipt digest mismatch" >&2
  exit 1
}
current="$state_directory/.current.$$"
canonical_status >"$current"
chmod 600 "$current"
if [ "$(sha256_file "$current")" != "$(sha256_file "$installed")" ]; then
  rm -f -- "$current"
  echo "portal-tailnet-serve: Serve state changed; no cleanup performed" >&2
  exit 1
fi
rm -f -- "$current"
"$tailscale_cli" serve --yes --https=9443 off >/dev/null
canonical_status >"$current"
chmod 600 "$current"
if [ "$(sha256_file "$current")" != "$(sha256_file "$baseline")" ]; then
  rm -f -- "$current"
  echo "portal-tailnet-serve: baseline was not restored" >&2
  exit 1
fi
rm -f -- "$current" "$receipt" "$baseline" "$installed"
rmdir "$state_directory" 2>/dev/null || true
echo "portal-tailnet-serve: exact HTTPS 9443 routes removed"
