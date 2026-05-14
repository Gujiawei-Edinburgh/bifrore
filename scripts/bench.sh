#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/common.sh"

usage() {
  echo "Usage: ./scripts/bench.sh [rust [filter]]"
  exit 1
}

MODE="${1:-rust}"
shift || true

run_rust_bench() {
  echo "Running Rust benchmarks..."
  if [[ $# -gt 0 ]]; then
    (cd "$RUST_DIR" && cargo bench -p tenon-core -- "$1")
  else
    (cd "$RUST_DIR" && cargo bench -p tenon-core)
  fi
}

case "$MODE" in
  rust)
    run_rust_bench "$@"
    ;;
  *)
    usage
    ;;
esac
