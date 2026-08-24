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
image_inspect_counter="$scratch/image-inspect-counter"
image_dir="$scratch/images"
image_ids="$scratch/image-ids"
docker_resources="$scratch/docker-resources"
docker_generation="$scratch/docker-generation"
docker_listed="$scratch/docker-listed"
printf 'midnight=0\nportal=0\n' >"$docker_state"
: >"$docker_log"
printf '0\n' >"$image_counter"
printf '0\n' >"$image_inspect_counter"
printf '0\n' >"$docker_generation"
mkdir "$image_dir" "$docker_listed"
: >"$image_ids"
: >"$docker_resources"
for image in resolver did-manager issuer; do
  archive_dir="$image_dir/$image"
  mkdir "$archive_dir"
  printf '{"architecture":"amd64","config":{"Labels":{"oxid.fixture":"%s"}},"os":"linux","rootfs":{"diff_ids":[],"type":"layers"}}\n' \
    "$image" >"$archive_dir/config.json"
  image_hash="$(sha256sum "$archive_dir/config.json" | awk '{print $1}')"
  mv "$archive_dir/config.json" "$archive_dir/$image_hash.json"
  printf '[{"Config":"%s.json","RepoTags":["%s:fixture"],"Layers":[]}]\n' \
    "$image_hash" "$image" >"$archive_dir/manifest.json"
  tar -C "$archive_dir" -cf "$image_dir/$image.tar" manifest.json "$image_hash.json"
  printf 'sha256:%s\n' "$image_hash" >>"$image_ids"
done

cat >"$fake_bin/docker" <<'FAKE_DOCKER'
#!/usr/bin/env bash
set -euo pipefail
state_file="${OXID_TEST_DOCKER_STATE:?}"
log_file="${OXID_TEST_DOCKER_LOG:?}"
resources="${OXID_TEST_DOCKER_RESOURCES:?}"
generation_file="${OXID_TEST_DOCKER_GENERATION:?}"
listed_dir="${OXID_TEST_DOCKER_LISTED:?}"
args=("$@")
set_state() {
  local key="$1" value="$2" tmp="${state_file}.tmp"
  awk -F= -v key="$key" -v value="$value" 'BEGIN{OFS="="} $1 == key {$2=value} {print}' "$state_file" >"$tmp"
  mv "$tmp" "$state_file"
}
remove_project() {
  local project="$1" tmp="${resources}.tmp"
  awk -F '\t' -v project="$project" '$1 != project' "$resources" >"$tmp"
  mv "$tmp" "$resources"
  rm -f -- "$listed_dir/$project.container" "$listed_dir/$project.running" \
    "$listed_dir/$project.network" "$listed_dir/$project.volume"
}
add_project() {
  local project="$1" generation service identity state code
  generation="$(<"$generation_file")"; generation=$((generation + 1)); printf '%s\n' "$generation" >"$generation_file"
  remove_project "$project"
  if [ "$project" = oxid-standalone ]; then
    for service in indexer node proof-server; do
      printf '%s\tcontainer\t%s-g%s-container-%s\t%s\trunning\t0\t\n' \
        "$project" "$project" "$generation" "$service" "$service" >>"$resources"
    done
  else
    for service in did-bootstrap did-manager did-resolver issuer smocker; do
      identity="$service"; state=running
      [ "$service" != did-bootstrap ] || state=exited
      if [ "${OXID_TEST_PORTAL_TOPOLOGY:-exact}" = wrong-service ] && [ "$service" = issuer ]; then
        identity=did-resolver
      fi
      printf '%s\tcontainer\t%s-g%s-container-%s\t%s\t%s\t0\t\n' \
        "$project" "$project" "$generation" "$service" "$identity" "$state" >>"$resources"
    done
    printf '%s\tnetwork\t%s-g%s-network-default\tdefault\t\t\t%s_default\n' \
      "$project" "$project" "$generation" "$project" >>"$resources"
    printf '%s\tvolume\t%s-g%s-volume-bootstrap\tbootstrap\t\t\t%s_bootstrap\n' \
      "$project" "$project" "$generation" "$project" >>"$resources"
  fi
}
compose_project() {
  local i count=0 project=""
  for ((i=0; i<${#args[@]}; i++)); do
    if [ "${args[i]}" = -p ]; then
      ((i + 1 < ${#args[@]})) || return 1
      project="${args[i+1]}"; count=$((count + 1)); i=$((i + 1))
    fi
  done
  [ "$count" -eq 1 ] && [[ "$project" =~ ^[a-z0-9][a-z0-9_-]*$ ]] || return 1
  printf '%s\n' "$project"
}
filtered_project() {
  local i count=0 project="" value
  for ((i=0; i<${#args[@]}; i++)); do
    value="${args[i]}"
    case "$value" in
      --filter)
        ((i + 1 < ${#args[@]})) || return 1
        value="${args[i+1]}"; i=$((i + 1)); count=$((count + 1))
        case "$value" in
          label=com.docker.compose.project=*) project="${value#label=com.docker.compose.project=}" ;;
          *) return 1 ;;
        esac
        ;;
      --filter=*|label=com.docker.compose.project=*) return 1 ;;
    esac
  done
  [ "$count" -eq 1 ] && [[ "$project" =~ ^[a-z0-9][a-z0-9_-]*$ ]] || return 1
  printf '%s\n' "$project"
}
list_resources() {
  local project="$1" kind="$2" running_only="$3" destination
  destination="$listed_dir/$project.$kind"
  if [ "$kind" = container ]; then
    [ "$running_only" -eq 0 ] || destination="$listed_dir/$project.running"
    awk -F '\t' -v project="$project" -v running_only="$running_only" \
      '$1 == project && $2 == "container" && (running_only == 0 || $5 == "running") {print $3}' "$resources" >"$destination"
  else
    awk -F '\t' -v project="$project" -v kind="$kind" '$1 == project && $2 == kind {print $3}' "$resources" >"$destination"
  fi
  cat "$destination"
}
inspect_resources() {
  local kind="$1" first_id project expected actual id row
  local -a ids
  [ "${args[1]:-}" = inspect ] && [ "${args[2]:-}" = --format ] && [ -n "${args[3]:-}" ] || return 1
  ids=("${args[@]:4}"); [ "${#ids[@]}" -gt 0 ] || return 1
  first_id="${ids[0]}"
  project="$(awk -F '\t' -v kind="$kind" -v id="$first_id" '$2 == kind && $3 == id {print $1}' "$resources")"
  [ -n "$project" ] && [[ "$project" != *$'\n'* ]] || return 1
  [ -f "$listed_dir/$project.$kind" ] || return 1
  expected="$(cat "$listed_dir/$project.$kind")"
  actual="$(printf '%s\n' "${ids[@]}")"
  [ "$actual" = "$expected" ] || return 1
  for id in "${ids[@]}"; do
    row="$(awk -F '\t' -v project="$project" -v kind="$kind" -v id="$id" '$1 == project && $2 == kind && $3 == id {print}' "$resources")"
    [ -n "$row" ] && [[ "$row" != *$'\n'* ]] || return 1
    case "$kind" in
      container)
        if [ "${args[3]}" = '{{index .Config.Labels "com.docker.compose.service"}}' ]; then
          awk -F '\t' '{printf "%s\n", $4}' <<<"$row"
        else
          [ "${args[3]}" = '{{index .Config.Labels "com.docker.compose.service"}}{{printf "\t"}}{{.State.Status}}{{printf "\t"}}{{.State.ExitCode}}' ] || return 1
          awk -F '\t' '{printf "%s\t%s\t%s\n", $4, $5, $6}' <<<"$row"
        fi
        ;;
      network|volume) awk -F '\t' '{printf "%s\t%s\n", $7, $4}' <<<"$row" ;;
      *) return 1 ;;
    esac
  done
}
case "${1:-}" in
  info) exit 0 ;;
  pull) exit 0 ;;
  load)
    count="$(<"${OXID_TEST_IMAGE_COUNTER:?}")"; count=$((count + 1))
    case "$count" in
      1) archive="${OXID_TEST_IMAGE_DIR:?}/resolver.tar" ;;
      2) archive="${OXID_TEST_IMAGE_DIR:?}/did-manager.tar" ;;
      3) archive="${OXID_TEST_IMAGE_DIR:?}/issuer.tar" ;;
      *) exit 1 ;;
    esac
    loaded_archive="${OXID_TEST_IMAGE_DIR:?}/loaded-$count.tar"
    cat >"$loaded_archive"
    cmp -s "$loaded_archive" "$archive" || exit 1
    printf '%s\n' "$count" >"${OXID_TEST_IMAGE_COUNTER:?}"
    printf 'Loaded image fixture %s\n' "$count"
    ;;
  image)
    [ "${2:-}" = inspect ] || exit 1
    reference="${5:-}"
    inspect_count="$(<"${OXID_TEST_IMAGE_INSPECT_COUNTER:?}")"; inspect_count=$((inspect_count + 1))
    [ "$inspect_count" -eq "$(<"${OXID_TEST_IMAGE_COUNTER:?}")" ] || exit 1
    expected="$(sed -n "${inspect_count}p" "${OXID_TEST_IMAGE_IDS:?}")"
    [ "$reference" = "$expected" ] || exit 1
    printf '%s\n' "$inspect_count" >"${OXID_TEST_IMAGE_INSPECT_COUNTER:?}"
    printf '%s\n' "$reference"
    ;;
  compose)
    project="$(compose_project)" || exit 1
    operation=""
    for value in "$@"; do case "$value" in up|down|logs|ps|exec) operation="$value"; break ;; esac; done
    case "$operation" in
      up)
        add_project "$project"
        if [ "$project" = oxid-standalone ]; then set_state midnight 1; else set_state portal 1; fi
        printf 'up:%s\n' "$project" >>"$log_file" ;;
      down)
        remove_project "$project"
        if [ "$project" = oxid-standalone ]; then set_state midnight 0; else set_state portal 0; fi
        printf 'down:%s\n' "$project" >>"$log_file" ;;
      logs|ps|exec) : ;;
      *) exit 1 ;;
    esac
    ;;
  ps)
    project="$(filtered_project)" || exit 1
    running_only=1
    for value in "$@"; do [ "$value" != -a ] || running_only=0; done
    if [ "$running_only" -eq 0 ]; then list_resources "$project" container 0
    else list_resources "$project" container 1
    fi
    ;;
  container)
    inspect_resources container || exit 1
    ;;
  network|volume)
    kind="$1"; operation="${2:-}"
    if [ "$operation" = ls ]; then
      project="$(filtered_project)" || exit 1
      list_resources "$project" "$kind" 0
    elif [ "$operation" = inspect ]; then
      inspect_resources "$kind" || exit 1
    else
      exit 1
    fi
    ;;
  *) exit 1 ;;
esac
FAKE_DOCKER
chmod +x "$fake_bin/docker"

cat >"$fake_bin/curl" <<'FAKE_CURL'
#!/usr/bin/env bash
set -euo pipefail
output="" data="" url="" previous="" argument=""
for argument in "$@"; do
  if [ "$previous" = output ]; then output="$argument"; previous=""; continue; fi
  if [ "$previous" = data ]; then data="$argument"; previous=""; continue; fi
  case "$argument" in
    --output|-o) previous=output ;;
    --output=*) output="${argument#--output=}" ;;
    -o?*) output="${argument#-o}" ;;
    --data|--data-raw|--data-binary|-d) previous=data ;;
    --data=*|--data-raw=*|--data-binary=*) data="${argument#*=}" ;;
    -d?*) data="${argument#-d}" ;;
    http://*|https://*) url="$argument" ;;
  esac
done
[ -z "$previous" ] && [ -n "$url" ] || exit 2
response=""
case "$url" in
  */api/v3/graphql|*/api/v4/graphql)
    case "$data" in
      '{"query":"query OxidSharedReadiness { block { height } }"}')
        [ "$output" = /dev/null ] || exit 22
        ;;
      '{"query":"query PortalSharedReadiness { block { height } }"}')
        [ -z "$output" ] || exit 22
        response='{"data":{"block":{"height":16}},"errors":[]}'
        ;;
      *) exit 22 ;;
    esac
    ;;
  http://127.0.0.1:9944)
    [ "$data" = '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' ] || exit 22
    response='{"result":{"number":"0x10"}}'
    ;;
  */mocks\?reset=true)
    [[ "$data" == @*didit.yml ]] || exit 22
    ;;
  */health|*/ready|http://127.0.0.1:6300/)
    [ -z "$data" ] || exit 22
    ;;
  *) exit 22 ;;
esac
if [ -n "$output" ]; then
  [ "$output" = /dev/null ] || printf '%s\n' "$response" >"$output"
elif [ -n "$response" ]; then
  printf '%s\n' "$response"
fi
FAKE_CURL
chmod +x "$fake_bin/curl"

cat >"$fake_bin/nix" <<'FAKE_NIX'
#!/usr/bin/env bash
set -euo pipefail
case " $* " in
  *midnight-did-resolver-image*) artifact="${OXID_TEST_IMAGE_DIR:?}/resolver.tar" ;;
  *did-manager-image*) artifact="${OXID_TEST_IMAGE_DIR:?}/did-manager.tar" ;;
  *issuer-image*) artifact="${OXID_TEST_IMAGE_DIR:?}/issuer.tar" ;;
  *) exit 1 ;;
esac
[ -f "$artifact" ] || exit 1
printf '%s\n' "$artifact"
FAKE_NIX
chmod +x "$fake_bin/nix"

export PATH="$fake_bin:$PATH"
export OXID_TEST_DOCKER_STATE="$docker_state"
export OXID_TEST_DOCKER_LOG="$docker_log"
export OXID_TEST_IMAGE_COUNTER="$image_counter"
export OXID_TEST_IMAGE_INSPECT_COUNTER="$image_inspect_counter"
export OXID_TEST_IMAGE_DIR="$image_dir"
export OXID_TEST_IMAGE_IDS="$image_ids"
export OXID_TEST_DOCKER_RESOURCES="$docker_resources"
export OXID_TEST_DOCKER_GENERATION="$docker_generation"
export OXID_TEST_DOCKER_LISTED="$docker_listed"
export OXID_TEST_PORTAL_TOPOLOGY=exact

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
assert_combined_result() {
  local profile="$1" operation="$2" midnight_operation="$3" midnight_state="$4" midnight_ownership="$5"
  local portal_state="$6" portal_midnight_state="$7" midnight_containers="$8" portal_containers="$9"
  local portal_project
  portal_project="$(awk -F= '$1 == "PORTAL_COMPOSE_PROJECT" {print $2}' "$profile")" || return 1
  [ ! -s "$scratch/stderr" ] || return 1
  jq -se \
    --arg operation "$operation" \
    --arg midnight_operation "$midnight_operation" \
    --arg midnight_state "$midnight_state" \
    --arg midnight_ownership "$midnight_ownership" \
    --arg portal_state "$portal_state" \
    --arg portal_midnight_state "$portal_midnight_state" \
    --arg portal_project "$portal_project" \
    --argjson midnight_containers "$midnight_containers" \
    --argjson portal_containers "$portal_containers" '
      length == 1
      and (.[0] | (
        type == "object"
        and keys == ["midnight","operation","portal","profile","schema"]
        and .schema == "oxid-local-headless-lifecycle-v1"
        and .operation == $operation
        and .profile == "headless"
        and (.midnight | (
          type == "object"
          and keys == ["containers","operation","ownership","profile","project","schema","state"]
          and .schema == "oxid-standalone-lifecycle-v2"
          and .operation == $midnight_operation
          and .profile == "headless"
          and .project == "oxid-standalone"
          and .state == $midnight_state
          and .ownership == $midnight_ownership
          and .containers == $midnight_containers
        ))
        and (.portal | (
          type == "object"
          and keys == ["containers","midnight_state","networks","operation","profile","project","running_containers","schema","state","volumes"]
          and .schema == "laceid-oxid-conformance-lifecycle-v2"
          and .operation == $operation
          and .profile == "headless"
          and .project == $portal_project
          and .midnight_state == $portal_midnight_state
          and .state == $portal_state
          and .containers == $portal_containers
          and .running_containers == (if $portal_state == "running" then 4 else 0 end)
          and .networks == (if $portal_state == "running" then 1 else 0 end)
          and .volumes == (if $portal_state == "running" then 1 else 0 end)
        ))
      ))
    ' "$scratch/stdout" >/dev/null
}
assert_no_combined_result() {
  [ ! -s "$scratch/stdout" ] &&
    ! grep -Fq 'oxid-local-headless-lifecycle-v1' "$scratch/stdout" "$scratch/stderr" &&
    [ "$(wc -l <"$scratch/stderr" | tr -d ' ')" -eq 2 ] &&
    grep -qx 'oxid-conformance: error=up_not_running' "$scratch/stderr" &&
    grep -qx 'local-headless: error=portal_up_failed' "$scratch/stderr"
}
assert_continuity_receipt() {
  local profile="$1" project ids
  project="$(awk -F= '$1 == "PORTAL_COMPOSE_PROJECT" {print $2}' "$profile")" || return 1
  [ "$(sed -n '1p' "${profile}.state/$project.shared-midnight.receipt")" = oxid-laceid-shared-receipt-v1 ] || return 1
  [[ "$(sed -n '2p' "${profile}.state/$project.shared-midnight.receipt")" =~ ^[0-9]+$ ]] || return 1
  ids="$(sed -n '3,$p' "${profile}.state/$project.shared-midnight.receipt")"
  [ "$(printf '%s\n' "$ids" | awk 'NF {count++} END {print count + 0}')" -eq 3 ]
}

base_profile="$(new_profile base oxidportal124headless)"
case_valid_profile() {
  local secrets
  secrets="$(awk -F= '/^PORTAL_(DID_MANAGER_API_KEY|DID_MANAGER_CONTROLLER_API_KEY|ISSUER_SESSION_TOKEN_SECRET|DIDIT_API_KEY)=/ {print $2}' "$base_profile")"
  [ "$(printf '%s\n' "$secrets" | grep -Ec '^[0-9a-f]{64}$')" -eq 4 ] &&
    [ "$(printf '%s\n' "$secrets" | sort -u | awk 'END {print NR}')" -eq 4 ] &&
    [ "$(printf '%s\n' "$secrets" | grep -Evc '^([0-9a-f])\1{63}$')" -eq 4 ] &&
    assert_fake_boundaries &&
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
  sentinel="deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
  cp "$base_profile" "$candidate"; replace_line "$candidate" PORTAL_DIDIT_API_KEY "$sentinel"
  run_local status "$candidate" || return 1
  assert_combined_result "$candidate" status status stopped attach stopped unavailable 0 0 || return 1
  ! grep -Fq "$sentinel" "$scratch/stdout" && ! grep -Fq "$sentinel" "$scratch/stderr"
}
seed_project_resources() {
  local project="$1" service state
  if [ "$project" = oxid-standalone ]; then
    for service in indexer node proof-server; do
      printf '%s\tcontainer\t%s-fixture-container-%s\t%s\trunning\t0\t\n' \
        "$project" "$project" "$service" "$service" >>"$docker_resources"
    done
  else
    for service in did-bootstrap did-manager did-resolver issuer smocker; do
      state=running; [ "$service" != did-bootstrap ] || state=exited
      printf '%s\tcontainer\t%s-fixture-container-%s\t%s\t%s\t0\t\n' \
        "$project" "$project" "$service" "$service" "$state" >>"$docker_resources"
    done
    printf '%s\tnetwork\t%s-fixture-network-default\tdefault\t\t\t%s_default\n' \
      "$project" "$project" "$project" >>"$docker_resources"
    printf '%s\tvolume\t%s-fixture-volume-bootstrap\tbootstrap\t\t\t%s_bootstrap\n' \
      "$project" "$project" "$project" >>"$docker_resources"
  fi
}
reset_fake() {
  printf 'midnight=%s\nportal=%s\n' "$1" "$2" >"$docker_state"
  : >"$docker_log"
  : >"$docker_resources"
  rm -f -- "$docker_listed"/*
  printf '0\n' >"$docker_generation"
  [ "$1" -eq 0 ] || seed_project_resources oxid-standalone
  printf '0\n' >"$image_counter"
  printf '0\n' >"$image_inspect_counter"
  rm -f -- "$image_dir"/loaded-*.tar
}
assert_fake_boundaries() {
  local response output="$scratch/curl-output" listed_a listed_b network_a volume_a
  local project_a=oxidportalboundarya project_b=oxidportalboundaryb
  local rpc='{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}'
  local oxid_query='{"query":"query OxidSharedReadiness { block { height } }"}'
  local portal_query='{"query":"query PortalSharedReadiness { block { height } }"}'
  local format='{{index .Config.Labels "com.docker.compose.service"}}{{printf "\t"}}{{.State.Status}}{{printf "\t"}}{{.State.ExitCode}}'
  local network_format='{{.Name}}{{printf "\t"}}{{index .Labels "com.docker.compose.network"}}'
  local volume_format='{{.Name}}{{printf "\t"}}{{index .Labels "com.docker.compose.volume"}}'
  local -a listed_a_ids listed_b_ids network_a_ids volume_a_ids

  response="$(curl --fail --silent --output /dev/null --data "$oxid_query" http://127.0.0.1:8088/api/v3/graphql)" || return 1
  [ -z "$response" ] || return 1
  response="$(curl --fail --silent --data "$portal_query" http://127.0.0.1:8088/api/v4/graphql)" || return 1
  [ "$response" = '{"data":{"block":{"height":16}},"errors":[]}' ] || return 1
  ! curl --fail --silent --data '{"query":"query Wrong { block { height } }"}' http://127.0.0.1:8088/api/v3/graphql >/dev/null 2>&1 || return 1
  ! curl --fail --silent http://127.0.0.1:8088/api/v4/graphql >/dev/null 2>&1 || return 1
  response="$(curl --fail --silent -o "$output" --data "$rpc" http://127.0.0.1:9944)" || return 1
  [ -z "$response" ] && grep -qx '{"result":{"number":"0x10"}}' "$output" || return 1

  reset_fake 0 0
  seed_project_resources "$project_a"; seed_project_resources "$project_b"
  ! docker ps -a label=com.docker.compose.project="$project_a" --quiet >/dev/null 2>&1 || return 1
  ! docker ps -a --filter label=wrong.project="$project_a" --quiet >/dev/null 2>&1 || return 1
  ! docker network ls --filter=label=com.docker.compose.project="$project_a" --quiet >/dev/null 2>&1 || return 1
  listed_a="$(docker ps -a --filter label=com.docker.compose.project="$project_a" --quiet)" || return 1
  listed_b="$(docker ps -a --filter label=com.docker.compose.project="$project_b" --quiet)" || return 1
  mapfile -t listed_a_ids <<<"$listed_a"; mapfile -t listed_b_ids <<<"$listed_b"
  docker container inspect --format "$format" "${listed_a_ids[@]}" >/dev/null || return 1
  ! docker container inspect --format "$format" "${listed_a_ids[@]:0:4}" unknown-container >/dev/null 2>&1 || return 1
  ! docker container inspect --format "$format" "${listed_a_ids[@]:0:4}" "${listed_b_ids[4]}" >/dev/null 2>&1 || return 1
  network_a="$(docker network ls --filter label=com.docker.compose.project="$project_a" --quiet)" || return 1
  volume_a="$(docker volume ls --filter label=com.docker.compose.project="$project_a" --quiet)" || return 1
  mapfile -t network_a_ids <<<"$network_a"; mapfile -t volume_a_ids <<<"$volume_a"
  docker network inspect --format "$network_format" "${network_a_ids[@]}" >/dev/null || return 1
  docker volume inspect --format "$volume_format" "${volume_a_ids[@]}" >/dev/null || return 1
  docker compose -p "$project_a" -f fixture up -d >/dev/null || return 1
  ! docker container inspect --format "$format" "${listed_a_ids[@]}" >/dev/null 2>&1 || return 1
  reset_fake 0 0
}
case_owner_lifecycle() {
  local profile
  profile="$(new_profile owner oxidportal124owner)"; reset_fake 0 0
  run_local up "$profile" || return 1
  assert_combined_result "$profile" up ensure ready owner running ready 3 5 || return 1
  grep -qx 'up:oxid-standalone' "$docker_log" || return 1
  grep -qx 'up:oxidportal124owner' "$docker_log" || return 1
  [ -f "${profile}.state/oxid-standalone.owner.receipt" ] || return 1
  run_local down "$profile" || return 1
  assert_combined_result "$profile" down down stopped owner stopped ready 0 0 || return 1
  grep -qx 'down:oxidportal124owner' "$docker_log" || return 1
  grep -qx 'down:oxid-standalone' "$docker_log" || return 1
  assert_file_absent "${profile}.state/oxid-standalone.owner.receipt"
}
case_attach_never_stops_midnight() {
  local profile
  profile="$(new_profile attach oxidportal124attach)"; reset_fake 1 0
  run_local up "$profile" || return 1
  assert_combined_result "$profile" up ensure ready attach running ready 3 5 || return 1
  ! grep -qx 'up:oxid-standalone' "$docker_log" || return 1
  assert_file_absent "${profile}.state/oxid-standalone.owner.receipt" || return 1
  run_local down "$profile" || return 1
  assert_combined_result "$profile" down down ready attach stopped ready 3 0 || return 1
  ! grep -qx 'down:oxid-standalone' "$docker_log" && assert_eq "$(awk -F= '$1=="midnight"{print $2}' "$docker_state")" 1
}
case_same_private_state_cross_cwd() {
  local profile other="$scratch/other-cwd"
  profile="$(new_profile same-state oxidportal124same)"; reset_fake 0 0; mkdir "$other"
  (cd "$oxid_root" && run_local up "$profile") || return 1
  assert_combined_result "$profile" up ensure ready owner running ready 3 5 || return 1
  (cd "$other" && run_local status "$profile") || return 1
  assert_combined_result "$profile" status status ready owner running ready 3 5 || return 1
  (cd "$other" && run_local down "$profile") || return 1
  assert_combined_result "$profile" down down stopped owner stopped ready 0 0 || return 1
  grep -qx 'down:oxid-standalone' "$docker_log"
}
case_no_cross_owner_cleanup() {
  local owner consumer consumer_state
  owner="$(new_profile cross-owner oxidportal124ownerx)"; reset_fake 0 0
  run_local up "$owner" || return 1
  assert_combined_result "$owner" up ensure ready owner running ready 3 5 || return 1
  consumer="$(new_profile cross-consumer oxidportal124consumerx)"
  consumer_state="${consumer}.state"
  replace_line "$consumer" PORTAL_COMPOSE_PROJECT oxidportal124ownerx
  replace_line "$consumer" LOCAL_STACK_STATE_DIR "$consumer_state"
  run_local down "$consumer" || return 1
  assert_combined_result "$consumer" down down ready attach stopped ready 3 0 || return 1
  [ "$(awk -F= '$1=="midnight"{print $2}' "$docker_state")" = 1 ] &&
    ! grep -qx 'down:oxid-standalone' "$docker_log"
}
case_wrong_same_count_portal_topology() {
  local profile receipt

  profile="$(new_profile wrong-attach oxidportal124wrongattach)"; reset_fake 1 0
  export OXID_TEST_PORTAL_TOPOLOGY=wrong-service
  ! run_local up "$profile" || return 1
  assert_no_combined_result || return 1
  grep -qx 'up:oxidportal124wrongattach' "$docker_log" || return 1
  grep -qx 'down:oxidportal124wrongattach' "$docker_log" || return 1
  ! grep -q '^down:oxid-standalone$' "$docker_log" || return 1
  [ "$(awk -F= '$1=="midnight"{print $2}' "$docker_state")" = 1 ] || return 1
  [ "$(awk -F= '$1=="portal"{print $2}' "$docker_state")" = 0 ] || return 1
  assert_file_absent "${profile}.state/oxid-standalone.owner.receipt" || return 1
  assert_continuity_receipt "$profile" || return 1

  profile="$(new_profile wrong-owner oxidportal124wrongowner)"; reset_fake 0 0
  ! run_local up "$profile" || return 1
  assert_no_combined_result || return 1
  grep -qx 'up:oxid-standalone' "$docker_log" || return 1
  grep -qx 'up:oxidportal124wrongowner' "$docker_log" || return 1
  grep -qx 'down:oxidportal124wrongowner' "$docker_log" || return 1
  grep -qx 'down:oxid-standalone' "$docker_log" || return 1
  [ "$(awk -F= '$1=="midnight"{print $2}' "$docker_state")" = 0 ] || return 1
  [ "$(awk -F= '$1=="portal"{print $2}' "$docker_state")" = 0 ] || return 1
  assert_file_absent "${profile}.state/oxid-standalone.owner.receipt" || return 1
  assert_continuity_receipt "$profile" || return 1
  receipt="${profile}.state/oxidportal124wrongowner.shared-midnight.receipt"
  [ -f "$receipt" ] && [ ! -L "$receipt" ]
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
test_case 'wrong same-count Portal topology fails closed with owner-safe rollback' case_wrong_same_count_portal_topology

printf 'local-headless-stack tests: passed=%d failed=%d\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
