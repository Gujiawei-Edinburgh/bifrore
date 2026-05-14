#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/scripts/common.sh"

usage() {
  echo "Usage: ./build.sh [tenon-oss|tenon-oss-pypi|all]"
  exit 1
}

if [[ $# -lt 1 ]]; then
  usage
fi

TARGET="$1"

case "$TARGET" in
  tenon-oss|oss)
    build_tenon_oss_binary
    ;;
  tenon-oss-pypi)
    build_tenon_oss_pypi
    ;;
  all)
    build_tenon_oss_pypi
    ;;
  *)
    usage
    ;;
esac

echo "Build artifacts are in $BUILD_DIR"
