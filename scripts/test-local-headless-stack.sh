#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
export LC_ALL=C

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
portal_helper_root="${OXID_TEST_PORTAL_HELPER_ROOT:-}"
[ -n "$portal_helper_root" ] || { echo "Set OXID_TEST_PORTAL_HELPER_ROOT to the authenticated Portal helper root." >&2; exit 2; }
protocol_commit="925ec8d04882eabd4ac7b784c70fc2f0c152faae"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/oxid-local-headless-test.XXXXXX")"
scratch="$(cd -- "$scratch" && pwd -P)"
cleanup() { rm -rf -- "$scratch"; }
trap cleanup EXIT INT TERM

pass=0
fail=0
test_case() {
  local name="$1"; shift
  if "$@"; then
    printf 'ok - %s\n' "$name"
    pass=$((pass + 1))
  else
    printf 'not ok - %s\n' "$name" >&2
    fail=$((fail + 1))
  fi
}
assert_eq() { [ "$1" = "$2" ]; }
assert_file_absent() { [ ! -e "$1" ] && [ ! -L "$1" ]; }
file_mode() { stat -f '%Lp' "$1" 2>/dev/null || stat -c '%a' "$1"; }

for path in \
  scripts/e2e/stack-env-v1.sh \
  scripts/e2e/validate-stack-env-v1.sh \
  scripts/standalone-lifecycle.sh \
  scripts/local-headless.sh \
  scripts/standalone-stack.yml; do
  [ -f "$repository_root/$path" ] || {
    printf 'not ok - production file missing: %s\n' "$path" >&2
    exit 1
  }
done
[ -x "$portal_helper_root/scripts/init-oxid-conformance-env.sh" ] || exit 1

oxid_root="$scratch/oxid"
mkdir -p "$oxid_root/scripts/e2e"
cp "$repository_root/scripts/e2e/stack-env-v1.sh" "$oxid_root/scripts/e2e/"
cp "$repository_root/scripts/e2e/validate-stack-env-v1.sh" "$oxid_root/scripts/e2e/"
cp "$repository_root/scripts/standalone-lifecycle.sh" "$oxid_root/scripts/"
cp "$repository_root/scripts/local-headless.sh" "$oxid_root/scripts/"
cp "$repository_root/scripts/standalone-stack.yml" "$oxid_root/scripts/"
chmod +x "$oxid_root/scripts/e2e/validate-stack-env-v1.sh" \
  "$oxid_root/scripts/standalone-lifecycle.sh" "$oxid_root/scripts/local-headless.sh"
git -C "$oxid_root" init -q -b issue-test
git -C "$oxid_root" config user.name 'Oxid test'
git -C "$oxid_root" config user.email 'oxid-test@example.invalid'
git -C "$oxid_root" add .
git -C "$oxid_root" commit -q -m 'test fixture'

protocol_source="$scratch/portal-protocol"
git clone -q --shared --no-checkout "$portal_helper_root" "$protocol_source"
git -C "$protocol_source" checkout -q --detach "$protocol_commit"

fake_bin="$scratch/bin"
mkdir -p "$fake_bin"
docker_state="$scratch/docker-state"
docker_log="$scratch/docker-log"
image_counter="$scratch/image-counter"
printf 'midnight=0\nportal=0\n' >"$docker_state"
: >"$docker_log"
printf '0\n' >"$image_counter"

cat >"$fake_bin/docker" <<'FAKE_DOCKER'
#!/usr/bin/env bash
set -euo pipefail
state_file="${OXID_TEST_DOCKER_STATE:?}"
log_file="${OXID_TEST_DOCKER_LOG:?}"
get_state() { awk -F= -v key="$1" '$1 == key { print $2 }' "$state_file"; }
set_state() {
  local key="$1" value="$2" tmp="${state_file}.tmp"
  awk -F= -v key="$key" -v value="$value" 'BEGIN{OFS="="} $1 == key {$2=value} {print}' "$state_file" >"$tmp"
  mv "$tmp" "$state_file"
}
project_from_args() {
  local previous="" value
  for value in "$@"; do
    if [ "$previous" = -p ]; then printf '%s\n' "$value"; return; fi
    case "$value" in label=com.docker.compose.project=*) printf '%s\n' "${value##*=}"; return ;; esac
    previous="$value"
  done
}
resource_state() {
  local project="$1"
  if [ "$project" = oxid-standalone ]; then get_state midnight; else get_state portal; fi
}
if [ "${1:-}" = info ]; then exit 0; fi
if [ "${1:-}" = load ]; then
  count="$(<"${OXID_TEST_IMAGE_COUNTER:?}")"; count=$((count + 1)); printf '%s\n' "$count" >"${OXID_TEST_IMAGE_COUNTER:?}"
  case "$count" in
    1) printf 'Loaded image: midnight-did-resolver:local\n' ;;
    2) printf 'Loaded image: laceid-did-manager:local\n' ;;
    *) printf 'Loaded image: laceid-issuer:local\n' ;;
  esac
  cat >/dev/null
  exit 0
fi
if [ "${1:-}" = compose ]; then
  project="$(project_from_args "$@")"
  operation=""
  for value in "$@"; do case "$value" in up|down|logs|ps|exec) operation="$value"; break ;; esac; done
  case "$operation" in
    up)
      if [ "$project" = oxid-standalone ]; then set_state midnight 1; else set_state portal 1; fi
      printf 'up:%s\n' "$project" >>"$log_file" ;;
    down)
      if [ "$project" = oxid-standalone ]; then set_state midnight 0; else set_state portal 0; fi
      printf 'down:%s\n' "$project" >>"$log_file" ;;
    logs) : ;;
  esac
  exit 0
fi
project="$(project_from_args "$@")"
running="$(resource_state "$project")"
case "${1:-} ${2:-}" in
  'ps -a'|'ps --filter'|'ps '*)
    if [ "$running" = 1 ]; then
      if [ "$project" = oxid-standalone ]; then printf 'midnight-node\nmidnight-indexer\nmidnight-proof\n'; else printf 'portal-one\nportal-two\nportal-three\nportal-four\nportal-five\n'; fi
    fi ;;
  'network ls') [ "$running" = 0 ] || printf '%s-network\n' "$project" ;;
  'volume ls') [ "$running" = 0 ] || printf '%s-volume\n' "$project" ;;
  'container ls') [ "$running" = 0 ] || printf '%s-issuer\n' "$project" ;;
esac
FAKE_DOCKER
chmod +x "$fake_bin/docker"

cat >"$fake_bin/curl" <<'FAKE_CURL'
#!/usr/bin/env bash
set -euo pipefail
case " $* " in
  *chain_getHeader*) printf '{"result":{"number":"0x10"}}\n' ;;
  *) : ;;
esac
FAKE_CURL
chmod +x "$fake_bin/curl"

cat >"$fake_bin/nix" <<'FAKE_NIX'
#!/usr/bin/env bash
set -euo pipefail
artifact="${OXID_TEST_NIX_ARTIFACT:?}"
: >"$artifact"
printf '%s\n' "$artifact"
FAKE_NIX
chmod +x "$fake_bin/nix"
: >"$scratch/nix-artifact"

export PATH="$fake_bin:$PATH"
export OXID_TEST_DOCKER_STATE="$docker_state"
export OXID_TEST_DOCKER_LOG="$docker_log"
export OXID_TEST_IMAGE_COUNTER="$image_counter"
export OXID_TEST_NIX_ARTIFACT="$scratch/nix-artifact"

new_profile() {
  local name="$1" project="$2" profile
  profile="$scratch/$name.env"
  "$portal_helper_root/scripts/init-oxid-conformance-env.sh" \
    --output "$profile" --oxid-root "$oxid_root" --portal-source "$protocol_source" \
    --project "$project" >/dev/null
  printf '%s\n' "$profile"
}
replace_line() {
  local file="$1" key="$2" value="$3" tmp
  tmp="${file}.tmp"
  awk -F= -v key="$key" -v value="$value" 'BEGIN{OFS="="} $1 == key {$0=key "=" value} {print}' "$file" >"$tmp"
  chmod 600 "$tmp" && mv "$tmp" "$file"
}
run_validate() {
  local profile="$1"
  "$oxid_root/scripts/e2e/validate-stack-env-v1.sh" "$profile" >"$scratch/stdout" 2>"$scratch/stderr"
}
run_local() {
  local operation="$1" profile="$2"
  "$oxid_root/scripts/local-headless.sh" "$operation" "$profile" >"$scratch/stdout" 2>"$scratch/stderr"
}

base_profile="$(new_profile base oxidportal124headless)"
case_valid_profile() {
  run_validate "$base_profile" &&
    jq -e '.schema == "oxid-stack-env-validation-v1" and .valid == true' "$scratch/stdout" >/dev/null &&
    [ ! -s "$scratch/stderr" ]
}
case_duplicate_key() {
  local candidate="$scratch/duplicate.env"
  cp "$base_profile" "$candidate"; printf 'STACK_SCHEMA=oxid-laceid-headless-v1\n' >>"$candidate"; chmod 600 "$candidate"
  ! run_validate "$candidate" && grep -qx 'stack-env: error=invalid_stack_env' "$scratch/stderr"
}
case_unknown_key() {
  local candidate="$scratch/unknown.env"
  cp "$base_profile" "$candidate"; printf 'UNREVIEWED_VALUE=closed\n' >>"$candidate"; chmod 600 "$candidate"
  ! run_validate "$candidate" && grep -qx 'stack-env: error=invalid_stack_env' "$scratch/stderr"
}
case_unsafe_mode() {
  local candidate="$scratch/public.env"
  cp "$base_profile" "$candidate"; chmod 644 "$candidate"
  ! run_validate "$candidate" && grep -qx 'stack-env: error=invalid_stack_env' "$scratch/stderr"
}
case_symlink() {
  local candidate="$scratch/link.env"
  ln -s "$base_profile" "$candidate"
  ! run_validate "$candidate" && grep -qx 'stack-env: error=invalid_stack_env' "$scratch/stderr"
}
case_noncanonical_path() {
  mkdir -p "$scratch/nested"
  ! run_validate "$scratch/nested/../base.env" && grep -qx 'stack-env: error=invalid_stack_env' "$scratch/stderr"
}
case_fixed_route() {
  local candidate="$scratch/route.env"
  cp "$base_profile" "$candidate"; replace_line "$candidate" SHARED_MIDNIGHT_INDEXER_V4_HOST_URL http://127.0.0.1:8088/api/v5/graphql
  ! run_validate "$candidate" && grep -qx 'stack-env: error=invalid_stack_env' "$scratch/stderr"
}
case_fixed_project() {
  local candidate="$scratch/project.env"
  cp "$base_profile" "$candidate"; replace_line "$candidate" SHARED_MIDNIGHT_PROJECT other-project
  ! run_validate "$candidate" && grep -qx 'stack-env: error=invalid_stack_env' "$scratch/stderr"
}
case_helper_authentication() {
  local candidate="$scratch/helper.env"
  cp "$base_profile" "$candidate"; replace_line "$candidate" PORTAL_HELPER_TREE ffffffffffffffffffffffffffffffffffffffff
  ! run_validate "$candidate" && grep -qx 'stack-env: error=invalid_helper' "$scratch/stderr"
}
case_secret_sentinel_not_exposed() {
  local candidate="$scratch/sentinel.env" sentinel
  sentinel="DO_NOT_"'EXPOSE_THIS_VALUE_1234567890'
  cp "$base_profile" "$candidate"; replace_line "$candidate" PORTAL_DIDIT_API_KEY "$sentinel"
  run_local status "$candidate" || return 1
  ! grep -Fq "$sentinel" "$scratch/stdout" && ! grep -Fq "$sentinel" "$scratch/stderr"
}
reset_fake() { printf 'midnight=%s\nportal=%s\n' "$1" "$2" >"$docker_state"; : >"$docker_log"; printf '0\n' >"$image_counter"; }
case_owner_lifecycle() {
  local profile
  profile="$(new_profile owner oxidportal124owner)"; reset_fake 0 0
  run_local up "$profile" || return 1
  grep -qx 'up:oxid-standalone' "$docker_log" || return 1
  grep -qx 'up:oxidportal124owner' "$docker_log" || return 1
  [ -f "${profile}.state/oxid-standalone.owner.receipt" ] || return 1
  run_local down "$profile" || return 1
  grep -qx 'down:oxidportal124owner' "$docker_log" || return 1
  grep -qx 'down:oxid-standalone' "$docker_log" || return 1
  assert_file_absent "${profile}.state/oxid-standalone.owner.receipt"
}
case_attach_never_stops_midnight() {
  local profile
  profile="$(new_profile attach oxidportal124attach)"; reset_fake 1 0
  run_local up "$profile" || return 1
  ! grep -qx 'up:oxid-standalone' "$docker_log" || return 1
  assert_file_absent "${profile}.state/oxid-standalone.owner.receipt" || return 1
  run_local down "$profile" || return 1
  ! grep -qx 'down:oxid-standalone' "$docker_log" && assert_eq "$(awk -F= '$1=="midnight"{print $2}' "$docker_state")" 1
}
case_same_private_state_cross_cwd() {
  local profile other="$scratch/other-cwd"
  profile="$(new_profile same-state oxidportal124same)"; reset_fake 0 0; mkdir "$other"
  (cd "$oxid_root" && run_local up "$profile") || return 1
  (cd "$other" && run_local status "$profile") || return 1
  (cd "$other" && run_local down "$profile") || return 1
  grep -qx 'down:oxid-standalone' "$docker_log"
}
case_no_cross_owner_cleanup() {
  local owner consumer consumer_state
  owner="$(new_profile cross-owner oxidportal124ownerx)"; reset_fake 0 0
  run_local up "$owner" || return 1
  consumer="$(new_profile cross-consumer oxidportal124consumerx)"
  consumer_state="${consumer}.state"
  replace_line "$consumer" PORTAL_COMPOSE_PROJECT oxidportal124ownerx
  replace_line "$consumer" LOCAL_STACK_STATE_DIR "$consumer_state"
  run_local down "$consumer" || return 1
  [ "$(awk -F= '$1=="midnight"{print $2}' "$docker_state")" = 1 ] &&
    ! grep -qx 'down:oxid-standalone' "$docker_log"
}

# Input validation/delegation contract.
test_case 'accepts the exact closed owner-private v1 profile' case_valid_profile
test_case 'rejects duplicate keys' case_duplicate_key
test_case 'rejects unknown keys' case_unknown_key
test_case 'rejects group/world-readable profile mode' case_unsafe_mode
test_case 'rejects profile symlinks' case_symlink
test_case 'rejects noncanonical profile paths' case_noncanonical_path
test_case 'rejects drifted fixed routes' case_fixed_route
test_case 'rejects drifted shared project' case_fixed_project
test_case 'authenticates the exact Portal helper tree' case_helper_authentication
test_case 'never emits a Portal secret sentinel' case_secret_sentinel_not_exposed
# Owner versus attach safety contract.
test_case 'exact owner receipt permits exact owner cleanup' case_owner_lifecycle
test_case 'attach shutdown never stops shared Midnight' case_attach_never_stops_midnight
test_case 'one private state receipt agrees across invocation directories' case_same_private_state_cross_cwd
test_case 'a different private-state consumer cannot clean the shared owner' case_no_cross_owner_cleanup

printf 'local-headless-stack tests: passed=%d failed=%d\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
