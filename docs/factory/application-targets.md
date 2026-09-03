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

Android deployment supports an explicitly selected physical device or emulator
accepted by the existing launcher policy. The default local profile accepts an
emulator; the reviewed Tailnet Portal path owns physical-device configuration.

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
