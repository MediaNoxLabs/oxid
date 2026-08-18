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
| `just ios-smoke` / `just android-smoke` | The scripted mobile end-to-end flows |
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

To exercise the authenticated Compact presentation worker in the explicit
native-custody conformance build, use:

```bash
OXID_MOBILE_CUSTODY=native OXID_MOBILE_PRESENTATION_PROVING=artifacts just ios-run
OXID_MOBILE_CUSTODY=native OXID_MOBILE_PRESENTATION_PROVING=artifacts just android-run
```

This large build is simulator/emulator conformance evidence, not physical-device
or production readiness. The ordinary commands remain proof-disabled.

## Driving the wallet without a UI

```bash
cargo run -p oxid-headless
```

speaks a versioned NDJSON protocol on stdin/stdout — one JSON request per
line, one response per line. Start with the capability discovery call and
explore from there; the [headless protocol](headless-protocol.md) chapter
covers the envelope, namespaces, and secret-hygiene guarantees.
