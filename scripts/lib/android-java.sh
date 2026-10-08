#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

# Android Gradle Plugin 8.x is certified with JDK 17 in this repository. Keep
# selection here so release builds, emulator builds, and Maestro evidence do
# not inherit an arbitrary host default.

oxid_android_java_major() {
  local java_home="$1" java_version major
  [ -f "$java_home/release" ] || return 1
  java_version="$(awk -F= '$1 == "JAVA_VERSION" { value=$2; gsub(/^"|"$/, "", value); print value; exit }' "$java_home/release")"
  [ -n "$java_version" ] || return 1
  major="${java_version%%.*}"
  if [ "$major" = "1" ]; then
    java_version="${java_version#1.}"
    major="${java_version%%.*}"
  fi
  case "$major" in
    ''|*[!0-9]*) return 1 ;;
  esac
  printf '%s' "$major"
}

oxid_android_try_java_home() {
  local candidate="$1" major
  [ -n "$candidate" ] && [ -x "$candidate/bin/java" ] || return 1
  major="$(oxid_android_java_major "$candidate")" || return 1
  [ "$major" = "17" ] || return 1
  OXID_ANDROID_RESOLVED_JAVA_HOME="$candidate"
  OXID_ANDROID_JAVA_MAJOR="$major"
  return 0
}

oxid_android_select_java() {
  local explicit_home="${OXID_ANDROID_JAVA_HOME:-}" ambient_home="${JAVA_HOME:-}"
  local command_java="" command_home="" mac_home=""

  if [ -n "$explicit_home" ]; then
    oxid_android_try_java_home "$explicit_home" || {
      echo "OXID_ANDROID_JAVA_HOME must name a JDK 17 home with an executable bin/java." >&2
      return 1
    }
  elif oxid_android_try_java_home "$ambient_home"; then
    :
  elif [ "$(uname -s)" = "Darwin" ] && [ -x /usr/libexec/java_home ]; then
    mac_home="$(/usr/libexec/java_home -v 17 2>/dev/null || true)"
    oxid_android_try_java_home "$mac_home" || {
      echo "Android builds require JDK 17; install it or set OXID_ANDROID_JAVA_HOME." >&2
      return 1
    }
  else
    command_java="$(command -v java 2>/dev/null || true)"
    if [ -n "$command_java" ]; then
      command_home="$($command_java -XshowSettings:properties -version 2>&1 \
        | awk -F= '$1 ~ /^[[:space:]]*java.home[[:space:]]*$/ { value=$2; gsub(/^[[:space:]]+|[[:space:]]+$/, "", value); print value; exit }')"
    fi
    oxid_android_try_java_home "$command_home" || {
      echo "Android builds require JDK 17; enter ./bootstrap.sh or set OXID_ANDROID_JAVA_HOME." >&2
      return 1
    }
  fi

  JAVA_HOME="$OXID_ANDROID_RESOLVED_JAVA_HOME"
  case ":$PATH:" in
    *":$JAVA_HOME/bin:"*) ;;
    *) PATH="$JAVA_HOME/bin:$PATH" ;;
  esac
  export JAVA_HOME PATH OXID_ANDROID_JAVA_MAJOR
}
