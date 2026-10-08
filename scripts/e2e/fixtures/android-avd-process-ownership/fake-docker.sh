#!/usr/bin/env bash
printf '%s\n' "$*" >>"$OXID_FAKE_DOCKER_LOG"
case "${1:-}" in
  info) exit 0 ;;
  ps)
    count=0
    [ ! -f "$OXID_FAKE_DOCKER_COUNT" ] || count="$(cat "$OXID_FAKE_DOCKER_COUNT")"
    count=$((count + 1))
    printf '%s\n' "$count" >"$OXID_FAKE_DOCKER_COUNT"
    if [ "$count" -eq 1 ]; then
      behavior="${OXID_FAKE_DOCKER_INITIAL:-empty}"
    else
      behavior="${OXID_FAKE_DOCKER_CLEANUP:-empty}"
      receipt="$(find "$OXID_FAKE_STACK_ROOT/target/portal-virtual-mobile/stack.lock" \
        -mindepth 1 -maxdepth 1 -type d -name 'receipt-*' -print -quit)"
      case "${OXID_FAKE_STACK_MUTATION:-none}" in
        receipt-nonempty) printf 'block\n' >"$receipt/blocker" ;;
        replace-receipt)
          rmdir "$receipt" && mkdir "$receipt" && printf 'foreign\n' >"$receipt/marker"
          ;;
        replace-lock)
          rm -rf "$OXID_FAKE_STACK_ROOT/target/portal-virtual-mobile/stack.lock"
          mkdir "$OXID_FAKE_STACK_ROOT/target/portal-virtual-mobile/stack.lock"
          printf 'foreign\n' >"$OXID_FAKE_STACK_ROOT/target/portal-virtual-mobile/stack.lock/marker"
          ;;
      esac
    fi
    case "$behavior" in
      empty) ;;
      nonempty) printf 'occupied-public-project\n' ;;
      error) exit 96 ;;
      timeout) sleep 30 ;;
      *) exit 95 ;;
    esac
    ;;
  *) exit 97 ;;
esac
