# ADR-0022: Nix defines the reproducible development and CI environment

- Status: Accepted
- Date: 2026-08-11
- Source: Repository harness requirements and M0 implementation
- Implementation state: Implemented

## Context

Oxid spans Rust, Dioxus, native WebView libraries, WASM/mobile targets,
dependency policy tools, coverage instrumentation, documentation checks, and
local agent tooling. Ad hoc host installations would make local and CI results
diverge and make public contributions harder to reproduce.

## Decision

The locked Nix flake is the authoritative development and CI environment.
`nix develop` provides the Rust/Dioxus toolchain, native libraries, quality and
coverage tools, Node.js, and project-local pinned Pi packages. CI invokes the
same repository scripts through that environment and builds the locked default
package.

Keep `Cargo.lock`, `flake.lock`, the non-Nix Rust toolchain declaration, and CI
commands aligned. Direnv is an optional entry point, not a second environment.
Private credentials remain inherited user state and are never committed by the
shell.

Project-local Pi configuration pins the external-review package; that package's
own metadata registers its extension and skill together. Apple simulator builds
use the flake's Dioxus CLI while delegating SDK discovery and target compilation
to the host Xcode and Rustup toolchain.

## Consequences

- Local and CI gates share versions and commands.
- Flake updates are reviewable supply-chain changes.
- Contributors need Nix for the supported full environment.
- Platform-specific build and device validation still require the relevant
  host or CI runner; Nix does not make mobile hosts interchangeable.
