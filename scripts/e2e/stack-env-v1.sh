# SPDX-License-Identifier: Apache-2.0
# Strict public-field loader for the shared Oxid + LaceID headless profile.
# The dotenv file is data: this file never sources/evaluates it and deliberately
# never assigns or exports Portal secret values.

readonly STACK_ENV_EXPECTED_HELPER_COMMIT="00d3d6c6b9ebe37e1a4bffc4dd7a3f27cf6e4b24"
readonly STACK_ENV_EXPECTED_HELPER_TREE="3cecc6e17d56b2c0d646150df3861005df831ed8"
readonly STACK_ENV_EXPECTED_PROTOCOL_COMMIT="925ec8d04882eabd4ac7b784c70fc2f0c152faae"
readonly STACK_ENV_EXPECTED_PROTOCOL_TREE="58b4597524f88a0ae2253439a44dab0dc60cbb6f"
readonly STACK_ENV_EXPECTED_PROTOCOL_PR_HEAD="9c82db23eabe8b6d758b2731f2225910ea627c14"
readonly STACK_ENV_EXPECTED_PROFILE_SOURCE="76e8edf394a4cb37ca822037272d543c68f25f71"
readonly STACK_ENV_EXPECTED_PROVENANCE="cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"
readonly STACK_ENV_EXPECTED_PORTAL_REMOTE="https://github.com/input-output-hk/lace-id-portal.git"
STACK_ENV_REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
readonly STACK_ENV_REPOSITORY_ROOT

STACK_ENV_PATH=""
STACK_SCHEMA=""
STACK_PROFILE=""
STACK_NETWORK=""
STACK_MIDNIGHT_OWNER=""
STACK_PORTAL_OWNER=""
OXID_ROOT=""
OXID_COMMIT=""
OXID_TREE=""
PORTAL_HELPER_ROOT=""
PORTAL_HELPER_COMMIT=""
PORTAL_HELPER_TREE=""
PORTAL_PROTOCOL_SOURCE_DIR=""
PORTAL_PROTOCOL_COMMIT=""
PORTAL_PROTOCOL_TREE=""
PORTAL_PROTOCOL_PR_HEAD=""
PORTAL_PROFILE_SOURCE_COMMIT=""
PORTAL_PROVENANCE_SHA256=""
LOCAL_STACK_STATE_DIR=""
SHARED_MIDNIGHT_PROJECT=""
PORTAL_COMPOSE_PROJECT=""
SHARED_MIDNIGHT_NODE_URL=""
SHARED_MIDNIGHT_INDEXER_HTTP_URL=""
SHARED_MIDNIGHT_INDEXER_WS_URL=""
SHARED_MIDNIGHT_PROOF_SERVER_URL=""
SHARED_MIDNIGHT_NODE_HOST_URL=""
SHARED_MIDNIGHT_INDEXER_V3_HOST_URL=""
SHARED_MIDNIGHT_INDEXER_V4_HOST_URL=""
SHARED_MIDNIGHT_PROOF_SERVER_HOST_URL=""
PORTAL_ISSUER_URL=""
PORTAL_HOLDER_RESOLVER_URL=""
PORTAL_HOST_ISSUER_ORIGIN=""
PORTAL_HOST_RESOLVER_ORIGIN=""
STACK_ENV_ERROR="invalid_stack_env"

stack_env_metadata() {
  local path="$1" metadata extra
  if metadata="$(stat -c '%u %a %s' -- "$path" 2>/dev/null)"; then :
  elif metadata="$(stat -f '%u %Lp %z' -- "$path" 2>/dev/null)"; then :
  else return 1; fi
  read -r STACK_ENV_FILE_OWNER STACK_ENV_FILE_MODE STACK_ENV_FILE_SIZE extra <<<"$metadata"
  [ -z "${extra:-}" ]
}

stack_env_dir_metadata() {
  local path="$1" metadata extra
  if metadata="$(stat -c '%u %a' -- "$path" 2>/dev/null)"; then :
  elif metadata="$(stat -f '%u %Lp' -- "$path" 2>/dev/null)"; then :
  else return 1; fi
  read -r STACK_ENV_DIR_OWNER STACK_ENV_DIR_MODE extra <<<"$metadata"
  [ -z "${extra:-}" ]
}

stack_env_canonical_file() {
  local path="$1" parent leaf resolved
  case "$path" in /*) ;; *) return 1 ;; esac
  leaf="${path##*/}"; parent="${path%/*}"
  [ -n "$leaf" ] || return 1
  [ -n "$parent" ] || parent=/
  resolved="$(cd -- "$parent" 2>/dev/null && pwd -P)" || return 1
  [ "${resolved%/}/$leaf" = "$path" ]
}

stack_env_canonical_dir() {
  local path="$1" resolved
  case "$path" in /*) ;; *) return 1 ;; esac
  [ -d "$path" ] && [ ! -L "$path" ] || return 1
  resolved="$(cd -- "$path" 2>/dev/null && pwd -P)" || return 1
  [ "$resolved" = "$path" ]
}

stack_env_assign_public() {
  local key="$1" value="$2"
  case "$key" in
    STACK_SCHEMA) STACK_SCHEMA="$value" ;;
    STACK_PROFILE) STACK_PROFILE="$value" ;;
    STACK_NETWORK) STACK_NETWORK="$value" ;;
    STACK_MIDNIGHT_OWNER) STACK_MIDNIGHT_OWNER="$value" ;;
    STACK_PORTAL_OWNER) STACK_PORTAL_OWNER="$value" ;;
    OXID_ROOT) OXID_ROOT="$value" ;;
    OXID_COMMIT) OXID_COMMIT="$value" ;;
    OXID_TREE) OXID_TREE="$value" ;;
    PORTAL_HELPER_ROOT) PORTAL_HELPER_ROOT="$value" ;;
    PORTAL_HELPER_COMMIT) PORTAL_HELPER_COMMIT="$value" ;;
    PORTAL_HELPER_TREE) PORTAL_HELPER_TREE="$value" ;;
    PORTAL_PROTOCOL_SOURCE_DIR) PORTAL_PROTOCOL_SOURCE_DIR="$value" ;;
    PORTAL_PROTOCOL_COMMIT) PORTAL_PROTOCOL_COMMIT="$value" ;;
    PORTAL_PROTOCOL_TREE) PORTAL_PROTOCOL_TREE="$value" ;;
    PORTAL_PROTOCOL_PR_HEAD) PORTAL_PROTOCOL_PR_HEAD="$value" ;;
    PORTAL_PROFILE_SOURCE_COMMIT) PORTAL_PROFILE_SOURCE_COMMIT="$value" ;;
    PORTAL_PROVENANCE_SHA256) PORTAL_PROVENANCE_SHA256="$value" ;;
    LOCAL_STACK_STATE_DIR) LOCAL_STACK_STATE_DIR="$value" ;;
    SHARED_MIDNIGHT_PROJECT) SHARED_MIDNIGHT_PROJECT="$value" ;;
    PORTAL_COMPOSE_PROJECT) PORTAL_COMPOSE_PROJECT="$value" ;;
    SHARED_MIDNIGHT_NODE_URL) SHARED_MIDNIGHT_NODE_URL="$value" ;;
    SHARED_MIDNIGHT_INDEXER_HTTP_URL) SHARED_MIDNIGHT_INDEXER_HTTP_URL="$value" ;;
    SHARED_MIDNIGHT_INDEXER_WS_URL) SHARED_MIDNIGHT_INDEXER_WS_URL="$value" ;;
    SHARED_MIDNIGHT_PROOF_SERVER_URL) SHARED_MIDNIGHT_PROOF_SERVER_URL="$value" ;;
    SHARED_MIDNIGHT_NODE_HOST_URL) SHARED_MIDNIGHT_NODE_HOST_URL="$value" ;;
    SHARED_MIDNIGHT_INDEXER_V3_HOST_URL) SHARED_MIDNIGHT_INDEXER_V3_HOST_URL="$value" ;;
    SHARED_MIDNIGHT_INDEXER_V4_HOST_URL) SHARED_MIDNIGHT_INDEXER_V4_HOST_URL="$value" ;;
    SHARED_MIDNIGHT_PROOF_SERVER_HOST_URL) SHARED_MIDNIGHT_PROOF_SERVER_HOST_URL="$value" ;;
    PORTAL_ISSUER_URL) PORTAL_ISSUER_URL="$value" ;;
    PORTAL_HOLDER_RESOLVER_URL) PORTAL_HOLDER_RESOLVER_URL="$value" ;;
    PORTAL_HOST_ISSUER_ORIGIN) PORTAL_HOST_ISSUER_ORIGIN="$value" ;;
    PORTAL_HOST_RESOLVER_ORIGIN) PORTAL_HOST_RESOLVER_ORIGIN="$value" ;;
    *) return 1 ;;
  esac
}

stack_env_parse_public() {
  local sanitized line key value
  sanitized="$(umask 077 && mktemp "${TMPDIR:-/tmp}/oxid-stack-env-public.XXXXXX")" || return 1
  if ! awk '
    BEGIN {
      split("STACK_SCHEMA STACK_PROFILE STACK_NETWORK STACK_MIDNIGHT_OWNER STACK_PORTAL_OWNER OXID_ROOT OXID_COMMIT OXID_TREE PORTAL_HELPER_ROOT PORTAL_HELPER_COMMIT PORTAL_HELPER_TREE PORTAL_PROTOCOL_SOURCE_DIR PORTAL_PROTOCOL_COMMIT PORTAL_PROTOCOL_TREE PORTAL_PROTOCOL_PR_HEAD PORTAL_PROFILE_SOURCE_COMMIT PORTAL_PROVENANCE_SHA256 LOCAL_STACK_STATE_DIR SHARED_MIDNIGHT_PROJECT PORTAL_COMPOSE_PROJECT SHARED_MIDNIGHT_NODE_URL SHARED_MIDNIGHT_INDEXER_HTTP_URL SHARED_MIDNIGHT_INDEXER_WS_URL SHARED_MIDNIGHT_PROOF_SERVER_URL SHARED_MIDNIGHT_NODE_HOST_URL SHARED_MIDNIGHT_INDEXER_V3_HOST_URL SHARED_MIDNIGHT_INDEXER_V4_HOST_URL SHARED_MIDNIGHT_PROOF_SERVER_HOST_URL PORTAL_ISSUER_URL PORTAL_HOLDER_RESOLVER_URL PORTAL_HOST_ISSUER_ORIGIN PORTAL_HOST_RESOLVER_ORIGIN PORTAL_WALLET_SEED PORTAL_DID_MANAGER_API_KEY PORTAL_DID_MANAGER_CONTROLLER_API_KEY PORTAL_ISSUER_SESSION_TOKEN_SECRET PORTAL_DIDIT_API_KEY", names, " ")
      for (i in names) allowed[names[i]] = 1
      secret["PORTAL_WALLET_SEED"] = 1
      secret["PORTAL_DID_MANAGER_API_KEY"] = 1
      secret["PORTAL_DID_MANAGER_CONTROLLER_API_KEY"] = 1
      secret["PORTAL_ISSUER_SESSION_TOKEN_SECRET"] = 1
      secret["PORTAL_DIDIT_API_KEY"] = 1
    }
    /^$/ || /^#/ { next }
    {
      split_at = index($0, "=")
      if (!split_at) exit 2
      key = substr($0, 1, split_at - 1)
      if (!(key in allowed) || (key in seen) || length($0) == split_at) exit 2
      seen[key] = 1
      count++
      if (!(key in secret)) print $0
    }
    END {
      if (count != 37) exit 2
      for (key in allowed) if (!(key in seen)) exit 2
    }
  ' "$STACK_ENV_PATH" >"$sanitized"; then
    rm -f -- "$sanitized"
    return 1
  fi
  while IFS= read -r line || [ -n "$line" ]; do
    key="${line%%=*}"; value="${line#*=}"
    stack_env_assign_public "$key" "$value" || { rm -f -- "$sanitized"; return 1; }
  done <"$sanitized"
  rm -f -- "$sanitized"
}

stack_env_sha() { [[ "$1" =~ ^[0-9a-f]{40}$ ]]; }
stack_env_digest() { [[ "$1" =~ ^[0-9a-f]{64}$ ]]; }

stack_env_validate_git_root() {
  local root="$1" commit="$2" tree="$3" include_untracked="$4" status actual
  stack_env_canonical_dir "$root" || return 1
  stack_env_sha "$commit" && stack_env_sha "$tree" || return 1
  [ "$(git -C "$root" rev-parse --show-toplevel 2>/dev/null)" = "$root" ] || return 1
  status="$(git -C "$root" status --porcelain=v1 --untracked-files="$include_untracked" 2>/dev/null)" || return 1
  [ -z "$status" ] || return 1
  actual="$(git -C "$root" rev-parse HEAD 2>/dev/null)" || return 1
  [ "$actual" = "$commit" ] || return 1
  actual="$(git -C "$root" rev-parse 'HEAD^{tree}' 2>/dev/null)" || return 1
  [ "$actual" = "$tree" ]
}

stack_env_validate_public_values() {
  local current_user remote
  [ "$STACK_SCHEMA" = oxid-laceid-headless-v1 ] &&
    [ "$STACK_PROFILE" = headless ] && [ "$STACK_NETWORK" = undeployed ] &&
    [ "$STACK_MIDNIGHT_OWNER" = oxid ] && [ "$STACK_PORTAL_OWNER" = portal ] || return 1
  [ "$OXID_ROOT" = "$STACK_ENV_REPOSITORY_ROOT" ] || return 1
  stack_env_validate_git_root "$OXID_ROOT" "$OXID_COMMIT" "$OXID_TREE" no || return 1

  [ "$PORTAL_HELPER_COMMIT" = "$STACK_ENV_EXPECTED_HELPER_COMMIT" ] &&
    [ "$PORTAL_HELPER_TREE" = "$STACK_ENV_EXPECTED_HELPER_TREE" ] || { STACK_ENV_ERROR=invalid_helper; return 1; }
  stack_env_validate_git_root "$PORTAL_HELPER_ROOT" "$PORTAL_HELPER_COMMIT" "$PORTAL_HELPER_TREE" no || { STACK_ENV_ERROR=invalid_helper; return 1; }
  remote="$(git -C "$PORTAL_HELPER_ROOT" remote get-url origin 2>/dev/null)" || { STACK_ENV_ERROR=invalid_helper; return 1; }
  [ "$remote" = "$STACK_ENV_EXPECTED_PORTAL_REMOTE" ] || { STACK_ENV_ERROR=invalid_helper; return 1; }
  git -C "$PORTAL_HELPER_ROOT" verify-commit "$PORTAL_HELPER_COMMIT" >/dev/null 2>&1 || { STACK_ENV_ERROR=invalid_helper; return 1; }
  [ -x "$PORTAL_HELPER_ROOT/scripts/oxid-conformance-lifecycle.sh" ] || { STACK_ENV_ERROR=invalid_helper; return 1; }

  [ "$PORTAL_PROTOCOL_SOURCE_DIR" != "$PORTAL_HELPER_ROOT" ] || return 1
  [ "$PORTAL_PROTOCOL_COMMIT" = "$STACK_ENV_EXPECTED_PROTOCOL_COMMIT" ] &&
    [ "$PORTAL_PROTOCOL_TREE" = "$STACK_ENV_EXPECTED_PROTOCOL_TREE" ] &&
    [ "$PORTAL_PROTOCOL_PR_HEAD" = "$STACK_ENV_EXPECTED_PROTOCOL_PR_HEAD" ] &&
    [ "$PORTAL_PROFILE_SOURCE_COMMIT" = "$STACK_ENV_EXPECTED_PROFILE_SOURCE" ] &&
    [ "$PORTAL_PROVENANCE_SHA256" = "$STACK_ENV_EXPECTED_PROVENANCE" ] || return 1
  stack_env_validate_git_root "$PORTAL_PROTOCOL_SOURCE_DIR" "$PORTAL_PROTOCOL_COMMIT" "$PORTAL_PROTOCOL_TREE" all || return 1
  if git -C "$PORTAL_PROTOCOL_SOURCE_DIR" symbolic-ref -q HEAD >/dev/null 2>&1; then return 1; fi

  stack_env_canonical_dir "$LOCAL_STACK_STATE_DIR" || return 1
  stack_env_dir_metadata "$LOCAL_STACK_STATE_DIR" || return 1
  current_user="$(id -u)" || return 1
  [ "$STACK_ENV_DIR_OWNER" = "$current_user" ] && [ "$STACK_ENV_DIR_MODE" = 700 ] || return 1
  case "$STACK_ENV_PATH/" in "$OXID_ROOT"/*|"$PORTAL_HELPER_ROOT"/*|"$PORTAL_PROTOCOL_SOURCE_DIR"/*) return 1 ;; esac
  case "$LOCAL_STACK_STATE_DIR/" in "$OXID_ROOT"/*|"$PORTAL_HELPER_ROOT"/*|"$PORTAL_PROTOCOL_SOURCE_DIR"/*) return 1 ;; esac

  [ "$SHARED_MIDNIGHT_PROJECT" = oxid-standalone ] || return 1
  [[ "$PORTAL_COMPOSE_PROJECT" =~ ^oxidportal[a-z0-9_-]{0,53}$ ]] || return 1
  [ "$SHARED_MIDNIGHT_NODE_URL" = http://host.docker.internal:9944 ] || return 1
  [ "$SHARED_MIDNIGHT_INDEXER_HTTP_URL" = http://host.docker.internal:8088/api/v3/graphql ] || return 1
  [ "$SHARED_MIDNIGHT_INDEXER_WS_URL" = ws://host.docker.internal:8088/api/v3/graphql/ws ] || return 1
  [ "$SHARED_MIDNIGHT_PROOF_SERVER_URL" = http://host.docker.internal:6300 ] || return 1
  [ "$SHARED_MIDNIGHT_NODE_HOST_URL" = http://127.0.0.1:9944 ] || return 1
  [ "$SHARED_MIDNIGHT_INDEXER_V3_HOST_URL" = http://127.0.0.1:8088/api/v3/graphql ] || return 1
  [ "$SHARED_MIDNIGHT_INDEXER_V4_HOST_URL" = http://127.0.0.1:8088/api/v4/graphql ] || return 1
  [ "$SHARED_MIDNIGHT_PROOF_SERVER_HOST_URL" = http://127.0.0.1:6300 ] || return 1
  [ "$PORTAL_ISSUER_URL" = http://127.0.0.1:18090 ] || return 1
  [ "$PORTAL_HOLDER_RESOLVER_URL" = http://host.docker.internal:18092 ] || return 1
  [ "$PORTAL_HOST_ISSUER_ORIGIN" = http://127.0.0.1:8090 ] || return 1
  [ "$PORTAL_HOST_RESOLVER_ORIGIN" = http://127.0.0.1:9092 ] || return 1
}

stack_env_load() {
  local current_user stripped_size
  STACK_ENV_ERROR=invalid_stack_env
  STACK_ENV_PATH="$1"
  stack_env_canonical_file "$STACK_ENV_PATH" || return 1
  [ -f "$STACK_ENV_PATH" ] && [ ! -L "$STACK_ENV_PATH" ] || return 1
  stack_env_metadata "$STACK_ENV_PATH" || return 1
  current_user="$(id -u)" || return 1
  [ "$STACK_ENV_FILE_OWNER" = "$current_user" ] && [ "$STACK_ENV_FILE_MODE" = 600 ] || return 1
  ((10#$STACK_ENV_FILE_SIZE <= 32768)) || return 1
  stripped_size="$(tr -d '\000' <"$STACK_ENV_PATH" | wc -c)" || return 1
  stripped_size="${stripped_size//[[:space:]]/}"
  ((10#$stripped_size == 10#$STACK_ENV_FILE_SIZE)) || return 1
  stack_env_parse_public || return 1
  stack_env_validate_public_values
}

stack_env_delegate_portal() {
  local operation="$1"
  case "$operation" in up|status|down) ;; *) return 2 ;; esac
  STACK_ENV_FILE="$STACK_ENV_PATH" "$PORTAL_HELPER_ROOT/scripts/oxid-conformance-lifecycle.sh" "$operation"
}
