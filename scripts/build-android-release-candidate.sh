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
compile_sdk="${OXID_ANDROID_COMPILE_SDK:-35}"
artifact_directory="$repository_root/target/android-release-candidate"
artifact="$artifact_directory/oxid-app-arm64-v8a-release.apk"
receipt="$artifact_directory/receipt.json"
raw_artifact="$repository_root/target/dx/oxid-app/release/android/app/app/build/outputs/apk/debug/app-debug.apk"
gradle_wrapper_properties="$repository_root/target/dx/oxid-app/release/android/app/gradle/wrapper/gradle-wrapper.properties"

fail() {
  echo "android release candidate: $*" >&2
  exit 1
}

for command_name in nix rustup java node jq shasum; do
  command -v "$command_name" >/dev/null 2>&1 || fail "required command '$command_name' is missing"
done
[ -n "$android_sdk" ] || fail "ANDROID_HOME or ANDROID_SDK_ROOT is required"
[ -d "$android_sdk/platforms/android-$compile_sdk" ] || fail "Android platform android-$compile_sdk is not installed"
sdk_platform_revision="$(awk -F= '$1 ~ /^[[:space:]]*Pkg.Revision[[:space:]]*$/ { value=$2; gsub(/^[[:space:]]+|[[:space:]]+$/, "", value); print value; exit }' "$android_sdk/platforms/android-$compile_sdk/source.properties")"
[ -n "$sdk_platform_revision" ] || fail "Android platform android-$compile_sdk has no package revision"
zipalign="$android_sdk/build-tools/$build_tools_version/zipalign"
[ -x "$zipalign" ] || fail "Android build-tools $build_tools_version with zipalign is not installed"
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

# NDK r27 alone does not guarantee 16 KiB ELF LOAD alignment. Measure its
# ordinary output first; only then use Android's documented linker flags.
linker_flags=""
build "$linker_flags"
[ -f "$raw_artifact" ] || fail "Dioxus did not create the release APK"
if ! node scripts/android-verify-16k.mjs "$raw_artifact"; then
  linker_flags="-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384"
  build "$linker_flags"
  [ -f "$raw_artifact" ] || fail "Dioxus did not create the remediated release APK"
fi

node scripts/android-verify-16k.mjs "$raw_artifact"
"$zipalign" -c -P 16 -v 4 "$raw_artifact"

gradle_version="$(awk -F= '/^distributionUrl=/ { value=$2; sub(/^.*gradle-/, "", value); sub(/-bin\.zip$/, "", value); print value; exit }' "$gradle_wrapper_properties")"
[ -n "$gradle_version" ] || fail "could not determine the generated Gradle wrapper version"

mkdir -p "$artifact_directory"
cp "$raw_artifact" "$artifact"
chmod 600 "$artifact"
artifact_sha256="$(shasum -a 256 "$artifact" | awk '{ print $1 }')"
artifact_bytes="$(wc -c < "$artifact" | tr -d ' ')"
head="$(git rev-parse HEAD)"
tree="$(git rev-parse 'HEAD^{tree}')"

umask 077
jq -n \
  --arg head "$head" --arg tree "$tree" \
  --arg artifactSha256 "$artifact_sha256" --argjson artifactBytes "$artifact_bytes" \
  --arg nix "$nix_version" --arg nixpkgsRevision "$nixpkgs_revision" \
  --arg rustc "$rustc_version" --arg cargo "$cargo_version" --arg gradle "$gradle_version" \
  --arg compileSdk "$compile_sdk" --arg sdkPlatformRevision "$sdk_platform_revision" \
  --arg buildTools "$build_tools_version" --arg ndk "$ndk_version" \
  --arg linkerFlags "$linker_flags" \
  '{schema:"oxid-android-release-candidate-receipt-v1",source:{head:$head,tree:$tree},artifact:{name:"oxid-app-arm64-v8a-release.apk",sha256:$artifactSha256,bytes:$artifactBytes,abis:["arm64-v8a"]},tools:{nix:$nix,nixpkgsRevision:$nixpkgsRevision,rustc:$rustc,cargo:$cargo,gradle:$gradle,android:{compileSdk:$compileSdk,sdkPlatformRevision:$sdkPlatformRevision,buildTools:$buildTools,ndk:$ndk}},build:{linkerFlags:$linkerFlags},checks:{androidVerify16k:"pass",zipalignPage16k:"pass"}}' \
  >"$receipt"
chmod 600 "$receipt"

echo "android-release-candidate: PASS artifact=target/android-release-candidate/oxid-app-arm64-v8a-release.apk receipt=target/android-release-candidate/receipt.json"
