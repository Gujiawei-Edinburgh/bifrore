#!/bin/bash

# fork from BifroMQ

if [ $# -lt 1 ];
then
  echo "USAGE: $0 service_name"
  exit 1
fi

NAME=$1
BASE_DIR=$(cd "$(dirname "$0")" && pwd)/..
PID_FILE="$BASE_DIR/bin/pid"
SIGNAL=${SIGNAL:-TERM}

if [ ! -f "$PID_FILE" ]; then
  echo "No PID file found for $NAME. Process might not be running."
  exit 1
fi

PIDS=$(cat "$PID_FILE")

if [ -z "$PIDS" ] || ! ps -p "$PIDS" > /dev/null 2>&1; then
  echo "No running process found for $NAME with PID $PIDS."
  rm -f "$PID_FILE"
  exit 1
fi

echo "Find $NAME process $PIDS, and stopping it..."
kill -s $SIGNAL "$PIDS"

ret=1
for i in {1..300}; do
    if ! ps -p "$PIDS" > /dev/null 2>&1; then
        echo "$NAME process $PIDS was stopped."
        ret=0
        break
    fi
    sleep 1
done

if [ $ret -eq 1 ]; then
    echo "Timeout: $NAME process $PIDS did not stop within 300 seconds!"
else
    rm -f "$PID_FILE"
fi

exit $ret