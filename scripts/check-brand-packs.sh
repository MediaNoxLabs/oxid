#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo run --quiet -p oxid-brand-build --bin oxid-brand-check -- \
  "$repo_root/brands"
