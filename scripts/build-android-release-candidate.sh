#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Build and statically certify an arm64 Android release candidate. This command
# deliberately never queries adb or selects a device/AVD.
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

android_sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
build_tools_version="${OXID_ANDROID_BUILD_TOOLS_VERSION:-35.0.0}"
ndk_version="${OXID_ANDROID_NDK_VERSION:-27.0.12077973}"
application_compile_sdk="34"
plugin_compile_sdk="35"
artifact_directory="$repository_root/target/android-release-candidate"
artifact="$artifact_directory/oxid-app-arm64-v8a-release.apk"
receipt="$artifact_directory/receipt.json"
raw_artifact="$repository_root/target/dx/oxid-app/release/android/app/app/build/outputs/apk/debug/app-debug.apk"
gradle_wrapper_properties="$repository_root/target/dx/oxid-app/release/android/app/gradle/wrapper/gradle-wrapper.properties"

fail() {
  echo "android release candidate: $*" >&2
  exit 1
}

# HEAD and its tree identify source inputs only when no tracked or untracked
# source changes exist. Ignored generated outputs (including target/) remain
# allowed so this command can write its artifact and receipt.
[ -z "$(git status --porcelain --untracked-files=all)" ] \
  || fail "source worktree is not clean; refusing to build an unverifiable receipt"
head="$(git rev-parse HEAD)"
tree="$(git rev-parse 'HEAD^{tree}')"

for command_name in nix rustup java node jq shasum; do
  command -v "$command_name" >/dev/null 2>&1 || fail "required command '$command_name' is missing"
done
[ -n "$android_sdk" ] || fail "ANDROID_HOME or ANDROID_SDK_ROOT is required"
platform_revision() {
  local api="$1" role="$2" revision
  [ -d "$android_sdk/platforms/android-$api" ] || fail "$role Android platform API $api is not installed"
  [ -f "$android_sdk/platforms/android-$api/source.properties" ] || fail "$role Android platform API $api has no package revision"
  revision="$(awk -F= '$1 ~ /^[[:space:]]*Pkg.Revision[[:space:]]*$/ { value=$2; gsub(/^[[:space:]]+|[[:space:]]+$/, "", value); print value; exit }' "$android_sdk/platforms/android-$api/source.properties")"
  [ -n "$revision" ] || fail "$role Android platform API $api has no package revision"
  printf '%s' "$revision"
}
application_sdk_platform_revision="$(platform_revision "$application_compile_sdk" application)"
plugin_sdk_platform_revision="$(platform_revision "$plugin_compile_sdk" native-plugin)"
aapt="$android_sdk/build-tools/$build_tools_version/aapt"
zipalign="$android_sdk/build-tools/$build_tools_version/zipalign"
[ -x "$aapt" ] && [ -x "$zipalign" ] || fail "Android build-tools $build_tools_version with aapt and zipalign is not installed"
android_ndk="$android_sdk/ndk/$ndk_version"
[ -d "$android_ndk" ] || fail "Android NDK $ndk_version is not installed"
[ "$(awk -F= '$1 ~ /^[[:space:]]*Pkg.Revision[[:space:]]*$/ { value=$2; gsub(/^[[:space:]]+|[[:space:]]+$/, "", value); print value; exit }' "$android_ndk/source.properties")" = "$ndk_version" ] \
  || fail "Android NDK directory does not report $ndk_version"

# The receipt deliberately records only versions and immutable identities. It
# contains no local paths, devices, signing configuration, or app data.
nix_version="$(nix --version)"
nixpkgs_revision="$(nix flake metadata --json | jq -r '.locks.nodes.nixpkgs.locked.rev')"
rustc_version="$(rustc --version)"
cargo_version="$(cargo --version)"

rustup target add aarch64-linux-android
rust_toolchain_bin="$(dirname -- "$(rustup which cargo)")"
dioxus_output="$(nix build .#dioxus-cli --no-link --print-out-paths)"
dioxus_cli="$dioxus_output/bin/dx"
[ -x "$dioxus_cli" ] || fail "Nix dioxus-cli output has no dx executable"

build() {
  local rustflags="${1:-}"
  ANDROID_HOME="$android_sdk" \
  ANDROID_SDK_ROOT="$android_sdk" \
  ANDROID_NDK_HOME="$android_ndk" \
  RUSTFLAGS="$rustflags" \
  GRADLE_OPTS="-Dorg.gradle.daemon=false" \
  KOTLIN_COMPILER_EXECUTION_STRATEGY=in-process \
  PATH="$rust_toolchain_bin:$android_sdk/platform-tools:/usr/bin:$PATH" \
    "$dioxus_cli" build \
      --android \
      --release \
      --package oxid-app \
      --no-default-features \
      --features mobile,standalone-development \
      --target aarch64-linux-android \
      --locked
}

# The measured NDK r27 build requires both documented flags for 16 KiB ELF
# LOAD alignment. Build exactly once with the permanent configuration.
linker_flags="-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384"
build "$linker_flags"
[ -f "$raw_artifact" ] || fail "Dioxus did not create the release APK"

node scripts/android-verify-16k.mjs "$raw_artifact"
"$zipalign" -c -P 16 -v 4 "$raw_artifact"
apk_badging="$("$aapt" dump badging "$raw_artifact")" || fail "could not inspect APK badging"
apk_package="$(printf '%s\n' "$apk_badging" | awk -F"'" '/^package: / { print $2; exit }')"
apk_abi="$(printf '%s\n' "$apk_badging" | awk -F"'" '/^native-code: / { print $2; exit }')"
apk_compile_sdk="$(printf '%s\n' "$apk_badging" | awk -F"'" '/^compileSdkVersion:/{ print $2; exit }')"
apk_min_sdk="$(printf '%s\n' "$apk_badging" | awk -F"'" '/^sdkVersion:/{ print $2; exit }')"
apk_target_sdk="$(printf '%s\n' "$apk_badging" | awk -F"'" '/^targetSdkVersion:/{ print $2; exit }')"
[ "$apk_package" = "io.medianox.oxid" ] || fail "APK package is not io.medianox.oxid"
[ "$apk_abi" = "arm64-v8a" ] || fail "APK native ABI is not arm64-v8a"
[ "$apk_compile_sdk" = "$application_compile_sdk" ] || fail "APK application compileSdk is not $application_compile_sdk"
[ "$apk_min_sdk" = "23" ] || fail "APK minSdk is not 23"
[ "$apk_target_sdk" = "35" ] || fail "APK targetSdk is not 35"

gradle_version="$(awk -F= '/^distributionUrl=/ { value=$2; sub(/^.*gradle-/, "", value); sub(/-bin\.zip$/, "", value); print value; exit }' "$gradle_wrapper_properties")"
[ -n "$gradle_version" ] || fail "could not determine the generated Gradle wrapper version"

mkdir -p "$artifact_directory"
cp "$raw_artifact" "$artifact"
chmod 600 "$artifact"
artifact_sha256="$(shasum -a 256 "$artifact" | awk '{ print $1 }')"
artifact_bytes="$(wc -c < "$artifact" | tr -d ' ')"
umask 077
jq -n \
  --arg head "$head" --arg tree "$tree" \
  --arg artifactSha256 "$artifact_sha256" --argjson artifactBytes "$artifact_bytes" \
  --arg nix "$nix_version" --arg nixpkgsRevision "$nixpkgs_revision" \
  --arg rustc "$rustc_version" --arg cargo "$cargo_version" --arg gradle "$gradle_version" \
  --arg applicationCompileSdk "$application_compile_sdk" --arg applicationSdkPlatformRevision "$application_sdk_platform_revision" \
  --arg pluginCompileSdk "$plugin_compile_sdk" --arg pluginSdkPlatformRevision "$plugin_sdk_platform_revision" \
  --arg buildTools "$build_tools_version" --arg ndk "$ndk_version" \
  --arg apkPackage "$apk_package" --arg apkAbi "$apk_abi" --arg apkCompileSdk "$apk_compile_sdk" \
  --arg apkMinSdk "$apk_min_sdk" --arg apkTargetSdk "$apk_target_sdk" \
  --arg linkerFlags "$linker_flags" --arg rustProfile "android-release" \
  --arg gradleVariant "debug" --arg signing "generated-debug" \
  '{schema:"oxid-android-release-candidate-receipt-v1",source:{head:$head,tree:$tree},artifact:{name:"oxid-app-arm64-v8a-release.apk",sha256:$artifactSha256,bytes:$artifactBytes,abis:["arm64-v8a"]},tools:{nix:$nix,nixpkgsRevision:$nixpkgsRevision,rustc:$rustc,cargo:$cargo,gradle:$gradle,android:{platforms:{application:{api:$applicationCompileSdk,revision:$applicationSdkPlatformRevision},nativePlugin:{api:$pluginCompileSdk,revision:$pluginSdkPlatformRevision}},buildTools:$buildTools,ndk:$ndk}},apk:{package:$apkPackage,abi:$apkAbi,compileSdk:$apkCompileSdk,minSdk:$apkMinSdk,targetSdk:$apkTargetSdk},build:{dioxus:{release:true,rustProfile:$rustProfile},androidWrapper:{gradleVariant:$gradleVariant,signing:$signing},linkerFlags:$linkerFlags},checks:{androidVerify16k:"pass",zipalignPage16k:"pass",apkBadging:"pass"}}' \
  >"$receipt"
chmod 600 "$receipt"

echo "android-release-candidate: PASS artifact=target/android-release-candidate/oxid-app-arm64-v8a-release.apk receipt=target/android-release-candidate/receipt.json"
