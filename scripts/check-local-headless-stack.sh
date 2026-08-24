#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repository_root"
for script in \
  scripts/e2e/stack-env-v1.sh \
  scripts/e2e/validate-stack-env-v1.sh \
  scripts/standalone-lifecycle.sh \
  scripts/local-headless.sh \
  scripts/standalone-up.sh \
  scripts/standalone-down.sh \
  scripts/test-local-headless-stack.sh; do bash -n "$script"; done
for recipe in local-headless-up local-headless-status local-headless-test local-headless-down; do
  rg -q "^${recipe} stack_env_file:" Justfile || { echo "Missing explicit-profile recipe: $recipe" >&2; exit 1; }
done
for marker in \
  'STACK_ENV_EXPECTED_HELPER_COMMIT="f7732be01171cf6a376ec0dd043f517e3f6fcf6b"' \
  'STACK_ENV_EXPECTED_HELPER_TREE="96accf0da80992c3b247458c3b21f22ee9db1d68"' \
  'STACK_ENV_EXPECTED_PROTOCOL_COMMIT="925ec8d04882eabd4ac7b784c70fc2f0c152faae"' \
  'STACK_ENV_EXPECTED_PROTOCOL_TREE="58b4597524f88a0ae2253439a44dab0dc60cbb6f"' \
  'git -C "$PORTAL_HELPER_ROOT" verify-commit' \
  'STACK_ENV_FILE="$STACK_ENV_PATH" "$PORTAL_HELPER_ROOT/scripts/oxid-conformance-lifecycle.sh"' \
  'oxid-standalone.owner.receipt' \
  'ownership=attach'; do
  rg -qF "$marker" scripts/e2e/stack-env-v1.sh scripts/standalone-lifecycle.sh scripts/local-headless.sh || {
    echo "Shared headless contract marker missing: $marker" >&2; exit 1;
  }
done
# The public loader may recognize secret key names to enforce the closed key
# set, but it must never assign/export their values.
assign_body="$(awk '/^stack_env_assign_public\(\)/,/^}/' scripts/e2e/stack-env-v1.sh)"
if rg -q 'PORTAL_(WALLET_SEED|DID_MANAGER_API_KEY|DID_MANAGER_CONTROLLER_API_KEY|ISSUER_SESSION_TOKEN_SECRET|DIDIT_API_KEY)\)' <<<"$assign_body"; then
  echo "Oxid public profile loader assigns a Portal secret value." >&2
  exit 1
fi
if rg -n 'just (compose-up|compose-down)|\["compose-(up|down)"\]|runLogged\("just", \["compose-(up|down)"\]' \
  scripts/e2e/portal-headless-e2e.sh scripts/e2e/portal-local-conformance.sh \
  scripts/e2e/portal-mobile-harness-lib.sh scripts/e2e/portal-mobile-support.mjs; then
  echo "Oxid Portal lifecycle duplicated Portal Compose ownership." >&2
  exit 1
fi
# Public hosted CI must remain static-only and source-credential-free.
"$repository_root/scripts/e2e/validate-portal-workflow-placement.sh" "$repository_root/.github/workflows" >/dev/null
printf 'Shared headless profile, delegation, ownership, and public-CI static contracts passed.\n'
