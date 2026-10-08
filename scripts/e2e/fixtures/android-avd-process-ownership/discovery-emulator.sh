#!/usr/bin/env bash
[ "${1:-}" = -list-avds ] || exit 90
printf '%s\n' zeta-avd alpha-avd
