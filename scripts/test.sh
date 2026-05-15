#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/common.sh"

usage() {
  echo "Usage: ./scripts/test.sh [core|tenon-oss|all]"
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

build_mqtt_test_client() {
  echo "Building MQTT test client..."
  (cd "$RUST_DIR" && cargo build --release -p tenon-test-mqtt)
}

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command for tenon-oss integration test: $command_name"
    exit 10
  fi
}

wait_for_process_exit() {
  local pid="$1"
  local seconds="$2"
  for _ in $(seq 1 "$seconds"); do
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      wait "$pid" || true
      return 0
    fi
    sleep 1
  done
  return 1
}

metric_value() {
  local metric_name="$1"
  curl -fsS http://127.0.0.1:9100/metrics | \
    awk -v name="$metric_name" '$1 == name { print $2; found=1; exit } END { if (!found) exit 1 }'
}

mqtt_publish() {
  local host="$1"
  local port="$2"
  local client_id="$3"
  local topic="$4"
  local payload="$5"
  "$RUST_DIR/target/release/tenon-test-mqtt" \
    --host "$host" \
    --port "$port" \
    --client-id "$client_id" \
    --topic "$topic" \
    --payload "$payload" \
    --timeout-secs 10
}

run_tenon_oss_integration_test() {
  echo "Running tenon-oss integration test..."
  require_command curl

  if curl -fsS http://127.0.0.1:9100/metrics >/dev/null 2>&1; then
    echo "Port 9100 is already serving /metrics; stop that process before running tenon-oss integration tests."
    exit 11
  fi

  build_tenon_oss_binary
  build_mqtt_test_client

  local work_dir broker_container broker_started mqtt_host mqtt_port binary_path config_path rule_path tenon_home tenon_pid
  work_dir="$(mktemp -d "${TMPDIR:-/tmp}/tenon-oss-itest.XXXXXX")"
  broker_container="tenon-oss-itest-$RANDOM-$$"
  broker_started=0
  mqtt_host="${TENON_TEST_MQTT_HOST:-127.0.0.1}"
  mqtt_port="${TENON_TEST_MQTT_PORT:-}"
  binary_path="$BUILD_DIR/$(oss_binary_name)"
  config_path="$work_dir/config.json"
  rule_path="$work_dir/rule.json"
  tenon_home="$work_dir/home"
  tenon_pid=""

  cleanup() {
    if [[ -n "$tenon_pid" ]] && kill -0 "$tenon_pid" >/dev/null 2>&1; then
      kill -TERM "$tenon_pid" >/dev/null 2>&1 || true
      wait_for_process_exit "$tenon_pid" 10 >/dev/null 2>&1 || kill -KILL "$tenon_pid" >/dev/null 2>&1 || true
    fi
    if [[ "$broker_started" == "1" ]]; then
      docker rm -f "$broker_container" >/dev/null 2>&1 || true
    fi
    rm -rf "$work_dir"
  }
  trap cleanup EXIT

  if [[ -z "$mqtt_port" ]]; then
    require_command docker
    docker run -d \
      --name "$broker_container" \
      -m 1g \
      -e MEM_LIMIT=1073741824 \
      -p 127.0.0.1::1883 \
      apache/bifromq:4.0.0-incubating >/dev/null
    broker_started=1
    mqtt_port="$(docker port "$broker_container" 1883/tcp | sed 's/.*://')"
  fi

  for _ in $(seq 1 60); do
    if mqtt_publish "$mqtt_host" "$mqtt_port" tenon-oss-itest-probe tenon/itest/probe ready >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  if ! mqtt_publish "$mqtt_host" "$mqtt_port" tenon-oss-itest-probe tenon/itest/probe ready >/dev/null 2>&1; then
    echo "MQTT broker did not become ready on $mqtt_host:$mqtt_port"
    if [[ "$broker_started" == "1" ]]; then
      docker logs "$broker_container" || true
    fi
    exit 12
  fi

  cat > "$rule_path" <<EOF
{
  "rules": [
    {
      "expression": "select * from data",
      "destinations": ["itest_noop"]
    }
  ]
}
EOF

  cat > "$config_path" <<EOF
{
  "rule_json_path": "$rule_path",
  "client_ids_path": "$work_dir/client_ids",
  "payload": {
    "format": "json"
  },
  "mqtt": {
    "host": "$mqtt_host",
    "port": $mqtt_port,
    "client_count": 1,
    "group_name": "tenon-oss-itest"
  },
  "sinks": {
    "itest_noop": {
      "type": "noop"
    }
  },
  "metrics": {
    "detailed_latency": false
  }
}
EOF

  TENON_HOME="$tenon_home" "$binary_path" -c "$config_path" &
  tenon_pid="$!"

  for _ in $(seq 1 30); do
    if curl -fsS http://127.0.0.1:9100/health >/dev/null 2>&1; then
      break
    fi
    if ! kill -0 "$tenon_pid" >/dev/null 2>&1; then
      echo "tenon-oss exited before becoming healthy"
      cat "$tenon_home"/log/tenon.log* 2>/dev/null || true
      exit 13
    fi
    sleep 1
  done
  curl -fsS http://127.0.0.1:9100/health >/dev/null

  local delivered
  delivered=0
  for i in $(seq 1 30); do
    mqtt_publish "$mqtt_host" "$mqtt_port" "tenon-oss-itest-pub-$i" data '{"temp":30,"hum":10}' >/dev/null
    sleep 1
    delivered="$(metric_value tenon_oss_noop_sink_messages_total || echo 0)"
    if awk "BEGIN { exit !($delivered >= 1) }"; then
      break
    fi
  done

  if ! awk "BEGIN { exit !($delivered >= 1) }"; then
    echo "tenon-oss did not deliver test MQTT message to noop sink"
    echo "==== metrics ===="
    curl -fsS http://127.0.0.1:9100/metrics || true
    echo "==== logs ===="
    cat "$tenon_home"/log/tenon.log* 2>/dev/null || true
    exit 14
  fi

  kill -TERM "$tenon_pid"
  wait_for_process_exit "$tenon_pid" 20 || {
    echo "tenon-oss did not stop after SIGTERM"
    cat "$tenon_home"/log/tenon.log* 2>/dev/null || true
    exit 15
  }
  tenon_pid=""
}

case "$TARGET" in
  core)
    run_core_tests
    ;;
  tenon-oss)
    run_tenon_oss_integration_test
    ;;
  all)
    run_core_tests
    run_tenon_oss_integration_test
    ;;
  *)
    usage
    ;;
esac
