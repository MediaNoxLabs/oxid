#!/usr/bin/env bash
parent="$(ps -p "$PPID" -o comm= 2>/dev/null)"
printf 'parent=%s serial=%s args=%s\n' "$parent" "${ANDROID_SERIAL:-unset}" "$*" >>"$OXID_FAKE_ADB_LOG"
case "${1:-}" in
  devices) printf 'List of devices attached\nfixture-device\tdevice\n' ;;
  get-state) printf 'offline\n' ;;
esac
