# Application target commands

## Command model

Oxid separates target work into three operations:

- **build** compiles an artifact and writes a private exact-source receipt;
- **deploy** verifies that receipt and installs the artifact without rebuilding
  or launching it;
- **run** builds, receipts, installs, and launches in one command.

Run these commands from the pinned Nix shell. Mobile builds retain the existing
compile-time custody, network, Portal-authority, and UI-profile checks.

| Target | Build | Deploy | Build, deploy, and run |
| --- | --- | --- | --- |
| Desktop | `just desktop-build` | not applicable | `just desktop-run` |
| Android | `just android-build` | `just android-deploy` | `just android-run` |
| iOS Simulator | `just ios-build` | `just ios-deploy` | `just ios-run` |

`just run` remains an alias in purpose for `just desktop-run`. The mobile
commands accept the same environment variables as the existing launchers, such
as `OXID_ANDROID_DEVICE`, `OXID_IOS_DEVICE`, `OXID_UI_PROFILE`,
`OXID_MOBILE_CUSTODY`, and `OXID_STANDALONE_NETWORK_PROFILE`.

The ordinary desktop pair keeps Cargo's unoptimized `dev` profile for short
edit, compile, and test loops. Use the dedicated live pair for a standalone
Midnight demo that performs cryptographic replay:

```bash
just desktop-live-build
just desktop-live-run
```

These commands select `desktop,standalone-development,standalone-local` and the
optimized, symbol-retaining `desktop-live` profile. They do not enable release
LTO, stripping, or production-only behavior. Agents must use this pair rather
than the ordinary desktop helpers when qualifying live standalone replay.

The opt-in native proof benchmark has its own development-only desktop pair:

```bash
just desktop-proof-benchmark-build
just desktop-proof-benchmark-run
```

These commands select `desktop,developer-proof-benchmark` explicitly and do not
produce a mobile artifact receipt or a release build. The run command starts
the UI but never starts a proof by itself. Follow the bounded scenario rendered
by `node scripts/demo-inventory.mjs prepare development-proof-benchmark-desktop`
before choosing a circuit.

## Build once, deploy repeatedly

```bash
just android-build
just android-deploy

just ios-build
just ios-deploy
```

Build selects a specific Android ABI or iOS Simulator architecture. Deploy must
select a compatible destination and the same compile-time profile. It rejects
the operation if the commit, source tree, platform target, profile fingerprint,
tracked or untracked source-state digest, artifact path, artifact content, or
private mode-`0600` receipt differs from the build. Re-run the build instead of
editing a receipt.

The iOS build command does not select, boot, install to, or open a Simulator.
The Android build still selects an online device or starts the configured AVD
to derive the exact ABI, but it does not install, launch, or add local reverse
routes. Set `OXID_ANDROID_DEVICE` or `OXID_ANDROID_AVD` to make that selection
deterministic.

The receipts are untracked build state:

```text
target/dx/oxid-app/debug/android/oxid-app-artifact-receipt.json
target/dx/oxid-app/debug/ios/oxid-app-artifact-receipt.json
```

## Android release candidate

For owner/supervisor review, build one arm64 release candidate without any ADB,
device, or AVD interaction:

```bash
nix develop --command just android-release-build
```

The command writes its deterministic artifact and a mode-`0600` receipt at:

```text
target/android-release-candidate/oxid-app-arm64-v8a-release.apk
target/android-release-candidate/receipt.json
```

It requires both reviewed Android platforms: API 34 for the Dioxus-generated
application and API 35 for the tracked native plugin, plus inspection
build-tools `35.0.0` with both `aapt` and `zipalign`, and NDK `27.0.12077973`. Only
`OXID_ANDROID_BUILD_TOOLS_VERSION` and `OXID_ANDROID_NDK_VERSION` select
installed reviewed tool versions; there is no application compile-SDK override,
because it does not control Dioxus's generated application project. The private
receipt binds the exact source head/tree, artifact digest/size/ABI, Nix and
nixpkgs revision, exact rustup Rust/Cargo binaries, Gradle wrapper and Android
Gradle Plugin, both Android platform revisions with their application/native-
plugin roles, inspection build-tools, and NDK versions. It records no local
paths, device IDs, signing material, or app data. Gradle packaging keeps the
Android Gradle Plugin's default build-tools selection; the receipt does not
mislabel the independently selected inspection tools as packaging tools.
It does record the generated wrapper's signing *kind*: Dioxus `--release` uses
Rust profile `android-release`, while the generated Android wrapper packages
Gradle variant `debug` with generated debug signing. This is a review
candidate, not a Play-signed or Gradle-release artifact.

The command captures a clean issue worktree and its HEAD/tree before the build,
then immediately before publishing the receipt fails closed unless the worktree
is still clean and those identities are unchanged (ignored generated outputs
such as `target/` may remain). An exclusive candidate-build lock prevents two
same-worktree invocations from sharing mutable build and receipt paths. It
performs exactly one Dioxus build using both
documented 16 KiB linker flags: `-Wl,-z,max-page-size=16384` and
`-Wl,-z,common-page-size=16384`. It first copies the resulting APK to the
private deterministic artifact snapshot; every subsequent 16 KiB, official
`zipalign -c -P 16 -v 4`, `aapt`, and digest check uses that snapshot only. It
fails closed unless `aapt` reports the complete native ABI set as exactly
`["arm64-v8a"]`, its package is `io.medianox.oxid`, application compile SDK is
34, min SDK is 23, and target SDK is 35. The receipt records those observed APK
badging values, including the complete ABI array, rather than inferring them
from an installed platform.

This command does not select, boot, install to, or launch an Android target.
The supervisor separately owns the reviewed Android 15+ 16 KiB target,
page-size observation, install, launch, and smoke evidence.

Smoke that exact candidate on a disposable Android emulator without rebuilding
or selecting a different artifact:

```bash
nix develop --command just android-smoke-prebuilt
```

The recipe admits only the exact mode-`0600` release receipt and APK produced
by `just android-release-build`. It verifies the receipt's source head/tree and
APK SHA-256, then independently checks the package, arm64 ABI, and launch
activity before installation. The smoke refuses physical devices and emulators
that already have a device credential. It creates an ephemeral credential for
the native-authorized recovery-phrase journey, never prints that credential or
the device identifier, and clears only its application data, forwarding, and
harness-owned credential on exit. To select another receipt-bound copy, pass
both paths explicitly:

```bash
nix develop --command just android-smoke-prebuilt \
  /absolute/path/to/oxid-app-arm64-v8a-release.apk \
  /absolute/path/to/receipt.json
```

Android deployment supports an explicitly selected physical device or emulator
accepted by the existing launcher policy. The default local profile accepts an
emulator; the reviewed Tailnet Portal path owns physical-device configuration.

## Android 16 KiB native-library inspection

Inspect an already-built APK without building, installing, launching, or
selecting an Android target:

```bash
just android-verify-16k
# For a milestone/release candidate:
just android-verify-16k /path/to/app-release.apk
```

The command checks every packaged `.so` member. An uncompressed member must
have 16 KiB-aligned ZIP data placement; a Deflate-compressed member is decoded
before inspection because Android extracts it instead of directly mapping the
archive entry. Every decoded ELF must have compatible `LOAD` alignment and
congruent file/virtual offsets. Failures name the exact archive-member path.
Native members are bounded to 512 MiB individually and in aggregate during
inspection, so neither a real debug library nor malformed compressed metadata
can demand an unbounded loader/decompression allocation.
The default is the existing local Android build output; a release candidate
must be passed explicitly. The hermetic fixtures cover compliant and
non-compliant ZIP/ELF cases, so neither an APK build nor an Android SDK is
needed to test the verifier.

The non-release Android path uses Dioxus's `android-dev` Cargo profile. Oxid
keeps incremental compilation and development assertions, enables light
optimization, retains limited line information, and strips full debug objects.
The launcher also supplies the NDK's documented `max-page-size=16384` and
`common-page-size=16384` linker constraints; the profile alone does not change
ELF LOAD alignment. Its version script preserves the Dioxus application entry,
Java/JNI, and NativeActivity symbols in every Android `cdylib`, while keeping
Rust implementation symbols out of Bionic's dynamic hash tables.
After every development build, the launcher checks 16 KiB ZIP/ELF alignment,
requires a non-empty SysV or GNU dynamic hash table, and bounds both native
library bytes and dynamic symbols **before** it writes an artifact receipt or
installs the APK. This is a development profile, not a disguised release build;
the independently receipt-bound `android-release` candidate remains unchanged.

This is an on-demand static artifact check. It is not evidence that an APK was
built with the pinned Android/Gradle/NDK toolchain and it does not replace the
supervisor-owned bounded 16 KiB-capable virtual/physical target smoke. Keep
artifact hashes, ABI set, tool versions, and target page-size evidence in the
final private/public review receipt as appropriate; never include device serials
or local SDK paths.

The release evidence follows Android's official
[16 KiB page-size guidance](https://developer.android.com/guide/practices/page-sizes),
including an independent `zipalign -c -P 16 -v 4 <apk>` cross-check when the
pinned Android build tools are available.

`ios-deploy` installs only into iOS Simulator. Physical iOS deployment is not
implemented because it requires an owner-approved signing, provisioning, and
device policy. These commands do not publish to an application store and do
not produce release artifacts.

## Profiles and cleanup

The receipt makes a build reusable, not portable between configurations. For
example, use the same environment on both commands:

```bash
OXID_UI_PROFILE=dev just android-build
OXID_UI_PROFILE=dev just android-deploy
```

Mobile deploy does not clear application data by default. Set the existing
`OXID_IOS_RESET_DATA=1` only when installing to iOS Simulator through deploy or
run. Android data resets remain owned by the explicit test/demo lifecycle that
requested them.
