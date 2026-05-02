#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/scripts/common.sh"

usage() {
  echo "Usage: ./build.sh [java|python|oss|all]"
  exit 1
}

if [[ $# -lt 1 ]]; then
  usage
fi

TARGET="$1"

case "$TARGET" in
  java)
    build_rust
    build_jni
    build_java_jar
    ;;
  python)
    build_rust
    build_python
    ;;
  oss)
    build_oss_binary
    ;;
  all)
    build_rust
    build_oss_binary
    build_jni
    build_java_jar
    build_python
    ;;
  *)
    usage
    ;;
esac

echo "Build artifacts are in $BUILD_DIR"
