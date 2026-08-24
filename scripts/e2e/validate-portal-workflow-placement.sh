#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

workflow_directory="${1:-.github/workflows}"
portal_credential_name="PORTAL_SOURCE""_TOKEN"
fail() {
  printf 'portal-workflow-placement: FAIL phase=%s\n' "$1" >&2
  exit 1
}

[ -d "$workflow_directory" ] && [ ! -L "$workflow_directory" ] || fail workflow-directory
command -v find >/dev/null 2>&1 || fail missing-find
command -v rg >/dev/null 2>&1 || fail missing-rg
workflow_list="$(find "$workflow_directory" -maxdepth 1 -type f \( -name '*.yml' -o -name '*.yaml' \) -print | sort)" || fail workflow-list
[ -n "$workflow_list" ] || fail workflow-list

while IFS= read -r workflow; do
  [ -f "$workflow" ] && [ ! -L "$workflow" ] || fail workflow-file
  if rg -qi 'secrets\.[^ }]*(portal)|portal[^:#]*(token|credential|secret)' "$workflow"; then
    fail private-source-credential
  fi
  if rg -qF "$portal_credential_name" "$workflow"; then
    fail private-source-credential
  fi
  if rg -qi \
    'input-output-hk/lace-id-portal|PORTAL_SOURCE_TREE:[[:space:]]*\$\{\{[[:space:]]*github\.workspace|target/portal-(headless-e2e|mobile-e2e|local-conformance)|name:[^#]*(real|pinned)[^#]*Portal[^#]*(evidence|conformance)|name:[^#]*Portal[^#]*(real|pinned)[^#]*(evidence|conformance)' \
    "$workflow"; then
    fail private-source-or-evidence-claim
  fi
  while IFS= read -r line || [ -n "$line" ]; do
    if [[ "$line" =~ ^[[:space:]]*bash[[:space:]]+-n[[:space:]]+scripts/(e2e/portal-(headless-e2e|local-conformance)\.sh|e2e/(check-portal-resource-leaks|portal-local-lock-lib|validate-portal-(headless-evidence|source-checkout|workflow-placement))\.sh|test-(ios|android)-portal-flow\.sh)[[:space:]]*$ ]]; then
      continue
    fi
    if rg -qi '^[[:space:]]*name:.*Portal.*(evidence|conformance)' <<<"$line" &&
      ! rg -qi '(static|contract|no private)' <<<"$line"; then
      fail false-hosted-claim
    fi
    if rg -q '^[[:space:]]{2}portal-(headless-e2e|mobile-smoke|local-conformance):' <<<"$line"; then
      fail false-hosted-claim
    fi
    case "$line" in
      *"just portal-local-conformance"*|*"just portal-headless-e2e"*|*"just portal-mobile-smoke"*|*"just ios-portal-smoke"*|*"just android-portal-smoke"*|*"scripts/e2e/portal-local-conformance.sh"*|*"scripts/e2e/portal-headless-e2e.sh"*|*"scripts/test-ios-portal-flow.sh"*|*"scripts/test-android-portal-flow.sh"*)
        fail hosted-real-execution
        ;;
    esac
  done <"$workflow"
done <<<"$workflow_list"

printf 'portal-workflow-placement: PASS workflows=%s\n' "$(wc -l <<<"$workflow_list" | tr -d ' ')"
