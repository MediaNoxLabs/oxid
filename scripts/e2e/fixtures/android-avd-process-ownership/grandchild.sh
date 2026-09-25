#!/usr/bin/env bash
trap 'printf "TERM\n" >"$2"' TERM
printf '%s\n' "$$" >"$1"
while :; do sleep 1; done
