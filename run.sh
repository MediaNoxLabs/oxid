#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

target="all"
light=false
strict=false

usage() {
  cat <<'USAGE'
Usage: ./run.sh [all|core|ui|headless|coverage|quality|clean|targets] [--light] [--strict]

  --light   Skip advisory, license, and rustdoc checks.
  --strict  Deny compiler and rustdoc warnings.
USAGE
}

while (($# > 0)); do
  case "$1" in
    all|core|ui|headless|coverage|quality|clean|targets)
      target="$1"
      ;;
    --light)
      light=true
      ;;
    --strict)
      strict=true
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

run_core() {
  local run_workspace_tests=true
  if [[ "${1:-}" == "--skip-workspace-tests" ]]; then
    run_workspace_tests=false
  fi
  cargo fmt --all --check
  ./scripts/check-architecture.sh
  ./scripts/check-arrayref-source.sh
  ./scripts/check-midnight-sources.sh
  cargo clippy --workspace --all-targets -- -D warnings
  if $run_workspace_tests; then
    cargo test --workspace
  fi
}

run_coverage_excluded_tests() {
  # The coverage measurement excludes these crates, so the `all` target runs
  # their tests directly; every other crate's tests execute exactly once under
  # cargo-llvm-cov instrumentation instead of once plain and once instrumented.
  cargo test -p oxid-ui-dioxus -p oxid-app -p oxid-headless
}

run_ui() {
  ./scripts/check-brand-packs.sh
  ./scripts/check-ui-css-classes.sh
  ./scripts/check-ui-design-tokens.sh
  ./scripts/check-ui-copy-labels.sh
  ./scripts/check-ui-profile-release.sh
  ./scripts/check-portal-mobile-harness.sh
  cargo check -p oxid-ui-dioxus
  # Adapter-only profile builds type-check the profile code itself, so they
  # state app-profile-authority deliberately. An application build must reach
  # the same code through oxid-app's guarded features instead.
  cargo check -p oxid-ui-dioxus --features ui-profile-dev,app-profile-authority
  cargo check -p oxid-ui-dioxus --features ui-profile-demo,app-profile-authority
  cargo test -p oxid-ui-dioxus --features ui-profile-demo,app-profile-authority
  cargo check -p oxid-app
  cargo check -p oxid-app --no-default-features --features mobile
  cargo check -p oxid-app --no-default-features --features mobile,standalone-development
  cargo check -p oxid-app --no-default-features --features mobile,standalone-development,ui-profile-dev
  cargo check -p oxid-app --no-default-features --features mobile,standalone-development,ui-profile-demo
}

run_headless() {
  ./scripts/check-portal-headless-evidence.sh
  ./scripts/check-portal-local-conformance.sh
  cargo check -p oxid-headless
}

run_coverage() {
  require_command cargo-llvm-cov
  cargo llvm-cov \
    --workspace \
    --exclude oxid-ui-dioxus \
    --exclude oxid-app \
    --exclude oxid-headless \
    --summary-only \
    --fail-under-lines 80
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Required command '$1' is missing; run this target from 'nix develop'." >&2
    exit 1
  fi
}

run_quality() {
  ./scripts/check-adr-links.sh
  require_command cargo-audit
  require_command cargo-deny
  ./scripts/check-advisories.sh
  cargo deny check bans licenses sources
  if $strict; then
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  else
    cargo doc --workspace --no-deps
  fi
}

case "$target" in
  all)
    run_core --skip-workspace-tests
    run_ui
    run_headless
    run_coverage_excluded_tests
    run_coverage
    if ! $light; then
      run_quality
    fi
    ;;
  core)
    run_core
    ;;
  ui)
    run_ui
    ;;
  headless)
    run_headless
    ;;
  coverage)
    run_coverage
    ;;
  quality)
    run_quality
    ;;
  clean)
    cargo clean
    ;;
  targets)
    printf '%s\n' all core ui headless coverage quality clean targets
    ;;
esac
