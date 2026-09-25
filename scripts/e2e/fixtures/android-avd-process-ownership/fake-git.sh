#!/usr/bin/env bash
if [ "${3:-}" = status ]; then exit 0; fi
if [ "${1:-}" = clone ]; then
  if [ "${OXID_FAKE_GIT_CLONE_MODE:-fail}" = block ]; then
    printf 'ready\n' >"$OXID_FAKE_GIT_READY"
    sleep 2
  fi
fi
exit 97
