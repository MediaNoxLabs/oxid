#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Thin orchestration for the two receipt-owned Tailnet boundaries.
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
mode="${1:-}"
case "$mode" in start|status|stop) ;; *) printf '%s\n' 'standalone-tailnet-round-trip: FAIL phase=usage' >&2; exit 1 ;; esac

case "$mode" in
start)
  "$root/scripts/standalone-tailnet-routes.sh" start
  if ! "$root/scripts/standalone-faucet-tailnet.sh" start; then
    "$root/scripts/standalone-tailnet-routes.sh" stop || true
    printf '%s\n' 'standalone-tailnet-round-trip: FAIL phase=faucet' >&2
    exit 1
  fi
  printf '%s\n' 'standalone-tailnet-round-trip: READY (run just ios-standalone-tailnet)'
  ;;
status)
  "$root/scripts/standalone-tailnet-routes.sh" status
  "$root/scripts/standalone-faucet-tailnet.sh" status
  printf '%s\n' 'standalone-tailnet-round-trip: READY'
  ;;
stop)
  "$root/scripts/standalone-faucet-tailnet.sh" stop
  "$root/scripts/standalone-tailnet-routes.sh" stop
  printf '%s\n' 'standalone-tailnet-round-trip: STOPPED'
  ;;
esac
