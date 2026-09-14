// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmod, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

test("release-candidate build is arm64-only, statically verified, and device-free", async () => {
  const [script, justfile, guide, nativePluginGradle] = await Promise.all([
    readFile(path.join(root, "scripts", "build-android-release-candidate.sh"), "utf8"),
    readFile(path.join(root, "Justfile"), "utf8"),
    readFile(path.join(root, "docs", "factory", "application-targets.md"), "utf8"),
    readFile(path.join(root, "crates", "adapters", "mobile-native-plugin", "android", "build.gradle.kts"), "utf8"),
  ]);

  assert.match(justfile, /^android-release-build:\n    \.\/scripts\/build-android-release-candidate\.sh$/m);
  assert.match(script, /--release/);
  assert.match(script, /--target aarch64-linux-android/);
  assert.match(script, /target\/android-release-candidate\/oxid-app-arm64-v8a-release\.apk/);
  assert.match(script, /node scripts\/android-verify-16k\.mjs "\$raw_artifact"/);
  assert.match(nativePluginGradle, /compileSdk = 35/);
  assert.match(script, /application_compile_sdk="34"/);
  assert.match(script, /plugin_compile_sdk="35"/);
  assert.doesNotMatch(script, /OXID_ANDROID_COMPILE_SDK/);
  assert.match(script, /platform_revision "\$application_compile_sdk" application/);
  assert.match(script, /platform_revision "\$plugin_compile_sdk" native-plugin/);
  assert.match(script, /aapt="\$android_sdk\/build-tools\/\$build_tools_version\/aapt"/);
  assert.match(script, /with aapt and zipalign is not installed/);
  assert.match(script, /"\$zipalign" -c -P 16 -v 4 "\$raw_artifact"/);
  assert.match(script, /"\$aapt" dump badging "\$raw_artifact"/);
  assert.match(script, /sed -n "s\/\^package: \.\*compileSdkVersion='/);
  assert.match(script, /APK package is not io\.medianox\.oxid/);
  assert.match(script, /APK native ABI is not arm64-v8a/);
  assert.match(script, /APK application compileSdk is not \$application_compile_sdk/);
  assert.match(script, /APK minSdk is not 23/);
  assert.match(script, /APK targetSdk is not 35/);
  assert.match(script, /platforms:\{application:\{api:\$applicationCompileSdk,revision:\$applicationSdkPlatformRevision\},nativePlugin:\{api:\$pluginCompileSdk,revision:\$pluginSdkPlatformRevision\}\}/);
  assert.match(script, /apk:\{package:\$apkPackage,abi:\$apkAbi,compileSdk:\$apkCompileSdk,minSdk:\$apkMinSdk,targetSdk:\$apkTargetSdk\}/);
  const permanentFlags = "-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384";
  assert.match(script, new RegExp(`linker_flags="${permanentFlags}"`));
  assert.equal((script.match(/build "\$linker_flags"/g) ?? []).length, 1);
  assert.doesNotMatch(script, /if ! node scripts\/android-verify-16k\.mjs/);
  assert.match(script, /git status --porcelain --untracked-files=all/);
  assert.match(script, /rustProfile "android-release"/);
  assert.match(script, /gradleVariant "debug"/);
  assert.match(script, /signing "generated-debug"/);
  assert.match(script, /dioxus:\{release:true,rustProfile:\$rustProfile\}/);
  assert.match(script, /androidWrapper:\{gradleVariant:\$gradleVariant,signing:\$signing\}/);
  assert.match(script, /chmod 600 "\$receipt"/);
  assert.doesNotMatch(script, /^\s*adb\s|OXID_ANDROID_DEVICE|OXID_ANDROID_AVD|am start/m);
  assert.match(guide, /just android-release-build/);
  assert.match(guide, /does not select, boot, install to, or launch/);
  assert.match(guide, /API 34 for the Dioxus-generated\napplication and API 35 for the tracked native plugin/);
  assert.match(guide, /both `aapt` and `zipalign`/);
  assert.match(guide, /application compile SDK\nis 34, min SDK is 23, and target SDK is 35/);
  assert.match(guide, /there is no application compile-SDK override/);
  assert.match(guide, /Rust profile `android-release`/);
  assert.match(guide, /Gradle variant `debug` with generated debug signing/);
});

test("release-candidate build fails before invoking Nix when its worktree is dirty", async (t) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "oxid-android-release-guard-"));
  t.after(() => rm(temporaryRoot, { recursive: true, force: true }));
  const scriptsDirectory = path.join(temporaryRoot, "scripts");
  const fakeBin = path.join(temporaryRoot, "bin");
  const sdk = path.join(temporaryRoot, "sdk");
  const marker = path.join(temporaryRoot, "nix-was-invoked");
  const scriptPath = path.join(scriptsDirectory, "build-android-release-candidate.sh");

  await Promise.all([
    mkdir(scriptsDirectory, { recursive: true }),
    mkdir(fakeBin, { recursive: true }),
    mkdir(path.join(sdk, "platforms", "android-34"), { recursive: true }),
    mkdir(path.join(sdk, "platforms", "android-35"), { recursive: true }),
    mkdir(path.join(sdk, "build-tools", "35.0.0"), { recursive: true }),
    mkdir(path.join(sdk, "ndk", "27.0.12077973"), { recursive: true }),
  ]);
  await copyFile(path.join(root, "scripts", "build-android-release-candidate.sh"), scriptPath);
  await chmod(scriptPath, 0o755);
  await writeFile(path.join(sdk, "platforms", "android-34", "source.properties"), "Pkg.Revision = 1\n");
  await writeFile(path.join(sdk, "platforms", "android-35", "source.properties"), "Pkg.Revision = 1\n");
  await writeFile(path.join(sdk, "ndk", "27.0.12077973", "source.properties"), "Pkg.Revision = 27.0.12077973\n");
  await writeFile(path.join(sdk, "build-tools", "35.0.0", "aapt"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  await writeFile(path.join(sdk, "build-tools", "35.0.0", "zipalign"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  await writeFile(path.join(fakeBin, "nix"), `#!/bin/sh\nprintf invoked >"${marker}"\nexit 1\n`, { mode: 0o755 });

  await run("git", ["init", "-q"], temporaryRoot);
  await run("git", ["config", "user.email", "test@example.invalid"], temporaryRoot);
  await run("git", ["config", "user.name", "Test"], temporaryRoot);
  await writeFile(path.join(temporaryRoot, ".gitignore"), "/bin/\n/sdk/\n");
  await run("git", ["add", "scripts/build-android-release-candidate.sh", ".gitignore"], temporaryRoot);
  await run("git", ["commit", "-qm", "fixture"], temporaryRoot);
  await writeFile(path.join(temporaryRoot, "uncommitted-source-input"), "dirty\n");

  const result = await run(scriptPath, [], temporaryRoot, {
    ...process.env,
    ANDROID_HOME: sdk,
    PATH: `${fakeBin}${path.delimiter}${process.env.PATH}`,
  }, false);
  assert.notEqual(result.code, 0);
  assert.match(result.stderr, /source worktree is not clean/);
  await assert.rejects(readFile(marker));

  await rm(path.join(temporaryRoot, "uncommitted-source-input"));
  const ignoredOnlyResult = await run(scriptPath, [], temporaryRoot, {
    ...process.env,
    ANDROID_HOME: sdk,
    PATH: `${fakeBin}${path.delimiter}${process.env.PATH}`,
  }, false);
  assert.notEqual(ignoredOnlyResult.code, 0);
  assert.equal(await readFile(marker, "utf8"), "invoked");
});

function run(command, args, cwd, env = process.env, rejectOnFailure = true) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, env, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", reject);
    child.on("close", (code) => {
      const result = { code, stdout, stderr };
      if (code !== 0 && rejectOnFailure) reject(new Error(`${command} failed: ${stderr}`));
      else resolve(result);
    });
  });
}
