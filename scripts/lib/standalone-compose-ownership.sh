#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Validates the exact three-service Compose identity before a launcher accepts
# or mutates an existing standalone project. Diagnostics intentionally omit
# checkout paths and container identifiers.
oxid_validate_standalone_compose_ownership() {
  local expected_compose="$1"
  local expected_working_directory="$2"
  local container_ids="$3"
  local id service config_files working_directory project
  local services=""

  while IFS= read -r id; do
    [ -n "$id" ] || continue
    service="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.service" }}' "$id" 2>/dev/null)" || return 1
    config_files="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.project.config_files" }}' "$id" 2>/dev/null)" || return 1
    working_directory="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.project.working_dir" }}' "$id" 2>/dev/null)" || return 1
    project="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.project" }}' "$id" 2>/dev/null)" || return 1
    [ "$project" = "oxid-standalone" ] || return 1
    [ "$config_files" = "$expected_compose" ] || return 1
    [ "$working_directory" = "$expected_working_directory" ] || return 1
    case "$service" in
      indexer|node|proof-server) ;;
      *) return 1 ;;
    esac
    services="${services}${service}
"
  done < <(jq -r '.[]' <<<"$container_ids")

  [ "$(printf '%s' "$services" | sort)" = $'indexer\nnode\nproof-server' ]
}
