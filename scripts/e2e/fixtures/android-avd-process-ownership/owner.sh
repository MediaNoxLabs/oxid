#!/usr/bin/env bash
printf '%s\n' "$$" >"$1"
bash "$2" "$3" "$4"
status=$?
printf '%s\n' "$status" >/dev/null
