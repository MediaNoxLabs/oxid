#!/usr/bin/env bash
printf '%s\n' "$*" >>"$OXID_FAKE_ADB_INVENTORY_LOG"
[ "$*" = 'devices -l' ] || {
  printf 'MUTATION\n' >>"$OXID_FAKE_ADB_INVENTORY_LOG"
  exit 97
}
printf 'List of devices attached\nR5CT1234ABC\tdevice product:fixture transport_id:1\n'
