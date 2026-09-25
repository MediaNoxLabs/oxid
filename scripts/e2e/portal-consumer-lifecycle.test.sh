#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
readonly ROOT
readonly LIFECYCLE="$ROOT/scripts/portal-consumer-lifecycle.sh"
readonly SERVICES_LIFECYCLE="$ROOT/scripts/e2e/portal-services-lifecycle.sh"
readonly PORTAL_RUNBOOK="$ROOT/docs/factory/portal-macos-laptop.md"

fail() {
  printf 'portal-consumer-lifecycle-contract: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[ -f "$LIFECYCLE" ] || fail lifecycle
[ -x "$SERVICES_LIFECYCLE" ] || fail services-lifecycle-wrapper
for mapping in \
  "midnight-did-resolver-image) image_id=\"\$(docker image inspect --format '{{.Id}}' midnight-did-resolver:0.1.0" \
  "did-manager-image) image_id=\"\$(docker image inspect --format '{{.Id}}' laceid-did-manager:0.1.0" \
  "issuer-image) image_id=\"\$(docker image inspect --format '{{.Id}}' laceid-issuer:0.1.0"; do
  grep -qF "$mapping" "$LIFECYCLE" || fail image-tag
done
if grep -qE '(midnight-did-resolver|laceid-did-manager|laceid-issuer):local' "$LIFECYCLE"; then
  fail stale-local-tag
fi

for tailnet_contract in \
  'PORTAL_TAILNET_MOCK_STATE_DIR' \
  'tailnet_mock_state_valid' \
  'tailnet-mock-transform.mjs' \
  '--validate "$TAILNET_MOCK_STATE" "$PORTAL_ISSUER_URL"' \
  '--data-binary "@$mock_state"'; do
  grep -qF -- "$tailnet_contract" "$LIFECYCLE" || fail tailnet-private-mock
 done

for preparation_contract in \
  'prerequisite|prepare|prepared-status|up|status|down|services-up|services-status|services-stop' \
  'oxid-portal-consumer-prepared-v1' \
  'prepare-checkpoint.json' \
  'prepared-receipt.json' \
  'midnight-did-resolver-image did-manager-image issuer-image' \
  'localCacheHit:$cacheHit' \
  'prepareDurationSeconds:$duration' \
  'PORTAL_CONSUMER_PREPARED_RECEIPT' \
  'prepared_receipt_valid "$EXTERNAL_PREPARED_RECEIPT" complete'; do
  grep -qF -- "$preparation_contract" "$LIFECYCLE" || fail resumable-preparation
done

for services_contract in \
  'run_services_up()' \
  'run_services_status()' \
  'run_services_stop()' \
  'compose up -d --wait --wait-timeout 600 smocker did-resolver did-manager issuer' \
  'compose stop --timeout 30 smocker did-resolver did-manager issuer' \
  'oxid-portal-consumer-services-status-v1'; do
  grep -qF -- "$services_contract" "$LIFECYCLE" || fail services-lifecycle
done

for wrapper_contract in \
  'portal-consumer-lifecycle.sh' \
  'exec "$ROOT/scripts/portal-consumer-lifecycle.sh" "${1:-}"'; do
  grep -qF -- "$wrapper_contract" "$SERVICES_LIFECYCLE" || fail services-lifecycle-wrapper
done

for lease_contract in \
  'OXID_PORTAL_CONSUMER_LEASE_DIR' \
  'OXID_PORTAL_CONSUMER_SESSION_ID' \
  'stateFingerprint' \
  'mkdir "$LEASE_DIR"' \
  'state:"contention"' \
  'lease-ambiguous' \
  'stale-lease' \
  'acquire_lease' \
  'lease_record_valid_for_session' \
  'lease_release_allowed=0' \
  'lease_release_allowed=1' \
  'cleanup-incomplete'; do
  grep -qF -- "$lease_contract" "$LIFECYCLE" || fail portal-lease
 done

grep -qF 'manual lease recovery' "$PORTAL_RUNBOOK" || fail lease-recovery-docs

services_body="$(sed -n '/run_services_up()/,/run_down()/p' "$LIFECYCLE")"
if printf '%s\n' "$services_body" | grep -qE 'build_image|compose down|adb |pm clear'; then
  fail services-mutation-boundary
fi

temporary="$(mktemp -d)"
trap 'rm -rf -- "$temporary"' EXIT
fake_bin="$temporary/bin"
fake_source="$temporary/portal"
lease="$temporary/lease"
mkdir -p "$fake_bin" "$fake_source"

cat >"$fake_bin/git" <<'FAKE_GIT'
#!/usr/bin/env bash
case "$*" in
  *"remote get-url origin"*) printf '%s\n' 'https://github.com/input-output-hk/lace-id-portal.git' ;;
  *"rev-parse HEAD^{tree}"*) printf '%s\n' '2d845d2293603dfd8adce5362c8a9941e6ba78a9' ;;
  *"rev-parse HEAD"*) printf '%s\n' '25499870f84d77173c46e4af3021311decfb840b' ;;
  *"status --porcelain"*) ;;
  *) exit 1 ;;
esac
FAKE_GIT
cat >"$fake_bin/docker" <<'FAKE_DOCKER'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$OXID_TEST_DOCKER_LOG"
if [ "${OXID_TEST_DOCKER_SIGNAL_PARENT:-0}" -eq 1 ]; then
  kill -TERM "$PPID"
fi
sleep "${OXID_TEST_DOCKER_DELAY:-0}"
exit 0
FAKE_DOCKER
cat >"$fake_bin/nix" <<'FAKE_NIX'
#!/usr/bin/env bash
exit 1
FAKE_NIX
chmod +x "$fake_bin/git" "$fake_bin/docker" "$fake_bin/nix"

session_a="$(printf 'a%.0s' {1..64})"
session_b="$(printf 'b%.0s' {1..64})"
state_a="$temporary/state-a"
state_b="$temporary/state-b"
winner_log="$temporary/winner-docker.log"
loser_log="$temporary/loser-docker.log"

env PATH="$fake_bin:$PATH" \
  PORTAL_INTEGRATION_CHECKOUT="$fake_source" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$state_a" \
  OXID_PORTAL_CONSUMER_LEASE_DIR="$lease" \
  OXID_PORTAL_CONSUMER_SESSION_ID="$session_a" \
  OXID_TEST_DOCKER_LOG="$winner_log" OXID_TEST_DOCKER_DELAY=1 \
  "$LIFECYCLE" status >"$temporary/winner.out" 2>"$temporary/winner.err" &
winner_pid=$!
for _ in {1..100}; do
  [ -f "$lease/owner.json" ] && break
  sleep 0.02
done
[ -f "$lease/owner.json" ] || fail lease-first-admission

set +e
env PATH="$fake_bin:$PATH" \
  PORTAL_INTEGRATION_CHECKOUT="$fake_source" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$state_b" \
  OXID_PORTAL_CONSUMER_LEASE_DIR="$lease" \
  OXID_PORTAL_CONSUMER_SESSION_ID="$session_b" \
  OXID_TEST_DOCKER_LOG="$loser_log" \
  "$LIFECYCLE" status >"$temporary/loser.out" 2>"$temporary/loser.err"
loser_status=$?
set -e
[ "$loser_status" -eq 2 ] || fail lease-contention-status
jq -e --arg owner "${session_a:0:16}" \
  '.schema == "oxid-portal-consumer-lease-v1" and .state == "contention" and .owner.session == $owner' \
  "$temporary/loser.out" >/dev/null || fail lease-contention-evidence
[ ! -s "$loser_log" ] || fail lease-contention-mutated-docker
wait "$winner_pid" || fail lease-winner
jq -e '.state == "stopped"' "$temporary/winner.out" >/dev/null || fail lease-winner-status
[ ! -e "$lease" ] || fail lease-winner-release
[ -s "$winner_log" ] || fail lease-winner-docker-query

set +e
session_c="$(printf 'c%.0s' {1..64})"
env PATH="$fake_bin:$PATH" \
  PORTAL_INTEGRATION_CHECKOUT="$fake_source" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$temporary/state-interrupted" \
  OXID_PORTAL_CONSUMER_LEASE_DIR="$lease" \
  OXID_PORTAL_CONSUMER_SESSION_ID="$session_c" \
  OXID_TEST_DOCKER_LOG="$temporary/interrupted-docker.log" OXID_TEST_DOCKER_SIGNAL_PARENT=1 \
  "$LIFECYCLE" status >"$temporary/interrupted.out" 2>"$temporary/interrupted.err"
interrupted_status=$?
set -e
[ "$interrupted_status" -ne 0 ] || fail lease-interrupt-status
[ ! -e "$lease" ] || fail lease-interrupt-release

mkdir "$lease"
chmod 700 "$lease"
state_fingerprint="$(printf '%s' "$state_a" | shasum -a 256 | awk '{print $1}')"
jq -cn --arg session "$session_a" --arg fingerprint "$state_fingerprint" \
  '{schema:"oxid-portal-consumer-lease-v1",session:$session,stateFingerprint:$fingerprint}' \
  >"$lease/owner.json"
chmod 600 "$lease/owner.json"
set +e
env PATH="$fake_bin:$PATH" \
  PORTAL_INTEGRATION_CHECKOUT="$fake_source" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$state_b" \
  OXID_PORTAL_CONSUMER_LEASE_DIR="$lease" \
  OXID_PORTAL_CONSUMER_SESSION_ID="$session_b" \
  OXID_TEST_DOCKER_LOG="$temporary/wrong-owner-docker.log" \
  "$LIFECYCLE" down >"$temporary/wrong-owner.out" 2>"$temporary/wrong-owner.err"
wrong_owner_status=$?
set -e
[ "$wrong_owner_status" -eq 2 ] || fail lease-wrong-owner-status
[ -f "$lease/owner.json" ] || fail lease-wrong-owner-preserved
[ ! -s "$temporary/wrong-owner-docker.log" ] || fail lease-wrong-owner-mutated-docker

set +e
env PATH="$fake_bin:$PATH" \
  PORTAL_INTEGRATION_CHECKOUT="$fake_source" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$state_a" \
  OXID_PORTAL_CONSUMER_LEASE_DIR="$lease" \
  OXID_PORTAL_CONSUMER_SESSION_ID="$session_a" \
  OXID_TEST_DOCKER_LOG="$temporary/stale-owner-docker.log" \
  "$LIFECYCLE" status >"$temporary/stale-owner.out" 2>"$temporary/stale-owner.err"
stale_owner_status=$?
set -e
[ "$stale_owner_status" -ne 0 ] || fail lease-stale-owner-status
grep -qF 'phase=stale-lease' "$temporary/stale-owner.err" || fail lease-stale-owner-evidence
[ -f "$lease/owner.json" ] || fail lease-stale-owner-preserved

rm -f "$lease/owner.json"
set +e
env PATH="$fake_bin:$PATH" \
  PORTAL_INTEGRATION_CHECKOUT="$fake_source" \
  OXID_PORTAL_CONSUMER_STATE_DIR="$state_b" \
  OXID_PORTAL_CONSUMER_LEASE_DIR="$lease" \
  OXID_PORTAL_CONSUMER_SESSION_ID="$session_b" \
  OXID_TEST_DOCKER_LOG="$temporary/ambiguous-docker.log" \
  "$LIFECYCLE" status >"$temporary/ambiguous.out" 2>"$temporary/ambiguous.err"
ambiguous_status=$?
set -e
[ "$ambiguous_status" -ne 0 ] || fail lease-ambiguous-status
grep -qF 'phase=lease-ambiguous' "$temporary/ambiguous.err" || {
  sed -n '1,20p' "$temporary/ambiguous.err" >&2
  fail lease-ambiguous-evidence
}
[ -d "$lease" ] || fail lease-ambiguous-preserved
[ ! -s "$temporary/ambiguous-docker.log" ] || fail lease-ambiguous-mutated-docker

printf 'portal-consumer-lifecycle-contract: PASS pinned-image-tags=0.1.0 tailnet-private-mock=true resumable-preparation=true lease-admission=atomic lease-contention=non-mutating lease-cleanup=owned services-no-build-install-launch-reset=true\n'
