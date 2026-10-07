#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
extension_path="$repo_root/.pi/npm/node_modules/@stixxert/pi-docker-sandbox/index.ts"

usage() {
  printf '%s\n' \
    "Usage: scripts/factory/pi-docker-sandbox.sh [PI_ARGS...]" \
    "       scripts/factory/pi-docker-sandbox.sh --check" \
    "" \
    "Start host Pi with a disposable, private Docker Sandbox deploy target." \
    "This launcher does not enable the package's full execution backend."
}

check_host() {
  if ! command -v sbx >/dev/null 2>&1; then
    echo "Docker Sandboxes CLI (sbx) is required; see ADR-0112." >&2
    exit 1
  fi
  sbx version >/dev/null
  if ! sbx ls >/dev/null; then
    echo "Docker Sandboxes is not authenticated or ready; run 'sbx login' and configure a reviewed network policy." >&2
    exit 1
  fi
}

check_package() {
  if [[ ! -f "$extension_path" ]]; then
    echo "missing exact project @stixxert/pi-docker-sandbox@1.1.6 closure" >&2
    echo "enter through ./bootstrap.sh once to provision the tracked Pi package closure" >&2
    exit 1
  fi
  node --input-type=module -e '
    import { readFile } from "node:fs/promises";
    import path from "node:path";
    const root = process.argv[1];
    const manifest = JSON.parse(await readFile(path.join(root, "package.json"), "utf8"));
    if (manifest.name !== "@stixxert/pi-docker-sandbox" || manifest.version !== "1.1.6") {
      throw new Error(`unexpected Docker Sandbox package ${manifest.name}@${manifest.version}`);
    }
    if (!manifest.pi?.extensions?.includes("./index.ts")) {
      throw new Error("Docker Sandbox package does not declare ./index.ts");
    }
  ' "$(dirname "$extension_path")"
}

case "${1:-}" in
  --help|-h)
    usage
    exit 0
    ;;
  --check)
    shift
    if (( $# != 0 )); then
      echo "--check does not accept additional arguments" >&2
      usage >&2
      exit 2
    fi
    check_host
    if [[ "${OXID_PI_DOCKER_SANDBOX_IN_DEVSHELL:-}" != "1" ]]; then
      exec env OXID_PI_DOCKER_SANDBOX_IN_DEVSHELL=1 \
        "$repo_root/bootstrap.sh" -- bash "$repo_root/scripts/factory/pi-docker-sandbox.sh" --check
    fi
    check_package
    printf 'Docker Sandbox deploy target ready: %s\n' "$(sbx version | head -1)"
    ;;
  *)
    check_host
    unset DOCKER_SANDBOX
    unset DOCKER_SANDBOX_ALLOW_UNSANDBOXED
    unset DOCKER_SANDBOX_ENV_ALLOWLIST
    unset DOCKER_SANDBOX_ENV_PASSTHROUGH
    export DOCKER_SANDBOX_AUTOCREATE="1"
    export DOCKER_SANDBOX_CPUS="${DOCKER_SANDBOX_CPUS:-4}"
    export DOCKER_SANDBOX_MEMORY="${DOCKER_SANDBOX_MEMORY:-8g}"
    export DOCKER_SANDBOX_WORKSPACE_RO="1"
    export DOCKER_SANDBOX_TEARDOWN="remove"
    exec "$repo_root/bootstrap.sh" --pi -e "$extension_path" "$@"
    ;;
esac
