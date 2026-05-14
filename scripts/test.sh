#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/common.sh"

usage() {
  echo "Usage: ./scripts/test.sh [core|all]"
  exit 1
}

if [[ $# -lt 1 ]]; then
  usage
fi

TARGET="$1"

run_core_tests() {
  echo "Running Rust core tests..."
  (cd "$RUST_DIR" && cargo test -p tenon-core --features mqtt -- --nocapture)
}

case "$TARGET" in
  core)
    run_core_tests
    ;;
  all)
    run_core_tests
    ;;
  *)
    usage
    ;;
esac
