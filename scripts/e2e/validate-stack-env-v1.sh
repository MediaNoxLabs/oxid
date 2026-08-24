#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
export LC_ALL=C
readonly repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
# shellcheck source=scripts/e2e/stack-env-v1.sh
source "$repository_root/scripts/e2e/stack-env-v1.sh"
[ "$#" -eq 1 ] || { printf 'stack-env: error=usage\n' >&2; exit 2; }
if ! stack_env_load "$1"; then printf 'stack-env: error=%s\n' "$STACK_ENV_ERROR" >&2; exit 2; fi
# The exact authenticated Portal helper owns secret-value validation. Its closed
# status output is intentionally discarded; Oxid never imports those values.
if ! stack_env_delegate_portal status >/dev/null; then
  printf 'stack-env: error=portal_validation_failed\n' >&2
  exit 2
fi
printf '{"schema":"oxid-stack-env-validation-v1","valid":true}\n'
