#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
FIXTURE="$SCRATCH/repository"
MOCK_BIN="$SCRATCH/bin"
MOCK_STATE="$SCRATCH/docker"
LAUNCHER_ONE="$SCRATCH/launcher-one"
LAUNCHER_TWO="$SCRATCH/launcher-two"
cleanup() {
  rm -rf -- "$SCRATCH"
}
trap cleanup EXIT

mkdir -p "$FIXTURE/scripts/lib" "$MOCK_BIN" "$MOCK_STATE" "$LAUNCHER_ONE" "$LAUNCHER_TWO"
cp "$ROOT/scripts/standalone-up.sh" "$ROOT/scripts/standalone-down.sh" \
  "$ROOT/scripts/standalone-status.sh" \
  "$ROOT/scripts/standalone-stack.yml" "$FIXTURE/scripts/"
cp "$ROOT/scripts/lib/standalone-compose-ownership.sh" \
  "$ROOT/scripts/lib/standalone-state.sh" "$FIXTURE/scripts/lib/"
git -C "$FIXTURE" init -q

cat >"$MOCK_BIN/docker" <<'MOCK_DOCKER'
#!/usr/bin/env bash
set -euo pipefail
state="${MOCK_DOCKER_STATE:?}"
command_name="${1:-}"
shift || true
case "$command_name" in
  info) exit 0 ;;
  ps)
    if [ -f "$state/active" ]; then
      arguments="$*"
      case "$arguments" in
        *compose.service=indexer*) printf '%s\n' indexer-id ;;
        *compose.service=node*) printf '%s\n' node-id ;;
        *compose.service=proof-server*) printf '%s\n' proof-id ;;
        *) printf '%s\n' indexer-id node-id proof-id ;;
      esac
    fi
    ;;
  inspect)
    format=""
    id=""
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --format) format="$2"; shift 2 ;;
        *) id="$1"; shift ;;
      esac
    done
    case "$format" in
      *State.Status*)
        case "$id" in
          indexer-id|node-id) echo 'running healthy 0' ;;
          proof-id) echo 'running none 0' ;;
          *) exit 1 ;;
        esac
        ;;
      *compose.service*)
        case "$id" in indexer-id) echo indexer ;; node-id) echo node ;; proof-id) echo proof-server ;; *) exit 1 ;; esac
        ;;
      *project.config_files*) cat "$state/compose-path" ;;
      *project.working_dir*) dirname "$(cat "$state/compose-path")" ;;
      *compose.project*) echo oxid-standalone ;;
      *) exit 1 ;;
    esac
    ;;
  compose)
    printf '%s\n' "$*" >>"$state/compose-invocations"
    compose_file=""
    operation=""
    while [ "$#" -gt 0 ]; do
      case "$1" in
        -f) compose_file="$2"; shift 2 ;;
        up|down|logs) operation="$1"; shift ;;
        *) shift ;;
      esac
    done
    case "$operation" in
      up)
        printf '%s\n' "$compose_file" >"$state/compose-path"
        : >"$state/active"
        ;;
      down) rm -f -- "$state/active" ;;
      logs) ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
MOCK_DOCKER

cat >"$MOCK_BIN/curl" <<'MOCK_CURL'
#!/usr/bin/env bash
set -euo pipefail
arguments="$*"
case "$arguments" in
  *chain_getHeader*) printf '%s\n' '{"result":{"number":"0x1"}}' ;;
  *StandaloneReadiness*) printf '%s\n' '{"data":{"block":{"height":1}}}' ;;
  *) : ;;
esac
MOCK_CURL
chmod +x "$MOCK_BIN/docker" "$MOCK_BIN/curl"

export MOCK_DOCKER_STATE="$MOCK_STATE"
export PATH="$MOCK_BIN:$PATH"
: >"$MOCK_STATE/foreign-project"

# A same-name project with only public labels is never enough for readiness.
: >"$MOCK_STATE/active"
if TMPDIR="$LAUNCHER_ONE" "$FIXTURE/scripts/standalone-status.sh" local \
  >/dev/null 2>"$SCRATCH/foreign-status.log"; then
  echo "standalone-state-lifecycle: FAIL foreign label-only status admitted" >&2
  exit 1
fi
grep -q 'durable receipt' "$SCRATCH/foreign-status.log"
rm -f -- "$MOCK_STATE/active"

TMPDIR="$LAUNCHER_ONE" "$FIXTURE/scripts/standalone-up.sh" local >/dev/null
STATE_DIRECTORY="$FIXTURE/.git/oxid/standalone"
[ -f "$STATE_DIRECTORY/owner-receipt.json" ]
[ -f "$STATE_DIRECTORY/canonical-compose.yml" ]
find "$STATE_DIRECTORY" -prune -perm 700 -print | grep -qx "$STATE_DIRECTORY"
find "$STATE_DIRECTORY/canonical-indexer.env" -prune -perm 600 -print \
  | grep -qx "$STATE_DIRECTORY/canonical-indexer.env"
rm -rf -- "$LAUNCHER_ONE"

TMPDIR="$LAUNCHER_TWO" "$FIXTURE/scripts/standalone-status.sh" local \
  | grep -q 'oxid standalone (local): READY'
TMPDIR="$LAUNCHER_TWO" "$FIXTURE/scripts/standalone-down.sh" >/dev/null
[ ! -e "$STATE_DIRECTORY/owner-receipt.json" ]
[ ! -e "$STATE_DIRECTORY/canonical-indexer.env" ]
[ ! -e "$STATE_DIRECTORY/canonical-compose.yml" ]
[ ! -e "$MOCK_STATE/active" ]
[ -f "$MOCK_STATE/foreign-project" ]
grep -q -- '-p oxid-standalone' "$MOCK_STATE/compose-invocations"

OVERRIDE_STATE="$SCRATCH/explicit-state"
PHYSICAL_SCRATCH="$(cd -- "$SCRATCH" && pwd -P)"
resolved_override="$(
  OXID_STANDALONE_STATE_DIR="$OVERRIDE_STATE" bash -c \
    'source "$1"; oxid_standalone_state_directory "$2"' _ \
    "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE"
)"
[ "$resolved_override" = "$PHYSICAL_SCRATCH/explicit-state" ]
mkdir -p "$FIXTURE/nested"
mkdir -p "$SCRATCH/nested-tmp"
if TMPDIR="$SCRATCH/nested-tmp" bash -c 'source "$1"; oxid_standalone_state_directory "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE/nested" \
  >/dev/null 2>"$SCRATCH/nested-topology.log"; then
  echo "standalone-state-lifecycle: FAIL nested launcher selected enclosing Git state" >&2
  exit 1
fi
grep -q 'not the checkout Git top level' "$SCRATCH/nested-topology.log"
if OXID_STANDALONE_STATE_DIR=relative/state bash -c \
  'source "$1"; oxid_standalone_state_directory "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE" >/dev/null 2>&1; then
  echo "standalone-state-lifecycle: FAIL relative override admitted" >&2
  exit 1
fi
if OXID_STANDALONE_STATE_DIR=/ bash -c \
  'source "$1"; oxid_standalone_state_directory "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE" >/dev/null 2>&1; then
  echo "standalone-state-lifecycle: FAIL root override admitted" >&2
  exit 1
fi
ln -s "$SCRATCH/missing" "$SCRATCH/dangling"
if OXID_STANDALONE_STATE_DIR="$SCRATCH/dangling/standalone" bash -c \
  'source "$1"; oxid_standalone_state_directory "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE" >/dev/null 2>&1; then
  echo "standalone-state-lifecycle: FAIL dangling-symlink override admitted" >&2
  exit 1
fi

: >"$MOCK_STATE/active"
if TMPDIR="$LAUNCHER_TWO" "$FIXTURE/scripts/standalone-down.sh" >/dev/null 2>&1; then
  echo "standalone-state-lifecycle: FAIL missing receipt admitted" >&2
  exit 1
fi
[ -f "$MOCK_STATE/active" ]
rm -f -- "$MOCK_STATE/active"

mv "$STATE_DIRECTORY" "$STATE_DIRECTORY.real"
ln -s "$STATE_DIRECTORY.real" "$STATE_DIRECTORY"
if TMPDIR="$LAUNCHER_TWO" "$FIXTURE/scripts/standalone-down.sh" \
  >/dev/null 2>"$SCRATCH/symlink-rejection.log"; then
  echo "standalone-state-lifecycle: FAIL symlinked state admitted" >&2
  exit 1
fi
grep -q 'Standalone state directory must not be a symlink' "$SCRATCH/symlink-rejection.log"
if OXID_STANDALONE_STATE_DIR="$STATE_DIRECTORY/" bash -c \
  'source "$1"; oxid_standalone_state_directory "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE" >/dev/null 2>&1; then
  echo "standalone-state-lifecycle: FAIL trailing-slash symlink admitted" >&2
  exit 1
fi

rm "$STATE_DIRECTORY"
mkdir -p "$STATE_DIRECTORY"
legacy_tmp="$SCRATCH/legacy-tmp"
mkdir -p "$legacy_tmp/oxid-standalone"
legacy_resolution="$(TMPDIR="$legacy_tmp" bash -c 'source "$1"; oxid_standalone_state_directory "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE" \
  2>"$SCRATCH/legacy-state.log")"
[ "$legacy_resolution" = "$PHYSICAL_SCRATCH/repository/.git/oxid/standalone" ]
grep -q 'Legacy standalone state detected' "$SCRATCH/legacy-state.log"
rm -rf -- "$legacy_tmp"

stale_candidate="$STATE_DIRECTORY/.startup-lease-candidate-old"
stale_quarantine="$STATE_DIRECTORY/.startup-lease-stale-old"
live_candidate="$STATE_DIRECTORY/.startup-lease-candidate-live"
printf 'stale\n' >"$stale_candidate"
printf 'stale\n' >"$stale_quarantine"
printf 'live\n' >"$live_candidate"
touch -t 202001010000 "$stale_candidate" "$stale_quarantine"
bash -c 'source "$1"; oxid_standalone_cleanup_lease_artifacts "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$STATE_DIRECTORY"
[ ! -e "$stale_candidate" ]
[ ! -e "$stale_quarantine" ]
[ -f "$live_candidate" ]

process_identity="$(bash -c 'source "$1"; oxid_standalone_process_start "$$"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh")"
[ -n "$process_identity" ]
case "$process_identity" in proc:*|lstart:*) ;; *) exit 1 ;; esac
synthetic_proc_start="$(bash -c 'source "$1"; oxid_standalone_process_start_from_proc_stat "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" \
  '42 (worker name ) with spaces) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 424242 20')"
[ "$synthetic_proc_start" = 424242 ]

ANCESTOR="$SCRATCH/ancestor"
mkdir -p "$ANCESTOR/real"
ln -s "$ANCESTOR/real" "$ANCESTOR/link"
resolved_ancestor="$(OXID_STANDALONE_STATE_DIR="$ANCESTOR/link/standalone" bash -c \
  'source "$1"; oxid_standalone_state_directory "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE")"
[ "$resolved_ancestor" = "$PHYSICAL_SCRATCH/ancestor/real/standalone" ]

printf '%s\n' \
  'standalone-state-lifecycle: PASS durable Git state rejected foreign labels and nested topology, detected legacy state, and bounded stale lease cleanup'
