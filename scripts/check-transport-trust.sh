#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

if rg -n 'danger_accept_invalid|tls_danger_accept_invalid' apps crates --glob '*.rs'; then
  echo "TLS certificate or hostname verification must never be disabled." >&2
  exit 1
fi

client_builders="$(rg -l '(reqwest::)?Client::builder\(' apps crates --glob '*.rs' | sort || true)"
expected_client_builders=$'crates/adapters/openid4vci/src/portal_internal_tests.rs\ncrates/adapters/platform-system/src/transport.rs'
if [ "$client_builders" != "$expected_client_builders" ]; then
  echo "Native HTTP clients must be constructed by oxid-adapter-platform-system." >&2
  echo "$client_builders" >&2
  exit 1
fi

bundled_root_owners="$(rg -l 'webpki_root_certs' apps crates --glob '*.rs' | sort || true)"
if [ "$bundled_root_owners" != "crates/adapters/platform-system/src/transport.rs" ]; then
  echo "Bundled public roots must be owned by the shared transport policy boundary." >&2
  echo "$bundled_root_owners" >&2
  exit 1
fi

websocket_callers="$(rg -l 'connect_async(_with_config|_tls_with_config)?\(' apps crates --glob '*.rs' | sort || true)"
expected_websocket_callers=$'crates/adapters/deployment-profile/src/readiness.rs\ncrates/adapters/midnight/src/indexer.rs\ncrates/adapters/midnight/src/shielded_transport.rs\ncrates/adapters/midnight/src/submission.rs'
if [ "$websocket_callers" != "$expected_websocket_callers" ]; then
  echo "WebSocket call sites must be reviewed and use the shared explicit connector." >&2
  echo "$websocket_callers" >&2
  exit 1
fi

while IFS= read -r source; do
  if ! rg -q 'websocket_connector_for' "$source"; then
    echo "$source opens a WebSocket without the shared transport policy." >&2
    exit 1
  fi
done <<<"$websocket_callers"

versions="$(awk '
  $0 == "name = \"rustls-platform-verifier\"" { found = 1; next }
  found && /^version = / { gsub(/version = /, ""); gsub(/"/, ""); print; found = 0 }
' Cargo.lock | sort -u)"
expected_versions=$'0.5.3\n0.7.0'
if [ "$versions" != "$expected_versions" ]; then
  echo "Review the mobile verifier initializer when the pinned verifier closure changes." >&2
  echo "$versions" >&2
  exit 1
fi

echo "Native transport trust policy passed."
