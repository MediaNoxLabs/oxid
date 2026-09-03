#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
exec "$repository_root/scripts/demo-stack.sh" start
