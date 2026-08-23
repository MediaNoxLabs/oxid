#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "The iOS Portal smoke test requires macOS and Xcode." >&2
  exit 1
fi
for command_name in curl jq nix node rg shasum; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Required command '$command_name' is missing." >&2
    exit 1
  }
done
[ -x /usr/bin/xcodebuild ] && [ -x /usr/bin/xcrun ] || {
  echo "Xcode is required for the iOS Portal smoke test." >&2
  exit 1
}

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"
xcode_developer_dir="$(env -u DEVELOPER_DIR /usr/bin/xcode-select -p)"
[ -d "$xcode_developer_dir" ] || {
  echo "The selected Xcode developer directory is unavailable." >&2
  exit 1
}
export OXID_XCODE_DEVELOPER_DIR="$xcode_developer_dir"
# shellcheck source=scripts/e2e/portal-mobile-harness-lib.sh
source "$repository_root/scripts/e2e/portal-mobile-harness-lib.sh"
portal_mobile_start ios

OXID_IOS_DEVICE="${OXID_IOS_DEVICE:-}" \
OXID_IOS_RESET_DATA=1 \
OXID_MOBILE_CUSTODY=development \
OXID_STANDALONE_NETWORK_PROFILE=local \
OXID_MOBILE_PORTAL_PROFILE=local \
  "$repository_root/scripts/run-ios-simulator.sh" \
  >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1

device="${OXID_IOS_DEVICE:-}"
if [ -z "$device" ]; then
  device="$(/usr/bin/xcrun simctl list devices booted -j | jq -r 'first(.devices[][] | select(.isAvailable and (.name | startswith("iPhone"))) | .udid) // empty')"
fi
[ -n "$device" ] || { portal_mobile_fail simulator; exit 1; }
app_bundle="$repository_root/target/dx/oxid-app/debug/ios/OxidApp.app"
bundle_identifier="$(/usr/bin/plutil -extract CFBundleIdentifier raw "$app_bundle/Info.plist")"
[ "$bundle_identifier" = "io.medianox.oxid" ] || { portal_mobile_fail app-id; exit 1; }
app_container="$(/usr/bin/xcrun simctl get_app_container "$device" "$bundle_identifier" data)"
printf '%s' "$device" | curl --noproxy '*' --fail --silent --show-error \
  -X POST --data-binary @- "$PORTAL_MOBILE_CONTROL_ORIGIN/ios-device" >/dev/null
did_store="$app_container/Library/Application Support/io.medianox.oxid/private/did-records.json"
node "$repository_root/scripts/e2e/portal-mobile-holder-sync.mjs" \
  "$did_store" "$PORTAL_MOBILE_CONTROL_ORIGIN" \
  >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1 &
PORTAL_MOBILE_HOLDER_SYNC_PID=$!

xcodegen_output="$(nix build .#xcodegen --no-link --print-out-paths)"
generated_project_root="$repository_root/target/mobile-tests/ios-portal"
mkdir -p "$generated_project_root"
OXID_REPOSITORY_ROOT="$repository_root" \
  "$xcodegen_output/bin/xcodegen" generate \
    --spec "$repository_root/tests/mobile/ios/project.yml" \
    --project "$generated_project_root"

host_user="$(id -un)"
env -i \
  "DEVELOPER_DIR=$xcode_developer_dir" \
  "HOME=$HOME" \
  "LANG=${LANG:-en_US.UTF-8}" \
  "LOGNAME=$host_user" \
  "OXID_PORTAL_CONTROL_ORIGIN=$PORTAL_MOBILE_CONTROL_ORIGIN" \
  "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
  "TMPDIR=${TMPDIR:-/tmp}" \
  "USER=$host_user" \
  /usr/bin/xcodebuild test \
    -project "$generated_project_root/OxidMobileSmoke.xcodeproj" \
    -scheme OxidUITests \
    -destination "platform=iOS Simulator,id=$device" \
    -derivedDataPath "$PORTAL_MOBILE_STATE_DIR/ios-derived-data" \
    -resultBundlePath "$PORTAL_MOBILE_STATE_DIR/ios-results.xcresult" \
    -only-testing:"OxidUITests/PortalFlowTests/testRealPortalOfferUsesStrictWarmColdConsentAndRestoresEncryptedCredential" \
    CODE_SIGNING_ALLOWED=NO \
    >>"$PORTAL_MOBILE_PRIVATE_LOG" 2>&1

kill -TERM "$PORTAL_MOBILE_HOLDER_SYNC_PID" >/dev/null 2>&1 || true
portal_mobile_wait_bounded "$PORTAL_MOBILE_HOLDER_SYNC_PID" \
  "$PORTAL_MOBILE_TERM_GRACE_SECONDS" >/dev/null 2>&1 || true
PORTAL_MOBILE_HOLDER_SYNC_PID=""

credential_store="$app_container/Library/Application Support/io.medianox.oxid/private/credentials.enc"
credential_key="$app_container/Library/Application Support/io.medianox.oxid/private/credentials.key"
credential_header="$(od -An -tx1 -N8 "$credential_store" | tr -d ' \r\n')"
credential_key_size="$(wc -c <"$credential_key" | tr -d ' ')"
[ "$credential_header" = "4f58494456433031" ] && [ "$credential_key_size" = "32" ] || {
  portal_mobile_fail encrypted-store
  exit 1
}
counters="$(curl --noproxy '*' --fail --silent "$PORTAL_MOBILE_CONTROL_ORIGIN/counters")"
jq -e '.token == 2 and .nonce == 1 and .credential == 1' >/dev/null <<<"$counters" || {
  portal_mobile_fail protocol-counts
  exit 1
}

portal_mobile_finish || { portal_mobile_fail support-finish; exit 1; }

device_name="$(/usr/bin/xcrun simctl list devices -j | jq -r --arg device "$device" 'first(.devices[][] | select(.udid == $device) | .name) // "unknown"')"
runtime="$(/usr/bin/xcrun simctl list devices -j | jq -r --arg device "$device" 'first(.devices | to_entries[] as $runtime | $runtime.value[] | select(.udid == $device) | $runtime.key) // "unknown"')"
portal_mobile_assert_evidence_source || exit 1
evidence="$repository_root/target/portal-mobile-e2e/ios/evidence.json"
evidence_directory="$(dirname -- "$evidence")"
mkdir -p "$evidence_directory"
if ! evidence_temp="$(umask 077 && mktemp "$evidence_directory/.evidence.json.tmp.XXXXXX")"; then
  portal_mobile_fail evidence-temp
  exit 1
fi
PORTAL_MOBILE_EVIDENCE_TEMP="$evidence_temp"
chmod 600 "$evidence_temp" || { portal_mobile_fail evidence-temp; exit 1; }
evidence_document='{
  schema:"oxid-portal-mobile-evidence-v1",
  oxid:{head:$head},
  portal:{integrationCommit:$portalCommit,integrationTree:$portalTree,prHead:$prHead,profileSourceCommit:$profileSource,provenanceSha256:$provenance},
  platform:{kind:"ios_simulator",model:$model,os:$os,applicationId:$app,profile:"standalone-local-development-portal"},
  acceptance:{mockKycApproved:true,warmColdCustomScheme:true,oneItemStrictRouter:true,explicitConsent:true,managedAuthenticationProof:true,separateJubjubAssertionBinding:true,strictFinalExchange:true,exactBundleImported:true,encryptedPersistence:true,processRestart:true,developmentCustodyReactivated:true,reverified:true,unavailableDenied:true,timeoutDenied:true,cameraUnavailable:true,secretFreeEvidence:true}
}'
evidence_sentinel='openid-credential-offer|pre-authorized|access[_-]?token|c_nonce|eyJ|did:|https?://|John|Doe|AB1234567|private.?parts|signed.?bytes|detached.?proof|[0-9A-F]{8}-[0-9A-F-]{27}'
if ! jq -cn \
  --arg head "$PORTAL_MOBILE_OXID_HEAD" \
  --arg model "$device_name" \
  --arg os "$runtime" \
  --arg app "$bundle_identifier" \
  --arg portalCommit "$PORTAL_INTEGRATION_COMMIT" \
  --arg portalTree "$PORTAL_INTEGRATION_TREE" \
  --arg prHead "$PORTAL_PR_HEAD" \
  --arg profileSource "$PORTAL_PROFILE_SOURCE" \
  --arg provenance "$PORTAL_PROVENANCE_SHA256" \
  "$evidence_document" >"$evidence_temp"; then
  portal_mobile_discard_evidence_temp "$evidence_temp" || true
  portal_mobile_fail evidence-generate
  exit 1
fi
portal_mobile_finalize_evidence \
  "$evidence" "$evidence_temp" "$evidence_document" "$evidence_sentinel" \
  --arg head "$PORTAL_MOBILE_OXID_HEAD" \
  --arg model "$device_name" \
  --arg os "$runtime" \
  --arg app "$bundle_identifier" \
  --arg portalCommit "$PORTAL_INTEGRATION_COMMIT" \
  --arg portalTree "$PORTAL_INTEGRATION_TREE" \
  --arg prHead "$PORTAL_PR_HEAD" \
  --arg profileSource "$PORTAL_PROFILE_SOURCE" \
  --arg provenance "$PORTAL_PROVENANCE_SHA256" || exit 1
printf 'iOS Portal simulator smoke passed at %s on %s (%s), app %s; evidence=%s\n' \
  "$PORTAL_MOBILE_OXID_HEAD" "$device_name" "$runtime" "$bundle_identifier" "${evidence#"$repository_root/"}"
