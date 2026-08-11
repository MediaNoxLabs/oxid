#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

target="all"
light=false
strict=false

usage() {
  cat <<'USAGE'
Usage: ./run.sh [all|core|ui|coverage|quality|clean|targets] [--light] [--strict]

  --light   Skip advisory, license, and rustdoc checks.
  --strict  Deny compiler and rustdoc warnings.
USAGE
}

while (($# > 0)); do
  case "$1" in
    all|core|ui|coverage|quality|clean|targets)
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
  cargo fmt --all --check
  ./scripts/check-architecture.sh
  ./scripts/check-midnight-sources.sh
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
}

run_ui() {
  cargo check -p oxid-ui-dioxus
  cargo check -p oxid-app
}

run_coverage() {
  require_command cargo-llvm-cov
  cargo llvm-cov \
    --workspace \
    --exclude oxid-ui-dioxus \
    --exclude oxid-app \
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
    run_core
    run_ui
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
    printf '%s\n' all core ui coverage quality clean targets
    ;;
esac
