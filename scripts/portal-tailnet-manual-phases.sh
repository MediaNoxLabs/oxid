#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Receipt-bound, state-preserving Android Portal manual phases.
set -euo pipefail

readonly ROOT="$(cd -- "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
readonly STATE="${OXID_PORTAL_PHASE_STATE_DIR:-$ROOT/target/portal-tailnet-manual/phases}"
readonly OPERATION="${1:-}"
readonly CONFIGURE_RECEIPT="$STATE/configure-receipt.json"
readonly BUILD_RECEIPT="$STATE/build-receipt.json"
readonly ADMIT_RECEIPT="$STATE/admit-receipt.json"
readonly INSTALL_RECEIPT="$STATE/install-receipt.json"
readonly LAUNCH_RECEIPT="$STATE/launch-receipt.json"
readonly APK="$ROOT/target/dx/oxid-app/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
readonly ARTIFACT_RECEIPT="$ROOT/target/dx/oxid-app/debug/android/oxid-app-artifact-receipt.json"
readonly PACKAGE="io.medianox.oxid"

fail() { printf 'portal-tailnet-manual-phases: FAIL phase=%s\n' "$1" >&2; exit 1; }
case "$OPERATION" in configure|build|admit|install|launch|status) ;; *) fail usage ;; esac
for command_name in git jq shasum; do command -v "$command_name" >/dev/null 2>&1 || fail missing-tool; done
umask 077
mkdir -p "$STATE"; chmod 700 "$STATE"
[ -d "$STATE" ] && [ ! -L "$STATE" ] || fail state
head="$(git -C "$ROOT" rev-parse HEAD)"; tree="$(git -C "$ROOT" rev-parse 'HEAD^{tree}')"
receipt_mode() { [ -f "$1" ] && [ ! -L "$1" ] && [ "$(stat -f '%Lp' "$1" 2>/dev/null || stat -c '%a' "$1")" = 600 ]; }
sha() { shasum -a 256 "$1" | awk '{print $1}'; }
write_receipt() { local target="$1" body="$2" candidate; candidate="$(mktemp "$STATE/.receipt.XXXXXX")"; printf '%s\n' "$body" >"$candidate"; chmod 600 "$candidate"; mv "$candidate" "$target"; }
config_valid() {
  receipt_mode "$CONFIGURE_RECEIPT" && jq -e --arg head "$head" --arg tree "$tree" '
    .schema == "oxid-portal-manual-configure-v1" and .source == {head:$head,tree:$tree}
    and .profile == "tailnet-android" and (.manifest.sha256 | test("^[0-9a-f]{64}$"))
    and (.manifest.schema | type == "string")' "$CONFIGURE_RECEIPT" >/dev/null
}
build_valid() {
  receipt_mode "$BUILD_RECEIPT" && config_valid && jq -e --arg config "$(sha "$CONFIGURE_RECEIPT")" --arg head "$head" --arg tree "$tree" '
    .schema == "oxid-portal-manual-build-v1" and .source == {head:$head,tree:$tree}
    and .predecessor.configureSha256 == $config and (.artifact.sha256 | test("^[0-9a-f]{64}$"))' "$BUILD_RECEIPT" >/dev/null
}
admit_valid() {
  receipt_mode "$ADMIT_RECEIPT" && build_valid && jq -e --arg build "$(sha "$BUILD_RECEIPT")" '
    .schema == "oxid-portal-manual-admit-v1" and .predecessor.buildSha256 == $build
    and .admission == {signing:"verified",schema:"verified",profile:"verified"}' "$ADMIT_RECEIPT" >/dev/null
}
install_valid() {
  receipt_mode "$INSTALL_RECEIPT" && admit_valid && jq -e --arg admit "$(sha "$ADMIT_RECEIPT")" '
    .schema == "oxid-portal-manual-install-v1" and .predecessor.admitSha256 == $admit
    and .applicationDataCleared == false and (.device.sha256 | test("^[0-9a-f]{64}$"))' "$INSTALL_RECEIPT" >/dev/null
}
launch_valid() {
  receipt_mode "$LAUNCH_RECEIPT" && install_valid && jq -e --arg install "$(sha "$INSTALL_RECEIPT")" '
    .schema == "oxid-portal-manual-launch-v1" and .predecessor.installSha256 == $install
    and .applicationDataCleared == false and .outcome == "running"' "$LAUNCH_RECEIPT" >/dev/null
}
android_phase() {
  OXID_MOBILE_CUSTODY=development \
  OXID_MOBILE_PORTAL_PROFILE=tailnet-android \
  OXID_BUILD_PORTAL_PUBLIC_ORIGIN="$(jq -r '.manifest.issuerOrigin' "$CONFIGURE_RECEIPT")" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH="$(jq -r '.manifest.path' "$CONFIGURE_RECEIPT")" \
  OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_SHA256="$(jq -r '.manifest.sha256' "$CONFIGURE_RECEIPT")" \
    "$ROOT/scripts/run-android-tailnet.sh" "$1"
}

case "$OPERATION" in
  configure)
    # The manifest is public deployment metadata; this receipt records only its identity.
    manifest="${OXID_BUILD_PORTAL_DEPLOYMENT_MANIFEST_PATH:-}"
    [[ "$manifest" = /* ]] && [ -f "$manifest" ] && [ ! -L "$manifest" ] || fail manifest
    manifest_sha="$(sha "$manifest")"
    jq -e --arg sha "$manifest_sha" '
      .schema == "oxid-portal-deployment-v3" and (.issuerOrigin | type == "string")
      and (.issuerDid | type == "string") and (.issuerMethod | type == "string")
      and (.issuerJubjubJwkSha256 | test("^[0-9a-f]{64}$"))
      and (tostring | test("(?i)(token|secret|capability|privateKey|wallet_seed)") | not)' "$manifest" >/dev/null || fail manifest-schema
    if config_valid; then exit 0; fi
    [ ! -e "$CONFIGURE_RECEIPT" ] || fail stale-configure-receipt
    write_receipt "$CONFIGURE_RECEIPT" "$(jq -cn --arg head "$head" --arg tree "$tree" --arg sha "$manifest_sha" --arg path "$manifest" --arg schema "$(jq -r .schema "$manifest")" --arg origin "$(jq -r .issuerOrigin "$manifest")" '{schema:"oxid-portal-manual-configure-v1",source:{head:$head,tree:$tree},profile:"tailnet-android",manifest:{path:$path,sha256:$sha,schema:$schema,issuerOrigin:$origin}}')"
    ;;
  build)
    config_valid || fail configure-receipt
    if build_valid; then exit 0; fi
    [ ! -e "$BUILD_RECEIPT" ] || fail stale-build-receipt
    android_phase build
    [ -f "$APK" ] && [ -f "$ARTIFACT_RECEIPT" ] || fail artifact
    write_receipt "$BUILD_RECEIPT" "$(jq -cn --arg head "$head" --arg tree "$tree" --arg config "$(sha "$CONFIGURE_RECEIPT")" --arg apk "$(sha "$APK")" '{schema:"oxid-portal-manual-build-v1",source:{head:$head,tree:$tree},predecessor:{configureSha256:$config},artifact:{sha256:$apk}}')"
    ;;
  admit)
    build_valid || fail build-receipt
    if admit_valid; then exit 0; fi
    [ ! -e "$ADMIT_RECEIPT" ] || fail stale-admit-receipt
    jq -e --arg sha "$(sha "$APK")" '.artifact.sha256 == $sha and .checks.androidVerify16k == "pass"' "$ARTIFACT_RECEIPT" >/dev/null || fail signing
    write_receipt "$ADMIT_RECEIPT" "$(jq -cn --arg build "$(sha "$BUILD_RECEIPT")" '{schema:"oxid-portal-manual-admit-v1",predecessor:{buildSha256:$build},admission:{signing:"verified",schema:"verified",profile:"verified"}}')"
    ;;
  install)
    admit_valid || fail admit-receipt
    if install_valid; then exit 0; fi
    [ ! -e "$INSTALL_RECEIPT" ] || fail stale-install-receipt
    android_phase deploy
    device="${OXID_ANDROID_DEVICE:-}"; [ -n "$device" ] || fail device
    write_receipt "$INSTALL_RECEIPT" "$(jq -cn --arg admit "$(sha "$ADMIT_RECEIPT")" --arg device "$(printf '%s' "$device" | shasum -a 256 | awk '{print $1}')" --arg apk "$(sha "$APK")" '{schema:"oxid-portal-manual-install-v1",predecessor:{admitSha256:$admit},device:{sha256:$device},artifact:{sha256:$apk},applicationDataCleared:false}')"
    ;;
  launch)
    install_valid || fail install-receipt
    if launch_valid; then exit 0; fi
    [ ! -e "$LAUNCH_RECEIPT" ] || fail stale-launch-receipt
    android_phase run
    write_receipt "$LAUNCH_RECEIPT" "$(jq -cn --arg install "$(sha "$INSTALL_RECEIPT")" '{schema:"oxid-portal-manual-launch-v1",predecessor:{installSha256:$install},outcome:"running",applicationDataCleared:false}')"
    ;;
  status)
    launch_valid && printf '%s\n' 'portal-tailnet-manual-phases: READY' || fail incomplete
    ;;
esac
