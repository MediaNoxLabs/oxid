#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repository_root"
for command_name in git jq node rg; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Required command '$command_name' is missing." >&2
    exit 1
  }
done

portal_credential_name="PORTAL_SOURCE""_TOKEN"
orchestrator="scripts/e2e/portal-local-conformance.sh"
evidence_tool="scripts/e2e/portal-local-evidence.mjs"
source_validator="scripts/e2e/validate-portal-source-checkout.sh"
workflow_validator="scripts/e2e/validate-portal-workflow-placement.sh"
resource_checker="scripts/e2e/check-portal-resource-leaks.sh"
lock_runner="scripts/e2e/with-portal-local-lock.sh"
for script in \
  "$orchestrator" \
  "$source_validator" \
  "$workflow_validator" \
  "$resource_checker" \
  "$lock_runner" \
  scripts/e2e/portal-headless-e2e.sh \
  scripts/test-ios-portal-flow.sh \
  scripts/test-android-portal-flow.sh; do
  bash -n "$script"
done
node --check "$evidence_tool"

line_of() {
  local marker="$1" line
  line="$(grep -nF "$marker" "$orchestrator" | head -1 | cut -d: -f1)"
  [[ "$line" =~ ^[0-9]+$ ]] || {
    echo "Local Portal orchestrator is missing step marker: $marker" >&2
    exit 1
  }
  printf '%s\n' "$line"
}
headless_line="$(line_of 'run_step "headless"')"
ios_portal_line="$(line_of 'run_step "ios-portal"')"
ios_standard_line="$(line_of 'run_step "ios-standard"')"
android_portal_line="$(line_of 'run_step "android-portal"')"
android_standard_line="$(line_of 'run_step "android-standard"')"
ios_attest_line="$(grep -nF 'attest-standard-smoke' "$orchestrator" | head -1 | cut -d: -f1)"
android_attest_line="$(grep -nF 'attest-standard-smoke' "$orchestrator" | tail -1 | cut -d: -f1)"
if ! (( headless_line < ios_portal_line &&
        ios_portal_line < ios_standard_line &&
        ios_standard_line < ios_attest_line &&
        ios_attest_line < android_portal_line &&
        android_portal_line < android_standard_line &&
        android_standard_line < android_attest_line )); then
  echo "Local Portal order must remain headless, iOS Portal/standard/attest, Android Portal/standard/attest." >&2
  exit 1
fi
for marker in \
  'EXPECTED_HEAD="$(git rev-parse HEAD)"' \
  'EXPECTED_BRANCH="$(git symbolic-ref --quiet --short HEAD' \
  'status --porcelain --untracked-files=no' \
  'assert_repository_state' \
  'assert_no_harness_leaks' \
  'validate-portal-source-checkout.sh' \
  'with-portal-local-lock.sh' \
  "trap '' INT TERM" \
  'attest-standard-smoke' \
  'rollback_publication' \
  'PUBLICATION_COMPLETE=1' \
  'portal-local-evidence.mjs'; do
  rg -qF "$marker" "$orchestrator" || {
    echo "Local Portal orchestration contract marker is missing: $marker" >&2
    exit 1
  }
done
if rg -qF "$portal_credential_name" "$orchestrator" ||
  rg -qi 'access[_-]?token|pre-authorized|offer|grant|jwt|capability' "$orchestrator"; then
  echo "Local Portal command construction contains a credential or protocol-material input." >&2
  exit 1
fi
for marker in OXID_PORTAL_IOS_EVIDENCE_PATH OXID_PORTAL_ANDROID_EVIDENCE_PATH; do
  rg -qF "$marker" scripts/test-*-portal-flow.sh || {
    echo "Portal platform evidence cannot be staged by the complete local orchestrator: $marker" >&2
    exit 1
  }
done
if rg -qF 'rm -f "$EVIDENCE"' scripts/e2e/portal-headless-e2e.sh; then
  echo "Headless interruption would discard prior evidence before replacement." >&2
  exit 1
fi
rg -q '^portal-local-conformance[[:space:]].*:$' Justfile || {
  echo "Justfile is missing the complete local Portal recipe." >&2
  exit 1
}

scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
head="0123456789abcdef0123456789abcdef01234567"
headless="$scratch/headless.json"
ios="$scratch/ios.json"
android="$scratch/android.json"
portal_document='{"helperCommit":"f7732be01171cf6a376ec0dd043f517e3f6fcf6b","helperTree":"96accf0da80992c3b247458c3b21f22ee9db1d68","integrationCommit":"925ec8d04882eabd4ac7b784c70fc2f0c152faae","integrationTree":"58b4597524f88a0ae2253439a44dab0dc60cbb6f","prHead":"9c82db23eabe8b6d758b2731f2225910ea627c14","profileSourceCommit":"76e8edf394a4cb37ca822037272d543c68f25f71","provenanceSha256":"cf86f4ddb06131d7570c835e8c6c62d524e8179fe6a53436b20d2d4e72b44d87"}'

jq -cn --arg head "$head" --argjson portal "$portal_document" '{
  acceptance:{
    confirmationRequired:true,encryptedPersistence:true,exactBundleImported:true,
    managedAuthenticationProof:true,mockKycApproved:true,newProcessRestore:true,
    refusalWithoutSecretCalls:true,replayRejected:true,reverified:true,
    separateJubjubAssertionBinding:true,sharedMidnightIdentityUnchanged:true
  },
  oxid:{head:$head},portal:$portal,schema:"oxid-portal-headless-evidence-v1"
}' >"$headless"
jq -cn --arg head "$head" --argjson portal "$portal_document" '{
  schema:"oxid-portal-mobile-evidence-v1",oxid:{head:$head},portal:$portal,
  platform:{kind:"ios_simulator",model:"iPhone 17 Pro",os:"com.apple.CoreSimulator.SimRuntime.iOS-26-4",applicationId:"io.medianox.oxid",profile:"standalone-local-development-portal"},
  acceptance:{
    mockKycApproved:true,warmColdCustomScheme:true,oneItemStrictRouter:true,
    explicitConsent:true,managedAuthenticationProof:true,separateJubjubAssertionBinding:true,
    strictFinalExchange:true,exactBundleImported:true,encryptedPersistence:true,
    processRestart:true,developmentCustodyReactivated:true,reverified:true,
    unavailableDenied:true,timeoutDenied:true,cameraUnavailable:true,secretFreeEvidence:true
  }
}' >"$ios"
jq -cn --arg head "$head" --argjson portal "$portal_document" '{
  schema:"oxid-portal-mobile-evidence-v1",oxid:{head:$head},portal:$portal,
  platform:{kind:"android_qemu_emulator",model:"sdk_gphone64_arm64",os:"15",apiLevel:"35",clockSkewSeconds:-2,applicationId:"io.medianox.oxid",profile:"standalone-local-development-portal",adbReversePorts:[6300,8088,9944,18090,18091,18093]},
  acceptance:{
    mockKycApproved:true,warmColdCustomScheme:true,oneItemStrictRouter:true,
    explicitConsent:true,managedAuthenticationProof:true,separateJubjubAssertionBinding:true,
    strictFinalExchange:true,exactBundleImported:true,encryptedPersistence:true,
    processRestart:true,developmentCustodyReactivated:true,reverified:true,
    malformedDenied:true,unavailableDenied:true,timeoutDenied:true,qemuVerified:true,
    clockSynchronized:true,noEmulatorAlias:true,secretFreeEvidence:true
  }
}' >"$android"
node "$evidence_tool" attest-standard-smoke --platform ios --evidence "$ios" --head "$head" >/dev/null
node "$evidence_tool" attest-standard-smoke --platform android --evidence "$android" --head "$head" >/dev/null
node "$evidence_tool" validate --head "$head" --headless "$headless" --ios "$ios" --android "$android" >/dev/null

assert_rejected() {
  local candidate_headless="$1" candidate_ios="$2" candidate_android="$3"
  if node "$evidence_tool" validate --head "$head" \
    --headless "$candidate_headless" --ios "$candidate_ios" --android "$candidate_android" \
    >/dev/null 2>&1; then
    echo "Local Portal evidence validator accepted a negative fixture." >&2
    exit 1
  fi
}
jq '.oxid.head = "ffffffffffffffffffffffffffffffffffffffff"' "$headless" >"$scratch/stale-headless.json"
assert_rejected "$scratch/stale-headless.json" "$ios" "$android"
jq '.oxid.head = "ffffffffffffffffffffffffffffffffffffffff"' "$ios" >"$scratch/mixed-ios.json"
assert_rejected "$headless" "$scratch/mixed-ios.json" "$android"
jq '.portal.integrationTree = "ffffffffffffffffffffffffffffffffffffffff"' "$android" >"$scratch/wrong-source.json"
assert_rejected "$headless" "$ios" "$scratch/wrong-source.json"
jq '.acceptance.standardSmoke = false' "$android" >"$scratch/partial-android.json"
assert_rejected "$headless" "$ios" "$scratch/partial-android.json"
# This remains shape-valid for the iOS model field, so only the private-value
# sentinel rejects it.
jq '.platform.model = "iPhone access-token"' "$ios" >"$scratch/private-ios.json"
assert_rejected "$headless" "$scratch/private-ios.json" "$android"

fake_source="$scratch/private-source"
git init -q "$fake_source"
git -C "$fake_source" remote add origin https://github.com/input-output-hk/lace-id-portal.git
if "$source_validator" "$fake_source" --offline >/dev/null 2>&1; then
  echo "Portal source validator accepted a checkout without the immutable source pins." >&2
  exit 1
fi

resource_repository="$scratch/resource-repository"
resource_source="$scratch/resource-source"
resource_tmp="$scratch/resource-tmp"
resource_stubs="$scratch/resource-stubs"
mkdir -p "$resource_repository" "$resource_source" "$resource_tmp" "$resource_stubs"
cat >"$resource_stubs/git" <<'STUB'
#!/usr/bin/env bash
if [ "${STUB_GIT_FAIL:-0}" = 1 ]; then exit 1; fi
printf '%b' "${STUB_WORKTREES:-}"
STUB
cat >"$resource_stubs/docker" <<'STUB'
#!/usr/bin/env bash
if [ "${STUB_DOCKER_FAIL:-0}" = 1 ]; then exit 1; fi
printf '%b' "${STUB_DOCKER_OUTPUT:-}"
STUB
chmod +x "$resource_stubs/git" "$resource_stubs/docker"
PATH="$resource_stubs:$PATH" TMPDIR="$resource_tmp" \
  "$resource_checker" "$resource_repository" "$resource_source" >/dev/null
if PATH="$resource_stubs:$PATH" TMPDIR="$resource_tmp" STUB_DOCKER_FAIL=1 \
  "$resource_checker" "$resource_repository" "$resource_source" >/dev/null 2>&1; then
  echo "Portal resource checker accepted a failed Docker query." >&2
  exit 1
fi
if PATH="$resource_stubs:$PATH" TMPDIR="$resource_tmp" STUB_DOCKER_OUTPUT=$'clean\noxidportal124leak\nmore\n' \
  "$resource_checker" "$resource_repository" "$resource_source" >/dev/null 2>&1; then
  echo "Portal resource checker missed a named Compose leak." >&2
  exit 1
fi
if PATH="$resource_stubs:$PATH" TMPDIR="$resource_tmp" \
  STUB_WORKTREES=$'worktree /tmp/clean\n\nworktree /tmp/oxid-portal-mobile-ios-925ec8d-test\n\n' \
  "$resource_checker" "$resource_repository" "$resource_source" >/dev/null 2>&1; then
  echo "Portal resource checker missed a detached Portal worktree leak." >&2
  exit 1
fi

lock_file="$scratch/kernel.lock"
lock_ready="$scratch/kernel-lock-ready"
lock_command_pid_file="$scratch/kernel-lock-command-pid"
"$lock_runner" "$lock_file" -- bash -c 'printf "%s\n" "$$" >"$2"; touch "$1"; sleep 30' _ "$lock_ready" "$lock_command_pid_file" &
lock_holder=$!
for _attempt in $(seq 1 100); do
  [ -e "$lock_ready" ] && break
  sleep 0.02
done
[ -e "$lock_ready" ] || {
  kill -KILL "$lock_holder" >/dev/null 2>&1 || true
  echo "Kernel-backed Portal lock holder did not start." >&2
  exit 1
}
if "$lock_runner" "$lock_file" -- true >/dev/null 2>&1; then
  kill -KILL "$lock_holder" >/dev/null 2>&1 || true
  echo "Kernel-backed Portal lock admitted a concurrent owner." >&2
  exit 1
fi
lock_command_pid="$(cat "$lock_command_pid_file")"
kill -TERM "$lock_command_pid" >/dev/null 2>&1 || true
wait "$lock_holder" >/dev/null 2>&1 || true
"$lock_runner" "$lock_file" -- true

publication_functions="$scratch/publication-functions.sh"
awk '
  /^restore_retained\(\) \{/ { capture=1 }
  /^rollback_publication\(\) \{/ { capture=1 }
  capture { print }
  capture && /^}$/ { capture=0; print "" }
' "$orchestrator" >"$publication_functions"
bash -c '
  set -euo pipefail
  source "$1"
  root="$2/publication"
  mkdir -p "$root/retained" "$root/prior"
  RETAINED_HEADLESS="$root/retained/headless.json"; PRIOR_HEADLESS="$root/prior/headless.json"; PRIOR_HEADLESS_PRESENT=1
  RETAINED_IOS="$root/retained/ios.json"; PRIOR_IOS="$root/prior/ios.json"; PRIOR_IOS_PRESENT=1
  RETAINED_ANDROID="$root/retained/android.json"; PRIOR_ANDROID="$root/prior/android.json"; PRIOR_ANDROID_PRESENT=1
  printf prior-headless >"$PRIOR_HEADLESS"; printf prior-ios >"$PRIOR_IOS"; printf prior-android >"$PRIOR_ANDROID"
  printf new-headless >"$RETAINED_HEADLESS"; printf prior-ios >"$RETAINED_IOS"; printf new-android >"$RETAINED_ANDROID"
  trap "" INT TERM
  kill -TERM "$$"
  rollback_publication
  cmp "$PRIOR_HEADLESS" "$RETAINED_HEADLESS"
  cmp "$PRIOR_IOS" "$RETAINED_IOS"
  cmp "$PRIOR_ANDROID" "$RETAINED_ANDROID"
' _ "$publication_functions" "$scratch"

"$workflow_validator" .github/workflows >/dev/null
workflow_fixtures="$scratch/workflows"
mkdir -p "$workflow_fixtures/good" "$workflow_fixtures/bad-execution" "$workflow_fixtures/bad-credential" "$workflow_fixtures/bad-evidence"
cat >"$workflow_fixtures/good/quality.yml" <<'YAML'
name: Quality
jobs:
  logs:
    steps:
      - uses: actions/upload-artifact@0000000000000000000000000000000000000000
        with:
          path: build/logs
YAML
"$workflow_validator" "$workflow_fixtures/good" >/dev/null
cat >"$workflow_fixtures/bad-execution/other.yaml" <<'YAML'
name: Other
jobs:
  run-private:
    steps:
      - run: bash -n scripts/e2e/portal-local-conformance.sh && ./scripts/e2e/portal-local-conformance.sh
YAML
if "$workflow_validator" "$workflow_fixtures/bad-execution" >/dev/null 2>&1; then
  echo "Workflow placement accepted real Portal execution from another workflow." >&2
  exit 1
fi
printf 'name: Other\nenv:\n  %s: value\n' "$portal_credential_name" >"$workflow_fixtures/bad-credential/other.yml"
if "$workflow_validator" "$workflow_fixtures/bad-credential" >/dev/null 2>&1; then
  echo "Workflow placement accepted a private Portal credential in another workflow." >&2
  exit 1
fi
cat >"$workflow_fixtures/bad-evidence/other.yml" <<'YAML'
name: Other
jobs:
  upload:
    steps:
      - uses: actions/upload-artifact@0000000000000000000000000000000000000000
        with:
          path: target/portal-mobile-e2e/ios/evidence.json
YAML
if "$workflow_validator" "$workflow_fixtures/bad-evidence" >/dev/null 2>&1; then
  echo "Workflow placement accepted a stale Portal evidence upload from another workflow." >&2
  exit 1
fi

workflow=".github/workflows/ci.yml"
static_job="$(awk '
  /^  portal-contracts:/ { capture=1 }
  capture && /^  [A-Za-z0-9_-]+:/ && $0 !~ /^  portal-contracts:/ { exit }
  capture { print }
' "$workflow")"
for marker in \
  'name: Portal public/static contracts (no private source)' \
  'timeout-minutes: 10' \
  'contents: read' \
  './scripts/check-portal-headless-evidence.sh' \
  './scripts/check-portal-local-conformance.sh' \
  './scripts/check-portal-mobile-harness.sh'; do
  grep -qF "$marker" <<<"$static_job" || {
    echo "Hosted Portal static contract job is missing marker: $marker" >&2
    exit 1
  }
done

printf 'Portal local order, same-head evidence, stale/mixed/partial/source rejection, kernel lock/rollback/leak behavior, no-secret construction, and all-workflow static-only boundaries passed.\n'
