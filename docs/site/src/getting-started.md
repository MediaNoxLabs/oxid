# Getting started

Everything reproducible in Oxid goes through [Nix](https://nixos.org) and the
[`just`](https://github.com/casey/just) task runner. The devshell pins the
Rust toolchain, the Compact compiler, and the zero-knowledge artifact
closures, so a first entry downloads several gigabytes — after that it is
warm.

## Prerequisites

- Nix with flakes enabled (the [Determinate installer](https://install.determinate.systems)
  is what CI uses).
- macOS or Linux. iOS simulator targets need a Mac with Xcode; Android
  targets need an emulator image.

## Build and verify

```bash
git clone https://github.com/MediaNoxLabs/oxid
cd oxid
nix develop            # enters the pinned toolchain shell
just check             # the light strict gate: fmt, architecture, clippy, tests, coverage
```

`just check` is the same command CI runs on every push. Useful targets:

| Command | What it does |
| --- | --- |
| `just check` | Light strict gate (fmt, architecture, sources, clippy, tests, coverage) |
| `just full` | The light gate plus advisories, licenses, and rustdoc |
| `just test` | `cargo test --workspace` only |
| `just lint` | Clippy with warnings denied |
| `just architecture` | The dependency-rules gate on its own |
| `just run` | Launch the Dioxus desktop shell |
| `just headless` | Launch the NDJSON headless adapter |
| `just ios-run` / `just android-run` | Standalone development app in a simulator/emulator |
| `just ios-dev` / `just android-dev` | Same standalone composition with the persistent developer capability profile |
| `just ios-dev-smoke` / `just android-dev-smoke` | Fresh-install developer banner and shared capability-manifest checks |
| `just ios-demo` / `just android-demo` | Compile-time standalone demo profile with the fixture bootstrap drawer |
| `just ios-demo-smoke` / `just android-demo-smoke` | Fresh-install safe setup and unchanged credential-review boundary |
| `just ios-smoke` / `just android-smoke` | The scripted mobile end-to-end flows |
| `just standalone-phone-up` / `just standalone-down` | Start/stop the loopback stack and Oxid-owned tailnet TLS routes |
| `just android-phone` | Build and launch the compile-time standalone tailnet profile on one physical Android device |
| `just nix-check` | Every hermetic flake check (slow, sandboxed) |

Run the gate from inside `nix develop` — coverage needs `cargo-llvm-cov`
from the devshell, and the gate fails fast with a clear message if a tool is
missing.

## Two composition modes — read this before judging a blank screen

A plain `just run` build calls the **production composition**, which
deliberately fails closed: until native custody and live transport pass
review, it wires unavailable adapters and the UI says so. This is a feature,
not a bug — see the [security model](security-model.md).

The **standalone development composition** (`just ios-run`,
`just android-run`, or `oxid-headless` with development environment
variables) enables process-local development custody, deterministic
simulations, and the real standalone SSI flows. That is where the demo
lives; [delivery status](status.md) maps each capability to its mode.

`just ios-dev` and `just android-dev` set only `OXID_UI_PROFILE=dev`. They keep
the same standalone composition while adding the shared public capability
viewer and a non-dismissible developer banner; normal release artifacts exclude
this presentation profile.

`just ios-demo` and `just android-demo` select the separate compile-time demo
profile. Its drawer sequences only existing standalone use cases. The funding
step admits only the exact undeployed simulator, and offer/login/presentation
fixtures stop at their unchanged review screens. The drawer never supplies
consent or claims production readiness; normal and native-custody artifacts
cannot select it.

To exercise the authenticated Compact presentation worker in the explicit
native-custody conformance build, use:

```bash
OXID_MOBILE_CUSTODY=native OXID_MOBILE_PRESENTATION_PROVING=artifacts just ios-run
OXID_MOBILE_CUSTODY=native OXID_MOBILE_PRESENTATION_PROVING=artifacts just android-run
```

This large build is simulator/emulator conformance evidence, not physical-device
or production readiness. The ordinary commands remain proof-disabled.

## Physical Android against the laptop standalone stack

Connect the laptop and phone to the same tailnet, authorize USB debugging, and
ensure no Android emulator or iOS simulator is running. Then use:

```bash
just standalone-phone-up
just android-phone
```

The stack stays on laptop loopback. The up command creates temporary
TLS-terminated Tailscale Serve routes and the phone command embeds their current
MagicDNS URLs only in the explicit `standalone-tailnet` development build. No
personal IP, local password, or endpoint is committed. The profile is
incompatible with native custody and is excluded from normal release artifacts.
Stop the owned containers and Serve routes with `just standalone-down`.

Physical identity-ingress evidence is intentionally interactive and never
clears application data. Generate the deterministic public offer with
`just android-phone-ingress show-offer-qr`, then run `prepare-scan` followed by
the expected `assert-qr-offer`, `assert-cancelled`, `assert-timeout`, or
`assert-unavailable` mode. Use `link-warm` and `link-cold` for custom-scheme
delivery. The harness refuses emulators and a concurrently booted iOS
simulator. Android Google Code Scanner owns camera access and declares no app
camera permission, so Android permission denial is not a supported outcome.

## Driving the wallet without a UI

```bash
cargo run -p oxid-headless
```

speaks a versioned NDJSON protocol on stdin/stdout — one JSON request per
line, one response per line. Start with the capability discovery call and
explore from there; the [headless protocol](headless-protocol.md) chapter
covers the envelope, namespaces, and secret-hygiene guarantees.
