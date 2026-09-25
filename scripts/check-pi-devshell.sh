#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for required_command in pi node jq realpath timeout; do
  if ! command -v "$required_command" >/dev/null 2>&1; then
    echo "missing Pi devshell command: $required_command" >&2
    exit 1
  fi
done

pi_executable="$(realpath "$(command -v pi)")"
if [[ "$pi_executable" != /nix/store/*/bin/pi ]]; then
  echo "Pi is not supplied by the pinned Nix development shell: $pi_executable" >&2
  echo "run this check through ./bootstrap.sh --check" >&2
  exit 1
fi

pi_version="$(pi --version)"
if [[ "$pi_version" != "0.85.1" ]]; then
  echo "Pi 0.85.1 is required for native detached child dispatch; found: $pi_version" >&2
  echo "enter through ./bootstrap.sh so the locked Nix runtime is active, then retry" >&2
  exit 1
fi

common_git_dir="$(git rev-parse --path-format=absolute --git-common-dir)"
expected_runtime_state="$common_git_dir/oxid-factory/pi-runtime-v1"
expected_session_dir="$expected_runtime_state/sessions"
expected_subagent_root="$expected_runtime_state/subagents"
for variable in PI_CODING_AGENT_SESSION_DIR PI_SUBAGENTS_TEMP_ROOT; do
  value="${!variable:-}"
  expected="$expected_session_dir"
  [[ "$variable" == "PI_SUBAGENTS_TEMP_ROOT" ]] && expected="$expected_subagent_root"
  if [[ -z "$value" || ! -d "$value" || "$(realpath "$value")" != "$(realpath "$expected")" ]]; then
    echo "$variable must use stable owner-private runtime state at $expected" >&2
    echo "re-enter with ./bootstrap.sh before dispatching a detached child" >&2
    exit 1
  fi
  if stat -f '%Lp' "$value" >/dev/null 2>&1; then
    state_mode="$(stat -f '%Lp' "$value")"
  else
    state_mode="$(stat -c '%a' "$value")"
  fi
  if [[ "$state_mode" != "700" ]]; then
    echo "$variable must reference a directory with mode 0700: $value" >&2
    echo "re-enter with ./bootstrap.sh to repair the stable runtime state directory" >&2
    exit 1
  fi
done

pi_contract="$(node scripts/factory/check-pi-devshell-config.mjs)"
expected_provider="$(printf '%s\n' "$pi_contract" | jq -er '.expectedProvider')"
expected_model="$(printf '%s\n' "$pi_contract" | jq -er '.expectedModel')"
review_package_root="$(printf '%s\n' "$pi_contract" | jq -er '.reviewPackageRoot')"
if ! timeout --kill-after=5s 30s pi --list-models "$expected_provider/$expected_model" \
  | node scripts/factory/check-pi-rpc-commands.mjs \
    --provider "$expected_provider" --model "$expected_model"; then
  echo "tracked Pi model is absent from the Nix-pinned catalog: $expected_provider/$expected_model" >&2
  exit 1
fi

review_package_json="$review_package_root/package.json"
if [[ ! -f "$review_package_json" ]]; then
  echo "missing exact project @input-output-hk/agent-review-pi@0.6.0" >&2
  echo "enter nix develop with a GitHub token that can read packages" >&2
  exit 1
fi

pi_rpc_stderr="$(mktemp "${TMPDIR:-/tmp}/oxid-pi-smoke.XXXXXX")"
agent_hashes_before="$(git hash-object .pi/agents/*.agent.md)"
trap 'rm -f "$pi_rpc_stderr"' EXIT
loader_path="$repo_root/.pi/npm/node_modules/@input-output-hk/agent-review-pi/skills/agent-review/SKILL.md"
if ! {
  printf '%s\n' '{"type":"get_commands"}'
} | timeout --kill-after=5s 30s pi --approve --offline --mode rpc --no-session 2>"$pi_rpc_stderr" \
  | node scripts/factory/check-pi-rpc-commands.mjs --loader-path "$loader_path"; then
  echo "Pi offline RPC startup failed:" >&2
  sed -n '1,20p' "$pi_rpc_stderr" >&2
  exit 1
fi
if grep -F "Failed to load skill" "$pi_rpc_stderr" >/dev/null; then
  echo "Pi rejected skill metadata during startup:" >&2
  grep -F "Failed to load skill" "$pi_rpc_stderr" >&2
  exit 1
fi

agent_hashes_after="$(git hash-object .pi/agents/*.agent.md)"
if [[ "$agent_hashes_after" != "$agent_hashes_before" ]]; then
  echo "Pi startup modified tracked project agent shadows:" >&2
  git diff --name-only -- .pi/agents >&2
  echo "suppress package extensions that rewrite consumer-owned policy before starting Pi" >&2
  exit 1
fi

echo "Pi devshell smoke passed: pi $pi_version, $expected_provider/$expected_model, agent-review-pi 0.6.0 extension and bundled skill available; unsafe taskflow resources suppressed."
