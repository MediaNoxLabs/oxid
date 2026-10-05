#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Bounded, read-only Docker engine admission for host demo workflows. Callers
# own the policy decision; this helper only classifies the local prerequisite
# and never restarts Docker or mutates containers.
oxid_require_docker_engine() {
  local timeout_seconds="${OXID_DOCKER_PROBE_TIMEOUT_SECONDS:-10}" status
  if ! command -v docker >/dev/null 2>&1; then
    printf '%s\n' 'docker-engine: NOT-READY state=missing remediation=install-docker-cli' >&2
    return 3
  fi
  if ! command -v timeout >/dev/null 2>&1; then
    printf '%s\n' 'docker-engine: NOT-READY state=missing-timeout remediation=enter-project-shell' >&2
    return 3
  fi
  if timeout -k 2s "${timeout_seconds}s" docker info >/dev/null 2>&1; then
    printf '%s\n' 'docker-engine: READY state=responsive'
    return 0
  else
    status=$?
  fi
  case "$status" in
    124|137)
      printf '%s\n' 'docker-engine: NOT-READY state=unresponsive remediation=restart-docker-desktop' >&2
      return 2
      ;;
    *)
      printf '%s\n' 'docker-engine: NOT-READY state=unavailable remediation=start-docker-desktop' >&2
      return 1
      ;;
  esac
}

# Bound read-only Docker discovery performed before a supervised lifecycle has
# taken ownership. Output remains available to the caller for exact matching.
oxid_docker_read() {
  local timeout_seconds="${OXID_DOCKER_READ_TIMEOUT_SECONDS:-15}"
  timeout -k 2s "${timeout_seconds}s" docker "$@"
}
