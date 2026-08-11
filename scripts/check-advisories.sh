#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

# Dioxus 0.7.10 -> dioxus-desktop 0.7.10 -> Wry 0.53.5 retains the
# target-specific GTK3 graph below. The exceptions are documented and bounded in
# docs/security/advisory-exceptions.md. Every advisory not named here is denied.
ignored_advisories=(
  RUSTSEC-2024-0411 # gdkwayland-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0412 # gdk: unmaintained GTK3 binding
  RUSTSEC-2024-0413 # atk: unmaintained GTK3 binding
  RUSTSEC-2024-0414 # gdkx11-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0415 # gtk: unmaintained GTK3 binding
  RUSTSEC-2024-0416 # atk-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0418 # gdk-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0419 # gtk3-macros: unmaintained GTK3 binding
  RUSTSEC-2024-0420 # gtk-sys: unmaintained GTK3 binding
  RUSTSEC-2024-0429 # glib: VariantStrIter unsoundness in GTK3 graph
  RUSTSEC-2024-0370 # proc-macro-error: unmaintained GTK3 macro dependency
  RUSTSEC-2024-0436 # paste: unmaintained image-codec build dependency
  RUSTSEC-2025-0057 # fxhash: unmaintained Wry HTML parser dependency
  RUSTSEC-2026-0097 # rand 0.7: constrained build-dependency unsoundness
  RUSTSEC-2025-0141 # bincode: unmaintained Midnight ZK dependency
)

audit_arguments=(--deny warnings)
for advisory in "${ignored_advisories[@]}"; do
  audit_arguments+=(--ignore "$advisory")
done

cargo audit "${audit_arguments[@]}"
