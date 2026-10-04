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
      printf '%s\n' indexer-id node-id proof-id
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

TMPDIR="$LAUNCHER_ONE" "$FIXTURE/scripts/standalone-up.sh" local >/dev/null
STATE_DIRECTORY="$FIXTURE/.git/oxid/standalone"
[ -f "$STATE_DIRECTORY/owner-receipt.json" ]
[ -f "$STATE_DIRECTORY/canonical-compose.yml" ]
rm -rf -- "$LAUNCHER_ONE"

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
if OXID_STANDALONE_STATE_DIR=relative/state bash -c \
  'source "$1"; oxid_standalone_state_directory "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE" >/dev/null 2>&1; then
  echo "standalone-state-lifecycle: FAIL relative override admitted" >&2
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
ANCESTOR="$SCRATCH/ancestor"
mkdir -p "$ANCESTOR/real"
ln -s "$ANCESTOR/real" "$ANCESTOR/link"
resolved_ancestor="$(OXID_STANDALONE_STATE_DIR="$ANCESTOR/link/standalone" bash -c \
  'source "$1"; oxid_standalone_state_directory "$2"' _ \
  "$FIXTURE/scripts/lib/standalone-state.sh" "$FIXTURE")"
[ "$resolved_ancestor" = "$PHYSICAL_SCRATCH/ancestor/real/standalone" ]

printf '%s\n' \
  'standalone-state-lifecycle: PASS durable Git state survived launcher exit and exact teardown preserved foreign projects'
