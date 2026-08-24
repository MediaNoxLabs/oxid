#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
fail() { printf 'portal-resource-leaks: FAIL phase=%s\n' "$1" >&2; exit 1; }
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: check-portal-resource-leaks.sh <absolute-oxid-root> <absolute-portal-source-tree> [allowed-portal-project]" >&2
  exit 2
fi
repository_root="$1"
source_tree="$2"
allowed_project="${3:-}"
[[ "$repository_root" = /* && "$source_tree" = /* ]] || fail paths
[ -d "$repository_root" ] && [ ! -L "$repository_root" ] || fail oxid-root
[ -d "$source_tree" ] && [ ! -L "$source_tree" ] || fail source-tree
if [ -n "$allowed_project" ] && ! [[ "$allowed_project" =~ ^oxidportal[a-z0-9_-]{0,53}$ ]]; then fail allowed-project; fi
for command_name in docker git rg; do command -v "$command_name" >/dev/null 2>&1 || fail "missing-$command_name"; done

[ ! -e "$repository_root/target/portal-mobile-e2e/ios/runtime" ] || fail ios-runtime
[ ! -e "$repository_root/target/portal-mobile-e2e/android/runtime" ] || fail android-runtime
[ ! -e "/tmp/oxid-portal-mobile-$(id -u).lock" ] || fail mobile-lock
[ ! -e "/tmp/oxid-portal-mobile-$(id -u).lock.reclaim" ] || fail mobile-reclaim

worktrees="$(git -C "$source_tree" worktree list --porcelain 2>/dev/null)" || fail worktree-query
containers="$(DOCKER_CLIENT_TIMEOUT=10 docker ps -a --format '{{.Label "com.docker.compose.project"}}' 2>/dev/null)" || fail container-query
networks="$(DOCKER_CLIENT_TIMEOUT=10 docker network ls --format '{{.Name}}' 2>/dev/null)" || fail network-query
volumes="$(DOCKER_CLIENT_TIMEOUT=10 docker volume ls --format '{{.Name}}' 2>/dev/null)" || fail volume-query
if rg -q '^worktree .*/oxid-portal-(integration|mobile)-' <<<"$worktrees"; then fail portal-worktree; fi
resource_lines="$containers
$networks
$volumes"
if [ -n "$allowed_project" ]; then
  resource_lines="$(printf '%s\n' "$resource_lines" | grep -Ev "^${allowed_project}($|_)" || true)"
fi
if rg -q '^oxidportal124' <<<"$resource_lines"; then fail compose-resource; fi
printf 'portal-resource-leaks: PASS\n'
